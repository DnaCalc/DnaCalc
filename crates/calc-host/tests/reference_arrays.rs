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

/// R25's unchanged captures prove that known_x admits both CHOOSE columns below
/// INDEX in these Legacy formulas. This checks execution/provenance admission;
/// corpus comparison still counts every differing numeric value as a failure.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_b1l__nested_regression_predictors_use_the_full_array() {
    for (bytes, captured, expected_count) in [
        (include_bytes!("../../calc-func/tests/fixtures/dcj9k_r25_regression/oracle.xlsx").as_slice(),
         include_bytes!("../../calc-func/tests/captures/dcj9k_r25_regression/oracle.capture.json").as_slice(), 456),
        (include_bytes!("../../calc-func/tests/fixtures/dcj9k_r25_qr_stores/oracle.xlsx").as_slice(),
         include_bytes!("../../calc-func/tests/captures/dcj9k_r25_qr_stores/oracle.capture.json").as_slice(), 38),
    ] {
        let capture = calc_verdict::CaptureRecord::decode(captured).unwrap();
        for budget in [usize::MAX, 1] {
            let mut book = HostSession::open(
                Arc::new(MemorySource::new(bytes.to_vec())), &OpenPolicy::default(),
                HostOptions { calc_mode: Some(CalcMode::Manual), ..HostOptions::default() },
            ).unwrap();
            book.set_calculation_budget(calc_host::WorkBudget::evaluations(budget));
            assert!(book.dispatch(SkinIntent::Rebuild).accepted());
            for _ in 0..10_000 {
                if !book.has_pending_calculation() { break; }
                book.advance_calculation();
            }
            assert!(!book.has_pending_calculation());
            let mut checked = 0;
            for observed in &capture.observations {
                let sheet = book.host().sheets().iter()
                    .find(|sheet| sheet.name().text().to_string_lossy() == observed.cell.sheet()).unwrap().id();
                let at = CellKey::new(sheet, observed.cell.row(), observed.cell.column()).unwrap();
                let Some(calc_host::CellInput::Formula(formula)) = book.host().model().cell(at) else { continue; };
                let text = formula.text().to_string_lossy();
                if !(text.contains("INDEX(LINEST(") || text.contains("INDEX(LOGEST("))
                    || !text.contains("CHOOSE({1,2},") { continue; }
                assert_eq!(formula.form(), calc_host::FormulaForm::Legacy);
                let page = project(book.host(), &ProjectionRequest { cells: vec![at] }).unwrap();
                let ProjectedValue::Calculated { value, calculation } = page.cells()[0].value() else {
                    panic!("nested known_x must calculate at {at:?}: {:?}", page.cells()[0].value());
                };
                assert!(value.as_number().is_some() || value.as_error().is_some());
                // These six independently reviewed 4x2 predictor controls have
                // finite captured coefficients. A scalar-intersection error
                // cannot satisfy their admission check. Ill-conditioned numeric
                // roots elsewhere retain the corpus's complete value comparison.
                if expected_count == 38 && [63, 64, 67, 68, 88, 89].contains(&at.row()) {
                    assert!(observed.observation.expected().as_number().is_some());
                    assert!(value.as_number().is_some(), "two predictor columns at {at:?}: {value:?}");
                }
                let stamp = calculation.stamp().expect("ordinary execution needs its result certificate");
                assert!(!stamp.reads.uses_imported_cache && !stamp.reads.uses_boundary_cache);
                assert!(matches!(stamp.computed_by, calc_engine::contract::ComputationOrigin::Evaluated { .. }));
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
    )).unwrap();
    let mut book = HostSession::open(
        Arc::new(MemorySource::new(include_bytes!(
            "fixtures/eng13_legacy/oracle.xlsx"
        ).to_vec())),
        &OpenPolicy::default(),
        HostOptions { calc_mode: Some(CalcMode::Automatic), ..HostOptions::default() },
    ).unwrap();
    assert!(book.dispatch(SkinIntent::Rebuild).accepted());
    for _ in 0..128 {
        if !book.has_pending_calculation() { break; }
        book.advance_calculation();
    }
    assert!(!book.has_pending_calculation());
    assert_eq!(capture.observations.len(), 50);
    for observed in &capture.observations {
        let sheet = book.host().sheets().iter()
            .find(|s| s.name().text().to_string_lossy() == observed.cell.sheet()).unwrap().id();
        let at = CellKey::new(sheet, observed.cell.row(), observed.cell.column()).unwrap();
        let page = project(book.host(), &ProjectionRequest { cells: vec![at] }).unwrap();
        let ProjectedValue::Calculated { value, .. } = page.cells()[0].value() else {
            panic!("the recorded result must calculate: {:?}", page.cells()[0]);
        };
        assert_eq!(value, observed.observation.expected(), "{}!{}",
            observed.cell.sheet(), observed.cell.a1());
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_b1l__retained_legacy_mround_and_quotient_reference_controls() {
    let bytes = include_bytes!("../../calc-func/tests/fixtures/fn2_questions/oracle.xlsx");
    let capture = calc_verdict::CaptureRecord::decode(include_bytes!("../../calc-func/tests/captures/fn2_questions/oracle.capture.json")).unwrap();
    let mut book = HostSession::open(Arc::new(MemorySource::new(bytes.to_vec())),&OpenPolicy::default(),HostOptions {
        calc_mode:Some(CalcMode::Automatic),..HostOptions::default()
    }).unwrap();
    assert!(book.dispatch(SkinIntent::Rebuild).accepted());
    for _ in 0..128 { if !book.has_pending_calculation() { break; } book.advance_calculation(); }
    assert!(!book.has_pending_calculation());
    let mut checked = 0;
    for observed in &capture.observations {
        if !["fn2_q176","fn2_q177","fn2_q178","fn2_q179","fn2_q180","fn2_q181","fn2_q182","fn2_q183","fn2_o176","fn2_o177","fn2_o178","fn2_o179","fn2_o180","fn2_o181","fn2_o182","fn2_o183"].contains(&observed.cell.sheet()) { continue; }
        if observed.cell.sheet().starts_with("fn2_o") && observed.cell.row()!=1 { continue; }
        let sheet = book.host().sheets().iter().find(|s|s.name().text().to_string_lossy()==observed.cell.sheet()).unwrap().id();
        let at = CellKey::new(sheet,observed.cell.row(),observed.cell.column()).unwrap();
        let page = project(book.host(),&ProjectionRequest {cells:vec![at]}).unwrap();
        let ProjectedValue::Calculated {value,..} = page.cells()[0].value() else { panic!("the recorded control must calculate: {:?}",page.cells()[0]); };
        assert_eq!(value,observed.observation.expected(),"{}!{}",observed.cell.sheet(),observed.cell.a1());
        checked+=1;
    }
    assert_eq!(checked,16);
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
        let expected = observed.observation.expected();
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
