//! Adapts the shared typed entry parser to the authored workbook model.

use calc_doc_model::{CellInput, CellText, FiniteNumber, FormulaText};
use calc_fml::CalcEnvironment;
use calc_formula_kernel::entry::{self, Entry};

pub use calc_formula_kernel::entry::EntryRefusal;
pub(crate) use calc_formula_kernel::entry::is_plain_number;

pub fn interpret_entry(text: &str) -> Result<Option<CellInput>, EntryRefusal> {
    interpret_entry_in(text, None)
}

/// Reads text under the workbook's Excel locale and date system.
pub fn interpret_entry_in(
    text: &str,
    environment: Option<&CalcEnvironment>,
) -> Result<Option<CellInput>, EntryRefusal> {
    Ok(
        entry::interpret_entry_in(text, environment)?.map(|entry| match entry {
            Entry::Number(number) => {
                CellInput::Number(FiniteNumber::new(number).expect("finite entry"))
            }
            Entry::Text(text) => {
                CellInput::Text(CellText::new(text).expect("entry length was checked"))
            }
            Entry::Formula(formula) => {
                CellInput::Formula(FormulaText::new(formula).expect("nonempty entry"))
            }
            Entry::Logical(value) => CellInput::Logical(value),
            Entry::Error(error) => CellInput::Error(error),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_doc_model::value::{ExcelText, WorksheetErrorCode};

    fn number(value: f64) -> Option<CellInput> {
        Some(CellInput::Number(FiniteNumber::new(value).unwrap()))
    }

    fn text(value: &str) -> Option<CellInput> {
        Some(CellInput::Text(
            CellText::new(ExcelText::from_interop_assignment(value)).unwrap(),
        ))
    }

    // Ported from OxFml's authored_input_returns_calc_value_for_cell_entry_literals.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn entry_literals() {
        assert_eq!(interpret_entry(""), Ok(None));
        assert_eq!(interpret_entry("123.4"), Ok(number(123.4)));
        assert_eq!(interpret_entry("'123.4"), Ok(text("123.4")));
        assert_eq!(
            interpret_entry("#DIV/0!"),
            Ok(Some(CellInput::Error(WorksheetErrorCode::Div0)))
        );
        assert_eq!(
            interpret_entry("#getting_data"),
            Ok(Some(CellInput::Error(WorksheetErrorCode::NA)))
        );
        assert_eq!(interpret_entry("#SPILL!"), Ok(text("#SPILL!")));
        assert_eq!(interpret_entry("true"), Ok(Some(CellInput::Logical(true))));
        assert_eq!(interpret_entry("-1.5e3"), Ok(number(-1500.0)));
        assert_eq!(interpret_entry(".5"), Ok(number(0.5)));
        assert_eq!(interpret_entry("Q1 2026"), Ok(text("Q1 2026")));
        assert_eq!(interpret_entry("abc"), Ok(text("abc")));
        assert_eq!(
            interpret_entry("123456789012345"),
            Ok(number(123456789012345.0))
        );
        assert_eq!(interpret_entry("1.50000000000000000"), Ok(number(1.5)));
    }

    /// dc-mx3x: the grid refuses a formula Excel's entry refuses, and stores nothing; a near
    /// neighbour Excel takes enters.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn formulas_excel_refuses_are_refused() {
        for refused in [
            "=IRR()",
            "=COUNTBLANK(2)",
            "=_xlfn.ANCHORARRAY(B1)",
            "=LET(xx,1,XX,2,xx)",
            "=LAMBDA(xx,XX,xx)(1,2)",
            "=IFERROR(#N/A,LAMBDA(xx,xx))(3)",
        ] {
            assert!(
                matches!(interpret_entry(refused), Err(EntryRefusal::Rejected { .. })),
                "{refused}"
            );
        }
        for taken in [
            "=IRR({-1,2})",
            "=COUNTBLANK(A1:A2)",
            "=LET(x,1,LET(x,2,x))",
            "=IF(TRUE,LAMBDA(x,x))(3)",
        ] {
            assert!(
                matches!(interpret_entry(taken), Ok(Some(CellInput::Formula(_)))),
                "{taken}"
            );
        }
    }

    /// Cell entries under en-ZA (decimal point, space between thousands), against Excel
    /// 16.0.20430 through `Range.FormulaLocal` on 2026-10-25 (see [`interpret_entry_in`]): each
    /// text Excel held as a number reads as that number, each it held as text is text.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn entries_read_under_a_locale() {
        let profile = calc_fml::FormatProfile::for_culture("en-ZA")
            .and_then(|profile| profile.with_separators(".", " "))
            .unwrap();
        let environment = CalcEnvironment {
            compatibility_version: 1,
            date_system: calc_fml::DateSystem::System1900,
            profile: Some(profile),
            now: None,
        };
        let entry = |text: &str| interpret_entry_in(text, Some(&environment));
        for (typed, value) in [
            ("5%", 0.05),
            ("-5%", -0.05),
            ("5 %", 0.05),
            ("(5)", -5.0),
            ("(5%)", -0.05),
            ("1e3", 1000.0),
            ("1E-2", 0.01),
            ("1.5e+3", 1500.0),
            ("+5", 5.0),
            ("-5", -5.0),
            (" 5", 5.0),
            ("5 ", 5.0),
            ("5.", 5.0),
            ("1 1/2", 1.5),
            ("1 000", 1000.0),
            ("R5", 5.0),
            ("2020-01-02", 43832.0),
            ("2020/1/2", 43832.0),
            ("2-Jan-2020", 43832.0),
            ("2 Jan 2020", 43832.0),
            ("January 2020", 43831.0),
            ("1/2/20", 36942.0),
            ("12:30", 0.5208333333333334),
            ("12:30:45", 0.5213541666666667),
            ("12:30 PM", 0.5208333333333334),
            ("1:30 am", 0.0625),
            ("25:00", 1.0416666666666667),
            ("10:", 0.4166666666666667),
            ("0:00:01.5", 1.736111111111111e-05),
        ] {
            assert_eq!(entry(typed), Ok(number(value)), "{typed:?}");
        }
        for typed in [
            "1,000",
            "1,000.5",
            "1,00",
            "12,34,567",
            "$5",
            "($5)",
            "$1,234.50",
            "3/4/2020",
            "Jan 2, 2020",
            "3/4/2020 12:30",
            "1/2/30",
            "13/2/2020",
            "2/30/2020",
            "1.2.2020",
            "$",
            "%",
            "1,2",
            "1'000",
            "0x10",
            "1d",
            "5%%",
            "$5%",
            ",5",
            "1..2",
            "£5",
            ":10",
            "1:2:3:4",
            "1/0",
            "5e",
            "5-",
            "5+",
        ] {
            assert_eq!(entry(typed), Ok(text(typed)), "{typed:?}");
        }
        // Not modelled: a date without a year with no clock, and a text Excel's entry may take
        // for a formula.
        for typed in ["1/2", "--5", "-1:30"] {
            assert!(
                matches!(entry(typed), Err(EntryRefusal::NumberShaped { .. })),
                "{typed:?}"
            );
        }
        assert_eq!(entry("TRUE"), Ok(Some(CellInput::Logical(true))));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn entry_formulas_and_refusals() {
        let Ok(Some(CellInput::Formula(formula))) = interpret_entry("=SUM(1,2)") else {
            panic!("a formula");
        };
        assert_eq!(formula.text().to_string_lossy(), "SUM(1,2)");
        assert_eq!(interpret_entry("="), Err(EntryRefusal::EmptyFormula));
        for shaped in [
            "12%",
            "1,000",
            "$5",
            "2026-10-23",
            "10:30",
            "1/2",
            " 12",
            "+A1",
            "-x",
            "1234567890123456",
            "1e999",
        ] {
            assert!(
                matches!(
                    interpret_entry(shaped),
                    Err(EntryRefusal::NumberShaped { .. })
                ),
                "{shaped}"
            );
        }
    }
}
