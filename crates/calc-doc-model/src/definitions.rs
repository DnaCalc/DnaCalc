//! What a workbook defines beside its cells' inputs that a calculation reads: array formulas,
//! defined names and tables, and the cells the model holds no input for because their content is
//! not modelled ([`UnloadedCell`]).
//!
//! These are authored facts as the file gave them. None of them holds a calculated value: an
//! array formula's anchor holds its formula text as the cell's [`crate::CellInput`], and the cells
//! of its area that it fills are not inputs at all; what a file cached for them is kept beside the
//! model by the codec, as file caches.

use crate::cache::CachedValue;
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

pub use calc_value::NameScope;

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

/// A book the workbook's external references name, as its externalLink part caches it (dc-ylr): a
/// file's formula writes `[1]Sheet1!A1` for cell A1 of sheet Sheet1 of the first such book.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalBook {
    /// The main external-link relationship target, without reducing it to a file name.
    pub target: Option<String>,
    /// The file's explicit absolute fallback relationship target, when present.
    pub absolute_target: Option<String>,
    /// The file name of the book the link targets (`Book2.xlsx`), the last part of its
    /// relationship's target, when the part names one: a typed formula names the book by it
    /// (`[Book2.xlsx]Sheet1!A1`).
    pub file_name: Option<String>,
    /// Its sheets, in the order the part lists their names.
    pub sheets: Vec<ExternalSheet>,
}

/// One sheet of an [`ExternalBook`]: its name and the values its link caches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalSheet {
    pub name: String,
    /// The ID the workbook's load issued the sheet (dc-ylr), by which a reference to it names it:
    /// a reference to another workbook is a reference, to a sheet of no workbook the model holds,
    /// whose cells only the link's cache gives. `None` when no ID could be issued.
    pub id: Option<SheetId>,
    /// The link's `sheetDataSet` holds a `sheetData` for the sheet. A cell such a cache does not
    /// hold is blank: Excel 16.0.20527 (2026-10-30, links not updated, the other book absent)
    /// gives `=[1]Sheet1!C1` 0 and `ISBLANK([1]Sheet1!C1)` TRUE where C1 was empty and is not
    /// cached. A sheet without one gives no cell.
    pub cached: bool,
    /// The link's `sheetData` is marked `refreshError`: Excel could not read the book when it last
    /// tried, and a reference to a cell it caches nothing for is #REF!.
    pub refresh_error: bool,
    /// The cached values, by row and column.
    pub cells: BTreeMap<(u32, u32), CachedValue>,
}

/// A workbook's connections and data model, as `xl/connections.xml` and `xl/model/item.data` hold
/// them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct DataModel {
    /// Every connection's name.
    pub connections: Vec<String>,
    /// The connection to the workbook's own data model.
    pub model_connection: Option<String>,
    /// Numeric connection identity of the workbook's own model, when unique and valid.
    pub model_connection_id: Option<u32>,
    /// Connections outside the workbook model and its worksheet sources.
    pub other_connections: Vec<String>,
    /// The worksheet tables feeding the model, by name.
    pub tables: Vec<String>,
    /// Every table the model part names, whatever feeds it. A model with one that no worksheet
    /// table feeds holds rows the workbook's cells do not.
    pub model_tables: Vec<String>,
    pub measures: Vec<ModelMeasure>,
    /// Rows read from the feeding worksheet tables at open or explicit refresh.
    pub snapshot: Option<ModelSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ModelSnapshot {
    pub tables: Vec<ModelSnapshotTable>,
}

/// Origin of the rows, distinct from cached formula results.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ModelSnapshotProvenance {
    WorksheetTables { revision: u64 },
    DecodedModel { part: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ModelSnapshotTable {
    pub provenance: ModelSnapshotProvenance,
    pub name: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<CachedValue>>>,
}

/// A measure of the data model.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ModelMeasure {
    pub name: String,
    /// The DAX after the `=`.
    pub expression: String,
}

/// A table (ListObject) of a sheet, as its table part declares it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Table {
    pub id: u32,
    pub style: TableStyle,
    pub filter: Option<crate::AutoFilter>,
    pub totals_functions: Vec<Option<String>>,
    pub totals_labels: Vec<Option<String>>,
    /// Calculated columns and other unmodelled table children retain their source.
    pub opaque: bool,
    /// Imported metadata whose axis references cannot be kept consistent.
    pub axis_opaque: bool,
    /// Calculated-column sources, anchored at the first data row.
    pub column_formulas: Vec<Option<String>>,
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

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct TableStyle {
    pub name: Option<String>,
    pub first_column: bool,
    pub last_column: bool,
    pub row_stripes: bool,
    pub column_stripes: bool,
}
impl TableStyle {
    /// The six accent variants of the supported medium band/header preset.
    pub fn medium_accent_theme(&self) -> Option<u32> {
        let name = self.name.as_deref()?;
        let index = name.strip_prefix("TableStyleMedium")?.parse::<u32>().ok()?;
        ((2..=7).contains(&index) && name == format!("TableStyleMedium{index}"))
            .then_some(index + 2)
    }
}

/// Why the model holds no input for a cell that the file gave content. A calculation reads such a
/// cell as not available, never as blank; the codec keeps the cell's bytes for a save.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum UnloadedCell {
    /// A value cell whose value is a rich value (a linked data type, an image in a cell): the
    /// file's `vm` names a rich value of this structure type (`_linkedEntity`, `_webimage`, ...),
    /// which the model does not represent. No bead owns modelling rich values yet.
    RichValue { structure: String },
    /// A value cell whose `vm` could not be followed to a rich value: the reason names the broken
    /// link. No bead owns modelling rich values yet.
    UnresolvedValueMetadata { reason: String },
    /// A cell of a what-if data table (`<f t="dataTable">`). Owner: L3.
    DataTable,
    /// A formula cell of an Excel 4.0 macro sheet (dc-xuw): its formula is written in the macro
    /// language (commands, `RETURN`, `GOTO`), which no engine here runs, so it is not calculated;
    /// what the file cached for it is kept, and the sheet's part is saved as it was read.
    MacroSheet,
    /// A formula cell whose formula Excel's formula entry refuses, by `rule` (dc-mx3x): Excel
    /// opens such a file only by repairing it, which removes the formula and keeps the value the
    /// file cached as a constant (measured on Excel 16.0.20527, 2026-11-07: a stored `IRR()` or
    /// `COUNTBLANK(2)` makes `Workbooks.Open` fail with 0x800A03EC, and with `CorruptLoad`
    /// repair the cell is blank, or holds its cached `<v>`, and `=A1` reads it so). The cell
    /// holds no formula here either: a calculation reads it as blank when the file cached
    /// nothing, and does not read a cached value (`cached`). The codec keeps its bytes.
    RejectedFormula { rule: String, cached: bool },
}

/// A workbook's iterative-calculation settings: the workbook part's `calcPr` attributes
/// `iterate`, `iterateCount` and `iterateDelta` (L3, dc-m3u). With iteration off a circular
/// reference is not calculated; with it on, the formulas on a cycle are calculated over and over,
/// at most `max_iterations` times, until no value changes by `max_change` or more.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IterationSettings {
    enabled: bool,
    max_iterations: u32,
    max_change_bits: u64,
}

impl IterationSettings {
    /// Excel's defaults, which a `calcPr` without the attributes means: off, 100 iterations, a
    /// maximum change of 0.001.
    pub const EXCEL_DEFAULT: IterationSettings = IterationSettings {
        enabled: false,
        max_iterations: 100,
        max_change_bits: 0x3f50_624d_d2f1_a9fc,
    };

    pub fn new(enabled: bool, max_iterations: u32, max_change: f64) -> Self {
        IterationSettings {
            enabled,
            max_iterations,
            max_change_bits: max_change.to_bits(),
        }
    }

    pub fn enabled(self) -> bool {
        self.enabled
    }

    pub fn max_iterations(self) -> u32 {
        self.max_iterations
    }

    pub fn max_change(self) -> f64 {
        f64::from_bits(self.max_change_bits)
    }
}

impl Default for IterationSettings {
    fn default() -> Self {
        IterationSettings::EXCEL_DEFAULT
    }
}

/// Which input cells a what-if data table (`TABLE()`) substitutes its row and column values into.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DataTableInputs {
    /// A one-variable table whose values stand in the row above it (`dtr="1"`): each of its
    /// cells calculates the formula in the column left of the table, in its row, with this cell
    /// set to the value above its column (`TABLE(C1,)`).
    Row(CellKey),
    /// A one-variable table whose values stand in the column left of it (`dtr="0"`): each cell
    /// calculates the formula in the row above the table, in its column, with this cell set to
    /// the value left of its row (`TABLE(,B1)`).
    Column(CellKey),
    /// A two-variable table (`dt2D="1"`): each cell calculates the formula above-left of the
    /// table with `row` set to the value above its column and `column` set to the value left of
    /// its row (`TABLE(B1,C1)`).
    Both { row: CellKey, column: CellKey },
}

/// A what-if data table: the area its results fill and its input cells, as the file's
/// `<f t="dataTable" ref r1 r2 dt2D dtr>` declares them (L3, dc-m3u). Measured on Excel 16.0
/// through COM on 2026-10-23 (`Range.Table`): with B3 `=B1*10+C1` and C1 = 5, `TABLE(,B1)` over
/// B4:B6 beside A4:A6 = 1, 2, 3 gave 15, 25, 35; `TABLE(B1,C1)` over B11:C12 with the formula
/// `=B1*1000+C1` in A10, B10:C10 = 7, 8 and A11:A12 = 1, 2 gave 7001, 8001, 7002, 8002.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DataTable {
    area: GridArea,
    inputs: DataTableInputs,
}

impl DataTable {
    /// A data table over `area`, whose top-left cell is not in the first row or column (the
    /// values and the formulas stand above and left of it). `None` otherwise.
    pub fn new(area: GridArea, inputs: DataTableInputs) -> Option<DataTable> {
        (area.first().row() > 1 && area.first().column() > 1).then_some(DataTable { area, inputs })
    }

    /// The cells the table's results fill.
    pub fn area(&self) -> GridArea {
        self.area
    }

    pub fn inputs(&self) -> DataTableInputs {
        self.inputs
    }

    /// For the cell `at` of the table: the formula cell it calculates, and each input cell with
    /// the cell whose value it is set to. `None` when `at` is not in the table.
    pub fn what_if(&self, at: CellKey) -> Option<(CellKey, Vec<(CellKey, CellKey)>)> {
        if !self.area.contains(at) {
            return None;
        }
        let sheet = at.sheet();
        let (top, left) = (self.area.first().row(), self.area.first().column());
        let cell = |row, column| CellKey::new(sheet, row, column).ok();
        let above = cell(top - 1, at.column())?;
        let beside = cell(at.row(), left - 1)?;
        Some(match self.inputs {
            DataTableInputs::Row(input) => (beside, vec![(input, above)]),
            DataTableInputs::Column(input) => (above, vec![(input, beside)]),
            DataTableInputs::Both { row, column } => (
                cell(top - 1, left - 1)?,
                vec![(row, above), (column, beside)],
            ),
        })
    }

    /// This table after its cells and input cells move by `moved_area` and `moved_cell`; `None`
    /// when any of them is deleted.
    pub fn moved(
        &self,
        moved_area: impl Fn(GridArea) -> Option<GridArea>,
        moved_cell: impl Fn(CellKey) -> Option<CellKey>,
    ) -> Option<DataTable> {
        let inputs = match self.inputs {
            DataTableInputs::Row(input) => DataTableInputs::Row(moved_cell(input)?),
            DataTableInputs::Column(input) => DataTableInputs::Column(moved_cell(input)?),
            DataTableInputs::Both { row, column } => DataTableInputs::Both {
                row: moved_cell(row)?,
                column: moved_cell(column)?,
            },
        };
        DataTable::new(moved_area(self.area)?, inputs)
    }
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
    styles: std::sync::Arc<crate::StyleTable>,
    conditional: BTreeMap<SheetId, Vec<crate::ConditionalRuleSource>>,
    validations: BTreeMap<SheetId, Vec<crate::Validation>>,
    review: BTreeMap<SheetId, Vec<crate::ReviewNote>>,
    links: BTreeMap<SheetId, Vec<crate::ReviewLink>>,
    page_setups: BTreeMap<SheetId, crate::PageSetup>,
    charts: BTreeMap<SheetId, std::sync::Arc<[crate::Chart]>>,
    pivots: std::sync::Arc<crate::pivot::PivotState>,
    data_model: Option<std::sync::Arc<DataModel>>,
    axis_layouts: crate::AxisLayouts,
    panes: BTreeMap<SheetId, crate::PaneLayout>,
    filters: BTreeMap<SheetId, crate::AutoFilter>,
    arrays: BTreeMap<CellKey, ArrayFormula>,
    names: crate::NameTable,
    tables: Vec<Table>,
    unloaded: BTreeMap<CellKey, UnloadedCell>,
    hidden_rows: BTreeMap<(SheetId, u32), RowHidden>,
    /// The workbook is macro-enabled (`.xlsm`, `.xltm`), so a VBA project may define functions.
    macro_enabled: bool,
    iteration: IterationSettings,
    data_tables: Vec<DataTable>,
    /// The workbook's file name (`Book1.xlsx`), when the package's source knows it.
    workbook_name: Option<String>,
    /// The cell formats edits gave cells (dc-09v): each an index into the workbook's cell formats
    /// (a cell's `s`), over the format the file gave the cell.
    formats: BTreeMap<CellKey, u32>,
    /// The books the external references name, by their number less one (`[1]` is the first):
    /// each the book its externalLink part caches, or `None` for a link not read.
    external_books: Vec<Option<ExternalBook>>,
}

impl Definitions {
    pub fn pivots(&self) -> &std::sync::Arc<crate::pivot::PivotState> {
        &self.pivots
    }
    pub fn data_model(&self) -> Option<&std::sync::Arc<DataModel>> {
        self.data_model.as_ref()
    }
    pub fn with_data_model(mut self, model: Option<DataModel>) -> Self {
        self.data_model = model.map(std::sync::Arc::new);
        self
    }
    pub fn with_shared_data_model(mut self, model: std::sync::Arc<DataModel>) -> Self {
        self.data_model = Some(model);
        self
    }
    pub fn with_pivots(mut self, state: std::sync::Arc<crate::pivot::PivotState>) -> Self {
        self.pivots = state;
        self
    }

    pub fn page_setup(&self, sheet: SheetId) -> crate::PageSetup {
        self.page_setups.get(&sheet).cloned().unwrap_or_default()
    }
    pub fn page_setups(&self) -> impl Iterator<Item = (SheetId, &crate::PageSetup)> {
        self.page_setups.iter().map(|(s, p)| (*s, p))
    }
    pub fn with_page_setup(mut self, sheet: SheetId, setup: crate::PageSetup) -> Self {
        if setup == crate::PageSetup::default() {
            self.page_setups.remove(&sheet);
        } else {
            self.page_setups.insert(sheet, setup);
        }
        self
    }

    pub fn charts(&self, sheet: SheetId) -> &[crate::Chart] {
        self.charts
            .get(&sheet)
            .map(|charts| charts.as_ref())
            .unwrap_or(&[])
    }
    pub fn all_charts(&self) -> impl Iterator<Item = (SheetId, &[crate::Chart])> {
        self.charts
            .iter()
            .map(|(sheet, charts)| (*sheet, charts.as_ref()))
    }
    pub fn with_charts(mut self, sheet: SheetId, charts: Vec<crate::Chart>) -> Self {
        if charts.is_empty() {
            self.charts.remove(&sheet);
        } else {
            self.charts.insert(sheet, charts.into());
        }
        self
    }

    pub fn styles(&self) -> &std::sync::Arc<crate::StyleTable> {
        &self.styles
    }
    pub fn with_styles(mut self, styles: std::sync::Arc<crate::StyleTable>) -> Self {
        self.styles = styles;
        self
    }

    pub fn validations(&self, sheet: SheetId) -> &[crate::Validation] {
        self.validations
            .get(&sheet)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    pub fn all_validations(&self) -> impl Iterator<Item = (SheetId, &[crate::Validation])> {
        self.validations.iter().map(|(s, v)| (*s, v.as_slice()))
    }
    pub fn with_validations(mut self, sheet: SheetId, rules: Vec<crate::Validation>) -> Self {
        if rules.is_empty() {
            self.validations.remove(&sheet);
        } else {
            self.validations.insert(sheet, rules);
        }
        self
    }

    pub fn review_links(&self, sheet: SheetId) -> &[crate::ReviewLink] {
        self.links.get(&sheet).map(Vec::as_slice).unwrap_or(&[])
    }
    pub fn all_review_links(&self) -> impl Iterator<Item = (SheetId, &[crate::ReviewLink])> {
        self.links.iter().map(|(s, links)| (*s, links.as_slice()))
    }
    pub fn with_review_links(mut self, sheet: SheetId, links: Vec<crate::ReviewLink>) -> Self {
        if links.is_empty() {
            self.links.remove(&sheet);
        } else {
            self.links.insert(sheet, links);
        }
        self
    }
    pub fn review_notes(&self, sheet: SheetId) -> &[crate::ReviewNote] {
        self.review.get(&sheet).map(Vec::as_slice).unwrap_or(&[])
    }
    pub fn all_review_notes(&self) -> impl Iterator<Item = (SheetId, &[crate::ReviewNote])> {
        self.review.iter().map(|(s, n)| (*s, n.as_slice()))
    }
    pub fn with_review_notes(mut self, sheet: SheetId, notes: Vec<crate::ReviewNote>) -> Self {
        if notes.is_empty() {
            self.review.remove(&sheet);
        } else {
            self.review.insert(sheet, notes);
        }
        self
    }

    pub fn conditional_rules(&self, sheet: SheetId) -> &[crate::ConditionalRuleSource] {
        self.conditional
            .get(&sheet)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    pub fn all_conditional_rules(
        &self,
    ) -> impl Iterator<Item = (SheetId, &[crate::ConditionalRuleSource])> {
        self.conditional.iter().map(|(s, r)| (*s, r.as_slice()))
    }
    pub fn with_conditional_rules(
        mut self,
        sheet: SheetId,
        rules: Vec<crate::ConditionalRuleSource>,
    ) -> Self {
        self.conditional.insert(sheet, rules);
        self
    }

    pub fn axis_layouts(&self) -> &crate::AxisLayouts {
        &self.axis_layouts
    }
    pub fn with_axis_layouts(mut self, layouts: crate::AxisLayouts) -> Self {
        self.axis_layouts = layouts;
        self
    }
    pub fn with_axis_format(self, edit: crate::AxisFormatEdit) -> Self {
        self.with_axis_formats([edit])
    }
    pub fn with_axis_formats(
        mut self,
        edits: impl IntoIterator<Item = crate::AxisFormatEdit>,
    ) -> Self {
        let mut rows = std::collections::BTreeSet::new();
        let mut outline_sheets = std::collections::BTreeSet::new();
        for edit in edits {
            if edit.axis() == crate::Axis::Rows {
                let old = self
                    .axis_layouts
                    .get(edit.sheet(), edit.axis(), edit.index());
                rows.insert((edit.sheet(), edit.index()));
                if old.outline_level() != edit.format().outline_level()
                    || old.collapsed() != edit.format().collapsed()
                    || edit.summary_after().is_some()
                {
                    outline_sheets.insert(edit.sheet());
                }
            }
            self.axis_layouts.set(edit);
        }
        for sheet in outline_sheets {
            rows.extend(
                self.axis_layouts
                    .bands(sheet, crate::Axis::Rows)
                    .map(|(r, _)| (sheet, r)),
            );
            rows.extend(
                self.hidden_rows
                    .range((sheet, 1)..=(sheet, 1048576))
                    .map(|(key, _)| *key),
            );
        }
        let outlined = self.axis_layouts.outlined();
        for (sheet, row) in rows {
            if self
                .filters
                .get(&sheet)
                .is_some_and(|f| f.filtered_rows.contains(&row))
                || (self.hidden_rows.get(&(sheet, row)) == Some(&RowHidden::Filtered)
                    && self.tables().iter().any(|t| {
                        t.sheet == sheet && row > t.area.first().row() && row <= t.area.last().row()
                    }))
            {
                self.hidden_rows.insert((sheet, row), RowHidden::Filtered);
            } else if outlined.get(sheet, crate::Axis::Rows, row).hidden() {
                self.hidden_rows.insert((sheet, row), RowHidden::Manual);
            } else {
                self.hidden_rows.remove(&(sheet, row));
            }
        }
        self
    }

    pub fn auto_filter(&self, sheet: SheetId) -> Option<&crate::AutoFilter> {
        self.filters.get(&sheet)
    }
    pub fn auto_filters(&self) -> impl Iterator<Item = (SheetId, &crate::AutoFilter)> {
        self.filters.iter().map(|(s, f)| (*s, f))
    }
    pub fn with_auto_filter(mut self, sheet: SheetId, filter: Option<crate::AutoFilter>) -> Self {
        let outlined = self.axis_layouts.outlined();
        if let Some(old) = self.filters.remove(&sheet) {
            for row in old.filtered_rows {
                let key = (sheet, row);
                if self.tables.iter().filter(|t| t.sheet == sheet).any(|t| {
                    t.filter
                        .as_ref()
                        .is_some_and(|f| f.filtered_rows.contains(&row))
                }) {
                    self.hidden_rows.insert(key, RowHidden::Filtered);
                } else if outlined.get(sheet, crate::Axis::Rows, row).hidden() {
                    self.hidden_rows.insert(key, RowHidden::Manual);
                } else {
                    self.hidden_rows.remove(&key);
                }
            }
        }
        if let Some(filter) = filter {
            for row in &filter.filtered_rows {
                self.hidden_rows.insert((sheet, *row), RowHidden::Filtered);
            }
            self.filters.insert(sheet, filter);
        }
        self
    }
    /// Physical visibility includes filtering; the authored axis table retains manual hiding.
    pub fn visible_axis_layouts(&self) -> crate::AxisLayouts {
        let mut layouts = self.axis_layouts.outlined();
        for ((sheet, row), why) in &self.hidden_rows {
            if *why == RowHidden::Filtered {
                let format = layouts
                    .get(*sheet, crate::Axis::Rows, *row)
                    .with_hidden(true);
                layouts.set(
                    crate::AxisFormatEdit::new(*sheet, crate::Axis::Rows, *row, format)
                        .expect("valid hidden row"),
                );
            }
        }
        layouts
    }
    pub fn frozen_panes(&self, sheet: SheetId) -> Option<crate::FrozenPanes> {
        match self.pane_layout(sheet) {
            Some(crate::PaneLayout::Frozen(p)) => Some(p),
            _ => None,
        }
    }
    pub fn panes(&self) -> impl Iterator<Item = (SheetId, crate::FrozenPanes)> + '_ {
        self.panes.iter().filter_map(|(&s, &p)| match p {
            crate::PaneLayout::Frozen(p) => Some((s, p)),
            _ => None,
        })
    }
    pub fn with_panes(
        mut self,
        panes: impl IntoIterator<Item = (SheetId, crate::FrozenPanes)>,
    ) -> Self {
        self.panes = panes
            .into_iter()
            .map(|(s, p)| (s, crate::PaneLayout::Frozen(p)))
            .collect();
        self
    }
    pub fn with_frozen_panes(mut self, sheet: SheetId, panes: Option<crate::FrozenPanes>) -> Self {
        if let Some(panes) = panes {
            self.panes.insert(sheet, crate::PaneLayout::Frozen(panes));
        } else {
            self.panes.remove(&sheet);
        }
        self
    }
    pub fn pane_layout(&self, sheet: SheetId) -> Option<crate::PaneLayout> {
        self.panes.get(&sheet).copied()
    }
    pub fn pane_layouts(&self) -> impl Iterator<Item = (SheetId, crate::PaneLayout)> + '_ {
        self.panes.iter().map(|(&s, &p)| (s, p))
    }
    pub fn with_pane_layouts(
        mut self,
        panes: impl IntoIterator<Item = (SheetId, crate::PaneLayout)>,
    ) -> Self {
        self.panes = panes.into_iter().collect();
        self
    }
    pub fn with_pane_layout(mut self, sheet: SheetId, layout: Option<crate::PaneLayout>) -> Self {
        if let Some(layout) = layout {
            self.panes.insert(sheet, layout);
        } else {
            self.panes.remove(&sheet);
        }
        self
    }
    pub fn new(
        arrays: impl IntoIterator<Item = ArrayFormula>,
        names: Vec<DefinedName>,
        tables: Vec<Table>,
        unloaded: impl IntoIterator<Item = (CellKey, UnloadedCell)>,
    ) -> Self {
        Definitions {
            styles: Default::default(),
            conditional: BTreeMap::new(),
            review: BTreeMap::new(),
            links: BTreeMap::new(),
            charts: BTreeMap::new(),
            pivots: Default::default(),
            data_model: None,
            page_setups: BTreeMap::new(),
            validations: BTreeMap::new(),
            axis_layouts: crate::AxisLayouts::default(),
            panes: BTreeMap::new(),
            filters: BTreeMap::new(),
            arrays: arrays
                .into_iter()
                .map(|array| (array.anchor(), array))
                .collect(),
            names: crate::NameTable::new(names),
            tables,
            unloaded: unloaded.into_iter().collect(),
            hidden_rows: BTreeMap::new(),
            macro_enabled: false,
            iteration: IterationSettings::EXCEL_DEFAULT,
            data_tables: Vec::new(),
            workbook_name: None,
            formats: BTreeMap::new(),
            external_books: Vec::new(),
        }
    }

    /// These definitions with the books the workbook's external references name (dc-ylr).
    pub fn with_external_books(mut self, books: Vec<Option<ExternalBook>>) -> Self {
        self.external_books = books;
        self
    }

    /// The book `[number]` names (`[1]` the first), `None` when there is none or it was not read.
    pub fn external_book(&self, number: u32) -> Option<&ExternalBook> {
        let index = usize::try_from(number).ok()?.checked_sub(1)?;
        self.external_books.get(index)?.as_ref()
    }

    /// The number of the book whose link targets the file named `file_name` (`[2]` for the
    /// second link), matched without regard to case; the first such link when several do.
    pub fn external_book_named(&self, file_name: &str) -> Option<u32> {
        let wanted = file_name.to_uppercase();
        self.external_books
            .iter()
            .position(|book| {
                book.as_ref()
                    .and_then(|book| book.file_name.as_ref())
                    .is_some_and(|name| name.to_uppercase() == wanted)
            })
            .and_then(|index| u32::try_from(index + 1).ok())
    }

    /// The sheet of another workbook that the load issued `sheet` for (dc-ylr), or `None` when
    /// `sheet` is no such sheet.
    pub fn external_sheet(&self, sheet: SheetId) -> Option<&ExternalSheet> {
        self.external_books
            .iter()
            .flatten()
            .flat_map(|book| book.sheets.iter())
            .find(|held| held.id == Some(sheet))
    }

    /// The books the external references name, each with its number (`[1]` the first); a link
    /// not read is left out.
    pub fn external_books(&self) -> impl Iterator<Item = (u32, &ExternalBook)> {
        self.external_books
            .iter()
            .enumerate()
            .filter_map(|(index, book)| Some((u32::try_from(index + 1).ok()?, book.as_ref()?)))
    }

    /// Each link's number and the file name of the book it targets, for a typed formula's
    /// reference that names the book by file name (dc-ylr).
    pub fn external_file_names(&self) -> Vec<(u32, String)> {
        self.external_books()
            .filter_map(|(number, book)| Some((number, book.file_name.clone()?)))
            .collect()
    }

    /// The books, mutably, for the load to issue their sheets' IDs.
    pub fn external_books_mut(&mut self) -> impl Iterator<Item = &mut ExternalBook> {
        self.external_books.iter_mut().flatten()
    }

    /// These definitions with the cell formats edits gave cells (index into the workbook's cell
    /// formats), in place of those they held.
    pub fn with_formats(mut self, formats: impl IntoIterator<Item = (CellKey, u32)>) -> Self {
        self.formats = formats.into_iter().collect();
        self
    }

    /// The cell format an edit gave `at` ([`crate::WorkbookOp::SetFormat`]); `None` when no edit
    /// did, and the cell keeps the format the file gave it.
    pub fn cell_format(&self, at: CellKey) -> Option<u32> {
        self.formats.get(&at).copied()
    }

    /// Every cell format edits gave, in cell order.
    pub fn cell_formats(&self) -> impl Iterator<Item = (CellKey, u32)> + '_ {
        self.formats.iter().map(|(at, format)| (*at, *format))
    }

    /// These definitions with `at`'s format set to `format`, or returned to the file's for `None`.
    pub(crate) fn with_format_of(mut self, at: CellKey, format: Option<u32>) -> Self {
        match format {
            Some(format) => {
                self.formats.insert(at, format);
            }
            None => {
                self.formats.remove(&at);
            }
        }
        self
    }

    /// These definitions of the workbook whose file name is `name` (`Book1.xlsx`, without its
    /// folder): INDIRECT's text names this workbook so (`[Book1.xlsx]Sheet1!A1`).
    pub fn with_workbook_name(mut self, name: Option<String>) -> Self {
        self.workbook_name = name;
        self
    }

    /// The workbook's file name ([`Definitions::with_workbook_name`]), `None` when not known.
    pub fn workbook_name(&self) -> Option<&str> {
        self.workbook_name.as_deref()
    }

    /// These definitions with the workbook's what-if data tables.
    pub fn with_data_tables(mut self, data_tables: Vec<DataTable>) -> Self {
        self.data_tables = data_tables;
        self
    }

    /// The workbook's what-if data tables ([`DataTable`]).
    pub fn data_tables(&self) -> &[DataTable] {
        &self.data_tables
    }

    /// The data table whose area holds `at`.
    pub fn data_table_containing(&self, at: CellKey) -> Option<&DataTable> {
        self.data_tables
            .iter()
            .find(|table| table.area.contains(at))
    }

    /// These definitions with the workbook's iterative-calculation settings.
    pub fn with_iteration(mut self, iteration: IterationSettings) -> Self {
        self.iteration = iteration;
        self
    }

    /// The workbook's iterative-calculation settings ([`IterationSettings`]).
    pub fn iteration(&self) -> IterationSettings {
        self.iteration
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

    /// Every hidden row, by sheet and row, with why.
    pub fn hidden_rows_all(&self) -> impl Iterator<Item = (SheetId, u32, RowHidden)> + '_ {
        self.hidden_rows
            .iter()
            .map(|((sheet, row), why)| (*sheet, *row, *why))
    }

    /// These definitions with `names` as the defined names.
    pub fn with_names(mut self, names: Vec<DefinedName>) -> Self {
        self.names = self.names.replaced(names);
        self
    }

    pub fn name_table(&self) -> &crate::NameTable {
        &self.names
    }

    pub(crate) fn with_name_table(mut self, names: crate::NameTable) -> Self {
        self.names = names;
        self
    }

    /// These definitions without anything of `sheet`: its array formulas, tables, unloaded
    /// cells, hidden rows and the names scoped to it (a removed sheet).
    pub fn without_sheet(mut self, sheet: SheetId) -> Self {
        self.validations.remove(&sheet);
        self.conditional.remove(&sheet);
        self.review.remove(&sheet);
        self.links.remove(&sheet);
        self.charts.remove(&sheet);
        std::sync::Arc::make_mut(&mut self.pivots)
            .sheets
            .remove(&sheet);
        self.page_setups.remove(&sheet);
        if self.styles.runs.iter().any(|r| r.area.sheet() == sheet) {
            let mut styles = (*self.styles).clone();
            styles.runs = styles.runs.without_sheet(sheet);
            self.styles = std::sync::Arc::new(styles);
        }
        self.axis_layouts.without_sheet(sheet);
        self.panes.remove(&sheet);
        self.filters.remove(&sheet);
        self.arrays.retain(|at, _| at.sheet() != sheet);
        self.tables.retain(|table| table.sheet != sheet);
        self.unloaded.retain(|at, _| at.sheet() != sheet);
        self.hidden_rows.retain(|(held, _), _| *held != sheet);
        self.names
            .retain(|name| name.scope != NameScope::Sheet(sheet));
        self.data_tables.retain(|table| table.area.sheet() != sheet);
        self.formats.retain(|at, _| at.sheet() != sheet);
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
        self.review.is_empty()
            && self.links.is_empty()
            && self.charts.is_empty()
            && self.pivots.tables().next().is_none()
            && self.pivots.caches.caches.is_empty()
            && self.page_setups.is_empty()
            && self.validations.is_empty()
            && self.conditional.is_empty()
            && self.axis_layouts.sheets().next().is_none()
            && self.panes.is_empty()
            && self.filters.is_empty()
            && self.arrays.is_empty()
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
        self.names.as_slice()
    }

    pub fn with_tables(mut self, tables: Vec<Table>) -> Self {
        let mut rows = std::collections::BTreeSet::new();
        for t in self.tables.iter().chain(tables.iter()) {
            if let Some(f) = &t.filter {
                rows.extend(f.filtered_rows.iter().map(|r| (t.sheet, *r)));
            }
        }
        self.tables = tables;
        let outlined = self.axis_layouts.outlined();
        for (sheet, row) in rows {
            let filtered = self
                .filters
                .get(&sheet)
                .is_some_and(|f| f.filtered_rows.contains(&row))
                || self.tables.iter().filter(|t| t.sheet == sheet).any(|t| {
                    t.filter
                        .as_ref()
                        .is_some_and(|f| f.filtered_rows.contains(&row))
                });
            if filtered {
                self.hidden_rows.insert((sheet, row), RowHidden::Filtered);
            } else if outlined.get(sheet, crate::Axis::Rows, row).hidden() {
                self.hidden_rows.insert((sheet, row), RowHidden::Manual);
            } else {
                self.hidden_rows.remove(&(sheet, row));
            }
        }
        self
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
            .chain(self.panes.keys().copied())
            .chain(self.filters.keys().copied())
            .chain(self.links.keys().copied())
            .chain(self.charts.keys().copied())
            .chain(self.pivots.sheets.keys().copied())
            .chain(self.page_setups.keys().copied())
            .chain(self.axis_layouts.sheets())
            .chain(self.data_tables.iter().map(|table| table.area.sheet()))
            .chain(
                self.names
                    .definitions()
                    .filter_map(|name| match name.scope {
                        NameScope::Sheet(sheet) => Some(sheet),
                        NameScope::Workbook => None,
                    }),
            )
    }

    /// These definitions after cell edits that set or cleared the cells for which `touched`
    /// answers true: an array formula whose anchor was touched is gone (the anchor now holds what
    /// the edit put there, and the area it filled is free), and so is an unloaded cell that was
    /// touched. `None` when nothing changed.
    ///
    /// The arrays of `added`, which the edits authored, are then added.
    pub(crate) fn after_edits(
        &self,
        touched: impl Fn(CellKey) -> bool,
        added: &[ArrayFormula],
    ) -> Option<Definitions> {
        let arrays_gone = self.arrays.keys().any(|at| touched(*at));
        let unloaded_gone = self.unloaded.keys().any(|at| touched(*at));
        if !arrays_gone && !unloaded_gone && added.is_empty() {
            return None;
        }
        let mut next = self.clone();
        next.arrays.retain(|at, _| !touched(*at));
        next.unloaded.retain(|at, _| !touched(*at));
        for array in added {
            next.arrays.insert(array.anchor(), *array);
        }
        Some(next)
    }
}
