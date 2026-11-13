//! The `dnacalc` command (dc-icq4.1): the headless runner of DNA Calc, on Linux and Windows.
//!
//! ```text
//! dnacalc recalc book.xlsx [--out file]         recalculate; report the counts; save with --out
//! dnacalc diff   book.xlsx                      recalculate and compare every formula cell with
//!                                               the file's cached values
//! dnacalc get    book.xlsx [Sheet!]A1 ...       a cell's formula, cached and calculated value
//! dnacalc set    book.xlsx [Sheet!]A1 TEXT ...  enter cells as typed, recalculate, save with --out
//! dnacalc run    book.xlsm --macro Mod.Proc     call a public VBA Function of the file's project
//! ```
//!
//! Options on every command: `--json` (one JSON document on stdout), `--macro-level
//! off|safe|unguarded` (default `off`: the file's VBA project does not run; `safe` runs it with
//! the operating-system services refused, `unguarded` with full access). `diff` takes `--cells`
//! to list the per-cell rows in text mode (JSON always has them).
//!
//! Everything goes through the one session (`calc_host::HostSession`) the applications use, and
//! the value comparison is `calc_verdict::compare`, the one the corpus oracle judges with.
//! Exit codes: 0 done (and, for `diff`, every formula cell matched), 1 done with differing or
//! refused cells, 2 usage or a typed refusal.

#![cfg(not(target_arch = "wasm32"))]

mod cells;
mod diff;
mod session;

use serde_json::{Value, json};
use std::io::Write;

pub const USAGE: &str = "usage: dnacalc <command> <book> [options]
  recalc <book> [--out <file>]            recalculate; save the result with --out
  diff   <book> [--cells]                 recalculate; compare every formula cell with the file's cached values
  get    <book> <cell>...                 a cell's formula, cached value and calculated value
  set    <book> <cell> <text> [<cell> <text>]... [--out <file>]   enter cells as typed, recalculate, save with --out
  run    <book> --macro <Module.Proc> [--arg <value>]... [--out <file>]   call a public VBA Function of the file's project
options: --json  one JSON document on stdout;  --macro-level off|safe|unguarded  (default off)
         --locale <culture>  the regional settings TEXT, VALUE and ASC read (default en-US, DNA Calc's own; Excel's are not read)
         NOW and TODAY read this machine's clock (UTC)
A cell is A1 or Sheet1!A1 (the first sheet when unqualified).
exit: 0 done (diff: all cells matched), 1 done with differing or refused cells, 2 usage or refusal";

/// A refusal or usage error: the message and the exit code.
pub(crate) struct Failure {
    pub(crate) code: u8,
    pub(crate) message: String,
}
pub(crate) fn fail(message: impl Into<String>) -> Failure {
    Failure {
        code: 2,
        message: message.into(),
    }
}

/// The parsed command line.
pub(crate) struct Cli {
    pub(crate) command: String,
    pub(crate) book: String,
    pub(crate) positional: Vec<String>,
    pub(crate) out: Option<String>,
    pub(crate) json: bool,
    pub(crate) list_cells: bool,
    pub(crate) level: calc_macro_security::MacroSecurity,
    pub(crate) macro_name: Option<String>,
    pub(crate) macro_args: Vec<String>,
    pub(crate) locale: String,
}

fn parse(args: &[String]) -> Result<Cli, Failure> {
    let mut rest = args.iter();
    let command = rest.next().ok_or_else(|| fail(USAGE))?.clone();
    let mut cli = Cli {
        command,
        book: String::new(),
        positional: Vec::new(),
        out: None,
        json: false,
        list_cells: false,
        level: calc_macro_security::MacroSecurity::Off,
        macro_name: None,
        macro_args: Vec::new(),
        locale: "en-US".into(),
    };
    let mut books = Vec::new();
    while let Some(arg) = rest.next() {
        let mut value = |what: &str| {
            rest.next()
                .cloned()
                .ok_or_else(|| fail(format!("{arg} needs {what}")))
        };
        match arg.as_str() {
            "--json" => cli.json = true,
            "--cells" => cli.list_cells = true,
            "--out" => cli.out = Some(value("a file")?),
            "--macro" => cli.macro_name = Some(value("Module.Proc")?),
            "--arg" => cli.macro_args.push(value("a value")?),
            "--locale" => cli.locale = value("a culture such as en-US")?,
            "--macro-level" => {
                let text = value("off, safe or unguarded")?;
                cli.level = calc_macro_security::MacroSecurity::parse(&text).ok_or_else(|| {
                    fail(format!(
                        "--macro-level {text:?}: expected off, safe or unguarded"
                    ))
                })?;
            }
            other if other.starts_with("--") => return Err(fail(format!("unknown option {other}"))),
            other => books.push(other.to_owned()),
        }
    }
    let mut books = books.into_iter();
    cli.book = books.next().ok_or_else(|| fail(USAGE))?;
    cli.positional = books.collect();
    Ok(cli)
}

/// Runs the command line `args` (without the program name), writing the report to `out`.
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    if args.iter().any(|a| a == "--help" || a == "-h" || a == "help") {
        let _ = writeln!(out, "{USAGE}");
        return 0;
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        let _ = writeln!(out, "dnacalc {}", env!("CARGO_PKG_VERSION"));
        return 0;
    }
    let result = parse(args).and_then(|cli| {
        let (report, code) = match cli.command.as_str() {
            "recalc" => cells::recalc(&cli)?,
            "diff" => diff::diff(&cli)?,
            "get" => cells::get(&cli)?,
            "set" => cells::set(&cli)?,
            "run" => cells::run_macro(&cli)?,
            other => return Err(fail(format!("unknown command {other:?}\n{USAGE}"))),
        };
        Ok((cli.json, cli.list_cells || cli.command != "diff", report, code))
    });
    match result {
        Ok((json, list, report, code)) => {
            let text = if json {
                serde_json::to_string_pretty(&report).expect("a JSON value serialises")
            } else {
                render_text(&report, list)
            };
            let _ = writeln!(out, "{text}");
            code
        }
        Err(failure) => {
            let wants_json = args.iter().any(|a| a == "--json");
            if wants_json {
                let _ = writeln!(
                    out,
                    "{}",
                    json!({"error": failure.message, "exit": failure.code})
                );
            } else {
                let _ = writeln!(err, "dnacalc: {}", failure.message);
            }
            failure.code
        }
    }
}

/// The text form: one `key: value` line per summary field, then the per-cell rows when asked.
fn render_text(report: &Value, list_cells: bool) -> String {
    let mut lines = Vec::new();
    if let Some(object) = report.as_object() {
        for (key, value) in object {
            if key == "cells" {
                continue;
            }
            lines.push(format!("{key}: {}", compact(value)));
        }
    }
    if let Some(cells) = report.get("cells").and_then(Value::as_array)
        && list_cells
    {
        for cell in cells {
            lines.push(compact(cell));
        }
    }
    lines.join("\n")
}

fn compact(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
