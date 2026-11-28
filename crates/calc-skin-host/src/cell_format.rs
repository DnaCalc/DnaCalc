//! Format Cells over CalcHost's style table: what the dialog reads and the edits it commits.

use crate::{State, grid_area, invalid, unavailable};
use calc_doc_model::{Border, BorderEdge, Fill, Font, StyleColor};
use calc_host::value::{CellKey, GridArea};
use calc_host::{
    CellEdit, EdgeChange, EdgeLine, FillColor, FormatPatch, HostState, InteriorPatch, RangeEdges,
    WorkbookCommand, decompose_edge,
};
use calc_skin::grid::{Area, Key};
use calc_skin::*;
use std::collections::BTreeSet;

const OWNER: &str = "dc-s2xup";

fn excel(reason: String) -> Refusal {
    Refusal::Excel { reason }
}

fn horizontal(value: HorizontalAlignment) -> &'static str {
    match value {
        HorizontalAlignment::General => "general",
        HorizontalAlignment::Left => "left",
        HorizontalAlignment::Center => "center",
        HorizontalAlignment::Right => "right",
        HorizontalAlignment::Fill => "fill",
        HorizontalAlignment::Justify => "justify",
        HorizontalAlignment::CenterAcrossSelection => "centerContinuous",
        HorizontalAlignment::Distributed => "distributed",
    }
}

const HORIZONTAL: [HorizontalAlignment; 8] = [
    HorizontalAlignment::General,
    HorizontalAlignment::Left,
    HorizontalAlignment::Center,
    HorizontalAlignment::Right,
    HorizontalAlignment::Fill,
    HorizontalAlignment::Justify,
    HorizontalAlignment::CenterAcrossSelection,
    HorizontalAlignment::Distributed,
];

fn vertical(value: VerticalAlignment) -> &'static str {
    match value {
        VerticalAlignment::Top => "top",
        VerticalAlignment::Center => "center",
        VerticalAlignment::Bottom => "bottom",
        VerticalAlignment::Justify => "justify",
        VerticalAlignment::Distributed => "distributed",
    }
}

const VERTICAL: [VerticalAlignment; 5] = [
    VerticalAlignment::Top,
    VerticalAlignment::Center,
    VerticalAlignment::Bottom,
    VerticalAlignment::Justify,
    VerticalAlignment::Distributed,
];

fn underline(value: UnderlineStyle) -> &'static str {
    match value {
        UnderlineStyle::None => "none",
        UnderlineStyle::Single => "single",
        UnderlineStyle::Double => "double",
        UnderlineStyle::SingleAccounting => "singleAccounting",
        UnderlineStyle::DoubleAccounting => "doubleAccounting",
    }
}

const UNDERLINE: [UnderlineStyle; 5] = [
    UnderlineStyle::None,
    UnderlineStyle::Single,
    UnderlineStyle::Double,
    UnderlineStyle::SingleAccounting,
    UnderlineStyle::DoubleAccounting,
];

fn hex(rgb: [u8; 3]) -> String {
    format!("{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

fn fill_color(color: FormatColor) -> FillColor {
    match color {
        FormatColor::Automatic => FillColor::Automatic,
        FormatColor::Rgb(rgb) => FillColor::Rgb(hex(rgb)),
    }
}

fn edge(stroke: Option<EdgeStroke>) -> Result<Option<EdgeChange>, Refusal> {
    let Some(stroke) = stroke else { return Ok(None) };
    if stroke.line == BorderLine::None {
        return Ok(Some(EdgeChange {
            line: Some(EdgeLine::None),
            ..Default::default()
        }));
    }
    let (line, weight) = decompose_edge(Some(stroke.line.name()))
        .ok_or_else(|| invalid("Choose a border line style"))?;
    Ok(Some(EdgeChange {
        line: Some(line),
        weight,
        color: Some(fill_color(stroke.color)),
    }))
}

/// The cells whose formats a border change may rewrite: the area and one cell around it.
fn reach(area: GridArea) -> (u32, u32, u32, u32) {
    let (first, last) = (area.first(), area.last());
    (
        first.row().saturating_sub(1),
        first.column().saturating_sub(1),
        last.row().saturating_add(1),
        last.column().saturating_add(1),
    )
}

/// The edits of one Format Cells commit, and the areas whose cells or formats they touch.
pub(crate) fn prepare(
    state: &State,
    selected: &[Area],
    format: &CellFormatting,
) -> Result<(Vec<CellEdit>, Vec<GridArea>), Refusal> {
    let session = &state.session;
    let host = session.host();
    let areas = selected.iter().map(|area| grid_area(*area)).collect::<Result<Vec<_>, _>>()?;
    let Some(first) = areas.first() else {
        return Err(invalid("Select cells to format"));
    };
    if areas.iter().any(|area| area.sheet() != first.sheet()) {
        return Err(invalid("Format cells on one worksheet at a time"));
    }
    if format.justify_distributed.is_some() {
        return Err(unavailable(OWNER, "Format Cells: Justify distributed"));
    }
    if format.direction.is_some() {
        return Err(unavailable(OWNER, "Format Cells: Text direction"));
    }
    let interior = (format.fill_color.is_some() || format.pattern.is_some() || format.pattern_color.is_some())
        .then(|| InteriorPatch {
            // No Color with no pattern chosen removes the fill, as the dialog's No Color does.
            pattern: match (format.pattern, format.fill_color) {
                (Some(pattern), _) => Some(pattern.name().into()),
                (None, Some(FormatColor::Automatic)) => Some("none".into()),
                (None, _) => None,
            },
            color: match (format.pattern, format.fill_color) {
                (None, Some(FormatColor::Automatic)) => None,
                (_, color) => color.map(fill_color),
            },
            pattern_color: format.pattern_color.map(fill_color),
        });
    let patch = FormatPatch {
        number_format: format.number_format.clone(),
        horizontal: format.horizontal.map(|value| horizontal(value).into()),
        vertical: format.vertical.map(|value| vertical(value).into()),
        wrap: format.wrap,
        shrink: format.shrink,
        rotation: format.orientation.map(|orientation| match orientation {
            TextOrientation::Degrees(degrees) => degrees,
            TextOrientation::Stacked => 255,
        }),
        indent: format.indent,
        font_name: format.font_name.clone(),
        font_size: format.font_size.clone(),
        bold: format.bold,
        italic: format.italic,
        underline_style: format.underline.map(|value| underline(value).into()),
        strike: format.strikethrough,
        superscript: format.superscript,
        subscript: format.subscript,
        font_color: format.font_color.map(|color| match color {
            FormatColor::Automatic => String::new(),
            FormatColor::Rgb(rgb) => hex(rgb),
        }),
        interior,
        locked: format.locked,
        hidden: format.hidden,
        ..Default::default()
    };
    patch.validate().map_err(invalid)?;
    let borders = RangeEdges {
        left: edge(format.borders.left)?,
        right: edge(format.borders.right)?,
        top: edge(format.borders.top)?,
        bottom: edge(format.borders.bottom)?,
        inside_horizontal: edge(format.borders.inside_horizontal)?,
        inside_vertical: edge(format.borders.inside_vertical)?,
        diagonal_up: edge(format.borders.diagonal_up)?,
        diagonal_down: edge(format.borders.diagonal_down)?,
    };
    let mut touched = areas.clone();
    if borders != RangeEdges::default() {
        for (index, area) in areas.iter().enumerate() {
            let (top, left, bottom, right) = reach(*area);
            if areas[..index].iter().any(|other| {
                let (first, last) = (other.first(), other.last());
                first.row() <= bottom && top <= last.row() && first.column() <= right && left <= last.column()
            }) {
                return Err(unavailable(OWNER, "Format Cells: borders on selected areas that touch"));
            }
            let affected = calc_host::border_affected_areas(*area, None);
            if host.merged_areas(area.sheet()).iter().any(|merge| {
                affected.iter().any(|part| merge.intersection(*part).is_some())
            }) {
                return Err(unavailable(OWNER, "Format Cells: borders at merged cells"));
            }
            for at in calc_host::format_samples(host, &affected).map_err(excel)? {
                let border = calc_host::border_for_style(host.styles(), host.cell_style(at)).map_err(excel)?;
                if border.is_some_and(|border| !border.other_children.is_empty()) {
                    return Err(unavailable(OWNER, "Format Cells: borders over unmodelled imported border fields"));
                }
            }
            touched.extend(affected.into_iter().skip(1));
        }
    }
    let mut edits = calc_host::prepare_format_cells(host, &areas, &patch, &borders).map_err(excel)?;
    if let Some(merge) = format.merge {
        let command = if merge {
            WorkbookCommand::MergeCells { areas: areas.clone(), across: false }
        } else {
            WorkbookCommand::UnmergeCells { areas: areas.clone() }
        };
        edits.extend(session.prepare_workbook_command(command).map_err(crate::batch::refusal)?);
    }
    Ok((edits, touched))
}

fn color(host: &HostState, color: Option<&StyleColor>) -> Option<FormatColor> {
    let Some(color) = color.filter(|color| !color.auto) else {
        return Some(FormatColor::Automatic);
    };
    let rgb = host.resolved_style_color(color)?;
    let digits = rgb.trim_start_matches('#');
    if !matches!(digits.len(), 6 | 8) {
        return None;
    }
    let [_, red, green, blue] = u32::from_str_radix(digits, 16).ok()?.to_be_bytes();
    Some(FormatColor::Rgb([red, green, blue]))
}

fn stroke(host: &HostState, edge: Option<&BorderEdge>) -> Option<EdgeStroke> {
    let Some(edge) = edge.filter(|edge| edge.style.as_deref().is_some_and(|style| style != "none")) else {
        return Some(EdgeStroke::NONE);
    };
    Some(EdgeStroke {
        line: BorderLine::named(edge.style.as_deref()?)?,
        color: color(host, edge.color.as_ref())?,
    })
}

struct Records<'a> {
    font: Font,
    fill: Option<&'a Fill>,
}

fn records(host: &HostState, style: u32) -> Records<'_> {
    let table = host.styles();
    let format = table.cell_formats.get(style as usize).cloned().unwrap_or_default();
    let parent = format.xf_id.and_then(|id| table.cell_style_formats.get(id as usize));
    let font = format.font_id.or_else(|| parent.and_then(|parent| parent.font_id)).unwrap_or(0);
    let fill = format.fill_id.or_else(|| parent.and_then(|parent| parent.fill_id)).unwrap_or(0);
    Records {
        font: table.fonts.get(font as usize).cloned().unwrap_or_default(),
        fill: table.fills.get(fill as usize),
    }
}

/// The format of one cell, every property stated. Borders are read per area, not here.
fn properties(host: &HostState, at: CellKey) -> CellFormatting {
    let style = host.cell_style(at);
    let Records { font, fill } = records(host, style);
    let alignment = host.cell_alignment(at).cloned().unwrap_or_default();
    let orientation = alignment.text_rotation.as_deref().unwrap_or("0").parse::<i16>().ok().and_then(|value| {
        match value {
            0..=90 => Some(TextOrientation::Degrees(value)),
            91..=180 => Some(TextOrientation::Degrees(90 - value)),
            255 => Some(TextOrientation::Stacked),
            _ => None,
        }
    });
    let table = host.styles();
    let format = table.cell_formats.get(style as usize);
    let parent = format.and_then(|format| format.xf_id).and_then(|id| table.cell_style_formats.get(id as usize));
    let protection = format.and_then(|format| format.protection.as_ref())
        .or_else(|| parent.and_then(|parent| parent.protection.as_ref()));
    let pattern = fill.and_then(|fill| fill.pattern_type.as_deref()).unwrap_or("none");
    let (cell_color, marks) = match (fill, pattern) {
        (None, _) | (_, "none") => (Some(FormatColor::Automatic), Some(FormatColor::Automatic)),
        (Some(fill), "solid") => (color(host, fill.fg_color.as_ref()), Some(FormatColor::Automatic)),
        (Some(fill), _) => (color(host, fill.bg_color.as_ref()), color(host, fill.fg_color.as_ref())),
    };
    let underline_name = font.underline.as_deref().unwrap_or("none");
    CellFormatting {
        number_format: Some(host.number_format(at).unwrap_or("General").into()),
        horizontal: {
            let name = alignment.horizontal.as_deref().unwrap_or("general");
            HORIZONTAL.into_iter().find(|value| horizontal(*value) == name)
        },
        vertical: {
            let name = alignment.vertical.as_deref().unwrap_or("bottom");
            VERTICAL.into_iter().find(|value| vertical(*value) == name)
        },
        indent: alignment.indent.as_deref().unwrap_or("0").parse().ok(),
        // The style model carries neither the distributed-justify flag nor the reading order.
        justify_distributed: None,
        wrap: Some(alignment.wrap_text),
        shrink: Some(alignment.shrink_to_fit),
        merge: None,
        direction: None,
        orientation,
        font_name: Some(font.name.clone().unwrap_or_else(|| "Calibri".into())),
        font_size: Some(font.size.clone().unwrap_or_else(|| "11".into())),
        bold: Some(font.bold),
        italic: Some(font.italic),
        underline: UNDERLINE.into_iter().find(|value| underline(*value) == underline_name),
        font_color: color(host, font.color.as_ref()),
        strikethrough: Some(font.strike),
        superscript: Some(font.vert_align.as_deref() == Some("superscript")),
        subscript: Some(font.vert_align.as_deref() == Some("subscript")),
        borders: RangeBorders::default(),
        fill_color: cell_color,
        pattern: FillPattern::named(pattern),
        pattern_color: marks,
        locked: Some(protection.and_then(|protection| protection.locked).unwrap_or(true)),
        hidden: Some(protection.and_then(|protection| protection.hidden).unwrap_or(false)),
    }
}

/// One border of the selection: the stroke every part shares, or `None` where they differ.
fn shared(
    host: &HostState,
    parts: &[(GridArea, fn(&Border) -> Option<&BorderEdge>)],
    work: &mut impl FnMut() -> Result<(), Refusal>,
) -> Result<Option<EdgeStroke>, Refusal> {
    let mut found: Option<Option<EdgeStroke>> = None;
    for (area, side) in parts {
        let mut styles = BTreeSet::new();
        for run in calc_host::format_regions(host, *area).map_err(excel)? {
            work()?;
            if !styles.insert(run.format) {
                continue;
            }
            let border = calc_host::border_for_style(host.styles(), run.format).map_err(excel)?;
            let next = stroke(host, border.and_then(|border| side(border)));
            match &found {
                None => found = Some(next),
                Some(first) if *first != next => return Ok(None),
                Some(_) => {}
            }
        }
    }
    Ok(found.flatten())
}

fn left(border: &Border) -> Option<&BorderEdge> {
    border.left.as_ref()
}
fn right(border: &Border) -> Option<&BorderEdge> {
    border.right.as_ref()
}
fn top(border: &Border) -> Option<&BorderEdge> {
    border.top.as_ref()
}
fn bottom(border: &Border) -> Option<&BorderEdge> {
    border.bottom.as_ref()
}
fn diagonal_up(border: &Border) -> Option<&BorderEdge> {
    border.diagonal.as_ref().filter(|_| border.diagonal_up)
}
fn diagonal_down(border: &Border) -> Option<&BorderEdge> {
    border.diagonal.as_ref().filter(|_| border.diagonal_down)
}

fn span(area: GridArea, rows: (u32, u32), columns: (u32, u32)) -> GridArea {
    let key = |row, column| CellKey::new(area.sheet(), row, column).expect("inside the selected area");
    GridArea::new(key(rows.0, columns.0), key(rows.1, columns.1)).expect("one worksheet")
}

fn borders(
    host: &HostState,
    areas: &[GridArea],
    mut work: impl FnMut() -> Result<(), Refusal>,
) -> Result<RangeBorders, Refusal> {
    type Side = fn(&Border) -> Option<&BorderEdge>;
    let mut parts: [Vec<(GridArea, Side)>; 8] = Default::default();
    let (mut rows, mut columns) = (false, false);
    for &area in areas {
        let (r1, c1, r2, c2) = (area.first().row(), area.first().column(), area.last().row(), area.last().column());
        parts[0].push((span(area, (r1, r2), (c1, c1)), left));
        parts[1].push((span(area, (r1, r2), (c2, c2)), right));
        parts[2].push((span(area, (r1, r1), (c1, c2)), top));
        parts[3].push((span(area, (r2, r2), (c1, c2)), bottom));
        if r1 != r2 {
            rows = true;
            parts[4].push((span(area, (r1, r2 - 1), (c1, c2)), bottom));
            parts[4].push((span(area, (r1 + 1, r2), (c1, c2)), top));
        }
        if c1 != c2 {
            columns = true;
            parts[5].push((span(area, (r1, r2), (c1, c2 - 1)), right));
            parts[5].push((span(area, (r1, r2), (c1 + 1, c2)), left));
        }
        parts[6].push((area, diagonal_down));
        parts[7].push((area, diagonal_up));
    }
    let mut read = |index: usize| shared(host, &parts[index], &mut work);
    Ok(RangeBorders {
        left: read(0)?,
        right: read(1)?,
        top: read(2)?,
        bottom: read(3)?,
        // A selection one row or one column deep has no inside line: the dialog disables it.
        inside_horizontal: if rows { read(4)? } else { Some(EdgeStroke::NONE) },
        inside_vertical: if columns { read(5)? } else { Some(EdgeStroke::NONE) },
        diagonal_down: read(6)?,
        diagonal_up: read(7)?,
    })
}

fn sample_text(host: &HostState, value: Option<&calc_host::value::CalcValue>, code: &str) -> String {
    let Some(value) = value else { return String::new() };
    let locale = host.options().locale.as_ref();
    let Some(profile) = locale else {
        return calc_formula_kernel::result::shown(value, locale);
    };
    let system = if host.declared_date1904() {
        calc_func::locale::DateSystem::System1904
    } else {
        calc_func::locale::DateSystem::System1900
    };
    match calc_func::cell_text_localized(profile, system, value, code) {
        Ok(text) => text,
        Err(calc_func::CellFormatError::Unshowable) => "#####".into(),
        Err(calc_func::CellFormatError::NotModelled) => calc_formula_kernel::result::shown(value, locale),
    }
}

fn value_at(state: &State, at: CellKey, budget: Budget) -> Result<Option<calc_host::value::CalcValue>, Refusal> {
    let page = crate::query::project_areas(state.session.host(), &[GridArea::cell(at)], budget)?;
    Ok(page.cells().first().and_then(|cell| crate::query::shown_value(state, cell.value()).0))
}

pub(crate) fn facts(
    state: &State,
    selected: &[Area],
    active: Key,
    budget: Budget,
    mut work: impl FnMut() -> Result<(), Refusal>,
) -> Result<CellFormatFacts, Refusal> {
    let host = state.session.host();
    let areas = selected.iter().map(|area| grid_area(*area)).collect::<Result<Vec<_>, _>>()?;
    let at = crate::cell(active)?;
    if areas.is_empty() || !areas.iter().any(|area| area.contains(at)) {
        return Err(invalid("The active cell must be one of the selected cells"));
    }
    for area in &areas {
        crate::query::checked_sheet(host, calc_skin::grid::Sheet::of(area.sheet()))?;
    }
    let mut common = properties(host, at);
    let mut styles = BTreeSet::from([host.cell_style(at)]);
    for sample in calc_host::format_samples(host, &areas).map_err(excel)? {
        work()?;
        if !styles.insert(host.cell_style(sample)) {
            continue;
        }
        let next = properties(host, sample);
        macro_rules! intersect {
            ($($field:ident),*) => {$(if common.$field != next.$field { common.$field = None; })*};
        }
        intersect!(
            number_format, horizontal, vertical, indent, wrap, shrink, orientation, font_name, font_size, bold,
            italic, underline, font_color, strikethrough, superscript, subscript, fill_color, pattern,
            pattern_color, locked, hidden
        );
    }
    common.borders = borders(host, &areas, &mut work)?;
    // Merge cells is checked when every area is exactly one merged cell, clear when none of the
    // selection is merged, and mixed otherwise.
    let merges = host.merged_areas(areas[0].sheet());
    let meeting = merges.iter().filter(|merge| areas.iter().any(|area| merge.intersection(*area).is_some())).count();
    common.merge = if meeting == 0 {
        Some(false)
    } else if areas.iter().all(|area| merges.contains(area)) {
        Some(true)
    } else {
        None
    };
    let value = value_at(state, at, budget)?;
    let code = host.number_format(at).unwrap_or("General").to_owned();
    Ok(CellFormatFacts {
        common,
        active,
        sample: value.as_ref().map_or(RawValue::Blank, crate::query::raw),
        sample_text: sample_text(host, value.as_ref(), &code),
        custom_formats: host.styles().number_formats.iter().map(|format| format.code.clone()).collect(),
        normal_font: {
            let font = records(host, 0).font;
            (font.name.unwrap_or_else(|| "Calibri".into()), font.size.unwrap_or_else(|| "11".into()))
        },
        date1904: host.declared_date1904(),
    })
}

pub(crate) fn sample(state: &State, at: Key, code: &str, budget: Budget) -> Result<FormatSample, Refusal> {
    let host = state.session.host();
    let at = crate::cell(at)?;
    crate::query::checked_sheet(host, calc_skin::grid::Sheet::of(at.sheet()))?;
    FormatPatch { number_format: Some(code.into()), ..Default::default() }.validate().map_err(invalid)?;
    let value = value_at(state, at, budget)?;
    Ok(FormatSample { text: sample_text(host, value.as_ref(), code) })
}
