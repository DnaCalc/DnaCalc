//! `cargo xtask capture --fixture <xlsx|xlsm> --out <capture.json> [--timeout <s>] [--case <id>]
//! [--verdict <report.json>] [--work-dir <dir>] [--companion <file>]... [--calculation
//! automatic|manual|semiautomatic] [--revision <n>] [--operation <text>]...`: the repository
//! command over the one Excel capture adapter, calc-oracle-excel (bead `infra-m1.2`).
//!
//! It takes one live capture of the fixture through `calc_oracle_excel::capture`, whose
//! documentation states what the isolation enforces: no setting changed beyond the worker's own
//! Excel instance, macros force-disabled and events off, a workbook whose package holds one of
//! the checked routes to external data (a refresh-on-load connection, pivot cache or query table,
//! an OLE or DDE link, a call to a listed external-data function) refused before Excel starts, a
//! capture refused before the workbook opens when an add-in is open in the worker's Excel
//! instance (a connected COM add-in is recorded by name, not refused and never switched off, and
//! the verdict counts it and certifies no raw comparison against the record), links and
//! connections recorded and never refreshed, the calculation policy set, a full rebuild, save and
//! reopen. It always keeps the CaptureRecord version 2 in the work directory as `capture.json`,
//! and computes the record's file-cache replay (`calc_verdict::file_cache_replay` of the record as
//! written and read back). It refuses the capture, writing neither `--out` nor `--verdict`, when
//! the record's session carries an isolation failure (a refresh-on-open connection or an OLE or
//! DDE link Excel reports, a reported Excel the adapter refused, could not open, had to terminate
//! or could not terminate, or none reported) or when the verdict refuses the replay (an inventory
//! failure, say). The refusals the worker makes itself (a package refused before Excel starts, an
//! Excel with an add-in open in its instance, a session setting that does not read back as set)
//! end the worker with a fatal error, so no record is written and the capture fails.
//! Otherwise it writes the record to `--out` and, with `--verdict`, the replay's verdict record,
//! the record the replay case compares on every platform. Companions are copied beside the
//! fixture's copy (the target of an external link). The work directory defaults to
//! `target/capture/<case>-<UTC time>` and must be new; the capture tool's revision names the
//! repository's commit (with `+dirty` for a changed tree) when git can read it.
//!
//! Excel must be installed on this Windows host and registered for automation; on the owner's
//! machine any agent may run this command (AGENTS.md, Excel on this machine). Paths are relative
//! to the current directory.
//!
//! Exit codes: 0 when the record (and the verdict record) was written; 1 when the capture failed
//! (setup, worker or result, the worker's own refusals among them) or a file cannot be written; 2
//! on a usage error; 3 when the worker timed out and it and its Excel were terminated; 4 when this
//! host cannot run a capture (not Windows, or Excel not registered for automation): not run; 5
//! when a completed record is refused for an isolation failure in its session or a refused
//! replay, with the record kept only in the work directory.

use crate::accept::utc_now;
use calc_oracle_excel::{
    CalculationMode, CalculationPolicy, CaptureError, CaptureOutcome, CaptureRequest, capture,
};
use calc_verdict::{CaptureRecord, STAGE_ISOLATION, VerdictPolicy, file_cache_replay};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

pub const USAGE: &str = "usage: cargo xtask capture --fixture <xlsx|xlsm> --out <capture.json> [--timeout <s>] [--case <id>] [--verdict <report.json>] [--work-dir <dir>] [--companion <file>]... [--calculation automatic|manual|semiautomatic] [--revision <n>] [--operation <text>]...";

const EXIT_FAILED: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_TIMED_OUT: u8 = 3;
const EXIT_UNAVAILABLE: u8 = 4;
const EXIT_REFUSED: u8 = 5;

/// The default timeout of one capture.
const DEFAULT_TIMEOUT_S: u64 = 300;

#[derive(Debug, PartialEq)]
struct Args {
    fixture: PathBuf,
    out: PathBuf,
    timeout: Duration,
    case: String,
    verdict: Option<PathBuf>,
    work_dir: Option<PathBuf>,
    companions: Vec<PathBuf>,
    policy: CalculationPolicy,
    revision: u64,
    operations: Vec<String>,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let (mut fixture, mut out, mut timeout, mut case, mut verdict, mut work_dir) =
        (None, None, None, None, None, None);
    let mut companions = Vec::new();
    let mut operations = Vec::new();
    let mut mode = None;
    let mut revision = None;
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let value = rest
            .next()
            .ok_or_else(|| format!("{flag} needs a value"))?
            .clone();
        let once = |slot: &mut Option<String>| {
            if slot.replace(value.clone()).is_some() {
                Err(format!("{flag} is given twice"))
            } else {
                Ok(())
            }
        };
        match flag.as_str() {
            "--fixture" => once(&mut fixture)?,
            "--out" => once(&mut out)?,
            "--timeout" => once(&mut timeout)?,
            "--case" => once(&mut case)?,
            "--verdict" => once(&mut verdict)?,
            "--work-dir" => once(&mut work_dir)?,
            "--calculation" => once(&mut mode)?,
            "--revision" => once(&mut revision)?,
            "--companion" => companions.push(PathBuf::from(value)),
            "--operation" => {
                if value.is_empty() {
                    return Err("--operation needs a non-empty operation".to_owned());
                }
                operations.push(value);
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    let fixture = PathBuf::from(fixture.ok_or("--fixture is required")?);
    let case = match case {
        Some(case) if case.is_empty() => return Err("--case needs a non-empty case".to_owned()),
        Some(case) => case,
        None => fixture
            .file_stem()
            .and_then(|stem| stem.to_str())
            .filter(|stem| !stem.is_empty())
            .ok_or("--case is required when the fixture's name gives none")?
            .to_owned(),
    };
    let timeout = match timeout {
        None => DEFAULT_TIMEOUT_S,
        Some(text) => text
            .parse::<u64>()
            .ok()
            .filter(|seconds| *seconds > 0)
            .ok_or_else(|| format!("--timeout {text:?} is not a positive number of seconds"))?,
    };
    let mut policy = CalculationPolicy::default();
    if let Some(text) = mode {
        policy.mode = CalculationMode::parse(&text).ok_or_else(|| {
            format!("--calculation {text:?} is not automatic, manual or semiautomatic")
        })?;
    }
    let revision = match revision {
        None => 0,
        Some(text) => text
            .parse()
            .map_err(|_| format!("--revision {text:?} is not an authored revision number"))?,
    };
    Ok(Args {
        fixture,
        out: PathBuf::from(out.ok_or("--out is required")?),
        timeout: Duration::from_secs(timeout),
        case,
        verdict: verdict.map(PathBuf::from),
        work_dir: work_dir.map(PathBuf::from),
        companions,
        policy,
        revision,
        operations,
    })
}

/// The repository's commit, with `+dirty` when the tree has changes, or `None` when git cannot
/// tell.
fn repository_revision() -> Option<String> {
    let here = Path::new(".");
    let head = crate::cargo::git(here, &["rev-parse", "HEAD"]).ok()?;
    let status = crate::cargo::git(here, &["status", "--porcelain"]).ok()?;
    let dirty = if status.trim().is_empty() {
        ""
    } else {
        "+dirty"
    };
    Some(format!("{}{dirty}", head.trim()))
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    fs::write(path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// The exit code of a capture error, with what to print.
fn failure(error: &CaptureError) -> u8 {
    match error {
        CaptureError::Unavailable { .. } => EXIT_UNAVAILABLE,
        CaptureError::TimedOut(_) => EXIT_TIMED_OUT,
        CaptureError::Setup { .. } | CaptureError::Worker { .. } | CaptureError::Result { .. } => {
            EXIT_FAILED
        }
    }
}

fn run_code(raw: &[String]) -> u8 {
    let args = match parse_args(raw) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("xtask capture: {e}\n{USAGE}");
            return EXIT_USAGE;
        }
    };
    let work_dir = args.work_dir.clone().unwrap_or_else(|| {
        PathBuf::from("target").join("capture").join(format!(
            "{}-{}",
            args.case,
            utc_now().replace(':', "")
        ))
    });
    let request = CaptureRequest {
        case: args.case.clone(),
        fixture: args.fixture.clone(),
        companions: args.companions.clone(),
        work_dir: work_dir.clone(),
        timeout: args.timeout,
        policy: args.policy,
        revision: args.revision,
        operations: args.operations.clone(),
        repository: repository_revision(),
    };
    println!(
        "xtask capture: capturing {} as case {} in {} (timeout {}s)",
        args.fixture.display(),
        args.case,
        work_dir.display(),
        args.timeout.as_secs()
    );
    match capture(&request) {
        Ok(outcome) => finish(&args, &outcome, &work_dir),
        Err(error) => {
            let code = failure(&error);
            let not_run = if code == EXIT_UNAVAILABLE {
                " (not run)"
            } else {
                ""
            };
            eprintln!("xtask capture{not_run}: {error}");
            code
        }
    }
}

/// What becomes of a capture taken: its record is kept as `capture.json` in `work_dir`; it is
/// refused ([`EXIT_REFUSED`]) when its session carries an isolation failure or the verdict refuses
/// its file-cache replay; otherwise the record goes to `--out` and, with `--verdict`, the replay's
/// verdict record to that path.
fn finish(args: &Args, outcome: &CaptureOutcome, work_dir: &Path) -> u8 {
    let session = outcome.record.session.as_ref();
    println!(
        "xtask capture: {} formula cells, {} raw observations, {} saved caches, {} reopened values, {} capture failures; Excel PID {} by worker PID {}",
        outcome.record.formulas.len(),
        outcome.record.observations.len(),
        session.map_or(0, |s| s.saved.len()),
        session.map_or(0, |s| s.reopened.len()),
        session.map_or(0, |s| s.failures.len()),
        outcome
            .run
            .excel_pid
            .map_or_else(|| "?".to_owned(), |pid| pid.to_string()),
        outcome.run.worker_pid
    );
    for failure in session.into_iter().flat_map(|s| &s.failures) {
        let cell = failure
            .cell
            .as_ref()
            .map(|cell| format!(" {}!{}", cell.sheet(), cell.a1()))
            .unwrap_or_default();
        println!(
            "xtask capture: failure [{}]{cell}: {}",
            failure.stage, failure.message
        );
    }
    let kept = work_dir.join("capture.json");
    if let Err(e) = write(&kept, &outcome.encoded) {
        eprintln!("xtask capture: {e}");
        return EXIT_FAILED;
    }
    let not_written = if args.verdict.is_some() {
        "neither the record nor the verdict record is written"
    } else {
        "the record is not written"
    };
    let isolation = session.map_or(0, |s| s.failures_at(STAGE_ISOLATION).count());
    if isolation > 0 {
        eprintln!(
            "xtask capture: refused: the capture records {isolation} isolation failure(s), so {not_written} to the paths given; the record is kept at {}",
            kept.display()
        );
        return EXIT_REFUSED;
    }
    // The replay is of the record as written and read back, as every platform reads it.
    let report = CaptureRecord::decode(outcome.encoded.as_bytes())
        .map_err(|e| format!("the written record does not decode: {e}"))
        .and_then(|record| {
            file_cache_replay(&record, &VerdictPolicy::default())
                .map_err(|e| format!("the verdict refuses its file-cache replay: {e}"))
        });
    let report = match report {
        Ok(report) => report,
        Err(e) => {
            eprintln!(
                "xtask capture: refused: {e}; {not_written} to the paths given, and the record is kept at {}",
                kept.display()
            );
            return EXIT_REFUSED;
        }
    };
    println!("xtask capture: {}", report.summary());
    if let Err(e) = write(&args.out, &outcome.encoded) {
        eprintln!("xtask capture: {e}");
        return EXIT_FAILED;
    }
    println!("xtask capture: record written to {}", args.out.display());
    if let Some(verdict) = &args.verdict {
        if let Err(e) = write(verdict, &report.encode()) {
            eprintln!("xtask capture: {e}");
            return EXIT_FAILED;
        }
        println!(
            "xtask capture: verdict record written to {}",
            verdict.display()
        );
    }
    0
}

pub fn run(args: &[String]) -> ExitCode {
    ExitCode::from(run_code(args))
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_oracle_excel::{ReportedExcel, TimeoutReport, WorkerRun, isolation_failures};

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn arguments_are_read_with_their_defaults() {
        let parsed = parse_args(&args("--fixture dir/replay.xlsx --out c.json")).unwrap();
        assert_eq!(parsed.case, "replay");
        assert_eq!(parsed.timeout, Duration::from_secs(DEFAULT_TIMEOUT_S));
        assert_eq!(parsed.policy, CalculationPolicy::default());
        assert_eq!(
            (parsed.revision, parsed.verdict, parsed.work_dir),
            (0, None, None)
        );
        let parsed = parse_args(&args(
            "--fixture f.xlsm --out c.json --timeout 30 --case k --verdict v.json --work-dir w --companion a.xlsx --companion b.xlsx --calculation manual --revision 2 --operation set-A1",
        ))
        .unwrap();
        assert_eq!(parsed.case, "k");
        assert_eq!(parsed.timeout, Duration::from_secs(30));
        assert_eq!(parsed.verdict, Some(PathBuf::from("v.json")));
        assert_eq!(parsed.work_dir, Some(PathBuf::from("w")));
        assert_eq!(
            parsed.companions,
            [PathBuf::from("a.xlsx"), PathBuf::from("b.xlsx")]
        );
        assert_eq!(parsed.policy.mode, CalculationMode::Manual);
        assert_eq!(parsed.revision, 2);
        assert_eq!(parsed.operations, ["set-A1"]);
        for (bad, refusal) in [
            ("--out c.json", "--fixture is required"),
            ("--fixture f.xlsx", "--out is required"),
            ("--fixture f.xlsx --out c --out d", "--out is given twice"),
            (
                "--fixture f.xlsx --out c --timeout 0",
                "not a positive number",
            ),
            (
                "--fixture f.xlsx --out c --timeout x",
                "not a positive number",
            ),
            (
                "--fixture f.xlsx --out c --calculation auto",
                "is not automatic",
            ),
            (
                "--fixture f.xlsx --out c --revision -1",
                "not an authored revision",
            ),
            ("--fixture f.xlsx --out c --excel", "needs a value"),
            ("--fixture f.xlsx --out c --excel 1", "unknown argument"),
        ] {
            let error = parse_args(&args(bad)).unwrap_err();
            assert!(error.contains(refusal), "{bad}: {error}");
        }
        assert_eq!(run_code(&args("--out c.json")), EXIT_USAGE);
    }

    #[test]
    fn each_capture_error_has_its_exit_code() {
        assert_eq!(
            failure(&CaptureError::Unavailable {
                reason: "r".to_owned()
            }),
            EXIT_UNAVAILABLE
        );
        assert_eq!(
            failure(&CaptureError::TimedOut(Box::new(TimeoutReport {
                timeout: Duration::from_secs(1),
                worker_pid: 1,
                excel_pid: None,
                excel: ReportedExcel::NotReported,
                worker_terminated: true,
                excel_image: "EXCEL.EXE".to_owned(),
                new_excel_processes: Ok(Vec::new()),
                stderr: String::new(),
            }))),
            EXIT_TIMED_OUT
        );
        for error in [
            CaptureError::Setup {
                message: "m".to_owned(),
            },
            CaptureError::Worker {
                message: "m".to_owned(),
                stderr: String::new(),
            },
            CaptureError::Result {
                message: "m".to_owned(),
            },
        ] {
            assert_eq!(failure(&error), EXIT_FAILED);
        }
    }

    /// A version 2 live capture of Sheet1!A1 (=1+1, raw 2, saved cache 2) whose session records
    /// `failures` (each a JSON failure object).
    fn outcome(failures: &str) -> CaptureOutcome {
        let text = format!(
            r#"{{"format": "dnacalc-capture-record", "version": 2, "case": "k", "source": "sha256:in",
             "acquisition": {{"kind": "live_capture"}},
             "environment": {{"excel_build": "16.0.1.2", "channel": "current", "compatibility": "calc-1",
               "numeric_environment": "excel-x86-64/cpu-x86-64", "settings": {{"date1904": "false"}}}},
             "session": {{"revision": 0, "operations": [], "context": "c", "tool": "t",
               "packages": {{"input": "sha256:in", "output": "sha256:out"}}, "failures": [{failures}]}},
             "sheets": [{{"name": "Sheet1", "visibility": "visible"}}],
             "formulas": [{{"sheet": "Sheet1", "row": 1, "column": 1, "kind": "ordinary", "formula": "=1+1"}}],
             "shared_ranges": [],
             "observations": [{{"sheet": "Sheet1", "row": 1, "column": 1, "excel_raw": {{"number": "4000000000000000"}}}}],
             "saved": [{{"sheet": "Sheet1", "row": 1, "column": 1, "excel_saved_cache": {{"t": "n", "v": "2"}}}}],
             "reopened": [{{"sheet": "Sheet1", "row": 1, "column": 1, "excel_raw": {{"number": "4000000000000000"}}}}]}}"#
        );
        let record = CaptureRecord::decode(text.as_bytes()).unwrap();
        let encoded = record.encode().unwrap();
        CaptureOutcome {
            record,
            encoded,
            run: WorkerRun {
                worker_pid: 1,
                excel_pid: Some(2),
                excel: ReportedExcel::Exited,
                exit_code: Some(0),
                done: true,
                other_output: Vec::new(),
                stderr: String::new(),
            },
            input_copy: PathBuf::from("in"),
            output_package: PathBuf::from("out"),
        }
    }

    /// `finish` on `outcome` in a fresh directory, with `--out` and `--verdict` there: its exit
    /// code, whether each of the two paths and the work directory's record were written.
    fn finished(name: &str, outcome: &CaptureOutcome) -> (u8, bool, bool, bool) {
        let dir = std::env::temp_dir().join(format!(
            "xtask-capture-finish-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let work = dir.join("work");
        let args = parse_args(&[
            "--fixture".to_owned(),
            "k.xlsx".to_owned(),
            "--out".to_owned(),
            dir.join("capture.json").display().to_string(),
            "--verdict".to_owned(),
            dir.join("verdict.json").display().to_string(),
        ])
        .unwrap();
        let code = finish(&args, outcome, &work);
        let written = (
            code,
            args.out.exists(),
            args.verdict.as_ref().unwrap().exists(),
            work.join("capture.json").exists(),
        );
        let _ = fs::remove_dir_all(&dir);
        written
    }

    /// A capture whose session carries an isolation failure is refused: exit 5, no record at
    /// `--out` and no verdict record, the record kept in the work directory only.
    #[test]
    fn a_capture_with_an_isolation_failure_is_refused_and_writes_no_verdict_record() {
        assert_eq!(finished("clean", &outcome("")), (0, true, true, true));
        // A failure about one cell's value does not refuse the capture.
        assert_eq!(
            finished(
                "value",
                &outcome(
                    r#"{"stage": "value", "sheet": "Sheet1", "row": 1, "column": 1, "message": "m"}"#
                )
            ),
            (0, true, true, true)
        );
        assert_eq!(
            finished(
                "isolation",
                &outcome(
                    r#"{"stage": "isolation", "message": "add-ins are still loaded: COM add-in X"}"#
                )
            ),
            (EXIT_REFUSED, false, false, true)
        );
        // The adapter's own refusal of a reported Excel arrives as an isolation failure too.
        let mut refused = outcome(
            r#"{"stage": "isolation", "message": "the worker reported Excel PID 2, a EXCEL.EXE process that was already running before the capture started"}"#,
        );
        refused.run.excel = ReportedExcel::Refused("already running".to_owned());
        assert_eq!(
            finished("refused", &refused),
            (EXIT_REFUSED, false, false, true)
        );
    }

    /// An Excel still running after its termination, or one the adapter had to terminate, gives
    /// the record the isolation failure `calc_oracle_excel::isolation_failures` maps it to, and the
    /// capture is refused: exit 5, with neither `--out` nor `--verdict` written.
    #[test]
    fn an_excel_still_running_or_terminated_after_the_worker_quit_refuses_the_capture() {
        for (name, excel) in [
            ("still-running", ReportedExcel::StillRunning),
            ("terminated", ReportedExcel::Terminated),
        ] {
            let mut taken = outcome("");
            taken.run.excel = excel;
            let failures = isolation_failures(&taken.run, Duration::from_secs(15));
            assert_eq!(failures.len(), 1, "{name}");
            if name == "still-running" {
                assert!(
                    failures[0].message.contains("Excel PID 2 is still running"),
                    "{}",
                    failures[0].message
                );
            }
            taken
                .record
                .session
                .as_mut()
                .expect("a version 2 record")
                .failures
                .extend(failures);
            taken.encoded = taken.record.encode().unwrap();
            assert_eq!(
                finished(name, &taken),
                (EXIT_REFUSED, false, false, true),
                "{name}"
            );
        }
        // An Excel that exited by itself gives no failure, and the capture is written.
        let mut clean = outcome("");
        clean.run.excel = ReportedExcel::Exited;
        assert_eq!(isolation_failures(&clean.run, Duration::from_secs(15)), []);
        assert_eq!(finished("exited", &clean), (0, true, true, true));
    }

    /// A capture whose file-cache replay the verdict refuses (an inventory failure) is refused
    /// alike.
    #[test]
    fn a_capture_whose_replay_is_refused_writes_nothing_to_its_paths() {
        assert_eq!(
            finished(
                "inventory",
                &outcome(r#"{"stage": "inventory", "message": "sheet Macro1 is a xlMacrosheet"}"#)
            ),
            (EXIT_REFUSED, false, false, true)
        );
    }

    /// A fixture that is not a workbook is refused before any process starts, whether or not
    /// Excel is installed here, and nothing is written.
    #[test]
    fn a_fixture_that_is_not_a_workbook_starts_nothing() {
        let out = std::env::temp_dir().join(format!(
            "xtask-capture-{}-never-written.json",
            std::process::id()
        ));
        let work = std::env::temp_dir().join(format!(
            "xtask-capture-{}-never-created",
            std::process::id()
        ));
        let code = run_code(&[
            "--fixture".to_owned(),
            "no-such-fixture.txt".to_owned(),
            "--out".to_owned(),
            out.display().to_string(),
            "--work-dir".to_owned(),
            work.display().to_string(),
        ]);
        assert!(
            code == EXIT_FAILED || code == EXIT_UNAVAILABLE,
            "exit code {code}"
        );
        assert!(!out.exists() && !work.exists());
    }
}
