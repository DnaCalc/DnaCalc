//! A calculation's result as both apps show it: its value type, the value itself, an array as a
//! bounded grid window and an error with what it means.
//!
//! The array window is ported from DnaTreeCalc's formula UX core
//! (`src/dnacalc-formula-ux-core/src/lib.rs` at DnaTreeCalc c68ecaa: `project_runtime_result`,
//! `project_array_window`, and `MAX_ARRAY_WINDOW_CELLS` from `src/dnacalc-skin-ir/src/formula.rs`)
//! onto calc-value's `CalcArray`, without the skin IR between them. Nothing here computes a
//! value: it lays out the value a calculation returned.

use crate::display;
use calc_func::locale::FormatProfile;
use calc_func::value::{CalcArray, CalcValue, CoreValue, WorksheetErrorCode};

/// The most cells an array window holds.
pub const MAX_ARRAY_WINDOW_CELLS: usize = 4_096;

/// A host-supplied value state. Calculation and numerical agreement are separate facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueState {
    Empty,
    Literal,
    Calculated,
    FileCache,
    Stale,
    Pending,
    Refused,
    WorksheetError,
}

impl ValueState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Empty => "Empty",
            Self::Literal => "Literal",
            Self::Calculated => "Calculated",
            Self::FileCache => "File cache",
            Self::Stale => "Stale",
            Self::Pending => "Pending",
            Self::Refused => "Refused",
            Self::WorksheetError => "Worksheet error",
        }
    }

    pub const fn explanation(self) -> &'static str {
        match self {
            Self::Empty => "No value in this cell.",
            Self::Literal => "A value entered directly into the worksheet.",
            Self::Calculated => {
                "Calculated by DNA Calc. Agreement with Excel has not been checked here."
            }
            Self::FileCache => "A value stored in the opened file. DNA Calc has not calculated it.",
            Self::Stale => "This value is out of date. Calculate to update it.",
            Self::Pending => "Workbook work is in progress. The displayed value may change.",
            Self::Refused => "DNA Calc could not calculate this formula. Read the reason below.",
            Self::WorksheetError => "The cell holds a worksheet error. Its source is shown below.",
        }
    }
}

/// The most rows an array window shows at first.
const FIRST_WINDOW_ROWS: usize = 32;

/// The most cells a scrolling window holds (an app whose result panel scrolls and draws only the
/// rows in view, DNA OneCalc), the most rows and the most columns.
pub const MAX_SCROLL_WINDOW_CELLS: usize = 50_000;
pub const MAX_SCROLL_WINDOW_ROWS: usize = 10_000;
pub const MAX_SCROLL_WINDOW_COLS: usize = 200;

/// How much of an array result a view holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Windowing {
    /// The first window: up to 32 rows within [`MAX_ARRAY_WINDOW_CELLS`] cells.
    #[default]
    First,
    /// A window of up to [`MAX_SCROLL_WINDOW_CELLS`] cells for a panel that scrolls over it.
    Scrolling,
}

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
    window_within(
        array,
        row_offset,
        col_offset,
        row_count,
        col_count,
        profile,
        MAX_ARRAY_WINDOW_CELLS,
    )
}

fn window_within(
    array: &CalcArray,
    row_offset: usize,
    col_offset: usize,
    row_count: usize,
    col_count: usize,
    profile: Option<&FormatProfile>,
    limit: usize,
) -> Result<ArrayWindow, ArrayWindowError> {
    if row_count == 0 || col_count == 0 {
        return Err(ArrayWindowError::Empty);
    }
    if row_count
        .checked_mul(col_count)
        .ok_or(ArrayWindowError::Overflow)?
        > limit
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

/// The window of `array` a scrolling panel draws from: up to [`MAX_SCROLL_WINDOW_ROWS`] rows and
/// [`MAX_SCROLL_WINDOW_COLS`] columns within [`MAX_SCROLL_WINDOW_CELLS`] cells, from the top left.
pub fn scrolling_window(array: &CalcArray, profile: Option<&FormatProfile>) -> ArrayWindow {
    let shape = array.shape();
    let cols = shape.cols.min(MAX_SCROLL_WINDOW_COLS).max(1);
    let rows = shape
        .rows
        .min(MAX_SCROLL_WINDOW_ROWS)
        .min((MAX_SCROLL_WINDOW_CELLS / cols).max(1));
    window_within(array, 0, 0, rows, cols, profile, MAX_SCROLL_WINDOW_CELLS)
        .expect("the scrolling window lies inside the array")
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
    if value.is_excel_overflow() {
        return calc_func::general_format(
            &calc_func::locale::format_profile(calc_func::locale::LocaleId::EnUs),
            value.as_number().expect("qualified numeric encoding"),
        );
    }
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
    /// The view of a calculation's result (`None`: the diagnostics stopped it), numbers shown
    /// under `profile` as a cell shows them.
    pub fn of(result: Option<&CalcValue>, profile: Option<&FormatProfile>) -> ResultView {
        Self::windowed(result, profile, Windowing::First)
    }

    /// [`ResultView::of`] with an array held as `windowing` says.
    pub fn windowed(
        result: Option<&CalcValue>,
        profile: Option<&FormatProfile>,
        windowing: Windowing,
    ) -> ResultView {
        match result {
            None => ResultView::NotCalculated,
            Some(value) => match value.core() {
                CoreValue::Array(array) => ResultView::Array(match windowing {
                    Windowing::First => first_window(array, profile),
                    Windowing::Scrolling => scrolling_window(array, profile),
                }),
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

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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
        assert!(matches!(
            ResultView::of(Some(&CalcValue::array(array)), None),
            ResultView::Array(ArrayWindow {
                total_rows: 2,
                total_cols: 2,
                ..
            })
        ));
        assert_eq!(
            ResultView::of(Some(&CalcValue::error(WorksheetErrorCode::Div0)), None),
            ResultView::Error {
                code: "#DIV/0!",
                meaning: "division by zero"
            }
        );
        assert_eq!(ResultView::of(None, None), ResultView::NotCalculated);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn numbers_show_as_a_cell_shows_them_with_the_exact_decimal_beside() {
        let profile = FormatProfile::for_culture("en-US").expect("en-US");
        let view = |value: f64| ResultView::of(Some(&CalcValue::number(value)), Some(&profile));
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

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_scrolling_window_holds_thousands_of_rows_within_its_bounds() {
        let tall = numbers(30_000, 3);
        let window = scrolling_window(&tall, None);
        assert_eq!(window.cells.len(), MAX_SCROLL_WINDOW_ROWS);
        assert_eq!(window.cells[0].len(), 3);
        assert_eq!(window.total_rows, 30_000);
        assert!(window.truncated());
        assert_eq!(window.cells[9_999][2].text, (9_999 * 3 + 2).to_string());
        let wide = numbers(1_000, 1_000);
        let window = scrolling_window(&wide, None);
        assert_eq!(window.cells[0].len(), MAX_SCROLL_WINDOW_COLS);
        assert!(window.cells.len() * window.cells[0].len() <= MAX_SCROLL_WINDOW_CELLS);
        // The first window of the same array is unchanged: 32 rows within 4,096 cells.
        assert_eq!(first_window(&wide, None).cells.len(), 32);
        let small = numbers(2, 2);
        assert!(!scrolling_window(&small, None).truncated());
        assert!(matches!(
            ResultView::windowed(Some(&CalcValue::array(tall)), None, Windowing::Scrolling),
            ResultView::Array(ArrayWindow {
                total_rows: 30_000,
                ..
            })
        ));
    }
}
/// One entry of a format picker: the group it is listed under, its name, and its format code in the
/// locale's separators (the form the format box takes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatChoice {
    pub group: &'static str,
    pub label: &'static str,
    pub code: String,
}

/// The formats a picker offers for `profile`: General, number, percent, scientific, currency, date, time and text, each code written with the profile's decimal and thousands
/// separators, and fractions. A locale whose thousands separator is a space (en-ZA, fr-FR) offers
/// only the improper fractions: Excel reads the space in `# ?/?` there as the group separator and
/// refuses the code. A person can still type any code; the picker only names the common ones.
pub fn format_choices(profile: &FormatProfile) -> Vec<FormatChoice> {
    let (d, t) = (profile.decimal_separator, profile.thousands_separator);
    let choice = |group, label, code: String| FormatChoice { group, label, code };
    let spaced = t.chars().all(char::is_whitespace);
    let fractions: Vec<FormatChoice> = if spaced {
        vec![
            choice("Fraction", "Improper, up to one digit", "?/?".to_owned()),
            choice("Fraction", "Improper, up to two digits", "??/??".to_owned()),
            choice("Fraction", "Improper, eighths", "?/8".to_owned()),
        ]
    } else {
        vec![
            choice("Fraction", "Up to one digit", "# ?/?".to_owned()),
            choice("Fraction", "Up to two digits", "# ??/??".to_owned()),
            choice("Fraction", "Up to three digits", "# ???/???".to_owned()),
            choice("Fraction", "Halves", "# ?/2".to_owned()),
            choice("Fraction", "Quarters", "# ?/4".to_owned()),
            choice("Fraction", "Eighths", "# ?/8".to_owned()),
            choice("Fraction", "Sixteenths", "# ??/16".to_owned()),
            choice("Fraction", "Hundredths", "# ?/100".to_owned()),
            choice("Fraction", "Improper, up to one digit", "?/?".to_owned()),
        ]
    };
    let mut choices = vec![
        choice("General", "General", "General".to_owned()),
        choice("Number", "Integer", "0".to_owned()),
        choice("Number", "Two decimals", format!("0{d}00")),
        choice("Number", "Thousands separator", format!("#{t}##0")),
        choice("Number", "Thousands, two decimals", format!("#{t}##0{d}00")),
        choice("Percent", "Percent", "0%".to_owned()),
        choice("Percent", "Percent, two decimals", format!("0{d}00%")),
        choice("Scientific", "Scientific", format!("0{d}00E+00")),
        choice("Currency", "Dollar", format!("$#{t}##0{d}00")),
        choice(
            "Currency",
            "Dollar, negatives in red",
            format!("$#{t}##0{d}00;[Red]-$#{t}##0{d}00"),
        ),
        choice("Date", "Year-month-day", "yyyy-mm-dd".to_owned()),
        choice("Date", "Day/month/year", "d/m/yyyy".to_owned()),
        choice("Date", "Day month-name year", "dd-mmm-yy".to_owned()),
        choice("Date", "Long date", "dddd d mmmm yyyy".to_owned()),
        choice("Time", "Time", "h:mm:ss".to_owned()),
        choice("Time", "Time, 12-hour", "h:mm AM/PM".to_owned()),
        choice("Time", "Elapsed hours", "[h]:mm:ss".to_owned()),
        choice(
            "Date and time",
            "Date and time",
            "yyyy-mm-dd hh:mm:ss".to_owned(),
        ),
        choice("Text", "Text", "@".to_owned()),
    ];
    let at = choices
        .iter()
        .position(|c| c.group == "Date")
        .unwrap_or(choices.len());
    choices.splice(at..at, fractions);
    choices
}

/// Applies an Excel number format to the shared scalar/array window. Formatting never evaluates.
pub fn formatted_view(
    result: Option<&CalcValue>,
    profile: Option<&FormatProfile>,
    code: &str,
    date_system: calc_func::locale::DateSystem,
) -> Result<ResultView, String> {
    formatted_view_windowed(result, profile, code, date_system, Windowing::First)
}

/// A value as a cell shows it under the format `code` (see [`formatted_view`]).
pub fn formatted_text(
    value: &CalcValue,
    profile: &FormatProfile,
    code: &str,
    date_system: calc_func::locale::DateSystem,
) -> Result<String, String> {
    if !matches!(value.core(), CoreValue::Number(_) | CoreValue::Text(_)) {
        return Ok(shown(value, Some(profile)));
    }
    calc_func::cell_text_localized(profile, date_system, value, code).map_err(|e| match e {
        calc_func::CellFormatError::Unshowable => {
            "This value cannot be shown in that format".to_owned()
        }
        calc_func::CellFormatError::NotModelled => "This number format is not modelled".to_owned(),
    })
}

/// [`formatted_view`] with an array held as `windowing` says.
pub fn formatted_view_windowed(
    result: Option<&CalcValue>,
    profile: Option<&FormatProfile>,
    code: &str,
    date_system: calc_func::locale::DateSystem,
    windowing: Windowing,
) -> Result<ResultView, String> {
    let mut view = ResultView::windowed(result, profile, windowing);
    if code.trim().is_empty() || code.eq_ignore_ascii_case("General") {
        return Ok(view);
    }
    let Some(profile) = profile else {
        return Err("The selected number-format locale is unavailable".into());
    };
    let format = |value: &CalcValue| formatted_text(value, profile, code, date_system);
    match (&mut view, result) {
        (ResultView::Value { text, exact, .. }, Some(value)) => {
            *text = format(value)?;
            *exact = (value.as_number().is_some() && plain(value) != *text).then(|| plain(value));
        }
        (ResultView::Array(window), Some(value)) => {
            if let CoreValue::Array(array) = value.core() {
                for (r, row) in window.cells.iter_mut().enumerate() {
                    for (c, cell) in row.iter_mut().enumerate() {
                        cell.text = format(
                            array
                                .get(window.row_offset + r, window.col_offset + c)
                                .expect("the shared window lies inside the array"),
                        )?;
                    }
                }
            }
        }
        _ => {}
    }
    Ok(view)
}

#[cfg(test)]
mod formatted_results {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn qualified_overflow_uses_measured_general_and_refuses_custom_formats() {
        let profile = FormatProfile::for_culture("en-US").unwrap();
        for (bits, expected) in [
            (0x7ff1_ccf3_85eb_c8a0, "2E+308"),
            (0x7ff0_0000_0000_000a, "1.8E+308"),
            (0xfff1_ccf3_85eb_c8a0, "2E+308"),
        ] {
            let value = CalcValue::excel_overflow_number(f64::from_bits(bits)).unwrap();
            assert_eq!(plain(&value), expected);
            assert_eq!(shown(&value, Some(&profile)), expected);
            assert!(
                matches!(formatted_view(Some(&value),Some(&profile),"General",calc_func::locale::DateSystem::System1900),Ok(ResultView::Value {text,..}) if text==expected)
            );
            for code in ["0", "0.00E+00", "yyyy-mm-dd", "@"] {
                assert!(
                    formatted_view(
                        Some(&value),
                        Some(&profile),
                        code,
                        calc_func::locale::DateSystem::System1900
                    )
                    .is_err(),
                    "{code}"
                );
            }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn every_picker_choice_formats_a_number_in_every_locale() {
        for culture in ["en-US", "en-GB", "en-ZA", "de-DE", "fr-FR"] {
            let profile = FormatProfile::for_culture(culture).unwrap();
            for choice in format_choices(&profile) {
                let value = CalcValue::number(1234.5);
                for system in [
                    calc_func::locale::DateSystem::System1900,
                    calc_func::locale::DateSystem::System1904,
                ] {
                    let view = formatted_view(Some(&value), Some(&profile), &choice.code, system);
                    assert!(
                        view.is_ok(),
                        "{culture} {} {:?}: {view:?}",
                        choice.label,
                        choice.code
                    );
                }
            }
        }
        // The date system moves the date a serial number is.
        let profile = FormatProfile::for_culture("en-US").unwrap();
        let value = CalcValue::number(43860.0);
        let show = |system| match formatted_view(Some(&value), Some(&profile), "yyyy-mm-dd", system)
        {
            Ok(ResultView::Value { text, .. }) => text,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            show(calc_func::locale::DateSystem::System1904),
            "2024-01-31"
        );
        assert_eq!(
            show(calc_func::locale::DateSystem::System1900),
            "2020-01-30"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn scalar_and_array_formats_share_the_cell_formatter() {
        let profile = FormatProfile::for_culture("de-DE").unwrap();
        let value = CalcValue::number(3.5);
        assert!(
            matches!(formatted_view(Some(&value),Some(&profile),"0,00",calc_func::locale::DateSystem::System1900),Ok(ResultView::Value {text,exact:Some(exact),..}) if text=="3,50" && exact=="3.5")
        );
        let array = CalcValue::array(
            CalcArray::from_rows(vec![vec![CalcValue::number(1.0), CalcValue::number(2.5)]])
                .unwrap(),
        );
        let ResultView::Array(window) = formatted_view(
            Some(&array),
            Some(&profile),
            "0,00",
            calc_func::locale::DateSystem::System1900,
        )
        .unwrap() else {
            panic!("array view")
        };
        assert_eq!(
            window.cells[0]
                .iter()
                .map(|c| c.text.as_str())
                .collect::<Vec<_>>(),
            ["1,00", "2,50"]
        );
        let error = CalcValue::error(WorksheetErrorCode::Div0);
        assert!(matches!(
            formatted_view(
                Some(&error),
                Some(&profile),
                "0,00",
                calc_func::locale::DateSystem::System1900
            ),
            Ok(ResultView::Error {
                code: "#DIV/0!",
                ..
            })
        ));
    }
}
