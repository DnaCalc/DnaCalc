//! Workbook canvas projection.
use crate::{cells, wire, workbook::Workbook};
use calc_value::CellKey;
use cells::Provenance;
pub use dna_calc_model::canvas::*;
/// The canvas around `center` as the page draws it (the workbook's server works it out, dc-p96):
/// the trace, each chip's text, the wires and the centre cell's inspector.
pub fn view_model(book: &Workbook, center: CellKey, back: u8, forward: u8) -> wire::Canvas {
    let host = book.host();
    let profile = book.profile().cloned();
    let trace = trace(
        center,
        back,
        forward,
        PER_COLUMN,
        |at| host.precedents(at),
        |at| host.dependents(at),
    );
    let center_sheet = center.sheet();
    let mut chips = Vec::new();
    let mut hidden = Vec::new();
    for (column, nodes) in &trace.columns {
        for (row, node) in nodes.iter().enumerate() {
            let (x, y) = position(&trace, *column, row);
            let chip = match *node {
                Node::Cell(at) => {
                    let label = if at.sheet() == center_sheet {
                        cells::address(at.row(), at.column())
                    } else {
                        cells::cell_label(host, at)
                    };
                    let (input, value, provenance) = match book.cell(at) {
                        Some(cell) => {
                            let shown = cells::shown(&cell, profile.as_ref(), book.cell_format(at));
                            let value = if shown.provenance == Provenance::Refused {
                                "refused".to_owned()
                            } else {
                                shown.text
                            };
                            (cells::input_text(cell.input()), value, shown.provenance)
                        }
                        None => (String::new(), String::new(), Provenance::Empty),
                    };
                    wire::Chip {
                        column: *column,
                        x,
                        y,
                        reference: cells::cell_label(host, at),
                        sheet: wire::Sheet::of(at.sheet()),
                        label,
                        cell: Some(wire::Key::of(at)),
                        input,
                        value,
                        provenance,
                        count: 1,
                    }
                }
                Node::Area(area) => wire::Chip {
                    column: *column,
                    x,
                    y,
                    label: if area.sheet() == center_sheet {
                        format!(
                            "{}:{}",
                            cells::address(area.first().row(), area.first().column()),
                            cells::address(area.last().row(), area.last().column())
                        )
                    } else {
                        cells::area_label(host, area)
                    },
                    reference: cells::area_label(host, area),
                    sheet: wire::Sheet::of(area.sheet()),
                    cell: None,
                    input: String::new(),
                    value: String::new(),
                    provenance: Provenance::Empty,
                    count: area.cell_count(),
                },
            };
            chips.push(chip);
        }
        if let Some(count) = trace.hidden.get(column) {
            let (x, y) = position(&trace, *column, nodes.len());
            hidden.push((*column, x, y, *count));
        }
    }
    let (width, height) = plane_size(&trace);
    let cell = book.cell(center);
    let (input, value, why, reason_is_detail) = match &cell {
        Some(cell) => {
            let shown = cells::shown(cell, profile.as_ref(), book.cell_format(center));
            (
                cells::input_text(cell.input()),
                shown.text,
                match cells::error_meaning(cell) {
                    Some(meaning) => format!("{}; {meaning}", cells::provenance_text(host, cell)),
                    None => cells::provenance_text(host, cell),
                },
                shown.provenance == Provenance::Calculated && shown.kind != "error",
            )
        }
        None => (String::new(), String::new(), "empty".to_owned(), false),
    };
    wire::Canvas {
        wires_svg: wires_svg(&trace),
        chips,
        hidden,
        width,
        height,
        inspector: wire::Inspector {
            label: cells::cell_label(host, center),
            input,
            value,
            why,
            reason_is_detail,
            format: host.number_format(center).unwrap_or("General").to_owned(),
            reads: host
                .precedents(center)
                .unwrap_or_default()
                .into_iter()
                .map(|area| cells::area_label(host, area))
                .collect(),
            read_by: host
                .dependents(center)
                .into_iter()
                .map(|cell| cells::cell_label(host, cell))
                .collect(),
        },
    }
}
