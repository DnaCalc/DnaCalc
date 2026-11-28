//! The interface descriptor: one source of interface facts for every projection.
//!
//! `descriptor/*.json` is written by `tools/com-descriptor` from the installed type libraries
//! (Excel 16.0, Office 2.8, Add-in Designer 1.0; the source file version is recorded in each
//! file). Names and ABI facts only: no documentation strings. To advertise another interface,
//! add its name to `descriptor/<library>.interfaces.txt` and regenerate; never hand-edit the JSON.

use serde::{Deserialize, de::DeserializeOwned};
use std::collections::HashMap;
use std::sync::OnceLock;

/// One library file as written by `tools/com-descriptor`.
#[derive(Clone, Debug, Deserialize)]
pub struct LibraryFile {
    /// Metadata identifying the originating library.
    pub library: LibraryInfo,
    /// Types extracted from the library in descriptor order.
    pub types: Vec<TypeInfo>,
    /// Enumerations and constants in their type-library order.
    #[serde(default)]
    pub enums: Vec<Enumeration>,
}

/// One enumeration from the type library.
#[derive(Clone, Debug, Deserialize)]
pub struct Enumeration {
    /// The name used in a type declaration.
    pub name: String,
    /// Every constant declared in the enumeration, including aliases.
    pub members: Vec<EnumMember>,
}

/// A named enumeration constant and its integer value.
#[derive(Clone, Debug, Deserialize)]
pub struct EnumMember {
    /// The member name.
    pub name: String,
    /// The exact integer supplied by the type library.
    pub value: i64,
}

#[derive(Clone, Debug, Deserialize)]
/// Identity and provenance of an Automation type library.
pub struct LibraryInfo {
    /// Library display name.
    pub name: String,
    /// Library GUID as recorded in the type library.
    pub guid: String,
    /// Type library's major and minor version.
    pub version: String,
    #[serde(default)]
    /// Locale ID of the type library.
    pub lcid: u32,
    #[serde(default)]
    /// Source type library file name as recorded by the extractor.
    pub source_file: String,
    #[serde(default)]
    /// Version of the source file, when available.
    pub file_version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
/// Kind of a described Automation type.
pub enum TypeKind {
    /// A COM interface with a vtable.
    Interface,
    /// An Automation dispinterface.
    Dispatch,
    /// A COM coclass.
    Coclass,
}

#[derive(Clone, Debug, Deserialize)]
/// An interface implemented or sourced by a coclass.
pub struct CoclassInterface {
    /// Name of the referenced interface.
    pub name: String,
    #[serde(default)]
    /// Whether this is the coclass's default interface for its role.
    pub default: bool,
    #[serde(default)]
    /// Whether this is an outgoing event interface.
    pub source: bool,
}

/// An interface, dispinterface or coclass.
#[derive(Clone, Debug, Deserialize)]
pub struct TypeInfo {
    /// Name used to identify the type in the descriptor.
    pub name: String,
    /// Interface, dispinterface or coclass category.
    pub kind: TypeKind,
    /// Interface or class GUID in type-library form.
    pub guid: String,
    #[serde(default)]
    /// Whether the interface is dual, supporting both vtable and dispatch calls.
    pub dual: bool,
    #[serde(default)]
    /// Type-library flags recorded for the type.
    pub flags: Vec<String>,
    #[serde(default)]
    /// Direct base interface name, when present.
    pub base: Option<String>,
    /// Total vtable slots including IUnknown (3) and IDispatch (4). A dispinterface has 7.
    #[serde(default)]
    pub vtable_slots: Option<u16>,
    #[serde(default)]
    /// Whether the interface is described only for dispatch.
    pub dispatch_only: bool,
    #[serde(default)]
    /// Interfaces implemented or sourced by the coclass.
    pub coclass_interfaces: Vec<CoclassInterface>,
    #[serde(default)]
    /// Members declared directly by this type, in descriptor order.
    pub members: Vec<Member>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
/// How a member is invoked through Automation.
pub enum InvokeKind {
    /// Read a property.
    Get,
    /// Assign a value to a property.
    Put,
    /// Assign an object reference to a property.
    Putref,
    /// Call a method.
    Method,
}

/// Outside capabilities named in the shared code execution policy.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub enum Capability {
    /// Read from the file system.
    #[serde(rename = "fs.read")]
    FsRead,
    /// Write to the file system.
    #[serde(rename = "fs.write")]
    FsWrite,
    /// Invoke an operating-system process.
    #[serde(rename = "process")]
    Process,
    /// Access the host environment.
    #[serde(rename = "env")]
    Env,
    /// Access host settings.
    #[serde(rename = "settings")]
    Settings,
    /// Call into a native library through a foreign-function interface.
    #[serde(rename = "ffi")]
    Ffi,
    /// Activate a COM object.
    #[serde(rename = "com.activate")]
    ComActivate,
    /// Send keyboard input to the user interface.
    #[serde(rename = "ui.keys")]
    UiKeys,
    /// Access the network.
    #[serde(rename = "network")]
    Network,
    /// Modify a VBA project.
    #[serde(rename = "project.write")]
    ProjectWrite,
    /// Display a message or input dialog.
    #[serde(rename = "ui.dialog")]
    UiDialog,
    /// Ask the person to pick a file or folder.
    #[serde(rename = "ui.filedialog")]
    UiFileDialog,
    /// Access the system clipboard.
    #[serde(rename = "clipboard")]
    Clipboard,
    /// Print a workbook or display a print preview.
    #[serde(rename = "print")]
    Print,
}

impl Capability {
    /// The stable spelling shared by the descriptor overlay and execution policy.
    pub fn key(self) -> &'static str {
        match self {
            Self::FsRead => "fs.read",
            Self::FsWrite => "fs.write",
            Self::Process => "process",
            Self::Env => "env",
            Self::Settings => "settings",
            Self::Ffi => "ffi",
            Self::ComActivate => "com.activate",
            Self::UiKeys => "ui.keys",
            Self::Network => "network",
            Self::ProjectWrite => "project.write",
            Self::UiDialog => Need::Dialog.key(),
            Self::UiFileDialog => Need::FileDialog.key(),
            Self::Clipboard => Need::Clipboard.key(),
            Self::Print => Need::Print.key(),
        }
    }

    pub(crate) const fn allowed_in_guarded(self) -> bool {
        matches!(self, Self::UiDialog | Self::UiFileDialog | Self::Clipboard | Self::Print)
    }
}

macro_rules! needs {
    ($($variant:ident => $key:literal),+ $(,)?) => {
        /// An Excel facility required by an object-model or C API member.
        #[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
        pub enum Need {
            $(#[doc = concat!("Descriptor need `", $key, "`.")]
              #[serde(rename = $key)]
              $variant),+
        }

        impl Need {
            /// Stable spelling used in descriptors and portability reports.
            pub const fn key(self) -> &'static str {
                match self { $(Self::$variant => $key),+ }
            }
        }
    };
}

needs! {
    FileTree => "fs",
    PersonalDisk => "fs.personal",
    WindowsPath => "path.windows",
    FileAttributes => "fs.attributes",
    Environment => "env",
    Settings => "settings",
    Process => "process",
    Dde => "dde",
    Mail => "mail",
    Dll => "native.code:dll",
    Xll => "native.code:xll",
    Dylib => "native.code:dylib",
    OsCom => "com.os",
    PortableClass => "com.portable",
    Network => "network",
    Dialog => "ui.dialog",
    Forms => "ui.forms",
    FileDialog => "ui.filedialog",
    Clipboard => "clipboard",
    InternalClipboard => "clipboard.internal",
    Print => "print",
    OpenExternal => "open.external",
    Block => "block",
    Keys => "ui.keys",
    Codegen => "codegen",
    Clock => "clock",
    Identity => "identity",
    Locale => "locale",
    Codepage => "codepage",
    Status => "ui.status",
    CommandBars => "ui.commandbars",
    EventLoop => "event-loop",
}

#[derive(Clone, Debug, Deserialize)]
/// Metadata for one method or property invocation form.
pub struct Member {
    /// Member name as recorded in the type library.
    pub name: String,
    /// Dispatch identifier used by IDispatch.
    pub dispid: i32,
    /// Method or property invocation form.
    pub invoke: InvokeKind,
    /// Outside capabilities from the committed reach overlay.
    #[serde(skip)]
    pub reach: Vec<Capability>,
    /// Host facilities from the committed needs overlay.
    #[serde(skip)]
    pub needs: Vec<Need>,
    /// vtable slot index (x64 `oVft / 8`); `None` for a dispinterface member.
    #[serde(default)]
    pub slot: Option<u16>,
    #[serde(default)]
    /// Whether the type library marks the member hidden.
    pub hidden: bool,
    #[serde(default)]
    /// Whether the type library marks the member restricted.
    pub restricted: bool,
    /// Every vtable parameter in order, including `[lcid]` and `[out, retval]`.
    #[serde(default)]
    pub params: Vec<Param>,
    /// The vtable function's return type (`HRESULT` for dual members).
    #[serde(rename = "return", default)]
    pub ret: String,
    #[serde(default)]
    /// Number of optional arguments recorded in the type library.
    pub optional_count: u16,
    #[serde(default)]
    /// Whether the member accepts a variable argument list.
    pub vararg: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
/// Type-library attributes of a vtable parameter.
pub enum ParamFlag {
    /// Supplies a value to the callee.
    In,
    /// Receives a value from the callee.
    Out,
    /// Supplies the locale identifier.
    Lcid,
    /// Receives the Automation return value.
    Retval,
    /// The caller may omit this parameter.
    Optional,
    /// The type library supplies a default value.
    Hasdefault,
}

#[derive(Clone, Debug, Deserialize)]
/// Metadata for one vtable parameter.
pub struct Param {
    /// `None` where the type library leaves a parameter unnamed (some dispinterface puts).
    #[serde(default)]
    pub name: Option<String>,
    /// Type string as `tools/com-descriptor` writes it (`VARIANT`, `BSTR*`, `Range*`,
    /// `enum:XlCalculation[I4]`, `SAFEARRAY(VARIANT)`, ...). See [`TypeRef::parse`].
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default)]
    /// Type-library flags that determine how the parameter is passed.
    pub flags: Vec<ParamFlag>,
    #[serde(default)]
    /// Default literal recorded in the descriptor, if any.
    pub default: Option<serde_json::Value>,
}

impl Param {
    /// Determine whether the parameter has the specified flag.
    pub fn has(&self, flag: ParamFlag) -> bool {
        self.flags.contains(&flag)
    }
    /// A parameter the caller supplies (not `[lcid]`, not `[out, retval]`).
    pub fn is_argument(&self) -> bool {
        !self.has(ParamFlag::Lcid) && !self.has(ParamFlag::Retval)
    }
    /// Parse this parameter's recorded type.
    pub fn type_ref(&self) -> TypeRef {
        TypeRef::parse(&self.ty)
    }
}

impl Member {
    /// The caller-supplied parameters, in declared order (what [`crate::Call::args`] holds).
    pub fn arguments(&self) -> impl Iterator<Item = &Param> {
        self.params.iter().filter(|param| param.is_argument())
    }
    /// Find the parameter flagged as the Automation return value.
    pub fn retval(&self) -> Option<&Param> {
        self.params.iter().find(|param| param.has(ParamFlag::Retval))
    }
    /// Determine whether the member receives a locale identifier.
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
    Enum {
        /// Enumeration name from the type library.
        name: String,
        /// Underlying scalar type name.
        base: String,
    },
    /// `alias:Name[BASE]`.
    Alias {
        /// Alias name from the type library.
        name: String,
        /// The type that the alias names.
        base: Box<TypeRef>,
    },
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
    /// Parse descriptor type syntax, treating an unrecognized token as a named type.
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
    /// The leading 32-bit group of the GUID.
    pub data1: u32,
    /// The first 16-bit group of the GUID.
    pub data2: u16,
    /// The second 16-bit group of the GUID.
    pub data3: u16,
    /// The final eight bytes in display order.
    pub data4: [u8; 8],
}

impl Guid {
    /// Parse the GUID's hexadecimal fields, with optional braces and hyphens.
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
pub struct InterfaceId(
    /// Zero-based type index in the descriptor that supplied this ID.
    pub u32,
);

/// All described libraries, indexed by type name (case-insensitive) and GUID.
pub struct Descriptor {
    /// Originating libraries in the order supplied to the descriptor.
    pub libraries: Vec<LibraryInfo>,
    enums: Vec<Vec<Enumeration>>,
    types: Vec<TypeInfo>,
    library_of: Vec<usize>,
    by_name: HashMap<String, InterfaceId>,
    by_guid: HashMap<Guid, InterfaceId>,
    /// Types by (library index, lower-case name): libraries may repeat a type name (MSForms'
    /// `Font`, `Page`, `Pages` beside Excel's), which `by_name` resolves to the first library.
    by_library_name: HashMap<(usize, String), InterfaceId>,
    xlc_reach: HashMap<String, Vec<Capability>>,
    xlf_reach: HashMap<String, Vec<Capability>>,
    xlc_needs: HashMap<String, Vec<Need>>,
    xlf_needs: HashMap<String, Vec<Need>>,
}

// Raw bytes, not include_str!: the descriptor is data, and os-imports does not follow include_str! into a file it cannot scan.
const EXCEL: &[u8] = include_bytes!("../descriptor/excel.json");
const OFFICE: &[u8] = include_bytes!("../descriptor/office.json");
const ADDIN_DESIGNER: &[u8] = include_bytes!("../descriptor/addin-designer.json");
const ACCESSIBILITY: &[u8] = include_bytes!("../descriptor/accessibility.json");
const MSFORMS: &[u8] = include_bytes!("../descriptor/msforms.json");

const REACH: &[u8] = include_bytes!("../descriptor/reach.json");
const NEEDS: &[u8] = include_bytes!("../descriptor/needs.json");

#[derive(Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "T: Deserialize<'de>"))]
struct Overlay<T> {
    members: HashMap<String, Vec<T>>,
    xlc: HashMap<String, Vec<T>>,
    #[serde(default)]
    xlf: HashMap<String, Vec<T>>,
}

impl Descriptor {
    /// The committed descriptor (Excel, Office, Add-in Designer, Accessibility, MSForms), parsed once.
    pub fn get() -> &'static Descriptor {
        static DESCRIPTOR: OnceLock<Descriptor> = OnceLock::new();
        DESCRIPTOR.get_or_init(|| {
            let text = |bytes: &'static [u8]| {
                std::str::from_utf8(bytes).expect("committed object-model descriptor is UTF-8")
            };
            let mut descriptor = Descriptor::from_files(&[
                text(EXCEL),
                text(OFFICE),
                text(ADDIN_DESIGNER),
                text(ACCESSIBILITY),
                text(MSFORMS),
            ])
            .expect("committed object-model descriptor is valid JSON");
            let reach = descriptor.load_overlay(REACH, "reach", |member, values| member.reach = values)
                .expect("committed reach overlay matches the descriptor");
            descriptor.xlc_reach = reach.xlc;
            descriptor.xlf_reach = reach.xlf;
            let needs = descriptor.load_overlay(NEEDS, "needs", |member, values| member.needs = values)
                .expect("committed needs overlay matches the descriptor");
            descriptor.xlc_needs = needs.xlc;
            descriptor.xlf_needs = needs.xlf;
            descriptor
        })
    }

    /// Build a descriptor from JSON library texts in input order.
    ///
    /// Returns the decoding error when a document or its fields cannot be deserialized.
    pub fn from_files(files: &[&str]) -> Result<Descriptor, String> {
        let mut descriptor = Descriptor {
            libraries: Vec::new(),
            enums: Vec::new(),
            types: Vec::new(),
            library_of: Vec::new(),
            by_name: HashMap::new(),
            by_guid: HashMap::new(),
            by_library_name: HashMap::new(),
            xlc_reach: HashMap::new(),
            xlf_reach: HashMap::new(),
            xlc_needs: HashMap::new(),
            xlf_needs: HashMap::new(),
        };
        for text in files {
            let file: LibraryFile = serde_json::from_str(text).map_err(|why| why.to_string())?;
            let library = descriptor.libraries.len();
            descriptor.libraries.push(file.library);
            descriptor.enums.push(file.enums);
            for ty in file.types {
                let id = InterfaceId(descriptor.types.len() as u32);
                if let Some(guid) = Guid::parse(&ty.guid) {
                    descriptor.by_guid.entry(guid).or_insert(id);
                }
                descriptor.by_name.entry(ty.name.to_ascii_lowercase()).or_insert(id);
                descriptor
                    .by_library_name
                    .entry((library, ty.name.to_ascii_lowercase()))
                    .or_insert(id);
                descriptor.types.push(ty);
                descriptor.library_of.push(library);
            }
        }
        Ok(descriptor)
    }

    /// Enumerations declared by the named library, in descriptor order.
    pub fn enumerations(&self, library: &str) -> &[Enumeration] {
        self.libraries.iter()
            .zip(&self.enums)
            .find(|(info, _)| info.name.eq_ignore_ascii_case(library))
            .map_or(&[], |(_, enums)| enums.as_slice())
    }

    fn load_overlay<T: DeserializeOwned>(
        &mut self,
        json: &[u8],
        name: &str,
        assign: impl Fn(&mut Member, Vec<T>),
    ) -> Result<Overlay<T>, String> {
        let mut overlay: Overlay<T> = serde_json::from_slice(json).map_err(|why| why.to_string())?;
        for ty in &mut self.types {
            for member in &mut ty.members {
                let key = format!("{}.{} ({:?})", ty.name, member.name, member.invoke);
                if let Some(values) = overlay.members.remove(&key) {
                    assign(member, values);
                }
            }
        }
        if !overlay.members.is_empty() {
            return Err(format!("unknown descriptor members in {name} overlay: {:?}", overlay.members.keys()));
        }
        let fold_names = |entries: HashMap<String, Vec<T>>| {
            entries.into_iter().map(|(key, values)| (key.to_ascii_lowercase(), values)).collect()
        };
        overlay.xlc = fold_names(overlay.xlc);
        overlay.xlf = fold_names(overlay.xlf);
        Ok(overlay)
    }

    /// Capabilities for commands that have no object-model member to inherit from.
    pub fn xlc_reach(&self, name: &str) -> &[Capability] {
        self.xlc_reach.get(&name.to_ascii_lowercase()).map_or(&[], Vec::as_slice)
    }

    /// Capabilities for C API functions and modifiers.
    pub fn xlf_reach(&self, name: &str) -> &[Capability] {
        self.xlf_reach.get(&name.to_ascii_lowercase()).map_or(&[], Vec::as_slice)
    }

    /// Required facilities for a C API command.
    pub fn xlc_needs(&self, name: &str) -> &[Need] {
        self.xlc_needs.get(&name.to_ascii_lowercase()).map_or(&[], Vec::as_slice)
    }

    /// Required facilities for a C API function or an info item such as `xlfGetWorkspace(23)`.
    pub fn xlf_needs(&self, name: &str) -> &[Need] {
        self.xlf_needs.get(&name.to_ascii_lowercase()).map_or(&[], Vec::as_slice)
    }

    /// A type by name. A library-qualified name (`Office.CommandBars`) is looked up by its
    /// type name: the described libraries do not repeat type names.
    pub fn id(&self, name: &str) -> Option<InterfaceId> {
        let name = name.rsplit('.').next().unwrap_or(name);
        self.by_name.get(&name.to_ascii_lowercase()).copied()
    }
    /// A type by name within the library of `owner`, else by [`Descriptor::id`]: the lookup for the
    /// names a type of that library mentions (its base, its default interface).
    pub fn id_near(&self, owner: InterfaceId, name: &str) -> Option<InterfaceId> {
        let name = name.rsplit('.').next().unwrap_or(name);
        let library = self.library_of[owner.0 as usize];
        self.by_library_name
            .get(&(library, name.to_ascii_lowercase()))
            .copied()
            .or_else(|| self.id(name))
    }
    /// A type by name within the library called `library` (case-insensitive).
    pub fn id_in(&self, library: &str, name: &str) -> Option<InterfaceId> {
        let name = name.rsplit('.').next().unwrap_or(name);
        let index = self.libraries.iter().position(|l| l.name.eq_ignore_ascii_case(library))?;
        self.by_library_name.get(&(index, name.to_ascii_lowercase())).copied()
    }
    /// Find the first described type with this GUID.
    pub fn by_guid(&self, guid: &Guid) -> Option<InterfaceId> {
        self.by_guid.get(guid).copied()
    }
    /// Look up a type by an ID from this descriptor. An out-of-range ID panics.
    pub fn ty(&self, id: InterfaceId) -> &TypeInfo {
        &self.types[id.0 as usize]
    }
    /// Iterate over type IDs and declarations in descriptor order.
    pub fn types(&self) -> impl Iterator<Item = (InterfaceId, &TypeInfo)> {
        self.types.iter().enumerate().map(|(i, ty)| (InterfaceId(i as u32), ty))
    }
    /// Return the library of an ID from this descriptor. An out-of-range ID panics.
    pub fn library(&self, id: InterfaceId) -> &LibraryInfo {
        &self.libraries[self.library_of[id.0 as usize]]
    }
    /// The default interface of a coclass, or the type itself.
    pub fn default_interface(&self, id: InterfaceId) -> InterfaceId {
        let ty = self.ty(id);
        if ty.kind == TypeKind::Coclass {
            if let Some(default) = ty.coclass_interfaces.iter().find(|i| i.default && !i.source) {
                if let Some(found) = self.id_near(id, &default.name) {
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
                .and_then(|base| self.id_near(at, base));
        }
        chain.iter().rev().flat_map(|at| self.ty(*at).members.iter()).collect()
    }
    /// Find a member by case-insensitive name and invocation kind, including inherited members.
    pub fn member(&self, id: InterfaceId, name: &str, kind: InvokeKind) -> Option<&Member> {
        self.members(id)
            .into_iter()
            .find(|m| m.invoke == kind && m.name.eq_ignore_ascii_case(name))
    }
    /// Find a member by dispatch identifier and invocation kind, including inherited members.
    pub fn member_by_dispid(&self, id: InterfaceId, dispid: i32, kind: InvokeKind) -> Option<&Member> {
        self.members(id).into_iter().find(|m| m.dispid == dispid && m.invoke == kind)
    }
    /// Find the member occupying a vtable slot, including inherited slots.
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
