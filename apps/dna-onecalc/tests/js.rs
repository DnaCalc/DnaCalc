use calc_func::value::{CalcValue, ExcelText};
use calc_macro_security::MacroSecurity;
use dna_onecalc::{InputValue, Session};

/// Native-only: QuickJS is the embedded native backend.
#[test]
fn office_js_receives_invocation_addresses_through_the_formula_explorer() {
    let mut session = Session::new("Bob's Region").unwrap();
    session.set_formula_cell("H1").unwrap();
    session
        .set_input("A1", InputValue::number(42.).unwrap())
        .unwrap();
    let addin = calc_addin_host::load_js_addin(
        MacroSecurity::Guarded,
        "probe.js",
        "Probe",
        "CustomFunctions.associate('ADDRESS', (value, invocation) => [[
            invocation.address, invocation.parameterAddresses[0], value
        ]]);",
        r#"{"functions":[{"id":"ADDRESS","name":"ADDRESS",
            "parameters":[{"name":"value","type":"number"}],
            "options":{"requiresAddress":true,"requiresParameterAddresses":true},
            "result":{"dimensionality":"matrix"}
        }]}"#,
    )
    .unwrap();
    session.set_user_functions(addin.callables().clone());
    let calculation = session.calculate_text("=Probe.ADDRESS(A1)");
    let result = calculation
        .result
        .unwrap_or_else(|| panic!("{:?}", calculation.diagnostics));
    let expected = |text| CalcValue::text(ExcelText::from_interop_assignment(text));
    assert_eq!(
        result.as_array().unwrap().row_slice(0).unwrap(),
        &[
            expected("'Bob''s Region'!H1"),
            expected("'Bob''s Region'!A1"),
            CalcValue::number(42.),
        ]
    );
}
