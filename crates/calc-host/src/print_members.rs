//! The PageSetup members a macro reads and writes (dc-oxh8w.8), as pure functions over the one
//! stored [`PageSetup`] and the scoped `Print_Area` / `Print_Titles` names. The automation handlers
//! in `automation/print.rs` only translate between `AutoValue` and [`PrintValue`]; the state is the
//! same state the Page Setup UI edits (`PageSetupChange`), so there is one route.
//!
//! Defaults are the OOXML defaults of an absent attribute. Where Excel's answer is not yet
//! measured the member is listed in `~/codex-runs/cl3/RUN-ON-WINDOWS.txt` (question PS1).

use crate::{HostState, PageSetupChange, print_name, print_references};
use calc_doc_model::{PageBreak, PageSetup};
use calc_value::{GridArea, SheetId};

/// A value a PageSetup member takes or returns, before Automation typing.
#[derive(Clone, Debug, PartialEq)]
pub enum PrintValue {
    /// A Boolean (`True`/`False`).
    Bool(bool),
    /// A whole number or enumeration constant.
    Int(i64),
    /// A real number (margins, in points).
    Real(f64),
    /// Text.
    Text(String),
}

/// Why a PageSetup member write was refused: Excel's 1004 or a type mismatch (13).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrintError {
    /// "Application-defined or object-defined error" (1004).
    Refused,
    /// "Type mismatch" (13).
    Mismatch,
}

/// Every PageSetup member this module serves.
pub const MEMBERS: &[&str] = &[
    "Orientation",
    "Zoom",
    "FitToPagesWide",
    "FitToPagesTall",
    "LeftMargin",
    "RightMargin",
    "TopMargin",
    "BottomMargin",
    "HeaderMargin",
    "FooterMargin",
    "CenterHorizontally",
    "CenterVertically",
    "PrintGridlines",
    "PrintHeadings",
    "BlackAndWhite",
    "Draft",
    "Order",
    "PaperSize",
    "FirstPageNumber",
    "PrintComments",
    "PrintErrors",
    "LeftHeader",
    "CenterHeader",
    "RightHeader",
    "LeftFooter",
    "CenterFooter",
    "RightFooter",
    "AlignMarginsHeaderFooter",
    "DifferentFirstPageHeaderFooter",
    "OddAndEvenPagesHeaderFooter",
    "ScaleWithDocHeaderFooter",
    "PrintArea",
    "PrintTitleRows",
    "PrintTitleColumns",
];

/// `xlAutomatic`.
pub const XL_AUTOMATIC: i64 = -4105;
const XL_PORTRAIT: i64 = 1;
const XL_LANDSCAPE: i64 = 2;
const XL_DOWN_THEN_OVER: i64 = 1;
const XL_OVER_THEN_DOWN: i64 = 2;
const XL_PRINT_NO_COMMENTS: i64 = -4142;
const XL_PRINT_SHEET_END: i64 = 1;
const XL_PRINT_IN_PLACE: i64 = 16;
const ROWS: u32 = 1_048_576;
const COLUMNS: u32 = 16_384;

fn flag(setup: &PageSetup, section: &str, name: &str, default: bool) -> bool {
    match setup.get(section, name) {
        Some("true" | "1") => true,
        Some("false" | "0") => false,
        _ => default,
    }
}

fn bool_text(value: bool) -> Option<String> {
    Some(if value { "1" } else { "0" }.into())
}

fn margin(setup: &PageSetup, name: &str) -> f64 {
    let default = match name {
        "top" | "bottom" => 0.75,
        "header" | "footer" => 0.3,
        _ => 0.7,
    };
    setup
        .get("pageMargins", name)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(default)
        * 72.0
}

/// Split an OOXML header/footer string (`&Lleft&Ccentre&Rright`) into its three sections. Text
/// before any section code is the centre section, as in Excel.
pub fn header_sections(text: &str) -> [String; 3] {
    let mut out = [String::new(), String::new(), String::new()];
    let mut at = 1;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            match chars.peek() {
                Some('L') => {
                    at = 0;
                    chars.next();
                }
                Some('C') => {
                    at = 1;
                    chars.next();
                }
                Some('R') => {
                    at = 2;
                    chars.next();
                }
                Some(&n) => {
                    out[at].push('&');
                    out[at].push(n);
                    chars.next();
                }
                None => out[at].push('&'),
            }
        } else {
            out[at].push(c);
        }
    }
    out
}

/// Join three sections back into the OOXML string; empty sections are left out.
pub fn header_join(sections: &[String; 3]) -> String {
    let mut out = String::new();
    for (code, text) in ["&L", "&C", "&R"].into_iter().zip(sections) {
        if !text.is_empty() {
            out.push_str(code);
            out.push_str(text);
        }
    }
    out
}

fn column_letters(column: u32) -> String {
    let mut n = column;
    let mut out = String::new();
    while n > 0 {
        n -= 1;
        out.insert(0, (b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    out
}

/// A print range as Excel's A1 text: `$A$1`, `$A$1:$B$2`, `$1:$3` or `$A:$C`.
pub fn area_text(area: &GridArea) -> String {
    let (a, b) = (area.first(), area.last());
    if a.column() == 1 && b.column() == COLUMNS {
        format!("${}:${}", a.row(), b.row())
    } else if a.row() == 1 && b.row() == ROWS {
        format!("${}:${}", column_letters(a.column()), column_letters(b.column()))
    } else if a == b {
        format!("${}${}", column_letters(a.column()), a.row())
    } else {
        format!(
            "${}${}:${}${}",
            column_letters(a.column()),
            a.row(),
            column_letters(b.column()),
            b.row()
        )
    }
}

fn whole_rows(area: &GridArea) -> bool {
    area.first().column() == 1 && area.last().column() == COLUMNS
}

fn whole_columns(area: &GridArea) -> bool {
    area.first().row() == 1 && area.last().row() == ROWS
}

fn name_areas(host: &HostState, sheet: SheetId, name: &str) -> Vec<GridArea> {
    print_name(host, sheet, name)
        .and_then(|text| print_references(host, sheet, &text).ok())
        .unwrap_or_default()
}

fn join(areas: impl Iterator<Item = String>) -> String {
    areas.collect::<Vec<_>>().join(",")
}

/// Read one PageSetup member of a worksheet.
pub fn read(host: &HostState, sheet: SheetId, member: &str) -> Option<PrintValue> {
    use PrintValue::*;
    let s = host.model().definitions().page_setup(sheet);
    let fit = flag(&s, "pageSetUpPr", "fitToPage", false);
    Some(match member {
        "Orientation" => Int(if s.get("pageSetup", "orientation") == Some("landscape") {
            XL_LANDSCAPE
        } else {
            XL_PORTRAIT
        }),
        // Excel: Zoom is False when fit-to-pages is in force.
        "Zoom" => {
            if fit {
                Bool(false)
            } else {
                Int(s.get("pageSetup", "scale").and_then(|v| v.parse().ok()).unwrap_or(100))
            }
        }
        "FitToPagesWide" | "FitToPagesTall" => {
            let name = if member == "FitToPagesWide" { "fitToWidth" } else { "fitToHeight" };
            match s.get("pageSetup", name).and_then(|v| v.parse::<i64>().ok()) {
                Some(0) => Bool(false),
                Some(n) => Int(n),
                None => Int(1),
            }
        }
        "LeftMargin" => Real(margin(&s, "left")),
        "RightMargin" => Real(margin(&s, "right")),
        "TopMargin" => Real(margin(&s, "top")),
        "BottomMargin" => Real(margin(&s, "bottom")),
        "HeaderMargin" => Real(margin(&s, "header")),
        "FooterMargin" => Real(margin(&s, "footer")),
        "CenterHorizontally" => Bool(flag(&s, "printOptions", "horizontalCentered", false)),
        "CenterVertically" => Bool(flag(&s, "printOptions", "verticalCentered", false)),
        "PrintGridlines" => Bool(flag(&s, "printOptions", "gridLines", false)),
        "PrintHeadings" => Bool(flag(&s, "printOptions", "headings", false)),
        "BlackAndWhite" => Bool(flag(&s, "pageSetup", "blackAndWhite", false)),
        "Draft" => Bool(flag(&s, "pageSetup", "draft", false)),
        "Order" => Int(if s.get("pageSetup", "pageOrder") == Some("overThenDown") {
            XL_OVER_THEN_DOWN
        } else {
            XL_DOWN_THEN_OVER
        }),
        "PaperSize" => Int(s.get("pageSetup", "paperSize").and_then(|v| v.parse().ok()).unwrap_or(1)),
        "FirstPageNumber" => {
            if flag(&s, "pageSetup", "useFirstPageNumber", false) {
                Int(s.get("pageSetup", "firstPageNumber").and_then(|v| v.parse().ok()).unwrap_or(1))
            } else {
                Int(XL_AUTOMATIC)
            }
        }
        "PrintComments" => Int(match s.get("pageSetup", "cellComments") {
            Some("atEnd") => XL_PRINT_SHEET_END,
            Some("asDisplayed") => XL_PRINT_IN_PLACE,
            _ => XL_PRINT_NO_COMMENTS,
        }),
        "PrintErrors" => Int(match s.get("pageSetup", "errors") {
            Some("blank") => 1,
            Some("dash") => 2,
            Some("NA") => 3,
            _ => 0,
        }),
        "LeftHeader" | "CenterHeader" | "RightHeader" | "LeftFooter" | "CenterFooter"
        | "RightFooter" => {
            let (field, at) = header_place(member)?;
            let text = s.headers.get(field).map(|t| String::from_utf16_lossy(t.utf16_code_units()));
            Text(header_sections(&text.unwrap_or_default())[at].clone())
        }
        "AlignMarginsHeaderFooter" => Bool(flag(&s, "headerFooter", "alignWithMargins", true)),
        "DifferentFirstPageHeaderFooter" => Bool(flag(&s, "headerFooter", "differentFirst", false)),
        "OddAndEvenPagesHeaderFooter" => Bool(flag(&s, "headerFooter", "differentOddEven", false)),
        "ScaleWithDocHeaderFooter" => Bool(flag(&s, "headerFooter", "scaleWithDoc", true)),
        "PrintArea" => Text(join(name_areas(host, sheet, "_xlnm.Print_Area").iter().map(area_text))),
        "PrintTitleRows" => Text(join(
            name_areas(host, sheet, "_xlnm.Print_Titles").iter().filter(|a| whole_rows(a)).map(area_text),
        )),
        "PrintTitleColumns" => Text(join(
            name_areas(host, sheet, "_xlnm.Print_Titles")
                .iter()
                .filter(|a| whole_columns(a) && !whole_rows(a))
                .map(area_text),
        )),
        _ => return None,
    })
}

fn header_place(member: &str) -> Option<(&'static str, usize)> {
    Some(match member {
        "LeftHeader" => ("oddHeader", 0),
        "CenterHeader" => ("oddHeader", 1),
        "RightHeader" => ("oddHeader", 2),
        "LeftFooter" => ("oddFooter", 0),
        "CenterFooter" => ("oddFooter", 1),
        "RightFooter" => ("oddFooter", 2),
        _ => return None,
    })
}

fn attribute(section: &str, name: &str, value: Option<String>) -> PageSetupChange {
    PageSetupChange {
        attributes: vec![(section.into(), name.into(), value)],
        ..Default::default()
    }
}

fn want_bool(value: &PrintValue) -> Result<bool, PrintError> {
    match value {
        PrintValue::Bool(b) => Ok(*b),
        _ => Err(PrintError::Mismatch),
    }
}

fn want_int(value: &PrintValue) -> Result<i64, PrintError> {
    match value {
        PrintValue::Int(n) => Ok(*n),
        PrintValue::Real(r) if r.fract() == 0.0 && r.abs() < 1e9 => Ok(*r as i64),
        _ => Err(PrintError::Mismatch),
    }
}

fn want_real(value: &PrintValue) -> Result<f64, PrintError> {
    match value {
        PrintValue::Int(n) => Ok(*n as f64),
        PrintValue::Real(r) => Ok(*r),
        _ => Err(PrintError::Mismatch),
    }
}

fn want_text(value: &PrintValue) -> Result<&str, PrintError> {
    match value {
        PrintValue::Text(t) => Ok(t),
        _ => Err(PrintError::Mismatch),
    }
}

fn whole_titles(
    host: &HostState,
    sheet: SheetId,
    text: &str,
    rows: bool,
) -> Result<Vec<GridArea>, PrintError> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let areas = print_references(host, sheet, text).map_err(|_| PrintError::Refused)?;
    let good = |a: &GridArea| if rows { whole_rows(a) } else { whole_columns(a) && !whole_rows(a) };
    if areas.len() != 1 || !areas.iter().all(good) {
        return Err(PrintError::Refused);
    }
    Ok(areas)
}

/// The change that sets one PageSetup member, or why Excel refuses it.
pub fn write(
    host: &HostState,
    sheet: SheetId,
    member: &str,
    value: &PrintValue,
) -> Result<PageSetupChange, PrintError> {
    let flag_member = |section: &str, name: &str| {
        want_bool(value).map(|b| attribute(section, name, bool_text(b)))
    };
    match member {
        "Orientation" => Ok(attribute(
            "pageSetup",
            "orientation",
            Some(match want_int(value)? {
                XL_PORTRAIT => "portrait".into(),
                XL_LANDSCAPE => "landscape".into(),
                _ => return Err(PrintError::Refused),
            }),
        )),
        "Zoom" => match value {
            // Zoom = False hands the scale to the fit-to-pages settings.
            PrintValue::Bool(false) => Ok(attribute("pageSetUpPr", "fitToPage", bool_text(true))),
            PrintValue::Bool(true) => Err(PrintError::Refused),
            other => {
                let scale = want_int(other)?;
                if !(10..=400).contains(&scale) {
                    return Err(PrintError::Refused);
                }
                let mut change = attribute("pageSetUpPr", "fitToPage", bool_text(false));
                change.attributes.push(("pageSetup".into(), "scale".into(), Some(scale.to_string())));
                Ok(change)
            }
        },
        "FitToPagesWide" | "FitToPagesTall" => {
            let name = if member == "FitToPagesWide" { "fitToWidth" } else { "fitToHeight" };
            let pages = match value {
                PrintValue::Bool(false) => 0,
                PrintValue::Bool(true) => return Err(PrintError::Refused),
                other => want_int(other)?,
            };
            if !(0..=32_767).contains(&pages) {
                return Err(PrintError::Refused);
            }
            Ok(attribute("pageSetup", name, Some(pages.to_string())))
        }
        "LeftMargin" | "RightMargin" | "TopMargin" | "BottomMargin" | "HeaderMargin"
        | "FooterMargin" => {
            let points = want_real(value)?;
            if !points.is_finite() || points < 0.0 {
                return Err(PrintError::Refused);
            }
            let name = member.trim_end_matches("Margin").to_ascii_lowercase();
            Ok(attribute("pageMargins", &name, Some((points / 72.0).to_string())))
        }
        "CenterHorizontally" => flag_member("printOptions", "horizontalCentered"),
        "CenterVertically" => flag_member("printOptions", "verticalCentered"),
        "PrintGridlines" => flag_member("printOptions", "gridLines"),
        "PrintHeadings" => flag_member("printOptions", "headings"),
        "BlackAndWhite" => flag_member("pageSetup", "blackAndWhite"),
        "Draft" => flag_member("pageSetup", "draft"),
        "AlignMarginsHeaderFooter" => flag_member("headerFooter", "alignWithMargins"),
        "DifferentFirstPageHeaderFooter" => flag_member("headerFooter", "differentFirst"),
        "OddAndEvenPagesHeaderFooter" => flag_member("headerFooter", "differentOddEven"),
        "ScaleWithDocHeaderFooter" => flag_member("headerFooter", "scaleWithDoc"),
        "Order" => Ok(attribute(
            "pageSetup",
            "pageOrder",
            Some(match want_int(value)? {
                XL_DOWN_THEN_OVER => "downThenOver".into(),
                XL_OVER_THEN_DOWN => "overThenDown".into(),
                _ => return Err(PrintError::Refused),
            }),
        )),
        "PaperSize" => {
            let code = want_int(value)?;
            if !(1..=118).contains(&code) {
                return Err(PrintError::Refused);
            }
            Ok(attribute("pageSetup", "paperSize", Some(code.to_string())))
        }
        "FirstPageNumber" => {
            let number = want_int(value)?;
            let mut change = PageSetupChange::default();
            if number == XL_AUTOMATIC {
                change.attributes.push(("pageSetup".into(), "useFirstPageNumber".into(), None));
                change.attributes.push(("pageSetup".into(), "firstPageNumber".into(), None));
            } else if (-32_767..=32_767).contains(&number) && number >= 0 {
                change.attributes.push(("pageSetup".into(), "useFirstPageNumber".into(), bool_text(true)));
                change.attributes.push(("pageSetup".into(), "firstPageNumber".into(), Some(number.to_string())));
            } else {
                return Err(PrintError::Refused);
            }
            Ok(change)
        }
        "PrintComments" => Ok(attribute(
            "pageSetup",
            "cellComments",
            Some(match want_int(value)? {
                XL_PRINT_NO_COMMENTS => "none".into(),
                XL_PRINT_SHEET_END => "atEnd".into(),
                XL_PRINT_IN_PLACE => "asDisplayed".into(),
                _ => return Err(PrintError::Refused),
            }),
        )),
        "PrintErrors" => Ok(attribute(
            "pageSetup",
            "errors",
            Some(match want_int(value)? {
                0 => "displayed".into(),
                1 => "blank".into(),
                2 => "dash".into(),
                3 => "NA".into(),
                _ => return Err(PrintError::Refused),
            }),
        )),
        "LeftHeader" | "CenterHeader" | "RightHeader" | "LeftFooter" | "CenterFooter"
        | "RightFooter" => {
            let (field, at) = header_place(member).ok_or(PrintError::Refused)?;
            let text = want_text(value)?;
            let current = host.model().definitions().page_setup(sheet);
            let mut sections = header_sections(
                &current
                    .headers
                    .get(field)
                    .map(|t| String::from_utf16_lossy(t.utf16_code_units()))
                    .unwrap_or_default(),
            );
            sections[at] = text.to_string();
            Ok(PageSetupChange {
                headers: vec![(field.into(), header_join(&sections))],
                ..Default::default()
            })
        }
        "PrintArea" => {
            let text = match value {
                PrintValue::Bool(false) => "",
                other => want_text(other)?,
            };
            if !text.trim().is_empty() {
                print_references(host, sheet, text).map_err(|_| PrintError::Refused)?;
            }
            Ok(PageSetupChange { print_area: Some(text.into()), ..Default::default() })
        }
        "PrintTitleRows" | "PrintTitleColumns" => {
            let rows = member == "PrintTitleRows";
            let text = match value {
                PrintValue::Bool(false) => "",
                other => want_text(other)?,
            };
            let set = whole_titles(host, sheet, text, rows)?;
            // The two bands share one name: keep the other band as it is.
            let keep: Vec<GridArea> = name_areas(host, sheet, "_xlnm.Print_Titles")
                .into_iter()
                .filter(|a| if rows { !whole_rows(a) } else { whole_rows(a) })
                .collect();
            let (first, second) = if rows { (set, keep) } else { (keep, set) };
            let joined = join(first.iter().chain(second.iter()).map(area_text));
            Ok(PageSetupChange { print_titles: Some(joined), ..Default::default() })
        }
        _ => Err(PrintError::Refused),
    }
}

// --- page breaks ---------------------------------------------------------------------------

/// The manual breaks of one direction in position order (the order Excel's collection lists them).
pub fn manual_breaks(setup: &PageSetup, horizontal: bool) -> Vec<PageBreak> {
    let mut breaks: Vec<PageBreak> = if horizontal { &setup.row_breaks } else { &setup.column_breaks }
        .iter()
        .filter(|b| b.manual)
        .copied()
        .collect();
    breaks.sort_by_key(|b| b.id);
    breaks
}

/// The breaks of the direction after adding a manual break before `line` (a row for horizontal
/// breaks, a column for vertical ones). A break already there stays one break.
pub fn with_break(setup: &PageSetup, horizontal: bool, line: u32) -> Result<Vec<PageBreak>, PrintError> {
    let limit = if horizontal { ROWS } else { COLUMNS };
    if line < 2 || line > limit {
        return Err(PrintError::Refused);
    }
    let mut breaks = if horizontal { setup.row_breaks.clone() } else { setup.column_breaks.clone() };
    let id = line - 1;
    match breaks.iter_mut().find(|b| b.id == id) {
        Some(existing) => existing.manual = true,
        None => breaks.push(PageBreak {
            id,
            min: 0,
            max: if horizontal { COLUMNS - 1 } else { ROWS - 1 },
            manual: true,
            pivot: false,
        }),
    }
    breaks.sort_by_key(|b| b.id);
    Ok(breaks)
}

/// The breaks of the direction after removing the manual break before `line`.
pub fn without_break(setup: &PageSetup, horizontal: bool, line: u32) -> Vec<PageBreak> {
    let breaks = if horizontal { &setup.row_breaks } else { &setup.column_breaks };
    breaks.iter().filter(|b| !(b.manual && b.id + 1 == line)).copied().collect()
}

/// `Extent`: `xlPageBreakFull` (1) when the break crosses the whole sheet, else partial (2).
pub fn break_extent(b: &PageBreak, horizontal: bool) -> i64 {
    let end = if horizontal { COLUMNS - 1 } else { ROWS - 1 };
    if b.min == 0 && b.max >= end { 1 } else { 2 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_sections_split_and_join_as_excel_writes_them() {
        assert_eq!(header_sections("&LLeft&CMid&RRight"), ["Left", "Mid", "Right"].map(String::from));
        // Text before any code is the centre section.
        assert_eq!(header_sections("Page &P"), ["", "Page &P", ""].map(String::from));
        // A literal ampersand stays doubled inside its section.
        assert_eq!(header_sections("&LA&&B")[0], "A&&B");
        assert_eq!(header_join(&["a".into(), String::new(), "c".into()]), "&La&Rc");
        assert_eq!(header_join(&Default::default()), "");
    }

    #[test]
    fn area_text_matches_excels_print_area_strings() {
        use calc_value::CellKey;
        let s = calc_value::IdAllocator::<SheetId>::new().issue().unwrap();
        let cell = |r, c| CellKey::new(s, r, c).unwrap();
        let area = |a, b| GridArea::new(a, b).unwrap();
        assert_eq!(area_text(&area(cell(1, 1), cell(1, 1))), "$A$1");
        assert_eq!(area_text(&area(cell(1, 1), cell(3, 2))), "$A$1:$B$3");
        assert_eq!(area_text(&area(cell(1, 1), cell(2, 16_384))), "$1:$2");
        assert_eq!(area_text(&area(cell(1, 1), cell(1_048_576, 2))), "$A:$B");
    }

    #[test]
    fn breaks_sort_dedupe_and_refuse_the_first_line() {
        let mut setup = PageSetup::default();
        setup.row_breaks = with_break(&setup, true, 21).unwrap();
        setup.row_breaks = with_break(&setup, true, 11).unwrap();
        setup.row_breaks = with_break(&setup, true, 11).unwrap();
        assert_eq!(manual_breaks(&setup, true).iter().map(|b| b.id).collect::<Vec<_>>(), [10, 20]);
        assert_eq!(break_extent(&manual_breaks(&setup, true)[0], true), 1);
        assert_eq!(with_break(&setup, true, 1), Err(PrintError::Refused));
        assert_eq!(with_break(&setup, false, 16_385), Err(PrintError::Refused));
        let after = without_break(&setup, true, 11);
        assert_eq!(after.iter().map(|b| b.id).collect::<Vec<_>>(), [20]);
    }
}
