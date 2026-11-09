//! The message set between OneCalc's surface and its [`Session`].
//!
//! Today the browser surface calls the session in its own page. The desktop must run the session
//! on the native thread that owns add-in calls (XLL and COM calls need the STA), with the page in
//! WebView2 beside it. Both routes are one message set: authored state goes in, a calculation
//! view and the function names come out, and nothing else crosses. [`LocalPort`] answers in
//! process (the browser's route, and the test's); a bridge port answers over the WebView2 bridge
//! (milestone 2 of the OneCalc desktop note), and both are to be tested by the same cases.
//!
//! The vocabulary is the one authored files already use: an input is a cell address and an
//! entry under a kind (`number`, `text`, `logical`, `error`, `blank`), as in `FormulaInput`. A
//! reply carries text the surface would show, never a calc-func value, so a reply is plain data
//! that serialises. Nothing here reaches a calculation engine, a workbook or a document codec.
//!
//! A request that cannot be honoured is a `Refused` reply with the reason; the session is left as
//! it was.

use crate::display;
use crate::grid;
use crate::session::{AddinRef, InputValue, ProjectReport, Session};
use crate::shown::{Authored, Shown};
use calc_fml::{CalcEnvironment, DateSystem, FormatProfile, FormulaForm};
use serde::{Deserialize, Serialize};

/// Bytes as base64 text in a message, so a file crosses the bridge as one JSON string.
pub mod bytes_as_base64 {
    use serde::{Deserialize, Deserializer, Serializer};

    const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn encode(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let n = (u32::from(chunk[0]) << 16)
                | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
                | u32::from(*chunk.get(2).unwrap_or(&0));
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(DIGITS[(n >> (18 - 6 * i) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    pub fn decode(text: &str) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(text.len() / 4 * 3);
        let (mut bits, mut have) = (0u32, 0);
        for c in text.bytes().take_while(|&c| c != b'=') {
            let v = DIGITS.iter().position(|&d| d == c)? as u32;
            bits = (bits << 6) | v;
            have += 6;
            if have >= 8 {
                have -= 8;
                out.push((bits >> have) as u8);
                bits &= (1 << have) - 1;
            }
        }
        Some(out)
    }

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        decode(&text).ok_or_else(|| serde::de::Error::custom("not base64"))
    }
}

/// What a surface asks of its session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Request {
    /// The formula text, with or without its leading `=`.
    SetFormula {
        text: String,
    },
    /// The cell the formula is entered in.
    SetFormulaCell {
        address: String,
    },
    /// Dynamic-array entry (`false`, as Excel 365) or legacy entry (`true`).
    SetForm {
        legacy: bool,
    },
    /// One input: the cell, the kind of entry and the entry text.
    SetInput {
        address: String,
        kind: String,
        entry: String,
    },
    RemoveInput {
        address: String,
    },
    ClearInputs,
    /// The locale profile (a culture name) and the clock (a serial date number) the date, time and
    /// number-format functions read. The clock comes from the owner of the page: a session has none,
    /// and without one those functions are refused.
    SetEnvironment {
        culture: String,
        now: Option<f64>,
    },
    /// Compiles a VBA standard module and publishes its functions, replacing the session's
    /// callable table; a module that does not compile changes nothing.
    LoadVba {
        name: String,
        source: String,
    },
    /// The user functions the session can call, by name.
    Functions,
    /// The registered functions with what their providers documented, and the version of the
    /// table they belong to ([`crate::shown::Shown::functions_version`]).
    FunctionDocs,
    /// Calculates the formula over the inputs.
    Calculate,
    /// The whole authored state in, what to show out: the surface's one request on each edit.
    Evaluate(Authored),
    /// Unloads the VBA module, if one is loaded.
    UnloadVba,
    /// The value of the part of the authored formula at `start`..`start + len` (UTF-16 code
    /// units): F9 on a selection.
    EvaluatePart {
        authored: Authored,
        start: usize,
        len: usize,
    },
    /// The authored formula's evaluation, part by part.
    Trace {
        authored: Authored,
    },
    /// A new, blank session; add-ins and their functions stay as they are.
    New,
    /// Opens the workbook `bytes` hold as the session (`name` is its file name).
    Open {
        name: String,
        #[serde(with = "bytes_as_base64")]
        bytes: Vec<u8>,
        /// The URL security zone of the file's Mark of the Web (its `Zone.Identifier` stream)
        /// when the host could read one: 3 (internet) and 4 (restricted) never run a VBA project.
        #[serde(default)]
        zone: Option<u32>,
    },
    /// Lets the open file's VBA project run (the person enabled macros for this file); answers
    /// with the file as opened, now with its project compiled and `Workbook_Open` raised.
    EnableMacros,
    /// Opens a workbook through the native host's file dialog (a host with no dialog refuses).
    OpenNative,
    /// The session saved as a workbook, the bytes in the reply.
    Save,
    /// The session saved through the native host: to the file it was opened from or last saved to,
    /// or through the file dialog when it has none (a host with no dialog refuses).
    SaveNative,
    /// The session saved through the native host's file dialog, whatever file it came from.
    SaveAsNative,
    /// The files the person opened or saved most recently, newest first (the native host keeps
    /// them; any other answers none).
    Recent,
    /// Opens a file from [`Request::Recent`] by its path.
    OpenRecent {
        path: String,
    },
    /// What the surface holds, so the window can say it and the host can ask before closing: the
    /// session's name and whether it has changes the file does not.
    SessionState {
        dirty: bool,
        name: String,
    },
    /// Opens in DNA Calc the file last refused with [`Reply::NotASession`].
    OpenInDnaCalc,
    /// Loads an add-in the person picks in the native host's file dialog (a host that cannot load
    /// add-ins refuses). The answer is [`Reply::Addins`].
    LoadAddinNative,
    /// Loads the add-in at `path`: one the session listed, or a path the person typed. When
    /// `expected_hash` is given (the SHA-256 the session listed) and the file no longer holds it,
    /// the host refuses and loads nothing.
    LoadAddin {
        path: String,
        expected_hash: Option<String>,
    },
    /// Unloads the add-in loaded from `path`, running its close routine and removing its functions.
    UnloadAddin {
        path: String,
    },
    /// The add-ins the host has loaded.
    Addins,
}

/// An add-in the host has loaded, as the surface lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadedAddin {
    /// What it is (`xll`).
    pub kind: String,
    /// The path it was loaded from.
    pub path: String,
    /// The SHA-256 of the file when it was loaded, as the session file lists it.
    pub hash: String,
    /// The worksheet functions it registered, in registration order.
    pub functions: Vec<String>,
    /// What its load reported: workbook events that failed or are not delivered.
    #[serde(default)]
    pub notes: Vec<String>,
}

/// The registered functions a surface completes and documents, as of one version of the table.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FunctionDocs {
    pub version: u64,
    /// Every registered name, in table order.
    pub names: Vec<String>,
    /// What the providers documented of them (argument names, help, category).
    pub docs: Vec<calc_fml::CallableDescriptor>,
}

/// A session a file opened, as the surface shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Opened {
    /// The file's name, as chosen.
    pub name: String,
    pub authored: Authored,
    pub shown: Shown,
    /// The add-ins the file lists, never loaded by opening it.
    pub addins: Vec<AddinRef>,
    /// The paths of the add-ins the host has loaded, when it has any.
    pub loaded: Vec<String>,
    /// The file's VBA project, when it carries one (an .xlsm session): what compiling it gave,
    /// or why it did not compile.
    #[serde(default)]
    pub project: Option<ProjectView>,
}

/// A session's VBA project as the surface lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectView {
    /// The project's worksheet functions; empty when it did not compile.
    pub functions: Vec<String>,
    /// Workbook events not delivered or that failed, and why the project did not compile or run.
    pub notes: Vec<String>,
    /// Whether the project may run.
    #[serde(default)]
    pub macros: MacrosView,
}

/// Whether the file's VBA project may run (Excel's default: not until the person enables it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MacrosView {
    /// The project was let run.
    #[default]
    Enabled,
    /// Held but not run; the person can enable macros for this file.
    Disabled,
    /// Held but not run, and not to be enabled: the file carries the Mark of the Web.
    Blocked,
}

impl From<crate::MacroState> for MacrosView {
    fn from(state: crate::MacroState) -> Self {
        match state {
            crate::MacroState::Enabled => Self::Enabled,
            crate::MacroState::Disabled => Self::Disabled,
            crate::MacroState::Blocked(_) => Self::Blocked,
        }
    }
}

/// What a calculation gave, as text for people.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalculationView {
    /// The value as a cell shows it; `None` when diagnostics stopped the calculation.
    pub value: Option<String>,
    /// Why there is no value, or what a user function reported.
    pub diagnostics: Vec<String>,
    /// The areas read, in order, with what each returned.
    pub reads: Vec<String>,
    /// Where the value came from; `None` when nothing was calculated.
    pub provenance: Option<String>,
}

/// What a session answers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Reply {
    /// The request changed the session and has nothing to report.
    Accepted,
    /// The cell a request named, as the session spells it (`Sheet1!B3`).
    Cell {
        address: String,
    },
    Calculated(CalculationView),
    /// The names of the functions a VBA module published, or all user functions.
    Functions {
        names: Vec<String>,
    },
    /// The registered functions of one version of the table ([`Request::FunctionDocs`]).
    FunctionDocs(FunctionDocs),
    Refused {
        message: String,
    },
    /// What the surface shows for an authored state ([`Request::Evaluate`]).
    Shown(Box<Shown>),
    /// The value of a part of the formula ([`Request::EvaluatePart`]).
    Part(crate::trace::PartValue),
    /// The parts of the formula and their values ([`Request::Trace`]).
    Trace(crate::trace::Trace),
    /// A file or a new session replaced the session.
    Opened(Box<Opened>),
    /// A file outside what a session holds: why, and the offer to open it in DNA Calc.
    NotASession {
        reason: String,
        offer: String,
    },
    /// The session as a workbook ([`Request::Save`]).
    File {
        name: String,
        #[serde(with = "bytes_as_base64")]
        bytes: Vec<u8>,
    },
    /// The session was written to this path by the native host.
    Saved {
        path: String,
    },
    /// The person closed the native file dialog; nothing changed.
    Cancelled,
    /// The add-ins the host has loaded (answers every add-in request that succeeds).
    Addins(Vec<LoadedAddin>),
    /// The recent files, newest first ([`Request::Recent`]).
    Recent(Vec<String>),
}

/// A session behind a message boundary. `&mut self` and no async: an answer is the thread's own
/// turn; what arrives later (RTD, native async returns) is pushed by the owner, not asked for.
pub trait SessionPort {
    fn send(&mut self, request: Request) -> Reply;
}

/// The session in the same process as its surface.
#[derive(Debug)]
pub struct LocalPort {
    session: Session,
}

impl LocalPort {
    pub fn new(session: Session) -> Self {
        Self { session }
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    /// For the owner that publishes add-in functions (`Session::set_user_functions`) between
    /// messages; a surface only sends requests.
    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    pub fn into_session(self) -> Session {
        self.session
    }
}

/// The typed value an entry gives under its kind, or why it gives none.
pub(crate) fn entry_value(
    session: &Session,
    kind: &str,
    entry: &str,
) -> Result<InputValue, String> {
    let text = entry.trim();
    let refused = |error| display::input_error(session, &error);
    match kind {
        "number" => match text.parse::<f64>() {
            Ok(number) => InputValue::number(number).map_err(refused),
            Err(_) => Err(format!("{entry:?} is not a number")),
        },
        "text" => InputValue::text(entry).map_err(refused),
        "logical" if text.eq_ignore_ascii_case("TRUE") => Ok(InputValue::logical(true)),
        "logical" if text.eq_ignore_ascii_case("FALSE") => Ok(InputValue::logical(false)),
        "logical" => Err(format!("{entry:?} is neither TRUE nor FALSE")),
        "error" => grid::error_literal(text)
            .map(InputValue::error)
            .ok_or_else(|| format!("{entry:?} is not a worksheet error such as #N/A")),
        "blank" => Ok(InputValue::blank()),
        other => Err(format!("{other:?} is not a kind of input")),
    }
}

/// The calculation environment a surface gives its session: the 1900 date system, the profile of
/// `culture`, and the owner's clock.
pub(crate) fn environment(culture: &str, now: Option<f64>, date_1904: bool) -> CalcEnvironment {
    CalcEnvironment {
        compatibility_version: 1,
        date_system: if date_1904 {
            DateSystem::System1904
        } else {
            DateSystem::System1900
        },
        profile: FormatProfile::for_culture(culture),
        now,
    }
}

/// A session that replaces `old` keeps its callable functions (the add-ins the host loaded) unless
/// they are a VBA module's, which a new session does not carry: the module is unloaded with the
/// session it was loaded into.
fn carry_functions(old: &mut Session, new: &mut Session) {
    if !old.has_vba_module() {
        new.set_user_functions(old.published_functions().clone());
    }
    // The owner of asynchronous returns belongs to the add-ins, not to the session it served.
    new.set_external_returns(old.take_external_returns());
    // So does the capability to join RTD topics. (The old formula's connections are the owner's to
    // disconnect: the new session has an empty ledger and reads nothing yet.)
    if let Some(connector) = old.rtd_connector_for_replacement() {
        new.set_rtd_connector(connector);
    }
}

/// The reply for a session that has just been opened or made: what to show for it.
pub fn opened(session: &mut Session, name: String, loaded: Vec<String>) -> Reply {
    opened_with(session, name, loaded, None)
}

/// A session's project as the surface lists it, with what `report` gave.
pub fn project_view(session: &Session, report: ProjectReport) -> ProjectView {
    ProjectView {
        functions: report.functions,
        notes: report.notes,
        macros: session
            .project()
            .map_or(MacrosView::Enabled, |p| p.macros.into()),
    }
}

/// [`opened`] for a file whose VBA project compiled (or did not).
pub fn opened_with(
    session: &mut Session,
    name: String,
    loaded: Vec<String>,
    project: Option<ProjectView>,
) -> Reply {
    let authored = crate::shown::authored_of(session);
    let shown = crate::shown::evaluate(session, &authored);
    Reply::Opened(Box::new(Opened {
        name,
        authored,
        shown,
        addins: session.addins().to_vec(),
        loaded,
        project,
    }))
}

/// A way for a surface to ask its session, one reply for each request in the order asked. The
/// surface never holds the session: an in-process link answers at once ([`LocalLink`]), a bridge
/// to a native session answers later (the answer arrives through `done`, after the call returns).
pub trait Link {
    fn call(&self, request: Request, done: Box<dyn FnOnce(Reply)>);

    /// Whether the other side shows native file dialogs ([`Request::OpenNative`],
    /// [`Request::SaveNative`]) rather than the page choosing and downloading files.
    fn native_files(&self) -> bool {
        false
    }

    /// Hands the surface's handler the messages the other side sends on its own (see
    /// [`crate::bridge::Event`]); a link whose other side never does ignores it.
    fn on_event(&self, _handler: std::rc::Rc<dyn Fn(String)>) {}
}

/// A [`LocalPort`] in the page: the browser build's link.
#[derive(Debug)]
pub struct LocalLink(std::cell::RefCell<LocalPort>);

impl LocalLink {
    pub fn new(port: LocalPort) -> Self {
        LocalLink(std::cell::RefCell::new(port))
    }
}

impl Link for LocalLink {
    fn call(&self, request: Request, done: Box<dyn FnOnce(Reply)>) {
        let reply = self.0.borrow_mut().send(request);
        done(reply);
    }
}

pub(crate) fn refused(message: impl Into<String>) -> Reply {
    Reply::Refused {
        message: message.into(),
    }
}

impl SessionPort for LocalPort {
    fn send(&mut self, request: Request) -> Reply {
        let session = &mut self.session;
        match request {
            Request::SetFormula { text } => {
                session.set_formula(&text);
                Reply::Accepted
            }
            Request::SetFormulaCell { address } => match session.set_formula_cell(&address) {
                Ok(cell) => Reply::Cell {
                    address: display::cell(session, cell),
                },
                Err(error) => refused(display::input_error(session, &error)),
            },
            Request::SetForm { legacy } => {
                session.set_form(if legacy {
                    FormulaForm::Legacy
                } else {
                    FormulaForm::Dynamic
                });
                Reply::Accepted
            }
            Request::SetInput {
                address,
                kind,
                entry,
            } => {
                let value = match entry_value(session, &kind, &entry) {
                    Ok(value) => value,
                    Err(message) => return refused(message),
                };
                match session.set_input(&address, value) {
                    Ok(cell) => Reply::Cell {
                        address: display::cell(session, cell),
                    },
                    Err(error) => refused(display::input_error(session, &error)),
                }
            }
            Request::RemoveInput { address } => match session.remove_input(&address) {
                Ok(_) => Reply::Accepted,
                Err(error) => refused(display::input_error(session, &error)),
            },
            Request::ClearInputs => {
                session.clear_inputs();
                Reply::Accepted
            }
            Request::SetEnvironment { culture, now } => {
                session.set_environment(Some(environment(
                    &culture,
                    now,
                    session.options().date_1904,
                )));
                Reply::Accepted
            }
            // With a VBA project (an .xlsm session) the module is the project's: its code changes
            // and the project part saved next is the changed one.
            Request::LoadVba { name, source }
                if session.project().is_some_and(|p| !p.modules().is_empty()) =>
            {
                match session.edit_project_module(&name, &source) {
                    Ok(report) => Reply::Functions {
                        names: report.functions,
                    },
                    Err(message) => refused(message),
                }
            }
            Request::LoadVba { name, source } => {
                match session.load_vba_module(&name, &source, calc_vba::DEFAULT_STEP_BUDGET) {
                    Ok(names) => Reply::Functions { names },
                    Err(message) => refused(message),
                }
            }
            Request::FunctionDocs => Reply::FunctionDocs(FunctionDocs {
                version: session.functions_version(),
                names: session.user_function_names(),
                docs: session.user_function_descriptors(),
            }),
            Request::Functions => Reply::Functions {
                names: session.user_function_names(),
            },
            Request::UnloadVba => {
                session.clear_user_functions();
                Reply::Accepted
            }
            Request::Evaluate(authored) => {
                Reply::Shown(Box::new(crate::shown::evaluate(session, &authored)))
            }
            Request::EvaluatePart {
                authored,
                start,
                len,
            } => match crate::trace::evaluate_part(session, &authored, start, len) {
                Ok(part) => Reply::Part(part),
                Err(message) => refused(message),
            },
            Request::Trace { authored } => Reply::Trace(crate::trace::trace(session, &authored)),
            Request::New => {
                let mut fresh = Session::new("Sheet1").expect("Sheet1 is a sheet name");
                carry_functions(session, &mut fresh);
                fresh.set_addins(session.addins().to_vec());
                *session = fresh;
                opened(session, String::new(), Vec::new())
            }
            Request::Open { name, bytes, zone } => match crate::file::open(&bytes) {
                Ok((mut next, vba)) => {
                    carry_functions(session, &mut next);
                    *session = next;
                    // Excel's default for a macro file: it opens with macros disabled (nothing
                    // compiles, nothing runs); with the Mark of the Web it can not be enabled.
                    let project = vba.map(|vba| match session.open_project(vba, &name, zone) {
                        Ok(report) => project_view(session, report),
                        Err(why) => project_view(
                            session,
                            ProjectReport {
                                functions: Vec::new(),
                                notes: vec![why],
                            },
                        ),
                    });
                    opened_with(session, name, Vec::new(), project)
                }
                Err(refusal) => Reply::NotASession {
                    reason: refusal.reason,
                    offer: crate::file::OFFER.to_owned(),
                },
            },
            // A session with a VBA project is saved as an .xlsm under the name of the file it came
            // from, which its ThisWorkbook.Name gives; any other as onecalc-session.xlsx.
            Request::Save => match session.raise_before_save() {
                Ok(true) => refused("Workbook_BeforeSave set Cancel: the session was not saved"),
                Err(message) => refused(message),
                Ok(false) => match crate::file::save(session) {
                    Ok(bytes) => Reply::File {
                        name: session
                            .project()
                            .map(|project| {
                                let stem = project
                                    .file_name
                                    .rsplit_once('.')
                                    .map_or(project.file_name.as_str(), |(stem, _)| stem);
                                format!(
                                    "{}.xlsm",
                                    if stem.is_empty() {
                                        "onecalc-session"
                                    } else {
                                        stem
                                    }
                                )
                            })
                            .unwrap_or_else(|| "onecalc-session.xlsx".to_owned()),
                        bytes,
                    },
                    Err(message) => refused(message),
                },
            },
            Request::EnableMacros => match session.enable_macros() {
                Ok(report) => {
                    let name = session
                        .project()
                        .map(|p| p.file_name.clone())
                        .unwrap_or_default();
                    let project = Some(project_view(session, report));
                    opened_with(session, name, Vec::new(), project)
                }
                Err(message) => refused(message),
            },
            Request::OpenNative
            | Request::SaveNative
            | Request::SaveAsNative
            | Request::OpenRecent { .. } => refused("this session has no native file dialog"),
            Request::Recent => Reply::Recent(Vec::new()),
            Request::SessionState { .. } => Reply::Accepted,
            Request::OpenInDnaCalc => refused("Open in DNA Calc needs the desktop app"),
            Request::LoadAddinNative | Request::LoadAddin { .. } | Request::UnloadAddin { .. } => {
                refused("add-ins load only in the desktop app")
            }
            Request::Addins => Reply::Addins(Vec::new()),
            Request::Calculate => {
                let calculation = session.calculate();
                Reply::Calculated(CalculationView {
                    value: calculation.result.as_ref().map(display::value),
                    diagnostics: calculation
                        .diagnostics
                        .iter()
                        .map(|d| display::diagnostic(session, d))
                        .collect(),
                    reads: calculation
                        .reads
                        .iter()
                        .map(|r| display::read(session, r))
                        .collect(),
                    provenance: crate::result::provenance(session, &calculation),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port() -> LocalPort {
        LocalPort::new(Session::new("Sheet1").unwrap())
    }

    fn calculated(reply: Reply) -> CalculationView {
        match reply {
            Reply::Calculated(view) => view,
            other => panic!("not a calculation: {other:?}"),
        }
    }

    /// The same session, driven directly, is what the port's view reports.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_calculation_through_the_port_is_the_sessions_own() {
        let mut port = port();
        assert_eq!(
            port.send(Request::SetFormulaCell {
                address: "C3".into()
            }),
            Reply::Cell {
                address: "Sheet1!C3".into()
            }
        );
        port.send(Request::SetFormula {
            text: "=A1*2+B1".into(),
        });
        for (address, entry) in [("A1", "20"), ("B1", "2")] {
            assert_eq!(
                port.send(Request::SetInput {
                    address: address.into(),
                    kind: "number".into(),
                    entry: entry.into(),
                }),
                Reply::Cell {
                    address: format!("Sheet1!{address}")
                }
            );
        }
        let view = calculated(port.send(Request::Calculate));

        let mut direct = Session::new("Sheet1").unwrap();
        direct.set_formula_cell("C3").unwrap();
        direct.set_formula("=A1*2+B1");
        direct
            .set_input("A1", InputValue::number(20.).unwrap())
            .unwrap();
        direct
            .set_input("B1", InputValue::number(2.).unwrap())
            .unwrap();
        let expected = direct.calculate();
        assert_eq!(view.value, expected.result.as_ref().map(display::value));
        assert!(view.value.as_deref().is_some_and(|v| v.contains("42")));
        assert!(view.diagnostics.is_empty());
        assert_eq!(view.reads.len(), expected.reads.len());
        assert_eq!(
            view.provenance,
            crate::result::provenance(&direct, &expected)
        );
    }

    /// A refused request says why and leaves the session as it was; a formula that does not parse
    /// is a calculation with diagnostics, not a refusal.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn refusals_name_the_reason_and_change_nothing() {
        let mut port = port();
        for (request, reason) in [
            (
                Request::SetInput {
                    address: "A1".into(),
                    kind: "number".into(),
                    entry: "x".into(),
                },
                "is not a number",
            ),
            (
                Request::SetInput {
                    address: "A1".into(),
                    kind: "colour".into(),
                    entry: "red".into(),
                },
                "is not a kind of input",
            ),
            (
                Request::SetInput {
                    address: "A1:A2".into(),
                    kind: "blank".into(),
                    entry: String::new(),
                },
                "A1:A2",
            ),
            (
                Request::SetFormulaCell {
                    address: "Nowhere!A1".into(),
                },
                "Nowhere",
            ),
        ] {
            match port.send(request) {
                Reply::Refused { message } => assert!(message.contains(reason), "{message}"),
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(port.session().inputs().count(), 0);
        port.send(Request::SetFormula { text: "=1+".into() });
        let view = calculated(port.send(Request::Calculate));
        assert!(view.value.is_none() && !view.diagnostics.is_empty());
    }

    /// VBA functions arrive as names; the set of user functions is asked for, never reached into;
    /// a module that does not compile is a refusal that keeps the earlier functions.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn vba_modules_publish_names_and_a_broken_module_changes_nothing() {
        let mut port = port();
        assert_eq!(
            port.send(Request::Functions),
            Reply::Functions { names: vec![] }
        );
        let loaded = port.send(Request::LoadVba {
            name: "Math".into(),
            source: calc_vba::EXAMPLE_MODULE.into(),
        });
        assert_eq!(
            loaded,
            Reply::Functions {
                names: vec!["ADD2".into(), "MATH.ADD2".into()]
            }
        );
        port.send(Request::SetFormula {
            text: "=Math.Add2(40,2)".into(),
        });
        let view = calculated(port.send(Request::Calculate));
        assert!(
            view.value.as_deref().is_some_and(|v| v.contains("42")),
            "{view:?}"
        );
        assert!(matches!(
            port.send(Request::LoadVba {
                name: "Broken".into(),
                source: "Function (".into()
            }),
            Reply::Refused { .. }
        ));
        assert_eq!(port.send(Request::Functions), loaded);
    }

    /// Requests and replies are plain data: they survive the bridge as JSON.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn messages_round_trip_as_json() {
        let requests = vec![
            Request::SetFormula { text: "=A1".into() },
            Request::SetInput {
                address: "A1".into(),
                kind: "error".into(),
                entry: "#N/A".into(),
            },
            Request::SetEnvironment {
                culture: "de-DE".into(),
                now: Some(45000.5),
            },
            Request::Calculate,
        ];
        for request in requests {
            let text = serde_json::to_string(&request).unwrap();
            assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
        }
        let mut port = port();
        port.send(Request::SetFormula {
            text: "=1/0".into(),
        });
        let reply = port.send(Request::Calculate);
        let text = serde_json::to_string(&reply).unwrap();
        assert_eq!(serde_json::from_str::<Reply>(&text).unwrap(), reply);
    }
}
