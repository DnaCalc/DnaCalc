//! The one verdict computation: one [`CellVerdict`] per formula cell of a CaptureRecord's
//! inventory, provenance first, and the counts every report states.
//!
//! The ladder for each inventory cell, in order:
//!
//! ```text
//! no result for the cell, or provenance missing      -> missing
//! provenance failed                                  -> failed
//! provenance not_modeled                             -> not_modeled
//! provenance file_cached                             -> file_cached
//! provenance stale                                   -> stale (reported)
//! provenance calculated, at another revision         -> stale (revision_not_run)
//!   under another context fingerprint                -> stale (context_not_run)
//!   with no evaluation of the cell at its tick,
//!   revision and context in the evaluation record    -> unproven
//!   with one that, in a version 2 record, returned
//!   no value or another value than the result's      -> unproven (returned)
//!   no observation for the cell                      -> not_run
//!   compared with the observation                    -> matched | mismatched
//! ```
//!
//! "Another revision" and "another context" are measured against the run the result set declares
//! and, for a version 2 capture record, also against the authored revision and the context
//! fingerprint the capture recorded independently: a result set whose run is at revision R, judged
//! against a capture of revision R+1, has every calculated cell stale. A version 1 record carries
//! neither, so there the result set's declaration is all the judge has.
//!
//! Only the last two rungs count a cell as calculated, and only a calculated cell can be matched:
//! a file-cached, stale or unproven value that equals the expectation is still not calculated (the
//! report counts such values separately, as the false passes a value-only comparison would have
//! scored). The proof of calculation is the evaluation record, a separate input meant to come from
//! the evaluator rather than the publisher of results (judge cannot check who wrote it); the result
//! set's own claim proves nothing. A version 2 record, made from the evaluator's own reports
//! (only the dc-qguw acceptance cases write one today), also says what each evaluation returned,
//! and a result whose value is not that value is unproven: the evaluation happened, but the value
//! published is not the one it gave. A version 1 record carries no returned value, so against it
//! a listed evaluation is all the proof there is, whatever value the result publishes
//! ([`crate::EvaluationRecord`]); the report says so, with the record's version and the count of
//! calculated cells proven without a value check (`calculated_without_value_check`). Bead
//! `dc-i1ro` moves the version 1 records to version 2 and refuses version 1. The denominator is the
//! inventory: a cell without a result is missing, never dropped, a result for a cell outside the
//! inventory is counted apart and never enters it, and an empty inventory is refused rather than
//! reported as calculated 0 of 0.
//!
//! A comparison is strict-bit certified only when its observation is a raw Excel observation, an
//! Exact target is selected, the record's Excel environment is that target, and, for a record
//! converted from legacy evidence, the evidence record's label for its entry names that target's
//! build. Every other comparison is uncertified, with its reason.
//!
//! A version 2 record carries the failures of its capture. The report counts them, in total and by
//! stage. A failure of the capture's isolation or of reading its environment leaves every
//! comparison against the record uncertified, with the reason `capture_failure`, whatever the
//! observation and the policy. A failure to establish the formula inventory makes the denominator
//! unknown, so such a record is refused ([`JudgeError::Inconsistent`]), as an empty one is.
//!
//! The cells of a legacy array formula's range ([`FormulaKind::LegacyArrayAnchor`] and
//! [`FormulaKind::LegacyArrayMember`]) are judged like ordinary formula cells, each by its own
//! value; the record is refused when its inventory does not list every cell of each range once,
//! the anchor and each member naming it. The formula is evaluated once, at its anchor, so each cell
//! of the range is proven by the anchor's evaluation: the value the range shows there, as a legacy
//! array fills it, must be the one the cell publishes (dc-nam).
//!
//! A record whose environment lists COM add-ins connected during the capture (the setting
//! [`crate::SETTING_COM_ADD_INS_CONNECTED`]) was taken in an Excel instance that was not isolated
//! from them, and what they did is not observed. The report counts them
//! (`com_add_ins_connected`), and a raw observation of such a record certifies no target: its
//! comparisons are uncertified with the reason `com_add_in_connected` where they would otherwise
//! be strict-bit certified. Bead `dc-quhd` owns capturing without a connected COM add-in.

use crate::JudgeError;
use crate::capture::{
    Acquisition, CaptureRecord, CellAddress, FormulaCell, FormulaKind, Observation,
    SheetVisibility, a1_text,
};
use crate::evaluations::EvaluationProof;
use crate::evidence::EvidenceLabels;
use crate::json::Out;
use crate::result::{CellResult, Provenance, ResultSet, RunIdentity};
use crate::session::{CaptureSession, STAGE_ENVIRONMENT, STAGE_INVENTORY, STAGE_ISOLATION};
use crate::typed::{MismatchClass, compare, value_out};
use crate::value::{CalcValue, CoreValue, MAX_COLUMNS, MAX_ROWS, NumericMode, WorksheetErrorCode};
use std::collections::{BTreeMap, BTreeSet};

pub const REPORT_FORMAT: &str = "dnacalc-verdict-report";
/// Version 2 adds the `captured` and `capture_failures` members a report on a version 2 capture
/// record carries, and the uncertified reason `capture_failure`. Version 3 adds what a report
/// judged against a version 1 evaluation record must show, the weaker proof it rests on: the
/// `evaluation_record_version` member and the count `calculated_without_value_check`, the
/// calculated cells whose evaluation the record lists without what it returned (every calculated
/// cell against a version 1 record, none against a version 2 record); and an unproven cell's
/// `returned` member, present when a version 2 record's evaluation of it returned no value or
/// another value than the result's. Version 4 adds the `com_add_ins_connected` member, the COM
/// add-ins a capture's environment lists as connected while it was taken, and the uncertified
/// reason `com_add_in_connected`. Version 5 adds the strata `legacy_array_anchor` and
/// `legacy_array_member`, and on the cell of a legacy array formula its `ref` (an anchor) or its
/// `anchor` (a member).
pub const REPORT_VERSION: u64 = 5;

/// What a verdict run is judged against: the numeric mode whose Exact target strict-bit certainty
/// needs, and the legacy evidence labels a record converted from legacy evidence needs.
#[derive(Clone, Debug, Default)]
pub struct VerdictPolicy<'a> {
    pub numeric_mode: NumericMode,
    pub evidence: Option<&'a EvidenceLabels>,
}

/// Why a comparison carries no strict-bit certainty.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum UncertifiedReason {
    /// The observation is a saved cache's `<v>` text, which is an observation of what Excel wrote,
    /// not a guaranteed binary64 oracle.
    SavedCache,
    /// The observation is a cache of unknown freshness.
    UnknownFreshness,
    /// The numeric mode is Fast: no Exact target is selected.
    NoExactTarget,
    /// The record names no Excel environment, so its raw observations certify no target.
    EnvironmentUnknown,
    /// The record's Excel environment is not the selected target.
    EnvironmentNotTarget,
    /// The record was converted from legacy evidence and no evidence record was supplied.
    EvidenceRecordAbsent,
    /// The record was converted from legacy evidence whose record has no such entry.
    EvidenceEntryUnknown,
    /// The legacy evidence entry's label does not name the selected target's Excel build number
    /// (by the rule of [`EvidenceLabels`]): a label of an unknown build names none, and a target
    /// whose `excel_build` holds no build number is named by no label.
    EvidenceLabelNotTarget,
    /// The capture record's session records a failure of the capture's isolation or of reading
    /// its environment, so none of its observations certifies any target.
    CaptureFailure,
    /// The record's environment lists a COM add-in connected in the Excel instance that took the
    /// capture ([`crate::SETTING_COM_ADD_INS_CONNECTED`]), so its raw observations certify no
    /// target: what the add-in did during the capture is not observed.
    ComAddInConnected,
}

impl UncertifiedReason {
    pub fn name(self) -> &'static str {
        match self {
            UncertifiedReason::SavedCache => "saved_cache",
            UncertifiedReason::UnknownFreshness => "unknown_freshness",
            UncertifiedReason::NoExactTarget => "no_exact_target",
            UncertifiedReason::EnvironmentUnknown => "environment_unknown",
            UncertifiedReason::EnvironmentNotTarget => "environment_not_target",
            UncertifiedReason::EvidenceRecordAbsent => "evidence_record_absent",
            UncertifiedReason::EvidenceEntryUnknown => "evidence_entry_unknown",
            UncertifiedReason::EvidenceLabelNotTarget => "evidence_label_not_target",
            UncertifiedReason::CaptureFailure => "capture_failure",
            UncertifiedReason::ComAddInConnected => "com_add_in_connected",
        }
    }
}

/// The evidence strength of one comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Certainty {
    /// Compared with a raw observation of the selected Exact target: numbers by their bits.
    StrictBits,
    Uncertified(UncertifiedReason),
}

/// Why a result counts as stale.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StaleSource {
    /// The result's own provenance is stale.
    Reported {
        last_calculated: Option<u64>,
        cause: String,
    },
    /// Calculated from the authored inputs of `revision`, which is not the revision judged: the
    /// run's, and for a version 2 capture record also the revision the capture observed.
    RevisionNotRun { revision: u64 },
    /// Calculated under the context `context`, which is not the context judged: the run's, and
    /// for a version 2 capture record also the context the capture calculated under.
    ContextNotRun { context: String },
}

impl StaleSource {
    fn name(&self) -> &'static str {
        match self {
            StaleSource::Reported { .. } => "reported",
            StaleSource::RevisionNotRun { .. } => "revision_not_run",
            StaleSource::ContextNotRun { .. } => "context_not_run",
        }
    }
}

/// The verdict for one formula cell. Exactly one per inventory cell.
#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    /// Calculated, and equal to the observation.
    Matched {
        certainty: Certainty,
    },
    /// Calculated, and different from the observation.
    Mismatched {
        certainty: Certainty,
        class: MismatchClass,
        expected: CalcValue,
        actual: CalcValue,
    },
    /// Calculated, but nothing was observed for the cell (the capture did not run, or did not
    /// observe it), so it was not compared.
    NotRun,
    /// The result claims calculation at `tick`, but the evaluation record holds no evaluation of
    /// the cell at that tick, revision and context (`returned` is `None`), or, in a version 2
    /// record, holds one that returned no value (`Err`, with why) or another value than the
    /// result publishes (`Ok`, with the value it returned). Not calculated.
    Unproven {
        tick: u64,
        returned: Option<Result<CalcValue, String>>,
    },
    /// Not calculated by this run.
    Stale {
        source: StaleSource,
    },
    /// The file's cached value, not calculated.
    FileCached {
        source: String,
    },
    NotModeled {
        feature: String,
        diagnostic: String,
    },
    Failed {
        fault: String,
    },
    /// No value: no result for the cell (`reason` is `None`) or a result whose provenance is
    /// missing.
    Missing {
        reason: Option<String>,
    },
}

impl Verdict {
    pub fn name(&self) -> &'static str {
        match self {
            Verdict::Matched { .. } => "matched",
            Verdict::Mismatched { .. } => "mismatched",
            Verdict::NotRun => "not_run",
            Verdict::Unproven { .. } => "unproven",
            Verdict::Stale { .. } => "stale",
            Verdict::FileCached { .. } => "file_cached",
            Verdict::NotModeled { .. } => "not_modeled",
            Verdict::Failed { .. } => "failed",
            Verdict::Missing { .. } => "missing",
        }
    }

    /// Whether the cell counts as calculated.
    pub fn is_calculated(&self) -> bool {
        matches!(
            self,
            Verdict::Matched { .. } | Verdict::Mismatched { .. } | Verdict::NotRun
        )
    }
}

/// One inventory cell's verdict with what it was judged from.
#[derive(Clone, Debug, PartialEq)]
pub struct CellVerdict {
    pub formula: FormulaCell,
    pub visibility: SheetVisibility,
    pub observation: Option<Observation>,
    pub verdict: Verdict,
    /// For a result that is not calculated but carries a value (file-cached, stale or unproven),
    /// whether that value equals the observation; `None` when there is no such value or no
    /// observation.
    pub value_equals_expectation: Option<bool>,
    /// The cell is calculated (matched, mismatched or not run) and its result read values another
    /// workbook's link caches hold, directly or through the formulas it read (dc-ylr): the result
    /// set's `external_links`. Such a cell is counted apart ([`Counts::external_links`]).
    pub external_links: bool,
}

/// A result for a cell the inventory does not list (a spill child or a constant cell, say). It is
/// counted apart and never enters the denominator.
#[derive(Clone, Debug, PartialEq)]
pub struct OutsideResult {
    pub cell: CellAddress,
    pub provenance: Provenance,
}

/// The counts of a report. `denominator` is the inventory's size and equals the sum of
/// `calculated`, `unproven`, `stale`, `file_cached`, `not_modeled`, `failed` and `missing`;
/// `calculated` is `matched + mismatched + not_run`; `matched + mismatched` is
/// `strict_certified + uncertified`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub denominator: u64,
    pub calculated: u64,
    pub matched: u64,
    pub mismatched: u64,
    pub not_run: u64,
    pub strict_certified: u64,
    pub uncertified: u64,
    pub unproven: u64,
    pub stale: u64,
    pub file_cached: u64,
    pub not_modeled: u64,
    pub failed: u64,
    pub missing: u64,
    /// Inventory cells not calculated whose value equals the observation: what a value-only
    /// comparison would have scored as passes.
    pub not_calculated_equal_to_expectation: u64,
    pub outside_inventory: u64,
    /// The entries of the evaluation record: the evaluator calls the instrument observed.
    pub evaluator_calls: u64,
    /// Calculated cells whose evaluation the evaluation record lists without what it returned (a
    /// version 1 record), so whose published value no evaluation was checked against: every
    /// calculated cell when the record is version 1, none when it is version 2. Part of
    /// `calculated`, not apart from it.
    pub calculated_without_value_check: u64,
    /// Calculated cells whose result read values another workbook's link caches hold (dc-ylr):
    /// part of `calculated`. A match among them rests on the file's cache of the other book, so
    /// a report states them apart from plain passes.
    pub external_links: u64,
    /// The matched cells among `external_links`: part of `matched`.
    pub matched_external_links: u64,
}

/// How the denominator divides by sheet visibility, by formula kind and by what was observed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Strata {
    pub visible_sheet: u64,
    pub hidden_sheet: u64,
    pub very_hidden_sheet: u64,
    pub ordinary: u64,
    pub shared_master: u64,
    pub shared_child: u64,
    /// The anchors of legacy array formulas.
    pub legacy_array_anchor: u64,
    /// The cells of legacy array formulas' ranges other than their anchors.
    pub legacy_array_member: u64,
    /// The top-left cells of what-if data tables' result areas (dc-10y).
    pub data_table_anchor: u64,
    /// The other cells of data tables' result areas.
    pub data_table_member: u64,
    /// Inventory cells with an observation.
    pub observed: u64,
    /// Inventory cells whose observation is a worksheet error value.
    pub error_expected: u64,
}

/// The failures a version 2 capture record's session records: their number, and their number at
/// each stage.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CaptureFailureCounts {
    pub total: u64,
    pub by_stage: BTreeMap<String, u64>,
}

/// The CellVerdict report of one verdict run.
#[derive(Clone, Debug, PartialEq)]
pub struct VerdictReport {
    pub case: String,
    pub source: String,
    pub acquisition: Acquisition,
    pub numeric_mode: NumericMode,
    pub run: RunIdentity,
    /// The authored revision and context fingerprint a version 2 capture record observed, which
    /// the run was judged against; `None` for a version 1 record.
    pub captured: Option<RunIdentity>,
    /// The failures a version 2 capture record's session records, counted; `None` for a version 1
    /// record, which records none.
    pub capture_failures: Option<CaptureFailureCounts>,
    /// The COM add-ins the capture record's environment lists as connected while the capture was
    /// taken ([`crate::SETTING_COM_ADD_INS_CONNECTED`]); `None` when the record carries no
    /// environment or its environment no such setting.
    pub com_add_ins_connected: Option<u64>,
    /// The version of the evaluation record judged against: 2 when each evaluation carries what it
    /// returned, 1 when it does not ([`Counts::calculated_without_value_check`]).
    pub evaluation_record_version: u64,
    pub counts: Counts,
    pub strata: Strata,
    pub cells: Vec<CellVerdict>,
    pub outside_inventory: Vec<OutsideResult>,
}

/// The certainty of a comparison with a raw observation of this record under this policy.
fn raw_certainty(capture: &CaptureRecord, policy: &VerdictPolicy<'_>) -> Certainty {
    let uncertified = Certainty::Uncertified;
    let Some(target) = policy.numeric_mode.target() else {
        return uncertified(UncertifiedReason::NoExactTarget);
    };
    let Some(environment) = &capture.environment else {
        return uncertified(UncertifiedReason::EnvironmentUnknown);
    };
    if environment.target != *target {
        return uncertified(UncertifiedReason::EnvironmentNotTarget);
    }
    if let Acquisition::LegacyEvidence { entry } = &capture.acquisition {
        let Some(labels) = policy.evidence else {
            return uncertified(UncertifiedReason::EvidenceRecordAbsent);
        };
        match labels.names_build(entry, target.excel_build()) {
            None => return uncertified(UncertifiedReason::EvidenceEntryUnknown),
            Some(false) => return uncertified(UncertifiedReason::EvidenceLabelNotTarget),
            Some(true) => {}
        }
    }
    if environment
        .com_add_ins_connected()
        .is_some_and(|connected| connected > 0)
    {
        return uncertified(UncertifiedReason::ComAddInConnected);
    }
    Certainty::StrictBits
}

/// The certainty of a comparison with `observation`, given the certainty a raw observation has
/// (`raw`) and whether the capture recorded a failure of its isolation or environment.
fn certainty_of(observation: &Observation, raw: Certainty, capture_failed: bool) -> Certainty {
    if capture_failed {
        return Certainty::Uncertified(UncertifiedReason::CaptureFailure);
    }
    match observation {
        Observation::ExcelRaw(_) => raw,
        Observation::ExcelSavedCache(_) => Certainty::Uncertified(UncertifiedReason::SavedCache),
        Observation::UnknownFreshnessCache(_) => {
            Certainty::Uncertified(UncertifiedReason::UnknownFreshness)
        }
    }
}

fn place(cell: &CellAddress) -> String {
    format!("{}!{}", cell.sheet(), cell.a1())
}

/// Checks a version 2 record's shared ranges against its inventory: each range on a listed sheet,
/// one range per shared group and one group per range, and every cell of a group inside its
/// range, so a group with a member outside its ref is refused with that member named.
///
/// A range may cover cells that are not of its group. Packages saved by Excel hold such refs:
/// desktop Excel 16.0.20430 saves a fill of B1:B500 as 64-row groups, the last with ref B449:B512
/// over twelve empty cells, and the three Ofgem price-cap packages, saved by Excel for the web,
/// hold refs that span cells of other shared groups, ordinary formulas, constants and empty cells
/// (both measured 2026-10-21). The range therefore says nothing about which of its cells hold
/// formulas, and the record is judged with every formula cell it lists in the denominator, each
/// once. For a live capture, that the list misses no formula cell rests on the capture worker's
/// cross-check against Excel's own `SpecialCells(xlCellTypeFormulas)`: a formula cell Excel
/// lists that the inventory lacks, or a cross-check that cannot run, is a failure at
/// [`STAGE_INVENTORY`], which refuses the record before these checks. A record from any other
/// acquisition has no such cross-check (bead dc-dhlu).
fn check_shared_ranges(
    session: &CaptureSession,
    sheets: &BTreeMap<&str, SheetVisibility>,
    group_cells: &BTreeMap<(&str, u64), BTreeSet<&CellAddress>>,
) -> Result<(), JudgeError> {
    let mut ranges: BTreeSet<(&str, u64)> = BTreeSet::new();
    for range in &session.shared_ranges {
        let (sheet, group, reference) = (range.sheet(), range.group(), range.reference());
        if !sheets.contains_key(sheet) {
            return Err(JudgeError::new(format!(
                "the shared range {reference} of group {group} is on sheet {sheet:?}, which the capture record does not list"
            )));
        }
        if !ranges.insert((sheet, group)) {
            return Err(JudgeError::new(format!(
                "the capture record lists two shared ranges for group {group} on sheet {sheet:?}"
            )));
        }
        let Some(cells) = group_cells.get(&(sheet, group)) else {
            return Err(JudgeError::new(format!(
                "the shared range {sheet}!{reference} names group {group}, to which no formula cell of the inventory belongs"
            )));
        };
        if let Some(outside) = cells.iter().find(|cell| !range.contains(cell)) {
            return Err(JudgeError::new(format!(
                "{} belongs to shared group {group} and lies outside the group's range {sheet}!{reference}",
                place(outside)
            )));
        }
    }
    if let Some((sheet, group)) = group_cells
        .keys()
        .find(|(sheet, group)| !ranges.contains(&(*sheet, *group)))
    {
        return Err(JudgeError::new(format!(
            "shared formula group {group} on sheet {sheet:?} has no shared range in the capture record"
        )));
    }
    Ok(())
}

/// Checks the legacy array formulas of an inventory: each anchor's range lies within Excel's grid
/// and every other cell of it is in the inventory as a member naming that anchor, and each
/// member's anchor is in the inventory as the anchor of a range that covers it. The inventory
/// thus lists every cell of each range once, as Excel lists each among its formula cells.
fn check_legacy_arrays(
    capture: &CaptureRecord,
    kinds: &BTreeMap<&CellAddress, FormulaKind>,
) -> Result<(), JudgeError> {
    for formula in &capture.formulas {
        let cell = &formula.cell;
        let what = match formula.kind {
            FormulaKind::DataTableAnchor { .. } | FormulaKind::DataTableMember { .. } => {
                "data table"
            }
            _ => "legacy array formula",
        };
        match formula.kind {
            FormulaKind::LegacyArrayAnchor { rows, columns }
            | FormulaKind::DataTableAnchor { rows, columns } => {
                let (first, last) = formula
                    .kind
                    .legacy_range(cell)
                    .expect("an anchor has a range");
                if rows == 0 || columns == 0 || last.0 > MAX_ROWS || last.1 > MAX_COLUMNS {
                    return Err(JudgeError::new(format!(
                        "the {what} at {} has a range of {rows} by {columns} cells, which is not a range of Excel's grid starting at the cell",
                        place(cell)
                    )));
                }
                let expected = formula
                    .kind
                    .member_of(first)
                    .expect("an anchor has members");
                for row in first.0..=last.0 {
                    for column in first.1..=last.1 {
                        if (row, column) == first {
                            continue;
                        }
                        let member = CellAddress::new(cell.sheet(), row, column)
                            .expect("a cell of a range within the grid");
                        match kinds.get(&member) {
                            Some(kind) if *kind == expected => {}
                            Some(kind) => {
                                return Err(JudgeError::new(format!(
                                    "{} lies in the range of the {what} at {} and the formula inventory lists it as {}, not as a member of that {}",
                                    place(&member),
                                    place(cell),
                                    kind.name(),
                                    if what == "data table" {
                                        "table"
                                    } else {
                                        "array"
                                    }
                                )));
                            }
                            None => {
                                return Err(JudgeError::new(format!(
                                    "{} lies in the range of the {what} at {} and is not in the formula inventory; Excel lists every cell of the range as a formula cell",
                                    place(&member),
                                    place(cell)
                                )));
                            }
                        }
                    }
                }
            }
            FormulaKind::LegacyArrayMember {
                anchor_row,
                anchor_column,
            }
            | FormulaKind::DataTableMember {
                anchor_row,
                anchor_column,
            } => {
                let covered =
                    CellAddress::new(cell.sheet(), anchor_row, anchor_column).and_then(|anchor| {
                        let kind = kinds.get(&anchor)?;
                        if kind.member_of((anchor_row, anchor_column)) != Some(formula.kind) {
                            return None;
                        }
                        let (first, last) = kind.legacy_range(&anchor)?;
                        ((first.0..=last.0).contains(&cell.row())
                            && (first.1..=last.1).contains(&cell.column()))
                        .then_some(())
                    });
                if covered.is_none() {
                    return Err(JudgeError::new(format!(
                        "{} is a member of a {what} anchored at {}!{}, which the formula inventory does not list as the anchor of a range covering it",
                        place(cell),
                        cell.sheet(),
                        a1_text(anchor_row, anchor_column)
                    )));
                }
            }
            FormulaKind::Ordinary
            | FormulaKind::SharedMaster { .. }
            | FormulaKind::SharedChild { .. } => {}
        }
    }
    Ok(())
}

/// The CellVerdict report for `results` judged against `capture`, with `evaluations` (an evaluation
/// record of either version, [`crate::EvaluationRecord`] or [`crate::InstrumentRecord`]) as the
/// proof of calculation, under `policy`: the one place a verdict is computed, for a file-cache
/// replay, a live capture and the product's conformance runs alike. Refuses an empty formula
/// inventory ([`JudgeError::EmptyInventory`]) and inputs inconsistent in themselves or with each
/// other ([`JudgeError::Inconsistent`]).
pub fn judge<'e>(
    capture: &CaptureRecord,
    results: &ResultSet,
    evaluations: impl Into<EvaluationProof<'e>>,
    policy: &VerdictPolicy<'_>,
) -> Result<VerdictReport, JudgeError> {
    let evaluations: EvaluationProof<'e> = evaluations.into();
    let (evaluations_case, evaluations_source) = evaluations.identity();
    for (what, case, source) in [
        ("result set", results.case.as_str(), results.source.as_str()),
        ("evaluation record", evaluations_case, evaluations_source),
    ] {
        if case != capture.case {
            return Err(JudgeError::new(format!(
                "the {what} is for case {case:?} and the capture record for case {:?}",
                capture.case
            )));
        }
        if source != capture.source {
            return Err(JudgeError::new(format!(
                "the {what} is for source {source:?} and the capture record for source {:?}",
                capture.source
            )));
        }
    }
    if capture.formulas.is_empty() {
        return Err(JudgeError::EmptyInventory {
            case: capture.case.clone(),
        });
    }

    let mut sheets: BTreeMap<&str, SheetVisibility> = BTreeMap::new();
    for sheet in &capture.sheets {
        if sheet.name.is_empty() {
            return Err(JudgeError::new(
                "the capture record has a sheet with an empty name",
            ));
        }
        if sheets.insert(&sheet.name, sheet.visibility).is_some() {
            return Err(JudgeError::new(format!(
                "the capture record lists the sheet {:?} twice",
                sheet.name
            )));
        }
    }

    let mut inventory: BTreeSet<&CellAddress> = BTreeSet::new();
    let mut kinds: BTreeMap<&CellAddress, FormulaKind> = BTreeMap::new();
    let mut groups: BTreeMap<(&str, u64), (u64, u64)> = BTreeMap::new();
    let mut group_cells: BTreeMap<(&str, u64), BTreeSet<&CellAddress>> = BTreeMap::new();
    for formula in &capture.formulas {
        if !sheets.contains_key(formula.cell.sheet()) {
            return Err(JudgeError::new(format!(
                "the formula inventory names {}, on a sheet the capture record does not list",
                place(&formula.cell)
            )));
        }
        if !inventory.insert(&formula.cell) {
            return Err(JudgeError::new(format!(
                "the formula inventory lists {} twice",
                place(&formula.cell)
            )));
        }
        kinds.insert(&formula.cell, formula.kind);
        if let Some(group) = formula.kind.group() {
            let entry = groups.entry((formula.cell.sheet(), group)).or_default();
            match formula.kind {
                FormulaKind::SharedMaster { .. } => entry.0 += 1,
                _ => entry.1 += 1,
            }
            group_cells
                .entry((formula.cell.sheet(), group))
                .or_default()
                .insert(&formula.cell);
        }
    }
    check_legacy_arrays(capture, &kinds)?;
    for ((sheet, group), (masters, children)) in &groups {
        if *masters != 1 {
            return Err(JudgeError::new(format!(
                "shared formula group {group} on sheet {sheet:?} has {masters} masters and {children} children; a group has exactly one master"
            )));
        }
    }
    if let Some(session) = &capture.session {
        if let Some(failure) = session.failures_at(STAGE_INVENTORY).next() {
            let cell = failure
                .cell
                .as_ref()
                .map(|cell| format!(" at {}", place(cell)))
                .unwrap_or_default();
            return Err(JudgeError::new(format!(
                "the capture record of case {:?} records an inventory failure{cell} ({}), so its formula inventory, the denominator, is not known to be complete",
                capture.case, failure.message
            )));
        }
        check_shared_ranges(session, &sheets, &group_cells)?;
        for (list, entries) in [("saved", &session.saved), ("reopened", &session.reopened)] {
            let mut seen: BTreeSet<&CellAddress> = BTreeSet::new();
            for entry in entries {
                if !inventory.contains(&entry.cell) {
                    return Err(JudgeError::new(format!(
                        "the capture record's {list} list holds {}, which is not in its formula inventory",
                        place(&entry.cell)
                    )));
                }
                if !seen.insert(&entry.cell) {
                    return Err(JudgeError::new(format!(
                        "the capture record's {list} list holds {} twice",
                        place(&entry.cell)
                    )));
                }
            }
        }
    }
    let captured = capture
        .session
        .as_ref()
        .map(|session| (session.revision, session.context.as_str()));
    let capture_failed = capture.session.as_ref().is_some_and(|session| {
        session.failures_at(STAGE_ISOLATION).next().is_some()
            || session.failures_at(STAGE_ENVIRONMENT).next().is_some()
    });

    let mut observations: BTreeMap<&CellAddress, &Observation> = BTreeMap::new();
    for observed in &capture.observations {
        if !inventory.contains(&observed.cell) {
            return Err(JudgeError::new(format!(
                "the capture record observes {}, which is not in its formula inventory",
                place(&observed.cell)
            )));
        }
        if capture.acquisition == Acquisition::FileCacheReplay
            && matches!(observed.observation, Observation::ExcelRaw(_))
        {
            return Err(JudgeError::new(format!(
                "the capture record is a file-cache replay and holds a raw Excel observation for {}; a file holds no raw observation",
                place(&observed.cell)
            )));
        }
        if observations
            .insert(&observed.cell, &observed.observation)
            .is_some()
        {
            return Err(JudgeError::new(format!(
                "the capture record observes {} twice",
                place(&observed.cell)
            )));
        }
    }

    // Each evaluation by its cell, tick, revision and context, with what it returned when a
    // version 2 record says.
    let entries = evaluations.entries();
    let mut evaluated: BTreeMap<(&CellAddress, u64, u64, &str), Option<&Returned>> =
        BTreeMap::new();
    for (evaluation, returned) in &entries {
        if evaluated
            .insert(
                (
                    &evaluation.cell,
                    evaluation.tick,
                    evaluation.revision,
                    &evaluation.context,
                ),
                *returned,
            )
            .is_some()
        {
            return Err(JudgeError::new(format!(
                "the evaluation record lists the evaluation of {} at tick {} (revision {}, context {:?}) twice",
                place(&evaluation.cell),
                evaluation.tick,
                evaluation.revision,
                evaluation.context
            )));
        }
    }

    let mut by_cell: BTreeMap<&CellAddress, &CellResult> = BTreeMap::new();
    let mut outside_inventory = Vec::new();
    for result in &results.results {
        match (result.provenance.value_rule(), &result.value) {
            (Some(true), None) => {
                return Err(JudgeError::new(format!(
                    "the {} result for {} carries no value",
                    result.provenance.name(),
                    place(&result.cell)
                )));
            }
            (Some(false), Some(_)) => {
                return Err(JudgeError::new(format!(
                    "the {} result for {} carries a value",
                    result.provenance.name(),
                    place(&result.cell)
                )));
            }
            _ => {}
        }
        if by_cell.insert(&result.cell, result).is_some() {
            return Err(JudgeError::new(format!(
                "the result set holds two results for {}",
                place(&result.cell)
            )));
        }
        if !inventory.contains(&result.cell) {
            outside_inventory.push(OutsideResult {
                cell: result.cell.clone(),
                provenance: result.provenance.clone(),
            });
        }
    }

    let raw = raw_certainty(capture, policy);
    let mut cells = Vec::with_capacity(capture.formulas.len());
    for formula in &capture.formulas {
        let observation = observations.get(&formula.cell).copied();
        let result = by_cell.get(&formula.cell).copied();
        let value = result.and_then(|result| result.value.as_ref());
        let equals = || {
            observation
                .zip(value)
                .map(|(observation, value)| compare(observation.expected(), value).is_none())
        };
        let (verdict, value_equals_expectation) = match result.map(|result| &result.provenance) {
            None => (Verdict::Missing { reason: None }, None),
            Some(Provenance::Missing { reason }) => (
                Verdict::Missing {
                    reason: Some(reason.clone()),
                },
                None,
            ),
            Some(Provenance::Failed { fault }) => (
                Verdict::Failed {
                    fault: fault.clone(),
                },
                None,
            ),
            Some(Provenance::NotModeled {
                feature,
                diagnostic,
            }) => (
                Verdict::NotModeled {
                    feature: feature.clone(),
                    diagnostic: diagnostic.clone(),
                },
                None,
            ),
            Some(Provenance::FileCached { source }) => (
                Verdict::FileCached {
                    source: source.clone(),
                },
                equals(),
            ),
            Some(Provenance::Stale {
                last_calculated,
                cause,
            }) => (
                Verdict::Stale {
                    source: StaleSource::Reported {
                        last_calculated: *last_calculated,
                        cause: cause.clone(),
                    },
                },
                equals(),
            ),
            Some(Provenance::Calculated {
                tick,
                revision,
                context,
                ..
            }) => {
                if *revision != results.run.revision
                    || captured.is_some_and(|(captured, _)| captured != results.run.revision)
                {
                    (
                        Verdict::Stale {
                            source: StaleSource::RevisionNotRun {
                                revision: *revision,
                            },
                        },
                        equals(),
                    )
                } else if *context != results.run.context
                    || captured.is_some_and(|(_, captured)| captured != results.run.context)
                {
                    (
                        Verdict::Stale {
                            source: StaleSource::ContextNotRun {
                                context: context.clone(),
                            },
                        },
                        equals(),
                    )
                } else if let Some(returned) = {
                    // A legacy array formula is evaluated once, at its anchor; each cell of its
                    // range is proven by that evaluation's array, filled as a legacy array fills
                    // its range ([`legacy_fill`]).
                    let (proving, offset) = match formula.kind {
                        FormulaKind::LegacyArrayMember {
                            anchor_row,
                            anchor_column,
                        } => (
                            CellAddress::new(formula.cell.sheet(), anchor_row, anchor_column),
                            Some((
                                formula.cell.row() - anchor_row,
                                formula.cell.column() - anchor_column,
                            )),
                        ),
                        FormulaKind::LegacyArrayAnchor { .. } => (None, Some((0, 0))),
                        _ => (None, None),
                    };
                    let own = evaluated
                        .get(&(&formula.cell, *tick, *revision, context.as_str()))
                        .copied();
                    match offset {
                        None => unproven(own, value),
                        // A member evaluated on its own (a version 1 record lists one) is proven
                        // by that evaluation; otherwise by its anchor's.
                        Some((row, column)) => {
                            let evaluation = own.or_else(|| {
                                proving.as_ref().and_then(|anchor| {
                                    evaluated
                                        .get(&(anchor, *tick, *revision, context.as_str()))
                                        .copied()
                                })
                            });
                            unproven_filled(evaluation, value, row, column)
                        }
                    }
                } {
                    (
                        Verdict::Unproven {
                            tick: *tick,
                            returned,
                        },
                        equals(),
                    )
                } else {
                    let verdict = match observation.zip(value) {
                        None => Verdict::NotRun,
                        Some((observation, value)) => {
                            let certainty = certainty_of(observation, raw, capture_failed);
                            match compare(observation.expected(), value) {
                                None => Verdict::Matched { certainty },
                                Some(class) => Verdict::Mismatched {
                                    certainty,
                                    class,
                                    expected: observation.expected().clone(),
                                    actual: value.clone(),
                                },
                            }
                        }
                    };
                    (verdict, None)
                }
            }
        };
        let external_links = verdict.is_calculated()
            && matches!(
                result.map(|result| &result.provenance),
                Some(Provenance::Calculated {
                    external_links: true,
                    ..
                })
            );
        cells.push(CellVerdict {
            formula: formula.clone(),
            visibility: sheets[formula.cell.sheet()],
            observation: observation.cloned(),
            verdict,
            value_equals_expectation,
            external_links,
        });
    }

    let mut counts = Counts {
        denominator: cells.len() as u64,
        outside_inventory: outside_inventory.len() as u64,
        evaluator_calls: entries.len() as u64,
        ..Counts::default()
    };
    let value_checked = evaluations.version() != 1;
    let mut strata = Strata::default();
    for cell in &cells {
        let certainty = match &cell.verdict {
            Verdict::Matched { certainty } => {
                counts.matched += 1;
                Some(certainty)
            }
            Verdict::Mismatched { certainty, .. } => {
                counts.mismatched += 1;
                Some(certainty)
            }
            Verdict::NotRun => {
                counts.not_run += 1;
                None
            }
            Verdict::Unproven { .. } => {
                counts.unproven += 1;
                None
            }
            Verdict::Stale { .. } => {
                counts.stale += 1;
                None
            }
            Verdict::FileCached { .. } => {
                counts.file_cached += 1;
                None
            }
            Verdict::NotModeled { .. } => {
                counts.not_modeled += 1;
                None
            }
            Verdict::Failed { .. } => {
                counts.failed += 1;
                None
            }
            Verdict::Missing { .. } => {
                counts.missing += 1;
                None
            }
        };
        match certainty {
            Some(Certainty::StrictBits) => counts.strict_certified += 1,
            Some(Certainty::Uncertified(_)) => counts.uncertified += 1,
            None => {}
        }
        if cell.verdict.is_calculated() {
            counts.calculated += 1;
            if !value_checked {
                counts.calculated_without_value_check += 1;
            }
        }
        if cell.external_links {
            counts.external_links += 1;
            if matches!(cell.verdict, Verdict::Matched { .. }) {
                counts.matched_external_links += 1;
            }
        }
        if cell.value_equals_expectation == Some(true) {
            counts.not_calculated_equal_to_expectation += 1;
        }
        match cell.visibility {
            SheetVisibility::Visible => strata.visible_sheet += 1,
            SheetVisibility::Hidden => strata.hidden_sheet += 1,
            SheetVisibility::VeryHidden => strata.very_hidden_sheet += 1,
        }
        match cell.formula.kind {
            FormulaKind::Ordinary => strata.ordinary += 1,
            FormulaKind::SharedMaster { .. } => strata.shared_master += 1,
            FormulaKind::SharedChild { .. } => strata.shared_child += 1,
            FormulaKind::LegacyArrayAnchor { .. } => strata.legacy_array_anchor += 1,
            FormulaKind::LegacyArrayMember { .. } => strata.legacy_array_member += 1,
            FormulaKind::DataTableAnchor { .. } => strata.data_table_anchor += 1,
            FormulaKind::DataTableMember { .. } => strata.data_table_member += 1,
        }
        if let Some(observation) = &cell.observation {
            strata.observed += 1;
            if matches!(observation.expected().core(), CoreValue::Error(_)) {
                strata.error_expected += 1;
            }
        }
    }

    Ok(VerdictReport {
        case: capture.case.clone(),
        source: capture.source.clone(),
        acquisition: capture.acquisition.clone(),
        numeric_mode: policy.numeric_mode.clone(),
        run: results.run.clone(),
        captured: capture.session.as_ref().map(|session| RunIdentity {
            revision: session.revision,
            context: session.context.clone(),
        }),
        capture_failures: capture.session.as_ref().map(|session| {
            let mut counts = CaptureFailureCounts::default();
            for failure in &session.failures {
                counts.total += 1;
                *counts.by_stage.entry(failure.stage.clone()).or_default() += 1;
            }
            counts
        }),
        com_add_ins_connected: capture
            .environment
            .as_ref()
            .and_then(|environment| environment.com_add_ins_connected()),
        evaluation_record_version: evaluations.version(),
        counts,
        strata,
        cells,
        outside_inventory,
    })
}

/// What an evaluation returned, as a version 2 evaluation record says: the value, or why none.
type Returned = Result<CalcValue, String>;

/// Why a calculated result is unproven, given the evaluation of its cell at its tick, revision and
/// context (`None` when the record holds none) and the value it publishes: `None` when it is
/// proven, `Some(None)` when there is no evaluation, and `Some(Some(returned))` when a version 2
/// record says the evaluation returned no value or another value than the published one.
///
/// An evaluation that returned an array of more than one cell proves a published #SPILL! (dc-677):
/// the anchor of a formula whose array cannot spill (its range blocked, or off the grid) shows
/// #SPILL!, as Excel's does, and that error is the evaluation's array placed, not another value.
/// The #SPILL! is still compared with Excel's observation like any value; a spill the array
/// should have made shows there as a mismatch, never as a pass.
fn unproven(
    evaluation: Option<Option<&Returned>>,
    published: Option<&CalcValue>,
) -> Option<Option<Returned>> {
    match evaluation {
        None => Some(None),
        // A version 1 record: the listed evaluation is all the proof there is, and the report
        // counts the cell in `calculated_without_value_check`.
        Some(None) => None,
        Some(Some(returned)) => {
            let gave_published = match (returned, published) {
                (Ok(value), Some(published)) => {
                    compare(value, published).is_none() || spill_of(value, published)
                }
                _ => false,
            };
            (!gave_published).then(|| Some(returned.clone()))
        }
    }
}

/// [`unproven`] for the cell at `row` and `column` of a legacy array formula's range, proven by
/// the anchor's evaluation: the value its range shows there ([`legacy_fill`]) must be the one the
/// cell publishes (dc-nam). The reported `returned` is the evaluation's own value.
fn unproven_filled(
    evaluation: Option<Option<&Returned>>,
    published: Option<&CalcValue>,
    row: u32,
    column: u32,
) -> Option<Option<Returned>> {
    match evaluation {
        None => Some(None),
        Some(None) => None,
        Some(Some(returned)) => {
            let gave_published = match (returned, published) {
                (Ok(value), Some(published)) => {
                    compare(&legacy_fill(value, row, column), published).is_none()
                }
                _ => false,
            };
            (!gave_published).then(|| Some(returned.clone()))
        }
    }
}

/// The value a legacy array formula's range shows at `row` and `column` from its anchor for the
/// evaluation's value `value`, as Excel fills it: a single value everywhere, a row of one row down
/// every row and a column of one column across every column, an element where the array has
/// one, and #N/A beyond it.
fn legacy_fill(value: &CalcValue, row: u32, column: u32) -> CalcValue {
    let CoreValue::Array(array) = value.core() else {
        return value.clone();
    };
    let shape = array.shape();
    let r = if shape.rows == 1 { 0 } else { row as usize };
    let c = if shape.cols == 1 { 0 } else { column as usize };
    array
        .get(r, c)
        .cloned()
        .unwrap_or_else(|| CalcValue::error(WorksheetErrorCode::NA))
}

/// Whether `published` is the #SPILL! an anchor shows for the array `returned` it cannot spill.
fn spill_of(returned: &CalcValue, published: &CalcValue) -> bool {
    matches!(returned.core(), CoreValue::Array(array) if array.cell_count() > 1)
        && matches!(
            published.core(),
            CoreValue::Error(WorksheetErrorCode::Spill)
        )
}

fn returned_out(returned: &Returned) -> Out {
    match returned {
        Ok(value) => Out::Object(vec![("value", value_out(value))]),
        Err(why) => Out::Object(vec![("no_value", Out::text(why.as_str()))]),
    }
}

fn certainty_members(certainty: Certainty, members: &mut Vec<(&'static str, Out)>) {
    match certainty {
        Certainty::StrictBits => members.push(("certainty", Out::text("strict_bits"))),
        Certainty::Uncertified(reason) => {
            members.push(("certainty", Out::text("uncertified")));
            members.push(("uncertified", Out::text(reason.name())));
        }
    }
}

fn cell_out(cell: &CellVerdict) -> Out {
    let address = &cell.formula.cell;
    let mut members = vec![
        ("sheet", Out::text(address.sheet())),
        ("row", Out::Unsigned(u64::from(address.row()))),
        ("column", Out::Unsigned(u64::from(address.column()))),
        ("cell", Out::text(address.a1())),
        ("kind", Out::text(cell.formula.kind.name())),
    ];
    if let Some(group) = cell.formula.kind.group() {
        members.push(("group", Out::Unsigned(group)));
    }
    if let Some((name, reference)) = cell.formula.kind.legacy_member(address) {
        members.push((name, Out::text(reference.as_str())));
    }
    members.push(("sheet_visibility", Out::text(cell.visibility.name())));
    members.push((
        "observation",
        cell.observation
            .as_ref()
            .map_or(Out::Null, |observation| Out::text(observation.origin())),
    ));
    if let Some(Observation::ExcelSavedCache(cache) | Observation::UnknownFreshnessCache(cache)) =
        &cell.observation
    {
        members.push((
            "cache",
            Out::Object(vec![
                ("t", Out::text(cache.cell_type().name())),
                ("v", Out::text(cache.text())),
            ]),
        ));
    }
    members.push(("verdict", Out::text(cell.verdict.name())));
    if cell.external_links {
        members.push(("external_links", Out::Bool(true)));
    }
    match &cell.verdict {
        Verdict::Matched { certainty } => certainty_members(*certainty, &mut members),
        Verdict::Mismatched {
            certainty,
            class,
            expected,
            actual,
        } => {
            certainty_members(*certainty, &mut members);
            members.push(("mismatch", class.out()));
            members.push(("expected", value_out(expected)));
            members.push(("actual", value_out(actual)));
        }
        Verdict::NotRun => {}
        Verdict::Unproven { tick, returned } => {
            members.push(("tick", Out::Unsigned(*tick)));
            if let Some(returned) = returned {
                members.push(("returned", returned_out(returned)));
            }
        }
        Verdict::Stale { source } => {
            members.push(("stale", Out::text(source.name())));
            match source {
                StaleSource::Reported {
                    last_calculated,
                    cause,
                } => {
                    members.push((
                        "last_calculated",
                        last_calculated.map_or(Out::Null, Out::Unsigned),
                    ));
                    members.push(("cause", Out::text(cause.as_str())));
                }
                StaleSource::RevisionNotRun { revision } => {
                    members.push(("revision", Out::Unsigned(*revision)));
                }
                StaleSource::ContextNotRun { context } => {
                    members.push(("context", Out::text(context.as_str())));
                }
            }
        }
        Verdict::FileCached { source } => members.push(("source", Out::text(source.as_str()))),
        Verdict::NotModeled {
            feature,
            diagnostic,
        } => {
            members.push(("feature", Out::text(feature.as_str())));
            members.push(("diagnostic", Out::text(diagnostic.as_str())));
        }
        Verdict::Failed { fault } => members.push(("fault", Out::text(fault.as_str()))),
        Verdict::Missing { reason } => members.push((
            "reason",
            reason
                .as_ref()
                .map_or(Out::Null, |reason| Out::text(reason.as_str())),
        )),
    }
    if let Some(equal) = cell.value_equals_expectation {
        members.push(("value_equals_expectation", Out::Bool(equal)));
    }
    Out::Object(members)
}

impl VerdictReport {
    /// The compared cells whose certainty is uncertified, counted by reason.
    pub fn uncertified_by_reason(&self) -> BTreeMap<UncertifiedReason, u64> {
        let mut by_reason = BTreeMap::new();
        for cell in &self.cells {
            if let Verdict::Matched {
                certainty: Certainty::Uncertified(reason),
            }
            | Verdict::Mismatched {
                certainty: Certainty::Uncertified(reason),
                ..
            } = &cell.verdict
            {
                *by_reason.entry(*reason).or_default() += 1;
            }
        }
        by_reason
    }

    /// The report as a JSON document (`dnacalc-verdict-report`, version 5): the headline
    /// (calculated of the denominator, then matched of the calculated), every count, the uncertified
    /// comparisons by reason, the strata, one line per inventory cell in inventory order, and the
    /// results outside the inventory. For a version 2 capture record a `captured` member after
    /// `run` names the revision and context the capture observed, and a `capture_failures` member
    /// after it counts the capture's failures (`{"total": n, "by_stage": {"<stage>": n, ...}}`); a
    /// report on a version 1 record has neither member. A record whose environment carries the
    /// setting `com_add_ins_connected` gives a `com_add_ins_connected` member next, the number of
    /// COM add-ins it lists as connected during the capture. Then `evaluation_record_version`
    /// names the version of the evaluation record judged against, before the headline.
    pub fn encode(&self) -> String {
        let c = &self.counts;
        let run = |identity: &RunIdentity| {
            Out::Object(vec![
                ("revision", Out::Unsigned(identity.revision)),
                ("context", Out::text(identity.context.as_str())),
            ])
        };
        let numeric_mode = match &self.numeric_mode {
            NumericMode::Fast => Out::Object(vec![("mode", Out::text("fast"))]),
            NumericMode::Exact(target) => Out::Object(vec![
                ("mode", Out::text("exact")),
                ("excel_build", Out::text(target.excel_build())),
                ("channel", Out::text(target.channel())),
                ("compatibility", Out::text(target.compatibility())),
                ("numeric_environment", Out::text(target.environment())),
            ]),
        };
        let by_reason = self.uncertified_by_reason();
        let s = &self.strata;
        let mut members = vec![
            ("format", Out::text(REPORT_FORMAT)),
            ("version", Out::Unsigned(REPORT_VERSION)),
            ("case", Out::text(self.case.as_str())),
            ("source", Out::text(self.source.as_str())),
            ("acquisition", self.acquisition.out()),
            ("numeric_mode", numeric_mode),
            ("run", run(&self.run)),
        ];
        if let Some(captured) = &self.captured {
            members.push(("captured", run(captured)));
        }
        if let Some(failures) = &self.capture_failures {
            members.push((
                "capture_failures",
                Out::Object(vec![
                    ("total", Out::Unsigned(failures.total)),
                    (
                        "by_stage",
                        Out::Members(
                            failures
                                .by_stage
                                .iter()
                                .map(|(stage, count)| (stage.clone(), Out::Unsigned(*count)))
                                .collect(),
                        ),
                    ),
                ]),
            ));
        }
        if let Some(connected) = self.com_add_ins_connected {
            members.push(("com_add_ins_connected", Out::Unsigned(connected)));
        }
        members.extend([
            (
                "evaluation_record_version",
                Out::Unsigned(self.evaluation_record_version),
            ),
            (
                "headline",
                Out::Object(vec![
                    (
                        "calculated_of_denominator",
                        Out::Array(vec![
                            Out::Unsigned(c.calculated),
                            Out::Unsigned(c.denominator),
                        ]),
                    ),
                    (
                        "matched_of_calculated",
                        Out::Array(vec![Out::Unsigned(c.matched), Out::Unsigned(c.calculated)]),
                    ),
                ]),
            ),
            (
                "counts",
                Out::Object(vec![
                    ("denominator", Out::Unsigned(c.denominator)),
                    ("calculated", Out::Unsigned(c.calculated)),
                    ("matched", Out::Unsigned(c.matched)),
                    ("mismatched", Out::Unsigned(c.mismatched)),
                    ("not_run", Out::Unsigned(c.not_run)),
                    ("strict_certified", Out::Unsigned(c.strict_certified)),
                    ("uncertified", Out::Unsigned(c.uncertified)),
                    ("unproven", Out::Unsigned(c.unproven)),
                    ("stale", Out::Unsigned(c.stale)),
                    ("file_cached", Out::Unsigned(c.file_cached)),
                    ("not_modeled", Out::Unsigned(c.not_modeled)),
                    ("failed", Out::Unsigned(c.failed)),
                    ("missing", Out::Unsigned(c.missing)),
                    (
                        "not_calculated_equal_to_expectation",
                        Out::Unsigned(c.not_calculated_equal_to_expectation),
                    ),
                    ("outside_inventory", Out::Unsigned(c.outside_inventory)),
                    ("evaluator_calls", Out::Unsigned(c.evaluator_calls)),
                    (
                        "calculated_without_value_check",
                        Out::Unsigned(c.calculated_without_value_check),
                    ),
                ]
                .into_iter()
                // Written only when a calculated cell read another workbook's link caches, so a
                // report without one reads as it did before (dc-ylr).
                .chain((c.external_links > 0).then(|| {
                    [
                        ("external_links", Out::Unsigned(c.external_links)),
                        (
                            "matched_external_links",
                            Out::Unsigned(c.matched_external_links),
                        ),
                    ]
                }).into_iter().flatten())
                .collect()),
            ),
            (
                "uncertified_by_reason",
                Out::Object(
                    by_reason
                        .iter()
                        .map(|(reason, count)| (reason.name(), Out::Unsigned(*count)))
                        .collect(),
                ),
            ),
            ("strata", {
                let mut strata = vec![
                    ("visible_sheet", Out::Unsigned(s.visible_sheet)),
                    ("hidden_sheet", Out::Unsigned(s.hidden_sheet)),
                    ("very_hidden_sheet", Out::Unsigned(s.very_hidden_sheet)),
                    ("ordinary", Out::Unsigned(s.ordinary)),
                    ("shared_master", Out::Unsigned(s.shared_master)),
                    ("shared_child", Out::Unsigned(s.shared_child)),
                    ("legacy_array_anchor", Out::Unsigned(s.legacy_array_anchor)),
                    ("legacy_array_member", Out::Unsigned(s.legacy_array_member)),
                ];
                // Written only when the inventory holds a data table, so a report without one
                // reads as it did before data tables were captured.
                if s.data_table_anchor + s.data_table_member > 0 {
                    strata.push(("data_table_anchor", Out::Unsigned(s.data_table_anchor)));
                    strata.push(("data_table_member", Out::Unsigned(s.data_table_member)));
                }
                strata.push(("observed", Out::Unsigned(s.observed)));
                strata.push(("error_expected", Out::Unsigned(s.error_expected)));
                Out::Object(strata)
            }),
            (
                "cells",
                Out::Array(self.cells.iter().map(cell_out).collect()),
            ),
            (
                "outside_inventory",
                Out::Array(
                    self.outside_inventory
                        .iter()
                        .map(|outside| {
                            Out::Object(vec![
                                ("sheet", Out::text(outside.cell.sheet())),
                                ("row", Out::Unsigned(u64::from(outside.cell.row()))),
                                ("column", Out::Unsigned(u64::from(outside.cell.column()))),
                                ("cell", Out::text(outside.cell.a1())),
                                ("provenance", Out::text(outside.provenance.name())),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]);
        Out::Object(members).document()
    }

    /// The report's counts on one line, headline first, with the uncertified comparisons by reason,
    /// the evaluation record's version with the calculated cells it proves without a value check
    /// and, for a version 2 capture record, the capture's failures by stage, then the COM add-ins
    /// connected during the capture when the record lists them.
    pub fn summary(&self) -> String {
        let c = &self.counts;
        let reasons: Vec<String> = self
            .uncertified_by_reason()
            .iter()
            .map(|(reason, count)| format!("{} {count}", reason.name()))
            .collect();
        let reasons = if reasons.is_empty() {
            String::new()
        } else {
            format!(" ({})", reasons.join(", "))
        };
        let capture_failures = self
            .capture_failures
            .as_ref()
            .map(|failures| {
                let stages: Vec<String> = failures
                    .by_stage
                    .iter()
                    .map(|(stage, count)| format!("{stage} {count}"))
                    .collect();
                if stages.is_empty() {
                    "; 0 capture failures".to_owned()
                } else {
                    format!(
                        "; {} capture failures ({})",
                        failures.total,
                        stages.join(", ")
                    )
                }
            })
            .unwrap_or_default();
        let com_add_ins = self
            .com_add_ins_connected
            .map(|connected| format!("; {connected} COM add-ins connected during the capture"))
            .unwrap_or_default();
        format!(
            "{}: calculated {} of {} formula cells; matched {} of {} calculated, mismatched {}, not run {}; of the {} compared, strict-bit certified {}, uncertified {}{}; not calculated: unproven {}, stale {}, file-cached {}, not modeled {}, failed {}, missing {}; {} not-calculated values equal their expectation; {} results outside the inventory; {} evaluator calls (evaluation record version {}, {} calculated without a value check){}{}",
            self.case,
            c.calculated,
            c.denominator,
            c.matched,
            c.calculated,
            c.mismatched,
            c.not_run,
            c.matched + c.mismatched,
            c.strict_certified,
            c.uncertified,
            reasons,
            c.unproven,
            c.stale,
            c.file_cached,
            c.not_modeled,
            c.failed,
            c.missing,
            c.not_calculated_equal_to_expectation,
            c.outside_inventory,
            c.evaluator_calls,
            self.evaluation_record_version,
            c.calculated_without_value_check,
            capture_failures,
            com_add_ins
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::{CacheCellType, SavedCache};
    use crate::evaluations::{
        Evaluation, EvaluationRecord, InstrumentRecord, InstrumentedEvaluation,
    };
    use crate::value::{ExcelText, NumericTargetId};

    const ENVIRONMENT: &str = r#"{"excel_build": "20430", "channel": "current", "compatibility": "2", "numeric_environment": "x86-64", "settings": {"date_system": "1900"}}"#;

    /// A record of one formula cell, Sheet1!A1, observed as `observation`.
    fn capture(acquisition: &str, environment: &str, observation: &str) -> CaptureRecord {
        let text = format!(
            r#"{{"format": "dnacalc-capture-record", "version": 1, "case": "c", "source": "s",
                "acquisition": {acquisition}, "environment": {environment},
                "sheets": [{{"name": "Sheet1", "visibility": "visible"}}],
                "formulas": [{{"sheet": "Sheet1", "row": 1, "column": 1, "kind": "ordinary"}}],
                "observations": [{{"sheet": "Sheet1", "row": 1, "column": 1, {observation}}}]}}"#
        );
        CaptureRecord::decode(text.as_bytes()).unwrap()
    }

    /// A live capture of build 20430 of the formula cells Sheet1!A1 down to row
    /// `observations.len()`, row `i + 1` observed as the member `observations[i]` (not observed
    /// when it is empty).
    fn column_capture(observations: &[&str]) -> CaptureRecord {
        let formulas: Vec<String> = (1..=observations.len())
            .map(|row| {
                format!(r#"{{"sheet": "Sheet1", "row": {row}, "column": 1, "kind": "ordinary"}}"#)
            })
            .collect();
        let observed: Vec<String> = observations
            .iter()
            .enumerate()
            .filter(|(_, observation)| !observation.is_empty())
            .map(|(i, observation)| {
                format!(
                    r#"{{"sheet": "Sheet1", "row": {}, "column": 1, {observation}}}"#,
                    i + 1
                )
            })
            .collect();
        let text = format!(
            r#"{{"format": "dnacalc-capture-record", "version": 1, "case": "c", "source": "s",
                "acquisition": {LIVE}, "environment": {ENVIRONMENT},
                "sheets": [{{"name": "Sheet1", "visibility": "visible"}}],
                "formulas": [{}], "observations": [{}]}}"#,
            formulas.join(","),
            observed.join(",")
        );
        CaptureRecord::decode(text.as_bytes()).unwrap()
    }

    /// A run at revision 3 under context "k" that publishes, for row `i + 1` of column A, the
    /// provenance `results[i].0` with the value member `results[i].1` (none when empty); a row
    /// whose provenance is empty gets no result.
    fn column_results(results: &[(&str, &str)]) -> ResultSet {
        let published: Vec<String> = results
            .iter()
            .enumerate()
            .filter(|(_, (provenance, _))| !provenance.is_empty())
            .map(|(i, (provenance, value))| {
                let value = if value.is_empty() {
                    String::new()
                } else {
                    format!(r#", "value": {value}"#)
                };
                format!(
                    r#"{{"sheet": "Sheet1", "row": {}, "column": 1, "provenance": {provenance}{value}}}"#,
                    i + 1
                )
            })
            .collect();
        let text = format!(
            r#"{{"format": "dnacalc-result-set", "version": 1, "case": "c", "source": "s",
                "run": {{"revision": 3, "context": "k"}}, "results": [{}]}}"#,
            published.join(",")
        );
        ResultSet::decode(text.as_bytes()).unwrap()
    }

    /// A run at revision 3 under context "k" that publishes `provenance` with the number 2 for
    /// Sheet1!A1.
    fn results(provenance: &str) -> ResultSet {
        column_results(&[(provenance, TWO)])
    }

    /// An evaluation record of case "c" listing these evaluations on Sheet1, each as (row,
    /// column, tick, revision, context).
    fn record_of(evaluations: &[(u32, u32, u64, u64, &str)]) -> EvaluationRecord {
        EvaluationRecord {
            case: "c".to_owned(),
            source: "s".to_owned(),
            evaluations: evaluations
                .iter()
                .map(|(row, column, tick, revision, context)| Evaluation {
                    cell: CellAddress::new("Sheet1", *row, *column).unwrap(),
                    tick: *tick,
                    revision: *revision,
                    context: (*context).to_owned(),
                })
                .collect(),
        }
    }

    /// The evaluation that proves CALCULATED: Sheet1!A1 at tick 7, revision 3, context "k".
    fn evaluated() -> EvaluationRecord {
        record_of(&[(1, 1, 7, 3, "k")])
    }

    /// The same evaluation as [`evaluated`] in a version 2 record, returning `returned`.
    fn instrumented(returned: &Result<CalcValue, String>) -> InstrumentRecord {
        let listed = evaluated();
        InstrumentRecord {
            case: listed.case,
            source: listed.source,
            evaluations: listed
                .evaluations
                .into_iter()
                .map(|evaluation| InstrumentedEvaluation {
                    evaluation,
                    returned: returned.clone(),
                })
                .collect(),
        }
    }

    const LIVE: &str = r#"{"kind": "live_capture"}"#;
    const TWO: &str = r#"{"number": "4000000000000000"}"#;
    const RAW_TWO: &str = r#""excel_raw": {"number": "4000000000000000"}"#;
    const CALCULATED: &str = r#"{"calculated": {"tick": 7, "revision": 3, "context": "k"}}"#;

    fn target(build: &str) -> NumericMode {
        NumericMode::Exact(NumericTargetId::new(build, "current", "2", "x86-64").unwrap())
    }

    fn exact(build: &str) -> VerdictPolicy<'static> {
        VerdictPolicy {
            numeric_mode: target(build),
            evidence: None,
        }
    }

    fn only_verdict(
        capture: &CaptureRecord,
        results: &ResultSet,
        evaluations: &EvaluationRecord,
        policy: &VerdictPolicy<'_>,
    ) -> Verdict {
        let report = judge(capture, results, evaluations, policy).unwrap();
        assert_eq!(report.cells.len(), 1);
        report.cells[0].verdict.clone()
    }

    fn matched(certainty: Certainty) -> Verdict {
        Verdict::Matched { certainty }
    }

    fn uncertified(reason: UncertifiedReason) -> Verdict {
        matched(Certainty::Uncertified(reason))
    }

    fn text(value: &str) -> CalcValue {
        CalcValue::text(ExcelText::from_utf16_code_units(
            value.encode_utf16().collect(),
        ))
    }

    /// dc-ylr: a calculated result that read another workbook's link caches is still judged as
    /// calculated, and the report counts it apart, in the counts and on the cell; a result without
    /// the member is not counted there.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_result_read_from_external_link_caches_is_counted_apart() {
        let live = capture(LIVE, ENVIRONMENT, RAW_TWO);
        let linked = results(
            r#"{"calculated": {"tick": 7, "revision": 3, "context": "k", "external_links": true}}"#,
        );
        let report = judge(&live, &linked, &evaluated(), &exact("20430")).unwrap();
        assert_eq!(report.cells[0].verdict, matched(Certainty::StrictBits));
        assert!(report.cells[0].external_links);
        assert_eq!(
            (
                report.counts.matched,
                report.counts.external_links,
                report.counts.matched_external_links
            ),
            (1, 1, 1)
        );
        let encoded: serde_json::Value = serde_json::from_str(&report.encode()).unwrap();
        assert_eq!(encoded["counts"]["external_links"], 1);
        assert_eq!(encoded["counts"]["matched_external_links"], 1);
        assert_eq!(encoded["cells"][0]["external_links"], true);

        let plain = judge(&live, &results(CALCULATED), &evaluated(), &exact("20430")).unwrap();
        assert!(!plain.cells[0].external_links);
        assert_eq!(plain.counts.external_links, 0);
        let encoded: serde_json::Value = serde_json::from_str(&plain.encode()).unwrap();
        assert!(encoded["counts"].get("external_links").is_none());
        assert!(encoded["cells"][0].get("external_links").is_none());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn strict_bits_need_a_raw_observation_of_the_selected_target() {
        let calculated = results(CALCULATED);
        let proof = evaluated();
        let live = capture(LIVE, ENVIRONMENT, RAW_TWO);
        assert_eq!(
            only_verdict(&live, &calculated, &proof, &exact("20430")),
            matched(Certainty::StrictBits)
        );
        assert_eq!(
            only_verdict(&live, &calculated, &proof, &VerdictPolicy::default()),
            uncertified(UncertifiedReason::NoExactTarget)
        );
        assert_eq!(
            only_verdict(&live, &calculated, &proof, &exact("19929")),
            uncertified(UncertifiedReason::EnvironmentNotTarget)
        );
        assert_eq!(
            only_verdict(
                &capture(LIVE, "null", RAW_TWO),
                &calculated,
                &proof,
                &exact("20430")
            ),
            uncertified(UncertifiedReason::EnvironmentUnknown)
        );
        assert_eq!(
            only_verdict(
                &capture(
                    LIVE,
                    ENVIRONMENT,
                    r#""excel_saved_cache": {"t": "n", "v": "2"}"#
                ),
                &calculated,
                &proof,
                &exact("20430")
            ),
            uncertified(UncertifiedReason::SavedCache)
        );
        assert_eq!(
            only_verdict(
                &capture(
                    r#"{"kind": "file_cache_replay"}"#,
                    "null",
                    r#""unknown_freshness_cache": {"t": "n", "v": "2.0"}"#
                ),
                &calculated,
                &proof,
                &exact("20430")
            ),
            uncertified(UncertifiedReason::UnknownFreshness)
        );
        // A decimal cache that reads as another binary64 is a mismatch, uncertified.
        let report = judge(
            &capture(
                LIVE,
                ENVIRONMENT,
                r#""excel_saved_cache": {"t": "n", "v": "2.0000000000000004"}"#,
            ),
            &calculated,
            &proof,
            &exact("20430"),
        )
        .unwrap();
        assert!(
            matches!(
                &report.cells[0].verdict,
                Verdict::Mismatched {
                    certainty: Certainty::Uncertified(UncertifiedReason::SavedCache),
                    class: MismatchClass::Number { ulp: Some(1) },
                    ..
                }
            ),
            "{:?}",
            report.cells[0].verdict
        );
    }

    /// A capture taken with a COM add-in connected is counted and certifies no target: the report
    /// counts the add-ins the environment's `com_add_ins_connected` lists (an escaped `;` or `|`
    /// within a name splits nothing), encodes the count after `run` and before
    /// `evaluation_record_version`, and a raw comparison that would be strict-bit certified is
    /// uncertified with the reason `com_add_in_connected`. An empty list counts 0 and certifies; a
    /// record without the setting carries no count. The reasons that come first (no Exact target)
    /// and a saved cache keep their own reason, with the count still reported.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_connected_com_add_in_is_counted_and_certifies_no_target() {
        let environment = |connected: &str| {
            format!(
                r#"{{"excel_build": "20430", "channel": "current", "compatibility": "2", "numeric_environment": "x86-64", "settings": {{"date_system": "1900", "com_add_ins_connected": "{connected}"}}}}"#
            )
        };
        let calculated = results(CALCULATED);
        let proof = evaluated();
        let one = capture(
            LIVE,
            &environment(
                "VS15ExcelAdaptor|Visual Studio Tools for Office Design-Time Adaptor for Excel",
            ),
            RAW_TWO,
        );
        let report = judge(&one, &calculated, &proof, &exact("20430")).unwrap();
        assert_eq!(report.com_add_ins_connected, Some(1));
        assert_eq!(
            report.cells[0].verdict,
            uncertified(UncertifiedReason::ComAddInConnected)
        );
        let text = report.encode();
        let encoded: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(encoded["com_add_ins_connected"], 1);
        assert_eq!(
            encoded["uncertified_by_reason"],
            serde_json::json!({"com_add_in_connected": 1})
        );
        assert_eq!(encoded["counts"]["strict_certified"], 0);
        let at = |member: &str| text.find(&format!("\"{member}\"")).unwrap();
        assert!(at("run") < at("com_add_ins_connected"), "{text}");
        assert!(
            at("com_add_ins_connected") < at("evaluation_record_version"),
            "{text}"
        );
        assert!(
            report
                .summary()
                .ends_with("; 1 COM add-ins connected during the capture"),
            "{}",
            report.summary()
        );

        let two = capture(
            LIVE,
            &environment("Contoso.A|a%3Bb%7Cc;Contoso.B|d"),
            RAW_TWO,
        );
        let report = judge(&two, &calculated, &proof, &exact("20430")).unwrap();
        assert_eq!(report.com_add_ins_connected, Some(2));

        let none = capture(LIVE, &environment(""), RAW_TWO);
        let report = judge(&none, &calculated, &proof, &exact("20430")).unwrap();
        assert_eq!(report.com_add_ins_connected, Some(0));
        assert_eq!(report.cells[0].verdict, matched(Certainty::StrictBits));
        let encoded: serde_json::Value = serde_json::from_str(&report.encode()).unwrap();
        assert_eq!(encoded["com_add_ins_connected"], 0);

        let unlisted = capture(LIVE, ENVIRONMENT, RAW_TWO);
        let report = judge(&unlisted, &calculated, &proof, &exact("20430")).unwrap();
        assert_eq!(report.com_add_ins_connected, None);
        assert_eq!(report.cells[0].verdict, matched(Certainty::StrictBits));
        let encoded: serde_json::Value = serde_json::from_str(&report.encode()).unwrap();
        assert!(encoded.get("com_add_ins_connected").is_none(), "{encoded}");

        let fast = judge(&one, &calculated, &proof, &VerdictPolicy::default()).unwrap();
        assert_eq!(fast.com_add_ins_connected, Some(1));
        assert_eq!(
            fast.cells[0].verdict,
            uncertified(UncertifiedReason::NoExactTarget)
        );
        let saved = capture(
            LIVE,
            &environment("VS15ExcelAdaptor|d"),
            r#""excel_saved_cache": {"t": "n", "v": "2"}"#,
        );
        let report = judge(&saved, &calculated, &proof, &exact("20430")).unwrap();
        assert_eq!(report.com_add_ins_connected, Some(1));
        assert_eq!(
            report.cells[0].verdict,
            uncertified(UncertifiedReason::SavedCache)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn legacy_evidence_certifies_only_the_target_its_label_names() {
        let record = r#"{"format": "dnacalc-evidence-record/1", "labels": [
            {"entry": "cache/oracle-20430/", "certifies_exact_target": "excel-build-20430"},
            {"entry": "cache/oracle-19929/", "certifies_exact_target": "excel-build-19929"},
            {"entry": "cache/unknown.json", "certifies_exact_target": null}]}"#;
        let labels = EvidenceLabels::decode(record.as_bytes()).unwrap();
        let calculated = results(CALCULATED);
        let proof = evaluated();
        let legacy = |entry: &str, build: &str| {
            let environment = ENVIRONMENT.replace("\"20430\"", &format!("{build:?}"));
            capture(
                &format!(r#"{{"kind": "legacy_evidence", "entry": "{entry}"}}"#),
                &environment,
                RAW_TWO,
            )
        };
        let with_labels = |build: &str| VerdictPolicy {
            numeric_mode: target(build),
            evidence: Some(&labels),
        };
        // The target's build in its short form and as Excel's full file version both reduce to
        // build number 20430, which the label excel-build-20430 names.
        for build in ["20430", "16.0.20430.20000"] {
            assert_eq!(
                only_verdict(
                    &legacy("cache/oracle-20430/", build),
                    &calculated,
                    &proof,
                    &with_labels(build)
                ),
                matched(Certainty::StrictBits),
                "{build}"
            );
            for entry in ["cache/oracle-19929/", "cache/unknown.json"] {
                assert_eq!(
                    only_verdict(
                        &legacy(entry, build),
                        &calculated,
                        &proof,
                        &with_labels(build)
                    ),
                    uncertified(UncertifiedReason::EvidenceLabelNotTarget),
                    "{entry} {build}"
                );
            }
        }
        // A target whose excel_build holds no build number by the rule is named by no label.
        for build in ["16.0", "20430.20000"] {
            assert_eq!(
                only_verdict(
                    &legacy("cache/oracle-20430/", build),
                    &calculated,
                    &proof,
                    &with_labels(build)
                ),
                uncertified(UncertifiedReason::EvidenceLabelNotTarget),
                "{build}"
            );
        }
        assert_eq!(
            only_verdict(
                &legacy("cache/absent/", "20430"),
                &calculated,
                &proof,
                &with_labels("20430")
            ),
            uncertified(UncertifiedReason::EvidenceEntryUnknown)
        );
        assert_eq!(
            only_verdict(
                &legacy("cache/oracle-20430/", "20430"),
                &calculated,
                &proof,
                &exact("20430")
            ),
            uncertified(UncertifiedReason::EvidenceRecordAbsent)
        );
    }

    /// An evaluation that returned an array of more than one cell proves a published #SPILL!
    /// (dc-677: the anchor of an array that cannot spill), which is then compared with Excel's;
    /// it proves no other value, and a one-cell array proves no #SPILL!.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn an_array_evaluation_proves_the_spill_error_its_anchor_shows() {
        let array = |rows: usize, cols: usize| {
            Ok(CalcValue::array(
                crate::value::CalcArray::new(
                    crate::value::ArrayShape { rows, cols },
                    (0..rows * cols)
                        .map(|n| CalcValue::number(n as f64))
                        .collect(),
                )
                .unwrap(),
            ))
        };
        let spill = r#"{"error": "spill"}"#;
        let live = capture(LIVE, ENVIRONMENT, &format!(r#""excel_raw": {spill}"#));
        let calculated = column_results(&[(CALCULATED, spill)]);
        let policy = exact("20430");
        let report = judge(&live, &calculated, &instrumented(&array(3, 1)), &policy).unwrap();
        assert_eq!(report.cells[0].verdict, matched(Certainty::StrictBits));
        let report = judge(&live, &calculated, &instrumented(&array(1, 1)), &policy).unwrap();
        assert!(
            matches!(report.cells[0].verdict, Verdict::Unproven { .. }),
            "{:?}",
            report.cells[0].verdict
        );
        // An array proves no other value: a published 2 stays unproven.
        let two = judge(
            &capture(LIVE, ENVIRONMENT, RAW_TWO),
            &results(CALCULATED),
            &instrumented(&array(3, 1)),
            &policy,
        )
        .unwrap();
        assert!(matches!(two.cells[0].verdict, Verdict::Unproven { .. }));
    }

    /// In a version 2 record an evaluation proves a calculated result only when it returned the
    /// value the result publishes: the published 2 equals the expectation, and an evaluation that
    /// returned another value, or none, leaves it unproven. The same evaluation in a version 1
    /// record, which does not say what it returned, proves the 2.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn an_instrumented_evaluation_proves_only_the_value_it_returned() {
        let live = capture(LIVE, ENVIRONMENT, RAW_TWO);
        let calculated = results(CALCULATED);
        let policy = exact("20430");
        let proven = judge(
            &live,
            &calculated,
            &instrumented(&Ok(CalcValue::number(2.0))),
            &policy,
        )
        .unwrap();
        assert_eq!(proven.cells[0].verdict, matched(Certainty::StrictBits));
        assert_eq!(proven.cells[0].value_equals_expectation, None);
        assert_eq!((proven.counts.calculated, proven.counts.matched), (1, 1));
        assert!(!proven.encode().contains("\"returned\""));
        // Its value was checked against what the evaluation returned.
        assert_eq!(
            (
                proven.evaluation_record_version,
                proven.counts.calculated_without_value_check
            ),
            (2, 0)
        );
        assert!(
            proven
                .summary()
                .contains("(evaluation record version 2, 0 calculated without a value check)"),
            "{}",
            proven.summary()
        );
        for (returned, shown) in [
            (
                Ok(CalcValue::number(3.0)),
                r#""returned":{"value":{"number":"4008000000000000"}}"#,
            ),
            // The same magnitude with the other sign is another value.
            (
                Ok(CalcValue::number(-2.0)),
                r#""returned":{"value":{"number":"c000000000000000"}}"#,
            ),
            (Ok(text("2")), r#""returned":{"value":{"text":"0032"}}"#),
            (
                Err("NearZeroFinalSum".to_owned()),
                r#""returned":{"no_value":"NearZeroFinalSum"}"#,
            ),
        ] {
            let report = judge(&live, &calculated, &instrumented(&returned), &policy).unwrap();
            assert_eq!(
                report.cells[0].verdict,
                Verdict::Unproven {
                    tick: 7,
                    returned: Some(returned.clone()),
                },
                "{returned:?}"
            );
            assert_eq!(report.cells[0].value_equals_expectation, Some(true));
            assert_eq!(
                (
                    report.counts.calculated,
                    report.counts.matched,
                    report.counts.unproven,
                    report.counts.not_calculated_equal_to_expectation,
                    report.counts.evaluator_calls
                ),
                (0, 0, 1, 1, 1),
                "{returned:?}"
            );
            assert!(report.encode().contains(shown), "{returned:?}");
            let listed = judge(&live, &calculated, &evaluated(), &policy).unwrap();
            assert_eq!(listed.cells[0].verdict, matched(Certainty::StrictBits));
            // The version 1 judgement says that its one calculated cell was proven without a
            // value check.
            assert_eq!(
                (
                    listed.evaluation_record_version,
                    listed.counts.calculated,
                    listed.counts.calculated_without_value_check
                ),
                (1, 1, 1)
            );
            assert!(
                listed
                    .summary()
                    .contains("(evaluation record version 1, 1 calculated without a value check)"),
                "{}",
                listed.summary()
            );
        }
        // The evaluation must still be at the claim's tick, revision and context.
        let mut elsewhere = instrumented(&Ok(CalcValue::number(2.0)));
        elsewhere.evaluations[0].evaluation.tick = 6;
        let report = judge(&live, &calculated, &elsewhere, &policy).unwrap();
        assert_eq!(
            report.cells[0].verdict,
            Verdict::Unproven {
                tick: 7,
                returned: None,
            }
        );
        // A version 2 record is checked as a version 1 record is.
        let mut twice = instrumented(&Ok(CalcValue::number(2.0)));
        twice.evaluations.push(twice.evaluations[0].clone());
        twice.evaluations[1].returned = Ok(CalcValue::number(3.0));
        let error = judge(&live, &calculated, &twice, &policy).unwrap_err();
        assert!(error.to_string().contains("twice"), "{error}");
        let mut other = instrumented(&Ok(CalcValue::number(2.0)));
        other.case = "d".to_owned();
        let error = judge(&live, &calculated, &other, &policy).unwrap_err();
        assert!(
            error.to_string().contains("evaluation record is for case"),
            "{error}"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_calculated_claim_counts_only_with_its_evaluation_in_the_evaluation_record() {
        let live = capture(LIVE, ENVIRONMENT, RAW_TWO);
        let policy = VerdictPolicy::default();
        let cases = [
            (
                r#"{"calculated": {"tick": 7, "revision": 2, "context": "k"}}"#,
                record_of(&[(1, 1, 7, 2, "k")]),
                Verdict::Stale {
                    source: StaleSource::RevisionNotRun { revision: 2 },
                },
            ),
            (
                r#"{"calculated": {"tick": 7, "revision": 3, "context": "other"}}"#,
                record_of(&[(1, 1, 7, 3, "other")]),
                Verdict::Stale {
                    source: StaleSource::ContextNotRun {
                        context: "other".to_owned(),
                    },
                },
            ),
            // No evaluation at the claimed tick.
            (
                CALCULATED,
                record_of(&[(1, 1, 6, 3, "k"), (1, 1, 8, 3, "k")]),
                Verdict::Unproven {
                    tick: 7,
                    returned: None,
                },
            ),
            // No evaluation at all.
            (
                CALCULATED,
                record_of(&[]),
                Verdict::Unproven {
                    tick: 7,
                    returned: None,
                },
            ),
            // The claimed tick, but another revision, another context or another cell.
            (
                CALCULATED,
                record_of(&[(1, 1, 7, 2, "k")]),
                Verdict::Unproven {
                    tick: 7,
                    returned: None,
                },
            ),
            (
                CALCULATED,
                record_of(&[(1, 1, 7, 3, "other")]),
                Verdict::Unproven {
                    tick: 7,
                    returned: None,
                },
            ),
            (
                CALCULATED,
                record_of(&[(1, 2, 7, 3, "k")]),
                Verdict::Unproven {
                    tick: 7,
                    returned: None,
                },
            ),
            (
                r#"{"stale": {"last_calculated": null, "cause": "edited"}}"#,
                evaluated(),
                Verdict::Stale {
                    source: StaleSource::Reported {
                        last_calculated: None,
                        cause: "edited".to_owned(),
                    },
                },
            ),
        ];
        for (provenance, evaluations, expected) in cases {
            let report = judge(&live, &results(provenance), &evaluations, &policy).unwrap();
            assert_eq!(
                report.cells[0].verdict, expected,
                "{provenance} {evaluations:?}"
            );
            assert_eq!(report.cells[0].value_equals_expectation, Some(true));
            assert_eq!(report.counts.calculated, 0);
            assert_eq!(report.counts.matched, 0);
            assert_eq!(report.counts.not_calculated_equal_to_expectation, 1);
            assert_eq!(
                report.counts.evaluator_calls,
                evaluations.evaluations.len() as u64
            );
        }
        let report = judge(
            &live,
            &results(CALCULATED),
            &record_of(&[(1, 1, 6, 3, "k"), (1, 1, 7, 3, "k")]),
            &policy,
        )
        .unwrap();
        assert_eq!(report.counts.calculated, 1);
        assert_eq!(report.counts.matched, 1);
        assert_eq!(report.counts.evaluator_calls, 2);
        assert_eq!(report.cells[0].value_equals_expectation, None);
    }

    /// One inventory cell per provenance: only a proven calculated cell counts as calculated, only
    /// an observed one is compared, and each other provenance is counted in its own category.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn every_provenance_is_counted_in_its_own_category_and_only_proven_cells_match() {
        // Rows 1 to 7 of column A; row 4 is not observed.
        let live = column_capture(&[RAW_TWO, RAW_TWO, RAW_TWO, "", RAW_TWO, RAW_TWO, RAW_TWO]);
        let results = column_results(&[
            (
                r#"{"not_modeled": {"feature": "LAMBDA", "diagnostic": "unsupported-function"}}"#,
                "",
            ),
            (r#"{"failed": {"fault": "admission refused"}}"#, ""),
            (r#"{"missing": {"reason": "no result published"}}"#, ""),
            (
                r#"{"calculated": {"tick": 4, "revision": 3, "context": "k"}}"#,
                TWO,
            ),
            (r#"{"file_cached": {"source": "s"}}"#, TWO),
            (
                r#"{"calculated": {"tick": 6, "revision": 3, "context": "k"}}"#,
                TWO,
            ),
            ("", ""),
        ]);
        let evaluations = record_of(&[(4, 1, 4, 3, "k"), (6, 1, 6, 3, "k")]);
        let report = judge(&live, &results, &evaluations, &exact("20430")).unwrap();
        let verdicts: Vec<Verdict> = report.cells.iter().map(|c| c.verdict.clone()).collect();
        assert_eq!(
            verdicts,
            vec![
                Verdict::NotModeled {
                    feature: "LAMBDA".to_owned(),
                    diagnostic: "unsupported-function".to_owned(),
                },
                Verdict::Failed {
                    fault: "admission refused".to_owned(),
                },
                Verdict::Missing {
                    reason: Some("no result published".to_owned()),
                },
                Verdict::NotRun,
                Verdict::FileCached {
                    source: "s".to_owned(),
                },
                Verdict::Matched {
                    certainty: Certainty::StrictBits,
                },
                Verdict::Missing { reason: None },
            ]
        );
        assert_eq!(
            report
                .cells
                .iter()
                .map(|c| c.verdict.is_calculated())
                .collect::<Vec<_>>(),
            vec![false, false, false, true, false, true, false]
        );
        assert_eq!(
            report
                .cells
                .iter()
                .map(|c| c.value_equals_expectation)
                .collect::<Vec<_>>(),
            vec![None, None, None, None, Some(true), None, None]
        );
        assert_eq!(
            report.counts,
            Counts {
                denominator: 7,
                calculated: 2,
                matched: 1,
                mismatched: 0,
                not_run: 1,
                strict_certified: 1,
                uncertified: 0,
                unproven: 0,
                stale: 0,
                file_cached: 1,
                not_modeled: 1,
                failed: 1,
                missing: 2,
                not_calculated_equal_to_expectation: 1,
                outside_inventory: 0,
                evaluator_calls: 2,
                // A version 1 record: both calculated cells are proven without a value check.
                calculated_without_value_check: 2,
                external_links: 0,
                matched_external_links: 0,
            }
        );
        assert_eq!(report.evaluation_record_version, 1);
        let encoded: serde_json::Value = serde_json::from_str(&report.encode()).unwrap();
        let names: Vec<&str> = encoded["cells"]
            .as_array()
            .unwrap()
            .iter()
            .map(|cell| cell["verdict"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "not_modeled",
                "failed",
                "missing",
                "not_run",
                "file_cached",
                "matched",
                "missing"
            ]
        );
        assert_eq!(encoded["cells"][0]["feature"], "LAMBDA");
        assert_eq!(encoded["cells"][1]["fault"], "admission refused");
        assert_eq!(encoded["cells"][2]["reason"], "no result published");
        assert!(encoded["cells"][3]["observation"].is_null());
        assert!(encoded["cells"][3].get("certainty").is_none());
        assert!(encoded["cells"][6]["reason"].is_null());
        assert_eq!(
            encoded["headline"]["calculated_of_denominator"],
            serde_json::json!([2, 7])
        );
        assert_eq!(
            encoded["headline"]["matched_of_calculated"],
            serde_json::json!([1, 2])
        );
    }

    /// Saved caches of each `t` are decoded to their typed value and compared by type.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn logical_and_text_saved_caches_are_decoded_and_compared_by_type() {
        let live = column_capture(&[
            r#""excel_saved_cache": {"t": "b", "v": "1"}"#,
            r#""excel_saved_cache": {"t": "str", "v": "ok"}"#,
            r#""excel_saved_cache": {"t": "str", "v": "ok"}"#,
            r#""excel_saved_cache": {"t": "b", "v": "0"}"#,
            r#""unknown_freshness_cache": {"t": "str", "v": ""}"#,
            r#""excel_saved_cache": {"t": "str", "v": "1"}"#,
        ]);
        let caches: Vec<(CacheCellType, &str, &CalcValue)> = live
            .observations
            .iter()
            .map(|observed| match &observed.observation {
                Observation::ExcelSavedCache(cache) | Observation::UnknownFreshnessCache(cache) => {
                    (cache.cell_type(), cache.text(), cache.value())
                }
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            caches,
            vec![
                (CacheCellType::Logical, "1", &CalcValue::logical(true)),
                (CacheCellType::Text, "ok", &text("ok")),
                (CacheCellType::Text, "ok", &text("ok")),
                (CacheCellType::Logical, "0", &CalcValue::logical(false)),
                (CacheCellType::Text, "", &text("")),
                (CacheCellType::Text, "1", &text("1")),
            ]
        );
        assert_eq!(
            SavedCache::new(CacheCellType::Text, "ok").unwrap().value(),
            &text("ok")
        );
        let calculated = |tick: u64| {
            format!(r#"{{"calculated": {{"tick": {tick}, "revision": 3, "context": "k"}}}}"#)
        };
        let provenances: Vec<String> = (1..=6).map(calculated).collect();
        let results = column_results(&[
            (&provenances[0], r#"{"logical": true}"#),
            (&provenances[1], r#"{"text": "006f006b"}"#),
            (&provenances[2], r#"{"text": "004f004b"}"#),
            (&provenances[3], r#"{"logical": true}"#),
            (&provenances[4], r#"{"text": ""}"#),
            (&provenances[5], r#"{"number": "3ff0000000000000"}"#),
        ]);
        let evaluations = record_of(
            &(1..=6)
                .map(|row| (row, 1, u64::from(row), 3, "k"))
                .collect::<Vec<_>>(),
        );
        let report = judge(&live, &results, &evaluations, &exact("20430")).unwrap();
        let saved = Certainty::Uncertified(UncertifiedReason::SavedCache);
        let verdicts: Vec<Verdict> = report.cells.iter().map(|c| c.verdict.clone()).collect();
        assert_eq!(
            verdicts,
            vec![
                Verdict::Matched { certainty: saved },
                Verdict::Matched { certainty: saved },
                Verdict::Mismatched {
                    certainty: saved,
                    class: MismatchClass::Text,
                    expected: text("ok"),
                    actual: text("OK"),
                },
                Verdict::Mismatched {
                    certainty: saved,
                    class: MismatchClass::Logical,
                    expected: CalcValue::logical(false),
                    actual: CalcValue::logical(true),
                },
                Verdict::Matched {
                    certainty: Certainty::Uncertified(UncertifiedReason::UnknownFreshness),
                },
                // The text "1" is not the number 1.
                Verdict::Mismatched {
                    certainty: saved,
                    class: MismatchClass::Kind {
                        expected: "text",
                        actual: "number",
                    },
                    expected: text("1"),
                    actual: CalcValue::number(1.0),
                },
            ]
        );
        assert_eq!(
            (
                report.counts.calculated,
                report.counts.matched,
                report.counts.mismatched,
                report.counts.uncertified,
                report.counts.strict_certified
            ),
            (6, 3, 3, 6, 0)
        );
        let encoded: serde_json::Value = serde_json::from_str(&report.encode()).unwrap();
        assert_eq!(
            encoded["cells"][0]["cache"],
            serde_json::json!({"t": "b", "v": "1"})
        );
        assert_eq!(
            encoded["cells"][1]["cache"],
            serde_json::json!({"t": "str", "v": "ok"})
        );
        assert_eq!(
            encoded["uncertified_by_reason"],
            serde_json::json!({"saved_cache": 5, "unknown_freshness": 1})
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn inconsistent_inputs_are_refused() {
        let live = capture(LIVE, ENVIRONMENT, RAW_TWO);
        let calculated = results(CALCULATED);
        let proof = evaluated();
        let policy = VerdictPolicy::default();
        let refused = |capture: &CaptureRecord,
                       results: &ResultSet,
                       proof: &EvaluationRecord,
                       needle: &str| {
            let error = judge(capture, results, proof, &policy).err().unwrap();
            assert!(
                matches!(error, JudgeError::Inconsistent { .. }),
                "{error:?}"
            );
            assert!(
                error.to_string().contains(needle),
                "{error} lacks {needle:?}"
            );
        };
        let mut other = calculated.clone();
        other.case = "d".to_owned();
        refused(&live, &other, &proof, "result set is for case");
        let mut other = calculated.clone();
        other.source = "t".to_owned();
        refused(&live, &other, &proof, "result set is for source");
        let mut other = proof.clone();
        other.case = "d".to_owned();
        refused(&live, &calculated, &other, "evaluation record is for case");
        let mut other = proof.clone();
        other.source = "t".to_owned();
        refused(
            &live,
            &calculated,
            &other,
            "evaluation record is for source",
        );
        let mut other = live.clone();
        other.sheets.push(other.sheets[0].clone());
        refused(&other, &calculated, &proof, "lists the sheet");
        let mut other = live.clone();
        other.formulas.push(other.formulas[0].clone());
        refused(&other, &calculated, &proof, "lists Sheet1!A1 twice");
        let mut other = live.clone();
        other.formulas[0].cell = CellAddress::new("Sheet2", 1, 1).unwrap();
        other.observations.clear();
        refused(&other, &calculated, &proof, "does not list");
        let mut other = live.clone();
        other.formulas[0].kind = FormulaKind::SharedChild { group: 4 };
        refused(&other, &calculated, &proof, "has 0 masters");
        let mut other = live.clone();
        other.formulas[0].kind = FormulaKind::SharedMaster { group: 4 };
        other.formulas.push(FormulaCell {
            cell: CellAddress::new("Sheet1", 2, 1).unwrap(),
            kind: FormulaKind::SharedMaster { group: 4 },
            formula: None,
            dynamic_array: false,
        });
        refused(&other, &calculated, &proof, "has 2 masters");
        let mut other = live.clone();
        other.observations[0].cell = CellAddress::new("Sheet1", 9, 9).unwrap();
        refused(&other, &calculated, &proof, "not in its formula inventory");
        let mut other = live.clone();
        other.observations.push(other.observations[0].clone());
        refused(&other, &calculated, &proof, "observes Sheet1!A1 twice");
        let mut other = live.clone();
        other.acquisition = Acquisition::FileCacheReplay;
        refused(&other, &calculated, &proof, "file-cache replay");
        let mut other = proof.clone();
        other.evaluations.push(other.evaluations[0].clone());
        refused(
            &live,
            &calculated,
            &other,
            "lists the evaluation of Sheet1!A1 at tick 7 (revision 3, context \"k\") twice",
        );
        let mut other = calculated.clone();
        other.results.push(other.results[0].clone());
        refused(&live, &other, &proof, "two results");
        let mut other = calculated.clone();
        other.results[0].value = None;
        refused(&live, &other, &proof, "carries no value");
        let mut other = calculated.clone();
        other.results[0].provenance = Provenance::NotModeled {
            feature: "f".to_owned(),
            diagnostic: "d".to_owned(),
        };
        refused(&live, &other, &proof, "carries a value");
    }

    /// An empty formula inventory is refused with its own error, not reported as 0 of 0.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn an_empty_inventory_is_refused() {
        let mut empty = capture(LIVE, ENVIRONMENT, RAW_TWO);
        empty.formulas.clear();
        empty.observations.clear();
        let error = judge(
            &empty,
            &column_results(&[]),
            &record_of(&[]),
            &VerdictPolicy::default(),
        )
        .err()
        .unwrap();
        assert_eq!(
            error,
            JudgeError::EmptyInventory {
                case: "c".to_owned()
            }
        );
        assert_eq!(
            error.to_string(),
            "the capture record of case \"c\" lists no formula cell, so there is nothing to judge"
        );
        // A result for a cell outside an empty inventory does not make it judgeable.
        let error = judge(
            &empty,
            &results(CALCULATED),
            &evaluated(),
            &VerdictPolicy::default(),
        )
        .err()
        .unwrap();
        assert!(
            matches!(error, JudgeError::EmptyInventory { .. }),
            "{error:?}"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn malformed_records_are_refused_with_their_path() {
        let good = format!(
            r#"{{"format": "dnacalc-capture-record", "version": 1, "case": "c", "source": "s",
                "acquisition": {LIVE}, "environment": null, "sheets": [], "formulas": [],
                "observations": []}}"#
        );
        assert!(CaptureRecord::decode(good.as_bytes()).is_ok());
        for (from, to, path) in [
            ("\"version\": 1", "\"version\": 3", "capture.version"),
            (
                "dnacalc-capture-record",
                "dnacalc-result-set",
                "capture.format",
            ),
            (
                "\"formulas\": []",
                "\"formulas\": [], \"extra\": 1",
                "capture",
            ),
            (
                LIVE,
                r#"{"kind": "legacy_evidence"}"#,
                "capture.acquisition",
            ),
            (
                LIVE,
                r#"{"kind": "live_capture", "entry": "x"}"#,
                "capture.acquisition",
            ),
            (
                "\"formulas\": []",
                r#""formulas": [{"sheet": "S", "row": 0, "column": 1, "kind": "ordinary"}]"#,
                "capture.formulas[0].row",
            ),
            (
                "\"formulas\": []",
                r#""formulas": [{"sheet": "S", "row": 1, "column": 1, "kind": "ordinary", "group": 1}]"#,
                "capture.formulas[0].group",
            ),
            (
                "\"observations\": []",
                r#""observations": [{"sheet": "S", "row": 1, "column": 1, "excel_raw": {"empty": true}, "unknown_freshness_cache": {"t": "n", "v": "1"}}]"#,
                "capture.observations[0]",
            ),
            (
                "\"observations\": []",
                r#""observations": [{"sheet": "S", "row": 1, "column": 1, "excel_saved_cache": {"t": "s", "v": "0"}}]"#,
                "capture.observations[0].excel_saved_cache.t",
            ),
            // The canonical record always names a cache's t, even for a number.
            (
                "\"observations\": []",
                r#""observations": [{"sheet": "S", "row": 1, "column": 1, "excel_saved_cache": {"v": "1"}}]"#,
                "capture.observations[0].excel_saved_cache",
            ),
        ] {
            let text = good.replace(from, to);
            assert_ne!(text, good, "{from} not found");
            let error = CaptureRecord::decode(text.as_bytes()).err().unwrap();
            assert_eq!(error.path, path, "{error}");
        }
        let results_text = r#"{"format": "dnacalc-result-set", "version": 1, "case": "c", "source": "s",
            "run": {"revision": 3, "context": "k"},
            "results": [{"sheet": "S", "row": 1, "column": 1, "provenance": {"file_cached": {"source": "p"}}, "value": {"number": "4000000000000000"}}]}"#;
        assert!(ResultSet::decode(results_text.as_bytes()).is_ok());
        for (from, to, path) in [
            (
                r#"{"file_cached": {"source": "p"}}"#,
                r#"{"cached": true}"#,
                "results.results[0].provenance",
            ),
            (
                r#"{"file_cached": {"source": "p"}}"#,
                r#"{"file_cached": {"source": ""}}"#,
                "results.results[0].provenance.file_cached.source",
            ),
            (
                r#""4000000000000000""#,
                "2",
                "results.results[0].value.number",
            ),
            (
                r#""revision": 3"#,
                r#""revision": -3"#,
                "results.run.revision",
            ),
            // A result set cannot carry the proof of its own calculation.
            (
                r#""run": {"revision": 3, "context": "k"},"#,
                r#""run": {"revision": 3, "context": "k"}, "evaluations": [],"#,
                "results",
            ),
        ] {
            let text = results_text.replace(from, to);
            assert_ne!(text, results_text, "{from} not found");
            let error = ResultSet::decode(text.as_bytes()).err().unwrap();
            assert_eq!(error.path, path, "{error}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_report_states_every_count_and_cell() {
        let live = capture(LIVE, ENVIRONMENT, RAW_TWO);
        let mut calculated = results(CALCULATED);
        calculated.results.push(CellResult {
            cell: CellAddress::new("Sheet1", 1, 2).unwrap(),
            provenance: Provenance::Calculated {
                tick: 7,
                revision: 3,
                context: "k".to_owned(),
                external_links: false,
            },
            value: Some(CalcValue::number(1.0)),
        });
        let report = judge(&live, &calculated, &evaluated(), &exact("20430")).unwrap();
        let encoded: serde_json::Value = serde_json::from_str(&report.encode()).unwrap();
        assert_eq!(encoded["format"], REPORT_FORMAT);
        assert_eq!(encoded["version"], 5);
        assert_eq!(encoded["evaluation_record_version"], 1);
        assert_eq!(encoded["counts"]["calculated_without_value_check"], 1);
        assert_eq!(
            encoded["headline"]["calculated_of_denominator"],
            serde_json::json!([1, 1])
        );
        assert_eq!(
            encoded["headline"]["matched_of_calculated"],
            serde_json::json!([1, 1])
        );
        assert_eq!(encoded["counts"]["strict_certified"], 1);
        assert_eq!(encoded["counts"]["outside_inventory"], 1);
        assert_eq!(encoded["counts"]["evaluator_calls"], 1);
        assert_eq!(encoded["numeric_mode"]["excel_build"], "20430");
        assert_eq!(encoded["cells"][0]["cell"], "A1");
        assert_eq!(encoded["cells"][0]["certainty"], "strict_bits");
        assert_eq!(encoded["outside_inventory"][0]["cell"], "B1");
        assert_eq!(encoded["outside_inventory"][0]["provenance"], "calculated");
        assert!(
            report
                .summary()
                .starts_with("c: calculated 1 of 1 formula cells; matched 1 of 1"),
            "{}",
            report.summary()
        );
        let saved = judge(
            &capture(
                LIVE,
                ENVIRONMENT,
                r#""excel_saved_cache": {"t": "n", "v": "2"}"#,
            ),
            &results(CALCULATED),
            &evaluated(),
            &exact("20430"),
        )
        .unwrap();
        assert!(
            saved
                .summary()
                .contains("strict-bit certified 0, uncertified 1 (saved_cache 1);"),
            "{}",
            saved.summary()
        );
    }
}
