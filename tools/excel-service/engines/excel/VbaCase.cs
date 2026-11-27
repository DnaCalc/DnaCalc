namespace Touchstone.Excel;

sealed record VbaCase(
    string Template,
    IReadOnlyDictionary<string, VbaParameter> Parameters,
    IReadOnlyList<string> Pins,
    IReadOnlyList<VbaSheet> Sheets,
    IReadOnlyDictionary<string, string> Names,
    IReadOnlyList<VbaModule> Modules,
    IReadOnlyList<VbaCall> Calls,
    IReadOnlyList<VbaPlace> Readback,
    IReadOnlyList<VbaPlace> EventLog)
{
    // Only the policy's checked template expansion or the local grammar test route calls this.
    internal static VbaCase FromChecked(Dictionary<string, object> fields)
    {
        var template = fields.GetValueOrDefault("template") as string;
        var parameters = template == null ? new() : ((Dictionary<string, object>)fields["params"])
            .ToDictionary(p => p.Key, p => p.Value is long number ? new VbaParameter(null, number) : new((string)p.Value, null));
        var pins = template == null ? [] : ((List<object>)fields["pins"]).Cast<string>().ToArray();
        return new(
            template, parameters, pins,
            List(fields, "sheets").Cast<Dictionary<string, object>>().Select(VbaSheet.FromChecked).ToArray(),
            ((Dictionary<string, object>)fields["names"]).ToDictionary(p => p.Key, p => (string)p.Value),
            List(fields, "modules").Cast<Dictionary<string, object>>()
                .Select(m => new VbaModule((string)m["name"], (string)m["kind"], (string)m["source"])).ToArray(),
            List(fields, "calls").Cast<Dictionary<string, object>>()
                .Select(c => new VbaCall((string)c["procedure"], List(c, "args").Select(VbaCell.FromChecked).ToArray())).ToArray(),
            List(fields, "readback").Cast<string>().Select(VbaPlace.FromChecked).ToArray(),
            List(fields, "event_log").Cast<string>().Select(VbaPlace.FromChecked).ToArray());
    }

    static List<object> List(Dictionary<string, object> fields, string key) => (List<object>)fields[key];

    // The ordinary sheet case and VBA's constructed layout share the COM layout writer.
    public Dictionary<string, object> Layout() => new()
    {
        ["sheets"] = Sheets.Select(s => (object)s.Fields()).ToList(),
        ["names"] = Names.ToDictionary(p => p.Key, p => (object)p.Value),
    };

    public Dictionary<string, object> Evidence()
    {
        var fields = Layout();
        fields["modules"] = Modules.Select(m => (object)new Dictionary<string, object>
            { ["name"] = m.Name, ["kind"] = m.Kind, ["source"] = m.Source }).ToList();
        fields["calls"] = Calls.Select(c => (object)new Dictionary<string, object>
            { ["procedure"] = c.Procedure, ["args"] = c.Arguments.Select(a => (object)a.Fields()).ToList() }).ToList();
        fields["readback"] = Readback.Select(p => (object)p.Label).ToList();
        fields["event_log"] = EventLog.Select(p => (object)p.Label).ToList();
        if (Template != null)
        {
            fields["template"] = Template;
            fields["params"] = Parameters.ToDictionary(p => p.Key, p => p.Value.Value);
            fields["pins"] = Pins.ToArray();
        }
        return fields;
    }
}

sealed record VbaParameter(string Text, long? Integer)
{
    public object Value => Integer.HasValue ? Integer.Value : Text;
}

sealed record VbaModule(string Name, string Kind, string Source);
sealed record VbaCall(string Procedure, IReadOnlyList<VbaCell> Arguments);

sealed record VbaPlace(string Label, string Sheet, string Cell)
{
    internal static VbaPlace FromChecked(string text)
    {
        int bang = text.LastIndexOf('!');
        return new(text, text[..bang], text[(bang + 1)..]);
    }
}

sealed record VbaSheet(string Name, IReadOnlyDictionary<string, VbaCell> Cells,
    IReadOnlyList<int> HiddenRows, IReadOnlyList<int> HiddenColumns)
{
    internal static VbaSheet FromChecked(Dictionary<string, object> fields)
    {
        var cells = fields.GetValueOrDefault("cells") as Dictionary<string, object>;
        return new((string)fields["name"], cells?.ToDictionary(p => p.Key, p => VbaCell.FromChecked(p.Value)),
            Positions("hidden_rows"), Positions("hidden_columns"));

        int[] Positions(string key) =>
            (fields.GetValueOrDefault(key) as List<object>)?.Select(n => checked((int)(long)n)).ToArray();
    }

    public Dictionary<string, object> Fields()
    {
        var fields = new Dictionary<string, object> { ["name"] = Name };
        if (Cells != null) fields["cells"] = Cells.ToDictionary(p => p.Key, p => (object)p.Value.Fields());
        if (HiddenRows != null) fields["hidden_rows"] = HiddenRows.Select(n => (object)(long)n).ToList();
        if (HiddenColumns != null) fields["hidden_columns"] = HiddenColumns.Select(n => (object)(long)n).ToList();
        return fields;
    }
}

abstract record VbaCell
{
    public sealed record Number(string Bits) : VbaCell;
    public sealed record Text(string Value, bool ExplicitCellMode) : VbaCell;
    public sealed record Boolean(bool Value, bool ExplicitCellMode) : VbaCell;
    public sealed record Error(string Code) : VbaCell;
    public sealed record Blank : VbaCell;
    public sealed record Formula(string Source) : VbaCell;

    internal static VbaCell FromChecked(object value)
    {
        var fields = (Dictionary<string, object>)value;
        return (string)fields["type"] switch
        {
            "number" => new Number((string)fields["bits"]),
            "text" => new Text((string)fields["value"], fields.ContainsKey("mode")),
            "boolean" => new Boolean((bool)fields["value"], fields.ContainsKey("mode")),
            "error" => new Error((string)fields["code"]),
            "blank" => new Blank(),
            "formula" => new Formula((string)fields["formula"]),
            _ => throw new InvalidOperationException("unknown checked VBA cell"),
        };
    }

    public Dictionary<string, object> Fields()
    {
        var (kind, field, value, mode) = this switch
        {
            Number n => ("number", "bits", (object)n.Bits, false),
            Text t => ("text", "value", t.Value, t.ExplicitCellMode),
            Boolean b => ("boolean", "value", b.Value, b.ExplicitCellMode),
            Error e => ("error", "code", e.Code, false),
            Blank => ("blank", null, null, false),
            Formula f => ("formula", "formula", f.Source, false),
            _ => throw new InvalidOperationException("unknown VBA cell"),
        };
        var fields = new Dictionary<string, object> { ["type"] = kind };
        if (field != null) fields[field] = value;
        if (mode) fields["mode"] = "cell";
        return fields;
    }
}
