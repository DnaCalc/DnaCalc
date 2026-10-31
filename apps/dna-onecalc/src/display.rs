//! Text for people about a session: its reads, diagnostics and refused inputs, as the surface
//! shows them. The labels both apps share (values, cells, areas, calc-fml's refusals) are
//! dna-formula-ui's [`dna_formula_ui::display`], re-exported here, over the session's sheet names.

use crate::fixture::{Read, RunCells};
use crate::session::{Diagnostic, InputError, Session, SheetError};
use calc_func::value::{CellKey, GridArea, SheetId};

pub use dna_formula_ui::display::*;

impl SheetNames for Session {
    fn sheet_name_of(&self, sheet: SheetId) -> Option<String> {
        self.sheet_name(sheet).map(str::to_owned)
    }
}

/// One read: the area asked for, then each cell or run of cells returned, in order:
/// `Sheet1!A1:A3: Sheet1!A1 = number 1.5; Sheet1!A2 blank; Sheet1!A3 = number 2`.
pub fn read(session: &Session, read: &Read) -> String {
    let mut parts = Vec::new();
    for run in &read.runs {
        let start = run.start;
        match &run.cells {
            RunCells::Values(values) => {
                for (offset, held) in values.iter().enumerate() {
                    let at = offset_cell(read.area, start, offset as u64);
                    parts.push(format!("{} = {}", cell(session, at), value(held)));
                }
            }
            RunCells::Blank(1) => parts.push(format!("{} blank", cell(session, start))),
            RunCells::Blank(count) => {
                parts.push(format!("{count} blank cells from {}", cell(session, start)))
            }
            RunCells::Missing(1) => parts.push(format!("{} no input", cell(session, start))),
            RunCells::Missing(count) => parts.push(format!(
                "{count} cells without input from {}",
                cell(session, start)
            )),
        }
    }
    format!("{}: {}", area(session, read.area), parts.join("; "))
}

/// The cell `offset` cells after `start` in the row-major order of `area`.
fn offset_cell(area: GridArea, start: CellKey, offset: u64) -> CellKey {
    let first = area.first();
    let width = u64::from(area.column_count());
    let at = u64::from(start.row() - first.row()) * width
        + u64::from(start.column() - first.column())
        + offset;
    let row = first.row() + (at / width) as u32;
    let column = first.column() + (at % width) as u32;
    CellKey::new(first.sheet(), row, column).expect("a cell of the area")
}

/// A diagnostic, with where it applies: a span of the formula text (UTF-16 code units), a cell,
/// an area or a function.
pub fn diagnostic(session: &Session, diagnostic: &Diagnostic) -> String {
    match diagnostic {
        Diagnostic::UserFunction(message) => message.clone(),
        Diagnostic::Syntax(syntax) => {
            format!("syntax at {}: {}", span(syntax.span), syntax.message)
        }
        Diagnostic::Compile(error) => compile_error(error),
        Diagnostic::Eval(fault) => eval_fault(session, fault),
    }
}

/// Why an input or the formula cell was refused.
pub fn input_error(session: &Session, error: &InputError) -> String {
    match error {
        InputError::NotACell { address } => format!("{address:?} is not one cell reference"),
        InputError::Address { address, error } => {
            format!("{address:?} refused: {}", bind_error(error))
        }
        InputError::FormulaCell { cell: at } => format!(
            "{} is the formula's cell, which holds no input",
            cell(session, *at)
        ),
        InputError::NumberNotHeld { value } => {
            format!("{value} is not a number a cell holds")
        }
        InputError::TextTooLong { units } => {
            format!("text of {units} UTF-16 code units is longer than Excel's 32,767")
        }
    }
}

/// Why a sheet was not added.
pub fn sheet_error(error: &SheetError) -> String {
    match error {
        SheetError::Duplicate { name } => format!("there is already a sheet named {name}"),
        SheetError::InvalidName { name } => format!("{name:?} is not a sheet name Excel accepts"),
    }
}
