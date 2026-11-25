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
    static readonly List<string> Addins = new();   // --open-addin: add-in files this worker's Excel opens (an environment variant)
    static long DeadlineTicks;          // watchdog: 0 = none
    static bool WatchdogFired;
    static int Batches;
    static string Stage = "";
    static long LastVerified;

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
                case "--open-addin": Addins.Add(args[i + 1]); break;
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
            // Test hook: the named worker's first start fails, so the pool's "down, no retry loop" path can be exercised.
            if (Environment.GetEnvironmentVariable("TOUCHSTONE_TEST_FAIL_START") == Name)
                throw new InvalidOperationException("start refused by TOUCHSTONE_TEST_FAIL_START");
            X = OwnedExcel.Start(Root, Name, Launch, Mt, StartupSeconds, Addins);
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
        if (t[0] != 'a' && t[0] != 'r' && t[0] != 'm') return 1;
        var p = t.Substring(1).Split('x');
        return int.Parse(p[0]) * int.Parse(p[1]);
    }

    static double Bits(object hex) => BitConverter.UInt64BitsToDouble(ulong.Parse((string)hex, NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture));
    static string HexOf(double d) => BitConverter.DoubleToUInt64Bits(d).ToString("x16");
    static bool IsHex16(string s) => s.Length == 16;

    const int MaxFormulaChars = 8192;

    sealed class RefusedFormulaException : Exception
    {
        public RefusedFormulaException(string message) : base(message) { }
    }

    static readonly Dictionary<string, (int Min, int Max, HashSet<int> Excluded)> FormulaArities = LoadFormulaArities();

    static Dictionary<string, (int Min, int Max, HashSet<int> Excluded)> LoadFormulaArities()
    {
        using var source = typeof(Program).Assembly.GetManifestResourceStream("admission-families.tsv");
        using var reader = new StreamReader(source);
        var arities = new Dictionary<string, (int, int, HashSet<int>)>(StringComparer.OrdinalIgnoreCase);
        while (reader.ReadLine() is string line)
        {
            string[] columns = line.Split('\t');
            var excluded = columns.Length == 5 ? columns[4].Split(',').Select(int.Parse).ToHashSet() : new HashSet<int>();
            arities.Add(columns[1], (int.Parse(columns[2]), int.Parse(columns[3]), excluded));
        }
        return arities;
    }

    /// <summary>
    /// Refuses formula shapes Excel rejects at entry (which make Range.Formula2R1C1 = object[,]
    /// throw E_OUTOFMEMORY / OutOfMemoryException) before touching Excel COM.
    /// </summary>
    static void ValidateGroupArity(string function, int arity)
    {
        string name = function;
        if (name.StartsWith("_XLFN.", StringComparison.OrdinalIgnoreCase)) name = name[6..];
        if (name.StartsWith("_XLWS.", StringComparison.OrdinalIgnoreCase)) name = name[6..];
        if (FormulaArities.TryGetValue(name, out var shape)
            && (arity < shape.Min || arity > shape.Max || shape.Excluded.Contains(arity)))
            throw new RefusedFormulaException($"{function} refuses {arity} arguments at Excel formula entry");
    }

    static void ValidateFormulaLength(string place, string formula)
    {
        if (formula.Length > MaxFormulaChars)
            throw new RefusedFormulaException($"{place} generated formula of {formula.Length} chars (limit {MaxFormulaChars})");
    }

    static void ValidateBatchFormulas(List<Group> groups, bool spill)
    {
        foreach (var g in groups)
        {
            ValidateGroupArity(g.F, g.Tokens.Length);
            bool perRow = g.Tokens.Any(tk => tk == "S" || tk == "B");
            int checkRows = perRow ? g.N : Math.Min(1, g.N);
            // Without literals the longest spill formula is the one with the largest row number.
            int first = spill && !perRow ? g.N - checkRows : 0;
            for (int i = first; i < first + checkRows; i++)
            {
                string f = spill ? SpillFormula(g, g.Row0 + i) : Formula(g, i);
                ValidateFormulaLength(g.F, f);
            }
        }
    }

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

    /// <summary>Merges regions of one column that touch vertically into single runs.</summary>
    static List<(int r, int c, int n, int m)> MergeRuns(List<(int r, int c, int n, int m)> regions)
    {
        var merged = new List<(int r, int c, int n, int m)>();
        foreach (var x in regions.OrderBy(x => x.c).ThenBy(x => x.r))
        {
            if (merged.Count > 0)
            {
                var last = merged[^1];
                if (last.c == x.c && last.m == x.m && last.r + last.n == x.r) { merged[^1] = (last.r, last.c, last.n + x.n, last.m); continue; }
            }
            merged.Add(x);
        }
        return merged;
    }

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
        double gcBefore = GC.GetTotalPauseDuration().TotalMilliseconds;
        var result = new Dictionary<string, object> { ["id"] = id, ["env_id"] = X.EnvId, ["worker"] = Name, ["batch"] = ++Batches };
        try
        {
            // Drift before: recycle and report; the broker re-queues nothing it has not been told about.
            // The previous batch ended with a verified fingerprint; only after an idle spell is it read again.
            var before = DateTime.UtcNow.Ticks - LastVerified > TimeSpan.TicksPerSecond * 5 ? X.Fingerprint() : X.Baseline;
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
            var groupList = (List<object>)req["groups"];
            var cases = groupList.Cast<Dictionary<string, object>>().Any(g => ((string)g["k"]).Split(',').Any(t => t == "L"));
            var special = groupList.Cast<Dictionary<string, object>>().Any(g => ((string)g["k"]).Split(',').Any(t => t == "f" || t.StartsWith("r") || t.StartsWith("m")));
            if ((special || cases) && serial) throw new InvalidOperationException("range/file arguments and sheet cases are not supported in serial mode");
            var groups = cases ? EvaluateCases(groupList, timing)
                : special ? EvaluateSpecial(groupList, echo, timing)
                : serial ? EvaluateSerial(groupList, echo, timing)
                : groupList.Count > 0 && ((Dictionary<string, object>)groupList[0]).ContainsKey("spill") ? EvaluateSpill(groupList, echo, timing)
                : EvaluateBatch(groupList, echo, false, timing);
            if (serial) result["order"] = "serial";
            Interlocked.Exchange(ref DeadlineTicks, 0);
            var fpWatch = Stopwatch.StartNew();
            bool quick = Batches % 20 != 0;
            var after = X.Fingerprint(quick);
            var expected = quick ? X.QuickBaseline() : X.Baseline;
            timing["fingerprint_ms"] = Math.Round(fpWatch.Elapsed.TotalMilliseconds, 3);
            LastVerified = DateTime.UtcNow.Ticks;
            if (after != expected)
            {
                result["error"] = "fingerprint-drift-after";
                result["detail"] = "expected " + expected + " got " + after;
                Send('R', Json.Write(result));
                Recycle("drift after");
                return;
            }
            result["groups"] = groups;
            timing["total_ms"] = Math.Round(sw.Elapsed.TotalMilliseconds, 3);
            timing["gc_pause_ms"] = Math.Round(GC.GetTotalPauseDuration().TotalMilliseconds - gcBefore, 3);
            result["timing"] = timing;
            Send('R', Json.Write(result));
        }
        catch (RefusedFormulaException e)
        {
            Interlocked.Exchange(ref DeadlineTicks, 0);
            result["error"] = "refused";
            result["detail"] = e.Message;
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

    // Asynchronous functions (IMPORTCSV, IMPORTTEXT, IMAGE) answer #GETTING_DATA (CVErr 2043) while their
    // query runs, and CalculationState stays xlPending (2). Spinning on the state never lets the query
    // finish and ran into the deadline (dc-4rsw). Wait as the host-function import job does: ask Excel to
    // finish async queries, up to 30 s, then read whatever the cell holds.
    const int GettingData = -2146826237;
    static void WaitForAsyncResult(dynamic cell)
    {
        // Stop 3 s before the batch deadline so the answer (#GETTING_DATA if still pending) is returned
        // instead of the watchdog ending Excel.
        long deadline = Interlocked.Read(ref DeadlineTicks);
        var until = DateTime.UtcNow.AddSeconds(30);
        if (deadline > 0) { var d = new DateTime(deadline, DateTimeKind.Utc).AddSeconds(-3); if (d < until) until = d; }
        while (true)
        {
            int state = (int)X.App.CalculationState;
            object v = cell.Value2;
            bool getting = v is int code && code == GettingData;
            if (state == 0 && !getting) return;
            if (DateTime.UtcNow > until) return;
            if (state == 1) { Thread.Sleep(0); continue; }
            try { X.App.CalculateUntilAsyncQueriesDone(); } catch { }
            Thread.Sleep(20);
        }
    }

    // The range/file path is deliberately call-at-a-time: each reference owns a real
    // block on Q, and a file exists only for the duration of the calculation.
    static List<object> EvaluateSpecial(List<object> groupList, bool echo, Dictionary<string, object> timing)
    {
        var output = new List<object>();
        dynamic sheet = X.SheetS, ranges = X.SheetQ, grid = X.SheetA;
        foreach (Dictionary<string, object> raw in groupList)
        {
            string[] tokens = ((string)raw["k"]).Length == 0 ? Array.Empty<string>() : ((string)raw["k"]).Split(',');
            ValidateGroupArity((string)raw["f"], tokens.Length);
            int width = tokens.Sum(TokenWidth), count = (int)(long)raw["n"];
            var values = (List<object>)raw["v"];
            var types = new StringBuilder(); var results = new List<object>();
            var inputTypes = new StringBuilder(); var inputs = new List<object>();
            bool spill = raw.TryGetValue("spill", out var sv) && sv is true;
            for (int i = 0; i < count; i++)
            {
                var refs = new List<string>(); var checks = new List<(object expected, object target, string fileContent)>();
                var files = new List<string>(); int offset = 0, rangeCol = 1;
                var callTypes = new StringBuilder(); var callInputs = new List<object>();
                try
                {
                    foreach (string token in tokens)
                    {
                        int w = TokenWidth(token); int start = i * width + offset;
                        if (token[0] == 'r' || token[0] == 'm')
                        {
                            var dims = token.Substring(1).Split('x'); int nr = int.Parse(dims[0]), nc = int.Parse(dims[1]);
                            if (token[0] == 'r') refs.Add("Q!$" + ColName(rangeCol) + "$1:$" + ColName(rangeCol + nc - 1) + "$" + nr);
                            else
                            {
                                string indices = "{" + string.Join(";", Enumerable.Range(0, nr).Select(r => string.Join(",", Enumerable.Range(0, nc).Select(c => r * nc + c + 1)))) + "}";
                                refs.Add("CHOOSE(" + indices + "," + string.Join(",", Enumerable.Range(0, w).Select(k => "S!$" + ColName(2 + offset + k) + "$1")) + ")");
                            }
                            for (int k = 0; k < w; k++)
                            {
                                var cell = (Dictionary<string, object>)values[start + k];
                                dynamic target = token[0] == 'r' ? Rng(ranges, 1 + k / nc, rangeCol + k % nc, 1 + k / nc, rangeCol + k % nc)
                                                                   : Rng(sheet, 1, 2 + offset + k, 1, 2 + offset + k);
                                object expected = WriteTyped(target, cell);
                                checks.Add((expected, (object)target, null));
                            }
                            if (token[0] == 'r') rangeCol += nc;
                        }
                        else if (token == "f")
                        {
                            string dir = Path.Combine(Root, "launches", Name + "-" + Launch + "-files");
                            Directory.CreateDirectory(dir);
                            string path = Path.Combine(dir, Guid.NewGuid().ToString("N") + ".txt");
                            string content = (string)values[start]; File.WriteAllText(path, content, new UTF8Encoding(false)); files.Add(path);
                            dynamic target = Rng(sheet, 1, 2 + offset, 1, 2 + offset); target.NumberFormat = "@"; target.Value2 = path;
                            checks.Add((path, (object)target, content)); refs.Add("S!$" + ColName(2 + offset) + "$1");
                        }
                        else
                        {
                            for (int k = 0; k < w; k++)
                            {
                                object v = values[start + k];
                                var cell = new Dictionary<string, object>();
                                char kind = token[0] == 'a' ? 'n' : token[0];
                                if (kind == 'n') { cell["type"] = "number"; cell["bits"] = v; }
                                else if (kind == 's' || kind == 'S') { cell["type"] = "text"; cell["value"] = v; }
                                else if (kind == 'b' || kind == 'B') { cell["type"] = "boolean"; cell["value"] = v; }
                                else if (kind == 'e') { cell["type"] = "error"; cell["code"] = v; }
                                else cell["type"] = "blank";
                                dynamic target = Rng(sheet, 1, 2 + offset + k, 1, 2 + offset + k);
                                object expected = WriteTyped(target, cell); checks.Add((expected, (object)target, null));
                            }
                            string Cell(int k) => "S!$" + ColName(2 + offset + k) + "$1";
                            if (token[0] == 'a')
                            {
                                var dims = token.Substring(1).Split('x'); int nr = int.Parse(dims[0]), nc = int.Parse(dims[1]);
                                string indices = "{" + string.Join(";", Enumerable.Range(0, nr).Select(r => string.Join(",", Enumerable.Range(0, nc).Select(c => r * nc + c + 1)))) + "}";
                                refs.Add("CHOOSE(" + indices + "," + string.Join(",", Enumerable.Range(0, w).Select(Cell)) + ")");
                            }
                            else if (token == "S") refs.Add("\"" + ((string)values[start]).Replace("\"", "\"\"") + "\"");
                            else if (token == "B") refs.Add((bool)values[start] ? "TRUE" : "FALSE");
                            else refs.Add(Cell(0));
                        }
                        offset += w;
                    }
                    string formula = (string)raw["f"] + "(" + string.Join(",", refs) + ")";
                    if (raw.TryGetValue("x", out var selector) && selector != null)
                    {
                        var x = (List<object>)selector; formula = "INDEX(" + formula + "," + x[0] + "," + x[1] + ")";
                    }
                    formula = "=" + formula;
                    ValidateFormulaLength((string)raw["f"], formula);
                    dynamic resultCell = spill ? grid.Range["A1"] : sheet.Range["A1"];
                    Stage = "special formula " + formula;
                    resultCell.Formula2 = formula;
                    Stage = "special calculate " + (string)raw["f"];
                    X.App.Calculate();
                    Stage = "special async wait " + (string)raw["f"];
                    WaitForAsyncResult(resultCell);
                    bool changed = false;
                    foreach (var check in checks)
                    {
                        dynamic target = check.target; object actual = target.Value2;
                        if (!Same(check.expected, actual)) changed = true;
                        if (check.fileContent != null)
                        {
                            string content = File.ReadAllText((string)check.expected, Encoding.UTF8);
                            if (content != check.fileContent) changed = true;
                            callTypes.Append('t'); callInputs.Add(content);
                        }
                        else { var (l, v) = Convert1(actual); callTypes.Append(l); callInputs.Add(v); }
                    }
                    if (changed)
                    {
                        types.Append('u'); results.Add(new Dictionary<string, object> { ["reason"] = "input-changed", ["read_back"] = new List<object>() });
                    }
                    else if (spill && !((object)resultCell.Value2 is int) && (bool)resultCell.HasSpill)
                    {
                        object body = resultCell.SpillingToRange.Value2;
                        var a = body as object[,]; int nr = a?.GetLength(0) ?? 1, nc = a?.GetLength(1) ?? 1;
                        var ty = new StringBuilder(); var vv = new List<object>();
                        if (nr > 64 || nc > 64) { types.Append('u'); results.Add(new Dictionary<string, object> { ["reason"] = "spill-larger-than-the-bound", ["read_back"] = new List<object>() }); }
                        else { if (a == null) { var (l, v) = Convert1(body); ty.Append(l); vv.Add(v); } else foreach (object cell in a) { var (l, v) = Convert1(cell); ty.Append(l); vv.Add(v); } types.Append('m'); results.Add(new Dictionary<string, object> { ["r"] = nr, ["c"] = nc, ["ty"] = ty.ToString(), ["v"] = vv }); }
                    }
                    else
                    {
                        object actual = resultCell.Value2; var (l, v) = Convert1(actual);
                        if (spill && actual is not int)
                        {
                            types.Append('m'); results.Add(new Dictionary<string, object> { ["r"] = 1, ["c"] = 1, ["ty"] = l.ToString(), ["v"] = new List<object> { v } });
                        }
                        else { types.Append(l); results.Add(v); }
                    }
                    inputTypes.Append(callTypes); inputs.AddRange(callInputs);
                }
                finally
                {
                    try { sheet.UsedRange.Clear(); }
                    finally
                    {
                        try { ranges.UsedRange.Clear(); if (spill) grid.UsedRange.Clear(); }
                        finally { foreach (string file in files) File.Delete(file); }
                    }
                }
            }
            var result = new Dictionary<string, object> { ["ty"] = types.ToString(), ["v"] = results };
            if (echo) { result["ity"] = inputTypes.ToString(); result["iv"] = inputs; }
            output.Add(result);
        }
        return output;
    }

    // ----- sheet cases (token L) ---------------------------------------------------------

    /// <summary>"A1" or "$AB$12" to (row, column), 1-based.</summary>
    static (int row, int col) A1(string text)
    {
        text = text.Replace("$", "");
        int i = 0, col = 0;
        while (i < text.Length && char.IsLetter(text[i])) { col = col * 26 + (char.ToUpperInvariant(text[i]) - 'A' + 1); i++; }
        if (i == 0 || i == text.Length) throw new InvalidOperationException("not an A1 cell: " + text);
        return (int.Parse(text.Substring(i), CultureInfo.InvariantCulture), col);
    }

    /// <summary>"Sheet1!H10" or "'My Sheet'!H10" to the sheet name and the cell.</summary>
    static (string sheet, string cell) Place(string at)
    {
        int i = at.LastIndexOf('!');
        if (i < 0) throw new InvalidOperationException("a probe's place names its sheet: " + at);
        return (at.Substring(0, i).Trim('\'').Replace("''", "'"), at.Substring(i + 1));
    }

    sealed class BuiltLayout
    {
        public dynamic Book;
        public string Key;
        public List<string> Changed = new();          // constants that did not read back as written
        public HashSet<string> Cells = new(StringComparer.OrdinalIgnoreCase);   // "Sheet!A1" of every layout cell
    }

    /// <summary>
    /// Builds a sheet case's workbook: a new workbook of one worksheet, its sheets named and ordered as listed,
    /// constants written per sheet as one Value2 rectangle (text cells formatted as text first, errors entered as
    /// error formulas afterwards), formulas by Formula2, names added, rows and columns hidden; then every constant
    /// read back in one rectangle per sheet. Ported from tools/excel-session/cell-context-job.ps1 (fn1), which did
    /// the same one COM call per cell and one workbook per case.
    /// </summary>
    static BuiltLayout BuildLayout(Dictionary<string, object> c, string key)
    {
        var built = new BuiltLayout { Key = key };
        dynamic book = X.App.Workbooks.Add(-4167);
        built.Book = book;
        var sheets = (List<object>)c["sheets"];
        dynamic previous = null;
        var made = new List<object>();
        for (int s = 0; s < sheets.Count; s++)
        {
            var sh = (Dictionary<string, object>)sheets[s];
            dynamic ws = s == 0 ? book.Worksheets.Item(1) : book.Worksheets.Add(Type.Missing, previous);
            ws.Name = (string)sh["name"];
            previous = ws;
            made.Add(ws);
        }
        var formulas = new List<(dynamic ws, string at, string f)>();
        var reads = new List<(dynamic ws, int r0, int c0, int r1, int c1, List<(int r, int c, object v, string at)> want)>();
        for (int s = 0; s < sheets.Count; s++)
        {
            var sh = (Dictionary<string, object>)sheets[s];
            dynamic ws = made[s];
            string name = (string)sh["name"];
            var cells = sh.TryGetValue("cells", out var cv) && cv is Dictionary<string, object> d ? d : new Dictionary<string, object>();
            var constants = new List<(int r, int c, object v, string at)>();
            var texts = new List<string>();
            var errors = new List<(string at, string code)>();
            foreach (var kv in cells)
            {
                built.Cells.Add(name + "!" + kv.Key.Replace("$", ""));
                var cell = (Dictionary<string, object>)kv.Value;
                var (r, col) = A1(kv.Key);
                switch ((string)cell["type"])
                {
                    case "number": constants.Add((r, col, Bits(cell["bits"]), kv.Key)); break;
                    case "text": constants.Add((r, col, (string)cell["value"], kv.Key)); texts.Add(kv.Key); break;
                    case "boolean": constants.Add((r, col, (bool)cell["value"], kv.Key)); break;
                    case "error":
                        string code = (string)cell["code"];
                        constants.Add((r, col, new ErrorWrapper(Codes[code]), kv.Key));
                        errors.Add((kv.Key, code));
                        break;
                    case "formula": formulas.Add((ws, kv.Key, (string)cell["formula"])); break;
                    default: throw new InvalidOperationException("unsupported layout cell " + kv.Key);
                }
            }
            if (constants.Count == 0) continue;
            int r0 = constants.Min(x => x.r), r1 = constants.Max(x => x.r), c0 = constants.Min(x => x.c), c1 = constants.Max(x => x.c);
            var block = new object[r1 - r0 + 1, c1 - c0 + 1];
            foreach (var x in constants) block[x.r - r0, x.c - c0] = x.v is ErrorWrapper ? null : x.v;
            // A Range of several areas takes one NumberFormat; its text is kept under 255 characters.
            for (int i = 0; i < texts.Count;)
            {
                var part = new List<string>();
                int length = 0;
                while (i < texts.Count && (part.Count == 0 || length + texts[i].Length + 1 < 240)) { part.Add(texts[i]); length += texts[i].Length + 1; i++; }
                ws.Range[string.Join(",", part)].NumberFormat = "@";
            }
            Rng(ws, r0, c0, r1, c1).Value2 = block;
            foreach (var (at, code) in errors) ws.Range[at].Formula = "=" + code;
            reads.Add((ws, r0, c0, r1, c1, constants));
        }
        foreach (var (ws, at, f) in formulas) ws.Range[at].Formula2 = f;
        if (c.TryGetValue("names", out var nv) && nv is Dictionary<string, object> names)
            foreach (var kv in names) book.Names.Add(kv.Key, (string)kv.Value);
        for (int s = 0; s < sheets.Count; s++)
        {
            var sh = (Dictionary<string, object>)sheets[s];
            dynamic ws = made[s];
            if (sh.TryGetValue("hidden_rows", out var hr) && hr is List<object> rows)
                foreach (var r in rows) ws.Rows.Item((int)(long)r).Hidden = true;
            if (sh.TryGetValue("hidden_columns", out var hc) && hc is List<object> cols)
                foreach (var col in cols) ws.Columns.Item((int)(long)col).Hidden = true;
        }
        foreach (var (ws, r0, c0, r1, c1, want) in reads)
        {
            object raw = Rng(ws, r0, c0, r1, c1).Value2;
            foreach (var x in want)
            {
                object back = raw is object[,] m ? m[m.GetLowerBound(0) + x.r - r0, m.GetLowerBound(1) + x.c - c0] : raw;
                if (!Same(x.v, back)) built.Changed.Add((string)ws.Name + "!" + x.at);
            }
        }
        return built;
    }

    static Dictionary<string, object> Matrix(object body)
    {
        var a = body as object[,];
        int nr = a?.GetLength(0) ?? 1, nc = a?.GetLength(1) ?? 1;
        var ty = new StringBuilder(); var vv = new List<object>();
        if (a == null) { var (l, v) = Convert1(body); ty.Append(l); vv.Add(v); }
        else foreach (object cell in a) { var (l, v) = Convert1(cell); ty.Append(l); vv.Add(v); }
        return new Dictionary<string, object> { ["r"] = nr, ["c"] = nc, ["ty"] = ty.ToString(), ["v"] = vv };
    }

    static void ValidateCaseFormulaLengths(Dictionary<string, object> layout)
    {
        foreach (Dictionary<string, object> sheet in (List<object>)layout["sheets"])
        {
            if (sheet.TryGetValue("cells", out var raw) && raw is Dictionary<string, object> cells)
                foreach (var item in cells)
                    if (item.Value is Dictionary<string, object> cell && (string)cell["type"] == "formula")
                        ValidateFormulaLength(item.Key, (string)cell["formula"]);
        }
        if (layout.TryGetValue("names", out var named) && named is Dictionary<string, object> names)
            foreach (var item in names) ValidateFormulaLength(item.Key, (string)item.Value);
        foreach (Dictionary<string, object> probe in (List<object>)layout["probes"])
            ValidateFormulaLength((string)probe["at"], (string)probe["formula"]);
    }

    /// <summary>
    /// Sheet cases: cases with the same sheets and names share one built workbook (rebuilt when the layout changes
    /// or a probe overwrote a layout cell); each case's probes are entered by Formula2 (a refused entry is answered
    /// as refused), calculated together (a full calculation for a layout's first case, then Calculate), read (the value, and the whole spill range when it
    /// is larger than one cell, up to 64 x 64) and cleared before the next case. A case whose constants did not read
    /// back as written is unread. Every workbook is closed unsaved before the batch ends.
    /// </summary>
    static List<object> EvaluateCases(List<object> groupList, Dictionary<string, object> timing)
    {
        var sw = Stopwatch.StartNew();
        double buildMs = 0, calcMs = 0, readMs = 0;
        int layouts = 0, cases = 0;
        BuiltLayout built = null;
        bool fresh = false;
        var output = new List<object>();
        try
        {
            foreach (Dictionary<string, object> raw in groupList)
            {
                if ((string)raw["k"] != "L") throw new InvalidOperationException("a sheet-case group has the one token L");
                int count = (int)(long)raw["n"];
                var values = (List<object>)raw["v"];
                var types = new StringBuilder(); var results = new List<object>();
                for (int i = 0; i < count; i++, cases++)
                {
                    var c = (Dictionary<string, object>)values[i];
                    ValidateCaseFormulaLengths(c);
                    string key = Json.Write(new Dictionary<string, object> { ["sheets"] = c["sheets"], ["names"] = c.TryGetValue("names", out var nm) ? nm : null });
                    var t0 = sw.Elapsed.TotalMilliseconds;
                    if (built == null || built.Key != key)
                    {
                        if (built != null) { built.Book.Close(false); built = null; }
                        Stage = "sheet case layout";
                        built = BuildLayout(c, key);
                        layouts++;
                        fresh = true;
                    }
                    buildMs += sw.Elapsed.TotalMilliseconds - t0;
                    if (built.Changed.Count > 0)
                    {
                        types.Append('u');
                        results.Add(new Dictionary<string, object> { ["reason"] = "input-changed", ["read_back"] = built.Changed.ToList() });
                        continue;
                    }
                    var probes = (List<object>)c["probes"];
                    var placed = new List<dynamic>();
                    var answers = new List<object>();
                    bool overwrote = false;
                    t0 = sw.Elapsed.TotalMilliseconds;
                    foreach (Dictionary<string, object> p in probes)
                    {
                        var (sheet, cell) = Place((string)p["at"]);
                        if (built.Cells.Contains(sheet + "!" + cell.Replace("$", ""))) overwrote = true;
                        Stage = "sheet case probe " + (string)p["formula"];
                        dynamic target = null;
                        try
                        {
                            target = built.Book.Worksheets.Item(sheet).Range[cell];
                            // "entry": "local" types the text as the user's locale reads an entry (FormulaLocal: a
                            // number, date, percentage or currency takes the number format Excel gives it).
                            if (p.TryGetValue("entry", out var en) && en as string == "local") target.FormulaLocal = (string)p["formula"];
                            else target.Formula2 = (string)p["formula"];
                            placed.Add(target); answers.Add(null);
                        }
                        catch (Exception e)
                        {
                            // Excel refused the entry (or the probe names no sheet of the case): an answer, not a failure.
                            placed.Add(null); answers.Add(new Dictionary<string, object> { ["refused"] = e.Message });
                        }
                    }
                    // The first case of a layout calculates the whole workbook; later ones need only what is
                    // dirty: the new probes and whatever the last case's cleared probes fed.
                    Stage = "sheet case calculate";
                    if (fresh) X.App.CalculateFull(); else X.App.Calculate();
                    fresh = false;
                    while ((int)X.App.CalculationState != 0) Thread.Sleep(0);
                    calcMs += sw.Elapsed.TotalMilliseconds - t0;
                    t0 = sw.Elapsed.TotalMilliseconds;
                    for (int k = 0; k < placed.Count; k++)
                    {
                        dynamic target = placed[k];
                        if (target == null) continue;
                        object top = target.Value2;
                        var (l, v) = Convert1(top);
                        var answer = new Dictionary<string, object> { ["ty"] = l.ToString(), ["v"] = v, ["spill"] = null };
                        var probe = (Dictionary<string, object>)probes[k];
                        if (probe.TryGetValue("entry", out var pe) && pe as string == "local") answer["nf"] = (string)target.NumberFormat;
                        if ((bool)target.HasSpill)
                        {
                            dynamic range = target.SpillingToRange;
                            int nr = (int)range.Rows.Count, nc = (int)range.Columns.Count;
                            if (nr > 64 || nc > 64) answer["spill"] = new Dictionary<string, object> { ["r"] = nr, ["c"] = nc, ["ty"] = "", ["v"] = new List<object>(), ["unread"] = "spill-larger-than-the-bound" };
                            else if (nr * nc > 1) answer["spill"] = Matrix((object)range.Value2);
                        }
                        answers[k] = answer;
                    }
                    // Clear, not ClearContents: entering a formula that reads a text-formatted cell gives
                    // the formula cell the @ format, and the next case's entry there would then be text.
                    foreach (var target in placed) if (target != null) target.Clear();
                    readMs += sw.Elapsed.TotalMilliseconds - t0;
                    if (overwrote) { built.Book.Close(false); built = null; }
                    types.Append('c');
                    results.Add(new Dictionary<string, object> { ["probes"] = answers });
                }
                output.Add(new Dictionary<string, object> { ["ty"] = types.ToString(), ["v"] = results });
            }
        }
        finally
        {
            if (built != null) { try { built.Book.Close(false); } catch { } }
        }
        timing["write_ms"] = Math.Round(buildMs, 3);
        timing["calc_ms"] = Math.Round(calcMs, 3);
        timing["read_ms"] = Math.Round(readMs, 3);
        timing["layouts"] = layouts;
        timing["calls"] = cases;
        return output;
    }

    static object WriteTyped(dynamic target, Dictionary<string, object> cell)
    {
        switch ((string)cell["type"])
        {
            case "number": double n = Bits(cell["bits"]); target.Value2 = n; return n;
            case "text": string s = (string)cell["value"]; target.NumberFormat = "@"; target.Value2 = s; return s;
            case "boolean": bool b = (bool)cell["value"]; target.Value2 = b; return b;
            case "error": string e = (string)cell["code"]; target.Formula2 = "=" + e; return new ErrorWrapper(Codes[e]);
            case "blank": return null;
            default: throw new InvalidOperationException("unsupported range cell");
        }
    }

    static List<object> EvaluateBatch(List<object> groupList, bool echo, bool serialCalc, Dictionary<string, object> timing)
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
            };
            g.TokenCol = new int[g.Tokens.Length];
            int w = 0;
            for (int t = 0; t < g.Tokens.Length; t++) { g.TokenCol[t] = w; w += TokenWidth(g.Tokens[t]); }
            g.Width = w;
            maxCols = Math.Max(maxCols, 1 + w);
            groups.Add(g);
        }
        // Layout: small groups first, clustered by which columns hold text (fewer NumberFormat runs and one
        // formula array for all of them), then the large groups, each of which gets one fill assignment.
        const int SmallGroup = 32;
        string Signature(Group g) => string.Concat(g.Tokens.Select(t => t == "s" || t == "S" ? "t" : t == "e" ? "e" : "."));
        var layout = serialCalc ? groups   // serial: the rows keep the order the calls were given in
            : groups.Where(g => g.N < SmallGroup).OrderBy(g => Signature(g), StringComparer.Ordinal)
            .Concat(groups.Where(g => g.N >= SmallGroup)).ToList();
        foreach (var g in layout) { g.Row0 = row; row += g.N; }
        int smallRows = layout.Where(g => g.N < SmallGroup).Sum(g => g.N);
        int totalRows = row - 1;
        if (totalRows > 1000000) throw new InvalidOperationException("batch-too-large");
        ValidateBatchFormulas(groups, false);
        dynamic sheet = X.SheetS;
        var written = new List<Block>();

        // 1. arguments. A batch of modest size is written as ONE Value2 rectangle (null cells stay blank);
        //    text columns get NumberFormat "@" first and error arguments are entered as literals afterwards.
        int argCols = maxCols - 1;
        if (argCols == 0) { /* Nullary calls have no argument rectangle. */ }
        else if ((long)totalRows * argCols <= 2_000_000)
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
            foreach (var (r, c, n, m) in MergeRuns(textRegions)) Rng(sheet, r, 2 + c, r + n - 1, 2 + c + m - 1).NumberFormat = "@";
            var cmp = (object[,])all.Clone();
            var errorRuns = MergeRuns(errorRegions);
            foreach (var (r, c, n, m) in errorRuns)
                for (int i = 0; i < n; i++) all[r - 1 + i, c] = null;
            timing["build_ms"] = Math.Round(sw.Elapsed.TotalMilliseconds, 3);
            Rng(sheet, 1, 2, totalRows, maxCols).Value2 = all;
            timing["value2_ms"] = Math.Round(sw.Elapsed.TotalMilliseconds, 3);
            foreach (var (r, c, n, m) in errorRuns)
            {
                var lit = new object[n, 1];
                for (int i = 0; i < n; i++) lit[i, 0] = "=" + Names[((ErrorWrapper)cmp[r - 1 + i, c]).ErrorCode];
                Rng(sheet, r, 2 + c, r + n - 1, 2 + c).Formula2R1C1 = lit;
            }
            written.Add(new Block { Row = 1, Col = 2, Data = cmp });
        }
        else WriteGroupArguments(sheet, groups, written);
        timing["args_ms"] = Math.Round(sw.Elapsed.TotalMilliseconds, 3);
        // 2. formulas: one array assignment for all small groups, one fill assignment per large group
        if (smallRows > 0)
        {
            var smallFns = string.Join(",", layout.Where(g => g.N < SmallGroup).Select(g => g.F).Distinct());
            Stage = "formulas small groups (" + smallFns + ")";
            var fs = new object[smallRows, 1];
            foreach (var g in layout.Where(g => g.N < SmallGroup))
                for (int i = 0; i < g.N; i++) fs[g.Row0 - 1 + i, 0] = Formula(g, i);
            Rng(sheet, 1, 1, smallRows, 1).Formula2R1C1 = fs;
        }
        foreach (var g in layout.Where(g => g.N >= SmallGroup))
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
        if (serialCalc)
        {
            // One calculation pass over the rows in the order given, on a worker with multithreaded calculation off:
            // a state-sensitive function sees the call before it (measured: each cell calculated alone does not).
            Rng(sheet, 1, 1, totalRows, 1).Calculate();
        }
        else X.App.Calculate();
        if (!serialCalc) while ((int)X.App.CalculationState != 0) Thread.Sleep(0);   // Range.Calculate is synchronous and leaves the state pending
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

    /// <summary>Writes every group's arguments one rectangle per token run (large batches and spill batches).</summary>
    static void WriteGroupArguments(dynamic sheet, List<Group> groups, List<Block> written)
    {
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
    }

    /// <summary>
    /// Array-returning functions: each call's formula sits on sheet A at an anchor of a grid (66 rows and columns
    /// apart, so a spill of up to 64 x 64 never meets its neighbour), its arguments in cells of sheet S, and the
    /// answer is the whole spill range as a matrix; #SPILL! and the other error values are typed answers.
    /// </summary>
    static List<object> EvaluateSpill(List<object> groupList, bool echo, Dictionary<string, object> timing)
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
            };
            g.TokenCol = new int[g.Tokens.Length];
            int w = 0;
            for (int t = 0; t < g.Tokens.Length; t++) { g.TokenCol[t] = w; w += TokenWidth(g.Tokens[t]); }
            g.Width = w;
            g.Row0 = row;
            row += g.N;
            maxCols = Math.Max(maxCols, 1 + w);
            groups.Add(g);
        }
        int totalRows = row - 1;
        if (totalRows > 1000) throw new InvalidOperationException("batch-too-large-for-spill");
        ValidateBatchFormulas(groups, true);
        dynamic sheet = X.SheetS, grid = X.SheetA;
        var written = new List<Block>();
        WriteGroupArguments(sheet, groups, written);
        var anchors = new List<string>();
        int number = 0;
        foreach (var g in groups)
            for (int i = 0; i < g.N; i++, number++)
            {
                string address = ColName(1 + number / 100 * 66) + (1 + number % 100 * 66);
                Stage = "spill formula " + g.F + " " + address;
                grid.Range[address].Formula2 = SpillFormula(g, g.Row0 + i);
                anchors.Add(address);
            }
        timing["write_ms"] = Math.Round(sw.Elapsed.TotalMilliseconds, 3);
        sw.Restart();
        Stage = "calculate";
        X.App.Calculate();
        while ((int)X.App.CalculationState != 0) Thread.Sleep(0);
        timing["calc_ms"] = Math.Round(sw.Elapsed.TotalMilliseconds, 3);
        sw.Restart();
        object raw2 = Rng(sheet, 1, 1, totalRows, maxCols).Value2;
        Func<int, int, object> at;
        if (raw2 is Array arr)
        {
            int lb0 = arr.GetLowerBound(0), lb1 = arr.GetLowerBound(1);
            var mat = (object[,])raw2;
            at = (r, c) => mat[lb0 + r - 1, lb1 + c - 1];
        }
        else at = (r, c) => raw2;
        var changed = new HashSet<int>();
        foreach (var b in written)
            for (int i = 0; i < b.Data.GetLength(0); i++)
                for (int j = 0; j < b.Data.GetLength(1); j++)
                    if (!Same(b.Data[i, j], at(b.Row + i, b.Col + j))) changed.Add(b.Row + i);
        timing["read_ms"] = Math.Round(sw.Elapsed.TotalMilliseconds, 3);
        sw.Restart();
        var output = new List<object>();
        int k = 0;
        foreach (var g in groups)
        {
            var ty = new StringBuilder(g.N);
            var vals = new List<object>(g.N);
            for (int i = 0; i < g.N; i++, k++)
            {
                int r = g.Row0 + i;
                if (changed.Contains(r))
                {
                    var back = new List<object>();
                    for (int j = 0; j < g.Width; j++) { var (l, v) = Convert1(at(r, 2 + j)); back.Add(new List<object> { l.ToString(), v }); }
                    ty.Append('u');
                    vals.Add(new Dictionary<string, object> { ["reason"] = "input-changed", ["read_back"] = back });
                    continue;
                }
                dynamic cell = grid.Range[anchors[k]];
                object top = cell.Value2;
                if (top is int)
                {
                    var (l, v) = Convert1(top);
                    ty.Append(l);
                    vals.Add(v);
                    continue;
                }
                bool spills = (bool)cell.HasSpill;
                object body = spills ? cell.SpillingToRange.Value2 : top;
                int rows = 1, cols = 1;
                Func<int, int, object> cellAt = (a, b) => body;
                if (body is Array ba && ba.Rank == 2)
                {
                    rows = ba.GetLength(0); cols = ba.GetLength(1);
                    int l0 = ba.GetLowerBound(0), l1 = ba.GetLowerBound(1);
                    var bm = (object[,])body;
                    cellAt = (a, b) => bm[l0 + a, l1 + b];
                }
                if (rows > 64 || cols > 64)
                {
                    ty.Append('u');
                    vals.Add(new Dictionary<string, object> { ["reason"] = "spill-larger-than-the-bound", ["read_back"] = new List<object>() });
                    continue;
                }
                var cty = new StringBuilder();
                var cv = new List<object>();
                for (int a = 0; a < rows; a++)
                    for (int b = 0; b < cols; b++) { var (l, v) = Convert1(cellAt(a, b)); cty.Append(l); cv.Add(v); }
                ty.Append('m');
                vals.Add(new Dictionary<string, object> { ["r"] = rows, ["c"] = cols, ["ty"] = cty.ToString(), ["v"] = cv });
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
        Rng(sheet, 1, 1, totalRows, maxCols).Clear();
        grid.UsedRange.Clear();
        timing["convert_ms"] = Math.Round(sw.Elapsed.TotalMilliseconds, 3);
        timing["calls"] = totalRows;
        return output;
    }

    static string SpillFormula(Group g, int row)
    {
        var refs = new List<string>();
        for (int t = 0; t < g.Tokens.Length; t++)
        {
            var tok = g.Tokens[t];
            int at = g.TokenCol[t];
            string Cell(int offset) => "S!$" + ColName(2 + offset) + "$" + row;
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
                refs.Add("CHOOSE(" + idx + "," + string.Join(",", Enumerable.Range(0, r * c).Select(kk => Cell(at + kk))) + ")");
            }
            else if (tok == "S") refs.Add("\"" + ((string)g.V[(row - g.Row0) * g.Width + at]).Replace("\"", "\"\"") + "\"");
            else if (tok == "B") refs.Add((bool)g.V[(row - g.Row0) * g.Width + at] ? "TRUE" : "FALSE");
            else refs.Add(Cell(at));
        }
        var f = g.F + "(" + string.Join(",", refs) + ")";
        if (g.Xsel != null) f = "INDEX(" + f + "," + g.Xsel[0] + "," + g.Xsel[1] + ")";
        return "=" + f;
    }

    /// <summary>
    /// Serial mode: every call becomes its own row, in the order of its request position (primer calls, flagged
    /// "p", first), calculated one Range.Calculate at a time; the answers are regrouped like the request's groups.
    /// </summary>
    static List<object> EvaluateSerial(List<object> groupList, bool echo, Dictionary<string, object> timing)
    {
        var calls = new List<(long pos, Dictionary<string, object> single, int group, int index)>();
        var sizes = new List<int>();
        long primerPosition = long.MinValue / 2;
        foreach (Dictionary<string, object> raw in groupList)
        {
            bool primer = raw.TryGetValue("p", out var flag) && flag is true;
            string k = (string)raw["k"];
            int width = k.Length == 0 ? 0 : k.Split(',').Sum(TokenWidth);
            int n = (int)(long)raw["n"];
            var values = (List<object>)raw["v"];
            for (int i = 0; i < n; i++)
            {
                long pos = primer ? primerPosition++ : raw.TryGetValue("o", out var o) ? (long)o + i : (long)((List<object>)raw["i"])[i];
                var single = new Dictionary<string, object> { ["f"] = raw["f"], ["k"] = k, ["n"] = 1L, ["v"] = values.GetRange(i * width, width) };
                if (raw.TryGetValue("x", out var xs)) single["x"] = xs;
                calls.Add((pos, single, primer ? -1 : sizes.Count, i));
            }
            if (!primer) sizes.Add(n);
        }
        calls.Sort((a, b) => a.pos.CompareTo(b.pos));
        var singles = calls.Select(c => (object)c.single).ToList();
        var answered = EvaluateBatch(singles, echo, true, timing);
        var ty = sizes.Select(n => new char[n]).ToList();
        var val = sizes.Select(n => new object[n]).ToList();
        var ity = sizes.Select(n => new List<char>[n]).ToList();
        var ival = sizes.Select(n => new List<object>[n]).ToList();
        for (int c = 0; c < calls.Count; c++)
        {
            var (_, _, group, index) = calls[c];
            if (group < 0) continue;
            var a = (Dictionary<string, object>)answered[c];
            ty[group][index] = ((string)a["ty"])[0];
            val[group][index] = ((List<object>)a["v"])[0];
            if (echo)
            {
                ity[group][index] = ((string)a["ity"]).ToList();
                ival[group][index] = (List<object>)a["iv"];
            }
        }
        var output = new List<object>();
        for (int g = 0; g < sizes.Count; g++)
        {
            var og = new Dictionary<string, object> { ["ty"] = new string(ty[g]), ["v"] = val[g].ToList() };
            if (echo)
            {
                og["ity"] = new string(ity[g].SelectMany(x => x).ToArray());
                og["iv"] = ival[g].SelectMany(x => x).ToList();
            }
            output.Add(og);
        }
        return output;
    }

    static readonly Dictionary<string, int> Codes = new()
    {
        ["#NULL!"] = 2000, ["#DIV/0!"] = 2007, ["#VALUE!"] = 2015, ["#REF!"] = 2023, ["#NAME?"] = 2029, ["#NUM!"] = 2036, ["#N/A"] = 2042,
    };
    static readonly Dictionary<int, string> Names = Codes.ToDictionary(kv => kv.Value, kv => kv.Key)
        .Concat(new Dictionary<int, string> { [2043] = "#GETTING_DATA", [2044] = "#UNKNOWN!", [2045] = "#SPILL!", [2046] = "#CONNECT!", [2047] = "#BLOCKED!", [2049] = "#FIELD!", [2050] = "#CALC!", [2051] = "#BUSY!" })
        .ToDictionary(kv => kv.Key, kv => kv.Value);

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
