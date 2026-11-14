//! Drawing objects the object model and commands add to a sheet (dc-piw0.3, dc-zjsg.1): auto
//! shapes, text boxes, lines and connectors. A command adds, changes or deletes one object of
//! the sheet's drawing in one transaction; positions arrive in points, as VBA gives them, and
//! are stored as the cell anchor Excel writes. Objects the file holds are never changed here
//! (dc-zjsg.4); an edit of one is refused before anything changes.

use crate::{CellEdit, HostState};
use calc_doc_model::{
    ChartAnchor, DrawingPoint, Shape, ShapeConnection, ShapeFill, ShapeKind, ShapeLine,
};
use calc_value::SheetId;

const EMU_PER_POINT: f64 = 12_700.0;
/// Excel's maximum digit width of the default font, in pixels (Calibri 11, Aptos Narrow 11).
const DIGIT_PIXELS: f64 = 7.0;
const DEFAULT_ROW_POINTS: f64 = 15.0;
const DEFAULT_COLUMN_PIXELS: f64 = 64.0;

/// A length in points as its binary64 bits, so commands compare exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Points(u64);
impl Points {
    pub fn new(points: f64) -> Points {
        Points(points.to_bits())
    }
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
        kind: ShapeKind,
        geometry: String,
        left: Points,
        top: Points,
        width: Points,
        height: Points,
        /// `None`: Excel's default name for the kind.
        name: Option<String>,
    },
    Delete {
        id: u32,
    },
    Name {
        id: u32,
        name: String,
    },
    /// Any of the rectangle's sides, in points; `None` keeps it.
    Place {
        id: u32,
        left: Option<Points>,
        top: Option<Points>,
        width: Option<Points>,
        height: Option<Points>,
    },
    /// The text, paragraphs split at `\r`, `\n` or `\r\n`.
    Text {
        id: u32,
        text: String,
    },
    Fill {
        id: u32,
        fill: ShapeFill,
    },
    Line {
        id: u32,
        line: ShapeLine,
    },
    /// Glue one end of a connector to a connection site (1-based as VBA counts) of a shape.
    Connect {
        id: u32,
        begin: bool,
        shape: u32,
        site: u32,
    },
}

fn refusal(s: &str) -> String {
    format!("{s} (dc-piw0.3); nothing changed")
}

/// A shape's default name: Excel numbers it from its id (the first object of a drawing has id 2
/// and is `Rectangle 1`). Unmeasured; see excel-questions.md Q1.
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

impl HostState {
    /// The drawing objects of `sheet` other than charts, in drawing order.
    pub fn shapes(&self, sheet: SheetId) -> &[Shape] {
        self.model().definitions().shapes(sheet)
    }

    fn column_points(&self, sheet: SheetId, column: u32) -> f64 {
        let Some(loaded) = self.loaded().sheet(sheet) else {
            return DEFAULT_COLUMN_PIXELS * 0.75;
        };
        let format = self.model().definitions().axis_layouts().get(
            sheet,
            calc_doc_model::Axis::Columns,
            column,
        );
        if format.hidden() {
            return 0.0;
        }
        let geometry = loaded.geometry();
        let characters = format
            .size()
            .or(geometry.default_column_width)
            .or_else(|| geometry.base_column_width.map(|_| 0.0).filter(|_| false));
        let pixels = match characters {
            Some(width) => {
                (((256.0 * width + (128.0 / DIGIT_PIXELS).trunc()) / 256.0) * DIGIT_PIXELS).trunc()
            }
            None => DEFAULT_COLUMN_PIXELS,
        };
        pixels * 0.75
    }

    fn row_points(&self, sheet: SheetId, row: u32) -> f64 {
        let format =
            self.model()
                .definitions()
                .axis_layouts()
                .get(sheet, calc_doc_model::Axis::Rows, row);
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
            .unwrap_or(DEFAULT_ROW_POINTS)
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

    /// A shape's rectangle in points: left, top, width, height.
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

/// The connection sites of a preset, as (x, y) fractions of its rectangle in Excel's order
/// (the preset shape definitions' `cxnLst`): a rectangle's top, left, bottom, right; an
/// ellipse's eight sites from the top, counter-clockwise.
fn sites(geometry: &str) -> Option<&'static [(f64, f64)]> {
    const RECT: [(f64, f64); 4] = [(0.5, 0.0), (0.0, 0.5), (0.5, 1.0), (1.0, 0.5)];
    const S: f64 = 0.146_446_609_406_726_24; // (1 - cos 45°) / 2
    const ELLIPSE: [(f64, f64); 8] = [
        (0.5, 0.0),
        (S, S),
        (0.0, 0.5),
        (S, 1.0 - S),
        (0.5, 1.0),
        (1.0 - S, 1.0 - S),
        (1.0, 0.5),
        (1.0 - S, S),
    ];
    match geometry {
        "rect" | "roundRect" => Some(&RECT),
        "ellipse" => Some(&ELLIPSE),
        _ => None,
    }
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
    if !host.axis_edits.is_empty() {
        return Err(refusal(
            "Save and reopen before adding shapes after row or column insertion or deletion",
        ));
    }
    let originals = host.loaded().model().definitions().shapes(sheet).len();
    let mut shapes = host.shapes(sheet).to_vec();
    let find = |shapes: &[Shape], id: u32| -> Result<usize, String> {
        let index = shapes
            .iter()
            .position(|s| s.id == id)
            .ok_or_else(|| refusal("The shape no longer exists"))?;
        if index < originals {
            return Err(refusal(
                "A shape the file holds cannot be changed yet (dc-zjsg.4)",
            ));
        }
        Ok(index)
    };
    let next_id = || -> u32 {
        let charts = host.model().definitions().charts(sheet).len() as u32;
        shapes
            .iter()
            .map(|s| s.id)
            .max()
            .unwrap_or(1)
            .max(1 + charts)
            + 1
    };
    let finite = |v: f64| v.is_finite() && (-1.0e7..=1.0e7).contains(&v);
    match change {
        ShapeChange::Add {
            kind,
            geometry,
            left,
            top,
            width,
            height,
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
            shape.offset = (
                (left * EMU_PER_POINT).round() as i64,
                (top * EMU_PER_POINT).round() as i64,
            );
            shape.extent = (
                (width * EMU_PER_POINT).round() as i64,
                (height * EMU_PER_POINT).round() as i64,
            );
            shapes.push(shape);
        }
        ShapeChange::Delete { id } => {
            let index = find(&shapes, id)?;
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
            let index = find(&shapes, id)?;
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
            let index = find(&shapes, id)?;
            let (l, t, w, h) = host.shape_rectangle(sheet, &shapes[index]);
            let (l, t, w, h) = (
                left.map_or(l, Points::get),
                top.map_or(t, Points::get),
                width.map_or(w, Points::get),
                height.map_or(h, Points::get),
            );
            if ![l, t, w, h].into_iter().all(finite) || l < 0.0 || t < 0.0 || w < 0.0 || h < 0.0 {
                return Err(refusal("Use a position and size on the worksheet"));
            }
            let s = &mut shapes[index];
            s.anchor = ChartAnchor::TwoCell {
                from: host.point_at(sheet, l, t),
                to: host.point_at(sheet, l + w, t + h),
            };
            s.offset = (
                (l * EMU_PER_POINT).round() as i64,
                (t * EMU_PER_POINT).round() as i64,
            );
            s.extent = (
                (w * EMU_PER_POINT).round() as i64,
                (h * EMU_PER_POINT).round() as i64,
            );
        }
        ShapeChange::Text { id, text } => {
            let index = find(&shapes, id)?;
            if shapes[index].kind == ShapeKind::Connector {
                return Err(refusal("A connector has no text"));
            }
            let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
            shapes[index].text = Some(normalized.split('\n').map(str::to_owned).collect());
            if text.is_empty() {
                shapes[index].text = None;
            }
        }
        ShapeChange::Fill { id, fill } => {
            let index = find(&shapes, id)?;
            shapes[index].fill = fill;
        }
        ShapeChange::Line { id, line } => {
            let index = find(&shapes, id)?;
            shapes[index].line = line;
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
            let table = shapes[glued]
                .geometry
                .as_deref()
                .and_then(sites)
                .ok_or_else(|| {
                    refusal("Connection sites of this shape are not modelled (dc-zjsg.5)")
                })?;
            if site == 0 || site as usize > table.len() {
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
            // The connector's rectangle follows its glued ends; an unglued end keeps its corner.
            let point = |s: &Shape, c: ShapeConnection| -> Option<(f64, f64)> {
                let g = shapes.iter().find(|g| g.id == c.shape)?;
                let (l, t, w, h) = host.shape_rectangle(sheet, g);
                let (fx, fy) = sites(g.geometry.as_deref()?)?.get(c.site as usize)?;
                let _ = s;
                Some((l + w * fx, t + h * fy))
            };
            let c = shapes[index].clone();
            let (l, t, w, h) = host.shape_rectangle(sheet, &c);
            let start = c
                .begin
                .and_then(|e| point(&c, e))
                .unwrap_or(if c.flip_horizontal {
                    (l + w, t)
                } else {
                    (l, t)
                });
            let end = c.end.and_then(|e| point(&c, e)).unwrap_or((l + w, t + h));
            let (x0, x1) = (start.0.min(end.0), start.0.max(end.0));
            let (y0, y1) = (start.1.min(end.1), start.1.max(end.1));
            let s = &mut shapes[index];
            s.flip_horizontal = start.0 > end.0;
            s.flip_vertical = start.1 > end.1;
            s.anchor = ChartAnchor::TwoCell {
                from: host.point_at(sheet, x0, y0),
                to: host.point_at(sheet, x1, y1),
            };
            s.offset = (
                (x0 * EMU_PER_POINT).round() as i64,
                (y0 * EMU_PER_POINT).round() as i64,
            );
            s.extent = (
                ((x1 - x0) * EMU_PER_POINT).round() as i64,
                ((y1 - y0) * EMU_PER_POINT).round() as i64,
            );
        }
    }
    host.loaded()
        .check_shape_set(sheet, &shapes)
        .map_err(|e| e.to_string())?;
    Ok(vec![CellEdit::Shapes { sheet, shapes }])
}
