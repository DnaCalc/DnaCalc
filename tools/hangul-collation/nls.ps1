Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices;
public static class NLS {
 [DllImport("kernel32.dll", CharSet=CharSet.Unicode)] public static extern int LCMapStringEx(string loc, uint flags, string src, int cch, byte[] dst, int cchDst, IntPtr v, IntPtr r, IntPtr p);
 [DllImport("kernel32.dll", CharSet=CharSet.Unicode)] public static extern int CompareStringEx(string loc, uint flags, string a, int ca, string b, int cb, IntPtr v, IntPtr r, IntPtr p);
 public static string Key(string s, uint f){ byte[] d=new byte[1024]; int n=LCMapStringEx("en-ZA", 0x400|f, s, s.Length, d, d.Length, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero); if(n<=0) return "ERR"; return BitConverter.ToString(d,0,n).Replace("-",""); }
 public static int Cmp(string a,string b,uint f){ return CompareStringEx("en-ZA", f, a, a.Length, b, b.Length, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero)-2; }
}
"@
