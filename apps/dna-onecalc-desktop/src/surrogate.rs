//! OneCalc's session in an add-in surrogate (docs/architecture/ADDIN_SURROGATE.md, section 11,
//! Stage 3): the page and the window stay in the controller (the process the person started); the
//! [`Host`] (the `Session` and the add-ins it loads) runs in a second process of this executable,
//! the surrogate, over the same request and reply messages the page already sends
//! ([`dna_onecalc::port`]), each in the envelope of `calc-surrogate-wire`.
//!
//! What crosses:
//! - controller to surrogate: [`kinds::CALL`] (a page [`Request`], as JSON), answered by
//!   [`kinds::REPLY`] (the [`Reply`]); [`kinds::CLOSE`] (the window is closing), answered by
//!   [`kinds::CLOSED`];
//! - surrogate to controller, while a call is open and under it as parent: [`kinds::SERVICE`] (one
//!   [`Files`] call: a dialog, a file read or write, the recent list), answered by
//!   [`kinds::SERVED`]; on its own, [`kinds::EVENT`] (`rtd`: a real-time value the formula reads
//!   arrived).
//!
//! The surrogate's own dialogs are the controller's: its [`Files`] is [`RemoteFiles`], which asks
//! the controller, and the controller's [`Remote::call`] answers with its own `Files` while it
//! waits. Everything an add-in calls synchronously (XLL callbacks, COM, VBA, RTD servers, the `$`
//! pool) stays in the surrogate. A call that runs past its limit stops being waited for (its late
//! result is refused); a surrogate that ends fails the open call with [`UNKNOWN_OUTCOME`], once;
//! restart is the caller's explicit act, and nothing is sent again.
//!
//! It names no Windows type: both ends and their tests run on every native target, over
//! `calc-platform-desktop`'s [`Wire`].
#![cfg_attr(not(windows), allow(dead_code))]

use crate::host::{CloseChoice, Files, Host, Libraries};
use calc_platform_desktop::surrogate::{Received, Wire};
use calc_surrogate_wire::{
    ControllerLink, Incarnation, Incoming, Kind, SurrogateIncoming, SurrogateLink, UNKNOWN_OUTCOME,
    WireError,
};
use dna_onecalc::port::{Reply, Request, bytes_as_base64};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// OneCalc's payload kinds.
pub mod kinds {
    use calc_surrogate_wire::Kind;
    pub const CALL: Kind = Kind(1);
    pub const REPLY: Kind = Kind(2);
    pub const EVENT: Kind = Kind(3);
    pub const SERVICE: Kind = Kind(4);
    pub const SERVED: Kind = Kind(5);
    pub const CLOSE: Kind = Kind(6);
    pub const CLOSED: Kind = Kind(7);
}

/// The largest message either end takes: a session file crosses whole, as base64 text.
pub const LIMIT: usize = 512 << 20;

/// What the page is told when the surrogate is not running (it stopped, and was not restarted).
pub const NOT_RUNNING: &str =
    "The add-in process is not running. Restart the add-in process to continue.";

/// What a call is answered with while the surrogate still runs one whose time ran out.
pub const STILL_BUSY: &str = "The add-in process is still running an earlier request. Wait for it, or stop the add-in process to continue.";

/// What a call whose time ran out is answered with. The native code is not stopped.
pub fn timed_out(limit: Duration) -> String {
    format!(
        "The add-in process did not answer within {} seconds. It is still running: wait for it, or stop the add-in process.",
        limit.as_secs()
    )
}

/// The answer to an open call when the surrogate stopped, with what ended it.
pub fn unknown_outcome() -> Reply {
    Reply::Refused {
        message: UNKNOWN_OUTCOME.to_owned(),
    }
}

/// One [`Files`] call, made by the surrogate's host and served by the controller.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Service {
    ChooseOpen,
    OpenedOrigin,
    ChooseSave {
        suggested: String,
        #[serde(with = "bytes_as_base64")]
        bytes: Vec<u8>,
    },
    ChooseAddin,
    OpenedPath,
    ReadRecent {
        path: String,
    },
    Recent,
    Remember {
        path: String,
    },
    WriteInPlace {
        path: String,
        #[serde(with = "bytes_as_base64")]
        bytes: Vec<u8>,
    },
    StoreMacroLevel {
        level: dna_onecalc::MacroSecurity,
    },
    ConfirmClose {
        name: String,
    },
    OpenInDnaCalc {
        path: Option<String>,
        name: String,
        #[serde(with = "bytes_as_base64")]
        bytes: Vec<u8>,
    },
}

/// The controller's answer to a [`Service`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Served {
    /// A file read: its name and bytes.
    File {
        name: String,
        #[serde(with = "bytes_as_base64")]
        bytes: Vec<u8>,
    },
    /// The person cancelled, or there is nothing.
    Nothing,
    Failed {
        message: String,
    },
    Origin(dna_onecalc::FileOrigin),
    /// A path: the file opened, saved or chosen.
    Path(String),
    Written(bool),
    Recent(Vec<String>),
    Close(Choice),
    Done,
}

/// [`CloseChoice`] on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Choice {
    Save,
    Discard,
    Cancel,
}

impl From<CloseChoice> for Choice {
    fn from(choice: CloseChoice) -> Self {
        match choice {
            CloseChoice::Save => Choice::Save,
            CloseChoice::Discard => Choice::Discard,
            CloseChoice::Cancel => Choice::Cancel,
        }
    }
}

impl From<Choice> for CloseChoice {
    fn from(choice: Choice) -> Self {
        match choice {
            Choice::Save => CloseChoice::Save,
            Choice::Discard => CloseChoice::Discard,
            Choice::Cancel => CloseChoice::Cancel,
        }
    }
}

fn path_text(path: &Path) -> String {
    path.display().to_string()
}

/// The controller serves one [`Service`] with its own [`Files`].
pub fn serve<F: Files + ?Sized>(files: &mut F, service: Service) -> Served {
    let failed = |message| Served::Failed { message };
    match service {
        Service::ChooseOpen => match files.choose_open() {
            Ok(Some((name, bytes))) => Served::File { name, bytes },
            Ok(None) => Served::Nothing,
            Err(message) => failed(message),
        },
        Service::OpenedOrigin => Served::Origin(files.opened_origin()),
        Service::ChooseSave { suggested, bytes } => match files.choose_save(&suggested, &bytes) {
            Ok(Some(path)) => Served::Path(path),
            Ok(None) => Served::Nothing,
            Err(message) => failed(message),
        },
        Service::ChooseAddin => match files.choose_addin() {
            Ok(Some(path)) => Served::Path(path_text(&path)),
            Ok(None) => Served::Nothing,
            Err(message) => failed(message),
        },
        Service::OpenedPath => match files.opened_path() {
            Some(path) => Served::Path(path_text(&path)),
            None => Served::Nothing,
        },
        Service::ReadRecent { path } => match files.read_recent(Path::new(&path)) {
            Ok((name, bytes)) => Served::File { name, bytes },
            Err(message) => failed(message),
        },
        Service::Recent => Served::Recent(files.recent()),
        Service::Remember { path } => {
            files.remember(&path);
            Served::Done
        }
        Service::WriteInPlace { path, bytes } => match files.write_in_place(&path, &bytes) {
            Ok(written) => Served::Written(written),
            Err(message) => failed(message),
        },
        Service::StoreMacroLevel { level } => {
            files.store_macro_level(level);
            Served::Done
        }
        Service::ConfirmClose { name } => Served::Close(files.confirm_close(&name).into()),
        Service::OpenInDnaCalc { path, name, bytes } => {
            match files.open_in_dna_calc(path.as_deref().map(Path::new), &name, &bytes) {
                Ok(()) => Served::Done,
                Err(message) => failed(message),
            }
        }
    }
}

/// The surrogate's end: its link and its wire.
pub struct SurrogateEnd {
    link: SurrogateLink,
    wire: Box<dyn Wire>,
    /// Why the connection ended, once it has.
    ended: Option<String>,
}

impl SurrogateEnd {
    pub fn new(incarnation: Incarnation, wire: Box<dyn Wire>) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(SurrogateEnd {
            link: SurrogateLink::new(incarnation, LIMIT),
            wire,
            ended: None,
        }))
    }

    fn send(&mut self, bytes: &[u8]) {
        if self.ended.is_none()
            && let Err(why) = self.wire.send(bytes)
        {
            self.ended = Some(why);
        }
    }

    /// Asks the controller to serve `service`, under the request being served, and waits for the
    /// answer (a person may take minutes over a dialog: there is no limit, but a closed connection
    /// ends the wait).
    fn ask(&mut self, service: &Service) -> Result<Served, String> {
        let payload = serde_json::to_vec(service).map_err(|e| e.to_string())?;
        let (id, bytes) = self
            .link
            .callback(kinds::SERVICE, kinds::SERVED, payload)
            .map_err(|e| e.to_string())?;
        self.send(&bytes);
        loop {
            if let Some(why) = &self.ended {
                return Err(why.clone());
            }
            loop {
                match self.link.next() {
                    Err(error) => {
                        self.ended = Some(error.to_string());
                        return Err(error.to_string());
                    }
                    Ok(None) => break,
                    Ok(Some(SurrogateIncoming::CallbackReply {
                        request, payload, ..
                    })) if request == id => {
                        return serde_json::from_slice(&payload).map_err(|e| e.to_string());
                    }
                    Ok(Some(SurrogateIncoming::Declined { request })) if request == id => {
                        return Err("the window did not serve this request".to_owned());
                    }
                    Ok(Some(other)) => {
                        eprintln!("dna-onecalc-desktop surrogate: ignored while waiting: {other:?}")
                    }
                }
            }
            match self.wire.receive(Duration::from_millis(50)) {
                Received::Bytes(bytes) => self.link.receive(&bytes),
                Received::Quiet => {}
                Received::Closed => self.ended = Some("the window closed".to_owned()),
            }
        }
    }
}

/// The surrogate host's [`Files`]: each call asks the controller.
pub struct RemoteFiles(pub Rc<RefCell<SurrogateEnd>>);

impl RemoteFiles {
    fn ask(&self, service: Service) -> Result<Served, String> {
        self.0.borrow_mut().ask(&service)
    }
}

fn unexpected(served: Served) -> String {
    format!("the window answered {served:?}")
}

impl Files for RemoteFiles {
    fn choose_open(&mut self) -> Result<Option<(String, Vec<u8>)>, String> {
        match self.ask(Service::ChooseOpen)? {
            Served::File { name, bytes } => Ok(Some((name, bytes))),
            Served::Nothing => Ok(None),
            Served::Failed { message } => Err(message),
            other => Err(unexpected(other)),
        }
    }

    fn opened_origin(&self) -> dna_onecalc::FileOrigin {
        match self.ask(Service::OpenedOrigin) {
            Ok(Served::Origin(origin)) => origin,
            // An origin not known is the least trusted one.
            _ => dna_onecalc::FileOrigin::Unknown,
        }
    }

    fn choose_save(&mut self, suggested: &str, bytes: &[u8]) -> Result<Option<String>, String> {
        match self.ask(Service::ChooseSave {
            suggested: suggested.to_owned(),
            bytes: bytes.to_vec(),
        })? {
            Served::Path(path) => Ok(Some(path)),
            Served::Nothing => Ok(None),
            Served::Failed { message } => Err(message),
            other => Err(unexpected(other)),
        }
    }

    fn choose_addin(&mut self) -> Result<Option<PathBuf>, String> {
        match self.ask(Service::ChooseAddin)? {
            Served::Path(path) => Ok(Some(PathBuf::from(path))),
            Served::Nothing => Ok(None),
            Served::Failed { message } => Err(message),
            other => Err(unexpected(other)),
        }
    }

    fn opened_path(&self) -> Option<PathBuf> {
        match self.ask(Service::OpenedPath) {
            Ok(Served::Path(path)) => Some(PathBuf::from(path)),
            _ => None,
        }
    }

    fn read_recent(&mut self, path: &Path) -> Result<(String, Vec<u8>), String> {
        match self.ask(Service::ReadRecent {
            path: path_text(path),
        })? {
            Served::File { name, bytes } => Ok((name, bytes)),
            Served::Failed { message } => Err(message),
            other => Err(unexpected(other)),
        }
    }

    fn recent(&self) -> Vec<String> {
        match self.ask(Service::Recent) {
            Ok(Served::Recent(list)) => list,
            _ => Vec::new(),
        }
    }

    fn remember(&mut self, path: &str) {
        let _ = self.ask(Service::Remember {
            path: path.to_owned(),
        });
    }

    fn write_in_place(&mut self, path: &str, bytes: &[u8]) -> Result<bool, String> {
        match self.ask(Service::WriteInPlace {
            path: path.to_owned(),
            bytes: bytes.to_vec(),
        })? {
            Served::Written(written) => Ok(written),
            Served::Failed { message } => Err(message),
            other => Err(unexpected(other)),
        }
    }

    fn store_macro_level(&mut self, level: dna_onecalc::MacroSecurity) {
        let _ = self.ask(Service::StoreMacroLevel { level });
    }

    fn confirm_close(&mut self, name: &str) -> CloseChoice {
        match self.ask(Service::ConfirmClose {
            name: name.to_owned(),
        }) {
            Ok(Served::Close(choice)) => choice.into(),
            // A window that cannot be asked keeps itself open.
            _ => CloseChoice::Cancel,
        }
    }

    fn open_in_dna_calc(
        &mut self,
        path: Option<&Path>,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), String> {
        match self.ask(Service::OpenInDnaCalc {
            path: path.map(path_text),
            name: name.to_owned(),
            bytes: bytes.to_vec(),
        })? {
            Served::Done => Ok(()),
            Served::Failed { message } => Err(message),
            other => Err(unexpected(other)),
        }
    }
}

/// The surrogate's loop: says hello, then answers each controller request with `host`, and between
/// requests runs `idle` (the message loop and the real-time poll; `true` when the page should
/// evaluate again). It returns when the controller closes the connection.
pub fn run_surrogate<L: Libraries>(
    host: &mut Host<RemoteFiles, L>,
    end: &Rc<RefCell<SurrogateEnd>>,
    mut idle: impl FnMut(&mut Host<RemoteFiles, L>) -> bool,
) -> Result<(), String> {
    {
        let mut end = end.borrow_mut();
        let hello = end.link.hello();
        end.send(&hello);
    }
    loop {
        // Every whole frame, taken before any is answered: answering asks the controller through
        // the same end.
        let mut taken = Vec::new();
        {
            let mut end = end.borrow_mut();
            if let Some(why) = end.ended.take() {
                return Err(why);
            }
            loop {
                match end.link.next() {
                    Ok(Some(next)) => taken.push(next),
                    Ok(None) => break,
                    Err(error) => return Err(error.to_string()),
                }
            }
        }
        for next in taken {
            let SurrogateIncoming::Request {
                request,
                kind,
                payload,
            } = next
            else {
                eprintln!("dna-onecalc-desktop surrogate: ignored {next:?}");
                continue;
            };
            let (kind, answer) = match kind {
                kinds::CALL => {
                    let reply = match serde_json::from_slice::<Request>(&payload) {
                        Ok(request) => host.handle(request),
                        Err(error) => Reply::Refused {
                            message: format!("the request could not be read: {error}"),
                        },
                    };
                    (kinds::REPLY, serde_json::to_vec(&reply))
                }
                kinds::CLOSE => (kinds::CLOSED, serde_json::to_vec(&host.close_requested())),
                other => {
                    return Err(format!("the window sent a request of kind {}", other.0));
                }
            };
            let answer = answer.map_err(|e| e.to_string())?;
            let mut end = end.borrow_mut();
            let bytes = end
                .link
                .reply(request, kind, answer)
                .map_err(|e| e.to_string())?;
            end.send(&bytes);
        }
        if idle(host) {
            let mut end = end.borrow_mut();
            let bytes = end.link.event(kinds::EVENT, b"rtd".to_vec());
            end.send(&bytes);
        }
        let received = end.borrow_mut().wire.receive(Duration::from_millis(20));
        match received {
            Received::Bytes(bytes) => end.borrow_mut().link.receive(&bytes),
            Received::Quiet => {}
            // The controller closed the connection: the end, unless this end had failed first.
            Received::Closed => return end.borrow_mut().ended.take().map_or(Ok(()), Err),
        }
    }
}

/// How a wait for the surrogate went.
enum Waited {
    Answer(Vec<u8>),
    /// The limit passed.
    TimedOut,
    /// The surrogate stopped (or the connection broke): it was ended, and the call's outcome is
    /// unknown.
    Stopped,
}

/// The controller's end of the connection to one surrogate incarnation.
pub struct Remote<W: Wire> {
    link: ControllerLink,
    wire: W,
    /// A call whose time ran out, still running in the surrogate.
    late: Option<u64>,
    /// Events from the surrogate the page has not been told of.
    events: Vec<String>,
    /// Frames refused (late results, another incarnation's messages), for the report.
    refused: Vec<calc_surrogate_wire::Refusal>,
    /// Why the connection ended, once it has.
    ended: Option<String>,
}

/// What a wait does between reads: runs the window's messages (so it stays responsive) and says
/// whether to keep waiting (`false`: the person asked to stop the add-in process).
pub type Pump<'a> = &'a mut dyn FnMut() -> bool;

impl<W: Wire> Remote<W> {
    /// The end over `wire` for the surrogate started as `incarnation`, once its hello arrives
    /// within `wait`.
    pub fn connect(
        incarnation: Incarnation,
        wire: W,
        wait: Duration,
        pump: Pump<'_>,
    ) -> Result<Self, String> {
        let mut remote = Remote {
            link: ControllerLink::new(incarnation, LIMIT),
            wire,
            late: None,
            events: Vec::new(),
            refused: Vec::new(),
            ended: None,
        };
        let deadline = Instant::now() + wait;
        while !remote.link.ready() {
            if Instant::now() > deadline {
                return Err(format!(
                    "the add-in process did not start within {} seconds",
                    wait.as_secs()
                ));
            }
            match remote.wire.receive(Duration::from_millis(20)) {
                Received::Bytes(bytes) => remote.link.receive(&bytes),
                Received::Quiet => {
                    if !pump() {
                        return Err("stopped before the add-in process started".to_owned());
                    }
                }
                Received::Closed => return Err("the add-in process ended as it started".into()),
            }
            remote.take(&mut |_| None).map_err(|e| e.to_string())?;
        }
        Ok(remote)
    }

    pub fn incarnation(&self) -> Incarnation {
        self.link.incarnation()
    }

    /// Whether the surrogate is up and the connection whole.
    pub fn running(&self) -> bool {
        self.ended.is_none() && self.link.ready()
    }

    /// Why the connection ended, once it has.
    pub fn ended(&self) -> Option<&str> {
        self.ended.as_deref()
    }

    /// The frames refused so far.
    pub fn refused(&self) -> &[calc_surrogate_wire::Refusal] {
        &self.refused
    }

    /// The surrogate's events since the last call: `rtd` when the page should evaluate again.
    pub fn take_events(&mut self) -> Vec<String> {
        if self.running() {
            if let Received::Bytes(bytes) = self.wire.receive(Duration::ZERO) {
                self.link.receive(&bytes);
            }
            if let Err(error) = self.take(&mut |_| None) {
                self.fail(error.to_string());
            }
        }
        std::mem::take(&mut self.events)
    }

    /// The surrogate stopped (it exited, or the supervisor ended it): every open call's outcome is
    /// unknown, and nothing more is taken from this incarnation.
    pub fn stopped(&mut self, why: impl Into<String>) -> Vec<u64> {
        self.ended.get_or_insert(why.into());
        self.late = None;
        self.link.stopped()
    }

    fn fail(&mut self, why: String) {
        self.stopped(why);
    }

    /// Everything decoded so far: replies go to `answer` (which returns `Some` for the one waited
    /// for), callbacks are served by `serve_one` through `answer`'s caller, events are kept.
    fn take(
        &mut self,
        answer: &mut dyn FnMut(Incoming) -> Option<Incoming>,
    ) -> Result<Option<Incoming>, WireError> {
        let mut found = None;
        while let Some(next) = self.link.next()? {
            match next {
                Incoming::Hello => {}
                Incoming::Event { payload, .. } => {
                    self.events.push(String::from_utf8_lossy(&payload).into_owned());
                }
                Incoming::Refused(refusal) => {
                    if let calc_surrogate_wire::Refusal::Late { request } = &refusal
                        && self.late == Some(*request)
                    {
                        self.late = None;
                    }
                    self.refused.push(refusal);
                }
                other => {
                    if let Some(kept) = answer(other) {
                        found = Some(kept);
                        break;
                    }
                }
            }
        }
        Ok(found)
    }

    /// Sends `kind` with `payload` and waits up to `limit` for its answer, serving the surrogate's
    /// [`Service`] requests under it with `files` and running `pump` between reads.
    fn exchange(
        &mut self,
        kind: Kind,
        expects: Kind,
        payload: Vec<u8>,
        files: &mut dyn Files,
        limit: Duration,
        pump: Pump<'_>,
    ) -> Result<Waited, String> {
        let (id, bytes) = self
            .link
            .request(kind, expects, payload)
            .map_err(|e| e.to_string())?;
        if let Err(why) = self.wire.send(&bytes) {
            self.fail(why);
            return Ok(Waited::Stopped);
        }
        let deadline = Instant::now() + limit;
        loop {
            let mut served = Vec::new();
            let taken = self.take(&mut |incoming| match incoming {
                Incoming::Reply { request, .. } if request == id => Some(incoming),
                Incoming::Callback { .. } => {
                    served.push(incoming);
                    None
                }
                other => {
                    eprintln!("dna-onecalc-desktop: unexpected from the add-in process: {other:?}");
                    None
                }
            });
            let taken = match taken {
                Ok(taken) => taken,
                Err(error) => {
                    self.fail(error.to_string());
                    return Ok(Waited::Stopped);
                }
            };
            for callback in served {
                let Incoming::Callback {
                    request, payload, ..
                } = callback
                else {
                    continue;
                };
                let answer = match serde_json::from_slice::<Service>(&payload) {
                    Ok(service) => serve(files, service),
                    Err(error) => Served::Failed {
                        message: format!("the request could not be read: {error}"),
                    },
                };
                let bytes = serde_json::to_vec(&answer).map_err(|e| e.to_string())?;
                let bytes = self
                    .link
                    .answer_callback(request, kinds::SERVED, bytes)
                    .map_err(|e| e.to_string())?;
                if let Err(why) = self.wire.send(&bytes) {
                    self.fail(why);
                    return Ok(Waited::Stopped);
                }
            }
            if let Some(Incoming::Reply { payload, .. }) = taken {
                return Ok(Waited::Answer(payload));
            }
            if Instant::now() > deadline {
                self.link.cancel(id);
                self.late = Some(id);
                return Ok(Waited::TimedOut);
            }
            match self.wire.receive(Duration::from_millis(10)) {
                Received::Bytes(bytes) => self.link.receive(&bytes),
                Received::Quiet => {
                    // The person stopped the add-in process: the caller ends it, and this call's
                    // outcome is unknown.
                    if !pump() {
                        self.fail("stopped by the person".to_owned());
                        return Ok(Waited::Stopped);
                    }
                }
                Received::Closed => {
                    self.fail("the add-in process ended".to_owned());
                    return Ok(Waited::Stopped);
                }
            }
        }
    }

    /// Whether a call whose time ran out still runs: what has arrived is taken first, so a late
    /// result that came in the meantime frees the surrogate.
    fn still_busy(&mut self) -> bool {
        if self.late.is_none() {
            return false;
        }
        if let Received::Bytes(bytes) = self.wire.receive(Duration::ZERO) {
            self.link.receive(&bytes);
        }
        if let Err(error) = self.take(&mut |_| None) {
            self.fail(error.to_string());
        }
        self.late.is_some()
    }

    /// The page's request, answered by the surrogate's host within `limit`.
    pub fn call(
        &mut self,
        request: &Request,
        files: &mut dyn Files,
        limit: Duration,
        pump: Pump<'_>,
    ) -> Reply {
        if !self.running() {
            return Reply::Refused {
                message: NOT_RUNNING.to_owned(),
            };
        }
        if self.still_busy() {
            return Reply::Refused {
                message: STILL_BUSY.to_owned(),
            };
        }
        let payload = match serde_json::to_vec(request) {
            Ok(payload) => payload,
            Err(error) => {
                return Reply::Refused {
                    message: format!("the request could not be encoded: {error}"),
                };
            }
        };
        match self.exchange(kinds::CALL, kinds::REPLY, payload, files, limit, pump) {
            Err(message) => Reply::Refused { message },
            Ok(Waited::Answer(payload)) => {
                serde_json::from_slice(&payload).unwrap_or_else(|error| Reply::Refused {
                    message: format!("the add-in process's answer could not be read: {error}"),
                })
            }
            Ok(Waited::TimedOut) => Reply::Refused {
                message: timed_out(limit),
            },
            Ok(Waited::Stopped) => unknown_outcome(),
        }
    }

    /// The window is closing: the surrogate's host asks about unsaved changes (through `files`).
    /// A surrogate that is not running, or does not answer, lets the window close.
    pub fn close_requested(
        &mut self,
        files: &mut dyn Files,
        limit: Duration,
        pump: Pump<'_>,
    ) -> bool {
        if !self.running() || self.still_busy() {
            return true;
        }
        match self.exchange(kinds::CLOSE, kinds::CLOSED, Vec::new(), files, limit, pump) {
            Ok(Waited::Answer(payload)) => serde_json::from_slice(&payload).unwrap_or(true),
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_platform_desktop::surrogate::{ChannelWire, random_bits, wire_pair};
    use dna_onecalc::port::LoadedAddin;
    use dna_onecalc::shown::Authored;
    use std::cell::Cell;

    /// The controller's dialogs, scripted.
    #[derive(Default)]
    struct Window {
        opens: Vec<(String, Vec<u8>)>,
        saved: Vec<(String, Vec<u8>)>,
        asked_close: Vec<String>,
        close: Option<CloseChoice>,
    }

    impl Files for Window {
        fn choose_open(&mut self) -> Result<Option<(String, Vec<u8>)>, String> {
            Ok(self.opens.pop())
        }
        fn opened_origin(&self) -> dna_onecalc::FileOrigin {
            dna_onecalc::FileOrigin::Local
        }
        fn choose_save(&mut self, suggested: &str, bytes: &[u8]) -> Result<Option<String>, String> {
            self.saved.push((suggested.to_owned(), bytes.to_vec()));
            Ok(Some(format!("C:/saved/{suggested}")))
        }
        fn choose_addin(&mut self) -> Result<Option<PathBuf>, String> {
            Ok(None)
        }
        fn confirm_close(&mut self, name: &str) -> CloseChoice {
            self.asked_close.push(name.to_owned());
            self.close.unwrap_or(CloseChoice::Discard)
        }
    }

    /// Add-ins in the surrogate, faked: `hang.xll` never returns from its load (until `release` is
    /// set), `crash.xll` ends the surrogate in the middle of its load, any other loads with one
    /// function.
    struct Addins {
        loaded: Vec<PathBuf>,
        loads: Rc<Cell<usize>>,
        release: std::sync::Arc<std::sync::atomic::AtomicBool>,
        crash: Rc<RefCell<SurrogateEndHandle>>,
    }

    /// What `crash.xll` does to the surrogate: drops its wire, as a process that died would.
    #[derive(Default)]
    struct SurrogateEndHandle(Option<Rc<RefCell<SurrogateEnd>>>);

    impl Libraries for Addins {
        fn load_xll(&mut self, path: &Path) -> Result<(), String> {
            self.loads.set(self.loads.get() + 1);
            match path.file_name().and_then(|n| n.to_str()) {
                Some("hang.xll") => {
                    while !self.release.load(std::sync::atomic::Ordering::SeqCst) {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    self.loaded.push(path.to_owned());
                    Ok(())
                }
                Some("crash.xll") => {
                    if let Some(end) = &self.crash.borrow().0 {
                        let (mut dead, _) = wire_pair();
                        dead.close();
                        end.borrow_mut().wire = Box::new(dead);
                        end.borrow_mut().ended = Some("crashed".to_owned());
                    }
                    Err("the process ended".to_owned())
                }
                _ => {
                    self.loaded.push(path.to_owned());
                    Ok(())
                }
            }
        }
        fn unload_xll(&mut self, path: &Path) -> Result<(), String> {
            self.loaded.retain(|p| p != path);
            Ok(())
        }
        fn loaded(&self) -> Vec<(PathBuf, Vec<String>)> {
            self.loaded
                .iter()
                .map(|p| (p.clone(), vec!["FAKEADD".to_owned()]))
                .collect()
        }
        fn load_xlam(
            &mut self,
            _path: &Path,
            _project: &calc_vba::OfficeProject,
            _install: bool,
        ) -> Result<Vec<String>, String> {
            Err("no VBA add-ins here".to_owned())
        }
        fn unload_xlam(&mut self, _path: &Path) -> Result<(), String> {
            Ok(())
        }
        fn loaded_xlam(&self) -> Vec<(PathBuf, Vec<String>)> {
            Vec::new()
        }
        fn load_vba(&mut self, _name: &str, _source: &str) -> Result<(), String> {
            Err("no VBA here".to_owned())
        }
        fn unload_vba(&mut self) {}
        fn callables(&self) -> Result<calc_fml::CallableTable, String> {
            Ok(calc_fml::CallableTable::default())
        }
        fn changed(&mut self) -> bool {
            false
        }
        fn cell_changed(&mut self, _row: u32, _column: u32) {}
        fn calculated(&mut self) {}
    }

    /// The surrogate on a thread of its own over one end of a wire pair, with the release switch
    /// for `hang.xll` and how many loads it ran.
    struct Started {
        remote: Remote<ChannelWire>,
        release: std::sync::Arc<std::sync::atomic::AtomicBool>,
        loads: std::sync::mpsc::Receiver<usize>,
        thread: std::thread::JoinHandle<Result<(), String>>,
    }

    fn start(start: u32) -> Started {
        let (near, far) = wire_pair();
        let incarnation = Incarnation::from_parts(random_bits(), start);
        let release = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (count, loads) = std::sync::mpsc::channel();
        let thread = {
            let release = release.clone();
            std::thread::spawn(move || {
                let end = SurrogateEnd::new(incarnation, Box::new(far));
                let loads = Rc::new(Cell::new(0));
                let crash = Rc::new(RefCell::new(SurrogateEndHandle(Some(end.clone()))));
                let session = dna_onecalc::Session::new("Sheet1").map_err(|e| format!("{e:?}"))?;
                let addins = Addins {
                    loaded: Vec::new(),
                    loads: loads.clone(),
                    release,
                    crash,
                };
                let mut host = Host::new(session, RemoteFiles(end.clone()), addins);
                let result = run_surrogate(&mut host, &end, |_| false);
                let _ = count.send(loads.get());
                result
            })
        };
        let remote = Remote::connect(incarnation, near, Duration::from_secs(30), &mut || true)
            .expect("the surrogate says hello");
        Started {
            remote,
            release,
            loads,
            thread,
        }
    }

    const LIMIT_OK: Duration = Duration::from_secs(30);

    /// A file the fake add-ins "load" (the host hashes it first), as its path text.
    fn addin(name: &str) -> String {
        let directory = std::env::temp_dir().join(format!(
            "dnacalc-onecalc-surrogate-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(name);
        std::fs::write(&path, name.as_bytes()).unwrap();
        path.display().to_string()
    }

    fn value(reply: Reply) -> String {
        match reply {
            Reply::Shown(shown) => shown.result,
            other => panic!("{other:?}"),
        }
    }

    fn sum(formula: &str) -> Authored {
        let mut authored = Authored::example_sum();
        authored.formula = formula.to_owned();
        authored
    }

    /// Native-only: threads. Requests the page sends reach the surrogate's session and come back
    /// with what the in-process host answers for the same requests.
    #[test]
    fn the_surrogate_answers_as_the_in_process_host_does() {
        let mut s = start(1);
        let mut window = Window::default();
        let mut local = Host::new(
            dna_onecalc::Session::new("Sheet1").unwrap(),
            Window::default(),
            Addins {
                loaded: Vec::new(),
                loads: Rc::new(Cell::new(0)),
                release: s.release.clone(),
                crash: Rc::new(RefCell::new(SurrogateEndHandle(None))),
            },
        );
        for request in [
            Request::Evaluate(sum("=SUM(A1:A3)")),
            Request::Evaluate(sum("=1/0")),
            Request::Evaluate(sum("=LEN(\"surrogate\")")),
            Request::LoadAddin {
                path: addin("one.xll"),
                expected_hash: None,
            },
            Request::Addins,
            Request::Save,
        ] {
            let remote = s.remote.call(&request, &mut window, LIMIT_OK, &mut || true);
            let here = local.handle(request.clone());
            match (&remote, &here) {
                // The hash is of the file, which neither host can read here.
                (Reply::Addins(a), Reply::Addins(b)) => {
                    let paths = |list: &[LoadedAddin]| {
                        list.iter().map(|a| a.path.clone()).collect::<Vec<_>>()
                    };
                    assert_eq!(paths(a), paths(b));
                }
                // The table's version counts every table this process published.
                (Reply::Shown(a), Reply::Shown(b)) => {
                    let (mut a, mut b) = (a.clone(), b.clone());
                    a.functions_version = 0;
                    b.functions_version = 0;
                    assert_eq!(a, b, "{request:?}");
                }
                _ => assert_eq!(remote, here, "{request:?}"),
            }
        }
        assert_eq!(
            value(s.remote.call(&Request::Evaluate(sum("=2+3")), &mut window, LIMIT_OK, &mut || true)),
            "number 5"
        );
        drop(s.remote);
        assert!(s.thread.join().unwrap().is_ok());
    }

    /// Native-only: threads. The surrogate's dialogs are the window's: a native open and save are
    /// served by the controller's `Files` while the call waits, under it as parent.
    #[test]
    fn dialogs_are_served_by_the_window_while_the_call_waits() {
        let mut s = start(1);
        let mut window = Window::default();
        let shown = value(s.remote.call(
            &Request::Evaluate(sum("=6*7")),
            &mut window,
            LIMIT_OK,
            &mut || true,
        ));
        assert_eq!(shown, "number 42");
        let reply = s
            .remote
            .call(&Request::SaveAsNative, &mut window, LIMIT_OK, &mut || true);
        assert!(matches!(&reply, Reply::Saved { path } if path.starts_with("C:/saved/")), "{reply:?}");
        let (name, bytes) = window.saved.pop().expect("the window saved the file");
        // The bytes the window wrote open again through the window's Open dialog.
        window.opens.push((name, bytes));
        let reply = s
            .remote
            .call(&Request::OpenNative, &mut window, LIMIT_OK, &mut || true);
        match reply {
            Reply::Opened(opened) => assert_eq!(opened.authored.formula, "=6*7"),
            other => panic!("{other:?}"),
        }
        // Closing with unsaved changes asks the person through the window, once.
        s.remote.call(
            &Request::SessionState {
                dirty: true,
                name: "Book.xlsx".to_owned(),
            },
            &mut window,
            LIMIT_OK,
            &mut || true,
        );
        window.close = Some(CloseChoice::Cancel);
        assert!(!s.remote.close_requested(&mut window, LIMIT_OK, &mut || true));
        assert_eq!(window.asked_close, vec!["Book.xlsx".to_owned()]);
        drop(s.remote);
        assert!(s.thread.join().unwrap().is_ok());
    }

    /// Native-only: threads. A call past its limit stops being waited for and the window keeps
    /// running its messages; the surrogate refuses new work until the late result arrives, which is
    /// not admitted; the add-in's load ran once.
    #[test]
    fn a_call_past_its_limit_is_not_waited_for_and_its_late_result_is_refused() {
        let mut s = start(1);
        let mut window = Window::default();
        let pumped = Cell::new(0usize);
        let mut pump = || {
            pumped.set(pumped.get() + 1);
            true
        };
        let reply = s.remote.call(
            &Request::LoadAddin {
                path: addin("hang.xll"),
                expected_hash: None,
            },
            &mut window,
            Duration::from_millis(300),
            &mut pump,
        );
        assert_eq!(
            reply,
            Reply::Refused {
                message: timed_out(Duration::from_millis(300))
            }
        );
        // The window ran its messages while it waited.
        assert!(pumped.get() > 3, "pumped {}", pumped.get());
        assert_eq!(
            s.remote
                .call(&Request::Addins, &mut window, LIMIT_OK, &mut || true),
            Reply::Refused {
                message: STILL_BUSY.to_owned()
            }
        );
        // The native code finishes: its result arrives late and is refused, and the surrogate takes
        // work again.
        s.release.store(true, std::sync::atomic::Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(10);
        let listed = loop {
            assert!(Instant::now() < deadline);
            match s
                .remote
                .call(&Request::Addins, &mut window, LIMIT_OK, &mut || true)
            {
                Reply::Refused { message } if message == STILL_BUSY => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                other => break other,
            }
        };
        assert!(matches!(&listed, Reply::Addins(list) if list.len() == 1), "{listed:?}");
        assert!(
            s.remote
                .refused()
                .iter()
                .any(|r| matches!(r, calc_surrogate_wire::Refusal::Late { .. }))
        );
        drop(s.remote);
        assert!(s.thread.join().unwrap().is_ok());
        assert_eq!(s.loads.recv().unwrap(), 1, "nothing was sent again");
    }

    /// Native-only: threads. The person stops the add-in process while a call hangs: the call is
    /// answered at once with the unknown outcome, though the native code still runs, and nothing
    /// more is taken from that process.
    #[test]
    fn stopping_the_add_in_process_answers_a_hung_call_with_the_unknown_outcome() {
        let mut s = start(1);
        let mut window = Window::default();
        let rounds = Cell::new(0);
        let reply = s.remote.call(
            &Request::LoadAddin {
                path: addin("hang.xll"),
                expected_hash: None,
            },
            &mut window,
            Duration::from_secs(600),
            &mut || {
                rounds.set(rounds.get() + 1);
                rounds.get() < 20
            },
        );
        assert_eq!(reply, unknown_outcome());
        assert!(!s.remote.running());
        // The supervisor ends the process now; the link has nothing open to report again.
        assert!(s.remote.stopped("stopped by the person").is_empty());
        assert_eq!(
            s.remote
                .call(&Request::Addins, &mut window, LIMIT_OK, &mut || true),
            Reply::Refused {
                message: NOT_RUNNING.to_owned()
            }
        );
        s.release.store(true, std::sync::atomic::Ordering::SeqCst);
        drop(s.remote);
        let _ = s.thread.join();
        assert_eq!(s.loads.recv().unwrap(), 1, "nothing was sent again");
    }

    /// Native-only: threads. A surrogate that ends in the middle of a call answers it with the
    /// unknown outcome, once; later calls are refused until a restart, and nothing is resent.
    #[test]
    fn a_surrogate_that_ends_in_a_call_reports_the_unknown_outcome_once() {
        let mut s = start(1);
        let mut window = Window::default();
        assert_eq!(
            value(s.remote.call(&Request::Evaluate(sum("=1+1")), &mut window, LIMIT_OK, &mut || true)),
            "number 2"
        );
        let reply = s.remote.call(
            &Request::LoadAddin {
                path: addin("crash.xll"),
                expected_hash: None,
            },
            &mut window,
            LIMIT_OK,
            &mut || true,
        );
        assert_eq!(reply, unknown_outcome());
        assert!(!s.remote.running());
        assert_eq!(
            s.remote
                .call(&Request::Addins, &mut window, LIMIT_OK, &mut || true),
            Reply::Refused {
                message: NOT_RUNNING.to_owned()
            }
        );
        assert!(s.thread.join().unwrap().is_err());
        assert_eq!(s.loads.recv().unwrap(), 1);
        // A restart is a new incarnation, started explicitly; it has nothing open and resends
        // nothing: its first load count is its own.
        let mut again = start(2);
        assert_ne!(again.remote.incarnation(), s.remote.incarnation());
        assert_eq!(
            again
                .remote
                .call(&Request::Addins, &mut window, LIMIT_OK, &mut || true),
            Reply::Addins(Vec::new())
        );
        drop(again.remote);
        assert!(again.thread.join().unwrap().is_ok());
        assert_eq!(again.loads.recv().unwrap(), 0);
    }
}
