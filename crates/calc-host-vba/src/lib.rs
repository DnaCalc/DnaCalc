//! Shared application composition of CalcHost and CalcVBA. One project session owns
//! worksheet functions and automatic document events, including projects read from files,
//! and routes every Excel/Office member call to `calc_host::automation::WorkbookAutomation`
//! (docs/architecture/OBJECT_MODEL.md section 8, dc-piw0.5).
use calc_host::{
    BeforeSaveOutcome, HostState, SaveOperation, WorkbookEventHandler,
    automation::{AutomationHost, WorkbookAutomation},
};
use calc_object_model::{AutoError, AutoValue, profile::ApplicationHost};
use calc_value::{GridArea, ReferenceLike, SheetId};
use calc_vba::events::{EventSource, LoadedProject, ModuleKind, ProjectModule};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

mod trust;
pub use trust::{FileOrigin, GrantReason, MacroBlock, ProjectGrant};

pub struct WorkbookEvents {
    project: LoadedProject,
    automation: Rc<WorkbookAutomation>,
    sheets: BTreeMap<SheetId, EventSource>,
    book: Option<EventSource>,
    max_steps: u64,
    file_project: bool,
    automatic_events: bool,
    /// Why a file's project runs; `None` for source modules a person loaded.
    grant: Option<ProjectGrant>,
    /// The policy's audit log the project's hosts write to (a granted file project only).
    audit: Option<calc_vba::policy::AuditHandle>,
    /// Every procedure run through the command route, in order (dc-vba-command-es70w).
    invocations: std::cell::RefCell<Vec<InvocationRecord>>,
}

pub use command::{
    Denial, InvocationKind, InvocationRecord, Outcome, ProcedureArg, ProcedureEntry,
    ProcedureKind, ProcedureResult,
};
mod command;
mod book;
pub use book::{ApplicationOptions, install_application};

struct VbaAutomationHost {
    events_enabled: Cell<bool>,
    /// The project `Application.Run` resolves its target in (set once the owner exists).
    owner: std::cell::RefCell<std::rc::Weak<WorkbookEvents>>,
}

impl VbaAutomationHost {
    fn new() -> Self {
        Self {
            events_enabled: Cell::new(true),
            owner: Default::default(),
        }
    }
}

impl ApplicationHost for VbaAutomationHost {
    fn hwnd(&self) -> isize {
        0
    }
    fn run(&self, name: &str, arguments: &[AutoValue]) -> Result<AutoValue, AutoError> {
        match self.owner.borrow().upgrade() {
            Some(owner) => owner.application_run(name, arguments),
            None => Err(AutoError::not_modelled("_Application.Run", "dc-piw0.5")),
        }
    }
    fn events_enabled(&self) -> bool {
        self.events_enabled.get()
    }
    fn set_events_enabled(&self, enabled: bool) {
        self.events_enabled.set(enabled);
    }
}

impl AutomationHost for VbaAutomationHost {
    fn with_invocation(
        &self,
        call: &mut dyn FnMut(&mut dyn calc_func::UserFunctionContext) -> Result<AutoValue, AutoError>,
    ) -> Option<Result<AutoValue, AutoError>> {
        calc_vba::with_active_invocation(call)
    }
    fn with_session(
        &self,
        _call: &mut dyn FnMut(&calc_host::HostSession) -> Result<AutoValue, AutoError>,
    ) -> Result<AutoValue, AutoError> {
        Err(AutoError::not_modelled(
            "Automation session outside a workbook event or worksheet invocation",
            "dc-piw0.5",
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleTarget {
    Standard,
    Class,
    Workbook,
    Worksheet(SheetId),
}

#[derive(Clone, Debug)]
pub struct ModuleSource {
    pub name: String,
    pub source: String,
    pub target: ModuleTarget,
}

#[derive(Debug)]
pub enum FileProjectError {
    Read(calc_host::VbaReadError),
    /// CalcVBA could not read the project part (its storage, directory, code page or text).
    Project(String),
    DocumentIdentity(String),
    UnsupportedModule(String),
    Preparation(String),
}
impl std::fmt::Display for FileProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "workbook VBA project (dc-snz.15): ")?;
        match self {
            Self::Read(error) => error.fmt(f),
            Self::Project(error) => write!(f, "invalid VBA project: {error}"),
            Self::DocumentIdentity(name) => write!(f, "ambiguous document code name {name:?}"),
            Self::UnsupportedModule(name) => write!(
                f,
                "class, designer or unbound document module {name:?} is not modelled"
            ),
            Self::Preparation(error) => f.write_str(error),
        }
    }
}
impl std::error::Error for FileProjectError {}

/// Preparation executes no handlers or worksheet functions. Failure never publishes
/// a partially loaded registry or substitutes the file's cached values. `grant` is the reason the
/// file's project may run at all ([`ProjectGrant`]).
pub fn prepare_file_project(
    grant: &ProjectGrant,
    host: &HostState,
    name: &str,
    max_steps: u64,
    backend: Option<calc_vba::Backend>,
) -> Result<Option<Rc<WorkbookEvents>>, FileProjectError> {
    prepare_file_project_with_events(grant, host, name, max_steps, backend, true)
}

/// A disabled-events admission still compiles every module. Its owner refuses
/// event delivery if a caller later enables events without preparing again.
pub fn prepare_file_project_with_events(
    grant: &ProjectGrant,
    host: &HostState,
    name: &str,
    max_steps: u64,
    backend: Option<calc_vba::Backend>,
    automatic_events: bool,
) -> Result<Option<Rc<WorkbookEvents>>, FileProjectError> {
    // calc-doc hands over the project part as a blob; CalcVBA reads what is inside it.
    let Some(part) = host
        .loaded()
        .vba_project(&Default::default())
        .map_err(FileProjectError::Read)?
    else {
        return Ok(None);
    };
    let project = calc_vba::read_office_project(&part.bytes).map_err(FileProjectError::Project)?;
    let references =
        calc_vba::events::ProjectReferences::of(&project).map_err(FileProjectError::Preparation)?;
    let mut documents = BTreeMap::new();
    for (name, sheet) in host
        .loaded()
        .vba_document_names()
        .map_err(FileProjectError::Read)?
    {
        let key = calc_value::fold_workbook_name(&name);
        if name.is_empty() || documents.insert(key, sheet).is_some() {
            return Err(FileProjectError::DocumentIdentity(name));
        }
    }
    let modules = project
        .modules()
        .iter()
        .map(|module| {
            let target = match module.kind {
                calc_vba::OfficeModuleKind::Standard => ModuleTarget::Standard,
                calc_vba::OfficeModuleKind::Class => {
                    if documents.contains_key(&calc_value::fold_workbook_name(&module.name)) {
                        return Err(FileProjectError::DocumentIdentity(module.name.clone()));
                    }
                    ModuleTarget::Class
                }
                calc_vba::OfficeModuleKind::Document | calc_vba::OfficeModuleKind::Designer => {
                    match documents.get(&calc_value::fold_workbook_name(&module.name)) {
                        Some(Some(sheet))
                            if module.kind == calc_vba::OfficeModuleKind::Document =>
                        {
                            ModuleTarget::Worksheet(*sheet)
                        }
                        Some(None) if module.kind == calc_vba::OfficeModuleKind::Document => {
                            ModuleTarget::Workbook
                        }
                        _ => return Err(FileProjectError::UnsupportedModule(module.name.clone())),
                    }
                }
            };
            Ok(ModuleSource {
                name: module.name.clone(),
                source: module.source.clone(),
                target,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    prepare_project(
        host,
        name,
        &modules,
        &references,
        max_steps,
        backend,
        Some(grant.clone()),
        automatic_events,
    )
    .map(Some)
    .map_err(FileProjectError::Preparation)
}

impl WorkbookEvents {
    fn require_automatic_events(&self) -> Result<(), String> {
        if self.automatic_events {
            Ok(())
        } else {
            Err("file project was admitted with automatic events disabled; prepare it again before event delivery (dc-snz.15)".into())
        }
    }
    /// Publish the UDF registry and event owner only after the entire project is ready.
    pub fn install(self: Rc<Self>, session: &mut calc_host::HostSession) {
        let functions = self.functions();
        self.install_with_registry(session, functions);
    }
    /// Why the file's project runs ([`ProjectGrant`]); `None` for source modules a person loaded.
    pub fn grant(&self) -> Option<&ProjectGrant> {
        self.grant.as_ref()
    }
    /// File providers may compose native entries before publishing the complete registry.
    pub fn functions(&self) -> calc_host::CallableTable {
        self.project.functions()
    }
    pub fn install_with_registry(
        self: Rc<Self>,
        session: &mut calc_host::HostSession,
        functions: calc_host::CallableTable,
    ) {
        session.set_user_functions(functions);
        session.set_workbook_event_handler(Some(self));
    }
}

/// Open-time activation is shared by the worker and native corpus runner. Source
/// replacements use `prepare`/`install` and do not replay Workbook_Open.
pub fn activate_file_project(
    grant: &ProjectGrant,
    session: &mut calc_host::HostSession,
    name: &str,
    max_steps: u64,
    backend: Option<calc_vba::Backend>,
) -> Result<bool, FileProjectError> {
    open_file_project(grant, session, name, max_steps, backend).map(|project| project.is_some())
}

/// [`activate_file_project`], returning the installed project, whose functions a host composes
/// with its add-ins' when it later publishes their table to the session.
pub fn open_file_project(
    grant: &ProjectGrant,
    session: &mut calc_host::HostSession,
    name: &str,
    max_steps: u64,
    backend: Option<calc_vba::Backend>,
) -> Result<Option<Rc<WorkbookEvents>>, FileProjectError> {
    let project = prepare_file_project_with_events(
        grant,
        session.host(),
        name,
        max_steps,
        backend,
        session.workbook_events_enabled(),
    )?;
    if let Some(project) = &project {
        project.clone().install(session);
        session.notify_workbook_open();
    }
    Ok(project)
}

pub fn prepare(
    host: &HostState,
    name: &str,
    modules: &[ModuleSource],
    max_steps: u64,
    backend: Option<calc_vba::Backend>,
) -> Result<Rc<WorkbookEvents>, String> {
    prepare_project(
        host,
        name,
        modules,
        &calc_vba::events::ProjectReferences::none(),
        max_steps,
        backend,
        None,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn prepare_project(
    host: &HostState,
    name: &str,
    modules: &[ModuleSource],
    references: &calc_vba::events::ProjectReferences,
    max_steps: u64,
    backend: Option<calc_vba::Backend>,
    grant: Option<ProjectGrant>,
    automatic_events: bool,
) -> Result<Rc<WorkbookEvents>, String> {
    let file_project = grant.is_some();
    let mut sheets = BTreeSet::new();
    let mut documents = Vec::new();
    let mut workbook = false;
    let modules = modules
        .iter()
        .map(|module| {
            let kind = match module.target {
                ModuleTarget::Standard => ModuleKind::Standard,
                ModuleTarget::Class => ModuleKind::Class,
                ModuleTarget::Workbook if !workbook => {
                    workbook = true;
                    ModuleKind::Workbook
                }
                ModuleTarget::Workbook => {
                    return Err("Duplicate VBA workbook document (dc-hsa5.12)".into());
                }
                ModuleTarget::Worksheet(sheet) => {
                    if host.model().sheet_by_id(sheet).is_none() || !sheets.insert(sheet) {
                        return Err(
                            "Unknown or duplicate VBA worksheet document (dc-hsa5.12)".into()
                        );
                    }
                    documents.push((module.name.clone(), sheet));
                    ModuleKind::Worksheet
                }
            };
            Ok(ProjectModule {
                name: module.name.clone(),
                source: module.source.clone(),
                kind,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    // One WorkbookAutomation instance owns every Excel/Office member call for this project.
    let application = Rc::new(VbaAutomationHost::new());
    let automation = Rc::new(WorkbookAutomation::new(application.clone()));
    automation.context_changed_from_host(host, name);
    automation.register();
    // The grant's policy wraps every host built for this load (CalcVBA's PolicyHost); a stricter
    // policy the thread already holds (Guarded) stays, and the thread's earlier policy is put back
    // after the load.
    let _policy = grant.as_ref().map(|grant| {
        calc_vba::policy::PolicyScope::enter(match grant.policy() {
            // CalcVBA keeps `PolicyMode::Safe` until the post-vx1 rename (dc-8ovd).
            calc_macro_security::VbaPolicy::Guarded => calc_vba::policy::PolicyMode::Safe,
            calc_macro_security::VbaPolicy::Full => calc_vba::policy::PolicyMode::Full,
        })
    });
    let loaded = if file_project {
        calc_vba::events::load_file_project_with_model(
            &modules,
            references,
            max_steps,
            backend,
            if automatic_events {
                calc_vba::events::DocumentEvents::All
            } else {
                calc_vba::events::DocumentEvents::None
            },
            automation.clone(),
        )
    } else {
        calc_vba::events::load_project_with_model(
            &modules,
            references,
            max_steps,
            backend,
            automation.clone(),
        )
    };
    let project = loaded?;
    let audit = _policy.as_ref().and_then(|_| calc_vba::policy::current_audit());
    let mut owner = WorkbookEvents {
        project,
        automation: automation.clone(),
        sheets: BTreeMap::new(),
        book: None,
        max_steps,
        file_project,
        automatic_events,
        grant,
        audit,
        invocations: Default::default(),
    };
    // Attach all new identities before replacing any live registration. A failed
    // attachment drops only the new project; its handlers have never been raised.
    for (module, sheet) in documents {
        let obj = automation
            .worksheet_object(sheet)
            .map_err(|e| e.description())?;
        let source = EventSource::for_object(automation.clone(), obj)?;
        owner.project.attach_document(&module, &source)?;
        owner.sheets.insert(sheet, source);
    }
    for module in modules.iter().filter(|m| m.kind == ModuleKind::Workbook) {
        let obj = automation.workbook_object().map_err(|e| e.description())?;
        let source = EventSource::for_object(automation.clone(), obj)?;
        owner.project.attach_document(&module.name, &source)?;
        owner.book = Some(source);
    }
    let owner = Rc::new(owner);
    *application.owner.borrow_mut() = Rc::downgrade(&owner);
    Ok(owner)
}

impl WorkbookEventHandler for WorkbookEvents {
    fn context_changed(&self, session: &calc_host::HostSession) {
        self.automation.context_changed(session);
    }
    fn file_identity_changed(&self, name: &str) {
        self.automation.set_full_name(name);
    }
    fn workbook_open(&self, session: &mut calc_host::HostSession) -> Result<(), String> {
        self.require_automatic_events()?;
        if let Some(source) = &self.book {
            self.automation
                .with_event_session(session, self.file_project, || {
                    source.open_model(self.max_steps)
                })?;
        }
        Ok(())
    }
    fn worksheet_change(
        &self,
        session: &mut calc_host::HostSession,
        sheet: SheetId,
        target: &[GridArea],
    ) -> Result<(), String> {
        self.require_automatic_events()?;
        if let Some(source) = self.sheets.get(&sheet) {
            let reference =
                ReferenceLike::from_areas(target.to_vec()).map_err(|e| e.to_string())?;
            let target_obj = self
                .automation
                .acquire_event_range(session, &reference)
                .map_err(|e| e.description())?;
            self.automation
                .with_event_session(session, self.file_project, || {
                    source.change_object(target_obj, self.max_steps)
                })?;
        }
        // Excel raises the workbook's Sheet event after the sheet's own.
        if let Some(book) = &self.book {
            let reference =
                ReferenceLike::from_areas(target.to_vec()).map_err(|e| e.to_string())?;
            let target_obj = self
                .automation
                .acquire_event_range(session, &reference)
                .map_err(|e| e.description())?;
            let sheet_obj = self.automation.worksheet_object(sheet).map_err(|e| e.description())?;
            self.automation
                .with_event_session(session, self.file_project, || {
                    book.sheet_change_object(sheet_obj, target_obj, self.max_steps)
                })?;
        }
        Ok(())
    }
    fn worksheet_calculate(
        &self,
        session: &mut calc_host::HostSession,
        sheet: SheetId,
    ) -> Result<(), String> {
        self.require_automatic_events()?;
        if let Some(source) = self.sheets.get(&sheet) {
            self.automation
                .with_event_session(session, self.file_project, || {
                    source.calculate_model(self.max_steps)
                })?;
        }
        if let Some(book) = &self.book {
            let sheet_obj = self.automation.worksheet_object(sheet).map_err(|e| e.description())?;
            self.automation
                .with_event_session(session, self.file_project, || {
                    book.sheet_calculate_object(sheet_obj, self.max_steps)
                })?;
        }
        Ok(())
    }
    fn selection_changed(
        &self,
        session: &mut calc_host::HostSession,
        sheet: SheetId,
        target: &[GridArea],
    ) -> Result<(), String> {
        self.require_automatic_events()?;
        if let Some(book) = &self.book {
            let reference =
                ReferenceLike::from_areas(target.to_vec()).map_err(|e| e.to_string())?;
            let target_obj = self
                .automation
                .acquire_event_range(session, &reference)
                .map_err(|e| e.description())?;
            let sheet_obj = self.automation.worksheet_object(sheet).map_err(|e| e.description())?;
            self.automation
                .with_event_session(session, self.file_project, || {
                    book.sheet_selection_change_object(sheet_obj, target_obj, self.max_steps)
                })?;
        }
        Ok(())
    }
    fn before_save(
        &self,
        session: &mut calc_host::HostSession,
        operation: SaveOperation,
    ) -> Result<BeforeSaveOutcome, String> {
        self.require_automatic_events()?;
        let cancel = if let Some(source) = &self.book {
            self.automation
                .with_event_session(session, self.file_project, || {
                    source.before_save_model(operation == SaveOperation::SaveAsDialog, self.max_steps)
                })?
        } else {
            false
        };
        Ok(BeforeSaveOutcome {
            edits: vec![],
            cancel,
        })
    }
}
