//! One bounded pagination plan for worksheet page views, preview and browser print.
//! Consumes existing sheet metrics and projected values; it never evaluates workbook formulas.
use crate::geometry::{GridMetrics, Sizes, sheet_metrics};
use crate::projection::{Area, Cell, Chart, Geometry, Sheet};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrintDocument {
    pub revision: u64,
    pub sheet: Sheet,
    pub sheet_name: String,
    pub book_name: String,
    pub settings: PageSettings,
    pub geometry: Geometry,
    pub plan: PrintPlan,
    pub cells: Vec<Cell>,
    pub charts: Vec<Chart>,
    pub notices: Vec<String>,
}

fn fail(detail: &str) -> String {
    format!("{detail} (dc-971p.57)")
}
fn length(m: &GridMetrics, rows: bool, first: u32, last: u32) -> f64 {
    if rows {
        m.row_start(last + 1) - m.row_start(first)
    } else {
        m.col_start(last + 1) - m.col_start(first)
    }
}
#[derive(Clone, Copy)]
struct Slice {
    first: u32,
    last: u32,
    title: Option<(u32, u32)>,
    length: f64,
    manual: bool,
}
// UI27 (Excel 16.0.20527, Microsoft Print to PDF at 600 dpi): printed bands are whole device
// pixels, and a band is a little larger than its nominal size. A 15 pt row (20 px) prints as 127
// device pixels and a 10.7109375-character column (75 px) as 504; widths and heights are rounded
// after the print scale is applied (q2-*, q3-fit-*). The column factor is calibrated at one
// width; other widths are open questions (excel-questions-ui30).
const COLUMN_DEVICE_PER_PX: f64 = 504. / 75.;
const ROW_DEVICE_PER_POINT: f64 = 127. / 15.;
/// Page and margin sizes: 600 dpi device pixels per CSS pixel.
const PAGE_DEVICE_PER_PX: f64 = 600. / 96.;
/// Bands in device pixels at one print scale, indexed like the pixel metrics.
fn device_metrics(m: &GridMetrics, geometry: &Geometry, scale: f64) -> GridMetrics {
    let round = |v: f64| (v * scale).round();
    let row_default = geometry.default_row_height.unwrap_or(15.) * ROW_DEVICE_PER_POINT;
    let col_default = m.col_width * COLUMN_DEVICE_PER_PX;
    let rows = geometry.rows.iter().map(|&(row, height, hidden)| {
        (
            row,
            if hidden {
                0.
            } else {
                round(height.map_or(row_default, |h| h * ROW_DEVICE_PER_POINT))
            },
        )
    });
    let mut columns = Vec::new();
    for &(first, last, _, hidden) in &geometry.columns {
        for column in first..=last.min(16_384) {
            let size = if hidden {
                0.
            } else {
                round((m.col_start(column + 1) - m.col_start(column)) * COLUMN_DEVICE_PER_PX)
            };
            columns.push((column, size));
        }
    }
    GridMetrics {
        scale: 1.,
        row_height: round(row_default),
        col_width: round(col_default),
        header_h: round(m.header_h * ROW_DEVICE_PER_POINT * 0.75),
        header_w: round(m.header_w * COLUMN_DEVICE_PER_PX),
        rows: Sizes::new(round(row_default), rows),
        cols: Sizes::new(round(col_default), columns),
    }
}
#[allow(clippy::too_many_arguments)]
fn slices(
    m: &GridMetrics,
    d: &GridMetrics,
    rows: bool,
    first: u32,
    last: u32,
    title: Option<(u32, u32)>,
    capacity: f64,
    breaks: &BTreeSet<u32>,
    titles_everywhere: bool,
) -> Result<Vec<Slice>, String> {
    let mut start = first;
    let mut out = Vec::new();
    while start <= last {
        if out.len() >= MAX_PRINT_PAGES {
            return Err(fail(
                "Preview exceeds 200 pages; reduce the print area or scaling",
            ));
        }
        let visible = if rows {
            m.visible_row(start, true, last)
        } else {
            m.visible_col(start, true, last)
        };
        if visible < start || visible > last || length(d, rows, visible, last) <= 0. {
            break;
        }
        start = visible;
        // UI27 q6-titles-break: a page that begins inside the title band repeats the whole band
        // and carries on after it (title 1:22, break after row 20: page two is rows 1..22, 23...).
        if let Some((a, b)) = title.filter(|_| !out.is_empty()) {
            if (a..=b).contains(&start) {
                start = b + 1;
                if start > last {
                    break;
                }
                continue;
            }
        }
        let repeat = title.filter(|(_, b)| titles_everywhere || start > *b);
        let repeated = repeat.map_or(0., |(a, b)| length(d, rows, a, b));
        let available = capacity - repeated;
        if !available.is_finite() || available <= 0. {
            return Err(fail("Print titles leave no room for worksheet content"));
        }
        // UI27 q3-fit-two, q4-fit-manual: content that exactly fills a page does not fit.
        if length(d, rows, start, start) >= available {
            return Err(fail(
                "A row or column is larger than the printable page; reduce the scale",
            ));
        }
        let limit = breaks.range(start..last).next().copied().unwrap_or(last);
        let (mut lo, mut hi) = (start, limit);
        while lo < hi {
            let mid = lo + (hi - lo).div_ceil(2);
            if length(d, rows, start, mid) < available {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        out.push(Slice {
            first: start,
            last: lo,
            title: repeat,
            length: repeat.map_or(0., |(a, b)| length(m, rows, a, b)) + length(m, rows, start, lo),
            manual: breaks.contains(&lo),
        });
        start = lo + 1;
    }
    Ok(out)
}
fn paper(settings: &PageSettings) -> Result<(f64, f64), String> {
    fn measure(value: &str) -> Option<f64> {
        for (unit, pixels) in [
            ("mm", 96. / 25.4),
            ("cm", 96. / 2.54),
            ("in", 96.),
            ("pt", 96. / 72.),
            ("pc", 16.),
            ("pi", 16.),
        ] {
            if let Some(n) = value.strip_suffix(unit) {
                return n
                    .parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite() && *n > 0.)
                    .map(|n| n * pixels);
            }
        }
        None
    }
    let size = if let (Some(w), Some(h)) = (
        settings.get("pageSetup", "paperWidth"),
        settings.get("pageSetup", "paperHeight"),
    ) {
        (
            measure(w).ok_or_else(|| fail("Invalid custom paper width"))?,
            measure(h).ok_or_else(|| fail("Invalid custom paper height"))?,
        )
    } else {
        match settings.get("pageSetup", "paperSize").unwrap_or("1") {
            "1" => (8.5 * 96., 11. * 96.),
            "3" => (11. * 96., 17. * 96.),
            "5" => (8.5 * 96., 14. * 96.),
            "7" => (7.25 * 96., 10.5 * 96.),
            "8" => (297. * 96. / 25.4, 420. * 96. / 25.4),
            "9" => (210. * 96. / 25.4, 297. * 96. / 25.4),
            "11" => (148. * 96. / 25.4, 210. * 96. / 25.4),
            _ => {
                return Err(fail(
                    "This paper size has no browser preview dimensions; choose Letter, Legal, Tabloid, Executive, A3, A4 or A5",
                ));
            }
        }
    };
    if [size.0, size.1]
        .iter()
        .any(|v| !v.is_finite() || *v > 14_400.)
    {
        return Err(fail(
            "Custom paper exceeds the browser's 150-inch paper limit",
        ));
    }
    Ok(
        if settings.get("pageSetup", "orientation") == Some("landscape") {
            (size.1.max(size.0), size.0.min(size.1))
        } else {
            size
        },
    )
}
pub fn paginate(
    settings: &PageSettings,
    geometry: &Geometry,
    areas: &[Area],
    title_rows: Option<(u32, u32)>,
    title_columns: Option<(u32, u32)>,
) -> Result<PrintPlan, String> {
    let m = sheet_metrics(Some(geometry));
    let (paper_width, paper_height) = paper(settings)?;
    let margins = [
        ("left", 0.7),
        ("right", 0.7),
        ("top", 0.75),
        ("bottom", 0.75),
        ("header", 0.3),
        ("footer", 0.3),
    ]
    .map(|(name, default)| settings.number("pageMargins", name, default) * 96.);
    if margins.iter().any(|v| *v < 0.) {
        return Err(fail("Margins cannot be negative"));
    }
    let width = paper_width - margins[0] - margins[1];
    let height = paper_height - margins[2] - margins[3];
    if width <= 0. || height <= 0. {
        return Err(fail("Margins leave no printable page"));
    }
    let headings = settings.flag("printOptions", "headings", false);
    let page_w = width * PAGE_DEVICE_PER_PX;
    let page_h = height * PAGE_DEVICE_PER_PX;
    let fit = settings.flag("pageSetUpPr", "fitToPage", false);
    // UI27 q4-manual: Excel 16.0.20527 stores row max=15 and column max=119
    // for A1:P120. A break spanning the complete printed area is not a partial cut.
    let breaks = |rows: bool, area: Area| -> Result<BTreeSet<u32>, String> {
        if fit {
            return Ok(BTreeSet::new());
        }
        let area = area.area().ok_or_else(|| fail("Invalid print area"))?;
        let (first, last, mut cross_first, mut cross_last, title) = if rows {
            (
                area.first().row(),
                area.last().row(),
                area.first().column() - 1,
                area.last().column() - 1,
                title_columns,
            )
        } else {
            (
                area.first().column(),
                area.last().column(),
                area.first().row() - 1,
                area.last().row() - 1,
                title_rows,
            )
        };
        if let Some((first, last)) = title {
            let first = first
                .checked_sub(1)
                .ok_or_else(|| fail("Invalid print-title band"))?;
            let last = last
                .checked_sub(1)
                .ok_or_else(|| fail("Invalid print-title band"))?;
            cross_first = cross_first.min(first);
            cross_last = cross_last.max(last);
        }
        let mut out = BTreeSet::new();
        for b in if rows {
            &settings.row_breaks
        } else {
            &settings.column_breaks
        } {
            if !b.manual {
                continue;
            }
            if b.pivot || b.min > b.max {
                return Err(fail(
                    "Pivot or invalid page-break spans need Excel pagination measurements",
                ));
            }
            if b.id < first || b.id > last || b.max < cross_first || b.min > cross_last {
                continue;
            }
            if b.min > cross_first || b.max < cross_last {
                return Err(fail(
                    "A page break crossing only part of a print area needs Excel pagination measurements",
                ));
            }
            out.insert(b.id);
        }
        Ok(out)
    };
    let dimensions = |area: Area, scale: f64| -> Result<(Vec<Slice>, Vec<Slice>), String> {
        let a = area.area().ok_or_else(|| fail("Invalid print area"))?;
        if title_rows.is_some_and(|(r, _)| r > a.first().row())
            || title_columns.is_some_and(|(c, _)| c > a.first().column())
        {
            return Err(fail(
                "Title bands after the start of the print area need Excel pagination measurements",
            ));
        }
        let rb = breaks(true, area)?;
        let cb = breaks(false, area)?;
        let d = device_metrics(&m, geometry, scale);
        let (hw, hh) = if headings { (d.header_w, d.header_h) } else { (0., 0.) };
        Ok((
            slices(
                &m,
                &d,
                true,
                a.first().row(),
                a.last().row(),
                title_rows,
                page_h - hh,
                &rb,
                false,
            )?,
            slices(
                &m,
                &d,
                false,
                a.first().column(),
                a.last().column(),
                title_columns,
                page_w - hw,
                &cb,
                false,
            )?,
        ))
    };
    let scale = if fit {
        let rb = BTreeSet::new();
        let cb = BTreeSet::new();
        let wide = settings.number("pageSetup", "fitToWidth", 1.);
        let tall = settings.number("pageSetup", "fitToHeight", 1.);
        if wide > MAX_PRINT_PAGES as f64 || tall > MAX_PRINT_PAGES as f64 {
            return Err(fail(
                "Fit targets above 200 pages exceed the browser preview limit",
            ));
        }
        (10..=100)
            .rev()
            .find_map(|p| {
                let s = f64::from(p) / 100.;
                let d = device_metrics(&m, geometry, s);
                let (hw, hh) = if headings { (d.header_w, d.header_h) } else { (0., 0.) };
                areas
                    .iter()
                    .all(|a| {
                        let Some(a) = a.area() else {
                            return false;
                        };
                        // Automatic axes must not constrain fit, including through preview limits.
                        // UI27 q3-fit-large-titles: fit counts the print titles on every page,
                        // the first included.
                        (wide == 0.
                            || slices(
                                &m,
                                &d,
                                false,
                                a.first().column(),
                                a.last().column(),
                                title_columns,
                                page_w - hw,
                                &cb,
                                true,
                            )
                            .is_ok_and(|c| c.len() as f64 <= wide))
                            && (tall == 0.
                                || slices(
                                    &m,
                                    &d,
                                    true,
                                    a.first().row(),
                                    a.last().row(),
                                    title_rows,
                                    page_h - hh,
                                    &rb,
                                    true,
                                )
                                .is_ok_and(|r| r.len() as f64 <= tall))
                    })
                    .then_some(s)
            })
            .ok_or_else(|| fail("Content cannot fit at Excel's minimum 10% scale"))?
    } else {
        settings.number("pageSetup", "scale", 100.) / 100.
    };
    if !(0.1..=4.).contains(&scale) {
        return Err(fail("Scale must be between 10% and 400%"));
    }
    let hw = if headings { m.header_w } else { 0. };
    let hh = if headings { m.header_h } else { 0. };
    let mut pages = Vec::new();
    let mut drawing_pixels = 0.;
    let first = if settings.flag("pageSetup", "useFirstPageNumber", false) {
        settings.number("pageSetup", "firstPageNumber", 1.) as u32
    } else {
        1
    };
    for area in areas {
        let (rows, columns) = dimensions(*area, scale)?;
        let sheet = area.0;
        let across = settings.get("pageSetup", "pageOrder") == Some("overThenDown");
        let mut positions = Vec::new();
        for (ri, r) in rows.iter().enumerate() {
            for (ci, c) in columns.iter().enumerate() {
                positions.push((ri, ci, *r, *c));
            }
        }
        if !across {
            positions.sort_by_key(|(r, c, _, _)| (*c, *r));
        }
        for (_, _, r, c) in positions {
            if pages.len() >= MAX_PRINT_PAGES {
                return Err(fail(
                    "Preview exceeds 200 pages; reduce the print area or scaling",
                ));
            }
            let make = |top, left, bottom, right| {
                Area(
                    crate::projection::Key(sheet.0, sheet.1, top, left),
                    crate::projection::Key(sheet.0, sheet.1, bottom, right),
                )
            };
            let mut xs = Vec::new();
            if let Some((a, b)) = c.title {
                xs.push((a, b));
            }
            xs.push((c.first, c.last));
            let mut ys = Vec::new();
            if let Some((a, b)) = r.title {
                ys.push((a, b));
            }
            ys.push((r.first, r.last));
            let mut regions = Vec::new();
            let mut y = hh;
            for (top, bottom) in ys {
                let h = length(&m, true, top, bottom);
                let mut x = hw;
                for (left, right) in &xs {
                    let w = length(&m, false, *left, *right);
                    if w > 16_384. || h > 16_384. || w * h > 16_000_000. {
                        return Err(fail(
                            "A print drawing exceeds browser canvas limits; reduce the print area or increase the scale",
                        ));
                    }
                    drawing_pixels += w * h;
                    if drawing_pixels > 32_000_000. {
                        return Err(fail(
                            "Print drawings exceed the browser preview budget; print a smaller area",
                        ));
                    }
                    regions.push(PrintRegion {
                        area: make(top, *left, bottom, *right),
                        x,
                        y,
                        width: w,
                        height: h,
                    });
                    x += w;
                }
                y += h;
            }
            let content_w = (c.length + hw) * scale;
            let content_h = (r.length + hh) * scale;
            pages.push(PrintPage {
                number: first.saturating_add(pages.len() as u32),
                body: make(r.first, c.first, r.last, c.last),
                regions,
                width: c.length + hw,
                height: r.length + hh,
                offset_x: margins[0]
                    + if settings.flag("printOptions", "horizontalCentered", false) {
                        (width - content_w) / 2.
                    } else {
                        0.
                    },
                offset_y: margins[2]
                    + if settings.flag("printOptions", "verticalCentered", false) {
                        (height - content_h) / 2.
                    } else {
                        0.
                    },
                manual_bottom: r.manual,
                manual_right: c.manual,
            });
        }
    }
    Ok(PrintPlan {
        paper_width,
        paper_height,
        margins,
        scale,
        pages,
    })
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
    use crate::projection::Key;
    fn area(t: u32, l: u32, b: u32, r: u32) -> Area {
        Area(Key(0, 0, t, l), Key(0, 0, b, r))
    }
    fn set(settings: &mut PageSettings, s: &str, n: &str, v: &str) {
        settings
            .sections
            .entry(s.into())
            .or_default()
            .insert(n.into(), v.into());
    }
    fn setup() -> PageSettings {
        let mut s = PageSettings::default();
        for name in ["left", "right", "top", "bottom"] {
            set(&mut s, "pageMargins", name, "0.5");
        }
        s
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn manual_breaks_titles_hidden_rows_and_page_order_use_one_plan() {
        let mut s = setup();
        s.row_breaks.push(PageBreak {
            id: 20,
            min: 0,
            max: 16383,
            manual: true,
            pivot: false,
        });
        s.column_breaks.push(PageBreak {
            id: 4,
            min: 0,
            max: 1048575,
            manual: true,
            pivot: false,
        });
        let mut g = Geometry::default();
        g.rows.push((30, Some(15.), true));
        let p = paginate(&s, &g, &[area(1, 1, 60, 20)], Some((1, 2)), Some((1, 1))).unwrap();
        assert_eq!(p.pages.len(), 6);
        assert_eq!(p.pages[0].body, area(1, 1, 20, 4));
        assert_eq!(p.pages[1].body, area(21, 1, 60, 4));
        assert_eq!(p.pages[1].regions[0].area, area(1, 1, 2, 4));
        assert!(p.pages[0].manual_bottom);
        assert_eq!(p.pages[2].regions.len(), 2);
        set(&mut s, "pageSetup", "pageOrder", "overThenDown");
        let q = paginate(&s, &g, &[area(1, 1, 60, 20)], Some((1, 2)), Some((1, 1))).unwrap();
        assert_eq!(q.pages[1].body.0.2, 1);
        assert_eq!(q.pages[1].body.0.3, 5);
        assert_eq!(q.pages[3].body.0.2, 21);
    }
    // Measurements: UI27 q4-manual saved.xlsx SHA256
    // 16ca94926eb5f82ac4cdb73435eafec942cabcc55a827e4751b983746dcdff38.
    // Automatic printer-dependent boundaries are not asserted by these manual-cut checks.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn measured_print_area_spans_apply_per_area_without_becoming_global_breaks() {
        let mut s = setup();
        s.row_breaks.push(PageBreak {
            id: 20,
            min: 0,
            max: 15,
            manual: true,
            pivot: false,
        });
        s.column_breaks.push(PageBreak {
            id: 4,
            min: 0,
            max: 119,
            manual: true,
            pivot: false,
        });
        let p = paginate(&s, &Geometry::default(), &[area(1, 1, 120, 16)], None, None).unwrap();
        assert_eq!(p.pages[0].body, area(1, 1, 20, 4));
        assert!(p.pages[0].manual_bottom && p.pages[0].manual_right);
        s.row_breaks[0].max = 7;
        s.column_breaks[0].max = 1048575;
        let p = paginate(
            &s,
            &Geometry::default(),
            &[area(5, 1, 80, 8), area(2, 11, 15, 14)],
            None,
            None,
        )
        .unwrap();
        assert_eq!(p.pages[0].body, area(5, 1, 20, 4));
        assert_eq!(p.pages.last().unwrap().body, area(2, 11, 15, 14));
        assert!(!p.pages.last().unwrap().manual_bottom && !p.pages.last().unwrap().manual_right);
        assert!(
            paginate(&s, &Geometry::default(), &[area(1, 1, 120, 16)], None, None)
                .unwrap_err()
                .contains("part of a print area")
        );
        set(&mut s, "pageSetUpPr", "fitToPage", "1");
        set(&mut s, "pageSetup", "fitToWidth", "1");
        set(&mut s, "pageSetup", "fitToHeight", "1");
        let p = paginate(&s, &Geometry::default(), &[area(1, 1, 120, 16)], None, None).unwrap();
        assert_eq!(p.pages.len(), 1);
        assert!(!p.pages[0].manual_bottom && !p.pages[0].manual_right);
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fit_shrinks_without_enlargement_ignores_manual_breaks_and_honours_orientation() {
        let mut s = setup();
        set(&mut s, "pageSetUpPr", "fitToPage", "1");
        set(&mut s, "pageSetup", "fitToWidth", "1");
        set(&mut s, "pageSetup", "fitToHeight", "0");
        s.row_breaks.push(PageBreak {
            id: 10,
            min: 0,
            max: 16383,
            manual: true,
            pivot: false,
        });
        let g = Geometry::default();
        let p = paginate(&s, &g, &[area(1, 1, 100, 20)], None, None).unwrap();
        assert!(p.scale < 1.);
        assert_eq!(
            p.pages
                .iter()
                .map(|p| p.body.0.3)
                .collect::<BTreeSet<_>>()
                .len(),
            1
        );
        assert!(p.pages[0].body.1.2 > 10);
        assert!(p.pages.iter().all(|p| !p.manual_bottom));
        let small = paginate(&s, &g, &[area(1, 1, 1, 1)], None, None).unwrap();
        assert_eq!(small.scale, 1.);
        assert!(
            paginate(&s, &g, &[area(1, 1, 10000, 1)], None, None)
                .unwrap_err()
                .contains("200 pages")
        );
        set(&mut s, "pageSetup", "paperSize", "9");
        set(&mut s, "pageSetup", "orientation", "landscape");
        let landscape = paginate(&s, &g, &[area(1, 1, 100, 20)], None, None).unwrap();
        assert!(landscape.paper_width > landscape.paper_height);
        assert!(landscape.scale > p.scale);
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn disjoint_areas_numbering_margins_and_limits_are_explicit() {
        let mut s = setup();
        set(&mut s, "pageSetup", "useFirstPageNumber", "1");
        set(&mut s, "pageSetup", "firstPageNumber", "7");
        let p = paginate(
            &s,
            &Geometry::default(),
            &[area(5, 3, 15, 5), area(2, 11, 5, 12)],
            Some((1, 2)),
            Some((1, 2)),
        )
        .unwrap();
        assert_eq!(p.pages.len(), 2);
        assert_eq!(p.pages[0].number, 7);
        assert_eq!(p.pages[1].number, 8);
        assert_eq!(p.pages[0].regions.len(), 4);
        set(&mut s, "pageMargins", "left", "9");
        assert!(paginate(&s, &Geometry::default(), &[area(1, 1, 1, 1)], None, None).is_err());
        let mut custom = setup();
        set(&mut custom, "pageSetup", "paperWidth", "200in");
        set(&mut custom, "pageSetup", "paperHeight", "11in");
        assert!(
            paginate(
                &custom,
                &Geometry::default(),
                &[area(1, 1, 1, 1)],
                None,
                None
            )
            .unwrap_err()
            .contains("paper limit")
        );
        assert!(
            paginate(
                &setup(),
                &Geometry::default(),
                &[area(1, 1, 1048576, 1)],
                None,
                None
            )
            .unwrap_err()
            .contains("200 pages")
        );
    }
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

#[cfg(test)]
mod ui27_cases;
#[cfg(test)]
mod ui27;
