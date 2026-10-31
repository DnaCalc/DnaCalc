//! `cargo xtask app bundle <app> [--out <dir>]` and `cargo xtask app smoke <app> [--bundle <dir>]
//! [--browser chromium|firefox] [--workbook <file> [--edit <sheet>!<cell>=<text>]]`: the browser
//! bundle of an application and its smoke test, for DNA OneCalc (`onecalc`) and DNA Calc
//! (`calc`). `cargo xtask onecalc bundle|smoke ...` is the same command for `onecalc`.
//!
//! `bundle` builds the app's browser entry (`apps/<package>/src/bin/<package>-web.rs`) for
//! wasm32-unknown-unknown in the workspace's `web` profile (release, optimised for size), runs
//! wasm-bindgen 0.2.117 (`--target web`) over it, and copies `apps/<package>/web/index.html` and its
//! start module `web/boot.js` (the fault record and the loader, so the page holds no inline script)
//! beside the output, into `target/<app>-bundle` unless `--out` names another folder. It prints each
//! file's size and the total. Nothing is published: the folder is served only by `smoke`, on
//! 127.0.0.1.
//!
//! `smoke` builds the bundle (or takes the one `--bundle` names), serves it on 127.0.0.1 from a
//! port the system picks (or drives the page `--url` names: `cargo xtask site smoke` serves a
//! packed release with the site's headers, dc-ysl), and drives it in headless Chrome and headless Firefox through their
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
use crate::site::SiteHeaders;
use serde_json::{Value, json};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub const USAGE: &str = "usage: cargo xtask app bundle onecalc|calc [--out <dir>]\n       cargo xtask app smoke onecalc|calc [--bundle <dir> | --url <page>] [--browser chromium|firefox] [--workbook <file> [--edit <sheet>!<cell>=<text>] [--skins] [--profile <cpuprofile.json>]]\n       cargo xtask onecalc bundle|smoke ... (the same, for onecalc)";

/// One browser application: its package, its entry, its page, and how its page reports itself.
#[derive(Clone)]
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
    /// Further scripts copied beside the page, relative to the workspace root (DNA Calc's
    /// calculation worker, `web/worker.js`, dc-p96).
    pub scripts: &'static [&'static str],
    /// The stylesheets, relative to the workspace root, in order: the bundle's `app.css` is
    /// Strand's token stylesheet (`dna_strand::stylesheet`) followed by these (dc-xib).
    pub styles: &'static [&'static str],
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

impl App {
    /// The page's start module (`web/boot.js` beside the page), relative to the workspace root.
    pub fn boot(&self) -> String {
        self.page.replace("index.html", "boot.js")
    }
}

/// The applications.
pub static APPS: [App; 2] = [
    App {
        key: "onecalc",
        label: "ONECALC",
        package: "dna-onecalc",
        entry: "dna-onecalc-web",
        page: "apps/dna-onecalc/web/index.html",
        scripts: &[],
        styles: &[
            "apps/dna-formula-ui/web/css/formula-field.css",
            "apps/dna-onecalc/web/css/surface.css",
        ],
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
        scripts: &["apps/dna-calc/web/worker.js"],
        styles: &[
            "apps/dna-formula-ui/web/css/formula-field.css",
            "apps/dna-calc/web/css/shell.css",
            "apps/dna-calc/web/css/grid.css",
            "apps/dna-calc/web/css/notebook.css",
            "apps/dna-calc/web/css/atlas.css",
            "apps/dna-calc/web/css/tree.css",
            "apps/dna-calc/web/css/canvas.css",
            "apps/dna-calc/web/css/ledger.css",
        ],
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
pub(crate) struct WorkbookRun {
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
            let mut url: Option<String> = None;
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
                    ("--url", Some(page)) => url = Some(page.clone()),
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
            let target = match (url, bundle_dir) {
                (Some(_), Some(_)) => return usage(raw),
                (Some(url), None) => SmokeTarget::Url(url),
                (None, dir) => SmokeTarget::Bundle(dir),
            };
            smoke(&ws, app, target, only.as_deref(), workbook.as_ref())
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
pub(crate) fn bundle(ws: &Workspace, app: &App, out: &Path) -> Result<Vec<(String, u64)>, String> {
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
    let boot = app.boot();
    fs::copy(ws.root.join(&boot), out.join("boot.js"))
        .map_err(|e| format!("cannot copy {boot}: {e}"))?;
    for script in app.scripts {
        let name = Path::new(script)
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("{script} has no file name"))?;
        fs::copy(ws.root.join(script), out.join(name))
            .map_err(|e| format!("cannot copy {script}: {e}"))?;
    }
    let mut sheet = dna_strand::stylesheet();
    for style in app.styles {
        let text = fs::read_to_string(ws.root.join(style))
            .map_err(|e| format!("cannot read {style}: {e}"))?;
        sheet.push_str(&text);
    }
    fs::write(out.join("app.css"), sheet).map_err(|e| format!("cannot write app.css: {e}"))?;
    let mut files = Vec::new();
    for name in [
        "index.html".to_owned(),
        "app.css".to_owned(),
        "boot.js".to_owned(),
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

/// Serves the files of `dir` on 127.0.0.1 from a thread; returns the port. With `headers`, every
/// answer carries the headers the site's Caddy snippet sets for its path (`cargo xtask site
/// serve`, dc-ysl.4); without, `Cache-Control: no-store` alone.
pub(crate) fn serve(dir: PathBuf, headers: Option<Arc<SiteHeaders>>) -> Result<u16, String> {
    serve_on(dir, headers, 0)
}

/// [`serve`] on a given port (0: one the system picks).
pub(crate) fn serve_on(
    dir: PathBuf,
    headers: Option<Arc<SiteHeaders>>,
    port: u16,
) -> Result<u16, String> {
    serve_mode(dir, headers, port, false, false)
}

fn serve_mode(
    dir: PathBuf,
    headers: Option<Arc<SiteHeaders>>,
    port: u16,
    worker_fault: bool,
    worker_latency: bool,
) -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|e| format!("cannot listen on 127.0.0.1:{port}: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("no local address: {e}"))?
        .port();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let dir = dir.clone();
            let headers = headers.clone();
            thread::spawn(move || {
                let _ = answer(
                    stream,
                    &dir,
                    headers.as_deref(),
                    worker_fault,
                    worker_latency,
                );
            });
        }
    });
    Ok(port)
}

/// The content type of a served file, by its extension, as Caddy's types give it.
pub(crate) fn content_type(name: &str) -> &'static str {
    match Path::new(name).extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("json") => "application/json",
        Some("txt") => "text/plain; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        _ => match name.rsplit('/').next() {
            Some("LICENSE") => "text/plain; charset=utf-8",
            _ => "application/octet-stream",
        },
    }
}

/// The file a request path names under the served folder: `/` is `index.html`; every segment is
/// plain (letters, digits, `-`, `_`, `.`), none is empty or `..`.
fn served_name(path: &str) -> Option<String> {
    let path = path.split('?').next().unwrap_or("/");
    let name = match path.trim_start_matches('/') {
        "" => "index.html".to_owned(),
        name => name.to_owned(),
    };
    let plain = name.split('/').all(|segment| {
        !segment.is_empty()
            && segment != ".."
            && segment != "."
            && segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    });
    plain.then_some(name)
}

/// Answers one request: a file of the folder by its path, `/` as `index.html`.
fn answer(
    mut stream: TcpStream,
    dir: &Path,
    headers: Option<&SiteHeaders>,
    worker_fault: bool,
    worker_latency: bool,
) -> std::io::Result<()> {
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
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_owned();
    let name = served_name(&path);
    let body = if worker_fault && name.as_deref() == Some("worker.js") {
        // Tool-only network fault: exercise the actual Worker error path with no product hook.
        thread::sleep(Duration::from_secs(3));
        Some(b"setTimeout(()=>postMessage('{}'),200); throw new Error('Smoke worker startup failure');".to_vec())
    } else if worker_latency && name.as_deref() == Some("smoke-latency.js") {
        Some(br#"const NativeWorker=window.Worker;window.Worker=class extends NativeWorker {set onmessage(fn){this.handler=fn;super.onmessage=fn?event=>setTimeout(()=>fn.call(this,event),300):null;}get onmessage(){return this.handler;}};"#.to_vec())
    } else if worker_latency && name.as_deref() == Some("index.html") {
        fs::read_to_string(dir.join("index.html")).ok().map(|html| {
            html.replacen(
                "<head>",
                "<head><script src=\"smoke-latency.js\"></script>",
                1,
            )
            .into_bytes()
        })
    } else {
        name.as_ref().and_then(|name| fs::read(dir.join(name)).ok())
    };
    let mut extra = String::new();
    match headers {
        Some(headers) => {
            for (header, value) in headers.for_path(path.split('?').next().unwrap_or("/")) {
                extra.push_str(&format!("{header}: {value}\r\n"));
            }
        }
        None => extra.push_str("Cache-Control: no-store\r\n"),
    }
    match (name, body) {
        (Some(name), Some(body)) => {
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n",
                content_type(&name),
                body.len()
            )?;
            stream.write_all(&body)?;
        }
        _ => {
            write!(
                stream,
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n{extra}Connection: close\r\n\r\n"
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
    downloads: PathBuf,
    bidi: Option<String>,
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

    /// W3C browsingContext.setViewport, which changes CSS viewport dimensions rather
    /// than resizing Firefox's desktop window (whose minimum width is 500 px).
    fn viewport(&self, width: u32, height: u32) -> Result<(), String> {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (width, height);
            Err("WebDriver viewport control is native-only".to_owned())
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let url = self
                .bidi
                .as_deref()
                .ok_or("the driver supplied no BiDi connection")?;
            let (mut socket, _) =
                tungstenite::connect(url).map_err(|e| format!("BiDi connect: {e}"))?;
            if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_mut() {
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .map_err(|e| e.to_string())?;
                stream
                    .set_write_timeout(Some(Duration::from_secs(10)))
                    .map_err(|e| e.to_string())?;
            }
            let mut command = |id: u64, method: &str, params: Value| -> Result<Value, String> {
                socket
                    .send(tungstenite::Message::Text(
                        json!({"id":id,"method":method,"params":params})
                            .to_string()
                            .into(),
                    ))
                    .map_err(|e| e.to_string())?;
                loop {
                    let message = socket.read().map_err(|e| e.to_string())?;
                    let tungstenite::Message::Text(text) = message else {
                        continue;
                    };
                    let value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                    if value.get("id").and_then(Value::as_u64) == Some(id) {
                        if value.get("type").and_then(Value::as_str) == Some("success") {
                            return Ok(value["result"].clone());
                        }
                        return Err(format!("BiDi {method}: {value}"));
                    }
                }
            };
            let tree = command(1, "browsingContext.getTree", json!({}))?;
            let context = tree
                .pointer("/contexts/0/context")
                .and_then(Value::as_str)
                .ok_or("BiDi returned no top-level context")?;
            command(
                2,
                "browsingContext.setViewport",
                json!({"context":context,"viewport":{"width":width,"height":height},"devicePixelRatio":1}),
            )?;
            let _ = socket.close(None);
            Ok(())
        }
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
    let downloads =
        std::env::temp_dir().join(format!("dnacalc-smoke-{}-{port}", std::process::id()));
    fs::create_dir_all(&downloads)
        .map_err(|error| format!("cannot create smoke download folder: {error}"))?;
    let capabilities = match browser.name {
        "firefox" => json!({
            "browserName": "firefox",
            "webSocketUrl": true,
            "moz:firefoxOptions": { "args": ["-headless", "-width=1280", "-height=1000"], "prefs": {
                "browser.download.folderList":2,"browser.download.dir":downloads,"browser.download.useDownloadDir":true,
                "browser.helperApps.neverAsk.saveToDisk":"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet,application/octet-stream"
            } }
        }),
        _ => json!({
            "browserName": "chrome",
            "goog:chromeOptions": { "args": ["--headless=new", "--disable-gpu", "--window-size=1280,1000"], "prefs": {"download.default_directory":downloads,"download.prompt_for_download":false} }
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
        downloads,
        bidi: created
            .pointer("/capabilities/webSocketUrl")
            .and_then(Value::as_str)
            .map(str::to_owned),
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
const F6: &str = "\u{E036}";
const F2: &str = "\u{E032}";

/// The selector of an element: a selector as written when `name` starts with `[` (or is `body`), otherwise the
/// element whose role attribute is `name`.
fn select(app: &App, name: &str) -> String {
    if name.starts_with('[') || name.starts_with('#') || name == "body" {
        name.to_owned()
    } else {
        format!("[{}=\"{name}\"]", app.role_attribute)
    }
}

/// What a case does. A name is a role or a selector ([`select`]).
pub enum Action {
    /// A native wheel gesture, with observation of its completed scroll.
    ScrollGrid(i32, i32),
    /// Remember actual view/geometry for a later scroll or skin/sheet round trip.
    RememberView(&'static str),
    /// Put a table on the real system clipboard by copying from a temporary external editor.
    ExternalClipboard(&'static str, bool),
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
    /// Two real pointer clicks at the element's centre.
    DoubleClick(&'static str),
    ContextClick(&'static str, bool),
    OpenFieldLink(&'static str),
    RememberFieldLink(&'static str),
    OpenRememberedLink,
    /// Two real clicks on a trailing row or column header boundary.
    DoubleClickEdge(&'static str, bool),
    CtrlClick(&'static str),
    /// Drag beyond the grid and hold, using real pointer actions.
    DragOutside(&'static str),
    /// An explicit discard decision when replacing a smoke fixture; clean work needs none.
    DiscardReplacement,
    /// Only files in this browser session's own temporary download folder.
    ClearDownloads,
    UploadDownload(&'static str),
    /// Prepare an explicitly manual cache fixture with the existing package-part editor.
    ManualDownloadFixture,
    ProtectedDownloadFixture,
    /// Observe delayed external events in fault cases.
    PauseMillis(u64),
    /// A real pointer drag between two rendered elements.
    Drag(&'static str, &'static str),
    DragToCell(&'static str, u32, &'static str),
    /// Drag a visible trailing header edge, optionally cancelling before release.
    ResizeHeader(&'static str, bool, i32, bool),
    /// Synthetic IME events exercise the composition boundary; this is not a device IME test.
    Composing(&'static str, &'static str),
    CompositionEnd(&'static str),
    /// Wait for a worker-backed projection before the next gesture.
    Wait(Check),
    /// Chooses this file (relative to the workspace root) in the file input.
    Upload(&'static str, &'static str),
    /// Sets the browser window's size (WebDriver's Set Window Rect): a narrow screen (dc-xib).
    Window(u32, u32),
}

/// What a case expects the page to show. A name is a role or a selector ([`select`]).
pub enum Check {
    PinnedCell,
    Before(&'static str, &'static str),
    ViewRetained,
    FitsText(&'static str),
    TextContains(&'static str, &'static str),
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
    /// The element owns keyboard focus.
    Focused(&'static str),
    /// Browser accessibility computation through the standard WebDriver endpoints.
    Role(&'static str, &'static str),
    Name(&'static str, &'static str),
    /// The whole element fits within the viewport, with rendered, reachable content.
    Reachable(&'static str),
    /// The selected element's solid text/background colours meet WCAG's normal-text ratio.
    Contrast(&'static str),
    Bounded(&'static str, usize),
    Count(&'static str, usize),
    ActiveDescendant(&'static str),
    DragExtended,
    PageAligned,
    ScrollAtEnd,
    /// The element's attribute.
    Attr(&'static str, &'static str, &'static str),
    /// The element's inline style sets each of these declarations (`"left:4px;color:#ff0000"`),
    /// compared as the browser holds them: each value is read back from the element's style and
    /// from a probe element given the expected value, so `#ff0000` and `rgb(255, 0, 0)` agree.
    /// The skins set styles through the CSS object model (dc-xib), whose serialisation is the
    /// browser's own.
    Style(&'static str, &'static str),
    /// The element's computed style has each of these declarations, compared as [`Check::Style`]
    /// compares (a probe element in the page gives the expected value its computed form).
    Computed(&'static str, &'static str),
    /// The page fits the window's width: no horizontal scroll of the document.
    Fits,
    /// The page's viewport is at most this many pixels wide (a narrow-screen case really is one).
    Narrow(u32),
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
        name: "the complete user-function registry gives an unknown name #NAME?",
        actions: &[Action::Formula("=NOSUCH(1)")],
        checks: &[
            Check::Text("result", "error #NAME?"),
            Check::Text("result-value", "#NAME?"),
            Check::Items("diagnostics", &[]),
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
            Check::Text("help-source", "Source: Microsoft"),
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
    // dc-xib: Strand's themes and a narrow screen.
    Case {
        name: "the theme switch shows the dark theme, and the system's again",
        actions: &[Action::Choose("theme", "cockpit-dark")],
        checks: &[
            Check::Computed("[data-onecalc=\"surface\"]", "color:#E2ECEF"),
            Check::Computed("body", "background-color:#1A262B"),
        ],
    },
    Case {
        name: "high contrast, then the system's theme",
        actions: &[
            Action::Choose("theme", "high-contrast"),
            Action::Choose("theme", ""),
        ],
        checks: &[Check::Computed("body", "background-color:#FFFFFF")],
    },
    Case {
        name: "on a 500-pixel screen (the narrowest a desktop Firefox window goes) the page fits its width",
        actions: &[Action::Window(500, 844)],
        checks: &[
            Check::Narrow(500),
            Check::Fits,
            Check::Text("sheets", "Sheet1"),
        ],
    },
    Case {
        name: "back on a wide screen",
        actions: &[Action::Window(1280, 1000)],
        checks: &[Check::Fits],
    },
    // dc-snz.3: the shipped page uses the same registry and bounded interpreter as the controller.
    Case {
        name: "load a VBA module and evaluate Add2(A1,2)",
        actions: &[
            Action::Click("vba-tools"),
            Action::Grid("A1", "40"),
            Action::Click("[data-onecalc=\"vba-load\"]"),
            Action::Formula("=Add2(A1,2)"),
        ],
        checks: &[
            Check::Text("result", "number 42"),
            Check::Starts("vba-status", "Loaded:"),
            Check::Items("diagnostics", &[]),
        ],
    },
    Case {
        name: "reload the VBA module and replace its code",
        actions: &[
            Action::Into(
                "vba-source",
                "Public Function Add2(a, b)\n Add2 = a + b + 10\nEnd Function\n",
            ),
            Action::Click("[data-onecalc=\"vba-load\"]"),
        ],
        checks: &[Check::Text("result", "number 52")],
    },
    Case {
        name: "an infinite VBA loop stops with a worksheet error and diagnostic",
        actions: &[
            Action::Into(
                "vba-source",
                "Public Function Spin()\n On Error Resume Next\n Do\n Loop\nEnd Function\n",
            ),
            Action::Click("[data-onecalc=\"vba-load\"]"),
            Action::Formula("=Spin()"),
        ],
        checks: &[
            Check::Text("result-value", "#VALUE!"),
            Check::Contains("diagnostics", "SPIN: execution budget"),
        ],
    },
    Case {
        name: "unloading a VBA module removes its functions",
        actions: &[Action::Click("[data-onecalc=\"vba-clear\"]")],
        checks: &[
            Check::Text("result-value", "#NAME?"),
            Check::Text("vba-status", "No VBA module loaded."),
        ],
    },
    Case {
        name: "sessions: replacing unexported inputs asks and Escape preserves them",
        actions: &[
            Action::Click("fixture-tools"),
            Action::Click("fixture-new"),
            Action::Wait(Check::Role("fixture-dialog", "dialog")),
            Action::Keys("fixture-keep", ESCAPE),
        ],
        checks: &[
            Check::Value("formula", "=Spin()"),
            Check::Focused("formula"),
            Check::Text("fixture-state", "Unexported changes"),
        ],
    },
    Case {
        name: "workbench: a blank session offers a working SUM example with blank A3",
        actions: &[
            Action::Click("fixture-new"),
            Action::Click("fixture-replace"),
            Action::Wait(Check::Text("result-state", "Ready")),
            Action::Click("try-example"),
        ],
        checks: &[
            Check::Text("result-value", "3.5"),
            Check::Text("result-state", "Calculated"),
            Check::Value("[data-onecalc=grid-cell][data-cell=A3]", ""),
            Check::Absent("[data-onecalc=result-details][open]"),
            Check::Count(
                "[data-onecalc=primary-result] [data-onecalc=result-value]",
                1,
            ),
            Check::Reachable("result-value"),
        ],
    },
    Case {
        name: "workbench: format and locale controls render the real calculation through the shared formatter",
        actions: &[
            Action::Click("format-tools"),
            Action::Choose("locale", "de-DE"),
            Action::Into("number-format", "0,00"),
        ],
        checks: &[
            Check::Text("result-value", "3,50"),
            Check::Text("result", "number 3.5"),
            Check::Text("format-note", ""),
            Check::Text("fixture-state", "Unexported changes"),
        ],
    },
    Case {
        name: "sessions: JSON export starts a real file download and records its snapshot",
        actions: &[Action::ClearDownloads, Action::Click("fixture-export")],
        checks: &[
            Check::Starts("fixture-status", "Download started: onecalc-session.json."),
            Check::Text("fixture-state", "Unchanged snapshot"),
        ],
    },
    Case {
        name: "workbench: a worksheet error is a calculated error with its meaning",
        actions: &[Action::Formula("=1/0")],
        checks: &[
            Check::Text("result-state", "Worksheet error"),
            Check::Text("result-value", "#DIV/0!"),
            Check::Text("result-error", "division by zero"),
            Check::Text("fixture-state", "Unexported changes"),
        ],
    },
    Case {
        name: "sessions: file import waits before discarding a changed formula",
        actions: &[
            Action::UploadDownload("onecalc-session.json"),
            Action::Wait(Check::Role("fixture-dialog", "dialog")),
            Action::Click("fixture-keep"),
        ],
        checks: &[
            Check::Value("formula", "=1/0"),
            Check::Text("result-value", "#DIV/0!"),
        ],
    },
    Case {
        name: "sessions: file restore restores grid inputs, formula, locale and format and recalculates",
        actions: &[
            Action::UploadDownload("onecalc-session.json"),
            Action::Click("fixture-replace"),
        ],
        checks: &[
            Check::Value("formula", "=SUM(A1:A3)"),
            Check::Value("locale", "de-DE"),
            Check::Value("number-format", "0,00"),
            Check::Value("[data-onecalc=grid-cell][data-cell=A1]", "1.5"),
            Check::Text("result-value", "3,50"),
            Check::Text("result-state", "Calculated"),
            Check::Text("fixture-state", "Unchanged snapshot"),
        ],
    },
    Case {
        name: "sessions: opening a shared fragment in this tab protects and replaces changed inputs",
        actions: &[
            Action::Click("fixture-share"),
            Action::RememberFieldLink("share-link"),
            Action::Formula("=1/0"),
            Action::OpenRememberedLink,
            Action::Wait(Check::Role("fixture-dialog", "dialog")),
            Action::Click("fixture-replace"),
        ],
        checks: &[
            Check::Value("formula", "=SUM(A1:A3)"),
            Check::Text("result-value", "3,50"),
            Check::Text("fixture-state", "Unchanged snapshot"),
        ],
    },
    Case {
        name: "sessions: a generated link restores its authored session in a fresh page",
        actions: &[
            Action::Click("fixture-share"),
            Action::OpenFieldLink("share-link"),
            Action::Wait(Check::Text("result-value", "3,50")),
            Action::Keys("formula", "\u{E009}z\u{E000}"),
        ],
        checks: &[
            Check::Value("formula", "=SUM(A1:A3)"),
            Check::Value("locale", "de-DE"),
            Check::Text("result-state", "Calculated"),
            Check::Text("fixture-state", "Unchanged snapshot"),
        ],
    },
    Case {
        name: "workbench: the array example uses the shared bounded result window",
        actions: &[
            Action::Click("fixture-tools"),
            Action::Click("example-array"),
        ],
        checks: &[
            Check::Array(&[&["1", "2"], &["3", "4"], &["5", "6"]]),
            Check::Text("result-state", "Calculated"),
        ],
    },
    Case {
        name: "workbench: the LAMBDA example executes with shared formula assistance",
        actions: &[
            Action::Click("example-lambda"),
            Action::Click("fixture-replace"),
        ],
        checks: &[
            Check::Text("result-value", "42"),
            Check::Text("result-state", "Calculated"),
            Check::Highlighted("function", "LAMBDA"),
        ],
    },
    Case {
        name: "workbench: a missing fixture input names the cell that needs a value",
        actions: &[
            Action::Click("example-missing"),
            Action::Click("fixture-replace"),
        ],
        checks: &[
            Check::Text("result-state", "Missing input"),
            Check::Contains("diagnostics", "missing input: Sheet1!Z99 has no input"),
        ],
    },
    Case {
        name: "workbench: typed input controls have names and resolve the missing value",
        actions: &[
            Action::Click("add-input"),
            Action::Into("input-cell", "Z99"),
            Action::Into("input-value", "4"),
        ],
        checks: &[
            Check::Text("result-value", "5"),
            Check::Name("input-cell", "Input 1 cell"),
            Check::Name("input-value", "Input 1 value"),
            Check::Text("result-state", "Calculated"),
        ],
    },
    Case {
        name: "sessions: links also restore typed inputs through the normal reader",
        actions: &[
            Action::Click("fixture-share"),
            Action::OpenFieldLink("share-link"),
            Action::Wait(Check::Text("result-value", "5")),
        ],
        checks: &[
            Check::Value("input-cell", "Z99"),
            Check::Value("input-value", "4"),
            Check::Text("result-state", "Calculated"),
        ],
    },
    Case {
        name: "sessions: an unqualified formula-address draft restores its effective sheet",
        actions: &[
            Action::Into("sheet-name", "Inputs"),
            Action::Click("add-sheet"),
            Action::Into("formula-cell", "Inputs!H1"),
            Action::Into("formula-cell", "H1"),
            Action::Click("fixture-tools"),
            Action::Click("fixture-share"),
            Action::OpenFieldLink("share-link"),
        ],
        checks: &[
            Check::Text("result-value", "5"),
            Check::Text("formula-cell-status", "Inputs!H1"),
            Check::Value("formula-cell", "H1"),
        ],
    },
    Case {
        name: "sessions: an invalid formula-address draft also retains its effective anchor",
        actions: &[
            Action::Into("formula-cell", "invalid!"),
            Action::Click("fixture-tools"),
            Action::Click("fixture-share"),
            Action::OpenFieldLink("share-link"),
        ],
        checks: &[
            Check::Text("result-value", "5"),
            Check::TextContains("formula-cell-status", "the formula stays in Inputs!H1"),
            Check::Value("formula-cell", "invalid!"),
        ],
    },
    Case {
        name: "sessions: invalid JSON is refused before the current formula or inputs change",
        actions: &[
            Action::Click("fixture-tools"),
            Action::Into("fixture-json", "this is not JSON"),
            Action::Click("fixture-restore"),
        ],
        checks: &[
            Check::Starts("fixture-status", "Invalid formula-session JSON:"),
            Check::Text("result-value", "5"),
            Check::Value("input-value", "4"),
            Check::Absent("[data-onecalc=fixture-dialog][open]"),
        ],
    },
    Case {
        name: "workbench: unknown names or calls, missing input and other worksheet errors remain distinct",
        actions: &[Action::Formula("=NO_SUCH_ONECALC_FUNCTION()")],
        checks: &[
            Check::Text("result-state", "Unknown name or function"),
            Check::Text("result-value", "#NAME?"),
            Check::Text("result-error", "a name that is not recognised"),
        ],
    },
    Case {
        name: "workbench: phone layout keeps the primary result and inputs ahead of the catalogue",
        actions: &[
            Action::Formula("=Z99+1"),
            Action::Click("fixture-tools"),
            Action::Window(390, 844),
            Action::Click("result-value"),
        ],
        checks: &[
            Check::Narrow(390),
            Check::Fits,
            Check::Reachable("primary-result"),
            Check::Before("grid", "functions"),
            Check::Text("result-value", "5"),
        ],
    },
    Case {
        name: "workbench: restore desktop after the phone case",
        actions: &[Action::Window(1280, 1000)],
        checks: &[Check::Fits],
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
        name: "the calculation worker starts from its own script and answers (dc-p96)",
        actions: &[],
        checks: &[Check::Attr(
            "[data-dnacalc-worker]",
            "data-dnacalc-worker",
            "ready",
        )],
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
            Action::Wait(Check::Value("cell-editor", "5")),
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
            Check::Value("name-box", "D2"),
            Check::Value(BAR, ""),
            Check::Focused("grid"),
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
            Check::Starts("status", "Download started: Book1.xlsx."),
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
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-doc/tests/fixtures/cdoc_m1_2/d1.saved.xlsx",
            ),
            Action::DiscardReplacement,
        ],
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
            Action::Wait(Check::Value("cell-editor", "5")),
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
        checks: &[
            Check::Text(cell!("A1"), "1"),
            Check::Value("name-box", "A2"),
        ],
    },
    Case {
        name: "New makes an empty workbook",
        actions: &[Action::Click("new"), Action::DiscardReplacement],
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
        checks: &[
            Check::Text(cell!("B200"), "9"),
            Check::Value("name-box", "B201"),
        ],
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
            Action::DiscardReplacement,
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
            Action::Keys(
                "grid",
                concat!("\u{E008}", "\u{E015}", "\u{E015}", "\u{E000}"),
            ),
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
        checks: &[
            Check::Text(cell!("D5"), "1"),
            Check::Text(cell!("E5"), "10"),
        ],
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
        checks: &[
            Check::Text(cell!("A1"), "1"),
            Check::Text(cell!("B1"), "10"),
        ],
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
        actions: &[
            Action::Click("undo"),
            Action::Into("name-box", "B1\u{E007}"),
        ],
        checks: &[
            Check::Text(cell!("A1"), "1"),
            Check::Absent(cell!("H1")),
            Check::Value(BAR, "=A1*10"),
        ],
    },
    Case {
        name: "a workbook's number formats show as Excel shows them",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_09v/formats.xlsx",
            ),
            Action::DiscardReplacement,
        ],
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
            Check::Style(cell!("B1"), "left:181px;top:22px;width:180px;height:20px"),
            Check::Style(cell!("D1"), "left:361px;top:22px;width:64px;height:20px"),
            Check::Style(cell!("A3"), "left:64px;top:62px;width:117px;height:40px"),
            Check::Style(cell!("A14"), "left:64px;top:282px;width:117px;height:20px"),
            // A15's negative section is [Color10]: Excel's palette entry 10.
            Check::Style(
                cell!("A15"),
                "left:64px;top:302px;width:117px;height:20px;color:#008000",
            ),
            // Column E is 4 characters (33 px): a formatted number too wide shows ####, as in
            // Excel, and a text overflows over the empty F to I.
            Check::Text(cell!("E1"), "####"),
            Check::Text(cell!("E2"), "12"),
            Check::Style(cell!("E3"), "left:425px;top:62px;width:289px;height:40px"),
            // A4's negative section is [Red].
            Check::Style(
                cell!("A4"),
                "left:64px;top:102px;width:117px;height:20px;color:#ff0000",
            ),
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
            Check::Style(cell!("A4"), "left:64px;top:82px;width:117px;height:40px"),
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
            Check::Style(cell!("C1"), "left:298px;top:22px;width:180px;height:20px"),
            Check::Text(cell!("E1"), "after hidden"),
            Check::Style(cell!("E1"), "left:478px;top:22px;width:64px;height:20px"),
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
        checks: &[Check::Starts("status", "Download started: formats.xlsx.")],
    },
    // dc-gf0: the tree, canvas and ledger skins over calc-host's dc_gf0 workbook (Data, Other and
    // Target Sheet, six defined names, formulas across the sheets).
    Case {
        name: "the tree outlines the workbook: its names, its sheets, their rows and cells",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Click("skin-tree"),
        ],
        checks: &[
            Check::Text(
                "[data-dnacalc=\"tree-row\"][data-key=\"names\"] .dc-count",
                "6",
            ),
            Check::Text(
                "[data-dnacalc=\"tree-row\"][data-key=\"sheet:Data\"] .dc-count",
                "13 cells",
            ),
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
            Check::Attr(
                "[data-dnacalc=\"tree-row\"][data-key=\"sheet:Data\"]",
                "data-folded",
                "true",
            ),
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
            Check::Attr(
                "[data-dnacalc=\"tree-row\"][data-focused=\"true\"]",
                "data-cell",
                "B3",
            ),
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
            Check::Attr(
                "[data-dnacalc=\"tree-row\"][data-focused=\"true\"]",
                "data-cell",
                "C3",
            ),
            Check::Value("name-box", "C3"),
        ],
    },
    Case {
        name: "the canvas centres on the active cell, what it reads to the left, what reads it to the right",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Click("skin-canvas"),
        ],
        checks: &[
            Check::Attr(
                "[data-dnacalc=\"canvas-node\"][data-center=\"true\"]",
                "data-node",
                "B1",
            ),
            Check::Text(
                "[data-dnacalc=\"canvas-node\"][data-node=\"B1\"] [data-dnacalc=\"canvas-value\"]",
                "10",
            ),
            Check::Attr(
                "[data-dnacalc=\"canvas-node\"][data-node=\"A1\"]",
                "data-column",
                "-1",
            ),
            Check::Attr(
                "[data-dnacalc=\"canvas-node\"][data-node=\"C1\"]",
                "data-column",
                "1",
            ),
            Check::Attr(
                "[data-dnacalc=\"canvas-node\"][data-node=\"Other!B3\"]",
                "data-column",
                "1",
            ),
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
            Check::Attr(
                "[data-dnacalc=\"canvas-node\"][data-node=\"B1\"]",
                "data-column",
                "-1",
            ),
            Check::Attr(
                "[data-dnacalc=\"canvas-node\"][data-node=\"A1\"]",
                "data-column",
                "-2",
            ),
        ],
    },
    Case {
        name: "the ledger lists every formula cell of the workbook",
        actions: &[Action::Click("skin-ledger")],
        checks: &[Check::Starts("ledger-count", "22 of 22 formula cells")],
    },
    Case {
        name: "the ledger keeps the cells holding a text",
        actions: &[
            Action::Choose("ledger-filter", "text"),
            Action::Into("ledger-text", "Other!"),
        ],
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
    // dc-6yw: conditional formats as Excel shows them (calc-host's dc_6yw/cf-excel.txt).
    Case {
        name: "conditional formats colour cells as Excel shows them",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_6yw/conditional-formats.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Click("skin-grid"),
        ],
        checks: &[
            Check::Style(cell!("A2"), "background-color:#ffc7ce"),
            Check::Style(cell!("A2"), "color:#9c0006"),
            Check::Style(cell!("B1"), "font-weight:700"),
            Check::Style(cell!("C2"), "background-color:#ffeb9c"),
            Check::Style(cell!("D2"), "font-style:italic"),
            Check::Style(cell!("E2"), "background-color:#86c97e"),
            Check::Style(cell!("J2"), "background-color:#83cceb"),
            Check::Attr(cell!("H2"), "data-icon", "3Arrows:2"),
            Check::Attr(cell!("H3"), "data-icon", "3Arrows:0"),
            Check::Attr(cell!("G6"), "data-bar", "1.0000"),
            Check::Text(cell!("N1"), "400.0%"),
            Check::Text(cell!("N2"), "9"),
        ],
    },
    Case {
        name: "a conditional format follows an edit: A3 set to 8 turns red",
        actions: &[
            Action::Into("name-box", "A3\u{E007}"),
            Action::Keys("grid", "8"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[Check::Style(cell!("A3"), "background-color:#ffc7ce")],
    },
    Case {
        name: "rule review explains the imported priority and active-cell condition",
        actions: &[
            Action::Click("review-tools"),
            Action::Click("[data-dnacalc=rule-row][data-index='0']"),
        ],
        checks: &[
            Check::Count("rule-row", 15),
            Check::TextContains(
                "[data-dnacalc=rule-row][data-index='0']",
                "Applies appearance",
            ),
            Check::Value("rule-applies", "A1:A10"),
            Check::Value("rule-formulas", "5"),
            Check::Reachable("review-dialog"),
        ],
    },
    Case {
        name: "rule conditions edit through the worker and explain a false condition",
        actions: &[
            Action::Into("rule-formulas", "50"),
            Action::Click("rule-apply"),
        ],
        checks: &[
            Check::TextContains(
                "[data-dnacalc=rule-row][data-index='0']",
                "Condition false for this cell",
            ),
            Check::Computed(cell!("A2"), "background-color:transparent"),
        ],
    },
    Case {
        name: "applies-to edits explain an excluded cell and retain its input",
        actions: &[
            Action::Click("[data-dnacalc=rule-row][data-index='0']"),
            Action::Into("rule-applies", "A1:A2"),
            Action::Into("rule-formulas", "5"),
            Action::Click("rule-apply"),
        ],
        checks: &[Check::TextContains(
            "[data-dnacalc=rule-row][data-index='0']",
            "Outside this cell",
        )],
    },
    Case {
        name: "rule edits have ordinary undo and restore imported appearance",
        actions: &[
            Action::Click("review-undo"),
            Action::Wait(Check::TextContains(
                "[data-dnacalc=rule-row][data-index='0']",
                "Condition false",
            )),
            Action::Click("review-undo"),
            Action::Wait(Check::TextContains(
                "[data-dnacalc=rule-row][data-index='0']",
                "Applies appearance",
            )),
            Action::Click("review-close"),
        ],
        checks: &[
            Check::Style(cell!("A2"), "background-color:#ffc7ce"),
            Check::Style(cell!("A3"), "background-color:#ffc7ce"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "users author a visual rule without replacing imported formats or artwork",
        actions: &[
            Action::Into("name-box", "E1:E10\u{E007}"),
            Action::Click("review-tools"),
            Action::Into("rule-applies", "E1:E10"),
            Action::Click("rule-add"),
            Action::Wait(Check::Count("rule-row", 16)),
            Action::Click("review-close"),
        ],
        checks: &[
            Check::Attr(cell!("E6"), "data-bar", "1.0000"),
            Check::Attr(cell!("H2"), "data-icon", "3Arrows:2"),
        ],
    },
    Case {
        name: "rule priority edits and removal are undoable in the same manager",
        actions: &[
            Action::Click("review-tools"),
            Action::Click("[data-dnacalc=rule-row][data-index='15']"),
            Action::Click("rule-up"),
            Action::Wait(Check::TextContains(
                "[data-dnacalc=rule-row][data-index='15']",
                "15 · dataBar",
            )),
            Action::Click("[data-dnacalc=rule-row][data-index='15']"),
            Action::Click("rule-remove"),
            Action::Wait(Check::Count("rule-row", 15)),
            Action::Click("review-undo"),
            Action::Wait(Check::Count("rule-row", 16)),
            Action::Click("review-close"),
        ],
        checks: &[Check::Attr(cell!("E6"), "data-bar", "1.0000")],
    },
    Case {
        name: "rule edits and imported extended bars survive save and reopen",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::Wait(Check::TextContains("status", "Download started:")),
            Action::UploadDownload("conditional-formats.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "conditional-formats.xlsx")),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Click("review-tools"),
        ],
        checks: &[
            Check::Count("rule-row", 16),
            Check::TextContains(
                "[data-dnacalc=rule-row][data-index='0']",
                "Applies appearance",
            ),
        ],
    },
    Case {
        name: "review close and keyboard escape restore the worksheet",
        actions: &[Action::Keys("review-close", ESCAPE)],
        checks: &[
            Check::Focused("grid"),
            Check::Attr(cell!("G6"), "data-bar", "1.0000"),
            Check::Style(cell!("A2"), "background-color:#ffc7ce"),
        ],
    },
    Case {
        name: "formula rules can visibly format a blank cell without inventing a value",
        actions: &[
            Action::Click("review-tools"),
            Action::Click("[data-dnacalc=rule-row][data-index='2']"),
            Action::Into("rule-applies", "P1"),
            Action::Into("rule-formulas", "TRUE"),
            Action::Click("rule-apply"),
            Action::Wait(Check::Absent("rule-formulas")),
            Action::Click("review-close"),
            Action::Into("name-box", "P1\u{E007}"),
        ],
        checks: &[
            Check::Style(cell!("P1"), "background-color:#ffeb9c"),
            Check::Text(cell!("P1"), ""),
            Check::Attr(cell!("P1"), "data-provenance", "empty"),
        ],
    },
    Case {
        name: "undo removes the blank-cell conditional appearance through the subscription",
        actions: &[Action::Click("undo")],
        checks: &[Check::Absent(cell!("P1"))],
    },
    // dc-6yw: data validation (calc-host's dc_6yw/dv-excel.txt).
    Case {
        name: "a list cell shows its drop-down and input message",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_6yw/data-validation.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Click("skin-grid"),
            Action::Into("name-box", "A1\u{E007}"),
        ],
        checks: &[
            Check::Text("validation-prompt", "ColourPick a colour"),
            Check::Attr("dropdown", "title", "Pick from the list"),
        ],
    },
    Case {
        name: "the drop-down lists the rule's items",
        actions: &[Action::Click("dropdown")],
        checks: &[Check::Items("dropdown-list", &["Red", "Green", "Blue"])],
    },
    Case {
        name: "a picked item is the cell's entry",
        actions: &[Action::Click(
            "[data-dnacalc=\"dropdown-list\"] li:nth-child(2)",
        )],
        checks: &[
            Check::Text(cell!("A1"), "Green"),
            Check::Absent("dropdown-list"),
        ],
    },
    Case {
        name: "a list read from cells lists their values",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Click("dropdown"),
        ],
        checks: &[Check::Items("dropdown-list", &["North", "South", "East"])],
    },
    Case {
        name: "validation list uses semantic keyboard options and Enter selects a value",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", "\u{E00A}\u{E015}\u{E000}"),
            Action::Wait(Check::Focused("dropdown-list")),
            Action::Keys("dropdown-list", "\u{E010}\u{E007}"),
        ],
        checks: &[
            Check::Text(cell!("A1"), "Blue"),
            Check::Absent("dropdown-list"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "validation list arrows change the semantic option and Escape keeps the cell",
        actions: &[
            Action::Keys("grid", "\u{E00A}\u{E015}\u{E000}"),
            Action::Wait(Check::Focused("dropdown-list")),
            Action::Keys("dropdown-list", "\u{E015}"),
        ],
        checks: &[
            Check::Role("dropdown-list", "listbox"),
            Check::Attr("#dc-validation-option-1", "aria-selected", "true"),
            Check::ActiveDescendant("dropdown-list"),
        ],
    },
    Case {
        name: "validation list Escape cancels without entering its option",
        actions: &[Action::Keys("dropdown-list", ESCAPE)],
        checks: &[
            Check::Text(cell!("A1"), "Blue"),
            Check::Absent("dropdown-list"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "a stop rule refuses an entry it does not allow, with its message",
        actions: &[
            Action::Into("name-box", "A2\u{E007}"),
            Action::Keys("grid", "P"),
            Action::Keys("cell-editor", "urple\u{E007}"),
        ],
        checks: &[
            Check::Text("status", "Not a colour: Choose Red, Green or Blue"),
            Check::Attr("status", "data-refused", "true"),
            Check::Absent(cell!("A2")),
        ],
    },
    Case {
        name: "Stop Retry retains and focuses the rejected draft",
        actions: &[Action::Click("validation-retry")],
        checks: &[
            Check::Value("cell-editor", "Purple"),
            Check::Focused("cell-editor"),
        ],
    },
    Case {
        name: "a retried validation draft can be corrected and committed",
        actions: &[Action::Into("cell-editor", "Red\u{E007}")],
        checks: &[
            Check::Text(cell!("A2"), "Red"),
            Check::Absent("cell-editor"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "Warning asks before changing the cell",
        actions: &[
            Action::Into("name-box", "D1\u{E007}"),
            Action::Keys("grid", "-"),
            Action::Keys("cell-editor", "1\u{E007}"),
        ],
        checks: &[
            Check::TextContains("validation-dialog", "Warning"),
            Check::Text("validation-message", "A positive number is expected"),
            Check::Absent(cell!("D1")),
        ],
    },
    Case {
        name: "Warning Edit retains the draft and Cancel discards it",
        actions: &[
            Action::Click("validation-retry"),
            Action::Wait(Check::Focused("cell-editor")),
            Action::Keys("cell-editor", ENTER),
            Action::Click("validation-cancel"),
        ],
        checks: &[
            Check::Absent(cell!("D1")),
            Check::Absent("cell-editor"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "Warning Keep is an explicit undoable entry",
        actions: &[
            Action::Keys("grid", "-"),
            Action::Keys("cell-editor", "1\u{E007}"),
            Action::Click("validation-keep"),
        ],
        checks: &[
            Check::Text(cell!("D1"), "-1"),
            Check::Text(
                "status",
                "Warning, entry kept: Microsoft Excel: A positive number is expected",
            ),
        ],
    },
    Case {
        name: "Warning acceptance has ordinary Undo and Redo",
        actions: &[
            Action::Click("undo"),
            Action::Wait(Check::Absent(cell!("D1"))),
            Action::Click("redo"),
        ],
        checks: &[Check::Text(cell!("D1"), "-1")],
    },
    Case {
        name: "Information waits for OK or Cancel and Escape cancels",
        actions: &[
            Action::Into("name-box", "E1\u{E007}"),
            Action::Keys("grid", "a"),
            Action::Keys("cell-editor", "bcdef\u{E007}"),
            Action::Wait(Check::TextContains("validation-dialog", "Information")),
            Action::Keys("validation-cancel", ESCAPE),
        ],
        checks: &[
            Check::Absent(cell!("E1")),
            Check::Absent("cell-editor"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "Information Keep stores the chosen entry",
        actions: &[
            Action::Keys("grid", "a"),
            Action::Keys("cell-editor", "bcdef\u{E007}"),
            Action::Click("validation-keep"),
        ],
        checks: &[
            Check::Text(cell!("E1"), "abcdef"),
            Check::Text(
                "status",
                "Information, entry kept: Microsoft Excel: At most five characters",
            ),
        ],
    },
    Case {
        name: "range entry also asks before accepting warning cells",
        actions: &[
            Action::Into("name-box", "D2:D3\u{E007}"),
            Action::Keys("grid", "-"),
            Action::Keys("cell-editor", "2\u{E009}\u{E007}\u{E000}"),
            Action::Wait(Check::TextContains(
                "validation-message",
                "2 selected cells",
            )),
        ],
        checks: &[Check::Absent(cell!("D2")), Check::Absent(cell!("D3"))],
    },
    Case {
        name: "range warning acceptance remains one Undo step",
        actions: &[
            Action::Click("validation-keep"),
            Action::Wait(Check::Text(cell!("D2"), "-2")),
            Action::Click("undo"),
        ],
        checks: &[Check::Absent(cell!("D2")), Check::Absent(cell!("D3"))],
    },
    Case {
        name: "validation decisions preserve rules through save and reopen",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::Wait(Check::TextContains("status", "Download started:")),
            Action::UploadDownload("data-validation.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "data-validation.xlsx")),
            Action::Into("name-box", "D2\u{E007}"),
            Action::Keys("grid", "-"),
            Action::Keys("cell-editor", "3\u{E007}"),
            Action::Wait(Check::TextContains("validation-dialog", "Warning")),
            Action::Click("validation-cancel"),
        ],
        checks: &[
            Check::Text(cell!("D1"), "-1"),
            Check::Text(cell!("E1"), "abcdef"),
            Check::Absent(cell!("D2")),
            Check::Focused("grid"),
        ],
    },
    // dc-6yw: charts drawn read-only (calc-host's dc_6yw/charts-excel.txt).
    Case {
        name: "charts are drawn where Excel puts them, with their series' values",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_6yw/charts.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Click("skin-grid"),
        ],
        checks: &[
            Check::Attr(
                "[data-dnacalc=\"chart\"][data-chart=\"Chart 1\"]",
                "data-kind",
                "column",
            ),
            Check::Text(
                "[data-dnacalc=\"chart\"][data-chart=\"Chart 1\"] [data-dnacalc=\"chart-title\"]",
                "Column",
            ),
            Check::Attr(
                "[data-dnacalc=\"chart\"][data-chart=\"Chart 1\"] [data-series=\"0\"][data-point=\"0\"]",
                "data-value",
                "12",
            ),
            Check::Attr(
                "[data-dnacalc=\"chart\"][data-chart=\"Chart 1\"] [data-series=\"1\"][data-point=\"5\"]",
                "data-value",
                "13",
            ),
            Check::Attr(
                "[data-dnacalc=\"chart\"][data-chart=\"Chart 3\"]",
                "data-kind",
                "line",
            ),
        ],
    },
    Case {
        name: "a chart follows an edit: B2 set to 40 moves its column and the total's line",
        actions: &[
            Action::Into("name-box", "B2\u{E007}"),
            Action::Keys("grid", "4"),
            Action::Wait(Check::Value("cell-editor", "4")),
            Action::Keys("cell-editor", "0\u{E007}"),
        ],
        checks: &[
            Check::Attr(
                "[data-dnacalc=\"chart\"][data-chart=\"Chart 1\"] [data-series=\"0\"][data-point=\"0\"]",
                "data-value",
                "40",
            ),
            Check::Attr(
                "[data-dnacalc=\"chart\"][data-chart=\"Chart 3\"] [data-series=\"0\"][data-point=\"0\"]",
                "data-value",
                "48",
            ),
        ],
    },
    // dc-xib: Strand's themes and a narrow screen.
    Case {
        name: "the theme switch shows the dark theme: petrol chrome, charcoal paper",
        actions: &[
            Action::Click("settings-toggle"),
            Action::Choose("theme", "cockpit-dark"),
            Action::Click("settings-toggle"),
        ],
        checks: &[
            Check::Computed("app", "background-color:#1A262B;color:#E2ECEF"),
            Check::Computed(
                "[data-dnacalc=\"app\"] .dc-mast",
                "background-color:#0F2229",
            ),
        ],
    },
    Case {
        name: "the system's theme again: light paper",
        actions: &[
            Action::Click("settings-toggle"),
            Action::Choose("theme", ""),
            Action::Click("settings-toggle"),
        ],
        checks: &[Check::Computed(
            "app",
            "background-color:#FFFFFF;color:#17282E",
        )],
    },
    Case {
        name: "on a 500-pixel screen (the narrowest a desktop Firefox window goes) the page fits its width and the grid still shows",
        actions: &[Action::Window(500, 844), Action::Click("skin-grid")],
        checks: &[
            Check::Narrow(500),
            Check::Fits,
            Check::Attr("grid", "tabindex", "0"),
        ],
    },
    Case {
        name: "back on a wide screen",
        actions: &[Action::Window(1280, 1000)],
        checks: &[Check::Fits],
    },
    // dc-971p.2: the cell and bar share the same draft, assistance and commit contract.
    Case {
        name: "edit session: cell completion opens on typed replacement",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("status", "New workbook")),
            Action::Keys("grid", "1"),
            Action::Keys("cell-editor", ENTER),
            Action::Keys("grid", "2"),
            Action::Keys("cell-editor", ENTER),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Keys("grid", "="),
            Action::Keys("cell-editor", "SUM"),
        ],
        checks: &[
            Check::Value("cell-editor", "=SUM"),
            Check::Text("[data-onecalc=\"completion-label\"]", "SUM"),
        ],
    },
    Case {
        name: "edit session: completion Escape dismisses only the popup",
        actions: &[Action::Keys("cell-editor", ESCAPE)],
        checks: &[
            Check::Value("cell-editor", "=SUM"),
            Check::Absent("[data-onecalc=\"completions\"]"),
            Check::Focused("cell-editor"),
        ],
    },
    Case {
        name: "edit session: completion Tab accepts before worksheet Tab can commit",
        actions: &[
            Action::Keys("cell-editor", "\u{E003}M"),
            Action::Keys("cell-editor", TAB),
        ],
        checks: &[
            Check::Value("cell-editor", "=SUM("),
            Check::Value("name-box", "B1"),
            Check::Text("[data-onecalc=\"active-parameter\"]", "number1"),
        ],
    },
    Case {
        name: "edit session: cell Enter commits once and returns focus below",
        actions: &[
            Action::Keys("cell-editor", "A1:A2)"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[
            Check::Text(cell!("B1"), "3"),
            Check::Value("name-box", "B2"),
            Check::Focused("grid"),
            Check::Absent("cell-editor"),
        ],
    },
    Case {
        name: "edit session: F2 seeds authored formula and Escape keeps exact source",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Keys("grid", F2),
            Action::Keys("cell-editor", "+100"),
            Action::Keys("cell-editor", ESCAPE),
        ],
        checks: &[
            Check::Value(BAR, "=SUM(A1:A2)"),
            Check::Text(cell!("B1"), "3"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "edit session: double click opens the authored source",
        actions: &[Action::DoubleClick(cell!("B1"))],
        checks: &[
            Check::Value("cell-editor", "=SUM(A1:A2)"),
            Check::Focused("cell-editor"),
        ],
    },
    Case {
        name: "edit session: local Ctrl+Z and Redo never enter the workbook journal",
        actions: &[
            Action::Keys("cell-editor", "+9"),
            Action::Keys("cell-editor", "\u{E009}z\u{E000}"),
        ],
        checks: &[
            Check::Value("cell-editor", "=SUM(A1:A2)+"),
            Check::Text(cell!("B1"), "3"),
            Check::Text(cell!("A2"), "2"),
        ],
    },
    Case {
        name: "edit session: toolbar Redo belongs to the current text edit",
        actions: &[Action::Click("redo")],
        checks: &[
            Check::Value("cell-editor", "=SUM(A1:A2)+9"),
            Check::Focused("cell-editor"),
            Check::Text(cell!("B1"), "3"),
        ],
    },
    Case {
        name: "edit session: bar point click retains the target and inserts a reference",
        actions: &[
            Action::Keys("cell-editor", ESCAPE),
            Action::Into("name-box", "B3\u{E007}"),
            Action::Into(BAR, "=A1+"),
            Action::Click(cell!("A2")),
        ],
        checks: &[
            Check::Value(BAR, "=A1+A2"),
            Check::Value("name-box", "B3"),
            Check::Focused(BAR),
            Check::Attr("point-reference", "class", "dc-reference"),
            Check::Text("edit-mode", "Point · F2 to edit text"),
        ],
    },
    Case {
        name: "edit session: F4 cycles a picked reference through four anchor forms",
        actions: &[Action::Keys(BAR, "\u{E034}")],
        checks: &[
            Check::Value(BAR, "=A1+$A$2"),
            Check::Value("name-box", "B3"),
        ],
    },
    Case {
        name: "edit session: F4 row absolute",
        actions: &[Action::Keys(BAR, "\u{E034}")],
        checks: &[Check::Value(BAR, "=A1+A$2")],
    },
    Case {
        name: "edit session: F4 column absolute",
        actions: &[Action::Keys(BAR, "\u{E034}")],
        checks: &[Check::Value(BAR, "=A1+$A2")],
    },
    Case {
        name: "edit session: F4 relative again and Ctrl+F2 transfers the draft",
        actions: &[
            Action::Keys(BAR, "\u{E034}"),
            Action::Keys(BAR, "\u{E009}\u{E032}\u{E000}"),
        ],
        checks: &[
            Check::Value("cell-editor", "=A1+A2"),
            Check::Focused("cell-editor"),
        ],
    },
    Case {
        name: "edit session: transferred draft commits to its original address with one undo step",
        actions: &[Action::Keys("cell-editor", ENTER)],
        checks: &[
            Check::Text(cell!("B3"), "3"),
            Check::Value("name-box", "B4"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "edit session: workbook Undo removes only that completed entry",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Absent(cell!("B3")),
            Check::Text(cell!("B1"), "3"),
            Check::Text(cell!("A2"), "2"),
            Check::Value(BAR, ""),
        ],
    },
    Case {
        name: "edit session: workbook Redo restores the exact pointed formula",
        actions: &[Action::Click("redo")],
        checks: &[Check::Text(cell!("B3"), "3"), Check::Value(BAR, "=A1+A2")],
    },
    Case {
        name: "edit session: bar Tab commits and moves right",
        actions: &[
            Action::Into("name-box", "B4\u{E007}"),
            Action::Into(BAR, "25"),
            Action::Keys(BAR, TAB),
        ],
        checks: &[
            Check::Text(cell!("B4"), "25"),
            Check::Value("name-box", "C4"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "edit session: bar Shift+Tab commits and moves left",
        actions: &[
            Action::Into(BAR, "27"),
            Action::Keys(BAR, "\u{E008}\u{E004}\u{E000}"),
        ],
        checks: &[
            Check::Text(cell!("C4"), "27"),
            Check::Value("name-box", "B4"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "edit session: bar Shift+Enter commits and moves up",
        actions: &[
            Action::Into(BAR, "28"),
            Action::Keys(BAR, "\u{E008}\u{E007}\u{E000}"),
        ],
        checks: &[
            Check::Text(cell!("B4"), "28"),
            Check::Value("name-box", "B3"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "edit session: cell Shift+Enter commits and moves up",
        actions: &[
            Action::Keys("grid", "4"),
            Action::Wait(Check::Value("cell-editor", "4")),
            Action::Keys("cell-editor", "\u{E008}\u{E007}\u{E000}"),
        ],
        checks: &[
            Check::Text(cell!("B3"), "4"),
            Check::Value("name-box", "B2"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "edit session: cell Shift+Tab commits and moves left",
        actions: &[
            Action::Keys("grid", "5"),
            Action::Wait(Check::Value("cell-editor", "5")),
            Action::Keys("cell-editor", "\u{E008}\u{E004}\u{E000}"),
        ],
        checks: &[
            Check::Text(cell!("B2"), "5"),
            Check::Value("name-box", "A2"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "edit session: rejected bar entry keeps the draft target source and focus",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into(BAR, "=A1+"),
            Action::Keys(BAR, ENTER),
        ],
        checks: &[
            Check::Value(BAR, "=A1+"),
            Check::Value("name-box", "B1"),
            Check::Text(cell!("B1"), "3"),
            Check::Focused(BAR),
            Check::Attr("status", "data-refused", "true"),
        ],
    },
    Case {
        name: "edit session: cancelling a rejected entry restores authored source",
        actions: &[Action::Keys(BAR, ESCAPE)],
        checks: &[Check::Value(BAR, "=SUM(A1:A2)"), Check::Focused("grid")],
    },
    Case {
        name: "edit session: formula pointing can drag a range without changing active cell",
        actions: &[
            Action::Into("name-box", "D1\u{E007}"),
            Action::Into(BAR, "=SUM("),
            Action::Drag(cell!("A1"), cell!("A2")),
        ],
        checks: &[
            Check::Value(BAR, "=SUM(A1:A2"),
            Check::Value("name-box", "D1"),
            Check::Focused(BAR),
        ],
    },
    Case {
        name: "edit session: completion leaves Enter to worksheet entry",
        actions: &[
            Action::Keys(BAR, ESCAPE),
            Action::Into(BAR, "=TRUE"),
            Action::Keys(BAR, ENTER),
        ],
        checks: &[
            Check::Text(cell!("D1"), "TRUE"),
            Check::Value("name-box", "D2"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "edit session: point mode arrows create references and F2 returns to caret movement",
        actions: &[
            Action::Into("name-box", "D1\u{E007}"),
            Action::Keys("grid", "="),
            Action::Keys("cell-editor", "\u{E012}"),
            Action::Keys("cell-editor", F2),
        ],
        checks: &[
            Check::Value("cell-editor", "=C1"),
            Check::Text("edit-mode", "Edit · F2 to point to references"),
        ],
    },
    Case {
        name: "edit session: switching skins keeps the draft and returns it to the bar",
        actions: &[
            Action::Keys("cell-editor", ESCAPE),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Keys("grid", F2),
            Action::Keys("cell-editor", "+4"),
            Action::Click("skin-canvas"),
        ],
        checks: &[
            Check::Value(BAR, "=SUM(A1:A2)+4"),
            Check::Value("name-box", "B1"),
            Check::Focused(BAR),
            Check::Absent("cell-editor"),
        ],
    },
    Case {
        name: "edit session: another skin uses the same commit and undo route",
        actions: &[
            Action::Keys(BAR, ENTER),
            Action::Click("skin-grid"),
            Action::Into("name-box", "B1\u{E007}"),
        ],
        checks: &[
            Check::Value(BAR, "=SUM(A1:A2)+4"),
            Check::Text(cell!("B1"), "7"),
        ],
    },
    Case {
        name: "edit session: synthetic in-cell composition Enter cannot commit",
        actions: &[
            Action::Into("name-box", "B7\u{E007}"),
            Action::Keys("grid", "x"),
            Action::Composing("cell-editor", "漢"),
        ],
        checks: &[
            Check::Value("cell-editor", "x漢"),
            Check::Value("name-box", "B7"),
            Check::Absent(cell!("B7")),
            Check::Focused("cell-editor"),
        ],
    },
    Case {
        name: "edit session: composition ends then Escape cancels without a workbook edit",
        actions: &[
            Action::CompositionEnd("cell-editor"),
            Action::Keys("cell-editor", ESCAPE),
        ],
        checks: &[
            Check::Value(BAR, ""),
            Check::Absent(cell!("B7")),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "edit session: synthetic bar composition Enter cannot commit",
        actions: &[Action::Into(BAR, "abc"), Action::Composing(BAR, "字")],
        checks: &[
            Check::Value(BAR, "abc字"),
            Check::Value("name-box", "B7"),
            Check::Absent(cell!("B7")),
            Check::Focused(BAR),
        ],
    },
    Case {
        name: "edit session: after composition a normal Enter commits the complete text",
        actions: &[
            Action::CompositionEnd(BAR),
            Action::Keys(BAR, ENTER),
            Action::Into("name-box", "B7\u{E007}"),
        ],
        checks: &[
            Check::Value(BAR, "abc字"),
            Check::Text(cell!("B7"), "abc字"),
            Check::Absent("cell-editor"),
        ],
    },
    Case {
        name: "edit session: a reference picked on another sheet retains the original edit target",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Into("name-box", "F5\u{E007}"),
            Action::Into(BAR, "="),
            Action::Click("[data-dnacalc=\"sheet-tab\"][data-sheet=\"Other\"]"),
            Action::Wait(Check::Text(cell!("A1"), "100")),
            Action::Click(cell!("A1")),
        ],
        checks: &[
            Check::Value(BAR, "=Other!A1"),
            Check::Starts("edit-session", "Editing Data!F5"),
            Check::Focused(BAR),
        ],
    },
    Case {
        name: "edit session: a cross-sheet point commits back on the edited sheet",
        actions: &[Action::Keys(BAR, ENTER)],
        checks: &[
            Check::Text(cell!("F5"), "100"),
            Check::Value("name-box", "F6"),
            Check::Focused("grid"),
            Check::Attr(
                "[data-dnacalc=\"sheet-tab\"][data-sheet=\"Data\"]",
                "aria-selected",
                "true",
            ),
        ],
    },
    Case {
        name: "edit session: cross-sheet entry undo restores the source selection and removes only that formula",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Value("name-box", "F5"),
            Check::Value(BAR, ""),
            Check::Absent(cell!("F5")),
            Check::Text(cell!("A1"), "1"),
        ],
    },
    Case {
        name: "edit session: entering unchanged source adds no workbook undo step",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Keys("grid", "1"),
            Action::Keys("cell-editor", ENTER),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", F2),
            Action::Keys("cell-editor", ENTER),
            Action::Click("undo"),
        ],
        checks: &[
            Check::Absent(cell!("A1")),
            Check::Value("name-box", "A1"),
            Check::Value(BAR, ""),
        ],
    },
    Case {
        name: "edit session: cell Shift+Left keeps the whole text selection for replacement",
        actions: &[
            Action::Keys("grid", "a"),
            Action::Keys("cell-editor", "bcdef"),
            Action::Keys("cell-editor", "\u{E008}\u{E012}\u{E012}\u{E012}\u{E000}"),
            Action::Keys("cell-editor", "Z"),
        ],
        checks: &[
            Check::Value("cell-editor", "abcZ"),
            Check::Absent(cell!("A1")),
            Check::Focused("cell-editor"),
        ],
    },
    Case {
        name: "edit session: bar Shift+Left keeps the whole text selection for replacement",
        actions: &[
            Action::Keys("cell-editor", ESCAPE),
            Action::Into(BAR, "abcdef"),
            Action::Keys(BAR, "\u{E008}\u{E012}\u{E012}\u{E012}\u{E000}"),
            Action::Keys(BAR, "Z"),
        ],
        checks: &[
            Check::Value(BAR, "abcZ"),
            Check::Absent(cell!("A1")),
            Check::Focused(BAR),
        ],
    },
    Case {
        name: "edit session: name-box navigation commits an accepted draft before moving",
        actions: &[Action::Into("name-box", "C3\u{E007}")],
        checks: &[
            Check::Text(cell!("A1"), "abcZ"),
            Check::Value("name-box", "C3"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "edit session: name-box navigation preserves rejected draft and focus",
        actions: &[
            Action::Into(BAR, "=1+"),
            Action::Into("name-box", "D4\u{E007}"),
        ],
        checks: &[
            Check::Value(BAR, "=1+"),
            Check::Focused(BAR),
            Check::Value("name-box", "C3"),
            Check::Starts("edit-session", "Editing Sheet1!C3"),
            Check::Absent(cell!("C3")),
        ],
    },
    Case {
        name: "shared states: Save preserves a rejected draft and returns keyboard focus",
        actions: &[Action::Click("save")],
        checks: &[
            Check::Value(BAR, "=1+"),
            Check::Focused(BAR),
            Check::Attr("status", "data-refused", "true"),
        ],
    },
    Case {
        name: "shared states: New protects a rejected draft and restores focus on Keep working",
        actions: &[Action::Click("new")],
        checks: &[
            Check::Role("replace-dialog", "dialog"),
            Check::Name("replace-dialog", "Keep your current work?"),
            Check::Focused("keep-working"),
            Check::Reachable("keep-working"),
        ],
    },
    Case {
        name: "shared states: keeping the workbook preserves source, draft and its editor",
        actions: &[Action::Click("keep-working")],
        checks: &[
            Check::Value(BAR, "=1+"),
            Check::Focused(BAR),
            Check::Absent("[data-dnacalc=replace-dialog][open]"),
        ],
    },
    Case {
        name: "shared states: discard explicitly replaces the workbook and clears history",
        actions: &[
            Action::Keys(BAR, ESCAPE),
            Action::Click("new"),
            Action::DiscardReplacement,
        ],
        checks: &[
            Check::Value("name-box", "A1"),
            Check::Value(BAR, ""),
            Check::Attr("undo", "disabled", ""),
            Check::Attr("redo", "disabled", ""),
            Check::Text("result-state", "Empty"),
        ],
    },
    Case {
        name: "shared states: entered literals are distinct from calculated values",
        actions: &[
            Action::Keys("grid", "8"),
            Action::Keys("cell-editor", ENTER),
            Action::Into("name-box", "A1\u{E007}"),
        ],
        checks: &[
            Check::Text("result-state", "Literal"),
            Check::Text("selected-value", "8"),
            Check::Role("grid", "grid"),
            Check::Name("grid", "Sheet1 worksheet"),
            Check::Role("#dc-grid-cell-1-1", "gridcell"),
            Check::Name("#dc-grid-cell-1-1", "A1, 8, literal"),
            Check::Bounded("[role=gridcell]", 5000),
            Check::ActiveDescendant("grid"),
        ],
    },
    Case {
        name: "shared states: Save enters an accepted draft before preparing its package",
        actions: &[
            Action::Into("name-box", "E1\u{E007}"),
            Action::Into(BAR, "=1+2"),
            Action::Click("save"),
        ],
        checks: &[
            Check::Starts("status", "Download started: Book1.xlsx."),
            Check::Text(cell!("E1"), "3"),
            Check::Absent("edit-session"),
        ],
    },
    Case {
        name: "shared states: a worksheet error has meaning and calculated provenance",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into(BAR, "=1/0\u{E007}"),
            Action::Into("name-box", "B1\u{E007}"),
        ],
        checks: &[
            Check::Text("result-state", "Worksheet error"),
            Check::Text("error-meaning", "division by zero"),
            Check::Starts(
                "cell-provenance",
                "Sheet1!B1: calculated by DNA Calc's engine",
            ),
            Check::Reachable("recover-edit"),
            Check::Attr("result-announcement", "aria-live", "polite"),
        ],
    },
    Case {
        name: "shared states: a refusal opens the exact reason without inventing an owner",
        actions: &[
            Action::Into("name-box", "C1\u{E007}"),
            Action::Into(BAR, "=WEBSERVICE(\"https://example.com\")\u{E007}"),
            Action::Into("name-box", "C1\u{E007}"),
        ],
        checks: &[
            Check::Text("result-state", "Refused"),
            Check::Starts("cell-provenance", "Sheet1!C1: refused: "),
            Check::Absent("refusal-owner"),
            Check::Reachable("recover-edit"),
        ],
    },
    Case {
        name: "shared states: backend-supplied ownership is visible with the refusal reason",
        actions: &[
            Action::Into("name-box", "F1\u{E007}"),
            Action::Into(BAR, "='[Missing.xlsx]Data'!A1\u{E007}"),
            Action::Into("name-box", "F1\u{E007}"),
        ],
        checks: &[
            Check::Text("result-state", "Refused"),
            Check::Text("refusal-owner", "Work package: dc-ylr"),
            Check::Starts("cell-provenance", "Sheet1!F1: refused: "),
        ],
    },
    Case {
        name: "shared refusal in Canvas",
        actions: &[
            Action::Into("name-box", "C1\u{E007}"),
            Action::Click("skin-canvas"),
        ],
        checks: &[
            Check::Text("result-state", "Refused"),
            Check::Reachable("recover-edit"),
        ],
    },
    Case {
        name: "shared refusal in Atlas",
        actions: &[Action::Click("skin-atlas")],
        checks: &[
            Check::Text("result-state", "Refused"),
            Check::Reachable("recover-edit"),
            Check::Absent("[data-dnacalc=atlas-diagnostics][open]"),
        ],
    },
    Case {
        name: "shared refusal in Ledger",
        actions: &[Action::Click("skin-ledger")],
        checks: &[
            Check::Text("result-state", "Refused"),
            Check::Name("ledger-filter", "Formula filter"),
            Check::Reachable("recover-edit"),
        ],
    },
    Case {
        name: "shared refusal in Tree",
        actions: &[Action::Click("skin-tree")],
        checks: &[
            Check::Text("result-state", "Refused"),
            Check::Role("tree", "tree"),
            Check::Name("tree", "Workbook outline"),
            Check::Bounded("[role=treeitem]", 3000),
            Check::ActiveDescendant("tree"),
        ],
    },
    Case {
        name: "shared refusal in Notebook",
        actions: &[Action::Click("skin-notebook")],
        checks: &[
            Check::Text("result-state", "Refused"),
            Check::Name("[data-dnacalc=block][data-cell=A1] input", "Input for A1"),
        ],
    },
    Case {
        name: "shared states: recovery opens the selected source without replacing it",
        actions: &[Action::Click("recover-edit")],
        checks: &[
            Check::Value(BAR, "=WEBSERVICE(\"https://example.com\")"),
            Check::Focused(BAR),
        ],
    },
    Case {
        name: "shared states: manual edits show Stale with an explicit Calculate recovery",
        actions: &[
            Action::Keys(BAR, ESCAPE),
            Action::Click("skin-grid"),
            Action::Into("name-box", "D1\u{E007}"),
            Action::Into(BAR, "=A1*2\u{E007}"),
            Action::Choose("calc-mode", "manual"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into(BAR, "9\u{E007}"),
            Action::Into("name-box", "D1\u{E007}"),
            Action::Click("result-toggle"),
        ],
        checks: &[
            Check::Text("result-state", "Stale"),
            Check::Text("selected-value", "16"),
            Check::Reachable("recover-calculate"),
        ],
    },
    Case {
        name: "shared states: recovery updates the selected result through the worker",
        actions: &[Action::Click("recover-calculate")],
        checks: &[
            Check::Text("result-state", "Calculated"),
            Check::Text("selected-value", "18"),
            Check::Starts(
                "state-explanation",
                "Calculated by DNA Calc. Agreement with Excel has not been checked here.",
            ),
        ],
    },
    Case {
        name: "shared states: Open protects edits; cancelling keeps the current workbook and history",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx",
            ),
            Action::Wait(Check::Focused("keep-working")),
            Action::Keys("keep-working", ESCAPE),
        ],
        checks: &[
            Check::Starts("document", "Book1.xlsx"),
            Check::Value("name-box", "D1"),
            Check::Value(BAR, "=A1*2"),
            Check::Text("selected-value", "18"),
        ],
    },
    Case {
        name: "shared states: a downloaded package is not confirmation of durable storage",
        actions: &[
            Action::Click("save"),
            Action::Wait(Check::Starts("status", "Download started: Book1.xlsx.")),
            Action::Click("new"),
        ],
        checks: &[
            Check::Reachable("keep-working"),
            Check::Text(
                "download-warning",
                "Download unconfirmed · check your files",
            ),
        ],
    },
    Case {
        name: "shared states: F6 exits the worksheet and returns without losing selection",
        actions: &[Action::Click("keep-working"), Action::Keys("grid", F6)],
        checks: &[
            Check::Focused("[data-dnacalc=sheet-tab][aria-selected=true]"),
            Check::Value("name-box", "D1"),
        ],
    },
    Case {
        name: "shared states: Shift+F6 returns to the workspace",
        actions: &[Action::Keys(
            "[data-dnacalc=sheet-tab][aria-selected=true]",
            "\u{E008}\u{E036}\u{E000}",
        )],
        checks: &[Check::Focused("grid"), Check::Value("name-box", "D1")],
    },
    Case {
        name: "shared shell: commands, worksheet tabs, status and editor fit at 390 pixels",
        actions: &[Action::Window(390, 844)],
        checks: &[
            Check::Narrow(390),
            Check::Fits,
            Check::Reachable("new"),
            Check::Reachable("open"),
            Check::Reachable("save"),
            Check::Reachable(BAR),
            Check::Reachable("sheet-tabs"),
            Check::Reachable("worker-status"),
            Check::Contrast("result-state"),
        ],
    },
    Case {
        name: "shared shell: commands, tabs and status fit at 1024 pixels",
        actions: &[Action::Window(1024, 768)],
        checks: &[
            Check::Fits,
            Check::Reachable("sheet-tabs"),
            Check::Reachable("worker-status"),
            Check::Reachable(BAR),
        ],
    },
    Case {
        name: "shared shell: dark theme state text has measured contrast",
        actions: &[
            Action::Window(1440, 1000),
            Action::Click("settings-toggle"),
            Action::Choose("theme", "cockpit-dark"),
            Action::Click("settings-toggle"),
        ],
        checks: &[
            Check::Fits,
            Check::Contrast("result-state"),
            Check::Reachable("sheet-tabs"),
            Check::Reachable("worker-status"),
        ],
    },
    Case {
        name: "shared shell: high-contrast theme state text has measured contrast",
        actions: &[
            Action::Click("settings-toggle"),
            Action::Choose("theme", "high-contrast"),
            Action::Click("settings-toggle"),
        ],
        checks: &[
            Check::Contrast("result-state"),
            Check::Reachable("sheet-tabs"),
            Check::Reachable("worker-status"),
        ],
    },
    Case {
        name: "shared states: a downloaded package explicitly declaring Manual opens with file-cache provenance",
        actions: &[
            Action::Click("settings-toggle"),
            Action::Choose("theme", ""),
            Action::Click("settings-toggle"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::Wait(Check::Starts("status", "Download started: Book1.xlsx.")),
            Action::ManualDownloadFixture,
            Action::UploadDownload("manual-cache.xlsx"),
            Action::DiscardReplacement,
            Action::Into("name-box", "D1\u{E007}"),
        ],
        checks: &[
            Check::Text("result-state", "File cache"),
            Check::Text("selected-value", "18"),
            Check::Starts("cell-provenance", "Sheet1!D1: the value the file stored"),
        ],
    },
    Case {
        name: "shared states: an uncommitted Notebook draft is protected by New",
        actions: &[
            Action::Click("skin-notebook"),
            Action::Wait(Check::Value(
                "[data-dnacalc=block][data-cell=A1] input",
                "9",
            )),
            Action::Into("[data-dnacalc=block][data-cell=A1] input", "123"),
            Action::Click("new"),
        ],
        checks: &[
            Check::Focused("keep-working"),
            Check::Reachable("keep-working"),
        ],
    },
    Case {
        name: "shared states: a Notebook draft survives Keep working and a skin round trip",
        actions: &[
            Action::Click("keep-working"),
            Action::Click("skin-grid"),
            Action::Click("skin-notebook"),
        ],
        checks: &[Check::Value(
            "[data-dnacalc=block][data-cell=A1] input",
            "123",
        )],
    },
    Case {
        name: "shared states: Save cannot silently omit a Notebook draft",
        actions: &[Action::Click("save")],
        checks: &[
            Check::Starts("status", "Finish the Notebook draft with Enter"),
            Check::Focused("[data-dnacalc=block][data-cell=A1] input"),
            Check::Value("[data-dnacalc=block][data-cell=A1] input", "123"),
        ],
    },
    Case {
        name: "shared states: cancelling a Notebook draft restores authored input",
        actions: &[Action::Keys(
            "[data-dnacalc=block][data-cell=A1] input",
            ESCAPE,
        )],
        checks: &[
            Check::Value("[data-dnacalc=block][data-cell=A1] input", "9"),
            Check::Absent("notebook-drafts"),
        ],
    },
    Case {
        name: "shared sheet tabs: arrows activate and focus other sheets",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Keys("[data-dnacalc=sheet-tab][aria-selected=true]", "\u{E014}"),
        ],
        checks: &[
            Check::Attr(
                "[data-dnacalc=sheet-tab][data-sheet=Other]",
                "aria-selected",
                "true",
            ),
            Check::Focused("[data-dnacalc=sheet-tab][data-sheet=Other]"),
        ],
    },
    Case {
        name: "shared sheet tabs: Home returns keyboard focus to the first sheet",
        actions: &[Action::Keys(
            "[data-dnacalc=sheet-tab][aria-selected=true]",
            "\u{E011}",
        )],
        checks: &[
            Check::Attr(
                "[data-dnacalc=sheet-tab][data-sheet=Data]",
                "aria-selected",
                "true",
            ),
            Check::Focused("[data-dnacalc=sheet-tab][data-sheet=Data]"),
        ],
    },
    Case {
        name: "shared shell: F6 leaves settings and closes the overlay",
        actions: &[
            Action::Click("skin-grid"),
            Action::Click("settings-toggle"),
            Action::Keys("settings-toggle", F6),
        ],
        checks: &[
            Check::Focused(BAR),
            Check::Absent("[data-dnacalc=settings][open]"),
        ],
    },
    Case {
        name: "shared shell: workspace resizing keeps the active cell editor visible",
        actions: &[
            Action::Keys(BAR, ESCAPE),
            Action::Into("name-box", "F200\u{E007}"),
            Action::Keys("grid", F2),
            Action::Window(390, 844),
        ],
        checks: &[
            Check::Reachable("cell-editor"),
            Check::Focused("cell-editor"),
            Check::Reachable("sheet-tabs"),
            Check::Reachable("worker-status"),
        ],
    },
    Case {
        name: "shared shell: leaving a resized editor keeps its authored source",
        actions: &[
            Action::Keys("cell-editor", ESCAPE),
            Action::Window(1440, 1000),
        ],
        checks: &[Check::Value("name-box", "F200"), Check::Value(BAR, "")],
    },
    Case {
        name: "shared tree semantics: an empty active cell still has a rendered outline focus",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Click("skin-tree"),
        ],
        checks: &[Check::Role("tree", "tree"), Check::ActiveDescendant("tree")],
    },
    Case {
        name: "navigation: contiguous cells stop at their edge",
        actions: &[
            Action::Click("skin-grid"),
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Into(BAR, "1\u{E007}"),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Into(BAR, "2\u{E007}"),
            Action::Into("name-box", "A3\u{E007}"),
            Action::Into(BAR, "3\u{E007}"),
            Action::Into("name-box", "A5\u{E007}"),
            Action::Into(BAR, "5\u{E007}"),
            Action::Into("name-box", "A6\u{E007}"),
            Action::Into(BAR, "6\u{E007}"),
            Action::Into("name-box", "A8\u{E007}"),
            Action::Into(BAR, "=\"\"\u{E007}"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", "\u{E009}\u{E015}\u{E000}"),
        ],
        checks: &[Check::Value("name-box", "A3"), Check::Value(BAR, "3")],
    },
    Case {
        name: "navigation: a blank gap lands at the next occupied cell",
        actions: &[Action::Keys("grid", "\u{E009}\u{E015}\u{E000}")],
        checks: &[Check::Value("name-box", "A5")],
    },
    Case {
        name: "navigation: repeated data-edge keys use canonical worker selection",
        actions: &[Action::Keys("grid", "\u{E009}\u{E015}\u{E015}\u{E000}")],
        checks: &[Check::Value("name-box", "A8"), Check::Value(BAR, "=\"\"")],
    },
    Case {
        name: "navigation: an empty formula result still occupies its cell",
        actions: &[Action::Keys("grid", "\u{E009}\u{E013}\u{E000}")],
        checks: &[Check::Value("name-box", "A6")],
    },
    Case {
        name: "navigation: Ctrl Shift extends to the same data edge",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", "\u{E009}\u{E008}\u{E015}\u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "A1"),
            Check::Attr("selection", "data-area", "A1:A3"),
        ],
    },
    Case {
        name: "selection: upper-right anchored extension keeps both selected columns",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Keys("grid", "\u{E008}\u{E012}\u{E015}\u{E015}\u{E000}"),
            Action::Wait(Check::Attr("selection", "data-area", "A1:B3")),
            Action::Keys("grid", "\u{E008}\u{E015}\u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "B1"),
            Check::Attr("selection", "data-area", "A1:B4"),
        ],
    },
    Case {
        name: "selection: lower-left anchored extension keeps both selected rows",
        actions: &[
            Action::Into("name-box", "A3\u{E007}"),
            Action::Keys("grid", "\u{E008}\u{E013}\u{E013}\u{E014}\u{E000}"),
            Action::Wait(Check::Attr("selection", "data-area", "A1:B3")),
            Action::Keys("grid", "\u{E008}\u{E014}\u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "A3"),
            Check::Attr("selection", "data-area", "A1:C3"),
        ],
    },
    Case {
        name: "navigation: a blank row can reach the physical last column",
        actions: &[
            Action::Into("name-box", "B2\u{E007}"),
            Action::Keys("grid", "\u{E009}\u{E014}\u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "XFD2"),
            Check::Reachable("active-cell"),
            Check::ActiveDescendant("grid"),
        ],
    },
    Case {
        name: "navigation: Ctrl Left finds the preceding occupied cell",
        actions: &[Action::Keys("grid", "\u{E009}\u{E012}\u{E000}")],
        checks: &[Check::Value("name-box", "A2")],
    },
    Case {
        name: "selection: the name box accepts a rectangle",
        actions: &[Action::Into("name-box", "C10:D12\u{E007}")],
        checks: &[
            Check::Value("name-box", "C10"),
            Check::Attr("selection", "data-area", "C10:D12"),
        ],
    },
    Case {
        name: "selection: Enter retains the rectangle while moving the active cell",
        actions: &[Action::Keys("grid", ENTER)],
        checks: &[
            Check::Value("name-box", "C11"),
            Check::Attr("selection", "data-area", "C10:D12"),
            Check::Attr(
                "[data-dnacalc=row-header][data-row='10']",
                "class",
                "dc-head dc-on",
            ),
            Check::Attr(
                "[data-dnacalc=row-header][data-row='12']",
                "class",
                "dc-head dc-on",
            ),
            Check::Attr("#dc-grid-cell-10-3", "aria-selected", "true"),
        ],
    },
    Case {
        name: "selection: unmeasured interior-anchor extension keeps the existing range",
        actions: &[Action::Keys("grid", "\u{E008}\u{E015}\u{E000}")],
        checks: &[
            Check::Value("name-box", "C11"),
            Check::Attr("selection", "data-area", "C10:D12"),
            Check::Starts(
                "status",
                "Range extension from an interior active cell awaits Excel measurement",
            ),
        ],
    },
    Case {
        name: "selection: Enter wraps down columns inside the rectangle",
        actions: &[Action::Keys("grid", "\u{E007}\u{E007}")],
        checks: &[
            Check::Value("name-box", "D10"),
            Check::Attr("selection", "data-area", "C10:D12"),
        ],
    },
    Case {
        name: "selection: Shift Enter reverses the range walk",
        actions: &[Action::Keys("grid", "\u{E008}\u{E007}\u{E000}")],
        checks: &[
            Check::Value("name-box", "C12"),
            Check::Attr("selection", "data-area", "C10:D12"),
        ],
    },
    Case {
        name: "selection: Tab walks across rows then wraps",
        actions: &[Action::Keys("grid", "\u{E004}\u{E004}")],
        checks: &[
            Check::Value("name-box", "C10"),
            Check::Attr("selection", "data-area", "C10:D12"),
        ],
    },
    Case {
        name: "selection: Shift Tab reverses the horizontal walk",
        actions: &[Action::Keys("grid", "\u{E008}\u{E004}\u{E000}")],
        checks: &[
            Check::Value("name-box", "D12"),
            Check::Attr("selection", "data-area", "C10:D12"),
        ],
    },
    Case {
        name: "selection: Ctrl Enter enters a rectangle as one workbook edit",
        actions: &[
            Action::Into("name-box", "B10:B12\u{E007}"),
            Action::Into(BAR, "7\u{E009}\u{E007}\u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "B10"),
            Check::Attr("selection", "data-area", "B10:B12"),
            Check::Text(cell!("B10"), "7"),
            Check::Text(cell!("B11"), "7"),
            Check::Text(cell!("B12"), "7"),
            Check::Absent("edit-session"),
        ],
    },
    Case {
        name: "selection: one Undo removes the entire Ctrl Enter entry",
        actions: &[Action::Keys("grid", "\u{E009}z\u{E000}")],
        checks: &[
            Check::Absent(cell!("B10")),
            Check::Absent(cell!("B11")),
            Check::Absent(cell!("B12")),
            Check::Attr("selection", "data-area", "B10:B12"),
        ],
    },
    Case {
        name: "selection: one Redo restores the entire entry",
        actions: &[Action::Keys("grid", "\u{E009}y\u{E000}")],
        checks: &[
            Check::Text(cell!("B10"), "7"),
            Check::Text(cell!("B12"), "7"),
        ],
    },
    Case {
        name: "selection: Ctrl Enter relocates formula references through the host",
        actions: &[
            Action::Into("name-box", "C10:D12\u{E007}"),
            Action::Into(BAR, "=A1+$A$2\u{E009}\u{E007}\u{E000}"),
        ],
        checks: &[
            Check::Text(cell!("C10"), "3"),
            Check::Text(cell!("C11"), "4"),
            Check::Text(cell!("C12"), "5"),
            Check::Text(cell!("D10"), "2"),
            Check::Attr("selection", "data-area", "C10:D12"),
        ],
    },
    Case {
        name: "selection: moved range source is authored as a relocated formula",
        actions: &[Action::Into("name-box", "C11\u{E007}")],
        checks: &[Check::Value(BAR, "=A2+$A$2")],
    },
    Case {
        name: "selection: Ctrl Space selects a column and retains the active cell",
        actions: &[
            Action::Into("name-box", "E4\u{E007}"),
            Action::Keys("grid", "\u{E009} \u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "E4"),
            Check::Attr("selection", "data-area", "E1:E1048576"),
            Check::Bounded("[role=gridcell]", 5000),
        ],
    },
    Case {
        name: "selection: Shift Space selects a row and retains the active cell",
        actions: &[
            Action::Into("name-box", "E4\u{E007}"),
            Action::Keys("grid", "\u{E008} \u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "E4"),
            Check::Attr("selection", "data-area", "A4:XFD4"),
        ],
    },
    Case {
        name: "selection: Ctrl Space expands every column touched by a rectangle",
        actions: &[
            Action::Into("name-box", "B2:C3\u{E007}"),
            Action::Keys("grid", "\u{E009} \u{E000}"),
        ],
        checks: &[
            Check::Attr("selection", "data-area", "B1:C1048576"),
            Check::Value("name-box", "B2"),
        ],
    },
    Case {
        name: "selection: Shift Space expands every row touched by a rectangle",
        actions: &[
            Action::Into("name-box", "B2:C3\u{E007}"),
            Action::Keys("grid", "\u{E008} \u{E000}"),
        ],
        checks: &[
            Check::Attr("selection", "data-area", "A2:XFD3"),
            Check::Value("name-box", "B2"),
        ],
    },
    Case {
        name: "selection: Ctrl A selects the current region",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", "\u{E009}a\u{E000}"),
        ],
        checks: &[Check::Attr("selection", "data-area", "A1:A3")],
    },
    Case {
        name: "selection: another Ctrl A selects the worksheet",
        actions: &[Action::Keys("grid", "\u{E009}a\u{E000}")],
        checks: &[
            Check::Attr("selection", "data-area", "A1:XFD1048576"),
            Check::Bounded("[role=gridcell]", 5000),
        ],
    },
    Case {
        name: "selection: Ctrl click adds a nonadjacent area",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::CtrlClick(cell!("A3")),
        ],
        checks: &[
            Check::Count("[data-dnacalc=selection]", 2),
            Check::Attr("#dc-grid-cell-1-1", "aria-selected", "true"),
            Check::Attr("#dc-grid-cell-3-1", "aria-selected", "true"),
            Check::Attr("#dc-grid-cell-2-1", "aria-selected", "false"),
            Check::Attr(
                "[data-dnacalc=row-header][data-row='2']",
                "class",
                "dc-head",
            ),
            Check::Attr(
                "[data-dnacalc=row-header][data-row='3']",
                "class",
                "dc-head dc-on",
            ),
        ],
    },
    Case {
        name: "selection: disjoint axis expansion is diagnosed without losing areas",
        actions: &[Action::Keys("grid", "\u{E009} \u{E000}")],
        checks: &[
            Check::Count("[data-dnacalc=selection]", 2),
            Check::Starts(
                "status",
                "Axis expansion for disjoint selections is not modelled",
            ),
        ],
    },
    Case {
        name: "selection: disjoint copying diagnoses the one-rectangle requirement",
        actions: &[Action::Keys("grid", "\u{E009}c\u{E000}")],
        checks: &[
            Check::Count("[data-dnacalc=selection]", 2),
            Check::Starts("status", "Select one rectangle for this command"),
        ],
    },
    Case {
        name: "selection: Ctrl click can remove a selected cell",
        actions: &[Action::CtrlClick(cell!("A1"))],
        checks: &[
            Check::Count("[data-dnacalc=selection]", 1),
            Check::Attr("#dc-grid-cell-1-1", "aria-selected", "false"),
            Check::Attr("#dc-grid-cell-3-1", "aria-selected", "true"),
        ],
    },
    Case {
        name: "selection: Delete clears disjoint areas with one Undo",
        actions: &[
            Action::CtrlClick(cell!("A1")),
            Action::Keys("grid", "\u{E017}"),
        ],
        checks: &[
            Check::Absent(cell!("A1")),
            Check::Absent(cell!("A3")),
            Check::Text(cell!("A2"), "2"),
        ],
    },
    Case {
        name: "selection: disjoint clearing Undo restores both sources and areas",
        actions: &[Action::Keys("grid", "\u{E009}z\u{E000}")],
        checks: &[
            Check::Text(cell!("A1"), "1"),
            Check::Text(cell!("A3"), "3"),
            Check::Count("[data-dnacalc=selection]", 2),
        ],
    },
    Case {
        name: "selection: a real drag beyond the viewport scrolls and extends",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::DragOutside(cell!("A1")),
        ],
        checks: &[Check::DragExtended, Check::Focused("grid")],
    },
    Case {
        name: "navigation: PageDown moves a screen while preserving relative position",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into("name-box", "A10\u{E007}"),
            Action::Keys("grid", "\u{E00F}"),
        ],
        checks: &[Check::PageAligned, Check::Reachable("active-cell")],
    },
    Case {
        name: "navigation: PageUp returns to the original cell",
        actions: &[Action::Keys("grid", "\u{E00E}")],
        checks: &[
            Check::Value("name-box", "A10"),
            Check::Reachable("active-cell"),
        ],
    },
    Case {
        name: "navigation: Ctrl End uses the current used corner",
        actions: &[Action::Keys("grid", "\u{E009}\u{E010}\u{E000}")],
        checks: &[
            Check::Value("name-box", "D12"),
            Check::Reachable("active-cell"),
        ],
    },
    Case {
        name: "navigation: a defined range resolves through the existing workbook catalogue",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Into("name-box", "InputRange\u{E007}"),
        ],
        checks: &[
            Check::Value("name-box", "A1"),
            Check::Attr("selection", "data-area", "A1:A3"),
            Check::Value(BAR, "1"),
        ],
    },
    Case {
        name: "navigation: a sheet-scoped name resolves on its sheet",
        actions: &[Action::Into("name-box", "LocalIn\u{E007}")],
        checks: &[Check::Value("name-box", "A2"), Check::Value(BAR, "2")],
    },
    Case {
        name: "navigation: a qualified range changes sheets",
        actions: &[Action::Into("name-box", "'Target Sheet'!B2:C3\u{E007}")],
        checks: &[
            Check::Value("name-box", "B2"),
            Check::Attr("selection", "data-area", "B2:C3"),
            Check::Attr(
                "[data-dnacalc=sheet-tab][data-sheet='Target Sheet']",
                "aria-selected",
                "true",
            ),
        ],
    },
    Case {
        name: "navigation: invalid input diagnoses the name without changing selection",
        actions: &[Action::Into("name-box", "XFE1\u{E007}")],
        checks: &[
            Check::Value("name-box", "B2"),
            Check::Starts("status", "\"XFE1\" is not a valid"),
            Check::Attr("selection", "data-area", "B2:C3"),
        ],
    },
    Case {
        name: "selection: a pointed cross-sheet edit restores its original range after returning",
        actions: &[
            Action::Into("name-box", "Data!A1:A3\u{E007}"),
            Action::Wait(Check::Attr(
                "[data-dnacalc=sheet-tab][data-sheet=Data]",
                "aria-selected",
                "true",
            )),
            Action::Wait(Check::Value("name-box", "A1")),
            Action::Into(BAR, "=1+"),
            Action::Wait(Check::Value(BAR, "=1+")),
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Other]"),
            Action::Wait(Check::Attr(
                "[data-dnacalc=sheet-tab][data-sheet=Other]",
                "aria-selected",
                "true",
            )),
            Action::Wait(Check::Text(cell!("A1"), "100")),
            Action::Click(cell!("A1")),
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Data]"),
            Action::Keys(BAR, "\u{E009}\u{E007}\u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "A1"),
            Check::Attr("selection", "data-area", "A1:A3"),
            Check::Text(cell!("A1"), "101"),
            Check::Text(cell!("A2"), "1"),
            Check::Text(cell!("A3"), "1"),
        ],
    },
    Case {
        name: "selection: cross-sheet range entry Undo restores all original cells",
        actions: &[Action::Keys("grid", "\u{E009}z\u{E000}")],
        checks: &[
            Check::Text(cell!("A1"), "1"),
            Check::Text(cell!("A2"), "2"),
            Check::Text(cell!("A3"), "3"),
            Check::Attr("selection", "data-area", "A1:A3"),
        ],
    },
    Case {
        name: "navigation: hidden columns are skipped",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_09v/formats.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Into("name-box", "B12\u{E007}"),
            Action::Keys("grid", "\u{E014}"),
        ],
        checks: &[Check::Value("name-box", "D12")],
    },
    Case {
        name: "navigation: hidden rows are skipped",
        actions: &[Action::Keys("grid", "\u{E015}")],
        checks: &[Check::Value("name-box", "D14")],
    },
    Case {
        name: "navigation: unmeasured table selection stages are explicitly diagnosed",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-doc/tests/fixtures/cdoc_m1_2/d1.saved.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Into("name-box", "Data!D2\u{E007}"),
            Action::Keys("grid", "\u{E009}a\u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "D2"),
            Check::Attr("selection", "data-area", "D2:D2"),
            Check::Starts("status", "Table selection stages are not modelled"),
        ],
    },
    Case {
        name: "navigation: physical last cell remains reachable in both browsers",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Into("name-box", "XFD1048576\u{E007}"),
        ],
        checks: &[
            Check::Value("name-box", "XFD1048576"),
            Check::Reachable("active-cell"),
            Check::ActiveDescendant("grid"),
            Check::ScrollAtEnd,
            Check::Bounded("[role=gridcell]", 5000),
        ],
    },
    Case {
        name: "navigation: arrows clamp at the physical worksheet edge",
        actions: &[Action::Keys("grid", "\u{E014}\u{E015}")],
        checks: &[
            Check::Value("name-box", "XFD1048576"),
            Check::Reachable("active-cell"),
        ],
    },
    Case {
        name: "navigation: Home and Ctrl Home use physical worksheet coordinates",
        actions: &[
            Action::Keys("grid", "\u{E011}"),
            Action::Wait(Check::Value("name-box", "A1048576")),
            Action::Keys("grid", "\u{E009}\u{E011}\u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "A1"),
            Check::Reachable("active-cell"),
        ],
    },
    Case {
        name: "navigation: the last row is reachable at maximum zoom",
        actions: &[
            Action::Click("zoom-in"),
            Action::Click("zoom-in"),
            Action::Click("zoom-in"),
            Action::Click("zoom-in"),
            Action::Click("zoom-in"),
            Action::Click("zoom-in"),
            Action::Click("zoom-in"),
            Action::Into("name-box", "XFD1048576\u{E007}"),
        ],
        checks: &[
            Check::Text("zoom", "400%"),
            Check::Reachable("active-cell"),
            Check::ScrollAtEnd,
            Check::ActiveDescendant("grid"),
        ],
    },
    Case {
        name: "navigation: entry at the last physical cell stays in bounds",
        actions: &[Action::Into(BAR, "9\u{E007}")],
        checks: &[
            Check::Value("name-box", "XFD1048576"),
            Check::Value(BAR, "9"),
            Check::Text("selected-value", "9"),
            Check::Reachable("active-cell"),
        ],
    },
    Case {
        name: "clipboard: a real external TSV matrix pastes into a new workbook",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Click("skin-atlas"),
            Action::Click("skin-grid"),
            Action::Wait(Check::Value("name-box", "A1")),
            Action::ExternalClipboard("2\t3\n4\t5", false),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
        ],
        checks: &[
            Check::Text(cell!("A1"), "2"),
            Check::Text(cell!("B1"), "3"),
            Check::Text(cell!("A2"), "4"),
            Check::Text(cell!("B2"), "5"),
            Check::Value(BAR, "2"),
        ],
    },
    Case {
        name: "clipboard: one Undo removes the whole external paste, Redo restores it",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Absent(cell!("A1")),
            Check::Absent(cell!("B1")),
            Check::Absent(cell!("A2")),
            Check::Absent(cell!("B2")),
        ],
    },
    Case {
        name: "clipboard: real HTML table paste decodes entities, formulas and empty cells",
        actions: &[
            Action::ExternalClipboard(
                "<table><tr><td>7</td><td>=A1*2</td></tr><tr><td>A &amp; B</td><td></td></tr></table>",
                true,
            ),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
        ],
        checks: &[
            Check::Text(cell!("A1"), "7"),
            Check::Text(cell!("B1"), "14"),
            Check::Text(cell!("A2"), "A & B"),
            Check::Absent(cell!("B2")),
        ],
    },
    Case {
        name: "clipboard: a conflict previews without changing cells; Escape cancels",
        actions: &[
            Action::ExternalClipboard("99\t100", false),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
            Action::Wait(Check::TextContains("paste-preview", "2 existing inputs")),
            Action::Keys("paste-confirm", ESCAPE),
        ],
        checks: &[
            Check::Text(cell!("A1"), "7"),
            Check::Text(cell!("B1"), "14"),
            Check::Absent("[data-dnacalc=clipboard-dialog][open]"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "clipboard: confirming replaces the destination as one undo group",
        actions: &[
            Action::Keys("grid", "\u{E009}v\u{E000}"),
            Action::Wait(Check::TextContains("paste-preview", "2 existing inputs")),
            Action::Click("paste-confirm"),
        ],
        checks: &[
            Check::Text(cell!("A1"), "99"),
            Check::Text(cell!("B1"), "100"),
        ],
    },
    Case {
        name: "clipboard: Undo restores the overwritten formula and value",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Text(cell!("A1"), "7"),
            Check::Text(cell!("B1"), "14"),
            Check::Attr(cell!("B1"), "data-formula", "true"),
        ],
    },
    Case {
        name: "clipboard: copied formulas are immutable snapshots with mixed anchors",
        actions: &[
            Action::Into("name-box", "C1\u{E007}"),
            Action::Into(BAR, "=$A1+A$1+$A$1\u{E007}"),
            Action::Into("name-box", "C1\u{E007}"),
            Action::Keys("grid", "\u{E009}c\u{E000}"),
            Action::Wait(Check::Count("copy-outline", 1)),
            Action::Into(BAR, "=999\u{E007}"),
            Action::Into("name-box", "D2\u{E007}"),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
        ],
        checks: &[
            Check::Value(BAR, "=$A2+B$1+$A$1"),
            Check::Attr(cell!("D2"), "data-formula", "true"),
            Check::Text(cell!("C1"), "999"),
        ],
    },
    Case {
        name: "clipboard: Paste Special values uses the value captured at copy time",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Keys("grid", "\u{E009}c\u{E000}"),
            Action::Wait(Check::Count("copy-outline", 1)),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into(BAR, "10\u{E007}"),
            Action::Click("[data-dnacalc=clipboard-tools] summary"),
            Action::Choose("paste-mode", "values"),
            Action::Into("name-box", "F1\u{E007}"),
            Action::Click("paste-snapshot"),
        ],
        checks: &[
            Check::Text(cell!("F1"), "14"),
            Check::Attr(cell!("F1"), "data-formula", "false"),
            Check::Text(cell!("B1"), "20"),
        ],
    },
    Case {
        name: "clipboard: the explicit text route needs no clipboard API permissions",
        actions: &[
            Action::Choose("paste-mode", "all"),
            Action::Into("name-box", "F3\u{E007}"),
            Action::ExternalClipboard("8\t9\n10\t11", false),
            Action::Click("paste-fallback"),
            Action::Keys("clipboard-text", "\u{E009}v\u{E000}"),
            Action::Click("paste-text"),
        ],
        checks: &[
            Check::Text(cell!("F3"), "8"),
            Check::Text(cell!("G3"), "9"),
            Check::Text(cell!("F4"), "10"),
            Check::Text(cell!("G4"), "11"),
        ],
    },
    Case {
        name: "clipboard: a malformed external matrix fails atomically",
        actions: &[
            Action::Into("name-box", "F3\u{E007}"),
            Action::ExternalClipboard("1\t2\n3", false),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
        ],
        checks: &[
            Check::Starts("status", "Paste a rectangular table"),
            Check::Text(cell!("F3"), "8"),
            Check::Text(cell!("G4"), "11"),
        ],
    },
    Case {
        name: "clipboard: a partial spill write is refused before any neighboring edit",
        actions: &[
            Action::Into("name-box", "I1\u{E007}"),
            Action::Into(BAR, "=SEQUENCE(3)\u{E007}"),
            Action::Into("name-box", "H2\u{E007}"),
            Action::ExternalClipboard("5\t6", false),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
        ],
        checks: &[
            Check::TextContains("status", "part of the array"),
            Check::Absent(cell!("H2")),
            Check::Text(cell!("I2"), "2"),
        ],
    },
    Case {
        name: "fill: the handle previews numeric series and cancellation leaves cells empty",
        actions: &[
            Action::Into("name-box", "A6\u{E007}"),
            Action::Into(BAR, "1\u{E007}"),
            Action::Into(BAR, "3\u{E007}"),
            Action::Into("name-box", "A6:A7\u{E007}"),
            Action::DragToCell("fill-handle", 10, "A"),
            Action::Wait(Check::TextContains("paste-preview", "3 cells")),
            Action::Click("paste-cancel"),
        ],
        checks: &[
            Check::Absent(cell!("A8")),
            Check::Absent(cell!("A10")),
            Check::Attr("selection", "data-area", "A6:A7"),
        ],
    },
    Case {
        name: "fill: a numeric series extends its seeds with one Undo",
        actions: &[
            Action::DragToCell("fill-handle", 10, "A"),
            Action::Wait(Check::TextContains("paste-preview", "3 cells")),
            Action::Click("paste-confirm"),
        ],
        checks: &[
            Check::Text(cell!("A8"), "5"),
            Check::Text(cell!("A9"), "7"),
            Check::Text(cell!("A10"), "9"),
        ],
    },
    Case {
        name: "fill: one Undo removes the series and retains its seeds",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Text(cell!("A6"), "1"),
            Check::Text(cell!("A7"), "3"),
            Check::Absent(cell!("A8")),
            Check::Absent(cell!("A10")),
        ],
    },
    Case {
        name: "fill: keyboard users preview and apply the same series through an end address",
        actions: &[
            Action::Into("fill-through", "A10\u{E007}"),
            Action::Wait(Check::TextContains("paste-preview", "3 cells")),
            Action::Click("paste-confirm"),
        ],
        checks: &[
            Check::Text(cell!("A8"), "5"),
            Check::Text(cell!("A10"), "9"),
        ],
    },
    Case {
        name: "fill: keyboard series is one Undo and compact commands fit a phone viewport",
        actions: &[Action::Click("undo"), Action::Window(390, 844)],
        checks: &[
            Check::Absent(cell!("A10")),
            Check::Fits,
            Check::Reachable("paste"),
            Check::Reachable("fill-through-preview"),
        ],
    },
    Case {
        name: "clipboard: native copy exports a table readable in a plain text editor",
        actions: &[
            Action::Window(1440, 1000),
            Action::Into("name-box", "F3:G4\u{E007}"),
            Action::Keys("grid", "\u{E009}c\u{E000}"),
            Action::PauseMillis(150),
            Action::Click("paste-fallback"),
            Action::Keys("clipboard-text", "\u{E009}v\u{E000}"),
        ],
        checks: &[Check::Value("clipboard-text", "8\t9\n10\t11")],
    },
    Case {
        name: "clipboard: external blanks clear a selected rectangle and repeat as a whole matrix",
        actions: &[
            Action::Click("paste-cancel"),
            Action::Into("name-box", "F3:G4\u{E007}"),
            Action::ExternalClipboard("\t12", false),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
            Action::Wait(Check::TextContains("paste-preview", "4 existing inputs")),
            Action::Click("paste-confirm"),
        ],
        checks: &[
            Check::Absent(cell!("F3")),
            Check::Absent(cell!("F4")),
            Check::Text(cell!("G3"), "12"),
            Check::Text(cell!("G4"), "12"),
        ],
    },
    Case {
        name: "clipboard: a failed cut paste retains the source and its snapshot",
        actions: &[
            Action::Into("name-box", "G3:G4\u{E007}"),
            Action::Keys("grid", "\u{E009}x\u{E000}"),
            Action::Into("name-box", "K1:L3\u{E007}"),
            Action::Click("paste-snapshot"),
        ],
        checks: &[
            Check::TextContains("status", "not a whole multiple"),
            Check::Text(cell!("G3"), "12"),
            Check::Text(cell!("G4"), "12"),
            Check::Attr("copy-outline", "data-cut", "true"),
            Check::Absent(cell!("K1")),
        ],
    },
    Case {
        name: "clipboard: cancelling a cut overwrite retains both source and destination",
        actions: &[
            Action::Into("name-box", "A6:A7\u{E007}"),
            Action::Click("paste-snapshot"),
            Action::Wait(Check::TextContains("paste-preview", "2 existing inputs")),
            Action::Click("paste-cancel"),
        ],
        checks: &[
            Check::Text(cell!("G3"), "12"),
            Check::Text(cell!("G4"), "12"),
            Check::Text(cell!("A6"), "1"),
            Check::Text(cell!("A7"), "3"),
        ],
    },
    Case {
        name: "clipboard: an edited cut source is refused without clearing either range",
        actions: &[
            Action::Into("name-box", "G3\u{E007}"),
            Action::Into(BAR, "15\u{E007}"),
            Action::Into("name-box", "K1\u{E007}"),
            Action::Click("paste-snapshot"),
        ],
        checks: &[
            Check::TextContains("status", "cut source changed"),
            Check::Text(cell!("G3"), "15"),
            Check::Absent(cell!("K1")),
        ],
    },
    Case {
        name: "fill: formula handle fill keeps relative and mixed references",
        actions: &[
            Action::Into("name-box", "C6\u{E007}"),
            Action::Into(BAR, "=$A6+A$6\u{E007}"),
            Action::Into("name-box", "C6\u{E007}"),
            Action::DragToCell("fill-handle", 9, "C"),
            Action::Wait(Check::TextContains("paste-preview", "3 cells")),
            Action::Click("paste-confirm"),
            Action::Into("name-box", "C9\u{E007}"),
        ],
        checks: &[
            Check::Value(BAR, "=$A9+A$6"),
            Check::Text(cell!("C7"), "4"),
            Check::Text(cell!("C9"), "1"),
        ],
    },
    Case {
        name: "fill: Ctrl R repeats the first column as one operation",
        actions: &[
            Action::Into("name-box", "A6:B7\u{E007}"),
            Action::Keys("grid", "\u{E009}r\u{E000}"),
        ],
        checks: &[Check::Text(cell!("B6"), "1"), Check::Text(cell!("B7"), "3")],
    },
    Case {
        name: "fill: an open formula draft disables the keyboard fill commands",
        actions: &[
            Action::Into("name-box", "A6\u{E007}"),
            Action::Into(BAR, "2"),
        ],
        checks: &[
            Check::Value(BAR, "2"),
            Check::Attr("fill-through", "disabled", ""),
            Check::Attr("fill-through-preview", "disabled", ""),
            Check::Absent("[data-dnacalc=clipboard-dialog][open]"),
            Check::Text(cell!("A6"), "1"),
        ],
    },
    Case {
        name: "clipboard: toolbar Paste refuses several selected areas before reading the clipboard",
        actions: &[
            Action::Keys(BAR, ESCAPE),
            Action::Into("name-box", "A6\u{E007}"),
            Action::CtrlClick(cell!("B7")),
            Action::Click("paste"),
        ],
        checks: &[
            Check::TextContains("status", "Select one rectangle"),
            Check::Text(cell!("A6"), "1"),
            Check::Text(cell!("B7"), "3"),
            Check::Absent("[data-dnacalc=clipboard-dialog][open]"),
        ],
    },
    Case {
        name: "clipboard: Paste Special formats retains content and uses the copied cell format",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_09v/formats.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "formats.xlsx")),
            Action::Into("name-box", "G3\u{E007}"),
            Action::Into(BAR, "45323\u{E007}"),
            Action::Into("name-box", "A3\u{E007}"),
            Action::Keys("grid", "\u{E009}c\u{E000}"),
            Action::Choose("paste-mode", "formats"),
            Action::Into("name-box", "G3\u{E007}"),
            Action::Click("paste-snapshot"),
        ],
        checks: &[
            Check::Text(cell!("G3"), "########"),
            Check::Value(BAR, "45323"),
            Check::Attr(cell!("G3"), "data-format", "applied"),
            Check::Attr(cell!("G3"), "data-formula", "false"),
        ],
    },
    Case {
        name: "clipboard: Undo repaints a format-only paste without changing its input",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Text(cell!("G3"), "45323"),
            Check::Value(BAR, "45323"),
            Check::Attr(cell!("G3"), "data-format", "general"),
        ],
    },
    Case {
        name: "clipboard: Redo repaints a format-only paste",
        actions: &[Action::Click("redo")],
        checks: &[Check::Text(cell!("G3"), "########")],
    },
    Case {
        name: "clipboard: Paste Special formulas keeps the destination format",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into(BAR, "=45000+323\u{E007}"),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Keys("grid", "\u{E009}c\u{E000}"),
            Action::Choose("paste-mode", "formulas"),
            Action::Into("name-box", "G3\u{E007}"),
            Action::Click("paste-snapshot"),
            Action::Wait(Check::TextContains("paste-preview", "1 existing inputs")),
            Action::Click("paste-confirm"),
        ],
        checks: &[
            Check::Attr(cell!("G3"), "data-format", "applied"),
            Check::Attr(cell!("G3"), "data-formula", "true"),
        ],
    },
    Case {
        name: "fill: date handle fill extends daily dates with the source format",
        actions: &[
            Action::Choose("paste-mode", "all"),
            Action::Into("name-box", "A3\u{E007}"),
            Action::DragToCell("fill-handle", 5, "A"),
            Action::Wait(Check::TextContains("paste-preview", "2 cells")),
            Action::Click("paste-confirm"),
        ],
        checks: &[
            Check::Text(cell!("A4"), "2024-02-01"),
            Check::Text(cell!("A5"), "2024-02-02"),
        ],
    },
    Case {
        name: "clipboard: special formats and a filled date survive save and reopen",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("formats.xlsx"),
            Action::DiscardReplacement,
        ],
        checks: &[
            Check::Text(cell!("A5"), "2024-02-02"),
            Check::Attr(cell!("G3"), "data-format", "applied"),
            Check::Attr(cell!("G3"), "data-formula", "true"),
        ],
    },
    Case {
        name: "clipboard: stale spill-child copy refuses an unavailable value without clearing the target",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "Book1.xlsx")),
            Action::Into(BAR, "3\u{E007}"),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into(BAR, "=SEQUENCE(A1)\u{E007}"),
            Action::Into("name-box", "D1\u{E007}"),
            Action::Into(BAR, "99\u{E007}"),
            Action::Choose("calc-mode", "manual"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into(BAR, "4\u{E007}"),
            Action::Into("name-box", "B2\u{E007}"),
            Action::Keys("grid", "\u{E009}c\u{E000}"),
            Action::Into("name-box", "D1\u{E007}"),
            Action::Click("paste-snapshot"),
        ],
        checks: &[
            Check::TextContains("status", "no current calculated value"),
            Check::Text(cell!("D1"), "99"),
            Check::Attr(cell!("B2"), "data-provenance", "stale"),
            Check::Absent("[data-dnacalc=clipboard-dialog][open]"),
        ],
    },
    Case {
        name: "clipboard: a protected sheet refuses a paste atomically",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "Book1.xlsx")),
            Action::Into(BAR, "7\u{E007}"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::ProtectedDownloadFixture,
            Action::UploadDownload("protected.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "protected.xlsx")),
            Action::Into("name-box", "A1\u{E007}"),
            Action::ExternalClipboard("9\t10", false),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
        ],
        checks: &[
            Check::TextContains("status", "protected sheet"),
            Check::Text(cell!("A1"), "7"),
            Check::Absent(cell!("B1")),
            Check::Text("document", "protected.xlsx"),
        ],
    },
    Case {
        name: "panes: imported frozen rows and columns remain fixed during a real wheel scroll",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-doc/tests/fixtures/dc_5sr/features.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "features.xlsx")),
            Action::Click("skin-atlas"),
            Action::Click("skin-grid"),
            Action::Wait(Check::Attr("grid", "data-frozen-rows", "1")),
            Action::Wait(Check::Attr("grid", "data-frozen-columns", "1")),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::Reachable(cell!("A1"))),
            Action::RememberView(cell!("A1")),
            Action::ScrollGrid(180, 300),
        ],
        checks: &[Check::PinnedCell, Check::Bounded("[role=gridcell]", 1800)],
    },
    Case {
        name: "panes: a click in the frozen corner edits the original cell after scrolling",
        actions: &[Action::Click(cell!("A1")), Action::Keys("grid", F2)],
        checks: &[
            Check::Value("name-box", "A1"),
            Check::Focused("cell-editor"),
        ],
    },
    Case {
        name: "panes: Unfreeze changes the viewport through a worker edit",
        actions: &[
            Action::Keys("cell-editor", ESCAPE),
            Action::Click("[data-dnacalc=pane-tools] summary"),
            Action::Click("unfreeze"),
        ],
        checks: &[
            Check::Attr("grid", "data-frozen-rows", "0"),
            Check::Attr("grid", "data-frozen-columns", "0"),
            Check::Style(cell!("A1"), "width:145px"),
            Check::Style(cell!("A3"), "height:40px"),
        ],
    },
    Case {
        name: "panes: Undo restores both imported frozen boundaries",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Attr("grid", "data-frozen-rows", "1"),
            Check::Attr("grid", "data-frozen-columns", "1"),
        ],
    },
    Case {
        name: "panes: Redo removes the restored boundaries",
        actions: &[Action::Click("redo")],
        checks: &[
            Check::Attr("grid", "data-frozen-rows", "0"),
            Check::Attr("grid", "data-frozen-columns", "0"),
        ],
    },
    Case {
        name: "panes: Freeze above and left resolves the active cell after pending navigation",
        actions: &[
            Action::Into("name-box", "C3\u{E007}"),
            Action::Click("freeze-active"),
        ],
        checks: &[
            Check::Attr("grid", "data-frozen-rows", "2"),
            Check::Attr("grid", "data-frozen-columns", "2"),
            Check::Reachable("active-cell"),
        ],
    },
    Case {
        name: "panes: authored frozen boundaries survive save and reopen",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("features.xlsx"),
            Action::DiscardReplacement,
        ],
        checks: &[
            Check::Attr("grid", "data-frozen-rows", "2"),
            Check::Attr("grid", "data-frozen-columns", "2"),
        ],
    },
    Case {
        name: "panes: first-row and first-column commands replace the frozen axes",
        actions: &[
            Action::Click("freeze-row"),
            Action::Wait(Check::Attr("grid", "data-frozen-rows", "1")),
            Action::Wait(Check::Attr("grid", "data-frozen-columns", "0")),
            Action::Click("freeze-column"),
        ],
        checks: &[
            Check::Attr("grid", "data-frozen-rows", "0"),
            Check::Attr("grid", "data-frozen-columns", "1"),
        ],
    },
    Case {
        name: "view: an offscreen active selection retains scroll and direct zoom across skins",
        actions: &[
            Action::Click("unfreeze"),
            Action::Click("[data-dnacalc=pane-tools] summary"),
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "cut-base.xlsx")),
            Action::Into("name-box", "M100:N102\u{E007}"),
            Action::Into("zoom-percent", "150\u{E007}"),
            Action::Wait(Check::Text("zoom", "150%")),
            Action::ScrollGrid(40, 100),
            Action::RememberView("active-cell"),
            Action::Click("skin-atlas"),
            Action::Click("skin-grid"),
        ],
        checks: &[
            Check::ViewRetained,
            Check::Attr("selection", "data-area", "M100:N102"),
            Check::Text("zoom", "150%"),
        ],
    },
    Case {
        name: "view: each sheet retains its own selection, viewport and zoom",
        actions: &[
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Other]"),
            Action::Into("name-box", "C8:D9\u{E007}"),
            Action::Into("zoom-percent", "75\u{E007}"),
            Action::Wait(Check::Text("zoom", "75%")),
            Action::ScrollGrid(30, 80),
            Action::RememberView("active-cell"),
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Data]"),
        ],
        checks: &[
            Check::ViewRetained,
            Check::Value("name-box", "M100"),
            Check::Text("zoom", "150%"),
        ],
    },
    Case {
        name: "view: Ctrl PageDown restores the next worksheet view",
        actions: &[Action::Keys("grid", "\u{E009}\u{E00F}\u{E000}")],
        checks: &[
            Check::ViewRetained,
            Check::Value("name-box", "C8"),
            Check::Text("zoom", "75%"),
            Check::Attr("selection", "data-area", "C8:D9"),
        ],
    },
    Case {
        name: "view: invalid percentages retain zoom and Reset returns to one hundred percent",
        actions: &[
            Action::Into("zoom-percent", "999\u{E007}"),
            Action::Wait(Check::TextContains(
                "status",
                "zoom percentage from 50 to 400",
            )),
            Action::Wait(Check::Text("zoom", "75%")),
            Action::Click("zoom-reset"),
        ],
        checks: &[
            Check::Text("zoom", "100%"),
            Check::Value("zoom-percent", "100"),
        ],
    },
    Case {
        name: "view: the full last-row label and address fit at phone width",
        actions: &[
            Action::Into("name-box", "XFD1048576\u{E007}"),
            Action::Wait(Check::Value("name-box", "XFD1048576")),
            Action::Wait(Check::Reachable("active-cell")),
            Action::Window(390, 844),
        ],
        checks: &[
            Check::Value("name-box", "XFD1048576"),
            Check::ScrollAtEnd,
            Check::ActiveDescendant("grid"),
            Check::FitsText("name-box"),
            Check::FitsText("[data-dnacalc=row-header][data-row='1048576']"),
            Check::Reachable("active-cell"),
            Check::Fits,
        ],
    },
    Case {
        name: "view: restore the desktop viewport after layout cases",
        actions: &[
            Action::Window(1440, 1000),
            Action::Into("name-box", "A1\u{E007}"),
        ],
        checks: &[
            Check::Value("name-box", "A1"),
            Check::Reachable("active-cell"),
        ],
    },
    Case {
        name: "axis: typed dimensions resize every selected row and column through the worker",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_09v/formats.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "formats.xlsx")),
            Action::Into("name-box", "A1:B3\u{E007}"),
            Action::Click("[data-dnacalc=axis-tools] summary"),
            Action::Into("column-size", "20\u{E007}"),
            Action::Wait(Check::Style(cell!("A1"), "width:145px")),
            Action::Into("row-size", "30\u{E007}"),
        ],
        checks: &[
            Check::Style(cell!("A1"), "left:64px;top:22px;width:145px;height:40px"),
            Check::Style(cell!("B1"), "left:209px;top:22px;width:145px;height:40px"),
            Check::Style(cell!("A3"), "top:102px;height:40px"),
            Check::Absent("cell-editor"),
        ],
    },
    Case {
        name: "axis: one Undo restores the whole row-height command and keeps widths",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Style(cell!("A1"), "width:145px;height:20px"),
            Check::Style(cell!("A3"), "top:62px;height:40px"),
        ],
    },
    Case {
        name: "axis: Redo restores heights and Hide removes a column without clearing its cells",
        actions: &[
            Action::Click("redo"),
            Action::Wait(Check::Style(cell!("A1"), "height:40px")),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Click("hide-columns"),
        ],
        checks: &[
            Check::Absent(cell!("B1")),
            Check::Style(cell!("D1"), "left:209px"),
            Check::Value("name-box", "B1"),
        ],
    },
    Case {
        name: "axis: Show returns the retained custom column width",
        actions: &[Action::Click("show-columns")],
        checks: &[
            Check::Style(cell!("B1"), "left:209px;width:145px"),
            Check::Style(cell!("D1"), "left:354px"),
        ],
    },
    Case {
        name: "axis: Hide and Show each occupy exactly one Undo step",
        actions: &[
            Action::Click("undo"),
            Action::Wait(Check::Absent(cell!("B1"))),
            Action::Click("undo"),
        ],
        checks: &[Check::Style(cell!("B1"), "width:145px")],
    },
    Case {
        name: "axis: hiding a row closes its gap and keeps the following row's size",
        actions: &[
            Action::Into("name-box", "A2\u{E007}"),
            Action::Click("hide-rows"),
        ],
        checks: &[
            Check::Absent(cell!("A2")),
            Check::Style(cell!("A3"), "top:62px;height:40px"),
        ],
    },
    Case {
        name: "axis: Show, Undo and Redo preserve the hidden row's custom height",
        actions: &[
            Action::Click("show-rows"),
            Action::Wait(Check::Style(cell!("A2"), "height:40px")),
            Action::Click("undo"),
            Action::Wait(Check::Absent(cell!("A2"))),
            Action::Click("redo"),
        ],
        checks: &[
            Check::Style(cell!("A2"), "height:40px"),
            Check::Style(cell!("A3"), "top:102px;height:40px"),
        ],
    },
    Case {
        name: "axis: a zero width hides and Show retains the last nonzero width",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into("column-size", "0\u{E007}"),
            Action::Wait(Check::Absent(cell!("B1"))),
            Action::Click("show-columns"),
        ],
        checks: &[Check::Style(cell!("B1"), "width:145px")],
    },
    Case {
        name: "axis: invalid height diagnoses the limit and leaves geometry unchanged",
        actions: &[Action::Into("row-size", "410\u{E007}")],
        checks: &[
            Check::TextContains("status", "409 points"),
            Check::Style(cell!("A1"), "height:40px"),
            Check::Absent("cell-editor"),
        ],
    },
    Case {
        name: "axis: physical widths, heights and hidden bands survive save and reopen",
        actions: &[
            Action::Click("hide-columns"),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Click("hide-rows"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("formats.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "formats.xlsx")),
        ],
        checks: &[
            Check::Absent(cell!("B1")),
            Check::Absent(cell!("A2")),
            Check::Style(cell!("A1"), "width:145px;height:40px"),
            Check::Style(cell!("A3"), "top:62px;height:40px"),
            Check::Style(cell!("D1"), "left:209px"),
        ],
    },
    Case {
        name: "axis: a rectangle spanning hidden bands shows them, including an imported zero-width column",
        actions: &[
            Action::Into("name-box", "A1:C3\u{E007}"),
            Action::Click("show-columns"),
            Action::Click("show-rows"),
        ],
        checks: &[
            Check::Style(cell!("B1"), "width:145px"),
            Check::Style("[data-dnacalc=column-header][data-column=C]", "width:64px"),
            Check::Style(cell!("A2"), "height:40px"),
            Check::Style(cell!("D1"), "left:418px"),
        ],
    },
    Case {
        name: "axis: whole-column insertion moves the authored widths and Undo restores them",
        actions: &[
            Action::Click("[data-dnacalc=column-header][data-column=B]"),
            Action::Click("insert-cells"),
        ],
        checks: &[
            Check::Style(cell!("C1"), "width:145px"),
            Check::Style("[data-dnacalc=column-header][data-column=B]", "width:145px"),
        ],
    },
    Case {
        name: "axis: dimension controls and labels remain reachable at phone width",
        actions: &[
            Action::Click("undo"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Window(390, 844),
        ],
        checks: &[
            Check::Reachable("column-size"),
            Check::Reachable("row-size"),
            Check::Reachable("show-columns"),
            Check::Reachable("show-rows"),
            Check::Fits,
        ],
    },
    Case {
        name: "axis: hidden rows and column widths recalculate SUBTOTAL and CELL through the app route",
        actions: &[
            Action::Window(1440, 1000),
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "Book1.xlsx")),
            Action::Into(BAR, "1\u{E007}"),
            Action::Into(BAR, "2\u{E007}"),
            Action::Into("name-box", "C1\u{E007}"),
            Action::Into(BAR, "=SUBTOTAL(109,A1:A2)\u{E007}"),
            Action::Into("name-box", "C3\u{E007}"),
            Action::Into(BAR, "=SUM(CELL(\"width\",B1))\u{E007}"),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into("column-size", "20\u{E007}"),
            Action::Wait(Check::Text(cell!("C3"), "20")),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Click("hide-rows"),
        ],
        checks: &[
            Check::Text(cell!("C1"), "1"),
            Check::Text(cell!("C3"), "20"),
        ],
    },
    Case {
        name: "axis: Undo of hiding recalculates the dependent total without changing inputs",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Text(cell!("C1"), "3"),
            Check::Text(cell!("A1"), "1"),
            Check::Text(cell!("A2"), "2"),
        ],
    },
    Case {
        name: "axis: dragging a selected header edge resizes all selected columns in one command",
        actions: &[
            Action::Into("name-box", "A1:B1048576\u{E007}"),
            Action::ResizeHeader(
                "[data-dnacalc=column-header][data-column=B]",
                false,
                70,
                false,
            ),
        ],
        checks: &[
            Check::Style(cell!("A1"), "width:215px"),
            Check::Style("[data-dnacalc=column-header][data-column=B]", "width:215px"),
            Check::Text(cell!("C3"), "30"),
            Check::Absent("resize-preview"),
        ],
    },
    Case {
        name: "axis: Undo restores the different widths before a group header drag",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Style(cell!("A1"), "width:64px"),
            Check::Style("[data-dnacalc=column-header][data-column=B]", "width:145px"),
            Check::Text(cell!("C3"), "20"),
        ],
    },
    Case {
        name: "axis: dragging a row boundary resizes selected rows in point units",
        actions: &[
            Action::Into("name-box", "A1:XFD2\u{E007}"),
            Action::ResizeHeader("[data-dnacalc=row-header][data-row='1']", true, 20, false),
        ],
        checks: &[
            Check::Style(cell!("A1"), "height:40px"),
            Check::Style(cell!("A2"), "height:40px"),
            Check::Absent("resize-preview"),
        ],
    },
    Case {
        name: "axis: Escape cancels header resizing without an Undo entry",
        actions: &[
            Action::Click("undo"),
            Action::Wait(Check::Style(cell!("A1"), "height:20px")),
            Action::Into("name-box", "B1\u{E007}"),
            Action::ResizeHeader(
                "[data-dnacalc=column-header][data-column=B]",
                false,
                35,
                true,
            ),
            Action::Wait(Check::Style(
                "[data-dnacalc=column-header][data-column=B]",
                "width:145px",
            )),
            Action::Click("undo"),
        ],
        checks: &[
            Check::Style("[data-dnacalc=column-header][data-column=B]", "width:64px"),
            Check::Absent("resize-preview"),
            Check::Text(cell!("A2"), "2"),
        ],
    },
    Case {
        name: "axis: dragging to zero hides a column and Show restores its retained width",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::ResizeHeader(
                "[data-dnacalc=column-header][data-column=B]",
                false,
                -64,
                false,
            ),
            Action::Wait(Check::Absent("[data-dnacalc=column-header][data-column=B]")),
            Action::Wait(Check::Style(cell!("C1"), "left:128px")),
            Action::Click("show-columns"),
        ],
        checks: &[
            Check::Style("[data-dnacalc=column-header][data-column=B]", "width:64px"),
            Check::Style(cell!("C1"), "left:192px"),
        ],
    },
    Case {
        name: "axis: a zoomed header drag changes physical width independently of zoom",
        actions: &[
            Action::Into("zoom-percent", "200%\u{E007}"),
            Action::Wait(Check::Text("zoom", "200%")),
            Action::Into("name-box", "A1\u{E007}"),
            Action::ResizeHeader(
                "[data-dnacalc=column-header][data-column=A]",
                false,
                70,
                false,
            ),
            Action::Wait(Check::Style(cell!("A1"), "width:198px")),
            Action::Click("zoom-reset"),
            Action::Wait(Check::Style(cell!("A1"), "width:99px")),
            Action::Click("undo"),
        ],
        checks: &[
            Check::Style(cell!("A1"), "width:64px"),
            Check::Text("zoom", "100%"),
        ],
    },
    Case {
        name: "axis: a dimension edit invalidates imported manual-mode caches without calculating them",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::ManualDownloadFixture,
            Action::UploadDownload("manual-cache.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "manual-cache.xlsx")),
            Action::Wait(Check::Attr(cell!("C3"), "data-provenance", "file-cache")),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into("column-size", "20\u{E007}"),
        ],
        checks: &[
            Check::Attr(cell!("C3"), "data-provenance", "stale"),
            Check::Attr(cell!("C1"), "data-provenance", "stale"),
            Check::Value("calc-mode", "manual"),
        ],
    },
    Case {
        name: "axis: calculation publishes current width and hidden-row results after manual edits",
        actions: &[
            Action::Click("calculate"),
            Action::Wait(Check::Text(cell!("C3"), "20")),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Click("hide-rows"),
            Action::Wait(Check::Attr(cell!("C1"), "data-provenance", "stale")),
            Action::Click("calculate"),
        ],
        checks: &[
            Check::Text(cell!("C1"), "1"),
            Check::Text(cell!("C3"), "20"),
            Check::Attr(cell!("C1"), "data-provenance", "calculated"),
            Check::Attr(cell!("C3"), "data-provenance", "calculated"),
        ],
    },
    Case {
        name: "axis: AutoFit measures selected displayed text and formula results in the grid font",
        actions: &[
            Action::Click("[data-dnacalc=axis-tools] summary"),
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "Book1.xlsx")),
            Action::Into(BAR, "'WWWWWWWWWWWWWWWWWWWW\u{E007}"),
            Action::Into(BAR, "short\u{E007}"),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into(BAR, "12345\u{E007}"),
            Action::Into(BAR, "=REPT(\"x\",30)\u{E007}"),
            Action::Into("name-box", "A1:B2\u{E007}"),
            Action::Click("[data-dnacalc=axis-tools] summary"),
            Action::RememberView("[data-dnacalc=column-header][data-column=A]"),
            Action::Click("autofit-columns"),
        ],
        checks: &[
            Check::TextContains("status", "Fitted contents"),
            Check::FitsText(cell!("A1")),
            Check::FitsText(cell!("B1")),
            Check::FitsText(cell!("B2")),
            Check::Text(cell!("B2"), "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"),
            Check::Text(cell!("B1"), "12345"),
        ],
    },
    Case {
        name: "axis: one Undo restores every width before AutoFit and preserves the contents",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::ViewRetained,
            Check::Style("[data-dnacalc=column-header][data-column=B]", "width:64px"),
            Check::Text(cell!("B2"), "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"),
        ],
    },
    Case {
        name: "axis: AutoFit height shrinks selected unwrapped rows in one command",
        actions: &[
            Action::Click("autofit-columns"),
            Action::Wait(Check::TextContains("status", "Fitted contents")),
            Action::Into("row-size", "40\u{E007}"),
            Action::Wait(Check::Style(cell!("A1"), "height:53px")),
            Action::Click("autofit-rows"),
        ],
        checks: &[
            Check::Style(cell!("A1"), "height:20px"),
            Check::Style(cell!("A2"), "height:20px"),
            Check::TextContains("status", "Fitted contents"),
        ],
    },
    Case {
        name: "axis: Undo and Redo preserve the height fitting as a group",
        actions: &[
            Action::Click("undo"),
            Action::Wait(Check::Style(cell!("A1"), "height:53px")),
            Action::Wait(Check::Style(cell!("A2"), "height:53px")),
            Action::Click("redo"),
        ],
        checks: &[
            Check::Style(cell!("A1"), "height:20px"),
            Check::Style(cell!("A2"), "height:20px"),
        ],
    },
    Case {
        name: "axis: fitted physical dimensions survive a saved package round trip",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("Book1.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "Book1.xlsx")),
        ],
        checks: &[
            Check::FitsText(cell!("A1")),
            Check::FitsText(cell!("B2")),
            Check::Style(cell!("A1"), "height:20px"),
        ],
    },
    Case {
        name: "axis: fitting an empty band retains its size without an Undo entry",
        actions: &[
            Action::Into("name-box", "D1\u{E007}"),
            Action::Into("column-size", "20\u{E007}"),
            Action::Wait(Check::Style(
                "[data-dnacalc=column-header][data-column=D]",
                "width:145px",
            )),
            Action::Click("autofit-columns"),
            Action::Wait(Check::TextContains("status", "No visible nonempty")),
            Action::Wait(Check::Style(
                "[data-dnacalc=column-header][data-column=D]",
                "width:145px",
            )),
            Action::Click("undo"),
        ],
        checks: &[Check::Style(
            "[data-dnacalc=column-header][data-column=D]",
            "width:64px",
        )],
    },
    Case {
        name: "axis: double-clicking a column boundary fits its whole column with one Undo",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into("column-size", "8.43\u{E007}"),
            Action::Wait(Check::Style(
                "[data-dnacalc=column-header][data-column=A]",
                "width:64px",
            )),
            Action::DoubleClickEdge("[data-dnacalc=column-header][data-column=A]", false),
            Action::Wait(Check::FitsText(cell!("A1"))),
            Action::Click("undo"),
        ],
        checks: &[Check::Style(
            "[data-dnacalc=column-header][data-column=A]",
            "width:64px",
        )],
    },
    Case {
        name: "axis: double-clicking a row boundary fits its content with one Undo",
        actions: &[
            Action::Into("row-size", "30\u{E007}"),
            Action::Wait(Check::Style(cell!("A1"), "height:40px")),
            Action::DoubleClickEdge("[data-dnacalc=row-header][data-row='1']", true),
            Action::Wait(Check::Style(cell!("A1"), "height:20px")),
            Action::Click("undo"),
        ],
        checks: &[Check::Style(cell!("A1"), "height:40px")],
    },
    Case {
        name: "axis: AutoFit refuses merged content atomically and explains the limit",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-doc/tests/fixtures/dc_5sr/features.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "features.xlsx")),
            Action::Into("name-box", "H1:I1\u{E007}"),
            Action::RememberView("[data-dnacalc=column-header][data-column=H]"),
            Action::Click("autofit-columns"),
        ],
        checks: &[Check::TextContains("status", "merged"), Check::ViewRetained],
    },
    Case {
        name: "axis: AutoFit refuses wrapping until that display is available",
        actions: &[
            Action::Into("name-box", "B5\u{E007}"),
            Action::Click("autofit-rows"),
        ],
        checks: &[
            Check::TextContains("status", "wrapping"),
            Check::Absent("cell-editor"),
        ],
    },
    Case {
        name: "axis: AutoFit diagnoses an unrendered custom font without changing width",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::RememberView("[data-dnacalc=column-header][data-column=A]"),
            Action::Click("autofit-columns"),
        ],
        checks: &[
            Check::TextContains("status", "custom fonts"),
            Check::ViewRetained,
        ],
    },
    Case {
        name: "axis: AutoFit commands remain reachable at phone width",
        actions: &[Action::Window(390, 844)],
        checks: &[
            Check::Reachable("autofit-columns"),
            Check::Reachable("autofit-rows"),
            Check::Fits,
        ],
    },
    Case {
        name: "shift: a rectangular selection offers explicit insertion choices and Escape keeps cells",
        actions: &[
            Action::Window(1440, 1000),
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into("[data-onecalc=formula]", "1\u{E007}"),
            Action::Into("[data-onecalc=formula]", "2\u{E007}"),
            Action::Into("[data-onecalc=formula]", "3\u{E007}"),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into("[data-onecalc=formula]", "=SUM(A1:A3)\u{E007}"),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Click("insert-cells"),
            Action::Wait(Check::Attr("structure-dialog", "open", "")),
        ],
        checks: &[
            Check::Text("shift-down", "Shift cells down"),
            Check::Text("shift-right", "Shift cells right"),
            Check::Reachable("shift-rows"),
            Check::Role("structure-dialog", "dialog"),
        ],
    },
    Case {
        name: "shift: cancelling the insertion keeps the source and Undo depth",
        actions: &[Action::Keys("shift-down", ESCAPE)],
        checks: &[
            Check::Absent("[data-dnacalc=structure-dialog][open]"),
            Check::Text(cell!("A2"), "2"),
            Check::Text(cell!("B1"), "6"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "shift: down insertion moves only the stripe and expands its range references",
        actions: &[
            Action::Keys("grid", concat!("\u{E009}", "\u{E008}", "=", "\u{E000}")),
            Action::Click("shift-down"),
        ],
        checks: &[
            Check::Absent(cell!("A2")),
            Check::Text(cell!("A3"), "2"),
            Check::Text(cell!("A4"), "3"),
            Check::Text(cell!("B1"), "6"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "shift: a shifted reference is authored source in the formula bar",
        actions: &[Action::Into("name-box", "B1\u{E007}")],
        checks: &[Check::Value("[data-onecalc=formula]", "=SUM(A1:A4)")],
    },
    Case {
        name: "shift: Undo restores the whole shift together",
        actions: &[
            Action::Click("undo"),
            Action::Into("name-box", "B1\u{E007}"),
        ],
        checks: &[
            Check::Text(cell!("A2"), "2"),
            Check::Text(cell!("A3"), "3"),
            Check::Absent(cell!("A4")),
            Check::Value("[data-onecalc=formula]", "=SUM(A1:A3)"),
        ],
    },
    Case {
        name: "shift: right insertion keeps the moved formula reading its original cells",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Click("insert-cells"),
            Action::Click("shift-right"),
        ],
        checks: &[Check::Absent(cell!("B1")), Check::Text(cell!("C1"), "6")],
    },
    Case {
        name: "shift: left deletion closes the gap without deleting adjacent rows",
        actions: &[Action::Click("delete-cells"), Action::Click("shift-right")],
        checks: &[
            Check::Text(cell!("B1"), "6"),
            Check::Absent(cell!("C1")),
            Check::Text(cell!("A2"), "2"),
        ],
    },
    Case {
        name: "shift: deleting a referenced cell authors a worksheet reference error",
        actions: &[
            Action::Into("name-box", "C2\u{E007}"),
            Action::Into("[data-onecalc=formula]", "=$A$2\u{E007}"),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Click("delete-cells"),
            Action::Click("shift-down"),
        ],
        checks: &[
            Check::Text(cell!("A2"), "3"),
            Check::Text(cell!("C2"), "#REF!"),
            Check::Attr(cell!("C2"), "data-provenance", "calculated"),
        ],
    },
    Case {
        name: "shift: saved cell shifts reopen with changed formulas",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("Book1.xlsx"),
            Action::DiscardReplacement,
        ],
        checks: &[
            Check::Text(cell!("A2"), "3"),
            Check::Text(cell!("C2"), "#REF!"),
        ],
    },
    Case {
        name: "shift: unsupported crossing ranges refuse the command with the draft selection intact",
        actions: &[
            Action::Into("name-box", "D1\u{E007}"),
            Action::Into("[data-onecalc=formula]", "=SUM(A1:B4)\u{E007}"),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Click("insert-cells"),
            Action::Click("shift-down"),
        ],
        checks: &[
            Check::TextContains("structure-dialog", "crossing ranges"),
            Check::Attr("structure-dialog", "open", ""),
            Check::Text(cell!("A2"), "3"),
        ],
    },
    Case {
        name: "sheets: add is available from the keyboard-reachable worksheet manager",
        actions: &[
            Action::Click("structure-cancel"),
            Action::Click("[data-dnacalc=sheet-tools] summary"),
            Action::Into("sheet-new-name", "Inputs"),
            Action::Click("sheet-add"),
            Action::Wait(Check::Count("[data-dnacalc=sheet-tab]", 2)),
        ],
        checks: &[
            Check::Text("[data-dnacalc=sheet-tab][aria-selected=true]", "Inputs"),
            Check::Value("name-box", "A1"),
            Check::Value("sheet-rename-name", "Inputs"),
        ],
    },
    Case {
        name: "sheets: new sheets accept engine formulas and keep prior sheet view",
        actions: &[
            Action::Click("[data-dnacalc=sheet-tools] summary"),
            Action::Into("[data-onecalc=formula]", "=Sheet1!A1*10\u{E007}"),
            Action::Into("name-box", "D5\u{E007}"),
            Action::Keys("grid", concat!("\u{E009}", "\u{E00E}", "\u{E000}")),
        ],
        checks: &[
            Check::Text("[data-dnacalc=sheet-tab][aria-selected=true]", "Sheet1"),
            Check::Value("name-box", "A2"),
        ],
    },
    Case {
        name: "sheets: keyboard sheet switching returns to each retained selection",
        actions: &[
            Action::Keys("grid", concat!("\u{E009}", "\u{E00F}", "\u{E000}")),
            Action::Click("[data-dnacalc=sheet-tools] summary"),
        ],
        checks: &[
            Check::Text("[data-dnacalc=sheet-tab][aria-selected=true]", "Inputs"),
            Check::Value("name-box", "D5"),
        ],
    },
    Case {
        name: "sheets: invalid and duplicate names are refused without losing the typed name",
        actions: &[
            Action::Into("sheet-rename-name", "Sheet1"),
            Action::Click("sheet-rename"),
        ],
        checks: &[
            Check::TextContains("status", "already exists"),
            Check::Value("sheet-rename-name", "Sheet1"),
            Check::Text("[data-dnacalc=sheet-tab][aria-selected=true]", "Inputs"),
        ],
    },
    Case {
        name: "sheets: rename quotes apostrophes and rewrites sheet references",
        actions: &[
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Sheet1]"),
            Action::Into("sheet-rename-name", "October's inputs"),
            Action::Click("sheet-rename"),
            Action::Wait(Check::Text(
                "[data-dnacalc=sheet-tab][aria-selected=true]",
                "October's inputs",
            )),
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Inputs]"),
            Action::Click("[data-dnacalc=sheet-tools] summary"),
            Action::Into("name-box", "A1\u{E007}"),
        ],
        checks: &[
            Check::Value("[data-onecalc=formula]", "='October''s inputs'!A1*10"),
            Check::Text(cell!("A1"), "10"),
        ],
    },
    Case {
        name: "sheets: moving a worksheet changes order and preserves its source",
        actions: &[
            Action::Click("[data-dnacalc=sheet-tools] summary"),
            Action::Into("sheet-position", "1"),
            Action::Click("sheet-move"),
        ],
        checks: &[
            Check::Text("[data-dnacalc=sheet-tab]:first-child", "Inputs"),
            Check::Text(cell!("A1"), "10"),
        ],
    },
    Case {
        name: "sheets: hide selects a visible neighbour and exposes the retained hidden sheet",
        actions: &[Action::Click("sheet-hide")],
        checks: &[
            Check::Count("[data-dnacalc=sheet-tab]", 1),
            Check::TextContains("hidden-sheets", "Inputs"),
            Check::Text(
                "[data-dnacalc=sheet-tab][aria-selected=true]",
                "October's inputs",
            ),
        ],
    },
    Case {
        name: "sheets: the last visible sheet cannot be hidden or deleted",
        actions: &[Action::Click("sheet-hide")],
        checks: &[
            Check::TextContains("status", "at least one visible"),
            Check::Count("[data-dnacalc=sheet-tab]", 1),
        ],
    },
    Case {
        name: "sheets: unhide restores the saved sheet and its previous selection",
        actions: &[Action::Click("sheet-unhide")],
        checks: &[
            Check::Count("[data-dnacalc=sheet-tab]", 2),
            Check::Text("[data-dnacalc=sheet-tab][aria-selected=true]", "Inputs"),
            Check::Text(cell!("A1"), "10"),
        ],
    },
    Case {
        name: "sheets: delete asks for a reviewable decision and Escape cancels",
        actions: &[
            Action::Click("sheet-delete"),
            Action::Wait(Check::Attr("sheet-delete-dialog", "open", "")),
            Action::Keys("sheet-delete-cancel", ESCAPE),
        ],
        checks: &[
            Check::Absent("[data-dnacalc=sheet-delete-dialog][open]"),
            Check::Count("[data-dnacalc=sheet-tab]", 2),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "sheets: deleting a sheet rewrites readers and one Undo restores it",
        actions: &[
            Action::Click("sheet-delete"),
            Action::Click("sheet-delete-confirm"),
            Action::Wait(Check::Count("[data-dnacalc=sheet-tab]", 1)),
            Action::Click("undo"),
        ],
        checks: &[Check::Count("[data-dnacalc=sheet-tab]", 2)],
    },
    Case {
        name: "sheets: added data and the reordered hidden sheet survive save and reopen",
        actions: &[
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Inputs]"),
            Action::Click("sheet-hide"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("Book1.xlsx"),
            Action::DiscardReplacement,
        ],
        checks: &[
            Check::Count("[data-dnacalc=sheet-tab]", 1),
            Check::Text("[data-dnacalc=sheet-tab]", "October's inputs"),
        ],
    },
    Case {
        name: "sheets: the hidden worksheet returns from the saved package with its calculated source",
        actions: &[
            Action::Click("[data-dnacalc=sheet-tools] summary"),
            Action::Click("sheet-unhide"),
            Action::Click("[data-dnacalc=sheet-tools] summary"),
            Action::Into("name-box", "A1\u{E007}"),
        ],
        checks: &[
            Check::Value("[data-onecalc=formula]", "='October''s inputs'!A1*10"),
            Check::Text(cell!("A1"), "10"),
        ],
    },
    Case {
        name: "sheets: the full manager and long tab names remain reachable at phone width",
        actions: &[
            Action::Window(390, 844),
            Action::Click("[data-dnacalc=sheet-tools] summary"),
        ],
        checks: &[
            Check::Reachable("sheet-add"),
            Check::Reachable("sheet-rename-name"),
            Check::Fits,
        ],
    },
    Case {
        name: "sheets: pending formula drafts block structural edits without losing text",
        actions: &[
            Action::Window(1440, 1000),
            Action::Click("[data-dnacalc=sheet-tools] summary"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into("[data-onecalc=formula]", "42"),
            Action::Click("insert-cells"),
        ],
        checks: &[
            Check::Value("[data-onecalc=formula]", "42"),
            Check::Absent("[data-dnacalc=structure-dialog][open]"),
            Check::Text(cell!("A1"), "10"),
            Check::TextContains("status", "Enter or cancel"),
        ],
    },
    Case {
        name: "sheets: structural refusal leaves the formula draft cancellable",
        actions: &[Action::Keys("[data-onecalc=formula]", ESCAPE)],
        checks: &[
            Check::Value("[data-onecalc=formula]", "='October''s inputs'!A1*10"),
            Check::Text(cell!("A1"), "10"),
        ],
    },
    Case {
        name: "review markers expose imported notes with accessible content",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-doc/tests/fixtures/dc_5sr/features.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "features.xlsx")),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::Role(
                "[data-dnacalc=review-marker][data-cell=A1]",
                "button",
            )),
            Action::Click("[data-dnacalc=review-marker][data-cell=A1]"),
        ],
        checks: &[
            Check::TextContains("review-note", "a comment"),
            Check::Role("review-dialog", "dialog"),
            Check::Reachable("review-dialog"),
        ],
    },
    Case {
        name: "hyperlink details expose a deliberate external link and its tooltip",
        actions: &[
            Action::Click("review-close"),
            Action::Into("name-box", "K1\u{E007}"),
            Action::Click("[data-dnacalc=review-marker][data-cell=K1]"),
        ],
        checks: &[
            Check::Count("review-link", 1),
            Check::Attr("[data-dnacalc=review-link] a", "rel", "noopener noreferrer"),
            Check::Attr("[data-dnacalc=review-link] a", "title", "a tip"),
        ],
    },
    Case {
        name: "physical row insertion moves notes and links without annotating the new row",
        actions: &[
            Action::Click("review-close"),
            Action::Into("name-box", "1:1\u{E007}"),
            Action::Click("insert-cells"),
            Action::Wait(Check::Absent("[data-dnacalc=review-marker][data-cell=A1]")),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Click("[data-dnacalc=review-marker][data-cell=A2]"),
        ],
        checks: &[
            Check::TextContains("review-note", "a comment"),
            Check::Count("[data-dnacalc=review-marker][data-cell=K2]", 1),
        ],
    },
    Case {
        name: "review content and moved references survive save and reopen",
        actions: &[
            Action::Click("review-close"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::Wait(Check::TextContains("status", "Download started:")),
            Action::UploadDownload("features.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "features.xlsx")),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Click("[data-dnacalc=review-marker][data-cell=A2]"),
        ],
        checks: &[
            Check::TextContains("review-note", "a comment"),
            Check::Count("[data-dnacalc=review-marker][data-cell=K2]", 1),
        ],
    },
    Case {
        name: "review manager remains bounded and its note is reachable on a phone",
        actions: &[
            Action::Click("review-close"),
            Action::Window(390, 844),
            Action::Click("review-tools"),
            Action::Keys("review-close", "\u{E010}"),
        ],
        checks: &[
            Check::Reachable("review-dialog"),
            Check::Reachable("review-note"),
            Check::Contrast("review-note"),
            Check::Fits,
        ],
    },
    Case {
        name: "review Escape closes the phone manager and returns to the grid",
        actions: &[
            Action::Keys("review-close", ESCAPE),
            Action::Window(1440, 1000),
        ],
        checks: &[Check::Focused("grid")],
    },
    Case {
        name: "summary: mixed current values show six aggregates and explicit exclusions",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("status", "New workbook")),
            Action::ExternalClipboard(
                "1\t=SUM(A1:A2)\ttext\n2\t=1/0\tTRUE\n\t=WEBSERVICE(\"https://example.com\")\t=\"\"",
                false,
            ),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
            Action::Wait(Check::Text(cell!("B1"), "3")),
            Action::Into("name-box", "A1:C4\u{E007}"),
            Action::Click("selection-summary-toggle"),
        ],
        checks: &[
            Check::Text("summary-count", "7"),
            Check::Text("summary-numeric", "3"),
            Check::Text("summary-sum", "6"),
            Check::Text("summary-average", "2"),
            Check::Text("summary-min", "1"),
            Check::Text("summary-max", "3"),
            Check::TextContains(
                "summary-provenance",
                "4 blanks · 1 empty text values · 1 errors",
            ),
            Check::TextContains("summary-provenance", "1 refused"),
        ],
    },
    Case {
        name: "summary: stale results are counted apart after a manual edit",
        actions: &[
            Action::Choose("calc-mode", "manual"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", "10\u{E007}"),
            Action::Wait(Check::Text(cell!("A1"), "10")),
            Action::Into("name-box", "A1:C4\u{E007}"),
        ],
        checks: &[
            Check::Text("summary-count", "6"),
            Check::Text("summary-numeric", "2"),
            Check::Text("summary-sum", "12"),
            Check::TextContains("summary-provenance", "1 stale · 1 refused"),
        ],
    },
    Case {
        name: "summary: downloaded manual caches never enter current numeric totals",
        actions: &[
            Action::Choose("calc-mode", "automatic"),
            Action::Click("calculate"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::Wait(Check::Starts("status", "Download started: Book1.xlsx.")),
            Action::ManualDownloadFixture,
            Action::UploadDownload("manual-cache.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "manual-cache.xlsx")),
            Action::Into("name-box", "A1:C4\u{E007}"),
        ],
        checks: &[
            Check::Text("summary-count", "4"),
            Check::Text("summary-numeric", "2"),
            Check::Text("summary-sum", "12"),
            Check::TextContains("summary-provenance", "3 file caches"),
        ],
    },
    Case {
        name: "local menu: protected worksheets disclose why editing commands are disabled",
        actions: &[
            Action::ProtectedDownloadFixture,
            Action::UploadDownload("protected.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "protected.xlsx")),
            Action::Keys("grid", "\u{E008}\u{E03A}\u{E000}"),
            Action::Wait(Check::TextContains("context-reason", "protected")),
        ],
        checks: &[
            Check::Attr("context-cut", "disabled", ""),
            Check::Attr("context-clear", "disabled", ""),
            Check::Focused("context-copy"),
        ],
    },
    Case {
        name: "local menu: disabled commands are skipped by keyboard navigation",
        actions: &[
            Action::Keys("context-copy", "\u{E015}"),
            Action::Wait(Check::Focused("context-find")),
            Action::Keys("context-find", ESCAPE),
        ],
        checks: &[
            Check::Focused("grid"),
            Check::Absent("[data-dnacalc=context-menu][open]"),
        ],
    },
    Case {
        name: "summary: whole-column requests explain their bounded cost without expanding the grid",
        actions: &[Action::Into("name-box", "A:A\u{E007}")],
        checks: &[
            Check::TextContains("selection-summary-toggle", "limited to 10000 cells"),
            Check::Absent("[data-dnacalc=summary-sum]"),
        ],
    },
    Case {
        name: "find: Ctrl F opens a focused workbook dialog and searches authored formulas in the original range",
        actions: &[
            Action::Click("selection-summary-toggle"),
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "cut-base.xlsx")),
            Action::Into("name-box", "A1:C3\u{E007}"),
            Action::Keys("grid", "\u{E009}f\u{E000}"),
            Action::Wait(Check::Focused("find-query")),
            Action::Choose("find-scope", "selection"),
            Action::Into("find-query", "SUM("),
            Action::Keys("find-query", ENTER),
        ],
        checks: &[
            Check::Role("find-dialog", "dialog"),
            Check::Count("find-hit", 2),
            Check::TextContains("find-results", "Data!B2"),
            Check::TextContains("find-results", "Data!C2"),
            Check::Reachable("find-dialog"),
        ],
    },
    Case {
        name: "find: displayed values and hit navigation retain the captured selection scope",
        actions: &[
            Action::Choose("find-look", "values"),
            Action::Into("find-query", "6"),
            Action::Click("find-whole"),
            Action::Click("find-all"),
            Action::Wait(Check::Count("find-hit", 3)),
            Action::Click("[data-dnacalc=find-results] li:last-child [data-dnacalc=find-hit]"),
            Action::Click("find-all"),
        ],
        checks: &[
            Check::Count("find-hit", 3),
            Check::Value("name-box", "C3"),
            Check::TextContains("find-results", "calculated"),
        ],
    },
    Case {
        name: "find: a completed search explains no matches",
        actions: &[
            Action::Into("find-query", "not present anywhere"),
            Action::Click("find-all"),
        ],
        checks: &[
            Check::Text("find-message", "No matches"),
            Check::Count("find-hit", 0),
        ],
    },
    Case {
        name: "find: workbook scope can navigate to another worksheet",
        actions: &[
            Action::Choose("find-scope", "workbook"),
            Action::Into("find-query", "100"),
            Action::Click("find-all"),
            Action::Wait(Check::Count("find-hit", 1)),
            Action::Click("find-hit"),
        ],
        checks: &[
            Check::TextContains("find-results", "Other!A1"),
            Check::Value("name-box", "A1"),
            Check::Text("[data-dnacalc=sheet-tab][aria-selected=true]", "Other"),
        ],
    },
    Case {
        name: "find: Escape returns focus and Ctrl G opens the existing reference resolver",
        actions: &[
            Action::Keys("find-query", ESCAPE),
            Action::Wait(Check::Focused("grid")),
            Action::Keys("grid", "\u{E009}g\u{E000}"),
            Action::Wait(Check::Focused("goto-target")),
            Action::Into("goto-target", "Data!A1:C3\u{E007}"),
        ],
        checks: &[
            Check::Absent("[data-dnacalc=find-dialog][open]"),
            Check::Value("name-box", "A1"),
            Check::Attr("selection", "data-area", "A1:C3"),
        ],
    },
    Case {
        name: "replace: Ctrl H replaces matching formulas as one workbook command",
        actions: &[
            Action::Keys("grid", "\u{E009}h\u{E000}"),
            Action::Wait(Check::Focused("find-query")),
            Action::Choose("find-scope", "selection"),
            Action::Choose("find-look", "formulas"),
            Action::Click("find-whole"),
            Action::Into("find-query", "SUM("),
            Action::Into("replace-text", "AVERAGE("),
            Action::Click("find-all"),
            Action::Wait(Check::Count("find-hit", 2)),
            Action::Click("replace-all"),
            Action::Wait(Check::Text("find-message", "No matches")),
            Action::Click("find-close"),
        ],
        checks: &[
            Check::Text(cell!("B2"), "2"),
            Check::Text(cell!("C2"), "2"),
            Check::Text(cell!("D3"), "6"),
        ],
    },
    Case {
        name: "replace: one Undo restores every replacement and Redo restores the formulas",
        actions: &[
            Action::Click("undo"),
            Action::Wait(Check::Text(cell!("B2"), "6")),
            Action::Wait(Check::Text(cell!("C2"), "6")),
            Action::Click("redo"),
        ],
        checks: &[Check::Text(cell!("B2"), "2"), Check::Text(cell!("C2"), "2")],
    },
    Case {
        name: "replace: edited formulas survive the application's saved package route",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::Wait(Check::Starts("status", "Download started: cut-base.xlsx.")),
            Action::UploadDownload("cut-base.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "cut-base.xlsx")),
            Action::Into("name-box", "B2\u{E007}"),
        ],
        checks: &[
            Check::Value("[data-onecalc=formula]", "=AVERAGE(A1:A3)"),
            Check::Text(cell!("B2"), "2"),
            Check::Text(cell!("C2"), "2"),
        ],
    },
    Case {
        name: "local menu: Shift F10 exposes named commands with keyboard focus",
        actions: &[
            Action::Into("name-box", "A1:A3\u{E007}"),
            Action::Keys("grid", "\u{E008}\u{E03A}\u{E000}"),
        ],
        checks: &[
            Check::Role("context-menu", "menu"),
            Check::Focused("context-copy"),
            Check::Role("context-clear", "menuitem"),
            Check::Reachable("context-menu"),
        ],
    },
    Case {
        name: "local menu: arrows move between enabled commands and Escape restores the grid",
        actions: &[
            Action::Wait(Check::Text("context-reason", "")),
            Action::Keys("context-copy", "\u{E015}"),
            Action::Wait(Check::Focused("context-cut")),
            Action::Keys("context-cut", ESCAPE),
        ],
        checks: &[
            Check::Absent("[data-dnacalc=context-menu][open]"),
            Check::Focused("grid"),
            Check::Attr("selection", "data-area", "A1:A3"),
        ],
    },
    Case {
        name: "local menu: a pointer menu inside a selected range keeps that range",
        actions: &[Action::ContextClick(cell!("A2"), false)],
        checks: &[
            Check::Attr("selection", "data-area", "A1:A3"),
            Check::Focused("context-copy"),
            Check::Value("name-box", "A1"),
        ],
    },
    Case {
        name: "local menu: Clear uses the existing selected-range command and one Undo",
        actions: &[
            Action::Click("context-clear"),
            Action::Wait(Check::Absent(cell!("A1"))),
            Action::Wait(Check::Absent(cell!("A2"))),
            Action::Click("undo"),
        ],
        checks: &[
            Check::Text(cell!("A1"), "1"),
            Check::Text(cell!("A2"), "2"),
            Check::Text(cell!("A3"), "3"),
        ],
    },
    Case {
        name: "local menu: right click outside the range makes the pointed cell active",
        actions: &[Action::ContextClick(cell!("B1"), false)],
        checks: &[
            Check::Value("name-box", "B1"),
            Check::TextContains("context-menu", "Data!B1"),
        ],
    },
    Case {
        name: "local menu: Shift right click preserves the browser command alternative",
        actions: &[
            Action::Keys("context-copy", ESCAPE),
            Action::ContextClick(cell!("B1"), true),
            Action::Keys("grid", ESCAPE),
        ],
        checks: &[
            Check::Absent("[data-dnacalc=context-menu][open]"),
            Check::Text(cell!("B1"), "10"),
        ],
    },
    Case {
        name: "find: the dialog and complete results remain reachable at phone width",
        actions: &[
            Action::Window(390, 844),
            Action::Click("find-tools"),
            Action::Choose("find-scope", "sheet"),
            Action::Choose("find-look", "formulas"),
            Action::Into("find-query", "AVERAGE"),
            Action::Click("find-all"),
        ],
        checks: &[
            Check::Narrow(390),
            Check::Fits,
            Check::Reachable("find-dialog"),
            Check::Count("find-hit", 2),
        ],
    },
    Case {
        name: "find: phone Escape restores the workbook",
        actions: &[
            Action::Keys("find-query", ESCAPE),
            Action::Window(1280, 1000),
        ],
        checks: &[Check::Focused("grid"), Check::Fits],
    },
];

/// The script that reads what one check looks at.
// Only the timing of actual Worker deliveries changes; the product and workbook path stay intact.
const CALC_LATENCY_CASES: &[Case] = &[
    Case {
        name: "worker latency: typing after navigation edits the destination",
        actions: &[
            Action::Wait(Check::Value("name-box", "A1")),
            Action::PauseMillis(800),
            Action::Keys("grid", "1\u{E007}"),
            Action::Wait(Check::Value("name-box", "A2")),
            Action::Keys("grid", "2\u{E007}"),
            Action::Wait(Check::Value("name-box", "A3")),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::Value("name-box", "A1")),
            Action::PauseMillis(400),
            Action::Keys("grid", "\u{E015}9\u{E007}"),
            Action::Wait(Check::Value("name-box", "A3")),
        ],
        checks: &[
            Check::Text(cell!("A1"), "1"),
            Check::Text(cell!("A2"), "9"),
            Check::Absent("cell-editor"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "worker latency: consecutive entry preserves every key and destination",
        actions: &[
            Action::Keys("grid", "78\u{E007}56\u{E007}"),
            Action::Wait(Check::Value("name-box", "A5")),
        ],
        checks: &[
            Check::Text(cell!("A3"), "78"),
            Check::Text(cell!("A4"), "56"),
            Check::Absent("cell-editor"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "worker latency: F2 immediately after navigation reads its destination source",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::Value("name-box", "A1")),
            Action::PauseMillis(400),
            Action::Keys("grid", "\u{E015}\u{E032}"),
            Action::Wait(Check::Value("cell-editor", "9")),
        ],
        checks: &[
            Check::Value("cell-editor", "9"),
            Check::Value("name-box", "A2"),
        ],
    },
    Case {
        name: "worker latency: rapid Ctrl clicks retain every area despite earlier replies",
        actions: &[
            Action::Keys("cell-editor", ESCAPE),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Wait(Check::Value("name-box", "B1")),
            Action::PauseMillis(700),
            Action::Keys("grid", "10\u{E007}"),
            Action::Wait(Check::Value("name-box", "B2")),
            Action::Into("name-box", "C1\u{E007}"),
            Action::Wait(Check::Value("name-box", "C1")),
            Action::PauseMillis(700),
            Action::Keys("grid", "20\u{E007}"),
            Action::Wait(Check::Value("name-box", "C2")),
            Action::Into("name-box", "D1\u{E007}"),
            Action::Wait(Check::Value("name-box", "D1")),
            Action::PauseMillis(700),
            Action::Keys("grid", "30\u{E007}"),
            Action::Wait(Check::Value("name-box", "D2")),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::Value("name-box", "A1")),
            Action::PauseMillis(400),
            Action::CtrlClick(cell!("B1")),
            Action::PauseMillis(180),
            Action::CtrlClick(cell!("C1")),
            Action::PauseMillis(180),
            Action::CtrlClick(cell!("D1")),
            Action::PauseMillis(700),
        ],
        checks: &[
            Check::Count("[data-dnacalc=selection]", 4),
            Check::Attr("#dc-grid-cell-1-2", "aria-selected", "true"),
            Check::Attr("#dc-grid-cell-1-3", "aria-selected", "true"),
            Check::Attr("#dc-grid-cell-1-4", "aria-selected", "true"),
        ],
    },
    Case {
        name: "worker latency: Ctrl Enter closes its draft and one Undo restores every area",
        actions: &[
            Action::Keys("grid", "9\u{E009}\u{E007}\u{E000}"),
            Action::Wait(Check::Absent("cell-editor")),
            Action::Wait(Check::Text(cell!("D1"), "9")),
            Action::Keys("grid", "\u{E009}z\u{E000}"),
            Action::PauseMillis(700),
        ],
        checks: &[
            Check::Text(cell!("A1"), "1"),
            Check::Text(cell!("B1"), "10"),
            Check::Text(cell!("C1"), "20"),
            Check::Text(cell!("D1"), "30"),
            Check::Absent("cell-editor"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "worker latency: a click after a queued formula seed points into its original destination",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::Value("name-box", "A1")),
            Action::PauseMillis(700),
            Action::Keys("grid", "\u{E015}="),
            Action::Click(cell!("B1")),
            Action::Wait(Check::Value("cell-editor", "=B1")),
            Action::Wait(Check::Value("name-box", "A2")),
            Action::Keys("cell-editor", "\u{E007}"),
            Action::Wait(Check::Value("name-box", "A3")),
        ],
        checks: &[
            Check::Text(cell!("A1"), "1"),
            Check::Text(cell!("A2"), "10"),
            Check::Text(cell!("B1"), "10"),
            Check::Absent("cell-editor"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "worker latency: Escape cancels a header drag while selection replies are pending",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::ResizeHeader(
                "[data-dnacalc=column-header][data-column=B]",
                false,
                35,
                true,
            ),
            Action::PauseMillis(900),
        ],
        checks: &[
            Check::Style("[data-dnacalc=column-header][data-column=B]", "width:64px"),
            Check::Absent("resize-preview"),
        ],
    },
    Case {
        name: "worker latency: a pending clear protects a clean workbook from replacement",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "cut-base.xlsx")),
            Action::PauseMillis(800),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::Value("name-box", "A1")),
            Action::PauseMillis(700),
            Action::Keys("grid", DELETE),
            Action::Click("new"),
            Action::Wait(Check::Attr("replace-dialog", "open", "")),
            Action::Click("keep-working"),
            Action::PauseMillis(700),
        ],
        checks: &[
            Check::Text("document", "cut-base.xlsx (changed)"),
            Check::Absent(cell!("A1")),
            Check::Absent("[data-dnacalc=replace-dialog][open]"),
        ],
    },
    Case {
        name: "worker latency: unsupported queued text shortcuts retain the draft and diagnose retry",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::Value("name-box", "A1")),
            Action::PauseMillis(700),
            Action::Keys("grid", "\u{E015}9\u{E009}\u{E003}\u{E000}"),
            Action::Wait(Check::Value("cell-editor", "9")),
        ],
        checks: &[
            Check::Value("cell-editor", "9"),
            Check::Value("name-box", "A2"),
            Check::Starts("status", "Workbook reply delayed this editor shortcut"),
        ],
    },
    Case {
        name: "worker latency: native paste data survives pending selection replies",
        actions: &[
            Action::Keys("cell-editor", ESCAPE),
            Action::ExternalClipboard("42\t43", false),
            Action::Into("name-box", "M4\u{E007}"),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "M4"),
            Check::Text(cell!("M4"), "42"),
            Check::Text(cell!("N4"), "43"),
            Check::Absent("cell-editor"),
        ],
    },
];

const CALC_WORKER_FAULT_CASES: &[Case] = &[
    Case {
        name: "worker transport: outstanding startup requests visibly say Pending",
        actions: &[],
        checks: &[
            Check::Starts("worker-status", "Pending"),
            Check::Text("result-state", "Pending"),
        ],
    },
    Case {
        name: "worker transport: a real Worker startup error is visible and releases pending requests",
        actions: &[
            Action::Wait(Check::Starts("worker-status", "Worker failed:")),
            Action::Window(1024, 768),
            Action::PauseMillis(350),
        ],
        checks: &[
            Check::Starts("worker-status", "Worker failed:"),
            Check::Starts("status", "New workbook failed:"),
            Check::Text("result-state", "Empty"),
            Check::Reachable("worker-status"),
        ],
    },
];

fn observe(app: &App, session: &Session, check: &Check) -> Result<Value, String> {
    let q = |name: &str| {
        format!(
            "document.querySelector('{}')",
            select(app, name).replace('\'', "\\'")
        )
    };
    if let Check::Role(name, _) | Check::Name(name, _) = check {
        let element = session.element(&select(app, name))?;
        let endpoint = if matches!(check, Check::Role(..)) {
            "computedrole"
        } else {
            "computedlabel"
        };
        return webdriver(
            session.port,
            "GET",
            &format!("/session/{}/element/{element}/{endpoint}", session.id),
            None,
        );
    }
    let script = match check {
        Check::Before(first, last) => {
            let a = serde_json::to_string(&select(app, first)).unwrap();
            let b = serde_json::to_string(&select(app, last)).unwrap();
            format!(
                "var a=document.querySelector({a}),b=document.querySelector({b});return !!a&&!!b&&a.getBoundingClientRect().top<b.getBoundingClientRect().top;"
            )
        }

        Check::PinnedCell | Check::ViewRetained => {
            let pinned=matches!(check,Check::PinnedCell);
            format!(r#"var g=document.querySelector('[data-dnacalc=grid]'),key=document.querySelector('[data-dnacalc=sheet-tab][aria-selected=true]')?.dataset.sheet,h=(window.__dcViews||{{}})[key];if(!g||!h)return {{ok:false}};var e=document.querySelector(h.selector);if(!e)return {{ok:false,held:h}};var r=e.getBoundingClientRect(),b=g.getBoundingClientRect(),s={{x:r.left-b.left,y:r.top-b.top,w:r.width,h:r.height,left:g.scrollLeft,top:g.scrollTop,zoom:document.querySelector('[data-dnacalc=zoom]')?.textContent,address:document.querySelector('[data-dnacalc=name-box]')?.value}};var same=['x','y','w','h'].every(k=>Math.abs(s[k]-h[k])<2);return {{ok:same&&({}),held:h,seen:s}};"#,
                if pinned {"s.left>h.left+1&&s.top>h.top+1"} else {"Math.abs(s.left-h.left)<2&&Math.abs(s.top-h.top)<2&&s.zoom===h.zoom&&s.address===h.address"})
        }
        Check::FitsText(name) => format!("var e={};if(!e)return null;var s=getComputedStyle(e),c=document.createElement('canvas').getContext('2d');c.font=s.font;return [c.measureText(e.value||e.textContent).width,e.clientWidth-parseFloat(s.paddingLeft)-parseFloat(s.paddingRight)];",q(name)),
        Check::Role(..) | Check::Name(..) => unreachable!(),
        Check::DragExtended => "var g=document.querySelector('[data-dnacalc=grid]'),s=document.querySelector('[data-dnacalc=selection]');if(!g||!s)return false;var a=s.dataset.area, n=Number(a.split(':')[1]?.match(/[0-9]+$/)?.[0]);return a.startsWith('A1:')&&n>g.querySelectorAll('[role=row]').length&&g.scrollTop>0;".to_owned(),
        Check::PageAligned => "var g=document.querySelector('[data-dnacalc=grid]'),a=document.querySelector('[data-dnacalc=active-cell]');if(!g||!a)return false;var r=a.getBoundingClientRect(),b=g.getBoundingClientRect();return r.top>b.top+22&&r.bottom<b.bottom-20&&g.scrollTop>0;".to_owned(),
        Check::ScrollAtEnd => "var g=document.querySelector('[data-dnacalc=grid]');return !!g&&g.scrollTop>0&&Math.abs(g.scrollHeight-g.clientHeight-g.scrollTop)<3&&g.scrollHeight<=8000001;".to_owned(),
        Check::ActiveDescendant(name) => format!(
            "var e={}; var id=e && e.getAttribute('aria-activedescendant'); var child=id && document.getElementById(id); return !!child && e.contains(child);",
            q(name)
        ),
        Check::Reachable(name) => format!(
            "var e={}; if(!e) return false; var r=e.getBoundingClientRect(); return r.width>0 && r.height>0 && r.left>=0 && r.top>=0 && r.right<=innerWidth+1 && r.bottom<=innerHeight+1 && getComputedStyle(e).visibility==='visible';",
            q(name)
        ),
        Check::Bounded(name, _) | Check::Count(name,_) => format!(
            "return document.querySelectorAll('{}').length;",
            select(app, name).replace('\'', "\\'")
        ),
        Check::Contrast(name) => format!(
            "var e={}; if(!e) return null; var s=getComputedStyle(e); function lum(c) {{ var v=c.match(/[0-9.]+/g).slice(0,3).map(Number).map(x=>{{x/=255;return x<=0.04045?x/12.92:Math.pow((x+0.055)/1.055,2.4);}}); return v[0]*0.2126+v[1]*0.7152+v[2]*0.0722; }} var a=lum(s.color),b=lum(s.backgroundColor); return (Math.max(a,b)+0.05)/(Math.min(a,b)+0.05);",
            q(name)
        ),
        Check::Text(name, _) | Check::Starts(name, _) | Check::TextContains(name,_) => {
            format!("var e = {}; return e ? e.textContent : null;", q(name))
        }
        Check::Items(name, _) | Check::Contains(name, _) => format!(
            "var e = {}; return e ? Array.from(e.querySelectorAll('li')).map(function (l) {{ return l.textContent; }}) : null;",
            q(name)
        ),
        Check::Absent(name) => format!("return {} === null;", q(name)),
        Check::Value(name, _) => format!("var e = {}; return e ? e.value : null;", q(name)),
        Check::Focused(name) => format!("return {} === document.activeElement;", q(name)),
        Check::Attr(name, attribute, _) => format!(
            "var e = {}; return e ? e.getAttribute('{attribute}') : null;",
            q(name)
        ),
        Check::Computed(name, declarations) => format!(
            "var e = {}; if (!e) {{ return null; }} var probe = document.createElement('div'); document.body.appendChild(probe); var out = []; '{declarations}'.split(';').forEach(function (d) {{ var i = d.indexOf(':'); if (i < 0) {{ return; }} var k = d.slice(0, i).trim(); probe.style.setProperty(k, d.slice(i + 1).trim()); out.push([k, getComputedStyle(e).getPropertyValue(k), getComputedStyle(probe).getPropertyValue(k)]); }}); probe.remove(); return out;",
            q(name)
        ),
        Check::Fits => {
            "return [document.documentElement.scrollWidth, window.innerWidth];".to_owned()
        }
        Check::Narrow(_) => "return window.innerWidth;".to_owned(),
        Check::Style(name, declarations) => format!(
            "var e = {}; if (!e) {{ return null; }} var probe = document.createElement('div'); var out = []; '{declarations}'.split(';').forEach(function (d) {{ var i = d.indexOf(':'); if (i < 0) {{ return; }} var k = d.slice(0, i).trim(); probe.style.setProperty(k, d.slice(i + 1).trim()); out.push([k, e.style.getPropertyValue(k), probe.style.getPropertyValue(k)]); }}); return out;",
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
        Check::PinnedCell | Check::ViewRetained => {
            seen.get("ok").and_then(Value::as_bool) == Some(true)
        }
        Check::FitsText(_) => seen.as_array().is_some_and(|v| {
            v[0].as_f64()
                .zip(v[1].as_f64())
                .is_some_and(|(need, have)| need <= have + 1.0)
        }),
        Check::Text(_, expected)
        | Check::Value(_, expected)
        | Check::Attr(_, _, expected)
        | Check::Role(_, expected)
        | Check::Name(_, expected) => seen.as_str() == Some(expected),
        Check::Starts(_, prefix) => seen.as_str().is_some_and(|text| text.starts_with(prefix)),
        Check::TextContains(_, part) => seen.as_str().is_some_and(|text| text.contains(part)),
        Check::Fits => seen.as_array().is_some_and(|sizes| {
            match (
                sizes.first().and_then(Value::as_f64),
                sizes.get(1).and_then(Value::as_f64),
            ) {
                (Some(scroll), Some(window)) => window > 0.0 && scroll <= window,
                _ => false,
            }
        }),
        Check::Narrow(width) => seen
            .as_f64()
            .is_some_and(|seen| seen > 0.0 && seen <= f64::from(*width)),
        Check::Style(..) | Check::Computed(..) => seen.as_array().is_some_and(|pairs| {
            !pairs.is_empty()
                && pairs.iter().all(|pair| {
                    pair.get(1)
                        .and_then(Value::as_str)
                        .is_some_and(|seen| !seen.is_empty())
                        && pair.get(1) == pair.get(2)
                })
        }),
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
        Check::Before(_, _)
        | Check::Absent(_)
        | Check::Focused(_)
        | Check::Reachable(_)
        | Check::ActiveDescendant(_)
        | Check::DragExtended
        | Check::PageAligned
        | Check::ScrollAtEnd => seen.as_bool() == Some(true),
        Check::Contrast(_) => seen.as_f64().is_some_and(|ratio| ratio >= 4.5),
        Check::Count(_, expected) => seen.as_u64() == Some(*expected as u64),
        Check::Bounded(_, limit) => seen
            .as_u64()
            .is_some_and(|count| count > 0 && count <= *limit as u64),
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
        Check::Before(first, last) => format!("{first} precedes {last} in the rendered layout"),
        Check::Text(name, expected) => format!("{name} is {expected:?}"),
        Check::Starts(name, prefix) => format!("{name} starts with {prefix:?}"),
        Check::TextContains(name, part) => format!("{name} contains {part:?}"),
        Check::Items(name, expected) => format!("{name} lists {expected:?}"),
        Check::Contains(name, wanted) => format!("{name} has an item starting {wanted:?}"),
        Check::Absent(name) => format!("no {name}"),
        Check::Value(name, expected) => format!("the field {name} holds {expected:?}"),
        Check::Focused(name) => format!("{name} owns keyboard focus"),
        Check::Role(name, value) => format!("{name} has computed accessibility role {value}"),
        Check::Name(name, value) => format!("{name} has computed accessible name {value}"),
        Check::Reachable(name) => format!("{name} fits within the viewport"),
        Check::PinnedCell => {
            "the frozen cell keeps its rectangle while both scroll axes move".to_owned()
        }
        Check::ViewRetained => {
            "the sheet retains its selection, zoom, scroll and active-cell geometry".to_owned()
        }
        Check::FitsText(name) => format!("{name} displays its complete text"),
        Check::DragExtended => {
            "selection extends beyond the initial viewport after a real out-of-view drag".to_owned()
        }
        Check::PageAligned => {
            "PageDown moves the viewport and retains an interior active-cell position".to_owned()
        }
        Check::ScrollAtEnd => {
            "physical scroll reaches the last worksheet row within browser bounds".to_owned()
        }
        Check::Contrast(name) => format!("{name} has text contrast at least 4.5:1"),
        Check::ActiveDescendant(name) => format!("{name} identifies a rendered active descendant"),
        Check::Count(name, expected) => format!("{name} has exactly {expected} elements"),
        Check::Bounded(name, limit) => format!("{name} has between 1 and {limit} elements"),
        Check::Attr(name, attribute, expected) => {
            format!("{name} has {attribute}={expected:?}")
        }
        Check::Style(name, declarations) => format!("{name}'s style sets {declarations:?}"),
        Check::Computed(name, declarations) => format!("{name} is shown with {declarations:?}"),
        Check::Fits => "the page fits the window's width".to_owned(),
        Check::Narrow(width) => format!("the viewport is at most {width} pixels wide"),
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
        Action::RememberFieldLink(name) => {
            let selector = serde_json::to_string(&select(app, name)).unwrap();
            session.script(&format!(
                "window.__ocSavedLink=document.querySelector({selector}).value;return null;"
            ))?;
            Ok(())
        }
        Action::OpenRememberedLink => {
            let url = session.script("return window.__ocSavedLink;")?;
            let url = url.as_str().ok_or("No remembered session link")?;
            session.post("/url", json!({"url":url}))?;
            Ok(())
        }
        Action::OpenFieldLink(name) => {
            let selector = serde_json::to_string(&select(app, name)).unwrap();
            let url =
                session.script(&format!("return document.querySelector({selector}).value;"))?;
            let url = url.as_str().ok_or("The field did not contain a link")?;
            session.post("/url", json!({"url":url}))?;
            session.post("/refresh", json!({}))?;
            Ok(())
        }
        Action::RememberView(name) => {
            // Name-box navigation resolves through the worker. Capture the selected view only
            // after that reply, rather than the input's temporary, still-typed range text.
            if let Some(reason) = wait_for(
                app,
                session,
                &Check::Absent("[data-dnacalc=skin-grid][disabled]"),
                SETTLE,
            ) {
                return Err(reason);
            }
            let selector = serde_json::to_string(&select(app, name)).unwrap();
            session.script(&format!(r#"var g=document.querySelector('[data-dnacalc=grid]'),e=document.querySelector({selector}),r=e.getBoundingClientRect(),b=g.getBoundingClientRect(),key=document.querySelector('[data-dnacalc=sheet-tab][aria-selected=true]').dataset.sheet;window.__dcViews=window.__dcViews||{{}};window.__dcViews[key]={{selector:{selector},x:r.left-b.left,y:r.top-b.top,w:r.width,h:r.height,left:g.scrollLeft,top:g.scrollTop,zoom:document.querySelector('[data-dnacalc=zoom]').textContent,address:document.querySelector('[data-dnacalc=name-box]').value}};return null;"#))?;
            Ok(())
        }
        Action::ScrollGrid(dx, dy) => {
            let before=session.script("var g=document.querySelector('[data-dnacalc=grid]'),r=g.getBoundingClientRect();return [g.scrollLeft,g.scrollTop,Math.round(r.left+r.width/2),Math.round(r.top+r.height/2)];")?;
            session.post("/actions",json!({"actions":[{"type":"wheel","id":"wheel","actions":[{"type":"scroll","x":before[2],"y":before[3],"deltaX":dx,"deltaY":dy,"duration":100}]}]}))?;
            let start = Instant::now();
            let mut stable = Instant::now();
            let mut last = Value::Null;
            while start.elapsed() < SETTLE {
                let after=session.script("var g=document.querySelector('[data-dnacalc=grid]'),r=g.querySelector('[data-dnacalc=active-cell]')?.getBoundingClientRect();return [g.scrollLeft,g.scrollTop,r?.left,r?.top];")?;
                if last != after {
                    stable = Instant::now();
                    last = after.clone();
                }
                if (*dx == 0 || after[0] != before[0])
                    && (*dy == 0 || after[1] != before[1])
                    && stable.elapsed() >= Duration::from_millis(100)
                {
                    return Ok(());
                }
                thread::sleep(Duration::from_millis(20));
            }
            Err("The wheel gesture did not settle after scrolling the worksheet".to_owned())
        }
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
        Action::ExternalClipboard(text, html) => {
            // Finish the preceding app command before focusing this external source editor:
            // a queued Go To reply would otherwise legitimately restore grid focus during Copy.
            if let Some(reason) = wait_for(
                app,
                session,
                &Check::Absent("[data-dnacalc=skin-grid][disabled]"),
                SETTLE,
            ) {
                return Err(reason);
            }
            let source_text = serde_json::to_string(text).unwrap();
            let expected=session.script(&format!("var e=document.createElement('{}');e.id='dc-external-clipboard';{} document.body.appendChild(e);e.focus();{} return {};",if *html {"div"} else {"textarea"},if *html {format!("e.contentEditable='true';e.innerHTML={source_text};")} else {format!("e.value={source_text};")},if *html {"var r=document.createRange();r.selectNodeContents(e);var s=getSelection();s.removeAllRanges();s.addRange(r);"} else {"e.select();"},if *html {"e.innerText"} else {"e.value"}))?;
            session.keys("#dc-external-clipboard", "\u{E009}c\u{E000}")?;
            // WebDriver can return before the OS copy has finished. Read it back through a real
            // native paste into another editor, rather than racing the app against an old clipboard.
            session.script("var p=document.createElement('textarea');p.id='dc-external-clipboard-probe';document.body.appendChild(p);window.__dcExternalPaste=function(e){if(document.activeElement===p){window.__dcExternalSeen={text:e.clipboardData.getData('text/plain'),html:e.clipboardData.getData('text/html')};e.preventDefault();}};document.addEventListener('paste',window.__dcExternalPaste,true);return null;")?;
            let started = Instant::now();
            let mut settled = false;
            while started.elapsed() < SETTLE {
                session.keys("#dc-external-clipboard-probe", "\u{E009}v\u{E000}")?;
                let seen = session.script("return window.__dcExternalSeen || null;")?;
                let plain = seen.get("text").and_then(Value::as_str).unwrap_or("");
                let markup = seen.get("html").and_then(Value::as_str).unwrap_or("");
                let normalize = |t: &str| t.replace("\r\n", "\n").replace('\r', "\n");
                let flat = |t: &str| t.chars().filter(|c| !c.is_whitespace()).collect::<String>();
                settled = if *html {
                    flat(plain) == flat(expected.as_str().unwrap_or(""))
                        && markup.to_ascii_lowercase().contains("<table")
                        && markup.contains("data-dnacalc-session")
                            == text.contains("data-dnacalc-session")
                } else {
                    normalize(plain) == normalize(text) && markup.is_empty()
                };
                if settled {
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
            session.script("document.removeEventListener('paste',window.__dcExternalPaste,true);delete window.__dcExternalPaste;delete window.__dcExternalSeen;document.querySelector('#dc-external-clipboard-probe').remove();document.querySelector('#dc-external-clipboard').remove();getSelection().removeAllRanges();document.querySelector('[data-dnacalc=grid]').focus();return null;")?;
            if settled {
                Ok(())
            } else {
                Err("The system clipboard did not contain the external test source".to_owned())
            }
        }
        Action::Into(name, text) => {
            let css = select(app, name);
            let element = session.element(&css)?;
            session.post(&format!("/element/{element}/click"), json!({}))?;
            session.keys(&css, "\u{E009}a\u{E000}")?;
            session.keys(&css, &format!("\u{E003}{text}"))
        }
        Action::PauseMillis(ms) => {
            thread::sleep(Duration::from_millis(*ms));
            Ok(())
        }
        Action::ClearDownloads => {
            for entry in fs::read_dir(&session.downloads).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                if entry.file_type().map_err(|e| e.to_string())?.is_file() {
                    fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        }
        Action::ManualDownloadFixture | Action::ProtectedDownloadFixture => {
            #[cfg(target_arch = "wasm32")]
            {
                Err("Package fixture preparation is native-only".to_owned())
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                let path = session.downloads.join("Book1.xlsx");
                let started = Instant::now();
                while !path.is_file() && started.elapsed() < SETTLE {
                    thread::sleep(Duration::from_millis(50));
                }
                let bytes = fs::read(&path).map_err(|e| e.to_string())?;
                let parts = crate::oracle_cases::package_parts(&bytes)?;
                if matches!(action, Action::ProtectedDownloadFixture) {
                    let name = "xl/worksheets/sheet1.xml";
                    let xml = std::str::from_utf8(parts.get(name).ok_or("no worksheet part")?)
                        .map_err(|e| e.to_string())?;
                    let xml = xml.replacen(
                        "</sheetData>",
                        "</sheetData><sheetProtection sheet=\"1\"/>",
                        1,
                    );
                    let bytes = crate::oracle_cases::replace_package_parts(
                        &bytes,
                        &std::collections::BTreeMap::from([(name.to_owned(), xml.into_bytes())]),
                    )?;
                    return fs::write(session.downloads.join("protected.xlsx"), bytes)
                        .map_err(|e| e.to_string());
                }
                let xml =
                    std::str::from_utf8(parts.get("xl/workbook.xml").ok_or("no workbook part")?)
                        .map_err(|e| e.to_string())?;
                if xml.contains("calcMode=") || !xml.contains("<calcPr ") {
                    return Err(
                        "manual smoke fixture expects the blank package's original calcPr"
                            .to_owned(),
                    );
                }
                let manual = xml.replacen("<calcPr ", "<calcPr calcMode=\"manual\" ", 1);
                let bytes = crate::oracle_cases::replace_package_parts(
                    &bytes,
                    &std::collections::BTreeMap::from([(
                        "xl/workbook.xml".to_owned(),
                        manual.into_bytes(),
                    )]),
                )?;
                fs::write(session.downloads.join("manual-cache.xlsx"), bytes)
                    .map_err(|e| e.to_string())
            }
        }
        Action::UploadDownload(name) => {
            let path = session.downloads.join(name);
            let started = Instant::now();
            while !path.is_file() && started.elapsed() < SETTLE {
                thread::sleep(Duration::from_millis(50));
            }
            upload(
                app,
                session,
                if app.key == "onecalc" {
                    "fixture-file"
                } else {
                    "open-file"
                },
                &path,
            )
        }
        Action::DiscardReplacement => {
            let started = Instant::now();
            while started.elapsed() < Duration::from_secs(2) {
                if session
                    .script(
                        "return !!document.querySelector('[data-dnacalc=replace-dialog][open]');",
                    )?
                    .as_bool()
                    == Some(true)
                {
                    let element = session.element(&select(app, "discard-work"))?;
                    session.post(&format!("/element/{element}/click"), json!({}))?;
                    return Ok(());
                }
                thread::sleep(Duration::from_millis(50));
            }
            Ok(())
        }
        Action::Click(name) => {
            let element = session.element(&select(app, name))?;
            session.post(&format!("/element/{element}/click"), json!({}))?;
            Ok(())
        }
        Action::CtrlClick(name) => {
            let element = session.element(&select(app, name))?;
            session.post("/actions",json!({"actions":[
                {"type":"key","id":"modifiers","actions":[{"type":"keyDown","value":"\u{E009}"},{"type":"pause"},{"type":"pause"},{"type":"keyUp","value":"\u{E009}"}]},
                {"type":"pointer","id":"mouse","parameters":{"pointerType":"mouse"},"actions":[{"type":"pointerMove","duration":0,"origin":{"element-6066-11e4-a52e-4f735466cecf":element},"x":0,"y":0},{"type":"pointerDown","button":0},{"type":"pointerUp","button":0},{"type":"pause"}]}
            ]}))?;
            Ok(())
        }
        Action::DragOutside(name) => {
            let from = session.element(&select(app, name))?;
            let coordinates=session.script("var r=document.querySelector('[data-dnacalc=grid]').getBoundingClientRect();return [Math.round(r.left+90),Math.min(innerHeight-2,Math.round(r.bottom+24))];")?;
            let x = coordinates[0].as_i64().ok_or("drag x")?;
            let y = coordinates[1].as_i64().ok_or("drag y")?;
            session.post("/actions",json!({"actions":[{"type":"pointer","id":"mouse","parameters":{"pointerType":"mouse"},"actions":[
                {"type":"pointerMove","duration":0,"origin":{"element-6066-11e4-a52e-4f735466cecf":from},"x":0,"y":0},{"type":"pointerDown","button":0},
                {"type":"pointerMove","duration":200,"origin":"viewport","x":x,"y":y},{"type":"pause","duration":700},{"type":"pointerUp","button":0}
            ]}]}))?;
            Ok(())
        }
        Action::DoubleClickEdge(name, rows) => {
            let selector = serde_json::to_string(&select(app, name)).unwrap();
            let xy = session.script(&format!(
                "var r=document.querySelector({selector}).getBoundingClientRect();return [{},{}];",
                if *rows {
                    "Math.round(r.left+r.width/2)"
                } else {
                    "Math.round(r.right-1)"
                },
                if *rows {
                    "Math.round(r.bottom-1)"
                } else {
                    "Math.round(r.top+r.height/2)"
                }
            ))?;
            session.post("/actions",json!({"actions":[{"type":"pointer","id":"mouse","parameters":{"pointerType":"mouse"},"actions":[
                {"type":"pointerMove","duration":0,"origin":"viewport","x":xy[0],"y":xy[1]},
                {"type":"pointerDown","button":0},{"type":"pointerUp","button":0},
                {"type":"pointerDown","button":0},{"type":"pointerUp","button":0}
            ]}]}))?;
            Ok(())
        }
        Action::ContextClick(name, shift) => {
            let element = session.element(&select(app, name))?;
            let mut actions = vec![
                json!({"type":"pointer","id":"mouse","parameters":{"pointerType":"mouse"},"actions":[{"type":"pointerMove","duration":0,"origin":{"element-6066-11e4-a52e-4f735466cecf":element},"x":0,"y":0},{"type":"pointerDown","button":2},{"type":"pointerUp","button":2}]}),
            ];
            if *shift {
                actions.push(json!({"type":"key","id":"context-modifier","actions":[{"type":"keyDown","value":"\u{E008}"},{"type":"pause"},{"type":"pause"},{"type":"keyUp","value":"\u{E008}"}]}));
            }
            session.post("/actions", json!({"actions":actions}))?;
            Ok(())
        }
        Action::DoubleClick(name) => {
            let element = session.element(&select(app, name))?;
            session.post("/actions", json!({"actions":[{"type":"pointer","id":"mouse","parameters":{"pointerType":"mouse"},"actions":[
                {"type":"pointerMove","duration":0,"origin":{"element-6066-11e4-a52e-4f735466cecf":element},"x":0,"y":0},
                {"type":"pointerDown","button":0},{"type":"pointerUp","button":0},
                {"type":"pointerDown","button":0},{"type":"pointerUp","button":0}
            ]}]}))?;
            Ok(())
        }
        Action::Drag(from, to) => {
            let from = session.element(&select(app, from))?;
            let to = session.element(&select(app, to))?;
            session.post("/actions", json!({"actions":[{"type":"pointer","id":"mouse","parameters":{"pointerType":"mouse"},"actions":[
                {"type":"pointerMove","duration":0,"origin":{"element-6066-11e4-a52e-4f735466cecf":from},"x":0,"y":0},
                {"type":"pointerDown","button":0},
                {"type":"pointerMove","duration":200,"origin":{"element-6066-11e4-a52e-4f735466cecf":to},"x":0,"y":0},
                {"type":"pointerUp","button":0}
            ]}]}))?;
            Ok(())
        }
        Action::ResizeHeader(name, rows, delta, cancel) => {
            let selector = serde_json::to_string(&select(app, name)).unwrap();
            let xy = session.script(&format!(
                "var r=document.querySelector({selector}).getBoundingClientRect();return [{},{}];",
                if *rows {
                    "Math.round(r.left+r.width/2)"
                } else {
                    "Math.round(r.right-1)"
                },
                if *rows {
                    "Math.round(r.bottom-1)"
                } else {
                    "Math.round(r.top+r.height/2)"
                }
            ))?;
            let x = xy[0].as_i64().ok_or("resize x")?;
            let y = xy[1].as_i64().ok_or("resize y")?;
            session.post("/actions",json!({"actions":[{"type":"pointer","id":"mouse","parameters":{"pointerType":"mouse"},"actions":[
                {"type":"pointerMove","duration":0,"origin":"viewport","x":x,"y":y},{"type":"pointerDown","button":0},
                {"type":"pointerMove","duration":200,"origin":"viewport","x":x+if *rows {0} else {i64::from(*delta)},"y":y+if *rows {i64::from(*delta)} else {0}}
            ]}]}))?;
            if *cancel {
                if session
                    .script("return !!document.querySelector('[data-dnacalc=resize-preview]');")?
                    != Value::Bool(true)
                {
                    return Err("No live resize preview before Escape".to_owned());
                }
                session.keys(&select(app, "grid"), "\u{E00C}")?;
            }
            session.post("/actions",json!({"actions":[{"type":"pointer","id":"mouse","parameters":{"pointerType":"mouse"},"actions":[{"type":"pointerUp","button":0}]}]}))?;
            Ok(())
        }
        Action::DragToCell(from, row, col) => {
            let from = session.element(&select(app, from))?;
            let xy=session.script(&format!("var c=document.querySelector('[data-dnacalc=column-header][data-column=\"{col}\"]').getBoundingClientRect();var r=document.querySelector('[data-dnacalc=row-header][data-row=\"{row}\"]').getBoundingClientRect();return [Math.round(c.left+c.width/2),Math.round(r.top+r.height/2)];"))?;
            session.post("/actions",json!({"actions":[{"type":"pointer","id":"mouse","parameters":{"pointerType":"mouse"},"actions":[{"type":"pointerMove","duration":0,"origin":{"element-6066-11e4-a52e-4f735466cecf":from},"x":0,"y":0},{"type":"pointerDown","button":0},{"type":"pointerMove","duration":200,"origin":"viewport","x":xy[0],"y":xy[1]},{"type":"pointerUp","button":0}]}]}))?;
            Ok(())
        }
        Action::Composing(name, text) => {
            let selector = serde_json::to_string(&select(app, name)).unwrap();
            let text = serde_json::to_string(text).unwrap();
            session.script(&format!("var e = document.querySelector({selector}); e.focus(); e.dispatchEvent(new CompositionEvent('compositionstart', {{bubbles:true}})); var s=e.selectionStart, t=e.selectionEnd; e.setRangeText({text},s,t,'end'); e.dispatchEvent(new InputEvent('input', {{bubbles:true,inputType:'insertCompositionText',data:{text},isComposing:true}})); e.dispatchEvent(new KeyboardEvent('keydown', {{key:'Enter',keyCode:229,isComposing:true,bubbles:true,cancelable:true}})); return null;"))?;
            Ok(())
        }
        Action::CompositionEnd(name) => {
            let selector = serde_json::to_string(&select(app, name)).unwrap();
            session.script(&format!("document.querySelector({selector}).dispatchEvent(new CompositionEvent('compositionend', {{bubbles:true}})); return null;"))?;
            Ok(())
        }
        Action::Wait(check) => wait_for(app, session, check, SETTLE).map_or(Ok(()), Err),
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
        Action::Window(width, height) => {
            // Chrome keeps a desktop window at least about 500 pixels wide, so its viewport is
            // set through the DevTools protocol (a phone's width, as its device toolbar does);
            // the wide size clears that. Firefox takes the window size itself.
            let wide = *width >= 1000;
            let emulated = if wide {
                session.post(
                    "/goog/cdp/execute",
                    json!({ "cmd": "Emulation.clearDeviceMetricsOverride", "params": {} }),
                )
            } else {
                session.post(
                    "/goog/cdp/execute",
                    json!({
                        "cmd": "Emulation.setDeviceMetricsOverride",
                        "params": { "width": width, "height": height, "deviceScaleFactor": 1, "mobile": true }
                    }),
                )
            };
            if session.bidi.is_some() {
                session.viewport(*width, *height)?;
            } else if emulated.is_err() || wide {
                session.post("/window/rect", json!({ "width": width, "height": height }))?;
            }
            Ok(())
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
/// Starts (or restarts) the page's freeze meter (dc-p96): a timer task that runs as often as the
/// page lets it and keeps the longest gap between two runs, the longest the page's main thread
/// was busy and could not answer input.
fn freeze_reset(session: &Session) {
    let _ = session.script(
        "window.__dcFreezeMax = 0; if (!window.__dcFreezeTick) { window.__dcFreezeLast = performance.now(); window.__dcFreezeTick = function () { var now = performance.now(); window.__dcFreezeMax = Math.max(window.__dcFreezeMax, now - window.__dcFreezeLast); window.__dcFreezeLast = now; setTimeout(window.__dcFreezeTick, 0); }; setTimeout(window.__dcFreezeTick, 0); } else { window.__dcFreezeLast = performance.now(); } return null;",
    );
}

/// The longest gap the freeze meter saw since [`freeze_reset`], after letting it run once more.
fn freeze_read(session: &Session) -> String {
    thread::sleep(Duration::from_millis(300));
    match session
        .script("return window.__dcFreezeMax || null;")
        .ok()
        .and_then(|v| v.as_f64())
    {
        Some(ms) => format!("{ms:.0} ms"),
        None => "an unknown time".to_owned(),
    }
}

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
            if let Err(e) =
                session.post("/goog/cdp/execute", json!({ "cmd": cmd, "params": params }))
            {
                return vec![format!("the profiler did not start ({cmd}): {e}")];
            }
        }
    }
    freeze_reset(session);
    if let Err(e) = upload(app, session, "open-file", &run.path) {
        return vec![format!("the workbook was not chosen: {e}")];
    }
    let opened = format!("Opened {name}: ");
    let started = Instant::now();
    loop {
        let status = text_of(app, session, "status");
        if status.starts_with(&opened) {
            println!("{label} WORKBOOK {browser} open: {status}");
            println!(
                "{label} WORKBOOK {browser} open froze the page for at most {}",
                freeze_read(session)
            );
            if profiling && let Some(path) = &run.profile {
                match session.post(
                    "/goog/cdp/execute",
                    json!({ "cmd": "Profiler.stop", "params": {} }),
                ) {
                    Ok(stopped) => match fs::write(path, stopped.to_string()) {
                        Ok(()) => {
                            println!("{label} WORKBOOK {browser} profile: {}", path.display())
                        }
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
            // The tab is clicked in the page: a long row of tabs may hide it from a pointer.
            session.script(&format!(
                "document.querySelector('{}').click(); return null;",
                tab.replace('\'', "\\'")
            ))?;
            session.clear(&select(app, "name-box"))?;
            session.keys(&select(app, "name-box"), &format!("{cell}{ENTER}"))?;
            session.keys(&select(app, "grid"), first)
        })();
        if let Err(e) = started_edit {
            failures.push(format!("the edit did not start: {e}"));
            return failures;
        }
        let before = text_of(app, session, "status");
        freeze_reset(session);
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
                println!(
                    "{label} WORKBOOK {browser} edit froze the page for at most {}",
                    freeze_read(session)
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
    (
        "canvas",
        "[data-dnacalc=\"canvas-node\"][data-center=\"true\"]",
    ),
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

/// What a smoke run drives: a bundle folder it serves itself (built first when none is named), or
/// a page already served at a URL (`--url`; `cargo xtask site smoke` serves a packed release with
/// the site's headers and passes its URL).
pub(crate) enum SmokeTarget {
    Bundle(Option<PathBuf>),
    Url(String),
}

pub(crate) fn smoke(
    ws: &Workspace,
    app: &App,
    target: SmokeTarget,
    only: Option<&str>,
    workbook: Option<&WorkbookRun>,
) -> ExitCode {
    let mut fault_page = None;
    let mut latency_page = None;
    let page = match target {
        SmokeTarget::Url(url) => {
            println!("{} driving the page at {url}", app.label);
            url
        }
        SmokeTarget::Bundle(bundle_dir) => {
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
            let headers = match SiteHeaders::load(ws) {
                Ok(headers) => Arc::new(headers),
                Err(e) => {
                    println!("{} RESULT failed: {e}", app.label);
                    return ExitCode::from(1);
                }
            };
            let port = match serve(dir.clone(), Some(headers.clone())) {
                Ok(port) => port,
                Err(e) => {
                    println!("{} RESULT failed: {e}", app.label);
                    return ExitCode::from(1);
                }
            };
            if app.key == "calc" {
                match serve_mode(dir.clone(), Some(headers.clone()), 0, true, false) {
                    Ok(port) => fault_page = Some(format!("http://127.0.0.1:{port}/index.html")),
                    Err(error) => {
                        println!("{} RESULT failed: {error}", app.label);
                        return ExitCode::from(1);
                    }
                }
            }
            if app.key == "calc" {
                match serve_mode(dir.clone(), Some(headers), 0, false, true) {
                    Ok(port) => latency_page = Some(format!("http://127.0.0.1:{port}/index.html")),
                    Err(error) => {
                        println!("{} RESULT failed: {error}", app.label);
                        return ExitCode::from(1);
                    }
                }
            }
            let page = format!("http://127.0.0.1:{port}/index.html");
            println!("{} serving {} at {page}", app.label, dir.display());
            page
        }
    };
    let mut fault_app = app.clone();
    fault_app.cases = CALC_WORKER_FAULT_CASES;
    let mut latency_app = app.clone();
    latency_app.cases = CALC_LATENCY_CASES;
    let mut all_passed = true;
    let mut summary = Vec::new();
    for browser in BROWSERS
        .iter()
        .filter(|b| only.is_none_or(|name| b.name == name))
    {
        let result = run_browser(ws, app, browser, &page, workbook)
            .and_then(|(passed, failed)| {
                if let Some(page) = fault_page.as_deref() {
                    run_browser(ws, &fault_app, browser, page, None)
                        .map(|(p, f)| (passed + p, failed + f))
                } else {
                    Ok((passed, failed))
                }
            })
            .and_then(|(passed, failed)| {
                if let Some(page) = latency_page.as_deref() {
                    run_browser(ws, &latency_app, browser, page, None)
                        .map(|(p, f)| (passed + p, failed + f))
                } else {
                    Ok((passed, failed))
                }
            });
        match result {
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
        app.cases.len()
            + if latency_page.is_some() {
                CALC_LATENCY_CASES.len()
            } else {
                0
            }
            + if fault_page.is_some() {
                CALC_WORKER_FAULT_CASES.len()
            } else {
                0
            },
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
    fn served_paths_stay_inside_the_folder() {
        assert_eq!(served_name("/").as_deref(), Some("index.html"));
        assert_eq!(
            served_name("/build.json?x=1").as_deref(),
            Some("build.json")
        );
        assert_eq!(
            served_name("/assets/0123456789abcdef/boot.js").as_deref(),
            Some("assets/0123456789abcdef/boot.js")
        );
        assert_eq!(served_name("/assets/../../secret"), None);
        assert_eq!(served_name("/assets//boot.js"), None);
        assert_eq!(served_name("/a/./b"), None);
        assert_eq!(served_name("/a%2fb"), None);
        assert_eq!(content_type("assets/x/a_bg.wasm"), "application/wasm");
        assert_eq!(content_type("LICENSE"), "text/plain; charset=utf-8");
    }

    #[test]
    fn chunked_bodies_join() {
        assert_eq!(
            unchunk(b"4\r\nWiki\r\n5;x=1\r\npedia\r\n0\r\n\r\n").unwrap(),
            b"Wikipedia"
        );
        assert!(unchunk(b"zz\r\n").is_err());
    }

    #[test]
    fn style_checks_need_every_declaration_as_the_browser_holds_it() {
        let check = Check::Style("r", "color:#010203");
        assert!(satisfied(
            &check,
            &json!([["color", "rgb(1, 2, 3)", "rgb(1, 2, 3)"]])
        ));
        assert!(!satisfied(
            &check,
            &json!([["color", "rgb(9, 9, 9)", "rgb(1, 2, 3)"]])
        ));
        assert!(!satisfied(&check, &json!([["color", "", ""]])));
        assert!(!satisfied(&check, &json!([])));
        assert!(!satisfied(&check, &Value::Null));
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
