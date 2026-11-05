//! The immutable model root: one authored revision of a document's sheets, their cells and its
//! skin layout.

use crate::cells::{Bounds, CellPage, OccupiedCells, SheetPages, locate};
use crate::definitions::{ArrayFormula, Definitions};
use crate::input::CellInput;
use crate::layout::{BlockId, LayoutError, SkinLayout, SkinLayoutEdit};
use crate::op::{OpBatch, WorkbookOp};
use crate::revision::AuthoredRevision;
use crate::sheet::{SheetKey, SheetName, SheetVisibility};
use calc_value::{CellKey, GridArea, SheetId};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// The lineage the next root made by [`ModelRoot::new`] or [`ModelRoot::with_cells`] starts.
static NEXT_LINEAGE: AtomicU64 = AtomicU64::new(1);

fn new_lineage() -> u64 {
    NEXT_LINEAGE.fetch_add(1, Ordering::Relaxed)
}

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

/// Why [`ModelRoot::new`], [`ModelRoot::with_cells`] or [`ModelRoot::with_layout`] refused its
/// sheets, cells or layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModelRootError {
    /// Two sheets have this key.
    DuplicateSheetKey(SheetKey),
    /// Two sheets have this ID.
    DuplicateSheetId(SheetId),
    /// This cell lies on a sheet the root does not hold.
    CellOnUnknownSheet(CellKey),
    /// This block's anchor lies on a sheet the root does not hold.
    BlockOnUnknownSheet(BlockId),
    /// An array formula, table, sheet-scoped name or unloaded cell of the definitions lies on
    /// this sheet, which the root does not hold.
    DefinitionOnUnknownSheet(SheetId),
    /// The array formula anchored at this cell has no formula input there.
    ArrayWithoutFormula(CellKey),
}

impl fmt::Display for ModelRootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelRootError::DuplicateSheetKey(key) => {
                write!(f, "two sheets have the key {}", key.get())
            }
            ModelRootError::DuplicateSheetId(id) => write!(f, "two sheets have the ID {id:?}"),
            ModelRootError::CellOnUnknownSheet(at) => write!(
                f,
                "the cell at row {} column {} lies on the sheet {:?}, which the root does not hold",
                at.row(),
                at.column(),
                at.sheet()
            ),
            ModelRootError::BlockOnUnknownSheet(block) => write!(
                f,
                "the anchor of the block {block} lies on a sheet the root does not hold"
            ),
            ModelRootError::DefinitionOnUnknownSheet(sheet) => write!(
                f,
                "a definition lies on the sheet {sheet:?}, which the root does not hold"
            ),
            ModelRootError::ArrayWithoutFormula(at) => write!(
                f,
                "the array formula anchored at row {} column {} has no formula there",
                at.row(),
                at.column()
            ),
        }
    }
}

/// Refuses sheets of which two share a key or an ID.
fn check_sheets(sheets: &[SheetEntry]) -> Result<(), ModelRootError> {
    for (index, sheet) in sheets.iter().enumerate() {
        let earlier = &sheets[..index];
        if earlier.iter().any(|other| other.key == sheet.key) {
            return Err(ModelRootError::DuplicateSheetKey(sheet.key));
        }
        if earlier.iter().any(|other| other.id == sheet.id) {
            return Err(ModelRootError::DuplicateSheetId(sheet.id));
        }
    }
    Ok(())
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
    /// The operation at this index inserts or deletes rows or columns, which
    /// [`ModelRoot::apply`] does not apply: [`ModelRoot::apply_structural`] does, with a formula
    /// rewriter.
    AxisEdit { operation: usize },
    /// The operation at this index inserts, renames, moves, removes, shows or hides a sheet,
    /// which [`ModelRoot::apply`] does not apply: [`ModelRoot::apply_structural`] does.
    SheetEdit { operation: usize },
    /// The skin layout edit at this index is refused, against the layout the batch's earlier
    /// operations leave.
    Layout {
        operation: usize,
        error: LayoutError,
    },
    /// The array formula the operation at this index authors has an input that is not a
    /// formula.
    ArrayInput { operation: usize },
    /// The array formula the operation at this index authors meets the area of the array formula
    /// anchored at `anchor`, which it does not replace (its anchor lies outside the area).
    ArrayOverlap { operation: usize, anchor: CellKey },
    /// [`ModelRoot::restored`] was given a root of another lineage.
    ForeignRoot,
    /// The operation at this index names a defined name the root does not hold.
    UnknownName { operation: usize },
    /// Retained records disagree with their scoped key or live order.
    InvalidNameRestoration { operation: usize },
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
            ApplyError::Layout { operation, error } => {
                write!(f, "operation {operation} edits the skin layout: {error}")
            }
            ApplyError::ArrayInput { operation } => write!(
                f,
                "operation {operation} authors an array formula whose input is not a formula"
            ),
            ApplyError::ArrayOverlap { operation, anchor } => write!(
                f,
                "operation {operation} authors an array formula that meets the array anchored at row {} column {}",
                anchor.row(),
                anchor.column()
            ),
            ApplyError::ForeignRoot => {
                f.write_str("the root to restore is not of this root's lineage")
            }
            ApplyError::InvalidNameRestoration { operation } => write!(
                f,
                "operation {operation} has inconsistent retained name records"
            ),
            ApplyError::UnknownName { operation } => write!(
                f,
                "operation {operation} names a defined name the root does not hold"
            ),
        }
    }
}

impl std::error::Error for ApplyError {}

/// One authored revision of a document: the revision, the sheets in order, each sheet's authored
/// cell inputs, and the document's [`SkinLayout`], which is document-only metadata that no
/// calculation reads. A root is immutable and shared behind an `Arc`; a change makes a new root. It maps each sheet's [`SheetKey`] to its [`SheetId`] and back, which is how wire
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
///
/// Each root made by [`ModelRoot::new`] or [`ModelRoot::with_cells`] starts a lineage of its own,
/// and every root [`ModelRoot::apply`] makes continues the lineage of the root it was applied to,
/// so [`ModelRoot::same_lineage`] tells a revision of a document from a root built apart from it,
/// such as another load of the same file. Equality compares the revision, the sheets, the cells
/// and the layout, not the lineage.
#[derive(Debug)]
pub struct ModelRoot {
    revision: AuthoredRevision,
    sheets: Vec<SheetEntry>,
    /// The cells of the sheet at the same position of `sheets`.
    cells: Vec<SheetPages>,
    layout: Arc<SkinLayout>,
    definitions: Arc<Definitions>,
    lineage: u64,
}

impl PartialEq for ModelRoot {
    fn eq(&self, other: &Self) -> bool {
        self.revision == other.revision
            && self.sheets == other.sheets
            && self.cells == other.cells
            && self.layout == other.layout
            && self.definitions == other.definitions
    }
}

impl Eq for ModelRoot {}

impl ModelRoot {
    /// Retain this exact authored root when the caller holds a borrowed view. Pages,
    /// definitions and layout stay shared; no revision or lineage is invented.
    pub fn pin(&self) -> Arc<Self> {
        Arc::new(Self {
            revision: self.revision,
            sheets: self.sheets.clone(),
            cells: self.cells.clone(),
            layout: self.layout.clone(),
            definitions: self.definitions.clone(),
            lineage: self.lineage,
        })
    }

    /// The same authored revision after a successful Save As. File identity is session
    /// context: it invalidates bindings in the host and is never undone with cell edits.
    pub fn with_file_identity(&self,name:&str,macro_enabled:bool)->Arc<Self> {
        Arc::new(Self {revision:self.revision,sheets:self.sheets.clone(),cells:self.cells.clone(),layout:self.layout.clone(),
            definitions:Arc::new(Definitions::clone(&self.definitions).with_workbook_name(Some(name.to_owned())).with_macro_enabled(macro_enabled)),lineage:self.lineage})
    }

    /// Physical source records at this root. Extent totals use wide arithmetic and never
    /// visit an empty coordinate. Requesting these statistics renders no member text.
    pub fn formula_source_stats(&self) -> crate::FormulaSourceStats {
        let mut stats = crate::FormulaSourceStats::default();
        let mut sources = BTreeSet::new();
        for sheet in &self.sheets {
            for (_, input) in self.sheet_cells(sheet.id).into_iter().flatten() {
                let CellInput::Formula(text) = input else {
                    continue;
                };
                stats.formula_cells += 1;
                if let Some(block) = text.block() {
                    stats.retained_members += 1;
                    stats.materialized_members += usize::from(text.materialized());
                    if sources.insert(Arc::as_ptr(block) as usize) {
                        stats.retained_sources += 1;
                        stats.membership_runs += match block.membership() {
                            crate::FormulaMembership::Dense => 1,
                            crate::FormulaMembership::Runs(runs) => runs.len(),
                        };
                        stats.extent_cells += u128::from(block.extent().cell_count());
                        stats.source_utf16_units +=
                            block.source().source().utf16_code_units().len() as u128;
                    }
                } else {
                    stats.inline_sources += 1;
                    stats.source_utf16_units += text.text().utf16_code_units().len() as u128;
                }
            }
        }
        stats
    }

    /// Replace only proven-equal formula storage, at the same revision and lineage. Values,
    /// dialect, ingress form, definitions and layout cannot change through this operation.
    pub fn with_formula_storage(
        &self,
        replacements: impl IntoIterator<Item = (CellKey, crate::FormulaText)>,
    ) -> Option<Arc<Self>> {
        let mut proposed = BTreeMap::new();
        let mut leaving = BTreeSet::new();
        let mut sources = BTreeMap::<usize, (Arc<crate::FormulaBlock>, usize)>::new();
        for (at, text) in replacements {
            let CellInput::Formula(held) = self.cell(at)? else {
                return None;
            };
            let block = text.block()?;
            if text.form() != crate::FormulaForm::Legacy
                || self.array_containing(at).is_some()
                || self.definitions.data_table_containing(at).is_some()
                || self
                    .definitions
                    .tables()
                    .iter()
                    .any(|table| table.area.contains(at))
                || text.member_at() != Some(at)
                || held.member_at().is_some_and(|member| member != at)
                || held.form() != text.form()
                || held.is_form_explicit() != text.is_form_explicit()
                || held.rendered() != block.source().render(at)
            {
                return None;
            }
            let key = Arc::as_ptr(block) as usize;
            sources.entry(key).or_insert_with(|| (block.clone(), 0)).1 += 1;
            if let Some(old) = held.block()
                && !Arc::ptr_eq(old, block)
            {
                leaving.insert(Arc::as_ptr(old) as usize);
            }
            if proposed.insert(at, text).is_some() {
                return None;
            }
        }
        if proposed.is_empty() {
            return Some(self.pin());
        }
        // Membership is an actual set, including after a physical replacement. Never
        // leave half of an old source claiming cells assigned to another source.
        for sheet in &self.sheets {
            for (at, input) in self.sheet_cells(sheet.id).into_iter().flatten() {
                let CellInput::Formula(text) = input else {
                    continue;
                };
                let Some(block) = text.block() else { continue };
                let key = Arc::as_ptr(block) as usize;
                if !proposed.contains_key(&at) {
                    if leaving.contains(&key) {
                        return None;
                    }
                    if let Some((_, members)) = sources.get_mut(&key) {
                        *members += 1;
                    }
                }
            }
        }
        if sources
            .values()
            .any(|(block, members)| block.member_count() != *members as u64)
        {
            return None;
        }
        let positions: BTreeMap<_, _> = self
            .sheets
            .iter()
            .enumerate()
            .map(|(i, sheet)| (sheet.id, i))
            .collect();
        let mut touched = Touched::new();
        for (at, text) in proposed {
            let position = *positions.get(&at.sheet())?;
            self.set_storage_into(&mut touched, position, at, CellInput::Formula(text));
        }
        Some(Arc::new(Self {
            revision: self.revision,
            sheets: self.sheets.clone(),
            cells: self.cells_after(touched),
            layout: self.layout.clone(),
            definitions: self.definitions.clone(),
            lineage: self.lineage,
        }))
    }

    /// The root of these sheets, in this order, with no cells and an empty layout, at `revision`;
    /// refused when two sheets share a key or an ID.
    pub fn new(
        revision: AuthoredRevision,
        sheets: Vec<SheetEntry>,
    ) -> Result<Arc<ModelRoot>, ModelRootError> {
        check_sheets(&sheets)?;
        let cells = vec![SheetPages::default(); sheets.len()];
        Ok(Arc::new(ModelRoot {
            revision,
            sheets,
            cells,
            layout: Arc::default(),
            definitions: Arc::default(),
            lineage: new_lineage(),
        }))
    }

    /// The root of these sheets, in this order, holding these cells' inputs, at `revision`: how a
    /// document's first revision is built from what a file holds. The cells come in any order; a
    /// later input for a cell replaces an earlier one. Each block of [`crate::PAGE_ROWS`] rows by
    /// [`crate::PAGE_COLUMNS`] columns that holds a cell gets one page, built once; a block without
    /// one gets no page, so the root's size follows the cells given, not the extent they span.
    ///
    /// Refused when two sheets share a key or an ID, or a cell lies on a sheet not among them. The
    /// root's layout is empty.
    pub fn with_cells(
        revision: AuthoredRevision,
        sheets: Vec<SheetEntry>,
        cells: impl IntoIterator<Item = (CellKey, CellInput)>,
    ) -> Result<Arc<ModelRoot>, ModelRootError> {
        Self::with_layout(revision, sheets, cells, SkinLayout::default())
    }

    /// [`ModelRoot::with_cells`] with a skin layout: how a document's first revision is built from
    /// a file that holds a view manifest. Refused as [`ModelRoot::with_cells`] refuses, and when a
    /// block's anchor lies on a sheet not among the sheets.
    pub fn with_layout(
        revision: AuthoredRevision,
        sheets: Vec<SheetEntry>,
        cells: impl IntoIterator<Item = (CellKey, CellInput)>,
        layout: SkinLayout,
    ) -> Result<Arc<ModelRoot>, ModelRootError> {
        check_sheets(&sheets)?;
        for block in layout.blocks() {
            if let Some(anchor) = block.anchor()
                && !sheets.iter().any(|sheet| sheet.id == anchor.sheet())
            {
                return Err(ModelRootError::BlockOnUnknownSheet(block.id()));
            }
        }
        let positions: BTreeMap<SheetId, usize> = sheets
            .iter()
            .enumerate()
            .map(|(position, sheet)| (sheet.id, position))
            .collect();
        // Each occupied block's cells, by (sheet position, page row, page column).
        let mut blocks: BTreeMap<(usize, u32, u32), BTreeMap<u16, CellInput>> = BTreeMap::new();
        for (at, input) in cells {
            let sheet = *positions
                .get(&at.sheet())
                .ok_or(ModelRootError::CellOnUnknownSheet(at))?;
            let (page_row, page_column, place) = locate(at);
            blocks
                .entry((sheet, page_row, page_column))
                .or_default()
                .insert(place, input);
        }
        let mut pages = vec![SheetPages::default(); sheets.len()];
        for ((sheet, page_row, page_column), cells) in blocks {
            let page = CellPage::build(page_row, page_column, cells).map(Arc::new);
            pages[sheet].set_page(page_row, page_column, page);
        }
        Ok(Arc::new(ModelRoot {
            revision,
            sheets,
            cells: pages,
            layout: Arc::new(layout),
            definitions: Arc::default(),
            lineage: new_lineage(),
        }))
    }

    /// [`ModelRoot::with_layout`] with the workbook's [`Definitions`]: its array formulas,
    /// defined names, tables and unloaded cells. Refused as [`ModelRoot::with_layout`] refuses,
    /// when a definition lies on a sheet not among the sheets, and when an array formula's anchor
    /// holds no formula among the cells.
    pub fn with_definitions(
        revision: AuthoredRevision,
        sheets: Vec<SheetEntry>,
        cells: impl IntoIterator<Item = (CellKey, CellInput)>,
        layout: SkinLayout,
        definitions: Definitions,
    ) -> Result<Arc<ModelRoot>, ModelRootError> {
        let root = Self::with_layout(revision, sheets, cells, layout)?;
        if let Some(sheet) = definitions
            .sheets()
            .find(|sheet| root.sheet_by_id(*sheet).is_none())
        {
            return Err(ModelRootError::DefinitionOnUnknownSheet(sheet));
        }
        if let Some(array) = definitions
            .arrays()
            .find(|array| !matches!(root.cell(array.anchor()), Some(CellInput::Formula(_))))
        {
            return Err(ModelRootError::ArrayWithoutFormula(array.anchor()));
        }
        let mut root = Arc::try_unwrap(root).expect("a root just built has one owner");
        root.definitions = Arc::new(definitions);
        Ok(Arc::new(root))
    }

    pub fn revision(&self) -> AuthoredRevision {
        self.revision
    }

    /// Whether `other` is of this root's lineage: one of them was made from the other by
    /// [`ModelRoot::apply`], directly or through other roots, or both from a third root.
    pub fn same_lineage(&self, other: &ModelRoot) -> bool {
        self.lineage == other.lineage
    }

    /// The next revision after this one, holding what `earlier` holds: its sheets, cells, layout
    /// and definitions, every page shared with it. This is how an edit no batch inverts (a
    /// structural edit, whose deletions drop cells and references) is undone: the revision before
    /// it comes back as a new revision. Refused when `earlier` is of another lineage or this
    /// revision is the last.
    pub fn restored(&self, earlier: &ModelRoot) -> Result<Arc<ModelRoot>, ApplyError> {
        if !self.same_lineage(earlier) {
            return Err(ApplyError::ForeignRoot);
        }
        let revision = self
            .revision
            .checked_next()
            .ok_or(ApplyError::RevisionExhausted)?;
        Ok(Arc::new(ModelRoot {
            revision,
            sheets: earlier.sheets.clone(),
            cells: earlier.cells.clone(),
            layout: Arc::clone(&earlier.layout),
            definitions: Arc::new(
                Definitions::clone(&earlier.definitions).with_name_table(
                    self.definitions
                        .name_table()
                        .restored_from(earlier.definitions.name_table()),
                ).with_workbook_name(self.definitions.workbook_name().map(str::to_owned)).with_macro_enabled(self.definitions.macro_enabled()),
            ),
            lineage: earlier.lineage,
        }))
    }

    /// An imported package's worksheets appended to this root, retaining its lineage and
    /// existing cell pages. The caller has reloaded the package using these live sheet IDs.
    pub fn with_imported_worksheets(&self, imported:&ModelRoot)->Result<Arc<Self>,ApplyError> {
        if imported.sheets.len() <= self.sheets.len() || imported.sheets[..self.sheets.len()] != self.sheets[..] {
            return Err(ApplyError::ForeignRoot);
        }
        let mut cells=self.cells.clone();
        cells.extend_from_slice(&imported.cells[self.cells.len()..]);
        let definitions=Definitions::clone(&imported.definitions)
            .with_name_table(self.definitions.name_table().restored_from(imported.definitions.name_table()))
            .with_workbook_name(self.definitions.workbook_name().map(str::to_owned))
            .with_macro_enabled(self.definitions.macro_enabled());
        Ok(Arc::new(Self{revision:self.revision,sheets:imported.sheets.clone(),cells,
            layout:self.layout.clone(),definitions:Arc::new(definitions),lineage:self.lineage}))
    }

    /// The sheets in workbook order.
    pub fn sheets(&self) -> &[SheetEntry] {
        &self.sheets
    }

    /// The skin layout: the blocks and their placements in each view. A root made by
    /// [`ModelRoot::apply`] from a batch that leaves the layout as it was shares it with the root
    /// it was applied to, by pointer.
    pub fn layout(&self) -> &Arc<SkinLayout> {
        &self.layout
    }

    /// The workbook's array formulas, defined names, tables and unloaded cells. A root made by
    /// [`ModelRoot::apply`] shares them with the root it was applied to, by pointer, unless the
    /// batch set or cleared an array formula's anchor or an unloaded cell.
    pub fn definitions(&self) -> &Arc<Definitions> {
        &self.definitions
    }

    /// The array formula whose area holds `at`, its anchor included.
    pub fn array_containing(&self, at: CellKey) -> Option<&ArrayFormula> {
        self.definitions.array_containing(at)
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
    /// cell of its area. A [`WorkbookOp::EditSkinLayout`] edits the layout, against the layout the
    /// earlier operations leave; it changes no cell, and a block it creates is anchored on a sheet
    /// the root holds.
    ///
    /// The new root holds a new page for each block whose cells the batch changed, and no page
    /// for a block it left empty; each page is built once per batch, however many operations
    /// touch it. Every other page, a page whose cells the batch set to what they already held
    /// included, is the old root's allocation, and so is every index node off the changed pages'
    /// paths. This root is unchanged; the caller decides whether the new root replaces it.
    ///
    /// Refused, with nothing made, when the batch is based on another revision, the revision is
    /// the last one, an operation names a sheet the root does not hold, the batch holds a
    /// structural edit ([`ApplyError::AxisEdit`], [`ApplyError::SheetEdit`]), or a layout edit is
    /// refused ([`ApplyError::Layout`]).
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
        let mut touched: Touched = BTreeMap::new();
        // Cell edits prune the actual source members without expanding every survivor's text.
        // Key by the shared allocation: separately supplied blocks with the same local ID must
        // not replace each other's source. This is storage identity, never a semantic key.
        let mut affected = BTreeMap::new();
        for op in &batch.operations {
            let area = match op {
                WorkbookOp::SetCell { at, .. } => Some(GridArea::cell(*at)),
                WorkbookOp::Clear { area } => Some(*area),
                WorkbookOp::SetArray { array, .. } => Some(array.area()),
                _ => None,
            };
            if let Some(area) = area {
                for (at, input) in self.cells(area).into_iter().flatten() {
                    if let CellInput::Formula(text) = input
                        && let Some(block) = text.block()
                    {
                        let entry = affected
                            .entry(Arc::as_ptr(block) as usize)
                            .or_insert_with(|| (block.clone(), std::collections::BTreeSet::new()));
                        entry.1.insert(at);
                    }
                }
            }
        }
        if !affected.is_empty() {
            let mut survivors = BTreeMap::<usize, Vec<(usize, CellKey)>>::new();
            for (position, sheet) in self.sheets.iter().enumerate() {
                for (at, input) in self.sheet_cells(sheet.id).into_iter().flatten() {
                    if let CellInput::Formula(text) = input
                        && let Some(block) = text.block()
                    {
                        let key = Arc::as_ptr(block) as usize;
                        if let Some((_, removed)) = affected.get(&key)
                            && !removed.contains(&at)
                        {
                            survivors.entry(key).or_default().push((position, at));
                        }
                    }
                }
            }
            for (key, members) in survivors {
                let retained = affected[&key]
                    .0
                    .retaining(members.iter().map(|(_, at)| *at));
                for (position, at) in members {
                    if let Some(block) = &retained {
                        let text = crate::FormulaText::member(block.clone(), at)
                            .expect("retaining validated every member");
                        self.set_storage_into(&mut touched, position, at, CellInput::Formula(text));
                    } else if let Some(input) = self.cell(at) {
                        // A caller-supplied block can disagree with its cell placement. Keep
                        // the existing individual-text fallback rather than alter its formula.
                        self.set_into(&mut touched, position, at, input);
                    }
                }
            }
        }
        // A working copy of the layout, made from this root's the first time an operation edits it.
        let mut layout: Option<SkinLayout> = None;
        // The areas the batch's cell operations set or cleared.
        let mut edited: Vec<GridArea> = Vec::new();
        // The array formulas the batch authors, with the operation that authored each, less those
        // a later operation of the batch set or cleared the anchor of.
        let mut added: Vec<(usize, ArrayFormula)> = Vec::new();
        // The cell formats the batch sets, in order.
        let mut formats: Vec<(CellKey, Option<u32>)> = Vec::new();
        let mut styles = None;
        let mut tables = None;
        let mut filters = Vec::new();
        let mut conditional = Vec::new();
        let mut review = Vec::new();
        let mut links = Vec::new();
        let mut charts = Vec::new();
        let mut validations = Vec::new();
        let mut panes = Vec::new();
        let mut axis_formats = Vec::new();
        // The defined names' formulas the batch sets, as the names will read.
        let mut names: Option<crate::NameTable> = None;
        let mut iteration: Option<crate::definitions::IterationSettings> = None;
        for (operation, op) in batch.operations.iter().enumerate() {
            match op {
                WorkbookOp::SetCharts {sheet,charts:values}=>{self.position_of(operation,*sheet)?;charts.push((*sheet,values.clone()));}
                WorkbookOp::SetStyles(value) => styles = Some(Arc::clone(value)),
                WorkbookOp::SetTables(value)=>{for t in value{self.position_of(operation,t.sheet)?;}tables=Some(value.clone());},
                WorkbookOp::SetValidations { sheet, rules } => {
                    self.position_of(operation, *sheet)?;
                    validations.push((*sheet, rules.clone()));
                }
                WorkbookOp::SetReviewLinks { sheet, links:values } => {
                    self.position_of(operation,*sheet)?;
                    if values.iter().any(|l|l.area.sheet()!=*sheet){return Err(ApplyError::AxisEdit{operation});}
                    links.push((*sheet,values.clone()));
                }
                WorkbookOp::SetReviewNotes { sheet, notes } => {
                    self.position_of(operation, *sheet)?;
                    if notes.iter().any(|n| n.at.sheet() != *sheet) {
                        return Err(ApplyError::AxisEdit { operation });
                    }
                    review.push((*sheet, notes.clone()));
                }
                WorkbookOp::SetConditionalRules { sheet, rules } => {
                    self.position_of(operation, *sheet)?;
                    conditional.push((*sheet, rules.clone()));
                }
                WorkbookOp::SetCell { at, input } => {
                    edited.push(GridArea::cell(*at));
                    added.retain(|(_, array)| array.anchor() != *at);
                    let sheet = self.position_of(operation, at.sheet())?;
                    self.set_into(&mut touched, sheet, *at, input);
                }
                WorkbookOp::Clear { area } => {
                    edited.push(*area);
                    added.retain(|(_, array)| !area.contains(array.anchor()));
                    let sheet = self.position_of(operation, area.sheet())?;
                    self.clear_into(&mut touched, sheet, *area);
                }
                WorkbookOp::SetArray { array, input } => {
                    if !matches!(input, CellInput::Formula(_)) {
                        return Err(ApplyError::ArrayInput { operation });
                    }
                    let area = array.area();
                    edited.push(area);
                    added.retain(|(_, earlier)| !meets(earlier.area(), area));
                    let sheet = self.position_of(operation, area.sheet())?;
                    self.clear_into(&mut touched, sheet, area);
                    self.set_into(&mut touched, sheet, array.anchor(), input);
                    added.push((operation, *array));
                }
                WorkbookOp::SetFormat { at, format } => {
                    self.position_of(operation, at.sheet())?;
                    formats.push((*at, *format));
                }
                WorkbookOp::SetNameFormula {
                    name,
                    scope,
                    formula,
                } => {
                    if let crate::NameScope::Sheet(sheet) = scope {
                        self.position_of(operation, *sheet)?;
                    }
                    let all = names.get_or_insert_with(|| self.definitions.name_table().clone());
                    if formula.is_none() && all.lookup(*scope, name).is_none() {
                        return Err(ApplyError::UnknownName { operation });
                    }
                    all.set_formula(*scope, name, formula.as_deref());
                }
                WorkbookOp::RestoreNames(snapshots) => {
                    let mut keys = std::collections::HashSet::new();
                    let mut origins = std::collections::HashSet::new();
                    let mut positions = std::collections::HashSet::new();
                    for snapshot in snapshots {
                        if let crate::NameScope::Sheet(sheet) = snapshot.key.scope {
                            self.position_of(operation, sheet)?;
                        }
                        let mut previous = None;
                        let positioned = snapshot
                            .records
                            .first()
                            .is_some_and(|record| record.position.is_some());
                        if !keys.insert(&snapshot.key)
                            || calc_value::NameKey::new(snapshot.key.scope, &snapshot.key.folded)
                                != snapshot.key
                        {
                            return Err(ApplyError::InvalidNameRestoration { operation });
                        }
                        for record in &snapshot.records {
                            if !origins.insert(record.origin)
                                || calc_value::NameKey::new(
                                    record.definition.scope,
                                    &record.definition.name,
                                ) != snapshot.key
                                || record.position.is_some() != positioned
                            {
                                return Err(ApplyError::InvalidNameRestoration { operation });
                            }
                            if let Some(position) = record.position {
                                if previous.is_some_and(|before| before >= position)
                                    || !positions.insert(position)
                                {
                                    return Err(ApplyError::InvalidNameRestoration { operation });
                                }
                                previous = Some(position);
                            }
                        }
                    }
                    names
                        .get_or_insert_with(|| self.definitions.name_table().clone())
                        .restore_records(snapshots);
                }
                WorkbookOp::SetIteration(settings) => iteration = Some(*settings),
                WorkbookOp::SetAutoFilter { sheet, filter } => {
                    self.position_of(operation, *sheet)?;
                    if filter.as_ref().is_some_and(|f| f.area.sheet()!=*sheet || !f.valid()) {
                        return Err(ApplyError::AxisEdit { operation });
                    }
                    filters.push((*sheet,filter.clone()));
                }
                WorkbookOp::SetAxisFormat(edit) => {
                    self.position_of(operation, edit.sheet())?;
                    axis_formats.push(*edit);
                }
                WorkbookOp::SetFrozenPanes {
                    sheet,
                    panes: value,
                } => {
                    self.position_of(operation, *sheet)?;
                    panes.push((*sheet, value.map(crate::PaneLayout::Frozen)));
                }
                WorkbookOp::SetPaneLayout {sheet,layout}=>{self.position_of(operation,*sheet)?;panes.push((*sheet,*layout));}
                WorkbookOp::EditAxis(_) => return Err(ApplyError::AxisEdit { operation }),
                WorkbookOp::EditSheet(_) => return Err(ApplyError::SheetEdit { operation }),
                WorkbookOp::EditSkinLayout(edit) => {
                    if let SkinLayoutEdit::CreateBlock { anchor, .. } = edit {
                        self.position_of(operation, anchor.sheet())?;
                    }
                    layout
                        .get_or_insert_with(|| SkinLayout::clone(&self.layout))
                        .edit(edit)
                        .map_err(|error| ApplyError::Layout { operation, error })?;
                }
            }
        }

        let cells = self.cells_after(touched);
        let layout = match layout {
            Some(layout) if layout != *self.layout => Arc::new(layout),
            _ => Arc::clone(&self.layout),
        };
        // An authored array must not meet an array the batch keeps.
        let touched_anchor = |at: CellKey| edited.iter().any(|area| area.contains(at));
        for (operation, array) in &added {
            if let Some(kept) = self
                .definitions
                .arrays_meeting(array.area())
                .find(|kept| !touched_anchor(kept.anchor()))
            {
                return Err(ApplyError::ArrayOverlap {
                    operation: *operation,
                    anchor: kept.anchor(),
                });
            }
        }
        let added: Vec<ArrayFormula> = added.into_iter().map(|(_, array)| array).collect();
        let mut definitions = match self.definitions.after_edits(touched_anchor, &added) {
            Some(definitions) => Arc::new(definitions),
            None => Arc::clone(&self.definitions),
        };
        if tables.is_some() || styles.is_some()
            || !conditional.is_empty()
            || !review.is_empty()
            || !links.is_empty()
            || !charts.is_empty()
            || !validations.is_empty()
            || !filters.is_empty()
            || !formats.is_empty()
            || !panes.is_empty()
            || !axis_formats.is_empty()
            || names.is_some()
            || iteration.is_some_and(|settings| settings != definitions.iteration())
        {
            let mut next = Definitions::clone(&definitions);
            if let Some(tables)=tables {next=next.with_tables(tables);}
        if let Some(styles) = styles {
                next = next.with_styles(styles);
            }
            for (sheet, filter) in filters { next = next.with_auto_filter(sheet, filter);
            }
            for (sheet, rules) in validations { next = next.with_validations(sheet, rules); }
            for (sheet, values) in charts {next=next.with_charts(sheet,values);}
            for (sheet, values) in links {next=next.with_review_links(sheet,values);}
            for (sheet, notes) in review {
                next = next.with_review_notes(sheet, notes); }
            for (sheet, rules) in conditional {
                next = next.with_conditional_rules(sheet, rules);
            }
            next = next.with_axis_formats(axis_formats);
            for (sheet, panes) in panes {
                next = next.with_pane_layout(sheet, panes);
            }
            if let Some(settings) = iteration {
                next = next.with_iteration(settings);
            }
            for (at, format) in formats {
                next = next.with_format_of(at, format);
            }
            if let Some(names) = names {
                next = next.with_name_table(names);
            }
            definitions = Arc::new(next);
        }
        Ok(Arc::new(ModelRoot {
            revision,
            sheets: self.sheets.clone(),
            cells,
            layout,
            definitions,
            lineage: self.lineage,
        }))
    }

    /// The root's contents taken apart: its sheets, every cell of each, its layout and its
    /// definitions (L3's structural edits work on them).
    pub(crate) fn parts(&self) -> crate::structural::Parts {
        let cells = self
            .sheets
            .iter()
            .map(|sheet| {
                let held = self
                    .sheet_cells(sheet.id)
                    .into_iter()
                    .flatten()
                    .map(|(at, input)| (at, input.clone()))
                    .collect();
                (sheet.id, held)
            })
            .collect();
        crate::structural::Parts {
            sheets: self.sheets.clone(),
            cells,
            layout: SkinLayout::clone(&self.layout),
            definitions: Definitions::clone(&self.definitions),
        }
    }

    /// The root of `parts` at `revision`, of this root's lineage. Each sheet's pages are built
    /// again from its cells.
    pub(crate) fn rebuilt(
        &self,
        parts: crate::structural::Parts,
        revision: AuthoredRevision,
    ) -> Arc<ModelRoot> {
        let mut pages = vec![SheetPages::default(); parts.sheets.len()];
        for (position, sheet) in parts.sheets.iter().enumerate() {
            let mut blocks: BTreeMap<(u32, u32), BTreeMap<u16, CellInput>> = BTreeMap::new();
            for (at, input) in parts.cells.get(&sheet.id).into_iter().flatten() {
                let (page_row, page_column, place) = locate(*at);
                blocks.entry((page_row, page_column)).or_default().insert(
                    place,
                    match input {
                        CellInput::Formula(text)
                            if text.member_at().is_some_and(|member| member != *at) =>
                        {
                            CellInput::Formula(text.detached())
                        }
                        _ => input.clone(),
                    },
                );
            }
            for ((page_row, page_column), cells) in blocks {
                let page = CellPage::build(page_row, page_column, cells).map(Arc::new);
                pages[position].set_page(page_row, page_column, page);
            }
        }
        Arc::new(ModelRoot {
            revision,
            sheets: parts.sheets,
            cells: pages,
            layout: Arc::new(parts.layout),
            definitions: Arc::new(parts.definitions),
            lineage: self.lineage,
        })
    }

    fn cells_after(&self, touched: Touched) -> Vec<SheetPages> {
        let mut cells = self.cells.clone();
        for ((sheet, page_row, page_column), working) in touched {
            let old = self.cells[sheet].page(page_row, page_column);
            let new = CellPage::build(page_row, page_column, working);
            match (old, new) {
                (Some(old), Some(new)) if old.same_storage(&new) => {}
                (None, None) => {}
                (_, new) => cells[sheet].set_page(page_row, page_column, new.map(Arc::new)),
            }
        }
        cells
    }

    /// Sets `at` (on the sheet at position `sheet`) to `input` in the working copies `touched`.
    fn set_into(&self, touched: &mut Touched, sheet: usize, at: CellKey, input: &CellInput) {
        let input = match input {
            CellInput::Formula(text) if text.block().is_some() => {
                CellInput::Formula(text.detached())
            }
            _ => input.clone(),
        };
        self.set_storage_into(touched, sheet, at, input);
    }

    fn set_storage_into(&self, touched: &mut Touched, sheet: usize, at: CellKey, input: CellInput) {
        let (page_row, page_column, place) = locate(at);
        touched
            .entry((sheet, page_row, page_column))
            .or_insert_with(|| self.working_copy(sheet, page_row, page_column))
            .insert(place, input);
    }

    /// Empties every cell of `area` (on the sheet at position `sheet`) in the working copies
    /// `touched`.
    fn clear_into(&self, touched: &mut Touched, sheet: usize, area: GridArea) {
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
                    let (row, column) = CellPage::position(*page_row, *page_column, *place);
                    !bounds.contains(row, column)
                });
            }
        }
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

/// The working copies of the pages a batch touches, by (sheet position, page row, page column).
type Touched = BTreeMap<(usize, u32, u32), BTreeMap<u16, CellInput>>;

/// Whether two areas share a cell.
fn meets(a: GridArea, b: GridArea) -> bool {
    a.sheet() == b.sheet()
        && a.first().row() <= b.last().row()
        && b.first().row() <= a.last().row()
        && a.first().column() <= b.last().column()
        && b.first().column() <= a.last().column()
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
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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

    /// A root built with its cells holds the same pages as one made by applying them to an empty
    /// root, at the revision it was given; a later input for a cell replaces an earlier one, and
    /// a cell on a sheet the root does not hold is refused.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn with_cells_matches_apply() {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let sheets: Vec<SheetId> = (0..3).map(|_| ids.issue().expect("an ID")).collect();
        let entries: Vec<SheetEntry> = sheets[..2]
            .iter()
            .zip(1..)
            .map(|(id, key)| {
                let name =
                    SheetName::new(ExcelText::from_utf16_code_units(vec![0x40 + key as u16]))
                        .expect("a name");
                SheetEntry::new(*id, SheetKey::new(key), name, SheetVisibility::Visible)
            })
            .collect();
        let mut rng = Rng(0x2545_F491_4F6C_DD1D);
        let mut cells = Vec::new();
        for counter in 0..2000 {
            let at = CellKey::new(
                sheets[rng.below(2) as usize],
                row(&mut rng),
                column(&mut rng),
            )
            .unwrap();
            cells.push((
                at,
                CellInput::Number(FiniteNumber::new(f64::from(counter)).unwrap()),
            ));
        }
        let built = ModelRoot::with_cells(AuthoredRevision::new(7), entries.clone(), cells.clone())
            .expect("cells on held sheets");
        let applied = ModelRoot::new(AuthoredRevision::new(6), entries.clone())
            .unwrap()
            .apply(&OpBatch {
                base: AuthoredRevision::new(6),
                operations: cells
                    .iter()
                    .map(|(at, input)| WorkbookOp::SetCell {
                        at: *at,
                        input: input.clone(),
                    })
                    .collect(),
            })
            .unwrap();
        assert_eq!(*built, *applied, "the same revision, sheets and pages");
        assert!(!built.same_lineage(&applied), "two roots built apart");
        let next = applied
            .apply(&OpBatch {
                base: applied.revision(),
                operations: vec![WorkbookOp::Clear {
                    area: GridArea::full_sheet(sheets[0]),
                }],
            })
            .unwrap();
        assert!(
            next.same_lineage(&applied),
            "a root and the root applied to it"
        );
        assert!(!next.same_lineage(&built));
        for sheet in &sheets[..2] {
            assert!(
                built
                    .pages(*sheet)
                    .unwrap()
                    .all(|page| page.cell_count() > 0)
            );
        }
        let elsewhere = CellKey::new(sheets[2], 1, 1).unwrap();
        assert_eq!(
            ModelRoot::with_cells(
                AuthoredRevision::INITIAL,
                entries,
                [(elsewhere, CellInput::Logical(true))]
            ),
            Err(ModelRootError::CellOnUnknownSheet(elsewhere))
        );
    }
}
