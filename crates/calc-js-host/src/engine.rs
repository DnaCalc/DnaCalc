use crate::{FunctionSpec, FunctionsMetadata};
use calc_value::CalcValue;
use std::{fmt, time::Duration};

/// Limits applied to module evaluation and to each synchronous invocation.
#[derive(Clone, Copy, Debug)]
pub struct ExecLimits {
    pub time: Duration,
    /// Ceiling for the add-in's entire JavaScript heap, including its retained state.
    pub heap_bytes: usize,
}

impl Default for ExecLimits {
    fn default() -> Self {
        Self {
            time: Duration::from_secs(1),
            heap_bytes: 32 * 1024 * 1024,
        }
    }
}

/// Argument already prepared from worksheet metadata, including repeated inputs.
#[derive(Debug)]
pub enum JsArgument {
    Value(CalcValue),
    Repeating(Vec<CalcValue>),
}

/// Values supplied in the final, implicit `CustomFunctions.Invocation` argument.
#[derive(Clone, Debug, Default)]
pub struct JsInvocation {
    pub function_name: String,
    pub address: Option<String>,
    /// One entry per formula argument. Non-reference arguments have no address.
    pub parameter_addresses: Option<Vec<Option<String>>>,
}

/// One isolated JavaScript runtime for one loaded add-in.
///
/// The callable adapter owns worksheet policy, reference reads, and registration. Backends own
/// guest execution, its limits, and conversion between guest values and prepared worksheet values.
/// The async and streaming owners can extend this seam with backend jobs and invocation tokens
/// (dc-26wtr.6 and dc-26wtr.7) and use the existing CalcFml continuation contract.
pub trait JsEngine: fmt::Debug {
    fn load_module(&mut self, id: &str, source: &str) -> Result<(), JsHostError>;
    fn register_functions(&mut self, metadata: &FunctionsMetadata) -> Result<(), JsHostError>;
    fn invoke_sync(
        &mut self,
        spec: &FunctionSpec,
        arguments: &[JsArgument],
        invocation: &JsInvocation,
    ) -> Result<CalcValue, JsHostError>;
}

/// Registration and execution failures. Worksheet errors themselves are returned as values.
#[derive(Debug, PartialEq, Eq)]
pub enum JsHostError {
    Metadata(String),
    Registration(String),
    Execution(String),
    Timeout,
    Deferred {
        reason: &'static str,
        owner: &'static str,
    },
}

impl fmt::Display for JsHostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Metadata(message) => write!(f, "custom function metadata: {message}"),
            Self::Registration(message) => write!(f, "custom function registration: {message}"),
            Self::Execution(message) => write!(f, "custom function execution: {message}"),
            Self::Timeout => f.write_str("custom function execution time limit exceeded"),
            Self::Deferred { reason, owner } => write!(f, "{reason} ({owner})"),
        }
    }
}

impl std::error::Error for JsHostError {}
