//! `cargo xtask package onecalc-clean-run --package <DNA-OneCalc folder> --work <dir> --out <dir>
//! [--x1 <X1.xll>] [--exceldna <net48 folder>] [--x5 <X5Native64.xll>] [--xlsm <DnaExamples.xlsm>]`:
//! the proof that the packaged folder runs on a clean machine (OneCalc milestone 7, dc-0f1k.13).
//!
//! The folder is copied to an empty directory outside the repository and run from there with a
//! fresh data folder (so a fresh WebView2 user-data folder), a minimal environment (Windows' own
//! variables, a temporary folder inside the work directory, a PATH of the system folders: nothing
//! that names the repository, which is checked) and no `--assets`: the page is the one inside the
//! executable. The add-ins and the .xlsm are copied beside it. The run is driven the way the
//! other app runs are: chromedriver attached to the app's WebView2 (DevTools protocol), plus the
//! application's own window (its title, its place, WM_CLOSE) and nothing else: no mouse, keyboard
//! or focus use. The native file dialogs need a person and are not driven: saves go through the
//! app's scripted chooser (`--save-dir`), a session reopens from the recent list or the page's file
//! input, and the unsaved-changes question is answered with `--close-answer`.
//!
//! Runs: A (the five-minute path: formulas, completion, F9 on a part, trace, format picker, title and
//! dirty mark, Ctrl+S and Ctrl+Shift+S, reopen from the recent list, the X1 fixture .xll, an
//! Excel-DNA add-in, DnaExamples.xlsm with macros enabled through the bar, RTD over the numeric
//! test server, a file OneCalc refuses and Open in DNA Calc); B (a second start: the window where it
//! was left, the recent list kept, Save writing the opened file in place, close asking and saving);
//! C (close discarding); D (WebView2 not found: the message); E (DNA Calc, the companion in the folder:
//! started from the copied folder with `--open`, its page inside the executable, showing the file's
//! sheets; and the process OneCalc's "Open in DNA Calc" starts is that executable and shows a window).
//! Exit code 0 when every check passed.

use super::xll::{
    CASE_WAIT, PAGE_WAIT, Shown, attach_page, click, enter_formula, evaluate, items_of,
    set_text_input, text_of, wait_until,
};
use super::*;
use crate::package::{CALC_EXE, EXE, unzip};
use std::collections::BTreeMap;
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsWindow,
    PostMessageW, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos, WM_CLOSE,
};

const WAIT: Duration = Duration::from_secs(60);

// ---------------------------------------------------------------------------------------------
// The application's own window.

struct Found {
    pid: u32,
    window: HWND,
}

unsafe extern "system" fn visit(window: HWND, found: LPARAM) -> i32 {
    let found = unsafe { &mut *(found as *mut Found) };
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(window, &mut pid) };
    let mut class = [0u16; 32];
    let length = unsafe { GetClassNameW(window, class.as_mut_ptr(), 32) } as usize;
    if pid == found.pid && String::from_utf16_lossy(&class[..length]) == "XLMAIN" {
        found.window = window;
        return 0;
    }
    1
}

/// The outermost window (class XLMAIN) of process `pid`.
fn window_of(pid: u32) -> Option<HWND> {
    let mut found = Found {
        pid,
        window: std::ptr::null_mut(),
    };
    unsafe { EnumWindows(Some(visit), &mut found as *mut Found as LPARAM) };
    (!found.window.is_null()).then_some(found.window)
}

fn window_title(window: HWND) -> String {
    let mut text = [0u16; 512];
    let length = unsafe { GetWindowTextW(window, text.as_mut_ptr(), 512) } as usize;
    String::from_utf16_lossy(&text[..length])
}

fn window_rect(window: HWND) -> (i32, i32, i32, i32) {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe { GetWindowRect(window, &mut rect) };
    (rect.left, rect.top, rect.right, rect.bottom)
}

// ---------------------------------------------------------------------------------------------
// The run.

struct Inputs {
    package: PathBuf,
    work: PathBuf,
    out: PathBuf,
    x1: PathBuf,
    exceldna: PathBuf,
    x5: PathBuf,
    xlsm: PathBuf,
    fixtures: PathBuf,
    repo: PathBuf,
}

/// What every check recorded.
#[derive(Default)]
struct Report {
    steps: Vec<Value>,
    failed: usize,
}

impl Report {
    fn check(&mut self, run: &str, name: &str, ok: bool, detail: Value) {
        println!(
            "ONECALC CLEAN {run} {} {name}: {detail}",
            if ok { "ok" } else { "FAILED" }
        );
        if !ok {
            self.failed += 1;
        }
        self.steps
            .push(json!({"run": run, "step": name, "ok": ok, "detail": detail}));
    }
}

/// A started copy of the application with its driver.
struct Launched {
    app: DriverProcess,
    _driver: Option<DriverProcess>,
    session: Option<Session>,
    window: HWND,
    pid: u32,
}

fn minimal_environment(inputs: &Inputs) -> Result<BTreeMap<String, String>, String> {
    let mut env = BTreeMap::new();
    for name in [
        "SystemRoot",
        "windir",
        "SystemDrive",
        "ComSpec",
        "PATHEXT",
        "OS",
        "PROCESSOR_ARCHITECTURE",
        "NUMBER_OF_PROCESSORS",
        "USERNAME",
        "USERDOMAIN",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "ProgramData",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
        "CommonProgramFiles",
        "ALLUSERSPROFILE",
        "PUBLIC",
        "HOMEDRIVE",
        "HOMEPATH",
        "COMPUTERNAME",
    ] {
        if let Ok(value) = std::env::var(name) {
            env.insert(name.to_owned(), value);
        }
    }
    let root = env.get("SystemRoot").cloned().ok_or("no SystemRoot")?;
    env.insert("PATH".into(), format!(r"{root}\system32;{root}"));
    let temp = inputs.work.join("tmp");
    fs::create_dir_all(&temp).map_err(|e| e.to_string())?;
    env.insert("TEMP".into(), temp.display().to_string());
    env.insert("TMP".into(), temp.display().to_string());
    let repo = inputs.repo.display().to_string().to_ascii_lowercase();
    for (name, value) in &env {
        if value.to_ascii_lowercase().contains(&repo) {
            return Err(format!(
                "the environment variable {name} names the repository: {value}"
            ));
        }
    }
    Ok(env)
}

fn launch(
    inputs: &Inputs,
    label: &str,
    extra: &[String],
    drive: bool,
    env_extra: &[(&str, String)],
) -> Result<Launched, String> {
    let exe = inputs.work.join("app").join(EXE);
    let mut command = Command::new(&exe);
    command
        .env_clear()
        .envs(minimal_environment(inputs)?)
        .current_dir(&inputs.work);
    for (name, value) in env_extra {
        command.env(name, value);
    }
    let upstream = free_port()?;
    command.args(["--data-dir"]).arg(inputs.work.join("data"));
    if drive {
        command.args(["--debug-port", &upstream.to_string()]);
    }
    let stderr = fs::File::create(inputs.work.join(format!("stderr-{label}.txt")))
        .map_err(|e| e.to_string())?;
    command
        .args(extra)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr);
    command.env("DNACALC_ONECALC_TRACE", "1");
    let child = command
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", exe.display()))?;
    let pid = child.id();
    let app = DriverProcess {
        child,
        port: upstream,
    };
    let started = Instant::now();
    let mut window: HWND = std::ptr::null_mut();
    while started.elapsed() < WAIT && window.is_null() {
        window = window_of(pid).unwrap_or(std::ptr::null_mut());
        thread::sleep(Duration::from_millis(100));
    }
    if !drive {
        return Ok(Launched {
            app,
            _driver: None,
            session: None,
            window,
            pid,
        });
    }
    while TcpStream::connect(("127.0.0.1", upstream)).is_err() {
        if started.elapsed() > Duration::from_secs(90) {
            return Err(format!(
                "{label}: the application did not open its DevTools port"
            ));
        }
        thread::sleep(Duration::from_millis(200));
    }
    let (driver, session) = attach_page(upstream, &inputs.out.join(format!("driver-{label}")))?;
    if !wait_until(WAIT, || !text_of(&session, "addin-count").is_empty()) {
        return Err(format!("{label}: the page did not mount"));
    }
    Ok(Launched {
        app,
        _driver: Some(driver),
        session: Some(session),
        window,
        pid,
    })
}

impl Launched {
    fn session(&self) -> &Session {
        self.session.as_ref().expect("a driven run")
    }

    /// Asks the window to close (WM_CLOSE, as its close button does) and waits for the process.
    fn close(&mut self) -> Option<i32> {
        unsafe { PostMessageW(self.window, WM_CLOSE, 0, 0) };
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(20) {
            if let Ok(Some(status)) = self.app.child.try_wait() {
                return status.code();
            }
            thread::sleep(Duration::from_millis(100));
        }
        None
    }

    fn running(&mut self) -> bool {
        matches!(self.app.child.try_wait(), Ok(None))
    }
}

fn script_text(session: &Session, script: &str) -> String {
    match session.script(script) {
        Ok(Value::String(text)) => text,
        Ok(other) => other.to_string(),
        Err(e) => format!("error: {e}"),
    }
}

fn page_title(session: &Session) -> String {
    script_text(session, "return document.title;")
}

fn open_details(session: &Session, role: &str) {
    let _ = session.script(&format!(
        "var d=document.querySelector('[data-onecalc=\"{role}\"]'); if(d&&!d.open){{d.querySelector('summary').click();}} return d?d.open:null;"
    ));
}

fn set_select(session: &Session, role: &str, value: &str) -> Result<(), String> {
    let quoted = serde_json::to_string(value).map_err(|e| e.to_string())?;
    session
        .script(&format!(
            "var e=document.querySelector('[data-onecalc=\"{role}\"]'); e.value={quoted}; e.dispatchEvent(new Event('change',{{bubbles:true}})); return e.value;"
        ))
        .map(|_| ())
}

fn set_grid(session: &Session, cell: &str, text: &str) -> Result<(), String> {
    let quoted = serde_json::to_string(text).map_err(|e| e.to_string())?;
    session
        .script(&format!(
            "var e=document.querySelector('[data-onecalc=\"grid-cell\"][data-cell=\"{cell}\"]'); e.value={quoted}; e.dispatchEvent(new Event('input',{{bubbles:true}})); return true;"
        ))
        .map(|_| ())
}

/// A key chord through the DevTools protocol (the page sees a real keydown with the modifiers).
fn chord(
    session: &Session,
    key: &str,
    code: &str,
    virtual_key: u32,
    modifiers: u32,
) -> Result<(), String> {
    for kind in ["keyDown", "keyUp"] {
        session.post(
            "/goog/cdp/execute",
            json!({"cmd": "Input.dispatchKeyEvent", "params": {
                "type": kind, "key": key, "code": code, "windowsVirtualKeyCode": virtual_key, "modifiers": modifiers
            }}),
        )?;
    }
    Ok(())
}

fn wait_title(launched: &mut Launched, wanted: &str) -> bool {
    let window = launched.window;
    wait_until(Duration::from_secs(15), || window_title(window) == wanted)
}

/// The formula in the first sheet of a saved session.
fn saved_formula(path: &Path) -> Result<String, String> {
    let zip = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let entries = unzip(&zip)?;
    let sheet = entries
        .iter()
        .find(|(name, _)| name == "xl/worksheets/sheet1.xml")
        .ok_or("the saved file has no sheet1.xml")?;
    let xml = String::from_utf8_lossy(&sheet.1).into_owned();
    let start = xml.find("<f").ok_or("the sheet holds no formula")?;
    let from = xml[start..].find('>').ok_or("bad formula element")? + start + 1;
    let to = xml[from..].find("</f>").ok_or("bad formula element")? + from;
    Ok(xml[from..to]
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\""))
}

fn saved_path(session: &Session) -> Option<PathBuf> {
    text_of(session, "file-status")
        .strip_prefix("Saved ")
        .map(|p| PathBuf::from(p.trim_end_matches('.')))
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for entry in fs::read_dir(from)
        .map_err(|e| format!("{}: {e}", from.display()))?
        .flatten()
    {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target).map_err(|e| format!("{}: {e}", target.display()))?;
        }
    }
    Ok(())
}

fn registry_has(progid: &str) -> bool {
    Command::new("reg")
        .args(["query", &format!(r"HKCU\Software\Classes\{progid}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Evaluates `formula` and checks the page's answer against `expected` (a recorded Excel value).
fn expect(report: &mut Report, run: &str, session: &Session, formula: &str, expected: &Value) {
    let answer = evaluate(session, formula);
    let wanted = Shown::of_json(expected);
    let (ok, detail) = match (&answer, &wanted) {
        (Ok((text, _, Some(shown))), Some(wanted)) => (shown.same(wanted), json!(text)),
        (Ok((text, diagnostics, _)), _) => {
            (false, json!({"result": text, "diagnostics": diagnostics}))
        }
        (Err(why), _) => (false, json!(why)),
    };
    report.check(
        run,
        formula,
        ok,
        json!({"page": detail, "expected": expected}),
    );
}

pub(crate) fn run(raw: &[String]) -> ExitCode {
    let mut options: BTreeMap<String, String> = BTreeMap::new();
    let mut args = raw.iter();
    while let Some(arg) = args.next() {
        match (arg.strip_prefix("--"), args.next()) {
            (Some(name), Some(value)) => {
                options.insert(name.to_owned(), value.clone());
            }
            _ => {
                eprintln!("{}", crate::package::USAGE);
                return ExitCode::from(2);
            }
        }
    }
    let result = (|| -> Result<(Report, PathBuf), String> {
        let ws = Workspace::load(&std::env::current_dir().map_err(|e| e.to_string())?)?;
        let need = |name: &str| {
            options
                .get(name)
                .map(PathBuf::from)
                .ok_or(format!("--{name} is needed"))
        };
        let fixtures = options
            .get("fixtures")
            .map(PathBuf::from)
            .unwrap_or_else(|| ws.root.join("crates/calc-xll-reference/fixture"));
        let inputs = Inputs {
            package: need("package")?,
            work: need("work")?,
            out: need("out")?,
            x1: need("x1")?,
            x5: need("x5")?,
            exceldna: options
                .get("exceldna")
                .map(PathBuf::from)
                .unwrap_or_else(|| fixtures.join("bin/Release/net48")),
            xlsm: options
                .get("xlsm")
                .map(PathBuf::from)
                .unwrap_or_else(|| fixtures.join("vba/DnaExamples.xlsm")),
            fixtures,
            repo: ws.root.clone(),
        };
        let report = clean_run(&inputs)?;
        Ok((report, inputs.out))
    })();
    match result {
        Ok((report, out)) => {
            let total = report.steps.len();
            let summary = json!({"checks": total, "passed": total - report.failed, "failed": report.failed, "steps": report.steps});
            let path = out.join("clean-run.json");
            let _ = fs::write(
                &path,
                serde_json::to_string_pretty(&summary).unwrap_or_default(),
            );
            println!(
                "ONECALC CLEAN total: {} of {total} checks passed; record {}",
                total - report.failed,
                path.display()
            );
            if report.failed == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(why) => {
            eprintln!("xtask package onecalc-clean-run: {why}");
            ExitCode::from(1)
        }
    }
}

fn clean_run(inputs: &Inputs) -> Result<Report, String> {
    let io = |what: &str, path: &Path, e: std::io::Error| format!("{what} {}: {e}", path.display());
    let work = inputs
        .work
        .canonicalize()
        .or_else(|_| fs::create_dir_all(&inputs.work).and_then(|()| inputs.work.canonicalize()))
        .map_err(|e| io("work directory", &inputs.work, e))?;
    let work = PathBuf::from(work.display().to_string().trim_start_matches(r"\\?\"));
    if work.starts_with(&inputs.repo) {
        return Err("the work directory is inside the repository".into());
    }
    if fs::read_dir(&work)
        .map_err(|e| io("cannot list", &work, e))?
        .next()
        .is_some()
    {
        return Err(format!("{} is not empty", work.display()));
    }
    let inputs = Inputs {
        work: work.clone(),
        package: inputs.package.clone(),
        out: inputs.out.clone(),
        x1: inputs.x1.clone(),
        exceldna: inputs.exceldna.clone(),
        x5: inputs.x5.clone(),
        xlsm: inputs.xlsm.clone(),
        fixtures: inputs.fixtures.clone(),
        repo: inputs.repo.clone(),
    };
    fs::create_dir_all(&inputs.out).map_err(|e| io("cannot create", &inputs.out, e))?;
    let mut report = Report::default();

    // The folder, and what goes beside it.
    copy_dir(&inputs.package, &work.join("app"))?;
    copy_dir(&inputs.exceldna, &work.join("addins/exceldna"))?;
    fs::create_dir_all(work.join("files")).map_err(|e| e.to_string())?;
    fs::copy(&inputs.x1, work.join("addins/X1.xll")).map_err(|e| io("x1", &inputs.x1, e))?;
    fs::copy(&inputs.x5, work.join("addins/X5Native64.xll"))
        .map_err(|e| io("x5", &inputs.x5, e))?;
    fs::copy(&inputs.xlsm, work.join("files/DnaExamples.xlsm"))
        .map_err(|e| io("xlsm", &inputs.xlsm, e))?;
    fs::write(
        work.join("files/not-a-session.xlsx"),
        b"this is not a workbook",
    )
    .map_err(|e| e.to_string())?;
    // A real workbook that is not a OneCalc session (two sheets and a pivot): OneCalc refuses it and DNA Calc opens it.
    let two_sheets = inputs
        .repo
        .join("crates/calc-host/tests/fixtures/pivot/m5-structure/base-range.xlsx");
    fs::copy(&two_sheets, work.join("files/two-sheets.xlsx"))
        .map_err(|e| io("two-sheets", &two_sheets, e))?;
    let mut folder = Vec::new();
    for entry in fs::read_dir(work.join("app"))
        .map_err(|e| e.to_string())?
        .flatten()
    {
        folder.push(entry.file_name().to_string_lossy().into_owned());
    }
    folder.sort();
    report.check(
        "setup",
        "the folder holds the exe and XLCALL32.DLL",
        folder.iter().any(|n| n == EXE)
            && folder
                .iter()
                .any(|n| n.eq_ignore_ascii_case("XLCALL32.DLL")),
        json!(folder),
    );
    let env = minimal_environment(&inputs)?;
    report.check(
        "setup",
        "no environment variable names the repository",
        true,
        json!({"variables": env.keys().collect::<Vec<_>>()}),
    );
    let saves = work.join("saves");
    let x1_cases = read_cases(&inputs.fixtures.join("x1-cases.json"))?;
    let x2_cases = read_cases(&inputs.fixtures.join("x2-cases.json"))?;
    let vba_cases = read_cases(&inputs.fixtures.join("vba/x9-vba-cases.json"))?;
    let case = |cases: &[Value], formula: &str| -> Value {
        cases
            .iter()
            .find(|c| c["formula"] == formula)
            .map(|c| c["value"].clone())
            .unwrap_or(Value::Null)
    };

    // Run A is OneCalc alone in its folder (its check that DNA Calc is missing needs that); DNA Calc is put back
    // for run B.
    let companion = work.join("app").join(CALC_EXE);
    let aside = work.join("app").join("dna-calc-desktop.exe.aside");
    let had_companion = companion.is_file();
    if had_companion {
        fs::rename(&companion, &aside).map_err(|e| e.to_string())?;
    }
    // ---- Run A: the five-minute path.
    let mut a = launch(
        &inputs,
        "a",
        &[
            "--save-dir".into(),
            saves.display().to_string(),
            "--close-answer".into(),
            "cancel".into(),
        ],
        true,
        &[],
    )?;
    let session_ok = a.session.is_some();
    report.check(
        "a",
        "started from the copied folder with the embedded page",
        session_ok && !a.window.is_null(),
        json!({"exe": work.join("app").join(EXE).display().to_string(), "pid": a.pid}),
    );
    {
        let s = a.session();
        let location = script_text(s, "return location.href;");
        let bridge = script_text(
            s,
            "return String(!!(window.chrome && window.chrome.webview));",
        );
        report.check(
            "a",
            "the page is served from memory, with the native bridge",
            location.starts_with("https://dnacalc.app/") && bridge == "true",
            json!({"href": location, "bridge": bridge}),
        );
        let faults = script_text(s, "return JSON.stringify(window.__onecalcFaults||[]);");
        report.check(
            "a",
            "the page recorded no fault",
            faults == "[]",
            json!(faults),
        );
    }
    report.check(
        "a",
        "a new session is titled Untitled",
        wait_title(&mut a, "Untitled - DNA OneCalc"),
        json!(window_title(a.window)),
    );

    // Formulas.
    for (formula, expected) in [
        ("=SUM(1,2,3)", json!(6.0)),
        ("=LAMBDA(x,x*2)(21)", json!(42.0)),
        ("=TEXT(1234.5,\"#,##0.00\")", json!("1,234.50")),
        ("=1/0", json!({"error": "#DIV/0!"})),
        ("=IF(2>1,\"yes\",\"no\")", json!("yes")),
    ] {
        expect(&mut report, "a", a.session(), formula, &expected);
    }
    {
        let s = a.session();
        let ok = evaluate(s, "=SEQUENCE(3,2)")
            .map(|(text, _, _)| text.starts_with("array"))
            .unwrap_or(false);
        report.check(
            "a",
            "=SEQUENCE(3,2) is an array",
            ok,
            json!(text_of(s, "result-shape")),
        );
        // Completion.
        enter_formula(s, "")?;
        enter_formula(s, "=SUMP")?;
        let listed = wait_until(Duration::from_secs(10), || {
            !super::xll::items_of_role(s, "completion").is_empty()
        });
        let items = super::xll::items_of_role(s, "completion");
        report.check(
            "a",
            "completion of =SUMP offers SUMPRODUCT",
            listed && items.iter().any(|i| i.contains("SUMPRODUCT")),
            json!(items.iter().take(5).collect::<Vec<_>>()),
        );
        // F9 on a part (the keys of the smoke suite: Home, Right, Shift+Right x8, F9).
        enter_formula(s, "=SUM(1,2)+3")?;
        s.keys(
            "[data-onecalc=\"formula\"]",
            "\u{E011}\u{E014}\u{E008}\u{E014}\u{E014}\u{E014}\u{E014}\u{E014}\u{E014}\u{E014}\u{E014}\u{E000}\u{E039}",
        )?;
        let done = wait_until(Duration::from_secs(10), || {
            text_of(s, "part-status").contains("evaluated to 3")
        });
        let formula = script_text(
            s,
            "return document.querySelector('[data-onecalc=\"formula\"]').value;",
        );
        report.check(
            "a",
            "F9 on a selected part replaces it with its value",
            done && formula == "=3+3" && text_of(s, "result") == "number 6",
            json!({"formula": formula, "status": text_of(s, "part-status")}),
        );
        // Trace.
        evaluate(s, "=SUM(1,2)*2")?;
        click(s, "trace-start")?;
        let traced = wait_until(Duration::from_secs(10), || {
            !items_of(s, "trace-steps").is_empty()
        });
        let steps = items_of(s, "trace-steps");
        report.check(
            "a",
            "the evaluation trace lists the parts with their values",
            traced
                && steps.iter().any(|t| t.contains("SUM(1,2) = 3"))
                && steps.last().is_some_and(|t| t.contains("= 6")),
            json!(steps),
        );
        // Format picker.
        evaluate(s, "=1234.5678")?;
        open_details(s, "format-controls");
        set_select(s, "format-picker", "#,##0.00")?;
        let formatted = wait_until(Duration::from_secs(10), || {
            text_of(s, "result-value") == "1,234.57"
        });
        report.check(
            "a",
            "the format picker formats the result",
            formatted,
            json!(text_of(s, "result-value")),
        );
        // Title and dirty mark, in the page and in the window.
        report.check(
            "a",
            "an edited session shows the dirty mark in the window title",
            wait_title(&mut a, "Untitled* - DNA OneCalc"),
            json!({"window": window_title(a.window), "page": page_title(a.session())}),
        );
    }
    // Ctrl+S (scripted chooser), then the title, the file and the recent list.
    {
        let s = a.session();
        open_details(s, "file-controls");
        let formula_at_save = script_text(
            s,
            "return document.querySelector('[data-onecalc=\"formula\"]').value;",
        );
        chord(s, "s", "KeyS", 83, 2)?;
        let saved = wait_until(CASE_WAIT, || saved_path(s).is_some());
        let first = saved_path(s).unwrap_or_default();
        let content = saved_formula(&first).unwrap_or_else(|e| e);
        report.check(
            "a",
            "Ctrl+S saves the session",
            saved && first.is_file() && content == formula_at_save.trim_start_matches('='),
            json!({"path": first.display().to_string(), "formula in file": content}),
        );
        let name = first
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        report.check(
            "a",
            "a saved session loses the mark and takes the file's name",
            wait_title(&mut a, &format!("{name} - DNA OneCalc")),
            json!(window_title(a.window)),
        );
        let s = a.session();
        let listed = wait_until(Duration::from_secs(10), || {
            items_of(s, "recent-files")
                .iter()
                .any(|p| Path::new(p) == first)
        });
        report.check(
            "a",
            "the saved file is in the recent list",
            listed,
            json!(items_of(s, "recent-files")),
        );
        // Ctrl+Shift+S asks again: the scripted chooser writes a second file.
        enter_formula(s, "=SUM(10,20)")?;
        let before = text_of(s, "file-status");
        chord(s, "S", "KeyS", 83, 10)?;
        let again = wait_until(CASE_WAIT, || {
            let t = text_of(s, "file-status");
            t != before && saved_path(s).is_some()
        });
        let second = saved_path(s).unwrap_or_default();
        report.check(
            "a",
            "Ctrl+Shift+S saves as another file",
            again && second.is_file() && second != first,
            json!(second.display().to_string()),
        );
        // Reopen the first from the recent list: the formula comes back.
        let before = text_of(s, "file-status");
        let selector = format!(
            "[data-onecalc=\"recent-open\"][data-path=\"{}\"]",
            first.display().to_string().replace('\\', "\\\\")
        );
        let element = s.element(&selector)?;
        s.post(&format!("/element/{element}/click"), json!({}))?;
        let _ = before;
        if text_of(s, "file-dialog").contains("Replace unsaved changes")
            && script_text(
                s,
                "return String(document.querySelector('[data-onecalc=\"file-dialog\"]').open);",
            ) == "true"
        {
            click(s, "file-replace")?;
        }
        let reopened = wait_until(CASE_WAIT, || {
            text_of(s, "file-status").starts_with(&format!("Opened {name}"))
        });
        let formula = script_text(
            s,
            "return document.querySelector('[data-onecalc=\"formula\"]').value;",
        );
        report.check(
            "a",
            "a recent file reopens",
            reopened && formula == formula_at_save,
            json!({"formula": formula, "status": text_of(s, "file-status")}),
        );
    }

    // Add-ins: the X1 fixture, then an Excel-DNA add-in, loaded by path.
    {
        let s = a.session();
        open_details(s, "addin-controls");
        for (label, path, count) in [
            ("X1 fixture", work.join("addins/X1.xll"), "1 loaded"),
            (
                "Excel-DNA net48",
                work.join("addins/exceldna/X0-AddIn64.xll"),
                "2 loaded",
            ),
        ] {
            set_text_input(s, "addin-path", &path.display().to_string())?;
            click(s, "addin-load-path")?;
            let loaded = wait_until(Duration::from_secs(240), || {
                text_of(s, "addin-count") == count
            });
            report.check(
                "a",
                &format!("{label} loads"),
                loaded,
                json!(text_of(s, "addin-status")),
            );
        }
        set_grid(s, "A1", "7")?;
        for formula in [
            "=MyAdd(1,2)",
            "=MyAdd(-4,9.5)",
            "=MyOwned()",
            "=MyThreadSafe(9)",
            "=MyAsync(41)",
            "=MyCallbacks(7,8)",
            "=MyReference(A1)",
        ] {
            expect(&mut report, "a", s, formula, &case(&x1_cases, formula));
        }
        for formula in [
            "=X0.Hello(\"world\")",
            "=X0.Async(41)",
            "=X0.ThreadSafe(21)",
        ] {
            expect(&mut report, "a", s, formula, &case(&x2_cases, formula));
        }
    }
    // DnaExamples.xlsm with macros enabled through the bar.
    {
        let s = a.session();
        let path = work.join("files/DnaExamples.xlsm");
        let element = s.element("[data-onecalc=\"file-input\"]")?;
        s.post(
            &format!("/element/{element}/value"),
            json!({"text": path.display().to_string()}),
        )?;
        thread::sleep(Duration::from_millis(300));
        if script_text(
            s,
            "return String(document.querySelector('[data-onecalc=\"file-dialog\"]').open);",
        ) == "true"
        {
            click(s, "file-replace")?;
        }
        let opened = wait_until(CASE_WAIT, || {
            text_of(s, "file-status").starts_with("Opened DnaExamples.xlsm")
        });
        let bar = text_of(s, "macros-bar");
        report.check(
            "a",
            "DnaExamples.xlsm opens with macros disabled",
            opened && bar.contains("disabled"),
            json!(bar.trim()),
        );
        click(s, "macros-enable")?;
        let enabled = wait_until(CASE_WAIT, || {
            let t = text_of(s, "vba-status");
            !t.is_empty() && !t.starts_with("Macros are disabled")
        });
        report.check(
            "a",
            "the bar enables the file's macros",
            enabled && text_of(s, "vba-status").contains("loaded"),
            json!(text_of(s, "vba-status")),
        );
        for formula in [
            "=DnaAdd(40,2)",
            "=DnaGreet(\"OneCalc\")",
            "=DnaJoin(\"a\",\"+\",7)",
            "=DnaSum(1,2,3.5)",
            "=DnaDictType()",
        ] {
            expect(&mut report, "a", s, formula, &case(&vba_cases, formula));
        }
    }
    // A file OneCalc refuses, and Open in DNA Calc with no DNA Calc beside the exe.
    {
        let s = a.session();
        let element = s.element("[data-onecalc=\"file-input\"]")?;
        s.post(
            &format!("/element/{element}/value"),
            json!({"text": work.join("files/not-a-session.xlsx").display().to_string()}),
        )?;
        thread::sleep(Duration::from_millis(300));
        if script_text(
            s,
            "return String(document.querySelector('[data-onecalc=\"file-dialog\"]').open);",
        ) == "true"
        {
            click(s, "file-replace")?;
        }
        let refused = wait_until(CASE_WAIT, || {
            text_of(s, "file-status").contains("outside what a OneCalc session holds")
        });
        let hidden = script_text(
            s,
            "return String(document.querySelector('[data-onecalc=\"open-in-dna-calc\"]').hidden);",
        );
        report.check(
            "a",
            "a file outside a session is refused with the offer",
            refused && hidden == "false",
            json!(text_of(s, "file-status")),
        );
        click(s, "open-in-dna-calc")?;
        let said = wait_until(CASE_WAIT, || {
            text_of(s, "file-status").contains("dna-calc-desktop.exe")
        });
        report.check(
            "a",
            "Open in DNA Calc says plainly that DNA Calc is not in the folder",
            said,
            json!(text_of(s, "file-status")),
        );
    }
    // RTD over the numeric test server: the X5 add-in registers its server for this process.
    {
        let s = a.session();
        let progid = format!("DnaCalc.X5.{}", a.pid);
        set_text_input(
            s,
            "addin-path",
            &work.join("addins/X5Native64.xll").display().to_string(),
        )?;
        click(s, "addin-load-path")?;
        let loaded = wait_until(Duration::from_secs(120), || {
            text_of(s, "addin-count") == "3 loaded"
        });
        report.check(
            "a",
            "the X5 numeric RTD add-in loads and registers its server",
            loaded && registry_has(&progid),
            json!({"progid": progid, "status": text_of(s, "addin-status")}),
        );
        for (formula, label) in [
            ("=10+X5.RtdValue()+3", "RTD through the add-in's function"),
            (
                &format!("=10+RTD(\"{progid}\",,\"a\")+3"),
                "RTD through the built-in RTD function",
            ),
        ] {
            enter_formula(s, "")?;
            wait_until(PAGE_WAIT, || text_of(s, "result") == "no formula");
            enter_formula(s, formula)?;
            let mut seen: Vec<String> = Vec::new();
            let reached = wait_until(Duration::from_secs(40), || {
                let now = text_of(s, "result");
                if !now.is_empty() && seen.last() != Some(&now) {
                    seen.push(now.clone());
                }
                now == "number 55"
            });
            report.check(
                "a",
                label,
                reached,
                json!({"formula": formula, "answers in order": seen}),
            );
        }
        enter_formula(s, "=99")?;
        let replaced = wait_until(CASE_WAIT, || text_of(s, "result") == "number 99");
        report.check(
            "a",
            "replacing the RTD formula ends its topic and shows the new value",
            replaced,
            json!(text_of(s, "result")),
        );
        // Unload X5: its close routine unregisters the private class.
        // The RTD client lives until the app closes (dc-snz.23): an add-in whose server has run is
        // refused its unload with that message, and its private class is removed when the app closes
        // (the last check of the run).
        let element = s.element("[data-onecalc=\"addin-loaded\"][data-path$=\"X5Native64.xll\"] [data-onecalc=\"addin-unload\"]")?;
        s.post(&format!("/element/{element}/click"), json!({}))?;
        let said = wait_until(CASE_WAIT, || {
            text_of(s, "addin-status").contains("waits for RTD server termination")
        });
        report.check(
            "a",
            "unloading an add-in whose RTD server ran is refused until the app closes (dc-snz.23)",
            said && text_of(s, "addin-count") == "3 loaded",
            json!({"status": text_of(s, "addin-status")}),
        );
    }
    // Close asks: dirty, close answered Cancel, the window stays; saved, close goes.
    {
        let s = a.session();
        enter_formula(s, "=7*6")?;
        let window = a.window;
        let dirty = wait_until(Duration::from_secs(15), || {
            window_title(window).contains("* - DNA OneCalc")
        });
        unsafe { PostMessageW(a.window, WM_CLOSE, 0, 0) };
        thread::sleep(Duration::from_secs(3));
        let stays = a.running() && unsafe { IsWindow(a.window) } != 0;
        report.check(
            "a",
            "closing with unsaved changes asks, and Cancel keeps the window",
            dirty && stays,
            json!({"title": window_title(a.window)}),
        );
        // Where the window goes: move it, save, close.
        unsafe {
            SetWindowPos(
                a.window,
                std::ptr::null_mut(),
                120,
                90,
                1100,
                780,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
        thread::sleep(Duration::from_millis(500));
        let rect = window_rect(a.window);
        chord(a.session(), "s", "KeyS", 83, 2)?;
        thread::sleep(Duration::from_secs(2));
        let code = a.close();
        report.check(
            "a",
            "a saved session closes without asking",
            code == Some(0),
            json!({"exit": code, "rect": rect}),
        );
        let settings = fs::read_to_string(work.join("data/settings.json")).unwrap_or_default();
        let parsed: Value = serde_json::from_str(&settings).unwrap_or(Value::Null);
        let saved_rect = (
            parsed["window"]["left"].as_i64().unwrap_or(-1) as i32,
            parsed["window"]["top"].as_i64().unwrap_or(-1) as i32,
            parsed["window"]["right"].as_i64().unwrap_or(-1) as i32,
            parsed["window"]["bottom"].as_i64().unwrap_or(-1) as i32,
        );
        report.check(
            "a",
            "the window's place and the recent files are saved",
            saved_rect == rect && parsed["recent"].as_array().is_some_and(|r| r.len() >= 2),
            json!({"settings": parsed, "rect": rect}),
        );
        report.check(
            "a",
            "the WebView2 profile is in the fresh data folder",
            work.join("data/WebView2").is_dir(),
            json!(work.join("data/WebView2").display().to_string()),
        );
        window_rect_store(&work, rect);
    }

    if had_companion {
        fs::rename(&aside, &companion).map_err(|e| e.to_string())?;
    }
    // ---- Run B: the second start.
    let rect_a = read_rect(&work);
    // DNA Calc beside the exe is the packaged one, with its page inside it (a folder packed without it gets no
    // companion, and the launch check below says so).
    let mut b = launch(
        &inputs,
        "b",
        &["--close-answer".into(), "save".into()],
        true,
        &[],
    )?;
    {
        let rect = window_rect(b.window);
        report.check(
            "b",
            "the window comes back where it was left",
            rect == rect_a,
            json!({"now": rect, "before": rect_a}),
        );
        let s = b.session();
        open_details(s, "file-controls");
        let recent = items_of(s, "recent-files");
        report.check(
            "b",
            "the recent files are still listed",
            recent.len() >= 2,
            json!(recent),
        );
        let first = recent.first().cloned().unwrap_or_default();
        let before_bytes = fs::read(&first).unwrap_or_default();
        let selector = format!(
            "[data-onecalc=\"recent-open\"][data-path=\"{}\"]",
            first.replace('\\', "\\\\")
        );
        let element = s.element(&selector)?;
        s.post(&format!("/element/{element}/click"), json!({}))?;
        let name = Path::new(&first)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let opened = wait_until(CASE_WAIT, || {
            text_of(s, "file-status").starts_with(&format!("Opened {name}"))
        });
        report.check(
            "b",
            "a recent file opens in a new run",
            opened,
            json!(text_of(s, "file-status")),
        );
        enter_formula(s, "=SUM(2,3)")?;
        report.check(
            "b",
            "the opened session shows its name and the mark",
            wait_title(&mut b, &format!("{name}* - DNA OneCalc")),
            json!(window_title(b.window)),
        );
        let s = b.session();
        chord(s, "s", "KeyS", 83, 2)?;
        let saved = wait_until(CASE_WAIT, || {
            text_of(s, "file-status").starts_with("Saved ")
        });
        let after = fs::read(&first).unwrap_or_default();
        report.check(
            "b",
            "Ctrl+S writes the opened file in place, without a dialog",
            saved
                && after != before_bytes
                && saved_formula(Path::new(&first)).as_deref() == Ok("SUM(2,3)"),
            json!({"file": first}),
        );
        // Open in DNA Calc with DNA Calc beside the exe: the launcher starts the packaged dna-calc-desktop.exe.
        let element = s.element("[data-onecalc=\"file-input\"]")?;
        s.post(
            &format!("/element/{element}/value"),
            json!({"text": work.join("files/two-sheets.xlsx").display().to_string()}),
        )?;
        wait_until(CASE_WAIT, || {
            text_of(s, "file-status").contains("outside what a OneCalc session holds")
        });
        click(s, "open-in-dna-calc")?;
        let launched = wait_until(CASE_WAIT, || {
            text_of(s, "file-status") == "Opened the file in DNA Calc."
        });
        report.check(
            "b",
            "Open in DNA Calc launches dna-calc-desktop.exe when it is beside the exe",
            launched,
            json!(text_of(s, "file-status")),
        );
        let spawned = wait_child(b.pid, CALC_EXE);
        let at_the_folder = spawned
            .as_ref()
            .is_some_and(|(_, path)| Path::new(path).starts_with(work.join("app")));
        report.check(
            "b",
            "the process it started is the packaged dna-calc-desktop.exe of the copied folder",
            at_the_folder,
            json!(spawned),
        );
        if let Some((calc_pid, _)) = spawned {
            let window = (0..300).find_map(|_| {
                thread::sleep(Duration::from_millis(100));
                window_of(calc_pid)
            });
            report.check(
                "b",
                "DNA Calc shows its window, so its page loaded from inside the executable",
                window.is_some(),
                json!({"pid": calc_pid}),
            );
            // It stays up while the page loads the file; then it is closed as a person would, by its window.
            thread::sleep(Duration::from_secs(8));
            let alive = child_alive(calc_pid);
            report.check(
                "b",
                "DNA Calc is still running after the page and the file loaded",
                alive,
                json!({"pid": calc_pid}),
            );
            if let Some(window) = window {
                unsafe { PostMessageW(window, WM_CLOSE, 0, 0) };
            }
            let closed = wait_until(Duration::from_secs(20), || !child_alive(calc_pid));
            report.check(
                "b",
                "closing its window ends it",
                closed,
                json!({"pid": calc_pid}),
            );
        }
        let s = b.session();
        // Close with unsaved changes, answered Save: the file is written first.
        enter_formula(s, "=SUM(4,5)")?;
        wait_title(&mut b, &format!("{name}* - DNA OneCalc"));
        let code = b.close();
        let formula = saved_formula(Path::new(&first)).unwrap_or_default();
        report.check(
            "b",
            "closing with unsaved changes and answering Save saves the file, then closes",
            code == Some(0) && formula == "SUM(4,5)",
            json!({"exit": code, "formula in file": formula}),
        );
    }

    // ---- Run C: close answered Discard.
    let mut c = launch(
        &inputs,
        "c",
        &["--close-answer".into(), "discard".into()],
        true,
        &[],
    )?;
    {
        let s = c.session();
        open_details(s, "file-controls");
        let first = items_of(s, "recent-files")
            .first()
            .cloned()
            .unwrap_or_default();
        let before = calc_verdict::sha256_hex(&fs::read(&first).unwrap_or_default());
        let selector = format!(
            "[data-onecalc=\"recent-open\"][data-path=\"{}\"]",
            first.replace('\\', "\\\\")
        );
        let element = s.element(&selector)?;
        s.post(&format!("/element/{element}/click"), json!({}))?;
        wait_until(CASE_WAIT, || {
            text_of(s, "file-status").starts_with("Opened ")
        });
        enter_formula(s, "=SUM(100,1)")?;
        thread::sleep(Duration::from_secs(1));
        let code = c.close();
        let after = calc_verdict::sha256_hex(&fs::read(&first).unwrap_or_default());
        report.check(
            "c",
            "closing with unsaved changes and answering Discard leaves the file as it was",
            code == Some(0) && before == after,
            json!({"exit": code}),
        );
    }

    // ---- Run E: DNA Calc, the companion, from the copied folder.
    calc_companion(&inputs, &mut report)?;

    // ---- Run D: no WebView2 runtime.
    let empty = work.join("no-webview2");
    fs::create_dir_all(&empty).map_err(|e| e.to_string())?;
    let message = work.join("message.txt");
    let mut d = launch(
        &inputs,
        "d",
        &["--message-file".into(), message.display().to_string()],
        false,
        &[(
            "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER",
            empty.display().to_string(),
        )],
    )?;
    let started = Instant::now();
    let mut code = None;
    while started.elapsed() < Duration::from_secs(30) {
        if let Ok(Some(status)) = d.app.child.try_wait() {
            code = status.code();
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let text = fs::read_to_string(&message).unwrap_or_default();
    report.check(
        "d",
        "without WebView2 the app says so, with the download link, and exits",
        code == Some(1)
            && text.contains("WebView2")
            && text.contains("https://developer.microsoft.com/microsoft-edge/webview2"),
        json!({"exit": code, "message": text}),
    );

    // What the runs left behind: the private COM class of X5 is gone, no profile in the temporary folder.
    let leftover = registry_has(&format!("DnaCalc.X5.{}", a.pid));
    report.check(
        "end",
        "no private COM class of this run is left in HKCU",
        !leftover,
        json!({"pid": a.pid}),
    );
    Ok(report)
}

/// The process `image` that `parent` started, with its executable's path: the first one found within the wait.
fn wait_child(parent: u32, image: &str) -> Option<(u32, String)> {
    let script = format!(
        "Get-CimInstance Win32_Process -Filter \"ParentProcessId={parent} and Name='{image}'\" | ForEach-Object {{ \"$($_.ProcessId)|$($_.ExecutablePath)\" }}"
    );
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(30) {
        if let Ok(out) = Command::new("pwsh")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            if let Some((pid, path)) = text.lines().next().and_then(|line| line.split_once('|')) {
                if let Ok(pid) = pid.trim().parse() {
                    return Some((pid, path.trim().to_owned()));
                }
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
    None
}

fn child_alive(pid: u32) -> bool {
    let script = format!(
        "if (Get-Process -Id {pid} -ErrorAction SilentlyContinue) {{ 'yes' }} else {{ 'no' }}"
    );
    Command::new("pwsh")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).trim() == "yes")
}

/// Run E: `dna-calc-desktop.exe --open <workbook>` from the copied folder, with the clean environment, no
/// `--assets` and no `page` folder, so its page is the one inside it. Driven like the others: chromedriver
/// on its WebView2; the page must show the workbook's sheets.
fn calc_companion(inputs: &Inputs, report: &mut Report) -> Result<(), String> {
    let exe = inputs.work.join("app").join(CALC_EXE);
    if !exe.is_file() {
        report.check(
            "e",
            "the folder holds dna-calc-desktop.exe",
            false,
            json!(exe.display().to_string()),
        );
        return Ok(());
    }
    let file = inputs.work.join("files/two-sheets.xlsx");
    let upstream = free_port()?;
    let stderr =
        fs::File::create(inputs.work.join("stderr-calc.txt")).map_err(|e| e.to_string())?;
    let child = Command::new(&exe)
        .env_clear()
        .envs(minimal_environment(inputs)?)
        .env("DNACALC_WEBVIEW2_DATA", inputs.work.join("calc-data"))
        .current_dir(&inputs.work)
        .arg("--open")
        .arg(&file)
        .args(["--debug-port", &upstream.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr)
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", exe.display()))?;
    let pid = child.id();
    let mut app = DriverProcess {
        child,
        port: upstream,
    };
    let started = Instant::now();
    let mut window: HWND = std::ptr::null_mut();
    while started.elapsed() < WAIT && window.is_null() {
        window = window_of(pid).unwrap_or(std::ptr::null_mut());
        thread::sleep(Duration::from_millis(100));
    }
    report.check(
        "e",
        "DNA Calc starts from the copied folder with no page folder and shows its window",
        !window.is_null(),
        json!({"pid": pid}),
    );
    while TcpStream::connect(("127.0.0.1", upstream)).is_err() {
        if started.elapsed() > Duration::from_secs(90) {
            report.check(
                "e",
                "DNA Calc opens its DevTools port",
                false,
                json!(upstream),
            );
            return Ok(());
        }
        thread::sleep(Duration::from_millis(200));
    }
    let (_driver, session) = attach_page(upstream, &inputs.out.join("driver-e"))?;
    let tabs = "return Array.from(document.querySelectorAll('[data-dnacalc=\"sheet-tab\"]')).map(t => t.textContent.trim()).join('|');";
    let shown = wait_until(WAIT, || !script_text(&session, tabs).is_empty());
    let text = script_text(&session, tabs);
    report.check(
        "e",
        "the page inside the executable shows the opened workbook's sheets",
        shown && text == "Src|Out",
        json!(text),
    );
    let status = script_text(
        &session,
        "var e=document.querySelector('[data-dnacalc=\"worker-status\"]'); return e ? e.textContent : '';",
    );
    report.check(
        "e",
        "the page's worker is running",
        !status.contains("fail") && !status.contains("error"),
        json!(status),
    );
    if !window.is_null() {
        unsafe { PostMessageW(window, WM_CLOSE, 0, 0) };
    }
    let closed = wait_until(Duration::from_secs(20), || {
        matches!(app.child.try_wait(), Ok(Some(_)))
    });
    report.check(
        "e",
        "closing the window ends DNA Calc",
        closed,
        json!({"pid": pid}),
    );
    Ok(())
}

fn read_cases(path: &Path) -> Result<Vec<Value>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    value
        .as_array()
        .cloned()
        .ok_or_else(|| format!("{} is not an array", path.display()))
}

fn window_rect_store(work: &Path, rect: (i32, i32, i32, i32)) {
    let _ = fs::write(
        work.join("rect-a.txt"),
        format!("{} {} {} {}", rect.0, rect.1, rect.2, rect.3),
    );
}

fn read_rect(work: &Path) -> (i32, i32, i32, i32) {
    let text = fs::read_to_string(work.join("rect-a.txt")).unwrap_or_default();
    let n: Vec<i32> = text
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    if n.len() == 4 {
        (n[0], n[1], n[2], n[3])
    } else {
        (0, 0, 0, 0)
    }
}
