//! Print settings and the page plan published by the workbook host.

use crate::grid::Area;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_PRINT_PAGES: usize = 200;
pub const MAX_PRINT_CELLS: usize = 100_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageBreak {
    pub id: u32,
    pub min: u32,
    pub max: u32,
    pub manual: bool,
    pub pivot: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageSettings {
    pub sections: BTreeMap<String, BTreeMap<String, String>>,
    pub headers: BTreeMap<String, String>,
    pub row_breaks: Vec<PageBreak>,
    pub column_breaks: Vec<PageBreak>,
}
impl PageSettings {
    pub fn get(&self, section: &str, name: &str) -> Option<&str> {
        self.sections.get(section)?.get(name).map(String::as_str)
    }
    pub fn number(&self, section: &str, name: &str, default: f64) -> f64 {
        self.get(section, name)
            .and_then(|v| v.parse().ok())
            .filter(|v: &f64| v.is_finite())
            .unwrap_or(default)
    }
    pub fn print_gridlines(&self) -> bool {
        self.flag("printOptions", "gridLines", false)
            && self.flag("printOptions", "gridLinesSet", true)
    }
    pub fn flag(&self, section: &str, name: &str, default: bool) -> bool {
        self.get(section, name)
            .map_or(default, |v| v == "1" || v == "true")
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageSetupChange {
    pub attributes: Vec<(String, String, Option<String>)>,
    pub headers: Vec<(String, String)>,
    pub row_breaks: Option<Vec<PageBreak>>,
    pub column_breaks: Option<Vec<PageBreak>>,
    pub print_area: Option<String>,
    pub print_titles: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageSetupState {
    pub revision: u64,
    pub settings: PageSettings,
    pub print_area: String,
    pub print_titles: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrintRegion {
    pub area: Area,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrintPage {
    pub number: u32,
    pub body: Area,
    pub regions: Vec<PrintRegion>,
    pub width: f64,
    pub height: f64,
    pub offset_x: f64,
    pub offset_y: f64,
    pub manual_bottom: bool,
    pub manual_right: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrintPlan {
    pub paper_width: f64,
    pub paper_height: f64,
    pub margins: [f64; 6],
    pub scale: f64,
    pub pages: Vec<PrintPage>,
}
/// A plan's page breaks as Excel lists them (`Worksheet.HPageBreaks` and `VPageBreaks`): the first row, or the
/// first column, of every page after the first in that direction, with whether the break is a stored manual one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlanBreaks {
    /// `(first row of the next page, manual)`, in row order.
    pub rows: Vec<(u32, bool)>,
    /// `(first column of the next page, manual)`, in column order.
    pub columns: Vec<(u32, bool)>,
}
impl PrintPlan {
    /// The breaks between this plan's pages. A print area of several ranges gives the breaks of each range, merged.
    /// The positions of the automatic ones follow the UI27 and UI30 device sizes, NOT an Excel break list (assumed
    /// until the PB1 macro of ~/codex-runs/eng13 is answered); the manual ones are the stored breaks.
    pub fn breaks(&self) -> PlanBreaks {
        let mut rows: BTreeMap<u32, bool> = BTreeMap::new();
        let mut columns: BTreeMap<u32, bool> = BTreeMap::new();
        let first_row = self.pages.iter().map(|p| p.body.0.2).min().unwrap_or(0);
        let first_column = self.pages.iter().map(|p| p.body.0.3).min().unwrap_or(0);
        for page in &self.pages {
            let (top, left) = (page.body.0.2, page.body.0.3);
            if top > first_row {
                // The page above this one ends in a manual break when it says so: the same cut, either way.
                let manual = self.pages.iter().any(|above| {
                    above.body.1.2 + 1 == top && above.body.0.3 == left && above.manual_bottom
                });
                let entry = rows.entry(top).or_insert(false);
                *entry |= manual;
            }
            if left > first_column {
                let manual = self.pages.iter().any(|before| {
                    before.body.1.3 + 1 == left && before.body.0.2 == top && before.manual_right
                });
                let entry = columns.entry(left).or_insert(false);
                *entry |= manual;
            }
        }
        PlanBreaks {
            rows: rows.into_iter().collect(),
            columns: columns.into_iter().collect(),
        }
    }

    /// The lines the PB1 macro records for a case, from this plan: `tag  pages  n` and `tag  H|V  i  address  type`
    /// (type -4105 automatic, -4135 manual), the address being the first cell of the next page.
    pub fn breaks_text(&self, tag: &str) -> String {
        let breaks = self.breaks();
        let first_row = self.pages.iter().map(|p| p.body.0.2).min().unwrap_or(1);
        let first_column = self.pages.iter().map(|p| p.body.0.3).min().unwrap_or(1);
        let mut out = format!("{tag}\tpages\t{}\n", self.pages.len());
        for (i, (row, manual)) in breaks.rows.iter().enumerate() {
            out += &format!(
                "{tag}\tH\t{}\t{}\t{}\n",
                i + 1,
                crate::address::address(*row, first_column),
                if *manual { -4135 } else { -4105 }
            );
        }
        for (i, (column, manual)) in breaks.columns.iter().enumerate() {
            out += &format!(
                "{tag}\tV\t{}\t{}\t{}\n",
                i + 1,
                crate::address::address(first_row, *column),
                if *manual { -4135 } else { -4105 }
            );
        }
        out
    }
}

fn fail(detail: &str) -> String {
    format!("{detail} (dc-971p.57)")
}
#[derive(Clone, Debug, PartialEq)]
pub struct HeaderRun {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: u8,
    pub strike: bool,
    pub script: i8,
    pub font: String,
    pub points: f64,
    pub color: String,
}
impl Default for HeaderRun {
    fn default() -> Self {
        Self {
            text: String::new(),
            bold: false,
            italic: false,
            underline: 0,
            strike: false,
            script: 0,
            font: "Arial".into(),
            points: 11.,
            color: "#000000".into(),
        }
    }
}
pub struct HeaderContext<'a> {
    pub page: u32,
    pub pages: usize,
    pub sheet: &'a str,
    pub book: &'a str,
    pub date: &'a str,
    pub time: &'a str,
}
/// Decode header/footer fields without injecting HTML; picture/path fields remain explicit refusals.
pub fn header_runs(text: &str, context: &HeaderContext<'_>) -> Result<[Vec<HeaderRun>; 3], String> {
    let chars = text.chars().collect::<Vec<_>>();
    let mut out: [Vec<HeaderRun>; 3] = Default::default();
    let mut section = 1;
    let mut style = HeaderRun::default();
    let mut i = 0;
    let append =
        |out: &mut [Vec<HeaderRun>; 3], section: usize, style: &HeaderRun, text: String| {
            let mut run = style.clone();
            run.text = text;
            out[section].push(run);
        };
    while i < chars.len() {
        if chars[i] != '&' {
            let start = i;
            while i < chars.len() && chars[i] != '&' {
                i += 1;
            }
            append(&mut out, section, &style, chars[start..i].iter().collect());
            continue;
        }
        i += 1;
        if i == chars.len() {
            append(&mut out, section, &style, "&".into());
            break;
        }
        let code = chars[i];
        i += 1;
        match code {
            'L' => section = 0,
            'C' => section = 1,
            'R' => section = 2,
            '&' => append(&mut out, section, &style, "&".into()),
            'P' => {
                if chars.get(i).is_some_and(|c| *c == '+' || *c == '-') {
                    return Err(fail(
                        "Header page-number arithmetic needs Excel measurements",
                    ));
                }
                append(&mut out, section, &style, context.page.to_string());
            }
            'N' => append(&mut out, section, &style, context.pages.to_string()),
            'A' => append(&mut out, section, &style, context.sheet.into()),
            'F' => append(&mut out, section, &style, context.book.into()),
            'D' => append(&mut out, section, &style, context.date.into()),
            'T' => append(&mut out, section, &style, context.time.into()),
            'B' => style.bold = !style.bold,
            'I' => style.italic = !style.italic,
            'U' => style.underline = if style.underline == 1 { 0 } else { 1 },
            'E' => style.underline = if style.underline == 2 { 0 } else { 2 },
            'S' => style.strike = !style.strike,
            'X' => style.script = if style.script == 1 { 0 } else { 1 },
            'Y' => style.script = if style.script == -1 { 0 } else { -1 },
            '"' => {
                let start = i;
                while i < chars.len() && chars[i] != '"' {
                    i += 1;
                }
                if i == chars.len() {
                    return Err(fail("Header font field is incomplete"));
                }
                let font = chars[start..i].iter().collect::<String>();
                i += 1;
                let (name, kind) = font.split_once(',').unwrap_or((&font, "Regular"));
                if name != "-" {
                    style.font = name.into();
                }
                style.bold = kind.contains("Bold");
                style.italic = kind.contains("Italic");
            }
            'K' => {
                if i + 6 > chars.len() || !chars[i..i + 6].iter().all(|c| c.is_ascii_hexdigit()) {
                    return Err(fail("Header theme colour fields need Excel measurements"));
                }
                style.color = format!("#{}", chars[i..i + 6].iter().collect::<String>());
                i += 6;
            }
            '0'..='9' => {
                let start = i - 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                style.points = chars[start..i]
                    .iter()
                    .collect::<String>()
                    .parse::<f64>()
                    .ok()
                    .filter(|v| (1.0..=409.0).contains(v))
                    .ok_or_else(|| fail("Invalid header font size"))?;
            }
            // UI27 q7-headers: Excel prints "FOOT &[Page] / &[Pages]" as "FOOT Page] / Pages]":
            // the ampersand and the bracket vanish and the rest stays.
            '[' => {}
            'G' => {
                return Err(fail(
                    "Header/footer pictures are preserved; printing them is not yet modelled",
                ));
            }
            'Z' => {
                return Err(fail(
                    "The browser has no original workbook path for the header/footer path field",
                ));
            }
            _ => {
                return Err(fail(&format!(
                    "Header/footer field &{code} is not modelled"
                )));
            }
        }
    }
    Ok(out)
}
pub fn page_header<'a>(
    settings: &'a PageSettings,
    page_index: usize,
    _page_number: u32,
    footer: bool,
) -> &'a str {
    let kind = if page_index == 0 && settings.flag("headerFooter", "differentFirst", false) {
        "first"
    } else if (page_index + 1) % 2 == 0 && settings.flag("headerFooter", "differentOddEven", false)
    {
        // UI27 q7-headers: odd and even follow the position in the printout, not the printed
        // number (first page number 4: the second page, numbered 5, takes the even header).
        "even"
    } else {
        "odd"
    };
    settings
        .headers
        .get(&format!(
            "{kind}{}",
            if footer { "Footer" } else { "Header" }
        ))
        .map(String::as_str)
        .unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn headers_decode_fields_styles_escaped_ampersands_and_refuse_missing_pictures() {
        let c = HeaderContext {
            page: 7,
            pages: 12,
            sheet: "A&B",
            book: "Book.xlsx",
            date: "2026-11-06",
            time: "12:00",
        };
        let runs = header_runs(
            "&L&& &A&C&BPage &P of &N&B&R&\"Arial,Italic\"&12&K112233&F",
            &c,
        )
        .unwrap();
        assert_eq!(
            runs[0].iter().map(|r| r.text.as_str()).collect::<String>(),
            "& A&B"
        );
        assert_eq!(
            runs[1].iter().map(|r| r.text.as_str()).collect::<String>(),
            "Page 7 of 12"
        );
        assert!(runs[1][0].bold);
        assert!(runs[2][0].italic);
        assert_eq!(runs[2][0].points, 12.);
        assert_eq!(runs[2][0].color, "#112233");
        assert!(header_runs("&G", &c).is_err());
        assert!(header_runs("&Z", &c).is_err());
    }
}
