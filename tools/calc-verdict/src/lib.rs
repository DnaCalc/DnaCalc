//! The one verdict path (plan section 10.1): the canonical CaptureRecord, the result set, the
//! evaluation record, and the provenance-first CellVerdict report computed from them. A file-cache
//! replay, a live Excel capture and the product's own conformance runs all hand this library a
//! [`CaptureRecord`], a [`ResultSet`] and an [`EvaluationRecord`]; [`judge`] is the only place a
//! verdict is computed.
//!
//! - [`CaptureRecord`]: what Excel was observed to produce, with the origin of each observation
//!   kept distinct ([`Observation::ExcelRaw`] raw `Value2` bits, [`Observation::ExcelSavedCache`]
//!   saved `<v>` text, [`Observation::UnknownFreshnessCache`]), the Excel environment, and the
//!   independently enumerated formula inventory that is the denominator.
//! - [`ResultSet`]: each published value with its [`Provenance`].
//! - [`EvaluationRecord`]: the evaluator calls an instrument observed during the run, a document
//!   of its own that the publisher of results does not write.
//! - [`judge`]: one [`CellVerdict`] per inventory cell. Only a result calculated by this run,
//!   with an evaluation in the evaluation record behind it, counts as calculated, and only a
//!   calculated result can be matched. Strict-bit certainty is marked per comparison
//!   ([`Certainty`]).
//! - [`VerdictReport::encode`]: the report as JSON, headline first.
//! - [`EvidenceLabels`]: the acquisition labels of a legacy evidence record, which decide what a
//!   record converted from that evidence may certify.
//!
//! Records are JSON with numbers as binary64 bits in hex. The library reads and writes bytes and
//! strings only; files are the caller's (`cargo xtask verdict` is the repository command).

mod capture;
mod evaluations;
mod evidence;
mod json;
mod result;
mod typed;
mod verdict;

/// The value model the comparisons are typed over, and the numeric mode that selects an Exact
/// target.
pub use calc_value as value;

pub use capture::{
    Acquisition, CAPTURE_FORMAT, CAPTURE_VERSION, CacheCellType, CaptureRecord, CellAddress,
    CellObservation, ExcelEnvironment, FormulaCell, FormulaKind, Observation, SavedCache,
    SheetEntry, SheetVisibility,
};
pub use evaluations::{EVALUATION_FORMAT, EVALUATION_VERSION, Evaluation, EvaluationRecord};
pub use evidence::{EVIDENCE_RECORD_FORMAT, EvidenceLabels};
pub use result::{CellResult, Provenance, RESULT_FORMAT, RESULT_VERSION, ResultSet, RunIdentity};
pub use typed::MismatchClass;
pub use verdict::{
    CellVerdict, Certainty, Counts, OutsideResult, REPORT_FORMAT, REPORT_VERSION, StaleSource,
    Strata, UncertifiedReason, Verdict, VerdictPolicy, VerdictReport, judge,
};

use std::fmt;

/// Why bytes are not a record of the expected format: where (a path such as
/// `capture.formulas[3].row`) and what is wrong there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError {
    pub path: String,
    pub message: String,
}

impl DecodeError {
    pub(crate) fn new(path: &str, message: impl Into<String>) -> DecodeError {
        DecodeError {
            path: path.to_owned(),
            message: message.into(),
        }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for DecodeError {}

/// Why [`judge`] refused its inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JudgeError {
    /// The capture record's formula inventory lists no formula cell. A report over it would state
    /// "calculated 0 of 0", which says nothing about any calculation, so none is written.
    EmptyInventory { case: String },
    /// A capture record, result set or evaluation record inconsistent in itself or with another.
    Inconsistent { message: String },
}

impl JudgeError {
    pub(crate) fn new(message: impl Into<String>) -> JudgeError {
        JudgeError::Inconsistent {
            message: message.into(),
        }
    }
}

impl fmt::Display for JudgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JudgeError::EmptyInventory { case } => write!(
                f,
                "the capture record of case {case:?} lists no formula cell, so there is nothing to judge"
            ),
            JudgeError::Inconsistent { message } => f.write_str(message),
        }
    }
}

impl std::error::Error for JudgeError {}
