//! The package for CalcFunc: the worksheet function catalogue and semantic kernels.
//!
//! Dependency rule: it never depends on calc-fml, calc-engine or calc-host; formatting is to reach it through a lower-level service contract.
//!
//! Its value API is the calc-value package, exposed as the module [`value`]: identities, checked
//! grid coordinates, worksheet values and the numeric policy.
//!
//! The catalogue ([`catalogue()`]) is generated at compile time from per-family fragments, each
//! entry with an explicit stable [`FunctionId`]. A caller resolves an ID or a surface to a
//! [`FunctionCallTarget`], reads its metadata (arity, parameter classes, [`ArgumentDemand`],
//! signature help, oracle formula, the [`FilePrefix`] a file writes before a named function) and
//! runs it with [`invoke`], supplying the arguments through
//! [`PreparedArguments`] and owning the [`EvalServices`] (the numeric mode and a [`CellReader`]
//! for reference arguments) and the [`FunctionCallScratch`].
//!
//! ## Invoke a worksheet function
//!
//! Use the catalogue to resolve a named call. The argument provider returns values on demand,
//! so hosts can keep lazy arguments unevaluated until a function reads them.
//!
//! ```
//! use calc_func::value::{CalcValue, NumericMode};
//! use calc_func::{EvalServices, FunctionCallScratch, Notation, PreparedArguments, catalogue, invoke};
//!
//! struct ScalarArgument(CalcValue);
//!
//! impl PreparedArguments for ScalarArgument {
//!     fn count(&self) -> usize {
//!         1
//!     }
//!     fn argument(&mut self, index: usize) -> CalcValue {
//!         assert_eq!(index, 0);
//!         self.0.clone()
//!     }
//! }
//!
//! let target = catalogue().resolve_surface(Notation::Call, "ABS").unwrap();
//! let mut args = ScalarArgument(CalcValue::number(-42.0));
//! let mut services = EvalServices::new(&NumericMode::Fast);
//! let mut scratch = FunctionCallScratch::default();
//! let result = invoke(target, &mut args, &mut services, &mut scratch).unwrap();
//! assert_eq!(result.as_number(), Some(42.0));
//! ```
//!
//! Worked examples and preview status are also in the [engine guide](https://dnacalc.com/developers/engine/).
//!
//! See the [crate contract](https://github.com/DnaCalc/DnaCalc/blob/master/crates/calc-func/CONTRACT.md).

#![warn(missing_docs)]

pub use calc_value as value;

mod call;
mod udf;
pub use udf::{
    MAX_TOPIC_STRINGS, RTD_CONNECT_FAILED, RTD_PROVIDER, ReferenceHandleError, ReferenceHandles,
    ReferenceRead, RtdCallRead, RtdConnection, RtdConnector, RtdCut, RtdEntry, RtdLease, RtdRead,
    RtdTopicRead, StoredReference, TopicKey, UserCallableIdentity, UserFunction,
    UserFunctionContext, UserFunctionInvocation, UserFunctionStep, UserInvocationFrame,
    UserReevaluationReason, UserView, AppSettings, read_rtd_call,
};
mod catalogue;
mod coerce;
mod cube;
mod decimal;
mod fetch;
mod file_prefix;
mod format;
/// One-line function descriptions for signature help and function help.
pub mod help;
mod import;
mod kernel;
pub mod locale;
mod number_text;
mod number_words;
mod ported;
pub mod trace;
mod value_text;

pub use call::{
    ApplicationFacts, CalculationMode, CallAbandoned, CallableInvoker, CellFormatFacts, CellReader,
    ColumnWidth, EvalServices, FunctionCallScratch, FunctionFault, HorizontalAlignment, HostFacts,
    PivotLookup, PreparedArguments, RandomSource, ReferenceText, SubtotalExclusions, TextReference,
    Unreadable, WeekState, WorkbookFacts, WorkbookFile, area_cells, invoke, invoke_slice,
};
pub use catalogue::{
    ArgumentDemand, ArgumentGroups, Availability, BindingForm, Catalogue, Deferral, EntryClass,
    EntryKind, EntryRule, EntryTraits, FunctionCallTarget, FunctionId, HeldReferences,
    HelpParameter, HostRouteKind, Implementation, LegacyPlace, Notation, OperandForm, PairedResult,
    ParameterClass, RegistrationError, ResultForm, SignatureHelp, ThreeDPlace, WholeResultPolicy,
    catalogue,
};
pub use cube::{
    CubeColumn, CubeFailure, CubeLookup, CubeMeasure, CubeMember, CubeModel, CubeTable,
};
pub use fetch::{
    FETCH_FUNCTIONS, FETCH_PROVIDER, FetchFunction, FetchKind, FetchReply, FetchRequest,
    FetchRoute, REDIRECT_LIMIT, RawResponse, decode_body, fetch_following, resolve_location,
    wire_url,
};
pub use calc_catalogue::file_call_name;
pub use file_prefix::FilePrefix;
pub use import::{IMPORT_FUNCTIONS, IMPORT_PROVIDER, ImportFunction, ImportKind, ImportRoute};
pub use number_text::{
    NumberTextInspection, NumberTextRounding, RoundingPath, inspect as inspect_number_text,
};

/// The order of two texts (UTF-16 code units) as Excel's comparison operators give it under the
/// Windows locale en-ZA: Excel's pass over ASCII ignoring case, then Windows' collation ignoring
/// case (dc-zsn.1); `None` when the collation does not decide them (a text holding a code unit it
/// builds no sort key for). `cargo xtask collation verify` checks it against Windows.
pub fn text_order(a: &[u16], b: &[u16]) -> Option<std::cmp::Ordering> {
    catalogue::text_order(a, b)
}

/// Whether the digits of a number literal written in a formula (no sign) are within the scale Excel
/// accepts, 1E-309 up to 9.99E+308: a literal below it makes the formula one Excel does not accept.
pub fn number_literal_scale_is_readable(text: &str) -> bool {
    coerce::literal_scale_is_readable(text)
}

/// Excel's adjustment of a final addition or subtraction: a nonzero result becomes
/// positive zero when its biased binary64 exponent is at least 50 below the left
/// operand's. The left operand decides, rather than the larger operand or any
/// earlier partial sum. Apply this only at a measured final arithmetic site.
///
/// Infix probes (dc-b1l, Excel 16.0.20430) and SUM probes (dc-ss3b, Excel
/// 16.0.20527, `docs/research/sum-cancellation-2026-11-12.md`) agree: around 1,
/// `(1+k*2^-52)-1` becomes zero for k <= 7, and stands for k >= 8. Around
/// the binade boundary at 2, swapping operands changes that boundary. SUM applies
/// the adjustment once, after its last numeric addition; a trailing numeric zero
/// therefore preserves an earlier cancellation residual.
pub fn adjust_final_sum(left: f64, result: f64) -> f64 {
    let exponent = |x: f64| ((x.to_bits() >> 52) & 0x7ff) as i64;
    if result != 0.0 && exponent(left) - exponent(result) >= 50 {
        0.0
    } else {
        result
    }
}

/// What `text` (UTF-16 code units) reads as under `environment`'s locale profile, date system and
/// clock, by the grammar VALUE reads text with: the number, date or time it is; `Ok(None)` when the
/// grammar does not read it (VALUE's #VALUE!); `Err(())` when the reading is not modelled (no
/// locale profile, a date without a year and no clock, and the other cases `value_text` lists).
/// Excel reads a cell entry typed under a locale the same way (dc-10y).
pub fn read_number_text(
    text: &[u16],
    environment: &locale::CalcEnvironment,
) -> Result<Option<f64>, ()> {
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
pub use format::{
    CellFormatError, DEFAULT_PALETTE, FormatColor, cell_color, cell_text, cell_text_localized,
};
