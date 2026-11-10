//! The bodies of the infra-m0.3 acceptance cases (the `#[test]` functions sit at the root of
//! `tests/acceptance.rs`, so libtest lists them under their registered names).
//!
//! Both cases drive the xtask binary this test build compiled (`CARGO_BIN_EXE_xtask`), the same
//! binary `cargo xtask` runs, as `accept --id <bead> --platform <platform>`, against a copy of
//! this checkout's working tree: every file git would commit, without `target/`, in its own git
//! repository with one commit, under `CARGO_TARGET_TMPDIR`. The copy adds one dedicated fixture
//! package, `tools/xtask-fixture` (from `tests/fixtures/accept/`), to its workspace members; no
//! real package's files are replaced, and the real workspace is never edited. Every cargo the copy
//! runs uses the copy's own `target/`.
//!
//! What is compared: the runner's exit code and printed lines, its receipt (both the one under
//! `target/accept/` and its copy under `target/accept-receipts/`, which must be identical), and, independently of the
//! runner, the marker files the fixture cases create when they really run. Evidence goes to
//! `target/accept/infra-m0.3/<case>/` with a `summary.txt`.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const BEAD: &str = "infra-m0.3";

/// The fixture package the copy adds, and where.
const FIXTURE_PACKAGE: &str = "xtask-fixture";
const FIXTURE_DIR: &str = "tools/xtask-fixture";

/// The triples every receipt's dependency checks must cover.
const SHIPPED_TARGETS: [&str; 3] = [
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
    "wasm32-unknown-unknown",
];
const WASM_TARGET: &str = "wasm32-unknown-unknown";

/// The platform this host runs, and the one it cannot.
fn host_platforms() -> (&'static str, &'static str) {
    match std::env::consts::OS {
        "windows" => ("windows", "linux"),
        "linux" => ("linux", "windows"),
        other => panic!("the infra-m0.3 cases run on Windows and Linux hosts, not {other}"),
    }
}

fn repo_root() -> PathBuf {
    super::repo_root()
}

/// Checks that `case` is registered under infra-m0.3 in the xtask manifest with its platforms.
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
}

/// The evidence one case keeps, and the failures it found.
pub(super) struct Evidence {
    case: &'static str,
    pub(super) dir: PathBuf,
    pub(super) lines: Vec<String>,
    failures: Vec<String>,
}

impl Evidence {
    fn new(case: &'static str) -> Evidence {
        Evidence::for_bead(BEAD, case)
    }

    /// A cleared evidence directory `target/accept/<bead>/<case>/`.
    pub(super) fn for_bead(bead: &str, case: &'static str) -> Evidence {
        let dir = repo_root()
            .join("target")
            .join("accept")
            .join(bead)
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

    pub(super) fn expect(&mut self, holds: bool, what: String) {
        if holds {
            self.lines.push(format!("ok: {what}"));
        } else {
            self.failures.push(what);
        }
    }

    pub(super) fn finish(self) {
        super::finish(self.case, &self.dir, &self.lines, &self.failures);
    }
}

const FIXTURE_IDENTITY: [&str; 8] = [
    "-c",
    "user.name=xtask fixture",
    "-c",
    "user.email=xtask-fixture@invalid",
    "-c",
    "commit.gpgsign=false",
    "-c",
    "core.autocrlf=false",
];

fn git_in(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(FIXTURE_IDENTITY)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("cannot run git {args:?}: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} in {} exited with {:?}: {}",
        dir.display(),
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// `text` with forward slashes, lower case on Windows, for comparing paths as text.
fn normal_path(text: &str) -> String {
    let text = text.replace('\\', "/");
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}

/// A TOML manifest with `line` added to its `[dependencies]` table: inserted right below the
/// table's header when the table exists (wherever it is and whatever it holds), or in a new table
/// at the end when it does not. A header is recognised with surrounding spaces and a trailing
/// comment.
fn add_dependency(manifest: &str, line: &str) -> String {
    let mut lines: Vec<String> = manifest.lines().map(str::to_owned).collect();
    let header = lines.iter().position(|text| {
        let code = text.split('#').next().unwrap_or_default();
        code.trim() == "[dependencies]"
    });
    match header {
        Some(at) => {
            lines.insert(at + 1, line.to_owned());
            lines.join("\n") + "\n"
        }
        None => format!("{}\n\n[dependencies]\n{line}\n", manifest.trim_end()),
    }
}

/// The workspace manifest with `member` added first to the `members` array of `[workspace]`.
fn add_workspace_member(manifest: &str, member: &str) -> String {
    let table = manifest
        .find("[workspace]")
        .expect("the workspace manifest has a [workspace] table");
    let key = manifest[table..]
        .find("members")
        .map(|at| table + at)
        .expect("[workspace] has a members key");
    let open = manifest[key..]
        .find('[')
        .map(|at| key + at)
        .expect("members is an array");
    format!(
        "{}\"{member}\", {}",
        &manifest[..=open],
        &manifest[open + 1..]
    )
}

/// What `cargo metadata` reports about the copy: package names, members with their manifests and
/// portability, and the resolved normal and build edges between packages.
struct Graph {
    members: BTreeMap<String, (PathBuf, String, Vec<PathBuf>)>,
    edges: BTreeMap<String, Vec<String>>,
}

impl Graph {
    fn parse(stdout: &str) -> Graph {
        let metadata: Value = serde_json::from_str(stdout).expect("cargo metadata prints JSON");
        let mut names = BTreeMap::new();
        let mut members = BTreeMap::new();
        let ids: BTreeSet<&str> = metadata["workspace_members"]
            .as_array()
            .expect("workspace members")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        for package in metadata["packages"].as_array().expect("packages") {
            let id = package["id"].as_str().expect("package id").to_owned();
            let name = package["name"].as_str().expect("package name").to_owned();
            if ids.contains(id.as_str()) {
                let manifest = PathBuf::from(package["manifest_path"].as_str().expect("manifest"));
                let portability = package["metadata"]["dnacalc"]["portability"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let roots = package["targets"]
                    .as_array()
                    .expect("targets")
                    .iter()
                    .filter(|target| {
                        target["kind"]
                            .as_array()
                            .is_some_and(|kinds| kinds.iter().any(|k| k == "lib" || k == "bin"))
                    })
                    .filter_map(|target| target["src_path"].as_str().map(PathBuf::from))
                    .collect();
                members.insert(name.clone(), (manifest, portability, roots));
            }
            names.insert(id, name);
        }
        let mut edges = BTreeMap::new();
        for node in metadata["resolve"]["nodes"].as_array().expect("resolve") {
            let from = names[node["id"].as_str().expect("node id")].clone();
            let to: Vec<String> = node["deps"]
                .as_array()
                .expect("deps")
                .iter()
                .filter(|dep| {
                    dep["dep_kinds"].as_array().is_some_and(|kinds| {
                        kinds
                            .iter()
                            .any(|kind| kind["kind"].is_null() || kind["kind"] == "build")
                    })
                })
                .map(|dep| names[dep["pkg"].as_str().expect("dep pkg")].clone())
                .collect();
            edges.entry(from).or_insert_with(Vec::new).extend(to);
        }
        Graph { members, edges }
    }

    /// The packages `name` reaches through normal and build edges, itself excluded.
    fn closure(&self, name: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut pending = vec![name.to_owned()];
        while let Some(next) = pending.pop() {
            for to in self.edges.get(&next).into_iter().flatten() {
                if seen.insert(to.clone()) {
                    pending.push(to.clone());
                }
            }
        }
        seen.remove(name);
        seen
    }

    fn member_dir(&self, name: &str) -> PathBuf {
        let (manifest, _, _) = self
            .members
            .get(name)
            .unwrap_or_else(|| panic!("{name} is not a workspace member"));
        manifest
            .parent()
            .expect("a manifest has a parent")
            .to_path_buf()
    }
}

/// A fixture package a workspace copy adds to its members: the package name, its directory in
/// the copy, the directory under `tests/fixtures/` its files come from, and each file as (name
/// there, path in the package).
pub(super) struct Fixture {
    pub(super) package: &'static str,
    pub(super) dir: &'static str,
    pub(super) source: &'static str,
    pub(super) files: &'static [(&'static str, &'static str)],
}

/// The fixture package of the infra-m0.3 cases.
static ACCEPT_FIXTURE: Fixture = Fixture {
    package: FIXTURE_PACKAGE,
    dir: FIXTURE_DIR,
    source: "accept",
    files: &[
        ("package.toml", "Cargo.toml"),
        ("lib.rs", "src/lib.rs"),
        ("acceptance.rs", "tests/acceptance.rs"),
        ("zero_run.rs", "tests/zero_run.rs"),
        ("acceptance.json", "acceptance.json"),
    ],
};

/// A copy of this checkout's working tree with a fixture package and its own git repository.
pub(super) struct WorkspaceCopy {
    pub(super) base: PathBuf,
    root: PathBuf,
    markers: PathBuf,
    graph: Graph,
    fixture: &'static Fixture,
    /// Variables every cargo and xtask run in the copy sets (`Some`) or removes (`None`).
    environment: Vec<(&'static str, Option<String>)>,
}

/// Writes the contents of `from` to `to` as a new file (a plain copy can keep the old
/// modification time).
fn write_afresh(from: &Path, to: &Path) {
    let bytes = fs::read(from).unwrap_or_else(|e| panic!("cannot read {}: {e}", from.display()));
    fs::create_dir_all(to.parent().expect("a file has a parent"))
        .unwrap_or_else(|e| panic!("cannot create the parent of {}: {e}", to.display()));
    fs::write(to, bytes).unwrap_or_else(|e| panic!("cannot write {}: {e}", to.display()));
}

/// Runs `command`, keeping its transcript as `<label>.log`; returns exit code, stdout and stderr.
fn run_kept(
    mut command: Command,
    label: &str,
    evidence: &mut Evidence,
) -> (Option<i32>, String, String) {
    let shown = format!("{command:?}");
    let output = command
        .output()
        .unwrap_or_else(|e| panic!("cannot start {shown}: {e}"));
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    super::write(
        &evidence.dir.join(format!("{label}.log")),
        format!(
            "{shown}\nexit code: {:?}\n--- stdout\n{stdout}\n--- stderr\n{stderr}",
            output.status.code(),
        ),
    );
    println!("{label}: exit code {:?}", output.status.code());
    (output.status.code(), stdout, stderr)
}

impl WorkspaceCopy {
    /// A copy with the infra-m0.3 fixture package.
    fn new(name: &str, evidence: &mut Evidence) -> WorkspaceCopy {
        WorkspaceCopy::with_fixture(name, &ACCEPT_FIXTURE, evidence)
    }

    /// Copies the working tree into `CARGO_TARGET_TMPDIR/<name>/ws`, keeping an earlier copy's
    /// `target/` so its builds are reused, adds the `fixture` package to the workspace members,
    /// updates the copy's Cargo.lock for it and commits it all. Files are written afresh, so they
    /// carry new modification times as in a fresh checkout and cargo never takes an earlier run's
    /// build of a since-restored file for a current one.
    pub(super) fn with_fixture(
        name: &str,
        fixture: &'static Fixture,
        evidence: &mut Evidence,
    ) -> WorkspaceCopy {
        WorkspaceCopy::with_fixtures(name, fixture, &[], evidence)
    }

    /// [`WorkspaceCopy::with_fixture`], adding each of `companions` to the workspace members
    /// beside `fixture` as well.
    pub(super) fn with_fixtures(
        name: &str,
        fixture: &'static Fixture,
        companions: &[&'static Fixture],
        evidence: &mut Evidence,
    ) -> WorkspaceCopy {
        let added: Vec<&'static Fixture> = std::iter::once(fixture)
            .chain(companions.iter().copied())
            .collect();
        let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        let root = base.join("ws");
        if root.is_dir() {
            for entry in fs::read_dir(&root).expect("list the old copy") {
                let path = entry.expect("an entry of the old copy").path();
                if path.file_name().is_some_and(|name| name == "target") {
                    continue;
                }
                if path.is_dir() {
                    super::remove_tree(&path).unwrap_or_else(|e| panic!("{e}"));
                } else {
                    fs::remove_file(&path)
                        .unwrap_or_else(|e| panic!("cannot delete {}: {e}", path.display()));
                }
            }
        }
        let source = repo_root();
        let listing = git_in(
            &source,
            &[
                "ls-files",
                "--cached",
                "--others",
                "--exclude-standard",
                "-z",
            ],
        );
        let mut copied = 0;
        for rel in listing.split('\0').filter(|rel| !rel.is_empty()) {
            let from = source.join(rel);
            if !from.is_file() {
                continue;
            }
            write_afresh(&from, &root.join(rel));
            copied += 1;
        }
        let workspace_manifest = root.join("Cargo.toml");
        for added in &added {
            let sources = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("fixtures")
                .join(added.source);
            let package = root.join(added.dir);
            for (from, to) in added.files {
                write_afresh(&sources.join(from), &package.join(to));
            }
            let text = fs::read_to_string(&workspace_manifest).expect("read the copy's Cargo.toml");
            super::write(&workspace_manifest, add_workspace_member(&text, added.dir));
        }
        let mut copy = WorkspaceCopy {
            base,
            root,
            markers: PathBuf::new(),
            graph: Graph {
                members: BTreeMap::new(),
                edges: BTreeMap::new(),
            },
            fixture,
            environment: Vec::new(),
        };
        let (code, stdout, stderr) = copy.cargo(
            &["metadata", "--format-version", "1", "--all-features"],
            "copy-lock",
            evidence,
        );
        assert_eq!(
            code,
            Some(0),
            "cargo metadata in the copy with the fixture package failed: {stderr}"
        );
        copy.graph = Graph::parse(&stdout);
        for added in &added {
            assert!(
                copy.graph.members.contains_key(added.package),
                "the copy's workspace has no {}",
                added.package
            );
        }
        git_in(&copy.root, &["init", "--quiet"]);
        git_in(&copy.root, &["add", "--all"]);
        git_in(
            &copy.root,
            &[
                "commit",
                "--quiet",
                "-m",
                "fixture: copy of the DnaCalc working tree",
            ],
        );
        copy.markers = copy.base.join("markers");
        copy.clear_markers();
        let packages: Vec<String> = added
            .iter()
            .map(|added| {
                format!(
                    "{} at {} (from tools/xtask/tests/fixtures/{})",
                    added.package, added.dir, added.source
                )
            })
            .collect();
        evidence.lines.push(format!(
            "copy: {copied} files of the working tree at {}, plus the fixture packages {} added to its workspace members and Cargo.lock, committed in its own repository",
            copy.root.display(),
            packages.join(", ")
        ));
        copy
    }

    /// Sets (`Some`) or removes (`None`) `name` for every later cargo and xtask run in the copy.
    pub(super) fn set_environment(&mut self, name: &'static str, setting: Option<String>) {
        self.environment.retain(|(known, _)| *known != name);
        self.environment.push((name, setting));
    }

    fn apply_environment(&self, command: &mut Command, extra: &[(&str, Option<&str>)]) {
        let copy_wide = self
            .environment
            .iter()
            .map(|(name, setting)| (*name, setting.as_deref()));
        for (name, setting) in copy_wide.chain(extra.iter().copied()) {
            match setting {
                Some(value) => command.env(name, value),
                None => command.env_remove(name),
            };
        }
    }

    fn clear_markers(&self) {
        super::remove_tree(&self.markers).unwrap_or_else(|e| panic!("{e}"));
        fs::create_dir_all(&self.markers).expect("create the marker directory");
    }

    fn marker(&self, case: &str) -> bool {
        self.markers.join(case).is_file()
    }

    pub(super) fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// `path` relative to the copy's root, with forward slashes.
    fn rel(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    /// Runs cargo in the copy, with no target directory from the environment.
    fn cargo(
        &self,
        args: &[&str],
        label: &str,
        evidence: &mut Evidence,
    ) -> (Option<i32>, String, String) {
        self.cargo_with(args, &[], label, evidence)
    }

    /// Runs cargo in the copy, with no target directory from the environment and `extra` set
    /// (`Some`) or removed (`None`) after the copy's own environment.
    pub(super) fn cargo_with(
        &self,
        args: &[&str],
        extra: &[(&str, Option<&str>)],
        label: &str,
        evidence: &mut Evidence,
    ) -> (Option<i32>, String, String) {
        let mut command = Command::new(super::cargo_program().0);
        command
            .args(args)
            .current_dir(&self.root)
            .env("XTASK_FIXTURE_MARKERS", &self.markers)
            .env_remove("CARGO_TARGET_DIR")
            .env_remove("CARGO_BUILD_TARGET_DIR");
        self.apply_environment(&mut command, extra);
        run_kept(command, label, evidence)
    }

    /// Runs `xtask accept <args>` in the copy. For a bead the copy's manifests register
    /// (`known`), the copy of the receipt must be identical to the one under target/;
    /// for an unknown bead there must be no copy.
    pub(super) fn accept(
        &self,
        args: &[&str],
        label: &str,
        known: bool,
        evidence: &mut Evidence,
    ) -> Accepted {
        let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
        command
            .arg("accept")
            .args(args)
            .current_dir(&self.root)
            .env("XTASK_FIXTURE_MARKERS", &self.markers)
            .env_remove("CARGO_TARGET_DIR")
            .env_remove("CARGO_BUILD_TARGET_DIR");
        self.apply_environment(&mut command, &[]);
        let (exit_code, stdout, _) = run_kept(command, label, evidence);
        let id = args[args.iter().position(|a| *a == "--id").expect("--id") + 1];
        let platform = args[args
            .iter()
            .position(|a| *a == "--platform")
            .expect("--platform")
            + 1];
        let receipt_path = self
            .root
            .join("target")
            .join("accept")
            .join(id)
            .join(platform)
            .join("receipt.json");
        let committed_rel = format!("target/accept-receipts/{id}/{platform}.json");
        let committed_path = self.root.join(&committed_rel);
        let receipt = fs::read_to_string(&receipt_path).ok();
        let committed = fs::read_to_string(&committed_path).ok();
        if known {
            evidence.expect(
                receipt.is_some() && receipt == committed,
                format!(
                    "{label}: the receipt {} exists and its copy {committed_rel} is identical",
                    receipt_path.display()
                ),
            );
        } else {
            evidence.expect(
                receipt.is_some() && committed.is_none(),
                format!(
                    "{label}: the receipt {} exists and no copy {committed_rel} was written for an unknown bead",
                    receipt_path.display()
                ),
            );
        }
        if let Some(text) = &receipt {
            fs::write(evidence.dir.join(format!("{label}.receipt.json")), text)
                .expect("keep the receipt");
        }
        Accepted {
            package: self.fixture.package,
            exit_code,
            stdout,
            text: receipt.clone().unwrap_or_default(),
            receipt: receipt
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or(Value::Null),
            own: committed_rel,
        }
    }

    /// Replaces a file of the copy, returning its old contents.
    fn replace(&self, rel: &str, contents: &str) -> String {
        let path = self.path(rel);
        let old = fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {rel}: {e}"));
        fs::write(&path, contents).unwrap_or_else(|e| panic!("cannot write {rel}: {e}"));
        old
    }
}

/// What one `xtask accept` run showed.
pub(super) struct Accepted {
    /// The fixture package whose cases the run's printed lines name.
    package: &'static str,
    pub(super) exit_code: Option<i32>,
    pub(super) stdout: String,
    text: String,
    pub(super) receipt: Value,
    own: String,
}

impl Accepted {
    pub(super) fn run(&self, case: &str) -> &Value {
        self.receipt["cases"]
            .as_array()
            .and_then(|runs| runs.iter().find(|run| run["case"] == case))
            .unwrap_or(&Value::Null)
    }

    /// The outcome the receipt records for `case`.
    pub(super) fn outcome(&self, case: &str) -> Option<&str> {
        self.run(case)["outcome"].as_str()
    }

    pub(super) fn result(&self) -> &str {
        self.receipt["result"].as_str().unwrap_or("no result")
    }

    /// Whether the run printed the line for `case` (or `<case>@<browser>`) with this outcome.
    pub(super) fn printed(&self, outcome: &str, case: &str) -> bool {
        let prefix = format!(
            "ACCEPT {outcome} {case} (package {}, exit code ",
            self.package
        );
        self.stdout.lines().any(|line| line.starts_with(&prefix))
    }

    fn line_containing(&self, text: &str) -> bool {
        self.stdout.lines().any(|line| line.contains(text))
    }

    fn check(&self, name: &str) -> &Value {
        self.receipt["architecture"]
            .as_array()
            .and_then(|checks| checks.iter().find(|check| check["check"] == name))
            .unwrap_or(&Value::Null)
    }

    /// The findings the receipt records for a check, or `None` when it passed.
    fn findings(&self, name: &str) -> Option<Vec<String>> {
        let check = self.check(name);
        if check["passed"] == true {
            return None;
        }
        Some(match check["findings"].as_array() {
            Some(findings) => findings
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            None => vec![format!("the receipt has no {name} check")],
        })
    }

    pub(super) fn checks_ok_except(&self, failing: &[&str]) -> bool {
        CHECKS
            .iter()
            .filter(|check| !failing.contains(check))
            .all(|check| self.findings(check).is_none())
    }

    /// The receipt's dirty paths.
    fn dirty_paths(&self) -> Vec<(String, Option<String>)> {
        self.receipt["host"]["git"]["dirty_paths"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|entry| {
                (
                    entry["path"].as_str().unwrap_or_default().to_owned(),
                    entry["blob"].as_str().map(str::to_owned),
                )
            })
            .collect()
    }
}

const CHECKS: [&str; 3] = ["dependencies", "os-imports", "wasm-refusal"];

/// A misspelled required test name fails the runner, and so does every other way of not
/// executing a registered case: a partial name, an ignored case, a failing case, a case that
/// reports it could not run (on standard output or standard error), an unregistered prefixed test,
/// a bead with no case for the platform, an unknown bead, a host that cannot run the platform, a
/// registered command that fails, and a case its target lists whose `--exact` run reports 0
/// passed. A correctly named case passes, and its marker file shows it ran. No run that executed
/// nothing records `passed`.
pub fn zero_tests_fail() {
    const CASE: &str = "infra_m0_3__zero_tests_fail";
    let _serial = super::serial();
    registered(CASE);
    let mut evidence = Evidence::new(CASE);
    let (host, other) = host_platforms();
    let copy = WorkspaceCopy::new("m0_3_zero", &mut evidence);
    evidence
        .lines
        .push(format!("host platform {host}; other platform {other}"));

    // Control: the correctly named case passes and really ran.
    let run = copy.accept(
        &["--id", "fixture-pass", "--platform", host, "--require-all"],
        "01-pass",
        true,
        &mut evidence,
    );
    evidence.expect(
        run.exit_code == Some(0) && run.result() == "passed",
        format!(
            "01-pass: exit code {:?} is 0 and the receipt result {} is passed",
            run.exit_code,
            run.result()
        ),
    );
    evidence.expect(
        run.printed("passed", "fixture_pass__alpha"),
        "01-pass: printed `ACCEPT passed fixture_pass__alpha`".to_owned(),
    );
    evidence.expect(
        run.outcome("fixture_pass__alpha") == Some("passed")
            && run.run("fixture_pass__alpha")["result_line"]
                .as_str()
                .is_some_and(|line| line.contains(" 1 passed; 0 failed; 0 ignored;")),
        format!(
            "01-pass: receipt outcome {:?} is passed with libtest's 1-passed line",
            run.outcome("fixture_pass__alpha")
        ),
    );
    evidence.expect(
        copy.marker("fixture_pass__alpha"),
        "01-pass: the fixture case created its marker, so it ran".to_owned(),
    );
    evidence.expect(
        run.checks_ok_except(&[]),
        "01-pass: all five architecture checks passed".to_owned(),
    );
    // Paths: relative to the copy, and nothing outside it named.
    let receipt_text = normal_path(&run.text.replace("\\\\", "/"));
    let copy_root = normal_path(&copy.root.to_string_lossy());
    let home = std::env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(|home| normal_path(&home))
        .unwrap_or_default();
    evidence.expect(
        !receipt_text.contains(&copy_root) && (home.len() < 4 || !receipt_text.contains(&home)),
        format!("01-pass: the receipt names neither the copy's root {copy_root} nor the home directory {home}"),
    );
    evidence.expect(
        run.run("fixture_pass__alpha")["argv"]
            .as_array()
            .is_some_and(|argv| !argv.iter().any(|arg| arg == "--target-dir")),
        "01-pass: the case argv uses the default workspace target directory".to_owned(),
    );

    // The hazard: cargo itself exits 0 for a misspelled name and runs 0 tests.
    copy.clear_markers();
    let (code, stdout, _) = copy.cargo(
        &[
            "test",
            "-p",
            FIXTURE_PACKAGE,
            "--test",
            "acceptance",
            "--locked",
            "--",
            "--exact",
            "fixture_misspelled__alpah",
            "--nocapture",
        ],
        "02-plain-cargo-misspelled",
        &mut evidence,
    );
    evidence.expect(
        code == Some(0) && stdout.contains("running 0 tests") && stdout.contains("0 passed"),
        format!("02-plain-cargo-misspelled: plain cargo exits {code:?} and runs 0 tests for the misspelled name"),
    );

    // The named behaviour: the runner fails the misspelled name and runs nothing for it.
    let run = copy.accept(
        &[
            "--id",
            "fixture-misspelled",
            "--platform",
            host,
            "--require-all",
        ],
        "03-misspelled",
        true,
        &mut evidence,
    );
    evidence.expect(
        run.exit_code == Some(1) && run.result() == "failed",
        format!(
            "03-misspelled: exit code {:?} is 1 and the result {} is failed",
            run.exit_code,
            run.result()
        ),
    );
    evidence.expect(
        run.printed("missing", "fixture_misspelled__alpah"),
        "03-misspelled: printed `ACCEPT missing fixture_misspelled__alpah`".to_owned(),
    );
    evidence.expect(
        run.outcome("fixture_misspelled__alpah") == Some("missing"),
        "03-misspelled: receipt outcome missing".to_owned(),
    );
    evidence.expect(
        !copy.marker("fixture_misspelled__alpha"),
        "03-misspelled: the correctly spelled test did not run in its place".to_owned(),
    );
    evidence.expect(run.line_containing("fixture_misspelled__alpha with the fixture-misspelled prefix that its acceptance.json does not register"), "03-misspelled: the unregistered real name is reported".to_owned());

    let run = copy.accept(
        &[
            "--id",
            "fixture-partial",
            "--platform",
            host,
            "--require-all",
        ],
        "04-partial",
        true,
        &mut evidence,
    );
    evidence.expect(
        run.exit_code == Some(1)
            && run.printed("missing", "fixture_partial__alpha")
            && run.result() == "failed",
        format!(
            "04-partial: a name that is only a prefix of a test is missing (exit code {:?}, result {})",
            run.exit_code,
            run.result()
        ),
    );
    evidence.expect(
        !copy.marker("fixture_partial__alpha_case"),
        "04-partial: the longer-named test did not run".to_owned(),
    );

    let run = copy.accept(
        &[
            "--id",
            "fixture-ignored",
            "--platform",
            host,
            "--require-all",
        ],
        "05-ignored",
        true,
        &mut evidence,
    );
    evidence.expect(
        run.exit_code == Some(1)
            && run.printed("ignored", "fixture_ignored__alpha")
            && run.result() == "failed",
        format!(
            "05-ignored: an ignored case fails as ignored (exit code {:?}, result {})",
            run.exit_code,
            run.result()
        ),
    );
    evidence.expect(
        !copy.marker("fixture_ignored__alpha"),
        "05-ignored: the ignored case did not run".to_owned(),
    );

    let run = copy.accept(
        &[
            "--id",
            "fixture-failing",
            "--platform",
            host,
            "--require-all",
        ],
        "06-failing",
        true,
        &mut evidence,
    );
    evidence.expect(
        run.exit_code == Some(1)
            && run.printed("failed", "fixture_failing__alpha")
            && run.result() == "failed",
        format!(
            "06-failing: a failing case fails (exit code {:?}, result {})",
            run.exit_code,
            run.result()
        ),
    );
    evidence.expect(
        copy.marker("fixture_failing__alpha"),
        "06-failing: the failing case ran".to_owned(),
    );

    let run = copy.accept(
        &[
            "--id",
            "fixture-notrun",
            "--platform",
            host,
            "--require-all",
        ],
        "07-notrun-require-all",
        true,
        &mut evidence,
    );
    evidence.expect(
        run.exit_code == Some(1)
            && run.printed("not_run", "fixture_notrun__alpha")
            && run.result() == "not_run",
        format!(
            "07-notrun-require-all: a case that prints ACCEPT-NOT-RUN on standard output is not_run, the result is {} (not passed) and the run exits {:?} under --require-all",
            run.result(),
            run.exit_code
        ),
    );
    evidence.expect(
        copy.marker("fixture_notrun__alpha"),
        "07-notrun-require-all: the case ran and then reported it could not prove anything"
            .to_owned(),
    );
    let run = copy.accept(
        &["--id", "fixture-notrun", "--platform", host],
        "08-notrun-without-require-all",
        true,
        &mut evidence,
    );
    evidence.expect(
        run.exit_code == Some(0)
            && run.outcome("fixture_notrun__alpha") == Some("not_run")
            && run.result() == "not_run",
        format!(
            "08-notrun-without-require-all: without --require-all the run exits {:?}, records the case not_run and the result {} (never passed)",
            run.exit_code,
            run.result()
        ),
    );

    copy.clear_markers();
    let run = copy.accept(
        &[
            "--id",
            "fixture-notrun-stderr",
            "--platform",
            host,
            "--require-all",
        ],
        "09-notrun-stderr",
        true,
        &mut evidence,
    );
    evidence.expect(
        run.exit_code == Some(1)
            && run.outcome("fixture_notrun_stderr__alpha") == Some("not_run")
            && run.result() == "not_run"
            && run.run("fixture_notrun_stderr__alpha")["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("browser driver")),
        format!(
            "09-notrun-stderr: a case that prints ACCEPT-NOT-RUN on standard error is not_run with its reason, the result is {} and the run exits {:?} under --require-all",
            run.result(),
            run.exit_code
        ),
    );
    evidence.expect(
        copy.marker("fixture_notrun_stderr__alpha"),
        "09-notrun-stderr: the case ran before it reported it could not prove anything".to_owned(),
    );

    let run = copy.accept(
        &[
            "--id",
            "fixture-unregistered",
            "--platform",
            host,
            "--require-all",
        ],
        "10-unregistered",
        true,
        &mut evidence,
    );
    evidence.expect(
        run.exit_code == Some(1)
            && run.printed("passed", "fixture_unregistered__beta")
            && run.result() == "failed",
        format!(
            "10-unregistered: the registered case passes but the run fails (exit code {:?}, result {})",
            run.exit_code,
            run.result()
        ),
    );
    evidence.expect(run.line_containing("fixture_unregistered__alpha with the fixture-unregistered prefix that its acceptance.json does not register"), "10-unregistered: the unregistered prefixed test is reported".to_owned());

    let run = copy.accept(
        &["--id", "fixture-excel", "--platform", host, "--require-all"],
        "11-no-case-for-platform",
        true,
        &mut evidence,
    );
    evidence.expect(
        run.exit_code == Some(1)
            && run.result() == "failed"
            && run.line_containing(&format!(
                "no case of fixture-excel declares platform {host}"
            )),
        format!(
            "11-no-case-for-platform: a bead without a {host} case fails (exit code {:?}, result {})",
            run.exit_code,
            run.result()
        ),
    );

    let run = copy.accept(
        &[
            "--id",
            "fixture-absent",
            "--platform",
            host,
            "--require-all",
        ],
        "12-unknown-id",
        false,
        &mut evidence,
    );
    evidence.expect(
        run.exit_code == Some(1)
            && run.result() == "failed"
            && run.receipt["copy"].is_null()
            && run.line_containing("no acceptance manifest registers fixture-absent"),
        format!(
            "12-unknown-id: an unknown bead fails (exit code {:?}, result {}) and writes no receipt copy",
            run.exit_code,
            run.result()
        ),
    );

    copy.clear_markers();
    let run = copy.accept(
        &["--id", "fixture-pass", "--platform", other, "--require-all"],
        "13-other-host-require-all",
        true,
        &mut evidence,
    );
    let reason = run.run("fixture_pass__alpha")["reason"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    evidence.expect(
        run.exit_code == Some(1)
            && run.printed("not_run", "fixture_pass__alpha")
            && run.result() == "not_run"
            && run.receipt["host"]["platform_matches_host"] == false
            && reason.contains(&format!("platform {other} needs a")),
        format!(
            "13-other-host-require-all: platform {other} on a {host} host is not_run ({reason}), the result is {} and the run exits {:?}",
            run.result(),
            run.exit_code
        ),
    );
    let run = copy.accept(
        &["--id", "fixture-pass", "--platform", other],
        "14-other-host",
        true,
        &mut evidence,
    );
    evidence.expect(
        run.exit_code == Some(0)
            && run.outcome("fixture_pass__alpha") == Some("not_run")
            && run.result() == "not_run"
            && run.receipt["host"]["os"] == std::env::consts::OS,
        format!(
            "14-other-host: without --require-all, platform {other} on a {host} host still records the case not_run and the result {} (never passed); exit code {:?}",
            run.result(),
            run.exit_code
        ),
    );
    evidence.expect(
        !copy.marker("fixture_pass__alpha"),
        "13/14-other-host: nothing ran".to_owned(),
    );

    // Registered commands: run once each, the runner's own command skipped and listed, and a
    // failing command fails the run although the case passes.
    let run = copy.accept(
        &[
            "--id",
            "fixture-commands",
            "--platform",
            host,
            "--require-all",
        ],
        "15-commands",
        true,
        &mut evidence,
    );
    let commands = run.receipt["commands"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let shown: Vec<String> = commands
        .iter()
        .map(|c| format!("{} -> {} ({})", c["argv"], c["outcome"], c["exit_code"]))
        .collect();
    evidence.expect(
        commands.len() == 3
            && commands[0]["outcome"] == "passed"
            && commands[0]["exit_code"] == 0
            && commands[0]["log"].is_string()
            && commands[1]["outcome"] == "skipped"
            && commands[1]["exit_code"].is_null()
            && commands[2]["outcome"] == "failed"
            && commands[2]["exit_code"].as_i64().is_some_and(|code| code != 0),
        format!("15-commands: the receipt records the passing command, the skipped runner command and the failing command with argv, exit code and log: {shown:?}"),
    );
    evidence.expect(
        run.exit_code == Some(1)
            && run.result() == "failed"
            && run.outcome("fixture_commands__alpha") == Some("passed")
            && run.line_containing("ACCEPT command skipped (exit code none): cargo xtask accept"),
        format!(
            "15-commands: the failing command fails the run (exit code {:?}, result {}) while the case passed",
            run.exit_code,
            run.result()
        ),
    );

    // A case its target lists, whose --exact run executes nothing and reports 0 passed. Unlike
    // the misspelled and partial names, the listing holds the exact name, so the runner runs the
    // case and must judge libtest's result line; 0 passed is never passed. The fixture target
    // (zero_run, harness = false) stands for a harness that lists a case and then filters it out.
    const ZERO_RUN: &str = "fixture_zero_run__alpha";
    copy.clear_markers();
    let (code, stdout, _) = copy.cargo(
        &[
            "test",
            "-p",
            FIXTURE_PACKAGE,
            "--test",
            "zero_run",
            "--locked",
            "--",
            "--list",
        ],
        "16-plain-cargo-list-zero-run",
        &mut evidence,
    );
    let listed = format!("{ZERO_RUN}: test");
    evidence.expect(
        code == Some(0) && stdout.lines().any(|line| line == listed),
        format!("16-plain-cargo-list-zero-run: plain cargo exits {code:?} and its listing holds the exact name {ZERO_RUN}"),
    );
    evidence.expect(
        !copy.marker(ZERO_RUN),
        "16-plain-cargo-list-zero-run: listing the target ran nothing".to_owned(),
    );
    let run = copy.accept(
        &[
            "--id",
            "fixture-zero-run",
            "--platform",
            host,
            "--require-all",
        ],
        "16-listed-zero-run",
        true,
        &mut evidence,
    );
    let outcome = run.outcome(ZERO_RUN).unwrap_or("no outcome").to_owned();
    let result_line = run.run(ZERO_RUN)["result_line"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    evidence.expect(
        run.exit_code == Some(1)
            && run.result() == "failed"
            && (outcome == "missing" || outcome == "failed")
            && run.printed(&outcome, ZERO_RUN),
        format!(
            "16-listed-zero-run: a listed case whose run reports 0 passed is not passed (receipt outcome {outcome}, reason {}), the result is {} and the run exits {:?} under --require-all",
            run.run(ZERO_RUN)["reason"],
            run.result(),
            run.exit_code
        ),
    );
    evidence.expect(
        result_line.contains(" 0 passed; 0 failed; 0 ignored;"),
        format!(
            "16-listed-zero-run: the receipt keeps the run's own result line {result_line:?}, so the runner judged a run rather than a name absent from the listing"
        ),
    );
    let filter = fs::read_to_string(copy.markers.join(ZERO_RUN)).unwrap_or_default();
    evidence.expect(
        filter
            .split_whitespace()
            .collect::<Vec<_>>()
            .windows(2)
            .any(|pair| pair == ["--exact", ZERO_RUN]),
        format!(
            "16-listed-zero-run: the fixture target's marker shows the runner ran it with --exact {ZERO_RUN} (arguments {filter:?})"
        ),
    );

    evidence
        .lines
        .push(format!("copy kept at {}", copy.base.display()));
    evidence.finish();
}

/// The lines of `text` that contain each marker, 1-based, in marker order.
fn lines_of(text: &str, markers: &[&str]) -> Vec<usize> {
    markers
        .iter()
        .map(|marker| {
            text.lines()
                .position(|line| line.contains(marker))
                .unwrap_or_else(|| panic!("no line contains {marker}"))
                + 1
        })
        .collect()
}

/// The OS imports added to calc-value for the os-imports scenario: each one the check must report,
/// and a test-only use of std::fs it must not.
const OS_IMPORTS: &str = r#"
use std::{io, net};

pub fn os_read(path: &str) -> io::Result<Vec<u8>> {
    std::fs::read(path)
}

pub fn os_address() -> Option<net::Ipv4Addr> {
    None
}

pub fn os_process() -> String {
    format!("{}", std::process::id())
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetTickCount() -> u32;
}

#[cfg(test)]
mod tests {
    #[test]
    fn uses_the_file_system() {
        assert!(std::fs::metadata(".").is_ok());
    }
}
"#;

/// One dependency-check scenario: the edges it adds (from a workspace member to a package by name
/// and directory), and the finding each resolved target must report.
struct DependencyScenario {
    label: String,
    edges: Vec<(String, String, PathBuf)>,
    expected: String,
    /// Text no finding may contain (for the transitive scenario: a direct edge).
    absent: Option<String>,
}

/// The layering scenarios the case plants whatever `tools/xtask/architecture.json` holds: the
/// package, the forbidden package added to it, and the start of the line its rule must report on
/// every shipped target. Each of the four packages calc-doc-model's rule forbids has its own
/// scenario, so a rule deleted from the file, or a forbidden list without one of them, fails the
/// case instead of taking the scenario with it. The scenarios generated from the file's rules skip
/// nothing: a later rule for one of these packages (a direct-scope one reports another line) keeps
/// its own generated scenario, which also checks the rule's whole reason.
const FIXED_RULE_SCENARIOS: [(&str, &str, &str); 4] = [
    (
        "calc-doc-model",
        "calc-engine",
        "calc-doc-model has in its dependency closure calc-engine (plan 1.3 and 2.1: ",
    ),
    (
        "calc-doc-model",
        "calc-fml",
        "calc-doc-model has in its dependency closure calc-fml (plan 1.3 and 2.1: ",
    ),
    (
        "calc-doc-model",
        "calc-host",
        "calc-doc-model has in its dependency closure calc-host (plan 1.3 and 2.1: ",
    ),
    (
        "calc-doc-model",
        "calc-doc",
        "calc-doc-model has in its dependency closure calc-doc (plan 1.3 and 2.1: ",
    ),
];

/// The dependency line that makes a package depend on the package in `dir`.
fn path_dependency(name: &str, dir: &Path) -> String {
    format!(
        "{name} = {{ path = \"{}\" }}",
        dir.to_string_lossy().replace('\\', "/")
    )
}

/// Writes a path crate outside the copy, named like a crate of the deny list or a plain shim.
fn stand_in_crate(dir: &Path, name: &str, dependency: Option<&str>) {
    fs::create_dir_all(dir.join("src")).expect("create a stand-in crate");
    let mut manifest = format!(
        "[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[lib]\npath = \"src/lib.rs\"\n"
    );
    if let Some(line) = dependency {
        manifest = add_dependency(&manifest, line);
    }
    super::write(&dir.join("Cargo.toml"), manifest);
    super::write(
        &dir.join("src").join("lib.rs"),
        format!("//! A stand-in crate named {name}, outside the workspace.\n"),
    );
}

/// Adding calc-engine to dna-onecalc, calc-host to calc-fml, each of calc-engine, calc-fml and
/// calc-host to calc-doc-model and a stand-in crate named calc-doc to it (planted whatever
/// `tools/xtask/architecture.json` holds, so the case fails when that file has no such rule or its
/// forbidden list lacks one of them; calc-doc's stand-in, needed while crates/calc-doc depends on
/// calc-doc-model, shows that the rule matches by name), a forbidden package to the package of each layering rule of that file, calc-engine
/// behind an intermediate package of dna-onecalc, or a native-only package to a portable one fails
/// the dependencies check on every shipped target; an OS import, an include macro or an OS-binding
/// crate in a portable package fails the os-imports check; each native-only package without its
/// wasm32 refusal fails the wasm-refusal check. Each mutation fails only its own check, the
/// fixture case still runs and passes, and the run exits 1; the unmodified copy passes every check.
pub fn forbidden_dependency() {
    const CASE: &str = "infra_m0_3__forbidden_dependency";
    let _serial = super::serial();
    registered(CASE);
    let mut evidence = Evidence::new(CASE);
    let (host, _) = host_platforms();
    let copy = WorkspaceCopy::new("m0_3_deps", &mut evidence);
    let accept_args = ["--id", "fixture-pass", "--platform", host, "--require-all"];
    let lock = copy.path("Cargo.lock");
    let original_lock = fs::read_to_string(&lock).expect("read the copy's Cargo.lock");

    let run = copy.accept(&accept_args, "00-unmodified", true, &mut evidence);
    evidence.expect(
        run.exit_code == Some(0) && run.checks_ok_except(&[]) && run.result() == "passed",
        format!(
            "00-unmodified: every check passes on the unmodified copy and the run exits {:?} with result {}",
            run.exit_code,
            run.result()
        ),
    );
    evidence.expect(
        copy.marker("fixture_pass__alpha"),
        "00-unmodified: the fixture case ran".to_owned(),
    );
    let targets: Vec<String> = run.check("dependencies")["details"]["targets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    evidence.expect(
        SHIPPED_TARGETS
            .iter()
            .all(|target| targets.iter().any(|t| t == target)),
        format!("00-unmodified: the dependencies check resolved every shipped triple: {targets:?}"),
    );

    // The manifest edit is TOML-aware: a second dependency goes into the existing table, and
    // cargo accepts the result.
    let fixture_manifest = format!("{FIXTURE_DIR}/Cargo.toml");
    let old = fs::read_to_string(copy.path(&fixture_manifest)).expect("read the fixture manifest");
    let once = add_dependency(
        &old,
        &path_dependency("calc-value", &copy.graph.member_dir("calc-value")),
    );
    let twice = add_dependency(
        &once,
        &path_dependency("calc-func", &copy.graph.member_dir("calc-func")),
    );
    copy.replace(&fixture_manifest, &twice);
    let (code, stdout, _) = copy.cargo(
        &["metadata", "--format-version", "1"],
        "00-toml-edit",
        &mut evidence,
    );
    let edited = (code == Some(0)).then(|| Graph::parse(&stdout));
    evidence.expect(
        twice.matches("[dependencies]").count() == 1
            && edited.as_ref().is_some_and(|graph| {
                let to = graph.edges.get(FIXTURE_PACKAGE).cloned().unwrap_or_default();
                to.contains(&"calc-value".to_owned()) && to.contains(&"calc-func".to_owned())
            }),
        format!("00-toml-edit: two dependencies added one after the other share one [dependencies] table and cargo metadata (exit code {code:?}) resolves both"),
    );
    copy.replace(&fixture_manifest, &old);
    fs::write(&lock, &original_lock).expect("restore Cargo.lock");

    // One scenario per layering rule, plus the transitive and portable/native-only ones.
    let rules = super::read_json(&Path::new(env!("CARGO_MANIFEST_DIR")).join("architecture.json"));
    // The pairs the bead names come first for their rules.
    let named: BTreeMap<&str, &str> =
        BTreeMap::from([("dna-onecalc", "calc-engine"), ("calc-fml", "calc-host")]);
    let mut scenarios = Vec::new();
    let mut generated_stand_in = false;
    for rule in rules["dependency_rules"]
        .as_array()
        .expect("dependency_rules")
    {
        let package = rule["package"].as_str().expect("rule package").to_owned();
        let mut forbidden = super::strings(&rule["forbidden"], "forbidden packages");
        if let Some(first) = named.get(package.as_str()) {
            forbidden.sort_by_key(|name| name.as_str() != *first);
        }
        let relation = if rule["scope"] == "direct" {
            "depends directly on"
        } else {
            "has in its dependency closure"
        };
        let expected = |added: &str| {
            format!(
                "{package} {relation} {added} ({}",
                rule["reason"].as_str().expect("rule reason")
            )
        };
        // A forbidden package that already reaches this one would close a cycle, which cargo
        // refuses before any check runs; the rule then holds by construction, so the scenario
        // adds a stand-in path crate of the forbidden package's name (version 0.0.0, outside the
        // copy) instead, which the name-based rule must reject the same way.
        let acyclic = forbidden
            .iter()
            .find(|candidate| !copy.graph.closure(candidate).contains(&package));
        scenarios.push(match acyclic {
            Some(added) => DependencyScenario {
                label: format!("rule-{package}-{added}"),
                edges: vec![(
                    package.clone(),
                    added.clone(),
                    copy.graph.member_dir(added),
                )],
                expected: expected(added),
                absent: None,
            },
            None => {
                generated_stand_in = true;
                let added = &forbidden[0];
                let dir = copy.base.join(format!("stand-in-{added}"));
                stand_in_crate(&dir, added, None);
                evidence.lines.push(format!(
                    "rule {package}: every forbidden package {forbidden:?} already depends on {package}; the scenario adds a stand-in crate named {added}"
                ));
                DependencyScenario {
                    label: format!("rule-{package}-stand-in-{added}"),
                    edges: vec![(package.clone(), added.clone(), dir)],
                    expected: expected(added),
                    absent: None,
                }
            }
        });
    }
    // The fixed scenarios: the pair and the expected line come from the case, not from the file.
    let fixed_packages: BTreeSet<&str> = FIXED_RULE_SCENARIOS
        .iter()
        .map(|(package, _, _)| *package)
        .collect();
    for package in fixed_packages {
        let in_file: Vec<String> = rules["dependency_rules"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|rule| rule["package"] == package)
            .map(Value::to_string)
            .collect();
        evidence.lines.push(format!(
            "fixed scenarios for {package}: planted whatever architecture.json holds; its rules for {package}: {in_file:?}"
        ));
    }
    for (package, added, expected) in FIXED_RULE_SCENARIOS {
        // Every fixed pair names members of the copy. As above, an added package that already
        // reaches this one gets a stand-in crate.
        assert!(
            copy.graph.members.contains_key(added),
            "{CASE}: the fixed scenario {package} -> {added} names {added}, which is not a member of the copy"
        );
        let (label, dir) = if copy.graph.closure(added).contains(package) {
            let dir = copy.base.join(format!("fixed-stand-in-{added}"));
            stand_in_crate(&dir, added, None);
            let why = format!(
                "{added} already has {package} in its dependency closure, so adding the member would close a cycle that cargo refuses before any check runs"
            );
            evidence.lines.push(format!(
                "fixed scenario {package} -> {added}: {why}; the scenario adds a stand-in crate named {added} (version 0.0.0, outside the copy, at {}) instead, which the name-based rule must reject the same way",
                dir.display()
            ));
            (format!("fixed-{package}-stand-in-{added}"), dir)
        } else {
            evidence.lines.push(format!(
                "fixed scenario {package} -> {added}: the scenario adds the member {added}, whose dependency closure does not contain {package}"
            ));
            (
                format!("fixed-{package}-{added}"),
                copy.graph.member_dir(added),
            )
        };
        scenarios.push(DependencyScenario {
            label,
            edges: vec![(package.to_owned(), added.to_owned(), dir)],
            expected: expected.to_owned(),
            absent: None,
        });
    }
    // The stand-in construction of the generated scenarios, exercised now so it is proven before
    // a rule of the file needs it. It runs unless a generated scenario already used a stand-in;
    // the fixed scenarios' stand-ins follow the case's own pairs, not the file's rules, so they
    // never decide whether it runs.
    if !generated_stand_in {
        let dir = copy.base.join("stand-in-calc-fml");
        stand_in_crate(&dir, "calc-fml", None);
        scenarios.push(DependencyScenario {
            label: "rule-calc-func-stand-in-calc-fml".to_owned(),
            edges: vec![("calc-func".to_owned(), "calc-fml".to_owned(), dir)],
            expected: "calc-func has in its dependency closure calc-fml (plan 1.3".to_owned(),
            absent: None,
        });
    }
    scenarios.push(DependencyScenario {
        label: "transitive-dna-onecalc".to_owned(),
        edges: vec![
            (
                "dna-onecalc".to_owned(),
                FIXTURE_PACKAGE.to_owned(),
                copy.graph.member_dir(FIXTURE_PACKAGE),
            ),
            (
                FIXTURE_PACKAGE.to_owned(),
                "calc-engine".to_owned(),
                copy.graph.member_dir("calc-engine"),
            ),
        ],
        expected: "dna-onecalc has in its dependency closure calc-engine (plan 1.3".to_owned(),
        absent: Some("dna-onecalc depends directly".to_owned()),
    });
    scenarios.push(DependencyScenario {
        label: "native-only-dna-calc".to_owned(),
        edges: vec![(
            "dna-calc".to_owned(),
            "calc-oracle-excel".to_owned(),
            copy.graph.member_dir("calc-oracle-excel"),
        )],
        expected: "portable dna-calc has native-only calc-oracle-excel in its dependency closure"
            .to_owned(),
        absent: None,
    });
    let mut edited_manifests = Vec::new();
    for scenario in &scenarios {
        let label = &scenario.label;
        let mut restore = Vec::new();
        for (from, to, dir) in &scenario.edges {
            let manifest = copy.rel(&copy.graph.member_dir(from).join("Cargo.toml"));
            let old = fs::read_to_string(copy.path(&manifest)).expect("read the manifest");
            copy.replace(&manifest, &add_dependency(&old, &path_dependency(to, dir)));
            restore.push((manifest, old));
        }
        let (code, _, stderr) = copy.cargo(
            &["metadata", "--format-version", "1"],
            &format!("{label}-lock"),
            &mut evidence,
        );
        let relocked = fs::read_to_string(&lock).is_ok_and(|text| text != original_lock);
        let added: Vec<String> = scenario
            .edges
            .iter()
            .map(|(from, to, _)| format!("{from} -> {to}"))
            .collect();
        evidence.expect(
            code == Some(0) && relocked,
            format!(
                "{label}: cargo metadata exits {code:?} and rewrites the copy's Cargo.lock with {added:?} ({relocked}){}",
                if code == Some(0) { String::new() } else { format!(": {stderr}") }
            ),
        );
        copy.clear_markers();
        let run = copy.accept(&accept_args, label, true, &mut evidence);
        let findings = run.findings("dependencies").unwrap_or_default();
        evidence.expect(
            run.exit_code == Some(1) && run.result() == "failed",
            format!(
                "{label}: exit code {:?} is 1 and the result {} is failed",
                run.exit_code,
                run.result()
            ),
        );
        let per_target: Vec<(String, usize)> = SHIPPED_TARGETS
            .iter()
            .map(|target| {
                let prefix = format!("[{target}] ");
                let n = findings
                    .iter()
                    .filter(|f| f.starts_with(&prefix) && f.contains(&scenario.expected))
                    .count();
                ((*target).to_owned(), n)
            })
            .collect();
        evidence.expect(
            per_target.iter().all(|(_, n)| *n == 1)
                && scenario
                    .absent
                    .as_ref()
                    .is_none_or(|absent| !findings.iter().any(|f| f.contains(absent))),
            format!("{label}: the dependencies check reports `{}` once on each shipped target {per_target:?}; all findings: {findings:?}", scenario.expected),
        );
        evidence.expect(
            run.checks_ok_except(&["dependencies"]),
            format!("{label}: every other check passed"),
        );
        evidence.expect(
            run.printed("passed", "fixture_pass__alpha") && copy.marker("fixture_pass__alpha"),
            format!(
                "{label}: the fixture case still ran and passed, so the check alone failed the run"
            ),
        );
        let dirty = run.dirty_paths();
        evidence.expect(
            restore.iter().all(|(manifest, _)| {
                dirty.iter().any(|(path, blob)| {
                    path == manifest && blob.as_ref().is_some_and(|b| b.len() == 40)
                })
            }) && !dirty.iter().any(|(path, _)| *path == run.own),
            format!(
                "{label}: the receipt lists each edited manifest as dirty with its blob, and not its own path {}",
                run.own
            ),
        );
        for (manifest, old) in &restore {
            copy.replace(manifest, old);
            edited_manifests.push(manifest.clone());
        }
        fs::write(&lock, &original_lock).expect("restore Cargo.lock");
    }
    evidence.lines.push(format!(
        "dependency scenarios: {:?}",
        scenarios.iter().map(|s| &s.label).collect::<Vec<_>>()
    ));

    // An OS import in a portable package's non-test source.
    let source = "crates/calc-value/src/lib.rs";
    let old = fs::read_to_string(copy.path(source)).expect("read calc-value");
    let mutated = format!("{old}{OS_IMPORTS}");
    copy.replace(source, &mutated);
    let lines = lines_of(
        &mutated,
        &[
            "use std::{io, net};",
            "std::fs::read(path)",
            "std::process::id()",
            "#[link(name",
            "unsafe extern \"system\"",
        ],
    );
    let expected: Vec<String> = [
        format!("{source}:{}: std::{{net}}", lines[0]),
        format!("{source}:{}: std::fs", lines[1]),
        format!("{source}:{}: std::process", lines[2]),
        format!("{source}:{}: #[link] attribute", lines[3]),
        format!("{source}:{}: extern \"system\"", lines[4]),
    ]
    .to_vec();
    copy.clear_markers();
    let run = copy.accept(&accept_args, "os-import", true, &mut evidence);
    let findings = run.findings("os-imports").unwrap_or_default();
    evidence.expect(
        run.exit_code == Some(1),
        format!("os-import: exit code {:?} is 1", run.exit_code),
    );
    evidence.expect(findings == expected, format!("os-import: the os-imports check reports exactly {expected:?} (the test-only std::fs is not reported); got {findings:?}"));
    evidence.expect(
        run.checks_ok_except(&["os-imports"]),
        "os-import: every other check passed".to_owned(),
    );
    evidence.expect(
        run.printed("passed", "fixture_pass__alpha") && copy.marker("fixture_pass__alpha"),
        "os-import: the fixture case still ran and passed".to_owned(),
    );
    copy.replace(source, &old);

    // An include macro, which the source scan does not follow, fails os-imports.
    let mutated = format!("{old}\npub const SOURCE: &str = include_str!(\"lib.rs\");\n");
    copy.replace(source, &mutated);
    let line = lines_of(&mutated, &["include_str!"])[0];
    copy.clear_markers();
    let run = copy.accept(&accept_args, "include-macro", true, &mut evidence);
    let os = run.findings("os-imports").unwrap_or_default();
    evidence.expect(
        run.exit_code == Some(1)
            && os == [format!("{source}:{line}: include_str! reads a file that os-imports does not follow, so that file is unchecked")]
            && run.checks_ok_except(&["os-imports"]),
        format!("include-macro: an include_str! fails os-imports {os:?} and nothing else (exit code {:?})", run.exit_code),
    );
    copy.replace(source, &old);

    // OS-binding crates: stand-in path crates outside the copy, named like the deny list, attached
    // to dna-calc. No member depends on DNA Calc's application core (checked below; no
    // architecture rule forbids one), so the check's lines name dna-calc alone, whichever portable
    // packages depend on each other.
    const HOLDER: &str = "dna-calc";
    let dependents: Vec<&String> = copy
        .graph
        .edges
        .iter()
        .filter(|(_, to)| to.iter().any(|name| name == HOLDER))
        .map(|(from, _)| from)
        .collect();
    evidence.expect(
        dependents.is_empty(),
        format!("os-binding: no package of the copy depends on {HOLDER}: {dependents:?}"),
    );
    let winapi = copy.base.join("winapi");
    stand_in_crate(&winapi, "winapi", None);
    let shim = copy.base.join("shim");
    stand_in_crate(&shim, "shim", Some(&path_dependency("winapi", &winapi)));
    let manifest = copy.rel(&copy.graph.member_dir(HOLDER).join("Cargo.toml"));
    let manifest = manifest.as_str();
    let old_manifest = fs::read_to_string(copy.path(manifest)).expect("read the holder's manifest");
    let binding: [(&str, &Path, Vec<String>); 2] = [
        (
            "os-binding-direct",
            &winapi,
            SHIPPED_TARGETS
                .iter()
                .map(|t| {
                    format!(
                        "[{t}] portable {HOLDER} depends directly on the OS-binding crate winapi"
                    )
                })
                .collect(),
        ),
        (
            "os-binding-transitive",
            &shim,
            vec![format!(
                "[{WASM_TARGET}] portable {HOLDER} has the OS-binding crate winapi in its {WASM_TARGET} dependency closure"
            )],
        ),
    ];
    for (label, dir, expected) in binding {
        let name = dir
            .file_name()
            .expect("a crate directory name")
            .to_string_lossy()
            .into_owned();
        copy.replace(
            manifest,
            &add_dependency(&old_manifest, &path_dependency(&name, dir)),
        );
        let (code, stdout, _) = copy.cargo(
            &["metadata", "--format-version", "1", "--all-features"],
            &format!("{label}-lock"),
            &mut evidence,
        );
        let relocked = fs::read_to_string(&lock).is_ok_and(|text| text != original_lock);
        evidence.expect(
            code == Some(0) && relocked,
            format!(
                "{label}: cargo metadata exits {code:?} and rewrites the copy's Cargo.lock with the {name} path crate ({relocked})"
            ),
        );
        // Which portable packages reach the stand-in, from the copy's resolved metadata.
        let resolved = (code == Some(0)).then(|| Graph::parse(&stdout));
        let reaching: Vec<String> = resolved
            .as_ref()
            .map(|graph| {
                graph
                    .members
                    .iter()
                    .filter(|(member, (_, portability, _))| {
                        portability == "portable" && graph.closure(member).contains("winapi")
                    })
                    .map(|(member, _)| member.clone())
                    .collect()
            })
            .unwrap_or_default();
        evidence.expect(
            reaching == [HOLDER],
            format!("{label}: the copy's resolved metadata has winapi in the closure of the portable packages {reaching:?}, exactly [{HOLDER:?}]"),
        );
        copy.clear_markers();
        let run = copy.accept(&accept_args, label, true, &mut evidence);
        let findings = run.findings("os-imports").unwrap_or_default();
        evidence.expect(
            run.exit_code == Some(1) && findings == expected,
            format!("{label}: exit code {:?}; the os-imports check reports exactly {expected:?} (a transitive OS-binding crate counts only in the wasm32 closure); got {findings:?}", run.exit_code),
        );
        // Each printed line names its package after `] portable `; that package must be the
        // holder, and its resolved closure must really contain the stand-in.
        let named: Vec<Option<&str>> = findings
            .iter()
            .map(|finding| {
                finding
                    .split_once("] portable ")
                    .and_then(|(_, rest)| rest.split(' ').next())
            })
            .collect();
        evidence.expect(
            !findings.is_empty()
                && named.iter().all(|package| {
                    package.is_some_and(|package| {
                        package == HOLDER && reaching.iter().any(|r| r == package)
                    })
                }),
            format!("{label}: every os-imports line names {HOLDER}, a package whose resolved closure contains winapi; the lines name {named:?}"),
        );
        evidence.expect(
            run.checks_ok_except(&["os-imports"]),
            format!("{label}: every other check passed"),
        );
        copy.replace(manifest, &old_manifest);
        fs::write(&lock, &original_lock).expect("restore Cargo.lock");
    }

    // Each native-only package without its wasm32 refusal.
    let native: Vec<(String, Vec<PathBuf>)> = copy
        .graph
        .members
        .iter()
        .filter(|(_, (_, portability, _))| portability == "native-only")
        .map(|(name, (_, _, roots))| (name.clone(), roots.clone()))
        .collect();
    evidence.expect(
        native.len() == 3,
        format!(
            "the copy has three native-only packages: {:?}",
            native.iter().map(|(name, _)| name).collect::<Vec<_>>()
        ),
    );
    for (name, roots) in &native {
        let label = format!("no-wasm-refusal-{name}");
        let Some(root) = roots.iter().find(|root| {
            fs::read_to_string(root).is_ok_and(|text| text.contains("compile_error!("))
        }) else {
            evidence.failures.push(format!(
                "{label}: no crate root of {name} holds a compile_error!"
            ));
            continue;
        };
        let rel = copy.rel(root);
        let old = fs::read_to_string(root).expect("read the crate root");
        let disabled = old.replace("#[cfg(target_arch = \"wasm32\")]", "#[cfg(any())]");
        evidence.expect(
            disabled != old,
            format!("{label}: the refusal's cfg in {rel} was disabled"),
        );
        copy.replace(&rel, &disabled);
        copy.clear_markers();
        let run = copy.accept(&accept_args, &label, true, &mut evidence);
        let findings = run.findings("wasm-refusal").unwrap_or_default();
        evidence.expect(
            run.exit_code == Some(1)
                && findings.len() == 1
                && findings[0].starts_with(&format!(
                    "{name} compiled for wasm32-unknown-unknown; a native-only package must refuse it"
                )),
            format!("{label}: exit code {:?}; the wasm-refusal check reports {name} compiling for wasm32 and nothing else: {findings:?}", run.exit_code),
        );
        evidence.expect(
            run.checks_ok_except(&["wasm-refusal"]),
            format!("{label}: every other check passed"),
        );
        copy.replace(&rel, &old);
    }

    evidence.lines.push(format!(
        "manifests edited and restored: {edited_manifests:?}; copy kept at {}",
        copy.base.display()
    ));
    evidence.finish();
}
