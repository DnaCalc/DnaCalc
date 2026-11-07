# Writes the GDI device advances Excel's grid text uses for printable ASCII.
# A browser reports unhinted, fractional advances, and Excel's hinted ones differ by more
# than rounding: Calibri at 15 ppem advances l by 4 where its outline gives 3.44.
# Reads installed fonts only. Run: pwsh -File gdi-advances.ps1 -Out <file>
param([Parameter(Mandatory)][string]$Out)
$ErrorActionPreference='Stop'
$source=@'
using System; using System.Runtime.InteropServices;
public static class GdiAdvances {
 [DllImport("gdi32.dll",CharSet=CharSet.Unicode)] static extern IntPtr CreateFontW(int h,int w,int esc,int ori,int weight,uint it,uint ul,uint so,uint cs,uint op,uint cp,uint q,uint pf,string face);
 [DllImport("gdi32.dll")] static extern IntPtr CreateCompatibleDC(IntPtr h);
 [DllImport("gdi32.dll")] static extern IntPtr SelectObject(IntPtr dc,IntPtr o);
 [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr o);
 [DllImport("gdi32.dll")] static extern bool DeleteDC(IntPtr o);
 [DllImport("gdi32.dll",CharSet=CharSet.Unicode)] static extern bool GetCharWidth32W(IntPtr dc,uint a,uint b,[Out] int[] w);
 public static int[] Widths(string face,int ppem,int weight,bool italic){
  var dc=CreateCompatibleDC(IntPtr.Zero);
  var f=CreateFontW(-ppem,0,0,0,weight,italic?1u:0u,0,0,1,0,0,0,0,face);
  var old=SelectObject(dc,f); var w=new int[95];
  if(!GetCharWidth32W(dc,32,126,w)) throw new Exception("GetCharWidth32 failed for "+face);
  SelectObject(dc,old); DeleteObject(f); DeleteDC(dc); return w;}
}
'@
Add-Type -TypeDefinition $source
$families='Calibri','Arial','Cambria','Consolas','Times New Roman'
$bytes=[System.Collections.Generic.List[byte]]::new()
foreach($family in $families){
 foreach($style in @(@(400,$false),@(700,$false),@(400,$true),@(700,$true))){
  foreach($ppem in 6..48){
   foreach($w in [GdiAdvances]::Widths($family,$ppem,$style[0],$style[1])){ $bytes.Add([byte]$w) }
  }
 }
}
[IO.File]::WriteAllBytes($Out,$bytes.ToArray())
"wrote $($bytes.Count) bytes"
