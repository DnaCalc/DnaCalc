//! F2 (dc-ip6): the text and logical functions ported from OxFunc, and the folded-in rules, called
//! through `calc_func::invoke` as the formula evaluator calls them. Expected values are Excel's
//! (16.0.20430 on this machine, Windows locale en-ZA, ANSI code page 1252, read ad hoc through COM
//! where a row says "Excel:"), Microsoft's documented results, or OxFunc's tests (OxFunc
//! 6fca3518, functions/).

mod f1_support;

use calc_func::value::WorksheetErrorCode::{Blocked, Calc, Connect, Div0, NA, Name, Null, Num, Ref, Value};
use f1_support::*;

/// Excel 20527 rounds this numeric text value above its 15-digit midpoint.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn numeric_text_rounds_above_midpoint_for_three_digit_magnitude() {
    let value = n(f64::from_bits(0xc089_0747_71ae_6a9e));
    text(call("REPLACEB", vec![t("x"), n(1.0), n(1.0), value.clone()]), "-800.90988479865");
    text(call("CONCAT", vec![value]), "-800.90988479865");
}

/// Excel 20527 rounds this two-digit magnitude above its decimal midpoint.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn numeric_text_rounds_above_midpoint_for_two_digit_magnitude() {
    let value = n(f64::from_bits(0xc04b_b79b_aa07_6a80));
    text(call("ENCODEURL", vec![value.clone()]), "-55.434437993642");
    text(call("CONCAT", vec![value]), "-55.434437993642");
}

/// Excel 20527 rounds above the midpoint after the extended-precision scaling.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn numeric_text_rounds_above_scaled_midpoint_for_one_digit_magnitude() {
    let value = n(f64::from_bits(0xbff9_267f_c512_f642));
    text(call("ENCODEURL", vec![value.clone()]), "-1.57189919454631");
    text(call("CONCAT", vec![value]), "-1.57189919454631");
}

/// Excel 20527 distinguishes one from two extended-precision units above the midpoint.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn numeric_text_rounds_after_two_scaled_midpoint_units() {
    text(
        call("TEXTJOIN", vec![t(""), b(true), n(f64::from_bits(0x3fe1_97a6_c3b0_0d57))]),
        "0.549762136652514",
    );
    text(call("CONCAT", vec![n(f64::from_bits(0x3fe0_bf10_37e6_835f))]), "0.523323163204718");
}

/// Excel 20527 applies the same tolerant start-position floor to all four surfaces.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn find_and_search_use_tolerant_start_positions() {
    for name in ["FIND", "FINDB", "SEARCH", "SEARCHB"] {
        exact(call(name, vec![t(""), t("abca"), n(1.0 - 1e-7)]), 1.0);
        error(call(name, vec![t(""), t("abca"), n(1.0 - 3e-7)]), Value);
        exact(call(name, vec![t(""), t("abca"), n(2.0 - 1e-7)]), 2.0);
        exact(call(name, vec![t("b"), t("abca"), n(3.0 - 3e-7)]), 2.0);
        error(call(name, vec![t("b"), t("abca"), n(3.0 - 1e-7)]), Value);
    }
    exact(call("FIND", vec![t(""), t("abca"), n(5.0)]), 5.0);
    error(call("SEARCH", vec![t(""), t("abca"), n(5.0)]), Value);
}

/// Excel 20527 applies the tolerant integer floor to CHAR, but not UNICHAR.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn char_uses_tolerant_floor_at_integer_boundaries() {
    text(call("CHAR", vec![n(f64::from_bits(1.0_f64.to_bits() - 1))]), "\u{1}");
    text(call("CHAR", vec![n(6.0 - 1e-7)]), "\u{6}");
    text(call("CHAR", vec![n(6.0 - 1e-6)]), "\u{5}");
    text(call("CHAR", vec![n(65.0 - 1e-7)]), "A");
    error(call("CHAR", vec![n(256.0 - 1e-7)]), Value);
    text(call("UNICHAR", vec![n(6.0 - 1e-7)]), "\u{5}");
    text(call("UNICHAR", vec![n(256.0 - 1e-7)]), "ÿ");
}

/// Excel 20527 distinguishes empty delimiter text, range blanks, and array blanks.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn textsplit_empty_delimiters_and_error_order() {
    use calc_func::value::CalcValue;
    for delimiter in [CalcValue::empty(), t("")] {
        error(call("TEXTSPLIT", vec![t("abc"), delimiter.clone()]), Value);
        error(call("TEXTSPLIT", vec![t("abc"), t(","), delimiter.clone()]), Value);
        error(call("TEXTSPLIT", vec![t("abc"), delimiter.clone(), e(NA)]), NA);
        error(call("TEXTSPLIT", vec![t("abc"), delimiter, t(";"), e(NA)]), NA);
    }
    error(call("TEXTSPLIT", vec![t("abc"), arr_values(vec![vec![t(""), t(",")]])]), Value);
    text(call("TEXTSPLIT", vec![t("abc"), arr_values(vec![vec![CalcValue::empty(), t(",")]])]), "abc");
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![CalcValue::empty(), t(",")]);
    let delimiter = sheet.area(1, 1, 2, 1);
    error(call_on(&mut sheet, "TEXTSPLIT", vec![t("abc"), delimiter]), Value);
}

/// Excel 20527 TEXTJOIN option controls and independent typed holdouts.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn textjoin_boolean_text_option() {
    let args = |option| vec![t("|"), option, t("a"), t(""), t("b")];
    text(call("TEXTJOIN", args(t("TrUe"))), "a|b");
    text(call("TEXTJOIN", args(t("FALSE"))), "a||b");
    error(call("TEXTJOIN", args(t("1"))), Value);
    error(call("TEXTJOIN", args(t(" TRUE "))), Value);
    error(call("TEXTJOIN", args(e(NA))), NA);
}

/// Excel 20527 instance-number controls and independent small-boundary holdouts.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn substitute_instance_number_boundaries() {
    let args = |instance| vec![t("aaaa"), t("a"), t("X"), instance];
    text(call("SUBSTITUTE", args(n(1.0 - 1e-8))), "Xaaa");
    text(call("SUBSTITUTE", args(n(2.0 - 1e-8))), "aXaa");
    error(call("SUBSTITUTE", args(n(1.0 - 1e-6))), Value);
    error(call("SUBSTITUTE", args(n(2_147_483_647.0))), Value);
}

/// Excel 20527's numeric cutoff is 20 binary64 steps below the signed 32-bit upper limit.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn substitute_upper_instance_boundary() {
    let args = |instance| vec![t("aaaa"), t("a"), t("X"), instance];
    let upper = 2_147_483_647.0_f64.to_bits();
    text(call("SUBSTITUTE", args(n(f64::from_bits(upper - 21)))), "aaaa");
    for offset in [20, 4, 2, 0] {
        error(call("SUBSTITUTE", args(n(f64::from_bits(upper - offset)))), Value);
    }
    text(call("SUBSTITUTE", args(t("2147483646.9999999"))), "aaaa");
    error(call("SUBSTITUTE", args(t("2147483647"))), Value);
}

/// Excel 20527 start-position controls around two integer boundaries.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn replace_start_uses_tolerant_floor() {
    for name in ["REPLACE", "REPLACEB"] {
        text(call(name, vec![t("abcdef"), n(f64::from_bits(1.0_f64.to_bits() - 1)), n(1.0), t("X")]), "Xbcdef");
        text(call(name, vec![t("abcdef"), n(2.0 - 1e-8), n(1.0), t("X")]), "aXcdef");
        error(call(name, vec![t("abcdef"), n(1.0 - 1e-6), n(1.0), t("X")]), Value);
        error(call(name, vec![t("abcdef"), n(1.0), n(-0.5), t("X")]), Value);
        text(call(name, vec![t("abcdef"), n(1.0), n(-1e-12), t("X")]), "Xabcdef");
        text(call(name, vec![t("abcdef"), n(1.0), n(32767.0), t("X")]), "X");
        error(call(name, vec![t("abcdef"), n(1.0), n(32768.0), t("X")]), Value);
        error(call(name, vec![t("abcdef"), n(0.0), e(NA), t("X")]), NA);
        error(call(name, vec![t("abcdef"), n(1.0), n(-0.5), e(Ref)]), Ref);
        error(call(name, vec![t("abcdef"), t("bad"), e(NA), t("X")]), Value);
        text(call(name, vec![t("abcdef"), n(32767.0), n(1.0), t("X")]), "abcdefX");
        error(call(name, vec![t("abcdef"), n(32768.0), n(1.0), t("X")]), Value);
        error(call(name, vec![t("abcdef"), n(32768.0), e(NA), t("X")]), NA);
    }
}

/// Excel propagates a formed worksheet error before converting character widths.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn width_conversion_propagates_errors_without_a_profile() {
    for name in ["ASC", "DBCS"] {
        for code in [Null, Div0, Value, Ref, Name, Num, NA] {
            error(call(name, vec![e(code)]), code);
        }
        text(call(name, vec![t("")]), "");
        text(call(name, vec![calc_func::value::CalcValue::empty()]), "");
        refused(call(name, vec![t("abc")]));
    }
}

/// Excel 20527: a decimal separator is sufficient when FIXED needs no grouping.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn fixed_uses_recorded_decimal_without_guessing_grouping() {
    use calc_func::value::NumericMode;
    use calc_func::{EvalServices, FunctionCallScratch, Notation, catalogue, invoke};
    let target = catalogue().resolve_surface(Notation::Call, "FIXED").unwrap();
    for (value, places, no_commas, expected) in [
        (-0.08094972, 2.0, false, "-0.08"),
        (1234.567, 2.0, true, "1234.57"),
        (0.75, 1.0, false, "0.8"),
        (0.0, 2.0, false, "0.00"),
        (-913.2312911517463, 2.0, true, "-913.23"),
        (-775.0742253257873, 1.0, true, "-775.1"),
        (-497.9144014035945, 2.0, true, "-497.91"),
        (126.60956256671375, 5.0, true, "126.60956"),
        (532.5360989077642, 5.0, true, "532.53610"),
        (112.94462000278281, -1.0, true, "110"),
    ] {
        let mut services =
            EvalServices::new(&NumericMode::Fast).with_number_separators(".", "\u{a0}");
        text(
            invoke(
                target,
                &mut Given(vec![n(value), n(places), b(no_commas)]),
                &mut services,
                &mut FunctionCallScratch::new(),
            ),
            expected,
        );
    }
    let mut partial = EvalServices::new(&NumericMode::Fast).with_number_separators(".", "\u{a0}");
    refused(invoke(
        target,
        &mut Given(vec![n(1234.567), n(2.0), b(false)]),
        &mut partial,
        &mut FunctionCallScratch::new(),
    ));
    let mut unknown = EvalServices::new(&NumericMode::Fast);
    refused(invoke(
        target,
        &mut Given(vec![n(12.0), n(2.0), b(true)]),
        &mut unknown,
        &mut FunctionCallScratch::new(),
    ));
}

/// Excel 20527 uses the tolerant integer boundary on the magnitude of places.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn fixed_and_dollar_use_tolerant_places_and_argument_order() {
    use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
    let env = CalcEnvironment {
        profile: Some(format_profile(LocaleId::CurrentExcelHost)),
        ..CalcEnvironment::dates(DateSystem::System1900)
    };
    let edge = 1.0_f64 - (2.0_f64.powi(-22) + 2.0_f64.powi(-33));
    let below = f64::from_bits(edge.to_bits() - 1);
    for (places, fixed, dollar) in [
        (edge, "12345.7", "R12 345.7"),
        (below, "12346", "R12 346"),
        (-edge, "12350", "R12 350"),
        (-below, "12346", "R12 346"),
    ] {
        text(call_in(&env, None, None, "FIXED", vec![n(12345.6789), n(places), b(true)]), fixed);
        text(call_in(&env, None, None, "DOLLAR", vec![n(12345.6789), n(places)]), dollar);
    }
    error(call_in(&env, None, None, "FIXED", vec![n(12.0), n(128.0), e(Div0)]), Div0);
    error(call_in(&env, None, None, "FIXED", vec![n(12.0), n(127.9999999), b(true)]), Value);
    error(call_in(&env, None, None, "DOLLAR", vec![n(12.0), n(127.9999999)]), Value);
}

/// Excel 20527 rejects comparison-prefixed numbers as numeric arguments.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn comparison_text_is_not_numeric_under_recorded_separators() {
    use calc_func::value::NumericMode;
    use calc_func::{EvalServices, FunctionCallScratch, Notation, catalogue, invoke};
    for name in ["T.INV", "GAMMA.INV", "F.INV", "BETA.INV", "VALUE"] {
        let target = catalogue().resolve_surface(Notation::Call, name).unwrap();
        for input in [
            ">1", "<=0", "<1", ">=1", "<>1", "=1", "  >1  ",
            ">1e3", "<=-0.5", ">+1", " < 3.5 ", "==1", "<> 123",
        ] {
            let args = match name {
                "VALUE" => vec![t(input)],
                "T.INV" => vec![n(0.5), t(input)],
                _ => vec![n(0.5), n(2.0), t(input)],
            };
            let mut services =
                EvalServices::new(&NumericMode::Fast).with_number_separators(".", "\u{a0}");
            error(
                invoke(target, &mut Given(args), &mut services, &mut FunctionCallScratch::new()),
                Value,
            );
        }
    }
    refused(call("VALUE", vec![t(">1")]));
}

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

/// dc-zsn.2: the unassigned units of the Hangul jamo blocks (U+A97D-U+A97F, U+D7C7-U+D7CA,
/// U+D7FC-U+D7FF) are ignorable, as Excel 16.0.20527 compares them (3132 measured relations,
/// tests/fixtures/fn5_hangul_gaps). Seven rows put an ignorable unit before archaic jamo, which
/// the collation does not model (collation.rs); Excel gives them the relation it gives with U+00AD
/// in each such unit's place, and so must calc-func (one of them, an iteration mark after a
/// hyphen, stays refused either way).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn unassigned_hangul_block_units_are_ignorable() {
    use std::cmp::Ordering;
    let units = |s: &str| -> Vec<u16> {
        s.split(' ')
            .filter(|x| !x.is_empty())
            .map(|x| u16::from_str_radix(x, 16).unwrap())
            .collect()
    };
    let relation = |r: &str| match r {
        "<" => Ordering::Less,
        "=" => Ordering::Equal,
        ">" => Ordering::Greater,
        other => panic!("relation {other}"),
    };
    let gap = |c: u16| matches!(c, 0xA97D..=0xA97F | 0xD7C7..=0xD7CA | 0xD7FC..=0xD7FF);
    let soft =
        |t: &[u16]| -> Vec<u16> { t.iter().map(|&c| if gap(c) { 0x00AD } else { c }).collect() };
    let (mut exact, mut through_soft_hyphen) = (0, 0);
    for line in include_str!("fixtures/fn5_hangul_gaps/excel-relations.tsv").lines() {
        if line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        let (a, b) = (units(cols[0]), units(cols[1]));
        let excel = relation(cols[2]);
        let ours = calc_func::text_order(&a, &b);
        if let Some(with_soft_hyphen) = cols.get(3) {
            assert_eq!(relation(with_soft_hyphen), excel, "{line}");
            assert_eq!(ours, calc_func::text_order(&soft(&a), &soft(&b)), "{line}");
            through_soft_hyphen += 1;
        } else {
            assert_eq!(ours, Some(excel), "{line}");
            exact += 1;
        }
    }
    assert_eq!((exact, through_soft_hyphen), (3125, 7));
}

/// Excel 16.0.20527 under en-ZA, Touchstone environment 218a399794f8. The
/// ASCII pass can leave U+30A0 at the start of the weighted text.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn double_hyphen_without_a_preceding_weight() {
    use calc_func::value::NumericMode;
    use calc_func::{EvalServices, FunctionCallScratch, Notation, catalogue, invoke};
    use std::cmp::Ordering;
    use std::cmp::Ordering::{Equal, Greater, Less};
    let cases = [
        ("゠", "ー", Equal),
        ("゠", "々", Equal),
        ("゠", "ゝ", Equal),
        ("゠", "〻", Equal),
        ("゠", "ヽ", Equal),
        ("゠", "ヾ", Equal),
        ("゠", "ゞ", Equal),
        ("゠", "ｰ", Equal),
        ("゠", "a", Greater),
        ("゠", "é", Greater),
        ("゠", "α", Greater),
        ("゠", "か", Greater),
        ("゠", "中", Greater),
        ("゠", "가", Greater),
        ("゠", "😀", Greater),
        ("゠", "💥", Greater),
        ("゠", "Ａ", Greater),
        ("゠", "-", Greater),
        ("゠", "／", Greater),
        ("゠", "\u{00ad}", Greater),
        ("゠", "\u{0301}", Greater),
        ("゠", "\u{e000}", Greater),
        ("a゠", "aー", Equal),
        ("A゠", "a々", Equal),
        ("ab゠", "abゝ", Equal),
        ("a゠", "aa", Greater),
        ("x゠", "xb", Greater),
        ("゠a", "ーa", Equal),
        ("゠é", "々é", Equal),
        ("゠か", "ゝか", Equal),
        ("゠あ", "゠い", Less),
        ("゠a", "゠b", Less),
        ("゠", "ーa", Less),
        ("a゠b", "aーb", Equal),
        ("a゠b", "aーa", Greater),
        ("゠\u{00ad}", "ー", Equal),
        ("゠\u{0301}", "ー", Greater),
        ("゠-", "ー", Greater),
        ("゠゠", "ーー", Equal),
        ("゠゠", "ー゠", Equal),
        ("゠ー", "ーー", Equal),
        ("゠々", "ーー", Equal),
        ("゠か々", "ーか々", Equal),
        ("゠a\u{0301}", "ーá", Equal),
        ("ー", "゠", Equal),
        ("é", "゠", Less),
        ("-", "゠", Less),
        ("a々", "A゠", Equal),
        ("ーa", "゠a", Equal),
        ("゠い", "゠あ", Greater),
    ];
    for (operator, holds) in [
        ("=", Ordering::is_eq as fn(Ordering) -> bool),
        ("<", Ordering::is_lt),
    ] {
        let target = catalogue().resolve_surface(Notation::Infix, operator).unwrap();
        for (left, right, expected) in cases {
            let result = invoke(
                target,
                &mut Given(vec![t(left), t(right)]),
                &mut EvalServices::new(&NumericMode::Fast),
                &mut FunctionCallScratch::new(),
            );
            assert_eq!(
                result.unwrap(),
                b(holds(expected)),
                "{left:?} {operator} {right:?}"
            );
        }
    }
}

/// dc-zsn.4: month and weekday names under an explicit locale tag are Excel's for that tag's LCID
/// (de-AT writes Jänner, es-ES ene., zh-TW 一月), 6528 dates read from Excel 16.0.20527
/// (tests/fixtures/fn5_tagged_names, held-out formats included).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn tagged_date_names_follow_the_tag_lcid() {
    use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
    let env = CalcEnvironment {
        profile: Some(format_profile(LocaleId::EnZa)),
        ..CalcEnvironment::dates(DateSystem::System1900)
    };
    let mut dates = 0;
    for line in include_str!("fixtures/fn5_tagged_names/excel-text.tsv").lines() {
        if line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        let (format, serials, answers) = (cols[0], cols[1].split(','), cols[2].split('|'));
        for (serial, expected) in serials.zip(answers) {
            let serial: f64 = serial.parse().unwrap();
            text(call_in(&env, None, None, "TEXT", vec![n(serial), t(format)]), expected);
            dates += 1;
        }
    }
    assert_eq!(dates, 2542 + 3444);
}

/// dc-zsn.4: under every LCID tag (primary languages 0x01 to 0xFF, sublanguages 1 to 16) and the BCP 47
/// tags Excel names, the month and weekday names and the AM/PM designators are Excel's; an LCID Excel does
/// not name writes the host's, as LCID 0 does (tests/fixtures/fn5_tagged_names/excel-lcids.tsv).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn tagged_names_for_every_lcid_are_excels() {
    use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
    let env = CalcEnvironment {
        profile: Some(format_profile(LocaleId::EnZa)),
        ..CalcEnvironment::dates(DateSystem::System1900)
    };
    // 1 January 2023 is serial 44927, a Sunday.
    let check = |tag: &str, lists: &[&str]| {
        for (serial, expected) in [0.25, 0.75].iter().zip(lists[4].split('|')) {
            let format = format!("[$-{tag}]AM/PM");
            text(call_in(&env, None, None, "TEXT", vec![n(*serial), t(&format)]), expected);
        }
        for (field, (code, names)) in ["mmmm", "mmm", "dddd", "ddd"].iter().zip(lists).enumerate() {
            for (i, expected) in names.split('|').enumerate() {
                let serial = if field < 2 {
                    [44927, 44958, 44986, 45017, 45047, 45078, 45108, 45139, 45170, 45200, 45231, 45261][i]
                } else {
                    44927 + i as i32
                };
                let format = format!("[$-{tag}]{code}");
                text(
                    call_in(&env, None, None, "TEXT", vec![n(f64::from(serial)), t(&format)]),
                    expected,
                );
            }
        }
    };
    let (mut fallback, mut named) = (Vec::new(), 0);
    let mut host: Vec<&str> = Vec::new();
    for line in include_str!("fixtures/fn5_tagged_names/excel-lcids.tsv").lines() {
        if line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols[0] == "fallback" {
            fallback = cols[1].split(',').collect();
            continue;
        }
        if cols[0] == "0" {
            host = cols[1..].to_vec();
        }
        check(cols[0], &cols[1..]);
        named += 1;
    }
    for tag in &fallback {
        check(tag, &host);
    }
    assert_eq!((named, fallback.len()), (1 + 246 + 176, 3834));
}

/// dc-zsn.4: aaa and AAA write the short weekday name, four or more the long one; aa, a and mixed-case
/// runs are literal; a weekday run in a number format is not a number format. Excel 16.0.20527 (en-ZA host,
/// Touchstone sheet cases, env 145016f4397d, 2026-11-16).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn weekday_a_runs_and_tagged_designators() {
    use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
    let env = CalcEnvironment {
        profile: Some(format_profile(LocaleId::EnZa)),
        ..CalcEnvironment::dates(DateSystem::System1900)
    };
    let text_of = |x: f64, code: &str| call_in(&env, None, None, "TEXT", vec![n(x), t(code)]);
    for (x, code, expected) in [
        (44927.0, "aaa", "Sun"),
        (44927.0, "aaaa", "Sunday"),
        (44927.0, "AAA", "Sun"),
        (44927.0, "aaaaaaa", "Sunday"),
        (44927.0, "Aaa", "Aaa"),
        (44927.0, "aAaA", "aAaA"),
        (44927.0, "a", "a"),
        (44927.0, "aa d", "aa 1"),
        (44928.0, "aaa d", "Mon 2"),
        (1.5, "aaa", "Sun"),
        (0.0, "aaa", "Sat"),
        (0.75, "aaa h AM/PM", "Sat 6 pm"),
        (0.75, "h aaa", "18 Sat"),
        (44927.0, "\"aaa\"", "aaa"),
        (44927.0, "\\aaaa", "aSun"),
        (44927.0, "aaam", "Sun1"),
        (44927.0, "[$-407]aaa", "So"),
        (44927.0, "[$-411]aaa", "日"),
        (44927.0, "[$-404]aaaa", "星期日"),
        (44927.0, "aaa;@", "Sun"),
        (0.75, "h am/pm", "6 pm"),
        (0.75, "h Am/Pm", "6 pm"),
        (0.75, "[$-409]h am/pm", "6 PM"),
        (0.75, "[$-407]hh:mm AM/PM", "06:00 PM"),
        (0.25, "[$-407]h:mm:ss AM/PM", "6:00:00 AM"),
        (0.75, "[$-411]AM/PM h", "午後 6"),
        (44927.75, "[$-411]aaa h AM/PM", "日 6 午後"),
        (0.5, "[$-412]h AM/PM", "12 오후"),
        (0.0, "[$-412]h AM/PM", "12 오전"),
        (0.75, "[$-9999]h AM/PM", "6 pm"),
        (0.75, "[$-de-AT]h AM/PM", "6 PM"),
    ] {
        text(text_of(x, code), expected);
    }
    error(text_of(-1.0, "aaa"), Value);
    error(text_of(44927.0, "0 aaa"), Value);
}

/// Excel 16.0.20527, en-ZA, Touchstone environment 218a399794f8.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn text_distinguishes_date_names_and_numeric_placeholders() {
    use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
    let env = CalcEnvironment {
        profile: Some(format_profile(LocaleId::CurrentExcelHost)),
        compatibility_version: 2,
        ..CalcEnvironment::dates(DateSystem::System1900)
    };
    let cases = [
        ("0 aaa", None),
        ("aaa 0", None),
        ("0 AAAA", None),
        ("# aaa", None),
        ("? aaa", None),
        ("[$-407]0 aaa", None),
        ("[$-411]aaaa 0", None),
        ("0 mmm", None),
        ("0 \"aaa\"", Some("45aaa")),
        ("0 Aaa", Some("45Aaa")),
        ("aaa \"0\"", Some("Sun 0")),
        ("[$-411]aaa ss.00", Some("日 00.00")),
        ("[$-407]0;aaa", Some("44928")),
        ("aaa_0", Some("Sun ")),
        ("aaa*0", Some("Sun")),
        ("aaa\\0", Some("Sun0")),
        ("0;0 aaa", None),
        ("0;aaa 0", None),
        ("aaa \"?\"", Some("Sun ?")),
        ("ddd 0", None),
        ("[$-407]mmmm #", None),
        ("hh:mm.0", Some("18:00.0")),
        ("aaa 1", None),
        ("aaa 0E+00", None),
        ("0;0;0;0 aaa", None),
        ("0;0;0;aaa \"0\"", None),
        ("aaa \"1\"", Some("Sun 1")),
        ("aaa 2024", None),
        ("aaa 0.00", None),
        ("aaa m.0", Some("Sun 0.0")),
        ("aaa ss.01", None),
        ("aaa ss.0", Some("Sun 00.0")),
        ("0 aaa;0", None),
        ("[$-407]mmm 1", None),
        ("ddd 1", None),
    ];
    let mut mismatches = Vec::new();
    for (code, expected) in cases {
        let expected = expected.map_or_else(|| e(Value), t);
        match call_in(&env, None, None, "TEXT", vec![n(44927.75), t(code)]) {
            Ok(got) if got == expected => {}
            got => mismatches.push(format!("{code}: expected {expected:?}, got {got:?}")),
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} of {} mismatched:\n{}",
        mismatches.len(),
        cases.len(),
        mismatches.join("\n")
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn char_code_unichar_unicode() {
    assert_excel_capture(
        call("CHAR", vec![n(65.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        252,
        1,
    );
    // Excel: CHAR(128) is the euro sign, CHAR(150) an en dash (code page 1252).
    text(call("CHAR", vec![n(128.0)]), "\u{20AC}");
    text(call("CHAR", vec![n(150.0)]), "\u{2013}");
    text(call("CHAR", vec![n(129.0)]), "\u{0081}");
    text(call("CHAR", vec![n(255.9)]), "\u{00FF}");
    assert_excel_capture(
        call("CHAR", vec![n(0.0)]),
        include_bytes!("captures/jnt/sections/jnt_text_formats.capture.json"),
        "jnt_text_formats",
        120,
        4,
    );
    assert_excel_capture(
        call("CHAR", vec![n(256.0)]),
        include_bytes!("captures/jnt/sections/jnt_text_formats.capture.json"),
        "jnt_text_formats",
        121,
        4,
    );
    // Excel: CODE(UNICHAR(8364)) is 128, CODE(UNICHAR(1000)) 63, CODE(UNICHAR(233)) 233.
    exact(call("CODE", vec![t("\u{20AC}")]), 128.0);
    exact(call("CODE", vec![t("\u{03E8}")]), 63.0);
    exact(call("CODE", vec![t("\u{00E9}")]), 233.0);
    exact(call("CODE", vec![t("\u{1F600}")]), 63.0);
    assert_excel_capture(
        call("CODE", vec![t("")]),
        include_bytes!("captures/catalogue/sections/f2_rows.capture.json"),
        "f2_rows",
        4,
        4,
    );
    assert_excel_capture(
        call("UNICHAR", vec![n(9731.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        333,
        1,
    );
    assert_excel_capture(
        call("UNICHAR", vec![n(0.0)]),
        include_bytes!("captures/jnt/sections/jnt_text_formats.capture.json"),
        "jnt_text_formats",
        182,
        4,
    );
    error(call("UNICHAR", vec![n(55296.0)]), NA);
    exact(call("UNICODE", vec![t("\u{20AC}")]), 8364.0);
    exact(call("UNICODE", vec![t("\u{1F600}x")]), 128512.0);
    assert_excel_capture(
        call("UNICODE", vec![t("")]),
        include_bytes!("captures/jnt/sections/jnt_text_formats.capture.json"),
        "jnt_text_formats",
        185,
        4,
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn slices_and_lengths() {
    assert_excel_capture(
        call("LEN", vec![t("abc")]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        294,
        1,
    );
    exact(call("LEN", vec![n(12.5)]), 4.0);
    // Excel 16.0.20527 (dc-fl2, the catalogue's sheet dcfl2_text): LEN counts UTF-16 code units,
    // so a surrogate pair is 2.
    exact(call("LEN", vec![t("\u{1F600}")]), 2.0);
    exact(call("LENB", vec![t("\u{1F600}")]), 2.0);
    text(call("LEFT", vec![t("abc")]), "a");
    assert_excel_capture(
        call("LEFT", vec![t("abc"), n(2.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        292,
        1,
    );
    assert_excel_capture(
        call("RIGHT", vec![t("abc"), n(2.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        313,
        1,
    );
    text(call("LEFT", vec![b(true), n(2.0)]), "TR");
    error(call("LEFT", vec![t("abc"), n(-1.0)]), Value);
    assert_excel_capture(
        call("MID", vec![t("abcdef"), n(2.0), n(3.0)]),
        include_bytes!("captures/jnt/sections/jnt_text_formats.capture.json"),
        "jnt_text_formats",
        138,
        4,
    );
    text(call("MID", vec![t("abc"), n(5.0), n(3.0)]), "");
    error(call("MID", vec![t("abc"), n(0.0), n(1.0)]), Value);
    assert_excel_capture(
        call("LEFTB", vec![t("abc"), n(2.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        293,
        1,
    );
    assert_excel_capture(
        call("RIGHTB", vec![t("abc"), n(2.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        314,
        1,
    );
    assert_excel_capture(
        call("MIDB", vec![t("abc"), n(2.0), n(1.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        298,
        1,
    );
    error(call("LEN", vec![e(Div0)]), Div0);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn casing_trim_clean_rept_exact() {
    text(call("UPPER", vec![t("straße")]), "STRAßE");
    assert_excel_capture(
        call("LOWER", vec![t("ABC")]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        296,
        1,
    );
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
    assert_excel_capture(
        call("FIND", vec![t("y"), t("xyz")]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        282,
        1,
    );
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
    assert_excel_capture(
        call("REPLACEB", vec![t("abc"), n(2.0), n(1.0), t("x")]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        311,
        1,
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
    assert_excel_capture(
        call("T", vec![t("x")]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        322,
        1,
    );
    assert_excel_capture(
        call("T", vec![n(1.0)]),
        include_bytes!("captures/catalogue/sections/f2_rows.capture.json"),
        "f2_rows",
        32,
        4,
    );
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
    assert_excel_capture(
        call("NOT", vec![t("1")]),
        include_bytes!("captures/jnt/sections/jnt_w111_text_dates.capture.json"),
        "jnt_w111_text_dates",
        4988,
        4,
    );
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
    for pattern in ["[\u{0001}-\u{FFFF}]", "[ω-α]", "[^ω-α]", "(a|b)"] {
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

/// Excel 20527 reports the older Japanese era names under LCID and language tags.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn tagged_japanese_eras_before_heisei() {
    use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
    let env = CalcEnvironment {
        profile: Some(format_profile(LocaleId::EnZa)),
        ..CalcEnvironment::dates(DateSystem::System1900)
    };
    for (serial, code, expected) in [
        (0.0, "ggge/mm/dd", "明治33/01/00"),
        (60.0, "ggge/mm/dd", "明治33/02/29"),
        (4594.0, "g e", "M 45"),
        (4594.0, "ggge", "明治45"),
        (4595.0, "gg ee", "大 01"),
        (9855.0, "ggge", "大正15"),
        (9856.0, "gg ee", "昭 01"),
        (32515.0, "ggge", "昭和64"),
        (32516.0, "ggge", "平成1"),
    ] {
        for tag in ["411", "ja-JP"] {
            let format = format!("[$-{tag}]{code}");
            text(
                call_in(&env, None, None, "TEXT", vec![n(serial), t(&format)]),
                expected,
            );
        }
    }
}
