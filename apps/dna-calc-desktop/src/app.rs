//! App mode (`dna-calc-desktop --app`): the same page the browser runs, hosted in WebView2, with
//! the workbook worker native behind the page's existing protocol (M2 of the grid design).
//!
//! The shell is skin deep. The page, the protocol and `dna_calc_worker::server::Server` are the
//! ones the browser runs; the shell only carries the page's JSON texts and tile bytes between the
//! page and a `Server` on this STA thread (`apps/dna-calc/web/desktop-bridge.js` is the page side),
//! and gives that server what a browser worker cannot have: real XLL user functions, COM and RTD,
//! and a fetch route without a cross-origin restriction. No behaviour lives here that a native UI
//! would have to redo. The window chain, the WebView2 host and its bridge are
//! `calc-platform-desktop`'s; the add-in set is `calc-addin-host`'s.
use crate::fetch::ThreadedFetch;
use crate::file_import::LocalFiles;
use calc_addin_host::{AddinHost, Document};
use calc_platform_desktop::{
    Shell, ThreadMessage, ThreadTimer, WebView, WebViewOptions, per_monitor_dpi_aware,
    run_message_loop, split_message,
};
use dna_calc_worker::{
    server::{self, Server},
    wire::{self, Event, FromServer},
};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

/// Session access for the workbook profile. All member semantics live in calc-host.
struct DesktopAutomationHost {
    application: Rc<dyn calc_object_model::profile::ApplicationHost>,
    native: std::rc::Weak<calc_xll_host::XllHost>,
    server: std::rc::Weak<RefCell<Server>>,
    events: RefCell<Rc<Cell<bool>>>,
}
impl calc_object_model::profile::ApplicationHost for DesktopAutomationHost {
    fn hwnd(&self) -> isize {
        self.application.hwnd()
    }
    fn run(
        &self,
        name: &str,
        args: &[calc_object_model::AutoValue],
    ) -> Result<calc_object_model::AutoValue, calc_object_model::AutoError> {
        self.application.run(name, args)
    }
    fn events_enabled(&self) -> bool {
        self.events.borrow().get()
    }
    fn set_events_enabled(&self, enabled: bool) {
        self.events.borrow().set(enabled);
    }
    fn path(&self) -> Result<String, calc_object_model::AutoError> {
        self.application.path()
    }
}
impl calc_host::automation::AutomationHost for DesktopAutomationHost {
    fn with_invocation(
        &self,
        call: &mut dyn FnMut(
            &mut dyn calc_func::UserFunctionContext,
        )
            -> Result<calc_object_model::AutoValue, calc_object_model::AutoError>,
    ) -> Option<Result<calc_object_model::AutoValue, calc_object_model::AutoError>> {
        let native = self.native.upgrade()?;
        let mut answer = None;
        let status = native.try_with_workbook_context(|context| {
            answer = Some(call(context));
            Ok(())
        });
        if answer.is_some() {
            return answer;
        }
        match status {
            Ok(None) => None,
            Err(why) => Some(Err(calc_object_model::AutoError::not_modelled(
                why,
                "dc-0f1k.24",
            ))),
            Ok(Some(())) => Some(Err(calc_host::automation::busy())),
        }
    }
    fn with_session(
        &self,
        call: &mut dyn FnMut(
            &calc_host::HostSession,
        )
            -> Result<calc_object_model::AutoValue, calc_object_model::AutoError>,
    ) -> Result<calc_object_model::AutoValue, calc_object_model::AutoError> {
        let server = self
            .server
            .upgrade()
            .ok_or(calc_object_model::AutoError::Disconnected)?;
        let server = server
            .try_borrow()
            .map_err(|_| calc_host::automation::busy())?;
        call(server.book().session())
    }
    fn context_changed(&self, session: &calc_host::HostSession) {
        self.events.replace(session.workbook_events_switch());
        if let Some(native) = self.native.upgrade() {
            let active = session
                .selection()
                .active
                .map(|at| at.sheet())
                .or_else(|| session.host().loaded().active_sheet())
                .or_else(|| session.host().sheets().first().map(|sheet| sheet.id()));
            let full = session.file_full_name();
            let name = full.rsplit(['/', '\\']).next().unwrap_or(full);
            for sheet in session.host().sheets() {
                native.set_sheet(
                    sheet.id(),
                    format!("[{name}]{}", sheet.name().text().to_string_lossy()),
                    Some(sheet.id()) == active,
                );
            }
        }
    }
}

fn bind_automation(
    addins: &AddinHost,
    server: &Rc<RefCell<Server>>,
) -> Result<
    (
        Rc<calc_host::automation::WorkbookAutomation>,
        Rc<dyn calc_host::WorkbookEventObserver>,
    ),
    String,
> {
    let projection = addins.projection_handle();
    let host = Rc::new(DesktopAutomationHost {
        application: projection.application_host(),
        native: Rc::downgrade(&addins.native_handle()),
        server: Rc::downgrade(server),
        events: RefCell::new(server.borrow().book().session().workbook_events_switch()),
    });
    let model = Rc::new(calc_host::automation::WorkbookAutomation::new(host));
    projection.bind_model(model.clone())?;
    let observer = model.observer(projection);
    server
        .borrow_mut()
        .set_workbook_event_observer(Some(observer.clone()));
    Ok((model, observer))
}

/// What the command line asks of app mode.
struct Options {
    /// Navigate here (the harness's bundle server), instead of mapping `assets`.
    url: Option<String>,
    /// The built page (`cargo xtask app bundle calc`), mapped as `https://dnacalc.app/`.
    assets: Option<PathBuf>,
    xlls: Vec<PathBuf>,
    debug_port: Option<u16>,
    size: (i32, i32),
    /// Write the WebView2 and WebGPU measurement here and exit.
    probe: Option<PathBuf>,
    /// Extra Chromium switches, for measuring a flag.
    webview_args: String,
    hidden: bool,
    /// Save downloads here, without the save dialog (the smoke suite's way to run the save cases).
    save_dir: Option<PathBuf>,
    /// Answer the page's print requests without the native print dialog, which blocks the page
    /// until a person closes it (a headless browser's `print()` also returns at once).
    no_print_ui: bool,
    /// A workbook file to open as the page starts (`--open FILE`, or a path as the first argument).
    open: Option<PathBuf>,
    /// `--trust-macros`: pins the macro security level to Unguarded (dc-15dk).
    trust_macros: bool,
    /// `--macros off|safe|unguarded`: pins the macro security level for this run.
    macros: Option<calc_macro_security::MacroSecurity>,
}

fn options(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        url: None,
        assets: None,
        xlls: Vec::new(),
        debug_port: None,
        size: (1280, 900),
        probe: None,
        webview_args: String::new(),
        hidden: false,
        save_dir: None,
        no_print_ui: false,
        open: None,
        trust_macros: false,
        macros: None,
    };
    let mut args = args.iter().enumerate();
    while let Some((position, arg)) = args.next() {
        let mut value = || {
            args.next()
                .map(|(_, v)| v.clone())
                .ok_or(format!("{arg} needs a value"))
        };
        match arg.as_str() {
            "--app" => {}
            "--url" => o.url = Some(value()?),
            "--assets" => o.assets = Some(PathBuf::from(value()?)),
            "--xll" => o.xlls.push(PathBuf::from(value()?)),
            "--debug-port" => {
                o.debug_port = Some(value()?.parse().map_err(|_| "--debug-port needs a port")?)
            }
            "--probe" => o.probe = Some(PathBuf::from(value()?)),
            "--webview-args" => o.webview_args = value()?,
            "--hidden" => o.hidden = true,
            "--no-print-ui" => o.no_print_ui = true,
            "--save-dir" => o.save_dir = Some(PathBuf::from(value()?)),
            "--open" => o.open = Some(PathBuf::from(value()?)),
            "--trust-macros" => o.trust_macros = true,
            "--macros" => {
                let text = value()?;
                o.macros = Some(
                    calc_macro_security::MacroSecurity::parse(&text)
                        .ok_or(format!("--macros is off, safe or unguarded, not {text}"))?,
                )
            }
            "--size" => {
                let text = value()?;
                let (w, h) = text.split_once('x').ok_or("--size needs WIDTHxHEIGHT")?;
                o.size = (
                    w.parse().map_err(|_| "--size width")?,
                    h.parse().map_err(|_| "--size height")?,
                );
            }
            // A file path as the first argument, as a double-click or a shell association passes it.
            other if position == 0 && !other.starts_with("--") => {
                o.open = Some(PathBuf::from(other))
            }
            other => return Err(format!("unknown app argument: {other}")),
        }
    }
    if o.assets.is_none() && o.url.is_none() && o.probe.is_none() && !crate::embedded::has_page() {
        // A build without its page looks for one in `page` beside the executable.
        o.assets = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|folder| folder.join("page")))
            .filter(|folder| folder.join("index.html").is_file());
    }
    if o.probe.is_none() && o.url.is_none() && o.assets.is_none() && !crate::embedded::has_page() {
        return Err("app mode needs --assets DIR (the built page; or a `page` folder beside the executable) or --url URL: this build carries no page".into());
    }
    Ok(o)
}

/// The timer that drives calculation, native returns, RTD and window size (milliseconds).
const TICK_MS: u32 = 8;
const TICK_ID: usize = 0x444e41;

/// The workbook name the add-ins see.
const WORKBOOK: &str = "Book1";

struct App {
    server: Rc<RefCell<Server>>,
    automation: Rc<calc_host::automation::WorkbookAutomation>,
    automation_observer: Rc<dyn calc_host::WorkbookEventObserver>,
    web: WebView,
    /// The add-in set: what `--xll` loaded at start and what the person loads and unloads in the
    /// page's add-in panel (dc-0f1k.23). It always exists, as OneCalc's does.
    addins: RefCell<AddinHost>,
    /// What each VBA add-in's load reported (its workbook events), shown beside it in the panel.
    addin_notes: RefCell<Vec<(PathBuf, Vec<String>)>>,
    /// The sheets the add-ins were last told about, by identity and name.
    sheets_bound: RefCell<Vec<(calc_value::SheetId, String)>>,
    /// An add-in load or unload is running: the add-in's open routine or the settling loop may
    /// pump messages, and a request that arrives meanwhile must not reach the set again.
    loading: Cell<bool>,
    fetch: Rc<ThreadedFetch>,
    frame: *mut std::ffi::c_void,
    bounds: Cell<(i32, i32)>,
    book: Cell<u64>,
    probe: Option<PathBuf>,
    quit: Cell<bool>,
    /// The add-ins' functions, bound to each new server (a page's load starts a new worker).
    functions: RefCell<Option<calc_host::CallableTable>>,
    /// Further workers the page starts beside its own (the recalculation-contract smoke case
    /// starts one, as a browser page may): each its own server, as each browser worker is its own
    /// module instance. They carry no add-ins, fetch route or RTD. Keyed by the bridge's channel.
    extras: RefCell<std::collections::BTreeMap<u32, Server>>,
    /// The file named on the command line, given to the first worker the page starts.
    opening: RefCell<Option<calc_platform_desktop::origin::OpenedFile>>,
    /// The macro security level in force (dc-15dk), given to every worker the page starts, and the
    /// per-user folder the person's choice is stored in (never a level the flag or the variable pins).
    macro_policy: Cell<calc_macro_security::MacroPolicy>,
    settings: PathBuf,
}

/// The native dialog's choices for Load add-in.
const ADDIN_FILES: [calc_platform_desktop::dialogs::FileFilter<'static>; 2] = [
    calc_platform_desktop::dialogs::FileFilter {
        name: "Excel add-ins (*.xll; *.xlam)",
        patterns: "*.xll;*.xlam",
    },
    calc_platform_desktop::dialogs::FileFilter {
        name: "All files",
        patterns: "*.*",
    },
];

/// The bridge prefixes every message of a worker other than the page's first with `@N` and U+0002
/// (channel N); the first worker's messages carry no prefix.
const CHANNEL_MARK: char = '\u{2}';

fn channel_of(message: &str) -> (u32, &str) {
    message
        .strip_prefix('@')
        .and_then(|rest| rest.split_once(CHANNEL_MARK))
        .and_then(|(channel, rest)| Some((channel.parse().ok()?, rest)))
        .unwrap_or((0, message))
}

/// Stores a changed macro security level in the per-user `settings` folder; a level the flag or the
/// variable pins is shown and never stored. Whether it wrote.
fn persist_macro_policy(
    settings: &std::path::Path,
    previous: calc_macro_security::MacroPolicy,
    now: calc_macro_security::MacroPolicy,
) -> std::io::Result<bool> {
    if now == previous || now.pinned().is_some() {
        return Ok(false);
    }
    calc_platform_desktop::macro_security::save(settings, now.level()).map(|()| true)
}

/// The page starts by asking for an empty workbook (`New`, or `OpenTemplate` with a stored
/// default). When a file was named on the command line, that first request becomes the Open of the
/// file, in the page's own command envelope, so the page installs it as it installs any opened
/// workbook. Returns the request to answer instead, or `None` when `text` is anything else.
fn startup_open(text: &str, opening: &calc_platform_desktop::origin::OpenedFile) -> Option<String> {
    crate::origin::open_request(text, opening, true)
}

/// The file `--open` names: its file name and bytes, read before any window shows.
fn read_open_file(
    path: &std::path::Path,
) -> Result<calc_platform_desktop::origin::OpenedFile, String> {
    calc_platform_desktop::origin::open_file(path).map_err(|e| format!("{}: {e}", path.display()))
}

impl App {
    fn post(&self, text: &str, bytes: Option<&[u8]>) {
        self.post_to(0, text, bytes);
    }

    fn post_to(&self, channel: u32, text: &str, bytes: Option<&[u8]>) {
        let result = if channel == 0 {
            self.web.post(text, bytes)
        } else {
            self.web
                .post(&format!("@{channel}{CHANNEL_MARK}{text}"), bytes)
        };
        if let Err(why) = result {
            eprintln!("dna-calc-desktop: {why}");
        }
    }

    fn post_event(&self, event: Event) {
        self.post_event_to(0, event);
    }

    fn post_event_to(&self, channel: u32, event: Event) {
        match event {
            Event::Tiles(batch) => match wire::encode_tiles(&batch) {
                Ok(bytes) => self.post_to(channel, wire::TILE_TAG, Some(&bytes)),
                Err(message) => self.post_to(
                    channel,
                    &wire::encode(&FromServer::Failed { id: None, message }),
                    None,
                ),
            },
            control => self.post_to(channel, &wire::encode(&FromServer::Event(control)), None),
        }
    }

    /// One request from the page's further worker `channel`.
    fn extra_request(&self, channel: u32, message: &str) {
        if message == "hello" {
            let mut server = Server::new();
            server.set_macro_policy(self.macro_policy.get());
            server.set_calculation_budget(calc_host::WorkBudget::evaluations(64));
            self.extras.borrow_mut().insert(channel, server);
            self.post_to(channel, &wire::encode(&FromServer::Started), None);
            return;
        }
        let (text, bytes) = split_message(message);
        let answered = self.extras.borrow_mut().get_mut(&channel).map(|server| {
            let (reply, out) = server::answer_text(server, text, bytes);
            (reply, out, server.take_events())
        });
        if let Some((reply, out, events)) = answered {
            for event in events {
                self.post_event_to(channel, event);
            }
            self.post_to(channel, &reply, out.as_deref());
        }
    }

    /// The further workers' calculation slices and events, once per tick.
    fn tick_extras(&self) {
        let channels: Vec<u32> = self.extras.borrow().keys().copied().collect();
        for channel in channels {
            let events = {
                let mut extras = self.extras.borrow_mut();
                let Some(server) = extras.get_mut(&channel) else {
                    continue;
                };
                let started = Instant::now();
                while server.has_pending_calculation()
                    && started.elapsed() < Duration::from_millis(6)
                {
                    server.advance_calculation();
                }
                server.take_events()
            };
            for event in events {
                self.post_event_to(channel, event);
            }
        }
    }

    fn flush_events(&self) {
        let events = self.server.borrow_mut().take_events();
        for event in events {
            self.post_event(event);
        }
    }

    /// One request from the page, answered as the browser worker answers it: events first, then
    /// the reply.
    fn request(&self, message: &str) {
        let (channel, message) = channel_of(message);
        if channel != 0 {
            return self.extra_request(channel, message);
        }
        if message == "hello" {
            // A page that loads starts its own worker, as in a browser: a new server. Workers the
            // previous page started are gone with it.
            self.extras.borrow_mut().clear();
            let mut server = Server::new();
            server.set_workbook_event_observer(Some(self.automation_observer.clone()));
            server.set_macro_policy(self.macro_policy.get());
            server.set_calculation_budget(calc_host::WorkBudget::evaluations(64));
            server.set_fetch_route(self.fetch.clone());
            server.set_import_route(Rc::new(LocalFiles));
            if let Some(functions) = self.functions.borrow().clone() {
                server.set_user_functions(functions);
            }
            self.book.set(server.workbook_id());
            *self.server.borrow_mut() = server;
            self.bind_sheets();
            self.post(&wire::encode(&FromServer::Started), None);
            return;
        }
        if let Some(json) = message.strip_prefix("probe:") {
            if let Some(path) = &self.probe {
                if let Err(why) = std::fs::write(path, json) {
                    eprintln!("dna-calc-desktop: cannot write the probe: {why}");
                }
            }
            self.quit.set(true);
            return;
        }
        let (text, bytes) = split_message(message);
        if bytes.is_none()
            && let Some(reply) = self.addin_request(text)
        {
            self.post(&reply, None);
            self.flush_events();
            return;
        }
        let mut text = text.to_owned();
        let mut bytes = bytes;
        let mut open_origin = None;
        let mut open_location = None;
        let substitute = self.opening.borrow().as_ref().and_then(|opening| {
            startup_open(&text, opening).map(|open| {
                (
                    open,
                    opening.bytes.clone(),
                    opening.origin,
                    opening.path.to_string_lossy().into_owned(),
                )
            })
        });
        if let Some((open, package, origin, location)) = substitute {
            self.opening.borrow_mut().take();
            open_origin = Some(origin);
            open_location = Some(location);
            text = open;
            bytes = Some(package);
        }
        if let Ok(message) = wire::decode::<wire::ToServer>(&text) {
            if matches!(&message.request, wire::Request::Command(envelope)
                if matches!(*envelope.request, wire::Request::OpenNative))
            {
                let selected = calc_platform_desktop::dialogs::open_file(
                    self.frame,
                    "Open a workbook",
                    &[calc_platform_desktop::dialogs::FileFilter {
                        name: "Excel workbooks",
                        patterns: "*.xlsx;*.xlsm;*.xlsb;*.xltx;*.xltm",
                    }],
                )
                .and_then(|path| path.map(|path| read_open_file(&path)).transpose());
                match selected {
                    Ok(Some(opened)) => {
                        text = crate::origin::open_request(&text, &opened, false)
                            .expect("checked native open");
                        open_origin = Some(opened.origin);
                        open_location = Some(opened.path.to_string_lossy().into_owned());
                        bytes = Some(opened.bytes);
                    }
                    Ok(None) => {
                        self.post(
                            &wire::encode(&FromServer::Reply {
                                id: message.id,
                                reply: wire::Reply::OpenCancelled,
                            }),
                            None,
                        );
                        return;
                    }
                    Err(message_text) => {
                        self.post(
                            &wire::encode(&FromServer::Failed {
                                id: Some(message.id),
                                message: message_text,
                            }),
                            None,
                        );
                        return;
                    }
                }
            }
        }
        let text = text.as_str();
        let (reply, out, events) = {
            let mut server = self.server.borrow_mut();
            server.set_open_file_location(open_location);
            let (reply, out) = match open_origin {
                Some(origin) => server
                    .with_open_origin(origin, |server| server::answer_text(server, text, bytes)),
                None => server::answer_text(&mut server, text, bytes),
            };
            self.store_macro_policy(server.macro_policy());
            (reply, out, server.take_events())
        };
        for event in events {
            self.post_event(event);
        }
        self.post(&reply, out.as_deref());
        self.rebind_after_open();
    }

    /// The page's add-in requests (dc-0f1k.23), answered here over the add-in set: list, load
    /// (the native dialog or a typed path) and unload. `None` for any other request, which the
    /// server answers.
    fn addin_request(&self, text: &str) -> Option<String> {
        use dna_calc_worker::wire::{Reply, Request, ToServer};
        let message = wire::decode::<ToServer>(text).ok()?;
        let answer = match message.request {
            Request::Addins => Ok(self.addin_list(false)),
            Request::LoadAddinNative | Request::LoadAddin { .. } | Request::UnloadAddin { .. }
                if self.loading.get() =>
            {
                Err("An add-in is loading; try again when it has".to_owned())
            }
            Request::LoadAddinNative => {
                match calc_platform_desktop::dialogs::open_file(
                    self.frame.cast(),
                    "Load an add-in",
                    &ADDIN_FILES,
                ) {
                    Ok(Some(path)) => self.load_addin(&path),
                    Ok(None) => Ok(self.addin_list(true)),
                    Err(why) => Err(why),
                }
            }
            Request::LoadAddin { path } => self.load_addin(std::path::Path::new(&path)),
            Request::UnloadAddin { path } => self.unload_addin(std::path::Path::new(&path)),
            _ => return None,
        };
        let id = message.id;
        Some(wire::encode(&match answer {
            Ok(list) => FromServer::Reply {
                id,
                reply: Reply::Addins(list),
            },
            Err(message) => FromServer::Failed {
                id: Some(id),
                message,
            },
        }))
    }

    /// What the add-in set holds, as the panel lists it.
    fn addin_list(&self, cancelled: bool) -> wire::AddinList {
        let set = self.addins.borrow();
        let notes = self.addin_notes.borrow();
        let xlls = set
            .xll_functions()
            .into_iter()
            .map(|(path, functions)| (path, "xll", functions));
        let xlams = set
            .vba_addins()
            .into_iter()
            .map(|(path, functions)| (path, "xlam", functions));
        wire::AddinList {
            native: true,
            cancelled,
            loaded: xlls
                .chain(xlams)
                .map(|(path, kind, functions)| wire::LoadedAddin {
                    kind: kind.to_owned(),
                    notes: notes
                        .iter()
                        .find(|(held, _)| *held == path)
                        .map(|(_, notes)| notes.clone())
                        .unwrap_or_default(),
                    path: path.display().to_string(),
                    functions,
                })
                .collect(),
        }
    }

    fn loaded_paths(&self) -> Vec<PathBuf> {
        let set = self.addins.borrow();
        set.xlls()
            .iter()
            .cloned()
            .chain(set.vba_addins().into_iter().map(|(path, _)| path))
            .collect()
    }

    /// Loads an .xll or a VBA .xlam (installed, as Excel's Add-ins dialog installs one), lets a
    /// slow add-in finish registering, and publishes every loaded function to the workbook.
    fn load_addin(&self, path: &std::path::Path) -> Result<wire::AddinList, String> {
        let extension = path
            .extension()
            .map(|extension| extension.to_string_lossy().to_ascii_lowercase());
        if !matches!(extension.as_deref(), Some("xll" | "xlam")) {
            return Err(format!(
                "{} is not an .xll or .xlam add-in; only those load here",
                path.display()
            ));
        }
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
        if !path.is_file() {
            return Err(format!("{}: no such file", path.display()));
        }
        if self.loaded_paths().contains(&path) {
            return Err(format!("{} is already loaded", path.display()));
        }
        self.loading.set(true);
        let loaded = (|| {
            let notes = crate::load_addin(&mut self.addins.borrow_mut(), &path)?;
            if extension.as_deref() == Some("xll") {
                self.addins
                    .borrow()
                    .settle(Duration::from_millis(800), Duration::from_secs(60));
            } else {
                self.addin_notes.borrow_mut().push((path.clone(), notes));
            }
            Ok::<(), String>(())
        })();
        self.loading.set(false);
        loaded?;
        self.publish_addins()?;
        Ok(self.addin_list(false))
    }

    fn unload_addin(&self, path: &std::path::Path) -> Result<wire::AddinList, String> {
        let path = path.to_owned();
        let is_xlam = self
            .addins
            .borrow()
            .vba_addins()
            .iter()
            .any(|(held, _)| *held == path);
        if is_xlam {
            self.addins.borrow_mut().unload_vba_addin(&path);
            self.addin_notes
                .borrow_mut()
                .retain(|(held, _)| *held != path);
        } else if self.addins.borrow().xlls().contains(&path) {
            self.loading.set(true);
            let unloaded = self.addins.borrow_mut().unload(&path);
            self.loading.set(false);
            unloaded?;
        } else {
            return Err(format!("{} is not loaded", path.display()));
        }
        self.publish_addins()?;
        Ok(self.addin_list(false))
    }

    /// The add-in set's functions, as one new table, to this and every later worker; the open
    /// workbook recalculates.
    fn publish_addins(&self) -> Result<(), String> {
        let table = self.addins.borrow().callables()?;
        *self.functions.borrow_mut() = Some(table.clone());
        self.server.borrow_mut().replace_user_functions(table);
        self.bind_sheets();
        Ok(())
    }

    /// Stores the macro security level the person chose in the page, per user (dc-15dk).
    fn store_macro_policy(&self, now: calc_macro_security::MacroPolicy) {
        let previous = self.macro_policy.replace(now);
        if let Err(why) = persist_macro_policy(&self.settings, previous, now) {
            eprintln!("dna-calc-desktop: the macro security level is not stored: {why}");
        }
    }

    /// A newly opened workbook has new sheet identities, and a renamed or added sheet a new name:
    /// the native host names them.
    fn rebind_after_open(&self) {
        let id = self.server.borrow().workbook_id();
        let names: Vec<(calc_value::SheetId, String)> = self
            .server
            .borrow()
            .book()
            .host()
            .sheets()
            .iter()
            .map(|sheet| {
                (
                    sheet.id(),
                    sheet.name().text().to_string_lossy().to_string(),
                )
            })
            .collect();
        let renamed = *self.sheets_bound.borrow() != names;
        if self.book.replace(id) != id || renamed {
            *self.sheets_bound.borrow_mut() = names;
            self.bind_sheets();
        }
    }

    fn bind_sheets(&self) {
        self.automation
            .context_changed(self.server.borrow().book().session());
    }

    /// The fetches that finished: each resumes its waiting cell.
    fn fetched(&self) {
        for (request, kind, reply) in self.fetch.take() {
            self.server
                .borrow_mut()
                .complete_fetch(request, reply.into_value(kind));
        }
        self.flush_events();
    }

    /// Native returns, RTD, calculation and the window's size, once per tick.
    fn tick(&self) {
        if let Some((width, height)) = calc_platform_desktop::client_size(self.frame) {
            if (width, height) != self.bounds.get() && width > 0 && height > 0 {
                self.bounds.set((width, height));
                self.web.set_bounds(width, height);
            }
        }
        if let Ok(addins) = self.addins.try_borrow() {
            for returned in addins.take_async_returns() {
                let mut server = self.server.borrow_mut();
                let token = server
                    .pending_async()
                    .into_iter()
                    .find(|token| token.request == returned.request);
                if let Some(token) = token {
                    let value = returned.value.unwrap_or_else(|_| {
                        calc_value::CalcValue::error(calc_value::WorksheetErrorCode::Value)
                    });
                    server.complete_async(&token, value);
                }
            }
            let evaluating = self.server.borrow().has_pending_calculation();
            for input in addins.poll_rtd(evaluating).unwrap_or_default() {
                self.server.borrow_mut().update_rtd(
                    input.topic,
                    input.lease,
                    input.sequence,
                    input.value,
                );
            }
        }
        let started = Instant::now();
        while self.server.borrow().has_pending_calculation()
            && started.elapsed() < Duration::from_millis(6)
        {
            self.server.borrow_mut().advance_calculation();
        }
        self.flush_events();
        self.tick_extras();
    }
}

/// The browser profile when `DNACALC_WEBVIEW2_DATA` names none: `%LOCALAPPDATA%\DNA Calc\webview2`,
/// else a folder under the temporary directory.
fn default_data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .filter(|v| !v.is_empty())
        .map(|base| PathBuf::from(base).join("DNA Calc").join("webview2"))
        .unwrap_or_else(|| std::env::temp_dir().join("dnacalc-webview2"))
}

pub fn run(args: &[String]) -> Result<(), String> {
    let o = options(args)?;
    let opening = o.open.as_deref().map(read_open_file).transpose()?;
    // WebView2 is the app's window: say clearly what is missing and where to get it.
    if o.probe.is_none() && calc_platform_desktop::runtime_version().is_none() {
        let text = crate::embedded::missing_runtime_message();
        eprintln!("dna-calc-desktop: {text}");
        if crate::embedded::has_page()
            && calc_platform_desktop::dialogs::message(
                std::ptr::null_mut(),
                "DNA Calc",
                &format!(
                    "{text}

Open the download page now?"
                ),
                calc_platform_desktop::dialogs::Buttons::YesNo,
                false,
            ) == calc_platform_desktop::dialogs::Pressed::Yes
        {
            calc_platform_desktop::dialogs::open_url(calc_platform_desktop::RUNTIME_DOWNLOAD);
        }
        return Err(text);
    }
    // Per-monitor awareness: the page renders at the display's real scale.
    per_monitor_dpi_aware();
    let shell = Shell::new("DNA Calc")?;
    let frame = shell.workbook_hwnd();
    shell.show(o.size.0, o.size.1);
    let user_data = std::env::var_os("DNACALC_WEBVIEW2_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(default_data_dir);
    let mut browser_args = o.webview_args.clone();
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
    if o.no_print_ui {
        web.add_script("window.__dnacalcNoPrintUi = true;")?;
    }
    web.add_script(include_str!("../../dna-calc/web/desktop-bridge.js"))?;
    if let Some(folder) = &o.save_dir {
        std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        web.save_downloads_to(folder)?;
    }

    // The native worker: the portable server, with the desktop's fetch route and add-ins. It reads
    // the day from the machine's clock, as the browser worker reads it from the browser's.
    dna_calc_worker::workbook::set_native_clock(calc_platform_desktop::local_serial_clock);
    let settings = calc_platform_desktop::macro_security::default_folder();
    let macro_policy =
        calc_platform_desktop::macro_security::policy(&settings, o.trust_macros, o.macros);
    let mut server = Server::new();
    server.set_macro_policy(macro_policy);
    server.set_calculation_budget(calc_host::WorkBudget::evaluations(64));
    let fetch = Rc::new(ThreadedFetch::new());
    server.set_fetch_route(fetch.clone());
    server.set_import_route(Rc::new(LocalFiles));
    // The add-in set always exists: the page loads add-ins into it whenever the person asks.
    let mut functions = None;
    let automation;
    let automation_observer;
    let server = Rc::new(RefCell::new(server));
    let addins = {
        let sheets: Vec<(calc_value::SheetId, String)> = server
            .borrow()
            .book()
            .host()
            .sheets()
            .iter()
            .map(|sheet| {
                (
                    sheet.id(),
                    sheet.name().text().to_string_lossy().to_string(),
                )
            })
            .collect();
        let sheet_refs: Vec<_> = sheets
            .iter()
            .map(|(id, name)| (*id, name.as_str()))
            .collect();
        let mut set = AddinHost::new(
            &shell,
            &Document {
                workbook: WORKBOOK,
                active_sheet: None,
                sheets: &sheet_refs,
            },
        )?;
        (automation, automation_observer) = bind_automation(&set, &server)?;
        for xll in &o.xlls {
            for note in crate::load_addin(&mut set, xll)? {
                eprintln!("dna-calc-desktop: {}: {note}", xll.display());
            }
        }
        set.enable_async_continuations()?;
        let rtd = set.enable_rtd()?;
        server.borrow_mut().set_rtd_connector(rtd);
        if !o.xlls.is_empty() {
            let table = set.callables()?;
            server.borrow_mut().set_user_functions(table.clone());
            functions = Some(table);
        }
        set
    };
    // Declared after the shell so that it drops first: the add-ins close while their windows exist.
    let app = Rc::new(App {
        book: Cell::new(server.borrow().workbook_id()),
        server: server.clone(),
        automation,
        automation_observer,
        web,
        addins: RefCell::new(addins),
        addin_notes: RefCell::new(Vec::new()),
        loading: Cell::new(false),
        sheets_bound: RefCell::new(Vec::new()),
        fetch,
        frame,
        bounds: Cell::new(o.size),
        probe: o.probe.clone(),
        quit: Cell::new(false),
        functions: RefCell::new(functions),
        extras: RefCell::new(Default::default()),
        opening: RefCell::new(opening),
        macro_policy: Cell::new(macro_policy),
        settings,
    });
    {
        let weak = Rc::downgrade(&app);
        app.web.on_download_completed(move |proposed| {
            let app = weak.upgrade()?;
            let server = app.server.try_borrow().ok()?;
            let expected = server.book().save_name();
            if calc_platform_desktop::proposed_name(proposed)?.to_string_lossy() != expected {
                return None;
            }
            let workbook = server.workbook_id();
            let weak = Rc::downgrade(&app);
            Some(Rc::new(move |path: &std::path::Path| {
                if let Some(app) = weak.upgrade() {
                    if let Ok(mut server) = app.server.try_borrow_mut() {
                        if server.workbook_id() == workbook && server.book().save_name() == expected
                        {
                            server.set_file_location(path.to_string_lossy().into_owned());
                        }
                    }
                }
            }) as Rc<dyn Fn(&std::path::Path)>)
        })?;
    }
    {
        // The page's messages must not keep the app alive: a weak handle lets it drop at exit.
        let weak = Rc::downgrade(&app);
        app.web.on_message(move |message| {
            if let Some(app) = weak.upgrade() {
                app.request(&message);
            }
        })?;
    }
    // Where the page comes from: the harness's server, a mapped folder, or the probe page.
    let target = if o.probe.is_some() {
        let folder = std::env::temp_dir().join("dnacalc-probe");
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        std::fs::write(
            folder.join("webgpu.html"),
            include_str!("../probe/webgpu.html"),
        )
        .map_err(|e| e.to_string())?;
        app.web.map_folder("dnacalc.app", &folder)?;
        "https://dnacalc.app/webgpu.html".to_owned()
    } else if let Some(url) = &o.url {
        url.clone()
    } else if let Some(assets) = &o.assets {
        app.web.map_folder("dnacalc.app", assets)?;
        "https://dnacalc.app/index.html".to_owned()
    } else {
        // The page this executable carries, answered from memory at the same origin.
        let page = crate::embedded::page()?;
        app.web.serve_assets("dnacalc.app", Rc::new(page))?;
        "https://dnacalc.app/index.html".to_owned()
    };
    app.bind_sheets();
    app.web.navigate(&target)?;
    let _ = o.hidden;
    let timer = ThreadTimer::start(TICK_ID, TICK_MS);
    // The loop ends when the page asks (the probe) or when the person closes the window.
    run_message_loop(
        || app.quit.get() || calc_platform_desktop::client_size(frame).is_none(),
        |message| match message {
            ThreadMessage::Timer(TICK_ID) => app.tick(),
            ThreadMessage::Background => app.fetched(),
            _ => {}
        },
    );
    drop(timer);
    drop(app);
    drop(shell);
    Ok(())
}

#[cfg(test)]
mod tests {
    /// Native-only: the desktop host shares the session switch even while the server is borrowed.
    #[test]
    fn automation_host_enable_events_uses_the_session_switch_during_callbacks() {
        use calc_host::automation::AutomationHost;
        use calc_object_model::profile::ApplicationHost;
        let application =
            calc_com_host::Projection::new(1, Rc::new(calc_xll_host::XllHost::new(1))).unwrap();
        let application = application.application_host();
        application.set_events_enabled(true);
        let server = Rc::new(RefCell::new(Server::new()));
        let events = server.borrow().book().session().workbook_events_switch();
        let host = DesktopAutomationHost {
            application: application.clone(),
            native: std::rc::Weak::new(),
            server: Rc::downgrade(&server),
            events: RefCell::new(events.clone()),
        };
        let callback = server.borrow_mut();
        callback
            .book()
            .session()
            .workbook_events_switch()
            .set(false);
        assert!(!host.events_enabled());
        host.set_events_enabled(true);
        assert!(callback.book().session().workbook_events_enabled());
        assert!(application.events_enabled());
        host.set_events_enabled(false);
        assert!(!callback.book().session().workbook_events_enabled());
        assert!(
            application.events_enabled(),
            "DNA Calc never writes the values-only host switch"
        );
        drop(callback);
        let replacement = Server::new();
        host.context_changed(replacement.book().session());
        assert!(host.events_enabled());
        host.set_events_enabled(false);
        assert!(!replacement.book().session().workbook_events_enabled());
        assert!(!events.get());
    }

    /// Native-only: creates the native projection and server used by the page.
    #[test]
    fn object_model_follows_the_page_session_selection_identity_and_references() {
        use calc_object_model::{AutoValue, Call, Descriptor, InvokeKind, ObjectModel};
        let shell = calc_platform_desktop::Shell::new("DNA Calc object model test").unwrap();
        let addins = calc_addin_host::AddinHost::new(
            &shell,
            &calc_addin_host::Document {
                workbook: "Book1",
                active_sheet: None,
                sheets: &[],
            },
        )
        .unwrap();
        let server = Rc::new(RefCell::new(dna_calc_worker::server::Server::new()));
        let (model, _observer) = super::bind_automation(&addins, &server).unwrap();
        let get = |object: &calc_object_model::ObjectRef, name: &str, args| {
            model
                .invoke(
                    object,
                    &Call {
                        interface: object.interface,
                        member: Descriptor::get()
                            .member(object.interface, name, InvokeKind::Get)
                            .unwrap(),
                        args,
                        lcid: 0,
                    },
                )
                .unwrap()
        };
        let object = |value| {
            if let AutoValue::Object(object) = value {
                object
            } else {
                panic!("object");
            }
        };
        let app = model.application();
        let sheet = server.borrow().book().host().sheets()[0].id();
        server
            .borrow_mut()
            .book_mut()
            .saved_as("Named.xlsx".into(), calc_host::SpreadsheetFormat::Xlsx);
        server
            .borrow_mut()
            .set_file_location(r"C:\Models\Named.xlsx".into());
        let book = object(get(&app, "ActiveWorkbook", vec![]));
        assert_eq!(
            get(&book, "FullName", vec![]).as_text().as_deref(),
            Some(r"C:\Models\Named.xlsx")
        );
        let at = calc_value::CellKey::new(sheet, 1, 1).unwrap();
        assert!(
            server
                .borrow_mut()
                .book_mut()
                .dispatch(calc_host::SkinIntent::EnterCell {
                    at,
                    text: "7".into()
                })
                .accepted()
        );
        let range = object(get(&app, "Range", vec![AutoValue::text("A1")]));
        assert!(matches!(get(&range, "Value2", vec![]), AutoValue::R8(7.)));
        assert!(
            server
                .borrow_mut()
                .book_mut()
                .dispatch(calc_host::SkinIntent::EditAxis(
                    calc_host::AxisEdit::new(
                        sheet,
                        calc_host::Axis::Rows,
                        calc_host::AxisChange::Insert,
                        1,
                        1
                    )
                    .unwrap()
                ))
                .accepted()
        );
        assert_eq!(
            get(&range, "Address", vec![]).as_text().as_deref(),
            Some("$A$2")
        );
        assert!(
            server
                .borrow_mut()
                .book_mut()
                .dispatch(calc_host::SkinIntent::Command(
                    calc_host::WorkbookCommand::Sheet {
                        sheet: None,
                        change: calc_host::SheetChange::Add {
                            name: "Other".into()
                        }
                    }
                ))
                .accepted()
        );
        let other = server
            .borrow()
            .book()
            .host()
            .sheets()
            .iter()
            .find(|s| s.name().text().to_string_lossy() == "Other")
            .unwrap()
            .id();
        let active = calc_value::CellKey::new(other, 3, 2).unwrap();
        assert!(
            server
                .borrow_mut()
                .book_mut()
                .dispatch(calc_host::SkinIntent::Select(calc_host::Selection {
                    active: Some(active),
                    areas: vec![calc_value::GridArea::cell(active)]
                }))
                .accepted()
        );
        let active_sheet = object(get(&app, "ActiveSheet", vec![]));
        assert_eq!(
            get(&active_sheet, "Name", vec![]).as_text().as_deref(),
            Some("Other")
        );
        assert!(
            server
                .borrow_mut()
                .execute_current(dna_calc_worker::wire::Request::New, None)
                .reply
                .is_ok()
        );
        let member = Descriptor::get()
            .member(range.interface, "Address", InvokeKind::Get)
            .unwrap();
        assert!(
            model
                .invoke(
                    &range,
                    &Call {
                        interface: range.interface,
                        member,
                        args: vec![],
                        lcid: 0
                    }
                )
                .is_err()
        );
        model.released(&range);
    }
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
        assert!(options(&args(&["--app", "--probe", "p.json"])).is_ok());
        assert!(options(&args(&["--app", "--bogus"])).is_err());
    }

    /// Native-only: `--open FILE` and a file path as the first argument both name the workbook
    /// to open; a path anywhere else is still an unknown argument, and `--open` needs a value.
    #[test]
    fn open_is_a_flag_or_the_first_argument() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let flag = options(&args(&["--open", "a.xlsx", "--assets", "dist"])).unwrap();
        assert_eq!(flag.open.as_deref(), Some(std::path::Path::new("a.xlsx")));
        let first = options(&args(&["b.xlsx", "--assets", "dist"])).unwrap();
        assert_eq!(first.open.as_deref(), Some(std::path::Path::new("b.xlsx")));
        let after_app = options(&args(&["--app", "--assets", "dist", "c.xlsx"]));
        assert!(
            after_app.is_err(),
            "a later bare argument is not a file to open"
        );
        assert!(options(&args(&["--open"])).is_err());
        assert!(
            options(&args(&["--assets", "dist"]))
                .unwrap()
                .open
                .is_none()
        );
    }

    /// Native-only: the page's first request for an empty workbook (New, or the stored default
    /// template) becomes the Open of the file named on the command line, in the page's own command
    /// envelope; the server answers it as an open, and no other request is touched.
    #[test]
    fn the_pages_startup_request_opens_the_named_file() {
        use dna_calc_worker::wire::{Request, ToServer};
        let bytes = include_bytes!("../../../crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx");
        let folder = std::env::temp_dir().join(format!("dnacalc-open-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("named.xlsx");
        std::fs::write(&file, bytes).unwrap();
        let opening = read_open_file(&file).unwrap();
        assert_eq!(
            (opening.name.as_str(), opening.bytes.len()),
            ("named.xlsx", bytes.len())
        );
        assert!(read_open_file(&folder.join("missing.xlsx")).is_err());
        std::fs::remove_dir_all(&folder).unwrap();

        // The person's macro security choice is stored per user; a pinned level is not (dc-15dk).
        {
            use calc_macro_security::{MacroPolicy, MacroSecurity};
            let settings =
                std::env::temp_dir().join(format!("dna-calc-macro-level-{}", std::process::id()));
            let ask = MacroPolicy::stored(MacroSecurity::Off);
            let none = MacroPolicy::stored(MacroSecurity::Safe);
            assert!(!persist_macro_policy(&settings, ask, ask).unwrap());
            assert!(persist_macro_policy(&settings, ask, none).unwrap());
            assert_eq!(
                calc_platform_desktop::macro_security::load(&settings),
                MacroSecurity::Safe
            );
            let pinned = MacroPolicy::resolve(MacroSecurity::Off, true, None, None);
            assert!(!persist_macro_policy(&settings, none, pinned).unwrap());
            assert_eq!(
                calc_platform_desktop::macro_security::load(&settings),
                MacroSecurity::Safe
            );
            std::fs::remove_dir_all(&settings).unwrap();
        }
        let mut server = Server::new();
        let blank = server.workbook_id();
        let page = |server: &Server, request: Request| {
            wire::encode(&ToServer {
                id: 1,
                request: server.envelope(request),
            })
        };
        // A request that is not the startup one is left alone.
        assert!(
            startup_open(
                &wire::encode(&ToServer {
                    id: 2,
                    request: Request::Summary
                }),
                &opening
            )
            .is_none()
        );
        assert!(
            startup_open(
                &page(
                    &server,
                    Request::OpenForEditing {
                        name: "t.xltx".into()
                    }
                ),
                &opening
            )
            .is_none()
        );
        assert!(startup_open("not json", &opening).is_none());
        for startup in [
            Request::New,
            Request::OpenTemplate {
                name: "Book.xltx".into(),
            },
        ] {
            let text = startup_open(&page(&server, startup), &opening).expect("substituted");
            let decoded: ToServer = wire::decode(&text).unwrap();
            match decoded.request {
                Request::Command(envelope) => {
                    assert_eq!(
                        *envelope.request,
                        Request::Open {
                            name: "named.xlsx".into()
                        }
                    );
                    assert_eq!(envelope.id, 1, "the page's own command identity is kept");
                }
                other => panic!("not a command: {other:?}"),
            }
        }
        let text = startup_open(&page(&server, Request::New), &opening).unwrap();
        let (reply, _) = server::answer_text(&mut server, &text, Some(opening.bytes.clone()));
        let reply: FromServer = wire::decode(&reply).unwrap();
        match reply {
            FromServer::Reply {
                reply: wire::Reply::Done {
                    outcome, opened, ..
                },
                ..
            } => {
                assert_eq!(outcome.error, None);
                assert!(opened.is_some(), "an open, not a new workbook");
            }
            other => panic!("not an open: {other:?}"),
        }
        assert_ne!(
            server.workbook_id(),
            blank,
            "the file replaced the blank workbook"
        );
        let reference = {
            let mut other = Server::new();
            let answer = other.execute_current(
                Request::Open {
                    name: "x.xlsx".into(),
                },
                Some(bytes.to_vec()),
            );
            assert!(answer.reply.is_ok());
            other.book().host().sheets().len()
        };
        assert_eq!(server.book().host().sheets().len(), reference);
    }
}
