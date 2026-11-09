//! Writes the session file the Excel check of the saved look starts from (dc-0f1k.20):
//! `cargo run -p dna-onecalc --example session_look -- <out.xlsx>`.
//! The session: `=A1:C4` (a 4 x 3 array result of numbers, text, a blank and an error) at K4, a
//! Formatting panel style, and one Conditional Formatting rule of each kind.
use dna_onecalc::Session;
use dna_onecalc::cf::{CellStyle, CfRuleSpec};
use dna_onecalc::file;
use dna_onecalc::shown::{Authored, evaluate};

fn main() {
    let out = std::env::args().nth(1).expect("an output path");
    let mut session = Session::new("Sheet1").expect("a session");
    let mut state = Authored::default();
    state.formula = "=A1:C4".to_owned();
    state.formula_cell = "K4".to_owned();
    // Row by row over A1:C4 (the grid is 8 columns wide).
    let columns = dna_onecalc::grid::COLUMNS as usize;
    let cells = [
        (0, 0, "5"),
        (0, 1, "abc"),
        (0, 2, "12"),
        (1, 0, "-3"),
        (1, 1, "a \"q\" b"),
        (1, 2, "40"),
        (2, 0, "8"),
        (2, 1, "TRUE"),
        (2, 2, "#N/A"),
        (3, 0, "5"),
        (3, 1, "xyz"),
        (3, 2, "100"),
    ];
    for (row, column, text) in cells {
        state.grid[row * columns + column] = text.to_owned();
    }
    state.style = CellStyle {
        font_family: "Consolas".into(),
        font_size: 14,
        bold: true,
        italic: true,
        underline: true,
        strike: true,
        text_color: "#336699".into(),
        fill: "#ffee99".into(),
        horizontal: "center".into(),
        wrap: true,
        border_top: true,
        border_bottom: true,
        border_left: false,
        border_right: true,
        border_color: "#112233".into(),
    };
    let with = |kind: &str, edit: &dyn Fn(&mut CfRuleSpec)| {
        let mut spec = CfRuleSpec::new(kind);
        edit(&mut spec);
        spec
    };
    state.cf_rules = vec![
        with("cellIs", &|s| {
            s.operator = "between".into();
            s.operand1 = "2".into();
            s.operand2 = "B1+5".into();
            s.stop = true;
            s.bold = true;
            s.number_format = "0.00\"x\"".into();
        }),
        with("text", &|s| {
            s.operator = "beginsWith".into();
            s.operand1 = "a \"q\"".into();
        }),
        with("blanks", &|s| s.operator = "noBlanks".into()),
        with("errors", &|_| {}),
        with("dates", &|s| s.operator = "lastWeek".into()),
        with("expression", &|s| {
            s.operand1 = "AND(A1>2,B1<$C$3)".into();
            s.italic = true;
            s.underline = true;
            s.strike = true;
        }),
        with("rank", &|s| {
            s.operator = "bottom".into();
            s.rank = 25;
            s.percent = true;
        }),
        with("average", &|s| {
            s.operator = "below".into();
            s.equal = true;
        }),
        with("duplicates", &|_| {}),
        with("uniques", &|s| s.fill = "#c6efce".into()),
        with("colorScale", &|s| s.mid = String::new()),
        with("colorScale", &|s| s.low = "#112233".into()),
        with("dataBar", &|s| s.bar_color = "#ff0000".into()),
        with("iconSet", &|s| {
            s.icon_set = "4TrafficLights".into();
            s.reverse = true;
        }),
    ];
    evaluate(&mut session, &state);
    std::fs::write(&out, file::save(&mut session).expect("saved")).expect("written");
}
