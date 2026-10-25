//! The worker's result file turned into a CaptureRecord version 2.
//!
//! The result (`dnacalc-capture-worker-result`, version 2) is what `worker/capture-worker.ps1`
//! writes:
//!
//! ```text
//! {"format": "dnacalc-capture-worker-result", "version": 2, "excel_pid": <pid> | null,
//!  "fatal": null | "<message>",
//!  "environment": null | {"excel_build": "...", "channel": "...", "compatibility": "...",
//!                         "numeric_environment": "...", "settings": [["<name>", "<value>"], ...]},
//!  "sheets": [{"name": "...", "visibility": "visible" | "hidden" | "very_hidden"}, ...],
//!  "formulas": [{"sheet", "row", "column",
//!                "type": "normal" | "shared" | "array" | "array_member" | "dataTable"
//!                        | "data_table_member" | ...,
//!                "si": n | null, "ref": "C1:C3" | null, "cm": "<cell metadata index>" | null,
//!                "dynamic_array_metadata": b,
//!                ["anchor": "L1" (array_member and data_table_member only)]}, ...],
//!  "cells": [{"sheet", "row", "column", "formula": "<UTF-16 hex>" | null, "value": <raw>,
//!             "spill": <spill>}, ...],
//!  "saved": [{"sheet", "row", "column", "t": "n" | "b" | "e" | "str" | ..., "v": "<UTF-16 hex>"}],
//!  "saved_formulas": [<as formulas, read from the package Excel saved>, ...],
//!  "spill_members": [{"sheet", "row", "column"}, ...],
//!  "reopened": [{"sheet", "row", "column", "value": <raw>, "spill": <spill>}, ...],
//!  "failures": [{"stage", ["sheet", "row", "column",] "message"}, ...]}
//! ```
//!
//! where `dynamic_array_metadata` says whether the formula cell's `cm` names a block of the
//! package's cell metadata that marks it as a dynamic-array formula (the metadata type XLDAPR with
//! `fDynamic`), an `array_member` entry is a cell of a legacy array formula's range other than its
//! anchor (the worker lists each, with its anchor's relative A1 reference, since the package holds
//! the formula on the anchor only), `<raw>` is `{"empty": true}`,
//! `{"number": "<16 hex digits: the binary64 bits>"}`, `{"text": "<UTF-16 hex>"}`,
//! `{"logical": b}`, `{"error_code": <Excel's CVErr code>}`,
//! `{"unsupported": "<.NET type of the Value2>"}` or `{"unread": "<why Value2 cannot be read>"}`,
//! `<spill>` is what Excel reports of the cell's spill: `null` (Range.HasSpill is False),
//! `{"ref": "B1:C3", "rows": r, "columns": c}` (Range.SpillingToRange's relative address, rows and
//! columns) or `{"unread": "<why the spill cannot be read>"}`, and `spill_members` are the cells
//! Excel lists among its formula cells (SpecialCells) that the package holds no formula for and
//! that lie in the spill range of an inventory cell. Version 1 had no spill, no
//! `dynamic_array_metadata`, no `saved_formulas` and no `spill_members`.
//!
//! The inventory is the worker's `formulas`, read from the package. A cell value, cache or formula
//! text that cannot be represented becomes a capture failure for that cell, so the record shows
//! what was not observed.
//!
//! Whether a formula spills is what Excel reports after the full rebuild, never what the input
//! package's `ref` claims. A spilling cell's Value2 is only the top-left value of its array, and
//! CaptureRecord version 2 has no array observation, so a cell that spills keeps its place in the
//! inventory (the denominator) with no raw observation, saved cache or reopened value, and a
//! failure of that cell at [`STAGE_SPILL`] names its spill range with its rows and columns; the
//! verdict has nothing to compare it with, so a calculated result for it is `not_run`, never
//! matched. The shape stays message text here: carrying the array in CaptureRecord and comparing
//! it in calc-verdict is bead dc-dw44's, and until then a spilling cell is not comparable. Its
//! shape must start at the cell and agree with the rows and columns Excel reports, and the package
//! Excel saved must hold the formula as a dynamic-array formula (`array` whose cell metadata `cm`
//! names the dynamic-array metadata XLDAPR, `dynamic_array_metadata`) whose `ref` is that range: a
//! saved package that disagrees, or that declares a dynamic-array spill range for a cell Excel
//! reports no spill for, is a `save` failure of that cell. The spill read again after reopening
//! the saved package must be the one captured, or the cell gets a `reopen` failure, and a reopened
//! value is recorded only for a cell that spills in neither. A spill that cannot be read after the
//! rebuild, or whose shape is inconsistent, refuses the whole conversion, as does a formula cell
//! the worker read nothing for: a spilling cell's value would otherwise pass for a scalar. An Excel
//! without Range.HasSpill (one older than dynamic arrays) therefore refuses every capture that
//! holds a formula: this adapter captures only an Excel with dynamic arrays.
//!
//! The cells a formula spills into are a separate population, never formula cells of the
//! inventory: each of `spill_members` must lie in the spill range recorded for an inventory cell
//! of its sheet (not that cell itself), and one that does not is an inventory failure, since Excel
//! then lists a formula cell the package does not hold.
//!
//! A formula the input package holds as a dynamic-array formula (type `array` whose cell metadata
//! `cm` names the dynamic-array metadata XLDAPR, `dynamic_array_metadata`, its `ref` starting at
//! the cell), which Excel with dynamic arrays saves for a formula that
//! could return an array, is an ordinary formula; when it does not spill after the rebuild it is
//! recorded as Excel shows it and flagged as dynamic ([`FormulaCell::dynamic_array`], a formula
//! over its one cell with no spill range), whatever range its input `ref` claimed.
//!
//! A legacy array formula (entered with Ctrl+Shift+Enter: type `array` whose cell metadata is
//! absent or does not name XLDAPR, its `ref` starting at the cell) is recorded over its whole
//! range, as Excel lists every cell of it among its formula cells, each showing its own element:
//! the anchor as [`FormulaKind::LegacyArrayAnchor`] with its range, and each `array_member` the
//! worker lists as [`FormulaKind::LegacyArrayMember`] naming its anchor, each with Excel's value
//! read like any other formula cell's. The package Excel saved must hold the anchor as a legacy
//! array formula over the same range and each member as a member of it, or the cell gets a `save`
//! failure. calc-verdict refuses a record that does not list every cell of each range once.
//!
//! A what-if data table (`<f t="dataTable" ref ...>` on the top-left cell of its result area,
//! dc-10y) is recorded over its whole area, as Excel lists every cell of it among its formula cells
//! (each holding `{=TABLE(...)}`): the top-left cell as [`FormulaKind::DataTableAnchor`] with its
//! area, and each `data_table_member` the worker lists as [`FormulaKind::DataTableMember`] naming
//! it, each with Excel's value read like any other formula cell's. The package Excel saved must
//! hold the table over the same area, or the cell gets a `save` failure.
//!
//! A formula of a type the record has no kind for (an `array` or `dataTable` formula whose `ref`
//! does not start at its cell) refuses the whole conversion: a record without that cell would have
//! a denominator one short, and one that called it ordinary would misreport it.

use calc_verdict::value::{
    CalcValue, ExcelText, MAX_COLUMNS, MAX_ROWS, NumericTargetId, WorksheetErrorCode,
};
use calc_verdict::{
    Acquisition, CacheCellType, CalculationContext, CaptureFailure, CaptureRecord, CaptureSession,
    CellAddress, CellObservation, ExcelEnvironment, FormulaCell, FormulaKind, Observation,
    STAGE_INVENTORY, SavedCache, SharedRange, SheetEntry, SheetVisibility,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const RESULT_FORMAT: &str = "dnacalc-capture-worker-result";
pub(crate) const RESULT_VERSION: u64 = 2;

/// The stage of the failure a formula cell that spills after the full rebuild gets: its message
/// names the spill range with its rows and columns, and the cell has no raw observation, saved
/// cache or reopened value, since CaptureRecord version 2 holds no array (bead dc-dw44 carries
/// arrays in CaptureRecord and compares them in calc-verdict; until then the cell is not
/// comparable, and its verdict is `not_run`).
pub const STAGE_SPILL: &str = "spill";

/// Why a capture whose spill state of a formula cell after the full rebuild cannot be read is
/// refused.
const UNREAD_SPILL: &str = "a spilling cell's Value2 is only the top-left value of its array, so a capture whose spill cannot be read after the full rebuild is refused rather than written with that value passing for a scalar";

/// What the adapter knows about a capture beside the worker's result. Crate-private, as
/// [`convert`] is: the record it gives is stamped a live capture.
pub(crate) struct Frame {
    pub(crate) case: String,
    pub(crate) source: String,
    pub(crate) tool: String,
    pub(crate) revision: u64,
    pub(crate) operations: Vec<String>,
    pub(crate) output_package: Option<String>,
    /// Failures the adapter observed itself (an Excel that had to be terminated, say).
    pub(crate) failures: Vec<CaptureFailure>,
}

/// The Excel error value of a CVErr code (the `XlCVError` values of Excel's object model), or
/// `None` for a code this value model has no error for.
pub(crate) fn error_from_code(code: u64) -> Option<WorksheetErrorCode> {
    Some(match code {
        2000 => WorksheetErrorCode::Null,
        2007 => WorksheetErrorCode::Div0,
        2015 => WorksheetErrorCode::Value,
        2023 => WorksheetErrorCode::Ref,
        2029 => WorksheetErrorCode::Name,
        2036 => WorksheetErrorCode::Num,
        2042 => WorksheetErrorCode::NA,
        2043 => WorksheetErrorCode::GettingData,
        2045 => WorksheetErrorCode::Spill,
        2046 => WorksheetErrorCode::Connect,
        2047 => WorksheetErrorCode::Blocked,
        2049 => WorksheetErrorCode::Field,
        2050 => WorksheetErrorCode::Calc,
        _ => return None,
    })
}

fn units(hex: &str) -> Option<Vec<u16>> {
    if !hex.len().is_multiple_of(4) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    hex.as_bytes()
        .chunks(4)
        .map(|chunk| {
            std::str::from_utf8(chunk)
                .ok()
                .and_then(|digits| u16::from_str_radix(digits, 16).ok())
        })
        .collect()
}

/// A raw Value2 as a typed value, or why it is not one this record can hold.
pub(crate) fn raw_value(raw: &Value) -> Result<CalcValue, String> {
    let members = raw
        .as_object()
        .filter(|members| members.len() == 1)
        .ok_or_else(|| format!("{raw} is not a raw value of one member"))?;
    let (kind, inner) = members.iter().next().expect("one member");
    match (kind.as_str(), inner) {
        ("empty", Value::Bool(true)) => Ok(CalcValue::empty()),
        ("number", Value::String(bits)) if bits.len() == 16 => u64::from_str_radix(bits, 16)
            .map(|bits| CalcValue::number(f64::from_bits(bits)))
            .map_err(|_| format!("{bits:?} is not 16 hex digits")),
        ("text", Value::String(hex)) => units(hex)
            .map(|units| CalcValue::text(ExcelText::from_utf16_code_units(units)))
            .ok_or_else(|| format!("{hex:?} is not UTF-16 code units in hex")),
        ("logical", Value::Bool(logical)) => Ok(CalcValue::logical(*logical)),
        ("error_code", Value::Number(code)) => code
            .as_u64()
            .and_then(error_from_code)
            .map(CalcValue::error)
            .ok_or_else(|| format!("Excel error code {code} is not one this value model holds")),
        ("unsupported", Value::String(kind)) => Err(format!(
            "Value2 is a {kind}, which this capture does not read"
        )),
        ("unread", Value::String(why)) => Err(format!("Range.Value2 cannot be read: {why}")),
        _ => Err(format!("{raw} is not a raw value")),
    }
}

fn text<'a>(value: &'a Value, name: &str, at: &str) -> Result<&'a str, String> {
    value[name]
        .as_str()
        .ok_or_else(|| format!("{at} has no string {name}"))
}

fn unsigned(value: &Value, name: &str, at: &str) -> Result<u64, String> {
    value[name]
        .as_u64()
        .ok_or_else(|| format!("{at} has no unsigned integer {name}"))
}

fn array<'a>(result: &'a Value, name: &str) -> Result<&'a Vec<Value>, String> {
    result[name]
        .as_array()
        .ok_or_else(|| format!("the worker's result has no array {name}"))
}

fn address(entry: &Value, at: &str) -> Result<CellAddress, String> {
    let sheet = text(entry, "sheet", at)?;
    let row = unsigned(entry, "row", at)?;
    let column = unsigned(entry, "column", at)?;
    u32::try_from(row)
        .ok()
        .zip(u32::try_from(column).ok())
        .and_then(|(row, column)| CellAddress::new(sheet, row, column))
        .ok_or_else(|| format!("{at} names no cell of Excel's grid"))
}

fn failure(stage: &str, cell: &CellAddress, message: String) -> CaptureFailure {
    CaptureFailure {
        stage: stage.to_owned(),
        cell: Some(cell.clone()),
        message,
    }
}

fn place(cell: &CellAddress) -> String {
    format!("{}!{}", cell.sheet(), cell.a1())
}

/// The 1-based row and column of a relative A1 reference (`B7`), or `None` when the text is not
/// one within Excel's grid.
fn parse_a1(text: &str) -> Option<(u32, u32)> {
    let split = text.find(|c: char| !c.is_ascii_uppercase())?;
    let (letters, digits) = text.split_at(split);
    if !(1..=3).contains(&letters.len())
        || digits.is_empty()
        || digits.starts_with('0')
        || !digits.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let column = letters
        .bytes()
        .fold(0u32, |n, b| n * 26 + u32::from(b - b'A' + 1));
    let row: u32 = digits.parse().ok()?;
    ((1..=MAX_ROWS).contains(&row) && (1..=MAX_COLUMNS).contains(&column)).then_some((row, column))
}

/// The first and last cell of a ref (`B1`, `B1:B1` or `B1:C3`), first neither below nor right of
/// last, or `None`.
fn parse_range(reference: &str) -> Option<((u32, u32), (u32, u32))> {
    let (first, last) = match reference.split_once(':') {
        Some((first, last)) => (parse_a1(first)?, parse_a1(last)?),
        None => {
            let cell = parse_a1(reference)?;
            (cell, cell)
        }
    };
    (first.0 <= last.0 && first.1 <= last.1).then_some((first, last))
}

/// The refusal of a formula of type `kind` at `cell`, the worker's `formula` entry, for which the
/// record has no kind.
fn no_kind(formula: &Value, cell: &CellAddress, kind: &str) -> String {
    format!(
        "{} holds a formula of type {kind:?} (ref {}, cell metadata {}), for which CaptureRecord version 2 has no kind; this capture is refused rather than written with that cell left out or misnamed",
        place(cell),
        formula["ref"].as_str().unwrap_or("none"),
        formula["cm"].as_str().unwrap_or("none")
    )
}

/// Whether a formula's `ref` names `cell` and no other cell (`B1`, or `B1:B1`).
fn names_only(reference: &str, cell: &CellAddress) -> bool {
    let own = (cell.row(), cell.column());
    parse_range(reference) == Some((own, own))
}

/// "1 row", "3 rows".
fn counted(n: u32, what: &str) -> String {
    if n == 1 {
        format!("1 {what}")
    } else {
        format!("{n} {what}s")
    }
}

/// The spill range Excel reports for a formula cell: its ref (relative A1, starting at the cell)
/// and its rows and columns.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Spill {
    reference: String,
    first: (u32, u32),
    last: (u32, u32),
    rows: u32,
    columns: u32,
}

impl Spill {
    /// `Sheet1!B1:B3 (3 rows by 1 column)`.
    fn shape(&self, cell: &CellAddress) -> String {
        format!(
            "{}!{} ({} by {})",
            cell.sheet(),
            self.reference,
            counted(self.rows, "row"),
            counted(self.columns, "column")
        )
    }
}

/// What the worker read of `cell`'s spill in the entry `read` (a `cells` or `reopened` entry): no
/// spill, a spill range, or why there is no spill to record (its member is absent or not of the
/// result's form, Excel's spill state could not be read, or the range Excel reports does not start
/// at the cell or does not have the rows and columns Excel reports).
fn spill_of(read: &Value, cell: &CellAddress) -> Result<Option<Spill>, String> {
    let at = place(cell);
    let Some(spill) = read.as_object().and_then(|members| members.get("spill")) else {
        return Err(format!("the worker read no spill state for {at}"));
    };
    if spill.is_null() {
        return Ok(None);
    }
    if let Some(why) = spill
        .as_object()
        .filter(|members| members.len() == 1)
        .and_then(|members| members.get("unread"))
        .and_then(Value::as_str)
    {
        return Err(format!("Excel's spill state of {at} cannot be read: {why}"));
    }
    let shape = spill
        .as_object()
        .filter(|members| members.len() == 3)
        .and_then(|_| {
            Some((
                spill["ref"].as_str()?,
                u32::try_from(spill["rows"].as_u64()?).ok()?,
                u32::try_from(spill["columns"].as_u64()?).ok()?,
            ))
        });
    let Some((reference, rows, columns)) = shape else {
        return Err(format!(
            "the spill state the worker read for {at} ({spill}) is not a spill range of a ref, rows and columns"
        ));
    };
    match parse_range(reference) {
        Some((first, last))
            if first == (cell.row(), cell.column())
                && last.0 - first.0 + 1 == rows
                && last.1 - first.1 + 1 == columns =>
        {
            Ok(Some(Spill {
                reference: reference.to_owned(),
                first,
                last,
                rows,
                columns,
            }))
        }
        _ => Err(format!(
            "Excel reports {at} spilling over {reference:?} with {} and {}, which is not a range of that shape starting at the cell",
            counted(rows, "row"),
            counted(columns, "column")
        )),
    }
}

/// The failure of `cell`'s saved formula `saved` (the package Excel saved) against the spill Excel
/// reported after the rebuild, or `None` when they agree: a spill must be saved as a
/// dynamic-array formula (`array` whose cell metadata names the dynamic-array metadata XLDAPR)
/// whose `ref` is the spill range, and a cell with no spill must not be saved as a dynamic-array
/// formula with a `ref` beyond its own cell.
fn saved_disagreement(saved: &Value, cell: &CellAddress, spill: Option<&Spill>) -> Option<String> {
    let kind = saved["type"].as_str().unwrap_or("none");
    let reference = saved["ref"].as_str();
    let dynamic = saved["dynamic_array_metadata"] == true;
    let held = held(saved);
    let dynamic_array = kind == "array" && dynamic;
    match spill {
        Some(spill) => (!dynamic_array
            || reference.and_then(parse_range) != Some((spill.first, spill.last)))
        .then(|| {
            format!(
                "Excel reports a spill over {} after the full rebuild, and {held}",
                spill.shape(cell)
            )
        }),
        None => (dynamic_array && reference.is_some_and(|reference| !names_only(reference, cell)))
            .then(|| format!("Excel reports no spill after the full rebuild, and {held}")),
    }
}

/// How the saved package holds a formula, as a failure message says it: "the saved package holds
/// the formula as type "array" with ref B1:C3 and cell metadata 1 (the dynamic-array metadata
/// XLDAPR)", with the anchor a legacy array member names.
fn held(saved: &Value) -> String {
    let kind = saved["type"].as_str().unwrap_or("none");
    let dynamic = saved["dynamic_array_metadata"] == true;
    let metadata = match (saved["cm"].as_str(), dynamic) {
        (None, _) => "cell metadata none".to_owned(),
        (Some(cm), true) => format!("cell metadata {cm} (the dynamic-array metadata XLDAPR)"),
        (Some(cm), false) => {
            format!("cell metadata {cm}, which does not name the dynamic-array metadata XLDAPR")
        }
    };
    let anchor = saved["anchor"]
        .as_str()
        .map(|anchor| format!(" and anchor {anchor}"))
        .unwrap_or_default();
    format!(
        "the saved package holds the formula as type {kind:?} with ref {} and {metadata}{anchor}",
        saved["ref"].as_str().unwrap_or("none")
    )
}

/// The failure of a legacy array cell's or a data table cell's saved formula `saved` (the package
/// Excel saved) against the input's, or `None` when they agree: the anchor must be saved as a
/// legacy array formula (`array` whose cell metadata does not name XLDAPR) or a data table over the
/// same range, and a member as a member of the same anchor.
fn saved_legacy_disagreement(
    saved: &Value,
    cell: &CellAddress,
    kind: FormulaKind,
) -> Option<String> {
    let legacy = saved["type"] == "array"
        && !(saved["cm"].as_str().is_some() && saved["dynamic_array_metadata"] == true);
    match kind {
        FormulaKind::LegacyArrayAnchor { .. } => {
            let range = kind.legacy_range(cell);
            (!legacy || saved["ref"].as_str().and_then(parse_range) != range).then(|| {
                let (first, last) = range.expect("an anchor has a range");
                format!(
                    "the input package holds a legacy array formula over {}!{}:{}, and {}",
                    cell.sheet(),
                    a1(first),
                    a1(last),
                    held(saved)
                )
            })
        }
        FormulaKind::LegacyArrayMember {
            anchor_row,
            anchor_column,
        } => {
            let anchor = a1((anchor_row, anchor_column));
            (saved["type"] != "array_member" || saved["anchor"].as_str() != Some(anchor.as_str()))
                .then(|| {
                    format!(
                        "the input package holds this cell as a member of the legacy array formula anchored at {}!{anchor}, and {}",
                        cell.sheet(),
                        held(saved)
                    )
                })
        }
        FormulaKind::DataTableAnchor { .. } => {
            let range = kind.legacy_range(cell);
            (saved["type"] != "dataTable" || saved["ref"].as_str().and_then(parse_range) != range)
                .then(|| {
                    let (first, last) = range.expect("an anchor has a range");
                    format!(
                        "the input package holds a data table over {}!{}:{}, and {}",
                        cell.sheet(),
                        a1(first),
                        a1(last),
                        held(saved)
                    )
                })
        }
        FormulaKind::DataTableMember {
            anchor_row,
            anchor_column,
        } => {
            let anchor = a1((anchor_row, anchor_column));
            (saved["type"] != "data_table_member"
                || saved["anchor"].as_str() != Some(anchor.as_str()))
            .then(|| {
                format!(
                    "the input package holds this cell in the data table whose top-left cell is {}!{anchor}, and {}",
                    cell.sheet(),
                    held(saved)
                )
            })
        }
        FormulaKind::Ordinary
        | FormulaKind::SharedMaster { .. }
        | FormulaKind::SharedChild { .. } => None,
    }
}

/// The relative A1 reference of a row and column (`B7`).
fn a1((row, column): (u32, u32)) -> String {
    CellAddress::new("A", row, column).map_or_else(|| format!("R{row}C{column}"), |cell| cell.a1())
}

/// The calculation context the settings read back from Excel give.
fn context(settings: &[(String, String)]) -> Result<CalculationContext, String> {
    let setting = |name: &str| {
        settings
            .iter()
            .find(|(setting, _)| setting == name)
            .map(|(_, value)| value.as_str())
            .ok_or_else(|| format!("the worker read no setting {name}"))
    };
    let flag = |name: &str| match setting(name)? {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!(
            "the setting {name} is {other:?}, not true or false"
        )),
    };
    Ok(CalculationContext {
        date1904: flag("date1904")?,
        iteration: flag("iteration")?,
        max_iterations: setting("max_iterations")?
            .parse()
            .map_err(|_| "the setting max_iterations is not a count".to_owned())?,
        max_change: f64::from_bits(
            u64::from_str_radix(setting("max_change")?, 16)
                .map_err(|_| "the setting max_change is not binary64 bits".to_owned())?,
        ),
        precision_as_displayed: flag("precision_as_displayed")?,
    })
}

fn environment(result: &Value) -> Result<ExcelEnvironment, String> {
    let at = "the worker's environment";
    let environment = &result["environment"];
    let part = |name: &str| text(environment, name, at);
    let target = NumericTargetId::new(
        part("excel_build")?,
        part("channel")?,
        part("compatibility")?,
        part("numeric_environment")?,
    )
    .ok_or_else(|| format!("{at} names an empty target part"))?;
    let mut settings = Vec::new();
    let mut names = BTreeSet::new();
    for pair in environment["settings"]
        .as_array()
        .ok_or_else(|| format!("{at} has no settings"))?
    {
        let (Some(name), Some(value)) = (pair[0].as_str(), pair[1].as_str()) else {
            return Err(format!(
                "{at} has the setting {pair}, which is not a name and a value"
            ));
        };
        if name.is_empty() || !names.insert(name.to_owned()) {
            return Err(format!("{at} names the setting {name:?} twice or empty"));
        }
        settings.push((name.to_owned(), value.to_owned()));
    }
    Ok(ExcelEnvironment { target, settings })
}

/// The CaptureRecord version 2 of a worker's result (the module documentation describes both), or
/// why there is none. [`crate::capture`] converts its worker's result with it; the record is
/// stamped [`Acquisition::LiveCapture`], so nothing outside this crate may call it on a result
/// that no live capture wrote.
pub(crate) fn convert(result: &Value, frame: Frame) -> Result<CaptureRecord, String> {
    if result["format"] != RESULT_FORMAT || result["version"] != RESULT_VERSION {
        return Err(format!(
            "the worker's result is not {RESULT_FORMAT} version {RESULT_VERSION}"
        ));
    }
    if let Some(fatal) = result["fatal"].as_str() {
        return Err(format!("the worker failed: {fatal}"));
    }
    let environment = environment(result)?;
    let context = context(&environment.settings)?.fingerprint();
    let mut failures = Vec::new();

    let mut sheets = Vec::new();
    for (i, sheet) in array(result, "sheets")?.iter().enumerate() {
        let at = format!("sheets[{i}]");
        sheets.push(SheetEntry {
            name: text(sheet, "name", &at)?.to_owned(),
            visibility: match text(sheet, "visibility", &at)? {
                "visible" => SheetVisibility::Visible,
                "hidden" => SheetVisibility::Hidden,
                "very_hidden" => SheetVisibility::VeryHidden,
                other => return Err(format!("{at} has the visibility {other:?}")),
            },
        });
    }

    let mut cells: BTreeMap<CellAddress, &Value> = BTreeMap::new();
    for (i, cell) in array(result, "cells")?.iter().enumerate() {
        cells.insert(address(cell, &format!("cells[{i}]"))?, cell);
    }
    let mut saved_formulas: BTreeMap<CellAddress, &Value> = BTreeMap::new();
    for (i, formula) in array(result, "saved_formulas")?.iter().enumerate() {
        saved_formulas.insert(address(formula, &format!("saved_formulas[{i}]"))?, formula);
    }
    let mut formulas = Vec::new();
    let mut shared_ranges = Vec::new();
    let mut observations = Vec::new();
    // The cells that spill after the full rebuild, with their spill ranges.
    let mut spills: BTreeMap<CellAddress, Spill> = BTreeMap::new();
    for (i, formula) in array(result, "formulas")?.iter().enumerate() {
        let at = format!("formulas[{i}]");
        let cell = address(formula, &at)?;
        // The input package holds the formula as a dynamic-array formula.
        let mut dynamic_array = false;
        let kind = match (text(formula, "type", &at)?, formula["si"].as_u64()) {
            ("normal", _) => FormulaKind::Ordinary,
            ("shared", Some(group)) => {
                match formula["ref"].as_str() {
                    Some(reference) => {
                        shared_ranges.push(SharedRange::new(cell.sheet(), group, reference).ok_or_else(
                        || format!("{at}: the shared range ref {reference:?} is not a range of cells"),
                    )?);
                        FormulaKind::SharedMaster { group }
                    }
                    None => FormulaKind::SharedChild { group },
                }
            }
            ("shared", None) => return Err(format!("{at}: a shared formula without a group")),
            ("array", _) => {
                // A dynamic-array formula has cell metadata naming XLDAPR; a legacy array formula
                // has none, or cell metadata that does not name it. Either has a ref starting at
                // its cell.
                let dynamic =
                    formula["cm"].as_str().is_some() && formula["dynamic_array_metadata"] == true;
                let range = formula["ref"]
                    .as_str()
                    .and_then(parse_range)
                    .filter(|(first, _)| *first == (cell.row(), cell.column()));
                match (dynamic, range) {
                    (true, Some(_)) => {
                        dynamic_array = true;
                        FormulaKind::Ordinary
                    }
                    (false, Some((first, last))) => FormulaKind::LegacyArrayAnchor {
                        rows: last.0 - first.0 + 1,
                        columns: last.1 - first.1 + 1,
                    },
                    (_, None) => return Err(no_kind(formula, &cell, "array")),
                }
            }
            ("dataTable", _) => {
                match formula["ref"]
                    .as_str()
                    .and_then(parse_range)
                    .filter(|(first, _)| *first == (cell.row(), cell.column()))
                {
                    Some((first, last)) => FormulaKind::DataTableAnchor {
                        rows: last.0 - first.0 + 1,
                        columns: last.1 - first.1 + 1,
                    },
                    None => return Err(no_kind(formula, &cell, "dataTable")),
                }
            }
            ("data_table_member", _) => {
                let anchor = formula["anchor"].as_str().and_then(parse_a1).ok_or_else(|| {
                    format!(
                        "{at}: the worker lists {} as a cell of a data table without the relative A1 reference of its top-left cell",
                        place(&cell)
                    )
                })?;
                FormulaKind::DataTableMember {
                    anchor_row: anchor.0,
                    anchor_column: anchor.1,
                }
            }
            ("array_member", _) => {
                let anchor = formula["anchor"].as_str().and_then(parse_a1).ok_or_else(|| {
                    format!(
                        "{at}: the worker lists {} as a member of a legacy array formula without the relative A1 reference of its anchor",
                        place(&cell)
                    )
                })?;
                FormulaKind::LegacyArrayMember {
                    anchor_row: anchor.0,
                    anchor_column: anchor.1,
                }
            }
            (other, _) => return Err(no_kind(formula, &cell, other)),
        };
        let mut text_formula = None;
        match cells.get(&cell) {
            None => {
                return Err(format!(
                    "the worker read nothing for {}, neither its value nor its spill state; {UNREAD_SPILL}",
                    place(&cell)
                ));
            }
            Some(read) => {
                match read["formula"].as_str() {
                    Some(hex) => match units(hex).map(|units| String::from_utf16(&units)) {
                        Some(Ok(formula)) if !formula.is_empty() => text_formula = Some(formula),
                        _ => failures.push(failure(
                            "value",
                            &cell,
                            "Excel's formula text is empty or not valid UTF-16".to_owned(),
                        )),
                    },
                    None => failures.push(failure(
                        "value",
                        &cell,
                        "the worker read no formula text".to_owned(),
                    )),
                }
                let spill =
                    spill_of(read, &cell).map_err(|why| format!("{why}; {UNREAD_SPILL}"))?;
                let legacy = matches!(
                    kind,
                    FormulaKind::LegacyArrayAnchor { .. }
                        | FormulaKind::LegacyArrayMember { .. }
                        | FormulaKind::DataTableAnchor { .. }
                        | FormulaKind::DataTableMember { .. }
                );
                let disagreement = saved_formulas.get(&cell).and_then(|saved| {
                    if legacy && spill.is_none() {
                        saved_legacy_disagreement(saved, &cell, kind)
                    } else {
                        saved_disagreement(saved, &cell, spill.as_ref())
                    }
                });
                match spill {
                    Some(spill) => {
                        failures.push(failure(
                            STAGE_SPILL,
                            &cell,
                            format!(
                                "Excel reports a spill over {} after the full rebuild; CaptureRecord version 2 holds no array, so the cell's value, saved cache and reopened value are not recorded and it is not compared as a scalar",
                                spill.shape(&cell)
                            ),
                        ));
                        // A dynamic-array formula flagged as such is one over its own cell, with
                        // no spill range.
                        dynamic_array = false;
                        spills.insert(cell.clone(), spill);
                    }
                    None => match raw_value(&read["value"]) {
                        Ok(value) => observations.push(CellObservation {
                            cell: cell.clone(),
                            observation: Observation::ExcelRaw(value),
                        }),
                        Err(message) => failures.push(failure("value", &cell, message)),
                    },
                }
                if let Some(disagreement) = disagreement {
                    failures.push(failure("save", &cell, disagreement));
                }
            }
        }
        formulas.push(FormulaCell {
            cell,
            kind,
            formula: text_formula,
            dynamic_array,
        });
    }

    // The cells Excel lists among its formula cells that the worker found in a spill range: each
    // must lie in a spill recorded above, off its anchor, or the inventory is not known to be
    // complete.
    for (i, entry) in array(result, "spill_members")?.iter().enumerate() {
        let member = address(entry, &format!("spill_members[{i}]"))?;
        let covered = spills.iter().any(|(anchor, spill)| {
            anchor.sheet() == member.sheet()
                && *anchor != member
                && (spill.first.0..=spill.last.0).contains(&member.row())
                && (spill.first.1..=spill.last.1).contains(&member.column())
        });
        if !covered {
            failures.push(failure(
                STAGE_INVENTORY,
                &member,
                "Excel lists a formula here that the package does not hold, as a cell of a spill, and no spill the capture recorded covers it".to_owned(),
            ));
        }
    }

    let mut saved = Vec::new();
    for (i, entry) in array(result, "saved")?.iter().enumerate() {
        let at = format!("saved[{i}]");
        let cell = address(entry, &at)?;
        if spills.contains_key(&cell) {
            // The cache of a spilling cell holds only the top-left value of its array.
            continue;
        }
        let cell_type = match text(entry, "t", &at)? {
            "n" => Ok(CacheCellType::Number),
            "b" => Ok(CacheCellType::Logical),
            "e" => Ok(CacheCellType::Error),
            "str" => Ok(CacheCellType::Text),
            other => Err(format!(
                "the saved cache's type t={other:?} is not one this record holds"
            )),
        };
        let cache = cell_type.and_then(|cell_type| {
            let text = units(text(entry, "v", &at)?)
                .and_then(|units| String::from_utf16(&units).ok())
                .ok_or_else(|| "the saved cache's text is not valid UTF-16".to_owned())?;
            SavedCache::new(cell_type, &text)
        });
        match cache {
            Ok(cache) => saved.push(CellObservation {
                cell,
                observation: Observation::ExcelSavedCache(cache),
            }),
            Err(message) => failures.push(failure("save", &cell, message)),
        }
    }

    let raw: BTreeMap<&CellAddress, &CalcValue> = observations
        .iter()
        .filter_map(|observed| match &observed.observation {
            Observation::ExcelRaw(value) => Some((&observed.cell, value)),
            _ => None,
        })
        .collect();
    let mut reopened = Vec::new();
    let mut reopen_failures = Vec::new();
    for (i, entry) in array(result, "reopened")?.iter().enumerate() {
        let cell = address(entry, &format!("reopened[{i}]"))?;
        // A reopened value is recorded only for a cell that spills neither after the rebuild nor
        // after reopening; a spill that changed, or cannot be read, after reopening is a failure.
        let captured = spills.get(&cell);
        match (captured, spill_of(entry, &cell)) {
            (None, Ok(None)) => {}
            (Some(captured), Ok(Some(reopened))) if *captured == reopened => continue,
            (captured, reopened) => {
                let captured = captured.map_or_else(
                    || "no spill".to_owned(),
                    |spill| format!("a spill over {}", spill.shape(&cell)),
                );
                let message = match reopened {
                    Ok(reopened) => format!(
                        "after reopening the saved package Excel reports {}, where the capture read {captured}",
                        reopened.map_or_else(
                            || "no spill".to_owned(),
                            |spill| format!("a spill over {}", spill.shape(&cell))
                        )
                    ),
                    Err(why) => format!(
                        "after reopening the saved package, {why}; the capture read {captured}"
                    ),
                };
                reopen_failures.push(failure("reopen", &cell, message));
                continue;
            }
        }
        match raw_value(&entry["value"]) {
            Ok(value) => {
                if let Some(before) = raw.get(&cell)
                    && !same_value(before, &value)
                {
                    reopen_failures.push(failure(
                        "reopen",
                        &cell,
                        format!(
                            "after reopening the saved package Excel reads {value:?}, where the capture read {before:?}"
                        ),
                    ));
                }
                reopened.push(CellObservation {
                    cell,
                    observation: Observation::ExcelRaw(value),
                });
            }
            Err(message) => reopen_failures.push(failure("reopen", &cell, message)),
        }
    }
    failures.extend(reopen_failures);

    for (i, entry) in array(result, "failures")?.iter().enumerate() {
        let at = format!("failures[{i}]");
        let cell = if entry.get("sheet").is_some() {
            Some(address(entry, &at)?)
        } else {
            None
        };
        failures.push(CaptureFailure {
            stage: text(entry, "stage", &at)?.to_owned(),
            cell,
            message: text(entry, "message", &at)?.to_owned(),
        });
    }
    failures.extend(frame.failures);

    Ok(CaptureRecord {
        case: frame.case,
        source: frame.source.clone(),
        acquisition: Acquisition::LiveCapture,
        environment: Some(environment),
        sheets,
        formulas,
        observations,
        session: Some(CaptureSession {
            revision: frame.revision,
            operations: frame.operations,
            context,
            tool: frame.tool,
            input_package: frame.source,
            output_package: frame.output_package,
            failures,
            shared_ranges,
            saved,
            reopened,
        }),
    })
}

/// Whether two raw values are the same: numbers by their bits, everything else by equality.
fn same_value(a: &CalcValue, b: &CalcValue) -> bool {
    match (a.as_number(), b.as_number()) {
        (Some(a), Some(b)) => a.to_bits() == b.to_bits(),
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn frame() -> Frame {
        Frame {
            case: "k".to_owned(),
            source: "sha256:in".to_owned(),
            tool: "tool".to_owned(),
            revision: 0,
            operations: Vec::new(),
            output_package: Some("sha256:out".to_owned()),
            failures: vec![CaptureFailure {
                stage: "isolation".to_owned(),
                cell: None,
                message: "terminated".to_owned(),
            }],
        }
    }

    /// A worker result of the shape `worker/capture-worker.ps1` writes, for the conversion only.
    fn result() -> Value {
        json!({
            "format": RESULT_FORMAT, "version": RESULT_VERSION, "excel_pid": 4242, "fatal": null,
            "environment": {
                "excel_build": "16.0.19127.20154", "channel": "current",
                "compatibility": "calc-191029", "numeric_environment": "excel-x86-64/cpu-x86-64",
                "settings": [["date1904", "false"], ["iteration", "false"],
                             ["max_iterations", "100"], ["max_change", "3f50624dd2f1a9fc"],
                             ["precision_as_displayed", "false"], ["calculation", "automatic"]]
            },
            "sheets": [{"name": "S", "visibility": "visible"}, {"name": "H", "visibility": "very_hidden"}],
            "formulas": [
                {"sheet": "S", "row": 1, "column": 2, "type": "normal", "si": null, "ref": null},
                {"sheet": "S", "row": 1, "column": 3, "type": "shared", "si": 0, "ref": "C1:C2"},
                {"sheet": "S", "row": 2, "column": 3, "type": "shared", "si": 0, "ref": null},
                {"sheet": "H", "row": 1, "column": 1, "type": "normal", "si": null, "ref": null}
            ],
            "cells": [
                {"sheet": "S", "row": 1, "column": 2, "formula": "003d0031002f0030", "value": {"error_code": 2007}, "spill": null},
                {"sheet": "S", "row": 1, "column": 3, "formula": "003d00410031", "value": {"number": "3ff0000000000000"}, "spill": null},
                {"sheet": "S", "row": 2, "column": 3, "formula": "003d00410032", "value": {"unsupported": "System.Object[,]"}, "spill": null},
                {"sheet": "H", "row": 1, "column": 1, "formula": "003d00220061002200260022d83dde000022", "value": {"text": "0061d83d"}, "spill": null}
            ],
            "saved": [
                {"sheet": "S", "row": 1, "column": 2, "t": "e", "v": "0023004400490056002f00300021"},
                {"sheet": "S", "row": 1, "column": 3, "t": "n", "v": "0031"},
                {"sheet": "H", "row": 1, "column": 1, "t": "d", "v": "0031"}
            ],
            "saved_formulas": [
                {"sheet": "S", "row": 1, "column": 2, "type": "normal", "si": null, "ref": null, "cm": null},
                {"sheet": "S", "row": 1, "column": 3, "type": "shared", "si": 0, "ref": "C1:C2", "cm": null},
                {"sheet": "S", "row": 2, "column": 3, "type": "shared", "si": 0, "ref": null, "cm": null},
                {"sheet": "H", "row": 1, "column": 1, "type": "normal", "si": null, "ref": null, "cm": null}
            ],
            "spill_members": [],
            "reopened": [
                {"sheet": "S", "row": 1, "column": 2, "value": {"error_code": 2007}, "spill": null},
                {"sheet": "S", "row": 1, "column": 3, "value": {"number": "3ff0000000000001"}, "spill": null}
            ],
            "failures": [{"stage": "inventory", "sheet": "S", "row": 9, "column": 9, "message": "extra"}]
        })
    }

    #[test]
    fn a_worker_result_becomes_a_version_2_record() {
        let record = convert(&result(), frame()).unwrap();
        assert_eq!(record.acquisition, Acquisition::LiveCapture);
        let environment = record.environment.as_ref().unwrap();
        assert_eq!(environment.target.excel_build(), "16.0.19127.20154");
        assert_eq!(environment.settings.len(), 6);
        let session = record.session.as_ref().unwrap();
        assert_eq!(
            session.context,
            "excel-calc-context/1;date1904=false;iteration=false;max_iterations=100;max_change=3f50624dd2f1a9fc;precision_as_displayed=false"
        );
        assert_eq!(session.input_package, "sha256:in");
        assert_eq!(
            session.shared_ranges,
            [SharedRange::new("S", 0, "C1:C2").unwrap()]
        );
        let kinds: Vec<FormulaKind> = record.formulas.iter().map(|f| f.kind).collect();
        assert_eq!(
            kinds,
            [
                FormulaKind::Ordinary,
                FormulaKind::SharedMaster { group: 0 },
                FormulaKind::SharedChild { group: 0 },
                FormulaKind::Ordinary
            ]
        );
        assert_eq!(record.formulas[0].formula.as_deref(), Some("=1/0"));
        // A formula text with an astral character survives its UTF-16 hex; a text value may hold a
        // lone surrogate, as Excel text can.
        assert_eq!(
            record.formulas[3].formula.as_deref(),
            Some("=\"a\"&\"\u{1f600}\"")
        );
        assert_eq!(
            record.observations[2].observation,
            Observation::ExcelRaw(CalcValue::text(ExcelText::from_utf16_code_units(vec![
                0x61, 0xd83d
            ])))
        );
        // The unsupported Value2 of S!C2 is a failure, not an observation.
        let observed: Vec<String> = record
            .observations
            .iter()
            .map(|o| format!("{}!{}", o.cell.sheet(), o.cell.a1()))
            .collect();
        assert_eq!(observed, ["S!B1", "S!C1", "H!A1"]);
        assert_eq!(
            record.observations[0].observation,
            Observation::ExcelRaw(CalcValue::error(WorksheetErrorCode::Div0))
        );
        // Saved caches: the t="d" cache of H!A1 is a failure.
        assert_eq!(session.saved.len(), 2);
        let stages: Vec<(String, Option<String>)> = session
            .failures
            .iter()
            .map(|f| {
                (
                    f.stage.clone(),
                    f.cell.as_ref().map(|c| format!("{}!{}", c.sheet(), c.a1())),
                )
            })
            .collect();
        assert_eq!(
            stages,
            [
                ("value".to_owned(), Some("S!C2".to_owned())),
                ("save".to_owned(), Some("H!A1".to_owned())),
                ("reopen".to_owned(), Some("S!C1".to_owned())),
                ("inventory".to_owned(), Some("S!I9".to_owned())),
                ("isolation".to_owned(), None),
            ]
        );
        // The record round-trips through the version 2 codec.
        let encoded = record.encode().unwrap();
        assert_eq!(CaptureRecord::decode(encoded.as_bytes()).unwrap(), record);
    }

    /// A data table's top-left cell, its ref starting at the cell, is a data table anchor over that
    /// area (dc-10y); a saved package that does not hold the table there is a save failure of the
    /// cell.
    #[test]
    fn a_data_table_is_recorded_over_its_area() {
        let mut table = result();
        table["formulas"][0]["type"] = json!("dataTable");
        table["formulas"][0]["ref"] = json!("B1");
        let record = convert(&table, frame()).unwrap();
        assert_eq!(
            record.formulas[0].kind,
            FormulaKind::DataTableAnchor {
                rows: 1,
                columns: 1
            }
        );
        let failures = format!("{:?}", record.session.as_ref().unwrap().failures);
        assert!(
            failures.contains("the input package holds a data table over S!B1:B1"),
            "{failures}"
        );
        let encoded = record.encode().unwrap();
        assert_eq!(CaptureRecord::decode(encoded.as_bytes()).unwrap(), record);
    }

    #[test]
    fn array_formulas_without_a_range_and_fatal_results_are_refused() {
        let mut array = result();
        array["formulas"][0]["type"] = json!("array");
        let error = convert(&array, frame()).err().unwrap();
        assert!(
            error.contains("S!B1 holds a formula of type \"array\" (ref none, cell metadata none)"),
            "{error}"
        );
        let mut table = result();
        table["formulas"][0]["type"] = json!("dataTable");
        table["formulas"][0]["ref"] = json!("A1:A3");
        let error = convert(&table, frame()).err().unwrap();
        assert!(
            error.starts_with("S!B1 holds a formula of type \"dataTable\" (ref A1:A3, cell metadata none), for which CaptureRecord version 2 has no kind"),
            "{error}"
        );
        let mut fatal = result();
        fatal["fatal"] = json!("Excel refused to open the workbook");
        let error = convert(&fatal, frame()).err().unwrap();
        assert_eq!(
            error,
            "the worker failed: Excel refused to open the workbook"
        );
        let mut no_context = result();
        no_context["environment"]["settings"] = json!([["date1904", "false"]]);
        let error = convert(&no_context, frame()).err().unwrap();
        assert!(error.contains("no setting iteration"), "{error}");
        let mut wrong = result();
        wrong["format"] = json!("other");
        assert!(convert(&wrong, frame()).is_err());
    }

    /// A single-cell formula saved as a dynamic array (type array, cell metadata cm, a ref of the
    /// cell itself) that does not spill is an ordinary formula flagged as dynamic; an array formula
    /// (dynamic or legacy) whose ref does not start at its cell or that has none is refused. A
    /// legacy array formula with a ref starting at its cell is recorded (the tests below).
    #[test]
    fn a_single_cell_dynamic_array_formula_is_ordinary_and_flagged() {
        for reference in ["B1", "B1:B1"] {
            let mut dynamic = result();
            dynamic["formulas"][0]["type"] = json!("array");
            dynamic["formulas"][0]["ref"] = json!(reference);
            dynamic["formulas"][0]["cm"] = json!("1");
            dynamic["formulas"][0]["dynamic_array_metadata"] = json!(true);
            let record = convert(&dynamic, frame()).unwrap();
            assert_eq!(
                record.formulas[0].kind,
                FormulaKind::Ordinary,
                "{reference}"
            );
            assert!(record.formulas[0].dynamic_array, "{reference}");
            assert!(record.formulas[1..].iter().all(|f| !f.dynamic_array));
            // The rest of the record is the one the plain formula gives.
            let mut plain = convert(&result(), frame()).unwrap();
            plain.formulas[0].dynamic_array = true;
            assert_eq!(record, plain);
            let encoded = record.encode().unwrap();
            assert_eq!(CaptureRecord::decode(encoded.as_bytes()).unwrap(), record);
        }
        // Each refused row is refused for its ref, with cell metadata that names XLDAPR (a dynamic
        // array) or none or cell metadata that does not name it (a legacy array).
        for (reference, cm, xldapr, what) in [
            (Some("C1"), Some("1"), true, "a ref that names another cell"),
            (
                Some("A1:B3"),
                Some("1"),
                true,
                "a ref that starts at another cell",
            ),
            (Some("B1:"), Some("1"), true, "a ref that is not a range"),
            (None, Some("1"), true, "no ref"),
            (
                Some("C1"),
                None,
                false,
                "a legacy ref that names another cell",
            ),
            (
                Some("A1:B3"),
                Some("1"),
                false,
                "a legacy ref that starts at another cell",
            ),
            (Some("B3:B1"), None, false, "a legacy ref that runs upwards"),
            (None, None, false, "a legacy array without a ref"),
        ] {
            let mut array = result();
            array["formulas"][0]["type"] = json!("array");
            array["formulas"][0]["ref"] = json!(reference);
            array["formulas"][0]["cm"] = json!(cm);
            array["formulas"][0]["dynamic_array_metadata"] = json!(xldapr);
            let error = convert(&array, frame()).err().unwrap();
            assert!(
                error.starts_with(&format!(
                    "S!B1 holds a formula of type \"array\" (ref {}, cell metadata {})",
                    reference.unwrap_or("none"),
                    cm.unwrap_or("none")
                )),
                "{what}: {error}"
            );
        }
    }

    /// [`result`] with S!B1 a legacy array formula over `reference` (cell metadata `cm`, which
    /// names no dynamic-array metadata), in the input and in the saved package, and each cell of
    /// `members` (row, column) a member of it: listed by the worker with its anchor B1, read with
    /// the value 20 and saved as a member with that value, and read again after reopening.
    fn with_legacy(reference: &str, cm: Option<&str>, members: &[(u32, u32)]) -> Value {
        let mut with = result();
        for list in ["formulas", "saved_formulas"] {
            with[list][0]["type"] = json!("array");
            with[list][0]["ref"] = json!(reference);
            with[list][0]["cm"] = json!(cm);
            with[list][0]["dynamic_array_metadata"] = json!(false);
        }
        for (row, column) in members {
            let member = json!({"sheet": "S", "row": row, "column": column, "type": "array_member", "si": null, "ref": null, "cm": null, "dynamic_array_metadata": false, "anchor": "B1"});
            for list in ["formulas", "saved_formulas"] {
                with[list].as_array_mut().unwrap().push(member.clone());
            }
            with["cells"].as_array_mut().unwrap().push(json!({"sheet": "S", "row": row, "column": column, "formula": "003d00410031003a00410032002a00310030", "value": {"number": "4034000000000000"}, "spill": null}));
            with["saved"].as_array_mut().unwrap().push(
                json!({"sheet": "S", "row": row, "column": column, "t": "n", "v": "00320030"}),
            );
            with["reopened"].as_array_mut().unwrap().push(json!({"sheet": "S", "row": row, "column": column, "value": {"number": "4034000000000000"}, "spill": null}));
        }
        with
    }

    /// A legacy array formula (type array with no cell metadata, or cell metadata that does not
    /// name XLDAPR, its ref starting at its cell) is recorded over its range: the anchor with its
    /// range and each member the worker lists naming its anchor, each with the value Excel shows,
    /// its saved cache and its reopened value, and no failure when the saved package holds them
    /// alike. The record round-trips.
    #[test]
    fn a_legacy_array_formula_is_recorded_over_its_range() {
        let anchor = |rows, columns| FormulaKind::LegacyArrayAnchor { rows, columns };
        let member = FormulaKind::LegacyArrayMember {
            anchor_row: 1,
            anchor_column: 2,
        };
        for (reference, cm, members, kinds) in [
            ("B1", None, vec![], vec![anchor(1, 1)]),
            ("B1:B1", Some("1"), vec![], vec![anchor(1, 1)]),
            ("B1:B2", None, vec![(2, 2)], vec![anchor(2, 1), member]),
            (
                "B1:B3",
                Some("1"),
                vec![(2, 2), (3, 2)],
                vec![anchor(3, 1), member, member],
            ),
        ] {
            let record = convert(&with_legacy(reference, cm, &members), frame())
                .unwrap_or_else(|e| panic!("{reference}: {e}"));
            let held: Vec<FormulaKind> = record
                .formulas
                .iter()
                .filter(|f| f.cell.column() == 2)
                .map(|f| f.kind)
                .collect();
            assert_eq!(held, kinds, "{reference}");
            assert!(record.formulas.iter().all(|f| !f.dynamic_array));
            assert_eq!(record.formulas.len(), 4 + members.len());
            let session = record.session.as_ref().unwrap();
            for (row, column) in &members {
                let at = CellAddress::new("S", *row, *column).unwrap();
                let a1 = place(&at);
                assert!(listed(&record.observations).contains(&a1), "{a1}");
                assert!(listed(&session.saved).contains(&a1), "{a1}");
                assert!(listed(&session.reopened).contains(&a1), "{a1}");
                assert_eq!(
                    record
                        .formulas
                        .iter()
                        .find(|f| f.cell == at)
                        .unwrap()
                        .formula
                        .as_deref(),
                    Some("=A1:A2*10")
                );
                assert_eq!(failures_at(&record, "S", &at.a1()), []);
            }
            assert_eq!(failures_at(&record, "S", "B1"), []);
            let encoded = record.encode().unwrap();
            assert_eq!(CaptureRecord::decode(encoded.as_bytes()).unwrap(), record);
        }
        // A member the worker lists without its anchor's reference refuses the conversion.
        let mut unanchored = with_legacy("B1:B2", None, &[(2, 2)]);
        unanchored["formulas"][4]["anchor"] = json!("$B$1");
        let error = convert(&unanchored, frame()).unwrap_err();
        assert_eq!(
            error,
            "formulas[4]: the worker lists S!B2 as a member of a legacy array formula without the relative A1 reference of its anchor"
        );
    }

    /// The package Excel saved must hold a legacy array formula's anchor as a legacy array formula
    /// over the same range, and each member as a member of that anchor: a saved anchor over another
    /// range, saved as a dynamic-array formula or as an ordinary one, and a saved member of another
    /// anchor or saved as another type, are save failures of their cells; the values are still
    /// recorded.
    #[test]
    fn a_saved_legacy_array_that_disagrees_is_a_save_failure() {
        let save = |record: &CaptureRecord, a1: &str| -> Vec<String> {
            failures_at(record, "S", a1)
                .into_iter()
                .filter(|(stage, _)| stage == "save")
                .map(|(_, message)| message)
                .collect()
        };
        let input = "the input package holds a legacy array formula over S!B1:B2, and the saved package holds the formula as type";
        for (change, expected) in [
            (
                json!({"ref": "B1:B3"}),
                format!("{input} \"array\" with ref B1:B3 and cell metadata none"),
            ),
            (
                json!({"cm": "1", "dynamic_array_metadata": true}),
                format!(
                    "{input} \"array\" with ref B1:B2 and cell metadata 1 (the dynamic-array metadata XLDAPR)"
                ),
            ),
            (
                json!({"type": "normal", "ref": null}),
                format!("{input} \"normal\" with ref none and cell metadata none"),
            ),
        ] {
            let mut changed = with_legacy("B1:B2", None, &[(2, 2)]);
            for (name, value) in change.as_object().unwrap() {
                changed["saved_formulas"][0][name] = value.clone();
            }
            let record = convert(&changed, frame()).unwrap();
            assert_eq!(save(&record, "B1"), [expected], "{change}");
            assert_eq!(save(&record, "B2"), Vec::<String>::new(), "{change}");
            assert!(listed(&record.observations).contains(&"S!B1".to_owned()));
        }
        let member = "the input package holds this cell as a member of the legacy array formula anchored at S!B1, and the saved package holds the formula as type";
        for (change, expected) in [
            (
                json!({"anchor": "A1"}),
                format!(
                    "{member} \"array_member\" with ref none and cell metadata none and anchor A1"
                ),
            ),
            (
                json!({"type": "normal", "anchor": null}),
                format!("{member} \"normal\" with ref none and cell metadata none"),
            ),
        ] {
            let mut changed = with_legacy("B1:B2", None, &[(2, 2)]);
            for (name, value) in change.as_object().unwrap() {
                changed["saved_formulas"][4][name] = value.clone();
            }
            let record = convert(&changed, frame()).unwrap();
            assert_eq!(save(&record, "B2"), [expected], "{change}");
            assert_eq!(save(&record, "B1"), Vec::<String>::new(), "{change}");
            assert!(listed(&record.observations).contains(&"S!B2".to_owned()));
        }
    }

    fn spill(reference: &str, rows: u64, columns: u64) -> Value {
        json!({"ref": reference, "rows": rows, "columns": columns})
    }

    /// S!B1's formula as the saved package holds it: a dynamic-array formula with `reference`
    /// (cell metadata 1, which names the dynamic-array metadata XLDAPR).
    fn saved_array(reference: &str) -> Value {
        json!({"sheet": "S", "row": 1, "column": 2, "type": "array", "si": null, "ref": reference, "cm": "1", "dynamic_array_metadata": true})
    }

    /// [`result`] with S!B1 a dynamic-array formula whose input ref is `input`, the spill the
    /// worker read for it after the rebuild (`captured`) and after reopening (`reopened`), and
    /// the saved package's formula of it (`saved`).
    fn with_spill(input: &str, captured: Value, reopened: Value, saved: Value) -> Value {
        let mut with = result();
        with["formulas"][0]["type"] = json!("array");
        with["formulas"][0]["ref"] = json!(input);
        with["formulas"][0]["cm"] = json!("1");
        with["formulas"][0]["dynamic_array_metadata"] = json!(true);
        with["cells"][0]["spill"] = captured;
        with["reopened"][0]["spill"] = reopened;
        with["saved_formulas"][0] = saved;
        with
    }

    /// The failures of `record` at `sheet`!`a1`, as (stage, message).
    fn failures_at(record: &CaptureRecord, sheet: &str, a1: &str) -> Vec<(String, String)> {
        record
            .session
            .as_ref()
            .unwrap()
            .failures
            .iter()
            .filter(|f| {
                f.cell
                    .as_ref()
                    .is_some_and(|c| c.sheet() == sheet && c.a1() == a1)
            })
            .map(|f| (f.stage.clone(), f.message.clone()))
            .collect()
    }

    /// The cells `list` holds an entry for, as `sheet!A1`.
    fn listed(list: &[CellObservation]) -> Vec<String> {
        list.iter().map(|o| place(&o.cell)).collect()
    }

    const NOT_A_SCALAR: &str = "after the full rebuild; CaptureRecord version 2 holds no array, so the cell's value, saved cache and reopened value are not recorded and it is not compared as a scalar";

    /// A cell that spills after the full rebuild keeps its place in the inventory, gets a spill
    /// failure naming its range, rows and columns, and has no raw observation, saved cache or
    /// reopened value: its Value2 (here #DIV/0!, the top-left value) is never recorded as its
    /// value. Whether it spills is Excel's report, whatever the input package says: H!A1, an
    /// ordinary formula in the input, spills too, and its saved package, which holds it as an
    /// ordinary formula, disagrees.
    #[test]
    fn a_cell_that_spills_is_recorded_as_a_spill_never_as_a_scalar() {
        for (reference, rows, columns, shape) in [
            ("B1:B3", 3, 1, "S!B1:B3 (3 rows by 1 column)"),
            ("B1:D1", 1, 3, "S!B1:D1 (1 row by 3 columns)"),
            ("B1:D2", 2, 3, "S!B1:D2 (2 rows by 3 columns)"),
        ] {
            let mut spilling = with_spill(
                reference,
                spill(reference, rows, columns),
                spill(reference, rows, columns),
                saved_array(reference),
            );
            spilling["cells"][3]["spill"] = spill("A1:A2", 2, 1);
            spilling["reopened"]
                .as_array_mut()
                .unwrap()
                .push(json!({"sheet": "H", "row": 1, "column": 1, "value": {"text": "0061"}, "spill": spill("A1:A2", 2, 1)}));
            let record = convert(&spilling, frame()).unwrap();
            let session = record.session.as_ref().unwrap();
            // The inventory is unchanged: B1 is an ordinary formula, not flagged, since a flagged
            // formula is one over its own cell with no spill range.
            assert_eq!(record.formulas.len(), 4);
            assert_eq!(
                (record.formulas[0].kind, record.formulas[0].dynamic_array),
                (FormulaKind::Ordinary, false)
            );
            assert_eq!(listed(&record.observations), ["S!C1"]);
            assert_eq!(listed(&session.saved), ["S!C1"]);
            assert_eq!(listed(&session.reopened), ["S!C1"]);
            assert_eq!(
                failures_at(&record, "S", "B1"),
                [(
                    STAGE_SPILL.to_owned(),
                    format!("Excel reports a spill over {shape} {NOT_A_SCALAR}")
                )]
            );
            assert_eq!(
                failures_at(&record, "H", "A1"),
                [
                    (
                        STAGE_SPILL.to_owned(),
                        format!("Excel reports a spill over H!A1:A2 (2 rows by 1 column) {NOT_A_SCALAR}")
                    ),
                    (
                        "save".to_owned(),
                        "Excel reports a spill over H!A1:A2 (2 rows by 1 column) after the full rebuild, and the saved package holds the formula as type \"normal\" with ref none and cell metadata none".to_owned()
                    )
                ]
            );
            // The rest of the record is the one the plain result gives.
            assert_eq!(failures_at(&record, "S", "C2").len(), 1);
            let encoded = record.encode().unwrap();
            assert_eq!(CaptureRecord::decode(encoded.as_bytes()).unwrap(), record);
        }
    }

    /// A spill that cannot be read after the rebuild, or whose shape is not a range starting at
    /// the cell with the rows and columns Excel reports, refuses the whole conversion.
    #[test]
    fn a_spill_whose_shape_cannot_be_read_refuses_the_capture() {
        let unread =
            json!({"unread": "Range.HasSpill reads as True and Range.SpillingToRange as nothing"});
        for (captured, expected) in [
            (
                unread,
                "Excel's spill state of S!B1 cannot be read: Range.HasSpill reads as True and Range.SpillingToRange as nothing",
            ),
            (
                spill("C1:C3", 3, 1),
                "Excel reports S!B1 spilling over \"C1:C3\" with 3 rows and 1 column, which is not a range of that shape starting at the cell",
            ),
            (
                spill("B1:B3", 4, 1),
                "Excel reports S!B1 spilling over \"B1:B3\" with 4 rows and 1 column",
            ),
            (
                spill("B1:B3", 3, 2),
                "Excel reports S!B1 spilling over \"B1:B3\" with 3 rows and 2 columns",
            ),
            (
                spill("B3:B1", 3, 1),
                "Excel reports S!B1 spilling over \"B3:B1\"",
            ),
            (
                spill("$B$1:$B$3", 3, 1),
                "Excel reports S!B1 spilling over \"$B$1:$B$3\"",
            ),
            (
                json!({"ref": "B1:B3"}),
                "is not a spill range of a ref, rows and columns",
            ),
            (
                json!(true),
                "is not a spill range of a ref, rows and columns",
            ),
        ] {
            let refused = with_spill("B1:B3", captured.clone(), Value::Null, saved_array("B1:B3"));
            let error = convert(&refused, frame()).unwrap_err();
            assert!(error.contains(expected), "{captured}: {error}");
            assert!(
                error.ends_with("so a capture whose spill cannot be read after the full rebuild is refused rather than written with that value passing for a scalar"),
                "{error}"
            );
        }
        // A cell entry without its spill member is refused as well: version 1 results had none.
        let mut missing = result();
        missing["cells"][1].as_object_mut().unwrap().remove("spill");
        let error = convert(&missing, frame()).unwrap_err();
        assert!(
            error.starts_with("the worker read no spill state for S!C1;"),
            "{error}"
        );
        // So is a formula cell the worker read nothing for.
        let mut unread = result();
        unread["cells"].as_array_mut().unwrap().remove(0);
        let error = convert(&unread, frame()).unwrap_err();
        assert!(
            error.starts_with(
                "the worker read nothing for S!B1, neither its value nor its spill state; a spilling cell's Value2"
            ),
            "{error}"
        );
        let mut first = result();
        first["version"] = json!(1);
        assert!(convert(&first, frame()).is_err());
    }

    /// The saved package must hold a spill as a dynamic-array formula (type array with cell
    /// metadata) whose ref is the range Excel reports, and must not declare a spill range for a
    /// cell Excel reports no spill for; a disagreement is a save failure of that cell, beside
    /// the spill failure of a cell that spills.
    #[test]
    fn a_saved_ref_that_disagrees_with_the_spill_is_a_save_failure() {
        let spilling = |saved: Value| {
            let record = convert(
                &with_spill("B1:B3", spill("B1:B3", 3, 1), spill("B1:B3", 3, 1), saved),
                frame(),
            )
            .unwrap();
            let failures = failures_at(&record, "S", "B1");
            assert_eq!(failures[0].0, STAGE_SPILL, "{failures:?}");
            assert!(!listed(&record.observations).contains(&"S!B1".to_owned()));
            failures[1..].to_vec()
        };
        let held = |held: &str| {
            vec![(
                "save".to_owned(),
                format!(
                    "Excel reports a spill over S!B1:B3 (3 rows by 1 column) after the full rebuild, and the saved package holds the formula as {held}"
                ),
            )]
        };
        assert_eq!(
            spilling(saved_array("B1:B4")),
            held(
                "type \"array\" with ref B1:B4 and cell metadata 1 (the dynamic-array metadata XLDAPR)"
            )
        );
        let mut legacy = saved_array("B1:B3");
        legacy["cm"] = Value::Null;
        legacy["dynamic_array_metadata"] = json!(false);
        assert_eq!(
            spilling(legacy),
            held("type \"array\" with ref B1:B3 and cell metadata none")
        );
        // Cell metadata that is present but does not name the dynamic-array metadata, and a saved
        // formula that does not say whether it does, are not a dynamic-array formula.
        let mut other = saved_array("B1:B3");
        other["dynamic_array_metadata"] = json!(false);
        assert_eq!(
            spilling(other),
            held(
                "type \"array\" with ref B1:B3 and cell metadata 1, which does not name the dynamic-array metadata XLDAPR"
            )
        );
        let mut unsaid = saved_array("B1:B3");
        unsaid
            .as_object_mut()
            .unwrap()
            .remove("dynamic_array_metadata");
        assert_eq!(
            spilling(unsaid),
            held(
                "type \"array\" with ref B1:B3 and cell metadata 1, which does not name the dynamic-array metadata XLDAPR"
            )
        );
        let mut normal = saved_array("B1:B3");
        normal["type"] = json!("normal");
        normal["ref"] = Value::Null;
        normal["cm"] = Value::Null;
        assert_eq!(
            spilling(normal),
            held("type \"normal\" with ref none and cell metadata none")
        );
        // The range compared is the one the ref names, in either form.
        assert_eq!(spilling(saved_array("B1:B3")), []);
        // A cell with no saved formula is the worker's save failure to report, not this check's.
        let mut unsaved = with_spill(
            "B1:B3",
            spill("B1:B3", 3, 1),
            spill("B1:B3", 3, 1),
            saved_array("B1:B3"),
        );
        unsaved["saved_formulas"].as_array_mut().unwrap().remove(0);
        let record = convert(&unsaved, frame()).unwrap();
        assert_eq!(failures_at(&record, "S", "B1").len(), 1);

        // No spill, and the saved package declares one: a save failure; the value Excel shows is
        // still recorded.
        let record = convert(
            &with_spill("B1", Value::Null, Value::Null, saved_array("B1:B3")),
            frame(),
        )
        .unwrap();
        assert_eq!(
            failures_at(&record, "S", "B1"),
            [(
                "save".to_owned(),
                "Excel reports no spill after the full rebuild, and the saved package holds the formula as type \"array\" with ref B1:B3 and cell metadata 1 (the dynamic-array metadata XLDAPR)".to_owned()
            )]
        );
        assert_eq!(listed(&record.observations), ["S!B1", "S!C1", "H!A1"]);
    }

    /// The cells Excel lists among its formula cells that the worker found in a spill range are
    /// never formula cells of the record: each that lies in a spill the conversion records, off
    /// its anchor, leaves no trace; one outside every recorded spill (beside it, on another sheet,
    /// the anchor itself, or in a range the conversion does not record as a spill) is an inventory
    /// failure naming it, which calc-verdict refuses to judge.
    #[test]
    fn spill_members_must_lie_in_a_recorded_spill() {
        let member = |sheet: &str, row: u32, column: u32| json!({"sheet": sheet, "row": row, "column": column});
        let with_members = |captured: Value, members: Vec<Value>| {
            let mut with = with_spill("B1:B3", captured.clone(), captured, saved_array("B1:B3"));
            with["spill_members"] = json!(members);
            convert(&with, frame()).unwrap()
        };
        // The inventory failures of a record, as `sheet!A1`; [`result`] carries one at S!I9.
        let inventory = |record: &CaptureRecord| -> Vec<String> {
            record
                .session
                .as_ref()
                .unwrap()
                .failures
                .iter()
                .filter(|f| f.stage == STAGE_INVENTORY)
                .map(|f| place(f.cell.as_ref().unwrap()))
                .collect()
        };
        let inside = vec![member("S", 2, 2), member("S", 3, 2)];
        let record = with_members(spill("B1:B3", 3, 1), inside.clone());
        assert_eq!(inventory(&record), ["S!I9"]);
        assert_eq!(record.formulas.len(), 4);
        for (outside, a1) in [
            (member("S", 4, 2), "B4"),
            (member("S", 2, 1), "A2"),
            (member("H", 2, 2), "B2"),
            (member("S", 1, 2), "B1"),
        ] {
            let sheet = outside["sheet"].as_str().unwrap().to_owned();
            let mut members = inside.clone();
            members.push(outside);
            let record = with_members(spill("B1:B3", 3, 1), members);
            let failures: Vec<(String, String)> = failures_at(&record, &sheet, a1)
                .into_iter()
                .filter(|(stage, _)| stage == STAGE_INVENTORY)
                .collect();
            assert_eq!(
                failures,
                [(
                    STAGE_INVENTORY.to_owned(),
                    "Excel lists a formula here that the package does not hold, as a cell of a spill, and no spill the capture recorded covers it".to_owned()
                )],
                "{sheet}!{a1}"
            );
            assert_eq!(record.formulas.len(), 4);
        }
        // With no spill recorded, every member is an inventory failure.
        let record = with_members(Value::Null, inside);
        assert_eq!(inventory(&record), ["S!B2", "S!B3", "S!I9"]);
    }

    /// A formula the input package holds as a dynamic array over a spill range that does not spill
    /// after the rebuild is recorded as Excel shows it, flagged as a dynamic-array formula, with no
    /// failure when the saved package holds it over its own cell.
    #[test]
    fn a_dynamic_array_input_that_no_longer_spills_is_recorded_as_excel_shows_it() {
        let record = convert(
            &with_spill("B1:B3", Value::Null, Value::Null, saved_array("B1")),
            frame(),
        )
        .unwrap();
        assert_eq!(
            (record.formulas[0].kind, record.formulas[0].dynamic_array),
            (FormulaKind::Ordinary, true)
        );
        assert_eq!(failures_at(&record, "S", "B1"), []);
        assert_eq!(
            record.observations[0],
            CellObservation {
                cell: CellAddress::new("S", 1, 2).unwrap(),
                observation: Observation::ExcelRaw(CalcValue::error(WorksheetErrorCode::Div0)),
            }
        );
        assert_eq!(
            listed(&record.session.as_ref().unwrap().reopened),
            ["S!B1", "S!C1"]
        );
    }

    /// The spill read after reopening the saved package must be the one captured: a spill that
    /// appears, disappears, changes or cannot be read after reopening is a reopen failure of the
    /// cell, and its reopened value is not recorded.
    #[test]
    fn a_spill_that_changes_after_reopening_is_a_reopen_failure() {
        let b1b3 = || spill("B1:B3", 3, 1);
        for (captured, reopened, expected) in [
            (
                b1b3(),
                Value::Null,
                Some(
                    "after reopening the saved package Excel reports no spill, where the capture read a spill over S!B1:B3 (3 rows by 1 column)",
                ),
            ),
            (
                b1b3(),
                spill("B1:B4", 4, 1),
                Some(
                    "after reopening the saved package Excel reports a spill over S!B1:B4 (4 rows by 1 column), where the capture read a spill over S!B1:B3 (3 rows by 1 column)",
                ),
            ),
            (
                b1b3(),
                json!({"unread": "x"}),
                Some(
                    "after reopening the saved package, Excel's spill state of S!B1 cannot be read: x; the capture read a spill over S!B1:B3 (3 rows by 1 column)",
                ),
            ),
            (
                Value::Null,
                b1b3(),
                Some(
                    "after reopening the saved package Excel reports a spill over S!B1:B3 (3 rows by 1 column), where the capture read no spill",
                ),
            ),
            (b1b3(), b1b3(), None),
        ] {
            let saved = if captured.is_null() {
                saved_array("B1")
            } else {
                saved_array("B1:B3")
            };
            let record = convert(
                &with_spill("B1:B3", captured.clone(), reopened, saved),
                frame(),
            )
            .unwrap();
            let reopen: Vec<String> = failures_at(&record, "S", "B1")
                .into_iter()
                .filter(|(stage, _)| stage == "reopen")
                .map(|(_, message)| message)
                .collect();
            assert_eq!(reopen, expected.into_iter().collect::<Vec<_>>());
            assert_eq!(listed(&record.session.as_ref().unwrap().reopened), ["S!C1"]);
        }
    }

    #[test]
    fn raw_values_and_error_codes() {
        assert_eq!(raw_value(&json!({"empty": true})), Ok(CalcValue::empty()));
        assert_eq!(
            raw_value(&json!({"number": "8000000000000000"}))
                .unwrap()
                .as_number()
                .map(f64::to_bits),
            Some(1 << 63)
        );
        assert_eq!(
            raw_value(&json!({"logical": false})),
            Ok(CalcValue::logical(false))
        );
        assert_eq!(
            raw_value(&json!({"unread": "the object is closed"})),
            Err("Range.Value2 cannot be read: the object is closed".to_owned())
        );
        for bad in [
            json!({"number": "1"}),
            json!({"text": "006"}),
            json!({"error_code": 2048}),
            json!({"unsupported": "System.DateTime"}),
            json!({"unread": 1}),
            json!({"empty": true, "logical": true}),
            json!({"date": "x"}),
        ] {
            assert!(raw_value(&bad).is_err(), "{bad}");
        }
        let codes: Vec<u64> = (2000..=2051)
            .filter(|code| error_from_code(*code).is_some())
            .collect();
        assert_eq!(
            codes,
            [
                2000, 2007, 2015, 2023, 2029, 2036, 2042, 2043, 2045, 2046, 2047, 2049, 2050
            ]
        );
    }
}
