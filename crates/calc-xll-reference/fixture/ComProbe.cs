// SPDX-License-Identifier: MIT
// Answer 13 D.15 (dc-6332.12, x19): an Excel-DNA add-in that reaches Excel's object model the way
// the framework does, once through the primary interop assembly (early bound: the _Application and
// Range vtables) and once late bound (dynamic, IDispatch). Each function returns one string with the
// observations in order, so the same cases compare Excel's and the host's answers (the formula passes
// the calling cell differs per route, so only its shape is reported): Version,
// Application.Caller, Application.Range, Range Address/Count/Rows/Columns and Application.Union.
// No cell is written and no value is read (OneCalc's Range holds none; the Value2 reads are separate
// functions whose answers are only recorded).
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using ExcelDna.Integration;
using Excel = Microsoft.Office.Interop.Excel;

public static class ComProbe
{
    private static string Describe(object v)
    {
        if (v == null) return "null";
        if (Marshal.IsComObject(v)) return "COM-object";
        if (v is Array a) return "array" + a.Rank;
        return v.GetType().Name + ":" + Convert.ToString(v, System.Globalization.CultureInfo.InvariantCulture);
    }
    private static void Observe(List<string> log, string label, Func<object> read)
    {
        try { log.Add(label + "=" + Describe(read())); }
        catch (Exception e) { log.Add(label + "=HRESULT:" + ((uint)e.HResult).ToString("X8")); }
    }
    private static readonly object M = Type.Missing;
    // The calling cell differs per route (Excel's capture formula sits elsewhere than OneCalc's), so
    // the probe reports the shape of Caller's Address and that Row agrees with it.
    private static string Cell(string address) =>
        System.Text.RegularExpressions.Regex.IsMatch(address, @"^\$[A-Z]{1,3}\$[0-9]+$") ? "single-cell" : address;
    private static int RowOf(string address) => int.Parse(address.Substring(address.LastIndexOf('$') + 1));
    private static string A1(Excel.Range range) =>
        range.get_Address(M, M, Excel.XlReferenceStyle.xlA1, M, M);

    [ExcelFunction(Name = "COM_PIA")]
    public static string Pia()
    {
        var log = new List<string>();
        Excel.Application app = null;
        Observe(log, "acquire", () => { app = (Excel.Application)ExcelDnaUtil.Application; return Marshal.IsComObject(app); });
        if (app == null) return string.Join(";", log);
        Observe(log, "Version", () => app.Version);
        Observe(log, "CallerAddress", () => Cell(A1((Excel.Range)app.Caller[M])));
        Observe(log, "CallerRowMatchesAddress", () => ((Excel.Range)app.Caller[M]).Row == RowOf(A1((Excel.Range)app.Caller[M])));
        Observe(log, "CallerCount", () => ((Excel.Range)app.Caller[M]).Count);
        Observe(log, "RangeAddress", () => A1(app.Range["A1:B2", M]));
        Observe(log, "RangeCount", () => app.Range["A1:B2", M].Count);
        Observe(log, "RowsCount", () => app.Range["A1:B2", M].Rows.Count);
        Observe(log, "ColumnsCount", () => app.Range["A1:B2", M].Columns.Count);
        Observe(log, "BoundingAddress", () => A1(app.Range["D5", "B2"]));
        Observe(log, "UnionAddress", () => A1(app.Union(app.Range["A1:A2", M], app.Range["C3", M])));
        Observe(log, "UnionCount", () => app.Union(app.Range["A1:A2", M], app.Range["C3", M]).Count);
        Observe(log, "BadSheet", () => A1(app.Range["Elsewhere!A1", M]));
        return string.Join(";", log);
    }

    [ExcelFunction(Name = "COM_DYNAMIC")]
    public static string Late()
    {
        var log = new List<string>();
        dynamic app = null;
        Observe(log, "acquire", () => { app = ExcelDnaUtil.Application; return Marshal.IsComObject((object)app); });
        if (app == null) return string.Join(";", log);
        Observe(log, "Version", () => app.Version);
        Observe(log, "CallerAddress", () => Cell((string)app.Caller.Address));
        Observe(log, "CallerRowMatchesAddress", () => (int)app.Caller.Row == RowOf((string)app.Caller.Address));
        Observe(log, "CallerCount", () => app.Caller.Count);
        Observe(log, "RangeAddress", () => app.Range["A1:B2"].Address);
        Observe(log, "RangeCount", () => app.Range["A1:B2"].Count);
        Observe(log, "RowsCount", () => app.Range["A1:B2"].Rows.Count);
        Observe(log, "ColumnsCount", () => app.Range["A1:B2"].Columns.Count);
        Observe(log, "BoundingAddress", () => app.Range["D5", "B2"].Address);
        Observe(log, "UnionAddress", () => app.Union(app.Range["A1:A2"], app.Range["C3"]).Address);
        Observe(log, "UnionCount", () => app.Union(app.Range["A1:A2"], app.Range["C3"]).Count);
        Observe(log, "BadSheet", () => app.Range["Elsewhere!A1"].Address);
        return string.Join(";", log);
    }

    // Recorded, not compared: OneCalc's Range holds no values (ONECALC_ADDINS.md), Excel's does.
    [ExcelFunction(Name = "COM_PIA_VALUE2")]
    public static string PiaValue2()
    {
        var log = new List<string>();
        try
        {
            var app = (Excel.Application)ExcelDnaUtil.Application;
            Observe(log, "Value2", () => app.Range["A1:B2", M].Value2);
        }
        catch (Exception e) { log.Add("acquire=HRESULT:" + ((uint)e.HResult).ToString("X8")); }
        return string.Join(";", log);
    }

    // Full-workbook probe: capture Excel first, then compare through DNA Calc's page.
    [ExcelFunction(Name = "COM_WORKBOOK_VALUES")]
    public static string WorkbookValues()
    {
        var log = new List<string>();
        dynamic app = ExcelDnaUtil.Application;
        Observe(log, "ThisCellMatchesCaller", () => (int)app.ThisCell.Row == (int)app.Caller.Row &&
            (int)app.ThisCell.Column == (int)app.Caller.Column);
        Observe(log, "ValueA1", () => app.Range["A1"].Value);
        Observe(log, "Value2A1", () => app.Range["A1"].Value2);
        Observe(log, "Value2Bits", () => BitConverter.DoubleToInt64Bits((double)app.Range["A1"].Value2).ToString("X16"));
        Observe(log, "ArrayBounds", () => {
            Array values = (Array)app.Range["A1:B2"].Value2;
            return values.GetLowerBound(0) + ":" + values.GetUpperBound(0) + ":" +
                values.GetLowerBound(1) + ":" + values.GetUpperBound(1);
        });
        return string.Join(";", log);
    }

    // D.15 second half (dc-6332.16): COM identity and a second-apartment client. Capture Excel first;
    // the same cases then run through the host. Identity compares canonical IUnknown pointers
    // (QueryInterface for IUnknown, which the RCW holds) within this apartment; nothing crosses
    // processes and nothing is registered.
    [DllImport("oleacc.dll")]
    private static extern int AccessibleObjectFromWindow(IntPtr hwnd, uint objectId, ref Guid iid,
        [MarshalAs(UnmanagedType.IDispatch)] out object result);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string className, string title);
    [DllImport("ole32.dll")]
    private static extern int CoMarshalInterThreadInterfaceInStream(ref Guid iid,
        [MarshalAs(UnmanagedType.IUnknown)] object unknown, out IntPtr stream);
    [DllImport("ole32.dll")]
    private static extern int CoGetInterfaceAndReleaseStream(IntPtr stream, ref Guid iid,
        [MarshalAs(UnmanagedType.IUnknown)] out object result);
    private static readonly Guid IID_IDispatch = new Guid("00020400-0000-0000-C000-000000000046");
    // Excel's _Application (the PIA's Excel._Application): only a registered proxy/stub (installed
    // Office metadata) can marshal it to another apartment.
    private static readonly Guid IID__Application = new Guid("000208D5-0000-0000-C000-000000000046");
    private const uint OBJID_NATIVEOM = 0xFFFFFFF0;

    private static IntPtr Identity(object o)
    {
        IntPtr unknown = Marshal.GetIUnknownForObject(o);
        Marshal.Release(unknown);
        return unknown;
    }
    private static string Hr(int code) => "HRESULT:" + ((uint)code).ToString("X8");
    private static object NativeWindow(dynamic app)
    {
        IntPtr main = new IntPtr((int)app.Hwnd);
        IntPtr desk = FindWindowEx(main, IntPtr.Zero, "XLDESK", null);
        IntPtr book = FindWindowEx(desk, IntPtr.Zero, "EXCEL7", null);
        if (book == IntPtr.Zero) throw new COMException("no EXCEL7 window", unchecked((int)0x80004005));
        Guid iid = IID_IDispatch;
        int code = AccessibleObjectFromWindow(book, OBJID_NATIVEOM, ref iid, out object window);
        if (code != 0) throw new COMException("AccessibleObjectFromWindow", code);
        return window;
    }

    [ExcelFunction(Name = "COM_IDENTITY")]
    public static string ComIdentity()
    {
        var log = new List<string>();
        dynamic app = null;
        Observe(log, "acquire", () => { app = ExcelDnaUtil.Application; return Marshal.IsComObject((object)app); });
        if (app == null) return string.Join(";", log);
        IntPtr appId = Identity((object)app);
        Observe(log, "AppType", () => Microsoft.VisualBasic.Information.TypeName((object)app));
        Observe(log, "AppApplicationIsApp", () => Identity((object)app.Application) == appId);
        Observe(log, "AppParentIsApp", () => Identity((object)app.Parent) == appId);
        object first = null, second = null;
        Observe(log, "Window", () => { first = NativeWindow(app); second = NativeWindow(app); return Marshal.IsComObject(first); });
        if (first != null)
        {
            dynamic window = first;
            Observe(log, "WindowType", () => Microsoft.VisualBasic.Information.TypeName(first));
            Observe(log, "WindowSameTwice", () => Identity(first) == Identity(second));
            Observe(log, "WindowApplicationIsApp", () => Identity((object)window.Application) == appId);
            Observe(log, "ParentType", () => Microsoft.VisualBasic.Information.TypeName((object)window.Parent));
            Observe(log, "ParentApplicationIsApp", () => Identity((object)window.Parent.Application) == appId);
            Observe(log, "WindowIsApp", () => Identity(first) == appId);
        }
        Observe(log, "RangeSameTwice", () => Identity((object)app.Range["A1"]) == Identity((object)app.Range["A1"]));
        return string.Join(";", log);
    }

    // The Application marshalled to a second STA thread through a stream while this (owner) thread
    // waits; the CLR's STA wait pumps COM calls. Late-bound Version goes through IDispatch's
    // system proxy; the typed QueryInterface needs _Application's proxy/stub registration.
    [ExcelFunction(Name = "COM_STA2")]
    public static string ComSecondApartment()
    {
        var log = new List<string>();
        object app = ExcelDnaUtil.Application;
        Guid iid = IID_IDispatch;
        int marshalled = CoMarshalInterThreadInterfaceInStream(ref iid, app, out IntPtr stream);
        log.Add("marshal=" + Hr(marshalled));
        if (marshalled != 0) return string.Join(";", log);
        string unmarshal = "not-run", version = "not-run", typed = "not-run", type = "not-run";
        var thread = new System.Threading.Thread(() =>
        {
            Guid local = IID_IDispatch;
            int code = CoGetInterfaceAndReleaseStream(stream, ref local, out object proxy);
            unmarshal = Hr(code);
            if (code != 0) return;
            try { version = Describe(((dynamic)proxy).Version); }
            catch (Exception e) { version = Hr(e.HResult); }
            try { type = Microsoft.VisualBasic.Information.TypeName(proxy); }
            catch (Exception e) { type = Hr(e.HResult); }
            IntPtr unknown = Marshal.GetIUnknownForObject(proxy);
            Guid application = IID__Application;
            int query = Marshal.QueryInterface(unknown, ref application, out IntPtr typedPointer);
            typed = Hr(query);
            if (query == 0) Marshal.Release(typedPointer);
            Marshal.Release(unknown);
            Marshal.ReleaseComObject(proxy);
        });
        thread.SetApartmentState(System.Threading.ApartmentState.STA);
        thread.IsBackground = true;
        thread.Start();
        bool finished = thread.Join(10000);
        log.Add("finished=" + finished);
        log.Add("unmarshal=" + unmarshal);
        log.Add("Version=" + version);
        log.Add("TypeName=" + type);
        log.Add("QueryApplication=" + typed);
        return string.Join(";", log);
    }
}
