//! Page boundaries against the printed pages Excel produced in the UI27 measurements
//! (Excel 16.0.20527, Microsoft Print to PDF at 600 dpi, Letter and A4, 0.5 inch margins).
use super::ui27_cases::{CASES, Case};
use super::*;
use crate::projection::Key;

fn area(a: (u32, u32, u32, u32)) -> Area {
    Area(Key(0, 0, a.0, a.1), Key(0, 0, a.2, a.3))
}
fn set(settings: &mut PageSettings, s: &str, n: &str, v: &str) {
    settings
        .sections
        .entry(s.into())
        .or_default()
        .insert(n.into(), v.into());
}
fn plan(c: &Case) -> Result<PrintPlan, String> {
    let mut s = PageSettings::default();
    for name in ["left", "right", "top", "bottom"] {
        set(&mut s, "pageMargins", name, "0.5");
    }
    set(&mut s, "pageSetup", "paperSize", &c.paper.to_string());
    if c.landscape {
        set(&mut s, "pageSetup", "orientation", "landscape");
    }
    if c.zoom == 0 {
        set(&mut s, "pageSetUpPr", "fitToPage", "1");
        set(&mut s, "pageSetup", "fitToWidth", &c.wide.to_string());
        set(&mut s, "pageSetup", "fitToHeight", &c.tall.to_string());
    } else {
        set(&mut s, "pageSetup", "scale", &c.zoom.to_string());
    }
    if c.across {
        set(&mut s, "pageSetup", "pageOrder", "overThenDown");
    }
    if c.headings {
        set(&mut s, "printOptions", "headings", "1");
    }
    for (rows, list) in [(true, c.row_breaks), (false, c.column_breaks)] {
        for &(id, min, max) in list {
            let b = PageBreak {
                id,
                min,
                max,
                manual: true,
                pivot: false,
            };
            if rows {
                s.row_breaks.push(b);
            } else {
                s.column_breaks.push(b);
            }
        }
    }
    let mut g = Geometry::default();
    g.default_row_height = Some(c.row_height);
    // The workbooks size columns A to P and leave the rest at the default width.
    g.default_column_width = Some(9.140625);
    g.columns.push((1, 16, Some(c.column_width), false));
    let areas: Vec<Area> = c.areas.iter().map(|a| area(*a)).collect();
    paginate(&s, &g, &areas, c.title_rows, c.title_columns)
}
fn spans(p: &PrintPage) -> (u32, u32, u32, u32) {
    let rows = p.regions.iter().map(|r| (r.area.0.2, r.area.1.2));
    let cols = p.regions.iter().map(|r| (r.area.0.3, r.area.1.3));
    (
        rows.clone().map(|r| r.0).min().unwrap(),
        rows.map(|r| r.1).max().unwrap(),
        cols.clone().map(|r| r.0).min().unwrap(),
        cols.map(|r| r.1).max().unwrap(),
    )
}
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn automatic_page_boundaries_match_the_excel_ui27_pdfs() {
    let mut wrong = Vec::new();
    for c in CASES {
        // A grouped drawing is printed as one extra picture spanning the grid on page five;
        // that is a drawing effect, not a page boundary (the suppressed twin matches).
        if c.id == "q10-drawings-group-printed" {
            continue;
        }
        match plan(c) {
            Err(e) => wrong.push(format!("{}: refused: {e}", c.id)),
            Ok(p) => {
                let got: Vec<_> = p.pages.iter().map(spans).collect();
                if got != c.pages {
                    wrong.push(format!(
                        "{}: scale {} got {:?} want {:?}",
                        c.id,
                        p.scale,
                        &got[..got.len().min(8)],
                        &c.pages[..c.pages.len().min(8)]
                    ));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{} of {} cases differ:\n{}", wrong.len(), CASES.len(), wrong.join("\n"));
}

/// The header and footer strings of UI27 q7-headers (Excel 16.0.20527), six pages numbered from
/// 4, a different first page and different odd and even pages: what each page printed.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn headers_and_footers_print_what_excel_printed_in_q7() {
    let mut s = PageSettings::default();
    set(&mut s, "headerFooter", "differentFirst", "1");
    set(&mut s, "headerFooter", "differentOddEven", "1");
    let strings = [
        ("firstHeader", "&LFIRST &P / &N"),
        ("firstFooter", "&CFIRSTFOOT &P"),
        ("evenHeader", "&LEVEN &P / &N"),
        ("evenFooter", "&CEVENFOOT &P"),
        (
            "oddHeader",
            "&LODD &P / &N && &A&C&\"Calibri,Bold\"&12&KFF0000RED &UUNDER&U",
        ),
        ("oddFooter", "&CFOOT &[Page] / &[Pages]"),
    ];
    for (name, text) in strings {
        s.headers.insert(name.into(), text.into());
    }
    let expected = [
        ("FIRST 4 / 6", "", "FIRSTFOOT 4"),
        ("EVEN 5 / 6", "", "EVENFOOT 5"),
        ("ODD 6 / 6 & Pagination", "RED UNDER", "FOOT Page] / Pages]"),
        ("EVEN 7 / 6", "", "EVENFOOT 7"),
        ("ODD 8 / 6 & Pagination", "RED UNDER", "FOOT Page] / Pages]"),
        ("EVEN 9 / 6", "", "EVENFOOT 9"),
    ];
    for (index, (left, centre, footer)) in expected.iter().enumerate() {
        let number = 4 + index as u32;
        let context = HeaderContext {
            page: number,
            pages: 6,
            sheet: "Pagination",
            book: "saved.xlsx",
            date: "",
            time: "",
        };
        let text = |footer: bool, section: usize| {
            let runs = header_runs(page_header(&s, index, number, footer), &context).unwrap();
            runs[section].iter().map(|r| r.text.as_str()).collect::<String>()
        };
        assert_eq!(text(false, 0), *left, "page {}", index + 1);
        assert_eq!(text(false, 1), *centre, "page {}", index + 1);
        assert_eq!(text(true, 1), *footer, "page {}", index + 1);
    }
}

/// UI30 P1 and P2 (Excel 16.0.20527, Microsoft Print to PDF at 600 dpi, Letter, 100%): the
/// printed pitch of columns and rows of other sizes at the default Aptos Narrow 11 Normal font,
/// from the PDF label positions (pure arithmetic over the measured pairs).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn column_and_row_sizes_print_as_excel_printed_them_in_ui30() {
    // (screen pixels of ColumnWidth 1, 2, 5, 8.43, 10, 15, 20, 40, 60), device pixels printed.
    let columns = [
        (12., 81.),
        (19., 128.),
        (40., 269.),
        (64., 430.),
        (75., 504.),
        (110., 739.),
        (145., 974.),
        (285., 1914.),
        (425., 2854.),
    ];
    for (px, device) in columns {
        assert_eq!(column_device(px).round(), device, "{px} px");
    }
    // Row heights of 8, 12, 12.75, 15, 20 and 30 pt.
    for (points, device) in [(8., 68.), (12., 101.), (12.75, 108.), (15., 127.), (20., 169.), (30., 254.)] {
        assert_eq!((points * ROW_DEVICE_PER_POINT).round(), device, "{points} pt");
    }
}

/// UI30 P3: the row-heading strip by the largest printed row number.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn heading_strip_widths_follow_the_row_number_digits_in_ui30() {
    for (last_row, device) in [(9, 173.), (99, 173.), (999, 173.), (9_999, 220.), (99_999, 267.), (999_999, 314.)] {
        assert_eq!(heading_width_device(last_row), device, "row {last_row}");
    }
}

/// UI30 P6: the fit-to-width cases with print title columns that the model does not reproduce.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn fit_with_title_columns_is_still_an_open_difference_from_excel() {
    for c in super::ui27_cases::UNMATCHED {
        let got = plan(c).map(|p| p.pages.iter().map(spans).collect::<Vec<_>>());
        assert_ne!(got.as_ref().ok(), Some(&c.pages.to_vec()), "{}: fixed, move it to CASES", c.id);
    }
}

/// UI30 P4: 19 columns whose printed widths sum to exactly the 4500 device pixels of the
/// printable width (18 columns of ColumnWidth 4, 222 each, and one of ColumnWidth 10, 504, in
/// either order) do not fit: Excel's first page holds 18 columns, as it holds 49 rows of 120 pixels
/// (14.2 pt) when 50 would fill the 6000 of the printable height (the p4-rows-exact cases).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn exactly_filled_column_pages_do_not_fit_in_ui30() {
    for wide_first in [false, true] {
        let mut s = PageSettings::default();
        for name in ["left", "right", "top", "bottom"] {
            set(&mut s, "pageMargins", name, "0.5");
        }
        let mut g = Geometry::default();
        g.default_row_height = Some(15.);
        g.default_column_width = Some(9.140625);
        if wide_first {
            g.columns.push((1, 1, Some(10.7109375), false));
            g.columns.push((2, 24, Some(4.7109375), false));
        } else {
            g.columns.push((1, 18, Some(4.7109375), false));
            g.columns.push((19, 19, Some(10.7109375), false));
            g.columns.push((20, 24, Some(4.7109375), false));
        }
        let plan = paginate(&s, &g, &[area((1, 1, 3, 24))], None, None).unwrap();
        let got: Vec<_> = plan.pages.iter().map(spans).collect();
        assert_eq!(got, [(1, 3, 1, 18), (1, 3, 19, 24)], "wide first {wide_first}");
    }
}
