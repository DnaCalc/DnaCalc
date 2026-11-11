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
}
