# Pixel observation only. The numeric oracle worker and Excel question service are unchanged.
param([Parameter(Mandatory=$true)][string]$Request,[switch]$Preflight,[string]$ExcelPath,[string]$WorkerDesktop)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Diagnostics;
using System.IO;
using System.Threading;
using System.Text;
public static class PaintDesktop {
 [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)] struct Startup {
   public int cb; public string reserved,desktop,title; public uint x,y,w,h,xc,yc,fill,flags;
   public short show,reserved2; public IntPtr reservedPtr,input,output,error;
 }
 [StructLayout(LayoutKind.Sequential)] struct ProcessInfo {public IntPtr process,thread;public uint pid,tid;}
 [DllImport("user32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern IntPtr CreateDesktop(string name,IntPtr device,IntPtr mode,uint flags,uint access,IntPtr security);
 [DllImport("user32.dll")] static extern bool CloseDesktop(IntPtr desktop);
 [DllImport("user32.dll")] static extern IntPtr GetThreadDesktop(uint thread);
 [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
 [DllImport("user32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool GetUserObjectInformation(IntPtr handle,int index,StringBuilder name,int length,out int needed);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool CreateProcess(string application,StringBuilder command,IntPtr processSecurity,IntPtr threadSecurity,bool inherit,uint flags,IntPtr environment,string directory,ref Startup startup,out ProcessInfo info);
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool SetHandleInformation(IntPtr handle,uint mask,uint flags);
 [DllImport("kernel32.dll")] static extern uint WaitForSingleObject(IntPtr handle,uint milliseconds);
 [DllImport("kernel32.dll")] static extern bool GetExitCodeProcess(IntPtr process,out uint code);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
 public static string Name() {
   var text=new StringBuilder(256);int needed;
   if(!GetUserObjectInformation(GetThreadDesktop(GetCurrentThreadId()),2,text,512,out needed))throw new System.ComponentModel.Win32Exception();
   return text.ToString();
 }
 public static int Run(string name,string executable,string arguments,string log) {
   // No SwitchDesktop, foreground, focus or input APIs. Every window stays on this owned desktop.
   var desktop=CreateDesktop(name,IntPtr.Zero,IntPtr.Zero,0,0x00c7,IntPtr.Zero);
   if(desktop==IntPtr.Zero)throw new System.ComponentModel.Win32Exception();
   ProcessInfo info=new ProcessInfo();
   try {
     using(var stream=new FileStream(log,FileMode.Create,FileAccess.Write,FileShare.ReadWrite)) {
       var handle=stream.SafeFileHandle.DangerousGetHandle();
       if(!SetHandleInformation(handle,1,1))throw new System.ComponentModel.Win32Exception();
       var startup=new Startup {cb=Marshal.SizeOf(typeof(Startup)),desktop="winsta0\\"+name,flags=0x101,show=0,output=handle,error=handle};
       if(!CreateProcess(executable,new StringBuilder("\""+executable+"\" "+arguments),IntPtr.Zero,IntPtr.Zero,true,0x08000000,IntPtr.Zero,null,ref startup,out info))throw new System.ComponentModel.Win32Exception();
       CloseHandle(info.thread);info.thread=IntPtr.Zero;
       if(WaitForSingleObject(info.process,0xffffffff)!=0)throw new Exception("Isolated paint worker wait failed");
       uint code;if(!GetExitCodeProcess(info.process,out code))throw new System.ComponentModel.Win32Exception();
       return (int)code;
     }
   } finally {if(info.thread!=IntPtr.Zero)CloseHandle(info.thread);if(info.process!=IntPtr.Zero)CloseHandle(info.process);CloseDesktop(desktop);}
 }
}
public static class PaintWindow {
 [StructLayout(LayoutKind.Sequential)] public struct Rect {public int L,T,R,B;}
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint flags);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr after,int x,int y,int w,int height,uint flags);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,out uint id);
 [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
 [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
 public delegate bool EnumProc(IntPtr h,IntPtr p);
 [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr h,EnumProc f,IntPtr p);
 [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f,IntPtr p);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h,System.Text.StringBuilder b,int n);
 [DllImport("oleacc.dll")] public static extern int AccessibleObjectFromWindow(IntPtr h,uint id,ref Guid iid,[MarshalAs(UnmanagedType.Interface)] out object result);
 public static object NativeWindow(int processId) {
   object result=null;
   EnumWindows((root,p)=>{uint id;GetWindowThreadProcessId(root,out id);if(id!=processId)return true;
     EnumChildWindows(root,(h,q)=>{var name=new System.Text.StringBuilder(128);GetClassName(h,name,128);
       if(name.ToString()=="EXCEL7") {var iid=new Guid("00020400-0000-0000-C000-000000000046");object value;
         if(AccessibleObjectFromWindow(h,0xFFFFFFF0,ref iid,out value)>=0 && value!=null){result=value;return false;}}
       return true;},IntPtr.Zero);return result==null;},IntPtr.Zero);
   return result;
 }
 public static Rect SheetRect(IntPtr root) {
   Rect best=new Rect(); long area=0;
   EnumChildWindows(root,(h,p)=>{var name=new System.Text.StringBuilder(128);GetClassName(h,name,128);Rect r;
     if(name.ToString()=="EXCEL7" && GetWindowRect(h,out r)) {long a=(long)(r.R-r.L)*(r.B-r.T);if(a>area){best=r;area=a;}}return true;},IntPtr.Zero);
   if(area==0)throw new Exception("No Excel worksheet child window");return best;
 }
 public static int OwnedExcel;
 public static long OwnedStartTicks;
 public sealed class Lease:IDisposable {
   FileStream stream; Timer timer; string path;
   public Lease(string p,string text,int milliseconds) {
     path=p;stream=new FileStream(p,FileMode.CreateNew,FileAccess.Write,FileShare.Read|FileShare.Delete);
     var bytes=System.Text.Encoding.UTF8.GetBytes(text);stream.Write(bytes,0,bytes.Length);stream.Flush();
     timer=new Timer(_=>{try{if(OwnedExcel!=0){var ownedProcess=Process.GetProcessById(OwnedExcel);if(ownedProcess.StartTime.ToUniversalTime().Ticks==OwnedStartTicks)ownedProcess.Kill();}}catch{}
       stream.Dispose();try{File.Delete(path);}catch{} Environment.Exit(3);},null,milliseconds,Timeout.Infinite);
   }
   public void Dispose(){timer.Dispose();stream.Dispose();File.Delete(path);}
 }
}
'@
[void][PaintWindow]::SetThreadDpiAwarenessContext([IntPtr](-4))
$job = Get-Content -LiteralPath $Request -Raw | ConvertFrom-Json
$out = [string]$job.out -replace '^\\\\\?\\',''
foreach ($inputBook in $job.books) { $inputBook.path = [string]$inputBook.path -replace '^\\\\\?\\','' }
$bootstrap = [string]$job.bootstrap.path -replace '^\\\\\?\\',''
if (-not $Preflight -and -not $WorkerDesktop) {
    $paintDesktop = 'DNA-Calc-Paint-'+$PID+'-'+[Guid]::NewGuid().ToString('N')
    $paintLog = Join-Path $out 'excel-worker.log'
    $paintShell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $paintArguments = '-NoProfile -STA -NonInteractive -File "'+$PSCommandPath+'" -Request "'+$Request+'" -WorkerDesktop "'+$paintDesktop+'"'
    if ($ExcelPath) { $paintArguments += ' -ExcelPath "'+$ExcelPath+'"' }
    $paintCode = [PaintDesktop]::Run($paintDesktop,$paintShell,$paintArguments,$paintLog)
    Get-Content -LiteralPath $paintLog
    if ($paintCode -ne 0) { throw "Isolated paint worker exited $paintCode" }
    exit 0
}
if (-not $Preflight -and ([PaintDesktop]::Name() -ne $WorkerDesktop -or -not $WorkerDesktop.StartsWith('DNA-Calc-Paint-'))) {
    throw 'Excel paint capture is permitted only on its own isolated desktop'
}
$lockPath = Join-Path $out 'excel-worker.lease'
$script:lock = $null
function Enter-Desktop {
    if ((Test-Path -LiteralPath $lockPath) -and ((Get-Date)-(Get-Item -LiteralPath $lockPath).LastWriteTime).TotalMinutes -gt 5) {
        Remove-Item -LiteralPath $lockPath
    }
    $milliseconds = 115000
    $script:lock = [PaintWindow+Lease]::new($lockPath,"DNA Calc paint capture worker $PID $(Get-Date -Format o)",$milliseconds)
    $script:lockStart = Get-Date
}
function Exit-Desktop {
    if ($null -ne $script:lock) {
        $elapsed = ((Get-Date)-$script:lockStart).TotalSeconds
        $script:lock.Dispose(); $script:lock=$null
        if ($elapsed -ge 120) { throw "Desktop lock exceeded two minutes: $elapsed" }
    }
}
function Write-Json($value,[string]$path) {
    [IO.File]::WriteAllText($path, (ConvertTo-Json -InputObject $value -Depth 30), [Text.UTF8Encoding]::new($false))
}
function File-Hash([string]$path) {
    $stream=[IO.File]::OpenRead($path); $hash=[Security.Cryptography.SHA256]::Create()
    try { return [BitConverter]::ToString($hash.ComputeHash($stream)).Replace('-','').ToLowerInvariant() }
    finally { $stream.Dispose(); $hash.Dispose() }
}
# PointsToScreenPixels accepts integral points. Confirm edges with Excel's screen-pixel hit test.
# https://learn.microsoft.com/en-us/office/vba/api/excel.window.rangefrompoint
function Refine-Edge($window, [bool]$rows, [int]$index, [int]$estimated, [int]$cross, [int]$radius) {
    for ($coordinate=$estimated-$radius;$coordinate -le $estimated+$radius;$coordinate++) {
        $hit=$null
        try {
            if ($rows) { $hit=$window.RangeFromPoint($cross,$coordinate) }
            else { $hit=$window.RangeFromPoint($coordinate,$cross) }
            if ($null -ne $hit) {
                $band=if($rows){$hit.Row}else{$hit.Column}
                if ($band -eq $index) { return $coordinate }
            }
        } catch { return $null }
        finally { if ($null -ne $hit -and [Runtime.InteropServices.Marshal]::IsComObject($hit)) { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($hit) } }
    }
    return $null
}
# Check path, codec and helper failures before launching Excel.
[void](Join-Path $out 'excel-session.json')
foreach ($inputBook in $job.books) {
    if ((File-Hash $inputBook.path) -ne $inputBook.sha256) { throw 'Workbook changed before opening' }
}
if (-not [IO.File]::Exists($bootstrap) -or (File-Hash $bootstrap) -ne $job.bootstrap.sha256) { throw 'Invalid authored startup workbook' }
if (-not $ExcelPath) {
    $candidates=@(
        [IO.Path]::Combine([Environment]::GetFolderPath('ProgramFiles'),'Microsoft Office\root\Office16\EXCEL.EXE'),
        [IO.Path]::Combine([Environment]::GetFolderPath('ProgramFilesX86'),'Microsoft Office\root\Office16\EXCEL.EXE'),
        [IO.Path]::Combine([Environment]::GetFolderPath('ProgramFiles'),'Microsoft Office\Office16\EXCEL.EXE'))
    $ExcelPath=$candidates | Where-Object {[IO.File]::Exists($_)} | Select-Object -First 1
}
if (-not $ExcelPath -or -not [IO.File]::Exists($ExcelPath)) { throw 'Excel executable not found; supply -ExcelPath' }
if ($Preflight) { Write-Host 'PAINT capture preflight passed; no Excel launched'; exit 0 }
$before = @(Get-Process EXCEL -ErrorAction SilentlyContinue | ForEach-Object Id)
$owned = $false; $excel = $null; $book = $null; $excelProcess = 0
$records = [Collections.Generic.List[object]]::new()
try {
    Enter-Desktop
    try {
        # /x creates a distinct process. Track it before waiting for Excel or its COM object;
        # a stalled startup must still leave a PID the lease watchdog can terminate.
        $start=[Diagnostics.ProcessStartInfo]::new()
        $start.FileName=$ExcelPath; $start.Arguments='/x /automation /r "'+$bootstrap+'"'
        $start.UseShellExecute=$false; $start.CreateNoWindow=$true; $start.WindowStyle=[Diagnostics.ProcessWindowStyle]::Hidden
        $launched=[Diagnostics.Process]::Start($start)
        $excelProcess=$launched.Id
        if ($excelProcess -eq 0 -or $excelProcess -eq 3144 -or $before -contains $excelProcess) { throw "Launch did not create an owned Excel: $excelProcess" }
        $excelStarted=$launched.StartTime.ToUniversalTime().Ticks
        [PaintWindow]::OwnedStartTicks = $excelStarted
        [PaintWindow]::OwnedExcel = $excelProcess
        $owned = $true
        Write-Host "PAINT owned Excel PID $excelProcess (launch 1)"
        $commandLine=$start.FileName+' '+$start.Arguments
        $session = @{pid=$excelProcess;launches=1;started=(Get-Date -Format o);process_start_ticks=$excelStarted;command_line=$commandLine;preexisting=$before;closed=$false;desktop=$WorkerDesktop;isolation='CreateDesktop/CreateProcess; no desktop switch or foreground/input APIs'}
        Write-Json $session (Join-Path $out 'excel-session.json')
        $startup=Get-Date
        $nativeWindow=$null
        while ($null -eq $nativeWindow) {
            if ($launched.HasExited) { throw 'Owned Excel exited during startup' }
            if (((Get-Date)-$startup).TotalSeconds -gt 45) { throw 'Owned Excel did not expose its workbook window' }
            $nativeWindow=[PaintWindow]::NativeWindow($excelProcess)
            if ($null -eq $nativeWindow) { Start-Sleep -Milliseconds 100 }
        }
        $excel=$nativeWindow.Application
        $hwnd=[IntPtr]$excel.Hwnd
        $windowProcess=[uint32]0
        [void][PaintWindow]::GetWindowThreadProcessId($hwnd,[ref]$windowProcess)
        if ($windowProcess -ne $excelProcess -or (Get-Process -Id $excelProcess).StartTime.ToUniversalTime().Ticks -ne $excelStarted) { throw 'Native object model belongs to another process' }
        $fontCollection = [Drawing.Text.InstalledFontCollection]::new()
        $session.installed_font_families = @($fontCollection.Families | ForEach-Object Name | Sort-Object)
        $fontCollection.Dispose()
        Write-Json $session (Join-Path $out 'excel-session.json')
        $excel.DisplayAlerts = $false
        $excel.EnableEvents = $false
        $excel.ScreenUpdating = $true
        $excel.Visible = $true
        $excel.WindowState = -4143 # xlNormal, owned instance only
        [void][PaintWindow]::SetWindowPos($hwnd,[IntPtr]::Zero,40,40,1760,1200,0x0054) # NOACTIVATE | NOZORDER | SHOWWINDOW
        $startupBook=@($excel.Workbooks | Where-Object {[string]$_.FullName -eq $bootstrap})
        if ($startupBook.Count -ne 1) { throw 'Authored startup workbook is not open' }
        $startupBook[0].Close($false)
    } finally { Exit-Desktop }
    $dpi = [PaintWindow]::GetDpiForWindow($hwnd)
    foreach ($inputBook in $job.books) {
        if ((File-Hash $inputBook.path) -ne $inputBook.sha256) { throw 'Workbook changed before opening' }
        Enter-Desktop
        try {
            $book = $excel.Workbooks.Open(([string]$inputBook.path -replace '^\\\\\?\\',''),0,$true)
            # Excel's SDI window can change when the last workbook closes.
            $hwnd = [IntPtr]$excel.Hwnd
            $windowProcess = [uint32]0
            [void][PaintWindow]::GetWindowThreadProcessId($hwnd,[ref]$windowProcess)
            if ($windowProcess -ne $excelProcess) { throw 'Workbook window belongs to another Excel process' }
            [void][PaintWindow]::SetWindowPos($hwnd,[IntPtr]::Zero,40,40,1760,1200,0x0054)
            $dpi = [PaintWindow]::GetDpiForWindow($hwnd)
        }
        finally { Exit-Desktop }
        foreach ($sheet in $book.Worksheets) {
            if ($sheet.Visible -ne -1) { continue }
            foreach ($zoom in $job.zooms) {
                Enter-Desktop
                try {
                    $sheet.Activate()
                    $win = $excel.ActiveWindow
                    $win.Zoom = [int]$zoom
                    $win.ScrollRow = 1; $win.ScrollColumn = 1
                    # Keep the authored selection and freeze panes: they are part of the image.
                    Start-Sleep -Milliseconds 300
                    $rect = [PaintWindow+Rect]::new()
                    [void][PaintWindow]::GetWindowRect($hwnd,[ref]$rect)
                    $originX = $win.PointsToScreenPixelsX(0) - $rect.L
                    $originY = $win.PointsToScreenPixelsY(0) - $rect.T
                    $sheetRect = [PaintWindow]::SheetRect($hwnd)
                    $xEdges=@{}; $yEdges=@{}
                    $radius=[int][Math]::Ceiling($dpi/72.*[int]$zoom/100.)+2
                    $crossX=$win.PointsToScreenPixelsX(0)+2
                    $crossY=$win.PointsToScreenPixelsY(0)+2
                    for ($c=1;$c -le $job.columns+1;$c++) {
                        $estimated=$win.PointsToScreenPixelsX($sheet.Cells.Item(1,$c).Left)
                        $xEdges[$c]=Refine-Edge $win $false $c $estimated $crossY $radius
                    }
                    for ($r=1;$r -le $job.rows+1;$r++) {
                        $estimated=$win.PointsToScreenPixelsY($sheet.Cells.Item($r,1).Top)
                        $yEdges[$r]=Refine-Edge $win $true $r $estimated $crossX $radius
                    }
                    $calibrated=(@($xEdges.Values | Where-Object {$null -eq $_}).Count + @($yEdges.Values | Where-Object {$null -eq $_}).Count) -eq 0
                    if ($calibrated) { $originX=$xEdges[1]-$rect.L; $originY=$yEdges[1]-$rect.T }
                    $cells = [Collections.Generic.List[object]]::new()
                    for ($r=1;$r -le $job.rows;$r++) {
                        for ($c=1;$c -le $job.columns;$c++) {
                            $cell = $sheet.Cells.Item($r,$c)
                            $x = $win.PointsToScreenPixelsX($cell.Left) - $rect.L
                            $y = $win.PointsToScreenPixelsY($cell.Top) - $rect.T
                            $right = $win.PointsToScreenPixelsX($cell.Left+$cell.Width) - $rect.L
                            $bottom = $win.PointsToScreenPixelsY($cell.Top+$cell.Height) - $rect.T
                            if ($calibrated) {
                                $x=$xEdges[$c]-$rect.L; $y=$yEdges[$r]-$rect.T
                                $right=$xEdges[$c+1]-$rect.L; $bottom=$yEdges[$r+1]-$rect.T
                            }
                            $display=$cell.DisplayFormat
                            $cells.Add(@{row=$r;col=$c;x=$x;y=$y;w=$right-$x;h=$bottom-$y;points=@{x=[double]$cell.Left;y=[double]$cell.Top;w=[double]$cell.Width;h=[double]$cell.Height};text=[string]$cell.Text;number_format=[string]$cell.NumberFormat;number_format_local=[string]$cell.NumberFormatLocal;display_fill_color=[long]$display.Interior.Color;display_fill_pattern=[int]$display.Interior.Pattern;display_font_color=[long]$display.Font.Color})
                            [void][Runtime.InteropServices.Marshal]::ReleaseComObject($display)
                            [void][Runtime.InteropServices.Marshal]::ReleaseComObject($cell)
                        }
                    }
                    $prefix = [IO.Path]::GetFileNameWithoutExtension([string]$inputBook.path)+'-'+$sheet.Name+'-excel-'+$zoom
                    $bitmap = [Drawing.Bitmap]::new($rect.R-$rect.L,$rect.B-$rect.T)
                    $graphics = [Drawing.Graphics]::FromImage($bitmap)
                    $dc=$graphics.GetHdc()
                    try { $printed=[PaintWindow]::PrintWindow($hwnd,$dc,2) }
                    finally { $graphics.ReleaseHdc($dc);$graphics.Dispose() }
                    try {
                        if (-not $printed) { throw 'PrintWindow failed; no screen-copy fallback' }
                        $bitmap.Save((Join-Path $out "$prefix.png"),[Drawing.Imaging.ImageFormat]::Png)
                    } finally { $bitmap.Dispose() }
                    $records.Add(@{sha256=$inputBook.sha256;image_sha256=(File-Hash (Join-Path $out "$prefix.png"));sheet=$sheet.Name;zoom=[int]$zoom;dpi=[int]$dpi;origin=@{x=$originX;y=$originY};sheet_bounds=@{x=$sheetRect.L-$rect.L;y=$sheetRect.T-$rect.T;w=$sheetRect.R-$sheetRect.L;h=$sheetRect.B-$sheetRect.T};cells=@($cells.ToArray());prefix=$prefix;capture='PrintWindow(PW_RENDERFULLCONTENT)';excel_version=$excel.Version;excel_build=$excel.Build;normal_font=$book.Styles.Item('Normal').Font.Name;normal_size=$book.Styles.Item('Normal').Font.Size;default_row_height=$sheet.StandardHeight;default_column_width=$sheet.StandardWidth;gridlines=$win.DisplayGridlines;freeze=$win.FreezePanes})
                    $records[$records.Count-1].coordinate_calibrated=$calibrated
                    $records[$records.Count-1].desktop=$WorkerDesktop
                    $records[$records.Count-1].format_environment=@{decimal_separator=[string]$excel.DecimalSeparator;thousands_separator=[string]$excel.ThousandsSeparator;use_system_separators=[bool]$excel.UseSystemSeparators}
                    $records[$records.Count-1].coordinate_method=if($calibrated){'RangeFromPoint pixel edges'}else{'integral-point API; requires raster calibration'}
                    Write-Json @($records.ToArray()) (Join-Path $out 'excel.json')
                    Write-Host "PAINT Excel $prefix PID $excelProcess (launch 1)"
                } finally { Exit-Desktop }
            }
        }
        Enter-Desktop
        try { $book.Close($false); [void][Runtime.InteropServices.Marshal]::ReleaseComObject($book);$book=$null }
        finally { Exit-Desktop }
        if ((File-Hash $inputBook.path) -ne $inputBook.sha256) { throw 'Workbook changed during capture' }
    }
} finally {
    Exit-Desktop
    if ($owned) {
        try {
            Enter-Desktop
            try { if ($null -ne $excel) { if ($null -ne $book) { $book.Close($false) }; $excel.Quit() } }
            finally { Exit-Desktop }
        } catch {
            # No UI fallback if another agent holds the desktop or the window has ended.
            $process = Get-Process -Id $excelProcess -ErrorAction SilentlyContinue
            if ($process -and $process.StartTime.ToUniversalTime().Ticks -eq $excelStarted) { Stop-Process -Id $excelProcess }
            $session.cleanup_note = $_.Exception.Message
        }
        if ($null -ne $excel) { [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($excel) }
        $excel=$null
        # An RCW finalizer can wait for a COM apartment that is already shutting down.
        # Explicitly released objects and the owned PID/start-time check below bound cleanup.
        [GC]::Collect()
        $process = Get-Process -Id $excelProcess -ErrorAction SilentlyContinue
        if ($process -and $process.StartTime.ToUniversalTime().Ticks -eq $excelStarted) {
            if (-not $process.WaitForExit(5000)) { Stop-Process -Id $excelProcess; [void]$process.WaitForExit(5000) }
        }
        $session.closed=$launched.HasExited; $session.finished=(Get-Date -Format o)
        Write-Json $session (Join-Path $out 'excel-session.json')
        if (-not $session.closed) { throw "Owned Excel PID $excelProcess did not exit" }
    }
}
