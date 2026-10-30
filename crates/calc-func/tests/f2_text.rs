//! F2 (dc-ip6): the text and logical functions ported from OxFunc, and the folded-in rules, called
//! through `calc_func::invoke` as the formula evaluator calls them. Expected values are Excel's
//! (16.0.20430 on this machine, Windows locale en-ZA, ANSI code page 1252, read ad hoc through COM
//! where a row says "Excel:"), Microsoft's documented results, or OxFunc's tests (OxFunc
//! 6fca3518, functions/).

mod f1_support;

use calc_func::value::WorksheetErrorCode::{Blocked, Calc, Connect, Div0, NA, Value};
use f1_support::*;

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
    text(call("REPLACE", vec![t("abcdef"), n(2.0), n(3.0), t("X")]), "aXef");
    text(call("REPLACEB", vec![t("abc"), n(2.0), n(1.0), t("x")]), "axc");
    text(call("SUBSTITUTE", vec![t("a-b-c"), t("-"), t("+")]), "a+b+c");
    text(call("SUBSTITUTE", vec![t("a-b-c"), t("-"), t("+"), n(2.0)]), "a-b+c");
    error(call("SUBSTITUTE", vec![t("a-b-c"), t("-"), t("+"), n(0.0)]), Value);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn joins_and_t() {
    text(call("CONCATENATE", vec![t("a"), n(1.0), b(false)]), "a1FALSE");
    error(call("CONCATENATE", vec![t("a"), e(NA)]), NA);
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![t("x"), t("y")]);
    sheet.column(4, 1, vec![t("z")]);
    let range = sheet.area(1, 1, 4, 1);
    text(call_on(&mut sheet, "CONCAT", vec![range.clone(), t("!")]), "xyz!");
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
    text(call("SWITCH", vec![n(2.0), n(1.0), t("a"), n(2.0), t("b")]), "b");
    text(call("SWITCH", vec![n(3.0), n(1.0), t("a"), t("z")]), "z");
    error(call("SWITCH", vec![n(3.0), n(1.0), t("a")]), NA);
    text(call("SWITCH", vec![t("A"), t("a"), t("hit"), t("miss")]), "hit");
    error(call("SWITCH", vec![e(Div0), n(1.0), t("a")]), Div0);
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
        assert_eq!(call("ISERROR", vec![e(code)]).unwrap().as_logical(), Some(true));
        assert_eq!(call("ISERR", vec![e(code)]).unwrap().as_logical(), Some(true));
        assert_eq!(call("ISNA", vec![e(code)]).unwrap().as_logical(), Some(false));
        assert_eq!(call("ISBLANK", vec![e(code)]).unwrap().as_logical(), Some(false));
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
