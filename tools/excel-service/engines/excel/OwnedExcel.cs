using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;

namespace Touchstone.Excel;

/// <summary>
/// One Excel process this worker started and verified (PID and start ticks), a blank scratch
/// book with sheets S (scalar), A (spill) and Q (isolated), manual calculation.
/// Ported from tools/excel-session/host.ps1 Start-Owned: /x creates our process, the object model is
/// bound through that process's own EXCEL7 window, never through COM activation.
/// </summary>
sealed class OwnedExcel
{
    const int ProtectedPid = 3144;   // the owner's Excel in host.ps1; never touched
    public dynamic App;
    public dynamic Book;
    public dynamic SheetS, SheetA, SheetQ;
    dynamic mtc, namesObj, sheetsObj, booksObj;   // child objects kept, so a fingerprint does not resolve them again
    public int Pid;
    public long StartTicks;
    public Process Process;
    public string Baseline;          // fingerprint taken after setup
    public Dictionary<string, object> Env;
    public string EnvId;
    public Dictionary<string, object> Snapshot0;

    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    delegate bool EnumProc(IntPtr hwnd, IntPtr state);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback, IntPtr state);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr hwnd, EnumProc callback, IntPtr state);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd, StringBuilder name, int count);
    [DllImport("oleacc.dll")] static extern int AccessibleObjectFromWindow(IntPtr hwnd, uint objectId, ref Guid iid, [MarshalAs(UnmanagedType.Interface)] out object result);

    static object NativeWindow(int processId)
    {
        object result = null;
        EnumWindows((root, state) =>
        {
            GetWindowThreadProcessId(root, out uint owner);
            if (owner != processId) return true;
            EnumChildWindows(root, (child, unused) =>
            {
                GetWindowThreadProcessId(child, out uint childOwner);
                if (childOwner != processId) return true;
                var name = new StringBuilder(128);
                GetClassName(child, name, 128);
                if (name.ToString() == "EXCEL7")
                {
                    var iid = new Guid("00020400-0000-0000-C000-000000000046");
                    if (AccessibleObjectFromWindow(child, 0xFFFFFFF0, ref iid, out object value) >= 0 && value != null) { result = value; return false; }
                }
                return true;
            }, IntPtr.Zero);
            return result == null;
        }, IntPtr.Zero);
        return result;
    }

    static string FindExcel()
    {
        var candidates = new[]
        {
            Path.Combine(Environment.GetEnvironmentVariable("ProgramFiles") ?? "", "Microsoft Office/root/Office16/EXCEL.EXE"),
            Path.Combine(Environment.GetEnvironmentVariable("ProgramFiles(x86)") ?? "", "Microsoft Office/root/Office16/EXCEL.EXE"),
            Path.Combine(Environment.GetEnvironmentVariable("ProgramFiles") ?? "", "Microsoft Office/Office16/EXCEL.EXE"),
        };
        return candidates.FirstOrDefault(File.Exists) ?? throw new InvalidOperationException("Excel executable not found.");
    }

    public static bool IdentityHolds(int pid, long ticks)
    {
        try
        {
            using var p = Process.GetProcessById(pid);
            return p.ProcessName.Equals("EXCEL", StringComparison.OrdinalIgnoreCase) && p.StartTime.ToUniversalTime().Ticks == ticks;
        }
        catch { return false; }
    }

    /// <summary>Terminates only the process this object started; refuses on any identity mismatch.</summary>
    public static string KillOwned(int pid, long ticks)
    {
        if (pid == 0 || pid == ProtectedPid) return "no owned Excel";
        try
        {
            using var p = Process.GetProcessById(pid);
            if (!p.ProcessName.Equals("EXCEL", StringComparison.OrdinalIgnoreCase) || p.StartTime.ToUniversalTime().Ticks != ticks)
                return "identity changed; termination refused";
            p.Kill();
            return p.WaitForExit(5000) ? "owned Excel terminated" : "owned Excel termination pending";
        }
        catch (ArgumentException) { return "already exited"; }
        catch (Exception e) { return e.GetType().Name; }
    }

    static object Get(object o, string name) => o.GetType().InvokeMember(name, System.Reflection.BindingFlags.GetProperty, null, o, null);

    public static OwnedExcel Start(string root, string name, int launch, int mtThreads, int startupSeconds)
    {
        var before = Process.GetProcessesByName("EXCEL").Select(p => p.Id).ToHashSet();
        var launches = Path.Combine(root, "launches");
        Directory.CreateDirectory(launches);
        // An empty CSV supplies the worksheet child needed for native object-model binding.
        var bootstrap = Path.Combine(launches, name + "-" + launch + "-bootstrap.csv");
        File.WriteAllText(bootstrap, "", new UTF8Encoding(false));
        var info = new ProcessStartInfo(FindExcel(), "/x /automation /r \"" + bootstrap + "\"")
        {
            UseShellExecute = false,
            CreateNoWindow = true,
            WindowStyle = ProcessWindowStyle.Hidden,
        };
        var process = Process.Start(info);
        int pid = process.Id;
        if (pid == ProtectedPid || before.Contains(pid) || !process.ProcessName.Equals("EXCEL", StringComparison.OrdinalIgnoreCase))
            throw new InvalidOperationException($"Excel launch is not owned ({pid}); it will not be quit or terminated.");
        long ticks = process.StartTime.ToUniversalTime().Ticks;
        var x = new OwnedExcel { Pid = pid, StartTicks = ticks, Process = process };
        try
        {
            var started = DateTime.UtcNow;
            object window = null;
            while (window == null)
            {
                if (process.HasExited) throw new InvalidOperationException("Owned Excel exited during startup.");
                if ((DateTime.UtcNow - started).TotalSeconds > startupSeconds)
                    throw new InvalidOperationException("Owned Excel did not expose its worksheet native object model.");
                window = NativeWindow(pid);
                if (window == null) Thread.Sleep(50);
            }
            dynamic win = window;
            dynamic app = win.Application;
            GetWindowThreadProcessId((IntPtr)(int)app.Hwnd, out uint bound);
            if (bound != pid || process.StartTime.ToUniversalTime().Ticks != ticks)
                throw new InvalidOperationException("Native object model belongs to another process; no application mutation attempted.");
            x.App = app;
            x.Setup(mtThreads);
        }
        catch
        {
            KillOwned(pid, ticks);
            throw;
        }
        return x;
    }

    void Setup(int mtThreads)
    {
        dynamic a = App;
        a.DisplayAlerts = false;
        a.EnableEvents = false;
        while ((int)a.Workbooks.Count > 0) a.Workbooks.Item(1).Close(false);
        Book = a.Workbooks.Add(-4167);
        SheetS = Book.Worksheets.Item(1);
        SheetS.Name = "S";
        SheetA = Book.Worksheets.Add(Type.Missing, SheetS);
        SheetA.Name = "A";
        SheetQ = Book.Worksheets.Add(Type.Missing, SheetA);
        SheetQ.Name = "Q";
        a.Calculation = -4135;
        a.Iteration = false;
        a.MaxIterations = 100;
        a.MaxChange = 0.001;
        a.CalculateBeforeSave = false;
        a.Visible = false;
        a.ScreenUpdating = false;
        a.Interactive = false;
        a.StatusBar = false;
        if (mtThreads > 0)
        {
            a.MultiThreadedCalculation.Enabled = true;
            a.MultiThreadedCalculation.ThreadMode = 1;
            a.MultiThreadedCalculation.ThreadCount = mtThreads;
        }
        else a.MultiThreadedCalculation.Enabled = false;
        // Same refusal as the witness worker: an open add-in or registered XLL function makes the instance not isolated.
        // Connected COM add-ins and installed (not open) add-ins are recorded in the environment entry.
        var loaded = new List<string>();
        foreach (dynamic c in a.AddIns2) if ((bool)c.IsOpen) loaded.Add("add-in open:" + (string)c.FullName);
        object registered = a.RegisteredFunctions;
        if (registered != null && registered is not DBNull) loaded.Add("XLL functions are registered");
        if (loaded.Count > 0) throw new InvalidOperationException("this Excel instance is not isolated: " + string.Join("; ", loaded));
        mtc = a.MultiThreadedCalculation; namesObj = Book.Names; sheetsObj = Book.Worksheets; booksObj = a.Workbooks;
        Snapshot0 = FullSnapshot();
        Baseline = Fingerprint();
        Env = BuildEnv(mtThreads);
        EnvId = Convert.ToHexString(SHA256.HashData(Encoding.ASCII.GetBytes(Json.Write(Env)))).ToLowerInvariant().Substring(0, 12);
        // Disclose the running state once, so a first batch starts from a verified fingerprint.
        GC.Collect();
    }

    static string Show(object v) => v switch { null => "null", double d => BitConverter.DoubleToInt64Bits(d).ToString("x16"), _ => v.ToString() };

    static readonly string[] QuickKeys = { "Calculation", "Iteration", "mtc", "Names", "Sheets", "Workbooks", "AutomationSecurity" };

    /// <summary>
    /// Sixteen cheap properties, compared after every batch (the full set only every 20th batch and after idle;
    /// the quick subset in between, because a COM read of a busy Excel can cost several milliseconds).
    /// </summary>
    public string Fingerprint(bool quick = false)
    {
        dynamic a = App;
        var sb = new StringBuilder();
        void P(string n, object v) => sb.Append(n).Append('=').Append(Show(v)).Append(';');
        P("Calculation", (object)a.Calculation);
        P("Iteration", (object)a.Iteration);
        if (!quick)
        {
            P("MaxIterations", (object)a.MaxIterations);
            P("MaxChange", (object)(double)a.MaxChange);
            P("PrecisionAsDisplayed", (object)Book.PrecisionAsDisplayed);
            P("Date1904", (object)Book.Date1904);
            P("CompatibilityVersion", (object)Book.CompatibilityVersion);
            P("UseSystemSeparators", (object)a.UseSystemSeparators);
            P("DecimalSeparator", (object)a.DecimalSeparator);
            P("ThousandsSeparator", (object)a.ThousandsSeparator);
        }
        P("mtc", (object)mtc.Enabled);
        if (!quick) P("mtc_mode", (object)mtc.ThreadMode);
        P("Names", VisibleNames());
        P("Sheets", (object)sheetsObj.Count);
        P("Workbooks", (object)booksObj.Count);
        P("AutomationSecurity", (object)a.AutomationSecurity);
        return sb.ToString();
    }

    /// <summary>The baseline restricted to the quick subset.</summary>
    public string QuickBaseline()
    {
        var parts = Baseline.Split(';', StringSplitOptions.RemoveEmptyEntries).Where(p => QuickKeys.Contains(p.Substring(0, p.IndexOf('='))));
        return string.Concat(parts.Select(p => p + ";"));
    }

    // Excel adds hidden _xlfn.* names for future functions; those are not state leakage.
    int VisibleNames()
    {
        dynamic names = namesObj;
        int count = (int)names.Count, visible = 0;
        if (count == 0) return 0;
        for (int i = 1; i <= count; i++)
        {
            string n = (string)names.Item(i).Name;
            if (!n.Contains("_xlfn", StringComparison.OrdinalIgnoreCase) && !n.Contains("_xlws", StringComparison.OrdinalIgnoreCase)) visible++;
        }
        return visible;
    }

    /// <summary>The full snapshot of host.ps1 Snapshot (add-ins, registered functions); start and idle only.</summary>
    public Dictionary<string, object> FullSnapshot()
    {
        dynamic a = App;
        var s = new Dictionary<string, object>();
        foreach (var key in new[] { "Visible", "DisplayAlerts", "ScreenUpdating", "EnableEvents", "Interactive", "AutomationSecurity", "Calculation", "Iteration", "MaxIterations", "UseSystemSeparators", "DecimalSeparator", "ThousandsSeparator", "CalculateBeforeSave" })
            s[key] = Get((object)a, key);
        s["mtc_enabled"] = (object)a.MultiThreadedCalculation.Enabled;
        s["mtc_mode"] = (object)a.MultiThreadedCalculation.ThreadMode;
        s["mtc_count"] = (object)a.MultiThreadedCalculation.ThreadCount;
        var com = new List<string>();
        foreach (dynamic c in a.COMAddIns) com.Add((string)c.ProgId + "|" + (bool)c.Connect);
        com.Sort(StringComparer.Ordinal);
        s["com_addins"] = com;
        var add = new List<string>();
        foreach (dynamic c in a.AddIns2) add.Add((string)c.FullName + "|" + (bool)c.Installed + "|" + (bool)c.IsOpen);
        add.Sort(StringComparer.Ordinal);
        s["addins"] = add;
        object reg = a.RegisteredFunctions;
        s["registered_functions"] = reg is Array arr ? arr.Cast<object>().Select(o => o?.ToString()).ToList() : new List<string>();
        s["workbooks"] = (object)a.Workbooks.Count;
        return s;
    }

    public bool SnapshotDrifted()
    {
        var now = FullSnapshot();
        foreach (var key in new[] { "AutomationSecurity", "Calculation", "Iteration", "EnableEvents", "DisplayAlerts", "com_addins", "addins", "registered_functions", "workbooks", "mtc_enabled", "mtc_mode" })
            if (Json.Write(now[key]) != Json.Write(Snapshot0[key])) return true;
        return false;
    }

    static string Machine(string path)
    {
        using var stream = File.OpenRead(path);
        using var reader = new BinaryReader(stream);
        stream.Position = 0x3C;
        stream.Position = reader.ReadInt32() + 4;
        return reader.ReadUInt16() switch { 0x8664 => "x86-64", 0x014C => "x86", 0xAA64 => "arm64", var m => "machine-" + m.ToString("x4") };
    }

    Dictionary<string, object> BuildEnv(int mtThreads)
    {
        dynamic a = App;
        string path = Process.MainModule.FileName;
        var fv = FileVersionInfo.GetVersionInfo(path);
        var e = new Dictionary<string, object>
        {
            ["engine"] = "excel",
            ["version"] = Convert.ToString((object)a.Version, System.Globalization.CultureInfo.InvariantCulture),
            ["build"] = Convert.ToString((object)a.Build, System.Globalization.CultureInfo.InvariantCulture),
            ["file_version"] = $"{fv.FileMajorPart}.{fv.FileMinorPart}.{fv.FileBuildPart}.{fv.FilePrivatePart}",
            ["machine"] = Machine(path),
            ["os"] = Environment.OSVersion.VersionString,
            ["cpu_id"] = Environment.GetEnvironmentVariable("PROCESSOR_IDENTIFIER") ?? "",
            ["logical_cpus"] = Environment.ProcessorCount,
            ["mt_calc"] = mtThreads > 0 ? $"manual:{mtThreads}" : "off",
            ["mt_enabled"] = (bool)a.MultiThreadedCalculation.Enabled,
            ["mt_threads_reported"] = (int)a.MultiThreadedCalculation.ThreadCount,
            ["calculation"] = (int)a.Calculation,
            ["iteration"] = (bool)a.Iteration,
            ["max_iterations"] = (int)a.MaxIterations,
            ["max_change"] = BitConverter.DoubleToInt64Bits((double)a.MaxChange).ToString("x16"),
            ["precision_as_displayed"] = (bool)Book.PrecisionAsDisplayed,
            ["date1904"] = (bool)Book.Date1904,
            ["compatibility_version"] = (int)Book.CompatibilityVersion,
            ["use_system_separators"] = (bool)a.UseSystemSeparators,
            ["decimal_separator"] = (string)a.DecimalSeparator,
            ["thousands_separator"] = (string)a.ThousandsSeparator,
            ["automation_security"] = (int)a.AutomationSecurity,
            ["scratch_sheets"] = "S,A,Q",
            ["com_addins_connected"] = ComConnected(),
            ["addins_installed"] = AddinsInstalled(),
        };
        return e;
    }

    List<string> ComConnected()
    {
        var l = new List<string>();
        foreach (dynamic c in App.COMAddIns) if ((bool)c.Connect) l.Add((string)c.ProgId);
        l.Sort(StringComparer.Ordinal);
        return l;
    }

    List<string> AddinsInstalled()
    {
        var l = new List<string>();
        foreach (dynamic c in App.AddIns2) if ((bool)c.Installed) l.Add(Path.GetFileName((string)c.FullName));
        l.Sort(StringComparer.Ordinal);
        return l;
    }

    public void Quit()
    {
        try { App.DisplayAlerts = false; App.EnableEvents = false; App.Quit(); } catch { }
        App = null; Book = null; SheetS = null; SheetA = null; SheetQ = null;
        GC.Collect(); GC.WaitForPendingFinalizers();
        try
        {
            if (!Process.WaitForExit(5000)) KillOwned(Pid, StartTicks);
        }
        catch { KillOwned(Pid, StartTicks); }
    }
}
