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
    let _ = step;
    calc_func::general_format(
        &calc_host::FormatProfile::for_culture("en-US").expect("en-US format profile"),
        value,
    )
}

/// Format a numeric tick with an explicit workbook axis format when supported.
fn axis_label(chart: &ChartView, axis_id: u32, value: f64, step: f64) -> String {
    let format = chart.axes.iter().find(|axis| axis.id == axis_id).and_then(|axis| axis.number_format.as_deref());
    format.and_then(|code| calc_func::cell_text(&chart.format_profile, chart.date_system,
        &calc_value::CalcValue::number(value), code).ok())
        .unwrap_or_else(|| label(value, step))
}

/// A validated explicit gridline RGB, or the existing default.
fn gridline_color(chart: &ChartView, axis_id: u32) -> String {
    chart.axes.iter().find(|axis| axis.id == axis_id)
        .and_then(|axis| axis.major_gridline_color.as_deref())
        .filter(|color| color.len() == 6 && color.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(|color| format!("#{color}"))
        .unwrap_or_else(|| GRID.to_owned())
}

/// An axis over `low..=high`: steps of 1, 2 or 5 times a power of ten, at most about eight over
/// the values, from zero when the values allow it, and a twentieth of the range of room past the
/// highest value (as Excel's 0 to 45 for values up to 40). Excel also fits the step to the axis'
/// length on screen, which is not modelled.
#[cfg(test)]
pub use calc_host::chart::axis;

fn font_pixels(chart: &ChartView) -> f64 {
    chart
        .text_style
        .size
        .filter(|n| (100..=40900).contains(n))
        .map_or(11., |n| f64::from(n) / 100. * 96. / 72.)
}
/// Generated SVG attributes are double quoted; only text tags enter this formatter.
fn svg_attribute(tag: &mut String, name: &str, value: &str) {
    let key = format!(" {name}=\"");
    if let Some(start) = tag.find(&key) {
        let value_start = start + key.len();
        if let Some(end) = tag[value_start..].find('"') {
            tag.replace_range(value_start..value_start + end, value);
            return;
        }
    }
    let at = tag.len().saturating_sub(1);
    tag.insert_str(at, &format!(" {name}=\"{value}\""));
}
fn text_colour(svg: String, chart: &ChartView) -> String {
    let mut result = String::with_capacity(svg.len());
    let mut rest = svg.as_str();
    while let Some(start) = rest.find("<text ") {
        result.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let mut tag = rest[..=end].to_owned();
        if let Some(color) = &chart.text_style.color {
            if tag.contains(&format!("fill=\"{AXIS}\"")) {
                svg_attribute(&mut tag, "fill", &format!("#{}", escape(color)));
            }
        }
        let target = tag
            .split(" data-chart-text=\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .map(str::to_owned);
        if let Some(id) = target.as_deref().and_then(|s| s.strip_prefix("axis:"))
            .and_then(|s| s.parse::<u32>().ok())
            && chart.axes.iter().any(|axis| axis.id == id && axis.tick_labels_hidden)
        {
            svg_attribute(&mut tag, "visibility", "hidden");
        }
        if target.as_deref() == Some("title") && let Some(bold) = chart.text_style.bold {
            svg_attribute(&mut tag, "font-weight", if bold { "700" } else { "400" });
        }
        if let Some(style) = target.as_ref().and_then(|key| chart.element_text.get(key)) {
            if let Some(size) = style.size.filter(|n| (100..=40900).contains(n)) {
                svg_attribute(
                    &mut tag,
                    "font-size",
                    &format!("{:.2}", f64::from(size) / 100. * 96. / 72.),
                );
            }
            if let Some(family) = &style.family {
                svg_attribute(
                    &mut tag,
                    "font-family",
                    &escape(&format!(
                        "\"{}\",Calibri,'Segoe UI',system-ui,sans-serif",
                        family.replace('\\', "\\\\").replace('"', "\\\"")
                    )),
                );
            }
            if let Some(bold) = style.bold {
                svg_attribute(&mut tag, "font-weight", if bold { "700" } else { "400" });
            }
            if let Some(italic) = style.italic {
                svg_attribute(
                    &mut tag,
                    "font-style",
                    if italic { "italic" } else { "normal" },
                );
            }
            if let Some(color) = &style.color {
                svg_attribute(&mut tag, "fill", &format!("#{}", escape(color)));
            }
        }
        result.push_str(&tag);
        rest = &rest[end + 1..];
    }
    result.push_str(rest);
    result
}

/// The chart's markup, `width` by `height` pixels.
pub fn chart_svg(chart: &ChartView, width: f64, height: f64) -> String {
    render_chart(chart, width, height, None)
}

/// Overlay matching line series from a pinned publication on the active chart's axes.
pub fn chart_comparison_svg(
    chart: &ChartView,
    baseline: &ChartView,
    width: f64,
    height: f64,
) -> Option<String> {
    let [active] = chart.plots.as_slice() else { return None; };
    let [previous] = baseline.plots.as_slice() else { return None; };
    if active.kind != ChartKind::Line
        || previous.kind != ChartKind::Line
        || active.stacked
        || previous.stacked
        || active.percent
        || previous.percent
        || active.series.is_empty()
        || active.series.len() != previous.series.len()
        || active.series.iter().zip(&previous.series).any(|(active, old)| {
            active.cached || old.cached || active.categories != old.categories
                || active.values.len() != old.values.len()
        })
    {
        return None;
    }
    let mark = active.series.len();
    let overlays: Vec<_> = active.series.iter().zip(&previous.series).map(|(active, old)| {
        let mut series = old.clone();
        series.color.clone_from(&active.color);
        series.error_bars.clear();
        series.trendlines.clear();
        series.labels = None;
        series.value_labels.clear();
        series
    }).collect();
    let mut combined = chart.clone();
    combined.plots[0].series.extend(overlays);
    Some(render_chart(&combined, width, height, Some(mark)))
}

fn render_chart(chart: &ChartView, width: f64, height: f64, baseline: Option<usize>) -> String {
    let mut svg = String::new();
    let text_pixels = font_pixels(chart);
    let family = chart
        .text_style
        .family
        .as_ref()
        .map(|s| {
            format!(
                "\"{}\",Calibri,'Segoe UI',system-ui,sans-serif",
                s.replace('\\', "\\\\").replace('"', "\\\"")
            )
        })
        .unwrap_or_else(|| "Calibri,'Segoe UI',system-ui,sans-serif".into());
    let weight = if chart.text_style.bold == Some(true) {
        "700"
    } else {
        "400"
    };
    let italic = if chart.text_style.italic == Some(true) {
        "italic"
    } else {
        "normal"
    };
    let _ = write!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100%\" height=\"100%\" viewBox=\"0 0 {width:.1} {height:.1}\" preserveAspectRatio=\"none\" font-family=\"{}\" font-size=\"{text_pixels:.2}\" font-weight=\"{weight}\" font-style=\"{italic}\">",
        escape(&family)
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
            "<text data-dnacalc=\"chart-title\" data-chart-text=\"title\" x=\"{:.1}\" y=\"22\" text-anchor=\"middle\" font-size=\"14\" font-weight=\"600\" fill=\"{AXIS}\">{}</text>",
            width / 2.0,
            escape(title)
        );
        top = 34.0;
    }
    if !calc_host::chart::axes_associated(chart) {
        axis_notice(
            &mut svg,
            &Area {
                left: 0.,
                top: 0.,
                right: width,
                bottom: height,
            },
            "Unassociated chart axes are preserved but not drawn here (dc-zjsg)",
        );
        svg.push_str("</svg>");
        return text_colour(svg, chart);
    }
    if chart.plots.len() > 1 && !calc_host::chart::combination_supported(chart) {
        let _ = write!(
            svg,
            "<text data-dnacalc=\"chart-notice\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"{AXIS}\">Combination chart preserved; its plots are not drawn here</text></svg>",
            width / 2.,
            height / 2.
        );
        return text_colour(svg, chart);
    }
    if chart.plots.iter().any(|p| p.percent) {
        let _ = write!(
            svg,
            "<text data-dnacalc=\"chart-notice\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"{AXIS}\">Percent-stacked chart preserved; its plots are not drawn here</text></svg>",
            width / 2.,
            height / 2.
        );
        return text_colour(svg, chart);
    }
    let cached = chart.plots.iter().flat_map(|p| &p.series).any(|s| s.cached);
    if cached {
        let _ = write!(
            svg,
            "<text data-dnacalc=\"chart-cache\" x=\"10\" y=\"{:.1}\" fill=\"{AXIS}\">Includes saved chart values</text>",
            height - 5.
        );
    }
    let Some(plot) = chart.plots.first() else {
        svg.push_str("</svg>");
        return text_colour(svg, chart);
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
                (
                    name,
                    if plot.vary_colors && !series.is_some_and(|s| s.explicit_color) {
                        chart.palette[j % chart.palette.len().max(1)].clone()
                    } else {
                        series.map(|s| s.color.clone()).unwrap_or_default()
                    },
                )
            })
            .collect()
    } else {
        chart
            .plots
            .iter()
            .flat_map(|p| &p.series)
            .take(baseline.unwrap_or(usize::MAX))
            .map(|s| (s.name.clone(), s.color.clone()))
            .collect()
    };
    let mut left = 10.;
    let mut right = width - 10.;
    let mut bottom = height - if cached { 26. } else { 10. };
    if chart.legend.is_some() && !entries.is_empty() {
        let legend_pixels = chart
            .element_text
            .get("legend")
            .and_then(|s| s.size)
            .filter(|n| (100..=40900).contains(n))
            .map_or_else(|| font_pixels(chart), |n| f64::from(n) / 100. * 96. / 72.);
        let position = match chart.legend.as_deref() {
            Some("l") => "l",
            Some("t") => "t",
            Some("b") => "b",
            Some("tr") => "tr",
            _ => "r",
        };
        // Bound the legend before laying out the plot. Long labels are clipped in their own
        // nested SVG, with the complete names retained in text and accessible descriptions.
        let horizontal = matches!(position, "t" | "b");
        let available_h = (bottom - top).max(1.);
        let max_h = if horizontal {
            available_h * 0.4
        } else {
            available_h
        };
        let max_w = if horizontal {
            (width - 20.).max(1.)
        } else {
            width * 0.4
        };
        let _ = write!(
            svg,
            "<desc data-dnacalc=\"chart-legend-description\">Legend: {}</desc>",
            escape(
                &entries
                    .iter()
                    .map(|(n, _)| n.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        );
        let row_height = (legend_pixels * 1.5 + 2.).max(18.);
        let baseline = legend_pixels + 2.;
        if max_h >= row_height && max_w >= 24. {
            let desired = entries
                .iter()
                .map(|(n, _)| n.chars().count() as f64 * legend_pixels * 0.6 + 22.)
                .fold(80., f64::max)
                .min(160.);
            let rows = (max_h / row_height).floor().max(1.) as usize;
            let max_columns = (max_w / 80.).floor().max(1.) as usize;
            let columns = if horizontal {
                ((max_w / desired).floor().max(1.) as usize)
                    .max(entries.len().div_ceil(rows))
                    .min(max_columns)
            } else {
                entries.len().div_ceil(rows).min(max_columns).max(1)
            };
            let rows = entries.len().div_ceil(columns).min(rows).max(1);
            let capacity = columns * rows;
            let overflow = entries.len() > capacity;
            let visible = entries
                .len()
                .min(capacity.saturating_sub(usize::from(overflow)));
            let slot = (if horizontal {
                max_w / columns as f64
            } else {
                desired.min(max_w / columns as f64)
            })
            .max(1.);
            let legend_w = slot * columns as f64;
            let legend_h = rows as f64 * row_height;
            let x = match position {
                "l" => {
                    left += legend_w + 6.;
                    10.
                }
                "t" | "b" => 10.,
                _ => {
                    right = width - legend_w - 6.;
                    right + 6.
                }
            };
            let y = match position {
                "t" => {
                    let y = top;
                    top += legend_h + 6.;
                    y
                }
                "b" => {
                    bottom -= legend_h + 6.;
                    bottom + 6.
                }
                "tr" => top,
                _ => top + (available_h - legend_h).max(0.) / 2.,
            };
            for (i, (name, color)) in entries.iter().take(visible).enumerate() {
                let col = if horizontal { i % columns } else { i / rows };
                let row = if horizontal { i / columns } else { i % rows };
                let (x, y) = (x + col as f64 * slot, y + row as f64 * row_height);
                let _ = write!(
                    svg,
                    "<g data-dnacalc=\"chart-legend\" data-position=\"{position}\" aria-label=\"{}\"><svg x=\"{x:.1}\" y=\"{y:.1}\" width=\"{slot:.1}\" height=\"{row_height:.1}\" overflow=\"hidden\"><rect x=\"0\" y=\"4\" width=\"7\" height=\"7\" fill=\"#{color}\"/><text data-chart-text=\"legend\" x=\"11\" y=\"{baseline:.1}\" fill=\"{AXIS}\">{}</text></svg></g>",
                    escape(name),
                    escape(name)
                );
            }
            if overflow {
                let i = visible;
                let col = if horizontal { i % columns } else { i / rows };
                let row = if horizontal { i / columns } else { i % rows };
                let _ = write!(
                    svg,
                    "<svg x=\"{:.1}\" y=\"{:.1}\" width=\"{slot:.1}\" height=\"{row_height:.1}\" overflow=\"hidden\"><text data-dnacalc=\"chart-legend-overflow\" data-chart-text=\"legend\" x=\"0\" y=\"{baseline:.1}\" fill=\"{AXIS}\">+{} entries</text></svg>",
                    x + col as f64 * slot,
                    y + row as f64 * row_height,
                    entries.len() - visible
                );
            }
        }
    }
    if !pie {
        // Reserve a strip for each associated axis title, outside the tick labels.
        // The precise Excel text metrics and manual title positions are not modelled here.
        for axis in &chart.axes {
            let Some(title) = axis.title.as_deref().filter(|s| !s.is_empty()) else { continue };
            if !chart.plots.iter().any(|plot| plot.axis_ids.contains(&axis.id)) {
                continue;
            }
            let (x, y, rotation) = match axis.position.as_deref() {
                Some("l") if right - left > 80. => {
                    let x = left + 11.;
                    left += 20.;
                    (x, (top + bottom) / 2., -90)
                }
                Some("r") if right - left > 80. => {
                    let x = right - 7.;
                    right -= 20.;
                    (x, (top + bottom) / 2., 90)
                }
                Some("t") if bottom - top > 80. => {
                    let y = top + 12.;
                    top += 20.;
                    ((left + right) / 2., y, 0)
                }
                Some("b") if bottom - top > 80. => {
                    let y = bottom - 3.;
                    bottom -= 20.;
                    ((left + right) / 2., y, 0)
                }
                _ => continue,
            };
            let _ = write!(svg,
                "<text data-dnacalc=\"chart-axis-title\" data-chart-text=\"axis-title:{}\" data-axis=\"{}\" x=\"{x:.1}\" y=\"{y:.1}\" text-anchor=\"middle\" transform=\"rotate({rotation} {x:.1} {y:.1})\" fill=\"{AXIS}\">{}</text>",
                axis.id, axis.id, escape(title));
        }
    }
    let area = Area {
        left,
        top,
        right,
        bottom,
    };
    if chart.plots.len() > 1 {
        match combination_layouts(chart, area) {
            Ok(layouts) => {
                // Excel draws area plots under column and bar plots, and those under line plots,
                // whatever order the plots were made in (UI28 combos-created.xlsx).
                let rank = |k: &ChartKind| match k {
                    ChartKind::Area => 0,
                    ChartKind::Column | ChartKind::Bar => 1,
                    _ => 2,
                };
                let mut order: Vec<usize> = (0..chart.plots.len()).collect();
                order.sort_by_key(|&i| rank(&chart.plots[i].kind));
                for i in order {
                    let p = &chart.plots[i];
                    let _ = write!(svg, "<g data-plot=\"{i}\">");
                    category_marks(&mut svg, chart, p, area, Some(layouts[i]), None);
                    svg.push_str("</g>");
                }
            }
            Err(message) => axis_notice(&mut svg, &area, &message),
        }
    } else {
        match &plot.kind {
            ChartKind::Pie | ChartKind::Doughnut => {
                pie_marks(&mut svg, chart, plot, &area);
            }
            ChartKind::Scatter | ChartKind::Bubble => scatter_marks(&mut svg, chart, plot, area),
            ChartKind::Radar => radar_marks(&mut svg, chart, plot, area),
            ChartKind::Waterfall => waterfall_marks(&mut svg, chart, plot, area),
            ChartKind::Funnel => funnel_marks(&mut svg, plot, area),
            ChartKind::BoxWhisker => box_marks(&mut svg, chart, plot, area),
            ChartKind::Histogram => histogram_marks(&mut svg, chart, plot, area),
            ChartKind::Surface => surface_marks(&mut svg, chart, plot, area),
            ChartKind::Treemap => treemap_marks(&mut svg, chart, plot, area),
            ChartKind::Sunburst => sunburst_marks(&mut svg, chart, plot, area),
            ChartKind::Column | ChartKind::Bar | ChartKind::Line | ChartKind::Area | ChartKind::Stock => {
                category_marks(&mut svg, chart, plot, area, None, baseline);
            }
            ChartKind::Other(kind) if kind.starts_with("cx:") => {
                let _ = write!(
                    svg,
                    "<text data-dnacalc=\"chart-notice\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"{AXIS}\">An Excel 2016 {} chart is preserved but not drawn here (dc-zjsg.6)</text>",
                    width / 2.0,
                    height / 2.0,
                    escape(&kind[3..])
                );
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
    }
    svg.push_str("</svg>");
    text_colour(svg, chart)
}

#[derive(Clone, Copy)]
struct Area {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

/// The points (in data units) of a series' trendline over its points `xy`, from `low` to `high` along x: a least-squares
/// fit of its kind (linear, exponential, logarithmic, power, a polynomial of its order) or a moving average of its
/// period, as Excel draws one (dc-zjsg.6). `None` when the data does not admit the fit (too few points, or zero or
/// negative values a logarithm needs).
fn trendline_points(t: &calc_host::ChartTrendline, xy: &[(f64, f64)], low: f64, high: f64) -> Option<Vec<(f64, f64)>> {
    let number = |v: &Option<String>| v.as_deref().and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite());
    let (low, high) = (low - number(&t.backward).unwrap_or(0.0), high + number(&t.forward).unwrap_or(0.0));
    let samples = |f: &dyn Fn(f64) -> f64, from: f64| -> Vec<(f64, f64)> {
        (0..=64).map(|i| from + (high - from) * i as f64 / 64.0).map(|x| (x, f(x))).filter(|p| p.1.is_finite()).collect()
    };
    // Least squares of y on the columns `basis` gives, with an intercept unless one is held.
    let fit = |xs: &[f64], ys: &[f64], degree: usize, held: Option<f64>| -> Option<Vec<f64>> {
        let n = xs.len();
        if n < degree + if held.is_some() { 0 } else { 1 } {
            return None;
        }
        let first = usize::from(held.is_some());
        let size = degree + 1 - first;
        let mut a = vec![vec![0.0f64; size + 1]; size];
        for (&x, &y) in xs.iter().zip(ys) {
            let y = y - held.unwrap_or(0.0);
            let powers: Vec<f64> = (first..=degree).map(|k| x.powi(k as i32)).collect();
            for r in 0..size {
                for c in 0..size {
                    a[r][c] += powers[r] * powers[c];
                }
                a[r][size] += powers[r] * y;
            }
        }
        // Gaussian elimination with partial pivoting.
        for col in 0..size {
            let pivot = (col..size).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
            if a[pivot][col].abs() < 1e-300 {
                return None;
            }
            a.swap(col, pivot);
            for row in 0..size {
                if row != col {
                    let f = a[row][col] / a[col][col];
                    for k in col..=size {
                        a[row][k] -= f * a[col][k];
                    }
                }
            }
        }
        let mut out: Vec<f64> = (0..size).map(|i| a[i][size] / a[i][i]).collect();
        if let Some(h) = held {
            out.insert(0, h);
        }
        Some(out)
    };
    let (xs, ys): (Vec<f64>, Vec<f64>) = xy.iter().copied().filter(|(x, y)| x.is_finite() && y.is_finite()).unzip();
    let held = number(&t.intercept);
    match t.kind.as_str() {
        "linear" | "" => {
            let c = fit(&xs, &ys, 1, held)?;
            Some(vec![(low, c[0] + c[1] * low), (high, c[0] + c[1] * high)])
        }
        "poly" => {
            let degree = t.order.unwrap_or(2).clamp(2, 6) as usize;
            let c = fit(&xs, &ys, degree, held)?;
            Some(samples(&|x| c.iter().enumerate().map(|(k, c)| c * x.powi(k as i32)).sum(), low))
        }
        "exp" => {
            if ys.iter().any(|y| *y <= 0.0) {
                return None;
            }
            let ln: Vec<f64> = ys.iter().map(|y| y.ln()).collect();
            let c = fit(&xs, &ln, 1, held.filter(|h| *h > 0.0).map(f64::ln))?;
            Some(samples(&|x| (c[0] + c[1] * x).exp(), low))
        }
        "log" => {
            if xs.iter().any(|x| *x <= 0.0) {
                return None;
            }
            let ln: Vec<f64> = xs.iter().map(|x| x.ln()).collect();
            let c = fit(&ln, &ys, 1, None)?;
            Some(samples(&|x| c[0] + c[1] * x.ln(), low.max(f64::MIN_POSITIVE)))
        }
        "power" => {
            if xs.iter().chain(&ys).any(|v| *v <= 0.0) {
                return None;
            }
            let (lx, ly): (Vec<f64>, Vec<f64>) = xs.iter().zip(&ys).map(|(x, y)| (x.ln(), y.ln())).unzip();
            let c = fit(&lx, &ly, 1, None)?;
            Some(samples(&|x| (c[0] + c[1] * x.ln()).exp(), low.max(f64::MIN_POSITIVE)))
        }
        "movingAvg" => {
            let period = t.period.unwrap_or(2).max(2) as usize;
            if xy.len() < period {
                return None;
            }
            Some((period - 1..xy.len()).map(|i| (xy[i].0, xy[i + 1 - period..=i].iter().map(|p| p.1).sum::<f64>() / period as f64)).collect())
        }
        _ => None,
    }
}

/// Where a series' error bars reach at each point, in value units (`low`, `high`, and the value they are drawn
/// about), as Excel computes them (dc-zjsg.6): a fixed amount, a percentage of the value, a number of standard
/// deviations (n - 1) about the series' mean, or the standard error, sqrt(sum of (y - mean)^2 / (n (n - 1))) (Excel-measured); plus, minus or both. A custom error amount is not drawn.
fn error_extents(e: &calc_host::ChartErrorBars, values: &[Option<f64>]) -> Vec<Option<(f64, f64, f64)>> {
    let amount = e.value.as_deref().and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite()).unwrap_or(match e.value_type.as_deref() {
        Some("percentage") => 5.0,
        Some("stdDev") => 1.0,
        _ => 1.0,
    });
    let present: Vec<f64> = values.iter().flatten().copied().filter(|v| v.is_finite()).collect();
    let n = present.len() as f64;
    let mean = present.iter().sum::<f64>() / n.max(1.0);
    let deviation = if n > 1.0 { (present.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt() } else { 0.0 };
    // Measured in Excel (dw2 Windows item 8): the deviations from the mean, not the raw values.
    let standard_error = if n > 1.0 { (present.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n * (n - 1.0))).sqrt() } else { 0.0 };
    values
        .iter()
        .map(|v| {
            let v = (*v).filter(|v| v.is_finite())?;
            let (centre, reach) = match e.value_type.as_deref() {
                Some("cust") => return None,
                Some("percentage") => (v, v.abs() * amount / 100.0),
                Some("stdDev") => (mean, deviation * amount),
                Some("stdErr") => (v, standard_error),
                _ => (v, amount),
            };
            Some(match e.kind.as_deref() {
                Some("plus") => (centre, centre + reach, centre),
                Some("minus") => (centre - reach, centre, centre),
                _ => (centre - reach, centre + reach, centre),
            })
        })
        .collect()
}

/// One error bar from `from` to `to` (pixels), along x when `across`, with end caps of `cap` pixels.
fn error_bar_svg(svg: &mut String, series: usize, point: usize, from: (f64, f64), to: (f64, f64), cap: Option<f64>) {
    let _ = write!(
        svg,
        "<path data-dnacalc=\"chart-error-bar\" data-series=\"{series}\" data-point=\"{point}\" d=\"M{:.1} {:.1}L{:.1} {:.1}",
        from.0, from.1, to.0, to.1
    );
    if let Some(cap) = cap {
        let across = (from.1 - to.1).abs() < 1e-9;
        for (x, y) in [from, to] {
            if across {
                let _ = write!(svg, "M{x:.1} {:.1}L{x:.1} {:.1}", y - cap, y + cap);
            } else {
                let _ = write!(svg, "M{:.1} {y:.1}L{:.1} {y:.1}", x - cap, x + cap);
            }
        }
    }
    let _ = write!(svg, "\" fill=\"none\" stroke=\"{AXIS}\" stroke-width=\"1\"/>");
}

/// A trendline's path in the plot: Excel's default look, a dotted line 1.5 points wide in the series' colour.
fn trendline_svg(svg: &mut String, series: usize, colour: &str, points: &[(f64, f64)]) {
    let mut d = String::new();
    for (k, (x, y)) in points.iter().enumerate() {
        let _ = write!(d, "{}{x:.1} {y:.1} ", if k == 0 { "M" } else { "L" });
    }
    let _ = write!(
        svg,
        "<path data-dnacalc=\"chart-trendline\" data-series=\"{series}\" d=\"{d}\" fill=\"none\" stroke=\"#{colour}\" stroke-width=\"2\" stroke-dasharray=\"0 4\" stroke-linecap=\"round\"/>"
    );
}

fn mark(svg: &mut String, series: usize, point: usize, value: f64) {
    let _ = write!(
        svg,
        " data-series=\"{series}\" data-point=\"{point}\" data-value=\"{value}\""
    );
}

#[derive(Clone, Copy)]
struct CategoryLayout {
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
    count: usize,
    index: usize,
    offset: usize,
    secondary: bool,
    ticks: bool,
}
fn combination_layouts(chart: &ChartView, area: Area) -> Result<Vec<CategoryLayout>, String> {
    // UI28 midCat answers: with crossBetween absent, an area plot alone crosses at the category
    // edges but an area and a line plot together cross between categories. Three plots are unmeasured.
    if chart.plots.len() > 2
        && chart.plots.iter().any(|p| p.kind == ChartKind::Area)
        && chart.plots.iter().any(|p| {
            calc_host::chart::value_axes(&chart.axes, &p.axis_ids, false)
                .ok()
                .is_some_and(|axes| axes[0].cross_between.is_none())
        })
    {
        return Err("Three-plot category crossing with an area is preserved but needs an Excel measurement before drawing (dc-zjsg)".into());
    }
    let first = calc_host::chart::category_scale(chart, &chart.plots[0])?;
    let width = |s: calc_host::chart::ChartScale, id: u32| {
        axis_label(chart, id, s.minimum, s.step)
            .len()
            .max(axis_label(chart, id, s.maximum, s.step).len()) as f64
            * 6.5
            + 8.
    };
    let mut right = 0.;
    let mut axes = Vec::new();
    for plot in &chart.plots {
        let axis = calc_host::chart::value_axes(&chart.axes, &plot.axis_ids, false)?[0];
        let secondary = axis.position.as_deref() == Some("r");
        if secondary {
            right = width(calc_host::chart::category_scale(chart, plot)?, axis.id);
        }
        axes.push((axis.id, secondary));
    }
    let first_axis = calc_host::chart::value_axes(&chart.axes, &chart.plots[0].axis_ids, false)?[0];
    let x0 = area.left + width(first, first_axis.id);
    let x1 = (area.right - right).max(x0 + 1.);
    let count = chart
        .plots
        .iter()
        .flat_map(|p| &p.series)
        .map(|s| s.values.len())
        .max()
        .unwrap_or(1)
        .max(1);
    let mut seen = std::collections::BTreeSet::new();
    let mut offset = 0;
    let mut layouts = Vec::new();
    for (index, (plot, (id, secondary))) in chart.plots.iter().zip(axes).enumerate() {
        layouts.push(CategoryLayout {
            x0,
            x1,
            y0: area.top,
            y1: area.bottom - 18.,
            count,
            index,
            offset,
            secondary,
            ticks: seen.insert(id),
        });
        offset += plot.series.len();
    }
    Ok(layouts)
}

/// Columns, bars, lines and areas over categories.
fn category_marks(
    svg: &mut String,
    chart: &ChartView,
    plot: &PlotView,
    area: Area,
    layout: Option<CategoryLayout>,
    baseline: Option<usize>,
) {
    let horizontal = plot.kind == ChartKind::Bar;
    let count = layout.map_or_else(
        || {
            plot.series
                .iter()
                .map(|s| s.values.len())
                .max()
                .unwrap_or(0)
                .max(1)
        },
        |l| l.count,
    );
    let offset = layout.map_or(0, |l| l.offset);
    let active_series = &plot.series[..baseline.unwrap_or(plot.series.len())];
    let scale = match calc_host::chart::category_scale(chart, plot) {
        Ok(scale) => scale,
        Err(message) => {
            axis_notice(svg, &area, &message);
            return;
        }
    };
    let (min, max, step) = (scale.minimum, scale.maximum, scale.step);
    let Ok(axes) = calc_host::chart::value_axes(&chart.axes, &plot.axis_ids, false) else {
        return;
    };
    let value_axis = axes[0].id;
    let grid_color = gridline_color(chart, value_axis);
    let Some(category_axis) = chart
        .axes
        .iter()
        .find(|a| plot.axis_ids.contains(&a.id) && a.id != value_axis)
        .map(|a| a.id)
    else {
        return;
    };

    // Room for the value labels and the category labels.
    let widest = [min, max]
        .iter()
        .map(|v| axis_label(chart, value_axis, *v, step).len())
        .max()
        .unwrap_or(1) as f64;
    let (plot_left, plot_bottom) = if horizontal {
        (area.left + 40.0, area.bottom - 16.0)
    } else {
        (area.left + widest * 6.5 + 8.0, area.bottom - 18.0)
    };
    let (x0, x1, y0, y1) = layout.map_or((plot_left, area.right, area.top, plot_bottom), |l| {
        (l.x0, l.x1, l.y0, l.y1)
    });
    // Where a value is along the value axis, and where a category's band starts.
    let value_at = |v: f64| {
        let t = scale.position(v).unwrap_or(0.);
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
    for v in scale.ticks() {
        if layout.is_some_and(|l| !l.ticks) {
            break;
        }
        let p = value_at(v);
        if layout.is_some_and(|l| l.secondary) {
            let _ = write!(
                svg,
                "<text data-dnacalc=\"chart-axis-tick\" data-chart-text=\"axis:{value_axis}\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"start\" fill=\"{AXIS}\">{}</text>",
                x1 + 4.,
                p + 4.,
                escape(&axis_label(chart, value_axis, v, step))
            );
            continue;
        }
        if horizontal {
            let _ = write!(
                svg,
                "<line x1=\"{p:.1}\" y1=\"{y0:.1}\" x2=\"{p:.1}\" y2=\"{y1:.1}\" stroke=\"{grid_color}\"/><text data-dnacalc=\"chart-axis-tick\" data-chart-text=\"axis:{value_axis}\" x=\"{p:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"{AXIS}\">{}</text>",
                y1 + 13.0,
                escape(&axis_label(chart, value_axis, v, step))
            );
        } else {
            let _ = write!(
                svg,
                "<line x1=\"{x0:.1}\" y1=\"{p:.1}\" x2=\"{x1:.1}\" y2=\"{p:.1}\" stroke=\"{grid_color}\"/><text data-dnacalc=\"chart-axis-tick\" data-chart-text=\"axis:{value_axis}\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" fill=\"{AXIS}\">{}</text>",
                x0 - 4.0,
                p + 4.0,
                escape(&axis_label(chart, value_axis, v, step))
            );
        }
    }
    // Where a category's point or label is: the middle of its band, or for an area the band's
    // edge, from the first category's to the last's (Excel's `crossBetween` of each).
    let between = calc_host::chart::value_axes(&chart.axes, &plot.axis_ids, false)
        .ok()
        .and_then(|a| a.first().and_then(|a| a.cross_between.as_deref()));
    if between.is_some_and(|v| !matches!(v, "between" | "midCat")) {
        axis_notice(
            svg,
            &area,
            "The stored category crossing is preserved but not drawn here (dc-zjsg)",
        );
        return;
    }
    if between == Some("midCat") && matches!(plot.kind, ChartKind::Column | ChartKind::Bar) {
        axis_notice(
            svg,
            &area,
            "Midpoint category crossing for columns and bars is preserved but not drawn here (dc-zjsg)",
        );
        return;
    }
    let at_edges = count > 1
        && between.map_or(
            plot.kind == ChartKind::Area && chart.plots.len() == 1,
            |v| v == "midCat",
        );
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
        if layout.is_some_and(|l| l.index > 0) {
            break;
        }
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
                "<text data-chart-text=\"axis:{category_axis}\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" fill=\"{AXIS}\">{}</text>",
                x0 - 4.0,
                middle + 4.0,
                escape(&text)
            );
        } else {
            let _ = write!(
                svg,
                "<text data-chart-text=\"axis:{category_axis}\" x=\"{middle:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"{AXIS}\">{}</text>",
                y1 + 13.0,
                escape(&text)
            );
        }
    }
    let _ = write!(
        svg,
        "<svg data-dnacalc=\"chart-plot\" data-axis-min=\"{min}\" data-axis-max=\"{max}\" data-axis-unit=\"{step}\" data-axis-reversed=\"{}\" x=\"{x0}\" y=\"{y0}\" width=\"{}\" height=\"{}\" viewBox=\"{x0} {y0} {} {}\" overflow=\"hidden\">",
        scale.reversed,
        x1 - x0,
        y1 - y0,
        x1 - x0,
        y1 - y0
    );
    let zero = value_at(0.0_f64.clamp(min, max));
    let mut data_labels = String::new();
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
                    if scale.position(v).is_none() {
                        continue;
                    }
                    let (from, to) = if plot.stacked {
                        let base = if v >= 0.0 { &mut up[j] } else { &mut down[j] };
                        let from = *base;
                        *base += v;
                        (value_at(from), value_at(*base))
                    } else {
                        (zero, value_at(v))
                    };
                    let bar_offset = band_start(j)
                        + (band - group) / 2.0
                        + if plot.stacked { 0.0 } else { i as f64 * width };
                    let (x, y, w, h) = if horizontal {
                        (from.min(to), bar_offset, (to - from).abs(), width)
                    } else {
                        (bar_offset, from.min(to), width, (to - from).abs())
                    };
                    let _ = write!(
                        svg,
                        "<rect data-dnacalc=\"chart-mark\" x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" fill=\"#{}\"",
                        series.color
                    );
                    mark(svg, i + offset, j, v);
                    svg.push_str("/>");
                    let composed = if series.value_labels.is_empty() { label_text(series, j, v, None, &series.name) } else { None };
                    if let Some(label) = composed.as_ref().or(series.value_labels.get(j).and_then(Option::as_ref)) {
                        if horizontal {
                            let (x, anchor) = if v >= 0. { (to - 4., "end") } else { (to + 4., "start") };
                            let _ = write!(data_labels, "<text data-dnacalc=\"chart-value-label\" data-series=\"{}\" data-point=\"{j}\" x=\"{x:.1}\" y=\"{:.1}\" text-anchor=\"{anchor}\" fill=\"{AXIS}\">{}</text>", i + offset, y + h / 2. + 4., escape(label));
                        } else {
                            // A column's label sits past its end (outEnd) unless the file puts it inside (Excel's default,
                            // dw2 Windows item 8), at the middle or at the base.
                            let y = match label_position(series, j).or(if plot.stacked { Some("ctr") } else { None }) {
                                Some("inEnd") => if v >= 0. { to + 12. } else { to - 4. },
                                Some("ctr") => (to + from) / 2. + 4.,
                                Some("inBase") => if v >= 0. { from - 4. } else { from + 12. },
                                _ => if v >= 0. { to - 4. } else { to + 12. },
                            };
                            let _ = write!(data_labels, "<text data-dnacalc=\"chart-value-label\" data-series=\"{}\" data-point=\"{j}\" x=\"{:.1}\" y=\"{y:.1}\" text-anchor=\"middle\" fill=\"{AXIS}\">{}</text>", i + offset, x + w / 2., escape(label));
                        }
                    }
                }
            }
        }
        ChartKind::Line | ChartKind::Area => {
            let mut base = vec![0.0f64; count];
            let x_at = middle_of;
            let mut shapes = String::new();
            for (i, series) in plot.series.iter().enumerate() {
                let i = i + offset;
                let points: Vec<(usize, f64)> = series
                    .values
                    .iter()
                    .enumerate()
                    .filter_map(|(j, v)| v.filter(|v| scale.position(*v).is_some()).map(|v| (j, v)))
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
                        "<path d=\"{d}\" fill=\"none\" stroke=\"#{}\" stroke-width=\"2.25\" stroke-linejoin=\"round\" data-series=\"{i}\"{stroke}/>",
                        series.color,
                        stroke = if baseline.is_some_and(|start| i >= start) {
                            " data-dnacalc=\"chart-baseline\" stroke-dasharray=\"1 5\" stroke-linecap=\"round\""
                        } else { "" },
                    );
                }
                if baseline.is_some_and(|start| i >= start) {
                    continue;
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
                    let composed = if series.value_labels.is_empty() { label_text(series, j, v, None, &series.name) } else { None };
                    if plot.kind == ChartKind::Line && let Some(label) = composed.as_ref().or(series.value_labels.get(j).and_then(Option::as_ref)) {
                        let _ = write!(data_labels, "<text data-dnacalc=\"chart-value-label\" data-series=\"{i}\" data-point=\"{j}\" x=\"{x:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"{AXIS}\">{}</text>", y - 4., escape(label));
                    }
                    if plot.stacked {
                        base[j] += v;
                    }
                }
            }
            svg.push_str(&shapes);
        }
        ChartKind::Stock => {
            // Each category's high-low line from its lowest to its highest value; the open (with four series, the
            // first) as a tick to the left, the close (the last) as a tick to the right; with up/down bars, a box
            // from open to close, white when the close is higher, grey (89, 89, 89: Excel's) when lower.
            let n = plot.series.len();
            let tick = (band / 6.0).clamp(2.0, 8.0);
            for j in 0..count {
                let values: Vec<f64> = plot.series.iter().filter_map(|s| s.values.get(j).copied().flatten()).filter(|v| scale.position(*v).is_some()).collect();
                let x = middle_of(j);
                if values.len() >= 2 && (plot.high_low_lines || !plot.up_down_bars) {
                    let (low, high) = values.iter().fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
                    let _ = write!(
                        svg,
                        "<line data-dnacalc=\"chart-high-low\" data-point=\"{j}\" x1=\"{x:.1}\" y1=\"{:.1}\" x2=\"{x:.1}\" y2=\"{:.1}\" stroke=\"{AXIS}\" stroke-width=\"0.75\"/>",
                        value_at(low),
                        value_at(high)
                    );
                }
                let at = |i: usize| plot.series.get(i).and_then(|s| s.values.get(j).copied().flatten()).filter(|v| scale.position(*v).is_some());
                let open = if n >= 4 { at(0) } else { None };
                let close = if n >= 3 { at(n - 1) } else { None };
                match (open, close) {
                    (Some(o), Some(c)) if plot.up_down_bars => {
                        let (top, bottom) = (value_at(o.max(c)), value_at(o.min(c)));
                        let fill = if c >= o { "#FFFFFF" } else { "#595959" };
                        let _ = write!(
                            svg,
                            "<rect data-dnacalc=\"chart-mark\" x=\"{:.1}\" y=\"{top:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{fill}\" stroke=\"{AXIS}\" stroke-width=\"0.75\"",
                            x - tick,
                            2.0 * tick,
                            (bottom - top).max(0.75)
                        );
                        mark(svg, n - 1 + offset, j, c);
                        svg.push_str("/>");
                    }
                    _ => {
                        for (value, left, series) in [(open, true, 0), (close, false, n.saturating_sub(1))] {
                            let Some(v) = value else { continue };
                            let y = value_at(v);
                            // Excel's close mark of a high-low-close is a tiny tick, 3 px (dw2 Windows item 5); an open tick
                            // to the left keeps the band's share.
                            let (from, to) = if left { (x - tick, x) } else { (x, x + 3.0) };
                            let _ = write!(
                                svg,
                                "<line data-dnacalc=\"chart-mark\" x1=\"{from:.1}\" y1=\"{y:.1}\" x2=\"{to:.1}\" y2=\"{y:.1}\" stroke=\"#{}\" stroke-width=\"1.5\"",
                                plot.series[series].color
                            );
                            mark(svg, series + offset, j, v);
                            svg.push_str("/>");
                        }
                    }
                }
            }
        }
        _ => {}
    }
    // A line plot's high-low lines from each category's lowest to its highest value, a line or area plot's drop
    // lines from each point down to the category axis, and error bars (dc-zjsg.6). Error bars on columns stand at
    // each column's middle; a stacked plot's are not drawn.
    if matches!(plot.kind, ChartKind::Line | ChartKind::Area) && !horizontal {
        for j in 0..count {
            let values: Vec<f64> = active_series.iter().filter_map(|s| s.values.get(j).copied().flatten()).filter(|v| scale.position(*v).is_some()).collect();
            let x = middle_of(j);
            if plot.high_low_lines && plot.kind == ChartKind::Line && values.len() >= 2 {
                let (low, high) = values.iter().fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
                let _ = write!(svg, "<line data-dnacalc=\"chart-high-low\" data-point=\"{j}\" x1=\"{x:.1}\" y1=\"{:.1}\" x2=\"{x:.1}\" y2=\"{:.1}\" stroke=\"{AXIS}\" stroke-width=\"0.75\"/>", value_at(low), value_at(high));
            }
            if plot.drop_lines {
                for v in &values {
                    let _ = write!(svg, "<line data-dnacalc=\"chart-drop-line\" data-point=\"{j}\" x1=\"{x:.1}\" y1=\"{:.1}\" x2=\"{x:.1}\" y2=\"{zero:.1}\" stroke=\"{AXIS}\" stroke-width=\"0.75\"/>", value_at(*v));
                }
            }
        }
    }
    if !plot.stacked && matches!(plot.kind, ChartKind::Column | ChartKind::Bar | ChartKind::Line | ChartKind::Area) {
        let group = band / 2.5;
        let width = group / active_series.len().max(1) as f64;
        for (i, series) in active_series.iter().enumerate() {
            for bars in series.error_bars.iter().filter(|b| b.direction.as_deref() != Some("x")) {
                for (j, extent) in error_extents(bars, &series.values).into_iter().enumerate() {
                    let Some((low, high, _)) = extent else { continue };
                    let along = if matches!(plot.kind, ChartKind::Column | ChartKind::Bar) {
                        band_start(j) + (band - group) / 2.0 + i as f64 * width + width / 2.0
                    } else {
                        middle_of(j)
                    };
                    let cap = bars.end_cap.then_some(4.0);
                    if horizontal {
                        error_bar_svg(svg, i + offset, j, (value_at(low), along), (value_at(high), along), cap);
                    } else {
                        error_bar_svg(svg, i + offset, j, (along, value_at(low)), (along, value_at(high)), cap);
                    }
                }
            }
        }
    }
    // Trendlines over the categories, numbered from 1 as Excel fits them (dc-zjsg.6).
    if matches!(plot.kind, ChartKind::Column | ChartKind::Bar | ChartKind::Line | ChartKind::Area) && count > 0 {
        let step = if count > 1 { middle_of(1) - middle_of(0) } else { 0.0 };
        let at_x = |x: f64| middle_of(0) + (x - 1.0) * step;
        for (i, series) in active_series.iter().enumerate() {
            let xy: Vec<(f64, f64)> = series.values.iter().enumerate().filter_map(|(j, v)| v.map(|v| ((j + 1) as f64, v))).collect();
            for (t, colour) in &series.trendlines {
                let Some(points) = trendline_points(t, &xy, 1.0, count as f64) else { continue };
                let drawn: Vec<(f64, f64)> = points
                    .into_iter()
                    .map(|(x, y)| if horizontal { (value_at(y), y1 - (at_x(x) - x0)) } else { (at_x(x), value_at(y)) })
                    .collect();
                trendline_svg(svg, i + offset, colour, &drawn);
            }
        }
    }
    // The category axis line, at zero.
    let axis_color = chart.axes.iter().find(|axis| axis.id == category_axis)
        .and_then(|axis| axis.line_color.as_deref())
        .filter(|color| color.len() == 6 && color.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(|color| format!("#{color}"))
        .unwrap_or_else(|| AXIS.to_owned());
    if layout.is_none_or(|l| l.index == 0)
        && !chart.axes.iter().any(|axis| axis.id == category_axis && axis.line_hidden)
    {
        if horizontal {
            let _ = write!(
                svg,
                "<line x1=\"{zero:.1}\" y1=\"{y0:.1}\" x2=\"{zero:.1}\" y2=\"{y1:.1}\" stroke=\"{axis_color}\"/>"
            );
        } else {
            let _ = write!(
                svg,
                "<line x1=\"{x0:.1}\" y1=\"{zero:.1}\" x2=\"{x1:.1}\" y2=\"{zero:.1}\" stroke=\"{axis_color}\"/>"
            );
        }
    }
    svg.push_str("</svg>");
    svg.push_str(&data_labels);
}

/// An Excel 2016 histogram (dc-zjsg.6): the first series' values counted into bins ([`calc_host::chart::histogram_bins`])
/// and drawn as columns with a small gap, each labelled with its interval; a Pareto sorts its columns from the
/// highest and adds the cumulative share as a line over a 0 to 100 % axis on the right. With categories and no
/// binning the values are counted by category. Gap, colours and the label layout ASSUMED until a side-by-side.
fn histogram_marks(svg: &mut String, chart: &ChartView, plot: &PlotView, area: Area) {
    let Some(series) = plot.series.first() else { return };
    let categories: Vec<String> = series.categories.iter().map(|c| c.clone().map(|v| text_of(&v)).unwrap_or_default()).collect();
    let mut bars: Vec<(String, usize)> = if !categories.is_empty() && plot.binning.is_none() {
        let mut groups: Vec<(String, usize)> = Vec::new();
        for c in &categories {
            match groups.iter_mut().find(|g| &g.0 == c) {
                Some(g) => g.1 += 1,
                None => groups.push((c.clone(), 1)),
            }
        }
        groups
    } else {
        let values: Vec<f64> = series.values.iter().flatten().copied().collect();
        let interval = |b: &calc_host::chart::Bin, first: bool| {
            let left = plot.binning.as_ref().is_some_and(|b| b.closed_left);
            let num = |v: f64| label(v, 0.0);
            if b.underflow {
                format!("{}{}", if left { "<" } else { "≤" }, num(b.high))
            } else if b.overflow {
                format!("{}{}", if left { "≥" } else { ">" }, num(b.low))
            } else if left {
                format!("[{}, {})", num(b.low), num(b.high))
            } else {
                format!("{}{}, {}]", if first { "[" } else { "(" }, num(b.low), num(b.high))
            }
        };
        let bins = calc_host::chart::histogram_bins(&values, plot.binning.as_ref());
        let first_regular = bins.iter().position(|b| !b.underflow).unwrap_or(0);
        bins.iter().enumerate().map(|(i, b)| (interval(b, i == first_regular), b.count)).collect()
    };
    if bars.is_empty() {
        return;
    }
    if plot.pareto {
        bars.sort_by(|a, b| b.1.cmp(&a.1));
    }
    let total: usize = bars.iter().map(|b| b.1).sum();
    let high = bars.iter().map(|b| b.1).max().unwrap_or(1) as f64;
    let Ok(scale) = calc_host::chart::chart_scale(0.0, high, None) else { return };
    let right = if plot.pareto { 36.0 } else { 0.0 };
    let (x0, x1, y0, y1) = (area.left + 40.0, area.right - right, area.top, area.bottom - 18.0);
    let y_of = |v: f64| y1 - scale.position(v).unwrap_or(0.0) * (y1 - y0);
    for v in scale.ticks() {
        let y = y_of(v);
        let _ = write!(svg, "<line x1=\"{x0:.1}\" y1=\"{y:.1}\" x2=\"{x1:.1}\" y2=\"{y:.1}\" stroke=\"{GRID}\"/><text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" fill=\"{AXIS}\">{}</text>", x0 - 4.0, y + 4.0, escape(&label(v, scale.step)));
    }
    let band = (x1 - x0) / bars.len() as f64;
    // Excel's own histograms/Pareto write gapWidth="0" (touching bars with a 1 px white border, dw3 Windows item 6);
    // otherwise bar width is band / (1 + gapWidth).
    let gap = plot.gap_width.unwrap_or(0.0).max(0.0);
    let bar = band / (1.0 + gap);
    let border = if gap <= 1e-6 { " stroke=\"#FFFFFF\" stroke-width=\"1\"" } else { "" };
    let mut running = 0usize;
    let mut line = String::new();
    for (j, (text, count)) in bars.iter().enumerate() {
        let x = x0 + j as f64 * band;
        let (top, bottom) = (y_of(*count as f64), y_of(0.0));
        let _ = write!(
            svg,
            "<rect data-dnacalc=\"chart-mark\" x=\"{:.1}\" y=\"{top:.1}\" width=\"{bar:.1}\" height=\"{:.1}\" fill=\"#{}\"{border}",
            x + (band - bar) / 2.0,
            (bottom - top).max(0.0),
            series.color
        );
        mark(svg, 0, j, *count as f64);
        svg.push_str("/>");
        let _ = write!(svg, "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"{AXIS}\">{}</text>", x + band / 2.0, y1 + 13.0, escape(text));
        running += count;
        if plot.pareto && total > 0 {
            let share = running as f64 / total as f64;
            let _ = write!(line, "{}{:.1} {:.1} ", if j == 0 { "M" } else { "L" }, x + band / 2.0, y1 - share * (y1 - y0));
        }
    }
    if plot.pareto && total > 0 {
        let _ = write!(svg, "<path data-dnacalc=\"chart-pareto\" d=\"{}\" fill=\"none\" stroke=\"#{}\" stroke-width=\"2\"/>", line.trim_end(), chart.palette[1 % chart.palette.len()]);
        for pct in [0, 20, 40, 60, 80, 100] {
            let _ = write!(svg, "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"{AXIS}\">{pct}%</text>", x1 + 4.0, y1 - pct as f64 / 100.0 * (y1 - y0) + 4.0);
        }
    }
    let _ = write!(svg, "<line x1=\"{x0:.1}\" y1=\"{y1:.1}\" x2=\"{x1:.1}\" y2=\"{y1:.1}\" stroke=\"{AXIS}\"/>");
}

/// A surface (dc-zjsg.6): the series as rows of a grid and the categories its columns, drawn in an oblique projection
/// (a series further back up and to the right), each cell a quadrilateral over the four heights in the colour of
/// the value band (the value axis's ticks) its mean falls in; a wireframe leaves the cells unfilled. The projection
/// and the band colours are ASSUMED until an Excel side-by-side.
fn surface_marks(svg: &mut String, chart: &ChartView, plot: &PlotView, area: Area) {
    let rows = plot.series.len();
    let columns = plot.series.iter().map(|s| s.values.len()).max().unwrap_or(0);
    if rows < 2 || columns < 2 {
        axis_notice(svg, &area, "A surface needs at least two series of two points");
        return;
    }
    let z = |r: usize, c: usize| plot.series[r].values.get(c).copied().flatten().filter(|v| v.is_finite());
    let (low, high) = plot.series.iter().flat_map(|s| s.values.iter().flatten().copied()).fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)));
    let Ok(scale) = calc_host::chart::chart_scale(low.min(0.0), high, None) else { return };
    let (width, height) = (area.right - area.left - 50.0, area.bottom - area.top - 30.0);
    let (depth_x, depth_y) = (width * 0.25, height * 0.25);
    let (plot_w, plot_h) = (width - depth_x, height - depth_y);
    let at = |r: usize, c: usize, v: f64| {
        let share = scale.position(v).unwrap_or(0.0);
        let back = r as f64 / (rows - 1) as f64;
        (
            area.left + 40.0 + c as f64 / (columns - 1) as f64 * plot_w + back * depth_x,
            area.bottom - 20.0 - share * plot_h - back * depth_y,
        )
    };
    let ticks = scale.ticks();
    let band = |v: f64| ticks.iter().rposition(|t| *t <= v).unwrap_or(0);
    for r in (0..rows - 1).rev() {
        for c in 0..columns - 1 {
            let corners = [(r, c), (r, c + 1), (r + 1, c + 1), (r + 1, c)];
            let heights: Vec<Option<f64>> = corners.iter().map(|(rr, cc)| z(*rr, *cc)).collect();
            if heights.iter().any(Option::is_none) {
                continue;
            }
            let heights: Vec<f64> = heights.into_iter().flatten().collect();
            let mean = heights.iter().sum::<f64>() / 4.0;
            let d: String = corners
                .iter()
                .zip(&heights)
                .enumerate()
                .map(|(k, ((rr, cc), v))| {
                    let (x, y) = at(*rr, *cc, *v);
                    format!("{}{x:.1} {y:.1}", if k == 0 { "M" } else { "L" })
                })
                .collect::<Vec<_>>()
                .join(" ");
            let fill = if plot.wireframe { "none".to_owned() } else { format!("#{}", chart.palette[band(mean) % chart.palette.len()]) };
            let _ = write!(svg, "<path data-dnacalc=\"chart-mark\" d=\"{d}Z\" fill=\"{fill}\" stroke=\"{AXIS}\" stroke-width=\"0.5\"");
            mark(svg, r, c, mean);
            svg.push_str("/>");
        }
    }
    for v in ticks {
        let (x, y) = at(0, 0, v);
        let _ = write!(svg, "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" fill=\"{AXIS}\">{}</text>", x - 4.0, y + 4.0, escape(&label(v, scale.step)));
    }
}

/// An Excel 2016 treemap (dc-zjsg.6): the series' hierarchy as squarified rectangles, a parent's rectangle holding its
/// children below a name strip, each top-level branch in its own palette colour (a leaf lighter the deeper it is).
/// Spacing and the strip's height ASSUMED until a side-by-side.
fn treemap_marks(svg: &mut String, chart: &ChartView, plot: &PlotView, area: Area) {
    use crate::hierarchy::{Rect, Tile, hierarchy, squarify};
    let Some(series) = plot.series.first() else { return };
    let roots = hierarchy(series);
    #[allow(clippy::too_many_arguments)]
    fn place(svg: &mut String, chart: &ChartView, tiles: &[Tile], rect: Rect, depth: usize, root: Option<usize>) {
        let mut order: Vec<(usize, &Tile)> = tiles.iter().enumerate().collect();
        order.sort_by(|a, b| tile_total(b.1).total_cmp(&tile_total(a.1)));
        let refs: Vec<&Tile> = order.iter().map(|(_, t)| *t).collect();
        for ((i, tile), r) in order.iter().zip(squarify(&refs, rect)) {
            let top = root.unwrap_or(*i);
            let colour = &chart.palette[top % chart.palette.len()];
            let (x, y, w, h) = (r.0 + 0.5, r.1 + 0.5, (r.2 - 1.0).max(0.0), (r.3 - 1.0).max(0.0));
            let fade = (depth as f64 * 0.18).min(0.6);
            match tile.point {
                Some(j) => {
                    let _ = write!(svg, "<rect data-dnacalc=\"chart-mark\" x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" fill=\"#{colour}\" fill-opacity=\"{:.2}\" stroke=\"#FFFFFF\" stroke-width=\"2\"", 1.0 - fade);
                    mark(svg, 0, j, tile.value);
                    svg.push_str("/>");
                    if w > 28.0 && h > 14.0 {
                        let _ = write!(svg, "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"#FFFFFF\">{}</text>", x + 3.0, y + 12.0, escape(&tile.name));
                    }
                }
                None => {
                    let strip = if h > 40.0 { 16.0 } else { 0.0 };
                    let _ = write!(svg, "<rect data-dnacalc=\"chart-group\" x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" fill=\"#{colour}\" fill-opacity=\"0.25\" stroke=\"#{colour}\"/>");
                    if strip > 0.0 {
                        let _ = write!(svg, "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"{AXIS}\">{}</text>", x + 3.0, y + 12.0, escape(&tile.name));
                    }
                    place(svg, chart, &tile.children, (x + 2.0, y + strip, (w - 4.0).max(0.0), (h - strip - 2.0).max(0.0)), depth + 1, Some(top));
                }
            }
        }
    }
    fn tile_total(t: &Tile) -> f64 {
        if t.children.is_empty() { t.value } else { t.children.iter().map(tile_total).sum() }
    }
    place(svg, chart, &roots, (area.left, area.top, area.right - area.left, area.bottom - area.top), 0, None);
}

/// An Excel 2016 sunburst (dc-zjsg.6): the series' hierarchy as rings of arcs from twelve o'clock clockwise, the
/// innermost ring the top level, each arc in its top-level branch's palette colour. Equal ring bands with the centre
/// hole counting as one band and 2 px white outlines (Excel-measured: dw3 Windows item 7).
fn sunburst_marks(svg: &mut String, chart: &ChartView, plot: &PlotView, area: Area) {
    use crate::hierarchy::{arcs, hierarchy};
    let Some(series) = plot.series.first() else { return };
    let roots = hierarchy(series);
    let all = arcs(&roots);
    let rings = all.iter().map(|a| a.ring + 1).max().unwrap_or(0);
    if rings == 0 {
        return;
    }
    let (cx, cy) = ((area.left + area.right) / 2.0, (area.top + area.bottom) / 2.0);
    let radius = ((area.right - area.left).min(area.bottom - area.top) / 2.0 - 4.0).max(10.0);
    // Excel (dw3 Windows item 7): centre hole counts as one equal band beside the hierarchy rings.
    let step = radius / (rings + 1) as f64;
    let point = |r: f64, turn: f64| {
        let angle = turn * std::f64::consts::TAU;
        (cx + r * angle.sin(), cy - r * angle.cos())
    };
    for arc in &all {
        let (inner, outer) = ((arc.ring + 1) as f64 * step, (arc.ring + 2) as f64 * step);
        let colour = &chart.palette[arc.root % chart.palette.len()];
        let large = i32::from(arc.to - arc.from > 0.5);
        let full = arc.to - arc.from >= 0.9999;
        let to = if full { arc.from + 0.9999 } else { arc.to };
        let ((x0, y0), (x1, y1), (x2, y2), (x3, y3)) = (point(outer, arc.from), point(outer, to), point(inner, to), point(inner, arc.from));
        let _ = write!(
            svg,
            "<path data-dnacalc=\"chart-mark\" d=\"M{x0:.1} {y0:.1}A{outer:.1} {outer:.1} 0 {large} 1 {x1:.1} {y1:.1}L{x2:.1} {y2:.1}A{inner:.1} {inner:.1} 0 {large} 0 {x3:.1} {y3:.1}Z\" fill=\"#{colour}\" fill-opacity=\"{:.2}\" stroke=\"#FFFFFF\" stroke-width=\"2\"",
            1.0 - (arc.ring as f64 * 0.15).min(0.5)
        );
        match arc.tile.point {
            Some(j) => mark(svg, 0, j, arc.tile.value),
            None => svg.push_str(" data-group=\"1\""),
        }
        svg.push_str("/>");
        if (arc.to - arc.from) * outer > 24.0 {
            let (lx, ly) = point((inner + outer) / 2.0, (arc.from + arc.to) / 2.0);
            let _ = write!(svg, "<text x=\"{lx:.1}\" y=\"{ly:.1}\" text-anchor=\"middle\" fill=\"#FFFFFF\">{}</text>", escape(&arc.tile.name));
        }
    }
}

/// An Excel 2016 waterfall (dc-zjsg.6): over its categories each point a column from the running total to the total
/// after it (up in accent 1, down in accent 2), a subtotal point a column from zero to its value in accent 3 that
/// sets the running total; the value axis spans zero and every total. Gap and colours as Excel's defaults (ASSUMED
/// until a side-by-side); no connector lines.
fn waterfall_marks(svg: &mut String, chart: &ChartView, plot: &PlotView, area: Area) {
    let Some(series) = plot.series.first() else { return };
    let count = series.values.len();
    if count == 0 {
        return;
    }
    let mut spans = Vec::with_capacity(count);
    let mut total = 0.0f64;
    for (j, v) in series.values.iter().enumerate() {
        let Some(v) = v.filter(|v| v.is_finite()) else {
            spans.push(None);
            continue;
        };
        if plot.subtotals.contains(&(j as u32)) {
            spans.push(Some((0.0, v, 2usize)));
            total = v;
        } else {
            spans.push(Some((total, total + v, usize::from(v < 0.0))));
            total += v;
        }
    }
    let (low, high) = spans.iter().flatten().fold((0.0f64, 0.0f64), |(a, b), (f, t, _)| (a.min(f.min(*t)), b.max(f.max(*t))));
    let Ok(scale) = calc_host::chart::chart_scale(low, high, None) else { return };
    let (x0, x1, y0, y1) = (area.left + 40.0, area.right, area.top, area.bottom - 18.0);
    let y_of = |v: f64| y1 - scale.position(v).unwrap_or(0.0) * (y1 - y0);
    for v in scale.ticks() {
        let y = y_of(v);
        let _ = write!(svg, "<line x1=\"{x0:.1}\" y1=\"{y:.1}\" x2=\"{x1:.1}\" y2=\"{y:.1}\" stroke=\"{GRID}\"/><text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" fill=\"{AXIS}\">{}</text>", x0 - 4.0, y + 4.0, escape(&label(v, scale.step)));
    }
    let band = (x1 - x0) / count as f64;
    // Excel (dw2 Windows item 8): a bar is the band over one plus the gap width (the file's, 0.5 without), and thin light-grey
    // connector lines run from each bar's end to the next bar.
    let bar = band / (1.0 + plot.gap_width.unwrap_or(0.5));
    let colours = [&chart.palette[0], &chart.palette[1 % chart.palette.len()], &chart.palette[2 % chart.palette.len()]];
    for (j, span) in spans.iter().enumerate() {
        let x = x0 + j as f64 * band;
        if let Some((from, to, kind)) = span {
            let (top, bottom) = (y_of(from.max(*to)), y_of(from.min(*to)));
            let _ = write!(
                svg,
                "<rect data-dnacalc=\"chart-mark\" data-kind=\"{}\" x=\"{:.1}\" y=\"{top:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"#{}\"",
                ["increase", "decrease", "total"][*kind],
                x + (band - bar) / 2.0,
                bar,
                (bottom - top).max(0.5),
                colours[*kind]
            );
            mark(svg, 0, j, to - if *kind == 2 { 0.0 } else { *from });
            svg.push_str("/>");
            if let Some(Some(_)) = spans.get(j + 1) {
                let y = y_of(*to);
                let _ = write!(svg, "<line data-dnacalc=\"chart-connector\" x1=\"{:.1}\" y1=\"{y:.1}\" x2=\"{:.1}\" y2=\"{y:.1}\" stroke=\"#A6A6A6\" stroke-width=\"0.75\"/>", x + (band + bar) / 2.0, x + band + (band - bar) / 2.0);
            }
        }
        let text = series.categories.get(j).cloned().flatten().map(|v| text_of(&v)).unwrap_or_else(|| (j + 1).to_string());
        let _ = write!(svg, "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"{AXIS}\">{}</text>", x + band / 2.0, y1 + 13.0, escape(&text));
    }
    let zero = y_of(0.0);
    let _ = write!(svg, "<line x1=\"{x0:.1}\" y1=\"{zero:.1}\" x2=\"{x1:.1}\" y2=\"{zero:.1}\" stroke=\"{AXIS}\"/>");
}

/// The `p`th quantile (0 to 1) of sorted `values` as Excel's box and whisker takes its quartiles: exclusive of the
/// median (QUARTILE.EXC's rank (n + 1) p) or inclusive (QUARTILE.INC's rank (n - 1) p + 1), interpolated between
/// neighbours and held to the ends.
fn quantile(values: &[f64], p: f64, inclusive: bool) -> f64 {
    let n = values.len();
    if n == 1 {
        return values[0];
    }
    let rank = if inclusive { (n as f64 - 1.0) * p + 1.0 } else { (n as f64 + 1.0) * p };
    let rank = rank.clamp(1.0, n as f64);
    let (low, frac) = (rank.floor() as usize, rank.fract());
    let a = values[low - 1];
    let b = values[low.min(n - 1)];
    a + (b - a) * frac
}

/// One box of a box and whisker: its quartiles, its whiskers' ends (the last values within 1.5 times the
/// interquartile range of the box), its mean and the values beyond the whiskers.
#[derive(Debug, PartialEq)]
struct BoxStats {
    q1: f64,
    median: f64,
    q3: f64,
    low: f64,
    high: f64,
    mean: f64,
    outliers: Vec<f64>,
}

fn box_stats(values: &[f64], inclusive: bool) -> Option<BoxStats> {
    let mut v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let (q1, median, q3) = (quantile(&v, 0.25, inclusive), quantile(&v, 0.5, true), quantile(&v, 0.75, inclusive));
    let reach = 1.5 * (q3 - q1);
    let inside: Vec<f64> = v.iter().copied().filter(|x| *x >= q1 - reach && *x <= q3 + reach).collect();
    Some(BoxStats {
        q1,
        median,
        q3,
        low: inside.first().copied().unwrap_or(q1).min(q1),
        high: inside.last().copied().unwrap_or(q3).max(q3),
        mean: v.iter().sum::<f64>() / v.len() as f64,
        outliers: v.iter().copied().filter(|x| *x < q1 - reach || *x > q3 + reach).collect(),
    })
}

/// An Excel 2016 box and whisker (dc-zjsg.6): the values grouped by their category (in the order categories first
/// appear; one group when there are none), a box a group and series from the first to the third quartile with the
/// median across it, whiskers to the last values within 1.5 interquartile ranges, outliers as circles and the mean
/// as an x. Box width and the marks' sizes ASSUMED until a side-by-side.
fn box_marks(svg: &mut String, chart: &ChartView, plot: &PlotView, area: Area) {
    let Some(first) = plot.series.first() else { return };
    let mut groups: Vec<String> = Vec::new();
    for c in first.categories.iter() {
        let name = c.clone().map(|v| text_of(&v)).unwrap_or_default();
        if !groups.contains(&name) {
            groups.push(name);
        }
    }
    if groups.is_empty() {
        groups.push(String::new());
    }
    let stats: Vec<Vec<Option<BoxStats>>> = plot
        .series
        .iter()
        .map(|s| {
            groups
                .iter()
                .map(|g| {
                    let values: Vec<f64> = s
                        .values
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| first.categories.is_empty() || first.categories.get(*j).cloned().flatten().map(|v| text_of(&v)).unwrap_or_default() == *g)
                        .filter_map(|(_, v)| *v)
                        .collect();
                    box_stats(&values, plot.box_inclusive)
                })
                .collect()
        })
        .collect();
    let all = stats.iter().flatten().flatten().flat_map(|b| [b.low, b.high].into_iter().chain(b.outliers.iter().copied()));
    let (low, high) = all.fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(v), b.max(v)));
    if low > high {
        return;
    }
    let Ok(scale) = calc_host::chart::chart_scale(low.min(0.0), high, None) else { return };
    let (x0, x1, y0, y1) = (area.left + 40.0, area.right, area.top, area.bottom - 18.0);
    let y_of = |v: f64| y1 - scale.position(v).unwrap_or(0.0) * (y1 - y0);
    for v in scale.ticks() {
        let y = y_of(v);
        let _ = write!(svg, "<line x1=\"{x0:.1}\" y1=\"{y:.1}\" x2=\"{x1:.1}\" y2=\"{y:.1}\" stroke=\"{GRID}\"/><text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" fill=\"{AXIS}\">{}</text>", x0 - 4.0, y + 4.0, escape(&label(v, scale.step)));
    }
    let band = (x1 - x0) / groups.len() as f64;
    let width = band * 0.6 / plot.series.len().max(1) as f64;
    let [mean_marker, _, outliers, _] = plot.box_visibility;
    for (g, name) in groups.iter().enumerate() {
        let _ = write!(svg, "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"{AXIS}\">{}</text>", x0 + band * (g as f64 + 0.5), y1 + 13.0, escape(name));
        for (i, series) in plot.series.iter().enumerate() {
            let Some(b) = &stats[i][g] else { continue };
            let left = x0 + band * g as f64 + band * 0.2 + i as f64 * width;
            let middle = left + width / 2.0;
            let colour = &series.color;
            let _ = write!(
                svg,
                "<path data-dnacalc=\"chart-whisker\" d=\"M{middle:.1} {:.1}V{:.1}M{middle:.1} {:.1}V{:.1}M{:.1} {:.1}H{:.1}M{:.1} {:.1}H{:.1}\" stroke=\"{AXIS}\" fill=\"none\"/>",
                y_of(b.q3), y_of(b.high), y_of(b.q1), y_of(b.low),
                middle - width / 4.0, y_of(b.high), middle + width / 4.0,
                middle - width / 4.0, y_of(b.low), middle + width / 4.0
            );
            let _ = write!(
                svg,
                "<rect data-dnacalc=\"chart-mark\" data-series=\"{i}\" data-point=\"{g}\" data-q1=\"{}\" data-median=\"{}\" data-q3=\"{}\" x=\"{left:.1}\" y=\"{:.1}\" width=\"{width:.1}\" height=\"{:.1}\" fill=\"#{colour}\" stroke=\"{AXIS}\"/>",
                b.q1, b.median, b.q3,
                y_of(b.q3),
                (y_of(b.q1) - y_of(b.q3)).max(0.5)
            );
            let _ = write!(svg, "<line x1=\"{left:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{AXIS}\" stroke-width=\"1.5\"/>", y_of(b.median), left + width, y_of(b.median));
            if mean_marker {
                let (x, y) = (middle, y_of(b.mean));
                let _ = write!(svg, "<path data-dnacalc=\"chart-mean\" d=\"M{:.1} {:.1}L{:.1} {:.1}M{:.1} {:.1}L{:.1} {:.1}\" stroke=\"{AXIS}\"/>", x - 3.0, y - 3.0, x + 3.0, y + 3.0, x - 3.0, y + 3.0, x + 3.0, y - 3.0);
            }
            if outliers {
                for v in &b.outliers {
                    let _ = write!(svg, "<circle data-dnacalc=\"chart-outlier\" cx=\"{middle:.1}\" cy=\"{:.1}\" r=\"2.5\" fill=\"none\" stroke=\"#{colour}\"/>", y_of(*v));
                }
            }
        }
    }
    let _ = chart;
}

/// An Excel 2016 funnel (dc-zjsg.6, dc-zjsg.15): a bar a category, top to bottom, centred, as wide against the widest
/// as its value, the category at its left, and data labels drawn only when `<cx:dataLabels>` enables them (Excel draws
/// none on an unstyled funnel, dw3-page2, and centred white values when `<cx:visibility value="1"/>` is set, dw2
/// `chartex-excel.xlsx` / `Funnel.png`).
fn funnel_marks(svg: &mut String, plot: &PlotView, area: Area) {
    let Some(series) = plot.series.first() else { return };
    let count = series.values.len();
    let widest = series.values.iter().flatten().copied().filter(|v| v.is_finite()).fold(0.0f64, f64::max);
    if count == 0 || widest <= 0.0 {
        return;
    }
    let (x0, x1, y0, y1) = (area.left + 60.0, area.right, area.top, area.bottom);
    let (middle, band) = ((x0 + x1) / 2.0, (y1 - y0) / count as f64);
    // A bar is the band over one plus the gap width (the file's; Excel's own funnel writes 0.06).
    let bar = band / (1.0 + plot.gap_width.unwrap_or(0.06));
    for (j, v) in series.values.iter().enumerate() {
        let y = y0 + j as f64 * band;
        let text = series.categories.get(j).cloned().flatten().map(|v| text_of(&v)).unwrap_or_else(|| (j + 1).to_string());
        let _ = write!(svg, "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" fill=\"{AXIS}\">{}</text>", x0 - 4.0, y + band / 2.0 + 4.0, escape(&text));
        // Excel draws no bar for a negative value (dw2 Windows item 8).
        let Some(v) = v.filter(|v| v.is_finite() && *v >= 0.0) else { continue };
        let w = (x1 - x0) * v / widest;
        let _ = write!(
            svg,
            "<rect data-dnacalc=\"chart-mark\" x=\"{:.1}\" y=\"{:.1}\" width=\"{w:.1}\" height=\"{:.1}\" fill=\"#{}\"",
            middle - w / 2.0,
            y + (band - bar) / 2.0,
            bar,
            series.color
        );
        mark(svg, 0, j, v);
        svg.push_str("/>");
        if let Some(text) = label_text(series, j, v, None, &series.name) {
            let _ = write!(
                svg,
                "<text data-dnacalc=\"chart-data-label\" data-point=\"{j}\" x=\"{middle:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"#FFFFFF\">{}</text>",
                y + band / 2.0 + 4.0,
                escape(&text)
            );
        }
    }
}

/// A radar's series over spokes, one spoke a category clockwise from twelve o'clock: its value axis as rings
/// (polygons) at the scale's ticks, labelled up the first spoke; each series a closed line (`standard`), with markers
/// (`marker`), or filled (`filled`). A gap breaks the line. Marker size and label offsets are ASSUMED until an Excel
/// side-by-side.
fn radar_marks(svg: &mut String, chart: &ChartView, plot: &PlotView, area: Area) {
    let scale = match calc_host::chart::category_scale(chart, plot) {
        Ok(scale) => scale,
        Err(message) => {
            axis_notice(svg, &area, &message);
            return;
        }
    };
    let Ok(axes) = calc_host::chart::value_axes(&chart.axes, &plot.axis_ids, false) else {
        return;
    };
    let value_axis = axes[0].id;
    let grid = gridline_color(chart, value_axis);
    let count = plot.series.iter().map(|s| s.values.len()).max().unwrap_or(0);
    if count == 0 {
        return;
    }
    let (cx, cy) = ((area.left + area.right) / 2.0, (area.top + area.bottom) / 2.0);
    let radius = ((area.right - area.left).min(area.bottom - area.top) / 2.0 - 18.0).max(4.0);
    let angle = |j: usize| (-90.0 + 360.0 * j as f64 / count as f64).to_radians();
    let at = |t: f64, j: usize| (cx + radius * t * angle(j).cos(), cy + radius * t * angle(j).sin());
    let fraction = |v: f64| scale.position(v);
    // Rings and spokes.
    for v in scale.ticks() {
        let Some(t) = fraction(v) else { continue };
        let mut d = String::new();
        for j in 0..count {
            let (x, y) = at(t, j);
            let _ = write!(d, "{}{x:.1} {y:.1} ", if j == 0 { "M" } else { "L" });
        }
        let _ = write!(svg, "<path data-dnacalc=\"chart-radar-ring\" d=\"{d}Z\" fill=\"none\" stroke=\"{grid}\"/>");
        let (x, y) = at(t, 0);
        let _ = write!(
            svg,
            "<text data-chart-text=\"axis:{value_axis}\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" fill=\"{AXIS}\">{}</text>",
            x - 3.0,
            y + 4.0,
            escape(&axis_label(chart, value_axis, v, scale.step))
        );
    }
    let categories = plot.series.iter().find(|s| !s.categories.is_empty()).map(|s| s.categories.clone()).unwrap_or_default();
    let category_axis = chart.axes.iter().find(|a| plot.axis_ids.contains(&a.id) && a.id != value_axis).map_or(0, |a| a.id);
    for j in 0..count {
        let (x, y) = at(1.0, j);
        let _ = write!(svg, "<line x1=\"{cx:.1}\" y1=\"{cy:.1}\" x2=\"{x:.1}\" y2=\"{y:.1}\" stroke=\"{grid}\"/>");
        let (lx, ly) = (cx + (radius + 10.0) * angle(j).cos(), cy + (radius + 10.0) * angle(j).sin());
        let anchor = if (lx - cx).abs() < 1.0 { "middle" } else if lx > cx { "start" } else { "end" };
        let text = categories.get(j).cloned().flatten().map(|v| text_of(&v)).unwrap_or_else(|| (j + 1).to_string());
        let _ = write!(
            svg,
            "<text data-chart-text=\"axis:{category_axis}\" x=\"{lx:.1}\" y=\"{:.1}\" text-anchor=\"{anchor}\" fill=\"{AXIS}\">{}</text>",
            ly + 4.0,
            escape(&text)
        );
    }
    let filled = plot.style.as_deref() == Some("filled");
    let markers = plot.style.as_deref() == Some("marker");
    for (i, series) in plot.series.iter().enumerate() {
        let points: Vec<Option<(f64, f64, f64)>> = (0..count)
            .map(|j| series.values.get(j).copied().flatten().and_then(|v| fraction(v).map(|t| (at(t, j), v))).map(|((x, y), v)| (x, y, v)))
            .collect();
        let whole = points.iter().all(Option::is_some);
        let mut d = String::new();
        let mut joined = false;
        for p in &points {
            match p {
                Some((x, y, _)) => {
                    let _ = write!(d, "{}{x:.1} {y:.1} ", if joined { "L" } else { "M" });
                    joined = true;
                }
                None => joined = false,
            }
        }
        if whole {
            d.push('Z');
        }
        if filled {
            let _ = write!(svg, "<path d=\"{d}\" fill=\"#{}\" data-series=\"{i}\"/>", series.color);
        } else {
            let _ = write!(
                svg,
                "<path d=\"{d}\" fill=\"none\" stroke=\"#{}\" stroke-width=\"2.25\" stroke-linejoin=\"round\" data-series=\"{i}\"/>",
                series.color
            );
        }
        for (j, p) in points.iter().enumerate() {
            let Some((x, y, v)) = p else { continue };
            let _ = write!(
                svg,
                "<circle data-dnacalc=\"chart-mark\" cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"{}\" fill=\"#{}\"",
                if markers { 3.0 } else { 0.0 },
                series.color
            );
            mark(svg, i, j, *v);
            svg.push_str("/>");
        }
    }
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
    let mut labels = String::new();
    for (j, value) in series.values.iter().enumerate() {
        let Some(v) = value.filter(|v| *v > 0.0) else {
            continue;
        };
        let sweep = v / total * std::f64::consts::TAU;
        let color = if plot.vary_colors && !series.explicit_color {
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
        // The slice's label (dc-zjsg.6): inside at its middle, or past its edge for `outEnd` (bestFit ASSUMED inside).
        if let Some(text) = label_text(series, j, v, Some(v / total), &series.name) {
            let reach = match label_position(series, j) {
                Some("outEnd") => r + 12.0,
                Some("ctr") => (r + hole) / 2.0,
                Some("inEnd") => r * 0.8,
                _ => hole + (r - hole) * 0.65,
            };
            let (x, y) = point(angle + sweep / 2.0, reach);
            let anchor = if reach > r { if x >= cx { "start" } else { "end" } } else { "middle" };
            let _ = write!(
                labels,
                "<text data-dnacalc=\"chart-data-label\" data-point=\"{j}\" x=\"{x:.1}\" y=\"{:.1}\" text-anchor=\"{anchor}\" fill=\"{AXIS}\">{}</text>",
                y + 4.0,
                escape(&text)
            );
        }
        angle += sweep;
    }
    svg.push_str(&labels);
}

/// The `dLblPos` for point `j` of `series`: the point's own `<c:dLbl><c:dLblPos>` when present, else the series'
/// `<c:dLbls><c:dLblPos>` (dc-zjsg.6).
fn label_position<'a>(series: &'a calc_host::chart::SeriesView, j: usize) -> Option<&'a str> {
    let all = series.labels.as_ref()?;
    all.points
        .iter()
        .find(|(idx, _)| *idx as usize == j)
        .and_then(|(_, own)| own.position.as_deref())
        .or(all.position.as_deref())
}

/// A point's data label as the series' `dLbls` composes it (dc-zjsg.6): its series name, category, value and (in a
/// pie) percentage in Excel's order, joined by the separator (`, ` when the file states none, ASSUMED); the value as
/// its cell shows it, else in General; the percentage rounded to a whole percent. `None` without labels.
fn label_text(series: &calc_host::chart::SeriesView, j: usize, value: f64, share: Option<f64>, name: &str) -> Option<String> {
    let all = series.labels.as_ref()?;
    // A point's own `dLbl` replaces the series' setting for it.
    let labels = all.points.iter().find(|(idx, _)| *idx as usize == j).map_or(all, |(_, own)| own);
    if labels.deleted {
        return None;
    }
    let mut parts = Vec::new();
    if labels.series {
        parts.push(name.to_owned());
    }
    if labels.category {
        parts.push(series.categories.get(j).cloned().flatten().map(|v| text_of(&v)).unwrap_or_else(|| (j + 1).to_string()));
    }
    if labels.value {
        parts.push(series.label_values.get(j).cloned().flatten().unwrap_or_else(|| label(value, 0.0)));
    }
    if labels.percent && let Some(share) = share {
        parts.push(format!("{}%", (share * 100.0).round()));
    }
    (!parts.is_empty()).then(|| parts.join(labels.separator.as_deref().unwrap_or(", ")))
}

/// A scatter plot's markers, on value axes for X and Y.
fn scatter_marks(svg: &mut String, chart: &ChartView, plot: &PlotView, area: Area) {
    let axes = match calc_host::chart::value_axes(&chart.axes, &plot.axis_ids, true) {
        Ok(axes) => axes,
        Err(message) => {
            axis_notice(svg, &area, &message);
            return;
        }
    };
    let (x_axis, y_axis) = (axes[0].id, axes[1].id);
    let grid_x = gridline_color(chart, x_axis);
    let grid_y = gridline_color(chart, y_axis);
    let series = plot
        .series
        .iter()
        .map(|s| (calc_host::chart::scatter_x_values(s), s))
        .collect::<Vec<_>>();
    if !series.iter().any(|(xs, s)| {
        xs.iter()
            .zip(&s.values)
            .any(|(x, y)| x.is_some() && y.is_some())
    }) {
        return;
    }
    let ((xl, xh), (yl, yh)) = calc_host::chart::scatter_scale_ranges(
        plot,
        axes[0].log_base.is_some(),
        axes[1].log_base.is_some(),
    );
    let xscale = calc_host::chart::chart_scale(xl, xh, Some(axes[0]));
    let yscale = calc_host::chart::chart_scale(yl, yh, Some(axes[1]));
    let (xscale, yscale) = match (xscale, yscale) {
        (Ok(x), Ok(y)) => (x, y),
        (Err(message), _) | (_, Err(message)) => {
            axis_notice(svg, &area, &message);
            return;
        }
    };
    let (xmin, xmax, xstep) = (xscale.minimum, xscale.maximum, xscale.step);
    let (ymin, ymax, ystep) = (yscale.minimum, yscale.maximum, yscale.step);
    let widest = axis_label(chart, y_axis, ymax, ystep).len().max(axis_label(chart, y_axis, ymin, ystep).len()) as f64;
    let (x0, x1, y0, y1) = (
        area.left + widest * 6.5 + 8.0,
        area.right,
        area.top,
        area.bottom - 18.0,
    );
    let px = |x: f64| {
        let t = xscale.position(x).unwrap_or(0.);
        x0 + t * (x1 - x0)
    };
    let py = |y: f64| {
        let t = yscale.position(y).unwrap_or(0.);
        y1 - t * (y1 - y0)
    };
    for v in yscale.ticks() {
        let _ = write!(
            svg,
            "<line x1=\"{x0:.1}\" y1=\"{:.1}\" x2=\"{x1:.1}\" y2=\"{:.1}\" stroke=\"{grid_y}\"/><text data-chart-text=\"axis:{y_axis}\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" fill=\"{AXIS}\">{}</text>",
            py(v),
            py(v),
            x0 - 4.0,
            py(v) + 4.0,
            escape(&axis_label(chart, y_axis, v, ystep))
        );
    }
    for v in xscale.ticks() {
        let _ = write!(
            svg,
            "<line x1=\"{:.1}\" y1=\"{y0:.1}\" x2=\"{:.1}\" y2=\"{y1:.1}\" stroke=\"{grid_x}\"/><text data-chart-text=\"axis:{x_axis}\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"{AXIS}\">{}</text>",
            px(v),
            px(v),
            px(v),
            y1 + 13.0,
            escape(&axis_label(chart, x_axis, v, xstep))
        );
    }
    let _ = write!(
        svg,
        "<svg data-dnacalc=\"chart-plot\" data-x-min=\"{xmin}\" data-x-max=\"{xmax}\" data-y-min=\"{ymin}\" data-y-max=\"{ymax}\" x=\"{x0}\" y=\"{y0}\" width=\"{}\" height=\"{}\" viewBox=\"{x0} {y0} {} {}\" overflow=\"hidden\">",
        x1 - x0,
        y1 - y0,
        x1 - x0,
        y1 - y0
    );
    // A bubble plot's largest bubble is 29.5 % of the plot's smaller side across at a scale of 100% (Excel-measured), its others by area (or by width), negative and zero sizes not shown.
    let bubble = plot.kind == ChartKind::Bubble;
    let largest = series.iter().flat_map(|(_, s)| s.sizes.iter().flatten().copied()).filter(|v| v.is_finite() && *v > 0.).fold(0.0f64, f64::max);
    // Excel (dw2 Windows item 5): the largest bubble is 29.5 % of the plot's smaller side across at 100 %.
    let biggest_radius = (x1 - x0).min(y1 - y0) * 0.1475 * plot.bubble_scale / 100.0;
    for (i, (xs, s)) in series.iter().enumerate() {
        for (j, (x, y)) in xs.iter().zip(&s.values).enumerate() {
            if let (Some(x), Some(y)) = (x, y) {
                if xscale.position(*x).is_none() || yscale.position(*y).is_none() {
                    continue;
                }
                if bubble {
                    let Some(size) = s.sizes.get(j).copied().flatten().filter(|v| v.is_finite() && *v > 0. && largest > 0.) else { continue };
                    let share = size / largest;
                    let r = biggest_radius * if plot.bubble_width { share } else { share.sqrt() };
                    let _ = write!(
                        svg,
                        "<circle data-dnacalc=\"chart-mark\" cx=\"{:.1}\" cy=\"{:.1}\" r=\"{r:.1}\" fill=\"#{}\" fill-opacity=\"0.75\" stroke=\"#{}\"",
                        px(*x),
                        py(*y),
                        s.color,
                        s.color
                    );
                    mark(svg, i, j, *y);
                    let _ = write!(svg, " data-x=\"{x}\" data-size=\"{size}\"");
                    svg.push_str("/>");
                    continue;
                }
                let _ = write!(
                    svg,
                    // Excel's first marker, a diamond.
                    "<path data-dnacalc=\"chart-mark\" d=\"M{:.1} {:.1}l4 4-4 4-4-4z\" fill=\"#{}\"",
                    px(*x),
                    py(*y) - 4.0,
                    s.color
                );
                mark(svg, i, j, *y);
                let _ = write!(svg, " data-x=\"{x}\"");
                svg.push_str("/>");
            }
        }
    }
    // Error bars along y (about the value) and along x (about the x value) (dc-zjsg.6).
    for (i, (xs, s)) in series.iter().enumerate() {
        for bars in &s.error_bars {
            let across = bars.direction.as_deref() == Some("x");
            let along: Vec<Option<f64>> = if across { xs.clone() } else { s.values.clone() };
            for (j, extent) in error_extents(bars, &along).into_iter().enumerate() {
                let (Some((low, high, _)), Some(Some(x)), Some(Some(y))) = (extent, xs.get(j), s.values.get(j)) else { continue };
                let cap = bars.end_cap.then_some(4.0);
                if across {
                    error_bar_svg(svg, i, j, (px(low), py(*y)), (px(high), py(*y)), cap);
                } else {
                    error_bar_svg(svg, i, j, (px(*x), py(low)), (px(*x), py(high)), cap);
                }
            }
        }
    }
    // Trendlines over the x values (dc-zjsg.6).
    for (i, (xs, s)) in series.iter().enumerate() {
        let xy: Vec<(f64, f64)> = xs.iter().zip(&s.values).filter_map(|(x, y)| Some(((*x)?, (*y)?))).collect();
        let (low, high) = xy.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
        for (t, colour) in &s.trendlines {
            let mut xy = xy.clone();
            xy.sort_by(|a, b| a.0.total_cmp(&b.0));
            let Some(points) = trendline_points(t, &xy, low, high) else { continue };
            let drawn: Vec<(f64, f64)> = points.into_iter().filter(|(x, y)| xscale.position(*x).is_some() || yscale.position(*y).is_some()).map(|(x, y)| (px(x), py(y))).collect();
            trendline_svg(svg, i, colour, &drawn);
        }
    }
    svg.push_str("</svg>");
}

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../calc-doc/tests/cdoc_m1_1_support/mod.rs"]
mod zip_support;

#[cfg(test)]
mod tests {
    use super::*;

    /// dc-zjsg.6: a box and whisker's statistics as Excel's help defines them: QUARTILE.EXC's or QUARTILE.INC's
    /// quartiles, whiskers to the last values within 1.5 interquartile ranges, the rest outliers.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_zjsg_6__box_and_whisker_quartiles_whiskers_and_outliers() {
        let values = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 40.0];
        // QUARTILE.EXC(1..9, 40; 1) = 2.75, (; 3) = 8.25; QUARTILE.INC = 3.25 and 7.75; the median 5.5.
        let exclusive = box_stats(&values, false).unwrap();
        assert_eq!((exclusive.q1, exclusive.median, exclusive.q3), (2.75, 5.5, 8.25));
        // 1.5 x 5.5 = 8.25 past the box: 40 is an outlier, the upper whisker ends at 9.
        assert_eq!((exclusive.low, exclusive.high, exclusive.outliers.clone()), (1.0, 9.0, vec![40.0]));
        let inclusive = box_stats(&values, true).unwrap();
        assert_eq!((inclusive.q1, inclusive.q3), (3.25, 7.75));
        assert!((exclusive.mean - 8.5).abs() < 1e-12);
    }

    /// dc-zjsg.6: 3-D column and pie charts are drawn flat; radar (marker, filled), bubble and stock (high-low-close,
    /// open-high-low-close with up/down bars) charts are drawn from their own plots.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_zjsg_6__three_d_radar_bubble_and_stock_charts_draw() {
        use calc_host::{HostOptions, HostSession, MemorySource, OpenPolicy, PackageRead, SkinIntent};
        use std::sync::Arc;
        let bytes = include_bytes!("../../calc-doc/tests/fixtures/drawing/chart-kinds.xlsx");
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
        let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
        assert!(book.dispatch(SkinIntent::Recalculate).accepted());
        let (charts, unread) = book.host().charts(book.host().sheets()[0].id());
        assert_eq!((charts.len(), unread), (22, 0));
        let svg = |name: &str| chart_svg(charts.iter().find(|c| c.name == name).unwrap(), 600., 350.);
        let count = |svg: &str, needle: &str| svg.matches(needle).count();
        for name in ["3-D Column", "3-D Pie", "Radar marker", "Radar filled", "Bubble", "Stock HLC", "Stock OHLC", "3-D Line", "Line trends", "Scatter exp", "Line drops", "Column sd", "Pie labels", "Column labels", "Waterfall", "Funnel", "Box", "Histogram", "Pareto", "Treemap", "Sunburst", "Surface"] {
            let drawn = svg(name);
            assert!(!drawn.contains("is not drawn here") && !drawn.contains("chart-notice"), "{name}: {drawn}");
        }
        // Trendlines (dc-zjsg.6): B's linear fit over Mon..Fri (10, 12, 9, 11, 14: y = 9.1 + 0.7x) drawn from category 1
        // to 6 (forward 1); C's polynomial and moving average; the scatter's exponential in red.
        let line = charts.iter().find(|c| c.name == "Line trends").unwrap();
        let (t, _) = &line.plots[0].series[0].trendlines[0];
        assert_eq!((t.kind.as_str(), t.forward.as_deref()), ("linear", Some("1")));
        let points = trendline_points(t, &[(1., 10.), (2., 12.), (3., 9.), (4., 11.), (5., 14.)], 1.0, 5.0).unwrap();
        assert_eq!(points.len(), 2);
        assert!((points[0].1 - 9.8).abs() < 1e-9 && (points[1].0 - 6.0).abs() < 1e-12 && (points[1].1 - 13.3).abs() < 1e-9, "{points:?}");
        let moving = &line.plots[0].series[1].trendlines[1].0;
        assert_eq!(trendline_points(moving, &[(1., 18.), (2., 20.), (3., 15.)], 1.0, 3.0).unwrap(), vec![(2.0, 19.0), (3.0, 17.5)]);
        let quad = &line.plots[0].series[1].trendlines[0].0;
        let fitted = trendline_points(quad, &[(1., 1.), (2., 4.), (3., 9.), (4., 16.)], 1.0, 4.0).unwrap();
        assert!(fitted.iter().all(|(x, y)| (y - x * x).abs() < 1e-9), "{fitted:?}");
        assert_eq!(count(&svg("Line trends"), "data-dnacalc=\"chart-trendline\""), 3);
        let scatter = svg("Scatter exp");
        assert_eq!(count(&scatter, "data-dnacalc=\"chart-trendline\""), 1);
        assert!(scatter.contains("stroke=\"#FF0000\" stroke-width=\"2\" stroke-dasharray"), "{scatter}");
        // Drop lines (ten points), high-low lines (five days) and error bars (dc-zjsg.6).
        let drops = svg("Line drops");
        assert_eq!((count(&drops, "chart-drop-line"), count(&drops, "chart-high-low"), count(&drops, "chart-error-bar")), (10, 5, 10));
        let line = charts.iter().find(|c| c.name == "Line drops").unwrap();
        let fixed = error_extents(&line.plots[0].series[0].error_bars[0], &[Some(10.0), None]);
        assert_eq!(fixed, vec![Some((9.0, 11.0, 10.0)), None]);
        let percent = &line.plots[0].series[1].error_bars[0];
        assert!(!percent.end_cap);
        assert_eq!(error_extents(percent, &[Some(20.0)]), vec![Some((20.0, 22.0, 20.0))]);
        // One standard deviation (n - 1) about the mean of 10, 12, 9, 11, 14: 11.2 +- 1.9235...
        let column = charts.iter().find(|c| c.name == "Column sd").unwrap();
        let sd = error_extents(&column.plots[0].series[0].error_bars[0], &column.plots[0].series[0].values);
        let (low, high, centre) = sd[0].unwrap();
        assert!((centre - 11.2).abs() < 1e-9 && (high - low - 2.0 * 3.7f64.sqrt()).abs() < 1e-9, "{sd:?}");
        assert_eq!(count(&svg("Column sd"), "chart-error-bar"), 5);
        assert_eq!(count(&svg("Scatter exp"), "chart-error-bar"), 5);
        // Labels composed from the dLbls (dc-zjsg.6): category and whole percent on the pie (Mon is 10 of 56: 18%),
        // series name and value with the stated separator on the columns.
        let pie = svg("Pie labels");
        assert_eq!(count(&pie, "chart-data-label"), 5);
        assert!(pie.contains(">Mon, 18%</text>"), "{pie}");
        let columns = svg("Column labels");
        assert!(columns.contains(">Low; 10</text>"), "{columns}");
        // Per-point labels: point 1 its own parts (category and value, ": "), point 2 none.
        assert!(columns.contains(">Tue: 12</text>") && columns.contains(">Low; 11</text>") && !columns.contains(">Low; 9</text>"), "{columns}");
        // Excel 2016 charts (dc-zjsg.6): a waterfall of 10, -3, 4, -2 and the subtotal 9 steps 0-10, 10-7, 7-11, 11-9,
        // then stands 0-9; a funnel of five centred bars; a treemap kept, said not drawn.
        // Excel's connector lines between waterfall bars (dw2 Windows item 8): four between five bars.
        assert_eq!(count(&svg("Waterfall"), "chart-connector"), 4);
        // Standard error from the deviations about the mean (Excel-measured: 0.86 for 10, 12, 9, 11, 14), standard
        // deviation one bar about the mean.
        let bars = |kind: &str| calc_host::ChartErrorBars { direction: None, kind: Some("both".into()), value_type: Some(kind.into()), value: None, end_cap: true };
        let values = [Some(10.0), Some(12.0), Some(9.0), Some(11.0), Some(14.0)];
        let (low, high, _) = error_extents(&bars("stdErr"), &values)[0].unwrap();
        assert!(((high - low) / 2.0 - 0.8602).abs() < 0.001, "{low} {high}");
        let (low, high, centre) = error_extents(&bars("stdDev"), &values)[3].unwrap();
        assert!((centre - 11.2).abs() < 1e-9 && (low - 9.28).abs() < 0.01 && (high - 13.12).abs() < 0.01, "{low} {high}");
        let waterfall = charts.iter().find(|c| c.name == "Waterfall").unwrap();
        assert_eq!((waterfall.plots[0].kind.clone(), waterfall.plots[0].subtotals.clone()), (ChartKind::Waterfall, vec![4]));
        assert_eq!(waterfall.plots[0].series[0].values, vec![Some(10.0), Some(-3.0), Some(4.0), Some(-2.0), Some(9.0)]);
        let drawn = svg("Waterfall");
        assert_eq!((count(&drawn, "data-kind=\"increase\""), count(&drawn, "data-kind=\"decrease\""), count(&drawn, "data-kind=\"total\"")), (2, 2, 1));
        assert_eq!(count(&svg("Funnel"), "<rect data-dnacalc=\"chart-mark\""), 5);
        // A surface over three series of five points: 4 x 2 cells, filled unless the file says wireframe.
        let surface = svg("Surface");
        assert_eq!(count(&surface, "<path data-dnacalc=\"chart-mark\""), 8, "{surface}");
        // A treemap over B2:B6 in two levels: five leaves in two parents; a sunburst of C: seven arcs, two of them parents.
        let tree = svg("Treemap");
        assert_eq!((count(&tree, "<rect data-dnacalc=\"chart-mark\""), count(&tree, "chart-group")), (5, 2), "{tree}");
        let burst = svg("Sunburst");
        assert_eq!((count(&burst, "<path data-dnacalc=\"chart-mark\""), count(&burst, "data-group")), (7, 2), "{burst}");
        // A box over B2:C6 (10, 12, 9, 11, 14, 18, 20, 15, 22, 19): QUARTILE.EXC 10.75 and 19.25, median 14.5.
        let boxed = svg("Box");
        assert!(boxed.contains("data-q1=\"10.75\" data-median=\"14.5\" data-q3=\"19.25\""), "{boxed}");
        assert_eq!((count(&boxed, "chart-mean"), count(&boxed, "chart-outlier")), (1, 0));
        // A histogram over C2:C6 (18, 20, 15, 22, 19) by 3 from 15: [15, 18], (18, 21], (21, 24] hold 2, 2 and 1; the
        // Pareto sorts them and adds the cumulative line.
        let histogram = svg("Histogram");
        assert_eq!(count(&histogram, "<rect data-dnacalc=\"chart-mark\""), 3, "{histogram}");
        assert!(histogram.contains(">[15, 18]</text>") && histogram.contains(">(18, 21]</text>") && !histogram.contains("chart-pareto"), "{histogram}");
        let pareto = svg("Pareto");
        assert!(count(&pareto, "chart-pareto") == 1 && pareto.contains(">100%</text>"), "{pareto}");
        assert_eq!(charts.iter().find(|c| c.name == "Pareto").unwrap().plots[0].series.len(), 1);
        // The drawing reader takes those frames for charts, not unread objects (their text-box fallbacks unread too).
        let sheet = book.host().sheets()[0].id();
        assert_eq!(book.host().loaded().sheet(sheet).unwrap().shapes().unread, 0);
        assert!(book.host().shapes(sheet).is_empty());
        // Two series of five columns; five slices.
        assert_eq!(count(&svg("3-D Column"), "<rect data-dnacalc=\"chart-mark\""), 10);
        assert!(charts.iter().find(|c| c.name == "3-D Column").unwrap().plots[0].three_d);
        assert_eq!(count(&svg("3-D Pie"), "data-dnacalc=\"chart-mark\""), 5);
        // A 3-D line with its depth axis: two lines of five points, the depth axis left out.
        assert_eq!(count(&svg("3-D Line"), "<circle data-dnacalc=\"chart-mark\""), 10);
        // A radar: a ring per tick, two series with visible markers; the filled one a filled path.
        let radar = svg("Radar marker");
        assert!(count(&radar, "chart-radar-ring") >= 3, "{radar}");
        assert_eq!(count(&radar, "<circle data-dnacalc=\"chart-mark\""), 10);
        assert!(radar.contains("r=\"3\""));
        assert!(svg("Radar filled").contains("Z\" fill=\"#"));
        // Bubbles sized by area: the largest (21 at Thu) has twice the radius of a size a quarter of it would.
        let bubble = svg("Bubble");
        assert_eq!(count(&bubble, "<circle data-dnacalc=\"chart-mark\""), 5);
        let radius = |size: &str| {
            let at = bubble.find(&format!("data-size=\"{size}\"")).unwrap();
            let tag = &bubble[bubble[..at].rfind("<circle").unwrap()..at];
            let r = tag.split(" r=\"").nth(1).unwrap().split('"').next().unwrap();
            r.parse::<f64>().unwrap()
        };
        let (big, small) = (radius("21"), radius("11"));
        assert!((small / big - (11.0f64 / 21.0).sqrt()).abs() < 0.02, "{big} {small}");
        // Stock: a high-low line per day; HLC closes as ticks, OHLC as up/down boxes.
        let hlc = svg("Stock HLC");
        assert_eq!(count(&hlc, "chart-high-low"), 5);
        assert_eq!(count(&hlc, "<line data-dnacalc=\"chart-mark\""), 5);
        let ohlc = svg("Stock OHLC");
        assert_eq!(count(&ohlc, "chart-high-low"), 5);
        assert_eq!(count(&ohlc, "<rect data-dnacalc=\"chart-mark\""), 5);
        // Mon opens 10, closes 16: an up (white) box; Tue opens 12, closes 12 (equal: up).
        assert!(ohlc.contains("fill=\"#FFFFFF\""));
    }

    /// dc-zjsg.6 (dw4): a per-point `<c:dLbl>` uses its own `dLblPos` when present on both column and pie charts,
    /// leaving other points on the series position.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dw4_dc_zjsg_6__per_point_dlbl_uses_own_dlbl_pos() {
        use calc_host::{HostOptions, HostSession, MemorySource, OpenPolicy, PackageRead, SkinIntent};
        use std::sync::Arc;
        let bytes = include_bytes!("../../calc-doc/tests/fixtures/drawing/chart-kinds.xlsx");
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
        let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
        assert!(book.dispatch(SkinIntent::Recalculate).accepted());
        let (charts, _) = book.host().charts(book.host().sheets()[0].id());
        let attr = |tag: &str, name: &str| -> f64 {
            let needle = format!("{name}=\"");
            tag.split(&needle).nth(1).unwrap().split('"').next().unwrap().parse().unwrap()
        };
        let label_tag = |svg: &str, kind: &str, point: usize| -> String {
            let needle = format!("data-dnacalc=\"{kind}\"");
            let pt = format!("data-point=\"{point}\"");
            svg.split('<')
                .find(|t| t.starts_with("text ") && t.contains(&needle) && t.contains(&pt))
                .unwrap_or_else(|| panic!("missing {kind} point {point} in {svg}"))
                .to_owned()
        };

        // Column: equal values, point 0 has its own dLblPos="inBase" while point 1 uses the series default outEnd.
        let mut col = charts.iter().find(|c| c.name == "Column labels").unwrap().clone();
        col.plots[0].series[0].values = vec![Some(10.0), Some(10.0)];
        let mut series_labels = col.plots[0].series[0].labels.clone().unwrap();
        series_labels.value = true;
        series_labels.series = false;
        series_labels.category = false;
        series_labels.percent = false;
        series_labels.position = None;
        series_labels.points.clear();
        let mut own = series_labels.clone();
        own.position = Some("inBase".into());
        series_labels.points.push((0, own));
        col.plots[0].series[0].labels = Some(series_labels);
        let col_svg = chart_svg(&col, 600., 350.);
        let y0 = attr(&label_tag(&col_svg, "chart-value-label", 0), "y");
        let y1 = attr(&label_tag(&col_svg, "chart-value-label", 1), "y");
        assert!(y0 > y1 + 40.0, "expected point 0 (inBase) well below point 1 (outEnd): y0={y0}, y1={y1}");

        // Pie: equal slices, point 0 has its own dLblPos="outEnd" while point 1 stays on series bestFit.
        let mut pie = charts.iter().find(|c| c.name == "Pie labels").unwrap().clone();
        pie.plots[0].series[0].values = vec![Some(10.0), Some(10.0)];
        let mut pie_labels = pie.plots[0].series[0].labels.clone().unwrap();
        pie_labels.value = true;
        pie_labels.category = false;
        pie_labels.percent = false;
        pie_labels.position = Some("bestFit".into());
        pie_labels.points.clear();
        let mut pie_own = pie_labels.clone();
        pie_own.position = Some("outEnd".into());
        pie_labels.points.push((0, pie_own));
        pie.plots[0].series[0].labels = Some(pie_labels);
        let pie_svg = chart_svg(&pie, 600., 350.);
        let t0 = label_tag(&pie_svg, "chart-data-label", 0);
        let t1 = label_tag(&pie_svg, "chart-data-label", 1);
        assert!(t0.contains("text-anchor=\"start\"") || t0.contains("text-anchor=\"end\""), "expected outEnd anchor on point 0: {t0}");
        assert!(t1.contains("text-anchor=\"middle\""), "expected inside anchor on point 1: {t1}");
    }

    /// dc-zjsg.6 (dw4): assertions for the Excel-measured chart details landed in acbfa9d4 (dw2 Windows results):
    /// funnel omits negative values, stock close tick is 3 px, down bars are #595959, largest bubble is 29.5% of
    /// plot height, sysDot trendline dash is `0 4`, and column data labels default to `outEnd`.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dw4_dc_zjsg_6__excel_measured_chart_details_from_acbfa9d4() {
        use calc_host::{HostOptions, HostSession, MemorySource, OpenPolicy, PackageRead, SkinIntent};
        use std::sync::Arc;
        let bytes = include_bytes!("../../calc-doc/tests/fixtures/drawing/chart-kinds.xlsx");
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
        let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
        assert!(book.dispatch(SkinIntent::Recalculate).accepted());
        let (charts, _) = book.host().charts(book.host().sheets()[0].id());
        let chart = |name: &str| charts.iter().find(|c| c.name == name).unwrap();
        let attr = |tag: &str, name: &str| -> f64 {
            let needle = format!("{name}=\"");
            tag.split(&needle).nth(1).unwrap().split('"').next().unwrap().parse().unwrap()
        };

        // 1. Funnel omits negative values (only the 3 non-negative values produce bars).
        let mut funnel_neg = chart("Funnel").clone();
        funnel_neg.plots[0].series[0].values = vec![Some(10.0), Some(-3.0), Some(4.0), Some(-2.0), Some(9.0)];
        let funnel_svg = chart_svg(&funnel_neg, 600., 350.);
        assert_eq!(funnel_svg.matches("<rect data-dnacalc=\"chart-mark\"").count(), 3, "{funnel_svg}");

        // 2. Stock HLC close tick is 3 px wide.
        let hlc_svg = chart_svg(chart("Stock HLC"), 600., 350.);
        let close_ticks: Vec<_> = hlc_svg.split('<').filter(|t| t.starts_with("line data-dnacalc=\"chart-mark\"")).collect();
        assert_eq!(close_ticks.len(), 5, "{hlc_svg}");
        for tick in close_ticks {
            let width = attr(tick, "x2") - attr(tick, "x1");
            assert!((width - 3.0).abs() < 0.05, "expected 3 px stock close tick, got {width}: {tick}");
        }

        // 3. Stock OHLC down bars use #595959 fill (close < open on Mon: 8 < 10).
        let mut ohlc_down = chart("Stock OHLC").clone();
        ohlc_down.plots[0].series[3].values[0] = Some(8.0);
        let ohlc_svg = chart_svg(&ohlc_down, 600., 350.);
        assert!(ohlc_svg.contains("fill=\"#595959\""), "{ohlc_svg}");
        assert!(ohlc_svg.contains("fill=\"#FFFFFF\""), "{ohlc_svg}");

        // 4. Bubble largest diameter is 29.5% of the plot's smaller side (height at 600x350).
        let bubble_svg = chart_svg(chart("Bubble"), 600., 350.);
        let plot_tag = bubble_svg.split('<').find(|t| t.starts_with("svg data-dnacalc=\"chart-plot\"")).unwrap();
        let plot_height = attr(plot_tag, "height");
        let big_tag = bubble_svg.split('<').find(|t| t.starts_with("circle data-dnacalc=\"chart-mark\"") && t.contains("data-size=\"21\"")).unwrap();
        let diameter = 2.0 * attr(big_tag, "r");
        assert!((diameter / plot_height - 0.295).abs() < 0.005, "diameter={diameter} plot_height={plot_height}");

        // 5. sysDot trendline dash is `0 4` with round caps.
        let scatter_svg = chart_svg(chart("Scatter exp"), 600., 350.);
        assert!(scatter_svg.contains("stroke-dasharray=\"0 4\" stroke-linecap=\"round\""), "{scatter_svg}");

        // 6. Column data labels default to outEnd (sits 4 px above the positive column top).
        let col_svg = chart_svg(chart("Column labels"), 600., 350.);
        let bar0 = col_svg.split('<').find(|t| t.starts_with("rect data-dnacalc=\"chart-mark\"") && t.contains("data-point=\"0\"")).unwrap();
        let lbl0 = col_svg.split('<').find(|t| t.starts_with("text data-dnacalc=\"chart-value-label\"") && t.contains("data-point=\"0\"")).unwrap();
        let (bar_y, lbl_y) = (attr(bar0, "y"), attr(lbl0, "y"));
        assert!(lbl_y < bar_y && (bar_y - lbl_y - 4.0).abs() < 0.2, "expected outEnd above bar top: bar_y={bar_y}, lbl_y={lbl_y}");
    }

    /// dc-zjsg.6 (dw4): Excel-measured sunburst and histogram layout from dw3 Windows items 6 & 7:
    /// sunburst orders parents/children descending while keeping data-order parent colours, leaves a
    /// one-band hole (`radius / (rings + 1)`), and uses 2 px white strokes; histogram honours `gapWidth="0"`
    /// with touching bars and 1 px white borders.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dw4_dc_zjsg_6__sunburst_and_histogram_match_excel_dw3_measurements() {
        use calc_host::{HostOptions, HostSession, MemorySource, OpenPolicy, PackageRead, SkinIntent};
        use std::sync::Arc;
        let bytes = include_bytes!("../../calc-doc/tests/fixtures/drawing/chart-kinds.xlsx");
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
        let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
        assert!(book.dispatch(SkinIntent::Recalculate).accepted());
        let (charts, _) = book.host().charts(book.host().sheets()[0].id());
        let chart = |name: &str| charts.iter().find(|c| c.name == name).unwrap();

        // Sunburst: Late (56) precedes Early (38) clockwise from 12 o'clock, keeping accent2 for Late and accent1
        // for Early; inner ring has a non-zero hole radius equal to one band and 2 px white stroke.
        let sb_chart = chart("Sunburst");
        let roots = crate::hierarchy::hierarchy(&sb_chart.plots[0].series[0]);
        let arcs = crate::hierarchy::arcs(&roots);
        assert_eq!((arcs[0].tile.name.as_str(), arcs[0].root), ("Late", 1));
        assert_eq!(arcs[1].tile.name.as_str(), "Thu"); // largest leaf in Late (22 > 19 > 15)
        let sb_svg = chart_svg(sb_chart, 600., 350.);
        assert!(sb_svg.contains("stroke=\"#FFFFFF\" stroke-width=\"2\""), "{sb_svg}");
        assert!(!sb_svg.contains("A0.0 0.0 "), "sunburst inner ring must leave a centre hole: {sb_svg}");

        // Histogram with gapWidth="0": adjacent bars touch (next x == prev x + width) with 1 px white stroke.
        let hist_svg = chart_svg(chart("Histogram"), 600., 350.);
        let attr = |tag: &str, name: &str| -> f64 {
            let needle = format!("{name}=\"");
            tag.split(&needle).nth(1).unwrap().split('"').next().unwrap().parse().unwrap()
        };
        let bars: Vec<_> = hist_svg.split('<').filter(|t| t.starts_with("rect data-dnacalc=\"chart-mark\"")).collect();
        assert_eq!(bars.len(), 3, "{hist_svg}");
        let (x0, w0, x1) = (attr(bars[0], "x"), attr(bars[0], "width"), attr(bars[1], "x"));
        assert!((x0 + w0 - x1).abs() < 0.2, "expected touching bars with gapWidth=0: x0={x0}, w0={w0}, x1={x1}");
        assert!(bars[0].contains("stroke=\"#FFFFFF\" stroke-width=\"1\""), "{}", bars[0]);
    }

    /// dc-zjsg.15 (dw3-page2-Kinds-excel-100.png & dw2 `chartex-excel.xlsx` / `Funnel.png`): an unstyled
    /// funnel draws no data labels; one with `<cx:dataLabels><cx:visibility value="1"/></cx:dataLabels>`
    /// draws centred white values on non-negative bars and obeys category/series/separator/hidden overrides.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dw5_dc_zjsg_15__funnel_labels_follow_cx_data_labels() {
        use calc_host::{HostOptions, HostSession, MemorySource, OpenPolicy, PackageRead, SkinIntent};
        use std::sync::Arc;

        let base = include_bytes!("../../calc-doc/tests/fixtures/drawing/chart-kinds.xlsx");
        let open_funnel = |bytes: Vec<u8>| {
            let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes));
            let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
            assert!(book.dispatch(SkinIntent::Recalculate).accepted());
            let (charts, _) = book.host().charts(book.host().sheets()[0].id());
            charts.into_iter().find(|c| c.name == "Funnel").unwrap()
        };
        let with_funnel_labels = |labels_xml: &str| -> Vec<u8> {
            let archive = zip_support::parse_zip(base);
            let entries: Vec<_> = archive
                .entries
                .iter()
                .map(|entry| {
                    let payload = entry.payload(base);
                    if entry.name == "xl/charts/chartEx2.xml" {
                        let raw = if entry.method == 0 {
                            payload.to_vec()
                        } else {
                            miniz_oxide::inflate::decompress_to_vec(payload).unwrap()
                        };
                        let xml = String::from_utf8(raw)
                            .unwrap()
                            .replace("<cx:dataId val=\"0\"/>", &format!("{labels_xml}<cx:dataId val=\"0\"/>"));
                        zip_support::RawEntry::stored(&entry.name, xml.as_bytes())
                    } else if entry.method == 0 {
                        zip_support::RawEntry::stored(&entry.name, payload)
                    } else {
                        zip_support::RawEntry::deflated(&entry.name, payload, entry.size, entry.crc32)
                    }
                })
                .collect();
            zip_support::build_zip(&entries)
        };

        // 1. Unstyled `chart-kinds.xlsx` Funnel (no `<cx:dataLabels>`): 5 bars, 0 data labels.
        let plain = open_funnel(base.to_vec());
        assert_eq!(plain.plots[0].series[0].labels, None);
        let plain_svg = chart_svg(&plain, 600., 350.);
        assert_eq!(plain_svg.matches("<rect data-dnacalc=\"chart-mark\"").count(), 5, "{plain_svg}");
        assert_eq!(plain_svg.matches("data-dnacalc=\"chart-data-label\"").count(), 0, "{plain_svg}");

        // 2. Explicit `<cx:dataLabels>` with all visibility flags 0: still 0 data labels.
        let off = open_funnel(with_funnel_labels(r#"<cx:dataLabels><cx:visibility seriesName="0" categoryName="0" value="0"/></cx:dataLabels>"#));
        let off_svg = chart_svg(&off, 600., 350.);
        assert_eq!(off_svg.matches("data-dnacalc=\"chart-data-label\"").count(), 0, "{off_svg}");

        // 3. Excel's `<cx:dataLabels><cx:visibility seriesName="0" categoryName="0" value="1"/></cx:dataLabels>`:
        //    5 centred white value labels (10, 12, 9, 11, 14), and negative values omit both bar and label.
        let mut valued = open_funnel(with_funnel_labels(r#"<cx:dataLabels><cx:visibility seriesName="0" categoryName="0" value="1"/></cx:dataLabels>"#));
        let valued_svg = chart_svg(&valued, 600., 350.);
        assert_eq!(valued_svg.matches("data-dnacalc=\"chart-data-label\"").count(), 5, "{valued_svg}");
        for expected in ["10", "12", "9", "11", "14"] {
            assert!(valued_svg.contains(&format!("fill=\"#FFFFFF\">{expected}</text>")), "missing {expected}: {valued_svg}");
        }
        valued.plots[0].series[0].values = vec![Some(10.0), Some(-3.0), Some(4.0), Some(-2.0), Some(9.0)];
        valued.plots[0].series[0].label_values = vec![Some("10".into()), Some("-3".into()), Some("4".into()), Some("-2".into()), Some("9".into())];
        let neg_svg = chart_svg(&valued, 600., 350.);
        assert_eq!(neg_svg.matches("<rect data-dnacalc=\"chart-mark\"").count(), 3, "{neg_svg}");
        assert_eq!(neg_svg.matches("data-dnacalc=\"chart-data-label\"").count(), 3, "{neg_svg}");
        assert!(!neg_svg.contains(">-3</text>") && !neg_svg.contains(">-2</text>"), "{neg_svg}");

        // 4. Category + value + custom separator + `<cx:dataLabelHidden idx="2"/>` + per-point override.
        let custom = open_funnel(with_funnel_labels(concat!(
            r#"<cx:dataLabels><cx:visibility seriesName="0" categoryName="1" value="1"/><cx:separator>: </cx:separator>"#,
            r#"<cx:dataLabelHidden idx="2"/>"#,
            r#"<cx:dataLabel idx="1"><cx:visibility seriesName="1" categoryName="0" value="1"/><cx:separator> - </cx:separator></cx:dataLabel>"#,
            r#"</cx:dataLabels>"#
        )));
        let custom_svg = chart_svg(&custom, 600., 350.);
        assert_eq!(custom_svg.matches("data-dnacalc=\"chart-data-label\"").count(), 4, "{custom_svg}");
        assert!(custom_svg.contains(">Mon: 10</text>"), "{custom_svg}");
        assert!(custom_svg.contains(">Low - 12</text>"), "{custom_svg}");
        assert!(!custom_svg.contains(">Wed: 9</text>"), "{custom_svg}");
        assert!(custom_svg.contains(">Thu: 11</text>"), "{custom_svg}");
        assert!(custom_svg.contains(">Fri: 14</text>"), "{custom_svg}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn imported_plot_marks_and_saved_value_notices_use_the_host_projection() {
        use calc_host::{
            HostOptions, HostSession, MemorySource, OpenPolicy, PackageRead, SkinIntent,
        };
        use std::sync::Arc;
        let bytes = include_bytes!("../../../crates/calc-host/tests/fixtures/dc_6yw/charts.xlsx");
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
        let mut book =
            HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
        assert!(book.dispatch(SkinIntent::Recalculate).accepted());
        let (charts, unread) = book.host().charts(book.host().sheets()[0].id());
        assert_eq!(unread, 0);
        assert_eq!(charts.len(), 6);
        for chart in &charts {
            let svg = chart_svg(chart, 600., 350.);
            assert!(svg.contains("data-point=\"0\""), "{}: {svg}", chart.name);
            assert!(!svg.contains("chart-cache"));
        }
        for index in [0, 5] {
            let mut hidden = charts[index].clone();
            hidden.axes[0].tick_labels_hidden = true;
            let target = format!("data-chart-text=\"axis:{}\"", hidden.axes[0].id);
            let svg = chart_svg(&hidden, 600., 350.);
            let tags: Vec<_> = svg.split('<').filter(|tag| tag.starts_with("text ") && tag.contains(&target)).collect();
            assert!(!tags.is_empty(), "{svg}");
            assert!(tags.iter().all(|tag| tag.contains("visibility=\"hidden\"")), "{svg}");
            assert!(svg.contains("<line "), "gridlines remain visible: {svg}");
        }
        let mut formatted = charts[0].clone();
        let value_axis = formatted.axes.iter_mut().find(|axis| axis.kind == "valAx").unwrap();
        let value_axis_id = value_axis.id;
        value_axis.number_format = Some("0.00%".into());
        assert_eq!(axis_label(&formatted, value_axis_id, 0.125, 0.1), "12.50%");
        let svg = chart_svg(&formatted, 600., 350.);
        assert!(svg.split('<').any(|tag| tag.starts_with("text ") && tag.contains("data-dnacalc=\"chart-axis-tick\"") && tag.contains('%')), "{svg}");
        let mut grid_chart = charts[0].clone();
        grid_chart.axes.iter_mut().find(|axis| axis.kind == "valAx").unwrap().major_gridline_color = Some("FF0000".into());
        let grid_svg = chart_svg(&grid_chart, 600., 350.);
        assert!(grid_svg.split('<').any(|tag| tag.starts_with("line ") && tag.contains("stroke=\"#FF0000\"")), "{grid_svg}");
        let mut colored_axis = charts[0].clone();
        colored_axis.axes.iter_mut().find(|axis| axis.kind == "catAx").unwrap().line_color = Some("FF0000".into());
        let colored_svg = chart_svg(&colored_axis, 600., 350.);
        assert!(colored_svg.split('<').any(|tag| tag.starts_with("line ") && tag.contains("stroke=\"#FF0000\"")), "{colored_svg}");
        let mut no_axis_line = charts[0].clone();
        let category = no_axis_line.axes.iter_mut().find(|axis| axis.kind == "catAx").unwrap();
        category.line_hidden = true;
        let axis_lines = |svg: &str| svg.split('<').filter(|tag| tag.starts_with("line ")
            && tag.contains(&format!("stroke=\"{AXIS}\""))).count();
        assert_eq!(axis_lines(&chart_svg(&charts[0], 600., 350.)), 1);
        let svg = chart_svg(&no_axis_line, 600., 350.);
        assert_eq!(axis_lines(&svg), 0);
        assert!(svg.contains(&format!("stroke=\"{GRID}\"")));
        let mut labelled = charts[0].clone();
        labelled.plots[0].series[0].value_labels = vec![Some("12 & units".into())];
        let svg = chart_svg(&labelled, 600., 350.);
        assert!(svg.contains("data-dnacalc=\"chart-value-label\""));
        assert!(svg.contains("12 &amp; units"));
        let mut line = charts[2].clone();
        line.plots[0].series[0].value_labels = vec![Some("20.00".into())];
        let svg = chart_svg(&line, 600., 350.);
        assert!(svg.contains("data-dnacalc=\"chart-value-label\""));
        assert!(svg.contains("20.00"));
        let mut cached = charts[0].clone();
        cached.plots[0].series[0].cached = true;
        assert!(chart_svg(&cached, 600., 350.).contains("Includes saved chart values"));
        cached.plots.push(cached.plots[0].clone());
        let svg = chart_svg(&cached, 600., 350.);
        assert!(svg.contains("Combination chart preserved"));
        assert!(!svg.contains("data-point="));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn logarithmic_live_line_has_equal_decade_spacing_and_real_gaps() {
        use calc_host::{
            HostOptions, HostSession, MemorySource, OpenPolicy, SkinIntent, WorkbookCommand,
        };
        use std::sync::Arc;
        let mut book = HostSession::open(
            Arc::new(MemorySource::new(calc_host::blank_package())),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        let sheet = book.host().sheets()[0].id();
        for (row, value) in [
            (1, "1"),
            (2, "10"),
            (3, "100"),
            (4, "0"),
            (5, "-2"),
            (6, "10"),
        ] {
            assert!(
                book.dispatch(SkinIntent::EnterCell {
                    at: calc_host::value::CellKey::new(sheet, row, 1).unwrap(),
                    text: value.into()
                })
                .accepted()
            );
        }
        let area = calc_host::value::GridArea::new(
            calc_host::value::CellKey::new(sheet, 1, 1).unwrap(),
            calc_host::value::CellKey::new(sheet, 6, 1).unwrap(),
        )
        .unwrap();
        let command = WorkbookCommand::ChartCreate {
            revision: book.host().revision().get(),
            area,
            kind: "line".into(),
            rows: false,
            headers: false,
            categories: false,
        };
        assert!(book.dispatch(SkinIntent::Command(command)).accepted());
        let command = WorkbookCommand::ChartLogAxis {
            sheet,
            revision: book.host().revision().get(),
            part: book.host().charts(sheet).0[0].part.clone(),
            axis: 2,
            base: Some("10".into()),
        };
        assert!(book.dispatch(SkinIntent::Command(command)).accepted());
        let svg = chart_svg(&book.host().charts(sheet).0[0], 600., 350.);
        assert!(!svg.contains("chart-notice"), "{svg}");
        assert!(!svg.contains("NaN"));
        assert!(!svg.contains("inf"));
        assert_eq!(svg.matches("data-dnacalc=\"chart-mark\"").count(), 4);
        assert!(!svg.contains("data-point=\"3\""));
        assert!(!svg.contains("data-point=\"4\""));
        let ys = svg
            .split("data-dnacalc=\"chart-mark\"")
            .skip(1)
            .map(|s| {
                s.split("cy=\"")
                    .nth(1)
                    .unwrap()
                    .split('"')
                    .next()
                    .unwrap()
                    .parse::<f64>()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert!(
            (ys[0] + ys[2] - 2. * ys[1]).abs() < 0.2,
            "Equal ratios must occupy equal distances: {ys:?}"
        );
        assert!((ys[1] - ys[3]).abs() < 0.2);
        let line = svg
            .split("<path d=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        assert_eq!(
            line.matches('M').count(),
            2,
            "Zero and negative points break the path: {line}"
        );
        let chart = &book.host().charts(sheet).0[0];
        let overlay = chart_comparison_svg(chart, chart, 600., 350.).unwrap();
        assert!(overlay.contains("data-dnacalc=\"chart-baseline\" stroke-dasharray=\"1 5\""));
        assert_eq!(overlay.matches("data-dnacalc=\"chart-mark\"").count(), 4);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn column_line_combination_aligns_categories_and_keeps_independent_value_axes() {
        use calc_host::{
            HostOptions, HostSession, MemorySource, OpenPolicy, SkinIntent, WorkbookCommand,
        };
        use std::sync::Arc;
        let mut book = HostSession::open(
            Arc::new(MemorySource::new(calc_host::blank_package())),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        let sheet = book.host().sheets()[0].id();
        for row in 1..=3 {
            for (column, value) in [
                (1, format!("Month {row}")),
                (2, row.to_string()),
                (3, (row * 1000).to_string()),
            ] {
                assert!(
                    book.dispatch(SkinIntent::EnterCell {
                        at: calc_host::value::CellKey::new(sheet, row, column).unwrap(),
                        text: value
                    })
                    .accepted()
                );
            }
        }
        let area = calc_host::value::GridArea::new(
            calc_host::value::CellKey::new(sheet, 1, 1).unwrap(),
            calc_host::value::CellKey::new(sheet, 3, 3).unwrap(),
        )
        .unwrap();
        let command = WorkbookCommand::ChartCreate {
            revision: book.host().revision().get(),
            area,
            kind: "combo".into(),
            rows: false,
            headers: false,
            categories: true,
        };
        assert!(book.dispatch(SkinIntent::Command(command)).accepted());
        let before = book.host().charts(sheet).0[0].clone();
        let svg = chart_svg(&before, 640., 360.);
        assert!(!svg.contains("chart-notice"), "{svg}");
        assert_eq!(svg.matches("data-dnacalc=\"chart-mark\"").count(), 6);
        assert_eq!(svg.matches("data-dnacalc=\"chart-legend\"").count(), 2);
        let attribute = |tag: &str, name: &str| {
            tag.split(&format!(" {name}=\""))
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .parse::<f64>()
                .unwrap()
        };
        let mark = |series: usize, point: usize| {
            svg.split('<')
                .find(|t| {
                    t.contains("data-dnacalc=\"chart-mark\"")
                        && t.contains(&format!("data-series=\"{series}\""))
                        && t.contains(&format!("data-point=\"{point}\""))
                })
                .unwrap()
        };
        for point in 0..3 {
            let bar = mark(0, point);
            let line = mark(1, point);
            assert!(
                (attribute(bar, "x") + attribute(bar, "width") / 2. - attribute(line, "cx")).abs()
                    < 0.2,
                "Shared category centre: {bar}; {line}"
            );
            assert!(
                (attribute(bar, "y") - attribute(line, "cy")).abs() < 0.2,
                "Proportional data on independent scales: {bar}; {line}"
            );
        }
        let command = WorkbookCommand::ChartAxis {
            sheet,
            revision: book.host().revision().get(),
            part: before.part.clone(),
            axis: 4,
            minimum: None,
            maximum: Some("10000".into()),
            major_unit: Some("2000".into()),
            reversed: false,
        };
        assert!(book.dispatch(SkinIntent::Command(command)).accepted());
        let after = book.host().charts(sheet).0[0].clone();
        let after_svg = chart_svg(&after, 640., 360.);
        let next_bar = after_svg
            .split('<')
            .find(|t| {
                t.contains("data-dnacalc=\"chart-mark\"")
                    && t.contains("data-series=\"0\"")
                    && t.contains("data-point=\"0\"")
            })
            .unwrap();
        assert_eq!(
            attribute(mark(0, 0), "y"),
            attribute(next_bar, "y"),
            "Secondary bounds keep the primary value position"
        );
        assert_eq!(
            attribute(mark(0, 0), "height"),
            attribute(next_bar, "height")
        );
        let mut orphan = before.clone();
        orphan.plots.truncate(1);
        assert!(!calc_host::chart::axes_associated(&orphan));
        assert!(calc_host::chart::category_scale(&orphan, &orphan.plots[0]).is_err());
        let notice = chart_svg(&orphan, 640., 360.);
        assert!(notice.contains("chart-notice"));
        assert!(!notice.contains("data-point="));
        let mut shared = before.clone();
        shared.axes.truncate(2);
        shared.plots[1].axis_ids = shared.plots[0].axis_ids.clone();
        assert!(calc_host::chart::combination_supported(&shared));
        let a = calc_host::chart::category_scale(&shared, &shared.plots[0]).unwrap();
        let b = calc_host::chart::category_scale(&shared, &shared.plots[1]).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.maximum, 3500.);
        assert!(!chart_svg(&shared, 640., 360.).contains("chart-notice"));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn authored_legend_positions_move_the_plot_and_keep_all_marks() {
        use calc_host::{
            HostOptions, HostSession, MemorySource, OpenPolicy, SkinIntent, WorkbookCommand,
        };
        use std::sync::Arc;
        let bytes = include_bytes!("../../../crates/calc-host/tests/fixtures/dc_6yw/charts.xlsx");
        let mut book = HostSession::open(
            Arc::new(MemorySource::new(bytes.to_vec())),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        assert!(book.dispatch(SkinIntent::Recalculate).accepted());
        let sheet = book.host().sheets()[0].id();
        let chart = book.host().charts(sheet).0[0].clone();
        let attribute = |tag: &str, name: &str| {
            tag.split(&format!(" {name}=\""))
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .parse::<f64>()
                .unwrap()
        };
        let mut plots = std::collections::BTreeMap::new();
        for position in ["r", "l", "t", "b", "tr"] {
            assert!(
                book.dispatch(SkinIntent::Command(WorkbookCommand::ChartLegend {
                    sheet,
                    revision: book.host().revision().get(),
                    part: chart.part.clone(),
                    position: position.into()
                }))
                .accepted()
            );
            let svg = chart_svg(&book.host().charts(sheet).0[0], 640., 360.);
            assert_eq!(svg.matches("data-dnacalc=\"chart-mark\"").count(), 12);
            assert_eq!(
                svg.matches(&format!("data-position=\"{position}\""))
                    .count(),
                2
            );
            let tag = svg
                .split('<')
                .find(|t| t.contains("data-dnacalc=\"chart-plot\""))
                .unwrap();
            plots.insert(
                position,
                (
                    attribute(tag, "x"),
                    attribute(tag, "y"),
                    attribute(tag, "width"),
                    attribute(tag, "height"),
                ),
            );
        }
        assert!(plots["l"].0 > plots["r"].0);
        assert!(plots["t"].1 > plots["b"].1);
        assert!(plots["t"].2 > plots["r"].2);
        assert!(plots["b"].3 < plots["r"].3);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn crowded_legends_keep_labels_and_clip_every_visual_entry_to_the_frame() {
        use calc_host::{HostOptions, HostSession, MemorySource, OpenPolicy, SkinIntent};
        use std::sync::Arc;
        let mut book = HostSession::open(
            Arc::new(MemorySource::new(
                include_bytes!("../../../crates/calc-host/tests/fixtures/dc_6yw/charts.xlsx")
                    .to_vec(),
            )),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        assert!(book.dispatch(SkinIntent::Recalculate).accepted());
        let mut chart = book.host().charts(book.host().sheets()[0].id()).0[0].clone();
        let base = chart.plots[0].series[0].clone();
        for (w, h, n) in [
            (640., 360., 25),
            (300., 180., 25),
            (200., 50., 25),
            (100., 60., 255),
        ] {
            chart.plots[0].series = (0..n)
                .map(|i| {
                    let mut s = base.clone();
                    s.name = format!("Region {i}: a very long & unabridged series label");
                    s
                })
                .collect();
            for position in ["r", "l", "t", "b", "tr"] {
                chart.legend = Some(position.into());
                let svg = chart_svg(&chart, w, h);
                assert!(svg.contains(&escape(&format!(
                    "Region {}: a very long & unabridged series label",
                    n - 1
                ))));
                for tag in svg.split('<').filter(|t| t.starts_with("svg x=")) {
                    let a = |name: &str| {
                        tag.split(&format!("{name}=\""))
                            .nth(1)
                            .unwrap()
                            .split('"')
                            .next()
                            .unwrap()
                            .parse::<f64>()
                            .unwrap()
                    };
                    let (x, y, width, height) = (a("x"), a("y"), a("width"), a("height"));
                    assert!(
                        x >= 0. && y >= 0. && x + width <= w + 0.11 && y + height <= h + 0.11,
                        "{w}x{h} {position}: {tag}"
                    );
                }
                if w == 640. {
                    assert_eq!(svg.matches("data-dnacalc=\"chart-legend\"").count(), 25);
                    assert!(!svg.contains("chart-legend-overflow"));
                }
            }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn chart_text_colour_does_not_recolour_marks_and_legend_rows_grow_with_font() {
        use calc_host::{HostOptions, HostSession, MemorySource, OpenPolicy, SkinIntent};
        use std::sync::Arc;
        let mut book = HostSession::open(
            Arc::new(MemorySource::new(
                include_bytes!("../../../crates/calc-host/tests/fixtures/dc_6yw/charts.xlsx")
                    .to_vec(),
            )),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        assert!(book.dispatch(SkinIntent::Recalculate).accepted());
        let mut chart = book.host().charts(book.host().sheets()[0].id()).0[0].clone();
        for series in &mut chart.plots[0].series {
            series.color = "595959".into();
            series.explicit_color = true;
        }
        chart.text_style = calc_host::chart::ChartTextStyle {
            family: Some("A & <Family>".into()),
            size: Some(1800),
            bold: Some(true),
            italic: Some(true),
            color: Some("FF0000".into()),
        };
        for position in ["r", "l", "t", "b", "tr"] {
            chart.legend = Some(position.into());
            let svg = chart_svg(&chart, 640., 360.);
            assert!(
                svg.contains("font-size=\"24.00\"")
                    && svg.contains("font-weight=\"700\"")
                    && svg.contains("font-style=\"italic\"")
            );
            assert!(svg.contains("A &amp; &lt;Family&gt;"));
            for tag in svg
                .split('<')
                .filter(|t| t.contains("data-dnacalc=\"chart-mark\""))
            {
                assert!(tag.contains("fill=\"#595959\""), "{tag}");
            }
            assert!(
                svg.split('<')
                    .filter(|t| t.starts_with("text "))
                    .all(|t| !t.contains("fill=\"#595959\""))
            );
            for tag in svg.split('<').filter(|t| t.starts_with("svg x=")) {
                let height = tag
                    .split("height=\"")
                    .nth(1)
                    .unwrap()
                    .split('"')
                    .next()
                    .unwrap()
                    .parse::<f64>()
                    .unwrap();
                assert!(height >= 36., "{tag}");
            }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn reversed_category_combinations_align_each_series_at_the_same_category() {
        use calc_host::{
            HostOptions, HostSession, MemorySource, OpenPolicy, SkinIntent, WorkbookCommand,
        };
        use std::sync::Arc;
        for kind in [
            "combo",
            "combo-column-area",
            "combo-line-column",
            "combo-line-area",
            "combo-area-column",
            "combo-area-line",
        ] {
            let mut book = HostSession::open(
                Arc::new(MemorySource::new(
                    include_bytes!("../../../crates/calc-host/tests/fixtures/dc_6yw/charts.xlsx")
                        .to_vec(),
                )),
                &OpenPolicy::default(),
                HostOptions::default(),
            )
            .unwrap();
            assert!(book.dispatch(SkinIntent::Recalculate).accepted());
            let sheet = book.host().sheets()[0].id();
            let area = calc_host::value::GridArea::new(
                calc_host::value::CellKey::new(sheet, 1, 1).unwrap(),
                calc_host::value::CellKey::new(sheet, 7, 3).unwrap(),
            )
            .unwrap();
            let command = WorkbookCommand::ChartCreate {
                revision: book.host().revision().get(),
                area,
                kind: kind.into(),
                rows: false,
                headers: true,
                categories: true,
            };
            assert!(book.dispatch(SkinIntent::Command(command)).accepted());
            let chart = book.host().charts(sheet).0.pop().unwrap();
            let svg = chart_svg(&chart, 640., 360.);
            if kind == "combo-area-line" {
                let mut shared = chart.clone();
                shared.axes.truncate(2);
                shared.plots[1].axis_ids = shared.plots[0].axis_ids.clone();
                for axis in &mut shared.axes {
                    if axis.kind == "valAx" {
                        axis.cross_between = None;
                    }
                }
                let svg = chart_svg(&shared, 640., 360.);
                assert!(!svg.contains("chart-notice"));
                for point in 0..6 {
                    let xs = svg
                        .split('<')
                        .filter(|tag| {
                            tag.contains("data-dnacalc=\"chart-mark\"")
                                && tag.contains(&format!("data-point=\"{point}\""))
                        })
                        .map(|tag| {
                            let attr = |key: &str| {
                                tag.split(&format!(" {key}=\""))
                                    .nth(1)
                                    .unwrap()
                                    .split('"')
                                    .next()
                                    .unwrap()
                                    .parse::<f64>()
                                    .unwrap()
                            };
                            if tag.starts_with("rect ") {
                                attr("x") + attr("width") / 2.
                            } else {
                                attr("cx")
                            }
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(xs.len(), 2);
                    assert!(
                        (xs[0] - xs[1]).abs() < 0.11,
                        "absent crossBetween with an area and a line crosses between categories: {xs:?}"
                    );
                }
            }
            if chart.plots.len() > 1 {
                let rank = |k: &ChartKind| match k {
                    ChartKind::Area => 0,
                    ChartKind::Column | ChartKind::Bar => 1,
                    _ => 2,
                };
                let mut seen: Vec<(usize, usize)> = (0..chart.plots.len())
                    .filter_map(|i| {
                        svg.find(&format!("<g data-plot=\"{i}\">"))
                            .map(|at| (at, i))
                    })
                    .collect();
                seen.sort();
                let ranks = seen
                    .iter()
                    .map(|&(_, i)| rank(&chart.plots[i].kind))
                    .collect::<Vec<_>>();
                assert!(
                    ranks.windows(2).all(|w| w[0] <= w[1]),
                    "{kind}: area under column under line, got {ranks:?}"
                );
            }

            for point in 0..6 {
                let positions = svg
                    .split('<')
                    .filter(|tag| {
                        tag.contains("data-dnacalc=\"chart-mark\"")
                            && tag.contains(&format!("data-point=\"{point}\""))
                    })
                    .map(|tag| {
                        let attr = |key: &str| {
                            tag.split(&format!(" {key}=\""))
                                .nth(1)
                                .unwrap()
                                .split('"')
                                .next()
                                .unwrap()
                                .parse::<f64>()
                                .unwrap()
                        };
                        if tag.starts_with("rect ") {
                            attr("x") + attr("width") / 2.
                        } else {
                            attr("cx")
                        }
                    })
                    .collect::<Vec<_>>();
                assert_eq!(positions.len(), 2, "{kind}: {svg}");
                assert!(
                    (positions[0] - positions[1]).abs() < 0.11,
                    "{kind}, point {point}: {positions:?}"
                );
            }
        }
        let book = HostSession::open(
            Arc::new(MemorySource::new(
                include_bytes!("../../../crates/calc-host/tests/fixtures/dc_6yw/charts.xlsx")
                    .to_vec(),
            )),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        let sheet = book.host().sheets()[0].id();
        for index in [0, 1] {
            let mut chart = book.host().charts(sheet).0[index].clone();
            for axis in &mut chart.axes {
                if axis.kind == "valAx" {
                    axis.cross_between = Some("midCat".into());
                }
            }
            let svg = chart_svg(&chart, 640., 360.);
            assert!(svg.contains("Midpoint category crossing"));
            assert!(!svg.contains("data-dnacalc=\"chart-mark\""));
        }
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

fn axis_notice(svg: &mut String, area: &Area, message: &str) {
    let _ = write!(
        svg,
        "<text data-dnacalc=\"chart-notice\" x=\"{}\" y=\"{}\" text-anchor=\"middle\" fill=\"{AXIS}\">{}</text>",
        (area.left + area.right) / 2.,
        (area.top + area.bottom) / 2.,
        escape(message)
    );
}

#[cfg(test)]
mod ui29_chart_tests {
    use super::*;
    use calc_host::{
        HostOptions, HostSession, MemorySource, OpenPolicy, SkinIntent, WorkbookCommand,
    };
    use std::sync::Arc;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn chart_three_plot_stacks_share_primary_extents_and_draw_each_axis_once() {
        let mut book = HostSession::open(
            Arc::new(MemorySource::new(calc_host::blank_package())),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        let sheet = book.host().sheets()[0].id();
        for (row, values) in [
            ["Region", "First", "Second", "Line", "Area"],
            ["East", "-2", "-5", "100", "8"],
            ["West", "3", "6", "200", "9"],
            ["North", "4", "7", "300", "10"],
        ]
        .into_iter()
        .enumerate()
        {
            for (col, text) in values.into_iter().enumerate() {
                assert!(
                    book.dispatch(SkinIntent::EnterCell {
                        at: calc_host::value::CellKey::new(sheet, row as u32 + 1, col as u32 + 1)
                            .unwrap(),
                        text: text.into()
                    })
                    .accepted()
                );
            }
        }
        let area = calc_host::value::GridArea::new(
            calc_host::value::CellKey::new(sheet, 1, 1).unwrap(),
            calc_host::value::CellKey::new(sheet, 4, 5).unwrap(),
        )
        .unwrap();
        let result = book.dispatch(SkinIntent::Command(WorkbookCommand::ChartCreate {
            revision: book.host().revision().get(),
            area,
            kind: "combo-three".into(),
            rows: false,
            headers: true,
            categories: true,
        }));
        assert!(result.accepted(), "{result:?}");
        let part = book.host().charts(sheet).0[0].part.clone();
        let result = book.dispatch(SkinIntent::Command(WorkbookCommand::ChartType {
            sheet,
            revision: book.host().revision().get(),
            part,
            plot: 0,
            kind: "column".into(),
            stacked: true,
        }));
        assert!(result.accepted(), "{result:?}");
        let chart = book.host().charts(sheet).0[0].clone();
        let mut titled = chart.clone();
        titled.title = Some("Chart title".into());
        titled.text_style.bold = Some(false);
        let axis = titled.axes.iter_mut().find(|axis| axis.position.as_deref() == Some("l")).unwrap();
        axis.title = Some("Revenue & costs".into());
        let key = format!("axis-title:{}", axis.id);
        titled.element_text.entry(key).or_default().family = Some("Arial".into());
        let titled_svg = chart_svg(&titled, 640., 360.);
        assert!(titled_svg.contains("data-dnacalc=\"chart-axis-title\""));
        assert!(titled_svg.contains("Revenue &amp; costs"));
        assert!(titled_svg.contains("&quot;Arial&quot;"));
        let title_tag = titled_svg.split("data-dnacalc=\"chart-title\"").nth(1).unwrap().split('>').next().unwrap();
        assert!(title_tag.contains("font-weight=\"400\""));
        let svg = chart_svg(&chart, 640., 360.);
        assert!(!svg.contains("chart-notice"), "{svg}");
        assert_eq!(svg.matches("data-dnacalc=\"chart-mark\"").count(), 12);
        for series in 0..4 {
            assert_eq!(
                svg.split('<')
                    .filter(|tag| tag.contains("data-dnacalc=\"chart-mark\"")
                        && tag.contains(&format!("data-series=\"{series}\"")))
                    .count(),
                3
            );
        }
        let ticks = calc_host::chart::category_scale(&chart, &chart.plots[0])
            .unwrap()
            .ticks()
            .into_iter()
            .count()
            + calc_host::chart::category_scale(&chart, &chart.plots[1])
                .unwrap()
                .ticks()
                .into_iter()
                .count();
        assert_eq!(
            svg.matches("data-dnacalc=\"chart-axis-tick\"").count(),
            ticks
        );
        let third = svg
            .split("<g data-plot=\"2\">")
            .nth(1)
            .unwrap()
            .split("</g>")
            .next()
            .unwrap();
        assert!(!third.contains("chart-axis-tick"));
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn chart_element_fonts_render_on_only_their_selected_text_and_preserve_marks() {
        let mut book = HostSession::open(
            Arc::new(MemorySource::new(
                include_bytes!("../../../crates/calc-host/tests/fixtures/dc_6yw/charts.xlsx")
                    .to_vec(),
            )),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        assert!(book.dispatch(SkinIntent::Recalculate).accepted());
        let sheet = book.host().sheets()[0].id();
        let chart = book.host().charts(sheet).0[0].clone();
        let original = chart_svg(&chart, 640., 360.);
        for (target, size, color) in [
            ("title", 1800, "D02030"),
            ("legend", 1300, "209040"),
            ("axis:1264926064", 900, "2030C0"),
            ("axis:1264925584", 1100, "C06010"),
        ] {
            let result = book.dispatch(SkinIntent::Command(
                WorkbookCommand::ChartElementTextStyle {
                    sheet,
                    revision: book.host().revision().get(),
                    part: chart.part.clone(),
                    target: target.into(),
                    style: calc_host::chart::ChartTextStyle {
                        family: Some("Arial".into()),
                        size: Some(size),
                        color: Some(color.into()),
                        ..Default::default()
                    },
                },
            ));
            assert!(result.accepted(), "{result:?}");
        }
        let svg = chart_svg(&book.host().charts(sheet).0[0], 640., 360.);
        for (target, size, color) in [
            ("title", "24.00", "D02030"),
            ("legend", "17.33", "209040"),
            ("axis:1264926064", "12.00", "2030C0"),
            ("axis:1264925584", "14.67", "C06010"),
        ] {
            let tags = svg
                .split('<')
                .filter(|t| {
                    t.starts_with("text ") && t.contains(&format!("data-chart-text=\"{target}\""))
                })
                .collect::<Vec<_>>();
            assert!(!tags.is_empty(), "{target}: {svg}");
            for tag in tags {
                assert!(
                    tag.contains(&format!("font-size=\"{size}\""))
                        && tag.contains(&format!("fill=\"#{color}\""))
                        && tag.contains("Arial"),
                    "{tag}"
                );
            }
        }
        let attributes = |drawing: &str| {
            drawing
                .split('<')
                .filter(|t| t.contains("data-dnacalc=\"chart-mark\""))
                .map(|tag| {
                    ["data-series", "data-point", "data-value", "fill"].map(|key| {
                        tag.split(&format!(" {key}=\""))
                            .nth(1)
                            .unwrap()
                            .split('"')
                            .next()
                            .unwrap()
                            .to_owned()
                    })
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(attributes(&original), attributes(&svg));
        let result = book.dispatch(SkinIntent::Command(
            WorkbookCommand::ChartElementTextStyle {
                sheet,
                revision: book.host().revision().get(),
                part: chart.part,
                target: "title".into(),
                style: Default::default(),
            },
        ));
        assert!(result.accepted());
        let svg = chart_svg(&book.host().charts(sheet).0[0], 640., 360.);
        let title = svg
            .split('<')
            .find(|t| t.contains("data-dnacalc=\"chart-title\""))
            .unwrap();
        assert!(title.contains("font-size=\"14\"") && title.contains("fill=\"#595959\""));
        assert!(svg.contains("fill=\"#209040\"") && svg.contains("fill=\"#2030C0\""));
    }
}
