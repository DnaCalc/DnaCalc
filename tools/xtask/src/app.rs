//! `cargo xtask app bundle <app> [--out <dir>]` and `cargo xtask app smoke <app> [--bundle <dir>]
//! [--browser chromium|firefox] [--workbook <file> [--edit <sheet>!<cell>=<text>]]`: the browser
//! bundle of an application and its smoke test, for DNA OneCalc (`onecalc`) and DNA Calc
//! (`calc`). `cargo xtask onecalc bundle|smoke ...` is the same command for `onecalc`.
//!
//! `bundle` builds the app's browser entry (`apps/<package>/src/bin/<package>-web.rs`) for
//! wasm32-unknown-unknown in the workspace's `web` profile (release, optimised for size), runs
//! wasm-bindgen 0.2.117 (`--target web`) over it, and copies `apps/<package>/web/index.html` beside
//! the output, into `target/<app>-bundle` unless `--out` names another folder. It prints each
//! file's size and the total. Nothing is published: the folder is served only by `smoke`, on
//! 127.0.0.1.
//!
//! `smoke` builds the bundle (or takes the one `--bundle` names), serves it on 127.0.0.1 from a
//! port the system picks, and drives it in headless Chrome and headless Firefox through their
//! WebDriver servers (chromedriver and geckodriver, found as `cargo xtask accept` finds them:
//! `CHROMEDRIVER` or `GECKODRIVER` naming a file, else PATH). In each browser it loads the page,
//! runs the app's cases in order (typing with WebDriver key input, the browser's own key and input
//! events; clicking; choosing a file for a file input) and reads back what the page shows. Every
//! expected value is derived by hand from Excel's documented behaviour. After every case it reads
//! the page's fault record: the panic message the app's panic hook writes on the root element and
//! every uncaught error and unhandled rejection the page recorded. A panic in a reactive effect,
//! which surfaces in a browser only as a trap (`RuntimeError: unreachable`) while the rest of the
//! page may still answer, fails the case there. A browser whose driver is missing fails the run;
//! it is never skipped.
//!
//! For DNA Calc, `--workbook` then opens a workbook from outside the repository through the page's
//! file input, optionally enters `<text>` in `<sheet>!<cell>` (`--edit`), opens the atlas, and
//! prints only what the page reports about it: the open and edit timings and the formula counts by
//! provenance. The workbook's content is not printed. With `--profile <file>`, Chrome's CPU profile
//! of the open (the DevTools protocol's Profiler, sampled every 200 microseconds, through
//! chromedriver's `goog/cdp/execute`) is written to the file, for dc-26q. With `--skins` (and
//! `--edit`), each of the grid, tree, canvas and ledger skins is shown in turn and the time it
//! takes to show is printed, then the edit is entered through the formula bar with that skin
//! shown (a number, one more for each skin, so each edit changes the cell) and the time until the
//! page's work for it is done is printed (dc-gf0): both measured in the page, from just before
//! the click or the Enter key to the first timer task that finds the skin's mark there.
//!
//! Exit code: 0 when every case passed in every browser run, 1 otherwise, 2 for a usage error.

use crate::accept::{BROWSERS, Browser, find_on_path, variable_file};
use crate::cargo::{self, WASM_TARGET, Workspace};
use serde_json::{Value, json};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub const USAGE: &str = "usage: cargo xtask app bundle onecalc|calc [--out <dir>]\n       cargo xtask app smoke onecalc|calc [--bundle <dir>] [--browser chromium|firefox] [--workbook <file> [--edit <sheet>!<cell>=<text>] [--skins] [--profile <cpuprofile.json>]]\n       cargo xtask onecalc bundle|smoke ... (the same, for onecalc)";

/// One browser application: its package, its entry, its page, and how its page reports itself.
pub struct App {
    /// The name on the command line.
    pub key: &'static str,
    /// The prefix of the command's output lines.
    pub label: &'static str,
    /// The package and binary of the browser entry.
    pub package: &'static str,
    pub entry: &'static str,
    /// The page, relative to the workspace root.
    pub page: &'static str,
    /// The default bundle folder, relative to the workspace root.
    pub default_out: &'static str,
    /// The attribute naming an element's role.
    pub role_attribute: &'static str,
    /// The root element's attribute holding a panic's message.
    pub panic_attribute: &'static str,
    /// The root element's attribute set once the app is mounted.
    pub ready_attribute: &'static str,
    /// The page's list of uncaught errors.
    pub faults: &'static str,
    pub cases: &'static [Case],
}

/// The applications.
pub static APPS: [App; 2] = [
    App {
        key: "onecalc",
        label: "ONECALC",
        package: "dna-onecalc",
        entry: "dna-onecalc-web",
        page: "apps/dna-onecalc/web/index.html",
        default_out: "target/onecalc-bundle",
        role_attribute: "data-onecalc",
        panic_attribute: "data-onecalc-panic",
        ready_attribute: "data-onecalc-ready",
        faults: "__onecalcFaults",
        cases: ONECALC_CASES,
    },
    App {
        key: "calc",
        label: "DNACALC",
        package: "dna-calc",
        entry: "dna-calc-web",
        page: "apps/dna-calc/web/index.html",
        default_out: "target/dnacalc-bundle",
        role_attribute: "data-dnacalc",
        panic_attribute: "data-dnacalc-panic",
        ready_attribute: "data-dnacalc-ready",
        faults: "__dnacalcFaults",
        cases: CALC_CASES,
    },
];

/// The wasm-bindgen CLI release the bundle needs: the one the apps' wasm-bindgen crate is pinned to.
const BINDGEN_VERSION: &str = "wasm-bindgen 0.2.117";

/// The cargo profile of the bundle: release, optimised for size (the workspace's `[profile.web]`).
const PROFILE: &str = "web";

/// How long a check waits for the page to show what it expects.
const SETTLE: Duration = Duration::from_secs(8);

/// How long a step over a workbook from outside the repository may take.
const WORKBOOK_SETTLE: Duration = Duration::from_secs(900);

/// `cargo xtask onecalc ...`: the app command for DNA OneCalc.
pub fn run_onecalc(raw: &[String]) -> ExitCode {
    match raw {
        [command, rest @ ..] => {
            let mut args = vec![command.clone(), "onecalc".to_owned()];
            args.extend(rest.iter().cloned());
            run(&args)
        }
        [] => usage(raw),
    }
}

/// What `--workbook` asks of a smoke run.
struct WorkbookRun {
    path: PathBuf,
    /// The sheet, the cell and the text to enter.
    edit: Option<(String, String, String)>,
    /// Where to write Chrome's CPU profile of the open (`--profile`), through the DevTools
    /// protocol chromedriver relays; Firefox's run takes none.
    profile: Option<PathBuf>,
    /// Time each skin's showing and the edit with it shown (`--skins`).
    skins: bool,
}

pub fn run(raw: &[String]) -> ExitCode {
    let ws = match std::env::current_dir()
        .map_err(|e| format!("no current directory: {e}"))
        .and_then(|cwd| Workspace::load(&cwd))
    {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("xtask app: cannot read the workspace: {e}");
            return ExitCode::from(1);
        }
    };
    let (command, app, rest) = match raw {
        [command, key, rest @ ..] => match APPS.iter().find(|app| app.key == key) {
            Some(app) => (command.as_str(), app, rest),
            None => return usage(raw),
        },
        _ => return usage(raw),
    };
    match command {
        "bundle" => {
            let out = match rest {
                [] => ws.root.join(app.default_out),
                [flag, dir] if flag == "--out" => PathBuf::from(dir),
                _ => return usage(raw),
            };
            match bundle(&ws, app, &out) {
                Ok(_) => ExitCode::SUCCESS,
                Err(e) => {
                    println!("{} RESULT failed: {e}", app.label);
                    ExitCode::from(1)
                }
            }
        }
        "smoke" => {
            let mut bundle_dir = None;
            let mut only: Option<String> = None;
            let mut workbook: Option<PathBuf> = None;
            let mut edit = None;
            let mut profile = None;
            let mut skins = false;
            let mut args = rest.iter();
            while let Some(arg) = args.next() {
                if arg == "--skins" && app.key == "calc" {
                    skins = true;
                    continue;
                }
                match (arg.as_str(), args.next()) {
                    ("--bundle", Some(dir)) => bundle_dir = Some(PathBuf::from(dir)),
                    ("--browser", Some(name)) => only = Some(name.clone()),
                    ("--workbook", Some(path)) if app.key == "calc" => {
                        workbook = Some(PathBuf::from(path))
                    }
                    ("--profile", Some(path)) if app.key == "calc" => {
                        profile = Some(PathBuf::from(path))
                    }
                    ("--edit", Some(spec)) => match parse_edit(spec) {
                        Some(parsed) => edit = Some(parsed),
                        None => return usage(raw),
                    },
                    _ => return usage(raw),
                }
            }
            if (edit.is_some() && workbook.is_none()) || (skins && edit.is_none()) {
                return usage(raw);
            }
            let workbook = workbook.map(|path| WorkbookRun {
                path,
                edit,
                profile,
                skins,
            });
            smoke(&ws, app, bundle_dir, only.as_deref(), workbook.as_ref())
        }
        _ => usage(raw),
    }
}

/// `<sheet>!<cell>=<text>` as its three parts; the sheet may be quoted.
fn parse_edit(spec: &str) -> Option<(String, String, String)> {
    let (target, text) = spec.split_once('=')?;
    let (sheet, cell) = target.rsplit_once('!')?;
    let sheet = sheet.trim_matches('\'');
    (!sheet.is_empty() && !cell.is_empty())
        .then(|| (sheet.to_owned(), cell.to_owned(), text.to_owned()))
}

fn usage(raw: &[String]) -> ExitCode {
    eprintln!("xtask app: unknown arguments {raw:?}\n{USAGE}");
    ExitCode::from(2)
}

// ---------------------------------------------------------------------------------------------
// The bundle.

/// Builds the bundle into `out` and returns its files with their sizes.
fn bundle(ws: &Workspace, app: &App, out: &Path) -> Result<Vec<(String, u64)>, String> {
    let bindgen = find_on_path("wasm-bindgen").ok_or("wasm-bindgen is not on PATH")?;
    let version = cargo::output(Command::new(&bindgen).arg("--version"))?;
    let version = cargo::text(&version.stdout).trim().to_owned();
    if version != BINDGEN_VERSION {
        return Err(format!(
            "{} is {version}, not {BINDGEN_VERSION}, the release the app's wasm-bindgen crate is pinned to",
            bindgen.display()
        ));
    }
    println!(
        "{} building {} for {WASM_TARGET} (profile {PROFILE})",
        app.label, app.entry
    );
    let target_dir = ws.accept_target_dir();
    let mut build = Command::new(cargo::cargo_program());
    build
        .current_dir(&ws.root)
        .args([
            "build",
            "--locked",
            "--profile",
            PROFILE,
            "-p",
            app.package,
            "--bin",
            app.entry,
        ])
        .args(["--target", WASM_TARGET])
        .arg("--target-dir")
        .arg(&target_dir);
    let built = cargo::output(&mut build)?;
    if !built.status.success() {
        return Err(format!(
            "cargo build failed:\n{}",
            cargo::tail(&built.stderr, 40)
        ));
    }
    let wasm = target_dir
        .join(WASM_TARGET)
        .join(PROFILE)
        .join(format!("{}.wasm", app.entry));
    fs::create_dir_all(out).map_err(|e| format!("cannot create {}: {e}", out.display()))?;
    let generated = cargo::output(
        Command::new(&bindgen)
            .args(["--target", "web", "--no-typescript", "--out-dir"])
            .arg(out)
            .arg(&wasm),
    )?;
    if !generated.status.success() {
        return Err(format!(
            "wasm-bindgen failed:\n{}",
            cargo::tail(&generated.stderr, 40)
        ));
    }
    fs::copy(ws.root.join(app.page), out.join("index.html"))
        .map_err(|e| format!("cannot copy {}: {e}", app.page))?;
    let mut files = Vec::new();
    for name in [
        "index.html".to_owned(),
        format!("{}.js", app.entry),
        format!("{}_bg.wasm", app.entry),
    ] {
        let size = fs::metadata(out.join(&name))
            .map_err(|e| format!("the bundle has no {name}: {e}"))?
            .len();
        files.push((name, size));
    }
    let total: u64 = files.iter().map(|(_, size)| size).sum();
    for (name, size) in &files {
        println!("{} bundle file {name}: {size} bytes", app.label);
    }
    println!(
        "{} bundle {}: {} files, {total} bytes",
        app.label,
        out.display(),
        files.len()
    );
    Ok(files)
}

// ---------------------------------------------------------------------------------------------
// The page server.

/// Serves the files of `dir` on 127.0.0.1 from a thread; returns the port.
fn serve(dir: PathBuf) -> Result<u16, String> {
    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|e| format!("cannot listen on 127.0.0.1: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("no local address: {e}"))?
        .port();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let dir = dir.clone();
            thread::spawn(move || {
                let _ = answer(stream, &dir);
            });
        }
    });
    Ok(port)
}

/// Answers one request: a file of the bundle by its plain name, `/` as `index.html`.
fn answer(mut stream: TcpStream, dir: &Path) -> std::io::Result<()> {
    let mut request = Vec::new();
    let mut buffer = [0u8; 4096];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
    }
    let line = String::from_utf8_lossy(&request);
    let path = line.split_whitespace().nth(1).unwrap_or("/");
    let path = path.split('?').next().unwrap_or("/");
    let name = match path.trim_start_matches('/') {
        "" => "index.html",
        name => name,
    };
    let plain = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !name.contains("..");
    let content_type = match Path::new(name).extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    };
    let body = if plain {
        fs::read(dir.join(name)).ok()
    } else {
        None
    };
    match body {
        Some(body) => {
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
                body.len()
            )?;
            stream.write_all(&body)?;
        }
        None => {
            stream.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )?;
        }
    }
    stream.flush()
}

// ---------------------------------------------------------------------------------------------
// WebDriver.

/// One HTTP request to a WebDriver server on 127.0.0.1; the response's JSON `value`, or the
/// error the server reported.
fn webdriver(port: u16, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .map_err(|e| format!("cannot reach the driver on port {port}: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(120)))
        .map_err(|e| e.to_string())?;
    let body = body.map(Value::to_string).unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .map_err(|e| format!("cannot write to the driver: {e}"))?;
    // A driver may keep the connection open whatever the request asks, so the answer is read to
    // its declared length (or its last chunk), not to the end of the stream.
    let mut response = Vec::new();
    let mut buffer = [0u8; 16384];
    let read_more = |stream: &mut TcpStream, response: &mut Vec<u8>, buffer: &mut [u8]| {
        let read = stream
            .read(buffer)
            .map_err(|e| format!("cannot read the driver's answer: {e}"))?;
        response.extend_from_slice(&buffer[..read]);
        Ok::<usize, String>(read)
    };
    let split = loop {
        if let Some(split) = response.windows(4).position(|w| w == b"\r\n\r\n") {
            break split;
        }
        if read_more(&mut stream, &mut response, &mut buffer)? == 0 {
            return Err("the driver's answer ended inside its header".to_owned());
        }
    };
    let head = String::from_utf8_lossy(&response[..split]).to_ascii_lowercase();
    let length = head.lines().find_map(|line| {
        line.strip_prefix("content-length:")
            .and_then(|n| n.trim().parse::<usize>().ok())
    });
    let chunked = head.contains("transfer-encoding: chunked");
    loop {
        let body = &response[split + 4..];
        let complete = match length {
            Some(length) => body.len() >= length,
            None if chunked => body.ends_with(b"0\r\n\r\n"),
            None => false,
        };
        if complete || read_more(&mut stream, &mut response, &mut buffer)? == 0 {
            break;
        }
    }
    let mut payload = response[split + 4..].to_vec();
    if let Some(length) = length {
        payload.truncate(length);
    } else if chunked {
        payload = unchunk(&payload)?;
    }
    let value: Value = serde_json::from_slice(&payload).map_err(|e| {
        format!(
            "the driver's answer is not JSON ({e}): {}",
            String::from_utf8_lossy(&payload)
        )
    })?;
    let value = value.get("value").cloned().unwrap_or(Value::Null);
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        let message = value.get("message").and_then(Value::as_str).unwrap_or("");
        return Err(format!("{method} {path}: {error}: {message}"));
    }
    Ok(value)
}

/// The body of a chunked HTTP response.
fn unchunk(mut data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let line_end = data
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or("a chunk without a size line")?;
        let size_text = String::from_utf8_lossy(&data[..line_end]);
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|e| format!("a chunk size {size_text:?}: {e}"))?;
        data = &data[line_end + 2..];
        if size == 0 {
            return Ok(out);
        }
        if data.len() < size {
            return Err("a chunk shorter than its size".to_owned());
        }
        out.extend_from_slice(&data[..size]);
        data = data.get(size + 2..).unwrap_or(&[]);
    }
}

/// A free port on 127.0.0.1, for a driver to listen on.
fn free_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    Ok(port)
}

/// A running WebDriver server, stopped when dropped.
struct DriverProcess {
    child: Child,
    port: u16,
}

impl Drop for DriverProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One browser session over a driver.
struct Session {
    port: u16,
    id: String,
}

impl Session {
    fn post(&self, path: &str, body: Value) -> Result<Value, String> {
        webdriver(
            self.port,
            "POST",
            &format!("/session/{}{path}", self.id),
            Some(&body),
        )
    }

    fn script(&self, script: &str) -> Result<Value, String> {
        self.post("/execute/sync", json!({ "script": script, "args": [] }))
    }

    /// The element `css` selects.
    fn element(&self, css: &str) -> Result<String, String> {
        let found = self.post("/element", json!({ "using": "css selector", "value": css }))?;
        found
            .as_object()
            .and_then(|o| o.values().next())
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("no element id for {css}"))
    }

    /// Clears the element `css` selects (WebDriver's Element Clear).
    fn clear(&self, css: &str) -> Result<(), String> {
        let element = self.element(css)?;
        self.post(&format!("/element/{element}/clear"), json!({}))?;
        Ok(())
    }

    fn keys(&self, css: &str, text: &str) -> Result<(), String> {
        let element = self.element(css)?;
        self.post(
            &format!("/element/{element}/value"),
            json!({ "text": text }),
        )?;
        Ok(())
    }

    fn end(&self) {
        let _ = webdriver(self.port, "DELETE", &format!("/session/{}", self.id), None);
    }
}

/// Starts `browser`'s driver and a headless session in it.
fn open(browser: &Browser) -> Result<(DriverProcess, Session), String> {
    let program = variable_file(browser.variable)
        .or_else(|| find_on_path(browser.program))
        .ok_or_else(|| {
            format!(
                "{} is not on PATH and {} names no file",
                browser.program, browser.variable
            )
        })?;
    let port = free_port()?;
    let mut command = Command::new(&program);
    match browser.name {
        "firefox" => command.args(["--port", &port.to_string()]),
        _ => command.arg(format!("--port={port}")),
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = command
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", program.display()))?;
    let driver = DriverProcess { child, port };
    let started = Instant::now();
    loop {
        if let Ok(status) = webdriver(port, "GET", "/status", None)
            && status.get("ready").and_then(Value::as_bool) == Some(true)
        {
            break;
        }
        if started.elapsed() > Duration::from_secs(20) {
            return Err(format!("{} did not become ready", browser.program));
        }
        thread::sleep(Duration::from_millis(200));
    }
    let capabilities = match browser.name {
        "firefox" => json!({
            "browserName": "firefox",
            "moz:firefoxOptions": { "args": ["-headless", "-width=1280", "-height=1000"] }
        }),
        _ => json!({
            "browserName": "chrome",
            "goog:chromeOptions": { "args": ["--headless=new", "--disable-gpu", "--window-size=1280,1000"] }
        }),
    };
    let created = webdriver(
        port,
        "POST",
        "/session",
        Some(&json!({ "capabilities": { "alwaysMatch": capabilities } })),
    )?;
    let id = created
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or("the driver gave no session id")?
        .to_owned();
    let session = Session {
        port: driver.port,
        id,
    };
    Ok((driver, session))
}

// ---------------------------------------------------------------------------------------------
// The cases.

/// WebDriver's codes for keys that are not characters.
const TAB: &str = "\u{E004}";
const ENTER: &str = "\u{E007}";
const ESCAPE: &str = "\u{E00C}";
const DELETE: &str = "\u{E017}";
const F2: &str = "\u{E032}";

/// The selector of an element: a selector as written when `name` starts with `[`, otherwise the
/// element whose role attribute is `name`.
fn select(app: &App, name: &str) -> String {
    if name.starts_with('[') {
        name.to_owned()
    } else {
        format!("[{}=\"{name}\"]", app.role_attribute)
    }
}

/// What a case does. A name is a role or a selector ([`select`]).
pub enum Action {
    /// DNA OneCalc: types into a grid cell, after clearing it.
    Grid(&'static str, &'static str),
    /// DNA OneCalc: clears the formula field and types this.
    Formula(&'static str),
    /// Types these keys into the element, without clearing it.
    Keys(&'static str, &'static str),
    /// Chooses the option with this value in the select element.
    Choose(&'static str, &'static str),
    /// Clears the field and types this into it.
    Into(&'static str, &'static str),
    /// Clicks the element.
    Click(&'static str),
    /// Chooses this file (relative to the workspace root) in the file input.
    Upload(&'static str, &'static str),
}

/// What a case expects the page to show. A name is a role or a selector ([`select`]).
pub enum Check {
    /// The element's text.
    Text(&'static str, &'static str),
    /// The element's text starts with this.
    Starts(&'static str, &'static str),
    /// The list items under the element, in order.
    Items(&'static str, &'static [&'static str]),
    /// The element contains a list item starting with this text.
    Contains(&'static str, &'static str),
    /// No element is selected.
    Absent(&'static str),
    /// The field's value.
    Value(&'static str, &'static str),
    /// The element's attribute.
    Attr(&'static str, &'static str, &'static str),
    /// DNA OneCalc: the array result's cells, row by row.
    Array(&'static [&'static [&'static str]]),
    /// DNA OneCalc: the text of the highlighted run with this role.
    Highlighted(&'static str, &'static str),
}

pub struct Case {
    name: &'static str,
    actions: &'static [Action],
    checks: &'static [Check],
}

/// DNA OneCalc's cases, in the order each browser runs them in one page. The grid entries of the
/// first case stay for the rest: A1 = 1.5, A2 = 2, B1 = hello, B2 = TRUE, A3 left blank.
const ONECALC_CASES: &[Case] = &[
    Case {
        name: "the page loads with the surface and an empty formula",
        actions: &[
            Action::Grid("A1", "1.5"),
            Action::Grid("A2", "2"),
            Action::Grid("B1", "hello"),
            Action::Grid("B2", "TRUE"),
        ],
        checks: &[
            Check::Text("result", "no formula"),
            Check::Text("sheets", "Sheet1"),
            Check::Text("formula-cell-status", "Sheet1!H1"),
        ],
    },
    Case {
        name: "arithmetic: =1+2 is the number 3",
        actions: &[Action::Formula("=1+2")],
        checks: &[
            Check::Text("result", "number 3"),
            Check::Text("result-type", "number"),
            Check::Text("result-value", "3"),
            Check::Starts(
                "provenance",
                "calculated in Sheet1!H1 by calc-fml's evaluator with calc-func's functions; no cells read",
            ),
            Check::Items("diagnostics", &[]),
        ],
    },
    Case {
        name: "a grid range with a blank cell: =SUM(A1:A3) is 3.5",
        actions: &[Action::Formula("=SUM(A1:A3)")],
        checks: &[
            Check::Text("result", "number 3.5"),
            Check::Items(
                "reads",
                &["Sheet1!A1:A3: Sheet1!A1 = number 1.5; Sheet1!A2 = number 2; Sheet1!A3 blank"],
            ),
            Check::Starts("provenance", "calculated in Sheet1!H1"),
            Check::Highlighted("function", "SUM"),
        ],
    },
    Case {
        name: "text from the grid: =B1&\" world\"",
        actions: &[Action::Formula("=B1&\" world\"")],
        checks: &[
            Check::Text("result-type", "text"),
            Check::Text("result-value", "hello world"),
        ],
    },
    Case {
        name: "a logical from the grid: =IF(B2,A1*10,0) is 15",
        actions: &[Action::Formula("=IF(B2,A1*10,0)")],
        checks: &[Check::Text("result", "number 15")],
    },
    Case {
        name: "an array constant times 10 is a 2 x 2 grid",
        actions: &[Action::Formula("={1,2;3,4}*10")],
        checks: &[
            Check::Text("result-type", "array"),
            Check::Starts("result-shape", "2 x 2 array"),
            Check::Array(&[&["10", "20"], &["30", "40"]]),
        ],
    },
    Case {
        name: "a grid range lifted: =A1:A2*2 is a 2 x 1 grid",
        actions: &[Action::Formula("=A1:A2*2")],
        checks: &[Check::Array(&[&["3"], &["4"]])],
    },
    Case {
        name: "an error value: =1/0 is #DIV/0!, shown with its meaning",
        actions: &[Action::Formula("=1/0")],
        checks: &[
            Check::Text("result-type", "error"),
            Check::Text("result-value", "#DIV/0!"),
            Check::Text("result-error", "division by zero"),
        ],
    },
    Case {
        name: "a syntax error is a diagnostic and no value",
        actions: &[Action::Formula("=A1*")],
        checks: &[
            Check::Text("result", "not calculated"),
            Check::Text("result-type", "no value"),
            Check::Contains("diagnostics", "syntax at 4..4: "),
        ],
    },
    Case {
        name: "an unknown function is refused by name",
        actions: &[Action::Formula("=NOSUCH(1)")],
        checks: &[
            Check::Text("result", "not calculated"),
            Check::Items("diagnostics", &["unknown function at 1..7: NOSUCH"]),
        ],
    },
    Case {
        name: "completion: =SUM lists SUM first; Tab accepts it with its parenthesis",
        actions: &[Action::Formula("=SUM")],
        checks: &[
            Check::Text("completion-label", "SUM"),
            Check::Text("help-name", "SUM"),
        ],
    },
    Case {
        name: "completion accepted: signature help marks the first argument",
        actions: &[Action::Keys("formula", TAB)],
        checks: &[
            Check::Value("formula", "=SUM("),
            Check::Absent("completions"),
            Check::Text("active-parameter", "number1"),
            Check::Text("signature", "SUM(number1, [number2], ...)"),
            Check::Text("help-name", "SUM"),
            Check::Text(
                "help-description",
                "Use this function to add the values in cells.",
            ),
        ],
    },
    Case {
        name: "the arguments typed after completion calculate",
        actions: &[Action::Keys("formula", "A1:A2,10)")],
        checks: &[
            Check::Value("formula", "=SUM(A1:A2,10)"),
            Check::Text("result", "number 13.5"),
        ],
    },
    Case {
        name: "signature help follows the argument the caret is in",
        actions: &[Action::Formula("=ROUND(A1,")],
        checks: &[
            Check::Text("active-parameter", "num_digits"),
            Check::Text("help-name", "ROUND"),
        ],
    },
    Case {
        name: "a number shows as a cell shows it, with the exact decimal beside",
        actions: &[Action::Formula("=0.1+0.2")],
        checks: &[
            Check::Text("result-value", "0.3"),
            Check::Text("result-exact", " exactly 0.30000000000000004"),
        ],
    },
    Case {
        name: "a date function reads the 1900 date system: =DATE(2024,1,31) is 45322",
        actions: &[Action::Formula("=DATE(2024,1,31)")],
        checks: &[Check::Text("result", "number 45322")],
    },
    Case {
        name: "a number format reads the en-US profile: =TEXT(1234.5,\"#,##0.00\")",
        actions: &[Action::Formula("=TEXT(1234.5,\"#,##0.00\")")],
        checks: &[Check::Text("result", "text \"1,234.50\"")],
    },
    Case {
        name: "NOW reads the browser's clock: =NOW()>45000 is TRUE",
        actions: &[Action::Formula("=NOW()>45000")],
        checks: &[Check::Text("result", "logical TRUE")],
    },
    Case {
        name: "the legacy form takes one value: ={1,2;3,4}*10 is 10",
        actions: &[
            Action::Choose("formula-form", "legacy"),
            Action::Formula("={1,2;3,4}*10"),
        ],
        checks: &[Check::Text("result", "number 10")],
    },
    Case {
        name: "back in the dynamic-array form the same formula is a 2 x 2 array",
        actions: &[Action::Choose("formula-form", "dynamic")],
        checks: &[Check::Array(&[&["10", "20"], &["30", "40"]])],
    },
    Case {
        name: "the function list finds ROUND by name and MROUND too",
        actions: &[
            Action::Formula("=1+"),
            Action::Into("function-filter", "round"),
        ],
        checks: &[
            Check::Contains("functions", "ROUND(number, num_digits)"),
            Check::Contains("functions", "MROUND(number, multiple)"),
        ],
    },
    Case {
        name: "a click in the function list inserts the call at the caret",
        actions: &[Action::Click(
            "[data-onecalc=\"function-entry\"][data-name=\"ROUND\"]",
        )],
        checks: &[
            Check::Value("formula", "=1+ROUND("),
            Check::Text("active-parameter", "number"),
            Check::Text("help-name", "ROUND"),
        ],
    },
    Case {
        name: "Escape dismisses the completion list",
        actions: &[Action::Formula("=AVER"), Action::Keys("formula", ESCAPE)],
        checks: &[
            Check::Absent("completions"),
            Check::Value("formula", "=AVER"),
        ],
    },
];

/// The formula field DNA Calc shares with DNA OneCalc, which carries OneCalc's role attribute.
const BAR: &str = "[data-onecalc=\"formula\"]";

/// A cell of DNA Calc's grid, by its address.
macro_rules! cell {
    ($address:literal) => {
        concat!("[data-dnacalc=\"cell\"][data-cell=\"", $address, "\"]")
    };
}

/// DNA Calc's cases, in the order each browser runs them in one page, over the new workbook it
/// starts with, then calc-doc's D1 fixture (an Excel-saved workbook) opened through the page.
const CALC_CASES: &[Case] = &[
    Case {
        name: "the page loads with a new workbook, Sheet1, at A1",
        actions: &[],
        checks: &[
            Check::Text("document", "Book1.xlsx"),
            Check::Text("[data-dnacalc=\"sheet-tab\"]", "Sheet1"),
            Check::Value("name-box", "A1"),
            Check::Attr("[data-dnacalc=\"skin-grid\"]", "aria-pressed", "true"),
        ],
    },
    Case {
        name: "typed numbers replace the cell, Enter enters them and moves down",
        actions: &[
            Action::Keys("grid", "2"),
            Action::Keys("cell-editor", ENTER),
            Action::Keys("grid", "3"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[
            Check::Text(cell!("A1"), "2"),
            Check::Text(cell!("A2"), "3"),
            Check::Attr(cell!("A1"), "data-provenance", "literal"),
            Check::Value("name-box", "A3"),
        ],
    },
    Case {
        name: "a formula entered in the cell is calculated: =A1*A2+1 is 7",
        actions: &[
            Action::Keys("grid", "="),
            Action::Keys("cell-editor", "A1*A2+1"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[
            Check::Text(cell!("A3"), "7"),
            Check::Attr(cell!("A3"), "data-provenance", "calculated"),
            Check::Attr(cell!("A3"), "data-formula", "true"),
        ],
    },
    Case {
        name: "the name box goes to a cell; the formula bar shows its formula and provenance",
        actions: &[Action::Into("name-box", "a3\u{E007}")],
        checks: &[
            Check::Value("name-box", "A3"),
            Check::Value(BAR, "=A1*A2+1"),
            Check::Starts(
                "cell-provenance",
                "Sheet1!A3: calculated by DNA Calc's engine",
            ),
        ],
    },
    Case {
        name: "an edit recalculates its dependents: A1 = 5 makes A3 16",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", "5"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[
            Check::Text(cell!("A1"), "5"),
            Check::Text(cell!("A3"), "16"),
        ],
    },
    Case {
        name: "Undo takes the edit back and Redo makes it again",
        actions: &[Action::Click("undo")],
        checks: &[Check::Text(cell!("A1"), "2"), Check::Text(cell!("A3"), "7")],
    },
    Case {
        name: "Redo",
        actions: &[Action::Click("redo")],
        checks: &[
            Check::Text(cell!("A1"), "5"),
            Check::Text(cell!("A3"), "16"),
        ],
    },
    Case {
        name: "a formula the engine refuses shows as refused, with the reason",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Keys("grid", "="),
            Action::Keys("cell-editor", "WEBSERVICE(\"http://example.com\")"),
            Action::Keys("cell-editor", ENTER),
            Action::Into("name-box", "B1\u{E007}"),
        ],
        checks: &[
            Check::Text(cell!("B1"), "refused"),
            Check::Attr(cell!("B1"), "data-provenance", "refused"),
            Check::Starts("cell-provenance", "Sheet1!B1: refused: "),
        ],
    },
    Case {
        name: "a dynamic array spills: ={1;2;3} in C1 fills C1:C3",
        actions: &[
            Action::Into("name-box", "C1\u{E007}"),
            Action::Keys("grid", "="),
            Action::Keys("cell-editor", "{1;2;3}"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[
            Check::Text(cell!("C1"), "1"),
            Check::Text(cell!("C3"), "3"),
            Check::Attr(cell!("C3"), "data-provenance", "calculated"),
        ],
    },
    Case {
        name: "the formula bar edits with signature help and enters with Enter",
        actions: &[
            Action::Into("name-box", "D1\u{E007}"),
            Action::Click(BAR),
            Action::Keys(BAR, "=SUM("),
        ],
        checks: &[
            Check::Text("[data-onecalc=\"active-parameter\"]", "number1"),
            Check::Value(BAR, "=SUM("),
        ],
    },
    Case {
        name: "the formula bar's entry is calculated: =SUM(A1:A3) is 24",
        actions: &[Action::Keys(BAR, "A1:A3)"), Action::Keys(BAR, ENTER)],
        checks: &[
            Check::Text(cell!("D1"), "24"),
            Check::Value(BAR, "=SUM(A1:A3)"),
        ],
    },
    Case {
        name: "in manual calculation an edit leaves its dependent stale",
        actions: &[
            Action::Choose("calc-mode", "manual"),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Keys("grid", "1"),
            Action::Keys("cell-editor", "0"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[
            Check::Text(cell!("A2"), "10"),
            Check::Attr(cell!("A3"), "data-provenance", "stale"),
            Check::Text(cell!("A3"), "16"),
        ],
    },
    Case {
        name: "Calculate recalculates: A3 is 51",
        actions: &[
            Action::Click("calculate"),
            Action::Choose("calc-mode", "automatic"),
        ],
        checks: &[
            Check::Text(cell!("A3"), "51"),
            Check::Attr(cell!("A3"), "data-provenance", "calculated"),
        ],
    },
    Case {
        name: "Delete clears the active cell",
        actions: &[
            Action::Into("name-box", "D1\u{E007}"),
            Action::Keys("grid", DELETE),
        ],
        checks: &[Check::Absent(cell!("D1"))],
    },
    Case {
        name: "Save writes the workbook's package",
        actions: &[Action::Click("save")],
        checks: &[
            Check::Starts("status", "Saved Book1.xlsx: "),
            Check::Text("document", "Book1.xlsx"),
        ],
    },
    Case {
        name: "the notebook lists the sheet's cells as blocks",
        actions: &[Action::Click("skin-notebook")],
        checks: &[
            Check::Starts("notebook-page", "7 cells; page 1 of 1"),
            Check::Attr(
                "[data-dnacalc=\"block\"][data-cell=\"B1\"]",
                "data-provenance",
                "refused",
            ),
        ],
    },
    Case {
        name: "a block's entry is entered and its dependents recalculate",
        actions: &[Action::Into(
            "[data-dnacalc=\"block\"][data-cell=\"A1\"] [data-dnacalc=\"block-input\"]",
            "6\u{E007}",
        )],
        checks: &[Check::Text(
            "[data-dnacalc=\"block\"][data-cell=\"A3\"] [data-dnacalc=\"block-value\"]",
            "61",
        )],
    },
    Case {
        name: "the atlas counts every formula cell, the refused one as refused",
        actions: &[Action::Click("skin-atlas")],
        checks: &[
            Check::Text(
                "atlas-totals",
                "3 formula cells: 2 calculated, 1 refused, 0 file cache, 0 stale; 66.67% calculated",
            ),
            Check::Text("atlas-mode", "Calculation: automatic"),
        ],
    },
    Case {
        name: "back in the grid, the active cell and the values are as they were",
        actions: &[Action::Click("skin-grid")],
        checks: &[
            Check::Value("name-box", "D1"),
            Check::Text(cell!("A3"), "61"),
        ],
    },
    Case {
        name: "an Excel-saved workbook opens through the file input, calculated",
        actions: &[Action::Upload(
            "open-file",
            "crates/calc-doc/tests/fixtures/cdoc_m1_2/d1.saved.xlsx",
        )],
        checks: &[
            Check::Text("document", "d1.saved.xlsx"),
            Check::Starts("status", "Opened d1.saved.xlsx: "),
            Check::Text("[data-dnacalc=\"sheet-tab\"][data-sheet=\"Calc\"]", "Calc"),
        ],
    },
    Case {
        name: "its sheet shows the engine's values, its dynamic array's spill included",
        actions: &[
            Action::Click("[data-dnacalc=\"sheet-tab\"][data-sheet=\"Calc\"]"),
            Action::Into("name-box", "U3\u{E007}"),
        ],
        checks: &[
            Check::Text(cell!("U3"), "21"),
            Check::Attr(cell!("U3"), "data-provenance", "calculated"),
            Check::Value(BAR, "=U2*3"),
        ],
    },
    Case {
        name: "the spill of SEQUENCE(3,2) at A1",
        actions: &[Action::Into("name-box", "A1\u{E007}")],
        checks: &[
            Check::Text(cell!("B2"), "4"),
            Check::Attr(cell!("B2"), "data-provenance", "calculated"),
        ],
    },
    Case {
        name: "an undo of an edit on another sheet shows that sheet and the restored cell",
        actions: &[
            Action::Click("[data-dnacalc=\"sheet-tab\"][data-sheet=\"Data\"]"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", "5"),
            Action::Keys("cell-editor", ENTER),
            Action::Click("[data-dnacalc=\"sheet-tab\"][data-sheet=\"Calc\"]"),
            Action::Click("undo"),
        ],
        checks: &[
            Check::Attr(
                "[data-dnacalc=\"sheet-tab\"][data-sheet=\"Data\"]",
                "aria-selected",
                "true",
            ),
            Check::Value("name-box", "A1"),
            Check::Text(cell!("A1"), "1"),
            Check::Value(BAR, "1"),
            Check::Starts("cell-provenance", "Data!A1: a literal value"),
        ],
    },
    Case {
        name: "F2 and Enter on the restored cell enter its own text again",
        actions: &[Action::Keys("grid", F2), Action::Keys("cell-editor", ENTER)],
        checks: &[Check::Text(cell!("A1"), "1"), Check::Value("name-box", "A2")],
    },
    Case {
        name: "New makes an empty workbook",
        actions: &[Action::Click("new")],
        checks: &[
            Check::Text("document", "Book1.xlsx"),
            Check::Absent("[data-dnacalc=\"cell\"]"),
            Check::Value("name-box", "A1"),
        ],
    },
    Case {
        name: "the grid scrolls past its first block of rows and enters a cell there",
        actions: &[
            Action::Into("name-box", "B200\u{E007}"),
            Action::Keys("grid", "9"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[Check::Text(cell!("B200"), "9"), Check::Value("name-box", "B201")],
    },
    Case {
        name: "back at the top, row 200 is out of view",
        actions: &[Action::Into("name-box", "A1\u{E007}")],
        checks: &[Check::Absent(cell!("B200")), Check::Value("name-box", "A1")],
    },
    Case {
        name: "at row 200 again, the entry is there",
        actions: &[Action::Into("name-box", "B200\u{E007}")],
        checks: &[Check::Text(cell!("B200"), "9")],
    },
    Case {
        name: "a spill fills E1:E3",
        actions: &[
            Action::Into("name-box", "E1\u{E007}"),
            Action::Keys("grid", "="),
            Action::Keys("cell-editor", "{1;2;3}"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[Check::Text(cell!("E3"), "3")],
    },
    Case {
        name: "the spill shrinks to E1:E2 and E3 is empty again",
        actions: &[
            Action::Into("name-box", "E1\u{E007}"),
            Action::Keys("grid", "="),
            Action::Keys("cell-editor", "{1;2}"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[Check::Text(cell!("E2"), "2"), Check::Absent(cell!("E3"))],
    },
    Case {
        name: "a new workbook for the range cases: A1 1, A2 2, B1 =A1*10",
        actions: &[
            Action::Click("new"),
            Action::Keys("grid", "1"),
            Action::Keys("cell-editor", ENTER),
            Action::Keys("grid", "2"),
            Action::Keys("cell-editor", ENTER),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Keys("grid", "="),
            Action::Keys("cell-editor", "A1*10"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[Check::Text(cell!("B1"), "10")],
    },
    Case {
        name: "Shift+Down selects a range and Ctrl+D fills it down, formulas relocated",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            // Shift down, Down twice, every key released (WebDriver's modifier and NULL keys).
            Action::Keys("grid", concat!("\u{E008}", "\u{E015}", "\u{E015}", "\u{E000}")),
            Action::Keys("grid", concat!("\u{E009}", "d", "\u{E000}")),
        ],
        checks: &[
            Check::Attr("selection", "data-area", "B1:B3"),
            Check::Text(cell!("B2"), "20"),
            Check::Text(cell!("B3"), "0"),
        ],
    },
    Case {
        name: "Ctrl+C copies a range and Ctrl+V pastes it with its formula relocated",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", concat!("\u{E008}", "\u{E014}", "\u{E000}")),
            Action::Keys("grid", concat!("\u{E009}", "c", "\u{E000}")),
            Action::Into("name-box", "D5\u{E007}"),
            Action::Keys("grid", concat!("\u{E009}", "v", "\u{E000}")),
        ],
        checks: &[
            Check::Text(cell!("D5"), "1"),
            Check::Text(cell!("E5"), "10"),
            Check::Value(BAR, "1"),
        ],
    },
    Case {
        name: "a click on a column header selects the column; Delete clears it",
        actions: &[
            Action::Click("[data-dnacalc=\"column-header\"][data-column=\"D\"]"),
            Action::Keys("grid", DELETE),
        ],
        checks: &[
            Check::Attr("selection", "data-area", "D1:D1048576"),
            Check::Absent(cell!("D5")),
            Check::Text(cell!("E5"), "0"),
        ],
    },
    Case {
        name: "Undo brings the column's cell back",
        actions: &[Action::Click("undo")],
        checks: &[Check::Text(cell!("D5"), "1"), Check::Text(cell!("E5"), "10")],
    },
    Case {
        name: "a row header and Insert insert a row: cells and references move down",
        actions: &[
            Action::Click("[data-dnacalc=\"row-header\"][data-row=\"2\"]"),
            Action::Click("insert-cells"),
        ],
        checks: &[
            Check::Attr("selection", "data-area", "A2:XFD2"),
            Check::Absent(cell!("A2")),
            Check::Text(cell!("A3"), "2"),
            Check::Text(cell!("B3"), "20"),
            Check::Text(cell!("D6"), "1"),
            Check::Text(cell!("E6"), "10"),
        ],
    },
    Case {
        name: "Delete deletes the row again: the cells and references move back",
        actions: &[Action::Click("delete-cells")],
        checks: &[
            Check::Text(cell!("A2"), "2"),
            Check::Text(cell!("B2"), "20"),
            Check::Text(cell!("D5"), "1"),
            Check::Text(cell!("E5"), "10"),
        ],
    },
    Case {
        name: "a column inserted before A moves everything right, and Undo takes it back",
        actions: &[
            Action::Click("[data-dnacalc=\"column-header\"][data-column=\"A\"]"),
            Action::Keys("grid", concat!("\u{E009}", "\u{E008}", "=", "\u{E000}")),
        ],
        checks: &[
            Check::Absent(cell!("A1")),
            Check::Text(cell!("B1"), "1"),
            Check::Text(cell!("C1"), "10"),
            Check::Value(BAR, ""),
        ],
    },
    Case {
        name: "Undo of the column insertion",
        actions: &[Action::Click("undo")],
        checks: &[Check::Text(cell!("A1"), "1"), Check::Text(cell!("B1"), "10")],
    },
    Case {
        name: "Ctrl+X and Ctrl+V move cells, and the formulas that read them follow",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", concat!("\u{E008}", "\u{E015}", "\u{E000}")),
            Action::Keys("grid", concat!("\u{E009}", "x", "\u{E000}")),
            Action::Into("name-box", "H1\u{E007}"),
            Action::Keys("grid", concat!("\u{E009}", "v", "\u{E000}")),
            Action::Into("name-box", "B1\u{E007}"),
        ],
        checks: &[
            Check::Absent(cell!("A1")),
            Check::Text(cell!("H1"), "1"),
            Check::Text(cell!("H2"), "2"),
            Check::Text(cell!("B1"), "10"),
            Check::Value(BAR, "=H1*10"),
        ],
    },
    Case {
        name: "Undo takes the move back",
        actions: &[Action::Click("undo"), Action::Into("name-box", "B1\u{E007}")],
        checks: &[
            Check::Text(cell!("A1"), "1"),
            Check::Absent(cell!("H1")),
            Check::Value(BAR, "=A1*10"),
        ],
    },
    Case {
        name: "a workbook's number formats show as Excel shows them",
        actions: &[Action::Upload(
            "open-file",
            "crates/calc-host/tests/fixtures/dc_09v/formats.xlsx",
        )],
        checks: &[
            Check::Text("document", "formats.xlsx"),
            Check::Text(cell!("A1"), "1,234.50"),
            Check::Attr(cell!("A1"), "data-format", "applied"),
            Check::Text(cell!("A2"), "25%"),
            Check::Text(cell!("A3"), "2024-01-31"),
            Check::Text(cell!("A4"), "(5.00)"),
            Check::Text(cell!("A10"), "1.50E+06"),
            Check::Text(cell!("A11"), "Qty 12"),
        ],
    },
    Case {
        name: "its column widths, row heights and hidden column and row lay out the grid",
        actions: &[],
        checks: &[
            // A is 117 px (16.71 characters), B 180 px (25.71), C hidden, D the default 64 px;
            // row 3 is 40 px (30 points), row 13 hidden, the others 20 px (15 points).
            Check::Attr(cell!("B1"), "style", "left:165px;top:22px;width:180px;height:20px"),
            Check::Attr(cell!("D1"), "style", "left:345px;top:22px;width:64px;height:20px"),
            Check::Attr(cell!("A3"), "style", "left:48px;top:62px;width:117px;height:40px"),
            Check::Attr(cell!("A14"), "style", "left:48px;top:282px;width:117px;height:20px"),
            // A15's negative section is [Color10]: Excel's palette entry 10.
            Check::Attr(cell!("A15"), "style", "left:48px;top:302px;width:117px;height:20px;color:#008000"),
            // Column E is 4 characters (33 px): a formatted number too wide shows ####, as in
            // Excel, and a text overflows over the empty F to I.
            Check::Text(cell!("E1"), "####"),
            Check::Text(cell!("E2"), "12"),
            Check::Attr(cell!("E3"), "style", "left:409px;top:62px;width:289px;height:40px"),
            // A4's negative section is [Red].
            Check::Attr(cell!("A4"), "style", "left:48px;top:102px;width:117px;height:20px;color:#ff0000"),
        ],
    },
    Case {
        name: "a row inserted at 2 moves the cells below with their formats and row heights",
        actions: &[
            Action::Click("[data-dnacalc=\"row-header\"][data-row=\"2\"]"),
            Action::Click("insert-cells"),
        ],
        checks: &[
            Check::Absent(cell!("A2")),
            Check::Text(cell!("A3"), "25%"),
            Check::Text(cell!("A4"), "2024-01-31"),
            // The file's 30-point row 3 is now row 4.
            Check::Attr(cell!("A4"), "style", "left:48px;top:82px;width:117px;height:40px"),
        ],
    },
    Case {
        name: "a column inserted before B takes A's width, and B's cells and widths move right",
        actions: &[
            Action::Click("[data-dnacalc=\"column-header\"][data-column=\"B\"]"),
            Action::Click("insert-cells"),
        ],
        checks: &[
            Check::Absent(cell!("B1")),
            // B is 117 px like A; the file's B (180 px) is C, its hidden C is D.
            Check::Text(cell!("C1"), "wide"),
            Check::Attr(cell!("C1"), "style", "left:282px;top:22px;width:180px;height:20px"),
            Check::Text(cell!("E1"), "after hidden"),
            Check::Attr(cell!("E1"), "style", "left:462px;top:22px;width:64px;height:20px"),
        ],
    },
    Case {
        name: "Ctrl+X and Ctrl+V move a formatted cell with its format",
        actions: &[
            Action::Into("name-box", "A3\u{E007}"),
            Action::Keys("grid", concat!("\u{E009}", "x", "\u{E000}")),
            Action::Into("name-box", "I3\u{E007}"),
            Action::Keys("grid", concat!("\u{E009}", "v", "\u{E000}")),
        ],
        checks: &[Check::Absent(cell!("A3")), Check::Text(cell!("I3"), "25%")],
    },
    Case {
        name: "Save after the insertions and the move writes the package",
        actions: &[Action::Click("save")],
        checks: &[Check::Starts("status", "Saved formats.xlsx: ")],
    },
    // dc-gf0: the tree, canvas and ledger skins over calc-host's dc_gf0 workbook (Data, Other and
    // Target Sheet, six defined names, formulas across the sheets).
    Case {
        name: "the tree outlines the workbook: its names, its sheets, their rows and cells",
        actions: &[
            Action::Upload("open-file", "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx"),
            Action::Click("skin-tree"),
        ],
        checks: &[
            Check::Text("[data-dnacalc=\"tree-row\"][data-key=\"names\"] .dc-count", "6"),
            Check::Text("[data-dnacalc=\"tree-row\"][data-key=\"sheet:Data\"] .dc-count", "13 cells"),
            Check::Text(
                "[data-dnacalc=\"tree-row\"][data-sheet=\"Data\"][data-cell=\"B3\"] [data-dnacalc=\"tree-value\"]",
                "108",
            ),
        ],
    },
    Case {
        name: "a sheet of the tree folds and unfolds",
        actions: &[Action::Click(
            "[data-dnacalc=\"tree-row\"][data-key=\"sheet:Data\"] [data-dnacalc=\"tree-toggle\"]",
        )],
        checks: &[
            Check::Attr("[data-dnacalc=\"tree-row\"][data-key=\"sheet:Data\"]", "data-folded", "true"),
            Check::Absent("[data-dnacalc=\"tree-row\"][data-sheet=\"Data\"][data-cell=\"B3\"]"),
        ],
    },
    Case {
        name: "unfolded again, a click on a cell makes it the active cell",
        actions: &[
            Action::Click(
                "[data-dnacalc=\"tree-row\"][data-key=\"sheet:Data\"] [data-dnacalc=\"tree-toggle\"]",
            ),
            Action::Click("[data-dnacalc=\"tree-row\"][data-sheet=\"Data\"][data-cell=\"B3\"]"),
        ],
        checks: &[
            Check::Value("name-box", "B3"),
            Check::Value(BAR, "=A1+Other!A1+D1"),
            Check::Attr("[data-dnacalc=\"tree-row\"][data-focused=\"true\"]", "data-cell", "B3"),
            // Other!B4, B12 and B13 sum ranges that hold Data!B3.
            Check::Text(
                "[data-dnacalc=\"tree-row\"][data-sheet=\"Data\"][data-cell=\"B3\"] [data-dnacalc=\"tree-dependents\"]",
                "read by 3 formulas",
            ),
        ],
    },
    Case {
        name: "Down and Enter in the tree move to the next cell and select it",
        actions: &[Action::Keys("tree", concat!("\u{E015}", "\u{E007}"))],
        checks: &[
            Check::Attr("[data-dnacalc=\"tree-row\"][data-focused=\"true\"]", "data-cell", "C3"),
            Check::Value("name-box", "C3"),
        ],
    },
    Case {
        name: "the canvas centres on the active cell, what it reads to the left, what reads it to the right",
        actions: &[Action::Into("name-box", "B1\u{E007}"), Action::Click("skin-canvas")],
        checks: &[
            Check::Attr("[data-dnacalc=\"canvas-node\"][data-center=\"true\"]", "data-node", "B1"),
            Check::Text(
                "[data-dnacalc=\"canvas-node\"][data-node=\"B1\"] [data-dnacalc=\"canvas-value\"]",
                "10",
            ),
            Check::Attr("[data-dnacalc=\"canvas-node\"][data-node=\"A1\"]", "data-column", "-1"),
            Check::Attr("[data-dnacalc=\"canvas-node\"][data-node=\"C1\"]", "data-column", "1"),
            Check::Attr("[data-dnacalc=\"canvas-node\"][data-node=\"Other!B3\"]", "data-column", "1"),
            Check::Text(
                "canvas-read-by",
                "Data!C1, Other!B3, Other!B4, Other!B12, Other!B13",
            ),
        ],
    },
    Case {
        name: "a click on a chip moves the canvas, and [ reaches one step further back",
        actions: &[
            Action::Click("[data-dnacalc=\"canvas-node\"][data-node=\"C1\"]"),
            Action::Keys("canvas", "["),
        ],
        checks: &[
            Check::Value("name-box", "C1"),
            Check::Text("canvas-depth", "2 back, 1 forward"),
            Check::Attr("[data-dnacalc=\"canvas-node\"][data-node=\"B1\"]", "data-column", "-1"),
            Check::Attr("[data-dnacalc=\"canvas-node\"][data-node=\"A1\"]", "data-column", "-2"),
        ],
    },
    Case {
        name: "the ledger lists every formula cell of the workbook",
        actions: &[Action::Click("skin-ledger")],
        checks: &[Check::Starts("ledger-count", "22 of 22 formula cells")],
    },
    Case {
        name: "the ledger keeps the cells holding a text",
        actions: &[Action::Choose("ledger-filter", "text"), Action::Into("ledger-text", "Other!")],
        checks: &[Check::Starts("ledger-count", "14 of 22 formula cells")],
    },
    Case {
        name: "sorted by value, the largest comes first, and a click selects it on its sheet",
        actions: &[
            Action::Choose("ledger-filter", ""),
            Action::Choose("ledger-sort", "value-desc"),
            Action::Click("[data-dnacalc=\"ledger-row\"] td"),
        ],
        checks: &[
            Check::Attr("[data-dnacalc=\"ledger-row\"]", "data-cell", "Other!B12"),
            Check::Value("name-box", "B12"),
            Check::Value(BAR, "=SUM(Data!A1:C3)"),
        ],
    },
];

/// The script that reads what one check looks at.
fn observe(app: &App, session: &Session, check: &Check) -> Result<Value, String> {
    let q = |name: &str| {
        format!(
            "document.querySelector('{}')",
            select(app, name).replace('\'', "\\'")
        )
    };
    let script = match check {
        Check::Text(name, _) | Check::Starts(name, _) => {
            format!("var e = {}; return e ? e.textContent : null;", q(name))
        }
        Check::Items(name, _) | Check::Contains(name, _) => format!(
            "var e = {}; return e ? Array.from(e.querySelectorAll('li')).map(function (l) {{ return l.textContent; }}) : null;",
            q(name)
        ),
        Check::Absent(name) => format!("return {} === null;", q(name)),
        Check::Value(name, _) => format!("var e = {}; return e ? e.value : null;", q(name)),
        Check::Attr(name, attribute, _) => format!(
            "var e = {}; return e ? e.getAttribute('{attribute}') : null;",
            q(name)
        ),
        Check::Array(_) => format!(
            "var e = {}; return e ? Array.from(e.querySelectorAll('tr')).map(function (r) {{ return Array.from(r.querySelectorAll('td')).map(function (c) {{ return c.textContent; }}); }}) : null;",
            q("result-array")
        ),
        Check::Highlighted(role_name, _) => format!(
            "return Array.from(document.querySelectorAll('[data-onecalc=\"highlight\"] [data-role=\"{role_name}\"]')).map(function (s) {{ return s.textContent; }});"
        ),
    };
    session.script(&script)
}

fn satisfied(check: &Check, seen: &Value) -> bool {
    match check {
        Check::Text(_, expected) | Check::Value(_, expected) | Check::Attr(_, _, expected) => {
            seen.as_str() == Some(expected)
        }
        Check::Starts(_, prefix) => seen.as_str().is_some_and(|text| text.starts_with(prefix)),
        Check::Items(_, expected) => seen.as_array().is_some_and(|items| {
            items.len() == expected.len()
                && items
                    .iter()
                    .zip(expected.iter())
                    .all(|(a, b)| a.as_str() == Some(b))
        }),
        Check::Contains(_, wanted) => seen.as_array().is_some_and(|items| {
            items
                .iter()
                .any(|item| item.as_str().is_some_and(|text| text.starts_with(wanted)))
        }),
        Check::Absent(_) => seen.as_bool() == Some(true),
        Check::Array(rows) => seen.as_array().is_some_and(|seen_rows| {
            seen_rows.len() == rows.len()
                && seen_rows.iter().zip(rows.iter()).all(|(seen_row, row)| {
                    seen_row.as_array().is_some_and(|cells| {
                        cells.len() == row.len()
                            && cells
                                .iter()
                                .zip(row.iter())
                                .all(|(a, b)| a.as_str() == Some(b))
                    })
                })
        }),
        Check::Highlighted(_, text) => seen
            .as_array()
            .is_some_and(|runs| runs.iter().any(|run| run.as_str() == Some(text))),
    }
}

fn describe(check: &Check) -> String {
    match check {
        Check::Text(name, expected) => format!("{name} is {expected:?}"),
        Check::Starts(name, prefix) => format!("{name} starts with {prefix:?}"),
        Check::Items(name, expected) => format!("{name} lists {expected:?}"),
        Check::Contains(name, wanted) => format!("{name} has an item starting {wanted:?}"),
        Check::Absent(name) => format!("no {name}"),
        Check::Value(name, expected) => format!("the field {name} holds {expected:?}"),
        Check::Attr(name, attribute, expected) => {
            format!("{name} has {attribute}={expected:?}")
        }
        Check::Array(rows) => format!("result-array is {rows:?}"),
        Check::Highlighted(role_name, text) => format!("{text:?} is highlighted as {role_name}"),
    }
}

/// The page's fault record: the app's panic message and every uncaught error it recorded.
fn faults(app: &App, session: &Session) -> Result<Vec<String>, String> {
    let seen = session.script(&format!(
        "var out = []; var p = document.documentElement.getAttribute('{}'); if (p !== null) {{ out.push('panic: ' + p); }} (window.{} || ['the page has no fault record']).forEach(function (f) {{ out.push(f); }}); return out;",
        app.panic_attribute, app.faults
    ))?;
    Ok(seen
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_str().unwrap_or("").to_owned())
                .collect()
        })
        .unwrap_or_else(|| vec![format!("the fault record is not a list: {seen}")]))
}

fn act(ws: &Workspace, app: &App, session: &Session, action: &Action) -> Result<(), String> {
    match action {
        Action::Grid(cell, text) => {
            let css = format!("{}[data-cell=\"{cell}\"]", select(app, "grid-cell"));
            session.clear(&css)?;
            session.keys(&css, text)
        }
        Action::Formula(text) => {
            session.clear(&select(app, "formula"))?;
            session.keys(&select(app, "formula"), text)
        }
        Action::Keys(name, keys) => session.keys(&select(app, name), keys),
        Action::Into(name, text) => {
            session.clear(&select(app, name))?;
            session.keys(&select(app, name), text)
        }
        Action::Click(name) => {
            let element = session.element(&select(app, name))?;
            session.post(&format!("/element/{element}/click"), json!({}))?;
            Ok(())
        }
        Action::Choose(name, value) => {
            let option =
                session.element(&format!("{} option[value=\"{value}\"]", select(app, name)))?;
            session.post(&format!("/element/{option}/click"), json!({}))?;
            Ok(())
        }
        Action::Upload(name, path) => {
            let path = ws.root.join(path);
            upload(app, session, name, &path)
        }
    }
}

/// Chooses the file `path` in the file input `name`.
fn upload(app: &App, session: &Session, name: &str, path: &Path) -> Result<(), String> {
    let path = path
        .canonicalize()
        .map_err(|e| format!("no file {}: {e}", path.display()))?;
    let text = path.display().to_string();
    // Windows' canonical form starts with \\?\, which a browser's file input does not take.
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned();
    session.keys(&select(app, name), &text)
}

/// Waits until `check` holds or `settle` passes; the failure, if any.
fn wait_for(app: &App, session: &Session, check: &Check, settle: Duration) -> Option<String> {
    let started = Instant::now();
    let mut last: Value;
    loop {
        match observe(app, session, check) {
            Ok(seen) if satisfied(check, &seen) => return None,
            Ok(seen) => last = seen,
            Err(e) => last = Value::String(format!("(script failed: {e})")),
        }
        if started.elapsed() > settle {
            return Some(format!("expected {}; saw {last}", describe(check)));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

/// Runs one case: its actions, then each check until it holds or [`SETTLE`] passes, then the fault
/// record. Returns the failures.
fn run_case(ws: &Workspace, app: &App, session: &Session, case: &Case) -> Vec<String> {
    let mut failures = Vec::new();
    for action in case.actions {
        if let Err(e) = act(ws, app, session, action) {
            failures.push(format!("action failed: {e}"));
            return failures;
        }
    }
    for check in case.checks {
        if let Some(failure) = wait_for(app, session, check, SETTLE) {
            failures.push(failure);
        }
    }
    match faults(app, session) {
        Ok(list) if list.is_empty() => {}
        Ok(list) => failures.extend(list.into_iter().map(|f| format!("page fault: {f}"))),
        Err(e) => failures.push(format!("cannot read the fault record: {e}")),
    }
    failures
}

/// The text of the element `name` selects, or why it could not be read.
fn text_of(app: &App, session: &Session, name: &str) -> String {
    let script = format!(
        "var e = document.querySelector('{}'); return e ? e.textContent : null;",
        select(app, name).replace('\'', "\\'")
    );
    match session.script(&script) {
        Ok(Value::String(text)) => text,
        Ok(other) => format!("(no text: {other})"),
        Err(e) => format!("(script failed: {e})"),
    }
}

/// Opens a workbook from outside the repository, enters the edit, and reports what the page says
/// about it: timings and counts only.
fn run_workbook(app: &App, session: &Session, browser: &str, run: &WorkbookRun) -> Vec<String> {
    let mut failures = Vec::new();
    let name = run
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let label = app.label;
    let profiling = run.profile.is_some() && browser == "chromium";
    if profiling {
        for (cmd, params) in [
            ("Profiler.enable", json!({})),
            ("Profiler.setSamplingInterval", json!({ "interval": 200 })),
            ("Profiler.start", json!({})),
        ] {
            if let Err(e) = session.post("/goog/cdp/execute", json!({ "cmd": cmd, "params": params })) {
                return vec![format!("the profiler did not start ({cmd}): {e}")];
            }
        }
    }
    if let Err(e) = upload(app, session, "open-file", &run.path) {
        return vec![format!("the workbook was not chosen: {e}")];
    }
    let opened = format!("Opened {name}: ");
    let started = Instant::now();
    loop {
        let status = text_of(app, session, "status");
        if status.starts_with(&opened) {
            println!("{label} WORKBOOK {browser} open: {status}");
            if profiling && let Some(path) = &run.profile {
                match session.post(
                    "/goog/cdp/execute",
                    json!({ "cmd": "Profiler.stop", "params": {} }),
                ) {
                    Ok(stopped) => match fs::write(path, stopped.to_string()) {
                        Ok(()) => println!("{label} WORKBOOK {browser} profile: {}", path.display()),
                        Err(e) => failures.push(format!("the profile was not written: {e}")),
                    },
                    Err(e) => failures.push(format!("the profiler did not stop: {e}")),
                }
            }
            break;
        }
        if status.contains("did not open") || started.elapsed() > WORKBOOK_SETTLE {
            failures.push(format!("the workbook did not open: {status}"));
            return failures;
        }
        thread::sleep(Duration::from_millis(200));
    }
    if run.skins
        && let Some(edit) = &run.edit
    {
        failures.extend(measure_skins(app, session, browser, edit));
        if !failures.is_empty() {
            return failures;
        }
    }
    if let Some((sheet, cell, text)) = run.edit.as_ref().filter(|_| !run.skins) {
        let tab = format!("[data-dnacalc=\"sheet-tab\"][data-sheet=\"{sheet}\"]");
        let split = text.chars().next().map_or(0, char::len_utf8);
        let (first, rest) = text.split_at(split);
        let started_edit = (|| {
            let element = session.element(&tab)?;
            session.post(&format!("/element/{element}/click"), json!({}))?;
            session.clear(&select(app, "name-box"))?;
            session.keys(&select(app, "name-box"), &format!("{cell}{ENTER}"))?;
            session.keys(&select(app, "grid"), first)
        })();
        if let Err(e) = started_edit {
            failures.push(format!("the edit did not start: {e}"));
            return failures;
        }
        let before = text_of(app, session, "status");
        if let Err(e) = session.keys(&select(app, "cell-editor"), &format!("{rest}{ENTER}")) {
            failures.push(format!("the edit was not entered: {e}"));
            return failures;
        }
        let started = Instant::now();
        loop {
            let status = text_of(app, session, "status");
            if status != before {
                println!(
                    "{label} WORKBOOK {browser} edit {sheet}!{cell}: {status} (seen after {:.1} s)",
                    started.elapsed().as_secs_f64()
                );
                break;
            }
            if started.elapsed() > WORKBOOK_SETTLE {
                failures.push("the edit's status never came".to_owned());
                return failures;
            }
            thread::sleep(Duration::from_millis(200));
        }
    }
    let started = Instant::now();
    if let Err(e) = act_click(app, session, "skin-atlas") {
        failures.push(e);
        return failures;
    }
    // The totals are computed once the atlas is shown; wait for a non-empty line.
    loop {
        let totals = text_of(app, session, "atlas-totals");
        if totals.contains("formula cells") {
            println!(
                "{label} WORKBOOK {browser} atlas: {totals} (shown after {:.1} s)",
                started.elapsed().as_secs_f64()
            );
            println!(
                "{label} WORKBOOK {browser} {}",
                text_of(app, session, "atlas-timings")
            );
            println!(
                "{label} WORKBOOK {browser} {}",
                text_of(app, session, "atlas-memory")
            );
            break;
        }
        if started.elapsed() > WORKBOOK_SETTLE {
            failures.push("the atlas never showed its totals".to_owned());
            break;
        }
        thread::sleep(Duration::from_millis(200));
    }
    match faults(app, session) {
        Ok(list) if list.is_empty() => {}
        Ok(list) => failures.extend(list.into_iter().map(|f| format!("page fault: {f}"))),
        Err(e) => failures.push(format!("cannot read the fault record: {e}")),
    }
    failures
}

/// The skins `--skins` times, each with the selector of what shows it is there.
const TIMED_SKINS: [(&str, &str); 4] = [
    ("grid", "[data-skin=\"grid\"]"),
    ("tree", "[data-dnacalc=\"tree-row\"][data-kind=\"cell\"]"),
    ("canvas", "[data-dnacalc=\"canvas-node\"][data-center=\"true\"]"),
    ("ledger", "[data-dnacalc=\"ledger-row\"]"),
];

/// Milliseconds in the page from the mark `window.__dcT0` until `css` selects an element, seen
/// from a timer task: one runs only once the page's work for the click or the key (the handler,
/// the intent, the reactive updates it queued) is done.
fn painted_after_mark(session: &Session, css: &str) -> Result<f64, String> {
    let script = format!(
        "var done = arguments[arguments.length - 1]; var sel = '{}';\
         function check() {{ var ms = performance.now() - window.__dcT0; if (document.querySelector(sel)) {{ done(ms); }} else if (ms > 100000) {{ done(-1); }} else {{ setTimeout(check, 2); }} }} setTimeout(check, 0);",
        css.replace('\'', "\\'")
    );
    let value = session.post("/execute/async", json!({ "script": script, "args": [] }))?;
    match value.get("value").unwrap_or(&value).as_f64() {
        Some(ms) if ms >= 0.0 => Ok(ms),
        Some(_) => Err(format!("nothing matched {css} within 100 s")),
        None => Err(format!("no time from the page: {value}")),
    }
}

/// `--skins`: shows each skin of [`TIMED_SKINS`] and enters the edit with it shown, printing the
/// times the page measured.
fn measure_skins(
    app: &App,
    session: &Session,
    browser: &str,
    (sheet, cell, text): &(String, String, String),
) -> Vec<String> {
    let label = app.label;
    if let Err(e) = session.post("/timeouts", json!({ "script": 600_000 })) {
        return vec![format!("the script timeout was not set: {e}")];
    }
    let base: f64 = text.parse().unwrap_or(1.0);
    for (index, (skin, mark)) in TIMED_SKINS.iter().enumerate() {
        let shown = (|| {
            session.script("window.__dcT0 = performance.now(); return null;")?;
            act_click(app, session, &format!("skin-{skin}"))?;
            painted_after_mark(session, mark)
        })();
        let shown = match shown {
            Ok(ms) => ms,
            Err(e) => return vec![format!("the {skin} skin did not show: {e}")],
        };
        let value = base + index as f64;
        let edited = (|| {
            // The tab is clicked in the page: a long row of tabs may hide it from a pointer.
            let tab = format!("[data-dnacalc=\"sheet-tab\"][data-sheet=\"{sheet}\"]");
            session.script(&format!(
                "document.querySelector('{}').click(); return null;",
                tab.replace('\'', "\\'")
            ))?;
            session.clear(&select(app, "name-box"))?;
            session.keys(&select(app, "name-box"), &format!("{cell}{ENTER}"))?;
            session.clear(BAR)?;
            session.keys(BAR, &value.to_string())?;
            session.script("window.__dcT0 = performance.now(); return null;")?;
            session.keys(BAR, ENTER)?;
            painted_after_mark(session, mark)
        })();
        let edited = match edited {
            Ok(ms) => ms,
            Err(e) => return vec![format!("the edit with the {skin} skin shown failed: {e}")],
        };
        println!(
            "{label} WORKBOOK {browser} skin {skin}: shown in {shown:.0} ms; edit {sheet}!{cell}={value} done in {edited:.0} ms; {}",
            text_of(app, session, "status")
        );
    }
    Vec::new()
}

fn act_click(app: &App, session: &Session, name: &str) -> Result<(), String> {
    let element = session.element(&select(app, name))?;
    session.post(&format!("/element/{element}/click"), json!({}))?;
    Ok(())
}

/// Runs every case in one browser; returns (passed, failed) or why the browser could not run.
fn run_browser(
    ws: &Workspace,
    app: &App,
    browser: &Browser,
    page: &str,
    workbook: Option<&WorkbookRun>,
) -> Result<(usize, usize), String> {
    let (_driver, session) = open(browser)?;
    let outcome = (|| {
        session.post("/url", json!({ "url": page }))?;
        let started = Instant::now();
        loop {
            let ready = session.script(&format!(
                "return document.documentElement.getAttribute('{}');",
                app.ready_attribute
            ))?;
            if ready.as_str() == Some("true") {
                break;
            }
            if started.elapsed() > Duration::from_secs(30) {
                let faults = faults(app, &session).unwrap_or_default();
                return Err(format!(
                    "the app did not start within 30 s; faults {faults:?}"
                ));
            }
            thread::sleep(Duration::from_millis(100));
        }
        let (mut passed, mut failed) = (0, 0);
        for case in app.cases {
            let failures = run_case(ws, app, &session, case);
            if failures.is_empty() {
                passed += 1;
                println!("{} SMOKE {} passed: {}", app.label, browser.name, case.name);
            } else {
                failed += 1;
                println!("{} SMOKE {} failed: {}", app.label, browser.name, case.name);
                for failure in failures {
                    println!("    {failure}");
                }
            }
        }
        if let Some(run) = workbook {
            let failures = run_workbook(app, &session, browser.name, run);
            if failures.is_empty() {
                passed += 1;
                println!(
                    "{} SMOKE {} passed: the workbook opened and was edited",
                    app.label, browser.name
                );
            } else {
                failed += 1;
                println!("{} SMOKE {} failed: the workbook", app.label, browser.name);
                for failure in failures {
                    println!("    {failure}");
                }
            }
        }
        Ok((passed, failed))
    })();
    session.end();
    outcome
}

fn smoke(
    ws: &Workspace,
    app: &App,
    bundle_dir: Option<PathBuf>,
    only: Option<&str>,
    workbook: Option<&WorkbookRun>,
) -> ExitCode {
    let dir = match bundle_dir {
        Some(dir) => dir,
        None => {
            let dir = ws.root.join(app.default_out);
            if let Err(e) = bundle(ws, app, &dir) {
                println!("{} RESULT failed: {e}", app.label);
                return ExitCode::from(1);
            }
            dir
        }
    };
    let port = match serve(dir.clone()) {
        Ok(port) => port,
        Err(e) => {
            println!("{} RESULT failed: {e}", app.label);
            return ExitCode::from(1);
        }
    };
    let page = format!("http://127.0.0.1:{port}/index.html");
    println!("{} serving {} at {page}", app.label, dir.display());
    let mut all_passed = true;
    let mut summary = Vec::new();
    for browser in BROWSERS
        .iter()
        .filter(|b| only.is_none_or(|name| b.name == name))
    {
        match run_browser(ws, app, browser, &page, workbook) {
            Ok((passed, failed)) => {
                all_passed &= failed == 0;
                summary.push(format!(
                    "{}: {passed} passed, {failed} failed",
                    browser.name
                ));
            }
            Err(e) => {
                all_passed = false;
                println!("{} SMOKE {} did not run: {e}", app.label, browser.name);
                summary.push(format!("{}: did not run", browser.name));
            }
        }
    }
    if summary.is_empty() {
        println!("{} RESULT failed: no browser named {only:?}", app.label);
        return ExitCode::from(2);
    }
    println!(
        "{} RESULT {}: {} cases per browser{}; {}",
        app.label,
        if all_passed { "passed" } else { "failed" },
        app.cases.len(),
        if workbook.is_some() {
            " and the workbook"
        } else {
            ""
        },
        summary.join("; ")
    );
    if all_passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunked_bodies_join() {
        assert_eq!(
            unchunk(b"4\r\nWiki\r\n5;x=1\r\npedia\r\n0\r\n\r\n").unwrap(),
            b"Wikipedia"
        );
        assert!(unchunk(b"zz\r\n").is_err());
    }

    #[test]
    fn checks_compare_what_the_page_shows() {
        assert!(satisfied(&Check::Text("r", "a"), &json!("a")));
        assert!(!satisfied(&Check::Text("r", "a"), &Value::Null));
        assert!(satisfied(&Check::Items("r", &[]), &json!([])));
        assert!(!satisfied(&Check::Items("r", &[]), &Value::Null));
        assert!(satisfied(
            &Check::Array(&[&["1", "2"]]),
            &json!([["1", "2"]])
        ));
        assert!(!satisfied(&Check::Array(&[&["1", "2"]]), &json!([["1"]])));
        assert!(satisfied(
            &Check::Contains("r", "SU"),
            &json!(["AB", "SUM"])
        ));
        assert!(satisfied(&Check::Attr("r", "x", "1"), &json!("1")));
        assert!(!satisfied(&Check::Value("r", "1"), &Value::Null));
    }

    #[test]
    fn roles_and_selectors_select() {
        assert_eq!(select(&APPS[0], "formula"), "[data-onecalc=\"formula\"]");
        assert_eq!(select(&APPS[1], "grid"), "[data-dnacalc=\"grid\"]");
        assert_eq!(select(&APPS[1], BAR), BAR);
        assert_eq!(cell!("A3"), "[data-dnacalc=\"cell\"][data-cell=\"A3\"]");
    }

    #[test]
    fn an_edit_names_a_sheet_a_cell_and_a_text() {
        assert_eq!(
            parse_edit("'Inputs 2'!B5=12.5"),
            Some(("Inputs 2".to_owned(), "B5".to_owned(), "12.5".to_owned()))
        );
        assert_eq!(
            parse_edit("Calc!C3==A1*2"),
            Some(("Calc".to_owned(), "C3".to_owned(), "=A1*2".to_owned()))
        );
        assert_eq!(parse_edit("B5=1"), None);
        assert_eq!(parse_edit("Sheet1!B5"), None);
    }
}
