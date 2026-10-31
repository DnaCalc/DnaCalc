//! `cargo xtask ci-audit --require-executed-targets <target>[,<target>...] (--run <id> | --from
//! <path>)... [--allow-dirty]`
//!
//! Merges the executed-target manifests that `cargo xtask ci windows|linux|wasm` wrote
//! (`target/ci/<job>/executed-targets.json`, `ci.rs`), taken from a CI run's artifacts (`--run
//! <id>`, downloaded with `gh run download` into `target/ci-audit/run-<id>/`) or from local
//! directories or files (`--from`, each of which must exist; one that holds no manifest adds
//! nothing and is reported with its count). A directory is searched for files named
//! `executed-targets.json` directly inside a `windows/`, `linux/` or `wasm/` directory, the place
//! `cargo xtask ci <job>` writes them; any other file of that name below it is listed as skipped,
//! never read. A file given by its own path is read whatever its name.
//!
//! It fails unless, for each required target, exactly one manifest covers it, that manifest's job
//! passed, and every portable package of this checkout's workspace (both app cores under `apps/`
//! among them) appears there built, with its test harness run on every target cargo built for it
//! and no failure: on wasm, under the runner for Chrome and for Firefox, and with the route probe
//! passed in both browsers.
//! Every manifest must name this checkout's commit and a clean working tree. A manifest made from a
//! dirty tree is reported on an `AUDIT dirty` line naming its changed paths, and fails the audit
//! unless `--allow-dirty` is given (a local tree before its commit); a CI checkout is clean. A
//! package whose harness ran zero tests is listed with 0 and counted as zero tests, never as
//! passed; on wasm the line also says when wasm-bindgen-test-runner reported no tests and started
//! no browser. The merged record goes to `target/ci-audit/audit.json`.

use crate::cargo::{self, Workspace};
use crate::ci;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

pub const USAGE: &str = "usage: cargo xtask ci-audit --require-executed-targets <target>[,<target>...] (--run <run id> | --from <dir or file>)... [--allow-dirty]";

const TARGETS: [&str; 3] = ["windows", "linux", "wasm"];
const BROWSERS: [&str; 2] = ["chromium", "firefox"];

struct Args {
    required: Vec<String>,
    runs: Vec<String>,
    froms: Vec<PathBuf>,
    allow_dirty: bool,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut parsed = Args {
        required: Vec::new(),
        runs: Vec::new(),
        froms: Vec::new(),
        allow_dirty: false,
    };
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--require-executed-targets" => {
                let list = rest
                    .next()
                    .ok_or("--require-executed-targets needs a list")?;
                for target in list.split(',').map(str::trim).filter(|t| !t.is_empty()) {
                    if !TARGETS.contains(&target) {
                        return Err(format!(
                            "unknown target {target}; expected windows, linux or wasm"
                        ));
                    }
                    if parsed.required.iter().any(|known| known == target) {
                        return Err(format!("target {target} is named twice"));
                    }
                    parsed.required.push(target.to_owned());
                }
            }
            "--run" => {
                let id = rest.next().ok_or("--run needs a run ID")?;
                if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
                    return Err(format!("{id} is not a GitHub Actions run ID"));
                }
                parsed.runs.push(id.clone());
            }
            "--from" => parsed
                .froms
                .push(PathBuf::from(rest.next().ok_or("--from needs a path")?)),
            "--allow-dirty" => parsed.allow_dirty = true,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if parsed.required.is_empty() {
        return Err("--require-executed-targets is required".to_owned());
    }
    if parsed.runs.is_empty() && parsed.froms.is_empty() {
        return Err("give at least one --run or --from".to_owned());
    }
    Ok(parsed)
}

/// The manifests at or below `path`, in path order, and the files named like one that are not
/// where `cargo xtask ci <job>` writes its manifest. `path` itself, when a file, is a manifest; in
/// a directory, an `executed-targets.json` counts only directly inside a `windows/`, `linux/` or
/// `wasm/` directory (`target/ci/<job>/`, also inside a downloaded CI artifact), so an altered or
/// copied manifest kept as evidence elsewhere is never read as a job's record.
fn manifest_files(path: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    if path.is_file() {
        return (vec![path.to_path_buf()], Vec::new());
    }
    let (mut found, mut skipped) = (Vec::new(), Vec::new());
    let mut pending = vec![path.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() && entry.file_name() == ci::MANIFEST_FILE {
                let in_job_dir = dir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| TARGETS.contains(&name));
                if in_job_dir {
                    found.push(path);
                } else {
                    skipped.push(path);
                }
            }
        }
    }
    found.sort();
    skipped.sort();
    (found, skipped)
}

/// The changed paths a manifest records for the working tree it was made from: empty when the
/// tree was clean, or why the manifest does not say.
fn recorded_dirty_paths(value: &Value) -> Result<Vec<String>, String> {
    let git = &value["host"]["git"];
    match git["dirty"].as_bool() {
        Some(false) => Ok(Vec::new()),
        Some(true) => {
            let paths: Vec<String> = git["dirty_paths"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|entry| format!("{} {}", str_of(entry, "status"), str_of(entry, "path")))
                .collect();
            if paths.is_empty() {
                Err("records a dirty working tree without its changed paths".to_owned())
            } else {
                Ok(paths)
            }
        }
        None => Err("does not record whether its working tree was clean".to_owned()),
    }
}

/// Downloads every artifact of CI run `id` into `dir` with `gh run download`, and reads the run's
/// summary with `gh run view`. Both only read from GitHub.
fn download_run(ws: &Workspace, id: &str, dir: &Path) -> Result<Value, String> {
    if dir.exists() {
        fs::remove_dir_all(dir).map_err(|e| format!("cannot clear {}: {e}", dir.display()))?;
    }
    let mut view = Command::new("gh");
    view.args([
        "run",
        "view",
        id,
        "--json",
        "databaseId,headSha,headBranch,event,status,conclusion,workflowName,url",
    ])
    .current_dir(&ws.root);
    let out = cargo::output(&mut view)?;
    if !out.status.success() {
        return Err(format!(
            "gh run view {id} exited with {:?}: {}",
            out.status.code(),
            cargo::text(&out.stderr).trim()
        ));
    }
    let summary: Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("gh run view {id} printed no JSON: {e}"))?;
    let mut download = Command::new("gh");
    download
        .args(["run", "download", id, "--dir"])
        .arg(dir)
        .current_dir(&ws.root);
    let out = cargo::output(&mut download)?;
    if !out.status.success() {
        return Err(format!(
            "gh run download {id} exited with {:?}: {}",
            out.status.code(),
            cargo::text(&out.stderr).trim()
        ));
    }
    Ok(summary)
}

/// One manifest as read.
struct Read {
    path: String,
    value: Value,
}

fn str_of<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}

/// The audit of one package entry: (row for the record, problems).
fn audit_package(target: &str, package: &str, entry: Option<&Value>) -> (Value, Vec<String>) {
    let mut problems = Vec::new();
    let Some(entry) = entry else {
        problems.push(format!(
            "{target}: {package} is not in the executed-target manifest"
        ));
        return (json!({ "package": package, "present": false }), problems);
    };
    let check = |run: &Value, on: &str, problems: &mut Vec<String>| {
        if run["built"] != true {
            problems.push(format!("{on}: {package} was not built"));
        }
        if run["harness_ran"] != true {
            problems.push(format!(
                "{on}: {package}'s test harness did not run every target cargo built for it"
            ));
        }
        if run["status"] == "failed" {
            problems.push(format!("{on}: {package} failed"));
        }
        if !matches!(str_of(run, "status"), "passed" | "zero_tests" | "failed") {
            problems.push(format!("{on}: {package} has status {:?}", run["status"]));
        }
        json!({
            "built": run["built"],
            "harness_ran": run["harness_ran"],
            "status": run["status"],
            "tests_run": run["tests_run"],
            "passed": run["passed"],
            "failed": run["failed"],
            "browser_started": run.get("browser_started"),
            "runner_reported_no_tests_for": run.get("runner_reported_no_tests_for"),
        })
    };
    let row = if target == "wasm" {
        let mut browsers = Map::new();
        for browser in BROWSERS {
            let run = entry["browsers"]
                .as_array()
                .and_then(|runs| runs.iter().find(|run| run["browser"] == browser));
            match run {
                Some(run) => {
                    browsers.insert(
                        browser.to_owned(),
                        check(run, &format!("wasm@{browser}"), &mut problems),
                    );
                }
                None => problems.push(format!("wasm: {package} has no run in {browser}")),
            }
        }
        json!({
            "package": package,
            "present": true,
            "role": entry["role"],
            "status": entry["status"],
            "browsers": browsers,
        })
    } else {
        let mut row = check(entry, target, &mut problems);
        row["package"] = json!(package);
        row["present"] = json!(true);
        row["role"] = entry["role"].clone();
        row
    };
    (row, problems)
}

pub fn run(raw: &[String]) -> ExitCode {
    let args = match parse_args(raw) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("xtask ci-audit: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("xtask ci-audit: no current directory: {e}");
            return ExitCode::from(1);
        }
    };
    let ws = match Workspace::load(&cwd) {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("xtask ci-audit: cannot read the workspace: {e}");
            return ExitCode::from(1);
        }
    };
    let out_dir = ws.root.join("target").join("ci-audit");
    if let Err(e) = fs::create_dir_all(&out_dir) {
        eprintln!("xtask ci-audit: cannot create {}: {e}", out_dir.display());
        return ExitCode::from(1);
    }
    let mut problems = Vec::new();
    let mut sources = Vec::new();
    let mut files = Vec::new();
    let print_skipped = |skipped: &[PathBuf]| {
        for path in skipped {
            println!(
                "AUDIT skipped {}: named {} but not directly inside a windows/, linux/ or wasm/ directory, where `cargo xtask ci <job>` writes its manifest; not read",
                ws.rel(path),
                ci::MANIFEST_FILE
            );
        }
    };
    for id in &args.runs {
        let dir = out_dir.join(format!("run-{id}"));
        match download_run(&ws, id, &dir) {
            Ok(summary) => {
                let (found, skipped) = manifest_files(&dir);
                println!(
                    "AUDIT run {id}: {} manifests in {}",
                    found.len(),
                    ws.rel(&dir)
                );
                print_skipped(&skipped);
                sources.push(json!({
                    "run": id,
                    "summary": summary,
                    "dir": ws.rel(&dir),
                    "manifests": found.iter().map(|path| ws.rel(path)).collect::<Vec<_>>(),
                    "skipped": skipped.iter().map(|path| ws.rel(path)).collect::<Vec<_>>(),
                }));
                files.extend(found);
            }
            Err(e) => problems.push(format!("run {id}: {e}")),
        }
    }
    for from in &args.froms {
        let path = cwd.join(from);
        // A source that holds no manifest adds nothing; each required target must still be
        // covered by exactly one manifest below. A path that does not exist is a mistake.
        if !path.exists() {
            problems.push(format!("--from {} does not exist", from.display()));
        }
        let (found, skipped) = manifest_files(&path);
        println!(
            "AUDIT from {}: {} manifests, {} files skipped",
            from.display(),
            found.len(),
            skipped.len()
        );
        print_skipped(&skipped);
        sources.push(json!({
            "from": from.to_string_lossy(),
            "manifests": found.iter().map(|path| ws.rel(path)).collect::<Vec<_>>(),
            "skipped": skipped.iter().map(|path| ws.rel(path)).collect::<Vec<_>>(),
        }));
        files.extend(found);
    }
    let mut seen = BTreeSet::new();
    let mut manifests = Vec::new();
    for file in files {
        let canonical = fs::canonicalize(&file).unwrap_or_else(|_| file.clone());
        if !seen.insert(canonical) {
            continue;
        }
        let shown = ws.rel(&file);
        match fs::read_to_string(&file)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str::<Value>(&text).map_err(|e| e.to_string()))
        {
            Ok(value) if value["format"] == ci::MANIFEST_FORMAT => {
                manifests.push(Read { path: shown, value })
            }
            Ok(value) => problems.push(format!(
                "{shown} has format {}, not {}",
                value["format"],
                ci::MANIFEST_FORMAT
            )),
            Err(e) => problems.push(format!("{shown} cannot be read: {e}")),
        }
    }

    let head = cargo::git(&ws.root, &["rev-parse", "HEAD"])
        .map(|head| head.trim().to_owned())
        .unwrap_or_else(|e| {
            problems.push(format!("this checkout's commit: {e}"));
            String::new()
        });
    let portable: Vec<(&str, String)> = ws
        .members
        .iter()
        .filter(|member| member.is_portable())
        .map(|member| (member.name.as_str(), ws.rel(&member.dir)))
        .collect();
    let app_cores: Vec<&str> = ws
        .members
        .iter()
        .filter(|member| ws.rel(&member.dir).starts_with("apps/"))
        .map(|member| member.name.as_str())
        .collect();
    if app_cores.len() < 2 {
        problems.push(format!(
            "the workspace has {} app cores under apps/, expected both (dna-onecalc and dna-calc)",
            app_cores.len()
        ));
    }
    for core in &app_cores {
        if !portable.iter().any(|(name, _)| name == core) {
            problems.push(format!("app core {core} is not portable"));
        }
    }

    let mut targets = Map::new();
    let (mut with_tests, mut zero) = (0, 0);
    for target in &args.required {
        let covering: Vec<&Read> = manifests
            .iter()
            .filter(|read| read.value["target"] == target.as_str())
            .collect();
        let read = match covering.as_slice() {
            [read] => *read,
            [] => {
                problems.push(format!(
                    "no executed-target manifest covers {target}; the sources hold {}",
                    manifests
                        .iter()
                        .map(|read| format!("{} ({})", read.path, read.value["target"]))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                continue;
            }
            many => {
                problems.push(format!(
                    "{} manifests cover {target}: {}",
                    many.len(),
                    many.iter()
                        .map(|read| read.path.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                continue;
            }
        };
        let value = &read.value;
        if value["result"] != "passed" || value["exit_code"] != 0 {
            problems.push(format!(
                "{target}: the job's result is {} (exit code {}): {}",
                value["result"], value["exit_code"], value["problems"]
            ));
        }
        let manifest_head = value["host"]["git"]["head"].as_str().unwrap_or_default();
        if manifest_head != head {
            problems.push(format!(
                "{target}: the manifest was made at commit {manifest_head:?}, but this checkout is at {head:?}"
            ));
        }
        let dirty = recorded_dirty_paths(value);
        match &dirty {
            Ok(paths) if paths.is_empty() => {}
            Ok(paths) => {
                println!(
                    "AUDIT dirty {target}: {} was made from a working tree with {} changed paths, {}: {}",
                    read.path,
                    paths.len(),
                    if args.allow_dirty {
                        "accepted because --allow-dirty is given"
                    } else {
                        "refused without --allow-dirty"
                    },
                    paths.join(", ")
                );
                if !args.allow_dirty {
                    problems.push(format!(
                        "{target}: {} was made from a dirty working tree ({} changed paths, listed on the AUDIT dirty line); only --allow-dirty accepts it",
                        read.path,
                        paths.len()
                    ));
                }
            }
            Err(e) => {
                println!("AUDIT dirty {target}: {} {e}; refused", read.path);
                problems.push(format!("{target}: {} {e}", read.path));
            }
        }
        let entries = value["packages"].as_array().cloned().unwrap_or_default();
        let mut rows = Vec::new();
        for (package, _) in &portable {
            let entry = entries.iter().find(|entry| entry["package"] == *package);
            let (row, found) = audit_package(target, package, entry);
            let shown_status = |run: &Value| -> String {
                let mut text = format!(
                    "{}: {} tests run, {} passed",
                    str_of(run, "status"),
                    run["tests_run"],
                    run["passed"]
                );
                let no_tests = run["runner_reported_no_tests_for"]
                    .as_array()
                    .map(|targets| targets.iter().filter_map(Value::as_str).collect::<Vec<_>>())
                    .unwrap_or_default();
                match run["browser_started"].as_bool() {
                    Some(true) => text.push_str(", the browser started"),
                    Some(false) if !no_tests.is_empty() => text.push_str(&format!(
                        ", wasm-bindgen-test-runner reported `no tests to run!` for {} and started no browser",
                        no_tests.join(", ")
                    )),
                    Some(false) => text.push_str(", no browser started"),
                    None => {}
                }
                text
            };
            let line = if target == "wasm" {
                BROWSERS
                    .iter()
                    .map(|browser| {
                        format!("{browser} {}", shown_status(&row["browsers"][*browser]))
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            } else {
                shown_status(&row)
            };
            println!(
                "AUDIT {target} {package} ({}): {line}",
                str_of(&row, "role")
            );
            let statuses: Vec<&str> = if target == "wasm" {
                BROWSERS
                    .iter()
                    .map(|browser| str_of(&row["browsers"][*browser], "status"))
                    .collect()
            } else {
                vec![str_of(&row, "status")]
            };
            if found.is_empty() {
                if statuses.iter().all(|status| *status == "passed") {
                    with_tests += 1;
                } else if statuses.iter().all(|status| *status != "failed") {
                    zero += 1;
                }
            }
            problems.extend(found);
            rows.push(row);
        }
        let mut record = json!({
            "manifest": read.path,
            "job_result": value["result"],
            "head": manifest_head,
            "dirty": value["host"]["git"]["dirty"],
            "dirty_paths": dirty.as_ref().ok(),
            "dirty_accepted_by_allow_dirty": args.allow_dirty && dirty.as_ref().is_ok_and(|paths| !paths.is_empty()),
            "rustc": value["host"]["rustc"],
            "packages": rows,
        });
        if target == "wasm" {
            let fixtures = value["fixtures"].as_array().cloned().unwrap_or_default();
            let mut probe = Map::new();
            for browser in BROWSERS {
                let run = fixtures.iter().find(|fixture| {
                    fixture["fixture"] == ci::PROBE_FIXTURE && fixture["browser"] == browser
                });
                match run {
                    Some(run) if run["outcome"] == "passed" => {
                        println!(
                            "AUDIT wasm {} in {browser}: passed, user agent {}",
                            ci::PROBE_FIXTURE,
                            run["user_agent"]
                        );
                    }
                    Some(run) => problems.push(format!(
                        "wasm: the {} did not pass in {browser}: {}",
                        ci::PROBE_FIXTURE,
                        run["reason"]
                    )),
                    None => problems.push(format!(
                        "wasm: the manifest has no {} run in {browser}",
                        ci::PROBE_FIXTURE
                    )),
                }
                probe.insert(
                    browser.to_owned(),
                    run.map_or(Value::Null, |run| {
                        json!({ "outcome": run["outcome"], "user_agent": run["user_agent"], "runner_banner": run["runner_banner"] })
                    }),
                );
            }
            if fixtures
                .iter()
                .any(|fixture| fixture["fixture"] == ci::INJECTED_FIXTURE)
            {
                problems.push(format!(
                    "wasm: the manifest holds the {} fixture, so it records a deliberately failed run",
                    ci::INJECTED_FIXTURE
                ));
            }
            record["route_probe"] = Value::Object(probe);
            record["tools"] = value["tools"].clone();
        }
        targets.insert(target.clone(), record);
    }

    for problem in &problems {
        println!("AUDIT problem: {problem}");
    }
    let result = if problems.is_empty() {
        "passed"
    } else {
        "failed"
    };
    let audit = json!({
        "format": "dnacalc-ci-audit/1",
        "command": std::iter::once("cargo xtask ci-audit".to_owned()).chain(raw.iter().cloned()).collect::<Vec<_>>(),
        "rule": "for each required target exactly one manifest (an executed-targets.json directly inside a windows/, linux/ or wasm/ directory, or a file named by its path), whose job passed, made at this checkout's commit from a clean working tree (a dirty one only with --allow-dirty), in which every portable package (both app cores among them) was built and had its test harness run on every target cargo built for it without a failure, on wasm under wasm-bindgen-test-runner for Chrome and for Firefox with the route probe passed in both browsers; zero-test packages are counted apart and never as passed, and on wasm a package for which wasm-bindgen-test-runner reported no tests started no browser",
        "allow_dirty": args.allow_dirty,
        "required_targets": args.required,
        "sources": sources,
        "checkout_head": head,
        "expected_packages": portable.iter().map(|(name, dir)| json!({ "package": name, "dir": dir })).collect::<Vec<_>>(),
        "app_cores": app_cores,
        "targets": targets,
        "counts": {
            "package_targets_with_tests_passed": with_tests,
            "package_targets_with_zero_tests": zero,
        },
        "problems": problems,
        "result": result,
    });
    let path = out_dir.join("audit.json");
    let written = fs::write(
        &path,
        serde_json::to_string_pretty(&audit).unwrap_or_default() + "\n",
    );
    let exit_code: u8 = match (&written, result) {
        (Ok(()), "passed") => 0,
        _ => 1,
    };
    if let Err(e) = written {
        println!("AUDIT problem: cannot write {}: {e}", path.display());
    }
    println!(
        "AUDIT RESULT {result}: targets {}; {} portable packages expected per target; {with_tests} package-targets with tests passed, {zero} with zero tests (not passes); {} problems; record {}; exit code {exit_code}",
        args.required.join(","),
        portable.len(),
        audit["problems"].as_array().map_or(0, Vec::len),
        ws.rel(&path)
    );
    ExitCode::from(exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Result<Args, String> {
        parse_args(&list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>())
    }

    #[test]
    fn arguments_are_checked() {
        let parsed = args(&[
            "--require-executed-targets",
            "windows,linux,wasm",
            "--run",
            "123",
            "--from",
            "x",
        ])
        .unwrap();
        assert_eq!(parsed.required, ["windows", "linux", "wasm"]);
        assert_eq!(parsed.runs, ["123"]);
        assert!(!parsed.allow_dirty);
        let dirty = args(&[
            "--require-executed-targets",
            "wasm",
            "--from",
            "x",
            "--allow-dirty",
        ]);
        assert!(dirty.unwrap().allow_dirty);
        assert!(args(&["--require-executed-targets", "windows,macos", "--from", "x"]).is_err());
        assert!(args(&["--require-executed-targets", "wasm,wasm", "--from", "x"]).is_err());
        assert!(args(&["--require-executed-targets", "wasm"]).is_err());
        assert!(args(&["--from", "x"]).is_err());
        assert!(args(&["--require-executed-targets", "wasm", "--run", "12a"]).is_err());
    }

    #[test]
    fn packages_are_audited_per_browser() {
        let run = |browser: &str, harness: bool, status: &str| json!({ "browser": browser, "built": true, "harness_ran": harness, "status": status, "tests_run": 0, "passed": 0 });
        let entry = json!({
            "package": "p",
            "role": "library",
            "status": "zero_tests",
            "browsers": [run("chromium", true, "zero_tests"), run("firefox", true, "zero_tests")],
        });
        let (_, problems) = audit_package("wasm", "p", Some(&entry));
        assert!(problems.is_empty(), "{problems:?}");
        let half = json!({ "package": "p", "browsers": [run("chromium", true, "passed")] });
        let (_, problems) = audit_package("wasm", "p", Some(&half));
        assert_eq!(problems, ["wasm: p has no run in firefox"]);
        let failed = json!({ "package": "p", "browsers": [run("chromium", false, "failed"), run("firefox", true, "passed")] });
        let (_, problems) = audit_package("wasm", "p", Some(&failed));
        assert_eq!(problems.len(), 2, "{problems:?}");
        let (_, problems) = audit_package("windows", "p", None);
        assert_eq!(
            problems,
            ["windows: p is not in the executed-target manifest"]
        );
        let native =
            json!({ "package": "p", "built": false, "harness_ran": true, "status": "zero_tests" });
        let (_, problems) = audit_package("linux", "p", Some(&native));
        assert_eq!(problems, ["linux: p was not built"]);
    }

    #[test]
    fn only_job_directories_hold_manifests() {
        let root = std::env::temp_dir().join(format!("xtask-ci-audit-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let job = root.join("ci").join("wasm");
        let evidence = root.join("accept").join("case").join("05-altered");
        for dir in [&job, &evidence] {
            fs::create_dir_all(dir).unwrap();
            fs::write(dir.join(ci::MANIFEST_FILE), "{}").unwrap();
        }
        let (found, skipped) = manifest_files(&root);
        assert_eq!(found, [job.join(ci::MANIFEST_FILE)]);
        assert_eq!(skipped, [evidence.join(ci::MANIFEST_FILE)]);
        // A file named by its own path is read whatever its directory.
        let file = evidence.join(ci::MANIFEST_FILE);
        assert_eq!(manifest_files(&file), (vec![file.clone()], Vec::new()));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn dirty_trees_are_read_with_their_paths() {
        let clean = json!({ "host": { "git": { "dirty": false, "dirty_paths": [] } } });
        assert_eq!(recorded_dirty_paths(&clean), Ok(Vec::new()));
        let dirty = json!({ "host": { "git": { "dirty": true, "dirty_paths": [
            { "status": "M", "path": "README.md" },
            { "status": "??", "path": "tools/xtask/src/ci.rs" },
        ] } } });
        assert_eq!(
            recorded_dirty_paths(&dirty),
            Ok(vec![
                "M README.md".to_owned(),
                "?? tools/xtask/src/ci.rs".to_owned()
            ])
        );
        let unlisted = json!({ "host": { "git": { "dirty": true } } });
        assert!(recorded_dirty_paths(&unlisted).is_err());
        let unknown = json!({ "host": { "git": { "dirty": null } } });
        assert!(recorded_dirty_paths(&unknown).is_err());
    }
}
