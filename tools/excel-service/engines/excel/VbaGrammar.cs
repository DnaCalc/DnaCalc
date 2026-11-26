using System.Text;
using System.Text.RegularExpressions;

namespace Touchstone.Excel;

/// <summary>
/// The positive grammar for Touchstone VBA probe source (dc-gqzuw). Same algorithm as
/// tools/excel-service/vba_grammar.py, over the same rule table (vba-probe-policy.json); keep the two in step.
/// Only the listed statement forms, expressions and member chains are accepted. A cell value written through
/// .Value or .Value2 must provably be data (a number, a boolean, a string that does not start with = + - or @).
/// </summary>
static class VbaGrammar
{
    static readonly Dictionary<string, object> Rules = VbaPolicy.RuleTable;
    static readonly Regex Lex = new(@"\G(?:" + ((string)Rules["lex"]).Replace("(?P<", "(?<") + ")", RegexOptions.CultureInvariant);
    static readonly string[] LexGroups = { "ws", "comment", "str", "hex", "num", "id", "cont", "op" };
    static readonly Regex Identifier = new(@"\A" + Rules["identifier"] + @"\z", RegexOptions.CultureInvariant);
    static readonly Regex ModuleName = new(@"\A" + Rules["module_name"] + @"\z", RegexOptions.CultureInvariant);
    static readonly Regex SheetDocumentName = new(@"\ASheet[1-9][0-9]{0,1}\z", RegexOptions.CultureInvariant);
    static readonly Regex SheetCodeName = new(@"\Asheet[1-9][0-9]{0,1}\z", RegexOptions.CultureInvariant);
    static HashSet<string> Set(string key) => ((string)Rules[key]).Split(' ').ToHashSet();
    static readonly HashSet<string> Keywords = Set("keywords"), ValueTypes = Set("value_types"), NumericTypes = Set("numeric_types"),
        Events = Set("event_names"), Constants = Set("constants");
    static readonly Dictionary<string, string> ObjectTypes = new() { ["range"] = "range", ["worksheet"] = "sheet", ["workbook"] = "wb", ["object"] = "pending" };
    static readonly Dictionary<string, (int low, int high, string safety)> Builtins = ((List<object>)Rules["builtins"])
        .Select(r => ((string)r).Split(' ')).ToDictionary(p => p[0], p => (int.Parse(p[1]), int.Parse(p[2]), p[3]));
    static readonly Dictionary<string, Dictionary<string, (string ret, int low, int high, string flags)>> Members = ReadMembers();
    static readonly HashSet<string> Reserved = Set("reserved").Concat(Keywords).Concat(ValueTypes).Concat(ObjectTypes.Keys)
        .Concat(Constants).Concat(Builtins.Keys).ToHashSet();
    static readonly Dictionary<string, string> Classes = ((List<object>)Rules["classes"]).Cast<string>()
        .Zip(new[] { "dict", "regexp" }, (c, k) => (c, k)).ToDictionary(p => p.c.ToLowerInvariant(), p => p.k);
    static readonly HashSet<string> ObjectKinds = new() { "wb", "sheet", "range", "dict", "regexp", "err" };
    static readonly string FormulaStart = (string)Rules["formula_chars"];
    static readonly HashSet<string> BinarySymbols = new() { "+", "-", "*", "/", "\\", "^", "&", "=", "<", ">", "<=", ">=", "<>" };
    static readonly HashSet<string> BinaryWords = new() { "mod", "and", "or", "xor", "eqv", "imp", "is", "like" };
    static readonly HashSet<string> BlockWords = new()
        { "if", "else", "elseif", "end", "for", "next", "do", "loop", "while", "wend", "select", "case", "dim", "const" };

    static Dictionary<string, Dictionary<string, (string, int, int, string)>> ReadMembers()
    {
        var table = new Dictionary<string, Dictionary<string, (string, int, int, string)>>();
        foreach (string row in (List<object>)Rules["members"])
        {
            var p = row.Split(' ');
            if (!table.TryGetValue(p[0], out var owner)) table[p[0]] = owner = new();
            owner[p[1]] = (p[2], int.Parse(p[3]), int.Parse(p[4]), p[5]);
        }
        return table;
    }

    public static string Escape(string text)
    {
        var sb = new StringBuilder();
        foreach (char c in text)
            if (c < 0x80) sb.Append(c);
            else if (c < 0x100) sb.Append("\\x").Append(((int)c).ToString("x2"));
            else sb.Append("\\u").Append(((int)c).ToString("x4"));
        return sb.ToString();
    }

    static RefusedVbaException Refuse(string what, string line)
    {
        var shown = Escape(line.Length <= 100 ? line : line[..100] + "...");
        return new RefusedVbaException($"VBA construct refused: {Escape(what)} in: {shown}");
    }

    sealed class Token
    {
        public readonly string Kind, Text, Low;
        public Token(string kind, string text) { Kind = kind; Text = text; Low = text.ToLowerInvariant(); }
    }

    sealed class Node
    {
        public readonly string Tag, Key;
        public readonly List<Node> Items;
        public readonly List<string> Ops;
        public Node(string tag, string key = null, List<Node> items = null, List<string> ops = null)
        { Tag = tag; Key = key; Items = items; Ops = ops; }
    }

    // Taint classes of a value (dc-5qqoz): Text is a string that cannot start with = + - or @; Number is a number or
    // boolean (it may be negative, so its text may start with -); NumText is a string made from a number; Bad is
    // anything else. A cell write takes Text or Number; a concatenation's leading operand must be Text.
    // rw17 (dc-erfm3): SafeText is a string that is also never empty (a non-empty literal, TypeName), the only kind that
    // may lead a concatenation; Text may be empty at run time ("", vbNullString, Empty, Null, Left(s, 0), an unassigned
    // variable), so a number after it could lead the result.
    const int SafeText = 0, Text = 1, Number = 2, NumText = 3, Bad = 4;
    static readonly Node Safe = new("safe"), Maybe = new("maybe"), Num = new("num"), Unsafe = new("unsafe");

    sealed class Last
    {
        public string Type, Member, Owner;
        public Var Var;
    }

    static readonly Last CallLast = new() { Type = "call" };

    static List<(string text, List<Token> tokens)> LogicalLines(string source)
    {
        if (source.Replace("\r\n", "").Contains('\r'))
            throw new RefusedVbaException("VBA construct refused: a bare carriage return");
        var lines = new List<(string, List<Token>)>();
        var tokens = new List<Token>();
        var text = "";
        bool pending = false;
        foreach (var physical in source.Replace("\r\n", "\n").Split('\n'))
        {
            int at = 0;
            bool afterSpace = false;
            pending = false;
            while (at < physical.Length)
            {
                var match = Lex.Match(physical, at);
                if (!match.Success) throw Refuse(physical[at].ToString(), physical);
                at = match.Index + match.Length;
                string kind = LexGroups.First(g => match.Groups[g].Success);
                if (kind == "cont")
                {
                    if (!afterSpace) throw Refuse("_", physical);
                    pending = true;
                    break;
                }
                afterSpace = kind == "ws";
                if (kind is not ("ws" or "comment")) tokens.Add(new Token(kind, match.Value));
            }
            text += physical + " ";
            if (!pending)
            {
                if (tokens.Count > 0) lines.Add((text.Trim(), tokens));
                tokens = new();
                text = "";
            }
        }
        if (pending) throw Refuse("_", text);
        return lines;
    }

    sealed class Var
    {
        public string Key, Kind;
        public bool Numeric, Param, Array, Const;
        public Var(string key, string kind = "val", bool numeric = false, bool param = false, bool array = false, bool isConst = false)
        { Key = key; Kind = kind; Numeric = numeric; Param = param; Array = array; Const = isConst; }
    }

    sealed class Proc
    {
        public string Module, Name, Key, Line;
        public bool IsFunction, Public;
        public List<(string name, string type)> Params;
        public List<(string text, List<Token> tokens)> Body = new();
    }

    sealed class Case
    {
        public readonly List<(string name, string kind, string source)> Modules;
        public readonly HashSet<string> Names;
        public readonly Dictionary<string, string> Kinds = new();
        public readonly Dictionary<string, Proc> Procs = new();
        public readonly Dictionary<string, Dictionary<string, Var>> ModuleVars = new();
        public readonly List<(string key, Node node)> Flows = new();
        public readonly List<(Node node, string line)> Writes = new();
        public readonly HashSet<string> Numeric = new();
        public readonly HashSet<string> Params = new();
        public readonly HashSet<string> Strings = new();     // declared As String: a number assigned becomes number text
        public HashSet<string> AllProcedures = new();

        public Case(List<(string, string, string)> modules)
        {
            Modules = modules;
            Names = modules.Select(m => m.Item1.ToLowerInvariant()).ToHashSet();
        }

        public string Declared(Token token, string line)
        {
            if (token.Kind != "id" || !Identifier.IsMatch(token.Text) || Reserved.Contains(token.Low) || Names.Contains(token.Low))
                throw Refuse(token.Text, line);
            return token.Low;
        }

        public HashSet<string> Run()
        {
            foreach (var (name, kind, source) in Modules) Scan(name.ToLowerInvariant(), kind, source);
            foreach (var proc in Procs.Values.ToList()) new Body(this, proc).Run();
            var classes = new Dictionary<string, int>();
            foreach (var key in Params.Concat(Numeric)) classes[key] = Number;
            bool changed = true;
            while (changed)
            {
                changed = false;
                foreach (var (key, node) in Flows)
                {
                    int old = classes.GetValueOrDefault(key, Text);
                    int next = Math.Max(old, Classify(node, classes));
                    if (Numeric.Contains(key)) next = Number;
                    else if (Strings.Contains(key) && next == Number) next = NumText;
                    if (next != old)
                    {
                        classes[key] = next;
                        changed = true;
                    }
                }
            }
            foreach (var (node, line) in Writes)
                if (Classify(node, classes) > Number) throw Refuse("a cell value that is not plainly data", line);
            AllProcedures = Procs.Values.Where(p => Kinds[p.Module] == "standard").Select(p => p.Key).ToHashSet();
            return Procs.Values.Where(p => p.Public && Kinds[p.Module] == "standard").Select(p => p.Key).ToHashSet();
        }

        public int Classify(Node node, Dictionary<string, int> classes)
        {
            switch (node.Tag)
            {
                case "safe": return SafeText;
                case "maybe": return Text;
                case "num": return Number;
                case "unsafe": return Bad;
                case "var": return classes.GetValueOrDefault(node.Key, Text);
            }
            var found = node.Items.Select(n => Classify(n, classes)).ToList();
            if (found.Contains(Bad)) return Bad;
            if (node.Tag == "join") return found.Count == 0 ? Text : found.Max();   // Array(...): elements written one by one
            if (node.Tag == "str")                                                   // Left, CStr, LCase, UCase
            {
                if (found[0] == Number) return NumText;
                if (found[0] > Number) return found[0];
                return node.Key == "left" ? Text : found[0];
            }
            var ops = node.Ops;                                                      // "cat": operands and the operators between
            if (ops.Contains("+"))                  // + adds a number, a boolean or a numeric string: only numbers
                return !ops.Contains("&") && found.All(c => c == Number) ? Number : Bad;
            if (ops.Any(op => op != "&")) return ops.Contains("&") ? Bad : Number;
            if (found[0] > SafeText && found.Any(c => c > Text)) return Bad;
            if (found[0] == SafeText) return SafeText;
            return Text;
        }

        void Scan(string module, string kind, string source)
        {
            Kinds[module] = kind;
            ModuleVars[module] = new();
            Proc current = null;
            bool explicitSeen = false;
            foreach (var (text, tokens) in LogicalLines(source))
            {
                var words = tokens.Select(t => t.Low).ToList();
                if (current != null)
                {
                    if (words[0] == "end" && words.Count == 2 && words[1] is "sub" or "function")
                    {
                        if (words[1] != (current.IsFunction ? "function" : "sub")) throw Refuse(text, text);
                        current = null;
                    }
                    else current.Body.Add((text, tokens));
                    continue;
                }
                if (words.Count == 2 && words[0] == "option" && words[1] == "explicit" && !explicitSeen)
                {
                    explicitSeen = true;
                    continue;
                }
                var header = Header(module, kind, tokens, text);
                if (header != null)
                {
                    if (Procs.ContainsKey(header.Key)) throw Refuse(header.Name, text);
                    Procs[header.Key] = current = header;
                    continue;
                }
                ModuleDeclaration(module, tokens, text);
            }
            if (current != null) throw Refuse("a procedure without End", current.Line);
        }

        Proc Header(string module, string kind, List<Token> tokens, string text)
        {
            int at = 0;
            bool isPublic = true;
            if (tokens[0].Low is "public" or "private") { isPublic = tokens[0].Low == "public"; at = 1; }
            if (at >= tokens.Count || tokens[at].Low is not ("sub" or "function")) return null;
            bool isFunction = tokens[at].Low == "function";
            if (at + 1 >= tokens.Count) throw Refuse(tokens[at].Text, text);
            var name = Declared(tokens[at + 1], text);
            if (kind == "document")
            {
                if (!Events.Contains(name) || !name.StartsWith(module == "thisworkbook" ? "workbook_" : "worksheet_", StringComparison.Ordinal))
                    throw Refuse(tokens[at + 1].Text, text);
            }
            else if (name.StartsWith("workbook_", StringComparison.Ordinal) || name.StartsWith("worksheet_", StringComparison.Ordinal))
                throw Refuse(tokens[at + 1].Text, text);
            int pos = at + 2;
            var parameters = new List<(string, string)>();
            if (pos < tokens.Count && tokens[pos].Text == "(")
            {
                pos++;
                if (pos < tokens.Count && tokens[pos].Text == ")") pos++;
                else
                    while (true)
                    {
                        if (pos < tokens.Count && tokens[pos].Low is "byval" or "byref") pos++;
                        if (pos >= tokens.Count) throw Refuse(text, text);
                        var pname = Declared(tokens[pos], text);
                        pos++;
                        var ptype = "variant";
                        if (pos + 1 < tokens.Count && tokens[pos].Low == "as")
                        {
                            ptype = tokens[pos + 1].Low;
                            if (!ValueTypes.Contains(ptype) && !ObjectTypes.ContainsKey(ptype)) throw Refuse(tokens[pos + 1].Text, text);
                            pos += 2;
                        }
                        if (parameters.Any(p => p.Item1 == pname)) throw Refuse(pname, text);
                        parameters.Add((pname, ptype));
                        if (pos < tokens.Count && tokens[pos].Text == ",") pos++;
                        else if (pos < tokens.Count && tokens[pos].Text == ")") { pos++; break; }
                        else throw Refuse(tokens[Math.Min(pos, tokens.Count - 1)].Text, text);
                    }
            }
            if (pos < tokens.Count && tokens[pos].Low == "as")
            {
                if (!isFunction || pos + 1 >= tokens.Count || !ValueTypes.Contains(tokens[pos + 1].Low)) throw Refuse(tokens[pos].Text, text);
                if (NumericTypes.Contains(tokens[pos + 1].Low)) Numeric.Add($"{module}.{name}.{name}");
                pos += 2;
            }
            if (pos != tokens.Count) throw Refuse(tokens[pos].Text, text);
            return new Proc { Module = module, Name = name, Key = $"{module}.{name}", IsFunction = isFunction, Public = isPublic, Params = parameters, Line = text };
        }

        void ModuleDeclaration(string module, List<Token> tokens, string text)
        {
            if (tokens[0].Low is not ("private" or "public" or "dim")) throw Refuse(tokens[0].Text, text);
            var rest = tokens.Skip(1).ToList();
            if (rest.Count > 0 && rest[0].Low == "const") Constant(rest.Skip(1).ToList(), text, ModuleVars[module], $"{module}..");
            else Variables(rest, text, ModuleVars[module], $"{module}..", false);
        }

        public void Variables(List<Token> tokens, string text, Dictionary<string, Var> scope, string prefix, bool allowObjects = true)
        {
            int pos = 0;
            while (true)
            {
                if (pos >= tokens.Count) throw Refuse(text, text);
                var name = Declared(tokens[pos], text);
                if (scope.ContainsKey(name)) throw Refuse(tokens[pos].Text, text);
                pos++;
                bool array = false;
                if (pos < tokens.Count && tokens[pos].Text == "(")
                {
                    array = true;
                    pos++;
                    while (pos < tokens.Count && tokens[pos].Text != ")")
                    {
                        var token = tokens[pos];
                        if (!(token.Kind == "num" && token.Text.All(char.IsAsciiDigit) || token.Low == "to" || token.Text == ","))
                            throw Refuse(token.Text, text);
                        pos++;
                    }
                    if (pos >= tokens.Count) throw Refuse(text, text);
                    pos++;
                }
                var vtype = "variant";
                if (pos + 1 < tokens.Count && tokens[pos].Low == "as")
                {
                    vtype = tokens[pos + 1].Low;
                    if (!ValueTypes.Contains(vtype) && !(allowObjects && !array && ObjectTypes.ContainsKey(vtype)))
                        throw Refuse(tokens[pos + 1].Text, text);
                    pos += 2;
                }
                scope[name] = new Var(prefix + name, ObjectTypes.GetValueOrDefault(vtype, "val"), NumericTypes.Contains(vtype), array: array);
                if (NumericTypes.Contains(vtype)) Numeric.Add(prefix + name);
                if (vtype == "string") Strings.Add(prefix + name);
                if (pos == tokens.Count) return;
                if (tokens[pos].Text != ",") throw Refuse(tokens[pos].Text, text);
                pos++;
            }
        }

        public void Constant(List<Token> tokens, string text, Dictionary<string, Var> scope, string prefix)
        {
            if (tokens.Count != 3 || tokens[1].Text != "=" || tokens[2].Kind is not ("num" or "str" or "hex")) throw Refuse(text, text);
            var name = Declared(tokens[0], text);
            if (scope.ContainsKey(name)) throw Refuse(tokens[0].Text, text);
            scope[name] = new Var(prefix + name, isConst: true);
            Flows.Add((prefix + name, Literal(tokens[2])));
        }

        public static Node Literal(Token token)
        {
            if (token.Kind == "str")
            {
                var body = token.Text[1..^1].Replace("\"\"", "\"");
                if (body.Length == 0) return Maybe;
                return FormulaStart.Contains(body[0]) ? Unsafe : Safe;
            }
            return Num;      // a number or a hex literal (&H8000 is -32768)
        }
    }

    sealed class Body
    {
        public readonly Case Case;
        public readonly Proc Proc;
        public readonly string Module, Prefix;
        public readonly Dictionary<string, Var> Scope = new();
        public readonly List<string> Stack = new();
        public readonly HashSet<string> Labels = new();
        public readonly List<(string label, string text)> Jumps = new();

        public Body(Case c, Proc proc)
        {
            Case = c; Proc = proc; Module = proc.Module; Prefix = proc.Key + ".";
            foreach (var (pname, ptype) in proc.Params)
            {
                var kind = ObjectTypes.GetValueOrDefault(ptype, "val");
                Scope[pname] = new Var(Prefix + pname, kind == "pending" ? "val" : kind, NumericTypes.Contains(ptype), param: true);
                if (NumericTypes.Contains(ptype)) c.Numeric.Add(Prefix + pname);
                c.Params.Add(Prefix + pname);
            }
            if (proc.IsFunction) Scope[proc.Name] = new Var(Prefix + proc.Name);
        }

        public void Run()
        {
            foreach (var (text, tokens) in Proc.Body) Logical(text, tokens);
            if (Stack.Count > 0) throw Refuse("an unclosed block", Proc.Line);
            foreach (var (label, text) in Jumps)
                if (!Labels.Contains(label)) throw Refuse(label, text);
        }

        void Logical(string text, List<Token> tokens)
        {
            if (tokens.Count >= 2 && tokens[1].Text == ":" && (
                    tokens[0].Kind == "num" && tokens[0].Text.All(char.IsAsciiDigit)
                    || tokens[0].Kind == "id" && !Reserved.Contains(tokens[0].Low)))
            {
                if (!Labels.Add(tokens[0].Low)) throw Refuse(tokens[0].Text, text);
                tokens = tokens.Skip(2).ToList();
            }
            var statement = new List<Token>();
            foreach (var token in tokens.Append(new Token("op", ":")))
            {
                if (token.Kind == "op" && token.Text == ":")
                {
                    if (statement.Count > 0) new Statement(this, text, statement).Run();
                    statement = new();
                }
                else statement.Add(token);
            }
        }
    }

    sealed class Statement
    {
        readonly Body body;
        readonly Case cs;
        readonly string text;
        readonly List<Token> tokens;
        int at;

        public Statement(Body b, string t, List<Token> tk) { body = b; cs = b.Case; text = t; tokens = tk; }

        static readonly Token EndToken = new("end", "");
        Token Peek(int ahead = 0) => at + ahead < tokens.Count ? tokens[at + ahead] : EndToken;
        Token Take()
        {
            var token = Peek();
            if (token.Kind == "end") throw Refuse("the end of the statement", text);
            at++;
            return token;
        }
        void Expect(string word)
        {
            var token = Take();
            if (token.Low != word) throw Refuse(token.Text, text);
        }
        void Done() { if (at != tokens.Count) throw Refuse(Peek().Text, text); }
        RefusedVbaException Fail(Token token = null)
        {
            token ??= Peek();
            return Refuse(token.Text.Length > 0 ? token.Text : text, text);
        }
        Var ScopeVar(Token token)
        {
            if (token.Kind != "id") return null;
            if (body.Scope.TryGetValue(token.Low, out var v)) return v;
            return cs.ModuleVars[body.Module].GetValueOrDefault(token.Low);
        }
        void Close(string what, bool keep = false)
        {
            if (body.Stack.Count == 0 || body.Stack[^1] != what) throw Fail();
            if (!keep) body.Stack.RemoveAt(body.Stack.Count - 1);
        }

        public void Run()
        {
            var first = Peek();
            var word = first.Low;
            var stack = body.Stack;
            if (first.Kind != "id") throw Fail();
            switch (word)
            {
                case "dim":
                    Take();
                    cs.Variables(tokens.Skip(1).ToList(), text, body.Scope, body.Prefix);
                    return;
                case "const":
                    Take();
                    cs.Constant(tokens.Skip(1).ToList(), text, body.Scope, body.Prefix);
                    return;
                case "set": SetStatement(); return;
                case "if": IfStatement(); return;
                case "elseif":
                case "else":
                    Take();
                    Close("if", true);
                    if (word == "elseif") { Expression(); Expect("then"); }
                    Done();
                    return;
                case "end":
                    {
                        Take();
                        var what = Take();
                        if (what.Low is not ("if" or "select")) throw Fail(what);
                        Close(what.Low);
                        Done();
                        return;
                    }
                case "for": ForStatement(); return;
                case "next":
                    Take();
                    Close("for");
                    if (Peek().Kind == "id") Take();
                    Done();
                    return;
                case "do":
                case "loop":
                    Take();
                    if (word == "do") stack.Add("do"); else Close("do");
                    if (Peek().Low is "while" or "until") { Take(); Expression(); }
                    Done();
                    return;
                case "while":
                    Take();
                    Expression();
                    stack.Add("while");
                    Done();
                    return;
                case "wend":
                    Take();
                    Close("while");
                    Done();
                    return;
                case "select":
                    Take();
                    Expect("case");
                    Expression();
                    stack.Add("select");
                    Done();
                    return;
                case "case":
                    Take();
                    Close("select", true);
                    if (Peek().Low == "else") Take();
                    else
                        while (true)
                        {
                            if (Peek().Low == "is")
                            {
                                Take();
                                if (Take().Text is not ("=" or "<" or ">" or "<=" or ">=" or "<>")) throw Fail();
                                Expression();
                            }
                            else
                            {
                                Expression();
                                if (Peek().Low == "to") { Take(); Expression(); }
                            }
                            if (Peek().Text != ",") break;
                            Take();
                        }
                    Done();
                    return;
                case "exit":
                    {
                        Take();
                        var what = Take().Low;
                        bool function = body.Proc.IsFunction;
                        if (!(what == "function" && function || what == "sub" && !function || what is "for" or "do" && stack.Contains(what)))
                            throw Fail(first);
                        Done();
                        return;
                    }
                case "on":
                    Take();
                    Expect("error");
                    if (Peek().Low == "resume") { Take(); Expect("next"); }
                    else
                    {
                        Expect("goto");
                        var target = Take();
                        if (target.Text != "0") Jump(target);
                    }
                    Done();
                    return;
                case "resume":
                    Take();
                    Expect("next");
                    Done();
                    return;
                case "goto":
                    Take();
                    Jump(Take());
                    Done();
                    return;
                case "call":
                    {
                        Take();
                        var (_, _, last) = Chain(false);
                        if (last is not { Type: "call" }) throw Fail(first);
                        Done();
                        return;
                    }
            }
            SimpleStatement();
        }

        void Jump(Token target)
        {
            if (!(target.Kind == "id" && !Reserved.Contains(target.Low) || target.Kind == "num" && target.Text.All(char.IsAsciiDigit)))
                throw Fail(target);
            body.Jumps.Add((target.Low, text));
        }

        void IfStatement()
        {
            Take();
            Expression();
            Expect("then");
            if (at == tokens.Count) { body.Stack.Add("if"); return; }
            var parts = new List<List<Token>> { new() };
            foreach (var token in tokens.Skip(at))
                if (token.Kind == "id" && token.Low == "else" && parts.Count == 1) parts.Add(new());
                else parts[^1].Add(token);
            foreach (var part in parts)
            {
                if (part.Count == 0 || BlockWords.Contains(part[0].Low)) throw Refuse(part.Count > 0 ? part[0].Text : "Then", text);
                new Statement(body, text, part).Run();
            }
            at = tokens.Count;
        }

        void ForStatement()
        {
            Take();
            bool each = Peek().Low == "each";
            if (each) Take();
            var token = Take();
            var v = ScopeVar(token);
            if (v == null || v.Param || v.Array || v.Const || v.Kind != "val") throw Fail(token);
            if (each)
            {
                cs.Flows.Add((v.Key, Unsafe));
                Expect("in");
                var (kind, _, _) = Chain(false);
                if (kind != "val") throw Fail();
            }
            else
            {
                Expect("=");
                cs.Flows.Add((v.Key, Num));      // a numeric loop: the variable holds numbers
                cs.Flows.Add((v.Key, Expression()));
                Expect("to");
                Expression();
                if (Peek().Low == "step") { Take(); Expression(); }
            }
            body.Stack.Add("for");
            Done();
        }

        void SetStatement()
        {
            Take();
            var token = Take();
            var v = ScopeVar(token);
            if (v == null || v.Param || v.Array || v.Const) throw Fail(token);
            Expect("=");
            if (Peek().Low == "nothing" && at + 1 == tokens.Count) { Take(); return; }
            var (kind, _, _) = Chain(false);
            Done();
            if (!ObjectKinds.Contains(kind) || kind == "err" || v.Kind != "pending" && v.Kind != kind) throw Fail(token);
            v.Kind = kind;
        }

        void SimpleStatement()
        {
            var first = Peek();
            var (_, _, last) = Chain(true);
            if (Peek().Text != "=")
            {
                if (last is not { Type: "call" }) throw Fail(first);
                Done();
                return;
            }
            Take();
            if (last == null || last.Type == "call") throw Fail(first);
            if (last.Type == "var")
            {
                var v = last.Var;
                if (v.Param || v.Const || v.Kind != "val") throw Fail(first);
                cs.Flows.Add((v.Key, Expression()));
            }
            else
            {
                if (!Members[last.Owner][last.Member].flags.Contains('s')) throw Fail(first);
                if (last.Member == "numberformat")
                {
                    if (tokens.Count - at != 1 || Peek().Kind != "str") throw Fail(first);
                    Take();
                }
                else
                {
                    var node = Expression();
                    if (last.Member is "value" or "value2" && last.Owner == "range") cs.Writes.Add((node, text));
                }
            }
            Done();
        }

        string Binary()
        {
            var token = Peek();
            if (token.Kind == "op" && BinarySymbols.Contains(token.Text)) return token.Text;
            if (token.Kind == "id" && BinaryWords.Contains(token.Low)) return token.Low;
            return null;
        }

        Node Expression()
        {
            var nodes = new List<Node>();
            var ops = new List<string>();
            while (true)
            {
                bool negated = false;
                while (Peek().Kind == "op" && Peek().Text == "-" || Peek().Kind == "id" && Peek().Low == "not")
                {
                    Take();
                    negated = true;
                }
                var (kind, node, _) = Chain(false);
                if (kind != "val") throw Fail();
                nodes.Add(negated ? Num : node);
                var op = Binary();
                if (op == null) break;
                Take();
                ops.Add(op);
            }
            if (nodes.Count == 1) return nodes[0];
            return new Node("cat", items: nodes, ops: ops);
        }

        List<Node> Arguments(int low, int high)
        {
            var nodes = new List<Node>();
            if (Peek().Text == "(")
            {
                Take();
                if (Peek().Text != ")")
                    while (true)
                    {
                        nodes.Add(Expression());
                        if (Peek().Text != ",") break;
                        Take();
                    }
                Expect(")");
            }
            if (nodes.Count < low || nodes.Count > high) throw Fail();
            return nodes;
        }

        (string kind, Node node, Last last) Chain(bool statement)
        {
            var token = Take();
            var word = token.Low;
            if (token.Kind == "num") return ("val", Num, null);
            if (token.Kind == "hex") return ("val", Num, null);        // &H8000 is -32768
            if (token.Kind == "str") return ("val", Case.Literal(token), null);
            if (token.Text == "(")
            {
                var inner = Expression();
                Expect(")");
                return ("val", inner, null);
            }
            if (token.Kind != "id") throw Fail(token);
            if (word is "true" or "false" || word.StartsWith("xl", StringComparison.Ordinal) && Constants.Contains(word))
                return ("val", Num, null);
            if (word is "empty" or "null" or "nothing" or "vbnullstring") return ("val", Maybe, null);
            if (Constants.Contains(word)) return ("val", Safe, null);
            if (Builtins.TryGetValue(word, out var builtin))
            {
                var args = Arguments(builtin.low, builtin.high);
                return ("val", builtin.safety switch
                {
                    "num" => Num,
                    "text" => Safe,
                    "never" => Unsafe,
                    _ => new Node(word == "array" ? "join" : "str", key: word, items: args),
                }, null);
            }
            string kind;
            if (word == "thisworkbook") kind = "wb";
            else if (word == "me")
            {
                if (cs.Kinds[body.Module] != "document") throw Fail(token);
                kind = body.Module == "thisworkbook" ? "wb" : "sheet";
            }
            else if (word == "err") kind = "err";
            else if (word == "createobject")
            {
                Expect("(");
                var literal = Take();
                if (literal.Kind != "str" || !Classes.ContainsKey(literal.Text[1..^1].ToLowerInvariant()))
                    throw Refuse("CreateObject requires one listed literal class", text);
                Expect(")");
                kind = Classes[literal.Text[1..^1].ToLowerInvariant()];
            }
            else
            {
                var v = ScopeVar(token);
                if (v != null)
                {
                    if (v.Kind == "val")
                    {
                        if (Peek().Text == "(")
                        {
                            if (!v.Array) throw Fail();
                            Arguments(1, 4);
                        }
                        return ("val", new Node("var", v.Key), new Last { Type = "var", Var = v });
                    }
                    if (v.Kind == "pending") throw Fail(token);
                    kind = v.Kind;
                    if (kind == "dict" && Peek().Text == "(")
                    {
                        Arguments(1, 1);
                        return ("val", Unsafe, new Last { Type = "member", Member = "item", Owner = "dict" });
                    }
                }
                else
                {
                    var proc = FindProc(token) ?? throw Fail(token);
                    return CallProc(token, proc, statement);
                }
            }
            return MemberChain(token, kind, statement);
        }

        (string, Node, Last) MemberChain(Token token, string kind, bool statement)
        {
            while (Peek().Text == ".")
            {
                Take();
                var name = Take();
                if (name.Kind != "id" || !Members.TryGetValue(kind, out var owner) || !owner.TryGetValue(name.Low, out var entry)) throw Fail(name);
                var (ret, low, high, flags) = entry;
                var nodes = Peek().Text == "(" ? Arguments(0, high) : new List<Node>();
                if (statement && flags.Contains('c') && high > 0 && nodes.Count == 0 && Peek().Kind != "end" && Peek().Text is not ("=" or "."))
                    while (true)
                    {
                        nodes.Add(Expression());
                        if (Peek().Text != ",") break;
                        Take();
                    }
                if (nodes.Count < low || nodes.Count > high) throw Fail(name);
                if (ObjectKinds.Contains(ret)) { kind = ret; continue; }
                if (Peek().Text == ".") throw Fail();
                var last = flags.Contains('c') && Peek().Text != "=" ? CallLast : new Last { Type = "member", Member = name.Low, Owner = kind };
                return ("val", ret is "num" or "bool" ? Num : Unsafe, last);
            }
            if (statement) throw Fail(token);
            return (kind, Safe, null);
        }

        Proc FindProc(Token token)
        {
            var word = token.Low;
            if (cs.Names.Contains(word) && Peek().Text == ".")
            {
                Take();
                var name = Take();
                cs.Procs.TryGetValue($"{word}.{name.Low}", out var qualified);
                if (cs.Kinds[word] != "standard" || qualified == null || !qualified.Public) throw Fail(name);
                return qualified;
            }
            if (cs.Procs.TryGetValue($"{body.Module}.{word}", out var own)) return own;
            return cs.Procs.Values.FirstOrDefault(o => o.Name == word && o.Public && cs.Kinds[o.Module] == "standard");
        }

        (string, Node, Last) CallProc(Token token, Proc proc, bool statement)
        {
            int count = proc.Params.Count;
            var nodes = new List<Node>();
            if (Peek().Text == "(") nodes = Arguments(count, count);
            else if (statement && count > 0)
                while (true)
                {
                    nodes.Add(Expression());
                    if (Peek().Text != ",") break;
                    Take();
                }
            if (nodes.Count != count) throw Fail(token);
            for (int i = 0; i < count; i++)
            {
                if (ObjectTypes.ContainsKey(proc.Params[i].type)) throw Fail(token);
                cs.Flows.Add(($"{proc.Key}.{proc.Params[i].name}", nodes[i]));
            }
            return ("val", proc.IsFunction ? new Node("var", $"{proc.Key}.{proc.Name}") : Unsafe, CallLast);
        }
    }

    /// <summary>Checks every module of a case; returns the lowercase entry points ("module.procedure") a call may name.</summary>
    public static (HashSet<string> entryPoints, HashSet<string> procedures) CheckModules(List<(string name, string kind, string source)> modules, int sheetCount)
    {
        var seen = new HashSet<string>();
        foreach (var (name, kind, _) in modules)
        {
            var low = name.ToLowerInvariant();
            bool valid = kind == "standard"
                ? ModuleName.IsMatch(name) && !Reserved.Contains(low) && !SheetCodeName.IsMatch(low) && low != "thisworkbook"
                : name == "ThisWorkbook" || SheetDocumentName.IsMatch(name) && int.Parse(name[5..]) >= 1 && int.Parse(name[5..]) <= sheetCount;
            if (!valid || !seen.Add(low) || kind is not ("standard" or "document"))
                throw new RefusedVbaException("invalid or duplicate VBA module name or kind: " + Escape(name));
        }
        var checkedCase = new Case(modules.Select(m => (m.name, m.kind, m.source)).ToList());
        var entryPoints = checkedCase.Run();
        // Every procedure of a standard module, Private ones too: a worksheet formula calls a Private Function (rw17).
        return (entryPoints, checkedCase.AllProcedures);
    }
}
