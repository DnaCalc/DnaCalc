//! `cargo xtask fidelity <dir> [--out <dir>] [--timeout <s>]`: document fidelity over a folder of
//! workbooks (bead `dc-1yp`): whether calc-doc opens each, saves it unchanged and as edited, and
//! reads back what it wrote.
//!
//! 1. **Workbooks.** Every `.xlsx`, `.xlsm` and `.xlsb` file directly in `<dir>` (any case of the
//!    extension, not an Excel lock file `~$...`), in name order.
//! 2. **Each workbook in a process of its own** (`fidelity --one <file> --out <dir>`, this
//!    executable), so a workbook that crashes or hangs the reader is a counted failure and the run
//!    carries on; one that runs past `--timeout` seconds (default 600) is stopped and counted.
//! 3. **What one workbook is put through**, through calc-host as DNA Calc opens and saves:
//!    - *open*: `HostState::open`; a refusal is recorded with its typed reason;
//!    - *unchanged save*: the opened revision saved and opened again: whether the bytes are the
//!      opened package's and whether the reopened model equals the opened one;
//!    - *edited save*: in a session, the workbook recalculated, one value and one formula reading
//!      it entered right of the first visible sheet's used area (`12345.5` and `=<that cell>*2`),
//!      recalculated, saved and opened again: whether the reopened model equals the session's
//!      (cells, sheets, names, arrays, tables, data tables, hidden rows, unloaded cells, settings,
//!      layout), whether each sheet's styled cells and row and column geometry are the opened
//!      ones (but the two cells entered), and how many formula cells are saved with the value the
//!      session calculated as their cached value.
//!
//!    The two saved packages are written to `<out>/packages/` for an Excel run that opens them
//!    without repair.
//! 4. **The summary.** `<out>/summary.json` and `<out>/summary.md`: the counts, refusals by reason
//!    and differences by kind; each workbook's record is `<out>/workbooks/<name>.json`.
//!
//! `--twins` (bead `dc-5sr`) takes only the `.xlsb` workbooks of `<dir>` and, besides the above,
//! opens each one's twin, the `.xlsx` or `.xlsm` of the same stem that Excel saved from the same
//! workbook, and compares the two opened models (cells, sheets, names, arrays, tables, data tables,
//! hidden rows, unloaded cells, settings, cell formats, layout, each sheet's styled cells and
//! geometry, the date system and the calculation mode). The SpreadsheetML package each `.xlsb` is
//! read as is written to `<out>/converted/`.
//!
//! `--out` defaults to `target/fidelity`. A summary of workbooks outside the repository belongs
//! outside it too: the per-workbook records quote cells and names; the counts alone may be
//! committed.
//!
//! Exit codes: 0 when the summary was written, whatever it says; 1 when it cannot be written; 2 on
//! a usage error.

use calc_host::value::{CellKey, MAX_COLUMNS, SheetId};
use calc_host::{
    HostOptions, HostSession, HostState, MemorySource, ModelRoot, OpenPolicy, PackageRead,
    ProjectedValue, ProjectionRequest, SavePolicy, SkinIntent, project,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const USAGE: &str = "usage: cargo xtask fidelity <dir> [--out <dir>] [--timeout <s>] [--twins]";

struct Args {
    dir: PathBuf,
    out: PathBuf,
    timeout: Duration,
    one: Option<PathBuf>,
    twins: bool,
}

fn parse(raw: &[String]) -> Result<Args, String> {
    let mut dir = None;
    let mut out = PathBuf::from("target").join("fidelity");
    let mut timeout = Duration::from_secs(600);
    let mut one = None;
    let mut twins = false;
    let mut rest = raw.iter();
    while let Some(arg) = rest.next() {
        let mut value = |name: &str| {
            rest.next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match arg.as_str() {
            "--out" => out = PathBuf::from(value("--out")?),
            "--timeout" => {
                timeout = Duration::from_secs(
                    value("--timeout")?
                        .parse()
                        .map_err(|_| "--timeout needs whole seconds".to_owned())?,
                )
            }
            "--one" => one = Some(PathBuf::from(value("--one")?)),
            "--twins" => twins = true,
            other if other.starts_with("--") => return Err(format!("unknown option {other}")),
            other => {
                if dir.replace(PathBuf::from(other)).is_some() {
                    return Err("one folder only".to_owned());
                }
            }
        }
    }
    let dir = match (dir, &one) {
        (Some(dir), _) => dir,
        (None, Some(_)) => PathBuf::new(),
        (None, None) => return Err("a folder of workbooks is needed".to_owned()),
    };
    Ok(Args {
        dir,
        out,
        timeout,
        one,
        twins,
    })
}

pub fn run(raw: &[String]) -> ExitCode {
    let args = match parse(raw) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("xtask fidelity: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let result = match &args.one {
        Some(file) => one(file, &args.out, args.twins),
        None => all(&args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask fidelity: {e}");
            ExitCode::from(1)
        }
    }
}

/// The workbooks of `dir`, in name order: only the `.xlsb` ones when `twins`.
fn workbooks(dir: &Path, twins: bool) -> Result<Vec<PathBuf>, String> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let extension = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            path.is_file()
                && !name.starts_with("~$")
                && if twins {
                    extension == "xlsb"
                } else {
                    matches!(extension.as_str(), "xlsx" | "xlsm" | "xlsb")
                }
        })
        .collect();
    found.sort();
    Ok(found)
}

fn record_path(out: &Path, file: &Path) -> PathBuf {
    let name = file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workbook");
    out.join("workbooks").join(format!("{name}.json"))
}

fn all(args: &Args) -> Result<(), String> {
    let files = workbooks(&args.dir, args.twins)?;
    fs::create_dir_all(args.out.join("workbooks"))
        .map_err(|e| format!("cannot create {}: {e}", args.out.display()))?;
    let me = std::env::current_exe().map_err(|e| format!("cannot find this executable: {e}"))?;
    let started = Instant::now();
    let mut records = Vec::with_capacity(files.len());
    for (index, file) in files.iter().enumerate() {
        let path = record_path(&args.out, file);
        let _ = fs::remove_file(&path);
        let begun = Instant::now();
        let mut command = Command::new(&me);
        command
            .arg("fidelity")
            .arg("--one")
            .arg(file)
            .arg("--out")
            .arg(&args.out);
        if args.twins {
            command.arg("--twins");
        }
        let mut child = command
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", me.display()))?;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if begun.elapsed() > args.timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(_) => break None,
            }
        };
        let name = file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_owned();
        let record = match (status, fs::read(&path)) {
            (Some(status), Ok(bytes)) if status.success() => serde_json::from_slice(&bytes)
                .unwrap_or_else(
                    |e| json!({"workbook": name, "crashed": format!("unreadable record: {e}")}),
                ),
            (Some(status), _) => {
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stderr.take() {
                    use std::io::Read;
                    let _ = pipe.read_to_string(&mut stderr);
                }
                let last = stderr
                    .lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("");
                json!({"workbook": name, "crashed": format!("exit {status}: {last}")})
            }
            (None, _) => {
                json!({"workbook": name, "crashed": format!("timed out after {} s", args.timeout.as_secs())})
            }
        };
        println!(
            "xtask fidelity: {}/{} {name}: {}",
            index + 1,
            files.len(),
            outcome(&record)
        );
        records.push(record);
    }
    let summary = summarize(&records, &args.dir, started.elapsed());
    write(
        &args.out.join("summary.json"),
        &serde_json::to_vec_pretty(&summary).unwrap(),
    )?;
    write(
        &args.out.join("summary.md"),
        summary_md(&summary).as_bytes(),
    )?;
    println!(
        "xtask fidelity: {}",
        summary["headline"].as_str().unwrap_or_default()
    );
    Ok(())
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// One line of what became of a workbook.
fn outcome(record: &Value) -> String {
    if let Some(crashed) = record["crashed"].as_str() {
        return format!("crashed: {crashed}");
    }
    if let Some(reason) = record["refused"]["reason"].as_str() {
        return format!("refused: {reason}");
    }
    let twin = match &record["twin"] {
        Value::Null => String::new(),
        twin if twin["equal"] == true => "twin equal, ".to_owned(),
        twin => format!(
            "twin differs: {}, ",
            twin["difference"]["kind"]
                .as_str()
                .or(twin["failure"].as_str())
                .unwrap_or("")
        ),
    };
    format!(
        "{twin}unchanged {}, edited {}",
        if record["unchanged"]["model_equal"] == true {
            "equal"
        } else {
            "differs"
        },
        match &record["edited"] {
            edited if edited["model_equal"] == true && edited["formats_equal"] == true =>
                "equal".to_owned(),
            edited => format!(
                "differs: {}",
                edited["difference"]["kind"]
                    .as_str()
                    .or(edited["failure"].as_str())
                    .unwrap_or("formats")
            ),
        }
    )
}

// ------------------------------------------------------------------------------------------
// One workbook.

fn open(bytes: &[u8]) -> Result<HostState, calc_host::OpenFailure> {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    HostState::open(source, &OpenPolicy::default(), HostOptions::default())
}

/// A refusal's typed reason: the variants of its type down to the first that carries a message,
/// as `Workbook/UnsupportedSheet`.
fn reason(failure: &calc_host::OpenFailure) -> String {
    let debug = format!("{failure:?}");
    let mut parts = Vec::new();
    for piece in debug.split(['(', '{', ' ']) {
        let piece = piece.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
        if piece.is_empty() || !piece.chars().next().is_some_and(char::is_uppercase) {
            break;
        }
        parts.push(piece.to_owned());
        if parts.len() == 3 {
            break;
        }
    }
    parts.join("/")
}

/// The name a saved package is written under: the workbook's stem, what was saved, and the
/// workbook's own extension (Excel opens a macro-enabled package only under `.xlsm`).
/// A binary workbook is saved as SpreadsheetML: `.xlsm` when it is macro-enabled, `.xlsx`
/// otherwise.
fn saved_name(name: &str, what: &str, macros: bool) -> String {
    match name.rsplit_once('.') {
        Some((stem, extension)) if extension.eq_ignore_ascii_case("xlsb") => {
            format!("{stem}.{what}.{}", if macros { "xlsm" } else { "xlsx" })
        }
        Some((stem, extension)) => format!("{stem}.{what}.{}", extension.to_ascii_lowercase()),
        None => format!("{name}.{what}.xlsx"),
    }
}

fn one(file: &Path, out: &Path, twins: bool) -> Result<(), String> {
    let name = file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workbook")
        .to_owned();
    let bytes = fs::read(file).map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    let mut record = json!({
        "workbook": name,
        "sha256": calc_verdict::sha256_hex(&bytes),
        "bytes": bytes.len(),
    });
    let packages = out.join("packages");
    fs::create_dir_all(&packages)
        .map_err(|e| format!("cannot create {}: {e}", packages.display()))?;
    let started = Instant::now();
    match open(&bytes) {
        Err(failure) => {
            record["refused"] = json!({"reason": reason(&failure), "message": failure.to_string()});
        }
        Ok(host) => {
            record["open_s"] = json!(started.elapsed().as_secs_f64());
            if twins {
                record["twin"] = twin(file, &bytes, &host, out);
            }
            record["sheets"] = json!(host.sheets().len());
            let macros = host.model().definitions().macro_enabled();
            record["unchanged"] = unchanged(
                &bytes,
                &host,
                &packages.join(saved_name(&name, "unchanged", macros)),
            );
            record["edited"] = edited(
                &bytes,
                &host,
                &packages.join(saved_name(&name, "edited", macros)),
            );
        }
    }
    record["total_s"] = json!(started.elapsed().as_secs_f64());
    fs::create_dir_all(out.join("workbooks")).map_err(|e| e.to_string())?;
    write(
        &record_path(out, file),
        &serde_json::to_vec_pretty(&record).unwrap(),
    )
}

/// The twin of the binary workbook `file` (the `.xlsx` or `.xlsm` of its stem), opened and
/// compared with the binary workbook's opened model; the package the binary workbook is read as is
/// written to `<out>/converted/`.
fn twin(file: &Path, bytes: &[u8], host: &HostState, out: &Path) -> Value {
    let converted = calc_host::convert_binary_workbook(
        Arc::new(MemorySource::new(bytes.to_vec())),
        &OpenPolicy::default(),
    );
    if let Ok(Some(package)) = &converted {
        let folder = out.join("converted");
        let _ = fs::create_dir_all(&folder);
        let stem = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("workbook");
        let extension = if host.model().definitions().macro_enabled() {
            "xlsm"
        } else {
            "xlsx"
        };
        let _ = fs::write(folder.join(format!("{stem}.{extension}")), package);
    }
    let Some(path) = ["xlsx", "xlsm"]
        .iter()
        .map(|extension| file.with_extension(extension))
        .find(|path| path.is_file())
    else {
        return json!({"failure": "no twin"});
    };
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_owned();
    let twin_bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) => return json!({"name": name, "failure": format!("read: {e}")}),
    };
    let twin = match open(&twin_bytes) {
        Ok(twin) => twin,
        Err(e) => {
            return json!({"name": name, "failure": format!("twin refused: {}", reason(&e)), "message": e.to_string()});
        }
    };
    let mut difference = model_difference(twin.model(), host.model(), false);
    if difference.is_none() {
        difference =
            formats_difference(&twin, host, &[]).map(|detail| ("styles".to_owned(), detail));
    }
    if difference.is_none() {
        difference = twin
            .loaded()
            .sheets()
            .iter()
            .zip(host.loaded().sheets())
            .find(|(a, b)| {
                a.conditional_formats() != b.conditional_formats()
                    || a.validations() != b.validations()
            })
            .map(|(a, b)| {
                let first = a
                    .conditional_formats()
                    .iter()
                    .zip(b.conditional_formats())
                    .find(|(x, y)| x != y);
                (
                    "conditional_formats".to_owned(),
                    clip(&format!(
                        "sheet {:?}: {} and {} blocks; first {first:?}; validations {:?} and {:?}",
                        a.id(),
                        a.conditional_formats().len(),
                        b.conditional_formats().len(),
                        a.validations(),
                        b.validations()
                    )),
                )
            });
    }
    if difference.is_none() {
        let settings = |h: &HostState| {
            format!(
                "{:?} {:?}",
                h.loaded().date_system(),
                h.loaded().calc_mode()
            )
        };
        if settings(&twin) != settings(host) {
            difference = Some((
                "settings".to_owned(),
                format!("{} and {}", settings(&twin), settings(host)),
            ));
        }
    }
    json!({
        "name": name,
        "equal": difference.is_none(),
        "difference": difference.map(|(kind, detail)| json!({"kind": kind, "detail": detail})),
    })
}

fn unchanged(bytes: &[u8], host: &HostState, path: &Path) -> Value {
    let mut saved = Vec::new();
    if let Err(e) = calc_host::save(host, &SavePolicy::default(), &mut saved) {
        return json!({"failure": format!("save: {e}")});
    }
    let _ = fs::write(path, &saved);
    let opened = saved == bytes;
    match open(&saved) {
        Err(e) => json!({"failure": format!("reopen: {e}"), "byte_equal": opened}),
        Ok(again) => {
            let difference = model_difference(host.model(), again.model(), true);
            json!({
                "byte_equal": opened,
                "model_equal": difference.is_none(),
                "difference": difference.map(|(kind, detail)| json!({"kind": kind, "detail": detail})),
            })
        }
    }
}

/// The cells the edit enters: two cells right of the first visible sheet's used area (below it
/// when it reaches the last column), and that sheet.
fn edit_cells(host: &HostState) -> Option<(SheetId, CellKey, CellKey)> {
    // A worksheet: an Excel 4.0 macro sheet is saved only as it was read (dc-xuw).
    let worksheet = |entry: &&calc_host::SheetEntry| {
        host.loaded()
            .sheet(entry.id())
            .is_some_and(|sheet| !sheet.is_macro_sheet())
    };
    let sheet = host
        .sheets()
        .iter()
        .filter(worksheet)
        .find(|entry| format!("{:?}", entry.visibility()) == "Visible")
        .or_else(|| host.sheets().iter().find(worksheet))?;
    let id = sheet.id();
    let free = |row: u32, column: u32| -> Option<(CellKey, CellKey)> {
        let a = CellKey::new(id, row, column).ok()?;
        let b = CellKey::new(id, row, column + 1).ok()?;
        (host.model().cell(a).is_none() && host.model().cell(b).is_none()).then_some((a, b))
    };
    let (row, column) = match host.used_area(id) {
        None => (1, 1),
        Some(area) if area.last().column() + 2 <= MAX_COLUMNS => (1, area.last().column() + 1),
        Some(area) => (area.last().row().saturating_add(1), 1),
    };
    // A used area that reaches the grid's last row and column: the first two free cells of the
    // first rows.
    let (a, b) = free(row, column).or_else(|| {
        (1..=100u32).find_map(|row| (1..=100u32).find_map(|column| free(row, column)))
    })?;
    Some((id, a, b))
}

fn letters(mut column: u32) -> String {
    let mut out = Vec::new();
    while column > 0 {
        out.push(b'A' + ((column - 1) % 26) as u8);
        column = (column - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

fn edited(bytes: &[u8], host: &HostState, path: &Path) -> Value {
    let Some((_, value_at, formula_at)) = edit_cells(host) else {
        return json!({"no_sheet": true});
    };
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    let mut session =
        match HostSession::open(source, &OpenPolicy::default(), HostOptions::default()) {
            Ok(session) => session,
            Err(e) => return json!({"failure": format!("session: {e}")}),
        };
    let started = Instant::now();
    let reference = format!("{}{}", letters(value_at.column()), value_at.row());
    for intent in [
        SkinIntent::Recalculate,
        SkinIntent::EnterCell {
            at: value_at,
            text: "12345.5".to_owned(),
        },
        SkinIntent::EnterCell {
            at: formula_at,
            text: format!("={reference}*2"),
        },
        SkinIntent::Recalculate,
    ] {
        let receipt = session.dispatch(intent.clone());
        if !receipt.accepted() {
            return json!({"failure": format!("{intent:?}: {:?}", receipt.error)});
        }
    }
    let recalc_s = started.elapsed().as_secs_f64();
    let mut saved = Vec::new();
    if let Err(e) = session.save(&SavePolicy::default(), &mut saved) {
        return json!({"failure": format!("save: {e}"), "recalc_s": recalc_s});
    }
    let _ = fs::write(path, &saved);
    let again = match open(&saved) {
        Ok(again) => again,
        Err(e) => return json!({"failure": format!("reopen: {e}"), "recalc_s": recalc_s}),
    };
    let difference = model_difference(session.host().model(), again.model(), false);
    let formats = formats_difference(host, &again, &[value_at, formula_at]);
    let caches = caches(session.host(), &again);
    json!({
        "cells": [format!("{}{}", letters(value_at.column()), value_at.row()), format!("{}{}", letters(formula_at.column()), formula_at.row())],
        "recalc_s": recalc_s,
        "model_equal": difference.is_none(),
        "difference": difference.map(|(kind, detail)| json!({"kind": kind, "detail": detail})),
        "formats_equal": formats.is_none(),
        "formats_difference": formats,
        "caches": caches,
    })
}

/// The first difference between two models, as (kind, detail); `None` when they are equal.
/// `revision` says whether their revisions are compared too.
fn model_difference(a: &ModelRoot, b: &ModelRoot, revision: bool) -> Option<(String, String)> {
    if revision && a.revision() != b.revision() {
        return Some((
            "revision".to_owned(),
            format!("{:?} and {:?}", a.revision(), b.revision()),
        ));
    }
    if a.sheets() != b.sheets() {
        return Some((
            "sheets".to_owned(),
            format!("{:?}\n{:?}", a.sheets(), b.sheets()),
        ));
    }
    for entry in a.sheets() {
        let cells = |root: &ModelRoot| -> BTreeMap<CellKey, calc_host::CellInput> {
            root.sheet_cells(entry.id())
                .into_iter()
                .flatten()
                .map(|(at, input)| (at, input.clone()))
                .collect()
        };
        let (x, y) = (cells(a), cells(b));
        if x != y {
            let first = x
                .iter()
                .find(|(at, input)| y.get(at) != Some(input))
                .map(|(at, input)| {
                    format!(
                        "R{}C{}: {input:?} and {:?}",
                        at.row(),
                        at.column(),
                        y.get(at)
                    )
                })
                .or_else(|| {
                    y.iter()
                        .find(|(at, _)| !x.contains_key(at))
                        .map(|(at, input)| {
                            format!("R{}C{}: none and {input:?}", at.row(), at.column())
                        })
                })
                .unwrap_or_default();
            return Some((
                "cells".to_owned(),
                format!(
                    "sheet {:?}: {} and {} cells; {first}",
                    entry.name().text().to_string_lossy(),
                    x.len(),
                    y.len()
                ),
            ));
        }
    }
    let (da, db) = (a.definitions(), b.definitions());
    let parts: [(&str, String, String); 9] = [
        (
            "names",
            format!("{:?}", da.names()),
            format!("{:?}", db.names()),
        ),
        ("arrays", arrays(da), arrays(db)),
        (
            "tables",
            format!("{:?}", da.tables()),
            format!("{:?}", db.tables()),
        ),
        (
            "data_tables",
            format!("{:?}", da.data_tables()),
            format!("{:?}", db.data_tables()),
        ),
        (
            "unloaded_cells",
            format!("{:?}", da.unloaded_cells().collect::<Vec<_>>()),
            format!("{:?}", db.unloaded_cells().collect::<Vec<_>>()),
        ),
        (
            "hidden_rows",
            format!("{:?}", da.hidden_rows_all().collect::<Vec<_>>()),
            format!("{:?}", db.hidden_rows_all().collect::<Vec<_>>()),
        ),
        (
            "iteration",
            format!("{:?}", da.iteration()),
            format!("{:?}", db.iteration()),
        ),
        (
            "macro_enabled",
            format!("{:?}", da.macro_enabled()),
            format!("{:?}", db.macro_enabled()),
        ),
        (
            "workbook_name",
            format!("{:?}", da.workbook_name()),
            format!("{:?}", db.workbook_name()),
        ),
    ];
    for (kind, x, y) in parts {
        if x != y {
            return Some((kind.to_owned(), clip(&format!("{x}\n{y}"))));
        }
    }
    if da.cell_formats().ne(db.cell_formats()) {
        return Some(("formats".to_owned(), String::new()));
    }
    if a.layout() != b.layout() {
        return Some(("layout".to_owned(), String::new()));
    }
    None
}

/// The array formulas: a legacy array by its area, a dynamic array by its anchor and kind (its
/// area is the extent of its last result, which a save writes as its `ref`).
fn arrays(definitions: &calc_host::Definitions) -> String {
    let shown: Vec<String> = definitions
        .arrays()
        .map(|array| match array.kind() {
            calc_host::ArrayKind::Legacy => format!("{:?}", array.area()),
            kind => format!("{:?} {kind:?}", array.anchor()),
        })
        .collect();
    format!("{shown:?}")
}

fn clip(text: &str) -> String {
    text.chars().take(2000).collect()
}

/// Whether each sheet's styled cells and geometry, as the files state them, are those of the
/// opened workbook, but the cells `entered`; the first difference otherwise.
fn formats_difference(
    opened: &HostState,
    again: &HostState,
    entered: &[CellKey],
) -> Option<String> {
    for (a, b) in opened.loaded().sheets().iter().zip(again.loaded().sheets()) {
        let styled = |sheet: &calc_host::LoadedSheet| -> Vec<(u32, u32, u32)> {
            sheet
                .styled_cells()
                .filter(|(row, column, _)| {
                    !entered.iter().any(|at| {
                        at.sheet() == sheet.id() && (at.row(), at.column()) == (*row, *column)
                    })
                })
                .collect()
        };
        let (x, y) = (styled(a), styled(b));
        if x != y {
            let first = x.iter().zip(&y).find(|(p, q)| p != q);
            return Some(format!(
                "styles of sheet {:?}: {} and {} styled cells; first {first:?}",
                a.id(),
                x.len(),
                y.len()
            ));
        }
        if a.geometry() != b.geometry() {
            return Some(format!("geometry of sheet {:?}", a.id()));
        }
    }
    None
}

/// The formula cells of the session's model: how many it calculated, how many of those the saved
/// package holds with that value as their cached value, and how many with another.
fn caches(session: &HostState, again: &HostState) -> Value {
    let formulas: Vec<CellKey> = session
        .model()
        .sheets()
        .iter()
        .flat_map(|entry| {
            session
                .model()
                .sheet_cells(entry.id())
                .into_iter()
                .flatten()
                .filter(|(_, input)| matches!(input, calc_host::CellInput::Formula(_)))
                .map(|(at, _)| at)
                .collect::<Vec<_>>()
        })
        .collect();
    let request = ProjectionRequest {
        cells: formulas.clone(),
    };
    let (Ok(calculated), Ok(saved)) = (project(session, &request), project(again, &request)) else {
        return json!({"failure": "a projection was refused"});
    };
    let (mut calculated_n, mut equal, mut other, mut missing) = (0, 0, 0, 0);
    let mut examples: Vec<String> = Vec::new();
    for (mine, theirs) in calculated.cells().iter().zip(saved.cells()) {
        let ProjectedValue::Calculated { value, .. } = mine.value() else {
            continue;
        };
        calculated_n += 1;
        match theirs.value() {
            ProjectedValue::FileCached(cache) => {
                if calc_verdict::compare(value, &cache.value().to_value()).is_none() {
                    equal += 1;
                } else {
                    other += 1;
                }
            }
            other => {
                missing += 1;
                if examples.len() < 10 {
                    examples.push(format!(
                        "{:?} R{}C{}: calculated {:?}, saved as {:?}",
                        mine.at().sheet(),
                        mine.at().row(),
                        mine.at().column(),
                        value,
                        other
                    ));
                }
            }
        }
    }
    json!({"formula_cells": formulas.len(), "calculated": calculated_n, "cached_equal": equal, "cached_other": other, "not_cached": missing, "not_cached_examples": examples})
}

// ------------------------------------------------------------------------------------------
// The summary.

fn summarize(records: &[Value], dir: &Path, wall: Duration) -> Value {
    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    let mut refused: BTreeMap<String, u64> = BTreeMap::new();
    let mut crashed: BTreeMap<String, u64> = BTreeMap::new();
    let mut unchanged_kinds: BTreeMap<String, u64> = BTreeMap::new();
    let mut edited_kinds: BTreeMap<String, u64> = BTreeMap::new();
    let mut extensions: BTreeMap<String, u64> = BTreeMap::new();
    let mut twin_kinds: BTreeMap<String, u64> = BTreeMap::new();
    let (mut formula_cells, mut calculated, mut cached_equal, mut cached_other, mut not_cached) =
        (0u64, 0u64, 0u64, 0u64, 0u64);
    for record in records {
        *counts.entry("workbooks").or_default() += 1;
        let name = record["workbook"].as_str().unwrap_or("");
        let extension = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        *extensions.entry(extension).or_default() += 1;
        if let Some(why) = record["crashed"].as_str() {
            *counts.entry("crashed").or_default() += 1;
            *crashed
                .entry(why.split(':').next().unwrap_or(why).to_owned())
                .or_default() += 1;
            continue;
        }
        if let Some(reason) = record["refused"]["reason"].as_str() {
            *counts.entry("refused").or_default() += 1;
            *refused.entry(reason.to_owned()).or_default() += 1;
            continue;
        }
        *counts.entry("opened").or_default() += 1;
        match &record["twin"] {
            Value::Null => {}
            twin if twin["equal"] == true => *counts.entry("twin_equal").or_default() += 1,
            twin => {
                *counts.entry("twin_differs").or_default() += 1;
                let kind = twin["difference"]["kind"]
                    .as_str()
                    .or(twin["failure"].as_str())
                    .unwrap_or("")
                    .to_owned();
                *twin_kinds.entry(kind).or_default() += 1;
            }
        }
        let unchanged = &record["unchanged"];
        if unchanged["byte_equal"] == true {
            *counts.entry("unchanged_byte_equal").or_default() += 1;
        }
        if unchanged["model_equal"] == true {
            *counts.entry("unchanged_model_equal").or_default() += 1;
        } else {
            let kind = unchanged["difference"]["kind"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| {
                    unchanged["failure"]
                        .as_str()
                        .map(|f| f.split(':').next().unwrap_or(f).to_owned())
                })
                .unwrap_or_default();
            *unchanged_kinds.entry(kind).or_default() += 1;
        }
        let edited = &record["edited"];
        if edited["no_sheet"] == true {
            *counts.entry("edited_no_sheet").or_default() += 1;
            continue;
        }
        if let Some(failure) = edited["failure"].as_str() {
            *edited_kinds
                .entry(format!(
                    "failure: {}",
                    failure.split(':').next().unwrap_or(failure)
                ))
                .or_default() += 1;
            continue;
        }
        *counts.entry("edited_saved").or_default() += 1;
        let model = edited["model_equal"] == true;
        let formats = edited["formats_equal"] == true;
        if model {
            *counts.entry("edited_model_equal").or_default() += 1;
        } else {
            *edited_kinds
                .entry(
                    edited["difference"]["kind"]
                        .as_str()
                        .unwrap_or("")
                        .to_owned(),
                )
                .or_default() += 1;
        }
        if formats {
            *counts.entry("edited_formats_equal").or_default() += 1;
        } else {
            *edited_kinds.entry("formats".to_owned()).or_default() += 1;
        }
        if model && formats {
            *counts.entry("edited_equal").or_default() += 1;
        }
        let caches = &edited["caches"];
        formula_cells += caches["formula_cells"].as_u64().unwrap_or(0);
        calculated += caches["calculated"].as_u64().unwrap_or(0);
        cached_equal += caches["cached_equal"].as_u64().unwrap_or(0);
        cached_other += caches["cached_other"].as_u64().unwrap_or(0);
        not_cached += caches["not_cached"].as_u64().unwrap_or(0);
    }
    let get = |key: &str| counts.get(key).copied().unwrap_or(0);
    let twins = if get("twin_equal") + get("twin_differs") > 0 {
        format!(
            "; same model as its twin {} of {} opened",
            get("twin_equal"),
            get("opened")
        )
    } else {
        String::new()
    };
    let headline = format!(
        "{} workbooks: opened {}, refused {}, crashed {}; unchanged save reopened equal {} (byte-equal {}); edited save reopened equal {} of {} saved{twins}",
        get("workbooks"),
        get("opened"),
        get("refused"),
        get("crashed"),
        get("unchanged_model_equal"),
        get("unchanged_byte_equal"),
        get("edited_equal"),
        get("edited_saved"),
    );
    json!({
        "format": "dnacalc-fidelity-summary",
        "version": 1,
        "dir": dir.display().to_string(),
        "repository": crate::capture::repository_revision(),
        "wall_time_s": wall.as_secs_f64(),
        "headline": headline,
        "counts": counts,
        "extensions": extensions,
        "refused_by_reason": refused,
        "crashed_by_reason": crashed,
        "unchanged_differences_by_kind": unchanged_kinds,
        "edited_differences_by_kind": edited_kinds,
        "twin_differences_by_kind": twin_kinds,
        "edited_caches": {"formula_cells": formula_cells, "calculated": calculated, "cached_equal": cached_equal, "cached_other": cached_other, "not_cached": not_cached},
    })
}

fn summary_md(summary: &Value) -> String {
    let mut out = String::from("# Document fidelity\n\n");
    out.push_str(&format!(
        "{}\n\n",
        summary["headline"].as_str().unwrap_or("")
    ));
    for (title, key) in [
        ("Counts", "counts"),
        ("Extensions", "extensions"),
        ("Refused by reason", "refused_by_reason"),
        ("Crashed", "crashed_by_reason"),
        (
            "Unchanged save: differences by kind",
            "unchanged_differences_by_kind",
        ),
        (
            "Edited save: differences by kind",
            "edited_differences_by_kind",
        ),
        ("Edited save: cached values", "edited_caches"),
        ("Twins: differences by kind", "twin_differences_by_kind"),
    ] {
        out.push_str(&format!("## {title}\n\n"));
        if let Some(map) = summary[key].as_object() {
            for (k, v) in map {
                out.push_str(&format!("- {k}: {v}\n"));
            }
        }
        out.push('\n');
    }
    out
}
