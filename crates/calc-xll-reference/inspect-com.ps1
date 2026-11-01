param([string]$ExcelPath='C:\Program Files\Microsoft Office\root\Office16\EXCEL.EXE')
$ErrorActionPreference='Stop'
# REGKIND_NONE reads metadata. It never registers a type library or creates Excel.
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;
public static class ExcelInterfaceFacts {
 [DllImport("oleaut32.dll",CharSet=CharSet.Unicode)] static extern int LoadTypeLibEx(string path,int kind,out ITypeLib lib);
 public static object[] Read(string path) {
  ITypeLib lib; Marshal.ThrowExceptionForHR(LoadTypeLibEx(path,2,out lib));
  var layouts=new List<object>();
  try { for(int i=0;i<lib.GetTypeInfoCount();++i) {
   string name,doc,help;int context;lib.GetDocumentation(i,out name,out doc,out context,out help);
   if(name!="_Application" && name!="Window") continue;
   ITypeInfo type;lib.GetTypeInfo(i,out type);
   bool dual=false;
   try { int reference;type.GetRefTypeOfImplType(-1,out reference);ITypeInfo actual;type.GetRefTypeInfo(reference,out actual);Marshal.ReleaseComObject(type);type=actual;dual=true; }
   catch(COMException e) { if(e.HResult!=unchecked((int)0x8002802b)) throw; }
   IntPtr attr;type.GetTypeAttr(out attr);
   try {
    var a=Marshal.PtrToStructure<TYPEATTR>(attr);var members=new List<object>();
    for(int f=0;f<a.cFuncs;++f) {
     IntPtr fp;type.GetFuncDesc(f,out fp);
     try {
      var d=Marshal.PtrToStructure<FUNCDESC>(fp);string[] names=new string[64];int got;type.GetNames(d.memid,names,64,out got);
      if(names[0]!="Application" && names[0]!="Parent" && names[0]!="Version" && names[0]!="Hwnd" && names[0]!="Run") continue;
      var parameters=new List<object>();
      for(int p=0;p<d.cParams;++p) {
       var element=Marshal.PtrToStructure<ELEMDESC>(IntPtr.Add(d.lprgelemdescParam,p*Marshal.SizeOf<ELEMDESC>()));
       parameters.Add(new {variant_type=element.tdesc.vt,flags=element.desc.paramdesc.wParamFlags.ToString()});
      }
      members.Add(new {name=names[0],dispid=d.memid,descriptor_slot=d.oVft/IntPtr.Size,invoke_kind=d.invkind.ToString(),parameters});
     } finally {type.ReleaseFuncDesc(fp);}
    }
    layouts.Add(new {name,iid=a.guid.ToString(),kind=a.typekind.ToString(),dual,vtable_bytes=a.cbSizeVft,members});
   } finally {type.ReleaseTypeAttr(attr);Marshal.ReleaseComObject(type);}
  }} finally {Marshal.ReleaseComObject(lib);}
  return layouts.ToArray();
 }
}
'@
[ordered]@{
    measured_utc=[DateTime]::UtcNow.ToString('O')
    excel_file_version=(Get-Item -LiteralPath $ExcelPath).VersionInfo.FileVersion
    excel_sha256=(Get-FileHash -LiteralPath $ExcelPath).Hash
    pointer_bytes=[IntPtr]::Size
    interfaces=[ExcelInterfaceFacts]::Read($ExcelPath)
} | ConvertTo-Json -Depth 10
