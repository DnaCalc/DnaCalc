//! The one verdict path (plan section 10.1): the canonical CaptureRecord, the result set, the
//! evaluation record, and the provenance-first CellVerdict report computed from them. A file-cache
//! replay, a live Excel capture and the product's own conformance runs all hand this library a
//! [`CaptureRecord`], a [`ResultSet`] and an evaluation record; [`judge`] is the only place a
//! verdict is computed.
//!
//! - [`CaptureRecord`]: what Excel was observed to produce, with the origin of each observation
//!   kept distinct ([`Observation::ExcelRaw`] raw `Value2` bits, [`Observation::ExcelSavedCache`]
//!   saved `<v>` text, [`Observation::UnknownFreshnessCache`]), the Excel environment, and the
//!   independently enumerated formula inventory that is the denominator.
//! - [`ResultSet`]: each published value with its [`Provenance`].
//! - [`InstrumentRecord`] (version 2) and [`EvaluationRecord`] (version 1): the evaluator calls
//!   an instrument observed during the run, a document of its own that the publisher of results
//!   does not write. A version 2 record also carries what each evaluation returned; version 1
//!   records are authored, by fixtures and by other packages' cases.
//! - [`judge`]: one [`CellVerdict`] per inventory cell. Only a result calculated by this run,
//!   with an evaluation in the evaluation record behind it that, in a version 2 record, returned
//!   the value published, counts as calculated, and only a calculated result can be matched.
//!   Strict-bit certainty is marked per comparison ([`Certainty`]).
//! - [`VerdictReport::encode`]: the report as JSON, headline first.
//! - [`EvidenceLabels`]: the acquisition labels of a legacy evidence record, which decide what a
//!   record converted from that evidence may certify.
//! - [`CaptureSession`]: what a version 2 CaptureRecord adds about how a live capture was taken
//!   (authored revision and operations, the [`CalculationContext`] fingerprint, the capture
//!   tool's revision, package hashes, failures, shared-formula ranges and the save and reopen
//!   round trip). [`CaptureRecord::encode`] writes version 2; the decoder reads versions 1 and 2.
//! - [`file_cache_replay`]: a version 2 live capture judged against the file-cache reading of the
//!   package Excel saved during it, and [`MissingCapture`], the typed answer for a capture
//!   artifact path whose capture has not been taken.
//! - [`WitnessSet`] and [`Severity`]: Excel's readings of one function on given arguments (OxFunc's
//!   WitnessSet rows, with the environment of each reading), and OxFunc's ODR-FN-005 classing of a
//!   calculated result against one, which `cargo xtask witness judge` and calc-func's pinned
//!   witness test both use.
//! - [`workbook_sections`]: an oracle workbook's worksheets as sections, each keyed by what
//!   decides its results, and [`section_record`] and [`merged_record`], which split a capture into
//!   section captures and judge several together (`section.rs`).
//! - [`sha256_hex`]: the content hash a version 2 record names its packages and its capture
//!   worker by.
//!
//! Records are JSON with numbers as binary64 bits in hex. The library reads and writes bytes and
//! strings only; files are the caller's (`cargo xtask verdict` and `cargo xtask capture` are the
//! repository commands).

mod capture;
mod capture_failures;
pub use capture_failures::{CaptureFailureClass, CaptureFailureEvidence};
#[cfg(test)]
mod capture_v2_tests;
mod evaluations;
mod evidence;
mod json;
mod replay;
mod result;
mod section;
mod session;
mod sha256;
mod typed;
mod verdict;
mod witness;

/// The value model the comparisons are typed over, and the numeric mode that selects an Exact
/// target.
pub use calc_value as value;

pub use capture::{
    Acquisition, CAPTURE_FORMAT, CAPTURE_VERSION, CacheCellType, CaptureRecord, CellAddress,
    CellObservation, EncodeError, ExcelEnvironment, FormulaCell, FormulaKind, Observation,
    SETTING_COM_ADD_INS_CONNECTED, SavedCache, SheetEntry, SheetVisibility,
};
pub use evaluations::{
    EVALUATION_FORMAT, EVALUATION_VERSION, Evaluation, EvaluationProof, EvaluationRecord,
    InstrumentRecord, InstrumentedEvaluation, evaluation_record_version,
};
pub use evidence::{EVIDENCE_RECORD_FORMAT, EvidenceLabels};
pub use replay::{
    MISSING_CAPTURE_FORMAT, MISSING_CAPTURE_VERSION, MissingCapture, file_cache_replay,
};
pub use result::{CellResult, Provenance, RESULT_FORMAT, RESULT_VERSION, ResultSet, RunIdentity};
pub use section::{
    SECTION_KEY_PREFIX, WorkbookSection, merged_record, section_record, stored_package_parts,
    workbook_sections,
};
pub use session::{
    CalculationContext, CaptureFailure, CaptureSession, STAGE_ENVIRONMENT, STAGE_INVENTORY,
    STAGE_ISOLATION, SharedRange,
};
pub use sha256::{Sha256, sha256_hex};
pub use typed::{MismatchClass, compare};
pub use verdict::{
    CaptureFailureCounts, CellVerdict, Certainty, Counts, OutsideResult, REPORT_FORMAT,
    REPORT_VERSION, StaleSource, Strata, UncertifiedReason, Verdict, VerdictPolicy, VerdictReport,
    judge,
};
pub use witness::{
    ReadingSource, Severity, WITNESS_SET_FORMAT, WitnessInvocation, WitnessRow, WitnessSet,
    decode_argument, decode_result, encode_result, result_index, select_result,
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
