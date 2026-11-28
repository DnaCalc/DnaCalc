//! Border edge changes keyed by Excel's line style and weight, for `Range.Borders` (dc-oxh8w.33).
use crate::FillColor;
use calc_doc_model::{Border, BorderEdge};
use calc_value::{CellKey, GridArea, MAX_COLUMNS, MAX_ROWS};

/// Excel's `XlLineStyle` families.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeLine {
    /// `xlLineStyleNone`: no line.
    None,
    /// `xlContinuous`.
    Continuous,
    /// `xlDash`.
    Dash,
    /// `xlDashDot`.
    DashDot,
    /// `xlDashDotDot`.
    DashDotDot,
    /// `xlDot`.
    Dot,
    /// `xlDouble`.
    Double,
    /// `xlSlantDashDot`.
    SlantDashDot,
}

/// Excel's `XlBorderWeight`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeWeight {
    /// `xlHairline`.
    Hairline,
    /// `xlThin`.
    Thin,
    /// `xlMedium`.
    Medium,
    /// `xlThick`.
    Thick,
}

/// Property changes of one border edge. A weight or colour set on an edge without a line draws a thin continuous line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EdgeChange {
    /// New line style family.
    pub line: Option<EdgeLine>,
    /// New weight.
    pub weight: Option<EdgeWeight>,
    /// New colour.
    pub color: Option<FillColor>,
}

/// Changes to individual cell edges, leaving other edges alone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EdgesPatch {
    /// Left edge.
    pub left: Option<EdgeChange>,
    /// Right edge.
    pub right: Option<EdgeChange>,
    /// Top edge.
    pub top: Option<EdgeChange>,
    /// Bottom edge.
    pub bottom: Option<EdgeChange>,
    /// Diagonal from the lower left to the upper right (`xlDiagonalUp`).
    pub diagonal_up: Option<EdgeChange>,
    /// Diagonal from the upper left to the lower right (`xlDiagonalDown`).
    pub diagonal_down: Option<EdgeChange>,
}

/// A side of a rectangular cell range with a corresponding side on its neighbour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BorderSide {
    Left,
    Top,
    Bottom,
    Right,
}

impl BorderSide {
    pub(crate) const ALL: [Self; 4] = [Self::Left, Self::Top, Self::Bottom, Self::Right];

    pub(crate) fn strips(self, area: GridArea) -> (GridArea, Option<(GridArea, Self)>) {
        let first = area.first();
        let last = area.last();
        let (top, left, bottom, right) = (first.row(), first.column(), last.row(), last.column());
        let rows = (top, bottom);
        let columns = (left, right);
        match self {
            Self::Left => (
                span(area, rows, (left, left)),
                (left > 1).then(|| (span(area, rows, (left - 1, left - 1)), Self::Right)),
            ),
            Self::Top => (
                span(area, (top, top), columns),
                (top > 1).then(|| (span(area, (top - 1, top - 1), columns), Self::Bottom)),
            ),
            Self::Bottom => (
                span(area, (bottom, bottom), columns),
                (bottom < MAX_ROWS)
                    .then(|| (span(area, (bottom + 1, bottom + 1), columns), Self::Top)),
            ),
            Self::Right => (
                span(area, rows, (right, right)),
                (right < MAX_COLUMNS)
                    .then(|| (span(area, rows, (right + 1, right + 1)), Self::Left)),
            ),
        }
    }

    pub(crate) fn patch(self, change: EdgeChange) -> EdgesPatch {
        let mut edges = EdgesPatch::default();
        let slot = match self {
            Self::Left => &mut edges.left,
            Self::Top => &mut edges.top,
            Self::Bottom => &mut edges.bottom,
            Self::Right => &mut edges.right,
        };
        *slot = Some(change);
        edges
    }
}

pub(crate) fn span(area: GridArea, rows: (u32, u32), columns: (u32, u32)) -> GridArea {
    let key = |row, column| CellKey::new(area.sheet(), row, column).expect("strip inside the grid");
    GridArea::new(key(rows.0, columns.0), key(rows.1, columns.1)).expect("one sheet")
}

const TABLE: &[(&str, EdgeLine, EdgeWeight)] = &[
    ("hair", EdgeLine::Continuous, EdgeWeight::Hairline),
    ("thin", EdgeLine::Continuous, EdgeWeight::Thin),
    ("medium", EdgeLine::Continuous, EdgeWeight::Medium),
    ("thick", EdgeLine::Continuous, EdgeWeight::Thick),
    ("dashed", EdgeLine::Dash, EdgeWeight::Thin),
    ("mediumDashed", EdgeLine::Dash, EdgeWeight::Medium),
    ("dotted", EdgeLine::Dot, EdgeWeight::Thin),
    ("dashDot", EdgeLine::DashDot, EdgeWeight::Thin),
    ("mediumDashDot", EdgeLine::DashDot, EdgeWeight::Medium),
    ("dashDotDot", EdgeLine::DashDotDot, EdgeWeight::Thin),
    ("mediumDashDotDot", EdgeLine::DashDotDot, EdgeWeight::Medium),
    ("double", EdgeLine::Double, EdgeWeight::Thick),
    ("slantDashDot", EdgeLine::SlantDashDot, EdgeWeight::Medium),
];

/// Line style and weight of a stored OOXML border style keyword.
pub fn decompose(style: Option<&str>) -> Option<(EdgeLine, Option<EdgeWeight>)> {
    match style {
        None | Some("none") => Some((EdgeLine::None, None)),
        Some(s) => TABLE.iter().find(|(name, ..)| *name == s).map(|(_, l, w)| (*l, Some(*w))),
    }
}

fn compose(line: EdgeLine, weight: EdgeWeight) -> Option<&'static str> {
    TABLE.iter().find(|(_, l, w)| *l == line && *w == weight).map(|(name, ..)| *name)
}

fn natural_weight(line: EdgeLine) -> EdgeWeight {
    match line {
        EdgeLine::Double => EdgeWeight::Thick,
        EdgeLine::SlantDashDot => EdgeWeight::Medium,
        _ => EdgeWeight::Thin,
    }
}

pub(crate) fn apply_edge(edge: &mut Option<BorderEdge>, change: &EdgeChange) -> Result<(), String> {
    if change.line == Some(EdgeLine::None) {
        *edge = None;
        return Ok(());
    }
    let current = edge
        .as_ref()
        .and_then(|e| decompose(e.style.as_deref()))
        .filter(|(line, _)| *line != EdgeLine::None);
    if change.line.is_some() || change.weight.is_some() {
        let line = change.line.or(current.map(|c| c.0)).unwrap_or(EdgeLine::Continuous);
        let candidates = [change.weight, current.and_then(|c| c.1), Some(natural_weight(line))];
        let found = candidates
            .into_iter()
            .flatten()
            .find_map(|w| compose(line, w))
            .filter(|_| change.weight.is_none() || compose(line, change.weight.unwrap()).is_some());
        // ww23 (Excel 16.0.20625): a Weight set alone on an edge whose line style has no such weight makes the edge
        // continuous at that weight. A line style and a weight set together that do not exist are refused.
        let style = match found {
            Some(style) => style,
            None if change.line.is_none() => change.weight.and_then(|w| compose(EdgeLine::Continuous, w)).ok_or("Excel has no border with that line style and weight")?,
            None => return Err("Excel has no border with that line style and weight".into()),
        };
        let target = edge.get_or_insert_with(BorderEdge::default);
        target.style = Some(style.into());
    } else if current.is_none() {
        // A colour set on an edge without a line draws a thin one (the colour follows below).
        *edge = Some(BorderEdge {
            style: Some("thin".into()),
            color: edge.take().and_then(|e| e.color),
        });
    }
    if let Some(color) = &change.color {
        let color = color.style()?;
        edge.as_mut().expect("edge drawn").color = color;
    }
    Ok(())
}

pub(crate) fn apply(border: &mut Border, patch: &EdgesPatch) -> Result<(), String> {
    for (edge, change) in [
        (&mut border.left, &patch.left),
        (&mut border.right, &patch.right),
        (&mut border.top, &patch.top),
        (&mut border.bottom, &patch.bottom),
    ] {
        if let Some(change) = change {
            apply_edge(edge, change)?;
        }
    }
    for (up, change) in [(true, &patch.diagonal_up), (false, &patch.diagonal_down)] {
        if let Some(change) = change {
            apply_edge(&mut border.diagonal, change)?;
            let drawn = border.diagonal.is_some();
            if up {
                border.diagonal_up = drawn;
            } else {
                border.diagonal_down = drawn;
            }
        }
    }
    Ok(())
}
