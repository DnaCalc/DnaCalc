// SPDX-License-Identifier: MIT
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using ExcelDna.Integration;
using Excel = Microsoft.Office.Interop.Excel;

public class IdentityProbe : IExcelAddIn
{
    private static string ComVersion;
    private static int ComHwnd;
    private static object RunResult;
    private static uint CommandThread;
    [DllImport("kernel32.dll")] private static extern uint GetCurrentThreadId();

    public void AutoOpen()
    {
        // The original PIA supplies the IID and vtable calls, independently of the host.
        var app = (Excel._Application)ExcelDnaUtil.Application;
        ComVersion = app.Version;
        ComHwnd = app.Hwnd;
        RunResult = app.Run("X2.Ping");
    }
    public void AutoClose() { }

    [ExcelCommand(Name="X2.Ping")]
    public static void Ping() { CommandThread = GetCurrentThreadId(); }

    [ExcelFunction(Name="X2.Identity", IsMacroType=true)]
    public static string Identity()
    {
        using (var process = Process.GetCurrentProcess())
            return String.Join("|", process.ProcessName,
                FileVersionInfo.GetVersionInfo(process.MainModule.FileName).FileMajorPart,
                Environment.GetEnvironmentVariable("EXCELDNA_ASSUME_EXCEL_PROCESS") ?? "",
                AppDomain.CurrentDomain.IsDefaultAppDomain(), ComVersion ?? "",
                ComHwnd, RunResult ?? "null", CommandThread, GetCurrentThreadId());
    }
}
