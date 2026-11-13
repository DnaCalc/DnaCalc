//! Explicit workbook projects and runtime-owned automatic document events.
//! Hosts attach document identities; the compiler selects private handlers.
use crate::{Backend, Module, OwnedSession, UserFunctions};
use std::rc::Rc;
mod source;
#[cfg(test)]
mod tests;
pub(crate) mod types;
pub use source::{EventReadContext, EventSource};
pub(crate) use source::{application_object, callback_active, object_type};

/// Commands admitted by a mutable document-event owner. Ordinary worksheet UDFs
/// have no authority to execute these operations.
#[derive(Clone, Debug)]
pub enum CalculationCommand {
    Dirty,
    Full,
    Rebuild,
    Range(calc_func::value::ReferenceLike),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleKind {
    Standard,
    Class,
    Worksheet,
    Workbook,
}
#[derive(Clone, Debug)]
pub struct ProjectModule {
    pub name: String,
    pub source: String,
    pub kind: ModuleKind,
}

pub struct LoadedProject {
    pub(crate) session: Rc<OwnedSession>,
    pub(crate) functions: UserFunctions,
}
impl LoadedProject {
    /// UDF entries and automatic handlers share this exact owning project session.
    pub fn functions(&self) -> UserFunctions {
        self.functions.clone()
    }
    pub fn attach_document(&self, module: &str, source: &EventSource) -> Result<(), String> {
        calcvba_host_api::Session::attach_document(&*self.session, module, source.object())
            .map_err(|e| format!("automatic document attachment ({module}, dc-hsa5.12): {e}"))
    }
    pub fn reset(&self) -> Result<(), String> {
        calcvba_host_api::Session::reset(&*self.session).map_err(|e| e.to_string())
    }
}

/// Which document events a project's handlers receive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentEvents {
    /// None: the project's worksheet functions only (the full UDF signature surface).
    None,
    /// The workbook's own events (`Workbook_Open`, `Workbook_AddinInstall`,
    /// `Workbook_BeforeSave`) beside the full UDF signature surface: an add-in's case.
    Workbook,
    /// Every automatic document event (DNA Calc's file projects), with the narrower UDF
    /// signatures that event projects admit (dc-hsa5.12).
    All,
}

/// The type-library references a project compiles against beside DNA's own objects.
#[derive(Clone, Debug, Default)]
pub struct ProjectReferences {
    pub(crate) compiler: Vec<calcvba_symbol::manifest::ProjectReference>,
    /// The reference names the platform's registered type libraries resolve.
    pub(crate) libraries: Vec<String>,
    /// Whether calls reach platform COM objects (a native Windows build only).
    pub(crate) com: bool,
    /// References unavailable to this compiler, named if compilation needs one.
    unavailable: Vec<String>,
}
impl ProjectReferences {
    pub fn none() -> Self {
        Self::default()
    }
    /// The references of an Office project read from its blob. Only a native Windows build
    /// reads registered type libraries. Elsewhere compilation can use the built-in objects, and
    /// reports the unavailable references if their types are needed.
    pub fn of(project: &crate::office::OfficeProject) -> Result<Self, String> {
        if !cfg!(all(windows, not(target_arch = "wasm32"))) {
            // A reference to another VBA project is refused on every platform.
            project.compiler_references()?;
            return Ok(Self {
                unavailable: project
                    .com_references()
                    .iter()
                    .map(|reference| reference.name().to_owned())
                    .collect(),
                ..Self::none()
            });
        }
        let compiler = project.compiler_references()?;
        let libraries: Vec<String> = project
            .references()
            .iter()
            .filter(|reference| {
                reference.guid().as_deref() != Some(crate::office::VBA_LIBRARY)
                    && reference.guid().as_deref() != Some(crate::office::EXCEL_LIBRARY)
            })
            .map(|reference| reference.name().to_owned())
            .collect();
        Ok(Self {
            compiler,
            libraries,
            com: !project.com_references().is_empty(),
            unavailable: Vec::new(),
        })
    }
}

impl ProjectReferences {
    /// The references a source project names (name and GUID of each type library). The Scripting
    /// library is served by the engine on every platform (its Dictionary, dc-tey4.4); any other
    /// library cannot be resolved without a registered type library and is named if needed.
    pub fn of_source_libraries(libraries: &[(String, Option<String>)]) -> Self {
        let mut out = Self::none();
        for (name, guid) in libraries {
            if name.eq_ignore_ascii_case("Scripting") {
                out.compiler.push(calcvba_symbol::manifest::ProjectReference::TypeLibrary {
                    name: name.clone(),
                    guid: guid.clone(),
                    version_major: None,
                    version_minor: None,
                    lcid: None,
                    import_lib: None,
                });
            } else {
                out.unavailable.push(name.clone());
            }
        }
        out
    }
}

pub fn load_project(
    modules: &[ProjectModule],
    max_steps: u64,
    backend: Option<Backend>,
) -> Result<LoadedProject, String> {
    load_project_with_references(modules, &ProjectReferences::none(), max_steps, backend)
}

/// [`load_project`] for a source project that names type-library references.
pub fn load_project_with_references(
    modules: &[ProjectModule],
    references: &ProjectReferences,
    max_steps: u64,
    backend: Option<Backend>,
) -> Result<LoadedProject, String> {
    // Every Excel workbook has a `ThisWorkbook` document; a project that names it (DnaExamples'
    // DnaBook) compiles. With no file behind the project nothing is attached, so reaching the
    // document refuses at run time instead of failing the whole load (dc-tey4.2).
    let mut with_book;
    let modules = if modules.iter().any(|m| m.kind == ModuleKind::Workbook) {
        modules
    } else {
        with_book = modules.to_vec();
        with_book.push(ProjectModule {
            name: "ThisWorkbook".into(),
            source: String::new(),
            kind: ModuleKind::Workbook,
        });
        &with_book[..]
    };
    load_project_impl(
        modules,
        references,
        max_steps,
        backend,
        false,
        DocumentEvents::All,
        crate::Profile::Full,
    )
}

/// An opened workbook's worksheet lookup also admits its private standard-module
/// functions, as recorded by dc-snz.7. VBA-to-VBA visibility stays unchanged.
pub fn load_file_project(
    modules: &[ProjectModule],
    max_steps: u64,
    backend: Option<Backend>,
) -> Result<LoadedProject, String> {
    load_file_project_with_events(modules, max_steps, backend, true)
}

pub fn load_file_project_with_events(
    modules: &[ProjectModule],
    max_steps: u64,
    backend: Option<Backend>,
    automatic_events: bool,
) -> Result<LoadedProject, String> {
    load_file_project_with(
        modules,
        &ProjectReferences::none(),
        max_steps,
        backend,
        if automatic_events {
            DocumentEvents::All
        } else {
            DocumentEvents::None
        },
    )
}

/// A file's project with its type-library references and the document events its handlers
/// receive.
pub fn load_file_project_with(
    modules: &[ProjectModule],
    references: &ProjectReferences,
    max_steps: u64,
    backend: Option<Backend>,
    events: DocumentEvents,
) -> Result<LoadedProject, String> {
    load_project_impl(
        modules,
        references,
        max_steps,
        backend,
        true,
        events,
        crate::Profile::Full,
    )
}

fn load_project_impl(
    modules: &[ProjectModule],
    references: &ProjectReferences,
    max_steps: u64,
    backend: Option<Backend>,
    private_worksheet_lookup: bool,
    events: DocumentEvents,
    profile: crate::Profile,
) -> Result<LoadedProject, String> {
    if modules.is_empty()
        || modules.len() > 1024
        || modules.iter().any(|m| m.source.len() > 1_048_576)
        || modules.iter().map(|m| m.source.len()).sum::<usize>() > 4 * 1_048_576
    {
        return Err("VBA project exceeds its source/module limits (dc-hsa5.12)".into());
    }
    let module = Module::load_event_project(modules, references, private_worksheet_lookup, events)
        .map_err(|e| {
            if references.unavailable.is_empty() {
                format!("document project (dc-hsa5.12): {e}")
            } else {
                format!("document project (dc-hsa5.12): {e}; VBA references {} cannot be resolved on this platform (dc-0f1k.4)", references.unavailable.join(", "))
            }
        })?;
    crate::prepare_project(module, max_steps, backend, profile)
        .map_err(|e| format!("document project (dc-hsa5.12): {e}"))
}

/// An Office project compiled for an application that has the project's workbook (an add-in or
/// a session file) but no workbook engine: its worksheet functions, and the workbook document
/// whose own events the application raises (theory 5a, dc-0f1k.4).
pub struct OfficeLoad {
    pub project: LoadedProject,
    /// The workbook document (`ThisWorkbook`), attached, when the project has one.
    pub workbook: Option<EventSource>,
    /// Document event handlers the project declares that this route does not deliver (sheet
    /// events, workbook events other than Open, AddinInstall and BeforeSave), by name.
    pub undelivered: Vec<String>,
}

const DELIVERED_WORKBOOK_EVENTS: [&str; 3] = ["Open", "AddinInstall", "BeforeSave"];

/// Compiles `project` with its type-library references, as the file named `workbook_name`
/// (what `ThisWorkbook.Name` gives). Document modules bind by their kind; a designer
/// (UserForm) or chart module with code is refused, since neither is modelled. Nothing runs:
/// the caller raises `Workbook_Open` (and `Workbook_AddinInstall`) through [`OfficeLoad::workbook`].
pub fn load_office_project(
    project: &crate::office::OfficeProject,
    workbook_name: &str,
    max_steps: u64,
    backend: Option<Backend>,
    profile: crate::Profile,
) -> Result<OfficeLoad, String> {
    use crate::office::OfficeModuleKind as K;
    let references = ProjectReferences::of(project)?;
    let mut modules = Vec::new();
    let mut undelivered = Vec::new();
    let mut workbook_module = None;
    for module in project.modules() {
        let kind = match (module.kind, module.document.as_deref()) {
            (K::Standard, _) => ModuleKind::Standard,
            (K::Class, _) => ModuleKind::Class,
            (K::Document, Some("Excel.Workbook")) if workbook_module.is_none() => {
                workbook_module = Some(module.name.clone());
                ModuleKind::Workbook
            }
            (K::Document, Some("Excel.Worksheet")) => ModuleKind::Worksheet,
            _ if !has_code(&module.source) => continue,
            _ => {
                return Err(format!(
                    "the module {} ({}) is not modelled: OneCalc runs standard, class, workbook and worksheet modules (dc-0f1k.4)",
                    module.name,
                    module.document.as_deref().unwrap_or("UserForm")
                ));
            }
        };
        if module.kind == K::Document {
            let prefix = if kind == ModuleKind::Workbook {
                "Workbook_"
            } else {
                "Worksheet_"
            };
            for handler in event_handlers(&module.source, prefix) {
                let delivered = kind == ModuleKind::Workbook
                    && DELIVERED_WORKBOOK_EVENTS
                        .iter()
                        .any(|event| handler.eq_ignore_ascii_case(event));
                if !delivered {
                    undelivered.push(format!("{}.{prefix}{handler}", module.name));
                }
            }
        }
        modules.push(ProjectModule {
            name: module.name.clone(),
            source: module.source.clone(),
            kind,
        });
    }
    let loaded = load_project_impl(
        &modules,
        &references,
        max_steps,
        backend,
        true,
        DocumentEvents::Workbook,
        profile,
    )?;
    let workbook = match workbook_module {
        Some(module) => {
            let source = EventSource::workbook(workbook_name);
            loaded.attach_document(&module, &source)?;
            Some(source)
        }
        None => None,
    };
    Ok(OfficeLoad {
        project: loaded,
        workbook,
        undelivered,
    })
}

/// Whether a module's source holds more than `Attribute` lines, `Option` statements, comments
/// and blank lines.
fn has_code(source: &str) -> bool {
    source.lines().map(str::trim).any(|line| {
        let lower = line.to_ascii_lowercase();
        !(line.is_empty()
            || line.starts_with('\'')
            || lower.starts_with("attribute ")
            || lower.starts_with("option ")
            || lower.starts_with("rem "))
    })
}

/// The event names of the `Sub <prefix><Event>` procedures a document module declares.
fn event_handlers(source: &str, prefix: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let mut word = words.next()?;
            if word.eq_ignore_ascii_case("Private") || word.eq_ignore_ascii_case("Public") {
                word = words.next()?;
            }
            if !word.eq_ignore_ascii_case("Sub") {
                return None;
            }
            let name = words.next()?.split('(').next()?;
            (name.len() > prefix.len() && name[..prefix.len()].eq_ignore_ascii_case(prefix))
                .then(|| name[prefix.len()..].to_owned())
        })
        .collect()
}
