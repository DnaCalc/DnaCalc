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
/// The pixel density shape positions are measured at (dc-piw0 Q9): points are DPI free, a
/// column's pixels scale with it.
const DPI: f64 = crate::facts::REFERENCE_DPI;

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
        /// Horizontal and vertical flip (a line drawn right to left or bottom to top).
        flips: (bool, bool),
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

    /// The maximum digit width in pixels at 96 DPI of the workbook's default font (Aptos Narrow 11:
    /// 8, Calibri 11: 7; a font the measure does not know falls back to Calibri 11's 7), which a
    /// column width in characters counts in. The display reads it to draw columns as wide as Excel.
    pub fn default_digit_px(&self) -> f64 {
        crate::facts::maximum_digit_width_at(self.styles(), DPI).unwrap_or(7.0)
    }

    /// The default row height in points of the workbook's default font (Aptos Narrow 11: 14.25,
    /// otherwise 15) when the sheet gives none.
    pub fn default_font_row_points(&self) -> f64 {
        crate::facts::default_row_points(self.styles())
    }

    /// The width in points of a column: whole pixels of the workbook's default font's digit
    /// width (Aptos Narrow 11: 8 px, Calibri 11: 7 px), converted at the pixel density.
    fn column_points(&self, sheet: SheetId, column: u32) -> f64 {
        // A font the measure does not know falls back to Calibri 11's 7 pixels.
        let mdw = crate::facts::maximum_digit_width_at(self.styles(), DPI).unwrap_or(7.0);
        let format = self.model().definitions().axis_layouts().get(
            sheet,
            calc_doc_model::Axis::Columns,
            column,
        );
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
        let mdw = crate::facts::maximum_digit_width_at(self.styles(), 96.0).unwrap_or(7.0);
        let format = self.model().definitions().axis_layouts().get(
            sheet,
            calc_doc_model::Axis::Columns,
            column,
        );
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

    /// A shape's rectangle in points: left, top, width, height.
    /// An anchor's rectangle in points (left, top, width, height), from the sheet's column widths and row heights.
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

/// A shape's rectangle in points: a stored one's from its anchor, one this session made from its transform.
fn rectangle(host: &HostState, sheet: SheetId, shapes: &[Shape], index: usize, originals: usize) -> (f64, f64, f64, f64) {
    if index < originals {
        host.anchor_rectangle(sheet, &shapes[index].anchor)
    } else {
        host.shape_rectangle(sheet, &shapes[index])
    }
}

/// Where connection site `site` (counted from 0, as `stCxn idx`) of `shape` is, in points, its box `rect`: from its
/// geometry's definition (a preset's from ECMA-376, a custom geometry's own `cxnLst`), flipped and turned about the
/// box's centre as the shape is. `None` for a site the geometry does not have.
fn site_point(shape: &Shape, rect: (f64, f64, f64, f64), site: u32) -> Option<(f64, f64)> {
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
    let (l, t, w, h) = rect;
    let (cx, cy) = (l + w / 2.0, t + h / 2.0);
    let (x, y) = (l + fx * w - cx, t + fy * h - cy);
    let (sin, cos) = (shape.rotation as f64 / 60_000.0).to_radians().sin_cos();
    Some((cx + x * cos - y * sin, cy + x * sin + y * cos))
}

/// The connectors glued to the shape `id` (dc-zjsg.5), each rerouted ([`reroute`]). A turned connector is left as it is.
fn reroute_connectors(host: &HostState, sheet: SheetId, shapes: &mut [Shape], id: u32, originals: usize) {
    let glued: Vec<usize> = (0..shapes.len())
        .filter(|&i| shapes[i].rotation == 0 && [shapes[i].begin, shapes[i].end].iter().any(|e| e.is_some_and(|e| e.shape == id)))
        .collect();
    for index in glued {
        reroute(host, sheet, shapes, index, originals);
    }
}

/// The connector at `index` redrawn between its ends as Excel reroutes one when a shape it joins moves or it is
/// glued: a glued end at its site, a free end where it was; its box the ends' box, flipped to run from its start to
/// its end.
fn reroute(host: &HostState, sheet: SheetId, shapes: &mut [Shape], index: usize, originals: usize) {
    {
        let (l, t, w, h) = rectangle(host, sheet, shapes, index, originals);
        let c = &shapes[index];
        let mut start = (if c.flip_horizontal { l + w } else { l }, if c.flip_vertical { t + h } else { t });
        let mut end = (if c.flip_horizontal { l } else { l + w }, if c.flip_vertical { t } else { t + h });
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
        let (l, t) = (start.0.min(end.0).max(0.0), start.1.min(end.1).max(0.0));
        let (w, h) = ((end.0 - start.0).abs(), (end.1 - start.1).abs());
        let emu = |v: f64| (v * EMU_PER_POINT).round() as i64;
        let stored = index < originals;
        let c = &mut shapes[index];
        c.anchor = match (stored, &c.anchor) {
            (true, ChartAnchor::OneCell { .. }) => ChartAnchor::OneCell { from: host.point_at(sheet, l, t), width: emu(w), height: emu(h) },
            (true, ChartAnchor::Absolute { .. }) => ChartAnchor::Absolute { x: emu(l), y: emu(t), width: emu(w), height: emu(h) },
            _ => ChartAnchor::TwoCell { from: host.point_at(sheet, l, t), to: host.point_at(sheet, l + w, t + h) },
        };
        c.offset = (emu(l), emu(t));
        c.extent = (emu(w), emu(h));
        c.flip_horizontal = end.0 < start.0;
        c.flip_vertical = end.1 < start.1;
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
    // A shape the file holds may be moved, resized or deleted (dc-zjsg.4): the save rewrites its stored anchor in
    // place, or drops it. One Excel keeps for an object DNA Calc does not edit (an equation, ink, SmartArt, an OLE or
    // ActiveX object) is not, nor a hidden one, nor one a stored connector is glued to (dc-zjsg.5).
    let find_movable = |shapes: &[Shape], id: u32, deleting: bool| -> Result<usize, String> {
        let index = shapes
            .iter()
            .position(|s| s.id == id && !s.hidden)
            .ok_or_else(|| refusal("The shape no longer exists"))?;
        if index >= originals {
            return Ok(index);
        }
        let shape = &shapes[index];
        if shape.kept.is_some() {
            return Err(refusal("An equation, ink, SmartArt, OLE or ActiveX object is kept as the file stores it"));
        }
        if deleting && shapes[..originals].iter().any(|s| [s.begin, s.end].iter().any(|e| e.is_some_and(|e| e.shape == id))) {
            return Err(refusal("A shape a stored connector is glued to cannot be deleted yet (dc-zjsg.5)"));
        }
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
            let index = find_movable(&shapes, id, false)?;
            let stored = index < originals;
            let (l, t, w, h) = if stored {
                host.anchor_rectangle(sheet, &shapes[index].anchor)
            } else {
                host.shape_rectangle(sheet, &shapes[index])
            };
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
            let emu = |v: f64| (v * EMU_PER_POINT).round() as i64;
            // A stored object keeps its anchor's kind (Excel moves a one-cell or absolute object as one).
            s.anchor = match (stored, &s.anchor) {
                (true, ChartAnchor::OneCell { .. }) => ChartAnchor::OneCell { from: host.point_at(sheet, l, t), width: emu(w), height: emu(h) },
                (true, ChartAnchor::Absolute { .. }) => ChartAnchor::Absolute { x: emu(l), y: emu(t), width: emu(w), height: emu(h) },
                _ => ChartAnchor::TwoCell { from: host.point_at(sheet, l, t), to: host.point_at(sheet, l + w, t + h) },
            };
            s.offset = (
                (l * EMU_PER_POINT).round() as i64,
                (t * EMU_PER_POINT).round() as i64,
            );
            s.extent = (
                (w * EMU_PER_POINT).round() as i64,
                (h * EMU_PER_POINT).round() as i64,
            );
            reroute_connectors(host, sheet, &mut shapes, id, originals);
        }
        ShapeChange::Text { id, text } => {
            let index = find(&shapes, id)?;
            if shapes[index].kind == ShapeKind::Connector {
                return Err(refusal("A connector has no text"));
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
    host.loaded()
        .check_shape_set(sheet, &shapes)
        .map_err(|e| e.to_string())?;
    Ok(vec![CellEdit::Shapes { sheet, shapes }])
}

