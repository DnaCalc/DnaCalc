using System.Globalization;
using System.Text;
using System.Text.Json;

namespace Touchstone.Excel;

/// <summary>Minimal JSON reader/writer: strings keep lone surrogates, output is ASCII.</summary>
static class Json
{
    public static object Parse(string text)
    {
        using var document = JsonDocument.Parse(text);
        return Value(document.RootElement);
    }

    static object Value(JsonElement value)
    {
        switch (value.ValueKind)
        {
            case JsonValueKind.Object:
                var fields = new Dictionary<string, object>();
                foreach (var field in value.EnumerateObject())
                {
                    string name;
                    try { name = field.Name; }
                    catch (InvalidOperationException error) { throw new FormatException("invalid UTF-16 JSON property name", error); }
                    if (!fields.TryAdd(name, Value(field.Value)))
                        throw new FormatException("duplicate JSON key");
                }
                return fields;
            case JsonValueKind.Array:
                return value.EnumerateArray().Select(Value).ToList();
            case JsonValueKind.String:
                // The wire transports UTF-16 units, including isolated escaped surrogates.
                int start = 0;
                return Str(value.GetRawText(), ref start);
            case JsonValueKind.Number:
                if (value.TryGetInt64(out long integer)) return integer;
                if (value.GetRawText().AsSpan().IndexOfAny('.', 'e', 'E') < 0)
                    throw new FormatException("JSON integer is outside the signed 64-bit range");
                if (value.TryGetDouble(out double real) && double.IsFinite(real)) return real;
                throw new FormatException("JSON number is not finite");
            case JsonValueKind.True: return true;
            case JsonValueKind.False: return false;
            case JsonValueKind.Null: return null;
            default: throw new FormatException("invalid JSON value");
        }
    }

    static string Str(string s, ref int i)
    {
        i++; // opening quote
        int start = i;
        while (i < s.Length && s[i] != '"' && s[i] != '\\') i++;
        if (s[i] == '"') { var r = s.Substring(start, i - start); i++; return r; }
        var sb = new StringBuilder();
        sb.Append(s, start, i - start);
        while (true)
        {
            char c = s[i++];
            if (c == '"') return sb.ToString();
            if (c != '\\') { sb.Append(c); continue; }
            char e = s[i++];
            switch (e)
            {
                case 'n': sb.Append('\n'); break;
                case 'r': sb.Append('\r'); break;
                case 't': sb.Append('\t'); break;
                case 'b': sb.Append('\b'); break;
                case 'f': sb.Append('\f'); break;
                case 'u': sb.Append((char)Convert.ToInt32(s.Substring(i, 4), 16)); i += 4; break;
                default: sb.Append(e); break;
            }
        }
    }

    public static void Quote(StringBuilder sb, string s)
    {
        sb.Append('"');
        foreach (char c in s)
        {
            if (c == '"') sb.Append("\\\"");
            else if (c == '\\') sb.Append("\\\\");
            else if (c < 0x20 || c > 0x7e) sb.Append("\\u").Append(((int)c).ToString("x4"));
            else sb.Append(c);
        }
        sb.Append('"');
    }

    /// <summary>Serialises dictionaries, lists, strings, numbers, bools and null, in insertion order.</summary>
    public static string Write(object value)
    {
        var sb = new StringBuilder();
        Write(sb, value);
        return sb.ToString();
    }

    public static void Write(StringBuilder sb, object v)
    {
        switch (v)
        {
            case null: sb.Append("null"); break;
            case string s: Quote(sb, s); break;
            case bool b: sb.Append(b ? "true" : "false"); break;
            case int n: sb.Append(n.ToString(CultureInfo.InvariantCulture)); break;
            case long n: sb.Append(n.ToString(CultureInfo.InvariantCulture)); break;
            case double d: sb.Append(d.ToString("R", CultureInfo.InvariantCulture)); break;
            case IDictionary<string, object> d:
                sb.Append('{');
                bool first = true;
                foreach (var kv in d)
                {
                    if (!first) sb.Append(',');
                    first = false;
                    Quote(sb, kv.Key); sb.Append(':'); Write(sb, kv.Value);
                }
                sb.Append('}');
                break;
            case System.Collections.IEnumerable e:
                sb.Append('[');
                bool f = true;
                foreach (var x in e) { if (!f) sb.Append(','); f = false; Write(sb, x); }
                sb.Append(']');
                break;
            default: Quote(sb, v.ToString()); break;
        }
    }
}
