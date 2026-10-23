//! A calculation's result as the surface shows it: its value type, the value itself, an array as a
//! bounded grid window, an error with what it means, and where the value came from.
//!
//! The array window is ported from DnaTreeCalc's formula UX core
//! (`src/dnacalc-formula-ux-core/src/lib.rs` at DnaTreeCalc c68ecaa: `project_runtime_result`,
//! `project_array_window`, and `MAX_ARRAY_WINDOW_CELLS` from `src/dnacalc-skin-ir/src/formula.rs`)
//! onto calc-value's `CalcArray`, without the skin IR between them. Nothing here computes a
//! value: it lays out the value the session's calculation returned.

use crate::display;
use crate::session::{Calculation, Session};
use calc_func::locale::FormatProfile;
use calc_func::value::{CalcArray, CalcValue, CoreValue, WorksheetErrorCode};

/// The most cells an array window holds.
pub const MAX_ARRAY_WINDOW_CELLS: usize = 4_096;

/// The most rows an array window shows at first.
const FIRST_WINDOW_ROWS: usize = 32;

/// One cell of an array window: its value type and its value as text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowCell {
    pub kind: &'static str,
    pub text: String,
}

/// A window of an array result: the array's whole shape, where the window starts, and its cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArrayWindow {
    pub total_rows: usize,
    pub total_cols: usize,
    pub row_offset: usize,
    pub col_offset: usize,
    pub cells: Vec<Vec<WindowCell>>,
}

impl ArrayWindow {
    /// Whether the window shows less than the whole array.
    pub fn truncated(&self) -> bool {
        let rows = self.cells.len();
        let cols = self.cells.first().map_or(0, Vec::len);
        self.row_offset > 0
            || self.col_offset > 0
            || rows < self.total_rows
            || cols < self.total_cols
    }
}

/// Why an array window was not made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArrayWindowError {
    Empty,
    TooLarge,
    OutsideArray,
    Overflow,
}

/// The window of `array` with `row_count` rows and `col_count` columns from the 0-based offsets,
/// each number shown as [`shown`] shows it under `profile`.
pub fn array_window(
    array: &CalcArray,
    row_offset: usize,
    col_offset: usize,
    row_count: usize,
    col_count: usize,
    profile: Option<&FormatProfile>,
) -> Result<ArrayWindow, ArrayWindowError> {
    if row_count == 0 || col_count == 0 {
        return Err(ArrayWindowError::Empty);
    }
    if row_count
        .checked_mul(col_count)
        .ok_or(ArrayWindowError::Overflow)?
        > MAX_ARRAY_WINDOW_CELLS
    {
        return Err(ArrayWindowError::TooLarge);
    }
    let shape = array.shape();
    let row_end = row_offset
        .checked_add(row_count)
        .ok_or(ArrayWindowError::Overflow)?;
    let col_end = col_offset
        .checked_add(col_count)
        .ok_or(ArrayWindowError::Overflow)?;
    if row_end > shape.rows || col_end > shape.cols {
        return Err(ArrayWindowError::OutsideArray);
    }
    let cells = (row_offset..row_end)
        .map(|row| {
            (col_offset..col_end)
                .map(|col| {
                    let value = array
                        .get(row, col)
                        .expect("a cell inside the checked window");
                    WindowCell {
                        kind: kind(value),
                        text: shown(value, profile),
                    }
                })
                .collect()
        })
        .collect();
    Ok(ArrayWindow {
        total_rows: shape.rows,
        total_cols: shape.cols,
        row_offset,
        col_offset,
        cells,
    })
}

/// The first window of `array`: up to 32 rows, and as many columns as the cell bound leaves.
pub fn first_window(array: &CalcArray, profile: Option<&FormatProfile>) -> ArrayWindow {
    let shape = array.shape();
    let rows = shape.rows.min(FIRST_WINDOW_ROWS);
    let cols = shape
        .cols
        .min((MAX_ARRAY_WINDOW_CELLS / rows.max(1)).max(1));
    array_window(array, 0, 0, rows, cols, profile).expect("the first window lies inside the array")
}

/// A value's type as the surface labels it.
pub fn kind(value: &CalcValue) -> &'static str {
    match value.core() {
        CoreValue::Number(_) => "number",
        CoreValue::Text(_) => "text",
        CoreValue::Logical(_) => "logical",
        CoreValue::Error(_) => "error",
        CoreValue::Empty => "blank",
        CoreValue::Missing => "omitted argument",
        CoreValue::Array(_) => "array",
        CoreValue::Reference(_) => "reference",
    }
}

/// A value as a cell of Excel's standard column width shows it: a number in the General format of
/// `profile` (`0.3` for 0.1+0.2, `0.333333333` for 1/3), and every other value as [`plain`] gives
/// it. Without a profile a number is [`plain`]'s shortest round-trip decimal.
pub fn shown(value: &CalcValue, profile: Option<&FormatProfile>) -> String {
    match (value.core(), profile) {
        (CoreValue::Number(number), Some(profile)) => calc_func::general_format(profile, *number),
        _ => plain(value),
    }
}

/// A value alone, without its type: `7`, `ax`, `TRUE`, `#N/A`, and nothing for a blank. A number
/// is the shortest decimal that round-trips its binary64 ([`display`]'s rule).
pub fn plain(value: &CalcValue) -> String {
    match value.core() {
        CoreValue::Number(number) => number.to_string(),
        CoreValue::Text(text) => text.to_string_lossy(),
        CoreValue::Logical(true) => "TRUE".to_owned(),
        CoreValue::Logical(false) => "FALSE".to_owned(),
        CoreValue::Error(code) => display::error_spelling(*code).to_owned(),
        CoreValue::Empty => String::new(),
        CoreValue::Missing => "(omitted)".to_owned(),
        CoreValue::Array(array) => {
            let shape = array.shape();
            format!("{{{}x{} array}}", shape.rows, shape.cols)
        }
        CoreValue::Reference(_) => "(reference)".to_owned(),
    }
}

/// What a worksheet error means, in a few words.
pub fn error_meaning(code: WorksheetErrorCode) -> &'static str {
    match code {
        WorksheetErrorCode::Null => "the ranges do not intersect",
        WorksheetErrorCode::Div0 => "division by zero",
        WorksheetErrorCode::Value => "a value of the wrong type",
        WorksheetErrorCode::Ref => "a reference that is not valid",
        WorksheetErrorCode::Name => "a name that is not recognised",
        WorksheetErrorCode::Num => "a number that is not valid for the calculation",
        WorksheetErrorCode::NA => "a value is not available",
        WorksheetErrorCode::Busy => "the value is still being calculated",
        WorksheetErrorCode::GettingData => "the data is still being fetched",
        WorksheetErrorCode::Spill => "the array result cannot spill",
        WorksheetErrorCode::Calc => "the calculation cannot produce a value",
        WorksheetErrorCode::Field => "a field that is not valid",
        WorksheetErrorCode::Blocked => "access is blocked",
        WorksheetErrorCode::Connect => "a connection failed",
    }
}

/// The result as the surface lays it out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ResultView {
    /// No formula has been entered.
    #[default]
    NoFormula,
    /// A scalar value: its type, the value as a cell shows it ([`shown`]), and, for a number whose
    /// shortest round-trip decimal differs from that, the decimal.
    Value {
        kind: &'static str,
        text: String,
        exact: Option<String>,
    },
    /// A worksheet error value, which the formula calculated: its spelling and meaning.
    Error {
        code: &'static str,
        meaning: &'static str,
    },
    /// An array value, as its first window.
    Array(ArrayWindow),
    /// The diagnostics stopped the calculation: no value.
    NotCalculated,
}

impl ResultView {
    /// The view of a calculation, numbers shown under `profile` as a cell shows them.
    pub fn of(calculation: &Calculation, profile: Option<&FormatProfile>) -> ResultView {
        match &calculation.result {
            None => ResultView::NotCalculated,
            Some(value) => match value.core() {
                CoreValue::Array(array) => ResultView::Array(first_window(array, profile)),
                CoreValue::Error(code) => ResultView::Error {
                    code: display::error_spelling(*code),
                    meaning: error_meaning(*code),
                },
                _ => {
                    let text = shown(value, profile);
                    let exact = plain(value);
                    ResultView::Value {
                        kind: kind(value),
                        exact: (value.as_number().is_some() && exact != text).then_some(exact),
                        text,
                    }
                }
            },
        }
    }

    /// The type label the surface shows beside the value.
    pub fn kind(&self) -> &'static str {
        match self {
            ResultView::NoFormula => "",
            ResultView::Value { kind, .. } => kind,
            ResultView::Error { .. } => "error",
            ResultView::Array(_) => "array",
            ResultView::NotCalculated => "no value",
        }
    }
}

/// Where a result came from: the formula's cell, the route that calculated it, and the cells it
/// read. `None` when nothing was calculated.
pub fn provenance(session: &Session, calculation: &Calculation) -> Option<String> {
    calculation.result.as_ref()?;
    let cell = display::cell(session, session.formula_cell());
    let reads = calculation.reads.len();
    let cells: u64 = calculation
        .reads
        .iter()
        .map(|read| read.area.cell_count())
        .sum();
    let read_text = match (reads, cells) {
        (0, _) => "no cells read".to_owned(),
        (1, 1) => "1 cell read".to_owned(),
        (1, cells) => format!("1 area read ({cells} cells)"),
        (reads, cells) => format!("{reads} areas read ({cells} cells)"),
    };
    Some(format!(
        "calculated in {cell} by calc-fml's evaluator with calc-func's functions; {read_text}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbers(rows: usize, cols: usize) -> CalcArray {
        CalcArray::from_rows(
            (0..rows)
                .map(|r| {
                    (0..cols)
                        .map(|c| CalcValue::number((r * cols + c) as f64))
                        .collect()
                })
                .collect(),
        )
        .expect("a rectangular array")
    }

    #[test]
    fn values_and_arrays_project_with_their_types() {
        let array = CalcArray::from_rows(vec![
            vec![CalcValue::number(1.0), CalcValue::logical(true)],
            vec![
                CalcValue::error(WorksheetErrorCode::NA),
                CalcValue::number(4.0),
            ],
        ])
        .expect("2x2");
        let window = array_window(&array, 0, 0, 2, 2, None).expect("the whole array");
        assert_eq!(
            window.cells[0][1],
            WindowCell {
                kind: "logical",
                text: "TRUE".to_owned()
            }
        );
        assert_eq!(window.cells[1][0].text, "#N/A");
        assert_eq!(window.cells[1][1].text, "4");
        assert!(!window.truncated());
        let calculation = Calculation {
            result: Some(CalcValue::array(array)),
            diagnostics: Vec::new(),
            reads: Vec::new(),
        };
        assert!(matches!(
            ResultView::of(&calculation, None),
            ResultView::Array(ArrayWindow {
                total_rows: 2,
                total_cols: 2,
                ..
            })
        ));
        let error = Calculation {
            result: Some(CalcValue::error(WorksheetErrorCode::Div0)),
            ..calculation.clone()
        };
        assert_eq!(
            ResultView::of(&error, None),
            ResultView::Error {
                code: "#DIV/0!",
                meaning: "division by zero"
            }
        );
        let none = Calculation {
            result: None,
            ..calculation
        };
        assert_eq!(ResultView::of(&none, None), ResultView::NotCalculated);
    }

    #[test]
    fn numbers_show_as_a_cell_shows_them_with_the_exact_decimal_beside() {
        let profile = FormatProfile::for_culture("en-US").expect("en-US");
        let view = |value: f64| {
            ResultView::of(
                &Calculation {
                    result: Some(CalcValue::number(value)),
                    diagnostics: Vec::new(),
                    reads: Vec::new(),
                },
                Some(&profile),
            )
        };
        // Excel's General format at the standard column width: 11 characters.
        assert_eq!(
            view(0.1 + 0.2),
            ResultView::Value {
                kind: "number",
                text: "0.3".to_owned(),
                exact: Some("0.30000000000000004".to_owned())
            }
        );
        assert_eq!(
            view(1.0 / 3.0),
            ResultView::Value {
                kind: "number",
                text: "0.333333333".to_owned(),
                exact: Some("0.3333333333333333".to_owned())
            }
        );
        assert_eq!(
            view(13.5),
            ResultView::Value {
                kind: "number",
                text: "13.5".to_owned(),
                exact: None
            }
        );
        assert_eq!(shown(&CalcValue::logical(true), Some(&profile)), "TRUE");
        assert_eq!(
            shown(&CalcValue::number(1.0 / 3.0), None),
            "0.3333333333333333"
        );
    }

    #[test]
    fn array_windows_stay_bounded() {
        let large = numbers(100, 100);
        let first = first_window(&large, None);
        assert_eq!(first.cells.len(), 32);
        assert!(first.cells.len() * first.cells[0].len() <= MAX_ARRAY_WINDOW_CELLS);
        assert!(first.truncated());
        assert_eq!(
            array_window(&large, 0, 0, 100, 100, None),
            Err(ArrayWindowError::TooLarge)
        );
        assert_eq!(
            array_window(&large, 99, 0, 2, 1, None),
            Err(ArrayWindowError::OutsideArray)
        );
        assert_eq!(
            array_window(&large, 0, 0, 0, 1, None),
            Err(ArrayWindowError::Empty)
        );
        assert_eq!(
            array_window(&large, usize::MAX, 0, 2, 1, None),
            Err(ArrayWindowError::Overflow)
        );
        let window = array_window(&large, 10, 20, 2, 3, None).expect("inside");
        assert_eq!(window.cells[1][2].text, (11 * 100 + 22).to_string());
    }
}
