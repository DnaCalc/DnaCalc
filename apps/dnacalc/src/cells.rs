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

/// A `--arg` as a procedure argument: a number or `TRUE`/`FALSE` as written, otherwise text.
fn procedure_argument(arg: &str) -> calc_host_vba::ProcedureArg {
    if let Ok(number) = arg.parse::<f64>() {
        calc_host_vba::ProcedureArg::Number(number)
    } else if arg.eq_ignore_ascii_case("true") || arg.eq_ignore_ascii_case("false") {
        calc_host_vba::ProcedureArg::Bool(arg.eq_ignore_ascii_case("true"))
    } else {
        calc_host_vba::ProcedureArg::Text(arg.to_owned())
    }
}

fn result_json(result: &calc_host_vba::ProcedureResult) -> Value {
    use calc_host_vba::ProcedureResult as R;
    match result {
        R::Empty => json!({"type": "empty"}),
        R::Bool(b) => json!({"type": "logical", "value": b}),
        R::Number(n) if n.is_finite() => json!({"type": "number", "value": n}),
        R::Number(n) => json!({"type": "number", "value": format!("{n:?}")}),
        R::Text(t) => json!({"type": "text", "value": t}),
        R::Other(kind) => json!({"type": "other", "vba_type": kind}),
    }
}

/// One invocation record as JSON: the entry, how it was reached, the outcome (success, runtime
/// error, typed refusal naming the member and its owning work package, not callable or failed)
/// and the policy's denials, caught by VBA or not.
fn invocation_json(record: &calc_host_vba::InvocationRecord) -> Value {
    use calc_host_vba::Outcome as O;
    let outcome = match &record.outcome {
        O::Success(value) => json!({"kind": "success", "result": result_json(value)}),
        O::RuntimeError { number, description, source } => json!({
            "kind": "runtime-error", "number": number, "description": description, "source": source,
        }),
        O::NotCallable(why) => json!({"kind": "not-callable", "reason": why}),
        O::Refused { member, owner, reason } => json!({
            "kind": "refused", "member": member, "owner": owner, "reason": reason,
        }),
        O::Failed(why) => json!({"kind": "failed", "reason": why}),
    };
    json!({
        "entry": record.entry,
        "invocation": "command",
        "outcome": outcome,
        "denials": record.denials.iter().map(|d| json!({
            "capability": d.capability, "subject": d.subject, "layer": d.layer,
        })).collect::<Vec<_>>(),
        "allowed": record.allowed,
    })
}

pub(crate) fn run_macro(cli: &Cli) -> Result<(Value, u8), Failure> {
    if cli.all_macros {
        return run_all(cli);
    }
    let name = cli
        .macro_name
        .as_deref()
        .ok_or_else(|| fail("run needs --macro <Module.Proc> or --all"))?;
    if cli.level == calc_macro_security::MacroSecurity::Off {
        return Err(fail(format!(
            "the macro level is off, so the file's VBA project does not run and {name} is not called; pass --macro-level guarded or unguarded"
        )));
    }
    let mut book = session::open(cli)?;
    let Some(project) = book.project.clone() else {
        let why = book.macros["reason"]
            .as_str()
            .unwrap_or("the file has no VBA project")
            .to_owned();
        return Err(fail(format!("{name} was not called: {why}")));
    };
    let args: Vec<_> = cli.macro_args.iter().map(|a| procedure_argument(a)).collect();
    // The command route: the Sub or Function runs against the live workbook session.
    let record = project.run_procedure(&mut book.session, name, &args);
    if let calc_host_vba::Outcome::NotCallable(why) = &record.outcome {
        return Err(fail(format!("{name} was not run: {why}")));
    }
    session::recalculate(&mut book.session)?;
    let saved = match &cli.out {
        Some(path) => Some((path.as_str(), session::save(&mut book.session, path)?)),
        None => None,
    };
    let mut report = json!({
        "book": cli.book,
        "macro": name,
        "macros": book.macros,
        "invocation": invocation_json(&record),
    });
    let code = match &record.outcome {
        calc_host_vba::Outcome::Success(value) => {
            report["result"] = result_json(value);
            0
        }
        _ => 1,
    };
    if let Some((path, bytes)) = saved {
        report["saved"] = json!({"path": path, "bytes": bytes});
    }
    Ok((report, code))
}

/// `run --all`: every public Sub and Function of the project that needs no argument, each in a
/// freshly opened session, so one entry's workbook edits or module state never reach another's
/// outcome. Entries that take required arguments are listed as `needs-arguments`.
fn run_all(cli: &Cli) -> Result<(Value, u8), Failure> {
    if cli.level == calc_macro_security::MacroSecurity::Off {
        return Err(fail("the macro level is off, so the file's VBA project does not run; pass --macro-level guarded or unguarded"));
    }
    let first = session::open(cli)?;
    let Some(project) = first.project.clone() else {
        let why = first.macros["reason"].as_str().unwrap_or("the file has no VBA project").to_owned();
        return Err(fail(format!("no macro was called: {why}")));
    };
    let entries: Vec<_> = project.procedures().into_iter().filter(|p| p.public).collect();
    let macros = first.macros.clone();
    drop(first);
    let (mut success, mut runtime, mut refused, mut other, mut skipped) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut rows = Vec::new();
    for entry in entries {
        let qualified = format!("{}.{}", entry.module, entry.name);
        if entry.parameters.iter().any(|p| !p.optional && !p.param_array) {
            skipped += 1;
            rows.push(json!({"entry": qualified, "invocation": "command",
                             "outcome": {"kind": "needs-arguments"}}));
            continue;
        }
        let mut book = session::open(cli)?;
        let Some(project) = book.project.clone() else {
            return Err(fail("the project was not admitted on reopening"));
        };
        let record = project.run_procedure(&mut book.session, &qualified, &[]);
        match &record.outcome {
            calc_host_vba::Outcome::Success(_) => success += 1,
            calc_host_vba::Outcome::RuntimeError { .. } => runtime += 1,
            calc_host_vba::Outcome::Refused { .. } => refused += 1,
            _ => other += 1,
        }
        rows.push(invocation_json(&record));
    }
    let report = json!({
        "book": cli.book,
        "macros": macros,
        "entries": rows.len(),
        "success": success,
        "runtime_error": runtime,
        "refused": refused,
        "failed": other,
        "needs_arguments": skipped,
        "invocations": rows,
    });
    Ok((report, u8::from(runtime + refused + other > 0)))
}
