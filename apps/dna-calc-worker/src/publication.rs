//! Bounded per-subscription delivery. One publication can be in flight; further host deltas
//! coalesce into a reset of the latest projection. Acknowledgements never calculate anything.
use crate::wire::{Cell, Event, Geometry, Key, Sheet};
use dna_calc_model::tiles::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(crate) struct TileStream {
    pub revision: u64,
    calculation_through: Option<crate::wire::Publication>,
    cells: BTreeMap<Key, Cell>,
    geometry: Vec<(Sheet, Geometry)>,
    pending: Option<Changes>,
    in_flight: Option<TileBarrier>,
    sequence: u64,
    failed: bool,
}
struct Changes {
    reset: bool,
    cells: Vec<Cell>,
    removed: Vec<Key>,
}
impl TileStream {
    pub fn update(
        &mut self,
        reset: bool,
        cells: Vec<Cell>,
        removed: Vec<Key>,
        revision: u64,
        geometry: Vec<(Sheet, Geometry)>,
        calculation: Option<crate::wire::Publication>,
    ) -> Result<(), String> {
        if self.failed && !reset {
            return Err("This projection needs a narrower subscription".into());
        }
        if reset {
            self.failed = false;
            self.cells.clear();
        }
        for at in &removed {
            self.cells.remove(at);
        }
        for cell in &cells {
            self.cells.insert(cell.at, cell.clone());
        }
        if calculation.is_some() {
            self.calculation_through = calculation;
        }
        self.geometry = geometry;
        self.revision = revision;
        let bytes = self
            .cells
            .values()
            .fold(0usize, |n, c| n.saturating_add(c.retained_bytes()))
            .saturating_add(
                self.geometry
                    .iter()
                    .map(|(_, g)| geometry_bytes(g))
                    .sum::<usize>(),
            );
        if self.cells.len() > MAX_TILE_CELLS || bytes > MAX_PUBLICATION_BYTES - MAX_TILE_BYTES {
            self.cells.clear();
            self.geometry.clear();
            self.pending = None;
            self.failed = true;
            return Err("This projection exceeds the tile budget; narrow its visible area".into());
        }
        self.pending = Some(
            if self.in_flight.is_some() || reset || self.pending.as_ref().is_some_and(|p| p.reset) {
                Changes {
                    reset: true,
                    cells: Vec::new(),
                    removed: Vec::new(),
                }
            } else if let Some(previous) = self.pending.take() {
                // Several host deltas from the same synchronous command remain one publication.
                // Keep the final operation for each address, including explicit spill removals.
                let mut changed: BTreeMap<_, _> =
                    previous.cells.into_iter().map(|c| (c.at, c)).collect();
                let mut emptied: BTreeSet<_> = previous.removed.into_iter().collect();
                for at in removed {
                    changed.remove(&at);
                    emptied.insert(at);
                }
                for c in cells {
                    emptied.remove(&c.at);
                    changed.insert(c.at, c);
                }
                Changes {
                    reset: false,
                    cells: changed.into_values().collect(),
                    removed: emptied.into_iter().collect(),
                }
            } else {
                Changes {
                    reset,
                    cells,
                    removed,
                }
            },
        );
        Ok(())
    }
    pub fn failed(&self) -> bool {
        self.failed
    }
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn acknowledge(&mut self, barrier: TileBarrier) {
        if self.in_flight == Some(barrier) {
            self.in_flight = None;
        }
    }
    pub fn reset(&mut self) {
        if self.failed {
            return;
        }
        self.in_flight = None;
        self.pending = Some(Changes {
            reset: true,
            cells: Vec::new(),
            removed: Vec::new(),
        });
    }
    pub fn publish(
        &mut self,
        id: u64,
        workbook: u64,
        transaction: u64,
    ) -> Result<Vec<Event>, String> {
        let result = (|| {
            if self.in_flight.is_some() || self.pending.is_none() {
                return Ok(Vec::new());
            }
            let changes = self.pending.take().unwrap();
            let cells = if changes.reset {
                self.cells.values().cloned().collect()
            } else {
                changes.cells
            };
            let stamp = TileStamp {
                calculation_through: self.calculation_through,
                id,
                workbook,
                transaction,
                base: self.sequence,
                next: self
                    .sequence
                    .checked_add(1)
                    .ok_or("Tile sequence exhausted")?,
                revision: self.revision,
                geometry_version: self.revision,
                style_version: self.revision,
                dictionary: transaction,
                reset: changes.reset,
            };
            let mut batch = TileBatch {
                stamp,
                fragment: 0,
                final_fragment: false,
                geometry: self.geometry.clone(),
                cells: Vec::new(),
                removed: Vec::new(),
            };
            let mut batches = Vec::new();
            for (cell, removed) in cells
                .into_iter()
                .map(|c| (Some(c), None))
                .chain(changes.removed.into_iter().map(|at| (None, Some(at))))
            {
                let cost = cell.as_ref().map_or(16, Cell::retained_bytes);
                if batch.cells.len() + batch.removed.len() >= MAX_TILE_RECORDS
                    || batch.retained_bytes().saturating_add(cost) > MAX_TILE_BYTES
                {
                    if batch.cells.is_empty()
                        && batch.removed.is_empty()
                        && batch.geometry.is_empty()
                    {
                        return Err("A cell exceeds the tile fragment budget".into());
                    }
                    let next_fragment = batch
                        .fragment
                        .checked_add(1)
                        .ok_or("Tile fragment count exhausted")?;
                    batches.push(batch);
                    batch = TileBatch {
                        stamp,
                        fragment: next_fragment,
                        final_fragment: false,
                        geometry: Vec::new(),
                        cells: Vec::new(),
                        removed: Vec::new(),
                    };
                }
                if let Some(cell) = cell {
                    batch.cells.push(cell);
                }
                if let Some(at) = removed {
                    batch.removed.push(at);
                }
                if batch.retained_bytes() > MAX_TILE_BYTES {
                    return Err("A cell exceeds the tile fragment budget".into());
                }
            }
            if batch.retained_bytes() > MAX_TILE_BYTES {
                return Err("Sheet geometry exceeds the tile fragment budget".into());
            }
            batch.final_fragment = true;
            batches.push(batch);
            // Validate every fragment before admitting any part of the transaction.
            for batch in &batches {
                crate::wire::encode_tiles(batch)?;
            }
            let mut events = vec![Event::TileAdmission(stamp)];
            events.extend(batches.into_iter().map(Event::Tiles));
            events.push(Event::TileCompletion(stamp.barrier()));
            self.sequence = stamp.next;
            self.in_flight = Some(stamp.barrier());
            Ok(events)
        })();
        if result.is_err() {
            self.failed = true;
            self.pending = None;
        }
        result
    }
}
