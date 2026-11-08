//! F9 on a part of a formula and the evaluation trace for DNA Calc's formula bar (dc-0f1k.7): the
//! shared walk of dna-formula-ui ([`dna_formula_ui::parts`]) over the workbook's host, which
//! evaluates each part as the edited cell would read it, over the published values and the
//! registered functions, and writes nothing.

use calc_host::{FormatProfile, HostState};
use calc_value::{CalcValue, CellKey};
use dna_calc_model::parts::{PartValue, Trace};
use dna_formula_ui::parts::PartEvaluator;

struct HostEvaluator<'a> {
    host: &'a HostState,
    anchor: CellKey,
    profile: Option<FormatProfile>,
}

impl PartEvaluator for HostEvaluator<'_> {
    fn calculate(&mut self, text: &str) -> Result<CalcValue, String> {
        self.host
            .evaluate_formula_text(text.strip_prefix('=').unwrap_or(text), self.anchor)
    }

    fn profile(&self) -> Option<FormatProfile> {
        self.profile.clone()
    }
}

pub fn evaluate_part(
    host: &HostState,
    profile: Option<&FormatProfile>,
    anchor: CellKey,
    formula: &str,
    start: usize,
    len: usize,
) -> Result<PartValue, String> {
    let mut evaluator = HostEvaluator {
        host,
        anchor,
        profile: profile.cloned(),
    };
    dna_formula_ui::parts::evaluate_part(&mut evaluator, formula, start, len)
}

pub fn trace(
    host: &HostState,
    profile: Option<&FormatProfile>,
    anchor: CellKey,
    formula: &str,
) -> Trace {
    let mut evaluator = HostEvaluator {
        host,
        anchor,
        profile: profile.cloned(),
    };
    dna_formula_ui::parts::trace(&mut evaluator, formula)
}

#[cfg(test)]
mod tests {
    use crate::{
        server::Server,
        wire::{Intent, Key, Reply, Request},
    };
    use calc_value::{CalcValue, CellKey};

    #[derive(Debug)]
    struct Double;
    impl calc_fml::UserFunction for Double {
        fn call(&self, arguments: &[CalcValue]) -> Result<CalcValue, String> {
            Ok(CalcValue::number(arguments[0].as_number().unwrap_or(0.0) * 2.0))
        }
    }

    fn enter(server: &mut Server, at: CellKey, text: &str) {
        server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(at),
                    text: text.into(),
                }),
                None,
            )
            .reply
            .unwrap();
    }

    fn span_of(formula: &str, part: &str) -> (usize, usize) {
        let at = formula.find(part).expect("the part is in the formula");
        (formula[..at].encode_utf16().count(), part.encode_utf16().count())
    }

    fn setup() -> (Server, CellKey) {
        let mut server = Server::new();
        let sheet = server.book().sheets()[0].id;
        let mut functions = calc_host::CallableTable::new();
        functions.register("Double", std::rc::Rc::new(Double)).unwrap();
        server.book_mut().set_user_functions(functions);
        enter(&mut server, CellKey::new(sheet, 1, 1).unwrap(), "1.5");
        enter(&mut server, CellKey::new(sheet, 2, 1).unwrap(), "2");
        (server, CellKey::new(sheet, 1, 3).unwrap())
    }

    fn part(server: &mut Server, anchor: CellKey, formula: &str, of: &str) -> Result<(String, String), String> {
        let (start, len) = span_of(formula, of);
        let Ok(Reply::Part(answer)) = server
            .handle(
                Request::EvaluatePart {
                    formula: formula.into(),
                    anchor: Key::of(anchor),
                    start,
                    len,
                },
                None,
            )
            .reply
        else {
            panic!("a part answer")
        };
        answer.map(|part| (part.literal, part.shown))
    }

    /// dc-0f1k.7: F9 on a part asks the workbook, which reads the cells, the registered functions
    /// and the LET bindings in scope as the edited cell would, and changes nothing.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_part_is_evaluated_over_the_workbook_and_its_registered_functions() {
        let (mut server, anchor) = setup();
        let revision = server.book().host().revision();
        let formula = "=SUM(A1:A2)*2+Double(A1)";
        assert_eq!(
            part(&mut server, anchor, formula, "SUM(A1:A2)"),
            Ok(("3.5".into(), "number 3.5".into()))
        );
        assert_eq!(part(&mut server, anchor, formula, "A1:A2").unwrap().0, "{1.5;2}");
        assert_eq!(part(&mut server, anchor, formula, "SUM(A1:A2)*2").unwrap().0, "7");
        // A part that calls a registered function is a typed refusal that names its owner: the
        // detached evaluation has no effect-free capability for user functions yet (dc-hsa5).
        let refusal = part(&mut server, anchor, formula, "Double(A1)").unwrap_err();
        assert!(refusal.contains("user function") && refusal.contains("dc-hsa5"), "{refusal}");
        let formula = "=LET(a,2,b,a*10,b+a)";
        assert_eq!(part(&mut server, anchor, formula, "b+a").unwrap().0, "22");
        let formula = "=MAP({1,2},LAMBDA(x,x*3))";
        assert!(part(&mut server, anchor, formula, "x*3").unwrap_err().contains("LAMBDA parameter"));
        let (formula, empty) = ("=1+1", Request::EvaluatePart {
            formula: "=1+1".into(),
            anchor: Key::of(anchor),
            start: 1,
            len: 0,
        });
        let _ = formula;
        let Ok(Reply::Part(answer)) = server.handle(empty, None).reply else {
            panic!("a part answer")
        };
        assert!(answer.unwrap_err().starts_with("Select"));
        assert_eq!(server.book().host().revision(), revision, "nothing is written");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_trace_lists_every_part_innermost_first_and_the_functions_are_listed() {
        let (mut server, anchor) = setup();
        let Ok(Reply::Trace(trace)) = server
            .handle(
                Request::TraceFormula {
                    formula: "=SUM(A1:A2)*2+Double(A1)".into(),
                    anchor: Key::of(anchor),
                },
                None,
            )
            .reply
        else {
            panic!("a trace")
        };
        let refusal = concat!(
            "Not evaluated here: a diagnostic snapshot calling a user function without an ",
            "effect-free capability (dc-hsa5)"
        );
        let steps: Vec<(&str, &str, usize)> = trace
            .steps
            .iter()
            .map(|step| (step.text.as_str(), step.value.as_str(), step.depth))
            .collect();
        assert_eq!(
            steps,
            [
                ("A1:A2", "{1.5;2}", 3),
                ("SUM(A1:A2)", "3.5", 2),
                ("SUM(A1:A2)*2", "7", 1),
                ("A1", "1.5", 2),
                ("Double(A1)", refusal, 1),
                ("SUM(A1:A2)*2+Double(A1)", refusal, 0),
            ]
        );
        let Ok(Reply::UserFunctions(names)) = server.handle(Request::UserFunctions, None).reply else {
            panic!("the registered functions")
        };
        assert_eq!(names, ["DOUBLE"], "the table folds the names");
    }
}
