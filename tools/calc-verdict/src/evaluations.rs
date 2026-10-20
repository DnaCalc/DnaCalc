//! The evaluation record: the evaluations an instrument on the evaluation route observed during a
//! run. It is its own document, a separate input of [`crate::judge`], so that the code which
//! publishes results need not supply the proof that it calculated them (plan section 10.1: a
//! status flag produced by the engine cannot establish that evaluation occurred); that holds only
//! for a record made from the evaluator's reports, which judge cannot check. A result whose
//! provenance claims calculation counts as calculated only when this record holds an evaluation of
//! the same cell at the claim's tick, revision and context, and, in a version 2 record, only when
//! that evaluation returned the value the result publishes.
//!
//! Version 2 ([`InstrumentRecord`]) carries, for each evaluation, what the evaluator returned: a
//! value or no value with the reason. [`crate::judge`] checks that returned value against the
//! published one; it cannot check who wrote the record, since the type is public and tests build
//! records by hand. Provenance comes from the writer: CalcEngine's `step` shows a `ReadObserver`
//! the report CalcFml's evaluator makes for each evaluation it runs (a value only CalcFml can
//! build), and the record is to be made from those reports, not from the results the engine
//! publishes. Today only the dc-qguw acceptance cases write a version 2 record that way; the
//! product writer is infra-m1.3's round trip.
//! Version 1 ([`EvaluationRecord`]) carries no returned value: an evaluation it lists proves that
//! the cell was evaluated at that tick, not which value the evaluation gave, so a result that
//! publishes another value than its evaluation returned is not caught. The version 1 records judged
//! today are authored: the fixtures of this package and of `cargo xtask verdict`, the empty record
//! of [`crate::file_cache_replay`], and the records other packages' cases build from their own call
//! sites. A report judged against one says so
//! (its `evaluation_record_version`, and `calculated_without_value_check` counting the calculated
//! cells proven without a value check); bead `dc-i1ro` moves them to version 2 and refuses
//! version 1.
//!
//! The JSON form:
//!
//! ```text
//! {"format": "dnacalc-evaluation-record", "version": 1 | 2,
//!  "case": "<case id>", "source": "<content hash of the input workbook>",
//!  "evaluations": [{"sheet": "Sheet1", "row": 1, "column": 1,
//!                   "tick": <calc tick>, "revision": <authored revision>,
//!                   "context": "<context fingerprint>",
//!                   "returned": {"value": <value>} | {"no_value": "<why>"}}, ...]}
//! ```
//!
//! `returned` is required in version 2 and refused in version 1. `<value>` is the canonical typed
//! value of `typed.rs` (numbers as binary64 bits in hex); `no_value` says why the evaluation gave
//! none, such as the evaluator's typed refusal, or what it returned when that is a value no cell
//! holds (a reference or an omitted argument, [`InstrumentedEvaluation::new`]).

use crate::DecodeError;
use crate::capture::{CellAddress, EncodeError, check_format};
use crate::json::{self, Fields, Out};
use crate::typed::{decode_value, representable, value_out};
use crate::value::{CalcValue, CoreValue};

pub const EVALUATION_FORMAT: &str = "dnacalc-evaluation-record";
/// The version [`InstrumentRecord::encode`] writes. Version 1, which [`EvaluationRecord::decode`]
/// reads, carries no returned values.
pub const EVALUATION_VERSION: u64 = 2;

const RECORD_MEMBERS: [&str; 5] = ["format", "version", "case", "source", "evaluations"];
const VERSION_1_ENTRY: [&str; 6] = ["sheet", "row", "column", "tick", "revision", "context"];
const VERSION_2_ENTRY: [&str; 7] = [
    "sheet", "row", "column", "tick", "revision", "context", "returned",
];

/// One evaluator call the instrument observed: the cell evaluated, the tick it ran at, the
/// authored revision its inputs came from and the calculation context it ran under.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evaluation {
    pub cell: CellAddress,
    pub tick: u64,
    pub revision: u64,
    pub context: String,
}

/// A version 1 evaluation record: evaluations without what they returned. Its consistency (no
/// evaluation listed twice, the same case and source as the capture record) is checked by
/// [`crate::judge`].
#[derive(Clone, Debug, PartialEq)]
pub struct EvaluationRecord {
    pub case: String,
    pub source: String,
    pub evaluations: Vec<Evaluation>,
}

/// One evaluation an instrument on the evaluator recorded, with what the evaluator returned:
/// `Ok` with the value, or `Err` with why it returned none.
#[derive(Clone, Debug, PartialEq)]
pub struct InstrumentedEvaluation {
    pub evaluation: Evaluation,
    pub returned: Result<CalcValue, String>,
}

impl InstrumentedEvaluation {
    /// The entry a writer of the record makes for `evaluation`, which returned `returned` (`Err`
    /// with why it returned no value). A returned value the record's canonical form does not hold
    /// (an omitted argument, a reference or an array inside an array, none of which a cell holds)
    /// is recorded as no value, saying what was returned, so that [`InstrumentRecord::encode`]
    /// writes the entry; an entry that holds such a value as `Ok` is refused by it.
    pub fn new(evaluation: Evaluation, returned: Result<CalcValue, String>) -> Self {
        let returned = match returned {
            Ok(value) if !representable(&value) => Err(format!(
                "returned {}, which the evaluation record's value form does not hold",
                unrepresentable_kind(&value)
            )),
            other => other,
        };
        InstrumentedEvaluation {
            evaluation,
            returned,
        }
    }
}

/// What a value the canonical form does not hold is, in the words of [`InstrumentedEvaluation::new`].
fn unrepresentable_kind(value: &CalcValue) -> &'static str {
    match value.core() {
        CoreValue::Missing => "an omitted argument",
        CoreValue::Reference(_) => "a reference",
        _ => "an array holding an omitted argument, a reference or an array",
    }
}

/// A version 2 evaluation record: the evaluations an instrument on the evaluator recorded, each
/// with what it returned. Its consistency is checked by [`crate::judge`], as a version 1 record's
/// is.
#[derive(Clone, Debug, PartialEq)]
pub struct InstrumentRecord {
    pub case: String,
    pub source: String,
    pub evaluations: Vec<InstrumentedEvaluation>,
}

/// The `case` and `source` members of a record, and its `evaluations` array with each entry's
/// members read as `entry` reads them. `version` is the version the record must have.
fn decode_record<T>(
    bytes: &[u8],
    version: u64,
    entry: impl Fn(&Fields<'_>) -> Result<T, DecodeError>,
    entry_members: &[&str],
) -> Result<(String, String, Vec<T>), DecodeError> {
    let root = json::parse(bytes, "evaluations")?;
    let fields = Fields::new(&root, "evaluations", &RECORD_MEMBERS)?;
    check_format(&fields, EVALUATION_FORMAT, version)?;
    let case = json::name(fields.required("case")?, &fields.at("case"))?.to_owned();
    let source = json::name(fields.required("source")?, &fields.at("source"))?.to_owned();
    let evaluations_at = fields.at("evaluations");
    let evaluations = json::array(fields.required("evaluations")?, &evaluations_at)?
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let path = format!("{evaluations_at}[{i}]");
            entry(&Fields::new(node, &path, entry_members)?)
        })
        .collect::<Result<Vec<_>, DecodeError>>()?;
    Ok((case, source, evaluations))
}

/// The cell, tick, revision and context of an entry.
fn decode_evaluation(evaluation: &Fields<'_>) -> Result<Evaluation, DecodeError> {
    Ok(Evaluation {
        cell: CellAddress::decode(evaluation)?,
        tick: json::unsigned(evaluation.required("tick")?, &evaluation.at("tick"))?,
        revision: json::unsigned(evaluation.required("revision")?, &evaluation.at("revision"))?,
        context: json::name(evaluation.required("context")?, &evaluation.at("context"))?.to_owned(),
    })
}

impl EvaluationRecord {
    /// The version 1 record in `bytes` (its JSON form, described in this module's documentation).
    /// A version 2 record is read with [`InstrumentRecord::decode`].
    pub fn decode(bytes: &[u8]) -> Result<EvaluationRecord, DecodeError> {
        let (case, source, evaluations) =
            decode_record(bytes, 1, decode_evaluation, &VERSION_1_ENTRY)?;
        Ok(EvaluationRecord {
            case,
            source,
            evaluations,
        })
    }
}

impl InstrumentRecord {
    /// The version 2 record in `bytes` (its JSON form, described in this module's documentation).
    /// A version 1 record is read with [`EvaluationRecord::decode`].
    pub fn decode(bytes: &[u8]) -> Result<InstrumentRecord, DecodeError> {
        let (case, source, evaluations) = decode_record(
            bytes,
            EVALUATION_VERSION,
            |evaluation| {
                let at = evaluation.at("returned");
                let (kind, inner) = json::one_member(
                    evaluation.required("returned")?,
                    &at,
                    &["value", "no_value"],
                )?;
                let at = format!("{at}.{kind}");
                let returned = if kind == "value" {
                    Ok(decode_value(inner, &at)?)
                } else {
                    Err(json::name(inner, &at)?.to_owned())
                };
                Ok(InstrumentedEvaluation {
                    evaluation: decode_evaluation(evaluation)?,
                    returned,
                })
            },
            &VERSION_2_ENTRY,
        )?;
        Ok(InstrumentRecord {
            case,
            source,
            evaluations,
        })
    }

    /// The record as a version 2 JSON document (this module's documentation), members in the
    /// order the documentation lists them. Refuses what the decoder would refuse: an empty case,
    /// source, context or reason for no value, and an `Ok` returned value the canonical form does
    /// not hold (an omitted argument, a reference or an array inside an array). A writer makes
    /// each entry with [`InstrumentedEvaluation::new`], which records such a value as `no_value`,
    /// saying what was returned, and this writes that entry.
    pub fn encode(&self) -> Result<String, EncodeError> {
        let refuse = |message: String| Err(EncodeError { message });
        if self.case.is_empty() || self.source.is_empty() {
            return refuse("the evaluation record's case and source must not be empty".to_owned());
        }
        let mut evaluations = Vec::with_capacity(self.evaluations.len());
        for entry in &self.evaluations {
            let evaluation = &entry.evaluation;
            let place = format!(
                "the evaluation of {}!{} at tick {}",
                evaluation.cell.sheet(),
                evaluation.cell.a1(),
                evaluation.tick
            );
            if evaluation.context.is_empty() {
                return refuse(format!("{place} has an empty context"));
            }
            let returned = match &entry.returned {
                Ok(value) if representable(value) => vec![("value", value_out(value))],
                Ok(_) => {
                    return refuse(format!(
                        "{place} returned a value the canonical form does not hold; InstrumentedEvaluation::new records it as no_value"
                    ));
                }
                Err(why) if why.is_empty() => {
                    return refuse(format!("{place} returned no value and no reason"));
                }
                Err(why) => vec![("no_value", Out::text(why.as_str()))],
            };
            let mut members = evaluation.cell.out_members();
            members.extend([
                ("tick", Out::Unsigned(evaluation.tick)),
                ("revision", Out::Unsigned(evaluation.revision)),
                ("context", Out::text(evaluation.context.as_str())),
                ("returned", Out::Object(returned)),
            ]);
            evaluations.push(Out::Object(members));
        }
        Ok(Out::Object(vec![
            ("format", Out::text(EVALUATION_FORMAT)),
            ("version", Out::Unsigned(EVALUATION_VERSION)),
            ("case", Out::text(self.case.as_str())),
            ("source", Out::text(self.source.as_str())),
            ("evaluations", Out::Array(evaluations)),
        ])
        .document())
    }
}

/// The version of the evaluation record in `bytes`, 1 or 2, read from its `format` and `version`
/// members only, so that a reader can choose [`EvaluationRecord::decode`] or
/// [`InstrumentRecord::decode`]. Refuses bytes that are not an evaluation record of either version.
pub fn evaluation_record_version(bytes: &[u8]) -> Result<u64, DecodeError> {
    let root = json::parse(bytes, "evaluations")?;
    let fields = Fields::new(&root, "evaluations", &RECORD_MEMBERS)?;
    let format = json::text(fields.required("format")?, &fields.at("format"))?;
    if format != EVALUATION_FORMAT {
        return Err(DecodeError::new(
            &fields.at("format"),
            format!("is {format:?}, expected {EVALUATION_FORMAT:?}"),
        ));
    }
    match json::unsigned(fields.required("version")?, &fields.at("version"))? {
        version @ (1 | EVALUATION_VERSION) => Ok(version),
        other => Err(DecodeError::new(
            &fields.at("version"),
            format!("is {other}, expected 1 or {EVALUATION_VERSION}"),
        )),
    }
}

/// What [`crate::judge`] takes as its proof of calculation: an evaluation record of either
/// version, borrowed. It is made with `From` from a reference to either record.
#[derive(Clone, Copy, Debug)]
pub struct EvaluationProof<'e>(Proof<'e>);

#[derive(Clone, Copy, Debug)]
enum Proof<'e> {
    Listed(&'e EvaluationRecord),
    Instrumented(&'e InstrumentRecord),
}

impl<'e> From<&'e EvaluationRecord> for EvaluationProof<'e> {
    fn from(record: &'e EvaluationRecord) -> Self {
        EvaluationProof(Proof::Listed(record))
    }
}

impl<'e> From<&'e InstrumentRecord> for EvaluationProof<'e> {
    fn from(record: &'e InstrumentRecord) -> Self {
        EvaluationProof(Proof::Instrumented(record))
    }
}

impl<'e> EvaluationProof<'e> {
    /// The record's version: 1 for an [`EvaluationRecord`], which says nothing of what its
    /// evaluations returned, and [`EVALUATION_VERSION`] for an [`InstrumentRecord`].
    pub fn version(self) -> u64 {
        match self.0 {
            Proof::Listed(_) => 1,
            Proof::Instrumented(_) => EVALUATION_VERSION,
        }
    }

    /// The record's case and source.
    pub(crate) fn identity(self) -> (&'e str, &'e str) {
        match self.0 {
            Proof::Listed(record) => (&record.case, &record.source),
            Proof::Instrumented(record) => (&record.case, &record.source),
        }
    }

    /// Each evaluation the record lists, in its order, with what it returned when the record
    /// says (version 2); `None` for every evaluation of a version 1 record.
    pub(crate) fn entries(self) -> Vec<(&'e Evaluation, Option<&'e Result<CalcValue, String>>)> {
        match self.0 {
            Proof::Listed(record) => record
                .evaluations
                .iter()
                .map(|evaluation| (evaluation, None))
                .collect(),
            Proof::Instrumented(record) => record
                .evaluations
                .iter()
                .map(|entry| (&entry.evaluation, Some(&entry.returned)))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{
        CellKey, ExcelText, IdAllocator, ReferenceLike, SheetId, WorksheetErrorCode,
    };

    const RECORD: &str = r#"{"format": "dnacalc-evaluation-record", "version": 1, "case": "c", "source": "s",
        "evaluations": [{"sheet": "S", "row": 2, "column": 3, "tick": 7, "revision": 4, "context": "k"}]}"#;

    const INSTRUMENTED: &str = r#"{"format": "dnacalc-evaluation-record", "version": 2, "case": "c", "source": "s",
        "evaluations": [
          {"sheet": "S", "row": 2, "column": 3, "tick": 7, "revision": 4, "context": "k", "returned": {"value": {"number": "4000000000000000"}}},
          {"sheet": "S", "row": 3, "column": 3, "tick": 7, "revision": 4, "context": "k", "returned": {"no_value": "refused"}}]}"#;

    fn evaluation(row: u32) -> Evaluation {
        Evaluation {
            cell: CellAddress::new("S", row, 3).unwrap(),
            tick: 7,
            revision: 4,
            context: "k".to_owned(),
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn evaluations_carry_cell_tick_revision_and_context() {
        let record = EvaluationRecord::decode(RECORD.as_bytes()).unwrap();
        assert_eq!(record.case, "c");
        assert_eq!(record.source, "s");
        assert_eq!(record.evaluations, vec![evaluation(2)]);
        assert_eq!(evaluation_record_version(RECORD.as_bytes()), Ok(1));
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
            // A version 1 record carries no returned value, and version 2 is not read as 1.
            (
                r#""tick": 7"#,
                r#""tick": 7, "returned": {"no_value": "x"}"#,
                "evaluations.evaluations[0]",
            ),
            (r#""version": 1"#, r#""version": 2"#, "evaluations.version"),
        ] {
            let text = RECORD.replace(from, to);
            assert_ne!(text, RECORD, "{from} not found");
            let error = EvaluationRecord::decode(text.as_bytes()).err().unwrap();
            assert_eq!(error.path, path, "{error}");
        }
    }

    /// A version 2 record carries what each evaluation returned, and writes back to the bytes it
    /// was read from, in this module's layout.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn instrumented_evaluations_carry_what_they_returned() {
        let record = InstrumentRecord::decode(INSTRUMENTED.as_bytes()).unwrap();
        assert_eq!(evaluation_record_version(INSTRUMENTED.as_bytes()), Ok(2));
        assert_eq!(
            record,
            InstrumentRecord {
                case: "c".to_owned(),
                source: "s".to_owned(),
                evaluations: vec![
                    InstrumentedEvaluation {
                        evaluation: evaluation(2),
                        returned: Ok(CalcValue::number(2.0)),
                    },
                    InstrumentedEvaluation {
                        evaluation: evaluation(3),
                        returned: Err("refused".to_owned()),
                    },
                ],
            }
        );
        let written = record.encode().unwrap();
        assert_eq!(
            InstrumentRecord::decode(written.as_bytes()),
            Ok(record.clone())
        );
        assert_eq!(
            written,
            concat!(
                "{\n",
                "  \"format\": \"dnacalc-evaluation-record\",\n",
                "  \"version\": 2,\n",
                "  \"case\": \"c\",\n",
                "  \"source\": \"s\",\n",
                "  \"evaluations\": [\n",
                "    {\"sheet\":\"S\",\"row\":2,\"column\":3,\"tick\":7,\"revision\":4,\"context\":\"k\",\"returned\":{\"value\":{\"number\":\"4000000000000000\"}}},\n",
                "    {\"sheet\":\"S\",\"row\":3,\"column\":3,\"tick\":7,\"revision\":4,\"context\":\"k\",\"returned\":{\"no_value\":\"refused\"}}\n",
                "  ]\n",
                "}\n"
            )
        );
        // Every kind of cell value round-trips.
        let mut kinds = record.clone();
        kinds.evaluations[0].returned =
            Ok(CalcValue::text(ExcelText::from_utf16_code_units(vec![
                0x61, 0xD800,
            ])));
        kinds.evaluations[1].returned = Ok(CalcValue::error(WorksheetErrorCode::Div0));
        assert_eq!(
            InstrumentRecord::decode(kinds.encode().unwrap().as_bytes()),
            Ok(kinds)
        );

        for (from, to, path) in [
            (
                r#", "returned": {"no_value": "refused"}"#,
                "",
                "evaluations.evaluations[1]",
            ),
            (
                r#""no_value": "refused""#,
                r#""no_value": """#,
                "evaluations.evaluations[1].returned.no_value",
            ),
            (
                r#""returned": {"no_value": "refused"}"#,
                r#""returned": {"no_value": "refused", "value": {"logical": true}}"#,
                "evaluations.evaluations[1].returned",
            ),
            (
                r#""returned": {"no_value": "refused"}"#,
                r#""returned": {"refused": "x"}"#,
                "evaluations.evaluations[1].returned",
            ),
            (
                r#""number": "4000000000000000""#,
                r#""number": "2""#,
                "evaluations.evaluations[0].returned.value.number",
            ),
            (r#""version": 2"#, r#""version": 1"#, "evaluations.version"),
        ] {
            let text = INSTRUMENTED.replace(from, to);
            assert_ne!(text, INSTRUMENTED, "{from} not found");
            let error = InstrumentRecord::decode(text.as_bytes()).err().unwrap();
            assert_eq!(error.path, path, "{error}");
        }
        let error = evaluation_record_version(
            INSTRUMENTED
                .replace(r#""version": 2"#, r#""version": 3"#)
                .as_bytes(),
        )
        .err()
        .unwrap();
        assert_eq!(error.path, "evaluations.version", "{error}");

        // What the decoder refuses is not written.
        for (change, needle) in [
            (
                (|r: &mut InstrumentRecord| r.evaluations[0].returned = Ok(CalcValue::missing()))
                    as fn(&mut InstrumentRecord),
                "does not hold",
            ),
            (
                |r: &mut InstrumentRecord| r.evaluations[1].returned = Err(String::new()),
                "no reason",
            ),
            (
                |r: &mut InstrumentRecord| r.evaluations[1].evaluation.context.clear(),
                "empty context",
            ),
            (
                |r: &mut InstrumentRecord| r.source.clear(),
                "must not be empty",
            ),
        ] {
            let mut refused = record.clone();
            change(&mut refused);
            let error = refused.encode().unwrap_err();
            assert!(error.message.contains(needle), "{error}");
        }
    }
    /// A writer records an evaluation that returned a value no cell holds (a reference, an
    /// omitted argument) as no value, saying what was returned, and the record writes that entry;
    /// an entry that holds such a value as returned is refused, never written.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_returned_value_no_cell_holds_is_recorded_as_no_value() {
        let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
        let a1 = CellKey::new(sheets.issue().unwrap(), 1, 1).unwrap();
        let reference = CalcValue::reference(ReferenceLike::cell(a1));
        for (value, said) in [
            (reference.clone(), "returned a reference, "),
            (CalcValue::missing(), "returned an omitted argument, "),
        ] {
            let entry = InstrumentedEvaluation::new(evaluation(2), Ok(value.clone()));
            assert_eq!(entry.evaluation, evaluation(2));
            match &entry.returned {
                Err(why) => assert!(why.starts_with(said), "{why}"),
                Ok(kept) => panic!("{kept:?} was kept as returned"),
            }
            let record = InstrumentRecord {
                case: "c".to_owned(),
                source: "s".to_owned(),
                evaluations: vec![entry],
            };
            let written = record.encode().unwrap();
            assert!(
                written.contains(r#""returned":{"no_value":"returned "#),
                "{written}"
            );
            assert_eq!(InstrumentRecord::decode(written.as_bytes()), Ok(record));
            // The same value held as returned is refused.
            let held = InstrumentRecord {
                case: "c".to_owned(),
                source: "s".to_owned(),
                evaluations: vec![InstrumentedEvaluation {
                    evaluation: evaluation(2),
                    returned: Ok(value),
                }],
            };
            let error = held.encode().unwrap_err();
            assert!(error.message.contains("does not hold"), "{error}");
        }
        // A value a cell holds and a reason for no value are kept as they are.
        for returned in [Ok(CalcValue::number(2.0)), Err("refused".to_owned())] {
            assert_eq!(
                InstrumentedEvaluation::new(evaluation(2), returned.clone()).returned,
                returned
            );
        }
    }
}
