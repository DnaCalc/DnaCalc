//! Name definitions are passive projections. A value is always a qualified, explicit evaluation.
use crate::projection::{Key, Sheet};
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameEvaluation {
    pub id: (u32, u32),
    pub version: u64,
    pub revision: u64,
    pub command: u64,
    pub anchor: Key,
    pub text: Result<String, String>,
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
