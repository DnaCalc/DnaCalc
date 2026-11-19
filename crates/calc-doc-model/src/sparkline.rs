//! Sparklines (dc-zjsg.7): the Excel 2010 worksheet extension `x14:sparklineGroups`, each group's look and its
//! sparklines (a data range and the cell it is drawn in), as the file states them. The extension stays in the
//! worksheet part as stored; this is what a reader may ask of it.

use crate::StyleColor;

/// One `x14:sparkline`: its data (`xm:f`, a reference as the file writes it, `Sheet1!A2:E2`) and its cell
/// (`xm:sqref`, `F2`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Sparkline {
    pub data: String,
    pub cell: String,
}

/// One `x14:sparklineGroup`. Attribute values are kept as the file writes them; `None` is the schema's default.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SparklineGroup {
    /// `type`: `line` (the default), `column` or `stacked` (win/loss).
    pub kind: Option<String>,
    /// `lineWeight` in points (0.75 by default).
    pub line_weight: Option<String>,
    /// `displayEmptyCellsAs`: `gap`, `zero` (the default) or `span`.
    pub empty_cells: Option<String>,
    pub markers: bool,
    pub high: bool,
    pub low: bool,
    pub first: bool,
    pub last: bool,
    pub negative: bool,
    /// `displayXAxis`.
    pub axis: bool,
    /// `displayHidden`: values in hidden rows and columns are drawn.
    pub hidden: bool,
    /// `rightToLeft`.
    pub right_to_left: bool,
    /// `minAxisType`, `maxAxisType`: `individual` (the default), `group` or `custom`.
    pub min_axis: Option<String>,
    pub max_axis: Option<String>,
    /// `manualMin`, `manualMax` for a custom axis.
    pub manual_min: Option<String>,
    pub manual_max: Option<String>,
    /// `dateAxis` with the group's `xm:f`: the dates the points stand at.
    pub date_axis: bool,
    pub dates: Option<String>,
    /// The colours by element (`colorSeries`, `colorNegative`, `colorAxis`, `colorMarkers`, `colorFirst`,
    /// `colorLast`, `colorHigh`, `colorLow`).
    pub colours: Vec<(String, StyleColor)>,
    pub sparklines: Vec<Sparkline>,
}

impl SparklineGroup {
    pub fn colour(&self, element: &str) -> Option<&StyleColor> {
        self.colours.iter().find(|(name, _)| name == element).map(|(_, c)| c)
    }
}
