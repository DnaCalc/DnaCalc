using System.Text;
using System.Text.RegularExpressions;

namespace Touchstone.Excel;

/// <summary>
/// The grammar used when admitting a Touchstone VBA probe template.
/// Only the listed statement forms, expressions and member chains are accepted. A cell value written through
/// .Value or .Value2 must provably be data (a number, a boolean, a string that does not start with = + - or @).
/// The same check runs on the rendered modules of every request (VbaPolicy.CheckGroup), so the package holds
/// only text this grammar read. Rules that keep this reader and Excel's reader the same (dc-heiq6): a module
/// begins with Option Explicit, a comment cannot end in a line continuation, a name has one declaration, and one
/// object-model call addresses a bounded block of the case workbook.
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
    static readonly Dictionary<string, Dictionary<string, (string ret, int low, int high, string flags, string rule)>> Members = ReadMembers();
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

    // A member row is "owner member returns low high flags [rule]". The rule says what its arguments may be:
    // a = literal A1 addresses or address parameters, i = plainly numeric indexes, z = whole-number sizes 1..MaxResize.
    static Dictionary<string, Dictionary<string, (string, int, int, string, string)>> ReadMembers()
    {
        var table = new Dictionary<string, Dictionary<string, (string, int, int, string, string)>>();
        foreach (string row in (List<object>)Rules["members"])
        {
            var p = row.Split(' ');
            if (!table.TryGetValue(p[0], out var owner)) table[p[0]] = owner = new();
            owner[p[1]] = (p[2], int.Parse(p[3]), int.Parse(p[4]), p[5], p.Length > 6 ? p[6] : "");
        }
        return table;
    }

    internal static readonly HashSet<string> LayoutFunctions = Set("layout_functions");
    // Every worksheet function the catalogue knows, lowercase. Excel answers a formula's call with its own function
    // before a procedure of the same name, so a procedure with such a name would let a formula call the built-in.
    internal static readonly HashSet<string> WorksheetFunctions = ReadWorksheetFunctions();

    static HashSet<string> ReadWorksheetFunctions()
    {
        using var resource = typeof(VbaGrammar).Assembly.GetManifestResourceStream("admission-families.tsv");
        using var reader = new StreamReader(resource);
        var names = new HashSet<string>();
        while (reader.ReadLine() is string line) names.Add(line.Split('\t')[1].ToLowerInvariant());
        return names;
    }
    static readonly HashSet<string> NumericBuiltins = Builtins.Where(b => b.Value.safety == "num").Select(b => b.Key).ToHashSet();
    static readonly HashSet<string> SheetNames = new() { "range", "cells", "rows", "columns" };   // a worksheet module's own cells
    static readonly Regex Rectangle = new(@"\A(\$?[A-Za-z]{1,3}\$?[1-9][0-9]{0,6})(?::(\$?[A-Za-z]{1,3}\$?[1-9][0-9]{0,6}))?\z", RegexOptions.CultureInvariant);
    const long MaxRangeCells = 1048576;     // one Range call: at most a column's worth of cells
    const int MaxResize = 1024, MaxDepth = 32, MaxLine = 1023, MaxContinuations = 24;

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
    // dc-heiq6: "starts" means the first character that is not a blank (space, tab, CR, LF). A blank string (" ", vbTab,
    // vbLf) is Text, never SafeText, and " =1" is Bad: whether Excel reads a blank-led entry as text is then no part of
    // the argument.
    const int SafeText = 0, Text = 1, Number = 2, NumText = 3, Bad = 4;
    static readonly Node Safe = new("safe"), Maybe = new("maybe"), Num = new("num"), Unsafe = new("unsafe");

    sealed class Last
    {
        public string Type, Member, Owner;
        public Var Var;
    }

    static readonly Last CallLast = new() { Type = "call" };

    static List<(string text, List<Token> tokens)> LogicalLines(string source, IReadOnlyDictionary<string, string> parameters)
    {
        if (source.Replace("\r\n", "").Contains('\r'))
            throw new RefusedVbaException("VBA construct refused: a bare carriage return");
        var lines = new List<(string, List<Token>)>();
        var tokens = new List<Token>();
        var text = "";
        bool pending = false;
        int continued = 0;
        foreach (var physical in source.Replace("\r\n", "\n").Split('\n'))
        {
            int at = 0;
            bool afterSpace = false;
            pending = false;
            int widest = physical.Length;      // with each placeholder at its longest rendering
            while (at < physical.Length)
            {
                if (parameters != null && physical.AsSpan(at).StartsWith("{{", StringComparison.Ordinal))
                {
                    var placeholder = VbaTemplates.Placeholder.Match(physical, at);
                    if (!placeholder.Success || placeholder.Index != at
                        || !parameters.TryGetValue(placeholder.Groups[1].Value, out var type))
                        throw Refuse("an undeclared or malformed placeholder", physical);
                    int end = at + placeholder.Length;
                    if ((at > 0 && TouchesLiteral(physical[at - 1])) || (end < physical.Length && TouchesLiteral(physical[end])))
                        throw Refuse("a placeholder joined to another token", physical);
                    tokens.Add(new Token(type == "long" ? "parameter-number" : type == "address" ? "parameter-address" : "parameter-string",
                        "placeholder " + placeholder.Value));
                    widest += Widest(type) - placeholder.Length;
                    at = end;
                    afterSpace = false;
                    continue;
                }
                var match = Lex.Match(physical, at);
                if (!match.Success) throw Refuse(physical[at].ToString(), physical);
                at = match.Index + match.Length;
                string kind = LexGroups.First(g => match.Groups[g].Success);
                if (parameters != null && (match.Value.Contains("{{", StringComparison.Ordinal)
                    || kind == "cont" && physical[at..].Contains("{{", StringComparison.Ordinal)))
                    throw Refuse("a placeholder inside a quoted string, comment or continuation", physical);
                // Excel continues a comment that ends in " _": the next physical line would be code here and comment there.
                if (kind == "comment" && match.Value.TrimEnd(' ', '\t').EndsWith('_'))
                    throw Refuse("a comment ending in a line continuation", physical);
                if (kind == "cont")
                {
                    if (!afterSpace) throw Refuse("_", physical);
                    pending = true;
                    break;
                }
                afterSpace = kind == "ws";
                if (kind is not ("ws" or "comment")) tokens.Add(new Token(kind, match.Value));
            }
            // Excel does not compile a longer line or a longer continued statement; it shows a dialog instead.
            if (widest > MaxLine) throw Refuse($"a line that can exceed {MaxLine} characters", physical);
            continued = pending ? continued + 1 : 0;
            if (continued > MaxContinuations) throw Refuse($"a statement continued over more than {MaxContinuations} line breaks", physical);
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

    // The longest text a parameter of this type is printed as (VbaTemplates.Render): a quoted text of 255 characters
    // with every quote doubled, a quoted sheet name or cell, a parenthesised negative Long, the longest listed word.
    static int Widest(string type) => type switch
    {
        "text" => 2 * 255 + 2,
        "sheet" => 31 + 2,
        "address" => 10 + 2,
        "long" => 13,
        _ => type.StartsWith("enum:", StringComparison.Ordinal) ? type[5..].Split('|').Max(word => word.Length) + 2 : 2 * 255 + 2,
    };

    static bool TouchesLiteral(char ch) => char.IsAsciiLetterOrDigit(ch) || "_.\"!#%$@&{}".Contains(ch);
    static bool IsTextParameter(string kind) => kind is "parameter-string" or "parameter-address";

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
        public readonly HashSet<string> ProcNames = new();
        readonly IReadOnlyDictionary<string, string> parameters;

        public Case(List<(string, string, string)> modules, IReadOnlyDictionary<string, string> parameters)
        {
            Modules = modules;
            this.parameters = parameters;
            Names = modules.Select(m => m.Item1.ToLowerInvariant()).ToHashSet();
        }

        public string Declared(Token token, string line)
        {
            if (token.Kind != "id" || !Identifier.IsMatch(token.Text) || Reserved.Contains(token.Low) || Names.Contains(token.Low))
                throw Refuse(token.Text, line);
            return token.Low;
        }

        // A name has one declaration (dc-heiq6): a local or a parameter never shadows a module variable of its module,
        // and no variable carries a procedure's name. Which declaration a use means then never depends on where the
        // Dim stands, the one point where this reader and Excel's could differ.
        public void DeclaredOnce(string name, string module, string line)
        {
            if (ModuleVars[module].ContainsKey(name) || ProcNames.Contains(name))
                throw Refuse(name + " (a name declared twice)", line);
        }

        public CheckedModules Run()
        {
            foreach (var (name, kind, source) in Modules) Scan(name.ToLowerInvariant(), kind, source);
            foreach (var (module, vars) in ModuleVars)
                foreach (var name in vars.Keys)
                    if (ProcNames.Contains(name)) throw Refuse(name + " (a name declared twice)", module);
            foreach (var proc in Procs.Values.ToList()) new Body(this, proc).Run();
            var classes = new Dictionary<string, int>();
            foreach (var key in Params.Concat(Numeric)) classes[key] = Number;
            foreach (var key in Params.Where(Strings.Contains)) classes[key] = NumText;     // a number argument becomes number text
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
            var entries = Procs.Values.Where(p => p.Public && Kinds[p.Module] == "standard").ToList();
            return new CheckedModules(entries.Select(p => p.Key).ToHashSet(),
                entries.Where(p => p.IsFunction).Select(p => p.Name).ToHashSet(), ProcNames);
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
            bool first = true;
            foreach (var (text, tokens) in LogicalLines(source, parameters))
            {
                var words = tokens.Select(t => t.Low).ToList();
                if (first)
                {
                    // With Option Explicit Excel refuses to compile a name it does not see declared, so a declaration
                    // this reader sees and Excel does not is a compile error there, never an implicit Variant.
                    if (words.Count != 2 || words[0] != "option" || words[1] != "explicit")
                        throw Refuse("a module that does not begin with Option Explicit", text);
                    first = false;
                    continue;
                }
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
            // A layout formula may call a template function by name, bare or after its module name. Neither spelling
            // may be a worksheet function (WEBSERVICE; a module Register with a function ID), or Excel would run its own.
            if (WorksheetFunctions.Contains(name) || WorksheetFunctions.Contains(module + "." + name))
                throw Refuse(tokens[at + 1].Text + " (a VBA procedure name collides with a worksheet function)", text);
            if (name.StartsWith("auto_", StringComparison.Ordinal))
                throw Refuse(tokens[at + 1].Text + " (an Auto_ macro name)", text);
            ProcNames.Add(name);
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
                else if (tokens[pos + 1].Low == "string") Strings.Add($"{module}.{name}.{name}");
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

        public void Variables(List<Token> tokens, string text, Dictionary<string, Var> scope, string prefix, bool allowObjects = true,
            string localIn = null)
        {
            int pos = 0;
            while (true)
            {
                if (pos >= tokens.Count) throw Refuse(text, text);
                var name = Declared(tokens[pos], text);
                if (scope.ContainsKey(name)) throw Refuse(tokens[pos].Text, text);
                if (localIn != null) DeclaredOnce(name, localIn, text);
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

        public void Constant(List<Token> tokens, string text, Dictionary<string, Var> scope, string prefix, string localIn = null)
        {
            // name = literal, or a negative number as -5 or (-5) (the form a negative Long parameter is printed in)
            if (tokens.Count < 3 || tokens[1].Text != "=") throw Refuse(text, text);
            var value = tokens.Skip(2).ToList();
            if (value.Count == 4 && value[0].Text == "(" && value[3].Text == ")") value = value.GetRange(1, 2);
            bool negative = value.Count == 2 && value[0].Kind == "op" && value[0].Text == "-" && value[1].Kind == "num";
            if (!negative && (value.Count != 1
                || value[0].Kind is not ("num" or "str" or "hex" or "parameter-number" or "parameter-string" or "parameter-address")))
                throw Refuse(text, text);
            var name = Declared(tokens[0], text);
            if (scope.ContainsKey(name)) throw Refuse(tokens[0].Text, text);
            if (localIn != null) DeclaredOnce(name, localIn, text);
            scope[name] = new Var(prefix + name, isConst: true);
            Flows.Add((prefix + name, negative ? Num : Literal(value[0])));
        }

        public static Node Literal(Token token)
        {
            if (IsTextParameter(token.Kind)) return Maybe;
            if (token.Kind == "str")
            {
                var visible = token.Text[1..^1].Replace("\"\"", "\"").TrimStart(' ', '\t');
                if (visible.Length == 0) return Maybe;
                return FormulaStart.Contains(visible[0]) ? Unsafe : Safe;
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
        public int Depth;

        public Body(Case c, Proc proc)
        {
            Case = c; Proc = proc; Module = proc.Module; Prefix = proc.Key + ".";
            // A worksheet formula can call a standard-module procedure with any cell value, text included (a template
            // layout formula =Probe(A1) over a request's text cell); Private hides a function from the function list,
            // not from a cell. Only a parameter of a numeric type is then a number: Excel converts the argument or
            // fails the call. Any other parameter is not plainly data.
            bool formulaCallable = c.Kinds[proc.Module] == "standard";
            foreach (var (pname, ptype) in proc.Params)
            {
                c.DeclaredOnce(pname, Module, proc.Line);
                var kind = ObjectTypes.GetValueOrDefault(ptype, "val");
                Scope[pname] = new Var(Prefix + pname, kind == "pending" ? "val" : kind, NumericTypes.Contains(ptype), param: true);
                if (NumericTypes.Contains(ptype)) c.Numeric.Add(Prefix + pname);
                if (ptype == "string") c.Strings.Add(Prefix + pname);
                c.Params.Add(Prefix + pname);
                if (formulaCallable && !NumericTypes.Contains(ptype)) c.Flows.Add((Prefix + pname, Unsafe));
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
                    cs.Variables(tokens.Skip(1).ToList(), text, body.Scope, body.Prefix, localIn: body.Module);
                    return;
                case "const":
                    Take();
                    cs.Constant(tokens.Skip(1).ToList(), text, body.Scope, body.Prefix, body.Module);
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
                new Statement(body, text, part).Run();     // a nested single-line If is refused above
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

        // The checker recurses with the source; a bound keeps a template from exhausting the worker's stack.
        void Deeper()
        {
            if (++body.Depth > MaxDepth) throw Refuse("a statement or expression nested too deeply", text);
        }

        Node Expression()
        {
            Deeper();
            try { return Operands(); }
            finally { body.Depth--; }
        }

        Node Operands()
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

        List<Node> Arguments(int low, int high, List<(int from, int to)> spans = null)
        {
            var nodes = new List<Node>();
            if (Peek().Text == "(")
            {
                Take();
                if (Peek().Text != ")")
                    while (true)
                    {
                        int from = at;
                        nodes.Add(Expression());
                        spans?.Add((from, at));
                        if (Peek().Text != ",") break;
                        Take();
                    }
                Expect(")");
            }
            if (nodes.Count < low || nodes.Count > high) throw Fail();
            return nodes;
        }

        // One object-model call addresses a bounded block of the case workbook (dc-heiq6). Range takes addresses the
        // checker can read: a literal A1 cell or rectangle of at most MaxRangeCells, or an address parameter (a literal
        // once rendered). So no address is built at run time: no defined name, no other sheet or workbook, no "A:XFD".
        // Indexes are plainly numbers, so a string such as "1:1048576" cannot stand for a block; a computed cell is
        // Cells(row, column). Resize takes both sizes as whole-number literals up to MaxResize. What one chain can then
        // address is a Range literal, a row, a column, the used range, or MaxResize x MaxResize cells.
        void CheckArguments(string rule, List<(int from, int to)> spans)
        {
            if (rule == "a")
            {
                var corners = new List<(long r1, long c1, long r2, long c2)>();
                bool known = true;
                foreach (var (from, to) in spans)
                {
                    var token = tokens[from];
                    if (to - from == 1 && token.Kind == "parameter-address") { known = false; continue; }
                    var match = to - from == 1 && token.Kind == "str" ? Rectangle.Match(token.Text[1..^1]) : Match.Empty;
                    if (!match.Success || !VbaPolicy.IsAddress(match.Groups[1].Value)
                        || match.Groups[2].Success && (spans.Count == 2 || !VbaPolicy.IsAddress(match.Groups[2].Value)))
                        throw Refuse("Range takes a literal A1 address or an address parameter (Cells(row, column) for a computed cell)", text);
                    var (r1, c1) = RowColumn(match.Groups[1].Value);
                    var (r2, c2) = match.Groups[2].Success ? RowColumn(match.Groups[2].Value) : (r1, c1);
                    corners.Add((Math.Min(r1, r2), Math.Min(c1, c2), Math.Max(r1, r2), Math.Max(c1, c2)));
                }
                if (known && (corners.Max(c => c.r2) - corners.Min(c => c.r1) + 1) * (corners.Max(c => c.c2) - corners.Min(c => c.c1) + 1) > MaxRangeCells)
                    throw Refuse($"a Range of more than {MaxRangeCells} cells", text);
            }
            else if (rule == "i")
            {
                foreach (var (from, to) in spans)
                    if (!PlainlyNumeric(from, to))
                        throw Refuse("a row, column or offset that is not plainly a number (a number, a numeric variable, CLng(...))", text);
            }
            else if (rule == "z")
            {
                foreach (var (from, to) in spans)
                {
                    var token = tokens[from];
                    if (to - from != 1 || token.Kind != "parameter-number" && !(token.Kind == "num" && token.Text.Length <= 4
                            && token.Text.All(char.IsAsciiDigit) && int.Parse(token.Text) is >= 1 and <= MaxResize))
                        throw Refuse($"Resize takes whole-number literals from 1 to {MaxResize}", text);
                }
            }
        }

        static (long row, long column) RowColumn(string address)
        {
            address = address.Replace("$", "");
            int i = 0;
            long column = 0;
            while (char.IsAsciiLetter(address[i])) column = column * 26 + char.ToUpperInvariant(address[i++]) - 'A' + 1;
            return (long.Parse(address[i..]), column);
        }

        // Numbers, numeric variables and arithmetic between them, or one call of a builtin that returns a number.
        bool PlainlyNumeric(int from, int to)
        {
            if (to - from >= 4 && tokens[from].Kind == "id" && NumericBuiltins.Contains(tokens[from].Low) && tokens[from + 1].Text == "(")
            {
                int depth = 0, close = -1;
                for (int i = from + 1; i < to && close < 0; i++)
                {
                    if (tokens[i].Kind == "op" && tokens[i].Text == "(") depth++;
                    else if (tokens[i].Kind == "op" && tokens[i].Text == ")" && --depth == 0) close = i;
                }
                if (close == to - 1) return true;
            }
            for (int i = from; i < to; i++)
            {
                var token = tokens[i];
                bool numeric = token.Kind is "num" or "hex" or "parameter-number"
                    || token.Kind == "op" && token.Text is "+" or "-" or "*" or "/" or "\\" or "^" or "(" or ")"
                    || token.Kind == "id" && (token.Low == "mod" || ScopeVar(token) is { Numeric: true, Array: false, Kind: "val" });
                if (!numeric) return false;
            }
            return to > from;
        }

        (string kind, Node node, Last last) Chain(bool statement)
        {
            var token = Take();
            var word = token.Low;
            if (token.Kind is "num" or "parameter-number") return ("val", Num, null);
            if (token.Kind == "hex") return ("val", Num, null);        // &H8000 is -32768
            if (token.Kind == "str" || IsTextParameter(token.Kind)) return ("val", Case.Literal(token), null);
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
            if (Constants.Contains(word)) return ("val", Maybe, null);      // vbTab, vbCr, vbLf, vbCrLf, vbNewLine: blanks
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
            else if (SheetNames.Contains(word) && cs.Kinds[body.Module] == "document" && body.Module != "thisworkbook")
            {
                // In a worksheet module these are the module's own sheet (Me.Range): a member of the document class
                // comes before the global of the same name. In any other module they mean the active sheet: refused.
                at--;
                return MemberChain(token, "sheet", statement, implied: true);
            }
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

        (string, Node, Last) MemberChain(Token token, string kind, bool statement, bool implied = false)
        {
            while (implied || Peek().Text == ".")
            {
                if (!implied) Take();
                implied = false;
                var name = Take();
                if (name.Kind != "id" || !Members.TryGetValue(kind, out var owner) || !owner.TryGetValue(name.Low, out var entry)) throw Fail(name);
                var (ret, low, high, flags, rule) = entry;
                var spans = new List<(int from, int to)>();
                var nodes = Peek().Text == "(" ? Arguments(0, high, spans) : new List<Node>();
                if (rule != "" && nodes.Count >= low) CheckArguments(rule, spans);
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

    /// <summary>Checks every module of a case; returns the lowercase entry points ("module.procedure") a call may name,
    /// the public standard-module functions a layout formula may call, and every procedure name.</summary>
    public static CheckedModules CheckModules(
        List<(string name, string kind, string source)> modules, int sheetCount,
        IReadOnlyDictionary<string, string> parameters = null, IReadOnlyDictionary<string, string[]> signatures = null)
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
        var checkedCase = new Case(modules, parameters);
        var result = checkedCase.Run();
        if (signatures != null)
            foreach (var (entry, arguments) in signatures)
            {
                var key = entry.ToLowerInvariant();
                if (!result.EntryPoints.Contains(key) || checkedCase.Procs[key].Params.Count != arguments.Length)
                    throw new RefusedVbaException("VBA template entry point does not match its declared signature: " + entry);
            }
        return result;
    }
}

/// <summary>What the grammar found in the modules it accepted. Names are lowercase.</summary>
sealed record CheckedModules(HashSet<string> EntryPoints, HashSet<string> Functions, HashSet<string> Procedures);
