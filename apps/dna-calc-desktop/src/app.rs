//! App mode (`dna-calc-desktop --app`): the same page the browser runs, hosted in WebView2, with
//! the workbook worker native behind the page's existing protocol (M2 of the grid design).
//!
//! The shell is skin deep. The page, the protocol and `dna_calc_worker::server::Server` are the
//! ones the browser runs; the shell only carries the page's JSON texts and tile bytes between the
//! page and a `Server` on this STA thread (`apps/dna-calc/web/desktop-bridge.js` is the page side),
//! and gives that server what a browser worker cannot have: real XLL user functions, COM and RTD,
//! and a fetch route without a cross-origin restriction. No behaviour lives here that a native UI
//! would have to redo.
use crate::{
    fetch::{ThreadedFetch, WM_FETCH},
    shell::Shell,
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
use std::result::Result;
use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::{
    Win32::Foundation::HWND as WinHwnd,
    core::{Interface, PCWSTR, PWSTR},
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
    };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = || args.next().cloned().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--app" => {}
            "--url" => o.url = Some(value()?),
            "--assets" => o.assets = Some(PathBuf::from(value()?)),
            "--xll" => o.xlls.push(PathBuf::from(value()?)),
            "--debug-port" => o.debug_port = Some(value()?.parse().map_err(|_| "--debug-port needs a port")?),
            "--probe" => o.probe = Some(PathBuf::from(value()?)),
            "--webview-args" => o.webview_args = value()?,
            "--hidden" => o.hidden = true,
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
    if o.probe.is_none() && o.url.is_none() && o.assets.is_none() {
        return Err("app mode needs --assets DIR (the built page) or --url URL".into());
    }
    Ok(o)
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(BASE64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The value of each base64 digit, 255 for a byte that is none.
const DIGITS: [u8; 256] = {
    let mut table = [255u8; 256];
    let mut i = 0;
    while i < 64 {
        table[BASE64[i] as usize] = i as u8;
        i += 1;
    }
    table
};

fn unbase64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut bits, mut have) = (0u32, 0);
    for c in text.bytes().take_while(|&c| c != b'=') {
        let v = DIGITS[c as usize];
        if v == 255 {
            return None;
        }
        bits = (bits << 6) | u32::from(v);
        have += 6;
        if have >= 8 {
            have -= 8;
            out.push((bits >> have) as u8);
            bits &= (1 << have) - 1;
        }
    }
    Some(out)
}

/// The separator between a message's text and its base64 bytes (see desktop-bridge.js).
const SEPARATOR: char = '\u{1}';

/// The timer that drives calculation, native returns, RTD and window size (milliseconds).
const TICK_MS: u32 = 8;
const TICK_ID: usize = 0x444e41;

struct App {
    server: RefCell<Server>,
    view: ICoreWebView2,
    controller: ICoreWebView2Controller,
    host: Option<Rc<calc_xll_host::XllHost>>,
    rtd: Option<Rc<calc_com_host::RtdClient>>,
    fetch: Rc<ThreadedFetch>,
    frame: windows_sys::Win32::Foundation::HWND,
    bounds: Cell<(i32, i32)>,
    book: Cell<u64>,
    probe: Option<PathBuf>,
    quit: Cell<bool>,
    /// The add-ins' functions, bound to each new server (a page's load starts a new worker).
    functions: RefCell<Option<calc_host::CallableTable>>,
}

impl App {
    fn post(&self, text: &str, bytes: Option<&[u8]>) {
        let message = match bytes {
            None => text.to_owned(),
            Some(bytes) => format!("{text}{SEPARATOR}{}", base64(bytes)),
        };
        let wide = CoTaskMemPWSTR::from(message.as_str());
        if let Err(why) = unsafe { self.view.PostWebMessageAsString(*wide.as_ref().as_pcwstr()) } {
            eprintln!("dna-calc-desktop: post to page failed: {why}");
        }
    }

    fn post_event(&self, event: Event) {
        match event {
            Event::Tiles(batch) => match wire::encode_tiles(&batch) {
                Ok(bytes) => self.post(wire::TILE_TAG, Some(&bytes)),
                Err(message) => {
                    self.post(&wire::encode(&FromServer::Failed { id: None, message }), None)
                }
            },
            control => self.post(&wire::encode(&FromServer::Event(control)), None),
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
        if message == "hello" {
            // A page that loads starts its own worker, as in a browser: a new server.
            let mut server = Server::new();
            server.set_calculation_budget(calc_host::WorkBudget::evaluations(64));
            server.set_fetch_route(self.fetch.clone());
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
        let (text, bytes) = match message.split_once(SEPARATOR) {
            Some((text, encoded)) => (text, unbase64(encoded)),
            None => (message, None),
        };
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
        let Some(host) = &self.host else { return };
        let server = self.server.borrow();
        for sheet in server.book().host().sheets() {
            host.set_sheet(sheet.id(), format!("[Book1]{}", sheet.name().text().to_string_lossy()), true);
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
        let (mut width, mut height) = (0, 0);
        let mut rect = windows_sys::Win32::Foundation::RECT { left: 0, top: 0, right: 0, bottom: 0 };
        if unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(self.frame, &mut rect) } != 0 {
            (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
        }
        if (width, height) != self.bounds.get() && width > 0 && height > 0 {
            self.bounds.set((width, height));
            let bounds = windows::Win32::Foundation::RECT { left: 0, top: 0, right: width, bottom: height };
            let _ = unsafe { self.controller.SetBounds(bounds) };
        }
        if let Some(host) = &self.host {
            for returned in host.take_async_returns() {
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
        }
        if let Some(rtd) = &self.rtd {
            let evaluating = self.server.borrow().has_pending_calculation();
            let inputs = if evaluating { rtd.poll_during_evaluation() } else { rtd.poll() };
            for input in inputs.unwrap_or_default() {
                self.server.borrow_mut().update_rtd(
                    input.provider,
                    input.subscription,
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
    }
}

fn environment(user_data: &std::path::Path, arguments: String) -> Result<ICoreWebView2Environment, String> {
    let options = CoreWebView2EnvironmentOptions::default();
    unsafe { options.set_additional_browser_arguments(arguments) };
    let options: ICoreWebView2EnvironmentOptions = options.into();
    let (tx, rx) = std::sync::mpsc::channel();
    let folder = CoTaskMemPWSTR::from(user_data.to_string_lossy().as_ref());
    CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| unsafe {
            CreateCoreWebView2EnvironmentWithOptions(PCWSTR::null(), *folder.as_ref().as_pcwstr(), &options, &handler)
                .map_err(webview2_com::Error::WindowsError)
        }),
        Box::new(move |code, environment| {
            code?;
            let _ = tx.send(environment);
            Ok(())
        }),
    )
    .map_err(|e| format!("WebView2 environment: {e:?}"))?;
    rx.recv()
        .ok()
        .flatten()
        .ok_or_else(|| "WebView2 gave no environment".to_owned())
}

fn controller(environment: &ICoreWebView2Environment, parent: WinHwnd) -> Result<ICoreWebView2Controller, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let environment = environment.clone();
    CreateCoreWebView2ControllerCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| unsafe {
            environment
                .CreateCoreWebView2Controller(parent, &handler)
                .map_err(webview2_com::Error::WindowsError)
        }),
        Box::new(move |code, controller| {
            code?;
            let _ = tx.send(controller);
            Ok(())
        }),
    )
    .map_err(|e| format!("WebView2 controller: {e:?}"))?;
    rx.recv()
        .ok()
        .flatten()
        .ok_or_else(|| "WebView2 gave no controller".to_owned())
}

fn add_script(view: &ICoreWebView2, source: &str) -> Result<(), String> {
    let view = view.clone();
    let source = source.to_owned();
    AddScriptToExecuteOnDocumentCreatedCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| unsafe {
            let js = CoTaskMemPWSTR::from(source.as_str());
            view.AddScriptToExecuteOnDocumentCreated(*js.as_ref().as_pcwstr(), &handler)
                .map_err(webview2_com::Error::WindowsError)
        }),
        Box::new(|code, _| code),
    )
    .map_err(|e| format!("WebView2 script: {e:?}"))
}

pub fn run(args: &[String]) -> Result<(), String> {
    let o = options(args)?;
    unsafe {
        // Per-monitor awareness: the page renders at the display's real scale.
        windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        );
    }
    let shell = Shell::new()?;
    let frame = shell.workbook_hwnd();
    shell.show(o.size.0, o.size.1);
    let user_data = std::env::var_os("DNACALC_WEBVIEW2_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("dnacalc-webview2"));
    let mut browser_args = o.webview_args.clone();
    if let Some(port) = o.debug_port {
        browser_args.push_str(&format!(" --remote-debugging-port={port} --remote-allow-origins=*"));
    }
    let env = environment(&user_data, browser_args.trim().to_owned())?;
    let controller = controller(&env, WinHwnd(frame.cast()))?;
    let view = unsafe { controller.CoreWebView2() }.map_err(|e| e.to_string())?;
    unsafe {
        let _ = controller.SetBounds(windows::Win32::Foundation::RECT { left: 0, top: 0, right: o.size.0, bottom: o.size.1 });
        let _ = controller.SetIsVisible(true);
    }
    add_script(&view, include_str!("../../dna-calc/web/desktop-bridge.js"))?;

    // The native worker: the portable server, with the desktop's fetch route and add-ins.
    let mut server = Server::new();
    server.set_calculation_budget(calc_host::WorkBudget::evaluations(64));
    let fetch = Rc::new(ThreadedFetch::new());
    server.set_fetch_route(fetch.clone());
    let (mut host, mut rtd, mut bound) = (None, None, None);
    if !o.xlls.is_empty() {
        let native = Rc::new(calc_xll_host::XllHost::new(shell.hwnd()));
        let mut shell_mut = shell;
        for sheet in server.book().host().sheets() {
            native.set_sheet(sheet.id(), format!("[Book1]{}", sheet.name().text().to_string_lossy()), true);
        }
        shell_mut.attach_host(native.clone())?;
        shell_mut.projection()?.set_workbook("Book1")?;
        for xll in &o.xlls {
            native.load(xll)?;
        }
        native.set_async_continuations(true)?;
        let client = Rc::new(calc_com_host::RtdClient::new()?);
        native.set_rtd_service(client.clone())?;
        let functions = native.functions()?;
        server.set_user_functions(functions.clone());
        bound = Some(functions);
        host = Some(native);
        rtd = Some(client);
        return drive(shell_mut, o, env, controller, view, server, fetch, host, rtd, bound);
    }
    drive(shell, o, env, controller, view, server, fetch, host, rtd, bound)
}

#[allow(clippy::too_many_arguments)]
fn drive(
    shell: Shell,
    o: Options,
    _env: ICoreWebView2Environment,
    controller: ICoreWebView2Controller,
    view: ICoreWebView2,
    server: Server,
    fetch: Rc<ThreadedFetch>,
    host: Option<Rc<calc_xll_host::XllHost>>,
    rtd: Option<Rc<calc_com_host::RtdClient>>,
    functions: Option<calc_host::CallableTable>,
) -> Result<(), String> {
    let app = Rc::new(App {
        book: Cell::new(server.workbook_id()),
        server: RefCell::new(server),
        view: view.clone(),
        controller,
        host,
        rtd,
        fetch,
        frame: shell.workbook_hwnd(),
        bounds: Cell::new(o.size),
        probe: o.probe.clone(),
        quit: Cell::new(false),
        functions: RefCell::new(functions),
    });
    {
        let app = app.clone();
        let mut token = 0i64;
        unsafe {
            view.add_WebMessageReceived(
                &WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
                    if let Some(args) = args {
                        let mut message = PWSTR::null();
                        if args.TryGetWebMessageAsString(&mut message).is_ok() {
                            let message = take_pwstr(message);
                            app.request(&message);
                        }
                    }
                    Ok(())
                })),
                &mut token,
            )
            .map_err(|e| e.to_string())?;
        }
    }
    // Where the page comes from: the harness's server, a mapped folder, or the probe page.
    let target = if o.probe.is_some() {
        let folder = std::env::temp_dir().join("dnacalc-probe");
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        std::fs::write(folder.join("webgpu.html"), include_str!("../probe/webgpu.html"))
            .map_err(|e| e.to_string())?;
        map_folder(&view, &folder)?;
        "https://dnacalc.app/webgpu.html".to_owned()
    } else if let Some(url) = &o.url {
        url.clone()
    } else {
        let assets = o.assets.as_ref().ok_or("no assets")?;
        map_folder(&view, assets)?;
        "https://dnacalc.app/index.html".to_owned()
    };
    app.bind_sheets();
    unsafe {
        let wide = CoTaskMemPWSTR::from(target.as_str());
        view.Navigate(*wide.as_ref().as_pcwstr()).map_err(|e| e.to_string())?;
        windows_sys::Win32::UI::WindowsAndMessaging::SetTimer(std::ptr::null_mut(), TICK_ID, TICK_MS, None);
    }
    let _ = o.hidden;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    let mut message: MSG = unsafe { std::mem::zeroed() };
    while !app.quit.get() && unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
        if message.hwnd.is_null() {
            match message.message {
                WM_TIMER if message.wParam == TICK_ID => app.tick(),
                WM_FETCH => app.fetched(),
                _ => {}
            }
            continue;
        }
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    unsafe { KillTimer(std::ptr::null_mut(), TICK_ID) };
    drop(app);
    drop(shell);
    Ok(())
}

fn map_folder(view: &ICoreWebView2, folder: &std::path::Path) -> Result<(), String> {
    let view = view.cast::<ICoreWebView2_3>().map_err(|e| e.to_string())?;
    let host = CoTaskMemPWSTR::from("dnacalc.app");
    let folder = CoTaskMemPWSTR::from(folder.to_string_lossy().as_ref());
    unsafe {
        view.SetVirtualHostNameToFolderMapping(
            *host.as_ref().as_pcwstr(),
            *folder.as_ref().as_pcwstr(),
            COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_ALLOW,
        )
    }
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: the carrier's base64 must agree with the page's atob/btoa framing.
    #[test]
    fn bytes_cross_the_bridge_exactly() {
        for length in [0usize, 1, 2, 3, 4, 5, 255, 256, 1000] {
            let bytes: Vec<u8> = (0..length).map(|i| (i * 7 + 3) as u8).collect();
            let text = base64(&bytes);
            assert_eq!(unbase64(&text).as_deref(), Some(bytes.as_slice()), "{length}");
        }
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(unbase64("TWE="), Some(b"Ma".to_vec()));
    }

    /// Native-only: argument parsing for app mode.
    #[test]
    fn app_options_need_a_page() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(options(&args(&["--app"])).is_err());
        let o = options(&args(&["--app", "--assets", "dist", "--size", "800x600", "--xll", "a.xll"])).unwrap();
        assert_eq!((o.size, o.xlls.len()), ((800, 600), 1));
        assert!(options(&args(&["--app", "--probe", "p.json"])).is_ok());
        assert!(options(&args(&["--app", "--bogus"])).is_err());
    }
}
