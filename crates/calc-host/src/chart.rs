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
//! Linear primary value-axis bounds, units and direction are retained and editable. Secondary
//! and logarithmic axes are preserved with a drawing notice. Not modelled: 3-D, chart number
//! formats, trendlines, error bars, data labels, and the darker and
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
    pub axis_ids: Vec<u32>,
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
    pub axes:Vec<calc_doc_model::ChartAxis>,
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
                        axis_ids:plot.axis_ids.clone(),
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
                axes:chart.axes.clone(),
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


/// A validated linear scale shared by command validation and chart drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartScale {
    pub minimum:f64,
    pub maximum:f64,
    pub step:f64,
    pub reversed:bool,
}

pub fn chart_scale(low:f64,high:f64,properties:Option<&calc_doc_model::ChartAxis>)->Result<ChartScale,String> {
    if !low.is_finite() || !high.is_finite() || !(high.max(0.)-low.min(0.)).is_finite() {
        return Err("This chart's values exceed the supported linear axis range".into());
    }
    let (automatic_min,automatic_max,automatic_step)=axis(low,high);
    let read=|value:Option<&str>|->Result<Option<f64>,String>{
        value.map(|s|s.parse::<f64>().ok().filter(|n|n.is_finite()).ok_or_else(||"Use finite numbers for axis bounds and units".into())).transpose()
    };
    let minimum=read(properties.and_then(|a|a.minimum.as_deref()))?;
    let maximum=read(properties.and_then(|a|a.maximum.as_deref()))?;
    let unit=read(properties.and_then(|a|a.major_unit.as_deref()))?;
    if properties.is_some_and(|a|a.log_base.is_some()) {return Err("Logarithmic axes are preserved but their scaling is not drawn here".into());}
    let mut min=minimum.unwrap_or(automatic_min);let mut max=maximum.unwrap_or(automatic_max);
    let automatic_span=automatic_max-automatic_min;
    if minimum.is_none() && min>=max {min=max-automatic_span;}
    if maximum.is_none() && max<=min {max=min+automatic_span;}
    let span=max-min;
    if !span.is_finite() || span<=0. {return Err("The axis maximum must be greater than its minimum".into());}
    if [low,high,0.].into_iter().any(|v|!(((v-min)/span)*1_000_000.).is_finite()){return Err("The fixed bounds are too narrow for this chart's values".into());}
    let step=if let Some(unit)=unit {unit} else if minimum.is_none() && maximum.is_none(){automatic_step}else{
        let raw=span/8.;let power=10f64.powf(raw.log10().floor());
        [1.,2.,5.,10.].into_iter().map(|m|m*power).find(|s|*s>=raw).unwrap_or(10.*power)
    };
    if !step.is_finite() || step<=0. || span/step>1000. {return Err("Use a positive major unit that produces at most 1,000 intervals".into());}
    Ok(ChartScale{minimum:min,maximum:max,step,reversed:properties.is_some_and(|a|a.reversed)})
}

/// The value extent of a category plot, including separate positive/negative stacks.
pub fn plot_value_range(plot:&PlotView)->(f64,f64) {
    let (mut low,mut high)=(0f64,0f64);
    if plot.stacked {
        let count=plot.series.iter().map(|s|s.values.len()).max().unwrap_or(0);
        for j in 0..count {
            let (mut up,mut down)=(0.,0.);
            for s in &plot.series {let v=s.values.get(j).copied().flatten().unwrap_or(0.);if v>=0.{up+=v}else{down+=v}}
            low=low.min(down);high=high.max(up);
        }
    }else{for v in plot.series.iter().flat_map(|s|s.values.iter().flatten()){low=low.min(*v);high=high.max(*v);}}
    (low,high)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_axis(host:&HostState,sheet:SheetId,revision:u64,part:&str,id:u32,minimum:Option<String>,maximum:Option<String>,major_unit:Option<String>,reversed:bool)->Result<Vec<crate::CellEdit>,String>{
    prepare_properties(host,sheet,revision,part,None,false,None)?;
    let mut charts=host.model().definitions().charts(sheet).to_vec();
    let chart=charts.iter_mut().find(|c|c.part==part).ok_or("The chart is unavailable")?;
    if chart.plots.len()!=1 {return Err("Editing axes of combination charts is not modelled (dc-971p.46)".into());}
    if chart.axes.iter().filter(|a|a.id==id).count()!=1 {return Err("The axis identity changed; reopen its controls (dc-971p.46)".into());}
    value_axes(&chart.axes,&chart.plots[0].axis_ids,chart.plots[0].kind=="scatterChart").map_err(|e|format!("{e} (dc-971p.46)"))?;
    let axis=chart.axes.iter_mut().find(|a|a.id==id && a.kind=="valAx").ok_or("Choose a value axis (dc-971p.46)")?;
    if axis.log_base.is_some(){return Err("Editing logarithmic axes is not modelled (dc-971p.46)".into());}
    let canonical=|value:Option<String>|->Result<Option<String>,String>{
        value.filter(|s|!s.trim().is_empty()).map(|s|s.trim().parse::<f64>().ok().filter(|n|n.is_finite()).map(|n|n.to_string()).ok_or_else(||"Use finite numeric axis bounds and units (dc-971p.46)".into())).transpose()
    };
    axis.minimum=canonical(minimum)?;axis.maximum=canonical(maximum)?;axis.major_unit=canonical(major_unit)?;axis.reversed=reversed;
    let view=host.charts(sheet).0.into_iter().find(|v|v.part==part).ok_or("The chart cannot be read")?;
    let plot=view.plots.first().ok_or("The chart has no plot")?;
    let (low,high)=if plot.kind==ChartKind::Scatter {
        let (x,y)=scatter_ranges(plot);
        if matches!(axis.position.as_deref(),Some("b"|"t")){x}else{y}
    }else if matches!(plot.kind,ChartKind::Column|ChartKind::Bar|ChartKind::Line|ChartKind::Area) {plot_value_range(plot)}else{
        return Err("This plot has no editable linear value axis (dc-971p.46)".into());
    };
    chart_scale(low,high,Some(axis)).map_err(|e|format!("{e} (dc-971p.46)"))?;
    host.loaded().check_chart_properties(sheet,chart).map_err(|e|e.to_string())?;
    if charts==host.model().definitions().charts(sheet){return Ok(Vec::new());}
    Ok(vec![crate::CellEdit::Charts{sheet,charts}])
}

/// Primary axis pairs must belong to this plot and cross each other. Scatter returns X then Y.
pub fn value_axes<'a>(axes:&'a [calc_doc_model::ChartAxis],ids:&[u32],scatter:bool)->Result<Vec<&'a calc_doc_model::ChartAxis>,String>{
    let invalid=||"Secondary or unassociated chart axes are preserved but not drawn or edited here".to_owned();
    if axes.len()!=2 || ids.len()!=2 || ids[0]==ids[1] || axes[0].id==axes[1].id {return Err(invalid());}
    let a=axes.iter().find(|a|a.id==ids[0]).ok_or_else(invalid)?;
    let b=axes.iter().find(|a|a.id==ids[1]).ok_or_else(invalid)?;
    if a.cross_axis!=Some(b.id) || b.cross_axis!=Some(a.id){return Err(invalid());}
    let horizontal=|a:&calc_doc_model::ChartAxis|matches!(a.position.as_deref(),Some("b"|"t"));
    let vertical=|a:&calc_doc_model::ChartAxis|matches!(a.position.as_deref(),Some("l"|"r"));
    if !((horizontal(a)&&vertical(b))||(horizontal(b)&&vertical(a))){return Err(invalid());}
    if scatter {
        if a.kind!="valAx" || b.kind!="valAx" {return Err(invalid());}
        Ok(if horizontal(a){vec![a,b]}else{vec![b,a]})
    }else{
        let (value,category)=if a.kind=="valAx"{(a,b)}else{(b,a)};
        if value.kind!="valAx" || !matches!(category.kind.as_str(),"catAx"|"dateAx"){return Err(invalid());}
        Ok(vec![value])
    }
}
/// Scatter X fallback is shared by drawing and axis-command validation.
pub fn scatter_x_values(series:&SeriesView)->Vec<Option<f64>> {
    let xs=series.categories.iter().map(|v|v.as_ref().and_then(|v|v.as_number())).collect::<Vec<_>>();
    if xs.is_empty() || xs.iter().any(Option::is_none){(1..=series.values.len()).map(|n|Some(n as f64)).collect()}else{xs}
}
pub fn scatter_ranges(plot:&PlotView)->((f64,f64),(f64,f64)) {
    let (mut xl,mut xh,mut yl,mut yh)=(0f64,0f64,0f64,0f64);
    for series in &plot.series {for (x,y) in scatter_x_values(series).iter().zip(&series.values) {if let (Some(x),Some(y))=(x,y){xl=xl.min(*x);xh=xh.max(*x);yl=yl.min(*y);yh=yh.max(*y);}}}
    ((xl,xh),(yl,yh))
}
