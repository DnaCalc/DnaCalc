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
//! signature help, oracle formula) and runs it with [`invoke`], supplying the arguments through
//! [`PreparedArguments`] and owning the [`EvalServices`] (the numeric mode and a [`CellReader`]
//! for reference arguments) and the [`FunctionCallScratch`].

pub use calc_value as value;

mod call;
mod catalogue;
mod coerce;
mod decimal;
mod kernel;
mod number_text;
mod number_words;
mod ported;

pub use call::{
    CellReader, EvalServices, FunctionCallScratch, FunctionFault, PreparedArguments, RandomSource,
    SubtotalExclusions, Unreadable, invoke,
};
pub use catalogue::{
    ArgumentDemand, Catalogue, FunctionCallTarget, FunctionId, HelpParameter, Notation,
    ParameterClass, RegistrationError, SignatureHelp, catalogue,
};
