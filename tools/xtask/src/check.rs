//! `cargo xtask check [--wasm]`: the per-commit check on this host.
//!
//! It first runs the five architecture checks `cargo xtask accept` runs (`arch::run_all`:
//! dependencies, os-imports, wasm-refusal, tree-reference-tokens and word-scan), one line each
//! with its findings, so a lane sees a layering, portability or wording finding before it pushes.
//! Then it builds the workspace (`cargo build --workspace`), then runs `cargo test -p <package>` for
//! every workspace member, one after the other, and prints one line per package: `passed` with
//! the test count, `zero tests` when its harnesses ran no test, or `failed` with the tail of its
//! log. xtask itself runs only its binary and library unit tests (`--bins`, plus `--lib` when it
//! has a library): its acceptance target builds lane clones and is run through `cargo xtask
//! accept`. With `--wasm` it then runs `cargo xtask ci wasm`, which tests the portable packages
//! under wasm-bindgen-test-runner in headless Chrome and Firefox (`ci.rs`), and prints one line per
//! portable package from that job's record: the tests each browser ran of those marked for the
//! browsers, the native-only tests grouped by their `Native-only:` reason, and the tests that are
//! neither (which fail the wasm job).
//!
//! Every cargo it starts uses `target/xtask-accept`, like `accept` and `ci`, so it never rebuilds
//! the running xtask binary. Logs go to `target/check/<package>.log` (the architecture checks' cargo
//! logs to `target/check/arch/`). The exit code is 1 when an architecture check, the build, any
//! package or the wasm run failed, 2 for a usage error, and 0 otherwise.

use crate::arch;
use crate::cargo::{self, Member, Workspace};
use crate::ci;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};

pub const USAGE: &str = "usage: cargo xtask check [--wasm]";

/// This tool's own package, whose acceptance target is left out of the check.
const SELF_PACKAGE: &str = "xtask";

/// How many log lines a failure prints.
const FAILURE_TAIL: usize = 40;

enum Status {
    Passed,
    ZeroTests,
    Failed,
}

/// Runs `command` from the workspace root with both output streams written to `log`; returns the
/// exit code (`None` when killed) and the output.
fn run_logged(
    ws: &Workspace,
    mut command: Command,
    log: &Path,
) -> Result<(Option<i32>, String), String> {
    command.current_dir(&ws.root);
    let out = cargo::output(&mut command)?;
    let text = format!("{}{}", cargo::text(&out.stdout), cargo::text(&out.stderr));
    fs::write(log, &text).map_err(|e| format!("cannot write {}: {e}", log.display()))?;
    Ok((out.status.code(), text))
}

fn cargo_command(ws: &Workspace, verb: &str) -> Command {
    let mut command = Command::new(cargo::cargo_program());
    command
        .args([verb, "--locked"])
        .arg("--target-dir")
        .arg(ws.accept_target_dir());
    command
}

fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// The test command for one package.
fn test_command(ws: &Workspace, member: &Member) -> Command {
    let mut command = cargo_command(ws, "test");
    command.args(["--no-fail-fast", "-p", &member.name]);
    if member.name == SELF_PACKAGE {
        command.arg("--bins");
        if member
            .targets
            .iter()
            .any(|target| target.kinds.iter().any(|kind| kind == "lib"))
        {
            command.arg("--lib");
        }
    }
    command
}

/// Tests one package and prints its line; returns whether it failed.
fn check_package(ws: &Workspace, member: &Member, log_dir: &Path) -> bool {
    let log = log_dir.join(format!("{}.log", member.name));
    println!("CHECK running cargo test -p {}", member.name);
    let (code, text) = match run_logged(ws, test_command(ws, member), &log) {
        Ok(result) => result,
        Err(e) => {
            println!("CHECK failed {}: {e}", member.name);
            return true;
        }
    };
    let (mut passed, mut failed, mut ignored, mut harnesses) = (0, 0, 0, 0);
    for counts in text.lines().filter_map(ci::result_counts) {
        harnesses += 1;
        passed += counts.passed;
        failed += counts.failed;
        ignored += counts.ignored;
    }
    let status = if code != Some(0) || failed > 0 {
        Status::Failed
    } else if passed == 0 {
        Status::ZeroTests
    } else {
        Status::Passed
    };
    let counts = format!(
        "{passed} passed, {failed} failed, {ignored} ignored in {harnesses} harnesses; log {}",
        ws.rel(&log)
    );
    match status {
        Status::Passed => println!("CHECK passed {}: {counts}", member.name),
        Status::ZeroTests => println!("CHECK zero tests {}: {counts}", member.name),
        Status::Failed => {
            println!(
                "CHECK failed {}: cargo exited with {code:?}; {counts}",
                member.name
            );
            println!("{}", tail(&text, FAILURE_TAIL));
        }
    }
    matches!(status, Status::Failed)
}

/// One line per portable package from the wasm job's record: where its tests run.
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
        println!("CHECK wasm coverage: no readable record at {}", ws.rel(&path));
        return;
    };
    for package in record["packages"].as_array().into_iter().flatten() {
        println!(
            "CHECK wasm {}: {}",
            package["package"].as_str().unwrap_or("?"),
            package["coverage"]["summary"]
                .as_str()
                .unwrap_or("the record holds no coverage")
        );
    }
}

pub fn run(raw: &[String]) -> ExitCode {
    let wasm = match raw {
        [] => false,
        [flag] if flag == "--wasm" => true,
        _ => {
            eprintln!("xtask check: unknown arguments {raw:?}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let ws = match std::env::current_dir()
        .map_err(|e| format!("no current directory: {e}"))
        .and_then(|cwd| Workspace::load(&cwd))
    {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("xtask check: cannot read the workspace: {e}");
            return ExitCode::from(1);
        }
    };
    let log_dir = ws.root.join("target").join("check");
    if let Err(e) = fs::create_dir_all(&log_dir) {
        eprintln!("xtask check: cannot create {}: {e}", log_dir.display());
        return ExitCode::from(1);
    }

    let mut failed: Vec<String> = Vec::new();
    println!("CHECK running the architecture checks");
    let checks = arch::run_all(&ws, &log_dir.join("arch"));
    let architecture = checks.len();
    for check in checks {
        println!(
            "CHECK {} architecture {}: {}",
            if check.passed { "passed" } else { "failed" },
            check.name,
            check.summary
        );
        for finding in &check.findings {
            println!("  {finding}");
        }
        if !check.passed {
            failed.push(format!("architecture {}", check.name));
        }
    }

    println!("CHECK running cargo build --workspace");
    let build_log = log_dir.join("build.log");
    let mut build = cargo_command(&ws, "build");
    build.arg("--workspace");
    match run_logged(&ws, build, &build_log) {
        Ok((Some(0), _)) => println!("CHECK passed build: log {}", ws.rel(&build_log)),
        Ok((code, text)) => {
            println!("CHECK failed build: cargo exited with {code:?}");
            println!("{}", tail(&text, FAILURE_TAIL));
            failed.push("build".to_owned());
            println!("CHECK RESULT failed: {}", failed.join(", "));
            return ExitCode::from(1);
        }
        Err(e) => {
            println!("CHECK failed build: {e}");
            return ExitCode::from(1);
        }
    }

    for member in &ws.members {
        if check_package(&ws, member, &log_dir) {
            failed.push(member.name.clone());
        }
    }

    let mut wasm_failed = false;
    if wasm {
        println!("CHECK running cargo xtask ci wasm");
        wasm_failed = ci::run(&["wasm".to_owned()]) != ExitCode::SUCCESS;
        print_browser_coverage(&ws);
        println!(
            "CHECK {} wasm: record target/ci/wasm",
            if wasm_failed { "failed" } else { "passed" }
        );
    }

    if failed.is_empty() && !wasm_failed {
        println!(
            "CHECK RESULT passed: {architecture} architecture checks, build and {} packages{}",
            ws.members.len(),
            if wasm {
                ", and wasm in Chrome and Firefox"
            } else {
                ""
            }
        );
        ExitCode::SUCCESS
    } else {
        let mut what = failed;
        if wasm_failed {
            what.push("wasm".to_owned());
        }
        println!("CHECK RESULT failed: {}", what.join(", "));
        ExitCode::from(1)
    }
}
