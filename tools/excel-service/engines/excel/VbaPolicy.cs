using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;

namespace Touchstone.Excel;

sealed class RefusedVbaException(string message) : Exception(message);

/// <summary>
/// Decodes the VBA wire case and constructs the workbook input. The worker's offline check and
/// Excel's execution route use this decoder.
/// </summary>
static class VbaPolicy
{
    static readonly Dictionary<string, object> Rules = ReadRules();
    public static Dictionary<string, object> RuleTable => Rules;
    internal static readonly Regex SheetName = new(@"\A" + Rules["sheet_name"] + @"\z", RegexOptions.CultureInvariant);
    static readonly string FormulaStart = (string)Rules["formula_chars"];
    const int FrameBudget = 40 * 1024;     // touchstone.protocol.FRAME_BUDGET: one VBA case is one group of one call

    static Dictionary<string, object> ReadRules()
    {
        using var resource = typeof(VbaPolicy).Assembly.GetManifestResourceStream("vba-probe-policy.json");
        using var reader = new StreamReader(resource);
        return (Dictionary<string, object>)Json.Parse(reader.ReadToEnd());
    }

    public static int Limit(string key) => (int)(long)Rules[key];

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
        bool offset = group.TryGetValue("o", out var o);
        bool indexed = group.TryGetValue("i", out var i);
        if (offset == indexed || (offset ? o is not 0L : i is not List<object> { Count: 1 } positions || positions[0] is not 0L))
            throw new RefusedVbaException("VBA position must be o: 0 or i: [0]");
        return (group, probe);
    }

    /// <summary>Offline test seam (--check-vba-expanded, --build-vba-expanded): checks a case that carries modules, so the grammar
    /// can be exercised with mutated sources. The Excel route (CheckGroup) never calls it and refuses such a case.</summary>
    public static VbaCase CheckExpandedGroup(object value)
    {
        var (group, probe) = Shape(value);
        return VbaCase.FromChecked(CheckExpanded(group, probe));
    }

    public static VbaCase CheckGroup(object value)
    {
        var (group, probe) = Shape(value);
        // Only a reviewed catalogue template with typed parameters reaches Excel; free module source is refused (dc-45ndr).
        if (!probe.ContainsKey("params") || probe.GetValueOrDefault("template") is not string template)
            throw new RefusedVbaException("a VBA case names a reviewed template with typed params; VBA source is not accepted");
        if (Json.Write(group).Length > FrameBudget)
            throw new RefusedVbaException("a VBA case must fit one submit frame (the 40 KiB frame budget)");
        VbaTemplateAdmission.RequireEmbedded();
        var expanded = VbaTemplates.Expand(probe);
        var sheets = SheetList(expanded);
        // Checked is used (dc-heiq6): the grammar reads the rendered modules, the text the package is built from, on
        // every request. Admission read the template with each placeholder as one literal; this reads what Excel compiles.
        var modules = ((List<object>)expanded["modules"]).Cast<Dictionary<string, object>>()
            .Select(m => ((string)m["name"], (string)m["kind"], (string)m["source"])).ToList();
        var modulesChecked = VbaGrammar.CheckModules(modules, sheets.Count, null, VbaTemplates.EntryPoints(template));
        CheckCalls(expanded, VbaTemplates.EntryPoints(template));
        CheckLayout(expanded, sheets);
        VbaTemplates.ApplyLayout(expanded, template, (Dictionary<string, object>)probe["params"], modulesChecked);
        expanded["template"] = template;
        expanded["params"] = probe["params"];
        expanded["pins"] = VbaTemplates.Pins(template);
        return VbaCase.FromChecked(expanded, VbaTemplates.Review(template));
    }

    static Dictionary<string, object> CheckExpanded(Dictionary<string, object> group, Dictionary<string, object> probe)
    {
        // No workbook travels with the case: the worker builds the package from the modules it checked.
        if (probe.Count != 6 || !new[] { "sheets", "names", "modules", "calls", "readback", "event_log" }.All(probe.ContainsKey))
            throw new RefusedVbaException("invalid VBA case fields");
        if (Json.Write(group).Length > FrameBudget)
            throw new RefusedVbaException("a VBA case must fit one submit frame (the 40 KiB frame budget)");
        if (probe["modules"] is not List<object> modules || modules.Count < 1 || modules.Count > Limit("max_modules"))
            throw new RefusedVbaException("invalid VBA module count");
        var sheets = SheetList(probe);
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
        var entryPoints = VbaGrammar.CheckModules(triples, sheets.Count).EntryPoints;
        CheckCalls(probe, null, entryPoints);
        CheckLayout(probe, sheets);
        return probe;
    }

    static List<object> SheetList(Dictionary<string, object> probe)
    {
        if (probe["sheets"] is not List<object> sheets || sheets.Count < 1 || sheets.Count > 16)
            throw new RefusedVbaException("invalid VBA sheet count");
        return sheets;
    }

    static void CheckCalls(Dictionary<string, object> probe, IReadOnlyDictionary<string, string[]> signatures, HashSet<string> offlineEntries = null)
    {
        if (probe["calls"] is not List<object> calls || calls.Count < 1 || calls.Count > Limit("max_calls"))
            throw new RefusedVbaException("invalid VBA call count");
        foreach (var item in calls)
        {
            if (item is not Dictionary<string, object> call || call.Count != 2
                || call.GetValueOrDefault("procedure") is not string procedure)
                throw new RefusedVbaException("invalid VBA call fields");
            string[] signature = null;
            if (signatures != null && !signatures.TryGetValue(procedure, out signature))
                throw new RefusedVbaException("VBA call must name an entry point of the template");
            if (offlineEntries != null && !offlineEntries.Contains(procedure.ToLowerInvariant()))
                throw new RefusedVbaException("VBA call must name a public standard-module procedure in the case");
            if (call.GetValueOrDefault("args") is not List<object> args || args.Count > Limit("max_args"))
                throw new RefusedVbaException("invalid VBA argument count");
            if (signature != null && signature.Length != args.Count)
                throw new RefusedVbaException("VBA arguments must match the template entry point signature");
            for (int i = 0; i < args.Count; i++)
            {
                var arg = args[i];
                CheckRangeCell(arg);
                if (signature != null && !Equals(((Dictionary<string, object>)arg)["type"], signature[i]))
                    throw new RefusedVbaException("VBA arguments must match the template entry point signature");
                // Case text reaches cells through the probe's own writes: it must be data, never a formula.
                if (arg is Dictionary<string, object> cell && Equals(cell.GetValueOrDefault("type"), "text")
                    && cell.GetValueOrDefault("value") is string text && text.Length > 0 && FormulaStart.Contains(text[0]))
                    throw new RefusedVbaException("VBA text arguments must not start with = + - or @");
            }
        }
    }

    static readonly string[] Errors = { "#NULL!", "#DIV/0!", "#VALUE!", "#REF!", "#NAME?", "#NUM!", "#N/A" };
    static readonly Regex Address = new(@"\A\$?[A-Za-z]{1,3}\$?[1-9][0-9]{0,6}\z", RegexOptions.CultureInvariant);
    internal static bool IsAddress(object value)
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

    static void CheckRangeCell(object value, int textLimit = 1024)
    {
        if (value is not Dictionary<string, object> cell || cell.GetValueOrDefault("type") is not string kind)
            throw new RefusedVbaException("range cells must be typed");
        var mode = cell.TryGetValue("mode", out var given) ? given : "cell";
        bool ok = kind switch
        {
            "number" => cell.Count == 2 && cell.ContainsKey("bits"),
            "text" => HasOnly(cell, "type", "value", "mode") && Equals(mode, "cell")
                      && cell.GetValueOrDefault("value") is string text && text.Length <= textLimit && !text.Contains('\0'),
            "boolean" => HasOnly(cell, "type", "value", "mode") && Equals(mode, "cell") && cell.GetValueOrDefault("value") is bool,
            "error" => cell.Count == 2 && cell.GetValueOrDefault("code") is string code && Errors.Contains(code),
            "blank" => cell.Count == 1,
            _ => false,
        };
        if (!ok) throw new RefusedVbaException("invalid range cell");
        if (kind == "number") CheckFinite(cell["bits"]);
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
                || !layout.Keys.All(IsAddress)
                || layout.Keys.Select(at => at.Replace("$", "")).Distinct(StringComparer.OrdinalIgnoreCase).Count() != layout.Count)
                throw new RefusedVbaException("invalid or duplicate VBA layout sheet");
        }
        if (probe["names"] is not Dictionary<string, object> { Count: 0 })
            throw new RefusedVbaException("VBA request layout cannot carry names");
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
                try { CheckRangeCell(cell, 32767); }
                catch (RefusedVbaException)
                {
                    var kind = (cell as Dictionary<string, object>)?.GetValueOrDefault("type") as string;
                    throw new RefusedVbaException($"invalid VBA layout {kind} (layout cell must be a value)");
                }
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
                if (bang < 0 || !text[..bang].All(c => c < 128) || !sheetNames.Contains(text[..bang].ToLowerInvariant()) || !IsAddress(text[(bang + 1)..]))
                    throw new RefusedVbaException($"VBA {field} must name a case sheet and one A1 cell");
            }
        }
    }

    // Covers the modules, layout, calls and readback; the worker builds the package from the modules.
    public static string CaseDigest(VbaCase probe) => Digest(probe.Evidence());

    internal static string Digest(object value) =>
        Convert.ToHexString(SHA256.HashData(Encoding.ASCII.GetBytes(Json.Write(Ordered(value))))).ToLowerInvariant();

    static object Ordered(object value) => value switch
    {
        Dictionary<string, object> fields => fields.OrderBy(kv => kv.Key, StringComparer.Ordinal)
            .ToDictionary(kv => kv.Key, kv => Ordered(kv.Value)),
        List<object> values => values.Select(Ordered).ToList(),
        _ => value,
    };

    public static void CheckApproval(VbaCase probe, string approvals, string excelFileVersion = null)
    {
        if (approvals == null) return;
        var review = probe.Review;
        if (review == null || review.Reviewer == null || review.ExcelBuildFamily == null)
            throw new RefusedVbaException("unapproved VBA template: template review is pending");
        if (excelFileVersion != null && !excelFileVersion.StartsWith(review.ExcelBuildFamily + ".", StringComparison.Ordinal))
            throw new RefusedVbaException("unapproved VBA template for this Excel build family");
        if (!File.Exists(Path.Combine(approvals, review.Pin + ".approved")))
            throw new RefusedVbaException("unapproved VBA template pin");
    }
}
