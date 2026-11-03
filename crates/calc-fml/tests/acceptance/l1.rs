//! L1 (dc-mzk): arrays and lifting, the reference operators, implicit intersection, spill
//! references and OFFSET, defined names, LET and LAMBDA, through `calc_fml::evaluate`, over the
//! cfml-m1.3 fixtures: [`LoggingCatalog`] with defined names and [`RunReader`] with spills.
//!
//! Ported from OxFml 15daac5's evaluator tests (`crates/oxfml_core/tests/evaluator_tests.rs`,
//! `ftc_1013_case_insensitive_shadowing_tests.rs`) and its operator array fixture
//! (`tests/fixtures/operator_array_arithmetic_cases.json`); the rows marked Excel 16.0 were
//! measured through COM on 2026-10-23.

use super::{LoggingCatalog, RunReader, cell_at};

use calc_fml::reference::{FormulaAnchor, ReferenceNotation};
use calc_fml::{
    CompileError, EvalFault, EvalFrame, EvalScratch, FormulaEvaluationContext, FormulaForm,
    PlanCache, compile_with, evaluate, instantiate, parse_formula,
};
use calc_func::value::{
    CalcArray, CalcValue, CellKey, ExcelText, GridArea, NumericMode, WorksheetErrorCode as W,
};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test;

/// A workbook of two sheets, Sheet1 and Sheet2, over the cfml-m1.3 fixtures, with defined names
/// and spills known.
pub(super) struct Book {
    catalog: LoggingCatalog,
    reader: RunReader,
}

impl Book {
    pub(super) fn new() -> Self {
        let mut catalog = LoggingCatalog::new(&["Sheet1", "Sheet2"]);
        catalog.names = Some(Default::default());
        let reader = RunReader {
            spills: Some(Default::default()),
            ..RunReader::default()
        };
        Book { catalog, reader }
    }

    /// The cell an A1 address names, `Sheet!A1` or `A1` on Sheet1.
    fn at(&self, address: &str) -> CellKey {
        match address.split_once('!') {
            Some((sheet, cell)) => cell_at(self.catalog.id(sheet), cell),
            None => cell_at(self.catalog.id("Sheet1"), address),
        }
    }

    fn area(&self, a: &str, b: &str) -> GridArea {
        GridArea::new(self.at(a), self.at(b)).unwrap()
    }

    pub(super) fn set(&mut self, address: &str, value: CalcValue) -> &mut Self {
        let at = self.at(address);
        self.reader.values.insert(at, value);
        self
    }

    pub(super) fn number(&mut self, address: &str, value: f64) -> &mut Self {
        self.set(address, CalcValue::number(value))
    }

    fn name(&mut self, name: &str, definition: &str) -> &mut Self {
        self.catalog.define(None, name, definition);
        self
    }

    fn local_name(&mut self, sheet: &str, name: &str, definition: &str) -> &mut Self {
        self.catalog.define(Some(sheet), name, definition);
        self
    }

    fn spill(&mut self, anchor: CellKey, area: GridArea) {
        self.reader
            .spills
            .get_or_insert_with(Default::default)
            .insert(anchor, area);
    }

    /// Compiles `formula` at `at` in `form` and evaluates it, with the catalogue as the
    /// formula's workbook context.
    pub(super) fn eval_in(
        &mut self,
        form: FormulaForm,
        formula: &str,
        at: &str,
    ) -> Result<Result<CalcValue, EvalFault>, CompileError> {
        let anchor = FormulaAnchor::new(self.at(at));
        let plan = compile_with(
            &parse_formula(formula),
            ReferenceNotation::A1,
            anchor,
            &self.catalog,
            &mut PlanCache::new(),
            form,
        )?;
        let instance = instantiate(&plan, anchor);
        let mut scratch = EvalScratch::new();
        let mut services = FormulaEvaluationContext::new(
            &NumericMode::Fast,
            &mut self.reader,
            None,
            None,
            calc_fml::ReadRequest::DEPENDENCY,
        )
        .with_catalog(&self.catalog);
        Ok(evaluate(
            EvalFrame::new(&instance, &mut scratch),
            &mut services,
        ))
    }

    /// A dynamic-array formula's value at Sheet1!Z100.
    pub(super) fn dynamic(&mut self, formula: &str) -> CalcValue {
        self.dynamic_at(formula, "Z100")
    }

    pub(super) fn dynamic_at(&mut self, formula: &str, at: &str) -> CalcValue {
        match self.eval_in(FormulaForm::Dynamic, formula, at) {
            Ok(Ok(value)) => value,
            other => panic!("{formula}: {other:?}"),
        }
    }

    /// A legacy formula's value at `at`.
    pub(super) fn legacy_at(&mut self, formula: &str, at: &str) -> CalcValue {
        match self.eval_in(FormulaForm::Legacy, formula, at) {
            Ok(Ok(value)) => value,
            other => panic!("{formula}: {other:?}"),
        }
    }
}

pub(super) fn n(value: f64) -> CalcValue {
    CalcValue::number(value)
}

pub(super) fn t(text: &str) -> CalcValue {
    CalcValue::text(ExcelText::from_interop_assignment(text))
}

pub(super) fn b(value: bool) -> CalcValue {
    CalcValue::logical(value)
}

pub(super) fn e(code: W) -> CalcValue {
    CalcValue::error(code)
}

/// The array of these rows.
pub(super) fn rows(rows: &[&[CalcValue]]) -> CalcValue {
    CalcValue::array(CalcArray::from_rows(rows.iter().map(|row| row.to_vec()).collect()).unwrap())
}

/// The array of these numbers, in rows.
pub(super) fn numbers(values: &[&[f64]]) -> CalcValue {
    rows(
        &values
            .iter()
            .map(|row| row.iter().map(|v| n(*v)).collect::<Vec<_>>())
            .collect::<Vec<_>>()
            .iter()
            .map(Vec::as_slice)
            .collect::<Vec<_>>(),
    )
}

/// L1 (dc-mzk): array constants, lifting over arrays and ranges, and array results, through
/// `calc_fml::evaluate`.
///
/// The operator rows are OxFml's `operator_array_arithmetic_cases.json` (OxFml 15daac5,
/// `crates/oxfml_core/tests/fixtures`) and its evaluator tests
/// (`evaluator_lifts_*`, `evaluator_ifs_array_condition_*`, `evaluator_choose_array_index_*`);
/// the legacy rows are Excel 16.0's, measured through COM on 2026-10-23 (`Range.Formula` entry
/// over A1:A3 = 1, 2, 3 and B1:B3 = 10, 20, 30; see the `legacy` module of calc-fml).
mod arrays {
    use super::*;

    fn data() -> Book {
        let mut book = Book::new();
        for (row, value) in [(1, 1.0), (2, 2.0), (3, 3.0)] {
            book.number(&format!("A{row}"), value);
            book.number(&format!("B{row}"), value * 10.0);
        }
        book
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn array_constants_are_values() {
        let mut book = Book::new();
        assert_eq!(
            book.dynamic("={1,2;3,4}"),
            numbers(&[&[1.0, 2.0], &[3.0, 4.0]])
        );
        assert_eq!(
            book.dynamic("={1,\"a\";TRUE,#N/A}"),
            rows(&[&[n(1.0), t("a")], &[b(true), e(W::NA)]])
        );
        assert_eq!(book.dynamic("={-1,+2,-0}"), numbers(&[&[-1.0, 2.0, 0.0]]));
        assert_eq!(book.dynamic("=SUM({1,2;3,4})"), n(10.0));
        assert_eq!(book.dynamic("=ROWS({1;2;3})"), n(3.0));
        // Rows of different lengths, and a reference or an expression inside, are no array constant.
        for refused in ["={1,2;3}", "={1+1}", "={A1}"] {
            assert!(
                matches!(
                    book.eval_in(FormulaForm::Dynamic, refused, "Z100"),
                    Err(CompileError::Syntax { .. } | CompileError::Reference { .. })
                ),
                "{refused}"
            );
        }
    }

    /// Excel's full spill results, from OxFunc-Evidence's recorded Formula2 calls:
    /// w111-broad-typed-20260929-002 / w111typed-00286-textsplit-arg1-row-array and
    /// w092-array-successor-max3x9-all-cycle-001 / w090-successor-00187-munit-arg1.
    /// A lifted call contributes its result's first cell, not a nested spill.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn lifted_array_results_contribute_their_first_cell() {
        let mut book = Book::new();
        assert_eq!(
            book.dynamic("=TEXTSPLIT(CHOOSE({1,2},\"a,b,c\",\"a,b,c\"),\",\")"),
            rows(&[&[t("a"), t("a")]])
        );
        assert_eq!(book.dynamic("=MUNIT({3,3})"), numbers(&[&[1.0, 1.0]]));
    }

    /// OxFunc-Evidence's w111-mround-reference-controls-run-typed-20260929 and
    /// w111-quotient-reference-controls-run-typed-20260929 record these Formula2 calls at
    /// J1:J4 and J20. A caller aligned with the range still gets #VALUE!; unary plus
    /// materializes an array, which lifts. INDEX and IF preserve the rejected reference.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn binary_numeric_calls_keep_reference_origin() {
        let mut book = Book::new();
        book.number("B1", 5.0)
            .number("B2", 7.0)
            .number("B3", 9.0)
            .number("A100", 2.0);
        for (function, expected) in [
            ("MROUND", numbers(&[&[6.0], &[8.0], &[10.0]])),
            ("QUOTIENT", numbers(&[&[2.0], &[3.0], &[4.0]])),
        ] {
            for at in ["J1", "J2", "J3", "J4", "J20"] {
                assert_eq!(
                    book.dynamic_at(&format!("={function}(B1:B3,A100)"), at),
                    e(W::Value),
                    "{function} at {at}"
                );
                assert_eq!(
                    book.dynamic_at(&format!("={function}(+B1:B3,A100)"), at),
                    expected,
                    "materialized {function} at {at}"
                );
            }
            for reference in ["INDEX(B1:B3,0)", "IF(TRUE,B1:B3,B1:B3)"] {
                assert_eq!(
                    book.dynamic_at(&format!("={function}({reference},A100)"), "J2"),
                    e(W::Value)
                );
            }
        }
    }

    /// Recorded MROUND/QUOTIENT typed controls (OxFunc-Evidence, W111, 2026-09-29): logicals
    /// are #VALUE!, explicit omissions #N/A. Padding remains an error at its own argument
    /// position, so an earlier error wins (MROUND's prepared-value impact assessment).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn binary_numeric_argument_errors_keep_their_order() {
        let mut book = Book::new();
        for formula in ["=MROUND(7,TRUE)", "=QUOTIENT(TRUE,2)"] {
            assert_eq!(book.dynamic(formula), e(W::Value), "{formula}");
        }
        for formula in ["=MROUND(7,)", "=QUOTIENT(,2)"] {
            assert_eq!(book.dynamic(formula), e(W::NA), "{formula}");
        }
        assert_eq!(
            book.dynamic("=MROUND({7,7,#REF!},{2,#NUM!})"),
            rows(&[&[n(8.0), e(W::Num), e(W::Ref)]])
        );
        // w111quotientpaddingheldout-00001-quotient-asymmetric-0-0-1.
        book.number("A100", f64::from_bits(0x409307e000000000));
        assert_eq!(
            book.dynamic("=QUOTIENT(CHOOSE({1,2,3,4,5},#N/A,#DIV/0!,#REF!,A100,TRUE),CHOOSE({1,2},#N/A,FALSE))"),
            rows(&[&[e(W::NA), e(W::Div0), e(W::Ref), e(W::NA), e(W::Value)]])
        );
    }

    /// OxFunc-Evidence w089-axis-witness-20260501-002 records this LET result as two #VALUE!
    /// cells. COUNTBLANK takes a reference whole and lifts over a computed array's cells.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn countblank_lifts_over_computed_arrays() {
        let mut book = Book::new();
        assert_eq!(
            book.dynamic("=LET(d,{\"\";1},COUNTBLANK(d))"),
            rows(&[&[e(W::Value)], &[e(W::Value)]])
        );
        // w111countholdout-01525-countblank-array-5.
        book.number("A100", 0.0);
        assert_eq!(
            book.dynamic("=COUNTBLANK(CHOOSE({1,2,3},\" \",A100,#REF!))"),
            rows(&[&[e(W::Value), e(W::Value), e(W::Ref)]])
        );
    }

    /// Recorded W111 conditional cases: IFERROR's 00222 mixed-shape case in the first
    /// heldout run, and IFNA's 00602 shape case in the surface run (2026-09-29).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn conditional_fallbacks_select_each_array_position() {
        let mut book = Book::new();
        assert_eq!(
            book.dynamic("=IFERROR(CHOOSE({1;2},\"fAlSe\",#NUM!),CHOOSE({1;2;3;4},\"FALSE \",\" TRUE\",\"tRuE\",#NUM!))"),
            rows(&[&[t("fAlSe")], &[t(" TRUE")], &[t("tRuE")], &[e(W::Num)]])
        );
        book.number("A100", 11.0)
            .number("A101", 12.0)
            .number("A102", 13.0);
        assert_eq!(
            book.dynamic("=IFNA(#N/A,CHOOSE({1,2,3},A100,A101,A102))"),
            numbers(&[&[11.0, 12.0, 13.0]])
        );
    }

    /// w111typed-00008-if-fresh-mixed-shape in the first heldout conditional run: an
    /// invalid condition stays #VALUE! even where an unselected branch has no coordinate.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn conditional_errors_precede_unselected_array_gaps() {
        let mut book = Book::new();
        book.number("A100", -2.0);
        assert_eq!(
            book.dynamic("=IF(CHOOSE({1,2,3,4},TRUE,FALSE,A100,\"\"),CHOOSE({1,2},#REF!,FALSE),CHOOSE({1},\"tRuE\"))"),
            rows(&[&[e(W::Ref), t("tRuE"), e(W::NA), e(W::Value)]])
        );
    }

    /// W111 conditional heldout cases 00227 (IFERROR) and 00010 (IF) record zero for a
    /// selected blank, including a blank that CHOOSE's omitted argument contributes.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn conditional_selected_blanks_are_zero() {
        let mut book = Book::new();
        assert_eq!(
            book.dynamic(
                "=IFERROR(CHOOSE({1,2,3;4,5,6},#REF!,,\" TRUE\",\"FALSE \",\"0\",#REF!),\"FALSE\")"
            ),
            rows(&[
                &[t("FALSE"), n(0.0), t(" TRUE")],
                &[t("FALSE "), t("0"), t("FALSE")]
            ])
        );
        book.set("F1", CalcValue::empty())
            .set("K1", t("tRuE"))
            .set("L1", e(W::Num))
            .number("M1", -2.0)
            .set("N1", e(W::Num));
        assert_eq!(
            book.dynamic("=IF(CHOOSE({1,2},#NULL!,\"tRuE\"),F1:F1,K1:N1)"),
            rows(&[&[e(W::Null), n(0.0), e(W::NA), e(W::NA)]])
        );
    }

    /// dc-ibp2 error_fallback!D38,D44,D45: scalar blanks remain empty text on
    /// concatenation, even though the W111 lifted blank positions become zero.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn conditional_scalar_fallback_blanks_concatenate_as_empty() {
        let mut book = Book::new();
        for formula in [
            "=IFERROR(A4,1)&\"|\"",
            "=IFERROR(#N/A,)&\"|\"",
            "=IFERROR(,1)&\"|\"",
        ] {
            assert_eq!(book.dynamic(formula), t("|"), "{formula}");
        }
    }

    /// Recorded w090-successor-00117-switch-arg3 and -00123-ifs-arg2: unused result
    /// arrays do not enlarge a scalar selection's shape.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn conditional_scalar_selections_ignore_unused_array_shapes() {
        let mut book = Book::new();
        assert_eq!(
            book.dynamic("=SWITCH(2,1,{\"a\",\"a\"},2,\"b\",\"other\")"),
            t("b")
        );
        assert_eq!(book.dynamic("=IFS(FALSE,{1,1},TRUE,2)"), n(2.0));
    }

    /// w090-successor-00183-switch-arg4: the first scalar case matches before
    /// an unused later array case, so that case does not enlarge the selection.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn conditional_unused_switch_cases_do_not_enlarge_a_scalar_match() {
        let mut book = Book::new();
        assert_eq!(
            book.dynamic("=SWITCH(\"A\",\"a\",1,{\"A\",\"A\"},2)"),
            n(1.0)
        );
    }

    /// w090-successor-00179-ifs-arg2 records both error cells: an invalid scalar
    /// condition retains the shape contributed by its array result.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn conditional_invalid_scalar_condition_keeps_the_array_shape() {
        let mut book = Book::new();
        assert_eq!(
            book.dynamic("=IFS(\"2\",{\"hit\",\"hit\"})"),
            rows(&[&[e(W::Value), e(W::Value)]])
        );
    }

    /// W111 typed cases 00688/689, 00742/743, 00748/749 and 00754/755: a scalar source error
    /// remains scalar, instead of becoming one cell of a padded result.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn array_functions_preserve_scalar_source_errors() {
        let mut book = Book::new();
        book.number("A100", 8.0).number("A101", 9.0);
        for (error, code) in [("#N/A", W::NA), ("#VALUE!", W::Value)] {
            for formula in [
                format!("=HSTACK({error},CHOOSE({{1;2}},A100,A101))"),
                format!("=VSTACK({error},CHOOSE({{1,2}},A100,A101))"),
                format!("=EXPAND({error},3,3,\"pad\")"),
                format!("=DROP({error},1)"),
            ] {
                assert_eq!(book.dynamic(&formula), e(code), "{formula}");
            }
        }
    }

    /// w090-successor-00071-expand-arg4 records #VALUE! for an array pad.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn expand_padding_array_is_one_argument() {
        let mut book = Book::new();
        assert_eq!(
            book.dynamic("=EXPAND({1,2;3,4},3,4,{\"x\",\"x\"})"),
            e(W::Value)
        );
        // Keep the existing admission of a scalar cell pad while changing the array role.
        book.number("A100", 7.0);
        assert_eq!(
            book.dynamic("=EXPAND({1,2},2,3,A100)"),
            numbers(&[&[1.0, 2.0, 7.0], &[7.0, 7.0, 7.0]])
        );
    }

    /// W111 typed cases 00766-00770: scalar WRAPCOLS vectors retain their value
    /// and do not acquire the padding rows that an actual array acquires.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn wrapcols_preserves_scalar_vectors() {
        let mut book = Book::new();
        book.number("A100", 2.0);
        for (value, expected) in [
            ("#N/A", e(W::NA)),
            ("#VALUE!", e(W::Value)),
            ("\"\"", t("")),
            ("\"2\"", t("2")),
            ("TRUE", CalcValue::logical(true)),
        ] {
            let formula = format!("=WRAPCOLS({value},A100,\"pad\")");
            assert_eq!(book.dynamic(&formula), expected, "{formula}");
        }
    }

    /// unswept-00303-isomitted-arg0_array_lift records one FALSE for the argument.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn isomitted_does_not_lift_over_an_array() {
        let mut book = Book::new();
        assert_eq!(book.dynamic("=ISOMITTED({2;3})"), CalcValue::logical(false));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn operators_lift_over_arrays_as_oxfml_fixture_rows() {
        let mut book = Book::new();
        let cases: [(&str, _); 8] = [
            (
                "={1,2,3;2,3,4}*-1",
                numbers(&[&[-1.0, -2.0, -3.0], &[-2.0, -3.0, -4.0]]),
            ),
            ("=2*{1,2;3,4}", numbers(&[&[2.0, 4.0], &[6.0, 8.0]])),
            ("=10-{1,2;3,4}", numbers(&[&[9.0, 8.0], &[7.0, 6.0]])),
            (
                "={1,2;3,4}+{10,20;30,40}",
                numbers(&[&[11.0, 22.0], &[33.0, 44.0]]),
            ),
            (
                "={8,6;4,2}/{2,0;1,2}",
                rows(&[&[n(4.0), e(W::Div0)], &[n(4.0), n(1.0)]]),
            ),
            (
                "={1,2;3,4}+{10,20,30}",
                rows(&[&[n(11.0), n(22.0), e(W::NA)], &[n(13.0), n(24.0), e(W::NA)]]),
            ),
            (
                "={\"2\",3;\"4\",\"5\"}+1",
                numbers(&[&[3.0, 4.0], &[5.0, 6.0]]),
            ),
            ("={1,\"bad\"}+1", rows(&[&[n(2.0), e(W::Value)]])),
        ];
        for (formula, expected) in cases {
            assert_eq!(book.dynamic(formula), expected, "{formula}");
        }
        assert_eq!(
            book.dynamic("=-{1,2,3;2,3,4}"),
            numbers(&[&[-1.0, -2.0, -3.0], &[-2.0, -3.0, -4.0]])
        );
        assert_eq!(
            book.dynamic("={1;2}&\"x\""),
            rows(&[&[t("1x")], &[t("2x")]])
        );
        assert_eq!(
            book.dynamic("={1,2,3}>=2"),
            rows(&[&[b(false), b(true), b(true)]])
        );
        assert_eq!(book.dynamic("={1,2}%"), numbers(&[&[0.01, 0.02]]));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn functions_of_single_values_lift() {
        let mut book = Book::new();
        assert_eq!(
            book.dynamic("=ROUND({1.25,2.75},1)"),
            numbers(&[&[1.3, 2.8]])
        );
        assert_eq!(
            book.dynamic("=ROUND(2.345,{0;1;2})"),
            numbers(&[&[2.0], &[2.3], &[2.35]])
        );
        assert_eq!(
            book.dynamic("=ISNUMBER({1,\"a\"})"),
            rows(&[&[b(true), b(false)]])
        );
        assert_eq!(
            book.dynamic("=NOT({TRUE,FALSE})"),
            rows(&[&[b(false), b(true)]])
        );
        // An aggregate takes the array whole.
        assert_eq!(book.dynamic("=SUM({1,2,3}*2)"), n(12.0));
        assert_eq!(book.dynamic("=SUMPRODUCT({1,2},{3,4})"), n(11.0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn lazy_selectors_lift_over_an_array_that_decides() {
        let mut book = Book::new();
        assert_eq!(
            book.dynamic("=IF({TRUE,FALSE},1,2)"),
            numbers(&[&[1.0, 2.0]])
        );
        assert_eq!(
            book.dynamic("=IF({1,0,1},{10,20,30},\"n\")"),
            rows(&[&[n(10.0), t("n"), n(30.0)]])
        );
        assert_eq!(
            book.dynamic("=CHOOSE({1,2},\"a\",\"b\")"),
            rows(&[&[t("a"), t("b")]])
        );
        assert_eq!(
            book.dynamic("=IFERROR({1,#N/A,3},0)"),
            numbers(&[&[1.0, 0.0, 3.0]])
        );
        assert_eq!(
            book.dynamic("=IFNA({#N/A,#DIV/0!},\"x\")"),
            rows(&[&[t("x"), e(W::Div0)]])
        );
        // A scalar decision stays lazy: the untaken branch does not run.
        assert_eq!(book.dynamic("=IF(TRUE,1,1/0)"), n(1.0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn dynamic_formulas_lift_over_ranges_and_spill() {
        let mut book = data();
        assert_eq!(book.dynamic("=A1:A3*2"), numbers(&[&[2.0], &[4.0], &[6.0]]));
        assert_eq!(
            book.dynamic("=A1:A3+B1:B3"),
            numbers(&[&[11.0], &[22.0], &[33.0]])
        );
        assert_eq!(book.dynamic("=SUM(A1:A3*B1:B3)"), n(140.0));
        assert_eq!(
            book.dynamic("=A1:B2"),
            numbers(&[&[1.0, 10.0], &[2.0, 20.0]])
        );
        assert_eq!(book.dynamic("=SUM(IF(A1:A3>1,1,0))"), n(2.0));
        assert_eq!(
            book.dynamic("=A1:A4"),
            rows(&[&[n(1.0)], &[n(2.0)], &[n(3.0)], &[CalcValue::empty()]])
        );
    }

    /// W111 independent MOD padding case 00008: the third dividend's #VALUE!
    /// precedes the divisor's absent third coordinate.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn eager_padding_keeps_earlier_argument_errors() {
        let mut book = Book::new();
        book.number("A100", f64::from_bits(0x4027b5c108bb5758))
            .number("A101", f64::from_bits(0xc02c90d8b901f086))
            .number("A102", f64::from_bits(0x402b94685e64e5f0));
        assert_eq!(
            book.dynamic("=MOD(CHOOSE({1;2;3},A100,A101,#VALUE!),CHOOSE({1;2},#NAME?,A102))"),
            rows(&[
                &[e(W::Name)],
                &[n(f64::from_bits(0x402a97f803c7db5a))],
                &[e(W::Value)]
            ])
        );
    }

    /// W111 text discovery cases 01616 and 01720 preserve the earlier text error
    /// when a later argument has no element at that position.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn eager_text_padding_keeps_argument_order() {
        let mut book = Book::new();
        book.number("A100", 123.0)
            .number("A101", -0.5)
            .number("A102", 1.0)
            .number("A103", 1.0)
            .number("A104", 2.0);
        assert_eq!(
            book.dynamic("=MID(CHOOSE({1,2,3;4,5,6},#N/A,#DIV/0!,#DIV/0!,FALSE,#N/A,A100),CHOOSE({1,2;3,4},#REF!,\"2\",A101,A102),CHOOSE({1,2,3},\"2\",A103,A104))"),
            rows(&[
                &[e(W::NA), e(W::Div0), e(W::Div0)],
                &[e(W::Value), e(W::NA), e(W::NA)]
            ])
        );
        book.number("A101", 123.0).number("A102", 123.0);
        assert_eq!(
            book.dynamic("=EXACT(CHOOSE({1,2;3,4;5,6},\"abc\",\"abc\",\"\",A100,\"\",#DIV/0!),CHOOSE({1,2;3,4},A101,A102,\"\",\"\"))"),
            rows(&[
                &[CalcValue::logical(false), CalcValue::logical(false)],
                &[CalcValue::logical(true), CalcValue::logical(false)],
                &[e(W::NA), e(W::Div0)]
            ])
        );
    }

    /// W111 depreciation padding case 00000: a missing salvage coordinate does
    /// not hide the earlier cost error. The scalar depreciation kernel is unchanged.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn eager_three_argument_padding_keeps_argument_order() {
        let mut book = Book::new();
        for cell in ["A100", "A101", "A102", "A103"] {
            book.number(cell, 1000.0);
        }
        for cell in ["A104", "A105", "A106", "A107"] {
            book.number(cell, 100.0);
        }
        book.number("A108", 10.0);
        assert_eq!(
            book.dynamic("=SLN(CHOOSE({1,2,3;4,5,6},A100,A101,#REF!,A102,A103,#REF!),CHOOSE({1,2;3,4},A104,A105,A106,A107),A108)"),
            rows(&[
                &[n(90.0), n(90.0), e(W::Ref)],
                &[n(90.0), n(90.0), e(W::Ref)]
            ])
        );
    }

    /// Independent W111 depreciation padding cases 00209/00334/00459 keep
    /// #N/A before a later optional argument's error. Preserve these controls
    /// while extending argument-order padding to the other measured functions.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn eager_padding_preserves_optional_argument_controls() {
        let mut book = Book::new();
        for cell in ["A100", "A101", "A102", "A103"] {
            book.number(cell, f64::from_bits(0x40957c0000000000));
        }
        book.number("A104", 125.0)
            .number("A105", 9.75)
            .number("A106", 3.125);
        for (function, optional, expected) in [
            ("DB", 4.5, 0x406b2be01a63a02a),
            ("DDB", 1.5, 0x40628a73e06e783b),
        ] {
            for cell in ["A107", "A108", "A109", "A110"] {
                book.number(cell, optional);
            }
            let formula = format!(
                "={function}(CHOOSE({{1,2;3,4}},A100,A101,A102,A103),A104,A105,A106,CHOOSE({{1,2,3;4,5,6}},A107,A108,#NULL!,A109,A110,#NULL!))"
            );
            let result = n(f64::from_bits(expected));
            assert_eq!(
                book.dynamic(&formula),
                rows(&[
                    &[result.clone(), result.clone(), e(W::NA)],
                    &[result.clone(), result, e(W::NA)]
                ]),
                "{formula}"
            );
        }
        book.number("A106", 1.125)
            .number("A107", 3.375)
            .number("A108", 1.5);
        let result = n(f64::from_bits(0x40764227a2b5d435));
        assert_eq!(
            book.dynamic("=VDB(CHOOSE({1,2;3,4},A100,A101,A102,A103),A104,A105,A106,A107,A108,CHOOSE({1,2,3;4,5,6},TRUE,TRUE,#NULL!,TRUE,TRUE,#NULL!))"),
            rows(&[
                &[result.clone(), result.clone(), e(W::NA)],
                &[result.clone(), result, e(W::NA)]
            ])
        );
    }

    /// W111 depreciation padding cases 00030/00045/00060: an already formed
    /// cost error precedes a later padded salvage coordinate, even for functions
    /// whose optional argument validation otherwise precedes their kernel call.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn eager_padding_keeps_formed_prefix_errors() {
        let mut book = Book::new();
        for cell in ["A100", "A101", "A102", "A103"] {
            book.number(cell, 1000.0);
        }
        for cell in ["A104", "A105", "A106", "A107"] {
            book.number(cell, 100.0);
        }
        book.number("A108", 10.0).number("A109", 2.0);
        for (function, optional, expected) in [
            ("DB", 6.0, 0x4067190624dd2f1a),
            ("DDB", 2.0, 0x4064000000000000),
        ] {
            book.number("A110", optional);
            let formula = format!(
                "={function}(CHOOSE({{1,2,3;4,5,6}},A100,A101,#REF!,A102,A103,#REF!),CHOOSE({{1,2;3,4}},A104,A105,A106,A107),A108,A109,A110)"
            );
            let result = n(f64::from_bits(expected));
            assert_eq!(
                book.dynamic(&formula),
                rows(&[
                    &[result.clone(), result.clone(), e(W::Ref)],
                    &[result.clone(), result, e(W::Ref)]
                ]),
                "{formula}"
            );
        }
        book.number("A109", 1.0)
            .number("A110", 2.0)
            .number("A111", 2.0);
        assert_eq!(
            book.dynamic("=VDB(CHOOSE({1,2,3;4,5,6},A100,A101,#REF!,A102,A103,#REF!),CHOOSE({1,2;3,4},A104,A105,A106,A107),A108,A109,A110,A111,TRUE)"),
            rows(&[
                &[n(160.0), n(160.0), e(W::Ref)],
                &[n(160.0), n(160.0), e(W::Ref)]
            ])
        );
    }

    /// W111 prepared rounding cases 00424/00661 record padded #N/A before
    /// coercion of the earlier text "(1.5)". No rounding algorithm changes here.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn eager_padding_preserves_rounding_coercion_controls() {
        let mut book = Book::new();
        for (cell, value) in [
            ("B200", 0xc068880000000000),
            ("C200", 0xc085f40000000000),
            ("B201", 0xc069f40000000000),
            ("C201", 0xc0be4f0000000000),
            ("A100", 0x401e000000000000),
        ] {
            book.number(cell, f64::from_bits(value));
        }
        book.set("D200", t("(1.5)")).set("D201", t("(1.5)"));
        assert_eq!(
            book.dynamic("=ROUNDDOWN(B200:D201,CHOOSE({1,2},A100,#DIV/0!))"),
            rows(&[
                &[n(f64::from_bits(0xc068880000000000)), e(W::Div0), e(W::NA)],
                &[n(f64::from_bits(0xc069f40000000000)), e(W::Div0), e(W::NA)]
            ])
        );
        book.set("B200", CalcValue::empty())
            .number("C200", f64::from_bits(0xc0a0a00000000000))
            .number("D200", f64::from_bits(0x4089b80000000000))
            .number("B201", f64::from_bits(0x4086f00000000000))
            .number("C201", f64::from_bits(0xc081390000000000))
            .set("G200", CalcValue::empty())
            .number("H200", -2.0);
        assert_eq!(
            book.dynamic("=ROUNDUP(B200:D201,G200:H200)"),
            rows(&[
                &[n(0.0), n(f64::from_bits(0xc0a1300000000000)), e(W::NA)],
                &[
                    n(f64::from_bits(0x4086f00000000000)),
                    n(f64::from_bits(0xc082c00000000000)),
                    e(W::NA)
                ]
            ])
        );
    }

    /// The legacy rows measured in Excel 16.0 (see the module documentation), each entered in row 2
    /// unless it says otherwise.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn legacy_formulas_intersect_where_excel_writes_at() {
        let mut book = data();
        let cases: [(&str, &str, _); 18] = [
            ("=A1:A3*1", "I2", n(2.0)),
            ("={1,2}*2", "I2", n(2.0)),
            ("=ABS({-1,-2})", "I4", n(1.0)),
            ("={4,5,6}", "I6", n(4.0)),
            ("=IFERROR(A1:A3,0)", "I2", n(2.0)),
            ("=A1:A3", "I3", n(3.0)),
            ("=A1:A3", "I5", e(W::Value)),
            ("=SUM(A1:A3*2)", "I2", n(4.0)),
            ("=SUM(ABS(A1:A3))", "I2", n(2.0)),
            ("=SUM({1,2}*2)", "I2", n(6.0)),
            ("=SUM(ABS({-1,-2}))", "I2", n(3.0)),
            ("=SUM({1,2}*A1:A2)", "I2", n(6.0)),
            ("=SUMPRODUCT(A1:A3*2)", "I2", n(12.0)),
            ("=SUMPRODUCT((A1:A3>1)*B1:B3)", "I2", n(50.0)),
            ("=SUMPRODUCT(ABS(A1:A3))", "I2", n(6.0)),
            ("=SUMPRODUCT(IFERROR(A1:A3,0))", "I2", n(2.0)),
            ("=SUM(IF(TRUE,A1:A3))", "I2", n(6.0)),
            ("=SUM(IFERROR(A1:A3,0))", "I2", n(2.0)),
        ];
        for (formula, at, expected) in cases {
            assert_eq!(book.legacy_at(formula, at), expected, "{formula} in {at}");
        }
        assert_eq!(book.legacy_at("=IF(A1:A3>1,1,0)", "I7"), e(W::Value));
        assert_eq!(book.legacy_at("=SUMPRODUCT(IF(A1:A3>1,1,0))", "I2"), n(1.0));
        assert_eq!(book.legacy_at("=COUNT(A1:A3*1)", "I2"), n(1.0));
        assert_eq!(book.legacy_at("=MAX(A1:A3*1)", "I2"), n(2.0));
        assert_eq!(
            book.legacy_at("=LOOKUP(2,1/(A1:A3=2),B1:B3)", "I2"),
            n(20.0)
        );
    }

    /// Where the measurements do not say what a legacy formula does, a range at a single-value place
    /// is refused, never guessed.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn legacy_formulas_refuse_unmeasured_places() {
        let mut book = data();
        // XLOOKUP's lookup array and CHISQ.TEST's ranges, which this row used before, were
        // measured by L3 (dc-m3u) and dc-10y and lift; COUNTIFS's range, which takes only a
        // reference, is not measured (Excel refuses to enter the formula). COUNTIF's lifts an
        // array of references (dc-b1l).
        let outcome = book
            .eval_in(FormulaForm::Legacy, "=COUNTIFS(A1:A3*1,1)", "I2")
            .unwrap();
        assert!(
            matches!(outcome, Err(EvalFault::AreaAsValue { .. })),
            "{outcome:?}"
        );
    }
}

/// L1 (dc-mzk): the range, union and intersection operators, implicit intersection (`@`), spill
/// references (`#`) and OFFSET, through `calc_fml::evaluate`.
///
/// Ported in kind from OxFml's evaluator tests (`evaluator_dispatches_range_and_intersection_*`,
/// `evaluator_runs_indirect_offset_and_iferror`, OxFml 15daac5), which ran without a grid; the
/// values here are Excel 16.0's, measured through COM on 2026-10-23 where a row says so.
mod references {
    use super::*;

    fn grid() -> Book {
        let mut book = Book::new();
        for row in 1..=10u32 {
            for (col, letter) in ["A", "B", "C", "D", "E"].iter().enumerate() {
                book.number(
                    &format!("{letter}{row}"),
                    f64::from(100 * row) + col as f64 + 1.0,
                );
            }
        }
        book
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn the_range_operator_spans_its_operands() {
        let mut book = grid();
        // A1:B2:C4 is A1:C4 (Excel: =ROWS(A1:B2:C4) is 4).
        assert_eq!(book.dynamic("=ROWS(A1:B2:C4)"), n(4.0));
        assert_eq!(book.dynamic("=COLUMNS(A1:B2:C4)"), n(3.0));
        assert_eq!(
            book.dynamic("=SUM((A1):(B2))"),
            n(101.0 + 102.0 + 201.0 + 202.0)
        );
        assert_eq!(
            book.dynamic("=SUM(A1:INDEX(A1:A10,3))"),
            n(101.0 + 201.0 + 301.0)
        );
        assert_eq!(book.dynamic("=ROWS(Sheet1!A1:Sheet1!A4)"), n(4.0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn the_intersection_operator_takes_shared_cells() {
        let mut book = grid();
        assert_eq!(
            book.dynamic("=A1:C3 B2:D4"),
            numbers(&[&[202.0, 203.0], &[302.0, 303.0]])
        );
        assert_eq!(
            book.dynamic("=SUM(A1:C3 B2:D4)"),
            n(202.0 + 203.0 + 302.0 + 303.0)
        );
        // Excel: =SUM((A1:A2 B1:B2)) is #NULL!; areas on two sheets are #VALUE!.
        assert_eq!(book.dynamic("=SUM((A1:A2 B1:B2))"), e(W::Null));
        assert_eq!(
            book.dynamic("=SUM((Sheet1!A1:A2 Sheet2!A1:A2))"),
            e(W::Value)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn the_union_operator_joins_areas() {
        let mut book = grid();
        assert_eq!(book.dynamic("=SUM((A1:A2,C3))"), n(101.0 + 201.0 + 303.0));
        assert_eq!(book.dynamic("=COUNT((A1:A2,C3,A1))"), n(4.0));
        // Excel: =SUM((S1!A1,S2!A1)) is #VALUE!.
        assert_eq!(book.dynamic("=SUM((Sheet1!A1,Sheet2!A1))"), e(W::Value));
        // A union where one value is needed is #VALUE!.
        assert_eq!(book.dynamic("=ABS((A1,B1))"), e(W::Value));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn implicit_intersection_takes_one_value() {
        let mut book = grid();
        // Excel: =@S1!C1:E10 entered on another sheet in D5 is S1!D5; in F5 and D11 #VALUE!.
        assert_eq!(book.dynamic_at("=@Sheet1!C1:E10", "Sheet2!D5"), n(504.0));
        assert_eq!(book.dynamic_at("=@Sheet1!C1:E10", "Sheet2!F5"), e(W::Value));
        assert_eq!(
            book.dynamic_at("=@Sheet1!C1:E10", "Sheet2!D11"),
            e(W::Value)
        );
        // A column takes the formula's row; a blank cell is 0 (Excel: =@S1!A1:A10 over a blank).
        assert_eq!(book.dynamic_at("=@Sheet1!B1:B10", "Sheet2!Z7"), n(702.0));
        assert_eq!(book.dynamic_at("=@Sheet1!F1:F10", "Sheet2!A5"), n(0.0));
        // Excel: =@{7,8} is 7; the file form _xlfn.SINGLE is the same operator.
        assert_eq!(book.dynamic("=@{7,8}"), n(7.0));
        assert_eq!(
            book.dynamic_at("=_xlfn.SINGLE(Sheet1!C1:E10)", "Sheet2!D5"),
            n(504.0)
        );
        // Excel: =@A1:A3 entered in H22 is #VALUE!.
        assert_eq!(book.dynamic_at("=@A1:A3", "H22"), e(W::Value));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn spill_references_read_the_spilled_range() {
        let mut book = grid();
        let spilled = book.area("A1", "A3");
        let anchor = book.at("A1");
        book.spill(anchor, spilled);
        assert_eq!(book.dynamic("=SUM(A1#)"), n(101.0 + 201.0 + 301.0));
        assert_eq!(
            book.dynamic("=SUM(_xlfn.ANCHORARRAY(A1))"),
            n(101.0 + 201.0 + 301.0)
        );
        assert_eq!(book.dynamic("=ROWS(B1#)"), e(W::Ref));
        // A reader that does not know spills refuses the reference, naming L3.
        book.reader.spills = None;
        let outcome = book
            .eval_in(FormulaForm::Dynamic, "=SUM(A1#)", "Z100")
            .unwrap();
        assert_eq!(
            outcome,
            Err(EvalFault::NotModelled {
                construct: "a spilled range reference (#) the reader does not know",
                owner: "dc-10y"
            })
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn offset_shifts_references() {
        let mut book = grid();
        assert_eq!(book.dynamic("=OFFSET(A1,2,1)"), n(302.0));
        assert_eq!(
            book.dynamic("=SUM(OFFSET(A1,1,1,2,2))"),
            n(202.0 + 203.0 + 302.0 + 303.0)
        );
        // Excel: an omitted or blank offset is 0.
        assert_eq!(book.dynamic("=OFFSET(A1,,1)"), n(102.0));
        assert_eq!(book.dynamic("=OFFSET(A1,Z50,0)"), n(101.0));
        assert_eq!(book.dynamic("=OFFSET(A1,-1,0)"), e(W::Ref));
        assert_eq!(book.dynamic("=ROWS(OFFSET(A5,0,0,-2))"), n(2.0));
        assert_eq!(
            book.dynamic("=OFFSET(A1,0,0,2,1)"),
            numbers(&[&[101.0], &[201.0]])
        );
        // Lifted, OFFSET gives an array of references (dc-b1l; Excel: =OFFSET(A1,{0;1},0) is
        // #VALUE! in each cell it spills to, which calc-engine shows, yet
        // =SUM(OFFSET(A1,{0;1;2},0)) adds the cells).
        let lifted = book
            .eval_in(FormulaForm::Dynamic, "=OFFSET(A1,{0;1},0)", "Z100")
            .unwrap();
        let Ok(lifted) = lifted else {
            panic!("{lifted:?}");
        };
        let array = lifted.as_array().expect("an array");
        assert_eq!(array.shape().rows, 2);
        assert!(
            array
                .iter_row_major()
                .all(|element| element.as_reference().is_some()),
            "{lifted:?}"
        );
        assert_eq!(book.dynamic("=SUM(OFFSET(A1,{0;1},0))"), n(302.0));
        // In a legacy formula OFFSET's result is intersected.
        assert_eq!(book.legacy_at("=OFFSET(A1,0,0,3,1)", "Z2"), n(201.0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn ranges_meeting_values_follow_the_form() {
        let mut book = grid();
        let _ = CalcValue::empty();
        assert_eq!(book.legacy_at("=A1:A10+0", "Z4"), n(401.0));
        assert_eq!(book.dynamic("=ROWS(A1:A10+0)"), n(10.0));
    }
}

/// L1 (dc-mzk): defined names at workbook and sheet scope, named LAMBDAs, and the refusal of a
/// name a catalogue cannot supply, through `calc_fml::evaluate`.
///
/// Ported from OxFml's evaluator tests (OxFml 15daac5, `crates/oxfml_core/tests/
/// evaluator_tests.rs`: `evaluator_uses_defined_name_bindings_for_sum`,
/// `evaluator_direct_deref_of_unresolved_name_yields_name_error_value`,
/// `evaluator_invokes_defined_name_callable_binding`,
/// `evaluator_executes_bounded_recursive_defined_name_callable`,
/// `evaluator_executes_bounded_mutual_recursive_defined_name_callables`), with the names held by
/// the test workbook's catalogue.
mod names {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn workbook_names_hold_values_and_references() {
        let mut book = Book::new();
        book.number("Sheet1!B2", 40.0)
            .number("Sheet1!B3", 2.0)
            .name("InputValue", "=40")
            .name("TaxRate", "0.25")
            .name("Inputs", "Sheet1!$B$2:$B$3")
            .name("Total", "=SUM(Inputs)*(1+TaxRate)");
        assert_eq!(book.dynamic("=SUM(InputValue,2)"), n(42.0));
        assert_eq!(book.dynamic("=taxrate*4"), n(1.0));
        assert_eq!(book.dynamic("=SUM(Inputs)"), n(42.0));
        assert_eq!(book.dynamic("=ROWS(Inputs)"), n(2.0));
        assert_eq!(book.dynamic("=Total"), n(52.5));
        assert_eq!(book.dynamic("=Inputs"), numbers(&[&[40.0], &[2.0]]));
        // A legacy formula intersects a name's range as it does a written one.
        assert_eq!(book.legacy_at("=Inputs", "Sheet2!C3"), n(2.0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn sheet_names_shadow_workbook_names() {
        let mut book = Book::new();
        book.name("Rate", "1").local_name("Sheet2", "Rate", "2");
        assert_eq!(book.dynamic_at("=Rate", "Sheet1!A1"), n(1.0));
        assert_eq!(book.dynamic_at("=Rate", "Sheet2!A1"), n(2.0));
        assert_eq!(book.dynamic_at("=Sheet2!Rate*10", "Sheet1!A1"), n(20.0));
        // Sheet1 holds no name of its own called Rate.
        assert_eq!(book.dynamic_at("=Sheet1!Rate", "Sheet2!A1"), e(W::Name));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn unknown_names_are_name_errors_and_unknown_catalogues_refuse() {
        let mut book = Book::new();
        assert_eq!(book.dynamic("=Missing"), e(W::Name));
        assert_eq!(book.dynamic("=IFERROR(Missing,2)"), n(2.0));
        // A call of a name no defined name holds could be a VBA or add-in function: refused.
        assert!(matches!(
            book.eval_in(FormulaForm::Dynamic, "=ISERROR(NoSuchFunction(1))", "Z100"),
            Err(CompileError::UnknownFunction { .. })
        ));
        // A catalogue that does not carry names: every name is refused, naming L3.
        book.catalog.names = None;
        match book.eval_in(FormulaForm::Dynamic, "=TaxRate*2", "Z100") {
            Err(CompileError::Unsupported {
                construct, owner, ..
            }) => assert_eq!((construct, owner), ("a defined name", "dc-10y")),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            book.eval_in(FormulaForm::Dynamic, "=NoSuchFunction(1)", "Z100"),
            Err(CompileError::UnknownFunction { .. })
        ));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn names_with_relative_references_read_from_the_formula_cell() {
        // L3 (dc-m3u): a file stores a name's relative reference relative to A1, and Excel reads
        // it from the cell of the formula that uses the name, wrapping around the grid's edge
        // (Excel 16.0, 2026-10-23: LeftCell, `RC[-1]` stored as `Nm!XFD1`, is B3 in C3 and XFD1 in
        // A1).
        let mut book = Book::new();
        book.number("Y100", 7.0)
            .number("XFD1", 9.0)
            .number("B3", 5.0);
        book.name("Here", "Sheet1!A1")
            .name("LeftCell", "Sheet1!XFD1");
        assert_eq!(book.dynamic_at("=Here", "Y100"), n(7.0));
        assert_eq!(book.dynamic_at("=LeftCell", "C3"), n(5.0));
        assert_eq!(book.dynamic_at("=LeftCell", "A1"), n(9.0));
        assert_eq!(book.dynamic_at("=SUM(LeftCell)", "Z100"), n(7.0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn named_lambdas_are_called_and_may_recurse() {
        // OxFml's rows call these Fact, IsEven and IsOdd, which are Excel's built-in FACT, ISEVEN and
        // ISODD: Excel calls the built-in for such a call, so they are renamed here.
        let mut book = Book::new();
        book.name("AddOne", "LAMBDA(x,x+1)")
            .name("MyFact", "=LAMBDA(n,IF(n<=1,1,n*MyFact(n-1)))")
            .name("EvenQ", "LAMBDA(n,IF(n=0,TRUE,OddQ(n-1)))")
            .name("OddQ", "LAMBDA(n,IF(n=0,FALSE,EvenQ(n-1)))");
        assert_eq!(book.dynamic("=AddOne(2)"), n(3.0));
        assert_eq!(book.dynamic("=MyFact(5)"), n(120.0));
        assert_eq!(book.dynamic("=EvenQ(10)"), b(true));
        assert_eq!(book.dynamic("=EvenQ(7)"), b(false));
        // A named LAMBDA reached as a value is #CALC! in a cell: the evaluator returns the LAMBDA.
        let value = book.dynamic("=AddOne");
        assert!(value.callable_value().is_some());
        assert_eq!(value.as_error(), Some(W::Calc));
        // A built-in name calls the built-in, never a name of that spelling.
        book.name("SUM", "LAMBDA(x,0)");
        assert_eq!(book.dynamic("=SUM(1,2)"), n(3.0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn recursion_deeper_than_the_evaluator_nests_is_refused() {
        let mut book = Book::new();
        book.name("CountDown", "LAMBDA(n,IF(n<=0,0,1+CountDown(n-1)))");
        assert_eq!(book.dynamic("=CountDown(40)"), n(40.0));
        let deep = book
            .eval_in(FormulaForm::Dynamic, "=CountDown(5000)", "Z100")
            .unwrap();
        assert!(
            matches!(
                deep,
                Err(EvalFault::NotModelled {
                    owner: "dc-10y",
                    ..
                })
            ),
            "{deep:?}"
        );
    }

    /// The deepest recursion the evaluator runs fits a 1 MB stack (the main thread's on Windows),
    /// in an unoptimised build: each level is a named LAMBDA call through IF, a lazy call.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn the_deepest_recursion_fits_a_one_megabyte_stack() {
        super::super::on_a_one_megabyte_stack(|| {
            let depth = calc_fml::MAX_CALL_DEPTH - 1;
            let mut book = Book::new();
            book.name("CountDown", "LAMBDA(n,IF(n<=0,0,1+CountDown(n-1)))");
            let outcome = book.dynamic(&format!("=CountDown({})", depth - 1));
            assert_eq!(outcome, n((depth - 1) as f64));
        });
    }
}

/// L1 (dc-mzk): LET and LAMBDA, through `calc_fml::evaluate`.
///
/// Ported from OxFml's evaluator tests (OxFml 15daac5, `crates/oxfml_core/tests/
/// evaluator_tests.rs`: `evaluator_runs_let_*`, `evaluator_runs_immediate_lambda_invocation`,
/// `evaluator_runs_helper_bound_lambda_*`, `evaluator_uses_lexical_not_dynamic_scope_*`,
/// `evaluator_executes_*_returned_lambda_*`, `evaluator_*_optional_bracket_parameter`,
/// `evaluator_matches_excel_builtin_colliding_let_recursive_name_frontier_ftc_0443`,
/// `evaluator_preserves_non_builtin_let_recursive_name_self_application`,
/// `evaluator_projects_direct_helper_local_self_recursion_as_name_error`, and
/// `ftc_1013_case_insensitive_shadowing_tests.rs`), and Excel 16.0 rows measured through COM on
/// 2026-10-23.
mod lambda {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn let_binds_names_in_order() {
        let mut book = Book::new();
        book.number("A1", 5.0);
        assert_eq!(book.dynamic("=LET(x,1,x+2)"), n(3.0));
        assert_eq!(book.dynamic("=LET(x,2,y,x*3,x+y)"), n(8.0));
        assert_eq!(book.dynamic("=LET(r,A1,SUM(r,2))"), n(7.0));
        assert_eq!(book.dynamic("=LET(x,1,LET(x,2,x)+x)"), n(3.0));
        assert_eq!(book.dynamic("=LET(X,4,x*2)"), n(8.0));
        // The file form: Excel writes _xlfn.LET and _xlpm. before each name.
        assert_eq!(book.dynamic("=_xlfn.LET(_xlpm.x,3,_xlpm.x*2)"), n(6.0));
        assert_eq!(book.dynamic("=LET(a,{1,2,3},SUM(a*a))"), n(14.0));
        // A name that is no name, and an even number of arguments, do not compile.
        for refused in ["=LET(A1,1,A1)", "=LET(1,1,2)", "=LET(x,1)"] {
            assert!(
                matches!(
                    book.eval_in(FormulaForm::Dynamic, refused, "Z100"),
                    Err(CompileError::Syntax { .. } | CompileError::Arity { .. })
                ),
                "{refused}"
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn lambdas_are_called_where_they_are_written_or_bound() {
        let mut book = Book::new();
        assert_eq!(book.dynamic("=LAMBDA(x,x+1)(2)"), n(3.0));
        assert_eq!(book.dynamic("=LAMBDA(200)()"), n(200.0));
        assert_eq!(book.dynamic("=LET(f,LAMBDA(x,x+1),f(2))"), n(3.0));
        assert_eq!(book.dynamic("=LET(f,LAMBDA(x,x^2),f(3))"), n(9.0));
        assert_eq!(book.dynamic("=LET(a,3,f,LAMBDA(x,x^2),f(a))"), n(9.0));
        // Lexical scope: f sees the x of its own LET.
        assert_eq!(
            book.dynamic("=LET(x,10,f,LAMBDA(y,x+y),LET(x,20,f(2)))"),
            n(12.0)
        );
        // A LAMBDA a LAMBDA returns keeps what it captured. (OxFml's rows name it `add5`, which is a
        // cell reference in Excel's grid, column ADD; Excel refuses it as a LET name.)
        assert_eq!(
            book.dynamic("=LET(adder,LAMBDA(n,LAMBDA(x,x+n)),addfive,adder(5),addfive(10))"),
            n(15.0)
        );
        assert_eq!(book.dynamic("=LAMBDA(n,LAMBDA(x,x+n))(5)(10)"), n(15.0));
        assert_eq!(
            book.dynamic(
                "=LET(base,100,adder,LAMBDA(n,LAMBDA(x,x+n+base)),addfive,adder(5),addfive(10))"
            ),
            n(115.0)
        );
        // A parameter shadows an outer name regardless of case (ftc-1013).
        assert_eq!(book.dynamic("=LET(N,4,LAMBDA(n,1/N)(2))"), n(0.5));
        assert_eq!(book.dynamic("=LET(N,4,LAMBDA(n,1/N)(0))"), e(W::Div0));
        // Lifting inside a body.
        assert_eq!(
            book.dynamic("=LAMBDA(v,v*2)({1,2})"),
            numbers(&[&[2.0, 4.0]])
        );
        // The file form.
        assert_eq!(
            book.dynamic("=_xlfn.LET(_xlpm.f,_xlfn.LAMBDA(_xlpm.x,_xlpm.x+1),_xlpm.f(2))"),
            n(3.0)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn optional_parameters_and_arity() {
        let mut book = Book::new();
        assert_eq!(
            book.dynamic("=LAMBDA(x,[y],IF(ISOMITTED(y),x*2,x+y))(5)"),
            n(10.0)
        );
        assert_eq!(
            book.dynamic("=LAMBDA(x,[y],IF(ISOMITTED(y),x*2,x+y))(5,3)"),
            n(8.0)
        );
        assert_eq!(
            book.dynamic("=LET(f,LAMBDA(x,[y],IF(ISOMITTED(y),x*2,x+y)),f(5))"),
            n(10.0)
        );
        assert_eq!(
            book.dynamic(
                "=_xlfn.LAMBDA(_xlpm.x,_xlop.y,_xlpm.x+IF(_xlfn.ISOMITTED(_xlpm.y),0,_xlpm.y))(1)"
            ),
            n(1.0)
        );
        assert_eq!(book.dynamic("=LAMBDA(a,b,ISOMITTED(b))(1,)"), b(true));
        assert_eq!(book.dynamic("=LAMBDA(a,ISOMITTED(a))(3)"), b(false));
        assert_eq!(book.dynamic("=ISOMITTED(1)"), b(false));
        // Excel 16.0: too few or too many arguments are #VALUE!.
        assert_eq!(book.dynamic("=LAMBDA(x,y,x)(1)"), e(W::Value));
        assert_eq!(book.dynamic("=LAMBDA(x,x)(1,2)"), e(W::Value));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn names_resolve_as_excel_resolves_them() {
        let mut book = Book::new();
        book.number("A1", 1.0);
        // A built-in function wins over a LET name of its spelling (Excel 16.0: 12, GCD's). Until
        // the catalogue holds GCD the call is refused as unknown, never resolved to the LET name.
        match book.eval_in(
            FormulaForm::Dynamic,
            "=LET(gcd,LAMBDA(a,b,a+b),gcd(48,36))",
            "Z100",
        ) {
            Ok(Ok(value)) => assert_eq!(value, n(12.0)),
            Err(CompileError::UnknownFunction { spelling, .. }) => assert_eq!(spelling, "gcd"),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            book.dynamic(
                "=LET(zzgcd,LAMBDA(self,a,b,IF(b=0,a,self(self,b,MOD(a,b)))),zzgcd(zzgcd,48,36))"
            ),
            n(12.0)
        );
        // A LET name is not in scope inside its own value: F there is the call of a name no defined
        // name holds, which Excel gives #NAME? (OxFml's row) and which is refused here, since a VBA
        // or add-in function of that name is not ruled out.
        assert!(matches!(
            book.eval_in(
                FormulaForm::Dynamic,
                "=LET(F,LAMBDA(n,IF(n<=0,0,1+F(n-1))),F(5))",
                "Z100"
            ),
            Err(CompileError::UnknownFunction { .. })
        ));
        assert_eq!(
            book.dynamic("=LET(f,LAMBDA(self,n,IF(n<=0,0,1+self(self,n-1))),f(f,3))"),
            n(3.0)
        );
        // Excel 16.0: calling what is not a LAMBDA.
        assert_eq!(book.dynamic("=(A1)(2)"), e(W::Ref));
        assert_eq!(book.dynamic("=LET(x,1,x(2))"), e(W::Value));
        // A LAMBDA left as the result is the callable, #CALC! in a cell.
        assert_eq!(book.dynamic("=LAMBDA(x,x)").as_error(), Some(W::Calc));
    }
}
