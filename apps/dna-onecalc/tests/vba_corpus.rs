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
//! Shared counts live in `common`; `windows` overrides only the routes that differ there.
//!
//! Capability policy (dc-10un): every project loads and runs under CalcVBA's capability policy,
//! `Safe` unless `corpus/policy-approvals.json` approves that exact project (its id and module
//! hashes) for `Full` after review of its triage. Under `Safe` a host-reaching operation (files,
//! processes, environment, settings, `Declare`, COM activation, `SendKeys`) raises a VBA error
//! instead of running, and the audit log is drained per case: a case with any denied request is
//! classed `refused (policy: <capability>)` and counts as refused, never as matched or
//! mismatched. Each project also gets a predicted set P (CalcVBA triage, from bound OxIR for the
//! module route, lexical for the others) and the observed set O (what the audit log saw,
//! allowed or denied); `O minus P` must be empty, `P minus O` is unconfirmed, not wrong.
//!
//! Native-only: reads the corpus files from the checkout and uses a large stack.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use calc_vba::policy::{self, Capability, CapabilitySet, PolicyMode, Prediction};
use dna_onecalc::{Session, display};
use serde_json::{Value, json};

#[path = "../../../crates/calc-doc/tests/cdoc_m1_1_support/mod.rs"]
#[allow(dead_code)]
mod support;
#[path = "../../../crates/calc-doc/tests/cdoc_m1_1_support/vba_project.rs"]
mod vba_project;

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/calc-vba/tests/corpus")
}

/// Windows-1252 text as the VBE exports it (the bytes the Excel job imports).
fn decode_source(bytes: &[u8]) -> String {
    match String::from_utf8(bytes.to_vec()) {
        Ok(text) => text,
        Err(_) => bytes.iter().map(|b| char::from(*b)).collect(),
    }
}

fn excel_text(case: &Value, route: Route) -> Option<String> {
    if case.get("excel_error").is_some() {
        return None;
    }
    // A file project can differ from the same add-in (ThisWorkbook.Name). Keep
    // both recorded answers in the existing cases, selected by execution route.
    let value = if route == Route::Xlsm {
        case.get("xlsm_value")
    } else {
        None
    }
    .unwrap_or(&case["value"]);
    Some(match value {
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
    /// Of `refused`: cases with a denied policy request (a subset, not an extra class).
    policy_denied: u32,
    missing: u32,
    details: Vec<Value>,
    mode: PolicyMode,
    predicted: CapabilitySet,
    predicted_source: &'static str,
    observed: CapabilitySet,
    /// Cases run after the first denial: module state a denial left can change them.
    after_first_denial: u32,
}

/// The policy approvals: a project runs `Full` only when its id and module hashes are listed.
struct ExecutionInputs {
    project: Vec<u8>,
    modules: Vec<Vec<u8>>,
    workbook: Option<Vec<u8>>,
}

fn approval_matches(
    a: &Value,
    id: &str,
    _manifest: &Value,
    route: Route,
    inputs: &ExecutionInputs,
) -> bool {
    use calc_verdict::sha256_hex as hash;
    a["policy"] == "full"
        && a["project"] == id
        && a["route"] == route_name(route)
        && a["project_sha256"] == hash(&inputs.project)
        && a["module_sha256"].as_array().is_some_and(|hashes| {
            hashes.iter().eq(inputs
                .modules
                .iter()
                .map(|bytes| Value::String(hash(bytes)))
                .collect::<Vec<_>>()
                .iter())
        })
        && match route {
            Route::Xlsm => inputs
                .workbook
                .as_ref()
                .is_some_and(|bytes| a["workbook_sha256"] == hash(bytes)),
            _ => true,
        }
}

fn approved_full(id: &str, manifest: &Value, route: Route, inputs: &ExecutionInputs) -> bool {
    let path = corpus_dir().join("policy-approvals.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return false;
    };
    let approvals: Value = serde_json::from_str(&text).unwrap();
    approvals["approvals"].as_array().is_some_and(|list| {
        list.iter()
            .any(|a| approval_matches(a, id, manifest, route, inputs))
    })
}

/// Native-only: validates the filesystem corpus runner input snapshots.
#[test]
fn full_approval_is_bound_to_route_project_and_executed_bytes() {
    use calc_verdict::sha256_hex as hash;
    let module = b"Public Function F(): F = 1: End Function".to_vec();
    let manifest = json!({"modules": [{"name":"Api", "path":"Api.bas", "sha256":hash(&module)}]});
    let mut inputs = ExecutionInputs {
        project: serde_json::to_vec(&manifest).unwrap(),
        modules: vec![module],
        workbook: Some(b"workbook".to_vec()),
    };
    let route = Route::Xlsm;
    let approval = json!({"project":"fixture", "policy":"full", "route":"xlsm", "project_sha256":hash(&inputs.project), "module_sha256":[hash(&inputs.modules[0])], "workbook_sha256":hash(inputs.workbook.as_ref().unwrap())});
    assert!(approval_matches(
        &approval, "fixture", &manifest, route, &inputs
    ));
    inputs.modules[0].push(b'\n');
    assert!(
        !approval_matches(&approval, "fixture", &manifest, route, &inputs),
        "changed module bytes"
    );
    inputs.modules[0].pop();
    inputs.workbook.as_mut().unwrap().push(1);
    assert!(
        !approval_matches(&approval, "fixture", &manifest, route, &inputs),
        "changed workbook bytes"
    );
    inputs.workbook.as_mut().unwrap().pop();
    assert!(
        !approval_matches(
            &approval,
            "fixture",
            &manifest,
            Route::Module(None),
            &inputs
        ),
        "another route"
    );
    inputs.project.push(b' ');
    assert!(
        !approval_matches(&approval, "fixture", &manifest, route, &inputs),
        "changed project metadata"
    );
}

/// Native-only: writes a temporary project and executes it on an owning native thread.
#[test]
fn a_swallowed_denial_is_one_refusal_even_when_the_value_matches_excel() {
    std::thread::Builder::new().stack_size(256 * 1024 * 1024).spawn(|| {
        let dir = std::env::temp_dir().join(format!("vba-policy-scoring-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Api.bas"), "Public Function Probe() As Long\n On Error Resume Next\n Dim x As String\n x = Environ(\"UNUSED_POLICY_TEST\")\n Probe = 42\nEnd Function\n").unwrap();
        std::fs::write(dir.join("project.json"), serde_json::to_vec(&json!({"primary":"Api", "modules":[{"name":"Api", "path":"Api.bas"}], "cases":"cases.json"})).unwrap()).unwrap();
        std::fs::write(dir.join("cases.json"), r#"[{"formula":"=Probe()","value":42}]"#).unwrap();
        let out = run_project(&dir, Route::Module(Some(calc_vba::Backend::Interpreter))).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(out.loaded.is_ok(), "{:?}", out.loaded);
        assert_eq!((out.matched, out.mismatched, out.refused, out.policy_denied), (0, 0, 1, 1));
        assert_eq!(out.details.len(), 1);
        assert_eq!(out.details[0]["class"], "refused");
    }).unwrap().join().unwrap();
}

/// Native-only: modifies an existing test package and runs its Workbook_Open on an owning thread.
#[test]
fn a_swallowed_initialization_denial_cannot_produce_matched_cases() {
    std::thread::Builder::new().stack_size(256 * 1024 * 1024).spawn(|| {
        let dir = std::env::temp_dir().join(format!("vba-policy-init-scoring-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let api = "Public Function Probe() As Long\n Probe = 42\nEnd Function\n";
        let book = "Private Sub Workbook_Open()\n On Error Resume Next\n Dim o As Object\n Set o = CreateObject(\"WinHttp.WinHttpRequest.5.1\")\nEnd Sub\n";
        let bytes = vba_project::replace_sources(include_bytes!("../../../crates/calc-doc/tests/fixtures/vba_udf_names/local.xlsm"), &[("Module1", api), ("PrivateModule", "Option Private Module\n"), ("Sheet1", ""), ("ThisWorkbook", book)]);
        std::fs::write(dir.join("fixture.xlsm"), vba_project::minimal_control(&bytes)).unwrap();
        std::fs::write(dir.join("Api.bas"), api).unwrap();
        std::fs::write(dir.join("Book.cls"), book).unwrap();
        std::fs::write(dir.join("project.json"), serde_json::to_vec(&json!({"modules":[{"name":"Module1", "path":"Api.bas"},{"name":"ThisWorkbook", "path":"Book.cls"}], "workbook":"fixture.xlsm", "cases":"cases.json"})).unwrap()).unwrap();
        std::fs::write(dir.join("cases.json"), r#"[{"formula":"=Probe()","value":42}]"#).unwrap();
        let out = run_project(&dir, Route::Xlsm).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(out.loaded.is_ok(), "{:?}", out.loaded);
        assert_eq!((out.matched, out.mismatched, out.refused, out.policy_denied), (0, 0, 1, 1));
    }).unwrap().join().unwrap();
}

fn capability_set(prediction: &Prediction) -> CapabilitySet {
    CapabilitySet::of(&prediction.capabilities.iter().copied().collect::<Vec<_>>())
}

fn names(set: CapabilitySet) -> Vec<&'static str> {
    set.iter().map(Capability::name).collect()
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

/// Source triage cannot see a file's references. Windows resolves their type libraries
/// during compilation, including stdole's metadata (its identity alone is portable).
fn predict_file_references(prediction: &mut Prediction, project: &calc_vba::OfficeProject) {
    let mut references = project.com_references();
    references.extend(project.references().iter().filter(|reference| {
        reference.guid().as_deref() == Some("00020430-0000-0000-C000-000000000046")
    }));
    for reference in references {
        prediction.capabilities.insert(Capability::ComActivate);
        prediction
            .details
            .entry(Capability::ComActivate)
            .or_default()
            .insert(format!("typelib {}", reference.name()));
    }
}

#[test]
fn file_reference_prediction_sees_ole_metadata_without_running_the_project() {
    for (name, bytes) in [
        (
            "DnaSimple.xlsm",
            include_bytes!("../../../crates/calc-xll-reference/fixture/vba/DnaSimple.xlsm")
                .as_slice(),
        ),
        (
            "DnaExamples.xlsm",
            include_bytes!("../../../crates/calc-xll-reference/fixture/vba/DnaExamples.xlsm")
                .as_slice(),
        ),
    ] {
        let (_, vba) = dna_onecalc::file::open(bytes).unwrap();
        let project = calc_vba::read_office_project(&vba.unwrap().project).unwrap();
        let mut prediction = policy::predict_lexical(&[]);
        assert!(prediction.capabilities.is_empty());
        predict_file_references(&mut prediction, &project);
        assert!(
            prediction.capabilities.contains(&Capability::ComActivate),
            "{name}"
        );
        assert!(
            prediction.details[&Capability::ComActivate].contains("typelib stdole"),
            "{name}"
        );
        assert_eq!(
            policy::drain_audit().total,
            0,
            "prediction must not resolve the references"
        );
        println!(
            "{name}: references {:?}; predicts native metadata access",
            project
                .references()
                .iter()
                .map(|r| r.name())
                .collect::<Vec<_>>()
        );
    }
}

fn platform_baseline(baseline: &Value, windows: bool) -> Value {
    let mut counts = baseline["common"]
        .as_object()
        .expect("shared corpus counts")
        .clone();
    if windows {
        counts.extend(
            baseline["windows"]
                .as_object()
                .expect("Windows overrides")
                .clone(),
        );
    }
    Value::Object(counts)
}

fn record_platform_baseline(baseline: &mut Value, current: Value, windows: bool) {
    if windows {
        baseline["windows"] = Value::Object(
            current
                .as_object()
                .unwrap()
                .iter()
                .filter(|(key, value)| baseline["common"].get(*key) != Some(*value))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        );
    } else {
        baseline["common"] = current;
    }
}

#[test]
fn recording_windows_counts_preserves_the_other_platform_baseline() {
    let mut baseline = json!({"common":{"a":1,"b":2},"windows":{"a":3}});
    assert_eq!(platform_baseline(&baseline, false), json!({"a":1,"b":2}));
    assert_eq!(platform_baseline(&baseline, true), json!({"a":3,"b":2}));
    record_platform_baseline(&mut baseline, json!({"a":4,"b":2}), true);
    assert_eq!(platform_baseline(&baseline, false), json!({"a":1,"b":2}));
    assert_eq!(platform_baseline(&baseline, true), json!({"a":4,"b":2}));
    assert_eq!(baseline["windows"], json!({"a":4}));
}

fn run_project(dir: &Path, route: Route) -> Option<Outcome> {
    // Read once, approve those bytes, then compile/open those same snapshots.
    let project = std::fs::read(dir.join("project.json")).unwrap();
    let manifest: Value = serde_json::from_slice(&project).unwrap();
    let modules = manifest["modules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| std::fs::read(dir.join(m["path"].as_str().unwrap())).unwrap())
        .collect();
    let workbook = if route == Route::Xlsm {
        Some(std::fs::read(dir.join(manifest["workbook"].as_str()?)).unwrap())
    } else {
        None
    };
    let inputs = ExecutionInputs {
        project,
        modules,
        workbook,
    };
    let id = dir.file_name().unwrap().to_string_lossy().into_owned();
    let mode = if approved_full(&id, &manifest, route, &inputs) {
        PolicyMode::Full
    } else {
        PolicyMode::Safe
    };
    policy::set_thread_policy(Some(mode));
    let sources: Vec<String> = inputs
        .modules
        .iter()
        .map(|bytes| decode_source(bytes))
        .collect();
    let (mut prediction, mut predicted_source) = match route {
        Route::Module(_) => {
            let modules = manifest["modules"].as_array().unwrap();
            let primary = manifest["primary"].as_str();
            let index = modules
                .iter()
                .position(|m| primary.is_none_or(|p| m["name"] == p))
                .unwrap();
            match calc_vba::predict_module(
                modules[index]["name"].as_str().unwrap(),
                &sources[index],
            ) {
                Ok(p) => (p, "bound"),
                Err(_) => (
                    policy::predict_lexical(&[sources[index].as_str()]),
                    "lexical",
                ),
            }
        }
        _ => (
            policy::predict_lexical(&sources.iter().map(String::as_str).collect::<Vec<_>>()),
            "lexical",
        ),
    };
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
            let index = modules.iter().position(|m| m == module).unwrap();
            let source = sources[index].clone();
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
                .enumerate()
                .map(|(index, m)| calc_vba::events::ProjectModule {
                    name: m["name"].as_str().unwrap().to_owned(),
                    source: sources[index].clone(),
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
            let bytes = inputs.workbook.as_ref().expect("approved input snapshot");
            let name = Path::new(path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            match dna_onecalc::file::open(&bytes) {
                Err(e) => Err(first_line(&format!("open: {e:?}"))),
                Ok((opened, vba)) => {
                    session = opened;
                    session.set_formula_cell("H1").unwrap();
                    match vba {
                        None => Err("the workbook holds no VBA project".to_owned()),
                        Some(vba) => {
                            if cfg!(windows)
                                && let Ok(project) = calc_vba::read_office_project(&vba.project)
                            {
                                predict_file_references(&mut prediction, &project);
                                predicted_source = "lexical+references";
                            }
                            session
                                .open_project(vba, &name, None)
                                .and_then(|_| session.enable_macros())
                                .map(|_| ())
                                .map_err(|e| first_line(&e))
                        }
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
        policy_denied: 0,
        missing: 0,
        details: Vec::new(),
        mode,
        predicted: capability_set(&prediction),
        predicted_source,
        observed: CapabilitySet::EMPTY,
        after_first_denial: 0,
    };
    // Whatever the load itself asked the host for (module-level initialisers).
    let load_audit = policy::drain_audit();
    out.observed = out.observed.union(load_audit.requested());
    let initialization_denied = load_audit.denied_set();
    let Some(cases) = manifest["cases"].as_str() else {
        return Some(out);
    };
    let cases: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(dir.join(cases)).unwrap()).unwrap();
    for case in &cases {
        let formula = case["formula"].as_str().unwrap();
        let Some(excel) = excel_text(case, route) else {
            out.missing += 1;
            out.details
                .push(json!({"formula": formula, "class": "missing"}));
            continue;
        };
        if !initialization_denied.is_empty() {
            out.refused += 1;
            out.policy_denied += 1;
            out.details.push(json!({"formula":formula, "class":"refused", "why":"policy denied during project initialization", "denied":names(initialization_denied)}));
            continue;
        }
        if let Err(reason) = &out.loaded {
            out.refused += 1;
            out.details.push(json!({"formula": formula, "class": "refused", "why": format!("project did not load: {reason}")}));
            continue;
        }
        session.set_formula(formula);
        let calculation = session.calculate();
        let audit = policy::drain_audit();
        out.observed = out.observed.union(audit.requested());
        if out.after_first_denial > 0 || audit.denied_any() {
            out.after_first_denial += 1;
        }
        if audit.denied_any() {
            out.policy_denied += 1;
            out.refused += 1;
            out.details.push(json!({"formula":formula, "class":"refused", "why":"policy denied", "denied":names(audit.denied_set())}));
            continue;
        }
        let why: Vec<String> = calculation
            .diagnostics
            .iter()
            .map(|d| first_line(&display::diagnostic(&session, d)))
            .collect();
        let Some(result) = calculation.result else {
            out.refused += 1;
            out.details
                .push(json!({"formula": formula, "class": "refused", "why": why.join(" | ")}));
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
            serde_json::from_str(&std::fs::read_to_string(dir.join("project.json")).unwrap())
                .unwrap();
        let mut routes = vec![
            Route::Module(Some(calc_vba::Backend::Interpreter)),
            Route::Module(Some(calc_vba::Backend::Jit)),
        ];
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
fn an_unmodelled_entry_cannot_match_excels_value_error() {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(|| {
            let dir = corpus_dir().join("vba-json");
            for backend in [calc_vba::Backend::Interpreter, calc_vba::Backend::Jit] {
                let out = run_project(&dir, Route::Project(backend)).unwrap();
                let invalid = out
                    .details
                    .iter()
                    .find(|case| case["formula"] == "=JsonRoundTrip(\"{bad\")")
                    .expect("an unsupported entry is recorded even when Excel expects #VALUE!");
                assert_eq!(invalid["class"], "refused", "{backend:?}: {invalid}");
                assert!(
                    invalid["why"].as_str().unwrap().contains("dc-hsa5.23"),
                    "{invalid}"
                );
                assert_eq!(out.matched, 0, "an unlinked entry calculated no values");
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn the_vba_corpus_runs_headless_and_its_counts_are_the_reviewed_ones() {
    let all = run_all();
    let mut counts = serde_json::Map::new();
    let mut report = serde_json::Map::new();
    // Figures are per route and backend, never one pooled number (a case is counted once per route).
    let mut by_route: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    let mut triage_misses: Vec<String> = Vec::new();
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
        let missed = o.observed.difference(o.predicted);
        let unconfirmed = o.predicted.difference(o.observed);
        println!(
            "  policy {:4} P({}) {:?} O {:?} O-P {:?} P-O {:?} policy-denied {} cases-after-first-denial {}",
            o.mode.name(),
            o.predicted_source,
            names(o.predicted),
            names(o.observed),
            names(missed),
            names(unconfirmed),
            o.policy_denied,
            o.after_first_denial
        );
        triage_misses.extend(missed.iter().map(|c| format!("{id}: {}", c.name())));
        report.insert(
            id.clone(),
            json!({
                "counts": {"loaded": o.loaded.is_ok(), "matched":o.matched, "mismatched":o.mismatched, "refused":o.refused, "missing":o.missing, "route":o.route},
                "load_error": o.loaded.clone().err(),
                "policy": o.mode.name(),
                "predicted": names(o.predicted), "predicted_from": o.predicted_source,
                "observed": names(o.observed),
                "observed_minus_predicted": names(missed),
                "predicted_minus_observed": names(unconfirmed),
                "policy_denied": o.policy_denied,
                "cases_after_first_denial": o.after_first_denial,
                "cases": o.details
            }),
        );
    }
    for (route, (pass, fail)) in &by_route {
        println!(
            "{route:20} pass {pass}, fail {fail} (mismatched + refused + missing), cases {}",
            pass + fail
        );
    }
    if let Ok(path) = std::env::var("DNACALC_VBA_CORPUS_REPORT") {
        std::fs::write(path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    }
    assert!(
        triage_misses.is_empty(),
        "observed capabilities that triage did not predict (each is a triage bug): {triage_misses:?}"
    );
    let baseline_path = corpus_dir().join("baseline.json");
    let current = Value::Object(counts);
    let mut baseline: Value =
        serde_json::from_str(&std::fs::read_to_string(&baseline_path).unwrap()).unwrap();
    if std::env::var("DNACALC_VBA_CORPUS_UPDATE").is_ok() {
        record_platform_baseline(&mut baseline, current, cfg!(windows));
        std::fs::write(
            &baseline_path,
            serde_json::to_string_pretty(&baseline).unwrap() + "\n",
        )
        .unwrap();
        return;
    }
    assert_eq!(
        current,
        platform_baseline(&baseline, cfg!(windows)),
        "the corpus counts moved; review the per-case report (DNACALC_VBA_CORPUS_REPORT) and \
         record the new counts with DNACALC_VBA_CORPUS_UPDATE=1"
    );
}
