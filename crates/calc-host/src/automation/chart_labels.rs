//! Data labels of a series (dc-oxh8w.18): `Series.HasDataLabels`, `Series.ApplyDataLabels` and `Series.DataLabels`
//! with the `DataLabels` members that say what a label shows and where it sits. Writes go through
//! `WorkbookCommand::ChartSeriesLabels`; the retained chart writer saves `c:dLbls`. Labels of single points
//! (`DataLabels(Index)`) and label text formatting are dc-oxh8w.28.

use super::report::report;
use super::*;
use crate::WorkbookCommand;
use calc_doc_model::ChartLabels;
use calc_object_model::ReportItem;

const POINTS: &str = "dc-oxh8w.28";

/// `XlDataLabelPosition` and the `c:dLblPos` value it stands for.
const POSITIONS: [(i32, &str); 8] = [
    (-4108, "ctr"),
    (3, "inEnd"),
    (4, "inBase"),
    (2, "outEnd"),
    (5, "bestFit"),
    (-4131, "l"),
    (-4152, "r"),
    (0, "t"),
];

fn shows(labels: &ChartLabels) -> bool {
    !labels.deleted && (labels.value || labels.category || labels.series || labels.percent)
}

impl WorkbookAutomation {
    /// The labels a series shows (its own, else its plot's) and whether the series states its own, with the
    /// command's coordinates (plot position, series index).
    fn labels_state(&self, target: &ObjectRef) -> Result<(Option<ChartLabels>, bool, bool, u32, u32), AutoError> {
        let (place, _) = self.series_at(target)?;
        self.real_chart(target, |c| {
            let mut at = place;
            for (plot_index, plot) in c.plots.iter().enumerate() {
                if at < plot.series.len() {
                    let s = &plot.series[at];
                    let plot_shows = plot.labels.as_ref().is_some_and(shows);
                    let effective = s.labels.clone().or_else(|| plot.labels.clone());
                    return Ok((effective, s.labels.is_some(), plot_shows, plot_index as u32, s.index));
                }
                at -= plot.series.len();
            }
            Err(AutoError::excel(424, "Object required"))
        })?
    }

    /// The command's coordinates of the series a report object denotes: plot position and series index.
    pub(super) fn series_coords(&self, target: &ObjectRef) -> Result<(u32, u32), AutoError> {
        let (_, _, _, plot, series) = self.labels_state(target)?;
        Ok((plot, series))
    }

    fn labels_command(&self, target: &ObjectRef, call: &Call<'_>, labels: Option<ChartLabels>) -> Reply {
        let (_, sheet, _) = Self::report_key(target)?;
        let part = Self::chart_token(target)?;
        let (_, _, _, plot, series) = self.labels_state(target)?;
        self.command(call, |s| {
            Ok(WorkbookCommand::ChartSeriesLabels {
                sheet,
                revision: s.host().revision().get(),
                part: part.clone(),
                plot,
                series,
                labels: labels.clone(),
            })
        })
    }

    /// Labels that show nothing: removed when the series follows a plot that shows none, else deleted for the series.
    fn no_labels(plot_shows: bool, keep: Option<&ChartLabels>) -> Option<ChartLabels> {
        plot_shows.then(|| ChartLabels { deleted: true, points: keep.map(|k| k.points.clone()).unwrap_or_default(), ..ChartLabels::default() })
    }

    pub(super) fn series_has_labels(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let (labels, ..) = self.labels_state(target)?;
        Ok(AutoValue::Bool(labels.as_ref().is_some_and(shows)))
    }

    pub(super) fn series_put_has_labels(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let on = geometry::boolean(one_value(call)?)?;
        let (labels, _, plot_shows, ..) = self.labels_state(target)?;
        let has = labels.as_ref().is_some_and(shows);
        if on == has {
            return Ok(AutoValue::Empty);
        }
        let next = if on {
            Some(ChartLabels { value: true, points: labels.map(|l| l.points).unwrap_or_default(), ..ChartLabels::default() })
        } else {
            Self::no_labels(plot_shows, labels.as_ref())
        };
        self.labels_command(target, call, next)
    }

    /// `Series.ApplyDataLabels(Type)`: xlDataLabelsShowNone (-4142), ShowValue (2, the default), ShowPercent (3),
    /// ShowLabel (4: the category name), ShowLabelAndPercent (5). Bubble sizes (6) and any other number are 1004.
    pub(super) fn series_apply_labels(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() > 1 {
            return Err(geometry::bad_count());
        }
        let kind = if call.arg(0).is_missing() { 2 } else { geometry::integer(call.arg(0))? };
        let (labels, _, plot_shows, ..) = self.labels_state(target)?;
        let points = labels.as_ref().map(|l| l.points.clone()).unwrap_or_default();
        let mut next = ChartLabels { points, ..ChartLabels::default() };
        match kind {
            -4142 => return self.labels_command(target, call, Self::no_labels(plot_shows, labels.as_ref())),
            2 => next.value = true,
            3 => next.percent = true,
            4 => next.category = true,
            5 => {
                next.category = true;
                next.percent = true;
            }
            _ => return Err(AutoError::excel(1004, "Unable to get the ApplyDataLabels property of the Series class")),
        }
        if let Some(old) = labels.as_ref() {
            next.position = old.position.clone();
            next.separator = old.separator.clone();
        }
        self.labels_command(target, call, Some(next))
    }

    pub(super) fn series_data_labels(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() > 1 {
            return Err(geometry::bad_count());
        }
        if !call.arg(0).is_missing() {
            return Err(AutoError::not_modelled("Series.DataLabels(Index): the label of one point", POINTS));
        }
        let (labels, ..) = self.labels_state(target)?;
        if !labels.as_ref().is_some_and(shows) {
            return Err(AutoError::excel(1004, "Unable to get the DataLabels property of the Series class"));
        }
        let (place, _) = self.series_at(target)?;
        let part = Self::chart_token(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        Ok(report(workbook, sheet, "DataLabels", ReportItem::DataLabels(part, place as u32)))
    }

    fn labels_read<T>(&self, target: &ObjectRef, read: impl Fn(&ChartLabels) -> T) -> Result<T, AutoError> {
        no_args_result(self.labels_state(target)?.0.as_ref().filter(|l| shows(l)).map(read))
    }

    /// Change one part of the labels the series shows now; the labels must still show something.
    fn labels_edit(&self, target: &ObjectRef, call: &Call<'_>, edit: impl Fn(&mut ChartLabels)) -> Reply {
        let (labels, ..) = self.labels_state(target)?;
        let mut next = labels.filter(shows).ok_or_else(|| AutoError::excel(1004, "Unable to set the property of the DataLabels class"))?;
        edit(&mut next);
        if !shows(&next) {
            // Excel's data labels with nothing to show are gone.
            return self.series_put_has_labels_off(target, call);
        }
        self.labels_command(target, call, Some(next))
    }
    fn series_put_has_labels_off(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let (labels, _, plot_shows, ..) = self.labels_state(target)?;
        self.labels_command(target, call, Self::no_labels(plot_shows, labels.as_ref()))
    }

    fn flag_get(&self, target: &ObjectRef, call: &Call<'_>, get: fn(&ChartLabels) -> bool) -> Reply {
        no_args(call)?;
        Ok(AutoValue::Bool(self.labels_read(target, get)?))
    }
    fn flag_put(&self, target: &ObjectRef, call: &Call<'_>, set: fn(&mut ChartLabels, bool)) -> Reply {
        let on = geometry::boolean(one_value(call)?)?;
        self.labels_edit(target, call, |l| set(l, on))
    }
    pub(super) fn labels_show_value(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.flag_get(t, c, |l| l.value)
    }
    pub(super) fn labels_put_show_value(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.flag_put(t, c, |l, on| l.value = on)
    }
    pub(super) fn labels_show_category(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.flag_get(t, c, |l| l.category)
    }
    pub(super) fn labels_put_show_category(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.flag_put(t, c, |l, on| l.category = on)
    }
    pub(super) fn labels_show_series(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.flag_get(t, c, |l| l.series)
    }
    pub(super) fn labels_put_show_series(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.flag_put(t, c, |l, on| l.series = on)
    }
    pub(super) fn labels_show_percent(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.flag_get(t, c, |l| l.percent)
    }
    pub(super) fn labels_put_show_percent(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.flag_put(t, c, |l, on| l.percent = on)
    }

    /// `DataLabels.Position`: the `XlDataLabelPosition` of the labels; unstated reads as xlLabelPositionOutsideEnd
    /// for a bar, line or scatter chart... Excel's per-type defaults are asked in RUN-ON-WINDOWS (dc-oxh8w.28).
    pub(super) fn labels_position(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let position = self.labels_read(target, |l| l.position.clone())?;
        match position {
            Some(p) => POSITIONS
                .iter()
                .find(|(_, name)| *name == p)
                .map(|(code, _)| AutoValue::I4(*code))
                .ok_or_else(|| AutoError::not_modelled(format!("DataLabels.Position {p}"), POINTS)),
            None => Err(AutoError::not_modelled("DataLabels.Position when the file states none (the chart type's default)", POINTS)),
        }
    }
    pub(super) fn labels_put_position(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let code = geometry::integer(one_value(call)?)?;
        let name = POSITIONS
            .iter()
            .find(|(c, _)| i64::from(*c) == code)
            .map(|(_, n)| (*n).to_owned())
            .ok_or_else(|| AutoError::excel(1004, "Unable to set the Position property of the DataLabels class"))?;
        self.labels_edit(target, call, |l| l.position = Some(name.clone()))
    }
    pub(super) fn labels_separator(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let separator = self.labels_read(target, |l| l.separator.clone())?;
        Ok(AutoValue::text(separator.as_deref().unwrap_or(",")))
    }
    pub(super) fn labels_put_separator(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let text = super::report::text_arg(one_value(call)?)?;
        self.labels_edit(target, call, |l| l.separator = Some(text.clone()))
    }
}

fn one_value<'a>(call: &'a Call<'_>) -> Result<&'a AutoValue, AutoError> {
    match &call.args[..] {
        [v] => Ok(v),
        _ => Err(geometry::bad_count()),
    }
}

fn no_args_result<T>(value: Option<T>) -> Result<T, AutoError> {
    value.ok_or_else(|| AutoError::excel(1004, "Unable to get the property of the DataLabels class"))
}
