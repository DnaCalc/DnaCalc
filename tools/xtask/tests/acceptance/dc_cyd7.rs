//! The body of the dc-cyd7 acceptance case (the `#[test]` function sits at the root of
//! `tests/acceptance.rs`, so libtest lists it under its registered name).
//!
//! The license is read from what cargo reports and from the LICENSE bytes, never from a flag. One
//! check, `check`, runs over a workspace directory: `cargo metadata --no-deps` for the workspace
//! and for its own workspace `tools/xtask/ci-browser-fixture` must report license "MIT" for every
//! package, the workspace's packages being exactly infra-m0.1's members; each member's manifest
//! must take it with `license.workspace = true` in its `[package]` table and set no license there
//! itself; and `LICENSE` must equal the expected text byte for byte once CRLF is read as LF.
//!
//! The expected text is the MIT text of the legacy repositories' LICENSE files with the holder
//! line "Copyright (c) 2026 DNA Kode, Inc.". For each of OxFunc, OxFml, OxCalc and DnaTreeCalc it
//! is the license blob that `migration/source-manifest.json` pins: this module's own
//! `git rev-parse <commit>:<path>` confirms that the blob is that repository's file at the pinned
//! commit, its own `git cat-file blob` reads the bytes in `<legacy root>/<repository>` (the legacy
//! root is the workspace root's parent or `DNACALC_LEGACY_ROOT`), and the one `Copyright (c) `
//! line is replaced by the holder line. Every readable one must give the same text; a host where
//! none is readable prints `ACCEPT-NOT-RUN` and never passes.
//!
//! The same check then runs on a copy of this checkout's working tree (every file git would commit,
//! under `CARGO_TARGET_TMPDIR`): unedited it must find nothing, and with one package's license
//! removed (dna-calc's `license.workspace = true`, then the browser fixture's `license`) or the
//! LICENSE holder line changed it must find exactly that. Last, `cargo xtask ci licenses` (the
//! xtask binary this test build compiled) must pass and record every member's own license as MIT.
//! Every cargo this case starts uses a target directory under `CARGO_TARGET_TMPDIR`. Evidence goes
//! to `target/accept/dc-cyd7/dc_cyd7__workspace_license/`.

use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BEAD: &str = "dc-cyd7";
const CASE: &str = "dc_cyd7__workspace_license";

/// The license expression every package must report.
const EXPRESSION: &str = "MIT";
const FIRST_LINE: &str = "MIT License";
const HOLDER_PREFIX: &str = "Copyright (c) ";
const HOLDER_LINE: &str = "Copyright (c) 2026 DNA Kode, Inc.";

/// The legacy repositories whose LICENSE is MIT (the owner's decision of 2026-10-18).
const LEGACY_REPOSITORIES: [&str; 4] = ["OxFunc", "OxFml", "OxCalc", "DnaTreeCalc"];
const LEGACY_ROOT_VARIABLE: &str = "DNACALC_LEGACY_ROOT";
const SOURCE_MANIFEST: &str = "migration/source-manifest.json";

/// The browser fixture's own workspace and its one package.
const FIXTURE_MANIFEST: &str = "tools/xtask/ci-browser-fixture/Cargo.toml";
const FIXTURE_PACKAGE: &str = "dnacalc-ci-browser-fixture";

/// The line each member's `[package]` table carries, and the fixture's own license line.
const INHERIT_LINE: &str = "license.workspace = true";
const FIXTURE_LICENSE_LINE: &str = "license = \"MIT\"";

/// The record `cargo xtask ci licenses` writes, relative to the workspace root.
const LICENSES_RECORD: &str = "target/ci/licenses/licenses.json";

fn repo_root() -> PathBuf {
    super::repo_root()
}

fn tmp_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(name)
}

/// Checks that `case` is registered under dc-cyd7 for windows and linux, and that the bead
/// registers exactly its two Acceptance commands and `cargo xtask ci licenses`.
fn registered(case: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("acceptance.json");
    let manifest = super::read_json(&path);
    let entry = manifest["beads"]
        .as_array()
        .and_then(|beads| beads.iter().find(|bead| bead["id"] == BEAD))
        .unwrap_or_else(|| panic!("{} has no entry for {BEAD}", path.display()));
    let cases = super::strings(&entry["cases"], "registered cases");
    assert_eq!(
        cases,
        [case],
        "{BEAD} must register exactly {case} in {}",
        path.display()
    );
    let platforms = super::strings(&entry["platforms"][case], "the case's platforms");
    assert_eq!(
        platforms,
        ["windows", "linux"],
        "{case} must declare windows and linux"
    );
    let commands: Vec<Vec<String>> = entry["commands"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|command| super::strings(command, "a registered command"))
        .collect();
    let argv = |text: &str| -> Vec<String> { text.split(' ').map(str::to_owned).collect() };
    assert_eq!(
        commands,
        vec![
            argv("cargo xtask accept --id dc-cyd7 --platform windows --require-all"),
            argv("cargo xtask accept --id dc-cyd7 --platform linux --require-all"),
            argv("cargo xtask ci licenses"),
        ],
        "{BEAD} must register exactly its two Acceptance commands and the licenses job"
    );
}

struct Evidence {
    dir: PathBuf,
    lines: Vec<String>,
    failures: Vec<String>,
}

impl Evidence {
    fn new() -> Evidence {
        let dir = repo_root()
            .join("target")
            .join("accept")
            .join(BEAD)
            .join(CASE);
        super::remove_tree(&dir).unwrap_or_else(|e| panic!("{e}"));
        fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("cannot create {}: {e}", dir.display()));
        Evidence {
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

    fn note(&mut self, text: String) {
        println!("ACCEPT-NOTE: {CASE}: {text}");
        self.lines.push(format!("note: {text}"));
    }

    fn not_run(mut self, reason: String) {
        println!("ACCEPT-NOT-RUN: {CASE}: {reason}");
        self.lines.push(format!("not run: {reason}"));
        self.finish();
    }

    fn finish(self) {
        super::finish(CASE, &self.dir, &self.lines, &self.failures);
    }
}

/// Runs `command`, keeping its transcript as `<label>.log` in the evidence directory.
fn run_kept(mut command: Command, label: &str, evidence: &mut Evidence) -> Output {
    let shown = format!("{command:?}");
    let output = command
        .output()
        .unwrap_or_else(|e| panic!("cannot start {shown}: {e}"));
    super::write(
        &evidence.dir.join(format!("{label}.log")),
        format!(
            "{shown}\nexit code: {:?}\n--- stdout\n{}\n--- stderr\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ),
    );
    println!("{label}: exit code {:?}", output.status.code());
    output
}

/// The legacy root, with how it was chosen. The path is shown only as `..` or by the variable's
/// name, never as an absolute host path.
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

/// A read-only git command in a legacy repository; its standard output, or why it failed.
fn legacy_git(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-c")
        .arg(format!(
            "safe.directory={}",
            dir.to_string_lossy().replace('\\', "/")
        ))
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("cannot start git {args:?}: {e}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(format!(
            "git {args:?} exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// The expected LICENSE text a legacy license blob gives: its text with CRLF read as LF and its
/// one holder line replaced by `HOLDER_LINE`, together with the holder line it had.
fn template(bytes: &[u8]) -> Result<(String, String), String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "is not UTF-8".to_owned())?
        .replace("\r\n", "\n");
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.first() != Some(&FIRST_LINE) {
        return Err(format!(
            "its first line is {:?}, not {FIRST_LINE:?}",
            lines.first()
        ));
    }
    let holders: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with(HOLDER_PREFIX))
        .map(|(i, _)| i)
        .collect();
    let [holder] = holders[..] else {
        return Err(format!(
            "it has {} lines starting {HOLDER_PREFIX:?}, not one",
            holders.len()
        ));
    };
    let original = lines[holder].to_owned();
    lines[holder] = HOLDER_LINE;
    Ok((lines.join("\n"), original))
}

/// One template read from a legacy repository.
struct Template {
    source: String,
    original_holder: String,
    text: String,
}

/// The templates of every readable legacy LICENSE the source manifest pins for
/// `LEGACY_REPOSITORIES`. A repository or blob this host cannot read is noted; a manifest without
/// the repository's MIT license file, a blob that is not the file at the pinned commit, or a file
/// that is not an MIT template fails the case.
fn legacy_templates(evidence: &mut Evidence) -> Vec<Template> {
    let manifest = super::read_json(&repo_root().join(SOURCE_MANIFEST));
    let (root, shown_root) = legacy_root();
    evidence.line(format!("legacy root: {shown_root}"));
    let mut templates = Vec::new();
    for name in LEGACY_REPOSITORIES {
        let Some(repository) = manifest["repositories"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|repository| repository["name"] == name)
        else {
            evidence.expect(
                false,
                format!("{SOURCE_MANIFEST} pins the repository {name}"),
            );
            continue;
        };
        let commit = repository["commit"].as_str().unwrap_or_default();
        let files: Vec<(&str, &str)> = repository["license"]["files"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|file| file["first_line"] == FIRST_LINE)
            .filter_map(|file| Some((file["path"].as_str()?, file["blob"].as_str()?)))
            .collect();
        evidence.expect(
            !commit.is_empty() && files.len() == 1,
            format!(
                "{SOURCE_MANIFEST} pins {name} at commit {commit:?} with one license file whose first line is {FIRST_LINE:?}: {files:?}"
            ),
        );
        let dir = root.join(name);
        for (path, blob) in files {
            let source = format!("{name}/{path} at {commit} (blob {blob})");
            if !dir.is_dir() {
                evidence.note(format!(
                    "{source} is not readable: the legacy root holds no {name}"
                ));
                continue;
            }
            let at_commit = match legacy_git(&dir, &["rev-parse", &format!("{commit}:{path}")]) {
                Ok(out) => String::from_utf8_lossy(&out).trim().to_owned(),
                Err(e) => {
                    evidence.note(format!("{source} is not readable: {e}"));
                    continue;
                }
            };
            evidence.expect(
                at_commit == blob,
                format!("{name}'s {path} at the pinned commit is the pinned blob {blob} (git rev-parse gives {at_commit})"),
            );
            if at_commit != blob {
                continue;
            }
            let bytes = match legacy_git(&dir, &["cat-file", "blob", blob]) {
                Ok(bytes) => bytes,
                Err(e) => {
                    evidence.note(format!("{source} is not readable: {e}"));
                    continue;
                }
            };
            match template(&bytes) {
                Ok((text, original_holder)) => {
                    evidence.line(format!(
                        "template: {source}, {} bytes, holder line {original_holder:?}",
                        bytes.len()
                    ));
                    templates.push(Template {
                        source,
                        original_holder,
                        text,
                    });
                }
                Err(e) => evidence.expect(false, format!("{source} is an MIT template: {e}")),
            }
        }
    }
    templates
}

/// One thing the check finds wrong: its kind (`members`, `metadata`, `inherit` or `license-text`),
/// what it is about, and the details.
struct Finding {
    kind: &'static str,
    subject: String,
    detail: String,
}

fn finding(kind: &'static str, subject: &str, detail: String) -> Finding {
    Finding {
        kind,
        subject: subject.to_owned(),
        detail,
    }
}

/// The `key = value` lines of a manifest's `[package]` table, with spaces removed.
fn package_table(text: &str) -> Vec<String> {
    let mut inside = false;
    let mut keys = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == "[package]";
        } else if inside && !line.is_empty() && !line.starts_with('#') {
            keys.push(line.replace(' ', ""));
        }
    }
    keys
}

/// Whether a member manifest takes its license from the workspace and sets none itself.
fn inherits(text: &str) -> bool {
    let keys = package_table(text);
    let inherit = INHERIT_LINE.replace(' ', "");
    let own = |key: &String| {
        ["license=", "license-file=", "license-file."]
            .iter()
            .any(|prefix| key.starts_with(prefix))
    };
    keys.contains(&inherit) && !keys.iter().any(own)
}

/// The first line where `found` differs from `expected`, both LF-separated.
fn first_difference(found: &str, expected: &str) -> String {
    let found_lines: Vec<&str> = found.split('\n').collect();
    let expected_lines: Vec<&str> = expected.split('\n').collect();
    for i in 0..found_lines.len().max(expected_lines.len()) {
        let (f, e) = (found_lines.get(i), expected_lines.get(i));
        if f != e {
            return format!("line {}: found {f:?}, expected {e:?}", i + 1);
        }
    }
    "the texts are equal".to_owned()
}

/// The check: what is wrong with the license of the workspace at `root`, judged from cargo
/// metadata, the member manifests and the LICENSE bytes against `expected`.
fn check(root: &Path, expected: &str, label: &str, evidence: &mut Evidence) -> Vec<Finding> {
    let mut findings = Vec::new();
    let packages = super::workspace_packages();
    let members: BTreeSet<&str> = packages.iter().map(|(name, _)| name.as_str()).collect();
    let fixture: BTreeSet<&str> = [FIXTURE_PACKAGE].into();
    for (workspace, manifest, wanted) in [
        ("workspace", "Cargo.toml", &members),
        ("ci-browser-fixture", FIXTURE_MANIFEST, &fixture),
    ] {
        let mut command = Command::new(super::cargo_program().0);
        command
            .args(["metadata", "--no-deps", "--format-version", "1"])
            .arg("--manifest-path")
            .arg(root.join(manifest))
            .current_dir(root)
            .env("CARGO_TARGET_DIR", tmp_dir("dc_cyd7-target"));
        let output = run_kept(command, &format!("{label}-metadata-{workspace}"), evidence);
        if !output.status.success() {
            findings.push(finding(
                "metadata",
                workspace,
                format!(
                    "cargo metadata exited with {:?}: {}",
                    output.status.code(),
                    super::tail(&output.stderr, 5)
                ),
            ));
            continue;
        }
        let metadata: Value = match serde_json::from_slice(&output.stdout) {
            Ok(metadata) => metadata,
            Err(e) => {
                findings.push(finding(
                    "metadata",
                    workspace,
                    format!("cargo metadata printed no JSON: {e}"),
                ));
                continue;
            }
        };
        let packages: Vec<&Value> = metadata["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .collect();
        let names: BTreeSet<&str> = packages
            .iter()
            .filter_map(|package| package["name"].as_str())
            .collect();
        if names != *wanted {
            findings.push(finding(
                "members",
                workspace,
                format!("cargo metadata reports the packages {names:?}, not {wanted:?}"),
            ));
        }
        for package in packages {
            let name = package["name"].as_str().unwrap_or_default();
            let license = package["license"].as_str();
            evidence.line(format!(
                "{label}: cargo metadata ({workspace}) reports {name} license {license:?}, license_file {}",
                package["license_file"]
            ));
            if license != Some(EXPRESSION) {
                findings.push(finding(
                    "metadata",
                    name,
                    format!("cargo metadata ({workspace}) reports license {license:?}, not {EXPRESSION:?}"),
                ));
            }
            if workspace != "workspace" {
                continue;
            }
            let manifest_path = package["manifest_path"].as_str().unwrap_or_default();
            match fs::read_to_string(manifest_path) {
                Ok(text) if inherits(&text) => {}
                Ok(_) => findings.push(finding(
                    "inherit",
                    name,
                    format!("its manifest's [package] table lacks `{INHERIT_LINE}` or sets a license of its own"),
                )),
                Err(e) => findings.push(finding(
                    "inherit",
                    name,
                    format!("cannot read its manifest: {e}"),
                )),
            }
        }
    }
    match fs::read(root.join("LICENSE")) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => {
                let text = text.replace("\r\n", "\n");
                if text != expected {
                    findings.push(finding(
                        "license-text",
                        "LICENSE",
                        first_difference(&text, expected),
                    ));
                }
            }
            Err(_) => findings.push(finding(
                "license-text",
                "LICENSE",
                "is not UTF-8".to_owned(),
            )),
        },
        Err(e) => findings.push(finding(
            "license-text",
            "LICENSE",
            format!("cannot read it: {e}"),
        )),
    }
    for found in &findings {
        evidence.line(format!(
            "{label}: finding {} {}: {}",
            found.kind, found.subject, found.detail
        ));
    }
    findings
}

/// Records whether the check's findings are exactly `wanted` (kind, subject) pairs.
fn expect_findings(
    findings: &[Finding],
    wanted: &[(&str, &str)],
    what: &str,
    evidence: &mut Evidence,
) {
    let found: BTreeSet<(&str, &str)> = findings
        .iter()
        .map(|found| (found.kind, found.subject.as_str()))
        .collect();
    let wanted: BTreeSet<(&str, &str)> = wanted.iter().copied().collect();
    evidence.expect(
        found == wanted && found.len() == findings.len(),
        format!("{what}: the check finds exactly {wanted:?}; found {found:?}"),
    );
}

/// A copy of every file git would commit from this checkout's working tree, under
/// `CARGO_TARGET_TMPDIR`; returns its root and the number of files.
fn working_tree_copy() -> (PathBuf, usize) {
    let source = repo_root();
    let base = tmp_dir("dc_cyd7-copy");
    super::remove_tree(&base).unwrap_or_else(|e| panic!("{e}"));
    let root = base.join("ws");
    let output = Command::new("git")
        .arg("-C")
        .arg(&source)
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .output()
        .unwrap_or_else(|e| panic!("cannot run git ls-files: {e}"));
    assert!(
        output.status.success(),
        "git ls-files exited with {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let listing = String::from_utf8_lossy(&output.stdout).into_owned();
    let mut copied = 0;
    for rel in listing.split('\0').filter(|rel| !rel.is_empty()) {
        let from = source.join(rel);
        if !from.is_file() {
            continue;
        }
        let to = root.join(rel);
        fs::create_dir_all(to.parent().expect("a file has a parent"))
            .unwrap_or_else(|e| panic!("cannot create the parent of {}: {e}", to.display()));
        fs::copy(&from, &to).unwrap_or_else(|e| panic!("cannot copy {rel}: {e}"));
        copied += 1;
    }
    (root, copied)
}

/// One edit of the working-tree copy: in `file`, the one line equal to `line` replaced by
/// `replacement` or removed, after which the check must find exactly `wanted` (kind, subject).
struct Variant {
    label: &'static str,
    file: &'static str,
    line: &'static str,
    replacement: Option<&'static str>,
    wanted: &'static [(&'static str, &'static str)],
}

/// `text` with its one line equal to `line` (apart from its line ending) replaced by
/// `replacement`, or removed when `replacement` is `None`.
fn edit_line(text: &str, line: &str, replacement: Option<&str>) -> Result<String, String> {
    let mut edited = String::new();
    let mut hits = 0;
    for piece in text.split_inclusive('\n') {
        let content = piece.trim_end_matches(['\r', '\n']);
        if content == line {
            hits += 1;
            if let Some(replacement) = replacement {
                edited.push_str(replacement);
                edited.push_str(&piece[content.len()..]);
            }
        } else {
            edited.push_str(piece);
        }
    }
    if hits == 1 {
        Ok(edited)
    } else {
        Err(format!("{hits} lines equal {line:?}, not one"))
    }
}

/// The licenses job of `cargo xtask ci`, from the workspace root: it must exit 0 with its record
/// passed, and record every member's own license as MIT.
fn licenses_job(members: &BTreeSet<&str>, evidence: &mut Evidence) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
    command
        .args(["ci", "licenses"])
        .current_dir(repo_root())
        .env("CARGO_TARGET_DIR", tmp_dir("dc_cyd7-target"));
    let output = run_kept(command, "ci-licenses", evidence);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result_line = stdout
        .lines()
        .find(|line| line.starts_with("CI RESULT"))
        .unwrap_or_default()
        .to_owned();
    evidence.expect(
        output.status.code() == Some(0),
        format!(
            "cargo xtask ci licenses exits 0 (exit code {:?}): {result_line}",
            output.status.code()
        ),
    );
    let path = repo_root().join(LICENSES_RECORD);
    let record: Value =
        match fs::read_to_string(&path).map(|text| serde_json::from_str::<Value>(&text)) {
            Ok(Ok(record)) => record,
            _ => {
                evidence.expect(false, format!("{LICENSES_RECORD} is readable JSON"));
                return;
            }
        };
    evidence.expect(
        record["result"] == "passed" && record["problems"].as_array().is_some_and(Vec::is_empty),
        format!(
            "{LICENSES_RECORD} records result {} with problems {}",
            record["result"], record["problems"]
        ),
    );
    let declared: Vec<(&str, Option<&str>)> = record["own_license"]["declared"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| {
            (
                entry["package"].as_str().unwrap_or_default(),
                entry["license"].as_str(),
            )
        })
        .collect();
    let names: BTreeSet<&str> = declared.iter().map(|(name, _)| *name).collect();
    evidence.expect(
        record["own_license"]["status"] == "declared"
            && names == *members
            && declared.len() == members.len()
            && declared
                .iter()
                .all(|(_, license)| *license == Some(EXPRESSION)),
        format!(
            "{LICENSES_RECORD} records the workspace's own license {} with every member declaring MIT: {declared:?}",
            record["own_license"]["status"]
        ),
    );
}

pub fn workspace_license() {
    registered(CASE);
    let mut evidence = Evidence::new();

    // The expected LICENSE text, from the legacy repositories' pinned LICENSE blobs.
    let templates = legacy_templates(&mut evidence);
    let Some(first) = templates.first() else {
        if evidence.failures.is_empty() {
            evidence.not_run(format!(
                "none of the legacy LICENSE files of {LEGACY_REPOSITORIES:?} that {SOURCE_MANIFEST} pins is readable under the legacy root, so the LICENSE text has no template"
            ));
        } else {
            evidence.finish();
        }
        return;
    };
    let expected = first.text.clone();
    for template in templates.iter().skip(1) {
        evidence.expect(
            template.text == expected,
            format!(
                "{} (holder {:?}) gives the same text as {} once its holder line is {HOLDER_LINE:?}: {}",
                template.source,
                template.original_holder,
                first.source,
                first_difference(&template.text, &expected)
            ),
        );
    }
    let read: Vec<&str> = templates.iter().map(|t| t.source.as_str()).collect();
    evidence.note(format!(
        "{} legacy LICENSE templates read: {read:?}",
        templates.len()
    ));

    // The check on this workspace.
    let findings = check(&repo_root(), &expected, "workspace", &mut evidence);
    expect_findings(
        &findings,
        &[],
        "this workspace: every package of both workspaces reports license MIT through cargo metadata, every member inherits it, and LICENSE equals the template with the holder line",
        &mut evidence,
    );

    // The same check on a working-tree copy, unedited and with one license removed or changed.
    let (copy, copied) = working_tree_copy();
    evidence.line(format!(
        "copy: {copied} files of the working tree under CARGO_TARGET_TMPDIR/dc_cyd7-copy/ws"
    ));
    let findings = check(&copy, &expected, "copy-unedited", &mut evidence);
    expect_findings(&findings, &[], "the unedited copy", &mut evidence);
    let variants = [
        Variant {
            label: "copy-member-license-removed",
            file: "apps/dna-calc/Cargo.toml",
            line: INHERIT_LINE,
            replacement: None,
            wanted: &[("metadata", "dna-calc"), ("inherit", "dna-calc")],
        },
        Variant {
            label: "copy-fixture-license-removed",
            file: FIXTURE_MANIFEST,
            line: FIXTURE_LICENSE_LINE,
            replacement: None,
            wanted: &[("metadata", FIXTURE_PACKAGE)],
        },
        Variant {
            label: "copy-holder-changed",
            file: "LICENSE",
            line: HOLDER_LINE,
            replacement: Some("Copyright (c) 2026 DnaCalc contributors"),
            wanted: &[("license-text", "LICENSE")],
        },
    ];
    for Variant {
        label,
        file,
        line,
        replacement,
        wanted,
    } in variants
    {
        let path = copy.join(file);
        let original = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read the copy's {file}: {e}"));
        match edit_line(&original, line, replacement) {
            Ok(edited) => {
                super::write(&path, &edited);
                let what = match replacement {
                    Some(replacement) => {
                        format!("{file} with {line:?} replaced by {replacement:?}")
                    }
                    None => format!("{file} without {line:?}"),
                };
                let findings = check(&copy, &expected, label, &mut evidence);
                expect_findings(
                    &findings,
                    wanted,
                    &format!("the copy with {what}"),
                    &mut evidence,
                );
                super::write(&path, &original);
            }
            Err(e) => evidence.expect(
                false,
                format!("{label}: the copy's {file} can be edited: {e}"),
            ),
        }
    }

    // The licenses job still passes and sees every member's license.
    let packages = super::workspace_packages();
    let members: BTreeSet<&str> = packages.iter().map(|(name, _)| name.as_str()).collect();
    licenses_job(&members, &mut evidence);

    evidence.finish();
}
