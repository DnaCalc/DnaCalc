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
    /// What `cells` retains, kept running on insert and remove.
    retained: usize,
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
impl Changes {
    fn reset() -> Self {
        Self {
            reset: true,
            cells: Vec::new(),
            removed: Vec::new(),
        }
    }

    /// The complete current root is already installed in TileStream::cells. If a pending
    /// delta grows too large, that root is the bounded replacement for all its operations.
    fn merge(previous: Self, cells: Vec<Cell>, removed: Vec<Key>) -> Self {
        let mut changed: BTreeMap<_, _> = previous.cells.into_iter().map(|c| (c.at, c)).collect();
        let mut emptied: BTreeSet<_> = previous.removed.into_iter().collect();
        let mut retained = changed
            .values()
            .fold(emptied.len().saturating_mul(16), |n, c| {
                n.saturating_add(c.retained_bytes())
            });
        for at in removed {
            if let Some(old) = changed.remove(&at) {
                retained = retained.saturating_sub(old.retained_bytes());
            }
            if emptied.insert(at) {
                retained = retained.saturating_add(16);
            }
            if changed.len() + emptied.len() > MAX_PUBLICATION_RECORDS
                || retained > MAX_PUBLICATION_BYTES - MAX_TILE_BYTES
            {
                return Self::reset();
            }
        }
        for cell in cells {
            if emptied.remove(&cell.at) {
                retained = retained.saturating_sub(16);
            }
            let cost = cell.retained_bytes();
            if let Some(old) = changed.insert(cell.at, cell) {
                retained = retained.saturating_sub(old.retained_bytes());
            }
            retained = retained.saturating_add(cost);
            if changed.len() + emptied.len() > MAX_PUBLICATION_RECORDS
                || retained > MAX_PUBLICATION_BYTES - MAX_TILE_BYTES
            {
                return Self::reset();
            }
        }
        Self {
            reset: false,
            cells: changed.into_values().collect(),
            removed: emptied.into_iter().collect(),
        }
    }
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
        let incoming = cells.iter().fold(removed.len().saturating_mul(16), |n, c| {
            n.saturating_add(c.retained_bytes())
        });
        if cells.len() > MAX_TILE_CELLS
            || cells.len().saturating_add(removed.len()) > MAX_PUBLICATION_RECORDS
            || incoming > MAX_PUBLICATION_BYTES - MAX_TILE_BYTES
        {
            self.reject();
            return Err(
                "This projection delta exceeds the tile budget; narrow its visible area".into(),
            );
        }
        if reset {
            self.failed = false;
            self.cells.clear();
            self.retained = 0;
        }
        self.revision = revision;
        let mut retained = self.retained;
        for at in &removed {
            if let Some(old) = self.cells.remove(at) {
                retained = retained.saturating_sub(old.retained_bytes());
            }
        }
        self.retained = retained;
        let geometry_size = geometry
            .iter()
            .fold(0usize, |n, (_, g)| n.saturating_add(geometry_bytes(g)));
        for cell in &cells {
            let previous = self.cells.get(&cell.at).map_or(0, Cell::retained_bytes);
            let next = retained
                .saturating_sub(previous)
                .saturating_add(cell.retained_bytes());
            let count = self.cells.len() + usize::from(previous == 0);
            if count > MAX_TILE_CELLS
                || next.saturating_add(geometry_size) > MAX_PUBLICATION_BYTES - MAX_TILE_BYTES
            {
                self.reject();
                return Err(
                    "This projection exceeds the tile budget; narrow its visible area".into(),
                );
            }
            retained = next;
            self.retained = retained;
            self.cells.insert(cell.at, cell.clone());
        }
        if retained.saturating_add(geometry_size) > MAX_PUBLICATION_BYTES - MAX_TILE_BYTES {
            self.reject();
            return Err("Sheet geometry exceeds the tile budget; narrow its visible area".into());
        }
        if calculation.is_some() {
            self.calculation_through = calculation;
        }
        self.geometry = geometry;
        self.pending = Some(
            if self.in_flight.is_some() || reset || self.pending.as_ref().is_some_and(|p| p.reset) {
                Changes::reset()
            } else if let Some(previous) = self.pending.take() {
                // Several host deltas from the same synchronous command remain one publication.
                // Keep the final operation for each address, including explicit spill removals.
                Changes::merge(previous, cells, removed)
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
    pub fn reject(&mut self) {
        self.cells.clear();
        self.retained = 0;
        self.geometry.clear();
        self.pending = None;
        self.failed = true;
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
        self.in_flight = None;
        if self.failed {
            return;
        }
        self.pending = Some(Changes::reset());
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
                geometry_version: transaction,
                style_version: transaction,
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
            let mut retained = 0usize;
            let mut records = 0usize;
            let mut append = |batch: TileBatch| -> Result<(), String> {
                retained = retained.saturating_add(batch.retained_bytes());
                records = records.saturating_add(batch.cells.len() + batch.removed.len());
                if retained > MAX_PUBLICATION_BYTES || records > MAX_PUBLICATION_RECORDS {
                    return Err("The tile publication exceeds its bounded budget".into());
                }
                crate::wire::encode_tiles(&batch)?;
                batches.push(batch);
                Ok(())
            };
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
                    append(batch)?;
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
            // Every fragment and the aggregate budget are validated before any admission.
            append(batch)?;
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
