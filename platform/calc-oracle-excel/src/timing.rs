//! Timing Excel's own calculation of one workbook (bead dc-10d9), under the isolation every
//! capture runs in ([`run_isolated`]): one worker process ([`TIMING_SCRIPT`], which reuses the
//! capture worker's functions), one new Excel instance it reports by PID, and termination only of
//! that worker and of an Excel this run started.
//!
//! The work directory a timing is given (it must not exist yet, or be empty) receives:
//!
//! ```text
//! <work>/in/<fixture name>   the copy of the fixture Excel opens, read-only
//! <work>/capture-worker.ps1  the capture worker, whose functions the timing worker uses
//! <work>/timing-worker.ps1   the timing worker
//! <work>/request.json        the worker's request
//! <work>/result.json         the worker's result
//! ```

use crate::capture::{excel_available, windows_powershell};
use crate::isolation::{EXCEL_IMAGE, Worker, WorkerRun, run_isolated};
use crate::{CaptureError, TIMING_SCRIPT, WORKER_SCRIPT};
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

/// How long the Excel a timing worker reported may outlive the worker's normal exit before it is
/// terminated.
const EXIT_GRACE: Duration = Duration::from_secs(15);

/// The input cell a timing edits, by its sheet, row and column (1-based). It must hold a number;
/// it is set to that number times 1.01 (1 when it holds 0) and then back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimedEdit {
    pub sheet: String,
    pub row: u32,
    pub column: u32,
}

/// One workbook to time in Excel.
#[derive(Clone, Debug)]
pub struct TimingRequest {
    /// The workbook (`.xlsx` or `.xlsm`).
    pub fixture: PathBuf,
    /// The work directory; it must not exist yet, or be empty.
    pub work_dir: PathBuf,
    /// How many times the workbook is opened and calculated in the one Excel instance.
    pub runs: u32,
    /// The one-cell edit each run times, if any.
    pub edit: Option<TimedEdit>,
    /// Whether to show what CalculateFull calculates, with multi-threaded calculation as the
    /// instance has it and switched off (the worker's step 5, bead dc-dxm).
    pub check_full: bool,
    /// How long the worker may run before it and its Excel are terminated.
    pub timeout: Duration,
}

/// A timing taken: the worker's result (see `worker/timing-worker.ps1`) and its run.
#[derive(Clone, Debug)]
pub struct TimingOutcome {
    pub result: serde_json::Value,
    pub run: WorkerRun,
}

fn setup(message: String) -> CaptureError {
    CaptureError::Setup { message }
}

/// Times Excel's open, `CalculateFullRebuild`, `CalculateFull` and the one-cell edit of
/// `request` (see the module and the worker's documentation).
pub fn time_workbook(request: &TimingRequest) -> Result<TimingOutcome, CaptureError> {
    excel_available().map_err(|reason| CaptureError::Unavailable { reason })?;
    let name = request
        .fixture
        .file_name()
        .ok_or_else(|| setup(format!("{} has no file name", request.fixture.display())))?;
    let work = std::path::absolute(&request.work_dir).map_err(|e| {
        setup(format!(
            "cannot resolve {}: {e}",
            request.work_dir.display()
        ))
    })?;
    if work.exists()
        && fs::read_dir(&work)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(true)
    {
        return Err(setup(format!(
            "the work directory {} is not empty; a timing needs a fresh one",
            work.display()
        )));
    }
    let input_dir = work.join("in");
    fs::create_dir_all(&input_dir)
        .map_err(|e| setup(format!("cannot create {}: {e}", input_dir.display())))?;
    let input_copy = input_dir.join(name);
    fs::copy(&request.fixture, &input_copy)
        .map_err(|e| setup(format!("cannot copy {}: {e}", request.fixture.display())))?;
    let script = work.join("timing-worker.ps1");
    for (path, text) in [
        (work.join("capture-worker.ps1"), WORKER_SCRIPT),
        (script.clone(), TIMING_SCRIPT),
    ] {
        fs::write(&path, text)
            .map_err(|e| setup(format!("cannot write {}: {e}", path.display())))?;
    }
    let request_file = work.join("request.json");
    let result_file = work.join("result.json");
    let edit = request.edit.as_ref().map(|edit| {
        serde_json::json!({
            "sheet": edit.sheet,
            "row": edit.row,
            "column": edit.column,
        })
    });
    let worker_request = serde_json::json!({
        "format": "dnacalc-timing-request",
        "version": 1,
        "input": input_copy.to_string_lossy(),
        "runs": request.runs,
        "edit": edit,
        "check_full": request.check_full,
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
        script.into_os_string(),
        OsString::from("-Request"),
        request_file.into_os_string(),
        OsString::from("-Result"),
        result_file.clone().into_os_string(),
    ])
    .collect();
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
    match result {
        Some(result) if run.done && run.exit_code == Some(0) => {
            if result["excel_pid"].as_u64() != run.excel_pid.map(u64::from) {
                return Err(CaptureError::Result {
                    message: format!(
                        "the result names Excel PID {} and the worker reported {:?}",
                        result["excel_pid"], run.excel_pid
                    ),
                });
            }
            Ok(TimingOutcome { result, run })
        }
        result => Err(CaptureError::Worker {
            message: format!(
                "the timing worker (PID {}) exited with {:?}: {}; {}",
                run.worker_pid,
                run.exit_code,
                result
                    .as_ref()
                    .and_then(|result| result["fatal"].as_str())
                    .unwrap_or("it wrote no result"),
                run.excel.describe(run.excel_pid)
            ),
            stderr: run.stderr.clone(),
        }),
    }
}
