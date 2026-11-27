using System.Text.RegularExpressions;

namespace Touchstone.Excel;

static class VbaTemplateAdmission
{
    static readonly Regex ParameterName = new(@"\A[a-z][a-z_]*\z", RegexOptions.CultureInvariant);
    static readonly Regex EnumType = new(@"\Aenum:[A-Za-z0-9_]+(?:\|[A-Za-z0-9_]+)*\z", RegexOptions.CultureInvariant);
    static readonly Regex SheetCell = new(@"\G\$?[A-Za-z]{1,3}\$?[1-9][0-9]{0,6}", RegexOptions.CultureInvariant);

    public static int Verify(string directory)
    {
        var catalogue = VbaTemplates.ReadCatalogue(directory);
        var limits = (Dictionary<string, object>)catalogue["limits"];
        if (limits.Count != 3 || limits.GetValueOrDefault("text") is not long text || text is < 1 or > 255
            || limits.GetValueOrDefault("long_min") is not long min || min < int.MinValue
            || limits.GetValueOrDefault("long_max") is not long max || max > int.MaxValue || min > max)
            throw new RefusedVbaException("invalid VBA template parameter limits");
        var templates = (Dictionary<string, object>)catalogue["templates"];
        if (catalogue.Count != 2 || templates.Count == 0)
            throw new RefusedVbaException("invalid VBA template catalogue");
        foreach (var (name, value) in templates)
        {
            var entry = (Dictionary<string, object>)value;
            if (entry.Count != 5 || !ParameterName.IsMatch(name))
                throw new RefusedVbaException("invalid VBA template catalogue entry");
            var parameters = VbaTemplates.Parameters(entry);
            if (parameters.Any(p => !ParameterName.IsMatch(p.Key)
                || p.Value is not ("sheet" or "address" or "text" or "long") && !EnumType.IsMatch(p.Value)))
                throw new RefusedVbaException("invalid VBA template parameter declaration");
            var signatures = VbaTemplates.EntryPoints(entry);
            if (signatures.Count == 0 || signatures.Keys.Distinct(StringComparer.OrdinalIgnoreCase).Count() != signatures.Count
                || signatures.Values.Any(args => args.Length > VbaPolicy.Limit("max_args")
                    || args.Any(kind => kind is not ("number" or "boolean" or "error" or "blank"))))
                throw new RefusedVbaException("invalid VBA template argument signature");
            var modules = (List<object>)entry["modules"];
            if (modules.Count < 1 || modules.Count > VbaPolicy.Limit("max_modules"))
                throw new RefusedVbaException("invalid VBA template module count");
            var sources = new List<(string name, string kind, string source)>();
            foreach (Dictionary<string, object> module in modules)
            {
                var source = VbaTemplates.TemplateSource(module, directory);
                if (module.Count != 4 || source.Length < 1 || source.Length > VbaPolicy.Limit("max_module_chars"))
                    throw new RefusedVbaException("invalid or oversized VBA template module");
                if (source.Any(ch => ch < ' ' && ch is not ('\t' or '\r' or '\n') || ch > '~'))
                    throw new RefusedVbaException("VBA template module has a control character");
                sources.Add(((string)module["name"], (string)module["kind"], source));
            }
            if (sources.Sum(m => m.source.Length) > VbaPolicy.Limit("max_source_chars"))
                throw new RefusedVbaException("oversized VBA template sources");
            VbaGrammar.CheckModules(sources, sheetCount: 16, parameters, signatures);
            var targets = new HashSet<(string sheet, string at)>();
            var names = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            foreach (var item in VbaTemplates.ReadLayout(entry, directory))
            {
                if (item.Sheet != null)
                {
                    var placeholder = VbaTemplates.Placeholder.Match(item.Sheet);
                    if (!VbaPolicy.SheetName.IsMatch(item.Sheet)
                        && (!placeholder.Success || placeholder.Length != item.Sheet.Length
                            || parameters.GetValueOrDefault(placeholder.Groups[1].Value) != "sheet"))
                        throw new RefusedVbaException("layout target sheet must be a case sheet or a single sheet placeholder");
                    if (!targets.Add((item.Sheet.ToUpperInvariant(), item.Address.Replace("$", "").ToUpperInvariant())))
                        throw new RefusedVbaException("duplicate VBA template layout cell");
                }
                else if (!names.Add(item.Name))
                    throw new RefusedVbaException("duplicate VBA template defined name");
                CheckFormula(item.Formula, parameters);
            }
            VbaTemplates.Review(name, entry, catalogue);
        }
        return templates.Count;
    }

    // This checks the splice positions in trusted template text. The Excel review covers
    // the fixed formula and every choice the typed parameter makes available.
    static void CheckFormula(string formula, IReadOnlyDictionary<string, string> parameters)
    {
        if (!formula.StartsWith('=') || formula.Length > 8192 || formula.Any(ch => ch < ' ' || ch > '~'))
            throw new RefusedVbaException("invalid VBA template layout formula");
        char quote = '\0';
        for (int at = 1; at < formula.Length; at++)
        {
            char ch = formula[at];
            if (formula.AsSpan(at).StartsWith("{{", StringComparison.Ordinal))
            {
                var token = VbaTemplates.Placeholder.Match(formula, at);
                if (quote != '\0' || !token.Success || token.Index != at
                    || !parameters.TryGetValue(token.Groups[1].Value, out var type))
                    throw new RefusedVbaException("invalid or quoted VBA layout placeholder");
                int left = at - 1;
                while (left > 0 && formula[left] == ' ') left--;
                if (!"=+-*/^&<>,;(".Contains(formula[left]))
                    throw new RefusedVbaException("VBA layout placeholder must begin a literal");
                int end = at + token.Length;
                if (type == "sheet")
                {
                    var cell = end < formula.Length && formula[end] == '!' ? SheetCell.Match(formula, end + 1) : Match.Empty;
                    if (!cell.Success || !VbaPolicy.IsAddress(cell.Value) || !EndsLiteral(formula, cell.Index + cell.Length))
                        throw new RefusedVbaException("a sheet placeholder must qualify a fixed A1 reference");
                }
                else if (!EndsLiteral(formula, end))
                    throw new RefusedVbaException("VBA layout placeholder must end a literal");
                at = end - 1;
            }
            else if (ch == quote)
            {
                if (at + 1 < formula.Length && formula[at + 1] == quote) at++;
                else quote = '\0';
            }
            else if (quote == '\0' && ch is '"' or '\'')
                quote = ch;
        }
        if (quote != '\0') throw new RefusedVbaException("unclosed quote in VBA template layout formula");
    }

    static bool EndsLiteral(string formula, int at)
    {
        while (at < formula.Length && formula[at] == ' ') at++;
        return at == formula.Length || "+-*/^&<>,;):%".Contains(formula[at]);
    }
}
