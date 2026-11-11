//! The VBA validation corpus (docs/plan/VBA_VALIDATION.md): every project under
//! `crates/calc-vba/tests/corpus/` is loaded the way OneCalc's "VBA functions" panel loads a
//! standard module (`Session::load_vba_module`, CalcVBA through calc-vba) and each recorded Excel
//! case is calculated in the session's formula cell. No Excel and no Windows are needed.
//!
//! A case counts as matched when the session's value equals Excel's recorded answer; mismatched
//! when a value came out and differs; refused when no value came out (the project did not load, a
//! typed refusal, a fault); missing when the corpus has no Excel answer for it. Pass = matched;
//! fail = mismatched + refused + missing, over all cases (AGENTS.md honesty rule 2).
//!
//! `corpus/baseline.json` holds the counts the last reviewed run produced; the test fails when a
//! project's counts move in either direction, so an improvement is recorded and a regression is
//! caught. `DNACALC_VBA_CORPUS_REPORT=<file>` writes the per-case report (JSON);
//! `DNACALC_VBA_CORPUS_UPDATE=1` rewrites the baseline from this run.
//!
//! Native-only: reads the corpus files from the checkout and uses a large stack.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dna_onecalc::{Session, display};
use serde_json::{Value, json};

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/calc-vba/tests/corpus")
}

/// Windows-1252 text as the VBE exports it (the bytes the Excel job imports).
fn read_source(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    match String::from_utf8(bytes.clone()) {
        Ok(text) => text,
        Err(_) => bytes.iter().map(|b| char::from(*b)).collect(),
    }
}

fn excel_text(case: &Value) -> Option<String> {
    if case.get("excel_error").is_some() {
        return None;
    }
    Some(match &case["value"] {
        Value::Number(n) => format!("number {}", n.as_f64().unwrap()),
        Value::String(s) => format!("text \"{}\"", s.replace('"', "\"\"")),
        Value::Bool(true) => "logical TRUE".to_owned(),
        Value::Bool(false) => "logical FALSE".to_owned(),
        Value::Object(o) => format!("error {}", o["error"].as_str().unwrap()),
        Value::Null => "blank".to_owned(),
        other => panic!("{other}"),
    })
}

/// One project: its counts and one line per case that did not match.
struct Outcome {
    route: String,
    loaded: Result<(), String>,
    matched: u32,
    mismatched: u32,
    refused: u32,
    missing: u32,
    details: Vec<Value>,
}

fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or("");
    line.chars().take(240).collect()
}

/// How a project reaches the engine: OneCalc's "VBA functions" route (one standard module,
/// `Session::load_vba_module`) on an explicit backend, the source-project route (every module of
/// the project, classes included, `calc_vba::events::load_project`, for a project of several
/// modules) or OneCalc's file route (an .xlsm of the corpus opened as a session, macros enabled;
/// the engine picks the backend).
#[derive(Clone, Copy, PartialEq)]
enum Route {
    Module(Option<calc_vba::Backend>),
    Project(calc_vba::Backend),
    Xlsm,
}

fn route_name(route: Route) -> &'static str {
    match route {
        Route::Module(Some(calc_vba::Backend::Interpreter)) => "module/interpreter",
        Route::Module(Some(calc_vba::Backend::Jit)) => "module/jit",
        Route::Module(None) => "module",
        Route::Project(calc_vba::Backend::Interpreter) => "project/interpreter",
        Route::Project(calc_vba::Backend::Jit) => "project/jit",
        Route::Xlsm => "xlsm",
    }
}

fn run_project(dir: &Path, route: Route) -> Option<Outcome> {
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("project.json")).unwrap()).unwrap();
    let mut session = Session::new("Sheet1").unwrap();
    session.set_formula_cell("H1").unwrap();
    let loaded = match route {
        Route::Module(backend) => {
            let modules = manifest["modules"].as_array().unwrap();
            let primary = manifest["primary"].as_str();
            let module = modules
                .iter()
                .find(|m| primary.is_none_or(|p| m["name"] == p))
                .expect("a primary module");
            let name = module["name"].as_str().unwrap();
            let source = read_source(&dir.join(module["path"].as_str().unwrap()));
            match backend {
                None => session
                    .load_vba_module(name, &source, calc_vba::DEFAULT_STEP_BUDGET)
                    .map(|_| ()),
                Some(backend) => calc_vba::load_module_with_backend(
                    name,
                    &source,
                    calc_vba::DEFAULT_STEP_BUDGET,
                    backend,
                )
                .map(|functions| session.set_user_functions(functions)),
            }
            .map_err(|e| first_line(&e))
        }
        Route::Project(backend) => {
            let modules: Vec<calc_vba::events::ProjectModule> = manifest["modules"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| calc_vba::events::ProjectModule {
                    name: m["name"].as_str().unwrap().to_owned(),
                    source: read_source(&dir.join(m["path"].as_str().unwrap())),
                    kind: match m["kind"].as_str() {
                        Some("class") => calc_vba::events::ModuleKind::Class,
                        _ => calc_vba::events::ModuleKind::Standard,
                    },
                })
                .collect();
            calc_vba::events::load_project(&modules, calc_vba::DEFAULT_STEP_BUDGET, Some(backend))
                .map(|project| session.set_user_functions(project.functions()))
                .map_err(|e| first_line(&e))
        }
        Route::Xlsm => {
            let path = manifest["workbook"].as_str()?;
            let bytes = std::fs::read(dir.join(path)).unwrap();
            let name = Path::new(path).file_name().unwrap().to_string_lossy().into_owned();
            match dna_onecalc::file::open(&bytes) {
                Err(e) => Err(first_line(&format!("open: {e:?}"))),
                Ok((opened, vba)) => {
                    session = opened;
                    session.set_formula_cell("H1").unwrap();
                    match vba {
                        None => Err("the workbook holds no VBA project".to_owned()),
                        Some(vba) => session
                            .open_project(vba, &name, None)
                            .and_then(|_| session.enable_macros())
                            .map(|_| ())
                            .map_err(|e| first_line(&e)),
                    }
                }
            }
        }
    };
    let mut out = Outcome {
        route: route_name(route).to_owned(),
        loaded,
        matched: 0,
        mismatched: 0,
        refused: 0,
        missing: 0,
        details: Vec::new(),
    };
    let Some(cases) = manifest["cases"].as_str() else {
        return Some(out);
    };
    let cases: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(dir.join(cases)).unwrap()).unwrap();
    for case in &cases {
        let formula = case["formula"].as_str().unwrap();
        let Some(excel) = excel_text(case) else {
            out.missing += 1;
            out.details.push(json!({"formula": formula, "class": "missing"}));
            continue;
        };
        if let Err(reason) = &out.loaded {
            out.refused += 1;
            out.details.push(json!({"formula": formula, "class": "refused", "why": format!("project did not load: {reason}")}));
            continue;
        }
        session.set_formula(formula);
        let calculation = session.calculate();
        let why: Vec<String> = calculation
            .diagnostics
            .iter()
            .map(|d| first_line(&display::diagnostic(&session, d)))
            .collect();
        let Some(result) = calculation.result else {
            out.refused += 1;
            out.details.push(json!({"formula": formula, "class": "refused", "why": why.join(" | ")}));
            continue;
        };
        // A dynamic-array result: Excel's answer is the value of the formula cell, the first element.
        let shown = match result.as_array() {
            Some(array) => array
                .get(0, 0)
                .map(|v| display::value(&v))
                .unwrap_or_else(|| "empty array".to_owned()),
            None => display::value(&result),
        };
        if shown == excel {
            out.matched += 1;
        } else {
            out.mismatched += 1;
            out.details.push(json!({"formula": formula, "class": "mismatched", "excel": excel, "dna": shown, "diagnostics": why}));
        }
    }
    Some(out)
}

fn run_all() -> BTreeMap<String, Outcome> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(corpus_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("project.json").exists())
        .collect();
    dirs.sort();
    let mut all = BTreeMap::new();
    for dir in dirs {
        let id = dir.file_name().unwrap().to_string_lossy().into_owned();
        let manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("project.json")).unwrap()).unwrap();
        let mut routes = vec![Route::Module(Some(calc_vba::Backend::Interpreter)), Route::Module(Some(calc_vba::Backend::Jit))];
        if manifest["modules"].as_array().is_some_and(|m| m.len() > 1) {
            routes.push(Route::Project(calc_vba::Backend::Interpreter));
            routes.push(Route::Project(calc_vba::Backend::Jit));
        }
        if manifest["workbook"].is_string() {
            routes.push(Route::Xlsm);
        }
        for route in routes {
            // A project's VBA runs on a large stack, as the application's worker does.
            let runner = dir.clone();
            let outcome = std::thread::Builder::new()
                .stack_size(256 * 1024 * 1024)
                .spawn(move || run_project(&runner, route))
                .unwrap()
                .join()
                .unwrap_or_else(|_| panic!("{id}: the run panicked"));
            if let Some(outcome) = outcome {
                all.insert(format!("{id}/{}", route_name(route)), outcome);
            }
        }
    }
    all
}

#[test]
fn the_vba_corpus_runs_headless_and_its_counts_are_the_reviewed_ones() {
    let all = run_all();
    let mut counts = serde_json::Map::new();
    let mut report = serde_json::Map::new();
    // Figures are per route and backend, never one pooled number (a case is counted once per route).
    let mut by_route: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    for (id, o) in &all {
        println!(
            "{id:34} load={:5} matched {:3} mismatched {:3} refused {:3} missing {:3}",
            if o.loaded.is_ok() { "ok" } else { "ERR" },
            o.matched,
            o.mismatched,
            o.refused,
            o.missing
        );
        let total = by_route.entry(o.route.clone()).or_default();
        total.0 += o.matched;
        total.1 += o.mismatched + o.refused + o.missing;
        counts.insert(
            id.clone(),
            json!({"route": o.route, "loaded": o.loaded.is_ok(), "matched": o.matched, "mismatched": o.mismatched, "refused": o.refused, "missing": o.missing}),
        );
        report.insert(
            id.clone(),
            json!({"load_error": o.loaded.clone().err(), "cases": o.details}),
        );
    }
    for (route, (pass, fail)) in &by_route {
        println!("{route:20} pass {pass}, fail {fail} (mismatched + refused + missing), cases {}", pass + fail);
    }
    if let Ok(path) = std::env::var("DNACALC_VBA_CORPUS_REPORT") {
        std::fs::write(path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    }
    let baseline_path = corpus_dir().join("baseline.json");
    let current = Value::Object(counts);
    if std::env::var("DNACALC_VBA_CORPUS_UPDATE").is_ok() {
        std::fs::write(&baseline_path, serde_json::to_string_pretty(&current).unwrap() + "\n").unwrap();
        return;
    }
    let baseline: Value = serde_json::from_str(&std::fs::read_to_string(&baseline_path).unwrap()).unwrap();
    assert_eq!(
        current, baseline,
        "the corpus counts moved; review the per-case report (DNACALC_VBA_CORPUS_REPORT) and \
         record the new counts with DNACALC_VBA_CORPUS_UPDATE=1"
    );
}
