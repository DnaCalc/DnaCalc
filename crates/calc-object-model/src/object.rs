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
    /// Session identity or projection handle that the interface addresses.
    pub key: ObjectKey,
}

/// What an object denotes. Workbook objects name session identities; they are resolved again on
/// every call, so a structural edit or a closed workbook is seen, never a stale copy.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ObjectKey {
    /// The application's root object.
    Application,
    /// The workbook window the host answers WM_GETOBJECT for.
    Window,
    /// The application's Workbooks collection.
    Workbooks,
    /// A workbook of the session, by the host's workbook identity.
    Workbook(u64),
    /// The Sheets/Worksheets collection of a workbook.
    Sheets(u64),
    /// One worksheet in an open workbook.
    Worksheet {
        /// Host identity of the containing workbook.
        workbook: u64,
        /// Session identity of the worksheet.
        sheet: SheetId,
    },
    /// A range: a handle of the session's reference tracker (calc-fml `ReferenceTracker`), so it
    /// follows structural edits and fails typed when its cells are deleted.
    Range(StoredReference),
    /// Areas owns one acquisition of the original tracked range, preserving area order.
    Areas(StoredReference),
    /// Rows/Columns preserve the range's areas; Count uses the first area's axis length.
    RangeView {
        /// Tracked source reference for this view.
        reference: StoredReference,
        /// Whether the view exposes rows or columns.
        axis: crate::geometry::RangeAxis,
    },
    /// The defined names collection of the identified workbook.
    Names(u64),
    /// One defined name in an open workbook.
    Name {
        /// Host identity of the containing workbook.
        workbook: u64,
        /// Session identity of the defined name.
        id: NameId,
    },
    /// The application's WorksheetFunction object.
    WorksheetFunction,
    /// An object of the report layer of one worksheet (tables, charts, drawings: dc-piw0). It
    /// names a table by its part, a chart by its part (or the token of a chart still waiting for
    /// its source data), a shape by its drawing id; resolved again on every call.
    Report {
        /// Host identity of the containing workbook.
        workbook: u64,
        /// Session identity of the worksheet owning this report object.
        sheet: SheetId,
        /// Identity of the table, chart, shape or related object on the worksheet.
        item: ReportItem,
    },
    /// An object a projection owns by nature (COMAddIns, CommandBars, ribbon objects): the
    /// projection's own handle.
    Host(u64),
}

impl ObjectRef {
    /// Pair a descriptor interface with the identity it addresses.
    pub fn new(interface: InterfaceId, key: ObjectKey) -> ObjectRef {
        ObjectRef { interface, key }
    }
}

/// Which report object of a worksheet an [`ObjectKey::Report`] denotes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ReportItem {
    /// The worksheet's table collection.
    ListObjects,
    /// A table, by its table part.
    ListObject(String),
    /// Columns collection of the table identified by its part.
    ListColumns(String),
    /// One column of a table, identified by table part and zero-based column index.
    ListColumn(String, u32),
    /// Rows collection of the table identified by its part.
    ListRows(String),
    /// The worksheet's embedded chart collection.
    ChartObjects,
    /// A chart frame, by chart part.
    ChartObject(String),
    /// The chart within a frame, identified by chart part or pending chart token.
    Chart(String),
    /// The title of the chart identified by chart part or pending chart token.
    ChartTitle(String),
    /// The series of a chart, by chart part.
    SeriesCollection(String),
    /// One series of a chart (chart part, zero-based place across the plots).
    Series(String, u32),
    /// One axis of a chart: chart part and `XlAxisType` (1 category, 2 value). (dc-oxh8w.11)
    Axis(String, u8),
    /// The title of one axis (chart part, axis type). (dc-oxh8w.12)
    AxisTitle(String, u8),
    /// The tick labels of one axis (chart part, axis type). (dc-oxh8w.12)
    TickLabels(String, u8),
    /// A chart as a member of `Shapes` (`Shapes.AddChart2`, `Shapes(n)`), by chart part.
    ChartShape(String),
    /// The worksheet's shapes collection.
    Shapes,
    /// A worksheet shape identified by its drawing ID.
    Shape(u32),
    /// The text frame of the shape identified by its drawing ID.
    TextFrame2(u32),
    /// The text range of the shape identified by its drawing ID.
    TextRange2(u32),
    /// The fill settings of the shape identified by its drawing ID.
    FillFormat(u32),
    /// The line settings of the shape identified by its drawing ID.
    LineFormat(u32),
    /// The connector settings of the shape identified by its drawing ID.
    ConnectorFormat(u32),
    /// A `ColorFormat` of a shape's fill (`false`) or line (`true`).
    ColorFormat(u32, bool),
    /// The worksheet's `PageSetup` (dc-oxh8w.8).
    PageSetup,
    /// The worksheet's `HPageBreaks` (`true`) or `VPageBreaks` (`false`) collection.
    PageBreaks(bool),
    /// One page break: horizontal (`true`) or vertical, and the row or column after the break.
    PageBreak(bool, u32),
}
