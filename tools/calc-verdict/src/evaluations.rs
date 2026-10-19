//! The evaluation record: the evaluator calls an instrument on the evaluation route observed during
//! a run. It is its own document, a separate input of [`crate::judge`], so that the code which
//! publishes results never supplies the proof that it calculated them (plan section 10.1: a status
//! flag produced by the engine cannot establish that evaluation occurred). A result whose
//! provenance claims calculation counts as calculated only when this record holds an evaluation of
//! the same cell at the claim's tick, revision and context.
//!
//! Producing the record from instrumented evaluator calls is bead `dc-qguw`; until it lands, the
//! records judged here are authored fixtures. The JSON form:
//!
//! ```text
//! {"format": "dnacalc-evaluation-record", "version": 1,
//!  "case": "<case id>", "source": "<content hash of the input workbook>",
//!  "evaluations": [{"sheet": "Sheet1", "row": 1, "column": 1,
//!                   "tick": <calc tick>, "revision": <authored revision>,
//!                   "context": "<context fingerprint>"}, ...]}
//! ```

use crate::DecodeError;
use crate::capture::{CellAddress, check_format};
use crate::json::{self, Fields};

pub const EVALUATION_FORMAT: &str = "dnacalc-evaluation-record";
pub const EVALUATION_VERSION: u64 = 1;

/// One evaluator call the instrument observed: the cell evaluated, the tick it ran at, the
/// authored revision its inputs came from and the calculation context it ran under.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evaluation {
    pub cell: CellAddress,
    pub tick: u64,
    pub revision: u64,
    pub context: String,
}

/// An evaluation record. Its consistency (no evaluation listed twice, the same case and source as
/// the capture record) is checked by [`crate::judge`].
#[derive(Clone, Debug, PartialEq)]
pub struct EvaluationRecord {
    pub case: String,
    pub source: String,
    pub evaluations: Vec<Evaluation>,
}

impl EvaluationRecord {
    /// The record in `bytes` (its JSON form, described in this module's documentation).
    pub fn decode(bytes: &[u8]) -> Result<EvaluationRecord, DecodeError> {
        let root = json::parse(bytes, "evaluations")?;
        let fields = Fields::new(
            &root,
            "evaluations",
            &["format", "version", "case", "source", "evaluations"],
        )?;
        check_format(&fields, EVALUATION_FORMAT, EVALUATION_VERSION)?;
        let case = json::name(fields.required("case")?, &fields.at("case"))?.to_owned();
        let source = json::name(fields.required("source")?, &fields.at("source"))?.to_owned();
        let evaluations_at = fields.at("evaluations");
        let evaluations = json::array(fields.required("evaluations")?, &evaluations_at)?
            .iter()
            .enumerate()
            .map(|(i, node)| {
                let path = format!("{evaluations_at}[{i}]");
                let evaluation = Fields::new(
                    node,
                    &path,
                    &["sheet", "row", "column", "tick", "revision", "context"],
                )?;
                Ok(Evaluation {
                    cell: CellAddress::decode(&evaluation)?,
                    tick: json::unsigned(evaluation.required("tick")?, &evaluation.at("tick"))?,
                    revision: json::unsigned(
                        evaluation.required("revision")?,
                        &evaluation.at("revision"),
                    )?,
                    context: json::name(
                        evaluation.required("context")?,
                        &evaluation.at("context"),
                    )?
                    .to_owned(),
                })
            })
            .collect::<Result<Vec<_>, DecodeError>>()?;
        Ok(EvaluationRecord {
            case,
            source,
            evaluations,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORD: &str = r#"{"format": "dnacalc-evaluation-record", "version": 1, "case": "c", "source": "s",
        "evaluations": [{"sheet": "S", "row": 2, "column": 3, "tick": 7, "revision": 4, "context": "k"}]}"#;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn evaluations_carry_cell_tick_revision_and_context() {
        let record = EvaluationRecord::decode(RECORD.as_bytes()).unwrap();
        assert_eq!(record.case, "c");
        assert_eq!(record.source, "s");
        assert_eq!(
            record.evaluations,
            vec![Evaluation {
                cell: CellAddress::new("S", 2, 3).unwrap(),
                tick: 7,
                revision: 4,
                context: "k".to_owned(),
            }]
        );
        for (from, to, path) in [
            (
                "dnacalc-evaluation-record",
                "dnacalc-result-set",
                "evaluations.format",
            ),
            (r#", "context": "k""#, "", "evaluations.evaluations[0]"),
            (
                r#""revision": 4"#,
                r#""revision": "4""#,
                "evaluations.evaluations[0].revision",
            ),
            (
                r#""context": "k""#,
                r#""context": """#,
                "evaluations.evaluations[0].context",
            ),
            (
                r#""tick": 7"#,
                r#""tick": 7, "value": {"number": "4000000000000000"}"#,
                "evaluations.evaluations[0]",
            ),
        ] {
            let text = RECORD.replace(from, to);
            assert_ne!(text, RECORD, "{from} not found");
            let error = EvaluationRecord::decode(text.as_bytes()).err().unwrap();
            assert_eq!(error.path, path, "{error}");
        }
    }
}
