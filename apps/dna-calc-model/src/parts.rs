//! Parts of a formula evaluated on their own, as they travel between a page and the engine that
//! calculates them: F9 on a selected part and the evaluation trace (dc-wr9 for DNA OneCalc,
//! dc-0f1k.7 for DNA Calc). The evaluation itself is `dna_formula_ui::parts`, over whichever
//! engine the owner has.

use serde::{Deserialize, Serialize};

/// The most steps a trace lists; a longer formula says so.
pub const MAX_TRACE_STEPS: usize = 120;

/// The value of a selected part: the text that replaces it, and the value as the surface shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PartValue {
    /// Formula text that gives the value back (`3`, `"ab"`, `{1,2;3,4}`).
    pub literal: String,
    /// The value with its type (`number 3`).
    pub shown: String,
}

/// One step of an evaluation trace: a part of the formula and its value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceStep {
    /// The part's span in the formula text, in UTF-16 code units.
    pub start: usize,
    pub len: usize,
    /// How deep the part sits in the formula's tree (the whole formula is 0).
    pub depth: usize,
    pub text: String,
    /// The value's type (`number`, `text`, ...), `error` for a worksheet error, or `none` when the
    /// part has no value of its own.
    pub kind: String,
    /// The value as a cell shows it, or why there is none.
    pub value: String,
}

/// The steps of a formula's evaluation, innermost parts first.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Trace {
    pub steps: Vec<TraceStep>,
    /// What the trace is and is not, shown beside it.
    pub note: String,
}
