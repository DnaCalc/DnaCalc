//! Presentation of the host search and arithmetic over application projections.
use crate::{cells, wire, workbook::Workbook};
use calc_host::value::{CellKey, CoreRef};
use calc_host::{ProjectedValue, ProjectionRequest, Selection};
use std::collections::BTreeSet;

pub fn host_options(
    options: &wire::FindOptions,
) -> Result<calc_host::search::SearchOptions, String> {
    Ok(calc_host::search::SearchOptions {
        sheet: options.sheet.id(),
        areas: options
            .areas
            .iter()
            .map(|a| a.area().ok_or_else(|| "Invalid search area".to_owned()))
            .collect::<Result<_, _>>()?,
        text: options.text.clone(),
        case_sensitive: options.case_sensitive,
        whole: options.whole,
        wildcards: options.wildcards,
        formulas: options.formulas,
        scope: match options.scope {
            wire::FindScope::Sheet => calc_host::search::SearchScope::Sheet,
            wire::FindScope::Selection => calc_host::search::SearchScope::Selection,
            wire::FindScope::Workbook => calc_host::search::SearchScope::Workbook,
        },
    })
}
pub fn find(book: &Workbook, options: &wire::FindOptions) -> Result<wire::Found, String> {
    let host = book.host();
    let found = calc_host::search::find(host, &host_options(options)?, |cell| {
        if options.formulas {
            cells::input_text(cell.input())
        } else {
            book.shown(cell).text
        }
    })?;
    let rows = found
        .rows
        .into_iter()
        .map(|cell| {
            let at = cell.at();
            let shown = book.shown(&cell);
            wire::FoundRow {
                at: wire::Key::of(at),
                label: dna_formula_ui::display::cell(&cells::WorkbookNames(host), at),
                source: cells::input_text(cell.input()),
                display: shown.text,
                provenance: shown.provenance,
                hidden: host
                    .model()
                    .sheet_by_id(at.sheet())
                    .is_some_and(|s| s.visibility() != calc_host::SheetVisibility::Visible),
            }
        })
        .collect();
    Ok(wire::Found {
        revision: host.revision().get(),
        rows,
        scanned: found.scanned,
        partial: found.partial,
    })
}

pub fn selection_stats(
    book: &Workbook,
    selection: &Selection,
) -> Result<wire::SelectionStats, String> {
    if selection.areas.iter().map(|a| a.cell_count()).sum::<u64>() > 10_000 {
        return Err("Selection summary is limited to 10000 cells; select a smaller range".into());
    }
    let mut cells = BTreeSet::new();
    for area in &selection.areas {
        for row in area.first().row()..=area.last().row() {
            for col in area.first().column()..=area.last().column() {
                cells.insert(CellKey::new(area.sheet(), row, col).unwrap());
            }
        }
    }
    let page = calc_host::project(
        book.host(),
        &ProjectionRequest {
            cells: cells.into_iter().collect(),
        },
    )
    .map_err(|e| e.to_string())?;
    let mut out = wire::SelectionStats {
        selected: page.cells().len() as u64,
        ..Default::default()
    };
    let (mut sum, mut correction) = (0.0, 0.0);
    let mut min = None::<f64>;
    let mut max = None::<f64>;
    for cell in page.cells() {
        let value = match cell.value() {
            ProjectedValue::Literal(value) => {
                out.literal += 1;
                Some(value)
            }
            ProjectedValue::Calculated { value, .. } => {
                out.calculated += 1;
                Some(value)
            }
            ProjectedValue::FileCached(_) => {
                out.cached += 1;
                None
            }
            ProjectedValue::Stale(_) => {
                out.stale += 1;
                None
            }
            ProjectedValue::NotModeled { .. } => {
                out.refused += 1;
                None
            }
            ProjectedValue::Empty => {
                out.blank += 1;
                None
            }
        };
        let Some(value) = value else {
            continue;
        };
        match value.core_ref() {
            CoreRef::Empty => out.blank += 1,
            CoreRef::Text(t) if t.utf16_code_units().is_empty() => {
                out.count += 1;
                out.empty_text += 1;
            }
            CoreRef::Number(n) => {
                out.count += 1;
                out.numeric += 1;
                let y = n - correction;
                let next = sum + y;
                correction = (next - sum) - y;
                sum = next;
                min = Some(min.map_or(n, |m| m.min(n)));
                max = Some(max.map_or(n, |m| m.max(n)));
            }
            CoreRef::Error(_) => {
                out.count += 1;
                out.errors += 1;
            }
            _ => out.count += 1,
        }
    }
    let display = |n: f64| {
        if n.is_finite() {
            n.to_string()
        } else {
            "Overflow".into()
        }
    };
    out.sum = if out.numeric == 0 {
        "—".into()
    } else {
        display(sum)
    };
    out.average = if out.numeric == 0 {
        "—".into()
    } else {
        display(sum / out.numeric as f64)
    };
    out.min = min.map(display).unwrap_or_else(|| "—".into());
    out.max = max.map(display).unwrap_or_else(|| "—".into());
    Ok(out)
}
