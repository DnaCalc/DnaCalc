//! Portable Formula Explorer tools.

use crate::{Refusal, Trial};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConditionalRule {
    GreaterThan(String),
    LessThan(String),
    EqualTo(String),
    Expression(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TestSuite {
    pub schema: String,
    pub cases: Vec<TestCase>,
    #[serde(default)]
    pub providers: Vec<ProviderRequirement>,
}

impl TestSuite {
    pub const SCHEMA: &str = "dna.formula-tests/1";
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TestCase {
    pub trial: Trial,
    pub expected: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formatted: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestOutcome {
    pub verdict: TestVerdict,
    pub actual: Option<String>,
    pub formatted: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestVerdict {
    Match,
    Mismatch,
    Refused(Refusal),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRequirement {
    pub name: String,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormulaOperation {
    Provider(ProviderOperation),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderOperation {
    LoadVba { name: String, source: String },
    UnloadVba,
    ChooseNative,
    LoadNative,
    CancelNative,
    UnloadNative { path: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeProvider {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCatalog {
    pub native: bool,
    pub vba_loaded: bool,
    pub functions: Vec<ProviderFunction>,
    pub libraries: Vec<ProviderLibrary>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderFunction {
    pub name: String,
    pub origin: String,
    pub signature: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderLibrary {
    pub path: String,
    pub sha256: String,
    pub functions: Vec<String>,
}
