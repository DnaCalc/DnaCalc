//! Series, chart-area and plot-area fills of a macro (dc-oxh8w.19, dc-oxh8w.31): `Interior.Color` and
//! `Format.Fill.ForeColor.RGB` of a series (`WorkbookCommand::ChartSeriesStyle`, the command the chart pane's series
//! colour uses) and of the chart area and the plot area (`WorkbookCommand::ChartAreaStyle`); the retained writer saves
//! the `c:spPr` fills. Borders (`Border.Color`, `LineStyle`) are dc-oxh8w.61.

use super::report::report;
use super::*;
use crate::WorkbookCommand;
use calc_object_model::{Paint, PaintLevel, ReportItem};

/// The BGR number VBA uses for a six-digit RGB colour.
fn bgr(rgb: &str) -> Option<i32> {
    let rgb = u32::from_str_radix(rgb, 16).ok()?;
    let (r, g, b) = ((rgb >> 16) & 0xFF, (rgb >> 8) & 0xFF, rgb & 0xFF);
    Some((r | (g << 8) | (b << 16)) as i32)
}

impl WorkbookAutomation {
    pub(super) fn series_interior(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let (place, _) = self.series_at(target)?;
        let part = Self::chart_token(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        Ok(report(workbook, sheet, "Interior", ReportItem::SeriesInterior(part, place as u32)))
    }

    /// `Series.Format`, `ChartArea.Format` and `PlotArea.Format`.
    pub(super) fn paint_format(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let paint = match &target.key {
            ObjectKey::Report { item: ReportItem::ChartArea(_), .. } => Paint::Area,
            ObjectKey::Report { item: ReportItem::PlotArea(_), .. } => Paint::Plot,
            _ => Paint::Series(self.series_at(target)?.0 as u32),
        };
        self.paint_child(target, paint, PaintLevel::Format, "ChartFormat")
    }

    /// `ChartArea.Interior` and `PlotArea.Interior`.
    pub(super) fn area_interior(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let paint = match &target.key {
            ObjectKey::Report { item: ReportItem::PlotArea(_), .. } => Paint::Plot,
            _ => Paint::Area,
        };
        self.paint_child(target, paint, PaintLevel::Interior, "Interior")
    }

    fn paint_child(&self, target: &ObjectRef, paint: Paint, level: PaintLevel, interface: &'static str) -> Reply {
        let part = Self::chart_token(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        Ok(report(workbook, sheet, interface, ReportItem::ChartPaint(part, paint, level)))
    }

    /// `ChartFormat.Fill` and `ChartFillFormat.ForeColor`: one level further down the same paint.
    pub(super) fn paint_next(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        match &target.key {
            ObjectKey::Report { item: ReportItem::ChartPaint(_, paint, PaintLevel::Format), .. } => {
                self.paint_child(target, *paint, PaintLevel::Fill, "ChartFillFormat")
            }
            ObjectKey::Report { item: ReportItem::ChartPaint(_, paint, PaintLevel::Fill), .. } => {
                self.paint_child(target, *paint, PaintLevel::ForeColor, "ColorFormat")
            }
            _ => Err(geometry::type_mismatch()),
        }
    }

    /// `Chart.ChartArea` and `Chart.PlotArea`.
    pub(super) fn chart_area(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let part = Self::chart_token(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        Ok(report(workbook, sheet, "ChartArea", ReportItem::ChartArea(part)))
    }
    pub(super) fn chart_plot_area(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let part = Self::chart_token(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        Ok(report(workbook, sheet, "PlotArea", ReportItem::PlotArea(part)))
    }

    /// What an `Interior` or `ColorFormat` object of a chart paints: a series, the chart area or the plot area.
    pub(super) fn paint_of(target: &ObjectRef) -> Option<Paint> {
        match &target.key {
            ObjectKey::Report { item: ReportItem::SeriesInterior(_, place), .. } => Some(Paint::Series(*place)),
            ObjectKey::Report { item: ReportItem::ChartPaint(_, paint, PaintLevel::Interior | PaintLevel::ForeColor), .. } => {
                Some(*paint)
            }
            _ => None,
        }
    }

    /// The colour a chart paint is drawn in now, as the BGR number VBA uses: a series' drawn colour; the chart area's and
    /// the plot area's stated fill, else white (Excel's answer for an unstated one is asked in RUN-ON-WINDOWS).
    pub(super) fn paint_color(&self, target: &ObjectRef, call: &Call<'_>, paint: Paint) -> Reply {
        no_args(call)?;
        let part = Self::chart_token(target)?;
        let color = match paint {
            Paint::Series(place) => self.read_host(target, |s, sheet, _| {
                let view = s.host().charts(sheet).0.into_iter().find(|c| c.part == part);
                Ok(view.and_then(|c| c.plots.into_iter().flat_map(|p| p.series).nth(place as usize)).map(|s| s.color))
            })?,
            Paint::Area => self.real_chart(target, |c| Some(c.area_fill.clone().unwrap_or_else(|| "FFFFFF".into())))?,
            Paint::Plot => self.real_chart(target, |c| Some(c.plot_fill.clone().unwrap_or_else(|| "FFFFFF".into())))?,
        };
        let value = color.and_then(|c| bgr(&c)).ok_or_else(|| AutoError::excel(424, "Object required"))?;
        Ok(AutoValue::I4(value))
    }

    /// `Interior.Color` of a series: the colour the chart draws it in, as the BGR number VBA uses.
    pub(super) fn interior_color(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if matches!(target.key, ObjectKey::RangeFormat(_)) {
            return self.range_interior_get(target, call);
        }
        match Self::paint_of(target) {
            // A cell range's interior is dc-oxh8w.2's; its handler takes this member over for ranges.
            None => Err(AutoError::not_modelled("Range.Interior.Color", "dc-oxh8w.2")),
            Some(paint) => self.paint_color(target, call, paint),
        }
    }

    pub(super) fn interior_put_color(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if matches!(target.key, ObjectKey::RangeFormat(_)) {
            return self.range_interior_put(target, call);
        }
        let paint = Self::paint_of(target).ok_or_else(|| AutoError::not_modelled("Range.Interior.Color", "dc-oxh8w.2"))?;
        self.paint_put_color(target, call, paint)
    }

    /// Put a BGR colour on a chart paint; out of range or fractional is 1004.
    pub(super) fn paint_put_color(&self, target: &ObjectRef, call: &Call<'_>, paint: Paint) -> Reply {
        let value = match &call.args[..] {
            [v] => v,
            _ => return Err(geometry::bad_count()),
        };
        let number = super::report_draw::number(value)?;
        if !number.is_finite() || number.fract() != 0.0 || !(0.0..=16_777_215.0).contains(&number) {
            return Err(AutoError::excel(1004, "Unable to set the Color property of the Interior class"));
        }
        let bgr = number as u32;
        let fill = format!("{:02X}{:02X}{:02X}", bgr & 0xFF, (bgr >> 8) & 0xFF, (bgr >> 16) & 0xFF);
        let (_, sheet, _) = Self::report_key(target)?;
        let part = Self::chart_token(target)?;
        match paint {
            Paint::Series(_) => {
                let (plot, series) = self.series_coords(target)?;
                self.command(call, |s| {
                    Ok(WorkbookCommand::ChartSeriesStyle {
                        sheet,
                        revision: s.host().revision().get(),
                        part: part.clone(),
                        plot,
                        series,
                        fill: Some(fill.clone()),
                    })
                })
            }
            Paint::Area | Paint::Plot => self.command(call, |s| {
                Ok(WorkbookCommand::ChartAreaStyle {
                    sheet,
                    revision: s.host().revision().get(),
                    part: part.clone(),
                    plot_area: paint == Paint::Plot,
                    fill: Some(fill.clone()),
                })
            }),
        }
    }
}
