//! `dnacalc diff`: recalculate and compare every formula cell with the file's cached values.
//!
//! The cached value is read before the recalculation (the host projects it as `FileCached`, which
//! nothing here calculated), the calculated value after it, and the two are compared with
//! `calc_verdict::compare`, the one value comparison the corpus oracle judges with. Counts follow
//! AGENTS.md honesty rule 2: pass = matched; fail = mismatched + refused + missing + failed, and
//! the rate is over all formula cells.

use crate::session::{self, provenance, value_json};
use crate::{Cli, Failure};
use calc_host::ProjectedValue;
use serde_json::{Value, json};

pub(crate) fn diff(cli: &Cli) -> Result<(Value, u8), Failure> {
    let mut book = session::open(cli)?;
    let sheets: Vec<_> = book.session.host().sheets().iter().map(|s| s.id()).collect();
    let mut formulas = Vec::new();
    for sheet in sheets {
        formulas.extend(book.session.host().formula_cells(sheet));
    }
    let cached = session::project(&book.session, formulas.clone());
    session::recalculate(&mut book.session)?;
    let calculated = session::project(&book.session, formulas.clone());

    let mut counts = [0u64; 5];
    let mut rows = Vec::new();
    for (before, after) in cached.iter().zip(&calculated) {
        let cell = session::a1(&book.session, before.at());
        let expected = match before.value() {
            ProjectedValue::FileCached(c) => Some(c.value().to_value()),
            _ => None,
        };
        let (kind, actual, note) = provenance(after.value());
        let (status, detail) = match (&expected, kind, &actual) {
            (None, ..) => ("missing", Some("the file holds no cached value for this formula".to_owned())),
            (Some(_), "not-modelled", _) => ("refused", note.clone()),
            (Some(expected), "calculated", Some(actual)) => {
                match calc_verdict::compare(expected, actual) {
                    None => ("matched", None),
                    Some(class) => ("mismatched", Some(format!("{class:?}"))),
                }
            }
            (Some(_), _, _) => ("failed", note.clone().or(Some(format!("provenance {kind}")))),
        };
        let slot = match status {
            "matched" => 0,
            "mismatched" => 1,
            "refused" => 2,
            "missing" => 3,
            _ => 4,
        };
        counts[slot] += 1;
        let mut row = json!({"cell": cell, "status": status});
        if let Some(expected) = &expected {
            row["cached"] = value_json(expected);
        }
        if let Some(actual) = &actual {
            row["calculated"] = value_json(actual);
        }
        if let Some(detail) = detail {
            row["detail"] = json!(detail);
        }
        rows.push(row);
    }
    let total = formulas.len() as u64;
    let [matched, mismatched, refused, missing, failed] = counts;
    let pass_rate = if total == 0 { 1.0 } else { matched as f64 / total as f64 };
    let code = u8::from(matched != total);
    Ok((
        json!({
            "book": cli.book,
            "macros": book.macros,
            "formula_cells": total,
            "matched": matched,
            "mismatched": mismatched,
            "refused": refused,
            "missing": missing,
            "failed": failed,
            "pass": matched,
            "fail": total - matched,
            "pass_rate": pass_rate,
            "cells": rows,
        }),
        code,
    ))
}
