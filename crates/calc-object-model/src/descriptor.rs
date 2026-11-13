//! The interface descriptor: one source of interface facts for every projection.
//!
//! `descriptor/*.json` is written by `tools/com-descriptor` from the installed type libraries
//! (Excel 16.0, Office 2.8, Add-in Designer 1.0; the source file version is recorded in each
//! file). Names and ABI facts only: no documentation strings. To advertise another interface,
//! add its name to `descriptor/<library>.interfaces.txt` and regenerate; never hand-edit the JSON.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

/// One library file as written by `tools/com-descriptor`.
#[derive(Clone, Debug, Deserialize)]
pub struct LibraryFile {
    pub library: LibraryInfo,
    pub types: Vec<TypeInfo>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LibraryInfo {
    pub name: String,
    pub guid: String,
    pub version: String,
    #[serde(default)]
    pub lcid: u32,
    #[serde(default)]
    pub source_file: String,
    #[serde(default)]
    pub file_version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TypeKind {
    Interface,
    Dispatch,
    Coclass,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CoclassInterface {
    pub name: String,
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub source: bool,
}

/// An interface, dispinterface or coclass.
#[derive(Clone, Debug, Deserialize)]
pub struct TypeInfo {
    pub name: String,
    pub kind: TypeKind,
    pub guid: String,
    #[serde(default)]
    pub dual: bool,
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub base: Option<String>,
    /// Total vtable slots including IUnknown (3) and IDispatch (4). A dispinterface has 7.
    #[serde(default)]
    pub vtable_slots: Option<u16>,
    #[serde(default)]
    pub dispatch_only: bool,
    #[serde(default)]
    pub coclass_interfaces: Vec<CoclassInterface>,
    #[serde(default)]
    pub members: Vec<Member>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InvokeKind {
    Get,
    Put,
    Putref,
    Method,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Member {
    pub name: String,
    pub dispid: i32,
    pub invoke: InvokeKind,
    /// vtable slot index (x64 `oVft / 8`); `None` for a dispinterface member.
    #[serde(default)]
    pub slot: Option<u16>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub restricted: bool,
    /// Every vtable parameter in order, including `[lcid]` and `[out, retval]`.
    #[serde(default)]
    pub params: Vec<Param>,
    /// The vtable function's return type (`HRESULT` for dual members).
    #[serde(rename = "return", default)]
    pub ret: String,
    #[serde(default)]
    pub optional_count: u16,
    #[serde(default)]
    pub vararg: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamFlag {
    In,
    Out,
    Lcid,
    Retval,
    Optional,
    Hasdefault,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Param {
    /// `None` where the type library leaves a parameter unnamed (some dispinterface puts).
    #[serde(default)]
    pub name: Option<String>,
    /// Type string as `tools/com-descriptor` writes it (`VARIANT`, `BSTR*`, `Range*`,
    /// `enum:XlCalculation[I4]`, `SAFEARRAY(VARIANT)`, ...). See [`TypeRef::parse`].
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default)]
    pub flags: Vec<ParamFlag>,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
}

impl Param {
    pub fn has(&self, flag: ParamFlag) -> bool {
        self.flags.contains(&flag)
    }
    /// A parameter the caller supplies (not `[lcid]`, not `[out, retval]`).
    pub fn is_argument(&self) -> bool {
        !self.has(ParamFlag::Lcid) && !self.has(ParamFlag::Retval)
    }
    pub fn type_ref(&self) -> TypeRef {
        TypeRef::parse(&self.ty)
    }
}

impl Member {
    /// The caller-supplied parameters, in declared order (what [`crate::Call::args`] holds).
    pub fn arguments(&self) -> impl Iterator<Item = &Param> {
        self.params.iter().filter(|param| param.is_argument())
    }
    pub fn retval(&self) -> Option<&Param> {
        self.params.iter().find(|param| param.has(ParamFlag::Retval))
    }
    pub fn takes_lcid(&self) -> bool {
        self.params.iter().any(|param| param.has(ParamFlag::Lcid))
    }
}

/// A parsed parameter or result type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeRef {
    /// `VARIANT`, `BSTR`, `I4`, `BOOL`, `R8`, `DATE`, `CY`, `HRESULT`, `VOID`, ...
    Base(String),
    /// A pointer level (`VARIANT*`, `BSTR*`, `Range*` is `Pointer(Named("Range"))`).
    Pointer(Box<TypeRef>),
    /// An interface or coclass of a described library, or `IDispatch` / `IUnknown`.
    Named(String),
    /// `enum:XlCalculation[I4]`: the enum's name and its underlying base type.
    Enum { name: String, base: String },
    /// `alias:Name[BASE]`.
    Alias { name: String, base: Box<TypeRef> },
    /// `SAFEARRAY(element)`.
    SafeArray(Box<TypeRef>),
    /// `record:Name`.
    Record(String),
}

const BASE_TYPES: &[&str] = &[
    "VARIANT", "BSTR", "I1", "I2", "I4", "I8", "UI1", "UI2", "UI4", "UI8", "INT", "UINT", "BOOL", "R4",
    "R8", "DATE", "CY", "DECIMAL", "ERROR", "HRESULT", "VOID",
];

impl TypeRef {
    pub fn parse(text: &str) -> TypeRef {
        let text = text.trim();
        if let Some(inner) = text.strip_suffix('*') {
            return TypeRef::Pointer(Box::new(TypeRef::parse(inner)));
        }
        if let Some(inner) = text.strip_prefix("SAFEARRAY(").and_then(|rest| rest.strip_suffix(')')) {
            return TypeRef::SafeArray(Box::new(TypeRef::parse(inner)));
        }
        if let Some(rest) = text.strip_prefix("enum:") {
            let (name, base) = split_bracket(rest);
            return TypeRef::Enum { name, base: base.unwrap_or_else(|| "I4".into()) };
        }
        if let Some(rest) = text.strip_prefix("alias:") {
            let (name, base) = split_bracket(rest);
            return TypeRef::Alias {
                name,
                base: Box::new(TypeRef::parse(&base.unwrap_or_else(|| "VARIANT".into()))),
            };
        }
        if let Some(rest) = text.strip_prefix("record:") {
            return TypeRef::Record(rest.to_string());
        }
        if BASE_TYPES.contains(&text) {
            TypeRef::Base(text.to_string())
        } else {
            TypeRef::Named(text.to_string())
        }
    }
    /// The type with enums and aliases replaced by their underlying type.
    pub fn underlying(&self) -> TypeRef {
        match self {
            TypeRef::Enum { base, .. } => TypeRef::Base(base.clone()),
            TypeRef::Alias { base, .. } => base.underlying(),
            TypeRef::Pointer(inner) => TypeRef::Pointer(Box::new(inner.underlying())),
            other => other.clone(),
        }
    }
}

fn split_bracket(text: &str) -> (String, Option<String>) {
    match text.split_once('[') {
        Some((name, rest)) => (name.to_string(), Some(rest.trim_end_matches(']').to_string())),
        None => (text.to_string(), None),
    }
}

/// A COM GUID, parsed from the descriptor's `{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}` text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Guid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

impl Guid {
    pub fn parse(text: &str) -> Option<Guid> {
        let hex: String = text.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        if hex.len() != 32 {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        let mut data4 = [0u8; 8];
        for (k, slot) in data4.iter_mut().enumerate() {
            *slot = byte(16 + 2 * k)?;
        }
        Some(Guid {
            data1: u32::from_str_radix(&hex[0..8], 16).ok()?,
            data2: u16::from_str_radix(&hex[8..12], 16).ok()?,
            data3: u16::from_str_radix(&hex[12..16], 16).ok()?,
            data4,
        })
    }
}

/// An index into [`Descriptor::types`]; stable for the life of the process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InterfaceId(pub u32);

/// All described libraries, indexed by type name (case-insensitive) and GUID.
pub struct Descriptor {
    pub libraries: Vec<LibraryInfo>,
    types: Vec<TypeInfo>,
    library_of: Vec<usize>,
    by_name: HashMap<String, InterfaceId>,
    by_guid: HashMap<Guid, InterfaceId>,
}

const EXCEL: &str = include_str!("../descriptor/excel.json");
const OFFICE: &str = include_str!("../descriptor/office.json");
const ADDIN_DESIGNER: &str = include_str!("../descriptor/addin-designer.json");
const ACCESSIBILITY: &str = include_str!("../descriptor/accessibility.json");

impl Descriptor {
    /// The committed descriptor (Excel, Office, Add-in Designer, Accessibility), parsed once.
    pub fn get() -> &'static Descriptor {
        static DESCRIPTOR: OnceLock<Descriptor> = OnceLock::new();
        DESCRIPTOR.get_or_init(|| {
            Descriptor::from_files(&[EXCEL, OFFICE, ADDIN_DESIGNER, ACCESSIBILITY])
                .expect("committed object-model descriptor is valid JSON")
        })
    }

    pub fn from_files(files: &[&str]) -> Result<Descriptor, String> {
        let mut descriptor = Descriptor {
            libraries: Vec::new(),
            types: Vec::new(),
            library_of: Vec::new(),
            by_name: HashMap::new(),
            by_guid: HashMap::new(),
        };
        for text in files {
            let file: LibraryFile = serde_json::from_str(text).map_err(|why| why.to_string())?;
            let library = descriptor.libraries.len();
            descriptor.libraries.push(file.library);
            for ty in file.types {
                let id = InterfaceId(descriptor.types.len() as u32);
                if let Some(guid) = Guid::parse(&ty.guid) {
                    descriptor.by_guid.entry(guid).or_insert(id);
                }
                descriptor.by_name.entry(ty.name.to_ascii_lowercase()).or_insert(id);
                descriptor.types.push(ty);
                descriptor.library_of.push(library);
            }
        }
        Ok(descriptor)
    }

    /// A type by name. A library-qualified name (`Office.CommandBars`) is looked up by its
    /// type name: the described libraries do not repeat type names.
    pub fn id(&self, name: &str) -> Option<InterfaceId> {
        let name = name.rsplit('.').next().unwrap_or(name);
        self.by_name.get(&name.to_ascii_lowercase()).copied()
    }
    pub fn by_guid(&self, guid: &Guid) -> Option<InterfaceId> {
        self.by_guid.get(guid).copied()
    }
    pub fn ty(&self, id: InterfaceId) -> &TypeInfo {
        &self.types[id.0 as usize]
    }
    pub fn types(&self) -> impl Iterator<Item = (InterfaceId, &TypeInfo)> {
        self.types.iter().enumerate().map(|(i, ty)| (InterfaceId(i as u32), ty))
    }
    pub fn library(&self, id: InterfaceId) -> &LibraryInfo {
        &self.libraries[self.library_of[id.0 as usize]]
    }
    /// The default interface of a coclass, or the type itself.
    pub fn default_interface(&self, id: InterfaceId) -> InterfaceId {
        let ty = self.ty(id);
        if ty.kind == TypeKind::Coclass {
            if let Some(default) = ty.coclass_interfaces.iter().find(|i| i.default && !i.source) {
                if let Some(found) = self.id(&default.name) {
                    return found;
                }
            }
        }
        id
    }
    /// Members of `id`, including those of its non-IDispatch base interfaces (base first).
    pub fn members(&self, id: InterfaceId) -> Vec<&Member> {
        let mut chain = Vec::new();
        let mut current = Some(id);
        while let Some(at) = current {
            chain.push(at);
            current = self
                .ty(at)
                .base
                .as_deref()
                .filter(|base| !base.eq_ignore_ascii_case("IDispatch") && !base.eq_ignore_ascii_case("IUnknown"))
                .and_then(|base| self.id(base));
        }
        chain.iter().rev().flat_map(|at| self.ty(*at).members.iter()).collect()
    }
    pub fn member(&self, id: InterfaceId, name: &str, kind: InvokeKind) -> Option<&Member> {
        self.members(id)
            .into_iter()
            .find(|m| m.invoke == kind && m.name.eq_ignore_ascii_case(name))
    }
    pub fn member_by_dispid(&self, id: InterfaceId, dispid: i32, kind: InvokeKind) -> Option<&Member> {
        self.members(id).into_iter().find(|m| m.dispid == dispid && m.invoke == kind)
    }
    pub fn member_by_slot(&self, id: InterfaceId, slot: u16) -> Option<&Member> {
        self.members(id).into_iter().find(|m| m.slot == Some(slot))
    }
    /// Every DISPID a name maps to on `id` (GetIDsOfNames), case-insensitive.
    pub fn dispid_of(&self, id: InterfaceId, name: &str) -> Option<i32> {
        self.members(id)
            .into_iter()
            .find(|m| m.name.eq_ignore_ascii_case(name))
            .map(|m| m.dispid)
    }
}
