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
    g.default_column_width = Some(c.column_width);
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
