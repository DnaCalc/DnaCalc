//! Spill ranges: the cells an array formula's result fills beside its anchor, owned by the anchor.
//!
//! A dynamic-array formula whose result has more than one value spills it from its anchor over
//! the rectangle of the result's shape, unless a cell of that rectangle other than the anchor
//! holds an input, belongs to another formula's spill or array, or lies past the grid: the anchor
//! then shows `#SPILL!` and fills nothing. A legacy (Ctrl+Shift+Enter) array formula fills the
//! fixed area the file gave it, one element per cell. The rule that a range already filled keeps
//! its cells, so the formula that would cover them shows `#SPILL!`, follows OxCalc
//! `src/oxcalc-core/src/grid/machine/calc_ref_sheet.rs` (`reference_spill_extent_is_blocked`,
//! commit 1bd514ef): an unblocked spill of another anchor blocks, and so does an authored cell.
//! The spill index's blocks follow the reverse index's (`graph.rs`).

use calc_value::{
    ArrayShape, CalcArray, CalcValue, CellKey, GridArea, MAX_COLUMNS, MAX_ROWS, SheetId,
    WorksheetErrorCode,
};
use std::collections::{BTreeMap, BTreeSet};

/// A spill in place: the area it fills, its anchor first, and one value per cell of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Spill {
    area: GridArea,
    values: CalcArray,
    fixed: bool,
}

impl Spill {
    /// The cells the spill fills, its anchor the first.
    pub fn area(&self) -> GridArea {
        self.area
    }

    /// The anchor: the cell that holds the formula.
    pub fn anchor(&self) -> CellKey {
        self.area.first()
    }

    /// Whether the area is a legacy array formula's, fixed by the file, rather than a dynamic
    /// array's spill.
    pub fn is_fixed(&self) -> bool {
        self.fixed
    }

    /// The value shown at `at`, which must lie in the area.
    pub fn value_at(&self, at: CellKey) -> &CalcValue {
        let row = (at.row() - self.area.first().row()) as usize;
        let column = (at.column() - self.area.first().column()) as usize;
        self.values
            .get(row, column)
            .expect("a spill holds one value per cell of its area")
    }
}

/// What an array result does at its anchor.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SpillOutcome {
    /// The result is one value; nothing spills.
    Scalar(CalcValue),
    /// The result spills over the spill's area.
    Fills(Spill),
    /// The result would cover `extent` (the part of it on the grid), which is blocked: the anchor
    /// shows `#SPILL!`.
    Blocked { extent: GridArea },
}

impl SpillOutcome {
    /// The value the anchor shows.
    pub(crate) fn anchor_value(&self) -> CalcValue {
        match self {
            SpillOutcome::Scalar(value) => value.clone(),
            SpillOutcome::Fills(spill) => spill.value_at(spill.anchor()).clone(),
            SpillOutcome::Blocked { .. } => CalcValue::error(WorksheetErrorCode::Spill),
        }
    }

    /// The cells beside the anchor the formula would cover, whatever blocks them.
    pub(crate) fn extent(&self) -> Option<GridArea> {
        match self {
            SpillOutcome::Scalar(_) => None,
            SpillOutcome::Fills(spill) => Some(spill.area),
            SpillOutcome::Blocked { extent } => Some(*extent),
        }
    }
}

/// A value as a cell of a spill shows it: a blank element as 0, as Excel shows an array element
/// that came from a blank cell; an element that holds an array (a GROUPBY result's, dc-h9u) as
/// #VALUE!, as Excel 16.0.20527 shows it. A callable shows its worksheet core (fn2 q017–020);
/// its identity stays available inside the expression until this publication boundary.
pub(crate) fn shown(value: &CalcValue) -> CalcValue {
    if value.callable_value().is_some() {
        CalcValue::new(value.core().clone())
    } else if value.is_empty() || value.is_missing() {
        CalcValue::number(0.0)
    } else if (value.as_array().is_some() && value.callable_value().is_none())
        || value.as_reference().is_some()
    {
        // dc-b1l: an element of an array of references too (`=OFFSET(A1,{0;1;2},0)` spills
        // #VALUE! in Excel 16.0.20527).
        CalcValue::error(WorksheetErrorCode::Value)
    } else {
        value.clone()
    }
}

/// The area a result of `shape` covers from `anchor`, clipped to the grid, and whether it fits.
pub(crate) fn extent_of(anchor: CellKey, shape: ArrayShape) -> (GridArea, bool) {
    let rows = u64::try_from(shape.rows).unwrap_or(u64::MAX);
    let columns = u64::try_from(shape.cols).unwrap_or(u64::MAX);
    let bottom = u64::from(anchor.row()) + rows - 1;
    let right = u64::from(anchor.column()) + columns - 1;
    let fits = bottom <= u64::from(MAX_ROWS) && right <= u64::from(MAX_COLUMNS);
    let last = CellKey::new(
        anchor.sheet(),
        bottom.min(u64::from(MAX_ROWS)) as u32,
        right.min(u64::from(MAX_COLUMNS)) as u32,
    )
    .expect("clipped to the grid");
    (GridArea::new(anchor, last).expect("one sheet"), fits)
}

/// The spill of `array` from `anchor` over `area`, its blank elements shown as 0.
pub(crate) fn spill_of(area: GridArea, array: &CalcArray) -> Spill {
    let values = array.iter_row_major().map(shown).collect();
    Spill {
        area,
        values: CalcArray::new(array.shape(), values).expect("the array's own shape"),
        fixed: false,
    }
}

/// A complete imported array image, used only by qualified boundary-cache reads.
pub(crate) fn cached_spill(area: GridArea, values: CalcArray, fixed: bool) -> Spill {
    Spill {
        area,
        values,
        fixed,
    }
}

/// A legacy array formula's result fitted to its fixed `area`, as Excel fills one: a single value
/// fills every cell, a result of one row is repeated down the rows and one of one column across
/// the columns, and a cell past the result shows #N/A.
pub(crate) fn fill_of(area: GridArea, value: &CalcValue) -> Spill {
    let rows = area.row_count() as usize;
    let columns = area.column_count() as usize;
    let mut cells = Vec::with_capacity(rows * columns);
    for row in 0..rows {
        for column in 0..columns {
            let element = match value.as_array() {
                Some(array) if value.callable_value().is_none() => {
                    let shape = array.shape();
                    let r = if shape.rows == 1 { 0 } else { row };
                    let c = if shape.cols == 1 { 0 } else { column };
                    array.get(r, c).map(shown)
                }
                _ => Some(shown(value)),
            };
            cells.push(element.unwrap_or_else(|| CalcValue::error(WorksheetErrorCode::NA)));
        }
    }
    Spill {
        area,
        values: CalcArray::new(
            ArrayShape {
                rows,
                cols: columns,
            },
            cells,
        )
        .expect("one value per cell of the area"),
        fixed: true,
    }
}

/// The cells of `extent` other than its first, as at most two rectangles: the rest of the first
/// row, and the rows below it.
pub(crate) fn beside_anchor(extent: GridArea) -> impl Iterator<Item = GridArea> {
    let (first, last) = (extent.first(), extent.last());
    let sheet = first.sheet();
    let cell = |row, column| CellKey::new(sheet, row, column).expect("a cell of the extent");
    let row_rest = (last.column() > first.column()).then(|| {
        GridArea::new(
            cell(first.row(), first.column() + 1),
            cell(first.row(), last.column()),
        )
        .expect("one sheet")
    });
    let below = (last.row() > first.row())
        .then(|| GridArea::new(cell(first.row() + 1, first.column()), last).expect("one sheet"));
    row_rest.into_iter().chain(below)
}

const ROW_BLOCK: u32 = 256;
const COLUMN_BLOCK: u32 = 64;
const MAX_BLOCKS: u64 = 64;

fn blocks(area: GridArea) -> (std::ops::RangeInclusive<u32>, std::ops::RangeInclusive<u32>) {
    let (first, last) = (area.first(), area.last());
    (
        (first.row() - 1) / ROW_BLOCK..=(last.row() - 1) / ROW_BLOCK,
        (first.column() - 1) / COLUMN_BLOCK..=(last.column() - 1) / COLUMN_BLOCK,
    )
}

fn block_count(area: GridArea) -> u64 {
    let (rows, columns) = blocks(area);
    u64::from(rows.end() - rows.start() + 1) * u64::from(columns.end() - columns.start() + 1)
}

fn meets(one: GridArea, other: GridArea) -> bool {
    let (a, b) = (one.first(), one.last());
    let (c, d) = (other.first(), other.last());
    one.sheet() == other.sheet()
        && a.row() <= d.row()
        && c.row() <= b.row()
        && a.column() <= d.column()
        && c.column() <= b.column()
}

/// Spills by anchor, with an index that finds the spill covering a cell.
#[derive(Clone, Debug, Default)]
pub(crate) struct SpillIndex {
    by_anchor: BTreeMap<CellKey, Spill>,
    blocks: BTreeMap<(SheetId, u32, u32), BTreeSet<CellKey>>,
    wide: BTreeMap<SheetId, BTreeSet<CellKey>>,
}

impl SpillIndex {
    pub(crate) fn is_empty(&self) -> bool {
        self.by_anchor.is_empty()
    }

    pub(crate) fn get(&self, anchor: CellKey) -> Option<&Spill> {
        self.by_anchor.get(&anchor)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Spill> {
        self.by_anchor.values()
    }

    pub(crate) fn insert(&mut self, spill: Spill) {
        let anchor = spill.anchor();
        self.remove(anchor);
        let area = spill.area;
        if block_count(area) > MAX_BLOCKS {
            self.wide.entry(area.sheet()).or_default().insert(anchor);
        } else {
            let (rows, columns) = blocks(area);
            for row in rows {
                for column in columns.clone() {
                    self.blocks
                        .entry((area.sheet(), row, column))
                        .or_default()
                        .insert(anchor);
                }
            }
        }
        self.by_anchor.insert(anchor, spill);
    }

    pub(crate) fn remove(&mut self, anchor: CellKey) -> Option<Spill> {
        let spill = self.by_anchor.remove(&anchor)?;
        let area = spill.area;
        let drop_from = |set: Option<&mut BTreeSet<CellKey>>| {
            if let Some(set) = set {
                set.remove(&anchor);
                set.is_empty()
            } else {
                false
            }
        };
        if block_count(area) > MAX_BLOCKS {
            if drop_from(self.wide.get_mut(&area.sheet())) {
                self.wide.remove(&area.sheet());
            }
        } else {
            let (rows, columns) = blocks(area);
            for row in rows {
                for column in columns.clone() {
                    let key = (area.sheet(), row, column);
                    if drop_from(self.blocks.get_mut(&key)) {
                        self.blocks.remove(&key);
                    }
                }
            }
        }
        Some(spill)
    }

    /// The spill whose area holds `at`, its anchor included.
    pub(crate) fn covering(&self, at: CellKey) -> Option<&Spill> {
        if self.by_anchor.is_empty() {
            return None;
        }
        let block = (
            at.sheet(),
            (at.row() - 1) / ROW_BLOCK,
            (at.column() - 1) / COLUMN_BLOCK,
        );
        self.blocks
            .get(&block)
            .into_iter()
            .flatten()
            .chain(self.wide.get(&at.sheet()).into_iter().flatten())
            .filter_map(|anchor| self.by_anchor.get(anchor))
            .find(|spill| spill.area.contains(at))
    }

    /// The spills whose areas meet `area`, each once.
    pub(crate) fn meeting(&self, area: GridArea) -> Vec<&Spill> {
        if self.by_anchor.is_empty() {
            return Vec::new();
        }
        let mut anchors: BTreeSet<CellKey> = BTreeSet::new();
        if block_count(area) > MAX_BLOCKS {
            let sheet = area.sheet();
            let first = (sheet, 0, 0);
            let last = (sheet, u32::MAX, u32::MAX);
            for set in self.blocks.range(first..=last).map(|(_, set)| set) {
                anchors.extend(set);
            }
        } else {
            let (rows, columns) = blocks(area);
            for row in rows {
                for column in columns.clone() {
                    if let Some(set) = self.blocks.get(&(area.sheet(), row, column)) {
                        anchors.extend(set);
                    }
                }
            }
        }
        if let Some(set) = self.wide.get(&area.sheet()) {
            anchors.extend(set);
        }
        anchors
            .into_iter()
            .filter_map(|anchor| self.by_anchor.get(&anchor))
            .filter(|spill| meets(spill.area, area))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_value::IdAllocator;

    fn sheet() -> SheetId {
        let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
        sheets.issue().unwrap()
    }

    fn row(values: &[f64]) -> CalcValue {
        CalcValue::array(
            CalcArray::from_rows(vec![values.iter().map(|v| CalcValue::number(*v)).collect()])
                .unwrap(),
        )
    }

    /// A legacy array's fill: a one-row result repeats down the area, and a cell past the
    /// result's columns shows #N/A.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_legacy_fill_repeats_one_row_and_pads_with_na() {
        let s = sheet();
        let area = GridArea::new(
            CellKey::new(s, 1, 1).unwrap(),
            CellKey::new(s, 2, 3).unwrap(),
        )
        .unwrap();
        let fill = fill_of(area, &row(&[1.0, 2.0]));
        let at = |r, c| fill.value_at(CellKey::new(s, r, c).unwrap()).clone();
        assert_eq!(at(1, 1), CalcValue::number(1.0));
        assert_eq!(at(2, 2), CalcValue::number(2.0));
        assert_eq!(at(2, 3), CalcValue::error(WorksheetErrorCode::NA));
    }

    /// The index finds the spill covering a cell, and the spills meeting an area, through blocks
    /// and through the wide list.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_index_finds_covering_and_meeting_spills() {
        let s = sheet();
        let key = |r, c| CellKey::new(s, r, c).unwrap();
        let mut index = SpillIndex::default();
        let small = GridArea::new(key(2, 2), key(4, 2)).unwrap();
        let tall = GridArea::new(key(1, 10), key(100_000, 10)).unwrap();
        index.insert(spill_of(
            small,
            &row(&[1.0, 2.0, 3.0])
                .as_array()
                .map(|a| {
                    CalcArray::new(
                        ArrayShape { rows: 3, cols: 1 },
                        a.iter_row_major().cloned().collect(),
                    )
                    .unwrap()
                })
                .unwrap(),
        ));
        let column: Vec<CalcValue> = (0..100_000).map(|i| CalcValue::number(i as f64)).collect();
        index.insert(spill_of(
            tall,
            &CalcArray::new(
                ArrayShape {
                    rows: 100_000,
                    cols: 1,
                },
                column,
            )
            .unwrap(),
        ));
        assert_eq!(
            index.covering(key(3, 2)).map(Spill::anchor),
            Some(key(2, 2))
        );
        assert_eq!(index.covering(key(5, 2)), None);
        assert_eq!(
            index
                .covering(key(99_999, 10))
                .map(|s| s.value_at(key(99_999, 10)).clone()),
            Some(CalcValue::number(99_998.0))
        );
        let whole = GridArea::new(key(1, 1), key(MAX_ROWS, 20)).unwrap();
        assert_eq!(index.meeting(whole).len(), 2);
        index.remove(key(1, 10));
        assert_eq!(index.covering(key(5, 10)), None);
        assert_eq!(index.meeting(whole).len(), 1);
    }
}
