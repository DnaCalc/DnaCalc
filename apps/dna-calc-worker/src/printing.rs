//! Print projections from the application's ordinary host, styles, calculated values and chart route.
use crate::{wire, workbook::Workbook};
use calc_host::ProjectedValue;
use calc_host::value::CellKey;
use dna_calc_model::printing::*;
use std::collections::BTreeSet;

pub fn settings(book: &Workbook, sheet: wire::Sheet) -> Result<PageSetupState, String> {
    let host = book.host();
    if host.model().sheet_by_id(sheet.id()).is_none() {
        return Err("Worksheet unavailable (dc-971p)".into());
    }
    Ok(PageSetupState {
        revision: host.revision().get(),
        settings: calc_host::print_plan::page_settings(host, sheet.id()),
        print_area: calc_host::print_name(host, sheet.id(), "_xlnm.Print_Area").unwrap_or_default(),
        print_titles: calc_host::print_name(host, sheet.id(), "_xlnm.Print_Titles")
            .unwrap_or_default(),
    })
}
pub fn change(change: PageSetupChange) -> calc_host::PageSetupChange {
    let brk = |b: PageBreak| calc_host::PageBreak {
        id: b.id,
        min: b.min,
        max: b.max,
        manual: b.manual,
        pivot: b.pivot,
    };
    calc_host::PageSetupChange {
        attributes: change.attributes,
        headers: change.headers,
        row_breaks: change.row_breaks.map(|v| v.into_iter().map(brk).collect()),
        column_breaks: change
            .column_breaks
            .map(|v| v.into_iter().map(brk).collect()),
        print_area: change.print_area,
        print_titles: change.print_titles,
    }
}
pub fn preview(
    book: &Workbook,
    sheet: wire::Sheet,
    revision: u64,
    geometry: wire::Geometry,
    charts: Vec<wire::Chart>,
    shapes: Vec<wire::DrawnShape>,
    sparklines: Vec<dna_calc_model::sparklines::DrawnSparkline>,
    controls: Vec<dna_calc_model::controls::DrawnControl>,
) -> Result<PrintDocument, String> {
    if book.has_pending_calculation() {
        return Err("Calculation is still running; print preview will refresh when it finishes (dc-971p.57)".into());
    }
    let host = book.host();
    if host.revision().get() != revision {
        return Err("The workbook changed; refresh print preview (dc-971p.57)".into());
    }
    let state = settings(book, sheet)?;
    let (chart_views, unread_charts) = host.charts(sheet.id());
    if unread_charts > 0
        || chart_views
            .iter()
            .any(|v| v.plots.iter().flat_map(|p| &p.series).any(|s| s.cached))
    {
        return Err("Print preview contains unread charts or saved chart values; resolve them before printing (dc-971p.57)".into());
    }
    if charts
        .iter()
        .any(|c| c.svg.contains("data-dnacalc=\"chart-notice\""))
    {
        return Err(
            "A chart is preserved but its drawing is not modelled for printing (dc-971p.57)".into(),
        );
    }
    // Every shown object prints as the grid draws it: a geometry not drawn, or a picture shown by its placeholder
    // (an EMF or WMF without a bitmap inside), is refused rather than left off the page.
    let shown = host.shapes(sheet.id()).iter().filter(|s| !s.hidden && s.prints).count();
    if shapes.len() != shown {
        return Err("A shape's geometry is preserved but not drawn; printing it is not modelled (dc-zjsg)".into());
    }
    fn placeholder(shape: &wire::DrawnShape) -> bool {
        shape.picture.as_ref().is_some_and(|p| p.placeholder.is_some()) || shape.children.iter().any(|c| placeholder(&c.shape))
    }
    if shapes.iter().any(placeholder) {
        return Err("A picture's image (EMF or WMF) is preserved but not drawn; printing it is not modelled (dc-zjsg)".into());
    }
    let entry = host
        .model()
        .sheet_by_id(sheet.id())
        .ok_or("Worksheet unavailable")?;
    let setup = host.model().definitions().page_setup(sheet.id());
    setup.validate()?;
    if setup
        .headers
        .values()
        .any(|v| String::from_utf16(v.utf16_code_units()).is_err())
    {
        return Err("A header/footer contains unpaired UTF-16; it is preserved but cannot be printed (dc-971p.57)".into());
    }
    if state
        .settings
        .get("pageSetup", "cellComments")
        .is_some_and(|v| v != "none")
    {
        return Err(
            "Comment printing is preserved; its pagination is not yet modelled (dc-971p.57)".into(),
        );
    }
    if state.settings.flag("pageSetup", "draft", false) {
        return Err("Draft quality depends on the printer; the setting is preserved but browser draft printing is not modelled (dc-971p.57)".into());
    }
    host.loaded().check_print_objects(sheet.id())?;
    let mut notices=vec!["Browser pagination uses the workbook's dimensions. Printer and Excel boundary measurements are pending.".into()];
    if state.settings.get("pageSetup", "paperSize").is_none() {
        notices.push(
            "No paper size is stored. Preview uses Letter; choose a paper size in Page Setup."
                .into(),
        );
    }
    if state
        .settings
        .get("pageSetup", "orientation")
        .is_none_or(|v| v == "default")
    {
        notices.push("Printer-default orientation is not available in the browser. Preview uses portrait; choose an orientation in Page Setup.".into());
    }
    if state.settings.flag("pageSetup", "blackAndWhite", false) {
        notices.push("Black and white preview uses browser grayscale. Excel printer colour conversion is awaiting measurement.".into());
    }
    // Charts reach rows and columns the cells do not: the print range covers them.
    let metrics = book.display_metrics(sheet.id());
    let (mut bottom, mut right) = (0, 0);
    if state.print_area.is_empty() {
        for chart in &charts {
            let (x, y, w, h) = match &chart.place {
                wire::ChartPlace::Cells { from, to } => (
                    metrics.col_start(from.0.3) + from.1,
                    metrics.row_start(from.0.2) + from.2,
                    metrics.col_start(to.0.3) + to.1 - (metrics.col_start(from.0.3) + from.1),
                    metrics.row_start(to.0.2) + to.2 - (metrics.row_start(from.0.2) + from.2),
                ),
                wire::ChartPlace::Sized { from, width, height } => (
                    metrics.col_start(from.0.3) + from.1,
                    metrics.row_start(from.0.2) + from.2,
                    *width,
                    *height,
                ),
                wire::ChartPlace::Absolute { x, y, width, height } => (*x, *y, *width, *height),
            };
            bottom = bottom.max(metrics.row_at((y + h - 1e-7).max(0.), 1048576));
            right = right.max(metrics.col_at((x + w - 1e-7).max(0.), 16384));
        }
    }
    let plan = calc_host::print_plan::print_plan(host, sheet.id(), &geometry, (bottom, right))?;
    // Check cardinality before expanding a full-sheet area, title band, or sparse far-away cell.
    let mut keys = BTreeSet::new();
    for region in plan.pages.iter().flat_map(|p| &p.regions) {
        let a = region.area.area().unwrap();
        if a.cell_count() > MAX_PRINT_CELLS as u64 {
            return Err(
                "Print preview exceeds 100,000 cells; reduce the print area (dc-971p.57)".into(),
            );
        }
        for row in a.first().row()..=a.last().row() {
            if metrics.height(row) == 0. {
                continue;
            }
            for col in a.first().column()..=a.last().column() {
                if metrics.width(col) == 0. {
                    continue;
                }
                keys.insert(CellKey::new(sheet.id(), row, col).unwrap());
                if keys.len() > MAX_PRINT_CELLS {
                    return Err(
                        "Print preview exceeds 100,000 cells; reduce the print area (dc-971p.57)"
                            .into(),
                    );
                }
            }
        }
    }
    for merged in &geometry.merges {
        if plan.pages.iter().flat_map(|p| &p.regions).any(|r| {
            r.area.0.2 <= merged.1.2
                && r.area.1.2 >= merged.0.2
                && r.area.0.3 <= merged.1.3
                && r.area.1.3 >= merged.0.3
        }) {
            keys.insert(merged.0.cell().ok_or("Invalid merged print cell")?);
        }
    }
    if keys.len() > MAX_PRINT_CELLS {
        return Err(
            "Print preview exceeds 100,000 cells; reduce the print area (dc-971p.57)".into(),
        );
    }
    let projected = book.project(keys.into_iter().collect());
    let mut cells = Vec::new();
    let mut retained = 0usize;
    let mut circular_holds = false;
    let mut external_caches = false;
    for p in projected {
        match p.value() {
            ProjectedValue::FileCached(_)
            | ProjectedValue::NotModeled { .. }
            | ProjectedValue::Stale(_) => {
                return Err("Print preview contains uncalculated, refused or stale formulas. \
                            Calculate or resolve them first (dc-971p.57)".into());
            }
            ProjectedValue::Calculated { calculation, .. } => {
                circular_holds |= calculation.held_by_circular_rule();
                external_caches |= calculation.reads_external_links();
                if calculation.stamp().is_some_and(|stamp| {
                    stamp.reads.uses_boundary_cache || stamp.reads.uses_imported_cache
                }) {
                    return Err("Print preview contains formulas calculated from cached inputs. \
                                Refresh the inputs and recalculate before printing (dc-971p.57)".into());
                }
            }
            ProjectedValue::Empty | ProjectedValue::Literal(_) => {}
        }
        let shown = book.shown(&p);
        if shown.style.unsupported.is_some() || shown.format == wire::FormatShown::NotModelled {
            return Err(
                "A cell's formatting is preserved but not modelled for printing (dc-971p.57)"
                    .into(),
            );
        }
        let cell = book.wire_cell(&p, &shown);
        retained = retained.saturating_add(cell.retained_bytes());
        if retained > wire::MAX_PUBLICATION_BYTES / 2 {
            return Err("Print preview exceeds the retained display budget; reduce the print area (dc-971p.57)".into());
        }
        cells.push(cell);
    }
    if circular_holds {
        notices.push("Print preview includes values held by circular references.".into());
    }
    if external_caches {
        notices.push("Print preview includes values from saved external link caches.".into());
    }
    Ok(PrintDocument {
        revision,
        sheet,
        sheet_name: entry.name().text().to_string_lossy(),
        book_name: book.name().into(),
        settings: state.settings,
        geometry,
        plan,
        cells,
        charts,
        pictures: pictures(&host.shapes(sheet.id()).iter().filter(|s| s.prints).cloned().collect::<Vec<_>>()),
        shapes,
        sparklines,
        controls,
        notices,
    })
}

/// The images of a sheet's pictures, in groups too, as `data:` URLs by their identity: a picture whose image
/// the page cannot decode keeps its placeholder (dc-zjsg.3).
fn pictures(shapes: &[calc_host::Shape]) -> Vec<(u32, String)> {
    fn walk(shapes: &[calc_host::Shape], out: &mut Vec<(u32, String)>) {
        for shape in shapes {
            if (shape.kind == calc_host::ShapeKind::Picture || matches!(shape.fill, calc_host::ShapeFill::Picture(_)))
                && let Some((content_type, bytes)) = shape.media.as_ref().and_then(|m| crate::picture::source(m).ok())
            {
                out.push((shape.id, dna_calc_model::drawn_shapes::data_url(content_type, &bytes)));
            }
            walk(&shape.children, out);
        }
    }
    let mut out = Vec::new();
    walk(shapes, &mut out);
    out
}
