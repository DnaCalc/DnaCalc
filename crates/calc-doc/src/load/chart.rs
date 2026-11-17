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
use crate::xml::{XmlElement, XmlEvent, XmlPart};

const DRAWING: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing";
const CHART: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
const SPREADSHEET_DRAWING: &str =
    "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
const CHART_NAMESPACE: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const RELATIONSHIP_ATTRIBUTES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub use calc_doc_model::{
    Chart, ChartAnchor, ChartData, ChartPlot, ChartSeries, DrawingPoint, SheetCharts,
};

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
    let (frames, _) = match read_drawing(package, &drawing) {
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

pub(super) fn unprinted_objects(package: &Package, part: &str) -> Result<usize, LoadError> {
    read_drawing(package, part).map(|(_, count)| count)
}

fn number(text: &str) -> i64 {
    text.trim().parse().unwrap_or(0)
}

/// The chart frames of a drawing part: each frame's name, anchor and chart relationship id.
fn read_drawing(
    package: &Package,
    part: &str,
) -> Result<(Vec<(String, ChartAnchor, String)>, usize), LoadError> {
    let xml = read_xml(package, part)?;
    let mut events = Events::new(&xml);
    events.root(
        SPREADSHEET_DRAWING,
        "wsDr",
        "the document element is not a spreadsheet drawing",
    )?;
    let mut frames = Vec::new();
    let mut other_objects = 0;
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
            other_objects += 1;
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
        let mut unsupported = false;
        loop {
            match events.next()? {
                Some(XmlEvent::Start(child)) => {
                    let local = child.name.local.as_str().to_owned();
                    if child.name.namespace.as_deref() == Some(SPREADSHEET_DRAWING)
                        && (matches!(
                            local.as_str(),
                            "grpSp" | "sp" | "pic" | "cxnSp" | "contentPart"
                        ) || local == "graphicFrame" && !path.is_empty()
                            || local == "clientData"
                                && matches!(
                                    child.attribute("fPrintsWithSheet"),
                                    Some("0" | "false")
                                ))
                    {
                        unsupported = true;
                    }
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
        if unsupported {
            other_objects += 1;
        }
        let Some(chart) = chart.filter(|_| frame) else {
            other_objects += 1;
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
    Ok((frames, other_objects))
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
    read_chart_xml(&xml, part, name, anchor)
}

fn read_chart_xml(
    xml: &XmlPart,
    part: &str,
    name: String,
    anchor: ChartAnchor,
) -> Result<Chart, LoadError> {
    let mut events = Events::new(xml);
    events.root(
        CHART_NAMESPACE,
        "chartSpace",
        "the document element is not a chart",
    )?;
    let mut chart = Chart {
        part: part.to_owned(),
        name,
        anchor,
        drawing_id: None,
        lang: None,
        unique_next: 0,
        title: None,
        title_formula: None,
        auto_title_deleted: false,
        legend: None,
        plots: Vec::new(),
        axes: Vec::new(),
        text_style: Default::default(),
        element_text: Default::default(),
        element_text_complex: Default::default(),
    };
    // The path of local names below chartSpace, and where in a series the reading is.
    let mut path: Vec<String> = Vec::new();
    let mut slot: Option<Slot> = None;
    let mut point: Option<usize> = None;
    let (mut title_default, mut title_run, mut title_end) = (
        calc_doc_model::ChartTextStyle::default(),
        calc_doc_model::ChartTextStyle::default(),
        calc_doc_model::ChartTextStyle::default(),
    );
    let mut title_paragraphs = 0;
    let mut title_runs = 0;
    let mut title = String::new();
    let mut title_cache: Option<String> = None;
    let mut axis_title_has_field = false;
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
        let in_axis_title = path.len() >= 6
            && path[0] == "chart"
            && path[1] == "plotArea"
            && matches!(path[2].as_str(), "valAx" | "catAx" | "dateAx" | "serAx")
            && path[3..6] == ["title", "tx", "rich"];
        let val = element.attribute("val").map(str::to_owned);
        if element.name.is(CHART_NAMESPACE, "dLbls") {
            let names = path.iter().map(String::as_str).collect::<Vec<_>>();
            let series_scope = matches!(names.as_slice(), ["chart", "plotArea", kind, "ser"] if kind.ends_with("Chart"));
            let plot_scope = matches!(names.as_slice(), ["chart", "plotArea", kind] if kind.ends_with("Chart"));
            if series_scope || plot_scope {
                let (simple, linked) = read_simple_value_labels(&mut events)?;
                if let Some(plot) = chart.plots.last_mut() {
                    let setting = if series_scope {
                        plot.series.last_mut().map(|series| (&mut series.show_values, &mut series.value_labels_source_linked))
                    } else { Some((&mut plot.show_values, &mut plot.value_labels_source_linked)) };
                    if let Some((setting, source_linked)) = setting {
                        let valid = setting.is_none() && simple;
                        *setting = Some(valid);
                        *source_linked = valid && linked;
                    }
                }
                continue;
            }
        }
        if path.is_empty() && element.name.is(CHART_NAMESPACE, "txPr") {
            chart.text_style = read_text_style(&mut events)?;
            continue;
        }
        if element.name.is(CHART_NAMESPACE, "txPr") {
            let target = match path
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice()
            {
                ["chart", "title"] => Some("title".to_owned()),
                ["chart", "legend"] => Some("legend".to_owned()),
                ["chart", "plotArea", axis, "title"] if axis.ends_with("Ax") => {
                    chart.axes.last().map(|a| format!("axis-title:{}", a.id))
                }
                ["chart", "plotArea", axis] if axis.ends_with("Ax") => {
                    chart.axes.last().map(|a| format!("axis:{}", a.id))
                }
                _ => None,
            };
            if let Some(target) = target {
                let (style, complex, _) = read_text_style_shape(&mut events)?;
                if complex {
                    chart.element_text_complex.insert(target.clone());
                }
                if style != Default::default() {
                    merge_font(chart.element_text.entry(target).or_default(), style);
                }
                continue;
            }
        }
        if in_title
            && element.name.namespace.as_deref()
                == Some("http://schemas.openxmlformats.org/drawingml/2006/main")
        {
            let names = path.iter().skip(2).map(String::as_str).collect::<Vec<_>>();
            if matches!((names.as_slice(), local.as_str()), (["tx", "rich"], "p")) {
                title_paragraphs += 1;
            }
            if matches!(
                (names.as_slice(), local.as_str()),
                (["tx", "rich", "p"], "r")
            ) {
                title_runs += 1;
            }
            if title_paragraphs > 1
                || title_runs > 1
                || matches!(
                    (names.as_slice(), local.as_str()),
                    (["tx", "rich", "p"], "fld" | "br")
                )
            {
                chart.element_text_complex.insert("title".into());
            }
            let run = matches!(
                names.as_slice(),
                ["tx", "rich", "p", "pPr", "defRPr"]
                    | ["tx", "rich", "p", "r", "rPr"]
                    | ["tx", "rich", "p", "endParaRPr"]
            );
            let properties = matches!(
                (names.as_slice(), local.as_str()),
                (["tx", "rich", "p", "pPr"], "defRPr")
                    | (["tx", "rich", "p", "r"], "rPr")
                    | (["tx", "rich", "p"], "endParaRPr")
            );
            let layer = if names.iter().any(|n| *n == "r") {
                &mut title_run
            } else if names.last() == Some(&"endParaRPr") || local == "endParaRPr" {
                &mut title_end
            } else {
                &mut title_default
            };
            if properties {
                font_attributes(layer, &element);
            }
            if local == "latin" && run {
                layer.family = element.attribute("typeface").map(str::to_owned);
            }
            if matches!(local.as_str(), "srgbClr" | "schemeClr")
                && names.last() == Some(&"solidFill")
                && matches!(
                    &names[..names.len() - 1],
                    ["tx", "rich", "p", "pPr", "defRPr"]
                        | ["tx", "rich", "p", "r", "rPr"]
                        | ["tx", "rich", "p", "endParaRPr"]
                )
            {
                layer.color = element.attribute("val").map(str::to_owned);
            }
        }
        let plot = chart.plots.last_mut();
        match (parent, local.as_str()) {
            (_, "fld") if in_axis_title
                && element.name.namespace.as_deref()
                    == Some("http://schemas.openxmlformats.org/drawingml/2006/main") =>
            {
                axis_title_has_field = true;
                if let Some(axis) = chart.axes.last_mut() {
                    axis.title = None;
                }
            }
            (_, "p" | "br") if in_axis_title
                && !axis_title_has_field
                && element.name.namespace.as_deref()
                    == Some("http://schemas.openxmlformats.org/drawingml/2006/main") =>
            {
                if let Some(title) = chart.axes.last_mut().and_then(|axis| axis.title.as_mut())
                    && !title.is_empty()
                {
                    title.push(' ');
                }
            }
            (_, "t") if in_axis_title
                && !axis_title_has_field
                && element.name.namespace.as_deref()
                    == Some("http://schemas.openxmlformats.org/drawingml/2006/main") =>
            {
                let text = events.text()?;
                if let Some(axis) = chart.axes.last_mut() {
                    axis.title.get_or_insert_with(String::new).push_str(&text);
                }
                continue;
            }
            (Some("plotArea"), "valAx" | "catAx" | "dateAx" | "serAx")
                if element.name.namespace.as_deref() == Some(CHART_NAMESPACE) =>
            {
                axis_title_has_field = false;
                chart.axes.push(calc_doc_model::ChartAxis {
                    kind: local.clone(),
                    ..Default::default()
                });
            }
            (
                Some("valAx" | "catAx" | "dateAx" | "serAx"),
                "axId" | "crossAx" | "axPos" | "majorUnit" | "crossBetween",
            ) if element.name.namespace.as_deref() == Some(CHART_NAMESPACE) => {
                if let Some(axis) = chart.axes.last_mut() {
                    match local.as_str() {
                        "axId" => axis.id = val.and_then(|s| s.parse().ok()).unwrap_or(0),
                        "axPos" => axis.position = val,
                        "crossBetween" => axis.cross_between = val,
                        "crossAx" => axis.cross_axis = val.and_then(|s| s.parse().ok()),
                        _ => axis.major_unit = val,
                    }
                }
            }
            (Some("scaling"), "min" | "max" | "orientation" | "logBase")
                if element.name.namespace.as_deref() == Some(CHART_NAMESPACE)
                    && path.len() == 4
                    && path[0] == "chart"
                    && path[1] == "plotArea"
                    && path[2].ends_with("Ax") =>
            {
                if let Some(axis) = chart.axes.last_mut() {
                    match local.as_str() {
                        "min" => axis.minimum = val,
                        "max" => axis.maximum = val,
                        "orientation" => axis.reversed = val.as_deref() == Some("maxMin"),
                        _ => axis.log_base = val,
                    }
                }
            }
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
            (Some(p), "axId")
                if p.ends_with("Chart")
                    && element.name.namespace.as_deref() == Some(CHART_NAMESPACE) =>
            {
                if let (Some(plot), Some(id)) = (plot, val.and_then(|s| s.parse().ok())) {
                    plot.axis_ids.push(id);
                }
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
            (Some("solidFill"), "srgbClr" | "schemeClr")
                if path.len() >= 4
                    && path[path.len() - 4..] == ["ser", "spPr", "ln", "solidFill"] =>
            {
                if let Some(plot) = plot
                    && matches!(plot.kind.as_str(), "lineChart" | "scatterChart")
                    && let Some(series) = plot.series.last_mut()
                    && series.fill.is_none()
                {
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
    if title_runs == 0 {
        merge_font(
            chart.element_text.entry("title".into()).or_default(),
            title_end,
        );
    }
    merge_font(
        chart.element_text.entry("title".into()).or_default(),
        title_default,
    );
    merge_font(
        chart.element_text.entry("title".into()).or_default(),
        title_run,
    );
    chart
        .element_text
        .retain(|_, style| *style != Default::default());
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

/// Only a single showVal flag is safe without modelling per-point and formatting overrides.
fn read_simple_value_labels(events: &mut Events<'_>) -> Result<(bool, bool), LoadError> {
    let mut depth = 0usize;
    let mut count = 0usize;
    let mut valid = true;
    let mut linked = false;
    let mut formats = 0usize;
    loop {
        match events.next()? {
            Some(XmlEvent::Start(element)) => {
                depth += 1;
                if depth == 1 && element.name.is(CHART_NAMESPACE, "showVal") {
                    count += 1;
                    valid &= matches!(element.attribute("val"), Some("1" | "true"));
                } else if depth == 1 && element.name.is(CHART_NAMESPACE, "numFmt") {
                    formats += 1;
                    linked = matches!(element.attribute("sourceLinked"), Some("1" | "true") | None)
                        && element.attribute("formatCode").is_some();
                    valid &= linked && formats == 1;
                } else { valid = false; }
            }
            Some(XmlEvent::End(_)) if depth == 0 => break,
            Some(XmlEvent::End(_)) => depth -= 1,
            Some(XmlEvent::Text(text)) => valid &= text.trim().is_empty(),
            None => return Err(events.invalid("the part ends inside chart data labels")),
        }
    }
    Ok((valid && count == 1, linked))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn value_labels_accept_only_an_unambiguous_show_value_flag() {
        for (labels, expected, linked) in [
            (r#"<c:showVal val="1"/>"#, Some(true), false),
            (r#"<c:numFmt formatCode="General" sourceLinked="1"/><c:showVal val="1"/>"#, Some(true), true),
            (r#"<c:showVal val="0"/>"#, Some(false), false),
            (r#"<c:showVal val="1">unexpected</c:showVal>"#, Some(false), false),
            (" \n<c:showVal val=\"1\"/> \n", Some(true), false),
            (r#"<c:showVal val="1"/><c:numFmt formatCode="0%" sourceLinked="0"/>"#, Some(false), false),
            (r#"<c:showVal val="1"/><c:dLbl><c:idx val="0"/></c:dLbl>"#, Some(false), false),
            (r#"<c:showVal val="1"/><c:showVal val="1"/>"#, Some(false), false),
        ] {
            let text = format!(r#"<c:chartSpace xmlns:c="{CHART_NAMESPACE}"><c:chart><c:plotArea><c:barChart><c:ser><c:idx val="0"/><c:dLbls>{labels}</c:dLbls></c:ser><c:dLbls><c:showVal val="1"/></c:dLbls></c:barChart></c:plotArea></c:chart></c:chartSpace>"#);
            let xml = XmlPart::new("chart.xml".into(), text.into_bytes(), crate::xml::XmlLimits::default());
            let chart = read_chart_xml(&xml, "chart.xml", "Chart".into(), ChartAnchor::Absolute {
                x: 0, y: 0, width: 100, height: 100,
            }).unwrap();
            assert_eq!(chart.plots[0].show_values, Some(true));
            assert_eq!(chart.plots[0].series[0].show_values, expected);
            assert_eq!(chart.plots[0].series[0].value_labels_source_linked, linked);
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn axis_titles_read_rich_text_without_using_linked_caches() {
        let text = format!(r#"<c:chartSpace xmlns:c="{CHART_NAMESPACE}" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea>
            <c:catAx><c:axId val="1"/><c:title><c:tx><c:rich><a:p><a:r><a:t>East &amp; </a:t></a:r><a:r><a:t>West</a:t></a:r></a:p><a:p><a:r><a:t>regions</a:t></a:r></a:p></c:rich></c:tx></c:title></c:catAx>
            <c:valAx><c:axId val="2"/><c:title><c:tx><c:strRef><c:f>Sheet1!A1</c:f><c:strCache><c:pt idx="0"><c:v>stale</c:v></c:pt></c:strCache></c:strRef></c:tx></c:title></c:valAx>
            <c:valAx><c:axId val="3"/><c:title><c:tx><c:rich><a:p><a:r><a:t>Prefix</a:t></a:r><a:fld id="x" type="date"><a:t>stale field</a:t></a:fld><a:r><a:t>Suffix</a:t></a:r></a:p></c:rich></c:tx></c:title></c:valAx>
            </c:plotArea></c:chart></c:chartSpace>"#);
        let xml = XmlPart::new("chart.xml".into(), text.into_bytes(), crate::xml::XmlLimits::default());
        let chart = read_chart_xml(&xml, "chart.xml", "Chart".into(), ChartAnchor::Absolute {
            x: 0, y: 0, width: 100, height: 100,
        }).unwrap();
        assert_eq!(chart.axes[0].title.as_deref(), Some("East & West regions"));
        assert_eq!(chart.axes[1].title, None);
        assert_eq!(chart.axes[2].title, None);
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn outline_colours_do_not_replace_shape_fills() {
        for kind in [
            "barChart",
            "areaChart",
            "pieChart",
            "lineChart",
            "scatterChart",
        ] {
            for shape in [true, false] {
                let fill = if shape {
                    r#"<a:solidFill><a:srgbClr val="FF0000"/></a:solidFill>"#
                } else {
                    ""
                };
                let text = format!(
                    r#"<c:chartSpace xmlns:c="{CHART_NAMESPACE}" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:{kind}><c:ser><c:idx val="0"/><c:spPr>{fill}<a:ln><a:solidFill><a:srgbClr val="0000FF"/></a:solidFill></a:ln></c:spPr></c:ser></c:{kind}></c:plotArea></c:chart></c:chartSpace>"#
                );
                let xml = XmlPart::new(
                    "chart.xml".into(),
                    text.into_bytes(),
                    crate::xml::XmlLimits::default(),
                );
                let chart = read_chart_xml(
                    &xml,
                    "chart.xml",
                    "Chart".into(),
                    ChartAnchor::Absolute {
                        x: 0,
                        y: 0,
                        width: 100,
                        height: 100,
                    },
                )
                .unwrap();
                assert_eq!(
                    chart.plots[0].series[0].fill.as_deref(),
                    if shape {
                        Some("FF0000")
                    } else if matches!(kind, "lineChart" | "scatterChart") {
                        Some("0000FF")
                    } else {
                        None
                    },
                    "{kind}, shape={shape}"
                );
            }
        }
    }
}

fn read_text_style(events: &mut Events<'_>) -> Result<calc_doc_model::ChartTextStyle, LoadError> {
    read_text_style_shape(events).map(|(_, _, legacy)| legacy)
}
fn read_text_style_shape(
    events: &mut Events<'_>,
) -> Result<
    (
        calc_doc_model::ChartTextStyle,
        bool,
        calc_doc_model::ChartTextStyle,
    ),
    LoadError,
> {
    let (mut default_font, mut run_font, mut end_font) = (
        calc_doc_model::ChartTextStyle::default(),
        calc_doc_model::ChartTextStyle::default(),
        calc_doc_model::ChartTextStyle::default(),
    );
    let mut paragraphs = 0;
    let mut runs = 0;
    let mut complex = false;
    let mut style = calc_doc_model::ChartTextStyle::default();
    let mut path = Vec::<String>::new();
    let run = |p: &[String]| {
        matches!(
            p.iter().map(String::as_str).collect::<Vec<_>>().as_slice(),
            ["p", "pPr", "defRPr"] | ["p", "r", "rPr"] | ["p", "endParaRPr"]
        )
    };
    loop {
        match events.next()? {
            Some(XmlEvent::Start(e)) => {
                if e.name.namespace.as_deref()
                    != Some("http://schemas.openxmlformats.org/drawingml/2006/main")
                {
                    events.skip()?;
                    continue;
                }
                let local = e.name.local.as_str();
                let names = path.iter().map(String::as_str).collect::<Vec<_>>();
                if names.is_empty() && local == "p" {
                    paragraphs += 1;
                }
                if names == ["p"] && local == "r" {
                    runs += 1;
                }
                complex |=
                    paragraphs > 1 || runs > 1 || (names == ["p"] && matches!(local, "fld" | "br"));
                let properties = matches!(
                    (names.as_slice(), local),
                    (["p", "pPr"], "defRPr") | (["p", "r"], "rPr") | (["p"], "endParaRPr")
                );
                let layer = if path.iter().any(|p| p == "r") {
                    &mut run_font
                } else if path.last().is_some_and(|p| p == "endParaRPr") || local == "endParaRPr" {
                    &mut end_font
                } else {
                    &mut default_font
                };
                if properties {
                    font_attributes(layer, &e);
                    if let Some(size) = e.attribute("sz").and_then(|s| s.parse().ok()) {
                        style.size = Some(size);
                    }
                    for (key, target) in [("b", &mut style.bold), ("i", &mut style.italic)] {
                        if let Some(v) = e.attribute(key) {
                            *target = match v {
                                "1" | "true" => Some(true),
                                "0" | "false" => Some(false),
                                _ => None,
                            };
                        }
                    }
                }
                if local == "latin" && run(&path) {
                    style.family = e.attribute("typeface").map(str::to_owned);
                    layer.family = style.family.clone();
                    events.skip()?;
                    continue;
                }
                if matches!(local, "srgbClr" | "schemeClr")
                    && path.last().is_some_and(|p| p == "solidFill")
                    && run(&path[..path.len() - 1])
                {
                    style.color = e.attribute("val").map(str::to_owned);
                    layer.color = style.color.clone();
                    events.skip()?;
                    continue;
                }
                if properties
                    || matches!((names.as_slice(), local), ([], "p") | (["p"], "pPr" | "r"))
                    || (local == "solidFill" && run(&path))
                {
                    path.push(local.into());
                } else {
                    events.skip()?;
                }
            }
            Some(XmlEvent::End(_)) => {
                if path.pop().is_none() {
                    let mut layered = if runs == 0 {
                        end_font
                    } else {
                        Default::default()
                    };
                    merge_font(&mut layered, default_font);
                    merge_font(&mut layered, run_font);
                    return Ok((layered, complex, style));
                }
            }
            Some(XmlEvent::Text(_)) => {}
            None => return Err(events.invalid("The chart text defaults end before their close")),
        }
    }
}

fn font_attributes(style: &mut calc_doc_model::ChartTextStyle, element: &XmlElement) {
    if let Some(size) = element.attribute("sz").and_then(|s| s.parse().ok()) {
        style.size = Some(size);
    }
    for (key, target) in [("b", &mut style.bold), ("i", &mut style.italic)] {
        if let Some(v) = element.attribute(key) {
            *target = match v {
                "1" | "true" => Some(true),
                "0" | "false" => Some(false),
                _ => None,
            };
        }
    }
}
fn merge_font(base: &mut calc_doc_model::ChartTextStyle, style: calc_doc_model::ChartTextStyle) {
    if style.family.is_some() {
        base.family = style.family;
    }
    if style.size.is_some() {
        base.size = style.size;
    }
    if style.bold.is_some() {
        base.bold = style.bold;
    }
    if style.italic.is_some() {
        base.italic = style.italic;
    }
    if style.color.is_some() {
        base.color = style.color;
    }
}
