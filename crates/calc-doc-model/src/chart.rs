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

/// One `c:ser`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ChartSeries {
    /// `c:idx`: the series' place in the chart's colours.
    pub index: u32,
    pub name: Option<ChartData>,
    /// `c:cat`, or a scatter series' `c:xVal`.
    pub categories: Option<ChartData>,
    /// `c:val`, or a scatter series' `c:yVal`.
    pub values: Option<ChartData>,
    /// The series' fill (`c:spPr/a:solidFill`): `RRGGBB` (`a:srgbClr`) or a scheme colour's name
    /// (`a:schemeClr`, e.g. `accent2`).
    pub fill: Option<String>,
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
    pub series: Vec<ChartSeries>,
}

/// One chart on a sheet.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Chart {
    /// The chart part.
    pub part: String,
    /// The frame's name (`xdr:cNvPr name`, e.g. `Chart 1`).
    pub name: String,
    pub anchor: ChartAnchor,
    /// The title's text, as written in the chart (`c:rich`), or its formula's cached text.
    pub title: Option<String>,
    /// The title's formula, when it reads a cell.
    pub title_formula: Option<String>,
    /// `c:autoTitleDeleted`: Excel shows no title when the chart writes none.
    pub auto_title_deleted: bool,
    /// The legend's position (`c:legendPos`: `r`, `l`, `t`, `b`, `tr`), `None` without a legend.
    pub legend: Option<String>,
    pub plots: Vec<ChartPlot>,
}

/// A sheet's charts and how many the reading left out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SheetCharts {
    pub charts: Vec<Chart>,
    pub unread: usize,
}
