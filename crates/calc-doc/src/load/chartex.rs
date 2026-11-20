//! Excel 2016's charts (dc-zjsg.6): the `cx:chart` frames of a drawing part (a frame Excel writes in a block of
//! alternatives, whole anchors or the object inside one, with a text-box fallback for older readers), and each
//! `cx:chartSpace` part they name, read into the chart model: a plot of kind `cx:<layoutId>` (`cx:waterfall`,
//! `cx:funnel`, `cx:clusteredColumn` for a histogram, `cx:treemap`, ...) whose series carry the formulas and cached
//! points of their data. The parts stay in the package as stored.

use super::drawing::{Node, alternate_content_branch, part_tree, point};
use crate::opc::Package;
use calc_doc_model::{Chart, ChartAnchor, ChartData, ChartPlot, ChartSeries};

pub(super) const CHARTEX: &str = "http://schemas.microsoft.com/office/2014/relationships/chartEx";
const CHARTEX_URI: &str = "http://schemas.microsoft.com/office/drawing/2014/chartex";

/// Whether `frame` (an `xdr:graphicFrame`) holds an Excel 2016 chart.
pub(super) fn is_chartex_frame(frame: &Node) -> bool {
    frame.child("graphic").and_then(|g| g.child("graphicData")).and_then(|d| d.attribute("uri")) == Some(CHARTEX_URI)
}

fn anchor_of(node: &Node) -> Option<ChartAnchor> {
    let ext = |n: &Node| (n.number("cx").unwrap_or(0), n.number("cy").unwrap_or(0));
    Some(match node.local.as_str() {
        "twoCellAnchor" => ChartAnchor::TwoCell { from: point(node.child("from")?), to: point(node.child("to")?) },
        "oneCellAnchor" => {
            let (width, height) = node.child("ext").map(ext).unwrap_or_default();
            ChartAnchor::OneCell { from: point(node.child("from")?), width, height }
        }
        "absoluteAnchor" => {
            let pos = node.child("pos")?;
            let (width, height) = node.child("ext").map(ext).unwrap_or_default();
            ChartAnchor::Absolute { x: pos.number("x").unwrap_or(0), y: pos.number("y").unwrap_or(0), width, height }
        }
        _ => return None,
    })
}

/// The chart frames of an anchor: its object, or the chosen alternative's.
fn anchor_frame(anchor: &Node, out: &mut Vec<(String, ChartAnchor, String, Option<String>)>) {
    let Some(place) = anchor_of(anchor) else { return };
    for object in &anchor.children {
        let object = match object.local.as_str() {
            "AlternateContent" => match alternate_content_branch(object).and_then(|b| b.children.first()) {
                Some(chosen) => chosen,
                None => continue,
            },
            _ => object,
        };
        if object.local == "graphicFrame" && is_chartex_frame(object) {
            let name = object.child("nvGraphicFramePr").and_then(|n| n.child("cNvPr")).and_then(|c| c.attribute("name")).unwrap_or_default();
            let id = object
                .child("graphic")
                .and_then(|g| g.child("graphicData"))
                .and_then(|d| d.child("chart"))
                .and_then(|c| c.attribute("r:id"));
            if let Some(id) = id {
                out.push((name.to_owned(), place, id.to_owned(), anchor.attribute("editAs").map(str::to_owned)));
            }
        }
    }
}

/// The Excel 2016 chart frames of the drawing part `part`, in its order: each frame's name, anchor and relationship.
pub(super) fn frames(package: &Package, part: &str) -> Vec<(String, ChartAnchor, String, Option<String>)> {
    let Some(root) = part_tree(package, part) else { return Vec::new() };
    let mut out = Vec::new();
    for child in &root.children {
        match child.local.as_str() {
            "AlternateContent" => {
                if let Some(branch) = alternate_content_branch(child) {
                    for anchor in &branch.children {
                        anchor_frame(anchor, &mut out);
                    }
                }
            }
            _ => anchor_frame(child, &mut out),
        }
    }
    out
}

/// The points of a dimension's level `level` (0 the first), by index.
fn level_points(node: &Node, level: usize) -> Vec<Option<String>> {
    let mut out = Vec::new();
    if let Some(level) = node.children.iter().filter(|c| c.local == "lvl").nth(level) {
        let count = level.number("ptCount").and_then(|n| usize::try_from(n).ok()).unwrap_or(0).min(1_000_000);
        out = vec![None; count];
        for pt in level.children.iter().filter(|c| c.local == "pt") {
            if let Some(i) = pt.number("idx").and_then(|i| usize::try_from(i).ok()).filter(|i| *i < 1_000_000) {
                if i >= out.len() {
                    out.resize(i + 1, None);
                }
                out[i] = Some(pt.text.clone());
            }
        }
    }
    out
}

/// A dimension's formula and its first level's points, by index.
fn dimension(node: &Node, numeric: bool) -> ChartData {
    let formula = node.child("f").map(|f| f.text.trim().to_owned());
    let mut cache = Vec::new();
    if let Some(level) = node.child("lvl") {
        let count = level.number("ptCount").and_then(|n| usize::try_from(n).ok()).unwrap_or(0).min(1_000_000);
        cache = vec![None; count];
        for pt in level.children.iter().filter(|c| c.local == "pt") {
            if let Some(i) = pt.number("idx").and_then(|i| usize::try_from(i).ok()) {
                if i >= cache.len() {
                    if i >= 1_000_000 {
                        continue;
                    }
                    cache.resize(i + 1, None);
                }
                cache[i] = Some(pt.text.clone());
            }
        }
    }
    ChartData { formula, cache, numeric }
}

fn text_of(node: &Node) -> String {
    let mut out = node.text.clone();
    for child in &node.children {
        out.push_str(&text_of(child));
    }
    out
}

/// The `cx:chartSpace` part `part` as a chart.
pub(super) fn read(package: &Package, part: &str, name: String, anchor: ChartAnchor) -> Option<Chart> {
    let root = part_tree(package, part)?;
    if root.local != "chartSpace" {
        return None;
    }
    let data = root.child("chartData")?;
    let dimensions = |id: &str| -> Option<(Option<ChartData>, Option<ChartData>, Vec<Vec<Option<String>>>)> {
        let set = data.children.iter().find(|d| d.local == "data" && d.attribute("id") == Some(id))?;
        let category_node = set.children.iter().find(|d| d.local == "strDim" || (d.local == "numDim" && d.attribute("type") == Some("cat")));
        let levels = category_node.map_or(0, |d| d.children.iter().filter(|c| c.local == "lvl").count());
        let outer: Vec<Vec<Option<String>>> = (1..levels).filter_map(|l| category_node.map(|d| level_points(d, l))).collect();
        let mut categories = category_node.map(|d| dimension(d, d.local == "numDim"));
        if levels > 1 {
            // The formula spans every level's cells: the leaves' points as the file cached them stand for them.
            if let Some(c) = categories.as_mut() {
                c.formula = None;
            }
        }
        let values = set
            .children
            .iter()
            .find(|d| d.local == "numDim" && matches!(d.attribute("type"), Some("val" | "size") | None))
            .map(|d| dimension(d, true));
        Some((categories, values, outer))
    };
    let chart_node = root.child("chart")?;
    let title = chart_node.child("title").map(text_of).map(|t| t.trim().to_owned()).filter(|t| !t.is_empty());
    let legend = chart_node.child("legend").map(|l| l.attribute("pos").unwrap_or("r").to_owned());
    let region = chart_node.child("plotArea")?.child("plotAreaRegion")?;
    let mut plot = ChartPlot::default();
    for (index, series) in region.children.iter().filter(|c| c.local == "series").enumerate() {
        let layout = series.attribute("layoutId").unwrap_or("clusteredColumn");
        if plot.kind.is_empty() {
            plot.kind = format!("cx:{layout}");
        }
        if layout == "paretoLine" {
            plot.pareto = true;
            continue;
        }
        let id = series.child("dataId").and_then(|d| d.attribute("val")).unwrap_or("0");
        let (categories, values, outer_levels) = dimensions(id)?;
        let name = series.child("tx").and_then(|t| t.child("txData")).map(|t| ChartData {
            formula: t.child("f").map(|f| f.text.trim().to_owned()),
            cache: vec![t.child("v").map(|v| v.text.clone())],
            numeric: false,
        });
        if let Some(statistics) = series.child("layoutPr").and_then(|l| l.child("statistics")) {
            plot.box_inclusive = statistics.attribute("quartileMethod") == Some("inclusive");
        }
        if let Some(v) = series.child("layoutPr").and_then(|l| l.child("visibility")) {
            let on = |name: &str, default: bool| v.attribute(name).map_or(default, |a| matches!(a, "1" | "true"));
            plot.box_visibility = Some([on("meanMarker", true), on("meanLine", false), on("outliers", true), on("nonoutliers", false)]);
        }
        if let Some(b) = series.child("layoutPr").and_then(|l| l.child("binning")) {
            let val = |name: &str| b.child(name).and_then(|n| n.attribute("val")).map(str::to_owned);
            plot.binning = Some(calc_doc_model::Binning {
                size: val("binSize"),
                count: val("binCount"),
                closed_left: b.attribute("intervalClosed") == Some("l"),
                underflow: val("underflow"),
                overflow: val("overflow"),
            });
        }
        if let Some(subtotals) = series.child("layoutPr").and_then(|l| l.child("subtotals")) {
            plot.subtotals = subtotals.children.iter().filter_map(|i| i.number("val")).filter_map(|v| u32::try_from(v).ok()).collect();
        }
        plot.series.push(ChartSeries { index: index as u32, name, categories, values, outer_levels, ..ChartSeries::default() });
    }
    if plot.series.is_empty() {
        return None;
    }
    Some(Chart {
        edit_as: None,
        part: part.to_owned(),
        name,
        anchor,
        drawing_id: None,
        lang: None,
        unique_next: 0,
        title,
        title_formula: None,
        auto_title_deleted: false,
        legend,
        plots: vec![plot],
        axes: Vec::new(),
        text_style: Default::default(),
        element_text: Default::default(),
        element_text_complex: Default::default(),
    })
}
