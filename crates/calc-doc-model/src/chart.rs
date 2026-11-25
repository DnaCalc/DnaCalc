//! Authored chart definitions, independent of document I/O and calculation.

/// A point of the sheet: a zero-based column and row, as the drawing part counts them, and the
/// offsets into that cell in EMU (914,400 to the inch; 9,525 to a pixel at 96 dots per inch).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DrawingPoint {
    pub column: u32,
    pub column_offset: i64,
    pub row: u32,
    pub row_offset: i64,
}

/// Where a chart's frame is on the sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChartAnchor {
    /// `xdr:twoCellAnchor`: its top-left and bottom-right corners.
    TwoCell {
        from: DrawingPoint,
        to: DrawingPoint,
    },
    /// `xdr:oneCellAnchor`: its top-left corner and its size in EMU.
    OneCell {
        from: DrawingPoint,
        width: i64,
        height: i64,
    },
    /// `xdr:absoluteAnchor`: its position and size in EMU.
    Absolute {
        x: i64,
        y: i64,
        width: i64,
        height: i64,
    },
}

/// A series' name, categories or values: the formula Excel keeps (`c:f`) and the points it cached
/// with it (`c:pt`, by index; a point the cache leaves out is `None`), or a literal's points.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ChartData {
    pub formula: Option<String>,
    pub cache: Vec<Option<String>>,
    /// Whether the cache is numbers (`c:numRef`, `c:numLit`) rather than text.
    pub numeric: bool,
}

/// One `c:trendline` of a series (dc-zjsg.6), as the file states it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ChartTrendline {
    /// `trendlineType`: `linear`, `exp`, `log`, `poly`, `power` or `movingAvg`.
    pub kind: String,
    /// `order` of a polynomial (2 to 6), `period` of a moving average.
    pub order: Option<u32>,
    pub period: Option<u32>,
    /// `forward` and `backward`: how far past the data it is drawn, in x units; `intercept`: where it is held to
    /// cross the value axis.
    pub forward: Option<String>,
    pub backward: Option<String>,
    pub intercept: Option<String>,
    /// Its line's `a:solidFill` colour (`RRGGBB` or a scheme colour's name).
    pub colour: Option<String>,
}

/// What a `c:dLbls` shows (dc-zjsg.6), for drawing: the parts of each label, where it sits and what separates its
/// parts. Label text formatting is not held.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ChartLabels {
    pub value: bool,
    pub category: bool,
    pub series: bool,
    pub percent: bool,
    /// `dLblPos`: `bestFit`, `ctr`, `inEnd`, `inBase`, `outEnd`, `t`, `b`, `l`, `r`.
    pub position: Option<String>,
    /// `separator`.
    pub separator: Option<String>,
    /// `delete`: no labels at all.
    pub deleted: bool,
    /// `numFmt`: the format code and whether it is `sourceLinked` (dc-oxh8w.29).
    pub number_format: Option<(String, bool)>,
    /// The `c:dLbl` overrides of single points, by `c:idx` (dc-zjsg.6): each with its own parts, position and separator
    /// (its text and layout are not held).
    pub points: Vec<(u32, ChartLabels)>,
}

/// One `c:errBars` of a series (dc-zjsg.6), as the file states it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ChartErrorBars {
    /// `errDir`: `x` or `y` (`None`: the value direction).
    pub direction: Option<String>,
    /// `errBarType`: `both`, `plus` or `minus`.
    pub kind: Option<String>,
    /// `errValType`: `fixedVal`, `percentage`, `stdDev`, `stdErr` or `cust`.
    pub value_type: Option<String>,
    /// `val`: the amount, percent or number of standard deviations.
    pub value: Option<String>,
    /// Drawn with end caps (`noEndCap` absent or 0).
    pub end_cap: bool,
}

impl Default for ChartErrorBars {
    fn default() -> Self {
        ChartErrorBars { direction: None, kind: None, value_type: None, value: None, end_cap: true }
    }
}

/// One `c:ser`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ChartSeries {
    /// `c:idx`: the series' place in the chart's colours.
    pub index: u32,
    /// A simple series-wide value-label setting; complex labels remain unmodelled.
    pub show_values: Option<bool>,
    /// The labels explicitly use the source cells' number formats.
    pub value_labels_source_linked: bool,
    pub name: Option<ChartData>,
    /// `c:cat`, or a scatter series' `c:xVal`.
    pub categories: Option<ChartData>,
    /// `c:val`, or a scatter series' `c:yVal`.
    pub values: Option<ChartData>,
    /// An Excel 2016 hierarchy chart's category levels above the leaves (`cx:strDim`'s later `cx:lvl`), nearest the
    /// leaves first; a parent's name is given at its first point and holds until the next (dc-zjsg.6).
    pub outer_levels: Vec<Vec<Option<String>>>,
    /// A bubble series' `c:bubbleSize` (dc-zjsg.6).
    pub bubble_sizes: Option<ChartData>,
    /// The series' `c:trendline`s (dc-zjsg.6).
    pub trendlines: Vec<ChartTrendline>,
    /// The series' `c:errBars` (dc-zjsg.6).
    pub error_bars: Vec<ChartErrorBars>,
    /// What the series' `c:dLbls` shows, for drawing (dc-zjsg.6).
    pub labels: Option<ChartLabels>,
    /// The series' fill (`c:spPr/a:solidFill`): `RRGGBB` (`a:srgbClr`) or a scheme colour's name
    /// (`a:schemeClr`, e.g. `accent2`).
    pub fill: Option<String>,
    /// The counter of the series' `c16:uniqueId` in a chart this session made (dc-piw0.10 Q17): it
    /// only goes up, a deleted series' number is not reused, `SetSourceData` keeps the numbers of the
    /// series it reuses. `None` writes the series' `idx + 2`.
    pub unique_id: Option<u32>,
}

/// One plot of a chart's plot area (`c:barChart`, `c:lineChart`, ...).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ChartPlot {
    /// The element's name: `barChart`, `bar3DChart`, `lineChart`, `areaChart`, `pieChart`,
    /// `doughnutChart`, `scatterChart`, `radarChart`, `bubbleChart`, ...
    pub kind: String,
    /// A bar plot's `c:barDir`: `col` or `bar`.
    pub bar_direction: Option<String>,
    /// `c:grouping`: `clustered`, `stacked`, `percentStacked` or `standard`.
    pub grouping: Option<String>,
    /// `c:varyColors`: each point its own colour (a pie's slices).
    pub vary_colors: bool,
    /// A simple plot-wide value-label setting; complex labels remain unmodelled.
    pub show_values: Option<bool>,
    /// The labels explicitly use the source cells' number formats.
    pub value_labels_source_linked: bool,
    pub series: Vec<ChartSeries>,
    /// The axes this plot actually references, in stored order.
    pub axis_ids: Vec<u32>,
    /// A radar plot's `c:radarStyle` (`standard`, `marker`, `filled`) (dc-zjsg.6).
    pub style: Option<String>,
    /// A bubble plot's `c:bubbleScale` (percent, 100 when absent) and `c:sizeRepresents` (`area` or `w`).
    pub bubble_scale: Option<u32>,
    pub bubble_width: bool,
    /// A stock or line plot's `c:hiLowLines` and a stock plot's `c:upDownBars` (dc-zjsg.6).
    pub high_low_lines: bool,
    /// A line or area plot's `c:dropLines` (dc-zjsg.6).
    pub drop_lines: bool,
    /// What the plot's `c:dLbls` shows, for drawing (dc-zjsg.6).
    pub labels: Option<ChartLabels>,
    /// An Excel 2016 waterfall's subtotal points (`cx:subtotals`), by index (dc-zjsg.6).
    pub subtotals: Vec<u32>,
    /// An Excel 2016 box and whisker's `cx:statistics quartileMethod="inclusive"` and `cx:visibility` (`meanMarker`,
    /// `meanLine`, `outliers`, `nonoutliers`) as the file states them (dc-zjsg.6).
    pub box_inclusive: bool,
    pub box_visibility: Option<[bool; 4]>,
    /// An Excel 2016 histogram's `cx:binning` (dc-zjsg.6); `None` for the automatic one.
    pub binning: Option<Binning>,
    /// A Pareto: the histogram's series is followed by a `paretoLine` series (the cumulative percentage).
    pub pareto: bool,
    /// An Excel 2016 chart's `cx:catScaling gapWidth` (as a share of a bar), as written; `None` when absent.
    pub gap_width: Option<String>,
    /// A surface plot's `c:wireframe` (dc-zjsg.6).
    pub wireframe: bool,
    pub up_down_bars: bool,
}

/// A histogram's `cx:binning` as the file states it: the numbers as written (the model is hashed, so no floats).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Binning {
    /// `cx:binSize val`: the width of a bin.
    pub size: Option<String>,
    /// `cx:binCount val`: the number of bins.
    pub count: Option<String>,
    /// `intervalClosed="l"`: bins closed on the left (the default is closed on the right).
    pub closed_left: bool,
    /// `cx:underflow val` / `cx:overflow val`: the value at or below (above) which values fall in one bin.
    pub underflow: Option<String>,
    pub overflow: Option<String>,
}

/// The outline of a chart area or plot area: what a file states, `None` where it states nothing (dc-oxh8w.61).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ChartBorder {
    /// `a:ln` carries `a:noFill`: no outline is drawn.
    pub none: bool,
    /// Six-digit RGB of the outline's `a:solidFill`.
    pub color: Option<String>,
    /// `a:prstDash val`.
    pub dash: Option<String>,
    /// `a:ln w`, in EMU.
    pub width: Option<u32>,
}

/// One chart on a sheet.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Chart {
    /// The anchor's `editAs` (`oneCell`, `absolute`; `None` for the default `twoCell`): how the frame follows the cells
    /// when rows or columns are resized or hidden.
    pub edit_as: Option<String>,
    /// The chart part.
    pub part: String,
    /// The frame's name (`xdr:cNvPr name`, e.g. `Chart 1`).
    pub name: String,
    pub anchor: ChartAnchor,
    /// The shape identity (`xdr:cNvPr id`) a chart this session made is written with: Excel's
    /// default name `Chart N` is its id less one, and the sheet's charts and shapes share one
    /// counter. `None` for a chart read from a file and for charts the user interface made.
    pub drawing_id: Option<u32>,
    /// The language (`a:rPr lang`) of the title run a chart this session made writes (Excel: the
    /// install/regional language, dc-piw0.10 Q15); `None` writes `en-US`.
    pub lang: Option<String>,
    /// The next `c16:uniqueId` counter a series of a chart this session made takes.
    pub unique_next: u32,
    /// The title's text, as written in the chart (`c:rich`), or its formula's cached text.
    pub title: Option<String>,
    /// The title's formula, when it reads a cell.
    pub title_formula: Option<String>,
    /// `c:autoTitleDeleted`: Excel shows no title when the chart writes none.
    pub auto_title_deleted: bool,
    /// The legend's position (`c:legendPos`: `r`, `l`, `t`, `b`, `tr`), `None` without a legend.
    pub legend: Option<String>,
    pub plots: Vec<ChartPlot>,
    pub axes: Vec<ChartAxis>,
    pub text_style: ChartTextStyle,
    /// The chart area's fill (`c:chartSpace/c:spPr`), six-digit RGB; `None` is the chart's own (dc-oxh8w.31).
    pub area_fill: Option<String>,
    /// The plot area's fill (`c:plotArea/c:spPr`), six-digit RGB; `None` is the chart's own.
    pub plot_fill: Option<String>,
    /// The chart area's outline (`c:chartSpace/c:spPr/a:ln`); all `None` is the chart's own (dc-oxh8w.61).
    pub area_border: ChartBorder,
    /// The plot area's outline (`c:plotArea/c:spPr/a:ln`).
    pub plot_border: ChartBorder,
    /// Multiple rich runs or paragraphs need a text-range editing model.
    pub element_text_complex: std::collections::BTreeSet<String>,
    pub element_text: std::collections::BTreeMap<String, ChartTextStyle>,
}

/// A sheet's charts and how many the reading left out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SheetCharts {
    pub charts: Vec<Chart>,
    pub unread: usize,
}

/// Retained linear-axis settings. Numeric source text is preserved until explicitly edited.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ChartAxis {
    pub id: u32,
    pub cross_axis: Option<u32>,
    pub kind: String,
    pub position: Option<String>,
    /// Whether the file explicitly hides the axis tick labels.
    pub tick_labels_hidden: bool,
    /// Whether the file explicitly suppresses the axis line.
    pub line_hidden: bool,
    /// Explicit RGB for the axis line, when its line is supported.
    pub line_color: Option<String>,
    /// Explicit axis label number format, when it is independent of source cells.
    pub number_format: Option<String>,
    /// Explicit RGB for the major gridlines, when their line is supported.
    pub major_gridline_color: Option<String>,
    /// Literal title text, when available.
    pub title: Option<String>,
    pub minimum: Option<String>,
    pub maximum: Option<String>,
    pub major_unit: Option<String>,
    pub reversed: bool,
    pub log_base: Option<String>,
    pub cross_between: Option<String>,
}

/// Known chart text properties; unrelated DrawingML remains in source.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ChartTextStyle {
    pub family: Option<String>,
    /// Hundredths of a point, as DrawingML stores `sz`.
    pub size: Option<u32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    /// RGB or a stored theme colour name.
    pub color: Option<String>,
}
