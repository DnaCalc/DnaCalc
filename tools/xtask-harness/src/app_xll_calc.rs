//! `cargo xtask app xll-accept calc --exe <dna-calc-desktop.exe> --bundle <dir> --plan <plan.json>
//! --out <dir> [--var NAME=VALUE]... [--set <name>]... [--allow-unmatched]`: DNA Calc's twin of the
//! OneCalc add-in acceptance (dc-0f1k.23, dc-hsa5.12), driven through DNA Calc's page.
//!
//! For each set of the plan it starts the desktop exe (`--app`, its WebView2 on a DevTools port),
//! attaches chromedriver to that WebView2 as the OneCalc run does, and does what a person does:
//! - an add-in set (`xll`, an .xll or a VBA .xlam): opens the page's Add-ins panel, types the
//!   path, presses Load path, waits for the panel to list it with its functions;
//! - a workbook set (`workbook`, an .xlsm): the exe opens the file as a double-click does
//!   (`--open`), and with `macros: "enable"` (the default; the level pinned to Off) the run presses
//!   the page's "Enable macros" button, with `macros: "unguarded"` the level is pinned to Unguarded
//!   and the project runs as the file opens;
//! then, for each case, types the address into the name box, types the formula into the formula
//! bar and presses Enter, goes back to the cell and reads what the page shows of it: its value
//! state (`result-state`: Calculated, Worksheet error, Refused...), its value (`selected-value`
//! with its kind, and `exact-value` where the shown text rounds a number). Every case is entered
//! in the same cell, so each answer is that formula's alone, as in the Excel runs.
//!
//! Excel's answers (`excel`): a controller record (`controller.jsonl` of the owned-session Excel
//! runs: `operation`, `value`) or a reopen record (`vba-job.ps1 -Action reopen`: Excel calculating
//! each formula inside the same .xlsm). Pass = matched Excel; mismatched, refused (the page shows
//! no calculated value) and missing (Excel gave no answer) are failures, over all cases. An add-in
//! set ends by unloading every add-in through the panel and records what the panel said.
//!
//! Exit code: 0 when every case of every selected set matched, 1 otherwise, 2 for a usage error.

use super::xll::{CASE_WAIT, LOAD_WAIT, PAGE_WAIT, Shown, attach_page, excel_value, expand, read_json, wait_until};
use super::*;
use std::collections::BTreeMap;

pub const USAGE: &str = "usage: cargo xtask app xll-accept calc --exe <dna-calc-desktop.exe> --bundle <dir> --plan <plan.json> --out <dir> [--var NAME=VALUE]... [--set <name>]... [--allow-unmatched]";

fn css(role: &str) -> String {
    if role.starts_with('[') {
        role.to_owned()
    } else {
        format!("[data-dnacalc=\"{role}\"]")
    }
}

fn text(session: &Session, role: &str) -> String {
    let quoted = serde_json::to_string(&css(role)).unwrap_or_default();
    match session.script(&format!(
        "var e = document.querySelector({quoted}); return e ? e.textContent : null;"
    )) {
        Ok(Value::String(text)) => text,
        _ => String::new(),
    }
}

fn items(session: &Session, role: &str) -> Vec<String> {
    let quoted = serde_json::to_string(&css(role)).unwrap_or_default();
    match session.script(&format!(
        "return Array.from(document.querySelectorAll({quoted})).map(function(e){{return e.textContent;}});"
    )) {
        Ok(Value::Array(items)) => items
            .iter()
            .filter_map(|i| i.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

fn press(session: &Session, role: &str) -> Result<(), String> {
    let element = session.element(&css(role))?;
    session
        .post(&format!("/element/{element}/click"), json!({}))
        .map(|_| ())
}

/// Sets a text field as typing would leave it (one input event).
fn type_into(session: &Session, role: &str, value: &str) -> Result<(), String> {
    let quoted = serde_json::to_string(&css(role)).unwrap_or_default();
    let value = serde_json::to_string(value).map_err(|e| e.to_string())?;
    session
        .script(&format!(
            "var e = document.querySelector({quoted}); e.focus(); e.value = {value}; e.dispatchEvent(new Event('input', {{bubbles: true}})); return true;"
        ))
        .map(|_| ())
}

/// Excel's answer for each formula: a controller record (one JSON object per line, `operation`
/// and `value`) or a reopen record (`rows`, each `formula` and Excel's `excel` value).
fn answers(path: &Path) -> Result<BTreeMap<String, Shown>, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim_start_matches('\u{feff}');
    let mut out = BTreeMap::new();
    if let Ok(record) = serde_json::from_str::<Value>(text)
        && let Some(rows) = record["rows"].as_array()
    {
        for row in rows {
            if let (Some(formula), Some(value)) = (row["formula"].as_str(), excel_value(&row["excel"])) {
                out.insert(formula.to_owned(), value);
            }
        }
        return Ok(out);
    }
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let record: Value = serde_json::from_str(line.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if let (Some(operation), Some(value)) = (
            record["operation"].as_str(),
            record.get("value").and_then(Shown::of_json),
        ) {
            out.insert(operation.to_owned(), value);
        }
    }
    Ok(out)
}

/// What the page shows of the active cell: its value state, its value as a [`Shown`] (from the
/// shown text, its kind and the exact value of a rounded number) and the raw texts.
struct Read {
    state: String,
    shown: Option<Shown>,
    detail: String,
}

fn read_active(session: &Session) -> Read {
    let raw = session
        .script(
            "var v = document.querySelector('[data-dnacalc=\"selected-value\"]'), x = document.querySelector('[data-dnacalc=\"exact-value\"]'), s = document.querySelector('[data-dnacalc=\"result-state\"]'), f = document.querySelector('[data-onecalc=\"formula\"]'), o = document.querySelector('[data-dnacalc=\"refusal-owner\"]'), p = document.querySelector('[data-dnacalc=\"cell-provenance\"]');\
             return {state: s ? s.textContent : '', text: v ? v.textContent : '', kind: v ? (v.getAttribute('data-kind') || '') : '', exact: x && !x.hidden ? x.textContent : '', input: f ? f.value : '', owner: o ? o.textContent : '', why: p ? p.textContent : ''};",
        )
        .unwrap_or(Value::Null);
    let field = |name: &str| raw[name].as_str().unwrap_or("").to_owned();
    let (state, shown_text, kind) = (field("state"), field("text"), field("kind"));
    let exact = field("exact").trim_start_matches("exactly ").trim().to_owned();
    let number = |t: &str| t.trim().parse::<f64>().ok();
    let shown = match state.as_str() {
        "Calculated" | "Worksheet error" | "Literal" => Some(match kind.as_str() {
            "number" => match number(&exact).or_else(|| number(&shown_text)) {
                Some(n) => Shown::Number(n),
                None => Shown::Text(shown_text.clone()),
            },
            "logical" => Shown::Logical(shown_text == "TRUE"),
            "error" => Shown::Error(shown_text.clone()),
            "text" => Shown::Text(shown_text.clone()),
            "" if shown_text.is_empty() => Shown::Blank,
            _ => match number(&exact) {
                Some(n) => Shown::Number(n),
                None => Shown::of_cell(&shown_text),
            },
        }),
        "Empty" => Some(Shown::Blank),
        _ => None,
    };
    Read {
        detail: format!(
            "{state}; {kind} {shown_text:?}{}; input {:?}{}{}",
            if exact.is_empty() { String::new() } else { format!(" exactly {exact}") },
            field("input"),
            if field("owner").is_empty() { String::new() } else { format!("; {}", field("owner")) },
            if field("why").is_empty() { String::new() } else { format!("; {}", field("why")) },
        ),
        state,
        shown,
    }
}

/// Moves the active cell to `cell` through the name box and waits for the page to follow.
fn go_to(session: &Session, cell: &str) -> Result<(), String> {
    // As a person does: select what the name box holds, type the address, press Enter.
    let name_box = css("name-box");
    session.keys(&name_box, &format!("\u{E009}a\u{E000}{cell}\u{E007}"))?;
    let at = |session: &Session| {
        session
            .script("var e = document.querySelector('[data-dnacalc=\"name-box\"]'); return e ? e.value : null;")
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .is_some_and(|v| v.eq_ignore_ascii_case(cell))
    };
    if !wait_until(PAGE_WAIT, || at(session)) {
        return Err(format!("the name box did not move to {cell}"));
    }
    Ok(())
}

/// Enters `formula` in `cell` through the formula bar (Enter commits and moves down), goes back to
/// the cell and waits until the page shows a settled value for this entry.
fn evaluate(session: &Session, cell: &str, formula: &str, previous: &str) -> Result<Read, String> {
    go_to(session, cell)?;
    let bar = "[data-onecalc=\"formula\"]";
    session.clear(bar)?;
    session.keys(bar, &format!("{formula}\u{E007}"))?;
    // Enter moved the active cell down; the entry is in when the cell is left.
    if !wait_until(PAGE_WAIT, || {
        session
            .script("var e = document.querySelector('[data-dnacalc=\"name-box\"]'); return e ? e.value : null;")
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .is_some_and(|v| !v.eq_ignore_ascii_case(cell))
    }) {
        // Leave the refused entry, so the next case starts from the grid.
        let _ = session.keys(bar, "\u{E00C}");
        return Err(format!(
            "the formula bar did not commit the entry: {}",
            text(session, "feedback")
        ));
    }
    go_to(session, cell)?;
    let input = |session: &Session| {
        session
            .script("var f = document.querySelector('[data-onecalc=\"formula\"]'); return f ? f.value : '';")
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default()
    };
    let settled = ["Calculated", "Worksheet error", "Refused", "Literal", "Empty"];
    let started = Instant::now();
    let mut last = (String::new(), String::new());
    let mut stable_since = Instant::now();
    loop {
        let now = input(session);
        let read = read_active(session);
        let seen = (now.clone(), read.detail.clone());
        if seen != last {
            last = seen;
            stable_since = Instant::now();
        }
        let fresh = !now.is_empty() && now != previous;
        if fresh
            && settled.contains(&read.state.as_str())
            && stable_since.elapsed() >= Duration::from_millis(250)
        {
            return Ok(read);
        }
        if started.elapsed() > CASE_WAIT {
            return Ok(read);
        }
        thread::sleep(Duration::from_millis(40));
    }
}

fn current_input(session: &Session) -> String {
    session
        .script("var f = document.querySelector('[data-onecalc=\"formula\"]'); return f ? f.value : '';")
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// The desktop exe in app mode with chromedriver attached to its WebView2.
fn start(
    exe: &Path,
    bundle: &Path,
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
    let app = DriverProcess {
        child,
        port: upstream,
    };
    let started = Instant::now();
    while TcpStream::connect(("127.0.0.1", upstream)).is_err() {
        if started.elapsed() > Duration::from_secs(60) {
            return Err("the desktop exe did not open its DevTools port".to_owned());
        }
        thread::sleep(Duration::from_millis(200));
    }
    let (driver, session) = attach_page(upstream, out)?;
    Ok((app, driver, session))
}

/// Loads `path` through the page's Add-ins panel: typed path, Load path. Whether the panel listed
/// it, and what its status said.
fn load_through_panel(session: &Session, path: &Path) -> Result<(bool, String), String> {
    session.script("var d = document.querySelector('[data-dnacalc=\"addin-menu\"]'); if (!d.open) { d.querySelector('summary').click(); } return d.open;")?;
    let before = items(session, "addin-loaded").len();
    type_into(session, "addin-path", &path.display().to_string())?;
    press(session, "addin-load-path")?;
    let came = wait_until(LOAD_WAIT, || {
        items(session, "addin-loaded").len() > before || {
            let status = text(session, "addin-status");
            !status.is_empty() && status != "Working…"
        }
    });
    let status = text(session, "addin-status");
    Ok((came && items(session, "addin-loaded").len() > before, status))
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
    let cell = set["cell"].as_str().unwrap_or("H1");
    let path_of = |key: &str| -> Option<PathBuf> {
        let text = expand(set[key].as_str()?, vars);
        let path = PathBuf::from(&text);
        Some(if path.is_absolute() { path } else { fixture_dir.join(path) })
    };
    let cases_path = path_of("cases").ok_or("a set needs cases")?;
    let excel_path = path_of("excel").ok_or("a set needs an excel record")?;
    let cases = read_json(&cases_path)?;
    let cases = cases.as_array().ok_or("cases are an array")?;
    let excel = answers(&excel_path)?;
    let (addin, workbook) = (path_of("xll"), path_of("workbook"));
    let macros = set["macros"].as_str().unwrap_or("enable");
    let mut extra = Vec::new();
    if let Some(file) = &workbook {
        extra.extend(["--open".to_owned(), file.display().to_string()]);
        extra.extend([
            "--macros".to_owned(),
            if macros == "unguarded" { "unguarded" } else { "off" }.to_owned(),
        ]);
    }
    let (_app, _driver, session) = start(exe, bundle, name, out, &extra)?;
    let outcome = (|| -> Result<Value, String> {
        if !wait_until(PAGE_WAIT, || text(&session, "addin-count").contains("loaded")) {
            return Err("the page did not mount its add-in panel".to_owned());
        }
        let mut opened = json!(null);
        if let Some(file) = &workbook {
            let file_name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if !wait_until(PAGE_WAIT, || text(&session, "document").contains(&file_name)) {
                return Err(format!("the page did not open {file_name}: {}", text(&session, "document")));
            }
            let bar_before = text(&session, "macro-status");
            if macros == "enable" {
                press(&session, "enable-macros")?;
                if !wait_until(PAGE_WAIT, || text(&session, "macro-status").is_empty()) {
                    return Ok(json!({"loaded": false, "status": text(&session, "macro-status"), "feedback": text(&session, "feedback")}));
                }
            }
            opened = json!({"file": file.display().to_string(), "macro_bar_before": bar_before, "macro_bar_after": text(&session, "macro-status"), "feedback": text(&session, "feedback"), "level": text(&session, "macro-security-label")});
        }
        // The sheet named as the Excel workbook the answers were captured in named it, through
        // the page's rename control, and the inputs the Excel runs had (X1: A1 = 7).
        if let Some(sheet) = set["sheet"].as_str() {
            type_into(&session, "sheet-rename-name", sheet)?;
            session.script(
                "document.querySelector('[data-dnacalc=\"sheet-rename\"]').click(); return true;",
            )?;
            if !wait_until(PAGE_WAIT, || {
                items(&session, "sheet-tab").iter().any(|t| t.trim() == sheet)
            }) {
                return Err(format!(
                    "the page did not rename the sheet to {sheet}: {:?}",
                    items(&session, "sheet-tab")
                ));
            }
        }
        for input in set["inputs"].as_array().into_iter().flatten() {
            let (Some(at), Some(entry)) = (input[0].as_str(), input[1].as_str()) else {
                return Err("an input is [cell, text]".to_owned());
            };
            go_to(&session, at)?;
            let bar = "[data-onecalc=\"formula\"]";
            session.clear(bar)?;
            session.keys(bar, &format!("{entry}\u{E007}"))?;
        }
        let mut listed = Vec::new();
        let mut status = String::new();
        if let Some(path) = &addin {
            // Companion add-ins first (the native owner beside the Excel-DNA ones, as in the
            // OneCalc run), then the one under test, each through the panel.
            let also: Vec<PathBuf> = set["also"]
                .as_array()
                .map(|list| {
                    list.iter()
                        .filter_map(Value::as_str)
                        .map(|p| PathBuf::from(expand(p, vars)))
                        .collect()
                })
                .unwrap_or_default();
            for path in also.iter().chain(std::iter::once(path)) {
                let (loaded, said) = load_through_panel(&session, path)?;
                status = said;
                listed = items(&session, "addin-loaded");
                if !loaded {
                    return Ok(json!({"loaded": false, "status": status, "listed": listed}));
                }
            }
        }
        let mut results = Vec::new();
        let (mut matched, mut mismatched, mut refused, mut missing) = (0, 0, 0, 0);
        let mut previous = current_input(&session);
        for case in cases {
            let formula = case["formula"].as_str().ok_or("a case needs a formula")?;
            let theirs = excel.get(formula);
            let case_started = Instant::now();
            let page = evaluate(&session, cell, formula, &previous);
            let seconds = case_started.elapsed().as_secs_f64();
            previous = current_input(&session);
            let (verdict, detail) = match (&page, theirs) {
                (Err(why), _) => ("refused", why.clone()),
                (Ok(read), _) if read.shown.is_none() => ("refused", read.detail.clone()),
                (Ok(_), None) => ("missing", "Excel gave no answer for this formula".to_owned()),
                (Ok(read), Some(excel_value)) => match &read.shown {
                    Some(shown) if shown.same(excel_value) => ("matched", read.detail.clone()),
                    _ => (
                        "mismatched",
                        format!("{}; Excel {}", read.detail, excel_value.describe()),
                    ),
                },
            };
            match verdict {
                "matched" => matched += 1,
                "mismatched" => mismatched += 1,
                "refused" => refused += 1,
                _ => missing += 1,
            }
            println!("  {verdict:<10} {formula:<44} {detail}");
            results.push(json!({
                "formula": formula, "verdict": verdict, "detail": detail,
                "excel": theirs.map(Shown::describe), "seconds": seconds,
            }));
        }
        let unload = if addin.is_some() {
            let mut statuses = Vec::new();
            let mut unloaded = true;
            loop {
                let before = items(&session, "addin-loaded").len();
                if before == 0 {
                    break;
                }
                session.script("var b = document.querySelectorAll('[data-dnacalc=\"addin-unload\"]'); b[b.length - 1].click(); return true;")?;
                let went = wait_until(Duration::from_secs(60), || items(&session, "addin-loaded").len() < before);
                statuses.push(text(&session, "addin-status"));
                if !went {
                    unloaded = false;
                    break;
                }
            }
            json!({"unloaded": unloaded, "status": statuses})
        } else {
            json!(null)
        };
        Ok(json!({
            "loaded": true, "status": status, "listed": listed, "opened": opened,
            "count": cases.len(), "matched": matched, "mismatched": mismatched, "refused": refused,
            "missing": missing, "unload": unload, "results": results,
        }))
    })();
    session.end();
    let outcome = outcome?;
    Ok(json!({
        "name": name, "xll": addin.map(|p| p.display().to_string()), "workbook": workbook.map(|p| p.display().to_string()),
        "macros": macros, "cell": cell, "cases_file": cases_path.display().to_string(),
        "excel_record": excel_path.display().to_string(), "outcome": outcome,
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
    vars.entry("fixture".to_owned())
        .or_insert_with(|| fixture_dir.display().to_string());
    vars.entry("root".to_owned())
        .or_insert_with(|| ws.root.display().to_string());
    let result = (|| -> Result<bool, String> {
        fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        let plan = read_json(&plan)?;
        let sets: Vec<&Value> = plan["sets"]
            .as_array()
            .ok_or("the plan needs sets")?
            .iter()
            .filter(|s| {
                only.is_empty() || s["name"].as_str().is_some_and(|n| only.iter().any(|o| o == n))
            })
            .collect();
        let mut reports = Vec::new();
        let (mut total, mut matched) = (0u64, 0u64);
        let mut clean = true;
        for set in sets {
            println!("DNACALC XLL-ACCEPT {} ...", set["name"].as_str().unwrap_or("?"));
            let report = match run_set(&exe, &bundle, set, &vars, &fixture_dir, &out) {
                Ok(report) => report,
                Err(why) => json!({"name": set["name"], "error": why, "outcome": {"loaded": false}}),
            };
            let o = &report["outcome"];
            total += o["count"].as_u64().unwrap_or(0);
            matched += o["matched"].as_u64().unwrap_or(0);
            clean &= o["loaded"].as_bool() == Some(true)
                && o["mismatched"].as_u64() == Some(0)
                && o["refused"].as_u64() == Some(0)
                && o["missing"].as_u64() == Some(0);
            println!(
                "DNACALC XLL-ACCEPT {}: loaded {} matched {}/{} mismatched {} refused {} missing {} unload {} {}",
                report["name"].as_str().unwrap_or("?"),
                o["loaded"],
                o["matched"],
                o["count"],
                o["mismatched"],
                o["refused"],
                o["missing"],
                o["unload"]["unloaded"],
                report["error"].as_str().unwrap_or(""),
            );
            reports.push(report);
        }
        let summary = json!({"sets": reports, "cases": total, "matched": matched});
        fs::write(
            out.join("summary.json"),
            serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        println!(
            "DNACALC XLL-ACCEPT total: matched {matched} of {total} cases; summary {}",
            out.join("summary.json").display()
        );
        Ok(clean)
    })();
    match result {
        Ok(clean) if clean || allow_unmatched => ExitCode::SUCCESS,
        Ok(_) => ExitCode::from(1),
        Err(e) => {
            eprintln!("xtask app xll-accept calc: {e}");
            ExitCode::from(1)
        }
    }
}
