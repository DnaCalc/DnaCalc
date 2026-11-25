//! Application state flags that need no workbook: `Application.ScreenUpdating`.
use super::*;
use crate::SkinIntent;

/// The Excel build the captures record (16.0.20527).
const EXCEL_BUILD: i32 = 20527;

impl WorkbookAutomation {
    pub(super) fn register_application(handlers: &mut Handlers<Self>) {
        handlers.on("Application", "ScreenUpdating", InvokeKind::Get, Self::get_screen_updating);
        handlers.on("Application", "ScreenUpdating", InvokeKind::Put, Self::put_screen_updating);
        handlers.on("Application", "StatusBar", InvokeKind::Get, Self::get_status_bar);
        handlers.on("Application", "StatusBar", InvokeKind::Put, Self::put_status_bar);
        handlers.on("Application", "Build", InvokeKind::Get, Self::get_build);
        handlers.on("Application", "Calculation", InvokeKind::Get, Self::get_calculation);
        handlers.on("Application", "Calculation", InvokeKind::Put, Self::put_calculation);
    }

    /// `Application.Build`: the build number of the Excel the captures were taken from
    /// (Excel 16.0.20527, the version `Application.Version` reports).
    fn get_build(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        if !call.args.is_empty() {
            return Err(AutoError::excel(1004, "Application-defined or object-defined error"));
        }
        Ok(AutoValue::I4(EXCEL_BUILD))
    }

    /// `Application.Calculation`: xlCalculationAutomatic (-4105), xlCalculationManual (-4135) or
    /// xlCalculationSemiautomatic (2).
    fn get_calculation(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        if !call.args.is_empty() {
            return Err(AutoError::excel(1004, "Application-defined or object-defined error"));
        }
        let mode = self.with_access(|access| {
            match access {
                Access::Invocation(_) => Err(AutoError::not_modelled(
                    "Application.Calculation read inside a worksheet function call",
                    "dc-oxh8w.33",
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
}
