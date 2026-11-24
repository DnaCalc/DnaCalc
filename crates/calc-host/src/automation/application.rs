//! Application state flags that need no workbook: `Application.ScreenUpdating`.
use super::*;

impl WorkbookAutomation {
    pub(super) fn register_application(handlers: &mut Handlers<Self>) {
        handlers.on("Application", "ScreenUpdating", InvokeKind::Get, Self::get_screen_updating);
        handlers.on("Application", "ScreenUpdating", InvokeKind::Put, Self::put_screen_updating);
        handlers.on("Application", "StatusBar", InvokeKind::Get, Self::get_status_bar);
        handlers.on("Application", "StatusBar", InvokeKind::Put, Self::put_status_bar);
    }

    /// The text in the status bar, or False when Excel's own status shows.
    fn get_status_bar(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        Ok(match &*self.status_bar.borrow() {
            Some(text) => AutoValue::text(text),
            None => AutoValue::Bool(false),
        })
    }

    /// Text shows; False restores the default status; anything else is a type mismatch.
    fn put_status_bar(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        let text = match call.arg(0) {
            AutoValue::Text(_) => call.arg(0).as_text(),
            AutoValue::Bool(false) => None,
            _ => return Err(geometry::type_mismatch()),
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
