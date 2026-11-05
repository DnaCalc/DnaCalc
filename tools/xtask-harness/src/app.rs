//! `cargo xtask app bundle <app> [--out <dir>]` and `cargo xtask app smoke <app> [--bundle <dir>]
//! [--browser chromium|firefox] [--suite core|full] [--renderer dom|canvas] [--renderer-report <dir>] [--group <name>] [--case <substring>] [--list] [--workbook <file> [--edit <sheet>!<cell>=<text>]]`: the browser
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
//! starts each independent group in a fresh page and runs its cases in order (typing with WebDriver key input, the browser's own key and input
//! events; clicking; choosing a file for a file input) and reads back what the page shows. Every
//! expected value is derived by hand from Excel's documented behaviour. After every case it reads
//! the page's fault record: the panic message the app's panic hook writes on the root element and
//! every uncaught error and unhandled rejection the page recorded. A panic in a reactive effect,
//! which surfaces in a browser only as a trap (`RuntimeError: unreachable`) while the rest of the
//! page may still answer, fails the case there. A browser whose driver is missing fails the run;
//! it is never skipped.
//!
//! With no filters, `smoke` runs the routine core (229 cases across the two apps and all six
//! Calc skins). `--suite full` runs every case; sweep uses full bundle suites, and site smoke
//! retains every ordinary case (worker-fault/latency cases require a local bundle). `--group name`
//! selects an independent scenario; `--case substring` matches case names without regard to
//! case. Both flags can repeat, and combine as group AND case. Explicit filters search the
//! full suite unless `--suite core` is supplied. Earlier cases inside a selected group run
//! as prerequisites with their assertions intact; they are printed and timed separately.
//! `--list` lists selected names without building or opening a browser. A miss selects nothing
//! and fails with exit code 2. `--bundle` reuses an already built app; timings exclude builds.
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
#[path = "app_renderer.rs"]
mod renderer;
#[path = "app_paint.rs"]
mod paint;
use serde_json::{Value, json};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub const USAGE: &str = "usage: cargo xtask app bundle onecalc|calc [--out <dir>]\n       cargo xtask app smoke onecalc|calc [--bundle <dir> | --url <page>] [--browser chromium|firefox] [--suite core|full] [--renderer dom|canvas] [--renderer-report <dir>] [--group <name>] [--case <substring>] [--list] [--workbook <file> [--edit <sheet>!<cell>=<text>] [--skins] [--profile <cpuprofile.json>]]\n       cargo xtask app paint-compare calc --out <dir> [--bundle <dir>] [--reference <dir>] [--workbook <file>] [--zooms 75,100,150]\n       cargo xtask onecalc bundle|smoke ... (the same, for onecalc)";

/// Independent page scenario. `core` is a prefix, so it has no omitted prerequisites.
pub struct CaseGroup {
    name: &'static str,
    start: usize,
    end: usize,
    core: usize,
    setup: &'static [Action],
}

/// Selection applies before building bundles or starting a browser. Explicit filters search
/// the full suite unless `--suite core` is also supplied.
#[derive(Default)]
pub(crate) struct SmokeSelection {
    suite: Option<String>,
    groups: Vec<String>,
    cases: Vec<String>,
    list: bool,
    renderer: Option<String>,
}
impl SmokeSelection {
    fn selected(&self, group: &CaseGroup, index: usize, case: &Case) -> bool {
        let core = self.suite.as_deref() == Some("core")
            || (self.suite.is_none() && self.groups.is_empty() && self.cases.is_empty());
        (!core || index < group.start + group.core)
            && (self.groups.is_empty() || self.groups.iter().any(|g| g == group.name))
            && (self.cases.is_empty()
                || self
                    .cases
                    .iter()
                    .any(|s| case.name.to_lowercase().contains(&s.to_lowercase())))
    }
    fn count(&self, app: &App) -> usize {
        app.groups
            .iter()
            .map(|g| {
                (g.start..g.end)
                    .filter(|&i| self.selected(g, i, &app.cases[i]))
                    .count()
            })
            .sum()
    }
}

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
    /// Optional separate worker package and binary, built into the same asset folder.
    pub worker: Option<(&'static str, &'static str)>,
    /// The page, relative to the workspace root.
    pub page: &'static str,
    /// Further scripts copied beside the page, relative to the workspace root (DNA Calc's
    /// calculation worker, `web/worker.js`, dc-p96).
    pub scripts: &'static [&'static str],
    /// The stylesheets, relative to the workspace root, in order: the bundle's `app.css` is
    /// Strand's token stylesheet (`dna_strand::stylesheet`) followed by these (dc-xib).
    /// Other CSS files under the app's web/css folder follow in filename order, so a skin's
    /// scoped stylesheet needs no additional registration here.
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
    pub groups: &'static [CaseGroup],
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
        worker: None,
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
        groups: ONECALC_GROUPS,
    },
    App {
        key: "calc",
        label: "DNACALC",
        package: "dna-calc",
        entry: "dna-calc-web",
        worker: Some(("dna-calc-worker", "dna-calc-worker-web")),
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
        groups: CALC_GROUPS,
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
    renderer_report: Option<PathBuf>,
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
        "paint-compare" if app.key == "calc" => paint::run(&ws, app, rest),
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
            let mut renderer_report = None;
            let mut selection = SmokeSelection::default();
            let mut args = rest.iter();
            while let Some(arg) = args.next() {
                if arg == "--list" {
                    selection.list = true;
                    continue;
                }
                if arg == "--skins" && app.key == "calc" {
                    skins = true;
                    continue;
                }
                match (arg.as_str(), args.next()) {
                    ("--renderer", Some(name))
                        if app.key == "calc" && matches!(name.as_str(), "dom" | "canvas") =>
                    {
                        selection.renderer = Some(name.clone())
                    }
                    ("--renderer-report", Some(path)) if app.key == "calc" => {
                        renderer_report = Some(PathBuf::from(path))
                    }
                    ("--suite", Some(suite)) if matches!(suite.as_str(), "core" | "full") => {
                        selection.suite = Some(suite.clone())
                    }
                    ("--group", Some(group)) => selection.groups.push(group.clone()),
                    ("--case", Some(case)) => selection.cases.push(case.clone()),
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
            if (edit.is_some() && workbook.is_none())
                || (skins && edit.is_none())
                || (renderer_report.is_some() && (workbook.is_none() || skins))
            {
                return usage(raw);
            }
            if workbook.as_deref() == Some(Path::new("@renderer-grid")) {
                let source = ws
                    .root
                    .join("crates/calc-doc/tests/fixtures/dc_5sr/features.xlsx");
                let generated = fs::read(source)
                    .map_err(|e| e.to_string())
                    .and_then(|b| calc_cases::fixtures::renderer_grid(&b));
                let path = ws.root.join("target/renderer-grid.xlsx");
                match generated.and_then(|b| fs::write(&path, b).map_err(|e| e.to_string())) {
                    Ok(()) => workbook = Some(path),
                    Err(e) => {
                        eprintln!("renderer fixture: {e}");
                        return ExitCode::from(1);
                    }
                }
            }
            let workbook = workbook.map(|path| WorkbookRun {
                path,
                edit,
                profile,
                skins,
                renderer_report,
            });
            let target = match (url, bundle_dir) {
                (Some(_), Some(_)) => return usage(raw),
                (Some(url), None) => SmokeTarget::Url(url),
                (None, dir) => SmokeTarget::Bundle(dir),
            };
            smoke_selected(
                &ws,
                app,
                target,
                only.as_deref(),
                workbook.as_ref(),
                &selection,
            )
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
    fs::create_dir_all(out).map_err(|e| format!("cannot create {}: {e}", out.display()))?;
    for (package, entry) in std::iter::once((app.package, app.entry)).chain(app.worker) {
        println!(
            "{} building {} for {WASM_TARGET} (profile {PROFILE})",
            app.label, entry
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
                package,
                "--bin",
                entry,
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
            .join(format!("{}.wasm", entry));
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
    let mut styles: Vec<_> = app.styles.iter().map(|style| ws.root.join(style)).collect();
    let style_dir = ws
        .root
        .join(app.page)
        .parent()
        .expect("application page folder")
        .join("css");
    let mut additional = Vec::new();
    for entry in
        fs::read_dir(&style_dir).map_err(|e| format!("cannot list {}: {e}", style_dir.display()))?
    {
        let path = entry
            .map_err(|e| format!("cannot list {}: {e}", style_dir.display()))?
            .path();
        if path.extension().is_some_and(|extension| extension == "css") && !styles.contains(&path) {
            additional.push(path);
        }
    }
    additional.sort();
    styles.extend(additional);
    let mut sheet = dna_strand::stylesheet();
    for style in styles {
        let text = fs::read_to_string(&style)
            .map_err(|e| format!("cannot read {}: {e}", style.display()))?;
        sheet.push_str(&text);
    }
    fs::write(out.join("app.css"), sheet).map_err(|e| format!("cannot write app.css: {e}"))?;
    let mut files = Vec::new();
    let mut names = vec![
        "index.html".to_owned(),
        "app.css".to_owned(),
        "boot.js".to_owned(),
        format!("{}.js", app.entry),
        format!("{}_bg.wasm", app.entry),
    ];
    names.extend(
        app.scripts
            .iter()
            .filter_map(|p| Path::new(p).file_name())
            .map(|n| n.to_string_lossy().into_owned()),
    );
    if let Some((_, entry)) = app.worker {
        names.extend([format!("{entry}.js"), format!("{entry}_bg.wasm")]);
    }
    for name in names {
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
    driver_pid: u32,
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
fn open(browser: &Browser, download_root: &Path) -> Result<(DriverProcess, Session), String> {
    let program = variable_file(browser.variable)
        .or_else(|| find_on_path(browser.program))
        .ok_or_else(|| {
            format!(
                "{} is not on PATH and {} names no file",
                browser.program, browser.variable
            )
        })?;
    let port = free_port()?;
    // ubs:ignore — the executable comes from the local driver setting or PATH, never workbook data.
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
    // Keep actual downloads on the workspace filesystem. A full /tmp on this
    // Linux host left empty JSON files even when Firefox reported a completed
    // download. Reading and importing the real file remains part of the smoke.
    let downloads = download_root.join(format!("{}-{port}", std::process::id()));
    fs::create_dir_all(&downloads)
        .map_err(|error| format!("cannot create smoke download folder: {error}"))?;
    let downloads = downloads.canonicalize().map_err(|e| e.to_string())?;
    // Browser preferences require an absolute native path. Windows canonical paths carry a
    // Win32 prefix which Chrome/Firefox preferences do not accept.
    let download_text = downloads.to_string_lossy();
    let downloads = PathBuf::from(download_text.strip_prefix(r"\\?\").unwrap_or(&download_text));
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
        driver_pid: driver.child.id(),
        bidi: created
            .pointer("/capabilities/webSocketUrl")
            .and_then(Value::as_str)
            .map(str::to_owned),
    };
    // Required controls may mount after a worker reply. Use the WebDriver lookup timeout;
    // assertions retain their own settling deadline and still fail on missing controls.
    session.post("/timeouts", json!({"implicit":1000}))?;
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
    /// Cell-boundary continuation in the bundled calculation worker, with an intervening Ping.
    WorkerCalculation,
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
    /// Opens a disclosure with a real summary click when it is closed.
    OpenDetails(&'static str),
    /// Clicks the element.
    Click(&'static str),
    /// Two real pointer clicks at the element's centre.
    DoubleClick(&'static str),
    ContextClick(&'static str, bool),
    OpenFieldLink(&'static str),
    RememberFieldLink(&'static str),
    OpenRememberedLink,
    /// Reload the actual application, preserving browser personal templates.
    Reload,
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
    UploadDownloadTo(&'static str, &'static str),
    /// Prepare an explicitly manual cache fixture with the existing package-part editor.
    ManualDownloadFixture,
    ProtectedDownloadFixture,
    /// Import a documented OOXML view using the existing package-part fixture editor.
    ViewDownloadFixture(&'static str),
    /// The same view-part editor applied to a named downloaded workbook.
    ViewDownloadFixtureFor(&'static str, &'static str),
    StyledFixture,
    AlignmentFixture,
    ValidationFixture(&'static str),
    /// Observe delayed external events in fault cases.
    PauseMillis(u64),
    /// Read-only snapshot of the actual plan before a presentation event.
    RememberPresentation,
    /// Synthetic FontFaceSet completion exercises the browser adapter, without fetching a font.
    FontLoaded,
    /// Load an installed font through FontFaceSet and require its real browser completion event.
    LoadLocalFont,
    /// Load a real face under the family authored on the retained font-reflow fixture cell.
    LoadAppliedFont,
    /// A real pointer drag between two rendered elements.
    Drag(&'static str, &'static str),
    DragToCell(&'static str, u32, &'static str),
    /// Drag the existing selection border, with optional Ctrl-copy or Escape cancellation.
    DragSelectionToCell(u32, &'static str, bool, bool),
    /// Drag a visible trailing header edge, optionally cancelling before release.
    ResizeHeader(&'static str, bool, i32, bool),
    SetPainter(&'static str),
    /// Synthetic IME events exercise the composition boundary; this is not a device IME test.
    Composing(&'static str, &'static str),
    CompositionEnd(&'static str),
    /// Wait for a worker-backed projection before the next gesture.
    Wait(Check),
    /// Chooses this file (relative to the workspace root) in the file input.
    Upload(&'static str, &'static str),
    /// Reuse a retained fixture with a different test-owned file identity.
    UploadAs(&'static str, &'static str, &'static str),
    /// Sets the browser window's size (WebDriver's Set Window Rect): a narrow screen (dc-xib).
    Window(u32, u32),
}

/// What a case expects the page to show. A name is a role or a selector ([`select`]).
pub enum Check {
    /// Browser visibility, including closed native disclosure contents (not viewport clipping).
    Rendered(&'static str, bool),
    /// Read an actual session-owned JSON download.
    DownloadedJson(&'static str, &'static str, &'static str),
    /// Exact semantic field from the renderer's live RenderPlan.
    PlanCell(&'static str, &'static str, &'static str),
    PlanAbsent(&'static str),
    PlanReady,
    PainterStable,
    HeaderHeightRetained,
    MinHeight(&'static str, u32),
    MaxWidth(&'static str, u32),
    CanvasInk(&'static str),
    MergedCell(&'static str, u32, u32),
    Editability(&'static str, bool),
    FontReflow(&'static str),
    ActiveSemantic(&'static str),
    /// Actual painted hash mask fits; adding one more glyph would overflow its content box.
    HashMask(&'static str),
    /// A named layout input changed, values and command count stayed fixed, and the plan rebuilt.
    PresentationChanged(&'static str),
    PinnedCell,
    /// Actual text bounds relative to its physical column, row and stored padding.
    Aligned(&'static str, &'static str, &'static str),
    /// Overflow stops at these physical column boundaries, irrespective of pixel widths.
    Spill(&'static str, &'static str, &'static str),
    Before(&'static str, &'static str),
    ViewRetained,
    FitsText(&'static str),
    TextBounds(&'static str),
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
    /// The browser hit test reaches the element's centre, including ancestor clipping.
    HitTarget(&'static str),
    /// The child fits wholly inside a clipping region.
    Contained(&'static str, &'static str),
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

/// A cell of DNA Calc's grid, by its address.
macro_rules! cell {
    ($address:literal) => {
        concat!("[data-dnacalc=\"cell\"][data-cell=\"", $address, "\"]")
    };
}

#[path = "../../../apps/dna-calc/tests/smoke/cases.rs"]
mod calc_cases;
#[path = "../../../apps/dna-onecalc/tests/smoke/cases.rs"]
mod onecalc_cases;
use calc_cases::{CALC_CASES, CALC_GROUPS, CALC_LATENCY_CASES, CALC_WORKER_FAULT_CASES};
use onecalc_cases::{ONECALC_CASES, ONECALC_GROUPS};
const BAR: &str = "[data-onecalc=\"formula\"]";

const READ_PLANS: &str = r#"var plans=Object.values(window.__dnacalcRenderPlans||{}).map(f=>{try{return JSON.parse(f());}catch(e){return null;}}).filter(s=>s&&s.active&&s.plan&&s.plan.publication&&!s.plan.interrupted);"#;

fn observe(app: &App, session: &Session, check: &Check) -> Result<Value, String> {
    if let Some(value) = renderer::core_observation(app, session, check)? {
        return Ok(value);
    }
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
    if let Check::DownloadedJson(name, pointer, _) = check {
        let bytes = fs::read(session.downloads.join(name)).map_err(|e| e.to_string())?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        return Ok(value.pointer(pointer).cloned().unwrap_or(Value::Null));
    }
    let script = match check {
        Check::MinHeight(name, pixels) => format!("var e={};return !!e&&e.getBoundingClientRect().height>={};",q(name),pixels),
        Check::Rendered(name, _) => format!("var e={};return e?e.checkVisibility():false;",q(name)),
        Check::DownloadedJson(..) => unreachable!(),
        Check::HeaderHeightRetained => "var h=document.querySelector('.dc-mast')?.getBoundingClientRect().height,b=window.__dcHeaderHeight;return typeof h==='number'&&typeof b==='number'&&Math.abs(h-b)<1;".to_owned(),
        Check::PainterStable => format!("{READ_PLANS}return JSON.stringify(window.__dcPresentation)===JSON.stringify(plans[0]);"),
        Check::HashMask(address) => {
            let address = serde_json::to_string(address).unwrap();
            format!(r#"var e=document.querySelector('[data-dnacalc=cell][data-cell='+{address}+']'),t=e?.querySelector('.dc-cell-text');if(!e||!t||!/^#+$/.test(t.textContent))return false;var s=getComputedStyle(e),capacity=e.clientWidth-parseFloat(s.paddingLeft)-parseFloat(s.paddingRight),have=t.getBoundingClientRect().width,probe=document.createElement('span');probe.className='dc-fit-measure';probe.style.setProperty('font',s.font);probe.style.setProperty('white-space','pre');probe.textContent=t.textContent+'#';document.body.appendChild(probe);var unit=t.querySelector("span span"),extra=unit?(t.textContent.length+1)*unit.getBoundingClientRect().width:probe.getBoundingClientRect().width;probe.remove();return have<=capacity+0.1&&extra>capacity+0.1;"#)
        }
        Check::ActiveSemantic(address) => {
            let address = serde_json::to_string(address).unwrap();
            format!("{READ_PLANS}var p=plans[0],a=p?.plan.semantic_rows.flatMap(r=>r.cells).filter(c=>c.active),g=document.querySelector('[data-dnacalc=grid][data-grid-active=true]');return a?.length===1&&a[0].address==={address}&&g?.getAttribute('aria-activedescendant')==='dc-grid-cell-'+p.pane+'-'+a[0].row+'-'+a[0].col;")
        }
        Check::MergedCell(address, rows, cols) => {
            let address = serde_json::to_string(address).unwrap();
            format!(r#"{READ_PLANS}var p=plans[0],cells=p?.plan.cells.filter(c=>c.address==={address}),semantic=p?.plan.semantic_rows.flatMap(r=>r.cells),s=semantic?.find(c=>c.address==={address});if(!s||!cells.length)return {{ok:false}};
            var covered=semantic.filter(c=>c.row>=s.row&&c.row<s.row+{rows}&&c.col>=s.col&&c.col<s.col+{cols}),e=document.getElementById('dc-grid-cell-'+p.pane+'-'+s.row+'-'+s.col);
            return {{ok:cells.every(c=>c.row_span==={rows}&&c.col_span==={cols}&&c.anchor.w>0&&c.anchor.h>0)&&s.row_span==={rows}&&s.col_span==={cols}&&covered.length===1&&e?.getAttribute('aria-rowspan')==='{rows}'&&e?.getAttribute('aria-colspan')==='{cols}',paint:cells,semantic:s,covered:covered.map(c=>c.address)}};"#)
        }
        Check::Editability(address, allowed) => {
            let address = serde_json::to_string(address).unwrap();
            format!(r#"{READ_PLANS}var p=plans[0],s=p?.plan.semantic_rows.flatMap(r=>r.cells).find(c=>c.address==={address}),cells=p?.plan.cells.filter(c=>c.address==={address});if(!s)return {{ok:false}};var e=document.getElementById('dc-grid-cell-'+p.pane+'-'+s.row+'-'+s.col),a=s.editability;return {{ok:a.checked&&a.can_begin==={allowed}&&({allowed}||!!a.refusal)&&cells.every(c=>JSON.stringify(c.editability)===JSON.stringify(a))&&e?.getAttribute('aria-readonly')===String(!{allowed}),semantic:s}};"#)
        }
        Check::FontReflow(address) => {
            let address = serde_json::to_string(address).unwrap();
            format!(r#"{READ_PLANS}var a=window.__dcPresentation?.plan.cells.find(c=>c.address==={address}),b=plans[0]?.plan.cells.find(c=>c.address==={address}),e=document.querySelector('[data-dnacalc=cell][data-cell='+{address}+'] .dc-cell-text'),before=window.__dcTextBounds?.[{address}],after=e?.getBoundingClientRect().width;
            return {{ok:!!a&&!!b&&a.text_measured&&b.text_measured&&a.shown.text===b.shown.text&&Math.abs(b.text_advance-a.text_advance)>1&&b.rect.w>a.rect.w&&Number.isFinite(before)&&Math.abs(after-before)>1&&Math.abs(after-b.text_advance)<1&&getComputedStyle(e).fontFamily.includes('DnacalcReflow')&&window.__dcAppliedFont?.trusted===true,before:a,after:b,dom_before:before,dom_after:after}};"#)
        }
        Check::CanvasInk(address) => {
            let address=serde_json::to_string(address).unwrap();
            format!(r#"{READ_PLANS}var p=plans[0]?.plan,c=p?.cells.find(c=>c.address==={address}),g=document.querySelector('[data-dnacalc=grid][data-grid-active=true]'),canvas=g?.querySelector('canvas');if(!c||!canvas||g.dataset.painter!=='canvas')return false;
            var r=c.anchor,d=devicePixelRatio,ctx=canvas.getContext('2d'),data=ctx.getImageData(Math.ceil((r.x+4)*d),Math.ceil((r.y+2)*d),Math.max(1,Math.floor((r.w-8)*d)),Math.max(1,Math.floor((r.h-4)*d))).data;
            var colors=new Set();for(var i=0;i<data.length;i+=4)colors.add([data[i],data[i+1],data[i+2],data[i+3]].join(','));return colors.size>4;"#)
        }

        Check::PlanCell(address, field, _) => {
            let address=serde_json::to_string(address).unwrap(); let field=serde_json::to_string(field).unwrap();
            format!("{READ_PLANS}var c=plans.flatMap(s=>s.plan.cells).find(c=>c.address==={address});if(!c)return null;var v={field}.split('.').reduce((v,k)=>v?.[k],c.shown);return typeof v==='boolean'||typeof v==='number'?String(v):v;")
        }
        Check::PlanAbsent(address) => {
            let address=serde_json::to_string(address).unwrap();
            format!("{READ_PLANS}return plans.length>0&&plans.every(s=>!s.plan.cells.some(c=>c.address==={address}));")
        }
        Check::PlanReady => format!("{READ_PLANS}return plans.length===1&&plans[0].pending===0&&!plans[0].plan.interrupted;"),
        Check::PresentationChanged(cause) => {
            let cause=serde_json::to_string(cause).unwrap();
            format!(r#"{READ_PLANS}var a=window.__dcPresentation,b=plans[0];if(!a||!b||b.pending)return {{ok:false,held:a,seen:b}};
            var x=a.plan,y=b.plan,cause={cause};var values=p=>p.cells.map(c=>[c.address,c.shown.text,c.shown.kind,c.shown.provenance,c.shown.formula,c.shown.format]);
            var input=cause==='resize'?(x.viewport.width!==y.viewport.width||x.viewport.height!==y.viewport.height):cause==='zoom'?x.scale!==y.scale:y.presentation.fonts>x.presentation.fonts;
            var same=a.commands===b.commands&&x.publication.workbook===y.publication.workbook&&x.publication.revision===y.publication.revision&&JSON.stringify(x.publication.calculation_through)===JSON.stringify(y.publication.calculation_through)&&JSON.stringify(values(x))===JSON.stringify(values(y));
            return {{ok:input&&same&&y.presentation.epoch>x.presentation.epoch,cause,input,same,held:a,seen:b}};"#)
        }
        Check::Spill(name, first, last) => {
            let first = serde_json::to_string(first).unwrap();
            let last = serde_json::to_string(last).unwrap();
            format!("var e={},a=document.querySelector('[data-dnacalc=column-header][data-column='+{first}+']'),b=document.querySelector('[data-dnacalc=column-header][data-column='+{last}+']');if(!e||!a||!b)return false;var r=e.getBoundingClientRect();return Math.abs(r.left-a.getBoundingClientRect().left)<1&&Math.abs(r.right-b.getBoundingClientRect().right)<1;",q(name))
        }
        Check::Aligned(name, horizontal, vertical) => {
            let h = serde_json::to_string(horizontal).unwrap();
            let v = serde_json::to_string(vertical).unwrap();
            format!(r#"var e={};if(!e)return false;var t=e.querySelector('.dc-cell-text'),r=e.getBoundingClientRect(),s=getComputedStyle(e),c=e.dataset.cell.match(/[A-Z]+/)[0],a=document.querySelector('[data-dnacalc=column-header][data-column='+c+']')?.getBoundingClientRect();if(!t||!a)return false;var b=t.getBoundingClientRect(),h={h},v={v},ok=true;
            if(h==='left')ok=Math.abs(b.left-(r.left+parseFloat(s.paddingLeft)))<1;
            if(h==='right')ok=Math.abs(b.right-(r.right-parseFloat(s.paddingRight)))<1;
            if(h==='center')ok=Math.abs((b.left+b.right-a.left-a.right)/2)<1;
            if(h.startsWith('across:')){{var z=document.querySelector('[data-dnacalc=column-header][data-column='+h.slice(7)+']')?.getBoundingClientRect();ok=!!z&&Math.abs((b.left+b.right-a.left-z.right)/2)<1;}}
            if(h==='spread'){{var w=document.createTreeWalker(t,NodeFilter.SHOW_TEXT),nodes=[];while(w.nextNode())if(w.currentNode.textContent.length)nodes.push(w.currentNode);if(!nodes.length)return false;var f=document.createRange(),l=document.createRange(),n=nodes[0],m=nodes[nodes.length-1];f.setStart(n,0);f.setEnd(n,1);l.setStart(m,m.length-1);l.setEnd(m,m.length);ok=Math.abs(f.getBoundingClientRect().left-r.left-parseFloat(s.paddingLeft))<1&&Math.abs(l.getBoundingClientRect().right-r.right+parseFloat(s.paddingRight))<1;}}
            var ts=getComputedStyle(t),fontProbe=document.createElement('canvas').getContext('2d');fontProbe.font=[s.fontStyle,s.fontWeight,s.fontSize,s.fontFamily].join(' ');var fm=fontProbe.measureText('Mg'),shift=ts.whiteSpace==='pre'&&ts.transform==='none'&&!t.textContent.includes('\n')?(fm.fontBoundingBoxAscent+fm.fontBoundingBoxDescent-parseFloat(s.fontSize))/2:0;
            if(v==='top')ok=ok&&Math.abs(b.top-r.top-parseFloat(s.paddingTop)-shift)<1;
            if(v==='bottom')ok=ok&&Math.abs(b.bottom-r.bottom+parseFloat(s.paddingBottom)+shift)<1;
            if(v==='center')ok=ok&&Math.abs((b.top+b.bottom-r.top-r.bottom)/2)<1;
            if(v==='justify'||v==='distributed'){{var lines=[...t.children].map(x=>x.getBoundingClientRect());ok=ok&&lines.length>1;var gap=lines.length>1?lines[1].top-lines[0].bottom:0;ok=ok&&gap>1&&lines.slice(1).every((l,i)=>Math.abs(l.top-lines[i].bottom-gap)<1);if(v==='justify')ok=ok&&Math.abs(lines[0].top-r.top)<1&&Math.abs(lines.at(-1).bottom-r.bottom)<1;else ok=ok&&lines[0].top>r.top+1&&lines.at(-1).bottom<r.bottom-1&&Math.abs(lines[0].top-r.top-(r.bottom-lines.at(-1).bottom))<1;}}
            return ok;"#,q(name))
        }
        Check::Before(first, last) => {
            let a = serde_json::to_string(&select(app, first)).unwrap();
            let b = serde_json::to_string(&select(app, last)).unwrap();
            format!(
                "var a=document.querySelector({a}),b=document.querySelector({b});return !!a&&!!b&&a.getBoundingClientRect().top<b.getBoundingClientRect().top;"
            )
        }

        Check::PinnedCell | Check::ViewRetained => {
            let pinned=matches!(check,Check::PinnedCell);
            format!(r#"var g=document.querySelector('[data-dnacalc=grid]'),key=document.querySelector('[data-dnacalc=sheet-tab][aria-selected=true]')?.dataset.sheet,h=(window.__dcViews||{{}})[key];if(!g||!h)return {{ok:false}};var e=document.querySelector(h.selector),r=e?.getBoundingClientRect()||window.__rendererCoreCheck?.({{op:'rect',selector:h.selector}}).seen;if(!r)return {{ok:false,held:h}};var b=g.getBoundingClientRect(),s={{x:r.left-b.left,y:r.top-b.top,w:r.width,h:r.height,left:g.scrollLeft,top:g.scrollTop,zoom:document.querySelector('[data-dnacalc=zoom]')?.textContent,address:document.querySelector('[data-dnacalc=name-box]')?.value}};var same=['x','y','w','h'].every(k=>Math.abs(s[k]-h[k])<2);return {{ok:same&&({}),held:h,seen:s}};"#,
                if pinned {"s.left>h.left+1&&s.top>h.top+1"} else {"Math.abs(s.left-h.left)<2&&Math.abs(s.top-h.top)<2&&s.zoom===h.zoom&&s.address===h.address"})
        }
        Check::TextBounds(name)=>format!("var e={};if(!e)return false;var t=e.querySelector('.dc-cell-text');if(!t)return false;var a=e.getBoundingClientRect(),b=t.getBoundingClientRect();return b.left>=a.left-1&&b.right<=a.right+1&&b.top>=a.top-1&&b.bottom<=a.bottom+1;",q(name)),
        Check::MaxWidth(name,_) => format!("var e={};return e?e.getBoundingClientRect().width:null;",q(name)),
        Check::FitsText(name) => format!("var e={};if(!e)return null;var s=getComputedStyle(e),c=document.createElement('canvas').getContext('2d');c.font=s.font;var t=e.querySelector('.dc-cell-text');return [t?t.getBoundingClientRect().width:c.measureText(e.value||e.textContent).width,e.clientWidth-parseFloat(s.paddingLeft)-parseFloat(s.paddingRight)];",q(name)),
        Check::Role(..) | Check::Name(..) => unreachable!(),
        Check::DragExtended => "var g=document.querySelector('[data-dnacalc=grid]'),s=document.querySelector('[data-dnacalc=selection]');if(!g||!s)return false;var a=s.dataset.area, n=Number(a.split(':')[1]?.match(/[0-9]+$/)?.[0]);return a.startsWith('A1:')&&n>g.querySelectorAll('[role=row]').length&&g.scrollTop>0;".to_owned(),
        Check::PageAligned => "var g=document.querySelector('[data-dnacalc=grid]'),a=document.querySelector('[data-dnacalc=active-cell]');if(!g||!a)return false;var r=a.getBoundingClientRect(),b=g.getBoundingClientRect();return r.top>b.top+22&&r.bottom<b.bottom-20&&g.scrollTop>0;".to_owned(),
        Check::ScrollAtEnd => "var g=document.querySelector('[data-dnacalc=grid]');return !!g&&g.scrollTop>0&&Math.abs(g.scrollHeight-g.clientHeight-g.scrollTop)<3&&g.scrollHeight<=8000001;".to_owned(),
        Check::ActiveDescendant(name) => format!(
            "var e={}; var id=e && e.getAttribute('aria-activedescendant'); var child=id && document.getElementById(id); return !!child && e.contains(child);",
            q(name)
        ),
        Check::Contained(child, parent) => format!(
            "var e={},p={};if(!e||!p)return false;var r=e.getBoundingClientRect(),b=p.getBoundingClientRect();return r.width>0&&r.height>0&&r.left>=b.left-1&&r.top>=b.top-1&&r.right<=b.right+1&&r.bottom<=b.bottom+1;",q(child),q(parent)),
        Check::Reachable(name) => format!(
            "var e={}; if(!e) return false; var r=e.getBoundingClientRect(); return r.width>0 && r.height>0 && r.left>=0 && r.top>=0 && r.right<=innerWidth+1 && r.bottom<=innerHeight+1 && getComputedStyle(e).visibility==='visible';",
            q(name)
        ),
        Check::HitTarget(name) => format!("var e={};if(!e)return false;var r=e.getBoundingClientRect(),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return !!hit&&e.contains(hit);",q(name)),
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
        Check::Value(name, _) => format!("var e = {}; return e ? (e.type===\"checkbox\" ? String(e.checked) : e.value) : null;", q(name)),
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
        Check::Rendered(_, expected) => seen.as_bool() == Some(*expected),
        Check::PinnedCell
        | Check::ViewRetained
        | Check::PresentationChanged(_)
        | Check::MergedCell(..)
        | Check::Editability(..)
        | Check::FontReflow(_) => seen.get("ok").and_then(Value::as_bool) == Some(true),
        Check::MaxWidth(_,width) => seen.as_f64().is_some_and(|n|n>0.&&n<=f64::from(*width)),
        Check::FitsText(_) => seen.as_array().is_some_and(|v| {
            v[0].as_f64()
                .zip(v[1].as_f64())
                .is_some_and(|(need, have)| need <= have + 1.0)
        }),
        Check::DownloadedJson(_, _, expected)
        | Check::Text(_, expected)
        | Check::PlanCell(_, _, expected)
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
        Check::Aligned(..)
        | Check::PlanAbsent(_)
        | Check::PlanReady
        | Check::PainterStable
        | Check::HeaderHeightRetained
        | Check::MinHeight(..)
        | Check::CanvasInk(_)
        | Check::ActiveSemantic(_)
        | Check::HashMask(_)
        | Check::Spill(..)
        | Check::TextBounds(_)
        | Check::Before(_, _)
        | Check::Absent(_)
        | Check::Focused(_)
        | Check::Contained(_, _)
        | Check::Reachable(_)
        | Check::HitTarget(_)
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
        Check::Rendered(name, expected) => format!("the browser reports {name} rendered: {expected}"),
        Check::DownloadedJson(name, pointer, expected) => format!("download {name} {pointer} is {expected:?}"),
        Check::PlanCell(address, field, expected) => {
            format!("RenderPlan {address}.{field} is {expected:?}")
        }
        Check::PlanAbsent(address) => format!("RenderPlan has no painted {address}"),
        Check::PlanReady => {
            "the committed RenderPlan is ready and the transport has no pending requests".into()
        }
        Check::PainterStable => {
            "switching painter preserves the exact committed plan and command count".into()
        }
        Check::CanvasInk(address) => format!("Canvas2D has rasterized text in {address}"),
        Check::PresentationChanged(cause) => format!(
            "{cause} rebuilds the RenderPlan without changing values, model revision or submitting a workbook command"
        ),
        Check::MergedCell(address, rows, cols) => {
            format!("{address} has one semantic identity and {rows} by {cols} merged paint spans")
        }
        Check::Editability(address, allowed) => format!(
            "{address} has committed host preflight editability {allowed} in paint and accessibility"
        ),
        Check::FontReflow(address) => {
            format!("{address}'s applied real font changes measured and painted width and overflow")
        }
        Check::HashMask(address) => format!(
            "{address}'s painted numeric overflow mask is the largest hash run that fits its actual font"
        ),
        Check::ActiveSemantic(address) => {
            format!("{address} is the one active semantic cell and accessibility descendant")
        }
        Check::Aligned(name, h, v) => {
            format!("{name} text aligns {h}/{v} in its physical source cell")
        }
        Check::Spill(name, first, last) => {
            format!("{name} text clips between columns {first} and {last}")
        }
        Check::MinHeight(name, pixels) => format!("{name} keeps at least {pixels} pixels of visible height"),
        Check::HeaderHeightRetained => "the workbook title retains the header's measured height".to_owned(),
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
        Check::HitTarget(name) => format!("the browser hit test reaches {name}"),
        Check::Contained(child, parent) => format!("{child} fits inside {parent}"),
        Check::PinnedCell => {
            "the frozen cell keeps its rectangle while both scroll axes move".to_owned()
        }
        Check::ViewRetained => {
            "the sheet retains its selection, zoom, scroll and active-cell geometry".to_owned()
        }
        Check::TextBounds(name) => format!("{name}'s rendered text fits within its cell"),
        Check::FitsText(name) => format!("{name} displays its complete text"),
        Check::MaxWidth(name,width) => format!("{name} is no wider than {width}px"),
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
        Action::WorkerCalculation => {
            let result = session.script(&calc_cases::fixtures::calculation_script())?;
            println!("{} WORKER finite calculation: {result}", app.label);
            Ok(())
        }
        Action::RememberFieldLink(name) => {
            let selector = serde_json::to_string(&select(app, name)).unwrap();
            session.script(&format!(
                "window.__ocSavedLink=document.querySelector({selector}).value;return null;"
            ))?;
            Ok(())
        }
        Action::Reload => {
            let url=session.script("return window.location.href;")?.as_str().ok_or("No page URL")?.to_owned();
            session.post("/url",json!({"url":url}))?;Ok(())
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
            let canvas_rect=renderer::cell_rect(app,session,name)?.unwrap_or(Value::Null);
            session.script(&format!(r#"var g=document.querySelector('[data-dnacalc=grid]'),e=document.querySelector({selector}),r=e?e.getBoundingClientRect():{canvas_rect},b=g.getBoundingClientRect(),key=document.querySelector('[data-dnacalc=sheet-tab][aria-selected=true]').dataset.sheet;window.__dcViews=window.__dcViews||{{}};window.__dcViews[key]={{selector:{selector},x:r.left-b.left,y:r.top-b.top,w:r.width,h:r.height,left:g.scrollLeft,top:g.scrollTop,zoom:document.querySelector('[data-dnacalc=zoom]').textContent,address:document.querySelector('[data-dnacalc=name-box]').value}};return null;"#))?;
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
        Action::OpenDetails(name) => {
            let css = select(app, name);
            let quoted = serde_json::to_string(&css).unwrap();
            if session.script(&format!(
                "return document.querySelector({quoted})?.open===true;"
            ))? != json!(true)
            {
                let element = session.element(&format!("{css} > summary"))?;
                session.post(&format!("/element/{element}/click"), json!({}))?;
            }
            Ok(())
        }
        Action::Into(name, text) => {
            let css = select(app, name);
            act_click(app, session, name)?;
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
        Action::ManualDownloadFixture
        | Action::ProtectedDownloadFixture
        | Action::ViewDownloadFixture(_)
        | Action::ViewDownloadFixtureFor(_, _)
        | Action::StyledFixture
        | Action::AlignmentFixture | Action::ValidationFixture(_) => {
            #[cfg(target_arch = "wasm32")]
            {
                Err("Package fixture preparation is native-only".to_owned())
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                let path = if matches!(action, Action::ValidationFixture(_)) {
                    ws.root
                        .join("crates/calc-host/tests/fixtures/dc_6yw/data-validation.xlsx")
                } else if matches!(action, Action::StyledFixture | Action::AlignmentFixture) {
                    ws.root
                        .join("crates/calc-doc/tests/fixtures/dc_5sr/features.xlsx")
                } else {
                    session.downloads.join(match action {Action::ViewDownloadFixtureFor(name,_)=>*name,_=>"Book1.xlsx"})
                };
                let bytes = downloaded_bytes(&path)?;
                if matches!(action, Action::AlignmentFixture) {
                    return fs::write(
                        session.downloads.join("alignment.xlsx"),
                        calc_cases::fixtures::alignment(&bytes)?,
                    )
                    .map_err(|e| e.to_string());
                }
                let parts = crate::oracle_cases::package_parts(&bytes)?;
                if let Action::ValidationFixture(kind) = action {
                    if matches!(*kind,"sort"|"sort-pointwise"|"sort-links"|"sort-periods") {return fs::write(session.downloads.join(format!("validation-{kind}.xlsx")),calc_cases::fixtures::sort_metadata(&bytes,kind)?).map_err(|e|e.to_string());}
                    let name = "xl/worksheets/sheet1.xml";
                    let xml =
                        std::str::from_utf8(parts.get(name).ok_or("no validation worksheet")?)
                            .map_err(|e| e.to_string())?;
                    if *kind != "list" {
                        let mut xml=xml.to_owned();
                        if *kind=="audit" {
                            let cell="<c r=\"A1\"><f>1</f><v>1</v></c>";
                            if let Some(start)=xml.find("<c r=\"A1\"") {
                                let end=start+xml[start..].find("</c>").ok_or("unclosed A1")?+4;
                                xml.replace_range(start..end,cell);
                            } else if let Some(row)=xml.find("<row r=\"1\"") {
                                let start=row+xml[row..].find('>').ok_or("unclosed row")?+1;
                                xml.insert_str(start,cell);
                            } else {
                                let start=xml.find("<sheetData>").ok_or("no sheet data")?+"<sheetData>".len();
                                xml.insert_str(start,&format!("<row r=\"1\">{cell}</row>"));
                            }
                        } else if *kind=="candidate" {
                            let rule=r#"<dataValidation type="custom" allowBlank="1" showErrorMessage="1" errorTitle="Positive only" error="Enter a positive result" sqref="M1:M5"><formula1>M1&gt;0</formula1></dataValidation>"#;
                            xml=xml.replace("<dataValidations count=\"8\">","<dataValidations count=\"9\">").replace("</dataValidations>",&format!("{rule}</dataValidations>"));
                        } else {return Err("Unknown validation fixture variation".into());}
                        let bytes=crate::oracle_cases::replace_package_parts(&bytes,&std::collections::BTreeMap::from([(name.to_owned(),xml.into_bytes())]))?;
                        return fs::write(session.downloads.join(format!("validation-{kind}.xlsx")),bytes).map_err(|e|e.to_string());
                    }
                    if !xml.contains("\"Red,Green,Blue\"") {
                        return Err("The retained validation list fixture changed".into());
                    }
                    let items = (1..=40)
                        .map(|n| format!("V{n:02}"))
                        .collect::<Vec<_>>()
                        .join(",");
                    let xml = xml
                        .replace("\"Red,Green,Blue\"", &format!("\"{items}\""))
                        .replace("sqref=\"A1:A5\"", "sqref=\"A1:A40\"")
                        .replace("promptTitle=\"Colour\"", "promptTitle=\"Choice\"")
                        .replace("Pick a colour", "Pick an item")
                        .replace("Choose Red, Green or Blue", "Choose an item from the list");
                    let bytes = crate::oracle_cases::replace_package_parts(
                        &bytes,
                        &std::collections::BTreeMap::from([(name.to_owned(), xml.into_bytes())]),
                    )?;
                    return fs::write(session.downloads.join("validation-list.xlsx"), bytes)
                        .map_err(|e| e.to_string());
                }
                if let Some(view)=match action {Action::ViewDownloadFixture(view)|Action::ViewDownloadFixtureFor(_,view)=>Some(view),_=>None} {
                    let name = "xl/worksheets/sheet1.xml";
                    let xml = std::str::from_utf8(parts.get(name).ok_or("no worksheet part")?)
                        .map_err(|e| e.to_string())?;
                    let xml = if let Some(start) = xml.find("<sheetViews") {
                        let end = start
                            + xml[start..].find("</sheetViews>").ok_or("unclosed views")?
                            + "</sheetViews>".len();
                        format!("{}{}{}", &xml[..start], view, &xml[end..])
                    } else {
                        xml.replacen("<sheetData", &format!("{view}<sheetData"), 1)
                    };
                    let bytes = crate::oracle_cases::replace_package_parts(
                        &bytes,
                        &std::collections::BTreeMap::from([(name.to_owned(), xml.into_bytes())]),
                    )?;
                    return fs::write(session.downloads.join("view.xlsx"), bytes)
                        .map_err(|e| e.to_string());
                }
                if matches!(action, Action::StyledFixture) {
                    let styles = "xl/styles.xml";
                    let sheet = "xl/worksheets/sheet1.xml";
                    let xml = std::str::from_utf8(parts.get(styles).ok_or("no styles")?)
                        .map_err(|e| e.to_string())?;
                    let mut xml = xml.replacen(
                        "<name val=\"Tahoma\"",
                        "<name val=\"DNA Calc Deliberately Absent Font 971p\"",
                        1,
                    );
                    // Extend the retained fixture's style table: rotation and indentation reuse its base font.
                    let count = xml
                        .split("<cellXfs count=\"")
                        .nth(1)
                        .and_then(|s| s.split('"').next())
                        .ok_or("cell formats")?
                        .parse::<usize>()
                        .map_err(|e| e.to_string())?;
                    xml=xml.replacen(&format!("<cellXfs count=\"{count}\""),&format!("<cellXfs count=\"{}\"",count+3),1).replacen("</cellXfs>","<xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyAlignment=\"1\"><alignment textRotation=\"45\"/></xf><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyAlignment=\"1\"><alignment indent=\"2\"/></xf><xf numFmtId=\"0\" fontId=\"2\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/></cellXfs>",1);
                    let sheet_xml = std::str::from_utf8(parts.get(sheet).ok_or("no worksheet")?)
                        .map_err(|e| e.to_string())?;
                    let sheet_xml=sheet_xml.replacen("</sheetData>",&format!("<row r=\"30\"><c r=\"A30\" s=\"{count}\" t=\"inlineStr\"><is><t>rotated wide text</t></is></c><c r=\"B30\" s=\"{}\" t=\"inlineStr\"><is><t>indented text</t></is></c><c r=\"D30\" s=\"{}\" t=\"inlineStr\"><is><t>unavailable font</t></is></c></row></sheetData>",count+1,count+2),1);
                    let bytes = crate::oracle_cases::replace_package_parts(
                        &bytes,
                        &std::collections::BTreeMap::from([
                            (styles.to_owned(), xml.into_bytes()),
                            (sheet.to_owned(), sheet_xml.into_bytes()),
                        ]),
                    )?;
                    return fs::write(session.downloads.join("styled.xlsx"), bytes)
                        .map_err(|e| e.to_string());
                }
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
                let start = xml
                    .find("<calcPr ")
                    .ok_or("manual smoke fixture has no calcPr")?;
                let end = start + xml[start..].find('>').ok_or("unterminated calcPr")?;
                let properties = &xml[start..end];
                let manual_properties = if properties.contains("calcMode=\"auto\"") {
                    properties.replacen("calcMode=\"auto\"", "calcMode=\"manual\"", 1)
                } else if properties.contains("calcMode=\"manual\"") {
                    properties.to_owned()
                } else if properties.contains("calcMode=") {
                    return Err(
                        "manual smoke fixture has an unsupported calculation mode".to_owned()
                    );
                } else {
                    properties.replacen("<calcPr ", "<calcPr calcMode=\"manual\" ", 1)
                };
                let manual = xml.replacen(properties, &manual_properties, 1);
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
        Action::UploadDownload(name) | Action::UploadDownloadTo(name, _) => {
            let path = session.downloads.join(name);
            downloaded_bytes(&path)?;
            upload(
                app,
                session,
                match action { Action::UploadDownloadTo(_,role)=>role,_=>if app.key=="onecalc" {"fixture-file"}else{"open-file"} },
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
        Action::Click(name) => act_click(app, session, name),
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
            session.script(&format!(r#"window.__dcResizeTrace=[];
                var selector={selector},node=document.querySelector(selector),grid=node.closest('[data-dnacalc=grid]');
                function capture(e){{var r=document.querySelector(selector)?.getBoundingClientRect(),g=grid.getBoundingClientRect(),hit=document.elementFromPoint(e.clientX,e.clientY);window.__dcResizeTrace.push({{type:e.type,x:e.clientX,y:e.clientY,header:r?.toJSON(),grid:g.toJSON(),target:e.target?.getAttribute('data-dnacalc'),hit:hit?.getAttribute('data-dnacalc'),hitColumn:hit?.getAttribute('data-column'),resizing:grid.dataset.resizing,time:performance.now(),active:document.querySelector('[data-dnacalc=name-box]')?.value,focus:document.activeElement?.getAttribute('data-dnacalc'),status:document.querySelector('[data-dnacalc=status]')?.textContent,areas:Array.from(document.querySelectorAll('[data-dnacalc=selection]')).map(e=>e.dataset.area),zoom:document.querySelector('[data-dnacalc=zoom]')?.textContent}});if(e.type==='mouseup'){{document.removeEventListener('mousedown',capture,true);document.removeEventListener('mouseup',capture,true);}}}}
                document.addEventListener('mousedown',capture,true);document.addEventListener('mouseup',capture,true);return null;"#))?;
            let xy = session.script(&format!(
                "var r=document.querySelector({selector}).getBoundingClientRect();window.__dcResizeTrace.push({{type:'sample',time:performance.now(),header:r.toJSON()}});return [{},{}];",
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
            if renderer::click_cell(app,session,name,true)? { return Ok(()); }
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
            session.script(&format!(r#"window.__dcResizeTrace=[];
                var selector={selector},node=document.querySelector(selector),grid=node.closest('[data-dnacalc=grid]');
                function capture(e){{var r=document.querySelector(selector)?.getBoundingClientRect(),g=grid.getBoundingClientRect(),hit=document.elementFromPoint(e.clientX,e.clientY);window.__dcResizeTrace.push({{type:e.type,x:e.clientX,y:e.clientY,header:r?.toJSON(),grid:g.toJSON(),target:e.target?.getAttribute('data-dnacalc'),hit:hit?.getAttribute('data-dnacalc'),hitColumn:hit?.getAttribute('data-column'),resizing:grid.dataset.resizing,time:performance.now(),active:document.querySelector('[data-dnacalc=name-box]')?.value,focus:document.activeElement?.getAttribute('data-dnacalc'),status:document.querySelector('[data-dnacalc=status]')?.textContent,areas:Array.from(document.querySelectorAll('[data-dnacalc=selection]')).map(e=>e.dataset.area),zoom:document.querySelector('[data-dnacalc=zoom]')?.textContent}});if(e.type==='mouseup'){{document.removeEventListener('mousedown',capture,true);document.removeEventListener('mouseup',capture,true);}}}}
                document.addEventListener('mousedown',capture,true);document.addEventListener('mouseup',capture,true);return null;"#))?;
            let xy = session.script(&format!(
                "var r=document.querySelector({selector}).getBoundingClientRect();window.__dcResizeTrace.push({{type:'sample',time:performance.now(),header:r.toJSON()}});return [{},{}];",
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
            println!("{} RESIZE {}: {}",app.label,name,session.script("return window.__dcResizeTrace;")?);
            Ok(())
        }
        Action::DragSelectionToCell(row, col, copy, cancel) => {
            let xy = session.script(&format!("var s=document.querySelector('[data-dnacalc=selection]').getBoundingClientRect();var c=document.querySelector('[data-dnacalc=column-header][data-column=\"{col}\"]').getBoundingClientRect();var r=document.querySelector('[data-dnacalc=row-header][data-row=\"{row}\"]').getBoundingClientRect();return [Math.round(s.left+4),Math.round(s.top+1),Math.round(c.left+c.width/2),Math.round(r.top+r.height/2)];"))?;
            let modifier = if *copy {json!({"type":"keyDown","value":"\u{E009}"})} else {json!({"type":"pause","duration":0})};
            let result = (|| -> Result<(), String> {
                session.post("/actions",json!({"actions":[
                    {"type":"key","id":"range-modifier","actions":[modifier,{"type":"pause","duration":0},{"type":"pause","duration":0}]},
                    {"type":"pointer","id":"mouse","parameters":{"pointerType":"mouse"},"actions":[
                        {"type":"pointerMove","duration":0,"origin":"viewport","x":xy[0],"y":xy[1]},
                        {"type":"pointerDown","button":0},
                        {"type":"pointerMove","duration":200,"origin":"viewport","x":xy[2],"y":xy[3]}
                    ]}
                ]}))?;
                if *cancel {
                    if let Some(reason)=wait_for(app,session,&Check::Rendered("move-preview",true),Duration::from_secs(5)) {return Err(reason);}
                    session.post("/actions",json!({"actions":[{"type":"key","id":"range-cancel","actions":[{"type":"keyDown","value":"\u{E00C}"},{"type":"keyUp","value":"\u{E00C}"}]}]}))?;
                }
                Ok(())
            })();
            let release = session.post("/actions",json!({"actions":[{"type":"pointer","id":"mouse","parameters":{"pointerType":"mouse"},"actions":[{"type":"pointerUp","button":0}]}]}));
            if *copy {session.post("/actions",json!({"actions":[{"type":"key","id":"range-modifier","actions":[{"type":"keyUp","value":"\u{E009}"}]}]}))?;}
            result?;
            release?;
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
        Action::SetPainter(mode) => {
            act(ws, app, session, &Action::OpenDetails("pane-tools"))?;
            let css = select(app, "grid-painter");
            session.keys(
                &css,
                if *mode == "canvas" {
                    "c\u{E007}"
                } else {
                    "d\u{E007}"
                },
            )?;
            if let Some(reason) = wait_for(
                app,
                session,
                &Check::Attr("grid", "data-painter", mode),
                SETTLE,
            ) {
                return Err(reason);
            }
            session.keys(&select(app, "grid"), ESCAPE)
        }
        Action::RememberPresentation => {
            if let Some(reason) = wait_for(app, session, &Check::PlanReady, SETTLE) {
                return Err(reason);
            }
            let started=Instant::now();
            loop {
                let current=session.script(&format!("{READ_PLANS}var p=plans[0],g=document.querySelector('[data-dnacalc=grid][data-grid-active=true]');return !!p&&!p.pending&&!!g&&p.plan.viewport.width===g.clientWidth&&p.plan.viewport.height===g.clientHeight;"))?;
                if current.as_bool()==Some(true) {break;}
                if started.elapsed()>SETTLE {return Err("The committed viewport does not match the actual active grid".into());}
                thread::sleep(Duration::from_millis(100));
            }
            session.script(&format!(
                "{READ_PLANS}window.__dcPresentation=plans[0];window.__dcHeaderHeight=document.querySelector('.dc-mast')?.getBoundingClientRect().height;window.__dcTextBounds=Object.fromEntries(Array.from(document.querySelectorAll('[data-dnacalc=cell]')).map(e=>[e.dataset.cell,e.querySelector('.dc-cell-text')?.getBoundingClientRect().width]));return null;"
            ))?;
            Ok(())
        }
        Action::FontLoaded => {
            session
                .script("document.fonts.dispatchEvent(new Event('loadingdone'));return null;")?;
            Ok(())
        }
        Action::LoadLocalFont | Action::LoadAppliedFont => {
            let applied = matches!(action, Action::LoadAppliedFont);
            let script = r#"return (async function() {
                const applied = $APPLIED, family = applied ? 'DnacalcReflow' : 'DnacalcSmokeFont';
                const face = new FontFace(family, applied ? 'local("DejaVu Sans Mono"), local("Liberation Mono"), local("Courier New")' : 'local("DejaVu Sans"), local("Liberation Sans"), local("Arial"), local("Segoe UI")');
                let finish, timer;
                const completed = new Promise((resolve, reject) => {
                    timer = setTimeout(() => reject(new Error('The local font did not complete within 8 seconds')), 8000);
                    finish = event => {
                    if (event.fontfaces.includes(face)) resolve(event.isTrusted);
                }; });
                document.fonts.addEventListener('loadingdone', finish);
                document.fonts.add(face);
                try {
                    const [faces, trusted] = await Promise.all([document.fonts.load('13px '+family, 'iiii123'), completed]);
                    if (!trusted || face.status !== 'loaded' || !faces.includes(face)) throw new Error('No real FontFaceSet completion for the loaded font');
                    const result = { status: face.status, trusted, faces: faces.length };
                    if (applied) window.__dcAppliedFont = result;
                    return result;
                } finally {
                    clearTimeout(timer);
                    document.fonts.removeEventListener('loadingdone', finish);
                    if (!applied) document.fonts.delete(face);
                }
            })();"#.replace("$APPLIED", if applied {"true"}else{"false"});
            let result = session.script(&script)?;
            println!("{} FONT completion: {result}", app.label);
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
        Action::UploadAs(name, path, filename) => {
            if Path::new(filename).file_name().and_then(|n|n.to_str()) != Some(filename) {
                return Err("Use one test filename".into());
            }
            let target = session.downloads.join(filename);
            fs::copy(ws.root.join(path), &target).map_err(|e|e.to_string())?;
            upload(app, session, name, &target)
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

/// A browser may create its final download name before finishing the file.
/// Use the existing package reader to wait for complete workbook bytes.
fn downloaded_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let started = Instant::now();
    loop {
        let result = fs::read(path).map_err(|e| e.to_string()).and_then(|bytes| {
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            if matches!(ext, "xlsx" | "xlsm" | "xltx" | "xltm" | "xlsb") {
                crate::oracle_cases::package_parts(&bytes)?;
            } else if ext == "json" {
                serde_json::from_slice::<Value>(&bytes).map_err(|e| e.to_string())?;
            } else if bytes.is_empty() {
                return Err("download is empty".into());
            }
            Ok(bytes)
        });
        match result {
            Ok(bytes) => return Ok(bytes),
            Err(error) if started.elapsed() >= SETTLE => return Err(format!("download {} is incomplete: {error}", path.display())),
            Err(_) => thread::sleep(Duration::from_millis(50)),
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
            let geometry = if let Check::Reachable(name) = check {
                session.script(&format!("var e=document.querySelector('{}');return {{viewport:[innerWidth,innerHeight],scroll:[scrollX,scrollY],rect:e?.getBoundingClientRect().toJSON(),ancestors:e?Array.from((function*(n){{for(;n;n=n.parentElement)yield n;}})(e)).slice(0,8).map(n=>({{tag:n.tagName,role:n.getAttribute('data-dnacalc'),rect:n.getBoundingClientRect().toJSON(),overflow:getComputedStyle(n).overflow}})):[]}};",select(app,name))).map(|v|format!("; geometry {v}")).unwrap_or_default()
            } else if matches!(check, Check::PlanCell(..) | Check::PlanAbsent(_)) {
                session.script(&format!(r#"{READ_PLANS}return {{held:window.__dcViews,active:document.querySelector('[data-dnacalc=name-box]')?.value,status:document.querySelector('[data-dnacalc=status]')?.textContent,plans:plans.map(p=>({{pane:p.pane,pending:p.pending,viewport:p.plan.viewport,scale:p.plan.scale,range:[p.plan.cells[0]?.address,p.plan.cells.at(-1)?.address],cells:p.plan.cells.filter(c=>c.shown.text).slice(-16).map(c=>[c.address,c.shown.text,c.shown.provenance])}}))}};"#)).map(|v|format!("; rendered state {v}")).unwrap_or_default()
            } else { String::new() };
            return Some(format!("expected {}; saw {last}{geometry}", describe(check)));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

/// Runs one case: its actions, then each check until it holds or [`SETTLE`] passes, then the fault
/// record. Returns the failures.
fn run_case(ws: &Workspace, app: &App, session: &Session, case: &Case) -> Vec<String> {
    let mut failures = Vec::new();
    let mut rendered_state_recorded = false;
    for (index, action) in case.actions.iter().enumerate() {
        if let Err(e) = act(ws, app, session, action) {
            failures.push(format!("action {} failed: {e}", index + 1));
            if matches!(action, Action::Keys(..) | Action::Wait(Check::Focused(_))) {
                if let Ok(state) = session.script(r#"return {focus:document.activeElement?.getAttribute('data-dnacalc'),tag:document.activeElement?.tagName,dialogs:Array.from(document.querySelectorAll('dialog[open]')).map(e=>e.getAttribute('data-dnacalc')),grids:Array.from(document.querySelectorAll('[data-dnacalc=grid]')).map(e=>({active:e.dataset.gridActive,inert:e.inert,tabindex:e.tabIndex,rect:e.getBoundingClientRect().toJSON()})),status:document.querySelector('[data-dnacalc=status]')?.textContent};"#) {
                    failures.push(format!("keyboard page state: {state}"));
                }
            }
            return failures;
        }
    }
    for check in case.checks {
        if let Some(failure) = wait_for(app, session, check, SETTLE) {
            failures.push(failure);
            if !rendered_state_recorded && matches!(check, Check::PlanCell(..) | Check::PlanAbsent(_)) {
                rendered_state_recorded = true;
                if let Ok(state) = session.script(&format!(r#"{READ_PLANS}return {{window:[innerWidth,innerHeight],active:document.querySelector('[data-dnacalc=name-box]')?.value,status:document.querySelector('[data-dnacalc=status]')?.textContent,grids:Array.from(document.querySelectorAll('[data-dnacalc=grid]')).map(g=>({{rect:g.getBoundingClientRect().toJSON(),scroll:[g.scrollLeft,g.scrollTop]}})),plans:plans.map(p=>({{pane:p.pane,pending:p.pending,viewport:p.plan.viewport,publication:p.plan.publication,range:[p.plan.cells[0]?.address,p.plan.cells.at(-1)?.address],formulas:p.plan.cells.filter(c=>c.shown.formula).slice(0,32).map(c=>[c.address,c.shown.text,c.shown.formula,c.shown.provenance])}}))}};"#)) {
                    failures.push(format!("rendered page state: {state}"));
                }
            }
            if matches!(check, Check::PainterStable) {
                if let Ok(state) = session.script(&format!(r#"{READ_PLANS}var changes=[];function compare(a,b,path){{if(JSON.stringify(a)===JSON.stringify(b)||changes.length>=32)return;if(a&&b&&typeof a==='object'&&typeof b==='object'){{for(var k of new Set([...Object.keys(a),...Object.keys(b)]))compare(a[k],b[k],path+'.'+k);}}else changes.push({{path,before:a,after:b}});}}compare(window.__dcPresentation,plans[0],'plan');return changes;"#)) {
                    failures.push(format!("presentation differences: {state}"));
                }
            }
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
    if run.renderer_report.is_some() {
        if let Err(e) = renderer::prepare(session, browser) {
            return vec![e];
        }
    }
    if run.renderer_report.is_none() {
        freeze_reset(session);
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
            if run.renderer_report.is_none() {
                println!(
                    "{label} WORKBOOK {browser} open froze the page for at most {}",
                    freeze_read(session)
                );
            }
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
    if run.renderer_report.is_some() {
        if let Err(e) = renderer::after_open(session) {
            return vec![e];
        }
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
        if run.renderer_report.is_none() {
            freeze_reset(session);
        }
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
                if run.renderer_report.is_none() {
                    println!(
                        "{label} WORKBOOK {browser} edit froze the page for at most {}",
                        freeze_read(session)
                    );
                }
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
    if let Some(dir) = &run.renderer_report {
        return match renderer::finish(session, browser, &run.path, dir) {
            Ok(()) => faults(app, session).unwrap_or_else(|e| vec![e]),
            Err(e) => vec![e],
        };
    }
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
    if renderer::click_cell(app,session,name,false)? { return Ok(()); }
    let selector = select(app, name);
    if name.starts_with("skin-") || matches!(name, "save" | "new" | "name-manager") {
        // These buttons are disabled until earlier workbook input has answered. WebDriver
        // reports a successful click on a disabled button without dispatching it. Wait for
        // the same availability a person sees before making the ordinary pointer click.
        let quoted = serde_json::to_string(&selector).unwrap();
        let started = Instant::now();
        let mut waited = false;
        loop {
            if session.script(&format!(
                "var e=document.querySelector({quoted});return !!e&&!e.disabled;"
            ))? == Value::Bool(true)
            {
                break;
            }
            if started.elapsed() >= SETTLE {
                return Err(format!("{name} did not become enabled before the click"));
            }
            waited = true;
            thread::sleep(Duration::from_millis(20));
        }
        if waited {
            println!(
                "{} SMOKE action waited for {name} to become enabled ({:.0} ms)",
                app.label,
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
    }
    let element = session.element(&selector)?;
    match session.post(&format!("/element/{element}/click"), json!({})) {
        // A worker reply can replace a control between lookup and click. The stale-reference
        // response means no click was dispatched; reacquire once, including field clicks.
        Err(error) if error.contains("stale element reference") => {
            let current = session.element(&selector)?;
            session.post(&format!("/element/{current}/click"), json!({}))?;
        }
        other => {
            other?;
        }
    }
    Ok(())
}

/// Navigate each group to a new document, clear retained browser storage and downloads, and
/// restore the desktop viewport. Only cases inside a group may share page state.
fn start_group(app: &App, session: &Session, page: &str) -> Result<(), String> {
    session.script(
        "try { localStorage.clear(); sessionStorage.clear(); } catch (_) {} return true;",
    )?;
    session.post("/window/rect", json!({"width":1280,"height":1000}))?;
    if session.bidi.is_some() {
        session.viewport(1280, 1000)?;
    } else {
        session.post(
            "/goog/cdp/execute",
            json!({"cmd":"Emulation.clearDeviceMetricsOverride","params":{}}),
        )?;
    }
    for entry in fs::read_dir(&session.downloads).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_file() {
            fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
        }
    }
    session.post("/url", json!({ "url": page }))?;
    let started = Instant::now();
    loop {
        let ready = session.script(&format!(
            "return document.documentElement.getAttribute('{}');",
            app.ready_attribute
        ))?;
        let worker_ready = app.key != "calc"
            || app.groups[0].name == "worker-fault"
            || session
                .script("return document.documentElement.getAttribute('data-dnacalc-worker')")?
                .as_str()
                == Some("ready");
        if ready.as_str() == Some("true") && worker_ready {
            return Ok(());
        }
        if started.elapsed() > Duration::from_secs(30) {
            return Err(format!(
                "the app did not start within 30 s; faults {:?}",
                faults(app, session)?
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn run_browser(
    ws: &Workspace,
    app: &App,
    browser: &Browser,
    page: &str,
    workbook: Option<&WorkbookRun>,
    selection: &SmokeSelection,
) -> Result<(usize, usize), String> {
    if let Some(run) = workbook.filter(|r| r.renderer_report.is_some()) {
        return renderer::run_measurements(ws, app, browser, page, run);
    }
    let (_driver, session) = open(browser, &ws.root.join("target/app-smoke-downloads"))?;
    let page = renderer::page(page, selection.renderer.as_deref());
    let page = page.as_str();
    let outcome = (|| {
        let (mut passed, mut failed) = (0, 0);
        for group in app.groups {
            let selected: Vec<usize> = (group.start..group.end)
                .filter(|&i| selection.selected(group, i, &app.cases[i]))
                .collect();
            let Some(&last) = selected.last() else {
                continue;
            };
            let started = Instant::now();
            start_group(app, &session, page)?;
            for action in group.setup {
                act(ws, app, &session, action)?;
            }
            let (mut gp, mut gf, mut prerequisites) = (0, 0, 0);
            // Earlier cases are explicit prerequisites, including all their assertions. A
            // failed prerequisite fails the run even when its name did not match the filter.
            for index in group.start..=last {
                let case = &app.cases[index];
                let requested = selected.contains(&index);
                let failures = run_case(ws, app, &session, case);
                if !requested {
                    prerequisites += 1;
                }
                if failures.is_empty() {
                    if requested {
                        passed += 1;
                        gp += 1;
                    }
                    println!(
                        "{} SMOKE {} {}passed: {}",
                        app.label,
                        browser.name,
                        if requested { "" } else { "prerequisite " },
                        case.name
                    );
                } else {
                    failed += 1;
                    gf += 1;
                    println!(
                        "{} SMOKE {} {}failed: {}",
                        app.label,
                        browser.name,
                        if requested { "" } else { "prerequisite " },
                        case.name
                    );
                    for failure in failures {
                        println!("    {failure}");
                    }
                    if case.actions.iter().any(|action|matches!(action,Action::ResizeHeader(..))) {
                        if let Ok(context)=session.script(r#"return {resize:window.__dcResizeTrace,active:document.querySelector('[data-dnacalc=name-box]')?.value,status:document.querySelector('[data-dnacalc=status]')?.textContent,focus:document.activeElement?.getAttribute('data-dnacalc'),zoom:document.querySelector('[data-dnacalc=zoom]')?.textContent};"#) {
                            println!("{} RESIZE FAILURE CONTEXT: {context}",app.label);
                        }
                    }
                }
            }
            println!(
                "{} GROUP {} {}: {gp} passed, {gf} failed, {prerequisites} prerequisites; {:.3} s (page setup included)",
                app.label,
                browser.name,
                group.name,
                started.elapsed().as_secs_f64()
            );
        }
        if let Some(run) = workbook {
            let started = Instant::now();
            start_group(app, &session, page)?;
            let failures = run_workbook(app, &session, browser.name, run);
            if failures.is_empty() {
                passed += 1;
            } else {
                failed += 1;
                for failure in failures {
                    println!("    {failure}");
                }
            }
            println!(
                "{} GROUP {} workbook: {:.3} s",
                app.label,
                browser.name,
                started.elapsed().as_secs_f64()
            );
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
    smoke_selected(
        ws,
        app,
        target,
        only,
        workbook,
        &SmokeSelection {
            suite: Some("full".into()),
            ..Default::default()
        },
    )
}

fn smoke_selected(
    ws: &Workspace,
    app: &App,
    target: SmokeTarget,
    only: Option<&str>,
    workbook: Option<&WorkbookRun>,
    selection: &SmokeSelection,
) -> ExitCode {
    const FAULT_GROUPS: [CaseGroup; 1] = [CaseGroup {
        name: "worker-fault",
        start: 0,
        end: CALC_WORKER_FAULT_CASES.len(),
        core: 0,
        setup: &[],
    }];
    const LATENCY_GROUPS: [CaseGroup; 3] = [CaseGroup {
        name: "worker-latency",
        start: 0,
        end: 22,
        core: 0,
        setup: &[],
    }, CaseGroup {
        name: "array-detail-latency",
        start: 22,
        end: 24,
        core: 0,
        setup: &[],
    }, CaseGroup {
        name: "review-latency",
        start: 24,
        end: CALC_LATENCY_CASES.len(),
        core: 0,
        setup: &[],
    }];
    let mut fault_app = app.clone();
    fault_app.cases = CALC_WORKER_FAULT_CASES;
    fault_app.groups = &FAULT_GROUPS;
    let mut latency_app = app.clone();
    latency_app.cases = CALC_LATENCY_CASES;
    latency_app.groups = &LATENCY_GROUPS;
    let extra = app.key == "calc" && matches!(&target, SmokeTarget::Bundle(_));
    let count = selection.count(app)
        + if extra {
            selection.count(&fault_app) + selection.count(&latency_app)
        } else {
            0
        };
    if count == 0 && workbook.is_none() {
        eprintln!("{} SMOKE: no cases match the selection", app.label);
        return ExitCode::from(2);
    }
    if selection.list {
        for variant in std::iter::once(app)
            .chain(extra.then_some(&fault_app))
            .chain(extra.then_some(&latency_app))
        {
            for group in variant.groups {
                for index in group.start..group.end {
                    if selection.selected(group, index, &variant.cases[index]) {
                        println!("{} {}: {}", app.key, group.name, variant.cases[index].name);
                    }
                }
            }
        }
        return ExitCode::SUCCESS;
    }
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
            if app.key == "calc" && selection.count(&fault_app) > 0 {
                match serve_mode(dir.clone(), Some(headers.clone()), 0, true, false) {
                    Ok(port) => fault_page = Some(format!("http://127.0.0.1:{port}/index.html")),
                    Err(error) => {
                        println!("{} RESULT failed: {error}", app.label);
                        return ExitCode::from(1);
                    }
                }
            }
            if app.key == "calc" && selection.count(&latency_app) > 0 {
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
    let mut all_passed = true;
    let mut summary = Vec::new();
    for browser in BROWSERS
        .iter()
        .filter(|b| only.is_none_or(|name| b.name == name))
    {
        let started = Instant::now();
        let result = run_browser(ws, app, browser, &page, workbook, selection)
            .and_then(|(passed, failed)| {
                if let Some(page) = fault_page.as_deref() {
                    run_browser(ws, &fault_app, browser, page, None, selection)
                        .map(|(p, f)| (passed + p, failed + f))
                } else {
                    Ok((passed, failed))
                }
            })
            .and_then(|(passed, failed)| {
                if let Some(page) = latency_page.as_deref() {
                    run_browser(ws, &latency_app, browser, page, None, selection)
                        .map(|(p, f)| (passed + p, failed + f))
                } else {
                    Ok((passed, failed))
                }
            });
        match result {
            Ok((passed, failed)) => {
                all_passed &= failed == 0;
                summary.push(format!(
                    "{}: {passed} passed, {failed} failed, {:.3} s",
                    browser.name,
                    started.elapsed().as_secs_f64()
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
    if workbook.is_some_and(|r| r.renderer_report.is_some()) {
        println!(
            "{} RESULT {}: 6 renderer trials per browser; {}",
            app.label,
            if all_passed { "passed" } else { "failed" },
            summary.join("; ")
        );
    } else {
        println!(
            "{} RESULT {}: {} cases per browser{}; {}",
            app.label,
            if all_passed { "passed" } else { "failed" },
            count,
            if workbook.is_some() {
                " and the workbook"
            } else {
                ""
            },
            summary.join("; ")
        );
    }
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
    fn group_ranges_cover_every_case_once_and_core_has_no_missing_prerequisite() {
        for app in &APPS {
            let mut next = 0;
            let mut names = std::collections::BTreeSet::new();
            for group in app.groups {
                assert_eq!(group.start, next, "{} {}", app.key, group.name);
                assert!(group.end > group.start);
                assert!(group.core <= group.end - group.start);
                assert!(names.insert(group.name));
                next = group.end;
            }
            assert_eq!(next, app.cases.len(), "{}", app.key);
            assert_eq!(
                SmokeSelection {
                    suite: Some("full".into()),
                    ..Default::default()
                }
                .count(app),
                app.cases.len()
            );
        }
        let total: usize = APPS
            .iter()
            .map(|app| SmokeSelection::default().count(app))
            .sum();
        assert_eq!(
            total, 363,
            "the two applications have 363 core cases including eight Find and hidden-name cases and all 355 inherited core cases"
        );
    }

    #[test]
    fn filters_search_full_names_and_a_missing_case_selects_nothing() {
        let app = &APPS[0];
        let filter = SmokeSelection {
            cases: vec!["REPLACE ITS CODE".into()],
            ..Default::default()
        };
        assert_eq!(filter.count(app), 1);
        assert_eq!(
            SmokeSelection {
                suite: Some("core".into()),
                cases: filter.cases.clone(),
                ..Default::default()
            }
            .count(app),
            0
        );
        assert_eq!(
            SmokeSelection {
                groups: vec!["vba".into()],
                ..Default::default()
            }
            .count(app),
            4
        );
        assert_eq!(
            SmokeSelection {
                cases: vec!["no such case".into()],
                ..Default::default()
            }
            .count(app),
            0
        );
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
