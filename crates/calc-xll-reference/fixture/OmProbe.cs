// SPDX-License-Identifier: MIT
// dc-1nbp.6 (14.e): object-model probes E6, E11 and E1 of harvest 14 section 4, added to the X19
// Excel-DNA client (ComProbe.cs). Each probe is an Excel-DNA command run by Application.Run; it
// builds its own workbook and writes one tab-separated line per observation (case, route, what,
// value) to om-<probe>.tsv beside the loaded XLL. Four routes reach the same object:
//   pia     the primary interop assembly (early bound, the dual interface vtable, .NET marshalling)
//   dynamic C# dynamic (late bound, IDispatch, .NET marshalling)
//   vtable  the dual interface slot called by hand (early bound, the raw VARIANT that comes back)
//   disp    IDispatch::Invoke called by hand (late bound, the raw VARIANT, HRESULT and EXCEPINFO)
// Slots and parameter layouts come from the object's own ITypeInfo at run time.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;
using ExcelDna.Integration;
using CT = System.Runtime.InteropServices.ComTypes;
using Excel = Microsoft.Office.Interop.Excel;

[ComImport, Guid("1CF2B120-547D-101B-8E65-08002B2BD119"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IOmErrorInfo
{
    [PreserveSig] int GetGUID(out Guid guid);
    [PreserveSig] int GetSource([MarshalAs(UnmanagedType.BStr)] out string source);
    [PreserveSig] int GetDescription([MarshalAs(UnmanagedType.BStr)] out string description);
    [PreserveSig] int GetHelpFile([MarshalAs(UnmanagedType.BStr)] out string file);
    [PreserveSig] int GetHelpContext(out int context);
}

public static class OmProbe
{
    private const int Lcid = 1033;
    private static readonly CultureInfo CI = CultureInfo.InvariantCulture;
    private static readonly object M = Type.Missing;
    private const int DISP_E_PARAMNOTFOUND = unchecked((int)0x80020004);
    private const int XlErrNA = unchecked((int)0x800A07FA);
    private const ushort GET = 2, METHOD = 1;

    // ---------------------------------------------------------------- output
    private static List<string> log;
    private static string currentCase = "";
    private static void Rec(string route, string what, string value) =>
        log.Add(currentCase + "\t" + route + "\t" + what + "\t" + value.Replace("\t", "\\t").Replace("\r", "\\r").Replace("\n", "\\n"));
    private static void Try(string route, string what, Func<object> read)
    {
        try { Rec(route, what, Managed(read())); }
        catch (Exception e) { Rec(route, what, Exc(e)); }
    }
    private static void Begin(string name) { log = new List<string> { "case\troute\twhat\tvalue" }; currentCase = ""; }
    private static void Flush(string name)
    {
        var dir = Path.GetDirectoryName((string)ExcelDnaUtil.XllPath);
        File.WriteAllLines(Path.Combine(dir, "om-" + name + ".tsv"), log, new UTF8Encoding(false));
    }
    private static string Quote(string s) => s == null ? "null" : "\"" + s.Replace("\"", "\"\"") + "\"";
    private static string Bits(double d) => BitConverter.DoubleToInt64Bits(d).ToString("X16");
    private static string Exc(Exception e)
    {
        while (e is TargetInvocationException && e.InnerException != null) e = e.InnerException;
        return "raised " + e.GetType().Name + " HRESULT=" + ((uint)e.HResult).ToString("X8") + " message=" + Quote(e.Message);
    }

    private static string Managed(object v)
    {
        if (v == null) return "null";
        if (v is DBNull) return "DBNull";
        if (v is double d) return "Double:" + d.ToString("R", CI) + " bits=" + Bits(d);
        if (v is DateTime t) return "DateTime:" + t.ToString("yyyy-MM-ddTHH:mm:ss.fffffff", CI) + " oa=" + t.ToOADate().ToString("R", CI) + " bits=" + Bits(t.ToOADate());
        if (v is decimal m) return "Decimal:" + m.ToString(CI);
        if (v is int i) return "Int32:" + i + " hex=" + ((uint)i).ToString("X8");
        if (v is string s) return "String:" + Quote(s) + " len=" + s.Length;
        if (v is bool b) return "Boolean:" + b;
        if (v is Array a)
        {
            var sb = new StringBuilder("Array:" + a.GetType().GetElementType().Name + " rank" + a.Rank);
            for (int r = 0; r < a.Rank; r++) sb.Append(" [" + a.GetLowerBound(r) + ".." + a.GetUpperBound(r) + "]");
            if (a.Rank == 2)
                for (int x = a.GetLowerBound(0); x <= a.GetUpperBound(0); x++)
                    for (int y = a.GetLowerBound(1); y <= a.GetUpperBound(1); y++)
                        sb.Append(" (" + x + "," + y + ")=" + Managed(a.GetValue(x, y)));
            return sb.ToString();
        }
        if (Marshal.IsComObject(v)) return "COM:" + ComTypeName(v);
        return v.GetType().Name + ":" + Convert.ToString(v, CI);
    }

    private static string ComTypeName(object o)
    {
        IntPtr pd = IntPtr.Zero;
        try { pd = Marshal.GetIDispatchForObject(o); return DispTypeName(pd); }
        catch (Exception e) { return "?" + Exc(e); }
        finally { if (pd != IntPtr.Zero) Marshal.Release(pd); }
    }

    // ---------------------------------------------------------------- native
    [DllImport("oleaut32.dll")] private static extern int VariantClear(IntPtr v);
    [DllImport("oleaut32.dll")] private static extern int SafeArrayGetDim(IntPtr psa);
    [DllImport("oleaut32.dll")] private static extern int SafeArrayGetLBound(IntPtr psa, int dim, out int lb);
    [DllImport("oleaut32.dll")] private static extern int SafeArrayGetUBound(IntPtr psa, int dim, out int ub);
    [DllImport("oleaut32.dll")] private static extern int SafeArrayGetVartype(IntPtr psa, out ushort vt);
    [DllImport("oleaut32.dll")] private static extern int SafeArrayAccessData(IntPtr psa, out IntPtr data);
    [DllImport("oleaut32.dll")] private static extern int SafeArrayUnaccessData(IntPtr psa);
    [DllImport("oleaut32.dll")] private static extern int GetErrorInfo(int reserved, out IntPtr info);
    [DllImport("oleaut32.dll")] private static extern int SetErrorInfo(int reserved, IntPtr info);
    [DllImport("oleaut32.dll")] private static extern void SysFreeString(IntPtr b);

    [UnmanagedFunctionPointer(CallingConvention.StdCall)] private delegate int F1(IntPtr a);
    [UnmanagedFunctionPointer(CallingConvention.StdCall)] private delegate int F2(IntPtr a, IntPtr b);
    [UnmanagedFunctionPointer(CallingConvention.StdCall)] private delegate int F3(IntPtr a, IntPtr b, IntPtr c);
    [UnmanagedFunctionPointer(CallingConvention.StdCall)] private delegate int F4(IntPtr a, IntPtr b, IntPtr c, IntPtr d);
    [UnmanagedFunctionPointer(CallingConvention.StdCall)] private delegate int F5(IntPtr a, IntPtr b, IntPtr c, IntPtr d, IntPtr e);
    [UnmanagedFunctionPointer(CallingConvention.StdCall)] private delegate int F6(IntPtr a, IntPtr b, IntPtr c, IntPtr d, IntPtr e, IntPtr f);
    [UnmanagedFunctionPointer(CallingConvention.StdCall)] private delegate int F7(IntPtr a, IntPtr b, IntPtr c, IntPtr d, IntPtr e, IntPtr f, IntPtr g);
    [UnmanagedFunctionPointer(CallingConvention.StdCall)] private delegate int F8(IntPtr a, IntPtr b, IntPtr c, IntPtr d, IntPtr e, IntPtr f, IntPtr g, IntPtr h);
    [UnmanagedFunctionPointer(CallingConvention.StdCall)] private delegate int F9(IntPtr a, IntPtr b, IntPtr c, IntPtr d, IntPtr e, IntPtr f, IntPtr g, IntPtr h, IntPtr i);

    private static int CallSlot(IntPtr fn, IntPtr[] a)
    {
        switch (a.Length)
        {
            case 1: return Marshal.GetDelegateForFunctionPointer<F1>(fn)(a[0]);
            case 2: return Marshal.GetDelegateForFunctionPointer<F2>(fn)(a[0], a[1]);
            case 3: return Marshal.GetDelegateForFunctionPointer<F3>(fn)(a[0], a[1], a[2]);
            case 4: return Marshal.GetDelegateForFunctionPointer<F4>(fn)(a[0], a[1], a[2], a[3]);
            case 5: return Marshal.GetDelegateForFunctionPointer<F5>(fn)(a[0], a[1], a[2], a[3], a[4]);
            case 6: return Marshal.GetDelegateForFunctionPointer<F6>(fn)(a[0], a[1], a[2], a[3], a[4], a[5]);
            case 7: return Marshal.GetDelegateForFunctionPointer<F7>(fn)(a[0], a[1], a[2], a[3], a[4], a[5], a[6]);
            case 8: return Marshal.GetDelegateForFunctionPointer<F8>(fn)(a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7]);
            case 9: return Marshal.GetDelegateForFunctionPointer<F9>(fn)(a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7], a[8]);
        }
        throw new ArgumentException("arity " + a.Length);
    }
    private static IntPtr Slot(IntPtr itf, int index) => Marshal.ReadIntPtr(Marshal.ReadIntPtr(itf), index * IntPtr.Size);
    private static IntPtr NewVariant() { var v = Marshal.AllocHGlobal(24); for (int i = 0; i < 24; i += 8) Marshal.WriteInt64(v, i, 0); return v; }
    private static void FreeVariant(IntPtr v) { VariantClear(v); Marshal.FreeHGlobal(v); }

    // An argument as the caller puts it in a VARIANT.
    public sealed class Arg
    {
        public string Kind; public object Value;
        public static readonly Arg Omit = new Arg { Kind = "omitted" };       // not passed (IDispatch: fewer cArgs)
        public static readonly Arg Pnf = new Arg { Kind = "VT_ERROR(DISP_E_PARAMNOTFOUND)" };
        public static readonly Arg Empty = new Arg { Kind = "VT_EMPTY" };
        public static readonly Arg Null = new Arg { Kind = "VT_NULL" };
        public static readonly Arg ErrNA = new Arg { Kind = "VT_ERROR(xlErrNA 800A07FA)" };
        public static Arg Of(object v) => new Arg { Kind = "value", Value = v };
        public override string ToString() => Kind == "value" ? Managed(Value) : Kind;
    }
    private static void WriteArg(IntPtr v, Arg a)
    {
        switch (a.Kind)
        {
            case "omitted":
            case "VT_ERROR(DISP_E_PARAMNOTFOUND)": Marshal.WriteInt16(v, 0, 10); Marshal.WriteInt32(v, 8, DISP_E_PARAMNOTFOUND); break;
            case "VT_EMPTY": break;
            case "VT_NULL": Marshal.WriteInt16(v, 0, 1); break;
            case "VT_ERROR(xlErrNA 800A07FA)": Marshal.WriteInt16(v, 0, 10); Marshal.WriteInt32(v, 8, XlErrNA); break;
            default: Marshal.GetNativeVariantForObject(a.Value, v); break;
        }
    }

    private static string DispTypeName(IntPtr pdisp)
    {
        IntPtr pti;
        int hr = CallSlot(Slot(pdisp, 4), new[] { pdisp, IntPtr.Zero, (IntPtr)Lcid, Out(out IntPtr cell) });
        pti = Marshal.ReadIntPtr(cell); Marshal.FreeHGlobal(cell);
        if (hr < 0 || pti == IntPtr.Zero) return "(GetTypeInfo " + ((uint)hr).ToString("X8") + ")";
        try
        {
            var ti = (CT.ITypeInfo)Marshal.GetObjectForIUnknown(pti);
            ti.GetDocumentation(-1, out string name, out _, out _, out _);
            return name;
        }
        finally { Marshal.Release(pti); }
    }
    private static IntPtr Out(out IntPtr cell) { cell = Marshal.AllocHGlobal(16); Marshal.WriteInt64(cell, 0, 0); Marshal.WriteInt64(cell, 8, 0); return cell; }

    // The raw VARIANT, every type with its exact payload.
    private static string Raw(IntPtr v)
    {
        ushort vt = (ushort)Marshal.ReadInt16(v);
        if ((vt & 0x4000) != 0) return "VT_BYREF|" + (vt & 0xBFFF);
        if ((vt & 0x2000) != 0) return "VT_ARRAY|" + VtName((ushort)(vt & 0x0FFF)) + " " + RawArray(Marshal.ReadIntPtr(v, 8));
        switch (vt)
        {
            case 0: return "VT_EMPTY";
            case 1: return "VT_NULL";
            case 2: return "VT_I2:" + Marshal.ReadInt16(v, 8);
            case 3: return "VT_I4:" + Marshal.ReadInt32(v, 8);
            case 5: { long b = Marshal.ReadInt64(v, 8); return "VT_R8:" + BitConverter.Int64BitsToDouble(b).ToString("R", CI) + " bits=" + b.ToString("X16"); }
            case 6: { long c = Marshal.ReadInt64(v, 8); return "VT_CY:int64=" + c + " (" + (c / 10000m).ToString(CI) + ")"; }
            case 7:
                {
                    long b = Marshal.ReadInt64(v, 8); double d = BitConverter.Int64BitsToDouble(b);
                    return "VT_DATE:" + d.ToString("R", CI) + " bits=" + b.ToString("X16") + " (" + DateTime.FromOADate(d).ToString("yyyy-MM-ddTHH:mm:ss.fff", CI) + ")";
                }
            case 8: { var p = Marshal.ReadIntPtr(v, 8); if (p == IntPtr.Zero) return "VT_BSTR:nullptr"; var s = Marshal.PtrToStringBSTR(p); return "VT_BSTR:" + Quote(s) + " len=" + s.Length; }
            case 9: { var p = Marshal.ReadIntPtr(v, 8); return p == IntPtr.Zero ? "VT_DISPATCH:nullptr" : "VT_DISPATCH:" + DispTypeName(p); }
            case 10: return "VT_ERROR:" + ((uint)Marshal.ReadInt32(v, 8)).ToString("X8");
            case 11: return "VT_BOOL:" + Marshal.ReadInt16(v, 8);
            case 13: return "VT_UNKNOWN";
            case 14: return "VT_DECIMAL:" + Convert.ToString(Marshal.GetObjectForNativeVariant(v), CI);
            case 20: return "VT_I8:" + Marshal.ReadInt64(v, 8);
        }
        return "VT_" + vt;
    }
    private static string VtName(ushort vt) => vt == 12 ? "VT_VARIANT" : "VT_" + vt;
    private static string RawArray(IntPtr psa)
    {
        int dims = SafeArrayGetDim(psa);
        SafeArrayGetVartype(psa, out ushort evt);
        var sb = new StringBuilder("dims=" + dims + " elem=" + VtName(evt));
        var lb = new int[dims]; var n = new int[dims]; int total = 1;
        for (int d = 0; d < dims; d++)
        {
            SafeArrayGetLBound(psa, d + 1, out lb[d]); SafeArrayGetUBound(psa, d + 1, out int ub);
            n[d] = ub - lb[d] + 1; total *= n[d];
            sb.Append(" [" + lb[d] + ".." + ub + "]");
        }
        if (evt != 12) return sb.ToString();
        SafeArrayAccessData(psa, out IntPtr data);
        try
        {
            // SAFEARRAY data is stored with the leftmost index varying fastest.
            for (int k = 0; k < total; k++)
            {
                string idx = dims == 2 ? (lb[0] + k % n[0]) + "," + (lb[1] + k / n[0]) : (lb[0] + k).ToString(CI);
                sb.Append(" (" + idx + ")=" + Raw(data + k * 24));
            }
        }
        finally { SafeArrayUnaccessData(psa); }
        return sb.ToString();
    }

    private static string TakeErrorInfo()
    {
        if (GetErrorInfo(0, out IntPtr p) != 0 || p == IntPtr.Zero) return "IErrorInfo=none";
        try
        {
            var ei = (IOmErrorInfo)Marshal.GetObjectForIUnknown(p);
            ei.GetSource(out string src); ei.GetDescription(out string desc);
            return "IErrorInfo source=" + Quote(src) + " description=" + Quote(desc);
        }
        finally { Marshal.Release(p); }
    }

    // IDispatch::Invoke by hand. Arguments in natural order; omitted ones are dropped from the end
    // (an Omit in the middle is passed as DISP_E_PARAMNOTFOUND, which is what omission means there).
    private static string Disp(object target, string name, ushort flags, Arg[] args, bool excepInfo = true)
    {
        IntPtr pd = Marshal.GetIDispatchForObject(target);
        var natural = new List<Arg>(args);
        while (natural.Count > 0 && natural[natural.Count - 1].Kind == "omitted") natural.RemoveAt(natural.Count - 1);
        int n = natural.Count;
        IntPtr rg = n == 0 ? IntPtr.Zero : Marshal.AllocHGlobal(24 * n);
        for (int i = 0; i < 3 * n; i++) Marshal.WriteInt64(rg, 8 * i, 0);
        IntPtr dp = Marshal.AllocHGlobal(24), res = NewVariant(), ex = Marshal.AllocHGlobal(64), argErr = Marshal.AllocHGlobal(8);
        IntPtr namePtr = Marshal.StringToHGlobalUni(name), names = Marshal.AllocHGlobal(8), ids = Marshal.AllocHGlobal(8), iid = Marshal.AllocHGlobal(16);
        try
        {
            for (int i = 0; i < 16; i += 8) Marshal.WriteInt64(iid, i, 0);
            Marshal.WriteIntPtr(names, namePtr);
            int hr = CallSlot(Slot(pd, 5), new[] { pd, iid, names, (IntPtr)1, (IntPtr)Lcid, ids });
            if (hr < 0) return "GetIDsOfNames HRESULT=" + ((uint)hr).ToString("X8");
            int dispid = Marshal.ReadInt32(ids);
            for (int i = 0; i < n; i++)
            {
                IntPtr slot = rg + 24 * (n - 1 - i);    // rgvarg is in reverse order
                for (int j = 0; j < 24; j += 8) Marshal.WriteInt64(slot, j, 0);
                WriteArg(slot, natural[i]);
            }
            Marshal.WriteIntPtr(dp, 0, rg); Marshal.WriteIntPtr(dp, 8, IntPtr.Zero); Marshal.WriteInt32(dp, 16, n); Marshal.WriteInt32(dp, 20, 0);
            for (int i = 0; i < 64; i += 8) Marshal.WriteInt64(ex, i, 0);
            Marshal.WriteInt64(argErr, 0, -1);
            SetErrorInfo(0, IntPtr.Zero);
            hr = CallSlot(Slot(pd, 6), new[] { pd, (IntPtr)dispid, iid, (IntPtr)Lcid, (IntPtr)flags, dp, res, excepInfo ? ex : IntPtr.Zero, argErr });
            var sb = new StringBuilder("dispid=0x" + dispid.ToString("X") + " cArgs=" + n + " HRESULT=" + ((uint)hr).ToString("X8"));
            if (hr >= 0) sb.Append(" result=" + Raw(res));
            else
            {
                if (excepInfo)
                {
                    IntPtr fill = Marshal.ReadIntPtr(ex, 48);
                    if (fill != IntPtr.Zero) { CallSlot(fill, new[] { ex }); sb.Append(" (deferred fill-in called)"); }
                    IntPtr src = Marshal.ReadIntPtr(ex, 8), desc = Marshal.ReadIntPtr(ex, 16), help = Marshal.ReadIntPtr(ex, 24);
                    sb.Append(" EXCEPINFO wCode=" + (ushort)Marshal.ReadInt16(ex, 0) + " scode=" + ((uint)Marshal.ReadInt32(ex, 56)).ToString("X8")
                        + " source=" + Quote(src == IntPtr.Zero ? null : Marshal.PtrToStringBSTR(src))
                        + " description=" + Quote(desc == IntPtr.Zero ? null : Marshal.PtrToStringBSTR(desc))
                        + " helpfile=" + Quote(help == IntPtr.Zero ? null : Marshal.PtrToStringBSTR(help)) + " helpcontext=" + Marshal.ReadInt32(ex, 32));
                    if (src != IntPtr.Zero) SysFreeString(src); if (desc != IntPtr.Zero) SysFreeString(desc); if (help != IntPtr.Zero) SysFreeString(help);
                }
                else sb.Append(" (no EXCEPINFO passed)");
                sb.Append(" argErr=" + Marshal.ReadInt32(argErr) + " " + TakeErrorInfo());
            }
            return sb.ToString();
        }
        finally
        {
            for (int i = 0; i < n; i++) VariantClear(rg + 24 * i);
            if (rg != IntPtr.Zero) Marshal.FreeHGlobal(rg);
            FreeVariant(res); Marshal.FreeHGlobal(dp); Marshal.FreeHGlobal(ex); Marshal.FreeHGlobal(argErr);
            Marshal.FreeHGlobal(namePtr); Marshal.FreeHGlobal(names); Marshal.FreeHGlobal(ids); Marshal.FreeHGlobal(iid);
            Marshal.Release(pd);
        }
    }

    // The dual interface's vtable slot by hand: the slot and parameter flags from ITypeInfo.
    private static string VCall(object target, string name, CT.INVOKEKIND kind, Arg[] args)
    {
        IntPtr pd = Marshal.GetIDispatchForObject(target), pti = IntPtr.Zero, pitf = IntPtr.Zero;
        var allocated = new List<IntPtr>();
        try
        {
            CallSlot(Slot(pd, 4), new[] { pd, IntPtr.Zero, (IntPtr)Lcid, Out(out IntPtr cell) });
            pti = Marshal.ReadIntPtr(cell); Marshal.FreeHGlobal(cell);
            var ti = (CT.ITypeInfo)Marshal.GetObjectForIUnknown(pti);
            ti.GetTypeAttr(out IntPtr pa); var ta = Marshal.PtrToStructure<CT.TYPEATTR>(pa); ti.ReleaseTypeAttr(pa);
            string via = "dual";
            if (ta.typekind == CT.TYPEKIND.TKIND_DISPATCH)
            {
                CT.ITypeInfo vti = null;
                try { ti.GetRefTypeOfImplType(-1, out int href); ti.GetRefTypeInfo(href, out vti); }
                catch (COMException e) when (e.HResult == unchecked((int)0x8002802B))
                {
                    // A pure dispinterface (Excel's Range, Areas): the vtable twin is a separate
                    // interface named I<name> in the same type library (IRange, IID ...-0001-...).
                    ti.GetDocumentation(-1, out string dname, out _, out _, out _);
                    ti.GetContainingTypeLib(out CT.ITypeLib lib, out _);
                    for (int i = 0; i < lib.GetTypeInfoCount() && vti == null; i++)
                    {
                        lib.GetDocumentation(i, out string n2, out _, out _, out _);
                        if (n2 == "I" + dname) lib.GetTypeInfo(i, out vti);
                    }
                    if (vti == null) return "no vtable twin for dispinterface " + dname;
                    via = "I" + dname;
                }
                ti = vti;
                ti.GetTypeAttr(out pa); ta = Marshal.PtrToStructure<CT.TYPEATTR>(pa); ti.ReleaseTypeAttr(pa);
            }
            var ids = new int[1]; ti.GetIDsOfNames(new[] { name }, 1, ids);
            CT.FUNCDESC fd = default; bool found = false;
            for (int i = 0; i < ta.cFuncs && !found; i++)
            {
                ti.GetFuncDesc(i, out IntPtr pf); var f = Marshal.PtrToStructure<CT.FUNCDESC>(pf);
                if (f.memid == ids[0] && f.invkind == kind) { fd = f; found = true; }
                else ti.ReleaseFuncDesc(pf);
            }
            if (!found) return "no vtable member " + name;
            var guid = ta.guid; int qi = Marshal.QueryInterface(pd, ref guid, out pitf);
            if (qi < 0 || pitf == IntPtr.Zero) { pitf = IntPtr.Zero; return "QueryInterface(" + via + " " + guid + ") HRESULT=" + ((uint)qi).ToString("X8"); }
            int slot = fd.oVft / IntPtr.Size;
            var native = new List<IntPtr> { pitf }; IntPtr ret = IntPtr.Zero; short retVt = 0; int next = 0;
            var layout = new StringBuilder();
            int esize = Marshal.SizeOf<CT.ELEMDESC>();
            for (int p = 0; p < fd.cParams; p++)
            {
                var ed = Marshal.PtrToStructure<CT.ELEMDESC>(fd.lprgelemdescParam + p * esize);
                var fl = ed.desc.paramdesc.wParamFlags;
                if ((fl & CT.PARAMFLAG.PARAMFLAG_FLCID) != 0) { native.Add((IntPtr)Lcid); layout.Append("lcid,"); }
                else if ((fl & CT.PARAMFLAG.PARAMFLAG_FRETVAL) != 0)
                {
                    ret = NewVariant(); allocated.Add(ret); native.Add(ret);
                    var inner = Marshal.PtrToStructure<CT.TYPEDESC>(ed.tdesc.lpValue); retVt = inner.vt; layout.Append("retval(VT_" + inner.vt + "),");
                }
                else
                {
                    if (ed.tdesc.vt != 12) return "parameter " + p + " is VT_" + ed.tdesc.vt + ", not VARIANT";
                    var v = NewVariant(); allocated.Add(v);
                    WriteArg(v, next < args.Length ? args[next] : Arg.Pnf); next++;
                    native.Add(v); layout.Append("VARIANT,");     // a by-value VARIANT travels as a pointer to a copy on x64
                }
            }
            SetErrorInfo(0, IntPtr.Zero);
            int hr = CallSlot(Slot(pitf, slot), native.ToArray());
            var sb = new StringBuilder("via=" + via + " slot=" + slot + " params=" + layout.ToString().TrimEnd(',') + " HRESULT=" + ((uint)hr).ToString("X8"));
            if (hr >= 0 && ret != IntPtr.Zero)
            {
                if (retVt == 12) sb.Append(" result=" + Raw(ret));
                else if (retVt == 26 || retVt == 29) { var p = Marshal.ReadIntPtr(ret); sb.Append(" result=interface " + (p == IntPtr.Zero ? "null" : DispTypeName(p))); if (p != IntPtr.Zero) Marshal.Release(p); Marshal.WriteInt64(ret, 0, 0); }
                else if (retVt == 8) { var p = Marshal.ReadIntPtr(ret); sb.Append(" result=BSTR " + Quote(Marshal.PtrToStringBSTR(p))); SysFreeString(p); Marshal.WriteInt64(ret, 0, 0); }
                else if (retVt == 3) { sb.Append(" result=I4 " + Marshal.ReadInt32(ret)); Marshal.WriteInt64(ret, 0, 0); }
                else { sb.Append(" result=VT_" + retVt + " raw=" + Marshal.ReadInt64(ret).ToString("X16")); Marshal.WriteInt64(ret, 0, 0); }
            }
            if (hr < 0) sb.Append(" " + TakeErrorInfo());
            return sb.ToString();
        }
        catch (Exception e) { return Exc(e); }
        finally
        {
            foreach (var v in allocated) FreeVariant(v);
            if (pitf != IntPtr.Zero) Marshal.Release(pitf);
            if (pti != IntPtr.Zero) Marshal.Release(pti);
            Marshal.Release(pd);
        }
    }
    private static void RecDisp(string what, object target, string name, ushort flags, params Arg[] args) => RecSafe("disp", what, () => Disp(target, name, flags, args));
    private static void RecDispNoEx(string what, object target, string name, ushort flags, params Arg[] args) => RecSafe("disp", what, () => Disp(target, name, flags, args, false));
    private static void RecV(string what, object target, string name, CT.INVOKEKIND kind, params Arg[] args) => RecSafe("vtable", what, () => VCall(target, name, kind, args));
    private static void RecSafe(string route, string what, Func<string> f) { try { Rec(route, what, f()); } catch (Exception e) { Rec(route, what, Exc(e)); } }

    private static Excel.Application App => (Excel.Application)ExcelDnaUtil.Application;
    private static string A1(Excel.Range r) => r.get_Address(M, M, Excel.XlReferenceStyle.xlA1, M, M);

    // ---------------------------------------------------------------- E6
    [ExcelCommand(Name = "OM_E6")]
    public static void E6()
    {
        Begin("e6");
        var app = App;
        try
        {
            currentCase = "env";
            Try("pia", "Version", () => app.Version);
            Try("pia", "Build", () => app.Build);
            Try("pia", "International(xlCountrySetting)", () => app.International[Excel.XlApplicationInternational.xlCountrySetting]);
            Try("pia", "DecimalSeparator", () => app.DecimalSeparator);
            Try("pia", "UseSystemSeparators", () => app.UseSystemSeparators);
            Try("pia", "Calculation", () => (int)app.Calculation);
            Try("pia", "lcid used by raw routes", () => Lcid);
            var book = app.Workbooks.Add(M);
            var s = (Excel.Worksheet)book.Worksheets[1];
            s.Name = "S";
            s.Range["A1", M].Value2 = 42.0;
            s.Range["A2", M].Value2 = 1.0; s.Range["B2", M].Value2 = "x"; s.Range["A3", M].Value2 = true; s.Range["B3", M].Value2 = 2.5;
            s.Range["A6", M].Formula = "=\"\"";
            s.Range["A7", M].Value2 = "'";
            s.Range["A8", M].Formula = "=NA()";
            s.Range["A9", M].Value2 = 45292.25; s.Range["A9", M].NumberFormat = "yyyy-mm-dd hh:mm";
            s.Range["A10", M].Value2 = 123.45678901234567; s.Range["A10", M].NumberFormat = "$#,##0.00";
            s.Range["A11", M].Value2 = 123.45678901234567; s.Range["A11", M].NumberFormat = "_($* #,##0.00_);_($* (#,##0.00);_($* \"-\"??_);_(@_)";
            s.Range["A12", M].Value2 = 45292.25; s.Range["A12", M].NumberFormat = "hh:mm:ss";
            s.Range["A13", M].Value2 = 123.45678901234567; s.Range["A13", M].Style = "Currency";
            s.Range["A14", M].Value2 = 123.45678901234567; s.Range["A14", M].NumberFormat = "[$$-409]#,##0.00";
            s.Range["A15", M].Value2 = 123.45678901234567; s.Range["A15", M].NumberFormat = "[$R-1C09] #,##0.00";
            s.Range["A16", M].Value2 = -0.00005; s.Range["A16", M].Style = "Currency";
            s.Range["A17", M].Value2 = 922337203685477.5807; s.Range["A17", M].Style = "Currency";
            s.Range["A18", M].Value2 = 1e300; s.Range["A18", M].Style = "Currency";
            s.Range["A19", M].Value2 = 45292.25; s.Range["A19", M].NumberFormat = "yyyy-mm-dd hh:mm"; s.Range["A19", M].Style = "Currency";
            s.Range["A20", M].Value2 = -1.0; s.Range["A20", M].NumberFormat = "yyyy-mm-dd";
            s.Range["A21", M].Value2 = 3000000.0; s.Range["A21", M].NumberFormat = "yyyy-mm-dd";
            var cells = new[] { "A1", "A2:B3", "A5", "A6", "A7", "A8", "A9", "A10", "A11", "A12", "A13", "A14", "A15", "A16", "A17", "A18", "A19", "A20", "A21", "A9:A10" };
            var labels = new Dictionary<string, string> {
                {"A1","scalar 42"}, {"A2:B3","2x2 block {1,\"x\";TRUE,2.5}"}, {"A5","empty cell"}, {"A6","formula =\"\""},
                {"A7","constant ' (empty text)"}, {"A8","=NA()"}, {"A9","45292.25 yyyy-mm-dd hh:mm"}, {"A10","123.45678901234567 $#,##0.00"},
                {"A11","123.45678901234567 accounting"}, {"A12","45292.25 hh:mm:ss"}, {"A13","123.45678901234567 Style Currency"},
                {"A14","123.45678901234567 [$$-409]#,##0.00"}, {"A15","123.45678901234567 [$R-1C09] #,##0.00"}, {"A16","-0.00005 Style Currency"},
                {"A17","922337203685477.5807 Style Currency"}, {"A18","1e300 Style Currency"}, {"A19","45292.25 date then Style Currency"},
                {"A20","-1 yyyy-mm-dd"}, {"A21","3000000 yyyy-mm-dd"}, {"A9:A10","date over currency block"} };
            foreach (var c in cells)
            {
                currentCase = c + " " + labels[c];
                var r = s.Range[c, M];
                Try("pia", "NumberFormat", () => r.NumberFormat);
                Try("pia", "Text", () => r.Text);
                Try("pia", "Value", () => r.Value[M]);
                Try("pia", "Value2", () => r.Value2);
                dynamic dr = r;
                Try("dynamic", "Value", () => dr.Value);
                Try("dynamic", "Value2", () => dr.Value2);
                RecV("Value", r, "Value", CT.INVOKEKIND.INVOKE_PROPERTYGET);
                RecV("Value2", r, "Value2", CT.INVOKEKIND.INVOKE_PROPERTYGET);
                RecDisp("Value", r, "Value", GET);
                RecDisp("Value2", r, "Value2", GET);
            }

            // Optional arguments: omitted, PARAMNOTFOUND, Empty, Null, Error, and a plain value.
            var opts = new[] { Arg.Omit, Arg.Pnf, Arg.Empty, Arg.Null, Arg.ErrNA, Arg.Of(false) };
            var a9 = s.Range["A9", M];
            var b2 = s.Range["B2:C3", M];
            foreach (var o in opts)
            {
                currentCase = "optional " + o;
                RecDisp("Range(B2:C3).Address(RowAbsolute=arg)", b2, "Address", GET, o);
                RecV("Range(B2:C3).Address(RowAbsolute=arg)", b2, "Address", CT.INVOKEKIND.INVOKE_PROPERTYGET, o);
                RecDisp("Application.Range(\"A1\", Cell2=arg)", app, "Range", GET, Arg.Of("A1"), o);
                RecV("Application.Range(\"A1\", Cell2=arg)", app, "Range", CT.INVOKEKIND.INVOKE_PROPERTYGET, Arg.Of("A1"), o);
                RecDisp("Range(A9).Value(RangeValueDataType=arg)", a9, "Value", GET, o);
                RecV("Range(A9).Value(RangeValueDataType=arg)", a9, "Value", CT.INVOKEKIND.INVOKE_PROPERTYGET, o);
                object po = o.Kind == "omitted" || o.Kind.StartsWith("VT_ERROR(DISP") ? M : o.Kind == "VT_EMPTY" ? null : o.Kind == "VT_NULL" ? (object)DBNull.Value
                    : o.Kind.StartsWith("VT_ERROR(xl") ? new ErrorWrapper(XlErrNA) : o.Value;
                Try("pia", "Range(B2:C3).Address(RowAbsolute=arg)", () => b2.get_Address(po, M, Excel.XlReferenceStyle.xlA1, M, M));
                Try("pia", "Application.Range(\"A1\", Cell2=arg)", () => A1(app.Range["A1", po]));
                Try("pia", "Range(A9).Value(RangeValueDataType=arg)", () => a9.Value[po]);
            }

            // A failing call through each route, with and without EXCEPINFO.
            foreach (var bad in new[] { "not a range", "Elsewhere!A1" })
            {
                currentCase = "failure Application.Range(\"" + bad + "\")";
                RecDisp("with EXCEPINFO", app, "Range", GET, Arg.Of(bad));
                RecDispNoEx("EXCEPINFO null", app, "Range", GET, Arg.Of(bad));
                RecV("vtable", app, "Range", CT.INVOKEKIND.INVOKE_PROPERTYGET, Arg.Of(bad));
                Try("pia", "Application.Range", () => app.Range[bad, M]);
                dynamic dapp = app;
                Try("dynamic", "Application.Range", () => dapp.Range(bad));
            }
            currentCase = "failure Range(A1).Offset(-1, 0)";
            var a1 = s.Range["A1", M];
            RecDisp("with EXCEPINFO", a1, "Offset", GET, Arg.Of(-1), Arg.Of(0));
            RecDispNoEx("EXCEPINFO null", a1, "Offset", GET, Arg.Of(-1), Arg.Of(0));
            RecV("vtable", a1, "Offset", CT.INVOKEKIND.INVOKE_PROPERTYGET, Arg.Of(-1), Arg.Of(0));
            Try("pia", "Offset", () => a1.Offset[-1, 0]);
            book.Close(false, M, M);
        }
        catch (Exception e) { currentCase = "fatal"; Rec("-", "-", Exc(e) + " " + e.StackTrace); }
        Flush("e6");
    }

    // ---------------------------------------------------------------- E11
    private static void DescribeRange(string route, Excel.Range r)
    {
        Try(route, "Areas.Count", () => r.Areas.Count);
        Try(route, "Count", () => r.Count);
        Try(route, "CountLarge", () => r.CountLarge);
        Try(route, "Cells.Count", () => r.Cells.Count);
        Try(route, "Address", () => A1(r));
        Try(route, "Rows.Count", () => r.Rows.Count);
        Try(route, "Columns.Count", () => r.Columns.Count);
        Try(route, "Row", () => r.Row);
        Try(route, "Column", () => r.Column);
        Try(route, "Areas", () =>
        {
            var parts = new List<string>();
            for (int i = 1; i <= r.Areas.Count; i++) parts.Add(i + ":" + A1(r.Areas[i]) + "/" + r.Areas[i].Count);
            return string.Join(" ", parts);
        });
        Try(route, "For Each order", () =>
        {
            var parts = new List<string>();
            foreach (Excel.Range c in r) parts.Add(c.get_Address(false, false, Excel.XlReferenceStyle.xlA1, M, M));
            return parts.Count + ": " + string.Join(" ", parts);
        });
        Try(route, "Value2 shape", () => r.Value2);
        Try(route, "Address(External)", () => r.get_Address(M, M, Excel.XlReferenceStyle.xlA1, true, M));
    }

    [ExcelCommand(Name = "OM_E11")]
    public static void E11()
    {
        Begin("e11");
        var app = App;
        try
        {
            var book = app.Workbooks.Add(M);
            var s = (Excel.Worksheet)book.Worksheets[1];
            s.Name = "S";
            var t = (Excel.Worksheet)book.Worksheets.Add(M, s, M, M);
            t.Name = "T";
            s.Activate();
            Excel.Range R(string a) => s.Range[a, M];
            var unions = new (string, Func<Excel.Range>)[] {
                ("Union(B2:C3, F5, C3:D4)", () => app.Union(R("B2:C3"), R("F5"), R("C3:D4"))),
                ("Union(C3:D4, F5, B2:C3) reversed", () => app.Union(R("C3:D4"), R("F5"), R("B2:C3"))),
                ("Union(F5, B2:C3, C3:D4) F5 first", () => app.Union(R("F5"), R("B2:C3"), R("C3:D4"))),
                ("Union(B2:C3, B2:C3) same area twice", () => app.Union(R("B2:C3"), R("B2:C3"))),
                ("Union(B2:C3, B4:C5) adjacent", () => app.Union(R("B2:C3"), R("B4:C5"))),
                ("Range(\"B2:C3,F5,C3:D4\")", () => R("B2:C3,F5,C3:D4")),
            };
            foreach (var (label, make) in unions)
            {
                currentCase = label;
                Excel.Range u = null;
                Try("pia", "make", () => { u = make(); return A1(u); });
                if (u != null) DescribeRange("pia", u);
                if (label.StartsWith("Union(B2:C3, F5"))
                {
                    dynamic du = u;
                    Try("dynamic", "Count", () => du.Count);
                    Try("dynamic", "Address", () => du.Address);
                    Try("dynamic", "Rows.Count", () => du.Rows.Count);
                    RecDisp("Count", u, "Count", GET);
                    RecV("Count", u, "Count", CT.INVOKEKIND.INVOKE_PROPERTYGET);
                    RecDisp("Value2", u, "Value2", GET);
                }
            }

            currentCase = "Union across sheets S!A1, T!A1";
            Try("pia", "Union", () => A1(app.Union(R("A1"), t.Range["A1", M])));
            dynamic dapp = app;
            Try("dynamic", "Union", () => dapp.Union(R("A1"), t.Range["A1", M]).Address);
            RecDisp("Union with EXCEPINFO", app, "Union", METHOD, Arg.Of(R("A1")), Arg.Of(t.Range["A1", M]));
            RecDispNoEx("Union EXCEPINFO null", app, "Union", METHOD, Arg.Of(R("A1")), Arg.Of(t.Range["A1", M]));
            currentCase = "Range(\"S!A1,T!A1\")";
            Try("pia", "Application.Range", () => A1(app.Range["S!A1,T!A1", M]));
            currentCase = "Union with ranges of a second workbook";
            var other = app.Workbooks.Add(M);
            var o1 = (Excel.Worksheet)other.Worksheets[1];
            Try("pia", "Union(other!A1, other!B2), other book active", () => A1(app.Union(o1.Range["A1", M], o1.Range["B2", M])));
            Try("pia", "Union(S!A1, other!A1)", () => A1(app.Union(R("A1"), o1.Range["A1", M])));
            other.Close(false, M, M);
            s.Activate();

            // Address argument combinations on the multi-area range.
            var u1 = app.Union(R("B2:C3"), R("F5"), R("C3:D4"));
            var c3 = R("C3"); var a1 = R("A1");
            var r1c1 = Excel.XlReferenceStyle.xlR1C1; var xa1 = Excel.XlReferenceStyle.xlA1;
            var combos = new (string, object, object, Excel.XlReferenceStyle, object, object)[] {
                ("default", M, M, xa1, M, M),
                ("RowAbsolute=False", false, M, xa1, M, M),
                ("ColumnAbsolute=False", M, false, xa1, M, M),
                ("RowAbsolute=False ColumnAbsolute=False", false, false, xa1, M, M),
                ("xlR1C1", M, M, r1c1, M, M),
                ("xlR1C1 RowAbsolute=False ColumnAbsolute=False (no RelativeTo)", false, false, r1c1, M, M),
                ("xlR1C1 RowAbsolute=False ColumnAbsolute=False RelativeTo=A1", false, false, r1c1, M, a1),
                ("xlR1C1 RowAbsolute=False ColumnAbsolute=False RelativeTo=C3", false, false, r1c1, M, c3),
                ("xlR1C1 RowAbsolute=False RelativeTo=C3", false, M, r1c1, M, c3),
                ("xlR1C1 absolute RelativeTo=C3", M, M, r1c1, M, c3),
                ("xlA1 RowAbsolute=False ColumnAbsolute=False RelativeTo=C3", false, false, xa1, M, c3),
                ("External=True", M, M, xa1, true, M),
                ("External=True xlR1C1", M, M, r1c1, true, M),
                ("External=True xlR1C1 relative RelativeTo=C3", false, false, r1c1, true, c3),
                ("External=False", M, M, xa1, false, M),
            };
            foreach (var (label, ra, ca, st, ex, rel) in combos)
            {
                currentCase = "Address " + label;
                Try("pia", "Union(B2:C3,F5,C3:D4)", () => u1.get_Address(ra, ca, st, ex, rel));
                Try("pia", "single B2:C3", () => R("B2:C3").get_Address(ra, ca, st, ex, rel));
            }
            currentCase = "Address RelativeTo on another sheet";
            Try("pia", "xlR1C1 relative RelativeTo=T!C3", () => u1.get_Address(false, false, r1c1, M, t.Range["C3", M]));
            currentCase = "Address RelativeTo a multi-cell range";
            Try("pia", "xlR1C1 relative RelativeTo=C3:D4", () => u1.get_Address(false, false, r1c1, M, R("C3:D4")));
            currentCase = "AddressLocal R1C1";
            Try("pia", "AddressLocal xlR1C1", () => u1.get_AddressLocal(M, M, r1c1, M, M));

            // Multi-area assignment.
            var targets = new[] { "B2", "C2", "B3", "C3", "F5", "G5", "F6", "G6", "D4", "H7" };
            var assigns = new (string, object)[] { ("scalar 7", 7.0), ("2x2 {1,2;3,4}", new object[,] { { 1.0, 2.0 }, { 3.0, 4.0 } }), ("1x1 {9}", new object[,] { { 9.0 } }), ("1x3 {1,2,3}", new object[,] { { 1.0, 2.0, 3.0 } }) };
            foreach (var (label, value) in assigns)
            {
                currentCase = "assign " + label + " to Range(\"B2:C3,F5:G6\").Value2";
                s.Range["A1:H8", M].Clear();
                var m = R("B2:C3,F5:G6");
                Try("pia", "assign", () => { m.Value2 = value; return "ok"; });
                Try("pia", "after", () =>
                {
                    var parts = new List<string>();
                    foreach (var c in targets) parts.Add(c + "=" + Managed(R(c).Value2));
                    return string.Join(" ", parts);
                });
            }
            book.Close(false, M, M);
        }
        catch (Exception e) { currentCase = "fatal"; Rec("-", "-", Exc(e) + " " + e.StackTrace); }
        Flush("e11");
    }

    // ---------------------------------------------------------------- E1
    private static long Unk(object o) { var p = Marshal.GetIUnknownForObject(o); try { return p.ToInt64(); } finally { Marshal.Release(p); } }
    private static void Seed(Excel.Worksheet s)
    {
        s.Range["A1:A10", M].Clear();
        s.Range["A1", M].Value2 = 10.0; s.Range["A2", M].Value2 = 20.0; s.Range["A3", M].Value2 = 30.0;
    }
    private static void Held(string route, Excel.Range h)
    {
        Try(route, "held.Address", () => A1(h));
        Try(route, "held.Address(External)", () => h.get_Address(M, M, Excel.XlReferenceStyle.xlA1, true, M));
        Try(route, "held.Value2", () => h.Value2);
        Try(route, "held.Row", () => h.Row);
        Try(route, "held.Worksheet.Name", () => h.Worksheet.Name);
        RecDisp("held.Address", h, "Address", GET);
        RecDisp("held.Value2", h, "Value2", GET);
    }

    [ExcelCommand(Name = "OM_E1")]
    public static void E1()
    {
        Begin("e1");
        var app = App;
        try
        {
            var book = app.Workbooks.Add(M);
            app.Calculation = Excel.XlCalculation.xlCalculationManual;
            var s = (Excel.Worksheet)book.Worksheets[1];
            s.Name = "S";
            Seed(s);
            s.Activate();

            currentCase = "Is: two acquisitions of Application.Range(\"A2\")";
            var a = app.Range["A2", M]; var b = app.Range["A2", M];
            Try("pia", "ReferenceEquals(a, b) (one RCW per IUnknown)", () => ReferenceEquals(a, b));
            Try("pia", "IUnknown(a) == IUnknown(b)", () => Unk(a) == Unk(b));
            var c = s.Range["A2", M]; var d = (Excel.Range)s.Cells[2, 1]; var e = a.Offset[0, 0];
            Try("pia", "IUnknown(a) == IUnknown(S.Range(A2))", () => Unk(a) == Unk(c));
            Try("pia", "IUnknown(a) == IUnknown(S.Cells(2,1))", () => Unk(a) == Unk(d));
            Try("pia", "IUnknown(a) == IUnknown(a.Offset(0,0))", () => Unk(a) == Unk(e));
            dynamic dapp = app;
            object da = dapp.Range("A2"), db = dapp.Range("A2");
            Try("dynamic", "IUnknown(a) == IUnknown(b)", () => Unk(da) == Unk(db));
            Try("pia", "IDispatch(a) == IDispatch(b)", () =>
            {
                IntPtr x = Marshal.GetIDispatchForObject(a), y = Marshal.GetIDispatchForObject(b);
                try { return x == y; } finally { Marshal.Release(x); Marshal.Release(y); }
            });
            Try("pia", "a.Address == b.Address", () => A1(a) == A1(b));


            currentCase = "held Range(\"A2\") across Rows(1).Insert";
            Seed(s);
            var h = s.Range["A2", M];
            Try("pia", "before Address", () => A1(h));
            Try("pia", "Rows(1).Insert", () => ((Excel.Range)s.Rows[1]).Insert(M, M));
            Held("pia", h);
            Try("pia", "fresh S.Range(A2).Value2", () => s.Range["A2", M].Value2);
            Try("pia", "fresh S.Range(A3).Value2", () => s.Range["A3", M].Value2);
            ((Excel.Range)s.Rows[1]).Delete(M);

            currentCase = "held Range(\"A2\") across Rows(2).Delete";
            Seed(s);
            h = s.Range["A2", M];
            var hRow = s.Range["A3", M];
            Try("pia", "Rows(2).Delete", () => ((Excel.Range)s.Rows[2]).Delete(M));
            Held("pia", h);
            RecV("held.Address vtable", h, "Address", CT.INVOKEKIND.INVOKE_PROPERTYGET);
            dynamic dh = h;
            Try("dynamic", "held.Address", () => dh.Address);
            Try("pia", "held A3 (below the deleted row) Address", () => A1(hRow));
            Try("pia", "held A3 (below the deleted row) Value2", () => hRow.Value2);

            currentCase = "held Range(\"A2:A3\") across Rows(2).Delete (partial)";
            Seed(s);
            var hb = s.Range["A2:A3", M];
            Try("pia", "Rows(2).Delete", () => ((Excel.Range)s.Rows[2]).Delete(M));
            Try("pia", "held.Address", () => A1(hb));
            Try("pia", "held.Value2", () => hb.Value2);

            currentCase = "held Range(\"A2\") across Columns(1).Insert";
            Seed(s);
            h = s.Range["A2", M];
            Try("pia", "Columns(1).Insert", () => ((Excel.Range)s.Columns[1]).Insert(M, M));
            Try("pia", "held.Address", () => A1(h));
            Try("pia", "held.Value2", () => h.Value2);
            ((Excel.Range)s.Columns[1]).Delete(M);

            currentCase = "held Range(\"A2\") across sheet rename S -> S2";
            Seed(s);
            h = s.Range["A2", M];
            Try("pia", "rename", () => { s.Name = "S2"; return s.Name; });
            Held("pia", h);
            s.Name = "S";

            currentCase = "held Range(\"A2\") across sheet delete";
            var v = (Excel.Worksheet)book.Worksheets.Add(M, s, M, M);
            v.Name = "V";
            v.Range["A1", M].Value2 = 10.0; v.Range["A2", M].Value2 = 20.0;
            h = v.Range["A2", M];
            bool alerts = app.DisplayAlerts;
            Try("pia", "DisplayAlerts before", () => alerts);
            Try("pia", "V.Delete", () => { app.DisplayAlerts = false; try { v.Delete(); return "deleted"; } finally { app.DisplayAlerts = alerts; } });
            Held("pia", h);
            RecV("held.Address vtable", h, "Address", CT.INVOKEKIND.INVOKE_PROPERTYGET);
            Try("pia", "DisplayAlerts after", () => app.DisplayAlerts);

            book.Close(false, M, M);
        }
        catch (Exception e) { currentCase = "fatal"; Rec("-", "-", Exc(e) + " " + e.StackTrace); }
        Flush("e1");
    }
}
