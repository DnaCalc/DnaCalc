# Runs a command on a private Win32 desktop so every window Excel creates stays off the owner's
# desktop. Child processes (session host, Excel) inherit that desktop. A guard on the caller's real
# desktop samples for any window of an Excel process launched under -SessionRoot and records it.
# No registry, Trust Center, macro or Excel setting is touched. Input focus cannot reach that desktop.
param([Parameter(Mandatory)][string]$Command,[Parameter(Mandatory)][string]$Log,[Parameter(Mandatory)][string]$GuardOut,[string]$SessionRoot='C:\Work\DnaCalc\codex-runs\xll20',[string]$DesktopName='DnaCalcHidden20')
$ErrorActionPreference='Stop'
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Threading;
public static class DnaPrivateDesktop {
 [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)] public struct STARTUPINFO {public int cb;public string lpReserved;public string lpDesktop;public string lpTitle;public int dwX,dwY,dwXSize,dwYSize,dwXCountChars,dwYCountChars,dwFillAttribute,dwFlags;public short wShowWindow,cbReserved2;public IntPtr lpReserved2,hStdInput,hStdOutput,hStdError;}
 [StructLayout(LayoutKind.Sequential)] public struct PROCESS_INFORMATION {public IntPtr hProcess,hThread;public int dwProcessId,dwThreadId;}
 [DllImport("user32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern IntPtr CreateDesktop(string name,IntPtr device,IntPtr mode,int flags,uint access,IntPtr sa);
 [DllImport("user32.dll")] static extern bool CloseDesktop(IntPtr h);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool CreateProcess(string app,System.Text.StringBuilder cmd,IntPtr pa,IntPtr ta,bool inherit,uint flags,IntPtr env,string dir,ref STARTUPINFO si,out PROCESS_INFORMATION pi);
 [DllImport("kernel32.dll")] static extern uint WaitForSingleObject(IntPtr h,uint ms);
 [DllImport("kernel32.dll")] static extern bool GetExitCodeProcess(IntPtr h,out uint code);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
 public delegate bool EnumProc(IntPtr hwnd,IntPtr state);
 [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb,IntPtr state);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
 [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd,System.Text.StringBuilder t,int n);
 [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
 static readonly object Sync=new object();
 static HashSet<uint> owned=new HashSet<uint>();
 public static long Samples,OwnedWindows,VisibleWindows,ForegroundHits;
 public static List<string> Findings=new List<string>();
 static Timer timer;
 public static void SetOwned(uint[] ids){lock(Sync){owned=new HashSet<uint>(ids);}}
 static void Tick(object o){lock(Sync){Samples++;if(owned.Count==0)return;
  EnumWindows((h,s)=>{uint p;GetWindowThreadProcessId(h,out p);if(owned.Contains(p)){OwnedWindows++;bool v=IsWindowVisible(h);if(v)VisibleWindows++;var c=new System.Text.StringBuilder(128);GetClassName(h,c,128);if(Findings.Count<50)Findings.Add(DateTime.UtcNow.ToString("O")+" pid="+p+" class="+c+" visible="+v);}return true;},IntPtr.Zero);
  uint fp;var f=GetForegroundWindow();if(f!=IntPtr.Zero){GetWindowThreadProcessId(f,out fp);if(owned.Contains(fp)){ForegroundHits++;if(Findings.Count<50)Findings.Add(DateTime.UtcNow.ToString("O")+" foreground pid="+fp);}}
 }}
 public static void StartGuard(){timer=new Timer(Tick,null,0,10);}
 public static void StopGuard(){timer.Dispose();Tick(null);}
 static IntPtr desktopHandle;
 static PROCESS_INFORMATION child;
 public static void Launch(string desktop,string cmd,string dir){
  desktopHandle=CreateDesktop(desktop,IntPtr.Zero,IntPtr.Zero,0,0x10000000,IntPtr.Zero);if(desktopHandle==IntPtr.Zero)throw new Exception("CreateDesktop "+Marshal.GetLastWin32Error());
  var si=new STARTUPINFO();si.cb=Marshal.SizeOf(si);si.lpDesktop="WinSta0\\"+desktop;
  if(!CreateProcess(null,new System.Text.StringBuilder(cmd),IntPtr.Zero,IntPtr.Zero,false,0x08000000,IntPtr.Zero,dir,ref si,out child))throw new Exception("CreateProcess "+Marshal.GetLastWin32Error());
 }
 public static bool Exited(uint ms){return WaitForSingleObject(child.hProcess,ms)==0;}
 public static int Finish(){uint code;GetExitCodeProcess(child.hProcess,out code);CloseHandle(child.hProcess);CloseHandle(child.hThread);CloseDesktop(desktopHandle);return (int)code;}
}
'@
$root=$SessionRoot.ToLowerInvariant()
$everOwned=[Collections.Generic.HashSet[uint32]]::new()
$inner='pwsh -NoProfile -NonInteractive -Command "'+$Command.Replace('"','\"')+' *> '''+$Log+'''; exit $LASTEXITCODE"'
[DnaPrivateDesktop]::StartGuard()
[DnaPrivateDesktop]::Launch($DesktopName,$inner,(Get-Location).Path)
while(-not [DnaPrivateDesktop]::Exited(500)){
    $ids=@(Get-CimInstance Win32_Process -Filter "Name='EXCEL.EXE'"|Where-Object {$_.CommandLine -and $_.CommandLine.ToLowerInvariant().Contains($root)}|ForEach-Object {[uint32]$_.ProcessId})
    [DnaPrivateDesktop]::SetOwned([uint32[]]$ids)
    foreach($i in $ids){[void]$everOwned.Add($i)}
}
[DnaPrivateDesktop]::StopGuard()
$code=[DnaPrivateDesktop]::Finish()
$guard=[ordered]@{desktop=$DesktopName;session_root=$SessionRoot;exit=$code;guard_samples=[DnaPrivateDesktop]::Samples;owned_excel_pids=@($everOwned|Sort-Object);windows_on_user_desktop=[DnaPrivateDesktop]::OwnedWindows;visible_on_user_desktop=[DnaPrivateDesktop]::VisibleWindows;foreground_hits=[DnaPrivateDesktop]::ForegroundHits;findings=@([DnaPrivateDesktop]::Findings);finished_utc=[DateTime]::UtcNow.ToString('O')}
$guard|ConvertTo-Json -Depth 5|Set-Content -LiteralPath $GuardOut -Encoding utf8NoBOM
$guard|ConvertTo-Json -Depth 5
exit $code
