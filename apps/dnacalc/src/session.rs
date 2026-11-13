//! Opening a workbook into the one session, the macro level, addresses and the JSON of values.

use crate::{Cli, Failure, fail};
use calc_host::value::{CalcValue, CellKey, CoreValue};
use calc_host::{
    CalcMode, CalcScope, EvalSelection, HostOptions, HostProgress, HostSession, MemorySource,
    OpenPolicy, ProjectedCell, ProjectedValue, ProjectionRequest, SavePolicy, SkinIntent,
};
use calc_macro_security::VbaPolicy;
use serde_json::{Value, json};
use std::rc::Rc;
use std::sync::Arc;

/// The most steps one VBA call may take.
pub(crate) const MAX_VBA_STEPS: u64 = 50_000_000;

/// An opened workbook, and what the macro level did with its VBA project.
pub(crate) struct Book {
    pub(crate) session: HostSession,
    pub(crate) macros: Value,
    pub(crate) project: Option<Rc<calc_host_vba::WorkbookEvents>>,
}

pub(crate) fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_owned())
}

/// Opens `cli.book`; at a level above `off` runs the file's VBA project as it opens (its
/// `Workbook_Open`, and its functions published to the sheets), under that level's policy.
pub(crate) fn open(cli: &Cli) -> Result<Book, Failure> {
    let bytes =
        std::fs::read(&cli.book).map_err(|e| fail(format!("cannot read {}: {e}", cli.book)))?;
    let name = file_name(&cli.book);
    let source = Arc::new(MemorySource::new(bytes).named(name.as_str()));
    let mut session = HostSession::open(
        source,
        &OpenPolicy::default(),
        HostOptions {
            // Excel recalculates a file whatever mode it declares when asked to; the runner
            // calculates under the automatic mode as the corpus oracle does.
            calc_mode: Some(CalcMode::Automatic),
            // A stated policy of the runner, as the corpus oracle's DNA-only runs state it: not
            // Excel's own regional settings, which a file does not record.
            locale: Some(calc_host::FormatProfile::for_culture(&cli.locale).ok_or_else(|| {
                fail(format!("--locale {:?} is not a culture this build knows", cli.locale))
            })?),
            ..HostOptions::default()
        },
    )
    .map_err(|e| fail(format!("{}: the package was refused: {e}", cli.book)))?;
    session.set_clock(Some(clock_now()));
    let has_project = session
        .host()
        .loaded()
        .vba_project(&Default::default())
        .map(|p| p.is_some())
        .unwrap_or(false);
    let (macros, project) = match (cli.level.vba_policy(), has_project) {
        (_, false) => (json!({"level": cli.level.key(), "project": false}), None),
        (None, true) => (
            json!({"level": cli.level.key(), "project": true, "ran": false,
                   "reason": "the macro level is off: the file's VBA project does not run"}),
            None,
        ),
        (Some(policy), true) => {
            let by = format!("dnacalc --macro-level {}", cli.level.key());
            let grant = calc_host_vba::ProjectGrant::trust_all(&by, policy);
            match calc_host_vba::open_file_project(
                &grant,
                &mut session,
                &name,
                MAX_VBA_STEPS,
                None,
            ) {
                Ok(project) => (
                    json!({"level": cli.level.key(), "project": true, "ran": true,
                           "policy": match policy { VbaPolicy::Safe => "safe", VbaPolicy::Full => "full" }}),
                    project,
                ),
                Err(error) => (
                    json!({"level": cli.level.key(), "project": true, "ran": false,
                           "reason": error.to_string()}),
                    None,
                ),
            }
        }
    };
    Ok(Book {
        session,
        macros,
        project,
    })
}

/// Runs the calculation of the whole workbook to its end (`ForceAll`: every formula, as Excel's
/// full calculation does).
pub(crate) fn recalculate(session: &mut HostSession) -> Result<(), Failure> {
    // An edit leaves its own calculation to finish first.
    drain(session)?;
    let receipt = session.dispatch(SkinIntent::Calculate {
        scope: CalcScope::Workbook,
        selection: EvalSelection::ForceAll,
    });
    if !receipt.accepted() {
        return Err(fail(format!("the recalculation was refused: {receipt:?}")));
    }
    drain(session)
}

pub(crate) fn drain(session: &mut HostSession) -> Result<(), Failure> {
    while session.has_pending_calculation() {
        match session.advance_calculation() {
            HostProgress::Failed(failure) => {
                return Err(fail(format!("the recalculation failed: {failure}")));
            }
            HostProgress::Waiting { .. } => {
                return Err(fail(
                    "the recalculation waits for an external provider this runner does not have",
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

/// The workbook saved as `.xlsx`/`.xlsm` bytes.
pub(crate) fn save(session: &mut HostSession, path: &str) -> Result<usize, Failure> {
    let mut bytes: Vec<u8> = Vec::new();
    session
        .save(&SavePolicy::default(), &mut bytes)
        .map_err(|e| fail(format!("the save was refused: {e}")))?;
    std::fs::write(path, &bytes).map_err(|e| fail(format!("cannot write {path}: {e}")))?;
    Ok(bytes.len())
}

fn sheet_name(session: &HostSession, cell: CellKey) -> String {
    session
        .host()
        .sheets()
        .iter()
        .find(|s| s.id() == cell.sheet())
        .map(|s| s.name().text().to_string_lossy())
        .unwrap_or_default()
}

pub(crate) fn a1(session: &HostSession, cell: CellKey) -> String {
    let mut column = cell.column();
    let mut letters = Vec::new();
    while column > 0 {
        column -= 1;
        letters.push(b'A' + (column % 26) as u8);
        column /= 26;
    }
    letters.reverse();
    format!(
        "{}!{}{}",
        quote_sheet(&sheet_name(session, cell)),
        String::from_utf8_lossy(&letters),
        cell.row()
    )
}

fn quote_sheet(name: &str) -> String {
    if name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_')
        && !name.is_empty()
    {
        name.to_owned()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// `A1`, `Sheet1!A1` or `'My Sheet'!A1` (the first sheet when unqualified).
pub(crate) fn parse_cell(session: &HostSession, text: &str) -> Result<CellKey, Failure> {
    let (sheet_text, cell_text) = match text.rsplit_once('!') {
        Some((sheet, cell)) => (Some(sheet), cell),
        None => (None, text),
    };
    let sheets = session.host().sheets();
    let sheet = match sheet_text {
        None => sheets.first().ok_or_else(|| fail("the workbook has no sheet"))?,
        Some(raw) => {
            let name = match raw.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
                Some(inner) => inner.replace("''", "'"),
                None => raw.to_owned(),
            };
            sheets
                .iter()
                .find(|s| {
                    s.name()
                        .text()
                        .to_string_lossy()
                        .eq_ignore_ascii_case(&name)
                })
                .ok_or_else(|| fail(format!("no sheet {name:?} in the workbook")))?
        }
    };
    let bad = || fail(format!("{text:?} is not a cell address"));
    let cell_text = cell_text.replace('$', "");
    let split = cell_text
        .find(|c: char| c.is_ascii_digit())
        .ok_or_else(bad)?;
    let (letters, digits) = cell_text.split_at(split);
    if letters.is_empty() || !letters.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(bad());
    }
    let column = letters
        .to_ascii_uppercase()
        .bytes()
        .try_fold(0u32, |acc, b| {
            acc.checked_mul(26)?.checked_add(u32::from(b - b'A') + 1)
        })
        .ok_or_else(bad)?;
    let row: u32 = digits.parse().map_err(|_| bad())?;
    CellKey::new(sheet.id(), row, column).map_err(|_| bad())
}

pub(crate) fn project(session: &HostSession, cells: Vec<CellKey>) -> Vec<ProjectedCell> {
    calc_host::project(session.host(), &ProjectionRequest { cells })
        .map(|page| page.cells().to_vec())
        .unwrap_or_default()
}

pub(crate) fn value_json(value: &CalcValue) -> Value {
    match value.core() {
        CoreValue::Number(n) if n.is_finite() => json!({"type": "number", "value": n}),
        CoreValue::Number(n) => json!({"type": "number", "value": format!("{n:?}")}),
        CoreValue::Text(t) => json!({"type": "text", "value": t.to_string_lossy()}),
        CoreValue::Logical(b) => json!({"type": "logical", "value": b}),
        CoreValue::Error(code) => json!({"type": "error", "value": format!("{code:?}")}),
        CoreValue::Empty | CoreValue::Missing => json!({"type": "empty"}),
        CoreValue::Array(array) => {
            json!({"type": "array", "rows": array.shape().rows, "columns": array.shape().cols})
        }
        CoreValue::Reference(_) => json!({"type": "reference"}),
    }
}

/// How a projected cell's value came to be, and the value if it has one.
pub(crate) fn provenance(value: &ProjectedValue) -> (&'static str, Option<CalcValue>, Option<String>) {
    match value {
        ProjectedValue::Empty => ("empty", None, None),
        ProjectedValue::Literal(v) => ("literal", Some(v.clone()), None),
        ProjectedValue::FileCached(cached) => {
            ("file-cached", Some(cached.value().to_value()), None)
        }
        ProjectedValue::Calculated { value, .. } => ("calculated", Some(value.clone()), None),
        ProjectedValue::NotModeled { refusal, .. } => {
            ("not-modelled", None, Some(refusal.to_string()))
        }
        ProjectedValue::Stale(stale) => ("stale", None, Some(format!("{stale:?}"))),
    }
}

pub(crate) fn cell_json(session: &HostSession, cell: &ProjectedCell) -> Value {
    let (kind, value, note) = provenance(cell.value());
    let formula = match cell.input() {
        Some(calc_host::CellInput::Formula(f)) => Some(f.text().to_string_lossy()),
        _ => None,
    };
    let mut out = json!({"cell": a1(session, cell.at()), "provenance": kind});
    if let Some(formula) = formula {
        out["formula"] = json!(formula);
    }
    if let Some(value) = value {
        out["value"] = value_json(&value);
    }
    if let Some(note) = note {
        out["note"] = json!(note);
    }
    out
}

/// This machine's clock (UTC) as a 1900-system serial date and time: what NOW and TODAY read.
pub(crate) fn clock_now() -> f64 {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    seconds / 86_400.0 + 25_569.0
}
