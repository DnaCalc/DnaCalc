# Inspect only the supplied owned Excel process. A native visibility observation
# is latched, hidden promptly and reported as a failed job, never a passing sample.
$script:hiddenSourceSha=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant()
if ('DnaHiddenExcel' -as [type]) {
 if([DnaHiddenExcel]::SourceSha -cne $script:hiddenSourceSha){throw 'Hidden-window helper changed; restart this owned session host before another job'}
} else { Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class DnaHiddenExcel {
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
 [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
 [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr hwnd,IntPtr after,int x,int y,int width,int height,uint flags);
 public delegate bool EnumProc(IntPtr hwnd,IntPtr state);
 [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback,IntPtr state);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd,System.Text.StringBuilder text,int count);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr hwnd,System.Text.StringBuilder text,int count);
 [StructLayout(LayoutKind.Sequential)] struct Rect {public int left,top,right,bottom;}
 [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hwnd,out Rect rect);
 [DllImport("user32.dll")] static extern IntPtr GetThreadDesktop(uint id);
 [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern bool GetUserObjectInformation(IntPtr h,int index,System.Text.StringBuilder info,int length,out int needed);
 public static string DesktopName(){var t=new System.Text.StringBuilder(256);int n;GetUserObjectInformation(GetThreadDesktop(GetCurrentThreadId()),2,t,512,out n);return t.ToString();}
 static System.Threading.Timer timer;
 static readonly object Sync=new object();
 static int owner; static long started,samples; static readonly List<string> violations=new List<string>();
 public static string SourceSha;
 public static void Start(IntPtr hwnd) {
  if(timer!=null){timer.Dispose();timer=null;}
  uint pid;GetWindowThreadProcessId(hwnd,out pid);
  lock(Sync){owner=(int)pid;started=System.Diagnostics.Process.GetProcessById(owner).StartTime.ToUniversalTime().Ticks;samples=0;violations.Clear();}
  timer=new System.Threading.Timer(Check,null,0,10);
 }
 static void Check(object ignored) {
  lock(Sync){samples++;
   try{using(var p=System.Diagnostics.Process.GetProcessById(owner)){if(!p.ProcessName.Equals("EXCEL",StringComparison.OrdinalIgnoreCase)||p.StartTime.ToUniversalTime().Ticks!=started){violations.Add("Owned Excel identity changed; no window touched");return;}}}catch{violations.Add("Owned Excel ended; no window touched");return;}
   EnumWindows((hwnd,state)=>{uint pid;GetWindowThreadProcessId(hwnd,out pid);
    if(pid==owner && IsWindowVisible(hwnd)){
     var cls=new System.Text.StringBuilder(256);var title=new System.Text.StringBuilder(512);Rect rect;
     GetClassName(hwnd,cls,cls.Capacity);GetWindowText(hwnd,title,title.Capacity);GetWindowRect(hwnd,out rect);
     violations.Add(DateTime.UtcNow.ToString("O")+" hwnd="+hwnd.ToString()+" class="+cls+" title="+title+" rect="+rect.left+","+rect.top+","+rect.right+","+rect.bottom);SetWindowPos(hwnd,IntPtr.Zero,0,0,0,0,0x0097);
    }return true;},IntPtr.Zero);
  }
 }
 public static string[] Violations(){Check(null);lock(Sync){return violations.ToArray();}}
 public static long Samples(){lock(Sync){return samples;}}
 public static int OwnerId(){lock(Sync){return owner;}}
 public static long StartTicks(){lock(Sync){return started;}}
 public static void Stop(){if(timer!=null){timer.Dispose();timer=null;}Check(null);}
}
'@
 [DnaHiddenExcel]::SourceSha=$script:hiddenSourceSha
}
function Start-HiddenExcel($Application) {
 $script:hiddenExcel=$Application
 $script:desktopName=[DnaHiddenExcel]::DesktopName()
 $script:privateDesktop=($script:desktopName -cne 'Default' -and $script:desktopName -cne 'Winlogon')
 $script:hiddenEntry=[bool]$Application.Visible
 $Application.Visible=$false
 $script:hiddenChecks=[Collections.Generic.List[object]]::new()
 [DnaHiddenExcel]::Start([IntPtr]$Application.Hwnd)
 try{Assert-HiddenExcel 'job-start'}catch{[DnaHiddenExcel]::Stop();throw}
}
function Assert-HiddenExcel([string]$Stage) {
 $visible=[bool]$script:hiddenExcel.Visible;$violations=@([DnaHiddenExcel]::Violations())
 $script:hiddenChecks.Add(@{stage=$Stage;utc=[DateTime]::UtcNow.ToString('O');visible=$visible;native_samples=[DnaHiddenExcel]::Samples();violations=$violations})
 if(($visible -or $violations.Count) -and -not $script:privateDesktop){$script:hiddenExcel.Visible=$false;throw "Excel visibility changed at $Stage; job stopped"}
}
function Stop-HiddenExcel {
 [DnaHiddenExcel]::Stop()
 $visible=[bool]$script:hiddenExcel.Visible;$violations=@([DnaHiddenExcel]::Violations())
 if($visible){$script:hiddenExcel.Visible=$false}
 @{helper_sha256=$script:hiddenSourceSha;excel_pid=[DnaHiddenExcel]::OwnerId();excel_start_ticks=[DnaHiddenExcel]::StartTicks();entry_visible=$script:hiddenEntry;exit_visible=$visible;checks=@($script:hiddenChecks.ToArray());native_samples=[DnaHiddenExcel]::Samples();violations=$(if($script:privateDesktop){@()}else{$violations});private_desktop=$script:privateDesktop;desktop=$script:desktopName;private_desktop_window_observations=$(if($script:privateDesktop){$violations}else{@()});compliant=($script:privateDesktop -or (-not $script:hiddenEntry -and -not $visible -and $violations.Count -eq 0));sampling_limit='Native top-level window sampling requests a 10 ms interval; scheduling can delay samples. Actual counts and operation checks are retained; between-sample transients cannot be excluded.'}
}
