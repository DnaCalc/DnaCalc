//! The triage of a `cargo xtask compare` run (bead `dc-kt3l`): exactly one class and one key for
//! every formula cell of the capture's inventory, read from the CellVerdict report
//! `calc_verdict::judge` computed and from what the run knows of each cell (its projected value
//! and typed refusal, and, for an evaluation whose value differs from the capture's, the reads and
//! calls the read observer was shown).
//!
//! The classes, in the order the summary lists them:
//!
//! - `matched`: calculated and equal to the capture's observation.
//! - `kernel-value` and `kernel-bits`: calculated and different from the observation, reading no
//!   other cell that differs, with a call compile did not insert. The class is inferred: the
//!   difference is taken to arise in a catalogue kernel the evaluation called, though it may lie
//!   in the formula around the call or in a constant argument, which the run does not check, and
//!   `triage.json`'s `attribution` says so. The key is the
//!   catalogue entry taken for the one whose result the formula returned: the last call the
//!   evaluator reported, leaving out the calls compile inserts. That is inferred, and
//!   `triage.json`'s `attribution` says so: the evaluator reports which entries an evaluation
//!   called, not which call's result it returned. `kernel-bits` when both values are numbers that agree
//!   to 15 significant digits (each rounded to 15 significant digits gives the same decimal),
//!   `kernel-value` otherwise. The cell carries that entry's FunctionId, every call, the values its
//!   reads were shown (at most [`READ_ENTRIES`] runs or values per evaluation, the rest counted as
//!   omitted), both values with their bits, the ulp distance and the 15-digit roundings.
//! - `formula`: CalcFml refused the formula (compile, a reference, a final sum near zero, an area
//!   used as a value, a call-interface fault), keyed by the typed refusal; or the formula's value
//!   differs from the observation with no function call at all.
//! - `function-missing`: the catalogue holds no entry of that spelling, or under Exact a catalogued
//!   function has no arithmetic (`ExactUnavailable`) or its kernel returned an omitted argument
//!   (`MissingReturned`); the key is the function's spelling, prefixed `ExactUnavailable:` or
//!   `MissingReturned:` for the two refusals of a function the catalogue holds.
//! - `not-modelled`: the catalogue holds the function and its kernel refuses the call
//!   (`NotImplemented`); the key is the function's spelling. The function is there; this call's
//!   Excel result is not modelled yet.
//!
//! Every refusal carries the work package that owns it (`owner` in `triage.json`): the bead the
//! refusal names (CalcFml's `Unsupported` construct), the package whose scope holds the function
//! it names (`owners.rs`), or, when neither names one, the library that refused. A cell reading a
//! root takes the root's owner with its class and key.
//! - `engine`: CalcEngine published no calculated value it could stand behind (a value no cell
//!   holds here, a stale or file-cached value after a successful recalculation, a calculated claim
//!   the evaluation record does not prove), or the recalculation as a whole failed.
//! - `context`: the run calculated under another calculation context than the capture's (a version
//!   2 capture's, as judge finds it), or, for a version 1 capture, which names none, than the
//!   package declares (its `date1904`); the key names the settings that differ.
//! - `doc`: calc-doc refused the package, or the workbook it loaded does not hold the formula the
//!   capture lists (no sheet of that name, no input, a literal); the key is the typed error.
//! - `capture`: the capture observed nothing for a calculated cell, or records a failure at the
//!   cell; or a refused formula reads a formula cell the capture's inventory does not list.
//!
//! Roles keep the cells where a difference starts apart from the cells it reaches. A `root` is
//! where it starts. A cell `reads_root` when it is not modelled only because it reads a formula
//! cell whose result is not calculated (CalcFml's `MissingInput` names that cell), or when it is
//! calculated and differs and reads a cell that also differs: it takes its root's class and key
//! and names the root, and the root counts it among the cells it blocks. A calculated cell counted
//! under a root keeps its own difference, since its value may also differ for a reason of its
//! own: that attribution is inferred from its reads. A `workbook` role means
//! the class belongs to the whole run (the package refused, the recalculation failed, another
//! context), not to one cell. A matched cell has no role.
//!
//! Every step is linear in the inventory or bounded by the reads of the differing cells: the
//! differing cells are indexed by sheet and column, so a read of a whole column costs one ordered
//! lookup per column holding a differing cell, not one per cell of the column.

use crate::owners::function_owner;
use calc_engine::NotModeled;
use calc_fml::{CompileError, EvalFault, Gap};
use calc_func::{FunctionFault, FunctionId, catalogue};
use calc_host::value::{CalcValue, CellKey, CoreValue, GridArea, SheetId};
use calc_verdict::{
    CaptureRecord, CellAddress, CellVerdict, MismatchClass, StaleSource, Verdict, VerdictReport,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Debug;

/// The most runs (a value, a run of blank cells or a run of cells the reader could not supply) an
/// evaluation's reads are kept with; the cells of the runs beyond it are counted as omitted.
pub(crate) const READ_ENTRIES: usize = 32;

/// The most root cells a key lists in `triage.json` (those blocking most cells first); the rest
/// are counted.
const KEY_ROOT_CELLS: usize = 100;

/// The most keys and, per key, root cells the printed summary names.
const SUMMARY_KEYS: usize = 12;
const SUMMARY_ROOTS: usize = 3;

/// The class of an inventory cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Class {
    Matched,
    KernelValue,
    KernelBits,
    Formula,
    FunctionMissing,
    NotModelled,
    Engine,
    Context,
    Doc,
    Capture,
}

impl Class {
    pub(crate) const ALL: [Class; 10] = [
        Class::Matched,
        Class::KernelValue,
        Class::KernelBits,
        Class::Formula,
        Class::FunctionMissing,
        Class::NotModelled,
        Class::Engine,
        Class::Context,
        Class::Doc,
        Class::Capture,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Class::Matched => "matched",
            Class::KernelValue => "kernel-value",
            Class::KernelBits => "kernel-bits",
            Class::Formula => "formula",
            Class::FunctionMissing => "function-missing",
            Class::NotModelled => "not-modelled",
            Class::Engine => "engine",
            Class::Context => "context",
            Class::Doc => "doc",
            Class::Capture => "capture",
        }
    }
}

/// Where a cell stands relative to the difference its class names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    Root,
    ReadsRoot,
    Workbook,
}

impl Role {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Role::Root => "root",
            Role::ReadsRoot => "reads_root",
            Role::Workbook => "workbook",
        }
    }
}

/// What the run made of one inventory cell, apart from the verdict.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Dna {
    /// `HostState::open` refused the package; the run's [`WorkbookFailure`] says why.
    OpenRefused,
    /// The workbook calc-doc loaded has no sheet of the cell's sheet name.
    SheetNotLoaded,
    /// The loaded workbook holds no input at the cell.
    Empty,
    /// The loaded workbook holds a literal at the cell.
    Literal,
    /// The run rejects the cell's formula, as it says Excel's entry does (dc-mx3x).
    Rejected,
    FileCached,
    Calculated,
    /// The engine published a typed refusal for the cell.
    Refused(Refusal),
    Stale,
}

/// A typed refusal as the triage reads it: its class and key, the refusal's own name, the
/// feature `report.json` names for it, and, for CalcFml's `MissingInput`, the formula cell whose
/// missing result stopped the evaluation.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Refusal {
    pub(crate) class: Class,
    pub(crate) key: String,
    pub(crate) refusal: String,
    /// The typed refusal with what it concerns (`Compile:UnknownFunction (NOSUCHFUNCTION)`,
    /// `Evaluation:MissingInput (reads Calc!B1)`): the `feature` of the cell's not-modelled
    /// provenance in `report.json`. Unlike the key, it never depends on the capture's inventory.
    pub(crate) feature: String,
    pub(crate) reads: Option<CellAddress>,
    /// The work package that owns what is refused: the bead the refusal itself names, the package
    /// whose scope holds the function it names (`owners.rs`), or the library that refused when
    /// neither names one.
    pub(crate) owner: String,
}

impl Refusal {
    fn new(class: Class, key: impl Into<String>, refusal: impl Into<String>) -> Refusal {
        let refusal = refusal.into();
        Refusal {
            class,
            key: key.into(),
            feature: refusal.clone(),
            refusal,
            reads: None,
            owner: String::new(),
        }
    }

    /// The refusal owned by `owner`.
    fn owned_by(mut self, owner: impl Into<String>) -> Refusal {
        self.owner = owner.into();
        self
    }

    /// The refusal owned by the package whose scope holds the function `spelling`.
    fn owned_by_function(self, spelling: &str) -> Refusal {
        self.owned_by(function_owner(spelling))
    }

    /// The refusal with what it concerns named in its feature.
    fn about(mut self, subject: impl std::fmt::Display) -> Refusal {
        self.feature = format!("{} ({subject})", self.refusal);
        self
    }
}

/// The name of an enum value's variant: its `Debug` text up to the first character that cannot
/// be part of an identifier (`Cell { .. }` gives `Cell`).
pub(crate) fn variant(value: &impl Debug) -> String {
    let text = format!("{value:?}");
    text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// The spelling of the catalogue entry `id`, or its `Debug` form when the catalogue does not
/// resolve it.
fn spelling(id: FunctionId) -> String {
    catalogue()
        .resolve(id)
        .map(|target| target.spelling().to_owned())
        .unwrap_or_else(|_| format!("{id:?}"))
}

/// The kind of a value CalcEngine refused to publish as calculated.
fn value_kind(value: &CalcValue) -> &'static str {
    if value.callable_value().is_some() {
        return "callable";
    }
    match value.core() {
        CoreValue::Number(_) => "number not finite",
        CoreValue::Text(_) => "text",
        CoreValue::Logical(_) => "logical",
        CoreValue::Error(_) => "error",
        CoreValue::Empty => "empty",
        CoreValue::Missing => "omitted argument",
        CoreValue::Array(_) => "array",
        CoreValue::Reference(_) => "reference",
    }
}

/// The owner of a refusal no bead and no function names: the library that refused.
const CALC_FML: &str = "no package named (calc-fml refused)";
const CALC_ENGINE: &str = "no package named (calc-engine refused)";

/// The owner CalcFml's documentation gives `CompileError::NumberLiteral`, which the variant does
/// not carry.
const NUMBER_LITERAL_OWNER: &str = "dc-10y";

/// The class, key and owner of the engine's typed refusal `refusal`, with `address` naming a cell
/// for CalcFml's `MissingInput`.
pub(crate) fn refusal_of(
    refusal: &NotModeled,
    address: &dyn Fn(CellKey) -> Option<CellAddress>,
) -> Refusal {
    match refusal {
        NotModeled::FormulaText => {
            Refusal::new(Class::Formula, "FormulaText", "FormulaText").owned_by(CALC_ENGINE)
        }
        NotModeled::Compile(error) => match error {
            CompileError::NotModelled {
                gap: Gap::UnknownFunction { spelling },
                ..
            } => {
                let spelling = spelling.to_ascii_uppercase();
                Refusal::new(
                    Class::FunctionMissing,
                    spelling.clone(),
                    "Compile:UnknownFunction",
                )
                .owned_by_function(&spelling)
                .about(spelling)
            }
            CompileError::NotModelled {
                gap: Gap::Construct { construct, owner },
                ..
            } => Refusal::new(
                Class::Formula,
                format!("Compile:Unsupported:{construct} ({owner})"),
                "Compile:Unsupported",
            )
            .owned_by(*owner)
            .about(format!("{construct}, {owner}")),
            CompileError::Reference { error, .. } => {
                let key = format!("Compile:Reference:{}", variant(error));
                Refusal::new(Class::Formula, key.clone(), key).owned_by(CALC_FML)
            }
            CompileError::NotModelled {
                gap: Gap::NumberLiteral,
                ..
            } => Refusal::new(
                Class::Formula,
                "Compile:NumberLiteral",
                "Compile:NumberLiteral",
            )
            .owned_by(NUMBER_LITERAL_OWNER),
            CompileError::Rejected { rule, .. } => {
                let key = format!("Compile:Rejected:{rule}");
                Refusal::new(Class::Formula, key.clone(), key).owned_by(CALC_FML)
            }
        },
        NotModeled::Evaluation(fault) => match fault {
            // The key holds only while the triage finds the cell read outside the inventory; a
            // cell whose read is an inventory cell takes that cell's root instead.
            EvalFault::MissingInput { cell } => {
                let reads = address(*cell);
                let read = reads.as_ref().map_or_else(
                    || format!("reads {cell:?}"),
                    |read| format!("reads {}!{}", read.sheet(), read.a1()),
                );
                Refusal {
                    reads,
                    ..Refusal::new(
                        Class::Capture,
                        "reads a formula cell outside the capture's inventory",
                        "Evaluation:MissingInput",
                    )
                    .owned_by("no package (the capture's inventory)")
                    .about(read)
                }
            }
            EvalFault::Function(fault) => match fault {
                // The catalogue holds the function and its kernel refuses this call: not
                // modelled, under the package that owns the function, never a missing function.
                FunctionFault::NotImplemented { id, argument } => {
                    let name = spelling(*id);
                    let about = match argument {
                        Some(index) => format!("{name}, argument {}", index + 1),
                        None => name.clone(),
                    };
                    Refusal::new(
                        Class::NotModelled,
                        name.clone(),
                        "Evaluation:Function:NotImplemented",
                    )
                    .owned_by_function(&name)
                    .about(about)
                }
                // Keyed apart from a missing function: the catalogue holds the function, and the
                // run's Exact numeric mode has no arithmetic for it yet.
                FunctionFault::ExactUnavailable { id } => Refusal::new(
                    Class::FunctionMissing,
                    format!("ExactUnavailable:{}", spelling(*id)),
                    "Evaluation:Function:ExactUnavailable",
                )
                .owned_by_function(&spelling(*id))
                .about(spelling(*id)),
                FunctionFault::MissingReturned { id } => Refusal::new(
                    Class::FunctionMissing,
                    format!("MissingReturned:{}", spelling(*id)),
                    "Evaluation:Function:MissingReturned",
                )
                .owned_by_function(&spelling(*id))
                .about(spelling(*id)),
                other => {
                    let key = format!("Evaluation:Function:{}", variant(other));
                    Refusal::new(Class::Formula, key.clone(), key).owned_by(CALC_FML)
                }
            },
            EvalFault::ReadContract(_) => Refusal::new(
                Class::Engine,
                "Evaluation:ReadContract",
                "Evaluation:ReadContract",
            )
            .owned_by(CALC_ENGINE),
            other => {
                let key = format!("Evaluation:{}", variant(other));
                Refusal::new(Class::Formula, key.clone(), key).owned_by(CALC_FML)
            }
        },
        NotModeled::Result(value) => Refusal::new(
            Class::Engine,
            format!("Result:{}", value_kind(value)),
            "Result",
        )
        .owned_by(CALC_ENGINE),
    }
}

/// Why the run as a whole produced no calculated value: the package refused (`doc`) or the
/// recalculation failed (`engine`), with the key and the typed error's text.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WorkbookFailure {
    pub(crate) class: Class,
    pub(crate) key: String,
    pub(crate) message: String,
}

/// One catalogue call an evaluation ran, as CalcFml's evaluator reported it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Call {
    pub(crate) id: FunctionId,
    pub(crate) spelling: String,
    /// The catalogue's notation (`Call`, `Prefix`, `Infix`, `Postfix`).
    pub(crate) notation: String,
    /// Compile inserted the call (the near-zero check's negations).
    pub(crate) synthetic: bool,
}

impl Call {
    /// The key a difference in this call's result is counted under: the spelling of a named
    /// call, the notation and spelling of an operator (`Infix +`).
    fn key(&self) -> String {
        if self.notation == "Call" {
            self.spelling.clone()
        } else {
            format!("{} {}", self.notation, self.spelling)
        }
    }

    fn json(&self) -> Value {
        json!({
            "id": format!("{:?}", self.id),
            "raw": self.id.raw(),
            "spelling": self.spelling,
            "notation": self.notation,
            "synthetic": self.synthetic,
        })
    }
}

/// One run of a read as the read observer was shown it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RunEntry {
    Value(CalcValue),
    Blank(u64),
    /// Cells the reader could not supply.
    Missing(u64),
}

/// One area an evaluation read: its sheet and bounds, its cell count, the runs kept, and the
/// cells of the runs not kept ([`READ_ENTRIES`]).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Read {
    pub(crate) sheet: String,
    pub(crate) top: u32,
    pub(crate) left: u32,
    pub(crate) bottom: u32,
    pub(crate) right: u32,
    pub(crate) cells: u64,
    pub(crate) runs: Vec<RunEntry>,
    pub(crate) omitted: u64,
}

impl Read {
    fn area(&self) -> String {
        let corner = |row, column| {
            CellAddress::new(&self.sheet, row, column)
                .map(|cell| cell.a1())
                .unwrap_or_else(|| format!("R{row}C{column}"))
        };
        let first = corner(self.top, self.left);
        if (self.top, self.left) == (self.bottom, self.right) {
            format!("{}!{first}", self.sheet)
        } else {
            format!("{}!{first}:{}", self.sheet, corner(self.bottom, self.right))
        }
    }

    fn json(&self) -> Value {
        let runs: Vec<Value> = self
            .runs
            .iter()
            .map(|run| match run {
                RunEntry::Value(value) => value_json(value),
                RunEntry::Blank(count) => json!({ "blank": count }),
                RunEntry::Missing(count) => json!({ "unsupplied": count }),
            })
            .collect();
        json!({
            "area": self.area(),
            "cells": self.cells,
            "runs": runs,
            "omitted_cells": self.omitted,
        })
    }
}

/// What the read observer kept of an evaluation whose value differs from the capture's: every
/// call it ran and the areas it read.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Retained {
    /// A recorded Excel entry refusal reached through the application's graph.
    /// It explains a failing comparison; it supplies no calculated value.
    pub(crate) entry_failure: Option<CellAddress>,
    pub(crate) calls: Vec<Call>,
    pub(crate) reads: Vec<Read>,
    /// The application's existing graph also records shape-only dependencies,
    /// which need no value read and therefore do not appear in `reads`.
    pub(crate) precedents: Vec<GridArea>,
}

/// A value as `triage.json` writes it: a number as its bits in hex and its shortest decimal.
pub(crate) fn value_json(value: &CalcValue) -> Value {
    if value.callable_value().is_some() {
        return json!({ "callable": true });
    }
    match value.core() {
        CoreValue::Number(number) => json!({
            "number": format!("{:016x}", number.to_bits()),
            "decimal": format!("{number:?}"),
        }),
        CoreValue::Text(text) => json!({ "text": text.to_string_lossy() }),
        CoreValue::Logical(logical) => json!({ "logical": logical }),
        CoreValue::Error(code) => json!({ "error": format!("{code:?}") }),
        CoreValue::Empty => json!({ "empty": true }),
        CoreValue::Missing => json!({ "omitted": true }),
        CoreValue::Array(array) => json!({
            "array": { "rows": array.shape().rows, "columns": array.shape().cols }
        }),
        CoreValue::Reference(_) => json!({ "reference": true }),
    }
}

/// A number rounded to 15 significant digits, with -0 written as 0.
fn digits15(number: f64) -> String {
    let number = if number == 0.0 { 0.0 } else { number };
    format!("{number:.14e}")
}

/// The difference of a calculated cell from its observation.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Difference {
    /// The call whose result the formula returned; `None` when the evaluation ran no call.
    pub(crate) function: Option<Call>,
    pub(crate) calls: Vec<Call>,
    pub(crate) reads: Vec<Read>,
    pub(crate) expected: CalcValue,
    pub(crate) actual: CalcValue,
    pub(crate) ulp: Option<u64>,
    /// Both values rounded to 15 significant digits, when both are finite numbers.
    pub(crate) digits15: Option<(String, String)>,
}

impl Difference {
    fn agrees_to_15_digits(&self) -> Option<bool> {
        self.digits15
            .as_ref()
            .map(|(expected, actual)| expected == actual)
    }

    fn json(&self) -> Value {
        let mut read_types = BTreeSet::new();
        for read in &self.reads {
            for run in &read.runs {
                let kind = match run {
                    RunEntry::Blank(_) => "blank",
                    RunEntry::Missing(_) => "unsupplied",
                    RunEntry::Value(value) => match value.core() {
                        CoreValue::Number(_) => "number",
                        CoreValue::Text(_) => "text",
                        CoreValue::Logical(_) => "logical",
                        CoreValue::Error(_) => "error",
                        CoreValue::Empty => "empty",
                        CoreValue::Missing => "omitted",
                        CoreValue::Array(_) => "array",
                        CoreValue::Reference(_) => "reference",
                    },
                };
                read_types.insert(kind);
            }
        }
        json!({
            "function": self.function.as_ref().map(Call::json),
            "calls": self.calls.iter().map(Call::json).collect::<Vec<_>>(),
            "reads": self.reads.iter().map(Read::json).collect::<Vec<_>>(),
            "observed_read_types": read_types,
            "expected": value_json(&self.expected),
            "actual": value_json(&self.actual),
            "ulp": self.ulp,
            "agrees_to_15_significant_digits": self.agrees_to_15_digits(),
            "expected_15": self.digits15.as_ref().map(|(expected, _)| expected),
            "actual_15": self.digits15.as_ref().map(|(_, actual)| actual),
        })
    }
}

/// One inventory cell's triage.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CellTriage {
    pub(crate) class: Class,
    pub(crate) key: String,
    pub(crate) role: Option<Role>,
    /// The inventory index of the root a `reads_root` cell takes its class from.
    pub(crate) root: Option<usize>,
    /// For a root, the cells that read it and take its class.
    pub(crate) blocks: u64,
    pub(crate) detail: Option<String>,
    pub(crate) difference: Option<Difference>,
    /// For a refused root and the cells reading it, the refusal's owner ([`Refusal::owner`]).
    pub(crate) owner: Option<String>,
}

impl CellTriage {
    fn new(class: Class, key: impl Into<String>, role: Option<Role>) -> CellTriage {
        CellTriage {
            class,
            key: key.into(),
            role,
            root: None,
            blocks: 0,
            detail: None,
            difference: None,
            owner: None,
        }
    }

    fn root(class: Class, key: impl Into<String>) -> CellTriage {
        CellTriage::new(class, key, Some(Role::Root))
    }

    fn with_detail(mut self, detail: impl Into<String>) -> CellTriage {
        self.detail = Some(detail.into());
        self
    }
}

/// The triage of every inventory cell, in the inventory's order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Triage {
    pub(crate) cells: Vec<CellTriage>,
}

/// The settings that differ between the capture's context fingerprint and the run's, as one key.
pub(crate) fn context_key(captured: &str, run: &str) -> String {
    let pairs = |text: &'_ str| -> BTreeMap<String, String> {
        text.split(';')
            .filter_map(|pair| pair.split_once('='))
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .collect()
    };
    let (captured_pairs, run_pairs) = (pairs(captured), pairs(run));
    let names: BTreeSet<&String> = captured_pairs.keys().chain(run_pairs.keys()).collect();
    let differs: Vec<String> = names
        .into_iter()
        .filter(|name| captured_pairs.get(*name) != run_pairs.get(*name))
        .map(|name| {
            format!(
                "{name}: capture {}, run {}",
                captured_pairs.get(name).map_or("absent", String::as_str),
                run_pairs.get(name).map_or("absent", String::as_str)
            )
        })
        .collect();
    if differs.is_empty() {
        format!("capture context {captured}, run context {run}")
    } else {
        differs.join("; ")
    }
}

/// The differing cells by sheet, column and row.
type DifferingIndex<'a> = HashMap<&'a str, BTreeMap<u32, BTreeSet<u32>>>;

/// The first differing cell of `read` in row-major order other than `own`, if any.
fn first_differing(
    index: &DifferingIndex<'_>,
    read: &Read,
    own: &CellAddress,
) -> Option<CellAddress> {
    let columns = index.get(read.sheet.as_str())?;
    let mut first: Option<(u32, u32)> = None;
    for (&column, rows) in columns.range(read.left..=read.right) {
        let found = rows.range(read.top..=read.bottom).find(|&&row| {
            !(read.sheet == own.sheet() && row == own.row() && column == own.column())
        });
        if let Some(&row) = found
            && first.is_none_or(|best| (row, column) < best)
        {
            first = Some((row, column));
        }
    }
    first.and_then(|(row, column)| CellAddress::new(&read.sheet, row, column))
}

/// The difference of a mismatched cell from its observation, with what the read observer kept of
/// its evaluation, or `None` when it kept nothing.
fn difference_of(cell: &CellVerdict, retained: Option<&Retained>) -> Option<Difference> {
    let Verdict::Mismatched {
        class,
        expected,
        actual,
        ..
    } = &cell.verdict
    else {
        unreachable!("only a mismatched cell has a difference");
    };
    let retained = retained?;
    let ulp = match class {
        MismatchClass::Number { ulp } => *ulp,
        _ => None,
    };
    let digits = match (expected.core(), actual.core()) {
        (CoreValue::Number(e), CoreValue::Number(a)) if e.is_finite() && a.is_finite() => {
            Some((digits15(*e), digits15(*a)))
        }
        _ => None,
    };
    // Inferred: the evaluator reports which entries an evaluation called, not which call's result
    // the formula returned, so the last call it reported that compile did not insert stands for
    // it.
    let function = retained
        .calls
        .iter()
        .rev()
        .find(|call| !call.synthetic)
        .cloned();
    Some(Difference {
        function,
        calls: retained.calls.clone(),
        reads: retained.reads.clone(),
        expected: expected.clone(),
        actual: actual.clone(),
        ulp,
        digits15: digits,
    })
}

/// The triage of a mismatched cell that reads no other differing cell: its kernel's difference,
/// from what the read observer kept of its evaluation.
fn difference_root(difference: Option<Difference>) -> CellTriage {
    let Some(difference) = difference else {
        return CellTriage::root(Class::Engine, "mismatch without evaluator evidence").with_detail(
            "the read observer kept no reads or calls for this cell's evaluation, though the verdict found its value different from the capture's",
        );
    };
    let triage = match &difference.function {
        None => CellTriage::root(Class::Formula, "value differs with no function call"),
        Some(call) if difference.agrees_to_15_digits() == Some(true) => {
            CellTriage::root(Class::KernelBits, call.key())
        }
        Some(call) => CellTriage::root(Class::KernelValue, call.key()),
    };
    CellTriage {
        difference: Some(difference),
        ..triage
    }
}

/// One cell's own triage, before the cells that read a root take its class: `Err(parent)` for a
/// refused cell whose evaluation stopped at inventory cell `parent`, and `None` for a mismatched
/// cell decided once the differing cells are known. `context` is the key of the settings in which
/// a version 2 capture's context differs from the run's; `declared`, for a version 1 capture, the
/// key of the settings the package declares and the run does not apply.
fn own_triage(
    cell: &CellVerdict,
    dna: &Dna,
    failure_stage: Option<&str>,
    context: Option<&str>,
    declared: Option<&str>,
    index: &HashMap<&CellAddress, usize>,
) -> Option<Result<CellTriage, usize>> {
    let capture_failure =
        |stage: &str| CellTriage::root(Class::Capture, format!("capture failure at stage {stage}"));
    // A version 1 capture names no context, so judge compares a value calculated under another
    // context than the package declares as it would any other; the triage classes it as judge
    // classes a version 2 capture's (`ContextNotRun`).
    if let (Some(declared), Dna::Calculated) = (declared, dna)
        && matches!(
            cell.verdict,
            Verdict::Matched { .. }
                | Verdict::Mismatched { .. }
                | Verdict::NotRun
                | Verdict::Unproven { .. }
        )
    {
        return Some(Ok(CellTriage::new(
            Class::Context,
            declared,
            Some(Role::Workbook),
        )
        .with_detail(format!(
            "calculated under a context the package does not declare (verdict {})",
            cell.verdict.name()
        ))));
    }
    Some(Ok(match &cell.verdict {
        Verdict::Matched { .. } => CellTriage::new(Class::Matched, "matched", None),
        Verdict::Mismatched { .. } => match failure_stage {
            Some(stage) => capture_failure(stage),
            None => return None,
        },
        Verdict::NotRun => match failure_stage {
            Some(stage) => capture_failure(stage),
            None => CellTriage::root(Class::Capture, "not observed")
                .with_detail("calculated, but the capture holds no observation for the cell"),
        },
        Verdict::Unproven { returned, .. } => CellTriage::root(
            Class::Engine,
            match returned {
                None => "unproven: no evaluation in the evaluation record",
                Some(Ok(_)) => "unproven: the evaluation returned another value",
                Some(Err(_)) => "unproven: the evaluation returned no value",
            },
        ),
        Verdict::Stale { source } => match source {
            StaleSource::ContextNotRun { .. } => CellTriage::new(
                Class::Context,
                context.unwrap_or("the result's context is not the run's"),
                Some(Role::Workbook),
            )
            .with_detail("calculated under the run's context, which is not the capture's"),
            StaleSource::RevisionNotRun { revision } => {
                CellTriage::root(Class::Engine, "stale: revision not run")
                    .with_detail(format!("calculated at revision {revision}"))
            }
            StaleSource::Reported { cause, .. } => {
                CellTriage::root(Class::Engine, format!("stale: {cause}"))
            }
        },
        Verdict::FileCached { .. } => CellTriage::root(Class::Engine, "file cache not replaced")
            .with_detail("the recalculation published, and the cell still projects the package's cached value"),
        Verdict::NotModeled { diagnostic, .. } => match dna {
            Dna::Refused(refusal) => {
                if let Some(parent) = refusal.reads.as_ref().and_then(|reads| index.get(reads)) {
                    return Some(Err(*parent));
                }
                CellTriage {
                    owner: Some(refusal.owner.clone()),
                    ..CellTriage::root(refusal.class, refusal.key.clone())
                        .with_detail(format!("{}: {diagnostic}", refusal.refusal))
                }
            }
            _ => CellTriage::root(Class::Engine, "not modelled").with_detail(diagnostic.clone()),
        },
        Verdict::Failed { fault } => {
            CellTriage::root(Class::Engine, "failed").with_detail(fault.clone())
        }
        Verdict::AgreedRejection { .. } => {
            CellTriage::new(Class::Matched, "agreed rejection", None)
        }
        Verdict::EntryAccepted { .. } => {
            CellTriage::root(Class::Formula, "entry accepted where Excel refused it")
                .with_detail("calculated, and Excel refused to enter the formula")
        }
        Verdict::EntryRejectedWrongly { rule } => {
            CellTriage::root(Class::Formula, "entry rejected where Excel took it")
                .with_detail(format!("rejected by {rule}, and Excel took the formula"))
        }
        Verdict::RejectionNotRun { rule } => CellTriage::root(Class::Capture, "not observed")
            .with_detail(format!(
                "rejected by {rule}, and the capture holds no observation for the cell"
            )),
        Verdict::Missing { reason } => {
            let (class, key) = match dna {
                Dna::SheetNotLoaded => (Class::Doc, "sheet not loaded"),
                Dna::Empty => (Class::Doc, "no input at a formula cell of the inventory"),
                Dna::Literal => (Class::Doc, "a literal at a formula cell of the inventory"),
                _ => (Class::Engine, "missing"),
            };
            let triage = CellTriage::root(class, key);
            match reason {
                Some(reason) => triage.with_detail(reason.clone()),
                None => triage,
            }
        }
    }))
}

/// The triage of every inventory cell of `report` (judged against `capture`), with `dna` the
/// run's account of each inventory cell in the inventory's order, `retained` what the read
/// observer kept of the evaluations whose value differs from the capture's, `workbook` the
/// failure of the whole run, if any, which every cell then takes, and `declared`, for a version 1
/// capture, the key of the context settings the package declares and the run does not apply,
/// which every calculated cell then takes.
pub(crate) fn triage(
    capture: &CaptureRecord,
    report: &VerdictReport,
    dna: &[Dna],
    retained: &HashMap<CellAddress, Retained>,
    workbook: Option<&WorkbookFailure>,
    declared: Option<&str>,
) -> Triage {
    assert_eq!(
        report.cells.len(),
        dna.len(),
        "the run accounts for each inventory cell once"
    );
    if let Some(failure) = workbook {
        let cells = report
            .cells
            .iter()
            .map(|cell| {
                CellTriage::new(failure.class, failure.key.clone(), Some(Role::Workbook))
                    .with_detail(format!(
                        "{} (verdict {})",
                        failure.message,
                        cell.verdict.name()
                    ))
            })
            .collect();
        return Triage { cells };
    }
    let index: HashMap<&CellAddress, usize> = report
        .cells
        .iter()
        .enumerate()
        .map(|(i, cell)| (&cell.formula.cell, i))
        .collect();
    let mut failure_stage: HashMap<&CellAddress, &str> = HashMap::new();
    for failure in capture.session.iter().flat_map(|session| &session.failures) {
        if let Some(cell) = &failure.cell {
            failure_stage.entry(cell).or_insert(failure.stage.as_str());
        }
    }
    let context = report
        .captured
        .as_ref()
        .map(|captured| context_key(&captured.context, &report.run.context));

    let n = report.cells.len();
    let mut cells: Vec<Option<CellTriage>> = Vec::with_capacity(n);
    let mut parent: Vec<Option<usize>> = vec![None; n];
    let mut pending: Vec<usize> = Vec::new();
    let mut differing: DifferingIndex<'_> = HashMap::new();
    for (i, (cell, dna)) in report.cells.iter().zip(dna).enumerate() {
        let address = &cell.formula.cell;
        if matches!(cell.verdict, Verdict::Mismatched { .. }) {
            differing
                .entry(address.sheet())
                .or_default()
                .entry(address.column())
                .or_default()
                .insert(address.row());
        }
        match own_triage(
            cell,
            dna,
            failure_stage.get(address).copied(),
            context.as_deref(),
            declared,
            &index,
        ) {
            Some(Ok(triage)) => cells.push(Some(triage)),
            Some(Err(read)) => {
                parent[i] = Some(read);
                cells.push(None);
            }
            None => {
                pending.push(i);
                cells.push(None);
            }
        }
    }

    // A mismatched cell reading another differing cell takes that cell's root, keeping its own
    // difference (the attribution is inferred: its value may also differ for a reason of its
    // own); one reading none is a root of its own difference.
    let mut own_difference: Vec<Option<Difference>> = vec![None; n];
    for &i in &pending {
        let cell = &report.cells[i];
        let kept = retained.get(&cell.formula.cell);
        let difference = difference_of(cell, kept);
        let entry_failure = kept.and_then(|kept|kept.entry_failure.as_ref());
        let read = entry_failure.cloned().or_else(||kept.and_then(|kept| {
            kept.reads
                .iter()
                .find_map(|read| first_differing(&differing, read, &cell.formula.cell))
        }));
        match read.and_then(|read| index.get(&read).copied()) {
            Some(read) => {
                parent[i] = Some(read);
                own_difference[i] = difference;
            }
            None => cells[i] = Some(difference_root(difference)),
        }
    }

    // Each cell with a parent takes the class and key of the root its chain of parents ends at.
    const UNSEEN: u8 = 0;
    const WALKING: u8 = 1;
    const DONE: u8 = 2;
    let mut state = vec![UNSEEN; n];
    let mut root_of: Vec<Option<usize>> = vec![None; n];
    for start in 0..n {
        if parent[start].is_none() || state[start] == DONE {
            continue;
        }
        let mut chain = Vec::new();
        let mut at = start;
        let root = loop {
            match parent[at] {
                None => break Some(at),
                Some(_) if state[at] == DONE => break root_of[at],
                Some(_) if state[at] == WALKING => break None,
                Some(next) => {
                    state[at] = WALKING;
                    chain.push(at);
                    at = next;
                }
            }
        };
        for link in chain {
            state[link] = DONE;
            root_of[link] = root;
        }
    }
    for i in 0..n {
        let Some(read) = parent[i] else { continue };
        let read_at = &report.cells[read].formula.cell;
        cells[i] = Some(match root_of[i] {
            Some(root) => {
                let (class, key, owner) = {
                    let root_triage = cells[root].as_ref().expect("a root is decided first");
                    (
                        root_triage.class,
                        root_triage.key.clone(),
                        root_triage.owner.clone(),
                    )
                };
                if let Some(root_triage) = cells[root].as_mut() {
                    root_triage.blocks += 1;
                }
                let root_at = &report.cells[root].formula.cell;
                let difference = own_difference[i].take();
                let own = if difference.is_some() {
                    "; its own value also differs, and is counted under the root by inference"
                } else {
                    ""
                };
                CellTriage {
                    root: Some(root),
                    difference,
                    owner,
                    ..CellTriage::new(class, key, Some(Role::ReadsRoot)).with_detail(format!(
                        "reads {}!{}, whose root is {}!{}{own}",
                        read_at.sheet(),
                        read_at.a1(),
                        root_at.sheet(),
                        root_at.a1()
                    ))
                }
            }
            None => CellTriage {
                difference: own_difference[i].take(),
                ..CellTriage::root(Class::Engine, "a cycle of reads among differing cells")
                    .with_detail(format!("reads {}!{}", read_at.sheet(), read_at.a1()))
            },
        });
    }
    Triage {
        cells: cells
            .into_iter()
            .map(|cell| cell.expect("every inventory cell is triaged"))
            .collect(),
    }
}

/// The cells of one class and key: its roots (inventory indexes), the cells reading them, and
/// the cells whose class belongs to the whole run.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct KeyCount {
    pub(crate) roots: Vec<usize>,
    pub(crate) blocked: u64,
    pub(crate) workbook: u64,
    /// The owner of the key's refusal, when its cells carry one.
    pub(crate) owner: Option<String>,
}

impl KeyCount {
    fn cells(&self) -> u64 {
        self.roots.len() as u64 + self.blocked + self.workbook
    }
}

/// `cell` as `Sheet!A1`.
fn place(cell: &CellVerdict) -> String {
    format!("{}!{}", cell.formula.cell.sheet(), cell.formula.cell.a1())
}

/// A count with its noun.
fn counted(count: u64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

impl Triage {
    /// The cells of each class and key, largest first.
    pub(crate) fn keys(&self) -> Vec<((Class, String), KeyCount)> {
        let mut keys: BTreeMap<(Class, String), KeyCount> = BTreeMap::new();
        for (i, cell) in self.cells.iter().enumerate() {
            if cell.class == Class::Matched {
                continue;
            }
            let entry = keys.entry((cell.class, cell.key.clone())).or_default();
            if entry.owner.is_none() {
                entry.owner.clone_from(&cell.owner);
            }
            match cell.role {
                Some(Role::Root) | None => entry.roots.push(i),
                Some(Role::ReadsRoot) => entry.blocked += 1,
                Some(Role::Workbook) => entry.workbook += 1,
            }
        }
        let mut keys: Vec<((Class, String), KeyCount)> = keys.into_iter().collect();
        for (_, count) in &mut keys {
            count
                .roots
                .sort_by_key(|&root| std::cmp::Reverse(self.cells[root].blocks));
        }
        keys.sort_by(|a, b| b.1.cells().cmp(&a.1.cells()).then_with(|| a.0.cmp(&b.0)));
        keys
    }

    /// The number of cells of each class, with how many are roots and how many read a root, for
    /// the cells `select` admits.
    fn by_class(&self, select: impl Fn(usize) -> bool) -> BTreeMap<Class, (u64, u64, u64)> {
        let mut counts: BTreeMap<Class, (u64, u64, u64)> = BTreeMap::new();
        for (_, cell) in self.cells.iter().enumerate().filter(|(i, _)| select(*i)) {
            let entry = counts.entry(cell.class).or_default();
            entry.0 += 1;
            match cell.role {
                Some(Role::Root) => entry.1 += 1,
                Some(Role::ReadsRoot) => entry.2 += 1,
                _ => {}
            }
        }
        counts
    }

    fn class_line(counts: &BTreeMap<Class, (u64, u64, u64)>) -> String {
        let parts: Vec<String> = Class::ALL
            .iter()
            .filter_map(|class| {
                let (cells, roots, reading) = counts.get(class)?;
                let roles = if *roots + *reading > 0 && (*reading > 0 || *roots != *cells) {
                    format!(
                        " ({}, {})",
                        counted(*roots, "root", "roots"),
                        counted(*reading, "reading a root", "reading a root")
                    )
                } else {
                    String::new()
                };
                Some(format!("{} {cells}{roles}", class.name()))
            })
            .collect();
        if parts.is_empty() {
            "none".to_owned()
        } else {
            parts.join(", ")
        }
    }

    /// The summary lines: classes, mismatched cells by class, and the not-modelled cells by the
    /// key of their root with the cells each root blocks.
    pub(crate) fn summary(&self, report: &VerdictReport) -> Vec<String> {
        let mut lines = vec![format!(
            "classes: {}",
            Self::class_line(&self.by_class(|_| true))
        )];
        let mismatched = |i: usize| matches!(report.cells[i].verdict, Verdict::Mismatched { .. });
        lines.push(format!(
            "mismatched by class: {}",
            Self::class_line(&self.by_class(mismatched))
        ));
        let not_modeled: Vec<((Class, String), KeyCount)> =
            self.keys()
                .into_iter()
                .filter(|(_, count)| {
                    count.roots.iter().any(|&root| {
                        matches!(report.cells[root].verdict, Verdict::NotModeled { .. })
                    })
                })
                .collect();
        if not_modeled.is_empty() {
            lines.push("not modelled by root key: none".to_owned());
        } else {
            lines.push("not modelled by root key (roots, then the cells reading them):".to_owned());
            for ((class, key), count) in not_modeled.iter().take(SUMMARY_KEYS) {
                let roots: Vec<String> = count
                    .roots
                    .iter()
                    .take(SUMMARY_ROOTS)
                    .map(|&root| {
                        format!(
                            "{} blocks {}",
                            place(&report.cells[root]),
                            self.cells[root].blocks
                        )
                    })
                    .collect();
                let more = count.roots.len().saturating_sub(SUMMARY_ROOTS);
                lines.push(format!(
                    "  {} {key}: {}, blocking {}: {}{}{}",
                    class.name(),
                    counted(count.roots.len() as u64, "root", "roots"),
                    counted(count.blocked, "cell", "cells"),
                    roots.join(", "),
                    if more > 0 {
                        format!(" and {more} more")
                    } else {
                        String::new()
                    },
                    count
                        .owner
                        .as_ref()
                        .map(|owner| format!("; owner {owner}"))
                        .unwrap_or_default()
                ));
            }
            if not_modeled.len() > SUMMARY_KEYS {
                lines.push(format!(
                    "  and {} more keys in triage.json",
                    not_modeled.len() - SUMMARY_KEYS
                ));
            }
        }
        lines
    }

    /// The members of `triage.json` before its cells, in order.
    pub(crate) fn header(&self, report: &VerdictReport, run: Value) -> Vec<(&'static str, Value)> {
        let class_counts = |counts: BTreeMap<Class, (u64, u64, u64)>| -> Value {
            let mut map = Map::new();
            for (class, (cells, roots, reading)) in counts {
                map.insert(
                    class.name().to_owned(),
                    json!({ "cells": cells, "roots": roots, "reading_a_root": reading }),
                );
            }
            Value::Object(map)
        };
        let mismatched = |i: usize| matches!(report.cells[i].verdict, Verdict::Mismatched { .. });
        let keys: Vec<Value> = self
            .keys()
            .into_iter()
            .map(|((class, key), count)| {
                let roots: Vec<Value> = count
                    .roots
                    .iter()
                    .take(KEY_ROOT_CELLS)
                    .map(|&root| {
                        json!({ "cell": place(&report.cells[root]), "blocks": self.cells[root].blocks })
                    })
                    .collect();
                json!({
                    "class": class.name(),
                    "key": key,
                    "owner": count.owner,
                    "roots": count.roots.len(),
                    "reading_a_root": count.blocked,
                    "workbook": count.workbook,
                    "root_cells": roots,
                    "root_cells_omitted": count.roots.len().saturating_sub(KEY_ROOT_CELLS),
                })
            })
            .collect();
        vec![
            ("format", json!("dnacalc-compare-triage")),
            ("version", json!(1)),
            ("case", json!(report.case)),
            ("source", json!(report.source)),
            ("run", run),
            (
                "attribution",
                json!({
                    "kernel_class": "inferred: a calculated cell that differs from the capture, reads no other differing cell and reported a call compile did not insert is classed kernel-value or kernel-bits; the difference may lie in the formula around the call or in a constant argument, which the run does not check",
                    "kernel_function": "inferred: a kernel-value or kernel-bits cell is keyed by the last call the evaluator reported for its evaluation that compile did not insert; the evaluator reports the calls, not which call's result the formula returned",
                    "mismatched_reads_root": "inferred: a mismatched cell whose kept reads cover another mismatched cell is counted under that cell's root and keeps its own difference; its own value may also differ for a reason of its own",
                    "refused_reads_root": "reported: a not-modelled cell whose evaluation stopped at a formula cell of the inventory (CalcFml's MissingInput names the cell) is counted under that cell's root",
                }),
            ),
            ("inventory", json!(self.cells.len())),
            ("classes", class_counts(self.by_class(|_| true))),
            (
                "mismatched_by_class",
                class_counts(self.by_class(mismatched)),
            ),
            ("keys", Value::Array(keys)),
        ]
    }

    /// One inventory cell as `triage.json` lists it.
    pub(crate) fn cell_json(&self, report: &VerdictReport, i: usize) -> Value {
        let cell = &self.cells[i];
        let verdict = &report.cells[i];
        let address = &verdict.formula.cell;
        let mut map = Map::new();
        map.insert("sheet".to_owned(), json!(address.sheet()));
        map.insert("row".to_owned(), json!(address.row()));
        map.insert("column".to_owned(), json!(address.column()));
        map.insert("cell".to_owned(), json!(address.a1()));
        map.insert("verdict".to_owned(), json!(verdict.verdict.name()));
        map.insert("live_excel_observation".into(), json!(matches!(verdict.observation, Some(calc_verdict::Observation::ExcelRaw(_)))));
        if verdict.external_links {
            // Calculated from values another workbook's link caches hold (dc-ylr): the corpus
            // summary counts a match among them apart from plain passes.
            map.insert("external_links".to_owned(), json!(true));
        }
        if verdict.circular_rule {
            // The circular-reference rule's value with iteration off (dc-y2o.1): the corpus
            // summary counts a match among them apart from plain passes.
            map.insert("circular_rule".to_owned(), json!(true));
        }
        map.insert("class".to_owned(), json!(cell.class.name()));
        map.insert("key".to_owned(), json!(cell.key));
        if let Some(owner) = &cell.owner {
            map.insert("owner".to_owned(), json!(owner));
        }
        if let Some(role) = cell.role {
            map.insert("role".to_owned(), json!(role.name()));
        }
        if let Some(root) = cell.root {
            map.insert("root".to_owned(), json!(place(&report.cells[root])));
        }
        if cell.blocks > 0 {
            map.insert("blocks".to_owned(), json!(cell.blocks));
        }
        if let Some(detail) = &cell.detail {
            map.insert("detail".to_owned(), json!(detail));
        }
        if let Some(difference) = &cell.difference {
            map.insert("difference".to_owned(), difference.json());
        }
        Value::Object(map)
    }

    /// `triage.json`: the header members, then one line per inventory cell.
    pub(crate) fn encode(&self, report: &VerdictReport, run: Value) -> String {
        let mut text = String::from("{\n");
        for (name, value) in self.header(report, run) {
            text.push_str(&format!("  {}: {},\n", json!(name), value));
        }
        text.push_str("  \"cells\": [\n");
        let cells: Vec<String> = (0..self.cells.len())
            .map(|i| format!("    {}", self.cell_json(report, i)))
            .collect();
        text.push_str(&cells.join(",\n"));
        if !cells.is_empty() {
            text.push('\n');
        }
        text.push_str("  ]\n}\n");
        text
    }
}

/// The sheet named for each sheet ID, for addresses of cells the engine names by key.
pub(crate) fn address_of(names: &HashMap<SheetId, String>, at: CellKey) -> Option<CellAddress> {
    CellAddress::new(names.get(&at.sheet())?, at.row(), at.column())
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_fml::syntax::TextSpan;
    use calc_host::value::{IdAllocator, WorksheetErrorCode};
    use calc_verdict::{
        Acquisition, CellObservation, CellResult, EvaluationRecord, FormulaCell, FormulaKind,
        InstrumentRecord, InstrumentedEvaluation, Observation, Provenance, ResultSet, RunIdentity,
        SheetEntry, SheetVisibility, VerdictPolicy, judge,
    };

    // The expectations in these tests are authored to exercise each class, not Excel's values:
    // a mismatch here is a number chosen to differ from the published one.

    const CASE: &str = "triage-unit";
    const SOURCE: &str = "sha256:unit";
    const CONTEXT: &str = "excel-calc-context/1;date1904=false;iteration=false;max_iterations=100;max_change=3f50624dd2f1a9fc;precision_as_displayed=false";

    fn at(row: u32, column: u32) -> CellAddress {
        CellAddress::new("S", row, column).unwrap()
    }

    fn number(n: f64) -> CalcValue {
        CalcValue::number(n)
    }

    /// A capture of the cells `observed` (row, column, expected value or none), all on sheet S.
    fn capture(observed: &[(u32, u32, Option<CalcValue>)]) -> CaptureRecord {
        CaptureRecord {
            case: CASE.to_owned(),
            source: SOURCE.to_owned(),
            acquisition: Acquisition::LiveCapture,
            environment: None,
            sheets: vec![SheetEntry {
                name: "S".to_owned(),
                visibility: SheetVisibility::Visible,
            }],
            formulas: observed
                .iter()
                .map(|(row, column, _)| FormulaCell {
                    cell: at(*row, *column),
                    kind: FormulaKind::Ordinary,
                    formula: None,
                    dynamic_array: false,
                })
                .collect(),
            observations: observed
                .iter()
                .filter_map(|(row, column, value)| {
                    Some(CellObservation {
                        cell: at(*row, *column),
                        observation: Observation::ExcelRaw(value.clone()?),
                    })
                })
                .collect(),
            session: None,
        }
    }

    /// The report of `results` (row, column, provenance, value) against `capture`, every
    /// calculated result proven by an evaluation that returned its value.
    fn judged(
        capture: &CaptureRecord,
        results: Vec<(u32, u32, Provenance, Option<CalcValue>)>,
    ) -> VerdictReport {
        let evaluations = results
            .iter()
            .filter_map(|(row, column, provenance, value)| match provenance {
                Provenance::Calculated {
                    tick,
                    revision,
                    context,
                    ..
                } => Some(InstrumentedEvaluation::new(
                    calc_verdict::Evaluation {
                        cell: at(*row, *column),
                        tick: *tick,
                        revision: *revision,
                        context: context.clone(),
                    },
                    Ok(value.clone().unwrap()),
                )),
                _ => None,
            })
            .collect();
        let record = InstrumentRecord {
            case: CASE.to_owned(),
            source: SOURCE.to_owned(),
            evaluations,
        };
        let results = ResultSet {
            case: CASE.to_owned(),
            source: SOURCE.to_owned(),
            run: RunIdentity {
                revision: 0,
                context: CONTEXT.to_owned(),
            },
            results: results
                .into_iter()
                .map(|(row, column, provenance, value)| CellResult {
                    cell: at(row, column),
                    provenance,
                    value,
                })
                .collect(),
        };
        judge(capture, &results, &record, &VerdictPolicy::default()).unwrap()
    }

    fn calculated() -> Provenance {
        Provenance::Calculated {
            tick: 1,
            revision: 0,
            context: CONTEXT.to_owned(),
            external_links: false,
        }
    }

    fn not_modeled() -> Provenance {
        Provenance::NotModeled {
            feature: "f".to_owned(),
            diagnostic: "d".to_owned(),
        }
    }

    fn call(spelling: &str, notation: &str, synthetic: bool) -> Call {
        Call {
            id: FunctionId::from_raw(7),
            spelling: spelling.to_owned(),
            notation: notation.to_owned(),
            synthetic,
        }
    }

    fn read(top: u32, left: u32, bottom: u32, right: u32) -> Read {
        Read {
            sheet: "S".to_owned(),
            top,
            left,
            bottom,
            right,
            cells: u64::from((bottom - top + 1) * (right - left + 1)),
            runs: Vec::new(),
            omitted: 0,
        }
    }

    fn summary(triage: &Triage) -> Vec<String> {
        triage
            .cells
            .iter()
            .map(|cell| {
                format!(
                    "{} {} {}{}",
                    cell.class.name(),
                    cell.key,
                    cell.role.map_or("-", Role::name),
                    cell.root
                        .map(|root| format!(" root={root}"))
                        .unwrap_or_default()
                )
            })
            .collect()
    }

    /// A difference that agrees to 15 significant digits is kernel-bits, one that does not is
    /// kernel-value, one with no call is formula; a cell reading a differing cell takes its root,
    /// through a chain, and the root counts the cells it blocks.
    #[test]
    fn differences_split_by_digits_and_reads() {
        // S!A1 differs in the last bit (1 ulp); S!A2 reads A1 and differs; S!A3 reads A2 and
        // differs; S!B1 differs at the third digit; S!B2 differs with no call; S!B3 matches.
        let one_ulp = f64::from_bits(1.1f64.to_bits() + 1);
        let capture = capture(&[
            (1, 1, Some(number(1.1))),
            (2, 1, Some(number(2.2))),
            (3, 1, Some(number(4.4))),
            (1, 2, Some(number(1.25))),
            (2, 2, Some(number(3.0))),
            (3, 2, Some(number(5.0))),
        ]);
        let report = judged(
            &capture,
            vec![
                (1, 1, calculated(), Some(number(one_ulp))),
                (2, 1, calculated(), Some(number(2.2000000000000006))),
                (3, 1, calculated(), Some(number(4.400000000000001))),
                (1, 2, calculated(), Some(number(1.5))),
                (2, 2, calculated(), Some(number(3.5))),
                (3, 2, calculated(), Some(number(5.0))),
            ],
        );
        let mut retained = HashMap::new();
        retained.insert(
            at(1, 1),
            Retained {
                entry_failure: None,
                precedents: Vec::new(),
                calls: vec![call("SUM", "Call", false)],
                reads: vec![read(1, 3, 2, 3)],
            },
        );
        retained.insert(
            at(2, 1),
            Retained {
                entry_failure: None,
                precedents: Vec::new(),
                calls: vec![call("+", "Infix", false), call("-", "Prefix", true)],
                reads: vec![read(1, 1, 1, 1)],
            },
        );
        retained.insert(
            at(3, 1),
            Retained {
                entry_failure: None,
                precedents: Vec::new(),
                calls: vec![call("*", "Infix", false)],
                reads: vec![read(1, 1, 2, 1)],
            },
        );
        retained.insert(
            at(1, 2),
            Retained {
                entry_failure: None,
                precedents: Vec::new(),
                calls: vec![call("ROUND", "Call", false)],
                reads: vec![read(3, 2, 3, 2)],
            },
        );
        retained.insert(
            at(2, 2),
            Retained {
                entry_failure: None,
                precedents: Vec::new(),
                calls: Vec::new(),
                reads: Vec::new(),
            },
        );
        let dna = vec![Dna::Calculated; 6];
        let triage = triage(&capture, &report, &dna, &retained, None, None);
        assert_eq!(
            summary(&triage),
            [
                "kernel-bits SUM root",
                "kernel-bits SUM reads_root root=0",
                "kernel-bits SUM reads_root root=0",
                "kernel-value ROUND root",
                "formula value differs with no function call root",
                "matched matched -",
            ]
        );
        assert_eq!(triage.cells[0].blocks, 2);
        let difference = triage.cells[0].difference.as_ref().unwrap();
        assert_eq!(difference.ulp, Some(1));
        assert_eq!(difference.agrees_to_15_digits(), Some(true));
        assert_eq!(
            difference
                .function
                .as_ref()
                .map(|call| call.spelling.as_str()),
            Some("SUM")
        );
        // A2 and A3 are counted under A1's root and keep their own differences: A2's value is not
        // A1's, and its own call (+) is what it returned.
        let reader = triage.cells[1].difference.as_ref().unwrap();
        assert_eq!(
            (reader.expected.clone(), reader.actual.clone()),
            (number(2.2), number(2.2000000000000006))
        );
        assert_eq!(
            reader.function.as_ref().map(|call| call.spelling.as_str()),
            Some("+")
        );
        assert!(
            triage.cells[1]
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("its own value also differs")),
            "{:?}",
            triage.cells[1].detail
        );
        assert!(triage.cells[2].difference.is_some());
        let rounded = triage.cells[3].difference.as_ref().unwrap();
        assert_eq!(
            rounded.digits15,
            Some((
                "1.25000000000000e0".to_owned(),
                "1.50000000000000e0".to_owned()
            ))
        );
        let lines = triage.summary(&report);
        assert_eq!(
            lines[1],
            "mismatched by class: kernel-value 1, kernel-bits 3 (1 root, 2 reading a root), formula 1"
        );
        assert_eq!(lines[2], "not modelled by root key: none");
    }

    /// The last call that compile did not insert is the function a difference is keyed by.
    #[test]
    fn a_synthetic_call_is_not_the_function() {
        let capture = capture(&[(1, 1, Some(number(1.0)))]);
        let report = judged(&capture, vec![(1, 1, calculated(), Some(number(2.0)))]);
        let mut retained = HashMap::new();
        retained.insert(
            at(1, 1),
            Retained {
                entry_failure: None,
                precedents: Vec::new(),
                calls: vec![
                    call("SUM", "Call", false),
                    call("+", "Infix", false),
                    call("-", "Prefix", true),
                    call("-", "Prefix", true),
                ],
                reads: Vec::new(),
            },
        );
        let triage = triage(&capture, &report, &[Dna::Calculated], &retained, None, None);
        assert_eq!(summary(&triage), ["kernel-value Infix + root"]);
        // Without retained evidence the cell is still classed, as an engine root.
        let triage = super::triage(
            &capture,
            &report,
            &[Dna::Calculated],
            &HashMap::new(),
            None,
            None,
        );
        assert_eq!(
            summary(&triage),
            ["engine mismatch without evaluator evidence root"]
        );
    }

    /// A refused cell whose evaluation stopped at another inventory cell reads that cell's root;
    /// one stopped at a cell outside the inventory is a capture root; a workbook failure gives
    /// every cell its class.
    #[test]
    fn refusals_take_their_roots_class() {
        let capture = capture(&[
            (1, 1, Some(CalcValue::error(WorksheetErrorCode::Name))),
            (2, 1, Some(CalcValue::error(WorksheetErrorCode::Name))),
            (3, 1, Some(CalcValue::error(WorksheetErrorCode::Name))),
            (4, 1, Some(number(1.0))),
            (5, 1, None),
        ]);
        let report = judged(
            &capture,
            vec![
                (1, 1, not_modeled(), None),
                (2, 1, not_modeled(), None),
                (3, 1, not_modeled(), None),
                (4, 1, not_modeled(), None),
                (5, 1, calculated(), Some(number(1.0))),
            ],
        );
        let unknown = Refusal::new(Class::FunctionMissing, "NOSUCH", "Compile:UnknownFunction")
            .owned_by("owner-x");
        let reading = |cell: Option<CellAddress>| Refusal {
            reads: cell,
            ..Refusal::new(
                Class::Capture,
                "reads a formula cell outside the capture's inventory",
                "Evaluation:MissingInput",
            )
        };
        let dna = vec![
            Dna::Refused(unknown),
            Dna::Refused(reading(Some(at(1, 1)))),
            Dna::Refused(reading(Some(at(2, 1)))),
            Dna::Refused(reading(Some(at(9, 9)))),
            Dna::Calculated,
        ];
        let triage = triage(&capture, &report, &dna, &HashMap::new(), None, None);
        assert_eq!(
            summary(&triage),
            [
                "function-missing NOSUCH root",
                "function-missing NOSUCH reads_root root=0",
                "function-missing NOSUCH reads_root root=0",
                "capture reads a formula cell outside the capture's inventory root",
                "capture not observed root",
            ]
        );
        let lines = triage.summary(&report);
        assert_eq!(
            lines[0],
            "classes: function-missing 3 (1 root, 2 reading a root), capture 2"
        );
        assert_eq!(
            lines[3],
            "  function-missing NOSUCH: 1 root, blocking 2 cells: S!A1 blocks 2; owner owner-x"
        );
        // The cells reading the root take its owner with its class and key.
        let owners: Vec<Option<&str>> = triage
            .cells
            .iter()
            .map(|cell| cell.owner.as_deref())
            .collect();
        assert_eq!(
            owners,
            [
                Some("owner-x"),
                Some("owner-x"),
                Some("owner-x"),
                Some(""),
                None
            ]
        );
        assert_eq!(
            triage.cell_json(&report, 1)["owner"],
            serde_json::json!("owner-x")
        );

        let failure = WorkbookFailure {
            class: Class::Doc,
            key: "Workbook:Cell:UnsupportedType".to_owned(),
            message: "refused".to_owned(),
        };
        let triage = super::triage(
            &capture,
            &report,
            &dna,
            &HashMap::new(),
            Some(&failure),
            None,
        );
        assert!(
            triage.cells.iter().all(|cell| cell.class == Class::Doc
                && cell.role == Some(Role::Workbook)
                && cell.key == failure.key),
            "{triage:?}"
        );
    }

    /// Engine refusals and missing results are classed from the run's account of the cell.
    #[test]
    fn missing_results_are_classed_by_what_the_workbook_holds() {
        let capture = capture(&[
            (1, 1, Some(number(1.0))),
            (2, 1, Some(number(1.0))),
            (3, 1, Some(number(1.0))),
        ]);
        let missing = |reason: &str| Provenance::Missing {
            reason: reason.to_owned(),
        };
        let report = judged(
            &capture,
            vec![
                (1, 1, missing("no sheet"), None),
                (2, 1, missing("empty"), None),
                (3, 1, missing("literal"), None),
            ],
        );
        let triage = triage(
            &capture,
            &report,
            &[Dna::SheetNotLoaded, Dna::Empty, Dna::Literal],
            &HashMap::new(),
            None,
            None,
        );
        assert_eq!(
            summary(&triage),
            [
                "doc sheet not loaded root",
                "doc no input at a formula cell of the inventory root",
                "doc a literal at a formula cell of the inventory root",
            ]
        );
        // An unproven claim: a calculated result with no evaluation behind it.
        let results = ResultSet {
            case: CASE.to_owned(),
            source: SOURCE.to_owned(),
            run: RunIdentity {
                revision: 0,
                context: CONTEXT.to_owned(),
            },
            results: vec![CellResult {
                cell: at(1, 1),
                provenance: calculated(),
                value: Some(number(1.0)),
            }],
        };
        let empty = EvaluationRecord {
            case: CASE.to_owned(),
            source: SOURCE.to_owned(),
            evaluations: Vec::new(),
        };
        let report = judge(&capture, &results, &empty, &VerdictPolicy::default()).unwrap();
        let triage = super::triage(
            &capture,
            &report,
            &[Dna::Calculated, Dna::OpenRefused, Dna::OpenRefused],
            &HashMap::new(),
            None,
            None,
        );
        assert_eq!(
            summary(&triage)[0],
            "engine unproven: no evaluation in the evaluation record root"
        );
    }

    /// For a version 1 capture, which names no context, a key of the settings the package
    /// declares and the run does not apply classes every calculated cell context, matched or not;
    /// a cell not calculated keeps its own class.
    #[test]
    fn a_declared_context_the_run_does_not_apply_is_context() {
        let capture = capture(&[
            (1, 1, Some(number(1.0))),
            (2, 1, Some(number(2.0))),
            (3, 1, Some(number(3.0))),
        ]);
        let report = judged(
            &capture,
            vec![
                (1, 1, calculated(), Some(number(1.0))),
                (2, 1, calculated(), Some(number(2.5))),
                (3, 1, not_modeled(), None),
            ],
        );
        let unknown = Refusal::new(Class::FunctionMissing, "NOSUCH", "Compile:UnknownFunction");
        let dna = [Dna::Calculated, Dna::Calculated, Dna::Refused(unknown)];
        let key = "date1904: package declares true, run applies false";
        let triage = triage(&capture, &report, &dna, &HashMap::new(), None, Some(key));
        assert_eq!(
            summary(&triage),
            [
                format!("context {key} workbook"),
                format!("context {key} workbook"),
                "function-missing NOSUCH root".to_owned(),
            ]
        );
    }

    /// The context key names the settings that differ.
    #[test]
    fn the_context_key_names_the_settings_that_differ() {
        let run = CONTEXT;
        let captured = CONTEXT.replace("date1904=false", "date1904=true");
        assert_eq!(
            context_key(&captured, run),
            "date1904: capture true, run false"
        );
        assert_eq!(
            context_key(run, run),
            format!("capture context {run}, run context {run}")
        );
    }

    /// The typed refusals map to their classes and keys.
    #[test]
    fn typed_refusals_have_their_classes() {
        let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
        let sheet = sheets.issue().unwrap();
        let mut names = HashMap::new();
        names.insert(sheet, "S".to_owned());
        let address = |key: CellKey| address_of(&names, key);
        let span = TextSpan::new(0, 3);
        let id_of = |name: &str| {
            catalogue()
                .targets()
                .find(|target| target.spelling() == name)
                .unwrap()
                .id()
        };
        let (sum, not) = (id_of("SUM"), id_of("NOT"));
        let cases: Vec<(NotModeled, Class, &str, &str)> = vec![
            (
                NotModeled::FormulaText,
                Class::Formula,
                "FormulaText",
                CALC_ENGINE,
            ),
            (
                NotModeled::Compile(CompileError::NotModelled {
                    span,
                    gap: Gap::UnknownFunction {
                        spelling: "NoSuch".to_owned(),
                    },
                }),
                Class::FunctionMissing,
                "NOSUCH",
                crate::owners::UNASSIGNED,
            ),
            (
                NotModeled::Compile(CompileError::NotModelled {
                    span,
                    gap: Gap::UnknownFunction {
                        spelling: "_xlfn.XLookup".to_owned(),
                    },
                }),
                Class::FunctionMissing,
                "_XLFN.XLOOKUP",
                "exactness dc-uar",
            ),
            (
                NotModeled::Compile(CompileError::NotModelled {
                    span,
                    gap: Gap::Construct {
                        construct: "a defined name",
                        owner: "dc-10y",
                    },
                }),
                Class::Formula,
                "Compile:Unsupported:a defined name (dc-10y)",
                "dc-10y",
            ),
            (
                NotModeled::Compile(CompileError::NotModelled {
                    span,
                    gap: Gap::NumberLiteral,
                }),
                Class::Formula,
                "Compile:NumberLiteral",
                NUMBER_LITERAL_OWNER,
            ),
            (
                NotModeled::Evaluation(EvalFault::Function(FunctionFault::NotImplemented {
                    id: sum,
                    argument: None,
                })),
                Class::NotModelled,
                "SUM",
                "exactness dc-j9k",
            ),
            (
                NotModeled::Evaluation(EvalFault::Function(FunctionFault::ArityMismatch {
                    id: sum,
                    supplied: 300,
                })),
                Class::Formula,
                "Evaluation:Function:ArityMismatch",
                CALC_FML,
            ),
            (
                NotModeled::Evaluation(EvalFault::NearZeroFinalSum { result: 1e-17 }),
                Class::Formula,
                "Evaluation:NearZeroFinalSum",
                CALC_FML,
            ),
            (
                NotModeled::Result(CalcValue::empty()),
                Class::Engine,
                "Result:empty",
                CALC_ENGINE,
            ),
        ];
        for (refusal, class, key, owner) in cases {
            let triaged = refusal_of(&refusal, &address);
            assert_eq!(
                (triaged.class, triaged.key.as_str(), triaged.owner.as_str()),
                (class, key, owner),
                "{refusal:?}"
            );
            assert_eq!(triaged.reads, None);
        }
        // NOT("1"): the catalogue holds NOT and its kernel refuses text it does not model. That
        // is not modelled under NOT's owner, never a missing function.
        let refused_not = refusal_of(
            &NotModeled::Evaluation(EvalFault::Function(FunctionFault::NotImplemented {
                id: not,
                argument: Some(0),
            })),
            &address,
        );
        assert_eq!(
            (
                refused_not.class,
                refused_not.key.as_str(),
                refused_not.owner.as_str(),
                refused_not.feature.as_str()
            ),
            (
                Class::NotModelled,
                "NOT",
                "exactness dc-uar",
                "Evaluation:Function:NotImplemented (NOT, argument 1)"
            )
        );
        assert_eq!(Class::NotModelled.name(), "not-modelled");
        let missing = refusal_of(
            &NotModeled::Evaluation(EvalFault::MissingInput {
                cell: CellKey::new(sheet, 4, 2).unwrap(),
            }),
            &address,
        );
        assert_eq!(missing.reads, Some(at(4, 2)));
        // The feature report.json names is the typed refusal with what it concerns, never the
        // key a cell outside the inventory would take.
        assert_eq!(missing.feature, "Evaluation:MissingInput (reads S!B4)");
        let exact = refusal_of(
            &NotModeled::Evaluation(EvalFault::Function(FunctionFault::ExactUnavailable {
                id: sum,
            })),
            &address,
        );
        assert_eq!(
            (exact.class, exact.key.as_str(), exact.feature.as_str()),
            (
                Class::FunctionMissing,
                "ExactUnavailable:SUM",
                "Evaluation:Function:ExactUnavailable (SUM)"
            )
        );
        let unknown = refusal_of(
            &NotModeled::Compile(CompileError::NotModelled {
                span,
                gap: Gap::UnknownFunction {
                    spelling: "NoSuch".to_owned(),
                },
            }),
            &address,
        );
        assert_eq!(unknown.feature, "Compile:UnknownFunction (NOSUCH)");
    }
}
