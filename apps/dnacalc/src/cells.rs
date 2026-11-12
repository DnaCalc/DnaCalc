//! `recalc`, `get`, `set` and `run`.

use crate::session::{self, cell_json, provenance};
use crate::{Cli, Failure, fail};
use calc_host::SkinIntent;
use serde_json::{Value, json};

fn counts(book: &session::Book) -> (u64, u64, u64, u64) {
    let sheets: Vec<_> = book.session.host().sheets().iter().map(|s| s.id()).collect();
    let (mut total, mut calculated, mut refused, mut other) = (0, 0, 0, 0);
    for sheet in sheets {
        let formulas = book.session.host().formula_cells(sheet);
        for cell in session::project(&book.session, formulas) {
            total += 1;
            match provenance(cell.value()).0 {
                "calculated" => calculated += 1,
                "not-modelled" => refused += 1,
                _ => other += 1,
            }
        }
    }
    (total, calculated, refused, other)
}

fn summary(book: &session::Book, cli: &Cli, saved: Option<(&str, usize)>) -> (Value, u8) {
    let (total, calculated, refused, other) = counts(book);
    let mut report = json!({
        "book": cli.book,
        "macros": book.macros,
        "formula_cells": total,
        "calculated": calculated,
        "refused": refused,
        "not_current": other,
    });
    if let Some((path, bytes)) = saved {
        report["saved"] = json!({"path": path, "bytes": bytes});
    }
    (report, u8::from(refused + other > 0))
}

pub(crate) fn recalc(cli: &Cli) -> Result<(Value, u8), Failure> {
    if !cli.positional.is_empty() {
        return Err(fail(crate::USAGE));
    }
    let mut book = session::open(cli)?;
    session::recalculate(&mut book.session)?;
    let saved = match &cli.out {
        Some(path) => Some((path.as_str(), session::save(&mut book.session, path)?)),
        None => None,
    };
    Ok(summary(&book, cli, saved))
}

pub(crate) fn get(cli: &Cli) -> Result<(Value, u8), Failure> {
    if cli.positional.is_empty() {
        return Err(fail("get needs at least one cell"));
    }
    let mut book = session::open(cli)?;
    let at: Vec<_> = cli
        .positional
        .iter()
        .map(|t| session::parse_cell(&book.session, t))
        .collect::<Result<_, _>>()?;
    let before = session::project(&book.session, at.clone());
    session::recalculate(&mut book.session)?;
    let after = session::project(&book.session, at);
    let cells: Vec<Value> = before
        .iter()
        .zip(&after)
        .map(|(b, a)| {
            let mut row = cell_json(&book.session, a);
            if let (_, Some(cached), _) = provenance(b.value())
                && provenance(b.value()).0 == "file-cached"
            {
                row["cached"] = session::value_json(&cached);
            }
            row
        })
        .collect();
    Ok((json!({"book": cli.book, "macros": book.macros, "cells": cells}), 0))
}

pub(crate) fn set(cli: &Cli) -> Result<(Value, u8), Failure> {
    if cli.positional.is_empty() || cli.positional.len() % 2 != 0 {
        return Err(fail("set needs <cell> <text> pairs"));
    }
    let mut book = session::open(cli)?;
    let mut at = Vec::new();
    for pair in cli.positional.chunks(2) {
        let cell = session::parse_cell(&book.session, &pair[0])?;
        let receipt = book.session.dispatch(SkinIntent::EnterCell {
            at: cell,
            text: pair[1].clone(),
        });
        if !receipt.accepted() {
            return Err(fail(format!("entering {} was refused: {receipt:?}", pair[0])));
        }
        at.push(cell);
    }
    session::recalculate(&mut book.session)?;
    let saved = match &cli.out {
        Some(path) => Some((path.as_str(), session::save(&mut book.session, path)?)),
        None => None,
    };
    let after = session::project(&book.session, at);
    let cells: Vec<Value> = after.iter().map(|c| cell_json(&book.session, c)).collect();
    let (mut report, code) = summary(&book, cli, saved);
    report["cells"] = json!(cells);
    Ok((report, code))
}

/// A `--arg` as the formula text of one argument: a number or `TRUE`/`FALSE` as written, text
/// quoted.
fn argument_text(arg: &str) -> String {
    if arg.parse::<f64>().is_ok() || arg.eq_ignore_ascii_case("true") || arg.eq_ignore_ascii_case("false") {
        arg.to_owned()
    } else {
        format!("\"{}\"", arg.replace('"', "\"\""))
    }
}

pub(crate) fn run_macro(cli: &Cli) -> Result<(Value, u8), Failure> {
    let name = cli
        .macro_name
        .as_deref()
        .ok_or_else(|| fail("run needs --macro <Module.Proc>"))?;
    if cli.level == calc_macro_security::MacroSecurity::Off {
        return Err(fail(format!(
            "the macro level is off, so the file's VBA project does not run and {name} is not called; pass --macro-level safe or unguarded"
        )));
    }
    let mut book = session::open(cli)?;
    if book.project.is_none() {
        let why = book.macros["reason"]
            .as_str()
            .unwrap_or("the file has no VBA project")
            .to_owned();
        return Err(fail(format!("{name} was not called: {why}")));
    }
    let args = cli
        .macro_args
        .iter()
        .map(|a| argument_text(a))
        .collect::<Vec<_>>()
        .join(",");
    // CalcVBA publishes a project's public Functions to the sheets, so a call is made as a
    // formula `Module.Proc(args)` in a scratch cell (the last row of the first sheet), which is
    // cleared again before the book is saved. Subs are not callable here.
    let scratch = session::parse_cell(&book.session, "A1048576")?;
    let mut outcome = Err(String::new());
    for text in [
        format!("={name}({args})"),
        format!("={}({args})", name.rsplit('.').next().unwrap_or(name)),
    ] {
        let receipt = book.session.dispatch(SkinIntent::EnterCell { at: scratch, text });
        if !receipt.accepted() {
            outcome = Err(format!("the call was refused: {receipt:?}"));
            continue;
        }
        session::recalculate(&mut book.session)?;
        let cell = session::project(&book.session, vec![scratch]);
        outcome = match cell.first().map(|c| provenance(c.value())) {
            Some(("calculated", Some(value), _)) => Ok(value),
            Some((kind, _, note)) => Err(note.unwrap_or_else(|| format!("no value ({kind})"))),
            None => Err("the scratch cell is unavailable".into()),
        };
        if outcome.is_ok() {
            break;
        }
    }
    let cleared = book.session.dispatch(SkinIntent::EnterCell { at: scratch, text: String::new() });
    if !cleared.accepted() {
        return Err(fail("the scratch cell could not be cleared"));
    }
    let value = outcome.map_err(|e| fail(format!("{name} was not run: {e}")))?;
    session::recalculate(&mut book.session)?;
    let saved = match &cli.out {
        Some(path) => Some((path.as_str(), session::save(&mut book.session, path)?)),
        None => None,
    };
    let mut report = json!({
        "book": cli.book,
        "macro": name,
        "macros": book.macros,
        "result": session::value_json(&value),
    });
    if let Some((path, bytes)) = saved {
        report["saved"] = json!({"path": path, "bytes": bytes});
    }
    Ok((report, 0))
}
