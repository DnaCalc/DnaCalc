//! Native-only: a desktop command-line use of the same OneCalc controller as the browser UI.
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), String> {
    use dna_onecalc::{InputValue, Session};
    let args: Vec<String> = std::env::args().collect();
    let formula = args.get(1).map(String::as_str).unwrap_or("=Module1.Add2(A1,2)");
    let source = match args.get(2) {
        Some(path) => std::fs::read_to_string(path).map_err(|e| e.to_string())?,
        None => calc_vba::EXAMPLE_MODULE.to_owned(),
    };
    let mut session = Session::new("Sheet1").map_err(|e| format!("{e:?}"))?;
    session
        .set_formula_cell("H1")
        .map_err(|e| format!("{e:?}"))?;
    session
        .set_input(
            "A1",
            InputValue::number(40.0).map_err(|e| format!("{e:?}"))?,
        )
        .map_err(|e| format!("{e:?}"))?;
    session.load_vba_module("Module1", &source, calc_vba::DEFAULT_STEP_BUDGET)?;
    session.set_formula(formula);
    let calculation = session.calculate();
    match &calculation.result {
        Some(value) => println!("{}", dna_onecalc::display::value(value)),
        None => return Err(format!("{:?}", calculation.diagnostics)),
    }
    for diagnostic in &calculation.diagnostics {
        eprintln!("{}", dna_onecalc::display::diagnostic(&session, diagnostic));
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {}
