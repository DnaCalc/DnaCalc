//! Profiles: member implementations. `values_only` holds the application members that need no
//! workbook; DNA OneCalc uses it alone and DNA Calc's workbook profile (`calc_host::automation`)
//! falls back to it.

use crate::model::AutoError;
use crate::value::AutoValue;

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
}
