using System.Collections.Concurrent;
using System.Diagnostics;
using System.Globalization;
using System.Runtime.InteropServices;
using System.Text;

namespace Touchstone.Excel;

/// <summary>
/// Excel engine worker for the touchstone/1 broker: one owned Excel, frames on stdin/stdout
/// (4-byte big-endian length, one type byte, JSON body; no compression on this pipe).
///   broker -> worker: B batch, Q quit, R recycle, F fingerprint check
///   worker -> broker: H hello (env), R result, X error/fatal, O ok
/// </summary>
static class Program
{
    static readonly Stream Out = Console.OpenStandardOutput();
    static readonly object OutLock = new();
    static OwnedExcel X;
    static string Root, Name;
    static int Mt, Launch, StartupSeconds = 60;
    static long DeadlineTicks;          // watchdog: 0 = none
    static bool WatchdogFired;
    static int Batches;
    static string Stage = "";

    static void Send(char type, string json)
    {
        var body = Encoding.ASCII.GetBytes(json);
        var header = new byte[5];
        int n = body.Length + 1;
        header[0] = (byte)(n >> 24); header[1] = (byte)(n >> 16); header[2] = (byte)(n >> 8); header[3] = (byte)n; header[4] = (byte)type;
        lock (OutLock) { Out.Write(header, 0, 5); Out.Write(body, 0, body.Length); Out.Flush(); }
    }

    static int Main(string[] args)
    {
        for (int i = 0; i + 1 < args.Length; i += 2)
        {
            switch (args[i])
            {
                case "--root": Root = args[i + 1]; break;
                case "--name": Name = args[i + 1]; break;
                case "--mt": Mt = int.Parse(args[i + 1]); break;
                case "--startup-seconds": StartupSeconds = int.Parse(args[i + 1]); break;
            }
        }
        if (Root == null || Name == null) { Console.Error.WriteLine("usage: excel-worker --root DIR --name NAME [--mt N]"); return 2; }
        Root = Path.GetFullPath(Root);
        var watchdog = new Timer(_ => Watchdog(), null, 25, 25);
        var input = new BlockingCollection<(char, string)>();
        var reader = new Thread(() => ReadLoop(input)) { IsBackground = true };
        reader.Start();
        try
        {
            if (!StartExcel(false)) return 3;
            while (true)
            {
                if (!input.TryTake(out var frame, 60000))
                {
                    IdleCheck();
                    continue;
                }
                var (type, body) = frame;
                if (type == 'Q') break;
                if (type == 'B') HandleBatch(body);
                else if (type == 'R') { if (!Recycle("requested")) return 3; }
                else if (type == 'F') Send('O', "{\"fingerprint\":" + Json.Write(X.Fingerprint()) + "}");
                else if (type == '\0') break;     // stdin closed
            }
        }
        finally
        {
            DeadlineTicks = 0;
            if (X != null) { X.Quit(); X = null; }
        }
        return 0;
    }

    static void ReadLoop(BlockingCollection<(char, string)> input)
    {
        var stdin = Console.OpenStandardInput();
        var header = new byte[5];
        while (true)
        {
            if (!ReadExact(stdin, header, 5)) { input.Add(('\0', null)); return; }
            int n = (header[0] << 24) | (header[1] << 16) | (header[2] << 8) | header[3];
            var body = new byte[n - 1];
            if (!ReadExact(stdin, body, n - 1)) { input.Add(('\0', null)); return; }
            input.Add(((char)header[4], Encoding.UTF8.GetString(body)));
        }
    }

    static bool ReadExact(Stream s, byte[] buffer, int count)
    {
        int got = 0;
        while (got < count)
        {
            int r = s.Read(buffer, got, count - got);
            if (r <= 0) return false;
            got += r;
        }
        return true;
    }

    static void Watchdog()
    {
        long d = Interlocked.Read(ref DeadlineTicks);
        if (d == 0 || DateTime.UtcNow.Ticks < d) return;
        var x = X;
        if (x == null || WatchdogFired) return;
        WatchdogFired = true;
        // Only this worker's own Excel, identified by PID and start ticks.
        var outcome = OwnedExcel.KillOwned(x.Pid, x.StartTicks);
        Console.Error.WriteLine($"watchdog: batch deadline passed; {outcome} (pid {x.Pid})");
    }

    /// <summary>Starts (or restarts) Excel. A failed start is final: no retry loop, the worker reports it and exits.</summary>
    static bool StartExcel(bool restart)
    {
        try
        {
            Launch++;
            X = OwnedExcel.Start(Root, Name, Launch, Mt, StartupSeconds);
            WatchdogFired = false;
            Warm();
            Send('H', Json.Write(new Dictionary<string, object>
            {
                ["name"] = Name, ["pid"] = X.Pid, ["start_ticks"] = X.StartTicks, ["launch"] = Launch,
                ["env_id"] = X.EnvId, ["env"] = X.Env, ["restart"] = restart,
                ["worker_pid"] = Environment.ProcessId,
            }));
            return true;
        }
        catch (Exception e)
        {
            // An Excel that started but failed its warm-up or hello is still ours to stop.
            if (X != null) { OwnedExcel.KillOwned(X.Pid, X.StartTicks); }
            X = null;
            Send('X', Json.Write(new Dictionary<string, object> { ["fatal"] = "excel-start-failed", ["detail"] = e.GetType().Name + ": " + e.Message }));
            return false;
        }
    }

    /// <summary>One throwaway batch covering every argument kind, so the first real batch does not pay for JIT and binder setup.</summary>
    static void Warm()
    {
        Dictionary<string, object> G(string f, string k, params object[] v) =>
            new() { ["f"] = f, ["k"] = k, ["n"] = 1L, ["v"] = v.ToList() };
        var groups = new List<object>
        {
            G("ABS", "n", "3ff0000000000000"), G("LEN", "s", "abc"), G("LEN", "S", "abc"), G("AND", "b,B", true, true),
            G("ISERROR", "e", "#N/A"), G("ISBLANK", "z", (object)null), G("SUM", "a1x2", "3ff0000000000000", "4000000000000000"),
        };
        try { EvaluateBatch(groups, true, false, new Dictionary<string, object>()); Batches = 0; }
        catch (Exception e) { Console.Error.WriteLine("warm-up failed: " + e.Message + " [" + Stage + "]"); }
    }

    static bool Recycle(string reason)
    {
        var old = X;
        if (old != null)
        {
            if (OwnedExcel.IdentityHolds(old.Pid, old.StartTicks)) old.Quit();
            X = null;
        }
        return StartExcel(true);
    }

    static void IdleCheck()
    {
        try
        {
            if (X.SnapshotDrifted() || X.Fingerprint() != X.Baseline) { Send('X', "{\"event\":\"idle-drift\"}"); Recycle("idle drift"); }
        }
        catch (Exception e) { Send('X', Json.Write(new Dictionary<string, object> { ["event"] = "idle-unreachable", ["detail"] = e.Message })); Recycle("idle unreachable"); }
    }

    // ----- batch evaluation -------------------------------------------------------------

    sealed class Group
    {
        public string F; public string[] Tokens; public int N; public int Width; public List<object> V; public List<object> Xsel;
        public int Row0;        // first sheet row (1-based)
        public int[] TokenCol;  // first cell offset (0-based) per token
    }

    static string ColName(int c)
    {
        var s = "";
        while (c > 0) { c--; s = (char)('A' + c % 26) + s; c /= 26; }
        return s;
    }

    static int TokenWidth(string t)
    {
        if (t[0] != 'a') return 1;
        var p = t.Substring(1).Split('x');
        return int.Parse(p[0]) * int.Parse(p[1]);
    }

    static double Bits(object hex) => BitConverter.UInt64BitsToDouble(ulong.Parse((string)hex, NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture));
    static string HexOf(double d) => BitConverter.DoubleToUInt64Bits(d).ToString("x16");
    static bool IsHex16(string s) => s.Length == 16;

    static string Formula(Group g, int row)
    {
        // Builds the R1C1 formula of one call (row = index within the group), as the v1 witness request did.
        var refs = new List<string>();
        int cell = 0;
        for (int t = 0; t < g.Tokens.Length; t++)
        {
            var tok = g.Tokens[t];
            int at = g.TokenCol[t];
            if (tok[0] == 'a')
            {
                var p = tok.Substring(1).Split('x');
                int r = int.Parse(p[0]), c = int.Parse(p[1]);
                var idx = new StringBuilder("{");
                for (int i = 0; i < r; i++)
                {
                    if (i > 0) idx.Append(';');
                    for (int j = 0; j < c; j++) { if (j > 0) idx.Append(','); idx.Append(i * c + j + 1); }
                }
                idx.Append('}');
                var cells = string.Join(",", Enumerable.Range(0, r * c).Select(k => "RC[" + (at + k + 1) + "]"));
                refs.Add("CHOOSE(" + idx + "," + cells + ")");
            }
            else if (tok == "S") refs.Add("\"" + ((string)g.V[row * g.Width + at]).Replace("\"", "\"\"") + "\"");
            else if (tok == "B") refs.Add((bool)g.V[row * g.Width + at] ? "TRUE" : "FALSE");
            else refs.Add("RC[" + (at + 1) + "]");
            cell++;
        }
        var f = g.F + "(" + string.Join(",", refs) + ")";
        if (g.Xsel != null) f = "INDEX(" + f + "," + g.Xsel[0] + "," + g.Xsel[1] + ")";
        return "=" + f;
    }

    static dynamic Rng(dynamic sheet, int r0, int c0, int r1, int c1) => sheet.Range[ColName(c0) + r0 + ":" + ColName(c1) + r1];

    sealed class Block { public int Row, Col; public object[,] Data; }

    static void HandleBatch(string body)
    {
        var sw = Stopwatch.StartNew();
        var req = (Dictionary<string, object>)Json.Parse(body);
        object id = req["id"];
        double deadlineMs = req.TryGetValue("deadline_ms", out var dm) && dm != null ? Convert.ToDouble(dm) : 60000;
        bool echo = req.TryGetValue("echo", out var ec) && ec is true;
        bool serial = req.TryGetValue("serial", out var se) && se is true;
        var timing = new Dictionary<string, object>();
        var result = new Dictionary<string, object> { ["id"] = id, ["env_id"] = X.EnvId, ["worker"] = Name, ["batch"] = ++Batches };
        try
        {
            // Drift before: recycle and report; the broker re-queues nothing it has not been told about.
            var before = X.Fingerprint();
            if (before != X.Baseline)
            {
                result["error"] = "fingerprint-drift-before";
                result["detail"] = "expected " + X.Baseline + " got " + before;
                Send('R', Json.Write(result));
                Recycle("drift before");
                return;
            }
            WatchdogFired = false;
            Interlocked.Exchange(ref DeadlineTicks, DateTime.UtcNow.AddMilliseconds(deadlineMs).Ticks);
            var groups = EvaluateBatch((List<object>)req["groups"], echo, serial, timing);
            Interlocked.Exchange(ref DeadlineTicks, 0);
            var after = X.Fingerprint();
            if (after != X.Baseline)
            {
                result["error"] = "fingerprint-drift-after";
                result["detail"] = "expected " + X.Baseline + " got " + after;
                Send('R', Json.Write(result));
                Recycle("drift after");
                return;
            }
            result["groups"] = groups;
            timing["total_ms"] = Math.Round(sw.Elapsed.TotalMilliseconds, 3);
            result["timing"] = timing;
            Send('R', Json.Write(result));
        }
        catch (Exception e)
        {
            Interlocked.Exchange(ref DeadlineTicks, 0);
            bool dead = WatchdogFired || !OwnedExcel.IdentityHolds(X.Pid, X.StartTicks);
            result["error"] = dead ? "excel-crashed" : "excel-error";
            result["detail"] = e.GetType().Name + ": " + e.Message + (WatchdogFired ? " (deadline)" : "") + " [" + Stage + "]";
            result["deadline"] = WatchdogFired;
            Send('R', Json.Write(result));
            if (dead) { X.Process?.Dispose(); X = null; StartExcel(true); }
            else
            {
                // Not dead but failed: clear the scratch sheet; if that fails the instance is recycled.
                try { Rng(X.SheetS, 1, 1, 2000, 200).Clear(); }
                catch { Recycle("clear failed"); }
            }
        }
    }

    static List<object> EvaluateBatch(List<object> groupList, bool echo, bool serial, Dictionary<string, object> timing)
    {
        var sw = Stopwatch.StartNew();
        var groups = new List<Group>();
        int row = 1, maxCols = 1;
        foreach (Dictionary<string, object> raw in groupList)
        {
            var g = new Group
            {
                F = (string)raw["f"],
                Tokens = ((string)raw["k"]).Length == 0 ? Array.Empty<string>() : ((string)raw["k"]).Split(','),
                N = (int)(long)raw["n"],
                V = (List<object>)raw["v"],
                Xsel = raw.TryGetValue("x", out var xs) ? (List<object>)xs : null,
                Row0 = row,
            };
            g.TokenCol = new int[g.Tokens.Length];
            int w = 0;
            for (int t = 0; t < g.Tokens.Length; t++) { g.TokenCol[t] = w; w += TokenWidth(g.Tokens[t]); }
            g.Width = w;
            row += g.N;
            maxCols = Math.Max(maxCols, 1 + w);
            groups.Add(g);
        }
        int totalRows = row - 1;
        if (totalRows > 1000000) throw new InvalidOperationException("batch-too-large");
        dynamic sheet = X.SheetS;
        var written = new List<Block>();

        // 1. arguments. A batch of modest size is written as ONE Value2 rectangle (null cells stay blank);
        //    text columns get NumberFormat "@" first and error arguments are entered as literals afterwards.
        int argCols = maxCols - 1;
        if ((long)totalRows * argCols <= 2_000_000)
        {
            var all = new object[totalRows, argCols];
            var textRegions = new List<(int r, int c, int n, int m)>();
            var errorRegions = new List<(int r, int c, int n, int m)>();
            foreach (var g in groups)
            {
                for (int t = 0; t < g.Tokens.Length; t++)
                {
                    char c = g.Tokens[t][0];
                    int w = TokenWidth(g.Tokens[t]), col = g.TokenCol[t];
                    if (c == 'z') continue;
                    if (c == 's' || c == 'S') textRegions.Add((g.Row0, col, g.N, 1));
                    if (c == 'e') errorRegions.Add((g.Row0, col, g.N, 1));
                    for (int i = 0; i < g.N; i++)
                    {
                        int baseIndex = i * g.Width + col;
                        for (int j = 0; j < w; j++)
                        {
                            object v = g.V[baseIndex + j];
                            all[g.Row0 - 1 + i, col + j] = c switch
                            {
                                'n' or 'a' => Bits(v),
                                's' or 'S' => (string)v,
                                'b' or 'B' => (bool)v,
                                _ => new ErrorWrapper(Codes[(string)v]),
                            };
                        }
                    }
                }
            }
            Stage = "args block";
            foreach (var (r, c, n, m) in textRegions) Rng(sheet, r, 2 + c, r + n - 1, 2 + c + m - 1).NumberFormat = "@";
            var cmp = (object[,])all.Clone();
            foreach (var (r, c, n, m) in errorRegions)
                for (int i = 0; i < n; i++) all[r - 1 + i, c] = null;
            Rng(sheet, 1, 2, totalRows, maxCols).Value2 = all;
            foreach (var (r, c, n, m) in errorRegions)
            {
                var lit = new object[n, 1];
                for (int i = 0; i < n; i++) lit[i, 0] = "=" + Names[((ErrorWrapper)cmp[r - 1 + i, c]).ErrorCode];
                Rng(sheet, r, 2 + c, r + n - 1, 2 + c).Formula2R1C1 = lit;
            }
            written.Add(new Block { Row = 1, Col = 2, Data = cmp });
        }
        else
        foreach (var g in groups)
        {
            int t = 0;
            while (t < g.Tokens.Length)
            {
                char c = g.Tokens[t][0];
                bool numeric = c == 'n' || c == 'a';
                int t2 = t;
                if (numeric) while (t2 + 1 < g.Tokens.Length && (g.Tokens[t2 + 1][0] == 'n' || g.Tokens[t2 + 1][0] == 'a')) t2++;
                int cellStart = g.TokenCol[t];
                int cellEnd = g.TokenCol[t2] + TokenWidth(g.Tokens[t2]);
                int m = cellEnd - cellStart;
                if (c == 'z') { t = t2 + 1; continue; }
                var data = new object[g.N, m];
                for (int i = 0; i < g.N; i++)
                {
                    int baseIndex = i * g.Width + cellStart;
                    for (int j = 0; j < m; j++)
                    {
                        object v = g.V[baseIndex + j];
                        switch (c)
                        {
                            case 'n': case 'a': data[i, j] = Bits(v); break;
                            case 's': case 'S': data[i, j] = (string)v; break;
                            case 'b': case 'B': data[i, j] = (bool)v; break;
                            case 'e': data[i, j] = new ErrorWrapper(Codes[(string)v]); break;
                        }
                    }
                }
                Stage = "args " + g.F + " token " + g.Tokens[t] + " rows " + g.Row0 + "+" + g.N;
                var range = Rng(sheet, g.Row0, 2 + cellStart, g.Row0 + g.N - 1, 2 + cellEnd - 1);
                if (c == 's' || c == 'S') range.NumberFormat = "@";
                if (c == 'e')
                {
                    var lit = new object[g.N, m];
                    for (int i = 0; i < g.N; i++)
                        for (int j = 0; j < m; j++) lit[i, j] = "=" + (string)g.V[i * g.Width + cellStart + j];
                    range.Formula2R1C1 = lit;
                }
                else range.Value2 = data;
                written.Add(new Block { Row = g.Row0, Col = 2 + cellStart, Data = data });
                t = t2 + 1;
            }
        }
        timing["args_ms"] = Math.Round(sw.Elapsed.TotalMilliseconds, 3);
        // 2. formulas, one assignment per group (per-row only where a literal is part of the text)
        foreach (var g in groups)
        {
            Stage = "formula " + g.F + " rows " + g.Row0 + "+" + g.N + " " + Formula(g, 0);
            var range = Rng(sheet, g.Row0, 1, g.Row0 + g.N - 1, 1);
            if (g.Tokens.Any(tk => tk == "S" || tk == "B"))
            {
                var fs = new object[g.N, 1];
                for (int i = 0; i < g.N; i++) fs[i, 0] = Formula(g, i);
                range.Formula2R1C1 = fs;
            }
            else range.Formula2R1C1 = Formula(g, 0);
        }
        timing["write_ms"] = Math.Round(sw.Elapsed.TotalMilliseconds, 3);
        sw.Restart();
        // 3. one Calculate for the batch
        Stage = "calculate";
        X.App.Calculate();
        while ((int)X.App.CalculationState != 0) Thread.Sleep(0);
        timing["calc_ms"] = Math.Round(sw.Elapsed.TotalMilliseconds, 3);
        sw.Restart();
        // 4. read results and arguments back as one rectangle
        object raw2 = Rng(sheet, 1, 1, totalRows, maxCols).Value2;
        Func<int, int, object> at;
        if (raw2 is Array arr)
        {
            int lb0 = arr.GetLowerBound(0), lb1 = arr.GetLowerBound(1);
            var mat = (object[,])raw2;
            at = (r, c) => mat[lb0 + r - 1, lb1 + c - 1];
        }
        else at = (r, c) => raw2;
        timing["read_ms"] = Math.Round(sw.Elapsed.TotalMilliseconds, 3);
        sw.Restart();
        // 5. results and the per-row input check
        var changed = new HashSet<int>();
        foreach (var b in written)
        {
            int rows = b.Data.GetLength(0), cols = b.Data.GetLength(1);
            for (int i = 0; i < rows; i++)
                for (int j = 0; j < cols; j++)
                    if (!Same(b.Data[i, j], at(b.Row + i, b.Col + j))) changed.Add(b.Row + i);
        }
        var output = new List<object>();
        foreach (var g in groups)
        {
            var ty = new StringBuilder(g.N);
            var vals = new List<object>(g.N);
            for (int i = 0; i < g.N; i++)
            {
                int r = g.Row0 + i;
                if (changed.Contains(r))
                {
                    var back = new List<object>();
                    for (int j = 0; j < g.Width; j++) { var (l, v) = Convert1(at(r, 2 + j)); back.Add(new List<object> { l.ToString(), v }); }
                    ty.Append('u');
                    vals.Add(new Dictionary<string, object> { ["reason"] = "input-changed", ["read_back"] = back });
                }
                else
                {
                    var (l, v) = Convert1(at(r, 1));
                    ty.Append(l);
                    vals.Add(v);
                }
            }
            var og = new Dictionary<string, object> { ["ty"] = ty.ToString(), ["v"] = vals };
            if (echo)
            {
                var ity = new StringBuilder();
                var iv = new List<object>();
                for (int i = 0; i < g.N; i++)
                    for (int j = 0; j < g.Width; j++) { var (l, v) = Convert1(at(g.Row0 + i, 2 + j)); ity.Append(l); iv.Add(v); }
                og["ity"] = ity.ToString();
                og["iv"] = iv;
            }
            output.Add(og);
        }
        // 6. clear the used block
        Rng(sheet, 1, 1, totalRows, maxCols).Clear();
        timing["convert_ms"] = Math.Round(sw.Elapsed.TotalMilliseconds, 3);
        timing["calls"] = totalRows;
        return output;
    }

    static readonly Dictionary<string, int> Codes = new()
    {
        ["#NULL!"] = 2000, ["#DIV/0!"] = 2007, ["#VALUE!"] = 2015, ["#REF!"] = 2023, ["#NAME?"] = 2029, ["#NUM!"] = 2036, ["#N/A"] = 2042,
    };
    static readonly Dictionary<int, string> Names = Codes.ToDictionary(kv => kv.Value, kv => kv.Key);

    static (char, object) Convert1(object v)
    {
        switch (v)
        {
            case null: return ('z', null);
            case double d: return ('n', HexOf(d));
            case string s: return ('t', s);
            case bool b: return ('b', b);
            case int e: { int code = e & 0xFFFF; return ('e', Names.TryGetValue(code, out var n) ? n : "#CODE" + code); }
            default: return ('u', v.GetType().FullName);
        }
    }

    static bool Same(object written, object read)
    {
        switch (written)
        {
            case null: return read == null;
            case ErrorWrapper w: return read is int i && (i & 0xFFFF) == w.ErrorCode;
            case double d: return read is double r && BitConverter.DoubleToInt64Bits(r) == BitConverter.DoubleToInt64Bits(d);
            case bool b: return read is bool rb && rb == b;
            case string s: return read is string rs && string.CompareOrdinal(rs, s) == 0;
        }
        return false;
    }
}
