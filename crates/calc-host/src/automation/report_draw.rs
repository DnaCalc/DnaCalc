//! Charts and drawings of the report objects (dc-piw0.2, dc-piw0.3): `ChartObjects`, `Chart`,
//! `Shapes`, `Shape` and the Office members reached from a shape. See `report.rs` for the rules
//! (descriptor-registered handlers, command context writes, uniform refusals).

use super::report::{PendingChart, report, text_arg};
use super::*;
use crate::{Points, ShapeChange, WorkbookCommand};
use calc_doc_model::{ChartAnchor, Shape, ShapeColour, ShapeFill, ShapeKind, ShapeLine};
use calc_object_model::ReportItem;

const CHARTS: &str = "dc-piw0.2";
const SHAPES: &str = "dc-piw0.3";
const EMU_PER_POINT: f64 = 12_700.0;

fn single(value: f64) -> AutoValue {
    AutoValue::R4(value as f32)
}

fn number(value: &AutoValue) -> Result<f64, AutoError> {
    match value {
        AutoValue::I2(v) => Ok(f64::from(*v)),
        AutoValue::I4(v) => Ok(f64::from(*v)),
        AutoValue::R4(v) => Ok(f64::from(*v)),
        AutoValue::R8(v) => Ok(*v),
        _ => Err(geometry::type_mismatch()),
    }
}

/// Which side of a rectangle a Left/Top/Width/Height member is.
#[derive(Clone, Copy)]
enum Side {
    Left,
    Top,
    Width,
    Height,
}

fn rectangle_with(side: Side, value: f64) -> [Option<Points>; 4] {
    let mut out = [None; 4];
    out[side as usize] = Some(Points::new(value));
    out
}

fn rgb_to_colour(value: i64) -> Result<ShapeColour, AutoError> {
    if !(0..=0xFF_FFFF).contains(&value) {
        return Err(geometry::type_mismatch());
    }
    let (r, g, b) = (value & 0xFF, (value >> 8) & 0xFF, (value >> 16) & 0xFF);
    Ok(ShapeColour::Rgb(((r << 16) | (g << 8) | b) as u32))
}

fn colour_to_rgb(hex: u32) -> i64 {
    let v = i64::from(hex);
    ((v >> 16) & 0xFF) | (v & 0xFF00) | ((v & 0xFF) << 16)
}

impl WorkbookAutomation {
    // --- charts ---------------------------------------------------------------------------------

    fn chart_token(target: &ObjectRef) -> Result<String, AutoError> {
        match &target.key {
            ObjectKey::Report {
                item:
                    ReportItem::ChartObject(token)
                    | ReportItem::Chart(token)
                    | ReportItem::ChartTitle(token),
                ..
            } => Ok(token.clone()),
            _ => Err(geometry::type_mismatch()),
        }
    }

    /// The chart a token denotes: its part when real, else the index of its pending frame. A pending
    /// frame whose source data was set resolves to the chart it made.
    fn resolve_chart(&self, token: &str) -> Result<ChartRef, AutoError> {
        match token.strip_prefix("pending:") {
            Some(n) => {
                let index: usize = n.parse().map_err(|_| geometry::type_mismatch())?;
                let pending = self.pending_charts.borrow();
                let frame = pending
                    .get(index)
                    .ok_or_else(|| AutoError::excel(424, "Object required"))?;
                Ok(match &frame.part {
                    Some(part) => ChartRef::Real(part.clone()),
                    None => ChartRef::Pending(index),
                })
            }
            None => Ok(ChartRef::Real(token.to_owned())),
        }
    }

    fn charts_of(session: &HostSession, sheet: SheetId) -> Vec<calc_doc_model::Chart> {
        session.host().model().definitions().charts(sheet).to_vec()
    }

    pub(super) fn chart_objects_count(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let waiting = self
            .pending_charts
            .borrow()
            .iter()
            .filter(|p| p.part.is_none())
            .count();
        let (_, sheet, _) = Self::report_key(target)?;
        self.read_host(target, |s, _, _| {
            Ok(AutoValue::I4(
                (Self::charts_of(s, sheet).len() + waiting) as i32,
            ))
        })
    }
    pub(super) fn chart_objects_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        let (workbook, _, _) = Self::report_key(target)?;
        let waiting: Vec<usize> = self
            .pending_charts
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, p)| p.part.is_none())
            .map(|(i, _)| i)
            .collect();
        self.read_host(target, |s, sheet, _| {
            let charts = Self::charts_of(s, sheet);
            let by_name = |name: &str| -> Option<String> {
                charts
                    .iter()
                    .find(|c| c.name.eq_ignore_ascii_case(name))
                    .map(|c| c.part.clone())
                    .or_else(|| {
                        let pending = self.pending_charts.borrow();
                        waiting
                            .iter()
                            .find(|&&i| {
                                pending[i]
                                    .name
                                    .as_deref()
                                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
                            })
                            .map(|i| format!("pending:{i}"))
                    })
            };
            let token = match call.arg(0) {
                AutoValue::Text(name) => by_name(&name.to_string_lossy()),
                other => {
                    let n = usize::try_from(geometry::integer(other)? - 1).ok();
                    n.and_then(|n| {
                        charts.get(n).map(|c| c.part.clone()).or_else(|| {
                            waiting
                                .get(n - charts.len().min(n))
                                .map(|i| format!("pending:{i}"))
                        })
                    })
                }
            }
            .ok_or_else(|| AutoError::excel(9, "Subscript out of range"))?;
            Ok(report(
                workbook,
                sheet,
                "ChartObject",
                ReportItem::ChartObject(token),
            ))
        })
    }
    /// `ChartObjects.Add(Left, Top, Width, Height)`: a frame that waits for `SetSourceData`.
    pub(super) fn chart_objects_add(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 4 {
            return Err(geometry::bad_count());
        }
        let (workbook, sheet, _) = Self::report_key(target)?;
        let mut place = [0.0; 4];
        for (i, slot) in place.iter_mut().enumerate() {
            *slot = number(call.arg(i))?;
        }
        self.with_target(target, |_, _| Ok(AutoValue::Empty))?;
        let mut pending = self.pending_charts.borrow_mut();
        pending.push(PendingChart {
            place,
            ..Default::default()
        });
        Ok(report(
            workbook,
            sheet,
            "ChartObject",
            ReportItem::ChartObject(format!("pending:{}", pending.len() - 1)),
        ))
    }
    pub(super) fn chart_object_name(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let token = Self::chart_token(target)?;
        match self.resolve_chart(&token)? {
            ChartRef::Pending(i) => {
                // Excel's default name is unmeasured (excel-questions.md Q1): "Chart " + the
                // number of the object on the sheet.
                let name = self.pending_charts.borrow()[i].name.clone();
                Ok(AutoValue::text(
                    &name.unwrap_or_else(|| format!("Chart {}", i + 1)),
                ))
            }
            ChartRef::Real(part) => self.read_host(target, |s, sheet, _| {
                Self::charts_of(s, sheet)
                    .into_iter()
                    .find(|c| c.part == part)
                    .map(|c| AutoValue::text(&c.name))
                    .ok_or_else(|| AutoError::excel(424, "Object required"))
            }),
        }
    }
    pub(super) fn chart_object_put_name(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let name = text_arg(call.args.last().ok_or_else(geometry::bad_count)?)?;
        self.chart_frame(target, call, Some(name), None)
    }
    fn chart_frame(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        name: Option<String>,
        side: Option<(Side, f64)>,
    ) -> Reply {
        let token = Self::chart_token(target)?;
        let (_, sheet, _) = Self::report_key(target)?;
        match self.resolve_chart(&token)? {
            ChartRef::Pending(i) => {
                let mut pending = self.pending_charts.borrow_mut();
                if let Some(name) = name {
                    pending[i].name = Some(name);
                }
                if let Some((side, value)) = side {
                    pending[i].place[side as usize] = value;
                }
                Ok(AutoValue::Empty)
            }
            ChartRef::Real(part) => {
                let place = side.map(|(side, value)| (side, value));
                self.command(call, |s| {
                    let current = Self::chart_rectangle(s, sheet, &part)?;
                    let mut rect = current;
                    if let Some((side, value)) = place {
                        rect[side as usize] = value;
                    }
                    Ok(WorkbookCommand::ChartFrame {
                        sheet,
                        revision: s.host().revision().get(),
                        part: part.clone(),
                        name: name.clone(),
                        place: place.map(|_| rect.map(Points::new)),
                    })
                })
            }
        }
    }
    /// A chart's rectangle in points (left, top, width, height) from its anchor.
    fn chart_rectangle(
        session: &HostSession,
        sheet: SheetId,
        part: &str,
    ) -> Result<[f64; 4], AutoError> {
        let host = session.host();
        let chart = host
            .model()
            .definitions()
            .charts(sheet)
            .iter()
            .find(|c| c.part == part)
            .ok_or_else(|| AutoError::excel(424, "Object required"))?;
        let corner = |p: &calc_doc_model::DrawingPoint| {
            (
                host.column_left(sheet, p.column) + p.column_offset as f64 / EMU_PER_POINT,
                host.row_top(sheet, p.row) + p.row_offset as f64 / EMU_PER_POINT,
            )
        };
        Ok(match chart.anchor {
            ChartAnchor::TwoCell { from, to } => {
                let (l, t) = corner(&from);
                let (r, b) = corner(&to);
                [l, t, r - l, b - t]
            }
            ChartAnchor::OneCell {
                from,
                width,
                height,
            } => {
                let (l, t) = corner(&from);
                [
                    l,
                    t,
                    width as f64 / EMU_PER_POINT,
                    height as f64 / EMU_PER_POINT,
                ]
            }
            ChartAnchor::Absolute {
                x,
                y,
                width,
                height,
            } => [
                x as f64 / EMU_PER_POINT,
                y as f64 / EMU_PER_POINT,
                width as f64 / EMU_PER_POINT,
                height as f64 / EMU_PER_POINT,
            ],
        })
    }
    fn chart_side(&self, target: &ObjectRef, call: &Call<'_>, side: Side) -> Reply {
        no_args(call)?;
        let token = Self::chart_token(target)?;
        match self.resolve_chart(&token)? {
            ChartRef::Pending(i) => {
                Ok(single(self.pending_charts.borrow()[i].place[side as usize]))
            }
            ChartRef::Real(part) => self.read_host(target, |s, sheet, _| {
                Ok(single(
                    Self::chart_rectangle(s, sheet, &part)?[side as usize],
                ))
            }),
        }
    }
    fn chart_put_side(&self, target: &ObjectRef, call: &Call<'_>, side: Side) -> Reply {
        let value = number(call.args.last().ok_or_else(geometry::bad_count)?)?;
        self.chart_frame(target, call, None, Some((side, value)))
    }
    pub(super) fn chart_object_left(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.chart_side(t, c, Side::Left)
    }
    pub(super) fn chart_object_top(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.chart_side(t, c, Side::Top)
    }
    pub(super) fn chart_object_width(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.chart_side(t, c, Side::Width)
    }
    pub(super) fn chart_object_height(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.chart_side(t, c, Side::Height)
    }
    pub(super) fn chart_object_put_left(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.chart_put_side(t, c, Side::Left)
    }
    pub(super) fn chart_object_put_top(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.chart_put_side(t, c, Side::Top)
    }
    pub(super) fn chart_object_put_width(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.chart_put_side(t, c, Side::Width)
    }
    pub(super) fn chart_object_put_height(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.chart_put_side(t, c, Side::Height)
    }
    pub(super) fn chart_object_chart(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let token = Self::chart_token(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        Ok(report(workbook, sheet, "Chart", ReportItem::Chart(token)))
    }

    fn real_chart<T>(
        &self,
        target: &ObjectRef,
        read: impl Fn(&calc_doc_model::Chart) -> T,
    ) -> Result<Option<T>, AutoError> {
        let token = Self::chart_token(target)?;
        match self.resolve_chart(&token)? {
            ChartRef::Pending(_) => Ok(None),
            ChartRef::Real(part) => self
                .read_host(target, |s, sheet, _| {
                    Self::charts_of(s, sheet)
                        .iter()
                        .find(|c| c.part == part)
                        .map(&read)
                        .ok_or_else(|| AutoError::excel(424, "Object required"))
                })
                .map(Some),
        }
    }
    fn pending_of(&self, target: &ObjectRef) -> Result<Option<usize>, AutoError> {
        match self.resolve_chart(&Self::chart_token(target)?)? {
            ChartRef::Pending(i) => Ok(Some(i)),
            ChartRef::Real(_) => Ok(None),
        }
    }
    pub(super) fn chart_type(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        if let Some(i) = self.pending_of(target)? {
            // An empty chart's type is unmeasured (Q6); a macro that never set one has the
            // column default.
            return Ok(AutoValue::I4(
                self.pending_charts.borrow()[i].kind.unwrap_or(51) as i32,
            ));
        }
        let kind = self
            .real_chart(target, |c| {
                let plot = c.plots.first()?;
                let stacked =
                    matches!(plot.grouping.as_deref(), Some("stacked" | "percentStacked"));
                Some(
                    match (plot.kind.as_str(), plot.bar_direction.as_deref(), stacked) {
                        ("barChart", Some("bar"), false) => 57,
                        ("barChart", Some("bar"), true) => 58,
                        ("barChart", _, false) => 51,
                        ("barChart", _, true) => 52,
                        ("lineChart", _, _) => 4,
                        ("areaChart", _, false) => 1,
                        ("areaChart", _, true) => 76,
                        ("pieChart", _, _) => 5,
                        ("doughnutChart", _, _) => -4120,
                        ("scatterChart", _, _) => -4169,
                        _ => 0,
                    },
                )
            })?
            .flatten()
            .filter(|k| *k != 0)
            .ok_or_else(|| AutoError::not_modelled("Chart.ChartType of this chart", CHARTS))?;
        Ok(AutoValue::I4(kind))
    }
    pub(super) fn chart_put_type(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let xl = geometry::integer(call.args.last().ok_or_else(geometry::bad_count)?)?;
        let (kind, stacked) = chart_kind(xl)?;
        if let Some(i) = self.pending_of(target)? {
            self.pending_charts.borrow_mut()[i].kind = Some(xl);
            return Ok(AutoValue::Empty);
        }
        let (_, sheet, _) = Self::report_key(target)?;
        let ChartRef::Real(part) = self.resolve_chart(&Self::chart_token(target)?)? else {
            return Err(geometry::type_mismatch());
        };
        self.command(call, |s| {
            Ok(WorkbookCommand::ChartType {
                sheet,
                revision: s.host().revision().get(),
                part: part.clone(),
                plot: 0,
                kind: kind.into(),
                stacked,
            })
        })
    }
    pub(super) fn chart_has_title(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        if let Some(i) = self.pending_of(target)? {
            return Ok(AutoValue::Bool(
                self.pending_charts.borrow()[i].has_title.unwrap_or(false),
            ));
        }
        let title = self.real_chart(target, |c| c.title.is_some())?;
        Ok(AutoValue::Bool(title.unwrap_or(false)))
    }
    pub(super) fn chart_put_has_title(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let on = geometry::boolean(call.args.last().ok_or_else(geometry::bad_count)?)?;
        if let Some(i) = self.pending_of(target)? {
            let mut pending = self.pending_charts.borrow_mut();
            pending[i].has_title = Some(on);
            if !on {
                pending[i].title = None;
            }
            return Ok(AutoValue::Empty);
        }
        // Switching a title on needs its text: Excel shows "Chart Title"; the title text is set
        // by ChartTitle.Text, which this member leaves to that (a macro sets both).
        let title = if on {
            Some("Chart Title".to_owned())
        } else {
            None
        };
        self.chart_properties(target, call, Some(title), None)
    }
    fn chart_properties(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        title: Option<Option<String>>,
        legend: Option<bool>,
    ) -> Reply {
        let (_, sheet, _) = Self::report_key(target)?;
        let ChartRef::Real(part) = self.resolve_chart(&Self::chart_token(target)?)? else {
            return Err(geometry::type_mismatch());
        };
        self.command(call, |s| {
            Ok(WorkbookCommand::ChartProperties {
                sheet,
                revision: s.host().revision().get(),
                part: part.clone(),
                set_title: title.is_some(),
                title: title.clone().flatten(),
                legend,
            })
        })
    }
    pub(super) fn chart_title(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let token = Self::chart_token(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        Ok(report(
            workbook,
            sheet,
            "ChartTitle",
            ReportItem::ChartTitle(token),
        ))
    }
    pub(super) fn chart_has_legend(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        if let Some(i) = self.pending_of(target)? {
            return Ok(AutoValue::Bool(
                self.pending_charts.borrow()[i].has_legend.unwrap_or(false),
            ));
        }
        Ok(AutoValue::Bool(
            self.real_chart(target, |c| c.legend.is_some())?
                .unwrap_or(false),
        ))
    }
    pub(super) fn chart_put_has_legend(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let on = geometry::boolean(call.args.last().ok_or_else(geometry::bad_count)?)?;
        if let Some(i) = self.pending_of(target)? {
            self.pending_charts.borrow_mut()[i].has_legend = Some(on);
            return Ok(AutoValue::Empty);
        }
        self.chart_properties(target, call, None, Some(on))
    }
    pub(super) fn chart_title_text(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        if let Some(i) = self.pending_of(target)? {
            let title = self.pending_charts.borrow()[i].title.clone();
            return title
                .map(|t| AutoValue::text(&t))
                .ok_or_else(|| AutoError::excel(1004, ""));
        }
        self.real_chart(target, |c| c.title.clone())?
            .flatten()
            .map(|t| AutoValue::text(&t))
            .ok_or_else(|| AutoError::excel(1004, ""))
    }
    pub(super) fn chart_title_put_text(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let text = text_arg(call.args.last().ok_or_else(geometry::bad_count)?)?;
        if let Some(i) = self.pending_of(target)? {
            let mut pending = self.pending_charts.borrow_mut();
            pending[i].title = Some(text);
            pending[i].has_title = Some(true);
            return Ok(AutoValue::Empty);
        }
        self.chart_properties(target, call, Some(Some(text)), None)
    }
    /// `Chart.SetSourceData(Source, PlotBy)` on a frame made by `ChartObjects.Add`: creates the
    /// chart over the range (series in rows when PlotBy is xlRows, else the shorter side of the
    /// range), then applies the name, place, title and legend held so far. A chart that already
    /// has data refuses (dc-piw0.2: the series commands exist, the member does not use them yet).
    pub(super) fn chart_set_source(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let Some(i) = self.pending_of(target)? else {
            return Err(AutoError::not_modelled(
                "Chart.SetSourceData on a chart that has data",
                CHARTS,
            ));
        };
        let (_, sheet, _) = Self::report_key(target)?;
        let range = self.source_range(target, call.arg(0), sheet)?;
        if range.area_count() != 1 {
            return Err(AutoError::excel(1004, ""));
        }
        let area = range.areas()[0];
        let frame = self.pending_charts.borrow()[i].clone();
        let (kind, _) = chart_kind(frame.kind.unwrap_or(51))?;
        let rows = match call.arg(1) {
            AutoValue::Missing => area.row_count() < area.column_count(),
            other => geometry::integer(other)? == 1,
        };
        let mut made = None;
        self.command(call, |s| {
            let (headers, categories) = Self::source_layout(s, area, rows);
            let revision = s.host().revision().get();
            made = Some((headers, categories));
            Ok(WorkbookCommand::ChartCreate {
                revision,
                area,
                kind: kind.into(),
                rows,
                headers,
                categories,
            })
        })?;
        let _ = made;
        let part = self.read_host(target, |s, sheet, _| {
            Self::charts_of(s, sheet)
                .last()
                .map(|c| AutoValue::text(&c.part))
                .ok_or_else(|| AutoError::excel(1004, ""))
        })?;
        let part = part.as_text().ok_or_else(busy)?;
        let place = frame.place.map(Points::new);
        self.command(call, |s| {
            Ok(WorkbookCommand::ChartFrame {
                sheet,
                revision: s.host().revision().get(),
                part: part.clone(),
                name: frame.name.clone(),
                place: Some(place),
            })
        })?;
        if frame.has_title.is_some() || frame.title.is_some() || frame.has_legend.is_some() {
            self.command(call, |s| {
                Ok(WorkbookCommand::ChartProperties {
                    sheet,
                    revision: s.host().revision().get(),
                    part: part.clone(),
                    set_title: frame.has_title.is_some() || frame.title.is_some(),
                    title: frame.title.clone(),
                    legend: frame.has_legend,
                })
            })?;
        }
        self.pending_charts.borrow_mut()[i].part = Some(part);
        Ok(AutoValue::Empty)
    }
    /// Whether the first series-name row (or column) and the category column (or row) of `area`
    /// are text, as Excel reads a source: a text corner row/column is headers/categories.
    fn source_layout(session: &HostSession, area: GridArea, rows: bool) -> (bool, bool) {
        let model = session.host().model();
        let is_text = |major: u32, minor: u32| {
            let at = CellKey::new(
                area.sheet(),
                area.first().row() + if rows { minor } else { major },
                area.first().column() + if rows { major } else { minor },
            );
            matches!(
                at.ok().and_then(|at| model.cell(at)),
                Some(crate::CellInput::Text(_))
            )
        };
        let (length, width) = if rows {
            (area.column_count(), area.row_count())
        } else {
            (area.row_count(), area.column_count())
        };
        let categories = width > 1 && (1..length).all(|m| is_text(m, 0));
        let first = u32::from(categories);
        let headers = length > 1 && (first..width).all(|n| is_text(0, n));
        (headers, categories)
    }

    // --- shapes ---------------------------------------------------------------------------------

    fn shape_id_of(target: &ObjectRef) -> Result<u32, AutoError> {
        match &target.key {
            ObjectKey::Report {
                item:
                    ReportItem::Shape(id)
                    | ReportItem::TextFrame2(id)
                    | ReportItem::TextRange2(id)
                    | ReportItem::FillFormat(id)
                    | ReportItem::LineFormat(id)
                    | ReportItem::ConnectorFormat(id)
                    | ReportItem::ColorFormat(id, _),
                ..
            } => Ok(*id),
            _ => Err(geometry::type_mismatch()),
        }
    }
    fn shape_of(session: &HostSession, sheet: SheetId, id: u32) -> Result<Shape, AutoError> {
        session
            .host()
            .shapes(sheet)
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or_else(|| AutoError::excel(424, "Object required"))
    }
    fn shape_command(&self, target: &ObjectRef, call: &Call<'_>, change: ShapeChange) -> Reply {
        let (_, sheet, _) = Self::report_key(target)?;
        self.command(call, |s| {
            Ok(WorkbookCommand::Shape {
                sheet,
                revision: s.host().revision().get(),
                change: change.clone(),
            })
        })
    }
    /// Excel's Shapes collection holds charts too, so Count counts them; items that are chart
    /// frames refuse (dc-piw0.2).
    pub(super) fn shapes_count(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.read_host(target, |s, sheet, _| {
            Ok(AutoValue::I4(
                (s.host().shapes(sheet).len() + s.host().model().definitions().charts(sheet).len())
                    as i32,
            ))
        })
    }
    pub(super) fn shapes_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        let (workbook, _, _) = Self::report_key(target)?;
        self.read_host(target, |s, sheet, _| {
            let shapes = s.host().shapes(sheet);
            let shape = match call.arg(0) {
                AutoValue::Text(name) => shapes
                    .iter()
                    .find(|x| x.name.eq_ignore_ascii_case(&name.to_string_lossy())),
                other => usize::try_from(geometry::integer(other)? - 1)
                    .ok()
                    .and_then(|n| shapes.get(n)),
            }
            .ok_or_else(|| {
                AutoError::excel(
                    -2147024809,
                    "The item with the specified name wasn't found.",
                )
            })?;
            Ok(report(
                workbook,
                sheet,
                "Shape",
                ReportItem::Shape(shape.id),
            ))
        })
    }
    fn add_shape(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        kind: ShapeKind,
        geometry_name: &str,
        rect: [f64; 4],
        flips: (bool, bool),
    ) -> Reply {
        let (workbook, sheet, _) = Self::report_key(target)?;
        let mut id = None;
        let [left, top, width, height] = rect;
        self.command(call, |s| {
            id = Some(
                s.host()
                    .shapes(sheet)
                    .iter()
                    .map(|x| x.id)
                    .max()
                    .unwrap_or(1)
                    .max(1 + s.host().model().definitions().charts(sheet).len() as u32)
                    + 1,
            );
            Ok(WorkbookCommand::Shape {
                sheet,
                revision: s.host().revision().get(),
                change: ShapeChange::Add {
                    kind,
                    geometry: geometry_name.into(),
                    left: Points::new(left),
                    top: Points::new(top),
                    width: Points::new(width),
                    height: Points::new(height),
                    flips,
                    name: None,
                },
            })
        })?;
        let id = id.ok_or_else(busy)?;
        Ok(report(workbook, sheet, "Shape", ReportItem::Shape(id)))
    }
    fn rect_args(call: &Call<'_>, first: usize) -> Result<[f64; 4], AutoError> {
        if call.args.len() != first + 4 {
            return Err(geometry::bad_count());
        }
        let mut rect = [0.0; 4];
        for (i, slot) in rect.iter_mut().enumerate() {
            *slot = number(call.arg(first + i))?;
        }
        Ok(rect)
    }
    /// `Shapes.AddShape(Type, Left, Top, Width, Height)`: rectangle, rounded rectangle and oval
    /// (the presets whose connection sites are modelled); other `MsoAutoShapeType`s refuse.
    pub(super) fn shapes_add_shape(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let rect = Self::rect_args(call, 1)?;
        let geometry_name = match geometry::integer(call.arg(0))? {
            1 => "rect",
            5 => "roundRect",
            9 => "ellipse",
            other => {
                return Err(AutoError::not_modelled(
                    format!("Shapes.AddShape MsoAutoShapeType {other}"),
                    SHAPES,
                ));
            }
        };
        self.add_shape(
            target,
            call,
            ShapeKind::Shape,
            geometry_name,
            rect,
            (false, false),
        )
    }
    /// `Shapes.AddTextbox(Orientation, Left, Top, Width, Height)`: horizontal text boxes.
    pub(super) fn shapes_add_textbox(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let rect = Self::rect_args(call, 1)?;
        if geometry::integer(call.arg(0))? != 1 {
            return Err(AutoError::not_modelled(
                "Shapes.AddTextbox vertical orientation",
                SHAPES,
            ));
        }
        self.add_shape(
            target,
            call,
            ShapeKind::TextBox,
            "rect",
            rect,
            (false, false),
        )
    }
    /// A line from (x1, y1) to (x2, y2): its rectangle is the corners', flipped as drawn.
    fn line_rect(a: [f64; 4]) -> ([f64; 4], (bool, bool)) {
        let [x1, y1, x2, y2] = a;
        (
            [x1.min(x2), y1.min(y2), (x2 - x1).abs(), (y2 - y1).abs()],
            (x1 > x2, y1 > y2),
        )
    }
    /// `Shapes.AddLine(BeginX, BeginY, EndX, EndY)`.
    pub(super) fn shapes_add_line(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let (rect, flips) = Self::line_rect(Self::rect_args(call, 0)?);
        self.add_shape(target, call, ShapeKind::Connector, "line", rect, flips)
    }
    /// `Shapes.AddConnector(Type, BeginX, BeginY, EndX, EndY)`: straight connectors; elbow and
    /// curved ones need their adjust handles (dc-zjsg.5).
    pub(super) fn shapes_add_connector(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let (rect, flips) = Self::line_rect(Self::rect_args(call, 1)?);
        if geometry::integer(call.arg(0))? != 1 {
            return Err(AutoError::not_modelled(
                "Shapes.AddConnector elbow and curved connectors",
                "dc-zjsg.5",
            ));
        }
        self.add_shape(target, call, ShapeKind::Connector, "line", rect, flips)
    }
    pub(super) fn shape_name(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let id = Self::shape_id_of(target)?;
        self.read_host(target, |s, sheet, _| {
            Ok(AutoValue::text(&Self::shape_of(s, sheet, id)?.name))
        })
    }
    pub(super) fn shape_put_name(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let id = Self::shape_id_of(target)?;
        let name = text_arg(call.args.last().ok_or_else(geometry::bad_count)?)?;
        self.shape_command(target, call, ShapeChange::Name { id, name })
    }
    pub(super) fn shape_id(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let id = Self::shape_id_of(target)?;
        self.read_host(target, |s, sheet, _| {
            Ok(AutoValue::I4(Self::shape_of(s, sheet, id)?.id as i32))
        })
    }
    pub(super) fn shape_delete(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let id = Self::shape_id_of(target)?;
        self.shape_command(target, call, ShapeChange::Delete { id })
    }
    fn shape_side(&self, target: &ObjectRef, call: &Call<'_>, side: Side) -> Reply {
        no_args(call)?;
        let id = Self::shape_id_of(target)?;
        self.read_host(target, |s, sheet, _| {
            let shape = Self::shape_of(s, sheet, id)?;
            let (l, t, w, h) = s.host().shape_rectangle(sheet, &shape);
            Ok(single([l, t, w, h][side as usize]))
        })
    }
    fn shape_put_side(&self, target: &ObjectRef, call: &Call<'_>, side: Side) -> Reply {
        let id = Self::shape_id_of(target)?;
        let value = number(call.args.last().ok_or_else(geometry::bad_count)?)?;
        let [left, top, width, height] = rectangle_with(side, value);
        self.shape_command(
            target,
            call,
            ShapeChange::Place {
                id,
                left,
                top,
                width,
                height,
            },
        )
    }
    pub(super) fn shape_left(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_side(t, c, Side::Left)
    }
    pub(super) fn shape_top(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_side(t, c, Side::Top)
    }
    pub(super) fn shape_width(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_side(t, c, Side::Width)
    }
    pub(super) fn shape_height(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_side(t, c, Side::Height)
    }
    pub(super) fn shape_put_left(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_put_side(t, c, Side::Left)
    }
    pub(super) fn shape_put_top(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_put_side(t, c, Side::Top)
    }
    pub(super) fn shape_put_width(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_put_side(t, c, Side::Width)
    }
    pub(super) fn shape_put_height(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_put_side(t, c, Side::Height)
    }
    fn shape_child(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        interface: &str,
        make: fn(u32) -> ReportItem,
    ) -> Reply {
        no_args(call)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        let id = Self::shape_id_of(target)?;
        self.read_host(target, |s, sheet, _| {
            Self::shape_of(s, sheet, id).map(|_| ())
        })?;
        Ok(report(workbook, sheet, interface, make(id)))
    }
    pub(super) fn shape_text_frame(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_child(t, c, "TextFrame2", ReportItem::TextFrame2)
    }
    pub(super) fn shape_fill(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_child(t, c, "FillFormat", ReportItem::FillFormat)
    }
    pub(super) fn shape_line(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_child(t, c, "LineFormat", ReportItem::LineFormat)
    }
    pub(super) fn shape_connector_format(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_child(t, c, "ConnectorFormat", ReportItem::ConnectorFormat)
    }
    pub(super) fn frame_text_range(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_child(t, c, "TextRange2", ReportItem::TextRange2)
    }
    /// The shape's text as Excel gives it: paragraphs joined by a carriage return.
    pub(super) fn range_text(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let id = Self::shape_id_of(target)?;
        self.read_host(target, |s, sheet, _| {
            let shape = Self::shape_of(s, sheet, id)?;
            Ok(AutoValue::text(&shape.text.unwrap_or_default().join("\r")))
        })
    }
    pub(super) fn range_put_text(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let id = Self::shape_id_of(target)?;
        let text = text_arg(call.args.last().ok_or_else(geometry::bad_count)?)?;
        self.shape_command(target, call, ShapeChange::Text { id, text })
    }
    fn colour_child(&self, target: &ObjectRef, call: &Call<'_>, line: bool) -> Reply {
        no_args(call)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        let id = Self::shape_id_of(target)?;
        Ok(report(
            workbook,
            sheet,
            "ColorFormat",
            ReportItem::ColorFormat(id, line),
        ))
    }
    pub(super) fn fill_colour(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.colour_child(t, c, false)
    }
    pub(super) fn line_colour(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.colour_child(t, c, true)
    }
    pub(super) fn colour_rgb(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let ObjectKey::Report {
            item: ReportItem::ColorFormat(id, line),
            ..
        } = &target.key
        else {
            return Err(geometry::type_mismatch());
        };
        let (id, line) = (*id, *line);
        self.read_host(target, |s, sheet, _| {
            let shape = Self::shape_of(s, sheet, id)?;
            let colour = if line {
                shape.line.colour.clone()
            } else {
                match shape.fill {
                    ShapeFill::Solid(c) => Some(c),
                    _ => None,
                }
            };
            match colour {
                Some(ShapeColour::Rgb(v)) => Ok(AutoValue::I4(colour_to_rgb(v) as i32)),
                // A colour the style or a theme decides is not measured (Q8).
                _ => Err(AutoError::not_modelled(
                    "ColorFormat.RGB of a style or theme colour",
                    SHAPES,
                )),
            }
        })
    }
    pub(super) fn colour_put_rgb(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let ObjectKey::Report {
            item: ReportItem::ColorFormat(id, line),
            ..
        } = &target.key
        else {
            return Err(geometry::type_mismatch());
        };
        let (id, line) = (*id, *line);
        let colour = rgb_to_colour(geometry::integer(
            call.args.last().ok_or_else(geometry::bad_count)?,
        )?)?;
        let (_, sheet, _) = Self::report_key(target)?;
        self.command(call, |s| {
            let shape = Self::shape_of(s, sheet, id)?;
            let change = if line {
                let mut next: ShapeLine = shape.line.clone();
                next.colour = Some(colour.clone());
                next.hidden = false;
                ShapeChange::Line { id, line: next }
            } else {
                ShapeChange::Fill {
                    id,
                    fill: ShapeFill::Solid(colour.clone()),
                }
            };
            Ok(WorkbookCommand::Shape {
                sheet,
                revision: s.host().revision().get(),
                change,
            })
        })
    }
    pub(super) fn line_weight(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let id = Self::shape_id_of(target)?;
        self.read_host(target, |s, sheet, _| {
            match Self::shape_of(s, sheet, id)?.line.weight {
                Some(w) => Ok(single(w as f64 / EMU_PER_POINT)),
                None => Err(AutoError::not_modelled(
                    "LineFormat.Weight of a style-driven line",
                    SHAPES,
                )),
            }
        })
    }
    pub(super) fn line_put_weight(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let id = Self::shape_id_of(target)?;
        let weight = number(call.args.last().ok_or_else(geometry::bad_count)?)?;
        if !(0.0..=1584.0).contains(&weight) {
            return Err(AutoError::excel(1004, ""));
        }
        let (_, sheet, _) = Self::report_key(target)?;
        self.command(call, |s| {
            let mut line = Self::shape_of(s, sheet, id)?.line;
            line.weight = Some((weight * EMU_PER_POINT).round() as i64);
            Ok(WorkbookCommand::Shape {
                sheet,
                revision: s.host().revision().get(),
                change: ShapeChange::Line { id, line },
            })
        })
    }
    fn connect(&self, target: &ObjectRef, call: &Call<'_>, begin: bool) -> Reply {
        if call.args.len() != 2 {
            return Err(geometry::bad_count());
        }
        let id = Self::shape_id_of(target)?;
        let AutoValue::Object(ObjectRef {
            key:
                ObjectKey::Report {
                    item: ReportItem::Shape(shape),
                    ..
                },
            ..
        }) = call.arg(0)
        else {
            return Err(geometry::type_mismatch());
        };
        let site = u32::try_from(geometry::integer(call.arg(1))?)
            .map_err(|_| geometry::type_mismatch())?;
        self.shape_command(
            target,
            call,
            ShapeChange::Connect {
                id,
                begin,
                shape: *shape,
                site,
            },
        )
    }
    pub(super) fn connector_begin(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.connect(t, c, true)
    }
    pub(super) fn connector_end(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.connect(t, c, false)
    }
}

/// A chart a member names: made, by its part, or waiting for source data (an index into the
/// pending frames).
enum ChartRef {
    Real(String),
    Pending(usize),
}

/// `XlChartType` to the chart commands' kind (and whether stacked). Kinds the chart writer does not
/// save refuse naming dc-zjsg.6.
fn chart_kind(xl_type: i64) -> Result<(&'static str, bool), AutoError> {
    Ok(match xl_type {
        51 => ("column", false),
        52 => ("column", true),
        57 => ("bar", false),
        58 => ("bar", true),
        4 => ("line", false),
        1 => ("area", false),
        76 => ("area", true),
        5 => ("pie", false),
        -4120 => ("doughnut", false),
        -4169 => ("scatter", false),
        other => {
            return Err(AutoError::not_modelled(
                format!("XlChartType {other}"),
                "dc-zjsg.6",
            ));
        }
    })
}
