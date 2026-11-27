using System.Security.Cryptography;
using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;

namespace Touchstone.Excel;

/// <summary>
/// The catalogue of reviewed VBA probe templates (dc-erfm3).
/// A VBA case names a template in vba-probes/catalogue.json and gives typed parameters. Each template module file is pinned
/// by SHA-256, and the package is built from the pinned text and the parameters only.
/// </summary>
static class VbaTemplates
{
    static readonly Dictionary<string, object> Catalogue = (Dictionary<string, object>)Json.Parse(Resource("vba-probes/catalogue.json"));
    internal static readonly Regex Placeholder = new(@"\{\{([a-z][a-z_]*)\}\}", RegexOptions.CultureInvariant);
    static readonly Regex Address = new(@"\A[A-Z]{1,3}[1-9][0-9]{0,6}\z", RegexOptions.CultureInvariant);
    static readonly Regex DefinedName = new(@"\A[A-Za-z][A-Za-z0-9_]{0,30}\z", RegexOptions.CultureInvariant);

    internal sealed record LayoutEntry(string Sheet, string Address, string Name, string Formula);

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

    internal static string TemplateSource(Dictionary<string, object> entry, string directory = null)
    {
        var file = (string)entry["file"];
        if (!Regex.IsMatch(file, @"\A[a-z0-9_-]+\.(bas|json)\z"))
            throw new RefusedVbaException("invalid VBA template filename");
        var data = directory == null ? ResourceBytes("vba-probes/" + file) : File.ReadAllBytes(Path.Combine(directory, file));
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

    internal static Dictionary<string, object> ReadCatalogue(string directory) => directory == null ? Catalogue
        : (Dictionary<string, object>)Json.Parse(File.ReadAllText(Path.Combine(directory, "catalogue.json")));

    internal static Dictionary<string, string> Parameters(Dictionary<string, object> entry) =>
        ((Dictionary<string, object>)entry["params"]).ToDictionary(p => p.Key, p => (string)p.Value);

    internal static Dictionary<string, string[]> EntryPoints(Dictionary<string, object> entry) =>
        ((Dictionary<string, object>)entry["entry_points"])
            .ToDictionary(p => p.Key, p => ((List<object>)p.Value).Cast<string>().ToArray());

    public static Dictionary<string, string[]> EntryPoints(string template) => EntryPoints(Template(template));

    internal static VbaTemplateReview Review(string name, Dictionary<string, object> entry, Dictionary<string, object> catalogue)
    {
        var review = (Dictionary<string, object>)entry["review"];
        if (review.Count != 3 || !review.ContainsKey("reviewer") || !review.ContainsKey("excel_build_family")
            || review.GetValueOrDefault("claim") is not string claim || string.IsNullOrWhiteSpace(claim)
            || review["reviewer"] is not (null or string) || review["excel_build_family"] is not (null or string))
            throw new RefusedVbaException("invalid VBA template review record");
        var reviewer = review["reviewer"] as string;
        var family = review["excel_build_family"] as string;
        if ((reviewer == null) != (family == null) || reviewer != null &&
            (string.IsNullOrWhiteSpace(reviewer) || !Regex.IsMatch(family, @"\A[0-9]+\.[0-9]+\.[0-9]+\z")))
            throw new RefusedVbaException("template review requires a reviewer and one Excel build family");
        var pin = VbaPolicy.Digest(new Dictionary<string, object>
            { ["template"] = name, ["entry"] = entry, ["limits"] = catalogue["limits"], ["rules"] = VbaPolicy.RuleTable });
        return new VbaTemplateReview(reviewer, claim, family, pin);
    }

    public static VbaTemplateReview Review(string name) => Review(name, Template(name), Catalogue);

    public static List<object> Pins(string template)
    {
        var entry = Template(template);
        return ((List<object>)entry["modules"]).Append(entry["layout"])
            .Select(file => ((Dictionary<string, object>)file)["sha256"]).ToList();
    }

    static bool AddressOk(object value) => value is string text && Address.IsMatch(text) && VbaPolicy.IsAddress(text);

    static string Quote(string text) => "\"" + text.Replace("\"", "\"\"") + "\"";

    static string CaseSheet(object value, Dictionary<string, string> sheetNames)
    {
        if (value is not string sheet || !VbaPolicy.SheetName.IsMatch(sheet)
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
                    return Quote(word);
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
            if (VbaPolicy.SheetName.IsMatch(name)) names.TryAdd(name, name);
        }
        return names;
    }

    static string Substitute(string source, Dictionary<string, string> rendered) =>
        Placeholder.Replace(source, m => rendered.TryGetValue(m.Groups[1].Value, out var text)
            ? text : throw new RefusedVbaException("VBA template uses an undeclared parameter: " + m.Groups[1].Value));

    internal static IEnumerable<LayoutEntry> ReadLayout(Dictionary<string, object> entry, string directory = null)
    {
        var layout = (List<object>)Json.Parse(TemplateSource((Dictionary<string, object>)entry["layout"], directory));
        if (layout.Count > 1000) throw new RefusedVbaException("oversized VBA template layout");
        foreach (Dictionary<string, object> item in layout)
        {
            if (item.Count == 2 && item.GetValueOrDefault("name") is string name && item.GetValueOrDefault("refers_to") is string refersTo
                && !name.StartsWith("auto_", StringComparison.OrdinalIgnoreCase) && DefinedName.IsMatch(name))
                yield return new(null, null, name, refersTo);
            else if (item.Count == 3 && item.GetValueOrDefault("sheet") is string target && item.GetValueOrDefault("at") is string at
                && VbaPolicy.IsAddress(at) && item.GetValueOrDefault("formula") is string formula)
                yield return new(target, at, null, formula);
            else
                throw new RefusedVbaException("invalid VBA template layout entry");
        }
    }

    public static void ApplyLayout(Dictionary<string, object> expanded, string template, Dictionary<string, object> given)
    {
        var entry = Template(template);
        var sheetList = (List<object>)expanded["sheets"];
        var sheetNames = CaseSheets(sheetList);
        var sheets = sheetList.Cast<Dictionary<string, object>>()
            .Select(s => new Dictionary<string, object>(s)).ToDictionary(s => (string)s["name"], StringComparer.Ordinal);
        var names = (Dictionary<string, object>)expanded["names"];
        var declared = Parameters(entry);
        var rendered = declared.ToDictionary(p => p.Key, p => Render(p.Value, given[p.Key], sheetNames, formula: true));
        foreach (var item in ReadLayout(entry))
        {
            if (item.Name != null)
            {
                if (!names.TryAdd(item.Name, Formula(item.Formula, rendered)))
                    throw new RefusedVbaException("duplicate VBA template defined name");
                continue;
            }
            var target = item.Sheet;
            var at = item.Address;
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
                [at] = new Dictionary<string, object> { ["type"] = "formula", ["formula"] = Formula(item.Formula, rendered) },
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
        var declared = Parameters(entry);
        if (probe["params"] is not Dictionary<string, object> given || given.Count != declared.Count || !declared.Keys.All(given.ContainsKey))
            throw new RefusedVbaException("VBA template parameters do not match the template");
        if (probe["sheets"] is not List<object> sheets
            || sheets.Any(s => s is not Dictionary<string, object> d || d.GetValueOrDefault("name") is not string))
            throw new RefusedVbaException("invalid VBA sheet count");
        var sheetNames = CaseSheets(sheets);
        var rendered = declared.ToDictionary(p => p.Key, p => Render(p.Value, given[p.Key], sheetNames));
        var modules = new List<object>();
        int sourceChars = 0;
        foreach (var item in (List<object>)entry["modules"])
        {
            var module = (Dictionary<string, object>)item;
            var moduleName = (string)module["name"];
            if ((string)module["kind"] == "document" && moduleName != "ThisWorkbook"
                && int.Parse(moduleName[5..], CultureInfo.InvariantCulture) > sheets.Count)
                throw new RefusedVbaException("VBA template document module requires a sheet in this case");
            var source = Substitute(TemplateSource(module), rendered);
            if (source.Length < 1 || source.Length > VbaPolicy.Limit("max_module_chars"))
                throw new RefusedVbaException("oversized rendered VBA module source");
            sourceChars += source.Length;
            modules.Add(new Dictionary<string, object> { ["name"] = module["name"], ["kind"] = module["kind"], ["source"] = source });
        }
        if (sourceChars > VbaPolicy.Limit("max_source_chars"))
            throw new RefusedVbaException("oversized rendered VBA module source");
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
