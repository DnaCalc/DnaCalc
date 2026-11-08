//! The authored operations admitted so far, their structural-edit descriptors (the skin layout's
//! is in `layout.rs`), and the batch that carries them. An operation names authored document
//! state only: it holds no engine type, no session operation and no calculated value. An
//! operation on an existing sheet names it by its
//! [`SheetId`], which travels on the wire as that sheet's [`SheetKey`]; an insert names the sheet
//! it adds by a new [`SheetKey`] alone.

use crate::definitions::ArrayFormula;
use crate::input::CellInput;
use crate::layout::SkinLayoutEdit;
use crate::revision::AuthoredRevision;
use crate::sheet::{SheetKey, SheetName, SheetVisibility};
use calc_value::{CellKey, GridArea, MAX_COLUMNS, MAX_ROWS, SheetId};
use std::fmt;
use std::num::NonZeroU32;

/// One complete imported record retained by name history, with optional live order.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RetainedName {
    pub origin: crate::NameOrigin,
    pub position: Option<usize>,
    pub definition: crate::DefinedName,
}
/// All original records for one scoped key, including imported duplicate fallbacks.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NameRestoration {
    pub key: calc_value::NameKey,
    pub records: Vec<RetainedName>,
}

/// One authored change to a workbook.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum WorkbookOp {
    /// The immutable style table, committed atomically with cell style assignments.
    SetStyles(std::sync::Arc<crate::StyleTable>),
    /// Criteria and filtered visibility committed together; manual hiding remains independent.
    SetAutoFilter { sheet: SheetId, filter: Option<crate::AutoFilter>,
    },
    SetTables(Vec<crate::Table>),
    SetPageSetup { sheet: SheetId, setup: crate::PageSetup },
    SetCharts {sheet:SheetId,charts:Vec<crate::Chart>},
    SetPivots(std::sync::Arc<crate::pivot::PivotState>),
    /// Replace the data model snapshot after an explicit refresh.
    SetDataModel(std::sync::Arc<crate::DataModel>),
    SetValidations { sheet: SheetId, rules: Vec<crate::Validation> },
    SetReviewLinks { sheet:SheetId, links:Vec<crate::ReviewLink> },
    SetReviewNotes {
        sheet: SheetId,
        notes: Vec<crate::ReviewNote> },
    /// Replaces a worksheet's complete conditional-rule state, retaining source fields.
    SetConditionalRules {
        sheet: SheetId,
        rules: Vec<crate::ConditionalRuleSource>,
    },
    /// Replace the authored input of one cell.
    SetCell { at: CellKey, input: CellInput },
    /// Remove the authored contents of every cell of an area.
    Clear { area: GridArea },
    /// Author an array formula: the rest of `array`'s area is emptied, its anchor (the area's
    /// first cell) holds `input`, which must be a formula ([`crate::ApplyError::ArrayInput`]),
    /// and the array fills the area: a legacy array's fixed area, or the extent a dynamic array's
    /// result last spilled into (the engine spills a dynamic array over its result's own shape).
    /// An array formula whose anchor lies in the area is replaced; one that meets the area
    /// without its anchor in it is refused ([`crate::ApplyError::ArrayOverlap`]).
    SetArray {
        array: ArrayFormula,
        input: CellInput,
    },
    /// Give one cell a cell format: an index into the workbook's cell formats (a cell's `s`), or
    /// `None` to return it to the format the file gave it (dc-09v). It changes no input and no
    /// calculation.
    SetFormat { at: CellKey, format: Option<u32> },
    /// Give the defined name `name` of `scope` (matched under the full Unicode case fold, as
    /// Excel matches names: dc-cpr, calc-value's `workbook_names_equal`) the formula `formula`,
    /// as the model holds a name's formula, defining the name when the root holds none of that
    /// scope; `None` removes it (dc-gf0: a cut moves the cells a name refers to, and a formula
    /// moved to another sheet takes the sheet-scoped names it uses with it). Removing a name the
    /// root does not hold is refused ([`crate::ApplyError::UnknownName`]). The model changes no
    /// cell for it: a formula that uses the name reads the new formula when it is bound again.
    SetNameFormula {
        name: String,
        scope: crate::definitions::NameScope,
        formula: Option<String>,
    },
    /// Restore retained scoped name groups through the ordinary authored edit route.
    RestoreNames(Vec<NameRestoration>),
    /// Set the workbook's iterative-calculation settings (dc-y2o.2): whether circular references
    /// are calculated iteratively, the most iterations and the maximum change, as the workbook
    /// part's `calcPr` holds them. It changes no cell.
    SetIteration(crate::definitions::IterationSettings),
    /// Changes the physical dimensions of one worksheet row or column.
    SetAxisFormat(crate::AxisFormatEdit),
    /// Freeze the leading rows and columns of a worksheet window; `None` unfreezes it.
    SetFrozenPanes {
        sheet: SheetId,
        panes: Option<FrozenPanes>,
    },
    SetPaneLayout { sheet: SheetId, layout: Option<PaneLayout> },
    /// Insert or delete rows or columns of one sheet.
    EditAxis(AxisEdit),
    /// Insert, rename, move, remove or show or hide a sheet.
    EditSheet(SheetEdit),
    /// Create or remove a block with its locator, or add, move or remove one of a block's
    /// placements in a view: document-only metadata, which changes no cell and no calculation.
    EditSkinLayout(SkinLayoutEdit),
}

/// Leading worksheet bands kept visible while the remaining bands scroll.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FrozenPanes {
    rows: u32,
    columns: u32,
}

/// One first-window pane layout; frozen bands and separate splits are mutually exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaneLayout {
    Frozen(FrozenPanes),
    Split(SplitPanes),
}

/// Separate-window splits in twentieths of a point, and the initial right/bottom origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SplitPanes {
    x: u64,
    y: u64,
    row: u32,
    column: u32,
}
impl SplitPanes {
    pub fn new(x: f64, y: f64, row: u32, column: u32) -> Option<Self> {
        (x.is_finite()
            && y.is_finite()
            && x >= 0.
            && y >= 0.
            && (x > 0. || y > 0.)
            && (1..=MAX_ROWS).contains(&row)
            && (1..=MAX_COLUMNS).contains(&column))
        .then_some(Self {
            x: x.to_bits(),
            y: y.to_bits(),
            row,
            column,
        })
    }
    pub fn positions(self) -> (u64, u64) {
        (self.x, self.y)
    }
    pub fn origin(self) -> (u32, u32) {
        (self.row, self.column)
    }
}
impl PaneLayout {
    pub fn moved(self, edit: AxisEdit) -> Option<Self> {
        Some(match self {
            Self::Frozen(p) => Self::Frozen(p.moved(edit)?),
            Self::Split(mut p) => {
                let old = if edit.axis() == Axis::Rows {
                    p.row
                } else {
                    p.column
                };
                let next = crate::moved_position(edit, old)
                    .unwrap_or(edit.first().min(edit.axis().size()));
                if edit.axis() == Axis::Rows {
                    p.row = next;
                } else {
                    p.column = next;
                }
                Self::Split(p)
            }
        })
    }
}

impl FrozenPanes {
    /// At least one frozen band, leaving a scrollable row and column within the worksheet.
    pub fn new(rows: u32, columns: u32) -> Option<Self> {
        ((rows != 0 || columns != 0) && rows < MAX_ROWS && columns < MAX_COLUMNS)
            .then_some(Self { rows, columns })
    }
    pub fn rows(self) -> u32 {
        self.rows
    }
    pub fn columns(self) -> u32 {
        self.columns
    }
    /// The boundary follows insertions and deletions before it.
    pub fn moved(self, edit: AxisEdit) -> Option<Self> {
        let boundary = match edit.axis() {
            Axis::Rows => self.rows,
            Axis::Columns => self.columns,
        };
        let next = if edit.first() > boundary {
            boundary
        } else {
            match edit.change() {
                AxisChange::Insert => boundary
                    .saturating_add(edit.count())
                    .min(edit.axis().size() - 1),
                AxisChange::Delete => {
                    boundary.saturating_sub(edit.last().min(boundary) - edit.first() + 1)
                }
            }
        };
        match edit.axis() {
            Axis::Rows => Self::new(next, self.columns),
            Axis::Columns => Self::new(self.rows, next),
        }
    }
}

/// The rows or the columns of a sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    Rows,
    Columns,
}

impl Axis {
    /// The number of positions on this axis of a sheet: [`MAX_ROWS`] or [`MAX_COLUMNS`].
    pub const fn size(self) -> u32 {
        match self {
            Axis::Rows => MAX_ROWS,
            Axis::Columns => MAX_COLUMNS,
        }
    }
}

/// Whether an [`AxisEdit`] inserts or deletes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AxisChange {
    Insert,
    Delete,
}

/// Why [`AxisEdit::new`] refused an edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AxisEditError {
    /// The first position is 0 or past the axis.
    FirstOutOfRange { axis: Axis, first: u32 },
    /// An edit inserts or deletes at least one row or column.
    ZeroCount,
    /// The edit's last position would lie past the axis.
    PastGridEnd { axis: Axis, first: u32, count: u32 },
}

impl fmt::Display for AxisEditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AxisEditError::FirstOutOfRange { axis, first } => write!(
                f,
                "position {first} is outside the {axis:?} 1 to {}",
                axis.size()
            ),
            AxisEditError::ZeroCount => f.write_str("the axis edit covers no position"),
            AxisEditError::PastGridEnd { axis, first, count } => write!(
                f,
                "{count} positions from {first} pass the last of the {axis:?} ({})",
                axis.size()
            ),
        }
    }
}

impl std::error::Error for AxisEditError {}

/// The structural-edit descriptor for rows and columns: on one sheet, insert or delete `count`
/// positions starting at the 1-based position `first`. Positions `first` to
/// `first + count - 1` lie inside the axis. Whether the sheet's contents allow the edit is
/// decided where the edit is applied, not here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AxisEdit {
    sheet: SheetId,
    axis: Axis,
    change: AxisChange,
    first: u32,
    count: u32,
}

impl AxisEdit {
    pub fn new(
        sheet: SheetId,
        axis: Axis,
        change: AxisChange,
        first: u32,
        count: u32,
    ) -> Result<Self, AxisEditError> {
        let size = axis.size();
        if first == 0 || first > size {
            return Err(AxisEditError::FirstOutOfRange { axis, first });
        }
        if count == 0 {
            return Err(AxisEditError::ZeroCount);
        }
        if count > size - first + 1 {
            return Err(AxisEditError::PastGridEnd { axis, first, count });
        }
        Ok(AxisEdit {
            sheet,
            axis,
            change,
            first,
            count,
        })
    }

    pub const fn sheet(self) -> SheetId {
        self.sheet
    }

    pub const fn axis(self) -> Axis {
        self.axis
    }

    pub const fn change(self) -> AxisChange {
        self.change
    }

    /// The first position, 1-based.
    pub const fn first(self) -> u32 {
        self.first
    }

    pub const fn count(self) -> u32 {
        self.count
    }

    /// The last position the edit covers, 1-based.
    pub const fn last(self) -> u32 {
        self.first + (self.count - 1)
    }
}

/// The structural-edit descriptor for sheets. A position is the 1-based place a sheet takes in
/// the workbook's sheet order after the edit; whether it lies within the order, and whether a
/// name is unique, is decided where the edit is applied.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SheetEdit {
    /// A new, visible sheet with this document-stable key. Its [`SheetId`] is issued when the
    /// edit is applied.
    Insert {
        key: SheetKey,
        name: SheetName,
        position: NonZeroU32,
    },
    Rename {
        sheet: SheetId,
        name: SheetName,
    },
    Move {
        sheet: SheetId,
        position: NonZeroU32,
    },
    Remove {
        sheet: SheetId,
    },
    /// Sets document-only metadata.
    SetVisibility {
        sheet: SheetId,
        visibility: SheetVisibility,
    },
}

/// The authored operations of one command, prepared against the authored revision `base`. It
/// holds workbook operations only: the command's source, undo grouping and session operations
/// belong to CalcHost's journal.
///
/// The operations are not checked against one another here. An operation may name a sheet that
/// an earlier operation of the same batch removes, and none can name the sheet an earlier insert
/// of the batch adds, which has no [`SheetId`] until the insert is applied. Whoever applies the
/// batch checks each operation against the state the earlier ones leave.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OpBatch {
    pub base: AuthoredRevision,
    pub operations: Vec<WorkbookOp>,
}
