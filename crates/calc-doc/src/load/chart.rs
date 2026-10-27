//! A worksheet's charts (dc-6yw): the drawing part its relationships name, read for the anchor of
//! each chart frame, and each chart part such a frame names, read for its title, its plots (bar,
//! line, area, pie, doughnut, scatter and the rest by their element names) and each plot's series
//! with the formulas and cached points of their name, categories (X values) and values (Y values).
//! The parts stay in the package as stored; what the series show now is calc-host's to evaluate.
//!
//! A drawing or chart part that cannot be read leaves its charts out and is counted
//! ([`SheetCharts::unread`]): a chart never refuses the workbook.

use super::{Events, LoadError, read_xml};
use crate::opc::{Package, TargetMode};
use crate::xml::{XmlElement, XmlEvent};

const DRAWING: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing";
const CHART: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
const SPREADSHEET_DRAWING: &str =
    "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
const CHART_NAMESPACE: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const RELATIONSHIP_ATTRIBUTES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// A point of the sheet: a zero-based column and row, as the drawing part counts them, and the
/// offsets into that cell in EMU (914,400 to the inch; 9,525 to a pixel at 96 dots per inch).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawingPoint {
    pub column: u32,
    pub column_offset: i64,
    pub row: u32,
    pub row_offset: i64,
}

/// Where a chart's frame is on the sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChartData {
    pub formula: Option<String>,
    pub cache: Vec<Option<String>>,
    /// Whether the cache is numbers (`c:numRef`, `c:numLit`) rather than text.
    pub numeric: bool,
}

/// One `c:ser`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SheetCharts {
    pub charts: Vec<Chart>,
    pub unread: usize,
}

fn relationship_id(element: &XmlElement) -> Option<&str> {
    element
        .attributes
        .iter()
        .find(|attribute| attribute.name.is(RELATIONSHIP_ATTRIBUTES, "id"))
        .map(|attribute| attribute.value.as_str())
}

/// The internal part `part`'s relationship `id` (or the first of type `kind` when `id` is
/// `None`) resolves to.
fn related(package: &Package, part: &str, id: Option<&str>, kind: &str) -> Option<String> {
    package
        .relationships_of(Some(part))?
        .items()
        .iter()
        .find(|item| {
            item.kind == kind
                && item.mode == TargetMode::Internal
                && id.is_none_or(|id| item.id == id)
        })?
        .resolved
        .clone()
}

/// The charts of the worksheet part `sheet_part`, in the drawing's order.
pub(crate) fn read_charts(package: &Package, sheet_part: &str) -> SheetCharts {
    let mut read = SheetCharts::default();
    let Some(drawing) = related(package, sheet_part, None, DRAWING) else {
        return read;
    };
    let frames = match read_drawing(package, &drawing) {
        Ok(frames) => frames,
        Err(_) => {
            read.unread += 1;
            return read;
        }
    };
    for (name, anchor, id) in frames {
        let chart = related(package, &drawing, Some(&id), CHART)
            .and_then(|part| read_chart(package, &part, name, anchor).ok());
        match chart {
            Some(chart) => read.charts.push(chart),
            None => read.unread += 1,
        }
    }
    read
}

fn number(text: &str) -> i64 {
    text.trim().parse().unwrap_or(0)
}

/// The chart frames of a drawing part: each frame's name, anchor and chart relationship id.
fn read_drawing(
    package: &Package,
    part: &str,
) -> Result<Vec<(String, ChartAnchor, String)>, LoadError> {
    let xml = read_xml(package, part)?;
    let mut events = Events::new(&xml);
    events.root(
        SPREADSHEET_DRAWING,
        "wsDr",
        "the document element is not a spreadsheet drawing",
    )?;
    let mut frames = Vec::new();
    loop {
        let element = match events.next()? {
            Some(XmlEvent::Start(element)) => element,
            Some(XmlEvent::Text(_)) => continue,
            Some(XmlEvent::End(_)) => break,
            None => return Err(events.invalid("the part ends inside wsDr")),
        };
        let kind = element.name.local.as_str().to_owned();
        if element.name.namespace.as_deref() != Some(SPREADSHEET_DRAWING)
            || !matches!(
                kind.as_str(),
                "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor"
            )
        {
            events.skip()?;
            continue;
        }
        // One anchor: its points, its size, and a chart frame's name and relationship.
        let (mut from, mut to) = (DrawingPoint::default(), DrawingPoint::default());
        let (mut x, mut y, mut width, mut height) = (0i64, 0i64, 0i64, 0i64);
        let mut name = String::new();
        let mut chart: Option<String> = None;
        let mut path: Vec<String> = Vec::new();
        let mut frame = false;
        loop {
            match events.next()? {
                Some(XmlEvent::Start(child)) => {
                    let local = child.name.local.as_str().to_owned();
                    match local.as_str() {
                        "graphicFrame" => frame = true,
                        "cNvPr" if frame => {
                            name = child.attribute("name").unwrap_or_default().to_owned();
                        }
                        "chart" if child.name.is(CHART_NAMESPACE, "chart") => {
                            chart = relationship_id(&child).map(str::to_owned);
                        }
                        "ext" if path.is_empty() => {
                            width = number(child.attribute("cx").unwrap_or_default());
                            height = number(child.attribute("cy").unwrap_or_default());
                        }
                        "pos" if path.is_empty() => {
                            x = number(child.attribute("x").unwrap_or_default());
                            y = number(child.attribute("y").unwrap_or_default());
                        }
                        "col" | "colOff" | "row" | "rowOff" if path.len() == 1 => {
                            let value = number(&events.text()?);
                            let point = if path[0] == "to" { &mut to } else { &mut from };
                            match local.as_str() {
                                "col" => point.column = u32::try_from(value).unwrap_or(0),
                                "row" => point.row = u32::try_from(value).unwrap_or(0),
                                "colOff" => point.column_offset = value,
                                _ => point.row_offset = value,
                            }
                            continue;
                        }
                        _ => {}
                    }
                    path.push(local);
                }
                Some(XmlEvent::End(_)) => {
                    if path.pop().is_none() {
                        break;
                    }
                }
                Some(XmlEvent::Text(_)) => {}
                None => return Err(events.invalid("the part ends inside an anchor")),
            }
        }
        let Some(chart) = chart.filter(|_| frame) else {
            continue;
        };
        let anchor = match kind.as_str() {
            "twoCellAnchor" => ChartAnchor::TwoCell { from, to },
            "oneCellAnchor" => ChartAnchor::OneCell {
                from,
                width,
                height,
            },
            _ => ChartAnchor::Absolute {
                x,
                y,
                width,
                height,
            },
        };
        frames.push((name, anchor, chart));
    }
    Ok(frames)
}

/// Which of a series' data an element under `c:ser` holds.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    Name,
    Categories,
    Values,
}

fn read_chart(
    package: &Package,
    part: &str,
    name: String,
    anchor: ChartAnchor,
) -> Result<Chart, LoadError> {
    let xml = read_xml(package, part)?;
    let mut events = Events::new(&xml);
    events.root(
        CHART_NAMESPACE,
        "chartSpace",
        "the document element is not a chart",
    )?;
    let mut chart = Chart {
        part: part.to_owned(),
        name,
        anchor,
        title: None,
        title_formula: None,
        auto_title_deleted: false,
        legend: None,
        plots: Vec::new(),
    };
    // The path of local names below chartSpace, and where in a series the reading is.
    let mut path: Vec<String> = Vec::new();
    let mut slot: Option<Slot> = None;
    let mut point: Option<usize> = None;
    let mut title = String::new();
    let mut title_cache: Option<String> = None;
    loop {
        let element = match events.next()? {
            Some(XmlEvent::Start(element)) => element,
            Some(XmlEvent::End(_)) => {
                match path.pop().as_deref() {
                    None => break,
                    Some("tx" | "cat" | "val" | "xVal" | "yVal")
                        if path.last().is_some_and(|p| p == "ser") =>
                    {
                        slot = None;
                    }
                    _ => {}
                }
                continue;
            }
            Some(XmlEvent::Text(_)) => continue,
            None => return Err(events.invalid("the part ends inside chartSpace")),
        };
        let local = element.name.local.as_str().to_owned();
        let parent = path.last().map(String::as_str);
        let in_title = path.len() >= 2 && path[0] == "chart" && path[1] == "title";
        let val = element.attribute("val").map(str::to_owned);
        let plot = chart.plots.last_mut();
        match (parent, local.as_str()) {
            (Some("chart"), "autoTitleDeleted") => {
                chart.auto_title_deleted = matches!(val.as_deref(), Some("1" | "true"));
            }
            (Some("chart"), "legend") => chart.legend = Some("r".to_owned()),
            (Some("legend"), "legendPos") => chart.legend = val.or(Some("r".to_owned())),
            (Some("plotArea"), kind) if kind.ends_with("Chart") => {
                chart.plots.push(ChartPlot {
                    kind: kind.to_owned(),
                    ..ChartPlot::default()
                });
            }
            (Some(p), "barDir") if p.ends_with("Chart") => {
                if let Some(plot) = plot {
                    plot.bar_direction = val;
                }
            }
            (Some(p), "grouping") if p.ends_with("Chart") => {
                if let Some(plot) = plot {
                    plot.grouping = val;
                }
            }
            (Some(p), "varyColors") if p.ends_with("Chart") => {
                if let Some(plot) = plot {
                    plot.vary_colors = matches!(val.as_deref(), Some("1" | "true") | None);
                }
            }
            (Some(p), "ser") if p.ends_with("Chart") => {
                if let Some(plot) = plot {
                    plot.series.push(ChartSeries::default());
                }
            }
            (Some("ser"), "idx") => {
                if let Some(series) = plot.and_then(|plot| plot.series.last_mut()) {
                    series.index = val.and_then(|v| v.trim().parse().ok()).unwrap_or(0);
                }
            }
            (Some("ser"), "tx" | "cat" | "val" | "xVal" | "yVal") => {
                slot = Some(match local.as_str() {
                    "tx" => Slot::Name,
                    "cat" | "xVal" => Slot::Categories,
                    _ => Slot::Values,
                });
            }
            (Some("solidFill"), "srgbClr" | "schemeClr")
                if path.len() >= 3
                    && path[path.len() - 2] == "spPr"
                    && path[path.len() - 3] == "ser" =>
            {
                if let Some(series) = plot.and_then(|plot| plot.series.last_mut()) {
                    series.fill = val;
                }
            }
            (_, "f") if in_title => {
                chart.title_formula = Some(events.text()?);
                continue;
            }
            (_, "v") if in_title && path.iter().any(|p| p == "strCache") => {
                title_cache = Some(events.text()?);
                continue;
            }
            (_, "t") if in_title => {
                title.push_str(&events.text()?);
                continue;
            }
            (_, "f") if slot.is_some() => {
                let text = events.text()?;
                if let Some(data) = data_of(&mut chart.plots, slot) {
                    data.formula = Some(text);
                }
                continue;
            }
            (_, "numRef" | "numLit") if slot.is_some() => {
                if let Some(data) = data_of(&mut chart.plots, slot) {
                    data.numeric = true;
                }
            }
            (_, "pt") if slot.is_some() => {
                point = element
                    .attribute("idx")
                    .and_then(|idx| idx.trim().parse().ok());
            }
            (Some("pt"), "v") if slot.is_some() => {
                let text = events.text()?;
                if let (Some(index), Some(data)) = (point, data_of(&mut chart.plots, slot))
                    && index < 1_048_576
                {
                    if data.cache.len() <= index {
                        data.cache.resize(index + 1, None);
                    }
                    data.cache[index] = Some(text);
                }
                continue;
            }
            (Some("tx"), "v") if slot == Some(Slot::Name) => {
                // A name written as text (`c:tx/c:v`).
                let text = events.text()?;
                if let Some(data) = data_of(&mut chart.plots, slot) {
                    data.cache = vec![Some(text)];
                }
                continue;
            }
            _ => {}
        }
        path.push(local);
    }
    // A paragraph break between a rich title's paragraphs is not kept: Excel's title text joins
    // them with a line break, which the title here shows as a space.
    chart.title = if !title.is_empty() {
        Some(title)
    } else {
        title_cache
    };
    Ok(chart)
}

/// The data of the last series of the last plot that `slot` names, made when missing.
fn data_of(plots: &mut [ChartPlot], slot: Option<Slot>) -> Option<&mut ChartData> {
    let series = plots.last_mut()?.series.last_mut()?;
    let data = match slot? {
        Slot::Name => &mut series.name,
        Slot::Categories => &mut series.categories,
        Slot::Values => &mut series.values,
    };
    Some(data.get_or_insert_with(ChartData::default))
}
