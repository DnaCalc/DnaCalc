//! Neutral drawing and control records for worksheet views.

use crate::grid::Key;
use crate::{ChartImage, RawValue, SourceRef};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorksheetObjects {
    pub charts: Vec<ChartObject>,
    pub drawings: Vec<DrawingObject>,
    pub sparklines: Vec<SparklineObject>,
    pub controls: Vec<FormControlObject>,
    pub unread_charts: usize,
    pub unread_drawings: usize,
}

/// Worksheet geometry in pixels at 100% zoom.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ObjectFrame {
    Cells {
        from: ObjectCorner,
        to: ObjectCorner,
    },
    Sized {
        from: ObjectCorner,
        width: f64,
        height: f64,
    },
    Absolute {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectCorner {
    pub cell: Key,
    pub dx: f64,
    pub dy: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChartObject {
    pub source: SourceRef,
    pub frame: ObjectFrame,
    pub image: ChartImage,
    pub data: ChartData,
}

/// The same resolved chart series used by the host renderer, in plot order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartData {
    pub plots: Vec<ChartPlotData>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartPlotData {
    pub kind: ChartPlotKind,
    pub series: Vec<ChartSeriesData>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChartPlotKind {
    Column,
    Bar,
    Line,
    Area,
    Pie,
    Doughnut,
    Scatter,
    Radar,
    Bubble,
    Stock,
    Waterfall,
    Funnel,
    BoxWhisker,
    Histogram,
    Surface,
    Treemap,
    Sunburst,
    Other(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartSeriesData {
    pub name: String,
    pub categories: Vec<Option<RawValue>>,
    pub outer_levels: Vec<Vec<Option<String>>>,
    /// Binary64 values and bubble sizes, with gaps preserved.
    pub values: Vec<Option<u64>>,
    pub sizes: Vec<Option<u64>>,
    pub uses_saved_values: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrawingKind {
    Shape,
    TextBox,
    Picture,
    Connector,
    Group,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrawingObject {
    pub id: u32,
    pub name: String,
    pub kind: DrawingKind,
    pub frame: Option<ObjectFrame>,
    pub hidden: bool,
    pub geometry: Option<String>,
    pub text: Option<Vec<String>>,
    pub picture: Option<PictureObject>,
    pub children: Vec<DrawingObject>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PictureObject {
    pub description: Option<String>,
    pub crop: [i64; 4],
    pub content_type: Option<String>,
    pub stored_bytes: Option<u64>,
    /// Embedded raster image when the retained media is displayable in a browser.
    pub data_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SparklineObject {
    pub at: Key,
    pub kind: String,
    pub marker_points: bool,
    pub color: Option<ObjectColor>,
    pub values: Vec<Option<u64>>,
    pub dates: Vec<Option<u64>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectColor {
    Rgb(String),
    Indexed(u32),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormControlObject {
    pub id: u32,
    pub name: String,
    pub kind: String,
    pub frame: Option<ObjectFrame>,
    pub caption: Option<String>,
    pub linked_cell: Option<Key>,
    pub linked_value: Option<RawValue>,
    pub items: Vec<String>,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub increment: Option<i64>,
    pub has_macro: bool,
}
