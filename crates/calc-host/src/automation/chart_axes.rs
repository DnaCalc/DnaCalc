//! Chart axes of a macro (dc-oxh8w.11): `Chart.Axes(Type)` and the scale members of `Axis`
//! (`MinimumScale`, `MaximumScale`, `MajorUnit`, their `...IsAuto` switches and `ReversePlotOrder`).
//! Writes go through `WorkbookCommand::ChartAxis`, the chart axis dialog's command, so the retained
//! chart writer saves them. Reading a scale that is automatic returns the renderer's computed scale (dc-oxh8w.13); the rest
//! refuses naming dc-oxh8w.23 (secondary groups, the Axes collection); titles and formats are in dc-oxh8w.12.

use super::report::report;
use super::report_draw::number;
use super::*;
use crate::WorkbookCommand;
use calc_object_model::ReportItem;

const AXES: &str = "dc-oxh8w.3";


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

/// The primary axis of a chart for `Axes(kind)`: by axis kind, not by place in the XML. A scatter chart's
/// xlCategory is its horizontal value axis and xlValue the vertical one.
fn axis_of(c: &calc_doc_model::Chart, kind: u8) -> Option<&calc_doc_model::ChartAxis> {
    let plot = c.plots.first()?;
    let mine: Vec<&calc_doc_model::ChartAxis> =
        plot.axis_ids.iter().filter_map(|id| c.axes.iter().find(|a| a.id == *id)).collect();
    if plot.kind == "scatterChart" {
        let horizontal = |a: &&calc_doc_model::ChartAxis| matches!(a.position.as_deref(), Some("b" | "t"));
        let found = mine.iter().find(|a| (kind == 1) == horizontal(a) && a.kind == "valAx");
        return found.copied().or_else(|| mine.get(usize::from(kind) - 1).copied());
    }
    let wanted = |a: &&calc_doc_model::ChartAxis| match kind {
        1 => matches!(a.kind.as_str(), "catAx" | "dateAx"),
        2 => a.kind == "valAx",
        _ => a.kind == "serAx",
    };
    mine.into_iter().find(wanted)
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
        if call.args.len() > 2 {
            return Err(geometry::bad_count());
        }
        let kind = geometry::integer(call.arg(0))?;
        if !(1..=3).contains(&kind) {
            return Err(AutoError::excel(1004, "Unable to get the Axes property of the Chart class"));
        }
        if !matches!(call.arg(1), AutoValue::Missing) {
            match geometry::integer(call.arg(1))? {
                1 => {}
                2 => return Err(AutoError::not_modelled("Chart.Axes on the secondary axis group", AXES)),
                _ => return Err(AutoError::excel(1004, "Unable to get the Axes property of the Chart class")),
            }
        }
        let part = Self::chart_token(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        let found = self.real_chart(target, |c| axis_of(c, kind as u8).is_some())?;
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
        let kind = Self::axis_key(target)?;
        self.real_chart(target, |c| axis_of(c, kind).map(&read))?
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
            None => Ok(AutoValue::R8(self.computed_scale(target, scale)?)),
        }
    }

    /// The renderer's scale for the axis: what Excel reads while a setting is automatic (dc-oxh8w.23).
    fn computed_scale(&self, target: &ObjectRef, scale: Scale) -> Result<f64, AutoError> {
        let (_, sheet, _) = Self::report_key(target)?;
        let part = Self::chart_token(target)?;
        let id = self.axis_read(target, scale.name(), |a| a.id)?;
        let computed = self.read_host(target, |s, _, _| Ok(crate::chart::axis_scale(s.host(), sheet, &part, id)))?;
        let computed = computed.map_err(|e| {
            AutoError::excel(1004, format!("Unable to get the {} property of the Axis class ({e})", scale.name()))
        })?;
        Ok(match scale {
            Scale::Minimum => computed.minimum,
            Scale::Maximum => computed.maximum,
            Scale::Unit => computed.step,
        })
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
        let v = number(one_arg(call)?)?;
        if !v.is_finite() || (matches!(scale, Scale::Unit) && v <= 0.0) {
            return Err(AutoError::excel(1004, "Unable to set the property of the Axis class"));
        }
        self.axis_write(target, call, scale, Some(v))
    }

    fn axis_put_auto(&self, target: &ObjectRef, call: &Call<'_>, scale: Scale) -> Reply {
        if geometry::boolean(one_arg(call)?)? {
            return self.axis_write(target, call, scale, None);
        }
        // False from automatic freezes the scale the chart shows now.
        let (kind, auto) = self.axis_read(target, scale.name(), |a| {
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
                format!("Unable to set the {}IsAuto property of the Axis class", scale.name()),
            ));
        }
        if auto {
            let current = self.computed_scale(target, scale)?;
            return self.axis_write(target, call, scale, Some(current));
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
        let on = geometry::boolean(one_arg(call)?)?;
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
        let on = geometry::boolean(one_arg(call)?)?;
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
        let text = super::report::text_arg(one_arg(call)?)?;
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
        let code = super::report::text_arg(one_arg(call)?)?;
        // Excel refuses a code it cannot use: the number format engine must be able to apply it.
        let part = Self::chart_token(target)?;
        let usable = self.read_host(target, |s, sheet, _| {
            let view = s.host().charts(sheet).0.into_iter().find(|c| c.part == part);
            Ok(view.is_some_and(|c| {
                !code.is_empty()
                    && code.chars().count() <= 255
                    && calc_func::cell_text(&c.format_profile, c.date_system, &calc_value::CalcValue::number(1.0), &code).is_ok()
            }))
        })?;
        if !usable {
            return Err(AutoError::excel(1004, "Unable to set the NumberFormat property of the TickLabels class"));
        }
        // A code put states it: the labels no longer follow the source cells, "General" included.
        self.axis_labels(target, call, None, Some(Some(code)))
    }
    pub(super) fn tick_put_number_format_linked(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let on = geometry::boolean(one_arg(call)?)?;
        let linked = self.axis_read(target, "NumberFormatLinked", |a| a.number_format.is_none())?;
        if on == linked {
            return Ok(AutoValue::Empty);
        }
        if on {
            return self.axis_labels(target, call, None, Some(None));
        }
        // Unlinked, the labels keep the format they show now: General when the source gave none.
        self.axis_labels(target, call, None, Some(Some("General".to_owned())))
    }
}

/// The one value of a property put: any other count is an argument error.
fn one_arg<'a>(call: &'a Call<'_>) -> Result<&'a AutoValue, AutoError> {
    match &call.args[..] {
        [v] => Ok(v),
        _ => Err(geometry::bad_count()),
    }
}
