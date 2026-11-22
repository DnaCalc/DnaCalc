//! dc-b1l: OFFSET and INDIRECT lifted over an array give an array of references, end to end
//! through calc-host in dynamic-array formulas typed as Excel's Range.Formula2 enters them. Every
//! expectation is Excel 16.0.20527's, read through COM on 2026-10-30 over 1..6 in A1:A6, 10..60 in
//! B1:B6 and "x" in C1 (the catalogue's dcb1l_language rows hold legacy-formula forms, which the
//! oracle corpus compares): SUM, AVERAGE and MAX read each reference's first cell, COUNT counts
//! none and COUNTA each, SUBTOTAL, SUMIF, COUNTIF, AREAS, ROW, INDEX, N and T run once per
//! reference, and the array itself, an operator over it and TEXT of it are #VALUE!.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_host::value::{CellKey, CoreRef, WorksheetErrorCode};
use calc_host::{
    CalcMode, HostOptions, HostSession, MemorySource, OpenPolicy, PackageRead, ProjectedValue,
    ProjectionRequest, SkinIntent, project,
};
use std::sync::Arc;

/// R25/R26's unchanged captures admit both CHOOSE columns below INDEX in these
/// Legacy regression and inverse formulas. This checks execution/provenance admission;
/// corpus comparison still counts every differing numeric value as a failure.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_b1l__nested_matrix_arguments_use_the_full_array() {
    for (bytes, captured, expected_count) in [
        (
            include_bytes!("../../calc-func/tests/fixtures/dcj9k_r25_regression/oracle.xlsx")
                .as_slice(),
            include_bytes!(
                "../../calc-func/tests/captures/dcj9k_r25_regression/oracle.capture.json"
            )
            .as_slice(),
            456,
        ),
        (
            include_bytes!("../../calc-func/tests/fixtures/dcj9k_r25_qr_stores/oracle.xlsx")
                .as_slice(),
            include_bytes!(
                "../../calc-func/tests/captures/dcj9k_r25_qr_stores/oracle.capture.json"
            )
            .as_slice(),
            38,
        ),
        (
            include_bytes!("../../calc-func/tests/fixtures/dcj9k_r26_regression/oracle.xlsx")
                .as_slice(),
            include_bytes!(
                "../../calc-func/tests/captures/dcj9k_r26_regression/oracle.capture.json"
            )
            .as_slice(),
            64,
        ),
    ] {
        let capture = calc_verdict::CaptureRecord::decode(captured).unwrap();
        for budget in [usize::MAX, 1] {
            let mut book = HostSession::open(
                Arc::new(MemorySource::new(bytes.to_vec())),
                &OpenPolicy::default(),
                HostOptions {
                    calc_mode: Some(CalcMode::Manual),
                    ..HostOptions::default()
                },
            )
            .unwrap();
            book.set_calculation_budget(calc_host::WorkBudget::evaluations(budget));
            assert!(book.dispatch(SkinIntent::Rebuild).accepted());
            for _ in 0..10_000 {
                if !book.has_pending_calculation() {
                    break;
                }
                book.advance_calculation();
            }
            assert!(!book.has_pending_calculation());
            let mut checked = 0;
            for observed in &capture.observations {
                let sheet = book
                    .host()
                    .sheets()
                    .iter()
                    .find(|sheet| sheet.name().text().to_string_lossy() == observed.cell.sheet())
                    .unwrap()
                    .id();
                let at = CellKey::new(sheet, observed.cell.row(), observed.cell.column()).unwrap();
                let Some(calc_host::CellInput::Formula(formula)) = book.host().model().cell(at)
                else {
                    continue;
                };
                let text = formula.text().to_string_lossy();
                if !(text.contains("INDEX(LINEST(")
                    || text.contains("INDEX(LOGEST(")
                    || text.contains("INDEX(MINVERSE("))
                    || !text.contains("CHOOSE({1,2},")
                {
                    continue;
                }
                if expected_count == 64 && !text.contains("INDEX(MINVERSE(") {
                    continue;
                }
                assert_eq!(formula.form(), calc_host::FormulaForm::Legacy);
                let page = project(book.host(), &ProjectionRequest { cells: vec![at] }).unwrap();
                let ProjectedValue::Calculated { value, calculation } = page.cells()[0].value()
                else {
                    panic!(
                        "nested matrix must calculate at {at:?}: {:?}",
                        page.cells()[0].value()
                    );
                };
                assert!(value.as_number().is_some() || value.as_error().is_some());
                // Six reviewed 4x2 predictor controls and all 64 R26 inverse
                // roots have finite captured numbers. A scalar-intersection
                // error cannot satisfy their admission check. Numeric roots
                // retain the corpus's complete value comparison.
                if expected_count == 64
                    || (expected_count == 38 && [63, 64, 67, 68, 88, 89].contains(&at.row()))
                {
                    assert!(
                        observed
                            .observation
                            .expected()
                            .expect("a value observation")
                            .as_number()
                            .is_some()
                    );
                    assert!(
                        value.as_number().is_some(),
                        "two predictor columns at {at:?}: {value:?}"
                    );
                }
                if expected_count == 64 && at.row() == 3829 {
                    // [[1.3,1],[1,2]] has the recorded top-left inverse 1.25;
                    // intersecting to one value cannot satisfy this control.
                    assert_eq!(
                        calc_verdict::compare(
                            observed
                                .observation
                                .expected()
                                .expect("a value observation"),
                            value
                        ),
                        None
                    );
                }
                let stamp = calculation
                    .stamp()
                    .expect("ordinary execution needs its result certificate");
                assert!(!stamp.reads.uses_imported_cache && !stamp.reads.uses_boundary_cache);
                assert!(matches!(
                    stamp.computed_by,
                    calc_engine::contract::ComputationOrigin::Evaluated { .. }
                ));
                checked += 1;
            }
            assert_eq!(checked, expected_count);
        }
    }
}

/// Excel 16.0.20527, eng13's unchanged Legacy capture. The workbook keeps true
/// second-argument references beside unary-plus controls, and computed Empty
/// beside raw blank references and empty text. All 50 roots and observers run
/// through the same host open/rebuild/project route as the application.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_b1l34_dc_ejvk5__legacy_argument_and_selected_result_origins() {
    let capture = calc_verdict::CaptureRecord::decode(include_bytes!(
        "captures/eng13_legacy/oracle.capture.json"
    ))
    .unwrap();
    let mut book = HostSession::open(
        Arc::new(MemorySource::new(
            include_bytes!("fixtures/eng13_legacy/oracle.xlsx").to_vec(),
        )),
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Automatic),
            ..HostOptions::default()
        },
    )
    .unwrap();
    assert!(book.dispatch(SkinIntent::Rebuild).accepted());
    for _ in 0..128 {
        if !book.has_pending_calculation() {
            break;
        }
        book.advance_calculation();
    }
    assert!(!book.has_pending_calculation());
    assert_eq!(capture.observations.len(), 50);
    for observed in &capture.observations {
        let sheet = book
            .host()
            .sheets()
            .iter()
            .find(|s| s.name().text().to_string_lossy() == observed.cell.sheet())
            .unwrap()
            .id();
        let at = CellKey::new(sheet, observed.cell.row(), observed.cell.column()).unwrap();
        let page = project(book.host(), &ProjectionRequest { cells: vec![at] }).unwrap();
        let ProjectedValue::Calculated { value, .. } = page.cells()[0].value() else {
            panic!("the recorded result must calculate: {:?}", page.cells()[0]);
        };
        assert_eq!(
            value,
            observed
                .observation
                .expected()
                .expect("a value observation"),
            "{}!{}",
            observed.cell.sheet(),
            observed.cell.a1()
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_b1l__retained_legacy_mround_and_quotient_reference_controls() {
    let bytes = include_bytes!("../../calc-func/tests/fixtures/fn2_questions/oracle.xlsx");
    let capture = calc_verdict::CaptureRecord::decode(include_bytes!(
        "../../calc-func/tests/captures/fn2_questions/oracle.capture.json"
    ))
    .unwrap();
    let mut book = HostSession::open(
        Arc::new(MemorySource::new(bytes.to_vec())),
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Automatic),
            ..HostOptions::default()
        },
    )
    .unwrap();
    assert!(book.dispatch(SkinIntent::Rebuild).accepted());
    for _ in 0..128 {
        if !book.has_pending_calculation() {
            break;
        }
        book.advance_calculation();
    }
    assert!(!book.has_pending_calculation());
    let mut checked = 0;
    for observed in &capture.observations {
        if ![
            "fn2_q176", "fn2_q177", "fn2_q178", "fn2_q179", "fn2_q180", "fn2_q181", "fn2_q182",
            "fn2_q183", "fn2_o176", "fn2_o177", "fn2_o178", "fn2_o179", "fn2_o180", "fn2_o181",
            "fn2_o182", "fn2_o183",
        ]
        .contains(&observed.cell.sheet())
        {
            continue;
        }
        if observed.cell.sheet().starts_with("fn2_o") && observed.cell.row() != 1 {
            continue;
        }
        let sheet = book
            .host()
            .sheets()
            .iter()
            .find(|s| s.name().text().to_string_lossy() == observed.cell.sheet())
            .unwrap()
            .id();
        let at = CellKey::new(sheet, observed.cell.row(), observed.cell.column()).unwrap();
        let page = project(book.host(), &ProjectionRequest { cells: vec![at] }).unwrap();
        let ProjectedValue::Calculated { value, .. } = page.cells()[0].value() else {
            panic!("the recorded control must calculate: {:?}", page.cells()[0]);
        };
        assert_eq!(
            value,
            observed
                .observation
                .expected()
                .expect("a value observation"),
            "{}!{}",
            observed.cell.sheet(),
            observed.cell.a1()
        );
        checked += 1;
    }
    assert_eq!(checked, 16);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_ejvk__computed_blank_selection_retains_reference_controls() {
    check_captured_book(
        include_bytes!("../../calc-func/tests/fixtures/fn2_questions/oracle.xlsx"),
        include_bytes!("../../calc-func/tests/captures/fn2_questions/oracle.capture.json"),
        &[
            "fn2_q017", "fn2_q018", "fn2_q019", "fn2_q020", "fn2_o017", "fn2_o018", "fn2_o019",
            "fn2_o020", "fn2_q194", "fn2_q195", "fn2_q210", "fn2_q211", "fn2_q212", "fn2_q213",
            "fn2_o194", "fn2_o195", "fn2_o210", "fn2_o211", "fn2_o212", "fn2_o213", "fn2_o214",
            "fn2_o215",
        ],
        46,
        None,
    );
}

/// Original fn5 packet workbooks and full Excel captures; all roots, observers,
/// spill shapes and calculated provenance run through the application host.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_b1l35_dc_b1l36_dc_b1l30_dc_zmk10__fn5_captured_boundaries() {
    let books: [(&[u8], &[u8], usize); 5] = [
        (
            include_bytes!("../../calc-func/tests/fixtures/fn5_helper_boundaries/oracle.xlsx"),
            include_bytes!(
                "../../calc-func/tests/captures/fn5_helper_boundaries/oracle.capture.json"
            ),
            526,
        ),
        (
            include_bytes!("../../calc-func/tests/fixtures/fn5_selection_boundaries/oracle.xlsx"),
            include_bytes!(
                "../../calc-func/tests/captures/fn5_selection_boundaries/oracle.capture.json"
            ),
            408,
        ),
        (
            include_bytes!("../../calc-func/tests/fixtures/fn5_regex_results/oracle.xlsx"),
            include_bytes!("../../calc-func/tests/captures/fn5_regex_results/oracle.capture.json"),
            160,
        ),
        (
            include_bytes!("../../calc-func/tests/fixtures/fn5_dates_1900/oracle.xlsx"),
            include_bytes!("../../calc-func/tests/captures/fn5_dates_1900/oracle.capture.json"),
            320,
        ),
        (
            include_bytes!("../../calc-func/tests/fixtures/fn5_digits_zero/oracle.xlsx"),
            include_bytes!("../../calc-func/tests/captures/fn5_digits_zero/oracle.capture.json"),
            432,
        ),
    ];
    for (bytes, captured, count) in books {
        // These captures record en-ZA with dot decimal and ordinary-space groups.
        let profile = calc_func::locale::format_profile(calc_func::locale::LocaleId::EnZa)
            .with_separators(".", " ")
            .unwrap();
        check_captured_book(bytes, captured, &[], count, Some(profile));
    }
}

/// Fn7 keeps scalar blanks, explicit omissions, larger ranges and arrays distinct.
/// Run all unchanged roots, observers and spill shapes through the application host.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_b1l43_dc_b1l44__fn7_argument_origins() {
    let books: [(&[u8], &[u8]); 2] = [
        (
            include_bytes!("../../calc-func/tests/fixtures/fn7_argument_origins/oracle.xlsx"),
            include_bytes!(
                "../../calc-func/tests/captures/fn7_argument_origins/oracle.capture.json"
            ),
        ),
        (
            include_bytes!("../../calc-func/tests/fixtures/fn7_date_argument_origins/oracle.xlsx"),
            include_bytes!(
                "../../calc-func/tests/captures/fn7_date_argument_origins/oracle.capture.json"
            ),
        ),
    ];
    for (bytes, captured) in books {
        let profile = calc_func::locale::format_profile(calc_func::locale::LocaleId::EnZa)
            .with_separators(".", " ")
            .unwrap();
        check_captured_book(bytes, captured, &[], 480, Some(profile));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_ebb5__groupby_shows_aggregate_and_field_headers() {
    check_captured_book(
        include_bytes!("../../../tests/corpus-examples/formulas/groupby-multiple-aggregates/example.xlsx"),
        include_bytes!("../../../tests/corpus-examples/formulas/groupby-multiple-aggregates/oracle.capture.json"),
        &[],
        1,
        None,
    );
}

fn check_captured_book(
    bytes: &[u8],
    captured: &[u8],
    sheets: &[&str],
    expected_count: usize,
    locale: Option<calc_host::FormatProfile>,
) {
    let capture = calc_verdict::CaptureRecord::decode(captured).unwrap();
    let mut book = HostSession::open(
        Arc::new(MemorySource::new(bytes.to_vec())),
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Automatic),
            locale,
            ..HostOptions::default()
        },
    )
    .unwrap();
    assert!(book.dispatch(SkinIntent::Rebuild).accepted());
    for _ in 0..128 {
        if !book.has_pending_calculation() {
            break;
        }
        book.advance_calculation();
    }
    assert!(!book.has_pending_calculation());
    let mut checked = 0;
    for observed in &capture.observations {
        if !sheets.is_empty() && !sheets.contains(&observed.cell.sheet()) {
            continue;
        }
        let sheet = book
            .host()
            .sheets()
            .iter()
            .find(|s| s.name().text().to_string_lossy() == observed.cell.sheet())
            .unwrap()
            .id();
        let at = CellKey::new(sheet, observed.cell.row(), observed.cell.column()).unwrap();
        let expected = observed
            .observation
            .expected()
            .expect("a value observation");
        let mut expected_cells = vec![(at, expected)];
        if let Some(array) = expected.as_array() {
            let extent = book
                .host()
                .spill_area(at)
                .expect("the recorded array must spill");
            assert_eq!(extent.first(), at);
            assert_eq!(
                (extent.last().row() - at.row() + 1) as usize,
                array.shape().rows
            );
            assert_eq!(
                (extent.last().column() - at.column() + 1) as usize,
                array.shape().cols
            );
            expected_cells.clear();
            for row in 0..array.shape().rows {
                for column in 0..array.shape().cols {
                    expected_cells.push((
                        CellKey::new(sheet, at.row() + row as u32, at.column() + column as u32)
                            .unwrap(),
                        array.get(row, column).unwrap(),
                    ));
                }
            }
        }
        for (cell, expected) in expected_cells {
            let page = project(book.host(), &ProjectionRequest { cells: vec![cell] }).unwrap();
            let ProjectedValue::Calculated { value, .. } = page.cells()[0].value() else {
                panic!(
                    "the recorded selection must calculate: {:?}",
                    page.cells()[0]
                );
            };
            assert_eq!(value, expected, "{}!{cell:?}", observed.cell.sheet());
            assert!(
                value.callable_value().is_none(),
                "cell publication keeps no callable"
            );
        }
        checked += 1;
    }
    assert_eq!(checked, expected_count);
}

fn shown(session: &HostSession, at: CellKey) -> String {
    let projection =
        project(session.host(), &ProjectionRequest { cells: vec![at] }).expect("projected");
    match projection.cells()[0].value() {
        ProjectedValue::Calculated { value, .. } => match value.core_ref() {
            CoreRef::Number(n) => format!("{n}"),
            CoreRef::Text(t) => format!("{:?}", String::from_utf16_lossy(t.utf16_code_units())),
            CoreRef::Logical(b) => format!("{b}"),
            CoreRef::Error(code) => format!("{code:?}"),
            other => format!("{other:?}"),
        },
        ProjectedValue::NotModeled { .. } => "not modelled".to_owned(),
        other => format!("{other:?}"),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_b1l__offset_and_indirect_over_an_array_give_references() {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(calc_host::blank_package()));
    let mut session = HostSession::open(
        source,
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Automatic),
            ..HostOptions::default()
        },
    )
    .expect("a new workbook opens");
    let sheet = session.host().sheets()[0].id();
    let at = |row: u32, column: u32| CellKey::new(sheet, row, column).expect("a cell");
    let mut enter = |row: u32, column: u32, text: &str| {
        let receipt = session.dispatch(SkinIntent::EnterCell {
            at: at(row, column),
            text: text.to_owned(),
        });
        assert!(receipt.accepted(), "{text}: {receipt:?}");
    };
    for row in 1..=6 {
        enter(row, 1, &row.to_string());
        enter(row, 2, &(10 * row).to_string());
    }
    enter(1, 3, "x");
    // Each formula in column E, four rows apart so that a spill of up to three rows has room.
    let formulas = [
        "=SUM(OFFSET(A1,{0,1,2},0))",
        "=SUM(OFFSET(A1,{0;1;2},0,2))",
        "=SUBTOTAL(9,OFFSET(A1,{0;1;2},0,2))",
        "=SUM(SUBTOTAL(9,OFFSET(A1,{0;1;2},0,2)))",
        "=N(OFFSET(A1,{0;1;2},0))",
        "=SUMIF(OFFSET(A1,{0;1},0,3),\">2\")",
        "=SUM(INDIRECT({\"A1\",\"A2\",\"B1\"}))",
        "=ROWS(OFFSET(A1,{0;1;2},0))",
        "=OFFSET(A1,{0;1;2},0)",
        "=COUNT(OFFSET(A1,{0,1,2},0))",
        "=COUNTA(OFFSET(A1,{0,1,2},0))",
        "=MAX(OFFSET(A1,{0,1,2},0,2))",
        "=AVERAGE(OFFSET(A1,{0,1,2},0))",
        "=ISREF(OFFSET(A1,{0;1},0))",
        "=AREAS(OFFSET(A1,{0;1},0))",
        "=ROW(OFFSET(A1,{0;1},0))",
        "=SUM(OFFSET(A1,{0,1,2},0)*1)",
        "=TEXT(OFFSET(A1,{0;1},0),\"0.0\")",
        "=INDEX(OFFSET(A1,{0;1},0,2),1,1)",
        "=T(OFFSET(C1,{0;1},0))",
        "=SUM(INDIRECT({\"A1:A2\",\"B1:B2\"}))",
    ];
    for (i, formula) in formulas.iter().enumerate() {
        enter(4 * i as u32 + 1, 5, formula);
    }
    let value = format!("{:?}", WorksheetErrorCode::Value);
    let expected: [&[&str]; 21] = [
        &["6"],
        &["6"],
        &["3", "5", "7"],
        &["15"],
        &["1", "2", "3"],
        &["3", "7"],
        &["13"],
        &["3"],
        &[&value, &value, &value],
        &["0"],
        &["3"],
        &["3"],
        &["2"],
        &["false"],
        &["1", "1"],
        &["1", "2"],
        &[&value],
        &[&value, &value],
        &["1", "2"],
        &["\"x\"", "\"\""],
        &["11"],
    ];
    for (i, values) in expected.iter().enumerate() {
        for (offset, value) in values.iter().enumerate() {
            let row = 4 * i as u32 + 1 + offset as u32;
            assert_eq!(
                shown(&session, at(row, 5)),
                *value,
                "{} (E{row})",
                formulas[i]
            );
        }
    }
}

/// Each consumer reads an array of references as Excel 16.0.20527 does (Touchstone sheet cases,
/// 2026-11-16, cell-context family reference-arrays, over 1..6 in A1:A6 and 10..60 in C1:C6).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_b1l__each_consumer_reads_an_array_of_references_as_excel() {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(calc_host::blank_package()));
    let mut session = HostSession::open(
        source,
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Automatic),
            ..HostOptions::default()
        },
    )
    .expect("a new workbook opens");
    let sheet = session.host().sheets()[0].id();
    let at = |row: u32, column: u32| CellKey::new(sheet, row, column).expect("a cell");
    let mut enter = |row: u32, column: u32, text: &str| {
        let receipt = session.dispatch(SkinIntent::EnterCell {
            at: at(row, column),
            text: text.to_owned(),
        });
        assert!(receipt.accepted(), "{text}: {receipt:?}");
    };
    for row in 1..=6 {
        enter(row, 1, &row.to_string());
        enter(row, 3, &(10 * row).to_string());
    }
    let r = "OFFSET(A1,{0;1;2},0,2)";
    let cases = [
        (format!("=MIN({r})"), vec!["1"]),
        (format!("=STDEV({r})"), vec!["1"]),
        (format!("=NPV(0.1,{r})"), vec!["4.815927873779113"]),
        (format!("=SUMPRODUCT({r})"), vec!["0"]),
        (format!("=MEDIAN({r})"), vec!["Value"]),
        (format!("=MATCH(2,{r},0)"), vec!["NA"]),
        (format!("=CORREL({r},{r})"), vec!["Num"]),
        (format!("=AND({r})"), vec!["NA"]),
        (format!("=ISERROR({r})"), vec!["true", "true", "true"]),
        (format!("=IFERROR({r},0)"), vec!["0", "0", "0"]),
        (format!("=RANK(1,{r})"), vec!["2", "NA", "NA"]),
        (format!("=COUNTIFS({r},\">1\")"), vec!["1", "2", "2"]),
        (format!("=INDEX({r},2,1)"), vec!["2", "3"]),
        (format!("=ISREF(INDEX({r},1,1))"), vec!["true"]),
        (format!("=INDEX({r},2)"), vec!["Value"]),
        (format!("=REDUCE(0,{r},LAMBDA(a,v,a+v))"), vec!["6", "9"]),
        (format!("=T({r})"), vec!["\"\"", "\"\"", "\"\""]),
        // fn8 2026-11-17 (Touchstone, family reference-arrays probes): UNIQUE keeps references
        // apart by their top rows and shows each kept one as #VALUE!; ARRAYTOTEXT is #CALC! once
        // its format reads as a number.
        (format!("=UNIQUE({r})"), vec!["Value", "Value", "Value"]),
        ("=ROWS(UNIQUE(OFFSET(A1,{0;0;1},0)))".to_owned(), vec!["2"]),
        (
            "=UNIQUE(INDIRECT({\"A1\";\"B1\"}))".to_owned(),
            vec!["Value"],
        ),
        (
            "=UNIQUE(OFFSET(A1,{0;0;1},0),FALSE,TRUE)".to_owned(),
            vec!["Value"],
        ),
        (
            "=UNIQUE(OFFSET(A1,0,{0,0,2}),TRUE,TRUE)".to_owned(),
            vec!["Calc"],
        ),
        (
            "=UNIQUE(OFFSET(A1,{0;0},{0,1}),TRUE)".to_owned(),
            vec!["Value", "Value"],
        ),
        (format!("=ARRAYTOTEXT({r})"), vec!["Calc"]),
        (format!("=ARRAYTOTEXT({r},-1)"), vec!["Calc"]),
        (format!("=ARRAYTOTEXT({r},\"x\")"), vec!["Value"]),
        (format!("=ARRAYTOTEXT({r},NA())"), vec!["NA"]),
        // AGGREGATE runs once per reference given as ref2 or later; ref1's are #VALUE!.
        (format!("=AGGREGATE(9,0,A1:A2,{r})"), vec!["6", "8", "10"]),
        (format!("=AGGREGATE(14,0,A1:A6,OFFSET(A1,{{0;1;2}},0))"), vec!["6", "5", "4"]),
        (format!("=AGGREGATE(9,0,{r},{r})"), vec!["Value", "Value", "Value"]),
        (format!("=AGGREGATE(9,0,{r})"), vec!["Value"]),
    ];
    for (i, (formula, _)) in cases.iter().enumerate() {
        enter(4 * i as u32 + 1, 8, formula);
    }
    for (i, (formula, values)) in cases.iter().enumerate() {
        for (offset, value) in values.iter().enumerate() {
            let row = 4 * i as u32 + 1 + offset as u32;
            assert_eq!(shown(&session, at(row, 8)), *value, "{formula} (H{row})");
        }
    }
}

/// WEEKNUM with a return type outside 1-21 numbers weeks from the start the evaluation's last
/// WEEKNUM or ISOWEEKNUM left, #NUM! with none (dc-amhpx; Excel 16.0.20527, Touchstone, cell-context
/// family weeknum-pass, each case asked alone). Known only to a formula alone with WEEKNUM in its
/// workbook; a second such formula leaves the state unknown, and the call is refused.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_amhpx__weeknum_reads_the_week_start_its_formula_left() {
    for (formula, value) in [
        ("=WEEKNUM(45000,30)", "Num"),
        ("=WEEKNUM(45000,2)&\"|\"&WEEKNUM(45000,30)", "\"12|12\""),
        ("=WEEKNUM(45000,1)&\"|\"&WEEKNUM(45000,30)", "\"11|11\""),
        ("=ISOWEEKNUM(45000)&\"|\"&WEEKNUM(45000,30)", "\"11|12\""),
        ("=WEEKNUM(45000,13)&\"|\"&WEEKNUM(45000,30)", "Num"),
        ("=IFERROR(WEEKNUM(-1,2),0)&\"|\"&WEEKNUM(45000,30)", "Num"),
        ("=WEEKNUM(45000,30)&\"|\"&WEEKNUM(45000,2)", "not modelled"),
    ] {
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(calc_host::blank_package()));
        let mut session = HostSession::open(
            source,
            &OpenPolicy::default(),
            HostOptions {
                calc_mode: Some(CalcMode::Automatic),
                ..HostOptions::default()
            },
        )
        .expect("a new workbook opens");
        let sheet = session.host().sheets()[0].id();
        let b2 = CellKey::new(sheet, 2, 2).unwrap();
        let entered = session.dispatch(SkinIntent::EnterCell {
            at: b2,
            text: formula.to_owned(),
        });
        assert!(entered.accepted(), "{formula}");
        assert_eq!(shown(&session, b2), value, "{formula}");
        // A second formula that may call WEEKNUM: the state is no longer known.
        let d4 = CellKey::new(sheet, 4, 4).unwrap();
        assert!(
            session
                .dispatch(SkinIntent::EnterCell {
                    at: d4,
                    text: "=WEEKNUM(45000,2)".to_owned(),
                })
                .accepted()
        );
        if formula == "=WEEKNUM(45000,30)" {
            assert!(session.dispatch(SkinIntent::Rebuild).accepted());
            assert_eq!(
                shown(&session, b2),
                "not modelled",
                "{formula} beside another WEEKNUM"
            );
        }
    }
}

/// ROW, ISFORMULA and FORMULATEXT of whole columns build their arrays, so a spill that passes the
/// sheet's edge is #SPILL! by the placement check and a consumer reads the whole array (Excel
/// 16.0.20527, Touchstone sheet cases, 2026-11-16/17, cell-context family reference-info, H10).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_mlhgn__whole_columns_are_built_and_placed() {
    for (formula, value) in [
        ("=ROW(C:D)", "Spill"),
        ("=ISFORMULA(C:D)", "Spill"),
        ("=FORMULATEXT(C:D)", "Spill"),
        ("=SUM(ROW(C:C))", "549756338176"),
        ("=ROWS(ISFORMULA(C:D))", "1048576"),
        ("=COUNTA(FORMULATEXT(A:A))", "1048576"),
    ] {
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(calc_host::blank_package()));
        let mut session = HostSession::open(
            source,
            &OpenPolicy::default(),
            HostOptions {
                calc_mode: Some(CalcMode::Automatic),
                ..HostOptions::default()
            },
        )
        .expect("a new workbook opens");
        let sheet = session.host().sheets()[0].id();
        let h10 = CellKey::new(sheet, 10, 8).unwrap();
        let entered = session.dispatch(SkinIntent::EnterCell {
            at: h10,
            text: formula.to_owned(),
        });
        assert!(entered.accepted(), "{formula}");
        assert_eq!(shown(&session, h10), value, "{formula}");
    }
}

/// A formula reads the cells its own spill occupies as blank, when it is entered and each time it
/// is calculated again with its spill in place, and its spill does not feed back (Excel P05,
/// 16.0.20527 through Touchstone sheet cases: cell-context family spill-placement own-*, and
/// fn8's re-evaluation probes 2026-11-17, a layout holding the formula in A1 read after a full
/// calculation and after each recalculation, R2 dnacalc-evidence/research/fn8-20261117-step4/).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_mlhgn__a_spill_over_cells_it_reads_reads_them_as_blank() {
    for (formula, expected) in [
        ("=SEQUENCE(3)+A3", ["1", "2", "3"]),
        ("=SEQUENCE(3)+A3+0*RAND()", ["1", "2", "3"]),
        ("=A2:A4&\"\"", ["\"\"", "\"\"", "\"\""]),
        ("=ISBLANK(A2:A4)", ["true", "true", "true"]),
        ("=OFFSET(B1,1,-1,3)+0*RAND()", ["0", "0", "0"]),
    ] {
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(calc_host::blank_package()));
        let mut session = HostSession::open(
            source,
            &OpenPolicy::default(),
            HostOptions {
                calc_mode: Some(CalcMode::Automatic),
                ..HostOptions::default()
            },
        )
        .expect("a new workbook opens");
        let sheet = session.host().sheets()[0].id();
        let at = |row| CellKey::new(sheet, row, 1).unwrap();
        let spill = |session: &HostSession| -> Vec<String> {
            (1..=3).map(|row| shown(session, at(row))).collect()
        };
        assert!(
            session
                .dispatch(SkinIntent::EnterCell {
                    at: at(1),
                    text: formula.to_owned(),
                })
                .accepted()
        );
        assert_eq!(spill(&session), expected, "{formula} entered");
        assert!(
            session
                .dispatch(SkinIntent::EnterCell {
                    at: CellKey::new(sheet, 1, 5).unwrap(),
                    text: "7".to_owned(),
                })
                .accepted()
        );
        assert_eq!(spill(&session), expected, "{formula} after an edit elsewhere");
        assert!(session.dispatch(SkinIntent::Rebuild).accepted());
        assert_eq!(spill(&session), expected, "{formula} after a full calculation");
    }
}
