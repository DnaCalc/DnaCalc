//! Office.js worksheet functions using CalcFml's callable registry.

mod callable;
mod engine;
mod metadata;

#[cfg(all(feature = "engine-quickjs", not(target_arch = "wasm32")))]
mod quickjs;

pub use callable::JsAddin;
pub use engine::{ExecLimits, JsArgument, JsEngine, JsHostError, JsInvocation};
pub use metadata::{
    DataType, Dimensionality, FunctionOptions, FunctionResult, FunctionSpec, FunctionsMetadata,
    Parameter,
};
#[cfg(all(feature = "engine-quickjs", not(target_arch = "wasm32")))]
pub use quickjs::QuickJs;
