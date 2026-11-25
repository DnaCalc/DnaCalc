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
        if matches!(target.key, ObjectKey::RangeFormat(_) | ObjectKey::Style { .. })
            || super::formatting::is_condition_part(target)
        {
            return self.range_interior_get(target, call);
        }
        match Self::paint_of(target) {
            // A cell range's interior is dc-oxh8w.2's; its handler takes this member over for ranges.
            None => Err(AutoError::not_modelled("Range.Interior.Color", "dc-oxh8w.2")),
            Some(paint) => self.paint_color(target, call, paint),
        }
    }

    pub(super) fn interior_put_color(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if matches!(target.key, ObjectKey::RangeFormat(_) | ObjectKey::Style { .. })
            || super::formatting::is_condition_part(target)
        {
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

/// `XlLineStyle` values a chart outline takes, with the `a:prstDash` each draws as. `None` is no outline; the dash
/// presets are an assumption until Excel's answers are recorded (dc-oxh8w.67).
const LINE_STYLES: [(i32, Option<&str>); 6] = [
    (1, Some("solid")),
    (-4115, Some("dash")),
    (-4118, Some("sysDot")),
    (4, Some("dashDot")),
    (5, Some("lgDashDotDot")),
    (-4142, None),
];
/// `XlBorderWeight` values with the `a:ln w` (EMU) each writes: hairline 0.25 pt, thin 0.75 pt, medium 1.5 pt, thick 2.25 pt.
const WEIGHTS: [(i32, u32); 4] = [(1, 3175), (2, 9525), (-4138, 19050), (4, 28575)];
/// What a chart outline reads as when the file states nothing: the default style's light grey, thin, continuous line.
const DEFAULT_BORDER_COLOR: &str = "D9D9D9";

impl WorkbookAutomation {
    /// `ChartArea.Border` and `PlotArea.Border`.
    pub(super) fn area_border(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let part = Self::chart_token(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        let plot = matches!(target.key, ObjectKey::Report { item: ReportItem::PlotArea(_), .. });
        Ok(report(workbook, sheet, "Border", ReportItem::ChartBorder(part, plot)))
    }

    /// `Series.Border`: the outline of the series (a bar's border, a line's line).
    pub(super) fn series_border(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let (place, _) = self.series_at(target)?;
        let part = Self::chart_token(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        Ok(report(workbook, sheet, "Border", ReportItem::SeriesBorder(part, place as u32)))
    }

    /// The outline a `Border` object of a chart denotes, as the file states it.
    fn border_state(&self, target: &ObjectRef) -> Result<calc_doc_model::ChartBorder, AutoError> {
        match &target.key {
            ObjectKey::Report { item: ReportItem::ChartBorder(_, plot), .. } => {
                let plot = *plot;
                self.real_chart(target, |c| if plot { c.plot_border.clone() } else { c.area_border.clone() })
            }
            ObjectKey::Report { item: ReportItem::SeriesBorder(..), .. } => {
                let (place, _) = self.series_at(target)?;
                self.real_chart(target, |c| c.plots.iter().flat_map(|p| p.series.iter()).nth(place).map(|s| s.line.clone()))?
                    .ok_or_else(|| AutoError::excel(424, "Object required"))
            }
            _ => Err(geometry::type_mismatch()),
        }
    }

    pub(super) fn chart_border_get(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let border = self.border_state(target)?;
        let series = matches!(target.key, ObjectKey::Report { item: ReportItem::SeriesBorder(..), .. });
        // A series that states nothing draws in Excel's automatic style, which differs by chart type: its answers wait
        // for Excel's (dc-yship).
        let unstated = |what: &str| AutoError::not_modelled(format!("{what} of a series outline the file does not state"), "dc-yship");
        match call.member.name.as_str() {
            "LineStyle" => {
                if border.none {
                    return Ok(AutoValue::I4(-4142));
                }
                if series && border == calc_doc_model::ChartBorder::default() {
                    return Err(unstated("Border.LineStyle"));
                }
                let code = border
                    .dash
                    .as_deref()
                    .map_or(Some(1), |d| LINE_STYLES.iter().find(|(_, p)| *p == Some(d)).map(|(c, _)| *c));
                code.map(AutoValue::I4).ok_or_else(|| AutoError::not_modelled("Border.LineStyle of this dash", "dc-yship"))
            }
            "Weight" => {
                if series && border.width.is_none() {
                    return Err(unstated("Border.Weight"));
                }
                let width = border.width.unwrap_or(9525);
                let pick = WEIGHTS.iter().min_by_key(|(_, w)| w.abs_diff(width)).map_or(2, |(c, _)| *c);
                Ok(AutoValue::I4(pick))
            }
            "Color" | "ColorIndex" => {
                if call.member.name == "ColorIndex" && !border.none && border.color.is_none() {
                    return Ok(AutoValue::I4(-4105));
                }
                if series && border.color.is_none() {
                    return Err(unstated("Border.Color"));
                }
                let rgb = border.color.as_deref().unwrap_or(DEFAULT_BORDER_COLOR);
                if call.member.name == "Color" {
                    return bgr(rgb).map(AutoValue::I4).ok_or_else(|| AutoError::excel(424, "Object required"));
                }
                // The palette entry nearest the colour, as the cell borders answer.
                let value = u32::from_str_radix(rgb, 16).map_err(|_| AutoError::excel(424, "Object required"))?;
                let channels = |v: u32| [(v >> 16) & 0xFF, (v >> 8) & 0xFF, v & 0xFF].map(|c| c as i64);
                let wanted = channels(value);
                let nearest = calc_func::DEFAULT_PALETTE
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, p)| {
                        let have = channels(u32::from_str_radix(p.trim_start_matches('#'), 16).unwrap_or(0));
                        (0..3).map(|i| (have[i] - wanted[i]).pow(2)).sum::<i64>()
                    })
                    .map_or(1, |(i, _)| i as i32 + 1);
                Ok(AutoValue::I4(nearest))
            }
            _ => Err(geometry::type_mismatch()),
        }
    }

    pub(super) fn chart_border_put(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let member = call.member.name.as_str();
        let [value] = &call.args[..] else {
            return Err(geometry::bad_count());
        };
        let number = super::report_draw::number(value).map_err(|_| AutoError::excel(1004, "Unable to set the property of the Border class"))?;
        let bad = || AutoError::excel(1004, format!("Unable to set the {member} property of the Border class"));
        if !number.is_finite() || number.fract() != 0.0 || number.abs() > f64::from(i32::MAX) {
            return Err(bad());
        }
        let number = number as i32;
        let mut border = self.border_state(target)?;
        match member {
            "LineStyle" => match LINE_STYLES.iter().find(|(c, _)| *c == number).ok_or_else(bad)?.1 {
                None => border.none = true,
                Some(dash) => {
                    border.none = false;
                    border.dash = Some(dash.to_owned());
                    border.color.get_or_insert_with(|| DEFAULT_BORDER_COLOR.to_owned());
                }
            },
            "Weight" => {
                border.width = Some(WEIGHTS.iter().find(|(c, _)| *c == number).ok_or_else(bad)?.1);
                if border.none {
                    border.none = false;
                    border.color.get_or_insert_with(|| DEFAULT_BORDER_COLOR.to_owned());
                }
            }
            "ColorIndex" => {
                // 1..=56 index the default palette; xlColorIndexAutomatic (-4105) is the automatic colour.
                if number == -4105 {
                    border.none = false;
                    border.color = None;
                } else {
                    let rgb = usize::try_from(number - 1).ok().and_then(|i| calc_func::DEFAULT_PALETTE.get(i)).ok_or_else(bad)?;
                    border.none = false;
                    border.color = Some(rgb.trim_start_matches('#').to_ascii_uppercase());
                }
            }
            _ => {
                if !(0..=0xFF_FF_FF).contains(&number) {
                    return Err(bad());
                }
                border.none = false;
                border.color = Some(format!("{:02X}{:02X}{:02X}", number & 0xFF, (number >> 8) & 0xFF, (number >> 16) & 0xFF));
            }
        }
        let part = Self::chart_token(target)?;
        let (_, sheet, _) = Self::report_key(target)?;
        if matches!(target.key, ObjectKey::Report { item: ReportItem::SeriesBorder(..), .. }) {
            let (plot, series) = self.series_coords(target)?;
            return self.command(call, |s| {
                Ok(WorkbookCommand::ChartSeriesBorder {
                    sheet,
                    revision: s.host().revision().get(),
                    part: part.clone(),
                    plot,
                    series,
                    border: border.clone(),
                })
            });
        }
        let plot = matches!(&target.key, ObjectKey::Report { item: ReportItem::ChartBorder(_, true), .. });
        self.command(call, |s| {
            Ok(WorkbookCommand::ChartAreaBorder {
                sheet,
                revision: s.host().revision().get(),
                part: part.clone(),
                plot_area: plot,
                border: border.clone(),
            })
        })
    }
}
