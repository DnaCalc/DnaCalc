//! Chart axes of a macro (dc-oxh8w.11): `Chart.Axes(Type)` and the scale members of `Axis`
//! (`MinimumScale`, `MaximumScale`, `MajorUnit`, their `...IsAuto` switches and `ReversePlotOrder`).
//! Writes go through `WorkbookCommand::ChartAxis`, the chart axis dialog's command, so the retained
//! chart writer saves them. Reading a scale that is automatic needs the renderer's computed scale
//! (Excel returns it) and refuses naming dc-oxh8w.13; axis titles and number formats are dc-oxh8w.12.

use super::report::report;
use super::report_draw::number;
use super::*;
use crate::WorkbookCommand;
use calc_object_model::ReportItem;

const AXES: &str = "dc-oxh8w.13";

#[derive(Clone, Copy)]
enum Scale {
    Minimum,
    Maximum,
    Unit,
}

impl Scale {
    fn name(self) -> &'static str {
        match self {
            Scale::Minimum => "MinimumScale",
            Scale::Maximum => "MaximumScale",
            Scale::Unit => "MajorUnit",
        }
    }
}

impl WorkbookAutomation {
    fn axis_key(target: &ObjectRef) -> Result<u8, AutoError> {
        match &target.key {
            ObjectKey::Report {
                item: ReportItem::Axis(_, kind) | ReportItem::AxisTitle(_, kind) | ReportItem::TickLabels(_, kind),
                ..
            } => Ok(*kind),
            _ => Err(geometry::type_mismatch()),
        }
    }

    /// `Chart.Axes(Type, AxisGroup)`: the primary category (1) or value (2) axis. Excel's error for a
    /// chart without that axis (a pie) is 1004.
    pub(super) fn chart_axes(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.arg(0).is_missing() {
            return Err(AutoError::not_modelled("Chart.Axes without a Type (the Axes collection)", AXES));
        }
        let kind = geometry::integer(call.arg(0))?;
        if !matches!(call.arg(1), AutoValue::Missing) && geometry::integer(call.arg(1))? != 1 {
            return Err(AutoError::not_modelled("Chart.Axes on the secondary axis group", AXES));
        }
        let part = Self::chart_token(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        let found = self.real_chart(target, |c| {
            let ids = c.plots.first().map(|p| p.axis_ids.clone()).unwrap_or_default();
            let place = usize::try_from(kind - 1).ok().filter(|p| *p < 2);
            place.and_then(|p| ids.get(p)).is_some_and(|id| c.axes.iter().any(|a| a.id == *id))
        })?;
        if !(1..=2).contains(&kind) {
            return Err(AutoError::not_modelled("Chart.Axes of the series axis type", AXES));
        }
        if !found {
            return Err(AutoError::excel(1004, "Unable to get the Axes property of the Chart class"));
        }
        Ok(report(workbook, sheet, "Axis", ReportItem::Axis(part, kind as u8)))
    }

    /// The axis a macro holds, as the chart stores it now.
    fn axis_read<T>(
        &self,
        target: &ObjectRef,
        what: &str,
        read: impl Fn(&calc_doc_model::ChartAxis) -> T,
    ) -> Result<T, AutoError> {
        let kind = Self::axis_key(target)? as usize;
        self.real_chart(target, |c| {
            let id = c.plots.first().and_then(|p| p.axis_ids.get(kind - 1)).copied();
            id.and_then(|id| c.axes.iter().find(|a| a.id == id)).map(&read)
        })?
        .ok_or_else(|| AutoError::excel(1004, format!("Unable to get the {what} property of the Axis class")))
    }

    fn axis_value(&self, target: &ObjectRef, call: &Call<'_>, scale: Scale) -> Reply {
        no_args(call)?;
        let name = scale.name();
        let (kind, value) = self.axis_read(target, name, |a| {
            (
                a.kind.clone(),
                match scale {
                    Scale::Minimum => a.minimum.clone(),
                    Scale::Maximum => a.maximum.clone(),
                    Scale::Unit => a.major_unit.clone(),
                },
            )
        })?;
        if kind != "valAx" {
            return Err(AutoError::excel(
                1004,
                format!("Unable to get the {name} property of the Axis class"),
            ));
        }
        match value.and_then(|v| v.parse::<f64>().ok()) {
            Some(v) => Ok(AutoValue::R8(v)),
            None => Err(AutoError::not_modelled(
                format!("Axis.{name} while automatic (Excel's computed scale)"),
                AXES,
            )),
        }
    }

    fn axis_auto(&self, target: &ObjectRef, call: &Call<'_>, scale: Scale) -> Reply {
        no_args(call)?;
        let name = format!("{}IsAuto", scale.name());
        let (kind, value) = self.axis_read(target, &name, |a| {
            (
                a.kind.clone(),
                match scale {
                    Scale::Minimum => a.minimum.is_none(),
                    Scale::Maximum => a.maximum.is_none(),
                    Scale::Unit => a.major_unit.is_none(),
                },
            )
        })?;
        if kind != "valAx" {
            return Err(AutoError::excel(
                1004,
                format!("Unable to get the {name} property of the Axis class"),
            ));
        }
        Ok(AutoValue::Bool(value))
    }

    /// Write one scale setting (`Some(number)` explicit, `None` automatic) and keep the others.
    fn axis_write(&self, target: &ObjectRef, call: &Call<'_>, scale: Scale, value: Option<f64>) -> Reply {
        let (_, sheet, _) = Self::report_key(target)?;
        let part = Self::chart_token(target)?;
        let name = scale.name();
        let current = self.axis_read(target, name, |a| a.clone())?;
        if current.kind != "valAx" {
            return Err(AutoError::excel(
                1004,
                format!("Unable to set the {name} property of the Axis class"),
            ));
        }
        let mut axis = current;
        let text = value.map(|v| v.to_string());
        match scale {
            Scale::Minimum => axis.minimum = text,
            Scale::Maximum => axis.maximum = text,
            Scale::Unit => axis.major_unit = text,
        }
        self.axis_command(target, call, sheet, &part, axis)
    }

    fn axis_command(
        &self,
        _target: &ObjectRef,
        call: &Call<'_>,
        sheet: SheetId,
        part: &str,
        axis: calc_doc_model::ChartAxis,
    ) -> Reply {
        self.command(call, |s| {
            Ok(WorkbookCommand::ChartAxis {
                sheet,
                revision: s.host().revision().get(),
                part: part.to_owned(),
                axis: axis.id,
                minimum: axis.minimum.clone(),
                maximum: axis.maximum.clone(),
                major_unit: axis.major_unit.clone(),
                reversed: axis.reversed,
            })
        })
    }

    fn axis_put_value(&self, target: &ObjectRef, call: &Call<'_>, scale: Scale) -> Reply {
        let v = number(call.args.last().ok_or_else(geometry::bad_count)?)?;
        if !v.is_finite() || (matches!(scale, Scale::Unit) && v <= 0.0) {
            return Err(AutoError::excel(1004, "Unable to set the property of the Axis class"));
        }
        self.axis_write(target, call, scale, Some(v))
    }

    fn axis_put_auto(&self, target: &ObjectRef, call: &Call<'_>, scale: Scale) -> Reply {
        if geometry::boolean(call.args.last().ok_or_else(geometry::bad_count)?)? {
            return self.axis_write(target, call, scale, None);
        }
        // False from automatic freezes the computed scale: that needs the renderer's scale.
        let auto = self.axis_read(target, scale.name(), |a| match scale {
            Scale::Minimum => a.minimum.is_none(),
            Scale::Maximum => a.maximum.is_none(),
            Scale::Unit => a.major_unit.is_none(),
        })?;
        if auto {
            return Err(AutoError::not_modelled(
                format!("Axis.{}IsAuto = False from automatic", scale.name()),
                AXES,
            ));
        }
        Ok(AutoValue::Empty)
    }

    pub(super) fn axis_min(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.axis_value(t, c, Scale::Minimum)
    }
    pub(super) fn axis_max(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.axis_value(t, c, Scale::Maximum)
    }
    pub(super) fn axis_unit(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.axis_value(t, c, Scale::Unit)
    }
    pub(super) fn axis_put_min(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.axis_put_value(t, c, Scale::Minimum)
    }
    pub(super) fn axis_put_max(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.axis_put_value(t, c, Scale::Maximum)
    }
    pub(super) fn axis_put_unit(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.axis_put_value(t, c, Scale::Unit)
    }
    pub(super) fn axis_min_auto(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.axis_auto(t, c, Scale::Minimum)
    }
    pub(super) fn axis_max_auto(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.axis_auto(t, c, Scale::Maximum)
    }
    pub(super) fn axis_unit_auto(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.axis_auto(t, c, Scale::Unit)
    }
    pub(super) fn axis_put_min_auto(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.axis_put_auto(t, c, Scale::Minimum)
    }
    pub(super) fn axis_put_max_auto(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.axis_put_auto(t, c, Scale::Maximum)
    }
    pub(super) fn axis_put_unit_auto(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.axis_put_auto(t, c, Scale::Unit)
    }

    pub(super) fn axis_reversed(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        Ok(AutoValue::Bool(self.axis_read(target, "ReversePlotOrder", |a| a.reversed)?))
    }
    pub(super) fn axis_put_reversed(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let on = geometry::boolean(call.args.last().ok_or_else(geometry::bad_count)?)?;
        let (_, sheet, _) = Self::report_key(target)?;
        let part = Self::chart_token(target)?;
        let mut axis = self.axis_read(target, "ReversePlotOrder", |a| a.clone())?;
        axis.reversed = on;
        self.axis_command(target, call, sheet, &part, axis)
    }
    /// `Axis.Type`: xlCategory (1) or xlValue (2).
    pub(super) fn axis_type(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        Ok(AutoValue::I4(i32::from(Self::axis_key(target)?)))
    }

    // --- titles and tick labels (dc-oxh8w.12) ---------------------------------------------------

    fn axis_labels(&self, target: &ObjectRef, call: &Call<'_>, title: Option<Option<String>>, format: Option<Option<String>>) -> Reply {
        let (_, sheet, _) = Self::report_key(target)?;
        let part = Self::chart_token(target)?;
        let id = self.axis_read(target, "Title", |a| a.id)?;
        self.command(call, |s| {
            Ok(WorkbookCommand::ChartAxisLabels {
                sheet,
                revision: s.host().revision().get(),
                part: part.clone(),
                axis: id,
                set_title: title.is_some(),
                title: title.clone().flatten(),
                set_number_format: format.is_some(),
                number_format: format.clone().flatten(),
            })
        })
    }

    pub(super) fn axis_has_title(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        Ok(AutoValue::Bool(self.axis_read(target, "HasTitle", |a| a.title.is_some())?))
    }
    pub(super) fn axis_put_has_title(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let on = geometry::boolean(call.args.last().ok_or_else(geometry::bad_count)?)?;
        let has = self.axis_read(target, "HasTitle", |a| a.title.is_some())?;
        if on == has {
            return Ok(AutoValue::Empty);
        }
        self.axis_labels(target, call, Some(on.then(|| "Axis Title".to_owned())), None)
    }
    pub(super) fn axis_title(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        if !self.axis_read(target, "AxisTitle", |a| a.title.is_some())? {
            return Err(AutoError::excel(-2147024809, "This object has no title."));
        }
        let part = Self::chart_token(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        Ok(report(workbook, sheet, "AxisTitle", ReportItem::AxisTitle(part, Self::axis_key(target)?)))
    }
    pub(super) fn axis_title_text(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        match self.axis_read(target, "Text", |a| a.title.clone())? {
            Some(t) => Ok(AutoValue::text(&t)),
            None => Err(AutoError::excel(-2147024809, "This object has no title.")),
        }
    }
    pub(super) fn axis_title_put_text(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let text = super::report::text_arg(call.args.last().ok_or_else(geometry::bad_count)?)?;
        if self.axis_read(target, "Text", |a| a.title.is_none())? {
            return Err(AutoError::excel(-2147024809, "This object has no title."));
        }
        if text.chars().count() > 255 {
            return Err(AutoError::excel(-2147467259, "Method 'Text' of object 'AxisTitle' failed"));
        }
        self.axis_labels(target, call, Some(Some(text)), None)
    }
    pub(super) fn axis_tick_labels(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let part = Self::chart_token(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        Ok(report(workbook, sheet, "TickLabels", ReportItem::TickLabels(part, Self::axis_key(target)?)))
    }
    pub(super) fn tick_number_format(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let code = self.axis_read(target, "NumberFormat", |a| a.number_format.clone())?;
        Ok(AutoValue::text(code.as_deref().unwrap_or("General")))
    }
    pub(super) fn tick_number_format_linked(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        Ok(AutoValue::Bool(self.axis_read(target, "NumberFormatLinked", |a| a.number_format.is_none())?))
    }
    pub(super) fn tick_put_number_format(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let code = super::report::text_arg(call.args.last().ok_or_else(geometry::bad_count)?)?;
        let kind = self.axis_read(target, "NumberFormat", |a| a.kind.clone())?;
        if kind != "valAx" || !matches!(code.as_str(), "0" | "0.00" | "#,##0" | "#,##0.00" | "0%" | "0.00%" | "General") {
            return Err(AutoError::not_modelled(
                format!("TickLabels.NumberFormat = {code:?} (only the retained value-axis codes read back)"),
                "dc-oxh8w.12",
            ));
        }
        let value = (code != "General").then_some(code);
        self.axis_labels(target, call, None, Some(value))
    }
    pub(super) fn tick_put_number_format_linked(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let on = geometry::boolean(call.args.last().ok_or_else(geometry::bad_count)?)?;
        if !on {
            return Err(AutoError::not_modelled("TickLabels.NumberFormatLinked = False (keeps the displayed format)", "dc-oxh8w.12"));
        }
        self.axis_labels(target, call, None, Some(None))
    }
}
