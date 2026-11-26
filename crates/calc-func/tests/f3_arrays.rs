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

/// dc-j9k.110, dc-j9k.111, and cb28 S1d/S2b/S3b/S3c:
/// - VLOOKUP/HLOOKUP floor `col_index_num`/`row_index_num` with `date_integer_floor`
///   (T22 at >=1 and flooring -2^31 + eps to -2^31 => #REF!).
/// - VLOOKUP/HLOOKUP/MATCH accept a reference at arg[2]:
///   - VLOOKUP/HLOOKUP defer dereferencing a cell reference until after a match (miss => #N/A),
///     and map cell errors/non-numeric text to #REF! (while inline errors/text stay #DIV/0!/#VALUE!).
///   - MATCH dereferences a cell reference eagerly, mapping multi-cell/error/text to #REF! and
///     blank to 0 (exact match), without snapping `-T23` to 0.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_j9k_110_lookup_index_reference_coercion_and_date_integer_floor() {
    use calc_func::value::CalcValue;
    use calc_func::value::WorksheetErrorCode::{Div0, Ref};
    let f = f64::from_bits;
    let blank = CalcValue::empty();

    // Pinned fuzz reps dc-j9k.110 and dc-j9k.111 (-2^31 + eps floors to -2^31 => #REF!):
    error(
        call(
            "HLOOKUP",
            vec![
                blank.clone(),
                arr(vec![vec![-1.0]]),
                n(f(0xc1df_ffff_ffff_fbfe)),
            ],
        ),
        Ref,
    );
    error(
        call(
            "HLOOKUP",
            vec![
                blank.clone(),
                arr(vec![vec![-0.5]]),
                n(f(0xc1df_ffff_ffff_ffff)),
                b(true),
            ],
        ),
        Ref,
    );

    // VLOOKUP / HLOOKUP 1-ULP T22 lock at 1 - T22 (in => col/row 1; out => 0 => #VALUE!):
    let vtbl = arr(vec![vec![-500.0, 10.0], vec![100.0, 20.0], vec![300.0, 30.0]]);
    exact(
        call("VLOOKUP", vec![n(150.0), vtbl.clone(), n(f(0x3fef_ffff_7ff0_0000))]),
        100.0,
    );
    error(
        call("VLOOKUP", vec![n(150.0), vtbl.clone(), n(f(0x3fef_ffff_7fef_ffff))]),
        Value,
    );
    let htbl = arr(vec![vec![-500.0, 100.0, 300.0], vec![10.0, 20.0, 30.0]]);
    exact(
        call("HLOOKUP", vec![n(150.0), htbl.clone(), n(f(0x3fef_ffff_7ff0_0000))]),
        100.0,
    );
    error(
        call("HLOOKUP", vec![n(150.0), htbl.clone(), n(f(0x3fef_ffff_7fef_ffff))]),
        Value,
    );

    // Inline error/text vs cell-reference error/text at arg[2] in VLOOKUP, HLOOKUP, MATCH:
    let mut sheet = Sheet::new();
    sheet.column(1, 3, vec![e(Div0), t(""), t("abc"), e(NA), blank.clone()]);
    let c_div0 = sheet.area(1, 3, 1, 3);
    let c_empty_str = sheet.area(2, 3, 2, 3);
    let c_abc = sheet.area(3, 3, 3, 3);
    let c_na = sheet.area(4, 3, 4, 3);
    let c_blank = sheet.area(5, 3, 5, 3);
    let c_multi = sheet.area(1, 3, 2, 3);

    // VLOOKUP miss with reference arg[2] => #N/A; inline => #DIV/0! / #VALUE!
    error(call_on(&mut sheet, "VLOOKUP", vec![n(-1000.0), vtbl.clone(), c_div0.clone()]), NA);
    error(call_on(&mut sheet, "VLOOKUP", vec![n(-1000.0), vtbl.clone(), c_empty_str.clone()]), NA);
    error(call_on(&mut sheet, "VLOOKUP", vec![n(-1000.0), vtbl.clone(), e(Div0)]), Div0);
    error(call_on(&mut sheet, "VLOOKUP", vec![n(-1000.0), vtbl.clone(), t("")]), Value);
    // VLOOKUP hit with bad cell reference arg[2] => #REF!
    error(call_on(&mut sheet, "VLOOKUP", vec![n(150.0), vtbl.clone(), c_div0.clone()]), Ref);
    error(call_on(&mut sheet, "VLOOKUP", vec![n(150.0), vtbl.clone(), c_empty_str.clone()]), Ref);
    error(call_on(&mut sheet, "VLOOKUP", vec![n(150.0), vtbl.clone(), c_abc.clone()]), Ref);
    error(call_on(&mut sheet, "VLOOKUP", vec![n(150.0), vtbl.clone(), c_na.clone()]), Ref);
    // HLOOKUP miss => #N/A; hit => #REF!
    error(call_on(&mut sheet, "HLOOKUP", vec![n(-1000.0), htbl.clone(), c_div0.clone()]), NA);
    error(call_on(&mut sheet, "HLOOKUP", vec![n(150.0), htbl.clone(), c_div0.clone()]), Ref);
    error(call_on(&mut sheet, "HLOOKUP", vec![n(150.0), htbl, c_empty_str.clone()]), Ref);

    // MATCH eager cell reference arg[2]: error/text/multi-cell => #REF! even on miss; blank => 0
    let mcol = col(&[-500.0, 100.0, 300.0]);
    error(call_on(&mut sheet, "MATCH", vec![n(-1000.0), mcol.clone(), c_na.clone()]), Ref);
    error(call_on(&mut sheet, "MATCH", vec![n(100.0), mcol.clone(), c_na]), Ref);
    error(call_on(&mut sheet, "MATCH", vec![n(100.0), mcol.clone(), c_div0]), Ref);
    error(call_on(&mut sheet, "MATCH", vec![n(100.0), mcol.clone(), c_empty_str]), Ref);
    error(call_on(&mut sheet, "MATCH", vec![n(100.0), mcol.clone(), c_abc]), Ref);
    error(call_on(&mut sheet, "MATCH", vec![n(100.0), mcol.clone(), c_multi]), Ref);
    exact(call_on(&mut sheet, "MATCH", vec![n(100.0), mcol.clone(), c_blank]), 2.0);
    // Inline error/text at MATCH arg[2] keep #DIV/0! / #VALUE!:
    error(call("MATCH", vec![n(100.0), mcol.clone(), e(Div0)]), Div0);
    error(call("MATCH", vec![n(100.0), mcol, t("")]), Value);
    // MATCH -T23 stays descending (< 0.0), not snapped to 0:
    exact(
        call("MATCH", vec![n(150.0), col(&[300.0, 200.0, 100.0]), n(f(0xbe80_0200_0000_0000))]),
        2.0,
    );
}

/// Excel 16.0.20625.20108 (probed 2026-11-25, Touchstone relay, AMD Ryzen 7 4800H) orders
/// binary64 keys exactly in SORT and SORTBY, both directions: keys 1 and 3 ulps apart are never
/// a tie, in either input order (dc-sort-adjacent-keys-oq630).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn sort_orders_adjacent_binary64_keys_exactly() {
    use calc_func::value::{CalcArray, CalcValue};
    let bases = [0.3_f64, 1.2, 7.7, 1e-5, 3.3e100, 1e15];
    for base in bases {
        for gap in [1u64, 3] {
            let lo = base;
            let hi = f64::from_bits(base.to_bits() + gap);
            for (first, second) in [(lo, hi), (hi, lo)] {
                let keys = CalcValue::array(CalcArray::from_rows(vec![vec![n(first)], vec![n(second)]]).unwrap());
                let tags = col(&[1.0, 2.0]);
                let tag_of = |k: f64| if k == first { 1.0 } else { 2.0 };
                let ascending = array(call("SORT", vec![keys.clone()]));
                assert_eq!(ascending.2, vec![lo, hi], "SORT {first:e} {second:e}");
                for (order, want) in [(1.0, [lo, hi]), (-1.0, [hi, lo])] {
                    let got = array(call("SORTBY", vec![tags.clone(), keys.clone(), n(order)]));
                    assert_eq!(got.2, want.map(tag_of).to_vec(), "SORTBY {first:e} {second:e} {order}");
                }
            }
        }
    }
}

/// dc-ztvv.26.132: Excel never builds a dynamic array larger than the sheet. Excel 16.0.20527
/// (Touchstone, 2026-11-25): a SEQUENCE or RANDARRAY dimension above 1,048,576 after flooring is
/// #VALUE! (checked before the zero-dimension #CALC!); more than 16,384 columns is #SPILL!. The
/// call must return at once, without building the cells.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn sequence_and_randarray_beyond_the_grid_answer_without_allocating() {
    use calc_func::value::WorksheetErrorCode::Spill;
    let huge = f64::from_bits(0x68db_eece_93c9_0823);
    for name in ["SEQUENCE", "RANDARRAY"] {
        error(call(name, vec![n(huge)]), Value);
        error(call(name, vec![n(huge), n(1.0)]), Value);
        error(call(name, vec![n(1.0), n(huge)]), Value);
        error(call(name, vec![n(1_048_577.0), n(1.0)]), Value);
        error(call(name, vec![n(1.0), n(1_048_577.5)]), Value);
        error(call(name, vec![n(1_048_577.0), n(0.0)]), Value);
        error(call(name, vec![n(0.0), n(1e300)]), Value);
        error(call(name, vec![n(1.0), n(16_385.0)]), Spill);
        error(call(name, vec![n(1.0), n(1_048_576.0)]), Spill);
    }
    error(call("SEQUENCE", vec![n(1_048_576.0), n(1_048_576.0)]), Spill);
    error(call("SEQUENCE", vec![n(0.5), n(1.0)]), Calc);
    assert_eq!(array(call("SEQUENCE", vec![n(1_048_576.9)])).0, 1_048_576);
}

/// dc-ztvv.26.132: EXPAND's target dimension, truncated, is #VALUE! below 1 or from 2^31, #NUM!
/// above 1,048,576, and a result of more than 16,384 columns is #SPILL! (Excel 16.0.20527).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn expand_beyond_the_grid_answers_without_allocating() {
    use calc_func::value::WorksheetErrorCode::Spill;
    let huge = f64::from_bits(0x68db_eece_93c9_0823);
    error(call("EXPAND", vec![n(1.0), n(huge)]), Value);
    error(call("EXPAND", vec![n(1.0), n(1.0), n(huge)]), Value);
    error(call("EXPAND", vec![n(1.0), n(2_147_483_648.0), n(1.0)]), Value);
    error(call("EXPAND", vec![n(1.0), n(2_147_483_647.9), n(1.0)]), Num);
    error(call("EXPAND", vec![n(1.0), n(1.0), n(1_000_000_000.0)]), Num);
    error(call("EXPAND", vec![n(1.0), n(1_048_577.1), n(1.0)]), Num);
    error(call("EXPAND", vec![n(1.0), n(0.99), n(1.0)]), Value);
    error(call("EXPAND", vec![n(1.0), n(1.0), n(16_385.0)]), Spill);
    error(call("EXPAND", vec![n(1.0), n(1_048_576.0), n(1_048_576.0)]), Spill);
    assert_eq!(array(call("EXPAND", vec![n(1.0), n(1.5), n(1.0)])), (1, 1, vec![1.0]));
}

/// Excel 16.0.20527 (Touchstone, 2026-11-25; dc-j9k.256 to .269): the wrap count is truncated and must be in
/// 1..=2147483647 (else #NUM!); an error vector wins first, then the count's own error, then a blank or
/// two-dimensional vector is #VALUE!, then the count range.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn wraprows_and_wrapcols_count_and_argument_order() {
    let column = || arr_values(vec![vec![n(1.0)], vec![n(2.0)], vec![n(3.0)]]);
    let square = || arr_values(vec![vec![n(1.0), n(2.0)], vec![n(3.0), n(4.0)]]);
    let blank = calc_func::value::CalcValue::empty;
    for (rows_name, cols_name) in [("WRAPROWS", false), ("WRAPCOLS", true)] {
        // WRAPROWS puts the vector in one row when the count is at least its length; WRAPCOLS in one column.
        let (r, c) = if cols_name { (3, 1) } else { (1, 3) };
        assert_eq!(array(call(rows_name, vec![column(), n(1.5 + 99_999.0)])), (r, c, vec![1.0, 2.0, 3.0]));
        assert_eq!(array(call(rows_name, vec![column(), n(2_147_483_647.5)])), (r, c, vec![1.0, 2.0, 3.0]));
        for bad in [0.5, 0.0, -1.0, 2_147_483_648.0, 4_294_967_296.0, 1e100] {
            error(call(rows_name, vec![column(), n(bad)]), Num);
            error(call(rows_name, vec![n(5.0), n(bad)]), Num);
        }
        assert_eq!(call(rows_name, vec![n(5.0), n(1.5)]), Ok(n(5.0)));
        error(call(rows_name, vec![square(), n(0.0)]), Value);
        error(call(rows_name, vec![square(), e(NA)]), NA);
        error(call(rows_name, vec![e(NA), n(-1.0)]), NA);
        error(call(rows_name, vec![blank(), n(2.0)]), Value);
        error(call(rows_name, vec![blank(), n(0.0)]), Value);
        error(call(rows_name, vec![column(), blank()]), Num);
    }
    // 1.5 truncates to 1: a column wraps to a column (rows), a row to one column per element (cols).
    assert_eq!(array(call("WRAPROWS", vec![column(), n(1.5)])), (3, 1, vec![1.0, 2.0, 3.0]));
    let padded = call("WRAPROWS", vec![column(), n(2.0), e(NA)]).unwrap();
    let padded = padded.as_array().unwrap();
    assert_eq!((padded.shape().rows, padded.shape().cols), (2, 2));
    assert_eq!(padded.get(1, 1).unwrap().as_error(), Some(NA));
}

/// Excel 16.0.20527 (Touchstone, 2026-11-25; dc-j9k.314 onwards): a RANDARRAY whose minimum exceeds its
/// maximum is #VALUE! even where the size alone would be #SPILL! or too big to model; a zero size is
/// #CALC! before that check.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn randarray_minimum_above_maximum_is_value_before_spill() {
    use calc_func::value::WorksheetErrorCode::Spill;
    error(call("RANDARRAY", vec![n(10.5), n(32768.0), n(664.0)]), Value);
    error(call("RANDARRAY", vec![n(10.0), n(16385.0), n(664.0)]), Value);
    error(call("RANDARRAY", vec![n(2000.0), n(2000.0), n(5.0), n(1.0)]), Value);
    error(call("RANDARRAY", vec![n(0.0), n(1.0), n(5.0), n(1.0)]), Calc);
    error(call("RANDARRAY", vec![n(1.0), n(1.0), n(1.2), n(1.5), b(true)]), Value);
    error(call("RANDARRAY", vec![n(10.5), n(32768.0)]), Spill);
}

/// Excel 16.0.20527 (Touchstone, dc-j9k.351 to .353): SEQUENCE floors a dimension the way RANDARRAY does,
/// so a negative dimension within about 2^-23 of zero is a zero dimension (#CALC!), not #VALUE!.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn sequence_tiny_negative_dimension_is_calc() {
    error(call("SEQUENCE", vec![n(f64::from_bits(0x8050_0000_0000_0000))]), Calc);
    error(call("SEQUENCE", vec![n(f64::from_bits(0x8010_0000_0000_0001)), n(0.0)]), Calc);
    error(call("SEQUENCE", vec![n(f64::from_bits(0x3d72_fe28_7039_747c)), n(f64::from_bits(0xbd9a_4031_0ea7_ccad))]), Calc);
    error(call("SEQUENCE", vec![n(-0.5)]), Value);
}

/// Excel 16.0.20527 (Touchstone, 2026-11-25; dc-j9k.277 to .281): ARRAYTOTEXT of a one-cell reference
/// that holds an error is that error whatever the format, and of a blank one-cell reference is #VALUE!;
/// a wider range shows its errors and blanks as text.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn arraytotext_of_one_cell_reference_holding_an_error_or_blank() {
    use calc_func::value::WorksheetErrorCode::Ref;
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![e(Ref), n(5.0)]);
    sheet.column(1, 3, vec![n(1.0)]);
    sheet.column(1, 4, vec![e(NA)]);
    let error_cell = sheet.area(1, 1, 1, 1);
    let blank_cell = sheet.area(1, 2, 1, 2);
    let wide = sheet.area(1, 3, 1, 4);
    for format in [vec![], vec![n(0.0)], vec![n(1.0)], vec![n(2.0)], vec![e(calc_func::value::WorksheetErrorCode::Div0)]] {
        let with = |first: calc_func::value::CalcValue| [vec![first], format.clone()].concat();
        error(call_on(&mut sheet, "ARRAYTOTEXT", with(error_cell.clone())), Ref);
        error(call_on(&mut sheet, "ARRAYTOTEXT", with(blank_cell.clone())), Value);
    }
    text(call_on(&mut sheet, "ARRAYTOTEXT", vec![wide.clone()]), "1, #N/A");
    text(call_on(&mut sheet, "ARRAYTOTEXT", vec![wide, n(1.0)]), "{1,#N/A}");
}

/// Excel 16.0.20625 (Touchstone sheet cases, dc-shqi1, dc-5299v): a one-cell reference to an
/// error or blank is dereferenced as a scalar in WRAPROWS/WRAPCOLS (error wins over bad count;
/// blank or missing returns #VALUE! even before a #DIV/0! count). `pad_with` is a value parameter
/// (`V`), so `calc_func::invoke` rejects a raw reference at index 2 (formula evaluation dereferences
/// a one-cell pad reference in `calc-fml`, tested in `calc-fml/tests/acceptance/f3.rs`).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn wraprows_and_wrapcols_one_cell_references_missing_and_pad_ref() {
    use calc_func::FunctionFault;
    use calc_func::value::WorksheetErrorCode::Div0;
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![e(NA), n(5.0), n(6.0), n(7.0)]);
    sheet.column(1, 5, vec![n(99.0)]);
    let err_cell = sheet.area(1, 1, 1, 1);
    let blank_cell = sheet.area(1, 3, 1, 3);
    let pad_cell = sheet.area(1, 5, 1, 5);
    let vec_ref = sheet.area(2, 1, 4, 1);
    for name in ["WRAPROWS", "WRAPCOLS"] {
        error(call_on(&mut sheet, name, vec![err_cell.clone(), n(-1.0)]), NA);
        error(call_on(&mut sheet, name, vec![err_cell.clone(), n(2.0)]), NA);
        error(call_on(&mut sheet, name, vec![err_cell.clone(), e(Div0)]), NA);
        error(call_on(&mut sheet, name, vec![blank_cell.clone(), n(-1.0)]), Value);
        error(call_on(&mut sheet, name, vec![blank_cell.clone(), n(2.0)]), Value);
        error(call_on(&mut sheet, name, vec![blank_cell.clone(), e(Div0)]), Value);
        error(call_on(&mut sheet, name, vec![missing(), n(2.0)]), Value);
        error(call_on(&mut sheet, name, vec![missing(), e(Div0)]), Value);
        assert!(matches!(
            call_on(&mut sheet, name, vec![vec_ref.clone(), n(2.0), pad_cell.clone()]),
            Err(FunctionFault::ReferenceArgument { index: 2, .. })
        ));
    }
    assert_eq!(
        array(call_on(&mut sheet, "WRAPROWS", vec![vec_ref.clone(), n(2.0), n(99.0)])),
        (2, 2, vec![5.0, 6.0, 7.0, 99.0])
    );
    assert_eq!(
        array(call_on(&mut sheet, "WRAPCOLS", vec![vec_ref, n(2.0), n(99.0)])),
        (2, 2, vec![5.0, 7.0, 6.0, 99.0])
    );
}

/// Excel 16.0.20625 (Touchstone sheet cases, dc-shqi1): MAKEARRAY rejects columns > 16,384 and
/// rows > 1,048,576 with #SPILL! without evaluating; generated arrays above GENERATED_CELL_LIMIT
/// refuse as not modelled without allocating >1.5 GiB.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn makearray_rejects_beyond_grid_columns_and_large_generated_arrays_refuse() {
    use calc_func::value::WorksheetErrorCode::Spill;
    error(call("MAKEARRAY", vec![n(1.0), n(16_385.0), a_lambda(2)]), Spill);
    error(call("MAKEARRAY", vec![n(1_048_577.0), n(1.0), a_lambda(2)]), Spill);
    refused(call("SEQUENCE", vec![n(2048.0), n(1024.0)]));
}

/// Excel 16.0.20625 (Touchstone bisection, dc-shqi1): SEQUENCE's negative zero-floor cutoff is
/// exactly -(2^-23 + 2^-34) = 0xbe80020000000000 (#CALC!), one ULP more negative is #VALUE!.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn sequence_negative_cutoff_is_exact_to_the_bit() {
    error(call("SEQUENCE", vec![n(f64::from_bits(0xbe80_0200_0000_0000))]), Calc);
    error(call("SEQUENCE", vec![n(f64::from_bits(0xbe80_0200_0000_0001))]), Value);
}

/// Excel 16.0.20625 (Touchstone, dc-shqi1): SORT descending orders 1-ULP adjacent binary64 keys
/// accurately and honours NumericMode::Exact refusal via site.binary64().
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn sort_descending_orders_adjacent_binary64_keys_and_checks_numeric_mode() {
    use calc_func::value::{NumericMode, NumericTargetId};
    use calc_func::{EvalServices, FunctionCallScratch, FunctionFault, Notation, catalogue, invoke};
    let a = 1.0_f64;
    let b = f64::from_bits(a.to_bits() + 1);
    let col = arr_values(vec![vec![n(a)], vec![n(b)]]);
    let (r, c, cells) = array(call("SORT", vec![col.clone(), n(1.0), n(-1.0)]));
    assert_eq!((r, c), (2, 1));
    assert_eq!(cells[0].to_bits(), b.to_bits());
    assert_eq!(cells[1].to_bits(), a.to_bits());

    let target = catalogue().resolve_surface(Notation::Call, "SORT").unwrap();
    let mode = NumericMode::Exact(
        NumericTargetId::new("test-build", "test-channel", "test-compat", "test-env").unwrap(),
    );
    let mut services = EvalServices::new(&mode);
    let mut scratch = FunctionCallScratch::new();
    // 1-arg SORT reaches order() directly without sort_index/sort_order coercion.
    let mut args = Given(vec![col]);
    assert!(matches!(
        invoke(target, &mut args, &mut services, &mut scratch),
        Err(FunctionFault::ExactUnavailable { .. })
    ));
}
