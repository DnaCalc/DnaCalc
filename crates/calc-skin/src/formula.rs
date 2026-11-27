//! Requests available to a formula view without a workbook port.

use crate::{Refusal, Shown, parts::Trace};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trial {
    pub formula: String,
    pub culture: String,
    pub number_format: String,
    pub now: Option<f64>,
    #[serde(default)]
    pub inputs: Vec<TestInput>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestInput {
    pub name: String,
    pub constant: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FormulaQuery {
    Evaluate(Trial),
    Trace(Trial),
    EvaluatePart {
        trial: Trial,
        start: usize,
        len: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FormulaAnswer {
    Evaluated(Shown<TrialValue>),
    Steps(Trace),
    Part(crate::parts::PartValue),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TrialValue {
    Scalar(TrialCell),
    Array {
        rows: usize,
        columns: usize,
        cells: Vec<Vec<TrialCell>>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrialCell {
    pub raw: RawValue,
    pub formatted: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RawValue {
    Number { bits: u64 },
    Text(Vec<u16>),
    Logical(bool),
    Error(String),
    Blank,
    Other { kind: String, display: String },
}

impl RawValue {
    pub fn display(&self) -> String {
        match self {
            Self::Number { bits } => f64::from_bits(*bits).to_string(),
            Self::Text(units) => String::from_utf16_lossy(units),
            Self::Logical(true) => "TRUE".into(),
            Self::Logical(false) => "FALSE".into(),
            Self::Blank => String::new(),
            Self::Error(code) | Self::Other { display: code, .. } => code.clone(),
        }
    }
}

/// A local host can answer immediately. A bridged host can answer after the request returns.
pub trait FormulaPort {
    fn ask(&self, query: FormulaQuery, complete: Box<dyn FnOnce(Result<FormulaAnswer, Refusal>)>);
}

/// A formula-only handoff. Caller data and previous results are absent from the URL.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormulaLink(pub String);

impl FormulaLink {
    pub fn fragment(&self) -> String {
        use std::fmt::Write;
        let mut fragment = "#f=".to_owned();
        for byte in self.0.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    fragment.push(char::from(byte));
                }
                _ => write!(fragment, "%{byte:02X}").expect("writing to a string"),
            }
        }
        fragment
    }

    pub fn from_fragment(fragment: &str) -> Option<Self> {
        let mut encoded = fragment.strip_prefix("#f=")?.split('&').next()?.bytes();
        let mut decoded = Vec::new();
        while let Some(byte) = encoded.next() {
            if byte == b'%' {
                let high = char::from(encoded.next()?).to_digit(16)?;
                let low = char::from(encoded.next()?).to_digit(16)?;
                decoded.push(((high << 4) | low) as u8);
            } else {
                decoded.push(byte);
            }
        }
        String::from_utf8(decoded).ok().map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fragment_preserves_exact_formula_text() {
        let link = FormulaLink("=LET(x, \"😃+&%\",\n x)".into());
        let fragment = link.fragment();
        assert!(fragment.starts_with("#f=%3DLET"));
        assert_eq!(FormulaLink::from_fragment(&fragment), Some(link));
        assert_eq!(FormulaLink::from_fragment("#f=%FC"), None);
        assert_eq!(FormulaLink::from_fragment("#f=%x1"), None);
    }
}
