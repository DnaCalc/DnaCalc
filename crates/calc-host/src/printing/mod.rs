//! Worksheet pagination used by preview, browser print, and the Excel object model.

use calc_skin::grid::{Area, Key};
use calc_skin::metrics::{ColumnBand, GridMetrics, RowBand, SheetBands, Sizes, sheet_metrics};
pub use calc_skin::print::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default)]
pub struct Geometry {
    pub default_digit_px: Option<f64>,
    pub default_font: Option<String>,
    pub default_row_height: Option<f64>,
    pub default_column_width: Option<f64>,
    pub base_column_width: Option<f64>,
    pub column_outline_levels: u8,
    pub row_outline_levels: u8,
    pub columns: Vec<ColumnBand>,
    pub rows: Vec<RowBand>,
    pub merges: Vec<Area>,
}

impl Geometry {
    fn bands(&self) -> SheetBands<'_> {
        SheetBands {
            default_digit_px: self.default_digit_px,
            default_row_height: self.default_row_height,
            default_column_width: self.default_column_width,
            base_column_width: self.base_column_width,
            column_outline_levels: self.column_outline_levels,
            row_outline_levels: self.row_outline_levels,
            columns: &self.columns,
            rows: &self.rows,
        }
    }
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
// UI27 and UI30 (Excel 16.0.20527, Microsoft Print to PDF at 600 dpi, 100%): printed bands are
// whole device pixels. A column prints as `digit * characters + padding` device pixels, where the
// digit and the padding are the Normal font's at the printer: a screen column of `px` pixels
// (7 pixels a digit and 5 of padding at Aptos Narrow 11 and Calibri 11) prints as
// `(px - 5) * 47 / 7 + 34`, so 12, 19, 40, 64, 75, 110, 145, 285 and 705 pixels print as 81,
// 128, 269, 430, 504, 739, 974, 1914 and 4500 (a column wider than the printable width is cut to
// it). Arial 10 prints 46 a digit with 33 of padding, Times New Roman 14 59 with 46 (9 pixels a
// digit, 7 of padding). Row heights print at a font-dependent rate: 15 pt is 127 device pixels at
// Aptos Narrow 11, 121 at Calibri 11 and Arial 10, 116 at Times New Roman 14. Widths and heights
// are rounded after the print scale is applied (q2-*, q3-fit-*). The model has no Normal font,
// so it uses the Aptos Narrow 11 rates, which are the new-workbook default.
const COLUMN_DIGIT_PX: f64 = 7.;
const COLUMN_PADDING_PX: f64 = 5.;
const COLUMN_DIGIT_DEVICE: f64 = 47.;
const COLUMN_PADDING_DEVICE: f64 = 34.;
/// A Calibri 11 workbook (default digit 7 px; PB1, Excel 16.0.20527, Microsoft Print to PDF): 15 pt rows print as 121
/// device pixels (UI30), and a 64 px column prints as 459 or 460 (the 13 PB1 layouts and 10 papers bound it to that
/// from both sides: A4 portrait fits 8 such columns across and not 9, B4 fits 11 and not 12). Its slope is taken from
/// the Aptos Narrow rate and the padding from that one fit: 63 device pixels, not 34. Wider columns are only bounded
/// (a 145 px column prints between 960 and 1152), so those are the least certain of the Calibri rates.
const CALIBRI_ROW_DEVICE_PER_POINT: f64 = 7.975;
const CALIBRI_COLUMN_PADDING_DEVICE: f64 = 63.;
/// The rates of one workbook's Normal font as far as they are known.
#[derive(Clone, Copy)]
struct Rates {
    row_per_point: f64,
    column_padding: f64,
}
fn rates(geometry: &Geometry) -> Rates {
    let calibri = match geometry.default_font.as_deref() {
        Some(name) => name.eq_ignore_ascii_case("Calibri"),
        None => geometry.default_digit_px == Some(7.),
    };
    if calibri {
        Rates { row_per_point: CALIBRI_ROW_DEVICE_PER_POINT, column_padding: CALIBRI_COLUMN_PADDING_DEVICE }
    } else {
        Rates { row_per_point: ROW_DEVICE_PER_POINT, column_padding: COLUMN_PADDING_DEVICE }
    }
}
/// Device pixels per point of row height: UI30 P2 puts it between 8.45 and 8.4583 (rows of 8,
/// 12, 12.75, 15, 20 and 30 pt print as 68, 101, 108, 127, 169 and 254).
const ROW_DEVICE_PER_POINT: f64 = 8.454;
/// The printed width of the row-heading strip (UI30 P3, Aptos Narrow 11 and Calibri 11): the
/// digits of the largest row number printed, at least three, at 47 device pixels each, and 32
/// of padding. The strip is the same for 9, 99 and 999 (173) and grows by 47 a digit (220, 267
/// and 314 for 9,999, 99,999 and 999,999). The column-heading strip is one default row (127).
fn heading_width_device(last_row: u32) -> f64 {
    let digits = last_row.max(1).to_string().len().max(3);
    COLUMN_DIGIT_DEVICE * digits as f64 + 32.
}
/// The printed width of a column of `px` screen pixels, before the print scale.
fn column_device(px: f64, rates: Rates) -> f64 {
    if px <= 0. {
        0.
    } else {
        (px - COLUMN_PADDING_PX) * COLUMN_DIGIT_DEVICE / COLUMN_DIGIT_PX + rates.column_padding
    }
}
/// Page and margin sizes: 600 dpi device pixels per CSS pixel.
const PAGE_DEVICE_PER_PX: f64 = 600. / 96.;
/// Bands in device pixels at one print scale, indexed like the pixel metrics.
fn device_metrics(m: &GridMetrics, geometry: &Geometry, scale: f64, last_row: u32) -> GridMetrics {
    let round = |v: f64| (v * scale).round();
    let rates = rates(geometry);
    let row_default = geometry.default_row_height.unwrap_or(15.) * rates.row_per_point;
    let col_default = column_device(m.col_width, rates);
    let rows = geometry.rows.iter().map(|&(row, height, hidden)| {
        (
            row,
            if hidden {
                0.
            } else {
                round(height.map_or(row_default, |h| h * rates.row_per_point))
            },
        )
    });
    let mut columns = Vec::new();
    for &(first, last, _, hidden) in &geometry.columns {
        for column in first..=last.min(16_384) {
            let size = if hidden {
                0.
            } else {
                round(column_device(m.col_start(column + 1) - m.col_start(column), rates))
            };
            columns.push((column, size));
        }
    }
    GridMetrics {
        scale: 1.,
        row_height: round(row_default),
        col_width: round(col_default),
        header_h: round(m.header_h * rates.row_per_point * 0.75),
        header_w: round(heading_width_device(last_row)),
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
                // UI30 P7 (two manual breaks in the band, titles 1:10, breaks above rows 4 and 8):
                // the page that starts at row 4 shows the band up to the next break, rows 1..7, and
                // the page that starts at row 8 repeats the whole band and carries on.
                if let Some(&id) = breaks.range(start..b).next() {
                    out.push(Slice {
                        first: a,
                        last: id,
                        title: None,
                        length: length(m, rows, a, id),
                        manual: true,
                    });
                    start = id + 1;
                    continue;
                }
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
/// The page of an Excel paper code (`pageSetup paperSize`, the DEVMODE `dmPaperSize` / `xlPaperSize` list) in
/// CSS pixels, portrait as the list gives it. The sizes are the list's own (inches or millimetres); that
/// Excel prints a code on exactly this paper has not been measured except for 1, 5, 7, 8, 9 and 11
/// (UI27), and a printer may substitute its own paper (assumed).
fn paper_size(code: u32) -> Option<(f64, f64)> {
    const IN: f64 = 96.;
    const MM: f64 = 96. / 25.4;
    Some(match code {
        1 | 2 | 18 => (8.5 * IN, 11. * IN),
        3 | 17 => (11. * IN, 17. * IN),
        5 => (8.5 * IN, 14. * IN),
        6 => (5.5 * IN, 8.5 * IN),
        7 => (7.25 * IN, 10.5 * IN),
        8 => (297. * MM, 420. * MM),
        9 | 10 => (210. * MM, 297. * MM),
        11 => (148. * MM, 210. * MM),
        // PB1 (Microsoft Print to PDF): Excel paged code 12 on 364 mm of height (64 rows of 15 pt between 0.75 inch margins), the
        // JIS B4 sheet, not the list's 250 x 354 mm (ISO B4 is code 33); the printer's substitution is what Excel printed on.
        12 => (257. * MM, 364. * MM),
        13 => (182. * MM, 257. * MM),
        14 => (8.5 * IN, 13. * IN),
        15 => (215. * MM, 275. * MM),
        16 => (10. * IN, 14. * IN),
        19 => (3.875 * IN, 8.875 * IN),
        20 => (4.125 * IN, 9.5 * IN),
        27 => (110. * MM, 220. * MM),
        28 => (162. * MM, 229. * MM),
        29 => (324. * MM, 458. * MM),
        30 => (229. * MM, 324. * MM),
        31 => (114. * MM, 162. * MM),
        32 => (114. * MM, 229. * MM),
        33 => (250. * MM, 353. * MM),
        34 => (176. * MM, 250. * MM),
        35 => (125. * MM, 176. * MM),
        37 => (3.875 * IN, 7.5 * IN),
        38 => (3.625 * IN, 6.5 * IN),
        43 => (100. * MM, 148. * MM),
        66 => (420. * MM, 594. * MM),
        70 => (105. * MM, 148. * MM),
        _ => return None,
    })
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
        settings
            .get("pageSetup", "paperSize")
            .unwrap_or("1")
            .parse::<u32>()
            .ok()
            .and_then(paper_size)
            .ok_or_else(|| {
                fail(
                    "This paper size has no browser preview dimensions; choose a size from Excel's paper list or enter custom dimensions",
                )
            })?
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
    let m = sheet_metrics(Some(geometry.bands()));
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
        let mut spans = BTreeMap::<u32, Vec<(u32, u32)>>::new();
        for b in if rows {
            &settings.row_breaks
        } else {
            &settings.column_breaks
        } {
            if !b.manual {
                continue;
            }
            if b.min > b.max {
                return Err(fail("Invalid page-break span"));
            }
            if b.id < first || b.id > last || b.max < cross_first || b.min > cross_last {
                continue;
            }
            spans.entry(b.id).or_default().push((b.min, b.max));
        }
        let mut out = BTreeSet::new();
        // Several stored spans can describe one cut. The pivot flag records its origin.
        for (id, mut spans) in spans {
            spans.sort_unstable();
            let mut next = cross_first;
            for (first, last) in spans {
                if first > next {
                    break;
                }
                next = next.max(last.saturating_add(1));
            }
            if next <= cross_last {
                return Err(fail(
                    "A page break crossing only part of a print area needs Excel pagination measurements",
                ));
            }
            out.insert(id);
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
        let d = device_metrics(&m, geometry, scale, a.last().row());
        let (hw, hh) = if headings {
            (d.header_w, d.header_h)
        } else {
            (0., 0.)
        };
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
                let last_row = areas
                    .iter()
                    .filter_map(|a| a.area())
                    .map(|a| a.last().row())
                    .max()
                    .unwrap_or(1);
                let d = device_metrics(&m, geometry, s, last_row);
                let (hw, hh) = if headings {
                    (d.header_w, d.header_h)
                } else {
                    (0., 0.)
                };
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
                    Key(sheet.0, sheet.1, top, left),
                    Key(sheet.0, sheet.1, bottom, right),
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

#[cfg(test)]
mod tests {
    use super::*;
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
        s.row_breaks.push(PageBreak {
            id: 20,
            min: 8,
            max: 15,
            manual: true,
            pivot: true,
        });
        let combined =
            paginate(&s, &Geometry::default(), &[area(1, 1, 120, 16)], None, None).unwrap();
        assert_eq!(combined.pages[0].body, area(1, 1, 20, 4));
        assert!(combined.pages[0].manual_bottom && combined.pages[0].manual_right);
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
    fn a_plans_breaks_list_as_excels_hpagebreaks_do_with_manual_ones_marked() {
        // The UI27 q4-manual sheet: a stored row break after row 20 (max 15 of 16 columns) and a column break after column 4.
        let mut s = setup();
        s.row_breaks.push(PageBreak { id: 20, min: 0, max: 15, manual: true, pivot: false });
        s.column_breaks.push(PageBreak { id: 4, min: 0, max: 119, manual: true, pivot: false });
        let plan = paginate(&s, &Geometry::default(), &[area(1, 1, 120, 16)], None, None).unwrap();
        let breaks = plan.breaks();
        // Excel lists the FIRST row of the next page: row 21 and column E (5), both manual; the automatic ones follow.
        assert_eq!(breaks.rows[0], (21, true));
        assert_eq!(breaks.columns[0], (5, true));
        assert!(breaks.rows[1..].iter().all(|(_, manual)| !manual));
        let rows: Vec<u32> = breaks.rows.iter().map(|(r, _)| *r).collect();
        assert!(rows.windows(2).all(|w| w[0] < w[1]));
        // No print area bigger than one page: no break.
        let single = paginate(&setup(), &Geometry::default(), &[area(1, 1, 5, 3)], None, None).unwrap();
        assert_eq!(single.breaks(), PlanBreaks::default());
        assert_eq!(single.breaks_text("t"), "t\tpages\t1\n");
        // The text is what PB1.bas writes: tag, H/V, index, the next page's first cell, the break type.
        let text = plan.breaks_text("c11");
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some(format!("c11\tpages\t{}", plan.pages.len()).as_str()));
        assert_eq!(lines.next(), Some("c11\tH\t1\tA21\t-4135"));
        assert!(text.contains("\nc11\tV\t1\tE1\t-4135\n"), "{text}");
        assert!(text.contains("\t-4105"), "{text}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn excels_paper_list_gives_a_page_to_each_code_and_refuses_unknown_ones() {
        let size = |code: &str| {
            let mut s = setup();
            set(&mut s, "pageSetup", "paperSize", code);
            paper(&s)
        };
        let close = |got: (f64, f64), want: (f64, f64)| (got.0 - want.0).abs() < 1e-6 && (got.1 - want.1).abs() < 1e-6;
        // The six measured in UI27.
        assert_eq!(size("1").unwrap(), (8.5 * 96., 11. * 96.));
        assert!(close(size("9").unwrap(), (210. * 96. / 25.4, 297. * 96. / 25.4)));
        // Others of the list: Folio, B4 (JIS), the envelopes, A2 and A6 (inches or millimetres as the list gives).
        assert_eq!(size("14").unwrap(), (8.5 * 96., 13. * 96.));
        // Code 12 is the JIS B4 sheet Excel paged on in PB1 (364 mm tall), not the list's 250 x 354 mm.
        assert!(close(size("12").unwrap(), (257. * 96. / 25.4, 364. * 96. / 25.4)));
        assert_eq!(size("20").unwrap(), (4.125 * 96., 9.5 * 96.));
        assert!(close(size("66").unwrap(), (420. * 96. / 25.4, 594. * 96. / 25.4)));
        assert!(close(size("70").unwrap(), (105. * 96. / 25.4, 148. * 96. / 25.4)));
        // A code no list entry has (or a printer's own paper, 256 and up) is refused, naming what to do.
        assert!(size("256").unwrap_err().contains("custom dimensions"));
        assert!(size("not a number").is_err());
        // Landscape swaps them, whatever the code.
        let mut s = setup();
        set(&mut s, "pageSetup", "paperSize", "14");
        set(&mut s, "pageSetup", "orientation", "landscape");
        assert_eq!(paper(&s).unwrap(), (13. * 96., 8.5 * 96.));
        // A sheet paginates on a code the old table did not have: Folio is shorter than Legal, longer than Letter.
        let pages = |code: &str| {
            let mut s = setup();
            set(&mut s, "pageSetup", "paperSize", code);
            paginate(&s, &Geometry::default(), &[area(1, 1, 200, 3)], None, None).unwrap().pages.len()
        };
        assert!(pages("5") <= pages("14") && pages("14") <= pages("1"));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dw5_dc_rlkcm__aptos_narrow_and_calibri_select_distinct_print_rates_at_7px_digit() {
        let mut aptos = Geometry::default();
        aptos.default_digit_px = Some(7.);
        aptos.default_font = Some("Aptos Narrow".into());
        let mut calibri = Geometry::default();
        calibri.default_digit_px = Some(7.);
        calibri.default_font = Some("Calibri".into());
        assert_eq!(rates(&aptos).row_per_point, ROW_DEVICE_PER_POINT);
        assert_eq!(rates(&aptos).column_padding, COLUMN_PADDING_DEVICE);
        assert_eq!(rates(&calibri).row_per_point, CALIBRI_ROW_DEVICE_PER_POINT);
        assert_eq!(rates(&calibri).column_padding, CALIBRI_COLUMN_PADDING_DEVICE);
    }
}

#[cfg(test)]
mod pb1;
#[cfg(test)]
mod ui27;
#[cfg(test)]
mod ui27_cases;
