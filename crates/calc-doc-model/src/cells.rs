//! Authored cell storage: each sheet's cell inputs in immutable, `Arc`-shared sparse pages.
//!
//! A sheet is divided into fixed blocks of [`PAGE_ROWS`] rows by [`PAGE_COLUMNS`] columns; a
//! [`CellPage`] holds the occupied cells of one block, so no page holds more than 4,096 cells, and
//! a block with no occupied cell has no page. A sheet's pages are found through a four-level
//! index of `Arc`-shared nodes, each with at most 64 children: the high and low bits of the
//! block's row index, then of its column index. A new root made by an edit copies the pages the
//! edit changes and the index nodes on their paths, at most 64 child pointers each, and shares
//! every other page and node with the old root by pointer. No allocation holds more than one
//! block's cells or 64 child pointers, so a one-cell edit never copies a whole sheet.

use crate::input::CellInput;
use calc_value::{CellKey, MAX_COLUMNS, MAX_ROWS, SheetId};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The rows one page spans: a page's first row is 1, 257, 513, ... .
pub const PAGE_ROWS: u32 = 256;

/// The columns one page spans: a page's first column is 1 (A), 17 (Q), 33 (AG), ... .
pub const PAGE_COLUMNS: u32 = 16;

/// The page rows of a sheet: 4,096.
const PAGE_ROW_COUNT: u32 = MAX_ROWS / PAGE_ROWS;

/// The page columns of a sheet: 1,024.
const PAGE_COLUMN_COUNT: u32 = MAX_COLUMNS / PAGE_COLUMNS;

// The pages divide the grid exactly, a page's places fit a u16, and the index's four levels
// (64 x 64 page rows, 32 x 32 page columns) address every page.
const _: () = assert!(PAGE_ROW_COUNT * PAGE_ROWS == MAX_ROWS);
const _: () = assert!(PAGE_COLUMN_COUNT * PAGE_COLUMNS == MAX_COLUMNS);
const _: () = assert!(PAGE_ROWS * PAGE_COLUMNS <= 1 << 16);
const _: () = assert!(PAGE_ROW_COUNT == ROW_FANOUT * ROW_FANOUT);
const _: () = assert!(PAGE_COLUMN_COUNT == COLUMN_FANOUT * COLUMN_FANOUT);

const ROW_FANOUT: u32 = 64;
const COLUMN_FANOUT: u32 = 32;

/// The page row, the page column and the place in the page of a cell.
pub(crate) fn locate(at: CellKey) -> (u32, u32, u16) {
    let row = at.row() - 1;
    let column = at.column() - 1;
    let place = (row % PAGE_ROWS) * PAGE_COLUMNS + column % PAGE_COLUMNS;
    (
        row / PAGE_ROWS,
        column / PAGE_COLUMNS,
        u16::try_from(place).expect("a place in a page fits a u16"),
    )
}

/// An inclusive rectangle of 1-based rows and columns.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Bounds {
    pub(crate) top: u32,
    pub(crate) left: u32,
    pub(crate) bottom: u32,
    pub(crate) right: u32,
}

impl Bounds {
    fn page_rows(self) -> (u32, u32) {
        ((self.top - 1) / PAGE_ROWS, (self.bottom - 1) / PAGE_ROWS)
    }

    fn page_columns(self) -> (u32, u32) {
        (
            (self.left - 1) / PAGE_COLUMNS,
            (self.right - 1) / PAGE_COLUMNS,
        )
    }

    pub(crate) fn contains(self, row: u32, column: u32) -> bool {
        (self.top..=self.bottom).contains(&row) && (self.left..=self.right).contains(&column)
    }
}

/// The occupied cells of one block of [`PAGE_ROWS`] rows by [`PAGE_COLUMNS`] columns of one
/// sheet, in row-major order. A page is immutable and never empty; roots hold it behind an `Arc`,
/// so a root made by an edit that did not touch the page holds the same allocation.
#[derive(Debug, PartialEq, Eq)]
pub struct CellPage {
    page_row: u16,
    page_column: u16,
    /// Each occupied cell's place, `row offset * PAGE_COLUMNS + column offset`, ascending.
    places: Box<[u16]>,
    /// The input of the cell at the same position of `places`.
    inputs: Box<[CellInput]>,
}

impl CellPage {
    /// The page of these cells (place to input), or `None` when there are none.
    pub(crate) fn build(
        page_row: u32,
        page_column: u32,
        cells: BTreeMap<u16, CellInput>,
    ) -> Option<CellPage> {
        if cells.is_empty() {
            return None;
        }
        let (places, inputs): (Vec<u16>, Vec<CellInput>) = cells.into_iter().unzip();
        Some(CellPage {
            page_row: u16::try_from(page_row).expect("a page row fits a u16"),
            page_column: u16::try_from(page_column).expect("a page column fits a u16"),
            places: places.into_boxed_slice(),
            inputs: inputs.into_boxed_slice(),
        })
    }

    /// The page's cells as a map from place to input, for an edit to change.
    pub(crate) fn working_copy(&self) -> BTreeMap<u16, CellInput> {
        self.places
            .iter()
            .copied()
            .zip(self.inputs.iter().cloned())
            .collect()
    }

    /// The first row of the page's block, 1-based.
    pub fn first_row(&self) -> u32 {
        u32::from(self.page_row) * PAGE_ROWS + 1
    }

    /// The first column of the page's block, 1-based.
    pub fn first_column(&self) -> u32 {
        u32::from(self.page_column) * PAGE_COLUMNS + 1
    }

    /// The last row of the page's block.
    pub fn last_row(&self) -> u32 {
        self.first_row() + (PAGE_ROWS - 1)
    }

    /// The last column of the page's block.
    pub fn last_column(&self) -> u32 {
        self.first_column() + (PAGE_COLUMNS - 1)
    }

    /// The number of occupied cells, at least one.
    pub fn cell_count(&self) -> usize {
        self.places.len()
    }

    /// The input of the cell at `row` and `column` (1-based, on the page's sheet), borrowed from
    /// the page; `None` when the cell is empty or outside the page's block.
    pub fn get(&self, row: u32, column: u32) -> Option<&CellInput> {
        if !(self.first_row()..=self.last_row()).contains(&row)
            || !(self.first_column()..=self.last_column()).contains(&column)
        {
            return None;
        }
        let place = (row - self.first_row()) * PAGE_COLUMNS + (column - self.first_column());
        let place = u16::try_from(place).ok()?;
        let index = self.places.binary_search(&place).ok()?;
        Some(&self.inputs[index])
    }

    /// The occupied cells as (row, column, input), in row-major order, borrowed from the page.
    pub fn iter(&self) -> impl Iterator<Item = (u32, u32, &CellInput)> + '_ {
        self.places
            .iter()
            .zip(self.inputs.iter())
            .map(|(place, input)| (self.row_of(*place), self.column_of(*place), input))
    }

    pub(crate) fn page_row_index(&self) -> u32 {
        u32::from(self.page_row)
    }

    pub(crate) fn page_column_index(&self) -> u32 {
        u32::from(self.page_column)
    }

    /// The row and column (1-based) of `place` in the page of this page row and page column.
    pub(crate) fn position(page_row: u32, page_column: u32, place: u16) -> (u32, u32) {
        (
            page_row * PAGE_ROWS + 1 + u32::from(place) / PAGE_COLUMNS,
            page_column * PAGE_COLUMNS + 1 + u32::from(place) % PAGE_COLUMNS,
        )
    }

    fn row_of(&self, place: u16) -> u32 {
        self.first_row() + u32::from(place) / PAGE_COLUMNS
    }

    fn column_of(&self, place: u16) -> u32 {
        self.first_column() + u32::from(place) % PAGE_COLUMNS
    }

    /// Whether any occupied cell lies inside `bounds`.
    pub(crate) fn has_cell_in(&self, bounds: Bounds) -> bool {
        let mut cursor = Cursor::new(self, bounds);
        cursor.settle();
        cursor.row().is_some()
    }
}

/// One node of a sheet's page index: up to 64 children, each at an index below 64, kept in index
/// order and found through the bitmap of occupied indexes.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Node<T> {
    occupied: u64,
    children: Vec<T>,
}

/// The bits below `n`, all 64 when `n` is 64 or more.
fn bits_below(n: u32) -> u64 {
    if n >= 64 { u64::MAX } else { (1u64 << n) - 1 }
}

/// The indexes of the set bits of a bitmap, ascending.
struct SetBits(u64);

impl Iterator for SetBits {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        if self.0 == 0 {
            return None;
        }
        let index = self.0.trailing_zeros();
        self.0 &= self.0 - 1;
        Some(index)
    }
}

impl<T> Node<T> {
    fn empty() -> Self {
        Node {
            occupied: 0,
            children: Vec::new(),
        }
    }

    fn is_empty(&self) -> bool {
        self.occupied == 0
    }

    fn slot(&self, index: u32) -> usize {
        (self.occupied & bits_below(index)).count_ones() as usize
    }

    fn holds(&self, index: u32) -> bool {
        self.occupied & (1u64 << index) != 0
    }

    fn get(&self, index: u32) -> Option<&T> {
        self.holds(index).then(|| &self.children[self.slot(index)])
    }

    fn get_mut(&mut self, index: u32) -> Option<&mut T> {
        if !self.holds(index) {
            return None;
        }
        let slot = self.slot(index);
        Some(&mut self.children[slot])
    }

    /// Sets the child at `index`, replacing the one there.
    fn set(&mut self, index: u32, child: T) {
        let slot = self.slot(index);
        if self.holds(index) {
            self.children[slot] = child;
        } else {
            self.children.insert(slot, child);
            self.occupied |= 1u64 << index;
        }
    }

    fn remove(&mut self, index: u32) {
        if self.holds(index) {
            let slot = self.slot(index);
            self.children.remove(slot);
            self.occupied &= !(1u64 << index);
        }
    }

    /// The children at indexes `first` to `last` inclusive, with their indexes, in index order.
    fn range(&self, first: u32, last: u32) -> impl Iterator<Item = (u32, &T)> {
        let below = bits_below(first);
        let wanted = self.occupied & bits_below(last + 1) & !below;
        let skip = (self.occupied & below).count_ones() as usize;
        SetBits(wanted).zip(self.children[skip..].iter())
    }

    fn iter(&self) -> impl Iterator<Item = &T> {
        self.children.iter()
    }
}

/// The pages of 32 consecutive page columns of one page row.
type ColumnGroup = Node<Arc<CellPage>>;
/// The pages of one page row, by group of 32 page columns.
type PageRow = Node<Arc<ColumnGroup>>;
/// 64 consecutive page rows.
type RowGroup = Node<Arc<PageRow>>;
/// A sheet's page rows, by group of 64.
type SheetIndex = Node<Arc<RowGroup>>;

/// The child of `node` at `index`, copied first when another root shares it, and created empty
/// when absent.
fn child_mut<U: Clone>(node: &mut Node<Arc<Node<U>>>, index: u32) -> &mut Node<U> {
    if !node.holds(index) {
        node.set(index, Arc::new(Node::empty()));
    }
    let child = node.get_mut(index).expect("the child was just set");
    Arc::make_mut(child)
}

/// One sheet's authored cells: its pages, found through its page index. An empty sheet has no
/// index.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SheetPages {
    index: Option<Arc<SheetIndex>>,
}

impl SheetPages {
    pub(crate) fn page(&self, page_row: u32, page_column: u32) -> Option<&Arc<CellPage>> {
        self.index
            .as_ref()?
            .get(page_row / ROW_FANOUT)?
            .get(page_row % ROW_FANOUT)?
            .get(page_column / COLUMN_FANOUT)?
            .get(page_column % COLUMN_FANOUT)
    }

    pub(crate) fn cell(&self, at: CellKey) -> Option<&CellInput> {
        let (page_row, page_column, _) = locate(at);
        self.page(page_row, page_column)?.get(at.row(), at.column())
    }

    /// Every page, in page order: by page row, then by page column.
    pub(crate) fn pages(&self) -> impl Iterator<Item = &Arc<CellPage>> {
        self.index.iter().flat_map(|index| {
            index.iter().flat_map(|group| {
                group
                    .iter()
                    .flat_map(|row| row.iter().flat_map(|columns| columns.iter()))
            })
        })
    }

    /// The pages of one page row whose blocks meet page columns `first` to `last`, in page
    /// column order.
    fn pages_of_row<'a>(row: &'a PageRow, first: u32, last: u32, out: &mut Vec<&'a CellPage>) {
        let (first_group, last_group) = (first / COLUMN_FANOUT, last / COLUMN_FANOUT);
        for (group, columns) in row.range(first_group, last_group) {
            let from = if group == first_group {
                first % COLUMN_FANOUT
            } else {
                0
            };
            let to = if group == last_group {
                last % COLUMN_FANOUT
            } else {
                COLUMN_FANOUT - 1
            };
            out.extend(columns.range(from, to).map(|(_, page)| &**page));
        }
    }

    /// The first page row from `from` to `bounds`' last page row with a page meeting `bounds`'
    /// page columns, with those pages in `out` (cleared first), in page column order.
    fn next_page_row<'a>(
        &'a self,
        from: u32,
        bounds: Bounds,
        out: &mut Vec<&'a CellPage>,
    ) -> Option<u32> {
        out.clear();
        let index = self.index.as_deref()?;
        let (_, last) = bounds.page_rows();
        let (first_column, last_column) = bounds.page_columns();
        if from > last {
            return None;
        }
        let (first_group, last_group) = (from / ROW_FANOUT, last / ROW_FANOUT);
        for (group, rows) in index.range(first_group, last_group) {
            let low = if group == first_group {
                from % ROW_FANOUT
            } else {
                0
            };
            let high = if group == last_group {
                last % ROW_FANOUT
            } else {
                ROW_FANOUT - 1
            };
            for (offset, row) in rows.range(low, high) {
                Self::pages_of_row(row, first_column, last_column, out);
                if !out.is_empty() {
                    return Some(group * ROW_FANOUT + offset);
                }
            }
        }
        None
    }

    /// Every page whose block meets `bounds`, in page order.
    pub(crate) fn pages_meeting(&self, bounds: Bounds) -> Vec<&CellPage> {
        let mut all = Vec::new();
        let mut row_pages = Vec::new();
        let mut from = bounds.page_rows().0;
        while let Some(page_row) = self.next_page_row(from, bounds, &mut row_pages) {
            all.append(&mut row_pages);
            from = page_row + 1;
        }
        all
    }

    /// Sets the page of one block, or removes it (`None`), copying the index nodes on its path
    /// that another root shares and removing the nodes left empty.
    pub(crate) fn set_page(
        &mut self,
        page_row: u32,
        page_column: u32,
        page: Option<Arc<CellPage>>,
    ) {
        let path = (
            page_row / ROW_FANOUT,
            page_row % ROW_FANOUT,
            page_column / COLUMN_FANOUT,
            page_column % COLUMN_FANOUT,
        );
        match page {
            Some(page) => {
                let index =
                    Arc::make_mut(self.index.get_or_insert_with(|| Arc::new(Node::empty())));
                let rows = child_mut(index, path.0);
                let row = child_mut(rows, path.1);
                let columns = child_mut(row, path.2);
                columns.set(path.3, page);
            }
            None => {
                if self.page(page_row, page_column).is_none() {
                    return;
                }
                let Some(index) = self.index.as_mut() else {
                    return;
                };
                let index = Arc::make_mut(index);
                let rows_empty = {
                    let rows = child_mut(index, path.0);
                    let row_empty = {
                        let row = child_mut(rows, path.1);
                        let columns_empty = {
                            let columns = child_mut(row, path.2);
                            columns.remove(path.3);
                            columns.is_empty()
                        };
                        if columns_empty {
                            row.remove(path.2);
                        }
                        row.is_empty()
                    };
                    if row_empty {
                        rows.remove(path.1);
                    }
                    rows.is_empty()
                };
                if rows_empty {
                    index.remove(path.0);
                }
                if index.is_empty() {
                    self.index = None;
                }
            }
        }
    }

    pub(crate) fn occupied(&self, sheet: SheetId, bounds: Bounds) -> OccupiedCells<'_> {
        OccupiedCells {
            sheet,
            pages: self,
            bounds,
            next_page_row: bounds.page_rows().0,
            scratch: Vec::new(),
            cursors: Vec::new(),
            row: None,
            at: 0,
        }
    }
}

/// A position in one page's cells, limited to the cells inside a rectangle.
#[derive(Debug)]
struct Cursor<'a> {
    page: &'a CellPage,
    /// The next cell's position in the page's arrays.
    next: usize,
    /// The position past the last cell in the rectangle's rows.
    end: usize,
    /// The rectangle's columns, as column offsets within the page.
    left: u16,
    right: u16,
}

impl<'a> Cursor<'a> {
    fn new(page: &'a CellPage, bounds: Bounds) -> Self {
        let width = PAGE_COLUMNS as u16;
        let first_row = bounds.top.max(page.first_row()) - page.first_row();
        let last_row = bounds.bottom.min(page.last_row()) - page.first_row();
        let left = bounds.left.max(page.first_column()) - page.first_column();
        let right = bounds.right.min(page.last_column()) - page.first_column();
        let start = u16::try_from(first_row).expect("a row offset fits a u16") * width;
        let stop = u32::from(u16::try_from(last_row).expect("a row offset fits a u16") * width)
            + PAGE_COLUMNS;
        Cursor {
            page,
            next: page.places.partition_point(|place| *place < start),
            end: page
                .places
                .partition_point(|place| u32::from(*place) < stop),
            left: u16::try_from(left).expect("a column offset fits a u16"),
            right: u16::try_from(right).expect("a column offset fits a u16"),
        }
    }

    /// Moves past the cells outside the rectangle's columns.
    fn settle(&mut self) {
        let width = PAGE_COLUMNS as u16;
        while self.next < self.end {
            let column = self.page.places[self.next] % width;
            if (self.left..=self.right).contains(&column) {
                break;
            }
            self.next += 1;
        }
    }

    /// The row of the next cell, once settled.
    fn row(&self) -> Option<u32> {
        (self.next < self.end).then(|| self.page.row_of(self.page.places[self.next]))
    }
}

/// The occupied cells of a rectangle of one sheet, borrowed from the root's pages, in row-major
/// order: by row, then by column. It walks one page row at a time, holding a cursor for each of
/// its pages that meets the rectangle, and visits no page outside the rectangle.
#[derive(Debug)]
pub struct OccupiedCells<'a> {
    sheet: SheetId,
    pages: &'a SheetPages,
    bounds: Bounds,
    /// The first page row not yet visited.
    next_page_row: u32,
    scratch: Vec<&'a CellPage>,
    /// One cursor for each page of the current page row that meets the rectangle, in column
    /// order.
    cursors: Vec<Cursor<'a>>,
    /// The row being visited, `None` between page rows.
    row: Option<u32>,
    /// The cursor being visited in that row.
    at: usize,
}

impl<'a> Iterator for OccupiedCells<'a> {
    type Item = (CellKey, &'a CellInput);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(row) = self.row {
                while let Some(cursor) = self.cursors.get_mut(self.at) {
                    if cursor.row() == Some(row) {
                        let index = cursor.next;
                        cursor.next += 1;
                        cursor.settle();
                        let page = cursor.page;
                        let at = CellKey::new(self.sheet, row, page.column_of(page.places[index]))
                            .expect("a page's cells lie inside the grid");
                        return Some((at, &page.inputs[index]));
                    }
                    self.at += 1;
                }
                self.row = self.cursors.iter().filter_map(Cursor::row).min();
                self.at = 0;
                continue;
            }
            let page_row =
                self.pages
                    .next_page_row(self.next_page_row, self.bounds, &mut self.scratch)?;
            self.next_page_row = page_row + 1;
            self.cursors.clear();
            for page in self.scratch.drain(..) {
                let mut cursor = Cursor::new(page, self.bounds);
                cursor.settle();
                self.cursors.push(cursor);
            }
            self.row = self.cursors.iter().filter_map(Cursor::row).min();
            self.at = 0;
        }
    }
}
