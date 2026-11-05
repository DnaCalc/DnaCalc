//! Acceptance cases owned by the dna-onecalc package, each registered by its exact name in
//! `apps/dna-onecalc/acceptance.json`. Every case is `#[test]` natively and `#[wasm_bindgen_test]`
//! on wasm32, with no browser or Node.js configuration of its own: `cargo xtask accept` forces the
//! browser with `WASM_BINDGEN_USE_BROWSER`.
//!
//! `onecalc_m1_1__reference_without_engine` drives the OneCalc controller (`dna_onecalc::Session`)
//! through its public API on every target, and on wasm32 also mounts the browser surface in an
//! element of the test page and drives it through dispatched DOM events, reading what it shows back
//! from the DOM. Both parts run the same examples (`EXAMPLES`): typed inputs, a formula, and the
//! result, diagnostics and reads expected for them. Expected cells are decoded by hand from the
//! examples' A1 text; expected values are derived by hand from Excel's documented operators and
//! functions and are not compared with Excel here (cfunc-m1.2's oracle capture is that evidence).
//! Two expectations are the implementation's current answer, not Excel's value, and are labelled
//! so: `=A1*3E-308-4.5000000000001E-308` (a difference below the normal range) is refused by calc-func as not modelled, and a cell with no input stops the
//! calculation where Excel would read a blank.
//!
//! `onecalc_m1_1__dependency_gate` reads `cargo metadata` itself, for every triple the
//! architecture check resolves, and walks dna-onecalc's resolved graph (its own normal, build and
//! dev dependencies, then their normal and build dependencies) with all features. Native only: it
//! starts cargo.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_fml::CompileError;
use calc_fml::EvalFault;
use calc_fml::reference::BindError;
use calc_fml::syntax::{SyntaxDiagnosticKind, TextSpan};
use calc_func::value::{CalcValue, CellKey, GridArea, SheetId, WorksheetErrorCode};
use calc_func::{FunctionFault, Notation, catalogue};
use dna_onecalc::{
    Calculation, Diagnostic, InputError, InputValue, Read, ReadRun, RunCells, Session, SheetError,
    display,
};

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn caller_comes_from_the_formula_instance() {
    let mut session = Session::new("Inputs").unwrap();
    for (at, row, column) in [("D7", 7.0, 4.0), ("B12", 12.0, 2.0)] {
        session.set_formula_cell(at).unwrap();
        for (formula, expected) in [
            ("=ROW()", row),
            ("=COLUMN()", column),
            ("=LET(f,LAMBDA(ROW()),f())", row),
        ] {
            session.set_formula(formula);
            let calculated = session.calculate();
            assert_eq!(
                calculated.result,
                Some(CalcValue::number(expected)),
                "{formula} at {at}: {:?}",
                calculated.diagnostics
            );
        }
    }
}

/// Evaluate retained raw Excel readings through the public application route. The canonical
/// capture codec supplies typed expected values, including their original UTF-16 code units.
/// Callers supply matching form/environment/qualified inputs. Selected formulas must not depend
/// on the source sheet's name; the session supplies their captured row and column.
fn check_captured_questions(
    bytes: &[u8], session: &mut Session,
    include: impl Fn(&calc_verdict::FormulaCell) -> bool, expected_count: usize,
) {
    let capture = calc_verdict::CaptureRecord::decode(bytes).unwrap();
    let mut failures = Vec::new();
    let mut compared = 0;
    for entry in capture.formulas.iter().filter(|entry| include(entry)) {
        let formula = entry.formula.as_ref().expect("a retained formula");
        let observation = capture.observations.iter().find(|o| o.cell == entry.cell)
            .expect("every selected question was observed");
        let calc_verdict::Observation::ExcelRaw(expected) = &observation.observation else {
            panic!("a selected question needs a raw Excel reading");
        };
        session.set_formula_cell(&entry.cell.a1()).unwrap();
        session.set_formula(formula);
        let calculated = session.calculate();
        // A dynamic formula's one-cell array publishes its sole primitive at the anchor.
        // Use the same exact projection as the witness verdict; larger arrays keep their shape.
        let published = calculated.result.as_ref().and_then(|value| {
            if entry.dynamic_array {
                calc_verdict::select_result(value, None)
            } else {
                Some(value.clone())
            }
        });
        compared += 1;
        if published.as_ref() != Some(expected) || !calculated.diagnostics.is_empty() {
            failures.push(format!("{} {formula}: expected {expected:?}, actual {:?}; {:?}",
                entry.cell.sheet(), calculated.result, calculated.diagnostics));
        }
    }
    assert_eq!(compared, expected_count, "all selected retained questions must run");
    assert!(failures.is_empty(), "{} of {compared} readings differ:\n{}", failures.len(), failures.join("\n"));
}

/// Fn3/fn7 scalar readings retain the original typed inputs and Excel results.
/// Fn7 service questions also retain their exact input readbacks and full service replies.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn delivered_scalar_readings_follow_the_application_route() {
    let readings: [&[u8]; 38] = [
        include_bytes!("../../../crates/calc-func/tests/captures/fn3_questions/LOWER.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn3_questions/PROPER.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn3_questions/REGEXTEST.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/SQRT.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/DATEVALUE.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/COUPNUM.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/COUPDAYSNC.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/COUPDAYBS.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/MODE.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/MODE.SNGL.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/DISC.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/INTRATE.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/RECEIVED.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/PRICEDISC.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/ACCRINTM.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/WORKDAY.INTL.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/NETWORKDAYS.INTL.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/DELTA.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/GESTEP.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/MULTINOMIAL.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/NETWORKDAYS.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/WORKDAY.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/YEARFRAC.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/TBILLEQ.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/TBILLPRICE.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/TBILLYIELD.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/ERF.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/SERIESSUM.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/IRR.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/TEXT.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/CUMIPMT.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/CUMPRINC.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/YIELD.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/ODDFYIELD.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/FIXED.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/VALUE.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/NUMBERVALUE.reading.json"),
        include_bytes!("../../../crates/calc-func/tests/captures/fn4_questions/SEARCH.reading.json"),


    ];
    let mut failures = Vec::new();
    let mut count = 0;
    for bytes in readings {
        let reading: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        for (witness_index, witness) in reading["witnesses"].as_array().unwrap().iter().enumerate() {
            let mut session = Session::new("Sheet1").unwrap();
            session.set_formula_cell("J20").unwrap();
            if reading.get("service_response").is_some() {
                use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
                session.set_environment(Some(CalcEnvironment {
                    profile: Some(format_profile(LocaleId::EnZa).with_separators(".", " ").unwrap()),
                    compatibility_version: witness["compatibility_version"].as_u64()
                        .or_else(|| reading["compatibility_version"].as_u64()).unwrap_or(1) as u32,
                    ..CalcEnvironment::dates(DateSystem::System1900)
                }));
            }
            let mut arguments = Vec::new();
            for (index, value) in witness["args"].as_array().unwrap().iter().enumerate() {
                let text = value.as_str().unwrap();
                let question = &reading["service_questions"][witness_index][index];
                let input = if question["type"].as_str() == Some("blank") {
                    InputValue::blank()
                } else if question["type"].as_str() == Some("boolean") {
                    InputValue::logical(question["value"].as_bool().unwrap())
                } else if question["type"].as_str() == Some("error") {
                    let value = calc_verdict::decode_result(text).unwrap();
                    let calc_func::value::CoreRef::Error(code) = value.core_ref() else {
                        panic!("a retained error input must decode as an error");
                    };
                    InputValue::error(code)
                } else if let Some(bits) = text.strip_prefix("0x") {
                    InputValue::number(f64::from_bits(u64::from_str_radix(bits, 16).unwrap())).unwrap()
                } else {
                    InputValue::text(text).unwrap()
                };
                let at = format!("{}1", (b'A' + index as u8) as char);
                session.set_input(&at, input).unwrap();
                let literal = question["mode"].as_str() == Some("literal");
                arguments.push(if literal {
                    if question["type"].as_str() == Some("boolean") {
                        text.to_owned()
                    } else {
                        format!("\"{}\"", text.replace('"', "\"\""))
                    }
                } else { at });
            }
            let formula = format!("={}({})", reading["function"].as_str().unwrap(), arguments.join(","));
            session.set_formula(&formula);
            let result = session.calculate();
            let expected = calc_verdict::decode_result(witness["expected_bits"].as_str().unwrap()).unwrap();
            count += 1;
            if result.result.as_ref() != Some(&expected) || !result.diagnostics.is_empty() {
                failures.push(format!("{} {formula}: expected {expected:?}, actual {:?}; {:?}", witness["id"], result.result, result.diagnostics));
            }
        }
    }
    assert_eq!(count, 3713);
    assert!(failures.is_empty(), "{} of{count} readings differ:\n{}", failures.len(), failures.join("\n"));
}

/// Fn7 preserves Excel's ASCII-prefix pass beside non-ASCII contexts, iteration
/// after punctuation, reversed relations and trailing characters. Native NLS
/// observations remain separate from these unchanged Excel answers.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zsn2__fn7_iteration_context_comparisons() {
    let mut session = Session::new("Sheet1").unwrap();
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/fn7_collation_context/oracle.capture.json"
    ), &mut session, |entry| {
        entry.cell.row() == 20 && entry.cell.column() == 10
            && !entry.formula.as_ref().unwrap().contains("SORT(")
    }, 210);
}

/// Fn6 distinguishes a trailing ignorable, a combining accent and an ordinary
/// primary after the same complete Jamo prefix. Different Jamo prefixes and
/// further Jamo/iteration units keep their typed refusal.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zsn2__identical_jamo_prefix_suffixes() {
    let mut session = Session::new("Sheet1").unwrap();
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/fn6_jamo_suffix/oracle.capture.json"
    ), &mut session, |entry| entry.cell.row() == 20 && entry.cell.column() == 10, 24);
}

/// The measured first primary group settles fn6's bare-Jamo boundaries while
/// incomplete same-group and contextual comparisons remain refused.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zsn25__fn6_iteration_primary_boundaries() {
    let mut session = Session::new("Sheet1").unwrap();
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/fn6_iteration_primary/oracle.capture.json"
    ), &mut session, |entry| entry.cell.row() == 20 && entry.cell.column() == 10, 60);
    for formula in [
        "=\"ᄀ\"=\"ㄱ\"",
        "=\"ᄃ\"<\"a\"",
        "=\"αᄀ\"<\"αㄱ\"",
        "=\"ᄀー\"<\"a\"",
        "=\"ά가z\"=\"ά가a\"",
        "=\"æ가z\"=\"æ가a\"",
        "=\"가ᅡz\"=\"가a\"",
        "=\"ㄱᅡz\"=\"ㄱa\"",
        "=\"α\u{e0100}가z\"=\"α\u{e0100}가a\"",
    ] {
        session.set_formula(formula);
        let calculated = session.calculate();
        assert!(calculated.result.is_none(), "{formula} remains outside the measured rule");
        assert!(calculated.diagnostics.iter().any(|diagnostic| matches!(diagnostic,
            Diagnostic::Eval(EvalFault::Function(FunctionFault::NotImplemented { .. }))
        )), "{formula}: {:?}", calculated.diagnostics);
    }
}

/// Fn7 pairs the same complete Jamo prefix and bare primary-group boundaries
/// with an ordinary Greek prefix, which the Windows suffix comparison retains.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zsn26__ordinary_prefix_before_jamo() {
    let mut session = Session::new("Sheet1").unwrap();
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/fn7_collation_contractions/oracle.capture.json"
    ), &mut session, |entry| {
        entry.cell.row() == 20 && entry.cell.column() == 10
            && matches!(entry.cell.sheet(), "q035" | "q036" | "q039" | "q040"
                | "q043" | "q044" | "q047" | "q048" | "q055" | "q056")
    }, 10);
}

/// Original fn6 Version-1/2 captures distinguish a lone high surrogate from a low
/// surrogate and an empty needle. All original primary questions remain intact,
/// including the two Version-2 SEARCH roots explained by valid-Unicode wildcard controls.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_b1l40_and42__surrogate_length_and_wildcard_units() {
    use calc_func::locale::{CalcEnvironment, DateSystem};
    let captures: [(&[u8], u32); 3] = [
        (include_bytes!("../../../crates/calc-func/tests/captures/fn6_utf16_needles/oracle.capture.json"), 1),
        (include_bytes!("../../../crates/calc-func/tests/captures/fn6_utf16_needles_compat1/oracle.capture.json"), 1),
        (include_bytes!("../../../crates/calc-func/tests/captures/fn6_utf16_needles_compat2/oracle.capture.json"), 2),
    ];
    let mut session = Session::new("Sheet1").unwrap();
    for (capture, version) in captures {
        let mut environment = CalcEnvironment::dates(DateSystem::System1900);
        environment.compatibility_version = version;
        session.set_environment(Some(environment));
        check_captured_questions(capture, &mut session, |entry| {
            entry.cell.row() == 20 && entry.cell.column() == 10
        }, 16);
    }
}

/// R24 retains logical literals beside explicit Missing slots for these four
/// financial entries. Empty references remain separate controls in the source.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_b1l37__financial_admission_uses_original_r24_answers() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_environment(Some(calc_func::locale::CalcEnvironment::dates(
        calc_func::locale::DateSystem::System1900,
    )));
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/dcztvv_r24/oracle.capture.json"
    ), &mut session, |f| f.cell.sheet() == "dcztvv_r24" && matches!(f.cell.row(),
        3040..=3042 | 3065..=3067 | 3287..=3289 | 3312..=3314 |
        3455..=3457 | 3480..=3482 | 3564..=3566 | 3589..=3591
    ), 24);
}

/// Fn4 qd225..qd236 use literal dates at the start of the 1900 calendar.
/// The original Excel error observations are retained unchanged.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn delivered_fn4_coupon_counts_at_calendar_start() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_environment(Some(calc_func::locale::CalcEnvironment::dates(
        calc_func::locale::DateSystem::System1900,
    )));
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/fn4_questions/functions-fn4-date-domain.capture.json"
    ), &mut session, |entry| entry.cell.row() == 20 && entry.cell.column() == 10
        && entry.cell.sheet().strip_prefix("qd").and_then(|s| s.parse::<usize>().ok())
            .is_some_and(|n| (225..=236).contains(&n)), 12);
}

/// All twelve months and seven weekdays per tag, plus the captured era boundaries.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn locale_tagged_date_names_follow_the_retained_fn3_readings() {
    use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
    let profile = format_profile(LocaleId::EnZa).with_separators(".", " ").unwrap();
    let mut session = Session::new("Sheet1").unwrap();
    // The tag supplies the names in all170 original readings. Their date-only
    // fields and spaces never read a host separator or AM/PM designator.
    for profile in [Some(profile), None] {
        session.set_environment(Some(CalcEnvironment {
            profile, ..CalcEnvironment::dates(DateSystem::System1900)
        }));
        check_captured_questions(include_bytes!(
            "../../../crates/calc-func/tests/captures/fn3_questions/oracle.capture.json"
        ), &mut session, |f| f.cell.sheet() == "fn3_text", 170);
    }
    for formula in [
        "=TEXT(45000,\"0.00\")",
        "=TEXT(45000,\"[$-ffff]mmmm\")",
        "=TEXT(45000,\"[$-407]m/d\")",
        "=TEXT(0.5,\"[$-407]hh:mm AM/PM\")",
        "=TEXT(45000,\"[>1.5][$-407]mmmm;[$-407]dddd\")",
        "=TEXT(45000,\"[$-407]   \")",
        "=FIXED(1.25,2,TRUE)",
    ] {
        session.set_formula(formula);
        assert!(session.calculate().result.is_none(), "{formula} still needs host context");
    }
}

/// Fn4 text-controls qt031..qt096 and list-controls ql041..ql085, unchanged live
/// captures from Excel 16.0.20527.20088, recorded en-ZA with dot/space separators.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn delivered_fn4_digit_scripts_and_numeric_date_widths() {
    use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
    let mut session = Session::new("Sheet1").unwrap();
    let profile = format_profile(LocaleId::EnZa).with_separators(".", " ").unwrap();
    session.set_environment(Some(CalcEnvironment {
        profile: Some(profile), ..CalcEnvironment::dates(DateSystem::System1900)
    }));
    let primary = |entry: &calc_verdict::FormulaCell, low: usize, high: usize| {
        entry.cell.row() == 20 && entry.cell.column() == 10
            && entry.cell.sheet().get(2..).and_then(|s| s.parse::<usize>().ok())
                .is_some_and(|n| (low..=high).contains(&n))
    };
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/fn4_questions/functions-fn4-text-controls.capture.json"
    ), &mut session, |entry| primary(entry, 31, 96), 66);
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/fn4_questions/functions-fn4-list-controls.capture.json"
    ), &mut session, |entry| primary(entry, 41, 85), 45);
}

/// Fn4 qt001..qt030 distinguish General display, active @ and quoted @.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn delivered_fn4_numeric_text_placeholders_use_general_display() {
    use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
    let mut session = Session::new("Sheet1").unwrap();
    let profile = format_profile(LocaleId::EnZa).with_separators(".", " ").unwrap();
    session.set_environment(Some(CalcEnvironment {
        profile: Some(profile), ..CalcEnvironment::dates(DateSystem::System1900)
    }));
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/fn4_questions/functions-fn4-text-controls.capture.json"
    ), &mut session, |entry| entry.cell.row() == 20 && entry.cell.column() == 10
        && entry.cell.sheet().strip_prefix("qt").and_then(|s| s.parse::<usize>().ok())
            .is_some_and(|n| (1..=30).contains(&n)), 30);
}

/// Fn4 ql104..ql133 distinguish MODE's ignored direct text/logicals from numbers.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn delivered_fn4_mode_ignores_text_and_logicals() {
    let mut session = Session::new("Sheet1").unwrap();
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/fn4_questions/functions-fn4-list-controls.capture.json"
    ), &mut session, |entry| entry.cell.row() == 20 && entry.cell.column() == 10
        && entry.cell.sheet().strip_prefix("ql").and_then(|s| s.parse::<usize>().ok())
            .is_some_and(|n| (104..=133).contains(&n)), 30);
}

/// Fn4 argument questions retain literal, held logical and error-order controls.
/// Blank-reference questions are separate; this session supplies the recorded logical cells.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn delivered_fn4_scalar_logical_positions_and_error_order() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_environment(Some(calc_func::locale::CalcEnvironment::dates(
        calc_func::locale::DateSystem::System1900,
    )));
    session.set_input("B1", InputValue::logical(true)).unwrap();
    session.set_input("B2", InputValue::logical(false)).unwrap();
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/fn4_questions/functions-fn4-arguments.capture.json"
    ), &mut session, |entry| entry.cell.row() == 20 && entry.cell.column() == 10
        && entry.cell.sheet().strip_prefix("qa").and_then(|s| s.parse::<usize>().ok())
            .is_some_and(|n| n <= 148 && !matches!(n,
                4 | 12 | 16 | 24 | 32 | 40 | 50 | 54 | 62 | 72 | 82 | 92 | 102 | 112 | 120 | 124 | 128 | 132)), 130);
}

/// The original fn4 blank-reference questions use genuine empty cells, not missing inputs.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn delivered_fn4_blank_date_references_read_serial_zero() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_environment(Some(calc_func::locale::CalcEnvironment::dates(
        calc_func::locale::DateSystem::System1900,
    )));
    session.set_input("B1", InputValue::blank()).unwrap();
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/fn4_questions/functions-fn4-arguments.capture.json"
    ), &mut session, |entry| entry.cell.row() == 20 && entry.cell.column() == 10
        && entry.cell.sheet().strip_prefix("qa").and_then(|s| s.parse::<usize>().ok())
            .is_some_and(|n| matches!(n,
                4 | 12 | 16 | 24 | 32 | 40 | 50 | 54 | 62 | 72 | 82 | 92 | 102 | 112 | 120 | 124 | 128 | 132)), 18);
}

/// Fn4 q093..q096 read a genuine typed #BUSY! input and retain Excel's four results.
/// These are new observations of this question workbook, not a recapture of dc_nzpw D157.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn delivered_fn4_busy_error_is_typed_through_the_application() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_input("B1", InputValue::error(WorksheetErrorCode::Busy)).unwrap();
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/fn4_questions/functions-fn4.capture.json"
    ), &mut session, |entry| entry.cell.row() == 20 && entry.cell.column() == 10
        && entry.cell.sheet().get(1..).and_then(|s| s.parse::<usize>().ok())
            .is_some_and(|n| (93..=96).contains(&n)), 4);
}

/// Fn2 roots 001..010 and 232..239 retain direct errors, actual one-cell arrays,
/// and a one-cell reference containing the same #N/A input as distinct origins.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn scalar_errors_keep_their_array_argument_origins() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_form(calc_fml::FormulaForm::Dynamic);
    session.add_sheet("Data").unwrap();
    session.set_input("Data!B1", InputValue::text("x").unwrap()).unwrap();
    session.set_input("Data!B2", InputValue::text("y").unwrap()).unwrap();
    session.set_input("Data!B3", InputValue::text("x").unwrap()).unwrap();
    session.set_input("Data!C1", InputValue::error(WorksheetErrorCode::NA)).unwrap();
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/fn2_questions/oracle.capture.json"
    ), &mut session, |f| {
        f.cell.sheet().strip_prefix("fn2_q").and_then(|n| n.parse::<u32>().ok())
            .is_some_and(|n| matches!(n, 1..=10 | 232..=239))
    }, 18);
}

/// Fn3 records all three directions for seven exact prefixes, three punctuation units and
/// three iteration/prolonged-sound marks, plus eight shorter-prefix controls.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn measured_iteration_suffixes_follow_their_exact_prefixes() {
    use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
    let mut session = Session::new("Sheet1").unwrap();
    session.set_form(calc_fml::FormulaForm::Dynamic);
    session.set_environment(Some(CalcEnvironment {
        profile: Some(format_profile(LocaleId::EnZa).with_separators(".", " ").unwrap()),
        ..CalcEnvironment::dates(DateSystem::System1900)
    }));
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/fn3_questions/oracle.capture.json"
    ), &mut session, |f| f.cell.sheet() == "fn3_collation"
        && matches!(f.cell.row(), 1 | 2 | 15 | 16 | 31 | 32 | 47 | 48 | 101..=289), 197);
}

/// Consequences of fn7's measured positive punctuation primary: an exact
/// primary prefix is smaller than its extension, and the punctuation primary
/// follows the measured Han repeat primary in group05. These were refusal
/// controls before that measurement; the consequences are not new Excel readings.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn measured_iteration_primary_and_unmeasured_contexts() {
    let mut session = Session::new("Sheet1").unwrap();
    for (formula, expected) in [
        ("=\"á-ゝ\"<\"a-\"", false),
        ("=\"ａ-ゝ\"<\"a-\"", false),
        ("=\"時A-ゝ\"<\"時a-\"", false),
        ("=\"時-ゝx\"<\"時-\"", false),
        ("=\"時-ゝ\"<\"時ゝ\"", false),
        ("=\"時-'ゝ\"<\"時\"", false),
    ] {
        session.set_formula(formula);
        let calculated = session.calculate();
        assert_eq!(calculated.result, Some(CalcValue::logical(expected)), "{formula}");
        assert!(calculated.diagnostics.is_empty(), "{formula}: {:?}", calculated.diagnostics);
    }
    for formula in [
        "=\"時-́ゝ\"<\"時-\"",     // A mark separates punctuation and iteration.
        "=\"時‐́ゝ\"<\"時‐\"",
        "=\"時–ゝ\"<\"時–\"",     // Unmeasured punctuation.
        "=\"時-ゞ\"<\"時-\"",     // Unmeasured iteration after punctuation.
        "=\"가-ゝ\"<\"가-\"",    // Jamo suffix depends on its preceding primary.
        "=\"時-­ゝ\"<\"時-\"",     // An ignorable separates the two units.
        "=\"時-ゝゝ\"<\"時-\"",    // Two iterations after punctuation.
    ] {
        session.set_formula(formula);
        let calculated = session.calculate();
        assert!(calculated.result.is_none(), "{formula}: {:?}", calculated.result);
        assert!(calculated.diagnostics.iter().any(|diagnostic| matches!(diagnostic,
            Diagnostic::Eval(EvalFault::Function(FunctionFault::NotImplemented { .. }))
        )), "{formula}: {:?}", calculated.diagnostics);
    }
}

/// Retained Dynamic fn2 q193: CHOOSE materializes selected blank references into zeros when
/// an array selector lifts the call. Scalar CHOOSE retains its reference origin.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_choose_array_selector_materializes_blank_references() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_form(calc_fml::FormulaForm::Dynamic);
    session.add_sheet("Data").unwrap();
    session.set_input("Data!Z1", InputValue::blank()).unwrap();
    session.set_input("Data!Z2", InputValue::blank()).unwrap();
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/fn2_questions/oracle.capture.json"
    ), &mut session, |f| f.cell.sheet() == "fn2_q193", 1);
    for formula in ["=ISBLANK(CHOOSE(1,Data!Z1,Data!Z2))", "=ISREF(CHOOSE(1,Data!Z1,Data!Z2))"] {
        session.set_formula(formula);
        let calculated = session.calculate();
        assert_eq!(calculated.result, Some(CalcValue::logical(true)), "{formula}: {:?}", calculated.diagnostics);
    }
}

/// Retained fn2 q187: an omitted optional LAMBDA argument reaches COUNTBLANK as Missing,
/// which is #VALUE!, rather than a blank reference to count or a typed refusal.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_countblank_optional_lambda_omission_is_a_value_error() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_form(calc_fml::FormulaForm::Dynamic);
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/fn2_questions/oracle.capture.json"
    ), &mut session, |f| f.cell.sheet() == "fn2_q187", 1);
    session.set_input("A1", InputValue::blank()).unwrap();
    session.set_formula("=COUNTBLANK(A1)");
    let reference = session.calculate();
    assert_eq!(reference.result, Some(CalcValue::number(1.0)), "{:?}", reference.diagnostics);
}

/// Implementation contract: selectors transport the chosen closure and its lexical bindings.
/// OneCalc inspects raw values; only workbook publication projects their worksheet face.
/// Fn5's composed-selector questions remain unanswered, so these are not Excel parity claims.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn selected_callables_keep_identity_before_cell_publication() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_form(calc_fml::FormulaForm::Dynamic);
    for selector in ["IFS(TRUE,ff)", "SWITCH(1,1,ff)"] {
        session.set_formula(&format!("=LET(ss,11,ff,LAMBDA(xx,ss+xx),{selector})"));
        let raw = session.calculate();
        assert!(raw.diagnostics.is_empty(), "{selector}: {:?}", raw.diagnostics);
        assert!(raw.result.as_ref().unwrap().callable_value().is_some());
        session.set_formula(&format!("=LET(ss,11,ff,LAMBDA(xx,ss+xx),gg,{selector},gg(7))"));
        let called = session.calculate();
        assert_eq!(called.result, Some(CalcValue::number(18.0)), "{selector}: {:?}", called.diagnostics);
        assert!(called.diagnostics.is_empty());
    }
    for selector in ["IFS(TRUE,HSTACK(ff,ff))", "SWITCH(1,1,HSTACK(ff,ff))"] {
        session.set_formula(&format!("=LET(ss,11,ff,LAMBDA(xx,ss+xx),{selector})"));
        let raw = session.calculate();
        assert!(raw.diagnostics.is_empty(), "{selector}: {:?}", raw.diagnostics);
        let array = raw.result.as_ref().unwrap().as_array().unwrap();
        assert_eq!((array.shape().rows, array.shape().cols), (1, 2));
        assert!(array.iter_row_major().all(|value| value.callable_value().is_some()));
        session.set_formula(&format!("=LET(ss,11,ff,LAMBDA(xx,ss+xx),gg,INDEX({selector},1,2),gg(7))"));
        let called = session.calculate();
        assert_eq!(called.result, Some(CalcValue::number(18.0)), "{selector}: {:?}", called.diagnostics);
        assert!(called.diagnostics.is_empty());
    }
}

/// Fn2 roots011/013/014 and their ROWS/COLUMNS observers establish zero-count and one-cell
/// behavior. Larger short vectors and matrices follow the documented WRAPROWS/WRAPCOLS contract.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn wrapping_keeps_short_vectors_and_validates_counts_and_shapes() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_form(calc_fml::FormulaForm::Dynamic);
    session.add_sheet("Data").unwrap();
    session.set_input("Data!B1", InputValue::text("x").unwrap()).unwrap();
    let mut cases = Vec::new();
    for (formula, expected) in [
        ("=ROWS(WRAPCOLS({1,2,3},5,\"pad\"))", 3.0),
        ("=COLUMNS(WRAPCOLS({1,2,3},5,\"pad\"))", 1.0),
        ("=ROWS(WRAPROWS({1;2;3},5,\"pad\"))", 1.0),
        ("=COLUMNS(WRAPROWS({1;2;3},5,\"pad\"))", 3.0),
        ("=ROWS(WRAPCOLS({\"2\"},2,\"pad\"))", 1.0),
        ("=COLUMNS(WRAPCOLS({\"2\"},2,\"pad\"))", 1.0),
        ("=ROWS(WRAPCOLS(Data!B1,2,\"pad\"))", 1.0),
        ("=COLUMNS(WRAPCOLS(Data!B1,2,\"pad\"))", 1.0),
        ("=ROWS(WRAPCOLS({1,2,3},2))", 2.0),
        ("=COLUMNS(WRAPCOLS({1,2,3},2))", 2.0),
    ] { cases.push((formula, CalcValue::number(expected))); }
    for formula in [
        "=WRAPCOLS(\"2\",0,\"pad\")", "=WRAPROWS(\"2\",0,\"pad\")",
        "=WRAPCOLS({1,2},-1)", "=WRAPROWS({1;2},-1)",
        "=WRAPCOLS({1,2},0.5)", "=WRAPROWS({1;2},0.5)",
    ] { cases.push((formula, CalcValue::error(WorksheetErrorCode::Num))); }
    for formula in ["=WRAPCOLS({1,2;3,4},2)", "=WRAPROWS({1,2;3,4},2)"] {
        cases.push((formula, CalcValue::error(WorksheetErrorCode::Value)));
    }
    for formula in ["=INDEX(WRAPCOLS({1,2,3},2),2,2)", "=INDEX(WRAPROWS({1,2,3},2),2,2)"] {
        cases.push((formula, CalcValue::error(WorksheetErrorCode::NA)));
    }
    cases.push(("=INDEX(WRAPCOLS(Data!B1,2,\"pad\"),1,1)",
        CalcValue::text(calc_func::value::ExcelText::from_interop_assignment("x"))));
    let mut failures = Vec::new();
    for (formula, expected) in cases {
        session.set_formula(formula);
        let calculated = session.calculate();
        if calculated.result != Some(expected.clone()) || !calculated.diagnostics.is_empty() {
            failures.push(format!("{formula}: expected {expected:?}, got {:?}; {:?}",
                calculated.result, calculated.diagnostics));
        }
    }
    assert!(failures.is_empty(), "{} wrapping differences:\n{}", failures.len(), failures.join("\n"));
}

/// Captured fn2 helper controls distinguish first/later scalar source errors, reference/array
/// origins, and single-cell BYROW/BYCOL results from unsupported nested MAP/SCAN results.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn lambda_helpers_follow_the_retained_source_and_result_rules() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_form(calc_fml::FormulaForm::Dynamic);
    session.add_sheet("Data").unwrap();
    session.set_input("Data!C1", InputValue::error(WorksheetErrorCode::NA)).unwrap();
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/fn2_questions/oracle.capture.json"
    ), &mut session, |f| {
        f.cell.sheet().strip_prefix("fn2_q").and_then(|n| n.parse::<u32>().ok())
            .is_some_and(|n| matches!(n, 31 | 33..=42 | 88..=92))
    }, 16);
}

/// Fn2 root165 preserves a formed later argument error before a zero-dimension result;
/// root164 and the existing lifted/default calls retain their captured behavior.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn sequence_argument_errors_precede_shape_validation() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_form(calc_fml::FormulaForm::Dynamic);
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/fn2_questions/oracle.capture.json"
    ), &mut session, |f| {
        f.cell.sheet().strip_prefix("fn2_q").and_then(|n| n.parse::<u32>().ok())
            .is_some_and(|n| matches!(n, 61..=65 | 80..=85 | 137 | 164..=165 | 173..=175))
    }, 17);
}

/// Fn2 roots022/208 distinguish expression errors and an already selected scalar result
/// from invalid conditions and selectors that really need array lifting.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn scalar_selection_keeps_its_shape_before_unused_arrays() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_form(calc_fml::FormulaForm::Dynamic);
    session.add_sheet("Data").unwrap();
    session.set_input("Data!Z1", InputValue::blank()).unwrap();
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/fn2_questions/oracle.capture.json"
    ), &mut session, |f| {
        f.cell.sheet().strip_prefix("fn2_q").and_then(|n| n.parse::<u32>().ok())
            .is_some_and(|n| matches!(n, 16 | 21..=22 | 84..=85 | 153 | 170..=171 | 194..=195 | 208..=209))
    }, 12);
}

/// Fn2 roots078/079 retain the first match of each lifted all-match result and
/// distinguish capture-group mode on the supported group-free pattern.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn regexextract_retains_the_captured_lifted_results() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_form(calc_fml::FormulaForm::Dynamic);
    check_captured_questions(include_bytes!(
        "../../../crates/calc-func/tests/captures/fn2_questions/oracle.capture.json"
    ), &mut session, |f| matches!(f.cell.sheet(), "fn2_q078" | "fn2_q079"), 2);
}

/// Fn5 establishes horizontal all-match results and one outer capture. Group-free
/// capture mode is Value even without a match; unsupported captures remain refused.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn regexextract_publishes_all_matches_and_keeps_unmeasured_modes_refused() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_form(calc_fml::FormulaForm::Dynamic);
    let text = |s| CalcValue::text(calc_func::value::ExcelText::from_interop_assignment(s));
    let mut failures = Vec::new();
    for (formula, expected) in [
        ("=ROWS(REGEXEXTRACT(\"DylanWilliams\",\"[A-Z][a-z]+\",1))", CalcValue::number(1.0)),
        ("=COLUMNS(REGEXEXTRACT(\"DylanWilliams\",\"[A-Z][a-z]+\",1))", CalcValue::number(2.0)),
        ("=INDEX(REGEXEXTRACT(\"DylanWilliams\",\"[A-Z][a-z]+\",1),1,2)", text("Williams")),
        ("=REGEXEXTRACT(\"DylanWilliams\",\"[A-Z][a-z]+\")", text("Dylan")),
        ("=REGEXEXTRACT(\"Dylan\",\"[A-Z][a-z]+\",1)", text("Dylan")),
        ("=REGEXEXTRACT(\"123\",\"[A-Z][a-z]+\",1)", CalcValue::error(WorksheetErrorCode::NA)),
        ("=REGEXEXTRACT(\"a12b34\",\"[0-9]+\",2)", CalcValue::error(WorksheetErrorCode::Value)),
        ("=REGEXEXTRACT(\"a12b34\",\"[0-9]+\",3)", CalcValue::error(WorksheetErrorCode::Value)),
        ("=REGEXEXTRACT(\"abc\",\"[0-9]+\",2)", CalcValue::error(WorksheetErrorCode::Value)),
        ("=REGEXEXTRACT(\"a12b\",\"([0-9]+)\",2)", text("12")),
    ] {
        session.set_formula(formula);
        let calculated = session.calculate();
        if calculated.result != Some(expected.clone()) || !calculated.diagnostics.is_empty() {
            failures.push(format!("{formula}: expected {expected:?}, got {:?}; {:?}",
                calculated.result, calculated.diagnostics));
        }
    }
    for formula in [
        "=REGEXEXTRACT(\"abc\",\"([0-9]+)\",2)",
        "=REGEXEXTRACT(\"a12b\",\"(([0-9]+))\",2)",
        "=REGEXEXTRACT(\"a12b\",\"([0-9]+)(b)\",2)",
    ] {
        session.set_formula(formula);
        let calculated = session.calculate();
        assert!(calculated.result.is_none(), "{formula}: {:?}", calculated.result);
        assert!(calculated.diagnostics.iter().any(|d| matches!(d,
            Diagnostic::Eval(EvalFault::Function(FunctionFault::NotImplemented { .. }))
        )), "{formula}: {:?}", calculated.diagnostics);
    }
    assert!(failures.is_empty(), "{} extraction differences:\n{}", failures.len(), failures.join("\n"));
}

/// Recorded POISSON branch/control answers from OxFunc-Evidence's
/// poisson.dist-branch-00128..00129, 00084..00085 and 00224..00225.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn poisson_validates_raw_counts_before_the_zero_count_exponential() {
    let mut session = Session::new("Inputs").unwrap();
    for function in ["POISSON.DIST", "POISSON"] {
        for flag in ["FALSE", "TRUE"] {
            for (count, mean, expected) in [
                ("0", "-1", CalcValue::number(f64::from_bits(0x4005_bf0a_8b14_5769))),
                ("-0.5", "0", CalcValue::error(WorksheetErrorCode::Num)),
                ("1", "-1", CalcValue::error(WorksheetErrorCode::Num)),
            ] {
                let formula = format!("={function}({count},{mean},{flag})");
                session.set_formula(&formula);
                let calculated = session.calculate();
                assert_eq!(calculated.result, Some(expected), "{formula}: {:?}", calculated.diagnostics);
                assert!(calculated.diagnostics.is_empty());
            }
        }
    }
}

/// OxFunc-Evidence w111-distribution-heldout-typed-20260929, NORM.S.DIST
/// cases 00000..00206: the cumulative position reads logical text, not numeric text.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn distribution_cumulative_flags_read_logical_text() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_formula_cell("J20").unwrap();
    session.set_input("A100", InputValue::number(0.0).unwrap()).unwrap();
    for (flag, expected) in [
        ("TrUe", CalcValue::number(0.5)),
        ("FAlSE", CalcValue::number(f64::from_bits(0x3fd9_8845_33d4_3651))),
        ("00", CalcValue::error(WorksheetErrorCode::Value)),
        ("+0", CalcValue::error(WorksheetErrorCode::Value)),
        ("1.0", CalcValue::error(WorksheetErrorCode::Value)),
        ("0%", CalcValue::error(WorksheetErrorCode::Value)),
        ("\u{b}TRUE", CalcValue::error(WorksheetErrorCode::Value)),
        ("TRUE\r", CalcValue::error(WorksheetErrorCode::Value)),
        ("\u{2003}TRUE", CalcValue::error(WorksheetErrorCode::Value)),
        ("FAL\u{17f}E", CalcValue::error(WorksheetErrorCode::Value)),
    ] {
        session.set_input("C4", InputValue::text(flag).unwrap()).unwrap();
        for argument in [format!("\"{flag}\""), "C4".to_owned()] {
            let formula = format!("=NORM.S.DIST(A100,{argument})");
            session.set_formula(&formula);
            let calculated = session.calculate();
            assert_eq!(calculated.result, Some(expected.clone()), "{formula:?}: {:?}", calculated.diagnostics);
            assert!(calculated.diagnostics.is_empty());
        }
    }
    session.set_formula("=NORM.S.DIST(A100,CHOOSE({1;2},TRUE,\"TrUe\"))");
    let expected = CalcValue::array(calc_func::value::CalcArray::from_rows(vec![
        vec![CalcValue::number(0.5)], vec![CalcValue::number(0.5)],
    ]).unwrap());
    assert_eq!(session.calculate().result, Some(expected));
}

/// OxFunc-Evidence w111-numeric-text-core-heldout-20260929, INT direct/reference
/// pairs 00600/00601, 01092/01093, 01112/01113 and 01122/01123.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_parenthesized_percent_text_keeps_its_suffix() {
    use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
    let mut profile = format_profile(LocaleId::EnZa);
    // Actual separators retained in the Excel captures, independent of locale defaults.
    profile.decimal_separator = ".";
    profile.thousands_separator = " ";
    let mut session = Session::new("Sheet1").unwrap();
    session.set_formula_cell("J20").unwrap();
    session.set_environment(Some(CalcEnvironment {
        profile: Some(profile), ..CalcEnvironment::dates(DateSystem::System1900)
    }));
    for text in ["(19.811671778304706)%", "(2e-2)%", "(2e-2) %", "(200e-2)%"] {
        session.set_input("B1", InputValue::text(text).unwrap()).unwrap();
        for argument in [format!("\"{text}\""), "B1".to_owned()] {
            let formula = format!("=INT({argument})");
            session.set_formula(&formula);
            let calculated = session.calculate();
            assert_eq!(calculated.result, Some(CalcValue::number(-1.0)), "{formula}: {:?}", calculated.diagnostics);
            assert!(calculated.diagnostics.is_empty());
        }
    }
}

/// OxFunc-Evidence cache/oracle-20430/build-20430/BAHTTEXT.jsonl,
/// oc1210/1211/1849 with zero, signed-zero and satang boundary controls.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_bahttext_retains_the_sign_of_a_negative_amount() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_formula_cell("J20").unwrap();
    session.set_formula("=BAHTTEXT(B1)");
    for (bits, expected) in [
        (0xbf50_624d_d2f1_a9fc, "ลบศูนย์บาทถ้วน"),
        (0xbf70_624d_d2f1_a9fc, "ลบศูนย์บาทถ้วน"),
        (0xbf72_ec6b_ce85_33b1, "ลบศูนย์บาทถ้วน"),
        (0x0000_0000_0000_0000, "ศูนย์บาทถ้วน"),
        (0x8000_0000_0000_0000, "ศูนย์บาทถ้วน"),
        (0x3f70_624d_d2f1_a9fc, "ศูนย์บาทถ้วน"),
        (0x3f74_7ae1_47ae_147b, "หนึ่งสตางค์"),
        (0xbf74_7ae1_47ae_147b, "ลบหนึ่งสตางค์"),
    ] {
        session.set_input("B1", InputValue::number(f64::from_bits(bits)).unwrap()).unwrap();
        let calculated = session.calculate();
        let expected = CalcValue::text(calc_func::value::ExcelText::from_interop_assignment(expected));
        assert_eq!(calculated.result, Some(expected), "{bits:#018x}: {:?}", calculated.diagnostics);
        assert!(calculated.diagnostics.is_empty());
    }
}

/// OxFunc-Evidence b028-agg-recheck-001 logical/text-number pairs; erf-arraylift-001
/// erfrecheck-00005..00024; W089 seed 00255 and W092 successor 00209..00211.
/// INTL calls: unswept-structural-sweep-001 cases 00445 and 00800.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_logical_scalar_admission() {
    use calc_func::locale::{CalcEnvironment, DateSystem};
    let mut session = Session::new("Sheet1").unwrap();
    session.set_environment(Some(CalcEnvironment::dates(DateSystem::System1900)));
    let cases = [
        ("DELTA", "", 0x0000_0000_0000_0000),
        ("FACTDOUBLE", "", 0x4000_0000_0000_0000),
        ("GESTEP", "", 0x3ff0_0000_0000_0000),
        ("MULTINOMIAL", "", 0x3ff0_0000_0000_0000),
        ("NETWORKDAYS", ",2", 0x3ff0_0000_0000_0000),
        ("SQRTPI", "", 0x4004_0d93_1ff6_2705),
        ("WORKDAY", ",2", 0x4010_0000_0000_0000),
        ("YEARFRAC", ",2", 0x0000_0000_0000_0000),
        ("ERF", "", 0x3fef_d9ae_1427_95e3),
        ("ERF.PRECISE", "", 0x3fef_d9ae_1427_95e3),
        ("SERIESSUM", ",1,2,{1,2}", 0x4032_0000_0000_0000),
    ];
    // Run every retained logical case before the assertion, so a red run records each one.
    let mut actuals: Vec<_> = cases.iter().map(|(name, rest, _)| {
        session.set_formula(&format!("={name}(TRUE{rest})"));
        (name, session.calculate().result)
    }).collect();
    let mut expected: Vec<_> = cases.iter().map(|(name, _, _)| {
        (name, Some(CalcValue::error(WorksheetErrorCode::Value)))
    }).collect();
    // ERFC("2") has a retained numeric mismatch owned by the numeric lane. Its exact
    // expected bits stay in the replay; these controls cover logical admission only.
    let more = [("TBILLEQ", ",2,2"), ("TBILLPRICE", ",2,2"), ("TBILLYIELD", ",2,2"),
        ("NETWORKDAYS.INTL", ",2"), ("WORKDAY.INTL", ",2"), ("ERFC", ""), ("ERFC.PRECISE", "")];
    for (name, rest) in &more {
        session.set_formula(&format!("={name}(TRUE{rest})"));
        actuals.push((name, session.calculate().result));
        expected.push((name, Some(CalcValue::error(WorksheetErrorCode::Value))));
    }
    assert_eq!(actuals, expected);
    for (name, rest, bits) in cases {
        let formula = format!("={name}(\"2\"{rest})");
        session.set_formula(&formula);
        assert_eq!(session.calculate().result, Some(CalcValue::number(f64::from_bits(bits))), "{formula}");
    }
    let row = CalcValue::array(calc_func::value::CalcArray::from_rows(vec![vec![
        CalcValue::error(WorksheetErrorCode::Value), CalcValue::error(WorksheetErrorCode::Value),
    ]]).unwrap());
    for arguments in ["{TRUE,TRUE},1,2,{1,2}", "TRUE,{1,1},2,{1,2}", "TRUE,1,{2,2},{1,2}"] {
        session.set_formula(&format!("=SERIESSUM({arguments})"));
        assert_eq!(session.calculate().result, Some(row.clone()));
    }
}

/// W111 address-blank-numeric-text-20260929 cases 0000/0002/0008/0010:
/// an omitted column fails, while an omitted row and a blank column are allowed in R1C1.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_address_omitted_column_differs_from_blank() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_formula_cell("J70").unwrap();
    session.set_input("A1", InputValue::number(3.0).unwrap()).unwrap();
    session.set_input("A2", InputValue::blank()).unwrap();
    session.set_input("A11", InputValue::number(2.0).unwrap()).unwrap();
    session.set_input("A12", InputValue::blank()).unwrap();
    session.set_input("A21", InputValue::number(4.0).unwrap()).unwrap();
    session.set_input("A31", InputValue::logical(false)).unwrap();
    session.set_input("A41", InputValue::text("Alpha").unwrap()).unwrap();
    session.set_formula("=ADDRESS(A1,,A21,A31,A41)");
    assert_eq!(session.calculate().result, Some(CalcValue::error(WorksheetErrorCode::Value)));
    for (formula, expected) in [
        ("=ADDRESS(,A11,A21,A31,A41)", "Alpha!RC[2]"),
        ("=ADDRESS(A2,A11,A21,A31,A41)", "Alpha!RC[2]"),
        ("=ADDRESS(A1,A12,A21,A31,A41)", "Alpha!R[3]C"),
    ] {
        session.set_formula(formula);
        assert_eq!(session.calculate().result, Some(CalcValue::text(
            calc_func::value::ExcelText::from_interop_assignment(expected))), "{formula}");
    }
}

/// The retained FIXED witnesses include 22 typed Excel readings that emit neither separator.
/// They supply the expected text; absent-context and rounded-grouping controls check which
/// environment fields the ordinary application route actually needs, without inventing a locale.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn fixed_requires_a_profile_only_for_emitted_separators() {
    use calc_func::locale::{CalcEnvironment, DateSystem};
    let set = calc_verdict::WitnessSet::decode(include_bytes!(
        "../../../crates/calc-func/tests/witnesses/FIXED.json"
    )).unwrap();
    let mut count = 0;
    let mut failures = Vec::new();
    for environment in [None, Some(CalcEnvironment::dates(DateSystem::System1900))] {
        for row in &set.witnesses {
            let args: Vec<_> = row.args.iter().map(|value|
                calc_verdict::decode_argument(value, "FIXED").unwrap()
            ).collect();
            let numbers: Vec<_> = args.iter().map(|value| value.as_number().unwrap()).collect();
            if numbers.len() != 3 || numbers[1].trunc() > 0.0 || numbers[2] == 0.0 { continue; }
            let mut session = Session::new("Sheet1").unwrap();
            session.set_environment(environment);
            session.set_formula_cell("J20").unwrap();
            for (cell, value) in ["A1", "B1", "C1"].into_iter().zip(numbers) {
                session.set_input(cell, InputValue::number(value).unwrap()).unwrap();
            }
            session.set_formula("=FIXED(A1,B1,C1)");
            let expected = calc_verdict::decode_result(&row.expected).unwrap();
            let result = session.calculate();
            count += 1;
            if result.result.as_ref() != Some(&expected) || !result.diagnostics.is_empty() {
                failures.push(format!("{}: expected {expected:?}, calculated {:?}; {:?}",
                    row.id, result.result, result.diagnostics));
            }
        }
    }
    assert_eq!(count, 44, "all 22 retained inputs in both absent-profile contexts must run");
    assert!(failures.is_empty(), "{} of {count} readings differ:\n{}", failures.len(), failures.join("\n"));
    for environment in [None, Some(CalcEnvironment::dates(DateSystem::System1900))] {
        let mut session = Session::new("Sheet1").unwrap();
        session.set_environment(environment);
        for formula in ["=FIXED(1.5,1,TRUE)", "=FIXED(1000,0,FALSE)",
            "=FIXED(999.5,0,FALSE)", "=FIXED(\"1,5\",0,TRUE)", "=DOLLAR(1,0)"] {
            session.set_formula(formula);
            let result = session.calculate();
            assert!(result.result.is_none(), "{formula}: {:?}", result.result);
            let [Diagnostic::Eval(EvalFault::Function(FunctionFault::NotImplemented { id, .. }))]
                = result.diagnostics.as_slice() else {
                panic!("{formula} needs its typed function refusal: {:?}", result.diagnostics);
            };
            let name = if formula.starts_with("=DOLLAR") { "DOLLAR" } else { "FIXED" };
            assert_eq!(*id, catalogue().resolve_surface(Notation::Call, name).unwrap().id());
        }
        // Contract control, not a new Excel reading: grouping needs rounded whole digits.
        // A three-digit result has no separator.
        session.set_formula("=FIXED(999.4,0,FALSE)");
        assert_eq!(session.calculate().result, Some(CalcValue::text(
            calc_func::value::ExcelText::from_interop_assignment("999"))));
    }
}

/// W111 date-typed 00155 and refinement 00772..00868; depreciation-options
/// 00000/00011 and independent 00092/00208; locale-profile 00193 and 00252.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_optional_placeholders_are_supplied_values() {
    use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
    let mut profile = format_profile(LocaleId::EnZa);
    profile.decimal_separator = ".";
    profile.thousands_separator = " ";
    let mut session = Session::new("Sheet1").unwrap();
    session.set_environment(Some(CalcEnvironment {
        profile: Some(profile), ..CalcEnvironment::dates(DateSystem::System1900)
    }));
    let formulas = ["=WEEKDAY(0,)", "=WEEKDAY(1,)", "=WEEKDAY(FALSE,)",
        "=WEEKDAY(TRUE,)", "=WEEKDAY(\"1\",)", "=WEEKDAY(,)",
        "=DB(1000,100,10,2,)", "=DDB(1000,100,10,2,)"];
    let mut actuals: Vec<_> = formulas.iter().map(|formula| {
        session.set_formula(formula);
        (*formula, session.calculate().result)
    }).collect();
    let mut expected: Vec<_> = formulas.iter().map(|formula| {
        (*formula, Some(CalcValue::error(WorksheetErrorCode::Num)))
    }).collect();
    // Recorded missing-argument controls differ between these two format functions.
    for (formula, shown) in [("=DOLLAR(1234.5,)", "R1 235"), ("=FIXED(1234.5,)", "1 234.50")] {
        session.set_formula(formula);
        actuals.push((formula, session.calculate().result));
        expected.push((formula, Some(CalcValue::text(
            calc_func::value::ExcelText::from_interop_assignment(shown)))));
    }
    assert_eq!(actuals, expected);
    // Leaving the return type out still uses WEEKDAY's default.
    session.set_formula("=WEEKDAY(1)");
    assert_eq!(session.calculate().result, Some(CalcValue::number(1.0)));
}

/// W111 locale-profile-20260929: 00145/00146/00151..00154, zero after currency rounding.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_dollar_retains_a_negative_sign_after_rounding() {
    use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
    let mut profile = format_profile(LocaleId::EnZa);
    profile.decimal_separator = ".";
    profile.thousands_separator = " ";
    let mut session = Session::new("Sheet1").unwrap();
    session.set_environment(Some(CalcEnvironment {
        profile: Some(profile), ..CalcEnvironment::dates(DateSystem::System1900)
    }));
    for (bits, places, expected) in [
        (0xbf74_7ae1_47ae_147b, 0, "-R0"), (0xbf74_7ae1_47ae_147b, 1, "-R0.0"),
        (0xc005_6666_6666_6666, -3, "R0"), (0xc005_6666_6666_6666, -1, "R0"),
        (0xbf74_7ae1_47ae_147b, -3, "R0"), (0xbf74_7ae1_47ae_147b, -1, "R0"),
    ] {
        session.set_input("A100", InputValue::number(f64::from_bits(bits)).unwrap()).unwrap();
        session.set_formula(&format!("=DOLLAR(A100,{places})"));
        assert_eq!(session.calculate().result, Some(CalcValue::text(
            calc_func::value::ExcelText::from_interop_assignment(expected))));
    }
}

/// W111 locale-profile-20260929: 00275/00287/00299/00311/00323.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_text_at_format_reads_numbers() {
    use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
    let mut profile = format_profile(LocaleId::EnZa);
    profile.decimal_separator = ".";
    profile.thousands_separator = " ";
    let mut session = Session::new("Sheet1").unwrap();
    session.set_environment(Some(CalcEnvironment {
        profile: Some(profile), ..CalcEnvironment::dates(DateSystem::System1900)
    }));
    session.set_formula("=TEXT(A100,\"@\")");
    for (bits, expected) in [(0, "0"), (0x3ff0_0000_0000_0000, "1"),
        (0xbff0_0000_0000_0000, "-1"), (0x4093_4a44_9ba5_e354, "1234.567"),
        (0x40e6_1212_0000_0000, "45200.5625")] {
        session.set_input("A100", InputValue::number(f64::from_bits(bits)).unwrap()).unwrap();
        assert_eq!(session.calculate().result, Some(CalcValue::text(
            calc_func::value::ExcelText::from_interop_assignment(expected))), "{bits:#018x}");
    }
}

/// Fn4 qt001 supplies the previously unanswered different-precision case.
/// Its exact captured text selects General rather than full numeric conversion.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_text_at_precision_uses_general_display() {
    use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
    let mut session = Session::new("Sheet1").unwrap();
    session.set_formula_cell("J20").unwrap();
    session.set_environment(Some(CalcEnvironment {
        profile: Some(format_profile(LocaleId::CurrentExcelHost)),
        ..CalcEnvironment::dates(DateSystem::System1900)
    }));
    session.set_input("A1", InputValue::number(123456789012345.0).unwrap()).unwrap();
    session.set_formula("=TEXT(A1,\"@\")");
    let result = session.calculate();
    assert_eq!(result.result, Some(CalcValue::text(
        calc_func::value::ExcelText::from_interop_assignment("1.23457E+14"))));
    assert!(result.diagnostics.is_empty());
}

/// W111 numeric-text-percent-bounds-20260929: zero coefficients, lower/upper exponent
/// refusals, and the retained positive controls at the upper exponent boundary.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_zero_percentage_exponents_are_validated() {
    use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
    let mut session = Session::new("Sheet1").unwrap();
    session.set_formula_cell("J20").unwrap();
    session.set_environment(Some(CalcEnvironment {
        profile: Some(format_profile(LocaleId::CurrentExcelHost)),
        ..CalcEnvironment::dates(DateSystem::System1900)
    }));
    let forms = |coefficient: &str, exponent: i32| [
        format!("{coefficient}e{exponent}%"), format!("% {coefficient}e{exponent}"),
        format!("( {coefficient}e{exponent} % )"),
    ];
    for coefficient in ["0", "00", "0.0", "0.00"] {
        for exponent in -311..=-307 {
            for text in forms(coefficient, exponent) {
                session.set_formula(&format!("=ABS(\"{text}\")"));
                assert_eq!(session.calculate().result, Some(CalcValue::error(WorksheetErrorCode::Value)), "{text}");
            }
        }
    }
    for coefficient in ["0", "00"] {
        for text in forms(coefficient, 311) {
            session.set_formula(&format!("=ABS(\"{text}\")"));
            assert_eq!(session.calculate().result, Some(CalcValue::error(WorksheetErrorCode::Value)), "{text}");
        }
    }
    for (coefficient, exponent) in [("0", 310), ("00", 310), ("0.0", 311), ("0.00", 311)] {
        for text in forms(coefficient, exponent) {
            session.set_formula(&format!("=ABS(\"{text}\")"));
            let result = session.calculate();
            assert_eq!(result.result, Some(CalcValue::number(0.0)), "{text}");
            assert!(result.diagnostics.is_empty());
        }
    }
}

/// OxFunc-Evidence unswept-structural-sweep-001, cases 00109..00126, 00284..00289,
/// 00476..00481, 00596..00601 and 00614..00619: paired lists skip scalar text/logicals.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_paired_statistics_skip_nonnumeric_scalar_items() {
    let mut session = Session::new("Sheet1").unwrap();
    let mut results = Vec::new();
    for function in ["CORREL", "COVARIANCE.P", "COVARIANCE.S", "INTERCEPT", "PEARSON", "RSQ", "SLOPE"] {
        for argument in ["\"\"", "\"2\"", "TRUE"] {
            let formula = format!("={function}({argument},2)");
            session.set_formula(&formula);
            results.push((formula, session.calculate(), CalcValue::error(WorksheetErrorCode::Div0)));
        }
        for (argument, expected) in [
            ("{2;3}", CalcValue::error(WorksheetErrorCode::NA)),
            ("NA()", CalcValue::error(WorksheetErrorCode::NA)),
            ("2", if function == "COVARIANCE.P" { CalcValue::number(0.0) }
                else { CalcValue::error(WorksheetErrorCode::Div0) }),
        ] {
            let formula = format!("={function}({argument},2)");
            session.set_formula(&formula);
            results.push((formula, session.calculate(), expected));
        }
    }
    for (formula, result, expected) in results {
        assert_eq!(result.result, Some(expected), "{formula}: {:?}", result.diagnostics);
        assert!(result.diagnostics.is_empty());
    }
}

/// W111 numeric-to-text-heldout-typed-20260929, EXACT cases 03436, 03453 and 03454.
/// The number is above its decimal midpoint by more than the measured tie-discrepancy band.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_number_text_above_the_tie_band_rounds_up() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_formula_cell("J20").unwrap();
    session.set_input("A100", InputValue::number(f64::from_bits(0x4449_4efd_9aa8_09c8)).unwrap()).unwrap();
    session.set_formula("=EXACT(A100,\"9.33720954786629E+20\")");
    let positive = session.calculate();
    for at in ["A100", "A101", "A103", "B200"] {
        session.set_input(at, InputValue::number(f64::from_bits(0xc449_4efd_9aa8_09c8)).unwrap()).unwrap();
    }
    session.set_input("A102", InputValue::number(0.0).unwrap()).unwrap();
    session.set_formula("=EXACT(B200,\"-9.33720954786629E+20\")");
    let negative = session.calculate();
    session.set_formula("=EXACT(CHOOSE({1,2;3,4},A100,A101,A102,A103),\"-9.33720954786629E+20\")");
    let array = session.calculate();
    for result in [positive, negative] {
        assert_eq!(result.result, Some(CalcValue::logical(true)), "{:?}", result.diagnostics);
        assert!(result.diagnostics.is_empty());
    }
    let expected = calc_func::value::CalcArray::from_rows(vec![
        vec![CalcValue::logical(true), CalcValue::logical(true)],
        vec![CalcValue::logical(false), CalcValue::logical(true)],
    ]).unwrap();
    assert_eq!(array.result, Some(CalcValue::array(expected)), "{:?}", array.diagnostics);
    assert!(array.diagnostics.is_empty());
}

/// OxFunc-Evidence w111-broad-20260929 FLOOR cases 00000..00009, 00062 and 00067:
/// zero has a zero multiple, while a nonzero number with zero significance is #DIV/0!.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_floor_zero_identity_precedes_its_multiple_error() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_formula_cell("J20").unwrap();
    for zero in [0x0000_0000_0000_0000, 0x8000_0000_0000_0000] {
        session.set_input("A1", InputValue::number(f64::from_bits(zero)).unwrap()).unwrap();
        for multiple in [0.0, 0.1, 2.0, -0.1, -2.0] {
            session.set_input("B1", InputValue::number(multiple).unwrap()).unwrap();
            session.set_formula("=FLOOR(A1,B1)");
            let result = session.calculate();
            assert_eq!(result.result, Some(CalcValue::number(0.0)), "{zero:#018x}, {multiple}: {:?}", result.diagnostics);
            assert_eq!(result.result.unwrap().as_number().map(f64::to_bits), Some(0));
            assert!(result.diagnostics.is_empty());
        }
    }
    for number in [1, -1] {
        session.set_formula(&format!("=FLOOR({number},0)"));
        assert_eq!(session.calculate().result, Some(CalcValue::error(WorksheetErrorCode::Div0)));
    }
}

/// OxFunc-Evidence w111boundary GCD/LCM 00187, 00190, 00194 and 00217,
/// discovery2 00764, heldout-typed 00030..00032, and lcm-array-order 00084..00090.
/// The operand boundary is inclusive; coercion, domain errors and reduction order remain visible.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_gcd_lcm_admit_the_exact_operand_boundary() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_formula_cell("J20").unwrap();
    let limit = f64::from_bits(0x4340_0000_0000_0000);
    for (left, right, gcd, lcm) in [
        (0.0, limit, Some(limit), Some(0.0)),
        (limit, 1.0, Some(1.0), Some(limit)),
        (limit, 2.0, Some(2.0), Some(limit)),
        (limit + 2.0, 0.0, None, None),
        (limit - 1.0, limit, Some(1.0), None),
    ] {
        session.set_input("A1", InputValue::number(left).unwrap()).unwrap();
        session.set_input("B1", InputValue::number(right).unwrap()).unwrap();
        for (function, expected) in [("GCD", gcd), ("LCM", lcm)] {
            let formula = format!("={function}(A1,B1)");
            session.set_formula(&formula);
            let result = session.calculate();
            let expected = expected.map(CalcValue::number).unwrap_or_else(|| CalcValue::error(WorksheetErrorCode::Num));
            assert_eq!(result.result, Some(expected), "{formula}, {left}, {right}: {:?}", result.diagnostics);
            assert!(result.diagnostics.is_empty());
        }
    }
    for (cell, bits) in [("A100", 0x416f_4961_e000_0000), ("B200", 0x416f_4961_e000_0000),
        ("A101", 0x4340_0000_0000_0000), ("C200", 0x4340_0000_0000_0000)] {
        session.set_input(cell, InputValue::number(f64::from_bits(bits)).unwrap()).unwrap();
    }
    for formula in ["=GCD(A100,A101)", "=GCD(CHOOSE({1,2},A100,A101))", "=GCD(B200:C200)"] {
        session.set_formula(formula);
        assert_eq!(session.calculate().result, Some(CalcValue::number(1.0)), "{formula}");
    }
    for (cells, bits) in [(["A100", "B200"], 0x4340_0000_0000_0000),
        (["A101", "C200"], 0x4008_0000_0000_0000), (["A102", "D200"], 0x4325_5555_5555_5556)] {
        for cell in cells { session.set_input(cell, InputValue::number(f64::from_bits(bits)).unwrap()).unwrap(); }
    }
    session.set_input("B201", InputValue::number(3.0).unwrap()).unwrap();
    session.set_input("B202", InputValue::number(f64::from_bits(0x4325_5555_5555_5556)).unwrap()).unwrap();
    for formula in ["=LCM(A100,A101,A102)", "=LCM(CHOOSE({1,2,3},A100,A101,A102))",
        "=LCM(B200:D200)", "=LCM(CHOOSE({1;2;3},A100,A101,A102))", "=LCM(B200:B202)", "=LCM(CHOOSE({1,2},A100,A101),A102)",
        "=LCM(A100,CHOOSE({1,2},A101,A102))"] {
        session.set_formula(formula);
        assert_eq!(session.calculate().result, Some(CalcValue::number(limit)), "{formula}");
    }
}

/// Retained w111-depreciation-padding-20260929 and independent-20260929 controls:
/// earlier bad text is #VALUE!, while earlier padding still precedes a later optional error.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_db_ddb_coercion_precedes_later_padding() {
    use calc_func::value::CalcArray;
    let cases: &[(&str, &str, &[f64], u64, WorksheetErrorCode)] = &[
        ("original-00033", "=DB(CHOOSE({1,2,3;4,5,6},A100,A101,\"bad\",A102,A103,\"bad\"),CHOOSE({1,2;3,4},A104,A105,A106,A107),A108,A109,A110)",
            &[1000.0, 1000.0, 1000.0, 1000.0, 100.0, 100.0, 100.0, 100.0, 10.0, 2.0, 6.0], 0x4067190624dd2f1a, WorksheetErrorCode::Value),
        ("original-00038", "=DB(CHOOSE({1,2,3;4,5,6},A100,A101,\"bad\",A102,A103,\"bad\"),A104,CHOOSE({1,2;3,4},A105,A106,A107,A108),A109,A110)",
            &[1000.0, 1000.0, 1000.0, 1000.0, 100.0, 10.0, 10.0, 10.0, 10.0, 2.0, 6.0], 0x4067190624dd2f1a, WorksheetErrorCode::Value),
        ("original-00043", "=DB(A100,CHOOSE({1,2,3;4,5,6},A101,A102,\"bad\",A103,A104,\"bad\"),CHOOSE({1,2;3,4},A105,A106,A107,A108),A109,A110)",
            &[1000.0, 100.0, 100.0, 100.0, 100.0, 10.0, 10.0, 10.0, 10.0, 2.0, 6.0], 0x4067190624dd2f1a, WorksheetErrorCode::Value),
        ("original-00048", "=DDB(CHOOSE({1,2,3;4,5,6},A100,A101,\"bad\",A102,A103,\"bad\"),CHOOSE({1,2;3,4},A104,A105,A106,A107),A108,A109,A110)",
            &[1000.0, 1000.0, 1000.0, 1000.0, 100.0, 100.0, 100.0, 100.0, 10.0, 2.0, 2.0], 0x4064000000000000, WorksheetErrorCode::Value),
        ("original-00053", "=DDB(CHOOSE({1,2,3;4,5,6},A100,A101,\"bad\",A102,A103,\"bad\"),A104,CHOOSE({1,2;3,4},A105,A106,A107,A108),A109,A110)",
            &[1000.0, 1000.0, 1000.0, 1000.0, 100.0, 10.0, 10.0, 10.0, 10.0, 2.0, 2.0], 0x4064000000000000, WorksheetErrorCode::Value),
        ("original-00058", "=DDB(A100,CHOOSE({1,2,3;4,5,6},A101,A102,\"bad\",A103,A104,\"bad\"),CHOOSE({1,2;3,4},A105,A106,A107,A108),A109,A110)",
            &[1000.0, 100.0, 100.0, 100.0, 100.0, 10.0, 10.0, 10.0, 10.0, 2.0, 2.0], 0x4064000000000000, WorksheetErrorCode::Value),
        ("independent-00209", "=DB(CHOOSE({1,2;3,4},A100,A101,A102,A103),A104,A105,A106,CHOOSE({1,2,3;4,5,6},A107,A108,#NULL!,A109,A110,#NULL!))",
            &[1375.0, 1375.0, 1375.0, 1375.0, 125.0, 9.75, 3.125, 4.5, 4.5, 4.5, 4.5], 0x406b2be01a63a02a, WorksheetErrorCode::NA),
        ("independent-00219", "=DB(CHOOSE({1,2,3;4,5,6},A100,A101,\"invalid\",A102,A103,\"invalid\"),A104,A105,A106,CHOOSE({1,2;3,4},A107,A108,A109,A110))",
            &[1375.0, 1375.0, 1375.0, 1375.0, 125.0, 9.75, 3.125, 4.5, 4.5, 4.5, 4.5], 0x406b2be01a63a02a, WorksheetErrorCode::Value),
        ("independent-00221", "=DB(CHOOSE({1,2;3,4},A100,A101,A102,A103),A104,A105,A106,CHOOSE({1,2,3;4,5,6},A107,A108,\"invalid\",A109,A110,\"invalid\"))",
            &[1375.0, 1375.0, 1375.0, 1375.0, 125.0, 9.75, 3.125, 4.5, 4.5, 4.5, 4.5], 0x406b2be01a63a02a, WorksheetErrorCode::NA),
        ("independent-00259", "=DB(A100,CHOOSE({1,2,3;4,5,6},A101,A102,\"invalid\",A103,A104,\"invalid\"),A105,A106,CHOOSE({1,2;3,4},A107,A108,A109,A110))",
            &[1375.0, 125.0, 125.0, 125.0, 125.0, 9.75, 3.125, 4.5, 4.5, 4.5, 4.5], 0x406b2be01a63a02a, WorksheetErrorCode::Value),
        ("independent-00261", "=DB(A100,CHOOSE({1,2;3,4},A101,A102,A103,A104),A105,A106,CHOOSE({1,2,3;4,5,6},A107,A108,\"invalid\",A109,A110,\"invalid\"))",
            &[1375.0, 125.0, 125.0, 125.0, 125.0, 9.75, 3.125, 4.5, 4.5, 4.5, 4.5], 0x406b2be01a63a02a, WorksheetErrorCode::NA),
        ("independent-00299", "=DB(A100,A101,A102,CHOOSE({1,2,3;4,5,6},A103,A104,\"invalid\",A105,A106,\"invalid\"),CHOOSE({1,2;3,4},A107,A108,A109,A110))",
            &[1375.0, 125.0, 9.75, 3.125, 3.125, 3.125, 3.125, 4.5, 4.5, 4.5, 4.5], 0x406b2be01a63a02a, WorksheetErrorCode::Value),
        ("independent-00301", "=DB(A100,A101,A102,CHOOSE({1,2;3,4},A103,A104,A105,A106),CHOOSE({1,2,3;4,5,6},A107,A108,\"invalid\",A109,A110,\"invalid\"))",
            &[1375.0, 125.0, 9.75, 3.125, 3.125, 3.125, 3.125, 4.5, 4.5, 4.5, 4.5], 0x406b2be01a63a02a, WorksheetErrorCode::NA),
        ("independent-00334", "=DDB(CHOOSE({1,2;3,4},A100,A101,A102,A103),A104,A105,A106,CHOOSE({1,2,3;4,5,6},A107,A108,#NULL!,A109,A110,#NULL!))",
            &[1375.0, 1375.0, 1375.0, 1375.0, 125.0, 9.75, 3.125, 1.5, 1.5, 1.5, 1.5], 0x40628a73e06e783b, WorksheetErrorCode::NA),
        ("independent-00344", "=DDB(CHOOSE({1,2,3;4,5,6},A100,A101,\"invalid\",A102,A103,\"invalid\"),A104,A105,A106,CHOOSE({1,2;3,4},A107,A108,A109,A110))",
            &[1375.0, 1375.0, 1375.0, 1375.0, 125.0, 9.75, 3.125, 1.5, 1.5, 1.5, 1.5], 0x40628a73e06e783b, WorksheetErrorCode::Value),
        ("independent-00346", "=DDB(CHOOSE({1,2;3,4},A100,A101,A102,A103),A104,A105,A106,CHOOSE({1,2,3;4,5,6},A107,A108,\"invalid\",A109,A110,\"invalid\"))",
            &[1375.0, 1375.0, 1375.0, 1375.0, 125.0, 9.75, 3.125, 1.5, 1.5, 1.5, 1.5], 0x40628a73e06e783b, WorksheetErrorCode::NA),
        ("independent-00384", "=DDB(A100,CHOOSE({1,2,3;4,5,6},A101,A102,\"invalid\",A103,A104,\"invalid\"),A105,A106,CHOOSE({1,2;3,4},A107,A108,A109,A110))",
            &[1375.0, 125.0, 125.0, 125.0, 125.0, 9.75, 3.125, 1.5, 1.5, 1.5, 1.5], 0x40628a73e06e783b, WorksheetErrorCode::Value),
        ("independent-00386", "=DDB(A100,CHOOSE({1,2;3,4},A101,A102,A103,A104),A105,A106,CHOOSE({1,2,3;4,5,6},A107,A108,\"invalid\",A109,A110,\"invalid\"))",
            &[1375.0, 125.0, 125.0, 125.0, 125.0, 9.75, 3.125, 1.5, 1.5, 1.5, 1.5], 0x40628a73e06e783b, WorksheetErrorCode::NA),
        ("independent-00424", "=DDB(A100,A101,A102,CHOOSE({1,2,3;4,5,6},A103,A104,\"invalid\",A105,A106,\"invalid\"),CHOOSE({1,2;3,4},A107,A108,A109,A110))",
            &[1375.0, 125.0, 9.75, 3.125, 3.125, 3.125, 3.125, 1.5, 1.5, 1.5, 1.5], 0x40628a73e06e783b, WorksheetErrorCode::Value),
        ("independent-00426", "=DDB(A100,A101,A102,CHOOSE({1,2;3,4},A103,A104,A105,A106),CHOOSE({1,2,3;4,5,6},A107,A108,\"invalid\",A109,A110,\"invalid\"))",
            &[1375.0, 125.0, 9.75, 3.125, 3.125, 3.125, 3.125, 1.5, 1.5, 1.5, 1.5], 0x40628a73e06e783b, WorksheetErrorCode::NA),
    ];
    for (id, formula, inputs, bits, error) in cases {
        let mut session = Session::new("Sheet1").unwrap();
        session.set_formula_cell("J20").unwrap();
        for (index, number) in inputs.iter().enumerate() {
            session.set_input(&format!("A{}", 100 + index), InputValue::number(*number).unwrap()).unwrap();
        }
        session.set_formula(formula);
        let calculated = session.calculate();
        let number = CalcValue::number(f64::from_bits(*bits));
        let row = vec![number.clone(), number, CalcValue::error(*error)];
        let expected = CalcValue::array(CalcArray::from_rows(vec![row.clone(), row]).unwrap());
        assert_eq!(calculated.result, Some(expected), "{id}: {formula}: {:?}", calculated.diagnostics);
        assert!(calculated.diagnostics.is_empty());
    }
}

/// W111 depreciation-padding and independent-20260929: required numeric
/// coercion and factor coercion precede a later absent array coordinate.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_vdb_coercion_precedes_later_padding() {
    use calc_func::value::CalcArray;
    let cases: &[(&str, &str, &[u64], usize, usize, u64, WorksheetErrorCode)] = &[
        ("w111dep-padding-00063-vdb-earlier-0-later-padding-1-badtext", "=VDB(CHOOSE({1,2,3;4,5,6},A100,A101,\"bad\",A102,A103,\"bad\"),CHOOSE({1,2;3,4},A104,A105,A106,A107),A108,A109,A110,A111,TRUE)",
            &[0x408f400000000000, 0x408f400000000000, 0x408f400000000000, 0x408f400000000000, 0x4059000000000000, 0x4059000000000000, 0x4059000000000000, 0x4059000000000000, 0x4024000000000000, 0x3ff0000000000000, 0x4000000000000000, 0x4000000000000000], 2, 3, 0x4064000000000000, WorksheetErrorCode::Value),
        ("w111dep-padding-00068-vdb-earlier-0-later-padding-2-badtext", "=VDB(CHOOSE({1,2,3;4,5,6},A100,A101,\"bad\",A102,A103,\"bad\"),A104,CHOOSE({1,2;3,4},A105,A106,A107,A108),A109,A110,A111,TRUE)",
            &[0x408f400000000000, 0x408f400000000000, 0x408f400000000000, 0x408f400000000000, 0x4059000000000000, 0x4024000000000000, 0x4024000000000000, 0x4024000000000000, 0x4024000000000000, 0x3ff0000000000000, 0x4000000000000000, 0x4000000000000000], 2, 3, 0x4064000000000000, WorksheetErrorCode::Value),
        ("w111dep-padding-00073-vdb-earlier-1-later-padding-2-badtext", "=VDB(A100,CHOOSE({1,2,3;4,5,6},A101,A102,\"bad\",A103,A104,\"bad\"),CHOOSE({1,2;3,4},A105,A106,A107,A108),A109,A110,A111,TRUE)",
            &[0x408f400000000000, 0x4059000000000000, 0x4059000000000000, 0x4059000000000000, 0x4059000000000000, 0x4024000000000000, 0x4024000000000000, 0x4024000000000000, 0x4024000000000000, 0x3ff0000000000000, 0x4000000000000000, 0x4000000000000000], 2, 3, 0x4064000000000000, WorksheetErrorCode::Value),
        ("w111dep-padding-independent-00469-vdb-independent-padding-0-6-False-badtext-False-False", "=VDB(CHOOSE({1,2,3;4,5,6},A100,A101,\"invalid\",A102,A103,\"invalid\"),A104,A105,A106,A107,A108,CHOOSE({1,2;3,4},TRUE,TRUE,TRUE,TRUE))",
            &[0x40957c0000000000, 0x40957c0000000000, 0x40957c0000000000, 0x40957c0000000000, 0x405f400000000000, 0x4023800000000000, 0x3ff2000000000000, 0x400b000000000000, 0x3ff8000000000000], 2, 3, 0x40764227a2b5d435, WorksheetErrorCode::Value),
        ("w111dep-padding-independent-00470-vdb-independent-padding-0-6-False-badtext-False-True", "=VDB(CHOOSE({1,2,3;4,5,6},A100,A101,\"invalid\",A102,A103,\"invalid\"),CHOOSE({1},A104),A105,A106,A107,A108,CHOOSE({1,2;3,4},TRUE,TRUE,TRUE,TRUE))",
            &[0x40957c0000000000, 0x40957c0000000000, 0x40957c0000000000, 0x40957c0000000000, 0x405f400000000000, 0x4023800000000000, 0x3ff2000000000000, 0x400b000000000000, 0x3ff8000000000000], 2, 3, 0x40764227a2b5d435, WorksheetErrorCode::Value),
        ("w111dep-padding-independent-00471-vdb-independent-padding-0-6-False-badtext-True-False", "=VDB(CHOOSE({1,2;3,4},A100,A101,A102,A103),A104,A105,A106,A107,A108,CHOOSE({1,2,3;4,5,6},TRUE,TRUE,\"invalid\",TRUE,TRUE,\"invalid\"))",
            &[0x40957c0000000000, 0x40957c0000000000, 0x40957c0000000000, 0x40957c0000000000, 0x405f400000000000, 0x4023800000000000, 0x3ff2000000000000, 0x400b000000000000, 0x3ff8000000000000], 2, 3, 0x40764227a2b5d435, WorksheetErrorCode::NA),
        ("w111dep-padding-independent-00489-vdb-independent-padding-0-6-True-badtext-False-False", "=VDB(CHOOSE({1,2;3,4;5,6},A100,A101,A102,A103,\"invalid\",\"invalid\"),A104,A105,A106,A107,A108,CHOOSE({1,2;3,4},TRUE,TRUE,TRUE,TRUE))",
            &[0x40957c0000000000, 0x40957c0000000000, 0x40957c0000000000, 0x40957c0000000000, 0x405f400000000000, 0x4023800000000000, 0x3ff2000000000000, 0x400b000000000000, 0x3ff8000000000000], 3, 2, 0x40764227a2b5d435, WorksheetErrorCode::Value),
        ("w111dep-padding-independent-00491-vdb-independent-padding-0-6-True-badtext-True-False", "=VDB(CHOOSE({1,2;3,4},A100,A101,A102,A103),A104,A105,A106,A107,A108,CHOOSE({1,2;3,4;5,6},TRUE,TRUE,TRUE,TRUE,\"invalid\",\"invalid\"))",
            &[0x40957c0000000000, 0x40957c0000000000, 0x40957c0000000000, 0x40957c0000000000, 0x405f400000000000, 0x4023800000000000, 0x3ff2000000000000, 0x400b000000000000, 0x3ff8000000000000], 3, 2, 0x40764227a2b5d435, WorksheetErrorCode::NA),
        ("w111dep-padding-independent-00509-vdb-independent-padding-1-6-False-badtext-False-False", "=VDB(A100,CHOOSE({1,2,3;4,5,6},A101,A102,\"invalid\",A103,A104,\"invalid\"),A105,A106,A107,A108,CHOOSE({1,2;3,4},TRUE,TRUE,TRUE,TRUE))",
            &[0x40957c0000000000, 0x405f400000000000, 0x405f400000000000, 0x405f400000000000, 0x405f400000000000, 0x4023800000000000, 0x3ff2000000000000, 0x400b000000000000, 0x3ff8000000000000], 2, 3, 0x40764227a2b5d435, WorksheetErrorCode::Value),
        ("w111dep-padding-independent-00511-vdb-independent-padding-1-6-False-badtext-True-False", "=VDB(A100,CHOOSE({1,2;3,4},A101,A102,A103,A104),A105,A106,A107,A108,CHOOSE({1,2,3;4,5,6},TRUE,TRUE,\"invalid\",TRUE,TRUE,\"invalid\"))",
            &[0x40957c0000000000, 0x405f400000000000, 0x405f400000000000, 0x405f400000000000, 0x405f400000000000, 0x4023800000000000, 0x3ff2000000000000, 0x400b000000000000, 0x3ff8000000000000], 2, 3, 0x40764227a2b5d435, WorksheetErrorCode::NA),
        ("w111dep-padding-independent-00529-vdb-independent-padding-1-6-True-badtext-False-False", "=VDB(A100,CHOOSE({1,2;3,4;5,6},A101,A102,A103,A104,\"invalid\",\"invalid\"),A105,A106,A107,A108,CHOOSE({1,2;3,4},TRUE,TRUE,TRUE,TRUE))",
            &[0x40957c0000000000, 0x405f400000000000, 0x405f400000000000, 0x405f400000000000, 0x405f400000000000, 0x4023800000000000, 0x3ff2000000000000, 0x400b000000000000, 0x3ff8000000000000], 3, 2, 0x40764227a2b5d435, WorksheetErrorCode::Value),
        ("w111dep-padding-independent-00531-vdb-independent-padding-1-6-True-badtext-True-False", "=VDB(A100,CHOOSE({1,2;3,4},A101,A102,A103,A104),A105,A106,A107,A108,CHOOSE({1,2;3,4;5,6},TRUE,TRUE,TRUE,TRUE,\"invalid\",\"invalid\"))",
            &[0x40957c0000000000, 0x405f400000000000, 0x405f400000000000, 0x405f400000000000, 0x405f400000000000, 0x4023800000000000, 0x3ff2000000000000, 0x400b000000000000, 0x3ff8000000000000], 3, 2, 0x40764227a2b5d435, WorksheetErrorCode::NA),
        ("w111dep-padding-independent-00549-vdb-independent-padding-5-6-False-badtext-False-False", "=VDB(A100,A101,A102,A103,A104,CHOOSE({1,2,3;4,5,6},A105,A106,\"invalid\",A107,A108,\"invalid\"),CHOOSE({1,2;3,4},TRUE,TRUE,TRUE,TRUE))",
            &[0x40957c0000000000, 0x405f400000000000, 0x4023800000000000, 0x3ff2000000000000, 0x400b000000000000, 0x3ff8000000000000, 0x3ff8000000000000, 0x3ff8000000000000, 0x3ff8000000000000], 2, 3, 0x40764227a2b5d435, WorksheetErrorCode::Value),
        ("w111dep-padding-independent-00551-vdb-independent-padding-5-6-False-badtext-True-False", "=VDB(A100,A101,A102,A103,A104,CHOOSE({1,2;3,4},A105,A106,A107,A108),CHOOSE({1,2,3;4,5,6},TRUE,TRUE,\"invalid\",TRUE,TRUE,\"invalid\"))",
            &[0x40957c0000000000, 0x405f400000000000, 0x4023800000000000, 0x3ff2000000000000, 0x400b000000000000, 0x3ff8000000000000, 0x3ff8000000000000, 0x3ff8000000000000, 0x3ff8000000000000], 2, 3, 0x40764227a2b5d435, WorksheetErrorCode::NA),
        ("w111dep-padding-independent-00569-vdb-independent-padding-5-6-True-badtext-False-False", "=VDB(A100,A101,A102,A103,A104,CHOOSE({1,2;3,4;5,6},A105,A106,A107,A108,\"invalid\",\"invalid\"),CHOOSE({1,2;3,4},TRUE,TRUE,TRUE,TRUE))",
            &[0x40957c0000000000, 0x405f400000000000, 0x4023800000000000, 0x3ff2000000000000, 0x400b000000000000, 0x3ff8000000000000, 0x3ff8000000000000, 0x3ff8000000000000, 0x3ff8000000000000], 3, 2, 0x40764227a2b5d435, WorksheetErrorCode::Value),
        ("w111dep-padding-independent-00571-vdb-independent-padding-5-6-True-badtext-True-False", "=VDB(A100,A101,A102,A103,A104,CHOOSE({1,2;3,4},A105,A106,A107,A108),CHOOSE({1,2;3,4;5,6},TRUE,TRUE,TRUE,TRUE,\"invalid\",\"invalid\"))",
            &[0x40957c0000000000, 0x405f400000000000, 0x4023800000000000, 0x3ff2000000000000, 0x400b000000000000, 0x3ff8000000000000, 0x3ff8000000000000, 0x3ff8000000000000, 0x3ff8000000000000], 3, 2, 0x40764227a2b5d435, WorksheetErrorCode::NA),
    ];
    for (id, formula, inputs, rows, cols, bits, error) in cases {
        let mut session = Session::new("Sheet1").unwrap();
        session.set_formula_cell("J20").unwrap();
        for (index, bits) in inputs.iter().enumerate() {
            session.set_input(&format!("A{}", 100 + index), InputValue::number(f64::from_bits(*bits)).unwrap()).unwrap();
        }
        session.set_formula(formula);
        let calculated = session.calculate();
        let expected = CalcArray::from_rows((0..*rows).map(|row| (0..*cols).map(|col| {
            if (*rows == 3 && row == 2) || (*cols == 3 && col == 2) {
                CalcValue::error(*error)
            } else {
                CalcValue::number(f64::from_bits(*bits))
            }
        }).collect()).collect()).unwrap();
        assert_eq!(calculated.result, Some(CalcValue::array(expected)), "{id}: {formula}: {:?}", calculated.diagnostics);
        assert!(calculated.diagnostics.is_empty());
    }
}

/// Selected W111 depreciation-options 00022..00031 and W089 seed 00323:
/// an explicit missing factor is zero, distinct from an absent optional factor.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_vdb_explicit_missing_factor_is_zero() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_formula_cell("J20").unwrap();
    for (index, value) in [1000.0, 100.0, 10.0, 1.0, 2.0].into_iter().enumerate() {
        session.set_input(&format!("A{}", 100 + index), InputValue::number(value).unwrap()).unwrap();
    }
    session.set_input("A105", InputValue::blank()).unwrap();
    for (formula, expected) in [
        ("=VDB(2400,300,120,6,18)", CalcValue::number(f64::from_bits(0x4078c4e5981baf06))),
        ("=VDB(A100,A101,A102,A103,A104,,FALSE)", CalcValue::number(f64::from_bits(0x4056800000000000))),
        ("=VDB(A100,A101,A102,A103,A104,A105,FALSE)", CalcValue::number(f64::from_bits(0x4056800000000000))),
        ("=VDB(A100,A101,A102,A103,A104,\"\",FALSE)", CalcValue::error(WorksheetErrorCode::Value)),
        ("=VDB(A100,A101,A102,A103,A104,\"2\",FALSE)", CalcValue::number(f64::from_bits(0x4064000000000000))),
        ("=VDB(A100,A101,A102,A103,A104,\"TRUE\",FALSE)", CalcValue::error(WorksheetErrorCode::Value)),
        ("=VDB(A100,A101,A102,A103,A104,\"FALSE\",FALSE)", CalcValue::error(WorksheetErrorCode::Value)),
        ("=VDB(A100,A101,A102,A103,A104,TRUE,FALSE)", CalcValue::number(f64::from_bits(0x4056800000000000))),
        ("=VDB(A100,A101,A102,A103,A104,#N/A,FALSE)", CalcValue::error(WorksheetErrorCode::NA)),
    ] {
        session.set_formula(formula);
        let calculated = session.calculate();
        assert_eq!(calculated.result, Some(expected), "{formula}: {:?}", calculated.diagnostics);
        assert!(calculated.diagnostics.is_empty());
    }
}

/// Recorded hypgeom.dist-domain-00026..00029 and the fresh fractional
/// support controls in OxFunc-Evidence's HYPGEOM.DIST banks.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn hypergeometric_validates_raw_counts_and_admits_an_empty_population() {
    let mut session = Session::new("Inputs").unwrap();
    for function in ["HYPGEOM.DIST", "HYPGEOMDIST"] {
        for (arguments, expected) in [
            ("0,0,-0.1,8", CalcValue::error(WorksheetErrorCode::Num)),
            ("0,0,0,0", CalcValue::number(1.0)),
            ("1,0,0,0", CalcValue::number(0.0)),
        ] {
            let tail = if function == "HYPGEOM.DIST" { ",FALSE" } else { "" };
            let formula = format!("={function}({arguments}{tail})");
            session.set_formula(&formula);
            let calculated = session.calculate();
            assert_eq!(calculated.result, Some(expected), "{formula}: {:?}", calculated.diagnostics);
            assert!(calculated.diagnostics.is_empty());
        }
    }
    session.set_formula("=HYPGEOM.DIST(1,0,0,0,TRUE)");
    assert_eq!(session.calculate().result, Some(CalcValue::number(1.0)));
}

/// The recorded Formula2 TEXTSPLIT case w111typed-00286-textsplit-arg1-row-array,
/// OxFunc-Evidence / w111-broad-typed-20260929-002, through the application controller.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn lifted_textsplit_preserves_the_recorded_spill() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_form(calc_fml::FormulaForm::Dynamic);
    session.set_formula("=TEXTSPLIT(CHOOSE({1,2},\"a,b,c\",\"a,b,c\"),\",\")");
    let calculated = session.calculate();
    let expected = CalcValue::array(
        calc_func::value::CalcArray::from_rows(vec![vec![
            CalcValue::text(calc_func::value::ExcelText::from_interop_assignment("a")),
            CalcValue::text(calc_func::value::ExcelText::from_interop_assignment("a")),
        ]])
        .unwrap(),
    );
    assert_eq!(calculated.result, Some(expected), "{:?}", calculated.diagnostics);
    assert!(calculated.diagnostics.is_empty());
}

/// OxFunc-Evidence w111-conditional-heldout-typed-20260929, cases 00129 and 00177:
/// a selected reference at the formula-result boundary displays blank positions as zero.
/// Empty text remains text, and errors and mixed-case text retain their exact values.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn selected_reference_results_publish_recorded_blank_zeros() {
    use calc_func::value::{CalcArray, ExcelText};
    let text = |s: &str| CalcValue::text(ExcelText::from_interop_assignment(s));
    let error = CalcValue::error;
    let cases = [
        (
            "=IF(A1:A1,F1:G3,CHOOSE({1},\"tRuE\"))",
            0.25,
            vec![
                vec![error(WorksheetErrorCode::Div0), CalcValue::empty()],
                vec![text("fAlSe"), error(WorksheetErrorCode::Value)],
                vec![text("FALSE "), error(WorksheetErrorCode::Ref)],
            ],
            vec![
                vec![error(WorksheetErrorCode::Div0), CalcValue::number(0.0)],
                vec![text("fAlSe"), error(WorksheetErrorCode::Value)],
                vec![text("FALSE "), error(WorksheetErrorCode::Ref)],
            ],
        ),
        (
            "=IF(A1:A1,F1:I3,CHOOSE({1;2;3;4},#NUM!,A100,A101,\"0\"))",
            -2.0,
            vec![
                vec![text(""), text("1"), text("0"), error(WorksheetErrorCode::Div0)],
                vec![text("FALSE "), text("fAlSe"), error(WorksheetErrorCode::Div0), error(WorksheetErrorCode::Num)],
                vec![text("0"), CalcValue::empty(), text("1"), text("")],
            ],
            vec![
                vec![text(""), text("1"), text("0"), error(WorksheetErrorCode::Div0)],
                vec![text("FALSE "), text("fAlSe"), error(WorksheetErrorCode::Div0), error(WorksheetErrorCode::Num)],
                vec![text("0"), CalcValue::number(0.0), text("1"), text("")],
            ],
        ),
    ];
    for (formula, condition, cells, expected) in cases {
        let mut session = Session::new("Sheet1").unwrap();
        session.set_form(calc_fml::FormulaForm::Dynamic);
        session.set_formula_cell("J20").unwrap();
        session.set_input("A1", InputValue::number(condition).unwrap()).unwrap();
        session.set_input("A100", InputValue::number(-2.0).unwrap()).unwrap();
        session.set_input("A101", InputValue::number(0.25).unwrap()).unwrap();
        for (row, values) in cells.iter().enumerate() {
            for (column, value) in values.iter().enumerate() {
                let input = match value.core_ref() {
                    calc_func::value::CoreRef::Text(t) => InputValue::text(&t.to_string_lossy()).unwrap(),
                    calc_func::value::CoreRef::Error(e) => InputValue::error(e),
                    calc_func::value::CoreRef::Empty => InputValue::blank(),
                    _ => unreachable!("the two retained fixtures contain text, errors and blanks"),
                };
                session.set_input(&format!("{}{}", char::from(b'F' + column as u8), row + 1), input).unwrap();
            }
        }
        let expected = CalcValue::array(CalcArray::from_rows(expected).unwrap());
        session.set_formula(formula);
        let calculated = session.calculate();
        assert_eq!(calculated.result, Some(expected), "{formula}: {:?}", calculated.diagnostics);
        assert!(calculated.diagnostics.is_empty());
    }
}

/// OxFunc-Evidence reference-fixture-001/002 and W111 broad typed 00890..00895,
/// 00960..00961: input literals have no formulas or subtotal/visibility exclusions.
/// Reference-operator-001 case 00004 also records #REF! for a literal input's A1#.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn plain_input_facts_reach_the_existing_reference_functions() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_formula_cell("J10").unwrap();
    session.set_input("A1", InputValue::number(7.0).unwrap()).unwrap();
    for (formula, expected) in [
        ("=ISFORMULA(A1)", CalcValue::logical(false)),
        ("=FORMULATEXT(A1)", CalcValue::error(WorksheetErrorCode::NA)),
        ("=SHEET(A1)", CalcValue::number(1.0)),
    ] {
        session.set_formula(formula);
        let calculated = session.calculate();
        assert_eq!(calculated.result, Some(expected), "{formula}: {:?}", calculated.diagnostics);
        assert!(calculated.diagnostics.is_empty());
    }
    for row in 1..=4 {
        session.set_input(&format!("A{row}"), InputValue::number(row as f64).unwrap()).unwrap();
    }
    for formula in ["=SUBTOTAL(9,A1:A4)", "=AGGREGATE(9,0,A1:A4)"] {
        session.set_formula(formula);
        let calculated = session.calculate();
        assert_eq!(calculated.result, Some(CalcValue::number(10.0)),
            "{formula}: {:?}", calculated.diagnostics);
        assert!(calculated.diagnostics.is_empty());
    }
    session.set_formula("=A1#");
    let calculated = session.calculate();
    assert_eq!(calculated.result, Some(CalcValue::error(WorksheetErrorCode::Ref)),
        "{:?}", calculated.diagnostics);
    assert!(calculated.diagnostics.is_empty());
    session.set_formula_cell("J20").unwrap();
    session.set_input("A2", InputValue::text("2").unwrap()).unwrap();
    session.set_input("A3", InputValue::logical(true)).unwrap();
    session.set_input("A4", InputValue::blank()).unwrap();
    session.set_input("A5", InputValue::number(4.0).unwrap()).unwrap();
    for (selector, expected) in [(1.0, 2.5), (2.0, 2.0), (3.0, 4.0), (4.0, 4.0), (5.0, 1.0), (9.0, 5.0)] {
        session.set_input("A100", InputValue::number(selector).unwrap()).unwrap();
        session.set_formula("=SUBTOTAL(A100,A1:A5)");
        let calculated = session.calculate();
        assert_eq!(calculated.result, Some(CalcValue::number(expected)),
            "SUBTOTAL {selector}: {:?}", calculated.diagnostics);
        assert!(calculated.diagnostics.is_empty());
    }
    // Missing values still stop evaluation. A session has no workbook metadata for an
    // unset input, and this change does not infer the current formula's own cell kind.
    for formula in ["=ISFORMULA(J20)", "=FORMULATEXT(Z99)", "=SUBTOTAL(9,A1:J20)", "=J20#", "=Z99#"] {
        session.set_formula(formula);
        assert!(session.calculate().result.is_none(), "{formula}");
    }
    session.set_formula("=SUBTOTAL(9,A1:A6)");
    let calculated = session.calculate();
    assert!(calculated.result.is_none());
    assert!(calculated.diagnostics.iter().any(|d| matches!(d,
        Diagnostic::Eval(EvalFault::MissingInput { .. })
    )), "{:?}", calculated.diagnostics);
}

/// W111 independent depreciation cases 00108/00111/00113/00114 and
/// 00224/00227/00229/00230: #N/A in each earlier argument precedes #DIV/0! last.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_scalar_errors_precede_depreciation_options() {
    let inputs = [
        0x4093_4a45_6d5c_faad, // cost
        0x4028_b0a3_d70a_3d71, // salvage
        0x401f_0000_0000_0000, // life
        0x400a_0000_0000_0000, // period
    ];
    for function in ["DB", "DDB"] {
        for error_at in 0..4 {
            let mut session = Session::new("Sheet1").unwrap();
            session.set_formula_cell("J20").unwrap();
            let mut arguments = Vec::new();
            let mut cell = 100;
            for (index, bits) in inputs.iter().enumerate() {
                if index == error_at {
                    arguments.push("#N/A".to_owned());
                } else {
                    let address = format!("A{cell}");
                    session.set_input(&address, InputValue::number(f64::from_bits(*bits)).unwrap()).unwrap();
                    arguments.push(address);
                    cell += 1;
                }
            }
            arguments.push("#DIV/0!".to_owned());
            let formula = format!("={function}({})", arguments.join(","));
            session.set_formula(&formula);
            let calculated = session.calculate();
            assert_eq!(calculated.result, Some(CalcValue::error(WorksheetErrorCode::NA)),
                "{formula}: {:?}", calculated.diagnostics);
            assert!(calculated.diagnostics.is_empty());
        }
    }
}

/// Retained unswept-structural-sweep-001 cases 00716 and 00718..00721.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_scalar_values_survive_trimrange() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_formula_cell("A1").unwrap();
    for (formula, expected) in [
        ("=TRIMRANGE(NA())", CalcValue::error(WorksheetErrorCode::NA)),
        ("=TRIMRANGE(2)", CalcValue::number(2.0)),
        ("=TRIMRANGE(\"\")", CalcValue::text(calc_func::value::ExcelText::from_interop_assignment(""))),
        ("=TRIMRANGE(\"2\")", CalcValue::text(calc_func::value::ExcelText::from_interop_assignment("2"))),
        ("=TRIMRANGE(TRUE)", CalcValue::logical(true)),
    ] {
        session.set_formula(formula);
        let calculated = session.calculate();
        assert_eq!(calculated.result, Some(expected), "{formula}: {:?}", calculated.diagnostics);
        assert!(calculated.diagnostics.is_empty());
    }
}

/// OxFunc-Evidence w111-address-numeric-text-syntax-20260929 (0630..0695),
/// w111-address-blank-numeric-text-20260929 (0057..0098) and the ABS numeric-text
/// controls reject these Unicode spaces. The w111-locale-profile-20260929 VALUE
/// controls 00118..00119 add NBSP and narrow NBSP within numeric text.
/// They are not ASCII trimming characters.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn measured_unicode_numeric_text_spaces_return_value_errors() {
    use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
    let mut session = Session::new("Sheet1").unwrap();
    session.set_environment(Some(CalcEnvironment {
        profile: Some(format_profile(LocaleId::EnZa)),
        ..CalcEnvironment::dates(DateSystem::System1900)
    }));
    session.set_formula_cell("J70").unwrap();
    for space in ['\u{00a0}', '\u{2003}', '\u{3000}'] {
        for text in [format!("{space}1"), format!("1{space}"), format!("{space}1{space}"),
            format!("1{space}%"), format!("({space}1)")] {
            session.set_input("A1", InputValue::text(&text).unwrap()).unwrap();
            for argument in ["A1".to_owned(), format!("\"{text}\"")] {
                let formula = format!("=ADDRESS({argument},2,4,FALSE,\"Alpha\")");
                session.set_formula(&formula);
                let calculated = session.calculate();
                assert_eq!(calculated.result, Some(CalcValue::error(WorksheetErrorCode::Value)),
                    "{formula}: {:?}", calculated.diagnostics);
                assert!(calculated.diagnostics.is_empty());
            }
        }
    }
    for text in ["\u{00a0}2", "2\u{00a0}"] {
        session.set_input("A1", InputValue::text(text).unwrap()).unwrap();
        for argument in ["A1".to_owned(), format!("\"{text}\"")] {
            session.set_formula(&format!("=ABS({argument})"));
            let calculated = session.calculate();
            assert_eq!(calculated.result, Some(CalcValue::error(WorksheetErrorCode::Value)),
                "ABS({argument}): {:?}", calculated.diagnostics);
            assert!(calculated.diagnostics.is_empty());
        }
    }
    for formula in ["=VALUE(\"1\u{00a0}234.5\")", "=VALUE(\"1\u{202f}234.5\")"] {
        session.set_formula(formula);
        let calculated = session.calculate();
        assert_eq!(calculated.result, Some(CalcValue::error(WorksheetErrorCode::Value)),
            "{formula}: {:?}", calculated.diagnostics);
        assert!(calculated.diagnostics.is_empty());
    }
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test;

/// Prints a line the acceptance runner keeps: on stdout natively, on the browser console (which
/// wasm-bindgen-test-runner forwards under `--nocapture`) on wasm32.
macro_rules! note {
    ($($arg:tt)*) => {{
        let line = format!($($arg)*);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_test::console_log!("{}", line);
        #[cfg(not(target_arch = "wasm32"))]
        println!("{line}");
    }};
}

// ---------------------------------------------------------------------------------------------
// The examples both parts run.

/// The kinds of typed input, as the surface's type selector names them.
#[derive(Clone, Copy, Debug)]
enum Kind {
    Number,
    Text,
    Logical,
    Error,
    Blank,
}

impl Kind {
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    fn selector(self) -> &'static str {
        match self {
            Kind::Number => "number",
            Kind::Text => "text",
            Kind::Logical => "logical",
            Kind::Error => "error",
            Kind::Blank => "blank",
        }
    }
}

/// The input the test means by `kind` and `entry`, built with `InputValue`'s constructors.
fn input(kind: Kind, entry: &str) -> InputValue {
    match kind {
        Kind::Number => InputValue::number(entry.parse().expect("a number entry")).expect("held"),
        Kind::Text => InputValue::text(entry).expect("short text"),
        Kind::Logical => InputValue::logical(entry == "TRUE"),
        Kind::Error => {
            assert_eq!(entry, "#N/A", "the examples use one error");
            InputValue::error(WorksheetErrorCode::NA)
        }
        Kind::Blank => InputValue::blank(),
    }
}

/// An expected line of text: exactly this, or starting with this where the rest is the parser's
/// own message, which the test does not derive.
#[derive(Clone, Copy, Debug)]
enum Line {
    Is(&'static str),
    Starts(&'static str),
}

impl Line {
    fn matches(self, text: &str) -> bool {
        match self {
            Line::Is(expected) => text == expected,
            Line::Starts(prefix) => text.starts_with(prefix),
        }
    }
}

/// Inputs and a formula, entered in Sheet1!C1 of a session with sheets Sheet1 and Sheet2, and
/// what the session shows for them.
struct Example {
    name: &'static str,
    inputs: &'static [(&'static str, Kind, &'static str)],
    formula: &'static str,
    result: &'static str,
    diagnostics: &'static [Line],
    reads: &'static [&'static str],
}

const FORMULA_CELL: &str = "C1";

const TWO_CELLS: &[(&str, Kind, &str)] = &[
    ("A1", Kind::Number, "1.5"),
    ("Sheet2!B2", Kind::Number, "4"),
];
const ONE_CELL: &[(&str, Kind, &str)] = &[("A1", Kind::Number, "1.5")];
const COLUMN: &[(&str, Kind, &str)] = &[
    ("A1", Kind::Number, "1.5"),
    ("A2", Kind::Blank, ""),
    ("A3", Kind::Number, "2"),
];

const READ_A1: &str = "Sheet1!A1: Sheet1!A1 = number 1.5";

/// The examples, in the order both parts run them. The first three are one session edited in
/// turn: two supplied cells (one on Sheet2), one of them changed, then removed.
const EXAMPLES: &[Example] = &[
    Example {
        name: "two supplied cells, one on another sheet",
        inputs: TWO_CELLS,
        formula: "=A1*Sheet2!B2+1",
        // 1.5 * 4 + 1
        result: "number 7",
        diagnostics: &[],
        reads: &[READ_A1, "Sheet2!B2: Sheet2!B2 = number 4"],
    },
    Example {
        name: "the Sheet2 input changed",
        inputs: &[
            ("A1", Kind::Number, "1.5"),
            ("Sheet2!B2", Kind::Number, "10"),
        ],
        formula: "=A1*Sheet2!B2+1",
        // 1.5 * 10 + 1
        result: "number 16",
        diagnostics: &[],
        reads: &[READ_A1, "Sheet2!B2: Sheet2!B2 = number 10"],
    },
    Example {
        name: "the Sheet2 input removed (the implementation's answer: no value is made up)",
        inputs: ONE_CELL,
        formula: "=A1*Sheet2!B2+1",
        result: "not calculated",
        diagnostics: &[Line::Is("missing input: Sheet2!B2 has no input")],
        reads: &[READ_A1, "Sheet2!B2: Sheet2!B2 no input"],
    },
    Example {
        name: "INDIRECT's text names a cell of another sheet",
        inputs: TWO_CELLS,
        formula: "=INDIRECT(\"Sheet2!B\"&2)*2",
        // Sheet2!B2 = 4, doubled.
        result: "number 8",
        diagnostics: &[],
        reads: &["Sheet2!B2: Sheet2!B2 = number 4"],
    },
    Example {
        name: "a text input",
        inputs: &[("Sheet2!A1", Kind::Text, "a")],
        formula: "=Sheet2!A1&\"x\"",
        result: "text \"ax\"",
        diagnostics: &[],
        reads: &["Sheet2!A1: Sheet2!A1 = text \"a\""],
    },
    Example {
        name: "a logical input",
        inputs: &[("B1", Kind::Logical, "TRUE")],
        formula: "=IF(B1,\"yes\",\"no\")",
        result: "text \"yes\"",
        diagnostics: &[],
        reads: &["Sheet1!B1: Sheet1!B1 = logical TRUE"],
    },
    Example {
        name: "an error input",
        inputs: &[("A1", Kind::Number, "1.5"), ("B3", Kind::Error, "#N/A")],
        formula: "=A1*B3",
        // An arithmetic operand's error is the result.
        result: "error #N/A",
        diagnostics: &[],
        reads: &[READ_A1, "Sheet1!B3: Sheet1!B3 = error #N/A"],
    },
    Example {
        name: "a blank input inside a range",
        inputs: COLUMN,
        formula: "=SUM(A1:A3)",
        // SUM skips the blank cell: 1.5 + 2
        result: "number 3.5",
        diagnostics: &[],
        reads: &["Sheet1!A1:A3: Sheet1!A1 = number 1.5; Sheet1!A2 blank; Sheet1!A3 = number 2"],
    },
    Example {
        name: "a range reaching a cell with no input (the implementation's answer)",
        inputs: COLUMN,
        formula: "=SUM(A1:A4)",
        result: "not calculated",
        diagnostics: &[Line::Is("missing input: Sheet1!A4 has no input")],
        reads: &[
            "Sheet1!A1:A4: Sheet1!A1 = number 1.5; Sheet1!A2 blank; Sheet1!A3 = number 2; Sheet1!A4 no input",
        ],
    },
    Example {
        name: "a formula the parser refuses",
        inputs: ONE_CELL,
        formula: "=A1*",
        result: "not calculated",
        // The right operand is missing at the end of the text, offset 4.
        diagnostics: &[Line::Starts("syntax at 4..4: ")],
        reads: &[],
    },
    Example {
        name: "a reference to a sheet the session does not have",
        inputs: ONE_CELL,
        formula: "=Sheet9!A1+1",
        result: "not calculated",
        diagnostics: &[Line::Is(
            "reference at 1..10 refused: there is no sheet named Sheet9",
        )],
        reads: &[],
    },
    Example {
        name: "a call calc-func does not model (the implementation's answer)",
        inputs: ONE_CELL,
        // A difference below the normal range (4.5E-308 less 4.5000000000001E-308), which calc-func
        // refuses.
        formula: "=A1*3E-308-4.5000000000001E-308",
        result: "not calculated",
        diagnostics: &[Line::Is(
            "function fault in the - operator: Excel's result is not modelled for this call",
        )],
        reads: &[READ_A1],
    },
];

/// Checks the text the session's calculation gives for `example`, as the surface shows it.
fn check_text(session: &Session, calculation: &Calculation, example: &Example) {
    let result = match &calculation.result {
        Some(value) => display::value(value),
        None => "not calculated".to_owned(),
    };
    assert_eq!(result, example.result, "{}: result", example.name);
    let diagnostics: Vec<String> = calculation
        .diagnostics
        .iter()
        .map(|diagnostic| display::diagnostic(session, diagnostic))
        .collect();
    check_lines(
        &diagnostics,
        example.diagnostics,
        example.name,
        "diagnostics",
    );
    let reads: Vec<String> = calculation
        .reads
        .iter()
        .map(|read| display::read(session, read))
        .collect();
    assert_eq!(reads, example.reads, "{}: reads", example.name);
}

fn check_lines(shown: &[String], expected: &[Line], example: &str, what: &str) {
    assert_eq!(
        shown.len(),
        expected.len(),
        "{example}: {what} {shown:?}, expected {expected:?}"
    );
    for (line, expected) in shown.iter().zip(expected) {
        assert!(
            expected.matches(line),
            "{example}: {what} line {line:?}, expected {expected:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The controller part.

fn key(sheet: SheetId, row: u32, column: u32) -> CellKey {
    CellKey::new(sheet, row, column).expect("a cell on the grid")
}

fn one_cell_read(cell: CellKey, cells: RunCells) -> Read {
    Read {
        area: GridArea::cell(cell),
        runs: vec![ReadRun { start: cell, cells }],
    }
}

fn numbers(values: &[f64]) -> RunCells {
    RunCells::Values(values.iter().map(|n| CalcValue::number(*n)).collect())
}

/// Replaces the session's inputs with the example's and calculates its formula.
fn run_example(session: &mut Session, example: &Example) -> Calculation {
    session.clear_inputs();
    for (address, kind, entry) in example.inputs {
        session
            .set_input(address, input(*kind, entry))
            .unwrap_or_else(|e| panic!("{}: input {address}: {e:?}", example.name));
    }
    session.set_formula(example.formula);
    session.calculate()
}

/// The controller part of the reference case. Returns the number of examples it checked.
fn controller_part() -> usize {
    let mut session = Session::new("Sheet1").expect("Sheet1 is a sheet name");
    let s2 = session.add_sheet("Sheet2").expect("Sheet2 is a sheet name");
    let s1 = session
        .sheets()
        .find(|(name, _)| *name == "Sheet1")
        .map(|(_, id)| id)
        .expect("the first sheet");
    assert_eq!(
        session.sheets().map(|(name, _)| name).collect::<Vec<_>>(),
        ["Sheet1", "Sheet2"]
    );
    assert_eq!(
        session.formula_cell(),
        key(s1, 1, 1),
        "a new session's formula is in A1"
    );
    assert_eq!(session.set_formula_cell(FORMULA_CELL), Ok(key(s1, 1, 3)));

    let (a1, a2, a3, a4) = (key(s1, 1, 1), key(s1, 2, 1), key(s1, 3, 1), key(s1, 4, 1));
    let sheet2_b2 = key(s2, 2, 2);

    // Two explicitly supplied cells, one on another sheet: the trace shows exactly the two reads.
    assert_eq!(session.set_input("A1", input(Kind::Number, "1.5")), Ok(a1));
    assert_eq!(
        session.set_input("Sheet2!B2", input(Kind::Number, "4")),
        Ok(sheet2_b2)
    );
    session.set_formula(EXAMPLES[0].formula);
    let first = session.calculate();
    assert_eq!(
        first,
        Calculation {
            result: Some(CalcValue::number(7.0)),
            diagnostics: vec![],
            reads: vec![
                one_cell_read(a1, numbers(&[1.5])),
                one_cell_read(sheet2_b2, numbers(&[4.0])),
            ],
        }
    );
    check_text(&session, &first, &EXAMPLES[0]);

    // The same formula after the Sheet2 input changes reads the new value.
    assert_eq!(
        session.set_input("sheet2!b2", input(Kind::Number, "10")),
        Ok(sheet2_b2)
    );
    let second = session.calculate();
    assert_eq!(second.result, Some(CalcValue::number(16.0)));
    assert_eq!(
        second.reads,
        [
            one_cell_read(a1, numbers(&[1.5])),
            one_cell_read(sheet2_b2, numbers(&[10.0]))
        ]
    );
    check_text(&session, &second, &EXAMPLES[1]);

    // With the Sheet2 input removed, the cell is reported missing, never given a value.
    assert_eq!(
        session.remove_input("Sheet2!B2"),
        Ok(Some(input(Kind::Number, "10")))
    );
    assert_eq!(session.input(sheet2_b2), None);
    let third = session.calculate();
    assert_eq!(
        third,
        Calculation {
            result: None,
            diagnostics: vec![Diagnostic::Eval(EvalFault::MissingInput {
                cell: sheet2_b2
            })],
            reads: vec![
                one_cell_read(a1, numbers(&[1.5])),
                one_cell_read(sheet2_b2, RunCells::Missing(1)),
            ],
        }
    );
    check_text(&session, &third, &EXAMPLES[2]);

    // The rest from a fresh set of inputs each.
    let mut checked = 3;
    for example in &EXAMPLES[3..] {
        let calculation = run_example(&mut session, example);
        check_text(&session, &calculation, example);
        checked += 1;
        match example.formula {
            "=SUM(A1:A3)" => assert_eq!(
                calculation.reads,
                [Read {
                    area: GridArea::new(a1, a3).expect("one sheet"),
                    runs: vec![
                        ReadRun {
                            start: a1,
                            cells: numbers(&[1.5])
                        },
                        ReadRun {
                            start: a2,
                            cells: RunCells::Blank(1)
                        },
                        ReadRun {
                            start: a3,
                            cells: numbers(&[2.0])
                        },
                    ],
                }]
            ),
            "=SUM(A1:A4)" => {
                assert_eq!(
                    calculation.diagnostics,
                    [Diagnostic::Eval(EvalFault::MissingInput { cell: a4 })]
                );
                assert_eq!(
                    calculation.reads,
                    [Read {
                        area: GridArea::new(a1, a4).expect("one sheet"),
                        runs: vec![
                            ReadRun {
                                start: a1,
                                cells: numbers(&[1.5])
                            },
                            ReadRun {
                                start: a2,
                                cells: RunCells::Blank(1)
                            },
                            ReadRun {
                                start: a3,
                                cells: numbers(&[2.0])
                            },
                            ReadRun {
                                start: a4,
                                cells: RunCells::Missing(1)
                            },
                        ],
                    }]
                );
            }
            "=A1*" => {
                let [Diagnostic::Syntax(syntax)] = calculation.diagnostics.as_slice() else {
                    panic!("one syntax diagnostic, got {:?}", calculation.diagnostics);
                };
                assert_eq!(syntax.span, TextSpan::new(4, 0));
                assert_eq!(syntax.kind, SyntaxDiagnosticKind::Message);
            }
            "=Sheet9!A1+1" => assert_eq!(
                calculation.diagnostics,
                [Diagnostic::Compile(CompileError::Reference {
                    span: TextSpan::new(1, 9),
                    error: BindError::UnknownSheet {
                        name: "Sheet9".to_owned()
                    },
                })]
            ),
            "=A1*3E-308-4.5000000000001E-308" => {
                let power = catalogue()
                    .resolve_surface(Notation::Infix, "-")
                    .expect("the catalogue holds -");
                assert_eq!(
                    calculation.diagnostics,
                    [Diagnostic::Eval(EvalFault::Function(
                        FunctionFault::NotImplemented {
                            id: power.id(),
                            argument: None,
                        }
                    ))]
                );
            }
            _ => {}
        }
    }

    // Addresses and values the controller refuses, each typed.
    let blank = InputValue::blank;
    assert_eq!(
        session.set_input("A1:A2", blank()),
        Err(InputError::NotACell {
            address: "A1:A2".to_owned()
        })
    );
    assert_eq!(
        session.set_input("Rate", blank()),
        Err(InputError::NotACell {
            address: "Rate".to_owned()
        })
    );
    assert_eq!(
        session.set_input("Sheet9!A1", blank()),
        Err(InputError::Address {
            address: "Sheet9!A1".to_owned(),
            error: BindError::UnknownSheet {
                name: "Sheet9".to_owned()
            },
        })
    );
    let c1 = key(s1, 1, 3);
    assert_eq!(
        session.set_input("C1", blank()),
        Err(InputError::FormulaCell { cell: c1 })
    );
    assert_eq!(session.input(c1), None);
    assert_eq!(session.set_input("A1", blank()), Ok(a1));
    assert_eq!(
        session.set_formula_cell("A1"),
        Err(InputError::FormulaCell { cell: a1 })
    );
    assert_eq!(
        session.formula_cell(),
        c1,
        "a refused formula cell leaves it in place"
    );
    for refused in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e-310] {
        assert!(
            matches!(
                InputValue::number(refused),
                Err(InputError::NumberNotHeld { value }) if value.to_bits() == refused.to_bits()
            ),
            "{refused} is refused"
        );
    }
    let zero = InputValue::number(-0.0).expect("zero is held");
    assert_eq!(
        zero.value().as_number().map(f64::to_bits),
        Some(0.0f64.to_bits()),
        "-0 is held as +0"
    );
    assert!(InputValue::text(&"a".repeat(32_767)).is_ok());
    assert_eq!(
        InputValue::text(&"a".repeat(32_768)),
        Err(InputError::TextTooLong { units: 32_768 })
    );
    // A character outside the Basic Multilingual Plane is two UTF-16 code units.
    assert_eq!(
        InputValue::text(&"\u{1F600}".repeat(16_384)),
        Err(InputError::TextTooLong { units: 32_768 })
    );

    // Sheet names the controller refuses, each typed.
    assert_eq!(
        session.add_sheet("sheet2"),
        Err(SheetError::Duplicate {
            name: "sheet2".to_owned()
        })
    );
    for invalid in [
        "",
        "Q1:Q2",
        "a/b",
        "[x]",
        "'Quoted",
        "Quoted'",
        "history",
        &"S".repeat(32),
    ] {
        assert_eq!(
            session.add_sheet(invalid),
            Err(SheetError::InvalidName {
                name: invalid.to_owned()
            }),
            "{invalid:?} is refused"
        );
    }
    assert!(session.add_sheet(&"S".repeat(31)).is_ok());
    assert!(session.add_sheet("Q2 Data").is_ok());
    assert_eq!(
        session.set_input("'Q2 Data'!B3", input(Kind::Number, "2")),
        Ok(key(session.sheets().last().expect("a sheet").1, 3, 2))
    );
    checked
}

// ---------------------------------------------------------------------------------------------
// The browser part.

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::{COLUMN, EXAMPLES, Example, FORMULA_CELL, check_lines};

    /// `=SUM(A1:A4)` in the surface: A4 is an empty cell of the scratch grid, which the surface
    /// gives the session as a blank cell, as a worksheet's empty cell is. SUM skips both blanks:
    /// 1.5 + 2.
    const SCRATCH_GRID_BLANK: Example = Example {
        name: "a range reaching an empty scratch-grid cell",
        inputs: COLUMN,
        formula: "=SUM(A1:A4)",
        result: "number 3.5",
        diagnostics: &[],
        reads: &[
            "Sheet1!A1:A4: Sheet1!A1 = number 1.5; Sheet1!A2 blank; Sheet1!A3 = number 2; Sheet1!A4 blank",
        ],
    };
    use dna_onecalc::surface::{Mounted, mount};
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{
        Element, Event, EventInit, HtmlElement, HtmlInputElement, HtmlSelectElement,
        HtmlTextAreaElement,
    };

    /// The surface mounted in a fresh element of the test page.
    pub struct Page {
        root: HtmlElement,
        _mounted: Mounted,
    }

    /// Lets the surface's reactive updates run: several turns of the event loop.
    pub async fn settle() {
        for _ in 0..5 {
            let promise = js_sys::Promise::new(&mut |resolve, _| {
                web_sys::window()
                    .expect("a window")
                    .set_timeout_with_callback(&resolve)
                    .expect("a timer");
            });
            JsFuture::from(promise).await.expect("the timer fires");
        }
    }

    fn dispatch(target: &Element, kind: &str) {
        let init = EventInit::new();
        init.set_bubbles(true);
        let event = Event::new_with_event_init_dict(kind, &init).expect("an event");
        target.dispatch_event(&event).expect("dispatched");
    }

    fn role(role: &str) -> String {
        format!("[data-onecalc=\"{role}\"]")
    }

    impl Page {
        pub fn mount() -> Page {
            let document = web_sys::window()
                .expect("a window")
                .document()
                .expect("a document");
            let root: HtmlElement = document
                .create_element("div")
                .expect("a div")
                .dyn_into()
                .expect("an HTML element");
            document
                .body()
                .expect("a body")
                .append_child(&root)
                .expect("appended");
            let mounted = mount(root.clone());
            Page {
                root,
                _mounted: mounted,
            }
        }

        fn one(&self, name: &str) -> Element {
            self.root
                .query_selector(&role(name))
                .expect("a selector")
                .unwrap_or_else(|| panic!("the surface has no {name}"))
        }

        fn all(&self, scope: &Element, selector: &str) -> Vec<Element> {
            let list = scope.query_selector_all(selector).expect("a selector");
            (0..list.length())
                .map(|i| {
                    list.item(i)
                        .expect("an item")
                        .dyn_into()
                        .expect("an element")
                })
                .collect()
        }

        pub fn type_into(&self, name: &str, text: &str) {
            type_text(&self.one(name), text);
        }

        pub fn click(&self, name: &str) {
            self.one(name)
                .dyn_into::<HtmlElement>()
                .expect("an HTML element")
                .click();
        }

        pub fn text(&self, name: &str) -> String {
            self.one(name).text_content().unwrap_or_default()
        }

        pub fn items(&self, name: &str) -> Vec<String> {
            self.all(&self.one(name), "li")
                .iter()
                .map(|item| item.text_content().unwrap_or_default())
                .collect()
        }

        pub fn rows(&self) -> Vec<Element> {
            self.all(&self.root, &role("input-row"))
        }

        fn in_row(&self, row: usize, name: &str) -> Element {
            let rows = self.rows();
            let row = rows
                .get(row)
                .unwrap_or_else(|| panic!("no input row {row}"));
            row.query_selector(&role(name))
                .expect("a selector")
                .unwrap_or_else(|| panic!("the row has no {name}"))
        }

        pub fn set_row(&self, row: usize, address: &str, kind: &str, entry: &str) {
            type_text(&self.in_row(row, "input-cell"), address);
            let select = self.in_row(row, "input-kind");
            select
                .clone()
                .dyn_into::<HtmlSelectElement>()
                .expect("a select")
                .set_value(kind);
            dispatch(&select, "change");
            type_text(&self.in_row(row, "input-value"), entry);
        }

        pub fn set_row_entry(&self, row: usize, entry: &str) {
            type_text(&self.in_row(row, "input-value"), entry);
        }

        pub fn remove_row(&self, row: usize) {
            self.in_row(row, "remove-input")
                .dyn_into::<HtmlElement>()
                .expect("an HTML element")
                .click();
        }

        pub fn row_statuses(&self) -> Vec<String> {
            (0..self.rows().len())
                .map(|row| {
                    self.in_row(row, "input-status")
                        .text_content()
                        .unwrap_or_default()
                })
                .collect()
        }

        /// Checks what the surface shows against `example`.
        pub fn check(&self, example: &Example) {
            assert_eq!(
                self.text("result"),
                example.result,
                "{}: shown result",
                example.name
            );
            check_lines(
                &self.items("diagnostics"),
                example.diagnostics,
                example.name,
                "shown diagnostics",
            );
            assert_eq!(
                self.items("reads"),
                example.reads,
                "{}: shown reads",
                example.name
            );
        }

        /// Replaces the input rows with the example's and enters its formula.
        pub async fn enter(&self, example: &Example) {
            while !self.rows().is_empty() {
                self.remove_row(0);
                settle().await;
            }
            for _ in example.inputs {
                self.click("add-input");
            }
            settle().await;
            for (row, (address, kind, entry)) in example.inputs.iter().enumerate() {
                self.set_row(row, address, kind.selector(), entry);
            }
            self.type_into("formula", example.formula);
            settle().await;
        }
    }

    /// Sets the text of an input, or of the formula's text area, and dispatches `input`.
    fn type_text(target: &Element, text: &str) {
        match target.clone().dyn_into::<HtmlInputElement>() {
            Ok(input) => input.set_value(text),
            Err(other) => other
                .dyn_into::<HtmlTextAreaElement>()
                .expect("an input or a text area")
                .set_value(text),
        }
        dispatch(target, "input");
    }

    pub async fn vba_part() {
        let page = Page::mount();
        settle().await;
        let a1 = page
            .root
            .query_selector("[data-onecalc=\"grid-cell\"][data-cell=\"A1\"]")
            .unwrap()
            .unwrap();
        type_text(&a1, "40");
        page.type_into("formula", "=Module1.Add2(A1,2)");
        page.click("vba-load");
        settle().await;
        assert!(page.text("vba-status").starts_with("Loaded:"));
        assert_eq!(page.text("result"), "number 42");
        page.type_into(
            "vba-source",
            &calc_vba::EXAMPLE_MODULE.replace("a + b", "a + b + 10"),
        );
        page.click("vba-load");
        settle().await;
        assert_eq!(page.text("result"), "number 52");
        page.type_into("vba-source", "Function Broken(\n");
        page.click("vba-load");
        settle().await;
        assert!(page.text("vba-status").starts_with("Module not loaded:"));
        assert_eq!(page.text("result"), "number 52");
        page.type_into(
            "vba-source",
            "Public Function Spin()\n On Error Resume Next\n Do\n Loop\nEnd Function\n",
        );
        page.click("vba-load");
        page.type_into("formula", "=Spin()");
        settle().await;
        assert_eq!(page.text("result-value"), "#VALUE!");
        assert!(page.text("diagnostics").contains("budget"));
        page.click("vba-clear");
        settle().await;
        assert_eq!(page.text("result-value"), "#NAME?");
    }

    /// The browser part of the reference case. Returns the number of examples it checked.
    pub async fn surface_part() -> usize {
        let page = Page::mount();
        settle().await;
        assert_eq!(page.text("result"), "no formula");
        assert_eq!(page.text("sheets"), "Sheet1");

        page.type_into("formula-cell", FORMULA_CELL);
        page.type_into("sheet-name", "Sheet2");
        page.click("add-sheet");
        settle().await;
        assert_eq!(page.text("sheets"), "Sheet1, Sheet2");
        assert_eq!(page.text("formula-cell-status"), "Sheet1!C1");
        page.click("add-sheet");
        settle().await;
        assert_eq!(
            page.text("sheet-status"),
            "there is already a sheet named Sheet2"
        );

        // Two supplied cells, typed into two rows.
        page.enter(&EXAMPLES[0]).await;
        page.check(&EXAMPLES[0]);
        assert_eq!(
            page.row_statuses(),
            ["Sheet1!A1 = number 1.5", "Sheet2!B2 = number 4"]
        );

        // The Sheet2 row's value edited.
        page.set_row_entry(1, "10");
        settle().await;
        page.check(&EXAMPLES[1]);

        // The Sheet2 row removed.
        page.remove_row(1);
        settle().await;
        assert_eq!(page.rows().len(), 1);
        page.check(&EXAMPLES[2]);

        let mut checked = 3;
        for example in &EXAMPLES[3..] {
            // Sheet1!A4 is a cell of the surface's scratch grid (dc-q4y), which is blank while
            // nothing is typed in it, so in the surface this range reads a blank where the
            // controller alone has no input for the cell.
            let example = if example.formula == "=SUM(A1:A4)" {
                &SCRATCH_GRID_BLANK
            } else {
                example
            };
            page.enter(example).await;
            page.check(example);
            checked += 1;
        }

        // Retyping row 1 as the formula's cell C1 is refused there, and it removes the row that
        // supplied Sheet2!B2, so =A1*Sheet2!B2+1 has no input for Sheet2!B2 and is not calculated.
        page.enter(&EXAMPLES[0]).await;
        page.set_row(1, "C1", "number", "4");
        settle().await;
        assert_eq!(
            page.row_statuses(),
            [
                "Sheet1!A1 = number 1.5",
                "Sheet1!C1 is the formula's cell, which holds no input"
            ]
        );
        assert_eq!(page.text("result"), "not calculated");
        checked
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn onecalc_m1_1__reference_without_engine() {
    let examples = controller_part();
    note!(
        "ACCEPT-NOTE: onecalc_m1_1__reference_without_engine: {examples} examples calculated through dna_onecalc::Session (calc_fml::compile, instantiate and evaluate over the session's sheets and inputs); each result, diagnostic and read compared with hand-derived values; the refusal of =A1*3E-308-4.5000000000001E-308 and the missing-input stops are the implementation's answers, not Excel's values; not compared with Excel here"
    );
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test]
async fn onecalc_m1_1__reference_without_engine() {
    let examples = controller_part();
    let shown = browser::surface_part().await;
    note!(
        "ACCEPT-NOTE: onecalc_m1_1__reference_without_engine: {examples} examples calculated through dna_onecalc::Session, and {shown} entered in the mounted surface through dispatched input, change and click events and read back from its DOM; each result, diagnostic and read compared with hand-derived values; the refusal of =A1*3E-308-4.5000000000001E-308 and the missing-input stops are the implementation's answers, not Excel's values; not compared with Excel here"
    );
}

// ---------------------------------------------------------------------------------------------
// The dependency case.

#[cfg(not(target_arch = "wasm32"))]
mod metadata {
    use serde_json::Value;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// The triples the architecture check resolves (tools/xtask: SHIPPED_TARGETS), plus the host
    /// triple when it is none of them.
    pub const SHIPPED: [&str; 3] = [
        "x86_64-pc-windows-msvc",
        "x86_64-unknown-linux-gnu",
        "wasm32-unknown-unknown",
    ];

    pub fn cargo() -> String {
        std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned())
    }

    pub fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("the workspace root")
    }

    /// The host triple cargo reports.
    pub fn host() -> String {
        let out = Command::new(cargo())
            .arg("-vV")
            .output()
            .expect("cargo -vV runs");
        assert!(out.status.success(), "cargo -vV failed");
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .expect("cargo -vV names its host")
            .trim()
            .to_owned()
    }

    /// The resolved graph for one triple: package names by ID, and each package's dependencies
    /// with the kinds of each edge (`normal`, `build`, `dev`).
    pub struct Graph {
        pub names: BTreeMap<String, String>,
        pub edges: BTreeMap<String, Vec<(String, BTreeSet<String>)>>,
        pub workspace: BTreeMap<String, String>,
    }

    pub fn resolve(root: &Path, target: &str) -> Graph {
        let out = Command::new(cargo())
            .args([
                "metadata",
                "--format-version",
                "1",
                "--all-features",
                "--locked",
                "--filter-platform",
                target,
            ])
            .current_dir(root)
            .output()
            .expect("cargo metadata runs");
        assert!(
            out.status.success(),
            "cargo metadata --filter-platform {target} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let metadata: Value = serde_json::from_slice(&out.stdout).expect("cargo metadata's JSON");
        let names: BTreeMap<String, String> = metadata["packages"]
            .as_array()
            .expect("packages")
            .iter()
            .map(|package| {
                (
                    package["id"].as_str().expect("an id").to_owned(),
                    package["name"].as_str().expect("a name").to_owned(),
                )
            })
            .collect();
        let workspace = metadata["workspace_members"]
            .as_array()
            .expect("workspace members")
            .iter()
            .map(|id| {
                let id = id.as_str().expect("an id").to_owned();
                (names[&id].clone(), id)
            })
            .collect();
        let mut edges = BTreeMap::new();
        for node in metadata["resolve"]["nodes"]
            .as_array()
            .expect("resolve nodes")
        {
            let id = node["id"].as_str().expect("a node id").to_owned();
            let deps = node["deps"]
                .as_array()
                .expect("deps")
                .iter()
                .map(|dep| {
                    let kinds = dep["dep_kinds"]
                        .as_array()
                        .expect("dep kinds")
                        .iter()
                        .map(|kind| kind["kind"].as_str().unwrap_or("normal").to_owned())
                        .collect();
                    (dep["pkg"].as_str().expect("a package id").to_owned(), kinds)
                })
                .collect();
            edges.insert(id, deps);
        }
        Graph {
            names,
            edges,
            workspace,
        }
    }

    impl Graph {
        /// The names of every package `root` builds with: its own normal, build and dev
        /// dependencies, then their normal and build dependencies, transitively.
        pub fn closure(&self, root: &str) -> BTreeSet<String> {
            let mut seen = BTreeSet::from([root.to_owned()]);
            let mut pending = vec![root.to_owned()];
            let mut names = BTreeSet::new();
            while let Some(id) = pending.pop() {
                for (dep, kinds) in self.edges.get(&id).into_iter().flatten() {
                    let followed =
                        id == root || kinds.iter().any(|kind| kind == "normal" || kind == "build");
                    if followed && seen.insert(dep.clone()) {
                        names.insert(self.names[dep].clone());
                        pending.push(dep.clone());
                    }
                }
            }
            names
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn onecalc_m1_1__dependency_gate() {
    let root = metadata::workspace_root();
    let host = metadata::host();
    let mut targets: Vec<String> = metadata::SHIPPED.iter().map(|t| (*t).to_owned()).collect();
    if !targets.contains(&host) {
        targets.push(host);
    }
    let forbidden = ["calc-engine", "calc-host"];
    let mut sizes = Vec::new();
    for target in &targets {
        let graph = metadata::resolve(&root, target);
        // The forbidden packages are real nodes of this graph, so their absence below is a
        // property of dna-onecalc's dependencies, not of a graph that lacks them.
        for name in forbidden {
            assert!(
                graph.workspace.contains_key(name),
                "[{target}] {name} is a workspace member of the resolved graph"
            );
        }
        let onecalc = &graph.workspace["dna-onecalc"];
        let closure = graph.closure(onecalc);
        // The walk reaches the formula libraries, the UI stack and, through the test-only edges,
        // the test harness: it follows path, registry and dev edges.
        for reached in ["calc-fml", "calc-func", "calc-value", "leptos", "web-sys"] {
            assert!(
                closure.contains(reached),
                "[{target}] dna-onecalc's closure reaches {reached}: {closure:?}"
            );
        }
        let harness = if target == "wasm32-unknown-unknown" {
            "wasm-bindgen-test"
        } else {
            "serde_json"
        };
        assert!(
            closure.contains(harness),
            "[{target}] the walk follows dna-onecalc's dev edges to {harness}"
        );
        // The walk reaches what dna-onecalc itself depends on: CalcFml and, through it, CalcFunc.
        for owned in ["calc-fml", "calc-func"] {
            assert!(
                closure.contains(owned),
                "[{target}] the walk from dna-onecalc reaches {owned}"
            );
        }
        let found: Vec<&str> = forbidden
            .into_iter()
            .filter(|name| closure.contains(*name))
            .collect();
        assert!(
            found.is_empty(),
            "[{target}] dna-onecalc's package graph contains {found:?}"
        );
        sizes.push(format!("{target}: {} packages", closure.len()));
    }
    note!(
        "ACCEPT-NOTE: onecalc_m1_1__dependency_gate: cargo metadata --all-features --locked --filter-platform for each of {}; dna-onecalc's package graph (its normal, build and dev edges, then normal and build edges) holds neither calc-engine nor calc-host ({})",
        targets.join(", "),
        sizes.join("; ")
    );
}

// dc-snz.3 uses the same module-loading/controller route on desktop and in the mounted UI.
fn vba_controller_part() {
    let mut session = Session::new("Sheet1").unwrap();
    session.set_formula_cell("H1").unwrap();
    session
        .set_input("A1", InputValue::number(40.0).unwrap())
        .unwrap();
    session
        .load_vba_module("Math", calc_vba::EXAMPLE_MODULE, 1000)
        .unwrap();
    session.set_formula("=Math.Add2(A1,2)");
    assert_eq!(session.calculate().result, Some(CalcValue::number(42.0)));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn dc_snz_3__vba() {
    vba_controller_part();
    note!("ACCEPT-NOTE: dc_snz_3__vba: Math.Add2(A1,2) through native OneCalc Session");
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test]
async fn dc_snz_3__vba() {
    vba_controller_part();
    browser::vba_part().await;
    note!(
        "ACCEPT-NOTE: dc_snz_3__vba: qualified Add2(A1,2) through OneCalc Session; browser additionally drives load, reload, failed load, budget and unload through the mounted surface"
    );
}

/// W111's recorded Version2 LEN results and Microsoft's five-function Unicode contract.
/// Version1 and B-function controls preserve the earlier UTF-16 behavior. LEFT/RIGHT retain
/// their separately measured policy; no locale or formula-form guess selects a version.
/// Fn8's exact SEARCH controls correct the earlier inferred wildcard-width assertion.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn workbook_compatibility_version_controls_only_the_five_text_counters() {
    use calc_func::locale::{CalcEnvironment, DateSystem};
    use calc_func::value::ExcelText;
    let units = |value: &[u16]| CalcValue::text(ExcelText::from_utf16_code_units(value.to_vec()));
    let mut session = Session::new("Sheet1").unwrap();
    for version in [1, 2] {
        let mut environment = CalcEnvironment::dates(DateSystem::System1900);
        environment.compatibility_version = version;
        session.set_environment(Some(environment));
        for (formula, expected) in [
            ("=LEN(\"A😀B\")", CalcValue::number(if version == 1 { 4.0 } else { 3.0 })),
            ("=LEN(\"👩‍💻\")", CalcValue::number(if version == 1 { 5.0 } else { 3.0 })),
            ("=LENB(\"A😀B\")", CalcValue::number(4.0)),
            ("=MID(\"A😀B\",2,1)", if version == 1 { units(&[0xD83D]) } else { units(&[0xD83D, 0xDE00]) }),
            ("=MID(\"A😀B\",3,1)", if version == 1 { units(&[0xDE00]) } else { units(&[66]) }),
            ("=MIDB(\"A😀B\",2,1)", units(&[0xD83D])),
            ("=FIND(\"B\",\"A😀B\")", CalcValue::number(if version == 1 { 4.0 } else { 3.0 })),
            ("=FINDB(\"B\",\"A😀B\")", CalcValue::number(4.0)),
            ("=SEARCH(\"?B\",\"A😀B\",2)", if version == 1 { CalcValue::number(3.0) } else { CalcValue::error(calc_func::value::WorksheetErrorCode::Value) }),
            ("=SEARCH(\"??B\",\"A😀B\",2)", CalcValue::number(2.0)),
            ("=SEARCHB(\"?B\",\"A😀B\",2)", CalcValue::number(3.0)),
            ("=REPLACE(\"A😀B\",2,1,\"x\")", if version == 1 { units(&[65,120,0xDE00,66]) } else { units(&[65,120,66]) }),
            ("=REPLACEB(\"A😀B\",2,1,\"x\")", units(&[65,120,0xDE00,66])),
            ("=LEFT(\"A😀B\",2)", units(&[65,0xD83D,0xDE00])),
            ("=RIGHT(\"A😀B\",2)", units(&[0xD83D,0xDE00,66])),
        ] {
            session.set_formula(formula);
            let calculated = session.calculate();
            assert_eq!(calculated.result, Some(expected), "version{version} {formula}: {:?}", calculated.diagnostics);
        }
    }
    let mut unsupported = CalcEnvironment::dates(DateSystem::System1900);
    unsupported.compatibility_version = 3;
    session.set_environment(Some(unsupported));
    session.set_formula("=LEN(\"A😀B\")");
    assert!(session.calculate().result.is_none(), "an unsupported version must not choose an inferred counting policy");
}

/// Original W111 POWER width and reciprocal captures: every base-minus-one reading
/// in r5/r6 and every exponent-minus-one reading in r1/r3. The same application
/// evaluator handles the function and operator; the latter is a shared-kernel control.
/// Also retain every original r3 subnormal result from normal positive inputs.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_power_unit_boundaries_use_the_kernel() {
    use calc_func::locale::{CalcEnvironment, DateSystem};
    let set = calc_verdict::WitnessSet::decode(include_bytes!(
        "../../../crates/calc-func/tests/witnesses/POWER.json"
    )).unwrap();
    let mut environment = CalcEnvironment::dates(DateSystem::System1900);
    environment.compatibility_version = 2;
    let mut session = Session::new("Sheet1").unwrap();
    session.set_environment(Some(environment));
    session.set_formula_cell("J20").unwrap();
    let mut rows = 0;
    let mut subnormal_rows = 0;
    let mut failures = Vec::new();
    for row in &set.witnesses {
        let args: Vec<_> = row.args.iter().map(|value|
            calc_verdict::decode_argument(value, "POWER").unwrap().as_number().unwrap()
        ).collect();
        let expected = calc_verdict::decode_result(&row.expected).unwrap();
        let unit_boundary = args[0] == -1.0
            || (matches!(row.reading.as_str(), "r1" | "r3") && args[1] == -1.0);
        let subnormal_result = expected.as_number().is_some_and(|n| n.is_subnormal());
        if !unit_boundary && !subnormal_result { continue; }
        assert!(!(unit_boundary && subnormal_result));
        if unit_boundary { rows += 1; } else { subnormal_rows += 1; }
        session.set_input("A1", InputValue::number(args[0]).unwrap()).unwrap();
        session.set_input("B1", InputValue::number(args[1]).unwrap()).unwrap();
        for formula in ["=POWER(A1,B1)", "=A1^B1"] {
            session.set_formula(formula);
            let result = session.calculate();
            if result.result.as_ref() != Some(&expected) || !result.diagnostics.is_empty() {
                failures.push(format!("{} {formula}: expected {expected:?}, calculated {:?}; {:?}",
                    row.id, result.result, result.diagnostics));
            }
        }
    }
    assert_eq!(rows, 1098, "all unchanged source observations must run");
    assert_eq!(subnormal_rows, 4, "all original r3 subnormal observations must run");
    assert!(failures.is_empty(), "{} of {} application checks differ:\n{}",
        failures.len(), 2 * (rows + subnormal_rows), failures.join("\n"));
}

/// Original W111 MEDIAN cancellation results from normal inputs, including every
/// retained subnormal result. The same Session route handles cells and held arrays.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_median_subnormal_results_use_the_kernel() {
    let set = calc_verdict::WitnessSet::decode(include_bytes!(
        "../../../crates/calc-func/tests/witnesses/MEDIAN.json"
    )).unwrap();
    let mut session = Session::new("Sheet1").unwrap();
    session.set_formula_cell("J20").unwrap();
    let mut rows = 0;
    let mut failures = Vec::new();
    for row in &set.witnesses {
        let expected = calc_verdict::decode_result(&row.expected).unwrap();
        if !expected.as_number().is_some_and(|n| n.is_subnormal()) { continue; }
        assert_eq!(row.args.len(), 2);
        for (cell, argument) in ["A1", "B1"].into_iter().zip(&row.args) {
            let number = calc_verdict::decode_argument(argument, "MEDIAN")
                .unwrap().as_number().unwrap();
            assert!(number.is_normal(), "{}: normal inputs distinguish publication", row.id);
            session.set_input(cell, InputValue::number(number).unwrap()).unwrap();
        }
        rows += 1;
        for formula in ["=MEDIAN(A1,B1)", "=MEDIAN(A1:B1)", "=MEDIAN(HSTACK(A1,B1))"] {
            session.set_formula(formula);
            let result = session.calculate();
            if result.result.as_ref() != Some(&expected) || !result.diagnostics.is_empty() {
                failures.push(format!("{} {formula}: expected {expected:?}, calculated {:?}; {:?}",
                    row.id, result.result, result.diagnostics));
            }
        }
    }
    assert_eq!(rows, 167, "every original subnormal observation must run");
    assert!(failures.is_empty(), "{} of {} application checks differ:\n{}",
        failures.len(), 3 * rows, failures.join("\n"));
}
