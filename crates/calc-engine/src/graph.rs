//! What a formula node reads, and the reverse indexes that find the nodes reading a changed cell.
//!
//! Every edge belongs to one workbook-wide graph: a [`Prerequisite`] names its sheet by
//! [`SheetId`], so a cell of one sheet and the cell at the same row and column of another are
//! different inputs, and a node may read any sheet of the workbook.

use calc_doc_model::{Axis, ModelRoot, SheetEntry};
use calc_value::{CellKey, GridArea, GridError, NodeId, SheetId};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{BuildHasherDefault, Hasher};

/// Whole rows or whole columns of one sheet: the positions `first` to `last`, both included.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AxisInterval {
    sheet: SheetId,
    axis: Axis,
    first: u32,
    last: u32,
}

impl AxisInterval {
    /// The positions between `one` and `other` (in either order, both included) on `axis` of
    /// `sheet`; the typed error when either is 0 or past the axis (the row or column checked
    /// first is the smaller).
    pub fn new(sheet: SheetId, axis: Axis, one: u32, other: u32) -> Result<Self, GridError> {
        let (first, last) = (one.min(other), one.max(other));
        let out_of_range = match axis {
            Axis::Rows => GridError::RowOutOfRange,
            Axis::Columns => GridError::ColumnOutOfRange,
        };
        if first == 0 {
            return Err(out_of_range(first));
        }
        if last > axis.size() {
            return Err(out_of_range(last));
        }
        Ok(AxisInterval {
            sheet,
            axis,
            first,
            last,
        })
    }

    pub const fn sheet(self) -> SheetId {
        self.sheet
    }

    pub const fn axis(self) -> Axis {
        self.axis
    }

    /// The first position, 1-based.
    pub const fn first(self) -> u32 {
        self.first
    }

    /// The last position, 1-based.
    pub const fn last(self) -> u32 {
        self.last
    }
}

/// A three-dimensional reference such as `Jan:Mar!B2:C4`: the same rows and columns on every
/// sheet from one endpoint to the other in the workbook's current sheet order. It is one edge.
/// Which sheets it covers is read from the sheet order of the input snapshot each time it is
/// asked, so moving a sheet in or out of the span needs no change to the edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SheetSpan {
    target: GridArea,
    last: SheetId,
}

impl SheetSpan {
    /// The span of `target`'s rows and columns from `target`'s sheet to `last`.
    pub const fn new(target: GridArea, last: SheetId) -> Self {
        SheetSpan { target, last }
    }

    /// The endpoint the reference names first; `target` lies on it.
    pub const fn first(self) -> SheetId {
        self.target.sheet()
    }

    /// The endpoint the reference names last.
    pub const fn last(self) -> SheetId {
        self.last
    }

    /// The rows and columns, as an area of the first endpoint.
    pub const fn target(self) -> GridArea {
        self.target
    }

    /// The sheets the span covers in the snapshot's current order, from the earlier endpoint to
    /// the later one, both included, borrowed from the snapshot; `None` when the snapshot does
    /// not hold an endpoint.
    pub fn sheets(self, inputs: &ModelRoot) -> Option<&[SheetEntry]> {
        span_members(inputs, self.first(), self.last)
    }
}

/// What a formula node reads, as CalcFml's binding of the node's plan is to state it; the graph
/// keeps each as one edge, never expanded into one edge per cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Prerequisite {
    /// The value of one cell. Its binding from a direct reference is ceng-m1.2's.
    Cell(CellKey),
    /// The values of every cell of a rectangle. Its binding from a range is ceng-m1.2's.
    Area(GridArea),
    /// The values of every cell of whole rows or whole columns. Its binding from a whole-row or
    /// whole-column reference comes with ceng-m1.2; its other facets are dc-m3u's.
    AxisValue(AxisInterval),
    /// The values of a rectangle on every sheet of a span. Its binding from a 3D reference and
    /// its calculation are dc-m3u's.
    SheetSpan(SheetSpan),
}

impl Prerequisite {
    /// The sheets this prerequisite names: one, or a span's two endpoints.
    /// Whether the cell `at` is one this prerequisite reads (a span's middle sheets aside: a span
    /// covers `at` only on one of its endpoints here).
    pub(crate) fn covers(self, at: CellKey) -> bool {
        match self {
            Prerequisite::Cell(cell) => cell == at,
            Prerequisite::Area(area) => area.contains(at),
            Prerequisite::AxisValue(interval) => {
                let position = match interval.axis() {
                    Axis::Rows => at.row(),
                    Axis::Columns => at.column(),
                };
                interval.sheet() == at.sheet()
                    && (interval.first()..=interval.last()).contains(&position)
            }
            Prerequisite::SheetSpan(span) => {
                (span.first() == at.sheet() || span.last() == at.sheet())
                    && Rect::of(span.target()).contains(at.row(), at.column())
            }
        }
    }

    /// Whether some cell of `area` is one this prerequisite reads.
    pub(crate) fn meets_area(self, area: GridArea) -> bool {
        let rect = Rect::of(area);
        match self {
            Prerequisite::Cell(cell) => area.contains(cell),
            Prerequisite::Area(own) => {
                own.sheet() == area.sheet() && Rect::of(own).intersects(rect)
            }
            Prerequisite::AxisValue(interval) => {
                let (low, high) = match interval.axis() {
                    Axis::Rows => (rect.top, rect.bottom),
                    Axis::Columns => (rect.left, rect.right),
                };
                interval.sheet() == area.sheet()
                    && low <= interval.last()
                    && interval.first() <= high
            }
            Prerequisite::SheetSpan(span) => {
                (span.first() == area.sheet() || span.last() == area.sheet())
                    && Rect::of(span.target()).intersects(rect)
            }
        }
    }

    /// Whether every cell of `area` is one this prerequisite reads.
    pub(crate) fn contains_area(self, area: GridArea) -> bool {
        match self {
            Prerequisite::Cell(cell) => area.cell_count() == 1 && area.first() == cell,
            Prerequisite::Area(own) => own.contains(area.first()) && own.contains(area.last()),
            Prerequisite::AxisValue(_) | Prerequisite::SheetSpan(_) => {
                self.covers(area.first()) && self.covers(area.last())
            }
        }
    }

    pub(crate) fn sheets(self) -> [SheetId; 2] {
        match self {
            Prerequisite::Cell(at) => [at.sheet(); 2],
            Prerequisite::Area(area) => [area.sheet(); 2],
            Prerequisite::AxisValue(interval) => [interval.sheet(); 2],
            Prerequisite::SheetSpan(span) => [span.first(), span.last()],
        }
    }
}

/// The sheets from `one` to `other` in `inputs`' order, both included, whichever comes first.
fn span_members(inputs: &ModelRoot, one: SheetId, other: SheetId) -> Option<&[SheetEntry]> {
    let sheets = inputs.sheets();
    let first = sheets.iter().position(|sheet| sheet.id() == one)?;
    let last = sheets.iter().position(|sheet| sheet.id() == other)?;
    Some(&sheets[first.min(last)..=first.max(last)])
}

/// A rectangle's rows and columns without its sheet, ordered so edges can live in sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Rect {
    top: u32,
    left: u32,
    bottom: u32,
    right: u32,
}

impl Rect {
    fn of(area: GridArea) -> Self {
        Rect {
            top: area.first().row(),
            left: area.first().column(),
            bottom: area.last().row(),
            right: area.last().column(),
        }
    }

    fn contains(self, row: u32, column: u32) -> bool {
        (self.top..=self.bottom).contains(&row) && (self.left..=self.right).contains(&column)
    }

    fn intersects(self, other: Rect) -> bool {
        self.top <= other.bottom
            && other.top <= self.bottom
            && self.left <= other.right
            && other.left <= self.right
    }
}

/// How many levels the rectangle index has. Level `l` cuts each sheet into blocks of
/// `16 * 4^l` rows by `4 * 4^l` columns; the last level's blocks (262,144 rows by 65,536 columns)
/// are larger than a quarter of the grid, so every rectangle fits a level.
const LEVELS: u8 = 8;

/// The rows and columns of a block of `level`.
const fn level_block(level: u8) -> (u32, u32) {
    (16 << (2 * level as u32), 4 << (2 * level as u32))
}

/// The level a rectangle is filed at: the first whose blocks are at least as tall and as wide as
/// it, so it meets at most two blocks down and two across.
fn level_of(rect: Rect) -> u8 {
    let (rows, columns) = (rect.bottom - rect.top + 1, rect.right - rect.left + 1);
    (0..LEVELS)
        .find(|level| {
            let (block_rows, block_columns) = level_block(*level);
            rows <= block_rows && columns <= block_columns
        })
        .unwrap_or(LEVELS - 1)
}

/// The key of the block of `level` that holds the cell at `row` and `column` of `sheet`.
fn block_of(sheet: SheetId, level: u8, row: u32, column: u32) -> BlockKey {
    let (block_rows, block_columns) = level_block(level);
    (
        sheet,
        level,
        (row - 1) / block_rows,
        (column - 1) / block_columns,
    )
}

/// A block of the rectangle index: its sheet, level, block row and block column.
type BlockKey = (SheetId, u8, u32, u32);

/// The hasher of the rectangle index's block keys (dc-10d9): a multiply-rotate hash of the few
/// integers a key holds, in place of the standard library's keyed SipHash, which a lookup of every
/// level for every cell a recalculation orders made a measurable share of ordering. The keys are
/// the engine's own block coordinates, never text a workbook chooses, so a keyed hash buys nothing
/// here.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BlockHasher(u64);

impl BlockHasher {
    const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(Self::SEED);
    }
}

impl Hasher for BlockHasher {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.add(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, n: u8) {
        self.add(u64::from(n));
    }

    fn write_u32(&mut self, n: u32) {
        self.add(u64::from(n));
    }

    fn write_u64(&mut self, n: u64) {
        self.add(n);
    }

    fn write_usize(&mut self, n: usize) {
        self.add(n as u64);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

type BlockMap<V> = HashMap<BlockKey, V, BuildHasherDefault<BlockHasher>>;

/// One stored rectangle edge: the node that reads it and its rows and columns (the sheet is the
/// key it is filed under).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RectEdge {
    node: NodeId,
    rect: Rect,
}

/// One stored axis-value edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct AxisEdge {
    first: u32,
    last: u32,
    node: NodeId,
}

/// One stored span edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SpanEdge {
    node: NodeId,
    first: SheetId,
    last: SheetId,
    rect: Rect,
}

/// The reverse indexes of the one graph: from an input to the nodes whose prerequisites read it.
/// Cell edges are keyed by cell; rectangle edges are filed in the blocks they meet at the level of
/// blocks of their own size, with the nodes reading one rectangle kept together, and checked for
/// containment (a cell's query looks in one block per level); axis-value edges are kept per sheet and axis;
/// span edges are kept once each and resolved against the snapshot's sheet order.
#[derive(Debug, Default)]
pub(crate) struct ReverseIndex {
    cells: BTreeMap<CellKey, NodeSet>,
    /// Each block's rectangles, each with the nodes that read it. A rectangle is filed in every
    /// block of its level it meets (at most four).
    rect_blocks: BlockMap<BTreeMap<Rect, NodeSet>>,
    /// The levels at which a rectangle has been filed (bit `l` for level `l`), so a cell's query
    /// looks only in those; a level whose last rectangle was removed keeps its bit.
    rect_levels: u8,
    /// How many times an edge has been filed or removed: the readers a query found stand while
    /// this is unchanged (dc-10d9, [`crate::recalc::ReadersCache`]).
    epoch: u64,
    rects_by_sheet: BTreeMap<SheetId, BTreeSet<RectEdge>>,
    row_values: BTreeMap<SheetId, BTreeSet<AxisEdge>>,
    column_values: BTreeMap<SheetId, BTreeSet<AxisEdge>>,
    spans: BTreeSet<SpanEdge>,
}

impl ReverseIndex {
    /// Files `node`'s prerequisites.
    pub(crate) fn insert(&mut self, node: NodeId, prerequisites: &[Prerequisite]) {
        for prerequisite in prerequisites {
            self.file(node, *prerequisite, true);
        }
    }

    /// Removes `node`'s prerequisites, which must be the ones filed for it.
    pub(crate) fn remove(&mut self, node: NodeId, prerequisites: &[Prerequisite]) {
        for prerequisite in prerequisites {
            self.file(node, *prerequisite, false);
        }
    }

    /// See [`ReverseIndex::epoch`] (the field).
    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }

    fn file(&mut self, node: NodeId, prerequisite: Prerequisite, insert: bool) {
        self.epoch += 1;
        match prerequisite {
            Prerequisite::Cell(at) => update(&mut self.cells, at, node, insert),
            Prerequisite::Area(area) => {
                let edge = RectEdge {
                    node,
                    rect: Rect::of(area),
                };
                let sheet = area.sheet();
                update(&mut self.rects_by_sheet, sheet, edge, insert);
                let rect = edge.rect;
                let level = level_of(rect);
                if insert {
                    self.rect_levels |= 1 << level;
                }
                let (first, last) = (
                    block_of(sheet, level, rect.top, rect.left),
                    block_of(sheet, level, rect.bottom, rect.right),
                );
                for block_row in first.2..=last.2 {
                    for block_column in first.3..=last.3 {
                        let key = (sheet, level, block_row, block_column);
                        if insert {
                            self.rect_blocks
                                .entry(key)
                                .or_default()
                                .entry(rect)
                                .or_default()
                                .insert(node);
                        } else if let Some(rects) = self.rect_blocks.get_mut(&key) {
                            update(rects, rect, node, false);
                            if rects.is_empty() {
                                self.rect_blocks.remove(&key);
                            }
                        }
                    }
                }
            }
            Prerequisite::AxisValue(interval) => {
                let edge = AxisEdge {
                    first: interval.first(),
                    last: interval.last(),
                    node,
                };
                let map = match interval.axis() {
                    Axis::Rows => &mut self.row_values,
                    Axis::Columns => &mut self.column_values,
                };
                update(map, interval.sheet(), edge, insert);
            }
            Prerequisite::SheetSpan(span) => {
                let edge = SpanEdge {
                    node,
                    first: span.first(),
                    last: span.last(),
                    rect: Rect::of(span.target()),
                };
                if insert {
                    self.spans.insert(edge);
                } else {
                    self.spans.remove(&edge);
                }
            }
        }
    }

    /// Adds to `out` every node with a value edge that covers the cell `at`. A node may be added
    /// more than once when more than one of its edges covers the cell.
    pub(crate) fn dependents_of_cell(
        &self,
        inputs: &ModelRoot,
        at: CellKey,
        out: &mut impl Extend<NodeId>,
    ) {
        let (sheet, row, column) = (at.sheet(), at.row(), at.column());
        if let Some(nodes) = self.cells.get(&at) {
            out.extend(nodes.iter().copied());
        }
        if !self.rect_blocks.is_empty() {
            for level in 0..LEVELS {
                if self.rect_levels & (1 << level) == 0 {
                    continue;
                }
                let Some(rects) = self.rect_blocks.get(&block_of(sheet, level, row, column)) else {
                    continue;
                };
                for (rect, nodes) in rects {
                    if rect.contains(row, column) {
                        out.extend(nodes.iter().copied());
                    }
                }
            }
        }
        for (map, position) in [(&self.row_values, row), (&self.column_values, column)] {
            let edges = map.get(&sheet).into_iter().flatten();
            out.extend(
                edges
                    .filter(|edge| (edge.first..=edge.last).contains(&position))
                    .map(|edge| edge.node),
            );
        }
        out.extend(
            self.spans
                .iter()
                .filter(|edge| edge.rect.contains(row, column) && covers(inputs, edge, sheet))
                .map(|edge| edge.node),
        );
    }

    /// Adds to `out` every node with a value edge that covers at least one cell of `area`, a
    /// node perhaps more than once.
    pub(crate) fn dependents_of_area(
        &self,
        inputs: &ModelRoot,
        area: GridArea,
        out: &mut impl Extend<NodeId>,
    ) {
        let sheet = area.sheet();
        let rect = Rect::of(area);
        out.extend(
            self.cells
                .range(area.first()..=area.last())
                .filter(|(at, _)| area.contains(**at))
                .flat_map(|(_, nodes)| nodes.iter().copied()),
        );
        let edges = self.rects_by_sheet.get(&sheet).into_iter().flatten();
        out.extend(
            edges
                .filter(|edge| edge.rect.intersects(rect))
                .map(|edge| edge.node),
        );
        for (map, low, high) in [
            (&self.row_values, rect.top, rect.bottom),
            (&self.column_values, rect.left, rect.right),
        ] {
            let edges = map.get(&sheet).into_iter().flatten();
            out.extend(
                edges
                    .filter(|edge| low <= edge.last && edge.first <= high)
                    .map(|edge| edge.node),
            );
        }
        out.extend(
            self.spans
                .iter()
                .filter(|edge| edge.rect.intersects(rect) && covers(inputs, edge, sheet))
                .map(|edge| edge.node),
        );
    }

    /// Adds to `out` every node with a value edge that covers at least one of `cells`, which all
    /// lie in `area`. The edges that meet `area` are the candidates (a rectangle is read from its
    /// sheet's list of rectangles), and a candidate counts only when one of `cells` lies inside
    /// it: an edge that meets `area` only at other cells adds nothing.
    pub(crate) fn dependents_of_cells_in_area(
        &self,
        inputs: &ModelRoot,
        area: GridArea,
        cells: &[CellKey],
        out: &mut BTreeSet<NodeId>,
    ) {
        debug_assert!(cells.iter().all(|at| area.contains(*at)));
        let sheet = area.sheet();
        let rect = Rect::of(area);
        let covers_one = |edge: Rect| cells.iter().any(|at| edge.contains(at.row(), at.column()));
        out.extend(cells.iter().filter_map(|at| self.cells.get(at)).flatten());
        let edges = self.rects_by_sheet.get(&sheet).into_iter().flatten();
        out.extend(
            edges
                .filter(|edge| edge.rect.intersects(rect) && covers_one(edge.rect))
                .map(|edge| edge.node),
        );
        let row: fn(CellKey) -> u32 = CellKey::row;
        let column: fn(CellKey) -> u32 = CellKey::column;
        for (map, low, high, position) in [
            (&self.row_values, rect.top, rect.bottom, row),
            (&self.column_values, rect.left, rect.right, column),
        ] {
            let edges = map.get(&sheet).into_iter().flatten();
            out.extend(
                edges
                    .filter(|edge| {
                        low <= edge.last
                            && edge.first <= high
                            && cells
                                .iter()
                                .any(|at| (edge.first..=edge.last).contains(&position(*at)))
                    })
                    .map(|edge| edge.node),
            );
        }
        out.extend(
            self.spans
                .iter()
                .filter(|edge| {
                    edge.rect.intersects(rect)
                        && covers_one(edge.rect)
                        && covers(inputs, edge, sheet)
                })
                .map(|edge| edge.node),
        );
    }
}

/// Whether `sheet` lies inside the span in the snapshot's current order. The engine refuses every
/// operation that removes or reorders a sheet, and a snapshot at the state's revision holds every
/// endpoint the state accepted, so an endpoint is never missing here.
fn covers(inputs: &ModelRoot, edge: &SpanEdge, sheet: SheetId) -> bool {
    span_members(inputs, edge.first, edge.last)
        .is_some_and(|members| members.iter().any(|member| member.id() == sheet))
}

fn update<K: Ord, V, S: Members<V>>(map: &mut BTreeMap<K, S>, key: K, value: V, insert: bool) {
    if insert {
        map.entry(key).or_default().add(value);
    } else if let Some(values) = map.get_mut(&key) {
        values.take(&value);
        if values.is_empty() {
            map.remove(&key);
        }
    }
}

/// A set the reverse index keeps its edges in.
trait Members<V>: Default {
    fn add(&mut self, value: V);
    fn take(&mut self, value: &V);
    fn is_empty(&self) -> bool;
}

impl<V: Ord> Members<V> for BTreeSet<V> {
    fn add(&mut self, value: V) {
        self.insert(value);
    }

    fn take(&mut self, value: &V) {
        self.remove(value);
    }

    fn is_empty(&self) -> bool {
        BTreeSet::is_empty(self)
    }
}

/// The nodes reading one cell or one rectangle, in ID order, as a sorted Vec (dc-dxm): most cells
/// are read by one formula or a few, which a BTreeSet holds in a node of eleven slots of its own
/// (about 120 bytes each, for the DTC model's 196,000 cells read).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NodeSet(Vec<NodeId>);

impl NodeSet {
    pub(crate) fn insert(&mut self, node: NodeId) {
        if let Err(at) = self.0.binary_search(&node) {
            self.0.insert(at, node);
        }
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, NodeId> {
        self.0.iter()
    }
}

impl Members<NodeId> for NodeSet {
    fn add(&mut self, value: NodeId) {
        self.insert(value);
    }

    fn take(&mut self, value: &NodeId) {
        if let Ok(at) = self.0.binary_search(value) {
            self.0.remove(at);
        }
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<'a> IntoIterator for &'a NodeSet {
    type Item = &'a NodeId;
    type IntoIter = std::slice::Iter<'a, NodeId>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
