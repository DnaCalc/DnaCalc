//! Chart sheets of a macro (dc-oxh8w.23): `Chart.Name` and `Chart.Location`. A chart sheet is a sheet kind of the model
//! (`SheetKind::Chart`); `Charts.Add`/`Sheets.Add(xlChart)` are in structure.rs, the sheet collections in navigation.rs.
//! `Chart.Location` moves a chart this session made between a chart sheet and a worksheet in one command; a chart the
//! file holds refuses naming dc-oxh8w.48.

use super::report::report;
use super::*;
use crate::{CommandIngress, SheetChange, SkinIntent, WorkbookCommand};
use calc_object_model::ReportItem;

/// xlLocationAsNewSheet and xlLocationAsObject.
const AS_NEW_SHEET: i64 = 1;
const AS_OBJECT: i64 = 2;
/// The place of a chart moved onto a worksheet, in points: the corner of the sheet and Excel's default chart size.
const OBJECT_PLACE: [f64; 4] = [0.0, 0.0, 360.0, 216.0];
const OWNER: &str = "dc-oxh8w.3";

impl WorkbookAutomation {
    pub(super) fn register_chart_sheets(handlers: &mut Handlers<Self>) {
        handlers.on("Chart", "Name", InvokeKind::Get, Self::chart_name);
        handlers.on("Chart", "Name", InvokeKind::Put, Self::chart_put_name);
        handlers.on("Chart", "Location", InvokeKind::Method, Self::chart_location);
    }

    /// `Chart.Name`: a chart sheet's name is its sheet's; an embedded chart is `<sheet> <object name>`.
    fn chart_name(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let part = Self::chart_token(target)?;
        self.read_host(target, |s, sheet, _| {
            let model = s.host().model();
            let entry = model.sheet_by_id(sheet).ok_or_else(|| AutoError::excel(424, "Object required"))?;
            let sheet_name = entry.name().text().to_string_lossy().to_string();
            if entry.kind() == calc_doc_model::SheetKind::Chart {
                return Ok(AutoValue::text(&sheet_name));
            }
            let chart = model
                .definitions()
                .charts(sheet)
                .iter()
                .find(|c| c.part == part)
                .ok_or_else(|| AutoError::excel(424, "Object required"))?;
            Ok(AutoValue::text(&format!("{sheet_name} {}", chart.name)))
        })
    }

    /// `Chart.Name = ...` renames a chart sheet; an embedded chart's name is its ChartObject's and cannot be put here.
    fn chart_put_name(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let name = call.args.last().ok_or_else(geometry::bad_count).and_then(|v| v.as_text().ok_or_else(geometry::type_mismatch))?;
        let (_, sheet, _) = Self::report_key(target)?;
        let embedded = {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            !meta.chart_sheets.contains(&sheet)
        };
        if embedded {
            // ww22 (VBA, Excel 16.0.20625.20108): an embedded chart's Chart.Name cannot be set: error 7.
            return Err(AutoError::excel(7, "Out of memory"));
        }
        self.with_write(|session| {
            let progress = Self::sheet_command(session, Some(sheet), SheetChange::Rename { name: name.clone() })?;
            Self::settle(session, progress, call)?;
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
    }

    /// `Chart.Location(Where, Name)`: xlLocationAsNewSheet (1) puts the chart on a new chart sheet named `Name`
    /// (`Chart<n>` when omitted); xlLocationAsObject (2) puts it on the worksheet `Name`. Returns the chart.
    fn chart_location(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.is_empty() || call.args.len() > 2 {
            return Err(geometry::bad_count());
        }
        let place = geometry::integer(call.arg(0))?;
        let part = Self::chart_token(target)?;
        let (_, from, _) = Self::report_key(target)?;
        let name = match call.arg(1) {
            AutoValue::Missing | AutoValue::Empty => None,
            other => Some(other.as_text().ok_or_else(geometry::type_mismatch)?),
        };
        let bad = || AutoError::excel(1004, "Application-defined or object-defined error");
        let mut result = Err(bad());
        let reply = self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            let model = session.host().model();
            let on_chart_sheet = meta.chart_sheets.contains(&from);
            let to = match place {
                AS_OBJECT => {
                    let name = name.clone().ok_or_else(bad)?;
                    let found = model
                        .sheets()
                        .iter()
                        .find(|s| {
                            calc_value::workbook_names_equal(&s.name().text().to_string_lossy(), &name)
                                && s.kind() == calc_doc_model::SheetKind::Worksheet
                        })
                        .map(|s| s.id())
                        .filter(|id| !meta.dialog_sheets.contains(id))
                        .ok_or_else(bad)?;
                    if found == from {
                        result = Self::chart_of_sheet_or_object(&meta, from, &part);
                        return Ok(AutoValue::Empty);
                    }
                    found
                }
                AS_NEW_SHEET => {
                    if on_chart_sheet {
                        // Already a chart sheet: it takes the new name.
                        if let Some(name) = &name {
                            let progress = Self::sheet_command(session, Some(from), SheetChange::Rename { name: name.clone() })?;
                            Self::settle(session, progress, call)?;
                            self.context_changed(session);
                        }
                        result = Self::chart_of_sheet_or_object(&*self.metadata()?, from, &part);
                        return Ok(AutoValue::Empty);
                    }
                    let name = name.clone().unwrap_or_else(|| Self::new_chart_sheet_name(model));
                    let known: Vec<SheetId> = model.sheets().iter().map(|s| s.id()).collect();
                    let progress = Self::sheet_command(session, None, SheetChange::AddChart { name })?;
                    Self::settle(session, progress, call)?;
                    session
                        .host()
                        .model()
                        .sheets()
                        .iter()
                        .map(|s| s.id())
                        .find(|id| !known.contains(id))
                        .ok_or_else(bad)?
                }
                _ => return Err(bad()),
            };
            let receipt = session.dispatch_from(
                CommandIngress::Automation,
                SkinIntent::Command(WorkbookCommand::ChartMove {
                    sheet: from,
                    revision: session.host().revision().get(),
                    part: part.clone(),
                    to,
                    place: OBJECT_PLACE.map(crate::Points::new),
                }),
            );
            if let Some(error) = receipt.error {
                // A new chart sheet that took no chart goes again.
                if place == AS_NEW_SHEET {
                    session.dispatch_from(
                        CommandIngress::Automation,
                        SkinIntent::Command(WorkbookCommand::Sheet { sheet: Some(to), change: SheetChange::Remove }),
                    );
                    self.context_changed(session);
                }
                return Err(AutoError::not_modelled(format!("Chart.Location: {error:?}"), OWNER));
            }
            // A chart sheet left without its chart goes (Excel: the chart's sheet is gone once the chart is an object).
            if on_chart_sheet {
                let removed = Self::sheet_command(session, Some(from), SheetChange::Remove)
                    .and_then(|progress| Self::settle(session, progress, call));
                if let Err(err) = removed {
                    // Roll back the chart move so the vacated chart sheet is not left orphaned.
                    session.dispatch_from(CommandIngress::Automation, SkinIntent::Undo);
                    self.context_changed(session);
                    return Err(err);
                }
            }
            // The chart's sheet is the active one, as after Excel's move.
            let first = CellKey::new(to, 1, 1).map_err(|_| geometry::type_mismatch())?;
            session.dispatch_from(CommandIngress::Automation, SkinIntent::Select(crate::Selection::cell(first)));
            self.context_changed(session);
            result = Self::chart_of_sheet_or_object(&*self.metadata()?, to, &part);
            Ok(AutoValue::Empty)
        });
        reply?;
        result
    }

    /// The Chart object of `part` on `sheet`: the chart sheet's chart or the embedded chart.
    fn chart_of_sheet_or_object(meta: &Snapshot, sheet: SheetId, part: &str) -> Reply {
        Ok(report(meta.workbook, sheet, "Chart", ReportItem::Chart(part.to_owned())))
    }
}
