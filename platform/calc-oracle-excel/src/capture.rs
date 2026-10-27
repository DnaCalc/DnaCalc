//! One live capture: the work directory, the worker request, the isolated run and the conversion
//! of the worker's result into a CaptureRecord version 2.
//!
//! The work directory a capture is given (it must not exist yet, or be empty) receives:
//!
//! ```text
//! <work>/in/<fixture name>       the copy of the fixture Excel opens, read-only
//! <work>/in/<companion name>     each companion file (the target of an external link, say)
//! <work>/out/<fixture name>      the package Excel saves, then reopens read-only
//! <work>/capture-worker.ps1      the worker script this package embeds
//! <work>/request.json            the worker's request
//! <work>/result.json             the worker's result
//! ```
//!
//! Excel never opens the fixture itself, and everything a capture writes stays in its work
//! directory, so a macro that ran, or a refreshed link, would leave its traces there.

use crate::convert::{self, Frame};
use crate::isolation::{EXCEL_IMAGE, ReportedExcel, Worker, WorkerRun, run_isolated};
use crate::{CaptureError, WORKER_SCRIPT};
use calc_verdict::{CaptureFailure, CaptureRecord, STAGE_ISOLATION, sha256_hex};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// How long the Excel a worker reported may outlive the worker's normal exit before it is
/// terminated.
const EXIT_GRACE: Duration = Duration::from_secs(15);

/// The longest full path of a workbook Excel opens, in characters (Microsoft's documented limit
/// for a path and file name together). A capture whose copy of its input, or the package it
/// saves, would have a longer one is refused before Excel starts (dc-677): Excel 16.0.20430's
/// Open of the copy of Annex 9's saved package failed under the old default work directory
/// ("Open method of Workbooks class failed") and opened under a short one.
const EXCEL_PATH_LIMIT: usize = 218;

/// The calculation mode the capture sets (Excel's `Application.Calculation`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CalculationMode {
    #[default]
    Automatic,
    Manual,
    SemiAutomatic,
}

impl CalculationMode {
    fn name(self) -> &'static str {
        match self {
            CalculationMode::Automatic => "automatic",
            CalculationMode::Manual => "manual",
            CalculationMode::SemiAutomatic => "semiautomatic",
        }
    }

    /// The mode a request names (`automatic`, `manual` or `semiautomatic`).
    pub fn parse(text: &str) -> Option<CalculationMode> {
        [
            CalculationMode::Automatic,
            CalculationMode::Manual,
            CalculationMode::SemiAutomatic,
        ]
        .into_iter()
        .find(|mode| mode.name() == text)
    }
}

/// The calculation policy the capture sets explicitly before it forces a full rebuild. The
/// default is Excel's own default: automatic, iteration off, 100 iterations, maximum change 0.001.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalculationPolicy {
    pub mode: CalculationMode,
    pub iteration: bool,
    pub max_iterations: u32,
    pub max_change: f64,
}

impl Default for CalculationPolicy {
    fn default() -> Self {
        CalculationPolicy {
            mode: CalculationMode::Automatic,
            iteration: false,
            max_iterations: 100,
            max_change: 0.001,
        }
    }
}

/// One capture to take.
#[derive(Clone, Debug)]
pub struct CaptureRequest {
    /// The case the record is for.
    pub case: String,
    /// The workbook to capture (`.xlsx` or `.xlsm`).
    pub fixture: PathBuf,
    /// Files copied beside the fixture's copy (the target of an external link, say).
    pub companions: Vec<PathBuf>,
    /// The capture's work directory; it must not exist yet, or be empty.
    pub work_dir: PathBuf,
    /// How long the worker may run before it and its Excel are terminated.
    pub timeout: Duration,
    pub policy: CalculationPolicy,
    /// The authored revision of the workbook and the operations that produced it from its source
    /// (0 and none for a fixture as authored).
    pub revision: u64,
    pub operations: Vec<String>,
    /// The revision of the repository the capture tool was built from, when known.
    pub repository: Option<String>,
}

/// A capture taken: its record (and that record's JSON), the worker's run, and the paths of the
/// copy Excel opened and of the package it saved.
#[derive(Clone, Debug)]
pub struct CaptureOutcome {
    pub record: CaptureRecord,
    pub encoded: String,
    pub run: WorkerRun,
    pub input_copy: PathBuf,
    pub output_package: PathBuf,
}

/// The capture tool's revision: this package's version, the content hash of the worker script it
/// embeds, and the repository revision when given.
pub fn capture_tool_revision(repository: Option<&str>) -> String {
    let mut revision = format!(
        "calc-oracle-excel {}; worker sha256:{}",
        env!("CARGO_PKG_VERSION"),
        sha256_hex(WORKER_SCRIPT.as_bytes())
    );
    if let Some(repository) = repository {
        revision.push_str("; repository ");
        revision.push_str(repository);
    }
    revision
}

/// Whether Excel is registered for automation on this host, or why a live capture cannot run.
pub fn excel_available() -> Result<(), String> {
    if !cfg!(windows) {
        return Err(format!(
            "a live Excel capture needs Windows with Excel; this host is {}",
            std::env::consts::OS
        ));
    }
    let key = r"HKCR\Excel.Application\CLSID";
    match Command::new(system32().join("reg.exe"))
        .args(["query", key])
        .output()
    {
        Ok(out) if out.status.success() => Ok(()),
        Ok(_) => Err(format!(
            "Excel is not registered for automation on this host ({key} is absent)"
        )),
        Err(e) => Err(format!("cannot run reg query {key}: {e}")),
    }
}

/// `%SystemRoot%\System32`.
fn system32() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from)
        .join("System32")
}

/// Windows PowerShell 5.1, by its full path so that nothing on PATH stands in for it.
pub fn windows_powershell() -> PathBuf {
    system32()
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe")
}

fn setup(message: String) -> CaptureError {
    CaptureError::Setup { message }
}

fn file_name(path: &Path) -> Result<String, CaptureError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| setup(format!("{} has no file name", path.display())))
}

/// The isolation failures a worker run gives the capture's record, from what became of the Excel
/// it reported: one refused, one that could not be opened, one that had to be terminated after the
/// worker quit it (`grace` after the worker's exit), one still running after its termination, and
/// none reported at all. An Excel that exited by itself gives none.
pub fn isolation_failures(run: &WorkerRun, grace: Duration) -> Vec<CaptureFailure> {
    let described = run.excel.describe(run.excel_pid);
    let message = match &run.excel {
        ReportedExcel::Exited | ReportedExcel::Gone => return Vec::new(),
        ReportedExcel::Terminated | ReportedExcel::StillRunning => format!(
            "{described}: it had not exited {} seconds after the worker quit it",
            grace.as_secs()
        ),
        ReportedExcel::NotReported | ReportedExcel::Refused(_) | ReportedExcel::Unopened(_) => {
            described
        }
    };
    vec![CaptureFailure {
        stage: STAGE_ISOLATION.to_owned(),
        cell: None,
        message,
    }]
}

/// Takes one capture (see the module documentation): [`CaptureError::Unavailable`] when this host
/// cannot run Excel, otherwise the record or why there is none.
/// This machine's local clock as a serial date and time of the 1900 date system (day 1 is
/// 1900-01-01, counting Excel's 1900-02-29, so a date from 1900-03-01 on is Excel's serial), or
/// `None` off Windows.
fn local_clock() -> Option<f64> {
    #[cfg(windows)]
    {
        // SAFETY: GetLocalTime writes the SYSTEMTIME it is given and nothing else.
        let now = unsafe {
            let mut now = std::mem::zeroed::<windows_sys::Win32::Foundation::SYSTEMTIME>();
            windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut now);
            now
        };
        let day = serial_day(
            i64::from(now.wYear),
            i64::from(now.wMonth),
            i64::from(now.wDay),
        );
        let seconds = f64::from(now.wHour) * 3600.0
            + f64::from(now.wMinute) * 60.0
            + f64::from(now.wSecond)
            + f64::from(now.wMilliseconds) / 1000.0;
        Some(day as f64 + seconds / 86_400.0)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// The 1900-system serial of a Gregorian date from 1900-03-01 on: days since 1899-12-30.
fn serial_day(year: i64, month: i64, day: i64) -> i64 {
    // Days from 1970-01-01 (H. Hinnant's days_from_civil), then 25569 days from 1899-12-30.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468 + 25_569
}

/// The clock to record as the setting calculation_clock: the local clock when the worker started,
/// whose date the calculation's NOW, TODAY and dates without a year read. `None` when either
/// reading is missing or the run crossed midnight, so the date Excel read is not known.
fn calculation_clock(before: Option<f64>, after: Option<f64>) -> Option<f64> {
    let (before, after) = (before?, after?);
    (before.floor() == after.floor()).then_some(before)
}

pub fn capture(request: &CaptureRequest) -> Result<CaptureOutcome, CaptureError> {
    excel_available().map_err(|reason| CaptureError::Unavailable { reason })?;
    let name = file_name(&request.fixture)?;
    let extension = Path::new(&name)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    if !matches!(extension.as_deref(), Some("xlsx" | "xlsm")) {
        return Err(setup(format!(
            "{} is not an .xlsx or .xlsm workbook",
            request.fixture.display()
        )));
    }
    let input_bytes = fs::read(&request.fixture)
        .map_err(|e| setup(format!("cannot read {}: {e}", request.fixture.display())))?;
    let source = format!("sha256:{}", sha256_hex(&input_bytes));

    let work = std::path::absolute(&request.work_dir).map_err(|e| {
        setup(format!(
            "cannot resolve the work directory {}: {e}",
            request.work_dir.display()
        ))
    })?;
    if work.exists()
        && fs::read_dir(&work)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(true)
    {
        return Err(setup(format!(
            "the work directory {} is not empty; a capture needs a fresh one",
            work.display()
        )));
    }
    let (input_dir, output_dir) = (work.join("in"), work.join("out"));
    // Excel opens no file whose full path is longer than EXCEL_PATH_LIMIT characters: its Open
    // then fails ("Open method of Workbooks class failed"), which a capture would report as the
    // worker's failure with no word of the path.
    for path in [input_dir.join(&name), output_dir.join(&name)] {
        let length = path.to_string_lossy().encode_utf16().count();
        if length > EXCEL_PATH_LIMIT {
            return Err(setup(format!(
                "the path {} is {length} characters, more than the {EXCEL_PATH_LIMIT} Excel opens; give a shorter --work-dir",
                path.display()
            )));
        }
    }
    for dir in [&input_dir, &output_dir] {
        fs::create_dir_all(dir)
            .map_err(|e| setup(format!("cannot create {}: {e}", dir.display())))?;
    }
    let input_copy = input_dir.join(&name);
    fs::write(&input_copy, &input_bytes)
        .map_err(|e| setup(format!("cannot write {}: {e}", input_copy.display())))?;
    for companion in &request.companions {
        let target = input_dir.join(file_name(companion)?);
        if target.exists() {
            return Err(setup(format!(
                "the companion {} has the name of a file already in {}",
                companion.display(),
                input_dir.display()
            )));
        }
        fs::copy(companion, &target).map_err(|e| {
            setup(format!(
                "cannot copy the companion {}: {e}",
                companion.display()
            ))
        })?;
    }
    let output_package = output_dir.join(&name);
    let script = work.join("capture-worker.ps1");
    let request_file = work.join("request.json");
    let result_file = work.join("result.json");
    fs::write(&script, WORKER_SCRIPT)
        .map_err(|e| setup(format!("cannot write {}: {e}", script.display())))?;
    let policy = request.policy;
    let worker_request = serde_json::json!({
        "format": "dnacalc-capture-request",
        "version": 1,
        "input": input_copy.to_string_lossy(),
        "output": output_package.to_string_lossy(),
        "calculation": {
            "mode": policy.mode.name(),
            "iteration": policy.iteration,
            "max_iterations": policy.max_iterations,
            "max_change": format!("{:016x}", policy.max_change.to_bits()),
        },
    });
    fs::write(&request_file, worker_request.to_string())
        .map_err(|e| setup(format!("cannot write {}: {e}", request_file.display())))?;

    let args: Vec<OsString> = [
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
    ]
    .iter()
    .map(OsString::from)
    .chain([
        script.clone().into_os_string(),
        OsString::from("-Request"),
        request_file.clone().into_os_string(),
        OsString::from("-Result"),
        result_file.clone().into_os_string(),
    ])
    .collect();
    // The local clock around the worker's run, which Excel's NOW reads during its calculation.
    let clock_before = local_clock();
    let run = run_isolated(&Worker {
        program: windows_powershell(),
        args,
        dir: work.clone(),
        timeout: request.timeout,
        exit_grace: EXIT_GRACE,
        excel_image: EXCEL_IMAGE.to_owned(),
    })?;

    let result: Option<serde_json::Value> = fs::read(&result_file)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    if !run.done || run.exit_code != Some(0) {
        let fatal = result
            .as_ref()
            .and_then(|result| result["fatal"].as_str())
            .map_or_else(String::new, |fatal| format!(": {fatal}"));
        // The failures the worker recorded before it stopped (an isolation refusal before Excel
        // started, say), each on its own line.
        let recorded: String = result
            .as_ref()
            .and_then(|result| result["failures"].as_array())
            .into_iter()
            .flatten()
            .map(|failure| {
                format!(
                    "\n  recorded failure [{}]: {}",
                    failure["stage"].as_str().unwrap_or("?"),
                    failure["message"].as_str().unwrap_or("?")
                )
            })
            .collect();
        // What became of the Excel it reported: as the record would carry it, or, for an Excel
        // that exited by itself, in words.
        let failures = isolation_failures(&run, EXIT_GRACE);
        let excel: String = if failures.is_empty() {
            format!("\n  {}", run.excel.describe(run.excel_pid))
        } else {
            failures
                .iter()
                .map(|failure| {
                    format!(
                        "\n  recorded failure [{}]: {}",
                        failure.stage, failure.message
                    )
                })
                .collect()
        };
        return Err(CaptureError::Worker {
            message: format!(
                "the worker (PID {}) exited with {:?}{}{fatal}{recorded}{excel}",
                run.worker_pid,
                run.exit_code,
                if run.done {
                    ""
                } else {
                    " without writing its result"
                }
            ),
            stderr: run.stderr.clone(),
        });
    }
    let result = result.ok_or_else(|| CaptureError::Result {
        message: format!("{} is not a JSON result", result_file.display()),
    })?;
    if result["excel_pid"].as_u64() != run.excel_pid.map(u64::from) {
        return Err(CaptureError::Result {
            message: format!(
                "the result names Excel PID {} and the worker reported {:?} on its protocol",
                result["excel_pid"], run.excel_pid
            ),
        });
    }

    let mut failures = isolation_failures(&run, EXIT_GRACE);
    // A saved package that cannot be read has no hash; the record says why, so the replay's later
    // refusal of a record without one is not its only trace. The message names the file only, as
    // the record may be committed.
    let output_hash = match fs::read(&output_package) {
        Ok(bytes) => Some(format!("sha256:{}", sha256_hex(&bytes))),
        Err(e) => {
            failures.push(CaptureFailure {
                stage: "save".to_owned(),
                cell: None,
                message: format!(
                    "the package Excel saved ({}) cannot be read: {e}",
                    output_package
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default()
                ),
            });
            None
        }
    };
    let clock_after = local_clock();
    let mut record = convert::convert(
        &result,
        Frame {
            case: request.case.clone(),
            source,
            tool: capture_tool_revision(request.repository.as_deref()),
            revision: request.revision,
            operations: request.operations.clone(),
            output_package: output_hash,
            failures,
        },
    )
    .map_err(|message| CaptureError::Result { message })?;
    if let (Some(environment), Some(clock)) = (
        record.environment.as_mut(),
        calculation_clock(clock_before, clock_after),
    ) {
        environment
            .settings
            .push(("calculation_clock".to_owned(), format!("{:016x}", clock.to_bits())));
    }
    let encoded = record.encode().map_err(|e| CaptureError::Result {
        message: e.to_string(),
    })?;
    let reread = CaptureRecord::decode(encoded.as_bytes())
        .map_err(|e| CaptureError::Result {
            message: format!("the written record does not decode: {e}"),
        })?
        .encode()
        .map_err(|e| CaptureError::Result {
            message: e.to_string(),
        })?;
    if reread != encoded {
        return Err(CaptureError::Result {
            message: "the written record does not decode to the same record".to_owned(),
        });
    }
    Ok(CaptureOutcome {
        record,
        encoded,
        run,
        input_copy,
        output_package,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tool_revision_names_the_embedded_worker() {
        let revision = capture_tool_revision(Some("abc123"));
        assert!(revision.starts_with("calc-oracle-excel "), "{revision}");
        assert!(
            revision.contains(&format!(
                "worker sha256:{}",
                sha256_hex(WORKER_SCRIPT.as_bytes())
            )),
            "{revision}"
        );
        assert!(revision.ends_with("; repository abc123"), "{revision}");
        assert!(!capture_tool_revision(None).contains("repository"));
    }

    #[test]
    fn serial_days_are_excel_serials() {
        assert_eq!(super::serial_day(1900, 3, 1), 61);
        assert_eq!(super::serial_day(2026, 1, 1), 46023);
        assert_eq!(super::serial_day(2026, 10, 25), 46320);
        assert_eq!(super::calculation_clock(Some(46320.25), Some(46320.75)), Some(46320.25));
        assert_eq!(super::calculation_clock(Some(46320.99), Some(46321.01)), None);
    }

    #[test]
    fn calculation_modes_are_named() {
        for mode in [
            CalculationMode::Automatic,
            CalculationMode::Manual,
            CalculationMode::SemiAutomatic,
        ] {
            assert_eq!(CalculationMode::parse(mode.name()), Some(mode));
        }
        assert_eq!(CalculationMode::parse("Automatic"), None);
        let policy = CalculationPolicy::default();
        assert_eq!(
            (
                policy.mode,
                policy.iteration,
                policy.max_iterations,
                policy.max_change.to_bits()
            ),
            (
                CalculationMode::Automatic,
                false,
                100,
                0x3f50_624d_d2f1_a9fc
            )
        );
    }

    fn run(excel: ReportedExcel) -> WorkerRun {
        WorkerRun {
            worker_pid: 10,
            excel_pid: Some(20),
            excel,
            exit_code: Some(0),
            done: true,
            other_output: Vec::new(),
            stderr: String::new(),
        }
    }

    /// Every outcome of the reported Excel but an exit of its own is an isolation failure of the
    /// record (which `cargo xtask capture` refuses), named as measured: an Excel still running
    /// after its termination reads as still running, never as terminated or exited.
    #[test]
    fn what_became_of_the_reported_excel_maps_to_isolation_failures() {
        let grace = Duration::from_secs(15);
        for clean in [ReportedExcel::Exited, ReportedExcel::Gone] {
            assert_eq!(
                isolation_failures(&run(clean.clone()), grace),
                [],
                "{clean:?}"
            );
        }
        let failures = isolation_failures(&run(ReportedExcel::StillRunning), grace);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].stage, STAGE_ISOLATION);
        assert_eq!(failures[0].cell, None);
        assert_eq!(
            failures[0].message,
            "Excel PID 20 is still running after its termination was requested and waited on: it had not exited 15 seconds after the worker quit it"
        );
        let failures = isolation_failures(&run(ReportedExcel::Terminated), grace);
        assert_eq!(
            failures[0].message,
            "Excel PID 20 was terminated and has exited: it had not exited 15 seconds after the worker quit it"
        );
        for (excel, words) in [
            (
                ReportedExcel::Refused("already running".to_owned()),
                "Excel PID 20 was not terminated, being refused: already running",
            ),
            (
                ReportedExcel::Unopened("access denied".to_owned()),
                "Excel PID 20 could not be opened (access denied), so it was not terminated and may still run",
            ),
        ] {
            let failures = isolation_failures(&run(excel), grace);
            assert_eq!(
                (
                    failures.len(),
                    failures[0].stage.as_str(),
                    failures[0].message.as_str()
                ),
                (1, STAGE_ISOLATION, words)
            );
        }
        let mut none = run(ReportedExcel::NotReported);
        none.excel_pid = None;
        let failures = isolation_failures(&none, grace);
        assert_eq!(
            failures[0].message,
            "the worker reported no Excel PID, so no Excel was terminated"
        );
    }

    /// Nothing starts for a request that is not a workbook: where Excel cannot run the capture is
    /// unavailable, and where it can the request is refused before any file is read or any
    /// process started (the fixture does not even exist).
    #[test]
    fn a_fixture_that_is_not_a_workbook_is_refused_before_anything_starts() {
        let request = CaptureRequest {
            case: "k".to_owned(),
            fixture: PathBuf::from("no-such-fixture.txt"),
            companions: Vec::new(),
            work_dir: std::env::temp_dir().join("calc-oracle-excel-unit-never-created"),
            timeout: Duration::from_secs(1),
            policy: CalculationPolicy::default(),
            revision: 0,
            operations: Vec::new(),
            repository: None,
        };
        match (excel_available(), capture(&request)) {
            (Err(reason), Err(CaptureError::Unavailable { reason: given })) => {
                assert_eq!(reason, given)
            }
            (Ok(()), Err(CaptureError::Setup { message })) => {
                assert!(
                    message.contains("is not an .xlsx or .xlsm workbook"),
                    "{message}"
                )
            }
            (available, other) => panic!("{available:?} gave {other:?}"),
        }
        assert!(!request.work_dir.exists());
    }
}
