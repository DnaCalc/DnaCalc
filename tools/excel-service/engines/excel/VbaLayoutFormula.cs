using System.Text.RegularExpressions;

namespace Touchstone.Excel;

/// <summary>
/// The language of a template's layout formulas and defined names (dc-heiq6). A formula is made only of numbers,
/// quoted strings, TRUE and FALSE, error values, A1 references (alone, or after a plain sheet name or a sheet
/// placeholder), the layout's own defined names, operators, and calls of a listed worksheet function
/// (vba-probe-policy.json, layout_functions) or of a public function of the template's own modules. Anything else is
/// refused, so a layout formula cannot name another workbook, a DDE or RTD topic, WEBSERVICE, CALL or REGISTER.ID,
/// INDIRECT, a function value (LET, LAMBDA, MAP) or the result of a call. The check runs on the template text at
/// admission, with each placeholder as one literal, and again on the rendered formula of every request.
/// </summary>
static class VbaLayoutFormula
{
    static readonly Regex Token = new(
        @"\G(?:(?<ws> +)|(?<str>""(?:[^""]|"""")*"")|(?<quoted>'[^']*'!)"
        + @"|(?<error>#(?:NULL!|DIV/0!|VALUE!|REF!|NAME\?|NUM!|N/A))"
        + @"|(?<num>(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[Ee][+-]?[0-9]+)?(?![A-Za-z0-9_.]))"
        + @"|(?<ref>\$?[A-Za-z]{1,3}\$?[1-9][0-9]{0,6}(?![A-Za-z0-9_.(!]))"
        + @"|(?<name>[A-Za-z_][A-Za-z0-9_.]*)(?<follow>[(!])?"
        + @"|(?<op><=|>=|<>|[-+*/^&=<>,():%]))", RegexOptions.CultureInvariant);
    static readonly Regex Reference = new(@"\G\$?[A-Za-z]{1,3}\$?[1-9][0-9]{0,6}(?![A-Za-z0-9_.(!])", RegexOptions.CultureInvariant);

    static RefusedVbaException Refuse(string what) => new("invalid VBA template layout formula: " + what);

    /// <param name="parameters">The template's declared parameter types at admission; null for a rendered formula.</param>
    /// <param name="names">The defined names of the same layout.</param>
    /// <param name="functions">The template's public standard-module functions, lowercase.</param>
    /// <param name="sheets">The case's sheet names for a rendered formula; null at admission, where any plain sheet name stands.</param>
    public static void Check(string formula, IReadOnlyDictionary<string, string> parameters, ISet<string> names,
        ISet<string> functions, ISet<string> sheets = null)
    {
        if (!formula.StartsWith('=') || formula.Length > 8192 || formula.Any(ch => ch < ' ' || ch > '~'))
            throw Refuse("it is formula text of at most 8192 printable ASCII characters");
        bool operand = false;      // the previous token ended a value: the next must be an operator
        for (int at = 1; at < formula.Length;)
        {
            if (formula.AsSpan(at).StartsWith("{{", StringComparison.Ordinal))
            {
                var placeholder = VbaTemplates.Placeholder.Match(formula, at);
                if (parameters == null || !placeholder.Success || placeholder.Index != at
                    || !parameters.TryGetValue(placeholder.Groups[1].Value, out var type))
                    throw Refuse("an undeclared or malformed layout placeholder");
                if (operand) throw Refuse("a layout placeholder must begin a literal");
                at += placeholder.Length;
                if (type == "sheet")
                {
                    if (at >= formula.Length || formula[at] != '!') throw Refuse("a sheet placeholder must qualify a fixed A1 reference");
                    at = Qualified(formula, at + 1, "a sheet placeholder");
                }
                else if (at < formula.Length && !" +-*/^&=<>,):%".Contains(formula[at]))
                    throw Refuse("a layout placeholder must end a literal");
                operand = true;
                continue;
            }
            var match = Token.Match(formula, at);
            if (!match.Success) throw Refuse("a character outside the layout formula language: " + VbaGrammar.Escape(formula[at].ToString()));
            at += match.Length;
            if (match.Groups["ws"].Success) continue;
            if (match.Groups["op"].Success)
            {
                var op = match.Value;
                if (op == "(" && operand) throw Refuse("a call of a value");
                operand = op is ")" or "%";
                continue;
            }
            if (operand) throw Refuse("two values with no operator between them");
            operand = true;
            if (match.Groups["quoted"].Success)
            {
                Sheet(match.Value[1..^2], sheets);
                at = Qualified(formula, at, "a sheet name");
            }
            else if (match.Groups["ref"].Success)
            {
                if (!VbaPolicy.IsAddress(match.Value)) throw Refuse("a reference outside the worksheet");
            }
            else if (match.Groups["name"].Success)
            {
                var name = match.Groups["name"].Value;
                var follow = match.Groups["follow"].Value;
                if (follow == "!")
                {
                    Sheet(name, sheets);
                    at = Qualified(formula, at, "a sheet name");
                }
                else if (follow == "(")
                {
                    if (!VbaGrammar.LayoutFunctions.Contains(name.ToLowerInvariant()) && !functions.Contains(name.ToLowerInvariant()))
                        throw Refuse("a function that is not a listed layout function or a public function of the template: " + name);
                    operand = false;
                }
                else if (!name.Equals("TRUE", StringComparison.OrdinalIgnoreCase) && !name.Equals("FALSE", StringComparison.OrdinalIgnoreCase)
                    && !names.Contains(name))
                    throw Refuse("a name the layout does not define: " + name);
            }
        }
    }

    static void Sheet(string name, ISet<string> sheets)
    {
        if (!VbaPolicy.SheetName.IsMatch(name) || sheets != null && !sheets.Contains(name))
            throw Refuse("a sheet qualifier that is not a plain case sheet name");
    }

    static int Qualified(string formula, int at, string what)
    {
        var cell = Reference.Match(formula, at);
        if (!cell.Success || !VbaPolicy.IsAddress(cell.Value)) throw Refuse(what + " must qualify a fixed A1 reference");
        return at + cell.Length;
    }

    /// <summary>A defined name of a layout is not a reference, a literal or one of the template's procedures. It may share
    /// its spelling with a worksheet function (Rate): a name is never called, and a call names a listed function.</summary>
    public static void CheckName(string name, ISet<string> procedures)
    {
        var low = name.ToLowerInvariant();
        if (Reference.Match(name) is { Success: true } cell && cell.Length == name.Length || low is "true" or "false"
            || procedures.Contains(low))
            throw Refuse("a defined name that reads as a reference, a literal or a procedure: " + name);
    }
}
