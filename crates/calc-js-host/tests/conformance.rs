//! The synchronous conformance cases use the public adapter and run against each backend.
//! Native-only: QuickJS is the first backend. Future backends run these cases with their factory.

#![cfg(all(not(target_arch = "wasm32"), feature = "engine-quickjs"))]

use calc_js_host::{ExecLimits, FunctionsMetadata, JsAddin, JsEngine, JsHostError, QuickJs};
use calc_value::{ArrayShape, CalcArray, CalcValue as V, ExcelText, WorksheetErrorCode as W};

fn engine(limits: ExecLimits) -> Box<dyn JsEngine> {
    Box::new(QuickJs::new(limits))
}

fn load(
    source: &str,
    functions: serde_json::Value,
    allow_errors: bool,
    limits: ExecLimits,
) -> Result<JsAddin, JsHostError> {
    let metadata = FunctionsMetadata::parse(
        "Probe",
        &serde_json::json!({
            "functions": functions,
            "allowErrorForDataTypeAny": allow_errors,
        })
        .to_string(),
    )?;
    JsAddin::load(engine(limits), "probe", source, &metadata)
}

fn call(addin: &JsAddin, name: &str, args: &[V]) -> Result<V, String> {
    let table = addin.callables().clone();
    table
        .resolve(table.bind(None, name).unwrap())
        .unwrap()
        .call(args)
}

fn matrix(rows: usize, cols: usize, cells: Vec<V>) -> V {
    V::array(CalcArray::new(ArrayShape { rows, cols }, cells).unwrap())
}

#[test]
fn registers_office_ids_and_publishes_descriptors() {
    let addin = load(
        "let n = 0; CustomFunctions.associate('COUNTER', () => ++n);",
        serde_json::json!([{
            "id":"COUNTER", "name":"NEXT", "description":"Next number",
            "parameters":[], "result":{}, "options":{"volatile":true}
        }]),
        false,
        ExecLimits::default(),
    )
    .unwrap();
    let table = addin.callables().clone();
    let descriptor = table.descriptor(None, "probe.next").unwrap();
    assert_eq!(descriptor.signature(), "Probe.NEXT()");
    assert_eq!(descriptor.description, "Next number");
    assert!(descriptor.volatile);
    assert!(table.bind(None, "PROBE.COUNTER").is_none());
    assert_eq!(call(&addin, "probe.next", &[]).unwrap(), V::number(1.));
    assert_eq!(call(&addin, "probe.next", &[]).unwrap(), V::number(2.));
    drop(addin);
    assert!(
        table
            .resolve(table.bind(None, "probe.next").unwrap())
            .unwrap()
            .call(&[])
            .is_err()
    );
}

#[test]
fn value_round_trip_preserves_matrix_order_utf16_and_signed_zero() {
    let addin = load(
        "CustomFunctions.associate('ECHO', a => a);",
        serde_json::json!([{
            "id":"ECHO", "name":"ECHO",
            "parameters":[{"name":"a", "type":"any", "dimensionality":"matrix"}],
            "result":{"dimensionality":"matrix"}
        }]),
        true,
        ExecLimits::default(),
    )
    .unwrap();
    let text = V::text(ExcelText::from_utf16_code_units(vec![
        0x41, 0, 0xd800, 0xd83d, 0xde80,
    ]));
    let input = matrix(
        2,
        3,
        vec![
            V::number(-0.0),
            V::logical(true),
            text,
            V::empty(),
            V::error(W::NA),
            V::number(f64::from_bits(1)),
        ],
    );
    let output = call(&addin, "Probe.ECHO", &[input.clone()]).unwrap();
    assert_eq!(output, input);
    let cells = output.as_array().unwrap().row_slice(0).unwrap();
    assert_eq!(
        cells[0].as_number().unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(
        output
            .as_array()
            .unwrap()
            .get(1, 2)
            .unwrap()
            .as_number()
            .unwrap()
            .to_bits(),
        1
    );
}

#[test]
fn packs_optional_repeating_and_matrix_parameters() {
    let addin = load(
        "CustomFunctions.associate('ARGS', (a, b, c, invocation) => [[a[0][0], b, c.length, c[1], typeof invocation]]);",
        serde_json::json!([{
            "id":"ARGS", "name":"ARGS",
            "parameters":[{"name":"a","type":"number","dimensionality":"matrix"},
                {"name":"b","optional":true}, {"name":"c","repeating":true}],
            "result":{"dimensionality":"matrix"}
        }]),
        false,
        ExecLimits::default(),
    ).unwrap();
    let output = call(
        &addin,
        "Probe.ARGS",
        &[V::number(9.), V::missing(), V::number(1.), V::logical(true)],
    )
    .unwrap();
    assert_eq!(
        output,
        matrix(
            1,
            5,
            vec![
                V::number(9.),
                V::empty(),
                V::number(2.),
                V::logical(true),
                V::text(ExcelText::from_utf16_code_units(
                    "object".encode_utf16().collect()
                ))
            ]
        )
    );
}

#[test]
fn handles_nonfinite_numbers_error_values_and_script_exceptions() {
    let addin = load(
        "CustomFunctions.associate('RUN', n => {
            if (n === 0) return NaN;
            if (n === 1) return Infinity;
            if (n === 2) throw new CustomFunctions.Error(CustomFunctions.ErrorCode.divisionByZero);
            if (n === 3) return new CustomFunctions.Error(CustomFunctions.ErrorCode.invalidValue);
            throw new Error('failed');
        });",
        serde_json::json!([{"id":"RUN","name":"RUN","parameters":[{"name":"n","type":"number"}],"result":{}}]),
        false,
        ExecLimits::default(),
    ).unwrap();
    for (n, error) in [(0., W::Num), (1., W::Num), (2., W::Div0), (3., W::Value)] {
        assert_eq!(
            call(&addin, "Probe.RUN", &[V::number(n)]).unwrap(),
            V::error(error)
        );
    }
    assert!(call(&addin, "Probe.RUN", &[V::number(4.)]).is_err());
    assert_eq!(
        call(&addin, "Probe.RUN", &[V::error(W::Ref)]).unwrap(),
        V::error(W::Ref)
    );
}

#[test]
fn limits_module_and_function_execution_and_recovers() {
    use std::time::Duration;
    let limits = ExecLimits {
        time: Duration::from_millis(100),
        heap_bytes: 2 * 1024 * 1024,
    };
    let functions =
        serde_json::json!([{"id":"RUN","name":"RUN","parameters":[{"name":"n"}],"result":{}}]);
    assert!(matches!(
        load("for (;;) {}", functions.clone(), false, limits),
        Err(JsHostError::Timeout)
    ));
    let addin = load(
        "CustomFunctions.associate('RUN', n => {
            if (n === 1) for (;;) {}
            if (n === 2) return new ArrayBuffer(8 * 1024 * 1024).byteLength;
            return typeof fetch === 'undefined' && typeof Excel === 'undefined' &&
                typeof require === 'undefined' && typeof process === 'undefined';
        });",
        functions,
        false,
        limits,
    )
    .unwrap();
    assert!(
        call(&addin, "Probe.RUN", &[V::number(1.)])
            .unwrap_err()
            .contains("time")
    );
    assert!(call(&addin, "Probe.RUN", &[V::number(2.)]).is_err());
    assert_eq!(
        call(&addin, "Probe.RUN", &[V::number(0.)]).unwrap(),
        V::logical(true)
    );
}

#[test]
fn refuses_deferred_shapes_and_unassociated_functions() {
    let metadata = |options| serde_json::json!([{"id":"RUN","name":"RUN","parameters":[],"result":{},"options":options}]);
    assert!(
        load(
            "",
            metadata(serde_json::json!({})),
            false,
            ExecLimits::default()
        )
        .is_err()
    );
    let stream = load(
        "CustomFunctions.associate('RUN', () => 0);",
        metadata(serde_json::json!({"stream":true})),
        false,
        ExecLimits::default(),
    )
    .unwrap();
    assert!(
        call(&stream, "Probe.RUN", &[])
            .unwrap_err()
            .contains("dc-26wtr.7")
    );
    let promise = load(
        "CustomFunctions.associate('RUN', async () => 0);",
        metadata(serde_json::json!({})),
        false,
        ExecLimits::default(),
    )
    .unwrap();
    assert!(
        call(&promise, "Probe.RUN", &[])
            .unwrap_err()
            .contains("dc-26wtr.6")
    );
}

#[test]
fn matrix_results_must_be_rectangular_and_nonrecursive() {
    let addin = load(
        "CustomFunctions.associate('RUN', n => {
            if (n === 0) return [];
            if (n === 1) return [[1], [2, 3]];
            const a = [[]]; a[0].push(a); return a;
        });",
        serde_json::json!([{"id":"RUN","name":"RUN","parameters":[{"name":"n"}],"result":{"dimensionality":"matrix"}}]),
        false,
        ExecLimits::default(),
    ).unwrap();
    assert_eq!(
        call(&addin, "Probe.RUN", &[V::number(0.)]).unwrap(),
        V::error(W::Calc)
    );
    for n in [1., 2.] {
        assert!(call(&addin, "Probe.RUN", &[V::number(n)]).is_err());
    }
}

#[test]
fn worksheet_matrices_are_ordinary_javascript_arrays() {
    let addin = load(
        "CustomFunctions.associate('RUN', a => {
            a[0].push(a[0][0]);
            a[0][0] = Object.keys(a[0]).length;
            return a;
        });",
        serde_json::json!([{
            "id":"RUN","name":"RUN",
            "parameters":[{"name":"a","type":"number","dimensionality":"matrix"}],
            "result":{"dimensionality":"matrix"}
        }]),
        false,
        ExecLimits::default(),
    )
    .unwrap();
    assert_eq!(
        call(&addin, "Probe.RUN", &[V::number(7.)]).unwrap(),
        matrix(1, 2, vec![V::number(2.), V::number(7.)])
    );
}
