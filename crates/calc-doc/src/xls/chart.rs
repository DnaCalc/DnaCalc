//! A chart substream of a BIFF8 worksheet (MS-XLS 2.1.7.20.6): the chart groups (bar, column,
//! line, area, pie, doughnut, scatter), their series with the worksheet ranges of their names,
//! values and categories, the title, axis titles and the legend, written as the DrawingML chart
//! part Excel saves. Bar and pie geometry, point-color variation, and primary linear value-axis
//! bounds and units are retained; other formatting (colours, fonts, gridlines, 3-D, data labels,
//! trendlines) is not read.

use super::records::Record;
use super::strings::Pieces;
use crate::xlsb::Xml;
use std::collections::BTreeMap;

const CHART_NAMESPACE: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const MAIN_NAMESPACE: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const RELATIONSHIPS_NAMESPACE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

const BEGIN: u16 = 0x1033;
const END: u16 = 0x1034;
const SERIES: u16 = 0x1003;
const SERIES_TEXT: u16 = 0x100D;
const AI: u16 = 0x1051;
const CHART_FORMAT: u16 = 0x1014;
const CHART_PROPERTIES: u16 = 0x1044;
const BAR: u16 = 0x1017;
const LINE: u16 = 0x1018;
const PIE: u16 = 0x1019;
const AREA: u16 = 0x101A;
const SCATTER: u16 = 0x101B;
const AXIS: u16 = 0x101D;
const AXIS_PARENT: u16 = 0x1041;
const VALUE_RANGE: u16 = 0x101F;
const LEGEND: u16 = 0x1015;
const TEXT: u16 = 0x1025;
const OBJECT_LINK: u16 = 0x1027;
const SERIES_TO_CHART_GROUP: u16 = 0x1045;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Kind {
    #[default]
    Bar,
    Line,
    Pie,
    Area,
    Scatter,
}

#[derive(Clone, Copy, Debug, Default)]
struct Group {
    kind: Kind,
    horizontal: bool,
    stacked: bool,
    percent: bool,
    doughnut: bool,
    gap: Option<u16>,
    overlap: Option<i16>,
    first_slice: Option<u16>,
    hole: Option<u16>,
    vary_colors: Option<bool>,
}

#[derive(Debug, Default)]
struct Series {
    group: usize,
    title: Option<String>,
    title_formula: Option<String>,
    values: Option<String>,
    categories: Option<String>,
}

#[derive(Clone, Copy, Debug)]
struct ValueRange {
    min: Option<f64>,
    max: Option<f64>,
    major: Option<f64>,
    minor: Option<f64>,
    reversed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Block {
    Series,
    Format,
    Axis,
    Text,
    Legend,
    Other,
    AxisParent,
}

#[derive(Debug, Default)]
pub(crate) struct Chart {
    groups: Vec<Group>,
    series: Vec<Series>,
    legend: bool,
    titles: BTreeMap<u16, String>,
    stack: Vec<u8>,
    pending: Option<u8>,
    link: Option<u16>,
    seen_chart: bool,
    plot_visible_only: Option<bool>,
    blanks: Option<&'static str>,
    primary_axis_group: bool,
    axis_index: Option<usize>,
    value_ranges: [Option<ValueRange>; 2],
    range_seen: [bool; 2],
}

fn block_code(block: Block) -> u8 {
    block as u8
}

fn block_of(code: u8) -> Block {
    match code {
        0 => Block::Series,
        1 => Block::Format,
        2 => Block::Axis,
        3 => Block::Text,
        4 => Block::Legend,
        6 => Block::AxisParent,
        _ => Block::Other,
    }
}

impl Chart {
    /// Reads one record of the chart substream; `print` prints the formula tokens of an AI record
    /// as the range they name.
    pub(crate) fn read(&mut self, record: &Record<'_>, print: &dyn Fn(&[u8]) -> Option<String>) {
        let body = record.body;
        match record.id {
            CHART_PROPERTIES if body.len() >= 3 => {
                self.plot_visible_only = Some(u16::from_le_bytes([body[0], body[1]]) & 2 != 0);
                self.blanks = match body[2] { 0 => Some("gap"), 1 => Some("zero"), 2 => Some("span"), _ => None };
            }
            SERIES => {
                self.series.push(Series {
                    group: 0,
                    ..Default::default()
                });
                self.pending = Some(block_code(Block::Series));
            }
            CHART_FORMAT => {
                self.groups.push(Group {
                    vary_colors: body.get(16..18).map(|b| u16::from_le_bytes([b[0], b[1]]) & 1 != 0),
                    ..Default::default()
                });
                self.pending = Some(block_code(Block::Format));
            }
            AXIS_PARENT if body.len() >= 2 => {
                self.primary_axis_group = u16::from_le_bytes([body[0], body[1]]) == 0;
                self.pending = Some(block_code(Block::AxisParent));
            }
            AXIS => {
                self.axis_index = body.get(..2).and_then(|b| match u16::from_le_bytes([b[0], b[1]]) {
                    0 => Some(0), 1 => Some(1), _ => None,
                });
                self.pending = Some(block_code(Block::Axis));
            }
            VALUE_RANGE if body.len() >= 42 && self.primary_axis_group
                && self.stack.iter().any(|c| block_of(*c) == Block::AxisParent)
                && self.stack.iter().any(|c| block_of(*c) == Block::Axis) => {
                if let Some(index) = self.axis_index {
                    if self.range_seen[index] {
                        self.value_ranges[index] = None;
                    } else {
                        self.range_seen[index] = true;
                        let flags = u16::from_le_bytes([body[40], body[41]]);
                        // A chart extension can specify a different logarithm base.
                        if flags & 0x20 == 0 {
                            let number = |at: usize| f64::from_le_bytes(body[at..at + 8].try_into().expect("eight bytes"));
                            let optional = |at: usize, bit: u16| {
                                let v = number(at);
                                (flags & bit == 0 && v.is_finite()).then_some(v)
                            };
                            let (min, max) = (optional(0, 1), optional(8, 2));
                            if !matches!((min, max), (Some(a), Some(b)) if a >= b) {
                                self.value_ranges[index] = Some(ValueRange {
                                    min, max,
                                    major: optional(16, 4).filter(|v| *v > 0.0),
                                    minor: optional(24, 8).filter(|v| *v > 0.0),
                                    reversed: flags & 0x40 != 0,
                                });
                            }
                        }
                    }
                }
            }
            TEXT => {
                self.pending = Some(block_code(Block::Text));
                self.link = None;
            }
            LEGEND => {
                self.legend = true;
                self.pending = Some(block_code(Block::Legend));
            }
            BEGIN => {
                let code = self.pending.take().unwrap_or(5);
                self.stack.push(code);
            }
            END => {
                if let Some(code) = self.stack.pop() {
                    match block_of(code) {
                        Block::Text => self.link = None,
                        Block::Axis => self.axis_index = None,
                        Block::AxisParent => self.primary_axis_group = false,
                        _ => {}
                    }
                }
            }
            BAR if body.len() >= 6 => {
                if let Some(g) = self.groups.last_mut() {
                    let flags = u16::from_le_bytes([body[4], body[5]]);
                    *g = Group {
                        kind: Kind::Bar,
                        horizontal: flags & 1 != 0,
                        stacked: flags & 2 != 0,
                        percent: flags & 4 != 0,
                        gap: (u16::from_le_bytes([body[2], body[3]]) <= 500)
                            .then(|| u16::from_le_bytes([body[2], body[3]])),
                        overlap: (-100..=100).contains(&i16::from_le_bytes([body[0], body[1]]))
                            .then(|| i16::from_le_bytes([body[0], body[1]])),
                        ..*g
                    };
                }
            }
            LINE | AREA if body.len() >= 2 => {
                if let Some(g) = self.groups.last_mut() {
                    let flags = u16::from_le_bytes([body[0], body[1]]);
                    *g = Group {
                        kind: if record.id == LINE { Kind::Line } else { Kind::Area },
                        horizontal: false,
                        stacked: flags & 1 != 0,
                        percent: flags & 2 != 0,
                        ..*g
                    };
                }
            }
            PIE if body.len() >= 4 => {
                if let Some(g) = self.groups.last_mut() {
                    *g = Group {
                        kind: Kind::Pie,
                        doughnut: u16::from_le_bytes([body[2], body[3]]) != 0,
                        first_slice: (u16::from_le_bytes([body[0], body[1]]) <= 360)
                            .then(|| u16::from_le_bytes([body[0], body[1]])),
                        hole: (10..=90).contains(&u16::from_le_bytes([body[2], body[3]]))
                            .then(|| u16::from_le_bytes([body[2], body[3]])),
                        ..*g
                    };
                }
            }
            SCATTER => {
                if let Some(g) = self.groups.last_mut() {
                    *g = Group {
                        kind: Kind::Scatter,
                        ..*g
                    };
                }
            }
            SERIES_TO_CHART_GROUP if body.len() >= 2 => {
                if let Some(s) = self.series.last_mut() {
                    s.group = usize::from(u16::from_le_bytes([body[0], body[1]]));
                }
            }
            AI if body.len() >= 8 => {
                let id = body[0];
                let reference_type = body[1];
                let count = usize::from(u16::from_le_bytes([body[6], body[7]]));
                let formula = (reference_type == 2 && count > 0)
                    .then(|| body.get(8..8 + count))
                    .flatten()
                    .and_then(print);
                if let Some(s) = self.series.last_mut() {
                    match id {
                        0 => s.title_formula = formula,
                        1 => s.values = formula,
                        2 => s.categories = formula,
                        _ => {}
                    }
                }
            }
            OBJECT_LINK if body.len() >= 2 => {
                self.link = Some(u16::from_le_bytes([body[0], body[1]]));
            }
            SERIES_TEXT => {
                let mut p = Pieces::new(record);
                if p.u16("SeriesText").is_ok()
                    && let Ok(count) = p.u8("SeriesText")
                    && let Ok(flag) = p.u8("SeriesText")
                    && let Ok(units) = p.characters(usize::from(count), flag & 1 != 0, "text")
                {
                    let text = String::from_utf16_lossy(&units);
                    let in_text = self.stack.iter().any(|c| block_of(*c) == Block::Text);
                    match (in_text, self.link) {
                        (true, Some(link)) => {
                            self.titles.insert(link, text);
                        }
                        _ => {
                            if let Some(s) = self.series.last_mut()
                                && s.title.is_none()
                            {
                                s.title = Some(text);
                            }
                        }
                    }
                }
            }
            0x1002 => self.seen_chart = true,
            _ => {}
        }
    }

    /// The chart part, or `None` when the chart has no series this reader writes.
    pub(crate) fn write(&self, _sheet: &str) -> Option<Vec<u8>> {
        if self.series.is_empty() {
            return None;
        }
        let mut x = Xml::new();
        x.open(
            "c:chartSpace",
            &[
                ("xmlns:c", CHART_NAMESPACE.to_owned()),
                ("xmlns:a", MAIN_NAMESPACE.to_owned()),
                ("xmlns:r", RELATIONSHIPS_NAMESPACE.to_owned()),
            ],
        );
        x.open("c:chart", &[]);
        if let Some(title) = self.titles.get(&1) {
            title_element(&mut x, title);
            x.empty("c:autoTitleDeleted", &[("val", "0".to_owned())]);
        } else {
            x.empty("c:autoTitleDeleted", &[("val", "1".to_owned())]);
        }
        x.open("c:plotArea", &[]);
        x.empty("c:layout", &[]);
        let groups = if self.groups.is_empty() {
            vec![Group::default()]
        } else {
            self.groups.iter().map(|g| Group { ..*g }).collect::<Vec<_>>()
        };
        let mut order = 0usize;
        let mut axes = false;
        let mut horizontal = false;
        let mut scatter = false;
        for (index, group) in groups.iter().enumerate() {
            let members: Vec<&Series> = self
                .series
                .iter()
                .filter(|s| s.group == index || (self.groups.is_empty()))
                .collect();
            if members.is_empty() {
                continue;
            }
            let (name, plain_axes) = match group.kind {
                Kind::Bar => ("c:barChart", true),
                Kind::Line => ("c:lineChart", true),
                Kind::Area => ("c:areaChart", true),
                Kind::Pie if group.doughnut => ("c:doughnutChart", false),
                Kind::Pie => ("c:pieChart", false),
                Kind::Scatter => ("c:scatterChart", true),
            };
            x.open(name, &[]);
            match group.kind {
                Kind::Bar => {
                    horizontal |= group.horizontal;
                    x.empty("c:barDir", &[("val", if group.horizontal { "bar" } else { "col" }.to_owned())]);
                    x.empty("c:grouping", &[("val", grouping(group, "clustered").to_owned())]);
                }
                Kind::Line | Kind::Area => {
                    x.empty("c:grouping", &[("val", grouping(group, "standard").to_owned())]);
                }
                Kind::Scatter => {
                    scatter = true;
                    x.empty("c:scatterStyle", &[("val", "lineMarker".to_owned())]);
                }
                Kind::Pie => {}
            }
            let varies = group.vary_colors.unwrap_or(group.kind == Kind::Pie)
                && members.len() == 1 && group.kind != Kind::Area;
            x.empty("c:varyColors", &[("val", u8::from(varies).to_string())]);
            for series in members {
                x.open("c:ser", &[]);
                x.empty("c:idx", &[("val", order.to_string())]);
                x.empty("c:order", &[("val", order.to_string())]);
                order += 1;
                if series.title_formula.is_some() || series.title.is_some() {
                    x.open("c:tx", &[]);
                    match (&series.title_formula, &series.title) {
                        (Some(f), _) => {
                            x.open("c:strRef", &[]);
                            x.element("c:f", &[], f);
                            x.close("c:strRef");
                        }
                        (None, Some(text)) => x.element("c:v", &[], text),
                        _ => {}
                    }
                    x.close("c:tx");
                }
                let (category_tag, value_tag) = if group.kind == Kind::Scatter {
                    ("c:xVal", "c:yVal")
                } else {
                    ("c:cat", "c:val")
                };
                if let Some(f) = &series.categories {
                    x.open(category_tag, &[]);
                    x.open(if group.kind == Kind::Scatter { "c:numRef" } else { "c:strRef" }, &[]);
                    x.element("c:f", &[], f);
                    x.close(if group.kind == Kind::Scatter { "c:numRef" } else { "c:strRef" });
                    x.close(category_tag);
                }
                if let Some(f) = &series.values {
                    x.open(value_tag, &[]);
                    x.open("c:numRef", &[]);
                    x.element("c:f", &[], f);
                    x.close("c:numRef");
                    x.close(value_tag);
                }
                x.close("c:ser");
            }
            if group.kind == Kind::Bar {
                if let Some(gap) = group.gap { x.empty("c:gapWidth", &[("val", gap.to_string())]); }
                if let Some(overlap) = group.overlap { x.empty("c:overlap", &[("val", overlap.to_string())]); }
            } else if group.kind == Kind::Pie {
                if let Some(angle) = group.first_slice { x.empty("c:firstSliceAng", &[("val", angle.to_string())]); }
                if group.doughnut && let Some(hole) = group.hole { x.empty("c:holeSize", &[("val", hole.to_string())]); }
            }
            if plain_axes {
                axes = true;
                x.empty("c:axId", &[("val", "111111111".to_owned())]);
                x.empty("c:axId", &[("val", "222222222".to_owned())]);
            }
            x.close(name);
        }
        if axes {
            let (first, second) = if scatter {
                ("c:valAx", "c:valAx")
            } else {
                ("c:catAx", "c:valAx")
            };
            let (first_pos, second_pos) = if horizontal { ("l", "b") } else { ("b", "l") };
            for (axis_index, (tag, id, cross, pos, title_key)) in [
                (first, "111111111", "222222222", first_pos, 3u16),
                (second, "222222222", "111111111", second_pos, 2u16),
            ].into_iter().enumerate() {
                let range = (tag == "c:valAx").then_some(self.value_ranges[axis_index]).flatten();
                x.open(tag, &[]);
                x.empty("c:axId", &[("val", id.to_owned())]);
                x.open("c:scaling", &[]);
                x.empty("c:orientation", &[("val", if range.is_some_and(|r| r.reversed) { "maxMin" } else { "minMax" }.to_owned())]);
                if let Some(value) = range.and_then(|r| r.max) { x.empty("c:max", &[("val", crate::xlsb::formula::number_text(value))]); }
                if let Some(value) = range.and_then(|r| r.min) { x.empty("c:min", &[("val", crate::xlsb::formula::number_text(value))]); }
                x.close("c:scaling");
                x.empty("c:delete", &[("val", "0".to_owned())]);
                x.empty("c:axPos", &[("val", pos.to_owned())]);
                if let Some(title) = self.titles.get(&title_key) {
                    title_element(&mut x, title);
                }
                x.empty("c:crossAx", &[("val", cross.to_owned())]);
                if let Some(value) = range.and_then(|r| r.major) { x.empty("c:majorUnit", &[("val", crate::xlsb::formula::number_text(value))]); }
                if let Some(value) = range.and_then(|r| r.minor) { x.empty("c:minorUnit", &[("val", crate::xlsb::formula::number_text(value))]); }
                x.close(tag);
            }
        }
        x.close("c:plotArea");
        if self.legend {
            x.open("c:legend", &[]);
            x.empty("c:legendPos", &[("val", "r".to_owned())]);
            x.close("c:legend");
        }
        x.empty("c:plotVisOnly", &[("val", u8::from(self.plot_visible_only.unwrap_or(true)).to_string())]);
        if let Some(blanks) = self.blanks { x.empty("c:dispBlanksAs", &[("val", blanks.to_owned())]); }
        x.close("c:chart");
        x.close("c:chartSpace");
        Some(x.out)
    }
}

fn grouping(group: &Group, plain: &'static str) -> &'static str {
    if group.percent {
        "percentStacked"
    } else if group.stacked {
        "stacked"
    } else {
        plain
    }
}

fn title_element(x: &mut Xml, title: &str) {
    x.open("c:title", &[]);
    x.open("c:tx", &[]);
    x.open("c:rich", &[]);
    x.empty("a:bodyPr", &[]);
    x.empty("a:lstStyle", &[]);
    x.open("a:p", &[]);
    x.open("a:r", &[]);
    x.element("a:t", &[], title);
    x.close("a:r");
    x.close("a:p");
    x.close("c:rich");
    x.close("c:tx");
    x.empty("c:overlay", &[("val", "0".to_owned())]);
    x.close("c:title");
}
