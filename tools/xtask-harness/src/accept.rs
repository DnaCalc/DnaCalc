//! `cargo xtask accept --id <bead> --platform windows|linux|wasm|excel [--require-all]`
//!
//! 1. Finds every workspace member's `acceptance.json` through `cargo metadata` and checks them
//!    all (`manifest.rs`).
//! 2. Runs the architecture checks (`arch.rs`), whatever the bead.
//! 3. Runs each Acceptance command the manifests register for the bead, once, from the workspace
//!    root with cargo's default target directory, keeping argv, exit code and log; a command
//!    that exits non-zero fails the run. A command that is itself `cargo xtask accept ...` is
//!    skipped and listed (the runner does not run itself).
//! 4. Selects the cases registered for the bead that declare the platform. An ID that no manifest
//!    registers fails, and so does a bead with no case for the platform. Other beads' cases never
//!    run.
//! 5. Lists the package's acceptance test target with libtest's `--list`, then runs each selected
//!    case on its own with `--exact <case> --nocapture`. A case has passed only when its exact name
//!    is in the listing, libtest's one result line reports 1 passed, 0 failed and 0 ignored, cargo
//!    exits 0, and the case printed no `ACCEPT-NOT-RUN: <case>: <reason>` line on standard output
//!    or standard error. A name absent from the listing is `missing` and is not run; an ignored
//!    case is `ignored`; a case that prints the not-run line, or that this host cannot run, is
//!    `not_run`; anything else is `failed`. A test carrying the bead's case prefix that the
//!    manifest does not register fails the run. The `ACCEPT-NOTE: <case>: <text>` lines a case
//!    prints are kept in its `notes`; they never change its outcome.
//! 6. Writes the receipt to `target/accept/<bead>/<platform>/receipt.json`, beside one log per
//!    cargo run, and, for a bead some manifest registers, copies it to
//!    `accept-receipts/<bead>/<platform>.json`. Paths in the receipt are relative to the workspace
//!    root; the toolchain, cargo home, home and temporary directories appear as `<rustup>`,
//!    `<cargo-home>`, `<home>` and `<temp>`. The host record names the legacy root (the
//!    directory holding the legacy repositories) as `..` (the workspace root's parent) or
//!    `<legacy-root>` (elsewhere, from `DNACALC_LEGACY_ROOT`). Format `dnacalc-accept-receipt/3` added that record; format
//!    `dnacalc-accept-receipt/4` added case notes and, on wasm, each browser's route environment
//!    and the runner's banner naming the browser that ran the case.
//!
//! The result: `failed` when a manifest, an architecture check or a command failed, the bead is
//! unknown, no case declares the platform, or any selected case is `failed`, `missing` or
//! `ignored`; otherwise `not_run` when any selected case is `not_run`; otherwise `passed`. A host
//! that cannot run the platform (`windows` off Windows, `linux` off Linux, `excel` off Windows)
//! runs no command and no case: every case is `not_run` with the reason, so such a run never
//! records `passed`. The run exits 0 for `passed`, and for `not_run` only without
//! `--require-all`; it exits 1 otherwise. Every cargo started here uses `target`.
//!
//! Platform routes: `windows` and `linux` run the cases natively on a host of that system; `excel`
//! runs them natively on a Windows host where Excel is registered for automation, and records
//! them as `not_run` where it is not. Every native case run carries `DNACALC_ACCEPT_PLATFORM` set
//! to its route, so a case whose windows and excel parts share one name starts Excel only on the
//! excel route; `wasm` runs the acceptance target for wasm32-unknown-unknown
//! under wasm-bindgen-test-runner once with chromedriver and once with geckodriver, and records a
//! browser whose runner or driver is missing as `not_run`. Every wasm run sets
//! `WASM_BINDGEN_USE_BROWSER`, so a test target that configures no mode of its own runs in the
//! browser rather than Node.js, and removes the runner's other mode variables. A target that
//! configures a mode keeps it (`run_in_node_experimental` still runs under Node.js), so a browser's
//! run counts as passed only when the first line the runner prints is its banner naming that
//! browser (`Running headless tests in Chrome on ...`, `... in Firefox on ...`); a run without it
//! is `failed`, whatever its result line says. Which host facts the excel and wasm routes read
//! (Excel's registration, programs on PATH, the driver variables) comes from a `Probe`, which the
//! unit tests replace to show each `not_run` branch.

use crate::arch::{self, CheckResult};
use crate::cargo::{self, WASM_TARGET, Workspace};
use crate::manifest::{self, BeadEntry, Platform};
// Whether Excel is registered for automation on this host, or why the excel route cannot run:
// the capture adapter's own probe, the one `cargo xtask capture` asks too.
#[cfg(not(target_arch = "wasm32"))]
use calc_oracle_excel::excel_available;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const USAGE: &str =
    "usage: cargo xtask accept --id <bead> --platform windows|linux|wasm|excel [--require-all]";

/// The prefix of the line a case prints when it cannot run on this host.
const NOT_RUN_MARKER: &str = "ACCEPT-NOT-RUN: ";

/// The prefix of a line a case prints for the receipt to keep in its `notes`.
const NOTE_MARKER: &str = "ACCEPT-NOTE: ";

/// The committed copies of the receipts, relative to the workspace root.
const RECEIPTS_DIR: &str = "accept-receipts";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Passed,
    Failed,
    NotRun,
    Missing,
    Ignored,
}

impl Outcome {
    fn as_str(self) -> &'static str {
        match self {
            Outcome::Passed => "passed",
            Outcome::Failed => "failed",
            Outcome::NotRun => "not_run",
            Outcome::Missing => "missing",
            Outcome::Ignored => "ignored",
        }
    }
}

/// One run of one case (or, on wasm, of one case in one browser).
struct CaseRun {
    package: String,
    case: String,
    argv: Vec<String>,
    exit_code: Option<i32>,
    result_line: Option<String>,
    outcome: Outcome,
    reason: Option<String>,
    duration_s: f64,
    log: Option<String>,
    notes: Vec<String>,
    /// On wasm, the run of the case in each browser.
    browsers: Vec<CaseRun>,
    /// On a wasm run in one browser, what the route did there.
    wasm: Option<BrowserRun>,
}

/// What the wasm route did for one browser: the variables it set (none when the runner or the
/// driver is missing and nothing started), and the banner line in which wasm-bindgen-test-runner
/// named the browser that ran the case, if it printed one.
struct BrowserRun {
    browser: &'static str,
    environment: Vec<(&'static str, String)>,
    banner: Option<String>,
    executed_in: Option<String>,
}

impl CaseRun {
    fn not_run(package: &str, case: &str, reason: String) -> CaseRun {
        CaseRun {
            package: package.to_owned(),
            case: case.to_owned(),
            argv: Vec::new(),
            exit_code: None,
            result_line: None,
            outcome: Outcome::NotRun,
            reason: Some(reason),
            duration_s: 0.0,
            log: None,
            notes: Vec::new(),
            browsers: Vec::new(),
            wasm: None,
        }
    }

    fn to_json(&self) -> Value {
        let mut value = json!({
            "package": self.package,
            "case": self.case,
            "argv": self.argv,
            "exit_code": self.exit_code,
            "result_line": self.result_line,
            "outcome": self.outcome.as_str(),
            "reason": self.reason,
            "duration_s": seconds(self.duration_s),
            "log": self.log,
            "notes": self.notes,
        });
        if let Some(wasm) = &self.wasm {
            let environment: serde_json::Map<String, Value> = wasm
                .environment
                .iter()
                .map(|(name, setting)| ((*name).to_owned(), json!(setting)))
                .collect();
            value["browser"] = json!(wasm.browser);
            value["environment"] = Value::Object(environment);
            value["environment_removed"] = json!(if wasm.environment.is_empty() {
                &[][..]
            } else {
                &WASM_REMOVED[..]
            });
            value["runner_banner"] = json!(wasm.banner);
            value["executed_in"] = json!(wasm.executed_in);
        }
        if !self.browsers.is_empty() {
            value["browsers"] = Value::Array(self.browsers.iter().map(CaseRun::to_json).collect());
        }
        value
    }
}

fn seconds(duration_s: f64) -> f64 {
    (duration_s * 1000.0).round() / 1000.0
}

struct Args {
    id: String,
    platform: Platform,
    require_all: bool,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let (mut id, mut platform, mut require_all) = (None, None, false);
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--id" => id = Some(rest.next().ok_or("--id needs a bead ID")?.clone()),
            "--platform" => {
                let value = rest.next().ok_or("--platform needs a platform")?;
                platform = Some(Platform::parse(value).ok_or_else(|| {
                    format!("unknown platform {value}; expected windows, linux, wasm or excel")
                })?);
            }
            "--require-all" => require_all = true,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let id = id.ok_or("--id is required")?;
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
    {
        return Err(format!("{id} is not a bead ID"));
    }
    Ok(Args {
        id,
        platform: platform.ok_or("--platform is required")?,
        require_all,
    })
}

/// The reason a case printed for not running, from standard output or standard error.
fn not_run_reason(case: &str, stdout: &str, stderr: &str) -> Option<String> {
    let marker = format!("{NOT_RUN_MARKER}{case}: ");
    stdout.lines().chain(stderr.lines()).find_map(|line| {
        line.find(&marker)
            .map(|at| line[at + marker.len()..].trim().to_owned())
    })
}

/// The `ACCEPT-NOTE: <case>: <text>` texts a case printed, standard output first.
fn case_notes(case: &str, stdout: &str, stderr: &str) -> Vec<String> {
    let marker = format!("{NOTE_MARKER}{case}: ");
    stdout
        .lines()
        .chain(stderr.lines())
        .filter_map(|line| {
            line.find(&marker)
                .map(|at| line[at + marker.len()..].trim().to_owned())
        })
        .collect()
}

/// The start of the line wasm-bindgen-test-runner prints first when it runs tests in a headless
/// browser: `Running headless tests in Chrome on <url>` or `... in Firefox on <url>`.
const BANNER_PREFIX: &str = "Running headless tests in ";

/// The browser the runner's banner names, with the banner line. The banner must be the first
/// non-empty line of standard output, ahead of libtest's `running N tests` and of anything the
/// tests print; a run under Node.js prints none.
pub(crate) fn runner_banner(stdout: &str) -> Option<(String, String)> {
    let first = stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let rest = first.strip_prefix(BANNER_PREFIX)?;
    let browser = rest.split(" on ").next().filter(|name| !name.is_empty())?;
    Some((browser.to_owned(), first.to_owned()))
}

/// A wasm run's outcome once the banner is checked: a run that libtest reports passed counts as
/// passed only when the runner's banner names `expected`; otherwise no browser, or another one,
/// ran it, and it is `failed`.
fn browser_checked(
    expected: &str,
    stdout: &str,
    (outcome, reason): (Outcome, Option<String>),
) -> (Outcome, Option<String>) {
    if outcome != Outcome::Passed {
        return (outcome, reason);
    }
    match runner_banner(stdout) {
        Some((browser, _)) if browser == expected => (outcome, reason),
        Some((browser, line)) => (
            Outcome::Failed,
            Some(format!(
                "wasm-bindgen-test-runner ran the case in {browser}, not {expected} ({line})"
            )),
        ),
        None => (
            Outcome::Failed,
            Some(format!(
                "wasm-bindgen-test-runner's output does not start with `{BANNER_PREFIX}{expected} on`, so {expected} did not run the case (a test target that configures Node.js runs there); its 1-passed result line is not browser evidence"
            )),
        ),
    }
}

/// The libtest result line of a run and what it says about one case.
fn classify(
    case: &str,
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
) -> (Outcome, Option<String>, Option<String>) {
    let results: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("test result: "))
        .collect();
    let not_run = not_run_reason(case, stdout, stderr);
    let [result] = results.as_slice() else {
        return (
            Outcome::Failed,
            results.last().map(|line| (*line).to_owned()),
            Some(format!(
                "expected one libtest result line, found {} (exit code {exit_code:?})",
                results.len()
            )),
        );
    };
    let count = |label: &str| {
        result.split(';').find_map(|part| {
            let mut words = part.split_whitespace().rev();
            let named = words.next() == Some(label);
            let number = words.next().and_then(|n| n.parse::<u64>().ok());
            if named { number } else { None }
        })
    };
    let line = Some((*result).to_owned());
    let (passed, failed, ignored) = (count("passed"), count("failed"), count("ignored"));
    match (passed, failed, ignored) {
        (Some(1), Some(0), Some(0)) if exit_code == Some(0) => match not_run {
            Some(reason) => (Outcome::NotRun, line, Some(reason)),
            None => (Outcome::Passed, line, None),
        },
        (Some(0), Some(0), Some(1)) if exit_code == Some(0) => (
            Outcome::Ignored,
            line,
            Some("libtest reports the case ignored".to_owned()),
        ),
        (Some(0), Some(0), Some(0)) => (
            Outcome::Missing,
            line,
            Some("the run executed 0 tests".to_owned()),
        ),
        _ => (
            Outcome::Failed,
            line,
            Some(format!(
                "libtest reports {result} (exit code {exit_code:?})"
            )),
        ),
    }
}

/// The names libtest's `--list` prints, one `<name>: test` line each.
fn listed_names(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|line| line.trim_end().strip_suffix(": test"))
        .map(str::to_owned)
        .collect()
}

/// `cargo test -p <package> --test <target> --locked
/// [--features <the target's required features>] [extra] -- [libtest]`, run from the workspace
/// root.
fn cargo_test(ws: &Workspace, entry: &BeadEntry, extra: &[&str], libtest: &[&str]) -> Command {
    let required: Vec<String> = ws
        .member(&entry.package)
        .and_then(|member| {
            member
                .targets
                .iter()
                .find(|target| target.name == entry.test)
        })
        .map(|target| target.required_features.clone())
        .unwrap_or_default();
    let mut command = Command::new(cargo::cargo_program());
    command.args([
        "test",
        "-p",
        &entry.package,
        "--test",
        &entry.test,
        "--locked",
    ]);
    if !required.is_empty() {
        command.args(["--features", &required.join(",")]);
    }
    command
        .args(extra)
        .arg("--")
        .args(libtest)
        .current_dir(&ws.root);
    command
}

struct Ran {
    argv: Vec<String>,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    duration_s: f64,
    log: String,
}

/// Runs `command` and keeps its transcript in `log`.
fn run_logged(ws: &Workspace, mut command: Command, log: &Path) -> Result<Ran, String> {
    let argv = cargo::argv(&command);
    let started = Instant::now();
    let out = cargo::output(&mut command)?;
    let duration_s = started.elapsed().as_secs_f64();
    let stdout = cargo::text(&out.stdout);
    let stderr = cargo::text(&out.stderr);
    let transcript = format!(
        "argv: {argv:?}\nexit code: {:?}\nduration: {duration_s:.3}s\n--- stdout\n{stdout}\n--- stderr\n{stderr}",
        out.status.code(),
    );
    fs::write(log, transcript).map_err(|e| format!("cannot write {}: {e}", log.display()))?;
    Ok(Ran {
        argv,
        exit_code: out.status.code(),
        stdout,
        stderr,
        duration_s,
        log: ws.rel(log),
    })
}

/// How one route runs cargo: `label` suffixes the log names and output lines, `extra` adds cargo
/// arguments (a `--target`), `removed` and `environment` are the variables it removes and sets,
/// and `browser` is the browser a wasm route runs the cases in, whose banner the runner must print.
struct Route<'a> {
    label: &'a str,
    extra: &'a [&'a str],
    removed: &'a [&'static str],
    environment: &'a [(&'static str, String)],
    browser: Option<&'static Browser>,
}

impl Route<'_> {
    fn apply(&self, command: &mut Command) {
        for name in self.removed {
            command.env_remove(name);
        }
        for (name, setting) in self.environment {
            command.env(name, setting);
        }
    }
}

/// Lists the acceptance target and runs each case in it along `route`.
fn run_listed(
    ws: &Workspace,
    entry: &BeadEntry,
    cases: &[&str],
    out_dir: &Path,
    route: &Route,
    problems: &mut Vec<String>,
) -> Vec<CaseRun> {
    let (label, extra) = (route.label, route.extra);
    let mut list = cargo_test(ws, entry, extra, &["--list"]);
    route.apply(&mut list);
    let list_log = out_dir.join(format!("{}-{}-list{label}.log", entry.package, entry.test));
    let listed = match run_logged(ws, list, &list_log) {
        Err(e) => Err(e),
        Ok(ran) if ran.exit_code != Some(0) => Err(format!(
            "listing {} --test {} exited with {:?}; see {}",
            entry.package, entry.test, ran.exit_code, ran.log
        )),
        Ok(ran) => Ok((listed_names(&ran.stdout), ran)),
    };
    let wasm = |banner: Option<(String, String)>| {
        route.browser.map(|browser| BrowserRun {
            browser: browser.name,
            environment: route.environment.to_vec(),
            executed_in: banner.as_ref().map(|(name, _)| name.clone()),
            banner: banner.map(|(_, line)| line),
        })
    };
    let (names, list_ran) = match listed {
        Ok(found) => found,
        Err(e) => {
            return cases
                .iter()
                .map(|case| {
                    let mut run = CaseRun::not_run(&entry.package, case, String::new());
                    run.outcome = Outcome::Failed;
                    run.reason = Some(e.clone());
                    run.wasm = wasm(None);
                    run
                })
                .collect();
        }
    };
    let prefix = format!("{}__", manifest::snake(&entry.id));
    for name in names.iter().filter(|name| name.starts_with(&prefix)) {
        if !entry.cases.iter().any(|case| case.name == *name) {
            problems.push(format!(
                "{} --test {} has a test {name} with the {} prefix that its acceptance.json does not register",
                entry.package, entry.test, entry.id
            ));
        }
    }
    let mut runs = Vec::new();
    for case in cases {
        if !names.iter().any(|name| name == case) {
            runs.push(CaseRun {
                package: entry.package.clone(),
                case: (*case).to_owned(),
                argv: list_ran.argv.clone(),
                exit_code: list_ran.exit_code,
                result_line: None,
                outcome: Outcome::Missing,
                reason: Some(format!(
                    "{} --test {} lists no test named exactly {case}",
                    entry.package, entry.test
                )),
                duration_s: 0.0,
                log: Some(list_ran.log.clone()),
                notes: Vec::new(),
                browsers: Vec::new(),
                wasm: wasm(None),
            });
            continue;
        }
        println!("ACCEPT running {case} (package {}){label}", entry.package);
        let mut command = cargo_test(ws, entry, extra, &["--exact", case, "--nocapture"]);
        route.apply(&mut command);
        let log = out_dir.join(format!("{case}{label}.log"));
        match run_logged(ws, command, &log) {
            Err(e) => {
                let mut run = CaseRun::not_run(&entry.package, case, e);
                run.outcome = Outcome::Failed;
                run.wasm = wasm(None);
                runs.push(run);
            }
            Ok(ran) => {
                let (outcome, result_line, reason) =
                    classify(case, &ran.stdout, &ran.stderr, ran.exit_code);
                let (outcome, reason) = match route.browser {
                    Some(browser) => {
                        browser_checked(browser.banner, &ran.stdout, (outcome, reason))
                    }
                    None => (outcome, reason),
                };
                runs.push(CaseRun {
                    package: entry.package.clone(),
                    case: (*case).to_owned(),
                    argv: ran.argv,
                    exit_code: ran.exit_code,
                    result_line,
                    outcome,
                    reason,
                    duration_s: ran.duration_s,
                    log: Some(ran.log),
                    notes: case_notes(case, &ran.stdout, &ran.stderr),
                    browsers: Vec::new(),
                    wasm: wasm(runner_banner(&ran.stdout)),
                });
            }
        }
    }
    runs
}

/// A wasm32 build of xtask (which main.rs refuses) has no capture adapter, so no Excel.
#[cfg(target_arch = "wasm32")]
fn excel_available() -> Result<(), String> {
    Err("a wasm32 build of xtask has no Excel capture adapter".to_owned())
}

/// Why this host cannot run `platform` at all, or `None` when it can.
fn host_mismatch(platform: Platform) -> Option<String> {
    let os = std::env::consts::OS;
    match platform {
        Platform::Windows if os != "windows" => Some(format!(
            "platform windows needs a Windows host; this host is {os}"
        )),
        Platform::Linux if os != "linux" => Some(format!(
            "platform linux needs a Linux host; this host is {os}"
        )),
        Platform::Excel if os != "windows" => Some(format!(
            "platform excel needs a Windows host with Excel; this host is {os}"
        )),
        _ => None,
    }
}

pub(crate) fn find_on_path(program: &str) -> Option<PathBuf> {
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

/// The file a driver variable names, when it is set and names a file.
pub(crate) fn variable_file(variable: &str) -> Option<PathBuf> {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .filter(|path| path.is_file())
}

/// What the excel and wasm routes read from the host before they start anything: whether Excel is
/// registered for automation, where PATH holds a program, and which file a driver variable names.
/// `run` uses `Probe::HOST`; the unit tests inject their own to reach every `not_run` branch.
#[derive(Clone, Copy)]
pub(crate) struct Probe {
    excel: fn() -> Result<(), String>,
    on_path: fn(&str) -> Option<PathBuf>,
    variable_file: fn(&str) -> Option<PathBuf>,
}

impl Probe {
    pub(crate) const HOST: Probe = Probe {
        excel: excel_available,
        on_path: find_on_path,
        variable_file,
    };
}

/// A browser of the wasm route: its name in the receipt, its driver's variable and program, and
/// the name wasm-bindgen-test-runner's banner gives it.
pub(crate) struct Browser {
    pub(crate) name: &'static str,
    pub(crate) variable: &'static str,
    pub(crate) program: &'static str,
    pub(crate) banner: &'static str,
}

pub(crate) static BROWSERS: [Browser; 2] = [
    Browser {
        name: "chromium",
        variable: "CHROMEDRIVER",
        program: "chromedriver",
        banner: "Chrome",
    },
    Browser {
        name: "firefox",
        variable: "GECKODRIVER",
        program: "geckodriver",
        banner: "Firefox",
    },
];

/// The runner program cargo starts for a wasm32-unknown-unknown test target.
const WASM_RUNNER_VARIABLE: &str = "CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER";

/// The variable that makes wasm-bindgen-test-runner run a test target that configures no mode of
/// its own in a browser rather than in Node.js.
const FORCE_BROWSER: &str = "WASM_BINDGEN_USE_BROWSER";

/// Removed from every wasm run before the route sets its own: the drivers (the route sets exactly
/// one), the runner's other modes (it refuses two at once), `NO_HEADLESS` (an interactive server
/// that never exits) and the runner's switches that skip browser or Node.js targets.
pub(crate) const WASM_REMOVED: [&str; 12] = [
    "CHROMEDRIVER",
    "GECKODRIVER",
    "SAFARIDRIVER",
    "WASM_BINDGEN_USE_DENO",
    "WASM_BINDGEN_USE_NODE_EXPERIMENTAL",
    "WASM_BINDGEN_USE_DEDICATED_WORKER",
    "WASM_BINDGEN_USE_SHARED_WORKER",
    "WASM_BINDGEN_USE_SERVICE_WORKER",
    "WASM_BINDGEN_USE_EMSCRIPTEN",
    "NO_HEADLESS",
    "WASM_BINDGEN_TEST_ONLY_NODE",
    "WASM_BINDGEN_TEST_ONLY_WEB",
];

/// The variables a wasm route sets for one browser, or why that browser cannot run here.
pub(crate) type WasmRoute = Result<Vec<(&'static str, String)>, String>;

/// The wasm route of each browser from what `probe` finds: the runner comes from PATH, the driver
/// from its variable when that names a file, otherwise from PATH.
pub(crate) fn wasm_routes(probe: &Probe) -> Vec<(&'static Browser, WasmRoute)> {
    let runner = (probe.on_path)("wasm-bindgen-test-runner");
    BROWSERS
        .iter()
        .map(|browser| {
            let driver = (probe.variable_file)(browser.variable)
                .or_else(|| (probe.on_path)(browser.program));
            let route = match (&runner, driver) {
                (None, _) => Err("wasm-bindgen-test-runner is not on PATH".to_owned()),
                (Some(_), None) => Err(format!(
                    "{} is not on PATH and {} names no file",
                    browser.program, browser.variable
                )),
                (Some(runner), Some(driver)) => Ok(vec![
                    (WASM_RUNNER_VARIABLE, runner.to_string_lossy().into_owned()),
                    (browser.variable, driver.to_string_lossy().into_owned()),
                    (FORCE_BROWSER, "1".to_owned()),
                ]),
            };
            (browser, route)
        })
        .collect()
}

fn run_wasm(
    ws: &Workspace,
    entry: &BeadEntry,
    cases: &[&str],
    out_dir: &Path,
    probe: &Probe,
    problems: &mut Vec<String>,
) -> Vec<CaseRun> {
    let mut per_browser: Vec<Vec<CaseRun>> = Vec::new();
    for (browser, route) in wasm_routes(probe) {
        let mut runs = match route {
            Err(reason) => cases
                .iter()
                .map(|case| {
                    let mut run = CaseRun::not_run(&entry.package, case, reason.clone());
                    run.wasm = Some(BrowserRun {
                        browser: browser.name,
                        environment: Vec::new(),
                        banner: None,
                        executed_in: None,
                    });
                    run
                })
                .collect(),
            Ok(environment) => {
                let label = format!("@{}", browser.name);
                let route = Route {
                    label: &label,
                    extra: &["--target", WASM_TARGET],
                    removed: &WASM_REMOVED,
                    environment: &environment,
                    browser: Some(browser),
                };
                run_listed(ws, entry, cases, out_dir, &route, problems)
            }
        };
        for run in &mut runs {
            run.case = format!("{}@{}", run.case, browser.name);
        }
        per_browser.push(runs);
    }
    cases
        .iter()
        .enumerate()
        .map(|(index, case)| {
            let browsers: Vec<CaseRun> = per_browser
                .iter_mut()
                .map(|runs| {
                    std::mem::replace(&mut runs[index], CaseRun::not_run("", "", String::new()))
                })
                .collect();
            let worst = [
                Outcome::Failed,
                Outcome::Missing,
                Outcome::Ignored,
                Outcome::NotRun,
            ]
            .into_iter()
            .find(|outcome| browsers.iter().any(|run| run.outcome == *outcome))
            .unwrap_or(Outcome::Passed);
            let reasons: Vec<String> = browsers
                .iter()
                .filter(|run| run.outcome != Outcome::Passed)
                .map(|run| {
                    format!(
                        "{}: {} ({})",
                        run.wasm.as_ref().map_or("", |wasm| wasm.browser),
                        run.outcome.as_str(),
                        run.reason.as_deref().unwrap_or_default()
                    )
                })
                .collect();
            CaseRun {
                package: entry.package.clone(),
                case: (*case).to_owned(),
                argv: browsers
                    .iter()
                    .find(|run| !run.argv.is_empty())
                    .map(|run| run.argv.clone())
                    .unwrap_or_default(),
                exit_code: browsers
                    .iter()
                    .find_map(|run| run.exit_code.filter(|code| *code != 0))
                    .or_else(|| browsers.iter().find_map(|run| run.exit_code)),
                result_line: None,
                outcome: worst,
                reason: (!reasons.is_empty()).then(|| reasons.join("; ")),
                duration_s: browsers.iter().map(|run| run.duration_s).sum(),
                log: None,
                notes: browsers
                    .iter()
                    .flat_map(|run| run.notes.iter().cloned())
                    .collect(),
                browsers,
                wasm: None,
            }
        })
        .collect()
}

/// The variable a native case run carries naming its platform route (`windows`, `linux` or
/// `excel`), so that one case can hold a part only the excel route runs: a case runs with Excel
/// only when this says `excel`, never on the windows route of a host that has Excel too.
pub(crate) const PLATFORM_VARIABLE: &str = "DNACALC_ACCEPT_PLATFORM";

/// The variables a native route sets for each case it runs.
fn native_environment(platform: Platform) -> Vec<(&'static str, String)> {
    vec![(PLATFORM_VARIABLE, platform.as_str().to_owned())]
}

/// Runs the cases of `entry` selected for `platform` along its route, or records each as
/// `not_run` with the reason this host cannot start that route: `mismatch` (a host that cannot run
/// the platform at all), or what `probe` reports about Excel, the wasm runner and the drivers.
#[allow(clippy::too_many_arguments)]
fn run_entry(
    ws: &Workspace,
    entry: &BeadEntry,
    cases: &[&str],
    platform: Platform,
    mismatch: Option<&str>,
    probe: &Probe,
    out_dir: &Path,
    problems: &mut Vec<String>,
) -> Vec<CaseRun> {
    let blocked = match (mismatch, platform) {
        (Some(reason), _) => Some(reason.to_owned()),
        (None, Platform::Excel) => (probe.excel)().err(),
        (None, _) => None,
    };
    if let Some(reason) = blocked {
        return cases
            .iter()
            .map(|case| CaseRun::not_run(&entry.package, case, reason.clone()))
            .collect();
    }
    match platform {
        Platform::Wasm => run_wasm(ws, entry, cases, out_dir, probe, problems),
        _ => {
            let environment = native_environment(platform);
            let route = Route {
                label: "",
                extra: &[],
                removed: &[],
                environment: &environment,
                browser: None,
            };
            run_listed(ws, entry, cases, out_dir, &route, problems)
        }
    }
}

/// One registered Acceptance command of the bead and what running it showed.
struct CommandRun {
    argv: Vec<String>,
    outcome: &'static str,
    exit_code: Option<i32>,
    reason: Option<String>,
    started: Option<Vec<String>>,
    result_lines: Vec<String>,
    duration_s: f64,
    log: Option<String>,
}

impl CommandRun {
    fn to_json(&self) -> Value {
        json!({
            "argv": self.argv,
            "outcome": self.outcome,
            "exit_code": self.exit_code,
            "reason": self.reason,
            "started": self.started,
            "environment": {},
            "libtest_result_lines": self.result_lines,
            "duration_s": seconds(self.duration_s),
            "log": self.log,
        })
    }

    fn failed(&self) -> bool {
        self.outcome == "failed"
    }
}

/// Whether a registered command runs this runner itself.
fn is_runner(argv: &[String]) -> bool {
    argv.len() >= 3 && argv[0] == "cargo" && argv[1] == "xtask" && argv[2] == "accept"
}

/// Runs the bead's registered commands, each once in registration order, unless this host cannot
/// run the platform (`blocked`).
fn run_commands(
    ws: &Workspace,
    entries: &[&BeadEntry],
    out_dir: &Path,
    blocked: Option<&str>,
) -> Vec<CommandRun> {
    let mut registered: Vec<&Vec<String>> = Vec::new();
    for argv in entries.iter().flat_map(|entry| &entry.commands) {
        if !registered.contains(&argv) {
            registered.push(argv);
        }
    }
    let mut runs = Vec::new();
    for (index, argv) in registered.into_iter().enumerate() {
        let mut run = CommandRun {
            argv: argv.clone(),
            outcome: "not_run",
            exit_code: None,
            reason: None,
            started: None,
            result_lines: Vec::new(),
            duration_s: 0.0,
            log: None,
        };
        if let Some(reason) = blocked {
            run.reason = Some(reason.to_owned());
        } else if is_runner(argv) {
            run.outcome = "skipped";
            run.reason = Some("the command is this runner itself; it is run on its own".to_owned());
        } else {
            let program = if argv[0] == "cargo" {
                cargo::cargo_program()
            } else {
                argv[0].clone().into()
            };
            let mut command = Command::new(program);
            command.args(&argv[1..]).current_dir(&ws.root);
            println!("ACCEPT running command {}", argv.join(" "));
            let log = out_dir.join(format!("command-{}.log", index + 1));
            match run_logged(ws, command, &log) {
                Err(e) => {
                    run.outcome = "failed";
                    run.reason = Some(e);
                }
                Ok(ran) => {
                    run.outcome = if ran.exit_code == Some(0) {
                        "passed"
                    } else {
                        "failed"
                    };
                    if ran.exit_code != Some(0) {
                        run.reason = Some(format!("exited with {:?}", ran.exit_code));
                    }
                    run.exit_code = ran.exit_code;
                    run.started = Some(ran.argv);
                    run.result_lines = ran
                        .stdout
                        .lines()
                        .map(str::trim)
                        .filter(|line| line.starts_with("test result: "))
                        .map(str::to_owned)
                        .collect();
                    run.duration_s = ran.duration_s;
                    run.log = Some(ran.log);
                }
            }
        }
        runs.push(run);
    }
    runs
}

/// The UTC time as `YYYY-MM-DDThh:mm:ssZ`.
pub(crate) fn utc_now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// The host, toolchain and checkout this run measured. `own` is this run's committed receipt
/// path, which is left out of the dirty paths: the run itself rewrites it.
fn host_record(ws: &Workspace, own: &str, problems: &mut Vec<String>) -> Value {
    let rustc = cargo::rustc_version();
    if let Err(e) = &rustc {
        problems.push(format!("rustc: {e}"));
    }
    let cargo_version = cargo::output(Command::new(cargo::cargo_program()).arg("-V"))
        .map(|out| cargo::text(&out.stdout).trim().to_owned());
    let head = cargo::git(&ws.root, &["rev-parse", "HEAD"]).map(|h| h.trim().to_owned());
    if let Err(e) = &head {
        problems.push(format!("the receipt cannot name the commit: {e}"));
    }
    let branch = cargo::git(&ws.root, &["symbolic-ref", "--short", "-q", "HEAD"])
        .map(|b| b.trim().to_owned())
        .ok();
    let mut dirty = Vec::new();
    let mut excluded = Vec::new();
    match cargo::git(
        &ws.root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    ) {
        Err(e) => problems.push(format!("git status: {e}")),
        Ok(status) => {
            let mut entries = status.split('\0').filter(|e| !e.is_empty());
            while let Some(entry) = entries.next() {
                if entry.len() < 4 {
                    continue;
                }
                let (code, path) = entry.split_at(3);
                if code.starts_with(['R', 'C']) {
                    entries.next();
                }
                if path == own {
                    excluded.push(path.to_owned());
                    continue;
                }
                dirty.push((code.trim().to_owned(), path.to_owned()));
            }
        }
    }
    let present: Vec<&str> = dirty
        .iter()
        .filter(|(_, path)| ws.root.join(path).is_file())
        .map(|(_, path)| path.as_str())
        .collect();
    let mut hashes = std::collections::BTreeMap::new();
    if !present.is_empty() {
        let mut args = vec!["hash-object", "--"];
        args.extend(present.iter().copied());
        match cargo::git(&ws.root, &args) {
            Ok(out) => {
                for (path, hash) in present.iter().zip(out.lines()) {
                    hashes.insert(*path, hash.trim().to_owned());
                }
            }
            Err(e) => problems.push(format!("git hash-object: {e}")),
        }
    }
    let dirty_paths: Vec<Value> = dirty
        .iter()
        .map(|(code, path)| json!({ "path": path, "status": code, "blob": hashes.get(path.as_str()) }))
        .collect();
    json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "legacy_root": crate::legacy::legacy_root_record(&ws.root),
        "rustc": rustc.as_ref().map(|(_, version)| version.clone()).ok(),
        "rustc_host": rustc.as_ref().map(|(host, _)| host.clone()).ok(),
        "cargo": cargo_version.ok(),
        "git": {
            "head": head.ok(),
            "branch": branch,
            "dirty": !dirty_paths.is_empty(),
            "dirty_paths": dirty_paths,
            "receipt_paths_left_out": excluded,
        },
    })
}

fn check_json(check: &CheckResult) -> Value {
    json!({
        "check": check.name,
        "passed": check.passed,
        "summary": check.summary,
        "findings": check.findings,
        "details": check.details,
    })
}

/// Rewrites the absolute paths in receipt strings: the workspace root becomes relative, and the
/// toolchain, cargo home, home and temporary directories become placeholders, so a committed
/// receipt names no directory outside the workspace.
pub(crate) struct Redactor {
    /// Normalised prefixes (forward slashes, lower case on Windows) with their replacements, most
    /// specific first; an empty replacement stands for the workspace root.
    rules: Vec<(String, &'static str)>,
}

/// `text` with backslashes as slashes, and ASCII lower case on Windows; byte offsets are kept.
fn normalised(text: &str) -> String {
    let bytes: Vec<u8> = text
        .bytes()
        .map(|b| match b {
            b'\\' => b'/',
            b if cfg!(windows) => b.to_ascii_lowercase(),
            b => b,
        })
        .collect();
    String::from_utf8(bytes).expect("only ASCII bytes change")
}

impl Redactor {
    pub(crate) fn new(ws: &Workspace) -> Redactor {
        let var = |name: &str| {
            std::env::var_os(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        let home = var(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
        let rustup = var("RUSTUP_HOME").or_else(|| home.as_ref().map(|h| h.join(".rustup")));
        let cargo_home = var("CARGO_HOME").or_else(|| home.as_ref().map(|h| h.join(".cargo")));
        let candidates = [
            (Some(ws.root.clone()), ""),
            (rustup, "<rustup>"),
            (cargo_home, "<cargo-home>"),
            (Some(std::env::temp_dir()), "<temp>"),
            (home, "<home>"),
        ];
        let rules = candidates
            .into_iter()
            .filter_map(|(path, replacement)| {
                let prefix = normalised(&path?.to_string_lossy());
                let prefix = prefix.trim_end_matches('/').to_owned();
                (prefix.len() > 3).then_some((prefix, replacement))
            })
            .collect();
        Redactor { rules }
    }

    fn apply(&self, text: &str) -> String {
        let mut out = text.to_owned();
        let mut changed = false;
        for (prefix, replacement) in &self.rules {
            let mut from = 0;
            loop {
                let norm = normalised(&out);
                let Some(found) = norm[from..].find(prefix.as_str()) else {
                    break;
                };
                let start = from + found;
                let end = start + prefix.len();
                let next = norm.as_bytes().get(end).copied();
                let continues_name = next.is_some_and(|b| {
                    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'~')
                });
                if continues_name {
                    from = end;
                    continue;
                }
                let (cut, with) = match (*replacement, next) {
                    ("", Some(b'/')) => (end + 1, ""),
                    ("", _) => (end, "."),
                    (other, _) => (end, other),
                };
                out.replace_range(start..cut, with);
                from = start + with.len();
                changed = true;
            }
        }
        if changed { out.replace('\\', "/") } else { out }
    }

    pub(crate) fn apply_value(&self, value: &mut Value) {
        match value {
            Value::String(text) => *text = self.apply(text),
            Value::Array(items) => items.iter_mut().for_each(|item| self.apply_value(item)),
            Value::Object(fields) => fields.values_mut().for_each(|item| self.apply_value(item)),
            _ => {}
        }
    }
}

/// The line the runner prints for one case, or for one browser's run of a wasm case, which also
/// names the browser the runner's banner named.
fn print_case_line(run: &CaseRun) {
    let code = run
        .exit_code
        .map_or_else(|| "none".to_owned(), |code| code.to_string());
    let ran_in = run
        .wasm
        .as_ref()
        .and_then(|wasm| wasm.executed_in.as_deref())
        .map(|browser| format!(", ran in {browser}"))
        .unwrap_or_default();
    match &run.reason {
        Some(reason) => println!(
            "ACCEPT {} {} (package {}, exit code {code}{ran_in}): {reason}",
            run.outcome.as_str(),
            run.case,
            run.package
        ),
        None => println!(
            "ACCEPT {} {} (package {}, exit code {code}{ran_in})",
            run.outcome.as_str(),
            run.case,
            run.package
        ),
    }
}

pub fn run(raw: &[String]) -> ExitCode {
    let args = match parse_args(raw) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("xtask accept: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let started = Instant::now();
    let started_utc = utc_now();
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("xtask accept: no current directory: {e}");
            return ExitCode::from(1);
        }
    };
    let ws = match Workspace::load(&cwd) {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("xtask accept: cannot read the workspace: {e}");
            return ExitCode::from(1);
        }
    };
    let platform = args.platform;
    let out_dir = ws
        .root
        .join("target")
        .join("accept")
        .join(&args.id)
        .join(platform.as_str());
    if out_dir.exists()
        && let Err(e) = fs::remove_dir_all(&out_dir)
    {
        eprintln!("xtask accept: cannot clear {}: {e}", out_dir.display());
        return ExitCode::from(1);
    }
    if let Err(e) = fs::create_dir_all(&out_dir) {
        eprintln!("xtask accept: cannot create {}: {e}", out_dir.display());
        return ExitCode::from(1);
    }
    println!(
        "ACCEPT {} on {platform}{}: workspace {}, cargo target directory {}",
        args.id,
        if args.require_all {
            " --require-all"
        } else {
            ""
        },
        ws.root.display(),
        cargo::TARGET_DIR
    );

    let committed_rel = format!("{RECEIPTS_DIR}/{}/{platform}.json", args.id);
    let mut problems = Vec::new();
    let mut host = host_record(&ws, &committed_rel, &mut problems);
    let manifests = manifest::load(&ws);
    problems.extend(manifests.errors.iter().map(|e| format!("manifest: {e}")));

    let checks = arch::run_all(&ws, &out_dir.join("arch"));
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
    }

    let entries: Vec<&BeadEntry> = manifests
        .entries
        .iter()
        .filter(|entry| entry.id == args.id)
        .collect();
    let known = !entries.is_empty();
    if !known {
        problems.push(format!("no acceptance manifest registers {}", args.id));
    }
    let mismatch = host_mismatch(platform);
    host["platform_matches_host"] = json!(mismatch.is_none());
    host["platform_mismatch"] = json!(mismatch);
    if let Some(reason) = &mismatch {
        println!("ACCEPT host cannot run {platform}: {reason}; no command or case runs");
    }

    let commands = run_commands(&ws, &entries, &out_dir, mismatch.as_deref());
    for command in &commands {
        let code = command
            .exit_code
            .map_or_else(|| "none".to_owned(), |code| code.to_string());
        println!(
            "ACCEPT command {} (exit code {code}): {}{}",
            command.outcome,
            command.argv.join(" "),
            command
                .reason
                .as_ref()
                .map(|reason| format!(" ({reason})"))
                .unwrap_or_default()
        );
    }

    let mut runs: Vec<CaseRun> = Vec::new();
    let mut selected = 0;
    for entry in &entries {
        let cases: Vec<&str> = entry
            .cases
            .iter()
            .filter(|case| case.platforms.contains(&platform))
            .map(|case| case.name.as_str())
            .collect();
        selected += cases.len();
        if cases.is_empty() {
            continue;
        }
        runs.extend(run_entry(
            &ws,
            entry,
            &cases,
            platform,
            mismatch.as_deref(),
            &Probe::HOST,
            &out_dir,
            &mut problems,
        ));
    }
    if known && selected == 0 {
        problems.push(format!(
            "no case of {} declares platform {platform}",
            args.id
        ));
    }

    for run in &runs {
        print_case_line(run);
        for browser in &run.browsers {
            print_case_line(browser);
        }
        for note in &run.notes {
            println!("ACCEPT note {}: {note}", run.case);
        }
    }
    for problem in &problems {
        println!("ACCEPT problem: {problem}");
    }

    let count = |outcome: Outcome| runs.iter().filter(|run| run.outcome == outcome).count();
    let checks_passed = checks.iter().filter(|check| check.passed).count();
    let commands_failed = commands.iter().filter(|command| command.failed()).count();
    let commands_with = |outcome: &str| commands.iter().filter(|c| c.outcome == outcome).count();
    let commands_run = commands_with("passed") + commands_failed;
    let failed = !problems.is_empty()
        || checks_passed != checks.len()
        || commands_failed > 0
        || runs.is_empty()
        || runs.iter().any(|run| {
            matches!(
                run.outcome,
                Outcome::Failed | Outcome::Missing | Outcome::Ignored
            )
        });
    let result = if failed {
        "failed"
    } else if count(Outcome::NotRun) > 0 {
        "not_run"
    } else {
        "passed"
    };

    let receipt_path = out_dir.join("receipt.json");
    let committed = ws.root.join(&committed_rel);
    let command: Vec<String> = ["cargo", "xtask", "accept"]
        .iter()
        .map(|arg| (*arg).to_owned())
        .chain(raw.iter().cloned())
        .collect();
    let mut exit_code = match result {
        "passed" => 0,
        "not_run" if !args.require_all => 0,
        _ => 1,
    };
    let mut receipt = json!({
        "format": "dnacalc-accept-receipt/4",
        "bead": args.id,
        "platform": platform.as_str(),
        "require_all": args.require_all,
        "result": result,
        "exit_code": exit_code,
        "result_rule": "failed when a manifest, an architecture check or a command failed, the bead is unknown, no case declares the platform, or a selected case is failed, missing or ignored; otherwise not_run when a selected case is not_run (every case is not_run on a host that cannot run the platform); otherwise passed. On wasm a case runs in each browser with WASM_BINDGEN_USE_BROWSER set, a browser's run is passed only when wasm-bindgen-test-runner's first line names that browser, and the case takes its worst browser outcome. The run exits 0 for passed, and for not_run only without --require-all.",
        "paths": "relative to the workspace root; <rustup>, <cargo-home>, <home> and <temp> stand for directories outside it, and host.legacy_root.path is .. for the workspace root's parent or <legacy-root> for a directory elsewhere",
        "command": command,
        "started_utc": started_utc,
        "duration_s": seconds(started.elapsed().as_secs_f64()),
        "host": host,
        "cargo_target_dir": cargo::TARGET_DIR,
        "manifests": {
            "files": manifests.files.iter().map(|path| ws.rel(path)).collect::<Vec<_>>(),
            "errors": manifests.errors,
        },
        "architecture": checks.iter().map(check_json).collect::<Vec<_>>(),
        "commands": commands.iter().map(CommandRun::to_json).collect::<Vec<_>>(),
        "cases": runs.iter().map(CaseRun::to_json).collect::<Vec<_>>(),
        "counts": {
            "selected": selected,
            "passed": count(Outcome::Passed),
            "failed": count(Outcome::Failed),
            "not_run": count(Outcome::NotRun),
            "missing": count(Outcome::Missing),
            "ignored": count(Outcome::Ignored),
            "commands_registered": commands.len(),
            "commands_run": commands_run,
            "commands_failed": commands_failed,
            "commands_skipped": commands_with("skipped"),
            "commands_not_run": commands_with("not_run"),
            "architecture_checks_passed": checks_passed,
            "architecture_checks": checks.len(),
        },
        "problems": problems,
        "receipt": ws.rel(&receipt_path),
        "committed_copy": known.then(|| committed_rel.clone()),
    });
    Redactor::new(&ws).apply_value(&mut receipt);
    let text = serde_json::to_string_pretty(&receipt).unwrap_or_default() + "\n";
    let mut written = fs::write(&receipt_path, &text).map_err(|e| e.to_string());
    if written.is_ok() && known {
        written = committed
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(&committed, &text))
            .map_err(|e| e.to_string());
    }
    if let Err(e) = &written {
        println!("ACCEPT problem: cannot write the receipt: {e}");
        exit_code = 1;
    }
    println!(
        "ACCEPT RESULT {result}: {} on {platform}{}: {} selected, {} passed, {} failed, {} not run, {} missing, {} ignored; commands: {} registered, {commands_run} run, {commands_failed} failed, {} skipped, {} not run; architecture checks {checks_passed}/{} passed; exit code {exit_code}; receipt {}{}",
        args.id,
        if args.require_all {
            " --require-all"
        } else {
            ""
        },
        selected,
        count(Outcome::Passed),
        count(Outcome::Failed),
        count(Outcome::NotRun),
        count(Outcome::Missing),
        count(Outcome::Ignored),
        commands.len(),
        commands_with("skipped"),
        commands_with("not_run"),
        checks.len(),
        ws.rel(&receipt_path),
        if known {
            format!(" (copy {committed_rel})")
        } else {
            " (no committed copy for an unknown bead)".to_owned()
        }
    );
    ExitCode::from(exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libtest_results_are_classified() {
        let ok = "running 1 test\ntest c ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.01s\n";
        assert_eq!(classify("c", ok, "", Some(0)).0, Outcome::Passed);
        assert_eq!(classify("c", ok, "", Some(101)).0, Outcome::Failed);
        let marker = format!(
            "running 1 test\ntest c ... {NOT_RUN_MARKER}c: no Excel\nok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n"
        );
        let (outcome, _, reason) = classify("c", &marker, "", Some(0));
        assert_eq!(
            (outcome, reason.as_deref()),
            (Outcome::NotRun, Some("no Excel"))
        );
        assert_eq!(classify("other", &marker, "", Some(0)).0, Outcome::Passed);
        // The marker on standard error counts the same as on standard output.
        let on_stderr = format!("{NOT_RUN_MARKER}c: no browser driver\n");
        let (outcome, _, reason) = classify("c", ok, &on_stderr, Some(0));
        assert_eq!(
            (outcome, reason.as_deref()),
            (Outcome::NotRun, Some("no browser driver"))
        );
        let zero = "running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 5 filtered out; finished in 0.00s\n";
        assert_eq!(classify("c", zero, "", Some(0)).0, Outcome::Missing);
        let ignored = "running 1 test\ntest c ... ignored\n\ntest result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s\n";
        assert_eq!(classify("c", ignored, "", Some(0)).0, Outcome::Ignored);
        let failed = "running 1 test\ntest c ... FAILED\n\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n";
        assert_eq!(classify("c", failed, "", Some(101)).0, Outcome::Failed);
        assert_eq!(classify("c", "", "", Some(101)).0, Outcome::Failed);
        assert_eq!(
            classify("c", &format!("{ok}{ok}"), "", Some(0)).0,
            Outcome::Failed
        );
    }

    #[test]
    fn listing_lines_give_exact_names() {
        let listing = "a__one: test\na__two: test\nb: bench\n\n2 tests, 1 benchmark\n";
        assert_eq!(listed_names(listing), ["a__one", "a__two"]);
    }

    #[test]
    fn arguments_are_checked() {
        let args =
            |list: &[&str]| parse_args(&list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
        let parsed = args(&["--id", "infra-m0.3", "--platform", "linux", "--require-all"]).unwrap();
        assert_eq!(
            (parsed.id.as_str(), parsed.platform, parsed.require_all),
            ("infra-m0.3", Platform::Linux, true)
        );
        assert!(args(&["--id", "infra-m0.3", "--platform", "macos"]).is_err());
        assert!(args(&["--id", "../x", "--platform", "linux"]).is_err());
        assert!(args(&["--platform", "linux"]).is_err());
        assert!(args(&["--id", "x", "--platform", "linux", "--all"]).is_err());
    }

    #[test]
    fn utc_dates_are_civil() {
        let now = utc_now();
        assert_eq!(now.len(), 20, "{now}");
        assert!(now.starts_with("20") && now.ends_with('Z'), "{now}");
    }

    #[test]
    fn the_runner_command_is_recognised() {
        let argv = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(is_runner(&argv(&["cargo", "xtask", "accept", "--id", "x"])));
        assert!(!is_runner(&argv(&["cargo", "test", "-p", "xtask"])));
        assert!(!is_runner(&argv(&["cargo", "xtask"])));
    }

    #[test]
    fn receipt_paths_are_relative_or_redacted() {
        let root = if cfg!(windows) {
            r"C:\Work\Repo"
        } else {
            "/work/repo"
        };
        let home = if cfg!(windows) {
            r"C:\Users\someone"
        } else {
            "/home/someone"
        };
        let redactor = Redactor {
            rules: vec![
                (normalised(root), ""),
                (normalised(&format!("{home}/.rustup")), "<rustup>"),
                (normalised(home), "<home>"),
            ],
        };
        let sep = if cfg!(windows) { r"\" } else { "/" };
        assert_eq!(redactor.apply(&format!("{root}{sep}target")), "target");
        assert_eq!(
            redactor.apply(&format!(
                "{home}{sep}.rustup{sep}toolchains{sep}stable{sep}bin{sep}cargo"
            )),
            "<rustup>/toolchains/stable/bin/cargo"
        );
        assert_eq!(
            redactor.apply(&format!("cannot read {home}{sep}notes.txt: denied")),
            "cannot read <home>/notes.txt: denied"
        );
        assert_eq!(redactor.apply(root), ".");
        // A sibling directory whose name only starts with the root's is not the root.
        assert_eq!(
            redactor.apply(&format!("{root}-Lane{sep}x")),
            format!("{root}-Lane{sep}x")
        );
        assert_eq!(
            redactor.apply(r"HKCR\Excel.Application\CLSID"),
            r"HKCR\Excel.Application\CLSID"
        );
    }

    // The not_run branches of the excel and wasm routes, each reached through `run_entry`, the
    // function `run` calls, with an injected probe. None of them may start a process: the
    // workspace root and the output directory do not exist, so a cargo run would fail the case
    // instead of leaving it not_run.

    const CASE: &str = "fixture_probe__alpha";
    const RUNNER: &str = "/probe/bin/wasm-bindgen-test-runner";
    const EXCEL_ABSENT: &str = r"Excel is not registered for automation on this host (HKCR\Excel.Application\CLSID is absent)";

    fn probe_workspace() -> (Workspace, PathBuf) {
        let root = std::env::temp_dir().join("xtask-probe-workspace-that-does-not-exist");
        let out_dir = root.join("target").join("accept");
        (
            Workspace {
                root,
                members: Vec::new(),
            },
            out_dir,
        )
    }

    fn probe_entry(platform: Platform) -> BeadEntry {
        BeadEntry {
            package: "probe-package".to_owned(),
            id: "fixture-probe".to_owned(),
            test: "acceptance".to_owned(),
            cases: vec![manifest::CaseDecl {
                name: CASE.to_owned(),
                platforms: vec![platform],
            }],
            commands: Vec::new(),
        }
    }

    /// Runs the probe entry's one case on `platform` with `probe`, checking that nothing started.
    fn run_probed(platform: Platform, mismatch: Option<&str>, probe: &Probe) -> CaseRun {
        let (ws, out_dir) = probe_workspace();
        let mut problems = Vec::new();
        let mut runs = run_entry(
            &ws,
            &probe_entry(platform),
            &[CASE],
            platform,
            mismatch,
            probe,
            &out_dir,
            &mut problems,
        );
        assert!(problems.is_empty(), "{problems:?}");
        assert!(
            !out_dir.exists(),
            "a probed not_run branch wrote {out_dir:?}"
        );
        assert_eq!(runs.len(), 1);
        let run = runs.remove(0);
        for started in std::iter::once(&run).chain(&run.browsers) {
            assert!(
                started.argv.is_empty() && started.exit_code.is_none() && started.log.is_none(),
                "{} started a process: {:?}",
                started.case,
                started.argv
            );
        }
        run
    }

    fn excel_absent() -> Result<(), String> {
        Err(EXCEL_ABSENT.to_owned())
    }

    fn never_excel() -> Result<(), String> {
        panic!("the excel probe must not be read here")
    }

    fn nothing(_: &str) -> Option<PathBuf> {
        None
    }

    fn never_path(name: &str) -> Option<PathBuf> {
        panic!("the PATH probe must not be read here ({name})")
    }

    fn runner_only(program: &str) -> Option<PathBuf> {
        (program == "wasm-bindgen-test-runner").then(|| PathBuf::from(RUNNER))
    }

    /// The (browser, outcome, reason) of each browser's run of a wasm case.
    fn browser_outcomes(run: &CaseRun) -> Vec<(&str, Outcome, &str)> {
        run.browsers
            .iter()
            .map(|browser| {
                (
                    browser.wasm.as_ref().map_or("", |wasm| wasm.browser),
                    browser.outcome,
                    browser.reason.as_deref().unwrap_or_default(),
                )
            })
            .collect()
    }

    #[test]
    fn excel_not_registered_is_not_run() {
        let probe = Probe {
            excel: excel_absent,
            on_path: never_path,
            variable_file: never_path,
        };
        let run = run_probed(Platform::Excel, None, &probe);
        assert_eq!(
            (run.case.as_str(), run.outcome, run.reason.as_deref()),
            (CASE, Outcome::NotRun, Some(EXCEL_ABSENT))
        );
        let json = run.to_json();
        assert_eq!(json["outcome"], "not_run");
        assert_eq!(json["reason"], EXCEL_ABSENT);
        assert!(json.get("browser").is_none() && json.get("browsers").is_none());
    }

    #[test]
    fn a_host_that_cannot_run_the_platform_reads_no_probe() {
        let probe = Probe {
            excel: never_excel,
            on_path: never_path,
            variable_file: never_path,
        };
        for platform in [Platform::Excel, Platform::Wasm] {
            let run = run_probed(platform, Some("this host cannot run it"), &probe);
            assert_eq!(
                (run.outcome, run.reason.as_deref()),
                (Outcome::NotRun, Some("this host cannot run it"))
            );
        }
    }

    #[test]
    fn wasm_runner_missing_is_not_run_in_both_browsers() {
        // The drivers are there; without the runner neither browser can run.
        let probe = Probe {
            excel: never_excel,
            on_path: |program| {
                (program != "wasm-bindgen-test-runner")
                    .then(|| PathBuf::from(format!("/probe/bin/{program}")))
            },
            variable_file: nothing,
        };
        let run = run_probed(Platform::Wasm, None, &probe);
        let missing = "wasm-bindgen-test-runner is not on PATH";
        assert_eq!(
            browser_outcomes(&run),
            [
                ("chromium", Outcome::NotRun, missing),
                ("firefox", Outcome::NotRun, missing)
            ]
        );
        assert_eq!(run.outcome, Outcome::NotRun);
        assert_eq!(
            run.reason.as_deref(),
            Some(
                "chromium: not_run (wasm-bindgen-test-runner is not on PATH); firefox: not_run (wasm-bindgen-test-runner is not on PATH)"
            )
        );
        let json = run.to_json();
        assert_eq!(json["browsers"][0]["case"], format!("{CASE}@chromium"));
        assert_eq!(json["browsers"][1]["case"], format!("{CASE}@firefox"));
        for browser in json["browsers"].as_array().unwrap() {
            assert_eq!(browser["environment"], json!({}));
            assert!(browser["executed_in"].is_null() && browser["runner_banner"].is_null());
        }
    }

    #[test]
    fn wasm_drivers_missing_are_not_run_each_with_its_reason() {
        let probe = Probe {
            excel: never_excel,
            on_path: runner_only,
            variable_file: nothing,
        };
        let run = run_probed(Platform::Wasm, None, &probe);
        assert_eq!(
            browser_outcomes(&run),
            [
                (
                    "chromium",
                    Outcome::NotRun,
                    "chromedriver is not on PATH and CHROMEDRIVER names no file"
                ),
                (
                    "firefox",
                    Outcome::NotRun,
                    "geckodriver is not on PATH and GECKODRIVER names no file"
                )
            ]
        );
        assert_eq!(run.outcome, Outcome::NotRun);
    }

    #[test]
    fn each_missing_driver_stops_only_its_own_browser() {
        // chromedriver missing, geckodriver on PATH: firefox gets a route, chromium does not.
        let probe = Probe {
            excel: never_excel,
            on_path: |program| match program {
                "wasm-bindgen-test-runner" => Some(PathBuf::from(RUNNER)),
                "geckodriver" => Some(PathBuf::from("/probe/bin/geckodriver")),
                _ => None,
            },
            variable_file: nothing,
        };
        let routes = wasm_routes(&probe);
        assert_eq!(routes[0].0.name, "chromium");
        assert_eq!(
            routes[0].1,
            Err("chromedriver is not on PATH and CHROMEDRIVER names no file".to_owned())
        );
        assert_eq!(routes[1].0.name, "firefox");
        assert_eq!(
            routes[1].1,
            Ok(vec![
                (WASM_RUNNER_VARIABLE, RUNNER.to_owned()),
                ("GECKODRIVER", "/probe/bin/geckodriver".to_owned()),
                (FORCE_BROWSER, "1".to_owned()),
            ])
        );
        // geckodriver missing, CHROMEDRIVER naming a file (it wins over PATH): the reverse.
        let probe = Probe {
            excel: never_excel,
            on_path: |program| match program {
                "wasm-bindgen-test-runner" => Some(PathBuf::from(RUNNER)),
                "chromedriver" => Some(PathBuf::from("/probe/path/chromedriver")),
                _ => None,
            },
            variable_file: |variable| {
                (variable == "CHROMEDRIVER").then(|| PathBuf::from("/probe/variable/chromedriver"))
            },
        };
        let routes = wasm_routes(&probe);
        assert_eq!(
            routes[0].1,
            Ok(vec![
                (WASM_RUNNER_VARIABLE, RUNNER.to_owned()),
                ("CHROMEDRIVER", "/probe/variable/chromedriver".to_owned()),
                (FORCE_BROWSER, "1".to_owned()),
            ])
        );
        assert_eq!(
            routes[1].1,
            Err("geckodriver is not on PATH and GECKODRIVER names no file".to_owned())
        );
    }

    #[test]
    fn the_wasm_route_removes_every_other_driver_and_mode() {
        for browser in &BROWSERS {
            assert!(WASM_REMOVED.contains(&browser.variable));
        }
        for variable in [
            "SAFARIDRIVER",
            "NO_HEADLESS",
            "WASM_BINDGEN_USE_NODE_EXPERIMENTAL",
            "WASM_BINDGEN_USE_DENO",
            "WASM_BINDGEN_TEST_ONLY_NODE",
        ] {
            assert!(WASM_REMOVED.contains(&variable), "{variable}");
        }
        assert!(!WASM_REMOVED.contains(&FORCE_BROWSER));
    }

    #[test]
    fn only_the_runner_banner_names_the_browser() {
        let chrome = "Running headless tests in Chrome on `http://127.0.0.1:60524/`\nTry find `webdriver.json` for configure browser's capabilities:\nNot found\n\nrunning 1 test\ntest c ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 1 filtered out; finished in 0.01s\n";
        let node = "running 1 test\nInvoking test: c\nFIXTURE-AGENT: Node.js/25\ntest c ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out; finished in 0.01s\n";
        // A test that prints the banner text itself is not the runner's banner.
        let printed = "running 1 test\nRunning headless tests in Chrome on `http://x/`\ntest c ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out; finished in 0.01s\n";
        assert_eq!(
            runner_banner(chrome),
            Some((
                "Chrome".to_owned(),
                "Running headless tests in Chrome on `http://127.0.0.1:60524/`".to_owned()
            ))
        );
        assert_eq!(runner_banner(node), None);
        assert_eq!(runner_banner(printed), None);

        let passed = (Outcome::Passed, None);
        assert_eq!(browser_checked("Chrome", chrome, passed.clone()), passed);
        let (outcome, reason) = browser_checked("Firefox", chrome, passed.clone());
        assert_eq!(outcome, Outcome::Failed);
        assert!(
            reason
                .unwrap()
                .contains("ran the case in Chrome, not Firefox")
        );
        for stdout in [node, printed] {
            let (outcome, reason) = browser_checked("Chrome", stdout, passed.clone());
            assert_eq!(outcome, Outcome::Failed);
            assert!(
                reason
                    .unwrap()
                    .contains("does not start with `Running headless tests in Chrome on`")
            );
        }
        // Outcomes other than passed are kept as they are.
        let not_run = (Outcome::NotRun, Some("no device".to_owned()));
        assert_eq!(browser_checked("Chrome", node, not_run.clone()), not_run);
        let failed = (Outcome::Failed, Some("libtest".to_owned()));
        assert_eq!(browser_checked("Chrome", node, failed.clone()), failed);
    }

    #[test]
    fn native_routes_name_their_platform_to_the_case() {
        for platform in [Platform::Windows, Platform::Linux, Platform::Excel] {
            assert_eq!(
                native_environment(platform),
                [("DNACALC_ACCEPT_PLATFORM", platform.as_str().to_owned())]
            );
        }
        let environment = native_environment(Platform::Windows);
        let route = Route {
            label: "",
            extra: &[],
            removed: &[],
            environment: &environment,
            browser: None,
        };
        let mut command = Command::new("cargo");
        route.apply(&mut command);
        let set: Vec<(String, Option<String>)> = command
            .get_envs()
            .map(|(name, value)| {
                (
                    name.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect();
        assert_eq!(
            set,
            [(PLATFORM_VARIABLE.to_owned(), Some("windows".to_owned()))]
        );
    }

    #[test]
    fn notes_are_kept_per_case() {
        let stdout = format!("x\n{NOTE_MARKER}c: first\n{NOTE_MARKER}other: not mine\n");
        let stderr = format!("prefix {NOTE_MARKER}c: second  \n");
        assert_eq!(case_notes("c", &stdout, &stderr), ["first", "second"]);
        assert!(case_notes("c", "nothing\n", "").is_empty());
    }
}
