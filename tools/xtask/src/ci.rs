//! `cargo xtask ci windows|linux|wasm|licenses|desktop`: the one repository command each job of
//! `.github/workflows/ci.yml` runs, and the same command anyone runs on a clone. The workflow holds
//! no test logic; what a job runs and records is here.
//!
//! - `windows`, `linux` (on a host of that system): the architecture checks (`arch.rs`), then every
//!   workspace package's native tests: `cargo test --workspace --lib --bins --test <t>...` for every
//!   integration test target except the acceptance targets, then `cargo test --workspace --doc`.
//!   Each package's acceptance target holds the beads' cases, which `cargo xtask accept` runs per
//!   bead: under a plain `cargo test` a case that prints `ACCEPT-NOT-RUN` reports passed, and only
//!   the runner records it `not_run`. The manifest lists the target under `excluded_targets`. Then, for each bead ID in
//!   `DNACALC_CI_ACCEPT` that declares a case for the platform, `cargo xtask accept --id <id>
//!   --platform <job> --require-all`, whose receipt the manifest names.
//! - `wasm`: every portable package's wasm test targets (the same selection, for
//!   wasm32-unknown-unknown; doc tests are not browser evidence and are listed as not run) under
//!   wasm-bindgen-test-runner 0.2.117, once for Chrome and once for Firefox, through the runner's
//!   browser route (`accept.rs`: `WASM_BINDGEN_USE_BROWSER`, one driver, and a run counts only under
//!   the runner's banner naming that browser). The runner starts a browser only for a module that
//!   holds wasm-bindgen tests: for a module without any it prints `no tests to run!` and starts
//!   none, which the manifest records as `runner_reported_no_tests` with `browser_started` false and
//!   status `zero_tests`, never `passed`. Then the route probe of
//!   `tools/xtask/ci-browser-fixture` in each browser, which records the user agent the browser
//!   reports from inside the test. With `DNACALC_CI_INJECT_BROWSER_FAILURE=1` it also runs that
//!   fixture's `failure` target, whose test fails in the browser on purpose, and the job then fails.
//!   Then `DNACALC_CI_ACCEPT` as above, for the beads that declare a wasm case.
//! - `licenses`: every dependency of the workspace and of the browser fixture comes from crates.io
//!   (no git source, no path outside the workspace) under a license expression that
//!   `tools/xtask/licenses.json` lists as reviewed. It reports, without failing, whether the
//!   workspace declares a license of its own.
//! - `desktop`: a release build of the native-only packages, whose files it keeps under
//!   `target/ci/desktop/artifacts/`, and a record of whether a desktop application package exists.
//!   No desktop package exists yet, so nothing is packaged, installed or started: this is not a
//!   packaging smoke. Bead `dc-x704` replaces it with one once the desktop application exists.
//!
//! `windows`, `linux` and `wasm` write `target/ci/<job>/executed-targets.json` (format
//! `dnacalc-executed-targets/1`): for every package, whether cargo built it, whether its test
//! harness ran each target cargo built for it, and the tests run, passed, failed and ignored as
//! libtest's result lines report them, per browser on wasm. A package whose harness ran zero tests
//! has status `zero_tests`: visible, and never `passed`. Each manifest also records the commit and
//! whether the working tree was clean, with every changed path when it was not. Every cargo run's
//! transcript (standard output and standard error merged in the order written) is kept beside it.
//! `cargo xtask ci-audit` (`ci_audit.rs`) merges these manifests across targets.
//!
//! Every cargo started here uses `target/xtask-accept`, and none sees `DNACALC_CI_ACCEPT`,
//! `DNACALC_CI_INJECT_BROWSER_FAILURE` or `DNACALC_CI_COMMIT`. When `DNACALC_CI_COMMIT` is set (the
//! workflow sets it to the commit that started the run), the checkout must be on master at that
//! commit. Every job checks that rustc reports the version `rust-toolchain.toml` pins. A job exits
//! 0 only when everything it ran passed.

use crate::accept::{self, BROWSERS, Browser, Probe, Redactor, WASM_REMOVED};
use crate::arch;
use crate::cargo::{self, Member, WASM_TARGET, Workspace};
use crate::manifest::{self, Manifests, Platform};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::Instant;

pub const USAGE: &str = "usage: cargo xtask ci windows|linux|wasm|licenses|desktop";

/// The format of `executed-targets.json`.
pub const MANIFEST_FORMAT: &str = "dnacalc-executed-targets/1";
pub const MANIFEST_FILE: &str = "executed-targets.json";

/// Bead IDs whose acceptance cases a platform job runs through `cargo xtask accept`.
pub const ACCEPT_VARIABLE: &str = "DNACALC_CI_ACCEPT";
/// Set to 1, the wasm job adds the fixture test that fails in the browser on purpose.
pub const INJECT_VARIABLE: &str = "DNACALC_CI_INJECT_BROWSER_FAILURE";
/// The commit the CI run was started for; the checkout must be on master at it.
pub const COMMIT_VARIABLE: &str = "DNACALC_CI_COMMIT";

/// The browser fixture, its own workspace, relative to the workspace root.
pub const FIXTURE_MANIFEST: &str = "tools/xtask/ci-browser-fixture/Cargo.toml";
pub const FIXTURE_PACKAGE: &str = "dnacalc-ci-browser-fixture";
/// The fixtures as the manifest names them, with their test target and test.
pub const PROBE_FIXTURE: &str = "route-probe";
pub const INJECTED_FIXTURE: &str = "injected-failure";
const FIXTURES: [(&str, &str, &str); 2] = [
    (PROBE_FIXTURE, "probe", "route_probe"),
    (INJECTED_FIXTURE, "failure", "injected_browser_failure"),
];

/// The line the fixture tests print with the user agent of whatever executed them.
const AGENT_MARKER: &str = "CI-BROWSER-AGENT ";

const RUNNER_PROGRAM: &str = "wasm-bindgen-test-runner";

/// The programs whose `--version` line the wasm job records for each browser, when on PATH.
const BROWSER_PROGRAMS: [(&str, &[&str]); 2] = [
    (
        "chrome",
        &[
            "google-chrome",
            "google-chrome-stable",
            "chromium",
            "chromium-browser",
        ],
    ),
    ("firefox", &["firefox"]),
];
/// The runner the browser route's banner rule is written against.
pub const RUNNER_VERSION: &str = "wasm-bindgen-test-runner 0.2.117";

/// Every package's acceptance target; `cargo xtask accept` runs it per bead.
const ACCEPTANCE_TARGET: &str = "acceptance";

pub const LICENSES_FILE: &str = "tools/xtask/licenses.json";
const TOOLCHAIN_FILE: &str = "rust-toolchain.toml";

/// What the desktop job does, as its output and `packaging.json` say it.
const DESKTOP_WHAT: &str = "a release build of the native-only packages, not a packaging smoke: no desktop application package exists yet, so nothing is packaged, installed or started; bead dc-x704 replaces this with a real packaging smoke once the desktop application exists (it depends on dnacalc-m2.1)";

/// Every test run passes `--nocapture` to the harness, so the transcript keeps what each test
/// printed (on wasm, the console of the browser that ran it).
const NOCAPTURE: [&str; 2] = ["--", "--nocapture"];

/// The variables no cargo or accept run started here inherits.
const CI_VARIABLES: [&str; 3] = [ACCEPT_VARIABLE, INJECT_VARIABLE, COMMIT_VARIABLE];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Job {
    Windows,
    Linux,
    Wasm,
    Licenses,
    Desktop,
}

impl Job {
    fn parse(text: &str) -> Option<Job> {
        [
            Job::Windows,
            Job::Linux,
            Job::Wasm,
            Job::Licenses,
            Job::Desktop,
        ]
        .into_iter()
        .find(|job| job.as_str() == text)
    }

    fn as_str(self) -> &'static str {
        match self {
            Job::Windows => "windows",
            Job::Linux => "linux",
            Job::Wasm => "wasm",
            Job::Licenses => "licenses",
            Job::Desktop => "desktop",
        }
    }

    /// What the job does, where its name alone would say more than that.
    fn what(self) -> Option<&'static str> {
        match self {
            Job::Desktop => Some(DESKTOP_WHAT),
            Job::Windows | Job::Linux | Job::Wasm | Job::Licenses => None,
        }
    }

    /// The acceptance platform a job's accept runs use.
    fn platform(self) -> Option<Platform> {
        match self {
            Job::Windows => Some(Platform::Windows),
            Job::Linux => Some(Platform::Linux),
            Job::Wasm => Some(Platform::Wasm),
            Job::Licenses | Job::Desktop => None,
        }
    }
}

/// How one target's run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    ZeroTests,
    Failed,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Passed => "passed",
            Outcome::ZeroTests => "zero_tests",
            Outcome::Failed => "failed",
        }
    }
}

/// The counts of one libtest result line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub passed: u64,
    pub failed: u64,
    pub ignored: u64,
}

impl Counts {
    fn add(&mut self, other: Counts) {
        self.passed += other.passed;
        self.failed += other.failed;
        self.ignored += other.ignored;
    }
}

/// The counts libtest's `test result: ...` line reports, or `None` when it lacks one of them.
pub fn result_counts(line: &str) -> Option<Counts> {
    let rest = line.trim().strip_prefix("test result: ")?;
    let count = |label: &str| {
        rest.split(';').find_map(|part| {
            let mut words = part.split_whitespace().rev();
            let named = words.next() == Some(label);
            let number = words.next().and_then(|n| n.parse::<u64>().ok());
            if named { number } else { None }
        })
    };
    Some(Counts {
        passed: count("passed")?,
        failed: count("failed")?,
        ignored: count("ignored")?,
    })
}

/// One compiler-artifact message of a cargo run.
#[derive(Clone, Debug)]
struct Artifact {
    package_id: String,
    target: String,
    kinds: Vec<String>,
    /// Built with the test profile: a test harness executable.
    test: bool,
    /// The file name of the executable, when cargo reports one.
    executable: Option<String>,
    filenames: Vec<String>,
}

/// What a block of the transcript belongs to: a test executable cargo ran (its file name) or the
/// doc tests of a library crate.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Header {
    Executable(String),
    Doc(String),
}

/// The transcript lines from one cargo `Running` or `Doc-tests` line to the next.
#[derive(Clone, Debug)]
struct Block {
    header: Header,
    output: Vec<String>,
}

/// A cargo test run's merged transcript, read back.
#[derive(Debug, Default)]
struct Transcript {
    artifacts: Vec<Artifact>,
    blocks: Vec<Block>,
    build_finished: Option<bool>,
}

fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned()
}

/// The executable a cargo status line `     Running <source> (<executable>)` names. Cargo indents
/// its status lines; the runner's banner `Running headless tests in ...` starts at the margin.
fn running_executable(line: &str) -> Option<String> {
    if !line.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = line.trim().strip_prefix("Running ")?;
    let (_, path) = rest.strip_suffix(')')?.rsplit_once(" (")?;
    Some(file_name(path))
}

/// The crate a cargo status line `   Doc-tests <crate>` names.
fn doc_tests_crate(line: &str) -> Option<String> {
    if !line.starts_with(char::is_whitespace) {
        return None;
    }
    let name = line.trim().strip_prefix("Doc-tests ")?.trim();
    (!name.is_empty() && !name.contains(' ')).then(|| name.to_owned())
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_transcript(text: &str) -> Transcript {
    let mut transcript = Transcript::default();
    for raw in text.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim_start();
        if trimmed.starts_with("{\"reason\":")
            && let Ok(message) = serde_json::from_str::<Value>(trimmed)
        {
            match message["reason"].as_str() {
                Some("compiler-artifact") => transcript.artifacts.push(Artifact {
                    package_id: message["package_id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    target: message["target"]["name"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    kinds: strings(&message["target"]["kind"]),
                    test: message["profile"]["test"].as_bool() == Some(true),
                    executable: message["executable"].as_str().map(file_name),
                    filenames: strings(&message["filenames"]),
                }),
                Some("build-finished") => {
                    transcript.build_finished = message["success"].as_bool();
                }
                _ => {}
            }
            continue;
        }
        let header = running_executable(line)
            .map(Header::Executable)
            .or_else(|| doc_tests_crate(line).map(Header::Doc));
        match (header, transcript.blocks.last_mut()) {
            (Some(header), _) => transcript.blocks.push(Block {
                header,
                output: Vec::new(),
            }),
            (None, Some(block)) => block.output.push(line.to_owned()),
            (None, None) => {}
        }
    }
    transcript
}

/// What one target's block of the transcript shows.
#[derive(Clone, Debug)]
pub struct Evaluated {
    pub outcome: Outcome,
    pub counts: Counts,
    pub result_line: Option<String>,
    /// wasm-bindgen-test-runner reported that the module holds no tests, so no browser started.
    pub no_tests: bool,
    /// The runner's banner line and the browser it names.
    pub banner: Option<(String, String)>,
    pub reason: Option<String>,
}

/// The outcome of one block: libtest's one result line decides, a cargo `error: ... failed, to
/// rerun pass` line fails it, and on a browser route (`browser`) a run that passed tests counts
/// only under the runner's banner naming that browser. A wasm module without tests makes the
/// runner print `no tests to run!` and start no browser: zero tests, never passed.
fn evaluate(output: &[String], browser: Option<&Browser>) -> Evaluated {
    let text = output.join("\n");
    let results: Vec<&str> = output
        .iter()
        .map(|line| line.trim())
        .filter(|line| line.starts_with("test result: "))
        .collect();
    let cargo_error = output
        .iter()
        .map(|line| line.trim())
        .find(|line| line.starts_with("error: ") && line.contains("failed, to rerun pass"));
    let no_tests = output.iter().any(|line| line.trim() == "no tests to run!");
    let banner = browser.and_then(|_| accept::runner_banner(&text));
    let mut evaluated = Evaluated {
        outcome: Outcome::Failed,
        counts: Counts::default(),
        result_line: results.last().map(|line| (*line).to_owned()),
        no_tests,
        banner: banner.clone(),
        reason: None,
    };
    let counts = match results.as_slice() {
        [line] => result_counts(line),
        _ => None,
    };
    if let Some(counts) = counts {
        evaluated.counts = counts;
    }
    if results.len() > 1 {
        evaluated.reason = Some(format!(
            "{} libtest result lines in one target's output",
            results.len()
        ));
        return evaluated;
    }
    if let Some(error) = cargo_error {
        evaluated.reason = Some(format!("cargo reports `{error}`"));
        return evaluated;
    }
    let Some(counts) = counts else {
        if results.is_empty() && no_tests && browser.is_some() {
            evaluated.outcome = Outcome::ZeroTests;
            evaluated.reason = Some(
                "wasm-bindgen-test-runner reported `no tests to run!`: the module holds no wasm-bindgen tests, so no browser started".to_owned(),
            );
        } else if results.is_empty() {
            evaluated.reason = Some("the target printed no libtest result line".to_owned());
        } else {
            evaluated.reason = Some(format!("unreadable result line {:?}", results[0]));
        }
        return evaluated;
    };
    if counts.failed > 0 {
        evaluated.reason = Some(format!("libtest reports {}", results[0]));
        return evaluated;
    }
    if let Some(browser) = browser
        && counts.passed > 0
    {
        match &banner {
            Some((name, _)) if name == browser.banner => {}
            Some((name, line)) => {
                evaluated.reason = Some(format!(
                    "wasm-bindgen-test-runner ran the tests in {name}, not {} ({line})",
                    browser.banner
                ));
                return evaluated;
            }
            None => {
                evaluated.reason = Some(format!(
                    "the runner's output does not start with `Running headless tests in {} on`, so {} did not run these tests (a target that configures Node.js runs there); its result line is not browser evidence",
                    browser.banner, browser.banner
                ));
                return evaluated;
            }
        }
    }
    if counts.passed > 0 {
        evaluated.outcome = Outcome::Passed;
    } else {
        evaluated.outcome = Outcome::ZeroTests;
        evaluated.reason = Some(format!(
            "the harness ran 0 tests ({} ignored)",
            counts.ignored
        ));
    }
    evaluated
}

/// One target of one package in one cargo run.
struct TargetRun {
    target: String,
    kind: String,
    run: String,
    executable: Option<String>,
    ran: bool,
    evaluated: Evaluated,
    log: String,
}

impl TargetRun {
    fn not_ran(
        target: &str,
        kind: &str,
        run: &str,
        executable: Option<String>,
        log: &str,
        reason: String,
    ) -> TargetRun {
        TargetRun {
            target: target.to_owned(),
            kind: kind.to_owned(),
            run: run.to_owned(),
            executable,
            ran: false,
            evaluated: Evaluated {
                outcome: Outcome::Failed,
                counts: Counts::default(),
                result_line: None,
                no_tests: false,
                banner: None,
                reason: Some(reason),
            },
            log: log.to_owned(),
        }
    }

    fn to_json(&self) -> Value {
        let e = &self.evaluated;
        json!({
            "target": self.target,
            "kind": self.kind,
            "run": self.run,
            "executable": self.executable,
            "ran": self.ran,
            "outcome": e.outcome.as_str(),
            "passed": e.counts.passed,
            "failed": e.counts.failed,
            "ignored": e.counts.ignored,
            "result_line": e.result_line,
            "runner_reported_no_tests": e.no_tests,
            "runner_banner": e.banner.as_ref().map(|(_, line)| line),
            "executed_in": e.banner.as_ref().map(|(name, _)| name),
            "reason": e.reason,
            "log": self.log,
        })
    }
}

/// One package's record for one route (native, or one browser).
struct PackageRun {
    built: bool,
    targets: Vec<TargetRun>,
}

impl PackageRun {
    fn harness_ran(&self) -> bool {
        !self.targets.is_empty() && self.targets.iter().all(|target| target.ran)
    }

    fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for target in &self.targets {
            counts.add(target.evaluated.counts);
        }
        counts
    }

    fn outcome(&self) -> Outcome {
        if !self.built
            || !self.harness_ran()
            || self
                .targets
                .iter()
                .any(|target| target.evaluated.outcome == Outcome::Failed)
        {
            Outcome::Failed
        } else if self.counts().passed > 0 {
            Outcome::Passed
        } else {
            Outcome::ZeroTests
        }
    }

    fn browser_started(&self) -> bool {
        self.targets
            .iter()
            .any(|target| target.evaluated.banner.is_some())
    }

    /// The targets for which wasm-bindgen-test-runner reported `no tests to run!` and started no
    /// browser.
    fn no_browser_targets(&self) -> Vec<&str> {
        self.targets
            .iter()
            .filter(|target| target.evaluated.no_tests && target.evaluated.banner.is_none())
            .map(|target| target.target.as_str())
            .collect()
    }

    fn fields(&self) -> Map<String, Value> {
        let counts = self.counts();
        let mut fields = Map::new();
        fields.insert("built".into(), json!(self.built));
        fields.insert("harness_ran".into(), json!(self.harness_ran()));
        fields.insert("tests_run".into(), json!(counts.passed + counts.failed));
        fields.insert("passed".into(), json!(counts.passed));
        fields.insert("failed".into(), json!(counts.failed));
        fields.insert("ignored".into(), json!(counts.ignored));
        fields.insert("status".into(), json!(self.outcome().as_str()));
        fields.insert(
            "targets".into(),
            Value::Array(self.targets.iter().map(TargetRun::to_json).collect()),
        );
        fields
    }
}

/// A cargo test run whose transcript has been read back, with the label and log it is kept under.
struct TestRun {
    label: String,
    transcript: Transcript,
    log: String,
    exit_code: Option<i32>,
}

/// The lib target's crate name of a member, when it has one.
fn lib_crate(member: &Member) -> Option<String> {
    member
        .targets
        .iter()
        .find(|target| {
            target.kinds.iter().any(|kind| {
                matches!(
                    kind.as_str(),
                    "lib" | "rlib" | "dylib" | "cdylib" | "staticlib" | "proc-macro"
                )
            })
        })
        .map(|target| target.name.replace('-', "_"))
}

/// The record of `member` from `runs`: every test executable cargo built for it must have a block
/// in its run's transcript, and with `doc` the doc-test run must have a block for its library.
fn package_run(
    member: &Member,
    runs: &[TestRun],
    doc: Option<&TestRun>,
    browser: Option<&Browser>,
) -> PackageRun {
    let mut built = !runs.is_empty();
    let mut targets = Vec::new();
    for run in runs {
        let own: Vec<&Artifact> = run
            .transcript
            .artifacts
            .iter()
            .filter(|artifact| artifact.package_id == member.id)
            .collect();
        if own.is_empty() || run.transcript.build_finished != Some(true) {
            built = false;
        }
        for artifact in own.iter().filter(|artifact| artifact.test) {
            let Some(executable) = &artifact.executable else {
                continue;
            };
            let kind = artifact.kinds.first().cloned().unwrap_or_default();
            let block = run
                .transcript
                .blocks
                .iter()
                .find(|block| block.header == Header::Executable(executable.clone()));
            targets.push(match block {
                Some(block) => TargetRun {
                    target: artifact.target.clone(),
                    kind,
                    run: run.label.clone(),
                    executable: Some(executable.clone()),
                    ran: true,
                    evaluated: evaluate(&block.output, browser),
                    log: run.log.clone(),
                },
                None => TargetRun::not_ran(
                    &artifact.target,
                    &kind,
                    &run.label,
                    Some(executable.clone()),
                    &run.log,
                    format!(
                        "cargo built {executable} but the transcript has no `Running` line for it"
                    ),
                ),
            });
        }
    }
    if let (Some(doc), Some(krate)) = (doc, lib_crate(member)) {
        let block = doc
            .transcript
            .blocks
            .iter()
            .find(|block| block.header == Header::Doc(krate.clone()));
        targets.push(match block {
            Some(block) => TargetRun {
                target: krate.clone(),
                kind: "doctest".to_owned(),
                run: doc.label.clone(),
                executable: None,
                ran: true,
                evaluated: evaluate(&block.output, None),
                log: doc.log.clone(),
            },
            None => TargetRun::not_ran(
                &krate,
                "doctest",
                &doc.label,
                None,
                &doc.log,
                format!("the doc-test run has no `Doc-tests {krate}` block"),
            ),
        });
    }
    PackageRun { built, targets }
}

/// A workspace member's role, from its top-level directory.
fn role(ws: &Workspace, member: &Member) -> &'static str {
    match ws.rel(&member.dir).split('/').next() {
        Some("crates") => "library",
        Some("apps") => "app-core",
        Some("platform") => "platform-adapter",
        Some("tools") => "tooling",
        _ => "other",
    }
}

/// The test target names `cargo xtask accept` owns: `acceptance` and every target an
/// acceptance manifest names.
fn acceptance_targets(manifests: &Manifests) -> BTreeSet<String> {
    let mut names = BTreeSet::from([ACCEPTANCE_TARGET.to_owned()]);
    names.extend(manifests.entries.iter().map(|entry| entry.test.clone()));
    names
}

fn integration_tests(member: &Member) -> Vec<&str> {
    member
        .targets
        .iter()
        .filter(|target| target.kinds.iter().any(|kind| kind == "test"))
        .map(|target| target.name.as_str())
        .collect()
}

/// `--lib --bins` and `--test <t>` for every integration test target of `members` that is not an
/// acceptance target.
fn target_selection(members: &[&Member], excluded: &BTreeSet<String>) -> Vec<String> {
    let mut selection = vec!["--lib".to_owned(), "--bins".to_owned()];
    let names: BTreeSet<&str> = members
        .iter()
        .flat_map(|member| integration_tests(member))
        .filter(|name| !excluded.contains(*name))
        .collect();
    for name in names {
        selection.push("--test".to_owned());
        selection.push(name.to_owned());
    }
    selection
}

fn excluded_json(member: &Member, excluded: &BTreeSet<String>) -> Value {
    Value::Array(
        integration_tests(member)
            .into_iter()
            .filter(|name| excluded.contains(*name))
            .map(|name| {
                json!({
                    "target": name,
                    "reason": "an acceptance target holds the beads' named cases, which `cargo xtask accept --id <bead>` runs per bead and platform (DNACALC_CI_ACCEPT in CI): under a plain cargo test a case that prints ACCEPT-NOT-RUN (no Excel, no legacy checkout, no browser) reports passed, and only the runner records it not_run, so package test runs do not run it",
                })
            })
            .collect(),
    )
}

/// The cargo of a test run: `cargo test --locked --no-fail-fast` with JSON messages, in
/// `target/xtask-accept`, from the workspace root, without the CI variables.
fn cargo_test(ws: &Workspace, args: &[String]) -> Command {
    let mut command = Command::new(cargo::cargo_program());
    command
        .args([
            "test",
            "--locked",
            "--no-fail-fast",
            "--message-format=json-render-diagnostics",
        ])
        .arg("--target-dir")
        .arg(ws.accept_target_dir())
        .args(args)
        .current_dir(&ws.root);
    for name in CI_VARIABLES {
        command.env_remove(name);
    }
    command
}

/// One finished process: its argument vector, exit code, merged output and the kept log.
struct Merged {
    argv: Vec<String>,
    exit_code: Option<i32>,
    text: String,
    duration_s: f64,
    log: String,
}

/// Runs `command` with standard output and standard error on one pipe, so the transcript keeps
/// the order in which cargo, the test harnesses and the runner wrote. Lines other than cargo's
/// JSON messages are echoed as they arrive; the whole transcript goes to `log`.
fn run_merged(ws: &Workspace, mut command: Command, log: &Path) -> Result<Merged, String> {
    let argv = cargo::argv(&command);
    println!("CI running {}", argv.join(" "));
    let (reader, writer) = std::io::pipe().map_err(|e| format!("cannot open a pipe: {e}"))?;
    let writer_err = writer
        .try_clone()
        .map_err(|e| format!("cannot share the pipe: {e}"))?;
    command
        .stdin(Stdio::null())
        .stdout(writer)
        .stderr(writer_err);
    let started = Instant::now();
    let mut child = command
        .spawn()
        .map_err(|e| format!("cannot start {argv:?}: {e}"))?;
    // The command holds this process's copies of the pipe's writing end; the reader sees the end
    // of the output only once they are closed.
    drop(command);
    let mut text = String::new();
    let mut lines = BufReader::new(reader);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match lines.read_until(b'\n', &mut buffer) {
            Ok(0) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buffer);
                if !line.trim_start().starts_with("{\"reason\":") {
                    print!("{line}");
                }
                text.push_str(&line);
            }
            Err(e) => return Err(format!("cannot read the output of {argv:?}: {e}")),
        }
    }
    let status = child
        .wait()
        .map_err(|e| format!("cannot wait for {argv:?}: {e}"))?;
    let duration_s = started.elapsed().as_secs_f64();
    let transcript = format!(
        "argv: {argv:?}\nexit code: {:?}\nduration: {duration_s:.3}s\n--- output (standard output and standard error, merged in the order written)\n{text}",
        status.code()
    );
    if let Some(parent) = log.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(log, transcript).map_err(|e| format!("cannot write {}: {e}", log.display()))?;
    println!(
        "CI ran {} (exit code {:?}, {duration_s:.1}s); transcript {}",
        argv.join(" "),
        status.code(),
        ws.rel(log)
    );
    Ok(Merged {
        argv,
        exit_code: status.code(),
        text,
        duration_s,
        log: ws.rel(log),
    })
}

fn rounded(seconds: f64) -> f64 {
    (seconds * 1000.0).round() / 1000.0
}

fn run_json(label: &str, merged: &Merged) -> Value {
    json!({
        "label": label,
        "argv": merged.argv,
        "exit_code": merged.exit_code,
        "duration_s": rounded(merged.duration_s),
        "log": merged.log,
    })
}

/// Runs one cargo test run and reads its transcript back; a run that could not start is a
/// problem and yields an empty transcript, so every package it was to run fails.
fn test_run(
    ws: &Workspace,
    label: &str,
    command: Command,
    log: &Path,
    runs: &mut Vec<Value>,
    problems: &mut Vec<String>,
) -> TestRun {
    match run_merged(ws, command, log) {
        Ok(merged) => {
            runs.push(run_json(label, &merged));
            let transcript = parse_transcript(&merged.text);
            if transcript.build_finished != Some(true) {
                problems.push(format!(
                    "{label}: cargo did not finish building (exit code {:?}); see {}",
                    merged.exit_code, merged.log
                ));
            }
            TestRun {
                label: label.to_owned(),
                transcript,
                log: merged.log,
                exit_code: merged.exit_code,
            }
        }
        Err(e) => {
            problems.push(format!("{label}: {e}"));
            TestRun {
                label: label.to_owned(),
                transcript: Transcript::default(),
                log: ws.rel(log),
                exit_code: None,
            }
        }
    }
}

/// The channel `rust-toolchain.toml` pins, read from its `channel = "<version>"` line.
fn pinned_channel(root: &Path) -> Result<String, String> {
    let text = fs::read_to_string(root.join(TOOLCHAIN_FILE))
        .map_err(|e| format!("cannot read {TOOLCHAIN_FILE}: {e}"))?;
    text.lines()
        .filter_map(|line| line.split('#').next())
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.trim() == "channel")
        .map(|(_, value)| value.trim().trim_matches('"').to_owned())
        .filter(|channel| !channel.is_empty())
        .ok_or_else(|| format!("{TOOLCHAIN_FILE} has no channel"))
}

/// The working tree's changes as `git status` reports them, untracked files included: (status
/// code, path) per entry, and for a rename or copy the new path.
fn dirty_paths(root: &Path) -> Result<Vec<(String, String)>, String> {
    let status = cargo::git(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    Ok(parse_status(&status))
}

/// The entries of `git status --porcelain=v1 -z` output.
fn parse_status(status: &str) -> Vec<(String, String)> {
    let mut paths = Vec::new();
    let mut entries = status.split('\0').filter(|entry| !entry.is_empty());
    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            continue;
        }
        let (code, path) = entry.split_at(3);
        if code.starts_with(['R', 'C']) {
            // The entry after a rename or copy is its source path.
            entries.next();
        }
        paths.push((code.trim().to_owned(), path.to_owned()));
    }
    paths
}

/// The host, toolchain and checkout this job measured, with its problems: a rustc other than the
/// pinned one, and, when `DNACALC_CI_COMMIT` is set, a checkout not on master at that commit.
fn host_record(ws: &Workspace, problems: &mut Vec<String>) -> Value {
    let rustc = cargo::rustc_version();
    let channel = pinned_channel(&ws.root);
    let matches = match (&rustc, &channel) {
        (Ok((_, version)), Ok(channel)) => {
            let ok = version.starts_with(&format!("rustc {channel} "));
            if !ok {
                problems.push(format!(
                    "rustc reports {version:?}, not the {channel} that {TOOLCHAIN_FILE} pins"
                ));
            }
            ok
        }
        (Err(e), _) | (_, Err(e)) => {
            problems.push(format!("toolchain: {e}"));
            false
        }
    };
    let cargo_version = cargo::output(Command::new(cargo::cargo_program()).arg("-V"))
        .map(|out| cargo::text(&out.stdout).trim().to_owned())
        .ok();
    let head = cargo::git(&ws.root, &["rev-parse", "HEAD"]).map(|h| h.trim().to_owned());
    let branch = cargo::git(&ws.root, &["symbolic-ref", "--short", "-q", "HEAD"])
        .map(|b| b.trim().to_owned())
        .ok();
    let dirty = dirty_paths(&ws.root);
    if let Err(e) = &head {
        problems.push(format!("the manifest cannot name the commit: {e}"));
    }
    if let Err(e) = &dirty {
        problems.push(format!(
            "the manifest cannot say whether the working tree is clean: {e}"
        ));
    }
    let expected = std::env::var(COMMIT_VARIABLE)
        .ok()
        .filter(|value| !value.is_empty());
    if let Some(expected) = &expected {
        if head.as_deref().ok() != Some(expected.as_str()) {
            problems.push(format!(
                "{COMMIT_VARIABLE} is {expected}, but the checkout is at {:?}",
                head.as_deref().ok()
            ));
        }
        if branch.as_deref() != Some("master") {
            problems.push(format!(
                "{COMMIT_VARIABLE} is set, but the checkout is on {branch:?}, not master"
            ));
        }
    }
    let ci_environment: Map<String, Value> = [
        "GITHUB_RUN_ID",
        "GITHUB_RUN_ATTEMPT",
        "GITHUB_WORKFLOW",
        "GITHUB_EVENT_NAME",
        "RUNNER_OS",
        "ImageOS",
        "ImageVersion",
    ]
    .into_iter()
    .filter_map(|name| {
        std::env::var(name)
            .ok()
            .map(|value| (name.to_owned(), json!(value)))
    })
    .collect();
    json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "rustc": rustc.as_ref().map(|(_, version)| version.clone()).ok(),
        "rustc_host": rustc.as_ref().map(|(host, _)| host.clone()).ok(),
        "cargo": cargo_version,
        "toolchain_pin": {
            "file": TOOLCHAIN_FILE,
            "channel": channel.ok(),
            "rustc_matches": matches,
        },
        "git": {
            "head": head.ok(),
            "branch": branch,
            "dirty": dirty.as_ref().map(|paths| !paths.is_empty()).ok(),
            "dirty_entries": dirty.as_ref().map(Vec::len).ok(),
            "dirty_paths": dirty.as_ref().ok().map(|paths| {
                paths
                    .iter()
                    .map(|(status, path)| json!({ "status": status, "path": path }))
                    .collect::<Vec<_>>()
            }),
        },
        "ci_commit": expected,
        "ci_environment": ci_environment,
    })
}

/// The bead IDs `DNACALC_CI_ACCEPT` lists, separated by commas or white space.
fn accept_ids() -> Vec<String> {
    std::env::var(ACCEPT_VARIABLE)
        .unwrap_or_default()
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Runs `cargo xtask accept --id <id> --platform <platform> --require-all` (this binary, which is
/// what `cargo xtask` runs) for each ID in `DNACALC_CI_ACCEPT` whose cases declare the platform.
fn run_accepts(
    ws: &Workspace,
    manifests: &Manifests,
    platform: Platform,
    out_dir: &Path,
    problems: &mut Vec<String>,
) -> Vec<Value> {
    let mut records = Vec::new();
    for id in accept_ids() {
        let valid = id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.');
        if !valid {
            problems.push(format!(
                "{ACCEPT_VARIABLE} names {id:?}, which is not a bead ID"
            ));
            records.push(json!({ "id": id, "outcome": "failed", "reason": "not a bead ID" }));
            continue;
        }
        let registered = manifests.entries.iter().any(|entry| entry.id == id);
        let declares = manifests.entries.iter().any(|entry| {
            entry.id == id
                && entry
                    .cases
                    .iter()
                    .any(|case| case.platforms.contains(&platform))
        });
        if registered && !declares {
            println!("CI accept {id}: no case of {id} declares {platform}; not run on this job");
            records.push(json!({
                "id": id,
                "platform": platform.as_str(),
                "outcome": "not_run",
                "reason": format!("no case of {id} declares {platform}"),
            }));
            continue;
        }
        let program = match std::env::current_exe() {
            Ok(program) => program,
            Err(e) => {
                problems.push(format!("cannot find this program to run accept: {e}"));
                continue;
            }
        };
        let mut command = Command::new(program);
        command
            .args([
                "accept",
                "--id",
                &id,
                "--platform",
                platform.as_str(),
                "--require-all",
            ])
            .current_dir(&ws.root)
            .stdin(Stdio::null());
        for name in CI_VARIABLES {
            command.env_remove(name);
        }
        let shown = format!("cargo xtask accept --id {id} --platform {platform} --require-all");
        println!("CI running {shown}");
        let log = out_dir.join(format!("accept-{id}.log"));
        let out = cargo::output(&mut command);
        let (exit_code, stdout) = match &out {
            Ok(out) => (out.status.code(), cargo::text(&out.stdout)),
            Err(e) => {
                problems.push(format!("{shown}: {e}"));
                (None, String::new())
            }
        };
        if let Ok(out) = &out {
            let _ = fs::write(
                &log,
                format!(
                    "argv: {shown}\nexit code: {exit_code:?}\n--- stdout\n{stdout}\n--- stderr\n{}",
                    cargo::text(&out.stderr)
                ),
            );
        }
        for line in stdout.lines().filter(|line| line.starts_with("ACCEPT ")) {
            println!("  {line}");
        }
        let receipt = format!("target/accept/{id}/{platform}/receipt.json");
        let result = fs::read_to_string(ws.root.join(&receipt))
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .and_then(|value| value["result"].as_str().map(str::to_owned));
        if exit_code != Some(0) {
            problems.push(format!(
                "{shown} exited with {exit_code:?} (receipt result {result:?}); see {}",
                ws.rel(&log)
            ));
        }
        records.push(json!({
            "id": id,
            "platform": platform.as_str(),
            "argv": ["cargo", "xtask", "accept", "--id", id, "--platform", platform.as_str(), "--require-all"],
            "exit_code": exit_code,
            "outcome": if exit_code == Some(0) { "passed" } else { "failed" },
            "result": result,
            "receipt": receipt,
            "committed_copy": registered.then(|| format!("accept-receipts/{id}/{platform}.json")),
            "log": ws.rel(&log),
        }));
    }
    records
}

/// One package's line; on a browser route it also says which targets ran in the browser and for
/// which the runner reported no tests and started none.
fn print_package_line(
    status: Outcome,
    package: &str,
    on: &str,
    run: &PackageRun,
    browser: Option<&Browser>,
) {
    let counts = run.counts();
    let route = browser.map_or_else(String::new, |browser| {
        let started = run
            .targets
            .iter()
            .filter(|target| target.evaluated.banner.is_some())
            .count();
        let no_tests = run.no_browser_targets();
        let mut text = format!(
            "; {} started for {started} of {} targets",
            browser.banner,
            run.targets.len()
        );
        if !no_tests.is_empty() {
            text.push_str(&format!(
                "; wasm-bindgen-test-runner reported `no tests to run!` and started no browser for {}",
                no_tests.join(", ")
            ));
        }
        text
    });
    println!(
        "CI {} {package} on {on}: built {}, harness ran {} of {} targets, {} tests run, {} passed, {} failed, {} ignored{route}",
        status.as_str(),
        run.built,
        run.targets.iter().filter(|t| t.ran).count(),
        run.targets.len(),
        counts.passed + counts.failed,
        counts.passed,
        counts.failed,
        counts.ignored
    );
    for target in run
        .targets
        .iter()
        .filter(|target| target.evaluated.outcome == Outcome::Failed)
    {
        println!(
            "  failed {} ({}): {}",
            target.target,
            target.kind,
            target.evaluated.reason.as_deref().unwrap_or_default()
        );
    }
}

/// The native job: architecture checks, package tests, doc tests, accept runs.
fn native_job(
    ws: &Workspace,
    manifests: &Manifests,
    job: Job,
    out_dir: &Path,
    record: &mut Map<String, Value>,
    problems: &mut Vec<String>,
) {
    let os = std::env::consts::OS;
    if os != job.as_str() {
        problems.push(format!(
            "job {} needs a {} host; this host is {os}, so nothing ran",
            job.as_str(),
            job.as_str()
        ));
        record.insert("packages".into(), json!([]));
        return;
    }
    let checks = arch::run_all(ws, &out_dir.join("arch"));
    for check in &checks {
        println!(
            "ARCH {} {}: {}",
            if check.passed { "ok" } else { "FAIL" },
            check.name,
            check.summary
        );
        for finding in &check.findings {
            println!("  {finding}");
        }
        if !check.passed {
            problems.push(format!("architecture check {} failed", check.name));
        }
    }
    record.insert(
        "architecture".into(),
        Value::Array(
            checks
                .iter()
                .map(|check| {
                    json!({
                        "check": check.name,
                        "passed": check.passed,
                        "summary": check.summary,
                        "findings": check.findings,
                    })
                })
                .collect(),
        ),
    );

    let excluded = acceptance_targets(manifests);
    let members: Vec<&Member> = ws.members.iter().collect();
    let mut runs = Vec::new();
    let mut args = vec!["--workspace".to_owned()];
    args.extend(target_selection(&members, &excluded));
    args.extend(NOCAPTURE.map(str::to_owned));
    let tests = test_run(
        ws,
        "tests",
        cargo_test(ws, &args),
        &out_dir.join("cargo-test.log"),
        &mut runs,
        problems,
    );
    let doc = test_run(
        ws,
        "doc-tests",
        cargo_test(
            ws,
            &["--workspace", "--doc"]
                .into_iter()
                .chain(NOCAPTURE)
                .map(str::to_owned)
                .collect::<Vec<_>>(),
        ),
        &out_dir.join("cargo-test-doc.log"),
        &mut runs,
        problems,
    );
    let mut packages = Vec::new();
    let mut any_failed = false;
    for member in &members {
        let run = package_run(member, std::slice::from_ref(&tests), Some(&doc), None);
        let status = run.outcome();
        any_failed |= status == Outcome::Failed;
        print_package_line(status, &member.name, job.as_str(), &run, None);
        let mut fields = Map::new();
        fields.insert("package".into(), json!(member.name));
        fields.insert("portability".into(), json!(member.portability));
        fields.insert("role".into(), json!(role(ws, member)));
        fields.extend(run.fields());
        fields.insert("excluded_targets".into(), excluded_json(member, &excluded));
        packages.push(Value::Object(fields));
    }
    if any_failed {
        problems.push(format!("a package failed on {}", job.as_str()));
    }
    for run in [&tests, &doc] {
        let attributed = run
            .transcript
            .blocks
            .iter()
            .any(|block| evaluate(&block.output, None).outcome == Outcome::Failed);
        if run.exit_code != Some(0) && !attributed && run.transcript.build_finished == Some(true) {
            problems.push(format!(
                "{}: cargo exited with {:?} although no target failed; see {}",
                run.label, run.exit_code, run.log
            ));
        }
    }
    record.insert("runs".into(), Value::Array(runs));
    record.insert("packages".into(), Value::Array(packages));
}

/// The first line `<program> --version` prints, or why it printed none.
fn version_of(program: &Path) -> Result<String, String> {
    let out = cargo::output(Command::new(program).arg("--version"))?;
    let text = cargo::text(&out.stdout);
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("{} --version printed nothing", program.display()))
}

/// The user agent a fixture test printed from inside whatever executed it.
fn agent_of(output: &[String], test: &str) -> Option<String> {
    let marker = format!("{AGENT_MARKER}{test}: ");
    output.iter().find_map(|line| {
        line.find(&marker)
            .map(|at| line[at + marker.len()..].trim().to_owned())
    })
}

/// What the wasm job ran in one browser: each portable package's record and the fixture runs.
type BrowserRuns = (&'static Browser, Vec<(String, PackageRun)>, Vec<Value>);

/// The wasm job: the portable packages and the fixture in each browser, then accept runs.
fn wasm_job(
    ws: &Workspace,
    manifests: &Manifests,
    out_dir: &Path,
    record: &mut Map<String, Value>,
    problems: &mut Vec<String>,
) {
    let inject = std::env::var(INJECT_VARIABLE).is_ok_and(|value| value == "1");
    record.insert("inject_browser_failure".into(), json!(inject));
    let mut tools = Map::new();
    match accept::find_on_path(RUNNER_PROGRAM) {
        None => problems.push(format!("{RUNNER_PROGRAM} is not on PATH")),
        Some(runner) => match version_of(&runner) {
            Ok(version) => {
                if version != RUNNER_VERSION {
                    problems.push(format!(
                        "{RUNNER_PROGRAM} reports {version:?}; the browser route's banner rule is written against {RUNNER_VERSION:?}"
                    ));
                }
                tools.insert(RUNNER_PROGRAM.into(), json!(version));
            }
            Err(e) => problems.push(e),
        },
    }
    let routes = accept::wasm_routes(&Probe::HOST);
    for (browser, route) in &routes {
        match route {
            Err(reason) => problems.push(format!(
                "{}: {reason}; the wasm job needs Chrome and Firefox",
                browser.name
            )),
            Ok(environment) => {
                let driver = environment
                    .iter()
                    .find(|(name, _)| *name == browser.variable)
                    .map(|(_, path)| PathBuf::from(path));
                if let Some(driver) = driver {
                    tools.insert(
                        browser.program.into(),
                        json!(version_of(&driver).unwrap_or_else(|e| e)),
                    );
                }
            }
        }
    }
    // The browsers' own version lines, where their programs are on PATH (the Linux runner image);
    // the user agent each browser reports to the route probe is recorded in every case.
    for (label, programs) in BROWSER_PROGRAMS {
        let found = programs
            .iter()
            .find_map(|program| accept::find_on_path(program));
        let version = match found {
            Some(path) => version_of(&path).unwrap_or_else(|e| e),
            None => format!(
                "not on PATH ({}); see browser_user_agents",
                programs.join(", ")
            ),
        };
        tools.insert(label.into(), json!(version));
    }
    record.insert("tools".into(), Value::Object(tools));

    let excluded = acceptance_targets(manifests);
    let portable: Vec<&Member> = ws.members.iter().filter(|m| m.is_portable()).collect();
    let mut args = vec!["--workspace".to_owned()];
    for member in ws.members.iter().filter(|m| !m.is_portable()) {
        args.push("--exclude".to_owned());
        args.push(member.name.clone());
    }
    args.extend(["--target".to_owned(), WASM_TARGET.to_owned()]);
    args.extend(target_selection(&portable, &excluded));
    args.extend(NOCAPTURE.map(str::to_owned));

    let mut fixture_targets = vec!["--test".to_owned(), "probe".to_owned()];
    if inject {
        fixture_targets.extend(["--test".to_owned(), "failure".to_owned()]);
    }
    let mut fixture_args = vec![
        "--manifest-path".to_owned(),
        ws.root
            .join(FIXTURE_MANIFEST)
            .to_string_lossy()
            .into_owned(),
        "--target".to_owned(),
        WASM_TARGET.to_owned(),
    ];
    fixture_args.extend(fixture_targets);
    fixture_args.extend(NOCAPTURE.map(str::to_owned));

    let mut runs = Vec::new();
    let mut per_browser: Vec<BrowserRuns> = Vec::new();
    for (browser, route) in &routes {
        let browser: &'static Browser = browser;
        let Ok(environment) = route else {
            continue;
        };
        let apply = |command: &mut Command| {
            for name in WASM_REMOVED {
                command.env_remove(name);
            }
            for (name, setting) in environment {
                command.env(name, setting);
            }
        };
        let mut command = cargo_test(ws, &args);
        apply(&mut command);
        let tests = test_run(
            ws,
            &format!("tests@{}", browser.name),
            command,
            &out_dir.join(format!("cargo-test@{}.log", browser.name)),
            &mut runs,
            problems,
        );
        let packages: Vec<(String, PackageRun)> = portable
            .iter()
            .map(|member| {
                let run = package_run(member, std::slice::from_ref(&tests), None, Some(browser));
                (member.name.clone(), run)
            })
            .collect();
        let attributed = tests
            .transcript
            .blocks
            .iter()
            .any(|block| evaluate(&block.output, Some(browser)).outcome == Outcome::Failed);
        if tests.exit_code != Some(0)
            && !attributed
            && tests.transcript.build_finished == Some(true)
        {
            problems.push(format!(
                "tests@{}: cargo exited with {:?} although no target failed; see {}",
                browser.name, tests.exit_code, tests.log
            ));
        }

        let mut command = cargo_test(ws, &fixture_args);
        apply(&mut command);
        let fixture = test_run(
            ws,
            &format!("fixture@{}", browser.name),
            command,
            &out_dir.join(format!("fixture@{}.log", browser.name)),
            &mut runs,
            problems,
        );
        let mut fixtures = Vec::new();
        for (name, target, test) in FIXTURES {
            if name == INJECTED_FIXTURE && !inject {
                continue;
            }
            let artifact = fixture.transcript.artifacts.iter().find(|artifact| {
                artifact.test && artifact.target == target && artifact.executable.is_some()
            });
            let header = artifact
                .and_then(|artifact| artifact.executable.clone())
                .map(Header::Executable);
            let block = header.and_then(|header| {
                fixture
                    .transcript
                    .blocks
                    .iter()
                    .find(|block| block.header == header)
            });
            let (evaluated, agent) = match block {
                Some(block) => (
                    evaluate(&block.output, Some(browser)),
                    agent_of(&block.output, test),
                ),
                None => (
                    Evaluated {
                        outcome: Outcome::Failed,
                        counts: Counts::default(),
                        result_line: None,
                        no_tests: false,
                        banner: None,
                        reason: Some(format!(
                            "the fixture run has no `Running` block for test target {target}"
                        )),
                    },
                    None,
                ),
            };
            let executed_in = evaluated.banner.as_ref().map(|(name, _)| name.clone());
            println!(
                "CI {} {name}@{} (package {FIXTURE_PACKAGE}, test target {target}{}): {}{}",
                evaluated.outcome.as_str(),
                browser.name,
                executed_in
                    .as_ref()
                    .map(|name| format!(", ran in {name}"))
                    .unwrap_or_default(),
                evaluated.result_line.as_deref().unwrap_or("no result line"),
                agent
                    .as_ref()
                    .map(|agent| format!("; user agent {agent}"))
                    .unwrap_or_default()
            );
            match name {
                PROBE_FIXTURE if evaluated.outcome != Outcome::Passed => {
                    problems.push(format!(
                        "the route probe did not pass in {}: {}",
                        browser.name,
                        evaluated.reason.as_deref().unwrap_or_default()
                    ));
                }
                INJECTED_FIXTURE if evaluated.outcome == Outcome::Failed => {
                    problems.push(format!(
                        "{INJECTED_FIXTURE} failed in {} as {INJECT_VARIABLE}=1 makes it fail",
                        browser.name
                    ));
                }
                INJECTED_FIXTURE => problems.push(format!(
                    "{INJECTED_FIXTURE} did not fail in {} ({}), so the fixture did not do what it must",
                    browser.name,
                    evaluated.outcome.as_str()
                )),
                _ => {}
            }
            fixtures.push(json!({
                "fixture": name,
                "browser": browser.name,
                "test_target": target,
                "test": test,
                "outcome": evaluated.outcome.as_str(),
                "passed": evaluated.counts.passed,
                "failed": evaluated.counts.failed,
                "result_line": evaluated.result_line,
                "runner_banner": evaluated.banner.as_ref().map(|(_, line)| line),
                "executed_in": executed_in,
                "user_agent": agent,
                "reason": evaluated.reason,
                "log": fixture.log,
            }));
        }
        per_browser.push((browser, packages, fixtures));
    }

    let mut packages = Vec::new();
    let mut any_failed = false;
    for member in &portable {
        let mut browsers = Vec::new();
        let (mut built, mut harness_ran, mut worst) = (true, true, Outcome::Passed);
        let mut zero = false;
        for browser in &BROWSERS {
            let Some((_, runs, _)) = per_browser.iter().find(|(b, _, _)| b.name == browser.name)
            else {
                (built, harness_ran, worst) = (false, false, Outcome::Failed);
                browsers.push(json!({
                    "browser": browser.name,
                    "built": false,
                    "harness_ran": false,
                    "status": "failed",
                    "reason": format!("no route to {} on this host", browser.name),
                }));
                continue;
            };
            let Some((_, run)) = runs.iter().find(|(name, _)| *name == member.name) else {
                (built, harness_ran, worst) = (false, false, Outcome::Failed);
                browsers.push(json!({
                    "browser": browser.name,
                    "built": false,
                    "harness_ran": false,
                    "status": "failed",
                    "reason": format!("the {} run has no record of {}", browser.name, member.name),
                }));
                continue;
            };
            let status = run.outcome();
            print_package_line(
                status,
                &format!("{}@{}", member.name, browser.name),
                "wasm",
                run,
                Some(browser),
            );
            built &= run.built;
            harness_ran &= run.harness_ran();
            match status {
                Outcome::Failed => worst = Outcome::Failed,
                Outcome::ZeroTests => zero = true,
                Outcome::Passed => {}
            }
            let mut fields = Map::new();
            fields.insert("browser".into(), json!(browser.name));
            fields.extend(run.fields());
            fields.insert("browser_started".into(), json!(run.browser_started()));
            fields.insert(
                "runner_reported_no_tests_for".into(),
                json!(run.no_browser_targets()),
            );
            browsers.push(Value::Object(fields));
        }
        if worst != Outcome::Failed && zero {
            worst = Outcome::ZeroTests;
        }
        any_failed |= worst == Outcome::Failed;
        packages.push(json!({
            "package": member.name,
            "portability": member.portability,
            "role": role(ws, member),
            "built": built,
            "harness_ran": harness_ran,
            "status": worst.as_str(),
            "browsers": browsers,
            "excluded_targets": excluded_json(member, &excluded),
            "not_run_on_wasm": [
                "doc tests: rustdoc would hand each doctest to wasm-bindgen-test-runner as a plain libtest module, which the runner reports as `no tests to run!` and rustdoc as passed, so no doctest result on wasm is browser evidence",
            ],
        }));
    }
    if any_failed {
        problems.push("a portable package failed on wasm".to_owned());
    }
    let fixtures: Vec<Value> = per_browser
        .iter()
        .flat_map(|(_, _, fixtures)| fixtures.iter().cloned())
        .collect();
    let agents: Map<String, Value> = fixtures
        .iter()
        .filter(|fixture| fixture["fixture"] == PROBE_FIXTURE)
        .filter_map(|fixture| {
            Some((
                fixture["browser"].as_str()?.to_owned(),
                fixture["user_agent"].clone(),
            ))
        })
        .collect();
    record.insert("browser_user_agents".into(), Value::Object(agents));
    record.insert("runs".into(), Value::Array(runs));
    record.insert("packages".into(), Value::Array(packages));
    record.insert("fixtures".into(), Value::Array(fixtures));
}

/// The reviewed sources and license expressions of `tools/xtask/licenses.json`.
struct AllowList {
    sources: Vec<String>,
    expressions: Vec<String>,
}

fn allow_list(root: &Path) -> Result<AllowList, String> {
    let text = fs::read_to_string(root.join(LICENSES_FILE))
        .map_err(|e| format!("cannot read {LICENSES_FILE}: {e}"))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| format!("{LICENSES_FILE} is not JSON: {e}"))?;
    let sources = strings(&value["sources"]);
    let expressions: Vec<String> = value["expressions"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["expression"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if sources.is_empty() || expressions.is_empty() {
        return Err(format!(
            "{LICENSES_FILE} lists no sources or no expressions"
        ));
    }
    Ok(AllowList {
        sources,
        expressions,
    })
}

/// The licenses job: every dependency's source and license expression, in the workspace and in
/// the browser fixture's workspace.
fn licenses_job(ws: &Workspace, record: &mut Map<String, Value>, problems: &mut Vec<String>) {
    let allow = match allow_list(&ws.root) {
        Ok(allow) => allow,
        Err(e) => {
            problems.push(e);
            return;
        }
    };
    let mut workspaces = Vec::new();
    let mut own_declared = Vec::new();
    let mut own_members = 0;
    for (label, manifest_path) in [
        ("workspace", None),
        ("ci-browser-fixture", Some(FIXTURE_MANIFEST)),
    ] {
        let mut command = Command::new(cargo::cargo_program());
        command
            .args([
                "metadata",
                "--format-version",
                "1",
                "--locked",
                "--all-features",
            ])
            .current_dir(&ws.root);
        if let Some(path) = manifest_path {
            command.arg("--manifest-path").arg(ws.root.join(path));
        }
        let metadata = cargo::output(&mut command).and_then(|out| {
            if out.status.success() {
                serde_json::from_slice::<Value>(&out.stdout)
                    .map_err(|e| format!("cargo metadata printed no JSON: {e}"))
            } else {
                Err(format!(
                    "cargo metadata exited with {:?}: {}",
                    out.status.code(),
                    cargo::tail(&out.stderr, 10)
                ))
            }
        });
        let metadata = match metadata {
            Ok(metadata) => metadata,
            Err(e) => {
                problems.push(format!("{label}: {e}"));
                continue;
            }
        };
        let members: BTreeSet<&str> = metadata["workspace_members"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let mut dependencies = Vec::new();
        let mut findings = Vec::new();
        for package in metadata["packages"].as_array().into_iter().flatten() {
            let id = package["id"].as_str().unwrap_or_default();
            let name = package["name"].as_str().unwrap_or_default();
            let version = package["version"].as_str().unwrap_or_default();
            let license = package["license"].as_str();
            let license_file = package["license_file"].as_str();
            if members.contains(id) {
                if label == "workspace" {
                    own_members += 1;
                    if license.is_some() || license_file.is_some() {
                        own_declared.push(json!({
                            "package": name,
                            "license": license,
                            "license_file": license_file,
                        }));
                    }
                }
                continue;
            }
            let source = package["source"].as_str();
            let mut problems_here = Vec::new();
            match source {
                Some(source) if allow.sources.iter().any(|allowed| allowed == source) => {}
                Some(source) if source.starts_with("git+") => {
                    problems_here.push(format!("comes from git ({source}), not crates.io"));
                }
                Some(source) => problems_here.push(format!("comes from {source}, not crates.io")),
                None => problems_here.push(
                    "is a path dependency outside the workspace, not a crates.io package"
                        .to_owned(),
                ),
            }
            match license {
                Some(expression) if allow.expressions.iter().any(|e| e == expression) => {}
                Some(expression) => problems_here.push(format!(
                    "declares license {expression:?}, which {LICENSES_FILE} does not list as reviewed"
                )),
                None => problems_here.push(format!(
                    "declares no license expression (license-file {license_file:?}), which {LICENSES_FILE} cannot review"
                )),
            }
            for problem in &problems_here {
                findings.push(format!("{label}: {name} {version} {problem}"));
            }
            dependencies.push(json!({
                "name": name,
                "version": version,
                "source": source,
                "license": license,
                "outcome": if problems_here.is_empty() { "allowed" } else { "refused" },
            }));
        }
        println!(
            "CI licenses {label}: {} dependencies, {} refused",
            dependencies.len(),
            findings.len()
        );
        for finding in &findings {
            println!("  {finding}");
        }
        problems.extend(findings.iter().cloned());
        workspaces.push(json!({
            "label": label,
            "manifest": manifest_path.unwrap_or("Cargo.toml"),
            "dependencies": dependencies,
            "findings": findings,
        }));
    }
    let own = if own_declared.is_empty() {
        let note = format!(
            "not chosen: none of the {own_members} workspace packages declares license or license-file; choosing the workspace's license is the owner's decision"
        );
        println!("CI note: the workspace's own license is {note}");
        json!({ "status": "not_chosen", "note": note, "declared": [] })
    } else {
        json!({ "status": "declared", "declared": own_declared })
    };
    record.insert("allow_list".into(), json!(LICENSES_FILE));
    record.insert("workspaces".into(), Value::Array(workspaces));
    record.insert("own_license".into(), own);
}

/// The desktop job: a release build of the native-only packages, its files kept, and whether a
/// desktop application package exists. It packages nothing; `dc-x704` owns the packaging smoke.
fn desktop_job(
    ws: &Workspace,
    out_dir: &Path,
    record: &mut Map<String, Value>,
    problems: &mut Vec<String>,
) {
    let native: Vec<&Member> = ws.members.iter().filter(|m| m.is_native_only()).collect();
    let mut command = Command::new(cargo::cargo_program());
    command
        .args([
            "build",
            "--release",
            "--locked",
            "--message-format=json-render-diagnostics",
        ])
        .arg("--target-dir")
        .arg(ws.accept_target_dir())
        .current_dir(&ws.root);
    for member in &native {
        command.args(["-p", &member.name]);
    }
    for name in CI_VARIABLES {
        command.env_remove(name);
    }
    let merged = match run_merged(ws, command, &out_dir.join("cargo-build-release.log")) {
        Ok(merged) => merged,
        Err(e) => {
            problems.push(e);
            return;
        }
    };
    let transcript = parse_transcript(&merged.text);
    if merged.exit_code != Some(0) || transcript.build_finished != Some(true) {
        problems.push(format!(
            "the release build exited with {:?}; see {}",
            merged.exit_code, merged.log
        ));
    }
    let artifacts_dir = out_dir.join("artifacts");
    let mut built = Vec::new();
    for member in &native {
        let files: Vec<&String> = transcript
            .artifacts
            .iter()
            .filter(|a| a.package_id == member.id && !a.kinds.iter().any(|k| k == "custom-build"))
            .flat_map(|a| a.filenames.iter())
            .collect();
        if files.is_empty() {
            problems.push(format!(
                "the release build reported no file for {}",
                member.name
            ));
        }
        for file in files {
            let source = PathBuf::from(file);
            let name = file_name(file);
            let kept = artifacts_dir.join(&name);
            let copied = fs::create_dir_all(&artifacts_dir).and_then(|()| fs::copy(&source, &kept));
            match copied {
                Ok(bytes) => built.push(json!({
                    "package": member.name,
                    "file": name,
                    "bytes": bytes,
                    "kept": ws.rel(&kept),
                })),
                Err(e) => problems.push(format!("cannot keep {}: {e}", source.display())),
            }
        }
    }
    let executables: Vec<String> = ws
        .members
        .iter()
        .filter(|member| role(ws, member) != "tooling")
        .flat_map(|member| {
            member
                .targets
                .iter()
                .filter(|target| target.kinds.iter().any(|kind| kind == "bin"))
                .map(move |target| format!("{}:{}", member.name, target.name))
        })
        .collect();
    let desktop = if executables.is_empty() {
        let note = "no desktop application package exists yet: no workspace package outside tools/ has a bin target, so there is nothing to package, install or start; this job is a release build of the native-only packages until bead dc-x704 replaces it with a packaging smoke of the desktop application (dc-x704 depends on dnacalc-m2.1)";
        println!("CI note: {note}");
        json!({ "exists": false, "executables": executables, "note": note })
    } else {
        let note = "a bin target exists outside tools/, but this job does not package, install or start it; bead dc-x704 owns that packaging smoke";
        println!("CI note: {note}");
        json!({ "exists": true, "executables": executables, "note": note })
    };
    record.insert("what".into(), json!(DESKTOP_WHAT));
    record.insert("packaged".into(), json!(false));
    record.insert("packaging_smoke_owner".into(), json!("dc-x704"));
    record.insert("runs".into(), json!([run_json("release-build", &merged)]));
    record.insert("built".into(), Value::Array(built));
    record.insert("desktop_application".into(), desktop);
}

pub fn run(raw: &[String]) -> ExitCode {
    let job = match raw {
        [job] => Job::parse(job),
        _ => None,
    };
    let Some(job) = job else {
        eprintln!("xtask ci: unknown or missing job {raw:?}\n{USAGE}");
        return ExitCode::from(2);
    };
    let started = Instant::now();
    let started_utc = accept::utc_now();
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("xtask ci: no current directory: {e}");
            return ExitCode::from(1);
        }
    };
    let ws = match Workspace::load(&cwd) {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("xtask ci: cannot read the workspace: {e}");
            return ExitCode::from(1);
        }
    };
    let out_dir = ws.root.join("target").join("ci").join(job.as_str());
    if out_dir.exists()
        && let Err(e) = fs::remove_dir_all(&out_dir)
    {
        eprintln!("xtask ci: cannot clear {}: {e}", out_dir.display());
        return ExitCode::from(1);
    }
    if let Err(e) = fs::create_dir_all(&out_dir) {
        eprintln!("xtask ci: cannot create {}: {e}", out_dir.display());
        return ExitCode::from(1);
    }
    println!(
        "CI {} in {}: cargo target directory {}, record {}",
        job.as_str(),
        ws.root.display(),
        cargo::ACCEPT_TARGET_DIR,
        ws.rel(&out_dir)
    );
    if let Some(what) = job.what() {
        println!("CI {}: {what}", job.as_str());
    }
    let mut problems = Vec::new();
    let host = host_record(&ws, &mut problems);
    let manifests = manifest::load(&ws);
    problems.extend(manifests.errors.iter().map(|e| format!("manifest: {e}")));

    let mut record = Map::new();
    match job {
        Job::Windows | Job::Linux => {
            native_job(&ws, &manifests, job, &out_dir, &mut record, &mut problems);
        }
        Job::Wasm => wasm_job(&ws, &manifests, &out_dir, &mut record, &mut problems),
        Job::Licenses => licenses_job(&ws, &mut record, &mut problems),
        Job::Desktop => desktop_job(&ws, &out_dir, &mut record, &mut problems),
    }
    // A native job on the other system ran nothing, and runs no accept either.
    let host_runs_job = match job {
        Job::Windows | Job::Linux => std::env::consts::OS == job.as_str(),
        Job::Wasm | Job::Licenses | Job::Desktop => true,
    };
    if let Some(platform) = job.platform()
        && host_runs_job
    {
        let accepts = run_accepts(&ws, &manifests, platform, &out_dir, &mut problems);
        record.insert("accept".into(), Value::Array(accepts));
    }
    for problem in &problems {
        println!("CI problem: {problem}");
    }
    let result = if problems.is_empty() {
        "passed"
    } else {
        "failed"
    };
    let exit_code: u8 = if problems.is_empty() { 0 } else { 1 };
    let (format, file) = match job {
        Job::Windows | Job::Linux | Job::Wasm => (MANIFEST_FORMAT, MANIFEST_FILE),
        Job::Licenses => ("dnacalc-licenses/1", "licenses.json"),
        // dc-x704 turns this record into the packaging smoke's; until then it records a build.
        Job::Desktop => ("dnacalc-native-release-build/1", "packaging.json"),
    };
    let mut manifest = json!({
        "format": format,
        "job": job.as_str(),
        "target": job.platform().map(Platform::as_str),
        "triple": match job {
            Job::Wasm => Some(WASM_TARGET.to_owned()),
            Job::Windows | Job::Linux => host["rustc_host"].as_str().map(str::to_owned),
            Job::Licenses | Job::Desktop => None,
        },
        "result": result,
        "exit_code": exit_code,
        "status_rule": "a package is passed when every target cargo built for it ran and its harness ran at least one test with none failing; zero_tests when every target ran and none ran a test (never counted as passed; on wasm this includes a module without wasm-bindgen tests, for which the runner reports `no tests to run!` and starts no browser, as runner_reported_no_tests_for and browser_started record); failed when it was not built, a target did not run, a test failed, or (on wasm) a target passed tests without the runner's banner naming the browser of its run. The job is passed only when no package failed and nothing else it ran failed.",
        "command": ["cargo", "xtask", "ci", job.as_str()],
        "started_utc": started_utc,
        "duration_s": rounded(started.elapsed().as_secs_f64()),
        "host": host,
        "cargo_target_dir": cargo::ACCEPT_TARGET_DIR,
        "problems": problems,
    });
    if let Value::Object(fields) = &mut manifest {
        fields.extend(record);
    }
    Redactor::new(&ws).apply_value(&mut manifest);
    let path = out_dir.join(file);
    let text = serde_json::to_string_pretty(&manifest).unwrap_or_default() + "\n";
    let exit_code = match fs::write(&path, text) {
        Ok(()) => exit_code,
        Err(e) => {
            println!("CI problem: cannot write {}: {e}", path.display());
            1
        }
    };
    println!(
        "CI RESULT {result}: {}{} with {} problems; record {}; exit code {exit_code}",
        job.as_str(),
        job.what()
            .map(|what| format!(" ({what})"))
            .unwrap_or_default(),
        manifest["problems"].as_array().map_or(0, Vec::len),
        ws.rel(&path)
    );
    ExitCode::from(exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(str::to_owned).collect()
    }

    #[test]
    fn result_lines_give_counts() {
        assert_eq!(
            result_counts(
                "test result: ok. 3 passed; 0 failed; 1 ignored; 0 measured; 2 filtered out; finished in 0.01s"
            ),
            Some(Counts {
                passed: 3,
                failed: 0,
                ignored: 1
            })
        );
        // wasm-bindgen-test's line has no `measured`.
        assert_eq!(
            result_counts(
                "test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 filtered out; finished in 0.01s"
            ),
            Some(Counts {
                passed: 0,
                failed: 1,
                ignored: 0
            })
        );
        assert_eq!(result_counts("test result: ok."), None);
        assert_eq!(result_counts("running 1 test"), None);
    }

    #[test]
    fn status_lines_open_blocks() {
        assert_eq!(
            running_executable(
                r"     Running unittests src\lib.rs (target/xtask-accept\debug\deps\calc_value-a1e2e6d0ba07c133.exe)"
            ),
            Some("calc_value-a1e2e6d0ba07c133.exe".to_owned())
        );
        assert_eq!(
            running_executable(
                "     Running tests/probe.rs (target/xtask-accept/wasm32-unknown-unknown/debug/deps/probe-c446e89b0647a142.wasm)"
            ),
            Some("probe-c446e89b0647a142.wasm".to_owned())
        );
        // The runner's banner starts at the margin and names no executable.
        assert_eq!(
            running_executable("Running headless tests in Chrome on `http://127.0.0.1:1/`"),
            None
        );
        assert_eq!(
            doc_tests_crate("   Doc-tests calc_value"),
            Some("calc_value".to_owned())
        );
        assert_eq!(doc_tests_crate("Doc-tests calc_value"), None);
    }

    const NATIVE: &str = r#"{"reason":"compiler-artifact","package_id":"path+file:///w/crates/a#0.1.0","target":{"kind":["lib"],"name":"a"},"profile":{"test":true},"executable":"C:\\w\\target\\debug\\deps\\a-1111.exe","filenames":["C:\\w\\target\\debug\\deps\\a-1111.exe"],"fresh":false}
{"reason":"compiler-artifact","package_id":"path+file:///w/crates/b#0.1.0","target":{"kind":["lib"],"name":"b"},"profile":{"test":true},"executable":"C:\\w\\target\\debug\\deps\\b-2222.exe","filenames":[],"fresh":false}
{"reason":"compiler-artifact","package_id":"path+file:///w/crates/b#0.1.0","target":{"kind":["test"],"name":"other"},"profile":{"test":true},"executable":"C:\\w\\target\\debug\\deps\\other-3333.exe","filenames":[],"fresh":false}
{"reason":"build-finished","success":true}
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.82s
     Running unittests src\lib.rs (target\debug\deps\a-1111.exe)

running 2 tests
test one ... ok
test two ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src\lib.rs (target\debug\deps\b-2222.exe)

running 1 test
test bad ... FAILED

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p b --lib`
error: 1 target failed:
    `-p b --lib`
"#;

    #[test]
    fn transcripts_attribute_each_block_to_its_executable() {
        let transcript = parse_transcript(NATIVE);
        assert_eq!(transcript.build_finished, Some(true));
        assert_eq!(transcript.artifacts.len(), 3);
        assert_eq!(
            transcript
                .blocks
                .iter()
                .map(|block| block.header.clone())
                .collect::<Vec<_>>(),
            [
                Header::Executable("a-1111.exe".to_owned()),
                Header::Executable("b-2222.exe".to_owned())
            ]
        );
        let a = evaluate(&transcript.blocks[0].output, None);
        assert_eq!((a.outcome, a.counts.passed), (Outcome::Passed, 2));
        let b = evaluate(&transcript.blocks[1].output, None);
        assert_eq!((b.outcome, b.counts.failed), (Outcome::Failed, 1));
        // The `error: 1 target failed` summary after the last block is not the block's failure.
        assert!(b.reason.unwrap().contains("to rerun pass `-p b --lib`"));
    }

    #[test]
    fn an_executable_without_its_block_did_not_run() {
        // Package b's `other` test target was built but has no Running block: b failed, and its
        // harness did not run every target.
        let transcript = parse_transcript(NATIVE);
        let run = TestRun {
            label: "tests".to_owned(),
            transcript,
            log: "log".to_owned(),
            exit_code: Some(101),
        };
        let member = |id: &str, name: &str| Member {
            name: name.to_owned(),
            id: id.to_owned(),
            dir: PathBuf::from("/w/crates").join(name),
            portability: Some("portable".to_owned()),
            targets: Vec::new(),
        };
        let a = package_run(
            &member("path+file:///w/crates/a#0.1.0", "a"),
            std::slice::from_ref(&run),
            None,
            None,
        );
        assert_eq!((a.outcome(), a.harness_ran()), (Outcome::Passed, true));
        let b = package_run(
            &member("path+file:///w/crates/b#0.1.0", "b"),
            std::slice::from_ref(&run),
            None,
            None,
        );
        assert_eq!((b.outcome(), b.harness_ran()), (Outcome::Failed, false));
        let other = b.targets.iter().find(|t| t.target == "other").unwrap();
        assert!(!other.ran);
        let c = package_run(
            &member("path+file:///w/crates/c#0.1.0", "c"),
            std::slice::from_ref(&run),
            None,
            None,
        );
        assert_eq!((c.built, c.outcome()), (false, Outcome::Failed));
    }

    #[test]
    fn zero_tests_are_visible_and_never_passed() {
        let native = lines(
            "\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n",
        );
        assert_eq!(evaluate(&native, None).outcome, Outcome::ZeroTests);
        let ignored = lines(
            "running 1 test\ntest x ... ignored\n\ntest result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s\n",
        );
        assert_eq!(evaluate(&ignored, None).outcome, Outcome::ZeroTests);
        let chrome = &BROWSERS[0];
        let empty_module = lines("no tests to run!\n");
        let evaluated = evaluate(&empty_module, Some(chrome));
        assert_eq!(
            (
                evaluated.outcome,
                evaluated.no_tests,
                evaluated.banner.is_none()
            ),
            (Outcome::ZeroTests, true, true)
        );
        // Natively, a target with no result line failed, whatever else it printed.
        assert_eq!(evaluate(&empty_module, None).outcome, Outcome::Failed);
        assert_eq!(
            evaluate(&lines("running 1 test\n"), None).outcome,
            Outcome::Failed
        );
    }

    #[test]
    fn a_browser_run_counts_only_under_its_banner() {
        let (chrome, firefox) = (&BROWSERS[0], &BROWSERS[1]);
        let in_chrome = lines(
            "Running headless tests in Chrome on `http://127.0.0.1:1/`\nTry find `webdriver.json` for configure browser's capabilities:\nNot found\n\nrunning 1 test\ntest t ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out; finished in 0.01s\n",
        );
        let evaluated = evaluate(&in_chrome, Some(chrome));
        assert_eq!(evaluated.outcome, Outcome::Passed);
        assert_eq!(evaluated.banner.unwrap().0, "Chrome");
        let wrong = evaluate(&in_chrome, Some(firefox));
        assert_eq!(wrong.outcome, Outcome::Failed);
        assert!(wrong.reason.unwrap().contains("not Firefox"));
        let in_node = lines(
            "running 1 test\ntest t ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out; finished in 0.01s\n",
        );
        let node = evaluate(&in_node, Some(chrome));
        assert_eq!(node.outcome, Outcome::Failed);
        assert!(node.reason.unwrap().contains("not browser evidence"));
        // Natively the same lines are an ordinary pass.
        assert_eq!(evaluate(&in_node, None).outcome, Outcome::Passed);
    }

    #[test]
    fn acceptance_targets_are_left_to_the_runner() {
        let target = |name: &str, kind: &str| cargo::Target {
            name: name.to_owned(),
            kinds: vec![kind.to_owned()],
            src_path: PathBuf::new(),
        };
        let member = Member {
            name: "p".to_owned(),
            id: "p".to_owned(),
            dir: PathBuf::from("/w/crates/p"),
            portability: Some("portable".to_owned()),
            targets: vec![
                target("p", "lib"),
                target("acceptance", "test"),
                target("matrix", "test"),
                target("node", "test"),
            ],
        };
        let excluded = BTreeSet::from(["acceptance".to_owned(), "node".to_owned()]);
        assert_eq!(
            target_selection(&[&member], &excluded),
            ["--lib", "--bins", "--test", "matrix"]
        );
        assert_eq!(
            excluded_json(&member, &excluded).as_array().unwrap().len(),
            2
        );
    }

    #[test]
    fn the_pinned_channel_is_read() {
        let dir = std::env::temp_dir().join(format!("xtask-ci-pin-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(TOOLCHAIN_FILE),
            "# a comment\n[toolchain]\nchannel = \"1.94.1\" # pinned\ncomponents = [\"rustfmt\"]\n",
        )
        .unwrap();
        assert_eq!(pinned_channel(&dir), Ok("1.94.1".to_owned()));
        fs::write(dir.join(TOOLCHAIN_FILE), "[toolchain]\n").unwrap();
        assert!(pinned_channel(&dir).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn agents_are_read_from_the_fixture_lines() {
        let output = lines(
            "Invoking test: route_probe\nCI-BROWSER-AGENT route_probe: Mozilla/5.0 HeadlessChrome/155.0.0.0 Safari/537.36\ntest route_probe ... ok\n",
        );
        assert_eq!(
            agent_of(&output, "route_probe").as_deref(),
            Some("Mozilla/5.0 HeadlessChrome/155.0.0.0 Safari/537.36")
        );
        assert_eq!(agent_of(&output, "other"), None);
    }

    #[test]
    fn jobs_parse() {
        assert_eq!(Job::parse("wasm"), Some(Job::Wasm));
        assert_eq!(Job::parse("desktop").and_then(Job::platform), None);
        assert_eq!(Job::parse("macos"), None);
        // The desktop job says what it is: a release build, no package, and who owns the smoke.
        let what = Job::Desktop.what().unwrap();
        assert!(what.starts_with("a release build of the native-only packages"));
        assert!(what.contains("nothing is packaged") && what.contains("dc-x704"));
        assert_eq!(Job::Wasm.what(), None);
    }

    #[test]
    fn status_entries_keep_their_paths() {
        // A leading space is part of the status code; a rename's source entry is skipped.
        let status = " M README.md\0?? new dir/file.txt\0R  to.rs\0from.rs\0";
        assert_eq!(
            parse_status(status),
            [
                ("M".to_owned(), "README.md".to_owned()),
                ("??".to_owned(), "new dir/file.txt".to_owned()),
                ("R".to_owned(), "to.rs".to_owned())
            ]
        );
        assert!(parse_status("").is_empty());
    }
}
