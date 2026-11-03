//! Host to neutral projection conversion.
pub use dna_calc_protocol::*;
pub fn geometry_of(geometry: &calc_host::SheetGeometry) -> Geometry {
    Geometry {
        coverage: Vec::new(),
        auto_filter: None,
        merges: Vec::new(),
        edit_refusal: None,
        zoom: None,
        top_left: None,
        pane_top_left: None,
        active_cell: None,
        active_pane: 0,
        split: None,
        frozen_rows: 0,
        frozen_columns: 0,
        split_window: false,
        default_row_height: geometry.default_row_height,
        default_column_width: geometry.default_column_width,
        base_column_width: geometry.base_column_width,
        columns: geometry
            .columns
            .iter()
            .map(|span| (span.first, span.last, span.width, span.hidden))
            .collect(),
        rows: geometry
            .rows
            .iter()
            .map(|(&row, format)| (row, format.height, format.hidden))
            .collect(),
    }
}

pub fn host_paste_mode(mode: PasteMode) -> calc_host::PasteMode {
    match mode {
        PasteMode::All => calc_host::PasteMode::All,
        PasteMode::Values => calc_host::PasteMode::Values,
        PasteMode::Formulas => calc_host::PasteMode::Formulas,
        PasteMode::Formats => calc_host::PasteMode::Formats,
    }
}

pub fn cell_of(
    shown: &CellShown,
    input: String,
    why: String,
    projected: &calc_host::ProjectedCell,
) -> Cell {
    Cell {
        style: shown.style.clone(),
        at: Key::of(shown.at),
        text: shown.text.clone(),
        kind: shown.kind.to_owned(),
        provenance: shown.provenance,
        formula: shown.formula,
        format: shown.format,
        color: shown.color.clone(),
        conditional: shown.conditional.clone(),
        input: if shown.formula {
            dna_formula_ui::display::formula(&input)
        } else {
            input
        },
        why,
        owner: crate::cells::refusal_owner(projected),
        error_meaning: crate::cells::error_meaning(projected),
    }
}
