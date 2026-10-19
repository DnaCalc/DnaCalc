//! Replaying a capture through the one verdict path, and the typed answer for a capture that has
//! not been taken.
//!
//! A capture artifact is committed at a fixed path so that every platform judges the same bytes.
//! Until the owner's Excel run takes the capture, that path holds the absent-capture marker
//! instead, which [`MissingCapture::recognize`] reads, so a replay reports [`MissingCapture`]
//! rather than passing or failing on bytes nobody captured:
//!
//! ```text
//! {"format": "dnacalc-capture-absent", "version": 1, "case": "<case id>",
//!  "artifact": "<what the path will hold>", "fixture": "<the workbook to capture>",
//!  "command": "<the command that writes the path>"}
//! ```
//!
//! [`file_cache_replay`] judges a version 2 live capture against the file-cache reading of the
//! package Excel saved during it.

use crate::capture::{Acquisition, CaptureRecord, Observation};
use crate::evaluations::EvaluationRecord;
use crate::json::{self, Fields};
use crate::result::{CellResult, Provenance, ResultSet, RunIdentity};
use crate::verdict::{VerdictPolicy, VerdictReport, judge};
use crate::{DecodeError, JudgeError};
use std::fmt;

pub const MISSING_CAPTURE_FORMAT: &str = "dnacalc-capture-absent";
pub const MISSING_CAPTURE_VERSION: u64 = 1;

/// A capture artifact path that holds no capture yet: the case it is for, what it will hold, the
/// workbook to capture and the command that writes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingCapture {
    pub case: String,
    pub artifact: String,
    pub fixture: String,
    pub command: String,
}

impl MissingCapture {
    /// The marker in `bytes`, or `None` when they are not the absent-capture marker: a capture
    /// record or a report, which their own decoders read, or a malformed marker, which those
    /// decoders then refuse.
    pub fn recognize(bytes: &[u8]) -> Option<MissingCapture> {
        Self::decode(bytes).ok()
    }

    fn decode(bytes: &[u8]) -> Result<MissingCapture, DecodeError> {
        let root = json::parse(bytes, "marker")?;
        let fields = Fields::new(
            &root,
            "marker",
            &[
                "format", "version", "case", "artifact", "fixture", "command",
            ],
        )?;
        crate::capture::check_format(&fields, MISSING_CAPTURE_FORMAT, MISSING_CAPTURE_VERSION)?;
        let text = |name: &str| -> Result<String, DecodeError> {
            Ok(json::name(fields.required(name)?, &fields.at(name))?.to_owned())
        };
        Ok(MissingCapture {
            case: text("case")?,
            artifact: text("artifact")?,
            fixture: text("fixture")?,
            command: text("command")?,
        })
    }
}

impl fmt::Display for MissingCapture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "MissingCapture: no capture of {} has been taken for case {}; this path will hold its {} once the owner's Excel run `{}` writes it",
            self.fixture, self.case, self.artifact, self.command
        )
    }
}

/// The file-cache replay of a version 2 live capture: the package Excel saved during the capture,
/// read for its cell caches without any calculation, judged against the capture's raw
/// observations under `policy`.
///
/// Each inventory cell with a saved cache gets a file-cached result whose source is the saved
/// package's hash and whose value is the cache's; a cell without one gets no result and is counted
/// missing. The evaluation record is empty, because nothing was evaluated, and the run is at the
/// capture's own revision and context. So the report counts no cell as calculated, however many
/// caches equal Excel's raw values: those are counted as `not_calculated_equal_to_expectation`.
///
/// Refuses ([`JudgeError::Inconsistent`]) a record that is not a version 2 live capture or that
/// names no saved package, and whatever [`judge`] refuses.
pub fn file_cache_replay(
    capture: &CaptureRecord,
    policy: &VerdictPolicy<'_>,
) -> Result<VerdictReport, JudgeError> {
    if capture.acquisition != Acquisition::LiveCapture {
        return Err(JudgeError::new(format!(
            "the capture record of case {:?} is not a live capture, so it saved no package to replay",
            capture.case
        )));
    }
    let Some(session) = &capture.session else {
        return Err(JudgeError::new(format!(
            "the capture record of case {:?} is version 1 and records no saved package",
            capture.case
        )));
    };
    let Some(output) = &session.output_package else {
        return Err(JudgeError::new(format!(
            "the capture record of case {:?} names no saved package",
            capture.case
        )));
    };
    let mut results = Vec::with_capacity(session.saved.len());
    for saved in &session.saved {
        let Observation::ExcelSavedCache(cache) = &saved.observation else {
            return Err(JudgeError::new(format!(
                "the saved entry of {}!{} is not a saved cache",
                saved.cell.sheet(),
                saved.cell.a1()
            )));
        };
        results.push(CellResult {
            cell: saved.cell.clone(),
            provenance: Provenance::FileCached {
                source: output.clone(),
            },
            value: Some(cache.value().clone()),
        });
    }
    let results = ResultSet {
        case: capture.case.clone(),
        source: capture.source.clone(),
        run: RunIdentity {
            revision: session.revision,
            context: session.context.clone(),
        },
        results,
    };
    let evaluations = EvaluationRecord {
        case: capture.case.clone(),
        source: capture.source.clone(),
        evaluations: Vec::new(),
    };
    judge(capture, &results, &evaluations, policy)
}
