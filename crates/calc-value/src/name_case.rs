//! Case-insensitive comparison of Excel names (dc-cpr, ported from OxFml 15daac5
//! `crates/oxfml_core/src/binding/name_case.rs`, W079 lane 7, `fml-kt8.7`).
//!
//! Excel names are case-insensitive for every letter, not only ASCII: a name
//! defined as `Größe` is found by `=GRÖSSE*2`. Until this lane the binder,
//! the evaluator's defined-name lookup, the structured-reference table and
//! column matches and the language service's proposals all compared with
//! `str::eq_ignore_ascii_case` / `to_ascii_uppercase`, so `GRÖSSE` bound to
//! `#NAME?` and even `=TOTAL*2` against a name defined as `Total` bound but
//! then failed at evaluation (`no binding available for defined name TOTAL`).
//!
//! # What Excel does (measured)
//!
//! Excel 16.0 build 20326 through COM on 2026-09-15: `Names.Add` for each
//! definition, then `Range.Formula = "=<reference>*2"` and `Range.Value2`;
//! LET / LAMBDA cases through `Range.Formula2`. Workbook names in the first
//! block were redefined to `=99` by a case-variant `Names.Add` where Excel
//! treated the variant as the same name (count unchanged), so `198` means
//! "resolved to the workbook name", `42` "resolved to the LET parameter".
//!
//! | defined | reference | value | rule taken |
//! |---|---|---|---|
//! | `Total` | `TOTAL`, `total` | 198 | ASCII case-insensitive |
//! | `Größe` | `GRÖSSE`, `GRÖßE`, `größe`, `GRÖẞE` (U+1E9E) | 198 | `ß` = `SS` = `ẞ` |
//! | `Strasse` | `Straße`, `StraẞE`, `STRASSE` | 42 | same, from the other side |
//! | `Μέγεθος` | `ΜΈΓΕΘΟΣ`, `μέγεθοσ`, `μέγεθος` | 198 | final `ς` = `Σ` = `σ` |
//! | `Μέγεθος` | `ΜΕΓΕΘΟΣ` (accent dropped) | `#NAME?` | diacritics significant |
//! | `Данные` | `ДАННЫЕ`, `данные` | 198 | Cyrillic case-insensitive |
//! | `Isik` | `ISIK`, `isik`, `IsIk` | 198 | |
//! | `Isik` | `İsik` (U+0130) | own name (new `Names.Add` accepted) | `İ` ≠ `I` |
//! | `ISIK` | `ıSIK` (U+0131) | own name (new `Names.Add` accepted) | `ı` ≠ `I` |
//! | `Wide` | `Ｗide` (U+FF37) | `#NAME?` | width significant |
//! | `Aether` / `Oeuvre` | `Æther`, `ÆTHER` / `Œuvre` | 42 | collation expansion |
//! | LET `Größe` | `GRÖßE`, `größe` | 42 | parameter matched |
//! | LET `Größe` | `GRÖSSE` | 198 (readback `größe`) | parameter **not** matched, workbook name took it |
//! | LET `Straße` | `StraẞE` | 42 | `ẞ` = `ß` in a parameter |
//! | LET `Straße` | `STRASSE` | readback `Strasse` | workbook name took it |
//! | LET `Æther` | `ÆTHER` / `AETHER` | 42 / readback `Aether` | `Æ` ≠ `AE` in a parameter |
//! | LET `Sigmaς` | `SIGMAΣ`, `sigmaσ` | 42 | |
//! | LET `xı` | `XI` | `#NAME?` | `ı` ≠ `I` |
//! | LET `xİ` | `xi` | `#NAME?` | `İ` ≠ `i` |
//! | table `Umsätze[Größe]` | `UMSÄTZE[GRÖSSE]`, `UMSÄTZE[GRÖßE]`, `umsätze[größe]` | 12 | tables and columns like workbook names |
//! | table `Umsätze[Μέγεθος]` | `[ΜΈΓΕΘΟΣ]`, `[μέγεθοσ]` | 6 | |
//!
//! Excel also rewrites every accepted reference to the defined name's stored
//! spelling (`=total*2` reads back `=TOTAL*2` once the name is `TOTAL`).
//!
//! # Decision
//!
//! Two rules, both Unicode case folding from `CaseFolding.txt` (Unicode
//! 15.0.0, [`crate::case_folding`]), never locale-dependent (no Turkic `T`
//! rows):
//!
//! - **Workbook-scoped names** — defined names, table names, column names —
//!   compare under the *full* fold (`C` + `F`). It reproduces every measured
//!   row above except the collation expansions `Æ` = `AE`, `Œ` = `OE`, which
//!   are not case mappings at all: Excel's workbook-name comparison is a
//!   linguistic ignore-case compare, and the full fold is the case-only part
//!   of it. Those expansions are a known, documented gap.
//! - **LET / LAMBDA parameters** compare under the *simple* fold (`C` + `S`):
//!   one code point to one, so `ß` ≠ `SS` while `ẞ` = `ß` and `ς` = `Σ`,
//!   exactly the parameter rows above.
//!
//! Per-character `to_lowercase` was rejected: it leaves `ς` ≠ `σ`. Per-character
//! `to_uppercase` (the "locale-independent uppercase compare" wording) was
//! rejected: Unicode's simple uppercase maps `ı` to `I`, which Excel does not
//! equate. ASCII behaviour is unchanged by construction — for ASCII both folds
//! are `to_ascii_lowercase`, which is also the fast path every function here
//! takes for ASCII input (the comparisons without allocating).

use crate::case_folding::{push_full_fold, simple_fold};

/// The comparison key of a workbook-scoped name (defined name, table name,
/// column name): the full Unicode case fold of `text`.
#[must_use]
pub fn fold_workbook_name(text: &str) -> String {
    if text.is_ascii() {
        return text.to_ascii_lowercase();
    }
    let mut folded = String::with_capacity(text.len());
    for ch in text.chars() {
        push_full_fold(ch, &mut folded);
    }
    folded
}

/// The comparison key of a LET / LAMBDA parameter name: the simple Unicode
/// case fold of `text`.
#[must_use]
pub fn fold_parameter_name(text: &str) -> String {
    if text.is_ascii() {
        return text.to_ascii_lowercase();
    }
    text.chars().map(simple_fold).collect()
}

/// `true` when `left` and `right` are the same workbook-scoped name under
/// Excel's case-insensitive rule (full case fold).
#[must_use]
pub fn workbook_names_equal(left: &str, right: &str) -> bool {
    if left.is_ascii() && right.is_ascii() {
        return left.eq_ignore_ascii_case(right);
    }
    fold_workbook_name(left) == fold_workbook_name(right)
}

/// `true` when `left` and `right` are the same LET / LAMBDA parameter name
/// (simple case fold).
#[must_use]
pub fn parameter_names_equal(left: &str, right: &str) -> bool {
    if left.is_ascii() && right.is_ascii() {
        return left.eq_ignore_ascii_case(right);
    }
    left.chars()
        .map(simple_fold)
        .eq(right.chars().map(simple_fold))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn ascii_names_compare_exactly_as_eq_ignore_ascii_case() {
        for (left, right) in [
            ("Total", "TOTAL"),
            ("Total", "total"),
            ("IsIk", "isik"),
            ("_x.1", "_X.1"),
            ("Total", "Totals"),
            ("Total", "Tota"),
            ("", "a"),
        ] {
            assert_eq!(
                workbook_names_equal(left, right),
                left.eq_ignore_ascii_case(right),
                "{left} / {right}"
            );
            assert_eq!(
                parameter_names_equal(left, right),
                left.eq_ignore_ascii_case(right),
                "{left} / {right}"
            );
        }
        assert_eq!(fold_workbook_name("Total_1"), "total_1");
        assert_eq!(fold_parameter_name("Total_1"), "total_1");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn ascii_fast_path_agrees_with_the_table_fold() {
        let ascii: String = ('\0'..='\u{7f}').collect();
        let table_full = ascii.chars().fold(String::new(), |mut out, ch| {
            push_full_fold(ch, &mut out);
            out
        });
        let table_simple: String = ascii.chars().map(simple_fold).collect();
        assert_eq!(fold_workbook_name(&ascii), table_full);
        assert_eq!(fold_parameter_name(&ascii), table_simple);
        assert_eq!(fold_workbook_name(&ascii), ascii.to_ascii_lowercase());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn workbook_names_follow_the_measured_full_fold_rows() {
        for variant in ["GRÖSSE", "GRÖßE", "größe", "GRÖ\u{1E9E}E"] {
            assert!(workbook_names_equal("Größe", variant), "{variant}");
        }
        for variant in ["Straße", "Stra\u{1E9E}E", "STRASSE"] {
            assert!(workbook_names_equal("Strasse", variant), "{variant}");
        }
        for variant in ["ΜΈΓΕΘΟΣ", "μέγεθοσ", "μέγεθος"] {
            assert!(workbook_names_equal("Μέγεθος", variant), "{variant}");
        }
        assert!(
            !workbook_names_equal("Μέγεθος", "ΜΕΓΕΘΟΣ"),
            "accent dropped"
        );
        assert!(workbook_names_equal("Данные", "ДАННЫЕ"));
        assert!(workbook_names_equal("Данные", "данные"));
        assert!(workbook_names_equal("Année", "ANNÉE"));
        assert!(!workbook_names_equal("Wide", "\u{FF37}ide"), "width");
        assert!(workbook_names_equal("e\u{0301}tat", "E\u{0301}TAT"));
        assert!(
            !workbook_names_equal("état", "e\u{0301}tat"),
            "no normalisation"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn turkish_i_forms_stay_distinct_in_both_rules() {
        // Excel accepted `İsik` and `ıSIK` as new names next to `Isik` / `ISIK`
        // and answered `#NAME?` for `=LET(xı,21,XI*2)` and `=LET(xİ,21,xi*2)`.
        for (left, right) in [("Isik", "\u{0130}sik"), ("ISIK", "\u{0131}SIK")] {
            assert!(!workbook_names_equal(left, right), "{left} / {right}");
            assert!(!parameter_names_equal(left, right), "{left} / {right}");
        }
        assert!(workbook_names_equal("\u{0130}sik", "\u{0130}SIK"));
        assert!(parameter_names_equal("x\u{0130}", "x\u{0130}"));
        assert!(parameter_names_equal("\u{0131}sik", "\u{0131}SIK"));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn parameter_names_follow_the_measured_simple_fold_rows() {
        assert!(parameter_names_equal("Größe", "GRÖßE"));
        assert!(parameter_names_equal("Größe", "größe"));
        assert!(!parameter_names_equal("Größe", "GRÖSSE"), "ß is not SS");
        assert!(parameter_names_equal("Straße", "Stra\u{1E9E}E"), "ẞ is ß");
        assert!(!parameter_names_equal("Straße", "STRASSE"));
        assert!(parameter_names_equal("Sigmaς", "SIGMAΣ"));
        assert!(parameter_names_equal("Sigmaς", "sigmaσ"));
        assert!(parameter_names_equal("Μέγεθος", "ΜΈΓΕΘΟΣ"));
        assert!(parameter_names_equal("Данные", "ДАННЫЕ"));
        assert!(!parameter_names_equal("Æther", "AETHER"));
        assert!(parameter_names_equal("Æther", "ÆTHER"));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn collation_expansions_are_the_documented_gap() {
        // Excel resolves `=ÆTHER*2` to the workbook name `Aether`; no case
        // fold does, and this helper does not claim to.
        assert!(!workbook_names_equal("Aether", "Æther"));
        assert!(!workbook_names_equal("Oeuvre", "Œuvre"));
    }
}
