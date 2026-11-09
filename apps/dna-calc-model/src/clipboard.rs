//! Shared clipboard output encoding.
pub fn encode_tsv(rows: &[Vec<String>]) -> String {
    rows.iter()
        .map(|row| {
            row.iter()
                .map(|text| {
                    if text.contains(['\t', '\r', '\n', '"']) {
                        format!("\"{}\"", text.replace('"', "\"\""))
                    } else {
                        text.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect::<Vec<_>>()
        .join("\r\n")
}

pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn encode_html(rows: &[Vec<String>], token: u64, session: &str) -> String {
    format!(
        "<table data-dnacalc-clipboard=\"{token}\" data-dnacalc-session=\"{}\">{}</table>",
        escape(session),
        rows.iter()
            .map(|row| format!(
                "<tr>{}</tr>",
                row.iter()
                    .map(|cell| format!("<td>{}</td>", escape(cell).replace('\n', "<br>")))
                    .collect::<String>()
            ))
            .collect::<String>()
    )
}

/// A selected rectangle follows the cell where the pointer first gripped it. An off-grid
/// footprint has no destination; the adapter leaves the workbook unchanged on release.
pub fn drag_destination(
    from: calc_value::GridArea,
    grip: (u32, u32),
    row: u32,
    column: u32,
) -> Option<calc_value::GridArea> {
    if !from.contains(calc_value::CellKey::new(from.sheet(), grip.0, grip.1).ok()?) {
        return None;
    }
    let row = row.checked_sub(grip.0.checked_sub(from.first().row())?)?;
    let column = column.checked_sub(grip.1.checked_sub(from.first().column())?)?;
    let first = calc_value::CellKey::new(from.sheet(), row, column).ok()?;
    let last = calc_value::CellKey::new(
        from.sheet(),
        row.checked_add(from.row_count() - 1)?,
        column.checked_add(from.column_count() - 1)?,
    )
    .ok()?;
    calc_value::GridArea::new(first, last).ok()
}

#[cfg(test)]
mod drag_tests {
    use super::*;
    use calc_value::{CellKey, GridArea, SheetId};
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_drag_preserves_its_grip_and_never_returns_a_partial_off_grid_rectangle() {
        let sheet = SheetId::from_parts(0, 1);
        let cell = |r, c| CellKey::new(sheet, r, c).unwrap();
        let from = GridArea::new(cell(2, 2), cell(4, 4)).unwrap();
        assert_eq!(
            drag_destination(from, (3, 3), 6, 6),
            Some(GridArea::new(cell(5, 5), cell(7, 7)).unwrap())
        );
        assert_eq!(drag_destination(from, (3, 3), 1, 1), None);
        assert_eq!(drag_destination(from, (3, 3), 1_048_576, 16_384), None);
        assert_eq!(drag_destination(from, (5, 5), 8, 8), None);
        assert_eq!(
            drag_destination(GridArea::cell(cell(1, 1)), (1, 1), 1_048_576, 16_384),
            Some(GridArea::cell(cell(1_048_576, 16_384)))
        );
    }
}
