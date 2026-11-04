//! The published generation: each formula cell's result with its provenance, and the value delta
//! a finished recalculation publishes.

use calc_doc_model::{AuthoredRevision, FileCached};
use calc_fml::{CompileError, EvalFault};
use calc_value::{CalcValue, CellKey, CoreRef, GridArea, NodeId, NumericMode, NumericTargetId};
use std::fmt;
use std::sync::Arc;

/// The identity of one recalculation: the engine state issues the next tick each time a
/// recalculation begins, counting up from 1, so every result a recalculation publishes names the
/// one recalculation that produced it. A recalculation that fails or is refused after it began
/// still used its tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CalcTick(u64);

impl CalcTick {
    pub(crate) const fn new(value: u64) -> Self {
        CalcTick(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// How a result was calculated: from the authored inputs of `revision`, by the recalculation
/// `tick`, under the numeric `mode`, whose Exact form carries the Excel target it reproduces.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Calculation {
    revision: AuthoredRevision,
    tick: CalcTick,
    /// Shared by every result of the recalculation (dc-dxm): an Exact mode names its Excel target
    /// in four strings, and each formula's result carries its calculation.
    mode: Arc<NumericMode>,
    /// The result read values another workbook's link caches, directly or through the formulas
    /// it read (dc-ylr): it is calculated, from inputs that are the file's cache of the other
    /// book rather than live ones.
    external_links: bool,
    /// The result is the value the circular-reference rule holds (dc-y2o.1): with iteration off,
    /// a formula on a circular reference, or one that reads such a formula, is not evaluated and
    /// keeps what its cell held, as Excel keeps it. The value is the rule's, not an evaluation's.
    circular_rule: bool,
    stamp: Option<Arc<crate::contract::ResultStamp>>,
}

impl Calculation {
    pub(crate) fn new(revision: AuthoredRevision, tick: CalcTick, mode: NumericMode) -> Self {
        Calculation {
            revision,
            tick,
            mode: Arc::new(mode),
            external_links: false,
            circular_rule: false,
            stamp: None,
        }
    }

    pub fn stamp(&self) -> Option<&crate::contract::ResultStamp> {
        self.stamp.as_deref()
    }
    pub(crate) fn set_stamp(&mut self, stamp: crate::contract::ResultStamp) {
        self.stamp = Some(Arc::new(stamp));
    }

    /// This calculation, marked as the circular-reference rule's (dc-y2o.1).
    pub(crate) fn with_circular_rule(mut self) -> Self {
        self.circular_rule = true;
        self
    }

    /// Whether the value is the one the circular-reference rule holds with iteration off
    /// (dc-y2o.1): the formula is on a circular reference, or reads one, and was not evaluated; it
    /// shows what its cell held (the value this engine last calculated for it, else the value
    /// the file stored, else an empty text), or 0 when it was entered since its last result.
    pub fn held_by_circular_rule(&self) -> bool {
        self.circular_rule
    }

    /// This calculation, marked as reading values another workbook's link caches.
    pub(crate) fn with_external_links(mut self) -> Self {
        self.external_links = true;
        self
    }

    /// Whether the result read values another workbook's link caches, directly or through the
    /// formulas it read, rather than only live inputs (dc-ylr).
    pub fn reads_external_links(&self) -> bool {
        self.external_links
    }

    /// The authored revision whose inputs the calculation read.
    pub fn revision(&self) -> AuthoredRevision {
        self.revision
    }

    pub fn tick(&self) -> CalcTick {
        self.tick
    }

    pub fn mode(&self) -> &NumericMode {
        &self.mode
    }

    /// The Excel target of an Exact calculation; `None` under Fast.
    pub fn target(&self) -> Option<&NumericTargetId> {
        self.mode.target()
    }
}

/// Why a formula's result is not modelled. Each is a typed refusal: the formula is reported as
/// not calculated and has no value.
#[derive(Clone, Debug, PartialEq)]
pub enum NotModeled {
    /// The formula's text is not valid UTF-16 (it holds an unpaired surrogate), so it was not
    /// parsed.
    FormulaText,
    /// CalcFml refused to compile the formula.
    Compile(CompileError),
    /// CalcFml's evaluation returned a typed refusal. A formula that reads a cell whose result is
    /// not calculated (a file cache, or a result not modelled) is refused here as
    /// [`EvalFault::MissingInput`].
    Evaluation(EvalFault),
    /// The evaluation returned a value a cell cannot hold as calculated here: a reference,
    /// an empty or omitted value, or an unqualified nonfinite number (an array result is
    /// shown by its anchor and spills, `spill.rs`).
    Result(CalcValue),
}

impl NotModeled {
    /// The calculated value `value`, or the refusal of a value a cell cannot hold as calculated
    /// here.
    pub(crate) fn publishable(value: CalcValue) -> Result<CalcValue, NotModeled> {
        // Expression results retain callable identity. A cell publishes its worksheet face,
        // using the same projection as spill cells (retained fn2 q017–020).
        let value = if value.callable_value().is_some() {
            crate::spill::shown(&value)
        } else {
            value
        };
        let held = value.callable_value().is_none()
            && match value.core_ref() {
                CoreRef::Number(number) => number.is_finite() || value.is_excel_overflow(),
                CoreRef::Text(_) | CoreRef::Logical(_) | CoreRef::Error(_) => true,
                CoreRef::Empty | CoreRef::Missing | CoreRef::Array(_) | CoreRef::Reference(_) => {
                    false
                }
            };
        if held {
            Ok(value)
        } else {
            Err(NotModeled::Result(value))
        }
    }
}

impl fmt::Display for NotModeled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotModeled::FormulaText => f.write_str("the formula text holds an unpaired surrogate"),
            NotModeled::Compile(error) => write!(f, "CalcFml refused to compile it: {error:?}"),
            NotModeled::Evaluation(fault) => write!(f, "CalcFml refused to evaluate it: {fault:?}"),
            NotModeled::Result(value) => {
                write!(f, "it evaluated to a value no cell holds here: {value:?}")
            }
        }
    }
}

/// A formula cell's published result.
#[derive(Clone, Debug, PartialEq)]
pub enum FormulaResult {
    /// The value this engine calculated through CalcFml.
    Calculated {
        value: CalcValue,
        calculation: Calculation,
    },
    /// The result a file stored for the formula. Nothing here calculated it: it never counts as
    /// calculated. Iteration starts and admitted scalar boundary reads retain its imported
    /// qualification rather than assigning it a producing invocation.
    FileCached(FileCached),
    /// The calculation refused the formula; it has no value.
    NotModeled {
        refusal: NotModeled,
        calculation: Calculation,
    },
}

impl FormulaResult {
    /// Keep a historical display value without carrying a reusable read certificate.
    pub(crate) fn without_stamp(mut self)->Self {
        match &mut self {Self::Calculated{calculation,..}|Self::NotModeled{calculation,..}=>calculation.stamp=None,Self::FileCached(_)=>{}}
        self
    }
    pub fn stamp(&self) -> Option<&crate::contract::ResultStamp> {
        match self {
            Self::Calculated { calculation, .. } | Self::NotModeled { calculation, .. } => {
                calculation.stamp()
            }
            Self::FileCached(_) => None,
        }
    }
    pub(crate) fn rebase_occupancy(&mut self, occupancy: impl Fn(GridArea) -> u64) {
        match self {
            Self::Calculated { calculation, .. } | Self::NotModeled { calculation, .. } => {
                if let Some(stamp) = calculation
                    .stamp
                    .as_mut()
                    .filter(|stamp| !stamp.reads.absences.is_empty())
                {
                    for proof in &mut Arc::make_mut(stamp).reads.absences {
                        // Publication has validated every exact blank run against the final
                        // producer cut. Candidate-time uncertainty is now discharged.
                        proof.occupancy = occupancy(proof.region);
                        if proof.cached.is_none() {
                            proof.pending_producers = 0;
                        }
                    }
                }
            }
            Self::FileCached(_) => {}
        }
    }
    pub(crate) fn set_stamp(&mut self, stamp: crate::contract::ResultStamp) {
        match self {
            Self::Calculated { calculation, .. } | Self::NotModeled { calculation, .. } => {
                calculation.set_stamp(stamp)
            }
            Self::FileCached(_) => {}
        }
    }

    /// The value, when this engine calculated it.
    pub fn calculated(&self) -> Option<(&CalcValue, &Calculation)> {
        match self {
            FormulaResult::Calculated { value, calculation } => Some((value, calculation)),
            FormulaResult::FileCached(_) | FormulaResult::NotModeled { .. } => None,
        }
    }
}

/// One change a finished recalculation published, at a formula cell.
#[derive(Clone, Debug, PartialEq)]
pub enum ValueChange {
    /// The node at `at` was evaluated, and [`crate::EngineState::result`] gives its result now
    /// published (dc-dxm: the delta names the cell, and no longer holds a copy of every result).
    Published { at: CellKey, node: NodeId },
    /// The formula at `at` was removed by an edit committed before the recalculation; its result
    /// is no longer published.
    Removed { at: CellKey, node: NodeId },
}

/// A change to the cells an array formula's result fills beside its anchor, which a finished
/// recalculation published: the area its published spill filled before and fills after (`None`
/// when it filled or fills only its anchor, was blocked, or was refused).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpillChange {
    pub anchor: CellKey,
    pub before: Option<GridArea>,
    pub after: Option<GridArea>,
}

/// What one finished recalculation published, in the order of the cells (sheet ID, then row,
/// then column).
#[derive(Clone, Debug, PartialEq)]
pub struct ValueDelta {
    revision: AuthoredRevision,
    tick: CalcTick,
    changes: Vec<ValueChange>,
    spill_changes: Vec<SpillChange>,
    completion: Option<crate::contract::CompletionReceipt>,
    events: Vec<crate::contract::CalcEvent>,
}

impl ValueDelta {
    pub(crate) fn new(
        revision: AuthoredRevision,
        tick: CalcTick,
        changes: Vec<ValueChange>,
    ) -> Self {
        ValueDelta {
            revision,
            tick,
            changes,
            spill_changes: Vec::new(),
            completion: None,
            events: Vec::new(),
        }
    }

    pub(crate) fn with_events(mut self, events: Vec<crate::contract::CalcEvent>) -> Self {
        self.events = events;
        self
    }
    pub fn events(&self) -> &[crate::contract::CalcEvent] {
        &self.events
    }

    pub(crate) fn with_completion(
        mut self,
        completion: crate::contract::CompletionReceipt,
    ) -> Self {
        self.completion = Some(completion);
        self
    }
    /// The unit installed by this delta, including a nonterminal scalar publication.
    pub fn publication(&self) -> Option<crate::contract::PublicationUnitId> {
        self.events.iter().find_map(|event| match event {
            crate::contract::CalcEvent::Published(unit) => Some(*unit),
            _ => None,
        })
    }
    pub fn completion(&self) -> Option<&crate::contract::CompletionReceipt> {
        self.completion.as_ref()
    }

    pub(crate) fn with_spill_changes(self, spill_changes: Vec<SpillChange>) -> Self {
        ValueDelta {
            spill_changes,
            ..self
        }
    }

    /// The anchors whose published spill areas changed, in anchor order. A cell a spill gained or
    /// lost changed its value though it holds no formula.
    pub fn spill_changes(&self) -> &[SpillChange] {
        &self.spill_changes
    }

    /// The authored revision the recalculation calculated.
    pub fn revision(&self) -> AuthoredRevision {
        self.revision
    }

    pub fn tick(&self) -> CalcTick {
        self.tick
    }

    pub fn changes(&self) -> &[ValueChange] {
        &self.changes
    }
}

/// [`crate::EngineState::with_file_caches`] was given a cached result for a cell that holds no
/// formula in the snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CacheWithoutFormula {
    pub at: CellKey,
}

impl fmt::Display for CacheWithoutFormula {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "a cached result was given for {:?}, which holds no formula",
            self.at
        )
    }
}

impl std::error::Error for CacheWithoutFormula {}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_value::{CalcArray, WorksheetErrorCode};

    /// Cells hold finite numbers, qualified Excel rounding encodings, texts, logicals and errors.
    /// An ordinary NaN with the same encoding as a qualified result is still refused.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn only_scalar_values_are_published() {
        for held in [
            CalcValue::number(2.5),
            CalcValue::excel_overflow_number(f64::from_bits(0x7ff1_ccf3_85eb_c8a0)).unwrap(),
            CalcValue::logical(true),
            CalcValue::error(WorksheetErrorCode::Div0),
            CalcValue::text(calc_value::ExcelText::from_interop_assignment("a")),
        ] {
            assert_eq!(NotModeled::publishable(held.clone()), Ok(held));
        }
        let array = CalcValue::array(
            CalcArray::from_rows(vec![vec![CalcValue::number(1.0), CalcValue::number(2.0)]])
                .expect("an array"),
        );
        for refused in [
            CalcValue::number(f64::INFINITY),
            CalcValue::number(f64::NAN),
            CalcValue::number(f64::from_bits(0x7ff1_ccf3_85eb_c8a0)),
            CalcValue::number(f64::from_bits(0xfff0_0000_0000_000a)),
            CalcValue::empty(),
            CalcValue::missing(),
            array,
        ] {
            assert!(matches!(
                NotModeled::publishable(refused),
                Err(NotModeled::Result(_))
            ));
        }
    }
}
