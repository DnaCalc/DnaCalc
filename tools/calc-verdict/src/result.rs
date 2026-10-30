//! The result set: what the code under test published for each cell, with the provenance of each
//! value.
//!
//! The provenance vocabulary is this library's own; CalcEngine's `ValueProvenance` (plan section
//! 2.2) maps onto it where the engine's results are handed over. A `calculated` provenance is a
//! claim, not proof: [`crate::judge`] counts a result as calculated only when it was calculated at
//! the run's revision and under the run's context and the separate evaluation record
//! ([`crate::EvaluationRecord`]), which the publisher of results does not write, holds an
//! evaluation of that cell at that tick, revision and context. The JSON form:
//!
//! ```text
//! {"format": "dnacalc-result-set", "version": 1,
//!  "case": "<case id>", "source": "<content hash of the input workbook>",
//!  "run": {"revision": <authored revision calculated>, "context": "<context fingerprint>"},
//!  "results": [{"sheet": "Sheet1", "row": 1, "column": 1,
//!               "provenance": {"calculated": {"tick": t, "revision": r, "context": "...",
//!                                             "external_links": true}}
//!                           | {"circular_rule": {"tick": t, "revision": r, "context": "...",
//!                                                "external_links": true}}
//!                           | {"file_cached": {"source": "..."}}
//!                           | {"stale": {"last_calculated": t | null, "cause": "..."}}
//!                           | {"not_modeled": {"feature": "...", "diagnostic": "..."}}
//!                           | {"missing": {"reason": "..."}}
//!                           | {"failed": {"fault": "..."}},
//!               "value": <value>}, ...]}
//! ```
//!
//! A calculated or file-cached result carries a value; a stale one may; a not-modeled, missing or
//! failed one carries none. A calculated result's `external_links` member, written only when
//! `true` (dc-ylr), says that the calculation read values another workbook's link caches in the
//! file hold, directly or through the formulas it read: calculated, from inputs that are the file's
//! cache of the other book rather than live ones. The verdict counts such cells apart
//! ([`crate::Counts::external_links`]) so a reader never takes them for plain passes.
//!
//! A `circular_rule` result (dc-y2o.1) is the value the circular-reference rule holds with
//! iteration off: the formula is on a circular reference, or reads one, so it was not evaluated,
//! and its cell keeps what it held, as Excel keeps it. No evaluation proves it, since none ran; it
//! is judged against the observation like a calculated result at the same revision and context,
//! and counted apart ([`crate::Counts::circular_rule`]), never as a plain pass.

use crate::DecodeError;
use crate::capture::{CellAddress, check_format};
use crate::json::{self, Fields, Node};
use crate::typed::decode_value;
use crate::value::CalcValue;

pub const RESULT_FORMAT: &str = "dnacalc-result-set";
pub const RESULT_VERSION: u64 = 1;

/// Where a published value came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Provenance {
    /// Calculated at `tick`, from the authored inputs of `revision`, under the calculation context
    /// whose fingerprint is `context`.
    Calculated {
        tick: u64,
        revision: u64,
        context: String,
        /// The calculation read values another workbook's link caches hold (dc-ylr).
        external_links: bool,
    },
    /// The value the circular-reference rule held at `tick` (dc-y2o.1): with iteration off, a
    /// formula on a circular reference, or one that reads such a formula, is not evaluated and
    /// keeps what its cell held. No evaluation proves it; it is counted apart.
    CircularRule {
        tick: u64,
        revision: u64,
        context: String,
        /// The value held was calculated from values another workbook's link caches hold.
        external_links: bool,
    },
    /// The value the workbook file cached, not calculated; `source` identifies the file.
    FileCached { source: String },
    /// Calculated once (at `last_calculated`, when known) and invalidated since, for `cause`.
    Stale {
        last_calculated: Option<u64>,
        cause: String,
    },
    /// A typed not-modeled result: the formula uses `feature`, which the code under test does not
    /// model.
    NotModeled { feature: String, diagnostic: String },
    /// The code under test published nothing for the cell, for `reason`.
    Missing { reason: String },
    /// The evaluation failed with an infrastructure or admission fault (not a worksheet error
    /// value, which is a calculated result).
    Failed { fault: String },
}

const PROVENANCES: [&str; 7] = [
    "calculated",
    "circular_rule",
    "file_cached",
    "stale",
    "not_modeled",
    "missing",
    "failed",
];

impl Provenance {
    fn decode(node: &Node, path: &str) -> Result<Provenance, DecodeError> {
        let (kind, inner) = json::one_member(node, path, &PROVENANCES)?;
        let at = format!("{path}.{kind}");
        let string = |fields: &Fields<'_>, name: &str| -> Result<String, DecodeError> {
            Ok(json::name(fields.required(name)?, &fields.at(name))?.to_owned())
        };
        match kind {
            "calculated" | "circular_rule" => {
                let fields = Fields::new(
                    inner,
                    &at,
                    &["tick", "revision", "context", "external_links"],
                )?;
                let tick = json::unsigned(fields.required("tick")?, &fields.at("tick"))?;
                let revision =
                    json::unsigned(fields.required("revision")?, &fields.at("revision"))?;
                let context = string(&fields, "context")?;
                let external_links = fields
                    .optional("external_links")
                    .map(|node| json::boolean(node, &fields.at("external_links")))
                    .transpose()?
                    .unwrap_or(false);
                Ok(if kind == "calculated" {
                    Self::Calculated {
                        tick,
                        revision,
                        context,
                        external_links,
                    }
                } else {
                    Self::CircularRule {
                        tick,
                        revision,
                        context,
                        external_links,
                    }
                })
            }
            "file_cached" => {
                let fields = Fields::new(inner, &at, &["source"])?;
                Ok(Self::FileCached {
                    source: string(&fields, "source")?,
                })
            }
            "stale" => {
                let fields = Fields::new(inner, &at, &["last_calculated", "cause"])?;
                let last_calculated = match fields.required("last_calculated")? {
                    Node::Null => None,
                    node => Some(json::unsigned(node, &fields.at("last_calculated"))?),
                };
                Ok(Self::Stale {
                    last_calculated,
                    cause: string(&fields, "cause")?,
                })
            }
            "not_modeled" => {
                let fields = Fields::new(inner, &at, &["feature", "diagnostic"])?;
                Ok(Self::NotModeled {
                    feature: string(&fields, "feature")?,
                    diagnostic: string(&fields, "diagnostic")?,
                })
            }
            "missing" => {
                let fields = Fields::new(inner, &at, &["reason"])?;
                Ok(Self::Missing {
                    reason: string(&fields, "reason")?,
                })
            }
            _ => {
                let fields = Fields::new(inner, &at, &["fault"])?;
                Ok(Self::Failed {
                    fault: string(&fields, "fault")?,
                })
            }
        }
    }

    pub(crate) fn name(&self) -> &'static str {
        match self {
            Provenance::Calculated { .. } => "calculated",
            Provenance::CircularRule { .. } => "circular_rule",
            Provenance::FileCached { .. } => "file_cached",
            Provenance::Stale { .. } => "stale",
            Provenance::NotModeled { .. } => "not_modeled",
            Provenance::Missing { .. } => "missing",
            Provenance::Failed { .. } => "failed",
        }
    }

    /// Whether a result of this provenance must, may or must not carry a value: `Some(true)`
    /// must, `Some(false)` must not, `None` may.
    pub(crate) fn value_rule(&self) -> Option<bool> {
        match self {
            Provenance::Calculated { .. }
            | Provenance::CircularRule { .. }
            | Provenance::FileCached { .. } => Some(true),
            Provenance::Stale { .. } => None,
            Provenance::NotModeled { .. }
            | Provenance::Missing { .. }
            | Provenance::Failed { .. } => Some(false),
        }
    }
}

/// The revision and calculation context the run calculated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunIdentity {
    pub revision: u64,
    pub context: String,
}

/// What the code under test published for one cell.
#[derive(Clone, Debug, PartialEq)]
pub struct CellResult {
    pub cell: CellAddress,
    pub provenance: Provenance,
    pub value: Option<CalcValue>,
}

/// A result set. Its consistency (one result per cell, each value present or absent as its
/// provenance requires) is checked by [`crate::judge`].
#[derive(Clone, Debug, PartialEq)]
pub struct ResultSet {
    pub case: String,
    pub source: String,
    pub run: RunIdentity,
    pub results: Vec<CellResult>,
}

impl ResultSet {
    /// The result set in `bytes` (its JSON form, described in this module's documentation).
    pub fn decode(bytes: &[u8]) -> Result<ResultSet, DecodeError> {
        let root = json::parse(bytes, "results")?;
        let fields = Fields::new(
            &root,
            "results",
            &["format", "version", "case", "source", "run", "results"],
        )?;
        check_format(&fields, RESULT_FORMAT, RESULT_VERSION)?;
        let case = json::name(fields.required("case")?, &fields.at("case"))?.to_owned();
        let source = json::name(fields.required("source")?, &fields.at("source"))?.to_owned();
        let run_fields = Fields::new(
            fields.required("run")?,
            &fields.at("run"),
            &["revision", "context"],
        )?;
        let run = RunIdentity {
            revision: json::unsigned(run_fields.required("revision")?, &run_fields.at("revision"))?,
            context: json::name(run_fields.required("context")?, &run_fields.at("context"))?
                .to_owned(),
        };

        let results_at = fields.at("results");
        let results = json::array(fields.required("results")?, &results_at)?
            .iter()
            .enumerate()
            .map(|(i, node)| {
                let path = format!("{results_at}[{i}]");
                let result = Fields::new(
                    node,
                    &path,
                    &["sheet", "row", "column", "provenance", "value"],
                )?;
                Ok(CellResult {
                    cell: CellAddress::decode(&result)?,
                    provenance: Provenance::decode(
                        result.required("provenance")?,
                        &result.at("provenance"),
                    )?,
                    value: result
                        .optional("value")
                        .map(|node| decode_value(node, &result.at("value")))
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>, DecodeError>>()?;

        Ok(ResultSet {
            case,
            source,
            run,
            results,
        })
    }
}
