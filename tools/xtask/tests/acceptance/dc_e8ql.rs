//! The body of the dc-e8ql acceptance case (the `#[test]` function sits at the root of
//! `tests/acceptance.rs`, so libtest lists it under its registered name).
//!
//! The case drives the xtask binary this test build compiled (`CARGO_BIN_EXE_xtask`), the same
//! binary `cargo xtask` runs, as `accept --id <fixture bead> --platform wasm [--require-all]`,
//! against a copy of this checkout's working tree built as the infra-m0.3 cases build theirs
//! (`acceptance/infra_m0_3.rs`), with one added fixture package, `xtask-browser-fixture` at
//! `tools/browser-fixture` (from `tests/fixtures/browser/`). Its two wasm test targets hold one
//! fixture case each: `fixture_browser__alpha` (bead `fixture-browser`) in a target that configures
//! no run mode, beside a second test whose name contains the case's, and
//! `fixture_browser_node__alpha` (bead `fixture-browser-node`) in a target that configures Node.js.
//!
//! The case reads the host itself, as the runner does, and takes one of two branches, which it
//! states in `ACCEPT-NOTE` lines that the runner keeps in this case's receipt entry:
//! - `browsers`: wasm-bindgen-test-runner on PATH and both drivers, each named by its variable or
//!   on PATH. The runner must record `fixture_browser__alpha` passed in chromium and in firefox,
//!   with the runner's banner naming Chrome and Firefox and the user agent each browser reported
//!   from inside the test; it must never record `fixture_browser_node__alpha` passed in a browser,
//!   although that case passes under Node.js. Plain cargo runs show what the runner guards against:
//!   with the route's earlier settings (runner and driver, no forced browser) the unconfigured case
//!   runs under Node.js and reports 1 passed, and `--list` and `--exact` under the runner select
//!   exactly one of the two tests where a substring filter selects both.
//! - `not_run`: the runner or a driver missing (the WSL clone has neither runner, driver nor
//!   browser). Each browser without its runner or driver must be recorded `not_run` with the reason
//!   that names the missing program, never passed, and the run exits 1 under `--require-all`.
//!
//! What is compared, independently of the runner's classification: the user agent each fixture
//! test prints from inside whatever executed it (HeadlessChrome, Firefox or Node.js), the runner's
//! transcripts and printed lines, and plain cargo runs of the same targets. Evidence goes to
//! `target/accept/dc-e8ql/dc_e8ql__browser_route/` with a `summary.txt`.

use super::infra_m0_3::{Accepted, Evidence, Fixture, WorkspaceCopy};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

const BEAD: &str = "dc-e8ql";
const CASE: &str = "dc_e8ql__browser_route";

const WASM_TARGET: &str = "wasm32-unknown-unknown";
const PACKAGE: &str = "xtask-browser-fixture";

static BROWSER_FIXTURE: Fixture = Fixture {
    package: PACKAGE,
    dir: "tools/browser-fixture",
    source: "browser",
    files: &[
        ("package.toml", "Cargo.toml"),
        ("lib.rs", "src/lib.rs"),
        ("acceptance.rs", "tests/acceptance.rs"),
        ("node.rs", "tests/node.rs"),
        ("acceptance.json", "acceptance.json"),
    ],
};

/// The fixture case in the target that configures no run mode, the other test there whose name
/// contains it, and the fixture case in the target that configures Node.js.
const BROWSER_CASE: &str = "fixture_browser__alpha";
const TWIN: &str = "twin_of_fixture_browser__alpha";
const NODE_CASE: &str = "fixture_browser_node__alpha";

const RUNNER_PROGRAM: &str = "wasm-bindgen-test-runner";
const RUNNER_VARIABLE: &str = "CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER";
const FORCE_BROWSER: &str = "WASM_BINDGEN_USE_BROWSER";
const BANNER_PREFIX: &str = "Running headless tests in ";

/// A browser as this case checks it: its name in the receipt, its driver's variable and program,
/// the name the runner's banner gives it, and the token of the user agent it reports.
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

/// Checks that the case is registered under dc-e8ql in the xtask manifest for windows and linux.
fn registered() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("acceptance.json");
    let manifest = super::read_json(&path);
    let entry = manifest["beads"]
        .as_array()
        .and_then(|beads| beads.iter().find(|bead| bead["id"] == BEAD))
        .unwrap_or_else(|| panic!("{} has no entry for {BEAD}", path.display()));
    let cases = super::strings(&entry["cases"], "registered cases");
    assert!(
        cases.iter().any(|registered| registered == CASE),
        "{CASE} is not registered under {BEAD} in {}; registered: {cases:?}",
        path.display()
    );
    let platforms = super::strings(&entry["platforms"][CASE], "the case's platforms");
    assert_eq!(
        platforms,
        ["windows", "linux"],
        "{CASE} must declare windows and linux"
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

/// The driver of `browser`: the file its variable names, otherwise the program on PATH.
fn driver(browser: &Browser) -> Option<PathBuf> {
    std::env::var_os(browser.variable)
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| on_path(browser.program))
}

/// Prints `text` as a note the runner keeps in this case's receipt entry, and keeps it in the
/// evidence.
fn note(evidence: &mut Evidence, text: String) {
    println!("ACCEPT-NOTE: {CASE}: {text}");
    evidence.lines.push(format!("note: {text}"));
}

/// The text after `FIXTURE-AGENT <test>: ` in `output`: the user agent of whatever ran the test.
fn agent_of(output: &str, test: &str) -> Option<String> {
    let marker = format!("FIXTURE-AGENT {test}: ");
    output.lines().find_map(|line| {
        line.find(&marker)
            .map(|at| line[at + marker.len()..].trim().to_owned())
    })
}

/// The `test result:` lines of `output`.
fn result_lines(output: &str) -> Vec<&str> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("test result: "))
        .collect()
}

/// The standard output section of a transcript the runner wrote (`--- stdout` to `--- stderr`).
fn transcript_stdout(transcript: &str) -> &str {
    let start = transcript
        .find("--- stdout\n")
        .map_or(0, |at| at + "--- stdout\n".len());
    let end = transcript[start..]
        .find("\n--- stderr")
        .map_or(transcript.len(), |at| start + at);
    &transcript[start..end]
}

/// The first non-empty line of `output`.
fn first_line(output: &str) -> &str {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
}

/// The receipt entry of `case` run in `browser`.
fn browser_run<'a>(run: &'a Accepted, case: &str, browser: &str) -> &'a Value {
    run.run(case)["browsers"]
        .as_array()
        .and_then(|runs| runs.iter().find(|entry| entry["browser"] == browser))
        .unwrap_or(&Value::Null)
}

/// The standard output the runner kept for one browser's run, read from the copy.
fn kept_stdout(copy: &WorkspaceCopy, entry: &Value) -> String {
    entry["log"]
        .as_str()
        .and_then(|log| fs::read_to_string(copy.path(log)).ok())
        .map(|text| transcript_stdout(&text).to_owned())
        .unwrap_or_default()
}

/// `cargo test` of one fixture target for wasm32 with libtest arguments, as the runner builds it.
fn plain_test<'a>(target: &'a str, libtest: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec![
        "test",
        "-p",
        PACKAGE,
        "--test",
        target,
        "--target",
        WASM_TARGET,
        "--locked",
        "--",
    ];
    args.extend_from_slice(libtest);
    args
}

pub fn browser_route() {
    let _serial = super::serial();
    registered();
    let mut evidence = Evidence::for_bead(BEAD, CASE);
    let runner = on_path(RUNNER_PROGRAM);
    let drivers: Vec<Option<PathBuf>> = BROWSERS.iter().map(driver).collect();
    evidence.lines.push(format!(
        "host: {RUNNER_PROGRAM} {runner:?}; {}",
        BROWSERS
            .iter()
            .zip(&drivers)
            .map(|(browser, found)| format!("{} driver {found:?}", browser.name))
            .collect::<Vec<_>>()
            .join("; ")
    ));
    let mut copy = WorkspaceCopy::with_fixture("dc_e8ql_browser", &BROWSER_FIXTURE, &mut evidence);
    // Only the runner may force the browser, so no run in the copy inherits that variable.
    copy.set_environment(FORCE_BROWSER, None);
    // The runner's Node.js harness writes this path into a JavaScript string literal; on Windows
    // backslashes in the default path can form JavaScript escapes and cause a syntax error
    // that stops the Node.js run before its test. With forward slashes the Node.js run completes,
    // so only the runner's banner check stands between it and a pass. Browser runs ignore it.
    let bench = copy.base.join("wbg_benchmark.json");
    copy.set_environment(
        "WASM_BINDGEN_BENCH_RESULT",
        Some(bench.to_string_lossy().replace('\\', "/")),
    );
    match (&runner, drivers.as_slice()) {
        (Some(runner), [Some(chromedriver), Some(_)]) => {
            browsers_branch(&copy, runner, chromedriver, &mut evidence)
        }
        _ => not_run_branch(&copy, runner.is_some(), &drivers, &mut evidence),
    }
    evidence
        .lines
        .push(format!("copy kept at {}", copy.base.display()));
    evidence.finish();
}

/// The host has the runner and both drivers: the runner's records are checked against the user
/// agents the browsers reported and against plain cargo runs.
fn browsers_branch(
    copy: &WorkspaceCopy,
    runner: &Path,
    chromedriver: &Path,
    evidence: &mut Evidence,
) {
    let runner_text = runner.to_string_lossy().into_owned();
    let chromedriver_text = chromedriver.to_string_lossy().into_owned();
    note(
        evidence,
        "branch browsers: this host has wasm-bindgen-test-runner, chromedriver and geckodriver, so the runner sends the fixture cases to Chrome and Firefox; the notes below say what each run showed".to_owned(),
    );
    // The route's earlier settings: the runner and one driver, every other driver removed, and
    // nothing forcing a browser.
    let old_route = [
        (RUNNER_VARIABLE, Some(runner_text.as_str())),
        ("CHROMEDRIVER", Some(chromedriver_text.as_str())),
        ("GECKODRIVER", None),
        ("SAFARIDRIVER", None),
        (FORCE_BROWSER, None),
    ];
    let mut forced = old_route;
    forced[4] = (FORCE_BROWSER, Some("1"));

    // The hazard: with the earlier settings the unconfigured case runs under Node.js and reports
    // 1 passed, which the earlier runner recorded as passed in chromium.
    let (code, stdout, _) = copy.cargo_with(
        &plain_test("acceptance", &["--exact", BROWSER_CASE, "--nocapture"]),
        &old_route,
        "01-plain-cargo-earlier-route",
        evidence,
    );
    let agent = agent_of(&stdout, BROWSER_CASE).unwrap_or_default();
    let results = result_lines(&stdout);
    evidence.expect(
        code == Some(0)
            && results.len() == 1
            && results[0].contains(" 1 passed; 0 failed; 0 ignored;")
            && agent.starts_with("Node.js/")
            && !stdout.contains(BANNER_PREFIX),
        format!(
            "01-plain-cargo-earlier-route: with the runner and CHROMEDRIVER but no forced browser, cargo exits {code:?} with {results:?}, no runner banner, and the test reports user agent {agent:?}: Node.js ran it"
        ),
    );
    note(
        evidence,
        format!(
            "earlier route (runner and CHROMEDRIVER, no {FORCE_BROWSER}): plain cargo ran {BROWSER_CASE} under user agent {agent:?} and reported {:?}, with no runner banner",
            results.first().copied().unwrap_or_default()
        ),
    );

    // The runner's route: each browser forced, its banner checked, the case passed in both.
    let run = copy.accept(
        &[
            "--id",
            "fixture-browser",
            "--platform",
            "wasm",
            "--require-all",
        ],
        "02-accept-browser",
        true,
        evidence,
    );
    evidence.expect(
        run.exit_code == Some(0)
            && run.result() == "passed"
            && run.outcome(BROWSER_CASE) == Some("passed")
            && run.printed("passed", BROWSER_CASE),
        format!(
            "02-accept-browser: exit code {:?} is 0, the result {} is passed and {BROWSER_CASE} is recorded passed",
            run.exit_code,
            run.result()
        ),
    );
    for browser in &BROWSERS {
        let label = format!("02-accept-browser {}", browser.name);
        let sub_case = format!("{BROWSER_CASE}@{}", browser.name);
        let entry = browser_run(&run, BROWSER_CASE, browser.name);
        let banner = entry["runner_banner"].as_str().unwrap_or_default();
        let result_line = entry["result_line"].as_str().unwrap_or_default();
        evidence.expect(
            entry["case"] == sub_case.as_str()
                && entry["outcome"] == "passed"
                && entry["executed_in"] == browser.banner
                && banner.starts_with(&format!("{BANNER_PREFIX}{} on ", browser.banner))
                && entry["exit_code"] == 0,
            format!(
                "{label}: the receipt records {sub_case} passed, executed in {} by the runner's banner {banner:?} (entry outcome {}, executed_in {})",
                browser.banner, entry["outcome"], entry["executed_in"]
            ),
        );
        let others_removed = BROWSERS
            .iter()
            .filter(|other| other.name != browser.name)
            .all(|other| {
                entry["environment"].get(other.variable).is_none()
                    && entry["environment_removed"]
                        .as_array()
                        .is_some_and(|removed| removed.iter().any(|name| name == other.variable))
            });
        evidence.expect(
            entry["environment"][FORCE_BROWSER] == "1"
                && entry["environment"][browser.variable].is_string()
                && entry["environment"][RUNNER_VARIABLE].is_string()
                && others_removed,
            format!(
                "{label}: the route set {FORCE_BROWSER}=1, {RUNNER_VARIABLE} and {} only, and removed the other drivers: {}",
                browser.variable, entry["environment"]
            ),
        );
        evidence.expect(
            result_line.contains(" 1 passed; 0 failed; 0 ignored;")
                && result_line.contains(" 1 filtered out;"),
            format!(
                "{label}: --exact {BROWSER_CASE} ran exactly one test under the runner and filtered out the other: {result_line:?}"
            ),
        );
        let kept = kept_stdout(copy, entry);
        let agent = agent_of(&kept, BROWSER_CASE).unwrap_or_default();
        let invoked = kept
            .lines()
            .filter(|line| line.trim().starts_with("Invoking test: "))
            .count();
        evidence.expect(
            first_line(&kept) == banner
                && agent.contains(browser.agent)
                && agent_of(&kept, TWIN).is_none()
                && invoked == 1,
            format!(
                "{label}: the kept log starts with the banner, {invoked} test was invoked (not {TWIN}), and the browser itself reported user agent {agent:?}, which names {}",
                browser.agent
            ),
        );
        evidence.expect(
            run.stdout.lines().any(|line| {
                line.starts_with(&format!(
                    "ACCEPT passed {sub_case} (package {PACKAGE}, exit code 0, ran in {})",
                    browser.banner
                ))
            }),
            format!(
                "{label}: the runner printed `ACCEPT passed {sub_case} (package {PACKAGE}, exit code 0, ran in {})`",
                browser.banner
            ),
        );
        note(
            evidence,
            format!(
                "{sub_case} recorded {}: runner banner {banner:?}; the test reported user agent {agent:?}; {result_line:?}",
                entry["outcome"].as_str().unwrap_or("nothing")
            ),
        );
    }

    // --list and --exact under the runner, with the browser forced as the route forces it.
    let (code, stdout, _) = copy.cargo_with(
        &plain_test("acceptance", &["--list"]),
        &forced,
        "03-plain-cargo-list",
        evidence,
    );
    let listed: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.trim_end().strip_suffix(": test"))
        .collect();
    evidence.expect(
        code == Some(0) && listed == [BROWSER_CASE, TWIN],
        format!(
            "03-plain-cargo-list: wasm-bindgen-test-runner --list exits {code:?} and lists exactly {listed:?}"
        ),
    );
    let (code, stdout, _) = copy.cargo_with(
        &plain_test("acceptance", &[BROWSER_CASE, "--nocapture"]),
        &forced,
        "04-plain-cargo-substring",
        evidence,
    );
    let results = result_lines(&stdout);
    let agents = [agent_of(&stdout, BROWSER_CASE), agent_of(&stdout, TWIN)];
    evidence.expect(
        code == Some(0)
            && results.len() == 1
            && results[0].contains(" 2 passed; 0 failed; 0 ignored;")
            && agents
                .iter()
                .all(|agent| agent.as_deref().is_some_and(|a| a.contains("HeadlessChrome/"))),
        format!(
            "04-plain-cargo-substring: without --exact the name selects both tests ({results:?}, user agents {agents:?}), so --exact is what selects one"
        ),
    );
    let partial = &BROWSER_CASE[..BROWSER_CASE.len() - 1];
    let (code, stdout, _) = copy.cargo_with(
        &plain_test("acceptance", &["--exact", partial, "--nocapture"]),
        &forced,
        "05-plain-cargo-partial-exact",
        evidence,
    );
    evidence.expect(
        code == Some(0)
            && result_lines(&stdout).is_empty()
            && !stdout.contains("FIXTURE-AGENT"),
        format!(
            "05-plain-cargo-partial-exact: --exact {partial} runs no test (exit code {code:?}, no result line), so the runner reads no 1-passed line for a partial name"
        ),
    );
    note(
        evidence,
        format!(
            "under wasm-bindgen-test-runner --list lists exactly {listed:?}; --exact {BROWSER_CASE} ran 1 test and filtered out 1; the substring filter ran 2; --exact {partial} ran none"
        ),
    );

    // A case not configured for a browser: its target configures Node.js, which the forced
    // browser does not override. It passes under Node.js and must not be recorded passed in a
    // browser.
    let run = copy.accept(
        &[
            "--id",
            "fixture-browser-node",
            "--platform",
            "wasm",
            "--require-all",
        ],
        "06-accept-node",
        true,
        evidence,
    );
    evidence.expect(
        run.exit_code == Some(1)
            && run.result() == "failed"
            && run.outcome(NODE_CASE) == Some("failed"),
        format!(
            "06-accept-node: exit code {:?} is 1, the result {} is failed and {NODE_CASE} is recorded {:?}",
            run.exit_code,
            run.result(),
            run.outcome(NODE_CASE)
        ),
    );
    for browser in &BROWSERS {
        let label = format!("06-accept-node {}", browser.name);
        let sub_case = format!("{NODE_CASE}@{}", browser.name);
        let entry = browser_run(&run, NODE_CASE, browser.name);
        let reason = entry["reason"].as_str().unwrap_or_default();
        let result_line = entry["result_line"].as_str().unwrap_or_default();
        let kept = kept_stdout(copy, entry);
        let agent = agent_of(&kept, NODE_CASE).unwrap_or_default();
        evidence.expect(
            entry["outcome"] == "failed"
                && entry["executed_in"].is_null()
                && entry["runner_banner"].is_null()
                && reason.contains(&format!(
                    "does not start with `{BANNER_PREFIX}{} on`",
                    browser.banner
                )),
            format!(
                "{label}: the receipt records {sub_case} failed for want of the runner's {} banner: {reason:?}",
                browser.banner
            ),
        );
        evidence.expect(
            entry["exit_code"] == 0
                && result_line.contains(" 1 passed; 0 failed; 0 ignored;")
                && agent.starts_with("Node.js/")
                && !kept.contains(BANNER_PREFIX),
            format!(
                "{label}: the run itself passed under Node.js (exit code {}, {result_line:?}, user agent {agent:?}), so only the banner check refused it",
                entry["exit_code"]
            ),
        );
        evidence.expect(
            run.printed("failed", &sub_case) && !run.printed("passed", &sub_case),
            format!("{label}: the runner printed {sub_case} failed, not passed"),
        );
        note(
            evidence,
            format!(
                "{sub_case} recorded {}: its run reported {result_line:?} under user agent {agent:?}, runner banner {}; reason {reason:?}",
                entry["outcome"].as_str().unwrap_or("nothing"),
                entry["runner_banner"]
            ),
        );
    }
}

/// The host lacks the runner or a driver: each browser without them is recorded not_run with the
/// reason naming what is missing, and no run records passed for it.
fn not_run_branch(
    copy: &WorkspaceCopy,
    has_runner: bool,
    drivers: &[Option<PathBuf>],
    evidence: &mut Evidence,
) {
    // What the runner must report for each browser on this host: `None` for a browser that can run.
    let expected: Vec<Option<String>> = BROWSERS
        .iter()
        .zip(drivers)
        .map(|(browser, driver)| match (has_runner, driver) {
            (false, _) => Some(format!("{RUNNER_PROGRAM} is not on PATH")),
            (true, None) => Some(format!(
                "{} is not on PATH and {} names no file",
                browser.program, browser.variable
            )),
            (true, Some(_)) => None,
        })
        .collect();
    let missing: Vec<String> = BROWSERS
        .iter()
        .zip(&expected)
        .filter_map(|(browser, reason)| reason.as_ref().map(|r| format!("{}: {r}", browser.name)))
        .collect();
    note(
        evidence,
        format!(
            "branch not_run: this host cannot run every browser of the wasm route ({}), so no real browser is proven here",
            missing.join("; ")
        ),
    );
    let any_runs = expected.iter().any(Option::is_none);

    for (label, case, bead, require_all) in [
        ("01-accept-not-run", BROWSER_CASE, "fixture-browser", true),
        (
            "02-accept-not-run-without-require-all",
            BROWSER_CASE,
            "fixture-browser",
            false,
        ),
        (
            "03-accept-node-not-run",
            NODE_CASE,
            "fixture-browser-node",
            true,
        ),
    ] {
        let mut args = vec!["--id", bead, "--platform", "wasm"];
        if require_all {
            args.push("--require-all");
        }
        let run = copy.accept(&args, label, true, evidence);
        for (browser, reason) in BROWSERS.iter().zip(&expected) {
            let sub_case = format!("{case}@{}", browser.name);
            let entry = browser_run(&run, case, browser.name);
            match reason {
                Some(reason) => {
                    evidence.expect(
                        entry["outcome"] == "not_run"
                            && entry["reason"] == reason.as_str()
                            && entry["environment"] == serde_json::json!({})
                            && entry["log"].is_null()
                            && entry["exit_code"].is_null()
                            && run.printed("not_run", &sub_case)
                            && !run.printed("passed", &sub_case),
                        format!(
                            "{label}: the receipt records {sub_case} not_run with the reason {reason:?} and started nothing (entry {entry})"
                        ),
                    );
                    note(
                        evidence,
                        format!(
                            "{label}: {sub_case} recorded {} ({}), receipt result {}, exit code {:?}",
                            entry["outcome"].as_str().unwrap_or("nothing"),
                            entry["reason"].as_str().unwrap_or_default(),
                            run.result(),
                            run.exit_code
                        ),
                    );
                }
                None => evidence.expect(
                    entry["executed_in"] == browser.banner || case == NODE_CASE,
                    format!(
                        "{label}: {sub_case} can run on this host; it ran in {}",
                        entry["executed_in"]
                    ),
                ),
            }
        }
        let expected_result = if case == NODE_CASE && any_runs {
            "failed"
        } else {
            "not_run"
        };
        let expected_exit = if require_all || expected_result == "failed" {
            1
        } else {
            0
        };
        evidence.expect(
            run.result() == expected_result
                && run.exit_code == Some(expected_exit)
                && run.outcome(case) != Some("passed"),
            format!(
                "{label}: the result {} is {expected_result} (never passed) and the run exits {:?}{}",
                run.result(),
                run.exit_code,
                if require_all {
                    " under --require-all"
                } else {
                    " without --require-all"
                }
            ),
        );
    }
}
