//! The bodies of the infra-m0.2 acceptance cases (the `#[test]` functions sit at the root of
//! `tests/acceptance.rs`, so libtest lists them under their registered names).
//!
//! Both cases drive the xtask binary this test build compiled (`CARGO_BIN_EXE_xtask`), the binary
//! `cargo xtask` runs, as `ci <job>` and `ci-audit ...`, the commands the CI jobs run, against a
//! copy of this checkout's working tree built as the infra-m0.3 cases build theirs
//! (`acceptance/infra_m0_3.rs`), with two added portable fixture packages from
//! `tests/fixtures/ci_matrix/`. `xtask-ci-matrix-fixture` at `tools/ci-matrix-fixture`: its
//! `matrix` test target holds one native test, which writes a marker file named after a nonce the
//! case compiles into it, and one wasm test, which prints that nonce and the user agent of the
//! browser that executed it; its `acceptance` target holds a test that fails, and leaves a marker,
//! if a CI package run ever runs it. `xtask-ci-zero-fixture` at `tools/ci-zero-fixture` holds no
//! test at all. Both cases share one copy, so its `target/` is reused.
//!
//! What is compared, independently of the CI manifest's own flags: the packages and app cores
//! `cargo metadata` reports in the copy; the kept cargo transcripts, re-read here (each package's
//! `Running` and `Doc-tests` blocks, their libtest result lines, the runner's banner or its `no
//! tests to run!` line, and the user agents the tests printed from inside the browsers); the
//! marker files; the copy's own `git status`; and the workflow file `.github/workflows/ci.yml`,
//! parsed here.
//!
//! What the packages show, and what the cases say about it: each is built and its test harness
//! runs; a package whose tests ran is `passed`, and one that ran 0 tests is `zero_tests`, never
//! passed. On wasm the product packages' tests run in Chrome and Firefox under the runner's banner;
//! for the zero-test fixture wasm-bindgen-test-runner reports `no tests to run!` and starts no
//! browser, which is the package-browser pair the audit must report as such (no product package is
//! a zero-test package any more). `platform_matrix` prints what it observed as `ACCEPT-NOTE` lines,
//! so the runner's receipt carries it.
//!
//! Without wasm-bindgen-test-runner on PATH and both drivers (each named by its variable or on
//! PATH), neither case can run its wasm half: each prints `ACCEPT-NOT-RUN` with the reason.
//! Evidence goes to `target/accept/infra-m0.2/<case>/` with a `summary.txt`. No evidence file is
//! named `executed-targets.json`, so an audit of `target/accept` never reads one as a job's
//! manifest.

use super::infra_m0_3::{Evidence, Fixture, WorkspaceCopy};
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const BEAD: &str = "infra-m0.2";
const PACKAGE: &str = "xtask-ci-matrix-fixture";
/// The fixture package without any test.
const ZERO_PACKAGE: &str = "xtask-ci-zero-fixture";
const COPY: &str = "infra_m0_2_ci";
const BROWSER_FIXTURE: &str = "dnacalc-ci-browser-fixture";

static MATRIX_FIXTURE: Fixture = Fixture {
    package: PACKAGE,
    dir: "tools/ci-matrix-fixture",
    source: "ci_matrix",
    files: &[
        ("package.toml", "Cargo.toml"),
        ("lib.rs", "src/lib.rs"),
        ("matrix.rs", "tests/matrix.rs"),
        ("acceptance.rs", "tests/acceptance.rs"),
        ("acceptance.json", "acceptance.json"),
    ],
};

/// A portable package with no test, natively or on wasm: the zero-test package-browser pairs.
static ZERO_FIXTURE: Fixture = Fixture {
    package: ZERO_PACKAGE,
    dir: "tools/ci-zero-fixture",
    source: "ci_matrix",
    files: &[
        ("zero.toml", "Cargo.toml"),
        ("zero_lib.rs", "src/lib.rs"),
        ("zero_acceptance.json", "acceptance.json"),
    ],
};

/// The variables no `ci` or `ci-audit` run in the copy inherits: the CI inputs (a nested run must
/// not run accept, inject a failure or expect the real run's commit), the target directory
/// settings and the forced browser, which only the route sets.
const REMOVED: [&str; 6] = [
    "DNACALC_CI_ACCEPT",
    "DNACALC_CI_INJECT_BROWSER_FAILURE",
    "DNACALC_CI_COMMIT",
    "CARGO_TARGET_DIR",
    "CARGO_BUILD_TARGET_DIR",
    "WASM_BINDGEN_USE_BROWSER",
];

const BANNER_PREFIX: &str = "Running headless tests in ";

/// A browser as these cases check it: its name in the manifest, its driver's variable and
/// program, the name the runner's banner gives it, and the token of the user agent it reports.
struct Browser {
    name: &'static str,
    variable: &'static str,
    program: &'static str,
    banner: &'static str,
    agent: &'static str,
}

const BROWSERS: [Browser; 2] = [
    Browser {
        name: "chromium",
        variable: "CHROMEDRIVER",
        program: "chromedriver",
        banner: "Chrome",
        agent: "HeadlessChrome/",
    },
    Browser {
        name: "firefox",
        variable: "GECKODRIVER",
        program: "geckodriver",
        banner: "Firefox",
        agent: "Firefox/",
    },
];

/// Checks that `case` is registered under infra-m0.2 in the xtask manifest for windows and linux.
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
    assert_eq!(
        platforms,
        ["windows", "linux"],
        "{case} must declare windows and linux"
    );
}

/// Where PATH holds `program`, looked up as the runner does.
fn on_path(program: &str) -> Option<PathBuf> {
    let names = if cfg!(windows) {
        vec![format!("{program}.exe"), program.to_owned()]
    } else {
        vec![program.to_owned()]
    };
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|path| path.is_file())
}

/// Why this host cannot run the browser half, or `None` when it has the runner and both drivers.
fn browsers_missing() -> Option<String> {
    let mut missing = Vec::new();
    if on_path("wasm-bindgen-test-runner").is_none() {
        missing.push("wasm-bindgen-test-runner is not on PATH".to_owned());
    }
    for browser in &BROWSERS {
        let named = std::env::var_os(browser.variable)
            .map(PathBuf::from)
            .is_some_and(|path| path.is_file());
        if !named && on_path(browser.program).is_none() {
            missing.push(format!(
                "{} is not on PATH and {} names no file",
                browser.program, browser.variable
            ));
        }
    }
    (!missing.is_empty()).then(|| missing.join("; "))
}

/// A nonce for this run, compiled into the fixture's tests.
fn nonce() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    format!("n{nanos:x}")
}

/// The shared copy, and the variables every run in it sets: the nonce and the marker directory.
struct Matrix {
    copy: WorkspaceCopy,
    nonce: String,
    markers: PathBuf,
}

impl Matrix {
    fn new(evidence: &mut Evidence) -> Matrix {
        let copy = WorkspaceCopy::with_fixtures(COPY, &MATRIX_FIXTURE, &[&ZERO_FIXTURE], evidence);
        let markers = copy.base.join("ci-matrix-markers");
        super::remove_tree(&markers).unwrap_or_else(|e| panic!("{e}"));
        let nonce = nonce();
        evidence.lines.push(format!(
            "nonce {nonce} compiled into the fixture's tests; markers in {}",
            markers.display()
        ));
        Matrix {
            copy,
            nonce,
            markers,
        }
    }

    fn root(&self) -> PathBuf {
        self.copy.path("")
    }

    /// Runs the xtask binary with `args` in the copy, keeping its transcript as `<label>.log`.
    fn xtask(
        &self,
        args: &[&str],
        set: &[(&str, &str)],
        label: &str,
        evidence: &mut Evidence,
    ) -> (Option<i32>, String) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
        command
            .args(args)
            .current_dir(self.root())
            .env("CI_MATRIX_NONCE", &self.nonce)
            .env("CI_MATRIX_MARKERS", &self.markers);
        for name in REMOVED {
            command.env_remove(name);
        }
        for (name, value) in set {
            command.env(name, value);
        }
        let shown = format!("{command:?}");
        let output = command
            .output()
            .unwrap_or_else(|e| panic!("cannot start {shown}: {e}"));
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr);
        super::write(
            &evidence.dir.join(format!("{label}.log")),
            format!(
                "{shown}\nexit code: {:?}\n--- stdout\n{stdout}\n--- stderr\n{stderr}",
                output.status.code()
            ),
        );
        println!("{label}: exit code {:?}", output.status.code());
        (output.status.code(), stdout)
    }

    /// The manifest a `ci <job>` run wrote in the copy, kept beside the evidence.
    fn manifest(&self, job: &str, label: &str, evidence: &mut Evidence) -> Value {
        let path = self
            .root()
            .join("target")
            .join("ci")
            .join(job)
            .join("executed-targets.json");
        let text = fs::read_to_string(&path).unwrap_or_default();
        super::write(&evidence.dir.join(format!("{label}.json")), &text);
        serde_json::from_str(&text).unwrap_or(Value::Null)
    }

    /// A file the copy's run wrote, by the path a manifest gives relative to the copy's root.
    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.root().join(rel)).unwrap_or_default()
    }

    fn head(&self) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(self.root())
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("git rev-parse in the copy");
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    /// The copy's `git status --porcelain=v1` lines, untracked files included.
    fn status(&self) -> Vec<String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(self.root())
            .args(["status", "--porcelain=v1", "--untracked-files=all"])
            .output()
            .expect("git status in the copy");
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

/// Prints `text` as an `ACCEPT-NOTE` of `case`, which the runner keeps in the receipt, and keeps it
/// among the evidence.
fn note(case: &str, text: String, evidence: &mut Evidence) {
    println!("ACCEPT-NOTE: {case}: {text}");
    evidence.lines.push(format!("note: {text}"));
}

/// One workspace member as the copy's `cargo metadata --no-deps` reports it.
struct Member {
    name: String,
    dir: String,
    portable: bool,
    lib_crate: Option<String>,
    tests: Vec<String>,
    /// Binary targets, by cargo's name (an app's browser entry is one).
    bins: Vec<String>,
}

/// The copy's members, read with its own `cargo metadata`.
fn members(matrix: &Matrix, evidence: &mut Evidence) -> Vec<Member> {
    let (code, stdout, stderr) = matrix.copy.cargo_with(
        &["metadata", "--no-deps", "--format-version", "1", "--locked"],
        &[],
        "00-cargo-metadata",
        evidence,
    );
    assert_eq!(code, Some(0), "cargo metadata in the copy failed: {stderr}");
    let metadata: Value = serde_json::from_str(&stdout).expect("cargo metadata prints JSON");
    let root = metadata["workspace_root"]
        .as_str()
        .expect("a workspace root")
        .replace('\\', "/");
    let ids = super::strings(&metadata["workspace_members"], "workspace members");
    metadata["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .filter(|package| ids.iter().any(|id| package["id"] == id.as_str()))
        .map(|package| {
            let manifest = package["manifest_path"]
                .as_str()
                .expect("a manifest path")
                .replace('\\', "/");
            let dir = manifest
                .strip_prefix(&format!("{root}/"))
                .and_then(|rest| rest.strip_suffix("/Cargo.toml"))
                .unwrap_or_default()
                .to_owned();
            let targets = package["targets"].as_array().cloned().unwrap_or_default();
            let kind_is = |target: &Value, kind: &str| {
                target["kind"]
                    .as_array()
                    .is_some_and(|kinds| kinds.iter().any(|k| k == kind))
            };
            Member {
                name: package["name"].as_str().unwrap_or_default().to_owned(),
                dir,
                portable: package["metadata"]["dnacalc"]["portability"] == "portable",
                lib_crate: targets
                    .iter()
                    .find(|target| kind_is(target, "lib"))
                    .and_then(|target| target["name"].as_str())
                    .map(|name| name.replace('-', "_")),
                tests: targets
                    .iter()
                    .filter(|target| kind_is(target, "test"))
                    .filter_map(|target| target["name"].as_str().map(str::to_owned))
                    .collect(),
                bins: targets
                    .iter()
                    .filter(|target| kind_is(target, "bin"))
                    .filter_map(|target| target["name"].as_str().map(str::to_owned))
                    .collect(),
            }
        })
        .collect()
}

/// Whether `line` is a cargo status line opening a block: `Running <src> (<exe>)` or
/// `Doc-tests <crate>`, indented as cargo indents its status lines.
fn opens_block(line: &str) -> bool {
    let trimmed = line.trim();
    line.starts_with(' ')
        && ((trimmed.starts_with("Running ") && trimmed.ends_with(')'))
            || trimmed.starts_with("Doc-tests "))
}

/// The lines after the first block-opening line that `header` accepts, up to the next one.
fn block(log: &str, header: impl Fn(&str) -> bool) -> Option<Vec<&str>> {
    let mut lines = log.lines();
    lines
        .by_ref()
        .find(|line| opens_block(line) && header(line.trim()))?;
    Some(lines.take_while(|line| !opens_block(line)).collect())
}

/// The block of the executable whose file name starts with `<stem>-`.
fn executable_block<'a>(log: &'a str, stem: &str) -> Option<Vec<&'a str>> {
    block(log, |line| {
        line.strip_prefix("Running ")
            .and_then(|rest| rest.strip_suffix(')'))
            .and_then(|rest| rest.rsplit_once(" ("))
            .map(|(_, path)| path.rsplit(['/', '\\']).next().unwrap_or(path))
            .is_some_and(|file| file.starts_with(&format!("{stem}-")))
    })
}

fn doc_block<'a>(log: &'a str, krate: &str) -> Option<Vec<&'a str>> {
    block(log, |line| line == format!("Doc-tests {krate}"))
}

fn result_lines<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    lines
        .iter()
        .map(|line| line.trim())
        .filter(|line| line.starts_with("test result: "))
        .collect()
}

/// The `passed` count of a libtest result line.
fn passed_of(line: &str) -> Option<u64> {
    line.split(';').find_map(|part| {
        let mut words = part.split_whitespace().rev();
        (words.next() == Some("passed"))
            .then(|| words.next().and_then(|n| n.parse().ok()))
            .flatten()
    })
}

fn first_line<'a>(lines: &[&'a str]) -> &'a str {
    lines
        .iter()
        .map(|line| line.trim())
        .find(|line| !line.is_empty())
        .unwrap_or_default()
}

fn entry<'a>(manifest: &'a Value, package: &str) -> &'a Value {
    manifest["packages"]
        .as_array()
        .and_then(|entries| entries.iter().find(|entry| entry["package"] == package))
        .unwrap_or(&Value::Null)
}

fn browser_run<'a>(entry: &'a Value, browser: &str) -> &'a Value {
    entry["browsers"]
        .as_array()
        .and_then(|runs| runs.iter().find(|run| run["browser"] == browser))
        .unwrap_or(&Value::Null)
}

fn fixture_run<'a>(manifest: &'a Value, fixture: &str, browser: &str) -> &'a Value {
    manifest["fixtures"]
        .as_array()
        .and_then(|runs| {
            runs.iter()
                .find(|run| run["fixture"] == fixture && run["browser"] == browser)
        })
        .unwrap_or(&Value::Null)
}

/// The log a manifest's run with `label` was kept in.
fn run_log<'a>(manifest: &'a Value, label: &str) -> &'a str {
    manifest["runs"]
        .as_array()
        .and_then(|runs| runs.iter().find(|run| run["label"] == label))
        .and_then(|run| run["log"].as_str())
        .unwrap_or_default()
}

fn not_run(case: &str, reason: &str, evidence: &mut Evidence) {
    println!("ACCEPT-NOT-RUN: {case}: {reason}");
    evidence.lines.push(format!("not run: {reason}"));
}

/// `cargo xtask ci <host>` and `cargo xtask ci wasm` in the copy record every portable package and
/// both app cores as built, with the test harness run on the host target and under
/// wasm-bindgen-test-runner for Chrome and Firefox, recording a zero-test run as `zero_tests` and a
/// module for which the runner reported no tests as starting no browser; `cargo xtask ci-audit`
/// accepts exactly that and refuses a missing target, package, browser run or route probe, a
/// manifest made from a dirty working tree (unless `--allow-dirty`), and never reads a manifest kept
/// outside a job directory.
pub fn platform_matrix() {
    const CASE: &str = "infra_m0_2__platform_matrix";
    let _serial = super::serial();
    registered(CASE);
    let mut evidence = Evidence::for_bead(BEAD, CASE);
    let (host, other) = match std::env::consts::OS {
        "windows" => ("windows", "linux"),
        "linux" => ("linux", "windows"),
        os => panic!("{CASE} runs on Windows and Linux hosts, not {os}"),
    };
    if let Some(reason) = browsers_missing() {
        not_run(
            CASE,
            &format!("this host cannot run the wasm half in Chrome and Firefox: {reason}"),
            &mut evidence,
        );
        evidence.finish();
        return;
    }
    let matrix = Matrix::new(&mut evidence);
    let members = members(&matrix, &mut evidence);
    let portable: Vec<&Member> = members.iter().filter(|m| m.portable).collect();
    let app_cores: BTreeSet<&str> = members
        .iter()
        .filter(|m| m.dir.starts_with("apps/"))
        .map(|m| m.name.as_str())
        .collect();
    // The packages under apps/ are this checkout's: the two application cores and the packages
    // they share.
    let packages = super::workspace_packages();
    let expected_cores: BTreeSet<&str> = packages
        .iter()
        .filter(|(_, dir)| dir.starts_with("apps/"))
        .map(|(name, _)| name.as_str())
        .collect();
    evidence.expect(
        app_cores == expected_cores
            && ["dna-calc", "dna-onecalc"]
                .iter()
                .all(|core| app_cores.contains(core))
            && app_cores
                .iter()
                .all(|core| portable.iter().any(|m| m.name == *core))
            && [PACKAGE, ZERO_PACKAGE]
                .iter()
                .all(|fixture| portable.iter().any(|m| m.name == *fixture)),
        format!(
            "cargo metadata in the copy: {} portable packages ({:?}), the packages {app_cores:?} under apps/ (expected {expected_cores:?}, dna-calc and dna-onecalc among them), all portable, and both fixtures among them",
            portable.len(),
            portable.iter().map(|m| m.name.as_str()).collect::<Vec<_>>()
        ),
    );
    let head = matrix.head();

    // The host target.
    let (code, stdout) = matrix.xtask(&["ci", host], &[], "01-ci-host", &mut evidence);
    let native = matrix.manifest(host, "01-ci-host.manifest", &mut evidence);
    let copy_status = matrix.status();
    evidence.expect(
        code == Some(0)
            && native["target"] == host
            && native["result"] == "passed"
            && native["exit_code"] == 0
            && native["host"]["git"]["head"] == head.as_str()
            && copy_status.is_empty()
            && native["host"]["git"]["dirty"] == false
            && native["host"]["git"]["dirty_paths"] == Value::Array(Vec::new())
            && stdout.lines().any(|line| line.starts_with("CI RESULT passed: ")),
        format!(
            "01-ci-host: `ci {host}` exits {code:?}, its manifest names target {} with result {} at the copy's commit {head} (manifest {}) and a clean tree (dirty {}, dirty paths {}; the copy's git status lists {copy_status:?}): {}",
            native["target"],
            native["result"],
            native["host"]["git"]["head"],
            native["host"]["git"]["dirty"],
            native["host"]["git"]["dirty_paths"],
            native["problems"]
        ),
    );
    let tests_log = matrix.read(run_log(&native, "tests"));
    let doc_log = matrix.read(run_log(&native, "doc-tests"));
    let (mut native_zero, mut native_tested, mut native_unran) =
        (Vec::new(), Vec::new(), Vec::new());
    for member in &portable {
        let record = entry(&native, &member.name);
        let krate = member.lib_crate.clone().unwrap_or_default();
        let mut blocks: Vec<(String, Option<Vec<&str>>)> = vec![
            (format!("lib {krate}"), executable_block(&tests_log, &krate)),
            (format!("doc {krate}"), doc_block(&doc_log, &krate)),
        ];
        for test in member.tests.iter().filter(|test| *test != "acceptance") {
            blocks.push((format!("test {test}"), executable_block(&tests_log, test)));
        }
        let mut recount = 0;
        let mut each_ran = true;
        for (_, lines) in &blocks {
            let results = lines.as_deref().map(result_lines).unwrap_or_default();
            match results.as_slice() {
                [line] => recount += passed_of(line).unwrap_or_default(),
                _ => each_ran = false,
            }
        }
        let expected_status = if recount > 0 { "passed" } else { "zero_tests" };
        if !each_ran {
            native_unran.push(member.name.clone());
        } else if recount == 0 {
            native_zero.push(member.name.clone());
        } else {
            native_tested.push(format!("{} ({recount} passed)", member.name));
        }
        evidence.expect(
            each_ran
                && record["built"] == true
                && record["harness_ran"] == true
                && record["passed"] == recount
                && record["status"] == expected_status
                && (!app_cores.contains(member.name.as_str()) || record["role"] == "app-core"),
            format!(
                "01-ci-host {}: built, and the kept transcripts hold one libtest result line in each of {:?}, which pass {recount} tests; the manifest records built {}, harness ran {}, {} passed, status {} (expected {expected_status}; zero_tests is never a pass), role {}",
                member.name,
                blocks.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(),
                record["built"],
                record["harness_ran"],
                record["passed"],
                record["status"],
                record["role"]
            ),
        );
    }
    let fixture = entry(&native, PACKAGE);
    let marker = matrix
        .markers
        .join(format!("matrix-native-{}", matrix.nonce));
    let excluded: Vec<&str> = fixture["excluded_targets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|target| target["target"].as_str())
        .collect();
    evidence.expect(
        marker.is_file()
            && tests_log.contains(&format!("CI-MATRIX native {}", matrix.nonce))
            && fixture["status"] == "passed"
            && fixture["passed"] == 1,
        format!(
            "01-ci-host fixture: the native test compiled with nonce {} wrote {} and printed its line, and the manifest counts it: {} passed, status {}",
            matrix.nonce,
            marker.display(),
            fixture["passed"],
            fixture["status"]
        ),
    );
    evidence.expect(
        !matrix.markers.join("acceptance-ran").exists()
            && excluded == ["acceptance"]
            && executable_block(&tests_log, "acceptance").is_none(),
        format!(
            "01-ci-host fixture: the acceptance target, whose test fails and leaves a marker when run, did not run (no marker, no Running block) and is listed excluded: {excluded:?}"
        ),
    );
    evidence.lines.push(format!(
        "01-ci-host statuses: {}",
        portable
            .iter()
            .map(|m| format!("{} {}", m.name, entry(&native, &m.name)["status"]))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    note(
        CASE,
        format!(
            "{host}, observed in the kept transcripts: {}; {} ({} packages) ran 0 tests, recorded zero_tests, not passed; tests ran only for {}",
            if native_unran.is_empty() {
                "every portable package was built and its test harness ran each of its targets"
                    .to_owned()
            } else {
                format!(
                    "the test harness did not run every target of {}",
                    native_unran.join(", ")
                )
            },
            native_zero.join(", "),
            native_zero.len(),
            if native_tested.is_empty() {
                "no package".to_owned()
            } else {
                native_tested.join(", ")
            }
        ),
        &mut evidence,
    );

    // wasm, in Chrome and Firefox.
    let (code, stdout) = matrix.xtask(&["ci", "wasm"], &[], "02-ci-wasm", &mut evidence);
    let wasm = matrix.manifest("wasm", "02-ci-wasm.manifest", &mut evidence);
    evidence.expect(
        code == Some(0)
            && wasm["target"] == "wasm"
            && wasm["result"] == "passed"
            && wasm["inject_browser_failure"] == false
            && wasm["host"]["git"]["head"] == head.as_str()
            && wasm["host"]["git"]["dirty"] == false
            && stdout
                .lines()
                .any(|line| line.starts_with("CI RESULT passed: ")),
        format!(
            "02-ci-wasm: `ci wasm` exits {code:?} with result {} at the copy's commit from a clean tree (dirty {}): {}",
            wasm["result"], wasm["host"]["git"]["dirty"], wasm["problems"]
        ),
    );
    // (browser, package) for each package whose transcript shows no browser started.
    let mut wasm_no_browser: Vec<(&str, String)> = Vec::new();
    for browser in &BROWSERS {
        let log = matrix.read(run_log(&wasm, &format!("tests@{}", browser.name)));
        let banner = format!("{BANNER_PREFIX}{} on ", browser.banner);
        let (mut no_browser, mut in_browser, mut unread) = (Vec::new(), Vec::new(), Vec::new());
        for member in &portable {
            let record = browser_run(entry(&wasm, &member.name), browser.name);
            let krate = member.lib_crate.clone().unwrap_or_default();
            let mut modules = vec![(krate.clone(), executable_block(&log, &krate))];
            for test in member.tests.iter().filter(|test| *test != "acceptance") {
                modules.push((test.clone(), executable_block(&log, test)));
            }
            // A binary target's executable is named with underscores; the manifest names it as
            // cargo does.
            for bin in &member.bins {
                modules.push((bin.clone(), executable_block(&log, &bin.replace('-', "_"))));
            }
            // Each module either holds no wasm-bindgen test (the runner prints `no tests to run!`,
            // no banner and no result line, and starts no browser) or ran under the runner's banner
            // naming this browser, with one result line.
            let (mut recount, mut each_ran) = (0, true);
            let (mut no_tests, mut ran_in_browser) = (BTreeSet::new(), Vec::new());
            for (module, lines) in &modules {
                let lines = lines.clone().unwrap_or_default();
                let results = result_lines(&lines);
                let printed_no_tests = lines.iter().any(|line| line.trim() == "no tests to run!");
                let any_banner = lines
                    .iter()
                    .any(|line| line.trim_start().starts_with(BANNER_PREFIX));
                if printed_no_tests && results.is_empty() && !any_banner {
                    no_tests.insert(module.clone());
                    continue;
                }
                match results.as_slice() {
                    [line] if first_line(&lines).starts_with(&banner) => {
                        let passed = passed_of(line).unwrap_or_default();
                        recount += passed;
                        ran_in_browser.push(format!("{module} ({passed} passed)"));
                    }
                    _ => each_ran = false,
                }
            }
            let started = !ran_in_browser.is_empty();
            let recorded_no_tests: BTreeSet<String> = record["runner_reported_no_tests_for"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|target| target.as_str().map(str::to_owned))
                .collect();
            let mut observed = Vec::new();
            if !no_tests.is_empty() {
                observed.push(format!(
                    "for {no_tests:?} wasm-bindgen-test-runner printed `no tests to run!`, no banner and no result line: it reported no tests and started no browser"
                ));
            }
            if started {
                observed.push(format!(
                    "{ran_in_browser:?} ran in {} under the runner's banner",
                    browser.banner
                ));
            }
            if !each_ran {
                observed.push(
                    "a module neither reported no tests nor ran under this browser's banner with one result line".to_owned(),
                );
            }
            let expected_status = if recount > 0 { "passed" } else { "zero_tests" };
            evidence.expect(
                each_ran
                    && record["built"] == true
                    && record["harness_ran"] == true
                    && record["passed"] == recount
                    && record["status"] == expected_status
                    && record["browser_started"] == started
                    && recorded_no_tests == no_tests,
                format!(
                    "02-ci-wasm {}@{}: built, and the kept transcript of its modules {:?} shows: {}; the manifest records built {}, harness ran {}, {} passed, status {} (expected {expected_status}; zero_tests is never a pass), browser started {} (expected {started}), runner reported no tests for {:?}",
                    member.name,
                    browser.name,
                    modules.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(),
                    observed.join("; "),
                    record["built"],
                    record["harness_ran"],
                    record["passed"],
                    record["status"],
                    record["browser_started"],
                    recorded_no_tests
                ),
            );
            if !each_ran {
                unread.push(member.name.clone());
            } else if started {
                in_browser.push(format!("{} ({recount} passed)", member.name));
            } else {
                no_browser.push(member.name.clone());
                wasm_no_browser.push((browser.name, member.name.clone()));
            }
        }
        let record = browser_run(entry(&wasm, PACKAGE), browser.name);
        let matrix_block = executable_block(&log, "matrix").unwrap_or_default();
        let marker = format!("CI-MATRIX-AGENT {}: ", matrix.nonce);
        let agent = matrix_block
            .iter()
            .find_map(|line| {
                line.find(&marker)
                    .map(|at| line[at + marker.len()..].trim())
            })
            .unwrap_or_default();
        evidence.expect(
            first_line(&matrix_block).starts_with(&banner)
                && agent.contains(browser.agent)
                && record["passed"] == 1
                && record["status"] == "passed"
                && record["browser_started"] == true,
            format!(
                "02-ci-wasm fixture@{}: the matrix module's transcript starts with the runner's {} banner and its test, compiled with nonce {}, reported user agent {agent:?} from inside the browser; the manifest counts {} passed, status {}",
                browser.name, browser.banner, matrix.nonce, record["passed"], record["status"]
            ),
        );
        let probe = fixture_run(&wasm, "route-probe", browser.name);
        let probe_passed = probe["outcome"] == "passed"
            && probe["executed_in"] == browser.banner
            && probe["user_agent"]
                .as_str()
                .is_some_and(|agent| agent.contains(browser.agent));
        evidence.expect(
            probe_passed,
            format!(
                "02-ci-wasm route-probe@{}: passed in {} with user agent {}",
                browser.name, probe["executed_in"], probe["user_agent"]
            ),
        );
        let mut text = format!(
            "wasm@{}, observed in the kept transcript: for every module of {} ({} packages) wasm-bindgen-test-runner reported `no tests to run!` and started no browser, recorded zero_tests, not passed; tests ran in {} under the runner's banner only for {}{}",
            browser.name,
            no_browser.join(", "),
            no_browser.len(),
            browser.banner,
            if in_browser.is_empty() {
                "no package".to_owned()
            } else {
                in_browser.join(", ")
            },
            if probe_passed {
                " and the route probe (passed)"
            } else {
                ""
            }
        );
        if !unread.is_empty() {
            text.push_str(&format!(
                "; the transcript does not show {} either way",
                unread.join(", ")
            ));
        }
        note(CASE, text, &mut evidence);
    }

    // The audit of both targets.
    let native_dir = format!("target/ci/{host}");
    let required = format!("{host},wasm");
    let (code, stdout) = matrix.xtask(
        &[
            "ci-audit",
            "--require-executed-targets",
            &required,
            "--from",
            &native_dir,
            "--from",
            "target/ci/wasm",
        ],
        &[],
        "03-ci-audit",
        &mut evidence,
    );
    let audit: Value =
        serde_json::from_str(&matrix.read("target/ci-audit/audit.json")).unwrap_or(Value::Null);
    super::write(
        &evidence.dir.join("03-ci-audit.audit.json"),
        serde_json::to_string_pretty(&audit).unwrap_or_default(),
    );
    let audited = |target: &str| -> BTreeSet<String> {
        audit["targets"][target]["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|row| row["present"] == true)
            .filter_map(|row| row["package"].as_str().map(str::to_owned))
            .collect()
    };
    let expected: BTreeSet<String> = portable.iter().map(|m| m.name.clone()).collect();
    evidence.expect(
        code == Some(0)
            && audit["result"] == "passed"
            && audit["allow_dirty"] == false
            && audited(host) == expected
            && audited("wasm") == expected
            && !stdout.lines().any(|line| line.starts_with("AUDIT dirty "))
            && stdout.lines().any(|line| line.starts_with("AUDIT RESULT passed: ")),
        format!(
            "03-ci-audit: `ci-audit --require-executed-targets {required}`, without --allow-dirty, exits {code:?} with result {}, prints no AUDIT dirty line for the clean copy's manifests, and lists every portable package on both targets: {:?}",
            audit["result"],
            audited("wasm")
        ),
    );
    let audit_line = |package: &str| -> String {
        stdout
            .lines()
            .find(|line| line.starts_with(&format!("AUDIT wasm {package} (")))
            .unwrap_or_default()
            .to_owned()
    };
    let unsaid: Vec<String> = wasm_no_browser
        .iter()
        .filter(|(browser, package)| {
            !audit_line(package).split("; ").any(|part| {
                part.contains(&format!("{browser} zero_tests: 0 tests run"))
                    && part.contains("`no tests to run!`")
                    && part.contains("started no browser")
            })
        })
        .map(|(browser, package)| format!("{package}@{browser}"))
        .collect();
    let zero_pairs: Vec<&str> = wasm_no_browser
        .iter()
        .filter(|(_, package)| package == ZERO_PACKAGE)
        .map(|(browser, _)| *browser)
        .collect();
    evidence.expect(
        zero_pairs.len() == BROWSERS.len() && unsaid.is_empty(),
        format!(
            "03-ci-audit: for each of the {} package-browser pairs whose transcript shows no browser started ({ZERO_PACKAGE} in {zero_pairs:?} among them), the AUDIT wasm line says zero_tests with 0 tests run and that wasm-bindgen-test-runner reported `no tests to run!` and started no browser (pairs it does not say this for: {unsaid:?}); e.g. {:?}",
            wasm_no_browser.len(),
            audit_line(ZERO_PACKAGE)
        ),
    );
    let all_three = format!("{host},{other},wasm");
    let (code, stdout) = matrix.xtask(
        &[
            "ci-audit",
            "--require-executed-targets",
            &all_three,
            "--from",
            &native_dir,
            "--from",
            "target/ci/wasm",
        ],
        &[],
        "04-ci-audit-missing-target",
        &mut evidence,
    );
    evidence.expect(
        code == Some(1)
            && stdout
                .lines()
                .any(|line| line.contains(&format!("no executed-target manifest covers {other}"))),
        format!(
            "04-ci-audit-missing-target: requiring {other} as well, with no manifest for it, exits {code:?} and names the missing target"
        ),
    );

    // Altered copies of the wasm manifest, each kept as `<label>.json` (never under the name a
    // job's manifest has) and given to the audit by its path: each must be refused, naming what is
    // missing.
    let tamper = |label: &str, change: &dyn Fn(&mut Value)| -> PathBuf {
        let mut altered = wasm.clone();
        change(&mut altered);
        let file = evidence.dir.join(format!("{label}.json"));
        fs::write(
            &file,
            serde_json::to_string_pretty(&altered).expect("serialise"),
        )
        .expect("write the altered manifest");
        file
    };
    let without_app_core = tamper("05-altered-no-dna-calc", &|manifest| {
        if let Some(entries) = manifest["packages"].as_array_mut() {
            entries.retain(|entry| entry["package"] != "dna-calc");
        }
    });
    let without_firefox = tamper("06-altered-calc-func-not-in-firefox", &|manifest| {
        for entry in manifest["packages"].as_array_mut().into_iter().flatten() {
            if entry["package"] == "calc-func"
                && let Some(runs) = entry["browsers"].as_array_mut()
            {
                runs.retain(|run| run["browser"] != "firefox");
            }
        }
    });
    let without_probe = tamper("07-altered-no-route-probe", &|manifest| {
        manifest["fixtures"] = Value::Array(Vec::new());
    });
    for (label, file, named) in [
        (
            "05-ci-audit-no-dna-calc",
            &without_app_core,
            "wasm: dna-calc is not in the executed-target manifest".to_owned(),
        ),
        (
            "06-ci-audit-calc-func-not-in-firefox",
            &without_firefox,
            "wasm: calc-func has no run in firefox".to_owned(),
        ),
        (
            "07-ci-audit-no-route-probe",
            &without_probe,
            "wasm: the manifest has no route-probe run in chromium".to_owned(),
        ),
    ] {
        let file_text = file.to_string_lossy().into_owned();
        let (code, stdout) = matrix.xtask(
            &[
                "ci-audit",
                "--require-executed-targets",
                &required,
                "--from",
                &native_dir,
                "--from",
                &file_text,
            ],
            &[],
            label,
            &mut evidence,
        );
        evidence.expect(
            code == Some(1)
                && stdout
                    .lines()
                    .any(|line| line.starts_with("AUDIT problem: ") && line.contains(&named)),
            format!("{label}: the audit of the altered manifest exits {code:?} with the problem {named:?}"),
        );
    }

    // A manifest-named file outside a job directory (as case evidence under target/accept would
    // be) is listed as skipped and never read: with the altered manifest that lacks dna-calc there,
    // an audit of the whole target directory still passes on the jobs' own manifests.
    let stray_dir = matrix
        .root()
        .join("target")
        .join("accept")
        .join("stray-evidence");
    fs::create_dir_all(&stray_dir).expect("create the stray manifest's directory");
    fs::copy(&without_app_core, stray_dir.join("executed-targets.json"))
        .expect("place the stray manifest");
    let (code, stdout) = matrix.xtask(
        &[
            "ci-audit",
            "--require-executed-targets",
            &required,
            "--from",
            "target",
        ],
        &[],
        "08-ci-audit-stray-manifest",
        &mut evidence,
    );
    let skipped_line = stdout
        .lines()
        .find(|line| line.starts_with("AUDIT skipped ") && line.contains("stray-evidence"))
        .unwrap_or_default()
        .to_owned();
    let stray_audit: Value =
        serde_json::from_str(&matrix.read("target/ci-audit/audit.json")).unwrap_or(Value::Null);
    let listed = |value: &Value| -> BTreeSet<String> {
        value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect()
    };
    let read_manifests = listed(&stray_audit["sources"][0]["manifests"]);
    let skipped = listed(&stray_audit["sources"][0]["skipped"]);
    evidence.expect(
        code == Some(0)
            && read_manifests
                == BTreeSet::from([
                    format!("target/ci/{host}/executed-targets.json"),
                    "target/ci/wasm/executed-targets.json".to_owned(),
                ])
            && skipped
                == BTreeSet::from(["target/accept/stray-evidence/executed-targets.json".to_owned()])
            && !skipped_line.is_empty(),
        format!(
            "08-ci-audit-stray-manifest: with target/accept/stray-evidence/executed-targets.json (the altered manifest without dna-calc) in the copy, `ci-audit --from target` exits {code:?}; its audit.json lists as read {read_manifests:?} and as skipped {skipped:?}, and it prints {skipped_line:?}"
        ),
    );
    super::remove_tree(&stray_dir).unwrap_or_else(|e| panic!("{e}"));

    // A manifest made from a dirty working tree: `ci` records the changed path, which the copy's
    // own git status lists, and the audit refuses it unless --allow-dirty is given.
    let probe_name = "ci-audit-dirty-probe.txt";
    let probe = matrix.root().join(probe_name);
    super::write(
        &probe,
        "an untracked file that makes the copy's working tree dirty\n",
    );
    let status = matrix.status();
    let (code, _) = matrix.xtask(&["ci", host], &[], "09-ci-host-dirty", &mut evidence);
    let dirty = matrix.manifest(host, "09-ci-host-dirty.manifest", &mut evidence);
    let recorded: Vec<String> = dirty["host"]["git"]["dirty_paths"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| {
            format!(
                "{} {}",
                entry["status"].as_str().unwrap_or_default(),
                entry["path"].as_str().unwrap_or_default()
            )
        })
        .collect();
    let expected_entry = format!("?? {probe_name}");
    evidence.expect(
        code == Some(0)
            && status == [expected_entry.clone()]
            && dirty["host"]["git"]["dirty"] == true
            && dirty["host"]["git"]["head"] == head.as_str()
            && recorded == [expected_entry.clone()],
        format!(
            "09-ci-host-dirty: with {probe_name} added, the copy's git status lists {status:?}; `ci {host}` exits {code:?} and its manifest records dirty {} at the same commit with the changed paths {recorded:?}",
            dirty["host"]["git"]["dirty"]
        ),
    );
    let audit_args = [
        "ci-audit",
        "--require-executed-targets",
        &required,
        "--from",
        &native_dir,
        "--from",
        "target/ci/wasm",
    ];
    let (code, stdout) = matrix.xtask(&audit_args, &[], "10-ci-audit-dirty", &mut evidence);
    let dirty_line = stdout
        .lines()
        .find(|line| line.starts_with(&format!("AUDIT dirty {host}: ")))
        .unwrap_or_default()
        .to_owned();
    evidence.expect(
        code == Some(1)
            && dirty_line.contains(&expected_entry)
            && dirty_line.contains("refused without --allow-dirty")
            && stdout.lines().any(|line| {
                line.starts_with(&format!("AUDIT problem: {host}: "))
                    && line.contains("dirty working tree")
            })
            && stdout.lines().any(|line| line.starts_with("AUDIT RESULT failed: ")),
        format!(
            "10-ci-audit-dirty: without --allow-dirty the audit exits {code:?}, prints {dirty_line:?} on a line of its own and fails"
        ),
    );
    let mut allowed_args = audit_args.to_vec();
    allowed_args.push("--allow-dirty");
    let (code, stdout) = matrix.xtask(
        &allowed_args,
        &[],
        "11-ci-audit-dirty-allowed",
        &mut evidence,
    );
    let dirty_line = stdout
        .lines()
        .find(|line| line.starts_with(&format!("AUDIT dirty {host}: ")))
        .unwrap_or_default()
        .to_owned();
    evidence.expect(
        code == Some(0)
            && dirty_line.contains(&expected_entry)
            && dirty_line.contains("accepted because --allow-dirty is given")
            && stdout.lines().any(|line| line.starts_with("AUDIT RESULT passed: ")),
        format!(
            "11-ci-audit-dirty-allowed: with --allow-dirty the same audit exits {code:?} and still prints {dirty_line:?}"
        ),
    );
    fs::remove_file(&probe).unwrap_or_else(|e| panic!("cannot delete {}: {e}", probe.display()));

    evidence
        .lines
        .push(format!("copy kept at {}", matrix.copy.base.display()));
    evidence.finish();
}

/// `cargo xtask ci wasm`, the CI wasm job's one command, fails with a deliberately failing browser
/// test added, naming it and recording it failed in each browser in which it ran; without it the
/// same command passes; and the workflow's wasm job runs exactly that command with nothing that
/// would keep its failure from failing the job.
pub fn browser_failure_is_red() {
    const CASE: &str = "infra_m0_2__browser_failure_is_red";
    let _serial = super::serial();
    registered(CASE);
    let mut evidence = Evidence::for_bead(BEAD, CASE);
    workflow_checks(&mut evidence);
    if let Some(reason) = browsers_missing() {
        not_run(
            CASE,
            &format!("this host cannot run a browser test in Chrome and Firefox: {reason}"),
            &mut evidence,
        );
        evidence.finish();
        return;
    }
    let matrix = Matrix::new(&mut evidence);

    let (code, stdout) = matrix.xtask(
        &["ci", "wasm"],
        &[("DNACALC_CI_INJECT_BROWSER_FAILURE", "1")],
        "01-ci-wasm-injected",
        &mut evidence,
    );
    let manifest = matrix.manifest("wasm", "01-ci-wasm-injected.manifest", &mut evidence);
    evidence.expect(
        code == Some(1)
            && manifest["result"] == "failed"
            && manifest["exit_code"] == 1
            && manifest["inject_browser_failure"] == true
            && stdout.lines().any(|line| line.starts_with("CI RESULT failed: wasm")),
        format!(
            "01-ci-wasm-injected: with DNACALC_CI_INJECT_BROWSER_FAILURE=1, `ci wasm` exits {code:?} and its manifest records result {} and exit code {}",
            manifest["result"], manifest["exit_code"]
        ),
    );
    for browser in &BROWSERS {
        let run = fixture_run(&manifest, "injected-failure", browser.name);
        let named = format!(
            "CI failed injected-failure@{} (package {BROWSER_FIXTURE}, test target failure, ran in {})",
            browser.name, browser.banner
        );
        evidence.expect(
            run["outcome"] == "failed"
                && run["failed"] == 1
                && run["passed"] == 0
                && run["executed_in"] == browser.banner
                && stdout.lines().any(|line| line.starts_with(&named)),
            format!(
                "01-ci-wasm-injected {}: the manifest records injected-failure failed ({} failed, {} passed) in {}, and the run printed `{named}`",
                browser.name, run["failed"], run["passed"], run["executed_in"]
            ),
        );
        let log = matrix.read(run["log"].as_str().unwrap_or_default());
        let failure = executable_block(&log, "failure").unwrap_or_default();
        let marker = "CI-BROWSER-AGENT injected_browser_failure: ";
        let agent = failure
            .iter()
            .find_map(|line| line.find(marker).map(|at| line[at + marker.len()..].trim()))
            .unwrap_or_default();
        let results = result_lines(&failure);
        evidence.expect(
            first_line(&failure).starts_with(&format!("{BANNER_PREFIX}{} on ", browser.banner))
                && agent.contains(browser.agent)
                && failure.iter().any(|line| line.contains("CI-INJECTED-FAILURE"))
                && results.len() == 1
                && results[0].contains(" 0 passed; 1 failed;"),
            format!(
                "01-ci-wasm-injected {}: the kept transcript shows the failing test ran in the browser, not at compile time: it starts with the runner's {} banner, the test reported user agent {agent:?} before its panic, and libtest reports {results:?}",
                browser.name, browser.banner
            ),
        );
        let probe = fixture_run(&manifest, "route-probe", browser.name);
        evidence.expect(
            probe["outcome"] == "passed",
            format!(
                "01-ci-wasm-injected {}: the route probe passed in the same run ({}), so the browser itself worked",
                browser.name, probe["outcome"]
            ),
        );
    }
    let failed_packages: Vec<&str> = manifest["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["status"] == "failed")
        .filter_map(|entry| entry["package"].as_str())
        .collect();
    let problems: Vec<&str> = manifest["problems"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    evidence.expect(
        failed_packages.is_empty()
            && !problems.is_empty()
            && problems.iter().all(|problem| problem.contains("injected-failure")),
        format!(
            "01-ci-wasm-injected: no package failed ({failed_packages:?}) and every problem names the injected fixture: {problems:?}"
        ),
    );

    // The control: the same command without the injected test passes.
    let (code, _) = matrix.xtask(&["ci", "wasm"], &[], "02-ci-wasm-control", &mut evidence);
    let control = matrix.manifest("wasm", "02-ci-wasm-control.manifest", &mut evidence);
    let injected_present = control["fixtures"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|run| run["fixture"] == "injected-failure");
    evidence.expect(
        code == Some(0) && control["result"] == "passed" && !injected_present,
        format!(
            "02-ci-wasm-control: without the variable the same command exits {code:?}, result {}, and runs no injected test",
            control["result"]
        ),
    );
    evidence
        .lines
        .push(format!("copy kept at {}", matrix.copy.base.display()));
    evidence.finish();
}

/// The workflow's wasm job runs `cargo xtask ci wasm` as exactly one step with nothing that keeps
/// its failure from failing the job; every action is pinned by commit SHA with its tag noted.
fn workflow_checks(evidence: &mut Evidence) {
    let path = super::repo_root()
        .join(".github")
        .join("workflows")
        .join("ci.yml");
    let text = super::read_text(&path);
    let workflow = match parse_yaml(&text) {
        Ok(workflow) => workflow,
        Err(e) => {
            evidence.expect(
                false,
                format!("workflow: {} does not parse: {e}", path.display()),
            );
            return;
        }
    };
    let wasm = &workflow["jobs"]["wasm"];
    let steps = wasm["steps"].as_array().cloned().unwrap_or_default();
    let runs: Vec<&str> = steps
        .iter()
        .filter_map(|step| step["run"].as_str())
        .map(str::trim)
        .collect();
    let ci_steps: Vec<&Value> = steps
        .iter()
        .filter(|step| step["run"].as_str().map(str::trim) == Some("cargo xtask ci wasm"))
        .collect();
    let continue_on_error = wasm.get("continue-on-error").is_some()
        || steps
            .iter()
            .any(|step| step.get("continue-on-error").is_some());
    evidence.expect(
        ci_steps.len() == 1 && ci_steps[0].get("if").is_none() && !continue_on_error,
        format!(
            "workflow: the wasm job has exactly one step running `cargo xtask ci wasm` ({} found), that step has no `if`, and no continue-on-error is set on the job or any step ({continue_on_error})",
            ci_steps.len()
        ),
    );
    let test_logic: Vec<&&str> = runs
        .iter()
        .filter(|run| {
            run.contains("cargo test")
                || run.contains("wasm-bindgen-test-runner")
                || run.contains("cargo xtask accept")
        })
        .collect();
    evidence.expect(
        test_logic.is_empty(),
        format!("workflow: no other wasm-job step runs tests itself: {test_logic:?}"),
    );
    let injected = wasm["env"]["DNACALC_CI_INJECT_BROWSER_FAILURE"]
        .as_str()
        .unwrap_or_default();
    let input = &workflow["on"]["workflow_dispatch"]["inputs"]["inject_browser_failure"];
    evidence.expect(
        injected.contains("inputs.inject_browser_failure") && input["type"] == "boolean",
        format!(
            "workflow: the wasm job sets DNACALC_CI_INJECT_BROWSER_FAILURE from the boolean dispatch input inject_browser_failure ({injected:?}, type {})",
            input["type"]
        ),
    );
    let mut pins = Vec::new();
    let mut unpinned = Vec::new();
    for line in text.lines() {
        let Some(uses) = line
            .trim()
            .strip_prefix("- uses: ")
            .or_else(|| line.trim().strip_prefix("uses: "))
        else {
            continue;
        };
        let (action, comment) = uses.split_once(" # ").unwrap_or((uses, ""));
        let sha = action
            .rsplit_once('@')
            .map(|(_, sha)| sha)
            .unwrap_or_default();
        if sha.len() == 40
            && sha.chars().all(|c| c.is_ascii_hexdigit())
            && comment.trim().starts_with('v')
        {
            pins.push(format!("{action} ({})", comment.trim()));
        } else {
            unpinned.push(uses.to_owned());
        }
    }
    evidence.expect(
        !pins.is_empty() && unpinned.is_empty(),
        format!(
            "workflow: every action is pinned by a 40-digit commit SHA with its release tag in a comment: {} uses, unpinned {unpinned:?}",
            pins.len()
        ),
    );
    let mut checkouts = 0;
    let mut uploads_always = true;
    let mut one_command = true;
    for (name, job) in workflow["jobs"].as_object().into_iter().flatten() {
        let steps = job["steps"].as_array().cloned().unwrap_or_default();
        for step in &steps {
            let uses = step["uses"].as_str().unwrap_or_default();
            if uses.starts_with("actions/checkout@") {
                checkouts += 1;
            }
            if uses.starts_with("actions/upload-artifact@") && step["if"] != "always()" {
                uploads_always = false;
            }
        }
        let commands = steps
            .iter()
            .filter_map(|step| step["run"].as_str())
            .filter(|run| run.trim().starts_with("cargo xtask "))
            .count();
        if commands != 1 {
            one_command = false;
            evidence.lines.push(format!(
                "workflow job {name} runs {commands} cargo xtask commands"
            ));
        }
    }
    // What each job uploads, as AGENTS.md states it.
    let expected_uploads: [(&str, &[&str]); 6] = [
        (
            "windows",
            &["target/ci", "target/accept", "accept-receipts"],
        ),
        ("linux", &["target/ci", "target/accept", "accept-receipts"]),
        ("wasm", &["target/ci", "target/accept", "accept-receipts"]),
        ("licenses", &["target/ci"]),
        ("desktop", &["target/ci"]),
        ("audit", &["target/ci-audit"]),
    ];
    let mut uploads = Vec::new();
    for (name, expected) in expected_uploads {
        let paths: Vec<String> = workflow["jobs"][name]["steps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|step| {
                step["uses"]
                    .as_str()
                    .is_some_and(|uses| uses.starts_with("actions/upload-artifact@"))
            })
            .flat_map(|step| {
                step["with"]["path"]
                    .as_str()
                    .unwrap_or_default()
                    .lines()
                    .map(|line| line.trim().to_owned())
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>()
            })
            .collect();
        uploads.push((name, paths.clone(), paths == expected));
    }
    evidence.expect(
        uploads.iter().all(|(_, _, matches)| *matches),
        format!(
            "workflow: each job uploads what AGENTS.md states (windows, linux and wasm: target/ci, target/accept, accept-receipts; licenses and desktop: target/ci; audit: target/ci-audit): {:?}",
            uploads
                .iter()
                .map(|(name, paths, _)| format!("{name} {paths:?}"))
                .collect::<Vec<_>>()
        ),
    );
    let desktop_name = workflow["jobs"]["desktop"]["name"]
        .as_str()
        .unwrap_or_default();
    evidence.expect(
        desktop_name.starts_with("release build of the native-only packages")
            && desktop_name.contains("dc-x704")
            && !desktop_name.contains("smoke"),
        format!(
            "workflow: the desktop job is named for what it does, a release build, and names dc-x704, the bead owning the packaging smoke: {desktop_name:?}"
        ),
    );
    evidence.expect(
        checkouts == workflow["jobs"].as_object().map_or(0, Map::len)
            && uploads_always
            && one_command,
        format!(
            "workflow: each of the {} jobs checks out the repository, runs one cargo xtask command and uploads its record with `if: always()` (checkouts {checkouts}, uploads always {uploads_always}, one command each {one_command})",
            workflow["jobs"].as_object().map_or(0, Map::len)
        ),
    );
}

/// A reader for the block-style YAML subset `.github/workflows/ci.yml` is written in: mappings,
/// sequences (of scalars or of mappings), plain, single- and double-quoted scalars, literal block
/// scalars (`|`, `|-`) and comments. It refuses anything else (flow collections, anchors, aliases,
/// tags, documents, tabs), so the workflow stays readable by this case. Scalars stay strings.
fn parse_yaml(text: &str) -> Result<Value, String> {
    let mut reader = Yaml {
        lines: text.lines().map(str::to_owned).collect(),
        at: 0,
    };
    reader.skip();
    if reader.at >= reader.lines.len() {
        return Ok(Value::Null);
    }
    let indent = reader.indent(reader.at)?;
    let value = reader.node(indent)?;
    reader.skip();
    if reader.at < reader.lines.len() {
        return Err(format!(
            "line {}: unexpected content {:?}",
            reader.at + 1,
            reader.lines[reader.at]
        ));
    }
    Ok(value)
}

struct Yaml {
    lines: Vec<String>,
    at: usize,
}

impl Yaml {
    fn indent(&self, at: usize) -> Result<usize, String> {
        let line = &self.lines[at];
        let indent = line.len() - line.trim_start_matches(' ').len();
        if line[indent..].starts_with('\t') {
            return Err(format!("line {}: a tab indents it", at + 1));
        }
        Ok(indent)
    }

    fn skip(&mut self) {
        while self.at < self.lines.len() {
            let trimmed = self.lines[self.at].trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                self.at += 1;
            } else {
                break;
            }
        }
    }

    /// The node starting at the current line, whose indentation is `indent`.
    fn node(&mut self, indent: usize) -> Result<Value, String> {
        let content = uncommented(&self.lines[self.at][indent..]);
        if content == "-" || content.starts_with("- ") {
            self.sequence(indent)
        } else {
            self.mapping(indent)
        }
    }

    fn mapping(&mut self, indent: usize) -> Result<Value, String> {
        let mut map = Map::new();
        loop {
            self.skip();
            if self.at >= self.lines.len() {
                break;
            }
            let here = self.indent(self.at)?;
            if here < indent {
                break;
            }
            if here > indent {
                return Err(format!("line {}: unexpected indentation", self.at + 1));
            }
            let content = uncommented(&self.lines[self.at][here..]).to_owned();
            if content == "-" || content.starts_with("- ") {
                break;
            }
            let (key, rest) = split_key(&content)
                .ok_or_else(|| format!("line {}: not a `key: value` line", self.at + 1))?;
            self.at += 1;
            let value = self.after_key(rest, indent)?;
            if map.insert(key.clone(), value).is_some() {
                return Err(format!("key {key} appears twice"));
            }
        }
        Ok(Value::Object(map))
    }

    fn sequence(&mut self, indent: usize) -> Result<Value, String> {
        let mut items = Vec::new();
        loop {
            self.skip();
            if self.at >= self.lines.len() || self.indent(self.at)? != indent {
                break;
            }
            let content = uncommented(&self.lines[self.at][indent..]).to_owned();
            let Some(after) = content.strip_prefix('-') else {
                break;
            };
            let after = after.trim_start();
            if after.is_empty() {
                self.at += 1;
                self.skip();
                if self.at < self.lines.len() && self.indent(self.at)? > indent {
                    let inner = self.indent(self.at)?;
                    items.push(self.node(inner)?);
                } else {
                    items.push(Value::Null);
                }
            } else if split_key(after).is_some() {
                // `- key: value` opens a mapping whose keys sit where `key` sits.
                let inner = self.lines[self.at].len()
                    - self.lines[self.at][indent + 1..].trim_start().len();
                let line = format!("{}{}", " ".repeat(inner), &self.lines[self.at][inner..]);
                self.lines[self.at] = line;
                items.push(self.mapping(inner)?);
            } else {
                self.at += 1;
                items.push(scalar(after)?);
            }
        }
        Ok(Value::Array(items))
    }

    /// The value after `key:` on a line indented `indent`.
    fn after_key(&mut self, rest: &str, indent: usize) -> Result<Value, String> {
        match rest {
            "" => {
                self.skip();
                if self.at < self.lines.len() && self.indent(self.at)? > indent {
                    let inner = self.indent(self.at)?;
                    self.node(inner)
                } else {
                    Ok(Value::Null)
                }
            }
            "|" | "|-" => {
                let mut block: Vec<String> = Vec::new();
                let mut block_indent = None;
                while self.at < self.lines.len() {
                    let line = &self.lines[self.at];
                    if line.trim().is_empty() {
                        block.push(String::new());
                        self.at += 1;
                        continue;
                    }
                    let here = self.indent(self.at)?;
                    if here <= indent {
                        break;
                    }
                    let base = *block_indent.get_or_insert(here);
                    if here < base {
                        return Err(format!("line {}: block scalar indentation", self.at + 1));
                    }
                    block.push(line[base..].to_owned());
                    self.at += 1;
                }
                while block.last().is_some_and(String::is_empty) {
                    block.pop();
                }
                let mut text = block.join("\n");
                if rest == "|" {
                    text.push('\n');
                }
                Ok(Value::String(text))
            }
            other => scalar(other),
        }
    }
}

/// `text` without a trailing comment: a `#` at its start or after white space, outside quotes.
fn uncommented(text: &str) -> &str {
    let mut quote = None;
    let mut previous = ' ';
    for (at, c) in text.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '\'' || c == '"' => {
                if previous == ' ' || previous == ':' || at == 0 {
                    quote = Some(c);
                }
            }
            None if c == '#' && previous.is_whitespace() => return text[..at].trim_end(),
            None => {}
        }
        previous = c;
    }
    text.trim_end()
}

/// The key and the rest of a `key: value` or `key:` line, when it is one.
fn split_key(content: &str) -> Option<(String, &str)> {
    if content.starts_with(['"', '\'']) {
        let quote = content.chars().next()?;
        let end = content[1..].find(quote)? + 1;
        let rest = content[end + 1..].strip_prefix(':')?;
        if !rest.is_empty() && !rest.starts_with(' ') {
            return None;
        }
        return Some((content[1..end].to_owned(), rest.trim()));
    }
    let (key, rest) = match content.find(": ") {
        Some(at) => (&content[..at], &content[at + 2..]),
        None => (content.strip_suffix(':')?, ""),
    };
    let plain = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'));
    plain.then(|| (key.to_owned(), rest.trim()))
}

fn scalar(text: &str) -> Result<Value, String> {
    let text = text.trim();
    if let Some(inner) = text.strip_prefix('"') {
        let inner = inner
            .strip_suffix('"')
            .ok_or_else(|| format!("unterminated string {text}"))?;
        return Ok(Value::String(
            inner.replace("\\\"", "\"").replace("\\\\", "\\"),
        ));
    }
    if let Some(inner) = text.strip_prefix('\'') {
        let inner = inner
            .strip_suffix('\'')
            .ok_or_else(|| format!("unterminated string {text}"))?;
        return Ok(Value::String(inner.replace("''", "'")));
    }
    if text.starts_with(['[', '{', '&', '*', '!', '>', '|', '%', '@', '`']) || text == "---" {
        return Err(format!(
            "{text:?} is outside the YAML subset this case reads"
        ));
    }
    Ok(Value::String(text.to_owned()))
}
