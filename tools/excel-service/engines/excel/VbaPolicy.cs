using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;

namespace Touchstone.Excel;

sealed class RefusedVbaException(string message) : Exception(message);

/// <summary>Checks authored VBA probe source before it can be handed to Excel.</summary>
static class VbaPolicy
{
    static readonly Dictionary<string, object> Rules = ReadRules();
    static readonly Regex Lex = new((string)Rules["lex"], RegexOptions.CultureInvariant);
    static readonly Regex Identifier = new(@"\A" + Rules["identifier"] + @"\z", RegexOptions.CultureInvariant);
    static readonly HashSet<string> Declares = Words("declares");
    static readonly HashSet<string> AllowedWords = new(Words("words").Concat(Words("objects")).Concat(Declares))
        { "createobject", "me" };
    static readonly HashSet<string> Members = Words("members");
    static readonly HashSet<string> Refused = Words("refused");
    static readonly HashSet<string> Undeclarable = Words("undeclarable");
    static readonly HashSet<string> Classes = ((List<object>)Rules["classes"])
        .Select(c => "\"" + ((string)c).ToLowerInvariant() + "\"").ToHashSet();
    static readonly HashSet<string> Punctuation = new()
        { ".", ":", "=", "<", ">", "+", "-", "*", "/", "\\", "^", "&", "(", ")", ",", "\n" };

    static Dictionary<string, object> ReadRules()
    {
        using var resource = typeof(VbaPolicy).Assembly.GetManifestResourceStream("vba-probe-policy.json");
        using var reader = new StreamReader(resource);
        return (Dictionary<string, object>)Json.Parse(reader.ReadToEnd());
    }

    public static int Limit(string key) => (int)(long)Rules[key];
    static HashSet<string> Words(string key) => ((string)Rules[key]).Split(' ').ToHashSet();

    public static bool IsVba(Dictionary<string, object> group) =>
        Equals(group.GetValueOrDefault("f"), "_VBA")
        || group.GetValueOrDefault("k") is string tokens && tokens.Split(',').Contains("V");

    public static Dictionary<string, object> CheckGroup(object value)
    {
        if (value is not Dictionary<string, object> group
            || !Equals(group.GetValueOrDefault("f"), "_VBA") || !Equals(group.GetValueOrDefault("k"), "V")
            || !Equals(group.GetValueOrDefault("n"), 1L)
            || group.GetValueOrDefault("v") is not List<object> values || values.Count != 1
            || values[0] is not Dictionary<string, object> probe)
            throw new RefusedVbaException("a VBA group contains one VBA case");
        if (probe.GetValueOrDefault("modules") is not List<object> modules
            || modules.Count < 1 || modules.Count > Limit("max_modules"))
            throw new RefusedVbaException("invalid VBA module count");
        var names = new HashSet<string>();
        var sources = new List<(string name, string kind, string source)>();
        foreach (var item in modules)
        {
            if (item is not Dictionary<string, object> module || module.Count != 3
                || module.GetValueOrDefault("name") is not string name || !Identifier.IsMatch(name)
                || module.GetValueOrDefault("kind") is not string kind || kind is not ("standard" or "document")
                || !names.Add(name.ToLowerInvariant()))
                throw new RefusedVbaException("invalid or duplicate VBA module name or kind");
            if (module.GetValueOrDefault("source") is not string source)
                throw new RefusedVbaException("invalid VBA module source");
            sources.Add((name.ToLowerInvariant(), kind, source));
        }
        if (sources.Sum(m => m.source.Length) > Limit("max_source_chars"))
            throw new RefusedVbaException("oversized VBA module source");
        var entryPoints = new HashSet<string>();
        foreach (var (name, kind, source) in sources)
        {
            var procedures = CheckSource(source, names);
            if (kind == "standard")
                entryPoints.UnionWith(procedures.Select(p => name + "." + p));
        }
        if (probe.GetValueOrDefault("calls") is not List<object> calls
            || calls.Count < 1 || calls.Count > Limit("max_calls"))
            throw new RefusedVbaException("invalid VBA call count");
        foreach (var item in calls)
        {
            if (item is not Dictionary<string, object> call || call.Count != 2
                || call.GetValueOrDefault("procedure") is not string procedure
                || !entryPoints.Contains(procedure.ToLowerInvariant()))
                throw new RefusedVbaException("VBA call must name a public standard-module procedure in the case");
            if (call.GetValueOrDefault("args") is not List<object> args || args.Count > Limit("max_args"))
                throw new RefusedVbaException("invalid VBA argument count");
            foreach (var arg in args) CheckRangeCell(arg);
        }
        CheckLayout(probe);
        return probe;
    }

    static readonly string[] Errors = { "#NULL!", "#DIV/0!", "#VALUE!", "#REF!", "#NAME?", "#NUM!", "#N/A" };
    static readonly Regex Address = new(@"\A\$?[A-Za-z]{1,3}\$?[1-9][0-9]{0,6}\z", RegexOptions.CultureInvariant);
    static readonly Regex StringLiteral = new("\"(?:[^\"]|\"\")*\"", RegexOptions.CultureInvariant);
    static readonly Regex FunctionCall = new(@"([A-Za-z_][A-Za-z0-9_.]*)\s*\(", RegexOptions.CultureInvariant);
    static readonly HashSet<string> BlockedFunctions = Words("blocked_functions").Select(w => w.ToUpperInvariant()).ToHashSet();
    static readonly HashSet<string> CatalogueFunctions = ReadCatalogue();

    static HashSet<string> ReadCatalogue()
    {
        using var resource = typeof(VbaPolicy).Assembly.GetManifestResourceStream("admission-families.tsv");
        using var reader = new StreamReader(resource);
        var names = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        while (reader.ReadLine() is string line) names.Add(line.Split('\t')[1]);
        return names;
    }

    static bool IsAddress(object value)
    {
        if (value is not string address || !Address.IsMatch(address)) return false;
        address = address.Replace("$", "");
        int i = 0;
        long column = 0;
        while (char.IsAsciiLetter(address[i])) column = column * 26 + char.ToUpperInvariant(address[i++]) - 'A' + 1;
        return column <= 16384 && long.Parse(address[i..]) <= 1048576;
    }

    static void CheckFinite(object bits)
    {
        if (bits is not string hex || hex.Length != 16 || hex.StartsWith("7ff") || hex.StartsWith("fff")
            || !hex.All(c => c is >= '0' and <= '9' or >= 'a' and <= 'f'))
            throw new RefusedVbaException("number must be finite binary64 hex, passed through a cell");
    }

    static bool HasOnly(Dictionary<string, object> cell, params string[] keys) => cell.Keys.All(keys.Contains);

    static void CheckRangeCell(object value)
    {
        if (value is not Dictionary<string, object> cell || cell.GetValueOrDefault("type") is not string kind)
            throw new RefusedVbaException("range cells must be typed");
        var mode = cell.GetValueOrDefault("mode") ?? "cell";
        bool ok = kind switch
        {
            "number" => cell.Count == 2 && cell.ContainsKey("bits"),
            "text" => HasOnly(cell, "type", "value", "mode") && Equals(mode, "cell")
                      && cell.GetValueOrDefault("value") is string text && text.Length <= 1024 && !text.Contains('\0'),
            "boolean" => HasOnly(cell, "type", "value", "mode") && Equals(mode, "cell") && cell.GetValueOrDefault("value") is bool,
            "error" => cell.Count == 2 && cell.GetValueOrDefault("code") is string code && Errors.Contains(code),
            "blank" => cell.Count == 1,
            _ => false,
        };
        if (!ok) throw new RefusedVbaException("invalid range cell");
        if (kind == "number") CheckFinite(cell["bits"]);
    }

    static void CheckLayoutFormula(string formula)
    {
        var body = StringLiteral.Replace(formula, "\"\"");
        if (body.Contains('[') || body.Contains('\\'))
            throw new RefusedVbaException("VBA layout formulas refuse external references");
        foreach (Match call in FunctionCall.Matches(body))
        {
            var name = call.Groups[1].Value.ToUpperInvariant();
            foreach (var prefix in new[] { "_XLFN.", "_XLWS." })
                if (name.StartsWith(prefix, StringComparison.Ordinal)) { name = name[prefix.Length..]; break; }
            if (BlockedFunctions.Contains(name) || !CatalogueFunctions.Contains(name))
                throw new RefusedVbaException("VBA layout formulas refuse the function " + call.Groups[1].Value);
        }
    }

    static void CheckLayout(Dictionary<string, object> probe)
    {
        if (probe.GetValueOrDefault("names") is not Dictionary<string, object> names
            || names.Values.Any(f => f is not string text || text.Length > 8192))
            throw new RefusedVbaException("invalid VBA worksheet layout");
        foreach (string formula in names.Values) CheckLayoutFormula(formula);
        if (probe.GetValueOrDefault("sheets") is not List<object> sheets || sheets.Count < 1 || sheets.Count > 16)
            throw new RefusedVbaException("invalid VBA sheet count");
        var sheetNames = new HashSet<string>();
        int cells = 0;
        foreach (var item in sheets)
        {
            if (item is not Dictionary<string, object> sheet || sheet.GetValueOrDefault("name") is not string name
                || name.Length < 1 || name.Length > 31 || !sheetNames.Add(name.ToLowerInvariant())
                || (sheet.TryGetValue("cells", out var raw) ? raw as Dictionary<string, object> : new()) is not { } layout
                || !layout.Keys.All(IsAddress))
                throw new RefusedVbaException("invalid or duplicate VBA layout sheet");
            foreach (var cell in layout.Values)
            {
                if (cell is not Dictionary<string, object> fields || fields.GetValueOrDefault("type") is not string kind
                    || kind is not ("number" or "text" or "boolean" or "error" or "formula"))
                    throw new RefusedVbaException("unsupported layout cell");
                if (kind == "formula")
                {
                    if (fields.GetValueOrDefault("formula") is not string formula || formula.Length > 8192)
                        throw new RefusedVbaException("a formula cell needs formula text");
                    CheckLayoutFormula(formula);
                }
                if (kind == "number") CheckFinite(fields.GetValueOrDefault("bits"));
                if (kind == "error" && !(fields.GetValueOrDefault("code") is string code && Errors.Contains(code)))
                    throw new RefusedVbaException("unsupported error cell");
            }
            cells += layout.Count;
        }
        if (cells > 10000) throw new RefusedVbaException("a sheet case has at most 10000 cells");
        foreach (var field in new[] { "readback", "event_log" })
        {
            if (probe.GetValueOrDefault(field) is not List<object> places || places.Count > Limit("max_reads"))
                throw new RefusedVbaException($"invalid VBA {field} count");
            foreach (var place in places)
            {
                var text = place as string ?? "";
                int bang = text.LastIndexOf('!');
                if (bang < 0 || !sheetNames.Contains(text[..bang].ToLowerInvariant()) || !IsAddress(text[(bang + 1)..]))
                    throw new RefusedVbaException($"VBA {field} must name a case sheet and one A1 cell");
            }
        }
    }

    // Includes both the bytes Excel opens and what the worker will do with them.
    public static string CaseDigest(Dictionary<string, object> probe) =>
        Convert.ToHexString(SHA256.HashData(Encoding.ASCII.GetBytes(Json.Write(Ordered(probe))))).ToLowerInvariant();

    static object Ordered(object value) => value switch
    {
        Dictionary<string, object> fields => fields.OrderBy(kv => kv.Key, StringComparer.Ordinal)
            .ToDictionary(kv => kv.Key, kv => Ordered(kv.Value)),
        List<object> values => values.Select(Ordered).ToList(),
        _ => value,
    };

    public static byte[] CheckWorkbook(Dictionary<string, object> probe, string approvals)
    {
        if (probe.GetValueOrDefault("workbook") is not string encoded || encoded.Length < 1
            || encoded.Length > Limit("max_workbook_chars"))
            throw new RefusedVbaException("invalid or oversized VBA workbook");
        byte[] bytes;
        try { bytes = Convert.FromBase64String(encoded); }
        catch (FormatException) { throw new RefusedVbaException("invalid VBA workbook encoding"); }
        if (!bytes.AsSpan().StartsWith("PK\u0003\u0004"u8))
            throw new RefusedVbaException("invalid VBA workbook encoding");
        VbaPackage.Verify(bytes, probe);
        if (approvals != null && !File.Exists(Path.Combine(approvals, CaseDigest(probe) + ".approved")))
            throw new RefusedVbaException("unapproved VBA case: the source, workbook and inputs require a local approval");
        return bytes;
    }

    static HashSet<string> CheckSource(string source, HashSet<string> moduleNames)
    {
        if (source.Length < 1 || source.Length > Limit("max_module_chars")
            || source.Any(c => (c < ' ' && c is not ('\t' or '\r' or '\n')) || c == '\x7f' || char.IsSurrogate(c)))
            throw new RefusedVbaException("invalid or oversized VBA module source");
        var tokens = Lex.Matches(source).Select(m => m.Value)
            .Where(t => t[0] is not ('\'' or ' ' or '\t'))
            .Select(t => t == "\r\n" ? "\n" : t).ToArray();
        var folded = tokens.Select(t => t.ToLowerInvariant()).ToArray();
        var procedures = new HashSet<string>();
        for (int i = 0; i + 1 < folded.Length; i++)
            if (folded[i] is "sub" or "function" && Identifier.IsMatch(tokens[i + 1])
                && (i == 0 || folded[i - 1] is not ("private" or "end")))
                procedures.Add(folded[i + 1]);
        for (int i = 0; i < folded.Length; i++)
        {
            var word = folded[i];
            if (Refused.Contains(word))
            {
                var name = word == "workbooks" && i + 2 < folded.Length && folded[i + 1] == "." && folded[i + 2] == "open"
                    ? "Workbooks.Open" : tokens[i];
                throw new RefusedVbaException("VBA construct refused: " + name);
            }
            if (word == "createobject" && (i + 3 >= folded.Length || folded[i + 1] != "("
                || !Classes.Contains(folded[i + 2]) || folded[i + 3] != ")" || i > 0 && folded[i - 1] == "."))
                throw new RefusedVbaException("VBA construct refused: CreateObject requires one listed literal class");
        }
        // A declared name is visible only in its procedure (module-level names and procedure names in the
        // whole module), and must be a VBA identifier that is not a keyword, a global or a refused name.
        var moduleDeclared = new HashSet<string>();
        var scopeOf = new HashSet<string>[tokens.Length];
        HashSet<string> current = null;
        for (int i = 0; i < folded.Length; i++)
        {
            var word = folded[i];
            var previous = i > 0 ? folded[i - 1] : "\n";
            if (word is "sub" or "function" && previous is not ("end" or "exit") && i + 1 < folded.Length)
            {
                var name = DeclaredName(tokens, i + 1);
                moduleDeclared.Add(name.ToLowerInvariant());
                current = new HashSet<string> { name.ToLowerInvariant() };
                if (i + 2 < folded.Length && folded[i + 2] == "(")
                    foreach (var parameter in Parameters(folded, i + 3))
                        current.Add(DeclaredName(tokens, Array.IndexOf(folded, parameter, i + 3)).ToLowerInvariant());
            }
            else if (word is "sub" or "function" && previous == "end")
            {
                scopeOf[i] = current;
                current = null;
                continue;
            }
            else if (word is "dim" or "const"
                || word is "private" or "public" && i + 1 < folded.Length
                    && folded[i + 1] is not ("sub" or "function" or "const" or "declare" or "dim"))
                (current ?? moduleDeclared).UnionWith(DeclarationNames(tokens, i + 1));
            scopeOf[i] = current;
        }
        for (int i = 0; i < tokens.Length; i++)
        {
            var token = tokens[i];
            var word = folded[i];
            if (token[0] == '"' && token.Length > 1 || token[0] is >= '0' and <= '9'
                || word.StartsWith("&h", StringComparison.Ordinal) || Punctuation.Contains(token))
                continue;
            bool allowed = i > 0 && folded[i - 1] == "." ? Members.Contains(word)
                : AllowedWords.Contains(word) || moduleDeclared.Contains(word)
                  || scopeOf[i] != null && scopeOf[i].Contains(word) || moduleNames.Contains(word);
            if (!allowed)
                throw new RefusedVbaException("VBA construct refused: " + token);
        }
        return procedures;
    }

    static string DeclaredName(string[] tokens, int at)
    {
        var name = tokens[at];
        var word = name.ToLowerInvariant();
        if (!Identifier.IsMatch(name) || Refused.Contains(word) || Undeclarable.Contains(word)
            || AllowedWords.Contains(word))
            throw new RefusedVbaException("VBA construct refused: " + name);
        return name;
    }

    static IEnumerable<string> DeclarationNames(string[] tokens, int start)
    {
        int depth = 0;
        bool expectName = true;
        for (int i = start; i < tokens.Length; i++)
        {
            var token = tokens[i];
            if (token is "\n" or ":" && depth == 0) yield break;
            if (token == "(") depth++;
            else if (token == ")") depth--;
            else if (depth == 0 && token == ",") expectName = true;
            else if (depth == 0 && expectName)
            {
                yield return DeclaredName(tokens, i).ToLowerInvariant();
                expectName = false;
            }
        }
    }

    static IEnumerable<string> Parameters(string[] tokens, int start)
    {
        int depth = 1;
        bool expectName = true;
        for (int i = start; i < tokens.Length; i++)
        {
            string token = tokens[i];
            if (token == "\n" || token == ")" && depth == 1) yield break;
            if (token == "(") depth++;
            else if (token == ")") depth--;
            else if (depth == 1 && token == ",") expectName = true;
            else if (depth == 1 && expectName && token is not ("byval" or "byref"))
            {
                if (Identifier.IsMatch(token)) yield return token;
                expectName = false;
            }
        }
    }
}
