using System.Security.Cryptography;
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
    static readonly Regex Address = new(@"\A([A-Z]{1,3})([1-9][0-9]{0,6})\z", RegexOptions.CultureInvariant);

    static string Resource(string name)
    {
        using var stream = typeof(VbaTemplates).Assembly.GetManifestResourceStream(name)
            ?? throw new RefusedVbaException("VBA template file is missing: " + name);
        using var reader = new StreamReader(stream, new UTF8Encoding(false));
        return reader.ReadToEnd();
    }

    static long Limit(string key) => (long)((Dictionary<string, object>)Catalogue["limits"])[key];

    static string TemplateSource(Dictionary<string, object> entry)
    {
        var file = (string)entry["file"];
        var text = Resource("vba-probes/" + file);
        if (Convert.ToHexString(SHA256.HashData(Encoding.ASCII.GetBytes(text))).ToLowerInvariant() != (string)entry["sha256"])
            throw new RefusedVbaException("VBA template file differs from its pinned hash: " + file);
        return text;
    }

    /// <summary>The pinned SHA-256 of each module file of a template, in catalogue order (bound into the case digest).</summary>
    public static List<object> Pins(string template) =>
        ((List<object>)((Dictionary<string, object>)((Dictionary<string, object>)Catalogue["templates"])[template])["modules"])
        .Select(m => (object)(string)((Dictionary<string, object>)m)["sha256"]).ToList();

    static bool AddressOk(object value)
    {
        if (value is not string text || Address.Match(text) is not { Success: true } m) return false;
        long column = 0;
        foreach (var ch in m.Groups[1].Value) column = column * 26 + ch - 64;
        return column <= 16384 && long.Parse(m.Groups[2].Value) <= 1048576;
    }

    static string Render(string kind, object value, HashSet<string> sheetNames)
    {
        switch (kind)
        {
            case "sheet":
                if (value is not string sheet || !sheetNames.Contains(sheet.ToLowerInvariant()))
                    throw new RefusedVbaException("VBA template parameter must name a case sheet");
                return "\"" + sheet + "\"";
            case "address":
                if (!AddressOk(value)) throw new RefusedVbaException("VBA template parameter must be one A1 cell");
                return "\"" + (string)value + "\"";
            case "text":
                if (value is not string text || text.Length < 1 || text.Length > Limit("text")
                    || text.Any(c => c < ' ' || c > '~') || "=+-@".Contains(text[0]))
                    throw new RefusedVbaException("VBA template text parameter must be 1..255 printable ASCII characters not starting with = + - or @");
                return "\"" + text.Replace("\"", "\"\"") + "\"";
            case "long":
                if (value is not long n || n < Limit("long_min") || n > Limit("long_max"))
                    throw new RefusedVbaException("VBA template parameter must be a Long integer");
                return n >= 0 ? n.ToString() : "(" + n + ")";
            default:
                if (kind.StartsWith("enum:", StringComparison.Ordinal))
                {
                    if (value is not string word || !kind[5..].Split('|').Contains(word))
                        throw new RefusedVbaException("VBA template parameter must be one of the listed words");
                    return word;
                }
                throw new RefusedVbaException("unknown VBA template parameter type");
        }
    }

    /// <summary>A template case becomes the same case with the modules the template builds.</summary>
    public static Dictionary<string, object> Expand(Dictionary<string, object> probe)
    {
        string[] required = { "sheets", "template", "params", "calls", "readback", "event_log" };
        if (!required.All(probe.ContainsKey) || probe.Keys.Any(k => !required.Contains(k) && k != "names"))
            throw new RefusedVbaException("invalid VBA case fields");
        var templates = (Dictionary<string, object>)Catalogue["templates"];
        if (probe["template"] is not string name || !templates.TryGetValue(name, out var found))
            throw new RefusedVbaException("unknown VBA probe template");
        var entry = (Dictionary<string, object>)found;
        var declared = (Dictionary<string, object>)entry["params"];
        if (probe["params"] is not Dictionary<string, object> given || given.Count != declared.Count || !declared.Keys.All(given.ContainsKey))
            throw new RefusedVbaException("VBA template parameters do not match the template");
        if (probe["sheets"] is not List<object> sheets
            || sheets.Any(s => s is not Dictionary<string, object> d || d.GetValueOrDefault("name") is not string))
            throw new RefusedVbaException("invalid VBA sheet count");
        var sheetNames = sheets.Select(s => ((string)((Dictionary<string, object>)s)["name"]).ToLowerInvariant()).ToHashSet();
        var rendered = declared.ToDictionary(p => p.Key, p => Render((string)p.Value, given[p.Key], sheetNames));
        var modules = new List<object>();
        foreach (var item in (List<object>)entry["modules"])
        {
            var module = (Dictionary<string, object>)item;
            var source = Placeholder.Replace(TemplateSource(module), m => rendered.TryGetValue(m.Groups[1].Value, out var text)
                ? text : throw new RefusedVbaException("VBA template uses an undeclared parameter: " + m.Groups[1].Value));
            modules.Add(new Dictionary<string, object> { ["name"] = module["name"], ["kind"] = module["kind"], ["source"] = source });
        }
        var entryPoints = ((List<object>)entry["entry_points"]).Cast<string>().ToHashSet();
        if (probe["calls"] is not List<object> calls
            || calls.Any(c => c is not Dictionary<string, object> d || d.GetValueOrDefault("procedure") is not string p || !entryPoints.Contains(p)))
            throw new RefusedVbaException("a VBA call must name an entry point of the template");
        return new Dictionary<string, object>
        {
            ["sheets"] = probe["sheets"],
            ["names"] = probe.GetValueOrDefault("names") ?? new Dictionary<string, object>(),
            ["modules"] = modules,
            ["calls"] = probe["calls"],
            ["readback"] = probe["readback"],
            ["event_log"] = probe["event_log"],
        };
    }
}
