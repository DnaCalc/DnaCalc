# Read-only: lists visible top-level windows on a named desktop. Sends nothing to them.
param([Parameter(Mandatory)][string]$Desktop)
Add-Type @'
using System;using System.Collections.Generic;using System.Runtime.InteropServices;using System.Text;
public static class DnaDesk {
 public delegate bool P(IntPtr h,IntPtr s);
 [DllImport("user32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern IntPtr OpenDesktop(string n,int f,bool i,uint a);
 [DllImport("user32.dll")] static extern bool CloseDesktop(IntPtr d);
 [DllImport("user32.dll")] static extern bool EnumDesktopWindows(IntPtr d,P cb,IntPtr s);
 [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr d,P cb,IntPtr s);
 [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h,StringBuilder s,int n);
 public static List<string> Run(string n){var o=new List<string>();var d=OpenDesktop(n,0,false,0x0041);if(d==IntPtr.Zero){o.Add("OpenDesktop failed "+Marshal.GetLastWin32Error());return o;}
  EnumDesktopWindows(d,(h,s)=>{if(IsWindowVisible(h)){uint p;GetWindowThreadProcessId(h,out p);var c=new StringBuilder(128);var t=new StringBuilder(300);GetClassName(h,c,128);GetWindowText(h,t,300);o.Add(p+" "+c+" | "+t);
    if(c.ToString()=="#32770"||c.ToString().StartsWith("NUIDialog")){EnumChildWindows(h,(k,x)=>{var a=new StringBuilder(64);var b=new StringBuilder(400);GetClassName(k,a,64);GetWindowText(k,b,400);if(b.Length>0)o.Add("    "+a+" | "+b);return true;},IntPtr.Zero);}}
   return true;},IntPtr.Zero);CloseDesktop(d);return o;}
}
'@
[DnaDesk]::Run($Desktop)
