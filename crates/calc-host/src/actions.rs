//! Workbook commands shared by every host client. Excel rules are prepared here before
//! the session commits one transaction, records history and recalculates.
use crate::value::{CellKey, GridArea, SheetId};
use crate::{Axis, CellEdit, HostState, SheetEdit, SheetKey, SheetName, SheetVisibility};
use std::collections::{BTreeMap, BTreeSet};

/// Sort columns in priority order, with an independent direction per column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SortKey {
    /// One-based worksheet column to compare.
    pub column: u32,
    /// Whether to put larger values first.
    pub descending: bool,
}

/// Optional first-key custom order or bounded case-sensitive text keys.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SortOptions {
    /// Whether case affects supported text comparisons.
    pub case_sensitive: bool,
    /// Explicit text order for the first sort key. An empty list uses the default ordering.
    pub custom_order: Vec<String>,
}

/// Change to a worksheet or its tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SheetChange {
    /// Append a worksheet.
    Add {
        /// Name of the new worksheet.
        name: String,
    },
    /// Append a chart sheet (dc-oxh8w.23); its chart is created by a `ChartCreateEmpty` or `ChartMove` command.
    AddChart {
        /// Name of the new chart sheet.
        name: String,
    },
    /// Rename the selected worksheet.
    Rename {
        /// New worksheet name.
        name: String,
    },
    /// Change the selected worksheet's tab position.
    Move {
        /// One-based destination position in workbook sheet order.
        position: u32,
    },
    /// Remove the selected worksheet.
    Remove,
    /// Hide the selected worksheet.
    Hide,
    /// Make the selected worksheet visible.
    Show,
    /// Set the worksheet's visibility, including very hidden (`Worksheet.Visible`, dc-oxh8w.47).
    SetVisibility(SheetVisibility),
}

/// Edit to a worksheet's conditional formatting rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConditionalChange {
    /// Add a cell, formula, statistical or duplicate condition with a solid fill.
    Condition {
        /// Rule kind understood by the conditional formatting builder.
        kind: String,
        /// Worksheet ranges to which the rule applies, expressed as reference text.
        applies: String,
        /// Solid fill color as an RGB string.
        color: String,
        /// Formula or operand text in the order required by the rule.
        formulas: Vec<String>,
        /// Comparison operator for rules that take one.
        operator: String,
        /// Threshold used by ranking rules.
        rank: u32,
        /// Whether a ranking threshold is interpreted as a percentage.
        percent: bool,
        /// Whether a match stops evaluation of lower-priority rules.
        stop: bool,
    },
    /// Append a preset rule with a solid fill.
    Add {
        /// Rule kind understood by the conditional formatting builder.
        kind: String,
        /// Worksheet ranges to which the rule applies, expressed as reference text.
        applies: String,
        /// Solid fill color as an RGB string.
        color: String,
    },
    /// Change the selected rule's ranges, formulas and stop behavior.
    Update {
        /// Worksheet ranges to which the rule applies, expressed as reference text.
        applies: String,
        /// Formula or operand text in the order required by the rule.
        formulas: Vec<String>,
        /// Whether a match stops evaluation of lower-priority rules.
        stop: bool,
    },
    /// Move the selected rule in priority order.
    Move {
        /// Whether to move toward higher priority.
        up: bool,
    },
    /// Delete the selected rule.
    Remove,
    /// Replace the differential format (`dxf`) the selected rule applies: what a macro sets through
    /// `FormatCondition.Interior` and `.Font` (dc-tqcvu). The rule must own its format; one shared with
    /// another rule is refused rather than changed under that rule.
    Format {
        /// The rule's new differential format.
        dxf: Box<calc_doc_model::DifferentialFormat>,
    },
}
/// Authored workbook commands submitted through [`crate::HostSession::dispatch`].
///
/// Add a worksheet to an in-memory workbook and check the new sheet count.
///
/// ```
/// use std::sync::Arc;
/// use calc_host::{
///     HostOptions, HostSession, MemorySource, OpenPolicy, SheetChange, SkinIntent,
///     WorkbookCommand, blank_package,
/// };
///
/// let mut session = HostSession::open(
///     Arc::new(MemorySource::new(blank_package())),
///     &OpenPolicy::default(),
///     HostOptions::default(),
/// ).unwrap();
/// let before = session.host().model().sheets().len();
/// let outcome = session.dispatch(SkinIntent::Command(WorkbookCommand::Sheet {
///     sheet: None,
///     change: SheetChange::Add { name: "Forecast".into() },
/// }));
/// assert!(outcome.accepted());
/// assert_eq!(session.host().model().sheets().len(), before + 1);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkbookCommand {
    /// Refresh the workbook's supported data sources and dependent model state.
    RefreshAll {
        /// Workbook revision against which this command was prepared.
        revision: u64,
    },
    /// Request a refresh of a named workbook connection.
    RefreshConnection {
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Name of the workbook connection.
        name: String,
    },
    /// Create a PivotTable with one row field and one value field.
    PivotCreate {
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Source area with field names in its first row.
        source: GridArea,
        /// Reference text for the upper-left cell of the PivotTable output.
        destination: String,
        /// Zero-based source field index for row labels.
        row: u32,
        /// Zero-based source field index to aggregate.
        value: u32,
        /// Aggregation token such as `sum`, `count`, or `average`.
        subtotal: String,
    },
    /// Create a PivotTable with ordered row, column and value fields.
    PivotCreateLayout {
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Source area with field names in its first row.
        source: GridArea,
        /// Reference text for the upper-left cell of the PivotTable output.
        destination: String,
        /// Zero-based source field indices for the row labels.
        rows: Vec<u32>,
        /// Zero-based source field indices for the column labels.
        columns: Vec<u32>,
        /// Pairs of zero-based source field index and aggregation token.
        values: Vec<(u32, String)>,
    },
    /// Refresh a PivotTable from its source.
    PivotRefresh {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Package part path identifying the target.
        part: String,
    },
    /// Recompute the PivotTable layout from its existing cache records.
    PivotSetLayout {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Package part path identifying the target.
        part: String,
        /// Row field indices in display order. The value `-2` represents the data fields.
        rows: Vec<i32>,
        /// Column field indices in display order. The value `-2` represents the data fields.
        columns: Vec<i32>,
    },
    /// Set the PivotTable cache to retain zero unused items.
    PivotForgetMissingItems {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Package part path identifying the target.
        part: String,
    },
    /// Set the PivotTable cache's unused-item retention limit.
    PivotSetMissingItemsLimit {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Package part path identifying the target.
        part: String,
        /// Explicit limit of zero or 32,500. Use `None` to clear the setting.
        limit: Option<u32>,
    },
    /// Change worksheet print settings.
    PageSetup {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Print settings edit.
        change: crate::PageSetupChange,
    },
    /// Add, change or delete one drawing object of a sheet (dc-piw0.3).
    Shape {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Drawing object edit.
        change: crate::ShapeChange,
    },
    /// Add, change or delete a sparkline (dc-zjsg.7).
    Sparkline {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Sparkline edit.
        change: crate::SparklineChange,
    },
    /// Name and place a chart this session made (`ChartObject.Name`, `.Left/.Top/.Width/.Height`);
    /// positions in points, `None` keeps the value (dc-piw0.2).
    ChartFrame {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Package part path identifying the target.
        part: String,
        /// New drawing name, or `None` to preserve it.
        name: Option<String>,
        /// Left, top, width and height in points, or `None` to preserve them.
        place: Option<[crate::Points; 4]>,
    },
    /// A chart without data at a place in points (`ChartObjects.Add`, `Shapes.AddChart2`); the
    /// drawing identity is the next free one of the sheet (dc-piw0.2).
    ChartCreateEmpty {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Chart type identifier.
        kind: String,
        /// Left, top, width and height in points.
        place: [crate::Points; 4],
        /// `Some`: the chart has this title (`AddChart2`); `None`: no title.
        title: Option<String>,
        /// Whether the chart starts with a visible legend.
        legend: bool,
        /// The first `c16:uniqueId` counter of the chart's series (Excel Q17/Q18: 0 for `AddChart2`; for
        /// `ChartObjects.Add` the series count of Excel's automatic source at Add time).
        unique_start: u32,
    },
    /// `Chart.Location`: a chart this session made moves to another sheet (dc-oxh8w.23): onto a chart sheet
    /// (an absolute frame) or onto a worksheet at `place` in points (left, top, width, height).
    ChartMove {
        /// Sheet that holds the chart now.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Package part path identifying the chart.
        part: String,
        /// The sheet that holds it afterwards.
        to: SheetId,
        /// Left, top, width and height in points on a worksheet; ignored for a chart sheet.
        place: [crate::Points; 4],
    },
    /// `Chart.SetSourceData`: the series of a chart this session made. `rows`: series run along rows.
    /// `headers`: how many leading cells of each series hold its name (a range when more than one);
    /// `categories`: how many leading series lines (rows when `rows`) hold the categories.
    ChartSource {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Package part path identifying the target.
        part: String,
        /// New source range.
        area: GridArea,
        /// Whether each series runs along a row. Otherwise series run along columns.
        rows: bool,
        /// Number of leading cells in each series that supply its name.
        headers: u32,
        /// Number of leading series lines that supply category labels.
        categories: u32,
    },
    /// Create a chart from a worksheet range.
    ChartCreate {
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Range supplying the chart data and any labels.
        area: GridArea,
        /// Chart type identifier.
        kind: String,
        /// Whether each series runs along a row. Otherwise series run along columns.
        rows: bool,
        /// Whether the source includes series names.
        headers: bool,
        /// Whether the source includes category labels.
        categories: bool,
    },
    /// Set the fill of a chart's chart area or plot area (dc-oxh8w.31).
    ChartAreaStyle {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Package part path identifying the target.
        part: String,
        /// The plot area (`true`) or the chart area (`false`).
        plot_area: bool,
        /// Six-digit RGB color, or `None` for the chart's own.
        fill: Option<String>,
    },
    /// Change the color of a chart series.
    ChartSeriesStyle {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Package part path identifying the target.
        part: String,
        /// Zero-based position of the plot within the chart.
        plot: u32,
        /// Series index stored in the chart.
        series: u32,
        /// Six-digit RGB color, or `None` for automatic coloring.
        fill: Option<String>,
    },
    /// Set what a series' data labels show (dc-oxh8w.18); `None` removes the series' own labels.
    ChartSeriesLabels {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Package part path identifying the target.
        part: String,
        /// Zero-based position of the plot within the chart.
        plot: u32,
        /// Series index stored in the chart.
        series: u32,
        /// The labels the series states (`None`: it states none and follows its plot).
        labels: Option<calc_doc_model::ChartLabels>,
    },
    /// Change the text style of a chart element.
    ChartElementTextStyle {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Package part path identifying the target.
        part: String,
        /// Identifier of the chart element to edit.
        target: String,
        /// Text styling for the selected element.
        style: calc_doc_model::ChartTextStyle,
    },
    /// Change the default text style of a chart.
    ChartTextStyle {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Package part path identifying the target.
        part: String,
        /// Default text styling for the chart.
        style: calc_doc_model::ChartTextStyle,
    },
    /// Change the chart type of a plot.
    ChartType {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Package part path identifying the target.
        part: String,
        /// Zero-based position of the plot within the chart.
        plot: u32,
        /// New chart type identifier.
        kind: String,
        /// Whether to stack series for the selected type.
        stacked: bool,
    },
    /// Set the legend position.
    ChartLegend {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Package part path identifying the target.
        part: String,
        /// Legend position token. Use `l`, `r`, `t`, `b`, or `tr`.
        position: String,
    },
    /// Set or clear logarithmic scaling on a value axis.
    ChartLogAxis {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Package part path identifying the target.
        part: String,
        /// Chart axis ID.
        axis: u32,
        /// Logarithm base as numeric text, or `None` for linear scaling.
        base: Option<String>,
    },
    /// Set the limits, major interval and direction of a value axis.
    ChartAxis {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Package part path identifying the target.
        part: String,
        /// Chart axis ID.
        axis: u32,
        /// Finite minimum as numeric text, or `None` for automatic scaling.
        minimum: Option<String>,
        /// Finite maximum as numeric text, or `None` for automatic scaling.
        maximum: Option<String>,
        /// Major interval as numeric text, or `None` for automatic spacing.
        major_unit: Option<String>,
        /// Whether values run in reverse order along the axis.
        reversed: bool,
    },
    /// Set an axis's title text and tick-label number format (dc-oxh8w.12).
    ChartAxisLabels {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Package part path identifying the target.
        part: String,
        /// Chart axis ID.
        axis: u32,
        /// Whether `title` replaces the axis title (`None` there removes it).
        set_title: bool,
        /// The literal axis title text.
        title: Option<String>,
        /// Whether `number_format` replaces the tick-label format (`None` there links it to the source).
        set_number_format: bool,
        /// The tick-label number format code.
        number_format: Option<String>,
    },
    /// Add, edit or remove a series from a plot.
    ChartSeries {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Package part path identifying the target.
        part: String,
        /// Zero-based position of the plot within the chart.
        plot: u32,
        /// Existing chart series index, or `None` to add a series.
        series: Option<u32>,
        /// Optional series name text.
        name: Option<String>,
        /// Optional source reference for the categories or X values.
        categories: Option<String>,
        /// Optional source reference for the series values.
        values: Option<String>,
        /// Whether to remove the identified series.
        remove: bool,
    },
    /// Change the title or legend visibility of a chart.
    ChartProperties {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Package part path identifying the target.
        part: String,
        /// New title text when `set_title` is true. Use `None` to clear it.
        title: Option<String>,
        /// Whether to apply the title edit.
        set_title: bool,
        /// New legend visibility, or `None` to preserve it.
        legend: Option<bool>,
    },
    /// Append the worksheets of an OOXML template as one undoable transaction.
    InsertTemplate {
        /// OOXML template package bytes.
        bytes: std::sync::Arc<[u8]>,
        /// Optional template name.
        name: Option<String>,
        /// Workbook revision against which this command was prepared.
        revision: u64,
    },
    /// Apply a review action at a worksheet cell.
    ReviewAction {
        /// Cell to which the review entry is attached.
        at: CellKey,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Review operation to apply.
        action: crate::ReviewAction,
    },
    /// Replace the text of an existing review entry.
    Review {
        /// Cell to which the review entry is attached.
        at: CellKey,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Index of the review entry to edit.
        index: u32,
        /// Replacement text.
        text: String,
    },
    /// Apply a defined-name edit.
    Name(crate::NameEdit),
    /// Change the filter for a worksheet table.
    TableFilter {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Package part path identifying the target.
        part: String,
        /// Filter edit for the identified table.
        change: crate::FilterChange,
    },
    /// Apply a worksheet table edit.
    Table {
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Table operation to apply.
        change: crate::TableChange,
    },
    /// Change the worksheet's range filter.
    AutoFilter {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Worksheet range-filter edit.
        change: crate::FilterChange,
    },
    /// Apply direct formatting to the specified ranges.
    FormatCells {
        /// Ranges receiving the formatting.
        areas: Vec<GridArea>,
        /// Formatting properties to change.
        patch: crate::FormatPatch,
    },
    /// Merge the ranges (each row of them separately with `across`).
    MergeCells {
        /// Ranges to merge.
        areas: Vec<GridArea>,
        /// Merge each row of a range separately.
        across: bool,
    },
    /// Unmerge every merged area meeting the ranges.
    UnmergeCells {
        /// Ranges to unmerge.
        areas: Vec<GridArea>,
    },
    /// Apply a worksheet tab change.
    Sheet {
        /// Target worksheet ID for an existing tab.
        sheet: Option<SheetId>,
        /// Worksheet operation to apply.
        change: SheetChange,
    },
    /// Change a worksheet's row or column outline.
    Outline {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Whether the outline covers rows. Otherwise it covers columns.
        rows: bool,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Outline operation to apply.
        change: crate::OutlineChange,
    },
    /// Change the size or visibility of consecutive rows or columns.
    AxisFormat {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Whether to format rows. Otherwise format columns.
        rows: bool,
        /// One-based index of the first affected row or column.
        first: u32,
        /// Number of consecutive rows or columns to format.
        count: u32,
        /// Row height in points or column width in characters, encoded as binary64 bits.
        /// Zero hides the bands. A positive size shows them unless `hidden` overrides it.
        /// Use `None` to preserve sizes.
        size_bits: Option<u64>,
        /// Explicit hidden state. Use `None` to leave visibility to the size edit.
        hidden: Option<bool>,
    },
    /// Apply content-fit sizes while preserving each band's visibility.
    FitContents {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Whether to resize rows. Otherwise resize columns.
        rows: bool,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Pairs of band index and fitted size encoded as binary64 bits.
        sizes: Vec<(u32, u64)>,
    },
    /// Set the number of leading rows and columns that stay visible while scrolling.
    FreezePanes {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Number of leading rows to freeze.
        rows: u32,
        /// Number of leading columns to freeze.
        columns: u32,
    },
    /// Set the worksheet's split panes. Zero in both dimensions clears an existing split.
    SplitPanes {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Horizontal split position in twentieths of a point, encoded as binary64 bits.
        x: u64,
        /// Vertical split position in twentieths of a point, encoded as binary64 bits.
        y: u64,
        /// Row coordinate of the split's top-left visible cell.
        row: u32,
        /// Column coordinate of the split's top-left visible cell.
        column: u32,
    },
    /// Apply a conditional formatting edit on a worksheet.
    Conditional {
        /// Worksheet that owns the target.
        sheet: SheetId,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Rule index for an edit to an existing rule.
        index: Option<u32>,
        /// Conditional formatting operation to apply.
        change: ConditionalChange,
    },
    /// Replace matching cell content.
    Replace {
        /// Search criteria and scope.
        options: crate::search::SearchOptions,
        /// Text to substitute for each match.
        replacement: String,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Restrict replacement to this cell, or use `None` to replace throughout the search scope.
        only: Option<CellKey>,
    },
    /// Sort rows by ordered column keys with optional text comparison settings.
    SortRowsWithOptions {
        /// Range whose rows will be sorted.
        area: GridArea,
        /// Sort keys in comparison priority order.
        keys: Vec<SortKey>,
        /// Whether the first row stays in place as a header.
        header: bool,
        /// Workbook revision against which this command was prepared.
        revision: u64,
        /// Text comparison options.
        options: SortOptions,
    },
    /// Sort rows by ordered column keys using the default options.
    SortRows {
        /// Range whose rows will be sorted.
        area: GridArea,
        /// Sort keys in comparison priority order.
        keys: Vec<SortKey>,
        /// Whether the first row stays in place as a header.
        header: bool,
        /// Workbook revision against which this command was prepared.
        revision: u64,
    },
    /// Apply a prepared paste plan.
    Paste(crate::paste::PastePlan),
    /// Insert or delete cells and shift neighboring cells along the selected axis.
    ShiftCells {
        /// Range selecting the cells to insert or delete.
        area: GridArea,
        /// Whether to shift along rows. Otherwise shift along columns.
        down: bool,
        /// Whether to insert cells. Otherwise delete cells.
        insert: bool,
        /// Whether inserted cells take their format from the cells right of or below them (Excel's
        /// xlFormatFromRightOrBelow) instead of the left or above.
        from_after: bool,
    },
}

pub(crate) fn prepare(
    host: &HostState,
    keys: &mut BTreeSet<SheetKey>,
    command: WorkbookCommand,
) -> Result<Vec<CellEdit>, String> {
    match command {
        WorkbookCommand::RefreshConnection { revision, name } => {
            if revision != host.revision().get() {
                return Err(
                    "The workbook changed before refreshing the connection (dc-8wp.18)".into(),
                );
            }
            let model = host
                .model()
                .definitions()
                .data_model()
                .ok_or("The workbook has no data model (dc-8wp.18)")?;
            if model
                .model_connection
                .as_ref()
                .is_none_or(|held| !calc_value::workbook_names_equal(held, &name))
            {
                return Err("Refreshing this connection is not modelled (dc-8wp.21)".into());
            }
            // Excel's model-connection refresh leaves the snapshot unchanged; Refresh All updates it.
            Ok(Vec::new())
        }
        WorkbookCommand::RefreshAll { revision } => {
            crate::facts::prepare_model_refresh(host, revision)
        }
        WorkbookCommand::PivotCreate {
            revision,
            source,
            destination,
            row,
            value,
            subtotal,
        } => crate::pivot::prepare_create(
            host,
            revision,
            source,
            &destination,
            row,
            value,
            &subtotal,
        ),
        WorkbookCommand::PivotCreateLayout {
            revision,
            source,
            destination,
            rows,
            columns,
            values,
        } => crate::pivot::prepare_create_layout(
            host,
            revision,
            source,
            &destination,
            &rows,
            &columns,
            &values,
        ),
        WorkbookCommand::PivotRefresh {
            sheet,
            revision,
            part,
        } => crate::pivot::prepare_refresh(host, sheet, revision, &part),
        WorkbookCommand::PivotSetLayout {
            sheet,
            revision,
            part,
            rows,
            columns,
        } => crate::pivot::prepare_set_layout(host, sheet, revision, &part, rows, columns),
        WorkbookCommand::PivotForgetMissingItems {
            sheet,
            revision,
            part,
        } => crate::pivot::prepare_forget_missing_items(host, sheet, revision, &part),
        WorkbookCommand::PivotSetMissingItemsLimit {
            sheet,
            revision,
            part,
            limit,
        } => crate::pivot::prepare_missing_items_limit(host, sheet, revision, &part, limit),
        WorkbookCommand::PageSetup {
            sheet,
            revision,
            change,
        } => crate::page_setup::prepare(host, sheet, revision, change),
        WorkbookCommand::Shape {
            sheet,
            revision,
            change,
        } => crate::shapes::prepare(host, sheet, revision, change),
        WorkbookCommand::Sparkline { sheet, revision, change } => crate::sparklines::prepare(host, sheet, revision, change),
        WorkbookCommand::ChartFrame {
            sheet,
            revision,
            part,
            name,
            place,
        } => crate::chart::prepare_frame(host, sheet, revision, &part, name, place),
        WorkbookCommand::ChartMove { sheet, revision, part, to, place } => {
            crate::chart::prepare_move(host, sheet, revision, &part, to, place.map(crate::Points::get))
        }
        WorkbookCommand::ChartCreateEmpty {
            sheet,
            revision,
            kind,
            place,
            title,
            legend,
            unique_start,
        } => {
            let [left, top, width, height] = place.map(crate::Points::get);
            if ![left, top, width, height]
                .into_iter()
                .all(|v| v.is_finite() && (0.0..=1.0e7).contains(&v))
                || width < 0.0
                || height < 0.0
            {
                return Err("Use a position and a size on the worksheet (dc-piw0.2)".into());
            }
            let id = host.next_drawing_id(sheet);
            crate::chart::prepare_create_empty(
                host,
                sheet,
                revision,
                &kind,
                crate::chart::NewChart {
                    anchor: if crate::chart::is_chart_sheet(host, sheet) {
                        crate::chart::CHART_SHEET_FRAME
                    } else {
                        calc_doc_model::ChartAnchor::TwoCell {
                            from: host.point_at(sheet, left, top),
                            to: host.point_at(sheet, left + width, top + height),
                        }
                    },
                    drawing_id: Some(id),
                    unique_start,
                    auto_title_deleted: false,
                    title,
                    legend,
                },
            )
        }
        WorkbookCommand::ChartSource {
            sheet,
            revision,
            part,
            area,
            rows,
            headers,
            categories,
        } => crate::chart::prepare_set_source(
            host, sheet, revision, &part, area, rows, headers, categories,
        ),
        WorkbookCommand::ChartCreate {
            revision,
            area,
            kind,
            rows,
            headers,
            categories,
        } => crate::chart::prepare_create(host, revision, area, &kind, rows, headers, categories),
        WorkbookCommand::ChartAreaStyle {
            sheet,
            revision,
            part,
            plot_area,
            fill,
        } => crate::chart::prepare_area_style(host, sheet, revision, &part, plot_area, fill),
        WorkbookCommand::ChartSeriesStyle {
            sheet,
            revision,
            part,
            plot,
            series,
            fill,
        } => crate::chart::prepare_series_style(host, sheet, revision, &part, plot, series, fill),
        WorkbookCommand::ChartSeriesLabels {
            sheet,
            revision,
            part,
            plot,
            series,
            labels,
        } => crate::chart::prepare_series_labels(host, sheet, revision, &part, plot, series, labels),
        WorkbookCommand::ChartElementTextStyle {
            sheet,
            revision,
            part,
            target,
            style,
        } => crate::chart::prepare_element_text_style(host, sheet, revision, &part, &target, style),
        WorkbookCommand::ChartTextStyle {
            sheet,
            revision,
            part,
            style,
        } => crate::chart::prepare_text_style(host, sheet, revision, &part, style),
        WorkbookCommand::ChartType {
            sheet,
            revision,
            part,
            plot,
            kind,
            stacked,
        } => crate::chart::prepare_type(host, sheet, revision, &part, plot, &kind, stacked),
        WorkbookCommand::ChartLegend {
            sheet,
            revision,
            part,
            position,
        } => crate::chart::prepare_legend(host, sheet, revision, &part, &position),
        WorkbookCommand::ChartLogAxis {
            sheet,
            revision,
            part,
            axis,
            base,
        } => crate::chart::prepare_log_axis(host, sheet, revision, &part, axis, base),
        WorkbookCommand::ChartAxis {
            sheet,
            revision,
            part,
            axis,
            minimum,
            maximum,
            major_unit,
            reversed,
        } => crate::chart::prepare_axis(
            host, sheet, revision, &part, axis, minimum, maximum, major_unit, reversed,
        ),
        WorkbookCommand::ChartAxisLabels {
            sheet,
            revision,
            part,
            axis,
            set_title,
            title,
            set_number_format,
            number_format,
        } => crate::chart::prepare_axis_labels(
            host, sheet, revision, &part, axis, set_title.then_some(title), set_number_format.then_some(number_format),
        ),
        WorkbookCommand::ChartSeries {
            sheet,
            revision,
            part,
            plot,
            series,
            name,
            categories,
            values,
            remove,
        } => crate::chart::prepare_series(
            host, sheet, revision, &part, plot, series, name, categories, values, remove,
        ),
        WorkbookCommand::ChartProperties {
            sheet,
            revision,
            part,
            title,
            set_title,
            legend,
        } => {
            crate::chart::prepare_properties(host, sheet, revision, &part, title, set_title, legend)
        }
        WorkbookCommand::InsertTemplate { .. } => {
            Err("Template insertion must use the session transaction (dc-bctq)".into())
        }
        WorkbookCommand::ReviewAction {
            at,
            revision,
            action,
        } => crate::review::prepare(host, at, revision, action),
        WorkbookCommand::Review {
            at,
            revision,
            index,
            text,
        } => crate::review::prepare(
            host,
            at,
            revision,
            crate::ReviewAction::Edit { index, text },
        ),

        WorkbookCommand::Outline {
            sheet,
            rows,
            revision,
            change,
        } => crate::outline::prepare(host, sheet, rows, revision, change),
        WorkbookCommand::SplitPanes {
            sheet,
            revision,
            x,
            y,
            row,
            column,
        } => {
            if revision != host.revision().get() {
                return Err("The workbook changed before splitting (dc-971p.16)".into());
            }
            if host.model().sheet_by_id(sheet).is_none() {
                return Err("The worksheet no longer exists".into());
            }
            let (x, y) = (f64::from_bits(x), f64::from_bits(y));
            if x == 0.
                && y == 0.
                && !matches!(
                    host.model().definitions().pane_layout(sheet),
                    Some(calc_doc_model::PaneLayout::Split(_))
                )
            {
                return Ok(Vec::new());
            }
            let layout = if x == 0. && y == 0. {
                None
            } else {
                Some(calc_doc_model::PaneLayout::Split(
                    calc_doc_model::SplitPanes::new(x, y, row, column)
                        .ok_or("Invalid split position (dc-971p.16)")?,
                ))
            };
            Ok(vec![CellEdit::PaneLayout { sheet, layout }])
        }
        WorkbookCommand::TableFilter {
            sheet,
            revision,
            part,
            change,
        } => crate::filter::prepare_for(host, sheet, revision, Some(&part), change),
        WorkbookCommand::Table { revision, change } => {
            crate::tables::prepare(host, revision, change)
        }
        WorkbookCommand::AutoFilter {
            sheet,
            revision,
            change,
        } => crate::filter::prepare(host, sheet, revision, change),
        WorkbookCommand::SortRowsWithOptions {
            area,
            keys,
            header,
            revision,
            options,
        } => sort_rows(host, area, &keys, header, revision, &options),
        WorkbookCommand::SortRows {
            area,
            keys,
            header,
            revision,
        } => sort_rows(host, area, &keys, header, revision, &SortOptions::default()),
        WorkbookCommand::Name(edit) => crate::names::prepare(host, edit),
        WorkbookCommand::FormatCells { areas, patch } => {
            crate::format::prepare(host, &areas, &patch)
        }
        WorkbookCommand::MergeCells { areas, across } => {
            crate::merge::prepare_merge(host, &areas, across)
        }
        WorkbookCommand::UnmergeCells { areas } => crate::merge::prepare_unmerge(host, &areas),
        WorkbookCommand::Sheet { sheet, change } => sheet_edits(host, keys, sheet, change),
        WorkbookCommand::AxisFormat {
            sheet,
            rows,
            first,
            count,
            size_bits,
            hidden,
        } => axis_format_edits(host, sheet, rows, first, count, size_bits, hidden),
        WorkbookCommand::FitContents {
            sheet,
            rows,
            revision,
            sizes,
        } => {
            if revision != host.revision().get() {
                return Err("The workbook changed while fitting; try again".into());
            }
            if sizes.len() > 16384 {
                return Err("Fit at most 16384 rows or columns at once".into());
            }
            let axis = if rows { Axis::Rows } else { Axis::Columns };
            let mut edits = Vec::new();
            let mut seen = BTreeSet::new();
            for (index, size) in sizes {
                if !seen.insert(index) {
                    return Err("A fitted band was repeated".into());
                }
                let hidden = host
                    .model()
                    .definitions()
                    .axis_layouts()
                    .get(sheet, axis, index)
                    .hidden();
                edits.extend(axis_format_edits(
                    host,
                    sheet,
                    rows,
                    index,
                    1,
                    Some(size),
                    Some(hidden),
                )?);
            }
            Ok(edits)
        }
        WorkbookCommand::FreezePanes {
            sheet,
            rows,
            columns,
        } => {
            if host.model().sheet_by_id(sheet).is_none() {
                return Err("The sheet is unavailable".into());
            }
            let panes = crate::FrozenPanes::new(rows, columns);
            if panes.is_none() && (rows != 0 || columns != 0) {
                return Err("The frozen boundary is outside the worksheet".into());
            }
            Ok(if panes == host.model().definitions().frozen_panes(sheet) {
                Vec::new()
            } else {
                vec![CellEdit::FrozenPanes { sheet, panes }]
            })
        }
        WorkbookCommand::Conditional {
            sheet,
            revision,
            index,
            change,
        } => conditional_edits(host, sheet, revision, index, change),
        WorkbookCommand::Replace {
            options,
            replacement,
            revision,
            only,
        } => {
            if revision != host.revision().get() {
                return Err("The workbook changed; run Find again before replacing".into());
            }
            let edits = crate::search::replace_edits(host, &options, &replacement, only)?;
            if edits.is_empty() {
                return Err("No entries changed".into());
            }
            Ok(edits)
        }
        WorkbookCommand::Paste(plan) => plan.into_edits(host),
        WorkbookCommand::ShiftCells {
            area,
            down,
            insert,
            from_after,
        } => {
            crate::shift::shift_cell_edits_from(host, area, down, insert, from_after)
        }
    }
}

fn sheet_edits(
    host: &HostState,
    sheet_keys: &mut BTreeSet<SheetKey>,
    sheet: Option<SheetId>,
    change: SheetChange,
) -> Result<Vec<CellEdit>, String> {
    use SheetChange as C;
    let model = host.model();
    let id = sheet;
    let require = || {
        id.filter(|id| model.sheet_by_id(*id).is_some())
            .ok_or_else(|| "The worksheet is unavailable".to_owned())
    };
    let name = |text: String| -> Result<SheetName, String> {
        if text.eq_ignore_ascii_case("History") {
            return Err("History is reserved by Excel; choose another worksheet name".to_owned());
        }
        SheetName::new(crate::value::ExcelText::from_utf16_code_units(
            text.encode_utf16().collect(),
        ))
        .map_err(|e| e.to_string())
    };
    let edit = match change {
        C::Add { name: text } => {
            let key = (1u32..)
                .map(SheetKey::new)
                .find(|key| {
                    !sheet_keys.contains(key) && model.sheets().iter().all(|s| s.key() != *key)
                })
                .ok_or("No sheet keys remain")?;
            sheet_keys.insert(key);
            SheetEdit::Insert {
                key,
                name: name(text)?,
                position: std::num::NonZeroU32::new(model.sheets().len() as u32 + 1).unwrap(),
            }
        }
        C::AddChart { name: text } => {
            let key = (1u32..)
                .map(SheetKey::new)
                .find(|key| {
                    !sheet_keys.contains(key) && model.sheets().iter().all(|s| s.key() != *key)
                })
                .ok_or("No sheet keys remain")?;
            sheet_keys.insert(key);
            SheetEdit::InsertChartSheet {
                key,
                name: name(text)?,
                position: std::num::NonZeroU32::new(model.sheets().len() as u32 + 1).unwrap(),
            }
        }
        C::Rename { name: text } => SheetEdit::Rename {
            sheet: require()?,
            name: name(text)?,
        },
        C::Move { position } => {
            if position == 0 || position as usize > model.sheets().len() {
                return Err("Choose a worksheet position in the workbook".to_owned());
            }
            SheetEdit::Move {
                sheet: require()?,
                position: std::num::NonZeroU32::new(position).unwrap(),
            }
        }
        C::Remove => SheetEdit::Remove { sheet: require()? },
        C::SetVisibility(visibility) => SheetEdit::SetVisibility { sheet: require()?, visibility },
        C::Hide | C::Show => {
            let sheet = require()?;
            if model.sheet_by_id(sheet).unwrap().visibility() == SheetVisibility::VeryHidden {
                return Err("This sheet is VeryHidden; unhiding it is unavailable here".to_owned());
            }
            SheetEdit::SetVisibility {
                sheet,
                visibility: if change == C::Hide {
                    SheetVisibility::Hidden
                } else {
                    SheetVisibility::Visible
                },
            }
        }
    };
    Ok(vec![CellEdit::Sheet(edit)])
}

pub use calc_value::stored_axis_size;

pub(crate) fn axis_format_edits(
    host: &HostState,
    sheet: SheetId,
    rows: bool,
    first: u32,
    count: u32,
    size_bits: Option<u64>,
    hidden: Option<bool>,
) -> Result<Vec<CellEdit>, String> {
    if host.model().sheet_by_id(sheet).is_none() {
        return Err("The sheet is unavailable".into());
    }
    let loaded = host.loaded().sheet(sheet);
    if loaded.is_some_and(|s| s.geometry().outline_unsupported) {
        return Err("Imported outline metadata is not modelled for dimension edits (dc-971p.39); nothing changed".into());
    }
    if loaded.is_some_and(|s| s.protected()) {
        return Err("Dimensions on a protected sheet are not available here".to_owned());
    }
    let axis = if rows {
        crate::Axis::Rows
    } else {
        crate::Axis::Columns
    };
    let last = first
        .checked_add(count)
        .and_then(|n| n.checked_sub(1))
        .filter(|n| first >= 1 && count > 0 && *n <= axis.size())
        .ok_or("The dimensions are outside the worksheet")?;
    if count > 16_384 {
        return Err("Change at most 16384 rows or columns at once".to_owned());
    }
    if rows
        && (hidden.is_some() || size_bits.is_some())
        && host
            .model()
            .definitions()
            .auto_filter(sheet)
            .is_some_and(|f| {
                f.unsupported && first <= f.area.last().row() && last > f.area.first().row()
            })
    {
        return Err("Row visibility for imported filter metadata is not modelled (dc-971p.37); nothing changed".into());
    }
    if rows
        && host
            .model()
            .definitions()
            .hidden_rows(sheet, first, last)
            .any(|(row, why)| {
                why == crate::RowHidden::Filtered
                    && (!host
                        .model()
                        .definitions()
                        .auto_filter(sheet)
                        .is_some_and(|f| f.filtered_rows.contains(&row))
                        || host.model().definitions().tables().iter().any(|t| {
                            t.sheet == sheet
                                && row > t.area.first().row()
                                && row <= t.area.last().row()
                        }))
            })
    {
        return Err(
            "Rows hidden by an imported table filter cannot be resized or shown here (dc-971p.37)"
                .into(),
        );
    }
    let size = size_bits
        .map(|b| stored_axis_size(rows, f64::from_bits(b)))
        .transpose()
        .map_err(str::to_owned)?;
    let mut edits = Vec::new();
    for index in first..=last {
        let old = host
            .model()
            .definitions()
            .axis_layouts()
            .get(sheet, axis, index);
        let mut new = old;
        if let Some(size) = size {
            new = match size {
                None => old.with_hidden(true),
                Some(n) => old
                    .with_size(Some(n), !rows)
                    .expect("a checked size")
                    .with_hidden(false),
            };
        }
        if let Some(hidden) = hidden {
            new = if !hidden && new.size() == Some(0.0) {
                new.with_size(None, false)
                    .expect("default size")
                    .with_hidden(false)
            } else {
                new.with_hidden(hidden)
            };
        }
        if old != new {
            edits.push(CellEdit::AxisFormat(
                crate::AxisFormatEdit::new(sheet, axis, index, new).expect("checked band indices"),
            ));
        }
    }
    // Drawing objects that move but don't size, or neither, keep their size (dc-zjsg.4).
    let kept = crate::shapes::kept_sizes(host, sheet, &edits);
    edits.extend(kept);
    Ok(edits)
}

pub(crate) fn conditional_edits(
    host: &HostState,
    sheet: crate::value::SheetId,
    revision: u64,
    index: Option<u32>,
    change: ConditionalChange,
) -> Result<Vec<CellEdit>, String> {
    if revision != host.revision().get() {
        return Err("Rules changed; reopen the current rule and try again".into());
    }
    if host.loaded().sheet(sheet).is_some_and(|s| s.protected()) {
        return Err("Worksheet protection prevents conditional rule edits".into());
    }
    if host
        .loaded()
        .sheet(sheet)
        .is_some_and(|s| s.conditional_formats().iter().any(|b| b.unread_container))
    {
        return Err("This worksheet has additional conditional-rule metadata; editing its rules is unavailable and the file is preserved (dc-971p.8)".into());
    }
    let mut rules = host.model().definitions().conditional_rules(sheet).to_vec();
    let ranges = |text: &str| -> Result<Vec<GridArea>, String> {
        let at = CellKey::new(sheet, 1, 1).unwrap();
        let mut areas = Vec::new();
        for token in text.split_whitespace() {
            match host.resolve_reference_text(at, token) {
                crate::ReferenceTarget::Area(a) if a.sheet() == sheet => areas.push(a),
                _ => {
                    return Err("Applies to needs space-separated ranges on this worksheet".into());
                }
            }
        }
        if areas.is_empty() || areas.len() > 128 {
            return Err("Choose between 1 and 128 ranges".into());
        }
        Ok(areas)
    };
    let mut staged_styles = None;
    let added = match &change {
        ConditionalChange::Add {
            kind,
            applies,
            color,
        } => Some(crate::new_conditional_rule(
            ranges(applies)?,
            kind,
            color.trim_start_matches('#'),
        )?),
        ConditionalChange::Condition {
            kind,
            applies,
            color,
            formulas,
            operator,
            rank,
            percent,
            stop,
        } => {
            let areas = ranges(applies)?;
            // An empty colour is a rule without formatting (what `FormatConditions.Add` makes): its own
            // format, never shared, so a later `Format` change touches this rule only.
            let unformatted = color.trim().is_empty();
            let dxf = if unformatted {
                calc_doc_model::DifferentialFormat::default()
            } else {
                let color = crate::format::color(color)?.ok_or("Choose a fill colour")?;
                calc_doc_model::DifferentialFormat {
                    fill: Some(calc_doc_model::Fill {
                        pattern_type: Some("solid".into()),
                        // Excel takes a differential format's solid fill from bgColor.
                        bg_color: Some(color),
                        ..Default::default()
                    }),
                    ..Default::default()
                }
            };
            let mut styles = (**host.styles()).clone();
            let id = styles
                .differential_formats
                .iter()
                .position(|old| !unformatted && old == &dxf)
                .unwrap_or_else(|| {
                    let id = styles.differential_formats.len();
                    styles.differential_formats.push(dxf);
                    id
                });
            let mut converted = Vec::new();
            for formula in formulas {
                let text = crate::FormulaText::new(crate::value::ExcelText::from_utf16_code_units(
                    formula
                        .trim()
                        .trim_start_matches('=')
                        .encode_utf16()
                        .collect(),
                ))
                .map_err(|e| e.to_string())?;
                let form = crate::file_form_with_links(
                    &text,
                    host.model().definitions().names(),
                    &host.model().definitions().external_file_names(),
                )
                .map_err(|e| format!("Conditional formula is unavailable: {e:?}"))?;
                converted.push(
                    String::from_utf16_lossy(&form.text)
                        .trim_start_matches('=')
                        .to_owned(),
                );
            }
            let mut source = calc_doc::new_condition_rule(
                areas, kind, operator, converted, *rank, *percent, id as u32,
            )?;
            source.stop = *stop;
            staged_styles = Some(styles);
            Some(source)
        }
        _ => None,
    };
    if let Some(mut source) = added {
        source.priority = rules
            .iter()
            .map(|r| r.priority)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        rules.push(source);
    } else {
        let i = index
            .map(|i| i as usize)
            .filter(|i| *i < rules.len())
            .ok_or("Select a rule first")?;
        let parsed = crate::conditional_rule(&rules[i])?;
        match change {
            ConditionalChange::Update {
                applies,
                formulas,
                stop,
            } => {
                let areas = ranges(&applies)?;
                if parsed.extension_id.is_some() {
                    return Err("Editing extension-backed rules is unavailable; the imported rule is preserved (dc-971p.8)".into());
                }
                let mut formulas = formulas;
                if formulas != rules[i].formulas {
                    if !matches!(parsed.kind.as_str(), "cellIs" | "expression")
                        || formulas.len() != rules[i].formulas.len()
                    {
                        return Err("This rule condition cannot be edited yet (dc-971p.8)".into());
                    }
                    for formula in &mut formulas {
                        let text = crate::FormulaText::new(
                            crate::value::ExcelText::from_utf16_code_units(
                                formula
                                    .trim()
                                    .trim_start_matches('=')
                                    .encode_utf16()
                                    .collect(),
                            ),
                        )
                        .map_err(|e| e.to_string())?;
                        let form = crate::file_form_with_links(
                            &text,
                            host.model().definitions().names(),
                            &host.model().definitions().external_file_names(),
                        )
                        .map_err(|e| format!("Conditional formula is unavailable: {e:?}"))?;
                        *formula = String::from_utf16_lossy(&form.text)
                            .trim_start_matches('=')
                            .to_owned();
                    }
                }
                rules[i].areas = areas;
                rules[i].formulas = formulas;
                rules[i].stop =
                    stop && !matches!(parsed.kind.as_str(), "colorScale" | "dataBar" | "iconSet");
                rules[i].authored = true;
            }
            ConditionalChange::Format { dxf } => {
                let id = parsed
                    .dxf
                    .ok_or("This rule has no format of its own to change (dc-tqcvu)")?
                    as usize;
                let shared = rules.iter().enumerate().any(|(n, other)| {
                    n != i
                        && crate::conditional_rule(other)
                            .map(|r| r.dxf == parsed.dxf)
                            .unwrap_or(true)
                });
                if id < host.loaded().styles().differential_formats.len() {
                    return Err("This rule's format came from the file; changing it needs a new format record (dc-oxh8w.62)".into());
                }
                if shared {
                    return Err("This rule shares its format with another rule; changing it would change both (dc-tqcvu)".into());
                }
                let mut styles = (**host.styles()).clone();
                *styles
                    .differential_formats
                    .get_mut(id)
                    .ok_or("The rule's format is missing from the styles part")? = *dxf;
                rules[i].authored = true;
                staged_styles = Some(styles);
            }
            ConditionalChange::Remove => {
                if parsed.extension_id.is_some() {
                    return Err("Removing extension-backed rules is unavailable; the imported rule is preserved (dc-971p.8)".into());
                }
                rules.remove(i);
            }
            ConditionalChange::Move { up } => {
                let mut order = (0..rules.len()).collect::<Vec<_>>();
                order.sort_by_key(|&n| rules[n].priority);
                let old = order.iter().position(|&n| n == i).unwrap();
                let destination = if up {
                    old.saturating_sub(1)
                } else {
                    (old + 1).min(order.len() - 1)
                };
                order.swap(old, destination);
                for (rank, index) in order.into_iter().enumerate() {
                    rules[index].priority = rank as i64 + 1;
                    rules[index].authored = true;
                }
            }
            _ => unreachable!(),
        }
    }
    let mut order = (0..rules.len()).collect::<Vec<_>>();
    order.sort_by_key(|&n| rules[n].priority);
    for (rank, index) in order.into_iter().enumerate() {
        rules[index].priority = rank as i64 + 1;
    }
    let mut edits = vec![CellEdit::ConditionalRules { sheet, rules }];
    if let Some(styles) = staged_styles
        && &styles != host.styles().as_ref()
    {
        edits.insert(0, CellEdit::Styles(std::sync::Arc::new(styles)));
    }
    Ok(edits)
}

/// A bounded row permutation. Ordering comes from the existing evaluator; edits use the same
/// immutable copy snapshots and reference relocation as Paste, in one transaction. Source-only
/// metadata and unsupported qualified references remain refused. Notes and validations follow
/// the same validated permutation without overwriting one row’s metadata with another’s.
fn sort_rows(
    host: &HostState,
    area: GridArea,
    keys: &[SortKey],
    header: bool,
    revision: u64,
    options: &SortOptions,
) -> Result<Vec<CellEdit>, String> {
    let unavailable = |detail: &str| {
        format!("Sorting {detail} is not modelled (dc-971p.38); nothing was changed")
    };
    if revision != host.revision().get() {
        return Err("The workbook changed before sorting; select the range again".into());
    }
    crate::check_destination(host, area, crate::PasteMode::All).map_err(|e| e.to_string())?;
    if keys.is_empty() || keys.len() > 32 {
        return Err("Choose 1 to 32 sort keys".into());
    }
    let mut columns = BTreeSet::new();
    for key in keys {
        if !(area.first().column()..=area.last().column()).contains(&key.column) {
            return Err("Every sort column must lie in the selected range".into());
        }
        if !columns.insert(key.column) {
            return Err("Choose each sort column only once".into());
        }
    }
    let first = area.first().row() + u32::from(header);
    if first > area.last().row() {
        return Err("Select at least one data row below the header".into());
    }
    let data = GridArea::new(
        CellKey::new(area.sheet(), first, area.first().column()).unwrap(),
        area.last(),
    )
    .unwrap();
    check_sorted_merges(host, data, keys)?;
    for table in host
        .model()
        .definitions()
        .tables()
        .iter()
        .filter(|t| t.area.intersection(data).is_some())
    {
        let top = table
            .area
            .first()
            .row()
            .checked_add(table.header_rows)
            .ok_or_else(|| unavailable("invalid table boundaries"))?;
        let bottom = table
            .area
            .last()
            .row()
            .checked_sub(table.totals_rows)
            .ok_or_else(|| unavailable("invalid table boundaries"))?;
        let complete = table.header_rows <= 1
            && table.totals_rows <= 1
            && top <= bottom
            && data.first().row() == top
            && data.last().row() == bottom
            && data.first().column() == table.area.first().column()
            && data.last().column() == table.area.last().column();
        if !complete {
            return Err(unavailable(
                "a partial table or its header/total cells; select the complete data body",
            ));
        }
        if table.opaque
            || table.axis_opaque
            || table.column_formulas.iter().any(Option::is_some)
            || table.filter.as_ref().is_some_and(|f| {
                f.unsupported || !f.columns.is_empty() || !f.filtered_rows.is_empty()
            })
        {
            return Err(unavailable(
                "filtered tables or unread/calculated table metadata",
            ));
        }
    }
    if host
        .model()
        .definitions()
        .arrays()
        .any(|a| a.area().intersection(data).is_some())
        || host
            .engine
            .spills()
            .any(|a| a.area().intersection(data).is_some())
    {
        return Err(unavailable(
            "array formulas; paste their Values into an ordinary range first",
        ));
    }
    if host
        .model()
        .definitions()
        .axis_layouts()
        .outline_groups(area.sheet(), Axis::Rows)
        .iter()
        .any(|group| {
            group.first.min(group.summary) <= data.last().row()
                && group.last.max(group.summary) >= data.first().row()
        })
    {
        return Err(unavailable("outline groups or their summary rows"));
    }
    // Excel Data Sort leaves hidden rows in place. Outline groups have separate rules.
    let mut fixed_rows: BTreeSet<u32> = host
        .model()
        .definitions()
        .auto_filter(area.sheet())
        .filter(|f| !f.unsupported && f.area.intersection(data).is_some())
        .map(|f| {
            f.filtered_rows
                .iter()
                .copied()
                .filter(|r| (first..=area.last().row()).contains(r))
                .collect()
        })
        .unwrap_or_default();
    if let Some(geometry) = host.sheet_geometry(area.sheet()) {
        fixed_rows.extend(
            geometry
                .rows
                .range(first..=area.last().row())
                .filter_map(|(&row, format)| format.hidden.then_some(row)),
        );
    }
    if host
        .model()
        .definitions()
        .unloaded_in(data)
        .next()
        .is_some()
    {
        return Err(unavailable("unloaded cell content"));
    }
    if host
        .model()
        .definitions()
        .auto_filter(area.sheet())
        .is_some_and(|filter| filter.area.intersection(data).is_some() && filter.unsupported)
    {
        return Err(unavailable("AutoFilter ranges with unread settings"));
    }
    if !host.loaded.cell_sort_metadata_supported(area.sheet()) {
        return Err(unavailable("source cell decorations or worksheet objects"));
    }
    if host.axis_edits.iter().any(|e| e.sheet() == area.sheet())
        && host
            .model()
            .definitions()
            .review_notes(area.sheet())
            .iter()
            .any(|n| {
                data.contains(n.at)
                    && matches!(
                        n.source.as_ref(),
                        Some(calc_doc_model::ReviewSource::Note { .. })
                    )
            })
    {
        return Err(unavailable(
            "notes after row or column edits; save and reopen first",
        ));
    }
    if host.axis_edits.iter().any(|e| e.sheet() == area.sheet())
        && !host
            .model()
            .definitions()
            .conditional_rules(area.sheet())
            .is_empty()
    {
        return Err(unavailable(
            "conditional formatting after row or column edits; save and reopen first",
        ));
    }
    let review = host.sheet_review(area.sheet());
    if !review.unsupported.is_empty()
        || host
            .loaded
            .check_sorted_links(area.sheet(), &review.links)
            .is_err()
    {
        return Err(unavailable("cells with unread review fields or hyperlinks"));
    }
    // Pointwise rules follow the same row permutation below; unread and population-based
    // rules retain their refusal. No formula value is inferred from a saved cache.
    if review.conditional_extensions
        || host.loaded.sheet(area.sheet()).is_some_and(|sheet| {
            sheet
                .conditional_formats()
                .iter()
                .any(|block| block.unread_container)
        })
        || !crate::copy_metadata::conditional_sort_supported(host, area.sheet())
    {
        return Err(unavailable("a sheet with conditional formatting"));
    }
    for row in first..=area.last().row() {
        for col in area.first().column()..=area.last().column() {
            let at = CellKey::new(area.sheet(), row, col).unwrap();
            if let Some(crate::CellInput::Formula(formula)) = host.model().cell(at) {
                if !sort_formula_references_supported(host, formula, at, false) {
                    return Err(unavailable("qualified or unsupported formula references"));
                }
            }
        }
    }
    let cells = (first..=data.last().row())
        .flat_map(|row| {
            keys.iter()
                .map(move |key| CellKey::new(area.sheet(), row, key.column).unwrap())
        })
        .collect();
    let page =
        crate::project(host, &crate::ProjectionRequest { cells }).map_err(|e| e.to_string())?;
    if page.cells().iter().any(|cell| {
        !matches!(
            cell.value(),
            crate::ProjectedValue::Empty
                | crate::ProjectedValue::Literal(_)
                | crate::ProjectedValue::Calculated { .. }
        )
    }) {
        return Err(unavailable(
            "keys without current calculated values; calculate the range first",
        ));
    }
    if data.row_count() < 2 {
        return Ok(Vec::new());
    }
    let (case_widths, custom) = sort_option_keys(host, data, keys, &page, options)?;
    let count = data.row_count();
    if count < 2 {
        return Ok(Vec::new());
    }
    let label = sort_label;
    // Empty keys stay last in both directions. Original row index is an explicit tie key.
    // Grid sorting treats all errors as equal. Normalise their codes through the evaluator,
    // whose SORTBY otherwise orders different error codes. No host comparator is introduced.
    let sequence = file_call_name(calc_catalogue::ported::arrays::SEQUENCE);
    let xmatch = file_call_name(calc_catalogue::lookup::XMATCH);
    let mut formula = format!("{}({sequence}({count})", file_call_name(calc_catalogue::ported::arrays::SORTBY));
    let mut sort_arguments = 3usize; // row indexes plus the final tie key/direction
    for (index, key) in keys.iter().enumerate() {
        let range = format!(
            "{}:{}",
            label(key.column, first),
            label(key.column, data.last().row())
        );
        let direction = if key.descending { -1 } else { 1 };
        formula.push_str(&format!(",ISBLANK({range}),1"));
        sort_arguments += 2;
        if case_widths[index] > 0 {
            formula.push_str(&format!(
                ",IF(ISTEXT({range}),1,IF(ISNUMBER({range}),0,IF(ISLOGICAL({range}),2,3))),{direction},\
                IF(ISTEXT({range}),0,IFERROR({range},NA())),{direction}"
            ));
            sort_arguments += 4;
            for position in 1..=case_widths[index] {
                formula.push_str(&format!(
                    ",IF(ISTEXT({range}),IFERROR(CODE(LOWER(MID({range},{position},1))),0),0),{direction},\
                    IF(ISTEXT({range}),IFERROR(CODE(MID({range},{position},1))<91,FALSE),FALSE),{direction}"
                ));
                sort_arguments += 4;
            }
        } else {
            let value = if index == 0 {
                custom
                    .as_ref()
                    .map(|list| format!("IF(ISBLANK({range}),0,{xmatch}({range},{list},0))"))
            } else {
                None
            }
            .unwrap_or_else(|| format!("IFERROR({range},NA())"));
            formula.push_str(&format!(",{value},{direction}"));
            sort_arguments += 2;
        }
    }
    if sort_arguments > 255 {
        return Err(unavailable("case-sensitive keys beyond 255 sort arguments"));
    }
    formula.push_str(&format!(",{sequence}({count}),1)"));
    if formula.encode_utf16().count() > 8192 {
        return Err(unavailable(
            "case-sensitive keys beyond the formula-length limit",
        ));
    }
    let value = calc_engine::DetachedFormula::compile_dynamic(host.model(), &formula, data.first())
        .and_then(|f| f.evaluate(&host.engine, host.model(), data.first()))
        .map_err(|e| format!("Sort could not calculate the row order (dc-971p.38): {e}"))?;
    let calc_value::CoreRef::Array(order) = value.core_ref() else {
        return Err(match value.core_ref() {
            calc_value::CoreRef::Error(code) => format!(
                "Sort could not calculate the row order: {} (dc-971p.38)",
                code.excel_spelling()
            ),
            _ => "Sort did not return a row order (dc-971p.38)".into(),
        });
    };
    if order.shape().rows != count as usize || order.shape().cols != 1 {
        return Err("Sort returned an unexpected row count (dc-971p.38)".into());
    }
    let mut seen = BTreeSet::new();
    let mut edits = Vec::new();
    let mut rows = BTreeMap::new();
    // The order of the whole range, then only its visible rows: a stable sort of the whole
    // restricted to a subset is the sort of that subset. Hidden rows map to themselves.
    let mut order_rows = Vec::with_capacity(count as usize);
    for index in 0..count {
        let Some(calc_value::CoreRef::Number(number)) =
            order.get(index as usize, 0).map(|v| v.core_ref())
        else {
            return Err("Sort did not return numeric row positions (dc-971p.38)".into());
        };
        if number < 1.
            || number > f64::from(count)
            || number.fract() != 0.
            || !seen.insert(number as u32)
        {
            return Err("Sort returned invalid row positions (dc-971p.38)".into());
        }
        order_rows.push(first + number as u32 - 1);
    }
    let targets: Vec<u32> = (first..=data.last().row())
        .filter(|row| !fixed_rows.contains(row))
        .collect();
    let sources: Vec<u32> = order_rows
        .into_iter()
        .filter(|row| !fixed_rows.contains(row))
        .collect();
    for row in &fixed_rows {
        rows.insert(*row, *row);
    }
    for (source_row, target_row) in sources.into_iter().zip(targets) {
        rows.insert(source_row, target_row);
        if source_row == target_row {
            continue;
        }
        let band = |row| {
            GridArea::new(
                CellKey::new(area.sheet(), row, data.first().column()).unwrap(),
                CellKey::new(area.sheet(), row, data.last().column()).unwrap(),
            )
            .unwrap()
        };
        let copy =
            crate::CopySnapshot::capture(host, band(source_row)).map_err(|e| e.to_string())?;
        edits.extend(
            copy.sorted_row_edits(host, band(target_row))
                .map_err(|e| e.to_string())?,
        );
    }
    crate::copy_metadata::permute_rows(host, data, &rows, &mut edits).map_err(|e| e.to_string())?;
    Ok(edits)
}

fn check_sorted_merges(host: &HostState, data: GridArea, keys: &[SortKey]) -> Result<(), String> {
    let merges = host.merged_areas(data.sheet());
    let mut merges = merges.iter().filter(|area| area.intersection(data).is_some());
    let Some(first) = merges.next() else {
        return Ok(());
    };
    let width = first.column_count();
    let start = data.first().column();
    let mut anchors = BTreeSet::new();
    // Excel admits a range tiled by identical merges. For horizontal tiles the
    // row permutation leaves every merge rectangle in place.
    for area in std::iter::once(first).chain(merges) {
        if area.row_count() != 1 {
            return Err("Sorting merged cells that span rows is not modelled (dc-971p.64)".into());
        }
        if area.column_count() != width
            || !data.contains(area.first())
            || !data.contains(area.last())
            || (area.first().column() - start) % width != 0
            || !anchors.insert(area.first())
        {
            return Err("Sorting requires identically sized merged cells that fill the range".into());
        }
    }
    if anchors.len() as u64 * u64::from(width) != data.cell_count() {
        return Err("Sorting requires identically sized merged cells that fill the range".into());
    }
    if keys.iter().any(|key| (key.column - start) % width != 0) {
        return Err("Sorting by a covered column of a merged cell is not modelled (dc-971p.64)".into());
    }
    Ok(())
}

// An admitted qualifier identifies one cell on another worksheet. The reference must
// stay fixed on every axis where the formula can be relocated. Row sorting moves cells
// vertically, while rebasing conditional-format fragments can also move their origin
// horizontally. Use the canonical binder and copy relocator for the check.
pub(crate) fn sort_formula_references_supported(
    host: &HostState,
    formula: &crate::FormulaText,
    at: CellKey,
    columns_may_move: bool,
) -> bool {
    use calc_fml::syntax::{SyntaxChild, SyntaxKind};
    let tree = calc_fml::parse_formula(&format!("={}", formula.text().to_string_lossy()));
    if !tree.diagnostics().is_empty() {
        return false;
    }
    let step = |index, maximum| {
        if index < maximum {
            index + 1
        } else {
            index - 1
        }
    };
    let probe = CellKey::new(
        at.sheet(),
        step(at.row(), crate::value::MAX_ROWS),
        if columns_may_move {
            step(at.column(), crate::value::MAX_COLUMNS)
        } else {
            at.column()
        },
    )
    .unwrap();
    let mut nodes = vec![(tree.root(), false)];
    while let Some((node, mut reference_operator)) = nodes.pop() {
        match node.kind() {
            SyntaxKind::SheetSpan3DReferenceExpr => return false,
            SyntaxKind::RangeExpr
            | SyntaxKind::TrimRangeExpr
            | SyntaxKind::IntersectionExpr
            | SyntaxKind::UnionExpr
            | SyntaxKind::PostfixExpr => reference_operator = true,
            SyntaxKind::QualifiedReferenceExpr => {
                if reference_operator {
                    return false;
                }
                let Some((_, target)) = calc_fml::reference::qualified_name(node) else {
                    return false;
                };
                // '$' distinguishes fixed-axis references from defined names. Binding
                // must also resolve the text to a valid cell.
                if !target.text().contains('$') {
                    return false;
                }
                let source = node.text();
                let source = source.trim();
                let crate::ReferenceTarget::Area(target) = host.resolve_reference_text(at, source)
                else {
                    return false;
                };
                if target.sheet() == at.sheet()
                    || target.row_count() != 1
                    || target.column_count() != 1
                {
                    return false;
                }
                let Some(crate::CellInput::Formula(reference)) =
                    crate::interpret_entry(&format!("={source}")).ok().flatten()
                else {
                    return false;
                };
                if calc_engine::relocate_formula(host.model(), &reference, at, probe).as_ref()
                    != Some(&reference)
                {
                    return false;
                }
            }
            _ => {}
        }
        nodes.extend(node.children().iter().filter_map(|child| match child {
            SyntaxChild::Node(node) => Some((node, reference_operator)),
            _ => None,
        }));
    }
    // A reference operator enclosing a qualifier is refused above. An independent local
    // range elsewhere in the formula does not alter a fixed qualified scalar's binding.
    true
}

fn sort_label(column: u32, row: u32) -> String {
    let mut col = column;
    let mut chars = Vec::new();
    while col > 0 {
        chars.push(char::from(b'A' + ((col - 1) % 26) as u8));
        col = (col - 1) / 26;
    }
    format!("{}{}", chars.into_iter().rev().collect::<String>(), row)
}

/// Additional keys use ordinary worksheet functions in the same detached evaluation as Sort.
/// Case-sensitive ASCII order follows Microsoft's character-by-character a,A,b,B sequence:
/// https://support.microsoft.com/en-us/excel/sort-data-in-a-range-or-table-in-excel
fn sort_option_keys(
    host: &HostState,
    data: GridArea,
    keys: &[SortKey],
    page: &crate::ProjectionPage,
    options: &SortOptions,
) -> Result<(Vec<usize>, Option<String>), String> {
    let unavailable =
        |detail: &str| format!("Sorting {detail} is not modelled (dc-971p.38); nothing changed");
    if options.case_sensitive && !options.custom_order.is_empty() {
        return Err(unavailable("case-sensitive custom lists"));
    }
    let mut widths = vec![0; keys.len()];
    if options.case_sensitive {
        for cell in page.cells() {
            let value = match cell.value() {
                crate::ProjectedValue::Literal(v)
                | crate::ProjectedValue::Calculated { value: v, .. } => Some(v),
                _ => None,
            };
            if let Some(text) = value.and_then(|v| v.as_text()) {
                let units = text.utf16_code_units();
                if units.len() > 52
                    || units
                        .iter()
                        .any(|u| !u8::try_from(*u).is_ok_and(|c| c.is_ascii_alphanumeric()))
                {
                    return Err(unavailable(
                        "case-sensitive labels outside 52 ASCII letters or digits",
                    ));
                }
                let index = keys
                    .iter()
                    .position(|k| k.column == cell.at().column())
                    .expect("projected sort key");
                widths[index] = widths[index].max(units.len());
            }
        }
    }
    let order = &options.custom_order;
    if order.is_empty() {
        return Ok((widths, None));
    }
    if keys[0].descending {
        return Err(
            "Reverse the custom list entries to reverse their order (dc-971p.38); nothing changed"
                .into(),
        );
    }
    if order.len() > 32
        || order.iter().any(String::is_empty)
        || order.join("\n").encode_utf16().count() > 255
    {
        return Err(
            "Enter 1 to 32 nonempty custom entries, at most 255 characters in total (dc-971p.38)"
                .into(),
        );
    }
    let list = format!(
        "{{{}}}",
        order
            .iter()
            .map(|s| format!("\"{}\"", s.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(";")
    );
    let evaluate = |text: &str| {
        calc_engine::DetachedFormula::compile_dynamic(host.model(), text, data.first())
            .and_then(|f| f.evaluate(&host.engine, host.model(), data.first()))
            .map_err(|e| format!("Custom order could not be calculated (dc-971p.38): {e}"))
    };
    let xmatch = file_call_name(calc_catalogue::lookup::XMATCH);
    let unique = evaluate(&format!("{xmatch}({list},{list},0)"))?;
    let distinct =
        match unique.core_ref() {
            calc_value::CoreRef::Array(positions) => positions.shape().rows == order.len()
                && positions.shape().cols == 1
                && positions.iter_row_major().enumerate().all(
                    |(i, v)| matches!(v.core_ref(),calc_value::CoreRef::Number(n)if n==(i+1)as f64),
                ),
            calc_value::CoreRef::Number(n) => order.len() == 1 && n == 1.,
            _ => false,
        };
    if !distinct {
        return Err(
            "Every custom entry must be distinct, ignoring case (dc-971p.38); nothing changed"
                .into(),
        );
    }
    let range = format!(
        "{}:{}",
        sort_label(keys[0].column, data.first().row()),
        sort_label(keys[0].column, data.last().row())
    );
    let ranks = evaluate(&format!("{xmatch}({range},{list},0)"))?;
    let calc_value::CoreRef::Array(ranks) = ranks.core_ref() else {
        return Err("The custom key did not return row positions (dc-971p.38)".into());
    };
    if ranks.shape().rows != data.row_count() as usize || ranks.shape().cols != 1 {
        return Err("The custom key returned an unexpected row count (dc-971p.38)".into());
    }
    for cell in page
        .cells()
        .iter()
        .filter(|c| c.at().column() == keys[0].column)
    {
        if matches!(cell.value(), crate::ProjectedValue::Empty) {
            continue;
        }
        let value = match cell.value() {
            crate::ProjectedValue::Literal(v)
            | crate::ProjectedValue::Calculated { value: v, .. } => v,
            _ => return Err(unavailable("custom keys without current values")),
        };
        let rank = ranks.get((cell.at().row() - data.first().row()) as usize, 0);
        if value.as_text().is_none()
            || !rank.is_some_and(|v| {
                matches!(v.core_ref(), calc_value::CoreRef::Number(n) if n >= 1.0 && n <= order.len() as f64)
            })
        {
            return Err("Every nonblank first-key value must be text included in the custom list (dc-971p.38); nothing changed".into());
        }
    }
    Ok((widths, Some(list)))
}

/// A measured function's name as a file writes its call (`_xlfn.XMATCH`), from the catalogue.
fn file_call_name(id: calc_catalogue::FunctionId) -> String {
    calc_catalogue::file_call_name_by_id(id).expect("a function with a measured file form")
}
