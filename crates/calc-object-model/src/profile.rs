//! Profiles: member implementations. `values_only` holds the application members that need no
//! workbook; DNA OneCalc uses it alone and DNA Calc's workbook profile (`calc_host::automation`)
//! falls back to it.

use crate::model::AutoError;
use crate::value::AutoValue;

pub mod values_only;

/// What the application (the desktop shell and its add-in host) supplies to the values-only
/// members. Implemented by `calc-com-host`/`calc-addin-host` over the window and `XllHost`.
pub trait ApplicationHost {
    /// The main window handle (Application.Hwnd).
    fn hwnd(&self) -> isize;
    /// Application.Run of a registered command or function, arguments in order.
    fn run(&self, name: &str, args: &[AutoValue]) -> Result<AutoValue, AutoError>;
    /// The application's one EnableEvents switch (the session's in DNA Calc).
    fn events_enabled(&self) -> bool;
    fn set_events_enabled(&self, enabled: bool);
    /// Directory containing the application executable. Supplied by the native shell.
    fn path(&self) -> Result<String, AutoError> {
        Err(AutoError::not_modelled("_Application.Path", "dc-1nbp.5"))
    }
}

impl<T: ApplicationHost + ?Sized> ApplicationHost for std::rc::Rc<T> {
    fn hwnd(&self) -> isize {
        (**self).hwnd()
    }
    fn run(&self, name: &str, args: &[AutoValue]) -> Result<AutoValue, AutoError> {
        (**self).run(name, args)
    }
    fn events_enabled(&self) -> bool {
        (**self).events_enabled()
    }
    fn set_events_enabled(&self, enabled: bool) {
        (**self).set_events_enabled(enabled);
    }
    fn path(&self) -> Result<String, AutoError> {
        (**self).path()
    }
}
