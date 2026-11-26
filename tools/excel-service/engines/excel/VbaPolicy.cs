using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;

namespace Touchstone.Excel;

sealed class RefusedVbaException(string message) : Exception(message);

/// <summary>
/// Checks an authored VBA case before it can be handed to Excel: the probe source against the positive grammar
/// (VbaGrammar), the layout, the calls and the readback addresses. Same checks, in the same order, as
/// tools/excel-service/policy.py (_check_vba_case); both read vba-probe-policy.json.
/// </summary>
static class VbaPolicy
{
    static readonly Dictionary<string, object> Rules = ReadRules();
    public static Dictionary<string, object> RuleTable => Rules;
    static readonly Regex SheetName = new(@"\A" + Rules["sheet_name"] + @"\z", RegexOptions.CultureInvariant);
    static readonly Regex DefinedName = new(@"\A" + Rules["defined_name"] + @"\z", RegexOptions.CultureInvariant);
    static readonly string FormulaStart = (string)Rules["formula_chars"];
    const int FrameBudget = 40 * 1024;     // touchstone.protocol.FRAME_BUDGET: one VBA case is one group of one call

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

    static (Dictionary<string, object> group, Dictionary<string, object> probe) Shape(object value)
    {
        if (value is not Dictionary<string, object> group
            || !Equals(group.GetValueOrDefault("f"), "_VBA") || !Equals(group.GetValueOrDefault("k"), "V")
            || !Equals(group.GetValueOrDefault("n"), 1L)
            || group.GetValueOrDefault("v") is not List<object> values || values.Count != 1
            || group.Keys.Any(k => k is not ("f" or "k" or "n" or "v" or "o" or "i"))
            || values[0] is not Dictionary<string, object> probe)
            throw new RefusedVbaException("a VBA group contains one VBA case");
        return (group, probe);
    }

    /// <summary>Offline test seam (--check-vba-expanded, --build-vba-expanded): checks a case that carries modules, so the grammar
    /// can be exercised with mutated sources. The Excel route (CheckGroup) never calls it and refuses such a case.</summary>
    public static Dictionary<string, object> CheckExpandedGroup(object value)
    {
        var (group, probe) = Shape(value);
        return CheckExpanded(group, probe);
    }

    public static Dictionary<string, object> CheckGroup(object value)
    {
        var (group, probe) = Shape(value);
        // Only a reviewed catalogue template with typed parameters reaches Excel; free module source is refused (dc-45ndr).
        if (!probe.ContainsKey("params") || probe.GetValueOrDefault("template") is not string template)
            throw new RefusedVbaException("a VBA case names a reviewed template with typed params; VBA source is not accepted");
        if (Json.Write(group).Length > FrameBudget)
            throw new RefusedVbaException("a VBA case must fit one submit frame (the 40 KiB frame budget)");
        var expanded = CheckExpanded(group, VbaTemplates.Expand(probe));
        // The approval digest binds the template, its typed parameters and the pinned module hashes as well as the expansion.
        expanded["template"] = template;
        expanded["params"] = probe["params"];
        expanded["pins"] = VbaTemplates.Pins(template);
        return expanded;
    }

    /// <summary>The checks every expanded case passes. The wire reaches it only through a template expansion.</summary>
    internal static Dictionary<string, object> CheckExpanded(Dictionary<string, object> group, Dictionary<string, object> probe)
    {
        // No workbook travels with the case: the worker builds the package from the modules it checked.
        if (probe.Count != 6 || !new[] { "sheets", "names", "modules", "calls", "readback", "event_log" }.All(probe.ContainsKey))
            throw new RefusedVbaException("invalid VBA case fields");
        if (Json.Write(group).Length > FrameBudget)
            throw new RefusedVbaException("a VBA case must fit one submit frame (the 40 KiB frame budget)");
        if (probe["modules"] is not List<object> modules || modules.Count < 1 || modules.Count > Limit("max_modules"))
            throw new RefusedVbaException("invalid VBA module count");
        if (probe["sheets"] is not List<object> sheets || sheets.Count < 1 || sheets.Count > 16)
            throw new RefusedVbaException("invalid VBA sheet count");
        var triples = new List<(string name, string kind, string source)>();
        int sourceChars = 0;
        foreach (var item in modules)
        {
            if (item is not Dictionary<string, object> module || module.Count != 3
                || !module.ContainsKey("name") || !module.ContainsKey("kind") || !module.ContainsKey("source"))
                throw new RefusedVbaException("invalid VBA module fields");
            if (module["name"] is not string name || module["kind"] is not string kind || kind is not ("standard" or "document"))
                throw new RefusedVbaException("invalid or duplicate VBA module name or kind");
            if (module["source"] is not string source || source.Length < 1 || source.Length > Limit("max_module_chars"))
                throw new RefusedVbaException("invalid or oversized VBA module source");
            foreach (char c in source)
                if (c < ' ' && c is not ('\t' or '\r' or '\n') || c > '~')
                    throw new RefusedVbaException($"VBA construct refused: {VbaGrammar.Escape(c.ToString())} (a control or non-ASCII character)");
            sourceChars += source.Length;
            triples.Add((name, kind, source));
        }
        if (sourceChars > Limit("max_source_chars"))
            throw new RefusedVbaException("oversized VBA module source");
        var (entryPoints, procedures) = VbaGrammar.CheckModules(triples, sheets.Count);
        foreach (var entry in procedures)
        {
            // A layout formula may call only worksheet functions; a probe function with such a name (Public or
            // Private, alone or after its module name) would answer it on an Excel build that lacks the function,
            // with a parameter the taint check never saw.
            foreach (var part in new[] { entry[(entry.LastIndexOf('.') + 1)..], entry })
            {
                var name = part.ToUpperInvariant();
                if (CatalogueFunctions.Contains(name) || BlockedFunctions.Contains(name) || BlockedPrefixes.Any(p => name.StartsWith(p, StringComparison.Ordinal)))
                    throw new RefusedVbaException("a VBA procedure name collides with a worksheet function: " + VbaGrammar.Escape(name));
            }
        }
        if (probe["calls"] is not List<object> calls || calls.Count < 1 || calls.Count > Limit("max_calls"))
            throw new RefusedVbaException("invalid VBA call count");
        foreach (var item in calls)
        {
            if (item is not Dictionary<string, object> call || call.Count != 2
                || call.GetValueOrDefault("procedure") is not string procedure
                || !entryPoints.Contains(procedure.ToLowerInvariant()))
                throw new RefusedVbaException("VBA call must name a public standard-module procedure in the case");
            if (call.GetValueOrDefault("args") is not List<object> args || args.Count > Limit("max_args"))
                throw new RefusedVbaException("invalid VBA argument count");
            foreach (var arg in args)
            {
                CheckRangeCell(arg);
                // Case text reaches cells through the probe's own writes: it must be data, never a formula.
                if (arg is Dictionary<string, object> cell && Equals(cell.GetValueOrDefault("type"), "text")
                    && cell.GetValueOrDefault("value") is string text && text.Length > 0 && FormulaStart.Contains(text[0]))
                    throw new RefusedVbaException("VBA text arguments must not start with = + - or @");
            }
        }
        CheckLayout(probe, sheets);
        return probe;
    }

    static readonly string[] Errors = { "#NULL!", "#DIV/0!", "#VALUE!", "#REF!", "#NAME?", "#NUM!", "#N/A" };
    static readonly Regex Address = new(@"\A\$?[A-Za-z]{1,3}\$?[1-9][0-9]{0,6}\z", RegexOptions.CultureInvariant);
    static readonly Regex FormulaToken = new("\\G(?:'(?:[^']|'')*'|\"(?:[^\"]|\"\")*\")", RegexOptions.CultureInvariant);
    static readonly Regex FunctionCall = new(@"([A-Za-z_][A-Za-z0-9_.]*)\s*\(", RegexOptions.CultureInvariant);
    static readonly HashSet<string> BlockedFunctions = Words("blocked_functions").Select(w => w.ToUpperInvariant()).ToHashSet();
    static readonly string[] BlockedPrefixes = Words("blocked_function_prefixes").Select(w => w.ToUpperInvariant()).ToArray();
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
        var mode = cell.TryGetValue("mode", out var given) ? given : "cell";
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

    /// <summary>The code of a layout formula with quoted sheet names and string literals set aside.</summary>
    static string LayoutScan(string formula)
    {
        var code = new StringBuilder();
        int at = 0;
        while (at < formula.Length)
        {
            int a = formula.IndexOf('\'', at), b = formula.IndexOf('"', at);
            int quote = a < 0 ? b : b < 0 ? a : Math.Min(a, b);
            if (quote < 0) { code.Append(formula, at, formula.Length - at); break; }
            code.Append(formula, at, quote - at);
            var match = FormulaToken.Match(formula, quote);
            if (!match.Success) throw new RefusedVbaException("VBA layout formulas refuse an unterminated quote");
            if (match.Value[0] == '\'')
            {
                if (!SheetName.IsMatch(match.Value[1..^1]))
                    throw new RefusedVbaException("VBA layout formulas refuse a quoted name that is not a plain sheet name");
                code.Append("Q!");
                at = match.Index + match.Length;
                if (at >= formula.Length || formula[at] != '!')
                    throw new RefusedVbaException("VBA layout formulas refuse a quoted name that is not a sheet reference");
                at++;
            }
            else
            {
                code.Append("\"\"");
                at = match.Index + match.Length;
            }
        }
        return code.ToString();
    }

    static void CheckLayoutFormula(string formula)
    {
        var body = LayoutScan(formula);
        if (body.IndexOfAny(new[] { '[', ']', '\\', '|' }) >= 0)
            throw new RefusedVbaException("VBA layout formulas refuse external references");
        foreach (Match call in FunctionCall.Matches(body))
        {
            var name = call.Groups[1].Value.ToUpperInvariant();
            while (name.StartsWith("_XLFN.", StringComparison.Ordinal) || name.StartsWith("_XLWS.", StringComparison.Ordinal)) name = name[6..];
            if (BlockedFunctions.Contains(name) || BlockedPrefixes.Any(p => name.StartsWith(p, StringComparison.Ordinal))
                || !CatalogueFunctions.Contains(name))
                throw new RefusedVbaException("VBA layout formulas refuse the function " + call.Groups[1].Value);
        }
    }

    static void CheckLayout(Dictionary<string, object> probe, List<object> sheets)
    {
        var sheetNames = new HashSet<string>();
        foreach (var item in sheets)
        {
            if (item is not Dictionary<string, object> sheet || sheet.GetValueOrDefault("name") is not string name
                || !SheetName.IsMatch(name) || !sheetNames.Add(name.ToLowerInvariant())
                || (sheet.TryGetValue("cells", out var raw) ? raw as Dictionary<string, object> : new()) is not { } layout
                || sheet.Keys.Any(k => k is not ("name" or "cells" or "hidden_rows" or "hidden_columns"))
                || !layout.Keys.All(IsAddress))
                throw new RefusedVbaException("invalid or duplicate VBA layout sheet");
        }
        if (probe["names"] is not Dictionary<string, object> names
            || names.Values.Any(f => f is not string text || text.Length > 8192))
            throw new RefusedVbaException("invalid VBA worksheet layout");
        int cells = 0;
        foreach (Dictionary<string, object> sheet in sheets)
        {
            foreach (var (field, limit) in new[] { ("hidden_rows", 1048576L), ("hidden_columns", 16384L) })
            {
                if (sheet.TryGetValue(field, out var raw)
                    && (raw is not List<object> hidden || hidden.Count > 1000 || hidden.Any(n => n is not long v || v < 1 || v > limit)))
                    throw new RefusedVbaException($"{field} are at most 1000 positions inside the worksheet");
            }
            var layout = sheet.TryGetValue("cells", out var rawCells) ? (Dictionary<string, object>)rawCells : new();
            foreach (var cell in layout.Values)
            {
                if (cell is not Dictionary<string, object> fields || fields.GetValueOrDefault("type") is not string kind
                    || kind is not ("number" or "text" or "boolean" or "error" or "formula"))
                    throw new RefusedVbaException("unsupported layout cell");
                if (kind == "formula" && !(fields.GetValueOrDefault("formula") is string f && f.Length <= 8192))
                    throw new RefusedVbaException("a formula cell needs formula text");
                if (kind == "number") CheckFinite(fields.GetValueOrDefault("bits"));
                if (kind == "error" && !(fields.GetValueOrDefault("code") is string code && Errors.Contains(code)))
                    throw new RefusedVbaException("unsupported error cell");
            }
            cells += layout.Count;
        }
        if (cells > 10000) throw new RefusedVbaException("a sheet case has at most 10000 cells");
        foreach (Dictionary<string, object> sheet in sheets)
            if (sheet.TryGetValue("cells", out var rawCells))
                foreach (Dictionary<string, object> fields in ((Dictionary<string, object>)rawCells).Values)
                {
                    var kind = (string)fields["type"];
                    if (kind == "formula") CheckLayoutFormula((string)fields["formula"]);
                    else if (kind == "boolean" && fields.GetValueOrDefault("value") is not bool)
                        throw new RefusedVbaException("invalid VBA layout boolean");
                    else if (kind == "text" && !(fields.GetValueOrDefault("value") is string value && !value.Contains('\0')))
                        throw new RefusedVbaException("invalid VBA layout text");
                }
        foreach (var (key, formula) in names)
        {
            if (!DefinedName.IsMatch(key) || key.StartsWith("auto_", StringComparison.OrdinalIgnoreCase))
                throw new RefusedVbaException("invalid VBA defined name");
            CheckLayoutFormula((string)formula);
        }
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

    // Covers the modules, layout, calls and readback; the worker builds the package from the modules.
    public static string CaseDigest(Dictionary<string, object> probe) =>
        Convert.ToHexString(SHA256.HashData(Encoding.ASCII.GetBytes(Json.Write(Ordered(probe))))).ToLowerInvariant();

    static object Ordered(object value) => value switch
    {
        Dictionary<string, object> fields => fields.OrderBy(kv => kv.Key, StringComparer.Ordinal)
            .ToDictionary(kv => kv.Key, kv => Ordered(kv.Value)),
        List<object> values => values.Select(Ordered).ToList(),
        _ => value,
    };

    /// <summary>Refuses a case whose digest has no approval file (when an approvals folder is given).</summary>
    public static void CheckApproval(Dictionary<string, object> probe, string approvals)
    {
        if (approvals != null && !File.Exists(Path.Combine(approvals, CaseDigest(probe) + ".approved")))
            throw new RefusedVbaException("unapproved VBA case: the source, inputs and layout require a local approval");
    }
}
