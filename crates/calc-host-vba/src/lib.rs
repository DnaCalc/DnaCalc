//! Shared application composition of CalcHost and CalcVBA. One project session owns
//! worksheet functions and automatic document events, including projects read from files.
use calc_host::{
    BeforeSaveOutcome, HostState, ProjectedValue, ProjectionRequest, SaveOperation,
    WorkbookEventHandler, project,
};
use calc_value::{ArrayShape, CalcArray, CalcValue, GridArea, ReferenceLike, SheetId};
use calc_vba::events::{EventReadContext, EventSource, LoadedProject, ModuleKind, ProjectModule};
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

pub struct WorkbookEvents {
    project: LoadedProject,
    sheets: BTreeMap<SheetId, EventSource>,
    book: Option<EventSource>,
    max_steps: u64,
    file_project: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleTarget {
    Standard,
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
    DocumentIdentity(String),
    UnsupportedModule(String),
    Preparation(String),
}
impl std::fmt::Display for FileProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "workbook VBA project (dc-snz.15): ")?;
        match self {
            Self::Read(error) => error.fmt(f),
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
/// a partially loaded registry or substitutes the file's cached values.
pub fn prepare_file_project(
    host: &HostState,
    name: &str,
    max_steps: u64,
    backend: Option<calc_vba::Backend>,
) -> Result<Option<Rc<WorkbookEvents>>, FileProjectError> {
    let Some(project) = host
        .loaded()
        .read_vba_project(&Default::default())
        .map_err(FileProjectError::Read)?
    else {
        return Ok(None);
    };
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
        .modules
        .into_iter()
        .map(|module| {
            let target = match module.kind {
                calc_host::VbaModuleKind::Procedural => ModuleTarget::Standard,
                calc_host::VbaModuleKind::DocumentClassOrDesigner => {
                    match documents.get(&calc_value::fold_workbook_name(&module.name)) {
                        Some(Some(sheet)) => ModuleTarget::Worksheet(*sheet),
                        Some(None) => ModuleTarget::Workbook,
                        None => return Err(FileProjectError::UnsupportedModule(module.name)),
                    }
                }
            };
            Ok(ModuleSource {
                name: module.name,
                source: module.source,
                target,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    prepare_project(host, name, &modules, max_steps, backend, true)
        .map(Some)
        .map_err(FileProjectError::Preparation)
}

impl WorkbookEvents {
    /// Publish the UDF registry and event owner only after the entire project is ready.
    pub fn install(self: Rc<Self>, session: &mut calc_host::HostSession) {
        session.set_user_functions(self.project.functions());
        session.set_workbook_event_handler(Some(self));
    }
}

/// Open-time activation is shared by the worker and native corpus runner. Source
/// replacements use `prepare`/`install` and do not replay Workbook_Open.
pub fn activate_file_project(
    session: &mut calc_host::HostSession,
    name: &str,
    max_steps: u64,
    backend: Option<calc_vba::Backend>,
) -> Result<bool, FileProjectError> {
    if let Some(project) = prepare_file_project(session.host(), name, max_steps, backend)? {
        project.install(session);
        session.notify_workbook_open();
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn prepare(
    host: &HostState,
    name: &str,
    modules: &[ModuleSource],
    max_steps: u64,
    backend: Option<calc_vba::Backend>,
) -> Result<Rc<WorkbookEvents>, String> {
    prepare_project(host, name, modules, max_steps, backend, false)
}

fn prepare_project(
    host: &HostState,
    name: &str,
    modules: &[ModuleSource],
    max_steps: u64,
    backend: Option<calc_vba::Backend>,
    file_project: bool,
) -> Result<Rc<WorkbookEvents>, String> {
    let mut sheets = BTreeSet::new();
    let mut documents = Vec::new();
    let mut workbook = false;
    let modules = modules
        .iter()
        .map(|module| {
            let kind = match module.target {
                ModuleTarget::Standard => ModuleKind::Standard,
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
    let project = calc_vba::events::load_project(&modules, max_steps, backend)?;
    let mut owner = WorkbookEvents {
        project,
        sheets: BTreeMap::new(),
        book: None,
        max_steps,
        file_project,
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
impl WorkbookEventHandler for WorkbookEvents {
    fn workbook_open(&self, host: &HostState) -> Result<(), String> {
        if let Some(source) = &self.book {
            source.open(&Read(host, self.file_project), self.max_steps)?;
        }
        Ok(())
    }
    fn worksheet_change(
        &self,
        host: &HostState,
        sheet: SheetId,
        target: &[GridArea],
    ) -> Result<(), String> {
        if let Some(source) = self.sheets.get(&sheet) {
            source.change(
                ReferenceLike::from_areas(target.to_vec()).map_err(|e| e.to_string())?,
                &Read(host, self.file_project),
                self.max_steps,
            )?;
        }
        Ok(())
    }
    fn worksheet_calculate(&self, host: &HostState, sheet: SheetId) -> Result<(), String> {
        if let Some(source) = self.sheets.get(&sheet) {
            source.calculate(&Read(host, self.file_project), self.max_steps)?;
        }
        Ok(())
    }
    fn before_save(&self, host: &HostState, _: SaveOperation) -> Result<BeforeSaveOutcome, String> {
        // These are programmatic save commands. The UI SaveAs dialog flag is a separate input.
        let cancel = if let Some(source) = &self.book {
            source.before_save(false, &Read(host, self.file_project), self.max_steps)?
        } else {
            false
        };
        Ok(BeforeSaveOutcome {
            edits: vec![],
            cancel,
        })
    }
}
