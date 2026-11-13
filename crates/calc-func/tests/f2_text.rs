//! F2 (dc-ip6): the text and logical functions ported from OxFunc, and the folded-in rules, called
//! through `calc_func::invoke` as the formula evaluator calls them. Expected values are Excel's
//! (16.0.20430 on this machine, Windows locale en-ZA, ANSI code page 1252, read ad hoc through COM
//! where a row says "Excel:"), Microsoft's documented results, or OxFunc's tests (OxFunc
//! 6fca3518, functions/).

mod f1_support;

use calc_func::value::WorksheetErrorCode::{Blocked, Calc, Connect, Div0, NA, Value};
use f1_support::*;

/// Excel 16.0.20527 numeric-format controls, without a supplied locale.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn text_digit_only_format_needs_no_locale() {
    use calc_func::locale::{CalcEnvironment, DateSystem};
    let env = CalcEnvironment::dates(DateSystem::System1900);
    for (value, code, expected) in [
        (-12.34, 0.0, "-12"),
        (0.5, 0.0, "1"),
        (-12.34, 1.0, "-1"),
        (12.34, 10.0, "112"),
        (-1.0, 45000.0, "-45001"),
        (0.0, 365.0, "365"),
        (-0.1, 365.0, "365"),
        (-0.5, 365.0, "-365"),
        (-12.34, -10.0, "--112"),
        (-0.1, -365.0, "-365"),
        (-0.5, -365.0, "--365"),
    ] {
        text(
            call_in(&env, None, None, "TEXT", vec![n(value), n(code)]),
            expected,
        );
    }
    text(
        call_in(&env, None, None, "TEXT", vec![n(12.34), t("0000")]),
        "0012",
    );
    assert!(call_in(&env, None, None, "TEXT", vec![n(12.34), t("0.00")]).is_err());
    let with_locale = CalcEnvironment {
        profile: Some(calc_func::locale::format_profile(
            calc_func::locale::LocaleId::EnZa,
        )),
        ..env
    };
    text(
        call_in(&with_locale, None, None, "TEXT", vec![n(-0.1), t("365")]),
        "365",
    );
    text(
        call_in(
            &with_locale,
            None,
            None,
            "TEXT",
            vec![n(-0.1), t("\"abc\"")],
        ),
        "-abc",
    );
}

/// Excel 16.0.20527 readings with a blank format, plain numeric text and
/// decimal separators supplied without a full locale profile.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn text_without_locale_uses_only_known_format_facts() {
    use calc_func::locale::{CalcEnvironment, DateSystem};
    use calc_func::value::{CalcValue, NumericMode};
    use calc_func::{EvalServices, FunctionCallScratch, Notation, catalogue, invoke};
    let env = CalcEnvironment::dates(DateSystem::System1900);
    for (value, format, expected) in [(-0.1, "", "-"), (0.5, "", "")] {
        text(
            call_in(&env, None, None, "TEXT", vec![n(value), t(format)]),
            expected,
        );
    }
    text(
        call_in(&env, None, None, "TEXT", vec![n(-12.0), CalcValue::empty()]),
        "-",
    );
    let with_locale = CalcEnvironment {
        profile: Some(calc_func::locale::format_profile(
            calc_func::locale::LocaleId::EnZa,
        )),
        ..env
    };
    text(
        call_in(&with_locale, None, None, "TEXT", vec![n(-0.1), t("")]),
        "-",
    );
    text(
        call_in(&env, None, None, "TEXT", vec![t("-0.76574825"), n(-1.0)]),
        "--1",
    );
    text(call_in(&env, None, None, "TEXT", vec![t(""), n(0.5)]), "");
    for (value, code) in [
        ("hello", 1e100),
        ("TRUE", 0.0),
        ("March", -1.0),
        ("abc def", 0.5),
    ] {
        text(
            call_in(&env, None, None, "TEXT", vec![t(value), n(code)]),
            value,
        );
    }
    assert!(call_in(&env, None, None, "TEXT", vec![t("1,234"), n(0.0)]).is_err());
    assert!(call_in(&env, None, None, "TEXT", vec![t("2026-01-02"), n(0.0)]).is_err());
    // Excel 20527: an unrecognised comma in numeric-only text is not a number
    // when the recorded decimal and group separators are dot and NBSP.
    for value in ["1,234", "12,345", "123,456"] {
        let mut services = EvalServices::new(&NumericMode::Fast)
            .with_environment(&env)
            .with_number_separators(".", "\u{00a0}");
        let target = catalogue().resolve_surface(Notation::Call, "TEXT").unwrap();
        text(
            invoke(target, &mut Given(vec![t(value), n(0.0)]), &mut services, &mut FunctionCallScratch::new()),
            value,
        );
    }
    let mut comma_group = EvalServices::new(&NumericMode::Fast)
        .with_environment(&env)
        .with_number_separators(".", ",");
    let text_target = catalogue().resolve_surface(Notation::Call, "TEXT").unwrap();
    refused(invoke(
        text_target,
        &mut Given(vec![t("1,234"), n(0.0)]),
        &mut comma_group,
        &mut FunctionCallScratch::new(),
    ));
    let target = catalogue().resolve_surface(Notation::Call, "TEXT").unwrap();
    let mode = NumericMode::Fast;
    for (value, code, expected) in [
        (-12.34, 0.5, "-12.5"),
        (0.5, -0.5, "-1.5"),
        (12.34, 10.25, "112.25"),
    ] {
        let mut services = EvalServices::new(&mode)
            .with_environment(&env)
            .with_number_separators(".", ",");
        text(
            invoke(
                target,
                &mut Given(vec![n(value), n(code)]),
                &mut services,
                &mut FunctionCallScratch::new(),
            ),
            expected,
        );
    }
    let mut services = EvalServices::new(&mode)
        .with_environment(&env)
        .with_number_separators(".", ",");
    text(
        invoke(
            target,
            &mut Given(vec![n(0.1), t("0.0")]),
            &mut services,
            &mut FunctionCallScratch::new(),
        ),
        "0.1",
    );
    for (value, code, expected) in [
        (0.48343425755440994, "1.0", "1.5"),
        (4186.0, "1.0", "14186.0"),
        (0.08839343501527819, "12.34", "12.34"),
        (0.5977337408497776, "12.34", "121.34"),
        (-0.7591400564284259, "-2.0", "--2.8"),
    ] {
        let mut services = EvalServices::new(&mode)
            .with_environment(&env)
            .with_number_separators(".", "\u{00A0}");
        text(
            invoke(
                target,
                &mut Given(vec![n(value), t(code)]),
                &mut services,
                &mut FunctionCallScratch::new(),
            ),
            expected,
        );
    }
    for (value, code, expected) in [
        (-12.34, "1,234", "-1,234"),
        (-0.1, "1,234", "1,234"),
        (12.34, "10,2", "112,2"),
    ] {
        let mut services = EvalServices::new(&mode)
            .with_environment(&env)
            .with_number_separators(".", "\u{00A0}");
        text(
            invoke(
                target,
                &mut Given(vec![n(value), t(code)]),
                &mut services,
                &mut FunctionCallScratch::new(),
            ),
            expected,
        );
    }
    let mut services = EvalServices::new(&mode)
        .with_environment(&env)
        .with_number_separators(",", ".");
    assert!(
        invoke(
            target,
            &mut Given(vec![n(12.34), t("1,234")]),
            &mut services,
            &mut FunctionCallScratch::new()
        )
        .is_err()
    );
}

/// Excel 16.0.20527: literal mantissas, exponent literals, carry boundaries,
/// signs and a numeric format converted to scientific text.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn text_scientific_literal_mantissa() {
    use calc_func::locale::{CalcEnvironment, DateSystem};
    let env = CalcEnvironment::dates(DateSystem::System1900);
    for (value, code, expected) in [
        (0.0, "1E+00", "1E+00"),
        (4.9, "1E+00", "1E+01"),
        (4.99999999999999, "1E+00", "1E+01"),
        (4.999999999999999, "1E+00", "1E+02"),
        (5.0, "1E+00", "1E+02"),
        (49.0, "1E+00", "1E+01"),
        (99.0, "1E+00", "1E+03"),
        (0.005, "1E+00", "1E-01"),
        (0.005, "1E-201", "1E-211"),
        (-5.0, "-1E+100", "--1E+102"),
        (5.0, "12E+3400", "12E+3402"),
        (5e50, "1E+00", "1E+52"),
    ] {
        text(
            call_in(&env, None, None, "TEXT", vec![n(value), t(code)]),
            expected,
        );
    }
    text(
        call_in(&env, None, None, "TEXT", vec![n(0.5), n(1e-100)]),
        "1E101",
    );
    text(
        call_in(&env, None, None, "TEXT", vec![n(12.34), n(1e100)]),
        "1E+101",
    );

    // Excel 20527 bank2-more-TEXT-0305 and bank2-scalar-TEXT-1029:
    // numeric codes become scientific format text before rendering.
    use calc_func::value::{CalcValue, NumericMode};
    use calc_func::{EvalServices, FunctionCallScratch, Notation, catalogue, invoke};
    let target = catalogue().resolve_surface(Notation::Call, "TEXT").unwrap();
    let mode = NumericMode::Fast;
    for (value, code, expected) in [
        (0.5, f64::from_bits(0x398daa22f02e1330), "1.82822185543783E31"),
        (
            f64::from_bits(0x3fd5712f01a17496),
            f64::from_bits(0x4ea5c4cf23f3d7f0),
            "7.51213186318725E+71",
        ),
    ] {
        let mut services = EvalServices::new(&mode)
            .with_environment(&env)
            .with_number_separators(".", "\u{00A0}");
        text(
            invoke(
                target,
                &mut Given(vec![n(value), n(code)]),
                &mut services,
                &mut FunctionCallScratch::new(),
            ),
            expected,
        );
    }
    // Recorded placeholder cases cover signs, a negative exponent and a blank value.
    for (value, code, expected) in [
        (t("-1"), 0x3225c9a77f0d2600, "-4.14075577552457E67"),
        (
            n(f64::from_bits(0x36ed27a8c689e724)),
            0x2f56d2cd1df017a8,
            "1.24304623427929E-843",
        ),
        (CalcValue::empty(), 0xd049f2a9f32eef1c, "-6.00912443166741E+78"),
    ] {
        let mut services = EvalServices::new(&mode)
            .with_environment(&env)
            .with_number_separators(".", "\u{00A0}");
        text(
            invoke(
                target,
                &mut Given(vec![value, n(f64::from_bits(code))]),
                &mut services,
                &mut FunctionCallScratch::new(),
            ),
            expected,
        );
    }
    assert!(call_in(
        &env,
        None,
        None,
        "TEXT",
        vec![n(0.5), n(f64::from_bits(0x398daa22f02e1330))],
    )
    .is_err());
}

/// Excel 20527 bank2 TEXT rows: literal letters and negative date codes can
/// be decided with recorded separators, while positive date rendering needs locale.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn text_partial_locale_literal_and_negative_date() {
    use calc_func::locale::{CalcEnvironment, DateSystem};
    use calc_func::value::NumericMode;
    use calc_func::{EvalServices, FunctionCallScratch, Notation, catalogue, invoke};
    let env = CalcEnvironment::dates(DateSystem::System1900);
    let target = catalogue().resolve_surface(Notation::Call, "TEXT").unwrap();
    let mode = NumericMode::Fast;
    for (bits, expected) in [
        (0x3f8b33dcdb340840, "i"),
        (0xaff28fbe97f89608, "-i"),
    ] {
        let mut services = EvalServices::new(&mode)
            .with_environment(&env)
            .with_number_separators(".", "\u{00A0}");
        text(
            invoke(
                target,
                &mut Given(vec![n(f64::from_bits(bits)), t("i")]),
                &mut services,
                &mut FunctionCallScratch::new(),
            ),
            expected,
        );
    }
    for (bits, code) in [
        (0xc033000000000001, "abc def"),
        (0xc054c00000000001, "hello"),
    ] {
        let mut services = EvalServices::new(&mode)
            .with_environment(&env)
            .with_number_separators(".", "\u{00A0}");
        error(
            invoke(
                target,
                &mut Given(vec![n(f64::from_bits(bits)), t(code)]),
                &mut services,
                &mut FunctionCallScratch::new(),
            ),
            Value,
        );
    }
    // Excel 20527: uppercase E without scientific syntax and lowercase
    // scientific-looking codes are invalid, including under several locale tags.
    for code in ["E", "TRUE", "1E2", "1e2", "1e+100", "-1e+100"] {
        let mut services = EvalServices::new(&mode)
            .with_environment(&env)
            .with_number_separators(".", "\u{00A0}");
        error(invoke(target, &mut Given(vec![n(0.5), t(code)]), &mut services, &mut FunctionCallScratch::new()), Value);
    }
    let mut services = EvalServices::new(&mode)
        .with_environment(&env)
        .with_number_separators(".", "\u{00A0}");
    text(
        invoke(target, &mut Given(vec![n(0.5), t("1E+0")]), &mut services, &mut FunctionCallScratch::new()),
        "1E+1",
    );
    let mut services = EvalServices::new(&mode)
        .with_environment(&env)
        .with_number_separators(".", "\u{00A0}");
    refused(invoke(
        target,
        &mut Given(vec![n(1.0), t("hello")]),
        &mut services,
        &mut FunctionCallScratch::new(),
    ));
}

/// Excel 16.0.20430, recorded typed Value2 observations, not legacy expected values:
/// OxFunc-Evidence/runs/w111-broad-typed-20260929-002/outcomes/excel.jsonl.gz, cases
/// w111typed-00266-textafter-arg1-empty-text, -00274-textbefore-arg1-empty-text and
/// -00282-textsplit-arg1-empty-text. Raw source SHA-256:
/// bd0276ca12ea325000d5ef4ec3536071a53b4efa8a0c9f0518b866634db6d671.
/// These scalar inputs have no compatibility-version ambiguity.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn empty_text_delimiter_errors_match_the_recorded_excel_values() {
    error(call("TEXTBEFORE", vec![t(""), t("-")]), NA);
    error(call("TEXTAFTER", vec![t(""), t("-")]), NA);
    error(call("TEXTSPLIT", vec![t(""), t(",")]), Value);
}

/// Excel 16.0.20527, en-ZA: the catalogue's recorded iteration-mark prefix comparison. The
/// enclosing operator reaches the same kernel as a formula; the corpus also exercises the
/// workbook through calc-host, calc-engine and calc-fml.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn iteration_prefix_and_measured_han_repeat_order() {
    use calc_func::value::NumericMode;
    use calc_func::{EvalServices, FunctionCallScratch, Notation, catalogue, invoke};
    use calc_verdict::{CaptureRecord, Observation};

    let capture = CaptureRecord::decode(include_bytes!(
        "captures/catalogue/sections/dczsn2_collation.capture.json"
    ))
    .expect("the committed Excel capture");
    let observed = capture
        .observations
        .iter()
        .find(|o| {
            o.cell.sheet() == "dczsn2_collation" && o.cell.row() == 61 && o.cell.column() == 4
        })
        .expect("D61 was read from Excel");
    let Observation::ExcelRaw(value) = &observed.observation else {
        panic!("this test needs an Excel Value2 observation");
    };
    let expected = value.as_logical().expect("Excel returned a logical");
    assert!(expected);
    let target = catalogue().resolve_surface(Notation::Infix, ">").unwrap();
    let mode = NumericMode::Fast;
    let result = invoke(
        target,
        &mut Given(vec![t("時々"), t("時")]),
        &mut EvalServices::new(&mode),
        &mut FunctionCallScratch::new(),
    )
    .expect("the strict primary prefix decides this comparison");
    assert_eq!(result.as_logical(), Some(expected));
    // Fn1 D2 now measures the Han repeat order separately from the earlier prefix reading.
    // Fn3 records the exact bare-prefix comparison too. Fn7 later measures the
    // punctuation primary that decides different prefixes with completed primaries.
    let order = |a: &str, b: &str| {
        calc_func::text_order(
            &a.encode_utf16().collect::<Vec<_>>(),
            &b.encode_utf16().collect::<Vec<_>>(),
        )
    };
    assert_eq!(order("時々", "時時"), Some(std::cmp::Ordering::Less));
    let fn3 = CaptureRecord::decode(include_bytes!("captures/fn3_questions/oracle.capture.json"))
        .expect("the retained fn3 Excel capture");
    let relation = fn3
        .formulas
        .iter()
        .find(|f| f.formula.as_deref() == Some("=\"カ-\"<\"カ-ゝ\""))
        .expect("the same prefix has a measured reverse relation");
    let observed = fn3
        .observations
        .iter()
        .find(|o| o.cell == relation.cell)
        .unwrap();
    let Observation::ExcelRaw(value) = &observed.observation else {
        panic!("a raw reading");
    };
    assert_eq!(value.as_logical(), Some(true));
    assert_eq!(order("カ-ゝ", "カ-"), Some(std::cmp::Ordering::Greater));
    assert_eq!(
        order("カ-ゝ", "カゝ"),
        Some(std::cmp::Ordering::Less),
        "fn7 measures the punctuation primary before the repeated kana"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn char_code_unichar_unicode() {
    text(call("CHAR", vec![n(65.0)]), "A");
    // Excel: CHAR(128) is the euro sign, CHAR(150) an en dash (code page 1252).
    text(call("CHAR", vec![n(128.0)]), "\u{20AC}");
    text(call("CHAR", vec![n(150.0)]), "\u{2013}");
    text(call("CHAR", vec![n(129.0)]), "\u{0081}");
    text(call("CHAR", vec![n(255.9)]), "\u{00FF}");
    error(call("CHAR", vec![n(0.0)]), Value);
    error(call("CHAR", vec![n(256.0)]), Value);
    // Excel: CODE(UNICHAR(8364)) is 128, CODE(UNICHAR(1000)) 63, CODE(UNICHAR(233)) 233.
    exact(call("CODE", vec![t("\u{20AC}")]), 128.0);
    exact(call("CODE", vec![t("\u{03E8}")]), 63.0);
    exact(call("CODE", vec![t("\u{00E9}")]), 233.0);
    exact(call("CODE", vec![t("\u{1F600}")]), 63.0);
    error(call("CODE", vec![t("")]), Value);
    text(call("UNICHAR", vec![n(9731.0)]), "\u{2603}");
    error(call("UNICHAR", vec![n(0.0)]), Value);
    error(call("UNICHAR", vec![n(55296.0)]), NA);
    exact(call("UNICODE", vec![t("\u{20AC}")]), 8364.0);
    exact(call("UNICODE", vec![t("\u{1F600}x")]), 128512.0);
    error(call("UNICODE", vec![t("")]), Value);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn slices_and_lengths() {
    exact(call("LEN", vec![t("abc")]), 3.0);
    exact(call("LEN", vec![n(12.5)]), 4.0);
    // Excel 16.0.20527 (dc-fl2, the catalogue's sheet dcfl2_text): LEN counts UTF-16 code units,
    // so a surrogate pair is 2.
    exact(call("LEN", vec![t("\u{1F600}")]), 2.0);
    exact(call("LENB", vec![t("\u{1F600}")]), 2.0);
    text(call("LEFT", vec![t("abc")]), "a");
    text(call("LEFT", vec![t("abc"), n(2.0)]), "ab");
    text(call("RIGHT", vec![t("abc"), n(2.0)]), "bc");
    text(call("LEFT", vec![b(true), n(2.0)]), "TR");
    error(call("LEFT", vec![t("abc"), n(-1.0)]), Value);
    text(call("MID", vec![t("abcdef"), n(2.0), n(3.0)]), "bcd");
    text(call("MID", vec![t("abc"), n(5.0), n(3.0)]), "");
    error(call("MID", vec![t("abc"), n(0.0), n(1.0)]), Value);
    text(call("LEFTB", vec![t("abc"), n(2.0)]), "ab");
    text(call("RIGHTB", vec![t("abc"), n(2.0)]), "bc");
    text(call("MIDB", vec![t("abc"), n(2.0), n(1.0)]), "b");
    error(call("LEN", vec![e(Div0)]), Div0);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn casing_trim_clean_rept_exact() {
    text(call("UPPER", vec![t("straße")]), "STRAßE");
    text(call("LOWER", vec![t("ABC")]), "abc");
    // Excel: LOWER("ΑΣ") ends in a final sigma.
    text(call("LOWER", vec![t("ΑΣ")]), "ας");
    text(call("PROPER", vec![t("ab cd-ef")]), "Ab Cd-Ef");
    text(call("TRIM", vec![t("  a   b  ")]), "a b");
    // Excel: CLEAN removes 129 as well as the controls below 32.
    text(call("CLEAN", vec![t("\u{0081}a\u{0009}b")]), "ab");
    text(call("REPT", vec![t("ab"), n(3.0)]), "ababab");
    error(call("REPT", vec![t("ab"), n(-1.0)]), Value);
    error(call("REPT", vec![t("ab"), n(20000.0)]), Value);
    let exact_result = call("EXACT", vec![t("a"), t("A")]).unwrap();
    assert_eq!(exact_result.as_logical(), Some(false));
    let same = call("EXACT", vec![n(1.0), t("1")]).unwrap();
    assert_eq!(same.as_logical(), Some(true));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn find_search_replace_substitute() {
    exact(call("FIND", vec![t("y"), t("xyz")]), 2.0);
    error(call("FIND", vec![t("Y"), t("xyz")]), Value);
    exact(call("FIND", vec![t(""), t("xyz"), n(2.0)]), 2.0);
    exact(call("SEARCH", vec![t("Y"), t("xyz")]), 2.0);
    // Excel: SEARCH folds case beyond ASCII.
    exact(call("SEARCH", vec![t("é"), t("XÉ")]), 2.0);
    exact(call("SEARCH", vec![t("σ"), t("ΑΣ")]), 2.0);
    exact(call("SEARCH", vec![t("b?d"), t("abcde")]), 2.0);
    exact(call("SEARCH", vec![t("~?"), t("a?b")]), 2.0);
    error(call("SEARCH", vec![t("q"), t("abc")]), Value);
    exact(call("FINDB", vec![t("c"), t("abc")]), 3.0);
    exact(call("SEARCHB", vec![t("C"), t("abc")]), 3.0);
    text(
        call("REPLACE", vec![t("abcdef"), n(2.0), n(3.0), t("X")]),
        "aXef",
    );
    text(
        call("REPLACEB", vec![t("abc"), n(2.0), n(1.0), t("x")]),
        "axc",
    );
    text(
        call("SUBSTITUTE", vec![t("a-b-c"), t("-"), t("+")]),
        "a+b+c",
    );
    text(
        call("SUBSTITUTE", vec![t("a-b-c"), t("-"), t("+"), n(2.0)]),
        "a-b+c",
    );
    error(
        call("SUBSTITUTE", vec![t("a-b-c"), t("-"), t("+"), n(0.0)]),
        Value,
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn joins_and_t() {
    text(
        call("CONCATENATE", vec![t("a"), n(1.0), b(false)]),
        "a1FALSE",
    );
    error(call("CONCATENATE", vec![t("a"), e(NA)]), NA);
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![t("x"), t("y")]);
    sheet.column(4, 1, vec![t("z")]);
    let range = sheet.area(1, 1, 4, 1);
    text(
        call_on(&mut sheet, "CONCAT", vec![range.clone(), t("!")]),
        "xyz!",
    );
    text(
        call_on(&mut sheet, "TEXTJOIN", vec![t("-"), b(true), range.clone()]),
        "x-y-z",
    );
    text(
        call_on(&mut sheet, "TEXTJOIN", vec![t("-"), b(false), range]),
        "x-y--z",
    );
    let long = "a".repeat(20000);
    error(call("CONCAT", vec![t(&long), t(&long)]), Calc);
    text(call("T", vec![t("x")]), "x");
    text(call("T", vec![n(1.0)]), "");
    error(call("T", vec![e(Div0)]), Div0);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ifs_and_switch() {
    exact(call("IFS", vec![b(false), n(1.0), b(true), n(2.0)]), 2.0);
    error(call("IFS", vec![b(false), n(1.0)]), NA);
    error(call("IFS", vec![t("x"), n(1.0)]), Value);
    error(call("IFS", vec![e(Div0), n(1.0)]), Div0);
    text(
        call("SWITCH", vec![n(2.0), n(1.0), t("a"), n(2.0), t("b")]),
        "b",
    );
    text(call("SWITCH", vec![n(3.0), n(1.0), t("a"), t("z")]), "z");
    error(call("SWITCH", vec![n(3.0), n(1.0), t("a")]), NA);
    text(
        call("SWITCH", vec![t("A"), t("a"), t("hit"), t("miss")]),
        "hit",
    );
    error(call("SWITCH", vec![e(Div0), n(1.0), t("a")]), Div0);
    // Excel 16.0.20527: a number whose fifteen-digit reading overflows matches every number,
    // from DBL_MAX less three units up; less four units does not; text does not match.
    let max = f64::MAX;
    let three_below = f64::from_bits(max.to_bits() - 3);
    let four_below = f64::from_bits(max.to_bits() - 4);
    text(
        call("SWITCH", vec![n(max), n(1.0), t("hit"), t("miss")]),
        "hit",
    );
    text(
        call("SWITCH", vec![n(-max), n(-1.0), t("hit"), t("miss")]),
        "hit",
    );
    text(
        call("SWITCH", vec![n(1.0), n(three_below), t("hit"), t("miss")]),
        "hit",
    );
    text(
        call("SWITCH", vec![n(four_below), n(1.0), t("hit"), t("miss")]),
        "miss",
    );
    text(
        call("SWITCH", vec![n(max), t("a"), t("hit"), t("miss")]),
        "miss",
    );
}

/// Folded in from dc-fwpz and cfunc-m1.2: digit text in a logical position is #VALUE! (Excel:
/// NOT("1") and IF("1",1,2) are #VALUE! in the committed captures).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn digit_text_is_no_logical() {
    error(call("NOT", vec![t("1")]), Value);
    error(call("NOT", vec![t("2020/01/02")]), Value);
    error(call("IFS", vec![t("1"), n(1.0)]), Value);
}

/// Folded in from dc-tt73: #BLOCKED! and #CONNECT! are errors like any other to the IS
/// functions, TYPE, N, ERROR.TYPE and IFERROR.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn blocked_and_connect_are_errors() {
    for code in [Blocked, Connect] {
        assert_eq!(
            call("ISERROR", vec![e(code)]).unwrap().as_logical(),
            Some(true)
        );
        assert_eq!(
            call("ISERR", vec![e(code)]).unwrap().as_logical(),
            Some(true)
        );
        assert_eq!(
            call("ISNA", vec![e(code)]).unwrap().as_logical(),
            Some(false)
        );
        assert_eq!(
            call("ISBLANK", vec![e(code)]).unwrap().as_logical(),
            Some(false)
        );
        exact(call("TYPE", vec![e(code)]), 16.0);
        error(call("N", vec![e(code)]), code);
    }
    exact(call("ERROR.TYPE", vec![e(Blocked)]), 11.0);
    exact(call("ERROR.TYPE", vec![e(Connect)]), 10.0);
}

/// Folded in from dc-uhdh: a blank cell's value given directly (IFNA over a blank cell gives it
/// as it is) is read as Excel reads it. Excel: COUNT(IFNA(Z7,0)) 1, COUNTA 1, AVERAGE(1,..) 0.5,
/// MIN(5,..) 0, MAX(-5,..) 0, PRODUCT(3,..) 0, SUM(3,..) 3, SUMPRODUCT(..) #VALUE!.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_direct_blank_in_the_sum_and_count_family() {
    let blank = calc_func::value::CalcValue::empty();
    exact(call("COUNT", vec![blank.clone()]), 1.0);
    exact(call("COUNTA", vec![blank.clone()]), 1.0);
    exact(call("AVERAGE", vec![n(1.0), blank.clone()]), 0.5);
    exact(call("MIN", vec![n(5.0), blank.clone()]), 0.0);
    exact(call("MAX", vec![n(-5.0), blank.clone()]), 0.0);
    exact(call("MAXA", vec![n(-1.0), blank.clone()]), 0.0);
    exact(call("MINA", vec![n(5.0), blank.clone()]), 0.0);
    exact(call("AVERAGEA", vec![n(1.0), blank.clone()]), 0.5);
    exact(call("PRODUCT", vec![n(3.0), blank.clone()]), 0.0);
    exact(call("SUM", vec![n(3.0), blank.clone()]), 3.0);
    exact(call("SUMSQ", vec![n(3.0), blank.clone()]), 9.0);
    error(call("SUMPRODUCT", vec![blank]), Value);
}

/// Excel 16.0.20527, en-ZA, fn1 LOWER/UPPER/PROPER Value2 readings and input audits,
/// preserved in OxFunc-Evidence/probes/2026-11-03-fn1.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn fn1_excel_case_mapping_controls() {
    for (input, expected) in [("ΑΣ", "ας"), ("ΑΣ́", "ασ́"), ("ΑΣ́Α", "ασ́α"), ("ΆΣ", "άς")]
    {
        text(call("LOWER", vec![t(input)]), expected);
    }
    text(call("UPPER", vec![t("𐐨")]), "𐐨");
    text(call("UPPER", vec![t("éßǆİςσΣﬀ")]), "ÉßǄİΣΣΣﬀ");
    text(call("PROPER", vec![t("άβ Σ́Σ")]), "ΆΒ Σ́Σ");
    text(call("PROPER", vec![t("ßstraße ǆemal")]), "ßstraße Ǆemal");
}

/// The fn1 wildcard readings count UTF-16 code units and compare simple Unicode case,
/// without accent removal or multi-character case expansions.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn fn1_excel_unicode_lookup_wildcards() {
    for (pattern, value, found) in [
        ("é*", "Élan", true),
        ("ασ*", "ΑΣ", true),
        ("ǆ*", "ǅemal", true),
        ("*SSE", "Straße", false),
        ("nai*", "naïve", false),
        ("e*", "é", false),
        ("?", "😀", false),
        ("??", "😀", true),
        ("*", "😀", true),
        ("*", "", true),
        ("Σ*", "ς", true),
        ("i*", "İ", false),
        ("a~*", "a*", true),
    ] {
        for (function, mode) in [("MATCH", 0.0), ("XMATCH", 2.0)] {
            let mut sheet = Sheet::new();
            sheet.column(1, 1, vec![t(value)]);
            let source = sheet.area(1, 1, 1, 1);
            let result = call_on(&mut sheet, function, vec![t(pattern), source, n(mode)]);
            if found {
                exact(result, 1.0);
            } else {
                error(result, NA);
            }
        }
    }
    for (function, mode) in [("MATCH", 0.0), ("XMATCH", 2.0)] {
        exact(
            call(
                function,
                vec![
                    t("?"),
                    arr_values(vec![vec![t("😀")], vec![t("a")], vec![t("ab")]]),
                    n(mode),
                ],
            ),
            2.0,
        );
    }
}

/// fn1 REGEXTEST raw readings. The sharp-s control forbids full case expansions.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn fn1_excel_regex_unicode_and_newline_controls() {
    for (input, pattern, mode, expected) in [
        ("Élan", "é", 1.0, true),
        ("Straße", "STRASSE", 1.0, false),
        ("ΑΣ", "ας", 1.0, true),
        ("😀", ".", 0.0, true),
        ("a\nb", "a.b", 0.0, false),
        ("中", "\\w", 0.0, true),
    ] {
        let result = call("REGEXTEST", vec![t(input), t(pattern), n(mode)]).unwrap();
        assert_eq!(
            result.as_logical(),
            Some(expected),
            "{input:?} / {pattern:?}"
        );
    }
}

/// Fn3 retains É in [é-é]. The other singleton ranges use the same simple-fold
/// equivalence as the retained fn3 long-s and Kelvin literal controls; they are consequences
/// of singleton range membership, not additional captured Excel questions.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn unicode_regex_singleton_ranges_use_the_measured_literal_equivalence() {
    for (input, pattern) in [("É", "[é-é]"), ("ſ", "[s-s]"), ("K", "[K-K]")] {
        let result = call("REGEXTEST", vec![t(input), t(pattern), n(1.0)]).unwrap();
        assert_eq!(result.as_logical(), Some(true), "{input:?} / {pattern:?}");
    }
    // These are admission limits, not predictions of an Excel value.
    for pattern in ["[\u{0001}-\u{FFFF}]", "[ω-α]", "[^ω-α]", "(a)"] {
        refused(call("REGEXTEST", vec![t("a"), t(pattern), n(1.0)]));
    }
}

/// Explicit instance_num changes the empty-text validation route in Excel's fn1 readings.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn fn1_excel_empty_text_with_explicit_instance() {
    for function in ["TEXTBEFORE", "TEXTAFTER"] {
        error(call(function, vec![t(""), t("-")]), NA);
        for instance in [1.0, -1.0, 2.0, -2.0, 0.0] {
            error(call(function, vec![t(""), t("-"), n(instance)]), Value);
        }
        text(call(function, vec![t(""), t(""), n(1.0)]), "");
        error(
            call(
                function,
                vec![t(""), t("-"), n(1.0), n(0.0), n(0.0), t("fallback")],
            ),
            Value,
        );
    }
}

/// Excel 16.0.20527 fn1 TEXT readings: locale tags choose names while the workbook's
/// separators remain in effect. The expected language strings are the raw Value2 answers.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn fn1_excel_text_locale_names() {
    use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
    let env = CalcEnvironment {
        profile: Some(format_profile(LocaleId::EnZa)),
        ..CalcEnvironment::dates(DateSystem::System1900)
    };
    for (code, expected) in [
        ("[$-407]mmmm dddd", "März Mittwoch"),
        ("[$-40C]mmmm dddd", "mars mercredi"),
        ("[$-411]mmmm dddd", "3月 水曜日"),
        ("[$-419]mmmm dddd", "Март среда"),
        ("[$-407]mmm", "Mrz"),
        ("[$-de-DE]mmmm", "März"),
        ("[$-411]ggge", "令和5"),
    ] {
        text(
            call_in(&env, None, None, "TEXT", vec![n(45000.0), t(code)]),
            expected,
        );
    }
}
