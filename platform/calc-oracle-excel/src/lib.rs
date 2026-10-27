//! The one Excel capture adapter (plan sections 10.1, 10.2 and 3.3): opens a workbook in a fresh,
//! isolated Excel instance, sets the calculation policy, forces a full rebuild, captures the raw
//! `Value2` of every formula cell (numbers as their binary64 bits) with its formula text and
//! whether it spills, Excel's build, compatibility, CPU, OS, locale, date system and calculation
//! settings, saves the workbook, reopens it and captures it again, and writes the whole as a
//! CaptureRecord version 2 of calc-verdict. A formula cell that spills after the rebuild is
//! recorded by the array its spill range shows (dc-677: its raw observation and its reopened
//! value, the spill's shape checked against the saved package's `ref` and its cell metadata,
//! which must name the dynamic-array metadata XLDAPR), never by the top-left value of its array
//! as a scalar; a value of the array the record cannot hold is a failure at [`STAGE_SPILL`]. The
//! cells it spills into never enter the formula inventory. A spill that cannot be read refuses the capture,
//! so an Excel without Range.HasSpill (one older than dynamic arrays) refuses every capture that
//! holds a formula (`convert.rs` holds the detail).
//!
//! What the isolation enforces, and how a capture shows it (`worker/capture-worker.ps1` holds the
//! detail):
//!
//! - Settings: the worker changes no setting that persists beyond its own Excel instance. The
//!   settings it sets are session-only: the Application's Visible, DisplayAlerts, ScreenUpdating,
//!   EnableEvents, Interactive and AutomationSecurity belong to the running instance, and the
//!   calculation settings (Calculation, Iteration, MaxIterations, MaxChange) are taken from the
//!   workbook Excel opens and saved with the package it saves in the capture's work directory. It
//!   writes no Excel option of the user's profile (not AskToUpdateLinks, not
//!   IgnoreRemoteRequests) and no add-in's state, so a capture killed on its timeout leaves
//!   nothing of the owner's Excel to put back; a unit test checks the script assigns none of them.
//! - Macros: AutomationSecurity is set to 3 (force-disable) and EnableEvents off before the
//!   workbook opens, and both are read back; one that does not read back as set refuses the
//!   capture before the workbook is opened.
//! - Add-ins: after Excel starts, the worker reads every add-in's state (AddIns2: IsOpen and
//!   Installed; COMAddIns: Connect; AddIns: Installed) and whether XLL functions are registered,
//!   and switches none off or on. One that is open in the capture's instance, one of AddIns2 whose
//!   state cannot be read, and registered XLL functions refuse the capture as isolation failures
//!   that name each by its file name, and Excel quits without the workbook having been opened. A
//!   connected COM add-in does not refuse it and is not disconnected: on the owner's machine the
//!   COM add-in VS15ExcelAdaptor, registered under HKLM with LoadBehavior 3, reads as connected in
//!   every automation instance measured, and the worker writes no add-in's state. This deviates
//!   from an isolated capture, and it is counted: the record lists every connected COM add-in as
//!   `ProgId|Description` in the setting `com_add_ins_connected`, and calc-verdict's report
//!   counts them (`com_add_ins_connected`) and certifies no raw comparison against such a record
//!   (`com_add_in_connected`); bead dc-quhd owns capturing without one. Every installed add-in of
//!   Application.AddIns is listed by its Name, a file name, in `add_ins_installed`. One whose state
//!   or names cannot be read is an environment failure. What a connected COM add-in does during
//!   the capture is not observed. These list settings separate items with `;` and fields with
//!   `|`, and write `%`, `;` and `|` within a name as `%25`, `%3B` and `%7C`.
//! - External content, refused before Excel starts (Excel never opens the workbook), by these
//!   checks of the package and no others: a data connection, pivot cache definition or query table
//!   with refreshOnLoad; an external link part that is not a link to another workbook (an OLE or
//!   DDE link); a call, in a cell formula of a worksheet, macro sheet or dialog sheet, a
//!   conditional-formatting or data-validation formula or a defined name, to RTD, WEBSERVICE (and
//!   so FILTERXML over it), IMAGE, STOCKHISTORY, a CUBE function, TRANSLATE, DETECTLANGUAGE,
//!   COPILOT, PY, IMPORTTEXT, IMPORTCSV, CALL, REGISTER.ID or SQL.REQUEST (the worker's
//!   `$ExternalDataFunctions`), matched without regard to case outside string literals. A link to
//!   another workbook is kept: the workbook opens with UpdateLinks 0, which never refreshes it, and
//!   its links and data connections are recorded in the capture's settings; an OLE or DDE link or
//!   a connection that refreshes on open that Excel reports (or one whose setting cannot be read)
//!   is an isolation failure. Any other route to external data or another application (an ActiveX
//!   control, an embedded OLE object, a linked data type, an add-in's function) is not checked,
//!   and so not modeled.
//! - Reads: Windows PowerShell's own property read turns a getter that fails into nothing, for
//!   COM objects too, which would read as a value (an add-in as not connected, a setting as off, a
//!   Value2 as empty). Every property the worker reads from Excel's objects (the settings read
//!   back, each add-in's state, a connection's refresh-on-open, every setting the record carries,
//!   Application.Hwnd, each worksheet's Name and UsedRange, HasFormula, Formula2, Value2,
//!   HasSpill, SpillingToRange with its Address, Rows.Count and Columns.Count, and the Row and
//!   Column of a listed formula cell) goes through COM's own call (`InvokeMember`), where a
//!   failed read throws, and is checked for its type: a read that fails, or gives another type,
//!   is an isolation failure, a refusal of the capture (a spill), a failure of that cell or of
//!   the inventory, or an environment failure, never a value. A setting is never a default cast
//!   from nothing: one that cannot be read is recorded as empty with an environment failure. The
//!   conversion refuses an empty date1904, iteration, max_iterations, max_change or
//!   precision_as_displayed (the context fingerprint rests on them) and an empty calculation
//!   version (the target's compatibility names it); any other setting that cannot be read is
//!   kept empty, and its environment failure leaves every comparison against the record
//!   uncertified. Two collections are read plainly because a method is called on them at once:
//!   Application.Workbooks before Open, and Workbook.Worksheets and Worksheet.Cells before Item.
//!   A failed read there gives nothing, and the method call on it throws. The add-in states and
//!   lists the record carries (`add_ins`, `com_add_ins_connected`, `add_ins_installed`) name each
//!   add-in by its ProgId or file name, never by a path.
//! - Processes: one worker process per capture, a timeout, and termination only of that worker and
//!   of the Excel process the adapter attributes to the capture ([`run_isolated`]): the PID the
//!   worker reported, when no process had that PID in the snapshot of the Excel image's processes
//!   taken before the worker started and the process with it now has the Excel image. That is an
//!   inference, not a proof that the capture started it. What became of that Excel is measured
//!   ([`ReportedExcel`]): one the adapter had to terminate, one still running after its
//!   termination, one it could not open or refused, and none reported at all are isolation
//!   failures.
//!
//! An isolation failure is recorded in the capture's session; calc-verdict then certifies no
//! comparison against the record, and `cargo xtask capture` refuses the capture.
//!
//! - [`capture`]: one capture ([`CaptureRequest`] to [`CaptureOutcome`]); the repository command
//!   `cargo xtask capture` calls it.
//! - The worker, [`WORKER_SCRIPT`]: one Windows PowerShell process per capture runs this script,
//!   which drives Excel through COM. PowerShell is used because Excel's object model is
//!   late-bound COM, which PowerShell binds directly; it ships with every Windows (5.1), and the
//!   capture adds no Rust dependency for COM. It is the only capture implementation here; legacy
//!   runners are read only by converters.
//! - [`run_isolated`]: the isolation every capture runs under ([`Worker`], [`WorkerRun`],
//!   [`ReportedExcel`], [`TimeoutReport`]).
//! - [`time_workbook`]: Excel's own open, `CalculateFullRebuild`, `CalculateFull` and one-cell
//!   edit times for one workbook, in an Excel instance started under the same isolation
//!   ([`TIMING_SCRIPT`], bead dc-10d9); `cargo xtask bench` calls it.
//! - [`excel_available`]: whether this host can run a capture at all; the acceptance runner's
//!   excel route asks the same probe.
//!
//! Missing Excel is [`CaptureError::Unavailable`], which callers report as not run, never as a
//! pass.
//!
//! Native only: it refuses to compile for wasm32, so it never enters portable library or browser
//! builds. It compiles on Linux, where every capture is unavailable.

#[cfg(target_arch = "wasm32")]
compile_error!(
    "calc-oracle-excel is native-only and must not be built for wasm32; portable and browser builds exclude it"
);

mod capture;
mod convert;
mod isolation;
mod timing;
#[cfg(all(test, windows))]
mod worker_tests;

pub use calc_verdict::sha256_hex;
pub use capture::{
    CalculationMode, CalculationPolicy, CaptureOutcome, CaptureRequest, capture,
    capture_tool_revision, excel_available, isolation_failures, windows_powershell,
};
pub use convert::STAGE_SPILL;
pub use isolation::{
    EXCEL_IMAGE, PROTOCOL, ReportedExcel, TimeoutReport, Worker, WorkerRun, run_isolated,
};
pub use timing::{TimedEdit, TimingOutcome, TimingRequest, time_workbook};

use std::fmt;

/// The worker script every capture runs (`worker/capture-worker.ps1`).
pub const WORKER_SCRIPT: &str = include_str!("../worker/capture-worker.ps1");

/// The worker that times Excel's own calculation of a workbook (`worker/timing-worker.ps1`, bead
/// dc-10d9), which runs under the same isolation and uses the capture worker's functions.
pub const TIMING_SCRIPT: &str = include_str!("../worker/timing-worker.ps1");

/// Why a capture did not produce a record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureError {
    /// The capture cannot run on this host: not Windows, or Excel is not registered for
    /// automation. A capture that cannot run is not run, never a pass.
    Unavailable { reason: String },
    /// The request or its work directory is unusable; nothing was started.
    Setup { message: String },
    /// The worker could not start, broke its protocol, refused the workbook or exited without
    /// completing.
    Worker { message: String, stderr: String },
    /// The worker did not finish within its timeout; it, and the Excel it reported when that was
    /// one the capture started, were terminated, with what became of each measured.
    TimedOut(Box<TimeoutReport>),
    /// The worker completed, but its result cannot be turned into a CaptureRecord.
    Result { message: String },
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::Unavailable { reason } => write!(f, "capture unavailable: {reason}"),
            CaptureError::Setup { message } => write!(f, "capture refused: {message}"),
            CaptureError::Worker { message, stderr } => {
                write!(f, "capture worker failed: {message}")?;
                if !stderr.trim().is_empty() {
                    write!(f, "\n--- worker standard error\n{}", stderr.trim_end())?;
                }
                Ok(())
            }
            CaptureError::TimedOut(report) => write!(
                f,
                "capture timed out after {:?}: the worker (PID {}) {}; {}{}",
                report.timeout,
                report.worker_pid,
                if report.worker_terminated {
                    "was terminated"
                } else {
                    "could not be terminated"
                },
                report.excel.describe(report.excel_pid),
                isolation::new_processes_text(&report.excel_image, &report.new_excel_processes)
            ),
            CaptureError::Result { message } => write!(f, "capture result refused: {message}"),
        }
    }
}

impl std::error::Error for CaptureError {}
