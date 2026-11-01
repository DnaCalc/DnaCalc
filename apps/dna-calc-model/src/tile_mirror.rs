//! A committed tile mirror with one bounded, invisible staging transaction.
use crate::{
    projection::{Cell, Geometry},
    tiles::*,
};
use calc_value::{CellKey, SheetId};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileApply {
    Staged,
    Applied(TileBarrier),
    Ignored,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileMirrorError {
    Admission,
    Continuity,
    Fragment,
    Identity,
    Version,
    Limit,
    Barrier,
    Address,
}

#[derive(Clone, Debug, PartialEq)]
struct Staging {
    stamp: TileStamp,
    fragment: u32,
    closed: bool,
    cells: BTreeMap<CellKey, Cell>,
    geometry: BTreeMap<SheetId, Geometry>,
    bytes: usize,
    records: usize,
}

/// Only completed publications can be read by a RenderPlan or accessibility projection.
/// A rejected fragment never changes the committed cells. Recovery requires an explicit reset.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewMirror {
    stamp: Option<TileStamp>,
    staging: Option<Staging>,
    cells: BTreeMap<CellKey, Cell>,
    geometry: BTreeMap<SheetId, Geometry>,
    failed: bool,
}
impl ViewMirror {
    pub fn stamp(&self) -> Option<TileStamp> {
        self.stamp
    }
    pub fn needs_reset(&self) -> bool {
        self.failed
    }
    pub fn cell(&self, at: CellKey) -> Option<&Cell> {
        self.cells.get(&at)
    }
    pub fn cells(&self) -> impl Iterator<Item = (&CellKey, &Cell)> {
        self.cells.iter()
    }
    pub fn geometry(&self, sheet: SheetId) -> Option<&Geometry> {
        self.geometry.get(&sheet)
    }
    pub fn invalidate(&mut self) {
        self.failed = true;
        self.staging = None;
    }

    pub fn admit(&mut self, stamp: TileStamp) -> Result<TileApply, TileMirrorError> {
        // Old controls, including old resets, cannot roll back an applied root.
        if self
            .stamp
            .is_some_and(|s| s.workbook == stamp.workbook && stamp.next <= s.next)
        {
            return Ok(TileApply::Ignored);
        }
        let result = self.admit_inner(stamp);
        if result.is_err() {
            self.invalidate();
        }
        result
    }
    fn admit_inner(&mut self, stamp: TileStamp) -> Result<TileApply, TileMirrorError> {
        if stamp.next == 0 || stamp.base.checked_add(1) != Some(stamp.next) {
            return Err(TileMirrorError::Continuity);
        }
        if stamp.geometry_version != stamp.transaction
            || stamp.style_version != stamp.transaction
            || stamp.dictionary != stamp.transaction
        {
            return Err(TileMirrorError::Version);
        }
        if let Some(current) = self.stamp {
            if stamp.id != current.id
                || stamp.workbook != current.workbook
                || stamp.transaction <= current.transaction
            {
                return Err(TileMirrorError::Identity);
            }
            if stamp.revision < current.revision {
                return Err(TileMirrorError::Version);
            }
            if !stamp.reset && (self.failed || stamp.base != current.next) {
                return Err(TileMirrorError::Continuity);
            }
        } else if !stamp.reset {
            return Err(TileMirrorError::Continuity);
        }
        if let Some(pending) = &self.staging {
            if stamp.id != pending.stamp.id || stamp.workbook != pending.stamp.workbook {
                return Err(TileMirrorError::Identity);
            }
            if pending.stamp == stamp {
                return Ok(TileApply::Ignored);
            }
            // Only a newer explicit reset can retire a broken or abandoned transaction.
            if !stamp.reset || stamp.next <= pending.stamp.next {
                return Err(TileMirrorError::Admission);
            }
        }
        self.staging = Some(Staging {
            stamp,
            fragment: 0,
            closed: false,
            cells: if stamp.reset {
                BTreeMap::new()
            } else {
                self.cells.clone()
            },
            geometry: if stamp.reset {
                BTreeMap::new()
            } else {
                self.geometry.clone()
            },
            bytes: 0,
            records: 0,
        });
        Ok(TileApply::Staged)
    }
    pub fn stage(&mut self, batch: &TileBatch) -> Result<TileApply, TileMirrorError> {
        if self
            .stamp
            .is_some_and(|s| s.workbook == batch.stamp.workbook && batch.stamp.next <= s.next)
        {
            return Ok(TileApply::Ignored);
        }
        let result = self.stage_inner(batch);
        if result.is_err() {
            self.invalidate();
        }
        result
    }
    fn stage_inner(&mut self, batch: &TileBatch) -> Result<TileApply, TileMirrorError> {
        let pending = self.staging.as_mut().ok_or(TileMirrorError::Admission)?;
        if batch.stamp != pending.stamp {
            return Err(TileMirrorError::Identity);
        }
        if pending.closed || batch.fragment != pending.fragment {
            return Err(TileMirrorError::Fragment);
        }
        if batch.retained_bytes() > MAX_TILE_BYTES
            || batch.cells.len() + batch.removed.len() > MAX_TILE_RECORDS
            || (batch.fragment > 0 && !batch.geometry.is_empty())
        {
            return Err(TileMirrorError::Limit);
        }
        pending.bytes = pending.bytes.saturating_add(batch.retained_bytes());
        pending.records = pending
            .records
            .saturating_add(batch.cells.len() + batch.removed.len());
        if pending.bytes > MAX_PUBLICATION_BYTES || pending.records > MAX_PUBLICATION_RECORDS {
            return Err(TileMirrorError::Limit);
        }
        for (sheet, geometry) in &batch.geometry {
            pending.geometry.insert(sheet.id(), geometry.clone());
        }
        for at in &batch.removed {
            pending
                .cells
                .remove(&at.cell().ok_or(TileMirrorError::Address)?);
        }
        for cell in &batch.cells {
            let at = cell.at.cell().ok_or(TileMirrorError::Address)?;
            if !pending.geometry.contains_key(&at.sheet()) {
                return Err(TileMirrorError::Version);
            }
            pending.cells.insert(at, cell.clone());
        }
        let retained = pending
            .cells
            .values()
            .fold(0usize, |n, c| n.saturating_add(c.retained_bytes()))
            .saturating_add(pending.geometry.values().map(geometry_bytes).sum::<usize>());
        if retained > MAX_PUBLICATION_BYTES {
            return Err(TileMirrorError::Limit);
        }
        if pending.cells.len() > MAX_TILE_CELLS {
            return Err(TileMirrorError::Limit);
        }
        pending.fragment = pending
            .fragment
            .checked_add(1)
            .ok_or(TileMirrorError::Limit)?;
        pending.closed = batch.final_fragment;
        Ok(TileApply::Staged)
    }
    pub fn complete(&mut self, barrier: TileBarrier) -> Result<TileApply, TileMirrorError> {
        if self.stamp.is_some_and(|s| {
            s.id == barrier.id && s.workbook == barrier.workbook && barrier.through <= s.next
        }) {
            return Ok(TileApply::Ignored);
        }
        if !self
            .staging
            .as_ref()
            .is_some_and(|s| s.closed && s.stamp.barrier() == barrier)
        {
            self.invalidate();
            return Err(TileMirrorError::Barrier);
        }
        let staged = self.staging.take().expect("checked above");
        self.cells = staged.cells;
        self.geometry = staged.geometry;
        self.stamp = Some(staged.stamp);
        self.failed = false;
        Ok(TileApply::Applied(barrier))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::{FormatShown, Key, Provenance, Sheet};
    fn cell(row: u32, text: &str) -> Cell {
        Cell {
            at: Key(0, 0, row, 1),
            text: text.into(),
            kind: "number".into(),
            provenance: Provenance::Calculated,
            formula: row == 1,
            format: FormatShown::General,
            style: Default::default(),
            color: None,
            conditional: None,
            input: if row == 1 {
                "=SEQUENCE(2)".into()
            } else {
                String::new()
            },
            why: "calculated".into(),
            owner: None,
            error_meaning: None,
        }
    }
    fn batch(next: u64, reset: bool, cells: Vec<Cell>, removed: Vec<Key>) -> TileBatch {
        TileBatch {
            stamp: TileStamp {
                calculation_through: None,
                id: 1,
                workbook: 7,
                transaction: next,
                base: next - 1,
                next,
                revision: next,
                geometry_version: next,
                style_version: next,
                dictionary: next,
                reset,
            },
            fragment: 0,
            final_fragment: true,
            cells,
            removed,
            geometry: vec![(Sheet(0, 0), Geometry::default())],
        }
    }
    fn apply(mirror: &mut ViewMirror, batch: &TileBatch) {
        mirror.admit(batch.stamp).unwrap();
        mirror.stage(batch).unwrap();
        assert_eq!(
            mirror.complete(batch.stamp.barrier()),
            Ok(TileApply::Applied(batch.stamp.barrier()))
        );
    }
    fn texts(m: &ViewMirror) -> Vec<&str> {
        m.cells().map(|(_, c)| c.text.as_str()).collect()
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn spill_growth_shrink_and_geometry_commit_only_after_the_completion_barrier() {
        let mut mirror = ViewMirror::default();
        apply(
            &mut mirror,
            &batch(1, true, vec![cell(1, "1"), cell(2, "2")], vec![]),
        );
        let mut first = batch(2, false, vec![cell(1, "10")], vec![Key(0, 0, 2, 1)]);
        first.final_fragment = false;
        first.geometry[0].1.default_row_height = Some(30.);
        mirror.admit(first.stamp).unwrap();
        mirror.stage(&first).unwrap();
        assert_eq!(texts(&mirror), ["1", "2"]);
        assert_eq!(
            mirror
                .geometry(Sheet(0, 0).id())
                .unwrap()
                .default_row_height,
            None
        );
        let mut last = first.clone();
        last.fragment = 1;
        last.final_fragment = true;
        last.geometry.clear();
        last.removed.clear();
        last.cells = vec![cell(3, "30")];
        mirror.stage(&last).unwrap();
        assert_eq!(texts(&mirror), ["1", "2"]); // even final_fragment alone is not completion
        mirror.complete(last.stamp.barrier()).unwrap();
        assert_eq!(texts(&mirror), ["10", "30"]);
        assert_eq!(
            mirror
                .geometry(Sheet(0, 0).id())
                .unwrap()
                .default_row_height,
            Some(30.)
        );
        assert_eq!(mirror.stamp(), Some(last.stamp));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn missing_or_reordered_fragments_keep_the_committed_root_and_need_an_explicit_reset() {
        let mut m = ViewMirror::default();
        apply(&mut m, &batch(1, true, vec![cell(1, "old")], vec![]));
        let mut broken = batch(2, false, vec![cell(1, "partial")], vec![]);
        m.admit(broken.stamp).unwrap();
        broken.fragment = 1;
        assert_eq!(m.stage(&broken), Err(TileMirrorError::Fragment));
        assert_eq!(texts(&m), ["old"]);
        assert!(m.needs_reset());
        assert_eq!(
            m.admit(batch(3, false, vec![], vec![]).stamp),
            Err(TileMirrorError::Continuity)
        );
        apply(&mut m, &batch(4, true, vec![cell(1, "latest")], vec![]));
        assert_eq!(texts(&m), ["latest"]);
        assert!(!m.needs_reset());
        let old = batch(1, true, vec![cell(1, "old")], vec![]);
        assert_eq!(m.admit(old.stamp), Ok(TileApply::Ignored));
        assert_eq!(m.stage(&old), Ok(TileApply::Ignored));
        assert_eq!(m.complete(old.stamp.barrier()), Ok(TileApply::Ignored));
        assert_eq!(texts(&m), ["latest"]);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn admission_identity_versions_and_closure_are_required_even_for_equal_values() {
        let original = batch(1, true, vec![cell(1, "1")], vec![]);
        let mut m = ViewMirror::default();
        assert_eq!(m.stage(&original), Err(TileMirrorError::Admission));
        apply(&mut m, &original);
        let mut next = batch(2, false, vec![cell(1, "1")], vec![]);
        next.stamp.geometry_version = 1;
        assert_eq!(m.admit(next.stamp), Err(TileMirrorError::Version));
        next = batch(3, true, vec![cell(1, "1")], vec![]);
        next.stamp.workbook = 8;
        assert_eq!(m.admit(next.stamp), Err(TileMirrorError::Identity));
        let mut next = batch(4, true, vec![cell(1, "1")], vec![]);
        next.final_fragment = false;
        m.admit(next.stamp).unwrap();
        m.stage(&next).unwrap();
        assert_eq!(
            m.complete(next.stamp.barrier()),
            Err(TileMirrorError::Barrier)
        );
        assert_eq!(m.stamp(), Some(original.stamp));
        assert_eq!(texts(&m), ["1"]);
        let next = batch(5, true, vec![cell(1, "1")], vec![]);
        apply(&mut m, &next);
        assert_eq!(texts(&m), ["1"]);
        assert_eq!(m.stamp().unwrap().revision, 5);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn malformed_addresses_missing_geometry_and_excess_records_never_install() {
        let mut address = batch(1, true, vec![cell(1, "x")], vec![]);
        address.cells[0].at.2 = 0;
        let records = batch(1, true, vec![cell(1, "x"); MAX_TILE_RECORDS + 1], vec![]);
        let mut bytes = batch(1, true, vec![cell(1, "x")], vec![]);
        bytes.cells[0].text = "x".repeat(MAX_TILE_BYTES);
        for (b, error) in [
            (address, TileMirrorError::Address),
            (records, TileMirrorError::Limit),
            (bytes, TileMirrorError::Limit),
        ] {
            let mut m = ViewMirror::default();
            m.admit(b.stamp).unwrap();
            assert_eq!(m.stage(&b), Err(error));
            assert!(m.cells().next().is_none());
        }
        let mut b = batch(1, true, vec![cell(1, "x")], vec![]);
        b.geometry.clear();
        let mut m = ViewMirror::default();
        m.admit(b.stamp).unwrap();
        assert_eq!(m.stage(&b), Err(TileMirrorError::Version));
    }
}
