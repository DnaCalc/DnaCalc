//! Excel's interface identities and vtable layout for the measured add-in surface.
//! No class registration, activation hook, type-library registration or registry write.
#[cfg(target_arch = "wasm32")]
compile_error!("calc-com-host is native-only");
#[cfg(all(windows, not(target_arch = "x86_64")))]
compile_error!("calc-com-host supports Windows x64 only");

#[cfg(windows)]
mod projection;
#[cfg(windows)]
pub use projection::Projection;
#[cfg(windows)]
mod rtd;
#[cfg(windows)]
pub use rtd::{RtdClient, RtdInput};

/// Read-only workbook metadata supplied by the full desktop application. OneCalc supplies no
/// bridge. The projection owns no workbook state and never borrows a live session by address.
pub trait WorkbookBridge {
    fn full_name(&self) -> String;
    fn active_sheet(&self) -> Option<String>;
    fn events_enabled(&self) -> bool;
    fn set_events_enabled(&self, enabled: bool);
}
