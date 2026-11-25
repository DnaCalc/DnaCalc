//! Series formatting of a macro (dc-oxh8w.19): `Series.Interior.Color` over `WorkbookCommand::ChartSeriesStyle`,
//! the command the chart pane's series colour uses; the retained writer saves the series `c:spPr` fill.
//! Plot area, chart area, `Format.Fill` and borders need types the descriptor lacks (dc-oxh8w.29).

use super::report::report;
use super::*;
use crate::WorkbookCommand;
use calc_object_model::ReportItem;

impl WorkbookAutomation {
    pub(super) fn series_interior(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let (place, _) = self.series_at(target)?;
        let part = Self::chart_token(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        Ok(report(workbook, sheet, "Interior", ReportItem::SeriesInterior(part, place as u32)))
    }

    fn interior_place(target: &ObjectRef) -> Result<usize, AutoError> {
        match &target.key {
            ObjectKey::Report { item: ReportItem::SeriesInterior(_, place), .. } => Ok(*place as usize),
            // A cell range's interior is dc-oxh8w.2's; its handler takes this member over for ranges.
            _ => Err(AutoError::not_modelled("Range.Interior.Color", "dc-oxh8w.2")),
        }
    }

    /// `Interior.Color` of a series: the colour the chart draws it in, as the BGR number VBA uses.
    pub(super) fn interior_color(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let place = Self::interior_place(target)?;
        no_args(call)?;
        let part = Self::chart_token(target)?;
        let color = self.read_host(target, |s, sheet, _| {
            let view = s.host().charts(sheet).0.into_iter().find(|c| c.part == part);
            Ok(view.and_then(|c| c.plots.into_iter().flat_map(|p| p.series).nth(place)).map(|s| s.color))
        })?;
        let rgb = color
            .and_then(|c| u32::from_str_radix(&c, 16).ok())
            .ok_or_else(|| AutoError::excel(424, "Object required"))?;
        let (r, g, b) = ((rgb >> 16) & 0xFF, (rgb >> 8) & 0xFF, rgb & 0xFF);
        Ok(AutoValue::I4((r | (g << 8) | (b << 16)) as i32))
    }

    pub(super) fn interior_put_color(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        Self::interior_place(target)?;
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
}
