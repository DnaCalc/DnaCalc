//! `cargo xtask accept --id <bead> --platform windows|linux|wasm|excel [--require-all]`
//!
//! 1. Finds every workspace member's `acceptance.json` through `cargo metadata` and checks them
//!    all (`manifest.rs`).
//! 2. Runs the architecture checks (`arch.rs`), whatever the bead.
//! 3. Runs each Acceptance command the manifests register for the bead, once, from the workspace
//!    root with `CARGO_TARGET_DIR=target/xtask-accept`, keeping argv, exit code and log; a command
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
//!    manifest does not register fails the run.
//! 6. Writes the receipt to `target/accept/<bead>/<platform>/receipt.json`, beside one log per
//!    cargo run, and, for a bead some manifest registers, copies it to
//!    `accept-receipts/<bead>/<platform>.json`. Paths in the receipt are relative to the workspace
//!    root; the toolchain, cargo home, home and temporary directories appear as `<rustup>`,
//!    `<cargo-home>`, `<home>` and `<temp>`. The host record names the legacy root that
//!    `pin-sources` reads as `..` (the workspace root's parent) or `<legacy-root>` (elsewhere,
//!    from `DNACALC_LEGACY_ROOT`). Format `dnacalc-accept-receipt/3` added that record.
//!
//! The result: `failed` when a manifest, an architecture check or a command failed, the bead is
//! unknown, no case declares the platform, or any selected case is `failed`, `missing` or
//! `ignored`; otherwise `not_run` when any selected case is `not_run`; otherwise `passed`. A host
//! that cannot run the platform (`windows` off Windows, `linux` off Linux, `excel` off Windows)
//! runs no command and no case: every case is `not_run` with the reason, so such a run never
//! records `passed`. The run exits 0 for `passed`, and for `not_run` only without
//! `--require-all`; it exits 1 otherwise. Every cargo started here uses `target/xtask-accept`.
//!
//! Platform routes: `windows` and `linux` run the cases natively on a host of that system; `excel`
//! runs them natively on a Windows host where Excel is registered for automation, and records
//! them as `not_run` where it is not; `wasm` runs the acceptance target for wasm32-unknown-unknown
//! under wasm-bindgen-test-runner once with chromedriver and once with geckodriver, and records a
//! browser whose runner or driver is missing as `not_run`.

use crate::arch::{self, CheckResult};
use crate::cargo::{self, WASM_TARGET, Workspace};
use crate::manifest::{self, BeadEntry, Platform};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const USAGE: &str =
    "usage: cargo xtask accept --id <bead> --platform windows|linux|wasm|excel [--require-all]";

/// The prefix of the line a case prints when it cannot run on this host.
const NOT_RUN_MARKER: &str = "ACCEPT-NOT-RUN: ";

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
    browsers: Vec<CaseRun>,
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
            browsers: Vec::new(),
        }
    }

    fn to_json(&self, browser: Option<&str>) -> Value {
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
        });
        if let Some(browser) = browser {
            value["browser"] = json!(browser);
        }
        if !self.browsers.is_empty() {
            value["browsers"] = Value::Array(
                self.browsers
                    .iter()
                    .map(|run| run.to_json(run.reason_browser()))
                    .collect(),
            );
        }
        value
    }

    /// The browser a wasm sub-run belongs to, stored in `case` as `<case>@<browser>`.
    fn reason_browser(&self) -> Option<&str> {
        self.case.rsplit_once('@').map(|(_, browser)| browser)
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

/// `cargo test -p <package> --test <target> --locked --target-dir target/xtask-accept [extra]
/// -- [libtest]`, run from the workspace root.
fn cargo_test(ws: &Workspace, entry: &BeadEntry, extra: &[&str], libtest: &[&str]) -> Command {
    let mut command = Command::new(cargo::cargo_program());
    command
        .args([
            "test",
            "-p",
            &entry.package,
            "--test",
            &entry.test,
            "--locked",
        ])
        .arg("--target-dir")
        .arg(ws.accept_target_dir())
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
/// arguments (a `--target`), and `configure` adds the environment.
struct Route<'a> {
    label: &'a str,
    extra: &'a [&'a str],
    configure: &'a dyn Fn(&mut Command),
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
    (route.configure)(&mut list);
    let list_log = out_dir.join(format!("{}-{}-list{label}.log", entry.package, entry.test));
    let listed = match run_logged(ws, list, &list_log) {
        Err(e) => Err(e),
        Ok(ran) if ran.exit_code != Some(0) => Err(format!(
            "listing {} --test {} exited with {:?}; see {}",
            entry.package, entry.test, ran.exit_code, ran.log
        )),
        Ok(ran) => Ok((listed_names(&ran.stdout), ran)),
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
                browsers: Vec::new(),
            });
            continue;
        }
        println!("ACCEPT running {case} (package {}){label}", entry.package);
        let mut command = cargo_test(ws, entry, extra, &["--exact", case, "--nocapture"]);
        (route.configure)(&mut command);
        let log = out_dir.join(format!("{case}{label}.log"));
        match run_logged(ws, command, &log) {
            Err(e) => {
                let mut run = CaseRun::not_run(&entry.package, case, e);
                run.outcome = Outcome::Failed;
                runs.push(run);
            }
            Ok(ran) => {
                let (outcome, result_line, reason) =
                    classify(case, &ran.stdout, &ran.stderr, ran.exit_code);
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
                    browsers: Vec::new(),
                });
            }
        }
    }
    runs
}

/// Whether Excel is registered for automation on this host, or why the excel route cannot run.
fn excel_available() -> Result<(), String> {
    let key = r"HKCR\Excel.Application\CLSID";
    match Command::new("reg").args(["query", key]).output() {
        Ok(out) if out.status.success() => Ok(()),
        Ok(_) => Err(format!(
            "Excel is not registered for automation on this host ({key} is absent)"
        )),
        Err(e) => Err(format!("cannot run reg query {key}: {e}")),
    }
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

fn find_on_path(program: &str) -> Option<PathBuf> {
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

/// The browsers of the wasm route: name, driver variable and driver program.
const BROWSERS: [(&str, &str, &str); 2] = [
    ("chromium", "CHROMEDRIVER", "chromedriver"),
    ("firefox", "GECKODRIVER", "geckodriver"),
];
const DRIVER_VARIABLES: [&str; 3] = ["CHROMEDRIVER", "GECKODRIVER", "SAFARIDRIVER"];

fn run_wasm(
    ws: &Workspace,
    entry: &BeadEntry,
    cases: &[&str],
    out_dir: &Path,
    problems: &mut Vec<String>,
) -> Vec<CaseRun> {
    let runner = find_on_path("wasm-bindgen-test-runner");
    let mut per_browser: Vec<Vec<CaseRun>> = Vec::new();
    for (browser, variable, program) in BROWSERS {
        let driver = std::env::var_os(variable)
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .or_else(|| find_on_path(program));
        let runs = match (&runner, driver) {
            (None, _) => cases
                .iter()
                .map(|case| {
                    CaseRun::not_run(
                        &entry.package,
                        &format!("{case}@{browser}"),
                        "wasm-bindgen-test-runner is not on PATH".to_owned(),
                    )
                })
                .collect(),
            (Some(_), None) => cases
                .iter()
                .map(|case| {
                    CaseRun::not_run(
                        &entry.package,
                        &format!("{case}@{browser}"),
                        format!("{program} is not on PATH and {variable} names no file"),
                    )
                })
                .collect(),
            (Some(runner), Some(driver)) => {
                let configure = |command: &mut Command| {
                    for name in DRIVER_VARIABLES {
                        command.env_remove(name);
                    }
                    command
                        .env("CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER", runner)
                        .env(variable, &driver);
                };
                let label = format!("@{browser}");
                let route = Route {
                    label: &label,
                    extra: &["--target", WASM_TARGET],
                    configure: &configure,
                };
                let mut runs = run_listed(ws, entry, cases, out_dir, &route, problems);
                for run in &mut runs {
                    run.case = format!("{}@{browser}", run.case);
                }
                runs
            }
        };
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
                        run.reason_browser().unwrap_or_default(),
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
                browsers,
            }
        })
        .collect()
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
            "environment": { "CARGO_TARGET_DIR": cargo::ACCEPT_TARGET_DIR },
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
            command
                .args(&argv[1..])
                .env("CARGO_TARGET_DIR", ws.accept_target_dir())
                .current_dir(&ws.root);
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
        "legacy_root": crate::pin::legacy_root_record(&ws.root),
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
struct Redactor {
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
    fn new(ws: &Workspace) -> Redactor {
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

    fn apply_value(&self, value: &mut Value) {
        match value {
            Value::String(text) => *text = self.apply(text),
            Value::Array(items) => items.iter_mut().for_each(|item| self.apply_value(item)),
            Value::Object(fields) => fields.values_mut().for_each(|item| self.apply_value(item)),
            _ => {}
        }
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
        cargo::ACCEPT_TARGET_DIR
    );

    let committed_rel = format!("{RECEIPTS_DIR}/{}/{platform}.json", args.id);
    let mut problems = Vec::new();
    let mut host = host_record(&ws, &committed_rel, &mut problems);
    let manifests = manifest::load(&ws);
    problems.extend(manifests.errors.iter().map(|e| format!("manifest: {e}")));

    let checks = arch::run_all(&ws, &manifests, &out_dir.join("arch"));
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
        let blocked = match platform {
            _ if mismatch.is_some() => mismatch.clone(),
            Platform::Excel => excel_available().err(),
            _ => None,
        };
        if let Some(reason) = blocked {
            runs.extend(
                cases
                    .iter()
                    .map(|case| CaseRun::not_run(&entry.package, case, reason.clone())),
            );
            continue;
        }
        let found = match platform {
            Platform::Wasm => run_wasm(&ws, entry, &cases, &out_dir, &mut problems),
            _ => {
                let route = Route {
                    label: "",
                    extra: &[],
                    configure: &|_| {},
                };
                run_listed(&ws, entry, &cases, &out_dir, &route, &mut problems)
            }
        };
        runs.extend(found);
    }
    if known && selected == 0 {
        problems.push(format!(
            "no case of {} declares platform {platform}",
            args.id
        ));
    }

    for run in &runs {
        let code = run
            .exit_code
            .map_or_else(|| "none".to_owned(), |code| code.to_string());
        match &run.reason {
            Some(reason) => println!(
                "ACCEPT {} {} (package {}, exit code {code}): {reason}",
                run.outcome.as_str(),
                run.case,
                run.package
            ),
            None => println!(
                "ACCEPT {} {} (package {}, exit code {code})",
                run.outcome.as_str(),
                run.case,
                run.package
            ),
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
        "format": "dnacalc-accept-receipt/3",
        "bead": args.id,
        "platform": platform.as_str(),
        "require_all": args.require_all,
        "result": result,
        "exit_code": exit_code,
        "result_rule": "failed when a manifest, an architecture check or a command failed, the bead is unknown, no case declares the platform, or a selected case is failed, missing or ignored; otherwise not_run when a selected case is not_run (every case is not_run on a host that cannot run the platform); otherwise passed. The run exits 0 for passed, and for not_run only without --require-all.",
        "paths": "relative to the workspace root; <rustup>, <cargo-home>, <home> and <temp> stand for directories outside it, and host.legacy_root.path is .. for the workspace root's parent or <legacy-root> for a directory elsewhere",
        "command": command,
        "started_utc": started_utc,
        "duration_s": seconds(started.elapsed().as_secs_f64()),
        "host": host,
        "cargo_target_dir": cargo::ACCEPT_TARGET_DIR,
        "manifests": {
            "files": manifests.files.iter().map(|path| ws.rel(path)).collect::<Vec<_>>(),
            "errors": manifests.errors,
        },
        "architecture": checks.iter().map(check_json).collect::<Vec<_>>(),
        "commands": commands.iter().map(CommandRun::to_json).collect::<Vec<_>>(),
        "cases": runs.iter().map(|run| run.to_json(None)).collect::<Vec<_>>(),
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
        assert_eq!(
            redactor.apply(&format!("{root}{sep}target{sep}xtask-accept")),
            "target/xtask-accept"
        );
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
}
