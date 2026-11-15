# A late-bound COM client against DNA Calc's own object model (dc-km7i, dc-piw0).
# It starts dna-calc-desktop in app mode (the page and its workbook worker), finds the EXCEL7 window
# of that process, obtains the object model with AccessibleObjectFromWindow(OBJID_NATIVEOM) as an
# add-in's COM client does, and drives it through IDispatch only (InvokeMember, no type library):
#   - assigns the read-only Shape.Type, .ID and .Connector and records the HRESULT (Excel answers
#     DISP_E_BADPARAMCOUNT 0x8002000E, which VBA and VBScript show as error 450);
#   - runs the report members of excel-questions.md: ListObjects.Add, Shapes.AddShape,
#     ChartObjects.Add, Chart.SetSourceData, and records what they answer.
# It touches no registry, Trust Center or Excel setting and registers nothing. It stops only the
# desktop process it started. Exit code 0 when every answer is the one the Rust tests expect. Run it on a private desktop (on-private-desktop.ps1) so no window
# appears on the owner's desktop.
param([Parameter(Mandatory)][string]$Exe, [Parameter(Mandatory)][string]$Bundle, [Parameter(Mandatory)][string]$Out)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path $Out | Out-Null
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class DnaComWindows {
 public delegate bool EnumProc(IntPtr hwnd, IntPtr state);
 [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr state);
 [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumProc cb, IntPtr state);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
 [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd, System.Text.StringBuilder t, int n);
 [DllImport("oleacc.dll")] static extern int AccessibleObjectFromWindow(IntPtr hwnd, uint id, ref Guid iid, [MarshalAs(UnmanagedType.IDispatch)] out object obj);
 static string Class(IntPtr h) { var c = new System.Text.StringBuilder(128); GetClassName(h, c, 128); return c.ToString(); }
 public static IntPtr FindExcel7(uint pid) {
  IntPtr found = IntPtr.Zero;
  EnumWindows((top, s) => {
   uint p; GetWindowThreadProcessId(top, out p);
   if (p != pid) return true;
   if (Class(top) == "EXCEL7") { found = top; return false; }
   EnumChildWindows(top, (child, s2) => { if (Class(child) == "EXCEL7") { found = child; return false; } return true; }, IntPtr.Zero);
   return found == IntPtr.Zero;
  }, IntPtr.Zero);
  return found;
 }
 public static object NativeOm(IntPtr hwnd) {
  Guid idispatch = new Guid("00020400-0000-0000-C000-000000000046");
  object o; int hr = AccessibleObjectFromWindow(hwnd, 0xFFFFFFF0u, ref idispatch, out o);
  if (hr != 0) throw new COMException("AccessibleObjectFromWindow", hr);
  return o;
 }
}

public static class DnaRawDispatch {
 [StructLayout(LayoutKind.Sequential)] struct DISPPARAMS { public IntPtr rgvarg; public IntPtr rgdispidNamedArgs; public uint cArgs; public uint cNamedArgs; }
 [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int GetIDs(IntPtr self, ref Guid riid, [MarshalAs(UnmanagedType.LPArray, ArraySubType = UnmanagedType.LPWStr)] string[] names, uint count, uint lcid, int[] ids);
 [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int Inv(IntPtr self, int dispid, ref Guid riid, uint lcid, ushort flags, ref DISPPARAMS p, IntPtr result, IntPtr excep, IntPtr argErr);
 static T Slot<T>(IntPtr self, int slot) where T : class {
  IntPtr vtable = Marshal.ReadIntPtr(self); return (T)(object)Marshal.GetDelegateForFunctionPointer(Marshal.ReadIntPtr(vtable, slot * IntPtr.Size), typeof(T));
 }
 // IDispatch::Invoke straight on the interface pointer: the raw HRESULT, none of the runtime's translation.
 public static int Put(IntPtr self, string name, object value) {
  Guid none = Guid.Empty; var ids = new int[1];
  int hr = Slot<GetIDs>(self, 5)(self, ref none, new[] { name }, 1, 0, ids);
  if (hr != 0) return hr;
  IntPtr arg = Marshal.AllocCoTaskMem(16); Marshal.GetNativeVariantForObject(value, arg);
  IntPtr named = Marshal.AllocCoTaskMem(4); Marshal.WriteInt32(named, -3);
  var p = new DISPPARAMS { rgvarg = arg, rgdispidNamedArgs = named, cArgs = 1, cNamedArgs = 1 };
  try { return Slot<Inv>(self, 6)(self, ids[0], ref none, 0, 4, ref p, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero); }
  finally { Marshal.FreeCoTaskMem(arg); Marshal.FreeCoTaskMem(named); }
 }
}
'@
$bf = [Reflection.BindingFlags]
function Invoke-Member($target, [string]$name, [Reflection.BindingFlags]$flags, [object[]]$arguments = @()) {
    try { return [System.__ComObject].InvokeMember($name, $flags, $null, $target, $arguments) }
    catch [System.Reflection.TargetInvocationException] { throw $_.Exception.InnerException }
}
function Get-Member2($t, $n, [object[]]$a = @()) { Invoke-Member $t $n ($bf::GetProperty -bor $bf::InvokeMethod) $a }
function Set-Member2($t, $n, $v) { Invoke-Member $t $n $bf::SetProperty @($v) | Out-Null }
function Call-Member($t, $n, [object[]]$a = @()) { Invoke-Member $t $n $bf::InvokeMethod $a }
function Attempt([scriptblock]$body) {
    try { return [ordered]@{ ok = $true; value = (& $body) } }
    catch { $e = $_.Exception; return [ordered]@{ ok = $false; hresult = ('0x{0:X8}' -f $e.HResult); message = $e.Message } }
}
$missing = [Type]::Missing
$result = [ordered]@{ started_utc = [DateTime]::UtcNow.ToString('O') }
$env:DNACALC_WEBVIEW2_DATA = Join-Path $Out 'webview2'
$process = Start-Process -FilePath $Exe -ArgumentList @('--app', '--assets', $Bundle, '--size', '1280x1000') -PassThru
try {
    $hwnd = [IntPtr]::Zero
    $deadline = [DateTime]::UtcNow.AddSeconds(90)
    while ($hwnd -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 300
        if ($process.HasExited) { throw "the desktop exe exited with $($process.ExitCode)" }
        $hwnd = [DnaComWindows]::FindExcel7([uint32]$process.Id)
    }
    if ($hwnd -eq [IntPtr]::Zero) { throw 'no EXCEL7 window for the desktop process' }
    $window = $null
    for ($i = 0; $i -lt 100 -and -not $window; $i++) {
        try { $window = [DnaComWindows]::NativeOm($hwnd) } catch { Start-Sleep -Milliseconds 300 }
    }
    if (-not $window) { throw 'AccessibleObjectFromWindow did not return the object model' }
    $app = Get-Member2 $window 'Application'
    $result.application_name = Get-Member2 $app 'Name'
    $sheet = Get-Member2 $app 'ActiveSheet'
    $result.active_sheet = Get-Member2 $sheet 'Name'

    # Report data, as the Rust tests dc_piw0__excel_answers_* enter it.
    $rows = @(@('Region', 'Q1', 'Q2'), @('North', 10.0, 20.0), @('South', 30.0, 40.0))
    for ($r = 0; $r -lt 3; $r++) { for ($c = 0; $c -lt 3; $c++) {
        $cell = Get-Member2 $app 'Range' @("$([char](65 + $c))$($r + 1)")
        Set-Member2 $cell 'Value2' $rows[$r][$c]
    } }
    $result.a2 = Get-Member2 (Get-Member2 $app 'Range' @('A2')) 'Value2'

    # Q4/Q5: ListObjects.Add(1, Range("A1:C3"), , 1).
    $tables = Get-Member2 $sheet 'ListObjects'
    $table = Call-Member $tables 'Add' @(1, (Get-Member2 $app 'Range' @('A1:C3')), $missing, 1)
    $result.table = [ordered]@{
        name = Get-Member2 $table 'Name'; style = Get-Member2 $table 'TableStyle'
        range = Get-Member2 (Get-Member2 $table 'Range') 'Address'; show_totals = Get-Member2 $table 'ShowTotals' }

    # Q1: ChartObjects.Add(200, 20, 300, 200) then SetSourceData, then Shapes.AddShape(1, 20, 120, 100, 50).
    $charts = Get-Member2 $sheet 'ChartObjects'
    $frame = Call-Member $charts 'Add' @(200.0, 20.0, 300.0, 200.0)
    $chart = Get-Member2 $frame 'Chart'
    $before = Get-Member2 (Get-Member2 $chart 'SeriesCollection') 'Count'
    Call-Member $chart 'SetSourceData' @((Get-Member2 $table 'Range')) | Out-Null
    $series = Get-Member2 $chart 'SeriesCollection'
    $names = @(); $count = Get-Member2 $series 'Count'
    for ($i = 1; $i -le $count; $i++) { $names += Get-Member2 (Get-Member2 $series 'Item' @($i)) 'Name' }
    $result.chart = [ordered]@{ frame_name = Get-Member2 $frame 'Name'; series_before = $before; series_count = $count; series_names = $names
        first_formula = Get-Member2 (Get-Member2 $series 'Item' @(1)) 'Formula' }
    $shapes = Get-Member2 $sheet 'Shapes'
    $rectangle = Call-Member $shapes 'AddShape' @(1, 20.0, 120.0, 100.0, 50.0)
    $drawing = @()
    for ($i = 1; $i -le (Get-Member2 $shapes 'Count'); $i++) {
        $shape = Get-Member2 $shapes 'Item' @($i)
        $drawing += [ordered]@{ name = Get-Member2 $shape 'Name'; id = Get-Member2 $shape 'ID'; type = Get-Member2 $shape 'Type' }
    }
    $result.shapes = $drawing
    $result.rectangle_name = Get-Member2 $rectangle 'Name'

    # The 450 case: a property PUT on a getter-only member.
    $pointer = [Runtime.InteropServices.Marshal]::GetIDispatchForObject($rectangle)
    $result.raw_put_type = '0x{0:X8}' -f [DnaRawDispatch]::Put($pointer, 'Type', 1)
    $result.raw_put_id = '0x{0:X8}' -f [DnaRawDispatch]::Put($pointer, 'ID', 5)
    $result.raw_put_connector = '0x{0:X8}' -f [DnaRawDispatch]::Put($pointer, 'Connector', $true)
    $result.raw_put_name = '0x{0:X8}' -f [DnaRawDispatch]::Put($pointer, 'Name', 'Box')
    $result.raw_put_unknown = '0x{0:X8}' -f [DnaRawDispatch]::Put($pointer, 'NoSuchMember', 1)
    [void][Runtime.InteropServices.Marshal]::Release($pointer)
    $result.put_type = Attempt { Set-Member2 $rectangle 'Type' 1 }
    # Controls: a writable property still works, and an unknown member is not 450.
    $result.put_name = Attempt { Set-Member2 $rectangle 'Name' 'Box'; Get-Member2 $rectangle 'Name' }
    $result.unknown_member = Attempt { Get-Member2 $rectangle 'NoSuchMember' }


    # What the Rust tests dc_piw0__excel_answers_* build: Table1 over $A$1:$C$3, Chart 1 (id 2, a chart) with
    # series Q1, Q2, Rectangle 2 (id 3, an autoshape); a PUT on a read-only member is DISP_E_BADPARAMCOUNT.
    $expected = [ordered]@{
        'table name' = @($result.table.name, 'Table1'); 'table range' = @($result.table.range, '$A$1:$C$3')
        'table style' = @($result.table.style, 'TableStyleMedium2')
        'chart name' = @($result.chart.frame_name, 'Chart 1'); 'series count' = @($result.chart.series_count, 2)
        'series names' = @(($result.chart.series_names -join ','), 'Q1,Q2')
        'series formula' = @($result.chart.first_formula, '=SERIES(Sheet1!$B$1,Sheet1!$A$2:$A$3,Sheet1!$B$2:$B$3,1)')
        'shape 1' = @(('{0}/{1}/{2}' -f $result.shapes[0].name, $result.shapes[0].id, $result.shapes[0].type), 'Chart 1/2/3')
        'shape 2' = @(('{0}/{1}/{2}' -f $result.shapes[1].name, $result.shapes[1].id, $result.shapes[1].type), 'Rectangle 2/3/1')
        'put Shape.Type' = @($result.raw_put_type, '0x8002000E'); 'put Shape.ID' = @($result.raw_put_id, '0x8002000E')
        'put Shape.Connector' = @($result.raw_put_connector, '0x8002000E')
        'put Shape.Name' = @($result.raw_put_name, '0x00000000'); 'put unknown member' = @($result.raw_put_unknown, '0x80020006')
    }
    $result.mismatches = @($expected.Keys | Where-Object { "$($expected[$_][0])" -ne "$($expected[$_][1])" } | ForEach-Object { "$_ : got $($expected[$_][0]), expected $($expected[$_][1])" })
}
catch { $e = $_.Exception; if ($e.InnerException) { $e = $e.InnerException }; $result.failure = ('{0} (0x{1:X8})' -f $e.Message, $e.HResult) }
finally {
    if ($process -and -not $process.HasExited) { Stop-Process -Id $process.Id -Force }
    $result.finished_utc = [DateTime]::UtcNow.ToString('O')
    $result | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $Out 'result.json') -Encoding utf8NoBOM
}
$result | ConvertTo-Json -Depth 6
if ($result.failure -or @($result.mismatches).Count -ne 0 -or -not $result.Contains('mismatches')) { exit 1 }
