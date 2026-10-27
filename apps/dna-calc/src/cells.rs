//! A projected cell as the skins show it: its text, the type of its value, and where the value
//! came from. Every value shown is the one calc-host projects ([`calc_host::project`] or a
//! subscription's delta), and its provenance travels with it: a value the engine calculated, a
//! literal, the file's cached value (shown as such, never as calculated), a refusal (shown as
//! refused, never as a value), or a stale value.
//!
//! The value text is DNA OneCalc's: a number in the General format of the host's locale profile
//! ([`dna_onecalc::result::shown`]), an error by its spelling. The labels of cells, areas and
//! refusals are DNA OneCalc's too ([`dna_onecalc::display`]), over the workbook's sheet names.

use calc_host::value::{CalcValue, CellKey, CoreValue, ExcelText, GridArea, SheetId};
use calc_host::{
    CellInput, FormatProfile, HostState, NotModeled, ProjectedCell, ProjectedValue, StaleCause,
};
use dna_onecalc::display::{self, SheetNames};
use dna_onecalc::result;

/// Where a shown value came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Provenance {
    /// The cell holds nothing.
    Empty,
    /// The cell's input is a literal, which is its value.
    Literal,
    /// The engine calculated the value, and it still stands.
    Calculated,
    /// The value the file stored for the formula; nothing here calculated it.
    FileCache,
    /// The engine refused the formula: it has no value.
    Refused,
    /// The formula has no value that stands at the current revision.
    Stale,
}

impl Provenance {
    /// The name the page carries in `data-provenance` and the class suffix.
    pub const fn name(self) -> &'static str {
        match self {
            Provenance::Empty => "empty",
            Provenance::Literal => "literal",
            Provenance::Calculated => "calculated",
            Provenance::FileCache => "file-cache",
            Provenance::Refused => "refused",
            Provenance::Stale => "stale",
        }
    }
}

/// One cell as a skin shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct CellShown {
    pub at: CellKey,
    /// The text in the cell: the value as a cell of standard width shows it, `refused` for a
    /// refusal, a stale cell's last value, or nothing.
    pub text: String,
    /// The value's type: `number`, `text`, `logical`, `error`, `array`, or empty.
    pub kind: &'static str,
    pub provenance: Provenance,
    /// Whether the cell's input is a formula.
    pub formula: bool,
    /// How the cell's number format applied.
    pub format: FormatShown,
    /// The color the number format gives the text (`[Red]`, `[Color10]`), as `#rrggbb`.
    pub color: Option<String>,
    /// What the sheet's conditional formats give the cell (dc-6yw), its colours resolved.
    pub conditional: Option<ConditionalShown>,
}

/// A cell's look under its conditional formats, as the grid draws it: colours as `#rrggbb`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConditionalShown {
    pub fill: Option<String>,
    pub font_color: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// A data bar: the share of the cell's width (0 to 1), its colour, whether it is a gradient.
    pub bar: Option<(f64, String, bool)>,
    /// An icon: its set's name and its index.
    pub icon: Option<(String, usize)>,
    /// The value is hidden (a data bar or icon set that shows no value).
    pub hide_value: bool,
}

/// The sheet names of a host, for DNA OneCalc's labels.
pub struct WorkbookNames<'a>(pub &'a HostState);

impl SheetNames for WorkbookNames<'_> {
    fn sheet_name_of(&self, sheet: SheetId) -> Option<String> {
        self.0
            .sheets()
            .iter()
            .find(|entry| entry.id() == sheet)
            .map(|entry| entry.name().text().to_string_lossy())
    }
}

/// A cell's label: `Sheet1!B3`.
pub fn cell_label(host: &HostState, at: CellKey) -> String {
    display::cell(&WorkbookNames(host), at)
}

/// An area's label: `Sheet1!A1:B3`.
pub fn area_label(host: &HostState, area: GridArea) -> String {
    display::area(&WorkbookNames(host), area)
}

pub use dna_onecalc::display::column_letters;

/// A cell's address without its sheet: `B3`.
pub fn address(row: u32, column: u32) -> String {
    format!("{}{row}", column_letters(column))
}

/// The row and column of an address such as `B3` or `$B$3` (any case), when it names a cell of
/// the grid.
pub fn parse_address(text: &str) -> Option<(u32, u32)> {
    let text = text.trim().replace('$', "").to_ascii_uppercase();
    let split = text.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = text.split_at(split);
    if letters.is_empty() || letters.len() > 3 || !letters.bytes().all(|b| b.is_ascii_uppercase()) {
        return None;
    }
    let column = letters
        .bytes()
        .fold(0u32, |n, b| n * 26 + u32::from(b - b'A' + 1));
    let row: u32 = digits.parse().ok()?;
    ((1..=1_048_576).contains(&row) && (1..=16_384).contains(&column)).then_some((row, column))
}

fn kind(value: &CalcValue) -> &'static str {
    match value.core() {
        CoreValue::Empty => "",
        _ => result::kind(value),
    }
}

/// How a cell's number format applied to what it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FormatShown {
    /// The General format (or no value to format).
    General,
    /// The cell's format code, rendered by calc-func's TEXT renderer.
    Applied,
    /// The renderer does not model the code: the value is shown in General instead.
    NotModelled,
    /// The value cannot be shown under the code (a negative date): `#` as Excel's cell shows it.
    Unshowable,
}

impl FormatShown {
    /// The name the page carries in `data-format`.
    pub const fn name(self) -> &'static str {
        match self {
            FormatShown::General => "general",
            FormatShown::Applied => "applied",
            FormatShown::NotModelled => "not-modelled",
            FormatShown::Unshowable => "unshowable",
        }
    }
}

/// A cell's number format: its code and the workbook's date system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellFormat<'a> {
    pub code: &'a str,
    /// The workbook calculates in the 1904 date system (calc-host's `declared_date1904`).
    pub date1904: bool,
    /// The workbook's redefined palette entries, 1 to 56 (calc-host's `palette_color`); `None`
    /// for Excel's default palette.
    pub palette: Option<&'a [Option<String>]>,
}

/// A value as a cell shows it under `format` (`None`, or the code `General`, for General): numbers
/// and text through calc-func's [`calc_func::cell_text`], the one renderer TEXT uses, in the
/// workbook's date system.
fn formatted(
    value: &CalcValue,
    profile: Option<&FormatProfile>,
    format: Option<CellFormat<'_>>,
) -> (String, FormatShown) {
    let general = || result::shown(value, profile);
    let (Some(CellFormat { code, date1904, .. }), Some(profile)) = (format, profile) else {
        return (general(), FormatShown::General);
    };
    let system = if date1904 {
        calc_func::locale::DateSystem::System1904
    } else {
        calc_func::locale::DateSystem::System1900
    };
    if code.eq_ignore_ascii_case("general")
        || !matches!(value.core(), CoreValue::Number(_) | CoreValue::Text(_))
    {
        return (general(), FormatShown::General);
    }
    match calc_func::cell_text(profile, system, value, code) {
        Ok(text) => (text, FormatShown::Applied),
        Err(calc_func::CellFormatError::Unshowable) => {
            ("#####".to_owned(), FormatShown::Unshowable)
        }
        Err(calc_func::CellFormatError::NotModelled) => (general(), FormatShown::NotModelled),
    }
}

/// A projected cell as the grid shows it, numbers under `profile` and the cell's number format
/// code `format` (calc-host's `HostState::number_format`).
pub fn shown(
    cell: &ProjectedCell,
    profile: Option<&FormatProfile>,
    format: Option<CellFormat<'_>>,
) -> CellShown {
    let formula = matches!(cell.input(), Some(CellInput::Formula(_)));
    let mut color = None;
    let mut value_of = |value: &CalcValue, provenance: Provenance| {
        let (text, shown) = formatted(value, profile, format);
        if shown == FormatShown::Applied {
            color = format.and_then(|format| {
                let color = calc_func::cell_color(format.code, value)?;
                Some(match (color, format.palette) {
                    (calc_func::FormatColor::Palette(index), Some(palette)) => palette
                        .get(usize::from(index).saturating_sub(1))
                        .cloned()
                        .flatten()
                        .unwrap_or_else(|| color.rgb().to_owned()),
                    _ => color.rgb().to_owned(),
                })
            });
        }
        (text, self::kind(value), provenance, shown)
    };
    let (text, kind, provenance, format) = match cell.value() {
        ProjectedValue::Empty => (String::new(), "", Provenance::Empty, FormatShown::General),
        ProjectedValue::Literal(value) => value_of(value, Provenance::Literal),
        ProjectedValue::FileCached(cache) => {
            value_of(&cache.value().to_value(), Provenance::FileCache)
        }
        ProjectedValue::Calculated { value, .. } => value_of(value, Provenance::Calculated),
        ProjectedValue::NotModeled { .. } => (
            "refused".to_owned(),
            "",
            Provenance::Refused,
            FormatShown::General,
        ),
        ProjectedValue::Stale(stale) => {
            let last = stale
                .last_value()
                .cloned()
                .or_else(|| stale.file_cache().map(|cache| cache.value().to_value()));
            match last {
                Some(value) => value_of(&value, Provenance::Stale),
                None => (String::new(), "", Provenance::Stale, FormatShown::General),
            }
        }
    };
    CellShown {
        at: cell.at(),
        text,
        kind,
        provenance,
        formula,
        format,
        color,
        conditional: None,
    }
}

/// The text the formula bar shows for a cell's input, as it would be typed: `=` and the formula,
/// a number as its shortest round-trip decimal, a text (with a leading `'` when the entry would
/// otherwise read it as something else), `TRUE`/`FALSE`, an error by its spelling.
pub fn input_text(input: Option<&CellInput>) -> String {
    match input {
        None => String::new(),
        Some(CellInput::Formula(formula)) => format!("={}", formula.text().to_string_lossy()),
        Some(CellInput::Number(number)) => number.get().to_string(),
        Some(CellInput::Text(text)) => {
            let text = text.text().to_string_lossy();
            match calc_host::interpret_entry(&text) {
                Ok(Some(CellInput::Text(_))) => text,
                _ => format!("'{text}"),
            }
        }
        Some(CellInput::Logical(true)) => "TRUE".to_owned(),
        Some(CellInput::Logical(false)) => "FALSE".to_owned(),
        Some(CellInput::Error(code)) => display::error_spelling(*code).to_owned(),
    }
}

/// Why the engine refused a formula, in words.
pub fn refusal_text(host: &HostState, refusal: &NotModeled) -> String {
    match refusal {
        NotModeled::FormulaText => "the formula text holds an unpaired surrogate".to_owned(),
        NotModeled::Compile(error) => display::compile_error(error),
        NotModeled::Evaluation(fault) => display::eval_fault(&WorkbookNames(host), fault),
        NotModeled::Result(value) => format!(
            "the formula evaluated to {}, which no cell holds here",
            display::value(value)
        ),
    }
}

/// Where a cell's value came from, in a sentence: the status line and the cell's tooltip.
pub fn provenance_text(host: &HostState, cell: &ProjectedCell) -> String {
    match cell.value() {
        ProjectedValue::Empty => "empty".to_owned(),
        ProjectedValue::Literal(_) => "a literal value, as entered".to_owned(),
        ProjectedValue::FileCached(cache) => format!(
            "the value the file stored ({}); not calculated by DNA Calc",
            cache.source().name()
        ),
        ProjectedValue::Calculated { calculation, .. } => format!(
            "calculated by DNA Calc's engine at revision {}",
            calculation.revision().get()
        ),
        ProjectedValue::NotModeled { refusal, .. } => {
            format!("refused: {}", refusal_text(host, refusal))
        }
        ProjectedValue::Stale(stale) => {
            let why = match stale.cause() {
                StaleCause::AwaitingRecalc => "awaiting recalculation",
                StaleCause::RecalcFailed => "the last recalculation failed",
                StaleCause::FileCacheUnverified => {
                    "the file's stored value, which an edit since opening may have changed"
                }
            };
            match stale.last_value() {
                Some(value) => format!("stale ({why}); last calculated {}", display::value(value)),
                None => format!("stale ({why})"),
            }
        }
    }
}

/// Text as a cell's [`ExcelText`].
pub fn excel_text(text: &str) -> ExcelText {
    ExcelText::from_utf16_code_units(text.encode_utf16().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn addresses_round_trip() {
        assert_eq!(address(3, 2), "B3");
        assert_eq!(column_letters(27), "AA");
        assert_eq!(column_letters(16_384), "XFD");
        assert_eq!(parse_address("b3"), Some((3, 2)));
        assert_eq!(parse_address("$AA$10"), Some((10, 27)));
        assert_eq!(parse_address("XFD1048576"), Some((1_048_576, 16_384)));
        assert_eq!(parse_address("XFE1"), None);
        assert_eq!(parse_address("A0"), None);
        assert_eq!(parse_address("Sheet1"), None);
        assert_eq!(parse_address("12"), None);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_formula_bar_shows_an_input_as_it_would_be_typed() {
        let entry = |text: &str| calc_host::interpret_entry(text).unwrap();
        for typed in ["=SUM(A1:A3)", "2.5", "TRUE", "#N/A", "hello", "'123"] {
            assert_eq!(input_text(entry(typed).as_ref()), typed);
        }
        assert_eq!(input_text(None), "");
    }
}
