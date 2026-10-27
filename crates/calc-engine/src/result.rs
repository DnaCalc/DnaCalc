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
}

impl Calculation {
    pub(crate) fn new(revision: AuthoredRevision, tick: CalcTick, mode: NumericMode) -> Self {
        Calculation {
            revision,
            tick,
            mode: Arc::new(mode),
        }
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
    /// The evaluation returned a value a cell cannot hold as calculated here: a callable, a
    /// reference, an empty or omitted value, or a number that is not finite (an array result is
    /// shown by its anchor and spills, `spill.rs`).
    Result(CalcValue),
}

impl NotModeled {
    /// The calculated value `value`, or the refusal of a value a cell cannot hold as calculated
    /// here.
    pub(crate) fn publishable(value: CalcValue) -> Result<CalcValue, NotModeled> {
        let held = value.callable_value().is_none()
            && match value.core_ref() {
                CoreRef::Number(number) => number.is_finite(),
                CoreRef::Text(_) | CoreRef::Logical(_) | CoreRef::Error(_) => true,
                CoreRef::Empty
                | CoreRef::Missing
                | CoreRef::Array(_)
                | CoreRef::Reference(_) => false,
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
    /// calculated, and is read only as the value an iterating formula on a circular reference
    /// starts from (dc-10y).
    FileCached(FileCached),
    /// The calculation refused the formula; it has no value.
    NotModeled {
        refusal: NotModeled,
        calculation: Calculation,
    },
}

impl FormulaResult {
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
        }
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

    /// The values a cell holds as calculated are finite numbers, texts, logicals and errors; every
    /// other value the evaluator could return is refused, not published.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn only_scalar_values_are_published() {
        for held in [
            CalcValue::number(2.5),
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
