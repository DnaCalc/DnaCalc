//! Worksheet drawing commands for auto shapes, text boxes, lines, connectors and pictures.
//!
//! A command applies one drawing change in a workbook transaction. Positions are given in points
//! from the worksheet's top-left corner and persisted as DrawingML anchors. Supported edits also
//! apply to shapes loaded from a workbook. The host preserves opaque objects such as SmartArt
//! and refuses operations that would rewrite their contents.

use crate::{CellEdit, HostState};
use calc_doc_model::{
    ChartAnchor, DrawingPoint, Shape, ShapeConnection, ShapeFill, ShapeKind, ShapeLine,
};
use calc_value::SheetId;

const EMU_PER_POINT: f64 = 12_700.0;
/// The pixel density shape positions are measured at (dc-piw0 Q9): points are DPI free, a
/// column's pixels scale with it.
const DPI: f64 = crate::facts::REFERENCE_DPI;

/// A length in points as its binary64 bits, so commands compare exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Points(u64);
impl Points {
    /// Store a point-valued coordinate, size, or displacement with its exact bits.
    ///
    /// ```
    /// use calc_host::Points;
    ///
    /// let half_inch = Points::new(36.0);
    /// assert_eq!(half_inch.get(), 36.0);
    /// ```
    pub fn new(points: f64) -> Points {
        Points(points.to_bits())
    }
    /// Recover the point value with the original binary64 bits.
    pub fn get(self) -> f64 {
        f64::from_bits(self.0)
    }
}

/// One change to the drawing objects of a sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShapeChange {
    /// A new auto shape (`geometry` is the preset name, e.g. `rect`), text box (`rect`) or
    /// line or connector (`line`, `straightConnector1`), its rectangle in points.
    Add {
        /// The drawing object kind.
        kind: ShapeKind,
        /// The DrawingML preset geometry name.
        geometry: String,
        /// The horizontal offset from the worksheet’s left edge, in points.
        left: Points,
        /// The vertical offset from the worksheet’s top edge, in points.
        top: Points,
        /// The rectangle width in points.
        width: Points,
        /// The rectangle height in points.
        height: Points,
        /// Horizontal and vertical flip (a line drawn right to left or bottom to top).
        flips: (bool, bool),
        /// `None`: Excel's default name for the kind.
        name: Option<String>,
    },
    /// A new picture of the image `bytes` (`content_type` `image/png`, `image/jpeg`, ...), its rectangle in points
    /// (dc-zjsg.4).
    AddPicture {
        /// The horizontal offset from the worksheet’s left edge, in points.
        left: Points,
        /// The vertical offset from the worksheet’s top edge, in points.
        top: Points,
        /// The rectangle width in points.
        width: Points,
        /// The rectangle height in points.
        height: Points,
        /// The image’s MIME type.
        content_type: String,
        /// The encoded image data.
        bytes: std::sync::Arc<[u8]>,
        /// `cNvPr descr`: the picture's alternative text (Excel writes the file's name).
        description: Option<String>,
    },
    /// Remove a drawing object.
    Delete {
        /// The drawing object ID on this sheet.
        id: u32,
    },
    /// Set the drawing object’s display name.
    Name {
        /// The drawing object ID on this sheet.
        id: u32,
        /// The new display name.
        name: String,
    },
    /// Set any of the rectangle’s coordinates or dimensions in points. Each `None` keeps its value.
    Place {
        /// The drawing object ID on this sheet.
        id: u32,
        /// The horizontal offset from the worksheet’s left edge, in points.
        left: Option<Points>,
        /// The vertical offset from the worksheet’s top edge, in points.
        top: Option<Points>,
        /// The rectangle width in points.
        width: Option<Points>,
        /// The rectangle height in points.
        height: Option<Points>,
    },
    /// The text, paragraphs split at `\r`, `\n` or `\r\n`.
    Text {
        /// The drawing object ID on this sheet.
        id: u32,
        /// The replacement text.
        text: String,
    },
    /// Replace the shape’s interior fill.
    Fill {
        /// The drawing object ID on this sheet.
        id: u32,
        /// The replacement interior fill.
        fill: ShapeFill,
    },
    /// Replace the shape’s line or outline formatting.
    Line {
        /// The drawing object ID on this sheet.
        id: u32,
        /// The replacement line formatting.
        line: ShapeLine,
    },
    /// Turn a shape to `rotation` (60,000ths of a degree clockwise, 0 to 21,600,000) about its centre.
    Rotate {
        /// The drawing object ID on this sheet.
        id: u32,
        /// The new clockwise rotation in units of 1/60,000 of a degree.
        rotation: i64,
    },
    /// A copy of a shape (Excel's Duplicate, Ctrl+D) over it, offset by `dx`, `dy` points, in front of everything.
    Duplicate {
        /// The drawing object ID on this sheet.
        id: u32,
        /// The horizontal displacement in points, positive to the right.
        dx: Points,
        /// The vertical displacement in points, positive downward.
        dy: Points,
    },
    /// Move a shape by `dx`, `dy` points (an arrow key's step), kept on the worksheet.
    Nudge {
        /// The drawing object ID on this sheet.
        id: u32,
        /// The horizontal displacement in points, positive to the right.
        dx: Points,
        /// The vertical displacement in points, positive downward.
        dy: Points,
    },
    /// Flip a shape left to right (`horizontal`) or top to bottom, as Excel's Flip Horizontal and Flip Vertical do.
    Flip {
        /// The drawing object ID on this sheet.
        id: u32,
        /// Whether to flip left to right. A false value flips top to bottom.
        horizontal: bool,
    },
    /// Move a shape in the drawing order (dc-zjsg.4).
    Order {
        /// The drawing object ID on this sheet.
        id: u32,
        /// The requested movement in the drawing order.
        order: ShapeOrder,
    },
    /// Group two or more top-level shapes into one group, in their drawing order, where the topmost was.
    Group {
        /// The drawing IDs of the top-level shapes to group.
        ids: Vec<u32>,
    },
    /// Take a group apart: its children become top-level shapes where it was.
    Ungroup {
        /// The drawing object ID on this sheet.
        id: u32,
    },
    /// Glue one end of a connector to a connection site (1-based as VBA counts) of a shape.
    Connect {
        /// The drawing object ID on this sheet.
        id: u32,
        /// Whether to attach the start of the connector. A false value attaches its end.
        begin: bool,
        /// The drawing ID of the shape to attach to.
        shape: u32,
        /// The one-based connection site on the target shape.
        site: u32,
    },
}

/// A step in the drawing order: Excel's Bring Forward, Send Backward, Bring to Front, Send to Back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeOrder {
    /// Move one position toward the front.
    Forward,
    /// Move one position toward the back.
    Backward,
    /// Move in front of all other objects.
    Front,
    /// Move behind all other objects.
    Back,
}

/// The file's own top-level objects (identity and kind).
struct Stored(std::collections::BTreeMap<u32, ShapeKind>);

impl Stored {
    fn holds(&self, shape: &Shape) -> bool {
        self.0.get(&shape.id) == Some(&shape.kind)
    }
}

fn contains_kept(shape: &Shape) -> bool {
    shape.children.iter().any(|c| c.kept.is_some() || contains_kept(c))
}

/// Whether Excel stores the anchor of a shape turned by `rotation` as the shape's box turned a quarter (from 45 up to
/// 135 degrees, and from 225 up to 315).
fn quarter(rotation: i64) -> bool {
    (45.0..135.0).contains(&(rotation as f64 / 60_000.0).rem_euclid(180.0))
}

/// A rectangle's width and height swapped about its centre.
fn swapped((l, t, w, h): (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    (l + w / 2.0 - h / 2.0, t + h / 2.0 - w / 2.0, h, w)
}

/// A top-level shape's anchor rectangle in points: a stored one's as its anchor states it, one this session made as
/// Excel would anchor its transform.
fn anchor_rect(host: &HostState, sheet: SheetId, shape: &Shape, stored: bool) -> (f64, f64, f64, f64) {
    if stored {
        host.anchor_rectangle(sheet, &shape.anchor)
    } else {
        let r = host.shape_rectangle(sheet, shape);
        if quarter(shape.rotation) { swapped(r) } else { r }
    }
}

/// A top-level shape put at the anchor rectangle `r` (points): its anchor (a stored one keeps its anchor's kind, as
/// Excel moves a one-cell or absolute object as one) and its transform, the anchor's box unturned.
fn set_place(host: &HostState, sheet: SheetId, s: &mut Shape, stored: bool, r: (f64, f64, f64, f64)) {
    let (l, t, w, h) = r;
    let emu = |v: f64| (v * EMU_PER_POINT).round() as i64;
    s.anchor = match (stored, &s.anchor) {
        (true, ChartAnchor::OneCell { .. }) => ChartAnchor::OneCell { from: host.point_at(sheet, l, t), width: emu(w), height: emu(h) },
        (true, ChartAnchor::Absolute { .. }) => ChartAnchor::Absolute { x: emu(l), y: emu(t), width: emu(w), height: emu(h) },
        _ => ChartAnchor::TwoCell { from: host.point_at(sheet, l, t), to: host.point_at(sheet, l + w, t + h) },
    };
    let (l, t, w, h) = if quarter(s.rotation) { swapped(r) } else { r };
    s.offset = (emu(l), emu(t));
    s.extent = (emu(w), emu(h));
}

fn emu_of(points: f64) -> i64 {
    (points * EMU_PER_POINT).round() as i64
}

fn refusal(s: &str) -> String {
    format!("{s} (dc-zjsg); nothing changed")
}

/// A shape's default name: Excel numbers it from its id (the first object of a drawing has id 2
/// and is `Rectangle 1`); a line is `Straight Connector N`, a connector `Straight Arrow Connector N`
/// (excel-answers.md Q0, Q1).
fn base_name(kind: ShapeKind, geometry: &str) -> &'static str {
    match (kind, geometry) {
        (ShapeKind::TextBox, _) => "TextBox",
        (ShapeKind::Connector, "line") => "Straight Connector",
        (ShapeKind::Connector, _) => "Straight Arrow Connector",
        (_, "ellipse") => "Oval",
        (_, "roundRect") => "Rectangle: Rounded Corners",
        _ => "Rectangle",
    }
}

/// The identities a session has issued (see [`HostState::next_drawing_id`]).
#[derive(Clone, Debug, Default)]
pub(crate) struct IssuedIds {
    pub(crate) drawing: std::collections::BTreeMap<SheetId, u32>,
    pub(crate) table_id: u32,
    pub(crate) table_number: u32,
}

impl HostState {
    /// The drawing objects of `sheet` other than charts, in drawing order.
    pub fn shapes(&self, sheet: SheetId) -> &[Shape] {
        self.model().definitions().shapes(sheet)
    }

    /// The next shape identity of `sheet`: charts and shapes share one counter that starts at 2
    /// (Excel, dc-piw0 Q1). Within a session an identity is never reused, even after the highest is
    /// deleted; in a reopened file the lowest free one is taken (dc-piw0.10 Q14).
    pub(crate) fn next_drawing_id(&self, sheet: SheetId) -> u32 {
        let charts = self.model().definitions().charts(sheet);
        let anonymous = charts.iter().filter(|c| c.drawing_id.is_none()).count() as u32;
        let used: std::collections::BTreeSet<u32> = self
            .shapes(sheet)
            .iter()
            .map(|s| s.id)
            .chain(charts.iter().filter_map(|c| c.drawing_id))
            // A deleted stored object's identity is not given again: the save matches stored objects by it.
            .chain(self.loaded().model().definitions().shapes(sheet).iter().map(|s| s.id))
            .chain(2..2 + anonymous)
            .collect();
        match self.issued.drawing.get(&sheet) {
            Some(floor) => (*floor).max(used.last().copied().unwrap_or(1)) + 1,
            None => (2u32..).find(|id| !used.contains(id)).unwrap_or(2),
        }
    }

    /// The language tag Excel writes on every text run it creates (`a:rPr lang`): the machine's
    /// install/regional language, here the host's locale, `en-US` without one (dc-piw0.10 Q15).
    pub(crate) fn run_language(&self) -> String {
        self.options()
            .locale
            .as_ref()
            .map_or("en-US", |profile| profile.id.stable_name())
            .to_owned()
    }

    /// Records the identities that exist now and the loaded file did not have: the counters of the
    /// session (called after every committed change).
    pub(crate) fn note_issued_ids(&mut self) {
        let mut issued = std::mem::take(&mut self.issued);
        for sheet in self.sheets().iter().map(|s| s.id()) {
            let drawing_ids = |model: &calc_doc_model::ModelRoot| -> Vec<u32> {
                model
                    .definitions()
                    .shapes(sheet)
                    .iter()
                    .map(|s| s.id)
                    .chain(
                        model
                            .definitions()
                            .charts(sheet)
                            .iter()
                            .filter_map(|c| c.drawing_id),
                    )
                    .collect()
            };
            let loaded = drawing_ids(self.loaded().model());
            if let Some(top) = drawing_ids(self.model())
                .into_iter()
                .filter(|id| !loaded.contains(id))
                .max()
            {
                let floor = issued.drawing.entry(sheet).or_insert(0);
                *floor = (*floor).max(top);
            }
        }
        let loaded: Vec<&calc_doc_model::Table> =
            self.loaded().model().definitions().tables().iter().collect();
        for table in self.model().definitions().tables() {
            if loaded.iter().any(|t| t.part == table.part) {
                continue;
            }
            issued.table_id = issued.table_id.max(table.id);
            if let Some(number) = table
                .display_name
                .strip_prefix("Table")
                .and_then(|n| n.parse::<u32>().ok())
            {
                issued.table_number = issued.table_number.max(number);
            }
        }
        self.issued = issued;
    }

    /// The id a new table gets: never a used one, never one this session issued before.
    pub(crate) fn next_table_id(&self, tables: &[calc_doc_model::Table]) -> u32 {
        let used: std::collections::BTreeSet<u32> = tables
            .iter()
            .chain(self.loaded().model().definitions().tables().iter())
            .map(|t| t.id)
            .collect();
        if self.issued.table_id == 0 {
            (1u32..).find(|id| !used.contains(id)).unwrap_or(1)
        } else {
            self.issued.table_id.max(used.last().copied().unwrap_or(0)) + 1
        }
    }

    /// The default name of a new table: `Table` and the lowest free number, or, once this session made a
    /// table, the next number after the highest it made (a deleted table's number is not reused).
    pub(crate) fn next_table_name(&self, tables: &[calc_doc_model::Table]) -> String {
        let taken = |n: u32| {
            let name = format!("Table{n}");
            tables.iter().any(|t| {
                calc_value::workbook_names_equal(&t.name, &name)
                    || calc_value::workbook_names_equal(&t.display_name, &name)
            })
        };
        let start = self.issued.table_number + 1;
        let first = if self.issued.table_number == 0 { 1 } else { start };
        format!("Table{}", (first..).find(|n| !taken(*n)).unwrap_or(first))
    }

    /// The maximum digit width in pixels at 96 DPI of the workbook's default font (Aptos Narrow 11
    /// and Calibri 11: 7 px; an unmeasured font falls back to 7 px), which a column width in
    /// characters counts in. The display reads it to draw columns as wide as Excel.
    pub fn default_digit_px(&self) -> f64 {
        crate::facts::maximum_digit_width_at(self.styles(), DPI).unwrap_or(7.0)
    }

    /// The workbook's default (Normal) font name, if known (used by print pagination to select
    /// Calibri vs Aptos Narrow printer rates when both share a 7 px screen digit width).
    pub fn default_font_name(&self) -> Option<&str> {
        crate::facts::default_font_name(self.styles())
    }

    /// The default row height in points of the workbook's default font (15 pt for Calibri 11 and
    /// Aptos Narrow 11) when the sheet gives none.
    pub fn default_font_row_points(&self) -> f64 {
        crate::facts::default_row_points(self.styles())
    }

    /// The width in points of a column: whole pixels of the workbook's default font's digit
    /// width (Aptos Narrow 11 and Calibri 11: 7 px), converted at the pixel density.
    fn column_points(&self, sheet: SheetId, column: u32) -> f64 {
        let format = self.model().definitions().axis_layouts().get(sheet, calc_doc_model::Axis::Columns, column);
        self.column_points_of(sheet, format)
    }

    /// A column's width in points with the format `format`.
    pub(crate) fn column_points_of(&self, sheet: SheetId, format: calc_doc_model::AxisFormat) -> f64 {
        // A font the measure does not know falls back to Calibri 11's 7 pixels.
        let mdw = crate::facts::maximum_digit_width_at(self.styles(), DPI).unwrap_or(7.0);
        if format.hidden() {
            return 0.0;
        }
        let geometry = self.loaded().sheet(sheet).map(|s| s.geometry());
        let pixels = match format
            .size()
            .or_else(|| geometry.and_then(|g| g.default_column_width))
        {
            Some(width) => crate::facts::column_pixels(width, mdw),
            None => crate::facts::default_column_pixels(
                geometry.and_then(|g| g.base_column_width).unwrap_or(8.0),
                mdw,
            ),
        };
        pixels * 72.0 / DPI
    }

    /// The width in characters Excel shows for a column (8.43 for the default column of a Calibri
    /// workbook; 0 when hidden): what `Range.Text` fits a General number into. The same measure as
    /// the workbook facts' `column_width` (`CELL("width")`).
    pub(crate) fn column_characters(&self, sheet: SheetId, column: u32) -> f64 {
        let format = self.model().definitions().axis_layouts().get(
            sheet,
            calc_doc_model::Axis::Columns,
            column,
        );
        self.column_characters_of(sheet, format)
    }

    pub(crate) fn column_characters_of(
        &self,
        sheet: SheetId,
        format: calc_doc_model::AxisFormat,
    ) -> f64 {
        let mdw = crate::facts::maximum_digit_width_at(self.styles(), 96.0).unwrap_or(7.0);
        if format.hidden() {
            return 0.0;
        }
        let geometry = self.loaded().sheet(sheet).map(|s| s.geometry());
        match format
            .size()
            .or_else(|| geometry.and_then(|g| g.default_column_width))
        {
            Some(width) => crate::facts::shown_characters(width, mdw),
            None => {
                let base = geometry.and_then(|g| g.base_column_width).unwrap_or(8.0);
                let pixels = crate::facts::default_column_pixels(base, mdw);
                (((pixels - 5.0) / mdw * 100.0 + 0.5).trunc() / 100.0).max(0.0)
            }
        }
    }

    fn row_points(&self, sheet: SheetId, row: u32) -> f64 {
        let format = self.model().definitions().axis_layouts().get(sheet, calc_doc_model::Axis::Rows, row);
        self.row_points_of(sheet, format)
    }

    /// A row's height in points with the format `format`.
    pub(crate) fn row_points_of(&self, sheet: SheetId, format: calc_doc_model::AxisFormat) -> f64 {
        if format.hidden() {
            return 0.0;
        }
        format
            .size()
            .or_else(|| {
                self.loaded()
                    .sheet(sheet)
                    .and_then(|s| s.geometry().default_row_height)
            })
            .unwrap_or_else(|| crate::facts::default_row_points(self.styles()))
    }

    /// The sheet's x offset, in points, of the left edge of zero-based `column`.
    pub fn column_left(&self, sheet: SheetId, column: u32) -> f64 {
        (0..column).map(|c| self.column_points(sheet, c + 1)).sum()
    }
    /// The sheet's y offset, in points, of the top edge of zero-based `row`.
    pub fn row_top(&self, sheet: SheetId, row: u32) -> f64 {
        (0..row).map(|r| self.row_points(sheet, r + 1)).sum()
    }

    /// The cell anchor point of the sheet position (`x`, `y`) in points.
    pub(crate) fn point_at(&self, sheet: SheetId, x: f64, y: f64) -> DrawingPoint {
        let (mut column, mut left) = (0u32, 0.0f64);
        while column < 16383 {
            let width = self.column_points(sheet, column + 1);
            if left + width > x {
                break;
            }
            left += width;
            column += 1;
        }
        let (mut row, mut top) = (0u32, 0.0f64);
        while row < 1_048_575 {
            let height = self.row_points(sheet, row + 1);
            if top + height > y {
                break;
            }
            top += height;
            row += 1;
        }
        DrawingPoint {
            column,
            column_offset: ((x - left) * EMU_PER_POINT).round() as i64,
            row,
            row_offset: ((y - top) * EMU_PER_POINT).round() as i64,
        }
    }

    /// The top left of the cell at 1-based `row` and `column`, in points from the worksheet's top left.
    pub fn cell_corner(&self, sheet: SheetId, row: u32, column: u32) -> (f64, f64) {
        let x = (1..column).map(|c| self.column_points(sheet, c)).sum::<f64>();
        let y = (1..row).map(|r| self.row_points(sheet, r)).sum::<f64>();
        (x, y)
    }

    /// Return the anchor rectangle as left, top, width and height in points.
    ///
    /// Cell-based anchors use the sheet’s current column widths and row heights.
    pub fn anchor_rectangle(&self, sheet: SheetId, anchor: &ChartAnchor) -> (f64, f64, f64, f64) {
        let x = |p: &DrawingPoint| (1..=p.column).map(|c| self.column_points(sheet, c)).sum::<f64>() + p.column_offset as f64 / EMU_PER_POINT;
        let y = |p: &DrawingPoint| (1..=p.row).map(|r| self.row_points(sheet, r)).sum::<f64>() + p.row_offset as f64 / EMU_PER_POINT;
        match anchor {
            ChartAnchor::TwoCell { from, to } => (x(from), y(from), x(to) - x(from), y(to) - y(from)),
            ChartAnchor::OneCell { from, width, height } => (x(from), y(from), *width as f64 / EMU_PER_POINT, *height as f64 / EMU_PER_POINT),
            ChartAnchor::Absolute { x, y, width, height } => {
                (*x as f64 / EMU_PER_POINT, *y as f64 / EMU_PER_POINT, *width as f64 / EMU_PER_POINT, *height as f64 / EMU_PER_POINT)
            }
        }
    }

    /// Return the shape rectangle as left, top, width and height in points.
    ///
    /// Cell-based shapes use their stored transform. Absolute shapes use their anchor.
    pub fn shape_rectangle(&self, sheet: SheetId, shape: &Shape) -> (f64, f64, f64, f64) {
        match shape.anchor {
            ChartAnchor::TwoCell { .. } | ChartAnchor::OneCell { .. } => (
                shape.offset.0 as f64 / EMU_PER_POINT,
                shape.offset.1 as f64 / EMU_PER_POINT,
                shape.extent.0 as f64 / EMU_PER_POINT,
                shape.extent.1 as f64 / EMU_PER_POINT,
            ),
            ChartAnchor::Absolute {
                x,
                y,
                width,
                height,
            } => (
                x as f64 / EMU_PER_POINT,
                y as f64 / EMU_PER_POINT,
                width as f64 / EMU_PER_POINT,
                height as f64 / EMU_PER_POINT,
            ),
        }
        .pipe(|r| {
            let _ = sheet;
            r
        })
    }
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}

/// How many connection sites a preset has (Excel, dc-piw0 Q7): a rectangle, rounded rectangle,
/// text box and diamond 4, an oval 8, an isosceles triangle 6, a line or connector 2.
pub(crate) fn connection_site_count(geometry: &str) -> Option<usize> {
    calc_doc_model::preset_geometry::preset(geometry, &[]).map(|g| g.sites.len())
}

/// A shape's box in points, unturned: a stored one's from its anchor, one this session made from its transform.
fn rectangle(host: &HostState, sheet: SheetId, shapes: &[Shape], index: usize, stored: &Stored) -> (f64, f64, f64, f64) {
    let shape = &shapes[index];
    let r = anchor_rect(host, sheet, shape, stored.holds(shape));
    if quarter(shape.rotation) { swapped(r) } else { r }
}

/// Where connection site `site` (counted from 0, as `stCxn idx`) of `shape` is, in points, its box `rect`: from its
/// geometry's definition (a preset's from ECMA-376, a custom geometry's own `cxnLst`), flipped and turned about the
/// box's centre as the shape is. `None` for a site the geometry does not have.
fn site_point(shape: &Shape, rect: (f64, f64, f64, f64), site: u32) -> Option<(f64, f64)> {
    let (fx, fy) = site_unit(shape, rect, site)?;
    let (l, t, w, h) = rect;
    let (cx, cy) = (l + w / 2.0, t + h / 2.0);
    let (x, y) = (l + fx * w - cx, t + fy * h - cy);
    let (sin, cos) = (shape.rotation as f64 / 60_000.0).to_radians().sin_cos();
    Some((cx + x * cos - y * sin, cy + x * sin + y * cos))
}

/// Connection site `site` of `shape` as a share of its (unturned) box, flipped as the shape is.
fn site_unit(shape: &Shape, rect: (f64, f64, f64, f64), site: u32) -> Option<(f64, f64)> {
    let geometry = match (&shape.geometry, &shape.custom_geometry) {
        (None, Some(custom)) => custom.clone(),
        (Some(name), _) => calc_doc_model::preset_geometry::preset(name, &shape.adjust)?,
        (None, None) => calc_doc_model::preset_geometry::preset("rect", &[])?,
    };
    let (w_emu, h_emu) = if shape.extent.0 > 0 && shape.extent.1 > 0 {
        (shape.extent.0 as f64, shape.extent.1 as f64)
    } else {
        (rect.2 * EMU_PER_POINT, rect.3 * EMU_PER_POINT)
    };
    let (mut fx, mut fy) = *calc_doc_model::preset_geometry::connection_sites(&geometry, w_emu, h_emu).get(site as usize)?;
    if shape.flip_horizontal {
        fx = 1.0 - fx;
    }
    if shape.flip_vertical {
        fy = 1.0 - fy;
    }
    Some((fx, fy))
}

/// The direction (an axis unit vector on the page, y down) a connector leaves `shape` by its site `site`: out of the
/// box edge the site is on, turned as the shape is. `None` for a site not on an edge or a shape not turned a quarter
/// multiple (the direction would be a guess).
fn site_exit(shape: &Shape, rect: (f64, f64, f64, f64), site: u32) -> Option<(i32, i32)> {
    if shape.rotation % (90 * 60_000) != 0 {
        return None;
    }
    let (fx, fy) = site_unit(shape, rect, site)?;
    let near = |v: f64, to: f64| (v - to).abs() < 1e-6;
    let local = match (fx, fy) {
        (x, y) if near(x, 0.0) && !near(y, 0.0) && !near(y, 1.0) => (-1.0, 0.0),
        (x, y) if near(x, 1.0) && !near(y, 0.0) && !near(y, 1.0) => (1.0, 0.0),
        (x, y) if near(y, 0.0) && !near(x, 0.0) && !near(x, 1.0) => (0.0, -1.0),
        (x, y) if near(y, 1.0) && !near(x, 0.0) && !near(x, 1.0) => (0.0, 1.0),
        _ => return None,
    };
    let v = turned(local, shape.rotation);
    Some((v.0.round() as i32, v.1.round() as i32))
}

/// The connectors glued to the shape `id` (dc-zjsg.5), each rerouted ([`reroute`]). A turned connector is left as it is.
fn reroute_connectors(host: &HostState, sheet: SheetId, shapes: &mut [Shape], id: u32, originals: &Stored) {
    let glued: Vec<usize> = (0..shapes.len())
        .filter(|&i| [shapes[i].begin, shapes[i].end].iter().any(|e| e.is_some_and(|e| e.shape == id)))
        .collect();
    for index in glued {
        reroute(host, sheet, shapes, index, originals);
    }
}

/// `v` turned clockwise (on the page, y down) by `rotation` 60,000ths of a degree.
fn turned(v: (f64, f64), rotation: i64) -> (f64, f64) {
    let (sin, cos) = (rotation as f64 / 60_000.0).to_radians().sin_cos();
    (v.0 * cos - v.1 * sin, v.0 * sin + v.1 * cos)
}

/// The connector at `index` redrawn between its ends as Excel reroutes one when a shape it joins moves or it is
/// glued: a glued end at its site, a free end where it was. The connector keeps its geometry, its turn and its
/// adjust values (an elbow's middle leg stays at its share of the box); its box is the one that, turned as the
/// connector is, runs from the start to the end, and its flips say which corner it starts from. A connector turned a
/// quarter (the usual elbow between a bottom and a top site) is anchored on that box turned, as Excel stores it.
fn reroute(host: &HostState, sheet: SheetId, shapes: &mut [Shape], index: usize, originals: &Stored) {
    let (l, t, w, h) = rectangle(host, sheet, shapes, index, originals);
    let c = &shapes[index];
    let mut rotation = c.rotation;
    let centre = (l + w / 2.0, t + h / 2.0);
    let corner = (if c.flip_horizontal { w / 2.0 } else { -w / 2.0 }, if c.flip_vertical { h / 2.0 } else { -h / 2.0 });
    let at = |v: (f64, f64)| {
        let v = turned(v, rotation);
        (centre.0 + v.0, centre.1 + v.1)
    };
    let mut start = at(corner);
    let mut end = at((-corner.0, -corner.1));
    let end_point = |connection: Option<calc_doc_model::ShapeConnection>| {
        let connection = connection?;
        let target = shapes.iter().position(|s| s.id == connection.shape)?;
        site_point(&shapes[target], rectangle(host, sheet, shapes, target, originals), connection.site)
    };
    if let Some(p) = end_point(c.begin) {
        start = p;
    }
    if let Some(p) = end_point(c.end) {
        end = p;
    }
    // An elbow or curved connector glued at both ends keeps its route while that still leaves the start and reaches the
    // end the way the sites face; otherwise it takes the class and turn that do (`elbow_route`). Curved connectors follow
    // the same site/turn rule as bent connectors (ASSUMED until measured in Excel; dc-zjsg.5).
    let mut new_class = None;
    if let (Some(a), Some(b)) = (c.begin, c.end) {
        let info = |connection: calc_doc_model::ShapeConnection| {
            let target = shapes.iter().position(|s| s.id == connection.shape)?;
            let rect = rectangle(host, sheet, shapes, target, originals);
            Some((site_exit(&shapes[target], rect, connection.site)?, rect))
        };
        if let (Some((d1, ra)), Some((d2, rb)), Some((prefix, class))) = (info(a), info(b), c.geometry.as_deref().and_then(elbow_class)) {
            let adjust = |name: &str| c.adjust.iter().find(|(n, _)| n == name).map_or(0.5, |(_, v)| *v as f64 / 100_000.0);
            let current = [adjust("adj1"), adjust("adj2"), adjust("adj3")];
            let gap = |(a0, a1): (f64, f64), (b0, b1): (f64, f64)| -> Option<f64> {
                if a1 < b0 - 0.01 {
                    Some((a1 + b0) / 2.0)
                } else if b1 < a0 - 0.01 {
                    Some((b1 + a0) / 2.0)
                } else {
                    None
                }
            };
            let gap_mid = (gap((ra.0, ra.0 + ra.2), (rb.0, rb.0 + rb.2)), gap((ra.1, ra.1 + ra.3), (rb.1, rb.1 + rb.3)));
            if let Some((class, rot, changed)) = elbow_route(start, end, d1, d2, class, rotation, current, gap_mid) {
                new_class = changed.map(|a| (prefix, class, a));
                rotation = rot;
            }
        }
    }
    // The run from start to end in the connector's own (unturned) frame.
    let d = turned((end.0 - start.0, end.1 - start.1), -rotation);
    let (w, h) = (d.0.abs(), d.1.abs());
    let centre = ((start.0 + end.0) / 2.0, (start.1 + end.1) / 2.0);
    let unturned = (centre.0 - w / 2.0, centre.1 - h / 2.0, w, h);
    let anchor = if quarter(rotation) { swapped(unturned) } else { unturned };
    let anchor = (anchor.0.max(0.0), anchor.1.max(0.0), anchor.2, anchor.3);
    let stored = originals.holds(&shapes[index]);
    let c = &mut shapes[index];
    c.rotation = rotation;
    set_place(host, sheet, c, stored, anchor);
    if let Some((prefix, class, a)) = new_class {
        c.geometry = Some(format!("{prefix}{class}"));
        c.adjust = (1..class.saturating_sub(1)).map(|i| (format!("adj{i}"), (a[i as usize - 1] * 100_000.0).round() as i64)).collect();
    }
    c.flip_horizontal = d.0 < 0.0;
    c.flip_vertical = d.1 < 0.0;
}

/// The preset family (`"bentConnector"` or `"curvedConnector"`) and class (2 to 5) of a routed connector's preset name.
fn elbow_class(geometry: &str) -> Option<(&'static str, u32)> {
    for prefix in ["bentConnector", "curvedConnector"] {
        if let Some(rest) = geometry.strip_prefix(prefix) {
            return rest.parse().ok().filter(|k| (2..=5).contains(k)).map(|k| (prefix, k));
        }
    }
    None
}

/// The path of an elbow connector of `class` in its own frame (start at the origin, end at (w, h)); `a` are its three
/// adjust values as shares.
fn elbow_path(class: u32, (w, h): (f64, f64), a: [f64; 3]) -> Vec<(f64, f64)> {
    match class {
        2 => vec![(0.0, 0.0), (w, 0.0), (w, h)],
        3 => vec![(0.0, 0.0), (a[0] * w, 0.0), (a[0] * w, h), (w, h)],
        4 => vec![(0.0, 0.0), (a[0] * w, 0.0), (a[0] * w, a[1] * h), (w, a[1] * h), (w, h)],
        _ => vec![(0.0, 0.0), (a[0] * w, 0.0), (a[0] * w, a[1] * h), (a[2] * w, a[1] * h), (a[2] * w, h), (w, h)],
    }
}

/// Whether an elbow of `class`, turned `rotation`, drawn from `start` to `end` (flipped as the run needs), leaves the
/// start in direction `d1`, arrives against `d2` (the direction the end's site faces out) and never doubles back.
fn elbow_fits(start: (f64, f64), end: (f64, f64), d1: (i32, i32), d2: (i32, i32), class: u32, rotation: i64, a: [f64; 3]) -> bool {
    let run = turned((end.0 - start.0, end.1 - start.1), -rotation);
    let (w, h) = (run.0.abs(), run.1.abs());
    let flip = (if run.0 < 0.0 { -1.0 } else { 1.0 }, if run.1 < 0.0 { -1.0 } else { 1.0 });
    let page: Vec<(f64, f64)> = elbow_path(class, (w, h), a)
        .into_iter()
        .map(|(x, y)| {
            let v = turned(((x - w / 2.0) * flip.0, (y - h / 2.0) * flip.1), rotation);
            (v.0 + (start.0 + end.0) / 2.0, v.1 + (start.1 + end.1) / 2.0)
        })
        .collect();
    let mut directions: Vec<(i32, i32)> = Vec::new();
    for pair in page.windows(2) {
        let (dx, dy) = (pair[1].0 - pair[0].0, pair[1].1 - pair[0].1);
        if dx.abs() < 0.01 && dy.abs() < 0.01 {
            continue;
        }
        if dx.abs() > 0.01 && dy.abs() > 0.01 {
            return false;
        }
        directions.push((if dx.abs() > 0.01 { dx.signum() as i32 } else { 0 }, if dy.abs() > 0.01 { dy.signum() as i32 } else { 0 }));
    }
    let (Some(&first), Some(&last)) = (directions.first(), directions.last()) else { return false };
    let doubles_back = directions.windows(2).any(|p| p[0].0 == -p[1].0 && p[0].1 == -p[1].1 && (p[0].0 != 0 || p[0].1 != 0));
    first == d1 && last == (-d2.0, -d2.1) && !doubles_back
}

/// How far past a shape's edge an elbow goes to turn round it, in points (Excel-measured: dw3 Windows item 4).
const ELBOW_STANDOFF: f64 = 18.0;

/// How an elbow connector of `class`, turned `rotation` with adjust shares `current`, is to run from `start` to `end`
/// when its ends leave by `d1` and face `d2`: `(class, turn, None)` if the same class and turn with its adjust
/// values still fit (kept as Excel keeps them), else the first of the classes 2 to 5, and quarter turns starting from
/// the current one, that fits, with the adjust values (shares) that do: the middle leg at the midpoint of the gap
/// between the two shapes' boxes (`gap_mid`, else half way), or, where the route must go round, a leg
/// [`ELBOW_STANDOFF`] outside the box (Excel-measured: dw3 Windows item 4). `None` if nothing fits: the connector is
/// left as it was.
fn elbow_route(
    start: (f64, f64),
    end: (f64, f64),
    d1: (i32, i32),
    d2: (i32, i32),
    class: u32,
    rotation: i64,
    current: [f64; 3],
    gap_mid: (Option<f64>, Option<f64>),
) -> Option<(u32, i64, Option<[f64; 3]>)> {
    let (class, rotation, adjust) = elbow_route_any(start, end, d1, d2, class, rotation, current, gap_mid)?;
    // Excel stores a quarter-turned run against the connector's y as flipH and the turn half a turn on (rot 90 -> 270
    // with flipH, dw2 Windows item 4); an unturned horizontal-start connector keeps rot 0 with flipV (dw3 item 4 c4).
    let used = adjust.unwrap_or(current);
    let run = turned((end.0 - start.0, end.1 - start.1), -rotation);
    let other = (rotation + 180 * 60_000).rem_euclid(360 * 60_000);
    if quarter(rotation) && run.1 < -0.01 && elbow_fits(start, end, d1, d2, class, other, used) {
        return Some((class, other, adjust));
    }
    Some((class, rotation, adjust))
}

fn elbow_route_any(
    start: (f64, f64),
    end: (f64, f64),
    d1: (i32, i32),
    d2: (i32, i32),
    class: u32,
    rotation: i64,
    current: [f64; 3],
    gap_mid: (Option<f64>, Option<f64>),
) -> Option<(u32, i64, Option<[f64; 3]>)> {
    if elbow_fits(start, end, d1, d2, class, rotation, current) {
        return Some((class, rotation, None));
    }
    let quarter_turn = 90 * 60_000;
    let share = |mid: Option<f64>, a: f64, b: f64| -> f64 {
        let Some(m) = mid else { return 0.5 };
        if (b - a).abs() > 0.5 {
            let s = (m - a) / (b - a);
            if s > 0.01 && s < 0.99 {
                return s;
            }
        }
        0.5
    };
    for k in 2..=5u32 {
        for step in 0..4 {
            let r = (rotation + step * quarter_turn).rem_euclid(4 * quarter_turn);
            let run = turned((end.0 - start.0, end.1 - start.1), -r);
            let (w, h) = (run.0.abs(), run.1.abs());
            let (mid_x, mid_y) = if quarter(r) {
                (share(gap_mid.1, start.1, end.1), share(gap_mid.0, start.0, end.0))
            } else {
                (share(gap_mid.0, start.0, end.0), share(gap_mid.1, start.1, end.1))
            };
            // Each adjust value: mid-gap (or half way), below the start's side of the box, or past the far side. Fewest detours first.
            let options = |mid: f64, size: f64| if size > 0.5 { vec![mid, -ELBOW_STANDOFF / size, 1.0 + ELBOW_STANDOFF / size] } else { vec![mid] };
            let (ox, oy) = (options(mid_x, w), options(mid_y, h));
            let mut combos: Vec<([f64; 3], usize)> = Vec::new();
            for (ix1, &x1) in ox.iter().enumerate() {
                for (iy2, &y2) in oy.iter().enumerate() {
                    for (ix3, &x3) in ox.iter().enumerate() {
                        // Unused adjust slots stay at their default (index 0) so duplicates are not tried.
                        let unused_ok = match k {
                            2 => ix1 == 0 && iy2 == 0 && ix3 == 0,
                            3 => iy2 == 0 && ix3 == 0,
                            4 => ix3 == 0,
                            _ => true,
                        };
                        if unused_ok {
                            let detours = usize::from(ix1 != 0) + usize::from(iy2 != 0) + usize::from(ix3 != 0);
                            combos.push(([x1, y2, x3], detours));
                        }
                    }
                }
            }
            combos.sort_by_key(|(_, detours)| *detours);
            for (a, _) in combos {
                if elbow_fits(start, end, d1, d2, k, r, a) {
                    return Some((k, r, Some(a)));
                }
            }
        }
    }
    None
}

pub(crate) fn prepare(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    change: ShapeChange,
) -> Result<Vec<CellEdit>, String> {
    if revision != host.revision().get() {
        return Err(refusal("The workbook changed; read the shapes again"));
    }
    let source = host
        .loaded()
        .sheet(sheet)
        .ok_or_else(|| refusal("Save and reopen the new worksheet before adding a shape"))?;
    if source.protected() || source.is_macro_sheet() {
        return Err(refusal("Drawing objects need an editable worksheet"));
    }
    if host.loaded().workbook_part().ends_with(".bin") {
        return Err(refusal("Save as XLSX and reopen before adding shapes"));
    }
    let frame = Frame::new(host, sheet);
    let mut shapes = frame.current(host, host.shapes(sheet));
    // The file's own top-level objects, by identity (a deleted one's identity is never issued again, so one is
    // stored while it is at the top level with its kind): a stored one's rectangle is its anchor's.
    let originals = Stored(host.loaded().model().definitions().shapes(sheet).iter().map(|s| (s.id, s.kind)).collect());
    let originals = &originals;
    let find = |shapes: &[Shape], id: u32| -> Result<usize, String> {
        let index = shapes
            .iter()
            .position(|s| s.id == id)
            .ok_or_else(|| refusal("The shape no longer exists"))?;
        if originals.holds(&shapes[index]) {
            return Err(refusal(
                "A shape the file holds cannot be changed yet (dc-zjsg.4)",
            ));
        }
        Ok(index)
    };
    // A shape the file holds may be moved, resized, turned, given text, fill or outline, put in another place of the
    // drawing order, grouped or deleted (dc-zjsg.4): the save rewrites its stored element in place, or drops it. One
    // Excel keeps for an object DNA Calc does not edit (an equation, ink, SmartArt, an OLE or ActiveX object) is not,
    // nor a hidden one. Deleting one a connector is glued to leaves the connector where it is, unglued (Excel).
    let find_movable = |shapes: &[Shape], id: u32, deleting: bool| -> Result<usize, String> {
        let index = shapes
            .iter()
            .position(|s| s.id == id && !s.hidden)
            .ok_or_else(|| refusal("The shape no longer exists"))?;
        let shape = &shapes[index];
        if shape.kept.is_some() || contains_kept(shape) {
            return Err(refusal("An equation, ink, SmartArt, OLE or ActiveX object is kept as the file stores it"));
        }
        if !originals.holds(shape) {
            return Ok(index);
        }
        let _ = deleting;
        Ok(index)
    };
    let next_id = || -> u32 { host.next_drawing_id(sheet) };
    let finite = |v: f64| v.is_finite() && (-1.0e7..=1.0e7).contains(&v);
    match change {
        ShapeChange::Add {
            kind,
            geometry,
            left,
            top,
            width,
            height,
            flips,
            name,
        } => {
            let (left, top, width, height) = (left.get(), top.get(), width.get(), height.get());
            if ![left, top, width, height].into_iter().all(finite) || left < 0.0 || top < 0.0 {
                return Err(refusal("Use a position and size on the worksheet"));
            }
            if width < 0.0 || height < 0.0 {
                return Err(refusal("Use a non-negative size"));
            }
            if !matches!(
                kind,
                ShapeKind::Shape | ShapeKind::TextBox | ShapeKind::Connector
            ) {
                return Err(refusal("Add an auto shape, text box or connector"));
            }
            let id = next_id();
            let name = match name {
                Some(name) => name,
                None => format!("{} {}", base_name(kind, &geometry), id - 1),
            };
            let from = host.point_at(sheet, left, top);
            let to = host.point_at(sheet, left + width, top + height);
            let mut shape = Shape::new(id, name, kind, ChartAnchor::TwoCell { from, to });
            shape.geometry = Some(geometry);
            shape.lang = Some(host.run_language());
            shape.offset = (
                (left * EMU_PER_POINT).round() as i64,
                (top * EMU_PER_POINT).round() as i64,
            );
            shape.extent = (
                (width * EMU_PER_POINT).round() as i64,
                (height * EMU_PER_POINT).round() as i64,
            );
            (shape.flip_horizontal, shape.flip_vertical) = flips;
            shapes.push(shape);
        }
        ShapeChange::AddPicture { left, top, width, height, content_type, bytes, description } => {
            let (left, top, width, height) = (left.get(), top.get(), width.get(), height.get());
            if ![left, top, width, height].into_iter().all(finite) || left < 0.0 || top < 0.0 || width <= 0.0 || height <= 0.0 {
                return Err(refusal("Use a position and size on the worksheet"));
            }
            if !matches!(content_type.as_str(), "image/png" | "image/jpeg" | "image/gif" | "image/bmp" | "image/tiff" | "image/x-emf" | "image/x-wmf") || bytes.is_empty() {
                return Err(refusal("Insert a PNG, JPEG, GIF, BMP, TIFF, EMF or WMF picture"));
            }
            let id = next_id();
            let from = host.point_at(sheet, left, top);
            let to = host.point_at(sheet, left + width, top + height);
            let mut shape = Shape::new(id, format!("Picture {}", id - 1), ShapeKind::Picture, ChartAnchor::TwoCell { from, to });
            // Excel anchors an inserted picture to move with its cells without sizing with them.
            shape.edit_as = Some("oneCell".into());
            shape.geometry = Some("rect".into());
            shape.offset = (emu_of(left), emu_of(top));
            shape.extent = (emu_of(width), emu_of(height));
            shape.media = Some(calc_doc_model::ShapePicture {
                image: Some(calc_doc_model::PictureMedia { part: String::new(), content_type, size: bytes.len() as u64, bytes }),
                svg: None,
                crop: [0; 4],
                description,
            });
            shapes.push(shape);
        }
        ShapeChange::Delete { id } => {
            let index = find_movable(&shapes, id, true)?;
            shapes.remove(index);
            // A connector glued to the deleted shape stays where it is, unglued (Excel).
            for s in &mut shapes {
                for end in [&mut s.begin, &mut s.end] {
                    if end.is_some_and(|e| e.shape == id) {
                        *end = None;
                    }
                }
            }
        }
        ShapeChange::Name { id, name } => {
            let index = find_movable(&shapes, id, false)?;
            if name.is_empty() || name.chars().count() > 255 {
                return Err(refusal("Use a shape name of 1 to 255 characters"));
            }
            shapes[index].name = name;
        }
        ShapeChange::Place {
            id,
            left,
            top,
            width,
            height,
        } => {
            let index = find_movable(&shapes, id, false)?;
            let stored = originals.holds(&shapes[index]);
            let (l, t, w, h) = anchor_rect(host, sheet, &shapes[index], stored);
            let (l, t, w, h) = (
                left.map_or(l, Points::get),
                top.map_or(t, Points::get),
                width.map_or(w, Points::get),
                height.map_or(h, Points::get),
            );
            if ![l, t, w, h].into_iter().all(finite) || l < 0.0 || t < 0.0 || w < 0.0 || h < 0.0 {
                return Err(refusal("Use a position and size on the worksheet"));
            }
            set_place(host, sheet, &mut shapes[index], stored, (l, t, w, h));
            reroute_connectors(host, sheet, &mut shapes, id, originals);
        }
        ShapeChange::Text { id, text } => {
            let index = find_movable(&shapes, id, false)?;
            if !matches!(shapes[index].kind, ShapeKind::Shape | ShapeKind::TextBox) {
                return Err(refusal("Only a shape or text box holds text"));
            }
            // Excel (Q8): a carriage return or CR LF starts a paragraph; a bare line feed stays in
            // its paragraph.
            let normalized = text.replace("\r\n", "\r");
            shapes[index].text = Some(normalized.split('\r').map(str::to_owned).collect());
            if text.is_empty() {
                shapes[index].text = None;
            }
        }
        ShapeChange::Fill { id, fill } => {
            let index = find_movable(&shapes, id, false)?;
            shapes[index].fill = fill;
        }
        ShapeChange::Line { id, line } => {
            let index = find_movable(&shapes, id, false)?;
            shapes[index].line = line;
        }
        ShapeChange::Rotate { id, rotation } => {
            let index = find_movable(&shapes, id, false)?;
            if !(0..21_600_000).contains(&rotation) {
                return Err(refusal("Use a turn from 0 up to 360 degrees"));
            }
            let stored = originals.holds(&shapes[index]);
            let unturned = rectangle(host, sheet, &shapes, index, originals);
            let s = &mut shapes[index];
            s.rotation = rotation;
            let r = if quarter(rotation) { swapped(unturned) } else { unturned };
            if r.0 < 0.0 || r.1 < 0.0 {
                return Err(refusal("The turned shape would leave the worksheet"));
            }
            set_place(host, sheet, s, stored, r);
            reroute_connectors(host, sheet, &mut shapes, id, originals);
        }
        ShapeChange::Duplicate { id, dx, dy } => {
            let index = find_movable(&shapes, id, false)?;
            let source = shapes[index].clone();
            let stored = originals.holds(&source);
            if stored && source.kind == ShapeKind::Group {
                return Err(refusal("Duplicating a group the file holds is not modelled (dc-zjsg.4)"));
            }
            let (dx, dy) = (dx.get(), dy.get());
            if !finite(dx) || !finite(dy) {
                return Err(refusal("Use a finite offset"));
            }
            let (l, t, w, h) = anchor_rect(host, sheet, &source, stored);
            // Identities for the copy and, in a group, each of its children.
            let mut next = next_id();
            fn renumber(shape: &mut Shape, next: &mut u32) {
                shape.id = *next;
                *next += 1;
                for child in &mut shape.children {
                    renumber(child, next);
                }
            }
            let mut copy = source.clone();
            renumber(&mut copy, &mut next);
            copy.copy_of = if stored { Some(source.id) } else { source.copy_of };
            // A default name ("Rectangle 3") is renumbered from the copy's identity; one the user gave stays.
            if let Some((base, number)) = source.name.rsplit_once(' ')
                && number.parse::<u32>().ok() == Some(source.id.saturating_sub(1))
            {
                copy.name = format!("{base} {}", copy.id - 1);
            }
            copy.begin = None;
            copy.end = None;
            set_place(host, sheet, &mut copy, false, ((l + dx).max(0.0), (t + dy).max(0.0), w, h));
            if !stored {
                // The copy's anchor kind is a new object's (two-cell); its transform follows its anchor.
                copy.edit_as = source.edit_as.clone();
            }
            shapes.push(copy);
        }
        ShapeChange::Nudge { id, dx, dy } => {
            let index = find_movable(&shapes, id, false)?;
            let stored = originals.holds(&shapes[index]);
            let (l, t, w, h) = anchor_rect(host, sheet, &shapes[index], stored);
            let (dx, dy) = (dx.get(), dy.get());
            if !finite(dx) || !finite(dy) {
                return Err(refusal("Use a finite step"));
            }
            let (l, t) = ((l + dx).max(0.0), (t + dy).max(0.0));
            set_place(host, sheet, &mut shapes[index], stored, (l, t, w, h));
            reroute_connectors(host, sheet, &mut shapes, id, originals);
        }
        ShapeChange::Flip { id, horizontal } => {
            let index = find_movable(&shapes, id, false)?;
            if !matches!(shapes[index].kind, ShapeKind::Shape | ShapeKind::TextBox | ShapeKind::Picture | ShapeKind::Group) {
                return Err(refusal("Flip a shape, text box, picture or group; a connector follows its ends"));
            }
            let s = &mut shapes[index];
            // A flip mirrors the turned shape on the page: its own flip toggles and its turn reverses.
            if horizontal {
                s.flip_horizontal = !s.flip_horizontal;
            } else {
                s.flip_vertical = !s.flip_vertical;
            }
            s.rotation = (21_600_000 - s.rotation).rem_euclid(21_600_000);
            reroute_connectors(host, sheet, &mut shapes, id, originals);
        }
        ShapeChange::Order { id, order } => {
            let index = shapes.iter().position(|s| s.id == id).ok_or_else(|| refusal("The shape no longer exists"))?;
            let last = shapes.len() - 1;
            let to = match order {
                ShapeOrder::Forward => (index + 1).min(last),
                ShapeOrder::Backward => index.saturating_sub(1),
                ShapeOrder::Front => last,
                ShapeOrder::Back => 0,
            };
            if to == index {
                return Err(refusal(if matches!(order, ShapeOrder::Forward | ShapeOrder::Front) {
                    "The shape is already in front"
                } else {
                    "The shape is already at the back"
                }));
            }
            let shape = shapes.remove(index);
            shapes.insert(to, shape);
        }
        ShapeChange::Group { ids } => {
            let mut members = Vec::new();
            for &id in &ids {
                let index = find_movable(&shapes, id, false)?;
                if members.contains(&index) {
                    return Err(refusal("Name each shape once"));
                }
                members.push(index);
            }
            if members.len() < 2 {
                return Err(refusal("Group two or more shapes"));
            }
            members.sort_unstable();
            let emu = |v: f64| (v * EMU_PER_POINT).round() as i64;
            // The group's box holds its members' anchors; each member is given in the group's child space, which is
            // the worksheet's own (Excel's chOff and chExt equal the group's off and ext).
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            let mut children = Vec::new();
            for &index in &members {
                let stored = originals.holds(&shapes[index]);
                let a = anchor_rect(host, sheet, &shapes[index], stored);
                (x0, y0, x1, y1) = (x0.min(a.0), y0.min(a.1), x1.max(a.0 + a.2), y1.max(a.1 + a.3));
                let (l, t, w, h) = rectangle(host, sheet, &shapes, index, originals);
                let mut child = shapes[index].clone();
                child.offset = (emu(l), emu(t));
                child.extent = (emu(w), emu(h));
                children.push(child);
            }
            let id = next_id();
            let anchor = ChartAnchor::TwoCell { from: host.point_at(sheet, x0, y0), to: host.point_at(sheet, x1, y1) };
            for child in &mut children {
                child.anchor = anchor;
                child.edit_as = None;
            }
            let mut group = Shape::new(id, format!("Group {}", id - 1), ShapeKind::Group, anchor);
            group.offset = (emu(x0), emu(y0));
            group.extent = (emu(x1 - x0), emu(y1 - y0));
            (group.child_offset, group.child_extent) = (group.offset, group.extent);
            group.children = children;
            let top = *members.last().expect("two or more");
            for &index in members.iter().rev() {
                shapes.remove(index);
            }
            shapes.insert(top + 1 - members.len(), group);
        }
        ShapeChange::Ungroup { id } => {
            let index = find_movable(&shapes, id, false)?;
            let group = shapes[index].clone();
            if group.kind != ShapeKind::Group {
                return Err(refusal("Only a group comes apart"));
            }
            let (gl, gt, gw, gh) = rectangle(host, sheet, &shapes, index, originals);
            // A child keeps where the group showed it: its box mirrored within the group's box by the group's flips
            // (its own flip toggled and its turn reversed), then turned with the group about the group's centre.
            let centre = (gl + gw / 2.0, gt + gh / 2.0);
            let full = 21_600_000i64;
            let scale = |extent: i64, space: i64| if space > 0 { extent as f64 / space as f64 } else { 1.0 };
            let (sx, sy) = (scale(emu_of(gw), group.child_extent.0), scale(emu_of(gh), group.child_extent.1));
            let mut children = group.children.clone();
            for child in &mut children {
                let l = gl + (child.offset.0 - group.child_offset.0) as f64 * sx / EMU_PER_POINT;
                let t = gt + (child.offset.1 - group.child_offset.1) as f64 * sy / EMU_PER_POINT;
                let (w, h) = (child.extent.0 as f64 * sx / EMU_PER_POINT, child.extent.1 as f64 * sy / EMU_PER_POINT);
                let mut c = (l + w / 2.0, t + h / 2.0);
                if group.flip_horizontal {
                    c.0 = 2.0 * centre.0 - c.0;
                    child.flip_horizontal = !child.flip_horizontal;
                    child.rotation = (full - child.rotation).rem_euclid(full);
                }
                if group.flip_vertical {
                    c.1 = 2.0 * centre.1 - c.1;
                    child.flip_vertical = !child.flip_vertical;
                    child.rotation = (full - child.rotation).rem_euclid(full);
                }
                if group.rotation != 0 {
                    let v = turned((c.0 - centre.0, c.1 - centre.1), group.rotation);
                    c = (centre.0 + v.0, centre.1 + v.1);
                    child.rotation = (child.rotation + group.rotation).rem_euclid(full);
                }
                let (l, t) = (c.0 - w / 2.0, c.1 - h / 2.0);
                let r = if quarter(child.rotation) { swapped((l, t, w, h)) } else { (l, t, w, h) };
                child.anchor = ChartAnchor::TwoCell { from: host.point_at(sheet, r.0.max(0.0), r.1.max(0.0)), to: host.point_at(sheet, (r.0 + r.2).max(0.0), (r.1 + r.3).max(0.0)) };
                // A stored object back at the top level keeps its own anchor's editAs.
                child.edit_as = match host.loaded().model().definitions().shapes(sheet).iter().find(|s| s.id == child.id && s.kind == child.kind) {
                    Some(stored) => stored.edit_as.clone(),
                    None => group.edit_as.clone(),
                };
                child.offset = (emu_of(l), emu_of(t));
                child.extent = (emu_of(w), emu_of(h));
            }
            shapes.splice(index..=index, children);
        }
        ShapeChange::Connect {
            id,
            begin,
            shape: target,
            site,
        } => {
            let index = find(&shapes, id)?;
            if shapes[index].kind != ShapeKind::Connector {
                return Err(refusal("Only a connector connects"));
            }
            let glued = shapes
                .iter()
                .position(|s| s.id == target)
                .ok_or_else(|| refusal("The shape to connect to no longer exists"))?;
            let count = match (&shapes[glued].geometry, &shapes[glued].custom_geometry) {
                (None, Some(custom)) => custom.sites.len(),
                (geometry, _) => connection_site_count(geometry.as_deref().unwrap_or("rect"))
                    .ok_or_else(|| refusal("Connection sites of this shape are not modelled (dc-zjsg.5)"))?,
            };
            if site == 0 || site as usize > count {
                return Err(refusal("The connection site does not exist on that shape"));
            }
            let connection = ShapeConnection {
                shape: target,
                site: site - 1,
            };
            if begin {
                shapes[index].begin = Some(connection);
            } else {
                shapes[index].end = Some(connection);
            }
            reroute(host, sheet, &mut shapes, index, originals);
        }
    }
    let shapes = frame.stored(host, host.shapes(sheet), shapes)?;
    host.loaded()
        .check_shape_set(sheet, &shapes)
        .map_err(|e| e.to_string())?;
    Ok(vec![CellEdit::Shapes { sheet, shapes }])
}

/// The session's row and column inserts and deletes on one sheet. The model holds a drawing object's anchor as the
/// file states it and the save moves the stored part through these edits, so an edit of an object works on its anchor
/// moved through them (where it is now: [`Frame::current`]) and writes the result back through them undone
/// ([`Frame::stored`]); an object the edit leaves where it is keeps its anchor exactly.
struct Frame(Vec<calc_doc_model::AxisEdit>);

impl Frame {
    fn new(host: &HostState, sheet: SheetId) -> Frame {
        Frame(host.axis_edits.iter().filter(|e| e.sheet() == sheet).copied().collect())
    }

    /// Index (0-based) `i` after the edit, a deleted one's corner at the band's end.
    fn forward_index(edit: calc_doc_model::AxisEdit, i: u32) -> u32 {
        calc_doc_model::moved_position(edit, i + 1).unwrap_or_else(|| match edit.change() {
            calc_doc_model::AxisChange::Delete => edit.first().min(edit.axis().size()),
            calc_doc_model::AxisChange::Insert => edit.axis().size(),
        }) - 1
    }

    /// The index before the edit of the (0-based) one `i` after it; `None` inside inserted cells or
    /// tail cells shifted in by a delete (neither has a pre-image in the file's coordinates).
    fn backward_index(edit: calc_doc_model::AxisEdit, i: u32) -> Option<u32> {
        let x = i + 1;
        let before = match edit.change() {
            calc_doc_model::AxisChange::Insert if x < edit.first() => x,
            calc_doc_model::AxisChange::Insert if x <= edit.last() => return None,
            calc_doc_model::AxisChange::Insert => x - edit.count(),
            calc_doc_model::AxisChange::Delete if x < edit.first() => x,
            calc_doc_model::AxisChange::Delete => x.checked_add(edit.count()).filter(|&b| b <= edit.axis().size())?,
        };
        Some(before - 1)
    }

    fn map_point(&self, p: &DrawingPoint, back: bool) -> Option<DrawingPoint> {
        let mut p = p.clone();
        let edits: Vec<_> = if back { self.0.iter().rev().copied().collect() } else { self.0.clone() };
        for edit in edits {
            let rows = edit.axis() == calc_doc_model::Axis::Rows;
            let i = if rows { p.row } else { p.column };
            let j = if back { Self::backward_index(edit, i)? } else { Self::forward_index(edit, i) };
            if rows { p.row = j } else { p.column = j }
        }
        Some(p)
    }

    /// `anchor` moved through the edits (`back`: undone), by the rules the save's anchor pass moves a stored one:
    /// corners each, or the top left with the span kept for a one-cell anchor and an `editAs="oneCell"` one, or not
    /// at all for an absolute one. `None` where an object's corner would lie in inserted cells.
    fn map_anchor(&self, anchor: &ChartAnchor, edit_as: Option<&str>, back: bool) -> Option<ChartAnchor> {
        Some(match anchor {
            ChartAnchor::Absolute { .. } => anchor.clone(),
            ChartAnchor::OneCell { from, width, height } => ChartAnchor::OneCell { from: self.map_point(from, back)?, width: *width, height: *height },
            ChartAnchor::TwoCell { from, to } if edit_as == Some("absolute") => ChartAnchor::TwoCell { from: from.clone(), to: to.clone() },
            ChartAnchor::TwoCell { from, to } if edit_as == Some("oneCell") => {
                let start = self.map_point(from, back)?;
                let (rows, columns) = (to.row.saturating_sub(from.row), to.column.saturating_sub(from.column));
                ChartAnchor::TwoCell {
                    to: DrawingPoint { row: start.row + rows, column: start.column + columns, ..to.clone() },
                    from: start,
                }
            }
            ChartAnchor::TwoCell { from, to } => ChartAnchor::TwoCell { from: self.map_point(from, back)?, to: self.map_point(to, back)? },
        })
    }

    /// The objects as they are now: anchors moved, and a shape of this session (not the file's) has its transform
    /// from where its anchor puts it.
    fn current(&self, host: &HostState, shapes: &[Shape]) -> Vec<Shape> {
        let mut shapes = shapes.to_vec();
        if self.0.is_empty() {
            return shapes;
        }
        let sheet = self.0[0].sheet();
        let stored = Stored(host.loaded().model().definitions().shapes(sheet).iter().map(|s| (s.id, s.kind)).collect());
        for s in &mut shapes {
            let Some(anchor) = self.map_anchor(&s.anchor, s.edit_as.as_deref(), false) else { continue };
            s.anchor = anchor;
            if !stored.holds(s) {
                let r = host.anchor_rectangle(sheet, &s.anchor);
                let (l, t, w, h) = if quarter(s.rotation) { swapped(r) } else { r };
                if matches!(s.anchor, ChartAnchor::TwoCell { .. } | ChartAnchor::OneCell { .. }) {
                    (s.offset, s.extent) = ((emu_of(l), emu_of(t)), (emu_of(w), emu_of(h)));
                }
            }
        }
        shapes
    }

    /// The objects `now` (worked on from [`Frame::current`] of `before`) with their anchors as the model holds them:
    /// one at the place it was keeps its anchor as it was, the others have the edits undone. A refusal where an
    /// object's corner lies in cells the session inserted (it has no place in the file's coordinates).
    fn stored(&self, _host: &HostState, before: &[Shape], mut now: Vec<Shape>) -> Result<Vec<Shape>, String> {
        if self.0.is_empty() {
            return Ok(now);
        }
        for s in &mut now {
            let original = before.iter().find(|b| b.id == s.id && b.kind == s.kind);
            if let Some(b) = original
                && self.map_anchor(&b.anchor, b.edit_as.as_deref(), false).as_ref() == Some(&s.anchor)
            {
                s.anchor = b.anchor.clone();
                continue;
            }
            s.anchor = self
                .map_anchor(&s.anchor, s.edit_as.as_deref(), true)
                .ok_or_else(|| refusal("Save and reopen before placing a shape in rows or columns inserted since the file was opened"))?;
        }
        Ok(now)
    }
}


/// The shapes and charts of `sheet` after the row and column format edits `edits` (dc-zjsg.4, dc-zjsg.14), as Excel
/// keeps them: a two-cell anchor whose editAs is `oneCell` (move but don't size) keeps its size from its top-left
/// corner, one whose editAs is `absolute` (don't move or size) keeps its place and size; their anchors' corners are
/// found anew in the new sizes. A plain two-cell anchor moves and sizes with its cells as it is. Empty when nothing
/// changes.
pub(crate) fn kept_sizes(host: &HostState, sheet: SheetId, edits: &[CellEdit]) -> Vec<CellEdit> {
    use calc_doc_model::Axis;
    let changed: std::collections::BTreeMap<(bool, u32), calc_doc_model::AxisFormat> = edits
        .iter()
        .filter_map(|e| match e {
            CellEdit::AxisFormat(edit) if edit.sheet() == sheet => Some(((edit.axis() == Axis::Rows, edit.index()), edit.format())),
            _ => None,
        })
        .collect();
    if changed.is_empty() {
        return Vec::new();
    }
    let layouts = host.model().definitions().axis_layouts();
    let column_new = |c: u32| host.column_points_of(sheet, changed.get(&(false, c)).copied().unwrap_or_else(|| layouts.get(sheet, Axis::Columns, c)));
    let row_new = |r: u32| host.row_points_of(sheet, changed.get(&(true, r)).copied().unwrap_or_else(|| layouts.get(sheet, Axis::Rows, r)));
    let x_new = |p: &DrawingPoint| (1..=p.column).map(column_new).sum::<f64>() + p.column_offset as f64 / EMU_PER_POINT;
    let y_new = |p: &DrawingPoint| (1..=p.row).map(row_new).sum::<f64>() + p.row_offset as f64 / EMU_PER_POINT;
    let point_new = |x: f64, y: f64| -> DrawingPoint {
        const EPS: f64 = 0.5 / EMU_PER_POINT;
        let (mut column, mut left) = (0u32, 0.0f64);
        while column < 16383 && left + column_new(column + 1) <= x + EPS {
            left += column_new(column + 1);
            column += 1;
        }
        let (mut row, mut top) = (0u32, 0.0f64);
        while row < 1_048_575 && top + row_new(row + 1) <= y + EPS {
            top += row_new(row + 1);
            row += 1;
        }
        let emu = |v: f64| (v.max(0.0) * EMU_PER_POINT).round() as i64;
        DrawingPoint { column, column_offset: emu(x - left), row, row_offset: emu(y - top) }
    };
    // An anchor as the new sizes keep it, or `None` when it stays.
    let keep = |anchor: &ChartAnchor, edit_as: Option<&str>| -> Option<ChartAnchor> {
        let ChartAnchor::TwoCell { from, .. } = anchor else { return None };
        let (l, t, w, h) = host.anchor_rectangle(sheet, anchor);
        let kept = match edit_as {
            Some("oneCell") => {
                let (x, y) = (x_new(from), y_new(from));
                ChartAnchor::TwoCell { from: from.clone(), to: point_new(x + w, y + h) }
            }
            Some("absolute") => ChartAnchor::TwoCell { from: point_new(l, t), to: point_new(l + w, t + h) },
            _ => return None,
        };
        (kept != *anchor).then_some(kept)
    };
    let frame = Frame::new(host, sheet);
    let mut shapes = frame.current(host, host.shapes(sheet));
    let mut any = false;
    for s in &mut shapes {
        // A kept object stays as the file stores it (its save refuses an edit).
        if s.kept.is_some() {
            continue;
        }
        if let Some(anchor) = keep(&s.anchor, s.edit_as.as_deref()) {
            s.anchor = anchor;
            any = true;
        }
    }
    let mut out = Vec::new();
    if any {
        // A shape whose corner would lie in inserted cells has no place in the file's coordinates: left as it is.
        if let Ok(shapes) = frame.stored(host, host.shapes(sheet), shapes) {
            out.push(CellEdit::Shapes { sheet, shapes });
        }
    }
    let before_charts = host.model().definitions().charts(sheet);
    if !before_charts.is_empty() {
        let mut charts = before_charts.to_vec();
        let mut charts_any = false;
        for (orig, c) in before_charts.iter().zip(&mut charts) {
            if !matches!(c.edit_as.as_deref(), Some("oneCell" | "absolute")) {
                continue;
            }
            let Some(current) = frame.map_anchor(&orig.anchor, orig.edit_as.as_deref(), false) else {
                continue;
            };
            let Some(kept) = keep(&current, orig.edit_as.as_deref()) else {
                continue;
            };
            let Some(stored) = frame.map_anchor(&kept, orig.edit_as.as_deref(), true) else {
                continue;
            };
            if stored != orig.anchor {
                c.anchor = stored;
                charts_any = true;
            }
        }
        if charts_any && host.loaded().check_chart_set(sheet, &charts).is_ok() {
            out.push(CellEdit::Charts { sheet, charts });
        }
    }
    out
}
