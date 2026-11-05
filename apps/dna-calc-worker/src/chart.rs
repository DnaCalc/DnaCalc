//! Authored charts drawn as SVG (dc-6yw): calc-host's [`ChartView`] (its series as the workbook
//! holds them now) laid out in the chart's frame as Excel's default style lays it out: the title
//! at the top, the legend at the right, a value axis from a rounded range with gridlines, and the
//! plot's marks — clustered or stacked columns and bars, lines, areas, pie and doughnut slices,
//! scatter markers. A kind not drawn here says so in its frame.
//!
//! Every mark carries `data-series`, `data-point` and `data-value`, so what a chart shows can be
//! read off the page. Text from the workbook (titles, names, labels) is escaped.

use calc_host::chart::text_of;
use calc_host::{ChartKind, ChartView, PlotView};
use std::fmt::Write;

const FONT: &str = "font-family=\"Calibri,'Segoe UI',system-ui,sans-serif\"";
const AXIS: &str = "#595959";
const GRID: &str = "#d9d9d9";

/// `text` with XML's special characters escaped.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// General numeric labels retain fractional bounds and units without binary arithmetic tails.
fn label(value: f64, step: f64) -> String {
    let _=step;
    calc_func::general_format(&calc_host::FormatProfile::for_culture("en-US").expect("en-US format profile"),value)
}

/// An axis over `low..=high`: steps of 1, 2 or 5 times a power of ten, at most about eight over
/// the values, from zero when the values allow it, and a twentieth of the range of room past the
/// highest value (as Excel's 0 to 45 for values up to 40). Excel also fits the step to the axis'
/// length on screen, which is not modelled.
pub use calc_host::chart::axis;

/// The chart's markup, `width` by `height` pixels.
pub fn chart_svg(chart: &ChartView, width: f64, height: f64) -> String {
    let mut svg = String::new();
    let _ = write!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100%\" height=\"100%\" viewBox=\"0 0 {width:.1} {height:.1}\" preserveAspectRatio=\"none\" {FONT} font-size=\"11\">"
    );
    let _ = write!(
        svg,
        "<rect x=\"0.5\" y=\"0.5\" width=\"{:.1}\" height=\"{:.1}\" fill=\"#fff\" stroke=\"{GRID}\"/>",
        (width - 1.0).max(0.0),
        (height - 1.0).max(0.0)
    );
    let mut top = 10.0;
    if let Some(title) = &chart.title {
        let _ = write!(
            svg,
            "<text data-dnacalc=\"chart-title\" x=\"{:.1}\" y=\"22\" text-anchor=\"middle\" font-size=\"14\" font-weight=\"600\" fill=\"{AXIS}\">{}</text>",
            width / 2.0,
            escape(title)
        );
        top = 34.0;
    }
    if chart.plots.len()>1 {let _=write!(svg,"<text data-dnacalc=\"chart-notice\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"{AXIS}\">Combination chart preserved; its plots are not drawn here</text></svg>",width/2.,height/2.);return svg;}
    if chart.plots.iter().any(|p|p.percent){let _=write!(svg,"<text data-dnacalc=\"chart-notice\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"{AXIS}\">Percent-stacked chart preserved; its plots are not drawn here</text></svg>",width/2.,height/2.);return svg;}
    let cached=chart.plots.iter().flat_map(|p|&p.series).any(|s|s.cached);
    if cached {let _=write!(svg,"<text data-dnacalc=\"chart-cache\" x=\"10\" y=\"{:.1}\" fill=\"{AXIS}\">Includes saved chart values</text>",height-5.);}
    let Some(plot) = chart.plots.first() else {
        svg.push_str("</svg>");
        return svg;
    };
    let pie = matches!(plot.kind, ChartKind::Pie | ChartKind::Doughnut);
    // The legend's entries: the series, or a pie's points.
    let entries: Vec<(String, String)> = if pie {
        let series = plot.series.first();
        let count = series.map_or(0, |s| s.values.len());
        (0..count)
            .map(|j| {
                let name = series
                    .and_then(|s| s.categories.get(j).cloned().flatten())
                    .map(|v| text_of(&v))
                    .unwrap_or_else(|| (j + 1).to_string());
                (name, chart.palette[j % chart.palette.len().max(1)].clone())
            })
            .collect()
    } else {
        plot.series
            .iter()
            .map(|s| (s.name.clone(), s.color.clone()))
            .collect()
    };
    let mut right = width - 10.0;
    if chart.legend.is_some() && !entries.is_empty() {
        let legend_width = 90.0f64.min(width * 0.3);
        right = width - legend_width - 6.0;
        let first = (height - entries.len() as f64 * 16.0) / 2.0;
        for (i, (name, color)) in entries.iter().enumerate() {
            let y = first + i as f64 * 16.0 + 8.0;
            let _ = write!(
                svg,
                "<g data-dnacalc=\"chart-legend\"><rect x=\"{:.1}\" y=\"{:.1}\" width=\"7\" height=\"7\" fill=\"#{color}\"/><text x=\"{:.1}\" y=\"{:.1}\" fill=\"{AXIS}\">{}</text></g>",
                right + 8.0,
                y - 6.0,
                right + 19.0,
                y + 1.0,
                escape(name)
            );
        }
    }
    let area = Area {
        left: 10.0,
        top,
        right,
        bottom: height - if cached{26.0}else{10.0},
    };
    match &plot.kind {
        ChartKind::Pie | ChartKind::Doughnut => {
            pie_marks(&mut svg, chart, plot, &area);
        }
        ChartKind::Scatter => scatter_marks(&mut svg, chart, plot, area),
        ChartKind::Column | ChartKind::Bar | ChartKind::Line | ChartKind::Area => {
            category_marks(&mut svg, chart, plot, area);
        }
        ChartKind::Other(kind) => {
            let _ = write!(
                svg,
                "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"{AXIS}\">A {} is not drawn here</text>",
                width / 2.0,
                height / 2.0,
                escape(kind)
            );
        }
    }
    svg.push_str("</svg>");
    svg
}

#[derive(Clone, Copy)]
struct Area {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

fn mark(svg: &mut String, series: usize, point: usize, value: f64) {
    let _ = write!(
        svg,
        " data-series=\"{series}\" data-point=\"{point}\" data-value=\"{value}\""
    );
}

/// Columns, bars, lines and areas over categories.
fn category_marks(svg: &mut String, chart:&ChartView, plot: &PlotView, area: Area) {
    let horizontal = plot.kind == ChartKind::Bar;
    let count = plot
        .series
        .iter()
        .map(|s| s.values.len())
        .max()
        .unwrap_or(0)
        .max(1);
    let (low,high)=calc_host::chart::plot_value_range(plot);
    let axes=match calc_host::chart::value_axes(&chart.axes,&plot.axis_ids,false){Ok(axes)=>axes,Err(message)=>{axis_notice(svg,&area,&message);return;}};
    let scale=match calc_host::chart::chart_scale(low,high,axes.first().copied()) {
        Ok(scale)=>scale,Err(message)=>{axis_notice(svg,&area,&message);return;}
    };
    let (min,max,step)=(scale.minimum,scale.maximum,scale.step);
    // Room for the value labels and the category labels.
    let widest = [min, max]
        .iter()
        .map(|v| label(*v, step).len())
        .max()
        .unwrap_or(1) as f64;
    let (plot_left, plot_bottom) = if horizontal {
        (area.left + 40.0, area.bottom - 16.0)
    } else {
        (area.left + widest * 6.5 + 8.0, area.bottom - 18.0)
    };
    let (x0, x1, y0, y1) = (plot_left, area.right, area.top, plot_bottom);
    // Where a value is along the value axis, and where a category's band starts.
    let value_at = |v: f64| {
        let t = (v - min) / (max - min);
        let t=if scale.reversed{1.-t}else{t};
        if horizontal {
            x0 + t * (x1 - x0)
        } else {
            y1 - t * (y1 - y0)
        }
    };
    let band = if horizontal {
        (y1 - y0) / count as f64
    } else {
        (x1 - x0) / count as f64
    };
    // A bar chart's first category is at the bottom, as Excel draws it.
    let band_start = |j: usize| {
        if horizontal {
            y1 - (j + 1) as f64 * band
        } else {
            x0 + j as f64 * band
        }
    };
    // Gridlines and value labels.
    let steps = ((max - min) / step+1e-9).floor() as usize;
    for k in 0..=steps {
        let v = min + k as f64 * step;
        let p = value_at(v);
        if horizontal {
            let _ = write!(
                svg,
                "<line x1=\"{p:.1}\" y1=\"{y0:.1}\" x2=\"{p:.1}\" y2=\"{y1:.1}\" stroke=\"{GRID}\"/><text data-dnacalc=\"chart-axis-tick\" x=\"{p:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"{AXIS}\">{}</text>",
                y1 + 13.0,
                label(v, step)
            );
        } else {
            let _ = write!(
                svg,
                "<line x1=\"{x0:.1}\" y1=\"{p:.1}\" x2=\"{x1:.1}\" y2=\"{p:.1}\" stroke=\"{GRID}\"/><text data-dnacalc=\"chart-axis-tick\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" fill=\"{AXIS}\">{}</text>",
                x0 - 4.0,
                p + 4.0,
                label(v, step)
            );
        }
    }
    // Where a category's point or label is: the middle of its band, or for an area the band's
    // edge, from the first category's to the last's (Excel's `crossBetween` of each).
    let at_edges = plot.kind == ChartKind::Area && count > 1;
    let middle_of = |j: usize| {
        if at_edges {
            x0 + j as f64 * (x1 - x0) / (count - 1) as f64
        } else {
            band_start(j) + band / 2.0
        }
    };
    // Category labels, from the first series that has them.
    let categories = plot
        .series
        .iter()
        .find(|s| !s.categories.is_empty())
        .map(|s| s.categories.clone())
        .unwrap_or_default();
    for j in 0..count {
        let text = categories
            .get(j)
            .cloned()
            .flatten()
            .map(|v| text_of(&v))
            .unwrap_or_else(|| (j + 1).to_string());
        let middle = middle_of(j);
        if horizontal {
            let _ = write!(
                svg,
                "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" fill=\"{AXIS}\">{}</text>",
                x0 - 4.0,
                middle + 4.0,
                escape(&text)
            );
        } else {
            let _ = write!(
                svg,
                "<text x=\"{middle:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"{AXIS}\">{}</text>",
                y1 + 13.0,
                escape(&text)
            );
        }
    }
    let _=write!(svg,"<svg data-dnacalc=\"chart-plot\" data-axis-min=\"{min}\" data-axis-max=\"{max}\" data-axis-unit=\"{step}\" data-axis-reversed=\"{}\" x=\"{x0}\" y=\"{y0}\" width=\"{}\" height=\"{}\" viewBox=\"{x0} {y0} {} {}\" overflow=\"hidden\">",scale.reversed,x1-x0,y1-y0,x1-x0,y1-y0);
    let zero = value_at(0.0_f64.clamp(min, max));
    match plot.kind {
        ChartKind::Column | ChartKind::Bar => {
            // Excel's default gap between categories is 150% of a bar.
            let group = band / 2.5;
            let width = if plot.stacked {
                group
            } else {
                group / plot.series.len().max(1) as f64
            };
            let mut up = vec![0.0f64; count];
            let mut down = vec![0.0f64; count];
            for (i, series) in plot.series.iter().enumerate() {
                for (j, value) in series.values.iter().enumerate() {
                    let Some(v) = *value else { continue };
                    let (from, to) = if plot.stacked {
                        let base = if v >= 0.0 { &mut up[j] } else { &mut down[j] };
                        let from = *base;
                        *base += v;
                        (value_at(from), value_at(*base))
                    } else {
                        (zero, value_at(v))
                    };
                    let offset = band_start(j)
                        + (band - group) / 2.0
                        + if plot.stacked { 0.0 } else { i as f64 * width };
                    let (x, y, w, h) = if horizontal {
                        (from.min(to), offset, (to - from).abs(), width)
                    } else {
                        (offset, from.min(to), width, (to - from).abs())
                    };
                    let _ = write!(
                        svg,
                        "<rect data-dnacalc=\"chart-mark\" x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" fill=\"#{}\"",
                        series.color
                    );
                    mark(svg, i, j, v);
                    svg.push_str("/>");
                }
            }
        }
        ChartKind::Line | ChartKind::Area => {
            let mut base = vec![0.0f64; count];
            let x_at = middle_of;
            let mut shapes = String::new();
            for (i, series) in plot.series.iter().enumerate() {
                let points: Vec<(usize, f64)> = series
                    .values
                    .iter()
                    .enumerate()
                    .filter_map(|(j, v)| v.map(|v| (j, v)))
                    .collect();
                let tops: Vec<(f64, f64)> = points
                    .iter()
                    .map(|&(j, v)| {
                        let total = if plot.stacked { base[j] + v } else { v };
                        (x_at(j), value_at(total))
                    })
                    .collect();
                if plot.kind == ChartKind::Area && !tops.is_empty() {
                    let mut d = String::new();
                    for (k, (x, y)) in tops.iter().enumerate() {
                        let _ = write!(d, "{}{x:.1} {y:.1} ", if k == 0 { "M" } else { "L" });
                    }
                    for &(j, _) in points.iter().rev() {
                        let below = if plot.stacked {
                            value_at(base[j])
                        } else {
                            zero
                        };
                        let _ = write!(d, "L{:.1} {below:.1} ", x_at(j));
                    }
                    let _ = write!(
                        shapes,
                        "<path d=\"{}Z\" fill=\"#{}\" data-series=\"{i}\"/>",
                        d, series.color
                    );
                } else if plot.kind == ChartKind::Line {
                    // A gap breaks the line, as Excel's default for empty cells does.
                    let mut d = String::new();
                    let mut last: Option<usize> = None;
                    for (&(j, _), (x, y)) in points.iter().zip(&tops) {
                        let join = last.is_some_and(|l| l + 1 == j);
                        let _ = write!(d, "{}{x:.1} {y:.1} ", if join { "L" } else { "M" });
                        last = Some(j);
                    }
                    let _ = write!(
                        shapes,
                        "<path d=\"{d}\" fill=\"none\" stroke=\"#{}\" stroke-width=\"2.25\" stroke-linejoin=\"round\" data-series=\"{i}\"/>",
                        series.color
                    );
                }
                for (&(j, v), (x, y)) in points.iter().zip(&tops) {
                    let _ = write!(
                        shapes,
                        "<circle data-dnacalc=\"chart-mark\" cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"{}\" fill=\"#{}\"",
                        if plot.kind == ChartKind::Line {
                            1.5
                        } else {
                            0.0
                        },
                        series.color
                    );
                    mark(&mut shapes, i, j, v);
                    shapes.push_str("/>");
                    if plot.stacked {
                        base[j] += v;
                    }
                }
            }
            svg.push_str(&shapes);
        }
        _ => {}
    }
    // The category axis line, at zero.
    if horizontal {
        let _ = write!(
            svg,
            "<line x1=\"{zero:.1}\" y1=\"{y0:.1}\" x2=\"{zero:.1}\" y2=\"{y1:.1}\" stroke=\"{AXIS}\"/>"
        );
    } else {
        let _ = write!(
            svg,
            "<line x1=\"{x0:.1}\" y1=\"{zero:.1}\" x2=\"{x1:.1}\" y2=\"{zero:.1}\" stroke=\"{AXIS}\"/>"
        );
    }
    svg.push_str("</svg>");
}

/// A pie's or doughnut's slices: the first series' positive values, clockwise from twelve.
fn pie_marks(svg: &mut String, chart: &ChartView, plot: &PlotView, area: &Area) {
    let Some(series) = plot.series.first() else {
        return;
    };
    let total: f64 = series.values.iter().flatten().filter(|v| **v > 0.0).sum();
    let (cx, cy) = (
        (area.left + area.right) / 2.0,
        (area.top + area.bottom) / 2.0,
    );
    let r = ((area.right - area.left).min(area.bottom - area.top) / 2.0 - 4.0).max(1.0);
    let hole = if plot.kind == ChartKind::Doughnut {
        r * 0.5
    } else {
        0.0
    };
    if total <= 0.0 {
        return;
    }
    let point = |angle: f64, radius: f64| {
        let a = angle - std::f64::consts::FRAC_PI_2;
        (cx + radius * a.cos(), cy + radius * a.sin())
    };
    let mut angle = 0.0;
    for (j, value) in series.values.iter().enumerate() {
        let Some(v) = value.filter(|v| *v > 0.0) else {
            continue;
        };
        let sweep = v / total * std::f64::consts::TAU;
        let color = if plot.vary_colors {
            chart.palette[j % chart.palette.len().max(1)].clone()
        } else {
            series.color.clone()
        };
        let large = i32::from(sweep > std::f64::consts::PI);
        let d = if sweep >= std::f64::consts::TAU - 1e-9 {
            // One slice is the whole circle.
            format!(
                "M{:.1} {:.1}a{r:.1} {r:.1} 0 1 1 0 {:.1}a{r:.1} {r:.1} 0 1 1 0 {:.1}Z",
                cx,
                cy - r,
                2.0 * r,
                -2.0 * r
            )
        } else {
            let (ax, ay) = point(angle, r);
            let (bx, by) = point(angle + sweep, r);
            let (hx, hy) = point(angle + sweep, hole);
            let (gx, gy) = point(angle, hole);
            if hole > 0.0 {
                format!(
                    "M{ax:.1} {ay:.1}A{r:.1} {r:.1} 0 {large} 1 {bx:.1} {by:.1}L{hx:.1} {hy:.1}A{hole:.1} {hole:.1} 0 {large} 0 {gx:.1} {gy:.1}Z"
                )
            } else {
                format!(
                    "M{cx:.1} {cy:.1}L{ax:.1} {ay:.1}A{r:.1} {r:.1} 0 {large} 1 {bx:.1} {by:.1}Z"
                )
            }
        };
        let _ = write!(
            svg,
            "<path data-dnacalc=\"chart-mark\" d=\"{d}\" fill=\"#{color}\" stroke=\"#fff\" stroke-width=\"1\""
        );
        mark(svg, 0, j, v);
        svg.push_str("/>");
        angle += sweep;
    }
}

/// A scatter plot's markers, on value axes for X and Y.
fn scatter_marks(svg: &mut String, chart:&ChartView, plot: &PlotView, area: Area) {
    let axes=match calc_host::chart::value_axes(&chart.axes,&plot.axis_ids,true){Ok(axes)=>axes,Err(message)=>{axis_notice(svg,&area,&message);return;}};
    let series=plot.series.iter().map(|s|(calc_host::chart::scatter_x_values(s),s)).collect::<Vec<_>>();
    if !series.iter().any(|(xs,s)|xs.iter().zip(&s.values).any(|(x,y)|x.is_some()&&y.is_some())){return;}
    let ((xl,xh),(yl,yh))=calc_host::chart::scatter_ranges(plot);
    let xscale=calc_host::chart::chart_scale(xl,xh,Some(axes[0]));
    let yscale=calc_host::chart::chart_scale(yl,yh,Some(axes[1]));
    let (xscale,yscale)=match (xscale,yscale){(Ok(x),Ok(y))=>(x,y),(Err(message),_)|(_,Err(message))=>{axis_notice(svg,&area,&message);return;}};
    let (xmin,xmax,xstep)=(xscale.minimum,xscale.maximum,xscale.step);let (ymin,ymax,ystep)=(yscale.minimum,yscale.maximum,yscale.step);
    let widest = label(ymax, ystep).len().max(label(ymin, ystep).len()) as f64;
    let (x0, x1, y0, y1) = (
        area.left + widest * 6.5 + 8.0,
        area.right,
        area.top,
        area.bottom - 18.0,
    );
    let px = |x: f64| {let t=(x-xmin)/(xmax-xmin);x0+(if xscale.reversed{1.-t}else{t})*(x1-x0)};
    let py = |y: f64| {let t=(y-ymin)/(ymax-ymin);y1-(if yscale.reversed{1.-t}else{t})*(y1-y0)};
    for k in 0..=((ymax - ymin) / ystep+1e-9).floor() as usize {
        let v = ymin + k as f64 * ystep;
        let _ = write!(
            svg,
            "<line x1=\"{x0:.1}\" y1=\"{:.1}\" x2=\"{x1:.1}\" y2=\"{:.1}\" stroke=\"{GRID}\"/><text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" fill=\"{AXIS}\">{}</text>",
            py(v),
            py(v),
            x0 - 4.0,
            py(v) + 4.0,
            label(v, ystep)
        );
    }
    for k in 0..=((xmax - xmin) / xstep+1e-9).floor() as usize {
        let v = xmin + k as f64 * xstep;
        let _ = write!(
            svg,
            "<line x1=\"{:.1}\" y1=\"{y0:.1}\" x2=\"{:.1}\" y2=\"{y1:.1}\" stroke=\"{GRID}\"/><text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"{AXIS}\">{}</text>",
            px(v),
            px(v),
            px(v),
            y1 + 13.0,
            label(v, xstep)
        );
    }
    let _=write!(svg,"<svg data-dnacalc=\"chart-plot\" data-x-min=\"{xmin}\" data-x-max=\"{xmax}\" data-y-min=\"{ymin}\" data-y-max=\"{ymax}\" x=\"{x0}\" y=\"{y0}\" width=\"{}\" height=\"{}\" viewBox=\"{x0} {y0} {} {}\" overflow=\"hidden\">",x1-x0,y1-y0,x1-x0,y1-y0);
    for (i, (xs, s)) in series.iter().enumerate() {
        for (j, (x, y)) in xs.iter().zip(&s.values).enumerate() {
            if let (Some(x), Some(y)) = (x, y) {
                let _ = write!(
                    svg,
                    // Excel's first marker, a diamond.
                    "<path data-dnacalc=\"chart-mark\" d=\"M{:.1} {:.1}l4 4-4 4-4-4z\" fill=\"#{}\"",
                    px(*x),
                    py(*y) - 4.0,
                    s.color
                );
                mark(svg, i, j, *y);
                let _=write!(svg," data-x=\"{x}\"");
                svg.push_str("/>");
            }
        }
    }
    svg.push_str("</svg>");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn imported_plot_marks_and_saved_value_notices_use_the_host_projection(){
        use std::sync::Arc;use calc_host::{HostSession,MemorySource,PackageRead,OpenPolicy,HostOptions,SkinIntent};
        let bytes=include_bytes!("../../../crates/calc-host/tests/fixtures/dc_6yw/charts.xlsx");let source:Arc<dyn PackageRead>=Arc::new(MemorySource::new(bytes.to_vec()));let mut book=HostSession::open(source,&OpenPolicy::default(),HostOptions::default()).unwrap();assert!(book.dispatch(SkinIntent::Recalculate).accepted());let (charts,unread)=book.host().charts(book.host().sheets()[0].id());assert_eq!(unread,0);assert_eq!(charts.len(),6);
        for chart in &charts{let svg=chart_svg(chart,600.,350.);assert!(svg.contains("data-point=\"0\""),"{}: {svg}",chart.name);assert!(!svg.contains("chart-cache"));}
        let mut cached=charts[0].clone();cached.plots[0].series[0].cached=true;assert!(chart_svg(&cached,600.,350.).contains("Includes saved chart values"));cached.plots.push(cached.plots[0].clone());let svg=chart_svg(&cached,600.,350.);assert!(svg.contains("Combination chart preserved"));assert!(!svg.contains("data-point="));
    }

    /// Native-only: the axis arithmetic and escaping, without a page.
    #[test]
    fn axes_round_out_and_text_is_escaped() {
        assert_eq!(axis(0.0, 27.0), (0.0, 30.0, 5.0));
        assert_eq!(axis(0.0, 40.0), (0.0, 45.0, 5.0));
        assert_eq!(axis(-3.0, 7.0), (-4.0, 8.0, 2.0));
        assert_eq!(label(0.5, 0.1), "0.5");
        assert_eq!(escape("a<b & \"c\""), "a&lt;b &amp; &quot;c&quot;");
    }
}

fn axis_notice(svg:&mut String,area:&Area,message:&str){let _=write!(svg,"<text data-dnacalc=\"chart-notice\" x=\"{}\" y=\"{}\" text-anchor=\"middle\" fill=\"{AXIS}\">{}</text>",(area.left+area.right)/2.,(area.top+area.bottom)/2.,escape(message));}
