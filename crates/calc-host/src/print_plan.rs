//! The one pagination route (dc-oxh8w.11): the sheet's stored page setup, geometry and print ranges go through
//! `calc_host::printing::paginate`, the route print preview, Page Break Preview and PDF print take, and
//! `Worksheet.HPageBreaks`, `VPageBreaks` and `PageSetup.Pages` read the same plan. The positions of the
//! automatic breaks are calibrated against the Excel PB1 answers (`crates/calc-host/src/printing/pb1.rs`).

use crate::HostState;
use crate::value::{CellKey, GridArea};
use calc_value::SheetId;
use crate::printing::{Geometry, PageBreak as ModelBreak, PageSettings, PrintPlan, paginate};
use calc_skin::grid::Area;

/// The geometry pagination reads: sizes, the default font's rates and the merges.
pub fn print_geometry(host: &HostState, sheet: SheetId) -> Geometry {
    let mut print = Geometry {
        default_digit_px: Some(host.default_digit_px()),
        default_font: host.default_font_name().map(str::to_owned),
        merges: host.merged_areas(sheet).into_iter().map(Area::of).collect(),
        ..Geometry::default()
    };
    if let Some(geometry) = host.sheet_geometry(sheet) {
        print.default_row_height = geometry.default_row_height;
        print.default_column_width = geometry.default_column_width;
        print.base_column_width = geometry.base_column_width;
        print.columns = geometry.columns.iter().map(|span| (span.first, span.last, span.width, span.hidden)).collect();
        print.rows = geometry.rows.iter().map(|(&row, format)| (row, format.height, format.hidden)).collect();
    }
    print.default_row_height.get_or_insert(host.default_font_row_points());
    print
}

/// The sheet's stored page setup as the pagination reads it.
pub fn page_settings(host: &HostState, sheet: SheetId) -> PageSettings {
    let setup = host.model().definitions().page_setup(sheet);
    let brk = |b: crate::PageBreak| ModelBreak { id: b.id, min: b.min, max: b.max, manual: b.manual, pivot: b.pivot };
    PageSettings {
        sections: setup.sections,
        headers: setup.headers.into_iter().map(|(k, v)| (k, v.to_string_lossy())).collect(),
        row_breaks: setup.row_breaks.into_iter().map(brk).collect(),
        column_breaks: setup.column_breaks.into_iter().map(brk).collect(),
    }
}

/// The print ranges: the print area, else the used area, formatted cells and merges, extended to
/// `extent` (the last row and column a drawing reaches; `(0, 0)` for none).
pub fn print_areas(
    host: &HostState,
    sheet: SheetId,
    print_area: &str,
    geometry: &Geometry,
    extent: (u32, u32),
) -> Result<Vec<GridArea>, String> {
    if !print_area.is_empty() {
        return crate::print_references(host, sheet, print_area);
    }
    let (mut bottom, mut right) = extent;
    if let Some(a) = host.used_area(sheet) {
        bottom = bottom.max(a.last().row());
        right = right.max(a.last().column());
    }
    for r in crate::format_regions(host, GridArea::full_sheet(sheet))?.into_iter().filter(|r| r.format != 0) {
        bottom = bottom.max(r.area.last().row());
        right = right.max(r.area.last().column());
    }
    for a in &geometry.merges {
        bottom = bottom.max(a.1.2);
        right = right.max(a.1.3);
    }
    if bottom == 0 || right == 0 {
        return Ok(Vec::new());
    }
    let area = GridArea::new(CellKey::new(sheet, 1, 1).unwrap(), CellKey::new(sheet, bottom, right).unwrap());
    Ok(area.into_iter().collect())
}

/// The print-title bands: one whole-row band and one whole-column band.
pub fn print_titles(
    host: &HostState,
    sheet: SheetId,
    titles: &str,
) -> Result<(Option<(u32, u32)>, Option<(u32, u32)>), String> {
    let (mut rows, mut cols) = (None, None);
    if !titles.is_empty() {
        for a in crate::print_references(host, sheet, titles)? {
            let (first, last) = (a.first(), a.last());
            if first.column() == 1 && last.column() == 16384 && rows.is_none() {
                rows = Some((first.row(), last.row()));
            } else if first.row() == 1 && last.row() == 1048576 && cols.is_none() {
                cols = Some((first.column(), last.column()));
            } else {
                return Err("Print titles need one whole-row and one whole-column band (dc-971p.57)".into());
            }
        }
    }
    Ok((rows, cols))
}

/// The pagination of one sheet, from its stored setup.
pub fn print_plan(
    host: &HostState,
    sheet: SheetId,
    geometry: &Geometry,
    extent: (u32, u32),
) -> Result<PrintPlan, String> {
    let settings = page_settings(host, sheet);
    let area = crate::print_name(host, sheet, "_xlnm.Print_Area").unwrap_or_default();
    let titles = crate::print_name(host, sheet, "_xlnm.Print_Titles").unwrap_or_default();
    let (rows, cols) = print_titles(host, sheet, &titles)?;
    let areas = print_areas(host, sheet, &area, geometry, extent)?;
    paginate(&settings, geometry, &areas.into_iter().map(Area::of).collect::<Vec<_>>(), rows, cols)
}

/// `PageSetup.Pages.Count`: the pages of the sheet's pagination (Excel PB1 `pages` lines).
pub fn page_count(host: &HostState, sheet: SheetId) -> Result<u32, String> {
    let geometry = print_geometry(host, sheet);
    print_plan(host, sheet, &geometry, (0, 0)).map(|plan| plan.pages.len() as u32)
}
