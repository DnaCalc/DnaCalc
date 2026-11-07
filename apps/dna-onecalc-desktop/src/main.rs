//! DNA OneCalc on the Windows desktop. With `--app` it shows OneCalc's page in WebView2 over the
//! native session ([`app`], [`host`]: open and save through the native dialogs, the add-ins loaded
//! through `calc-addin-host`). Without it, it is the command-line harness over
//! `dna_onecalc::Session` (formulas without a workbook or a calculation engine) that the add-in
//! comparisons run, with the window chain from `calc-platform-desktop`.
//!
//! OneCalc's dependency ceiling holds here: this executable reaches no `calc-host` and no
//! `calc-engine`. The harness for DNA Calc's workbook composition is the other executable,
//! `dna-calc-desktop`; each exports its own `MdCallBack12` and carries its own version resource.
#[cfg(target_arch = "wasm32")]
compile_error!("dna-onecalc-desktop is native-only");

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

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod host;

#[cfg(windows)]
const WORKBOOK: &str = "Book1";

#[cfg(windows)]
fn run() -> Result<(), String> {
    use calc_addin_host::{AddinHost, Document, value_json};
    use calc_platform_desktop::Shell;
    use calc_value::SheetId;
    use std::path::PathBuf;
    // `--app`: the OneCalc page in WebView2 over the native session (src/app.rs).
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.first().map(String::as_str) == Some("--app") {
        return app::run(&raw);
    }
    let mut xlls = Vec::new();
    let mut formula = "=MyAdd(1,2)".to_owned();
    let mut self_test = false;
    let mut cases_file = None;
    let mut commands = Vec::new();
    let mut early_commands = Vec::new();
    let mut reload = false;
    let mut reload_new_host = false;
    let mut ribbon_ui = false;
    let mut sheet_name = "X1".to_owned();
    let mut wait = None;
    let mut ready = None;
    let mut startup_ack = None;
    let mut finished = None;
    let mut release = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--xll" => xlls.push(PathBuf::from(args.next().ok_or("--xll needs a path")?)),
            "--formula" => formula = args.next().ok_or("--formula needs text")?,
            "--self-test" => self_test = true,
            "--cases" => {
                cases_file = Some(PathBuf::from(args.next().ok_or("--cases needs a path")?))
            }
            "--command" => commands.push(args.next().ok_or("--command needs a name")?),
            "--sheet" => sheet_name = args.next().ok_or("--sheet needs a name")?,
            "--early-command" => {
                early_commands.push(args.next().ok_or("--early-command needs a name")?)
            }
            "--reload" => reload = true,
            "--ribbon-ui" => ribbon_ui = true,
            "--reload-new-host" => {
                reload = true;
                reload_new_host = true;
            }
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
                    "dna-onecalc-desktop --xll PATH [--xll PATH ...] [--formula TEXT | --cases PATH | --self-test] [--sheet NAME] [--command NAME] [--early-command NAME] [--reload]"
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }
    if xlls.is_empty() {
        return Err("provide --xll PATH".into());
    }
    let shell = Shell::new("DNA OneCalc")?;
    if let Some(path) = ready {
        std::fs::write(path, "ready").map_err(|e| e.to_string())?;
    }
    if let Some(path) = wait {
        shell.wait_for(&path)?;
    }
    let mut session = dna_onecalc::Session::new(&sheet_name).map_err(|e| format!("{e:?}"))?;
    let sheets: Vec<(SheetId, String)> = session
        .sheets()
        .map(|(name, id)| (id, name.to_owned()))
        .collect();
    let sheet_refs: Vec<(SheetId, &str)> = sheets.iter().map(|(id, n)| (*id, n.as_str())).collect();
    let document = Document {
        workbook: WORKBOOK,
        active_sheet: Some(&sheet_name),
        sheets: &sheet_refs,
    };
    // The add-in set is dropped before the shell, so add-ins close while their windows exist.
    let mut addins = AddinHost::new(&shell, &document)?;
    for xll in &xlls {
        addins.load(xll)?;
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
    let registrations = addins.registrations();
    let ribbons = if ribbon_ui {
        addins.ribbons()?
    } else {
        Vec::new()
    };
    let stale_name = registrations
        .iter()
        .find(|r| r.macro_type == 1)
        .ok_or("add-in registered no worksheet functions")?
        .name
        .clone();
    shell.pump_for(std::time::Duration::from_millis(100));
    session
        .set_formula_cell("B3")
        .map_err(|e| format!("{e:?}"))?;
    let input_cell = session
        .set_input(
            "A1",
            dna_onecalc::InputValue::number(7.).map_err(|e| format!("{e:?}"))?,
        )
        .map_err(|e| format!("{e:?}"))?;
    addins.sheet_changed(input_cell.row(), input_cell.column())?;
    session.set_user_functions(addins.callables()?);
    let tests: Vec<serde_json::Value> = if self_test || cases_file.is_some() {
        let text = if let Some(path) = cases_file {
            std::fs::read_to_string(path).map_err(|e| e.to_string())?
        } else {
            include_str!("../../../crates/calc-xll-reference/fixture/x1-cases.json").to_owned()
        };
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(&text).map_err(|e| e.to_string())?;
        cases
    } else {
        vec![serde_json::json!({"formula": formula, "value": null})]
    };
    let mut results = Vec::new();
    for case in tests {
        let formula = case["formula"].as_str().ok_or("case needs formula")?;
        let expected = match case["value"].as_str() {
            Some(text) if text.starts_with("<workbook>|") => {
                serde_json::json!(text.replacen("<workbook>", WORKBOOK, 1))
            }
            _ => case["value"].clone(),
        };
        if let Some(enabled) = case["enable_events"].as_bool() {
            addins.enable_events(enabled)?;
        }
        if let Some(edit) = case.get("edit") {
            let address = edit["cell"].as_str().ok_or("edit needs cell")?;
            let value = edit["value"]
                .as_f64()
                .ok_or("desktop case edit needs a number")?;
            let cell = session
                .set_input(
                    address,
                    dna_onecalc::InputValue::number(value).map_err(|e| format!("{e:?}"))?,
                )
                .map_err(|e| format!("{e:?}"))?;
            addins.sheet_changed(cell.row(), cell.column())?;
        }
        session.set_formula(formula);
        let cell = session.formula_cell();
        addins.sheet_changed(cell.row(), cell.column())?;
        let calculation = session.calculate();
        if !calculation.diagnostics.is_empty() {
            return Err(format!("{formula}: {:?}", calculation.diagnostics));
        }
        let value = value_json(
            calculation
                .result
                .as_ref()
                .ok_or("formula has no calculated value")?,
        );
        if !expected.is_null()
            && value != expected
            && !(value.is_number() && expected.is_number() && value.as_f64() == expected.as_f64())
        {
            return Err(format!(
                "{formula}: expected {expected}, calculated {value}"
            ));
        }
        results.push(serde_json::json!({"formula":formula,"value":value}));
        addins.after_calculate()?;
        shell.pump_for(std::time::Duration::from_millis(20));
    }
    let mut command_results = Vec::new();
    for command in commands {
        let value = addins.run(&command, &[])?;
        command_results.push(serde_json::json!({"command":command,"value":value_json(&value)}));
        shell.pump_for(std::time::Duration::from_millis(200));
    }
    for command in early_commands {
        let value = addins.run_early(&command)?;
        command_results.push(
            serde_json::json!({"command":command,"value":value_json(&value),"early_bound":true}),
        );
        shell.pump_for(std::time::Duration::from_millis(200));
    }
    let mut reload_results = Vec::new();
    if reload {
        let retained = addins.callables()?;
        for xll in xlls.iter().rev() {
            addins.unload(xll)?;
        }
        if retained.call(&stale_name, &[])?
            != calc_value::CalcValue::error(calc_value::WorksheetErrorCode::Name)
        {
            return Err("retained registration ran after removal".into());
        }
        session.set_user_functions(addins.callables()?);
        drop(retained);
        if reload_new_host {
            addins.restart(&shell, &document)?;
        }
        for xll in &xlls {
            addins.load(xll)?;
        }
        session.set_user_functions(addins.callables()?);
        shell.pump_for(std::time::Duration::from_millis(100));
        for case in &results {
            let formula = case["formula"].as_str().ok_or("missing reload formula")?;
            session.set_formula(formula);
            let calculation = session.calculate();
            if !calculation.diagnostics.is_empty() {
                return Err(format!(
                    "after reload {formula}: {:?}",
                    calculation.diagnostics
                ));
            }
            let value = value_json(calculation.result.as_ref().ok_or("missing reload value")?);
            if value != case["value"] {
                return Err(format!("reload changed {formula}: {value}"));
            }
            reload_results.push(serde_json::json!({"formula":formula,"value":value}));
        }
    }
    let native = addins.native();
    let retained = addins.callables()?;
    if native.live_callback_allocations() != 0 {
        return Err(format!(
            "{} callback allocations remain before close",
            native.live_callback_allocations()
        ));
    }
    addins.close_addins()?;
    if native.live_callback_allocations() != 0 {
        return Err(format!(
            "{} callback allocations remain",
            native.live_callback_allocations()
        ));
    }
    let stale = retained.call(&stale_name, &[])?;
    if stale != calc_value::CalcValue::error(calc_value::WorksheetErrorCode::Name) {
        return Err("retained registration ran after unload".into());
    }
    if !native.diagnostics().is_empty() {
        return Err(format!("XLL callback refusals: {:?}", native.diagnostics()));
    }
    println!(
        "{}",
        serde_json::json!({"host":"DNA Calc / OneCalc controller","registrations":registrations.len(),"duplicate_use_count":registrations.iter().find(|r|r.name=="MyAdd").map(|r|r.use_count),"numbered_exports":registrations.iter().filter_map(|r|r.procedure.strip_prefix('f').and_then(|n|n.parse::<u32>().ok())).fold(None,|a:Option<(u32,u32)>,n|Some(a.map(|(lo,hi)|(lo.min(n),hi.max(n))).unwrap_or((n,n)))),"ribbons":ribbons,"results":results,"reload_results":reload_results,"runtime_delay_releases":native.runtime_delay_releases(),"commands":command_results,"live_callback_allocations":native.live_callback_allocations(),"closed_registration":"#NAME?","closed_name":stale_name})
    );
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
                eprintln!("dna-onecalc-desktop: {why}");
                std::process::ExitCode::FAILURE
            }
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("dna-onecalc-desktop XLL hosting requires Windows x64");
        std::process::ExitCode::FAILURE
    }
}
