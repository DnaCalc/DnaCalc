//! Workbook outline projection.
use calc_value::{CellKey, SheetId};
pub use dna_calc_model::tree::*;
use std::collections::BTreeSet;
/// Authored keys and a bounded window of published spill children. This reads no cached values.
pub fn outline_keys(
    host: &calc_host::HostState,
    sheet: SheetId,
    rows: usize,
    reveal: Option<CellKey>,
) -> (BTreeSet<CellKey>, u64) {
    let mut keys: BTreeSet<_> = host.input_cells(sheet).into_iter().collect();
    let mut omitted = 0;
    let mut seen = Vec::new();
    for area in host
        .occupied_areas(sheet)
        .into_iter()
        .filter(|a| a.first() != a.last())
    {
        if seen.contains(&area) {
            continue;
        }
        seen.push(area);
        let bottom = area.last().row().min(
            area.first()
                .row()
                .saturating_add(rows.clamp(1, 500) as u32 - 1),
        );
        let right = area
            .last()
            .column()
            .min(area.first().column() + ROW_CELLS as u32 - 1);
        for row in area.first().row()..=bottom {
            for col in area.first().column()..=right {
                keys.insert(CellKey::new(sheet, row, col).unwrap());
            }
        }
        omitted += u64::from(area.last().row() - area.first().row() + 1)
            * u64::from(area.last().column() - area.first().column() + 1)
            - u64::from(bottom - area.first().row() + 1)
                * u64::from(right - area.first().column() + 1);
        if let Some(at) = reveal.filter(|at| area.contains(*at)) {
            if keys.insert(at) {
                omitted = omitted.saturating_sub(1);
            }
        }
    }
    if let Some(at) = reveal.filter(|at| at.sheet() == sheet) {
        keys.insert(at);
    }
    (keys, omitted)
}
