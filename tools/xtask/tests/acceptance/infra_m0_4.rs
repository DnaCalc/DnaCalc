//! The bodies of the infra-m0.4 acceptance cases (the `#[test]` functions sit at the root of
//! `tests/acceptance.rs`, so libtest lists them under their registered names).
//!
//! Both cases drive the xtask binary this test build compiled (`CARGO_BIN_EXE_xtask`), the binary
//! `cargo xtask` runs, from the workspace root: `pin-sources --out <temporary file>` to generate a
//! manifest and `pin-sources --lookup` to read one. Neither case writes the tracked
//! `migration/source-manifest.json`; `source_manifest` checks that its bytes are unchanged by the
//! case. What the cases compare is measured here, independently of that binary: each blob with
//! this module's own `git rev-parse <commit>:<path>`, each pinned commit with `git cat-file -t`,
//! each license file with `git ls-tree`, file contents with this module's own `git cat-file
//! --batch` (runner markers, research-x87 packages, x87 research-surface calls, module
//! declarations), each citation against the tracker's own text, the plan audit's blobs against
//! the pinned commits, and the counts against the inputs this module counts itself.
//!
//! The legacy repositories are only read. Every case reads `git status --porcelain` of each of
//! them before and after and requires the two to be identical. The status read adds the same
//! `-c` settings the generator uses (`STATUS_CONFIG`), so that a Windows host and a WSL host
//! reading the same checkouts through `/mnt/c` measure the same thing; `summary.txt` records the
//! exact argument vector. A legacy repository's working-tree state is reported, never a reason to
//! fail: every pinned blob comes from a commit object. Evidence goes to
//! `target/accept/infra-m0.4/<case>/`.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BEAD: &str = "infra-m0.4";
const MANIFEST: &str = "migration/source-manifest.json";
const PLAN_AUDIT: &str = "docs/plan/source-audit.json";
const LEGACY_ROOT_VARIABLE: &str = "DNACALC_LEGACY_ROOT";

/// The repositories the bead's Do names, and OxVba, which three beads cite. Listed here on purpose
/// rather than taken from the generator.
const REPOSITORIES: [&str; 12] = [
    "OxFunc",
    "OxFml",
    "OxCalc",
    "OxDoc",
    "DnaTreeCalc",
    "DnaOneCalc",
    "OxReplay",
    "OxXlPlay",
    "ExcelFunctionsHandbook",
    "DnaVisiCalc",
    "Foundation",
    "OxVba",
];

/// The `-c` settings added to `status`, as the generator adds them (see `src/pin_git.rs`).
const STATUS_CONFIG: [&str; 4] = [
    "core.excludesFile=",
    "core.autocrlf=true",
    "core.checkStat=minimal",
    "core.trustctime=false",
];

/// The fields every pinned input must carry.
const SIX_FIELDS: [&str; 6] = [
    "repository",
    "commit",
    "path",
    "blob",
    "license_status",
    "disposition",
];

const DISPOSITIONS: [&str; 4] = ["port", "reference", "quarantine_derived", "quarantined"];
const CATEGORIES: [&str; 5] = [
    "parser",
    "function-family",
    "corpus",
    "oracle-runner",
    "other",
];

/// The quarantined numeric directory (infra-m0.5's subject).
const QUARANTINED_DIR: (&str, &str) = ("OxFunc", "crates/oxfunc_core/src/excel_numeric/");

/// The source-discovery paths of the capture lineage (the Port from text of 19 infra beads).
const DISCOVERY_DIRS: [(&str, &str); 2] =
    [("OxFunc", "smart-fuzzer/"), ("OxFunc", "tools/xll-addin/")];

/// Text that marks a file as driving Excel through COM or automation, as this test reads it.
const RUNNER_MARKERS: [&str; 5] = [
    "Excel.Application",
    "Workbooks.Open",
    "Marshal",
    "Interop",
    "ComObject",
];

/// The files the 2026-10-14 review found missing because the module declarations of pinned
/// inputs were not resolved.
const DECLARED_BY_REVIEW: [&str; 13] = [
    "OxFunc/crates/oxfunc_core/src/coercion_decimal.rs",
    "OxFunc/crates/oxfunc_core/src/coercion_decimal_powers.rs",
    "OxFunc/crates/oxfunc_core/src/functions/complex_decimal.rs",
    "OxFunc/crates/oxfunc_core/src/functions/complex_decimal_powers.rs",
    "OxFunc/crates/oxfunc_core/src/functions/address_name_classes.rs",
    "OxCalc/src/oxcalc-core/src/grid/reference_engine/parse.rs",
    "OxCalc/src/oxcalc-core/src/grid/reference_engine/profile_codec.rs",
    "OxCalc/src/oxcalc-core/src/grid/reference_engine/transform.rs",
    "OxFml/crates/oxfml_core/src/consumer/editor/types.rs",
    "DnaTreeCalc/src/dnacalc-stage-sheet/src/canvas.rs",
    "DnaTreeCalc/src/dnacalc-stage-sheet/src/edit.rs",
    "DnaTreeCalc/src/dnacalc-stage-sheet/src/gestures.rs",
    "DnaTreeCalc/src/dnacalc-stage-sheet/src/viewport.rs",
];

fn repo_root() -> PathBuf {
    super::repo_root()
}

/// Checks that `case` is registered under infra-m0.4 in the xtask manifest for windows and linux,
/// and that the bead registers exactly its two Acceptance commands (so an accept run never runs
/// `pin-sources` and never rewrites the tracked manifest).
fn registered(case: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("acceptance.json");
    let manifest = super::read_json(&path);
    let entry = manifest["beads"]
        .as_array()
        .and_then(|beads| beads.iter().find(|bead| bead["id"] == BEAD))
        .unwrap_or_else(|| panic!("{} has no entry for {BEAD}", path.display()));
    let cases = super::strings(&entry["cases"], "registered cases");
    assert!(
        cases.iter().any(|registered| registered == case),
        "{case} is not registered under {BEAD} in {}; registered: {cases:?}",
        path.display()
    );
    let platforms = super::strings(&entry["platforms"][case], "the case's platforms");
    assert!(
        platforms.iter().any(|p| p == "windows") && platforms.iter().any(|p| p == "linux"),
        "{case} must declare windows and linux, not {platforms:?}"
    );
    let commands: Vec<Vec<String>> = entry["commands"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|command| super::strings(command, "a registered command"))
        .collect();
    let accept = |platform: &str| -> Vec<String> {
        [
            "cargo",
            "xtask",
            "accept",
            "--id",
            BEAD,
            "--platform",
            platform,
            "--require-all",
        ]
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect()
    };
    assert_eq!(
        commands,
        vec![accept("windows"), accept("linux")],
        "{BEAD} must register exactly its two Acceptance commands"
    );
}

struct Evidence {
    case: &'static str,
    dir: PathBuf,
    lines: Vec<String>,
    failures: Vec<String>,
}

impl Evidence {
    fn new(case: &'static str) -> Evidence {
        let dir = repo_root()
            .join("target")
            .join("accept")
            .join(BEAD)
            .join(case);
        super::remove_tree(&dir).unwrap_or_else(|e| panic!("{e}"));
        fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("cannot create {}: {e}", dir.display()));
        Evidence {
            case,
            dir,
            lines: Vec::new(),
            failures: Vec::new(),
        }
    }

    fn line(&mut self, text: String) {
        self.lines.push(text);
    }

    fn expect(&mut self, holds: bool, what: String) {
        if holds {
            self.lines.push(format!("ok: {what}"));
        } else {
            self.failures.push(what);
        }
    }

    fn finish(self) {
        super::finish(self.case, &self.dir, &self.lines, &self.failures);
    }
}

/// The legacy root, as the generator chooses it, with how it was chosen. The path is shown only
/// as `..` or by the variable's name, never as an absolute host path.
fn legacy_root() -> (PathBuf, String) {
    match std::env::var_os(LEGACY_ROOT_VARIABLE).filter(|v| !v.is_empty()) {
        Some(value) => (
            PathBuf::from(value),
            format!("the directory {LEGACY_ROOT_VARIABLE} names"),
        ),
        None => (
            repo_root()
                .parent()
                .expect("the workspace root has a parent")
                .to_path_buf(),
            "the workspace root's parent (..)".to_owned(),
        ),
    }
}

fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// A read-only git command in a legacy repository.
fn legacy_git(dir: &Path, config: &[&str], args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command
        .arg("--no-optional-locks")
        .arg("-c")
        .arg(format!("safe.directory={}", slashed(dir)));
    for setting in config {
        command.arg("-c").arg(setting);
    }
    command.arg("-C").arg(dir).args(args);
    command
}

fn output(mut command: Command) -> Output {
    let shown = format!("{command:?}");
    command
        .output()
        .unwrap_or_else(|e| panic!("cannot start {shown}: {e}"))
}

fn legacy_read(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = output(legacy_git(dir, &[], args));
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        Err(format!(
            "git {args:?} exited with {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// The contents of `blobs` by this module's own `git cat-file --batch` (absent objects are left
/// out).
fn read_blobs(dir: &Path, blobs: &BTreeSet<String>) -> BTreeMap<String, Vec<u8>> {
    let mut command = legacy_git(dir, &[], &["cat-file", "--batch"]);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("cannot start git cat-file --batch: {e}"));
    let mut stdin = child.stdin.take().expect("cat-file has a standard input");
    let request: String = blobs.iter().map(|b| format!("{b}\n")).collect();
    let writer = std::thread::spawn(move || {
        stdin
            .write_all(request.as_bytes())
            .expect("git cat-file takes the object list");
    });
    let out = child
        .wait_with_output()
        .expect("git cat-file --batch finishes");
    writer.join().expect("the object list was written");
    let bytes = out.stdout;
    let mut found = BTreeMap::new();
    let mut at = 0;
    while at < bytes.len() {
        let Some(end) = bytes[at..].iter().position(|b| *b == b'\n') else {
            break;
        };
        let header = String::from_utf8_lossy(&bytes[at..at + end]).into_owned();
        at += end + 1;
        let fields: Vec<&str> = header.split_whitespace().collect();
        if let [object, _kind, size] = fields.as_slice() {
            let size: usize = size.parse().expect("cat-file prints a size");
            found.insert((*object).to_owned(), bytes[at..at + size].to_vec());
            at += size + 1;
        }
    }
    found
}

/// The status argument vector shown in the evidence.
fn status_argv(dir: &Path) -> String {
    format!(
        "{:?}",
        legacy_git(dir, &STATUS_CONFIG, &["status", "--porcelain"])
    )
}

/// `git status --porcelain` of every legacy repository (with exit code), keyed by name.
fn statuses(root: &Path) -> BTreeMap<String, String> {
    REPOSITORIES
        .iter()
        .map(|name| {
            let dir = root.join(name);
            let out = output(legacy_git(&dir, &STATUS_CONFIG, &["status", "--porcelain"]));
            (
                (*name).to_owned(),
                format!(
                    "exit {:?}\n{}{}",
                    out.status.code(),
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                ),
            )
        })
        .collect()
}

fn write_statuses(path: &Path, statuses: &BTreeMap<String, String>) {
    let mut text = String::new();
    for (name, status) in statuses {
        text.push_str(&format!("== {name}\n{status}\n"));
    }
    super::write(path, text);
}

/// Runs the xtask binary from the workspace root, keeping its transcript as `<label>.*`.
fn xtask(evidence: &Evidence, label: &str, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
    command.args(args).current_dir(repo_root());
    super::run_recorded(label, command, &evidence.dir)
}

fn scratch(case: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    super::remove_tree(&dir).unwrap_or_else(|e| panic!("{e}"));
    fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("cannot create {}: {e}", dir.display()));
    dir
}

/// The repository-state fields of a manifest: what the repositories looked like at pin time,
/// which a fresh pin reports but which pin no content.
const STATE_FIELDS: [&str; 5] = [
    "working_tree",
    "stash_count",
    "on_origin",
    "origin_branches",
    "branch",
];
const STATE_SUMMARY: [&str; 3] = [
    "repositories_with_tracked_changes",
    "repositories_with_untracked_entries",
    "repositories_not_on_origin",
];

/// The manifest without its pin date and repository state: the pinned content (repositories'
/// commits and license blobs, inputs with blobs, dispositions and categories, citations,
/// exclusions, families, rules and notes).
fn pinned_content(manifest: &Value) -> Value {
    let mut copy = manifest.clone();
    if let Some(object) = copy.as_object_mut() {
        object.remove("pin_date");
    }
    for repository in copy["repositories"].as_array_mut().into_iter().flatten() {
        if let Some(object) = repository.as_object_mut() {
            for field in STATE_FIELDS {
                object.remove(field);
            }
        }
    }
    if let Some(summary) = copy["summary"].as_object_mut() {
        for field in STATE_SUMMARY {
            summary.remove(field);
        }
    }
    copy
}

/// The repository-state differences between two manifests, one line each.
fn state_differences(left: &Value, right: &Value) -> Vec<String> {
    let mut found = Vec::new();
    let index = |manifest: &Value| -> BTreeMap<String, Value> {
        manifest["repositories"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| (text(&r["name"]).to_owned(), r.clone()))
            .collect()
    };
    let (a, b) = (index(left), index(right));
    for (name, committed) in &a {
        for field in STATE_FIELDS {
            let fresh = b.get(name).map(|r| &r[field]);
            if Some(&committed[field]) != fresh {
                found.push(format!(
                    "{name}.{field}: committed {} / fresh {}",
                    committed[field],
                    fresh.map_or("absent".to_owned(), Value::to_string)
                ));
            }
        }
    }
    found
}

/// The top-level keys whose values differ, and for the inputs the first differing entries.
fn differences(left: &Value, right: &Value) -> Vec<String> {
    let mut found = Vec::new();
    let keys: BTreeSet<&String> = left
        .as_object()
        .into_iter()
        .chain(right.as_object())
        .flat_map(|object| object.keys())
        .collect();
    for key in keys {
        if left[key] != right[key] {
            found.push(format!("{key} differs"));
        }
    }
    if left["inputs"] != right["inputs"] {
        let index = |manifest: &Value| -> BTreeMap<String, Value> {
            manifest["inputs"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|entry| (key_of(entry), entry.clone()))
                .collect()
        };
        let (a, b) = (index(left), index(right));
        let names: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
        for name in names
            .into_iter()
            .filter(|n| a.get(*n) != b.get(*n))
            .take(10)
        {
            found.push(format!(
                "input {name}: committed {} / fresh {}",
                a.get(name).map_or("absent".to_owned(), Value::to_string),
                b.get(name).map_or("absent".to_owned(), Value::to_string)
            ));
        }
    }
    found
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}

fn key_of(entry: &Value) -> String {
    format!("{}/{}", text(&entry["repository"]), text(&entry["path"]))
}

/// The Port from section of every bead of the tracker, read here independently of the generator.
fn port_from_sections(tracker: &Path) -> BTreeMap<String, String> {
    let mut sections = BTreeMap::new();
    for line in super::read_text(tracker).lines() {
        if line.trim().is_empty() {
            continue;
        }
        let record: Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("{} holds a line that is not JSON: {e}", tracker.display()));
        let description = text(&record["description"]);
        if let Some(start) = description.find("## Port from\n") {
            let rest = &description[start + "## Port from\n".len()..];
            let end = rest.find("\n## ").unwrap_or(rest.len());
            sections.insert(
                text(&record["id"]).to_owned(),
                rest[..end].trim().to_owned(),
            );
        }
    }
    sections
}

/// The blob of each `<commit>:<path>` by this module's own `git rev-parse`, in chunks; a chunk that
/// fails (one absent path fails it) is retried one path at a time.
fn rev_parse_blobs(
    dir: &Path,
    commit: &str,
    paths: &[String],
) -> BTreeMap<String, Result<String, String>> {
    let mut found = BTreeMap::new();
    for chunk in paths.chunks(100) {
        let specs: Vec<String> = chunk.iter().map(|p| format!("{commit}:{p}")).collect();
        let mut args = vec!["rev-parse"];
        args.extend(specs.iter().map(String::as_str));
        match legacy_read(dir, &args) {
            Ok(out) if out.lines().count() == chunk.len() => {
                for (path, line) in chunk.iter().zip(out.lines()) {
                    found.insert(path.clone(), Ok(line.trim().to_owned()));
                }
            }
            _ => {
                for path in chunk {
                    let spec = format!("{commit}:{path}");
                    found.insert(path.clone(), legacy_read(dir, &["rev-parse", &spec]));
                }
            }
        }
    }
    found
}

/// The top-level `mod <name>;` declarations of a Rust text as this test reads them: unindented
/// lines, with the `#[path = "..."]` value of the attribute lines just above.
fn top_level_declarations(source: &str) -> Vec<(String, Option<String>)> {
    let mut found = Vec::new();
    let mut path = None;
    for line in source.lines() {
        if line.starts_with("#[") {
            if let Some(rest) = line.strip_prefix("#[path = \"")
                && let Some(end) = rest.find('"')
            {
                path = Some(rest[..end].to_owned());
            }
            continue;
        }
        let declaration = line
            .strip_prefix("pub(crate) ")
            .or_else(|| line.strip_prefix("pub(super) "))
            .or_else(|| line.strip_prefix("pub "))
            .unwrap_or(line);
        if let Some(rest) = declaration.strip_prefix("mod ")
            && let Some(name) = rest.trim_end().strip_suffix(';')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            found.push((name.to_owned(), path.take()));
        } else {
            path = None;
        }
    }
    found
}

/// `relative` joined to the directory `dir`, with `.` and `..` resolved.
fn join(dir: &str, relative: &str) -> String {
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// Whether a Cargo manifest text enables oxfunc_core's research-x87 feature, as this test reads
/// it: a line that forwards `oxfunc_core/research-x87`, or an `oxfunc_core` dependency line or
/// table that lists `"research-x87"`.
fn manifest_enables_research(manifest: &str) -> bool {
    let mut table = "";
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            table = line;
            continue;
        }
        if line.contains("oxfunc_core/research-x87") {
            return true;
        }
        let on_dependency =
            table.contains("dependencies.oxfunc_core") || line.starts_with("oxfunc_core ");
        if on_dependency && line.contains("\"research-x87\"") {
            return true;
        }
    }
    false
}

/// Whether a Rust text names the x87 research surface in code, as this test reads it: a line
/// outside `//` comments that holds `excel_numeric::research` or `excel_numeric::{` with
/// `research` in the braces, not inside a string.
fn names_research_surface(source: &str) -> bool {
    source.lines().any(|line| {
        let code = line.trim_start();
        if code.starts_with("//") {
            return false;
        }
        let code = code.split("//").next().unwrap_or_default();
        let unquoted: String = code.split('"').step_by(2).collect();
        unquoted.contains("excel_numeric::research")
            || unquoted
                .split("excel_numeric::{")
                .nth(1)
                .is_some_and(|group| group.split('}').next().unwrap_or("").contains("research"))
    })
}

/// infra_m0_4__source_manifest: every input carries the six fields and `verification:
/// provisional`, its blob is the one this test resolves at a pinned commit object, licenses match
/// the pinned trees, every citation is in the tracker, discovery paths are never port, runner
/// candidates, quarantine_derived inputs and declared modules match this test's own reading of
/// the blobs, the plan-audit drift matches this test's comparison, and a fresh pin written to a
/// temporary file reproduces the committed manifest's pinned content. The tracked manifest's bytes
/// are unchanged by the case.
pub fn source_manifest() {
    const CASE: &str = "infra_m0_4__source_manifest";
    registered(CASE);
    let mut evidence = Evidence::new(CASE);
    let (root, chosen) = legacy_root();
    if !root.is_dir() {
        let reason = format!("the legacy root ({chosen}) does not exist on this host");
        println!("ACCEPT-NOT-RUN: {CASE}: {reason}");
        evidence.line(format!("not run: {reason}"));
        evidence.finish();
        return;
    }
    evidence.line(format!("legacy root: {chosen}"));
    evidence.line(format!(
        "status read: {} (and likewise for every legacy repository)",
        status_argv(&root.join(REPOSITORIES[0]))
    ));
    let before = statuses(&root);
    write_statuses(&evidence.dir.join("legacy-status-before.txt"), &before);
    let working = repo_root().join(MANIFEST);
    let working_before = fs::read(&working).ok();

    // The committed manifest: the copy HEAD tracks, or the working-tree file before its first
    // commit.
    let in_head = Command::new("git")
        .arg("-C")
        .arg(repo_root())
        .args(["show", &format!("HEAD:{MANIFEST}")])
        .output()
        .ok()
        .filter(|out| out.status.success());
    let (committed_text, source) = match in_head {
        Some(out) => (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            format!("HEAD:{MANIFEST}"),
        ),
        None => (
            super::read_text(&working),
            format!("{MANIFEST} in the working tree (HEAD does not track it yet)"),
        ),
    };
    let committed: Value = serde_json::from_str(&committed_text)
        .unwrap_or_else(|e| panic!("{source} is not JSON: {e}"));
    evidence.line(format!("committed manifest read from {source}"));
    evidence.expect(
        !committed_text.to_ascii_lowercase().contains("verified"),
        format!("{source} contains the word verified nowhere"),
    );

    // A fresh pin, written outside the workspace.
    let fresh_path = scratch(CASE).join("source-manifest.json");
    let fresh_out = xtask(
        &evidence,
        "pin-sources-fresh",
        &["pin-sources", "--out", &fresh_path.to_string_lossy()],
    );
    evidence.expect(
        fresh_out.status.success(),
        format!(
            "a fresh cargo xtask pin-sources --out <temporary file> run exits 0 (exit {:?})",
            fresh_out.status.code()
        ),
    );
    if fresh_out.status.success() {
        let fresh = super::read_json(&fresh_path);
        let found = differences(&pinned_content(&committed), &pinned_content(&fresh));
        evidence.expect(
            found.is_empty(),
            format!(
                "a fresh pin reproduces the pinned content of {source} (commits, inputs with blobs, dispositions and categories, citations, exclusions, families, license blobs, rules and notes; committed pin date {}, fresh {}); differences: {found:?}",
                text(&committed["pin_date"]),
                text(&fresh["pin_date"])
            ),
        );
        let state = state_differences(&committed, &fresh);
        evidence.line(format!(
            "repository state at pin time versus now (reported, not failing: a blob comes from a commit object): {}",
            if state.is_empty() {
                "no difference".to_owned()
            } else {
                state.join("; ")
            }
        ));
        if working.is_file() && !source.starts_with(MANIFEST) {
            let on_disk = super::read_json(&working);
            let found = differences(&pinned_content(&committed), &pinned_content(&on_disk));
            evidence.expect(
                found.is_empty(),
                format!("{MANIFEST} in the working tree holds the pinned content of {source}; differences: {found:?}"),
            );
        }
    }

    // Repositories: the pinned commit is a commit object; licenses match the tree.
    let repositories: BTreeMap<String, Value> = committed["repositories"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| (text(&entry["name"]).to_owned(), entry.clone()))
        .collect();
    evidence.expect(
        repositories
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            == REPOSITORIES.iter().copied().collect::<BTreeSet<_>>(),
        format!(
            "the manifest pins exactly the {} repositories {REPOSITORIES:?}",
            REPOSITORIES.len()
        ),
    );
    let mut commits = BTreeMap::new();
    for name in REPOSITORIES {
        let Some(entry) = repositories.get(name) else {
            continue;
        };
        let dir = root.join(name);
        let commit = text(&entry["commit"]).to_owned();
        let head = legacy_read(&dir, &["rev-parse", "HEAD"]);
        let kind = legacy_read(&dir, &["cat-file", "-t", &commit]);
        evidence.expect(
            entry["present"] == true && kind.as_deref() == Ok("commit"),
            format!(
                "{name}: pinned {commit} is a commit object ({}); HEAD now {}",
                super::shown_git(&kind),
                super::shown_git(&head)
            ),
        );
        let root_files =
            legacy_read(&dir, &["ls-tree", "--name-only", &commit]).unwrap_or_default();
        let licenses: Vec<&str> = root_files
            .lines()
            .filter(|file| {
                let lower = file.to_ascii_lowercase();
                ["license", "licence", "copying", "unlicense"]
                    .iter()
                    .any(|prefix| lower.starts_with(prefix))
            })
            .collect();
        let recorded: Vec<(&str, &str)> = entry["license"]["files"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|file| (text(&file["path"]), text(&file["blob"])))
            .collect();
        let resolved: Vec<(&str, String)> = licenses
            .iter()
            .map(|file| {
                (
                    *file,
                    legacy_read(&dir, &["rev-parse", &format!("{commit}:{file}")])
                        .unwrap_or_default(),
                )
            })
            .collect();
        let status = if licenses.is_empty() {
            "none-found"
        } else {
            "found"
        };
        evidence.expect(
            recorded
                .iter()
                .map(|(p, b)| (*p, (*b).to_owned()))
                .collect::<Vec<_>>()
                == resolved
                && entry["license"]["status"] == status,
            format!("{name}: license files and blobs {resolved:?} at {commit} give status {status}; recorded {recorded:?}"),
        );
        commits.insert(name.to_owned(), (commit, status));
    }

    // Inputs: six fields, provisional, a blob this test resolves itself at the pinned commit.
    let inputs = committed["inputs"].as_array().cloned().unwrap_or_default();
    let by_key: BTreeMap<String, &Value> = inputs.iter().map(|e| (key_of(e), e)).collect();
    let mut by_repository: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let mut field_problems = Vec::new();
    for entry in &inputs {
        let missing: Vec<&str> = SIX_FIELDS
            .iter()
            .copied()
            .filter(|field| text(&entry[*field]).is_empty())
            .collect();
        let repository = text(&entry["repository"]);
        let (commit, license) = commits
            .get(repository)
            .map(|(c, l)| (c.as_str(), *l))
            .unwrap_or(("", ""));
        let shown = key_of(entry);
        if !missing.is_empty() {
            field_problems.push(format!("{shown} lacks {missing:?}"));
        }
        if text(&entry["commit"]) != commit {
            field_problems.push(format!(
                "{shown} names commit {} not {commit}",
                text(&entry["commit"])
            ));
        }
        if text(&entry["license_status"]) != license {
            field_problems.push(format!(
                "{shown} license {} not {license}",
                text(&entry["license_status"])
            ));
        }
        if !DISPOSITIONS.contains(&text(&entry["disposition"])) {
            field_problems.push(format!("{shown} disposition {}", entry["disposition"]));
        }
        if entry["verification"] != "provisional" {
            field_problems.push(format!("{shown} verification {}", entry["verification"]));
        }
        if !CATEGORIES.contains(&text(&entry["category"])) {
            field_problems.push(format!("{shown} category {}", entry["category"]));
        }
        if entry["beads"].as_array().is_none_or(Vec::is_empty) {
            field_problems.push(format!("{shown} names no citing bead"));
        }
        by_repository
            .entry(repository.to_owned())
            .or_default()
            .push((
                text(&entry["path"]).to_owned(),
                text(&entry["blob"]).to_owned(),
            ));
    }
    evidence.expect(
        !inputs.is_empty() && field_problems.is_empty(),
        format!(
            "all {} inputs carry {SIX_FIELDS:?}, their repository's commit and license status, a known disposition and category, verification provisional and a citing bead; problems: {:?}",
            inputs.len(),
            field_problems.iter().take(10).collect::<Vec<_>>()
        ),
    );
    let mut matched = 0;
    let mut mismatched = Vec::new();
    for (repository, entries) in &by_repository {
        let Some((commit, _)) = commits.get(repository) else {
            continue;
        };
        let paths: Vec<String> = entries.iter().map(|(p, _)| p.clone()).collect();
        let resolved = rev_parse_blobs(&root.join(repository), commit, &paths);
        for (path, blob) in entries {
            match resolved.get(path) {
                Some(Ok(found)) if found == blob => matched += 1,
                other => mismatched.push(format!(
                    "{repository}/{path}: recorded {blob}, git gives {other:?}"
                )),
            }
        }
    }
    evidence.expect(
        matched == inputs.len() && mismatched.is_empty(),
        format!(
            "{matched} of {} input blobs equal this test's git rev-parse <commit>:<path> at the pinned commit; mismatches: {:?}",
            inputs.len(),
            mismatched.iter().take(10).collect::<Vec<_>>()
        ),
    );
    for entry in committed["excluded"].as_array().into_iter().flatten() {
        let repository = text(&entry["repository"]);
        let spec = format!("{}:{}", text(&entry["commit"]), text(&entry["path"]));
        let found = legacy_read(&root.join(repository), &["rev-parse", &spec]);
        evidence.expect(
            found.as_deref() == Ok(text(&entry["blob"])),
            format!(
                "excluded {repository}/{} has blob {} at its pinned commit",
                text(&entry["path"]),
                text(&entry["blob"])
            ),
        );
    }

    // Citations: each cited input's beads name it in their Port from text, each declared module's
    // beads reach its parent, and every bead whose Port from text names a legacy repository path
    // is accounted for.
    let sections = port_from_sections(&repo_root().join(".beads/issues.jsonl"));
    let mut uncited = Vec::new();
    let mut accounted: BTreeSet<String> = BTreeSet::new();
    for section in ["inputs", "unavailable", "excluded"] {
        for entry in committed[section].as_array().into_iter().flatten() {
            for bead in super::strings(&entry["beads"], "beads") {
                accounted.insert(bead);
            }
        }
    }
    let mut routes_checked = 0;
    for entry in &inputs {
        // Each group of beads that reached the input the same way: the routes when the beads
        // differ, otherwise the input's own beads and via.
        let groups: Vec<Value> = match entry["routes"].as_array() {
            Some(routes) => routes.clone(),
            None => vec![entry.clone()],
        };
        for group in &groups {
            routes_checked += 1;
            let beads = super::strings(&group["beads"], "beads");
            for via in super::strings(&group["via"], "via") {
                if let Some(parent) = via.strip_prefix("declared module of ") {
                    let parent_key = format!("{}/{parent}", text(&entry["repository"]));
                    let parent_beads: BTreeSet<String> = by_key
                        .get(&parent_key)
                        .map(|p| super::strings(&p["beads"], "beads").into_iter().collect())
                        .unwrap_or_default();
                    for bead in &beads {
                        if !parent_beads.contains(bead) {
                            uncited.push(format!(
                                "{bead} reaches {} as a declared module of {parent_key}, which is not its input",
                                key_of(entry)
                            ));
                        }
                    }
                    continue;
                }
                // What the bead's own Port from text must name for this route.
                let named = via
                    .strip_prefix("cited ")
                    .or_else(|| via.strip_prefix("discovery "))
                    .or_else(|| via.strip_prefix("test of "));
                let needed = if let Some(cited) = named {
                    cited
                        .trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .unwrap_or(cited)
                        .to_owned()
                } else if via.starts_with("catalogue ") {
                    "surface_dispatch.rs".to_owned()
                } else {
                    continue;
                };
                for bead in &beads {
                    if !sections.get(bead).is_some_and(|s| s.contains(&needed)) {
                        uncited.push(format!(
                            "{bead} reaches {} by {via:?} but its Port from text does not name {needed}",
                            key_of(entry)
                        ));
                    }
                }
            }
        }
    }
    evidence.expect(
        uncited.is_empty(),
        format!(
            "in all {routes_checked} routes, every bead names in its own Port from text the file or directory it cites, the catalogue file for catalogue routes and the directory for test routes, and every declared module's beads reach its parent; problems: {:?}",
            uncited.iter().take(10).collect::<Vec<_>>()
        ),
    );
    let citing: BTreeSet<&String> = sections
        .iter()
        .filter(|(_, s)| {
            REPOSITORIES.iter().any(|r| {
                s.match_indices(&format!("{r}/")).any(|(at, _)| {
                    s[at + r.len() + 1..].starts_with(|c: char| c.is_ascii_alphanumeric())
                })
            })
        })
        .map(|(bead, _)| bead)
        .collect();
    let unaccounted: Vec<&&String> = citing.iter().filter(|b| !accounted.contains(**b)).collect();
    evidence.expect(
        unaccounted.is_empty(),
        format!(
            "all {} beads whose Port from text names a legacy repository path appear among the inputs, unavailable or excluded entries; missing: {unaccounted:?}",
            citing.len()
        ),
    );

    // Quarantine.
    let (q_repository, q_dir) = QUARANTINED_DIR;
    let under_quarantine: Vec<&Value> = inputs
        .iter()
        .filter(|e| text(&e["repository"]) == q_repository && text(&e["path"]).starts_with(q_dir))
        .collect();
    let x87 = format!("{q_dir}x87.rs");
    evidence.expect(
        under_quarantine.iter().any(|e| text(&e["path"]) == x87)
            && under_quarantine
                .iter()
                .all(|e| text(&e["disposition"]) == "quarantined"),
        format!(
            "{} inputs under {q_repository}/{q_dir}, x87.rs among them, are all quarantined",
            under_quarantine.len()
        ),
    );

    // The contents this test reads itself: every Rust input, every discovery input and every
    // Cargo.toml of the OxFunc tree.
    let oxfunc_commit = commits
        .get("OxFunc")
        .map(|(c, _)| c.clone())
        .unwrap_or_default();
    let oxfunc_tree: BTreeMap<String, String> = legacy_read(
        &root.join("OxFunc"),
        &["ls-tree", "-r", "--full-tree", &oxfunc_commit],
    )
    .unwrap_or_default()
    .lines()
    .filter_map(|line| {
        let (meta, path) = line.split_once('\t')?;
        let object = meta.split(' ').nth(2)?;
        Some((path.to_owned(), object.to_owned()))
    })
    .collect();
    let in_discovery = |e: &Value| {
        DISCOVERY_DIRS
            .iter()
            .any(|(r, d)| text(&e["repository"]) == *r && text(&e["path"]).starts_with(d))
    };
    let mut contents: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for name in REPOSITORIES {
        let wanted: BTreeSet<String> = inputs
            .iter()
            .filter(|e| text(&e["repository"]) == name)
            .filter(|e| text(&e["path"]).ends_with(".rs") || in_discovery(e))
            .map(|e| text(&e["blob"]).to_owned())
            .chain(
                (name == "OxFunc")
                    .then(|| {
                        oxfunc_tree
                            .iter()
                            .filter(|(p, _)| p.ends_with("Cargo.toml"))
                            .map(|(_, b)| b.clone())
                            .collect::<Vec<_>>()
                    })
                    .into_iter()
                    .flatten(),
            )
            .collect();
        if !wanted.is_empty() {
            contents.extend(read_blobs(&root.join(name), &wanted));
        }
    }
    let content_of = |e: &Value| -> String {
        contents
            .get(text(&e["blob"]))
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
            .unwrap_or_default()
    };

    // Discovery paths: never port; runner candidates are exactly the non-markdown discovery files
    // that hold a runner marker.
    let discovery: Vec<&Value> = inputs.iter().filter(|e| in_discovery(e)).collect();
    let ported: Vec<String> = discovery
        .iter()
        .filter(|e| text(&e["disposition"]) == "port")
        .map(|e| key_of(e))
        .collect();
    evidence.expect(
        !discovery.is_empty() && ported.is_empty(),
        format!(
            "none of the {} inputs under the source-discovery paths {DISCOVERY_DIRS:?} has disposition port; ported: {:?}",
            discovery.len(),
            ported.iter().take(10).collect::<Vec<_>>()
        ),
    );
    let mut runner_problems = Vec::new();
    let mut runners = 0;
    for entry in &discovery {
        let path = text(&entry["path"]);
        let expected: Vec<String> = if path.to_ascii_lowercase().ends_with(".md") {
            Vec::new()
        } else {
            let body = content_of(entry);
            RUNNER_MARKERS
                .iter()
                .filter(|m| body.contains(**m))
                .map(|m| (*m).to_owned())
                .collect()
        };
        let recorded: Vec<String> = entry["runner_candidate"]["markers"]
            .as_array()
            .map(|_| super::strings(&entry["runner_candidate"]["markers"], "markers"))
            .unwrap_or_default();
        if !recorded.is_empty() {
            runners += 1;
            if !text(&entry["runner_candidate"]["selection"]).contains("infra-m1.2") {
                runner_problems.push(format!("{} does not say infra-m1.2 selects", key_of(entry)));
            }
        }
        if recorded != expected {
            runner_problems.push(format!(
                "{}: recorded {recorded:?}, this test reads {expected:?}",
                key_of(entry)
            ));
        }
        let category = text(&entry["category"]);
        if category != "oracle-runner" && category != "corpus" {
            runner_problems.push(format!("{} category {category}", key_of(entry)));
        }
    }
    evidence.expect(
        runners > 0
            && runner_problems.is_empty()
            && committed["summary"]["runner_candidates"] == runners,
        format!(
            "{runners} runner candidates, each naming infra-m1.2 as the selector, are exactly the non-markdown discovery files whose blobs hold one of {RUNNER_MARKERS:?}, and every discovery file is oracle-runner or corpus; summary {}; problems: {:?}",
            committed["summary"]["runner_candidates"],
            runner_problems.iter().take(10).collect::<Vec<_>>()
        ),
    );

    // quarantine_derived: exactly the files of research-x87 packages and the Rust files that
    // name the research surface, outside the quarantined directory.
    let research_packages: Vec<String> = oxfunc_tree
        .iter()
        .filter(|(path, blob)| {
            path.ends_with("Cargo.toml")
                && contents
                    .get(*blob)
                    .is_some_and(|b| manifest_enables_research(&String::from_utf8_lossy(b)))
        })
        .map(|(path, _)| path.trim_end_matches("Cargo.toml").to_owned())
        .collect();
    let packages: Vec<String> = oxfunc_tree
        .keys()
        .filter(|p| p.ends_with("Cargo.toml"))
        .map(|p| p.trim_end_matches("Cargo.toml").to_owned())
        .collect();
    let mut derived_problems = Vec::new();
    let mut derived = 0;
    for entry in &inputs {
        let path = text(&entry["path"]);
        let is_oxfunc = text(&entry["repository"]) == "OxFunc";
        let package = packages
            .iter()
            .filter(|dir| is_oxfunc && path.starts_with(dir.as_str()))
            .max_by_key(|dir| dir.len());
        let in_research_package = package.is_some_and(|dir| research_packages.contains(dir));
        let calls = path.ends_with(".rs") && names_research_surface(&content_of(entry));
        let quarantined_dir = is_oxfunc && path.starts_with(q_dir);
        let expected = !quarantined_dir && (in_research_package || calls);
        let recorded = text(&entry["disposition"]) == "quarantine_derived";
        if recorded {
            derived += 1;
        }
        if expected != recorded {
            derived_problems.push(format!(
                "{}: recorded {}, this test expects quarantine_derived {expected} (research package {in_research_package}, calls the surface {calls})",
                key_of(entry),
                text(&entry["disposition"])
            ));
        }
    }
    evidence.expect(
        derived > 0
            && derived_problems.is_empty()
            && committed["summary"]["by_disposition"]["quarantine_derived"] == derived,
        format!(
            "{derived} quarantine_derived inputs are exactly the files of the research-x87 packages {research_packages:?} and the Rust files that name excel_numeric::research in code, as this test reads the blobs; summary {}; problems: {:?}",
            committed["summary"]["by_disposition"]["quarantine_derived"],
            derived_problems.iter().take(10).collect::<Vec<_>>()
        ),
    );

    // Declared modules: every top-level `mod x;` of every Rust input names a pinned input (under
    // any of the readings rustc may apply), an excluded entry, or a counted skip.
    let excluded_keys: BTreeSet<String> = committed["excluded"]
        .as_array()
        .into_iter()
        .flatten()
        .map(key_of)
        .collect();
    let notes = committed["notes"].to_string();
    let mut declaration_problems = Vec::new();
    let mut declarations = 0;
    for entry in inputs.iter().filter(|e| text(&e["path"]).ends_with(".rs")) {
        let repository = text(&entry["repository"]);
        let path = text(&entry["path"]);
        let (dir, file) = path.rsplit_once('/').unwrap_or(("", path));
        let stem = file.trim_end_matches(".rs");
        for (name, path_attribute) in top_level_declarations(&content_of(entry)) {
            declarations += 1;
            let candidates: Vec<String> = match &path_attribute {
                Some(relative) => vec![join(dir, relative)],
                None => vec![
                    join(dir, &format!("{name}.rs")),
                    join(dir, &format!("{name}/mod.rs")),
                    join(dir, &format!("{stem}/{name}.rs")),
                    join(dir, &format!("{stem}/{name}/mod.rs")),
                ],
            };
            let keys: Vec<String> = candidates
                .iter()
                .map(|c| format!("{repository}/{c}"))
                .collect();
            let pinned = keys
                .iter()
                .any(|k| by_key.contains_key(k) || excluded_keys.contains(k));
            let counted = notes.contains(&format!("{repository}/{path}: mod {name}"));
            if !pinned && !counted {
                declaration_problems.push(format!(
                    "{repository}/{path} declares mod {name} ({path_attribute:?}); none of {candidates:?} is an input, excluded or counted"
                ));
            }
        }
    }
    evidence.expect(
        declarations > 0 && declaration_problems.is_empty(),
        format!(
            "all {declarations} top-level module declarations of the Rust inputs name a pinned input, an excluded entry or a counted skip; problems: {:?}",
            declaration_problems.iter().take(10).collect::<Vec<_>>()
        ),
    );
    let missing_named: Vec<&str> = DECLARED_BY_REVIEW
        .iter()
        .copied()
        .filter(|key| !by_key.contains_key(*key))
        .collect();
    evidence.expect(
        missing_named.is_empty(),
        format!(
            "the {} declared modules the review named are inputs (their blobs are among those checked above); missing: {missing_named:?}",
            DECLARED_BY_REVIEW.len()
        ),
    );

    // Plan-audit drift: the changed list is exactly this test's comparison of the plan audit's
    // blobs with the pinned commits.
    let audit = super::read_json(&repo_root().join(PLAN_AUDIT));
    let mut expected_changes = BTreeSet::new();
    for source in audit["sources"].as_array().into_iter().flatten() {
        let full = text(&source["path"]);
        let Some((repository, path)) = full.split_once('/') else {
            continue;
        };
        let Some((commit, _)) = commits.get(repository) else {
            continue;
        };
        let pinned = legacy_read(
            &root.join(repository),
            &["rev-parse", &format!("{commit}:{path}")],
        )
        .ok();
        if pinned.as_deref() != Some(text(&source["blob_sha"])) {
            expected_changes.insert((
                full.to_owned(),
                text(&source["blob_sha"]).to_owned(),
                pinned.unwrap_or_default(),
            ));
        }
    }
    let recorded_changes: BTreeSet<(String, String, String)> =
        committed["changed_since_plan_audit"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| {
                (
                    text(&e["path"]).to_owned(),
                    text(&e["audit_blob"]).to_owned(),
                    text(&e["pinned_blob"]).to_owned(),
                )
            })
            .collect();
    evidence.expect(
        !recorded_changes.is_empty() && recorded_changes == expected_changes,
        format!(
            "changed_since_plan_audit lists exactly the {} plan-audit blobs that differ at the pinned commits (path, audit blob, pinned blob): {expected_changes:?}; recorded {recorded_changes:?}",
            expected_changes.len()
        ),
    );

    let after = statuses(&root);
    write_statuses(&evidence.dir.join("legacy-status-after.txt"), &after);
    evidence.expect(
        before == after,
        format!(
            "git status --porcelain of all {} legacy repositories is identical before and after",
            REPOSITORIES.len()
        ),
    );
    let working_after = fs::read(&working).ok();
    evidence.expect(
        working_before == working_after,
        format!("{MANIFEST} in the working tree is byte-identical before and after the case"),
    );
    evidence.finish();
}

/// infra_m0_4__missing_source: a port input absent from the pinned manifest is reported
/// unavailable by the lookup route and by the generator, and never counted as a port input; no
/// input is called verified; quarantined inputs and never-pinned bulk evidence exit non-zero.
pub fn missing_source() {
    const CASE: &str = "infra_m0_4__missing_source";
    registered(CASE);
    let mut evidence = Evidence::new(CASE);

    // The committed manifest, as port beads read it.
    let manifest_text = super::read_text(&repo_root().join(MANIFEST));
    let manifest: Value = serde_json::from_str(&manifest_text)
        .unwrap_or_else(|e| panic!("{MANIFEST} is not JSON: {e}"));
    let inputs = manifest["inputs"].as_array().cloned().unwrap_or_default();
    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    let mut none_found = 0;
    for entry in &inputs {
        *counts.entry(text(&entry["disposition"])).or_default() += 1;
        if text(&entry["license_status"]) == "none-found" {
            none_found += 1;
        }
    }
    let count = |d: &str| counts.get(d).copied().unwrap_or(0);
    let summary = &manifest["summary"];
    evidence.expect(
        summary["port_provisional"] == count("port")
            && DISPOSITIONS
                .iter()
                .all(|d| summary["by_disposition"][*d] == count(d))
            && summary["inputs"] == inputs.len() as u64
            && DISPOSITIONS.iter().map(|d| count(d)).sum::<u64>() == inputs.len() as u64,
        format!(
            "{MANIFEST}: port_provisional {} equals the {} port inputs counted here; {} reference, {} quarantined and {} quarantine_derived inputs are not among them",
            summary["port_provisional"],
            count("port"),
            count("reference"),
            count("quarantined"),
            count("quarantine_derived")
        ),
    );
    let provisional = inputs
        .iter()
        .filter(|e| e["verification"] == "provisional")
        .count();
    evidence.expect(
        provisional == inputs.len()
            && summary["verification"]["provisional"] == inputs.len() as u64
            && !manifest_text.to_ascii_lowercase().contains("verified"),
        format!(
            "all {provisional} of {} inputs are verification provisional, and {MANIFEST} contains the word verified nowhere",
            inputs.len()
        ),
    );
    evidence.expect(
        summary["license_none_found"] == none_found,
        format!(
            "summary.license_none_found {} equals the {none_found} inputs counted here with license status none-found",
            summary["license_none_found"]
        ),
    );
    let unavailable_keys: BTreeSet<String> = manifest["unavailable"]
        .as_array()
        .into_iter()
        .flatten()
        .map(key_of)
        .collect();
    let input_keys: BTreeSet<String> = inputs.iter().map(key_of).collect();
    evidence.expect(
        summary["unavailable"] == unavailable_keys.len() as u64
            && unavailable_keys.is_disjoint(&input_keys),
        format!(
            "{} unavailable entries, none of them an input",
            unavailable_keys.len()
        ),
    );

    // The lookup route on the committed manifest.
    let absent = "OxFml/no/such/file.rs";
    let out = xtask(
        &evidence,
        "lookup-absent",
        &["pin-sources", "--lookup", absent],
    );
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    evidence.expect(
        out.status.code() == Some(1)
            && stdout.starts_with(&format!("unavailable {absent}"))
            && !input_keys.contains(absent),
        format!(
            "--lookup {absent} prints unavailable and exits 1 (exit {:?}): {}",
            out.status.code(),
            stdout.trim()
        ),
    );
    let mut lookup_outputs = vec![stdout];
    for (label, wanted, code, line) in [
        (
            "lookup-pinned",
            "OxFml/crates/oxfml_core/src/eval/mod.rs",
            0,
            None,
        ),
        (
            "lookup-declared",
            "OxFunc/crates/oxfunc_core/src/coercion_decimal.rs",
            0,
            None,
        ),
        (
            "lookup-quarantined",
            "OxFunc/crates/oxfunc_core/src/excel_numeric/x87.rs",
            3,
            Some("quarantined: do not copy (infra-m0.5)"),
        ),
    ] {
        let out = xtask(&evidence, label, &["pin-sources", "--lookup", wanted]);
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let entry = inputs.iter().find(|e| key_of(e) == wanted);
        let expected = entry.map(|e| {
            (
                text(&e["commit"]).to_owned(),
                text(&e["blob"]).to_owned(),
                text(&e["disposition"]).to_owned(),
            )
        });
        let shows = expected.as_ref().is_some_and(|(commit, blob, d)| {
            stdout.starts_with(&format!("pinned {wanted}\n"))
                && stdout.contains(&format!("  commit {commit}\n"))
                && stdout.contains(&format!("  blob {blob}\n"))
                && stdout.contains(&format!("  disposition {d}\n"))
                && stdout.contains("  verification provisional\n")
                && line.is_none_or(|l| stdout.contains(&format!("{l}\n")))
        });
        evidence.expect(
            out.status.code() == Some(code) && shows,
            format!(
                "--lookup {wanted} exits {code} and prints the manifest's commit, blob and disposition {expected:?}{} (exit {:?})",
                line.map(|l| format!(" and {l:?}")).unwrap_or_default(),
                out.status.code()
            ),
        );
        lookup_outputs.push(stdout);
    }
    let bulk = "OxFunc/smart-fuzzer/cache";
    let out = xtask(
        &evidence,
        "lookup-bulk-evidence",
        &["pin-sources", "--lookup", bulk],
    );
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    evidence.expect(
        out.status.code() == Some(4)
            && stdout.starts_with("not pinned: bulk evidence (infra-m0.6, DnaCalc/OxFunc-Evidence)")
            && !input_keys.iter().any(|k| k.starts_with(bulk)),
        format!(
            "--lookup {bulk} prints that it is never-pinned bulk evidence and exits 4 (exit {:?}): {}",
            out.status.code(),
            stdout.trim()
        ),
    );
    lookup_outputs.push(stdout);
    evidence.expect(
        lookup_outputs
            .iter()
            .all(|o| !o.to_ascii_lowercase().contains("verified")),
        "no lookup output contains the word verified".to_owned(),
    );

    // The generator route: a bead citing a path absent at the pinned commit.
    let (root, chosen) = legacy_root();
    if !root.is_dir() {
        let reason = format!("the legacy root ({chosen}) does not exist on this host");
        println!("ACCEPT-NOT-RUN: {CASE}: {reason}");
        evidence.line(format!("generator route not run: {reason}"));
        evidence.finish();
        return;
    }
    evidence.line(format!("legacy root: {chosen}"));
    let before = statuses(&root);
    write_statuses(&evidence.dir.join("legacy-status-before.txt"), &before);
    let oxfml = root.join("OxFml");
    let commit = legacy_read(&oxfml, &["rev-parse", "HEAD"]).unwrap_or_default();
    let absent_here = legacy_read(
        &oxfml,
        &["cat-file", "-t", &format!("{commit}:no/such/file.rs")],
    );
    let present_here = legacy_read(
        &oxfml,
        &[
            "rev-parse",
            &format!("{commit}:crates/oxfml_core/src/eval/mod.rs"),
        ],
    );
    evidence.expect(
        absent_here.is_err() && present_here.is_ok(),
        format!(
            "at OxFml {commit}, no/such/file.rs is absent and crates/oxfml_core/src/eval/mod.rs is blob {}",
            super::shown_git(&present_here)
        ),
    );
    let tracker = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pin/issues.jsonl");
    let out_path = scratch(CASE).join("fixture-manifest.json");
    let out = xtask(
        &evidence,
        "pin-sources-fixture",
        &[
            "pin-sources",
            "--tracker",
            &tracker.to_string_lossy(),
            "--out",
            &out_path.to_string_lossy(),
        ],
    );
    evidence.expect(
        out.status.success(),
        format!(
            "pin-sources over the fixture tracker exits 0 (exit {:?})",
            out.status.code()
        ),
    );
    if out.status.success() {
        let fixture_text = super::read_text(&out_path);
        let fixture: Value = serde_json::from_str(&fixture_text)
            .unwrap_or_else(|e| panic!("the fixture manifest is not JSON: {e}"));
        let unavailable: Vec<&Value> = fixture["unavailable"]
            .as_array()
            .into_iter()
            .flatten()
            .collect();
        let listed = unavailable.iter().any(|e| {
            text(&e["repository"]) == "OxFml"
                && text(&e["path"]) == "no/such/file.rs"
                && super::strings(&e["beads"], "beads") == ["dc-fixture.1"]
                && text(&e["reason"]).contains(&commit)
        });
        evidence.expect(
            listed && unavailable.len() == 1,
            format!("the fixture manifest lists OxFml/no/such/file.rs as unavailable for dc-fixture.1, absent at {commit}: {unavailable:?}"),
        );
        let fixture_inputs = fixture["inputs"].as_array().cloned().unwrap_or_default();
        let shown: Vec<(String, String, Vec<String>)> = fixture_inputs
            .iter()
            .map(|e| {
                (
                    key_of(e),
                    text(&e["disposition"]).to_owned(),
                    super::strings(&e["beads"], "beads"),
                )
            })
            .collect();
        let expected = vec![
            (
                "OxFml/crates/oxfml_core/src/eval/mod.rs".to_owned(),
                "port".to_owned(),
                vec!["dc-fixture.1".to_owned()],
            ),
            (
                "OxFunc/crates/oxfunc_core/src/excel_numeric/x87.rs".to_owned(),
                "quarantined".to_owned(),
                vec!["dc-fixture.2".to_owned()],
            ),
        ];
        evidence.expect(
            shown == expected,
            format!("the fixture manifest's inputs are exactly the present OxFml file (port) and x87.rs (quarantined): {shown:?}"),
        );
        let ports = fixture_inputs
            .iter()
            .filter(|e| text(&e["disposition"]) == "port")
            .count() as u64;
        evidence.expect(
            fixture["summary"]["port_provisional"] == ports
                && ports == 1
                && fixture["summary"]["unavailable"] == 1
                && fixture["summary"]["by_disposition"]["quarantined"] == 1
                && !fixture_text.to_ascii_lowercase().contains("verified"),
            format!(
                "the fixture manifest counts 1 provisional port input (counted here: {ports}), 1 unavailable and 1 quarantined, and calls nothing verified: {}",
                fixture["summary"]
            ),
        );
        let out = xtask(
            &evidence,
            "lookup-fixture-absent",
            &[
                "pin-sources",
                "--lookup",
                absent,
                "--manifest",
                &out_path.to_string_lossy(),
            ],
        );
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        evidence.expect(
            out.status.code() == Some(1)
                && stdout.starts_with(&format!("unavailable {absent}: cited by dc-fixture.1"))
                && stdout.contains(&commit),
            format!(
                "--lookup {absent} on the fixture manifest names dc-fixture.1 and the pinned commit, and exits 1 (exit {:?}): {}",
                out.status.code(),
                stdout.trim()
            ),
        );
    }
    let after = statuses(&root);
    write_statuses(&evidence.dir.join("legacy-status-after.txt"), &after);
    evidence.expect(
        before == after,
        format!(
            "git status --porcelain of all {} legacy repositories is identical before and after",
            REPOSITORIES.len()
        ),
    );
    evidence.finish();
}
