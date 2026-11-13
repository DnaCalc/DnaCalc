// SPDX-License-Identifier: MIT
using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using System.Runtime.InteropServices.ComTypes;
using ExcelDna.Integration;

public class Probe : IExcelAddIn
{
    private static IConnectionPoint EventPoint;
    private static int EventCookie;
    private static readonly EventSink Sink = new EventSink();
    private static readonly object LogLock = new object();
    private static string LogPath => Path.Combine(Path.GetDirectoryName(ExcelDnaUtil.XllPath), "fixture.tsv");
    [DllImport("kernel32.dll")] private static extern uint GetCurrentThreadId();
    internal static void Log(string kind, object value)
    {
        lock (LogLock) File.AppendAllText(LogPath, DateTime.UtcNow.ToString("O") + "\t" +
            GetCurrentThreadId() + "\t" + kind + "\t" + Convert.ToString(value).Replace("\r", " ").Replace("\n", " ") + "\n");
    }
    public void AutoOpen()
    {
        Log("AutoOpen", ExcelDnaUtil.XllPath);
        Log("Runtime", RuntimeEnvironment.GetSystemVersion());
        Log("Integration assembly", typeof(ExcelDnaUtil).Assembly.FullName);
        // Observe our own accesses explicitly; debugger records framework dispatch separately.
        dynamic app = ExcelDnaUtil.Application;
        Log("COM Application.Version get", app.Version);
        Log("COM Application.Hwnd get", app.Hwnd);
        Log("COM Workbook identity", "name=" + app.ActiveWorkbook.Name + ";full=" + app.ActiveWorkbook.FullName +
            ";path=" + app.ActiveWorkbook.Path + ";active=" + app.ActiveSheet.Name);
        var container = (IConnectionPointContainer)(object)app;
        Guid events = new Guid("00024413-0000-0000-C000-000000000046");
        container.FindConnectionPoint(ref events, out EventPoint);
        EventPoint.Advise(Sink, out EventCookie);
        Log("COM AppEvents Advise", EventCookie);
        ExcelAsyncUtil.QueueAsMacro(() =>
        {
            Log("QueueAsMacro enter", "startup");
            object result;
            Log("QueueAsMacro xlfCaller rc", XlCall.TryExcel(XlCall.xlfCaller, out result));
            Log("QueueAsMacro xlfCaller value", Describe(result));
            Log("QueueAsMacro exit", "startup");
        });
    }
    public void AutoClose()
    {
        if (EventPoint != null) { EventPoint.Unadvise(EventCookie); EventPoint = null; Log("COM AppEvents Unadvise", EventCookie); }
        Log("AutoClose", "called");
    }
    private static string Describe(object value)
    {
        var reference = value as ExcelReference;
        if (reference != null) return "ExcelReference sheet=" + reference.SheetId + " rows=" +
            reference.RowFirst + ":" + reference.RowLast + " cols=" + reference.ColumnFirst + ":" + reference.ColumnLast;
        return value == null ? "null" : value.GetType().Name + ":" + value;
    }
    [ExcelFunction(Name="X0.Hello")]
    public static object Hello(string name) { Log("Hello", name); return "Hello, " + name; }
    [ExcelFunction(Name="X0.ThreadSafe", IsThreadSafe=true)]
    public static double ThreadSafe(double value)
    {
        // Enough independent work to observe worker-thread dispatch without a timing claim.
        Thread.SpinWait(200000);
        Log("ThreadSafe", value);
        return value * 2;
    }
    [ExcelFunction(Name="X0.Reference", IsMacroType=true)]
    public static object Reference([ExcelArgument(AllowReference=true)] object value)
    {
        Log("Reference argument", Describe(value));
        return value is ExcelReference ? ((ExcelReference)value).GetValue() : value;
    }
    [ExcelFunction(Name="X0.Caller", IsMacroType=true)]
    public static object Caller()
    {
        var caller = XlCall.Excel(XlCall.xlfCaller);
        Log("Caller", Describe(caller));
        return Describe(caller);
    }
    [ExcelFunction(Name="X0.Big")]
    public static object Big()
    {
        var values = new object[1,4096];
        for (int i=0; i<4096; ++i) values[0,i] = (double)i;
        // Keep the mixed-array marshaler in use (numeric-only arrays use a different allocator).
        values[0,0] = "marker";
        return values;
    }
    [ExcelFunction(Name="X0.Async")]
    public static void Async(double value, ExcelAsyncHandle handle)
    {
        Log("Async enter", value);
        ThreadPool.QueueUserWorkItem(_ =>
        {
            Thread.Sleep(150);
            Log("Async SetResult", handle.SetResult(value + 1));
        });
    }
    [ExcelCommand(Name="X0.Queue")]
    public static void Queue()
    {
        Log("Queue request", "command");
        ExcelAsyncUtil.QueueAsMacro(() => Log("Queue executed", XlCall.Excel(XlCall.xlGetName)));
    }
    [ExcelCommand(Name="X0.Context")]
    public static void Context()
    {
        foreach (int fn in new[] { XlCall.xlGetName, XlCall.xlStack, XlCall.xlfCaller })
        {
            object result;
            Log("Context " + fn, XlCall.TryExcel(fn, out result) + " " + Describe(result));
        }
        ThreadPool.QueueUserWorkItem(_ =>
        {
            foreach (int fn in new[] { XlCall.xlGetName, XlCall.xlStack, XlCall.xlfCaller })
            {
                object result;
                Log("Outside " + fn, XlCall.TryExcel(fn, out result) + " " + Describe(result));
            }
        });
    }
}

[ComVisible(true), Guid("00024413-0000-0000-C000-000000000046"), InterfaceType(ComInterfaceType.InterfaceIsIDispatch)]
public interface IProbeEvents
{
    [DispId(0x61c)] void SheetChange([MarshalAs(UnmanagedType.IDispatch)] object sheet, [MarshalAs(UnmanagedType.IDispatch)] object target);
    [DispId(0x61b)] void SheetCalculate([MarshalAs(UnmanagedType.IDispatch)] object sheet);
    [DispId(0xa34)] void AfterCalculate();
}
[ComVisible(true), ClassInterface(ClassInterfaceType.None)]
public class EventSink : IProbeEvents
{
    private int _sheetChanges;
    public void SheetChange(object sheet, object target)
    {
        dynamic sh = sheet; dynamic range = target;
        Probe.Log("AppEvents SheetChange Target", "book=" + sh.Parent.Name + ";full=" + sh.Parent.FullName +
            ";path=" + sh.Parent.Path + ";sheet=" + sh.Name +
            ";target=" + range.Address + ";count=" + range.Count);
        if (Interlocked.Increment(ref _sheetChanges) == 1)
        {
            Probe.Log("AppEvents SheetChange", "sheet=" + sh.Name + ";target=" + range.Address);
        }
        else Probe.Log("AppEvents SheetChange", "received");
    }
    public void SheetCalculate(object sheet) { Probe.Log("AppEvents SheetCalculate", "sheet"); }
    public void AfterCalculate() { Probe.Log("AppEvents AfterCalculate", "called"); }
}
