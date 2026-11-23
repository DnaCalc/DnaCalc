use crate::JsHostError;
use serde::Deserialize;
use std::collections::BTreeSet;

/// One add-in's `functions.json` with the namespace supplied by its manifest.
#[derive(Clone, Debug)]
pub struct FunctionsMetadata {
    pub namespace: String,
    pub functions: Vec<FunctionSpec>,
    pub allow_error_for_data_type_any: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MetadataFile {
    functions: Vec<FunctionSpec>,
    #[serde(default)]
    allow_error_for_data_type_any: bool,
}

impl FunctionsMetadata {
    /// Parse Office's registration metadata before starting the add-in's script.
    pub fn parse(namespace: &str, json: &str) -> Result<Self, JsHostError> {
        let file: MetadataFile =
            serde_json::from_str(json).map_err(|e| JsHostError::Metadata(e.to_string()))?;
        let metadata = Self {
            namespace: namespace.to_owned(),
            functions: file.functions,
            allow_error_for_data_type_any: file.allow_error_for_data_type_any,
        };
        metadata.validate()?;
        Ok(metadata)
    }

    pub(crate) fn validate(&self) -> Result<(), JsHostError> {
        if !identifier(&self.namespace) {
            return Err(JsHostError::Metadata("invalid add-in namespace".into()));
        }
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for spec in &self.functions {
            if !identifier(&spec.id)
                || !identifier(&spec.name)
                || !ids.insert(spec.id.to_ascii_uppercase())
                || !names.insert(spec.name.to_ascii_uppercase())
            {
                return Err(JsHostError::Metadata(format!(
                    "invalid or duplicate function ID or name: {}",
                    spec.id
                )));
            }
            if spec.parameters.iter().enumerate().any(|(i, param)| {
                param.name.is_empty() || (param.repeating && i + 1 != spec.parameters.len())
            }) {
                return Err(JsHostError::Metadata(format!(
                    "unnamed parameter or repeating parameter before the last one: {}",
                    spec.id
                )));
            }
            if spec.options.requires_parameter_addresses
                && spec.result.dimensionality != Dimensionality::Matrix
            {
                return Err(JsHostError::Metadata(format!(
                    "requiresParameterAddresses needs a matrix result: {}",
                    spec.id
                )));
            }
        }
        Ok(())
    }

    /// Name used in the worksheet, distinct from the association ID in the script.
    pub fn worksheet_name(&self, spec: &FunctionSpec) -> String {
        format!("{}.{}", self.namespace, spec.name)
    }
}

fn identifier(value: &str) -> bool {
    value.split('.').all(|part| {
        !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    })
}

/// Declaration of one function in the metadata file.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionSpec {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub help_url: Option<String>,
    pub parameters: Vec<Parameter>,
    pub result: FunctionResult,
    #[serde(default)]
    pub options: FunctionOptions,
}

/// An input parameter in declared order.
#[derive(Clone, Debug, Deserialize)]
pub struct Parameter {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "type", default)]
    pub data_type: DataType,
    #[serde(default)]
    pub dimensionality: Dimensionality,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub repeating: bool,
}

/// Worksheet result shape and help text.
#[derive(Clone, Debug, Deserialize)]
pub struct FunctionResult {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub dimensionality: Dimensionality,
}

/// Metadata options supplied by the add-in.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FunctionOptions {
    pub volatile: bool,
    pub stream: bool,
    pub cancelable: bool,
    pub requires_address: bool,
    pub requires_parameter_addresses: bool,
    pub support_sync: bool,
}

/// Worksheet values admitted at the JavaScript boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataType {
    #[default]
    Any,
    Boolean,
    Number,
    String,
}

/// The expected shape of a worksheet parameter or result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dimensionality {
    #[default]
    Scalar,
    Matrix,
}
