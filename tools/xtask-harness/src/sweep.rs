//! `cargo xtask sweep [--skip <step>]...`: the slow and structural validation, run on purpose at a
//! milestone or on a schedule, never required for a commit. It runs every step even when an
//! earlier one fails, then prints one summary line per step and one result line.
//!
//! The steps, by the name `--skip` takes:
//! - `architecture`: the five architecture checks (`arch::run_all`: dependencies, os-imports,
//!   wasm-refusal, tree-reference-tokens and the word scan over all text and history against its
//!   baseline);
//! - `provenance`: the whole provenance scan (`licence::check`): every file under calc-func and
//!   calc-value has a licence-ledger entry, and no marker of a removed source reappears;
//! - `ceiling`: the dependency ceiling on the host and wasm32: the resolved closures (normal and
//!   build edges) of dna-onecalc and dna-formula-ui, the formula UI both apps show, hold none of
//!   calc-engine, calc-doc and calc-host, and dna-calc does not depend directly on dna-onecalc
//!   (what the apps share lives in dna-formula-ui); on the host, dna-onecalc-desktop, the shipped
//!   OneCalc executable, holds neither calc-host nor calc-engine, and dna-calc-desktop holds no
//!   dna-onecalc;
//! - `wasm`: `cargo xtask ci wasm`, the portable packages' tests in headless Chrome and Firefox,
//!   with one line per package of where its tests ran; a test neither run in the browsers nor
//!   marked `Native-only:` fails the wasm job, and so this step;
//! - `app-smoke`: both applications' full browser smoke suites, including all case groups;
//! - `acceptance`: xtask's acceptance suite (`cargo test -p xtask --features acceptance --test
//!   acceptance`, one case at a time), which builds copies of the workspace and lane clones and
//!   takes hours. `infra_m0_1__independent_lanes` refuses a tree with uncommitted files, so run
//!   the sweep on a clean tree.
//!
//! Logs go to `target/sweep/`. The exit code is 1 when a step failed, 2 for a usage error.

use crate::arch;
use crate::cargo::{self, Resolved, Workspace};
use crate::ci;
use crate::licence;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::Instant;

pub const USAGE: &str = "usage: cargo xtask sweep [--skip architecture|provenance|ceiling|wasm|app-smoke|acceptance]...";

const STEPS: [&str; 6] = [
    "architecture",
    "provenance",
    "ceiling",
    "wasm",
    "app-smoke",
    "acceptance",
];

/// The packages DNA OneCalc's ceiling covers, and the packages none of them may reach.
const CEILING_PACKAGES: [&str; 2] = ["dna-onecalc", "dna-formula-ui"];
const ABOVE_CEILING: [&str; 3] = ["calc-engine", "calc-doc", "calc-host"];
/// OneCalc's desktop executable (native-only, so checked on the host) and what it may not reach.
const DESKTOP_CEILING_PACKAGES: [&str; 1] = ["dna-onecalc-desktop"];
const ABOVE_DESKTOP_CEILING: [&str; 2] = ["calc-engine", "calc-host"];

/// The shipped targets the ceiling is resolved for, besides the host.
const WASM_TARGET: &str = "wasm32-unknown-unknown";

struct Step {
    name: &'static str,
    passed: bool,
    summary: String,
    seconds: u64,
}

/// The dependency ceiling's findings over the host's and wasm32's resolved graphs.
fn ceiling(ws: &Workspace) -> (bool, String) {
    let host = match cargo::rustc_version() {
        Ok((host, _)) => host,
        Err(e) => return (false, format!("not checked: {e}")),
    };
    let mut findings = Vec::new();
    for target in [host.as_str(), WASM_TARGET] {
        let resolved = match Resolved::load(ws, target) {
            Ok(resolved) => resolved,
            Err(e) => {
                findings.push(format!("[{target}] cannot resolve the graph: {e}"));
                continue;
            }
        };
        for package in CEILING_PACKAGES {
            let Some(member) = ws.member(package) else {
                findings.push(format!("{package} is not a workspace member"));
                continue;
            };
            let closure = resolved.reach(&member.id, |edge| edge.normal || edge.build, false);
            for above in ABOVE_CEILING {
                if closure.contains(above) {
                    findings.push(format!(
                        "[{target}] {package} has {above} in its dependency closure"
                    ));
                }
            }
        }
        if target == host.as_str() {
            for package in DESKTOP_CEILING_PACKAGES {
                let Some(member) = ws.member(package) else {
                    findings.push(format!("{package} is not a workspace member"));
                    continue;
                };
                let closure = resolved.reach(&member.id, |edge| edge.normal || edge.build, false);
                for above in ABOVE_DESKTOP_CEILING {
                    if closure.contains(above) {
                        findings.push(format!(
                            "[{target}] {package} has {above} in its dependency closure"
                        ));
                    }
                }
            }
            if let Some(desktop) = ws.member("dna-calc-desktop") {
                let closure = resolved.reach(&desktop.id, |edge| edge.normal || edge.build, false);
                if closure.contains("dna-onecalc") {
                    findings.push(format!(
                        "[{target}] dna-calc-desktop has dna-onecalc in its dependency closure; DNA Calc's executable does not carry the OneCalc application"
                    ));
                }
            }
        }
        if let Some(calc) = ws.member("dna-calc") {
            let direct = resolved.reach(&calc.id, |edge| edge.normal || edge.build, true);
            if direct.contains("dna-onecalc") {
                findings.push(format!(
                    "[{target}] dna-calc depends directly on dna-onecalc; what the apps share belongs in dna-formula-ui"
                ));
            }
        }
    }
    if findings.is_empty() {
        (
            true,
            format!(
                "{CEILING_PACKAGES:?} reach none of {ABOVE_CEILING:?}, {DESKTOP_CEILING_PACKAGES:?} reach none of {ABOVE_DESKTOP_CEILING:?} and dna-calc-desktop none of dna-onecalc, and dna-calc does not depend on dna-onecalc, on {host} and {WASM_TARGET}"
            ),
        )
    } else {
        (false, findings.join("; "))
    }
}

/// One line per portable package from the wasm job's record: where its tests ran.
fn print_browser_coverage(ws: &Workspace) {
    let path = ws
        .root
        .join("target")
        .join("ci")
        .join("wasm")
        .join(ci::MANIFEST_FILE);
    let record: Option<Value> = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    let Some(record) = record else {
        println!(
            "SWEEP wasm coverage: no readable record at {}",
            ws.rel(&path)
        );
        return;
    };
    for package in record["packages"].as_array().into_iter().flatten() {
        println!(
            "SWEEP wasm {}: {}",
            package["package"].as_str().unwrap_or("?"),
            package["coverage"]["summary"]
                .as_str()
                .unwrap_or("the record holds no coverage")
        );
    }
}

/// xtask's acceptance suite, one case at a time, logged to `log`.
fn acceptance(ws: &Workspace, log: &Path) -> (bool, String) {
    let mut command = Command::new(cargo::cargo_program());
    command
        .args([
            "test",
            "--locked",
            "-p",
            "xtask",
            "--features",
            "acceptance",
            "--test",
            "acceptance",
            "--target-dir",
        ])
        .arg(ws.accept_target_dir())
        .args(["--", "--test-threads=1", "--show-output"])
        .current_dir(&ws.root);
    let out = match cargo::output(&mut command) {
        Ok(out) => out,
        Err(e) => return (false, e),
    };
    let text = format!("{}{}", cargo::text(&out.stdout), cargo::text(&out.stderr));
    if let Err(e) = fs::write(log, &text) {
        return (false, format!("cannot write {}: {e}", log.display()));
    }
    let failing: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("test ") && line.ends_with("... FAILED"))
        .collect();
    let report = xtask::test_output::summarize(&text);
    let passed = out.status.success() && failing.is_empty() && report.counts.failed == 0;
    (
        passed,
        format!(
            "{} passed, {} failed, {} ignored, {} not run in {} harnesses; failing {failing:?}; log {}",
            report.counts.passed,
            report.counts.failed,
            report.counts.ignored,
            report.not_run,
            report.harnesses,
            ws.rel(log)
        ),
    )
}

pub fn run(raw: &[String]) -> ExitCode {
    let mut skip = Vec::new();
    let mut args = raw.iter();
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next()) {
            ("--skip", Some(step)) if STEPS.contains(&step.as_str()) => skip.push(step.clone()),
            _ => {
                eprintln!("xtask sweep: unknown arguments {raw:?}\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let started = Instant::now();
    let ws = match std::env::current_dir()
        .map_err(|e| format!("no current directory: {e}"))
        .and_then(|cwd| Workspace::load(&cwd))
    {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("xtask sweep: cannot read the workspace: {e}");
            return ExitCode::from(1);
        }
    };
    let log_dir = ws.root.join("target").join("sweep");
    if let Err(e) = fs::create_dir_all(&log_dir) {
        eprintln!("xtask sweep: cannot create {}: {e}", log_dir.display());
        return ExitCode::from(1);
    }
    let mut steps: Vec<Step> = Vec::new();
    for name in STEPS {
        if skip.iter().any(|skipped| skipped == name) {
            println!("SWEEP skipped {name} (--skip)");
            continue;
        }
        println!("SWEEP running {name}");
        let at = Instant::now();
        let (passed, summary) = match name {
            "architecture" => {
                let checks = arch::run_all(&ws, &log_dir.join("arch"));
                let mut lines = Vec::new();
                let mut passed = true;
                for check in checks {
                    println!(
                        "SWEEP {} architecture {}: {}",
                        if check.passed { "passed" } else { "failed" },
                        check.name,
                        check.summary
                    );
                    for finding in &check.findings {
                        println!("  {finding}");
                    }
                    passed &= check.passed;
                    lines.push(format!(
                        "{} {}",
                        check.name,
                        if check.passed { "passed" } else { "failed" }
                    ));
                }
                (passed, lines.join(", "))
            }
            "provenance" => {
                let result = licence::check(&ws.root);
                for finding in &result.findings {
                    println!("  {finding}");
                }
                (result.passed, result.summary)
            }
            "ceiling" => ceiling(&ws),
            "wasm" => {
                let passed = ci::run(&["wasm".to_owned()]) == ExitCode::SUCCESS;
                print_browser_coverage(&ws);
                (passed, "record target/ci/wasm".to_owned())
            }
            "app-smoke" => {
                let mut passed = true;
                for application in &crate::app::APPS {
                    passed &= crate::app::smoke(
                        &ws,
                        application,
                        crate::app::SmokeTarget::Bundle(None),
                        None,
                        None,
                    ) == ExitCode::SUCCESS;
                }
                (
                    passed,
                    "Full app suites; group timings printed above".into(),
                )
            }
            _ => acceptance(&ws, &log_dir.join("acceptance.log")),
        };
        steps.push(Step {
            name,
            passed,
            summary,
            seconds: at.elapsed().as_secs(),
        });
    }
    println!("SWEEP summary:");
    for step in &steps {
        println!(
            "SWEEP {} {} ({} min {} s): {}",
            if step.passed { "passed" } else { "failed" },
            step.name,
            step.seconds / 60,
            step.seconds % 60,
            step.summary
        );
    }
    let failed: Vec<&str> = steps
        .iter()
        .filter(|step| !step.passed)
        .map(|step| step.name)
        .collect();
    let took = started.elapsed().as_secs();
    if failed.is_empty() {
        println!(
            "SWEEP RESULT passed: {} steps, {} skipped ({} min {} s)",
            steps.len(),
            skip.len(),
            took / 60,
            took % 60
        );
        ExitCode::SUCCESS
    } else {
        println!(
            "SWEEP RESULT failed: {} ({} min {} s)",
            failed.join(", "),
            took / 60,
            took % 60
        );
        ExitCode::from(1)
    }
}
