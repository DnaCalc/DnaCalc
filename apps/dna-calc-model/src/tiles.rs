//! The frozen grid's cell and geometry atoms over the shared bounded tile records.
use crate::projection::{Cell, Geometry, Key};
pub use calc_skin::tiles::{
    CELL_BYTES, MAX_PENDING_REQUESTS, MAX_PUBLICATION_BYTES, MAX_PUBLICATION_RECORDS,
    MAX_SUBSCRIPTIONS, MAX_TILE_BYTES, MAX_TILE_CELLS, MAX_TILE_RECORDS, TileBarrier, TileStamp,
};

pub type TileBatch = calc_skin::tiles::TileBatch<Cell, Geometry>;

impl calc_skin::tiles::TileCell for Cell {
    fn key(&self) -> Key {
        self.at
    }

    fn set_key(&mut self, at: Key) {
        self.at = at;
    }

    fn retained_bytes(&self) -> usize {
        Cell::retained_bytes(self)
    }
}

impl calc_skin::tiles::TileGeometry for Geometry {
    fn retained_bytes(&self) -> usize {
        geometry_bytes(self)
    }
}

impl Cell {
    /// What the cell retains: its inline size plus the bytes of its strings. The wire form is
    /// bounded separately by the length of the encoded fragment, so no escaping factor is charged.
    pub fn retained_bytes(&self) -> usize {
        let mut strings = vec![
            self.text.as_str(),
            self.kind.as_str(),
            self.input.as_str(),
            self.why.as_str(),
        ];
        strings.extend(
            [
                self.color.as_deref(),
                self.owner.as_deref(),
                self.error_meaning.as_deref(),
                self.style.family.as_deref(),
                self.style.horizontal.as_deref(),
                self.style.vertical.as_deref(),
                self.style.unsupported.as_deref(),
            ]
            .into_iter()
            .flatten(),
        );
        if let Some(c) = &self.conditional {
            strings.extend(
                [
                    c.fill.as_deref(),
                    c.font_color.as_deref(),
                    c.bar.as_ref().map(|b| b.1.as_str()),
                    c.icon.as_ref().map(|i| i.0.as_str()),
                ]
                .into_iter()
                .flatten(),
            );
        }
        let mut brush_bytes = 0usize;
        if let Some(brush) = &self.style.brush {
            match brush {
                crate::projection::FillShown::Pattern {
                    name,
                    foreground,
                    background,
                } => strings.extend([name.as_str(), foreground.as_str(), background.as_str()]),
                crate::projection::FillShown::Gradient { kind, stops, .. } => {
                    strings.push(kind.as_str());
                    strings.extend(stops.iter().map(|(_, color)| color.as_str()));
                    brush_bytes = stops.len().saturating_mul(64);
                }
            }
        }
        for edge in self
            .style
            .borders
            .iter()
            .chain(self.style.diagonal.iter())
            .flatten()
        {
            strings.push(edge.style.as_str());
            strings.extend(edge.color.as_deref());
        }
        strings
            .into_iter()
            .fold(CELL_BYTES.saturating_add(brush_bytes), |n, s| {
                n.saturating_add(s.len())
            })
    }
}
pub fn geometry_bytes(g: &Geometry) -> usize {
    1024usize
        .saturating_add(g.coverage.len().saturating_mul(64))
        .saturating_add(
            g.rows
                .len()
                .saturating_add(g.columns.len())
                .saturating_mul(128),
        )
        .saturating_add(g.merges.len().saturating_mul(128))
        .saturating_add(
            g.edit_refusal
                .as_ref()
                .map_or(0, |s| s.len().saturating_mul(6)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_per_cell_charge_covers_the_inline_cell() {
        assert!(
            std::mem::size_of::<Cell>() <= CELL_BYTES,
            "Cell is now {} bytes",
            std::mem::size_of::<Cell>()
        );
    }
}
