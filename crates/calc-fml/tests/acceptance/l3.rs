//! L3 (dc-m3u): structured references to tables, 3-D references, external references, names with
//! relative references, calls of names no defined name holds, and INDIRECT text naming a defined
//! name, through `calc_fml::evaluate` over the cfml-m1.3 fixtures ([`LoggingCatalog`] with tables
//! and sheet spans, [`RunReader`]).
//!
//! The rows marked measured were entered through COM into Excel 16.0.20430 on 2026-10-23 with
//! `Range.Formula` (a legacy formula) and read back through `Range.Value2`, over the workbook each
//! case builds: sheets F, Ja, Fe, Ma and Tab; Ja!A1:B1 = 1, 10, Fe!A1:B1 = 2, 20, Ma!A1:B1 = 3,
//! "x"; and the table Sales at Tab!A1:C5 (a header row Name, Qty, `Unit [Price]`; data rows a, 2,
//! 5 / b, 3, 7 / c, 4, 11; a totals row Total, 9, 23). The rows marked derived follow Microsoft's
//! documented rules and were not measured.

use super::{LoggingCatalog, RunReader, cell_at};

use calc_fml::reference::{BindError, FormulaAnchor, ReferenceNotation, TableInfo};
use calc_fml::{
    CompileError, EvalFault, EvalFrame, EvalScratch, EvaluationServices, FormulaForm, PlanCache,
    compile_with, evaluate, instantiate, parse_formula,
};
use calc_func::value::{
    CalcValue, CellKey, ExcelText, GridArea, NumericMode, WorksheetErrorCode as W,
};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test;

/// The measured workbook.
struct Book {
    catalog: LoggingCatalog,
    reader: RunReader,
}

fn n(x: f64) -> CalcValue {
    CalcValue::number(x)
}

fn t(s: &str) -> CalcValue {
    CalcValue::text(ExcelText::from_interop_assignment(s))
}

fn e(code: W) -> CalcValue {
    CalcValue::error(code)
}

fn b(x: bool) -> CalcValue {
    CalcValue::logical(x)
}

impl Book {
    fn new() -> Self {
        let mut catalog = LoggingCatalog::new(&["F", "Ja", "Fe", "Ma", "Tab", "Calc"]);
        catalog.names = Some(Default::default());
        catalog.spans = true;
        catalog.no_macros = true;
        let tab = catalog.id("Tab");
        let calc = catalog.id("Calc");
        let area = |sheet, a: &str, z: &str| {
            GridArea::new(cell_at(sheet, a), cell_at(sheet, z)).expect("an area")
        };
        catalog.tables = Some(vec![
            TableInfo {
                name: "Sales".to_owned(),
                sheet: tab,
                area: area(tab, "A1", "C5"),
                header_rows: 1,
                totals_rows: 1,
                columns: vec!["Name".into(), "Qty".into(), "Unit [Price]".into()],
            },
            // A table without a header row or a totals row, whose last column is calculated.
            TableInfo {
                name: "Lines".to_owned(),
                sheet: calc,
                area: area(calc, "B2", "D4"),
                header_rows: 0,
                totals_rows: 0,
                columns: vec!["Units".into(), "Price".into(), "Amount".into()],
            },
        ]);
        let mut book = Book {
            catalog,
            reader: RunReader::default(),
        };
        for (address, value) in [
            ("Ja!A1", n(1.0)),
            ("Fe!A1", n(2.0)),
            ("Ma!A1", n(3.0)),
            ("Ja!B1", n(10.0)),
            ("Fe!B1", n(20.0)),
            ("Ma!B1", t("x")),
            ("Tab!A1", t("Name")),
            ("Tab!B1", t("Qty")),
            ("Tab!C1", t("Unit [Price]")),
            ("Tab!A2", t("a")),
            ("Tab!B2", n(2.0)),
            ("Tab!C2", n(5.0)),
            ("Tab!A3", t("b")),
            ("Tab!B3", n(3.0)),
            ("Tab!C3", n(7.0)),
            ("Tab!A4", t("c")),
            ("Tab!B4", n(4.0)),
            ("Tab!C4", n(11.0)),
            ("Tab!A5", t("Total")),
            ("Tab!B5", n(9.0)),
            ("Tab!C5", n(23.0)),
            ("Calc!B2", n(2.0)),
            ("Calc!C2", n(1.5)),
            ("Calc!B3", n(4.0)),
            ("Calc!C3", n(2.5)),
            ("Calc!B4", n(6.0)),
            ("Calc!C4", n(0.5)),
        ] {
            let at = book.at(address);
            book.reader.values.insert(at, value);
        }
        book
    }

    fn at(&self, address: &str) -> CellKey {
        let (sheet, cell) = address.split_once('!').unwrap_or(("F", address));
        cell_at(self.catalog.id(sheet), cell)
    }

    fn eval(
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
        let mut services = EvaluationServices::new(&NumericMode::Fast, &mut self.reader)
            .with_catalog(&self.catalog);
        Ok(evaluate(
            EvalFrame::new(&instance, &mut scratch),
            &mut services,
        ))
    }

    /// A legacy formula's value at `at`.
    fn legacy(&mut self, formula: &str, at: &str) -> CalcValue {
        match self.eval(FormulaForm::Legacy, formula, at) {
            Ok(Ok(value)) => value,
            other => panic!("{formula} at {at}: {other:?}"),
        }
    }

    fn refusal(&mut self, formula: &str, at: &str) -> CompileError {
        match self.eval(FormulaForm::Legacy, formula, at) {
            Err(error) => error,
            other => panic!("{formula} at {at} compiled: {other:?}"),
        }
    }
}

fn same(a: &CalcValue, b: &CalcValue) -> bool {
    match (a.as_number(), b.as_number()) {
        (Some(x), Some(y)) => (x - y).abs() <= 1e-12 * x.abs().max(1.0),
        _ => a == b,
    }
}

fn check(book: &mut Book, rows: &[(&str, &str, CalcValue)]) {
    for (formula, at, expected) in rows {
        let value = book.legacy(formula, at);
        assert!(
            same(&value, expected),
            "{formula} at {at}: {value:?}, expected {expected:?}"
        );
    }
}

/// Structured references, each measured (module documentation), in the rows of sheet F where
/// Excel held them (row 48 on), and in the table's own rows on Tab.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn structured_references_select_the_measured_parts_of_a_table() {
    let mut book = Book::new();
    check(
        &mut book,
        &[
            ("=SUM(Sales[Qty])", "B48", n(9.0)),
            ("=Sales[[#Headers],[Qty]]", "B49", t("Qty")),
            ("=Sales[[#Totals],[Qty]]", "B50", n(9.0)),
            ("=ROWS(Sales[#All])", "B51", n(5.0)),
            ("=ROWS(Sales[#Data])", "B52", n(3.0)),
            ("=ROWS(Sales[])", "B53", n(3.0)),
            ("=ROWS(Sales)", "B53", n(3.0)),
            ("=SUM(Sales[])", "B54", n(32.0)),
            ("=SUM(Sales)", "B54", n(32.0)),
            ("=SUM(Sales[[Qty]:[Unit '[Price']]])", "B55", n(32.0)),
            ("=COUNTA(Sales[#Headers])", "B56", n(3.0)),
            ("=ROWS(Sales[[#Headers],[#Data]])", "B57", n(4.0)),
            ("=ROWS(Sales[[#Data],[#Totals]])", "B58", n(4.0)),
            ("=Sales[[#This Row],[Qty]]", "B59", e(W::Value)),
            ("=SUM(Sales[Unit '[Price']])", "B61", n(23.0)),
            ("=COLUMNS(Sales[#All])", "B62", n(3.0)),
            ("=ROWS(Sales[[#All],[Qty]])", "B64", n(5.0)),
            ("=SUM(Sales[#Totals])", "B65", n(32.0)),
            ("=Sales[[#Totals],[Name]]", "B66", t("Total")),
            ("=COUNTA(Sales[[#Headers],[Name]:[Qty]])", "B67", n(2.0)),
            ("=SUM(sales[qty])", "B68", n(9.0)),
            ("=Sales[#This Row]", "B70", e(W::Value)),
            ("=SUM(Sales[Qty],Sales[Qty])", "B71", n(18.0)),
            ("=SUM(Sales[Qty]:Sales[Unit '[Price']])", "B72", n(32.0)),
            ("=ISREF(Sales[])", "B73", b(true)),
            ("=COLUMN(Sales[Qty])", "B75", n(2.0)),
            ("=Sales[[#This Row],[Qty]]", "Tab!E2", n(2.0)),
            ("=Sales[@Qty]", "Tab!E2", n(2.0)),
            ("=Sales[[#This Row],[Qty]]", "Tab!F3", n(3.0)),
            ("=Sales[[#This Row],[Qty]]", "Tab!E8", e(W::Value)),
            ("=Sales[[#This Row],[Unit '[Price']]]", "Tab!G3", n(7.0)),
            ("=Sales[@[Unit '[Price']]]", "Tab!G3", n(7.0)),
            // dc-10y: Excel 16.0.20430 (2026-10-25) drops a sheet qualifier before a structured
            // reference, whatever sheet it names (bind.rs, `qualified_structured`).
            ("=SUM(Tab!Sales[Qty])", "B76", n(9.0)),
            ("=SUM('Tab'!Sales[Qty])", "B77", n(9.0)),
            ("=SUM(F!Sales[Qty])", "B78", n(9.0)),
            ("=Tab!Sales[[#This Row],[Qty]]", "Tab!E2", n(2.0)),
        ],
    );
}

/// Derived: an unqualified structured reference in a table's calculated column names the table
/// that holds the formula; a table without a header row gives #REF! for its headers (Microsoft's
/// "Using structured references with Excel tables"); a table or column the workbook does not hold
/// is refused, as Excel refuses to enter it.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn unqualified_references_and_missing_parts() {
    let mut book = Book::new();
    check(
        &mut book,
        &[
            ("=[@Units]*[@Price]", "Calc!D2", n(3.0)),
            ("=[@Units]*[@Price]", "Calc!D3", n(10.0)),
            ("=SUM([Units])", "Calc!D4", n(12.0)),
            ("=Lines[[#This Row],[Units]]*2", "Calc!D4", n(12.0)),
            ("=ISERROR(Lines[#Headers])", "F1", b(true)),
            ("=ERROR.TYPE(Lines[[#Totals],[Price]])", "F1", n(4.0)),
            ("=ROWS(Lines)", "F1", n(3.0)),
        ],
    );
    assert!(matches!(
        book.refusal("=SUM(Nope[Qty])", "F1"),
        CompileError::Reference {
            error: BindError::UnknownTable { .. },
            ..
        }
    ));
    assert!(matches!(
        book.refusal("=SUM(Sales[Nope])", "F1"),
        CompileError::Reference {
            error: BindError::UnknownColumn { .. },
            ..
        }
    ));
    // Outside every table, `[@Units]` is no structured reference: Excel 16.0.20430 (2026-10-25)
    // reads a file's `SUM([Qty])` there as `SUM(#REF!)` (dc-10y).
    assert_eq!(book.legacy("=[@Units]", "F1"), e(W::Ref));
    assert_eq!(book.legacy("=SUM([Units])", "F1"), e(W::Ref));
    assert_eq!(book.legacy("=[[Units]]", "F1"), e(W::Ref));
    assert_eq!(book.legacy("=ISREF([Units])", "F1"), b(false));
    assert_eq!(book.legacy("=IFERROR([Units],5)", "F1"), n(5.0));
    // A LAMBDA's optional parameter inside a table is still a parameter.
    assert_eq!(book.legacy("=LAMBDA(x,[Units],x)(5)", "Calc!D2"), n(5.0));
}

/// 3-D references, each measured (module documentation) at F!B1 down.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn three_d_references_reach_the_functions_that_take_them() {
    let mut book = Book::new();
    check(
        &mut book,
        &[
            ("=Ja:Ma!A1", "B1", e(W::Ref)),
            ("=Ja:Ma!A1+1", "B2", e(W::Ref)),
            ("=SUM(Ja:Ma!A1)", "B3", n(6.0)),
            ("=SUM(Ja:Ma!A1:B1)", "B4", n(36.0)),
            ("=SUM(Ma:Ja!A1)", "B5", n(6.0)),
            ("=SUM('Ja:Ma'!A1)", "B6", n(6.0)),
            ("=AVERAGE(Ja:Ma!A1)", "B7", n(2.0)),
            ("=COUNT(Ja:Ma!A1:B1)", "B8", n(5.0)),
            ("=COUNTA(Ja:Ma!A1:B1)", "B9", n(6.0)),
            ("=MAX(Ja:Ma!A1)", "B10", n(3.0)),
            ("=MIN(Ja:Ma!A1)", "B11", n(1.0)),
            ("=PRODUCT(Ja:Ma!A1)", "B12", n(6.0)),
            ("=STDEV(Ja:Ma!A1)", "B13", n(1.0)),
            ("=VAR.S(Ja:Ma!A1)", "B14", n(1.0)),
            ("=STDEV.P(Ja:Ma!A1)", "B15", n(0.816496580927726)),
            ("=AVERAGEA(Ja:Ma!A1:B1)", "B16", n(6.0)),
            ("=MAXA(Ja:Ma!B1)", "B17", n(20.0)),
            ("=SUMSQ(Ja:Ma!A1)", "B18", n(14.0)),
            ("=MEDIAN(Ja:Ma!A1)", "B19", n(2.0)),
            ("=LARGE(Ja:Ma!A1,1)", "B20", n(3.0)),
            ("=RANK(1,Ja:Ma!A1)", "B21", n(3.0)),
            ("=INDEX(Ja:Ma!A1,1)", "B22", e(W::Value)),
            ("=ROWS(Ja:Ma!A1:B2)", "B23", e(W::Value)),
            ("=AREAS(Ja:Ma!A1)", "B24", e(W::Value)),
            ("=ISREF(Ja:Ma!A1)", "B25", b(false)),
            ("=SUMPRODUCT(Ja:Ma!A1)", "B26", e(W::Ref)),
            ("=ABS(Ja:Ma!A1)", "B27", e(W::Ref)),
            ("=COUNTIF(Ja:Ma!A1,\">0\")", "B28", e(W::Value)),
            ("=N(Ja:Ma!A1)", "B29", e(W::Value)),
            ("=CONCAT(Ja:Ma!A1)", "B30", t("123")),
            ("=AND(Ja:Ma!A1)", "B31", b(true)),
            ("=OR(Ja:Ma!A1)", "B32", b(true)),
            ("=IFERROR(Ja:Ma!A1,\"e\")", "B33", t("e")),
            ("=ISERROR(Ja:Ma!A1)", "B34", b(true)),
            ("=SHEETS(Ja:Ma!A1)", "B35", n(3.0)),
            ("=INDIRECT(\"Ja:Ma!A1\")", "B37", e(W::Ref)),
            ("=SUM(Ja:Ma!A1,Ja:Fe!B1)", "B39", n(36.0)),
            ("=SUM(Ja:Ma!A1:A2 Ja:Ma!A1)", "B41", e(W::Value)),
            ("=COUNTBLANK(Ja:Ma!C1)", "B42", e(W::Value)),
            ("=MODE(Ja:Ma!A1)", "B43", e(W::Ref)),
            ("=TEXTJOIN(\",\",TRUE,Ja:Ma!A1)", "B44", t("1,2,3")),
            ("=MAX(Ja:Ma!A:A)", "B45", n(3.0)),
            ("=VARP(Ja:Ma!A1)", "B46", n(0.666666666666667)),
            ("=STDEVA(Ja:Ma!A1:B1)", "B47", n(7.72010362624751)),
            // dc-10y, measured on 2026-10-25 (compile.rs, THREE_D_TAKEN).
            ("=SMALL(Ja:Ma!A1,1)", "B48", n(1.0)),
            ("=GEOMEAN(Ja:Ma!A1)", "B49", n(1.8171205928321397)),
            ("=NPV(1,Ja:Ma!A1)", "B50", n(1.375)),
            ("=ISNUMBER(Ja:Ma!A1)", "B51", b(false)),
            ("=IRR(Ja:Ma!A1)", "B52", e(W::Value)),
        ],
    );
    // A position whose handling of a 3-D reference is not measured (SUMIF's criteria, where a
    // 3-D reference and #REF! both match nothing): refused.
    assert!(matches!(
        book.refusal("=SUMIF(A1:A3,Ja:Ma!A1)", "B1"),
        CompileError::Unsupported {
            owner: "dc-10y",
            ..
        }
    ));
}

/// External references are refused where no link caches their sheet: a book a file's formula
/// numbers (`[1]`) or a typed formula names by its file name, when it runs against a catalogue that
/// caches no link (dc-ylr), and INDIRECT's text.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn external_references_are_refused() {
    let mut book = Book::new();
    for formula in [
        "=[1]Ja!A1",
        "=SUM('[Book2.xlsx]Ja'!A1:B2)",
        "=[Book2.xlsx]Ja!A1+1",
        r"=SUM('C:\Data\[Book2.xlsx]Ja'!A1:B2)",
    ] {
        assert_eq!(
            book.eval(FormulaForm::Legacy, formula, "B1"),
            Ok(Err(EvalFault::NotModelled {
                construct: "a reference to another workbook whose sheet no link of this workbook caches",
                owner: "dc-ylr",
            })),
            "{formula}"
        );
    }
    assert_eq!(
        book.eval(FormulaForm::Legacy, "=INDIRECT(\"[Book2]Ja!A1\")", "B1"),
        Ok(Err(EvalFault::NotModelled {
            construct: "reference text naming another workbook (an external reference)",
            owner: "dc-10y",
        }))
    );
}

/// Measured: names with relative references read from the formula's cell, wrapping around the
/// grid's edge; a call of a name no function or defined name holds is #NAME? in a workbook
/// without macros; INDIRECT reads a relative name from its own cell and a constant name as no
/// reference. Excel 16.0, 2026-10-23, sheet Nm (here F): A3 = 5, B3 = 7, B2 = 9, A4 = 6;
/// LeftCell `RC[-1]` (stored `F!XFD1`), AboveCell `R[-1]C` (stored `F!A1048576`), ColA `RC1`
/// (stored `F!$A1`), Konst 5, Abs `F!$A$1`. Excel's A1 held `=LeftCell` (0, read from XFD1); here
/// A1 holds 100, so `=INDIRECT("Abs")` is 100 where Excel showed A1's 0, and `=LeftCell` is
/// evaluated at A1 to show the wrap.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn relative_names_unknown_calls_and_indirect_names() {
    let mut book = Book::new();
    // C3 holds `=LeftCell`, 7 in Excel, which `=AboveCell` in C4 reads.
    for (address, value) in [
        ("A3", 5.0),
        ("B3", 7.0),
        ("B2", 9.0),
        ("A4", 6.0),
        ("A1", 100.0),
        ("C3", 7.0),
    ] {
        let at = book.at(address);
        book.reader.values.insert(at, n(value));
    }
    book.catalog
        .define(None, "LeftCell", "F!XFD1")
        .define(None, "AboveCell", "F!A1048576")
        .define(None, "ColA", "F!$A1")
        .define(None, "Konst", "5")
        .define(None, "Abs", "F!$A$1");
    check(
        &mut book,
        &[
            ("=LeftCell", "C3", n(7.0)),
            ("=AboveCell", "C4", n(7.0)),
            ("=ColA", "D4", n(6.0)),
            ("=LeftCell", "A1", n(0.0)),
            ("=INDIRECT(\"LeftCell\")", "E3", n(0.0)),
            ("=INDIRECT(\"Konst\")", "E4", e(W::Ref)),
            ("=INDIRECT(\"Abs\")", "E5", n(100.0)),
            ("=FOO(1)", "F1", e(W::Name)),
            ("=foo", "F2", e(W::Name)),
            ("=SUM(FOO(1),2)", "F3", e(W::Name)),
            ("=IFERROR(FOO(1),3)", "F4", n(3.0)),
            ("=Konst(2)", "G1", e(W::Value)),
        ],
    );
    // A workbook that may hold VBA functions: the call is refused.
    book.catalog.no_macros = false;
    assert!(matches!(
        book.refusal("=FOO(1)", "F1"),
        CompileError::UnknownFunction { .. }
    ));
}

/// Measured: where a legacy formula lifts or intersects a range expression at the parameters L3
/// measured (the `legacy` module's documentation), each in the row of sheet Lg (here F) where
/// Excel held it, over A1:A3 = 1, 2, 3 and B1:B3 = 10, 20, 30.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn legacy_places_follow_the_measured_parameters() {
    let mut book = Book::new();
    for (address, value) in [
        ("A1", 1.0),
        ("A2", 2.0),
        ("A3", 3.0),
        ("B1", 10.0),
        ("B2", 20.0),
        ("B3", 30.0),
    ] {
        let at = book.at(address);
        book.reader.values.insert(at, n(value));
    }
    check(
        &mut book,
        &[
            ("=XLOOKUP(2,A1:A3*1,B1:B3)", "D5", n(20.0)),
            ("=XLOOKUP(2,A1:A3,B1:B3*1)", "D6", n(20.0)),
            ("=XLOOKUP(A1:A3*1,A1:A3,B1:B3)", "D7", e(W::Value)),
            ("=XMATCH(2,A1:A3*1)", "D8", n(2.0)),
            ("=XMATCH(A1:A3*1,A1:A3)", "D9", e(W::Value)),
            ("=MEDIAN(A1:A3*1)", "D13", e(W::Value)),
            ("=LARGE(A1:A3*1,1)", "D14", e(W::Value)),
            ("=SMALL(A1:A3*1,2)", "D15", e(W::Value)),
            ("=STDEV(A1:A3*1)", "D16", e(W::Value)),
            ("=VAR(A1:A3*1)", "D17", e(W::Value)),
            ("=MMULT(A1:A3*1,1)", "D20", n(1.0)),
            ("=NPV(0.1,A1:A3*1)", "D22", e(W::Value)),
            ("=CORREL(A1:A3*1,B1:B3)", "D25", n(1.0)),
            ("=PERCENTILE(A1:A3*1,0.5)", "D26", e(W::Value)),
            ("=QUARTILE(A1:A3*1,1)", "D27", e(W::Value)),
            ("=FREQUENCY(A1:A3*1,2)", "D29", n(2.0)),
            ("=MODE(A1:A3*1)", "D30", e(W::NA)),
            ("=GEOMEAN(A1:A3*1)", "D34", e(W::Value)),
            ("=SLOPE(B1:B3,A1:A3*1)", "D42", n(10.0)),
            ("=INTERCEPT(B1:B3*1,A1:A3)", "D43", n(0.0)),
            ("=FORECAST(2,B1:B3,A1:A3*1)", "D44", n(20.0)),
            ("=SUMX2MY2(A1:A3*1,B1:B3)", "D45", n(-1386.0)),
            ("=SUMXMY2(A1:A3*1,B1:B3)", "D46", n(1134.0)),
            ("=COVAR(A1:A3*1,B1:B3)", "D47", n(20.0 / 3.0)),
            ("=PEARSON(A1:A3*1,B1:B3)", "D48", n(1.0)),
            ("=RSQ(A1:A3*1,B1:B3)", "D49", n(1.0)),
            ("=STEYX(B1:B3,A1:A3*1)", "D50", n(0.0)),
            ("=STDEVP(A1:A3*1)", "D53", e(W::Value)),
            ("=VARP(A1:A3*1)", "D54", e(W::Value)),
        ],
    );
}
