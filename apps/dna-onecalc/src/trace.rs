//! Parts of the formula evaluated on their own: F9 on a selected part, and the evaluation trace
//! (the port of DnaOneCalc's X-Ray, dc-wr9). The evaluation is dna-formula-ui's
//! [`dna_formula_ui::parts`], shared with DNA Calc; here it runs over the session's one evaluator,
//! [`Session::calculate_text`], with the same inputs, form, environment and functions as the whole
//! formula.

use crate::display;
use crate::session::Session;
use crate::shown::{Authored, prepare};
use calc_fml::FormatProfile;
use calc_func::value::CalcValue;
use dna_formula_ui::parts::PartEvaluator;

pub use dna_formula_ui::parts::{MAX_TRACE_STEPS, PartValue, Trace, TraceStep};

/// The session as the evaluator of a part.
struct SessionEvaluator<'a>(&'a mut Session);

impl PartEvaluator for SessionEvaluator<'_> {
    fn calculate(&mut self, text: &str) -> Result<CalcValue, String> {
        let calculation = self.0.calculate_text(text);
        match calculation.result {
            Some(value) => Ok(value),
            None => Err(calculation
                .diagnostics
                .first()
                .map_or_else(|| "no value".to_owned(), |d| display::diagnostic(self.0, d))),
        }
    }

    fn profile(&self) -> Option<FormatProfile> {
        self.0
            .environment()
            .and_then(|environment| environment.profile)
    }
}

/// The value of the part of `authored`'s formula at `start`..`start + len` (UTF-16 code units).
pub fn evaluate_part(
    session: &mut Session,
    authored: &Authored,
    start: usize,
    len: usize,
) -> Result<PartValue, String> {
    prepare(session, authored);
    dna_formula_ui::parts::evaluate_part(
        &mut SessionEvaluator(session),
        &authored.formula,
        start,
        len,
    )
}

/// The evaluation trace of `authored`'s formula: every part with its value, innermost first.
pub fn trace(session: &mut Session, authored: &Authored) -> Trace {
    prepare(session, authored);
    dna_formula_ui::parts::trace(&mut SessionEvaluator(session), &authored.formula)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid;

    fn authored(formula: &str, cells: &[(usize, &str)]) -> Authored {
        let mut state = Authored {
            formula: formula.to_owned(),
            ..Authored::default()
        };
        for (index, text) in cells {
            state.grid[*index] = (*text).to_owned();
        }
        state
    }

    fn span_of(formula: &str, part: &str) -> (usize, usize) {
        let at = formula.find(part).expect("the part is in the formula");
        let start = formula[..at].encode_utf16().count();
        (start, part.encode_utf16().count())
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_selected_part_gives_its_value_as_formula_text() {
        let mut session = Session::new("Sheet1").unwrap();
        let state = authored(
            "=SUM(A1:A2)*2+LEN(\"ab\")",
            &[(0, "1.5"), (grid::COLUMNS as usize, "2")],
        );
        let (start, len) = span_of(&state.formula, "SUM(A1:A2)");
        let part = evaluate_part(&mut session, &state, start, len).unwrap();
        assert_eq!(
            (part.literal.as_str(), part.shown.as_str()),
            ("3.5", "number 3.5")
        );
        let (start, len) = span_of(&state.formula, "LEN(\"ab\")");
        assert_eq!(
            evaluate_part(&mut session, &state, start, len)
                .unwrap()
                .literal,
            "2"
        );
        // A range is an array constant, a text keeps its quotes, an error is its spelling.
        let (start, len) = span_of(&state.formula, "A1:A2");
        assert_eq!(
            evaluate_part(&mut session, &state, start, len)
                .unwrap()
                .literal,
            "{1.5;2}"
        );
        let state = authored("=\"a\"&\"b\"&1/0", &[]);
        let (start, len) = span_of(&state.formula, "\"a\"&\"b\"");
        assert_eq!(
            evaluate_part(&mut session, &state, start, len)
                .unwrap()
                .literal,
            "\"ab\""
        );
        let (start, len) = span_of(&state.formula, "1/0");
        assert_eq!(
            evaluate_part(&mut session, &state, start, len)
                .unwrap()
                .literal,
            "#DIV/0!"
        );
        // The whole formula, with its `=`; and nothing selected is a request to select.
        let (_, whole) = span_of(&state.formula, &state.formula);
        assert!(
            evaluate_part(&mut session, &state, 0, whole)
                .unwrap()
                .literal
                .starts_with('#')
        );
        assert!(
            evaluate_part(&mut session, &state, 2, 0)
                .unwrap_err()
                .starts_with("Select")
        );
        // An unknown function is #NAME?, a value like any other.
        assert!(
            evaluate_part(&mut session, &authored("=NOSUCH(1)+1", &[]), 1, 9)
                .is_ok_and(|v| v.literal == "#NAME?")
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_part_inside_let_sees_the_bindings_before_it_and_a_lambda_parameter_has_no_value() {
        let mut session = Session::new("Sheet1").unwrap();
        let state = authored("=LET(a,2,b,a*10,b+a)", &[]);
        let (start, len) = span_of(&state.formula, "b+a");
        assert_eq!(
            evaluate_part(&mut session, &state, start, len)
                .unwrap()
                .literal,
            "22"
        );
        let (start, len) = span_of(&state.formula, "a*10");
        assert_eq!(
            evaluate_part(&mut session, &state, start, len)
                .unwrap()
                .literal,
            "20"
        );
        let state = authored("=MAP({1,2},LAMBDA(x,x*3))", &[]);
        let (start, len) = span_of(&state.formula, "x*3");
        let refusal = evaluate_part(&mut session, &state, start, len).unwrap_err();
        assert!(refusal.contains("LAMBDA parameter"), "{refusal}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_trace_lists_every_part_innermost_first_with_its_value() {
        let mut session = Session::new("Sheet1").unwrap();
        let state = authored(
            "=SUM(A1:A2)*2",
            &[(0, "1.5"), (grid::COLUMNS as usize, "2")],
        );
        let traced = trace(&mut session, &state);
        let steps: Vec<(&str, &str, usize)> = traced
            .steps
            .iter()
            .map(|s| (s.text.as_str(), s.value.as_str(), s.depth))
            .collect();
        assert_eq!(
            steps,
            [
                ("A1:A2", "{1.5;2}", 2),
                ("SUM(A1:A2)", "3.5", 1),
                ("SUM(A1:A2)*2", "7", 0)
            ]
        );
        // The names a LET declares have no step; its values and body do, with the bindings.
        let state = authored("=LET(a,2,a*5)", &[]);
        let traced = trace(&mut session, &state);
        let texts: Vec<&str> = traced.steps.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, ["a", "a*5", "LET(a,2,a*5)"]);
        assert_eq!(traced.steps[0].value, "2");
        assert_eq!(traced.steps[2].value, "10");
        // A constant formula is its own step; a syntax error is no trace.
        assert_eq!(
            super::trace(&mut session, &authored("=42", &[]))
                .steps
                .len(),
            1
        );
        assert!(
            super::trace(&mut session, &authored("=1+", &[]))
                .steps
                .is_empty()
        );
    }
}
