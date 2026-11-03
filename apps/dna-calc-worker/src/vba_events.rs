//! Application composition of the host's event positions and CalcVBA's automatic sources.
use crate::wire::{VbaModule, VbaModuleTarget};
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

pub(crate) struct WorkbookEvents {
    pub(crate) project: LoadedProject,
    sheets: BTreeMap<SheetId, EventSource>,
    book: Option<EventSource>,
    max_steps: u64,
}
pub(crate) fn prepare(
    host: &HostState,
    name: &str,
    modules: &[VbaModule],
    max_steps: u64,
    backend: Option<calc_vba::Backend>,
) -> Result<Rc<WorkbookEvents>, String> {
    let mut sheets = BTreeSet::new();
    let mut documents = Vec::new();
    let mut workbook = false;
    let modules = modules
        .iter()
        .map(|module| {
            let kind = match module.target {
                VbaModuleTarget::Standard => ModuleKind::Standard,
                VbaModuleTarget::Workbook if !workbook => {
                    workbook = true;
                    ModuleKind::Workbook
                }
                VbaModuleTarget::Workbook => {
                    return Err("Duplicate VBA workbook document (dc-hsa5.12)".into());
                }
                VbaModuleTarget::Worksheet(sheet) => {
                    let sheet = sheet.id();
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

struct Read<'a>(&'a HostState);
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
    fn worksheet_change(
        &self,
        host: &HostState,
        sheet: SheetId,
        target: &[GridArea],
    ) -> Result<(), String> {
        if let Some(source) = self.sheets.get(&sheet) {
            source.change(
                ReferenceLike::from_areas(target.to_vec()).map_err(|e| e.to_string())?,
                &Read(host),
                self.max_steps,
            )?;
        }
        Ok(())
    }
    fn worksheet_calculate(&self, host: &HostState, sheet: SheetId) -> Result<(), String> {
        if let Some(source) = self.sheets.get(&sheet) {
            source.calculate(&Read(host), self.max_steps)?;
        }
        Ok(())
    }
    fn before_save(&self, host: &HostState, _: SaveOperation) -> Result<BeforeSaveOutcome, String> {
        // These are programmatic save commands. The UI SaveAs dialog flag is a separate input.
        let cancel = if let Some(source) = &self.book {
            source.before_save(false, &Read(host), self.max_steps)?
        } else {
            false
        };
        Ok(BeforeSaveOutcome {
            edits: vec![],
            cancel,
        })
    }
}
