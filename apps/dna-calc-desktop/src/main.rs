#[cfg(target_arch = "wasm32")]
compile_error!("dna-calc-desktop is native-only");
#[cfg(windows)]
mod engine;
#[cfg(windows)]
mod shell;

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
fn run() -> Result<(), String> {
    use std::path::PathBuf;
    let mut xlls = Vec::new();
    let mut formula = "=MyAdd(1,2)".to_owned();
    let mut self_test = false;
    let mut cases_file = None;
    let mut commands = Vec::new();
    let mut early_commands = Vec::new();
    let mut early_execution = Vec::new();
    let mut reload = false;
    let mut reload_new_host = false;
    let mut engine = false;
    let mut ribbon_ui = false;
    let mut sheet_name = "X1".to_owned();
    let mut workbook_name = "Book1".to_owned();
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
            "--workbook-name" => {
                workbook_name = args.next().ok_or("--workbook-name needs a name")?
            }
            "--early-command" => {
                early_commands.push(args.next().ok_or("--early-command needs a name")?)
            }
            "--early-execution" => {
                early_execution.push(args.next().ok_or("--early-execution needs a name")?)
            }
            "--reload" => reload = true,
            "--engine" => engine = true,
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
                    "dna-calc-desktop --xll PATH [--xll PATH ...] [--formula TEXT | --cases PATH | --self-test] [--sheet NAME] [--workbook-name NAME] [--command NAME] [--early-command NAME] [--engine --early-execution NAME] [--reload]"
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
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
    if workbook_name != "Book1" && !engine {
        return Err("--workbook-name requires --engine (dc-snz.23)".into());
    }
    if xlls.is_empty() {
        return Err("provide --xll PATH".into());
    }
    let mut shell = shell::Shell::new()?;
    if let Some(path) = ready {
        std::fs::write(path, "ready").map_err(|e| e.to_string())?;
    }
    if let Some(path) = wait {
        shell.wait_for(&path)?;
    }
    let mut host = std::rc::Rc::new(calc_xll_host::XllHost::new(shell.hwnd()));
    let mut session = dna_onecalc::Session::new(&sheet_name).map_err(|e| format!("{e:?}"))?;
    for (name, id) in session.sheets() {
        host.set_sheet(id, format!("[{workbook_name}]{name}"), true);
    }
    shell.attach_host(host.clone())?;
    shell.projection()?.set_workbook(&workbook_name)?;
    shell.projection()?.set_sheet(&sheet_name)?;
    for xll in &xlls {
        if let Err(why) = host.load(&xll) {
            return Err(format!(
                "{why}; after rollback: {} active registrations, {} callable names, {} callback allocations",
                host.registrations()
                    .iter()
                    .filter(|r| r.use_count > 0)
                    .count(),
                host.functions()?.names().count(),
                host.live_callback_allocations()
            ));
        }
    }
    if let Some(path) = startup_ack {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while !path.is_file() {
            if std::time::Instant::now() > deadline {
                return Err("add-in startup event acknowledgment expired".into());
            }
            shell.pump_for(std::time::Duration::from_millis(10));
            shell.projection()?.after_calculate()?;
        }
    }
    let registrations = host.registrations();
    if !engine && !early_execution.is_empty() {
        return Err("--early-execution requires --engine".into());
    }
    let early_execution = early_execution
        .iter()
        .map(|name| {
            host.active_registration_id(name)
                .ok_or_else(|| format!("no active early-execution registration: {name}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let ribbons = if ribbon_ui {
        shell.projection()?.ribbons()?
    } else {
        Vec::new()
    };
    if engine {
        let mut output = engine::run(
            &shell,
            &host,
            &sheet_name,
            &workbook_name,
            cases_file,
            formula,
            &early_execution,
        )?;
        output["ribbons"] = serde_json::json!(ribbons);
        shell.projection()?.close_addins()?;
        println!("{output}");
        if let Some(path) = finished {
            std::fs::write(path, "finished").map_err(|e| e.to_string())?;
        }
        if let Some(path) = release {
            shell.wait_for(&path)?;
        }
        return Ok(());
    }
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
    shell
        .projection()?
        .sheet_change(input_cell.row(), input_cell.column())?;
    session.set_user_functions(host.functions()?);
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
                serde_json::json!(text.replacen("<workbook>", &workbook_name, 1))
            }
            _ => case["value"].clone(),
        };
        if let Some(enabled) = case["enable_events"].as_bool() {
            shell.projection()?.enable_events(enabled)?;
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
            shell
                .projection()?
                .sheet_change(cell.row(), cell.column())?;
        }
        session.set_formula(&formula);
        let cell = session.formula_cell();
        shell
            .projection()?
            .sheet_change(cell.row(), cell.column())?;
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
        shell.projection()?.after_calculate()?;
        shell.pump_for(std::time::Duration::from_millis(20));
    }
    let mut command_results = Vec::new();
    for command in commands {
        let value = host.run(&command, &[])?;
        command_results.push(serde_json::json!({"command":command,"value":value_json(&value)}));
        shell.pump_for(std::time::Duration::from_millis(200));
    }
    for command in early_commands {
        let value = shell.run_early(&command)?;
        command_results.push(
            serde_json::json!({"command":command,"value":value_json(&value),"early_bound":true}),
        );
        shell.pump_for(std::time::Duration::from_millis(200));
    }
    let mut reload_results = Vec::new();
    if reload {
        let retained = host.functions()?;
        for xll in xlls.iter().rev() {
            host.unload(xll)?;
        }
        if retained.call(&stale_name, &[])?
            != calc_value::CalcValue::error(calc_value::WorksheetErrorCode::Name)
        {
            return Err("retained registration ran after removal".into());
        }
        session.set_user_functions(host.functions()?);
        drop(retained);
        if reload_new_host {
            host = std::rc::Rc::new(calc_xll_host::XllHost::new(shell.hwnd()));
            for (name, id) in session.sheets() {
                host.set_sheet(id, format!("[{workbook_name}]{name}"), true);
            }
            shell.attach_host(host.clone())?;
            shell.projection()?.set_workbook(&workbook_name)?;
            shell.projection()?.set_sheet(&sheet_name)?;
        }
        for xll in &xlls {
            host.load(xll)?;
        }
        session.set_user_functions(host.functions()?);
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
    let retained = host.functions()?;
    if host.live_callback_allocations() != 0 {
        return Err(format!(
            "{} callback allocations remain before close",
            host.live_callback_allocations()
        ));
    }
    shell.projection()?.close_addins()?;
    if host.live_callback_allocations() != 0 {
        return Err(format!(
            "{} callback allocations remain",
            host.live_callback_allocations()
        ));
    }
    let stale = retained.call(&stale_name, &[])?;
    if stale != calc_value::CalcValue::error(calc_value::WorksheetErrorCode::Name) {
        return Err("retained registration ran after unload".into());
    }
    if !host.diagnostics().is_empty() {
        return Err(format!("XLL callback refusals: {:?}", host.diagnostics()));
    }
    println!(
        "{}",
        serde_json::json!({"host":"DNA Calc / OneCalc controller","registrations":registrations.len(),"duplicate_use_count":registrations.iter().find(|r|r.name=="MyAdd").map(|r|r.use_count),"numbered_exports":registrations.iter().filter_map(|r|r.procedure.strip_prefix('f').and_then(|n|n.parse::<u32>().ok())).fold(None,|a:Option<(u32,u32)>,n|Some(a.map(|(lo,hi)|(lo.min(n),hi.max(n))).unwrap_or((n,n)))),"ribbons":ribbons,"results":results,"reload_results":reload_results,"runtime_delay_releases":host.runtime_delay_releases(),"commands":command_results,"live_callback_allocations":host.live_callback_allocations(),"closed_registration":"#NAME?","closed_name":stale_name})
    );
    if let Some(path) = finished {
        std::fs::write(path, "finished").map_err(|e| e.to_string())?;
    }
    if let Some(path) = release {
        shell.wait_for(&path)?;
    }
    Ok(())
}

#[cfg(windows)]
fn value_json(value: &calc_value::CalcValue) -> serde_json::Value {
    use calc_value::CoreRef;
    match value.core_ref() {
        CoreRef::Number(n) => serde_json::json!(n),
        CoreRef::Logical(b) => serde_json::json!(b),
        CoreRef::Text(t) => serde_json::json!(String::from_utf16_lossy(t.utf16_code_units())),
        CoreRef::Error(e) => serde_json::json!({"error":e.excel_spelling()}),
        CoreRef::Empty => serde_json::Value::Null,
        other => serde_json::json!({"value":format!("{other:?}")}),
    }
}
fn main() -> std::process::ExitCode {
    // Set before this process creates threads or loads any add-in. Older Excel-DNA
    // releases can ignore this opt-in; the executable retains its own name.
    #[cfg(windows)]
    unsafe {
        std::env::set_var("EXCELDNA_ASSUME_EXCEL_PROCESS", "1");
    }
    #[cfg(windows)]
    {
        match run() {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(why) => {
                eprintln!("dna-calc-desktop: {why}");
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
