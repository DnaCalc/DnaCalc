// SPDX-License-Identifier: MIT
// Answer 13 D.1's managed status 64 pair (dc-6332.11, x17): Excel-DNA's non-throwing TryExcel
// and throwing Excel calls of xlCoerce on a reference whose producer is not calculated yet,
// ordinary and thread-safe. Each entry appends to a log that UC_LOG returns and UC_RESET clears,
// so the same case list compares Excel's entries and statuses with the hosts'. No Excel or COM
// call is made except the measured xlCoerce.
using System.Collections.Generic;
using ExcelDna.Integration;

public static class UncalcedProbe
{
    private static readonly object Gate = new object();
    private static readonly List<string> Entries = new List<string>();
    private static double? Last;
    private static void Log(string line) { lock (Gate) Entries.Add(line); }
    private static string Shown(object value) =>
        value is double d ? "num:" + d.ToString(System.Globalization.CultureInfo.InvariantCulture)
        : value is ExcelError e ? "err:" + e : value == null ? "null" : value.GetType().Name + ":" + value;

    [ExcelFunction(Name = "UC_RESET", IsMacroType = true)]
    public static double Reset() { lock (Gate) { Entries.Clear(); } return 1; }
    [ExcelFunction(Name = "UC_LOG")]
    public static string LogText() { lock (Gate) return string.Join(";", Entries); }
    /// The producer logs only a changed value (Excel may calculate it twice with one value).
    [ExcelFunction(Name = "UC_LATE")]
    public static double Late(double x)
    {
        lock (Gate) { if (Last != x) { Last = x; Entries.Add("late:" + x.ToString(System.Globalization.CultureInfo.InvariantCulture)); } }
        return x;
    }
    private static object Try(string name, object r)
    {
        object value;
        var rc = XlCall.TryExcel(XlCall.xlCoerce, out value, r);
        Log(name + ":" + (int)rc + (rc == XlCall.XlReturn.XlReturnSuccess ? ":" + Shown(value) : ""));
        return rc == XlCall.XlReturn.XlReturnSuccess ? value : (object)(double)(int)rc;
    }
    private static object Throw(string name, object r)
    {
        try
        {
            var value = XlCall.Excel(XlCall.xlCoerce, r);
            Log(name + ":0:" + Shown(value));
            return value;
        }
        catch (XlCallException e)
        {
            Log(name + ":caught:" + (int)e.xlReturn);
            return -999.0;
        }
    }
    [ExcelFunction(Name = "ED_TRY_READ")]
    public static object TryRead([ExcelArgument(AllowReference = true)] object r) => Try("try", r);
    [ExcelFunction(Name = "ED_TRY_READ_SAFE", IsThreadSafe = true)]
    public static object TryReadSafe([ExcelArgument(AllowReference = true)] object r) => Try("trysafe", r);
    [ExcelFunction(Name = "ED_THROW_READ")]
    public static object ThrowRead([ExcelArgument(AllowReference = true)] object r) => Throw("throw", r);
    [ExcelFunction(Name = "ED_THROW_READ_SAFE", IsThreadSafe = true)]
    public static object ThrowReadSafe([ExcelArgument(AllowReference = true)] object r) => Throw("throwsafe", r);
}
