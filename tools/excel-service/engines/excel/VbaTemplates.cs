using System.Security.Cryptography;
using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;

namespace Touchstone.Excel;

/// <summary>
/// The catalogue of reviewed VBA probe templates (dc-erfm3); same algorithm and files as tools/excel-service/vba_templates.py.
/// A VBA case names a template in vba-probes/catalogue.json and gives typed parameters. Each template module file is pinned
/// by SHA-256, and the package is built from the pinned text and the parameters only.
/// </summary>
static class VbaTemplates
{
    static readonly Dictionary<string, object> Catalogue = (Dictionary<string, object>)Json.Parse(Resource("vba-probes/catalogue.json"));
    static readonly Regex Placeholder = new(@"\{\{([a-z][a-z_]*)\}\}", RegexOptions.CultureInvariant);
    static readonly Regex SheetName = new(@"\A[A-Za-z0-9_][A-Za-z0-9_ ]{0,30}\z", RegexOptions.CultureInvariant);
    static readonly Regex Address = new(@"\A([A-Z]{1,3})([1-9][0-9]{0,6})\z", RegexOptions.CultureInvariant);

    static byte[] ResourceBytes(string name)
    {
        using var stream = typeof(VbaTemplates).Assembly.GetManifestResourceStream(name)
            ?? throw new RefusedVbaException("VBA template file is missing: " + name);
        using var copy = new MemoryStream();
        stream.CopyTo(copy);
        return copy.ToArray();
    }

    static string Resource(string name) => Encoding.UTF8.GetString(ResourceBytes(name));

    static long Limit(string key) => (long)((Dictionary<string, object>)Catalogue["limits"])[key];

    static string TemplateSource(Dictionary<string, object> entry)
    {
        var file = (string)entry["file"];
        var data = ResourceBytes("vba-probes/" + file);          // the raw stream bytes are what the manifest pins
        if (Convert.ToHexString(SHA256.HashData(data)).ToLowerInvariant() != (string)entry["sha256"])
            throw new RefusedVbaException("VBA template file differs from its pinned hash: " + file);
        try
        {
            return Encoding.GetEncoding("us-ascii", EncoderFallback.ExceptionFallback, DecoderFallback.ExceptionFallback).GetString(data);
        }
        catch (DecoderFallbackException)
        {
            throw new RefusedVbaException("VBA template file is not ASCII: " + file);
        }
    }

    static Dictionary<string, object> Template(string name)
    {
        var templates = (Dictionary<string, object>)Catalogue["templates"];
        return templates.TryGetValue(name, out var entry) ? (Dictionary<string, object>)entry
            : throw new RefusedVbaException("unknown VBA probe template");
    }

    public static List<object> Pins(string template)
    {
        var entry = Template(template);
        return ((List<object>)entry["modules"]).Append(entry["layout"])
            .Select(file => ((Dictionary<string, object>)file)["sha256"]).ToList();
    }

    static bool AddressOk(object value)
    {
        if (value is not string text || Address.Match(text) is not { Success: true } m) return false;
        long column = 0;
        foreach (var ch in m.Groups[1].Value) column = column * 26 + ch - 64;
        return column <= 16384 && long.Parse(m.Groups[2].Value) <= 1048576;
    }

    static string Quote(string text) => "\"" + text.Replace("\"", "\"\"") + "\"";

    static string CaseSheet(object value, Dictionary<string, string> sheetNames)
    {
        if (value is not string sheet || !SheetName.IsMatch(sheet)
            || !sheetNames.TryGetValue(sheet, out var canonical))
            throw new RefusedVbaException("VBA template parameter must name a case sheet");
        return canonical;
    }

    static string Render(string kind, object value, Dictionary<string, string> sheetNames, bool formula = false)
    {
        switch (kind)
        {
            case "sheet":
                var sheet = CaseSheet(value, sheetNames);
                return formula ? "'" + sheet + "'" : Quote(sheet);
            case "address":
                if (!AddressOk(value)) throw new RefusedVbaException("VBA template parameter must be one A1 cell");
                return Quote((string)value);
            case "text":
                if (value is not string text || text.Length < 1 || text.Length > Limit("text")
                    || text.Any(c => c < ' ' || c > '~') || "=+-@".Contains(text[0]))
                    throw new RefusedVbaException("VBA template text parameter must be 1..255 printable ASCII characters not starting with = + - or @");
                return Quote(text);
            case "long":
                if (value is not long n || n < Limit("long_min") || n > Limit("long_max"))
                    throw new RefusedVbaException("VBA template parameter must be a Long integer");
                var number = n.ToString(CultureInfo.InvariantCulture);
                return n >= 0 ? number : "(" + number + ")";
            default:
                if (kind.StartsWith("enum:", StringComparison.Ordinal))
                {
                    if (value is not string word || !kind[5..].Split('|').Contains(word))
                        throw new RefusedVbaException("VBA template parameter must be one of the listed words");
                    return formula ? Quote(word) : word;
                }
                throw new RefusedVbaException("unknown VBA template parameter type");
        }
    }

    static Dictionary<string, string> CaseSheets(List<object> sheets)
    {
        var names = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        foreach (Dictionary<string, object> sheet in sheets)
        {
            var name = (string)sheet["name"];
            if (SheetName.IsMatch(name)) names.TryAdd(name, name);
        }
        return names;
    }

    static string Substitute(string source, Dictionary<string, string> rendered) =>
        Placeholder.Replace(source, m => rendered.TryGetValue(m.Groups[1].Value, out var text)
            ? text : throw new RefusedVbaException("VBA template uses an undeclared parameter: " + m.Groups[1].Value));

    public static void ApplyLayout(Dictionary<string, object> expanded, string template, Dictionary<string, object> given)
    {
        var entry = Template(template);
        var sheetList = (List<object>)expanded["sheets"];
        var sheetNames = CaseSheets(sheetList);
        var sheets = sheetList.Cast<Dictionary<string, object>>()
            .Select(s => new Dictionary<string, object>(s)).ToDictionary(s => (string)s["name"], StringComparer.Ordinal);
        var names = (Dictionary<string, object>)expanded["names"];
        var declared = (Dictionary<string, object>)entry["params"];
        var rendered = declared.ToDictionary(p => p.Key, p => Render((string)p.Value, given[p.Key], sheetNames, formula: true));
        var layout = (List<object>)Json.Parse(TemplateSource((Dictionary<string, object>)entry["layout"]));
        foreach (Dictionary<string, object> item in layout)
        {
            if (item.Count == 2 && item.GetValueOrDefault("name") is string name && item.GetValueOrDefault("refers_to") is string refersTo
                && !name.StartsWith("auto_", StringComparison.OrdinalIgnoreCase) && Regex.IsMatch(name, @"\A[A-Za-z][A-Za-z0-9_]{0,30}\z"))
            {
                if (!names.TryAdd(name, Formula(refersTo, rendered)))
                    throw new RefusedVbaException("duplicate VBA template defined name");
                continue;
            }
            if (item.Count != 3 || item.GetValueOrDefault("sheet") is not string target || item.GetValueOrDefault("at") is not string at
                || !VbaPolicy.IsAddress(at) || item.GetValueOrDefault("formula") is not string formula)
                throw new RefusedVbaException("invalid VBA template layout entry");
            var placeholder = Placeholder.Match(target);
            if (placeholder.Success && placeholder.Length == target.Length
                && declared.GetValueOrDefault(placeholder.Groups[1].Value) is "sheet")
                target = CaseSheet(given[placeholder.Groups[1].Value], sheetNames);
            var sheet = sheets[CaseSheet(target, sheetNames)];
            var cells = sheet.GetValueOrDefault("cells") as Dictionary<string, object> ?? new();
            if (cells.Keys.Any(key => string.Equals(key.Replace("$", ""), at.Replace("$", ""), StringComparison.OrdinalIgnoreCase)))
                throw new RefusedVbaException("VBA template formula cell overlaps a request cell");
            sheet["cells"] = new Dictionary<string, object>(cells)
            {
                [at] = new Dictionary<string, object> { ["type"] = "formula", ["formula"] = Formula(formula, rendered) },
            };
        }
        expanded["sheets"] = sheetList.Cast<Dictionary<string, object>>().Select(s => (object)sheets[(string)s["name"]]).ToList();
    }

    static string Formula(string source, Dictionary<string, string> rendered)
    {
        var text = Substitute(source, rendered);
        if (text.Length > 8192 || !text.StartsWith('='))
            throw new RefusedVbaException("invalid VBA template layout formula");
        return text;
    }

    /// <summary>A template case becomes the same case with the modules the template builds.</summary>
    public static Dictionary<string, object> Expand(Dictionary<string, object> probe)
    {
        string[] required = { "sheets", "template", "params", "calls", "readback", "event_log" };
        if (probe.ContainsKey("names"))
            throw new RefusedVbaException("VBA request layout cannot carry names");
        if (probe.Count != required.Length || !required.All(probe.ContainsKey))
            throw new RefusedVbaException("invalid VBA case fields");
        if (probe["template"] is not string name)
            throw new RefusedVbaException("unknown VBA probe template");
        var entry = Template(name);
        var declared = (Dictionary<string, object>)entry["params"];
        if (probe["params"] is not Dictionary<string, object> given || given.Count != declared.Count || !declared.Keys.All(given.ContainsKey))
            throw new RefusedVbaException("VBA template parameters do not match the template");
        if (probe["sheets"] is not List<object> sheets
            || sheets.Any(s => s is not Dictionary<string, object> d || d.GetValueOrDefault("name") is not string))
            throw new RefusedVbaException("invalid VBA sheet count");
        var sheetNames = CaseSheets(sheets);
        var rendered = declared.ToDictionary(p => p.Key, p => Render((string)p.Value, given[p.Key], sheetNames));
        var modules = new List<object>();
        foreach (var item in (List<object>)entry["modules"])
        {
            var module = (Dictionary<string, object>)item;
            var source = Substitute(TemplateSource(module), rendered);
            modules.Add(new Dictionary<string, object> { ["name"] = module["name"], ["kind"] = module["kind"], ["source"] = source });
        }
        var entryPoints = ((List<object>)entry["entry_points"]).Cast<string>().ToHashSet();
        if (probe["calls"] is not List<object> calls
            || calls.Any(c => c is not Dictionary<string, object> d || d.GetValueOrDefault("procedure") is not string p || !entryPoints.Contains(p)))
            throw new RefusedVbaException("a VBA call must name an entry point of the template");
        return new Dictionary<string, object>
        {
            ["sheets"] = probe["sheets"],
            ["names"] = new Dictionary<string, object>(StringComparer.OrdinalIgnoreCase),
            ["modules"] = modules,
            ["calls"] = probe["calls"],
            ["readback"] = probe["readback"],
            ["event_log"] = probe["event_log"],
        };
    }
}
