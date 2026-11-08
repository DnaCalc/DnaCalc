//! App mode (`dna-onecalc-desktop --app`): the page the browsers run, hosted in WebView2, with the
//! session native behind the page.
//!
//! The shell is skin deep. The page is OneCalc's surface over a bridge link
//! ([`dna_onecalc::bridge`]); this module carries each request string the page posts to
//! [`Host::answer`] on the STA thread (the one that owns add-in calls) and posts the answer back.
//! The window chain, the WebView2 host and its string bridge are `calc-platform-desktop`'s, the
//! add-in set `calc-addin-host`'s, and the file dialogs `calc-platform-desktop`'s: nothing here is
//! OneCalc-only beyond wiring them to the host.
use crate::host::{Files, Host, Libraries};
use calc_addin_host::{AddinHost, Document};
use calc_fml::CallableTable;
use calc_platform_desktop::dialogs::{self, FileFilter};
use calc_platform_desktop::{
    Shell, ThreadMessage, ThreadTimer, WebView, WebViewOptions, per_monitor_dpi_aware,
    pump_pending, run_message_loop,
};
use calc_value::CalcValue;
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
    /// The built page (`cargo xtask app bundle onecalc`), mapped as `https://dnacalc.app/`.
    assets: Option<PathBuf>,
    xlls: Vec<PathBuf>,
    /// The first sheet's name, which add-ins see (the comparisons with Excel name it as the
    /// Excel workbook they ran in did).
    sheet: String,
    debug_port: Option<u16>,
    size: (i32, i32),
    /// Save sessions here, each in a new numbered folder under the name the session suggests,
    /// instead of asking in the save dialog: an acceptance run drives the page's Save without a
    /// person (the dialog's own code is not exercised then).
    save_dir: Option<PathBuf>,
}

fn options(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        url: None,
        assets: None,
        xlls: Vec::new(),
        sheet: "Sheet1".to_owned(),
        debug_port: None,
        size: (1280, 900),
        save_dir: None,
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
            "--debug-port" => {
                o.debug_port = Some(value()?.parse().map_err(|_| "--debug-port needs a port")?)
            }
            "--size" => {
                let text = value()?;
                let (w, h) = text.split_once('x').ok_or("--size needs WIDTHxHEIGHT")?;
                o.size = (
                    w.parse().map_err(|_| "--size width")?,
                    h.parse().map_err(|_| "--size height")?,
                );
            }
            other => return Err(format!("unknown app argument: {other}")),
        }
    }
    if o.url.is_none() && o.assets.is_none() {
        return Err("app mode needs --assets DIR (the built page) or --url URL".into());
    }
    Ok(o)
}

/// The workbook name the add-ins see.
const WORKBOOK: &str = "Book1";

/// The timer that follows the window's size (milliseconds).
const TICK_MS: u32 = 50;
const TICK_ID: usize = 0x4f4e43;

/// The native file dialogs, owned by the window.
struct Dialogs {
    owner: *mut std::ffi::c_void,
    /// `--save-dir`: where saves go without the dialog, and how many there have been.
    save_dir: Option<(PathBuf, usize)>,
    /// The Mark of the Web zone of the file last chosen to open.
    zone: Option<u32>,
}

/// The zone in a file's `Zone.Identifier` alternate data stream (`ZoneId=3`), when it has one.
fn mark_of_the_web(path: &std::path::Path) -> Option<u32> {
    let mut stream = path.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    let text = std::fs::read_to_string(std::path::PathBuf::from(stream)).ok()?;
    zone_of(&text)
}

/// `ZoneId=<n>` in the text of a `Zone.Identifier` stream.
pub(crate) fn zone_of(text: &str) -> Option<u32> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("ZoneId="))
        .find_map(|zone| zone.trim().parse().ok())
}

const SESSION_FILES: [FileFilter<'static>; 2] = [
    FileFilter {
        name: "OneCalc sessions (*.xlsx; *.xlsm)",
        patterns: "*.xlsx;*.xlsm",
    },
    FileFilter {
        name: "All files",
        patterns: "*.*",
    },
];

impl Files for Dialogs {
    fn choose_open(&mut self) -> Result<Option<(String, Vec<u8>)>, String> {
        let Some(path) = dialogs::open_file(self.owner.cast(), "Open a session", &SESSION_FILES)?
        else {
            return Ok(None);
        };
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        self.zone = mark_of_the_web(&path);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Some((name, bytes)))
    }

    fn opened_zone(&self) -> Option<u32> {
        self.zone
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
        let extension = if suggested.ends_with(".xlsm") { "xlsm" } else { "xlsx" };
        let Some(path) =
            dialogs::save_file(self.owner.cast(), "Save the session", &SESSION_FILES, extension)?
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
struct Addins {
    set: Rc<RefCell<AddinHost>>,
    /// Each loaded .xll with the highest registration id there was before it loaded: a
    /// registration belongs to the last add-in whose floor is below its id.
    xlls: Vec<(PathBuf, u64)>,
    /// What the registrations looked like when the table was last published.
    published: Cell<u64>,
}

impl Addins {
    fn new(set: Rc<RefCell<AddinHost>>) -> Self {
        Addins {
            set,
            xlls: Vec::new(),
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

    /// Runs the add-ins' events and the message loop until the registrations have been quiet for
    /// [`SETTLE_QUIET`].
    fn settle(&self) {
        let (start, mut quiet_since, mut last) = (Instant::now(), Instant::now(), self.signature());
        while start.elapsed() < SETTLE_LIMIT && quiet_since.elapsed() < SETTLE_QUIET {
            pump_pending();
            let _ = self.set.borrow().after_calculate();
            std::thread::sleep(Duration::from_millis(10));
            let now = self.signature();
            if now != last {
                last = now;
                quiet_since = Instant::now();
            }
        }
    }
}

impl Libraries for Addins {
    fn load_xll(&mut self, path: &Path) -> Result<(), String> {
        let floor = self
            .set
            .borrow()
            .registrations()
            .iter()
            .map(|r| r.id)
            .max()
            .unwrap_or(0);
        self.set.borrow_mut().load(path)?;
        self.xlls.push((path.to_owned(), floor));
        self.settle();
        Ok(())
    }

    fn unload_xll(&mut self, path: &Path) -> Result<(), String> {
        self.set.borrow_mut().unload(path)?;
        self.xlls.retain(|(held, _)| held != path);
        Ok(())
    }

    fn loaded(&self) -> Vec<(PathBuf, Vec<String>)> {
        let registrations = self.set.borrow().registrations();
        self.xlls
            .iter()
            .enumerate()
            .map(|(index, (path, floor))| {
                let ceiling = self
                    .xlls
                    .get(index + 1)
                    .map_or(u64::MAX, |(_, next_floor)| *next_floor);
                let mut own: Vec<_> = registrations
                    .iter()
                    .filter(|r| {
                        r.id > *floor
                            && r.id <= ceiling
                            && r.macro_type == 1
                            && r.use_count > 0
                            && !r.name.is_empty()
                    })
                    .collect();
                own.sort_by_key(|r| r.id);
                (path.clone(), own.iter().map(|r| r.name.clone()).collect())
            })
            .collect()
    }

    fn load_xlam(
        &mut self,
        path: &Path,
        project: &calc_vba::OfficeProject,
        install: bool,
    ) -> Result<Vec<String>, String> {
        let load = self.set.borrow_mut().load_vba_addin(path, project, install)?;
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
        self.signature() != self.published.get()
    }

    fn cell_changed(&mut self, row: u32, column: u32) {
        if let Err(why) = self.set.borrow().sheet_changed(row, column) {
            eprintln!("dna-onecalc-desktop: add-in events: {why}");
        }
    }

    fn calculated(&mut self) {
        if let Err(why) = self.set.borrow().after_calculate() {
            eprintln!("dna-onecalc-desktop: add-in events: {why}");
        }
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

struct App {
    host: RefCell<Host<Dialogs, Addins>>,
    web: WebView,
    frame: *mut std::ffi::c_void,
    bounds: Cell<(i32, i32)>,
    /// Requests that arrived while one was being answered (an evaluation that waits for an add-in
    /// runs the message loop, which delivers the page's next message): answered in order after it.
    waiting: RefCell<VecDeque<String>>,
    busy: Cell<bool>,
}

impl App {
    /// One request from the page: answered with the same id. A request that arrives while another
    /// is being answered waits its turn.
    fn request(&self, message: &str) {
        self.waiting.borrow_mut().push_back(message.to_owned());
        if self.busy.replace(true) {
            return;
        }
        loop {
            let Some(next) = self.waiting.borrow_mut().pop_front() else {
                break;
            };
            let answer = self.host.borrow_mut().answer(&next);
            match answer {
                Some(text) => {
                    if let Err(why) = self.web.post(&text, None) {
                        eprintln!("dna-onecalc-desktop: {why}");
                    }
                }
                None => eprintln!("dna-onecalc-desktop: a message that is no call was ignored"),
            }
        }
        self.busy.set(false);
    }

    /// The window's size, once per tick.
    fn tick(&self) {
        if let Some((width, height)) = calc_platform_desktop::client_size(self.frame.cast()) {
            if (width, height) != self.bounds.get() && width > 0 && height > 0 {
                self.bounds.set((width, height));
                self.web.set_bounds(width, height);
            }
        }
    }
}

pub fn run(args: &[String]) -> Result<(), String> {
    let o = options(args)?;
    per_monitor_dpi_aware();
    let shell = Shell::new("DNA OneCalc")?;
    let frame = shell.workbook_hwnd();
    shell.show(o.size.0, o.size.1);
    let user_data = std::env::var_os("DNACALC_WEBVIEW2_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("dnacalc-onecalc-webview2"));
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
            size: o.size,
        },
    )?;

    let mut session = Session::new(&o.sheet).map_err(|e| format!("{e:?}"))?;
    let sheets: Vec<(calc_value::SheetId, String)> = session
        .sheets()
        .map(|(name, id)| (id, name.to_owned()))
        .collect();
    let sheet_refs: Vec<(calc_value::SheetId, &str)> =
        sheets.iter().map(|(id, name)| (*id, name.as_str())).collect();
    // The add-in set always exists: the page loads add-ins into it whenever the person asks.
    let set = Rc::new(RefCell::new(AddinHost::new(
        &shell,
        &Document {
            workbook: WORKBOOK,
            active_sheet: Some(&o.sheet),
            sheets: &sheet_refs,
        },
    )?));
    // Registered functions that answer later (native async, thread-safe on the pool) suspend the
    // evaluation instead of blocking the call; the session waits for their returns.
    set.borrow().enable_async_continuations()?;
    session.set_external_returns(Some(Box::new(Returns {
        set: set.clone(),
        arrived: BTreeMap::new(),
        since: Instant::now(),
    })));
    let mut host = Host::new(session, Dialogs { owner: frame.cast(), save_dir: o.save_dir.clone().map(|d| (d, 0)), zone: None }, Addins::new(set.clone()));
    for xll in &o.xlls {
        match host.handle(Request::LoadAddin {
            path: xll.display().to_string(),
            expected_hash: None,
        }) {
            Reply::Addins(_) => {}
            Reply::Refused { message } => return Err(message),
            other => return Err(format!("unexpected answer loading {}: {other:?}", xll.display())),
        }
    }
    // Declared after the shell so that it drops first: the add-ins close while their windows exist.
    let app = Rc::new(App {
        host: RefCell::new(host),
        web,
        frame: frame.cast(),
        bounds: Cell::new(o.size),
        waiting: RefCell::new(VecDeque::new()),
        busy: Cell::new(false),
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
    let target = if let Some(url) = &o.url {
        url.clone()
    } else {
        let assets = o.assets.as_ref().ok_or("no assets")?;
        app.web.map_folder("dnacalc.app", assets)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: argument parsing for app mode.
    #[test]
    fn app_options_need_a_page() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(options(&args(&["--app"])).is_err());
        let o = options(&args(&[
            "--app", "--assets", "dist", "--size", "800x600", "--xll", "a.xll",
        ]))
        .unwrap();
        assert_eq!((o.size, o.xlls.len()), ((800, 600), 1));
        assert!(options(&args(&["--app", "--url", "http://127.0.0.1:1/"])).is_ok());
        assert!(options(&args(&["--app", "--bogus"])).is_err());
    }
}

#[cfg(test)]
mod zone_tests {
    use super::zone_of;

    /// The `Zone.Identifier` stream of a downloaded file (Windows writes `ZoneId` under `[ZoneTransfer]`).
    #[test]
    fn the_zone_is_read_from_the_stream_text() {
        assert_eq!(zone_of("[ZoneTransfer]\r\nZoneId=3\r\nReferrerUrl=https://example.test/\r\n"), Some(3));
        assert_eq!(zone_of("[ZoneTransfer]\nZoneId=4"), Some(4));
        assert_eq!(zone_of("[ZoneTransfer]\r\nZoneId=x\r\n"), None);
        assert_eq!(zone_of(""), None);
    }

    /// A file with a real `Zone.Identifier` alternate data stream is read through its path.
    #[test]
    fn a_file_with_an_alternate_stream_reports_its_zone() {
        let dir = std::env::temp_dir().join(format!("onecalc-zone-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("marked.xlsm");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(super::mark_of_the_web(&file), None);
        let mut stream = file.as_os_str().to_owned();
        stream.push(":Zone.Identifier");
        std::fs::write(std::path::PathBuf::from(stream), "[ZoneTransfer]\r\nZoneId=3\r\n").unwrap();
        assert_eq!(super::mark_of_the_web(&file), Some(3));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
