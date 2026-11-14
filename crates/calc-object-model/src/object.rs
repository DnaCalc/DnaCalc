//! Object identities. An [`ObjectRef`] names what an object is; it holds no workbook state.
//! Projections wrap it (a COM wrapper per acquisition; Excel's own `Is` behaviour for two
//! acquisitions of the same range is measured by dc-1nbp.6 E1 before anything relies on it).

use crate::descriptor::InterfaceId;
use calc_func::StoredReference;
use calc_value::{NameId, SheetId};

/// An object of the model: the interface it answers as, and what it denotes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ObjectRef {
    /// The default interface (for example `_Application`, `Range`, `COMAddIns`).
    pub interface: InterfaceId,
    pub key: ObjectKey,
}

/// What an object denotes. Workbook objects name session identities; they are resolved again on
/// every call, so a structural edit or a closed workbook is seen, never a stale copy.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ObjectKey {
    Application,
    /// The workbook window the host answers WM_GETOBJECT for.
    Window,
    Workbooks,
    /// A workbook of the session, by the host's workbook identity.
    Workbook(u64),
    /// The Sheets/Worksheets collection of a workbook.
    Sheets(u64),
    Worksheet {
        workbook: u64,
        sheet: SheetId,
    },
    /// A range: a handle of the session's reference tracker (calc-fml `ReferenceTracker`), so it
    /// follows structural edits and fails typed when its cells are deleted.
    Range(StoredReference),
    /// Areas owns one acquisition of the original tracked range, preserving area order.
    Areas(StoredReference),
    /// Rows/Columns preserve the range's areas; Count uses the first area's axis length.
    RangeView {
        reference: StoredReference,
        axis: crate::geometry::RangeAxis,
    },
    Names(u64),
    Name {
        workbook: u64,
        id: NameId,
    },
    WorksheetFunction,
    /// An object of the report layer of one worksheet (tables, charts, drawings: dc-piw0). It
    /// names a table by its part, a chart by its part (or the token of a chart still waiting for
    /// its source data), a shape by its drawing id; resolved again on every call.
    Report {
        workbook: u64,
        sheet: SheetId,
        item: ReportItem,
    },
    /// An object a projection owns by nature (COMAddIns, CommandBars, ribbon objects): the
    /// projection's own handle.
    Host(u64),
}

impl ObjectRef {
    pub fn new(interface: InterfaceId, key: ObjectKey) -> ObjectRef {
        ObjectRef { interface, key }
    }
}

/// Which report object of a worksheet an [`ObjectKey::Report`] denotes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ReportItem {
    ListObjects,
    /// A table, by its table part.
    ListObject(String),
    ListColumns(String),
    ListColumn(String, u32),
    ListRows(String),
    ChartObjects,
    /// A chart frame, by chart part or pending token.
    ChartObject(String),
    Chart(String),
    ChartTitle(String),
    Shapes,
    Shape(u32),
    TextFrame2(u32),
    TextRange2(u32),
    FillFormat(u32),
    LineFormat(u32),
    ConnectorFormat(u32),
    /// A `ColorFormat` of a shape's fill (`false`) or line (`true`).
    ColorFormat(u32, bool),
}
