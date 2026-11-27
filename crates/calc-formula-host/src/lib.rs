//! Level-zero formula sessions using CalcFml's evaluator and CalcFunc's functions.

pub mod cf;
pub mod display;
pub mod file;
mod fixture;
#[cfg(test)]
mod format_oracle;
pub mod grid;
pub mod result;
pub mod rtd;
mod session;
pub mod shown;
pub mod trace;

use calc_fml::{CalcEnvironment, DateSystem, FormatProfile};

pub use calc_macro_security::{FileOrigin, MacroBlock, MacroPolicy, MacroSecurity, Pin};
pub use fixture::{Read, ReadRun, RunCells};
pub use session::{
    AddinRef, Calculation, Diagnostic, ExternalReturns, InputError, InputValue, MacroEnable,
    MacroState, ProjectReport, Session, SessionOptions, SessionProject, SheetError,
};

/// The calculation environment a surface gives its session: the selected date system, the profile of
/// `culture`, and the owner's clock.
pub fn environment(culture: &str, now: Option<f64>, date_1904: bool) -> CalcEnvironment {
    CalcEnvironment {
        compatibility_version: 1,
        date_system: if date_1904 {
            DateSystem::System1904
        } else {
            DateSystem::System1900
        },
        profile: FormatProfile::for_culture(culture),
        now,
    }
}

