//! Name definitions are passive projections. A value is always a qualified, explicit evaluation.
use crate::projection::{Area, Key, Sheet};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameRecord {
    pub id: (u32, u32),
    pub version: u64,
    pub name: String,
    pub scope: Option<Sheet>,
    pub formula: String,
    pub hidden: bool,
    pub comment: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamePage {
    pub revision: u64,
    pub rows: Vec<NameRecord>,
    pub page: u64,
    pub total: usize,
}
/// Bounded execution metadata. Body indices are local to this diagnostic command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameBodyEvaluation {
    pub ordinal: u64,
    pub parent: Option<u64>,
    pub id: (u32, u32),
    pub version: u64,
    pub scope: Option<Sheet>,
    pub caller: Key,
    pub definition_sheet: Option<Sheet>,
    pub body: usize,
    pub result: NameBodyResult,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NameBodyResult {
    Empty,
    Missing,
    Number,
    Text,
    Logical,
    Error(String),
    Reference {
        areas: Vec<Area>,
        omitted_areas: usize,
    },
    Array {
        rows: usize,
        columns: usize,
    },
    Callable,
    Refused,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameEvaluationRoot {
    pub command: (u64, u64),
    pub pass: u32,
    pub evaluation: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameEvaluationEvidence {
    #[serde(default)]
    pub calculation: Option<NameCalculationEvidence>,
    pub results: u64,
    pub workbook: (u64, u64),
    /// Informational numeric policy, never a shared-value reuse key.
    pub numeric: String,
    /// Owner pass identity for the selected name and caller in NameEvaluation.
    #[serde(default)]
    pub root: Option<NameEvaluationRoot>,
    /// Supplied clock and actual unit draws, encoded as binary64 hex bits.
    #[serde(default)]
    pub clock: Option<String>,
    #[serde(default)]
    pub random_draws: Vec<String>,
    #[serde(default)]
    pub omitted_random_draws: u64,
    pub bodies: Vec<NameBodyEvaluation>,
    pub omitted: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameEvaluation {
    pub id: (u32, u32),
    pub version: u64,
    pub revision: u64,
    pub command: u64,
    pub anchor: Key,
    #[serde(default)]
    pub evidence: Option<NameEvaluationEvidence>,
    pub text: Result<String, String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameUserCall {
    pub runtime: u64,
    pub parent: Option<u64>,
    pub provider: (u64, u32, u64),
    pub call_site: u32,
    pub occurrence: Vec<u32>,
    pub completed: bool,
    pub non_preemptible: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameObservation {
    pub area: Area,
    pub view: u64,
    pub imported: bool,
    pub dirty: bool,
    pub external: bool,
    pub provisional: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameCalculationEvidence {
    pub runtime_root: Option<u64>,
    pub calls: Vec<NameUserCall>,
    pub omitted_calls: usize,
    pub observations: Vec<NameObservation>,
    pub omitted_observations: usize,
    pub udf_volatile: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NameCalculationReply {
    Pending {
        token: u64,
    },
    Ready {
        token: u64,
        evaluation: NameEvaluation,
    },
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NameState {
    pub open: bool,
    pub listing: NamePage,
    pub query: String,
    pub page: u64,
    pub selected: Option<NameRecord>,
    pub creating: bool,
    pub name: String,
    pub scope: String,
    pub formula: String,
    pub anchor: String,
    pub anchor_sheet: String,
    pub evaluation: Option<NameEvaluation>,
    pub evaluation_observation: u64,
    pub message: String,
    pub epoch: u64,
    pub refresh: u64,
    pub edit_revision: u64,
}
