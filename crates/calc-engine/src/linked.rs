//! Immutable inputs from other workbooks, calculated by the ordinary engine.
//! File caches never become source values here. An unavailable source cell leaves the
//! destination's explicitly qualified external-link fallback in charge.
use crate::{Calculation, EngineState, FormulaResult, contract::*};
use calc_doc_model::{AuthoredRevision, CellInput, ModelRoot};
use calc_value::{CalcValue, GridArea};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LinkedValue {
    pub value: CalcValue,
    // Retain the producing calculation, including its certificate and source pins.
    pub calculation: Option<Calculation>,
}

#[derive(Debug, PartialEq)]
pub(crate) struct LinkedSheet {
    pub name: String,
    pub cells: BTreeMap<(u32, u32), Option<LinkedValue>>,
    pub blank_proven: bool,
}

/// A source model and calculated results at one immutable frontier. Only EngineState
/// constructs it; callers cannot supply cached numbers as calculated source values.
#[derive(Debug)]
pub struct LinkedWorkbookSnapshot {
    id: u64,
    model: Arc<ModelRoot>,
    workbook: WorkbookInstanceId,
    results: ReadViewId,
    pub(crate) sheets: Vec<LinkedSheet>,
}
impl LinkedWorkbookSnapshot {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn revision(&self) -> AuthoredRevision {
        self.model.revision()
    }
    pub fn workbook(&self) -> WorkbookInstanceId {
        self.workbook
    }
    pub fn results(&self) -> ReadViewId {
        self.results
    }
    pub fn calculation(&self, sheet: &str, row: u32, column: u32) -> Option<&Calculation> {
        self.sheet(sheet)?
            .cells
            .get(&(row, column))?
            .as_ref()?
            .calculation
            .as_ref()
    }
    fn same_inputs(&self, other: &Self) -> bool {
        self.workbook == other.workbook
            && Arc::ptr_eq(&self.model, &other.model)
            && self.sheets == other.sheets
    }
    pub(crate) fn sheet(&self, name: &str) -> Option<&LinkedSheet> {
        self.sheets.iter().find(|s| {
            s.name
                .chars()
                .flat_map(char::to_uppercase)
                .eq(name.chars().flat_map(char::to_uppercase))
        })
    }
}
impl PartialEq for LinkedWorkbookSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for LinkedWorkbookSnapshot {}
impl std::hash::Hash for LinkedWorkbookSnapshot {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

pub(crate) type LinkedWorkbooks = BTreeMap<u32, Arc<LinkedWorkbookSnapshot>>;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LinkedInputStamp {
    pub version: u64,
    pub sources: Vec<Arc<LinkedWorkbookSnapshot>>,
}

impl EngineState {
    /// Pins actual authored inputs and certified results. Dirty, imported, refused,
    /// circular and qualified results remain unavailable, even when their numbers agree.
    pub fn linked_snapshot(&self, inputs: Arc<ModelRoot>) -> Option<Arc<LinkedWorkbookSnapshot>> {
        if inputs.revision() != self.revision() {
            return None;
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = NEXT
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |n| n.checked_add(1),
            )
            .expect("linked input identity exhausted");
        let mut sheets = Vec::new();
        for sheet in inputs.sheets() {
            let mut cells = BTreeMap::new();
            let mut blank_proven = inputs
                .definitions()
                .unloaded_in(GridArea::full_sheet(sheet.id()))
                .next()
                .is_none();
            for (at, input) in inputs.sheet_cells(sheet.id()).into_iter().flatten() {
                let value = match input {
                    CellInput::Number(n) => Some(LinkedValue {
                        value: CalcValue::number(n.get()),
                        calculation: None,
                    }),
                    CellInput::Text(t) => Some(LinkedValue {
                        value: CalcValue::text(t.text().clone()),
                        calculation: None,
                    }),
                    CellInput::Logical(b) => Some(LinkedValue {
                        value: CalcValue::logical(*b),
                        calculation: None,
                    }),
                    CellInput::Error(e) => Some(LinkedValue {
                        value: CalcValue::error(*e),
                        calculation: None,
                    }),
                    CellInput::Formula(_) => {
                        let value = self
                            .validated_result(&inputs, at)
                            .and_then(|_| self.result(at))
                            .and_then(FormulaResult::calculated)
                            .map(|(value, calculation)| LinkedValue {
                                value: self
                                    .spill(at)
                                    .map_or_else(|| value.clone(), |s| s.value_at(at).clone()),
                                calculation: Some(calculation.clone()),
                            });
                        blank_proven &= value.is_some();
                        value
                    }
                };
                cells.insert((at.row(), at.column()), value);
            }
            for spill in self.spills().filter(|s| s.anchor().sheet() == sheet.id()) {
                let valid = self.validated_result(&inputs, spill.anchor()).is_some();
                let calculation = self
                    .result(spill.anchor())
                    .and_then(FormulaResult::calculated)
                    .map(|(_, c)| c.clone());
                let area = spill.area();
                for row in area.first().row()..=area.last().row() {
                    for column in area.first().column()..=area.last().column() {
                        let at =
                            calc_value::CellKey::new(sheet.id(), row, column).expect("spill cell");
                        cells.insert(
                            (row, column),
                            valid.then(|| LinkedValue {
                                value: spill.value_at(at).clone(),
                                calculation: calculation.clone(),
                            }),
                        );
                    }
                }
                blank_proven &= valid;
            }
            // TABLE results and unloaded inputs also occupy cells without ordinary formulas.
            for node in self.nodes().filter(|n| n.anchor().sheet() == sheet.id()) {
                cells
                    .entry((node.anchor().row(), node.anchor().column()))
                    .or_insert_with(|| {
                        blank_proven = false;
                        None
                    });
            }
            for (at, _) in inputs
                .definitions()
                .unloaded_in(GridArea::full_sheet(sheet.id()))
            {
                cells.insert((at.row(), at.column()), None);
            }
            sheets.push(LinkedSheet {
                name: sheet.name().text().to_string_lossy(),
                cells,
                blank_proven,
            });
        }
        Some(Arc::new(LinkedWorkbookSnapshot {
            id,
            model: inputs,
            workbook: self.workbook_instance(),
            results: self.result_view(),
            sheets,
        }))
    }

    /// Replaces the external-input context for one link. Existing commands retain their
    /// immutable source cut; live hosts supersede them before using a new cut.
    pub fn set_linked_workbook(
        &mut self,
        number: u32,
        source: Option<Arc<LinkedWorkbookSnapshot>>,
    ) {
        if let (Some(old), Some(source)) = (self.linked_workbooks.get(&number), &source)
            && old.same_inputs(source)
        {
            return;
        }
        let mut next = (*self.linked_workbooks).clone();
        if let Some(source) = source {
            next.insert(number, source);
        } else {
            next.remove(&number);
        }
        if *self.linked_workbooks == next {
            return;
        }
        self.linked_workbooks = Arc::new(next);
        self.linked_input_version = self
            .linked_input_version
            .checked_add(1)
            .expect("link epoch exhausted");
        self.volatile_input_version = self
            .volatile_input_version
            .checked_add(1)
            .expect("input epoch exhausted");
        self.contract.invalidate(self.by_anchor.values().copied());
        self.pending.extend(self.by_anchor.values().copied());
    }
}
