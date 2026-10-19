//! The worker's result file turned into a CaptureRecord version 2.
//!
//! The result (`dnacalc-capture-worker-result`, version 1) is what `worker/capture-worker.ps1`
//! writes:
//!
//! ```text
//! {"format": "dnacalc-capture-worker-result", "version": 1, "excel_pid": <pid> | null,
//!  "fatal": null | "<message>",
//!  "environment": null | {"excel_build": "...", "channel": "...", "compatibility": "...",
//!                         "numeric_environment": "...", "settings": [["<name>", "<value>"], ...]},
//!  "sheets": [{"name": "...", "visibility": "visible" | "hidden" | "very_hidden"}, ...],
//!  "formulas": [{"sheet", "row", "column", "type": "normal" | "shared" | "array" | ...,
//!                "si": n | null, "ref": "C1:C3" | null, "cm": "<cell metadata index>" | null}, ...],
//!  "cells": [{"sheet", "row", "column", "formula": "<UTF-16 hex>" | null, "value": <raw>}, ...],
//!  "saved": [{"sheet", "row", "column", "t": "n" | "b" | "e" | "str" | ..., "v": "<UTF-16 hex>"}],
//!  "reopened": [{"sheet", "row", "column", "value": <raw>}, ...],
//!  "failures": [{"stage", ["sheet", "row", "column",] "message"}, ...]}
//! ```
//!
//! where `<raw>` is `{"empty": true}`, `{"number": "<16 hex digits: the binary64 bits>"}`,
//! `{"text": "<UTF-16 hex>"}`, `{"logical": b}`, `{"error_code": <Excel's CVErr code>}`,
//! `{"unsupported": "<.NET type of the Value2>"}` or `{"unread": "<why Value2 cannot be read>"}`.
//!
//! The inventory is the worker's `formulas`, read from the package. A cell value, cache or formula
//! text that cannot be represented becomes a capture failure for that cell, so the record shows
//! what was not observed. A single-cell dynamic-array formula (type `array` with cell metadata
//! `cm`, its `ref` the cell itself), which Excel with dynamic arrays saves for a formula that
//! could return an array, is an ordinary formula flagged as dynamic
//! ([`FormulaCell::dynamic_array`]). A formula of a type the record has no kind for (a legacy
//! array formula entered over one cell or many, a dynamic-array formula over a spill range, a
//! data-table formula) refuses the whole conversion: a record without that cell would have a
//! denominator one short, and one that called it ordinary would misreport it.

use calc_verdict::value::{CalcValue, ExcelText, NumericTargetId, WorksheetErrorCode};
use calc_verdict::{
    Acquisition, CacheCellType, CalculationContext, CaptureFailure, CaptureRecord, CaptureSession,
    CellAddress, CellObservation, ExcelEnvironment, FormulaCell, FormulaKind, Observation,
    SavedCache, SharedRange, SheetEntry, SheetVisibility,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const RESULT_FORMAT: &str = "dnacalc-capture-worker-result";
pub(crate) const RESULT_VERSION: u64 = 1;

/// What the adapter knows about a capture beside the worker's result.
pub(crate) struct Frame {
    pub case: String,
    pub source: String,
    pub tool: String,
    pub revision: u64,
    pub operations: Vec<String>,
    pub output_package: Option<String>,
    /// Failures the adapter observed itself (an Excel that had to be terminated, say).
    pub failures: Vec<CaptureFailure>,
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

/// Whether a formula's `ref` names `cell` and no other cell (`B1`, or `B1:B1`).
fn names_only(reference: &str, cell: &CellAddress) -> bool {
    let own = cell.a1();
    reference == own
        || reference
            .split_once(':')
            .is_some_and(|(first, last)| first == own && last == own)
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

/// The CaptureRecord version 2 of a worker's result, or why there is none.
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
    let mut formulas = Vec::new();
    let mut shared_ranges = Vec::new();
    let mut observations = Vec::new();
    for (i, formula) in array(result, "formulas")?.iter().enumerate() {
        let at = format!("formulas[{i}]");
        let cell = address(formula, &at)?;
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
            ("array", _)
                if formula["cm"].as_str().is_some()
                    && formula["ref"]
                        .as_str()
                        .is_some_and(|reference| names_only(reference, &cell)) =>
            {
                dynamic_array = true;
                FormulaKind::Ordinary
            }
            (other, _) => {
                return Err(format!(
                    "{} holds a formula of type {other:?} (ref {}, cell metadata {}), for which CaptureRecord version 2 has no kind; this capture is refused rather than written with that cell left out or misnamed",
                    place(&cell),
                    formula["ref"].as_str().unwrap_or("none"),
                    formula["cm"].as_str().unwrap_or("none")
                ));
            }
        };
        let mut text_formula = None;
        match cells.get(&cell) {
            None => failures.push(failure(
                "value",
                &cell,
                "the worker read no value for this formula cell".to_owned(),
            )),
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
                match raw_value(&read["value"]) {
                    Ok(value) => observations.push(CellObservation {
                        cell: cell.clone(),
                        observation: Observation::ExcelRaw(value),
                    }),
                    Err(message) => failures.push(failure("value", &cell, message)),
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

    let mut saved = Vec::new();
    for (i, entry) in array(result, "saved")?.iter().enumerate() {
        let at = format!("saved[{i}]");
        let cell = address(entry, &at)?;
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
            "format": RESULT_FORMAT, "version": 1, "excel_pid": 4242, "fatal": null,
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
                {"sheet": "S", "row": 1, "column": 2, "formula": "003d0031002f0030", "value": {"error_code": 2007}},
                {"sheet": "S", "row": 1, "column": 3, "formula": "003d00410031", "value": {"number": "3ff0000000000000"}},
                {"sheet": "S", "row": 2, "column": 3, "formula": "003d00410032", "value": {"unsupported": "System.Object[,]"}},
                {"sheet": "H", "row": 1, "column": 1, "formula": "003d00220061002200260022d83dde000022", "value": {"text": "0061d83d"}}
            ],
            "saved": [
                {"sheet": "S", "row": 1, "column": 2, "t": "e", "v": "0023004400490056002f00300021"},
                {"sheet": "S", "row": 1, "column": 3, "t": "n", "v": "0031"},
                {"sheet": "H", "row": 1, "column": 1, "t": "d", "v": "0031"}
            ],
            "reopened": [
                {"sheet": "S", "row": 1, "column": 2, "value": {"error_code": 2007}},
                {"sheet": "S", "row": 1, "column": 3, "value": {"number": "3ff0000000000001"}}
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

    #[test]
    fn array_formulas_and_fatal_results_are_refused() {
        let mut array = result();
        array["formulas"][0]["type"] = json!("array");
        let error = convert(&array, frame()).err().unwrap();
        assert!(
            error.contains("S!B1 holds a formula of type \"array\""),
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
    /// cell itself) is an ordinary formula flagged as dynamic; a legacy array formula (no cm) over
    /// one cell or several, and a dynamic array over a spill range, are refused.
    #[test]
    fn a_single_cell_dynamic_array_formula_is_ordinary_and_flagged() {
        for reference in ["B1", "B1:B1"] {
            let mut dynamic = result();
            dynamic["formulas"][0]["type"] = json!("array");
            dynamic["formulas"][0]["ref"] = json!(reference);
            dynamic["formulas"][0]["cm"] = json!("1");
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
        for (reference, cm, what) in [
            (Some("B1"), None, "a legacy array formula over its one cell"),
            (Some("B1:B2"), None, "a legacy array formula over two cells"),
            (
                Some("B1:B3"),
                Some("1"),
                "a dynamic array over a spill range",
            ),
            (Some("C1"), Some("1"), "a ref that names another cell"),
            (None, Some("1"), "no ref"),
        ] {
            let mut array = result();
            array["formulas"][0]["type"] = json!("array");
            array["formulas"][0]["ref"] = json!(reference);
            array["formulas"][0]["cm"] = json!(cm);
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
