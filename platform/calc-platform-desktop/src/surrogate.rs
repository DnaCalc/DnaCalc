//! The add-in surrogate's process boundary (docs/architecture/ADDIN_SURROGATE.md sections 6 and
//! 10): the bytes between the controller and its surrogate, and on Windows the launch, the job
//! object and the supervisor.
//!
//! It moves bytes and never decodes a payload: the frames are `calc-surrogate-wire`'s, and each
//! application decodes its own seam messages. A [`Wire`] is one end of a byte connection with a
//! bounded wait, so the controller's wait can keep the window responsive. [`StreamWire`] reads a
//! pipe on a thread of its own (a child's stdout, the surrogate's stdin) and builds on every
//! native target; [`wire_pair`] joins two ends in one process (the in-process tests).
//!
//! [`Supervisor`] starts the surrogate (the same executable in its surrogate role) with its
//! standard input and output as the connection, holds it in a job object that ends it and every
//! process it started when the supervisor ends it or the controller exits, and ends it without
//! waiting for its teardown (a live worker can keep a process from draining indefinitely). It is
//! Windows-only: elsewhere [`Supervisor::launch`] refuses.

use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::Duration;

pub use calc_surrogate_wire::Incarnation;

/// What a bounded wait on a [`Wire`] gave.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Received {
    /// Bytes, in the order they were written.
    Bytes(Vec<u8>),
    /// Nothing within the wait.
    Quiet,
    /// The other end closed (it exited, or was ended); nothing more arrives.
    Closed,
}

/// One end of a byte connection.
pub trait Wire {
    /// Writes all of `bytes`.
    fn send(&mut self, bytes: &[u8]) -> Result<(), String>;
    /// What arrives within `wait`.
    fn receive(&mut self, wait: Duration) -> Received;
}

/// A pipe read on its own thread and written in place.
pub struct StreamWire<W: Write> {
    writer: W,
    inbox: mpsc::Receiver<Vec<u8>>,
    closed: bool,
}

impl<W: Write> StreamWire<W> {
    /// The end that writes to `writer` and reads `reader` (on a thread that ends when the reader
    /// does).
    pub fn new<R: Read + Send + 'static>(mut reader: R, writer: W) -> Self {
        let (sender, inbox) = mpsc::channel();
        std::thread::Builder::new()
            .name("surrogate-wire".to_owned())
            .spawn(move || {
                let mut buffer = vec![0u8; 64 * 1024];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if sender.send(buffer[..n].to_vec()).is_err() {
                                break;
                            }
                        }
                    }
                }
            })
            .expect("a thread for the surrogate's pipe");
        StreamWire {
            writer,
            inbox,
            closed: false,
        }
    }
}

impl<W: Write> Wire for StreamWire<W> {
    fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.writer
            .write_all(bytes)
            .and_then(|()| self.writer.flush())
            .map_err(|e| format!("the add-in process's pipe: {e}"))
    }

    fn receive(&mut self, wait: Duration) -> Received {
        if self.closed {
            return Received::Closed;
        }
        let first = match self.inbox.recv_timeout(wait) {
            Ok(bytes) => bytes,
            Err(mpsc::RecvTimeoutError::Timeout) => return Received::Quiet,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.closed = true;
                return Received::Closed;
            }
        };
        // Whatever else is already here comes with it.
        let mut bytes = first;
        while let Ok(more) = self.inbox.try_recv() {
            bytes.extend_from_slice(&more);
        }
        Received::Bytes(bytes)
    }
}

/// The surrogate's end of the connection: its standard input, and the standard output it was
/// started with. On Windows the process's standard output is then pointed at its standard error,
/// so that what an add-in prints (a Python `print`, a C runtime `printf` initialised later) never
/// lands among the frames.
pub fn surrogate_wire() -> Result<StreamWire<Box<dyn Write>>, String> {
    #[cfg(windows)]
    {
        use std::os::windows::io::FromRawHandle;
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::System::Console::{
            GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
        };
        // SAFETY: standard handle queries and replacement for this process.
        let (out, err) = unsafe { (GetStdHandle(STD_OUTPUT_HANDLE), GetStdHandle(STD_ERROR_HANDLE)) };
        if out.is_null() || out == INVALID_HANDLE_VALUE {
            return Err("the add-in process has no output pipe".to_owned());
        }
        // SAFETY: as above; `err` may be null (no standard error), which drops prints.
        if unsafe { SetStdHandle(STD_OUTPUT_HANDLE, err) } == 0 {
            return Err(format!(
                "the add-in process's output could not be moved: {}",
                std::io::Error::last_os_error()
            ));
        }
        // SAFETY: the pipe handle this process was started with; from here only this file uses it.
        let frames = unsafe { std::fs::File::from_raw_handle(out) };
        Ok(StreamWire::new(std::io::stdin(), Box::new(frames)))
    }
    #[cfg(not(windows))]
    {
        Ok(StreamWire::new(std::io::stdin(), Box::new(std::io::stdout())))
    }
}

/// One end of a connection inside one process ([`wire_pair`]).
pub struct ChannelWire {
    to: Option<mpsc::Sender<Vec<u8>>>,
    from: mpsc::Receiver<Vec<u8>>,
    closed: bool,
}

/// Two joined ends in one process: what one sends the other receives.
pub fn wire_pair() -> (ChannelWire, ChannelWire) {
    let (a_to, b_from) = mpsc::channel();
    let (b_to, a_from) = mpsc::channel();
    (
        ChannelWire {
            to: Some(a_to),
            from: a_from,
            closed: false,
        },
        ChannelWire {
            to: Some(b_to),
            from: b_from,
            closed: false,
        },
    )
}

impl ChannelWire {
    /// Closes this end: the other end receives [`Received::Closed`] after what was sent.
    pub fn close(&mut self) {
        self.to = None;
    }
}

impl Wire for ChannelWire {
    fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.to
            .as_ref()
            .ok_or_else(|| "this end is closed".to_owned())?
            .send(bytes.to_vec())
            .map_err(|_| "the other end is closed".to_owned())
    }

    fn receive(&mut self, wait: Duration) -> Received {
        if self.closed {
            return Received::Closed;
        }
        match self.from.recv_timeout(wait) {
            Ok(mut bytes) => {
                while let Ok(more) = self.from.try_recv() {
                    bytes.extend_from_slice(&more);
                }
                Received::Bytes(bytes)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => Received::Quiet,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.closed = true;
                Received::Closed
            }
        }
    }
}

/// 128 bits the operating system's per-process random keys seed (std's `RandomState`), mixed with
/// the clock and the process ID, for an [`Incarnation`]: never the process ID alone.
pub fn random_bits() -> u128 {
    use std::hash::{BuildHasher, Hasher};
    let clock = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let mut halves = [0u64; 2];
    for (index, half) in halves.iter_mut().enumerate() {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u128(clock);
        hasher.write_u32(std::process::id());
        hasher.write_usize(index);
        *half = hasher.finish();
    }
    (u128::from(halves[0]) << 64) | u128::from(halves[1])
}

/// The command-line argument that starts an executable in its surrogate role, followed by the
/// incarnation's text ([`Incarnation::to_text`]).
pub const SURROGATE_ARG: &str = "--surrogate";

/// How a surrogate ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ended {
    /// Still running.
    Running,
    /// It exited by itself, with this code (a crash is an exit code too: an access violation is
    /// 0xC0000005).
    Exited(Option<i32>),
    /// The supervisor ended it.
    Stopped,
    /// No surrogate was started, or it was already reported.
    None,
}

/// The surrogate processes of one controller: at most one running, each under a new incarnation.
pub struct Supervisor {
    /// How many surrogates this controller has started: the incarnation's start counter.
    starts: u32,
    #[cfg(windows)]
    current: Option<windows_job::Running>,
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl Supervisor {
    pub fn new() -> Self {
        Supervisor {
            starts: 0,
            #[cfg(windows)]
            current: None,
        }
    }

    /// How many surrogates have been started.
    pub fn starts(&self) -> u32 {
        self.starts
    }

    /// Starts `exe` in its surrogate role ([`SURROGATE_ARG`] and a new incarnation, then `args`),
    /// in a job object, with its standard input and output as the returned wire. A surrogate still
    /// running is ended first. Standard error is inherited (the surrogate's diagnostics).
    #[cfg(windows)]
    pub fn launch(
        &mut self,
        exe: &std::path::Path,
        args: &[std::ffi::OsString],
    ) -> Result<(Incarnation, StreamWire<std::process::ChildStdin>), String> {
        self.stop();
        self.starts += 1;
        let incarnation = Incarnation::from_parts(random_bits(), self.starts);
        let (running, wire) = windows_job::Running::start(exe, &incarnation, args)?;
        self.current = Some(running);
        Ok((incarnation, wire))
    }

    /// Off Windows there is no job object, so no surrogate is started.
    #[cfg(not(windows))]
    pub fn launch(
        &mut self,
        exe: &std::path::Path,
        _args: &[std::ffi::OsString],
    ) -> Result<(Incarnation, StreamWire<std::process::ChildStdin>), String> {
        Err(format!(
            "an add-in process ({}) is started only on Windows, where a job object holds it",
            exe.display()
        ))
    }

    /// Ends the running surrogate and every process it started, without waiting for them to
    /// drain. Its incarnation is retired: the caller's link rejects anything more from it.
    pub fn stop(&mut self) -> Ended {
        #[cfg(windows)]
        if let Some(mut running) = self.current.take() {
            running.terminate();
            return Ended::Stopped;
        }
        Ended::None
    }

    /// Waits up to `wait` for the running surrogate to exit by itself (its input was closed: it
    /// closes its add-ins first), then ends it if it has not.
    pub fn finish(&mut self, wait: Duration) -> Ended {
        let deadline = std::time::Instant::now() + wait;
        loop {
            match self.poll() {
                Ended::Running if std::time::Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                Ended::Running => return self.stop(),
                other => return other,
            }
        }
    }

    /// Whether the running surrogate has ended by itself. An ended one is reported once and then
    /// forgotten.
    pub fn poll(&mut self) -> Ended {
        #[cfg(windows)]
        if let Some(running) = &mut self.current {
            return match running.exited() {
                None => Ended::Running,
                Some(code) => {
                    self.current = None;
                    Ended::Exited(code)
                }
            };
        }
        Ended::None
    }

    /// The running surrogate's process ID (for a report; never an identity).
    pub fn process_id(&self) -> Option<u32> {
        #[cfg(windows)]
        if let Some(running) = &self.current {
            return Some(running.child.id());
        }
        None
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(windows)]
mod windows_job {
    use super::*;
    use std::os::windows::io::AsRawHandle;
    use std::process::{Child, ChildStdin, Command, Stdio};
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    };

    /// A started surrogate and the job that holds it.
    pub(super) struct Running {
        pub(super) child: Child,
        job: HANDLE,
    }

    impl Running {
        pub(super) fn start(
            exe: &std::path::Path,
            incarnation: &Incarnation,
            args: &[std::ffi::OsString],
        ) -> Result<(Running, StreamWire<ChildStdin>), String> {
            // SAFETY: an unnamed job with default security.
            let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if job.is_null() {
                return Err(format!(
                    "no job object for the add-in process: {}",
                    std::io::Error::last_os_error()
                ));
            }
            // Closing the job's last handle (the controller ends, however) ends every process in it.
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            // SAFETY: `limits` is the structure the information class names, of its size.
            let set = unsafe {
                SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    (&raw const limits).cast(),
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            if set == 0 {
                let error = std::io::Error::last_os_error();
                // SAFETY: the handle was created above.
                unsafe { CloseHandle(job) };
                return Err(format!("the add-in process's job object: {error}"));
            }
            let mut command = Command::new(exe); // ubs:ignore - The executable is this application's own image in its surrogate role.
            command
                .arg(SURROGATE_ARG)
                .arg(incarnation.to_text())
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit());
            let mut child = match command.spawn() {
                Ok(child) => child,
                Err(e) => {
                    // SAFETY: the handle was created above.
                    unsafe { CloseHandle(job) };
                    return Err(format!("the add-in process did not start ({}): {e}", exe.display()));
                }
            };
            // The surrogate starts nothing before it has read its first request, so assigning it
            // after the start leaves no child outside the job.
            // SAFETY: both handles are live; the child's handle is owned by `child`.
            let assigned =
                unsafe { AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE) };
            if assigned == 0 {
                let error = std::io::Error::last_os_error();
                let _ = child.kill();
                // SAFETY: the handle was created above.
                unsafe { CloseHandle(job) };
                return Err(format!("the add-in process could not join its job: {error}"));
            }
            let stdin = child.stdin.take().ok_or("the add-in process has no input pipe")?;
            let stdout = child.stdout.take().ok_or("the add-in process has no output pipe")?;
            Ok((Running { child, job }, StreamWire::new(stdout, stdin)))
        }

        /// The exit code once it has exited by itself.
        pub(super) fn exited(&mut self) -> Option<Option<i32>> {
            match self.child.try_wait() {
                Ok(Some(status)) => Some(status.code()),
                Ok(None) => None,
                Err(_) => Some(None),
            }
        }

        /// Ends every process in the job, without waiting.
        pub(super) fn terminate(&mut self) {
            if self.job.is_null() {
                return;
            }
            // SAFETY: the job handle is live until it is closed here, once.
            unsafe {
                TerminateJobObject(self.job, 1);
                CloseHandle(self.job);
            }
            self.job = std::ptr::null_mut();
        }
    }

    impl Drop for Running {
        fn drop(&mut self) {
            self.terminate();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_surrogate_wire::{ControllerLink, Incoming, Kind, SurrogateIncoming, SurrogateLink};

    const LIMIT: usize = 1 << 20;

    /// Native-only: threads and pipes. The two link ends talk over a joined pair with the surrogate
    /// on a thread of its own, as a process would.
    #[test]
    fn a_link_runs_over_a_wire_pair_between_threads() {
        let (mut near, mut far) = wire_pair();
        let incarnation = Incarnation::from_parts(random_bits(), 1);
        let surrogate = std::thread::spawn(move || {
            let mut link = SurrogateLink::new(incarnation, LIMIT);
            far.send(&link.hello()).unwrap();
            loop {
                match far.receive(Duration::from_secs(5)) {
                    Received::Bytes(bytes) => link.receive(&bytes),
                    Received::Quiet => panic!("no request"),
                    Received::Closed => return,
                }
                while let Some(next) = link.next().unwrap() {
                    if let SurrogateIncoming::Request {
                        request, payload, ..
                    } = next
                    {
                        let mut answer = payload.clone();
                        answer.reverse();
                        far.send(&link.reply(request, Kind(2), answer).unwrap()).unwrap();
                    }
                }
            }
        });
        let mut link = ControllerLink::new(incarnation, LIMIT);
        let mut replies = Vec::new();
        let mut asked = false;
        while replies.len() < 2 {
            match near.receive(Duration::from_secs(5)) {
                Received::Bytes(bytes) => link.receive(&bytes),
                other => panic!("{other:?}"),
            }
            while let Some(next) = link.next().unwrap() {
                match next {
                    Incoming::Hello => {}
                    Incoming::Reply { payload, .. } => replies.push(payload),
                    other => panic!("{other:?}"),
                }
            }
            if link.ready() && !asked {
                asked = true;
                for text in [&b"abc"[..], b"xyz"] {
                    let (_, bytes) = link.request(Kind(1), Kind(2), text.to_vec()).unwrap();
                    near.send(&bytes).unwrap();
                }
            }
        }
        assert_eq!(replies, vec![b"cba".to_vec(), b"zyx".to_vec()]);
        near.close();
        surrogate.join().unwrap();
    }

    /// Native-only: a closed end is seen as closed, after what was sent before it closed.
    #[test]
    fn a_closed_end_is_seen_after_its_last_bytes() {
        let (mut near, mut far) = wire_pair();
        far.send(b"last").unwrap();
        drop(far);
        assert_eq!(
            near.receive(Duration::from_millis(100)),
            Received::Bytes(b"last".to_vec())
        );
        assert_eq!(near.receive(Duration::from_millis(100)), Received::Closed);
        assert_eq!(near.receive(Duration::ZERO), Received::Closed);
        assert_eq!(
            wire_pair().0.receive(Duration::from_millis(1)),
            Received::Quiet
        );
    }

    /// Native-only: random bits differ from call to call, so two starts never share an incarnation.
    #[test]
    fn incarnations_get_fresh_random_bits() {
        let a = random_bits();
        let b = random_bits();
        assert_ne!(a, b);
        assert_ne!(a >> 64, 0);
    }

    /// Native-only: the supervisor on a platform without job objects refuses to start a surrogate.
    #[cfg(not(windows))]
    #[test]
    fn off_windows_no_surrogate_is_started() {
        let mut supervisor = Supervisor::new();
        let refused = supervisor
            .launch(std::path::Path::new("/bin/true"), &[])
            .err()
            .expect("refused");
        assert!(refused.contains("only on Windows"), "{refused}");
        assert_eq!(supervisor.starts(), 0);
        assert_eq!(supervisor.poll(), Ended::None);
        assert_eq!(supervisor.stop(), Ended::None);
        assert_eq!(supervisor.process_id(), None);
    }

    /// Native-only: a real child process over its pipes. The test binary runs itself as the far end
    /// (`DNACALC_WIRE_ECHO`), which answers every frame of the link and exits when its input closes.
    #[test]
    fn a_link_crosses_a_real_process_boundary_over_pipes() {
        if std::env::var_os("DNACALC_WIRE_ECHO").is_some() {
            return;
        }
        let exe = std::env::current_exe().unwrap();
        let incarnation = Incarnation::from_parts(random_bits(), 1);
        let mut child = std::process::Command::new(exe) // ubs:ignore - The test binary re-executed as its own echo end.
            .args([
                "--exact",
                "surrogate::tests::echo_end",
                "--include-ignored",
                "--test-threads=1",
            ])
            .env("DNACALC_WIRE_ECHO", incarnation.to_text())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        // The test harness prints on the child's stdout, so the echo end writes its frames to
        // stderr.
        let stderr = child.stderr.take().unwrap();
        let mut wire = StreamWire::new(stderr, stdin);
        let mut link = ControllerLink::new(incarnation, LIMIT);
        let mut got = Vec::new();
        let mut asked = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while got.is_empty() {
            assert!(std::time::Instant::now() < deadline, "no answer");
            match wire.receive(Duration::from_millis(100)) {
                Received::Bytes(bytes) => link.receive(&bytes),
                Received::Quiet => continue,
                Received::Closed => panic!("the echo end closed"),
            }
            while let Some(next) = link.next().unwrap() {
                match next {
                    Incoming::Hello => {}
                    Incoming::Reply { payload, .. } => got.push(payload),
                    other => panic!("{other:?}"),
                }
            }
            if link.ready() && !asked {
                asked = true;
                let (_, bytes) = link.request(Kind(1), Kind(2), b"ping".to_vec()).unwrap();
                wire.send(&bytes).unwrap();
            }
        }
        assert_eq!(got, vec![b"gnip".to_vec()]);
        drop(wire);
        let status = child.wait().unwrap();
        assert!(status.success(), "{status:?}");
    }

    /// The far end of [`a_link_crosses_a_real_process_boundary_over_pipes`]; it does nothing unless
    /// that test started this binary for it.
    #[test]
    #[ignore = "run only as the child of a_link_crosses_a_real_process_boundary_over_pipes"]
    fn echo_end() {
        let Some(text) = std::env::var_os("DNACALC_WIRE_ECHO") else {
            return;
        };
        let incarnation = Incarnation::parse(&text.to_string_lossy()).unwrap();
        let mut link = SurrogateLink::new(incarnation, LIMIT);
        let mut wire = StreamWire::new(std::io::stdin(), std::io::stderr());
        wire.send(&link.hello()).unwrap();
        loop {
            match wire.receive(Duration::from_secs(30)) {
                Received::Bytes(bytes) => link.receive(&bytes),
                Received::Quiet => panic!("no request"),
                Received::Closed => return,
            }
            while let Some(next) = link.next().unwrap() {
                if let SurrogateIncoming::Request {
                    request, payload, ..
                } = next
                {
                    let mut answer = payload.clone();
                    answer.reverse();
                    wire.send(&link.reply(request, Kind(2), answer).unwrap()).unwrap();
                }
            }
        }
    }
}
