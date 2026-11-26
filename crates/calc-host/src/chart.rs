//! A sheet's charts as a skin draws them (dc-6yw): each chart's place on the sheet, title, legend
//! and plots, and each series' name, categories (a scatter's X values) and values as the workbook
//! holds them now. A series' formulas are evaluated by the one evaluator
//! (through [`calc_engine::DetachedFormula`]), so a chart follows
//! every edit and recalculation; a series whose formula does not evaluate shows the points the
//! file cached with it, and says so ([`SeriesView::cached`]).
//!
//! As Excel reads a series' cells (measured through `Chart.SeriesCollection(i).Values` and
//! `.XValues`: tests/fixtures/dc_6yw/charts-excel.txt): a number is plotted; anything else is a
//! gap (`None`). A series' colour is its `c:spPr` fill, else the theme's accent colour for its
//! index (accent1 to accent6 in turn); a plot that varies its colours by point (a pie) takes them
//! by point in the same order.
//!
//! Linear and logarithmic value axes are retained, editable and drawn, including column/line
//! combinations with primary or secondary pairs. Stacked logarithmic plots are preserved
//! with a drawing notice. Not modelled: 3-D, chart number
//! formats, trendlines, error bars, data labels, and the darker and
//! lighter accents Excel uses past the sixth series.

use crate::state::HostState;
use calc_doc::{ChartAnchor, ChartData, DrawingPoint};
pub use calc_doc_model::ChartTextStyle;
use calc_doc_model::value::{CalcValue, CellKey, CoreRef, SheetId};

/// The picture formats of `Chart.Export`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    /// Portable Network Graphics.
    Png,
    /// Graphics Interchange Format.
    Gif,
    /// JPEG.
    Jpg,
}

/// Draws a chart to the bytes of a picture, `width` by `height` pixels. calc-host cannot depend on the
/// drawing (it is built on [`ChartView`]), so the application installs it ([`set_chart_exporter`]);
/// calc-chart-render's `install` is the one implementation.
pub type ChartExporter = fn(&ChartView, u32, u32, ExportFormat) -> Result<Vec<u8>, String>;

static EXPORTER: std::sync::OnceLock<ChartExporter> = std::sync::OnceLock::new();

/// Install the chart rasteriser; the first installation stays.
pub fn set_chart_exporter(exporter: ChartExporter) {
    let _ = EXPORTER.set(exporter);
}

/// The installed chart rasteriser, if any.
pub fn chart_exporter() -> Option<ChartExporter> {
    EXPORTER.get().copied()
}

/// EMU to a pixel at 96 dots per inch.
pub const EMU_PER_PIXEL: f64 = 9_525.0;

/// A corner of a chart: a cell, and the offset into it in pixels at 100%.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartCorner {
    /// Worksheet cell containing the anchor point.
    pub cell: CellKey,
    /// Horizontal offset from the cell origin in pixels at 100% zoom.
    pub dx: f64,
    /// Vertical offset from the cell origin in pixels at 100% zoom.
    pub dy: f64,
}

/// Where a chart is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChartPlace {
    /// Between two corners: it moves and sizes with the cells under them.
    Cells {
        /// Top-left anchor.
        from: ChartCorner,
        /// Bottom-right anchor.
        to: ChartCorner,
    },
    /// From a corner, with a size in pixels at 100%.
    Sized {
        /// Top-left anchor.
        from: ChartCorner,
        /// Fixed width in pixels at 100% zoom.
        width: f64,
        /// Fixed height in pixels at 100% zoom.
        height: f64,
    },
    /// At a position on the sheet, in pixels at 100%.
    Absolute {
        /// Horizontal position from the sheet origin.
        x: f64,
        /// Vertical position from the sheet origin.
        y: f64,
        /// Fixed width.
        width: f64,
        /// Fixed height.
        height: f64,
    },
}

/// What a plot draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChartKind {
    /// Vertical bars (`barDir="col"`).
    Column,
    /// Horizontal bars.
    Bar,
    /// Points joined by lines across categories.
    Line,
    /// Filled regions beneath series lines across categories.
    Area,
    /// Slices of a circle sized by value.
    Pie,
    /// Slices of a ring sized by value.
    Doughnut,
    /// Points positioned by numeric X and Y coordinates.
    Scatter,
    /// `radarChart` (dc-zjsg.6).
    Radar,
    /// `bubbleChart`: a scatter whose points are sized (dc-zjsg.6).
    Bubble,
    /// `stockChart`: high-low lines with open and close marks over categories (dc-zjsg.6).
    Stock,
    /// An Excel 2016 waterfall (`cx:waterfall`): each point a step from the running total, subtotals from zero.
    Waterfall,
    /// An Excel 2016 funnel (`cx:funnel`): centred bars as wide as their values.
    Funnel,
    /// An Excel 2016 box and whisker (`cx:boxWhisker`): each category's values as quartiles, whiskers and outliers.
    BoxWhisker,
    /// An Excel 2016 histogram (`cx:clusteredColumn`): the values counted into bins; a Pareto adds the cumulative line.
    Histogram,
    /// A surface (`surface3DChart`, or a top-down `surfaceChart` contour): the series as rows of a grid, the values heights.
    Surface,
    /// An Excel 2016 treemap (`cx:treemap`): nested rectangles sized by value.
    Treemap,
    /// An Excel 2016 sunburst (`cx:sunburst`): rings of arcs sized by value, one ring a level.
    Sunburst,
    /// Another kind, by its element name (`surfaceChart`, `ofPieChart`, ...).
    Other(String),
}

/// One series as the chart shows it now.
#[derive(Clone, Debug, PartialEq)]
pub struct SeriesView {
    /// Display name resolved for the series.
    pub name: String,
    /// The categories (a scatter's X values), one per point; `None` for a blank.
    pub categories: Vec<Option<CalcValue>>,
    /// The values (a scatter's Y values); `None` is a gap.
    pub values: Vec<Option<f64>>,
    /// A hierarchy chart's category levels above the leaves, nearest first (see `ChartSeries::outer_levels`).
    pub outer_levels: Vec<Vec<Option<String>>>,
    /// Displayable live value labels, when the source format is supported.
    pub value_labels: Vec<Option<String>>,
    /// The series' colour, `RRGGBB`.
    pub color: String,
    /// A direct series colour takes precedence over automatic point colours.
    pub explicit_color: bool,
    /// Whether any values, categories/X coordinates or series name use the file's saved data.
    pub cached: bool,
    /// A bubble series' sizes, one per point.
    pub sizes: Vec<Option<f64>>,
    /// The series' trendlines as the file states them, the colour resolved to `RRGGBB` (the series' when unstated).
    pub trendlines: Vec<(calc_doc_model::ChartTrendline, String)>,
    /// The series' error bars as the file states them.
    pub error_bars: Vec<calc_doc_model::ChartErrorBars>,
    /// What the series' labels show (its own `dLbls`, else its plot's), and each value as its cell shows it when
    /// they show values (empty when the cells' formats are not ones the session renders).
    pub labels: Option<calc_doc_model::ChartLabels>,
    /// Source value labels indexed by point when formatting is available.
    pub label_values: Vec<Option<String>>,
}

/// One plot of a chart.
#[derive(Clone, Debug, PartialEq)]
pub struct PlotView {
    /// Plot geometry used to draw the series.
    pub kind: ChartKind,
    /// `stacked` or `percentStacked` grouping: each series is drawn on those before it.
    pub stacked: bool,
    /// Whether the stack is expressed as percentages of its category total.
    pub percent: bool,
    /// Each point its own colour (a pie's slices).
    pub vary_colors: bool,
    /// Series in plot order.
    pub series: Vec<SeriesView>,
    /// Identifiers of the axes used by this plot.
    pub axis_ids: Vec<u32>,
    /// Drawn flat from a 3-D plot (`bar3DChart`, `line3DChart`, `area3DChart`, `pie3DChart`).
    pub three_d: bool,
    /// A radar's style: `standard`, `marker` or `filled`.
    pub style: Option<String>,
    /// Bubble scale as a percentage of the default size.
    pub bubble_scale: f64,
    /// Whether the bubble size denotes its width. The default is area.
    pub bubble_width: bool,
    /// Whether a stock or line plot shows high-low lines.
    pub high_low_lines: bool,
    /// Whether line or area points are connected to their category axis by drop lines.
    pub drop_lines: bool,
    /// A waterfall's subtotal points, by index.
    pub subtotals: Vec<u32>,
    /// A box and whisker's quartile method (inclusive or exclusive) and what it shows: mean markers, mean line,
    /// outliers, inner points.
    pub box_inclusive: bool,
    /// Histogram binning options, with `None` selecting automatic binning.
    pub binning: Option<calc_doc_model::Binning>,
    /// Whether the histogram includes a cumulative percentage line.
    pub pareto: bool,
    /// A surface plot drawn as a wireframe.
    pub wireframe: bool,
    /// An Excel 2016 chart's gap between bars as a share of a bar (`None`: the kind's default).
    pub gap_width: Option<f64>,
    /// Visibility of mean markers, mean line, outliers and inner points, in that order.
    pub box_visibility: [bool; 4],
    /// Whether stock plots show bars between the opening and closing values.
    pub up_down_bars: bool,
}

/// One chart as a skin draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct ChartView {
    /// The frame's name (`Chart 1`).
    pub name: String,
    /// Package part path identifying this chart.
    pub part: String,
    /// Position and sizing behavior on the worksheet.
    pub place: ChartPlace,
    /// Display title, when present.
    pub title: Option<String>,
    /// The legend's position (`r`, `l`, `t`, `b`, `tr`), `None` without a legend.
    pub legend: Option<String>,
    /// Plots in chart display order.
    pub plots: Vec<PlotView>,
    /// Axis definitions referenced by the plots.
    pub axes: Vec<calc_doc_model::ChartAxis>,
    /// Workbook format profile used to render explicit axis labels.
    pub format_profile: calc_fml::FormatProfile,
    /// Workbook date system used to interpret serial dates.
    pub date_system: calc_func::locale::DateSystem,
    /// The theme's accent colours, accent1 to accent6, `RRGGBB`: a varied plot's point colours.
    pub palette: Vec<String>,
    /// Default chart text style.
    pub text_style: calc_doc_model::ChartTextStyle,
    /// Text style overrides keyed by chart element.
    pub element_text: std::collections::BTreeMap<String, calc_doc_model::ChartTextStyle>,
}

/// Excel's 2013 theme accents, for a workbook whose theme gives none.
const OFFICE_ACCENTS: [&str; 6] = ["4472C4", "ED7D31", "A5A5A5", "FFC000", "5B9BD5", "70AD47"];

impl HostState {
    /// The workbook theme's twelve colours as `RRGGBB` in scheme order (dk1, lt1, dk2, lt2, accent1 to
    /// accent6, hlink, folHlink); empty when the workbook has no theme part (drawing colours, dc-zjsg.2).
    pub fn theme_colours(&self) -> &[String] {
        self.loaded.theme_colors()
    }

    /// The widths in EMU of the theme's line styles, for a shape's `a:lnRef` (dc-zjsg.2).
    pub fn theme_line_widths(&self) -> &[i64] {
        self.loaded.theme_line_widths()
    }

    /// The workbook's accent1–accent6, with the same Office fallback for charts and tables.
    pub fn theme_accent(&self, index: usize) -> Option<String> {
        let fallback = OFFICE_ACCENTS.get(index)?;
        Some(
            self.loaded
                .theme_colors()
                .get(4 + index)
                .filter(|c| c.len() == 6)
                .cloned()
                .unwrap_or_else(|| (*fallback).to_owned()),
        )
    }
    /// The charts on `sheet`, in the drawing's order, with their series as the workbook holds
    /// them now; and how many charts the file holds that could not be read.
    pub fn charts(&self, sheet: SheetId) -> (Vec<ChartView>, usize) {
        // A chart sheet a macro made has no loaded part and nothing unread (dc-oxh8w.23).
        let no_charts = calc_doc_model::SheetCharts::default();
        let read = match self.loaded.sheet(sheet) {
            Some(loaded) => loaded.charts(),
            None if is_chart_sheet(self, sheet) => &no_charts,
            None => return (Vec::new(), 0),
        };
        let theme = self.loaded.theme_colors();
        let palette: Vec<String> = (0..6)
            .map(|i| self.theme_accent(i).expect("six theme accents"))
            .collect();
        let scheme = |name: &str| -> Option<String> {
            let slot = match name {
                "dk1" | "tx1" => 0,
                "lt1" | "bg1" => 1,
                "dk2" | "tx2" => 2,
                "lt2" | "bg2" => 3,
                accent => 3 + accent.strip_prefix("accent")?.parse::<usize>().ok()?,
            };
            if (4..10).contains(&slot) {
                return palette.get(slot - 4).cloned();
            }
            theme.get(slot).filter(|c| c.len() == 6).cloned()
        };
        let origin = CellKey::new(sheet, 1, 1).expect("A1 is a cell");
        let mut views = Vec::new();
        let mut unread = read.unread;
        for chart in self.model().definitions().charts(sheet) {
            let Some(place) = self.frame_place(sheet, &chart.anchor, chart.edit_as.as_deref()) else {
                // Its cells were deleted: Excel deletes the chart with them.
                continue;
            };
            let plots = chart
                .plots
                .iter()
                .map(|plot| {
                    let kind = match plot.kind.as_str() {
                        "barChart" | "bar3DChart" if plot.bar_direction.as_deref() == Some("bar") => {
                            ChartKind::Bar
                        }
                        "barChart" | "bar3DChart" => ChartKind::Column,
                        "lineChart" | "line3DChart" => ChartKind::Line,
                        "areaChart" | "area3DChart" => ChartKind::Area,
                        "pieChart" | "pie3DChart" => ChartKind::Pie,
                        "doughnutChart" => ChartKind::Doughnut,
                        "scatterChart" => ChartKind::Scatter,
                        "radarChart" => ChartKind::Radar,
                        "bubbleChart" => ChartKind::Bubble,
                        "stockChart" => ChartKind::Stock,
                        "cx:waterfall" => ChartKind::Waterfall,
                        "cx:funnel" => ChartKind::Funnel,
                        "cx:boxWhisker" => ChartKind::BoxWhisker,
                        "cx:clusteredColumn" => ChartKind::Histogram,
                        "surfaceChart" | "surface3DChart" => ChartKind::Surface,
                        "cx:treemap" => ChartKind::Treemap,
                        "cx:sunburst" => ChartKind::Sunburst,
                        other => ChartKind::Other(other.to_owned()),
                    };
                    let grouping = plot.grouping.as_deref().unwrap_or_default();
                    PlotView {
                        // A 3-D plot drawn flat leaves out its series axis, and the id 0 a clustered 3-D column names
                        // for the series axis it does not have.
                        axis_ids: if plot.kind.contains("3D") {
                            plot.axis_ids.iter().copied().filter(|id| chart.axes.iter().any(|a| a.id == *id && a.kind != "serAx")).collect()
                        } else {
                            plot.axis_ids.clone()
                        },
                        kind,
                        three_d: plot.kind.contains("3D"),
                        style: plot.style.clone(),
                        bubble_scale: f64::from(plot.bubble_scale.unwrap_or(100).min(300)),
                        bubble_width: plot.bubble_width,
                        high_low_lines: plot.high_low_lines,
                        drop_lines: plot.drop_lines,
                        subtotals: plot.subtotals.clone(),
                        box_inclusive: plot.box_inclusive,
                        binning: plot.binning.clone(),
                        pareto: plot.pareto,
                        wireframe: plot.wireframe,
                        gap_width: plot.gap_width.as_deref().and_then(|g| g.trim().parse().ok()).filter(|g: &f64| g.is_finite() && *g >= 0.0),
                        box_visibility: plot.box_visibility.unwrap_or([true, false, true, false]),
                        up_down_bars: plot.up_down_bars,
                        stacked: matches!(grouping, "stacked" | "percentStacked"),
                        percent: grouping == "percentStacked",
                        vary_colors: plot.vary_colors,
                        series: plot
                            .series
                            .iter()
                            .map(|series| {
                                let (values, cached) =
                                    self.series_values(series.values.as_ref(), origin);
                                let source_linked = if series.show_values.is_some() { series.value_labels_source_linked }
                                    else { plot.value_labels_source_linked };
                                let value_labels = if matches!(plot.kind.as_str(), "barChart" | "lineChart") && plot.show_values != Some(false)
                                    && series.show_values.or(plot.show_values) == Some(true) {
                                    self.simple_value_labels(series.values.as_ref(), &values, cached, source_linked, origin)
                                } else { Vec::new() };
                                let (categories, category_cached) = series
                                    .categories
                                    .as_ref()
                                    .map(|data| self.data_values(data, origin))
                                    .unwrap_or_default();
                                let (names, name_cached) = series
                                    .name
                                    .as_ref()
                                    .map(|data| self.data_values(data, origin))
                                    .unwrap_or_default();
                                let name = names
                                    .into_iter()
                                    .flatten()
                                    .next()
                                    .map(|value| text_of(&value))
                                    .unwrap_or_else(|| format!("Series{}", series.index + 1));
                                let cached = cached || category_cached || name_cached;
                                let color = series
                                    .fill
                                    .as_deref()
                                    .and_then(|fill| {
                                        if fill.len() == 6
                                            && fill.chars().all(|c| c.is_ascii_hexdigit())
                                        {
                                            Some(fill.to_ascii_uppercase())
                                        } else {
                                            scheme(fill)
                                        }
                                    })
                                    .unwrap_or_else(|| palette[series.index as usize % 6].clone());
                                let sizes = series
                                    .bubble_sizes
                                    .as_ref()
                                    .map(|data| self.series_values(Some(data), origin).0)
                                    .unwrap_or_default();
                                let trendlines = series
                                    .trendlines
                                    .iter()
                                    .map(|t| {
                                        let colour = t
                                            .colour
                                            .as_deref()
                                            .and_then(|c| if c.len() == 6 && c.chars().all(|c| c.is_ascii_hexdigit()) { Some(c.to_ascii_uppercase()) } else { scheme(c) })
                                            .unwrap_or_else(|| color.clone());
                                        (t.clone(), colour)
                                    })
                                    .collect();
                                let labels = series.labels.clone().or_else(|| plot.labels.clone()).filter(|l| !l.deleted && (l.value || l.category || l.series || l.percent || !l.points.is_empty()));
                                let label_values = if labels.as_ref().is_some_and(|l| l.value || l.points.iter().any(|(_, p)| p.value)) {
                                    self.simple_value_labels(series.values.as_ref(), &values, cached, true, origin)
                                } else {
                                    Vec::new()
                                };
                                SeriesView {
                                    labels,
                                    label_values,
                                    error_bars: series.error_bars.clone(),
                                    trendlines,
                                    sizes,
                                    name,
                                    categories,
                                    values,
                                    outer_levels: series.outer_levels.clone(),
                                    value_labels,
                                    color,
                                    explicit_color: series.fill.is_some(),
                                    cached,
                                }
                            })
                            .collect(),
                    }
                })
                .collect::<Vec<_>>();
            if plots.is_empty() {
                unread += 1;
                continue;
            }
            // Without a title of its own, a chart of one series shows the series' name, unless the
            // title was deleted.
            let title = chart.title.clone().or_else(|| {
                let mut series = plots.iter().flat_map(|plot| plot.series.iter());
                match (series.next(), series.next(), chart.auto_title_deleted) {
                    (Some(only), None, false) => Some(only.name.clone()),
                    _ => None,
                }
            });
            views.push(ChartView {
                name: chart.name.clone(),
                part: chart.part.clone(),
                place,
                title,
                legend: chart.legend.clone(),
                plots,
                palette: palette.clone(),
                element_text: chart
                    .element_text
                    .iter()
                    .map(|(key, font)| {
                        let mut font = font.clone();
                        font.color = font.color.as_ref().and_then(|c| {
                            if c.len() == 6 && c.bytes().all(|b| b.is_ascii_hexdigit()) {
                                Some(c.clone())
                            } else {
                                scheme(c)
                            }
                        });
                        (key.clone(), font)
                    })
                    .collect(),
                text_style: {
                    let mut style = chart.text_style.clone();
                    style.color = style.color.as_ref().and_then(|c| {
                        if c.len() == 6 && c.bytes().all(|b| b.is_ascii_hexdigit()) {
                            Some(c.clone())
                        } else {
                            scheme(c)
                        }
                    });
                    style
                },
                // A 3-D chart drawn flat has no series (depth) axis.
                axes: chart.axes.iter().filter(|a| !(a.kind == "serAx" && chart.plots.iter().any(|p| p.kind.contains("3D")))).cloned().collect(),
                format_profile: self.entry_environment().profile.unwrap_or_else(|| calc_func::locale::format_profile(calc_func::locale::LocaleId::EnUs)),
                date_system: if self.declared_date1904() { calc_func::locale::DateSystem::System1904 } else { calc_func::locale::DateSystem::System1900 },
            });
        }
        (views, unread)
    }

    /// What a chart's series were made from (dc-piw0.11 Q22): whether they run along rows, and the
    /// area that holds every cell they name, values, categories and names. Excel keeps this
    /// state on the chart; here it is read back from the series' references. `None` for a chart
    /// without a series that names cells.
    pub(crate) fn chart_source_of(&self, sheet: SheetId, chart: &calc_doc_model::Chart) -> Option<(bool, calc_value::GridArea)> {
        let origin = CellKey::new(sheet, 1, 1).ok()?;
        let area_of = |data: &Option<ChartData>| match self.resolve_reference_text(origin, data.as_ref()?.formula.as_deref()?.trim_start_matches('=')) {
            crate::ReferenceTarget::Area(area) => Some(area),
            _ => None,
        };
        let series: Vec<&calc_doc_model::ChartSeries> = chart.plots.iter().flat_map(|p| p.series.iter()).collect();
        let first = series.first()?;
        let values = area_of(&first.values)?;
        let name = area_of(&first.name);
        let categories = area_of(&first.categories);
        // A row of values runs along rows, a column along columns; one cell is read from where its name sits
        // (left of it in a row series, above it in a column series), else from its categories.
        let rows = if values.column_count() > 1 && values.row_count() == 1 {
            true
        } else if values.row_count() > 1 && values.column_count() == 1 {
            false
        } else if let Some(name) = name {
            name.first().column() < values.first().column()
        } else if let Some(categories) = categories {
            categories.row_count() == 1 && categories.column_count() > 1
        } else {
            return None;
        };
        let (mut top, mut left) = (u32::MAX, u32::MAX);
        let (mut bottom, mut right) = (0, 0);
        for data in series.iter().flat_map(|s| [&s.name, &s.categories, &s.values]) {
            let Some(area) = area_of(data) else { continue };
            if area.sheet() != values.sheet() {
                continue;
            }
            top = top.min(area.first().row());
            left = left.min(area.first().column());
            bottom = bottom.max(area.last().row());
            right = right.max(area.last().column());
        }
        let area = calc_value::GridArea::new(
            CellKey::new(values.sheet(), top, left).ok()?,
            CellKey::new(values.sheet(), bottom, right).ok()?,
        )
        .ok()?;
        Some((rows, area))
    }

    /// The name a series shows: the text of the cell it names, else `Series<n>` (dc-piw0 Q6).
    pub(crate) fn series_display_name(
        &self,
        sheet: SheetId,
        series: &calc_doc_model::ChartSeries,
        place: usize,
    ) -> String {
        let origin = CellKey::new(sheet, 1, 1).expect("A1 is a cell");
        series
            .name
            .as_ref()
            .map(|data| self.data_values(data, origin).0)
            .and_then(|names| names.into_iter().flatten().next())
            .map(|value| self.name_cell_text(series, origin, &value))
            .unwrap_or_else(|| format!("Series{}", place + 1))
    }

    /// The text of the cell a series' name reads: a number under a number format shows as the cell shows it (Q22:
    /// a date label is `2023-03-15`), else its value as text.
    fn name_cell_text(&self, series: &calc_doc_model::ChartSeries, origin: CellKey, value: &CalcValue) -> String {
        let shown = (|| {
            let formula = series.name.as_ref()?.formula.as_deref()?;
            let crate::ReferenceTarget::Area(area) = self.resolve_reference_text(origin, formula.trim_start_matches('=')) else {
                return None;
            };
            if area.cell_count() != 1 || !matches!(value.core_ref(), CoreRef::Number(_)) {
                return None;
            }
            let code = self.number_format(area.first())?;
            if code.eq_ignore_ascii_case("general") {
                return None;
            }
            let environment = self.entry_environment();
            let fallback;
            let profile = match environment.profile.as_ref() {
                Some(profile) => profile,
                None => {
                    fallback = calc_func::locale::format_profile(calc_func::locale::LocaleId::EnUs);
                    &fallback
                }
            };
            let system = if self.declared_date1904() {
                calc_func::locale::DateSystem::System1904
            } else {
                calc_func::locale::DateSystem::System1900
            };
            calc_func::cell_text(profile, system, value, code).ok()
        })();
        shown.unwrap_or_else(|| text_of(value))
    }

    /// A drawing point's cell where the session's insertions and deletions moved it, with its
    /// offset in pixels; `None` when its cell was deleted.
    fn chart_corner(&self, sheet: SheetId, point: &DrawingPoint) -> Option<ChartCorner> {
        let (row, column) = (point.row + 1, point.column + 1);
        let area = *self
            .current_areas(sheet, &[(row, column, row, column)])
            .first()?;
        Some(ChartCorner {
            cell: area.first(),
            dx: point.column_offset as f64 / EMU_PER_PIXEL,
            dy: point.row_offset as f64 / EMU_PER_PIXEL,
        })
    }

    /// Where a drawing object's anchor puts it: a chart's, a shape's.
    pub fn chart_place(&self, sheet: SheetId, anchor: &ChartAnchor) -> Option<ChartPlace> {
        self.drawing_place(sheet, anchor, None)
    }

    /// Where a chart frame is: by its anchor, with `editAs="oneCell"` (moves but does not size) reported as `Sized`
    /// from its top-left corner and `editAs="absolute"` as `Absolute` using the current row heights and column widths
    /// (`shapes::kept_sizes` rewrites the stored anchor on row/column format changes and Save writes it, dc-zjsg.14).
    pub fn frame_place(&self, sheet: SheetId, anchor: &ChartAnchor, edit_as: Option<&str>) -> Option<ChartPlace> {
        let (ChartAnchor::TwoCell { from, to }, Some("oneCell" | "absolute")) = (anchor, edit_as) else {
            return self.drawing_place(sheet, anchor, edit_as);
        };
        let start = if edit_as == Some("absolute") {
            ChartCorner {
                cell: CellKey::new(sheet, from.row + 1, from.column + 1).ok()?,
                dx: from.column_offset as f64 / EMU_PER_PIXEL,
                dy: from.row_offset as f64 / EMU_PER_PIXEL,
            }
        } else {
            self.chart_corner(sheet, from)?
        };
        let current_from = DrawingPoint {
            column: start.cell.column() - 1,
            column_offset: from.column_offset,
            row: start.cell.row() - 1,
            row_offset: from.row_offset,
        };
        let current_to = DrawingPoint {
            column: current_from.column + to.column.saturating_sub(from.column),
            column_offset: to.column_offset,
            row: current_from.row + to.row.saturating_sub(from.row),
            row_offset: to.row_offset,
        };
        let (left, top, width, height) = self.anchor_rectangle(
            sheet,
            &ChartAnchor::TwoCell { from: current_from, to: current_to },
        );
        let px = crate::facts::REFERENCE_DPI / 72.0;
        if edit_as == Some("absolute") {
            return Some(ChartPlace::Absolute { x: left * px, y: top * px, width: width * px, height: height * px });
        }
        Some(ChartPlace::Sized { from: start, width: width * px, height: height * px })
    }

    /// Where a drawing object's anchor puts it now, moved through the session's row and column inserts and deletes as
    /// Excel moves an object over inserted or deleted cells (the save's anchor pass does the same to the stored part):
    /// a two-cell anchor's corners each to where their cell goes; a one-cell anchor, or a two-cell one whose `editAs`
    /// is `oneCell`, by its top-left corner with its span kept; a two-cell anchor whose `editAs` is `absolute` not at all.
    pub fn drawing_place(&self, sheet: SheetId, anchor: &ChartAnchor, edit_as: Option<&str>) -> Option<ChartPlace> {
        let px = |emu: i64| emu as f64 / EMU_PER_PIXEL;
        if let ChartAnchor::TwoCell { from, to } = anchor
            && matches!(edit_as, Some("oneCell" | "absolute"))
        {
            let start = if edit_as == Some("absolute") {
                ChartCorner {
                    cell: CellKey::new(sheet, from.row + 1, from.column + 1).ok()?,
                    dx: px(from.column_offset),
                    dy: px(from.row_offset),
                }
            } else {
                self.chart_corner(sheet, from)?
            };
            let end = ChartCorner {
                cell: CellKey::new(
                    sheet,
                    start.cell.row() + to.row.saturating_sub(from.row),
                    start.cell.column() + to.column.saturating_sub(from.column),
                )
                .ok()?,
                dx: px(to.column_offset),
                dy: px(to.row_offset),
            };
            return Some(ChartPlace::Cells { from: start, to: end });
        }
        Some(match anchor {
            ChartAnchor::TwoCell { from, to } => ChartPlace::Cells {
                from: self.chart_corner(sheet, from)?,
                to: self.chart_corner(sheet, to)?,
            },
            ChartAnchor::OneCell {
                from,
                width,
                height,
            } => ChartPlace::Sized {
                from: self.chart_corner(sheet, from)?,
                width: px(*width),
                height: px(*height),
            },
            ChartAnchor::Absolute {
                x,
                y,
                width,
                height,
            } => ChartPlace::Absolute {
                x: px(*x),
                y: px(*y),
                width: px(*width),
                height: px(*height),
            },
        })
    }

    /// A reference the file wrote on `sheet`, rewritten through the session's row and column inserts and deletes.
    pub(crate) fn current_reference(&self, formula: &str, sheet: SheetId) -> String {
        use calc_doc_model::{FormulaRewriter, NameScope, StructuralChange};
        let mut current = formula.to_owned();
        for edit in &self.axis_edits {
            if let Some(next) = calc_engine::ReferenceRewriter.name(
                self.loaded.model(),
                &current,
                NameScope::Sheet(sheet),
                &StructuralChange::Axis(*edit),
            ) {
                current = next;
            }
        }
        current
    }

    /// A series' data as the workbook holds it now (its formula evaluated), else as the file
    /// cached it; and whether it is the cache.
    pub(crate) fn data_values(&self, data: &ChartData, origin: CellKey) -> (Vec<Option<CalcValue>>, bool) {
        if let Some(values) = data.formula.as_deref().and_then(|formula| {
            let current = self.current_reference(formula, origin.sheet());
            self.formula_values(&current, origin, origin)
        }) {
            return (values, false);
        }
        let cached = data
            .cache
            .iter()
            .map(|point| {
                point
                    .as_deref()
                    .map(|text| match text.trim().parse::<f64>() {
                        Ok(n) if data.numeric => CalcValue::number(n),
                        _ => CalcValue::text(
                            calc_doc_model::value::ExcelText::from_utf16_code_units(
                                text.encode_utf16().collect(),
                            ),
                        ),
                    })
            })
            .collect();
        // Literal points are the chart source itself; only a failed reference uses a cache.
        (cached, data.formula.is_some())
    }

    fn series_values(&self, data: Option<&ChartData>, origin: CellKey) -> (Vec<Option<f64>>, bool) {
        let Some(data) = data else {
            return (Vec::new(), false);
        };
        let (values, cached) = self.data_values(data, origin);
        (
            values
                .iter()
                .map(|value| match value.as_ref().map(CalcValue::core_ref) {
                    Some(CoreRef::Number(n)) => Some(n),
                    _ => None,
                })
                .collect(),
            cached,
        )
    }

    /// Label live, directly referenced one-dimensional sources with a supported number format.
    fn simple_value_labels(&self, data: Option<&ChartData>, values: &[Option<f64>], cached: bool, source_linked: bool, origin: CellKey) -> Vec<Option<String>> {
        if cached || !self.axis_edits.is_empty() { return Vec::new(); }
        let Some(formula) = data.and_then(|data| data.formula.as_deref()) else { return Vec::new() };
        let crate::ReferenceTarget::Area(area) = self.resolve_reference_text(origin, formula.trim_start_matches('=')) else { return Vec::new() };
        if (area.row_count() != 1 && area.column_count() != 1) || area.cell_count() != values.len() as u64 {
            return Vec::new();
        }
        let environment = self.entry_environment();
        let fallback;
        let profile = match environment.profile.as_ref() {
            Some(profile) => profile,
            None => { fallback = calc_func::locale::format_profile(calc_func::locale::LocaleId::EnUs); &fallback }
        };
        let mut labels = Vec::with_capacity(values.len());
        let system = if self.declared_date1904() { calc_func::locale::DateSystem::System1904 }
            else { calc_func::locale::DateSystem::System1900 };
        for (index, value) in values.iter().enumerate() {
            let row = area.first().row() + if area.column_count() == 1 { index as u32 } else { 0 };
            let column = area.first().column() + if area.row_count() == 1 { index as u32 } else { 0 };
            let Ok(at) = CellKey::new(area.sheet(), row, column) else { return Vec::new() };
            let Some(code) = self.number_format(at) else { return Vec::new() };
            if !source_linked && !code.eq_ignore_ascii_case("general") { return Vec::new(); }
            let text = match value {
                None => None,
                Some(number) if code.eq_ignore_ascii_case("general") => Some(calc_func::general_format(profile, *number)),
                Some(number) => match calc_func::cell_text(profile, system, &CalcValue::number(*number), code) {
                    Ok(text) => Some(text), Err(_) => return Vec::new(),
                },
            };
            labels.push(text);
        }
        labels
    }
}

/// A value as a chart's label shows it: a text as it is, a number in its shortest form (the cell's
/// number format is not applied), a logical as TRUE or FALSE; anything else as nothing. A chart's
/// labels and a validation list's items show this.
pub fn text_of(value: &CalcValue) -> String {
    match value.core_ref() {
        CoreRef::Text(text) => text.to_string_lossy(),
        CoreRef::Number(n) => format!("{n}"),
        CoreRef::Logical(b) => if b { "TRUE" } else { "FALSE" }.to_owned(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod value_label_tests {
    use super::*;
    use crate::{HostOptions, HostSession, MemorySource, OpenPolicy, SkinIntent, WorkbookCommand};
    use std::sync::Arc;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn labels_use_live_general_values_and_refuse_saved_caches() {
        let bytes = include_bytes!("../tests/fixtures/dc_6yw/charts.xlsx");
        let mut session = HostSession::open(Arc::new(MemorySource::new(bytes.to_vec())), &OpenPolicy::default(), HostOptions::default()).unwrap();
        assert!(session.dispatch(SkinIntent::Recalculate).accepted());
        let host = session.host();
        let sheet = host.sheets()[0].id();
        let data = host.model().definitions().charts(sheet)[0].plots[0].series[0].values.as_ref().unwrap().clone();
        let origin = CellKey::new(sheet, 1, 1).unwrap();
        let (values, cached) = host.series_values(Some(&data), origin);
        assert!(!cached);
        let labels = host.simple_value_labels(Some(&data), &values, cached, false, origin);
        assert_eq!(labels, ["12", "18", "9", "22", "15", "27"].map(|s| Some(s.to_owned())));
        assert!(host.simple_value_labels(Some(&data), &values, true, false, origin).is_empty());
        assert!(host.simple_value_labels(Some(&data), &values[..1], false, false, origin).is_empty());
        let cell = CellKey::new(sheet, 2, 2).unwrap();
        assert!(session.dispatch(SkinIntent::Command(WorkbookCommand::FormatCells {
            areas: vec![calc_value::GridArea::cell(cell)],
            patch: crate::FormatPatch { number_format: Some("0.00".into()), ..Default::default() },
        })).accepted());
        assert!(session.host().simple_value_labels(Some(&data), &values, false, false, origin).is_empty());
        let formatted = session.host().simple_value_labels(Some(&data), &values, false, true, origin);
        assert_eq!(formatted.first().and_then(Option::as_deref), Some("12.00"));
    }
}

/// Title and legend visibility use the authored document command and its ordinary history.
pub(crate) fn prepare_properties(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    part: &str,
    title: Option<String>,
    set_title: bool,
    legend: Option<bool>,
) -> Result<Vec<crate::CellEdit>, String> {
    if revision != host.revision().get() {
        return Err("The workbook changed; reopen the chart controls (dc-zjsg)".into());
    }
    // A chart sheet a macro made has no loaded part (dc-oxh8w.23).
    let source = host.loaded().sheet(sheet);
    if source.is_none() && !is_chart_sheet(host, sheet) {
        return Err("The chart worksheet no longer exists".into());
    }
    if source.is_some_and(|s| s.protected() || s.is_macro_sheet()) {
        return Err(
            "Editing charts on this protected or macro worksheet is not modelled (dc-zjsg)".into(),
        );
    }
    if host.loaded().workbook_part().ends_with(".bin") {
        return Err(
            "Save as XLSX and reopen before editing charts in an XLSB workbook (dc-zjsg)".into(),
        );
    }
    let mut charts = host.model().definitions().charts(sheet).to_vec();
    let index = charts
        .iter()
        .position(|c| c.part == part)
        .ok_or("The chart no longer belongs to this worksheet; reopen its controls")?;
    let original = charts[index].clone();
    let mut changed = original.clone();
    if set_title {
        changed.title = title;
        changed.title_formula = None;
        changed.auto_title_deleted = changed.title.is_none();
    }
    if let Some(show) = legend {
        changed.legend = if show {
            changed.legend.or_else(|| Some("r".into()))
        } else {
            None
        };
    }
    if changed.title.is_none() && changed.title_formula.is_none() {
        changed.element_text.remove("title");
    }
    if changed.legend.is_none() {
        changed.element_text.remove("legend");
    }
    if changed == original {
        return Ok(Vec::new());
    }
    host.loaded()
        .check_chart_properties(sheet, &changed)
        .map_err(|e| e.to_string())?;
    charts[index] = changed;
    Ok(vec![crate::CellEdit::Charts { sheet, charts }])
}

/// Edit one series through the retained chart writer. A missing index adds a series.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_series(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    part: &str,
    plot: u32,
    series: Option<u32>,
    name: Option<String>,
    categories: Option<String>,
    values: Option<String>,
    remove: bool,
) -> Result<Vec<crate::CellEdit>, String> {
    // Reuse the chart target, protection, format and revision checks.
    prepare_properties(host, sheet, revision, part, None, false, None)?;
    if !host.axis_edits.is_empty() {
        return Err("Save and reopen before editing chart sources after inserting or deleting rows or columns (dc-zjsg)".into());
    }
    let mut charts = host.model().definitions().charts(sheet).to_vec();
    let chart = charts
        .iter_mut()
        .find(|c| c.part == part)
        .ok_or("The chart is unavailable")?;
    let next = chart
        .plots
        .iter()
        .flat_map(|p| &p.series)
        .map(|s| s.index)
        .chain(
            host.loaded()
                .model()
                .definitions()
                .charts(sheet)
                .iter()
                .filter(|c| c.part == part)
                .flat_map(|c| &c.plots)
                .flat_map(|p| &p.series)
                .map(|s| s.index),
        )
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("The series index limit was reached")?;
    let total = chart.plots.iter().map(|p| p.series.len()).sum::<usize>();
    // A series a macro's chart gains takes the next uniqueId counter (dc-piw0.10 Q17).
    let issue = chart.drawing_id.is_some().then_some(chart.unique_next);
    let mut issued = false;
    let p = chart
        .plots
        .get_mut(plot as usize)
        .ok_or("The chart plot is unavailable")?;
    if !matches!(
        p.kind.as_str(),
        "barChart" | "lineChart" | "areaChart" | "pieChart" | "doughnutChart" | "scatterChart"
    ) {
        return Err("Editing series of this chart type is not modelled (dc-zjsg)".into());
    }
    let position = series
        .map(|index| {
            p.series
                .iter()
                .position(|s| s.index == index)
                .ok_or("The series changed; reopen its controls")
        })
        .transpose()?;
    if remove {
        let i = position.ok_or("Select a series to remove")?;
        if p.series.len() == 1 {
            return Err("Keep at least one series in a chart plot (dc-zjsg)".into());
        }
        p.series.remove(i);
    } else {
        if position.is_none() && total >= 255 {
            return Err("A chart can have at most 255 series (dc-zjsg)".into());
        }
        if position.is_none() && matches!(p.kind.as_str(), "pieChart" | "doughnutChart") {
            return Err("Adding pie or doughnut series is not modelled; edit the existing series (dc-zjsg)".into());
        }
        let mut changed =
            position
                .map(|i| p.series[i].clone())
                .unwrap_or_else(|| {
                    issued = issue.is_some();
                    calc_doc_model::ChartSeries {
                        index: next,
                        unique_id: issue,
                        ..Default::default()
                    }
                });
        let origin = CellKey::new(sheet, 1, 1).unwrap();
        let range = |text: &str, numeric: bool, single: bool| {
            range_data(host, origin, text, numeric, single)
        };
        if let Some(text) = name {
            if text.encode_utf16().count() > 32767
                || text.chars().any(|c| {
                    (c < ' ' && !matches!(c, '\t' | '\n' | '\r'))
                        || matches!(c, '\u{fffe}' | '\u{ffff}')
                })
            {
                return Err("The series name contains unsupported text or exceeds 32,767 UTF-16 units (dc-zjsg)".into());
            }
            changed.name = if text.is_empty() {
                None
            } else if text.starts_with('=') {
                Some(range(&text, false, true)?)
            } else {
                Some(ChartData {
                    cache: vec![Some(text)],
                    ..Default::default()
                })
            };
        }
        if let Some(text) = categories {
            changed.categories = if text.trim().is_empty() {
                None
            } else {
                Some(range(&text, p.kind == "scatterChart", false)?)
            };
        }
        if let Some(text) = values {
            changed.values = Some(range(&text, true, false)?);
        }
        if changed.values.is_none() {
            return Err("Choose a values range for the new series (dc-zjsg)".into());
        }
        if let Some(i) = position {
            p.series[i] = changed;
        } else {
            p.series.push(changed);
        }
    }
    if issued {
        chart.unique_next += 1;
    }
    host.loaded()
        .check_chart_properties(sheet, chart)
        .map_err(|e| e.to_string())?;
    if charts == host.model().definitions().charts(sheet) {
        return Ok(Vec::new());
    }
    Ok(vec![crate::CellEdit::Charts { sheet, charts }])
}

/// Return automatic linear bounds and major tick spacing as `(minimum, maximum, step)`.
pub fn axis(low: f64, high: f64) -> (f64, f64, f64) {
    let (mut low, mut high) = (low.min(0.0), high.max(0.0));
    if !(low.is_finite() && high.is_finite()) || high - low <= 0.0 {
        low = low.min(0.0);
        high = low + 1.0;
    }
    let raw = (high - low) / 8.0;
    let power = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * power)
        .find(|s| *s >= raw)
        .unwrap_or(10.0 * power);
    let room = (high - low) / 20.0;
    let min = if low < 0.0 {
        ((low - room) / step).floor() * step
    } else {
        0.0
    };
    let max = if high > 0.0 {
        ((high + room) / step).ceil() * step
    } else {
        0.0
    };
    (min, max, step)
}

/// A validated value scale shared by command validation and chart drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartScale {
    /// Lower numeric bound.
    pub minimum: f64,
    /// Upper numeric bound.
    pub maximum: f64,
    /// Major tick spacing on a linear axis or the base on a logarithmic axis.
    pub step: f64,
    /// Whether increasing values run in the reverse display direction.
    pub reversed: bool,
    /// Logarithmic base, or `None` for a linear axis.
    pub log_base: Option<f64>,
}

/// Build a scale from the data extent and any authored axis overrides.
///
/// Invalid bounds or spacing return an error.
///
/// ```
/// use calc_host::chart::chart_scale;
///
/// let scale = chart_scale(2.0, 18.0, None).unwrap();
/// assert_eq!(scale.minimum, 0.0);
/// assert!(scale.maximum >= 18.0);
/// assert_eq!(scale.position(scale.maximum), Some(1.0));
/// assert_eq!(scale.ticks().first(), Some(&scale.minimum));
/// ```
pub fn chart_scale(
    low: f64,
    high: f64,
    properties: Option<&calc_doc_model::ChartAxis>,
) -> Result<ChartScale, String> {
    if let Some(axis) = properties.filter(|a| a.log_base.is_some()) {
        return logarithmic_scale(low, high, axis);
    }
    if !low.is_finite() || !high.is_finite() || !(high.max(0.) - low.min(0.)).is_finite() {
        return Err("This chart's values exceed the supported linear axis range".into());
    }
    let (automatic_min, automatic_max, automatic_step) = axis(low, high);
    let read = |value: Option<&str>| -> Result<Option<f64>, String> {
        value
            .map(|s| {
                s.parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .ok_or_else(|| "Use finite numbers for axis bounds and units".into())
            })
            .transpose()
    };
    let minimum = read(properties.and_then(|a| a.minimum.as_deref()))?;
    let maximum = read(properties.and_then(|a| a.maximum.as_deref()))?;
    let unit = read(properties.and_then(|a| a.major_unit.as_deref()))?;
    let mut min = minimum.unwrap_or(automatic_min);
    let mut max = maximum.unwrap_or(automatic_max);
    let automatic_span = automatic_max - automatic_min;
    if minimum.is_none() && min >= max {
        min = max - automatic_span;
    }
    if maximum.is_none() && max <= min {
        max = min + automatic_span;
    }
    let span = max - min;
    if !span.is_finite() || span <= 0. {
        return Err("The axis maximum must be greater than its minimum".into());
    }
    if [low, high, 0.]
        .into_iter()
        .any(|v| !(((v - min) / span) * 1_000_000.).is_finite())
    {
        return Err("The fixed bounds are too narrow for this chart's values".into());
    }
    let step = if let Some(unit) = unit {
        unit
    } else if minimum.is_none() && maximum.is_none() {
        automatic_step
    } else {
        let raw = span / 8.;
        let power = 10f64.powf(raw.log10().floor());
        [1., 2., 5., 10.]
            .into_iter()
            .map(|m| m * power)
            .find(|s| *s >= raw)
            .unwrap_or(10. * power)
    };
    if !step.is_finite() || step <= 0. || span / step > 1000. {
        return Err("Use a positive major unit that produces at most 1,000 intervals".into());
    }
    Ok(ChartScale {
        minimum: min,
        maximum: max,
        step,
        reversed: properties.is_some_and(|a| a.reversed),
        log_base: None,
    })
}

impl ChartScale {
    /// Normalized axis coordinate, with nonpositive logarithmic values omitted.
    pub fn position(self, value: f64) -> Option<f64> {
        let (v, min, max) = if self.log_base.is_some() {
            if value <= 0. || !value.is_finite() {
                return None;
            }
            (value.ln(), self.minimum.ln(), self.maximum.ln())
        } else {
            (value, self.minimum, self.maximum)
        };
        let t = (v - min) / (max - min);
        t.is_finite()
            .then_some(if self.reversed { 1. - t } else { t })
    }
    /// Return major tick values in ascending numeric order.
    pub fn ticks(self) -> Vec<f64> {
        if let Some(base) = self.log_base {
            let log = base.ln();
            let first = (self.minimum.ln() / log - 1e-12).ceil() as i32;
            let last = (self.maximum.ln() / log + 1e-12).floor() as i32;
            let values = (first..=last)
                .take(1001)
                .map(|k| base.powi(k))
                .filter(|v| {
                    v.is_finite()
                        && *v > 0.
                        && *v >= self.minimum * (1. - 1e-12)
                        && *v <= self.maximum * (1. + 1e-12)
                })
                .collect::<Vec<_>>();
            if values.is_empty() {
                vec![self.minimum, self.maximum]
            } else {
                values
            }
        } else {
            (0..=(((self.maximum - self.minimum) / self.step + 1e-9).floor() as usize).min(1000))
                .map(|k| self.minimum + k as f64 * self.step)
                .collect()
        }
    }
}

fn logarithmic_scale(
    low: f64,
    high: f64,
    axis: &calc_doc_model::ChartAxis,
) -> Result<ChartScale, String> {
    let base = axis
        .log_base
        .as_deref()
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite() && (2. ..=1000.).contains(v))
        .ok_or("Use a logarithmic base from 2 to 1,000")?;
    if low <= 0. || !low.is_finite() || !high.is_finite() || high < low {
        return Err("A logarithmic axis needs positive data values".into());
    }
    let bound = |v: Option<&str>| -> Result<Option<f64>, String> {
        v.map(|v| {
            v.parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && *v > 0.)
                .ok_or_else(|| "Logarithmic bounds must be positive finite numbers".into())
        })
        .transpose()
    };
    let minimum = bound(axis.minimum.as_deref())?;
    let maximum = bound(axis.maximum.as_deref())?;
    let log = base.ln();
    let auto_min = base.powf((low.ln() / log).floor());
    let auto_max = base.powf((high.ln() / log).ceil());
    let mut min = minimum.unwrap_or(if auto_min > 0. && auto_min.is_finite() {
        auto_min
    } else {
        low
    });
    let mut max = maximum.unwrap_or(if auto_max > 0. && auto_max.is_finite() {
        auto_max
    } else {
        high
    });
    if maximum.is_none() && max <= min {
        max = (min * base).min(f64::MAX);
    }
    if minimum.is_none() && min >= max {
        min = max / base;
    }
    let intervals = (max.ln() - min.ln()) / log;
    if !min.is_finite()
        || min <= 0.
        || !max.is_finite()
        || max <= min
        || !intervals.is_finite()
        || intervals <= 0.
        || intervals > 1000.
    {
        return Err("Use positive logarithmic bounds with at most 1,000 intervals".into());
    }
    Ok(ChartScale {
        minimum: min,
        maximum: max,
        step: base,
        reversed: axis.reversed,
        log_base: Some(base),
    })
}

/// The value extent of a category plot, including separate positive/negative stacks.
pub fn plot_value_range(plot: &PlotView) -> (f64, f64) {
    let (mut low, mut high) = (0f64, 0f64);
    if plot.stacked {
        let count = plot
            .series
            .iter()
            .map(|s| s.values.len())
            .max()
            .unwrap_or(0);
        for j in 0..count {
            let (mut up, mut down) = (0., 0.);
            for s in &plot.series {
                let v = s.values.get(j).copied().flatten().unwrap_or(0.);
                if v >= 0. { up += v } else { down += v }
            }
            low = low.min(down);
            high = high.max(up);
        }
    } else {
        for v in plot.series.iter().flat_map(|s| s.values.iter().flatten()) {
            low = low.min(*v);
            high = high.max(*v);
        }
    }
    (low, high)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_axis(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    part: &str,
    id: u32,
    minimum: Option<String>,
    maximum: Option<String>,
    major_unit: Option<String>,
    reversed: bool,
) -> Result<Vec<crate::CellEdit>, String> {
    prepare_properties(host, sheet, revision, part, None, false, None)?;
    let mut charts = host.model().definitions().charts(sheet).to_vec();
    let chart = charts
        .iter_mut()
        .find(|c| c.part == part)
        .ok_or("The chart is unavailable")?;
    check_axes_for_edit(host, sheet, part)?;
    if chart.axes.iter().filter(|a| a.id == id).count() != 1 {
        return Err("The axis identity changed; reopen its controls (dc-zjsg)".into());
    }
    value_axes(
        &chart.axes,
        &chart.plots[0].axis_ids,
        chart.plots[0].kind == "scatterChart",
    )
    .map_err(|e| format!("{e} (dc-zjsg)"))?;
    let axis = chart
        .axes
        .iter_mut()
        .find(|a| a.id == id && a.kind == "valAx")
        .ok_or("Choose a value axis (dc-zjsg)")?;
    let canonical = |value: Option<String>| -> Result<Option<String>, String> {
        value
            .filter(|s| !s.trim().is_empty())
            .map(|s| {
                s.trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .map(|n| n.to_string())
                    .ok_or_else(|| "Use finite numeric axis bounds and units (dc-zjsg)".into())
            })
            .transpose()
    };
    axis.minimum = canonical(minimum)?;
    axis.maximum = canonical(maximum)?;
    axis.major_unit = canonical(major_unit)?;
    axis.reversed = reversed;
    validate_axis_view(host, sheet, part, axis).map_err(|e| format!("{e} (dc-zjsg)"))?;
    host.loaded()
        .check_chart_properties(sheet, chart)
        .map_err(|e| e.to_string())?;
    if charts == host.model().definitions().charts(sheet) {
        return Ok(Vec::new());
    }
    Ok(vec![crate::CellEdit::Charts { sheet, charts }])
}

/// Report whether the plot axis references account for every chart axis and the chart axis IDs are unique.
pub fn axes_associated(chart: &ChartView) -> bool {
    let actual = chart
        .axes
        .iter()
        .map(|a| a.id)
        .collect::<std::collections::BTreeSet<_>>();
    let used = chart
        .plots
        .iter()
        .flat_map(|p| p.axis_ids.iter().copied())
        .collect::<std::collections::BTreeSet<_>>();
    actual.len() == chart.axes.len() && actual == used
}
fn check_axes_for_edit(host: &HostState, sheet: SheetId, part: &str) -> Result<(), String> {
    let view = host
        .charts(sheet)
        .0
        .into_iter()
        .find(|c| c.part == part)
        .ok_or("The chart is unavailable")?;
    if !axes_associated(&view) || (view.plots.len() != 1 && !combination_supported(&view)) {
        return Err(
            "Editing unassociated axes or this combination is not modelled (dc-zjsg)".into(),
        );
    }
    Ok(())
}
/// Report whether the chart uses a supported combination of plots and axis pairs.
pub fn combination_supported(chart: &ChartView) -> bool {
    if !axes_associated(chart) {
        return false;
    }
    if !(2..=3).contains(&chart.plots.len())
        || chart
            .plots
            .iter()
            .enumerate()
            .any(|(i, p)| chart.plots[..i].iter().any(|before| before.kind == p.kind))
        || chart.plots.iter().any(|p| {
            !matches!(
                p.kind,
                ChartKind::Column | ChartKind::Line | ChartKind::Area
            ) || p.percent
                || p.series.is_empty()
        })
    {
        return false;
    }
    if !matches!(chart.axes.len(), 2 | 4) {
        return false;
    }
    let Ok(pairs) = chart
        .plots
        .iter()
        .map(|p| value_axes(&chart.axes, &p.axis_ids, false))
        .collect::<Result<Vec<_>, _>>()
    else {
        return false;
    };
    // Excel writes a combination as area, bar, line, so the plot on the primary axes is not always first.
    if !pairs
        .iter()
        .any(|pair| pair[0].position.as_deref() == Some("l"))
    {
        return false;
    }
    let mut sides = std::collections::BTreeMap::new();
    for pair in pairs {
        let Some(side @ ("l" | "r")) = pair[0].position.as_deref() else {
            return false;
        };
        if sides
            .insert(side, pair[0].id)
            .is_some_and(|id| id != pair[0].id)
        {
            return false;
        }
    }
    sides.len() == chart.axes.len() / 2
}
/// Plots sharing a value axis share its data extent as well as its authored properties.
pub fn category_scale(chart: &ChartView, plot: &PlotView) -> Result<ChartScale, String> {
    if !axes_associated(chart) {
        return Err("Unassociated chart axes are preserved but not drawn here (dc-zjsg)".into());
    }
    let axes = value_axes(&chart.axes, &plot.axis_ids, false)?;
    let axis = axes[0];
    let log = axis.log_base.is_some();
    let mut extent: Option<(f64, f64)> = None;
    for p in chart.plots.iter().filter(|p| p.axis_ids.contains(&axis.id)) {
        let (low, high) = plot_scale_range(p, log)?;
        if log && low <= 0. {
            continue;
        }
        extent = Some(extent.map_or((low, high), |(a, b)| (a.min(low), b.max(high))));
    }
    let (low, high) = extent.unwrap_or((0., 0.));
    chart_scale(low, high, Some(axis))
}

/// Positive extents exclude values that cannot be represented on a logarithmic axis.
pub fn plot_scale_range(plot: &PlotView, log: bool) -> Result<(f64, f64), String> {
    if !log {
        return Ok(plot_value_range(plot));
    }
    if plot.stacked {
        return Err(
            "Logarithmic stacked plots are preserved but not drawn or edited here (dc-zjsg)"
                .into(),
        );
    }
    Ok(positive_range(
        plot.series
            .iter()
            .flat_map(|s| s.values.iter().flatten().copied()),
    ))
}
fn positive_range(values: impl Iterator<Item = f64>) -> (f64, f64) {
    let mut range: Option<(f64, f64)> = None;
    for v in values.filter(|v| v.is_finite() && *v > 0.) {
        range = Some(range.map_or((v, v), |(a, b)| (a.min(v), b.max(v))));
    }
    range.unwrap_or((0., 0.))
}
/// Return the X and Y extents of paired scatter points, excluding nonpositive coordinates on logarithmic axes.
pub fn scatter_scale_ranges(plot: &PlotView, xlog: bool, ylog: bool) -> ((f64, f64), (f64, f64)) {
    if !xlog && !ylog {
        return scatter_ranges(plot);
    }
    let points = plot
        .series
        .iter()
        .flat_map(|s| {
            scatter_x_values(s)
                .into_iter()
                .zip(&s.values)
                .filter_map(|(x, y)| x.zip(*y))
        })
        .filter(|(x, y)| (!xlog || *x > 0.) && (!ylog || *y > 0.))
        .collect::<Vec<_>>();
    let extent = |x: bool, log: bool| {
        let values = points.iter().map(|(a, b)| if x { *a } else { *b });
        if log {
            positive_range(values)
        } else {
            values.fold((0f64, 0f64), |(a, b), v| (a.min(v), b.max(v)))
        }
    };
    (extent(true, xlog), extent(false, ylog))
}
fn validate_axis_view(
    host: &HostState,
    sheet: SheetId,
    part: &str,
    axis: &calc_doc_model::ChartAxis,
) -> Result<(), String> {
    axis_view_scale(host, sheet, part, axis).map(|_| ())
}

/// The scale the renderer draws for `axis` of the chart `part` on `sheet` (the automatic one where
/// the axis leaves a setting automatic): what Excel's `Axis.MinimumScale` reads (dc-oxh8w.13).
pub fn axis_scale(host: &HostState, sheet: SheetId, part: &str, axis_id: u32) -> Result<ChartScale, String> {
    let view = host
        .charts(sheet)
        .0
        .into_iter()
        .find(|v| v.part == part)
        .ok_or("The chart cannot be read")?;
    let axis = view.axes.iter().find(|a| a.id == axis_id).ok_or("The axis is unavailable")?.clone();
    axis_view_scale(host, sheet, part, &axis)
}

fn axis_view_scale(
    host: &HostState,
    sheet: SheetId,
    part: &str,
    axis: &calc_doc_model::ChartAxis,
) -> Result<ChartScale, String> {
    let mut view = host
        .charts(sheet)
        .0
        .into_iter()
        .find(|v| v.part == part)
        .ok_or("The chart cannot be read")?;
    *view
        .axes
        .iter_mut()
        .find(|a| a.id == axis.id)
        .ok_or("The axis is unavailable")? = axis.clone();
    let plot = view
        .plots
        .iter()
        .find(|p| p.axis_ids.contains(&axis.id))
        .ok_or("The axis is not associated with a plot (dc-zjsg)")?;
    let (low, high) = if plot.kind == ChartKind::Scatter {
        let axes = value_axes(&view.axes, &plot.axis_ids, true)?;
        let x = if axes[0].id == axis.id { axis } else { axes[0] };
        let y = if axes[1].id == axis.id { axis } else { axes[1] };
        let (xr, yr) = scatter_scale_ranges(plot, x.log_base.is_some(), y.log_base.is_some());
        if x.id == axis.id { xr } else { yr }
    } else if matches!(
        plot.kind,
        ChartKind::Column | ChartKind::Bar | ChartKind::Line | ChartKind::Area
    ) {
        return category_scale(&view, plot);
    } else {
        return Err("This plot has no editable value axis (dc-zjsg)".into());
    };
    chart_scale(low, high, Some(axis))
}
/// Axis title and tick-label format edits (dc-oxh8w.12): the retained writer saves both.
pub(crate) fn prepare_axis_labels(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    part: &str,
    id: u32,
    title: Option<Option<String>>,
    number_format: Option<Option<String>>,
) -> Result<Vec<crate::CellEdit>, String> {
    prepare_properties(host, sheet, revision, part, None, false, None)?;
    let mut charts = host.model().definitions().charts(sheet).to_vec();
    let chart = charts
        .iter_mut()
        .find(|c| c.part == part)
        .ok_or("The chart is unavailable")?;
    check_axes_for_edit(host, sheet, part)?;
    let axis = chart
        .axes
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or("The axis identity changed; reopen its controls (dc-zjsg)")?;
    if let Some(title) = title {
        axis.title = title;
    }
    if let Some(format) = number_format {
        axis.number_format = format;
    }
    host.loaded()
        .check_chart_properties(sheet, chart)
        .map_err(|e| e.to_string())?;
    if charts == host.model().definitions().charts(sheet) {
        return Ok(Vec::new());
    }
    Ok(vec![crate::CellEdit::Charts { sheet, charts }])
}
pub(crate) fn prepare_log_axis(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    part: &str,
    id: u32,
    base: Option<String>,
) -> Result<Vec<crate::CellEdit>, String> {
    prepare_properties(host, sheet, revision, part, None, false, None)?;
    let mut charts = host.model().definitions().charts(sheet).to_vec();
    let chart = charts
        .iter_mut()
        .find(|c| c.part == part)
        .ok_or("The chart is unavailable")?;
    check_axes_for_edit(host, sheet, part)?;
    value_axes(
        &chart.axes,
        &chart.plots[0].axis_ids,
        chart.plots[0].kind == "scatterChart",
    )?;
    let axis = chart
        .axes
        .iter_mut()
        .find(|a| a.id == id && a.kind == "valAx")
        .ok_or("Choose a value axis (dc-zjsg)")?;
    axis.log_base = base
        .map(|v| {
            v.trim()
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite() && (2. ..=1000.).contains(n))
                .map(|n| n.to_string())
                .ok_or_else(|| "Use a logarithmic base from 2 to 1,000 (dc-zjsg)".to_string())
        })
        .transpose()?;
    validate_axis_view(host, sheet, part, axis).map_err(|e| format!("{e} (dc-zjsg)"))?;
    host.loaded()
        .check_chart_properties(sheet, chart)
        .map_err(|e| e.to_string())?;
    if charts == host.model().definitions().charts(sheet) {
        return Ok(Vec::new());
    }
    Ok(vec![crate::CellEdit::Charts { sheet, charts }])
}

/// Associated primary/secondary axis pairs cross each other. Scatter returns X then Y.
pub fn value_axes<'a>(
    axes: &'a [calc_doc_model::ChartAxis],
    ids: &[u32],
    scatter: bool,
) -> Result<Vec<&'a calc_doc_model::ChartAxis>, String> {
    let invalid =
        || "Unassociated chart axes are preserved but not drawn or edited here".to_owned();
    if !matches!(axes.len(), 2 | 4)
        || ids.len() != 2
        || ids[0] == ids[1]
        || axes
            .iter()
            .map(|a| a.id)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != axes.len()
    {
        return Err(invalid());
    }
    let a = axes.iter().find(|a| a.id == ids[0]).ok_or_else(invalid)?;
    let b = axes.iter().find(|a| a.id == ids[1]).ok_or_else(invalid)?;
    if a.cross_axis != Some(b.id) || b.cross_axis != Some(a.id) {
        return Err(invalid());
    }
    let horizontal =
        |a: &calc_doc_model::ChartAxis| matches!(a.position.as_deref(), Some("b" | "t"));
    let vertical = |a: &calc_doc_model::ChartAxis| matches!(a.position.as_deref(), Some("l" | "r"));
    if !((horizontal(a) && vertical(b)) || (horizontal(b) && vertical(a))) {
        return Err(invalid());
    }
    if scatter {
        if a.kind != "valAx" || b.kind != "valAx" {
            return Err(invalid());
        }
        Ok(if horizontal(a) {
            vec![a, b]
        } else {
            vec![b, a]
        })
    } else {
        let (value, category) = if a.kind == "valAx" { (a, b) } else { (b, a) };
        if value.kind != "valAx" || !matches!(category.kind.as_str(), "catAx" | "dateAx") {
            return Err(invalid());
        }
        Ok(vec![value])
    }
}
/// Return numeric X coordinates, using sequential one-based point positions if any X is missing or nonnumeric.
pub fn scatter_x_values(series: &SeriesView) -> Vec<Option<f64>> {
    let xs = series
        .categories
        .iter()
        .map(|v| v.as_ref().and_then(|v| v.as_number()))
        .collect::<Vec<_>>();
    if xs.is_empty() || xs.iter().any(Option::is_none) {
        (1..=series.values.len()).map(|n| Some(n as f64)).collect()
    } else {
        xs
    }
}
/// Return linear X and Y extents of paired scatter points, including zero on both axes.
pub fn scatter_ranges(plot: &PlotView) -> ((f64, f64), (f64, f64)) {
    let (mut xl, mut xh, mut yl, mut yh) = (0f64, 0f64, 0f64, 0f64);
    for series in &plot.series {
        for (x, y) in scatter_x_values(series).iter().zip(&series.values) {
            if let (Some(x), Some(y)) = (x, y) {
                xl = xl.min(*x);
                xh = xh.max(*x);
                yl = yl.min(*y);
                yh = yh.max(*y);
            }
        }
    }
    ((xl, xh), (yl, yh))
}

/// Convert supported 2-D plots without replacing their series identities or source references.
pub(crate) fn prepare_type(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    part: &str,
    plot: u32,
    kind: &str,
    stacked: bool,
) -> Result<Vec<crate::CellEdit>, String> {
    prepare_properties(host, sheet, revision, part, None, false, None)?;
    let mut charts = host.model().definitions().charts(sheet).to_vec();
    let chart = charts
        .iter_mut()
        .find(|c| c.part == part)
        .ok_or("The chart is unavailable")?;
    check_axes_for_edit(host, sheet, part)?;
    if chart.plots.len() > 3 || plot as usize >= chart.plots.len() {
        return Err("Choose a plot in a single chart or a category combination of up to three plots (dc-zjsg)".into());
    }
    let combo = chart.plots.len() > 1;
    if combo && kind == "bar" {
        return Err("Combination type changes use vertical plots (dc-zjsg)".into());
    }
    if combo
        && stacked
        && chart.axes.iter().any(|a| {
            a.kind == "valAx"
                && a.position.as_deref() == Some("r")
                && chart.plots[plot as usize].axis_ids.contains(&a.id)
        })
    {
        return Err("Changing secondary-axis combination plots to stacked grouping is not modelled (dc-zjsg)".into());
    }
    if stacked
        && chart
            .axes
            .iter()
            .any(|a| a.log_base.is_some() && chart.plots[plot as usize].axis_ids.contains(&a.id))
    {
        return Err("Stacked logarithmic plots are not modelled (dc-zjsg)".into());
    }
    let p = &mut chart.plots[plot as usize];
    let category = |kind: &str| matches!(kind, "barChart" | "lineChart" | "areaChart");
    let pie = |kind: &str| matches!(kind, "pieChart" | "doughnutChart");
    let (next, direction) = match kind {
        "column" => ("barChart", Some("col")),
        "bar" => ("barChart", Some("bar")),
        "line" => ("lineChart", None),
        "area" => ("areaChart", None),
        "pie" => ("pieChart", None),
        "doughnut" => ("doughnutChart", None),
        _ => return Err("Choose a supported two-dimensional chart type (dc-zjsg)".into()),
    };
    if category(&p.kind) && category(next) {
        value_axes(&chart.axes, &p.axis_ids, false).map_err(|e| format!("{e} (dc-zjsg)"))?;
        let horizontal = p.kind == "barChart" && p.bar_direction.as_deref() == Some("bar");
        if !combo && horizontal != (kind == "bar") {
            for axis in &mut chart.axes {
                axis.position = axis.position.as_deref().map(|pos| {
                    match pos {
                        "b" => "l",
                        "t" => "r",
                        "l" => "b",
                        "r" => "t",
                        other => other,
                    }
                    .to_owned()
                });
            }
        }
    } else if !(pie(&p.kind) && pie(next)) {
        return Err(
            "Convert within column/bar/line/area, or between pie and doughnut; scatter and cross-family conversions are not modelled (dc-zjsg)".into(),
        );
    }
    if pie(next) && p.series.len() != 1 {
        return Err(
            "Changing multi-series pie or doughnut charts is not modelled (dc-zjsg)".into(),
        );
    }
    if pie(next) && stacked {
        return Err("Pie and doughnut charts do not have stacked grouping".into());
    }
    p.kind = next.into();
    p.bar_direction = direction.map(str::to_owned);
    p.grouping = category(next).then(|| {
        if stacked {
            "stacked"
        } else if next == "barChart" {
            "clustered"
        } else {
            "standard"
        }
        .into()
    });
    host.loaded()
        .check_chart_properties(sheet, chart)
        .map_err(|e| e.to_string())?;
    if charts == host.model().definitions().charts(sheet) {
        return Ok(Vec::new());
    }
    Ok(vec![crate::CellEdit::Charts { sheet, charts }])
}

pub(crate) fn cell_a1(at: CellKey) -> String {
    let mut n = at.column();
    let mut letters = String::new();
    while n > 0 {
        n -= 1;
        letters.insert(0, (b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    format!("{letters}{}", at.row())
}
/// A sheet name as Excel writes it in a formula: quoted only when it has to be.
pub(crate) fn excel_reference(canonical: &str) -> String {
    let Some(rest) = canonical.strip_prefix('\'') else {
        return canonical.to_owned();
    };
    let Some(end) = rest.find("'!") else {
        return canonical.to_owned();
    };
    let (name, tail) = rest.split_at(end);
    let word = name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        && name.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_');
    let letters = name.chars().take_while(char::is_ascii_alphabetic).count();
    let cell_like = (1..=3).contains(&letters)
        && letters < name.len()
        && name[letters..].chars().all(|c| c.is_ascii_digit());
    let r1c1 = name
        .strip_prefix(['R', 'r'])
        .is_some_and(|r| r.chars().all(|c| c.is_ascii_digit() || matches!(c, 'C' | 'c')));
    if word && !cell_like && !r1c1 && !name.contains("''") {
        format!("{name}{}", &tail[1..])
    } else {
        canonical.to_owned()
    }
}

fn range_data(
    host: &HostState,
    origin: CellKey,
    text: &str,
    numeric: bool,
    single: bool,
) -> Result<ChartData, String> {
    let text = text.trim().trim_start_matches('=').trim();
    let crate::ReferenceTarget::Area(area) = host.resolve_reference_text(origin, text) else {
        return Err(
            "Use a single worksheet cell or a one-row or one-column A1 range (dc-zjsg)".into(),
        );
    };
    let count = u64::from(area.row_count()) * u64::from(area.column_count());
    if count > crate::validation::LIST_LIMIT as u64
        || (area.row_count() > 1 && area.column_count() > 1)
        || (single && count != 1)
    {
        return Err("Chart ranges must be one row or column with at most 32,767 cells; a name reference must be one cell (dc-zjsg)".into());
    }
    // Canonical absolute references keep edits independent of the selected cell.
    let sheet_name = host
        .model()
        .sheet_by_id(area.sheet())
        .ok_or("The source worksheet is unavailable")?
        .name()
        .text()
        .to_string_lossy()
        .replace('\'', "''");
    let a1 = |at: CellKey| {
        let mut n = at.column();
        let mut letters = String::new();
        while n > 0 {
            n -= 1;
            letters.insert(0, (b'A' + (n % 26) as u8) as char);
            n /= 26;
        }
        format!("${letters}${}", at.row())
    };
    let first = a1(area.first());
    let last = a1(area.last());
    Ok(ChartData {
        // Excel writes a sheet name quoted only when it has to be (Q0).
        formula: Some(excel_reference(&format!(
            "'{sheet_name}'!{first}{}",
            if first == last {
                String::new()
            } else {
                format!(":{last}")
            }
        ))),
        cache: Vec::new(),
        numeric,
    })
}
/// Build column- or row-oriented series from one selection, with explicit header/category choices.
/// What frames a chart a command makes: its anchor, its identity and its title and legend.
pub(crate) struct NewChart {
    pub anchor: calc_doc_model::ChartAnchor,
    /// The shape identity a macro's chart takes (its name is `Chart <id - 1>`); `None` for the
    /// interface's, which takes the first free `Chart N`.
    pub drawing_id: Option<u32>,
    /// The first `c16:uniqueId` counter of a macro's chart (Excel, dc-piw0.10 Q17: 0 for
    /// `Shapes.AddChart2`; for `ChartObjects.Add` the value is not yet explained, 2 stands as before).
    pub unique_start: u32,
    pub title: Option<String>,
    /// `c:autoTitleDeleted`: false lets Excel title a one-series chart with its series name.
    pub auto_title_deleted: bool,
    pub legend: bool,
}

/// The frame of the chart on a chart sheet: an absolute anchor over the sheet (dc-oxh8w.23; the size Excel
/// writes for a new chart sheet at the default page, in EMU).
pub(crate) const CHART_SHEET_FRAME: calc_doc_model::ChartAnchor =
    calc_doc_model::ChartAnchor::Absolute { x: 0, y: 0, width: 9_293_678, height: 6_000_000 };

/// Whether the sheet is a chart sheet.
pub(crate) fn is_chart_sheet(host: &HostState, sheet: SheetId) -> bool {
    host.model().sheet_by_id(sheet).is_some_and(|s| s.kind() == calc_doc_model::SheetKind::Chart)
}

fn editable(host: &HostState, sheet: SheetId, fail: &dyn Fn(&str) -> String) -> Result<(), String> {
    // A chart sheet a macro made has no loaded part yet; the saver writes it from the model.
    let source = host.loaded().sheet(sheet);
    if source.is_none() && !is_chart_sheet(host, sheet) {
        return Err(fail("Save and reopen the new worksheet before creating a chart"));
    }
    if source.is_some_and(|s| s.protected() || s.is_macro_sheet()) {
        return Err(fail("Creating charts requires an editable worksheet"));
    }
    if host.loaded().workbook_part().ends_with(".bin") {
        return Err(fail("Save as XLSX and reopen before creating charts"));
    }
    if !host.axis_edits.is_empty() {
        return Err(fail(
            "Save and reopen before creating charts after row or column insertion or deletion",
        ));
    }
    Ok(())
}

/// The series a source range holds: along rows or columns, with the first row or column as the
/// series names and/or the categories. Returns them with the number of series.
fn make_series(
    host: &HostState,
    area: calc_value::GridArea,
    scatter: bool,
    rows: bool,
    headers: u32,
    categories: u32,
    fail: &dyn Fn(&str) -> String,
) -> Result<Vec<calc_doc_model::ChartSeries>, String> {
    use calc_doc_model::ChartSeries;
    let sheet = area.sheet();
    let (length, width) = if rows {
        (area.column_count(), area.row_count())
    } else {
        (area.row_count(), area.column_count())
    };
    let points = length
        .checked_sub(headers)
        .filter(|n| *n > 0)
        .ok_or_else(|| fail("Select at least one data point after the series names"))?;
    let count = width
        .checked_sub(categories)
        .filter(|n| *n > 0)
        .ok_or_else(|| fail("Select at least one values series after the categories"))?;
    if count > 255 || points > 32767 || u64::from(points) * u64::from(count) > 262144 {
        return Err(fail(
            "Use at most 255 series, 32,767 points per series and 262,144 points in total",
        ));
    }
    let cell = |major: u32, minor: u32| {
        CellKey::new(
            sheet,
            area.first().row() + if rows { minor } else { major },
            area.first().column() + if rows { major } else { minor },
        )
        .unwrap()
    };
    let data = |first: CellKey, last: CellKey, numeric: bool| {
        range_data(
            host,
            area.first(),
            &format!("{}:{}", cell_a1(first), cell_a1(last)),
            numeric,
            false,
        )
    };
    // A scatter chart's X values are numbers, unless they are text, which Excel keeps as text
    // (`c:strRef`) and plots as 1, 2, 3 (dc-piw0.10 Q17).
    let text_x = host
        .model()
        .cell(cell(headers, 0))
        .is_some_and(|c| matches!(c, crate::CellInput::Text(_)));
    // Several category lines over several points are multi-level categories (not modelled).
    if categories > 1 && points > 1 {
        return Err(fail("Multi-level categories are not modelled"));
    }
    let cats = if categories > 0 {
        Some(data(
            cell(headers, 0),
            cell(length - 1, categories - 1),
            scatter && !text_x,
        )?)
    } else {
        None
    };
    let mut series = Vec::new();
    for i in 0..count {
        let minor = i + categories;
        let name = if headers > 0 {
            Some(data(cell(0, minor), cell(headers - 1, minor), false)?)
        } else {
            None
        };
        series.push(ChartSeries {
            index: i,
            show_values: None,
            outer_levels: Vec::new(),
            value_labels_source_linked: false,
            name,
            categories: cats.clone(),
            values: Some(data(cell(headers, minor), cell(length - 1, minor), true)?),
            fill: None,
            line: Default::default(),
            unique_id: None,
            bubble_sizes: None,
            trendlines: Vec::new(),
            error_bars: Vec::new(),
            labels: None,
        });
    }
    Ok(series)
}

pub(crate) fn prepare_create(
    host: &HostState,
    revision: u64,
    area: calc_value::GridArea,
    kind: &str,
    rows: bool,
    headers: bool,
    categories: bool,
) -> Result<Vec<crate::CellEdit>, String> {
    let fail = |s: &str| format!("{s} (dc-zjsg)");
    if revision != host.revision().get() {
        return Err(fail("The workbook changed; select the chart data again"));
    }
    let sheet = area.sheet();
    editable(host, sheet, &fail)?;
    let scatter = kind == "scatter";
    let series = make_series(
        host,
        area,
        scatter,
        rows,
        u32::from(headers),
        u32::from(categories),
        &fail,
    )?;
    let count = series.len() as u32;
    assemble(
        host,
        sheet,
        kind,
        series,
        count,
        categories,
        NewChart {
            anchor: ChartAnchor::OneCell {
                from: DrawingPoint {
                    column: area.last().column().saturating_add(1).min(16383),
                    row: area.first().row() - 1,
                    ..Default::default()
                },
                width: 640 * 9525,
                height: 360 * 9525,
            },
            drawing_id: None,
            unique_start: 0,
            title: None,
            auto_title_deleted: true,
            legend: true,
        },
        &fail,
    )
}

/// A chart without data, as `ChartObjects.Add` and `Shapes.AddChart2` make one (dc-piw0.2): the
/// chart type's plot and axes with no series, framed by `new`.
pub(crate) fn prepare_create_empty(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    kind: &str,
    new: NewChart,
) -> Result<Vec<crate::CellEdit>, String> {
    let fail = |s: &str| format!("{s} (dc-piw0)");
    if revision != host.revision().get() {
        return Err(fail("The workbook changed; read the sheet again"));
    }
    editable(host, sheet, &fail)?;
    assemble(host, sheet, kind, Vec::new(), 0, false, new, &fail)
}

/// Whether the file the workbook was opened from holds the chart part (on any sheet).
fn file_held(host: &HostState, part: &str) -> bool {
    host.loaded().model().definitions().all_charts().any(|(_, held)| held.iter().any(|c| c.part == part))
}

/// `Chart.Location`: a chart this session made moves from `sheet` to `to`, a chart sheet (one chart, the sheet's
/// absolute frame) or a worksheet (a two-cell frame at `place` in points), keeping its part, plots and axes.
pub(crate) fn prepare_move(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    part: &str,
    to: SheetId,
    place: [f64; 4],
) -> Result<Vec<crate::CellEdit>, String> {
    let fail = |s: &str| format!("{s} (dc-oxh8w.3)");
    if revision != host.revision().get() {
        return Err(fail("The workbook changed; read the chart again"));
    }
    let mut source = host.model().definitions().charts(sheet).to_vec();
    let index = source.iter().position(|c| c.part == part).ok_or_else(|| fail("The chart no longer exists"))?;
    let file_held = file_held(host, part);
    if sheet == to {
        return Err(fail("The chart is on that sheet"));
    }
    editable(host, to, &fail)?;
    let mut chart = source.remove(index);
    let mut target = host.model().definitions().charts(to).to_vec();
    let id = host.next_drawing_id(to);
    if is_chart_sheet(host, to) {
        if !target.is_empty() {
            return Err(fail("A chart sheet holds one chart"));
        }
        chart.anchor = CHART_SHEET_FRAME;
    } else {
        let [left, top, width, height] = place;
        if ![left, top, width, height].into_iter().all(|v| v.is_finite() && (0.0..=1.0e7).contains(&v)) {
            return Err(fail("Use a position and a size on the worksheet"));
        }
        chart.anchor = calc_doc_model::ChartAnchor::TwoCell {
            from: host.point_at(to, left, top),
            to: host.point_at(to, left + width, top + height),
        };
    }
    // A chart the file holds keeps its name and its part: its frame is written anew on the sheet that receives it.
    if !file_held {
        chart.drawing_id = Some(id);
        chart.name = format!("Chart {}", id - 1);
    }
    target.push(chart);
    host.loaded().check_chart_set_on(to, &target, is_chart_sheet(host, to)).map_err(|e| e.to_string())?;
    Ok(vec![
        crate::CellEdit::Charts { sheet, charts: source },
        crate::CellEdit::Charts { sheet: to, charts: target },
    ])
}

/// `Chart.SetSourceData`: the series of a chart this session made become those of `area`, the
/// chart keeping its type, frame, name, title and legend.
pub(crate) fn prepare_set_source(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    part: &str,
    area: calc_value::GridArea,
    rows: bool,
    headers: u32,
    categories: u32,
) -> Result<Vec<crate::CellEdit>, String> {
    let fail = |s: &str| format!("{s} (dc-piw0)");
    if revision != host.revision().get() {
        return Err(fail("The workbook changed; read the chart again"));
    }
    let mut charts = host.model().definitions().charts(sheet).to_vec();
    let index = charts
        .iter()
        .position(|c| c.part == part)
        .ok_or_else(|| fail("The chart no longer exists"))?;
    if file_held(host, part) {
        return Err(fail("The series of a chart the file holds are not replaced yet"));
    }
    if charts[index].plots.len() != 1 {
        return Err(fail("Only a chart with one plot takes new source data"));
    }
    let scatter = charts[index].plots[0].kind == "scatterChart";
    // A single empty cell leaves the chart without series (Excel Q6, Q21 e5); an empty range of
    // more cells reads its first row and column as labels and plots the rest (Q21 c12, e1-e4).
    let model = host.model();
    let single_blank = area.cell_count() == 1 && model.cell(area.first()).is_none();
    let series = if single_blank {
        Vec::new()
    } else {
        make_series(host, area, scatter, rows, headers, categories, &fail)?
    };
    // Excel keeps every series of a source in a pie or doughnut part (Q17); the ring or circle shows the first.
    if scatter && categories == 0 && !series.is_empty() {
        return Err(fail("Use the first column or row as X values for a scatter chart"));
    }
    // Excel (dc-piw0.10 Q17): the series a re-plot reuses keep their uniqueId; each new one takes the
    // chart's next counter, which never goes back.
    let mut series = series;
    let old: Vec<Option<u32>> = charts[index].plots[0].series.iter().map(|s| s.unique_id).collect();
    if charts[index].drawing_id.is_some() {
        for (i, s) in series.iter_mut().enumerate() {
            s.unique_id = old.get(i).copied().flatten().or_else(|| {
                let id = charts[index].unique_next;
                charts[index].unique_next += 1;
                Some(id)
            });
        }
    }
    charts[index].plots[0].series = series;
    host.loaded()
        .check_chart_set_on(sheet, &charts, is_chart_sheet(host, sheet))
        .map_err(|e| e.to_string())?;
    Ok(vec![crate::CellEdit::Charts { sheet, charts }])
}

fn assemble(
    host: &HostState,
    sheet: SheetId,
    kind: &str,
    series: Vec<calc_doc_model::ChartSeries>,
    count: u32,
    categories: bool,
    new: NewChart,
    fail: &dyn Fn(&str) -> String,
) -> Result<Vec<crate::CellEdit>, String> {
    use calc_doc_model::{Chart, ChartAxis, ChartPlot};
    // UI30 P10: `column-percent`, `bar-percent`, `line-percent` and `area-percent` are the
    // 100% stacked plots (grouping percentStacked) of the same kinds.
    let (kind, percent) = match kind.strip_suffix("-percent") {
        Some(base @ ("column" | "bar" | "line" | "area")) => (base, true),
        _ => (kind, false),
    };
    let (plot_kind, dir) = match kind {
        "column" | "combo" | "combo-three" | "combo-column-area" => ("barChart", Some("col")),
        "combo-line-column" | "combo-line-area" => ("lineChart", None),
        "combo-area-line" | "combo-area-column" => ("areaChart", None),
        "bar" => ("barChart", Some("bar")),
        "line" => ("lineChart", None),
        "area" => ("areaChart", None),
        "pie" => ("pieChart", None),
        "doughnut" => ("doughnutChart", None),
        "scatter" => ("scatterChart", None),
        _ => return Err(fail("Choose a supported two-dimensional chart type")),
    };
    let combo = matches!(
        kind,
        "combo"
            | "combo-three"
            | "combo-column-area"
            | "combo-line-column"
            | "combo-line-area"
            | "combo-area-line"
            | "combo-area-column"
    );
    if combo && count < 2 && count > 0 {
        return Err(fail(
            "Select at least two values series for a category combination (dc-zjsg)",
        ));
    }
    if kind == "combo-three" && count < 3 && count > 0 {
        return Err(fail(
            "Select at least three values series for column, line and area plots",
        ));
    }
    let pie = matches!(kind, "pie" | "doughnut");
    if pie && count != 1 && count > 0 {
        return Err(fail("Select one values series for a pie or doughnut"));
    }
    if kind == "scatter" && !categories && count > 0 {
        return Err(fail(
            "Use the first column or row as X values for a scatter chart",
        ));
    }
    let mut axes = if pie {
        Vec::new()
    } else {
        vec![
            ChartAxis {
                id: 1,
                cross_axis: Some(2),
                kind: if kind == "scatter" { "valAx" } else { "catAx" }.into(),
                position: Some(if kind == "bar" { "l" } else { "b" }.into()),
                ..Default::default()
            },
            ChartAxis {
                id: 2,
                cross_axis: Some(1),
                kind: "valAx".into(),
                position: Some(if kind == "bar" { "b" } else { "l" }.into()),
                cross_between: Some(if kind == "area" { "midCat" } else { "between" }.into()),
                ..Default::default()
            },
        ]
    };
    if kind == "scatter" {
        axes[0].cross_between = Some("between".into());
    }
    if combo {
        // Excel (UI28 combos-created.xlsx): the secondary value axis, then the deleted secondary
        // category axis at the bottom.
        axes.push(ChartAxis {
            id: 4,
            cross_axis: Some(3),
            kind: "valAx".into(),
            position: Some("r".into()),
            cross_between: Some("between".into()),
            ..Default::default()
        });
        axes.push(ChartAxis {
            id: 3,
            cross_axis: Some(4),
            kind: "catAx".into(),
            position: Some("b".into()),
            ..Default::default()
        });
    }
    let mut plots = vec![ChartPlot {
        kind: plot_kind.into(),
        bar_direction: dir.map(str::to_owned),
        grouping: (!pie && kind != "scatter").then(|| {
            if percent {
                "percentStacked"
            } else if plot_kind == "barChart" {
                "clustered"
            } else {
                "standard"
            }
            .into()
        }),
        vary_colors: pie,
        show_values: None,
        value_labels_source_linked: false,
        series,
        axis_ids: if pie { Vec::new() } else { vec![1, 2] },
        ..ChartPlot::default()
    }];
    if kind == "combo-three" {
        let area = plots[0].series.pop().unwrap();
        let line = plots[0].series.pop().unwrap();
        for (kind, series, axis_ids) in [
            ("lineChart", line, vec![3, 4]),
            ("areaChart", area, vec![1, 2]),
        ] {
            plots.push(ChartPlot {
                kind: kind.into(),
                grouping: Some("standard".into()),
                series: vec![series],
                axis_ids,
                ..Default::default()
            });
        }
    } else if combo {
        let last = plots[0].series.pop().unwrap();
        let second = match kind {
            "combo-column-area" | "combo-line-area" => "areaChart",
            "combo-line-column" | "combo-area-column" => "barChart",
            _ => "lineChart",
        };
        plots.push(ChartPlot {
            kind: second.into(),
            bar_direction: (second == "barChart").then(|| "col".into()),
            grouping: Some(
                if second == "barChart" {
                    "clustered"
                } else {
                    "standard"
                }
                .into(),
            ),
            series: vec![last],
            axis_ids: vec![3, 4],
            ..Default::default()
        });
    }
    // Excel writes the plots of a two-plot combination as area, bar, line whatever the order they
    // were made in (UI30 P9, UI28 combos); each series keeps its own index and order. The three-plot
    // combination is unmeasured and keeps the order it is made in.
    if plots.len() == 2 {
        plots.sort_by_key(|p| match p.kind.as_str() {
            "areaChart" => 0,
            "barChart" => 1,
            "lineChart" => 2,
            _ => 3,
        });
    }
    let mut charts = host.model().definitions().charts(sheet).to_vec();
    let reserved = host
        .model()
        .definitions()
        .all_charts()
        .flat_map(|(_, cs)| cs.iter().map(|c| c.part.clone()))
        .collect::<Vec<_>>();
    let part = host
        .loaded()
        .available_part_name("charts/dnaChart", "xml", &reserved);
    let names = charts
        .iter()
        .map(|c| c.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let name = match new.drawing_id {
        Some(id) => format!("Chart {}", id - 1),
        None => (1u32..)
            .map(|i| format!("Chart {i}"))
            .find(|n| !names.contains(n.as_str()))
            .unwrap(),
    };
    charts.push(Chart {
        edit_as: None,
        part,
        name,
        anchor: new.anchor,
        drawing_id: new.drawing_id,
        lang: Some(host.run_language()),
        unique_next: new.unique_start,
        title: new.title,
        title_formula: None,
        auto_title_deleted: new.auto_title_deleted,
        legend: new.legend.then(|| "r".into()),
        plots,
        axes,
        text_style: Default::default(),
        element_text: Default::default(),
        area_fill: None,
        plot_fill: None,
        area_border: Default::default(),
        plot_border: Default::default(),
        element_text_complex: Default::default(),
    });
    host.loaded()
        .check_chart_set_on(sheet, &charts, is_chart_sheet(host, sheet))
        .map_err(|e| e.to_string())?;
    Ok(vec![crate::CellEdit::Charts { sheet, charts }])
}

/// A chart this session made takes a name and a place on the sheet. A chart the file holds keeps
/// its frame (the save writes only new frames).
pub(crate) fn prepare_frame(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    part: &str,
    name: Option<String>,
    place: Option<[crate::Points; 4]>,
) -> Result<Vec<crate::CellEdit>, String> {
    use calc_doc_model::ChartAnchor;
    let fail = |s: &str| format!("{s} (dc-piw0)");
    if revision != host.revision().get() {
        return Err(fail("The workbook changed; read the chart again"));
    }
    let mut charts = host.model().definitions().charts(sheet).to_vec();
    let index = charts
        .iter()
        .position(|c| c.part == part)
        .ok_or_else(|| fail("The chart no longer exists"))?;
    if file_held(host, part) {
        return Err(fail("A chart the file holds keeps its name and place"));
    }
    if let Some(name) = name {
        if name.is_empty()
            || name.chars().count() > 255
            || charts
                .iter()
                .enumerate()
                .any(|(i, c)| i != index && c.name == name)
            || host.shapes(sheet).iter().any(|s| s.name == name)
        {
            return Err(fail("Use a unique chart name of 1 to 255 characters"));
        }
        charts[index].name = name;
    }
    if let Some([left, top, width, height]) = place {
        let [left, top, width, height] = [left, top, width, height].map(|p| p.get());
        if ![left, top, width, height]
            .into_iter()
            .all(|v| v.is_finite() && (0.0..=1.0e7).contains(&v))
            || width < 0.0
            || height < 0.0
        {
            return Err(fail("Use a position and a size on the worksheet"));
        }
        // Excel writes a macro's chart as a two-cell anchor (dc-piw0 Q0).
        charts[index].anchor = ChartAnchor::TwoCell {
            from: host.point_at(sheet, left, top),
            to: host.point_at(sheet, left + width, top + height),
        };
    }
    host.loaded()
        .check_chart_set_on(sheet, &charts, is_chart_sheet(host, sheet))
        .map_err(|e| e.to_string())?;
    Ok(vec![crate::CellEdit::Charts { sheet, charts }])
}

/// Legend placement is an authored property, validated and saved through the retained chart route.
pub(crate) fn prepare_legend(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    part: &str,
    position: &str,
) -> Result<Vec<crate::CellEdit>, String> {
    prepare_properties(host, sheet, revision, part, None, false, None)?;
    if !matches!(position, "l" | "r" | "t" | "b" | "tr") {
        return Err("Choose Left, Right, Top, Bottom or Top right (dc-zjsg)".into());
    }
    let mut charts = host.model().definitions().charts(sheet).to_vec();
    let chart = charts
        .iter_mut()
        .find(|c| c.part == part)
        .ok_or("The chart is unavailable (dc-zjsg)")?;
    chart.legend = Some(position.into());
    host.loaded()
        .check_chart_properties(sheet, chart)
        .map_err(|e| e.to_string())?;
    if charts == host.model().definitions().charts(sheet) {
        return Ok(Vec::new());
    }
    Ok(vec![crate::CellEdit::Charts { sheet, charts }])
}

/// Series colour editing uses the retained DrawingML writer; an empty colour restores inheritance.
#[allow(clippy::too_many_arguments)]
/// A series' data labels (dc-oxh8w.18): the retained writer saves `c:dLbls`.
pub(crate) fn prepare_series_labels(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    part: &str,
    plot: u32,
    series: u32,
    labels: Option<calc_doc_model::ChartLabels>,
) -> Result<Vec<crate::CellEdit>, String> {
    prepare_properties(host, sheet, revision, part, None, false, None)?;
    let mut charts = host.model().definitions().charts(sheet).to_vec();
    let chart = charts
        .iter_mut()
        .find(|c| c.part == part)
        .ok_or("The chart is unavailable")?;
    let plot = chart.plots.get_mut(plot as usize).ok_or("The plot is unavailable")?;
    if !matches!(
        plot.kind.as_str(),
        "barChart" | "lineChart" | "areaChart" | "pieChart" | "doughnutChart" | "scatterChart"
    ) {
        return Err("Data labels of this chart type are not modelled (dc-zjsg)".into());
    }
    let series = plot
        .series
        .iter_mut()
        .find(|s| s.index == series)
        .ok_or("The series is unavailable")?;
    if series.labels == labels {
        return Ok(Vec::new());
    }
    // The writer states every part of a label, so a reopened series reads as not the simple value-only form.
    series.show_values = labels.as_ref().map(|_| false);
    series.value_labels_source_linked = false;
    series.labels = labels;
    host.loaded()
        .check_chart_properties(sheet, chart)
        .map_err(|e| e.to_string())?;
    Ok(vec![crate::CellEdit::Charts { sheet, charts }])
}
/// `ChartArea.Interior.Color` and its kin: the fill of the chart area or the plot area (dc-oxh8w.31).
pub(crate) fn prepare_area_style(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    part: &str,
    plot_area: bool,
    fill: Option<String>,
) -> Result<Vec<crate::CellEdit>, String> {
    prepare_properties(host, sheet, revision, part, None, false, None)?;
    let fill = fill.map(|s| s.trim().strip_prefix('#').unwrap_or(s.trim()).to_ascii_uppercase());
    if fill.as_ref().is_some_and(|s| s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit())) {
        return Err("Use a six-digit RGB colour (dc-zjsg)".into());
    }
    let mut charts = host.model().definitions().charts(sheet).to_vec();
    let chart = charts.iter_mut().find(|c| c.part == part).ok_or("The chart is unavailable")?;
    let slot = if plot_area { &mut chart.plot_fill } else { &mut chart.area_fill };
    if *slot == fill {
        return Ok(Vec::new());
    }
    *slot = fill;
    host.loaded().check_chart_properties(sheet, chart).map_err(|e| e.to_string())?;
    Ok(vec![crate::CellEdit::Charts { sheet, charts }])
}
/// `ChartArea.Border` and `PlotArea.Border`: the outline of the chart area or the plot area (dc-oxh8w.61).
pub(crate) fn prepare_area_border(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    part: &str,
    plot_area: bool,
    border: calc_doc_model::ChartBorder,
) -> Result<Vec<crate::CellEdit>, String> {
    prepare_properties(host, sheet, revision, part, None, false, None)?;
    let mut charts = host.model().definitions().charts(sheet).to_vec();
    let chart = charts.iter_mut().find(|c| c.part == part).ok_or("The chart is unavailable")?;
    let slot = if plot_area { &mut chart.plot_border } else { &mut chart.area_border };
    if *slot == border {
        return Ok(Vec::new());
    }
    *slot = border;
    host.loaded().check_chart_properties(sheet, chart).map_err(|e| e.to_string())?;
    Ok(vec![crate::CellEdit::Charts { sheet, charts }])
}
pub(crate) fn prepare_series_style(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    part: &str,
    plot: u32,
    series: u32,
    fill: Option<String>,
) -> Result<Vec<crate::CellEdit>, String> {
    prepare_properties(host, sheet, revision, part, None, false, None)?;
    let fill = fill
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("automatic"))
        .map(|s| s.strip_prefix('#').unwrap_or(&s).to_ascii_uppercase());
    if fill
        .as_ref()
        .is_some_and(|s| s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("Use a six-digit RGB colour, or Automatic (dc-zjsg)".into());
    }
    let mut charts = host.model().definitions().charts(sheet).to_vec();
    let chart = charts
        .iter_mut()
        .find(|c| c.part == part)
        .ok_or("The chart is unavailable")?;
    let plot = chart
        .plots
        .get_mut(plot as usize)
        .ok_or("The plot is unavailable")?;
    if !matches!(
        plot.kind.as_str(),
        "barChart" | "lineChart" | "areaChart" | "pieChart" | "doughnutChart" | "scatterChart"
    ) {
        return Err("Series colours for this chart type are not modelled (dc-zjsg)".into());
    }
    let series = plot
        .series
        .iter_mut()
        .find(|s| s.index == series)
        .ok_or("The series is unavailable")?;
    if series.fill == fill {
        return Ok(Vec::new());
    }
    series.fill = fill.clone();
    // The writer paints a stated outline colour with the series colour; the model says the same.
    if let (Some(fill), Some(_)) = (fill, series.line.color.as_ref())
        && !series.line.none
    {
        series.line.color = Some(fill);
    }
    host.loaded()
        .check_chart_properties(sheet, chart)
        .map_err(|e| e.to_string())?;
    Ok(vec![crate::CellEdit::Charts { sheet, charts }])
}

/// `Series.Border`: the outline of one series (dc-oxh8w.67).
pub(crate) fn prepare_series_border(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    part: &str,
    plot: u32,
    series: u32,
    border: calc_doc_model::ChartBorder,
) -> Result<Vec<crate::CellEdit>, String> {
    prepare_properties(host, sheet, revision, part, None, false, None)?;
    let mut charts = host.model().definitions().charts(sheet).to_vec();
    let chart = charts.iter_mut().find(|c| c.part == part).ok_or("The chart is unavailable")?;
    let plot = chart.plots.get_mut(plot as usize).ok_or("The plot is unavailable")?;
    let series = plot.series.iter_mut().find(|s| s.index == series).ok_or("The series is unavailable")?;
    if series.line == border {
        return Ok(Vec::new());
    }
    series.line = border;
    host.loaded().check_chart_properties(sheet, chart).map_err(|e| e.to_string())?;
    Ok(vec![crate::CellEdit::Charts { sheet, charts }])
}

pub(crate) fn prepare_text_style(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    part: &str,
    mut style: calc_doc_model::ChartTextStyle,
) -> Result<Vec<crate::CellEdit>, String> {
    prepare_properties(host, sheet, revision, part, None, false, None)?;
    style.color = style
        .color
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("automatic"));
    style.family = style
        .family
        .filter(|s| !s.eq_ignore_ascii_case("automatic"));
    let mut charts = host.model().definitions().charts(sheet).to_vec();
    let chart = charts
        .iter_mut()
        .find(|c| c.part == part)
        .ok_or("The chart is unavailable")?;
    if style
        .color
        .as_ref()
        .zip(chart.text_style.color.as_ref())
        .is_some_and(|(new, old)| {
            new.strip_prefix('#')
                .unwrap_or(new)
                .eq_ignore_ascii_case(old)
        })
    {
        style.color = chart.text_style.color.clone();
    } else if style.color != chart.text_style.color {
        style.color = style
            .color
            .map(|s| s.strip_prefix('#').unwrap_or(&s).to_ascii_uppercase());
    }
    if chart.text_style == style {
        return Ok(Vec::new());
    }
    chart.text_style = style;
    host.loaded()
        .check_chart_properties(sheet, chart)
        .map_err(|e| e.to_string())?;
    Ok(vec![crate::CellEdit::Charts { sheet, charts }])
}

pub(crate) fn prepare_element_text_style(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    part: &str,
    target: &str,
    mut style: calc_doc_model::ChartTextStyle,
) -> Result<Vec<crate::CellEdit>, String> {
    prepare_properties(host, sheet, revision, part, None, false, None)?;
    let mut charts = host.model().definitions().charts(sheet).to_vec();
    let chart = charts
        .iter_mut()
        .find(|c| c.part == part)
        .ok_or("The chart is unavailable")?;
    let valid = match target {
        "title" => chart.title.is_some() || chart.title_formula.is_some(),
        "legend" => chart.legend.is_some(),
        _ => target
            .strip_prefix("axis:")
            .and_then(|s| s.parse::<u32>().ok())
            .is_some_and(|id| {
                target == format!("axis:{id}")
                    && chart.axes.iter().filter(|a| a.id == id).count() == 1
            }),
    };
    if chart.element_text_complex.contains(target) {
        return Err(
            "Editing chart text with several runs or paragraphs is not modelled (dc-zjsg)"
                .into(),
        );
    }
    if !valid {
        return Err("Choose an existing title, legend or axis (dc-zjsg)".into());
    }
    style.color = style
        .color
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("automatic"));
    style.family = style
        .family
        .filter(|s| !s.eq_ignore_ascii_case("automatic"));
    let empty = calc_doc_model::ChartTextStyle::default();
    let before = chart.element_text.get(target).unwrap_or(&empty);
    if style
        .color
        .as_ref()
        .zip(before.color.as_ref())
        .is_some_and(|(new, old)| {
            new.strip_prefix('#')
                .unwrap_or(new)
                .eq_ignore_ascii_case(old)
        })
    {
        style.color = before.color.clone();
    } else if style.color != before.color {
        style.color = style
            .color
            .map(|s| s.strip_prefix('#').unwrap_or(&s).to_ascii_uppercase());
    }
    if *before == style {
        return Ok(Vec::new());
    }
    if style == empty {
        chart.element_text.remove(target);
    } else {
        chart.element_text.insert(target.into(), style);
    }
    host.loaded()
        .check_chart_properties(sheet, chart)
        .map_err(|e| e.to_string())?;
    Ok(vec![crate::CellEdit::Charts { sheet, charts }])
}

/// One bin of a histogram: the values from `low` to `high` (closed on the right unless `closed_left`; the first bin
/// closed at its low end too, the last of a left-closed set at its high end), or, for the underflow bin, at or below
/// `high`, and for the overflow bin above `low`.
#[derive(Clone, Debug, PartialEq)]
pub struct Bin {
    /// Lower edge of the bin.
    pub low: f64,
    /// Upper edge of the bin.
    pub high: f64,
    /// Number of observations assigned to the bin.
    pub count: usize,
    /// Whether this is the special bin below the regular bins.
    pub underflow: bool,
    /// Whether this is the special bin above the regular bins.
    pub overflow: bool,
}

/// The bins Excel's histogram counts `values` into (dc-zjsg.6): bins of `cx:binSize`, or `cx:binCount` of them over
/// the range, else Scott's normal reference rule (width 3.49 s / n^(1/3), s the sample standard deviation) from the
/// smallest value; with an underflow value a first bin for the values at or below it (below it, left-closed) and
/// with an overflow value a last bin for those above it (at or above). The rule's rounding and the bin edges Excel
/// shows have not been measured (ASSUMED until an Excel side-by-side).
pub fn histogram_bins(values: &[f64], binning: Option<&calc_doc_model::Binning>) -> Vec<Bin> {
    let v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return Vec::new();
    }
    let number = |s: &Option<String>| s.as_deref().and_then(|t| t.trim().parse::<f64>().ok()).filter(|x| x.is_finite());
    let default = calc_doc_model::Binning::default();
    let binning = binning.unwrap_or(&default);
    let left = binning.closed_left;
    let (under, over) = (number(&binning.underflow), number(&binning.overflow));
    let n = v.len() as f64;
    let (min, max) = v.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| (a.min(*x), b.max(*x)));
    let below = |x: f64, u: f64| if left { x < u } else { x <= u };
    let above = |x: f64, o: f64| if left { x >= o } else { x > o };
    let central: Vec<f64> = v.iter().copied().filter(|x| !under.is_some_and(|u| below(*x, u)) && !over.is_some_and(|o| above(*x, o))).collect();
    let (lo, hi) = (under.unwrap_or(min), over.unwrap_or(max));
    let width = number(&binning.size).filter(|w| *w > 0.0).or_else(|| {
        let count = number(&binning.count).filter(|c| *c >= 1.0)?;
        Some((hi - lo) / count.floor()).filter(|w| *w > 0.0)
    });
    let width = width.unwrap_or_else(|| {
        let mean = v.iter().sum::<f64>() / n;
        let s = if v.len() > 1 { (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt() } else { 0.0 };
        let w = 3.49 * s / n.cbrt();
        if w > 0.0 { w } else { 1.0 }
    });
    let bins = match number(&binning.count).filter(|c| *c >= 1.0 && number(&binning.size).is_none()) {
        Some(c) => c.floor() as usize,
        None => (((hi - lo) / width) - 1e-9).ceil().max(1.0) as usize,
    }
    .min(100_000);
    let mut out: Vec<Bin> = (0..bins)
        .map(|k| Bin { low: lo + k as f64 * width, high: lo + (k + 1) as f64 * width, count: 0, underflow: false, overflow: false })
        .collect();
    for x in central {
        let steps = (x - lo) / width;
        let k = if left { (steps + 1e-9).floor() } else { (steps - 1e-9).ceil() - 1.0 };
        out[(k.max(0.0) as usize).min(bins - 1)].count += 1;
    }
    if let Some(u) = under {
        let count = v.iter().filter(|x| below(**x, u)).count();
        out.insert(0, Bin { low: f64::NEG_INFINITY, high: u, count, underflow: true, overflow: false });
    }
    if let Some(o) = over {
        let count = v.iter().filter(|x| above(**x, o)).count();
        out.push(Bin { low: o, high: f64::INFINITY, count, underflow: false, overflow: true });
    }
    out
}

#[cfg(test)]
mod histogram_tests {
    use super::histogram_bins;
    use calc_doc_model::Binning;

    fn counts(values: &[f64], binning: Option<&Binning>) -> Vec<usize> {
        histogram_bins(values, binning).iter().map(|b| b.count).collect()
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn bins_by_size_closed_right_or_left() {
        let values = [1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 4.0, 5.0];
        let size = |left| Binning { size: Some("2".into()), closed_left: left, ..Binning::default() };
        assert_eq!(counts(&values, Some(&size(false))), [6, 2]);
        assert_eq!(counts(&values, Some(&size(true))), [3, 5]);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn bins_by_count_and_with_under_and_overflow() {
        let values = [1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 4.0, 5.0];
        let count = Binning { count: Some("4".into()), ..Binning::default() };
        assert_eq!(counts(&values, Some(&count)), [3, 3, 1, 1]);
        let outer = Binning { size: Some("1".into()), underflow: Some("2".into()), overflow: Some("4".into()), ..Binning::default() };
        let bins = histogram_bins(&values, Some(&outer));
        assert_eq!(bins.iter().map(|b| b.count).collect::<Vec<_>>(), [3, 3, 1, 1]);
        assert!(bins[0].underflow && bins[3].overflow);
    }

    /// Scott's rule on 1..=10: width 3.49 * 3.0277 / 10^(1/3) = 4.905, so two bins of five.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn automatic_bins_follow_scotts_rule() {
        let values: Vec<f64> = (1..=10).map(f64::from).collect();
        let bins = histogram_bins(&values, None);
        assert_eq!(bins.iter().map(|b| b.count).collect::<Vec<_>>(), [5, 5]);
        assert!((bins[0].high - bins[0].low - 4.905).abs() < 0.01);
    }
}
