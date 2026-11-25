//! Application state flags that need no workbook: `Application.ScreenUpdating`.
use super::*;
use crate::SkinIntent;

/// The Excel build the captures record (16.0.20527).
const EXCEL_BUILD: i32 = 20527;
/// The calculation engine version the captures record (`calcId`).
const EXCEL_CALCULATION_VERSION: i32 = 191029;
/// `Application.OperatingSystem` in the captures.
const EXCEL_OPERATING_SYSTEM: &str = "Windows (64-bit) NT 10.00";

fn excel_error() -> AutoError {
    AutoError::excel(1004, "Application-defined or object-defined error")
}
fn excel_no_args(call: &Call<'_>) -> Result<(), AutoError> {
    if call.args.is_empty() { Ok(()) } else { Err(excel_error()) }
}

impl WorkbookAutomation {
    pub(super) fn register_application(handlers: &mut Handlers<Self>) {
        handlers.on("Application", "ScreenUpdating", InvokeKind::Get, Self::get_screen_updating);
        handlers.on("Application", "ScreenUpdating", InvokeKind::Put, Self::put_screen_updating);
        handlers.on("Application", "DisplayAlerts", InvokeKind::Get, Self::get_display_alerts);
        handlers.on("Application", "DisplayAlerts", InvokeKind::Put, Self::put_display_alerts);
        handlers.on("Application", "CutCopyMode", InvokeKind::Get, Self::get_cut_copy_mode);
        handlers.on("Application", "CutCopyMode", InvokeKind::Put, Self::put_cut_copy_mode);
        handlers.on("Application", "StatusBar", InvokeKind::Get, Self::get_status_bar);
        handlers.on("Application", "StatusBar", InvokeKind::Put, Self::put_status_bar);
        handlers.on("Application", "Build", InvokeKind::Get, Self::get_build);
        handlers.on("Application", "Calculation", InvokeKind::Get, Self::get_calculation);
        handlers.on("Application", "Calculation", InvokeKind::Put, Self::put_calculation);
        handlers.on("Application", "CalculationVersion", InvokeKind::Get, Self::get_calculation_version);
        handlers.on("Application", "OperatingSystem", InvokeKind::Get, Self::get_operating_system);
        handlers.on("Application", "CalculateBeforeSave", InvokeKind::Get, Self::get_calculate_before_save);
        handlers.on("Application", "Iteration", InvokeKind::Get, Self::get_iteration);
        handlers.on("Application", "Iteration", InvokeKind::Put, Self::put_iteration);
        handlers.on("Application", "MaxIterations", InvokeKind::Get, Self::get_max_iterations);
        handlers.on("Application", "MaxIterations", InvokeKind::Put, Self::put_max_iterations);
        handlers.on("Application", "MaxChange", InvokeKind::Get, Self::get_max_change);
        handlers.on("Application", "MaxChange", InvokeKind::Put, Self::put_max_change);
        handlers.on("Workbook", "Date1904", InvokeKind::Get, Self::get_date1904);
    }

    /// `Application.CalculationVersion`: the calculation engine version the captures record.
    fn get_calculation_version(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        excel_no_args(call)?;
        Ok(AutoValue::I4(EXCEL_CALCULATION_VERSION))
    }

    /// `Application.OperatingSystem`: the string the captured Excel reports.
    fn get_operating_system(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        excel_no_args(call)?;
        Ok(AutoValue::text(EXCEL_OPERATING_SYSTEM))
    }

    /// Reads a value from the session; inside a worksheet function call there is none.
    fn session_read<T>(
        &self,
        what: &str,
        read: impl Fn(&HostSession) -> T,
    ) -> Result<T, AutoError> {
        self.with_access(|access| match access {
            Access::Invocation(_) => Err(AutoError::not_modelled(
                format!("{what} read inside a worksheet function call"),
                "dc-oxh8w.40",
            )),
            Access::Session(session, _) => Ok(read(session)),
        })
    }

    /// `Application.CalculateBeforeSave`: the `calcOnSave` the workbook's file carries.
    fn get_calculate_before_save(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        excel_no_args(call)?;
        let value = self.session_read("Application.CalculateBeforeSave", |s| {
            s.host().calculate_before_save()
        })?;
        Ok(AutoValue::Bool(value))
    }

    fn iteration_settings(&self, what: &str) -> Result<calc_doc_model::IterationSettings, AutoError> {
        self.session_read(what, |s| s.host().model().definitions().iteration())
    }

    fn get_iteration(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        excel_no_args(call)?;
        Ok(AutoValue::Bool(self.iteration_settings("Application.Iteration")?.enabled()))
    }

    fn get_max_iterations(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        excel_no_args(call)?;
        let count = self.iteration_settings("Application.MaxIterations")?.max_iterations();
        Ok(AutoValue::I4(count as i32))
    }

    fn get_max_change(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        excel_no_args(call)?;
        Ok(AutoValue::R8(self.iteration_settings("Application.MaxChange")?.max_change()))
    }

    /// An iteration setting changes through the document's one setting, so undo, save and the
    /// recalculation that follows see it.
    fn put_iteration_setting(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        change: impl Fn(calc_doc_model::IterationSettings, &AutoValue) -> Result<calc_doc_model::IterationSettings, AutoError>,
    ) -> Reply {
        self.mutate(target, call, |session, _| {
            if call.args.len() != 1 {
                return Err(excel_error());
            }
            let current = session.host().model().definitions().iteration();
            Ok(SkinIntent::Edit {
                edits: vec![crate::CellEdit::SetIteration(change(current, call.arg(0))?)],
            })
        })
    }

    fn put_iteration(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.put_iteration_setting(target, call, |now, value| {
            let enabled = geometry::boolean(value)?;
            Ok(calc_doc_model::IterationSettings::new(enabled, now.max_iterations(), now.max_change()))
        })
    }

    /// Excel's Maximum Iterations box takes 1 to 32767.
    fn put_max_iterations(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.put_iteration_setting(target, call, |now, value| {
            let count = u32::try_from(geometry::integer(value)?)
                .ok()
                .filter(|n| (1..=32767).contains(n))
                .ok_or_else(|| AutoError::excel(1004, "Unable to set the MaxIterations property of the Application class"))?;
            Ok(calc_doc_model::IterationSettings::new(now.enabled(), count, now.max_change()))
        })
    }

    fn put_max_change(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.put_iteration_setting(target, call, |now, value| {
            let change = match value {
                AutoValue::I2(v) => f64::from(*v),
                AutoValue::I4(v) => f64::from(*v),
                AutoValue::R4(v) => f64::from(*v),
                AutoValue::R8(v) => *v,
                _ => return Err(geometry::type_mismatch()),
            };
            if !change.is_finite() || change < 0.0 {
                return Err(AutoError::excel(
                    1004,
                    "Unable to set the MaxChange property of the Application class",
                ));
            }
            Ok(calc_doc_model::IterationSettings::new(now.enabled(), now.max_iterations(), change))
        })
    }

    /// `Workbook.Date1904`: the date system the loaded file declares.
    fn get_date1904(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        excel_no_args(call)?;
        let value = self.session_read("Workbook.Date1904", |s| s.host().declared_date1904())?;
        Ok(AutoValue::Bool(value))
    }

    /// `Application.Build`: the build number of the Excel the captures were taken from
    /// (Excel 16.0.20527, the version `Application.Version` reports).
    fn get_build(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        excel_no_args(call)?;
        Ok(AutoValue::I4(EXCEL_BUILD))
    }

    /// `Application.Calculation`: xlCalculationAutomatic (-4105), xlCalculationManual (-4135) or
    /// xlCalculationSemiautomatic (2).
    fn get_calculation(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        excel_no_args(call)?;
        let mode = self.with_access(|access| {
            match access {
                Access::Invocation(_) => Err(AutoError::not_modelled(
                    "Application.Calculation read inside a worksheet function call",
                    "dc-oxh8w.35",
                )),
                Access::Session(session, _) => Ok(session.calc_mode()),
            }
        })?;
        Ok(AutoValue::I4(match mode {
            calc_doc::CalcMode::Manual => -4135,
            calc_doc::CalcMode::AutomaticExceptTables => 2,
            calc_doc::CalcMode::Automatic => -4105,
        }))
    }

    /// Setting a mode other than Manual recalculates, as Excel does on leaving manual mode.
    fn put_calculation(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.mutate(target, call, |_, _| {
            if call.args.len() != 1 {
                return Err(AutoError::excel(1004, "Application-defined or object-defined error"));
            }
            let mode = match geometry::integer(call.arg(0)) {
                Ok(-4105) => calc_doc::CalcMode::Automatic,
                Ok(-4135) => calc_doc::CalcMode::Manual,
                Ok(2) => calc_doc::CalcMode::AutomaticExceptTables,
                _ => {
                    return Err(AutoError::excel(
                        1004,
                        "Unable to set the Calculation property of the Application class",
                    ));
                }
            };
            Ok(SkinIntent::SetCalcMode(mode))
        })
    }

    /// The text in the status bar, or False when Excel's own status shows.
    fn get_status_bar(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        Ok(match &*self.status_bar.borrow() {
            Some(text) => AutoValue::text(text),
            None => AutoValue::Bool(false),
        })
    }

    /// Excel documents text assignment and Boolean False to restore its status.
    fn put_status_bar(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        let text = match call.arg(0) {
            AutoValue::Text(_) => call.arg(0).as_text(),
            AutoValue::Bool(false) => None,
            _ => return Err(AutoError::not_modelled("Application.StatusBar coercion", "dc-oxh8w.9")),
        };
        *self.status_bar.borrow_mut() = text;
        Ok(AutoValue::Empty)
    }

    fn get_screen_updating(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        Ok(AutoValue::Bool(self.screen_updating.get()))
    }

    fn put_screen_updating(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        self.screen_updating.set(geometry::boolean(call.arg(0))?);
        Ok(AutoValue::Empty)
    }

    fn get_display_alerts(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        Ok(AutoValue::Bool(self.display_alerts.get()))
    }

    fn put_display_alerts(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        self.display_alerts.set(geometry::boolean(call.arg(0))?);
        Ok(AutoValue::Empty)
    }

    /// `Application.CutCopyMode`: False with nothing marked, xlCopy (1) or xlCut (2) while the clipboard holds a
    /// Range.Copy or Range.Cut.
    fn get_cut_copy_mode(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        Ok(match self.clipboard.borrow().as_ref() {
            None => AutoValue::Bool(false),
            Some(clip) if clip.is_cut() => AutoValue::I4(2),
            Some(_) => AutoValue::I4(1),
        })
    }

    /// Assigning False cancels Cut or Copy mode and empties the clipboard's marked range; Excel accepts the
    /// xlCopy and xlCut constants it reports (a no-op while that mode is on), refused otherwise.
    fn put_cut_copy_mode(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        match call.arg(0) {
            AutoValue::Bool(false) => *self.clipboard.borrow_mut() = None,
            AutoValue::I2(0) | AutoValue::I4(0) => *self.clipboard.borrow_mut() = None,
            AutoValue::I2(1 | 2) | AutoValue::I4(1 | 2) | AutoValue::Bool(true) => {}
            _ => return Err(excel_error()),
        }
        Ok(AutoValue::Empty)
    }
}
