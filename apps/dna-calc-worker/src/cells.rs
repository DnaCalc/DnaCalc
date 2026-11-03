//! A projected cell as the skins show it: its text, the type of its value, and where the value
//! came from. Every value shown is the one calc-host projects ([`calc_host::project`] or a
//! subscription's delta), and its provenance travels with it: a value the engine calculated, a
//! literal, the file's cached value (shown as such, never as calculated), a refusal (shown as
//! refused, never as a value), or a stale value.
//!
//! The value text is the one DNA OneCalc shows too: a number in the General format of the host's
//! locale profile ([`dna_formula_ui::result::shown`]), an error by its spelling. So are the labels
//! of cells, areas and refusals ([`dna_formula_ui::display`]), over the workbook's sheet names.

use calc_host::value::{CalcValue, CellKey, CoreValue, ExcelText, GridArea, SheetId};
use calc_host::{
    CellInput, FormatProfile, HostState, NotModeled, ProjectedCell, ProjectedValue, StaleCause,
};
use dna_formula_ui::display::{self, SheetNames};
use dna_formula_ui::result;

pub use dna_calc_model::projection::{CellShown, ConditionalShown, FormatShown, Provenance};

/// The sheet names of a host, for the shared labels ([`dna_formula_ui::display`]).
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

pub use dna_formula_ui::display::{address, column_letters, parse_address};

fn kind(value: &CalcValue) -> &'static str {
    match value.core() {
        CoreValue::Empty => "",
        _ => result::kind(value),
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
        style: Default::default(),
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
pub use calc_host::search::entry_text as input_text;

/// Presentation source only. Clipboard, search and saved inputs retain canonical text.
pub fn display_input(input: Option<&calc_host::CellInput>) -> String {
    let text = input_text(input);
    if matches!(input, Some(calc_host::CellInput::Formula(_))) {
        display::formula(&text)
    } else {
        text
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

/// Only ownership present in the backend's typed refusal travels to the page.
pub fn refusal_owner(cell: &ProjectedCell) -> Option<String> {
    let ProjectedValue::NotModeled { refusal, .. } = cell.value() else {
        return None;
    };
    match refusal {
        NotModeled::Compile(error) => display::compile_owner(error).map(str::to_owned),
        NotModeled::Evaluation(fault) => display::eval_owner(fault).map(str::to_owned),
        _ => None,
    }
}

pub fn error_meaning(cell: &ProjectedCell) -> Option<String> {
    let value = match cell.value() {
        ProjectedValue::Literal(value) | ProjectedValue::Calculated { value, .. } => value.clone(),
        ProjectedValue::FileCached(cache) => cache.value().to_value(),
        ProjectedValue::Stale(stale) => stale
            .last_value()
            .cloned()
            .or_else(|| stale.file_cache().map(|cache| cache.value().to_value()))?,
        _ => return None,
    };
    match value.core() {
        CoreValue::Error(code) => Some(result::error_meaning(*code).to_owned()),
        _ => None,
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

/// The same indexed-colour resolution for stored and conditional styles.
pub fn indexed_color(host: &HostState, index: u32) -> Option<String> {host.indexed_color(index)}
pub fn style_color(host: &HostState, color: &calc_host::StyleColor) -> Option<String> {host.resolved_style_color(color)}
