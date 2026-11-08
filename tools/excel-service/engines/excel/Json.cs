using System.Globalization;
using System.Text;

namespace Touchstone.Excel;

/// <summary>Minimal JSON reader/writer: strings keep lone surrogates, output is ASCII.</summary>
static class Json
{
    public static object Parse(string text)
    {
        int i = 0;
        return Value(text, ref i);
    }

    static void Ws(string s, ref int i) { while (i < s.Length && (s[i] == ' ' || s[i] == '\n' || s[i] == '\r' || s[i] == '\t')) i++; }

    static object Value(string s, ref int i)
    {
        Ws(s, ref i);
        char c = s[i];
        if (c == '{')
        {
            i++;
            var d = new Dictionary<string, object>();
            Ws(s, ref i);
            if (s[i] == '}') { i++; return d; }
            while (true)
            {
                Ws(s, ref i);
                var key = Str(s, ref i);
                Ws(s, ref i);
                i++; // :
                d[key] = Value(s, ref i);
                Ws(s, ref i);
                if (s[i] == ',') { i++; continue; }
                i++; // }
                return d;
            }
        }
        if (c == '[')
        {
            i++;
            var l = new List<object>();
            Ws(s, ref i);
            if (s[i] == ']') { i++; return l; }
            while (true)
            {
                l.Add(Value(s, ref i));
                Ws(s, ref i);
                if (s[i] == ',') { i++; continue; }
                i++; // ]
                return l;
            }
        }
        if (c == '"') return Str(s, ref i);
        if (c == 't') { i += 4; return true; }
        if (c == 'f') { i += 5; return false; }
        if (c == 'n') { i += 4; return null; }
        int start = i;
        bool real = false;
        while (i < s.Length && "+-0123456789.eE".IndexOf(s[i]) >= 0) { if (s[i] == '.' || s[i] == 'e' || s[i] == 'E') real = true; i++; }
        var num = s.AsSpan(start, i - start);
        if (!real && long.TryParse(num, NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out var n)) return n;
        return double.Parse(num, CultureInfo.InvariantCulture);
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
