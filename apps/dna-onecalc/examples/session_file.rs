//! Writes the session file the Excel round-trip proof starts from (dc-0f1k.5):
//! `cargo run -p dna-onecalc --example session_file -- <out.xlsx>`.
//! The session: `=SUM(A1:A3)+LEN(B1)` in the dynamic form at H1 over A1 = 1.5, A2 = 2, B1 = text,
//! B2 = TRUE, B3 = #N/A, locale de-DE, number format 0,00 and one listed add-in.
use dna_onecalc::file;
use dna_onecalc::shown::{Authored, evaluate};
use dna_onecalc::{AddinRef, Session};

fn main() {
    let out = std::env::args().nth(1).expect("an output path");
    let mut session = Session::new("Sheet1").expect("a session");
    let mut state = Authored::example_sum();
    state.formula = "=SUM(A1:A3)+LEN(B1)".to_owned();
    state.grid[1] = "é & <text>".to_owned();
    state.grid[9] = "TRUE".to_owned();
    state.grid[17] = "#N/A".to_owned();
    state.locale = "de-DE".to_owned();
    state.number_format = "0,00".to_owned();
    evaluate(&mut session, &state);
    session.set_addins(vec![AddinRef {
        kind: "xll".to_owned(),
        path: r"C:\libs\Example.xll".to_owned(),
        hash: "sha256:0123".to_owned(),
    }]);
    std::fs::write(&out, file::save(&mut session).expect("saved")).expect("written");
}
