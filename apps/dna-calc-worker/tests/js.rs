use calc_host::ProjectedValue;
use calc_value::{CellKey, WorksheetErrorCode};
use dna_calc_model::MacroSecurity;
use dna_calc_worker::{
    server::Server,
    wire::{Intent, Key, Request},
};

/// Native-only: QuickJS is the embedded native backend.
#[test]
fn office_js_uses_the_worker_registry_and_recalculates_when_unloaded() {
    let mut server = Server::new();
    let sheet = server.book().host().sheets()[0].id();
    let addin = calc_addin_host::load_js_addin(
        MacroSecurity::Guarded,
        "probe.js",
        "Probe",
        "CustomFunctions.associate('SUM', a => a[0][0] + a[1][0]);",
        r#"{"functions":[{"id":"SUM","name":"SUM",
            "parameters":[{"name":"a","type":"number","dimensionality":"matrix"}],
            "result":{}
        }]}"#,
    )
    .unwrap();
    server.set_user_functions(addin.callables().clone());
    let cell = |row, column| CellKey::new(sheet, row, column).unwrap();
    let formula = cell(1, 2);
    for (at, text) in [
        (cell(1, 1), "17"),
        (cell(2, 1), "25"),
        (formula, "=Probe.SUM(A1:A2)"),
    ] {
        server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(at),
                    text: text.into(),
                }),
                None,
            )
            .reply
            .unwrap();
    }
    let settle = |server: &mut Server| {
        for _ in 0..1000 {
            if !server.has_pending_calculation() {
                return;
            }
            server.advance_calculation();
        }
        panic!("worker did not settle");
    };
    settle(&mut server);
    let result = server.book().project(vec![formula]);
    assert!(
        matches!(result[0].value(), ProjectedValue::Calculated { value, .. }
            if value.as_number() == Some(42.)),
        "{:?}",
        result[0].value()
    );
    drop(addin);
    server.set_user_functions(calc_host::CallableTable::new());
    settle(&mut server);
    let result = server.book().project(vec![formula]);
    assert!(
        matches!(result[0].value(), ProjectedValue::Calculated { value, .. }
            if value.as_error() == Some(WorksheetErrorCode::Name)),
        "{:?}",
        result[0].value()
    );
}
