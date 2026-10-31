//! `cargo xtask check [--all]`: the per-commit check on this host, meant to take minutes on a warm
//! tree.
//!
//! In order:
//! - the word scan over new text only (`words::check_new`: the lines the working tree adds to the
//!   docs, AGENTS.md, README.md and the tracker since `HEAD` met `origin/master`, and the messages
//!   of the commits not pushed yet), so pushed text never fails it;
//! - the provenance marker scan (`licence::check_markers`: no marker of a removed third-party or
//!   Microsoft source reappears in code);
//! - `cargo build --workspace`;
//! - one `cargo test --no-fail-fast` over the touched packages and every workspace package that
//!   depends on one of them (dev-dependencies included). A package is touched when a file under its
//!   directory changed since `HEAD` met `origin/master` (committed, staged, unstaged or untracked);
//!   a change to the workspace's own `Cargo.toml` or `Cargo.lock` touches every package. With
//!   `--all` every package is tested. xtask's acceptance suite needs the `acceptance` feature, so a
//!   plain test run leaves it out; `cargo xtask sweep` runs it.
//!
//! The architecture checks, the licence ledger's completeness, the wasm run in the browsers and
//! the acceptance suite are not part of this check: `cargo xtask sweep` runs them on purpose, at a
//! milestone or on a schedule.
//!
//! Every cargo it starts uses `target/xtask-accept`, like `accept` and `ci`, so it never rebuilds
//! the running xtask binary. Logs go to `target/check/`. The exit code is 1 when a scan, the build
//! or a test failed, 2 for a usage error, and 0 otherwise.

use crate::cargo::{self, Resolved, Workspace};
use crate::licence;
use crate::test_output::{self, Counts};
use crate::words;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::Instant;

pub const USAGE: &str = "usage: cargo xtask check [--all]";

/// How many log lines a failure prints.
const FAILURE_TAIL: usize = 60;

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

/// The paths (relative to the root, `/` separators) changed since `base`: committed after it,
/// staged, unstaged or untracked.
pub fn changed_paths(root: &Path, base: &str) -> Result<BTreeSet<String>, String> {
    let mut paths = BTreeSet::new();
    let diff = cargo::git(root, &["diff", "--name-only", "-z", base])?;
    let untracked = cargo::git(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    for path in diff.split('\0').chain(untracked.split('\0')) {
        if !path.is_empty() {
            paths.insert(path.replace('\\', "/"));
        }
    }
    Ok(paths)
}

/// The packages a change to `paths` touches: the member whose directory holds each path (the
/// deepest one), or every member for the workspace's own `Cargo.toml` and `Cargo.lock`.
pub fn touched_packages(ws: &Workspace, paths: &BTreeSet<String>) -> BTreeSet<String> {
    let dirs: Vec<(String, &str)> = ws
        .members
        .iter()
        .map(|member| (ws.rel(&member.dir), member.name.as_str()))
        .collect();
    let mut touched = BTreeSet::new();
    for path in paths {
        if path == "Cargo.toml" || path == "Cargo.lock" {
            touched.extend(ws.members.iter().map(|member| member.name.clone()));
            continue;
        }
        if let Some((_, name)) = dirs
            .iter()
            .filter(|(dir, _)| path.starts_with(&format!("{dir}/")))
            .max_by_key(|(dir, _)| dir.len())
        {
            touched.insert((*name).to_owned());
        }
    }
    touched
}

/// `touched` and every workspace member whose resolved dependency closure (normal, build and dev
/// edges) holds one of them.
fn with_dependents(
    ws: &Workspace,
    resolved: &Resolved,
    touched: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut packages = touched.clone();
    for member in &ws.members {
        let closure = resolved.reach(&member.id, |_| true, false);
        if closure.iter().any(|name| touched.contains(name)) {
            packages.insert(member.name.clone());
        }
    }
    packages
}

fn print_result(name: &str, passed: bool, summary: &str, findings: &[String]) {
    println!(
        "CHECK {} {name}: {summary}",
        if passed { "passed" } else { "failed" }
    );
    for finding in findings {
        println!("  {finding}");
    }
}

pub fn run(raw: &[String]) -> ExitCode {
    let all = match raw {
        [] => false,
        [flag] if flag == "--all" => true,
        _ => {
            eprintln!("xtask check: unknown arguments {raw:?}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let started = Instant::now();
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

    for result in [words::check_new(&ws.root), licence::check_markers(&ws.root)] {
        print_result(
            result.name,
            result.passed,
            &result.summary,
            &result.findings,
        );
        if !result.passed {
            failed.push(result.name.to_owned());
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

    // The packages to test.
    let packages: BTreeSet<String> = if all {
        println!("CHECK testing every package (--all)");
        ws.members
            .iter()
            .map(|member| member.name.clone())
            .collect()
    } else {
        let (base, how) = words::new_text_base(&ws.root);
        let touched = match changed_paths(&ws.root, &base) {
            Ok(paths) => {
                let touched = touched_packages(&ws, &paths);
                println!(
                    "CHECK {} changed paths since {how}; touched packages {touched:?}",
                    paths.len()
                );
                touched
            }
            Err(e) => {
                println!("CHECK failed changes: {e}");
                failed.push("changes".to_owned());
                BTreeSet::new()
            }
        };
        if touched.is_empty() {
            touched
        } else {
            let host = cargo::rustc_version().map(|(host, _)| host);
            match host.and_then(|host| Resolved::load(&ws, &host)) {
                Ok(resolved) => with_dependents(&ws, &resolved, &touched),
                Err(e) => {
                    println!("CHECK failed dependents: {e}");
                    failed.push("dependents".to_owned());
                    touched
                }
            }
        }
    };

    let mut tested = String::from("no package touched, so no tests (0 not run)");
    if !packages.is_empty() {
        let shown: Vec<&str> = packages.iter().map(String::as_str).collect();
        println!(
            "CHECK running one cargo test over {} packages: {}",
            shown.len(),
            shown.join(", ")
        );
        let log = log_dir.join("test.log");
        let mut test = cargo_command(&ws, "test");
        test.arg("--no-fail-fast");
        for package in &packages {
            test.args(["-p", package]);
        }
        // Successful cases' output must remain visible: libtest otherwise hides their
        // ACCEPT-NOT-RUN markers and reports them as passes.
        test.args(["--", "--show-output"]);
        match run_logged(&ws, test, &log) {
            Ok((code, text)) => {
                let report = test_output::summarize(&text);
                let Counts {
                    passed,
                    failed: failures,
                    ignored,
                } = report.counts;
                let harnesses = report.harnesses;
                let not_run = report.not_run;
                let counts = format!(
                    "{passed} passed, {failures} failed, {ignored} ignored, {not_run} not run in {harnesses} harnesses of {} packages; log {}",
                    packages.len(),
                    ws.rel(&log)
                );
                if code == Some(0) && failures == 0 {
                    println!("CHECK passed tests: {counts}");
                    tested = format!("tests of {} packages", packages.len());
                } else {
                    println!("CHECK failed tests: cargo exited with {code:?}; {counts}");
                    let failing: Vec<&str> = text
                        .lines()
                        .filter(|line| line.trim_end().ends_with("... FAILED"))
                        .collect();
                    for line in &failing {
                        println!("  {}", line.trim());
                    }
                    println!("{}", tail(&text, FAILURE_TAIL));
                    failed.push("tests".to_owned());
                }
            }
            Err(e) => {
                println!("CHECK failed tests: {e}");
                failed.push("tests".to_owned());
            }
        }
    }

    let took = started.elapsed().as_secs();
    if failed.is_empty() {
        println!(
            "CHECK RESULT passed: the new-text word scan, the provenance markers, the build and {tested} ({} min {} s)",
            took / 60,
            took % 60
        );
        ExitCode::SUCCESS
    } else {
        println!(
            "CHECK RESULT failed: {} ({} min {} s)",
            failed.join(", "),
            took / 60,
            took % 60
        );
        ExitCode::from(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cargo::Member;
    use std::path::PathBuf;

    fn member(name: &str, dir: &str) -> Member {
        Member {
            name: name.to_owned(),
            id: name.to_owned(),
            dir: PathBuf::from(format!("/w/{dir}")),
            portability: Some("portable".to_owned()),
            targets: Vec::new(),
        }
    }

    #[test]
    fn a_path_touches_the_package_whose_directory_holds_it() {
        let ws = Workspace {
            root: PathBuf::from("/w"),
            members: vec![
                member("calc-func", "crates/calc-func"),
                member("xtask", "tools/xtask"),
            ],
        };
        let paths = |list: &[&str]| -> BTreeSet<String> {
            list.iter().map(|path| (*path).to_owned()).collect()
        };
        assert_eq!(
            touched_packages(&ws, &paths(&["crates/calc-func/src/lib.rs", "docs/a.md"])),
            BTreeSet::from(["calc-func".to_owned()])
        );
        assert_eq!(
            touched_packages(&ws, &paths(&["crates/calc-funcs/x.rs", "AGENTS.md"])),
            BTreeSet::new()
        );
        assert_eq!(
            touched_packages(&ws, &paths(&["Cargo.lock"])),
            BTreeSet::from(["calc-func".to_owned(), "xtask".to_owned()])
        );
    }
}
