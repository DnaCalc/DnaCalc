#[cfg(target_arch = "wasm32")]
compile_error!("dna-calc-desktop is native-only");
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
    let mut wait = None;
    let mut ready = None;
    let mut finished = None;
    let mut release = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--xll" => xlls.push(PathBuf::from(args.next().ok_or("--xll needs a path")?)),
            "--formula" => formula = args.next().ok_or("--formula needs text")?,
            "--self-test" => self_test = true,
            "--ready" => ready = Some(PathBuf::from(args.next().ok_or("--ready needs a path")?)),
            "--wait" => wait = Some(PathBuf::from(args.next().ok_or("--wait needs a path")?)),
            "--finished" => {
                finished = Some(PathBuf::from(args.next().ok_or("--finished needs a path")?))
            }
            "--release" => {
                release = Some(PathBuf::from(args.next().ok_or("--release needs a path")?))
            }
            "--help" => {
                println!("dna-calc-desktop --xll PATH [--formula '=MyAdd(1,2)'] [--self-test]");
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }
    if xlls.is_empty() {
        return Err("provide --xll PATH".into());
    }
    let shell = shell::Shell::new()?;
    if let Some(path) = ready {
        std::fs::write(path, "ready").map_err(|e| e.to_string())?;
    }
    if let Some(path) = wait {
        shell.wait_for(&path)?;
    }
    let host = calc_xll_host::XllHost::new(shell.hwnd());
    for xll in xlls {
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
    let registrations = host.registrations();
    let mut session = dna_onecalc::Session::new("X1").map_err(|e| format!("{e:?}"))?;
    session
        .set_formula_cell("B3")
        .map_err(|e| format!("{e:?}"))?;
    session
        .set_input(
            "A1",
            dna_onecalc::InputValue::number(7.).map_err(|e| format!("{e:?}"))?,
        )
        .map_err(|e| format!("{e:?}"))?;
    session.set_user_functions(host.functions()?);
    let tests: Vec<(String, serde_json::Value)> = if self_test {
        let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../../crates/calc-xll-reference/fixture/x1-cases.json"
        ))
        .map_err(|e| e.to_string())?;
        cases
            .into_iter()
            .map(|c| {
                (
                    c["formula"].as_str().unwrap().to_owned(),
                    c["value"].clone(),
                )
            })
            .collect()
    } else {
        vec![(formula, serde_json::Value::Null)]
    };
    let mut results = Vec::new();
    for (formula, expected) in tests {
        session.set_formula(&formula);
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
        if self_test && value != expected {
            return Err(format!(
                "{formula}: expected {expected}, calculated {value}"
            ));
        }
        results.push(serde_json::json!({"formula":formula,"value":value}));
    }
    let retained = host.functions()?;
    if host.live_callback_allocations() != 0 {
        return Err(format!(
            "{} callback allocations remain before close",
            host.live_callback_allocations()
        ));
    }
    host.close()?;
    if host.live_callback_allocations() != 0 {
        return Err(format!(
            "{} callback allocations remain",
            host.live_callback_allocations()
        ));
    }
    let stale = retained.call(
        "MyAdd",
        &[
            calc_value::CalcValue::number(1.),
            calc_value::CalcValue::number(2.),
        ],
    )?;
    if stale != calc_value::CalcValue::error(calc_value::WorksheetErrorCode::Name) {
        return Err("retained registration ran after unload".into());
    }
    if !host.diagnostics().is_empty() {
        return Err(format!("XLL callback refusals: {:?}", host.diagnostics()));
    }
    println!(
        "{}",
        serde_json::json!({"host":"DNA Calc / OneCalc controller","registrations":registrations.len(),"duplicate_use_count":registrations.iter().find(|r|r.name=="MyAdd").map(|r|r.use_count),"results":results,"live_callback_allocations":host.live_callback_allocations(),"closed_registration":"#NAME?"})
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
