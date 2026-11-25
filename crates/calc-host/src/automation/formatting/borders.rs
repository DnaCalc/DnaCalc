//! `Range.Borders`, `Border` and `Range.BorderAround`, written through Format Cells' Border tab one edge at a time.
use super::interior::{color_bgr, color_index};
use super::mutation::PreWrite;
use super::*;
use crate::{EdgeChange, EdgeLine, EdgeWeight, EdgesPatch, FillColor, decompose_edge};
use calc_doc_model::{Border, BorderEdge};
use calc_value::CellKey;

const AUTOMATIC_COLOR: i32 = -4105;
const NO_LINE: i32 = -4142;
// XlBordersIndex.
pub(super) const DIAGONAL_DOWN: i32 = 5;
pub(super) const DIAGONAL_UP: i32 = 6;
pub(super) const LEFT: i32 = 7;
pub(super) const TOP: i32 = 8;
pub(super) const BOTTOM: i32 = 9;
pub(super) const RIGHT: i32 = 10;
const INSIDE_VERTICAL: i32 = 11;
const INSIDE_HORIZONTAL: i32 = 12;
pub(super) const LINES: &[(i32, EdgeLine)] = &[
    (NO_LINE, EdgeLine::None),
    (1, EdgeLine::Continuous),
    (-4115, EdgeLine::Dash),
    (4, EdgeLine::DashDot),
    (5, EdgeLine::DashDotDot),
    (-4118, EdgeLine::Dot),
    (-4119, EdgeLine::Double),
    (13, EdgeLine::SlantDashDot),
];
pub(super) const WEIGHTS: &[(i32, EdgeWeight)] = &[
    (1, EdgeWeight::Hairline),
    (2, EdgeWeight::Thin),
    (-4138, EdgeWeight::Medium),
    (4, EdgeWeight::Thick),
];

/// One edge of an area: the cells carrying it and which side of their format changes.
struct Strip {
    area: GridArea,
    edge: i32,
    primary: bool,
}

fn span(area: GridArea, rows: (u32, u32), cols: (u32, u32)) -> GridArea {
    let key = |row, column| CellKey::new(area.sheet(), row, column).expect("strip inside the grid");
    GridArea::new(key(rows.0, cols.0), key(rows.1, cols.1)).expect("one sheet")
}

/// The strips Excel changes for one border index of one area, including the neighbours' matching edges.
fn strips(area: GridArea, edge: i32) -> Result<Vec<Strip>, AutoError> {
    let (r1, c1, r2, c2) = (area.first().row(), area.first().column(), area.last().row(), area.last().column());
    let mut out = Vec::new();
    let mut add = |area, edge, primary| out.push(Strip { area, edge, primary });
    match edge {
        LEFT => {
            add(span(area, (r1, r2), (c1, c1)), LEFT, true);
            if c1 > 1 {
                add(span(area, (r1, r2), (c1 - 1, c1 - 1)), RIGHT, false);
            }
        }
        RIGHT => {
            add(span(area, (r1, r2), (c2, c2)), RIGHT, true);
            if c2 < calc_value::MAX_COLUMNS {
                add(span(area, (r1, r2), (c2 + 1, c2 + 1)), LEFT, false);
            }
        }
        TOP => {
            add(span(area, (r1, r1), (c1, c2)), TOP, true);
            if r1 > 1 {
                add(span(area, (r1 - 1, r1 - 1), (c1, c2)), BOTTOM, false);
            }
        }
        BOTTOM => {
            add(span(area, (r2, r2), (c1, c2)), BOTTOM, true);
            if r2 < calc_value::MAX_ROWS {
                add(span(area, (r2 + 1, r2 + 1), (c1, c2)), TOP, false);
            }
        }
        INSIDE_VERTICAL => {
            if c1 == c2 {
                return Err(format_error());
            }
            add(span(area, (r1, r2), (c1, c2 - 1)), RIGHT, true);
            add(span(area, (r1, r2), (c1 + 1, c2)), LEFT, true);
        }
        INSIDE_HORIZONTAL => {
            if r1 == r2 {
                return Err(format_error());
            }
            add(span(area, (r1, r2 - 1), (c1, c2)), BOTTOM, true);
            add(span(area, (r1 + 1, r2), (c1, c2)), TOP, true);
        }
        DIAGONAL_DOWN | DIAGONAL_UP => add(area, edge, true),
        _ => return Err(format_error()),
    }
    Ok(out)
}

fn edge_list(edge: i32) -> Vec<i32> {
    if edge == 0 {
        vec![LEFT, TOP, BOTTOM, RIGHT, INSIDE_VERTICAL, INSIDE_HORIZONTAL]
    } else {
        vec![edge]
    }
}

fn patch_for(edge: i32, change: &EdgeChange) -> FormatPatch {
    let mut edges = EdgesPatch::default();
    let slot = match edge {
        LEFT => &mut edges.left,
        RIGHT => &mut edges.right,
        TOP => &mut edges.top,
        BOTTOM => &mut edges.bottom,
        DIAGONAL_UP => &mut edges.diagonal_up,
        _ => &mut edges.diagonal_down,
    };
    *slot = Some(change.clone());
    FormatPatch {
        edges: Some(edges),
        ..Default::default()
    }
}

fn cell_border(host: &HostState, style: u32) -> Result<Option<&Border>, AutoError> {
    let styles = host.styles();
    let format = styles.cell_formats.get(style as usize);
    if format.is_none() && style != 0 {
        return Err(AutoError::not_modelled(format!("Cell format {style} is unavailable"), OWNER));
    }
    let id = format.and_then(|xf| xf.border_id.or_else(|| styles.cell_style_formats.get(xf.xf_id? as usize)?.border_id));
    match id {
        Some(id) => styles
            .borders
            .get(id as usize)
            .map(Some)
            .ok_or_else(|| AutoError::not_modelled(format!("Border record {id} is unavailable"), OWNER)),
        None => Ok(styles.borders.first()),
    }
}

pub(super) fn side(border: Option<&Border>, edge: i32) -> Option<&BorderEdge> {
    let border = border?;
    match edge {
        LEFT => border.left.as_ref(),
        RIGHT => border.right.as_ref(),
        TOP => border.top.as_ref(),
        BOTTOM => border.bottom.as_ref(),
        DIAGONAL_UP => border.diagonal.as_ref().filter(|_| border.diagonal_up),
        _ => border.diagonal.as_ref().filter(|_| border.diagonal_down),
    }
}

impl WorkbookAutomation {
    pub(super) fn register_range_borders(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "Borders", InvokeKind::Get, Self::range_borders);
        handlers.on("Borders", "Item", InvokeKind::Get, Self::borders_item);
        handlers.on("Borders", "_Default", InvokeKind::Get, Self::borders_item);
        for interface in ["Borders", "Border"] {
            for member in ["LineStyle", "Weight", "Color", "ColorIndex"] {
                handlers.on(interface, member, InvokeKind::Get, Self::get_border_property);
                handlers.on(interface, member, InvokeKind::Put, Self::put_border_property);
            }
        }
        handlers.on("Range", "BorderAround", InvokeKind::Method, Self::border_around);
    }

    fn range_borders(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        self.edge_object(target, "Borders", 0)
    }

    fn borders_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if super::conditions::is_condition_part(target) {
            return self.condition_borders_item(target, call);
        }
        format_arity(call, 1)?;
        let edge = integral_format_arg(call)?;
        if !(DIAGONAL_DOWN..=INSIDE_HORIZONTAL).contains(&edge) {
            return Err(format_error());
        }
        self.edge_object(target, "Border", edge)
    }

    fn edge_object(&self, target: &ObjectRef, class: &'static str, edge: i32) -> Reply {
        let (ObjectKey::RangeEdge { .. } | ObjectKey::Range(_) | ObjectKey::RangeFormat(_)) = target.key else {
            return Err(geometry::type_mismatch());
        };
        self.with_range(target, |access, _, range, _| {
            let reference = self.retain_range(access, range)?;
            Ok(AutoValue::Object(object(class, ObjectKey::RangeEdge { reference, edge })))
        })
    }

    fn get_border_property(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if super::conditions::is_condition_part(target) {
            return self.condition_border_get(target, call);
        }
        if matches!(target.key, ObjectKey::Report { item: calc_object_model::ReportItem::ChartBorder(..) | calc_object_model::ReportItem::SeriesBorder(..), .. }) {
            return self.chart_border_get(target, call);
        }
        format_arity(call, 0)?;
        let ObjectKey::RangeEdge { edge: index, .. } = target.key else {
            return Err(geometry::type_mismatch());
        };
        let member = call.member.name.as_str();
        self.with_range(target, |access, _, range, _| {
            let Access::Session(session, _) = access else {
                return Err(AutoError::not_modelled(format!("{} inside a worksheet invocation", call.label()), OWNER));
            };
            let host = session.host();
            let mut uniform: Option<i32> = None;
            for edge in edge_list(index) {
                for area in range.areas() {
                    let strips = match strips(*area, edge) {
                        Ok(strips) => strips,
                        Err(_) if index == 0 => continue,
                        Err(error) => return Err(error),
                    };
                    for strip in strips.iter().filter(|s| s.primary) {
                        let samples = format_samples(host, &[strip.area]).map_err(|r| AutoError::not_modelled(r, OWNER))?;
                        for at in samples {
                            let border = cell_border(host, host.cell_style(at))?;
                            let value = edge_property(host, side(border, strip.edge), member)?;
                            if uniform.is_some_and(|prior| prior != value) {
                                return Ok(AutoValue::Null);
                            }
                            uniform = Some(value);
                        }
                    }
                }
            }
            Ok(AutoValue::I4(uniform.ok_or_else(format_error)?))
        })
    }

    fn put_border_property(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if super::conditions::is_condition_part(target) {
            return self.condition_border_put(target, call);
        }
        if matches!(target.key, ObjectKey::Report { item: calc_object_model::ReportItem::ChartBorder(..) | calc_object_model::ReportItem::SeriesBorder(..), .. }) {
            return self.chart_border_put(target, call);
        }
        format_arity(call, 1)?;
        let ObjectKey::RangeEdge { edge: index, .. } = target.key else {
            return Err(geometry::type_mismatch());
        };
        let mut change = EdgeChange::default();
        match call.member.name.as_str() {
            "LineStyle" => change.line = Some(pick(LINES, integral_format_arg(call)?)?),
            "Weight" => change.weight = Some(pick(WEIGHTS, integral_format_arg(call)?)?),
            "Color" => change.color = Some(rgb_color(integral_format_arg(call)?)?),
            _ => change.color = Some(indexed_color(integral_format_arg(call)?)?),
        }
        self.write_edges(target, call, move |area| {
            let mut out = Vec::new();
            for edge in edge_list(index) {
                match strips(area, edge) {
                    Ok(list) => out.extend(list.into_iter().map(|s| (s.area, patch_for(s.edge, &change)))),
                    Err(_) if index == 0 => {}
                    Err(error) => return Err(error),
                }
            }
            Ok(out)
        })
    }

    fn border_around(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() > 5 {
            return Err(geometry::bad_count());
        }
        let given = |i: usize| !matches!(call.arg(i), AutoValue::Missing | AutoValue::Empty);
        let number = |i: usize| -> Result<i32, AutoError> {
            let value = super::super::report_draw::number(call.arg(i)).map_err(|_| format_error())?;
            if value.fract() != 0.0 || !value.is_finite() || value.abs() > f64::from(i32::MAX) {
                return Err(format_error());
            }
            Ok(value as i32)
        };
        if given(4) {
            return Err(AutoError::not_modelled("Range.BorderAround ThemeColor", OWNER));
        }
        let line = if given(0) { pick(LINES, number(0)?)? } else { EdgeLine::Continuous };
        let weight = if given(1) { pick(WEIGHTS, number(1)?)? } else { EdgeWeight::Thin };
        let color = if given(3) {
            rgb_color(number(3)?)?
        } else if given(2) {
            indexed_color(number(2)?)?
        } else {
            FillColor::Automatic
        };
        let change = EdgeChange {
            line: Some(line),
            weight: (line != EdgeLine::None).then_some(weight),
            color: (line != EdgeLine::None).then_some(color),
        };
        self.write_edges(target, call, move |area| {
            let mut out = Vec::new();
            for edge in [LEFT, TOP, BOTTOM, RIGHT] {
                out.extend(strips(area, edge)?.into_iter().map(|s| (s.area, patch_for(s.edge, &change))));
            }
            Ok(out)
        })
    }

    fn write_edges(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        plan: impl Fn(GridArea) -> Result<Vec<(GridArea, FormatPatch)>, AutoError>,
    ) -> Reply {
        let neither = |_: &HostSession, _: Option<ReferenceLike>| -> Result<SkinIntent, AutoError> {
            Err(AutoError::not_modelled(format!("{} without a range", call.label()), OWNER))
        };
        self.mutate_pre(
            target,
            call,
            OWNER,
            |session, reference| {
                let range = reference.ok_or_else(geometry::type_mismatch)?;
                self.guard_locked(session, range)?;
                let mut stages = Vec::new();
                for area in range.areas() {
                    stages.extend(plan(*area)?);
                }
                for (area, patch) in stages {
                    patch.validate().map_err(|_| format_error())?;
                    let receipt = session.dispatch_from(
                        crate::CommandIngress::Automation,
                        SkinIntent::Command(WorkbookCommand::FormatCells { areas: vec![area], patch }),
                    );
                    if let Some(error) = receipt.error {
                        return Err(AutoError::not_modelled(format!("{}: {error}", call.label()), OWNER));
                    }
                }
                Ok(PreWrite::Done)
            },
            neither,
        )
    }
}

pub(super) fn pick<T: Copy>(table: &[(i32, T)], value: i32) -> Result<T, AutoError> {
    table.iter().find(|(v, _)| *v == value).map(|(_, t)| *t).ok_or_else(format_error)
}

pub(super) fn rgb_color(rgb: i32) -> Result<FillColor, AutoError> {
    if !(0..=0x00FF_FFFF).contains(&rgb) {
        return Err(format_error());
    }
    let rgb = rgb as u32;
    Ok(FillColor::Rgb(format!("{:06X}", (rgb & 0xFF) << 16 | rgb & 0xFF00 | rgb >> 16 & 0xFF)))
}

pub(super) fn indexed_color(index: i32) -> Result<FillColor, AutoError> {
    match index {
        AUTOMATIC_COLOR => Ok(FillColor::Automatic),
        1..=56 => Ok(FillColor::Indexed(index as u8 + 7)),
        _ => Err(format_error()),
    }
}

pub(super) fn edge_property(host: &HostState, edge: Option<&BorderEdge>, member: &str) -> Result<i32, AutoError> {
    let style = edge.and_then(|e| e.style.as_deref());
    let (line, weight) = decompose_edge(style)
        .ok_or_else(|| AutoError::not_modelled(format!("Border style stored as {}", style.unwrap_or("")), OWNER))?;
    let color = edge.and_then(|e| e.color.as_ref());
    match member {
        "LineStyle" => Ok(LINES.iter().find(|(_, l)| *l == line).map(|(v, _)| *v).expect("every line has a code")),
        // A border without a line reports thin, the weight a new line gets.
        "Weight" => Ok(WEIGHTS
            .iter()
            .find(|(_, w)| Some(*w) == weight.or(Some(EdgeWeight::Thin)))
            .map(|(v, _)| *v)
            .expect("every weight has a code")),
        "Color" => color_bgr(host, color, 0),
        _ => color_index(host, color),
    }
}
