//! Cell-context witnesses (dc-483v): a case is a small workbook layout and one formula, judged in
//! a real sheet on both sides.
//!
//! A case set (`dnacalc-cell-context/1`, under `tools/xtask-harness/cell-context/`) holds cases
//! of one generator family:
//!
//! ```json
//! {"format": "dnacalc-cell-context/1", "family": "reference-info",
//!  "layouts": {"base": {"sheets": [{"name": "Sheet1",
//!                                   "cells": {"B2": "0x3ff0000000000000", "C3": "text:abc", "D4": "=B2*2"},
//!                                   "hidden_rows": [3], "hidden_columns": []}],
//!                       "names": {"Rate": "=Sheet1!$B$2"}}},
//!  "cases": [{"id": "row-0001", "function": "ROW", "layout": "base",
//!             "at": "Sheet1!F6", "formula": "=ROW(B2:B4)", "layout_sha": "..."}]}
//! ```
//!
//! `layout_sha` is the SHA-256 of the resolved case (its layout's sheets and names, `at` and
//! `formula`, as canonical JSON); an answer is judged only against the layout Excel built.
//!
//! A cell is the witness form of a constant (`0x` and 16 hex digits for a number, `text:`,
//! `logical:true|false`, `error:Value`) or a formula (`=`). The formula goes into the cell `at`
//! as a dynamic-array formula; the cells right of and below it are empty unless the case fills
//! them (that is the spill room).
//!
//! Excel answers a case set with `tools/excel-session/cell-context-job.ps1`, which builds each
//! layout in a new workbook of an owned Excel and writes the set back with `answer` per case:
//! `{"value": <witness form>, "spill": [[...]] | null}` and its `capture_provenance`; the answers
//! go to the evidence repository's `readings/cell-context/`. DNA Calc answers a case by entering
//! the same layout into a blank workbook through `calc_host::HostSession` (the route the
//! application's grid uses: sheet commands, cell edits, hidden-axis commands, defined names and
//! a typed formula entry), calculating and projecting the formula cell and its spill area.
//!
//! `cargo xtask cells predict [--family F]` prints DNA Calc's answers; `cargo xtask cells judge
//! [--family F] [--report <json>]` compares them with every answered set and counts each case
//! as matched, mismatched or refused (a refusal is a failure).
use crate::cargo::Workspace;
use crate::legacy::legacy_root;
use calc_host::value::{CalcValue, CellKey, CoreRef, ExcelText, GridArea, SheetId};
use calc_host::{
    CalcMode, CellEdit, CellInput, FiniteNumber, HostOptions, HostSession, MemorySource,
    NameEdit, NameScope, OpenPolicy, PackageRead, ProjectedValue, ProjectionRequest, SheetChange,
    SkinIntent, WorkbookCommand, project,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

pub const FORMAT: &str = "dnacalc-cell-context/1";
const CASES: &str = "tools/xtask-harness/cell-context";
const ANSWERS: &str = "readings/cell-context";
const USAGE: &str = "cargo xtask cells predict [--family F] | cargo xtask cells judge [--family F] [--report <json>]";

/// A1 column letters and row of `text` (`B12`, `$B$12`), 1-based.
pub fn a1(text: &str) -> Option<(u32, u32)> {
    let text = text.replace('$', "");
    let letters: String = text.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let digits = &text[letters.len()..];
    if letters.is_empty() || letters.len() > 3 || digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let column = letters
        .bytes()
        .fold(0u32, |n, b| n * 26 + u32::from(b.to_ascii_uppercase() - b'A' + 1));
    let row: u32 = digits.parse().ok()?;
    (row >= 1 && row <= 1_048_576 && column <= 16_384).then_some((row, column))
}

/// `Sheet!A1` (the sheet name may be quoted) into the sheet name and the cell.
fn address(text: &str) -> Option<(String, (u32, u32))> {
    let (sheet, cell) = text.rsplit_once('!')?;
    let sheet = sheet.trim_matches('\'').replace("''", "'");
    Some((sheet, a1(cell)?))
}

fn excel_text(text: &str) -> ExcelText {
    ExcelText::from_utf16_code_units(text.encode_utf16().collect())
}

/// The witness form of a calculated value.
pub fn witness_form(value: &CalcValue) -> String {
    match value.core_ref() {
        CoreRef::Number(n) => format!("0x{:016x}", n.to_bits()),
        CoreRef::Text(t) => format!("text:{}", t.to_string_lossy()),
        CoreRef::Logical(b) => format!("logical:{}", if b { "True" } else { "False" }),
        CoreRef::Error(code) => format!("error:{code:?}"),
        CoreRef::Empty | CoreRef::Missing => "blank".to_owned(),
        _ => "fault:Unshown".to_owned(),
    }
}

fn constant(text: &str) -> Result<CellInput, String> {
    if let Some(hex) = text.strip_prefix("0x") {
        let bits = u64::from_str_radix(hex, 16).map_err(|e| format!("{text}: {e}"))?;
        return FiniteNumber::from_bits(bits)
            .map(CellInput::Number)
            .ok_or_else(|| format!("{text}: not finite"));
    }
    if let Some(rest) = text.strip_prefix("text:") {
        return calc_host::CellText::new(excel_text(rest))
            .map(CellInput::Text)
            .ok_or_else(|| format!("{text}: not cell text"));
    }
    match text {
        "logical:true" | "logical:True" => return Ok(CellInput::Logical(true)),
        "logical:false" | "logical:False" => return Ok(CellInput::Logical(false)),
        _ => {}
    }
    if let Some(name) = text.strip_prefix("error:") {
        use calc_host::value::WorksheetErrorCode as E;
        let code = match name {
            "Null" => E::Null,
            "Div0" => E::Div0,
            "Value" => E::Value,
            "Ref" => E::Ref,
            "Name" => E::Name,
            "Num" => E::Num,
            "NA" => E::NA,
            _ => return Err(format!("{text}: unknown error")),
        };
        return Ok(CellInput::Error(code));
    }
    Err(format!("{text}: not a witness-form constant"))
}

/// The error formula Excel's side enters for an `error:` constant.
fn error_formula(name: &str) -> Option<&'static str> {
    Some(match name {
        "Null" => "=#NULL!",
        "Div0" => "=#DIV/0!",
        "Value" => "=#VALUE!",
        "Ref" => "=#REF!",
        "Name" => "=#NAME?",
        "Num" => "=#NUM!",
        "NA" => "=#N/A",
        _ => return None,
    })
}

/// A case with its layout's sheets and names in place (a case may also carry them itself).
pub fn resolved(set: &Value, case: &Value) -> Value {
    let mut out = case.clone();
    if let Some(key) = case["layout"].as_str()
        && let Some(layout) = set["layouts"].get(key)
    {
        out["sheets"] = layout["sheets"].clone();
        out["names"] = layout["names"].clone();
    }
    out
}

/// DNA Calc's answer for one case: `{"value", "spill"}`, or an error naming the layout step that
/// the host refused.
pub fn evaluate(
    case: &Value,
    locale: Option<calc_host::FormatProfile>,
    clock: Option<f64>,
) -> Result<Value, String> {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(calc_host::blank_package()));
    let mut session = HostSession::open(
        source,
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Automatic),
            locale,
            ..HostOptions::default()
        },
    )
    .map_err(|e| format!("open: {e:?}"))?;
    session.set_clock(clock);
    let sheets = case["sheets"].as_array().ok_or("case: sheets")?;
    let mut ids: BTreeMap<String, SheetId> = BTreeMap::new();
    for (i, sheet) in sheets.iter().enumerate() {
        let name = sheet["name"].as_str().ok_or("sheet: name")?.to_owned();
        let receipt = if i == 0 {
            let first = session.host().sheets()[0].id();
            session.dispatch(SkinIntent::Command(WorkbookCommand::Sheet {
                sheet: Some(first),
                change: SheetChange::Rename { name: name.clone() },
            }))
        } else {
            session.dispatch(SkinIntent::Command(WorkbookCommand::Sheet {
                sheet: None,
                change: SheetChange::Add { name: name.clone() },
            }))
        };
        if let Some(error) = receipt.error {
            return Err(format!("sheet {name}: {error}"));
        }
        let id = session
            .host()
            .sheets()
            .iter()
            .find(|s| s.name().text().to_string_lossy().eq_ignore_ascii_case(&name))
            .map(|s| s.id())
            .ok_or_else(|| format!("sheet {name}: not found after entry"))?;
        ids.insert(name.to_ascii_uppercase(), id);
    }
    let sheet_id = |name: &str| ids.get(&name.to_ascii_uppercase()).copied();
    // Constants first in one edit, then formulas typed in cell order. Text constants are in
    // text-formatted cells, as Excel's side writes them (NumberFormat "@" before Value2).
    let mut text_cells = Vec::new();
    let mut edits = Vec::new();
    let mut formulas = Vec::new();
    for sheet in sheets {
        let id = sheet_id(sheet["name"].as_str().unwrap_or_default()).ok_or("sheet id")?;
        if let Some(cells) = sheet["cells"].as_object() {
            for (at, content) in cells {
                let (row, column) = a1(at).ok_or_else(|| format!("cell {at}"))?;
                let key = CellKey::new(id, row, column).map_err(|e| format!("cell {at}: {e:?}"))?;
                let content = content.as_str().ok_or_else(|| format!("cell {at}: not a string"))?;
                // Excel's side enters an error constant as its error formula (`=#N/A`), as the
                // Touchstone worker and cell-context-job.ps1 do: the cell holds a formula.
                if let Some(spelling) = content.strip_prefix("error:").and_then(error_formula) {
                    formulas.push((key, spelling.to_owned()));
                } else if content.starts_with('=') {
                    formulas.push((key, content.to_owned()));
                } else {
                    if content.starts_with("text:") {
                        text_cells.push(GridArea::cell(key));
                    }
                    edits.push(CellEdit::Set { at: key, input: constant(content)? });
                }
            }
        }
    }
    if !text_cells.is_empty() {
        let receipt = session.dispatch(SkinIntent::Command(WorkbookCommand::FormatCells {
            areas: text_cells,
            patch: calc_host::FormatPatch {
                number_format: Some("@".to_owned()),
                ..calc_host::FormatPatch::default()
            },
        }));
        if let Some(error) = receipt.error {
            return Err(format!("text formats: {error}"));
        }
    }
    if !edits.is_empty() {
        let receipt = session.dispatch(SkinIntent::Edit { edits });
        if let Some(error) = receipt.error {
            return Err(format!("constants: {error}"));
        }
    }
    for (name, formula) in case["names"].as_object().into_iter().flatten() {
        let receipt = session.dispatch(SkinIntent::Command(WorkbookCommand::Name(NameEdit {
            id: None,
            version: None,
            scope: NameScope::Workbook,
            name: name.clone(),
            formula: formula.as_str().map(str::to_owned),
        })));
        if let Some(error) = receipt.error {
            return Err(format!("name {name}: {error}"));
        }
    }
    for (key, formula) in formulas {
        let receipt = session.dispatch(SkinIntent::EnterCell { at: key, text: formula.clone() });
        if let Some(error) = receipt.error {
            return Err(format!("formula {formula}: {error}"));
        }
    }
    for sheet in sheets {
        let id = sheet_id(sheet["name"].as_str().unwrap_or_default()).ok_or("sheet id")?;
        for (field, rows) in [("hidden_rows", true), ("hidden_columns", false)] {
            for index in sheet[field].as_array().into_iter().flatten() {
                let first = index.as_u64().ok_or_else(|| format!("{field}: not a number"))? as u32;
                let receipt = session.dispatch(SkinIntent::Command(WorkbookCommand::AxisFormat {
                    sheet: id,
                    rows,
                    first,
                    count: 1,
                    size_bits: None,
                    hidden: Some(true),
                }));
                if let Some(error) = receipt.error {
                    return Err(format!("{field} {first}: {error}"));
                }
            }
        }
    }
    let (sheet, (row, column)) =
        address(case["at"].as_str().ok_or("case: at")?).ok_or("case: at is not Sheet!A1")?;
    let at = CellKey::new(sheet_id(&sheet).ok_or("case: at names no sheet")?, row, column)
        .map_err(|e| format!("at: {e:?}"))?;
    let formula = case["formula"].as_str().ok_or("case: formula")?;
    let receipt = session.dispatch(SkinIntent::EnterCell { at, text: formula.to_owned() });
    if receipt.error.is_some() {
        // Excel refuses the same entries with its "There's a problem with this formula"
        // message (the Excel job records a refused Formula2 assignment the same way).
        return Ok(json!({"value": "entry:refused", "spill": null}));
    }
    let host = session.host();
    let shown = |key: CellKey| -> Result<String, String> {
        let page = project(host, &ProjectionRequest { cells: vec![key] }).map_err(|e| format!("project: {e:?}"))?;
        let cell = page.cells().first().ok_or("project: no cell")?;
        Ok(match cell.value() {
            ProjectedValue::Calculated { value, .. } | ProjectedValue::Literal(value) => witness_form(value),
            ProjectedValue::Empty => "blank".to_owned(),
            // CalcFml's entry check claims Excel refuses this formula at entry: scored as Excel's
            // refused entry (CompileError::Rejected).
            ProjectedValue::NotModeled {
                refusal: calc_host::NotModeled::Compile(calc_fml::CompileError::Rejected { .. }),
                ..
            } => "entry:refused".to_owned(),
            ProjectedValue::NotModeled { refusal, .. } => format!("fault:NotModeled:{refusal:?}"),
            other => format!("fault:{other:?}"),
        })
    };
    let value = shown(at)?;
    let spill = match host.spill_area(at) {
        Some(area) if area.first() != area.last() => {
            let (first, last) = (area.first(), area.last());
            let mut rows = Vec::new();
            for r in first.row()..=last.row() {
                let mut cells = Vec::new();
                for c in first.column()..=last.column() {
                    cells.push(shown(CellKey::new(at.sheet(), r, c).map_err(|e| format!("{e:?}"))?)?);
                }
                rows.push(cells);
            }
            Value::from(rows)
        }
        _ => Value::Null,
    };
    Ok(json!({"value": value, "spill": spill}))
}

/// The witness form of one bank argument cell (`0x` hex, text, logical, blank, `{"error"}`), or
/// `None` for a blank cell.
fn argument_cell(arg: &Value) -> Result<Option<String>, String> {
    Ok(match arg {
        Value::Null => None,
        Value::Bool(b) => Some(format!("logical:{b}")),
        Value::String(text) if text.len() == 18 && text.starts_with("0x") => Some(text.clone()),
        Value::String(text) => Some(format!("text:{text}")),
        Value::Object(error) if error.len() == 1 => {
            let code = error["error"].as_str().ok_or("argument: unknown object")?;
            // An error argument is entered as its error formula (`=#N/A`), as the worker does.
            let spelling = match code {
                "Null" => "#NULL!",
                "Div0" => "#DIV/0!",
                "Value" => "#VALUE!",
                "Ref" => "#REF!",
                "Name" => "#NAME?",
                "Num" => "#NUM!",
                "NA" => "#N/A",
                _ => return Err(format!("argument: unknown error {code}")),
            };
            Some(format!("={spelling}"))
        }
        other => return Err(format!("argument: not a scalar cell: {other}")),
    })
}

fn column_letters(mut column: u32) -> String {
    let mut out = Vec::new();
    while column > 0 {
        column -= 1;
        out.push(b'A' + (column % 26) as u8);
        column /= 26;
    }
    out.reverse();
    String::from_utf8(out).expect("ASCII")
}

/// DNA Calc's answer for a bank call that Excel answered as a one-call serial request
/// (`touchstone/1-serial-single`, dc-ymjf.2.85): the scratch book's sheets S, A and Q, the
/// arguments in S!B1 onwards (an array argument one cell per element, row by row, read back
/// through `CHOOSE`), and `=F(B1,C1,...)` in S!A1, wrapped in `INDEX(..., r, c)` for a selected
/// result, as Program.cs `EvaluateSerial` builds it. The value of S!A1 in witness form.
pub fn serial_single(
    function: &str,
    args: &[Value],
    index: Option<(usize, usize)>,
    context: Option<&calc_verdict::WitnessInvocation>,
) -> String {
    // The attested culture with the recorded separators; without one no profile is assumed.
    let locale = context.and_then(|c| {
        let profile = calc_host::FormatProfile::for_culture(c.locale.as_deref()?)?;
        profile.with_separators(c.decimal_separator.as_deref()?, c.thousands_separator.as_deref()?)
    });
    let built = (|| -> Result<Value, String> {
        let mut cells = serde_json::Map::new();
        let mut refs = Vec::new();
        let mut column = 2u32;
        let mut place = |arg: &Value, cells: &mut serde_json::Map<String, Value>| -> Result<String, String> {
            let at = format!("{}1", column_letters(column));
            column += 1;
            if let Some(content) = argument_cell(arg)? {
                cells.insert(at.clone(), Value::String(content));
            }
            Ok(at)
        };
        for arg in args {
            match arg {
                Value::Array(rows) => {
                    let rows: Vec<&Vec<Value>> = if rows.iter().all(Value::is_array) {
                        rows.iter().filter_map(Value::as_array).collect()
                    } else {
                        vec![rows]
                    };
                    let mut indices = Vec::new();
                    let mut sources = Vec::new();
                    let mut k = 0;
                    for row in &rows {
                        let mut line = Vec::new();
                        for cell in *row {
                            k += 1;
                            line.push(k.to_string());
                            sources.push(place(cell, &mut cells)?);
                        }
                        indices.push(line.join(","));
                    }
                    refs.push(format!("CHOOSE({{{}}},{})", indices.join(";"), sources.join(",")));
                }
                Value::Object(range) if range.contains_key("range") => {
                    return Err("a range argument has no serial-single layout".to_owned());
                }
                scalar => refs.push(place(scalar, &mut cells)?),
            }
        }
        let mut formula = format!("{function}({})", refs.join(","));
        if let Some((r, c)) = index {
            formula = format!("INDEX({formula},{r},{c})");
        }
        let case = json!({
            "sheets": [{"name": "S", "cells": cells}, {"name": "A"}, {"name": "Q"}],
            "names": {},
            "at": "S!A1",
            "formula": format!("={formula}"),
        });
        evaluate(&case, locale, None)
    })();
    match built {
        Ok(answer) => match answer["value"].as_str() {
            // The worker reads Value2: a formula showing an empty cell reads as 0.
            Some("blank") => "0x0000000000000000".to_owned(),
            Some(value) if value.starts_with("fault:") => {
                let end = value[6..].find(':').map_or(value.len(), |i| i + 6);
                value[..end].to_owned()
            }
            Some(value) => value.to_owned(),
            None => "fault:Unshown".to_owned(),
        },
        Err(_) => "fault:WitnessLayout".to_owned(),
    }
}

/// The locale a set's answers record (`capture_provenance.calculation_settings`): the user
/// culture with Excel's International separators. Without them no profile is assumed.
fn recorded_locale(set: &Value) -> Option<calc_host::FormatProfile> {
    let settings = &set["capture_provenance"]["calculation_settings"];
    let culture = settings["user_culture"].as_str()?;
    let decimal = settings["international_decimal_separator"].as_str()?;
    let thousands = settings["international_thousands_separator"].as_str()?;
    calc_host::FormatProfile::for_culture(culture)?.with_separators(decimal, thousands)
}

/// The calculation clock a set's answers record: Excel's NOW() before and after its cases,
/// used when both fall on one day (a serial date and time, as binary64 bits in hex).
fn recorded_clock(set: &Value) -> Option<f64> {
    let settings = &set["capture_provenance"]["calculation_settings"];
    let read = |key: &str| {
        settings[key]
            .as_str()
            .and_then(|hex| u64::from_str_radix(hex, 16).ok())
            .map(f64::from_bits)
            .filter(|v| v.is_finite())
    };
    let (before, after) = (read("calculation_clock")?, read("calculation_clock_after")?);
    (before.floor() == after.floor()).then_some(before)
}

fn read_sets(dir: &Path, family: Option<&str>) -> Result<Vec<(PathBuf, Value)>, String> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(out);
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    paths.sort();
    for path in paths {
        let doc: Value = serde_json::from_slice(&std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if doc["format"] != FORMAT {
            continue;
        }
        if family.is_some_and(|f| doc["family"] != f) {
            continue;
        }
        out.push((path, doc));
    }
    Ok(out)
}

fn parse(args: &[String]) -> Result<(String, Option<String>, Option<PathBuf>), String> {
    let mut it = args.iter();
    let verb = it.next().ok_or("a verb is required")?.clone();
    let (mut family, mut report) = (None, None);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--family" => family = Some(it.next().ok_or("--family needs a value")?.clone()),
            "--report" => report = Some(PathBuf::from(it.next().ok_or("--report needs a path")?)),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if !matches!(verb.as_str(), "predict" | "judge") {
        return Err(format!("unknown verb {verb}"));
    }
    Ok((verb, family, report))
}

pub fn run(args: &[String]) -> ExitCode {
    match run_inner(args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("xtask cells: {e}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn run_inner(args: &[String]) -> Result<ExitCode, String> {
    let (verb, family, report) = parse(args)?;
    let ws = Workspace::load(&std::env::current_dir().map_err(|e| e.to_string())?)?;
    if verb == "predict" {
        // The en-ZA profile of this repository's Excel host (dc-7ofm); a prediction is not a verdict.
        let locale = calc_host::FormatProfile::for_culture("en-ZA").and_then(|p| p.with_separators(".", " "));
        for (path, set) in read_sets(&ws.root.join(CASES), family.as_deref())? {
            for case in set["cases"].as_array().into_iter().flatten() {
                let answer = evaluate(&resolved(&set, case), locale, None).unwrap_or_else(|e| json!({"value": format!("fault:Layout:{e}"), "spill": null}));
                println!("{}\t{}\t{}\t{}", path.file_name().unwrap().to_string_lossy(), case["id"].as_str().unwrap_or("?"), case["formula"].as_str().unwrap_or("?"), answer);
            }
        }
        return Ok(ExitCode::SUCCESS);
    }
    let answers = legacy_root(&ws.root).path.join(crate::evidence::CHECKOUT).join(ANSWERS);
    let cases = read_sets(&ws.root.join(CASES), family.as_deref())?;
    let answered = read_sets(&answers, family.as_deref())?;
    // function -> (cases, matched, mismatched, refused, awaiting answer)
    let mut counts: BTreeMap<String, [usize; 5]> = BTreeMap::new();
    let mut differences = Vec::new();
    let mut by_id: BTreeMap<String, (Value, Option<calc_host::FormatProfile>, Option<f64>)> = BTreeMap::new();
    for (_, set) in &answered {
        let locale = recorded_locale(set);
        let clock = recorded_clock(set);
        for case in set["cases"].as_array().into_iter().flatten() {
            if let Some(id) = case["id"].as_str() {
                by_id.insert(format!("{}/{id}", set["family"].as_str().unwrap_or("")), (resolved(set, case), locale, clock));
            }
        }
    }
    for (_, set) in &cases {
        for case in set["cases"].as_array().into_iter().flatten() {
            let function = case["function"].as_str().unwrap_or("?").to_owned();
            let entry = counts.entry(function.clone()).or_default();
            entry[0] += 1;
            let key = format!("{}/{}", set["family"].as_str().unwrap_or(""), case["id"].as_str().unwrap_or(""));
            let Some((excel, locale, clock)) = by_id.get(&key) else {
                entry[4] += 1;
                continue;
            };
            // The answered case is judged as Excel saw it: its own layout, as recorded.
            if excel["layout_sha"] != case["layout_sha"] {
                entry[4] += 1;
                continue;
            }
            // Excel could not hold the layout as written: a failure that no calculation fixes.
            if excel["answer"]["value"].as_str().is_some_and(|v| v.starts_with("unread:")) {
                entry[3] += 1;
                differences.push(json!({"id": key, "function": function, "excel": excel["answer"], "dna_calc": null}));
                continue;
            }
            let ours = evaluate(excel, *locale, *clock).unwrap_or_else(|e| json!({"value": format!("fault:Layout:{e}"), "spill": null}));
            let theirs = &excel["answer"];
            if ours == *theirs {
                entry[1] += 1;
            } else {
                let refused = ours["value"].as_str().is_some_and(|v| v.starts_with("fault:"));
                entry[if refused { 3 } else { 2 }] += 1;
                differences.push(json!({"id": key, "function": function, "formula": excel["formula"], "at": excel["at"], "excel": theirs, "dna_calc": ours}));
            }
        }
    }
    let mut totals = [0usize; 5];
    for (function, c) in &counts {
        println!("xtask cells: {function:<14} {:>6} cases {:>6} matched {:>5} mismatched {:>5} refused {:>6} awaiting an Excel answer", c[0], c[1], c[2], c[3], c[4]);
        for (t, v) in totals.iter_mut().zip(c) {
            *t += v;
        }
    }
    println!(
        "xtask cells: {} of {} cases matched; failed {} (mismatched {}, refused {}, awaiting an Excel answer {})",
        totals[1], totals[0], totals[2] + totals[3] + totals[4], totals[2], totals[3], totals[4]
    );
    if let Some(path) = report {
        let functions: Vec<Value> = counts.iter().map(|(f, c)| json!({"function": f, "cases": c[0], "matched": c[1], "mismatched": c[2], "refused": c[3], "awaiting": c[4]})).collect();
        let doc = json!({"format": "dnacalc-cell-context-report/1", "functions": functions, "differences": differences});
        std::fs::write(&path, serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())? + "\n").map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a1_addresses_read_as_excel_writes_them() {
        assert_eq!(a1("A1"), Some((1, 1)));
        assert_eq!(a1("$AB$12"), Some((12, 28)));
        assert_eq!(a1("XFD1048576"), Some((1_048_576, 16_384)));
        assert_eq!(a1("XFE1"), None);
        assert_eq!(address("'My Sheet'!C3"), Some(("My Sheet".to_owned(), (3, 3))));
    }

    #[test]
    fn a_layout_is_entered_and_calculated_through_the_host_session() {
        let case = json!({
            "id": "t", "function": "ROW",
            "sheets": [{"name": "Data", "cells": {"B2": "0x4000000000000000", "B3": "=B2*3"}, "hidden_rows": [3]},
                       {"name": "Other", "cells": {}}],
            "at": "Data!D4", "formula": "=ROW(B2:B3)"
        });
        let answer = evaluate(&case, None, None).unwrap();
        assert_eq!(answer["value"], format!("0x{:016x}", 2f64.to_bits()));
        assert_eq!(answer["spill"], json!([[format!("0x{:016x}", 2f64.to_bits())], [format!("0x{:016x}", 3f64.to_bits())]]));
        let sheet = json!({"id": "s", "function": "SHEET", "sheets": [{"name": "A", "cells": {}}, {"name": "B", "cells": {}}],
            "at": "B!A1", "formula": "=SHEET()"});
        assert_eq!(evaluate(&sheet, None, None).unwrap()["value"], format!("0x{:016x}", 2f64.to_bits()));
        let text = json!({"id": "f", "function": "FORMULATEXT", "sheets": [{"name": "S", "cells": {"A1": "=1+2"}}],
            "at": "S!C1", "formula": "=FORMULATEXT(A1)"});
        assert_eq!(evaluate(&text, None, None).unwrap()["value"], "text:=1+2");
    }

    #[test]
    fn a_serial_single_call_sits_in_s_a1_with_its_arguments_from_b1() {
        let one = format!("0x{:016x}", 1f64.to_bits());
        assert_eq!(serial_single("ROW", &[json!(" abc ")], None, None), one);
        assert_eq!(serial_single("COLUMN", &[json!(true)], None, None), format!("0x{:016x}", 2f64.to_bits()));
        // OFFSET(B1, 1, 0) reads the blank B2 below the arguments; Value2 shows it as 0.
        assert_eq!(serial_single("OFFSET", &[json!("x"), json!(one), json!("0x0000000000000000")], None, None), "0x0000000000000000");
        assert_eq!(serial_single("SUM", &[json!([["0x4000000000000000", "0x4008000000000000"]])], None, None),
                   format!("0x{:016x}", 5f64.to_bits()));
        assert_eq!(serial_single("ROW", &[json!({"error": "NA"})], Some((1, 1)), None), one);
    }

    /// Native-only: exercises the application's HostSession and workbook entry route.
    #[test]
    fn ets_text_options_are_read_before_the_unequal_history_shape() {
        let locale = calc_host::FormatProfile::for_culture("en-ZA").and_then(|p| p.with_separators(".", " "));
        // Excel 16.0.20527 read these options from cells in a serial single-call layout.
        for (input, expected) in [("bad", "error:Value"), ("1,234", "error:Value"), ("1", "error:NA"), ("25%", "error:NA")] {
            for name in ["FORECAST.ETS.STAT", "FORECAST.ETS.SEASONALITY"] {
                let formula = format!("={name}(3,{{1;2}},{{1;2}},B1)");
                let case = json!({"sheets": [{"name": "S", "cells": {"B1": format!("text:{input}")}}],
                    "names": {}, "at": "S!A1", "formula": formula});
                assert_eq!(evaluate(&case, locale, None).unwrap()["value"], expected, "{name}, option {input:?}");
            }
        }
        // Excel reads this as a date in the current year. With no recorded clock,
        // the application must not claim to have decided the shape result.
        let case = json!({"sheets": [{"name": "S", "cells": {"B1": "text:1/2"}}], "names": {},
            "at": "S!A1", "formula": "=FORECAST.ETS.STAT(3,{1;2},{1;2},B1)"});
        assert!(evaluate(&case, locale, None).unwrap()["value"].as_str().unwrap().starts_with("fault:NotModeled"));
    }
}
