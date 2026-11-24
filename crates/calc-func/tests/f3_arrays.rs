//! F3 (dc-728): the dynamic-array functions and the LAMBDA helpers called through
//! `calc_func::invoke` alone: a reference argument read as the array of its cells, and the
//! refusal of a LAMBDA helper whose services carry no caller of LAMBDA values. The formula route
//! (calc-fml's evaluator, which supplies that caller) is tested in calc-fml's `acceptance/f3.rs`.

mod f1_support;

use calc_func::value::WorksheetErrorCode::{Calc, NA, Num, Value};
use f1_support::*;

/// Excel 20527 floors positive fractional dimensions and advances each result by one addition.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn sequence_fractional_dimensions_and_accumulation() {
    let values = [
        0x4019_528e_c053_3b42, 0x4019_1f0a_fdaf_66dc,
        0x417c_c35e_d987_30f8, 0xc183_ecbe_a9df_3a4b,
    ].map(f64::from_bits);
    let (rows, cols, cells) = array(call("SEQUENCE", values.into_iter().map(n).collect()));
    assert_eq!((rows, cols), (6, 6));
    assert_eq!(cells[6].to_bits(), 0xc1aa_4ab2_239d_f152);
    assert_eq!(cells[13].to_bits(), 0xc1be_947f_e672_4ba9);
    assert_eq!(cells[20].to_bits(), 0xc1c8_01d3_5d8a_cf57);
    error(call("SEQUENCE", vec![n(0.0), n(-0.1)]), Value);
    error(call("SEQUENCE", vec![b(false), n(-0.1)]), Value);
    error(call("SEQUENCE", vec![n(0.5), n(1.0)]), Calc);
    error(call("SEQUENCE", vec![n(0.5), t("bad")]), Value);
    error(call("SEQUENCE", vec![n(0.5), n(1.0), t("bad")]), Value);
    error(call("SEQUENCE", vec![n(0.5), n(1.0), n(1.0), t("bad")]), Value);
    assert_eq!(array(call("SEQUENCE", vec![n(f64::from_bits(1.0_f64.to_bits() - 1))])), (1, 1, vec![1.0]));
    error(call("SEQUENCE", vec![n(0.0), e(NA)]), NA);
    let blank = calc_func::value::CalcValue::empty();
    error(call("SEQUENCE", vec![blank.clone(), n(2.1)]), Calc);
    assert_eq!(array(call("SEQUENCE", vec![n(2.1), n(1.0), blank.clone(), n(2.0)])), (2, 1, vec![0.0, 2.0]));
    assert_eq!(array(call("SEQUENCE", vec![n(2.1), n(1.0), n(4.0), blank])), (2, 1, vec![4.0, 4.0]));
    assert_eq!(array(call("SEQUENCE", vec![n(2.1), n(1.0), missing(), missing()])), (2, 1, vec![1.0, 2.0]));
    // A yielded application call must resume the same accumulation and retain its overflow cells.
    use calc_func::{catalogue, invoke_slice, EvalServices, FunctionCallScratch, Notation};
    use calc_func::value::NumericMode;
    let target = catalogue().resolve_surface(Notation::Call, "SEQUENCE").unwrap();
    let mode = NumericMode::Fast;
    let mut services = EvalServices::new(&mode);
    let mut scratch = FunctionCallScratch::new();
    let mut arguments = Given(values.into_iter().map(n).collect());
    assert_eq!(invoke_slice(target, &mut arguments, &mut services, &mut scratch, 5), Ok(None));
    let resumed = invoke_slice(target, &mut arguments, &mut services, &mut scratch, 100)
        .unwrap().unwrap();
    assert_eq!(resumed.as_array().unwrap().get(1, 0).unwrap().as_number().unwrap().to_bits(), 0xc1aa_4ab2_239d_f152);
    let overflow_args = vec![n(2.1), n(3.1), n(1e308), n(1e308)];
    let expected = arr_values(vec![
        vec![n(1e308), e(Num), e(Num)],
        vec![e(Num), e(Num), e(Num)],
    ]);
    assert_eq!(call("SEQUENCE", overflow_args.clone()), Ok(expected.clone()));
    let mut overflow = Given(overflow_args);
    let mut scratch = FunctionCallScratch::new();
    assert_eq!(invoke_slice(target, &mut overflow, &mut services, &mut scratch, 1), Ok(None));
    assert_eq!(
        invoke_slice(target, &mut overflow, &mut services, &mut scratch, 5),
        Ok(Some(expected))
    );
}

/// Excel 20527 stores each SEQUENCE step, including across rows and resumption.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn sequence_integer_dimensions_accumulate_in_spill_order() {
    use calc_func::value::NumericMode;
    use calc_func::{EvalServices, FunctionCallScratch, Notation, catalogue, invoke_slice};

    let args = vec![
        n(7.0),
        n(5.0),
        n(f64::from_bits(0xc16d_e79b_284c_2c68)),
        n(f64::from_bits(0xc18e_44bb_1fe3_3456)),
    ];
    let (rows, cols, cells) = array(call("SEQUENCE", args));
    assert_eq!((rows, cols), (7, 5));
    assert_eq!(cells[5].to_bits(), 0xc1b3_da31_cd30_621a);

    let target = catalogue().resolve_surface(Notation::Call, "SEQUENCE").unwrap();
    let mut services = EvalServices::new(&NumericMode::Fast);
    let mut scratch = FunctionCallScratch::new();
    let mut overflow = Given(vec![n(2.0), n(3.0), n(-1e308), n(1e308)]);
    assert_eq!(invoke_slice(target, &mut overflow, &mut services, &mut scratch, 2), Ok(None));
    assert_eq!(
        invoke_slice(target, &mut overflow, &mut services, &mut scratch, 4),
        Ok(Some(arr_values(vec![
            vec![n(-1e308), n(0.0), n(1e308)],
            vec![e(Num), e(Num), e(Num)],
        ])))
    );

    let minimum = f64::MIN_POSITIVE;
    let next_normal = f64::from_bits(minimum.to_bits() + 1);
    let (_, _, cells) = array(call("SEQUENCE", vec![n(2.0), n(3.0), n(next_normal), n(-minimum)]));
    assert_eq!(cells[1].to_bits(), 1);
    assert_eq!(cells[2].to_bits(), 0x800f_ffff_ffff_ffff);
    assert_eq!(array(call("SEQUENCE", vec![n(1.0), n(1.0), n(1e308), n(1e308)])), (1, 1, vec![1e308]));
}

/// Excel 16.0.20527 distinguishes blank optional cells from omitted SORT
/// arguments and propagates a scalar source error before an invalid index.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn sort_blank_arguments_and_scalar_source_errors() {
    use calc_func::value::{CalcArray, CalcValue};
    let input = arr(vec![vec![3.0, 1.0], vec![2.0, 4.0]]);
    let blank = CalcValue::empty();
    error(
        call("SORT", vec![input.clone(), blank.clone(), n(1.0)]),
        Value,
    );
    error(
        call("SORT", vec![input.clone(), n(1.0), blank.clone()]),
        Value,
    );
    let blank_array = CalcValue::array(CalcArray::from_rows(vec![vec![blank]]).unwrap());
    error(
        call("SORT", vec![input.clone(), blank_array, n(1.0)]),
        Value,
    );
    assert_eq!(
        array(call("SORT", vec![input, missing(), n(1.0)])),
        (2, 2, vec![2.0, 4.0, 3.0, 1.0])
    );
    error(call("SORT", vec![e(NA), n(2.0), n(1.0)]), NA);
    let held_error = CalcValue::array(CalcArray::from_rows(vec![vec![e(NA)]]).unwrap());
    error(call("SORT", vec![held_error, n(2.0), n(1.0)]), Value);
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![e(NA), n(3.0)]);
    let one = sheet.area(1, 1, 1, 1);
    let many = sheet.area(1, 1, 2, 1);
    error(call_on(&mut sheet, "SORT", vec![one, n(2.0), n(1.0)]), NA);
    error(
        call_on(&mut sheet, "SORT", vec![many, n(2.0), n(1.0)]),
        Value,
    );
}

/// Excel 20527 merges adjacent ascending runs in pairs, taking the left row
/// when the current keys compare equal. These cases distinguish the merge order.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn sort_merges_adjacent_ascending_runs_in_pairs() {
    use calc_func::value::{CalcArray, CalcValue};

    for (keys, order, expected_tags) in [
        (["aー", "a-ー", "a-a", "aa"], 1.0, [1.0, 3.0, 2.0, 0.0]),
        (["aa", "a-a", "a-ー", "aa"], -1.0, [1.0, 0.0, 3.0, 2.0]),
    ] {
        let rows = keys
            .iter()
            .enumerate()
            .map(|(row, key)| vec![t(key), n(row as f64)])
            .collect();
        let input = CalcValue::array(CalcArray::from_rows(rows).unwrap());
        let result = call("SORT", vec![input, n(1.0), n(order)]).unwrap();
        let result = result.as_array().unwrap();
        let actual_tags = std::array::from_fn(|row| result.get(row, 1).unwrap().as_number().unwrap());
        assert_eq!(actual_tags, expected_tags);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn references_are_read_as_arrays() {
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![n(3.0), n(1.0), n(2.0)]);
    let range = sheet.area(1, 1, 3, 1);
    assert_eq!(
        array(call_on(&mut sheet, "SORT", vec![range.clone()])),
        (3, 1, vec![1.0, 2.0, 3.0])
    );
    // TAKE and DROP of a range give the reference to the cells they keep; of an array, the array.
    let kept = call_on(&mut sheet, "TAKE", vec![range.clone(), n(-2.0)]).expect("a reference");
    assert_eq!(kept, sheet.area(2, 1, 3, 1));
    let dropped = call_on(&mut sheet, "DROP", vec![range.clone(), n(1.0)]).expect("a reference");
    assert_eq!(dropped, sheet.area(2, 1, 3, 1));
    error(
        call_on(&mut sheet, "DROP", vec![range.clone(), n(3.0)]),
        Calc,
    );
    assert_eq!(
        array(call("TAKE", vec![col(&[3.0, 1.0, 2.0]), n(-2.0)])),
        (2, 1, vec![1.0, 2.0])
    );
    assert_eq!(
        array(call_on(&mut sheet, "TOROW", vec![range.clone()])),
        (1, 3, vec![3.0, 1.0, 2.0])
    );
    // A blank cell of the range is an empty value, which TOCOL's ignore 1 leaves out.
    let wider = sheet.area(1, 1, 4, 1);
    assert_eq!(
        array(call_on(&mut sheet, "TOCOL", vec![wider, n(1.0)])),
        (3, 1, vec![3.0, 1.0, 2.0])
    );
    error(
        call_on(
            &mut sheet,
            "FILTER",
            vec![range.clone(), arr(vec![vec![0.0], vec![0.0], vec![0.0]])],
        ),
        Calc,
    );
    assert_eq!(
        array(call("SEQUENCE", vec![n(2.0), n(2.0), n(0.0), n(0.5)])),
        (2, 2, vec![0.0, 0.5, 1.0, 1.5])
    );
    assert_excel_capture(
        call("SEQUENCE", vec![n(-1.0)]),
        include_bytes!("captures/jnt/sections/jnt_aggregate_arrays.capture.json"),
        "jnt_aggregate_arrays",
        75,
        4,
    );
    // Direct invocation leaves optional-array lifting to calc-fml even when the range
    // argument is a scalar error. It must keep that typed refusal.
    refused(call(
        "TRIMRANGE",
        vec![
            calc_func::value::CalcValue::error(calc_func::value::WorksheetErrorCode::NA),
            arr(vec![vec![0.0, 1.0]]),
        ],
    ));
}

/// A callable value of one parameter whose identity no evaluator knows.
#[derive(Debug)]
struct Opaque;

impl calc_func::value::OpaqueCallable for Opaque {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn a_lambda(params: usize) -> calc_func::value::CalcValue {
    calc_func::value::CalcValue::callable(calc_func::value::CallableValue::new(
        calc_func::value::CallableArityShape::exact(params),
        "LAMBDA",
        std::sync::Arc::new(Opaque),
    ))
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_lambda_helper_without_a_caller_is_refused() {
    refused(call("MAP", vec![arr(vec![vec![1.0]]), a_lambda(1)]));
    refused(call("MAKEARRAY", vec![n(1.0), n(1.0), a_lambda(2)]));
    // What the helper decides before any call stays a worksheet error: a value that is no LAMBDA,
    // a LAMBDA of the wrong arity, a dimension that is not a positive integer.
    error(call("MAP", vec![arr(vec![vec![1.0]]), n(5.0)]), Value);
    error(
        call("REDUCE", vec![n(0.0), arr(vec![vec![1.0]]), a_lambda(1)]),
        Value,
    );
    error(call("MAKEARRAY", vec![n(0.0), n(1.0), a_lambda(2)]), Value);
}

/// dc-ztvv.26.34..43 (cb28 S1d/S2c/S3b): INDEX applies `date_integer_floor`
/// (`T23` at `ceil <= 0`, `T22` at `ceil >= 1`) to `row_num`, `column_num`, and
/// `area_num` before checking `< 0`, `< 1`, or bounds.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_ztvv_26_34_index_selectors_use_date_integer_floor() {
    use calc_func::value::WorksheetErrorCode::Ref;
    let one = arr(vec![vec![42.0]]);
    let two_by_two = arr(vec![vec![10.0, 20.0], vec![30.0, 40.0]]);
    let f = f64::from_bits;

    // -T23 boundary (ceil == 0): -2^-1022 and -T23 snap to 0; -T23 - 1 ULP is #VALUE!.
    assert_eq!(
        array(call("INDEX", vec![one.clone(), n(f(0x8010_0000_0000_0000))])),
        (1, 1, vec![42.0])
    );
    assert_eq!(
        array(call("INDEX", vec![one.clone(), n(f(0xbe80_0200_0000_0000))])),
        (1, 1, vec![42.0])
    );
    error(call("INDEX", vec![one.clone(), n(f(0xbe80_0200_0000_0001))]), Value);

    // Pinned fuzz reps: row=-2^-1022-3*2^-1074 snaps to 0 (whole column / first cell / area>1 #REF!).
    let col3 = arr(vec![vec![-10.0], vec![2.0], vec![2.0]]);
    assert_eq!(
        array(call("INDEX", vec![col3.clone(), n(f(0x8010_0000_0000_0003))])),
        (3, 1, vec![-10.0, 2.0, 2.0])
    );
    error(
        call(
            "INDEX",
            vec![col3, n(f(0x8010_0000_0000_0003)), n(1.0), n(2.0)],
        ),
        Ref,
    );

    // T22 boundary (ceil >= 1) on row_num, column_num, and area_num:
    exact(
        call(
            "INDEX",
            vec![two_by_two.clone(), n(f(0x3fff_ffff_bff8_0000)), n(2.0)],
        ),
        40.0,
    );
    exact(
        call(
            "INDEX",
            vec![two_by_two.clone(), n(2.0), n(f(0x3fff_ffff_bff8_0000))],
        ),
        40.0,
    );
    error(
        call(
            "INDEX",
            vec![two_by_two, n(f(0x4007_ffff_dffc_0000)), n(1.0)],
        ),
        Ref,
    );
    exact(
        call(
            "INDEX",
            vec![one.clone(), n(1.0), n(1.0), n(f(0x3fef_ffff_7ff0_0000))],
        ),
        42.0,
    );
    error(
        call(
            "INDEX",
            vec![one, n(1.0), n(1.0), n(f(0x3fef_ffff_7fef_ffff))],
        ),
        Value,
    );
}
