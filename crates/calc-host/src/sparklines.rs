//! A sheet's sparklines with their values now (dc-zjsg.7): each group as the file states it, and each sparkline's
//! cell and the numbers of its data range, read as a chart series reads its data (the reference rewritten through
//! the session's row and column inserts and deletes, then evaluated). A value that is not a number (text, an
//! error, a blank cell) is an empty point. Not modelled yet: a date axis's spacing (points are evenly spaced) and
//! leaving out values in hidden rows and columns.

use crate::state::HostState;
use calc_doc_model::value::{CellKey, CoreRef, SheetId};
use calc_doc_model::{ChartData, SparklineGroup};

fn sheet_of(area: calc_doc_model::value::GridArea) -> SheetId {
    area.first().sheet()
}

/// One sparkline: the cell it is drawn in and its values in order.
#[derive(Clone, Debug, PartialEq)]
pub struct SparklineView {
    pub cell: CellKey,
    pub values: Vec<Option<f64>>,
}

impl HostState {
    /// A sparkline group's colour against the workbook's theme (a theme colour with its tint as cells take it).
    pub fn sparkline_colour(&self, color: &calc_doc_model::StyleColor) -> Option<crate::LookColor> {
        crate::conditional::resolve(self, color)
    }

    /// The sparkline groups of `sheet` with their sparklines' cells and values; a sparkline whose cell does not
    /// resolve is left out.
    pub fn sparklines(&self, sheet: SheetId) -> Vec<(SparklineGroup, Vec<SparklineView>)> {
        let Ok(origin) = CellKey::new(sheet, 1, 1) else { return Vec::new() };
        self.model
            .definitions()
            .sparklines(sheet)
            .iter()
            .map(|group| {
                let views = group
                    .sparklines
                    .iter()
                    .filter_map(|line| {
                        let cell_text = self.current_reference(&line.cell, sheet);
                        let crate::ReferenceTarget::Area(area) = self.resolve_reference_text(origin, &cell_text) else {
                            return None;
                        };
                        let cell = area.first();
                        // A data area is read cell by cell, so a blank cell is an empty point (a reference's
                        // evaluation gives it as 0); anything else is evaluated as a chart series is.
                        let data_text = self.current_reference(&line.data, sheet);
                        if let crate::ReferenceTarget::Area(data) = self.resolve_reference_text(cell, &data_text)
                            && data.cell_count() <= 10_000
                        {
                            let (first, last) = (data.first(), data.last());
                            let values = (first.row()..=last.row())
                                .flat_map(|row| (first.column()..=last.column()).map(move |column| (row, column)))
                                .filter_map(|(row, column)| CellKey::new(sheet_of(data), row, column).ok())
                                .map(|at| match crate::conditional::cell_value(self, at).as_ref().map(|v| v.core_ref()) {
                                    Some(CoreRef::Number(n)) => Some(n),
                                    _ => None,
                                })
                                .collect();
                            return Some(SparklineView { cell, values });
                        }
                        let data = ChartData { formula: Some(line.data.clone()), cache: Vec::new(), numeric: true };
                        let (values, cached) = self.data_values(&data, cell);
                        let values = if cached {
                            Vec::new()
                        } else {
                            values
                                .iter()
                                .map(|v| match v.as_ref().map(|v| v.core_ref()) {
                                    Some(CoreRef::Number(n)) => Some(n),
                                    _ => None,
                                })
                                .collect()
                        };
                        Some(SparklineView { cell, values })
                    })
                    .collect();
                (group.clone(), views)
            })
            .collect()
    }
}
