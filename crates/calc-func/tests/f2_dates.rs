//! F2 (dc-ip6): the date and time functions, the number-format functions (TEXT, VALUE, FIXED,
//! DOLLAR, NUMBERVALUE), the host-fact functions, ADDRESS, the database functions and the other
//! text functions, called through `calc_func::invoke` in a calculation environment. Expected
//! values are Excel's (16.0.20430 on this machine, Windows locale en-ZA with `.` and a space as
//! separators; "Excel:" rows read ad hoc through COM), Microsoft's documented examples, or
//! OxFunc's tests (OxFunc 6fca3518, functions/).

mod f1_support;

use calc_func::HostFacts;
use calc_func::locale::{CalcEnvironment, DateSystem, FormatProfile};
use calc_func::value::{CalcValue, CellKey, SheetId, WorksheetErrorCode::*};
use f1_support::*;

/// Excel 20527 format-flag boundary and independent controls for both text renderers.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn value_and_array_to_text_use_tolerant_format_flag() {
    for name in ["VALUETOTEXT", "ARRAYTOTEXT"] {
        error(call(name, vec![t("abc"), n(-0.5)]), Value);
        text(call(name, vec![t("abc"), n(-1e-12)]), "abc");
        error(call(name, vec![t("abc"), n(2.0 - 1e-8)]), Value);
    }
    text(call("VALUETOTEXT", vec![t("abc"), n(1.0 - 1e-8)]), "\"abc\"");
    text(call("ARRAYTOTEXT", vec![t("abc"), n(1.0 - 1e-8)]), "{\"abc\"}");
    text(call("VALUETOTEXT", vec![e(Ref), n(0.0)]), "#REF!");
    error(call("ARRAYTOTEXT", vec![e(Ref), n(-0.5)]), Ref);
}

/// This machine's settings: en-ZA with `.` and a space, the 1900 date system, and a clock in
/// 2026 (VALUE("1/2") read 2 January 2026).
fn host() -> CalcEnvironment {
    CalcEnvironment {
        compatibility_version: 1,
        date_system: DateSystem::System1900,
        profile: Some(
            FormatProfile::for_culture("en-ZA")
                .unwrap()
                .with_separators(".", " ")
                .unwrap(),
        ),
        now: Some(46318.6),
    }
}

fn c(
    env: &CalcEnvironment,
    spelling: &str,
    args: Vec<CalcValue>,
) -> Result<CalcValue, calc_func::FunctionFault> {
    call_in(env, None, None, spelling, args)
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dates_without_an_environment_are_refused() {
    refused(call("DATE", vec![n(2024.0), n(1.0), n(15.0)]));
    refused(call("YEAR", vec![n(45000.0)]));
    refused(call("TEXT", vec![n(1.0), t("0.00")]));
}

/// Excel 16.0.20625 rejects out-of-range dates and rounds serials just below a whole day.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn days360_validates_and_rounds_serials() {
    let env = host();
    for invalid in [f64::MAX, 2_958_466.0, 2_958_466.0 - 0.000_005, -f64::EPSILON] {
        for method in [false, true] {
            error(c(&env, "DAYS360", vec![n(invalid), n(1.0), b(method)]), Num);
            error(c(&env, "DAYS360", vec![n(1.0), n(invalid), b(method)]), Num);
        }
    }
    let before = |day: f64| n(f64::from_bits(day.to_bits() - 1));
    exact(c(&env, "DAYS360", vec![n(45351.0), before(45716.0)]), 358.0);
    exact(c(&env, "DAYS360", vec![before(60.0), n(0.0)]), -60.0);
    exact(c(&env, "DAYS360", vec![n(0.0), before(1.0), b(true)]), 1.0);
    exact(c(&env, "DAYS360", vec![n(0.0), n(1.0 - 0.000_006)]), 0.0);
    exact(c(&env, "DAYS360", vec![n(45351.0), n(45381.0), t("TRUE")]), 31.0);
    error(c(&env, "DAYS360", vec![n(45351.0), n(45381.0), t("0.1")]), Value);
}

/// Excel 16.0.20625 checks DATEDIF's calendar serials before doing even a day count.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn datedif_checks_calendar_serials() {
    let env = host();
    for unit in ["d", "m"] {
        error(c(&env, "DATEDIF", vec![n(0.0), n(f64::MAX), t(unit)]), Num);
        error(c(&env, "DATEDIF", vec![n(-f64::EPSILON), n(1.0), t(unit)]), Num);
    }
    let before_march = f64::from_bits(61.0_f64.to_bits() - 1);
    exact(c(&env, "DATEDIF", vec![n(1.0), n(before_march), t("d")]), 59.0);
    exact(c(&env, "DATEDIF", vec![n(1.0), n(before_march), t("m")]), 2.0);
    exact(c(&env, "DATEDIF", vec![n(60.0), n(61.0), t("md")]), 1.0);
    exact(c(&env, "DATEDIF", vec![n(60.0), n(61.0), t("yd")]), 1.0);
    error(c(&env, "DATEDIF", vec![n(1.0), n(2.0), t(" d ")]), Num);
    for (start, end, days) in [
        (43890.0, 44256.0, 0.0),
        (43890.0, 44561.0, 306.0),
        (43702.0, 45133.0, 336.0),
        (43521.0, 45579.0, 231.0),
    ] {
        exact(c(&env, "DATEDIF", vec![n(start), n(end), t("yd")]), days);
    }
}

/// Excel 16.0.20625 admits equal truncated dates even when fractional rounding reverses them.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn datedif_uses_excel_leap_rules_when_rounding_reverses_a_day() {
    let env = host();
    for (unit, expected) in [
        ("d", 0.0),
        ("m", -1.0),
        ("y", -1.0),
        ("md", 30.0),
        ("yd", 365.0),
        ("ym", 11.0),
    ] {
        exact(c(&env, "DATEDIF", vec![n(1.999999), n(1.5), t(unit)]), expected);
    }
    for (calendar_day, expected) in [
        (62.0_f64, 365.0), // March 1900 includes Excel's fictitious leap day.
        (45000.0, 364.0), // March 2023 uses the start year's calendar.
        (45261.0, 365.0), // December 2023 uses the following leap year.
        (45366.0, 365.0), // March 2024 uses the start year's calendar.
        (72776.0, 365.0), // April 2099 treats 2100 as a leap year for this remainder.
        (73141.0, 364.0), // April 2100 uses the following ordinary year.
    ] {
        let before = f64::from_bits(calendar_day.to_bits() - 1);
        exact(c(&env, "DATEDIF", vec![n(before), n(calendar_day - 0.5), t("yd")]), expected);
    }
}

/// Excel 16.0.20625 treats 28 February 1900 as the month end for YEARFRAC basis 0.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn yearfrac_us_day_count_uses_the_gregorian_february_end() {
    let env = host();
    error(c(&env, "YEARFRAC", vec![n(7909.0), n(12681.0), n(-0.25)]), Num);
    exact(c(&env, "YEARFRAC", vec![n(59.0), n(60.0)]), -1.0 / 360.0);
    exact(c(&env, "YEARFRAC", vec![n(59.0), n(61.0)]), 1.0 / 360.0);
    exact(c(&env, "YEARFRAC", vec![n(89.0), n(60.0)]), 30.0 / 360.0);
    exact(c(&env, "YEARFRAC", vec![n(44985.0), n(45016.0)]), 31.0 / 360.0);
    error(c(&env, "YEARFRAC", vec![n(-1.0), n(45100.0), b(false)]), Value);
    error(c(&env, "YEARFRAC", vec![e(NA), n(45100.0), n(-1.0)]), Num);
    error(c(&env, "YEARFRAC", vec![n(-1.0), n(45100.0), e(Ref)]), Ref);
}

/// Excel 16.0.20625 rejects negative fractional holidays before truncating them.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn business_days_check_holiday_serials_before_truncation() {
    let env = host();
    for (name, second) in [
        ("NETWORKDAYS", 45001.0),
        ("NETWORKDAYS.INTL", 45001.0),
        ("WORKDAY", 2.0),
        ("WORKDAY.INTL", 2.0),
    ] {
        for holiday in [n(-0.25), arr(vec![vec![45001.0], vec![-0.25]])] {
            let mut args = vec![n(45000.0), n(second)];
            if name.ends_with(".INTL") {
                args.push(n(1.0));
            }
            args.push(holiday);
            error(c(&env, name, args), Num);
        }
    }
}

/// Excel 16.0.20625 reads a weekend string as a seven-day mask and a blank cell as code zero.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn intl_business_days_distinguish_weekend_argument_kinds() {
    let env = host();
    for (name, second, expected) in [
        ("NETWORKDAYS.INTL", 45001.0, 2.0),
        ("WORKDAY.INTL", 2.0, 45002.0),
    ] {
        let with_weekend = |kind| c(&env, name, vec![n(45000.0), n(second), kind]);
        error(with_weekend(t("1")), Value);
        error(with_weekend(CalcValue::empty()), Num);
        exact(with_weekend(missing()), expected);
        exact(with_weekend(n(1.0)), expected);
    }
}

/// Excel 20527: an invalid year cannot wrap to 1900 or be rescued from 10000 by a negative month.
/// Negative years within the ordinary range still normalize with their month before the serial check.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn date_years_do_not_wrap_or_clip_before_month_normalization() {
    for system in [DateSystem::System1900, DateSystem::System1904] {
        let env = CalcEnvironment { date_system: system, ..host() };
        for (year, month, day) in [
            (10000.0, -1.0, -307.0),
            (10000.0, -12.0, 1.0),
            (65536.0, -12.0, 1.0),
            (1e100, -12.0, 1.0),
            (-65536.0, 1.0, 1.0),
            (-65537.000001, 32766.0, 1.0),
            (-2147481748.0, 1.0, 1.0),
            (-8589934592.000002, 1.0, 1.0),
            (-1e100, 1.0, 1.0),
        ] {
            error(c(&env, "DATE", vec![n(year), n(month), n(day)]), Num);
        }
    }
    exact(c(&host(), "DATE", vec![n(-1.0), n(13.0), n(1.0)]), 1.0);
    exact(c(&host(), "DATE", vec![n(9999.0), n(1.0), n(1.0)]), 2958101.0);
}

/// Excel 20527 distinguishes numeric text, empty text, blank cells, and date strings.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn numeric_and_empty_date_text_needs_no_locale_profile() {
    use calc_func::value::NumericMode;
    use calc_func::{EvalServices, FunctionCallScratch, Notation, catalogue, invoke};

    for name in ["DATEVALUE", "TIMEVALUE"] {
        let target = catalogue().resolve_surface(Notation::Call, name).unwrap();
        for value in ["", "  ", "TRUE", "May", "abc def", "-0.97918323", "1e2", "+123.45"] {
            let mode = NumericMode::Fast;
            let mut services = EvalServices::new(&mode).with_number_separators(".", "\u{a0}");
            error(
                invoke(target, &mut Given(vec![t(value)]), &mut services, &mut FunctionCallScratch::new()),
                Value,
            );
        }
        let mode = NumericMode::Fast;
        let mut services = EvalServices::new(&mode).with_number_separators(".", "\u{a0}");
        refused(invoke(
            target,
            &mut Given(vec![t("2024/01/05")]),
            &mut services,
            &mut FunctionCallScratch::new(),
        ));
    }
    assert_excel_capture(
        call("VALUE", vec![t("")]),
        include_bytes!("captures/catalogue/sections/f2_rows.capture.json"),
        "f2_rows",
        50,
        4,
    );
    error(call("VALUE", vec![t("hello")]), Value);
    exact(call("VALUE", vec![CalcValue::empty()]), 0.0);
    refused(call("DATEVALUE", vec![t("1.5")]));
}

/// Excel 20527: empty patterns match boundaries; a leading quantifier is invalid.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn empty_and_invalid_regex_patterns() {
    assert_eq!(call("REGEXTEST", vec![t("a😀b"), t("")]).unwrap().as_logical(), Some(true));
    text(call("REGEXEXTRACT", vec![t("ab"), t("")]), "");
    let result = call("REGEXEXTRACT", vec![t("ab"), t(""), n(1.0)]).unwrap();
    let array = result.as_array().unwrap();
    assert_eq!((array.shape().rows, array.shape().cols), (1, 3));
    for col in 0..3 {
        assert_eq!(array.get(0, col).unwrap().as_text().unwrap().to_string_lossy(), "");
    }
    for (occurrence, expected) in [(0.0, "XaXbX"), (2.0, "aXb"), (-1.0, "abX")] {
        text(call("REGEXREPLACE", vec![t("ab"), t(""), t("X"), n(occurrence)]), expected);
    }
    for pattern in ["*", "+abc", "?a"] {
        error(call("REGEXTEST", vec![t("abc"), t(pattern)]), Value);
        error(call("REGEXEXTRACT", vec![t("abc"), t(pattern)]), Value);
        error(call("REGEXREPLACE", vec![t("abc"), t(pattern), t("X")]), Value);
    }
    assert_eq!(call("REGEXTEST", vec![t("aaa"), t("a*")]).unwrap().as_logical(), Some(true));
    refused(call("REGEXTEST", vec![t("aaa"), t("a??")]));
}

/// Excel 20527 empty-pattern replacement positions at CRLF, UTF-16 and leading LF boundaries.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn empty_regex_replace_boundaries() {
    let cases = [
        ("a\r\nb", "X", 0.0, "XaX\rX\nbX"),
        ("\r\n", "X", -1.0, "\r\nX"),
        ("\r\n\r\n", "X", -2.0, "\r\n\rX\n"),
        ("\r\nA", "X", 3.0, "\r\nAX"),
        ("\r\nA", "X", 4.0, "\r\nA"),
        ("a😀b", "X", 1.0, "Xa😀b"),
        ("a😀b", "X", 2.0, "aX😀b"),
        ("\n😀", "X", 2.0, "\nX😀"),
        ("\n\tA0中中ñ0中 0中+", "X", 3.0, "\n\tAX0中中ñ0中 0中+"),
        ("\n中 bb\t中éb\n0\t中", "_", 3.0, "\n中 _bb\t中éb\n0\t中"),
        ("\nñ\n0中b0\nAAbé ", "X", 3.0, "\nñ\nX0中b0\nAAbé "),
        ("\n\n中 \n\t 中éñ0\n中", "_", 0.0, "_\n\n_中_ _\n_\t_ _中_é_ñ_0_\n_中_"),
        ("\n\t+bé中\tA中é 中\t", "X", 3.0, "\n\t+Xbé中\tA中é 中\t"),
        ("\naaaaaaaaaaa", "X", 2.0, "\nXaaaaaaaaaaa"),
        ("\naaaaaaaaaaaaa", "X", 2.0, "\nXaaaaaaaaaaaaa"),
    ];
    let replace = |input, replacement, occurrence| {
        call("REGEXREPLACE", vec![t(input), t(""), t(replacement), n(occurrence)])
    };
    for (input, replacement, occurrence, expected) in cases {
        text(replace(input, replacement, occurrence), expected);
    }
    for occurrence in [0.0, 3.0, 4.0, -1.0, -3.0] {
        error(replace("a😀b", "X", occurrence), Value);
    }
}

/// Excel 20527 checks REPT's sign before truncation and bounds empty-text counts separately.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn rept_count_sign_and_empty_text_limit() {
    for value in ["", "x", "😀"] {
        for count in [-1.0, -0.5, -1e-100, -f64::MIN_POSITIVE] {
            error(call("REPT", vec![t(value), n(count)]), Value);
        }
    }
    text(call("REPT", vec![t("x"), n(0.999)]), "");
    text(call("REPT", vec![t("x"), n(1.999)]), "x");
    for count in [32768.0, 45000.0, 2_147_483_647.5] {
        text(call("REPT", vec![t(""), n(count)]), "");
        text(call("REPT", vec![missing(), n(count)]), "");
    }
    error(call("REPT", vec![t(""), n(2_147_483_648.0)]), Value);
    error(call("REPT", vec![t("x"), n(32768.0)]), Value);
    error(call("REPT", vec![t("😀"), n(16384.0)]), Value);
}

/// Excel 20527: instance floor, delimiter bounds, empty delimiters and fallback coercion.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn text_delimiters_instance_and_fallback() {
    text(call("TEXTBEFORE", vec![t("a-b-c"), t("-"), n(-0.5)]), "a-b");
    text(call("TEXTAFTER", vec![t("a-b-c"), t("-"), n(-0.5)]), "c");
    let just_below_one = f64::from_bits(1.0_f64.to_bits() - 1);
    text(call("TEXTBEFORE", vec![t("a-b-c"), t("-"), n(just_below_one)]), "a");
    for name in ["TEXTBEFORE", "TEXTAFTER"] {
        error(call(name, vec![t("abc"), t("zz"), n(3.0)]), Value);
        error(call(name, vec![t("a"), t("zz"), n(1.0)]), Value);
        error(call(name, vec![t("a"), t("zz")]), NA);
        error(call(name, vec![t("abc"), t(""), n(0.0)]), Value);
        text(call(name, vec![t("abc"), t("Z"), n(1.0), n(0.0), n(0.0), n(0.5)]), "0.5");
        text(call(name, vec![t("abc"), t("Z"), n(1.0), n(0.0), n(0.0), b(true)]), "TRUE");
        error(call(name, vec![t("abc"), t("Z"), n(1.0), n(0.0), n(0.0), e(Div0)]), Div0);
    }
    text(call("TEXTBEFORE", vec![t("a"), t(""), n(100.0)]), "a");
    text(call("TEXTAFTER", vec![t("a"), t(""), n(100.0)]), "");
    text(call("TEXTBEFORE", vec![t("abc"), t(""), n(-100.0)]), "abc");
    text(call("TEXTAFTER", vec![t("abc"), t(""), n(100.0)]), "abc");
    text(call("TEXTAFTER", vec![t("abc"), t(""), n(1e100)]), "abc");
    text(call("TEXTBEFORE", vec![t("abc"), t(""), n(-1e100)]), "abc");
    text(call("TEXTBEFORE", vec![t("a-b"), t("-"), n(1.0), n(0.0), n(0.0), e(Div0)]), "a");
    error(call("TEXTAFTER", vec![t("abc"), t("Z"), n(1.0), n(2.0), e(Null)]), Null);
    error(call("TEXTBEFORE", vec![t("abc"), t("Z"), e(NA), n(2.0), e(Null)]), NA);
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![e(Div0)]);
    let delimiter = sheet.area(1, 1, 1, 1);
    error(
        call_on(&mut sheet, "TEXTAFTER", vec![t("abc"), delimiter, n(1.0), n(2.0), e(Null)]),
        Div0,
    );
}

/// Excel 16.0.20527, typed Value2 calls: argument errors win in order and a logical
/// format gives #VALUE! even for a text, logical or blank value.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn text_errors_do_not_require_a_locale() {
    error(call("TEXT", vec![e(NA), e(Div0)]), NA);
    error(call("TEXT", vec![n(1.0), e(Ref)]), Ref);
    for value in [n(123.0), t("abc"), b(true), missing()] {
        error(call("TEXT", vec![value, b(true)]), Value);
    }
    refused(call("TEXT", vec![n(1.0), t("0.00")]));
}

/// Excel 16.0.20527, typed Value2 calls with numeric format arguments.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn text_logical_with_numeric_format_needs_no_locale() {
    for code in [0.0, 1.0, -1.0, 0.5, 1e100] {
        text(call("TEXT", vec![b(true), n(code)]), "TRUE");
        text(call("TEXT", vec![b(false), n(code)]), "FALSE");
    }
    refused(call("TEXT", vec![b(true), t("@!")]));
}

/// Excel 16.0.20527, typed range cells: wildcard characters in range text are
/// literal, not numeric matches. Comparison operators at the start of range
/// text likewise do not form numeric text. Recorded separators suffice.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn countif_numeric_criterion_rejects_non_numeric_labels() {
    use calc_func::value::NumericMode;
    use calc_func::{EvalServices, FunctionCallScratch, Notation, catalogue, invoke};

    let mut sheet = Sheet::new();
    sheet.column(
        1,
        1,
        vec![t("a*0"), t("1?0"), t("1~0"), t("<=0"), t(">1"), t("0")],
    );
    let area = sheet.area(1, 1, 6, 1);
    let target = catalogue()
        .resolve_surface(Notation::Call, "COUNTIF")
        .unwrap();
    let mode = NumericMode::Fast;
    let mut services =
        EvalServices::with_cells(&mode, &mut sheet).with_number_separators(".", "\u{a0}");
    exact(
        invoke(
            target,
            &mut Given(vec![area, t("=0")]),
            &mut services,
            &mut FunctionCallScratch::new(),
        ),
        1.0,
    );

    // Excel 20527: under dot/NBSP separators these are text, including as criteria.
    let mut comma_sheet = Sheet::new();
    comma_sheet.column(1, 1, vec![t("1,234"), n(1234.0), t("12,345")]);
    let comma_area = comma_sheet.area(1, 1, 3, 1);
    for criterion in [t("1,234"), n(1234.0), t("12,345")] {
        let mut services = EvalServices::with_cells(&mode, &mut comma_sheet)
            .with_number_separators(".", "\u{a0}");
        exact(
            invoke(target, &mut Given(vec![comma_area.clone(), criterion]), &mut services, &mut FunctionCallScratch::new()),
            1.0,
        );
    }

    // Without the captured separators, the numeric coercion is still unknown.
    let mut unprofiled = Sheet::new();
    unprofiled.column(1, 1, vec![t(">1")]);
    let area = unprofiled.area(1, 1, 1, 1);
    let mut services = EvalServices::with_cells(&mode, &mut unprofiled);
    refused(invoke(
        target,
        &mut Given(vec![area, n(0.0)]),
        &mut services,
        &mut FunctionCallScratch::new(),
    ));
}

/// Q3, Excel 16.0.20527: these return-type classes always give #NUM!, and the
/// captured serial and return-type bits locate the observed conversion boundaries.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn weeknum_stable_invalid_types_and_input_boundaries() {
    let e = host();
    for kind in [
        0.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 18.0, 19.0, 20.0,
    ] {
        error(c(&e, "WEEKNUM", vec![n(43835.0), n(kind)]), Num);
    }
    exact(c(&e, "WEEKNUM", vec![n(43835.0), n(11.0 - 2.3e-7)]), 1.0);
    error(c(&e, "WEEKNUM", vec![n(43835.0), n(11.0 - 2.4e-7)]), Num);
    for kind in [-1.0, 22.0, 99.0, 22.0 - 1e-7] {
        refused(c(&e, "WEEKNUM", vec![n(43835.0), n(kind)]));
    }
    error(c(&e, "WEEKNUM", vec![n(-1.0), n(22.0)]), Num);
    exact(
        c(
            &e,
            "WEEKNUM",
            vec![n(f64::from_bits(0x40e5_6780_0000_0000)), n(2.0)],
        ),
        2.0,
    );
    exact(
        c(
            &e,
            "WEEKNUM",
            vec![n(f64::from_bits(0x40e5_677f_ffff_ffff)), n(2.0)],
        ),
        1.0,
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn date_parts_and_serials() {
    let e = host();
    assert_excel_capture(
        c(&e, "DATE", vec![n(2024.0), n(1.0), n(15.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        257,
        1,
    );
    assert_excel_capture(
        c(&e, "DATE", vec![n(1900.0), n(2.0), n(29.0)]),
        include_bytes!("captures/jnt/sections/jnt_dates_random.capture.json"),
        "jnt_dates_random",
        2,
        4,
    );
    exact(c(&e, "DATE", vec![n(108.0), n(1.0), n(2.0)]), 39449.0);
    exact(c(&e, "DATE", vec![n(2008.0), n(14.0), n(2.0)]), 39846.0);
    error(c(&e, "DATE", vec![n(10000.0), n(1.0), n(1.0)]), Num);
    assert_excel_capture(
        c(&e, "YEAR", vec![n(45000.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        342,
        1,
    );
    assert_excel_capture(
        c(&e, "MONTH", vec![n(45000.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        300,
        1,
    );
    assert_excel_capture(
        c(&e, "DAY", vec![n(45000.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        261,
        1,
    );
    exact(c(&e, "HOUR", vec![n(45000.75)]), 18.0);
    assert_excel_capture(
        c(&e, "MINUTE", vec![n(0.51)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        299,
        1,
    );
    assert_excel_capture(
        c(&e, "SECOND", vec![n(0.5)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        317,
        1,
    );
    assert_excel_capture(
        c(&e, "YEAR", vec![n(-1.0)]),
        include_bytes!("captures/jnt/sections/jnt_w111_text_dates.capture.json"),
        "jnt_w111_text_dates",
        3721,
        4,
    );
    assert_excel_capture(
        c(&e, "TIME", vec![n(12.0), n(30.0), n(0.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        328,
        1,
    );
    assert_excel_capture(
        c(&e, "DAYS", vec![n(45010.0), n(45000.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        262,
        1,
    );
    assert_excel_capture(
        c(&e, "DAYS360", vec![n(45000.0), n(45100.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        263,
        1,
    );
    assert_excel_capture(
        c(&e, "EDATE", vec![n(45000.0), n(1.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        277,
        1,
    );
    assert_excel_capture(
        c(&e, "EOMONTH", vec![n(45000.0), n(1.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        279,
        1,
    );
    assert_excel_capture(
        c(&e, "EDATE", vec![b(true), n(1.0)]),
        include_bytes!("captures/jnt/sections/jnt_w111_text_dates.capture.json"),
        "jnt_w111_text_dates",
        4070,
        4,
    );
    assert_excel_capture(
        c(&e, "WEEKDAY", vec![n(45000.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        338,
        1,
    );
    exact(c(&e, "WEEKDAY", vec![n(45000.0), n(2.0)]), 3.0);
    assert_excel_capture(
        c(&e, "WEEKNUM", vec![n(45000.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        339,
        1,
    );
    assert_excel_capture(
        c(&e, "ISOWEEKNUM", vec![n(45000.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        291,
        1,
    );
    exact(
        c(&e, "DATEDIF", vec![n(45000.0), n(45365.0), t("d")]),
        365.0,
    );
    exact(c(&e, "DATEDIF", vec![n(45000.0), n(45365.0), t("m")]), 11.0);
    error(c(&e, "DATEDIF", vec![n(45365.0), n(45000.0), t("d")]), Num);
    assert_excel_capture(
        c(&e, "NETWORKDAYS", vec![n(45000.0), n(45030.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        301,
        1,
    );
    assert_excel_capture(
        c(&e, "WORKDAY", vec![n(45000.0), n(10.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        340,
        1,
    );
    exact(
        c(
            &e,
            "NETWORKDAYS.INTL",
            vec![n(45000.0), n(45030.0), t("0000011")],
        ),
        23.0,
    );
    error(
        c(&e, "WORKDAY.INTL", vec![n(45000.0), n(10.0), t("1111111")]),
        Value,
    );
    // Microsoft's YEARFRAC example: 2012/1/1 to 2012/7/30, basis 0, 1 and 3.
    close(
        c(&e, "YEARFRAC", vec![n(40909.0), n(41120.0)]),
        0.58055556,
        1e-8,
    );
    close(
        c(&e, "YEARFRAC", vec![n(40909.0), n(41120.0), n(1.0)]),
        0.57650273,
        1e-8,
    );
    close(
        c(&e, "YEARFRAC", vec![n(40909.0), n(41120.0), n(3.0)]),
        0.57808219,
        1e-8,
    );
    exact(c(&e, "TODAY", vec![]), 46318.0);
    exact(c(&e, "NOW", vec![]), 46318.6);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn the_1904_date_system() {
    let mut e = host();
    e.date_system = DateSystem::System1904;
    exact(
        c(&e, "DATE", vec![n(2024.0), n(1.0), n(15.0)]),
        45306.0 - 1462.0,
    );
    exact(c(&e, "DATE", vec![n(1904.0), n(1.0), n(1.0)]), 0.0);
    error(c(&e, "DATE", vec![n(1903.0), n(12.0), n(31.0)]), Num);
    exact(c(&e, "YEAR", vec![n(0.0)]), 1904.0);
    exact(c(&e, "YEAR", vec![n(45306.0 - 1462.0)]), 2024.0);
    exact(c(&e, "EOMONTH", vec![n(0.0), n(0.0)]), 30.0);
    exact(c(&e, "WEEKDAY", vec![n(0.0)]), 6.0);
    exact(c(&e, "VALUE", vec![t("2024/01/15")]), 45306.0 - 1462.0);
    text(c(&e, "TEXT", vec![n(0.0), t("yyyy-mm-dd")]), "1904-01-01");
    exact(c(&e, "TODAY", vec![]), 46318.0 - 1462.0);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn decimal_reader_uses_extended_reciprocal_at_fifteen_digits() {
    let env = host();
    for (source, bits) in [
        ("0.526151728402037", 0x3fe0_d63c_2647_0f58),
        ("0.757980888354922", 0x3fe8_4161_22cf_4700),
        ("0.811932786917339", 0x3fe9_fb5a_77cb_8600),
    ] {
        let expected = f64::from_bits(bits);
        exact(c(&env, "VALUE", vec![t(source)]), expected);
        exact(c(&env, "NUMBERVALUE", vec![t(source), t("."), t(",")]), expected);
    }
    exact(c(&env, "VALUE", vec![t("526151728402037e-15")]), f64::from_bits(0x3fe0_d63c_2647_0f58));
    exact(c(&env, "VALUE", vec![t("526151728402037e-13%")]), f64::from_bits(0x3fe0_d63c_2647_0f58));
    exact(
        c(&env, "NUMBERVALUE", vec![n(f64::from_bits(0xbfe1_4d34_fbc6_84a8)), t("1")]),
        f64::from_bits(0xbfe1_4d34_fbc6_84a8),
    );
    for separator in [",", ".", " ", "\u{a0}"] {
        error(c(&env, "NUMBERVALUE", vec![t("123.45"), t(separator), t(separator)]), Value);
        error(c(&env, "NUMBERVALUE", vec![n(12.0), t(separator), t(separator)]), Value);
        error(c(&env, "NUMBERVALUE", vec![e(NA), t(separator), t(separator)]), NA);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn value_datevalue_timevalue() {
    let e = host();
    // Excel: VALUE("1 234.5") 1234.5, VALUE("R 12") 12, VALUE("12:30") 0.520833..., VALUE("1/2")
    // 46024, DATEVALUE("2024/01/05 12:30") 45296, TIMEVALUE("25:00") 0.041666666666666741.
    assert_excel_capture(
        c(&e, "VALUE", vec![t("1 234.5")]),
        include_bytes!("captures/catalogue/sections/f2_rows.capture.json"),
        "f2_rows",
        38,
        4,
    );
    exact(c(&e, "VALUE", vec![t("R 12")]), 12.0);
    exact(c(&e, "VALUE", vec![t("12:30")]), 0.52083333333333337);
    exact(c(&e, "VALUE", vec![t("1/2")]), 46024.0);
    assert_excel_capture(
        c(&e, "VALUE", vec![t("1,234")]),
        include_bytes!("captures/catalogue/sections/f2_rows.capture.json"),
        "f2_rows",
        48,
        4,
    );
    assert_excel_capture(
        c(&e, "VALUE", vec![b(true)]),
        include_bytes!("captures/catalogue/sections/f2_rows.capture.json"),
        "f2_rows",
        51,
        4,
    );
    exact(c(&e, "VALUE", vec![n(5.0)]), 5.0);
    exact(c(&e, "DATEVALUE", vec![t("2024/01/05 12:30")]), 45296.0);
    exact(c(&e, "DATEVALUE", vec![t("12:30")]), 0.0);
    error(c(&e, "DATEVALUE", vec![t("12")]), Value);
    error(c(&e, "DATEVALUE", vec![n(45000.0)]), Value);
    exact(c(&e, "TIMEVALUE", vec![t("25:00")]), 0.041666666666666741);
    exact(
        c(&e, "TIMEVALUE", vec![t("2024/01/05 12:30")]),
        0.52083333333575865,
    );
    // Number text in arithmetic reads the same way once the environment holds a profile.
    exact(c(&e, "SUM", vec![t("1 jan 2020"), n(1.0)]), 43832.0);
    exact(c(&e, "NUMBERVALUE", vec![t("1 234.5")]), 1234.5);
    exact(c(&e, "NUMBERVALUE", vec![t("")]), 0.0);
    // Live Excel 16.0.20625.20108 en-ZA via Touchstone (dc-j9k.229): ASCII spaces are stripped, and a
    // doubled trailing percent divides twice, whatever the separators given.
    exact(c(&e, "NUMBERVALUE", vec![t("12 34")]), 1234.0);
    exact(c(&e, "NUMBERVALUE", vec![t("12 34"), t("%")]), 1234.0);
    exact(c(&e, "NUMBERVALUE", vec![t("3.5%%")]), f64::from_bits(0x3f36_f006_8db8_bac8));
    exact(c(&e, "NUMBERVALUE", vec![t("3.5%%"), t("\n")]), f64::from_bits(0x3f36_f006_8db8_bac8));
    exact(c(&e, "NUMBERVALUE", vec![t("3.5%%"), t(".")]), f64::from_bits(0x3f36_f006_8db8_bac8));
    error(c(&e, "NUMBERVALUE", vec![t("1,234.5")]), Value);
    error(c(&e, "NUMBERVALUE", vec![t("1.234,5"), t(",")]), Value);
    for (text, decimal, group, expected) in [
        ("12", "-", "1", 2.0),
        ("-12", "-", "x", 0.12),
        ("-12", ".", "-", 12.0),
        ("123", "2", ",", 1.3),
        ("0.5", "-", ",", 0.5),
        ("1%", "%", ",", 1.0),
        ("12%", ".", "%", 12.0),
        ("0.123", "-", "1", 0.23),
        ("0.123", ".", "1", 0.123),
    ] {
        exact(
            c(&e, "NUMBERVALUE", vec![t(text), t(decimal), t(group)]),
            expected,
        );
    }

    exact(c(&e, "NUMBERVALUE", vec![t("1,5"), t(","), t(".")]), 1.5);

    // Excel 20527 controls: decisive argument errors precede missing formatting locale.
    use calc_func::value::NumericMode;
    use calc_func::{EvalServices, FunctionCallScratch, Notation, catalogue, invoke};
    let partial = CalcEnvironment::dates(DateSystem::System1900);
    let mode = NumericMode::Fast;
    for name in ["VALUE", "DATEVALUE", "TIMEVALUE"] {
        let target = catalogue().resolve_surface(Notation::Call, name).unwrap();
        let mut services = EvalServices::new(&mode).with_environment(&partial)
            .with_number_separators(".", "\u{a0}");
        error(invoke(target, &mut Given(vec![t("1,234")]), &mut services, &mut FunctionCallScratch::new()), Value);
        let mut unknown = EvalServices::new(&mode).with_environment(&partial)
            .with_number_separators(".", ",");
        refused(invoke(target, &mut Given(vec![t("1,234")]), &mut unknown, &mut FunctionCallScratch::new()));
    }
    let dollar = catalogue().resolve_surface(Notation::Call, "DOLLAR").unwrap();
    for (args, expected) in [
        (vec![n(1.0), t("1,234")], Value),
        (vec![f1_support::e(Div0), t("1,234")], Div0),
        (vec![t("1,234"), f1_support::e(Num)], Value),
        (vec![n(1.0), f1_support::e(NA)], NA),
    ] {
        let mut services = EvalServices::new(&mode).with_environment(&partial)
            .with_number_separators(".", "\u{a0}");
        error(invoke(dollar, &mut Given(args), &mut services, &mut FunctionCallScratch::new()), expected);
    }
    let mut services = EvalServices::new(&mode).with_environment(&partial)
        .with_number_separators(".", "\u{a0}");
    refused(invoke(dollar, &mut Given(vec![n(1.0), n(2.0)]), &mut services, &mut FunctionCallScratch::new()));
}

/// Touchstone Excel 16.0.20527 confirmed the Batch 1 time text and nearby controls.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn unmarked_time_text_admits_one_extended_component() {
    let env = host();
    for (input, bits) in [
        ("0:240", 0x3fc5_5555_5555_5555),
        ("0:0240", 0x3fc5_5555_5555_5555),
        ("0:1441", 0x3f46_c16c_16c1_7000),
        ("0:0:9999", 0x3fbd_a06d_3a06_d3a0),
        ("0:240:59.", 0x3fc5_6bb5_aa49_9388),
    ] {
        exact(c(&env, "TIMEVALUE", vec![t(input)]), f64::from_bits(bits));
    }
    for input in ["0:001", "0:10000", "0:240:60", "0:60:240", "001:240", "0:240 PM"] {
        error(c(&env, "TIMEVALUE", vec![t(input)]), Value);
    }
    exact(c(&env, "VALUE", vec![t("0:240")]), f64::from_bits(0x3fc5_5555_5555_5555));
    exact(c(&env, "DATEVALUE", vec![t("0:240")]), 0.0);
}

/// Touchstone Excel 16.0.20527, with the date serial stored before the date or time is extracted.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn date_qualified_time_text_admits_one_extended_component() {
    let env = host();
    for (input, value, day, time) in [
        (
            "2020/01/02 0:240",
            0x40e5_6705_5555_5555,
            0x40e5_6700_0000_0000,
            0x3fc5_5555_5554_0000,
        ),
        (
            "0:240 1 jan 2020",
            0x40e5_66e5_5555_5555,
            0x40e5_66e0_0000_0000,
            0x3fc5_5555_5554_0000,
        ),
        (
            "2020-01-02 0:0:240.5",
            0x40e5_6700_16cd_8efb,
            0x40e5_6700_0000_0000,
            0x3f66_cd8e_fb00_0000,
        ),
        (
            "2020-01-02 01:9999",
            0x40e5_67df_8888_8889,
            0x40e5_67c0_0000_0000,
            0x3fef_8888_8889_0000,
        ),
    ] {
        for (function, bits) in [("VALUE", value), ("DATEVALUE", day), ("TIMEVALUE", time)] {
            exact(c(&env, function, vec![t(input)]), f64::from_bits(bits));
        }
    }
    for input in [
        "2020/01/02 0:240:60",
        "2020/01/02 0:240 AM",
        "0:240: 2020/01/02",
        "0:240 1/02/2020",
        "2020/01/32 0:240",
    ] {
        error(c(&env, "TIMEVALUE", vec![t(input)]), Value);
    }
}

/// Excel 20625 reads a number after the space as part of an unfinished time.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn time_text_borrows_the_number_after_trailing_punctuation() {
    let env = host();
    for (input, value, day, time) in [
        (
            "0:240:59. 2020/01/02",
            0x40e6_7905_5af2_51c2,
            0x40e6_7900_0000_0000,
            0x3fc5_6bc9_4708_0000,
        ),
        (
            "0:240: 1 jan 2020",
            0x40e5_66e5_556d_9b1e,
            0x40e5_66e0_0000_0000,
            0x3fc5_55b6_6c78_0000,
        ),
        (
            "0:40:59. 2025/01/02",
            0x40e6_7900_e92b_3b87,
            0x40e6_7900_0000_0000,
            0x3f9d_2567_70e0_0000,
        ),
        (
            "0:40:59. 2025",
            0x3f9d_2567_70d0_e3d2,
            0,
            0x3f9d_2567_70d0_e3d2,
        ),
    ] {
        for (function, bits) in [("VALUE", value), ("DATEVALUE", day), ("TIMEVALUE", time)] {
            exact(c(&env, function, vec![t(input)]), f64::from_bits(bits));
        }
    }
    for input in ["0:40:59. Jan 2020", "0:240: 2020/01/02", "0:40:59. 1/1/2020"] {
        error(c(&env, "TIMEVALUE", vec![t(input)]), Value);
    }
}

/// dc-iuwct.44 (Excel 16.0.20527 en-ZA, 52/53 + 17/17 + 10/10 frozen predictions):
/// year-less date text resolves against the calculation clock's year (same clock source as
/// TODAY/NOW) in VALUE, DATEVALUE, NUMBERVALUE, date functions, and arithmetic coercion,
/// without making any of those functions volatile.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn yearless_date_text_uses_today_clock_across_functions_and_arithmetic() {
    use calc_func::value::NumericMode;
    use calc_func::{EvalServices, FunctionCallScratch, Notation, catalogue, invoke};

    let call_op = |env: &CalcEnvironment, notation: Notation, op: &str, args: Vec<CalcValue>| {
        let target = catalogue().resolve_surface(notation, op).unwrap();
        let mode = NumericMode::Fast;
        let mut services = EvalServices::new(&mode).with_environment(env);
        invoke(
            target,
            &mut Given(args),
            &mut services,
            &mut FunctionCallScratch::new(),
        )
    };

    // 2026-11-20 (serial 46346, non-leap year): matches Excel 16.0.20527 live session.
    let e2026 = CalcEnvironment {
        now: Some(46346.5),
        ..host()
    };
    exact(c(&e2026, "TODAY", vec![]), 46346.0);
    for (input, value_expected, datevalue_expected) in [
        ("1-3", 46025.0, 46025.0),
        ("1/3", 46025.0, 46025.0),
        ("3/4", 46085.0, 46085.0),
        ("1 jan", 46023.0, 46023.0),
        ("jan 3", 46025.0, 46025.0),
        ("13-1", 46035.0, 46035.0),
        ("1-13", 46035.0, 46035.0),
        ("29-2", 47150.0, 47150.0),
        ("1-3 12:00", 46025.5, 46025.0),
    ] {
        exact(c(&e2026, "VALUE", vec![t(input)]), value_expected);
        exact(c(&e2026, "DATEVALUE", vec![t(input)]), datevalue_expected);
        exact(
            call_op(&e2026, Notation::Infix, "+", vec![t(input), n(0.0)]),
            value_expected,
        );
    }
    error(c(&e2026, "VALUE", vec![t("2-29")]), Value);
    error(c(&e2026, "DATEVALUE", vec![t("2-29")]), Value);
    error(
        call_op(&e2026, Notation::Infix, "+", vec![t("2-29"), n(0.0)]),
        Value,
    );

    // Unary `+` keeps text untouched; double unary `-` coerces through the same date grammar.
    text(call_op(&e2026, Notation::Prefix, "+", vec![t("1-3")]), "1-3");
    let neg = call_op(&e2026, Notation::Prefix, "-", vec![t("1-3")]).unwrap();
    exact(call_op(&e2026, Notation::Prefix, "-", vec![neg]), 46025.0);

    // Date and aggregate functions share the same coercion and clock.
    exact(c(&e2026, "SUM", vec![t("1-3"), n(0.0)]), 46025.0);
    exact(c(&e2026, "YEAR", vec![t("1-3")]), 2026.0);
    exact(c(&e2026, "MONTH", vec![t("1-3")]), 1.0);
    exact(c(&e2026, "DAY", vec![t("1-3")]), 3.0);
    exact(c(&e2026, "EDATE", vec![t("1-3"), n(1.0)]), 46056.0);
    exact(c(&e2026, "DAYS", vec![t("1-10"), t("1-3")]), 7.0);
    exact(c(&e2026, "TIMEVALUE", vec![t("1-3 12:00")]), 0.5);

    // NUMBERVALUE strips ASCII spaces and trailing percents before falling back to VALUE.
    for (input, dec, grp, expected) in [
        ("1e-3", ".", "e", 46025.0),
        ("1e-3%", ".", "e", 460.25),
        ("1e/3", ".", "e", 46025.0),
        ("1-3", ".", ",", 46025.0),
        ("1-3", ",", ".", 46025.0),
        ("1/3", ".", ",", 46025.0),
        ("3/4", ".", ",", 46085.0),
        ("1/2", ".", ",", 46024.0),
        ("1/2%", ".", ",", 460.24),
        ("1 1/2", ".", ",", 46328.0),
        ("1 jan", ".", ",", 46023.0),
        ("jan 3", ".", ",", 46025.0),
        ("13-1", ".", ",", 46035.0),
        ("1-13", ".", ",", 46035.0),
        ("29-2", ".", ",", 47150.0),
        ("2024/01/05", ".", ",", 45296.0),
        ("2024-01-05", ".", ",", 45296.0),
        ("1 jan 2020", ".", ",", 43831.0),
        ("1-jan-2020", ".", ",", 43831.0),
        ("12:00", ".", ",", 0.5),
        ("12:30", ".", ",", 0.5208333333333334),
        ("12:30:15", ".", ",", 0.5210069444444444),
        ("12:30%", ".", ",", 0.005208333333333334),
        ("1/2", "/", ",", 1.2),
        ("1/2", ".", "/", 12.0),
        ("1:3", ":", ",", 1.3),
        ("1-3", "-", ",", 1.3),
        ("1-3", ".", "-", 13.0),
    ] {
        exact(
            c(&e2026, "NUMBERVALUE", vec![t(input), t(dec), t(grp)]),
            expected,
        );
    }
    for input in ["2-29", "1-3 12:00", "2024/01/05 12:30", "12:30 pm"] {
        error(
            c(&e2026, "NUMBERVALUE", vec![t(input), t("."), t(",")]),
            Value,
        );
    }

    // Moving the shared clock to a leap year (2024-06-15, serial 45458) shifts all of them
    // together and makes "2-29" valid (2024-02-29 = 45351).
    let e2024 = CalcEnvironment {
        now: Some(45458.5),
        ..host()
    };
    exact(c(&e2024, "TODAY", vec![]), 45458.0);
    exact(c(&e2024, "VALUE", vec![t("1-3")]), 45294.0);
    exact(c(&e2024, "DATEVALUE", vec![t("1-3")]), 45294.0);
    exact(c(&e2024, "NUMBERVALUE", vec![t("1e-3"), t("."), t("e")]), 45294.0);
    exact(c(&e2024, "NUMBERVALUE", vec![t("1/3"), t("."), t(",")]), 45294.0);
    exact(call_op(&e2024, Notation::Infix, "+", vec![t("1-3"), n(0.0)]), 45294.0);
    exact(c(&e2024, "VALUE", vec![t("2-29")]), 45351.0);
    exact(c(&e2024, "DATEVALUE", vec![t("2-29")]), 45351.0);
    exact(c(&e2024, "NUMBERVALUE", vec![t("2-29"), t("."), t(",")]), 45351.0);
    exact(call_op(&e2024, Notation::Infix, "+", vec![t("2-29"), n(0.0)]), 45351.0);

    // Without a calculation clock, year-less date text is refused instead of guessing a year,
    // whereas an explicit-year date still evaluates.
    let no_clock = CalcEnvironment {
        now: None,
        ..host()
    };
    refused(c(&no_clock, "VALUE", vec![t("1-3")]));
    refused(c(&no_clock, "DATEVALUE", vec![t("1-3")]));
    refused(c(&no_clock, "NUMBERVALUE", vec![t("1e-3"), t("."), t("e")]));
    refused(call_op(&no_clock, Notation::Infix, "+", vec![t("1-3"), n(0.0)]));
    exact(c(&no_clock, "VALUE", vec![t("2024-01-05")]), 45296.0);
    exact(c(&no_clock, "NUMBERVALUE", vec![t("2024/01/05"), t("."), t(",")]), 45296.0);

    // Volatility (Excel 16.0.20527 WEEKNUM probe, 10/10): none of these functions are volatile,
    // unlike TODAY and NOW.
    for name in ["VALUE", "DATEVALUE", "NUMBERVALUE", "YEAR"] {
        assert!(
            !catalogue()
                .resolve_surface(Notation::Call, name)
                .unwrap()
                .traits()
                .volatile,
            "{name} must not be volatile"
        );
    }
    assert!(
        !catalogue()
            .resolve_surface(Notation::Infix, "+")
            .unwrap()
            .traits()
            .volatile
    );
    for name in ["TODAY", "NOW"] {
        assert!(
            catalogue()
                .resolve_surface(Notation::Call, name)
                .unwrap()
                .traits()
                .volatile,
            "{name} must be volatile"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn text_fixed_dollar() {
    let e = host();
    text(c(&e, "TEXT", vec![n(1234.5), t("#,##0.00")]), "1,234.50");
    text(c(&e, "TEXT", vec![n(1234567.891), t("#,##0")]), "1234,568");
    text(
        c(&e, "TEXT", vec![n(1.0 / 3.0), t("General")]),
        "0.333333333",
    );
    text(
        c(&e, "TEXT", vec![n(45000.0), t("dd mmm yyyy")]),
        "15 Mar 2023",
    );
    text(c(&e, "TEXT", vec![t("abc"), t("0.00")]), "abc");
    text(c(&e, "TEXT", vec![t("12"), t("0.00")]), "12.00");
    text(c(&e, "TEXT", vec![b(true), t("0")]), "TRUE");
    error(c(&e, "TEXT", vec![n(-1.0), t("yyyy")]), Value);
    // Excel 16.0.20430 under en-ZA (dc-uar): a fraction with an integer part is #VALUE!, and a
    // fraction alone renders.
    error(c(&e, "TEXT", vec![n(5.0), t("# ?/?")]), Value);
    text(c(&e, "TEXT", vec![n(0.7), t("?/?")]), "2/3");
    text(c(&e, "FIXED", vec![n(1234.567)]), "1 234.57");
    text(c(&e, "FIXED", vec![n(1234.567), n(1.0), b(true)]), "1234.6");
    text(c(&e, "DOLLAR", vec![n(1234.567)]), "R1 234.57");
    text(c(&e, "DOLLAR", vec![n(-1234.567)]), "-R1 234.57");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn delimited_split_regex_and_conversions() {
    let e = host();
    text(c(&e, "TEXTBEFORE", vec![t("a-b-c"), t("-")]), "a");
    text(c(&e, "TEXTAFTER", vec![t("a-b-c"), t("-"), n(-1.0)]), "c");
    text(
        c(
            &e,
            "TEXTBEFORE",
            vec![t("a-b-c"), t("-"), n(-1.0), n(0.0), n(1.0)],
        ),
        "a-b",
    );
    error(
        c(
            &e,
            "TEXTAFTER",
            vec![t("a-b-c"), t("-"), n(-4.0), n(0.0), n(1.0)],
        ),
        NA,
    );
    text(
        c(
            &e,
            "TEXTBEFORE",
            vec![
                t("a-b-c"),
                t("x"),
                missing(),
                missing(),
                missing(),
                t("none"),
            ],
        ),
        "none",
    );
    text(c(&e, "TEXTSPLIT", vec![t("ab"), t("-")]), "ab");
    let (rows, cols, _) = {
        let value = c(&e, "TEXTSPLIT", vec![t("1,2;3"), t(","), t(";")]).unwrap();
        let array = value.as_array().expect("an array").clone();
        let shape = array.shape();
        (shape.rows, shape.cols, ())
    };
    assert_eq!((rows, cols), (2, 2));
    assert_eq!(
        c(&e, "REGEXTEST", vec![t("ab12"), t("[0-9]")])
            .unwrap()
            .as_logical(),
        Some(true)
    );
    text(c(&e, "REGEXEXTRACT", vec![t("ab12"), t("[0-9]+")]), "12");
    text(
        c(&e, "REGEXREPLACE", vec![t("ab12"), t("[0-9]"), t("x")]),
        "abxx",
    );
    refused(c(&e, "REGEXTEST", vec![t("ab"), t("(a|b)")]));
    text(c(&e, "VALUETOTEXT", vec![t("a\"b"), n(1.0)]), "\"a\"\"b\"");
    text(
        c(
            &e,
            "ARRAYTOTEXT",
            vec![
                arr_values(vec![vec![n(1.0), t("a")], vec![b(true), e_na()]]),
                n(1.0),
            ],
        ),
        "{1,\"a\";TRUE,#N/A}",
    );
    text(
        c(
            &e,
            "ARRAYTOTEXT",
            vec![arr_values(vec![
                vec![n(1.0), t("a")],
                vec![b(true), e_na()],
            ])],
        ),
        "1, a, TRUE, #N/A",
    );
    text(c(&e, "ASC", vec![t("ＡＢＣ")]), "ＡＢＣ");
    exact(c(&e, "HYPERLINK", vec![t("x"), n(5.0)]), 5.0);
    assert_excel_capture(
        c(&e, "HYPERLINK", vec![t("https://example.com")]),
        include_bytes!("captures/catalogue/sections/f2_rows.capture.json"),
        "f2_rows",
        179,
        4,
    );
    // Excel 16.0.20430: ENCODEURL("a b") is a%20b (dc-uar; sheet dcuar_text of the catalogue oracle).
    text(c(&e, "ENCODEURL", vec![t("a b")]), "a%20b");
    // Excel: BAHTTEXT of these numbers on this machine, read ad hoc.
    for (number, expected) in [
        (
            0.0,
            "\u{0e28}\u{0e39}\u{0e19}\u{0e22}\u{0e4c}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}",
        ),
        (
            1.0,
            "\u{0e2b}\u{0e19}\u{0e36}\u{0e48}\u{0e07}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}",
        ),
        (
            11.0,
            "\u{0e2a}\u{0e34}\u{0e1a}\u{0e40}\u{0e2d}\u{0e47}\u{0e14}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}",
        ),
        (
            21.0,
            "\u{0e22}\u{0e35}\u{0e48}\u{0e2a}\u{0e34}\u{0e1a}\u{0e40}\u{0e2d}\u{0e47}\u{0e14}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}",
        ),
        (
            101.0,
            "\u{0e2b}\u{0e19}\u{0e36}\u{0e48}\u{0e07}\u{0e23}\u{0e49}\u{0e2d}\u{0e22}\u{0e40}\u{0e2d}\u{0e47}\u{0e14}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}",
        ),
        (
            1234567.89,
            "\u{0e2b}\u{0e19}\u{0e36}\u{0e48}\u{0e07}\u{0e25}\u{0e49}\u{0e32}\u{0e19}\u{0e2a}\u{0e2d}\u{0e07}\u{0e41}\u{0e2a}\u{0e19}\u{0e2a}\u{0e32}\u{0e21}\u{0e2b}\u{0e21}\u{0e37}\u{0e48}\u{0e19}\u{0e2a}\u{0e35}\u{0e48}\u{0e1e}\u{0e31}\u{0e19}\u{0e2b}\u{0e49}\u{0e32}\u{0e23}\u{0e49}\u{0e2d}\u{0e22}\u{0e2b}\u{0e01}\u{0e2a}\u{0e34}\u{0e1a}\u{0e40}\u{0e08}\u{0e47}\u{0e14}\u{0e1a}\u{0e32}\u{0e17}\u{0e41}\u{0e1b}\u{0e14}\u{0e2a}\u{0e34}\u{0e1a}\u{0e40}\u{0e01}\u{0e49}\u{0e32}\u{0e2a}\u{0e15}\u{0e32}\u{0e07}\u{0e04}\u{0e4c}",
        ),
        (
            1000001.0,
            "\u{0e2b}\u{0e19}\u{0e36}\u{0e48}\u{0e07}\u{0e25}\u{0e49}\u{0e32}\u{0e19}\u{0e40}\u{0e2d}\u{0e47}\u{0e14}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}",
        ),
        (
            0.5,
            "\u{0e2b}\u{0e49}\u{0e32}\u{0e2a}\u{0e34}\u{0e1a}\u{0e2a}\u{0e15}\u{0e32}\u{0e07}\u{0e04}\u{0e4c}",
        ),
        (
            0.01,
            "\u{0e2b}\u{0e19}\u{0e36}\u{0e48}\u{0e07}\u{0e2a}\u{0e15}\u{0e32}\u{0e07}\u{0e04}\u{0e4c}",
        ),
        (
            -12.25,
            "\u{0e25}\u{0e1a}\u{0e2a}\u{0e34}\u{0e1a}\u{0e2a}\u{0e2d}\u{0e07}\u{0e1a}\u{0e32}\u{0e17}\u{0e22}\u{0e35}\u{0e48}\u{0e2a}\u{0e34}\u{0e1a}\u{0e2b}\u{0e49}\u{0e32}\u{0e2a}\u{0e15}\u{0e32}\u{0e07}\u{0e04}\u{0e4c}",
        ),
        (
            10.0,
            "\u{0e2a}\u{0e34}\u{0e1a}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}",
        ),
        (
            20.0,
            "\u{0e22}\u{0e35}\u{0e48}\u{0e2a}\u{0e34}\u{0e1a}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}",
        ),
        (
            1000000.0,
            "\u{0e2b}\u{0e19}\u{0e36}\u{0e48}\u{0e07}\u{0e25}\u{0e49}\u{0e32}\u{0e19}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}",
        ),
        (
            11000000.0,
            "\u{0e2a}\u{0e34}\u{0e1a}\u{0e40}\u{0e2d}\u{0e47}\u{0e14}\u{0e25}\u{0e49}\u{0e32}\u{0e19}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}",
        ),
        (
            1.005,
            "\u{0e2b}\u{0e19}\u{0e36}\u{0e48}\u{0e07}\u{0e1a}\u{0e32}\u{0e17}\u{0e2b}\u{0e19}\u{0e36}\u{0e48}\u{0e07}\u{0e2a}\u{0e15}\u{0e32}\u{0e07}\u{0e04}\u{0e4c}",
        ),
        (
            100000000000.0,
            "\u{0e2b}\u{0e19}\u{0e36}\u{0e48}\u{0e07}\u{0e41}\u{0e2a}\u{0e19}\u{0e25}\u{0e49}\u{0e32}\u{0e19}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}",
        ),
        (
            2.999,
            "\u{0e2a}\u{0e32}\u{0e21}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}",
        ),
    ] {
        text(call("BAHTTEXT", vec![n(number)]), expected);
    }
}

fn e_na() -> CalcValue {
    e(NA)
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn address_formats_references() {
    text(call("ADDRESS", vec![n(2.0), n(3.0)]), "$C$2");
    text(call("ADDRESS", vec![n(2.0), n(3.0), n(2.0)]), "C$2");
    text(
        call("ADDRESS", vec![n(2.0), n(3.0), n(2.0), b(false)]),
        "R2C[3]",
    );
    text(
        call(
            "ADDRESS",
            vec![n(2.0), n(3.0), n(1.0), b(false), t("[Book1]Sheet1")],
        ),
        "[Book1]Sheet1!R2C3",
    );
    text(
        call(
            "ADDRESS",
            vec![n(2.0), n(3.0), n(1.0), b(false), t("EXCEL SHEET")],
        ),
        "'EXCEL SHEET'!R2C3",
    );
    error(call("ADDRESS", vec![n(0.0), n(3.0)]), Value);
}

struct Host {
    sheet: SheetId,
    formulas: Vec<(CellKey, String)>,
}

impl HostFacts for Host {
    fn pivot_data_cell(
        &mut self,
        _: &str,
        _: &[calc_func::value::GridArea],
        _: &[(String, calc_func::value::CalcValue)],
    ) -> calc_func::PivotLookup {
        calc_func::PivotLookup::NotModelled
    }

    fn is_external_sheet(&mut self, _: SheetId) -> bool {
        false
    }
    fn sheet_named(&mut self, _: &str) -> Option<SheetId> {
        None
    }
    fn sheet_name(&mut self, _: SheetId) -> Option<String> {
        None
    }
    fn workbook_facts(&mut self) -> Option<std::sync::Arc<dyn calc_func::WorkbookFacts>> {
        None
    }

    fn caller(&mut self) -> Option<CellKey> {
        CellKey::new(self.sheet, 1, 1).ok()
    }
    fn is_formula(&mut self, cell: CellKey) -> Option<bool> {
        Some(self.formulas.iter().any(|(at, _)| *at == cell))
    }
    fn formula_text(&mut self, cell: CellKey) -> Option<Option<String>> {
        Some(
            self.formulas
                .iter()
                .find(|(at, _)| *at == cell)
                .map(|(_, f)| f.clone()),
        )
    }
    fn sheet_index(&mut self, sheet: SheetId) -> Option<u32> {
        (sheet == self.sheet).then_some(1)
    }
    fn sheet_count(&mut self) -> Option<u32> {
        Some(3)
    }
}

/// Live Excel (16.0.20625, dc-j9k.236-.239): WORKDAY.INTL checks the target range first (start +
/// floor(days) outside 0..=2958465 is #NUM!), returns the start for zero days without reading the
/// weekend or the holidays, reads the holidays before the weekend, and only then rejects a bad weekend.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn workday_intl_checks_range_then_zero_days_then_holidays_then_weekend() {
    let env = host();
    let w = |args: Vec<CalcValue>| c(&env, "WORKDAY.INTL", args);
    // The target is out of range: #NUM! whatever the weekend says.
    error(w(vec![n(61.0), n(-100.0), t("1111111")]), Num);
    error(w(vec![n(61.0), n(-100.0), t("abc")]), Num);
    error(w(vec![n(61.0), n(-62.0), t("abc")]), Num);
    error(w(vec![n(61.0), n(-61.5), t("abc")]), Num);
    error(w(vec![n(61.0), n(2958405.0), t("abc")]), Num);
    error(w(vec![n(2958465.0), n(1.0), t("abc")]), Num);
    error(w(vec![n(61.0), n(-100.0), n(1.0), e(NA)]), Num);
    // In range: a bad weekend is #VALUE! (text) or #NUM! (code).
    error(w(vec![n(61.0), n(-61.0), t("abc")]), Value);
    error(w(vec![n(61.0), n(-60.5), t("abc")]), Value);
    error(w(vec![n(61.0), n(-0.5), t("abc")]), Value);
    error(w(vec![n(61.0), n(2958404.0), t("abc")]), Value);
    error(w(vec![n(61.0), n(1.0), t("1111111")]), Value);
    error(w(vec![n(61.0), n(1.0), n(0.0)]), Num);
    error(w(vec![n(61.0), n(1.0), n(18.0)]), Num);
    // Holidays are read before the weekend.
    error(w(vec![n(61.0), n(1.0), t("abc"), e(NA)]), NA);
    error(w(vec![n(61.0), n(1.0), n(1.0), e(NA)]), NA);
    // Zero days: the start, even with a bad weekend or holidays; only the start is checked.
    exact(w(vec![n(61.0), n(0.0), n(0.0)]), 61.0);
    exact(w(vec![n(61.0), n(0.5), t("abc")]), 61.0);
    exact(w(vec![n(61.0), n(0.0), n(1.0), e(NA)]), 61.0);
    exact(w(vec![n(1.0), n(0.0), t("1111111")]), 1.0);
    exact(w(vec![n(2958465.0), n(0.0), t("abc")]), 2958465.0);
    error(w(vec![n(2958466.0), n(0.0), n(7.0)]), Num);
    error(w(vec![n(-1.0), n(0.0), n(7.0)]), Num);
}

/// dc-shqi1: in 1904 date mode, WORKDAY.INTL must check its target against the 1904 origin (serial
/// 1462 in 1900 coordinates) before reading holidays or weekend.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn workday_intl_1904_system_checks_target_against_1904_origin() {
    let mut env = host();
    env.date_system = DateSystem::System1904;
    let w = |args: Vec<CalcValue>| c(&env, "WORKDAY.INTL", args);
    error(w(vec![n(1.0), n(-5.0), t("abc")]), Num);
    error(w(vec![n(1.0), n(-2.0), t("abc")]), Num);
    error(w(vec![n(1.0), n(-1.0), t("abc")]), Value);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn host_facts() {
    let e = host();
    let mut sheet = Sheet::new();
    sheet.column(1, 2, vec![t("x"), n(5.0)]);
    let mut facts = Host {
        sheet: sheet.id,
        formulas: vec![(sheet.key(2, 2), "=2+3".to_owned())],
    };
    let b1 = sheet.area(1, 2, 1, 2);
    let b2 = sheet.area(2, 2, 2, 2);
    let b3 = sheet.area(3, 2, 3, 2);
    assert_eq!(
        call_in(&e, None, Some(&mut facts), "ISFORMULA", vec![b2.clone()])
            .unwrap()
            .as_logical(),
        Some(true)
    );
    assert_eq!(
        call_in(&e, None, Some(&mut facts), "ISFORMULA", vec![b1.clone()])
            .unwrap()
            .as_logical(),
        Some(false)
    );
    refused(call("ISFORMULA", vec![b1.clone()]));
    text(
        call_in(&e, None, Some(&mut facts), "FORMULATEXT", vec![b2.clone()]),
        "=2+3",
    );
    error(
        call_in(&e, None, Some(&mut facts), "FORMULATEXT", vec![b1.clone()]),
        NA,
    );
    exact(call_in(&e, None, Some(&mut facts), "SHEET", vec![]), 1.0);
    // dc-6yw: ROW() and COLUMN() without a reference are the caller's (A1 here).
    exact(call_in(&e, None, Some(&mut facts), "ROW", vec![]), 1.0);
    exact(call_in(&e, None, Some(&mut facts), "COLUMN", vec![]), 1.0);
    refused(call("ROW", vec![]));
    exact(call_in(&e, None, Some(&mut facts), "SHEETS", vec![]), 3.0);
    exact(call("SHEETS", vec![b1.clone()]), 1.0);
    text(
        call_in(
            &e,
            None,
            Some(&mut facts),
            "CELL",
            vec![t("address"), b2.clone()],
        ),
        "$B$2",
    );
    exact(call("CELL", vec![t("row"), b2.clone()]), 2.0);
    exact(call("CELL", vec![t("col"), b2.clone()]), 2.0);
    text(
        call_on(&mut sheet, "CELL", vec![t("type"), b1.clone()]),
        "l",
    );
    text(
        call_on(&mut sheet, "CELL", vec![t("type"), b3.clone()]),
        "b",
    );
    exact(
        call_on(&mut sheet, "CELL", vec![t("contents"), b2.clone()]),
        5.0,
    );
    error(call("CELL", vec![t("bogus"), b2.clone()]), Value);
    refused(call("CELL", vec![t("width"), b2]));
    refused(call("INFO", vec![t("system")]));
    error(call("INFO", vec![t("bogus")]), Value);
    let col = sheet.area(1, 2, 3, 2);
    let (rows, cols, _) = {
        let value = call_on(&mut sheet, "TRANSPOSE", vec![col]).unwrap();
        let shape = value.as_array().unwrap().shape();
        (shape.rows, shape.cols, ())
    };
    assert_eq!((rows, cols), (1, 3));
}

/// Microsoft's DSUM page example: a tree database and criteria.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn database_functions() {
    let mut sheet = Sheet::new();
    // A1:E7 database, A9:F11 criteria.
    let rows: [[CalcValue; 5]; 7] = [
        [t("Tree"), t("Height"), t("Age"), t("Yield"), t("Profit")],
        [t("Apple"), n(18.0), n(20.0), n(14.0), n(105.0)],
        [t("Pear"), n(12.0), n(12.0), n(10.0), n(96.0)],
        [t("Cherry"), n(13.0), n(14.0), n(9.0), n(105.0)],
        [t("Apple"), n(14.0), n(15.0), n(10.0), n(75.0)],
        [t("Pear"), n(9.0), n(8.0), n(8.0), n(76.8)],
        [t("Apple"), n(8.0), n(9.0), n(6.0), n(45.0)],
    ];
    for (r, row) in rows.iter().enumerate() {
        for (col, value) in row.iter().enumerate() {
            sheet
                .cells
                .insert((r as u32 + 1, col as u32 + 1), value.clone());
        }
    }
    let criteria: [[CalcValue; 2]; 3] = [
        [t("Tree"), t("Height")],
        [t("=Apple"), t(">10")],
        [t("Pear"), CalcValue::empty()],
    ];
    for (r, row) in criteria.iter().enumerate() {
        for (col, value) in row.iter().enumerate() {
            if !value.is_empty() {
                sheet
                    .cells
                    .insert((r as u32 + 9, col as u32 + 1), value.clone());
            }
        }
    }
    let db = sheet.area(1, 1, 7, 5);
    let crit = sheet.area(9, 1, 11, 2);
    let apple_only = sheet.area(9, 1, 10, 2);
    // Apple taller than 10 (two) or any Pear (two).
    exact(
        call_on(
            &mut sheet,
            "DCOUNT",
            vec![db.clone(), t("Age"), crit.clone()],
        ),
        4.0,
    );
    close(
        call_on(
            &mut sheet,
            "DSUM",
            vec![db.clone(), t("Profit"), crit.clone()],
        ),
        352.8,
        1e-15,
    );
    exact(
        call_on(
            &mut sheet,
            "DMAX",
            vec![db.clone(), t("Profit"), crit.clone()],
        ),
        105.0,
    );
    exact(
        call_on(
            &mut sheet,
            "DMIN",
            vec![db.clone(), t("Profit"), crit.clone()],
        ),
        75.0,
    );
    exact(
        call_on(
            &mut sheet,
            "DAVERAGE",
            vec![db.clone(), n(3.0), apple_only.clone()],
        ),
        17.5,
    );
    exact(
        call_on(
            &mut sheet,
            "DSUM",
            vec![db.clone(), t("Profit"), apple_only.clone()],
        ),
        180.0,
    );
    exact(
        call_on(
            &mut sheet,
            "DPRODUCT",
            vec![db.clone(), t("Yield"), apple_only.clone()],
        ),
        140.0,
    );
    error(
        call_on(
            &mut sheet,
            "DGET",
            vec![db.clone(), t("Yield"), apple_only.clone()],
        ),
        Num,
    );
    exact(
        call_on(
            &mut sheet,
            "DCOUNTA",
            vec![db.clone(), t("Profit"), crit.clone()],
        ),
        4.0,
    );
    close(
        call_on(
            &mut sheet,
            "DSTDEV",
            vec![db.clone(), t("Yield"), crit.clone()],
        ),
        (19.0f64 / 3.0).sqrt(),
        1e-12,
    );
    error(
        call_on(&mut sheet, "DSUM", vec![db, t("Nope"), crit]),
        Value,
    );
}

/// Live Excel (16.0.20625, dc-j9k.271-.276): a criteria header that names no field makes the cell a
/// computed criterion (nonzero number or TRUE holds, zero, FALSE, text and error fail).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn database_computed_criteria_and_held_field() {
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![t("A"), n(5.0), n(7.0)]);
    sheet.column(1, 2, vec![t("B"), n(6.0), n(8.0)]);
    sheet.column(1, 7, vec![t("Q"), n(5.0)]);
    let db = sheet.area(1, 1, 3, 2);
    let crit = sheet.area(1, 7, 2, 7);
    for (value, count) in [
        (n(5.0), 2.0), (n(-5.0), 2.0), (n(1e300), 2.0), (b(true), 2.0),
        (n(0.0), 0.0), (b(false), 0.0), (t("Al"), 0.0), (t(">1"), 0.0), (t("=5"), 0.0), (e(NA), 0.0),
    ] {
        sheet.cells.insert((2, 7), value);
        exact(call_on(&mut sheet, "DCOUNT", vec![db.clone(), n(1.0), crit.clone()]), count);
    }
    sheet.cells.insert((2, 7), n(5.0));
    exact(call_on(&mut sheet, "DPRODUCT", vec![db.clone(), n(1.0), crit.clone()]), 35.0);
    error(call_on(&mut sheet, "DGET", vec![db.clone(), n(1.0), crit.clone()]), Num);
    // A numeric header that names no field, a computed criterion alike.
    sheet.cells.insert((1, 7), n(7.0));
    exact(call_on(&mut sheet, "DSUM", vec![db.clone(), n(1.0), crit.clone()]), 12.0);
    // An error cell in a matching record is the result.
    sheet.column(1, 4, vec![t("S"), n(5.0), e(NA)]);
    let db3 = sheet.area(1, 1, 3, 4);
    error(call_on(&mut sheet, "DPRODUCT", vec![db3, t("S"), crit.clone()]), NA);
    // The same error typed in the formula is the result (the formula evaluator maps a held one to #VALUE!).
    for (held, code) in [(e(NA), NA), (e(Name), Name), (e(Div0), Div0)] {
        sheet.cells.insert((1, 7), t("A"));
        error(call_on(&mut sheet, "DPRODUCT", vec![db.clone(), held, crit.clone()]), code);
    }
}

/// dc-y7tqj (rw12 finding 16): a computed criterion cell (header naming no database field or blank)
/// holding a formula re-evaluates per record in Excel; refuse typed instead of collapsing to a
/// fixed truth over all records. Literal computed criteria and formula field criteria still run.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn database_computed_criteria_formula_is_refused() {
    use calc_func::{FunctionFault, value::NumericMode};
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![t("A"), n(5.0), n(7.0)]);
    sheet.column(1, 2, vec![t("B"), n(6.0), n(8.0)]);
    sheet.column(1, 7, vec![t("Check"), b(true)]);
    let db = sheet.area(1, 1, 3, 2);
    let crit = sheet.area(1, 7, 2, 7);
    let crit_cell = CellKey::new(sheet.id, 2, 7).unwrap();
    let call_with_host = |sheet: &mut Sheet, host: &mut Host, name: &str, args: Vec<CalcValue>| {
        let entry = calc_func::catalogue()
            .resolve_surface(calc_func::Notation::Call, name)
            .unwrap();
        let mode = NumericMode::Fast;
        let mut services = calc_func::EvalServices::with_cells(&mode, sheet).with_host(host);
        let mut scratch = calc_func::FunctionCallScratch::new();
        calc_func::invoke(entry, &mut Given(args), &mut services, &mut scratch)
    };
    let mut formula_host = Host {
        sheet: sheet.id,
        formulas: vec![(crit_cell, "=A2>6".into())],
    };
    // Computed criterion holding a formula must refuse rather than match every record.
    for header in [t("Check"), CalcValue::empty()] {
        sheet.cells.insert((1, 7), header);
        let res = call_with_host(&mut sheet, &mut formula_host, "DCOUNT", vec![db.clone(), n(1.0), crit.clone()]);
        assert!(
            matches!(res, Err(FunctionFault::NotImplemented { argument: Some(2), .. })),
            "expected NotImplemented refusal for formula computed criterion, got {res:?}"
        );
    }
    // Same formula cell under a matching field header is a normal field criterion (not computed).
    sheet.cells.insert((1, 7), t("A"));
    sheet.cells.insert((2, 7), t(">6"));
    let field_ok = call_with_host(&mut sheet, &mut formula_host, "DCOUNT", vec![db.clone(), n(1.0), crit.clone()]);
    exact(field_ok, 1.0);
    // Literal computed criterion (not a formula) still runs as live Excel measured (dc-j9k.271).
    let mut literal_host = Host { sheet: sheet.id, formulas: vec![] };
    sheet.cells.insert((1, 7), t("Check"));
    sheet.cells.insert((2, 7), b(true));
    let literal_ok = call_with_host(&mut sheet, &mut literal_host, "DCOUNT", vec![db, n(1.0), crit]);
    exact(literal_ok, 2.0);
}

/// Live Excel (16.0.20625, Touchstone 2026-11-26, dc-j9k.555 and .557): DCOUNT and DCOUNTA count the
/// matching records only when the field is left out; a field that is a blank cell, "" or 0 is #VALUE!.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dcount_field_left_out_counts_but_blank_field_is_value() {
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![n(-1.5), n(3.0), n(-7.0), n(9.0)]);
    sheet.column(1, 3, vec![n(-1.5), n(3.0)]);
    let db = sheet.area(1, 1, 4, 1);
    let crit = sheet.area(1, 3, 2, 3);
    for name in ["DCOUNT", "DCOUNTA"] {
        exact(call_on(&mut sheet, name, vec![db.clone(), missing(), crit.clone()]), 1.0);
        for field in [CalcValue::empty(), t(""), n(0.0)] {
            error(call_on(&mut sheet, name, vec![db.clone(), field, crit.clone()]), Value);
        }
        exact(call_on(&mut sheet, name, vec![db.clone(), n(1.0), crit.clone()]), 1.0);
    }
}

/// Live Excel (16.0.20625, Touchstone 2026-11-26): a numeric field is floored with the date
/// arguments' tolerance. Two columns, the second text: 0.9999999 and 1.9999995 read column 1 (count 1),
/// 1.9999999 and 2.9999996 column 2 (count 0), 0.9999996 is column 0 and 2.9999999 column 3 (#VALUE!).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn database_field_number_uses_the_date_floor_tolerance() {
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![t("h"), n(1.0), n(2.0)]);
    sheet.column(1, 2, vec![t("k"), t("x"), t("y")]);
    sheet.column(1, 4, vec![t("h"), n(1.0)]);
    let db = sheet.area(1, 1, 3, 2);
    let crit = sheet.area(1, 4, 2, 4);
    for (field, count) in [(0.9999999, 1.0), (1.0, 1.0), (1.9999995, 1.0), (1.9999999, 0.0), (2.9999996, 0.0), (1.5, 1.0)] {
        exact(call_on(&mut sheet, "DCOUNT", vec![db.clone(), n(field), crit.clone()]), count);
    }
    for field in [0.9999996, 0.9999989, 0.5, 2.9999999, 3.0] {
        error(call_on(&mut sheet, "DCOUNT", vec![db.clone(), n(field), crit.clone()]), Value);
    }
}

/// Excel 20527: A-plus-digit database criteria are text, not time or currency.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn database_a_digit_criteria() {
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![t("Key"), t("a1"), t("a10"), t("a2"), t("b1"), t("A123"), n(1.0)]);
    sheet.column(1, 2, vec![t("Value"), n(1.0), n(2.0), n(3.0), n(4.0), n(5.0), n(6.0)]);
    sheet.column(9, 1, vec![t("Key"), t("a1")]);
    let db = sheet.area(1, 1, 7, 2);
    let criteria = sheet.area(9, 1, 10, 1);
    for (name, expected) in [("DCOUNT", 3.0), ("DSUM", 8.0)] {
        exact(call_on(&mut sheet, name, vec![db.clone(), t("Value"), criteria.clone()]), expected);
    }
    error(call_on(&mut sheet, "DGET", vec![db.clone(), t("Value"), criteria.clone()]), Num);
    sheet.cells.insert((10, 1), t("<>a2"));
    for (name, expected) in [("DCOUNT", 5.0), ("DSUM", 18.0)] {
        exact(call_on(&mut sheet, name, vec![db.clone(), t("Value"), criteria.clone()]), expected);
    }
    error(call_on(&mut sheet, "DGET", vec![db, t("Value"), criteria]), Num);
}

/// Excel 20527 validates database and criteria reference heights before inspecting the field.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn database_one_row_is_not_a_database() {
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![t("Key"), n(1.0)]);
    sheet.column(1, 2, vec![t("Value"), n(2.0)]);
    sheet.column(5, 1, vec![t("Key"), n(1.0)]);
    let one_cell = sheet.area(1, 1, 1, 1);
    let one_row = sheet.area(1, 1, 1, 2);
    let valid = sheet.area(1, 1, 2, 2);
    let criteria_header = sheet.area(5, 1, 5, 1);
    let criteria = sheet.area(5, 1, 6, 1);
    for name in [
        "DAVERAGE", "DCOUNT", "DCOUNTA", "DGET", "DMAX", "DMIN", "DPRODUCT", "DSTDEV",
        "DSTDEVP", "DSUM", "DVAR", "DVARP",
    ] {
        for db in [one_cell.clone(), one_row.clone()] {
            for field in [n(1.0), t("Key"), e(NA)] {
                error(
                    call_on(&mut sheet, name, vec![db.clone(), field, criteria.clone()]),
                    Value,
                );
            }
        }
        for field in [n(1.0), t("Key"), e(NA)] {
            error(
                call_on(&mut sheet, name, vec![valid.clone(), field, criteria_header.clone()]),
                Value,
            );
        }
    }
    exact(
        call_on(&mut sheet, "DSUM", vec![valid, t("Value"), criteria.clone()]),
        2.0,
    );
    // This area exceeds the dense-grid limit and has only its header held. Its original height,
    // rather than the number of held rows, determines whether it is a valid database.
    sheet.column(1, 3, vec![t("Key")]);
    let large = sheet.area(1, 3, 1_048_576, 7);
    exact(call_on(&mut sheet, "DSUM", vec![large, n(1.0), criteria]), 0.0);
}

/// The single-call surface requires a criteria reference independently of formula lifting.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn database_requires_criteria_reference() {
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![t("Key"), n(1.0)]);
    sheet.column(1, 2, vec![t("Value"), n(7.0)]);
    let database = sheet.area(1, 1, 2, 2);
    let criteria = arr_values(vec![vec![t("Key")], vec![n(1.0)]]);
    error(call_on(&mut sheet, "DSUM", vec![database, n(2.0), criteria]), Value);
}
