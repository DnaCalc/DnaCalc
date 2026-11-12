//! Host to neutral projection conversion.
pub use dna_calc_protocol::*;
pub fn geometry_of(geometry: &calc_host::SheetGeometry) -> Geometry {
    Geometry {
        outline: Vec::new(),
        summary_below: geometry.summary_below.unwrap_or(true),
        summary_right: geometry.summary_right.unwrap_or(true),
        coverage: Vec::new(),
        auto_filter: None,
        tables: Vec::new(),
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
        exact: exact_number(&shown.text, projected),
    }
}

/// The exact value of a number the shown text rounds (see [`Cell::exact`]).
fn exact_number(text: &str, projected: &calc_host::ProjectedCell) -> Option<String> {
    use calc_host::ProjectedValue;
    let value = match projected.value() {
        ProjectedValue::Literal(value) | ProjectedValue::Calculated { value, .. } => value.clone(),
        ProjectedValue::FileCached(cache) => cache.value().to_value(),
        _ => return None,
    };
    let mut value = value;
    while let calc_value::CoreRef::Array(array) = value.core_ref() {
        value = array.get(0, 0)?.clone();
    }
    let number = value.as_number().filter(|n| n.is_finite())?;
    if text.trim().parse::<f64>().ok() == Some(number) {
        return None;
    }
    Some(excel_e_notation(number))
}

/// `2.43290200817664E+18`, `-1E-05`, `0.5`: the shortest digits that read back as `number`,
/// with an exponent only where Excel's General would use one.
pub fn excel_e_notation(number: f64) -> String {
    let plain = format!("{number}");
    let magnitude = number.abs();
    if number == 0.0 || (1e-5..1e15).contains(&magnitude) {
        return plain;
    }
    let scientific = format!("{number:e}");
    match scientific.split_once('e') {
        Some((mantissa, exponent)) => match exponent.strip_prefix('-') {
            Some(negative) => format!("{mantissa}E-{negative:0>2}"),
            None => format!("{mantissa}E+{exponent:0>2}"),
        },
        None => scientific,
    }
}

#[cfg(test)]
mod exact_tests {
    /// The exact value's spelling reads back as the same double, with Excel's E notation.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn exact_values_read_back_as_the_same_double() {
        for (number, text) in [
            (2.43290200817664e18, "2.43290200817664E+18"),
            (-1e-7, "-1E-07"),
            (0.1 + 0.2, "0.30000000000000004"),
            (3.141592653589793, "3.141592653589793"),
            (0.0, "0"),
        ] {
            let spelled = super::excel_e_notation(number);
            assert_eq!(spelled, text);
            assert_eq!(spelled.parse::<f64>().unwrap(), number);
        }
    }
}
