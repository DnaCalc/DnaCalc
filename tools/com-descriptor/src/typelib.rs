//! Loading a type library and turning the listed types into the descriptor.

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VS_FIXEDFILEINFO, VerQueryValueW,
};
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, CoInitializeEx, ITypeInfo, ITypeLib, TYPEATTR, TYPEDESC,
};
use windows::Win32::System::Ole::{
    LoadRegTypeLib, LoadTypeLibEx, QueryPathOfRegTypeLib, REGKIND_NONE, VarBstrFromDec,
};
use windows::Win32::System::Variant::VARIANT;
use windows::core::{BSTR, GUID, HSTRING};

/// Pointer size of the x64 vtables this tool reads (oVft and cbSizeVft are byte offsets).
const PTR: u32 = 8;

#[derive(Serialize)]
struct Descriptor {
    library: Library,
    types: Vec<TypeEntry>,
}

#[derive(Serialize)]
struct Library {
    name: String,
    guid: String,
    version: String,
    lcid: u32,
    source_file: String,
    file_version: Option<String>,
}

#[derive(Serialize)]
struct TypeEntry {
    name: String,
    kind: &'static str,
    guid: String,
    dual: bool,
    flags: Vec<&'static str>,
    base: Option<String>,
    vtable_slots: Option<u32>,
    dispatch_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    coclass_interfaces: Option<Vec<CoclassInterface>>,
    members: Vec<Member>,
}

#[derive(Serialize)]
struct CoclassInterface {
    name: String,
    default: bool,
    source: bool,
}

#[derive(Serialize)]
struct Member {
    name: String,
    dispid: i32,
    invoke: &'static str,
    slot: Option<u32>,
    hidden: bool,
    restricted: bool,
    params: Vec<Param>,
    #[serde(rename = "return")]
    ret: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    optional_count: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vararg: Option<bool>,
}

#[derive(Serialize)]
struct Param {
    name: Option<String>,
    #[serde(rename = "type")]
    ty: String,
    flags: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default: Option<Value>,
}

enum Source {
    Registered { guid: GUID, major: u16, minor: u16 },
    File(String),
}

struct Args {
    source: Source,
    interfaces: Option<String>,
    out: Option<String>,
    list: bool,
}

const USAGE: &str = "usage: com-descriptor (--library <LIBID>,<major.minor> | --file <path>) (--interfaces <list file> --out <json> | --list)";

pub fn run(argv: Vec<String>) -> i32 {
    let args = match parse_args(&argv) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("com-descriptor: {e}\n{USAGE}");
            return 64;
        }
    };
    match execute(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("com-descriptor: {e}");
            1
        }
    }
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut source = None;
    let mut interfaces = None;
    let mut out = None;
    let mut list = false;
    let mut it = argv.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "--library" => {
                let v = value()?;
                let (g, ver) = v
                    .split_once(',')
                    .ok_or("--library takes <LIBID>,<major.minor>")?;
                let (ma, mi) = ver
                    .split_once('.')
                    .ok_or("--library version is <major.minor>")?;
                let major = ma
                    .trim()
                    .parse()
                    .map_err(|_| format!("bad major version {ma}"))?;
                let minor = parse_minor(mi.trim())?;
                source = Some(Source::Registered {
                    guid: parse_guid(g)?,
                    major,
                    minor,
                });
            }
            "--file" => source = Some(Source::File(value()?)),
            "--interfaces" => interfaces = Some(value()?),
            "--out" => out = Some(value()?),
            "--list" => list = true,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let source = source.ok_or("one of --library or --file is required")?;
    if !list && (interfaces.is_none() || out.is_none()) {
        return Err("--interfaces and --out are required (or --list)".into());
    }
    Ok(Args {
        source,
        interfaces,
        out,
        list,
    })
}

/// Registry version keys are hexadecimal (`2.8`, `1.9`, `1.a`); accept that form.
fn parse_minor(s: &str) -> Result<u16, String> {
    u16::from_str_radix(s, 16).map_err(|_| format!("bad minor version {s}"))
}

fn parse_guid(s: &str) -> Result<GUID, String> {
    let hex: String = s
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .chars()
        .filter(|c| *c != '-')
        .collect();
    if hex.len() != 32 {
        return Err(format!("bad GUID {s}"));
    }
    u128::from_str_radix(&hex, 16)
        .map(GUID::from_u128)
        .map_err(|_| format!("bad GUID {s}"))
}

fn guid_string(g: &GUID) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        g.data1,
        g.data2,
        g.data3,
        g.data4[0],
        g.data4[1],
        g.data4[2],
        g.data4[3],
        g.data4[4],
        g.data4[5],
        g.data4[6],
        g.data4[7]
    )
}

fn execute(args: &Args) -> Result<i32, String> {
    // SAFETY: plain COM initialisation of this thread; the type library calls below need it.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let (lib, source_file) = load(&args.source)?;
    let lib_attr = unsafe {
        let p = lib.GetLibAttr().map_err(|e| format!("GetLibAttr: {e}"))?;
        let a = *p;
        lib.ReleaseTLibAttr(p);
        a
    };
    let lib_name = lib_doc_name(&lib, -1)?;
    let count = unsafe { lib.GetTypeInfoCount() };
    let mut index = HashMap::new();
    for i in 0..count {
        let name = lib_doc_name(&lib, i as i32)?;
        if args.list {
            let ti = unsafe { lib.GetTypeInfo(i) }.map_err(|e| format!("GetTypeInfo({i}): {e}"))?;
            let a = type_attr(&ti)?;
            let dual = if a.wTypeFlags & 0x40 != 0 {
                " dual"
            } else {
                ""
            };
            println!("{}{}\t{}", kind_name(a.typekind.0), dual, name);
        }
        index.insert(name, i);
    }
    if args.list {
        return Ok(0);
    }
    let list_path = args.interfaces.as_deref().unwrap_or_default();
    let list =
        std::fs::read_to_string(list_path).map_err(|e| format!("reading {list_path}: {e}"))?;
    let names: Vec<&str> = list
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(|l| l.split_whitespace().next().unwrap_or(l))
        .collect();

    let ctx = Ctx {
        lib_guid: lib_attr.guid,
    };
    let mut types = Vec::new();
    let mut missing = Vec::new();
    for name in names {
        let Some(&i) = index.get(name) else {
            missing.push(name);
            continue;
        };
        let ti = unsafe { lib.GetTypeInfo(i) }.map_err(|e| format!("GetTypeInfo({name}): {e}"))?;
        types.push(ctx.entry(name, &ti)?);
    }

    let file_version = file_version(&source_file);
    let descriptor = Descriptor {
        library: Library {
            name: lib_name,
            guid: guid_string(&lib_attr.guid),
            version: format!("{}.{}", lib_attr.wMajorVerNum, lib_attr.wMinorVerNum),
            lcid: lib_attr.lcid,
            source_file,
            file_version,
        },
        types,
    };
    let out_path = args.out.as_deref().unwrap_or_default();
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(
        &mut buf,
        serde_json::ser::PrettyFormatter::with_indent(b" "),
    );
    descriptor
        .serialize(&mut ser)
        .map_err(|e| format!("serialising: {e}"))?;
    buf.push(b'\n');
    std::fs::write(out_path, &buf).map_err(|e| format!("writing {out_path}: {e}"))?;
    for t in &descriptor.types {
        eprintln!(
            "{}\t{}\tslots={:?}\tmembers={}",
            t.name,
            t.kind,
            t.vtable_slots,
            t.members.len()
        );
    }
    if missing.is_empty() {
        Ok(0)
    } else {
        for m in &missing {
            eprintln!(
                "com-descriptor: {m} is not defined in {}",
                descriptor.library.name
            );
        }
        Ok(1)
    }
}

fn load(source: &Source) -> Result<(ITypeLib, String), String> {
    match source {
        Source::Registered { guid, major, minor } => unsafe {
            let lib = LoadRegTypeLib(guid, *major, *minor, 0)
                .map_err(|e| format!("LoadRegTypeLib {}: {e}", guid_string(guid)))?;
            let path = QueryPathOfRegTypeLib(guid, *major, *minor, 0)
                .map(|b| b.to_string())
                .unwrap_or_default();
            Ok((lib, path.trim_end_matches('\0').to_string()))
        },
        Source::File(path) => unsafe {
            let lib = LoadTypeLibEx(&HSTRING::from(path.as_str()), REGKIND_NONE)
                .map_err(|e| format!("LoadTypeLibEx {path}: {e}"))?;
            Ok((lib, path.clone()))
        },
    }
}

/// The version resource of the library's file; a registered path may end in `\<resource index>`.
fn file_version(path: &str) -> Option<String> {
    let mut candidate = path.to_string();
    if !std::path::Path::new(&candidate).is_file() {
        let (head, tail) = path.rsplit_once('\\')?;
        if !tail.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        candidate = head.to_string();
    }
    let wide = HSTRING::from(candidate.as_str());
    unsafe {
        let size = GetFileVersionInfoSizeW(&wide, None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(&wide, None, size, data.as_mut_ptr().cast()).ok()?;
        let mut p = std::ptr::null_mut();
        let mut len = 0u32;
        if !VerQueryValueW(data.as_ptr().cast(), &HSTRING::from("\\"), &mut p, &mut len).as_bool()
            || p.is_null()
        {
            return None;
        }
        let f = &*(p as *const VS_FIXEDFILEINFO);
        Some(format!(
            "{}.{}.{}.{}",
            f.dwFileVersionMS >> 16,
            f.dwFileVersionMS & 0xFFFF,
            f.dwFileVersionLS >> 16,
            f.dwFileVersionLS & 0xFFFF
        ))
    }
}

fn lib_doc_name(lib: &ITypeLib, index: i32) -> Result<String, String> {
    let mut name = BSTR::default();
    let mut ctx = 0u32;
    unsafe { lib.GetDocumentation(index, Some(&mut name), None, &mut ctx, None) }
        .map_err(|e| format!("ITypeLib::GetDocumentation({index}): {e}"))?;
    Ok(name.to_string())
}

fn ti_name(ti: &ITypeInfo) -> Result<String, String> {
    let mut name = BSTR::default();
    let mut ctx = 0u32;
    unsafe { ti.GetDocumentation(-1, Some(&mut name), None, &mut ctx, None) }
        .map_err(|e| format!("ITypeInfo::GetDocumentation: {e}"))?;
    Ok(name.to_string())
}

fn type_attr(ti: &ITypeInfo) -> Result<TYPEATTR, String> {
    unsafe {
        let p = ti.GetTypeAttr().map_err(|e| format!("GetTypeAttr: {e}"))?;
        let a = *p;
        ti.ReleaseTypeAttr(p);
        Ok(a)
    }
}

fn kind_name(k: i32) -> &'static str {
    match k {
        0 => "enum",
        1 => "record",
        2 => "module",
        3 => "interface",
        4 => "dispatch",
        5 => "coclass",
        6 => "alias",
        7 => "union",
        _ => "unknown",
    }
}

fn type_flags(f: u16) -> Vec<&'static str> {
    const NAMES: [(u16, &str); 15] = [
        (0x1, "appobject"),
        (0x2, "cancreate"),
        (0x4, "licensed"),
        (0x8, "predeclid"),
        (0x10, "hidden"),
        (0x20, "control"),
        (0x40, "dual"),
        (0x80, "nonextensible"),
        (0x100, "oleautomation"),
        (0x200, "restricted"),
        (0x400, "aggregatable"),
        (0x800, "replaceable"),
        (0x1000, "dispatchable"),
        (0x2000, "reversebind"),
        (0x4000, "proxy"),
    ];
    NAMES
        .iter()
        .filter(|(bit, _)| f & bit != 0)
        .map(|(_, n)| *n)
        .collect()
}

fn param_flags(f: u16) -> Vec<&'static str> {
    const NAMES: [(u16, &str); 6] = [
        (0x1, "in"),
        (0x2, "out"),
        (0x4, "lcid"),
        (0x8, "retval"),
        (0x10, "optional"),
        (0x20, "hasdefault"),
    ];
    NAMES
        .iter()
        .filter(|(bit, _)| f & bit != 0)
        .map(|(_, n)| *n)
        .collect()
}

fn invoke_name(k: i32) -> &'static str {
    match k {
        1 => "method",
        2 => "get",
        4 => "put",
        8 => "putref",
        _ => "unknown",
    }
}

/// The inherited IUnknown and IDispatch methods a dispinterface may repeat; counted by
/// `vtable_slots`, never listed.
const BASE_METHODS: [&str; 7] = [
    "QueryInterface",
    "AddRef",
    "Release",
    "GetTypeInfoCount",
    "GetTypeInfo",
    "GetIDsOfNames",
    "Invoke",
];

struct Ctx {
    lib_guid: GUID,
}

impl Ctx {
    fn entry(&self, name: &str, ti: &ITypeInfo) -> Result<TypeEntry, String> {
        let attr = type_attr(ti)?;
        let kind = attr.typekind.0;
        let dual = attr.wTypeFlags & 0x40 != 0;
        let mut e = TypeEntry {
            name: name.to_string(),
            kind: kind_name(kind),
            guid: guid_string(&attr.guid),
            dual,
            flags: type_flags(attr.wTypeFlags),
            base: None,
            vtable_slots: None,
            dispatch_only: false,
            coclass_interfaces: None,
            members: Vec::new(),
        };
        match kind {
            5 => {
                let mut list = Vec::new();
                for i in 0..attr.cImplTypes as u32 {
                    let rti = unsafe {
                        ti.GetRefTypeInfo(
                            ti.GetRefTypeOfImplType(i)
                                .map_err(|e| format!("{name}: impl {i}: {e}"))?,
                        )
                    }
                    .map_err(|e| format!("{name}: impl {i}: {e}"))?;
                    let flags = unsafe { ti.GetImplTypeFlags(i) }
                        .map_err(|e| format!("{name}: impl flags {i}: {e}"))?
                        .0;
                    list.push(CoclassInterface {
                        name: self.qualified_name(&rti)?,
                        default: flags & 0x1 != 0,
                        source: flags & 0x2 != 0,
                    });
                }
                e.coclass_interfaces = Some(list);
            }
            4 if dual => {
                // The dual interface's vtable view: real parameters in order, lcid and retval included.
                let iti = unsafe {
                    ti.GetRefTypeInfo(
                        ti.GetRefTypeOfImplType(u32::MAX)
                            .map_err(|e| format!("{name}: vtable view: {e}"))?,
                    )
                }
                .map_err(|e| format!("{name}: vtable view: {e}"))?;
                let iattr = type_attr(&iti)?;
                e.base = self.base(&iti, &iattr)?;
                e.vtable_slots = Some(iattr.cbSizeVft as u32 / PTR);
                e.members = self.members(name, &iti, &iattr, false)?;
            }
            4 => {
                e.dispatch_only = true;
                e.base = self.base(ti, &attr)?;
                e.vtable_slots = Some(attr.cbSizeVft as u32 / PTR);
                e.members = self.members(name, ti, &attr, true)?;
            }
            3 => {
                e.base = self.base(ti, &attr)?;
                e.vtable_slots = Some(attr.cbSizeVft as u32 / PTR);
                e.members = self.members(name, ti, &attr, false)?;
            }
            other => {
                return Err(format!(
                    "{name} is a {} (type kind {other}), not an interface, dispinterface or coclass",
                    kind_name(other)
                ));
            }
        }
        Ok(e)
    }

    fn base(&self, ti: &ITypeInfo, attr: &TYPEATTR) -> Result<Option<String>, String> {
        if attr.cImplTypes == 0 {
            return Ok(None);
        }
        let rti = unsafe {
            ti.GetRefTypeInfo(
                ti.GetRefTypeOfImplType(0)
                    .map_err(|e| format!("base: {e}"))?,
            )
        }
        .map_err(|e| format!("base: {e}"))?;
        Ok(Some(self.qualified_name(&rti)?))
    }

    /// A type's name, prefixed `<Library>.` when it lives in another library than the one read
    /// (Office types in Excel, stdole types anywhere), so a generator can find its definition.
    fn qualified_name(&self, ti: &ITypeInfo) -> Result<String, String> {
        let name = ti_name(ti)?;
        // IUnknown and IDispatch keep their plain names, as in the `IDispatch*` type strings.
        if name == "IUnknown" || name == "IDispatch" {
            return Ok(name);
        }
        let mut lib = None;
        let mut idx = 0u32;
        if unsafe { ti.GetContainingTypeLib(&mut lib, &mut idx) }.is_ok()
            && let Some(lib) = lib
        {
            let guid = unsafe {
                let p = lib.GetLibAttr().map_err(|e| format!("GetLibAttr: {e}"))?;
                let g = (*p).guid;
                lib.ReleaseTLibAttr(p);
                g
            };
            if guid != self.lib_guid {
                return Ok(format!("{}.{name}", lib_doc_name(&lib, -1)?));
            }
        }
        Ok(name)
    }

    fn members(
        &self,
        tname: &str,
        ti: &ITypeInfo,
        attr: &TYPEATTR,
        dispatch_view: bool,
    ) -> Result<Vec<Member>, String> {
        let mut out = Vec::new();
        // GetNames returns the names of the first function carrying a memid; a put sharing the
        // get's memid gets the get's names. Track the first function per memid to flag a count
        // difference, where those names would not fit.
        let mut first_params: HashMap<i32, i16> = HashMap::new();
        let mut unnamed: Vec<String> = Vec::new();
        for i in 0..attr.cFuncs as u32 {
            let p = unsafe { ti.GetFuncDesc(i) }
                .map_err(|e| format!("{tname}: GetFuncDesc({i}): {e}"))?;
            let fd = unsafe { *p };
            let result = (|| -> Result<Option<Member>, String> {
                let n = fd.cParams.max(0) as usize;
                let mut names = vec![BSTR::default(); n + 1];
                let mut got = 0u32;
                unsafe { ti.GetNames(fd.memid, &mut names, &mut got) }
                    .map_err(|e| format!("{tname}: GetNames({}): {e}", fd.memid))?;
                let mname = names[0].to_string();
                let restricted = fd.wFuncFlags.0 as u32 & 0x1 != 0;
                if dispatch_view && restricted && BASE_METHODS.contains(&mname.as_str()) {
                    return Ok(None);
                }
                let first = *first_params.entry(fd.memid).or_insert(fd.cParams);
                let names_fit = first == fd.cParams;
                if !names_fit {
                    unnamed.push(mname.clone());
                }
                let mut params = Vec::with_capacity(n);
                for j in 0..n {
                    let ed = unsafe { &*fd.lprgelemdescParam.add(j) };
                    let pd = unsafe { ed.Anonymous.paramdesc };
                    let pf = pd.wParamFlags.0;
                    let default = if pf & 0x20 != 0 && !pd.pparamdescex.is_null() {
                        Some(unsafe { variant_json(&(*pd.pparamdescex).varDefaultValue) })
                    } else {
                        None
                    };
                    let name = if names_fit && j + 1 < got as usize {
                        Some(names[j + 1].to_string())
                    } else {
                        None
                    };
                    params.push(Param {
                        name,
                        ty: self.type_string(ti, &ed.tdesc)?,
                        flags: param_flags(pf),
                        default,
                    });
                }
                Ok(Some(Member {
                    name: mname,
                    dispid: fd.memid,
                    invoke: invoke_name(fd.invkind.0),
                    slot: if dispatch_view {
                        None
                    } else {
                        Some(fd.oVft as u32 / PTR)
                    },
                    hidden: fd.wFuncFlags.0 as u32 & 0x40 != 0,
                    restricted,
                    params,
                    ret: self.type_string(ti, &fd.elemdescFunc.tdesc)?,
                    optional_count: if fd.cParamsOpt > 0 {
                        Some(fd.cParamsOpt)
                    } else {
                        None
                    },
                    vararg: if fd.cParamsOpt == -1 {
                        Some(true)
                    } else {
                        None
                    },
                }))
            })();
            unsafe { ti.ReleaseFuncDesc(p) };
            if let Some(m) = result? {
                out.push(m);
            }
        }
        for i in 0..attr.cVars as u32 {
            let p = unsafe { ti.GetVarDesc(i) }
                .map_err(|e| format!("{tname}: GetVarDesc({i}): {e}"))?;
            let vd = unsafe { *p };
            let result = (|| -> Result<Member, String> {
                let mut names = vec![BSTR::default(); 1];
                let mut got = 0u32;
                unsafe { ti.GetNames(vd.memid, &mut names, &mut got) }
                    .map_err(|e| format!("{tname}: GetNames({}): {e}", vd.memid))?;
                Ok(Member {
                    name: names[0].to_string(),
                    dispid: vd.memid,
                    invoke: "property",
                    slot: None,
                    hidden: vd.wVarFlags.0 as u32 & 0x40 != 0,
                    restricted: vd.wVarFlags.0 as u32 & 0x80 != 0,
                    params: Vec::new(),
                    ret: self.type_string(ti, &vd.elemdescVar.tdesc)?,
                    optional_count: None,
                    vararg: None,
                })
            })();
            unsafe { ti.ReleaseVarDesc(p) };
            out.push(result?);
        }
        if !unnamed.is_empty() {
            eprintln!(
                "com-descriptor: {tname}: {} members share a memid with an earlier function of another parameter count, so GetNames cannot name their parameters (left null): {}",
                unnamed.len(),
                unnamed.join(", ")
            );
        }
        Ok(out)
    }

    fn type_string(&self, ti: &ITypeInfo, td: &TYPEDESC) -> Result<String, String> {
        let s = match td.vt.0 {
            0 => "EMPTY",
            1 => "NULL",
            2 => "I2",
            3 => "I4",
            4 => "R4",
            5 => "R8",
            6 => "CY",
            7 => "DATE",
            8 => "BSTR",
            9 => "IDispatch*",
            10 => "ERROR",
            11 => "BOOL",
            12 => "VARIANT",
            13 => "IUnknown*",
            14 => "DECIMAL",
            16 => "I1",
            17 => "UI1",
            18 => "UI2",
            19 => "UI4",
            20 => "I8",
            21 => "UI8",
            22 => "INT",
            23 => "UINT",
            24 => "VOID",
            25 => "HRESULT",
            26 => {
                return Ok(format!(
                    "{}*",
                    self.type_string(ti, unsafe { &*td.Anonymous.lptdesc })?
                ));
            }
            27 => {
                return Ok(format!(
                    "SAFEARRAY({})",
                    self.type_string(ti, unsafe { &*td.Anonymous.lptdesc })?
                ));
            }
            28 => {
                let ad = unsafe { &*td.Anonymous.lpadesc };
                let dims =
                    unsafe { std::slice::from_raw_parts(ad.rgbounds.as_ptr(), ad.cDims as usize) };
                let dims: Vec<String> = dims.iter().map(|b| b.cElements.to_string()).collect();
                return Ok(format!(
                    "CARRAY({})[{}]",
                    self.type_string(ti, &ad.tdescElem)?,
                    dims.join(",")
                ));
            }
            29 => {
                let rti = unsafe { ti.GetRefTypeInfo(td.Anonymous.hreftype) }
                    .map_err(|e| format!("GetRefTypeInfo: {e}"))?;
                let a = type_attr(&rti)?;
                let name = self.qualified_name(&rti)?;
                return Ok(match a.typekind.0 {
                    0 => format!("enum:{name}[I4]"),
                    6 => format!("alias:{name}[{}]", self.type_string(&rti, &a.tdescAlias)?),
                    1 | 7 => format!("record:{name}"),
                    _ => name,
                });
            }
            30 => "LPSTR",
            31 => "LPWSTR",
            other => return Ok(format!("VT_{other}")),
        };
        Ok(s.to_string())
    }
}

/// A parameter default as a JSON literal.
unsafe fn variant_json(v: &VARIANT) -> Value {
    unsafe {
        let inner = &v.Anonymous.Anonymous;
        let u = &inner.Anonymous;
        match inner.vt.0 {
            0 | 1 => Value::Null,
            2 => u.iVal.into(),
            3 => u.lVal.into(),
            4 => (u.fltVal as f64).into(),
            5 => u.dblVal.into(),
            6 => (u.cyVal.int64 as f64 / 10000.0).into(),
            7 => u.date.into(),
            8 => Value::String(u.bstrVal.to_string()),
            10 => u.scode.into(),
            11 => Value::Bool(u.boolVal.0 != 0),
            14 => VarBstrFromDec(&v.Anonymous.decVal, 0, 0)
                .map(|b| Value::String(b.to_string()))
                .unwrap_or(Value::Null),
            16 => u.cVal.into(),
            17 => u.bVal.into(),
            18 => u.uiVal.into(),
            19 => u.ulVal.into(),
            20 => u.llVal.into(),
            21 => u.ullVal.into(),
            22 => u.intVal.into(),
            23 => u.uintVal.into(),
            other => {
                eprintln!("com-descriptor: default of variant type {other} written as null");
                Value::Null
            }
        }
    }
}
