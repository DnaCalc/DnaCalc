//! Acceptance cases owned by the xtask package. Every case is registered by its exact name in
//! `tools/xtask/acceptance.json` and checks that registration before anything else.
//!
//! Cargo started from a case never shares a target directory with the outer `cargo test`, which
//! owns `target/`: the build case runs its inner cargo with its own `CARGO_TARGET_DIR`, and each
//! lane build runs in its own clone, whose target directory the lanes case reads back from cargo
//! before building. Each case keeps its evidence under `target/accept/<bead>/<case>/`, and its
//! `summary.txt` names the cargo program it ran. Every case holds one shared lock for its whole
//! body, so an unfiltered `cargo test -p xtask` runs them one at a time and no case's cargo runs
//! while another case is measuring.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::hash::{DefaultHasher, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};

/// Held for the whole body of every case. A case that panicked poisons it; the next case still
/// takes it, because the lock only orders the cases and guards no data.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

const BEAD: &str = "infra-m0.1";
const WASM_TARGET: &str = "wasm32-unknown-unknown";

/// The workspace has exactly these members, each at exactly this directory; a missing, moved or
/// extra member fails the build case.
const EXPECTED_PACKAGES: [(&str, &str); 12] = [
    ("calc-value", "crates/calc-value"),
    ("calc-func", "crates/calc-func"),
    ("calc-fml", "crates/calc-fml"),
    ("calc-doc-model", "crates/calc-doc-model"),
    ("calc-doc", "crates/calc-doc"),
    ("calc-engine", "crates/calc-engine"),
    ("calc-host", "crates/calc-host"),
    ("dna-onecalc", "apps/dna-onecalc"),
    ("dna-calc", "apps/dna-calc"),
    ("calc-platform-desktop", "platform/calc-platform-desktop"),
    ("calc-oracle-excel", "platform/calc-oracle-excel"),
    ("xtask", "tools/xtask"),
];

/// The packages that never build for wasm32.
const EXPECTED_NATIVE_ONLY: [&str; 3] = ["calc-platform-desktop", "calc-oracle-excel", "xtask"];

/// The four library lanes that must build at the same time in separate clones.
const LANES: [(&str, &str); 4] = [
    ("Func", "calc-func"),
    ("Fml", "calc-fml"),
    ("Doc", "calc-doc"),
    ("Engine", "calc-engine"),
];

/// How long the four lane builds together may run before the remaining ones are killed and fail.
const LANE_DEADLINE: Duration = Duration::from_secs(600);

/// The variables that would point a lane build at a target directory other than its clone's own;
/// every lane cargo runs with both removed from its environment.
const TARGET_DIR_VARIABLES: [&str; 2] = ["CARGO_TARGET_DIR", "CARGO_BUILD_TARGET_DIR"];

const PLAN: &str = "DNA_CALC_REIMPLEMENTATION_PLAN.md";

/// The plan bundle that bead descriptions cite.
const PLAN_BUNDLE: [&str; 7] = [
    PLAN,
    "BEAD_GRAPH.md",
    "BEAD_DETAILS.md",
    "beads.plan.jsonl",
    "beads.br.jsonl",
    "coverage.json",
    "validate_plan.py",
];

/// What AGENTS.md must name, as (rule, exact text).
const AGENTS_REQUIREMENTS: [(&str, &str); 30] = [
    ("pick work", "`br ready`"),
    ("pick work", "`bv --robot-next`"),
    ("read a bead", "`br show <id>`"),
    (
        "claim",
        "`br update <id> --status in_progress --assignee <clone-name> --actor <clone-name>`",
    ),
    (
        "one bead per clone",
        "One bead is being built per clone at a time",
    ),
    (
        "acceptance",
        "`cargo test -p <package> --test acceptance -- --exact <case> --nocapture`",
    ),
    ("acceptance count", "must report `1 passed`"),
    ("acceptance receipt", "`target/accept/<id>/`"),
    ("acceptance manifest", "`acceptance.json`"),
    ("zero-test run", "A zero-test run"),
    ("commit", "git commit -m \"<id>: "),
    (
        "commit",
        "one build commit per bead on master, bead ID first",
    ),
    ("close", "`br close <id> --reason \""),
    ("flush the tracker", "br sync --flush-only"),
    ("follow-up bead", "br create ... --deps blocks:<current>"),
    ("prerequisite bead", "br dep add <current> <new>"),
    (
        "master only",
        "Git: master only, no branches, no worktrees, ever",
    ),
    (
        "pull, work, commit, push",
        "The loop is: `git pull --no-rebase`, work on one bead, `git commit`",
    ),
    ("pull, work, commit, push", "`git push`"),
    (
        "concurrent agents",
        "their own full clone of this repository on master",
    ),
    ("pull before starting", "Pull before starting."),
    (
        "rejected push",
        "If a push is rejected, run `git pull --no-rebase`, re-run the bead's acceptance, and push again.",
    ),
    (
        "one cargo per target directory",
        "Never run two cargo invocations in one target directory at the same time.",
    ),
    ("honesty", "Provenance before value"),
    ("honesty", "Count every fallback"),
    ("honesty", "No faked behaviour"),
    ("honesty", "No seams without traffic"),
    ("honesty", "One of everything"),
    ("honesty", "The machine is not the product"),
    ("honesty", "Measure, do not assert"),
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("tools/xtask sits two directories below the repository root")
        .to_path_buf()
}

/// A cleared evidence directory for one case.
fn receipt_dir(case: &str) -> PathBuf {
    let dir = repo_root()
        .join("target")
        .join("accept")
        .join(BEAD)
        .join(case);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("cannot clear {}: {e}", dir.display()));
    }
    fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("cannot create {}: {e}", dir.display()));
    dir
}

fn write(path: &Path, contents: impl AsRef<[u8]>) {
    fs::write(path, contents).unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
}

fn read_text(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&read_text(path))
        .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", path.display()))
}

fn strings(value: &Value, what: &str) -> Vec<String> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("{what} is not an array: {value}"))
        .iter()
        .map(|item| {
            item.as_str()
                .unwrap_or_else(|| panic!("{what} holds a non-string item: {item}"))
                .to_owned()
        })
        .collect()
}

/// This bead's entry in the xtask acceptance manifest, after checking that `case` is registered.
fn registered_entry(case: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("acceptance.json");
    let manifest = read_json(&path);
    assert_eq!(
        manifest["package"],
        "xtask",
        "{} names the wrong package",
        path.display()
    );
    let entry = manifest["beads"]
        .as_array()
        .and_then(|beads| beads.iter().find(|bead| bead["id"] == BEAD))
        .unwrap_or_else(|| panic!("{} has no entry for {BEAD}", path.display()))
        .clone();
    let cases = strings(&entry["cases"], "registered cases");
    assert!(
        cases.iter().any(|registered| registered == case),
        "{case} is not registered under {BEAD} in {}; registered: {cases:?}",
        path.display()
    );
    entry
}

/// The one registered command that `matches` selects.
fn registered_command(
    entry: &Value,
    what: &str,
    matches: impl Fn(&[String]) -> bool,
) -> Vec<String> {
    let commands: Vec<Vec<String>> = entry["commands"]
        .as_array()
        .unwrap_or_else(|| panic!("{BEAD} registers no commands"))
        .iter()
        .map(|command| strings(command, "a registered command"))
        .collect();
    let mut selected = commands.into_iter().filter(|argv| matches(argv));
    let command = selected
        .next()
        .unwrap_or_else(|| panic!("{BEAD} registers no {what} command"));
    assert!(
        selected.next().is_none(),
        "{BEAD} registers more than one {what} command"
    );
    command
}

fn target_of(argv: &[String]) -> Option<&str> {
    argv.iter()
        .position(|arg| arg == "--target")
        .and_then(|i| argv.get(i + 1))
        .map(String::as_str)
}

fn excludes_of(argv: &[String]) -> BTreeSet<String> {
    argv.windows(2)
        .filter(|pair| pair[0] == "--exclude")
        .map(|pair| pair[1].clone())
        .collect()
}

/// The cargo that inner commands run, and an evidence line saying which one it is: the `CARGO` the
/// outer cargo set, or `cargo` from PATH when `CARGO` is unset (the PATH fallback).
fn cargo_program() -> (OsString, String) {
    match std::env::var_os("CARGO") {
        Some(program) => {
            let shown = format!(
                "cargo program: {} (the CARGO variable)",
                Path::new(&program).display()
            );
            (program, shown)
        }
        None => (
            OsString::from("cargo"),
            "cargo program: cargo from PATH (PATH fallback: CARGO is unset)".to_owned(),
        ),
    }
}

/// A registered cargo command, run from the repository root in `target_dir`.
fn inner_cargo(argv: &[String], extra: &[&str], target_dir: &Path) -> Command {
    assert_eq!(
        argv.first().map(String::as_str),
        Some("cargo"),
        "{argv:?} is not a cargo command"
    );
    let mut command = Command::new(cargo_program().0);
    command
        .args(&argv[1..])
        .args(extra)
        .current_dir(repo_root())
        .env("CARGO_TARGET_DIR", target_dir);
    command
}

fn run_recorded(label: &str, mut command: Command, receipt: &Path) -> Output {
    let shown = format!("{command:?}");
    let output = command
        .output()
        .unwrap_or_else(|e| panic!("cannot start {shown}: {e}"));
    let code = output.status.code();
    write(&receipt.join(format!("{label}.stdout")), &output.stdout);
    write(&receipt.join(format!("{label}.stderr")), &output.stderr);
    write(
        &receipt.join(format!("{label}.exit")),
        format!("{shown}\nexit code: {code:?}\n"),
    );
    println!("{label}: {shown}\n  exit code: {code:?}");
    output
}

fn tail(bytes: &[u8], lines: usize) -> String {
    let text = String::from_utf8_lossy(bytes);
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// Whether `path` lies inside `dir`, comparing paths as text (case-insensitively on Windows) so
/// that cargo's spelling of the target directory need not match ours byte for byte.
fn is_within(path: &Path, dir: &Path) -> bool {
    let normal = |p: &Path| {
        let text = p.to_string_lossy().replace('\\', "/");
        if cfg!(windows) {
            text.to_lowercase()
        } else {
            text
        }
    };
    let (path, dir) = (normal(path), normal(dir));
    path.strip_prefix(dir.trim_end_matches('/'))
        .is_some_and(|rest| rest.starts_with('/'))
}

struct Member {
    name: String,
    id: String,
    dir: PathBuf,
    portability: Option<String>,
}

fn workspace_members(metadata: &Value) -> Vec<Member> {
    let root = PathBuf::from(
        metadata["workspace_root"]
            .as_str()
            .expect("metadata has a workspace_root"),
    );
    let ids = strings(&metadata["workspace_members"], "workspace_members");
    let packages = metadata["packages"]
        .as_array()
        .expect("metadata has packages");
    ids.into_iter()
        .map(|id| {
            let package = packages
                .iter()
                .find(|package| package["id"] == id.as_str())
                .unwrap_or_else(|| panic!("workspace member {id} is missing from packages"));
            let manifest = PathBuf::from(
                package["manifest_path"]
                    .as_str()
                    .expect("package has a manifest_path"),
            );
            let dir = manifest
                .parent()
                .and_then(|dir| dir.strip_prefix(&root).ok())
                .unwrap_or_else(|| panic!("{} is outside the workspace root", manifest.display()))
                .to_path_buf();
            Member {
                name: package["name"]
                    .as_str()
                    .expect("package has a name")
                    .to_owned(),
                id,
                dir,
                portability: package["metadata"]["dnacalc"]["portability"]
                    .as_str()
                    .map(str::to_owned),
            }
        })
        .collect()
}

struct Artifact {
    package_id: String,
    kinds: Vec<String>,
    filenames: Vec<PathBuf>,
    fresh: bool,
}

/// The compiler-artifact messages cargo printed with `--message-format=json-render-diagnostics`.
/// A message without a string `package_id` or a boolean `fresh` is recorded as a failure and never
/// read as a default.
fn compiler_artifacts(stdout: &[u8], platform: &str, failures: &mut Vec<String>) -> Vec<Artifact> {
    let mut artifacts = Vec::new();
    for line in String::from_utf8_lossy(stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        let message: Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("cargo printed a line that is not JSON ({e}): {line}"));
        if message["reason"] != "compiler-artifact" {
            continue;
        }
        let (Some(package_id), Some(fresh)) =
            (message["package_id"].as_str(), message["fresh"].as_bool())
        else {
            failures.push(format!(
                "{platform}: a compiler-artifact message lacks a string package_id or a boolean fresh: {line}"
            ));
            continue;
        };
        artifacts.push(Artifact {
            package_id: package_id.to_owned(),
            kinds: strings(&message["target"]["kind"], "an artifact's target kinds"),
            filenames: strings(&message["filenames"], "an artifact's filenames")
                .into_iter()
                .map(PathBuf::from)
                .collect(),
            fresh,
        });
    }
    artifacts
}

/// Checks that cargo reported `member` compiled into `output_dir` in this run, with every reported
/// file on disk.
fn check_compiled(
    member: &Member,
    artifacts: &[Artifact],
    output_dir: &Path,
    platform: &str,
    evidence: &mut Vec<String>,
    failures: &mut Vec<String>,
) {
    let own: Vec<&Artifact> = artifacts
        .iter()
        .filter(|artifact| {
            artifact.package_id == member.id
                && !artifact.kinds.iter().any(|kind| kind == "custom-build")
        })
        .collect();
    if own.is_empty() {
        failures.push(format!(
            "{}: cargo reported no compiled artifact for {platform}",
            member.name
        ));
        return;
    }
    for artifact in own {
        if artifact.filenames.is_empty() {
            failures.push(format!(
                "{}: a {platform} artifact lists no files",
                member.name
            ));
        }
        if artifact.fresh {
            failures.push(format!(
                "{}: cargo reused a {platform} artifact instead of compiling it in this run",
                member.name
            ));
        }
        for file in &artifact.filenames {
            if !is_within(file, output_dir) {
                failures.push(format!(
                    "{}: {} is not under the {platform} output directory {}",
                    member.name,
                    file.display(),
                    output_dir.display()
                ));
            } else if !file.is_file() {
                failures.push(format!(
                    "{}: reported {platform} file {} does not exist",
                    member.name,
                    file.display()
                ));
            }
        }
        evidence.push(format!(
            "{platform}: {} {:?} fresh={} {}",
            member.name,
            artifact.kinds,
            artifact.fresh,
            artifact
                .filenames
                .iter()
                .map(|f| f.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
}

fn finish(case: &str, receipt: &Path, evidence: &[String], failures: &[String]) {
    let mut summary = format!("{case}\n\nevidence:\n");
    for line in evidence {
        summary.push_str(&format!("  {line}\n"));
    }
    summary.push_str(&format!("\nfailures: {}\n", failures.len()));
    for line in failures {
        summary.push_str(&format!("  {line}\n"));
    }
    write(&receipt.join("summary.txt"), &summary);
    println!("{summary}");
    assert!(
        failures.is_empty(),
        "{case} failed:\n  {}",
        failures.join("\n  ")
    );
}

/// Every portable package builds natively and for wasm32-unknown-unknown; the workspace has
/// exactly the bead's members, and the wasm32 check excludes exactly the native-only ones.
#[test]
fn infra_m0_1__workspace_builds() {
    const CASE: &str = "infra_m0_1__workspace_builds";
    let _serial = serial();
    let entry = registered_entry(CASE);
    let receipt = receipt_dir(CASE);
    let root = repo_root();
    let inner_target = Path::new(env!("CARGO_TARGET_TMPDIR")).join(CASE);
    // Every artifact must come from this run, not from an earlier one.
    remove_tree(&inner_target).unwrap_or_else(|e| panic!("cannot clear the inner target: {e}"));
    let mut evidence = vec![
        cargo_program().1,
        format!("inner CARGO_TARGET_DIR: {}", inner_target.display()),
    ];
    let mut failures = Vec::new();

    let metadata_argv = registered_command(&entry, "cargo metadata", |argv| {
        argv.get(1).is_some_and(|arg| arg == "metadata")
    });
    let output = run_recorded(
        "1-cargo-metadata",
        inner_cargo(&metadata_argv, &[], &inner_target),
        &receipt,
    );
    assert!(
        output.status.success(),
        "cargo metadata failed:\n{}",
        tail(&output.stderr, 40)
    );
    let metadata: Value =
        serde_json::from_slice(&output.stdout).expect("cargo metadata printed JSON");
    let members = workspace_members(&metadata);

    let by_name: BTreeMap<&str, &Member> = members
        .iter()
        .map(|member| (member.name.as_str(), member))
        .collect();
    for (name, dir) in EXPECTED_PACKAGES {
        match by_name.get(name) {
            None => failures.push(format!("workspace member {name} is missing")),
            Some(member) if member.dir != Path::new(dir) => failures.push(format!(
                "{name} is at {}, expected {dir}",
                member.dir.display()
            )),
            Some(member) => evidence.push(format!("member {name} at {}", member.dir.display())),
        }
    }
    for member in &members {
        if !EXPECTED_PACKAGES
            .iter()
            .any(|(name, _)| *name == member.name)
        {
            failures.push(format!(
                "unexpected workspace member {} at {}",
                member.name,
                member.dir.display()
            ));
        }
    }

    let mut native_only = BTreeSet::new();
    for member in &members {
        match member.portability.as_deref() {
            Some("native-only") => {
                native_only.insert(member.name.clone());
            }
            Some("portable") => {}
            other => failures.push(format!(
                "{} declares portability {other:?}; expected \"portable\" or \"native-only\"",
                member.name
            )),
        }
        let manifest = root.join(&member.dir).join("acceptance.json");
        if manifest.is_file() {
            let parsed = read_json(&manifest);
            if parsed["package"] != member.name.as_str() || !parsed["beads"].is_array() {
                failures.push(format!(
                    "{} does not name {} with a beads list",
                    manifest.display(),
                    member.name
                ));
            }
        } else {
            failures.push(format!(
                "{} has no acceptance manifest at {}",
                member.name,
                manifest.display()
            ));
        }
        if !root.join(&member.dir).join("tests").is_dir() {
            failures.push(format!("{} has no tests directory", member.name));
        }
    }
    let expected_native: BTreeSet<String> = EXPECTED_NATIVE_ONLY
        .iter()
        .map(|name| name.to_string())
        .collect();
    if native_only != expected_native {
        failures.push(format!(
            "declared native-only packages {native_only:?}, expected {expected_native:?}"
        ));
    }
    evidence.push(format!("declared native-only: {native_only:?}"));

    let wasm_argv = registered_command(&entry, "wasm32 cargo check", |argv| {
        argv.get(1).is_some_and(|arg| arg == "check") && target_of(argv) == Some(WASM_TARGET)
    });
    if !wasm_argv.iter().any(|arg| arg == "--workspace") {
        failures.push(format!(
            "the wasm32 check {wasm_argv:?} does not cover the workspace"
        ));
    }
    let excluded = excludes_of(&wasm_argv);
    if excluded != native_only {
        failures.push(format!(
            "the wasm32 check excludes {excluded:?}, but the declared native-only packages are {native_only:?}"
        ));
    }
    evidence.push(format!("wasm32 check excludes: {excluded:?}"));

    let json = ["--locked", "--message-format=json-render-diagnostics"];

    let native_argv = registered_command(&entry, "native cargo check", |argv| {
        argv.get(1).is_some_and(|arg| arg == "check") && target_of(argv).is_none()
    });
    let output = run_recorded(
        "2-cargo-check-native",
        inner_cargo(&native_argv, &json, &inner_target),
        &receipt,
    );
    if output.status.success() {
        let artifacts = compiler_artifacts(&output.stdout, "native", &mut failures);
        for member in &members {
            check_compiled(
                member,
                &artifacts,
                &inner_target.join("debug"),
                "native",
                &mut evidence,
                &mut failures,
            );
        }
    } else {
        failures.push(format!(
            "the native check exited with {:?}:\n{}",
            output.status.code(),
            tail(&output.stderr, 40)
        ));
    }

    let output = run_recorded(
        "3-cargo-check-wasm32",
        inner_cargo(&wasm_argv, &json, &inner_target),
        &receipt,
    );
    if output.status.success() {
        let artifacts = compiler_artifacts(&output.stdout, WASM_TARGET, &mut failures);
        for member in &members {
            if native_only.contains(&member.name) {
                if artifacts
                    .iter()
                    .any(|artifact| artifact.package_id == member.id)
                {
                    failures.push(format!(
                        "native-only {} was compiled for wasm32",
                        member.name
                    ));
                }
            } else {
                let output_dir = inner_target.join(WASM_TARGET);
                check_compiled(
                    member,
                    &artifacts,
                    &output_dir,
                    WASM_TARGET,
                    &mut evidence,
                    &mut failures,
                );
            }
        }
    } else {
        failures.push(format!(
            "the wasm32 check exited with {:?}:\n{}",
            output.status.code(),
            tail(&output.stderr, 40)
        ));
    }

    finish(CASE, &receipt, &evidence, &failures);
}

/// The raw standard output of a git command, or why it did not succeed.
fn git_output(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run git {args:?} in {}: {e}", dir.display()))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(format!(
            "git {args:?} in {} exited with {:?}: {}",
            dir.display(),
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    git_output(dir, args).map(|out| out.trim().to_owned())
}

/// A git result as text: the output, or the error.
fn shown_git(result: &Result<String, String>) -> &str {
    match result {
        Ok(text) | Err(text) => text,
    }
}

/// One snapshot entry: a file's size, modification time and, when hashed, content hash. A
/// directory records only that it exists.
type Entry = (u64, Option<SystemTime>, Option<u64>);
type Snapshot = BTreeMap<PathBuf, Entry>;

/// Every file and directory under `root` except `skip` and what lies below it, keyed by its path
/// relative to `root`. Files record size and modification time, plus a content hash when
/// `hash_contents` is set; directories record only that they exist. An entry that disappears
/// between being listed and being read is left out.
fn snapshot(root: &Path, skip: Option<&Path>, hash_contents: bool) -> Snapshot {
    let mut entries = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in
            fs::read_dir(&dir).unwrap_or_else(|e| panic!("cannot list {}: {e}", dir.display()))
        {
            let path = entry.expect("directory entry").path();
            if skip == Some(path.as_path()) {
                continue;
            }
            let meta = match fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => panic!("cannot stat {}: {e}", path.display()),
            };
            let relative = path
                .strip_prefix(root)
                .expect("entry under the root")
                .to_path_buf();
            if meta.is_dir() {
                entries.insert(relative, (0, None, None));
                pending.push(path);
            } else {
                let modified = meta
                    .modified()
                    .unwrap_or_else(|e| panic!("no modification time for {}: {e}", path.display()));
                let hash = hash_contents.then(|| {
                    let mut hasher = DefaultHasher::new();
                    hasher.write(
                        &fs::read(&path)
                            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display())),
                    );
                    hasher.finish()
                });
                entries.insert(relative, (meta.len(), Some(modified), hash));
            }
        }
    }
    entries
}

/// The paths that were added, removed or changed between two snapshots.
fn changed_paths(before: &Snapshot, after: &Snapshot) -> Vec<PathBuf> {
    before
        .keys()
        .chain(after.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|path| before.get(*path) != after.get(*path))
        .cloned()
        .collect()
}

fn shown_paths(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect()
}

fn remove_tree(path: &Path) -> Result<(), String> {
    let mut last_error = String::new();
    for _ in 0..5 {
        if !path.exists() {
            return Ok(());
        }
        match fs::remove_dir_all(path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_error = e.to_string();
                clear_readonly(path);
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
    Err(format!("cannot delete {}: {last_error}", path.display()))
}

/// Git marks object files read-only, which can stop a recursive delete on Windows.
// Windows only, where this clears the read-only attribute rather than widening Unix modes.
#[cfg(windows)]
#[allow(clippy::permissions_set_readonly_false)]
fn clear_readonly(path: &Path) {
    let mut pending = vec![path.to_path_buf()];
    while let Some(next) = pending.pop() {
        let Ok(meta) = fs::symlink_metadata(&next) else {
            continue;
        };
        if meta.is_dir() {
            if let Ok(entries) = fs::read_dir(&next) {
                pending.extend(entries.filter_map(|entry| entry.ok().map(|entry| entry.path())));
            }
        } else if meta.permissions().readonly() {
            let mut permissions = meta.permissions();
            permissions.set_readonly(false);
            let _ = fs::set_permissions(&next, permissions);
        }
    }
}

#[cfg(not(windows))]
fn clear_readonly(_path: &Path) {}

/// The cargo home cargo itself uses, and where that came from: `CARGO_HOME` when it is set and not
/// empty, otherwise `.cargo` under the home directory.
fn cargo_home() -> (PathBuf, &'static str) {
    match std::env::var_os("CARGO_HOME").filter(|dir| !dir.is_empty()) {
        Some(dir) => {
            let dir = PathBuf::from(dir);
            assert!(
                dir.is_absolute(),
                "CARGO_HOME={} is relative, so the lane builds would each resolve it from their own clone",
                dir.display()
            );
            (dir, "the CARGO_HOME variable")
        }
        None => (
            std::env::home_dir()
                .expect("a home directory")
                .join(".cargo"),
            "the default .cargo under the home directory (CARGO_HOME is unset)",
        ),
    }
}

/// What `cargo metadata --no-deps` reports in one lane clone.
struct CloneMetadata {
    target_directory: PathBuf,
    /// Each workspace package's normal and build dependencies on other workspace packages.
    workspace_deps: BTreeMap<String, BTreeSet<String>>,
}

fn clone_metadata(stdout: &[u8]) -> Result<CloneMetadata, String> {
    let metadata: Value = serde_json::from_slice(stdout)
        .map_err(|e| format!("cargo metadata printed no JSON: {e}"))?;
    let target_directory = metadata["target_directory"]
        .as_str()
        .ok_or("cargo metadata reports no target_directory")?;
    let packages = metadata["packages"]
        .as_array()
        .ok_or("cargo metadata reports no packages")?;
    let names = packages
        .iter()
        .map(|package| package["name"].as_str().ok_or("a package has no name"))
        .collect::<Result<BTreeSet<&str>, _>>()?;
    let mut workspace_deps = BTreeMap::new();
    for package in packages {
        let name = package["name"].as_str().ok_or("a package has no name")?;
        let mut on = BTreeSet::new();
        for dependency in package["dependencies"]
            .as_array()
            .ok_or_else(|| format!("{name} has no dependencies list"))?
        {
            let dependency_name = dependency["name"]
                .as_str()
                .ok_or_else(|| format!("{name} has a dependency without a name"))?;
            if names.contains(dependency_name) && dependency["kind"] != "dev" {
                on.insert(dependency_name.to_owned());
            }
        }
        workspace_deps.insert(name.to_owned(), on);
    }
    Ok(CloneMetadata {
        target_directory: PathBuf::from(target_directory),
        workspace_deps,
    })
}

/// `package` and every workspace package it depends on, directly or through others.
fn with_workspace_deps(
    package: &str,
    workspace_deps: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut closure = BTreeSet::from([package.to_owned()]);
    let mut pending = vec![package.to_owned()];
    while let Some(next) = pending.pop() {
        for dependency in workspace_deps.get(&next).into_iter().flatten() {
            if closure.insert(dependency.clone()) {
                pending.push(dependency.clone());
            }
        }
    }
    closure
}

/// Whether cargo's `target_directory` is `<clone>/target`, comparing canonical paths so that short
/// and long spellings of one directory agree. Anything that cannot be resolved does not match.
fn is_clone_target(target_directory: &Path, clone: &Path) -> bool {
    let (Some(parent), Ok(clone)) = (target_directory.parent(), fs::canonicalize(clone)) else {
        return false;
    };
    target_directory.file_name() == Some(OsStr::new("target"))
        && fs::canonicalize(parent).is_ok_and(|parent| parent == clone)
}

/// The packages a cargo fingerprint directory holds build state for. Its entries are named
/// `<package>-<16 hex digits>`; any other entry is an error rather than skipped.
fn fingerprint_packages(dir: &Path) -> Result<BTreeSet<String>, String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("cannot list {}: {e}", dir.display()))?;
    let mut packages = BTreeSet::new();
    for entry in entries {
        let name = entry
            .map_err(|e| format!("cannot list {}: {e}", dir.display()))?
            .file_name()
            .to_string_lossy()
            .into_owned();
        match name.rsplit_once('-') {
            Some((package, hash))
                if !package.is_empty()
                    && hash.len() == 16
                    && hash.chars().all(|c| c.is_ascii_hexdigit()) =>
            {
                packages.insert(package.to_owned());
            }
            _ => {
                return Err(format!(
                    "{} holds {name}, which is not named <package>-<hash>",
                    dir.display()
                ));
            }
        }
    }
    Ok(packages)
}

/// How a lane build ended: it exited, or it was still running at `LANE_DEADLINE` and was killed.
#[derive(Clone, Copy)]
enum LaneEnd {
    /// `at` is when `try_wait` first saw the exit, and `seen_by` names the poll that saw it.
    Exited {
        at: Duration,
        code: Option<i32>,
        seen_by: &'static str,
    },
    Killed {
        at: Duration,
    },
}

impl LaneEnd {
    fn at(self) -> Duration {
        match self {
            LaneEnd::Exited { at, .. } | LaneEnd::Killed { at } => at,
        }
    }
}

const SEEN_BEFORE_LATER_SPAWN: &str = "the poll before a later lane started";
const SEEN_AFTER_LAST_SPAWN: &str = "a poll after every lane started";

/// One library lane: its clone, what cargo and git report for the clone, its build log name in the
/// receipt, the clone's state outside `target/` before the build, and what was measured after it.
struct Lane {
    name: &'static str,
    package: &'static str,
    log: String,
    clone: PathBuf,
    cloned: bool,
    /// The clone's own `git rev-parse HEAD`.
    commit: Option<String>,
    target_directory: Option<PathBuf>,
    workspace_members: BTreeSet<String>,
    /// The lane's package and the workspace packages it depends on.
    allowed: BTreeSet<String>,
    before: Snapshot,
    /// When the build's spawn returned.
    started: Option<Duration>,
    end: Option<LaneEnd>,
    changed_outside_target: Vec<String>,
    fingerprint_packages: Option<BTreeSet<String>>,
    foreign_packages: Vec<String>,
}

/// Deletes the lane clones if the case panics before its own cleanup.
struct DeleteOnDrop(PathBuf);

impl Drop for DeleteOnDrop {
    fn drop(&mut self) {
        let _ = remove_tree(&self.0);
    }
}

/// Kills and waits every lane build that is still running when it drops, so a panic after the
/// spawns does not leave cargo running. The case declares it after the clone guard, so it drops
/// first. Killing cargo does not necessarily stop the compiler processes it started.
struct KillOnDrop(Vec<Child>);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        for child in &mut self.0 {
            if matches!(child.try_wait(), Ok(None)) {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}

/// Four clones of this checkout on master build the Func, Fml, Doc and Engine packages at the same
/// time, each in its own target directory. A clone sees only committed work, so this case runs
/// after the build commit. What it measures, and nothing more:
///
/// - Before any clone is made: the source checkout is on master and
///   `git status --porcelain=v1 --untracked-files=all` prints nothing (`target/` is ignored).
///   Otherwise no clone is made and the case fails, because the clones would build `HEAD` without
///   the uncommitted work.
/// - Before any build: each clone's own `HEAD`, which must be the source's, and each clone's
///   target directory as `cargo metadata` reports it in the clone, with `CARGO_TARGET_DIR` and
///   `CARGO_BUILD_TARGET_DIR` removed from its environment exactly as for the builds. Unless it is
///   `<clone>/target` for every clone, no build runs and the case fails, so a shared
///   `build.target-dir` in a committed `.cargo/config.toml` fails it.
/// - Each clone's tree outside `target/`, before and after the builds: every directory, and every
///   file's size, modification time and content hash.
/// - The cargo home (`CARGO_HOME`, or `.cargo` under the home directory), before and after the
///   builds: every directory, and every file's size and modification time. Any added, removed or
///   changed path fails.
/// - The source checkout: `HEAD` and `git status --porcelain=v1 --untracked-files=all`, from before
///   the clones are made to after the builds end; any difference fails.
/// - Each lane's `target/debug/.fingerprint` after the builds: the packages it holds build state
///   for, which must include the lane's package and no other workspace package outside that
///   package's own workspace dependencies (it has none today).
/// - Each build's start (when its spawn returned), its end (when `try_wait` first saw it exit:
///   every started build is polled before each later spawn and every 5 ms after the last), and its
///   exit code. A build that exits before the last one starts fails the overlap check, and so does
///   any run in which no poll after the last start finds all four running. A build still running
///   at `LANE_DEADLINE` is killed and fails the case.
///
/// `lanes.json` and `summary.txt` are written when a clone or a build fails; a panic elsewhere in
/// the case leaves the receipt folder without them, and the case still fails. Other locations, such
/// as the OS temp directory outside the clones or the rustup home, are not measured.
#[test]
fn infra_m0_1__independent_lanes() {
    const CASE: &str = "infra_m0_1__independent_lanes";
    const MEASURED: [&str; 7] = [
        "before any clone is made: the source checkout is on master and `git status --porcelain=v1 --untracked-files=all` prints nothing (target/ is ignored); otherwise no clone is made",
        "before any build: each clone's own HEAD, which must equal the source's, and each clone's target directory as `cargo metadata --no-deps --format-version 1 --offline --locked` reports it in the clone with CARGO_TARGET_DIR and CARGO_BUILD_TARGET_DIR removed; it must be <clone>/target for every clone, or no build runs",
        "each clone's tree outside target/, before and after the builds: every directory, and every file's size, modification time and content hash",
        "the cargo home, before and after the builds: every directory, and every file's size and modification time; any added, removed or changed path fails",
        "the source checkout: HEAD and `git status --porcelain=v1 --untracked-files=all`, from before the clones are made to after the builds end",
        "each lane's target/debug/.fingerprint after the builds: the packages it holds build state for; no other workspace package outside the lane package's workspace dependencies may appear",
        "each build's start (when its spawn returned), end (when try_wait first saw it exit, polling every started build before each later spawn and every 5 ms after the last) and exit code; a build that ended before the last started fails, a poll after the last start must find all four running, and a build still running at the deadline is killed and fails",
    ];
    const NOT_MEASURED: &str = "any other location, including the OS temp directory outside the lane clones and the rustup home";
    const STATUS: [&str; 4] = [
        "--no-optional-locks",
        "status",
        "--porcelain=v1",
        "--untracked-files=all",
    ];
    let _serial = serial();
    registered_entry(CASE);
    let receipt = receipt_dir(CASE);
    let source = repo_root();
    let (cargo, cargo_shown) = cargo_program();
    let mut evidence = vec![
        cargo_shown.clone(),
        format!("lane build deadline: {LANE_DEADLINE:?}"),
    ];
    let mut failures = Vec::new();

    let outer_environment: BTreeMap<&str, Option<String>> = TARGET_DIR_VARIABLES
        .iter()
        .map(|&name| {
            let value = std::env::var_os(name).map(|value| value.to_string_lossy().into_owned());
            (name, value)
        })
        .collect();
    for (name, value) in &outer_environment {
        evidence.push(match value {
            Some(value) => {
                format!("outer environment sets {name}={value}; every lane cargo runs without it")
            }
            None => format!(
                "outer environment does not set {name}; every lane cargo runs without it regardless"
            ),
        });
    }

    let branch = git(&source, &["symbolic-ref", "--short", "HEAD"]);
    let head = git(&source, &["rev-parse", "HEAD"]);
    let status_before = git_output(&source, &STATUS);
    match &branch {
        Ok(branch) if branch == "master" => {}
        Ok(branch) => failures.push(format!(
            "source {} is on {branch}, not master",
            source.display()
        )),
        Err(e) => failures.push(format!("source: {e}")),
    }
    if let Err(e) = &head {
        failures.push(format!("source: {e}"));
    }
    let source_clean = matches!(&status_before, Ok(status) if status.is_empty());
    match &status_before {
        Ok(_) if source_clean => evidence.push(format!(
            "source {} on {} at {}; `git status --porcelain=v1 --untracked-files=all` prints nothing",
            source.display(),
            shown_git(&branch),
            shown_git(&head)
        )),
        Ok(status) => failures.push(format!(
            "source {} has uncommitted or untracked files, and the clones would build HEAD without them; commit them first. git status --porcelain=v1 --untracked-files=all:\n{}",
            source.display(),
            status.trim_end()
        )),
        Err(e) => failures.push(format!("source: {e}")),
    }

    let lanes_root = std::env::temp_dir().join("dnacalc-lanes");
    remove_tree(&lanes_root).unwrap_or_else(|e| panic!("stale lane clones: {e}"));
    let clones = DeleteOnDrop(lanes_root.clone());

    let mut lanes: Vec<Lane> = LANES
        .iter()
        .enumerate()
        .map(|(index, &(name, package))| Lane {
            name,
            package,
            log: format!("lane-{}-{package}.log", index + 1),
            clone: lanes_root.join((index + 1).to_string()),
            cloned: false,
            commit: None,
            target_directory: None,
            workspace_members: BTreeSet::new(),
            allowed: BTreeSet::new(),
            before: Snapshot::new(),
            started: None,
            end: None,
            changed_outside_target: Vec::new(),
            fingerprint_packages: None,
            foreign_packages: Vec::new(),
        })
        .collect();

    if !failures.is_empty() {
        evidence.push(
            "no clone was made, because of the failures found in the source checkout".to_owned(),
        );
    } else {
        for (index, lane) in lanes.iter_mut().enumerate() {
            let (name, package) = (lane.name, lane.package);
            match Command::new("git")
                .args(["clone", "--no-hardlinks", "--quiet"])
                .arg(&source)
                .arg(&lane.clone)
                .stdin(Stdio::null())
                .output()
            {
                Err(e) => {
                    failures.push(format!("lane {name}: cannot run git clone: {e}"));
                    continue;
                }
                Ok(output) if !output.status.success() => {
                    failures.push(format!(
                        "lane {name}: git clone into {} exited with {:?}:\n{}",
                        lane.clone.display(),
                        output.status.code(),
                        tail(&output.stderr, 20)
                    ));
                    continue;
                }
                Ok(_) => lane.cloned = true,
            }
            let clone_branch = git(&lane.clone, &["symbolic-ref", "--short", "HEAD"]);
            let clone_head = git(&lane.clone, &["rev-parse", "HEAD"]);
            let on_source_head =
                clone_branch.as_deref() == Ok("master") && clone_head.is_ok() && clone_head == head;
            let line = format!(
                "lane {name}: clone {} on {} at {}",
                lane.clone.display(),
                shown_git(&clone_branch),
                shown_git(&clone_head)
            );
            if on_source_head {
                evidence.push(line);
            } else {
                failures.push(format!(
                    "{line}; the source is on master at {}",
                    shown_git(&head)
                ));
            }
            lane.commit = clone_head.ok();

            let mut command = Command::new(&cargo);
            command
                .args([
                    "metadata",
                    "--no-deps",
                    "--format-version",
                    "1",
                    "--offline",
                    "--locked",
                ])
                .current_dir(&lane.clone);
            for variable in TARGET_DIR_VARIABLES {
                command.env_remove(variable);
            }
            let output = run_recorded(&format!("lane-{}-metadata", index + 1), command, &receipt);
            if !output.status.success() {
                failures.push(format!(
                    "lane {name}: cargo metadata exited with {:?}:\n{}",
                    output.status.code(),
                    tail(&output.stderr, 20)
                ));
            } else {
                match clone_metadata(&output.stdout) {
                    Err(e) => failures.push(format!("lane {name}: {e}")),
                    Ok(metadata) => {
                        if is_clone_target(&metadata.target_directory, &lane.clone) {
                            evidence.push(format!(
                                "lane {name}: cargo reports target directory {}, the clone's own target/",
                                metadata.target_directory.display()
                            ));
                        } else {
                            failures.push(format!(
                                "lane {name}: cargo reports target directory {}, not {}",
                                metadata.target_directory.display(),
                                lane.clone.join("target").display()
                            ));
                        }
                        if !metadata.workspace_deps.contains_key(package) {
                            failures
                                .push(format!("lane {name}: {package} is not a workspace member"));
                        }
                        lane.allowed = with_workspace_deps(package, &metadata.workspace_deps);
                        lane.workspace_members = metadata.workspace_deps.into_keys().collect();
                        lane.target_directory = Some(metadata.target_directory);
                    }
                }
            }
            if lane.clone.join("target").exists() {
                failures.push(format!(
                    "lane {name}: clone {} has a target directory before its build",
                    lane.clone.display()
                ));
            }
        }
    }

    let (home, home_source) = cargo_home();
    let mut cargo_home_record = serde_json::json!({
        "path": home.display().to_string(),
        "from": home_source,
    });
    let mut overlap_record = Value::Null;
    let builds_run = failures.is_empty();
    if builds_run {
        for lane in &mut lanes {
            lane.before = snapshot(&lane.clone, Some(&lane.clone.join("target")), true);
        }
        let home_before = snapshot(&home, None, false);

        let start = Instant::now();
        // Declared after the clone guard, so on a panic the builds are killed before the clones go.
        let mut builds = KillOnDrop(Vec::new());
        for index in 0..lanes.len() {
            // A build that has already exited gets an end time before this lane's start, which
            // fails the overlap check below.
            for (earlier, child) in lanes[..index].iter_mut().zip(&mut builds.0) {
                if earlier.end.is_none()
                    && let Some(status) = child.try_wait().expect("poll a lane build")
                {
                    earlier.end = Some(LaneEnd::Exited {
                        at: start.elapsed(),
                        code: status.code(),
                        seen_by: SEEN_BEFORE_LATER_SPAWN,
                    });
                }
            }
            let lane = &mut lanes[index];
            let log_path = receipt.join(&lane.log);
            let log = fs::File::create(&log_path)
                .unwrap_or_else(|e| panic!("cannot create {}: {e}", log_path.display()));
            let mut command = Command::new(&cargo);
            command
                .args(["check", "-p", lane.package, "--locked", "--offline"])
                .current_dir(&lane.clone)
                .stdin(Stdio::null())
                .stdout(log.try_clone().expect("log handle"))
                .stderr(log);
            for variable in TARGET_DIR_VARIABLES {
                command.env_remove(variable);
            }
            let shown = format!("{command:?}");
            let child = command
                .spawn()
                .unwrap_or_else(|e| panic!("cannot start {shown}: {e}"));
            lane.started = Some(start.elapsed());
            builds.0.push(child);
            evidence.push(format!(
                "lane {}: started {shown} without {TARGET_DIR_VARIABLES:?}",
                lane.name
            ));
        }

        let deadline = start + LANE_DEADLINE;
        let mut all_running_at = None;
        while lanes.iter().any(|lane| lane.end.is_none()) {
            let pass_at = start.elapsed();
            let mut running = 0;
            for (lane, child) in lanes.iter_mut().zip(&mut builds.0) {
                if lane.end.is_none() {
                    match child.try_wait().expect("poll a lane build") {
                        Some(status) => {
                            lane.end = Some(LaneEnd::Exited {
                                at: start.elapsed(),
                                code: status.code(),
                                seen_by: SEEN_AFTER_LAST_SPAWN,
                            })
                        }
                        None => running += 1,
                    }
                }
            }
            // Every build had started before `pass_at`, and one seen running later in this pass
            // was running at `pass_at` too.
            if running == lanes.len() && all_running_at.is_none() {
                all_running_at = Some(pass_at);
            }
            if Instant::now() >= deadline {
                // Killing cargo does not necessarily stop the compiler processes it started; the
                // case fails either way, and the after-snapshots below may then see their writes.
                for (lane, child) in lanes.iter_mut().zip(&mut builds.0) {
                    if lane.end.is_none() {
                        let killed = child.kill().and_then(|()| child.wait());
                        lane.end = Some(LaneEnd::Killed {
                            at: start.elapsed(),
                        });
                        failures.push(format!(
                            "lane {}: cargo check -p {} was still running at the {LANE_DEADLINE:?} deadline and was killed ({killed:?})",
                            lane.name,
                            lane.package,
                        ));
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        // Every build has been waited for.
        drop(builds);

        let home_after = snapshot(&home, None, false);
        let home_changes = shown_paths(&changed_paths(&home_before, &home_after));
        evidence.push(format!(
            "cargo home {} ({home_source}): {} entries before the builds, {} after; added, removed or changed paths: {home_changes:?}",
            home.display(),
            home_before.len(),
            home_after.len(),
        ));
        if !home_changes.is_empty() {
            failures.push(format!(
                "the lane builds changed the shared cargo home {}: {home_changes:?}",
                home.display(),
            ));
        }
        cargo_home_record["entries_before"] = home_before.len().into();
        cargo_home_record["entries_after"] = home_after.len().into();
        cargo_home_record["changed_paths"] = home_changes.into();

        for lane in &mut lanes {
            let (name, package) = (lane.name, lane.package);
            let started = lane.started.expect("every lane build started");
            match lane.end.expect("every lane build ended") {
                LaneEnd::Exited { at, code, seen_by } => {
                    evidence.push(format!(
                        "lane {name}: cargo check -p {package} exit code {code:?}; started {:.3}s, exit seen at {:.3}s by {seen_by}",
                        started.as_secs_f64(),
                        at.as_secs_f64()
                    ));
                    if code != Some(0) {
                        let log = read_text(&receipt.join(&lane.log));
                        failures.push(format!(
                            "lane {name}: cargo check -p {package} exited with {code:?}:\n{log}"
                        ));
                    }
                }
                LaneEnd::Killed { at } => evidence.push(format!(
                    "lane {name}: cargo check -p {package} started {:.3}s, killed at {:.3}s",
                    started.as_secs_f64(),
                    at.as_secs_f64()
                )),
            }

            let after = snapshot(&lane.clone, Some(&lane.clone.join("target")), true);
            lane.changed_outside_target = shown_paths(&changed_paths(&lane.before, &after));
            if lane.changed_outside_target.is_empty() {
                evidence.push(format!(
                    "lane {name}: no file or directory outside target/ changed ({} entries compared)",
                    after.len()
                ));
            } else {
                failures.push(format!(
                    "lane {name}: build wrote outside its target/: {:?}",
                    lane.changed_outside_target
                ));
            }

            let fingerprints = lane.clone.join("target").join("debug").join(".fingerprint");
            match fingerprint_packages(&fingerprints) {
                Err(e) => failures.push(format!("lane {name}: {e}")),
                Ok(held) => {
                    lane.foreign_packages = held
                        .iter()
                        .filter(|held| {
                            lane.workspace_members.contains(*held) && !lane.allowed.contains(*held)
                        })
                        .cloned()
                        .collect();
                    evidence.push(format!(
                        "lane {name}: {} holds build state for {held:?}; allowed workspace packages {:?}",
                        fingerprints.display(),
                        lane.allowed
                    ));
                    if !held.contains(package) {
                        failures.push(format!(
                            "lane {name}: no {package} build state in {}",
                            fingerprints.display()
                        ));
                    }
                    if !lane.foreign_packages.is_empty() {
                        failures.push(format!(
                            "lane {name}: {} holds build state for other workspace packages {:?}",
                            fingerprints.display(),
                            lane.foreign_packages
                        ));
                    }
                    lane.fingerprint_packages = Some(held);
                }
            }
        }

        let last_start = lanes
            .iter()
            .filter_map(|lane| lane.started)
            .max()
            .expect("four lanes started");
        let first_end = lanes
            .iter()
            .filter_map(|lane| lane.end.map(LaneEnd::at))
            .min()
            .expect("four lanes ended");
        let all_running = match all_running_at {
            Some(at) => format!("a poll at {:.3}s found all four running", at.as_secs_f64()),
            None => "no poll after the last start found all four running".to_owned(),
        };
        if last_start < first_end && all_running_at.is_some() {
            evidence.push(format!(
                "all four builds were running together: the last started at {:.3}s, {all_running}, and the first exit was seen at {:.3}s",
                last_start.as_secs_f64(),
                first_end.as_secs_f64()
            ));
        } else {
            failures.push(format!(
                "the builds did not overlap: the last started at {:.3}s, the first exit was seen at {:.3}s, and {all_running}",
                last_start.as_secs_f64(),
                first_end.as_secs_f64()
            ));
        }
        overlap_record = serde_json::json!({
            "last_start_s": last_start.as_secs_f64(),
            "first_end_s": first_end.as_secs_f64(),
            "all_running_seen_s": all_running_at.map(|at| at.as_secs_f64()),
        });
    } else {
        evidence
            .push("no lane build ran, because of the failures found before the builds".to_owned());
    }

    let head_after = git(&source, &["rev-parse", "HEAD"]);
    let status_after = git_output(&source, &STATUS);
    let source_unchanged = head.is_ok()
        && head_after == head
        && status_before.is_ok()
        && status_after == status_before;
    if source_unchanged {
        evidence.push(format!(
            "source checkout unchanged: HEAD {} and git status ({} entries) identical at the start of the case and after the lane work",
            shown_git(&head),
            shown_git(&status_after).lines().count()
        ));
    } else {
        failures.push(format!(
            "the source checkout changed or could not be read: HEAD {} -> {}; git status before:\n{}\ngit status after:\n{}",
            shown_git(&head),
            shown_git(&head_after),
            shown_git(&status_before),
            shown_git(&status_after)
        ));
    }

    let lanes_root_existed = lanes_root.exists();
    let lanes_root_deleted = match remove_tree(&lanes_root) {
        Ok(()) if lanes_root_existed => {
            evidence.push(format!("deleted the clones under {}", lanes_root.display()));
            true
        }
        Ok(()) => {
            evidence.push(format!(
                "{} does not exist, so there was nothing to delete",
                lanes_root.display()
            ));
            false
        }
        Err(e) => {
            failures.push(e);
            false
        }
    };
    drop(clones);

    fn status_lines(status: &Result<String, String>) -> Option<Vec<&str>> {
        status.as_ref().ok().map(|s| s.lines().collect())
    }
    let lane_records: Vec<Value> = lanes
        .iter()
        .map(|lane| {
            let (exit_code, killed_at_deadline, ended_s, end_seen_by) = match lane.end {
                Some(LaneEnd::Exited { at, code, seen_by }) => {
                    (code, false, Some(at.as_secs_f64()), Some(seen_by))
                }
                Some(LaneEnd::Killed { at }) => (
                    None,
                    true,
                    Some(at.as_secs_f64()),
                    Some("the kill at the deadline"),
                ),
                None => (None, false, None, None),
            };
            serde_json::json!({
                "lane": lane.name,
                "package": lane.package,
                "clone": lane.clone.display().to_string(),
                "cloned": lane.cloned,
                "commit": lane.commit,
                "target_directory": lane.target_directory.as_ref().map(|dir| dir.display().to_string()),
                "built": matches!(lane.end, Some(LaneEnd::Exited { code: Some(0), .. })),
                "exit_code": exit_code,
                "killed_at_deadline": killed_at_deadline,
                "started_s": lane.started.map(|at| at.as_secs_f64()),
                "ended_s": ended_s,
                "end_seen_by": end_seen_by,
                "changed_outside_target": lane.changed_outside_target,
                "fingerprint_packages": lane.fingerprint_packages,
                "allowed_workspace_packages": lane.allowed,
                "foreign_workspace_packages": lane.foreign_packages,
                "log": lane.started.map(|_| lane.log.clone()),
            })
        })
        .collect();
    write(
        &receipt.join("lanes.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "measured": MEASURED,
            "not_measured": NOT_MEASURED,
            "cargo": cargo_shown,
            "deadline_s": LANE_DEADLINE.as_secs_f64(),
            "outer_environment": outer_environment,
            "removed_from_lane_environment": TARGET_DIR_VARIABLES,
            "source_checkout": {
                "path": source.display().to_string(),
                "branch": branch.as_ref().ok(),
                "head_before": head.as_ref().ok(),
                "status_before": status_lines(&status_before),
                "clean_before": source_clean,
                "head_after": head_after.as_ref().ok(),
                "status_after": status_lines(&status_after),
                "unchanged": source_unchanged,
            },
            "builds_run": builds_run,
            "overlap": overlap_record,
            "cargo_home": cargo_home_record,
            "lanes": lane_records,
            "cleanup": {
                "lanes_root": lanes_root.display().to_string(),
                "existed": lanes_root_existed,
                "deleted": lanes_root_deleted,
            },
            "failures": failures,
        }))
        .expect("lane records serialise"),
    );

    finish(CASE, &receipt, &evidence, &failures);
}

/// One parsed `§` reference: a top-level section and, optionally, a subsection.
type SectionRef = (u32, Option<u32>);

fn parse_section_ref(text: &str) -> Option<(SectionRef, &str)> {
    let digits = text.len() - text.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let section = text[..digits].parse().ok()?;
    let rest = &text[digits..];
    if let Some(after_dot) = rest.strip_prefix('.') {
        let sub_digits = after_dot.len()
            - after_dot
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .len();
        if sub_digits > 0 {
            let subsection = after_dot[..sub_digits].parse().ok()?;
            return Some(((section, Some(subsection)), &after_dot[sub_digits..]));
        }
    }
    Some(((section, None), rest))
}

/// The plan sections a bead description cites, in the forms `§§1, 10–13 of <plan>` and `plan §8`.
fn plan_citations(description: &str) -> Vec<SectionRef> {
    let mut cited = Vec::new();
    let mut rest = description;
    while let Some(position) = rest.find('§') {
        let before = &rest[..position];
        let mut tail = rest[position..].trim_start_matches('§');
        let mut refs = Vec::new();
        while let Some((first, after)) = parse_section_ref(tail) {
            tail = after;
            let range_end = tail
                .strip_prefix('–')
                .or_else(|| tail.strip_prefix('-'))
                .and_then(parse_section_ref);
            match range_end {
                Some((last, after)) => {
                    refs.extend((first.0..=last.0).map(|section| (section, None)));
                    tail = after;
                }
                None => refs.push(first),
            }
            match tail.strip_prefix(", ") {
                Some(next) if next.starts_with(|c: char| c.is_ascii_digit()) => tail = next,
                _ => break,
            }
        }
        if tail.starts_with(&format!(" of {PLAN}")) || before.ends_with("plan ") {
            cited.extend(refs);
        }
        rest = tail;
    }
    cited
}

/// The numbered `## n.` and `### n.m` headings of the plan.
fn plan_headings(plan: &str) -> BTreeSet<SectionRef> {
    plan.lines()
        .filter_map(|line| {
            if let Some(heading) = line.strip_prefix("### ") {
                parse_section_ref(heading)
                    .filter(|(r, _)| r.1.is_some())
                    .map(|(r, _)| r)
            } else if let Some(heading) = line.strip_prefix("## ") {
                parse_section_ref(heading)
                    .filter(|(r, rest)| r.1.is_none() && rest.starts_with('.'))
                    .map(|(r, _)| r)
            } else {
                None
            }
        })
        .collect()
}

fn jsonl_ids(path: &Path) -> BTreeSet<String> {
    read_text(path)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let record: Value = serde_json::from_str(line).unwrap_or_else(|e| {
                panic!("{} holds a line that is not JSON ({e})", path.display())
            });
            record["id"]
                .as_str()
                .unwrap_or_else(|| panic!("{} holds a record without an id", path.display()))
                .to_owned()
        })
        .collect()
}

/// AGENTS.md names the claim, acceptance, commit and close commands, the master-only loop and the
/// one-cargo-per-target-directory rule; docs/plan holds the plan text and bead exports the
/// tracker's bead descriptions cite.
#[test]
fn infra_m0_1__agent_contract() {
    const CASE: &str = "infra_m0_1__agent_contract";
    let _serial = serial();
    registered_entry(CASE);
    let receipt = receipt_dir(CASE);
    let root = repo_root();
    let mut evidence = vec!["cargo program: none; this case runs no cargo command".to_owned()];
    let mut failures = Vec::new();

    let agents = read_text(&root.join("AGENTS.md"));
    for (rule, text) in AGENTS_REQUIREMENTS {
        if agents.contains(text) {
            evidence.push(format!("AGENTS.md names {rule}: {text}"));
        } else {
            failures.push(format!("AGENTS.md does not name {rule}: missing {text:?}"));
        }
    }
    for (index, _) in agents.match_indices("`docs/plan/") {
        let span = &agents[index + 1..];
        let cited = &span[..span.find('`').unwrap_or(span.len())];
        if root.join(cited).exists() {
            evidence.push(format!("AGENTS.md cites {cited}, which exists"));
        } else {
            failures.push(format!("AGENTS.md cites {cited}, which does not exist"));
        }
    }

    let plan_dir = root.join("docs").join("plan");
    for name in PLAN_BUNDLE {
        let path = plan_dir.join(name);
        match fs::metadata(&path) {
            Ok(meta) if meta.len() > 0 => {
                evidence.push(format!("docs/plan/{name}: {} bytes", meta.len()))
            }
            _ => failures.push(format!("docs/plan/{name} is missing or empty")),
        }
    }
    read_json(&plan_dir.join("coverage.json"));
    let plan_ids = jsonl_ids(&plan_dir.join("beads.plan.jsonl"));
    let br_ids = jsonl_ids(&plan_dir.join("beads.br.jsonl"));
    evidence.push(format!(
        "beads.plan.jsonl: {} records; beads.br.jsonl: {} records",
        plan_ids.len(),
        br_ids.len()
    ));
    let headings = plan_headings(&read_text(&plan_dir.join(PLAN)));
    let detailed: BTreeSet<String> = read_text(&plan_dir.join("BEAD_DETAILS.md"))
        .lines()
        .filter_map(|line| line.strip_prefix("## "))
        .filter_map(|line| line.split(" — ").next())
        .map(str::to_owned)
        .collect();

    let tracker = read_text(&root.join(".beads").join("issues.jsonl"));
    let (mut issues, mut citing, mut citations, mut imported) = (0, 0, 0, 0);
    let mut sections_cited = BTreeSet::new();
    for line in tracker.lines().filter(|line| !line.trim().is_empty()) {
        let issue: Value = serde_json::from_str(line).expect("tracker line is JSON");
        let id = issue["id"].as_str().expect("tracker issue has an id");
        issues += 1;
        let Some(description) = issue["description"].as_str() else {
            failures.push(format!("tracker issue {id} has no string description"));
            continue;
        };
        let cited = plan_citations(description);
        if description.contains(PLAN) {
            citing += 1;
        }
        for section in cited {
            citations += 1;
            sections_cited.insert(section);
            if !headings.contains(&section) {
                failures.push(format!(
                    "{id} cites plan section {section:?}, which {PLAN} does not have"
                ));
            }
        }
        for name in PLAN_BUNDLE
            .iter()
            .filter(|name| description.contains(*name))
        {
            if !plan_dir.join(name).is_file() {
                failures.push(format!("{id} cites {name}, which docs/plan does not hold"));
            }
        }
        if !id.starts_with("dc-") {
            imported += 1;
            for (export, ids) in [("beads.plan.jsonl", &plan_ids), ("beads.br.jsonl", &br_ids)] {
                if !ids.contains(id) {
                    failures.push(format!("imported bead {id} is not in docs/plan/{export}"));
                }
            }
            if !detailed.contains(id) {
                failures.push(format!(
                    "imported bead {id} has no section in docs/plan/BEAD_DETAILS.md"
                ));
            }
        }
    }
    if citing == 0 || citations == 0 {
        failures.push(format!(
            "no tracker description cites {PLAN}, so nothing was checked"
        ));
    }
    evidence.push(format!(
        "tracker: {issues} issues, {imported} imported; {citing} descriptions cite {PLAN} with {citations} section references"
    ));
    evidence.push(format!(
        "plan sections cited: {sections_cited:?}; numbered headings in {PLAN}: {}",
        headings.len()
    ));

    finish(CASE, &receipt, &evidence, &failures);
}
