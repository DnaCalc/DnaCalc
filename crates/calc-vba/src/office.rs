//! The VBA project of an Office file, read from its `vbaProject.bin` part (theory 5a, dc-0f1k.4).
//!
//! The file belongs to calc-doc: the OPC package, the part, its relationship and content type, and
//! putting a changed part back on save. calc-doc hands over the part as an opaque blob. CalcVBA
//! (`calcvba-office`, built without its file and package handling) reads everything inside it: the
//! compound storage, MS-OVBA decompression, the `dir` stream with the modules, the code page and
//! the references. This module is the glue's view of that project; [`crate::events`] compiles it
//! and registers its public procedures as worksheet functions.
use calcvba_office as office;
use calcvba_symbol::manifest as sym;

/// What a module of the project is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfficeModuleKind {
    Standard,
    Class,
    /// A document module: `ThisWorkbook` or a sheet's, bound to its document by code name.
    Document,
    /// A designer (UserForm): its code is not modelled.
    Designer,
}

/// One module: its name, its kind and its source as exported (with its `Attribute` lines).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfficeModule {
    pub name: String,
    /// The module's stream in the project's `VBA` storage.
    pub stream_name: String,
    /// Byte offset of the compressed source in that stream (the compiled cache comes first).
    pub source_offset: usize,
    pub kind: OfficeModuleKind,
    /// Whether the directory marks the module private (`Option Private Module` aside).
    pub private: bool,
    pub source: String,
    /// For a document module, the host document type its `VB_Base` names
    /// (`Excel.Workbook`, `Excel.Worksheet`, `Excel.Chart`).
    pub document: Option<String>,
}

/// One reference of the project, in the project's order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OfficeReference {
    /// A registered type library: `libid` is the `*\G{guid}#major.minor#lcid#path#description`
    /// the VBE writes.
    TypeLibrary { name: String, libid: String },
    /// Another VBA project (a referenced .xlam or workbook).
    Project { name: String, path: String },
    /// An ActiveX control's type library.
    Control { name: String },
}

impl OfficeReference {
    /// The name VBA code qualifies the library's types with (`Scripting`, `stdole`).
    pub fn name(&self) -> &str {
        match self {
            Self::TypeLibrary { name, .. }
            | Self::Project { name, .. }
            | Self::Control { name } => name,
        }
    }
    /// The library's GUID, upper case without braces, for a registered type library.
    pub fn guid(&self) -> Option<String> {
        let Self::TypeLibrary { libid, .. } = self else {
            return None;
        };
        let start = libid.find('{')? + 1;
        let end = start + libid[start..].find('}')?;
        Some(libid[start..end].to_ascii_uppercase())
    }
}

/// The VBA library itself (VBE7): built into CalcVBA's compiler.
pub(crate) const VBA_LIBRARY: &str = "000204EF-0000-0000-C000-000000000046";
/// The Excel object library: bound to DNA's own objects where OneCalc and DNA Calc have them.
pub(crate) const EXCEL_LIBRARY: &str = "00020813-0000-0000-C000-000000000046";
/// OLE Automation (stdole2.tlb): CalcVBA's catalogue knows it on every platform.
pub(crate) const STDOLE_LIBRARY: &str = "00020430-0000-0000-C000-000000000046";

/// A project as CalcVBA read it from the blob.
#[derive(Clone, Debug)]
pub struct OfficeProject {
    project: office::Project,
    modules: Vec<OfficeModule>,
    references: Vec<OfficeReference>,
}

/// The size limits CalcVBA reads a project blob under (the whole blob, the `dir` stream, the
/// module count, each module's and all modules' source).
pub use office::ReadLimits as OfficeReadLimits;

/// Reads a `vbaProject.bin` blob under CalcVBA's default limits. Nothing is compiled or run.
pub fn read_office_project(blob: &[u8]) -> Result<OfficeProject, String> {
    read_office_project_with(blob, &OfficeReadLimits::default())
}

/// [`read_office_project`] under `limits`.
pub fn read_office_project_with(
    blob: &[u8],
    limits: &OfficeReadLimits,
) -> Result<OfficeProject, String> {
    let project = office::Project::read_bin_with_limits(blob, limits).map_err(|e| e.to_string())?;
    OfficeProject::of(project)
}

impl OfficeProject {
    fn of(project: office::Project) -> Result<Self, String> {
        let modules = project
            .modules
            .iter()
            .map(|module| {
                Ok(OfficeModule {
                    name: module.name.clone(),
                    stream_name: module.stream_name.clone(),
                    source_offset: module.text_offset() as usize,
                    private: module.private,
                    kind: match module.kind {
                        office::ModuleKind::Standard => OfficeModuleKind::Standard,
                        office::ModuleKind::Class => OfficeModuleKind::Class,
                        office::ModuleKind::Document => OfficeModuleKind::Document,
                        office::ModuleKind::Form => OfficeModuleKind::Designer,
                    },
                    source: module.source.clone(),
                    document: module.host_document_type().map_err(|e| e.to_string())?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let references = project
            .references
            .iter()
            .map(|reference| match &reference.kind {
                office::ReferenceKind::Registered { libid } => OfficeReference::TypeLibrary {
                    name: reference.name.clone(),
                    libid: libid.clone(),
                },
                office::ReferenceKind::Project { absolute, .. } => OfficeReference::Project {
                    name: reference.name.clone(),
                    path: absolute.clone(),
                },
                office::ReferenceKind::Control { .. } => OfficeReference::Control {
                    name: reference.name.clone(),
                },
            })
            .collect();
        Ok(Self {
            project,
            modules,
            references,
        })
    }

    /// The project's name (`VBAProject` unless renamed).
    pub fn name(&self) -> &str {
        &self.project.properties.name
    }
    /// The code page the project's text is stored in.
    pub fn code_page(&self) -> u16 {
        self.project.properties.codepage
    }
    /// The modules, in the project's order.
    pub fn modules(&self) -> &[OfficeModule] {
        &self.modules
    }
    /// The references, in the project's order.
    pub fn references(&self) -> &[OfficeReference] {
        &self.references
    }

    /// The same project with the code of `name` replaced (or a standard or class module of that
    /// name added). `code` is the body as the VBE shows it, without `Attribute` lines.
    pub fn with_module(
        &self,
        name: &str,
        kind: OfficeModuleKind,
        code: &str,
    ) -> Result<OfficeProject, String> {
        let kind = match kind {
            OfficeModuleKind::Standard => office::ModuleKind::Standard,
            OfficeModuleKind::Class => office::ModuleKind::Class,
            OfficeModuleKind::Document => office::ModuleKind::Document,
            OfficeModuleKind::Designer => office::ModuleKind::Form,
        };
        let mut project = self.project.clone();
        project
            .put_module(name, kind, code)
            .map_err(|e| e.to_string())?;
        let mut changed = Self::of(project)?;
        // `to_blob` writes every module's source from offset 0 (the compiled cache is dropped), so
        // the offsets this project reports are the written blob's, not the blob it was read from.
        for module in &mut changed.modules {
            module.source_offset = 0;
        }
        Ok(changed)
    }

    /// The blob of this project, as calc-doc puts it back into the file. A project read and not
    /// changed is written by its owner as the bytes it was read from, never through here.
    pub fn to_blob(&self) -> Result<Vec<u8>, String> {
        self.project.to_bin().map_err(|e| e.to_string())
    }

    /// The type-library references the compiler resolves itself: every reference but VBA's own
    /// library and Excel's (which the glue binds to DNA's objects). A reference to another VBA
    /// project is refused: one project is compiled at a time.
    pub(crate) fn compiler_references(&self) -> Result<Vec<sym::ProjectReference>, String> {
        let mut references = Vec::new();
        for (reference, held) in self.references.iter().zip(&self.project.references) {
            match reference {
                OfficeReference::Project { name, path } => {
                    return Err(format!(
                        "the reference to the VBA project {name} ({path}) is not modelled: one project is compiled at a time (dc-0f1k.4)"
                    ));
                }
                OfficeReference::TypeLibrary { .. }
                    if matches!(
                        reference.guid().as_deref(),
                        Some(VBA_LIBRARY | EXCEL_LIBRARY)
                    ) => {}
                _ => references.push(held.type_library().map_err(|e| e.to_string())?),
            }
        }
        Ok(references)
    }

    /// The references that need the platform's registered type libraries (all but VBA, Excel and
    /// OLE Automation), by the name code uses.
    pub fn com_references(&self) -> Vec<&OfficeReference> {
        self.references
            .iter()
            .filter(|reference| {
                !matches!(
                    reference.guid().as_deref(),
                    Some(VBA_LIBRARY | EXCEL_LIBRARY | STDOLE_LIBRARY)
                )
            })
            .collect()
    }
}
