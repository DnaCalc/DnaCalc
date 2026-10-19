//! The immutable model root: one authored revision of a document's sheets and their cells.

use crate::cells::{Bounds, CellPage, OccupiedCells, SheetPages, locate};
use crate::input::CellInput;
use crate::op::{OpBatch, WorkbookOp};
use crate::revision::AuthoredRevision;
use crate::sheet::{SheetKey, SheetName, SheetVisibility};
use calc_value::{CellKey, GridArea, SheetId};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

/// One sheet of a [`ModelRoot`]: its session ID, its document-stable key, its name and its
/// visibility.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetEntry {
    id: SheetId,
    key: SheetKey,
    name: SheetName,
    visibility: SheetVisibility,
}

impl SheetEntry {
    pub fn new(id: SheetId, key: SheetKey, name: SheetName, visibility: SheetVisibility) -> Self {
        SheetEntry {
            id,
            key,
            name,
            visibility,
        }
    }

    pub fn id(&self) -> SheetId {
        self.id
    }

    pub fn key(&self) -> SheetKey {
        self.key
    }

    pub fn name(&self) -> &SheetName {
        &self.name
    }

    pub fn visibility(&self) -> SheetVisibility {
        self.visibility
    }
}

/// Why [`ModelRoot::new`] refused its sheets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModelRootError {
    /// Two sheets have this key.
    DuplicateSheetKey(SheetKey),
    /// Two sheets have this ID.
    DuplicateSheetId(SheetId),
}

impl fmt::Display for ModelRootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelRootError::DuplicateSheetKey(key) => {
                write!(f, "two sheets have the key {}", key.get())
            }
            ModelRootError::DuplicateSheetId(id) => write!(f, "two sheets have the ID {id:?}"),
        }
    }
}

impl std::error::Error for ModelRootError {}

/// Why [`ModelRoot::apply`] refused a batch. A refused batch makes no root, and the root it was
/// applied to is immutable, so nothing changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ApplyError {
    /// The batch was prepared against another revision than the root's.
    StaleBase {
        batch: AuthoredRevision,
        root: AuthoredRevision,
    },
    /// The root's revision is the last one, so no revision can follow it.
    RevisionExhausted,
    /// The operation at this index of the batch names a sheet the root does not hold.
    UnknownSheet { operation: usize, sheet: SheetId },
    /// The operation at this index inserts or deletes rows or columns. This package does not
    /// apply axis edits to its pages; cdoc-m4.1's axis maps do.
    AxisEdit { operation: usize },
    /// The operation at this index inserts, renames, moves, removes, shows or hides a sheet. This
    /// package does not apply sheet edits; cdoc-m4.3 authors the sheet lifecycle.
    SheetEdit { operation: usize },
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApplyError::StaleBase { batch, root } => write!(
                f,
                "the batch is based on revision {} but the root is at revision {}",
                batch.get(),
                root.get()
            ),
            ApplyError::RevisionExhausted => {
                f.write_str("the root is at the last revision; no revision follows it")
            }
            ApplyError::UnknownSheet { operation, sheet } => write!(
                f,
                "operation {operation} names the sheet {sheet:?}, which the root does not hold"
            ),
            ApplyError::AxisEdit { operation } => write!(
                f,
                "operation {operation} inserts or deletes rows or columns, which the model root does not apply"
            ),
            ApplyError::SheetEdit { operation } => write!(
                f,
                "operation {operation} edits the workbook's sheets, which the model root does not apply"
            ),
        }
    }
}

impl std::error::Error for ApplyError {}

/// One authored revision of a document: the revision, the sheets in order, and each sheet's
/// authored cell inputs. A root is immutable and shared behind an `Arc`; a change makes a new
/// root. It maps each sheet's [`SheetKey`] to its [`SheetId`] and back, which is how wire
/// operations, which carry keys, are resolved.
///
/// A sheet's cells are held in [`CellPage`]s, each the occupied cells of one block of
/// [`crate::PAGE_ROWS`] rows by [`crate::PAGE_COLUMNS`] columns, behind an `Arc`. Every read
/// borrows from the pages: [`ModelRoot::cell`], [`ModelRoot::cells`] and
/// [`ModelRoot::sheet_cells`]. A root made by [`ModelRoot::apply`] holds new pages only for the
/// blocks the batch changed and the same allocation for every other page.
///
/// It keeps no record of the keys that removed sheets held, so it cannot tell a reused key from a
/// new one; whoever applies a [`crate::SheetEdit`] checks that a key is never reused within one
/// document.
#[derive(Debug, PartialEq, Eq)]
pub struct ModelRoot {
    revision: AuthoredRevision,
    sheets: Vec<SheetEntry>,
    /// The cells of the sheet at the same position of `sheets`.
    cells: Vec<SheetPages>,
}

impl ModelRoot {
    /// The root of these sheets, in this order, with no cells, at `revision`; refused when two
    /// sheets share a key or an ID.
    pub fn new(
        revision: AuthoredRevision,
        sheets: Vec<SheetEntry>,
    ) -> Result<Arc<ModelRoot>, ModelRootError> {
        for (index, sheet) in sheets.iter().enumerate() {
            let earlier = &sheets[..index];
            if earlier.iter().any(|other| other.key == sheet.key) {
                return Err(ModelRootError::DuplicateSheetKey(sheet.key));
            }
            if earlier.iter().any(|other| other.id == sheet.id) {
                return Err(ModelRootError::DuplicateSheetId(sheet.id));
            }
        }
        let cells = vec![SheetPages::default(); sheets.len()];
        Ok(Arc::new(ModelRoot {
            revision,
            sheets,
            cells,
        }))
    }

    pub fn revision(&self) -> AuthoredRevision {
        self.revision
    }

    /// The sheets in workbook order.
    pub fn sheets(&self) -> &[SheetEntry] {
        &self.sheets
    }

    pub fn sheet_by_key(&self, key: SheetKey) -> Option<&SheetEntry> {
        self.sheets.iter().find(|sheet| sheet.key == key)
    }

    pub fn sheet_by_id(&self, id: SheetId) -> Option<&SheetEntry> {
        self.sheets.iter().find(|sheet| sheet.id == id)
    }

    fn pages_of(&self, sheet: SheetId) -> Option<&SheetPages> {
        let position = self.sheets.iter().position(|entry| entry.id == sheet)?;
        Some(&self.cells[position])
    }

    /// The authored input of one cell, borrowed from its page. `None` when the cell is empty or
    /// its sheet is not in this root; [`ModelRoot::sheet_by_id`] tells the two apart.
    pub fn cell(&self, at: CellKey) -> Option<&CellInput> {
        self.pages_of(at.sheet())?.cell(at)
    }

    /// The occupied cells of `area`, borrowed from the pages, in row-major order (by row, then by
    /// column); `None` when the area's sheet is not in this root.
    pub fn cells(&self, area: GridArea) -> Option<OccupiedCells<'_>> {
        let (first, last) = (area.first(), area.last());
        let bounds = Bounds {
            top: first.row(),
            left: first.column(),
            bottom: last.row(),
            right: last.column(),
        };
        Some(self.pages_of(area.sheet())?.occupied(area.sheet(), bounds))
    }

    /// Every occupied cell of a sheet, borrowed from the pages, in row-major order; `None` when
    /// the sheet is not in this root.
    pub fn sheet_cells(&self, sheet: SheetId) -> Option<OccupiedCells<'_>> {
        self.cells(GridArea::full_sheet(sheet))
    }

    /// A sheet's pages, in page order (by first row, then by first column); `None` when the sheet
    /// is not in this root. An empty block has no page.
    pub fn pages(&self, sheet: SheetId) -> Option<impl Iterator<Item = &Arc<CellPage>>> {
        Some(self.pages_of(sheet)?.pages())
    }

    /// The page whose block holds `at`; `None` when the block has no occupied cell or the sheet is
    /// not in this root.
    pub fn page_of(&self, at: CellKey) -> Option<&Arc<CellPage>> {
        let (page_row, page_column, _) = locate(at);
        self.pages_of(at.sheet())?.page(page_row, page_column)
    }

    fn position_of(&self, operation: usize, sheet: SheetId) -> Result<usize, ApplyError> {
        self.sheets
            .iter()
            .position(|entry| entry.id == sheet)
            .ok_or(ApplyError::UnknownSheet { operation, sheet })
    }

    /// The root that applying `batch` to this one makes, at the next revision. Operations apply
    /// in order: a later operation on a cell sees what an earlier one left there. A
    /// [`WorkbookOp::SetCell`] replaces one cell's input and a [`WorkbookOp::Clear`] empties every
    /// cell of its area.
    ///
    /// The new root holds a new page for each block whose cells the batch changed, and no page
    /// for a block it left empty; each page is built once per batch, however many operations
    /// touch it. Every other page, a page whose cells the batch set to what they already held
    /// included, is the old root's allocation, and so is every index node off the changed pages'
    /// paths. This root is unchanged; the caller decides whether the new root replaces it.
    ///
    /// Refused, with nothing made, when the batch is based on another revision, the revision is
    /// the last one, an operation names a sheet the root does not hold, or the batch holds a
    /// structural edit ([`ApplyError::AxisEdit`], [`ApplyError::SheetEdit`]).
    pub fn apply(&self, batch: &OpBatch) -> Result<Arc<ModelRoot>, ApplyError> {
        if batch.base != self.revision {
            return Err(ApplyError::StaleBase {
                batch: batch.base,
                root: self.revision,
            });
        }
        let revision = self
            .revision
            .checked_next()
            .ok_or(ApplyError::RevisionExhausted)?;

        // A working copy of each page the batch touches, by (sheet position, page row, page
        // column), made from this root's page the first time an operation changes the block.
        let mut touched: BTreeMap<(usize, u32, u32), BTreeMap<u16, CellInput>> = BTreeMap::new();
        for (operation, op) in batch.operations.iter().enumerate() {
            match op {
                WorkbookOp::SetCell { at, input } => {
                    let sheet = self.position_of(operation, at.sheet())?;
                    let (page_row, page_column, place) = locate(*at);
                    touched
                        .entry((sheet, page_row, page_column))
                        .or_insert_with(|| self.working_copy(sheet, page_row, page_column))
                        .insert(place, input.clone());
                }
                WorkbookOp::Clear { area } => {
                    let sheet = self.position_of(operation, area.sheet())?;
                    let (first, last) = (area.first(), area.last());
                    let bounds = Bounds {
                        top: first.row(),
                        left: first.column(),
                        bottom: last.row(),
                        right: last.column(),
                    };
                    for page in self.cells[sheet].pages_meeting(bounds) {
                        let key = (sheet, page.page_row_index(), page.page_column_index());
                        if !touched.contains_key(&key) && page.has_cell_in(bounds) {
                            touched.insert(key, page.working_copy());
                        }
                    }
                    let (first_page_row, first_page_column, _) = locate(first);
                    let (last_page_row, last_page_column, _) = locate(last);
                    let span = (sheet, first_page_row, 0)..=(sheet, last_page_row, u32::MAX);
                    for ((_, page_row, page_column), cells) in touched.range_mut(span) {
                        if (first_page_column..=last_page_column).contains(page_column) {
                            cells.retain(|place, _| {
                                let (row, column) =
                                    CellPage::position(*page_row, *page_column, *place);
                                !bounds.contains(row, column)
                            });
                        }
                    }
                }
                WorkbookOp::EditAxis(_) => return Err(ApplyError::AxisEdit { operation }),
                WorkbookOp::EditSheet(_) => return Err(ApplyError::SheetEdit { operation }),
            }
        }

        let mut cells = self.cells.clone();
        for ((sheet, page_row, page_column), working) in touched {
            let old = self.cells[sheet].page(page_row, page_column);
            let new = CellPage::build(page_row, page_column, working);
            match (old, new) {
                (Some(old), Some(new)) if **old == new => {}
                (None, None) => {}
                (_, new) => cells[sheet].set_page(page_row, page_column, new.map(Arc::new)),
            }
        }
        Ok(Arc::new(ModelRoot {
            revision,
            sheets: self.sheets.clone(),
            cells,
        }))
    }

    fn working_copy(
        &self,
        sheet: usize,
        page_row: u32,
        page_column: u32,
    ) -> BTreeMap<u16, CellInput> {
        self.cells[sheet]
            .page(page_row, page_column)
            .map(|page| page.working_copy())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::FiniteNumber;
    use calc_value::{ExcelText, IdAllocator, MAX_COLUMNS, MAX_ROWS};

    /// A fixed-seed xorshift64 generator, so a failure repeats.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }

        fn pick(&mut self, from: &[u32]) -> u32 {
            from[self.below(from.len() as u64) as usize]
        }
    }

    /// Rows and columns at and beside page edges and grid edges, or a small random one.
    fn row(rng: &mut Rng) -> u32 {
        const EDGES: [u32; 12] = [
            1, 2, 255, 256, 257, 512, 513, 16_384, 16_385, 1_048_320, 1_048_321, MAX_ROWS,
        ];
        if rng.below(2) == 0 {
            rng.pick(&EDGES)
        } else {
            1 + rng.below(600) as u32
        }
    }

    fn column(rng: &mut Rng) -> u32 {
        const EDGES: [u32; 10] = [1, 15, 16, 17, 32, 33, 512, 513, 16_368, MAX_COLUMNS];
        if rng.below(2) == 0 {
            rng.pick(&EDGES)
        } else {
            1 + rng.below(60) as u32
        }
    }

    type Model = BTreeMap<(usize, u32, u32), CellInput>;

    fn expected(model: &Model, sheet: usize, area: GridArea) -> Vec<(u32, u32, CellInput)> {
        model
            .iter()
            .filter(|((s, r, c), _)| {
                *s == sheet && CellKey::new(area.sheet(), *r, *c).is_ok_and(|at| area.contains(at))
            })
            .map(|((_, r, c), input)| (*r, *c, input.clone()))
            .collect()
    }

    /// Random batches of SetCell and Clear applied to a root and to an ordered map: after each,
    /// the root's sheet, rectangle and cell reads equal the map's, every page is non-empty and
    /// holds only cells of its block, and every page whose cells did not change is the old
    /// root's allocation.
    #[test]
    fn pages_match_an_ordered_map_under_random_batches() {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let sheets: Vec<SheetId> = (0..2).map(|_| ids.issue().expect("an ID")).collect();
        let name = |text: &str| {
            SheetName::new(ExcelText::from_utf16_code_units(
                text.encode_utf16().collect(),
            ))
            .expect("a name")
        };
        let mut root = ModelRoot::new(
            AuthoredRevision::INITIAL,
            vec![
                SheetEntry::new(
                    sheets[0],
                    SheetKey::new(1),
                    name("A"),
                    SheetVisibility::Visible,
                ),
                SheetEntry::new(
                    sheets[1],
                    SheetKey::new(2),
                    name("B"),
                    SheetVisibility::Visible,
                ),
            ],
        )
        .expect("two sheets");
        let mut model = Model::new();
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let mut counter = 0.0;
        let mut shared_pages = 0;
        for _ in 0..300 {
            let mut operations = Vec::new();
            for _ in 0..1 + rng.below(8) {
                let sheet = rng.below(2) as usize;
                let at = CellKey::new(sheets[sheet], row(&mut rng), column(&mut rng)).unwrap();
                if rng.below(4) == 0 {
                    let other =
                        CellKey::new(sheets[sheet], row(&mut rng), column(&mut rng)).unwrap();
                    let area = GridArea::new(at, other).unwrap();
                    model.retain(|(s, r, c), _| {
                        *s != sheet || !area.contains(CellKey::new(sheets[sheet], *r, *c).unwrap())
                    });
                    operations.push(WorkbookOp::Clear { area });
                } else {
                    // Every tenth set repeats the cell's current input, a change of nothing.
                    let input = match model.get(&(sheet, at.row(), at.column())) {
                        Some(input) if rng.below(10) == 0 => input.clone(),
                        _ => {
                            counter += 1.0;
                            CellInput::Number(FiniteNumber::new(counter).unwrap())
                        }
                    };
                    model.insert((sheet, at.row(), at.column()), input.clone());
                    operations.push(WorkbookOp::SetCell { at, input });
                }
            }
            let batch = OpBatch {
                base: root.revision(),
                operations,
            };
            let next = root.apply(&batch).expect("cell operations on held sheets");
            for (index, sheet) in sheets.iter().enumerate() {
                let full = GridArea::full_sheet(*sheet);
                let read: Vec<_> = next
                    .sheet_cells(*sheet)
                    .unwrap()
                    .map(|(at, input)| (at.row(), at.column(), input.clone()))
                    .collect();
                assert_eq!(read, expected(&model, index, full));
                let a = CellKey::new(*sheet, row(&mut rng), column(&mut rng)).unwrap();
                let b = CellKey::new(*sheet, row(&mut rng), column(&mut rng)).unwrap();
                let area = GridArea::new(a, b).unwrap();
                let read: Vec<_> = next
                    .cells(area)
                    .unwrap()
                    .map(|(at, input)| (at.row(), at.column(), input.clone()))
                    .collect();
                assert_eq!(read, expected(&model, index, area));
                assert_eq!(
                    next.cell(a),
                    model.get(&(index, a.row(), a.column())),
                    "cell {a:?}"
                );
                let old_pages: BTreeMap<(u32, u32), &Arc<CellPage>> = root
                    .pages(*sheet)
                    .unwrap()
                    .map(|page| ((page.first_row(), page.first_column()), page))
                    .collect();
                for page in next.pages(*sheet).unwrap() {
                    assert!(page.cell_count() > 0, "no empty page");
                    for (r, c, input) in page.iter() {
                        assert!((page.first_row()..=page.last_row()).contains(&r));
                        assert!((page.first_column()..=page.last_column()).contains(&c));
                        assert_eq!(model.get(&(index, r, c)), Some(input));
                    }
                    let block = (page.first_row(), page.first_column());
                    if let Some(old) = old_pages.get(&block)
                        && **old == *page
                    {
                        assert!(Arc::ptr_eq(old, page), "an unchanged page is shared");
                        shared_pages += 1;
                    }
                }
            }
            root = next;
        }
        assert!(shared_pages > 1000, "{shared_pages} shared pages");
    }
}
