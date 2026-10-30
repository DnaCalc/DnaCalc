//! The package for CalcFunc: the worksheet function catalogue and semantic kernels.
//!
//! Dependency rule: it never depends on calc-fml, calc-engine or calc-host; formatting is to reach it through a lower-level service contract.
//!
//! Its value API is the calc-value package, exposed as the module [`value`]: identities, checked
//! grid coordinates, worksheet values and the numeric policy.
//!
//! The catalogue ([`catalogue`]) is generated at compile time from per-family fragments, each
//! entry with an explicit stable [`FunctionId`]. A caller resolves an ID or a surface to a
//! [`FunctionCallTarget`], reads its metadata (arity, parameter classes, [`ArgumentDemand`],
//! signature help, oracle formula, the [`FilePrefix`] a file writes before a named function) and
//! runs it with [`invoke`], supplying the arguments through
//! [`PreparedArguments`] and owning the [`EvalServices`] (the numeric mode and a [`CellReader`]
//! for reference arguments) and the [`FunctionCallScratch`].

pub use calc_value as value;

mod call;
mod catalogue;
mod coerce;
mod decimal;
mod file_prefix;
mod format;
/// One-line function descriptions for signature help and function help.
pub mod help;
mod kernel;
pub mod locale;
mod number_text;
mod number_words;
mod ported;
mod value_text;

pub use call::{
    ApplicationFacts, CalculationMode, CallAbandoned, CallableInvoker, CellFormatFacts, CellReader,
    ColumnWidth, EvalServices, FunctionCallScratch, FunctionFault, HorizontalAlignment, HostFacts,
    PreparedArguments, RandomSource, ReferenceText, SubtotalExclusions, TextReference, Unreadable,
    WorkbookFacts, WorkbookFile, invoke,
};
pub use catalogue::{
    ArgumentDemand, Catalogue, FunctionCallTarget, FunctionId, HelpParameter, Notation,
    ParameterClass, RegistrationError, SignatureHelp, catalogue,
};
pub use file_prefix::FilePrefix;

/// What `text` (UTF-16 code units) reads as under `environment`'s locale profile, date system and
/// clock, by the grammar VALUE reads text with: the number, date or time it is; `Ok(None)` when the
/// grammar does not read it (VALUE's #VALUE!); `Err(())` when the reading is not modelled (no
/// locale profile, a date without a year and no clock, and the other cases `value_text` lists).
/// Excel reads a cell entry typed under a locale the same way (dc-10y).
pub fn read_number_text(text: &[u16], environment: &locale::CalcEnvironment) -> Result<Option<f64>, ()> {
    let profile = environment.profile.as_ref().ok_or(())?;
    let context = value_text::ReadContext {
        profile,
        date_system: environment.date_system,
        now: environment.now,
    };
    match value_text::read(text, &context) {
        Ok(reading) => Ok(Some(reading.value())),
        Err(value_text::Unread::Rejected) => Ok(None),
        Err(value_text::Unread::NotModelled) => Err(()),
    }
}

/// A number in Excel's General format under a locale profile, as a cell shows it.
pub use format::general as general_format;

/// A cell's value as the cell shows it under a number format code, by TEXT's renderer (dc-09v).
pub use format::{CellFormatError, DEFAULT_PALETTE, FormatColor, cell_color, cell_text};
