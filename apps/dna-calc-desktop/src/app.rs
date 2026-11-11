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
    server: RefCell<Server>,
    web: WebView,
    addins: Option<AddinHost>,
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
    opening: RefCell<Option<(String, Vec<u8>)>>,
}

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

/// The page starts by asking for an empty workbook (`New`, or `OpenTemplate` with a stored
/// default). When a file was named on the command line, that first request becomes the Open of the
/// file, in the page's own command envelope, so the page installs it as it installs any opened
/// workbook. Returns the request to answer instead, or `None` when `text` is anything else.
fn startup_open(text: &str, opening: &(String, Vec<u8>)) -> Option<String> {
    use dna_calc_worker::wire::{Request, ToServer};
    let mut message = wire::decode::<ToServer>(text).ok()?;
    let Request::Command(envelope) = &mut message.request else {
        return None;
    };
    if !matches!(
        *envelope.request,
        Request::New | Request::OpenTemplate { .. }
    ) {
        return None;
    }
    *envelope.request = Request::Open {
        name: opening.0.clone(),
    };
    Some(wire::encode(&message))
}

/// The file `--open` names: its file name and bytes, read before any window shows.
fn read_open_file(path: &std::path::Path) -> Result<(String, Vec<u8>), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| format!("{}: not a file", path.display()))?;
    Ok((name, bytes))
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
        let mut text = text.to_owned();
        let mut bytes = bytes;
        let substitute =
            self.opening.borrow().as_ref().and_then(|opening| {
                startup_open(&text, opening).map(|open| (open, opening.1.clone()))
            });
        if let Some((open, package)) = substitute {
            self.opening.borrow_mut().take();
            text = open;
            bytes = Some(package);
        }
        let text = text.as_str();
        let (reply, out, events) = {
            let mut server = self.server.borrow_mut();
            let (reply, out) = server::answer_text(&mut server, text, bytes);
            (reply, out, server.take_events())
        };
        for event in events {
            self.post_event(event);
        }
        self.post(&reply, out.as_deref());
        self.rebind_after_open();
    }

    /// A newly opened workbook has new sheet identities: the native host names them.
    fn rebind_after_open(&self) {
        let id = self.server.borrow().workbook_id();
        if self.book.replace(id) != id {
            self.bind_sheets();
        }
    }

    fn bind_sheets(&self) {
        let Some(addins) = &self.addins else { return };
        let server = self.server.borrow();
        for sheet in server.book().host().sheets() {
            addins.bind_sheet(sheet.id(), &sheet.name().text().to_string_lossy());
        }
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
        if let Some(addins) = &self.addins {
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
        .unwrap_or_else(|| std::env::temp_dir().join("dnacalc-webview2"));
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
    let mut server = Server::new();
    server.set_calculation_budget(calc_host::WorkBudget::evaluations(64));
    let fetch = Rc::new(ThreadedFetch::new());
    server.set_fetch_route(fetch.clone());
    server.set_import_route(Rc::new(LocalFiles));
    let (mut addins, mut functions) = (None, None);
    if !o.xlls.is_empty() {
        let sheets: Vec<(calc_value::SheetId, String)> = server
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
        for xll in &o.xlls {
            for note in crate::load_addin(&mut set, xll)? {
                eprintln!("dna-calc-desktop: {}: {note}", xll.display());
            }
        }
        set.enable_async_continuations()?;
        let rtd = set.enable_rtd()?;
        let table = set.callables()?;
        server.set_user_functions(table.clone());
        server.set_rtd_connector(rtd);
        functions = Some(table);
        addins = Some(set);
    }
    // Declared after the shell so that it drops first: the add-ins close while their windows exist.
    let app = Rc::new(App {
        book: Cell::new(server.workbook_id()),
        server: RefCell::new(server),
        web,
        addins,
        fetch,
        frame,
        bounds: Cell::new(o.size),
        probe: o.probe.clone(),
        quit: Cell::new(false),
        functions: RefCell::new(functions),
        extras: RefCell::new(Default::default()),
        opening: RefCell::new(opening),
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
            (opening.0.as_str(), opening.1.len()),
            ("named.xlsx", bytes.len())
        );
        assert!(read_open_file(&folder.join("missing.xlsx")).is_err());
        std::fs::remove_dir_all(&folder).unwrap();

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
        let (reply, _) = server::answer_text(&mut server, &text, Some(opening.1.clone()));
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
