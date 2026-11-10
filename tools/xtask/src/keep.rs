//! Periodic checks in a dedicated, read-mostly master clone. Only tracker changes are pushed.
use crate::cargo::{self, Workspace};
use crate::check::{self, Failure, Options, Report};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

pub const USAGE: &str = "usage: cargo xtask keep --once";
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn home() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or("no home directory".into())
}
fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let temp = path.with_extension("tmp");
    fs::write(
        &temp,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    // Windows rename cannot replace an existing destination.
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    fs::rename(temp, path).map_err(|e| e.to_string())
}
fn read_json(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
fn list(root: &Path, label: &str) -> Result<Value, String> {
    serde_json::from_str(&check::br(root, &["list", "--label", label, "--json"])?)
        .map_err(|e| e.to_string())
}
fn status(text: &str) -> Result<(), String> {
    let dir = home()?.join("keeper");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(
        dir.join("status.txt"),
        format!("{} {} {text}\n", now(), check::platform()),
    )
    .map_err(|e| e.to_string())
}
fn own(issue: &Value) -> bool {
    issue["owner"].as_str() == Some(&format!("keeper-{}", check::platform()))
        && matches!(issue["status"].as_str(), Some("open" | "in_progress"))
}
fn log_trailer(root: &Path, sha: &str) -> String {
    cargo::git(
        root,
        &[
            "show",
            "-s",
            "--format=%(trailers:key=Agent-Log,valueonly)",
            sha,
        ],
    )
    .unwrap_or_default()
    .trim()
    .to_owned()
}
fn attribution(root: &Path, sha: &str) -> String {
    let trailer = log_trailer(root, sha);
    if trailer.is_empty() {
        "missing (historical commit; inspect git show for its author)".into()
    } else {
        trailer
    }
}
fn failing_line(failure: &Failure) -> String {
    format!(
        "failing: {} {} {} {}",
        check::platform(),
        failure.package,
        failure.target,
        failure.name
    )
}
fn record_flaky(ws: &Workspace, failure: &Failure, sha: &str) -> Result<bool, String> {
    let marker = failing_line(failure);
    refresh_tracker(&ws.root)?;
    let existing = list(&ws.root, "flaky")?;
    if check::issues(&existing).iter().any(|i| {
        own(i)
            && i["description"]
                .as_str()
                .unwrap_or("")
                .lines()
                .any(|l| l == marker)
    }) {
        return Ok(false);
    }
    let body = format!(
        "Keeper retry passed at {sha}.\nAgent-Log: {}\n{marker}",
        attribution(&ws.root, sha)
    );
    check::br(
        &ws.root,
        &[
            "create",
            "--title",
            &format!("Flaky {} {}", check::platform(), failure.name),
            "--type",
            "bug",
            "--priority",
            "2",
            "--labels",
            "flaky,keeper",
            "--owner",
            &format!("keeper-{}", check::platform()),
            "--assignee",
            &format!("keeper-{}", check::platform()),
            "--description",
            &body,
        ],
    )?;
    Ok(true)
}

fn record_flaky_command(ws: &Workspace, name: &str, sha: &str) -> Result<bool, String> {
    let marker = format!("command: {} {name}", check::platform());
    refresh_tracker(&ws.root)?;
    let existing = list(&ws.root, "flaky")?;
    if check::issues(&existing).iter().any(|i| {
        own(i)
            && i["description"]
                .as_str()
                .unwrap_or("")
                .lines()
                .any(|l| l == marker)
    }) {
        return Ok(false);
    }
    let body = format!(
        "Keeper command failed then passed its retry at {sha}.\n{marker}\nAgent-Log: {}",
        attribution(&ws.root, sha)
    );
    check::br(
        &ws.root,
        &[
            "create",
            "--title",
            &format!("Flaky {} {name} command", check::platform()),
            "--type",
            "bug",
            "--priority",
            "2",
            "--labels",
            "flaky,keeper",
            "--owner",
            &format!("keeper-{}", check::platform()),
            "--assignee",
            &format!("keeper-{}", check::platform()),
            "--description",
            &body,
        ],
    )?;
    Ok(true)
}

fn has_health_drift(new_report: &Value, previous: &Value) -> bool {
    if previous.is_null() {
        return false;
    }
    let Some(new_packages) = new_report["packages"].as_array() else {
        return false;
    };
    let Some(old_packages) = previous["packages"].as_array() else {
        return false;
    };
    for new_pkg in new_packages {
        let name = new_pkg["package"].as_str().unwrap_or_default();
        if let Some(old_pkg) = old_packages.iter().find(|p| p["package"] == name) {
            for metric in [
                "rustfmt",
                "leptosfmt",
                "lines_over_160",
                "redundant_clone",
                "needless_pass_by_value",
                "clone_on_copy",
                "unnecessary_to_owned",
            ] {
                let new_val = new_pkg[metric].as_u64().unwrap_or(0);
                let old_val = old_pkg[metric].as_u64().unwrap_or(0);
                if new_val > old_val {
                    return true;
                }
            }
        } else {
            for metric in [
                "rustfmt",
                "leptosfmt",
                "lines_over_160",
                "redundant_clone",
                "needless_pass_by_value",
                "clone_on_copy",
                "unnecessary_to_owned",
            ] {
                if new_pkg[metric].as_u64().unwrap_or(0) > 0 {
                    return true;
                }
            }
        }
    }
    false
}

fn update_health_drift_bead(ws: &Workspace, report: &Value) -> Result<bool, String> {
    let table = report["table"].as_str().unwrap_or("");
    let body = format!("Code health drift detected during weekly review:\n\n{table}");
    refresh_tracker(&ws.root)?;
    let existing = list(&ws.root, "code-health")?;
    let found = check::issues(&existing)
        .into_iter()
        .find(|i| own(i) && i["title"].as_str() == Some("Code health drift"));
    if let Some(issue) = found {
        let id = issue["id"].as_str().ok_or("bead has no id")?;
        check::br(&ws.root, &["update", id, "--description", &body, "--force"])?;
        Ok(true)
    } else {
        check::br(
            &ws.root,
            &[
                "create",
                "--title",
                "Code health drift",
                "--type",
                "task",
                "--priority",
                "2",
                "--labels",
                "code-health,keeper",
                "--owner",
                &format!("keeper-{}", check::platform()),
                "--assignee",
                &format!("keeper-{}", check::platform()),
                "--description",
                &body,
            ],
        )?;
        Ok(true)
    }
}

/// Retry the exact test through its real Cargo target. Zero tests is unavailable, never green.
fn retry(ws: &Workspace, failure: &Failure, log: &Path) -> Result<Option<bool>, String> {
    retry_probe(ws, &failure.json(), log)
}
fn build_outcome(code: Option<i32>, text: &str) -> Option<bool> {
    if code == Some(0) {
        Some(true)
    } else if code == Some(101)
        && [
            "could not compile",
            "failed to parse manifest",
            "failed to load manifest",
            "invalid table header",
        ]
        .iter()
        .any(|s| text.contains(s))
    {
        Some(false)
    } else {
        None
    }
}

fn check_master(root: &Path, options: &Options) -> Result<Report, String> {
    match Workspace::load(root) {
        Ok(ws) => check::execute(&ws, options),
        Err(error) => {
            let dir = root.join("target/check");
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let log = dir.join(format!("metadata-{}.log", now()));
            fs::write(&log, format!("cargo metadata failed: {error}\n"))
                .map_err(|e| e.to_string())?;
            Ok(Report {
                passed: false,
                failures: Vec::new(),
                errors: vec!["workspace".into()],
                identified_test_failures: false,
                log,
                seconds: 0.0,
            })
        }
    }
}

fn retry_probe(ws: &Workspace, probe: &Value, log: &Path) -> Result<Option<bool>, String> {
    if probe["command"] == "workspace" {
        let (code, text) = check::run_logged(
            &ws.root,
            Command::new(cargo::cargo_program()).args(["build", "--locked", "--workspace"]),
            log,
        )?;
        return Ok(build_outcome(code, &text));
    }
    let failure = Failure::from_json(probe).ok_or("invalid exact-test probe")?;
    let (code, text) = check::run_logged(&ws.root, &mut failure.command()?, log)?;
    let summary = crate::test_output::summarize(&text);
    if code == Some(0) && summary.counts.passed > 0 && summary.counts.failed == 0 {
        return Ok(Some(true));
    }
    if summary.counts.failed > 0 {
        return Ok(Some(false));
    }
    Ok(None)
}
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Bisect executes the same exact test at each historical checkout. The probe returns 125 for
/// build failures or absent tests, so an unavailable case cannot become a false good commit.
fn bisect(
    ws: &Workspace,
    probe: &Value,
    good: &str,
    bad: &str,
    dir: &Path,
) -> Result<String, String> {
    let probe_file = dir.join("probe.json");
    write_json(&probe_file, probe)?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let script = dir.join("bisect.sh");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\n{} keep --probe {}\n",
            quote(&exe.to_string_lossy().replace('\\', "/")),
            quote(&probe_file.to_string_lossy().replace('\\', "/"))
        ),
    )
    .map_err(|e| e.to_string())?;
    // Confirm both endpoints with the exact same predicate used by the bisection.
    cargo::git(&ws.root, &["bisect", "start", bad, good])?;
    let result = (|| {
        cargo::git(&ws.root, &["checkout", "--detach", good])?;
        if retry_probe(ws, probe, &dir.join("bisect-endpoints.log"))? != Some(true) {
            return Err(format!(
                "bisect good endpoint {good} does not pass the exact test"
            ));
        }
        cargo::git(&ws.root, &["checkout", "--detach", bad])?;
        if retry_probe(ws, probe, &dir.join("bisect-endpoints.log"))? != Some(false) {
            return Err(format!(
                "bisect bad endpoint {bad} does not fail the exact test"
            ));
        }
        // Restart after endpoint validation, so bisect chooses its first candidate itself.
        cargo::git(&ws.root, &["bisect", "reset"])?;
        cargo::git(&ws.root, &["bisect", "start", bad, good])?;
        let (code, text) = check::run_logged(
            &ws.root,
            Command::new("git")
                .args(["bisect", "run", "sh"])
                .arg(&script),
            &dir.join("bisect.log"),
        )?;
        if code != Some(0) {
            // The log may list candidate commits; the bead names the range, never a culprit.
            let _ = text;
            return Err("bisection could not identify one commit (see target/keep/bisect.log)".into());
        }
        cargo::git(&ws.root, &["rev-parse", "refs/bisect/bad"]).map(|s| s.trim().to_owned())
    })();
    let restored = cargo::git(&ws.root, &["bisect", "reset"]);
    restored?;
    result
}

/// A new keeper may start during an existing failure. Look backwards for an actually passing
/// endpoint rather than guessing that a parent was green. Keep this inside bisect's checkout scope.
fn find_good(ws: &Workspace, probe: &Value, bad: &str, dir: &Path) -> Result<String, String> {
    cargo::git(&ws.root, &["bisect", "start", bad])?;
    let result = (|| {
        for distance in [1, 2, 4, 8, 16, 32, 64, 128] {
            let Ok(candidate) = cargo::git(
                &ws.root,
                &["rev-parse", "--verify", &format!("{bad}~{distance}")],
            ) else {
                break;
            };
            let candidate = candidate.trim();
            cargo::git(&ws.root, &["checkout", "--detach", candidate])?;
            if retry_probe(ws, probe, &dir.join("endpoints.log"))? == Some(true) {
                return Ok(candidate.to_owned());
            }
        }
        Err("no passing exact-test endpoint in the preceding 128 first-parent commits".into())
    })();
    cargo::git(&ws.root, &["bisect", "reset"])?;
    result
}

/// The master-red text for a failure that bisection could not pin to one commit: the commit
/// range, and that no single culprit was found. It never names a commit as the cause.
fn no_culprit(good: Option<&str>, bad: &str, why: &str) -> String {
    let range = match good {
        Some(good) => format!("commit range {good}..{bad}"),
        None => format!("commits up to {bad} (no passing endpoint was found)"),
    };
    format!("no single culprit found in the {range}; no commit is named as the cause.\nreason: {why}")
}

/// Bisect one probe: the diagnosis text for the bead, and the culprit commit when one was found.
fn diagnose(
    ws: &Workspace,
    probe: &Value,
    good: Option<&str>,
    sha: &str,
    dir: &Path,
) -> (String, Option<String>) {
    let endpoint = match good {
        Some(good) => Ok(good.to_owned()),
        None => find_good(ws, probe, sha, dir),
    };
    let good = match endpoint {
        Ok(good) => good,
        Err(e) => return (no_culprit(None, sha, &e), None),
    };
    match bisect(ws, probe, &good, sha, dir) {
        Ok(first) => (
            format!(
                "first bad commit: {first}\nAgent-Log: {}",
                attribution(&ws.root, &first)
            ),
            Some(first),
        ),
        Err(e) => (no_culprit(Some(&good), sha, &e), None),
    }
}

/// Every tracker write starts here (AGENTS.md): pull, then import into br's database, so the
/// write sees what other lanes pushed while the keeper checked.
fn refresh_tracker(root: &Path) -> Result<(), String> {
    cargo::git(root, &["pull", "--ff-only"])?;
    check::br(root, &["sync", "--import-only"])?;
    Ok(())
}

fn red_bead(
    ws: &Workspace,
    report: &Report,
    sha: &str,
    good: Option<&str>,
    dir: &Path,
) -> Result<(String, bool), String> {
    let existing = list(&ws.root, "master-red")?;
    let prefix = format!("Keeper {} is red at {sha}.", check::platform());
    if let Some(issue) = check::issues(&existing).iter().find(|i| own(i)) {
        let description = issue["description"].as_str().unwrap_or("");
        let previous: Vec<_> = description
            .lines()
            .filter(|l| l.starts_with("failing: "))
            .map(str::to_owned)
            .collect();
        let current: Vec<_> = report.failures.iter().map(failing_line).collect();
        if description.starts_with(&prefix) && previous == current {
            return Ok((issue["id"].as_str().ok_or("bead has no id")?.into(), false));
        }
    }
    // Only a bisected culprit's log names an assignee; the tip commit is not the culprit.
    let mut assignee = String::new();
    let mut body = format!(
        "Keeper {} is red at {sha}. Fix forward; aim to repair within one hour.\n",
        check::platform()
    );
    let mut diagnosed = false;
    for failure in &report.failures {
        let (diagnosis, culprit) = diagnose(ws, &failure.json(), good, sha, dir);
        if let Some(first) = &culprit {
            let trailer = log_trailer(&ws.root, first);
            if !trailer.is_empty() {
                assignee = trailer;
            }
        }
        diagnosed = true;
        body.push_str(&format!("{}\n{diagnosis}\n", failing_line(failure)));
    }
    if report.failures.is_empty() && report.errors.iter().any(|e| e == "workspace") {
        let probe = json!({"command":"workspace"});
        let (diagnosis, culprit) = diagnose(ws, &probe, good, sha, dir);
        if let Some(first) = &culprit {
            let trailer = log_trailer(&ws.root, first);
            if !trailer.is_empty() {
                assignee = trailer;
            }
        }
        diagnosed = true;
        body.push_str(&format!("{diagnosis}\n"));
    }
    if report.failures.is_empty() {
        body.push_str(&format!("check errors: {:?}\n", report.errors));
        if !diagnosed {
            body.push_str(&no_culprit(good, sha, "these check errors have no exact test to bisect"));
            body.push('\n');
        }
    }
    if assignee.is_empty() {
        assignee = format!("keeper-{}", check::platform());
    }
    body.push_str(&format!("evidence: target/keep/{sha}.json\n"));
    // Bisecting can take long: pull and import again right before the write.
    refresh_tracker(&ws.root)?;
    let existing = list(&ws.root, "master-red")?;
    if let Some(issue) = check::issues(&existing).iter().find(|i| own(i)) {
        let id = issue["id"].as_str().ok_or("bead has no id")?;
        if issue["description"].as_str() != Some(&body) {
            check::br(
                &ws.root,
                &[
                    "update",
                    id,
                    "--assignee",
                    &assignee,
                    "--description",
                    &body,
                    // The keeper owns this bead and restates its whole body each round; a shorter
                    // body is the newer finding, not a loss, and br refuses that rewrite without it.
                    "--force",
                ],
            )?;
            Ok((id.into(), true))
        } else {
            Ok((id.into(), false))
        }
    } else {
        let id = check::br(
            &ws.root,
            &[
                "create",
                "--title",
                &format!("Master red on {} at {}", check::platform(), &sha[..8]),
                "--type",
                "bug",
                "--priority",
                "0",
                "--labels",
                "master-red,keeper",
                "--owner",
                &format!("keeper-{}", check::platform()),
                "--assignee",
                &assignee,
                "--description",
                &body,
                "--silent",
            ],
        )?;
        Ok((id.trim().into(), true))
    }
}
fn close_red(ws: &Workspace, sha: &str) -> Result<bool, String> {
    refresh_tracker(&ws.root)?;
    let existing = list(&ws.root, "master-red")?;
    let mut changed = false;
    for issue in check::issues(&existing).iter().filter(|i| own(i)) {
        let id = issue["id"].as_str().ok_or("bead has no id")?;
        check::br(
            &ws.root,
            &[
                "close",
                id,
                "--reason",
                &format!(
                    "Keeper check and any exact retries passed on {} at {sha}; evidence target/keep/{sha}.json",
                    check::platform()
                ),
            ],
        )?;
        changed = true;
    }
    Ok(changed)
}

/// Validate each outgoing commit against its first parent, including merge commits. A source
/// change brought in by a merge must not slip past the keeper's tracker-only write permission.
fn validate_outgoing(root: &Path) -> Result<(), String> {
    let pending = cargo::git(root, &["log", "--format=%H", "origin/master..HEAD"])?;
    for commit in pending.lines().filter(|s| !s.is_empty()) {
        let parents = cargo::git(root, &["rev-list", "--parents", "-n", "1", commit])?;
        let parent_rows: Vec<_> = parents.split_whitespace().skip(1).collect();
        let first_parent = parent_rows
            .first()
            .ok_or("keeper refuses an outgoing root commit")?;
        // A reconciliation merge may import source already present on origin/master. It must
        // add only tracker content relative to that fetched upstream; its outgoing ordinary
        // parents are checked separately below, so hidden source commits remain forbidden.
        let upstream_parent;
        let base = if parent_rows.len() > 1 {
            upstream_parent = parent_rows
                .iter()
                .find(|parent| {
                    cargo::git(
                        root,
                        &["merge-base", "--is-ancestor", parent, "origin/master"],
                    )
                    .is_ok()
                })
                .ok_or("keeper refuses a merge with no fetched upstream parent")?;
            *upstream_parent
        } else {
            first_parent
        };
        let paths = cargo::git(root, &["diff", "--no-renames", "--name-only", base, commit])?;
        if paths.lines().any(|p| !p.starts_with(".beads/")) {
            return Err(format!("keeper refuses outgoing source commit {commit}"));
        }
    }
    if cargo::git(
        root,
        &["merge-base", "--is-ancestor", "origin/master", "HEAD"],
    )
    .is_ok()
    {
        let paths = cargo::git(
            root,
            &[
                "diff",
                "--no-renames",
                "--name-only",
                "origin/master",
                "HEAD",
            ],
        )?;
        if paths.lines().any(|p| !p.starts_with(".beads/")) {
            return Err("keeper HEAD changes source relative to fetched upstream".into());
        }
    }
    Ok(())
}

fn tracker_message() -> String {
    format!(
        "master-red: {} keeper findings\n\nAgent-Log: {}",
        check::platform(),
        std::env::var("DNACALC_AGENT_LOG")
            .unwrap_or_else(|_| format!("keeper-{}:~/keeper/keeper.log", check::platform()))
    )
}

fn reconcile_tracker(root: &Path) -> Result<(), String> {
    let pulled = cargo::git(root, &["pull", "--no-rebase", "--no-commit"]);
    if let Err(error) = pulled {
        let conflicts = cargo::git(root, &["diff", "--name-only", "--diff-filter=U"])?;
        if conflicts.trim() != ".beads/issues.jsonl" {
            return Err(error);
        }
        // Import every remote row losslessly into br's existing DB, which still holds local
        // keeper edits. Only br writes the reconciled tracker export, including conflict repair.
        let incoming = root.join("target/keep/incoming.jsonl");
        fs::write(
            &incoming,
            cargo::git(root, &["show", "origin/master:.beads/issues.jsonl"])?,
        )
        .map_err(|e| e.to_string())?;
        let out = Command::new("br")
            .current_dir(root)
            .env("BEADS_JSONL", &incoming)
            .args([
                "sync",
                "--reconcile",
                "--allow-external-jsonl",
                "--no-auto-import",
                "--no-auto-flush",
            ])
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!(
                "tracker reconciliation: {}",
                cargo::text(&out.stderr)
            ));
        }
        check::br(
            root,
            &["sync", "--flush-only", "--force", "--no-auto-import"],
        )?;
        cargo::git(root, &["add", "--", ".beads/issues.jsonl"])?;
    }
    if cargo::git(root, &["rev-parse", "--verify", "MERGE_HEAD"]).is_ok() {
        cargo::git(root, &["commit", "-m", &tracker_message()])?;
    }
    Ok(())
}

fn push_pending(root: &Path) -> Result<(), String> {
    for _ in 0..3 {
        validate_outgoing(root)?;
        if cargo::git(root, &["push", "origin", "master"]).is_ok() {
            return Ok(());
        }
        reconcile_tracker(root)?;
    }
    Err("keeper tracker push still rejected after three attempts".into())
}

/// Stage only br's export. Validate again before every push, including recovery after an outage.
fn push_tracker(ws: &Workspace) -> Result<(), String> {
    check::br(&ws.root, &["sync", "--flush-only"])?;
    cargo::git(&ws.root, &["add", "--", ".beads/"])?;
    let staged = cargo::git(&ws.root, &["diff", "--cached", "--name-only"])?;
    if staged.trim().is_empty() {
        return Ok(());
    }
    if staged.lines().any(|p| !p.starts_with(".beads/")) {
        return Err("keeper refuses to commit changes outside the tracker".into());
    }
    cargo::git(&ws.root, &["commit", "-m", &tracker_message()])?;
    push_pending(&ws.root)
}

fn full_due(state: &Value, commits: u64, time: u64) -> bool {
    state["last_full"].as_str().is_none()
        || commits >= 20
        || time.saturating_sub(state["full_at"].as_u64().unwrap_or(0)) >= 7200
}

fn trim_records(dir: &Path, extension: &str, retain: usize) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    let mut paths: Vec<_> = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == extension)
                && p.file_name()
                    .is_some_and(|n| n != "state.json" && n != "probe.json" && n != "results.json")
        })
        .collect();
    paths.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok());
    let remove = paths.len().saturating_sub(retain);
    for path in paths.into_iter().take(remove) {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn once(ws: &Workspace) -> Result<bool, String> {
    let started = std::time::Instant::now();
    if cargo::git(&ws.root, &["branch", "--show-current"])?.trim() != "master" {
        return Err("keeper requires master".into());
    }
    let dirty = cargo::git(&ws.root, &["status", "--porcelain"])?;
    if !dirty.trim().is_empty() {
        return Err("keeper requires a clean clone; preserve and inspect its local changes".into());
    }
    // Pending tracker commits from a prior network outage may be pushed, source commits may not.
    validate_outgoing(&ws.root)?;
    let pending = cargo::git(&ws.root, &["log", "--format=%H", "origin/master..HEAD"])?;
    if !pending.trim().is_empty() {
        push_pending(&ws.root)?;
    }
    // Bring br's database up to the pulled tracker; otherwise a later flush of a keeper bead is
    // refused ("Refusing to export stale database: JSONL contains issues absent from the database").
    refresh_tracker(&ws.root)?;
    let updated_ws = Workspace::load(&ws.root).unwrap_or_else(|_| Workspace {
        root: ws.root.clone(),
        members: Vec::new(),
    });
    let ws = &updated_ws;
    let sha = cargo::git(&ws.root, &["rev-parse", "HEAD"])?
        .trim()
        .to_owned();
    let dir = ws.root.join("target/keep");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    trim_records(&ws.root.join("target/check"), "log", 64)?;
    trim_records(&dir, "json", 200)?;
    for name in [
        "retry.log",
        "probe.log",
        "bisect.log",
        "bisect-endpoints.log",
        "endpoints.log",
        "sweep.log",
    ] {
        let log = dir.join(name);
        if fs::metadata(&log).is_ok_and(|m| m.len() > 16 * 1024 * 1024) {
            let previous = log.with_extension("previous.log");
            if previous.exists() {
                fs::remove_file(&previous).map_err(|e| e.to_string())?;
            }
            fs::rename(&log, previous).map_err(|e| e.to_string())?;
        }
    }
    let state_file = dir.join("state.json");
    let mut state = if state_file.exists() {
        read_json(&state_file)?
    } else {
        json!({})
    };
    let commits = if let Some(full) = state["last_full"].as_str() {
        cargo::git(
            &ws.root,
            &["rev-list", "--count", &format!("{full}..{sha}")],
        )?
        .trim()
        .parse()
        .map_err(|e| format!("commit count: {e}"))?
    } else {
        0
    };
    let full = full_due(&state, commits, now());
    let sweep_due = cfg!(windows)
        && (state["sweep_green"] == false
            || now().saturating_sub(state["sweep_at"].as_u64().unwrap_or(0)) >= 86400);
    if !full && !sweep_due && state["last_checked"].as_str() == Some(&sha) && state["green"] == true
    {
        status(&format!("green {sha}; unchanged"))?;
        return Ok(true);
    }
    status(&format!("checking {sha}; full={full}"))?;
    let good = state["last_green"].as_str().map(str::to_owned);
    let options = Options {
        all: full,
        dependents: !full,
        base: good.clone(),
        keeper: true,
    };
    let mut report = check_master(&ws.root, &options)?;
    let initial_check = report.json();
    let mut retries = Vec::new();
    let mut flaky_commands = Vec::new();
    let mut tracker_changed = false;
    let mut flaky = Vec::new();
    if !report.passed {
        if report.errors == ["tests"] && report.identified_test_failures {
            let mut retries_passed = true;
            let mut persistent = Vec::new();
            for failure in &report.failures {
                let outcome = retry(ws, failure, &dir.join("retry.log"))?;
                retries.push(json!({"failure":failure.json(),"passed":outcome}));
                if outcome == Some(true) {
                    flaky.push(failure.clone());
                } else {
                    retries_passed = false;
                    persistent.push(failure.clone());
                }
            }
            report.failures = persistent;
            report.passed = retries_passed;
            if retries_passed {
                report.errors.clear();
            }
        } else {
            // Non-test failures have no exact runnable test, so repeat the same check once.
            report = check_master(&ws.root, &options)?;
            retries.push(json!({"command":"check","passed":report.passed}));
            if report.passed {
                flaky_commands.push("check");
            }
        }
    }
    if report.passed && sweep_due {
        let mut sweep_passed = false;
        for attempt in 0..2 {
            // The keeper's ff-only pull may change the launcher itself. Build the harness in
            // target/debug and run its private copy, leaving target/xtask's Windows executable alone.
            let executable = crate::harness::profile_executable(ws, "dev")?;
            let _copy = crate::harness::CopyCleanup(executable.clone());
            let (code, _) = check::run_logged(
                &ws.root,
                &mut crate::harness::command(&executable, &["sweep".into()]),
                &report.log,
            )?;
            if code == Some(0) {
                sweep_passed = true;
                if attempt > 0 {
                    flaky_commands.push("sweep");
                }
                break;
            }
        }
        state["sweep_at"] = json!(now());
        state["sweep_green"] = json!(sweep_passed);
        if !sweep_passed {
            report.passed = false;
            report.errors.push("nightly sweep".into());
        } else if cfg!(windows) {
            let health_due = state["health_at"].as_u64().is_none()
                || now().saturating_sub(state["health_at"].as_u64().unwrap_or(0)) >= 7 * 86400;
            if health_due {
                let executable = crate::harness::profile_executable(ws, "dev")?;
                let _copy = crate::harness::CopyCleanup(executable.clone());
                let (_code, _) = check::run_logged(
                    &ws.root,
                    &mut crate::harness::command(&executable, &["health".into()]),
                    &report.log,
                )?;
                let health_file = ws.root.join("target/health.json");
                if health_file.exists() {
                    if let Ok(new_report) = read_json(&health_file) {
                        let previous = &state["health_report"];
                        if has_health_drift(&new_report, previous) {
                            tracker_changed |= update_health_drift_bead(ws, &new_report)?;
                        }
                        state["health_report"] = new_report;
                        state["health_at"] = json!(now());
                    }
                }
            }
        }
    }
    let bead = if report.passed {
        tracker_changed |= close_red(ws, &sha)?;
        state["last_green"] = json!(sha);
        if full {
            state["last_full"] = json!(sha);
            state["full_at"] = json!(now());
        }
        None
    } else {
        let (id, changed) = red_bead(ws, &report, &sha, good.as_deref(), &dir)?;
        tracker_changed |= changed;
        Some(id)
    };
    for failure in flaky {
        tracker_changed |= record_flaky(ws, &failure, &sha)?;
    }
    for command in flaky_commands {
        tracker_changed |= record_flaky_command(ws, command, &sha)?;
    }
    let record = json!({"seconds":started.elapsed().as_secs_f64(),"initial_check":initial_check,"retries":retries,"sha":sha,"platform":check::platform(),"at":now(),"full":full,"green":report.passed,"bead":bead,"check":report.json()});
    write_json(&dir.join(format!("{sha}.json")), &record)?;
    state["last_checked"] = json!(sha);
    state["green"] = json!(report.passed);
    write_json(&state_file, &state)?;
    status(&format!(
        "{} {sha}; bead={}",
        if report.passed { "green" } else { "red" },
        bead.as_deref().unwrap_or("none")
    ))?;
    if tracker_changed {
        push_tracker(ws)?;
    }
    Ok(report.passed)
}

pub fn run(raw: &[String]) -> ExitCode {
    // The bisection subprocess keeps using the current keeper binary while source checkouts move.
    if let [flag, file] = raw
        && flag == "--probe"
    {
        let result = (|| {
            let probe = read_json(Path::new(file))?;
            let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
            let root = PathBuf::from(cargo::git(&cwd, &["rev-parse", "--show-toplevel"])?.trim());
            let ws = Workspace {
                root,
                members: Vec::new(),
            };
            retry_probe(&ws, &probe, &ws.root.join("target/keep/probe.log"))
        })();
        return ExitCode::from(match result {
            Ok(Some(true)) => 0,
            Ok(Some(false)) => 1,
            _ => 125,
        });
    }
    if raw != ["--once"] {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    match std::env::current_dir()
        .map_err(|e| e.to_string())
        .and_then(|cwd| cargo::git(&cwd, &["rev-parse", "--show-toplevel"]))
        .and_then(|root| {
            once(&Workspace {
                root: PathBuf::from(root.trim()),
                members: Vec::new(),
            })
        }) {
        Ok(green) => ExitCode::from(if green { 0 } else { 1 }),
        Err(e) => {
            eprintln!("keeper: {e}");
            let _ = status(&format!("error: {e}"));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_checks_are_due_initially_after_twenty_commits_or_two_hours() {
        assert!(full_due(&json!({}), 0, 1));
        let state = json!({"last_full":"abc","full_at":100});
        assert!(!full_due(&state, 19, 7299));
        assert!(full_due(&state, 20, 101));
        assert!(full_due(&state, 0, 7300));
        assert!(!full_due(&state, 0, 1));
    }
    #[test]
    fn shell_arguments_preserve_spaces_and_single_quotes() {
        assert_eq!(quote("a b'c"), "'a b'\\''c'");
    }
    #[test]
    fn outgoing_source_commits_and_source_hidden_in_a_merge_are_refused() {
        let root = std::env::temp_dir().join(format!("xtask-keeper-push-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let git = |args: &[&str]| cargo::git(&root, args).unwrap();
        git(&["init", "--initial-branch=master"]);
        git(&["config", "user.name", "Fixture"]);
        git(&["config", "user.email", "fixture@example.invalid"]);
        fs::write(root.join("source.txt"), "original").unwrap();
        git(&["add", "source.txt"]);
        git(&["commit", "-m", "baseline"]);
        let baseline = git(&["rev-parse", "HEAD"]);
        git(&["update-ref", "refs/remotes/origin/master", baseline.trim()]);
        // Git plumbing constructs tracker trees without another writer touching .beads files.
        fs::write(root.join("tracker-content"), "fixture").unwrap();
        let blob = git(&["hash-object", "-w", "tracker-content"]);
        git(&[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("100644,{},.beads/fixture", blob.trim()),
        ]);
        git(&["commit", "-m", "tracker"]);
        let tracker = git(&["rev-parse", "HEAD"]);
        assert!(validate_outgoing(&root).is_ok());
        fs::write(root.join("source.txt"), "source changed").unwrap();
        git(&["add", "source.txt"]);
        let tree = git(&["write-tree"]);
        let merge = git(&[
            "commit-tree",
            tree.trim(),
            "-p",
            tracker.trim(),
            "-p",
            baseline.trim(),
            "-m",
            "merge with source",
        ]);
        git(&["update-ref", "HEAD", merge.trim()]);
        assert!(validate_outgoing(&root).is_err());
        let ordinary = git(&[
            "commit-tree",
            tree.trim(),
            "-p",
            tracker.trim(),
            "-m",
            "source",
        ]);
        git(&["update-ref", "HEAD", ordinary.trim()]);
        assert!(validate_outgoing(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn a_failed_bisection_names_the_range_and_no_culprit() {
        let body = no_culprit(Some("aaa1111"), "bbb2222", "bisection could not identify one commit");
        assert!(body.contains("no single culprit found"), "{body}");
        assert!(body.contains("aaa1111..bbb2222"), "{body}");
        assert!(!body.contains("first bad commit"), "{body}");
        let open = no_culprit(None, "bbb2222", "no passing exact-test endpoint");
        assert!(open.contains("no single culprit found") && open.contains("bbb2222"), "{open}");
        assert!(!open.contains("first bad commit"), "{open}");
    }
    #[test]
    fn bisection_without_a_passing_endpoint_gives_no_culprit() {
        let root = std::env::temp_dir().join(format!("xtask-keeper-bisect-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let git = |args: &[&str]| cargo::git(&root, args).unwrap();
        git(&["init", "--initial-branch=master"]);
        git(&["config", "user.name", "Fixture"]);
        git(&["config", "user.email", "fixture@example.invalid"]);
        fs::write(root.join("a.txt"), "a").unwrap();
        git(&["add", "a.txt"]);
        git(&["commit", "-m", "only commit"]);
        let sha = git(&["rev-parse", "HEAD"]).trim().to_owned();
        let ws = Workspace { root: root.clone(), members: Vec::new() };
        fs::create_dir_all(root.join("target/keep")).unwrap();
        let (text, culprit) = diagnose(&ws, &json!({"command":"workspace"}), None, &sha, &root.join("target/keep"));
        assert!(culprit.is_none(), "{text}");
        assert!(text.contains("no single culprit found") && text.contains(&sha), "{text}");
        assert!(!text.contains("first bad commit"), "{text}");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn build_probe_separates_source_failures_from_unavailable_evidence() {
        assert_eq!(build_outcome(Some(0), ""), Some(true));
        for text in [
            "error: could not compile p",
            "error: failed to parse manifest at Cargo.toml",
            "error: failed to load manifest for workspace member",
            "error: invalid table header",
        ] {
            assert_eq!(build_outcome(Some(101), text), Some(false));
        }
        assert_eq!(
            build_outcome(Some(101), "failed to download dependency"),
            None
        );
        assert_eq!(build_outcome(None, "killed"), None);
    }
}
