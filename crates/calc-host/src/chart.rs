//! A sheet's charts as a skin draws them (dc-6yw): each chart's place on the sheet, title, legend
//! and plots, and each series' name, categories (a scatter's X values) and values as the workbook
//! holds them now. A series' formulas are evaluated by the one evaluator
//! ([`HostState::formula_values`], through [`calc_engine::DetachedFormula`]), so a chart follows
//! every edit and recalculation; a series whose formula does not evaluate shows the points the
//! file cached with it, and says so ([`SeriesView::cached`]).
//!
//! As Excel reads a series' cells (measured through `Chart.SeriesCollection(i).Values` and
//! `.XValues`: tests/fixtures/dc_6yw/charts-excel.txt): a number is plotted; anything else is a
//! gap (`None`). A series' colour is its `c:spPr` fill, else the theme's accent colour for its
//! index (accent1 to accent6 in turn); a plot that varies its colours by point (a pie) takes them
//! by point in the same order.
//!
//! Not modelled (drawn as the plain form of their kind): 3-D, a secondary axis, axis scaling and
//! number formats the chart states, trendlines, error bars, data labels, and the darker and
//! lighter accents Excel uses past the sixth series.

use crate::state::HostState;
use calc_doc::{ChartAnchor, ChartData, DrawingPoint};
use calc_doc_model::value::{CalcValue, CellKey, CoreRef, SheetId};

/// EMU to a pixel at 96 dots per inch.
pub const EMU_PER_PIXEL: f64 = 9_525.0;

/// A corner of a chart: a cell, and the offset into it in pixels at 100%.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartCorner {
    pub cell: CellKey,
    pub dx: f64,
    pub dy: f64,
}

/// Where a chart is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChartPlace {
    /// Between two corners: it moves and sizes with the cells under them.
    Cells { from: ChartCorner, to: ChartCorner },
    /// From a corner, with a size in pixels at 100%.
    Sized {
        from: ChartCorner,
        width: f64,
        height: f64,
    },
    /// At a position on the sheet, in pixels at 100%.
    Absolute {
        x: f64,
        y: f64,
        width: f64,
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
    Line,
    Area,
    Pie,
    Doughnut,
    Scatter,
    /// Another kind, by its element name (`radarChart`, `bubbleChart`, `stockChart`, ...).
    Other(String),
}

/// One series as the chart shows it now.
#[derive(Clone, Debug, PartialEq)]
pub struct SeriesView {
    pub name: String,
    /// The categories (a scatter's X values), one per point; `None` for a blank.
    pub categories: Vec<Option<CalcValue>>,
    /// The values (a scatter's Y values); `None` is a gap.
    pub values: Vec<Option<f64>>,
    /// The series' colour, `RRGGBB`.
    pub color: String,
    /// Whether any values, categories/X coordinates or series name use the file's saved data.
    pub cached: bool,
}

/// One plot of a chart.
#[derive(Clone, Debug, PartialEq)]
pub struct PlotView {
    pub kind: ChartKind,
    /// `stacked` or `percentStacked` grouping: each series is drawn on those before it.
    pub stacked: bool,
    pub percent: bool,
    /// Each point its own colour (a pie's slices).
    pub vary_colors: bool,
    pub series: Vec<SeriesView>,
}

/// One chart as a skin draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct ChartView {
    /// The frame's name (`Chart 1`).
    pub name: String,
    pub part:String,
    pub place: ChartPlace,
    pub title: Option<String>,
    /// The legend's position (`r`, `l`, `t`, `b`, `tr`), `None` without a legend.
    pub legend: Option<String>,
    pub plots: Vec<PlotView>,
    /// The theme's accent colours, accent1 to accent6, `RRGGBB`: a varied plot's point colours.
    pub palette: Vec<String>,
}

/// Excel's 2013 theme accents, for a workbook whose theme gives none.
const OFFICE_ACCENTS: [&str; 6] = ["4472C4", "ED7D31", "A5A5A5", "FFC000", "5B9BD5", "70AD47"];

impl HostState {
    /// The workbook's accent1–accent6, with the same Office fallback for charts and tables.
    pub fn theme_accent(&self, index: usize) -> Option<String> {
        let fallback = OFFICE_ACCENTS.get(index)?;
        Some(self.loaded.theme_colors().get(4+index).filter(|c|c.len()==6).cloned().unwrap_or_else(||(*fallback).to_owned()))
    }
    /// The charts on `sheet`, in the drawing's order, with their series as the workbook holds
    /// them now; and how many charts the file holds that could not be read.
    pub fn charts(&self, sheet: SheetId) -> (Vec<ChartView>, usize) {
        let Some(loaded) = self.loaded.sheet(sheet) else {
            return (Vec::new(), 0);
        };
        let read = loaded.charts();
        let theme = self.loaded.theme_colors();
        let palette: Vec<String> = (0..6)
            .map(|i| {
                self.theme_accent(i).expect("six theme accents")
            })
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
            let Some(place) = self.chart_place(sheet, &chart.anchor) else {
                // Its cells were deleted: Excel deletes the chart with them.
                continue;
            };
            let plots = chart
                .plots
                .iter()
                .map(|plot| {
                    let kind = match plot.kind.as_str() {
                        "barChart"
                            if plot.bar_direction.as_deref() == Some("bar") =>
                        {
                            ChartKind::Bar
                        }
                        "barChart" => ChartKind::Column,
                        "lineChart" => ChartKind::Line,
                        "areaChart" => ChartKind::Area,
                        "pieChart" => ChartKind::Pie,
                        "doughnutChart" => ChartKind::Doughnut,
                        "scatterChart" => ChartKind::Scatter,
                        other => ChartKind::Other(other.to_owned()),
                    };
                    let grouping = plot.grouping.as_deref().unwrap_or_default();
                    PlotView {
                        kind,
                        stacked: matches!(grouping, "stacked" | "percentStacked"),
                        percent: grouping == "percentStacked",
                        vary_colors: plot.vary_colors,
                        series: plot
                            .series
                            .iter()
                            .map(|series| {
                                let (values, cached) =
                                    self.series_values(series.values.as_ref(), origin);
                                let (categories,category_cached)=series.categories.as_ref().map(|data|self.data_values(data,origin)).unwrap_or_default();
                                let (names,name_cached)=series.name.as_ref().map(|data|self.data_values(data,origin)).unwrap_or_default();
                                let name=names.into_iter().flatten().next().map(|value|text_of(&value)).unwrap_or_else(||format!("Series{}",series.index+1));
                                let cached=cached||category_cached||name_cached;
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
                                SeriesView {
                                    name,
                                    categories,
                                    values,
                                    color,
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
                part:chart.part.clone(),
                place,
                title,
                legend: chart.legend.clone(),
                plots,
                palette: palette.clone(),
            });
        }
        (views, unread)
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

    fn chart_place(&self, sheet: SheetId, anchor: &ChartAnchor) -> Option<ChartPlace> {
        let px = |emu: i64| emu as f64 / EMU_PER_PIXEL;
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

    /// A series' data as the workbook holds it now (its formula evaluated), else as the file
    /// cached it; and whether it is the cache.
    fn data_values(&self, data: &ChartData, origin: CellKey) -> (Vec<Option<CalcValue>>, bool) {
        if let Some(values) = data
            .formula
            .as_deref()
             .and_then(|formula| {
                use calc_doc_model::{FormulaRewriter,NameScope,StructuralChange};
                let mut current=formula.to_owned();
                for edit in &self.axis_edits {
                    if let Some(next)=calc_engine::ReferenceRewriter.name(self.loaded.model(),&current,NameScope::Sheet(origin.sheet()),&StructuralChange::Axis(*edit)) {current=next;}
                }
                self.formula_values(&current, origin, origin)
            })
        {
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
        return Err("The workbook changed; reopen the chart controls (dc-e5z5)".into());
    }
    let source = host
        .loaded()
        .sheet(sheet)
        .ok_or("The chart worksheet no longer exists")?;
    if source.protected() || source.is_macro_sheet() {
        return Err(
            "Editing charts on this protected or macro worksheet is not modelled (dc-e5z5)"
                .into(),
        );
    }
    if host.loaded().workbook_part().ends_with(".bin") {
        return Err(
            "Save as XLSX and reopen before editing charts in an XLSB workbook (dc-e5z5)".into(),
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
    host: &HostState, sheet:SheetId, revision:u64, part:&str, plot:u32,
    series:Option<u32>, name:Option<String>, categories:Option<String>,
    values:Option<String>, remove:bool,
) -> Result<Vec<crate::CellEdit>,String> {
    // Reuse the chart target, protection, format and revision checks.
    prepare_properties(host,sheet,revision,part,None,false,None)?;
    if !host.axis_edits.is_empty() {
        return Err("Save and reopen before editing chart sources after inserting or deleting rows or columns (dc-971p.46)".into());
    }
    let mut charts=host.model().definitions().charts(sheet).to_vec();
    let chart=charts.iter_mut().find(|c|c.part==part).ok_or("The chart is unavailable")?;
    let next=chart.plots.iter().flat_map(|p|&p.series).map(|s|s.index)
        .chain(host.loaded().model().definitions().charts(sheet).iter().filter(|c|c.part==part).flat_map(|c|&c.plots).flat_map(|p|&p.series).map(|s|s.index))
        .max().unwrap_or(0).checked_add(1).ok_or("The series index limit was reached")?;
    let p=chart.plots.get_mut(plot as usize).ok_or("The chart plot is unavailable")?;
    if !matches!(p.kind.as_str(),"barChart"|"lineChart"|"areaChart"|"pieChart"|"doughnutChart"|"scatterChart") {
        return Err("Editing series of this chart type is not modelled (dc-971p.46)".into());
    }
    let position=series.map(|index|p.series.iter().position(|s|s.index==index).ok_or("The series changed; reopen its controls")).transpose()?;
    if remove {
        let i=position.ok_or("Select a series to remove")?;
        if p.series.len()==1 {return Err("Keep at least one series in a chart plot (dc-971p.46)".into());}
        p.series.remove(i);
    } else {
        if position.is_none() && p.series.len()>=256 {return Err("A chart plot can have at most 256 series here (dc-971p.46)".into());}
        let mut changed=position.map(|i|p.series[i].clone()).unwrap_or_else(||calc_doc_model::ChartSeries{index:next,..Default::default()});
        let origin=CellKey::new(sheet,1,1).unwrap();
        let range=|text:&str,numeric:bool,single:bool|->Result<ChartData,String>{
            let text=text.trim().trim_start_matches('=').trim();
            let crate::ReferenceTarget::Area(area)=host.resolve_reference_text(origin,text) else {
                return Err("Use a single worksheet cell or a one-row or one-column A1 range (dc-971p.46)".into());
            };
            let count=u64::from(area.row_count())*u64::from(area.column_count());
            if count>crate::validation::LIST_LIMIT as u64 || (area.row_count()>1 && area.column_count()>1) || (single && count!=1) {
                return Err("Chart ranges must be one row or column with at most 32,767 cells; a name reference must be one cell (dc-971p.46)".into());
            }
            // Canonical absolute references keep edits independent of the selected cell.
            let sheet_name=host.model().sheet_by_id(area.sheet()).ok_or("The source worksheet is unavailable")?.name().text().to_string_lossy().replace('\'',"''");
            let a1=|at:CellKey|{let mut n=at.column();let mut letters=String::new();while n>0 {n-=1;letters.insert(0,(b'A'+(n%26) as u8)as char);n/=26;}format!("${letters}${}",at.row())};
            let first=a1(area.first());let last=a1(area.last());
            Ok(ChartData{formula:Some(format!("'{sheet_name}'!{first}{}",if first==last {String::new()}else{format!(":{last}")})),cache:Vec::new(),numeric})
        };
        if let Some(text)=name {
            if text.encode_utf16().count()>32767 || text.chars().any(|c|(c<' ' && !matches!(c,'\t'|'\n'|'\r')) || matches!(c,'\u{fffe}'|'\u{ffff}')) {return Err("The series name contains unsupported text or exceeds 32,767 UTF-16 units (dc-971p.46)".into());}
            changed.name=if text.is_empty(){None}else if text.starts_with('='){Some(range(&text,false,true)?)}else{Some(ChartData{cache:vec![Some(text)],..Default::default()})};
        }
        if let Some(text)=categories {changed.categories=if text.trim().is_empty(){None}else{Some(range(&text,p.kind=="scatterChart",false)?)};}
        if let Some(text)=values {changed.values=Some(range(&text,true,false)?);}
        if changed.values.is_none(){return Err("Choose a values range for the new series (dc-971p.46)".into());}
        if let Some(i)=position {p.series[i]=changed;} else {p.series.push(changed);}
    }
    host.loaded().check_chart_properties(sheet,chart).map_err(|e|e.to_string())?;
    if charts==host.model().definitions().charts(sheet){return Ok(Vec::new());}
    Ok(vec![crate::CellEdit::Charts{sheet,charts}])
}
