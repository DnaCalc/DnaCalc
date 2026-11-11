//! DNA Calc on the Windows desktop: the workbook composition over CalcHost, with the add-ins
//! loaded through `calc-addin-host` and the window chain and WebView2 host from
//! `calc-platform-desktop`. Two modes:
//! - `--app`: the DNA Calc page in WebView2 over the native workbook worker (`app.rs`);
//! - `--engine`: the command-line harness through CalcHost's checked route (`engine.rs`), whose
//!   case files are the same ones the OneCalc harness runs (`dna-onecalc-desktop`).
//!
//! This executable does not depend on `dna-onecalc`; OneCalc's harness is the other executable.
//! Each exports its own `MdCallBack12` and carries its own version resource.
#[cfg(target_arch = "wasm32")]
compile_error!("dna-calc-desktop is native-only");
#[cfg(windows)]
mod app;
#[cfg(windows)]
mod embedded;
#[cfg(windows)]
mod engine;
#[cfg(windows)]
mod fetch;
mod file_import;
mod origin;

#[cfg(windows)]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MdCallBack12(
    function: i32,
    count: i32,
    args: *const *mut calc_xll_abi::XlOper12,
    result: *mut calc_xll_abi::XlOper12,
) -> i32 {
    unsafe { calc_xll_host::dispatch(function, count, args, result) }
}

/// Loads one add-in file into `set`: an `.xlam` (a VBA add-in: its project's public functions,
/// installed as Excel's Add-ins dialog installs one, so `Workbook_Open` then
/// `Workbook_AddinInstall` run) or else an `.xll`. Returns what the add-in's workbook events
/// reported, for the caller to show.
#[cfg(windows)]
pub(crate) fn load_addin(
    set: &mut calc_addin_host::AddinHost,
    path: &std::path::Path,
) -> Result<Vec<String>, String> {
    let is_xlam = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xlam"));
    if !is_xlam {
        set.load(path)?;
        return Ok(Vec::new());
    }
    let named = |why: String| format!("{}: {why}", path.display());
    let bytes = std::fs::read(path).map_err(|e| named(e.to_string()))?;
    let vba = calc_host::read_vba_file(bytes)
        .map_err(|refusal| named(refusal.to_string()))?
        .ok_or_else(|| named("the add-in has no VBA project".to_owned()))?;
    let load = set
        .load_vba_addin_part(path, &vba.project, true)
        .map_err(named)?;
    Ok(load.notes)
}

#[cfg(windows)]
fn run() -> Result<(), String> {
    use calc_addin_host::{AddinHost, Document};
    use calc_platform_desktop::Shell;
    use std::path::PathBuf;
    let all: Vec<String> = std::env::args().skip(1).collect();
    if all.first().map(String::as_str) == Some("--version") {
        println!("DNA Calc {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    // `--open FILE` and a file path as the first argument are app mode: the page with that workbook.
    let first_is_file = all.first().is_some_and(|arg| !arg.starts_with("--"));
    if first_is_file || all.iter().any(|arg| arg == "--app" || arg == "--open") {
        return app::run(&all);
    }
    let mut xlls = Vec::new();
    let mut formula = "=MyAdd(1,2)".to_owned();
    let mut cases_file = None;
    let mut early_execution = Vec::new();
    let mut engine = false;
    let mut ribbon_ui = false;
    let mut sheet_name = "X1".to_owned();
    let mut workbook_name = "Book1".to_owned();
    let mut wait = None;
    let mut ready = None;
    let mut startup_ack = None;
    let mut finished = None;
    let mut release = None;
    let mut workbook = None;
    let mut enable_macros = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--xll" => xlls.push(PathBuf::from(args.next().ok_or("--xll needs a path")?)),
            "--formula" => formula = args.next().ok_or("--formula needs text")?,
            "--self-test" => {}
            "--cases" => {
                cases_file = Some(PathBuf::from(args.next().ok_or("--cases needs a path")?))
            }
            "--sheet" => sheet_name = args.next().ok_or("--sheet needs a name")?,
            "--workbook-name" => {
                workbook_name = args.next().ok_or("--workbook-name needs a name")?
            }
            "--early-execution" => {
                early_execution.push(args.next().ok_or("--early-execution needs a name")?)
            }
            "--engine" => engine = true,
            "--workbook" => {
                workbook = Some(PathBuf::from(args.next().ok_or("--workbook needs a path")?))
            }
            // The person enables the --workbook file's own VBA project (dc-0f1k.22); without it the
            // project is held, not run, as the page holds it until its bar's Enable macros.
            "--enable-macros" => enable_macros = true,
            "--ribbon-ui" => ribbon_ui = true,
            "--ready" => ready = Some(PathBuf::from(args.next().ok_or("--ready needs a path")?)),
            "--wait" => wait = Some(PathBuf::from(args.next().ok_or("--wait needs a path")?)),
            "--startup-ack" => {
                startup_ack = Some(PathBuf::from(
                    args.next().ok_or("--startup-ack needs a path")?,
                ))
            }
            "--finished" => {
                finished = Some(PathBuf::from(args.next().ok_or("--finished needs a path")?))
            }
            "--release" => {
                release = Some(PathBuf::from(args.next().ok_or("--release needs a path")?))
            }
            "--help" => {
                println!(
                    "dna-calc-desktop [FILE | --open FILE] --app (--assets DIR | --url URL | a `page` folder beside the executable) [--xll PATH ...] | --engine --xll PATH [--xll PATH ...] (PATH an .xll or a VBA .xlam) [--workbook FILE [--enable-macros]] [--formula TEXT | --cases PATH] [--sheet NAME] [--workbook-name NAME] [--early-execution NAME]"
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }
    if !engine {
        return Err("choose --app (the page) or --engine (the CalcHost harness); the OneCalc harness is dna-onecalc-desktop".into());
    }
    if workbook_name.is_empty()
        || workbook_name
            .chars()
            .any(|c| c.is_control() || c == '[' || c == ']')
    {
        return Err(
            "--workbook-name needs a nonempty workbook name without brackets or control characters"
                .into(),
        );
    }
    if xlls.is_empty() && workbook.is_none() {
        return Err("provide --xll PATH or --workbook PATH".into());
    }
    let shell = Shell::new("DNA Calc")?;
    if let Some(path) = ready {
        std::fs::write(path, "ready").map_err(|e| e.to_string())?;
    }
    if let Some(path) = wait {
        shell.wait_for(&path)?;
    }
    // The add-in set is dropped before the shell, so add-ins close while their windows exist.
    // The engine names its sheets once its workbook exists.
    let mut addins = AddinHost::new(
        &shell,
        &Document {
            workbook: &workbook_name,
            active_sheet: Some(&sheet_name),
            sheets: &[],
        },
    )?;
    for xll in &xlls {
        for note in load_addin(&mut addins, xll)? {
            eprintln!("dna-calc-desktop: {}: {note}", xll.display());
        }
    }
    if let Some(path) = startup_ack {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while !path.is_file() {
            if std::time::Instant::now() > deadline {
                return Err("add-in startup event acknowledgment expired".into());
            }
            shell.pump_for(std::time::Duration::from_millis(10));
            addins.after_calculate()?;
        }
    }
    let early_execution = early_execution
        .iter()
        .map(|name| {
            addins
                .active_registration_id(name)
                .ok_or_else(|| format!("no active early-execution registration: {name}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let ribbons = if ribbon_ui {
        addins.ribbons()?
    } else {
        Vec::new()
    };
    let mut output = engine::run(
        &shell,
        &mut addins,
        &sheet_name,
        &workbook_name,
        cases_file,
        formula,
        &early_execution,
        workbook.as_deref(),
        enable_macros,
    )?;
    output["ribbons"] = serde_json::json!(ribbons);
    addins.close_addins()?;
    println!("{output}");
    if let Some(path) = finished {
        std::fs::write(path, "finished").map_err(|e| e.to_string())?;
    }
    if let Some(path) = release {
        shell.wait_for(&path)?;
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    // Before this process creates threads or loads any add-in (see `prepare_process`).
    #[cfg(windows)]
    {
        calc_addin_host::prepare_process();
        match run() {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(why) => {
                eprintln!("dna-calc-desktop: {why}");
                // The packaged executable has no console to print to: say it in a box (a missing
                // WebView2 has said its own message already). A run with a test flag only
                // reports on stderr and by its exit code.
                let scripted = std::env::args().any(|a| {
                    [
                        "--debug-port",
                        "--save-dir",
                        "--probe",
                        "--hidden",
                        "--message-file",
                    ]
                    .contains(&a.as_str())
                });
                if embedded::has_page()
                    && !scripted
                    && !why.starts_with("DNA Calc shows its window")
                {
                    calc_platform_desktop::dialogs::message(
                        std::ptr::null_mut(),
                        "DNA Calc",
                        &why,
                        calc_platform_desktop::dialogs::Buttons::Ok,
                        true,
                    );
                }
                std::process::ExitCode::FAILURE
            }
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("dna-calc-desktop XLL hosting requires Windows x64");
        std::process::ExitCode::FAILURE
    }
}
