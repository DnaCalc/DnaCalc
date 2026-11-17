//! A chart substream of a BIFF8 worksheet (MS-XLS 2.1.7.20.6): the chart groups (bar, column,
//! line, area, pie, doughnut, scatter), their series with the worksheet ranges of their names,
//! values and categories, the title, axis titles and the legend, written as the DrawingML chart
//! part Excel saves. Bar and pie geometry, point-color variation, and primary linear value-axis
//! bounds and units, unambiguous solid series colors, and simple value-label settings are retained;
//! other formatting (fonts, gridlines, 3-D, most data labels, trendlines) is not read.

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
const TICK: u16 = 0x101E;
const AXIS_LINE: u16 = 0x1021;
const AXIS_NUMBER_FORMAT: u16 = 0x104E;
const SI_INDEX: u16 = 0x1065;
const LEGEND: u16 = 0x1015;
const TEXT: u16 = 0x1025;
const DEFAULT_TEXT: u16 = 0x1024;
const FONT_X: u16 = 0x1026;
const AL_RUNS: u16 = 0x1050;
const OBJECT_LINK: u16 = 0x1027;
const SERIES_TO_CHART_GROUP: u16 = 0x1045;
const DATA_FORMAT: u16 = 0x1006;
const LINE_FORMAT: u16 = 0x1007;
const AREA_FORMAT: u16 = 0x100A;
const ATTACHED_LABEL: u16 = 0x100C;

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
    show_values: bool,
    labels_seen: bool,
}

#[derive(Debug, Default)]
struct Series {
    group: usize,
    title: Option<String>,
    title_formula: Option<String>,
    values: Option<String>,
    value_format_seen: bool,
    value_format_invalid: bool,
    value_format_linked: bool,
    categories: Option<String>,
    category_type: Option<u16>,
    category_count: Option<u16>,
    value_type: Option<u16>,
    value_count: Option<u16>,
    fill: Option<String>,
    fill_seen: bool,
    line: Option<String>,
    line_seen: bool,
    show_values: bool,
    labels_seen: bool,
}

#[derive(Debug, Default)]
struct SeriesFormat {
    series: Option<usize>,
    fill: Option<String>,
    area_seen: bool,
    line: Option<String>,
    line_seen: bool,
    invalid: bool,
    show_values: bool,
    labels_seen: bool,
    group_default: bool,
    labels_header_valid: bool,
}

#[derive(Debug, Default)]
struct ValueCache {
    /// None is an explicitly blank point, not a calculated value.
    points: BTreeMap<u16, Option<f64>>,
    invalid: bool,
}

#[derive(Debug)]
enum CategoryPoint { Number(f64), Text(Vec<u16>), Blank }

#[derive(Debug, Default)]
struct CategoryCache {
    points: BTreeMap<u16, CategoryPoint>,
    invalid: bool,
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
    DataFormat,
}

#[derive(Debug, Default)]
pub(crate) struct Chart {
    groups: Vec<Group>,
    series: Vec<Series>,
    legend: bool,
    titles: BTreeMap<u16, String>,
    title_formula: Option<String>,
    seen_title_links: std::collections::BTreeSet<u16>,
    stack: Vec<u8>,
    pending: Option<u8>,
    link: Option<u16>,
    attached_text: Option<String>,
    attached_formula: Option<String>,
    attached_ai_seen: bool,
    attached_invalid: bool,
    attached_font: Option<(String, bool, bool)>,
    attached_color: Option<String>,
    attached_font_seen: bool,
    attached_font_invalid: bool,
    pending_default_font: Option<usize>,
    active_default_font: Option<usize>,
    default_font_ai_valid: bool,
    default_font_pos_seen: bool,
    default_fonts: [Option<(String, bool, bool)>; 2],
    default_font_seen: [bool; 2],
    default_font_invalid: bool,
    title_font: Option<(String, bool, bool)>,
    title_color: Option<String>,
    axis_title_colors: [Option<String>; 2],
    legend_color: Option<String>,
    axis_title_fonts: [Option<(String, bool, bool)>; 2],
    axis_fonts: [Option<(String, bool, bool)>; 2],
    axis_font_seen: [bool; 2],
    axis_font_invalid: [bool; 2],
    primary_axis_seen: [bool; 2],
    tick_seen: [bool; 2],
    tick_invalid: [bool; 2],
    tick_labels_hidden: [bool; 2],
    pending_axis_line: Option<usize>,
    axis_line_seen: [bool; 2],
    axis_line_invalid: [bool; 2],
    axis_line_hidden: [bool; 2],
    axis_line_color: [Option<String>; 2],
    axis_format_seen: [bool; 2],
    axis_format_invalid: [bool; 2],
    axis_formats: [Option<String>; 2],
    pending_major_gridline: Option<usize>,
    major_gridline_seen: [bool; 2],
    major_gridline_invalid: [bool; 2],
    major_gridline_color: [Option<String>; 2],
    primary_axis_parent_valid: bool,
    legend_font: Option<(String, bool, bool)>,
    legend_text_seen: bool,
    legend_font_invalid: bool,
    seen_chart: bool,
    plot_visible_only: Option<bool>,
    blanks: Option<&'static str>,
    primary_axis_group: bool,
    axis_index: Option<usize>,
    value_ranges: [Option<ValueRange>; 2],
    range_seen: [bool; 2],
    cache_section: u16,
    cache_stage: u16,
    cache_invalid: bool,
    value_caches: BTreeMap<usize, ValueCache>,
    category_caches: BTreeMap<usize, CategoryCache>,
    data_format: Option<SeriesFormat>,
    auxiliary_series: bool,
    data_labels_ambiguous: bool,
    label_format_ambiguous: bool,
    print_header: Option<Vec<u16>>,
    print_footer: Option<Vec<u16>>,
    print_header_seen: bool,
    print_footer_seen: bool,
    print_text_invalid: bool,
    print_setup: Option<[u8; 34]>,
    print_setup_seen: bool,
    print_setup_invalid: bool,
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
        7 => Block::DataFormat,
        _ => Block::Other,
    }
}

impl Chart {
    /// Reads one record of the chart substream; `print` prints the formula tokens of an AI record
    /// as the range they name.
    pub(crate) fn read(&mut self, record: &Record<'_>, print: &dyn Fn(&[u8]) -> Option<String>,
        code_page: Option<u16>, styles: &super::styles::Styles, embedded: bool) {
        let body = record.body;
        if let Some(index) = self.pending_axis_line && record.id != LINE_FORMAT {
            self.pending_axis_line = None;
            self.axis_line_invalid[index] = true;
        }
        if let Some(index) = self.pending_major_gridline && record.id != LINE_FORMAT {
            self.pending_major_gridline = None;
            self.major_gridline_invalid[index] = true;
        }
        if record.id == DEFAULT_TEXT && self.stack.len() != 1 {
            // Group-level default labels can carry their own number format.
            self.label_format_ambiguous = true;
        }
        if self.pending_default_font.is_some() && record.id != TEXT {
            self.pending_default_font = None;
            self.default_font_invalid = true;
        }
        if self.stack.len() == 1 && matches!(record.id, 0x08A5 | 0x08A6) {
            self.default_font_invalid = true;
        }
        if embedded && record.id == 0x01AA { self.print_setup_invalid = true; }
        if embedded && self.stack.is_empty() && record.id == 0x00A1 {
            if self.print_setup_seen { self.print_setup_invalid = true; }
            self.print_setup_seen = true;
            self.print_setup = if record.continues.is_empty() { body.try_into().ok() } else { None };
            if self.print_setup.is_none() { self.print_setup_invalid = true; }
        }
        if embedded && matches!(record.id, 0x0866 | 0x089C) { self.print_text_invalid = true; }
        if embedded && self.stack.is_empty() && matches!(record.id, 0x0014 | 0x0015) {
            let (seen, value) = if record.id == 0x0014 {
                (&mut self.print_header_seen, &mut self.print_header)
            } else { (&mut self.print_footer_seen, &mut self.print_footer) };
            if *seen { self.print_text_invalid = true; }
            *seen = true;
            let parsed = if body.is_empty() && record.continues.is_empty() { Some(Vec::new()) }
            else if code_page.is_none() && record.continues.is_empty() {
                let mut p = Pieces::new(record);
                p.u16("chart print text length").ok().and_then(|count| {
                    let flags = p.u8("chart print text flags").ok()?;
                    if flags & !1 != 0 { return None; }
                    let text = p.characters(usize::from(count), flags & 1 != 0, "chart print text").ok()?;
                    p.done().then_some(text)
                })
            } else { None };
            if parsed.as_ref().is_none_or(|text| text.windows(2).any(|w| w == [38, 71] || w == [38, 103])) {
                self.print_text_invalid = true;
            }
            *value = parsed;
        }
        if self.stack.iter().any(|code| block_of(*code) == Block::DataFormat)
            && (record.id == 0x1066 || (0x0800..=0x08ff).contains(&record.id))
            && let Some(format) = &mut self.data_format { format.invalid = true; }
        match record.id {
            0x104B | 0x105B => self.auxiliary_series = true,
            DATA_FORMAT if body.len() == 8 && record.continues.is_empty() => {
                let point = u16::from_le_bytes([body[0], body[1]]);
                let series = usize::from(u16::from_le_bytes([body[2], body[3]]));
                self.data_format = Some(SeriesFormat { series: (point == 0xffff).then_some(series),
                    group_default: body == [0, 0, 0, 0, 0xfd, 0xff, 0, 0],
                    labels_header_valid: body[6..8] == [0, 0], ..Default::default() });
                self.pending = Some(block_code(Block::DataFormat));
            }
            ATTACHED_LABEL => {
                let in_format = self.stack.last().is_some_and(|code| block_of(*code) == Block::DataFormat);
                let in_series = self.stack.iter().any(|code| block_of(*code) == Block::Series);
                let in_group = self.stack.iter().any(|code| block_of(*code) == Block::Format);
                if in_format && in_series {
                    if let Some(format) = &mut self.data_format {
                        if !format.labels_header_valid || format.series.is_none_or(|index| index >= self.series.len()) {
                            self.data_labels_ambiguous = true;
                        }
                        if format.labels_seen { self.data_labels_ambiguous = true; }
                        format.labels_seen = true;
                        format.show_values = body == [1, 0] && record.continues.is_empty();
                        if !format.show_values { self.data_labels_ambiguous = true; }
                    }
                } else if in_format && in_group && let Some(group) = self.groups.last_mut() {
                    if group.labels_seen || body != [1, 0] || !record.continues.is_empty()
                        || !self.data_format.as_ref().is_some_and(|format| format.group_default && !format.invalid) {
                        self.data_labels_ambiguous = true;
                    } else { group.show_values = true; }
                    group.labels_seen = true;
                } else { self.data_labels_ambiguous = true; }
            }
            0x086A | 0x086B => self.data_labels_ambiguous = true,
            AREA_FORMAT if self.stack.last().is_some_and(|code| block_of(*code) == Block::DataFormat) => {
                if let Some(format) = &mut self.data_format {
                    if format.area_seen { format.invalid = true; }
                    format.area_seen = true;
                    if body.len() == 16 && record.continues.is_empty() && body[3] == 0
                        && u16::from_le_bytes([body[8], body[9]]) == 1
                        && u16::from_le_bytes([body[10], body[11]]) == 0 {
                        let index = u16::from_le_bytes([body[12], body[13]]);
                        let rgb = format!("{:02X}{:02X}{:02X}", body[0], body[1], body[2]);
                        if styles.color_attributes(index).first().is_some_and(|(kind, value)|
                            *kind == "rgb" && value.strip_prefix("FF") == Some(rgb.as_str())) {
                            format.fill = Some(rgb);
                        }
                    }
                }
            }
            LINE_FORMAT if self.stack.last().is_some_and(|code| block_of(*code) == Block::DataFormat) => {
                if let Some(format) = &mut self.data_format {
                    if format.line_seen { format.invalid = true; }
                    format.line_seen = true;
                    // Explicit solid narrow line, with the RGB and indexed palette in agreement.
                    if body.len() == 12 && record.continues.is_empty() && body[3] == 0
                        && u16::from_le_bytes([body[4], body[5]]) == 0
                        && u16::from_le_bytes([body[6], body[7]]) == 0
                        && u16::from_le_bytes([body[8], body[9]]) == 0 {
                        let index = u16::from_le_bytes([body[10], body[11]]);
                        let rgb = format!("{:02X}{:02X}{:02X}", body[0], body[1], body[2]);
                        if styles.color_attributes(index).first().is_some_and(|(kind, value)|
                            *kind == "rgb" && value.strip_prefix("FF") == Some(rgb.as_str())) {
                            format.line = Some(rgb);
                        }
                    }
                }
            }
            CHART_PROPERTIES if body.len() >= 3 => {
                self.plot_visible_only = Some(u16::from_le_bytes([body[0], body[1]]) & 2 != 0);
                self.blanks = match body[2] { 0 => Some("gap"), 1 => Some("zero"), 2 => Some("span"), _ => None };
            }
            SERIES => {
                self.series.push(Series {
                    group: 0,
                    category_type: body.get(0..2).map(|b| u16::from_le_bytes([b[0], b[1]])),
                    category_count: body.get(4..6).map(|b| u16::from_le_bytes([b[0], b[1]])),
                    value_type: body.get(2..4).map(|b| u16::from_le_bytes([b[0], b[1]])),
                    value_count: body.get(6..8).map(|b| u16::from_le_bytes([b[0], b[1]])),
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
            SI_INDEX => {
                let next = (body.len() == 2 && record.continues.is_empty())
                    .then(|| u16::from_le_bytes([body[0], body[1]]));
                if next.is_none_or(|n| !(1..=3).contains(&n) || self.cache_stage & (1 << n) != 0) {
                    self.cache_invalid = true;
                    self.cache_section = 0;
                } else {
                    self.cache_section = next.unwrap_or(0);
                    self.cache_stage |= 1 << self.cache_section;
                }
            }
            0x0201 | 0x0203 | 0x0204 | 0x0205 if self.cache_section == 1 => {
                if body.len() < 6 { self.cache_invalid = true; }
                else {
                    let row = u16::from_le_bytes([body[0], body[1]]);
                    let column = usize::from(u16::from_le_bytes([body[2], body[3]]));
                    if let Some(series) = self.series.get(column) {
                        let cache = self.value_caches.entry(column).or_default();
                        let value = match record.id {
                            0x0201 if body.len() == 6 && record.continues.is_empty() => Some(None),
                            0x0203 if body.len() == 14 && record.continues.is_empty() => {
                                let n = f64::from_le_bytes(body[6..14].try_into().expect("eight bytes"));
                                n.is_finite().then_some(Some(n))
                            }
                            _ => None,
                        };
                        if series.value_type != Some(1) || series.value_count.is_none_or(|count| row >= count)
                            || value.is_none() || cache.points.contains_key(&row) {
                            cache.invalid = true;
                        } else if let Some(value) = value { cache.points.insert(row, value); }
                    } else { self.cache_invalid = true; }
                }
            }
            0x0201 | 0x0203 | 0x0204 | 0x0205 if self.cache_section == 2 => {
                if body.len() < 6 { self.cache_invalid = true; }
                else {
                    let row = u16::from_le_bytes([body[0], body[1]]);
                    let column = usize::from(u16::from_le_bytes([body[2], body[3]]));
                    if let Some(series) = self.series.get(column) {
                        let cache = self.category_caches.entry(column).or_default();
                        let value = if !record.continues.is_empty() { None } else { match record.id {
                            0x0201 if body.len() == 6 && matches!(series.category_type, Some(1 | 3)) => Some(CategoryPoint::Blank),
                            0x0203 if body.len() == 14 && series.category_type == Some(1) => {
                                let n = f64::from_le_bytes(body[6..14].try_into().expect("eight bytes"));
                                n.is_finite().then_some(CategoryPoint::Number(n))
                            }
                            0x0204 if body.len() >= 9 && body[8] & !1 == 0 && series.category_type == Some(3) => {
                                let mut pieces = Pieces::new(record);
                                pieces.skip(6, "chart category").ok().and_then(|_| pieces.unicode_string("chart category").ok())
                                    .filter(|_| pieces.done()).map(CategoryPoint::Text)
                            }
                            _ => None,
                        }};
                        if series.category_count.is_none_or(|count| row >= count) || value.is_none() || cache.points.contains_key(&row) {
                            cache.invalid = true;
                        } else if let Some(value) = value { cache.points.insert(row, value); }
                    } else { self.cache_invalid = true; }
                }
            }
            AXIS_PARENT if body.len() >= 2 => {
                self.primary_axis_group = u16::from_le_bytes([body[0], body[1]]) == 0;
                self.primary_axis_parent_valid = body.len() == 18 && record.continues.is_empty();
                self.pending = Some(block_code(Block::AxisParent));
            }
            AXIS => {
                self.axis_index = body.get(..2).and_then(|b| match u16::from_le_bytes([b[0], b[1]]) {
                    0 => Some(0), 1 => Some(1), _ => None,
                });
                if self.primary_axis_group && self.stack.iter().any(|c| block_of(*c) == Block::AxisParent)
                    && let Some(index) = self.axis_index {
                    if body.len() != 18 || !record.continues.is_empty() || !self.primary_axis_parent_valid {
                        self.axis_font_invalid[index] = true;
                        self.tick_invalid[index] = true;
                        self.axis_line_invalid[index] = true;
                        self.axis_format_invalid[index] = true;
                        self.major_gridline_invalid[index] = true;
                    }
                    if self.primary_axis_seen[index] { self.axis_font_invalid[index] = true; self.tick_invalid[index] = true; self.axis_line_invalid[index] = true; self.axis_format_invalid[index] = true; self.major_gridline_invalid[index] = true; }
                    self.primary_axis_seen[index] = true;
                }
                self.pending = Some(block_code(Block::Axis));
            }
            TICK if self.primary_axis_group && self.stack.last().is_some_and(|code| block_of(*code) == Block::Axis) => {
                if let Some(index) = self.axis_index {
                    if self.tick_seen[index] { self.tick_invalid[index] = true; }
                    self.tick_seen[index] = true;
                    if code_page.is_none() && self.primary_axis_parent_valid && body.len() == 30
                        && record.continues.is_empty() && body[0] <= 3 && body[1] <= 3 && body[2] <= 3 {
                        self.tick_labels_hidden[index] = body[2] == 0;
                    } else { self.tick_invalid[index] = true; }
                }
            }
            AXIS_NUMBER_FORMAT if self.primary_axis_group && self.stack.last().is_some_and(|code| block_of(*code) == Block::Axis) => {
                if let Some(index) = self.axis_index {
                    if self.axis_format_seen[index] { self.axis_format_invalid[index] = true; }
                    self.axis_format_seen[index] = true;
                    if code_page.is_none() && self.primary_axis_parent_valid && body.len() == 2 && record.continues.is_empty() {
                        self.axis_formats[index] = styles.simple_axis_format(u16::from_le_bytes([body[0], body[1]])).map(str::to_owned);
                    } else { self.axis_format_invalid[index] = true; }
                }
            }
            AXIS_LINE if self.primary_axis_group && self.stack.last().is_some_and(|code| block_of(*code) == Block::Axis) => {
                if let Some(index) = self.axis_index {
                    if body.len() != 2 || !record.continues.is_empty() || !self.primary_axis_parent_valid {
                        self.axis_line_invalid[index] = true;
                        self.major_gridline_invalid[index] = true;
                    } else if body == [0, 0] {
                        if self.axis_line_seen[index] { self.axis_line_invalid[index] = true; }
                        self.axis_line_seen[index] = true;
                        self.pending_axis_line = Some(index);
                    } else if body == [1, 0] {
                        if self.major_gridline_seen[index] { self.major_gridline_invalid[index] = true; }
                        self.major_gridline_seen[index] = true;
                        self.pending_major_gridline = Some(index);
                    }
                }
            }
            LINE_FORMAT if self.pending_axis_line.is_some() => {
                if let Some(index) = self.pending_axis_line.take() {
                    if code_page.is_none() && record.continues.is_empty() && body.len() == 12 {
                        self.axis_line_hidden[index] = body[4..6] == [5, 0] && body[6..8] == [255, 255]
                            && body[8..10] == [8, 0] && body[10..12] == [0x4d, 0];
                        if body[3] == 0 && body[4..8] == [0, 0, 255, 255]
                            && matches!(body[8..10], [0, 0] | [4, 0]) {
                            let rgb = format!("FF{:02X}{:02X}{:02X}", body[0], body[1], body[2]);
                            self.axis_line_color[index] = styles.rgb(u16::from_le_bytes([body[10], body[11]]))
                                .filter(|palette| palette == &rgb).map(|_| rgb[2..].to_owned());
                        }
                    } else { self.axis_line_invalid[index] = true; }
                }
            }
            LINE_FORMAT if self.pending_major_gridline.is_some() => {
                if let Some(index) = self.pending_major_gridline.take() {
                    if code_page.is_none() && record.continues.is_empty() && body.len() == 12
                        && body[3] == 0 && body[4..10] == [0, 0, 255, 255, 0, 0] {
                        let rgb = format!("FF{:02X}{:02X}{:02X}", body[0], body[1], body[2]);
                        self.major_gridline_color[index] = styles.rgb(u16::from_le_bytes([body[10], body[11]]))
                            .filter(|palette| palette == &rgb).map(|_| rgb[2..].to_owned());
                    } else { self.major_gridline_invalid[index] = true; }
                }
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
            DEFAULT_TEXT if self.stack.len() == 1 => {
                if body.len() != 2 || !record.continues.is_empty() {
                    self.default_font_invalid = true;
                } else {
                    self.pending_default_font = match u16::from_le_bytes([body[0], body[1]]) {
                        2 => Some(0), 3 => Some(1),
                        _ => { self.default_font_invalid = true; None }
                    };
                }
            }
            TEXT => {
                self.pending = Some(block_code(Block::Text));
                self.active_default_font = self.pending_default_font.take();
                self.default_font_ai_valid = false;
                self.default_font_pos_seen = false;
                self.link = None;
                self.attached_text = None;
                self.attached_formula = None;
                self.attached_ai_seen = false;
                self.attached_invalid = false;
                self.attached_font = None;
                // MS-XLS 2.4.324: rgbText and icvText describe the same explicit colour.
                // Automatic colours, malformed/local records and palette disagreements stay unread.
                self.attached_color = if code_page.is_none() && body.len() == 32 && record.continues.is_empty()
                    && body[7] == 0 && u16::from_le_bytes([body[24], body[25]]) & 0x41 == 0 {
                    let rgb = format!("{:02X}{:02X}{:02X}", body[4], body[5], body[6]);
                    let index = u16::from_le_bytes([body[26], body[27]]);
                    styles.color_attributes(index).first().is_some_and(|(kind, value)|
                        *kind == "rgb" && value.strip_prefix("FF") == Some(rgb.as_str())).then_some(rgb)
                } else { None };
                self.attached_font_seen = false;
                self.attached_font_invalid = body.len() != 32 || !record.continues.is_empty();
            }
            0x104F if self.active_default_font.is_some()
                && self.stack.last().is_some_and(|code| block_of(*code) == Block::Text) => {
                if self.default_font_pos_seen || self.attached_font_seen || self.attached_ai_seen
                    || body.len() != 20 || !record.continues.is_empty() {
                    self.default_font_invalid = true;
                }
                self.default_font_pos_seen = true;
            }
            FONT_X if self.stack.last().is_some_and(|code| block_of(*code) == Block::Text) => {
                if self.active_default_font.is_some() && (!self.default_font_pos_seen || self.attached_ai_seen) {
                    self.default_font_invalid = true;
                }
                if self.attached_font_seen { self.attached_font_invalid = true; }
                self.attached_font_seen = true;
                self.attached_font = if code_page.is_none() && record.continues.is_empty() && body.len() == 2 {
                    styles.chart_font(u16::from_le_bytes([body[0], body[1]]))
                } else { None };
                if self.attached_font.is_none() { self.attached_color = None; }
            }
            FONT_X if self.primary_axis_group && self.stack.last().is_some_and(|code| block_of(*code) == Block::Axis) => {
                if let Some(index) = self.axis_index {
                    if self.axis_font_seen[index] { self.axis_font_invalid[index] = true; }
                    self.axis_font_seen[index] = true;
                    self.axis_fonts[index] = if code_page.is_none() && record.continues.is_empty() && body.len() == 2 {
                        styles.chart_font(u16::from_le_bytes([body[0], body[1]]))
                    } else { None };
                }
            }
            AL_RUNS | 0x08A5 | 0x08A6 if self.stack.last().is_some_and(|code| block_of(*code) == Block::Text) => {
                self.attached_font_invalid = true;
            }
            AL_RUNS | 0x08A5 | 0x08A6 if self.primary_axis_group && self.stack.last().is_some_and(|code| block_of(*code) == Block::Axis) => {
                if let Some(index) = self.axis_index { self.axis_font_invalid[index] = true; }
            }
            AL_RUNS | 0x08A5 | 0x08A6 if self.stack.last().is_some_and(|code| block_of(*code) == Block::Legend) => {
                self.legend_font_invalid = true;
            }
            LEGEND => {
                if self.legend || body.len() != 20 || !record.continues.is_empty() { self.legend_font_invalid = true; }
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
                        Block::Text => {
                            if let Some(index) = self.active_default_font.take() {
                                if self.stack.len() != 1 || self.default_font_seen[index]
                                    || self.attached_invalid || self.attached_font_invalid || self.link.is_some()
                                    || self.attached_font.is_none() || !self.default_font_ai_valid
                                    || !self.default_font_pos_seen {
                                    self.default_font_invalid = true;
                                } else {
                                    self.default_fonts[index] = self.attached_font.take();
                                }
                                self.default_font_seen[index] = true;
                            }
                            if self.stack.last().is_some_and(|code| block_of(*code) == Block::Legend) {
                                if self.legend_text_seen || self.link.is_some() || self.attached_invalid || self.attached_font_invalid {
                                    self.legend_font_invalid = true;
                                } else {
                                    self.legend_font = self.attached_font.take();
                                    self.legend_color = self.attached_color.clone();
                                }
                                self.legend_text_seen = true;
                            }
                            if let Some(link) = self.link {
                                if link == 1 {
                                    self.title_color = (!self.seen_title_links.contains(&link)
                                        && !self.attached_invalid && !self.attached_font_invalid)
                                        .then(|| self.attached_color.clone()).flatten();
                                    self.title_font = if !self.seen_title_links.contains(&link)
                                        && !self.attached_invalid && !self.attached_font_invalid {
                                        self.attached_font.take()
                                    } else { None };
                                }
                                if (2..=3).contains(&link) {
                                    self.axis_title_colors[usize::from(link - 2)] = (!self.seen_title_links.contains(&link)
                                        && !self.attached_invalid && !self.attached_font_invalid)
                                        .then(|| self.attached_color.clone()).flatten();
                                    self.axis_title_fonts[usize::from(link - 2)] = if !self.seen_title_links.contains(&link)
                                        && !self.attached_invalid && !self.attached_font_invalid {
                                        self.attached_font.take()
                                    } else { None };
                                }
                                if !self.seen_title_links.insert(link) {
                                    self.titles.remove(&link);
                                    if link == 1 { self.title_formula = None; }
                                } else if !self.attached_invalid && let Some(formula) = self.attached_formula.take() {
                                    if link == 1 {
                                        self.title_formula = Some(formula);
                                        if let Some(text) = self.attached_text.take() { self.titles.insert(link, text); }
                                    }
                                } else if !self.attached_invalid && let Some(text) = self.attached_text.take() {
                                    self.titles.insert(link, text);
                                }
                            }
                            self.link = None;
                            self.attached_text = None;
                            self.attached_formula = None;
                        }
                        Block::Axis => self.axis_index = None,
                        Block::AxisParent => { self.primary_axis_group = false; self.primary_axis_parent_valid = false; }
                        Block::DataFormat => {
                            if let Some(format) = self.data_format.take()
                                && let Some(index) = format.series
                                && let Some(series) = self.series.get_mut(index) {
                                if series.fill_seen { series.fill = None; }
                                else if !format.invalid { series.fill = format.fill; }
                                series.fill_seen = true;
                                if series.line_seen { series.line = None; }
                                else if !format.invalid { series.line = format.line; }
                                series.line_seen = true;
                                if format.labels_seen {
                                    if series.labels_seen { series.show_values = false; self.data_labels_ambiguous = true; }
                                    else if !format.invalid { series.show_values = format.show_values; }
                                    series.labels_seen = true;
                                }
                            }
                        }
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
                if self.stack.iter().any(|c| block_of(*c) == Block::Text) {
                    if self.active_default_font.is_some() {
                        self.default_font_ai_valid = body.len() == 8 && record.continues.is_empty()
                            && id == 0 && reference_type == 1 && count == 0
                            && self.default_font_pos_seen && self.attached_font_seen;
                        if u16::from_le_bytes([body[2], body[3]]) & 1 != 0 {
                            self.label_format_ambiguous = true;
                        }
                    }
                    if self.attached_ai_seen { self.attached_invalid = true; }
                    self.attached_ai_seen = true;
                    if reference_type == 2 && count > 0 {
                        if id == 0 && formula.is_some() { self.attached_formula = formula; }
                        else { self.attached_invalid = true; }
                    }
                } else if let Some(s) = self.series.last_mut() {
                    match id {
                        0 => s.title_formula = formula,
                        1 => {
                            if s.value_format_seen { s.value_format_invalid = true; }
                            s.value_format_seen = true;
                            s.value_format_linked = !s.value_format_invalid && reference_type == 2
                                && formula.is_some() && record.continues.is_empty() && body.len() == 8 + count
                                && u16::from_le_bytes([body[2], body[3]]) == 0;
                            s.values = formula;
                        }
                        2 => s.categories = formula,
                        _ => {}
                    }
                }
            }
            OBJECT_LINK if body.len() >= 2 => {
                if self.stack.iter().any(|c| block_of(*c) == Block::Text) {
                    if self.link.is_some() { self.attached_invalid = true; }
                    self.link = Some(u16::from_le_bytes([body[0], body[1]]));
                    if self.link == Some(4) { self.data_labels_ambiguous = true; }
                }
            }
            SERIES_TEXT => {
                let text = if let Some(page) = code_page {
                    body.get(2).and_then(|count| body.get(3..3 + usize::from(*count)))
                        .filter(|bytes| record.continues.is_empty() && body.len() == 3 + bytes.len())
                        .and_then(|bytes| super::codepage::decode(page, bytes).ok())
                } else {
                    let mut p = Pieces::new(record);
                    p.u16("SeriesText").ok().and_then(|_| p.u8("SeriesText").ok())
                        .and_then(|count| p.u8("SeriesText").ok().and_then(|flag|
                            p.characters(usize::from(count), flag & 1 != 0, "text").ok()))
                        .map(|units| String::from_utf16_lossy(&units))
                };
                if let Some(text) = text {
                    if self.stack.iter().any(|c| block_of(*c) == Block::Text) {
                        if self.attached_text.is_some() { self.attached_invalid = true; }
                        self.attached_text = Some(text);
                    } else if let Some(s) = self.series.last_mut() && s.title.is_none() {
                        s.title = Some(text);
                    }
                } else if self.stack.iter().any(|c| block_of(*c) == Block::Text) {
                    self.attached_invalid = true;
                }
            }
            0x1002 => self.seen_chart = true,
            _ => {}
        }
    }

    /// Avoid inventing an empty or partial chart when an old series source could not be decoded.
    pub(crate) fn has_all_value_sources(&self) -> bool {
        !self.series.is_empty() && self.series.iter().all(|series| series.values.is_some())
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
        if self.title_formula.is_some() || self.titles.contains_key(&1) {
            if let Some(formula) = &self.title_formula {
                linked_title_element(&mut x, formula, self.titles.get(&1).map(String::as_str),
                    self.title_font.as_ref(), self.title_color.as_deref());
            } else if let Some(title) = self.titles.get(&1) {
                title_element(&mut x, title, self.title_font.as_ref(), self.title_color.as_deref());
            }
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
            let members: Vec<(usize, &Series)> = self
                .series
                .iter()
                .enumerate()
                .filter(|(_, s)| s.group == index || (self.groups.is_empty()))
                .collect();
            if members.is_empty() {
                continue;
            }
            let group_source_linked = !self.label_format_ambiguous && !self.default_font_invalid
                && members.iter().all(|(_, series)| series.value_format_linked);
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
            for (series_index, series) in members {
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
                if !self.auxiliary_series && matches!(group.kind, Kind::Bar | Kind::Pie | Kind::Area)
                    && let Some(fill) = &series.fill {
                    x.open("c:spPr", &[]); x.open("a:solidFill", &[]);
                    x.empty("a:srgbClr", &[("val", fill.clone())]);
                    x.close("a:solidFill"); x.close("c:spPr");
                }
                if !self.auxiliary_series && matches!(group.kind, Kind::Line | Kind::Scatter)
                    && let Some(color) = &series.line {
                    x.open("c:spPr", &[]); x.open("a:ln", &[]); x.open("a:solidFill", &[]);
                    x.empty("a:srgbClr", &[("val", color.clone())]);
                    x.close("a:solidFill"); x.close("a:ln"); x.close("c:spPr");
                }
                if !self.data_labels_ambiguous && !self.auxiliary_series && series.show_values && !group.show_values {
                    value_labels_element(&mut x, !self.label_format_ambiguous && !self.default_font_invalid && series.value_format_linked);
                }
                let (category_tag, value_tag) = if group.kind == Kind::Scatter {
                    ("c:xVal", "c:yVal")
                } else {
                    ("c:cat", "c:val")
                };
                if let Some(f) = &series.categories {
                    let cache = self.category_caches.get(&series_index).filter(|cache|
                        !self.cache_invalid && self.cache_stage == 0b1110 && !cache.invalid && !cache.points.is_empty());
                    let numeric = group.kind == Kind::Scatter || (cache.is_some() && series.category_type == Some(1));
                    x.open(category_tag, &[]);
                    x.open(if numeric { "c:numRef" } else { "c:strRef" }, &[]);
                    x.element("c:f", &[], f);
                    if let (Some(count), Some(cache)) = (series.category_count, cache)
                        && (series.category_type == Some(1) || (!numeric && series.category_type == Some(3)))
                    {
                        let cache_tag = if numeric { "c:numCache" } else { "c:strCache" };
                        x.open(cache_tag, &[]);
                        x.empty("c:ptCount", &[("val", count.to_string())]);
                        for (index, point) in &cache.points {
                            if !matches!(point, CategoryPoint::Blank) {
                                x.open("c:pt", &[("idx", index.to_string())]);
                                x.open("c:v", &[]);
                                match point {
                                    CategoryPoint::Number(n) => x.text_str(&crate::xlsb::formula::number_text(*n)),
                                    CategoryPoint::Text(text) => x.text(text),
                                    CategoryPoint::Blank => {},
                                }
                                x.close("c:v"); x.close("c:pt");
                            }
                        }
                        x.close(cache_tag);
                    }
                    x.close(if numeric { "c:numRef" } else { "c:strRef" });
                    x.close(category_tag);
                }
                if let Some(f) = &series.values {
                    x.open(value_tag, &[]);
                    x.open("c:numRef", &[]);
                    x.element("c:f", &[], f);
                    if !self.cache_invalid && self.cache_stage == 0b1110
                        && let (Some(count), Some(cache)) = (series.value_count, self.value_caches.get(&series_index))
                        && !cache.invalid && !cache.points.is_empty()
                    {
                        x.open("c:numCache", &[]);
                        x.empty("c:ptCount", &[("val", count.to_string())]);
                        for (index, value) in &cache.points {
                            if let Some(value) = value {
                                x.open("c:pt", &[("idx", index.to_string())]);
                                x.element("c:v", &[], &crate::xlsb::formula::number_text(*value));
                                x.close("c:pt");
                            }
                        }
                        x.close("c:numCache");
                    }
                    x.close("c:numRef");
                    x.close(value_tag);
                }
                x.close("c:ser");
            }
            if !self.data_labels_ambiguous && !self.auxiliary_series && group.show_values {
                value_labels_element(&mut x, group_source_linked);
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
                if tag == "c:valAx" && !self.major_gridline_invalid[axis_index]
                    && let Some(color) = self.major_gridline_color[axis_index].as_ref() {
                    x.open("c:majorGridlines", &[]); x.open("c:spPr", &[]); x.open("a:ln", &[]);
                    x.open("a:solidFill", &[]); x.empty("a:srgbClr", &[("val", color.clone())]);
                    x.close("a:solidFill"); x.close("a:ln"); x.close("c:spPr"); x.close("c:majorGridlines");
                }
                if let Some(title) = self.titles.get(&title_key) {
                    title_element(&mut x, title, self.axis_title_fonts[usize::from(title_key - 2)].as_ref(),
                        self.axis_title_colors[usize::from(title_key - 2)].as_deref());
                }
                if tag == "c:valAx" && !self.axis_format_invalid[axis_index]
                    && let Some(code) = self.axis_formats[axis_index].as_ref() {
                    x.empty("c:numFmt", &[("formatCode", code.clone()), ("sourceLinked", "0".to_owned())]);
                }
                if !self.tick_invalid[axis_index] && self.tick_labels_hidden[axis_index] {
                    x.empty("c:tickLblPos", &[("val", "none".to_owned())]);
                }
                if !self.axis_line_invalid[axis_index] && self.axis_line_hidden[axis_index] {
                    x.open("c:spPr", &[]); x.open("a:ln", &[]); x.empty("a:noFill", &[]); x.close("a:ln"); x.close("c:spPr");
                } else if !self.axis_line_invalid[axis_index]
                    && let Some(color) = self.axis_line_color[axis_index].as_ref() {
                    x.open("c:spPr", &[]); x.open("a:ln", &[]); x.open("a:solidFill", &[]);
                    x.empty("a:srgbClr", &[("val", color.clone())]);
                    x.close("a:solidFill"); x.close("a:ln"); x.close("c:spPr");
                }
                text_font_element(&mut x, if self.axis_font_invalid[axis_index] { None } else { self.axis_fonts[axis_index].as_ref() }, None);
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
            text_font_element(&mut x, if self.legend_font_invalid { None } else { self.legend_font.as_ref() },
                if self.legend_font_invalid { None } else { self.legend_color.as_deref() });
            x.close("c:legend");
        }
        x.empty("c:plotVisOnly", &[("val", u8::from(self.plot_visible_only.unwrap_or(true)).to_string())]);
        if let Some(blanks) = self.blanks { x.empty("c:dispBlanksAs", &[("val", blanks.to_owned())]); }
        x.close("c:chart");
        if !self.default_font_invalid && self.default_font_seen == [true, true]
            && self.default_fonts[0] == self.default_fonts[1] {
            text_font_element(&mut x, self.default_fonts[0].as_ref(), None);
        }
        let print_text = !self.print_text_invalid && (self.print_header.as_ref().is_some_and(|v| !v.is_empty())
            || self.print_footer.as_ref().is_some_and(|v| !v.is_empty()));
        // DrawingML's pageSetup has no usePrinterDefaults attribute.
        let print_setup = self.print_setup.filter(|data| !self.print_setup_invalid && data[10] & 4 == 0);
        if print_text || print_setup.is_some() {
            x.open("c:printSettings", &[]);
            if print_text {
                x.open("c:headerFooter", &[]);
                for (name, value) in [("c:oddHeader", &self.print_header), ("c:oddFooter", &self.print_footer)] {
                    if let Some(value) = value.as_ref().filter(|v| !v.is_empty()) {
                        x.open(name, &[]); x.text(value); x.close(name);
                    }
                }
                x.close("c:headerFooter");
            }
            if let Some(data) = print_setup { super::features::write_chart_setup(&mut x, &data, "c:pageSetup"); }
            x.close("c:printSettings");
        }
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

fn value_labels_element(x: &mut Xml, source_linked: bool) {
    x.open("c:dLbls", &[]);
    if source_linked {
        x.empty("c:numFmt", &[("formatCode", "General".to_owned()), ("sourceLinked", "1".to_owned())]);
    }
    x.empty("c:showVal", &[("val", "1".to_owned())]);
    x.close("c:dLbls");
}

fn title_element(x: &mut Xml, title: &str, font: Option<&(String, bool, bool)>, color: Option<&str>) {
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
    text_font_element(x, font, color);
    x.close("c:title");
}

fn linked_title_element(x: &mut Xml, formula: &str, cached: Option<&str>, font: Option<&(String, bool, bool)>, color: Option<&str>) {
    x.open("c:title", &[]); x.open("c:tx", &[]); x.open("c:strRef", &[]);
    x.element("c:f", &[], formula);
    if let Some(cached) = cached {
        x.open("c:strCache", &[]); x.empty("c:ptCount", &[("val", "1".to_owned())]);
        x.open("c:pt", &[("idx", "0".to_owned())]); x.element("c:v", &[], cached);
        x.close("c:pt"); x.close("c:strCache");
    }
    x.close("c:strRef"); x.close("c:tx"); x.empty("c:overlay", &[("val", "0".to_owned())]);
    text_font_element(x, font, color);
    x.close("c:title");
}

fn text_font_element(x: &mut Xml, font: Option<&(String, bool, bool)>, color: Option<&str>) {
    if font.is_none() && color.is_none() { return; }
    x.open("c:txPr", &[]); x.empty("a:bodyPr", &[]); x.empty("a:lstStyle", &[]);
    x.open("a:p", &[]); x.open("a:pPr", &[]);
    let attrs = font.map(|(_, bold, italic)| vec![("b", u8::from(*bold).to_string()), ("i", u8::from(*italic).to_string())])
        .unwrap_or_default();
    x.open("a:defRPr", &attrs);
    if let Some(color) = color {
        x.open("a:solidFill", &[]); x.empty("a:srgbClr", &[("val", color.to_owned())]); x.close("a:solidFill");
    }
    if let Some((face, _, _)) = font { x.empty("a:latin", &[("typeface", face.clone())]); }
    x.close("a:defRPr"); x.close("a:pPr"); x.empty("a:endParaRPr", &[]); x.close("a:p"); x.close("c:txPr");
}
