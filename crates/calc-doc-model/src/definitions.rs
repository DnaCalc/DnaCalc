//! What a workbook defines beside its cells' inputs that a calculation reads: array formulas,
//! defined names and tables, and the cells the model holds no input for because their content is
//! not modelled ([`UnloadedCell`]).
//!
//! These are authored facts as the file gave them. None of them holds a calculated value: an
//! array formula's anchor holds its formula text as the cell's [`crate::CellInput`], and the cells
//! of its area that it fills are not inputs at all; what a file cached for them is kept beside the
//! model by the codec, as file caches.

use calc_value::{CellKey, GridArea, SheetId};
use std::collections::BTreeMap;

/// How a file stored an array formula.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArrayKind {
    /// A legacy array formula (entered with Ctrl+Shift+Enter): `<f t="array" ref="..">` without
    /// dynamic-array cell metadata. Its area is fixed: every cell of it shows one element of the
    /// result.
    Legacy,
    /// A dynamic-array formula: `<f t="array" ref="..">` on a cell whose `cm` names dynamic-array
    /// properties (`XLDAPR`, `fDynamic="1"`). Its area is the extent the result spilled into when
    /// the file was saved; `collapsed` is the properties' `fCollapsed`.
    Dynamic { collapsed: bool },
}

/// An array formula: its anchor cell holds the formula text, and it fills `area`, whose top-left
/// cell is the anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ArrayFormula {
    area: GridArea,
    kind: ArrayKind,
}

impl ArrayFormula {
    pub fn new(area: GridArea, kind: ArrayKind) -> Self {
        ArrayFormula { area, kind }
    }

    /// The cells the formula fills, its anchor first.
    pub fn area(&self) -> GridArea {
        self.area
    }

    /// The cell that holds the formula text.
    pub fn anchor(&self) -> CellKey {
        self.area.first()
    }

    pub fn kind(&self) -> ArrayKind {
        self.kind
    }
}

/// Where a defined name is visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NameScope {
    /// Every sheet of the workbook.
    Workbook,
    /// One sheet (the file's `localSheetId`).
    Sheet(SheetId),
}

/// A defined name as the workbook part declares it: its name, scope, the formula text it stands
/// for (without a leading `=`, with the file's `_xlfn.` prefixes as written), and the attributes
/// the file gave it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DefinedName {
    pub name: String,
    pub scope: NameScope,
    pub formula: String,
    pub hidden: bool,
    pub comment: Option<String>,
    /// `function`: the name is a macro function name.
    pub function: bool,
    /// `vbProcedure`: the name refers to a VBA procedure.
    pub vb_procedure: bool,
    /// `xlm`: the name refers to an Excel 4.0 macro.
    pub xlm: bool,
}

/// A table (ListObject) of a sheet, as its table part declares it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Table {
    /// The table's `name`.
    pub name: String,
    /// The table's `displayName`, the name structured references use.
    pub display_name: String,
    /// The table part the table was read from.
    pub part: String,
    pub sheet: SheetId,
    /// The table's `ref`: its header row, data rows and totals row.
    pub area: GridArea,
    /// `headerRowCount`: 1 unless the table has no header row.
    pub header_rows: u32,
    /// `totalsRowCount`: 0 unless the table shows a totals row.
    pub totals_rows: u32,
    /// The columns' names, left to right.
    pub columns: Vec<String>,
}

/// Why the model holds no input for a cell that the file gave content. A calculation reads such a
/// cell as not available, never as blank; the codec keeps the cell's bytes for a save.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum UnloadedCell {
    /// A value cell whose value is a rich value (a linked data type, an image in a cell): the
    /// file's `vm` names a rich value of this structure type (`_linkedEntity`, `_webimage`, ...),
    /// which the model does not represent. Owner: D1 (dc-ndi).
    RichValue { structure: String },
    /// A value cell whose `vm` could not be followed to a rich value: the reason names the broken
    /// link. Owner: D1 (dc-ndi).
    UnresolvedValueMetadata { reason: String },
    /// A cell of a what-if data table (`<f t="dataTable">`). Owner: L3.
    DataTable,
}

/// Why a row is hidden.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RowHidden {
    /// Hidden by hand: SUBTOTAL's function numbers 101 to 111 and AGGREGATE's options leave it
    /// out.
    Manual,
    /// Hidden by a filter (a row of a sheet's or a table's filter range): every SUBTOTAL function
    /// number leaves it out.
    Filtered,
}

/// A workbook's [`ArrayFormula`]s, [`DefinedName`]s, [`Table`]s and [`UnloadedCell`]s, and its
/// hidden rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Definitions {
    arrays: BTreeMap<CellKey, ArrayFormula>,
    names: Vec<DefinedName>,
    tables: Vec<Table>,
    unloaded: BTreeMap<CellKey, UnloadedCell>,
    hidden_rows: BTreeMap<(SheetId, u32), RowHidden>,
    /// The workbook is macro-enabled (`.xlsm`, `.xltm`), so a VBA project may define functions.
    macro_enabled: bool,
}

impl Definitions {
    pub fn new(
        arrays: impl IntoIterator<Item = ArrayFormula>,
        names: Vec<DefinedName>,
        tables: Vec<Table>,
        unloaded: impl IntoIterator<Item = (CellKey, UnloadedCell)>,
    ) -> Self {
        Definitions {
            arrays: arrays
                .into_iter()
                .map(|array| (array.anchor(), array))
                .collect(),
            names,
            tables,
            unloaded: unloaded.into_iter().collect(),
            hidden_rows: BTreeMap::new(),
            macro_enabled: false,
        }
    }

    /// These definitions of a macro-enabled workbook (`.xlsm`, `.xltm`) when `macro_enabled`: a
    /// VBA project may then define functions, so a call of a name no function or defined name
    /// holds is not known to be #NAME?.
    pub fn with_macro_enabled(mut self, macro_enabled: bool) -> Self {
        self.macro_enabled = macro_enabled;
        self
    }

    /// Whether the workbook is macro-enabled ([`Definitions::with_macro_enabled`]).
    pub fn macro_enabled(&self) -> bool {
        self.macro_enabled
    }

    /// These definitions with `rows` hidden: each a sheet, a row (1-based) and why it is hidden.
    /// A row not listed is visible. The rows are authored facts, read from the file's `<row
    /// hidden="1">`; structural edits do not move them yet (L3, dc-m3u).
    pub fn with_hidden_rows(
        mut self,
        rows: impl IntoIterator<Item = (SheetId, u32, RowHidden)>,
    ) -> Self {
        self.hidden_rows = rows
            .into_iter()
            .map(|(sheet, row, why)| ((sheet, row), why))
            .collect();
        self
    }

    /// The hidden rows of `sheet` from `first` to `last` (both included), in order, with why.
    pub fn hidden_rows(
        &self,
        sheet: SheetId,
        first: u32,
        last: u32,
    ) -> impl Iterator<Item = (u32, RowHidden)> + '_ {
        self.hidden_rows
            .range((sheet, first)..=(sheet, last))
            .map(|((_, row), why)| (*row, *why))
    }

    /// Whether nothing is defined.
    pub fn is_empty(&self) -> bool {
        self.arrays.is_empty()
            && self.names.is_empty()
            && self.tables.is_empty()
            && self.unloaded.is_empty()
            && self.hidden_rows.is_empty()
    }

    /// Every array formula, by anchor, in cell order.
    pub fn arrays(&self) -> impl Iterator<Item = &ArrayFormula> {
        self.arrays.values()
    }

    /// The array formula anchored at `anchor`.
    pub fn array(&self, anchor: CellKey) -> Option<&ArrayFormula> {
        self.arrays.get(&anchor)
    }

    /// The array formula whose area holds `at`, its anchor included.
    pub fn array_containing(&self, at: CellKey) -> Option<&ArrayFormula> {
        let first = CellKey::new(at.sheet(), 1, 1).expect("A1 is in the grid");
        self.arrays
            .range(first..=at)
            .rev()
            .map(|(_, array)| array)
            .find(|array| array.area.contains(at))
    }

    /// The array formulas whose areas meet `area`.
    pub fn arrays_meeting(&self, area: GridArea) -> impl Iterator<Item = &ArrayFormula> {
        let (first, last) = (area.first(), area.last());
        let start = CellKey::new(area.sheet(), 1, 1).expect("A1 is in the grid");
        self.arrays
            .range(start..=last)
            .map(|(_, array)| array)
            .filter(move |array| {
                let (a, b) = (array.area.first(), array.area.last());
                a.row() <= last.row()
                    && b.row() >= first.row()
                    && a.column() <= last.column()
                    && b.column() >= first.column()
            })
    }

    pub fn names(&self) -> &[DefinedName] {
        &self.names
    }

    pub fn tables(&self) -> &[Table] {
        &self.tables
    }

    /// Every cell the model holds no input for, with why, in cell order.
    pub fn unloaded_cells(&self) -> impl Iterator<Item = (CellKey, &UnloadedCell)> {
        self.unloaded.iter().map(|(at, why)| (*at, why))
    }

    /// The unloaded cells of `area`, in cell order.
    pub fn unloaded_in(&self, area: GridArea) -> impl Iterator<Item = (CellKey, &UnloadedCell)> {
        self.unloaded
            .range(area.first()..=area.last())
            .filter(move |(at, _)| area.contains(**at))
            .map(|(at, why)| (*at, why))
    }

    pub fn unloaded(&self, at: CellKey) -> Option<&UnloadedCell> {
        self.unloaded.get(&at)
    }

    /// Whether a calculation must read `at` as not available: a cell an array formula fills
    /// other than its anchor, or an unloaded cell. `has_input` says whether the model holds an
    /// input for the cell, which a calculation reads instead.
    pub fn not_available(&self, at: CellKey, has_input: bool) -> bool {
        if has_input {
            return false;
        }
        self.unloaded.contains_key(&at)
            || self
                .array_containing(at)
                .is_some_and(|array| array.anchor() != at)
    }

    pub(crate) fn sheets(&self) -> impl Iterator<Item = SheetId> + '_ {
        self.arrays
            .keys()
            .map(|at| at.sheet())
            .chain(self.unloaded.keys().map(|at| at.sheet()))
            .chain(self.tables.iter().map(|table| table.sheet))
            .chain(self.hidden_rows.keys().map(|(sheet, _)| *sheet))
            .chain(self.names.iter().filter_map(|name| match name.scope {
                NameScope::Sheet(sheet) => Some(sheet),
                NameScope::Workbook => None,
            }))
    }

    /// These definitions after cell edits that set or cleared the cells for which `touched`
    /// answers true: an array formula whose anchor was touched is gone (the anchor now holds what
    /// the edit put there, and the area it filled is free), and so is an unloaded cell that was
    /// touched. `None` when nothing changed.
    pub(crate) fn after_edits(&self, touched: impl Fn(CellKey) -> bool) -> Option<Definitions> {
        let arrays_gone = self.arrays.keys().any(|at| touched(*at));
        let unloaded_gone = self.unloaded.keys().any(|at| touched(*at));
        if !arrays_gone && !unloaded_gone {
            return None;
        }
        let mut next = self.clone();
        next.arrays.retain(|at, _| !touched(*at));
        next.unloaded.retain(|at, _| !touched(*at));
        Some(next)
    }
}
