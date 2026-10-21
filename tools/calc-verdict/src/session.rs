//! What a version 2 CaptureRecord adds about how the capture was taken (plan section 10.2): the
//! authored revision and operations of the workbook observed, the fingerprint of the calculation
//! context it was calculated under, the capture tool's revision, the content hashes of the package
//! opened and of the package Excel saved, what went wrong, each shared-formula range's ref, and
//! the round trip: each inventory cell's cache in the saved package and the raw `Value2` Excel read
//! back after reopening it. The JSON form is in `capture.rs`.

use crate::DecodeError;
use crate::capture::{CellAddress, CellObservation, a1_text, parse_a1};
use crate::json::{self, Fields, Node, Out};

/// How a version 2 capture was taken.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptureSession {
    /// The authored revision of the workbook the capture observed: 0 for a fixture as authored,
    /// and for an edited workbook the revision its operations brought it to.
    pub revision: u64,
    /// The authored operations that produced the observed workbook from the source workbook, in
    /// order, each as its operation text (none for a fixture as authored). They are carried, not
    /// interpreted: the operation vocabulary is CalcHost's.
    pub operations: Vec<String>,
    /// The fingerprint of the calculation context the capture calculated under
    /// ([`CalculationContext::fingerprint`]).
    pub context: String,
    /// The revision of the capture tool (adapter and worker) that took the capture.
    pub tool: String,
    /// The content hash of the package the capture opened (the record's `source`).
    pub input_package: String,
    /// The content hash of the package Excel saved, or `None` when none was saved.
    pub output_package: Option<String>,
    /// What went wrong during the capture, in order. A record with failures still records what
    /// was observed; each failure says what was not. Failures at [`STAGE_ISOLATION`] or
    /// [`STAGE_ENVIRONMENT`] leave every comparison uncertified, and one at [`STAGE_INVENTORY`]
    /// makes [`crate::judge`] refuse the record.
    pub failures: Vec<CaptureFailure>,
    /// Each shared-formula range of the package, with the ref the package declares for it.
    pub shared_ranges: Vec<SharedRange>,
    /// The saved cache of each inventory cell in the package Excel saved (each observation is an
    /// [`crate::Observation::ExcelSavedCache`]).
    pub saved: Vec<CellObservation>,
    /// The raw `Value2` of each inventory cell after Excel reopened the package it saved (each
    /// observation is an [`crate::Observation::ExcelRaw`]).
    pub reopened: Vec<CellObservation>,
}

/// One thing that went wrong during a capture: the stage it happened in (such as
/// [`STAGE_ISOLATION`], [`STAGE_ENVIRONMENT`], [`STAGE_INVENTORY`], `value`, `save` or `reopen`),
/// the cell it concerns when there is one, and what happened.
///
/// Three stages decide how [`crate::judge`] treats the whole record; every other stage concerns
/// what its message names (a cell's value, its saved cache or its value after reopening) and is
/// counted in the report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureFailure {
    pub stage: String,
    pub cell: Option<CellAddress>,
    pub message: String,
}

/// The stage of a failure of the capture's isolation: the Excel instance was not the isolated,
/// fresh instance the capture requires (an add-in still loaded or of unreadable state, a workbook
/// with a refresh-on-open connection or an OLE or DDE link, a reported Excel process the adapter
/// refused to own). No comparison against a record with such a failure is strict-bit certified,
/// and `cargo xtask capture` refuses the capture.
pub const STAGE_ISOLATION: &str = "isolation";
/// The stage of a failure to read the environment the capture ran in (a setting, the CPU, a
/// language or the workbook's compatibility). No comparison against a record with such a failure
/// is strict-bit certified.
pub const STAGE_ENVIRONMENT: &str = "environment";
/// The stage of a failure to establish the formula inventory (a sheet whose cells are not read,
/// a cell the package and Excel disagree on, a formula cross-check that could not run). The
/// inventory is the denominator, so [`crate::judge`] refuses a record with such a failure.
pub const STAGE_INVENTORY: &str = "inventory";

impl CaptureSession {
    /// The failures recorded at `stage`, in order.
    pub fn failures_at<'a>(&'a self, stage: &'a str) -> impl Iterator<Item = &'a CaptureFailure> {
        self.failures
            .iter()
            .filter(move |failure| failure.stage == stage)
    }
}

/// A shared-formula range: shared group `group` (the package's `si`, scoped to its sheet) on
/// `sheet`, and the ref its master declares. Every cell of the ref is a cell of the group and every
/// cell of the group lies in the ref; [`crate::judge`] refuses a record where either fails.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedRange {
    sheet: String,
    group: u64,
    reference: String,
    first: (u32, u32),
    last: (u32, u32),
}

impl SharedRange {
    /// The range of group `group` on `sheet` with ref `reference`: one relative A1 cell (`C1`) or
    /// two (`C1:C3`), the first neither below nor right of the last. `None` when the sheet name is
    /// empty or the ref is not of that form within Excel's grid.
    pub fn new(sheet: &str, group: u64, reference: &str) -> Option<SharedRange> {
        if sheet.is_empty() {
            return None;
        }
        let (first, last) = match reference.split_once(':') {
            Some((first, last)) => (parse_a1(first)?, parse_a1(last)?),
            None => {
                let cell = parse_a1(reference)?;
                (cell, cell)
            }
        };
        if first.0 > last.0 || first.1 > last.1 {
            return None;
        }
        Some(SharedRange {
            sheet: sheet.to_owned(),
            group,
            reference: reference.to_owned(),
            first,
            last,
        })
    }

    pub fn sheet(&self) -> &str {
        &self.sheet
    }

    pub fn group(&self) -> u64 {
        self.group
    }

    /// The ref as the record holds it.
    pub fn reference(&self) -> &str {
        &self.reference
    }

    /// Whether `cell` lies in the range (on its sheet).
    pub fn contains(&self, cell: &CellAddress) -> bool {
        cell.sheet() == self.sheet
            && (self.first.0..=self.last.0).contains(&cell.row())
            && (self.first.1..=self.last.1).contains(&cell.column())
    }

    /// The number of cells in the range.
    pub fn cell_count(&self) -> u64 {
        u64::from(self.last.0 - self.first.0 + 1) * u64::from(self.last.1 - self.first.1 + 1)
    }

    /// The cells of the range in row-major order.
    pub fn cells(&self) -> impl Iterator<Item = CellAddress> + '_ {
        (self.first.0..=self.last.0).flat_map(move |row| {
            (self.first.1..=self.last.1)
                .filter_map(move |column| CellAddress::new(&self.sheet, row, column))
        })
    }

    /// The first cell's A1 form, for messages.
    pub(crate) fn first_a1(&self) -> String {
        a1_text(self.first.0, self.first.1)
    }

    pub(crate) fn out(&self) -> Out {
        Out::Object(vec![
            ("sheet", Out::text(self.sheet.as_str())),
            ("group", Out::Unsigned(self.group)),
            ("ref", Out::text(self.reference.as_str())),
        ])
    }
}

/// The settings a full recalculation's values depend on beyond the workbook's cells and the Excel
/// that ran it: the date system, iterative calculation and precision as displayed. Its fingerprint
/// is the `context` a version 2 capture records; a result set must name the same fingerprint as
/// its run's context for its calculated values to be judged against that capture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalculationContext {
    /// The workbook uses the 1904 date system.
    pub date1904: bool,
    /// Iterative calculation is enabled.
    pub iteration: bool,
    pub max_iterations: u32,
    pub max_change: f64,
    pub precision_as_displayed: bool,
}

impl CalculationContext {
    /// Excel's default iteration limits: 100 iterations and a maximum change of 0.001.
    const DEFAULT_LIMITS: (u32, f64) = (100, 0.001);

    /// A context without iterative calculation. Its limits are Excel's defaults, which play no
    /// part in any value while iteration is off.
    pub const fn without_iteration(date1904: bool, precision_as_displayed: bool) -> Self {
        CalculationContext {
            date1904,
            iteration: false,
            max_iterations: Self::DEFAULT_LIMITS.0,
            max_change: Self::DEFAULT_LIMITS.1,
            precision_as_displayed,
        }
    }

    /// The fingerprint, one line naming every setting (`max_change` as its binary64 bits):
    /// `excel-calc-context/1;date1904=false;iteration=false;max_iterations=100;`
    /// `max_change=3f50624dd2f1a9fc;precision_as_displayed=false`. While iteration is off the
    /// limits change no value, so the fingerprint names Excel's defaults whatever the context
    /// holds: a capture taken with iteration off and a run that applies no iteration name the
    /// same context.
    pub fn fingerprint(&self) -> String {
        let (max_iterations, max_change) = if self.iteration {
            (self.max_iterations, self.max_change)
        } else {
            Self::DEFAULT_LIMITS
        };
        format!(
            "excel-calc-context/1;date1904={};iteration={};max_iterations={};max_change={:016x};precision_as_displayed={}",
            self.date1904,
            self.iteration,
            max_iterations,
            max_change.to_bits(),
            self.precision_as_displayed
        )
    }
}

fn decode_failure(node: &Node, path: &str) -> Result<CaptureFailure, DecodeError> {
    let fields = Fields::new(node, path, &["stage", "sheet", "row", "column", "message"])?;
    let located = ["sheet", "row", "column"]
        .iter()
        .any(|name| fields.optional(name).is_some());
    Ok(CaptureFailure {
        stage: json::name(fields.required("stage")?, &fields.at("stage"))?.to_owned(),
        cell: located.then(|| CellAddress::decode(&fields)).transpose()?,
        message: json::name(fields.required("message")?, &fields.at("message"))?.to_owned(),
    })
}

impl CaptureFailure {
    fn out(&self) -> Out {
        let mut members = vec![("stage", Out::text(self.stage.as_str()))];
        if let Some(cell) = &self.cell {
            members.extend(cell.out_members());
        }
        members.push(("message", Out::text(self.message.as_str())));
        Out::Object(members)
    }
}

impl CaptureSession {
    /// The session of a version 2 record: its `session` member and its `shared_ranges`, `saved`
    /// and `reopened` lists.
    pub(crate) fn decode(record: &Fields<'_>) -> Result<CaptureSession, DecodeError> {
        let session_at = record.at("session");
        let fields = Fields::new(
            record.required("session")?,
            &session_at,
            &[
                "revision",
                "operations",
                "context",
                "tool",
                "packages",
                "failures",
            ],
        )?;
        let operations_at = fields.at("operations");
        let operations = json::array(fields.required("operations")?, &operations_at)?
            .iter()
            .enumerate()
            .map(|(i, node)| json::name(node, &format!("{operations_at}[{i}]")).map(str::to_owned))
            .collect::<Result<Vec<_>, DecodeError>>()?;
        let packages_at = fields.at("packages");
        let packages = Fields::new(
            fields.required("packages")?,
            &packages_at,
            &["input", "output"],
        )?;
        let output_package = match packages.required("output")? {
            Node::Null => None,
            node => Some(json::name(node, &packages.at("output"))?.to_owned()),
        };
        let failures_at = fields.at("failures");
        let failures = json::array(fields.required("failures")?, &failures_at)?
            .iter()
            .enumerate()
            .map(|(i, node)| decode_failure(node, &format!("{failures_at}[{i}]")))
            .collect::<Result<Vec<_>, DecodeError>>()?;

        let ranges_at = record.at("shared_ranges");
        let shared_ranges = json::array(record.required("shared_ranges")?, &ranges_at)?
            .iter()
            .enumerate()
            .map(|(i, node)| {
                let path = format!("{ranges_at}[{i}]");
                let range = Fields::new(node, &path, &["sheet", "group", "ref"])?;
                let sheet = json::name(range.required("sheet")?, &range.at("sheet"))?;
                let group = json::unsigned(range.required("group")?, &range.at("group"))?;
                let reference = json::name(range.required("ref")?, &range.at("ref"))?;
                SharedRange::new(sheet, group, reference).ok_or_else(|| {
                    DecodeError::new(
                        &range.at("ref"),
                        format!(
                            "{reference:?} is not a range of relative A1 cells within Excel's grid, first to last"
                        ),
                    )
                })
            })
            .collect::<Result<Vec<_>, DecodeError>>()?;

        Ok(CaptureSession {
            revision: json::unsigned(fields.required("revision")?, &fields.at("revision"))?,
            operations,
            context: json::name(fields.required("context")?, &fields.at("context"))?.to_owned(),
            tool: json::name(fields.required("tool")?, &fields.at("tool"))?.to_owned(),
            input_package: json::name(packages.required("input")?, &packages.at("input"))?
                .to_owned(),
            output_package,
            failures,
            shared_ranges,
            saved: CellObservation::decode_list(
                record.required("saved")?,
                &record.at("saved"),
                &["excel_saved_cache"],
            )?,
            reopened: CellObservation::decode_list(
                record.required("reopened")?,
                &record.at("reopened"),
                &["excel_raw"],
            )?,
        })
    }

    /// The `session` member (the three lists are written by the record).
    pub(crate) fn out(&self) -> Out {
        Out::Object(vec![
            ("revision", Out::Unsigned(self.revision)),
            (
                "operations",
                Out::Array(
                    self.operations
                        .iter()
                        .map(|operation| Out::text(operation.as_str()))
                        .collect(),
                ),
            ),
            ("context", Out::text(self.context.as_str())),
            ("tool", Out::text(self.tool.as_str())),
            (
                "packages",
                Out::Object(vec![
                    ("input", Out::text(self.input_package.as_str())),
                    (
                        "output",
                        self.output_package
                            .as_ref()
                            .map_or(Out::Null, |hash| Out::text(hash.as_str())),
                    ),
                ]),
            ),
            (
                "failures",
                Out::Array(self.failures.iter().map(CaptureFailure::out).collect()),
            ),
        ])
    }
}
