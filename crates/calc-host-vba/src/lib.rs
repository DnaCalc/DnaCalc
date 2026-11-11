//! Shared application composition of CalcHost and CalcVBA. One project session owns
//! worksheet functions and automatic document events, including projects read from files.
use calc_host::{
    BeforeSaveOutcome, HostState, ProjectedValue, ProjectionRequest, SaveOperation,
    WorkbookEventHandler, project,
};
use calc_value::{ArrayShape, CalcArray, CalcValue, GridArea, ReferenceLike, SheetId};
use calc_vba::events::{EventReadContext, EventSource, LoadedProject, ModuleKind, ProjectModule};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

mod trust;
pub use trust::{FileOrigin, GrantReason, MacroBlock, ProjectGrant};

pub struct WorkbookEvents {
    project: LoadedProject,
    sheets: BTreeMap<SheetId, EventSource>,
    book: Option<EventSource>,
    max_steps: u64,
    file_project: bool,
    automatic_events: bool,
    /// Why a file's project runs; `None` for source modules a person loaded.
    grant: Option<ProjectGrant>,
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
    let project = if file_project {
        calc_vba::events::load_file_project_with(
            &modules,
            references,
            max_steps,
            backend,
            if automatic_events {
                calc_vba::events::DocumentEvents::All
            } else {
                calc_vba::events::DocumentEvents::None
            },
        )?
    } else {
        calc_vba::events::load_project(&modules, max_steps, backend)?
    };
    let mut owner = WorkbookEvents {
        project,
        sheets: BTreeMap::new(),
        book: None,
        max_steps,
        file_project,
        automatic_events,
        grant,
    };
    // Attach all new identities before replacing any live registration. A failed
    // attachment drops only the new project; its handlers have never been raised.
    for (module, sheet) in documents {
        let name = host
            .model()
            .sheet_by_id(sheet)
            .expect("validated worksheet")
            .name()
            .text()
            .to_string_lossy();
        let source = EventSource::worksheet(&name);
        owner.project.attach_document(&module, &source)?;
        owner.sheets.insert(sheet, source);
    }
    for module in modules.iter().filter(|m| m.kind == ModuleKind::Workbook) {
        let source = EventSource::workbook(name);
        owner.project.attach_document(&module.name, &source)?;
        owner.book = Some(source);
    }
    Ok(Rc::new(owner))
}

// A copied event value outlives its read in mutable VBA state. Until that state
// carries observation provenance, file projects cannot erase these qualifications.
fn event_input_needs_provenance(value: &ProjectedValue) -> bool {
    match value {
        ProjectedValue::FileCached(_) | ProjectedValue::Stale(_) => true,
        ProjectedValue::Calculated { calculation, .. } => {
            calculation.reads_external_links()
                || calculation.held_by_circular_rule()
                || calculation.stamp().is_none_or(|stamp| {
                    stamp.reads.uses_imported_cache
                        || stamp.reads.uses_boundary_cache
                        || stamp.reads.legacy_boundary
                        || !stamp.reads.observations.is_empty()
                })
        }
        _ => false,
    }
}

struct Read<'a>(&'a HostState, bool);
impl EventReadContext for Read<'_> {
    fn read_reference(&self, reference: &ReferenceLike) -> Result<CalcValue, String> {
        if reference.area_count() != 1 {
            return Err("Value2 of a multi-area event Target is not modelled".into());
        }
        let area = reference.areas()[0];
        if area.cell_count() > 1_048_576 {
            return Err("Event Range.Value2 exceeds its materialization limit".into());
        }
        let mut cells = Vec::with_capacity(area.cell_count() as usize);
        for row in area.first().row()..=area.last().row() {
            for column in area.first().column()..=area.last().column() {
                cells.push(
                    calc_value::CellKey::new(area.sheet(), row, column)
                        .map_err(|e| e.to_string())?,
                );
            }
        }
        let page = project(self.0, &ProjectionRequest { cells }).map_err(|e| e.to_string())?;
        let values = page
            .cells()
            .iter()
            .map(|cell| match cell.value() {
                value if self.1 && event_input_needs_provenance(value) => Err(
                    "File-project event reads with cached or uncertified provenance are not modelled (dc-snz.15)".into(),
                ),
                ProjectedValue::Literal(value) | ProjectedValue::Calculated { value, .. } => {
                    Ok(value.clone())
                }
                ProjectedValue::FileCached(cache) => Ok(cache.value().to_value()),
                ProjectedValue::Stale(stale) => Ok(stale
                    .last_value()
                    .cloned()
                    .or_else(|| stale.file_cache().map(|cache| cache.value().to_value()))
                    .unwrap_or_else(CalcValue::empty)),
                ProjectedValue::Empty => Ok(CalcValue::empty()),
                ProjectedValue::NotModeled { .. } => {
                    Err("A VBA event read an unmodelled formula result".into())
                }
            })
            .collect::<Result<Vec<_>, String>>()?;
        if values.len() == 1 {
            return Ok(values[0].clone());
        }
        CalcArray::new(
            ArrayShape {
                rows: area.row_count() as usize,
                cols: area.column_count() as usize,
            },
            values,
        )
        .map(CalcValue::array)
        .ok_or_else(|| "Invalid event Range.Value2 array shape".into())
    }
}

/// One synchronous loan of the authoritative session. A host operation releases
/// every read borrow before dispatch, and a nested event receives a fresh reborrow
/// of this same session. No queued edits, session pointers or copied workbook owner.
struct Application<'a> {
    session: RefCell<&'a mut calc_host::HostSession>,
    file_project: bool,
}
impl EventReadContext for Application<'_> {
    fn read_reference(&self, reference: &ReferenceLike) -> Result<CalcValue, String> {
        let session = self.session.borrow();
        Read(session.host(), self.file_project).read_reference(reference)
    }
    fn resolve_reference(&self, text: &str) -> Result<ReferenceLike, String> {
        let session = self.session.borrow();
        let host = session.host();
        let sheet = session
            .selection()
            .active
            .map(|at| at.sheet())
            .or_else(|| host.loaded().active_sheet())
            .filter(|sheet| host.model().sheet_by_id(*sheet).is_some())
            .ok_or("Range lookup has no active worksheet (dc-hsa5.12)")?;
        let origin = calc_value::CellKey::new(sheet, 1, 1).map_err(|e| e.to_string())?;
        match host.resolve_reference_text(origin, text) {
            calc_host::ReferenceTarget::Area(area) => Ok(ReferenceLike::area(area)),
            _ => Err(
                "application Range lookup outside a single local area is not modelled (dc-hsa5.12)"
                    .into(),
            ),
        }
    }
    fn write_reference(&self, reference: &ReferenceLike, value: CalcValue) -> Result<(), String> {
        if reference.area_count() != 1 {
            return Err("multi-area Range.Value2 writes are not modelled (dc-hsa5.12)".into());
        }
        let area = reference.areas()[0];
        if area.cell_count() > 1_048_576 {
            return Err("Range.Value2 write exceeds its materialization limit (dc-hsa5.12)".into());
        }
        let array = value.as_array();
        if let Some(array) = array
            && (array.shape().rows != area.row_count() as usize
                || array.shape().cols != area.column_count() as usize)
        {
            return Err(
                "Range.Value2 array/target shape differences are not modelled (dc-hsa5.12)".into(),
            );
        }
        // Validate every element before dispatch. One Automation edit owns the
        // complete rectangle, its calculation and its nested Change event.
        let scalar = array.is_none().then(|| value_input(&value)).transpose()?;
        let edits = if scalar == Some(None) {
            vec![calc_host::CellEdit::Clear { area }]
        } else {
            let mut edits = Vec::with_capacity(area.cell_count() as usize);
            for row in 0..area.row_count() {
                for column in 0..area.column_count() {
                    let input = if let Some(array) = array {
                        value_input(
                            array
                                .get(row as usize, column as usize)
                                .expect("validated rectangle"),
                        )?
                    } else {
                        scalar.as_ref().expect("scalar input").clone()
                    };
                    let at = calc_value::CellKey::new(
                        area.sheet(),
                        area.first().row() + row,
                        area.first().column() + column,
                    )
                    .map_err(|e| e.to_string())?;
                    edits.push(match input {
                        Some(input) => calc_host::CellEdit::Set { at, input },
                        None => calc_host::CellEdit::Clear {
                            area: GridArea::cell(at),
                        },
                    });
                }
            }
            edits
        };
        let receipt = self.session.borrow_mut().dispatch_from(
            calc_host::CommandIngress::Automation,
            calc_host::SkinIntent::Edit { edits },
        );
        match receipt.recalc {
            Some(calc_host::HostProgress::Failed(error)) => {
                return Err(format!(
                    "calculation after a macro write failed: {error:?} (dc-hsa5.12)"
                ));
            }
            Some(calc_host::HostProgress::Waiting { .. }) => {
                return Err(
                    "calculation after a macro write is waiting for a provider (dc-hsa5.12)".into(),
                );
            }
            Some(calc_host::HostProgress::Cancelled) => {
                return Err("calculation after a macro write was cancelled (dc-hsa5.12)".into());
            }
            _ => {}
        }
        if self.session.borrow().has_unfinished_calculation() {
            return Err("calculation after a macro write is unfinished (dc-hsa5.12)".into());
        }
        match receipt.error {
            Some(error) => Err(error.to_string()),
            None => Ok(()),
        }
    }
    fn calculate(&self, command: calc_vba::events::CalculationCommand) -> Result<(), String> {
        use calc_host::{CalcScope, EvalSelection, SkinIntent};
        use calc_vba::events::CalculationCommand as C;
        let intent = match command {
            C::Dirty => SkinIntent::Recalculate,
            C::Full => SkinIntent::Calculate {
                scope: CalcScope::Workbook,
                selection: EvalSelection::ForceAll,
            },
            C::Rebuild => SkinIntent::Rebuild,
            C::Range(reference) => SkinIntent::CalculateRange {
                areas: reference.areas().to_vec(),
            },
        };
        let mut session = self.session.borrow_mut();
        let receipt = session.dispatch_from(calc_host::CommandIngress::Automation, intent);
        if let Some(error) = receipt.error {
            return Err(error.to_string());
        }
        let mut progress = receipt.recalc;
        loop {
            match progress {
                Some(calc_host::HostProgress::Failed(error)) => return Err(format!("macro calculation failed: {error:?} (dc-hsa5.12)")),
                Some(calc_host::HostProgress::Waiting { .. }) => return Err("a synchronous macro calculation waiting for a provider is not modelled (dc-hsa5.12)".into()),
                Some(calc_host::HostProgress::Cancelled) => return Err("macro calculation was cancelled (dc-hsa5.12)".into()),
                _ => {},
            }
            if !session.has_pending_calculation() {
                return if session.has_unfinished_calculation() {
                    Err(
                        "a synchronous macro calculation has unfinished provider work (dc-hsa5.12)"
                            .into(),
                    )
                } else {
                    Ok(())
                };
            }
            progress = Some(session.advance_calculation());
        }
    }
    fn events_enabled(&self) -> Result<bool, String> {
        Ok(self.session.borrow().workbook_events_enabled())
    }
    fn set_events_enabled(&self, enabled: bool) -> Result<(), String> {
        self.session
            .borrow_mut()
            .set_workbook_events_enabled(enabled);
        Ok(())
    }
}
fn value_input(value: &CalcValue) -> Result<Option<calc_host::CellInput>, String> {
    use calc_host::{CellInput, CellText, FiniteNumber};
    use calc_value::CoreRef;
    Ok(Some(match value.core_ref() {
        CoreRef::Empty => return Ok(None),
        CoreRef::Number(n) => CellInput::Number(
            FiniteNumber::new(n)
                .ok_or("nonfinite Range.Value2 write is not modelled (dc-hsa5.12)")?,
        ),
        CoreRef::Text(t) => CellInput::Text(
            CellText::new(t.clone())
                .ok_or("Range.Value2 text exceeds the cell limit (dc-hsa5.12)")?,
        ),
        CoreRef::Logical(b) => CellInput::Logical(b),
        CoreRef::Error(e) => CellInput::Error(e),
        _ => return Err("a nonscalar Range.Value2 element is not modelled (dc-hsa5.12)".into()),
    }))
}
impl WorkbookEventHandler for WorkbookEvents {
    fn file_identity_changed(&self, name: &str) {
        if let Some(source) = &self.book {
            source.set_name(name);
        }
    }
    fn workbook_open(&self, session: &mut calc_host::HostSession) -> Result<(), String> {
        self.require_automatic_events()?;
        if let Some(source) = &self.book {
            source.open(
                &Application {
                    session: RefCell::new(session),
                    file_project: self.file_project,
                },
                self.max_steps,
            )?;
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
            source.change(
                ReferenceLike::from_areas(target.to_vec()).map_err(|e| e.to_string())?,
                &Application {
                    session: RefCell::new(session),
                    file_project: self.file_project,
                },
                self.max_steps,
            )?;
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
            source.calculate(
                &Application {
                    session: RefCell::new(session),
                    file_project: self.file_project,
                },
                self.max_steps,
            )?;
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
            source.before_save(
                operation == SaveOperation::SaveAsDialog,
                &Application {
                    session: RefCell::new(session),
                    file_project: self.file_project,
                },
                self.max_steps,
            )?
        } else {
            false
        };
        Ok(BeforeSaveOutcome {
            edits: vec![],
            cancel,
        })
    }
}
