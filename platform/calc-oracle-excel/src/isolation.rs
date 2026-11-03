//! The isolation of one capture: one worker process per capture, a deadline, and on timeout the
//! termination of that worker and of the one Excel process the worker reported starting, when the
//! adapter can establish that the process is one this capture started.
//!
//! The worker speaks a line protocol on its standard output, each line starting with
//! [`PROTOCOL`]:
//!
//! ```text
//! dnacalc-capture-worker/1 excel-pid <pid>   the Excel instance this worker started (once)
//! dnacalc-capture-worker/1 done              the worker wrote its result file
//! ```
//!
//! Before the worker starts, the adapter lists the PIDs of the running processes of the Excel
//! image ([`Worker::excel_image`], `EXCEL.EXE` for a capture). A reported PID is owned, and may be
//! terminated, only when it is absent from that list and the process it names, opened at once, runs
//! that image. A PID in the list (an Excel that was running before the capture started, such as
//! the owner's own) or of another image is refused: nothing is opened on it or terminated, and the
//! run carries the refusal ([`ReportedExcel::Refused`]), which the capture records as an isolation
//! failure. The handle held on an owned Excel means its PID cannot be reused by another process
//! while the capture runs, so terminating through it reaches only that process. Killing the worker
//! does not reach Excel (Excel is started by COM, not as the worker's child), which is why an owned
//! Excel is terminated on its own. An owned Excel that outlives the worker's normal exit by more
//! than the grace period is terminated too.
//!
//! What became of the reported Excel is measured, never assumed ([`ReportedExcel`]): an owned
//! Excel exited by itself, was terminated and then exited, or is still running after its
//! termination was requested and waited on; a reported PID that could not be opened is its own
//! outcome, told apart from one whose process had already exited. A worker that times out or
//! breaks its protocol before reporting its Excel leaves the adapter unable to tell which Excel is
//! its own: the run then lists the processes of the Excel image absent from the list taken before
//! the worker started ([`TimeoutReport::new_excel_processes`]) and terminates none of them.

use crate::CaptureError;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

/// The prefix of every protocol line a worker prints.
pub const PROTOCOL: &str = "dnacalc-capture-worker/1";

/// The image of the Excel process a capture's worker starts.
pub const EXCEL_IMAGE: &str = "EXCEL.EXE";

/// How long the output pipes may stay open after the worker has exited (a process the worker
/// started can hold them) before the run stops reading them.
const DRAIN: Duration = Duration::from_secs(2);

/// One worker process to run in isolation.
#[derive(Clone, Debug)]
pub struct Worker {
    /// The program the worker process runs, and its arguments.
    pub program: PathBuf,
    pub args: Vec<OsString>,
    /// The working directory of the worker.
    pub dir: PathBuf,
    /// How long the worker may run before it and the Excel it reported are terminated.
    pub timeout: Duration,
    /// How long the Excel the worker reported may outlive the worker's normal exit before it is
    /// terminated.
    pub exit_grace: Duration,
    /// The file name of the image a reported Excel must run ([`EXCEL_IMAGE`] for a capture),
    /// compared without regard to case; the processes of this image running before the worker
    /// starts are never terminated.
    pub excel_image: String,
}

/// What became of the Excel process a worker reported, as the adapter measured it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReportedExcel {
    /// A queued job returned the owned session to its recorded baseline; receipt path retained.
    SessionReady(String),
    /// The worker reported no Excel process.
    NotReported,
    /// The reported process is not one the adapter can show this capture started (why): nothing
    /// was opened on it for termination, and it was not terminated.
    Refused(String),
    /// The reported process could not be opened (why) for a reason other than that no process
    /// has its PID (access denied, say): whether it still runs is unknown, and it was not
    /// terminated.
    Unopened(String),
    /// When the adapter came to open the reported process, after the worker had exited, no
    /// process had its PID any more: it had exited.
    Gone,
    /// The owned Excel exited without being terminated (within the grace period after the
    /// worker's exit, or before a termination that a timeout or protocol failure called for).
    Exited,
    /// The owned Excel was still running when the adapter terminated it, and it had exited within
    /// the wait after its termination.
    Terminated,
    /// The owned Excel was still running when the adapter terminated it, and it was still running
    /// after its termination was requested and waited on.
    StillRunning,
}

impl ReportedExcel {
    /// What became of the reported Excel `pid`, in words.
    pub fn describe(&self, pid: Option<u32>) -> String {
        let named = pid.map_or_else(
            || "the reported Excel".to_owned(),
            |pid| format!("Excel PID {pid}"),
        );
        match self {
            ReportedExcel::SessionReady(receipt) => {
                format!("{named} returned to its session baseline; receipt {receipt}")
            }
            ReportedExcel::NotReported => {
                "the worker reported no Excel PID, so no Excel was terminated".to_owned()
            }
            ReportedExcel::Refused(why) => {
                format!("{named} was not terminated, being refused: {why}")
            }
            ReportedExcel::Unopened(why) => format!(
                "{named} could not be opened ({why}), so it was not terminated and may still run"
            ),
            ReportedExcel::Gone => {
                format!("{named} had already exited when the adapter came to open it")
            }
            ReportedExcel::Exited => format!("{named} exited without being terminated"),
            ReportedExcel::Terminated => format!("{named} was terminated and has exited"),
            ReportedExcel::StillRunning => format!(
                "{named} is still running after its termination was requested and waited on"
            ),
        }
    }
}

/// What one worker run did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerRun {
    pub worker_pid: u32,
    /// The Excel process the worker reported, if it reported one.
    pub excel_pid: Option<u32>,
    /// What became of the reported Excel: an owned one still running once the worker had exited
    /// and the grace period had passed was terminated.
    pub excel: ReportedExcel,
    pub exit_code: Option<i32>,
    /// The worker printed `done`: its result file is written.
    pub done: bool,
    /// The worker's standard output lines that are not protocol lines, and its standard error.
    pub other_output: Vec<String>,
    pub stderr: String,
}

/// A worker that did not finish within its timeout, and what was terminated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeoutReport {
    pub timeout: Duration,
    pub worker_pid: u32,
    /// The Excel process the worker had reported, if any.
    pub excel_pid: Option<u32>,
    /// What became of the reported Excel: an owned one was terminated.
    pub excel: ReportedExcel,
    /// Whether the termination of the worker was requested without an error.
    pub worker_terminated: bool,
    /// The image the processes below run ([`Worker::excel_image`]).
    pub excel_image: String,
    /// The running processes of the Excel image that are absent from the list taken before the
    /// worker started, listed after the terminations above (or why they could not be listed). An
    /// Excel the worker started without reporting it is among them, but the adapter cannot tell it
    /// from one started otherwise during the capture, so none of them is terminated.
    pub new_excel_processes: Result<Vec<u32>, String>,
    pub stderr: String,
}

/// The processes of `image` running now that are absent from `before`.
fn new_processes(image: &str, before: &BTreeSet<u32>) -> Result<Vec<u32>, String> {
    process::running(image).map(|now| now.difference(before).copied().collect())
}

/// What a report says of `new` (see [`TimeoutReport::new_excel_processes`]): nothing when there
/// are none.
pub(crate) fn new_processes_text(image: &str, new: &Result<Vec<u32>, String>) -> String {
    match new {
        Ok(pids) if pids.is_empty() => String::new(),
        Ok(pids) => format!(
            "; {image} processes absent from the list taken before the capture are running ({}), none of them terminated: the adapter cannot tell one the worker started without reporting it from one started otherwise",
            pids.iter()
                .map(|pid| format!("PID {pid}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Err(why) => format!(
            "; the {image} processes started during the capture cannot be listed ({why}), so an Excel the worker may have started is unaccounted for"
        ),
    }
}

enum Event {
    Line(String),
    Closed,
}

/// Reads `reader` line by line (lossily, so a stray byte cannot end the protocol), sending each
/// line and then `Closed`.
fn forward_lines(reader: impl Read, sender: mpsc::Sender<Event>) {
    let mut reader = BufReader::new(reader);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buffer)
                    .trim_end_matches(['\r', '\n'])
                    .to_owned();
                if sender.send(Event::Line(line)).is_err() {
                    return;
                }
            }
        }
    }
    let _ = sender.send(Event::Closed);
}

/// What one protocol line says.
#[derive(Debug, PartialEq, Eq)]
enum Line {
    ExcelPid(u32),
    Done,
    Other,
}

/// Reads one standard output line: a protocol line, or another line; an error for a protocol
/// line this adapter does not know.
fn parse_line(line: &str) -> Result<Line, String> {
    let Some(rest) = line
        .strip_prefix(PROTOCOL)
        .and_then(|rest| rest.strip_prefix(' '))
    else {
        return Ok(Line::Other);
    };
    if rest == "done" {
        return Ok(Line::Done);
    }
    if let Some(pid) = rest.strip_prefix("excel-pid ") {
        return pid
            .parse::<u32>()
            .ok()
            .filter(|pid| *pid != 0)
            .map(Line::ExcelPid)
            .ok_or_else(|| format!("the worker reported the Excel PID {pid:?}, which is not one"));
    }
    Err(format!(
        "the worker printed the unknown protocol line {line:?}"
    ))
}

/// Runs `worker` in isolation: lists the running processes of its Excel image, starts it, reads
/// its protocol, and terminates it and the Excel it reported (when that Excel is owned, see the
/// module documentation) when it does not finish within its timeout
/// ([`CaptureError::TimedOut`]). A protocol violation (an unknown protocol line, a second Excel
/// PID, a PID that cannot be opened while the worker runs) also terminates the worker and an owned
/// Excel and is a [`CaptureError::Worker`]. Off Windows nothing starts:
/// [`CaptureError::Unavailable`].
pub fn run_isolated(worker: &Worker) -> Result<WorkerRun, CaptureError> {
    if !cfg!(windows) {
        return Err(CaptureError::Unavailable {
            reason: format!(
                "an isolated Excel capture runs on Windows; this host is {}",
                std::env::consts::OS
            ),
        });
    }
    let before = process::running(&worker.excel_image).map_err(|message| CaptureError::Setup {
        message: format!(
            "cannot list the running {} processes before the capture: {message}",
            worker.excel_image
        ),
    })?;
    let mut command = Command::new(&worker.program);
    command
        .args(&worker.args)
        .current_dir(&worker.dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: the worker opens no console window.
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().map_err(|e| CaptureError::Worker {
        message: format!("cannot start the worker {}: {e}", worker.program.display()),
        stderr: String::new(),
    })?;
    let worker_pid = child.id();
    let (sender, events) = mpsc::channel();
    let stdout = child.stdout.take().expect("stdout is piped");
    thread::spawn(move || forward_lines(stdout, sender));
    let (stderr_sender, stderr_text) = mpsc::channel();
    let mut stderr = child.stderr.take().expect("stderr is piped");
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        let _ = stderr_sender.send(String::from_utf8_lossy(&bytes).into_owned());
    });
    let collect_stderr = |wait: Duration| {
        stderr_text
            .recv_timeout(wait)
            .unwrap_or_else(|_| "(the worker's standard error was still open)".to_owned())
    };

    let deadline = Instant::now() + worker.timeout;
    let mut state = State::new(before, &worker.excel_image);
    let mut closed = false;
    let status = loop {
        let mut failure = None;
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => failure = Some(format!("cannot read the worker's state: {e}")),
        }
        let now = Instant::now();
        if failure.is_none() && now < deadline {
            if closed {
                // The worker closed its output and is still running: wait for its exit.
                thread::sleep((deadline - now).min(Duration::from_millis(50)));
            } else {
                match events.recv_timeout((deadline - now).min(Duration::from_millis(100))) {
                    Ok(Event::Line(line)) => failure = state.read(line, false).err(),
                    Ok(Event::Closed) | Err(RecvTimeoutError::Disconnected) => closed = true,
                    Err(RecvTimeoutError::Timeout) => {}
                }
            }
        }
        let timed_out = failure.is_none() && Instant::now() >= deadline;
        if failure.is_some() || timed_out {
            let killed = child.kill();
            let worker_terminated = killed.is_ok();
            let _ = child.wait();
            let excel = state.settle(Duration::ZERO);
            let excel_pid = state.excel_pid();
            let new_excel_processes = new_processes(&worker.excel_image, &state.before);
            let stderr = collect_stderr(DRAIN);
            return Err(match failure {
                Some(message) => CaptureError::Worker {
                    message: format!(
                        "{message}; the worker (PID {worker_pid}) {}; {}{}",
                        match &killed {
                            Ok(()) => "was terminated".to_owned(),
                            Err(e) => format!("could not be terminated ({e})"),
                        },
                        excel.describe(excel_pid),
                        new_processes_text(&worker.excel_image, &new_excel_processes)
                    ),
                    stderr,
                },
                None => CaptureError::TimedOut(Box::new(TimeoutReport {
                    timeout: worker.timeout,
                    worker_pid,
                    excel_pid,
                    excel,
                    worker_terminated,
                    excel_image: worker.excel_image.clone(),
                    new_excel_processes,
                    stderr,
                })),
            });
        }
    };

    // The worker has exited: read what it printed before it did.
    let drain_until = Instant::now() + DRAIN;
    while !closed {
        let now = Instant::now();
        if now >= drain_until {
            break;
        }
        match events.recv_timeout(drain_until - now) {
            Ok(Event::Line(line)) => {
                if let Err(message) = state.read(line, true) {
                    let excel = state.settle(Duration::ZERO);
                    return Err(CaptureError::Worker {
                        message: format!(
                            "{message}; the worker (PID {worker_pid}) had exited; {}",
                            excel.describe(state.excel_pid())
                        ),
                        stderr: collect_stderr(DRAIN),
                    });
                }
            }
            Ok(Event::Closed) | Err(_) => closed = true,
        }
    }
    Ok(WorkerRun {
        worker_pid,
        excel_pid: state.excel_pid(),
        excel: state.settle(worker.exit_grace),
        exit_code: status.code(),
        done: state.done,
        other_output: state.other_output,
        stderr: collect_stderr(DRAIN),
    })
}

/// The outcome of an owned Excel: it exited by itself (`exited`, which waits for its exit), or
/// it was terminated (`terminate`, which terminates it and reports whether it has exited after
/// the wait that follows) and then exited, or it is still running after that.
fn settle(exited: impl FnOnce() -> bool, terminate: impl FnOnce() -> bool) -> ReportedExcel {
    if exited() {
        ReportedExcel::Exited
    } else if terminate() {
        ReportedExcel::Terminated
    } else {
        ReportedExcel::StillRunning
    }
}

/// The Excel process a worker reported, as the adapter holds it.
enum Reported {
    /// Started by this capture (absent from the list taken before the worker started, and of the
    /// Excel image): a handle on it, through which it is waited on and terminated.
    Owned(process::Handle),
    /// Reported only after the worker had exited, when no process had its PID any more: it had
    /// exited too.
    Gone,
    /// Not a process this capture started, or not one it can show it started; why. Never opened
    /// for termination, never terminated.
    Refused(String),
    /// A PID that could not be opened (why), while the worker ran or, after it had exited, for a
    /// reason other than that no process has the PID. Never terminated.
    Unopened(String),
}

/// What the worker's protocol has said so far: the Excel it reported, as held, whether it printed
/// `done`, and its other output; and what a reported Excel is checked against.
struct State {
    /// The PIDs of the processes of the Excel image running before the worker started.
    before: BTreeSet<u32>,
    image: String,
    excel: Option<(u32, Reported)>,
    done: bool,
    other_output: Vec<String>,
}

impl State {
    fn new(before: BTreeSet<u32>, image: &str) -> State {
        State {
            before,
            image: image.to_owned(),
            excel: None,
            done: false,
            other_output: Vec::new(),
        }
    }

    fn excel_pid(&self) -> Option<u32> {
        self.excel.as_ref().map(|(pid, _)| *pid)
    }

    /// What becomes of the reported Excel: an owned one that has not exited within `grace` is
    /// terminated, and the outcome is measured through its handle.
    fn settle(&self, grace: Duration) -> ReportedExcel {
        match &self.excel {
            None => ReportedExcel::NotReported,
            Some((_, Reported::Refused(why))) => ReportedExcel::Refused(why.clone()),
            Some((_, Reported::Unopened(why))) => ReportedExcel::Unopened(why.clone()),
            Some((_, Reported::Gone)) => ReportedExcel::Gone,
            Some((_, Reported::Owned(handle))) => {
                settle(|| handle.wait(grace), || handle.terminate())
            }
        }
    }

    /// Takes one standard output line in; an error for a protocol violation. A reported Excel in
    /// the list taken before the worker started, or of another image than the Excel image, is
    /// refused (see the module documentation).
    fn read(&mut self, line: String, exited: bool) -> Result<(), String> {
        match parse_line(&line)? {
            Line::Done => self.done = true,
            Line::Other => self.other_output.push(line),
            Line::ExcelPid(pid) => {
                if let Some(first) = self.excel_pid() {
                    return Err(format!(
                        "the worker reported a second Excel PID {pid} after {first}"
                    ));
                }
                if self.before.contains(&pid) {
                    self.excel = Some((
                        pid,
                        Reported::Refused(format!(
                            "the worker reported Excel PID {pid}, a {} process that was already running before the capture started, so not one this capture started; it is not terminated",
                            self.image
                        )),
                    ));
                } else {
                    self.take(pid, process::Handle::open(pid), exited)?;
                }
            }
        }
        Ok(())
    }

    /// Holds the reported Excel `pid` as `opened` allows. An opened process of the Excel image is
    /// owned and one of another image refused. While the worker runs (`exited` false), a PID that
    /// cannot be opened is a protocol violation, since the adapter could not terminate it; it is
    /// kept as not opened, so the failure names it. Once the worker has exited, a PID that no
    /// process has any more had exited, and one that cannot be opened for another reason (access
    /// denied, say) is kept as not opened.
    fn take(
        &mut self,
        pid: u32,
        opened: Result<process::Handle, process::OpenError>,
        exited: bool,
    ) -> Result<(), String> {
        let reported = match opened {
            Ok(handle) => match handle.image_name() {
                Ok(name) if name.eq_ignore_ascii_case(&self.image) => Reported::Owned(handle),
                Ok(name) => Reported::Refused(format!(
                    "the worker reported Excel PID {pid}, whose image is {name}, not {}; it is not terminated",
                    self.image
                )),
                Err(message) => Reported::Refused(format!(
                    "the worker reported Excel PID {pid}, whose image cannot be read ({message}); it is not terminated"
                )),
            },
            Err(error) if exited && error.no_such_process => Reported::Gone,
            Err(error) if exited => Reported::Unopened(error.message),
            Err(error) => {
                let violation = format!(
                    "{} while the worker was running, so the adapter could not have terminated it",
                    error.message
                );
                self.excel = Some((pid, Reported::Unopened(error.message)));
                return Err(violation);
            }
        };
        self.excel = Some((pid, reported));
        Ok(())
    }
}

/// The processes of an image and a handle on a process the worker reported, through which it is
/// identified, waited on and terminated.
#[cfg(windows)]
mod process {
    use std::collections::BTreeSet;
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_INVALID_PARAMETER, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        PROCESS_TERMINATE, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
    };

    /// The text of a NUL-terminated UTF-16 buffer.
    fn text(units: &[u16]) -> String {
        let end = units
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(units.len());
        String::from_utf16_lossy(&units[..end])
    }

    /// The PIDs of the running processes whose image file name is `image` (without regard to
    /// case), from a snapshot of every process.
    pub(crate) fn running(image: &str) -> Result<BTreeSet<u32>, String> {
        // SAFETY: CreateToolhelp32Snapshot takes plain values and returns INVALID_HANDLE_VALUE on
        // failure.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(format!(
                "CreateToolhelp32Snapshot failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..PROCESSENTRY32W::default()
        };
        let mut pids = BTreeSet::new();
        // SAFETY: snapshot is a valid snapshot handle and entry a PROCESSENTRY32W whose dwSize is
        // set, as Process32FirstW and Process32NextW require.
        let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
        while more {
            if text(&entry.szExeFile).eq_ignore_ascii_case(image) {
                pids.insert(entry.th32ProcessID);
            }
            // SAFETY: as above.
            more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
        }
        // SAFETY: snapshot was opened above and is closed exactly once, here.
        unsafe { CloseHandle(snapshot) };
        Ok(pids)
    }

    pub(crate) use super::OpenError;

    pub(crate) struct Handle(HANDLE);

    impl Handle {
        /// A handle on process `pid`, opened for identification, waiting and termination only.
        /// OpenProcess fails with ERROR_INVALID_PARAMETER when no process has the PID.
        pub(crate) fn open(pid: u32) -> Result<Handle, OpenError> {
            // SAFETY: OpenProcess takes plain values and returns a null handle on failure.
            let handle = unsafe {
                OpenProcess(
                    PROCESS_TERMINATE | PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                    0,
                    pid,
                )
            };
            if handle.is_null() {
                let error = std::io::Error::last_os_error();
                return Err(OpenError {
                    no_such_process: error.raw_os_error()
                        == i32::try_from(ERROR_INVALID_PARAMETER).ok(),
                    message: format!("cannot open the reported Excel process {pid}: {error}"),
                });
            }
            Ok(Handle(handle))
        }

        /// The file name of the image the process runs (`EXCEL.EXE`).
        pub(crate) fn image_name(&self) -> Result<String, String> {
            let mut buffer = vec![0u16; 32_768];
            let mut size = buffer.len() as u32;
            // SAFETY: self.0 is an open process handle with PROCESS_QUERY_LIMITED_INFORMATION
            // access, and buffer holds size UTF-16 units.
            let ok = unsafe {
                QueryFullProcessImageNameW(
                    self.0,
                    PROCESS_NAME_WIN32,
                    buffer.as_mut_ptr(),
                    &mut size,
                )
            };
            if ok == 0 {
                return Err(format!(
                    "QueryFullProcessImageNameW failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let path = String::from_utf16_lossy(&buffer[..size as usize]);
            Ok(path
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or_default()
                .to_owned())
        }

        /// Whether the process has exited within `timeout`.
        pub(crate) fn wait(&self, timeout: Duration) -> bool {
            let millis = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX - 1);
            // SAFETY: self.0 is an open process handle owned by this value.
            unsafe { WaitForSingleObject(self.0, millis) == WAIT_OBJECT_0 }
        }

        /// Terminates the process and waits up to ten seconds for it to exit; whether it has.
        pub(crate) fn terminate(&self) -> bool {
            // SAFETY: self.0 is an open process handle with PROCESS_TERMINATE access. A process
            // that has already exited makes this fail, which the wait below then reports as exited.
            unsafe { TerminateProcess(self.0, 1) };
            self.wait(Duration::from_secs(10))
        }
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: self.0 was opened by OpenProcess and is closed exactly once, here.
            unsafe { CloseHandle(self.0) };
        }
    }
}

/// Why a reported PID could not be opened.
#[derive(Debug)]
pub(crate) struct OpenError {
    /// No process has the PID (it has exited, or never existed).
    pub(crate) no_such_process: bool,
    pub(crate) message: String,
}

/// Off Windows no worker starts ([`run_isolated`] refuses first), so no process is listed and no
/// handle is ever opened.
#[cfg(not(windows))]
mod process {
    pub(crate) use super::OpenError;
    use std::collections::BTreeSet;
    use std::time::Duration;

    pub(crate) fn running(image: &str) -> Result<BTreeSet<u32>, String> {
        Err(format!(
            "the running {image} processes are listed on Windows only here"
        ))
    }

    pub(crate) struct Handle;

    impl Handle {
        pub(crate) fn open(pid: u32) -> Result<Handle, OpenError> {
            Err(OpenError {
                no_such_process: false,
                message: format!(
                    "cannot open the reported Excel process {pid}: process handles are Windows only here"
                ),
            })
        }

        pub(crate) fn image_name(&self) -> Result<String, String> {
            Err("process images are read on Windows only here".to_owned())
        }

        pub(crate) fn wait(&self, _timeout: Duration) -> bool {
            true
        }

        pub(crate) fn terminate(&self) -> bool {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_lines_are_read_exactly() {
        assert_eq!(
            parse_line("dnacalc-capture-worker/1 excel-pid 4242"),
            Ok(Line::ExcelPid(4242))
        );
        assert_eq!(parse_line("dnacalc-capture-worker/1 done"), Ok(Line::Done));
        assert_eq!(parse_line("WARNING: something"), Ok(Line::Other));
        assert_eq!(
            parse_line("dnacalc-capture-worker/10 done"),
            Ok(Line::Other)
        );
        for bad in [
            "dnacalc-capture-worker/1 excel-pid 0",
            "dnacalc-capture-worker/1 excel-pid -3",
            "dnacalc-capture-worker/1 excel-pid 12 13",
            "dnacalc-capture-worker/1 finished",
            "dnacalc-capture-worker/1 ",
        ] {
            assert!(parse_line(bad).is_err(), "{bad:?}");
        }
    }

    /// The three outcomes of an owned Excel stay apart: one that exits within the grace period is
    /// never terminated; one that does not is terminated, and is `Terminated` only when it has
    /// exited after that and `StillRunning` when it has not. Each is described as measured.
    #[test]
    fn an_owned_excel_exited_terminated_or_still_running() {
        let terminate_called = std::cell::Cell::new(false);
        assert_eq!(
            settle(
                || true,
                || {
                    terminate_called.set(true);
                    true
                }
            ),
            ReportedExcel::Exited
        );
        assert!(
            !terminate_called.get(),
            "an Excel that exited is never terminated"
        );
        assert_eq!(settle(|| false, || true), ReportedExcel::Terminated);
        assert_eq!(settle(|| false, || false), ReportedExcel::StillRunning);
        assert_eq!(
            ReportedExcel::StillRunning.describe(Some(77)),
            "Excel PID 77 is still running after its termination was requested and waited on"
        );
        assert_eq!(
            ReportedExcel::Terminated.describe(Some(77)),
            "Excel PID 77 was terminated and has exited"
        );
        assert_eq!(
            ReportedExcel::Exited.describe(Some(77)),
            "Excel PID 77 exited without being terminated"
        );
    }

    /// A reported Excel that cannot be opened is a protocol failure while the worker runs (it
    /// could not be terminated), and the failure keeps its PID; once the worker has exited, a PID
    /// that no process has any more had exited.
    #[test]
    fn an_excel_pid_that_cannot_be_opened() {
        // PIDs are multiples of four, so this one names no process.
        let line = format!("{PROTOCOL} excel-pid 4294967293");
        let mut running = State::new(BTreeSet::new(), EXCEL_IMAGE);
        let error = running.read(line.clone(), false).unwrap_err();
        assert!(
            error.contains("cannot open the reported Excel process 4294967293"),
            "{error}"
        );
        assert_eq!(running.excel_pid(), Some(4294967293));
        let described = running.settle(Duration::ZERO).describe(running.excel_pid());
        assert!(
            described.starts_with("Excel PID 4294967293 could not be opened (")
                && described.ends_with("so it was not terminated and may still run"),
            "{described}"
        );
        let mut exited = State::new(BTreeSet::new(), EXCEL_IMAGE);
        exited.read(line.clone(), true).unwrap();
        if cfg!(windows) {
            assert!(matches!(exited.excel, Some((4294967293, Reported::Gone))));
            assert_eq!(exited.settle(Duration::ZERO), ReportedExcel::Gone);
        } else {
            assert!(matches!(
                exited.excel,
                Some((4294967293, Reported::Unopened(_)))
            ));
        }
        let error = exited.read(line, true).unwrap_err();
        assert!(error.contains("a second Excel PID"), "{error}");
    }

    /// Once the worker has exited, only a PID that no process has reads as exited; a PID that
    /// cannot be opened for another reason (access denied) is its own outcome, never terminated.
    #[test]
    fn an_open_failure_after_the_exit_is_gone_only_when_no_process_has_the_pid() {
        let denied = || OpenError {
            no_such_process: false,
            message: "cannot open the reported Excel process 8: Access is denied.".to_owned(),
        };
        let mut state = State::new(BTreeSet::new(), EXCEL_IMAGE);
        state.take(8, Err(denied()), true).unwrap();
        assert_eq!(
            state.settle(Duration::ZERO),
            ReportedExcel::Unopened(
                "cannot open the reported Excel process 8: Access is denied.".to_owned()
            )
        );
        let mut state = State::new(BTreeSet::new(), EXCEL_IMAGE);
        state
            .take(
                8,
                Err(OpenError {
                    no_such_process: true,
                    message: "gone".to_owned(),
                }),
                true,
            )
            .unwrap();
        assert_eq!(state.settle(Duration::ZERO), ReportedExcel::Gone);
        // While the worker runs, either is a protocol failure that keeps the PID.
        let mut state = State::new(BTreeSet::new(), EXCEL_IMAGE);
        let error = state.take(8, Err(denied()), false).unwrap_err();
        assert!(error.contains("while the worker was running"), "{error}");
        assert!(matches!(state.excel, Some((8, Reported::Unopened(_)))));
    }

    /// The System process (PID 4) exists and cannot be opened for termination by a test process:
    /// the open fails with an error that is not "no such process"; a PID no process has is.
    #[cfg(windows)]
    #[test]
    fn opening_a_protected_process_is_not_read_as_gone() {
        match process::Handle::open(4) {
            Ok(_) => panic!("the System process opened for termination"),
            Err(error) => assert!(!error.no_such_process, "{error:?}"),
        }
        match process::Handle::open(4294967293) {
            Ok(_) => panic!("a PID that is not a multiple of four opened"),
            Err(error) => assert!(error.no_such_process, "{error:?}"),
        }
    }

    /// A reported PID in the list taken before the worker started is refused before anything is
    /// opened on it, whether or not it still runs.
    #[test]
    fn a_reported_pid_running_before_the_capture_is_refused_unopened() {
        let line = format!("{PROTOCOL} excel-pid 4294967293");
        let mut state = State::new(BTreeSet::from([4294967293]), EXCEL_IMAGE);
        state.read(line, false).unwrap();
        let Some((4294967293, Reported::Refused(refusal))) = &state.excel else {
            panic!("the PID is not refused");
        };
        assert!(
            refusal.contains("PID 4294967293, a EXCEL.EXE process that was already running before the capture started"),
            "{refusal}"
        );
    }

    /// A reported PID of another image than the Excel image is refused: here the test process
    /// itself, which runs and is not EXCEL.EXE.
    #[cfg(windows)]
    #[test]
    fn a_reported_pid_of_another_image_is_refused() {
        let pid = std::process::id();
        let mut state = State::new(BTreeSet::new(), EXCEL_IMAGE);
        state
            .read(format!("{PROTOCOL} excel-pid {pid}"), false)
            .unwrap();
        let ReportedExcel::Refused(refusal) = state.settle(Duration::ZERO) else {
            panic!("the test process is not refused");
        };
        let image = std::env::current_exe()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(
            refusal.contains(&format!("whose image is {image}, not EXCEL.EXE")),
            "{refusal}"
        );
        // The same process, with its own image as the Excel image, is owned.
        let mut owned = State::new(BTreeSet::new(), &image.to_ascii_uppercase());
        owned
            .read(format!("{PROTOCOL} excel-pid {pid}"), false)
            .unwrap();
        assert!(matches!(owned.excel, Some((_, Reported::Owned(_)))));
    }

    /// The process list holds a process of the image started before it was taken.
    #[cfg(windows)]
    #[test]
    fn the_process_list_holds_the_running_processes_of_an_image() {
        let image = std::env::current_exe()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let listed = process::running(&image.to_ascii_lowercase()).unwrap();
        assert!(listed.contains(&std::process::id()), "{listed:?}");
        assert!(
            !process::running("no-such-image-dnacalc.exe")
                .unwrap()
                .contains(&std::process::id())
        );
    }

    /// A worker that runs `command` in Windows PowerShell, with powershell.exe as its Excel image.
    #[cfg(windows)]
    fn powershell_worker(command: String, timeout: Duration) -> Worker {
        Worker {
            program: crate::windows_powershell(),
            args: ["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"]
                .into_iter()
                .map(OsString::from)
                .chain([OsString::from(command)])
                .collect(),
            dir: std::env::temp_dir(),
            timeout,
            exit_grace: Duration::from_secs(1),
            excel_image: "powershell.exe".to_owned(),
        }
    }

    /// A worker that reports a PID that was running before it started (standing for the owner's
    /// Excel) completes, with the refusal carried and that process left running, on a normal exit
    /// and on a timeout alike.
    #[cfg(windows)]
    #[test]
    fn a_worker_reporting_a_process_from_the_list_terminates_nothing_of_it() {
        use std::os::windows::process::CommandExt;
        // The stand-in for the owner's Excel: a PowerShell process started before the worker.
        let mut owner = Command::new(crate::windows_powershell())
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 120",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .spawn()
            .unwrap();
        let owner_pid = owner.id();
        let worker = |hang: bool, timeout: Duration| {
            powershell_worker(
                format!(
                    "[Console]::Out.WriteLine('{PROTOCOL} excel-pid {owner_pid}'); [Console]::Out.Flush(); {}",
                    if hang {
                        "Start-Sleep -Seconds 120"
                    } else {
                        "exit 0"
                    }
                ),
                timeout,
            )
        };
        let run = run_isolated(&worker(false, Duration::from_secs(60))).unwrap();
        assert_eq!(run.excel_pid, Some(owner_pid));
        let ReportedExcel::Refused(refusal) = &run.excel else {
            panic!("the owner's stand-in is not refused: {run:?}");
        };
        assert!(
            refusal.contains("already running before the capture started"),
            "{refusal}"
        );
        assert!(
            matches!(owner.try_wait(), Ok(None)),
            "the owner's stand-in was terminated"
        );

        match run_isolated(&worker(true, Duration::from_secs(5))) {
            Err(CaptureError::TimedOut(report)) => {
                assert_eq!(report.excel_pid, Some(owner_pid));
                assert!(
                    matches!(report.excel, ReportedExcel::Refused(_)),
                    "{report:?}"
                );
                assert!(report.worker_terminated);
            }
            other => panic!("a hanging worker must time out, got {other:?}"),
        }
        assert!(
            matches!(owner.try_wait(), Ok(None)),
            "the owner's stand-in was terminated"
        );
        let _ = owner.kill();
        let _ = owner.wait();
    }

    /// A worker that starts a process of the Excel image and hangs without reporting it: on the
    /// timeout the run lists that process among the ones absent from the list taken before the
    /// worker started, and terminates none of them (it still runs afterwards).
    #[cfg(windows)]
    #[test]
    fn a_timeout_before_the_excel_pid_lists_new_excel_processes_and_terminates_none() {
        let started = std::env::temp_dir().join(format!(
            "calc-oracle-excel-unreported-{}.txt",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&started);
        let command = format!(
            "$p = Start-Process -FilePath (Join-Path $env:SystemRoot 'System32\\WindowsPowerShell\\v1.0\\powershell.exe') -ArgumentList '-NoProfile','-NonInteractive','-Command','Start-Sleep -Seconds 120' -WindowStyle Hidden -PassThru; Set-Content -LiteralPath '{}' -Value $p.Id; Start-Sleep -Seconds 120",
            started.display()
        );
        let report = match run_isolated(&powershell_worker(command, Duration::from_secs(15))) {
            Err(CaptureError::TimedOut(report)) => report,
            other => panic!("a hanging worker must time out, got {other:?}"),
        };
        let standin: u32 = std::fs::read_to_string(&started)
            .expect("the worker wrote its stand-in's PID")
            .trim()
            .trim_start_matches('\u{feff}')
            .parse()
            .expect("a PID");
        let _ = std::fs::remove_file(&started);
        let still_running = process::running("powershell.exe")
            .unwrap()
            .contains(&standin);
        // Clean up before asserting, so a failure leaves nothing behind.
        if let Ok(handle) = process::Handle::open(standin) {
            handle.terminate();
        }
        assert_eq!(report.excel_pid, None);
        assert_eq!(report.excel, ReportedExcel::NotReported);
        let new = report.new_excel_processes.clone().unwrap();
        assert!(
            new.contains(&standin),
            "the unreported stand-in (PID {standin}) is not listed: {new:?}"
        );
        assert!(still_running, "the unreported stand-in was terminated");
        let text = CaptureError::TimedOut(report).to_string();
        assert!(
            text.contains(&format!("PID {standin}")) && text.contains("none of them terminated"),
            "{text}"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn nothing_starts_off_windows() {
        let worker = Worker {
            program: PathBuf::from("/bin/true"),
            args: Vec::new(),
            dir: std::env::temp_dir(),
            timeout: Duration::from_secs(1),
            exit_grace: Duration::from_secs(1),
            excel_image: EXCEL_IMAGE.to_owned(),
        };
        assert!(matches!(
            run_isolated(&worker),
            Err(CaptureError::Unavailable { .. })
        ));
    }
}
