//! Light per-commit checks; dependent and full runs belong to the periodic keeper.
use crate::cargo::{self, Resolved, Workspace};
use crate::{licence, test_output};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const USAGE: &str = "usage: cargo xtask check [--dependents | --all] [--base <rev>]";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub dependents: bool,
    pub all: bool,
    pub base: Option<String>,
    pub keeper: bool,
}
impl Options {
    pub fn parse(raw: &[String]) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = raw.iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--dependents" if !options.dependents => options.dependents = true,
                "--all" if !options.all => options.all = true,
                "--base" if options.base.is_none() => {
                    options.base = Some(
                        args.next()
                            .filter(|s| !s.starts_with('-'))
                            .ok_or(USAGE)?
                            .clone(),
                    );
                }
                _ => return Err(USAGE.into()),
            }
        }
        if options.all && options.dependents {
            return Err(USAGE.into());
        }
        Ok(options)
    }
    fn allow_known_red(&self) -> bool {
        !self.all && !self.dependents && !self.keeper
    }
}

/// Both handles share one file cursor. Child writes arrive in their original order rather
/// than being regrouped by stream. Every command appends its duration to the same run log.
pub fn run_logged(
    root: &Path,
    command: &mut Command,
    log: &Path,
) -> Result<(Option<i32>, String), String> {
    command.current_dir(root);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|e| e.to_string())?;
    writeln!(file, "RUN {:?}", cargo::argv(command)).map_err(|e| e.to_string())?;
    let start = file.metadata().map_err(|e| e.to_string())?.len();
    let started = Instant::now();
    command.stdout(Stdio::from(file.try_clone().map_err(|e| e.to_string())?));
    command.stderr(Stdio::from(file.try_clone().map_err(|e| e.to_string())?));
    let status = command
        .status()
        .map_err(|e| format!("cannot start {command:?}: {e}"))?;
    let bytes = fs::read(log).map_err(|e| e.to_string())?;
    let text = cargo::text(&bytes[start as usize..]);
    writeln!(
        file,
        "END exit {:?}; elapsed {:.3}s",
        status.code(),
        started.elapsed().as_secs_f64()
    )
    .map_err(|e| e.to_string())?;
    println!(
        "CHECK command exit {:?}; {:.3}s; log {}",
        status.code(),
        started.elapsed().as_secs_f64(),
        log.display()
    );
    Ok((status.code(), text))
}

fn cargo_command(verb: &str) -> Command {
    let mut command = Command::new(cargo::cargo_program());
    command.args([verb, "--locked"]);
    command
}

pub fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// The paths (relative to the root, `/` separators) changed since `base`: committed after it,
/// staged, unstaged or untracked.
pub fn changed_paths(root: &Path, base: &str) -> Result<BTreeSet<String>, String> {
    let mut paths = BTreeSet::new();
    let diff = cargo::git(root, &["diff", "--no-renames", "--name-only", "-z", base])?;
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Failure {
    pub package: String,
    /// lib, bin:<name>, test:<name>, example:<name> or doc.
    pub target: String,
    pub name: String,
}
impl Failure {
    pub fn json(&self) -> Value {
        json!({"package":self.package,"target":self.target,"name":self.name})
    }
    pub fn from_json(v: &Value) -> Option<Self> {
        Some(Self {
            package: v["package"].as_str()?.into(),
            target: v["target"].as_str()?.into(),
            name: v["name"].as_str()?.into(),
        })
    }
    pub fn command(&self) -> Result<Command, String> {
        let mut command = cargo_command("test");
        command.args(["-p", &self.package]);
        if self.target == "lib" {
            command.arg("--lib");
        } else if self.target == "doc" {
            command.arg("--doc");
        } else if let Some((kind, name)) = self.target.split_once(':') {
            if !["test", "bin", "example", "bench"].contains(&kind) {
                return Err("unknown test target kind".into());
            }
            command.args([&format!("--{kind}"), name]);
        } else {
            return Err("unknown test target".into());
        }
        command.args(["--", "--exact", &self.name, "--show-output"]);
        Ok(command)
    }
}

/// Cargo artifacts identify executable/package/target without guessing from test names.
/// This also distinguishes integration targets with the same name in different packages.
pub fn failures(ws: &Workspace, text: &str) -> Vec<Failure> {
    let mut artifacts = BTreeMap::new();
    let mut current = None;
    let mut failed = BTreeSet::new();
    for line in text.lines() {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            if v["reason"] == "compiler-artifact"
                && let Some(exe) = v["executable"].as_str()
                && let Some(member) = ws.members.iter().find(|m| v["package_id"] == m.id)
            {
                let kinds = &v["target"]["kind"];
                let kind = kinds[0].as_str().unwrap_or("");
                let name = v["target"]["name"].as_str().unwrap_or("");
                let target = if matches!(
                    kind,
                    "lib" | "rlib" | "proc-macro" | "cdylib" | "dylib" | "staticlib"
                ) {
                    "lib".into()
                } else {
                    format!("{kind}:{name}")
                };
                let key = Path::new(exe)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                artifacts.insert(key, (member.name.clone(), target));
            }
        }
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Running ") {
            current = rest.rsplit_once('(').and_then(|(_, path)| {
                let path = path.trim_end_matches(')').replace('\\', "/");
                artifacts
                    .get(path.rsplit('/').next().unwrap_or(""))
                    .cloned()
            });
        } else if let Some(package) = trimmed.strip_prefix("Doc-tests ") {
            current = Some((package.trim().replace('_', "-"), "doc".into()));
        }
        if let Some(name) = trimmed
            .strip_prefix("test ")
            .and_then(|s| s.strip_suffix(" ... FAILED"))
            && let Some((package, target)) = &current
        {
            failed.insert(Failure {
                package: package.clone(),
                target: target.clone(),
                name: name.into(),
            });
        }
    }
    failed.into_iter().collect()
}

pub fn platform() -> &'static str {
    if cfg!(windows) { "windows" } else { "linux" }
}

pub fn br(root: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new("br");
    command.current_dir(root).args(args);
    // br sync overflows the default main-thread stack on Windows (AGENTS.md).
    if std::env::var_os("RUST_MIN_STACK").is_none() {
        command.env("RUST_MIN_STACK", "67108864");
    }
    let out = command.output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(cargo::text(&out.stdout))
    } else {
        Err(format!("br failed: {}", cargo::text(&out.stderr)))
    }
}

pub fn issues(value: &Value) -> &[Value] {
    value
        .as_array()
        .or_else(|| value["issues"].as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// All failures must have an exact runnable identity before retries or known-red handling can
/// affect the result. Missing harnesses, build errors and crashes require a full rerun.
fn identified_failures(code: Option<i32>, text: &str, found: &[Failure]) -> bool {
    code == Some(101)
        && !found.is_empty()
        && found.len() as u64 == test_output::summarize(text).counts.failed
        && !text.lines().any(|l| {
            l.contains("could not compile")
                || l.contains("process didn't exit successfully")
                || l.contains("doctest failed")
                || (l.starts_with("error:")
                    && !l.starts_with("error: test failed,")
                    && !l.contains(" targets failed:")
                    && !l.contains(" target failed:"))
        })
}

/// Only open keeper-owned master-red records, and only exact platform/package/target/name rows.
pub fn known_red(value: &Value, platform: &str) -> BTreeMap<Failure, String> {
    let mut known = BTreeMap::new();
    for issue in issues(value) {
        if !matches!(issue["status"].as_str(), Some("open" | "in_progress"))
            || issue["owner"].as_str() != Some(&format!("keeper-{platform}"))
            || !issue["labels"]
                .as_array()
                .is_some_and(|labels| labels.iter().any(|l| l == "master-red"))
        {
            continue;
        }
        for line in issue["description"].as_str().unwrap_or("").lines() {
            if let Some(rest) = line.strip_prefix("failing: ") {
                let parts: Vec<_> = rest.split_whitespace().collect();
                if parts.len() == 4 && parts[0] == platform {
                    known.insert(
                        Failure {
                            package: parts[1].into(),
                            target: parts[2].into(),
                            name: parts[3].into(),
                        },
                        issue["id"].as_str().unwrap_or("unknown").into(),
                    );
                }
            }
        }
    }
    known
}

pub struct Report {
    pub passed: bool,
    pub failures: Vec<Failure>,
    pub errors: Vec<String>,
    pub identified_test_failures: bool,
    pub log: PathBuf,
    pub seconds: f64,
}
impl Report {
    pub fn json(&self) -> Value {
        json!({"passed":self.passed,"failures":self.failures.iter().map(Failure::json).collect::<Vec<_>>(),"errors":self.errors,"identified_test_failures":self.identified_test_failures,"log":self.log,"seconds":self.seconds})
    }
}

fn step(
    ws: &Workspace,
    command: &mut Command,
    log: &Path,
    name: &str,
    errors: &mut Vec<String>,
) -> Result<String, String> {
    let (code, text) = run_logged(&ws.root, command, log)?;
    if code != Some(0) {
        println!("CHECK failed {name}:\n{}", tail(&text, 60));
        errors.push(name.into());
    }
    Ok(text)
}

fn warn_logs(root: &Path) {
    if let Ok(text) = cargo::git(
        root,
        &[
            "log",
            "--no-merges",
            "--format=%h%x09%(trailers:key=Agent-Log,valueonly)",
            "origin/master..HEAD",
        ],
    ) {
        for line in text.lines().filter(|s| !s.trim().is_empty()) {
            if let Some((sha, trailer)) = line.split_once('\t')
                && trailer.trim().is_empty()
            {
                println!("CHECK warning: unpushed commit {sha} lacks an Agent-Log trailer");
            }
        }
    }
}

pub fn execute(ws: &Workspace, options: &Options) -> Result<Report, String> {
    let started = Instant::now();
    let dir = ws.root.join("target/check");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let log = dir.join(format!("{stamp}-{}.log", std::process::id()));
    let mut errors = Vec::new();
    let markers = licence::check_markers(&ws.root);
    println!(
        "CHECK {} {}: {}",
        if markers.passed { "passed" } else { "failed" },
        markers.name,
        markers.summary
    );
    for finding in markers.findings {
        println!("  {finding}");
    }
    if !markers.passed {
        errors.push(markers.name.into());
    }
    warn_logs(&ws.root);
    let base = options
        .base
        .clone()
        .unwrap_or_else(|| new_text_base(&ws.root).0);
    let paths = changed_paths(&ws.root, &base)?;
    let touched = touched_packages(ws, &paths);
    println!(
        "CHECK {} changed paths since {base}; touched packages {touched:?}",
        paths.len()
    );
    let host = cargo::rustc_version()?.0;
    let packages = if options.all {
        ws.members.iter().map(|m| m.name.clone()).collect()
    } else if options.dependents && !touched.is_empty() {
        with_dependents(ws, &Resolved::load(ws, &host)?, &touched)
    } else {
        touched.clone()
    };
    let mut build = cargo_command(if options.all || options.dependents {
        "build"
    } else {
        "check"
    });
    build.arg("--workspace");
    if !options.all && !options.dependents {
        build.arg("--all-targets");
    }
    step(ws, &mut build, &log, "workspace", &mut errors)?;
    let mut found = Vec::new();
    let mut identified_test_failures = false;
    if errors.is_empty() && !packages.is_empty() {
        println!("CHECK testing packages {packages:?}");
        let mut test = cargo_command("test");
        test.args(["--no-fail-fast", "--message-format=json-render-diagnostics"]);
        for package in &packages {
            test.args(["-p", package]);
        }
        test.args(["--", "--show-output"]);
        let (code, text) = run_logged(&ws.root, &mut test, &log)?;
        found = failures(ws, &text);
        let summary = test_output::summarize(&text);
        println!(
            "CHECK tests: {} passed, {} failed, {} ignored, {} not run in {} harnesses",
            summary.counts.passed,
            summary.counts.failed,
            summary.counts.ignored,
            summary.not_run,
            summary.harnesses
        );
        if code != Some(0) || summary.counts.failed != 0 {
            let known = if options.allow_known_red() {
                match br(&ws.root, &["list", "--label", "master-red", "--json"]) {
                    Ok(text) => known_red(
                        &serde_json::from_str::<Value>(&text).map_err(|e| e.to_string())?,
                        platform(),
                    ),
                    Err(e) => {
                        println!("CHECK warning: cannot read known red tests: {e}");
                        BTreeMap::new()
                    }
                }
            } else {
                BTreeMap::new()
            };
            for failure in &found {
                if let Some(bead) = known.get(failure) {
                    println!("CHECK {:?}: failed, known red ({bead})", failure);
                } else {
                    println!("CHECK failed {:?}", failure);
                }
            }
            // Compilation, process crashes, doctest or unparsed failures cannot be excused.
            identified_test_failures = identified_failures(code, &text, &found);
            let only_known =
                identified_test_failures && found.iter().all(|f| known.contains_key(f));
            if !only_known {
                errors.push("tests".into());
                println!("{}", tail(&text, 60));
            }
        }
    }
    if !options.all && !options.dependents {
        if cfg!(windows) && !touched.is_empty() {
            let out = Command::new("rustup")
                .args(["target", "list", "--installed"])
                .output()
                .map_err(|e| e.to_string())?;
            if out.status.success()
                && cargo::text(&out.stdout)
                    .lines()
                    .any(|l| l == "x86_64-unknown-linux-gnu")
            {
                let linux = with_dependents(
                    ws,
                    &Resolved::load(ws, "x86_64-unknown-linux-gnu")?,
                    &touched,
                );
                let mut command = cargo_command("check");
                command.args(["--all-targets", "--target", "x86_64-unknown-linux-gnu"]);
                for package in linux {
                    command.args(["-p", &package]);
                }
                step(ws, &mut command, &log, "Linux cross check", &mut errors)?;
            } else {
                println!(
                    "CHECK note: Linux cross check skipped; x86_64-unknown-linux-gnu target is not installed"
                );
            }
        }
        let portable: Vec<_> = ws
            .members
            .iter()
            .filter(|m| m.is_portable() && touched.contains(&m.name))
            .collect();
        if !portable.is_empty() {
            let mut command = cargo_command("check");
            command.args(["--target", cargo::WASM_TARGET]);
            for member in portable {
                command.args(["-p", &member.name]);
            }
            step(ws, &mut command, &log, "wasm32", &mut errors)?;
        }
    }
    let report = Report {
        passed: errors.is_empty(),
        failures: found,
        errors,
        identified_test_failures,
        log,
        seconds: started.elapsed().as_secs_f64(),
    };
    fs::write(dir.join("results.json"), report.json().to_string()).map_err(|e| e.to_string())?;
    println!(
        "CHECK RESULT {}: {:.3}s; log {}",
        if report.passed { "passed" } else { "failed" },
        report.seconds,
        report.log.display()
    );
    Ok(report)
}

pub fn run(raw: &[String]) -> ExitCode {
    let options = match Options::parse(raw) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    match std::env::current_dir()
        .map_err(|e| e.to_string())
        .and_then(|cwd| Workspace::load(&cwd))
        .and_then(|ws| execute(&ws, &options))
    {
        Ok(report) => ExitCode::from(if report.passed { 0 } else { 1 }),
        Err(e) => {
            eprintln!("xtask check: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The commit new text is measured from: where `HEAD` meets `origin/master` (`HEAD` itself when
/// the repository has no `origin/master`), with how it was found.
pub fn new_text_base(root: &Path) -> (String, String) {
    match crate::cargo::git(root, &["merge-base", "HEAD", "origin/master"]) {
        Ok(base) => (
            base.trim().to_owned(),
            "where HEAD meets origin/master".to_owned(),
        ),
        Err(_) => (
            "HEAD".to_owned(),
            "HEAD (no origin/master: only uncommitted text is new)".to_owned(),
        ),
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
                member("nested", "crates/calc-func/nested"),
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
            touched_packages(&ws, &paths(&["crates/calc-func/nested/src/lib.rs"])),
            BTreeSet::from(["nested".to_owned()])
        );
        assert_eq!(
            touched_packages(&ws, &paths(&["Cargo.lock"])),
            BTreeSet::from([
                "calc-func".to_owned(),
                "xtask".to_owned(),
                "nested".to_owned()
            ])
        );
    }
    #[test]
    fn modes_and_explicit_baselines_are_independent() {
        let parse =
            |args: &[&str]| Options::parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(parse(&[]).unwrap(), Options::default());
        assert!(parse(&[]).unwrap().allow_known_red());
        let dependent = parse(&["--base", "HEAD~2", "--dependents"]).unwrap();
        assert!(dependent.dependents);
        assert_eq!(dependent.base.as_deref(), Some("HEAD~2"));
        assert!(!dependent.allow_known_red());
        assert!(!parse(&["--all"]).unwrap().allow_known_red());
        assert!(
            !Options {
                keeper: true,
                ..Options::default()
            }
            .allow_known_red()
        );
        for args in [
            &["--base"][..],
            &["--all", "--dependents"],
            &["--all", "--all"],
            &["--base", "--all"],
            &["--other"],
        ] {
            assert!(parse(args).is_err());
        }
    }

    #[test]
    fn known_red_requires_open_keeper_ownership_and_exact_platform_and_target() {
        let record = json!({"id":"dc-red","status":"open","owner":"keeper-linux","labels":["keeper","master-red"],
            "description":"failing: linux a test:vba exact_name\nfailing: windows a test:vba other\nfailing: linux a test:vba too many words"});
        let failure = Failure {
            package: "a".into(),
            target: "test:vba".into(),
            name: "exact_name".into(),
        };
        let value = json!({"issues":[record.clone()]});
        assert_eq!(
            known_red(&value, "linux").get(&failure).map(String::as_str),
            Some("dc-red")
        );
        assert!(!known_red(&value, "windows").contains_key(&failure));
        assert!(!known_red(&value, "linux").contains_key(&Failure {
            target: "lib".into(),
            ..failure.clone()
        }));
        for (key, val) in [
            ("status", json!("closed")),
            ("owner", json!("lane")),
            ("labels", json!(["flaky"])),
        ] {
            let mut changed = record.clone();
            changed[key] = val;
            assert!(known_red(&json!([changed]), "linux").is_empty());
        }
    }

    #[test]
    fn cargo_artifacts_disambiguate_identically_named_integration_targets() {
        let ws = Workspace {
            root: PathBuf::from("/w"),
            members: vec![member("a", "a"), member("b", "b")],
        };
        let mut output = String::new();
        for pkg in ["a", "b"] {
            output.push_str(&json!({"reason":"compiler-artifact","package_id":pkg,"target":{"kind":["test"],"name":"same"},"executable":format!("/w/target/debug/deps/same-{pkg}")}).to_string());
            output.push('\n');
        }
        output.push_str("     Running tests/same.rs (target/debug/deps/same-a)\ntest exact ... FAILED\n     Running tests/same.rs (target/debug/deps/same-b)\ntest exact ... FAILED\n");
        let parsed = failures(&ws, &output);
        assert_eq!(parsed.len(), 2);
        let lib_output = format!(
            "{}\n Running unittests src/lib.rs (target/debug/deps/a-lib)\ntest library_case ... FAILED\n",
            json!({"reason":"compiler-artifact","package_id":"a","target":{"kind":["cdylib","rlib"],"name":"a"},"executable":"/w/target/debug/deps/a-lib"})
        );
        assert_eq!(failures(&ws, &lib_output)[0].target, "lib");
        assert_eq!(parsed[0].package, "a");
        assert_eq!(parsed[1].package, "b");
        assert_eq!(parsed[0].target, "test:same");
        let args: Vec<_> = parsed[0]
            .command()
            .unwrap()
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "test",
                "--locked",
                "-p",
                "a",
                "--test",
                "same",
                "--",
                "--exact",
                "exact",
                "--show-output"
            ]
        );
        assert_eq!(
            Failure::from_json(&parsed[0].json()),
            Some(parsed[0].clone())
        );
    }

    #[test]
    fn retries_cannot_hide_unidentified_crashed_or_uncompiled_harnesses() {
        let failure = Failure {
            package: "p".into(),
            target: "lib".into(),
            name: "a".into(),
        };
        let text = "test a ... FAILED\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\nerror: test failed, to rerun pass `-p p --lib`\nerror: 1 target failed:\n";
        assert!(identified_failures(Some(101), text, &[failure.clone()]));
        assert!(!identified_failures(Some(1), text, &[failure.clone()]));
        for extra in [
            "error: could not compile p",
            "error: process didn't exit successfully",
            "error: doctest failed",
            "error: unexpected infrastructure failure",
            "test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out",
        ] {
            assert!(!identified_failures(
                Some(101),
                &format!("{text}\n{extra}"),
                &[failure.clone()]
            ));
        }
    }

    #[test]
    fn command_logs_keep_stream_order_and_command_timings() {
        let root = std::env::temp_dir().join(format!("xtask-streams-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let log = root.join("run.log");
        #[cfg(windows)]
        let mut command = {
            let mut c = Command::new("cmd");
            c.args(["/D", "/C", "echo first & echo second 1>&2 & echo third"]);
            c
        };
        #[cfg(not(windows))]
        let mut command = {
            let mut c = Command::new("sh");
            c.args([
                "-c",
                "printf 'first\\n'; printf 'second\\n' >&2; printf 'third\\n'",
            ]);
            c
        };
        let (code, output) = run_logged(&root, &mut command, &log).unwrap();
        assert_eq!(code, Some(0));
        let lines: Vec<_> = output.lines().map(str::trim).collect();
        assert_eq!(lines, ["first", "second", "third"]);
        assert!(
            fs::read_to_string(&log)
                .unwrap()
                .contains("END exit Some(0); elapsed")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changed_paths_include_committed_staged_unstaged_untracked_and_deletions() {
        let root = std::env::temp_dir().join(format!("xtask-check-paths-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        cargo::git(&root, &["init"]).unwrap();
        cargo::git(&root, &["config", "user.name", "Fixture"]).unwrap();
        cargo::git(&root, &["config", "user.email", "fixture@example.invalid"]).unwrap();
        for name in ["committed", "staged", "unstaged", "deleted"] {
            fs::write(root.join(name), "old").unwrap();
        }
        cargo::git(&root, &["add", "."]).unwrap();
        cargo::git(&root, &["commit", "-m", "base"]).unwrap();
        let base = cargo::git(&root, &["rev-parse", "HEAD"]).unwrap();
        fs::write(root.join("committed"), "new").unwrap();
        cargo::git(&root, &["add", "committed"]).unwrap();
        cargo::git(&root, &["commit", "-m", "changed"]).unwrap();
        fs::write(root.join("staged"), "new").unwrap();
        cargo::git(&root, &["add", "staged"]).unwrap();
        fs::write(root.join("unstaged"), "new").unwrap();
        fs::write(root.join("untracked"), "new").unwrap();
        fs::remove_file(root.join("deleted")).unwrap();
        assert_eq!(
            changed_paths(&root, base.trim()).unwrap(),
            ["committed", "staged", "unstaged", "deleted", "untracked"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );
        fs::remove_dir_all(root).unwrap();
    }
}
