use crate::{Refusal, Stamp};
use serde::{Deserialize, Serialize};

pub use calc_macro_security::MacroSecurity;

/// The operator's preflight for one named procedure in the current workbook.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroReview {
    pub mode: MacroSecurity,
    pub available_modes: Vec<MacroSecurity>,
    pub procedure: Option<String>,
    pub module_source: Option<String>,
    pub reach: Vec<String>,
    pub grants: Vec<String>,
    pub blocked: Option<String>,
    /// A single use on the carrier that reviewed it.
    pub approval: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroDenial {
    pub capability: String,
    pub subject: String,
}

/// Observed effects of the approved invocation, including denials handled by VBA itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroRun {
    pub stamp: Stamp,
    pub procedure: String,
    pub denials: Vec<MacroDenial>,
    pub error: Option<Refusal>,
}
