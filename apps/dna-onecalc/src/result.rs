//! A calculation's result as the surface shows it. The layout both apps share (a value as a cell
//! shows it, an array window, an error's meaning) is dna-formula-ui's
//! [`dna_formula_ui::result`], re-exported here; where a session's result came from is OneCalc's.

use crate::session::{Calculation, Session};

pub use dna_formula_ui::result::*;

/// Where a result came from: the formula's cell, the route that calculated it, and the cells it
/// read. `None` when nothing was calculated.
pub fn provenance(session: &Session, calculation: &Calculation) -> Option<String> {
    calculation.result.as_ref()?;
    let cell = crate::display::cell(session, session.formula_cell());
    let reads = calculation.reads.len();
    let cells: u64 = calculation
        .reads
        .iter()
        .map(|read| read.area.cell_count())
        .sum();
    let read_text = match (reads, cells) {
        (0, _) => "no cells read".to_owned(),
        (1, 1) => "1 cell read".to_owned(),
        (1, cells) => format!("1 area read ({cells} cells)"),
        (reads, cells) => format!("{reads} areas read ({cells} cells)"),
    };
    Some(format!(
        "calculated in {cell} by calc-fml's evaluator with calc-func's functions; {read_text}"
    ))
}
