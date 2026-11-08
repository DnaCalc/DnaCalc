//! `cargo xtask app xll-accept onecalc --exe <dna-onecalc-desktop.exe> --bundle <dir> --plan <plan.json>
//! --out <dir> [--var NAME=VALUE]... [--set <name>]...`: the add-in acceptance run of OneCalc
//! milestone 4 (dc-0f1k, dc-snz.2), driven through the page.
//!
//! For each add-in set of the plan it starts the desktop exe (`--app`, its WebView2 on a DevTools
//! port, the first sheet named as the Excel workbook the answers were captured in named it), attaches
//! chromedriver to that WebView2 as the smoke test does, and then does what a person does: opens the
//! add-in panel, types the .xll's path, presses Load, waits for the page to list the add-in, enters
//! each case's formula in the formula field and reads the page's result. It does not use the
//! command-line harness, the CLI cases route or any second evaluator.
//!
//! The answers it compares with are Excel's: the controller records (`controller.jsonl`) of the
//! untraced Excel runs of `crates/calc-xll-reference/run.ps1` in an owned hidden session, one per
//! set, which `--var` names (`{excel}`). A case's own recorded `value` is checked against Excel's too,
//! so a stale case file shows. The report counts, per set and in all, as the honesty rules require:
//! matched Excel, mismatched, refused (the page said the add-in function is not modelled, or the
//! evaluation did not complete) and missing (Excel gave no answer); refused and missing count as
//! failures, and the pass rate is over all cases. Finally the page unloads the add-in again and the
//! report records what it said.
//!
//! A set whose `xll` is an `.xlam` loads a VBA add-in the same way (milestone 5, dc-0f1k.4); its
//! Excel answers come from `crates/calc-xll-reference/fixture/vba/vba-job.ps1 -Action cases`.
//!
//! A set with `session` (an .xlsm session file, milestone 5) opens that file through the page's
//! file input, so its own VBA project loads; enters each case formula in `cell` and reads the
//! page's result; saves the session through the page's Save (the exe runs with `--save-dir`, so
//! each save lands in a numbered folder under the file's own name without the dialog); then has
//! Excel reopen every saved file in the owned hidden session (`vba-job.ps1 -Action reopen`, through
//! the session host already running for this run) and compares the page's answer with what Excel
//! calculates in the reopened file, and records whether Excel repaired it (a "[Repaired]" name or a
//! new `error*.xml` repair log).
//!
//! Exit code: 0 when every case of every selected set matched, 1 otherwise, 2 for a usage error.

use super::*;
use std::collections::BTreeMap;

pub const USAGE: &str = "usage: cargo xtask app xll-accept onecalc --exe <dna-onecalc-desktop.exe> --bundle <dir> --plan <plan.json> --out <dir> [--var NAME=VALUE]... [--set <name>]... [--allow-unmatched]";

const PAGE_WAIT: Duration = Duration::from_secs(60);
const LOAD_WAIT: Duration = Duration::from_secs(240);
const CASE_WAIT: Duration = Duration::from_secs(60);

/// What a value is, for comparing the page's text with Excel's number.
#[derive(Clone, Debug, PartialEq)]
enum Shown {
    Number(f64),
    Text(String),
    Logical(bool),
    Error(String),
    Blank,
}

impl Shown {
    fn describe(&self) -> String {
        match self {
            Shown::Number(n) => format!("number {n}"),
            Shown::Text(t) => format!("text {t:?}"),
            Shown::Logical(b) => format!("logical {}", if *b { "TRUE" } else { "FALSE" }),
            Shown::Error(e) => format!("error {e}"),
            Shown::Blank => "blank".to_owned(),
        }
    }

    /// An Excel answer as the controller records it: a JSON number, string, boolean, null or
    /// `{"error": "#N/A"}`.
    fn of_json(value: &Value) -> Option<Shown> {
        Some(match value {
            Value::Number(n) => Shown::Number(n.as_f64()?),
            Value::String(s) => Shown::Text(s.clone()),
            Value::Bool(b) => Shown::Logical(*b),
            Value::Null => Shown::Blank,
            Value::Object(o) => Shown::Error(o.get("error")?.as_str()?.to_owned()),
            Value::Array(_) => return None,
        })
    }

    /// The page's `result` text: `number 42`, `text "a"`, `logical TRUE`, `error #N/A`, `blank`.
    fn of_page(text: &str) -> Option<Shown> {
        if let Some(n) = text.strip_prefix("number ") {
            return n.parse().ok().map(Shown::Number);
        }
        if let Some(t) = text.strip_prefix("text \"") {
            return Some(Shown::Text(t.strip_suffix('"')?.replace("\"\"", "\"")));
        }
        match text {
            "logical TRUE" => return Some(Shown::Logical(true)),
            "logical FALSE" => return Some(Shown::Logical(false)),
            "blank" => return Some(Shown::Blank),
            _ => {}
        }
        text.strip_prefix("error ").map(|e| Shown::Error(e.to_owned()))
    }

    /// An element of an array result as the table shows it.
    fn of_cell(text: &str) -> Shown {
        if let Ok(n) = text.parse::<f64>() {
            return Shown::Number(n);
        }
        match text {
            "TRUE" => Shown::Logical(true),
            "FALSE" => Shown::Logical(false),
            _ if text.starts_with('#') => Shown::Error(text.to_owned()),
            _ => Shown::Text(text.to_owned()),
        }
    }

    /// Process-dependent numbers inside a text are not compared: Excel's own sheet handle in a
    /// reference description (`sheet=2494755788000`) is a different number in every process.
    fn normalized(&self) -> Shown {
        match self {
            Shown::Text(t) => {
                let mut out = String::new();
                let mut rest = t.as_str();
                while let Some(at) = rest.find("sheet=") {
                    out.push_str(&rest[..at + 6]);
                    rest = &rest[at + 6..];
                    let digits = rest.chars().take_while(char::is_ascii_digit).count();
                    if digits > 0 {
                        out.push('N');
                    }
                    rest = &rest[digits..];
                }
                out.push_str(rest);
                Shown::Text(out)
            }
            other => other.clone(),
        }
    }

    fn same(&self, other: &Shown) -> bool {
        match (&self.normalized(), &other.normalized()) {
            // A blank result is the number 0 in a cell (Excel shows 0).
            (Shown::Blank, Shown::Number(n)) | (Shown::Number(n), Shown::Blank) => *n == 0.0,
            (Shown::Number(a), Shown::Number(b)) => a == b,
            (a, b) => a == b,
        }
    }
}

fn expand(text: &str, vars: &BTreeMap<String, String>) -> String {
    let mut out = text.to_owned();
    for (name, value) in vars {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

fn read_json(path: &Path) -> Result<Value, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| format!("{}: {e}", path.display()))
}

/// Excel's answer for each formula of a controller record: the last line whose operation is the
/// formula text.
fn excel_answers(path: &Path) -> Result<BTreeMap<String, Value>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut answers = BTreeMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let record: Value =
            serde_json::from_str(line.trim_start_matches('\u{feff}')).map_err(|e| format!("{}: {e}", path.display()))?;
        if let (Some(operation), Some(value)) = (record.get("operation").and_then(Value::as_str), record.get("value")) {
            answers.insert(operation.to_owned(), value.clone());
        }
    }
    Ok(answers)
}

fn text_of(session: &Session, role: &str) -> String {
    let script = format!(
        "var e = document.querySelector('[data-onecalc=\"{role}\"]'); return e ? e.textContent : null;"
    );
    match session.script(&script) {
        Ok(Value::String(text)) => text,
        _ => String::new(),
    }
}

fn items_of(session: &Session, role: &str) -> Vec<String> {
    let script = format!(
        "return Array.from(document.querySelectorAll('[data-onecalc=\"{role}\"] li')).map(function(e){{return e.textContent;}});"
    );
    match session.script(&script) {
        Ok(Value::Array(items)) => items.iter().filter_map(|i| i.as_str().map(str::to_owned)).collect(),
        _ => Vec::new(),
    }
}

/// Sets the formula field's text as typing would leave it and tells the page (one input event).
fn enter_formula(session: &Session, formula: &str) -> Result<(), String> {
    let quoted = serde_json::to_string(formula).map_err(|e| e.to_string())?;
    session
        .script(&format!(
            "var e = document.querySelector('[data-onecalc=\"formula\"]'); e.focus(); e.value = {quoted}; e.dispatchEvent(new Event('input', {{bubbles: true}})); return true;"
        ))
        .map(|_| ())
}

fn set_text_input(session: &Session, role: &str, text: &str) -> Result<(), String> {
    let quoted = serde_json::to_string(text).map_err(|e| e.to_string())?;
    session
        .script(&format!(
            "var e = document.querySelector('[data-onecalc=\"{role}\"]'); e.focus(); e.value = {quoted}; e.dispatchEvent(new Event('input', {{bubbles: true}})); return true;"
        ))
        .map(|_| ())
}

fn click(session: &Session, role: &str) -> Result<(), String> {
    let element = session.element(&format!("[data-onecalc=\"{role}\"]"))?;
    session.post(&format!("/element/{element}/click"), json!({})).map(|_| ())
}

fn wait_until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let started = Instant::now();
    loop {
        if done() {
            return true;
        }
        if started.elapsed() > limit {
            return false;
        }
        thread::sleep(Duration::from_millis(40));
    }
}

/// The settled result of the formula in the field: the page first shows "no formula" for an empty
/// field, so each case is entered after an empty one and waits for the answer to change.
fn evaluate(session: &Session, formula: &str) -> Result<(String, Vec<String>, Option<Shown>), String> {
    enter_formula(session, "")?;
    if !wait_until(PAGE_WAIT, || text_of(session, "result") == "no formula") {
        return Err("the page did not return to an empty formula".to_owned());
    }
    enter_formula(session, formula)?;
    if !wait_until(CASE_WAIT, || text_of(session, "result") != "no formula") {
        return Err("the page showed no result within the wait".to_owned());
    }
    // The answer is in; let any second reply (completion, a late diagnostic) land.
    thread::sleep(Duration::from_millis(30));
    let result = text_of(session, "result");
    let diagnostics = items_of(session, "diagnostics");
    let first = if result.starts_with("array of") {
        session
            .script("var e = document.querySelector('[data-onecalc=\"result-array\"] td'); return e ? e.textContent : null;")
            .ok()
            .and_then(|v| v.as_str().map(Shown::of_cell))
    } else {
        Shown::of_page(&result)
    };
    Ok((result, diagnostics, first))
}

/// The registered function names an add-in's line in the page's list shows (`... (sha256:..) - A, B Unload`).
fn names_listed(line: &str) -> Vec<String> {
    let Some((_, tail)) = line.split_once(") - ") else {
        return Vec::new();
    };
    tail.trim_end_matches("Unload")
        .split(',')
        .map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty() && !n.starts_with("no worksheet"))
        .collect()
}

/// For the first functions of each add-in the page lists: types a call to it and reads the
/// signature line and the function help panel, then types the start of its name and reads the
/// completion list. The text is what the add-in gave `xlfRegister`; the run compares nothing here
/// (Excel's own tooltip is not readable without its window), it records what a person is shown.
fn describe_functions(session: &Session, listed: &[String]) -> Value {
    let mut shown = Vec::new();
    for name in listed.iter().flat_map(|line| names_listed(line)).take(8) {
        let call = format!("={name}(");
        if enter_formula(session, "").is_err() || enter_formula(session, &call).is_err() {
            continue;
        }
        wait_until(Duration::from_secs(5), || !text_of(session, "signature").is_empty());
        thread::sleep(Duration::from_millis(60));
        let signature = text_of(session, "signature");
        let help_name = text_of(session, "help-name");
        let help_signature = text_of(session, "help-signature");
        let description = text_of(session, "help-description");
        let status = text_of(session, "help-status");
        let parameters = items_of_role(session, "help-parameter");
        let prefix: String = name.chars().take(name.chars().count().clamp(1, 4)).collect();
        let _ = enter_formula(session, "");
        let _ = enter_formula(session, &format!("={prefix}"));
        thread::sleep(Duration::from_millis(80));
        let completions = items_of_role(session, "completion");
        shown.push(json!({
            "function": name, "signature": signature, "help_name": help_name, "help_signature": help_signature,
            "description": description, "parameters": parameters, "status": status,
            "completion_prefix": prefix, "completions": completions.into_iter().take(6).collect::<Vec<_>>(),
        }));
    }
    let _ = enter_formula(session, "");
    json!(shown)
}

fn items_of_role(session: &Session, role: &str) -> Vec<String> {
    let script = format!(
        "return Array.from(document.querySelectorAll('[data-onecalc=\"{role}\"]')).map(function(e){{return e.textContent;}});"
    );
    match session.script(&script) {
        Ok(Value::Array(items)) => items.iter().filter_map(|i| i.as_str().map(str::to_owned)).collect(),
        _ => Vec::new(),
    }
}

/// Opens a session that lists `xll` twice (the right hash and a wrong one) and loads what it offers.
fn reopen_offer(session: &Session, xll: &Path, name: &str, out: &Path) -> Result<Value, String> {
    let bytes = fs::read(xll).map_err(|e| e.to_string())?;
    let hash = format!("sha256:{}", calc_verdict::sha256_hex(&bytes));
    let wrong = format!("sha256:{}", "0".repeat(64));
    let path_text = xll.display().to_string();
    let escape = |t: &str| t.replace('&', "&amp;").replace('<', "&lt;").replace('"', "&quot;");
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>
<onecalc xmlns=\"urn:dnacalc:onecalc-session\" version=\"1\"><options locale=\"en-US\" numberFormat=\"General\"/><addins><addin kind=\"{k}\" path=\"{p}\" hash=\"{wrong}\"/><addin kind=\"{k}\" path=\"{p}\" hash=\"{hash}\"/></addins></onecalc>",
        p = escape(&path_text),
        k = xll.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
    );
    let file = calc_doc::SessionFile {
        sheet: "Sheet1".to_owned(),
        formula: Some(calc_doc::SessionFormula {
            row: 1,
            column: 8,
            text: "1+1".to_owned(),
            array: calc_doc::SessionArray::Dynamic,
        }),
        constants: Vec::new(),
        custom: Some(calc_doc::CustomXmlItem {
            namespace: "urn:dnacalc:onecalc-session".to_owned(),
            content: content.into_bytes(),
        }),
        date_1904: false,
        cached: None,
        vba: None,
    };
    let session_bytes = calc_doc::write_session(&file).map_err(|e| e.to_string())?;
    let directory = out.join("reopen");
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let path = directory.join(format!("{name}.xlsx"));
    fs::write(&path, session_bytes).map_err(|e| e.to_string())?;
    // The add-in is unloaded by now; the page's own state may be unsaved, which asks first.
    let offered_before = items_of(session, "addins-offered").len();
    let element = session.element("[data-onecalc=\"file-input\"]")?;
    let text = path.canonicalize().map_err(|e| e.to_string())?.display().to_string();
    let text = text.strip_prefix(r"\?\").unwrap_or(&text).to_owned();
    session.post(&format!("/element/{element}/value"), json!({"text": text}))?;
    thread::sleep(Duration::from_millis(300));
    if text_of(session, "file-dialog").contains("Replace unsaved changes") {
        let visible = session.script("return document.querySelector('[data-onecalc=\"file-dialog\"]').open;").ok().and_then(|v| v.as_bool());
        if visible == Some(true) {
            click(session, "file-replace")?;
        }
    }
    if !wait_until(PAGE_WAIT, || items_of(session, "addins-offered").len() >= 2) {
        return Err(format!("the page offered {} add-ins (before {offered_before})", items_of(session, "addins-offered").len()));
    }
    let offered = items_of(session, "addins-offered");
    let loaded_after_open = items_of(session, "addins-loaded").len();
    // The first offer carries the wrong hash.
    session.script("document.querySelectorAll('[data-onecalc=\"addin-load-listed\"]')[0].click(); return true;")?;
    thread::sleep(Duration::from_millis(400));
    let refusal = text_of(session, "addin-status");
    let loaded_after_wrong = items_of(session, "addins-loaded").len();
    session.script("document.querySelectorAll('[data-onecalc=\"addin-load-listed\"]')[1].click(); return true;")?;
    let came = wait_until(LOAD_WAIT, || {
        items_of(session, "addins-loaded").len() > loaded_after_wrong || {
            let status = text_of(session, "addin-status");
            status != refusal
                && ["refused", "already loaded", "cannot", "rollback", "not an .xll", "failed", "dc-snz"]
                    .iter()
                    .any(|word| status.contains(word))
        }
    });
    let status = text_of(session, "addin-status");
    let loaded_after_right = items_of(session, "addins-loaded").len();
    Ok(json!({
        "offered": offered, "loaded_by_opening": loaded_after_open, "wrong_hash_status": refusal,
        "loaded_after_wrong_hash": loaded_after_wrong, "right_hash_status": status,
        "loaded_after_right_hash": loaded_after_right, "answered": came,
    }))
}

/// The desktop exe (`--app`, its WebView2 on a DevTools port, the first sheet named `sheet`, and
/// `extra` arguments), with chromedriver attached to that WebView2 as the smoke test attaches it.
fn start_page(
    exe: &Path,
    bundle: &Path,
    sheet: &str,
    name: &str,
    out: &Path,
    extra: &[String],
) -> Result<(DriverProcess, DriverProcess, Session), String> {
    let upstream = free_port()?;
    let child = Command::new(exe)
        .args([
            "--app",
            "--assets",
            &bundle.display().to_string(),
            "--sheet",
            sheet,
            "--debug-port",
            &upstream.to_string(),
            "--size",
            "1280x1000",
        ])
        .args(extra)
        .env("DNACALC_WEBVIEW2_DATA", out.join(format!("webview2-{name}")))
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", exe.display()))?;
    let app = DriverProcess { child, port: upstream };
    let started = Instant::now();
    while TcpStream::connect(("127.0.0.1", upstream)).is_err() {
        if started.elapsed() > Duration::from_secs(60) {
            return Err("the desktop exe did not open its DevTools port".to_owned());
        }
        thread::sleep(Duration::from_millis(200));
    }
    let browser = &BROWSERS[0];
    let version = variable_file(browser.variable)
        .or_else(|| find_on_path(browser.program))
        .and_then(|driver| cargo::output(Command::new(driver).arg("--version")).ok())
        .map(|out| cargo::text(&out.stdout))
        .and_then(|text| text.split_whitespace().nth(1).map(str::to_owned))
        .unwrap_or_default();
    let proxy = paint::devtools_proxy(upstream, format!("Chrome/{version}"))?;
    let (driver, session) = open_attached(browser, &out.join("downloads"), None, Some(&format!("127.0.0.1:{proxy}")))?;
    Ok((app, driver, session))
}

/// Opens the workbook at `path` through the page's file input, answering the page's question about
/// unsaved changes, and waits for the page to say it opened it.
fn open_through_page(session: &Session, path: &Path) -> Result<(), String> {
    let element = session.element("[data-onecalc=\"file-input\"]")?;
    let text = path.canonicalize().map_err(|e| e.to_string())?.display().to_string();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned();
    session.post(&format!("/element/{element}/value"), json!({"text": text}))?;
    thread::sleep(Duration::from_millis(300));
    if text_of(session, "file-dialog").contains("Replace unsaved changes") {
        let visible = session.script("return document.querySelector('[data-onecalc=\"file-dialog\"]').open;").ok().and_then(|v| v.as_bool());
        if visible == Some(true) {
            click(session, "file-replace")?;
        }
    }
    let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !wait_until(PAGE_WAIT, || text_of(session, "file-status").contains(&format!("Opened {file}"))) {
        return Err(format!("the page did not open {file}: {}", text_of(session, "file-status")));
    }
    Ok(())
}

/// A session set (see the module comment): open the .xlsm through the page, evaluate and save each
/// case, have Excel reopen the saved files, compare.
fn run_session_set(
    exe: &Path,
    bundle: &Path,
    set: &Value,
    vars: &BTreeMap<String, String>,
    fixture_dir: &Path,
    out: &Path,
) -> Result<Value, String> {
    let name = set["name"].as_str().ok_or("a set needs a name")?;
    let file = PathBuf::from(expand(set["session"].as_str().ok_or("a session set needs a session")?, vars));
    let sheet = set["sheet"].as_str().unwrap_or("Sheet1");
    let cell = set["cell"].as_str().unwrap_or("H1");
    let cases_path = {
        let text = expand(set["cases"].as_str().ok_or("a set needs cases")?, vars);
        let path = PathBuf::from(&text);
        if path.is_absolute() { path } else { fixture_dir.join(path) }
    };
    let job = PathBuf::from(expand(set["excel_job"].as_str().ok_or("a session set needs excel_job")?, vars));
    let cases = read_json(&cases_path)?;
    let cases = cases.as_array().ok_or("cases are an array")?;
    let saved_dir = out.join(format!("saved-{name}"));
    if saved_dir.exists() {
        return Err(format!("{} exists; earlier evidence is never overwritten", saved_dir.display()));
    }
    let file_hash = calc_verdict::sha256_hex(&fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?);
    let (_app, _driver, session) =
        start_page(exe, bundle, sheet, name, out, &["--save-dir".to_owned(), saved_dir.display().to_string()])?;
    let page = (|| -> Result<(Value, Vec<(String, Result<(String, Vec<String>, Option<Shown>), String>, Option<String>)>), String> {
        if !wait_until(PAGE_WAIT, || !text_of(&session, "addin-count").is_empty()) {
            return Err("the page did not mount".to_owned());
        }
        open_through_page(&session, &file)?;
        // A macro file opens with macros disabled, as in Excel: the person enables them for it.
        click(&session, "macros-enable")?;
        if !wait_until(PAGE_WAIT, || {
            let status = text_of(&session, "vba-status");
            !status.is_empty() && !status.starts_with("Macros are disabled")
        }) {
            return Err("the page did not enable the file's macros".to_owned());
        }
        let project = text_of(&session, "vba-status");
        set_text_input(&session, "formula-cell", cell)?;
        // The Save button is in the session-file panel, which a person opens first.
        session.script("var d = document.querySelector('[data-onecalc=\"file-controls\"]'); if (!d.open) { d.querySelector('summary').click(); } return d.open;")?;
        let mut rows = Vec::new();
        for case in cases {
            let formula = case["formula"].as_str().ok_or("a case needs a formula")?.to_owned();
            let answer = evaluate(&session, &formula);
            let before = text_of(&session, "file-status");
            click(&session, "file-save")?;
            let saved = wait_until(CASE_WAIT, || {
                let status = text_of(&session, "file-status");
                status != before && status.starts_with("Saved ")
            })
            .then(|| text_of(&session, "file-status"))
            .and_then(|s| s.strip_prefix("Saved ").map(|p| p.trim_end_matches('.').to_owned()));
            rows.push((formula, answer, saved));
        }
        // An edit of the project through the VBA panel (its module goes into the file's project),
        // then its formula, evaluated and saved like a case: Excel reopens the changed project.
        let mut edit_status = Value::Null;
        if let Some(edit) = set.get("edit") {
            session.script("var d = document.querySelector('.oc-vba-panel'); if (!d.open) { d.querySelector('summary').click(); } return d.open;")?;
            set_text_input(&session, "vba-source", edit["source"].as_str().ok_or("an edit needs source")?)?;
            click(&session, "vba-load")?;
            wait_until(CASE_WAIT, || text_of(&session, "vba-status").starts_with("Loaded:") || text_of(&session, "vba-status").starts_with("Module not loaded"));
            edit_status = json!(text_of(&session, "vba-status"));
            let formula = edit["formula"].as_str().ok_or("an edit needs a formula")?.to_owned();
            let answer = evaluate(&session, &formula);
            let before = text_of(&session, "file-status");
            click(&session, "file-save")?;
            let saved = wait_until(CASE_WAIT, || {
                let status = text_of(&session, "file-status");
                status != before && status.starts_with("Saved ")
            })
            .then(|| text_of(&session, "file-status"))
            .and_then(|s| s.strip_prefix("Saved ").map(|p| p.trim_end_matches('.').to_owned()));
            rows.push((formula, answer, saved));
        }
        Ok((json!({"project": project, "edit": edit_status}), rows))
    })();
    session.end();
    let (opened, rows) = page?;
    // Excel reopens every saved file in the owned hidden session.
    let files: Vec<String> = rows.iter().filter_map(|(_, _, saved)| saved.clone()).collect();
    let excel_dir = out.join(format!("excel-{name}"));
    let mut command = Command::new("pwsh");
    command.args(["-NoProfile", "-Sta", "-File"]).arg(&job).args(["-Action", "reopen", "-OutDirectory"]).arg(&excel_dir).args(["-FormulaCell", cell, "-Files"]);
    command.arg(files.join(","));
    let status = cargo::output(&mut command).map(|o| cargo::text(&o.stdout)).unwrap_or_else(|e| e);
    let reopened = read_json(&excel_dir.join("reopen.json")).unwrap_or(Value::Null);
    let by_file: BTreeMap<String, &Value> = reopened["rows"]
        .as_array()
        .map(|rows| rows.iter().filter_map(|r| Some((r["file"].as_str()?.to_owned(), r))).collect())
        .unwrap_or_default();
    let mut results = Vec::new();
    let (mut matched, mut mismatched, mut refused, mut missing, mut repaired) = (0, 0, 0, 0, 0);
    for (formula, answer, saved) in &rows {
        let excel = saved.as_ref().and_then(|f| by_file.get(f));
        let theirs = excel.and_then(|r| excel_value(&r["excel"]));
        let repair = excel.is_some_and(|r| {
            r["repaired"].as_bool() == Some(true) || r["repair_logs"].as_array().is_some_and(|l| !l.is_empty())
        });
        if repair {
            repaired += 1;
        }
        let (verdict, detail) = match (answer, &theirs) {
            (Err(why), _) => ("refused", why.clone()),
            (Ok((result, diagnostics, _)), _) if result == "not calculated" => {
                ("refused", format!("not calculated: {}", diagnostics.join(" | ")))
            }
            (Ok(_), None) => ("missing", format!("Excel gave no answer for the saved file {saved:?}")),
            (Ok((result, diagnostics, shown)), Some(excel_value)) => match shown {
                Some(shown) if shown.same(excel_value) => ("matched", result.clone()),
                _ => ("mismatched", format!("{result}; Excel {}; {}", excel_value.describe(), diagnostics.join(" | "))),
            },
        };
        match verdict {
            "matched" => matched += 1,
            "mismatched" => mismatched += 1,
            "refused" => refused += 1,
            _ => missing += 1,
        }
        results.push(json!({
            "formula": formula, "verdict": verdict, "detail": detail, "saved": saved,
            "excel": theirs.as_ref().map(Shown::describe), "excel_formula": excel.map(|r| r["formula"].clone()),
            "repaired": repair, "excel_name": excel.map(|r| r["name"].clone()),
        }));
    }
    Ok(json!({
        "name": name, "session": file.display().to_string(), "session_sha256": file_hash,
        "cases_file": cases_path.display().to_string(), "excel_record": excel_dir.join("reopen.json").display().to_string(),
        "excel_job_output": status,
        "outcome": {
            "loaded": true, "opened": opened, "count": rows.len(), "matched": matched, "mismatched": mismatched,
            "refused": refused, "missing": missing, "repaired": repaired, "saved": files.len(), "results": results,
            "unload": {"unloaded": null}, "reopen": {"loaded_after_right_hash": null},
        },
    }))
}

/// A value `vba-job.ps1` recorded (`{value, type}`, `{error_code}` or `{blank}`) as the page shows it.
fn excel_value(value: &Value) -> Option<Shown> {
    if value["blank"].as_bool() == Some(true) {
        return Some(Shown::Blank);
    }
    if let Some(code) = value["error_code"].as_i64() {
        let name = match code {
            -2146826288 => "#NULL!",
            -2146826281 => "#DIV/0!",
            -2146826273 => "#VALUE!",
            -2146826265 => "#REF!",
            -2146826259 => "#NAME?",
            -2146826252 => "#NUM!",
            -2146826246 => "#N/A",
            _ => return None,
        };
        return Some(Shown::Error(name.to_owned()));
    }
    Shown::of_json(&value["value"])
}

fn run_set(
    exe: &Path,
    bundle: &Path,
    set: &Value,
    vars: &BTreeMap<String, String>,
    fixture_dir: &Path,
    out: &Path,
) -> Result<Value, String> {
    let name = set["name"].as_str().ok_or("a set needs a name")?;
    let xll = PathBuf::from(expand(set["xll"].as_str().ok_or("a set needs an xll")?, vars));
    let sheet = set["sheet"].as_str().unwrap_or("X0");
    let cell = set["cell"].as_str().unwrap_or("B3");
    let cases_path = {
        let text = expand(set["cases"].as_str().ok_or("a set needs cases")?, vars);
        let path = PathBuf::from(&text);
        if path.is_absolute() { path } else { fixture_dir.join(path) }
    };
    let excel_path = PathBuf::from(expand(set["excel"].as_str().ok_or("a set needs an excel record")?, vars));
    let cases = read_json(&cases_path)?;
    let cases = cases.as_array().ok_or("cases are an array")?;
    let excel = excel_answers(&excel_path)?;
    let xll_bytes = fs::read(&xll).map_err(|e| format!("{}: {e}", xll.display()))?;
    let xll_hash = calc_verdict::sha256_hex(&xll_bytes);
    let (_app, _driver, session) = start_page(exe, bundle, sheet, name, out, &[])?;
    let outcome = (|| -> Result<Value, String> {
        if !wait_until(PAGE_WAIT, || text_of(&session, "addin-count").ends_with("loaded")) {
            return Err("the page did not mount its add-in panel".to_owned());
        }
        // The formula cell and the one input the Excel runs had (A1 = 7).
        set_text_input(&session, "formula-cell", cell)?;
        session.script("var e = document.querySelector('[data-cell=\"A1\"]'); e.focus(); e.value = '7'; e.dispatchEvent(new Event('input', {bubbles: true})); return true;")?;
        // Load the add-in through the panel.
        session.script("var d = document.querySelector('[data-onecalc=\"addin-controls\"]'); if (!d.open) { d.querySelector('summary').click(); } return d.open;")?;
        // Companion add-ins first (the own native-owner add-in beside the Excel-DNA ones), then
        // the one under test; each is typed into the panel and loaded with its button.
        let also: Vec<PathBuf> = set["also"]
            .as_array()
            .map(|list| list.iter().filter_map(Value::as_str).map(|p| PathBuf::from(expand(p, vars))).collect())
            .unwrap_or_default();
        let (mut loaded, mut load_seconds, mut status) = (true, 0.0, String::new());
        for path in also.iter().chain(std::iter::once(&xll)) {
            let before = items_of(&session, "addins-loaded").len();
            set_text_input(&session, "addin-path", &path.display().to_string())?;
            let load_started = Instant::now();
            click(&session, "addin-load-path")?;
            let came = wait_until(LOAD_WAIT, || {
                items_of(&session, "addins-loaded").len() > before || {
                    let status = text_of(&session, "addin-status");
                    ["refused", "not the file", "already loaded", "cannot", "rollback", "not an .xll", "failed", "dc-snz", "dc-0f1k", "compile", "VBA project"]
                        .iter()
                        .any(|word| status.contains(word))
                }
            });
            load_seconds += load_started.elapsed().as_secs_f64();
            status = text_of(&session, "addin-status");
            if !came || items_of(&session, "addins-loaded").len() <= before {
                loaded = false;
                break;
            }
        }
        let listed = items_of(&session, "addins-loaded");
        if !loaded || listed.is_empty() {
            return Ok(json!({"loaded": false, "status": status, "load_seconds": load_seconds}));
        }
        // What the page shows of the registered functions: signature help and function help while
        // a call is typed, and the completion list for the start of a name.
        let documented = describe_functions(&session, &listed);
        let mut results = Vec::new();
        let (mut matched, mut mismatched, mut refused, mut missing, mut stale) = (0, 0, 0, 0, 0);
        for case in cases {
            let formula = case["formula"].as_str().ok_or("a case needs a formula")?;
            let formula_sent = formula.replace("{pid}", "0");
            let mine = case.get("value").and_then(Shown::of_json);
            let theirs = excel
                .get(formula)
                .or_else(|| excel.get(formula_sent.as_str()))
                .and_then(Shown::of_json);
            let case_started = Instant::now();
            let page = evaluate(&session, &formula_sent);
            let seconds = case_started.elapsed().as_secs_f64();
            let (verdict, detail) = match (&page, &theirs) {
                (Err(why), _) => ("refused", why.clone()),
                (Ok((result, diagnostics, _)), _) if result == "not calculated" => {
                    ("refused", format!("not calculated: {}", diagnostics.join(" | ")))
                }
                (Ok((result, diagnostics, shown)), Some(excel_value)) => match shown {
                    Some(shown) if shown.same(excel_value) => ("matched", result.clone()),
                    Some(_) => ("mismatched", format!("{result}; Excel {}; {}", excel_value.describe(), diagnostics.join(" | "))),
                    None => ("mismatched", format!("{result}; Excel {}", excel_value.describe())),
                },
                (Ok(_), None) => ("missing", "Excel gave no answer for this formula".to_owned()),
            };
            match verdict {
                "matched" => matched += 1,
                "mismatched" => mismatched += 1,
                "refused" => refused += 1,
                _ => missing += 1,
            }
            // The case file's own expectation against Excel's: a stale file shows here.
            // A case whose recorded value is null asserts nothing (a process-dependent result).
            let case_agrees = match (&mine, &theirs) {
                _ if case.get("value").is_some_and(Value::is_null) => true,
                (Some(a), Some(b)) => a.same(b),
                _ => true,
            };
            if !case_agrees {
                stale += 1;
            }
            results.push(json!({
                "formula": formula, "verdict": verdict, "detail": detail,
                "excel": theirs.as_ref().map(Shown::describe), "case_value_agrees_with_excel": case_agrees,
                "seconds": seconds,
            }));
        }
        // Unload through the panel: every add-in, the last loaded first.
        let unload = (|| -> Result<Value, String> {
            let mut unloaded = true;
            let mut statuses = Vec::new();
            loop {
                let before = items_of(&session, "addins-loaded").len();
                if before == 0 {
                    break;
                }
                session.script("var b = document.querySelectorAll('[data-onecalc=\"addin-unload\"]'); b[b.length - 1].click(); return true;")?;
                let went = wait_until(Duration::from_secs(60), || items_of(&session, "addins-loaded").len() < before);
                statuses.push(text_of(&session, "addin-status"));
                if !went {
                    unloaded = false;
                    break;
                }
            }
            Ok(json!({"unloaded": unloaded, "status": statuses}))
        })()
        .unwrap_or_else(|e| json!({"unloaded": false, "status": e}));
        // Reopen: a session file that lists the add-in (with the right hash, and with a wrong one)
        // is opened through the page's file input, which offers to load what it lists and loads
        // nothing itself; loading the listed entry with the wrong hash is refused, the right one loads.
        let reopen = reopen_offer(&session, &xll, name, out).unwrap_or_else(|e| json!({"error": e}));
        Ok(json!({
            "loaded": true, "status": status, "listed": listed, "load_seconds": load_seconds, "reopen": reopen,
            "documented": documented,
            "count": cases.len(), "matched": matched, "mismatched": mismatched, "refused": refused,
            "missing": missing, "case_values_stale": stale, "unload": unload, "results": results,
        }))
    })();
    session.end();
    let outcome = outcome?;
    Ok(json!({
        "name": name, "xll": xll.display().to_string(), "xll_sha256": xll_hash,
        "sheet": sheet, "cases_file": cases_path.display().to_string(), "excel_record": excel_path.display().to_string(),
        "outcome": outcome,
    }))
}

pub fn run(ws: &Workspace, raw: &[String]) -> ExitCode {
    let (mut exe, mut bundle, mut plan, mut out) = (None, None, None, None);
    let mut vars = BTreeMap::new();
    let mut only: Vec<String> = Vec::new();
    let mut allow_unmatched = false;
    let mut args = raw.iter();
    while let Some(arg) = args.next() {
        if arg == "--allow-unmatched" {
            allow_unmatched = true;
            continue;
        }
        match (arg.as_str(), args.next()) {
            ("--exe", Some(v)) => exe = Some(PathBuf::from(v)),
            ("--bundle", Some(v)) => bundle = Some(PathBuf::from(v)),
            ("--plan", Some(v)) => plan = Some(PathBuf::from(v)),
            ("--out", Some(v)) => out = Some(PathBuf::from(v)),
            ("--set", Some(v)) => only.push(v.clone()),
            ("--var", Some(v)) => match v.split_once('=') {
                Some((name, value)) => {
                    vars.insert(name.to_owned(), value.to_owned());
                }
                None => {
                    eprintln!("{USAGE}");
                    return ExitCode::from(2);
                }
            },
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let (Some(exe), Some(bundle), Some(plan), Some(out)) = (exe, bundle, plan, out) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let fixture_dir = ws.root.join("crates/calc-xll-reference/fixture");
    let result = (|| -> Result<(Value, bool), String> {
        fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        let plan = read_json(&plan)?;
        let sets: Vec<&Value> = plan["sets"]
            .as_array()
            .ok_or("the plan needs sets")?
            .iter()
            .filter(|s| only.is_empty() || s["name"].as_str().is_some_and(|n| only.iter().any(|o| o == n)))
            .collect();
        let mut reports = Vec::new();
        let (mut total, mut matched) = (0u64, 0u64);
        let mut clean = true;
        for set in sets {
            let run = if set.get("session").is_some() { run_session_set } else { run_set };
            let report = match run(&exe, &bundle, set, &vars, &fixture_dir, &out) {
                Ok(report) => report,
                Err(why) => json!({"name": set["name"], "error": why, "outcome": {"loaded": false}}),
            };
            let o = &report["outcome"];
            total += o["count"].as_u64().unwrap_or(0);
            matched += o["matched"].as_u64().unwrap_or(0);
            let set_clean = o["loaded"].as_bool() == Some(true)
                && o["mismatched"].as_u64() == Some(0)
                && o["refused"].as_u64() == Some(0)
                && o["missing"].as_u64() == Some(0);
            clean &= set_clean;
            println!(
                "ONECALC XLL-ACCEPT {}: loaded {} matched {}/{} mismatched {} refused {} missing {} unload {} reopen {}",
                report["name"].as_str().unwrap_or("?"),
                o["loaded"],
                o["matched"],
                o["count"],
                o["mismatched"],
                o["refused"],
                o["missing"],
                o["unload"]["unloaded"],
                o["reopen"]["loaded_after_right_hash"],
            );
            reports.push(report);
        }
        let summary = json!({"sets": reports, "cases": total, "matched": matched});
        fs::write(out.join("summary.json"), serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        println!("ONECALC XLL-ACCEPT total: matched {matched} of {total} cases; summary {}", out.join("summary.json").display());
        Ok((summary, clean))
    })();
    match result {
        Ok((_, clean)) if clean || allow_unmatched => ExitCode::SUCCESS,
        Ok(_) => ExitCode::from(1),
        Err(e) => {
            eprintln!("xtask app xll-accept: {e}");
            ExitCode::from(1)
        }
    }
}
