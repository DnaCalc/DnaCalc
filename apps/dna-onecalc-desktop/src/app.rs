//! App mode (`dna-onecalc-desktop --app`): the page the browsers run, hosted in WebView2, with the
//! session native behind the page.
//!
//! The shell is skin deep. The page is OneCalc's surface over a bridge link
//! ([`dna_onecalc::bridge`]); this module carries each request string the page posts to
//! [`Host::answer`] on the STA thread (the one that owns add-in calls) and posts the answer back.
//! The window chain, the WebView2 host and its string bridge are `calc-platform-desktop`'s, the
//! add-in set `calc-addin-host`'s, and the file dialogs `calc-platform-desktop`'s: nothing here is
//! OneCalc-only beyond wiring them to the host.
use crate::host::{CloseChoice, Files, Host, Libraries, RtdLinks, RtdUpdate};
use crate::settings::Settings;
use crate::surrogate::{Remote, RemoteFiles, SurrogateEnd, run_surrogate};
use calc_platform_desktop::surrogate::{StreamWire, Supervisor};
use calc_addin_host::{AddinHost, Document, RtdClient, RtdProvider};
use calc_fml::CallableTable;
use calc_platform_desktop::dialogs::{self, Buttons, FileFilter, Pressed};
use calc_platform_desktop::{
    Shell, ThreadMessage, ThreadTimer, WebView, WebViewOptions, per_monitor_dpi_aware,
    pump_pending, run_message_loop,
};
use calc_value::CalcValue;
use dna_onecalc::bridge::{Answer, Call};
use dna_onecalc::port::{Reply, Request};
use dna_onecalc::{ExternalReturns, Session};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};

/// What the command line asks of app mode.
struct Options {
    /// Navigate here (the harness's bundle server), instead of mapping `assets`.
    url: Option<String>,
    /// The built page (`cargo xtask app bundle onecalc`), mapped as `https://dnacalc.app/`. A
    /// packaged executable carries its page and needs neither this nor `url`.
    assets: Option<PathBuf>,
    xlls: Vec<PathBuf>,
    /// The first sheet's name, which add-ins see (the comparisons with Excel name it as the
    /// Excel workbook they ran in did).
    sheet: String,
    debug_port: Option<u16>,
    /// The first client size; without one the window comes back where it was, else 1280 by 900.
    size: Option<(i32, i32)>,
    /// The folder for the app's data (settings, the WebView2 profile); without one,
    /// `%LOCALAPPDATA%\DNA OneCalc`.
    data_dir: Option<PathBuf>,
    /// Answers the question asked when a window with unsaved changes closes, instead of asking
    /// (an acceptance run closes the window without a person): `save`, `discard` or `cancel`.
    close_answer: Option<CloseChoice>,
    /// Writes the app's own messages (a missing WebView2, a file DNA Calc cannot be started for)
    /// to this file instead of showing them in a box.
    message_file: Option<PathBuf>,
    /// Save sessions here, each in a new numbered folder under the name the session suggests,
    /// instead of asking in the save dialog: an acceptance run drives the page's Save without a
    /// person (the dialog's own code is not exercised then).
    save_dir: Option<PathBuf>,
    /// `--open-file PATH` (repeatable): the files the native Open dialog answers with, in order,
    /// instead of showing it (an acceptance run drives the page's Open button without a person).
    /// The file is read by the same route as a dialog's pick (`origin::open_file`), so the page
    /// receives its real path and Mark of the Web, never an Unknown origin.
    open_files: Vec<PathBuf>,
    /// `--trust-macros`: pins the macro security level to Unguarded (dc-15dk).
    trust_macros: bool,
    /// `--macros off|guarded|unguarded`: pins the macro security level for this run (`safe` is
    /// accepted as an alias for `guarded`).
    macros: Option<dna_onecalc::MacroSecurity>,
    /// `--transport surrogate` (or `DNACALC_ONECALC_TRANSPORT=surrogate`): the session and its
    /// add-ins run in a second process of this executable (docs/architecture/ADDIN_SURROGATE.md,
    /// Stage 3); the default `in-process` runs them in this one (the conformance mode).
    surrogate: bool,
    /// `--call-limit SECONDS`: how long the window waits for the add-in process to answer one
    /// request before it stops waiting (the call goes on running there).
    call_limit: Duration,
}

/// How long the window waits for the add-in process's answer by default: longer than the slowest
/// add-in load the acceptance runs wait for (four minutes).
const CALL_LIMIT: Duration = Duration::from_secs(300);

/// How long the add-in process may take to start.
const START_LIMIT: Duration = Duration::from_secs(60);

/// How long a closing window waits for the add-in process to close its add-ins and exit before it
/// is ended.
const FINISH_LIMIT: Duration = Duration::from_secs(20);

fn options(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        url: None,
        assets: None,
        xlls: Vec::new(),
        sheet: "Sheet1".to_owned(),
        debug_port: None,
        size: None,
        data_dir: None,
        close_answer: None,
        message_file: None,
        save_dir: None,
        open_files: Vec::new(),
        trust_macros: false,
        macros: None,
        surrogate: std::env::var("DNACALC_ONECALC_TRANSPORT").is_ok_and(|t| t == "surrogate"),
        call_limit: CALL_LIMIT,
    };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = || args.next().cloned().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--app" => {}
            "--url" => o.url = Some(value()?),
            "--assets" => o.assets = Some(PathBuf::from(value()?)),
            "--xll" => o.xlls.push(PathBuf::from(value()?)),
            "--sheet" => o.sheet = value()?,
            "--save-dir" => o.save_dir = Some(PathBuf::from(value()?)),
            "--data-dir" => o.data_dir = Some(PathBuf::from(value()?)),
            "--open-file" => o.open_files.push(PathBuf::from(value()?)),
            "--trust-macros" => o.trust_macros = true,
            "--macros" => {
                let text = value()?;
                o.macros = Some(
                    dna_onecalc::MacroSecurity::parse(&text)
                        .ok_or(format!("--macros is off, guarded or unguarded, not {text}"))?,
                )
            }
            "--message-file" => o.message_file = Some(PathBuf::from(value()?)),
            "--transport" => {
                o.surrogate = match value()?.as_str() {
                    "surrogate" => true,
                    "in-process" => false,
                    other => {
                        return Err(format!(
                            "--transport is in-process or surrogate, not {other}"
                        ));
                    }
                }
            }
            "--call-limit" => {
                o.call_limit = Duration::from_secs(
                    value()?
                        .parse()
                        .map_err(|_| "--call-limit needs a number of seconds")?,
                )
            }
            "--close-answer" => {
                o.close_answer = Some(match value()?.as_str() {
                    "save" => CloseChoice::Save,
                    "discard" => CloseChoice::Discard,
                    "cancel" => CloseChoice::Cancel,
                    other => {
                        return Err(format!(
                            "--close-answer is save, discard or cancel, not {other}"
                        ));
                    }
                })
            }
            "--debug-port" => {
                o.debug_port = Some(value()?.parse().map_err(|_| "--debug-port needs a port")?)
            }
            "--size" => {
                let text = value()?;
                let (w, h) = text.split_once('x').ok_or("--size needs WIDTHxHEIGHT")?;
                o.size = Some((
                    w.parse().map_err(|_| "--size width")?,
                    h.parse().map_err(|_| "--size height")?,
                ));
            }
            other => return Err(format!("unknown app argument: {other}")),
        }
    }
    if o.url.is_none() && o.assets.is_none() && !crate::embedded::has_page() {
        return Err(
            "app mode needs --assets DIR (the built page) or --url URL: this build carries no page"
                .into(),
        );
    }
    Ok(o)
}

impl Options {
    /// Whether a test flag is present: such a run reports on stderr and by its exit code, and
    /// shows no box that waits for a click.
    fn scripted(&self) -> bool {
        self.debug_port.is_some()
            || self.close_answer.is_some()
            || self.data_dir.is_some()
            || self.save_dir.is_some()
            || !self.open_files.is_empty()
            || self.message_file.is_some()
    }
}

/// The workbook name the add-ins see.
const WORKBOOK: &str = "Book1";

/// The timer that follows the window's size and position (milliseconds).
const TICK_MS: u32 = 50;
const TICK_ID: usize = 0x4f4e43;

/// The native file dialogs, owned by the window.
struct Dialogs {
    owner: *mut std::ffi::c_void,
    /// `--save-dir`: where saves go without the dialog, and how many there have been.
    save_dir: Option<(PathBuf, usize)>,
    /// `--open-file`: the answers the Open dialog gives, next first.
    open_files: std::collections::VecDeque<PathBuf>,
    /// The Mark of the Web zone of the file last chosen to open.
    origin: dna_onecalc::FileOrigin,
    /// The path of the file last chosen to open.
    path: Option<PathBuf>,
    /// The remembered window place and recent files.
    settings: Rc<RefCell<Settings>>,
    /// `--close-answer`: what a close with unsaved changes does without asking.
    close_answer: Option<CloseChoice>,
    /// `--message-file`: where the app's messages go instead of a box.
    message_file: Option<PathBuf>,
    /// DNA Calc's executable when it is in the folder with this one.
    dna_calc: Option<PathBuf>,
    /// A test flag is present: nothing here waits for a click.
    scripted: bool,
    /// The per-user folder the macro security level is stored in (dc-15dk): `--data-dir`, else the
    /// folder both desktop applications share.
    macro_folder: PathBuf,
}

/// What the app says to the person: in a box, or in the file `--message-file` names.
fn say(owner: *mut std::ffi::c_void, message_file: Option<&Path>, scripted: bool, text: &str) {
    match message_file {
        Some(file) => {
            let _ = std::fs::write(file, text);
        }
        // A scripted run never waits for a click.
        None if scripted => eprintln!("dna-onecalc-desktop: {text}"),
        None => {
            dialogs::message(owner.cast(), "DNA Calc - Formula Explorer", text, Buttons::Ok, true);
        }
    }
}

/// Where DNA Calc's executable would be: beside this one.
fn dna_calc_beside_us() -> Option<PathBuf> {
    let folder = std::env::current_exe().ok()?.parent()?.to_owned();
    let exe = folder.join("dna-calc-desktop.exe");
    exe.is_file().then_some(exe)
}

fn session_files(open: bool) -> [FileFilter<'static>; 2] {
    let (name, patterns) = if open {
        ("Formula Explorer sessions (*.xlsx; *.xlsm; *.xls; *.xla; *.xlt)", "*.xlsx;*.xlsm;*.xls;*.xla;*.xlt")
    } else {
        ("Formula Explorer sessions (*.xlsx; *.xlsm)", "*.xlsx;*.xlsm")
    };
    [FileFilter { name, patterns }, FileFilter { name: "All files", patterns: "*.*" }]
}

impl Files for Dialogs {
    fn choose_open(&mut self) -> Result<Option<(String, Vec<u8>)>, String> {
        let path = if let Some(scripted) = self.open_files.pop_front() {
            scripted
        } else if let Some(path) =
            dialogs::open_file(self.owner.cast(), "Open a session", &session_files(true))?
        {
            path
        } else {
            return Ok(None);
        };
        self.read_recent(&path).map(Some)
    }

    fn read_recent(&mut self, path: &Path) -> Result<(String, Vec<u8>), String> {
        let opened = calc_platform_desktop::origin::open_file(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        self.origin = opened.origin;
        self.path = Some(opened.path);
        Ok((opened.name, opened.bytes))
    }

    fn opened_path(&self) -> Option<PathBuf> {
        self.path.clone()
    }

    fn store_macro_level(&mut self, level: dna_onecalc::MacroSecurity) {
        if let Err(why) = calc_platform_desktop::macro_security::save(&self.macro_folder, level) {
            eprintln!("dna-onecalc-desktop: the macro security level is not stored: {why}");
        }
    }

    fn recent(&self) -> Vec<String> {
        self.settings.borrow().recent.clone()
    }

    fn remember(&mut self, path: &str) {
        let mut settings = self.settings.borrow_mut();
        settings.push_recent(path);
        settings.save();
    }

    fn write_in_place(&mut self, path: &str, bytes: &[u8]) -> Result<bool, String> {
        // A run that saves into numbered folders never overwrites the file it opened.
        if self.save_dir.is_some() {
            return Ok(false);
        }
        std::fs::write(path, bytes).map_err(|e| format!("{path}: {e}"))?;
        Ok(true)
    }

    fn confirm_close(&mut self, name: &str) -> CloseChoice {
        if let Some(answer) = self.close_answer {
            return answer;
        }
        if self.scripted {
            eprintln!(
                "dna-onecalc-desktop: unsaved changes in {name}; no --close-answer given, the window stays open"
            );
            return CloseChoice::Cancel;
        }
        match dialogs::message(
            self.owner.cast(),
            "DNA Calc - Formula Explorer",
            &format!("Save changes to {name}?"),
            Buttons::YesNoCancel,
            false,
        ) {
            Pressed::Yes => CloseChoice::Save,
            Pressed::No => CloseChoice::Discard,
            _ => CloseChoice::Cancel,
        }
    }

    fn open_in_dna_calc(
        &mut self,
        path: Option<&Path>,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), String> {
        let Some(exe) = &self.dna_calc else {
            let text = "Formula Explorer cannot open this file, and DNA Calc (dna-calc-desktop.exe) is not in the Formula Explorer folder. Put it there to open such files in DNA Calc from here, or open the file in DNA Calc yourself.".to_owned();
            say(
                self.owner,
                self.message_file.as_deref(),
                self.scripted,
                &text,
            );
            return Err(text);
        };
        // A file the page read has no path: a copy goes in the app's data folder.
        let file = match path {
            Some(path) => path.to_owned(),
            None => {
                let folder = self.settings.borrow().folder().join("refused");
                std::fs::create_dir_all(&folder)
                    .map_err(|e| format!("{}: {e}", folder.display()))?;
                let copy = folder.join(name);
                std::fs::write(&copy, bytes).map_err(|e| format!("{}: {e}", copy.display()))?;
                copy
            }
        };
        std::process::Command::new(exe) // ubs:ignore - Executable is the sibling DNA Calc application discovered by the native shell.
            .arg("--open")
            .arg(&file)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("DNA Calc did not start ({}): {e}", exe.display()))
    }

    fn opened_origin(&self) -> dna_onecalc::FileOrigin {
        self.origin
    }

    fn choose_save(&mut self, suggested: &str, bytes: &[u8]) -> Result<Option<String>, String> {
        if let Some((directory, count)) = &mut self.save_dir {
            *count += 1;
            let folder = directory.join(count.to_string());
            std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
            let path = folder.join(suggested);
            std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            return Ok(Some(path.display().to_string()));
        }
        // A session with a VBA project is an .xlsm.
        let extension = if suggested.ends_with(".xlsm") {
            "xlsm"
        } else {
            "xlsx"
        };
        let Some(path) = dialogs::save_file(
            self.owner.cast(),
            "Save the session",
            &session_files(false),
            extension,
        )?
        else {
            return Ok(None);
        };
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Some(path.display().to_string()))
    }

    fn choose_addin(&mut self) -> Result<Option<PathBuf>, String> {
        dialogs::open_file(self.owner.cast(), "Load an add-in", &ADDIN_FILES)
    }
}

const ADDIN_FILES: [FileFilter<'static>; 2] = [
    FileFilter {
        name: "Excel add-ins (*.xll; *.xlam)",
        patterns: "*.xll;*.xlam",
    },
    FileFilter {
        name: "All files",
        patterns: "*.*",
    },
];

/// How long an evaluation waits for an add-in's asynchronous return before it gives up.
const RETURN_WAIT: Duration = Duration::from_secs(30);

/// How long after a load the host keeps running the add-ins' events and message loop to let a
/// slow add-in finish registering (a Python runtime does so after its open routine returns), and
/// how long the registrations must stay unchanged to call that done.
const SETTLE_QUIET: Duration = Duration::from_millis(800);
const SETTLE_LIMIT: Duration = Duration::from_secs(60);

/// The .xll add-ins this process loaded, over `calc-addin-host`'s add-in set.
pub(crate) struct Addins {
    set: Rc<RefCell<AddinHost>>,
    /// What the registrations looked like when the table was last published.
    published: Cell<u64>,
}

impl Addins {
    pub(crate) fn new(set: Rc<RefCell<AddinHost>>) -> Self {
        Addins {
            set,
            published: Cell::new(0),
        }
    }

    /// A number that changes when a registration is added or removed.
    fn signature(&self) -> u64 {
        let set = self.set.borrow();
        set.registrations().iter().fold(0u64, |sum, r| {
            sum.wrapping_mul(1_000_003)
                .wrapping_add(r.id)
                .wrapping_add(u64::from(r.use_count) << 40)
        })
    }
}

impl Libraries for Addins {
    fn load_xll(&mut self, path: &Path) -> Result<(), String> {
        self.set.borrow_mut().load(path)?;
        self.set.borrow().settle(SETTLE_QUIET, SETTLE_LIMIT);
        Ok(())
    }

    fn unload_xll(&mut self, path: &Path) -> Result<(), String> {
        self.set.borrow_mut().unload(path)
    }

    fn loaded(&self) -> Vec<(PathBuf, Vec<String>)> {
        self.set.borrow().xll_functions()
    }

    fn load_xlam(
        &mut self,
        path: &Path,
        project: &calc_vba::OfficeProject,
        install: bool,
    ) -> Result<Vec<String>, String> {
        let load = self
            .set
            .borrow_mut()
            .load_vba_addin(path, project, install)?;
        Ok(load.notes)
    }

    fn unload_xlam(&mut self, path: &Path) -> Result<(), String> {
        if self.set.borrow_mut().unload_vba_addin(path) {
            Ok(())
        } else {
            Err(format!("{} is not loaded", path.display()))
        }
    }

    fn loaded_xlam(&self) -> Vec<(PathBuf, Vec<String>)> {
        self.set.borrow().vba_addins()
    }

    fn load_vba(&mut self, name: &str, source: &str) -> Result<(), String> {
        self.set
            .borrow_mut()
            .load_vba_module(name, source, calc_vba::DEFAULT_STEP_BUDGET)
    }

    fn unload_vba(&mut self) {
        let mut set = self.set.borrow_mut();
        let modules: Vec<String> = set.vba_modules().map(str::to_owned).collect();
        for module in modules {
            set.unload_vba_module(&module);
        }
    }

    fn callables(&self) -> Result<CallableTable, String> {
        let table = self.set.borrow().callables()?;
        self.published.set(self.signature());
        Ok(table)
    }

    fn changed(&mut self) -> bool {
        self.signature() != self.published.get() // ubs:ignore - Public registration-count signatures detect changes; no authentication data.
    }

    fn cell_changed(&mut self, _row: u32, _column: u32) {
        // OneCalc has scratch inputs, not workbook Worksheet/Range objects (dc-svqh).
    }

    fn calculated(&mut self) {
        if let Err(why) = self.set.borrow().after_calculate() {
            eprintln!("dna-onecalc-desktop: add-in events: {why}");
        }
    }

    fn rtd(&self) -> Option<Rc<dyn RtdLinks>> {
        let client: Rc<RtdClient> = self.set.borrow().rtd().cloned()?;
        Some(client)
    }

    fn registration_names(&self) -> Vec<String> {
        self.set
            .borrow()
            .registrations()
            .iter()
            .map(|r| r.name.clone())
            .collect()
    }

    fn poll_rtd(&mut self, evaluating: bool) -> Vec<RtdUpdate> {
        match self.set.borrow().poll_rtd(evaluating) {
            Ok(inputs) => inputs
                .into_iter()
                .map(|input| RtdUpdate {
                    topic: input.topic,
                    lease: input.lease,
                    sequence: input.sequence,
                    value: input.value,
                })
                .collect(),
            Err(why) => {
                eprintln!("dna-onecalc-desktop: RTD: {why}");
                Vec::new()
            }
        }
    }
}

/// The host drives calc-com-host's RTD client through its seam.
impl RtdLinks for RtdClient {
    fn begin_replacement(
        &self,
        providers: &std::collections::BTreeSet<RtdProvider>,
        caller: calc_value::CellKey,
    ) -> Result<Vec<(calc_fml::TopicKey, calc_fml::RtdLease)>, String> {
        RtdClient::begin_replacement(self, providers, caller)
    }

    fn finish_replacement(
        &self,
        providers: &std::collections::BTreeSet<RtdProvider>,
        caller: calc_value::CellKey,
    ) -> Result<(), String> {
        RtdClient::finish_replacement(self, providers, caller)
    }

    fn reconcile_evaluation(
        &self,
        providers: &std::collections::BTreeSet<RtdProvider>,
        caller: calc_value::CellKey,
        invocations: &std::collections::BTreeSet<u64>,
    ) -> Result<(), String> {
        RtdClient::reconcile_evaluation(self, providers, caller, invocations)
    }
}

/// Where an evaluation waits for an add-in's asynchronous return: it takes the returns the add-in
/// host has, and runs the message loop while it has none.
struct Returns {
    set: Rc<RefCell<AddinHost>>,
    arrived: BTreeMap<u64, Result<CalcValue, String>>,
    /// When the calculation under way began.
    since: Instant,
}

impl std::fmt::Debug for Returns {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Returns")
            .field("arrived", &self.arrived.len())
            .finish()
    }
}

impl ExternalReturns for Returns {
    fn begin(&mut self) {
        self.since = Instant::now();
    }

    fn idle(&mut self) -> Result<(), String> {
        if self.since.elapsed() > RETURN_WAIT {
            return Err(format!(
                "an add-in call on the pool did not finish within {} seconds",
                RETURN_WAIT.as_secs()
            ));
        }
        pump_pending();
        std::thread::sleep(Duration::from_millis(1));
        Ok(())
    }

    fn await_return(&mut self, request: u64) -> Result<CalcValue, String> {
        let deadline = Instant::now() + RETURN_WAIT;
        loop {
            for returned in self.set.borrow().take_async_returns() {
                self.arrived.insert(returned.request, returned.value);
            }
            if let Some(value) = self.arrived.remove(&request) {
                return value;
            }
            if Instant::now() > deadline {
                return Err(format!(
                    "the add-in did not return request {request} within {} seconds",
                    RETURN_WAIT.as_secs()
                ));
            }
            pump_pending();
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

/// The session, its add-ins and its add-in set, as this process builds them: in app mode in-process,
/// or in the surrogate role.
pub(crate) fn session_and_addins(
    shell: &Shell,
    sheet: &str,
) -> Result<(Session, Rc<RefCell<AddinHost>>), String> {
    let mut session = Session::new(sheet).map_err(|e| format!("{e:?}"))?;
    let sheets: Vec<(calc_value::SheetId, String)> = session
        .sheets()
        .map(|(name, id)| (id, name.to_owned()))
        .collect();
    let sheet_refs: Vec<(calc_value::SheetId, &str)> = sheets
        .iter()
        .map(|(id, name)| (*id, name.as_str()))
        .collect();
    // The add-in set always exists: the page loads add-ins into it whenever the person asks.
    let set = Rc::new(RefCell::new(AddinHost::with_profile(
        shell,
        &Document {
            workbook: WORKBOOK,
            active_sheet: Some(sheet),
            sheets: &sheet_refs,
        },
        calc_vba::Profile::ValuesOnly,
    )?));
    // Registered functions that answer later (native async, thread-safe on the pool) suspend the
    // evaluation instead of blocking the call; the session waits for their returns.
    set.borrow().enable_async_continuations()?;
    session.set_external_returns(Some(Box::new(Returns {
        set: set.clone(),
        arrived: BTreeMap::new(),
        since: Instant::now(),
    })));
    // The add-ins' real-time servers reach the formula: the session joins and starts their topics.
    let rtd = set.borrow_mut().enable_rtd()?;
    session.set_rtd_connector(rtd);
    Ok((session, set))
}

/// The session behind the page: in this process, or in the add-in process.
enum Backend {
    Local(Host<Dialogs, Addins>),
    Remote(Controller),
}

/// The window's side of the add-in process: the dialogs it serves, the supervisor that started it,
/// and the connection.
struct Controller {
    dialogs: Dialogs,
    supervisor: Supervisor,
    remote: Option<Remote<StreamWire<std::process::ChildStdin>>>,
    /// What the add-in process is started with after its role and incarnation.
    launch: Vec<std::ffi::OsString>,
    limit: Duration,
}

impl Controller {
    /// Starts a new add-in process (a running one is ended first) and waits for its hello.
    fn start(&mut self) -> Result<(), String> {
        self.end("replaced");
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let (incarnation, wire) = self.supervisor.launch(&exe, &self.launch)?;
        let remote = Remote::connect(incarnation, wire, START_LIMIT, &mut || {
            pump_pending();
            true
        });
        match remote {
            Ok(remote) => {
                self.remote = Some(remote);
                Ok(())
            }
            Err(why) => {
                self.supervisor.stop();
                Err(why)
            }
        }
    }

    /// Ends the add-in process at once, and retires its incarnation.
    fn end(&mut self, why: &str) {
        if let Some(remote) = &mut self.remote {
            remote.stopped(why);
        }
        self.remote = None;
        self.supervisor.stop();
    }

    /// One page request. `stop` is set when the person asks to stop the add-in process while the
    /// request waits.
    fn handle(&mut self, request: Request, stop: &Cell<bool>) -> Reply {
        match request {
            Request::StopAddinProcess => {
                self.end("stopped by the person");
                Reply::Addins(Vec::new())
            }
            Request::RestartAddinProcess => match self.start() {
                Ok(()) => Reply::Addins(Vec::new()),
                Err(message) => Reply::Refused { message },
            },
            request => {
                let Some(remote) = &mut self.remote else {
                    return Reply::Refused {
                        message: crate::surrogate::NOT_RUNNING.to_owned(),
                    };
                };
                stop.set(false);
                let reply = remote.call(&request, &mut self.dialogs, self.limit, &mut || {
                    pump_pending();
                    !stop.get()
                });
                if stop.replace(false) || !remote.running() {
                    self.end("stopped");
                }
                reply
            }
        }
    }

    /// The add-in process's events since the last tick, and `addin-process-stopped` when it ended
    /// by itself.
    fn events(&mut self) -> Vec<String> {
        let Some(remote) = &mut self.remote else {
            return Vec::new();
        };
        let mut events = remote.take_events();
        let ended = match self.supervisor.poll() {
            calc_platform_desktop::surrogate::Ended::Exited(code) => Some(format!(
                "the add-in process ended (exit code {})",
                code.map_or("unknown".to_owned(), |c| format!("{:#x}", c as u32))
            )),
            _ if !remote.running() => Some(remote.ended().unwrap_or("ended").to_owned()),
            _ => None,
        };
        if let Some(why) = ended {
            eprintln!("dna-onecalc-desktop: {why}");
            self.end(&why);
            events.push("addin-process-stopped".to_owned());
        }
        events
    }

    fn close_requested(&mut self) -> bool {
        match &mut self.remote {
            None => true,
            Some(remote) => remote.close_requested(&mut self.dialogs, self.limit, &mut || {
                pump_pending();
                true
            }),
        }
    }
}

impl Drop for Controller {
    /// The window closed: the add-in process's input closes, so it closes its add-ins and exits;
    /// one that does not within the limit is ended.
    fn drop(&mut self) {
        self.remote = None;
        self.supervisor.finish(FINISH_LIMIT);
    }
}

struct App {
    backend: RefCell<Backend>,
    web: WebView,
    /// The outermost window (as `Shell::hwnd` gives it), for its place.
    root: usize,
    settings: Rc<RefCell<Settings>>,
    /// Requests that arrived while one was being answered (an evaluation that waits for an add-in
    /// runs the message loop, which delivers the page's next message): answered in order after it.
    waiting: RefCell<VecDeque<String>>,
    busy: Cell<bool>,
    /// The person asked to stop the add-in process while a request waits for it.
    stop: Cell<bool>,
}

/// Whether the page's message is the request to stop the add-in process.
fn asks_to_stop(message: &str) -> bool {
    serde_json::from_str::<Call>(message)
        .is_ok_and(|call| call.request == Request::StopAddinProcess)
}

impl App {
    /// One request from the page: answered with the same id. A request that arrives while another
    /// is being answered waits its turn; only Stop add-in process acts at once (on the request
    /// being waited for).
    fn request(&self, message: &str) {
        if self.busy.get() && asks_to_stop(message) {
            self.stop.set(true);
        }
        self.waiting.borrow_mut().push_back(message.to_owned());
        if self.busy.replace(true) {
            return;
        }
        loop {
            let Some(next) = self.waiting.borrow_mut().pop_front() else {
                break;
            };
            let (answer, remote) = self.answer(&next);
            match answer {
                Some(text) => {
                    if let Err(why) = self.web.post(&text, None) {
                        eprintln!("dna-onecalc-desktop: {why}");
                    }
                }
                None => eprintln!("dna-onecalc-desktop: a message that is no call was ignored"),
            }
            // The page learns that the add-ins run in a process of their own (it can stop and
            // restart it) from the answer to its first question about them.
            if remote
                && serde_json::from_str::<Call>(&next).is_ok_and(|c| c.request == Request::Addins)
            {
                let _ = self.web.post(r#"{"event":"addin-process"}"#, None);
            }
        }
        self.busy.set(false);
    }

    /// The JSON text of the answer to the JSON text of a call, and whether the add-ins run in a
    /// process of their own.
    fn answer(&self, text: &str) -> (Option<String>, bool) {
        let mut backend = self.backend.borrow_mut();
        match &mut *backend {
            Backend::Local(host) => (host.answer(text), false),
            Backend::Remote(controller) => {
                let Ok(Call { id, request }) = serde_json::from_str(text) else {
                    return (None, true);
                };
                let reply = controller.handle(request, &self.stop);
                (serde_json::to_string(&Answer { id, reply }).ok(), true)
            }
        }
    }

    /// The window is closing: asks about unsaved changes, and keeps the window's place for the next
    /// run. `false` keeps the window open.
    fn close_requested(&self) -> bool {
        // A request being answered (an add-in call that pumps messages) is in the way: not now.
        let Ok(mut backend) = self.backend.try_borrow_mut() else {
            return false;
        };
        let allowed = match &mut *backend {
            Backend::Local(host) => host.close_requested(),
            Backend::Remote(controller) => controller.close_requested(),
        };
        drop(backend);
        if allowed {
            if let Some(place) = calc_platform_desktop::placement_of(self.root) {
                let mut settings = self.settings.borrow_mut();
                settings.window = Some(place);
                settings.save();
            }
        }
        allowed
    }

    /// Real-time values the add-ins' servers sent: the page evaluates its formula again. From the
    /// add-in process, its events, and that it ended.
    fn poll_rtd(&self) {
        if self.busy.get() {
            return;
        }
        let Ok(mut backend) = self.backend.try_borrow_mut() else {
            return;
        };
        let events = match &mut *backend {
            Backend::Local(host) => {
                if host.poll_rtd() {
                    vec!["rtd".to_owned()]
                } else {
                    Vec::new()
                }
            }
            Backend::Remote(controller) => controller.events(),
        };
        drop(backend);
        for event in events {
            let text = serde_json::json!({ "event": event }).to_string();
            if let Err(why) = self.web.post(&text, None) {
                eprintln!("dna-onecalc-desktop: {why}");
            }
        }
    }

    /// RTD updates and the window's geometry, once per tick.
    fn tick(&self) {
        self.poll_rtd();
        self.web.sync_parent();
    }
}

/// The macro security options of the command line, passed on to the add-in process.
fn macro_args(o: &Options) -> Vec<std::ffi::OsString> {
    let mut args = Vec::new();
    if o.trust_macros {
        args.push("--trust-macros".into());
    }
    if let Some(level) = o.macros {
        args.push("--macros".into());
        args.push(level.key().into());
    }
    if let Some(dir) = &o.data_dir {
        args.push("--data-dir".into());
        args.push(dir.into());
    }
    args
}

pub fn run(args: &[String]) -> Result<(), String> {
    let o = options(args)?;
    // WebView2 is the app's window: say clearly what is missing and where to get it.
    if calc_platform_desktop::runtime_version().is_none() {
        let text = crate::embedded::missing_runtime_message();
        say(
            std::ptr::null_mut(),
            o.message_file.as_deref(),
            o.scripted(),
            &text,
        );
        if !o.scripted()
            && dialogs::message(
                std::ptr::null_mut(),
                "DNA Calc - Formula Explorer",
                "Open the download page now?",
                Buttons::YesNo,
                false,
            ) == Pressed::Yes
        {
            dialogs::open_url(calc_platform_desktop::RUNTIME_DOWNLOAD);
        }
        return Err(text);
    }
    per_monitor_dpi_aware();
    let data_dir = o
        .data_dir
        .clone()
        .unwrap_or_else(crate::settings::default_folder);
    let settings = Rc::new(RefCell::new(Settings::load(&data_dir)));
    let macro_folder = o
        .data_dir
        .clone()
        .unwrap_or_else(calc_platform_desktop::macro_security::default_folder);
    let shell = Shell::new("DNA Calc - Formula Explorer")?;
    let frame = shell.workbook_hwnd();
    let size = o.size.unwrap_or((1280, 900));
    match (o.size, settings.borrow().window) {
        // Where the window was, when that is still on a display.
        (None, Some(place)) if place.is_on_screen() => {
            shell.show_placement(&place);
        }
        _ => shell.show(size.0, size.1),
    }
    let user_data = std::env::var_os("DNACALC_WEBVIEW2_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.join("WebView2"));
    let mut browser_args = String::new();
    if let Some(port) = o.debug_port {
        browser_args.push_str(&format!(
            " --remote-debugging-port={port} --remote-allow-origins=*"
        ));
    }
    let web = WebView::create(
        frame,
        &WebViewOptions {
            user_data: &user_data,
            browser_args,
        },
    )?;

    let dialogs = Dialogs {
        owner: frame.cast(),
        save_dir: o.save_dir.clone().map(|d| (d, 0)),
        open_files: o.open_files.iter().cloned().collect(),
        origin: dna_onecalc::FileOrigin::Unknown,
        path: None,
        settings: settings.clone(),
        close_answer: o.close_answer,
        message_file: o.message_file.clone(),
        dna_calc: dna_calc_beside_us(),
        scripted: o.scripted(),
        macro_folder: macro_folder.clone(),
    };
    // In-process (the conformance mode): the session and the add-in set live here. With the
    // surrogate transport they live in the add-in process, and this process keeps none.
    let mut set = None;
    let mut backend = if o.surrogate {
        let mut launch: Vec<std::ffi::OsString> = vec!["--sheet".into(), o.sheet.clone().into()];
        launch.extend(macro_args(&o));
        let mut controller = Controller {
            dialogs,
            supervisor: Supervisor::new(),
            remote: None,
            launch,
            limit: o.call_limit,
        };
        controller.start()?;
        Backend::Remote(controller)
    } else {
        let (session, addins) = session_and_addins(&shell, &o.sheet)?;
        set = Some(addins.clone());
        let mut host = Host::new(session, dialogs, Addins::new(addins));
        // The macro security level in force: what the person stored, or the level pinned by
        // `--trust-macros` or `DNACALC_TRUST_ALL_MACROS=1`.
        host.session_mut()
            .set_macro_policy(calc_platform_desktop::macro_security::policy(
                &macro_folder,
                o.trust_macros,
                o.macros,
            ));
        Backend::Local(host)
    };
    let stop = Cell::new(false);
    for xll in &o.xlls {
        let request = Request::LoadAddin {
            path: xll.display().to_string(),
            expected_hash: None,
        };
        let reply = match &mut backend {
            Backend::Local(host) => host.handle(request),
            Backend::Remote(controller) => controller.handle(request, &stop),
        };
        match reply {
            Reply::Addins(_) => {}
            Reply::Refused { message } => return Err(message),
            other => {
                return Err(format!(
                    "unexpected answer loading {}: {other:?}",
                    xll.display()
                ));
            }
        }
    }
    // Declared after the shell so that it drops first: the add-ins close while their windows exist.
    let app = Rc::new(App {
        backend: RefCell::new(backend),
        web,
        root: shell.hwnd(),
        settings: settings.clone(),
        waiting: RefCell::new(VecDeque::new()),
        busy: Cell::new(false),
        stop,
    });
    {
        // The page's messages must not keep the app alive: a weak handle lets it drop at exit.
        let weak = Rc::downgrade(&app);
        app.web.on_message(move |message| {
            if let Some(app) = weak.upgrade() {
                app.request(&message);
            }
        })?;
    }
    // The window's title follows the page's: the session's name and a mark for unsaved changes.
    let root = shell.hwnd();
    app.web
        .on_title_changed(move |title| calc_platform_desktop::set_window_title(root, &title))?;
    {
        let weak = Rc::downgrade(&app);
        shell.set_close_handler(Some(Rc::new(move || {
            weak.upgrade().is_none_or(|app| app.close_requested())
        })));
    }
    let target = if let Some(url) = &o.url {
        url.clone()
    } else if let Some(assets) = &o.assets {
        app.web.map_folder("dnacalc.app", assets)?;
        "https://dnacalc.app/index.html".to_owned()
    } else {
        let page = crate::embedded::page()?;
        app.web.serve_assets("dnacalc.app", Rc::new(page))?;
        "https://dnacalc.app/index.html".to_owned()
    };
    app.web.navigate(&target)?;
    let timer = ThreadTimer::start(TICK_ID, TICK_MS);
    // The loop ends when the window is closed.
    run_message_loop(
        || calc_platform_desktop::client_size(frame).is_none(),
        |message| {
            if message == ThreadMessage::Timer(TICK_ID) {
                app.tick();
            }
        },
    );
    drop(timer);
    drop(app);
    drop(set);
    drop(shell);
    Ok(())
}

/// The surrogate role (`dna-onecalc-desktop --surrogate <incarnation> [--sheet NAME] [--data-dir
/// DIR] [--trust-macros] [--macros LEVEL]`): the session and its add-ins, with the hidden window
/// chain and the message pump the add-ins need, answering the window's process over its standard
/// input and output. Its dialogs are the window's.
pub fn run_surrogate_role(args: &[String]) -> Result<(), String> {
    let mut args = args.iter();
    let incarnation = args
        .next()
        .and_then(|text| calc_surrogate_wire::Incarnation::parse(text))
        .ok_or("--surrogate needs the incarnation the window chose")?;
    let (mut sheet, mut data_dir, mut trust_macros, mut macros) =
        ("Sheet1".to_owned(), None, false, None);
    while let Some(arg) = args.next() {
        let mut value = || args.next().cloned().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--sheet" => sheet = value()?,
            "--data-dir" => data_dir = Some(PathBuf::from(value()?)),
            "--trust-macros" => trust_macros = true,
            "--macros" => {
                let text = value()?;
                macros = Some(
                    dna_onecalc::MacroSecurity::parse(&text)
                        .ok_or(format!("--macros is off, guarded or unguarded, not {text}"))?,
                )
            }
            other => return Err(format!("unknown surrogate argument: {other}")),
        }
    }
    // Frames go out on the standard output this process was started with; anything an add-in
    // prints goes to standard error from here on.
    let wire = calc_platform_desktop::surrogate::surrogate_wire()?;
    let shell = Shell::new("DNA Calc - Formula Explorer")?;
    let (session, set) = session_and_addins(&shell, &sheet)?;
    let end = SurrogateEnd::new(incarnation, Box::new(wire));
    let mut host = Host::new(session, RemoteFiles(end.clone()), Addins::new(set.clone()));
    let macro_folder =
        data_dir.unwrap_or_else(calc_platform_desktop::macro_security::default_folder);
    host.session_mut()
        .set_macro_policy(calc_platform_desktop::macro_security::policy(
            &macro_folder,
            trust_macros,
            macros,
        ));
    let result = run_surrogate(&mut host, &end, |host| {
        pump_pending();
        host.poll_rtd()
    });
    // The add-ins close while their windows exist.
    drop(host);
    drop(set);
    drop(shell);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: argument parsing for app mode.
    #[test]
    fn app_options_need_a_page() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            options(&args(&["--app"])).is_err(),
            !crate::embedded::has_page()
        );
        let o = options(&args(&[
            "--app", "--assets", "dist", "--size", "800x600", "--xll", "a.xll",
        ]))
        .unwrap();
        assert_eq!((o.size, o.xlls.len()), (Some((800, 600)), 1));
        let o = options(&args(&[
            "--app",
            "--url",
            "http://x/",
            "--data-dir",
            "d",
            "--close-answer",
            "save",
            "--message-file",
            "m.txt",
        ]))
        .unwrap();
        assert_eq!(o.close_answer, Some(CloseChoice::Save));
        assert_eq!(
            (o.data_dir, o.message_file),
            (Some("d".into()), Some("m.txt".into()))
        );
        assert!(
            options(&args(&[
                "--app",
                "--url",
                "http://x/",
                "--close-answer",
                "maybe"
            ]))
            .is_err()
        );
        assert!(options(&args(&["--app", "--url", "http://127.0.0.1:1/"])).is_ok());
        assert!(options(&args(&["--app", "--bogus"])).is_err());
        // --open-file answers the Open dialog, in order, and makes the run scripted.
        let o = options(&args(&[
            "--app", "--url", "http://x/", "--open-file", "a.xlsm", "--open-file", "b.xlsx",
        ]))
        .unwrap();
        assert_eq!(o.open_files, vec![PathBuf::from("a.xlsm"), PathBuf::from("b.xlsx")]);
        assert!(o.scripted());
        assert!(options(&args(&["--app", "--url", "http://x/", "--open-file"])).is_err());
        // `--macros guarded` is the canonical name; `--macros safe` remains an alias (dc-8ovd).
        assert_eq!(
            options(&args(&["--app", "--url", "http://x/", "--macros", "guarded"]))
                .unwrap()
                .macros,
            Some(dna_onecalc::MacroSecurity::Guarded)
        );
        assert_eq!(
            options(&args(&["--app", "--url", "http://x/", "--macros", "safe"]))
                .unwrap()
                .macros,
            Some(dna_onecalc::MacroSecurity::Guarded)
        );
    }
}
