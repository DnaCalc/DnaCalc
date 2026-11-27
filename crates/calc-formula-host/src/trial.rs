//! Level-zero requests over the existing session evaluator.

use crate::{Calculation, Diagnostic, Session, display, environment};
use calc_fml::reference::{ReferenceNotation, is_cell_callee};
use calc_fml::syntax::{SyntaxKind, SyntaxNode, TokenKind, lex};
use calc_fml::{DateSystem, EvalFault, FormatProfile, parse_formula};
use calc_formula_kernel::{parts, result};
use calc_func::value::fold_defined_name;
use calc_func::value::{CalcValue, CoreValue};
use calc_skin::{
    CalcGeneration, FormulaAnswer, FormulaLink, FormulaQuery, Provenance, RawValue, Refusal, Shown,
    TestInput, TrialCell, TrialValue,
};
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct FormulaHost {
    session: Session,
    generation: u64,
}

impl Default for FormulaHost {
    fn default() -> Self {
        Self {
            session: Session::new("Trial").expect("valid internal compilation scope"),
            generation: 0,
        }
    }
}

impl FormulaHost {
    /// Handles the formula-only port. Each trial gets its own literal fixture and environment.
    pub fn handle(&mut self, query: FormulaQuery) -> Result<FormulaAnswer, Refusal> {
        let trial = match &query {
            FormulaQuery::Evaluate(trial)
            | FormulaQuery::Trace(trial)
            | FormulaQuery::EvaluatePart { trial, .. } => trial,
        };
        if trial.formula.trim().is_empty() {
            return Err(invalid("Enter a formula"));
        }
        let session = &mut self.session;
        let environment = environment(&trial.culture, trial.now, false);
        let profile = environment
            .profile
            .ok_or_else(|| invalid("This locale is unavailable"))?;
        session.set_environment(Some(environment));
        session.set_trial_names(local_names(&trial.inputs)?);
        match query {
            FormulaQuery::Evaluate(trial) => {
                let calculated = session.calculate_unplaced(&trial.formula);
                let value = value(session, calculated, &trial.formula)?;
                let value = project(&value, &profile, &trial.number_format)?;
                self.generation += 1;
                let calc = CalcGeneration(self.generation);
                Ok(FormulaAnswer::Evaluated(Shown {
                    value,
                    provenance: Provenance::Calculated { calc },
                }))
            }
            FormulaQuery::Trace(trial) => {
                let calculated = session.calculate_unplaced(&trial.formula);
                value(session, calculated, &trial.formula)?;
                let trace = parts::trace(&mut TrialParts(session, &trial.formula), &trial.formula);
                Ok(FormulaAnswer::Steps(trace))
            }
            FormulaQuery::EvaluatePart { trial, start, len } => parts::evaluate_part(
                &mut TrialParts(session, &trial.formula),
                &trial.formula,
                start,
                len,
            )
            .map(FormulaAnswer::Part)
            .map_err(invalid),
        }
    }
}

fn local_names(inputs: &[TestInput]) -> Result<BTreeMap<String, String>, Refusal> {
    let mut names = BTreeMap::new();
    for input in inputs {
        let name = input.name.trim();
        if name.is_empty() && input.constant.trim().is_empty() {
            continue;
        }
        let tokens = lex(name);
        let valid = tokens
            .first()
            .is_some_and(|token| token.kind() == TokenKind::Identifier && token.text() == name)
            && !is_cell_callee(name, ReferenceNotation::A1)
            && !is_cell_callee(name, ReferenceNotation::R1C1)
            && !["R", "C", "TRUE", "FALSE"]
                .iter()
                .any(|reserved| name.eq_ignore_ascii_case(reserved))
            && name.encode_utf16().count() <= 255;
        if !valid {
            return Err(invalid(format!("{name:?} cannot be a local input name")));
        }
        let source = format!("={}", input.constant.trim());
        let tree = parse_formula(&source);
        if !tree.diagnostics().is_empty() || !tree.expression().is_some_and(constant) {
            return Err(invalid(format!("{name} needs an Excel constant")));
        }
        if names.insert(fold_defined_name(name), source).is_some() {
            return Err(invalid(format!("{name} is already a local input")));
        }
    }
    Ok(names)
}

fn constant(node: &SyntaxNode) -> bool {
    match node.kind() {
        SyntaxKind::NumberLiteralExpr
        | SyntaxKind::StringLiteralExpr
        | SyntaxKind::ErrorLiteralExpr => true,
        SyntaxKind::IdentifierExpr => ["TRUE", "FALSE"]
            .iter()
            .any(|text| node.text().trim().eq_ignore_ascii_case(text)),
        SyntaxKind::PrefixExpr | SyntaxKind::PostfixExpr => {
            node.tokens().iter().any(|token| {
                matches!(
                    token.kind(),
                    TokenKind::Plus | TokenKind::Minus | TokenKind::Percent
                )
            }) && node
                .child_nodes()
                .all(|child| child.kind() == SyntaxKind::NumberLiteralExpr)
        }
        SyntaxKind::ArrayLiteralExpr => node.child_nodes().all(constant),
        _ => false,
    }
}

struct TrialParts<'a>(&'a mut Session, &'a str);

impl parts::PartEvaluator for TrialParts<'_> {
    fn calculate(&mut self, text: &str) -> Result<CalcValue, String> {
        let calculated = self.0.calculate_unplaced(text);
        value(self.0, calculated, self.1).map_err(|refusal| match refusal {
            Refusal::RequiresWorkbook { reason, .. } | Refusal::Invalid { reason } => reason,
            Refusal::NotModelled { what, .. } => what,
            other => format!("{other:?}"),
        })
    }

    fn profile(&self) -> Option<FormatProfile> {
        self.0.environment()?.profile
    }
}

fn invalid(reason: impl Into<String>) -> Refusal {
    Refusal::Invalid {
        reason: reason.into(),
    }
}

fn value(session: &Session, calculation: Calculation, formula: &str) -> Result<CalcValue, Refusal> {
    if let Some(value) = calculation.result {
        return Ok(value);
    }
    let diagnostic = calculation
        .diagnostics
        .last()
        .ok_or_else(|| invalid("No formula"))?;
    let reason = display::diagnostic(session, diagnostic);
    match diagnostic {
        Diagnostic::Eval(EvalFault::NotModelled {
            owner: "dc-iopjy", ..
        })
        | Diagnostic::Eval(EvalFault::MissingInput { .. }) => Err(Refusal::RequiresWorkbook {
            route: FormulaLink(formula.to_owned()),
            reason: "This formula needs a workbook cell or a workbook value.".into(),
        }),
        Diagnostic::Eval(EvalFault::NotModelled { owner, .. })
        | Diagnostic::Eval(EvalFault::Function(calc_func::FunctionFault::NotHeld {
            owner, ..
        })) => Err(Refusal::NotModelled {
            owner: (*owner).into(),
            what: reason,
        }),
        Diagnostic::Compile(error @ calc_fml::CompileError::NotModelled { .. }) => {
            Err(Refusal::NotModelled {
                owner: error.owner().unwrap_or("dc-iopjy").into(),
                what: reason,
            })
        }
        Diagnostic::Eval(_) => Err(Refusal::NotModelled {
            owner: "dc-iopjy".into(),
            what: reason,
        }),
        _ => Err(invalid(reason)),
    }
}

fn project(
    value: &CalcValue,
    profile: &FormatProfile,
    format: &str,
) -> Result<TrialValue, Refusal> {
    let to_cell = |value: &CalcValue| -> Result<TrialCell, Refusal> {
        let raw = match value.core() {
            CoreValue::Number(number) => RawValue::Number {
                bits: number.to_bits(),
            },
            CoreValue::Text(text) => RawValue::Text(text.utf16_code_units().to_vec()),
            CoreValue::Logical(value) => RawValue::Logical(*value),
            CoreValue::Error(code) => RawValue::Error(display::error_spelling(*code).into()),
            CoreValue::Empty => RawValue::Blank,
            _ => RawValue::Other {
                kind: result::kind(value).into(),
                display: result::plain(value),
            },
        };
        let formatted = if format.trim().is_empty() || format.eq_ignore_ascii_case("General") {
            result::shown(value, Some(profile))
        } else {
            result::formatted_text(value, profile, format, DateSystem::System1900)
                .map_err(invalid)?
        };
        Ok(TrialCell { raw, formatted })
    };
    let Some(array) = value.as_array() else {
        return to_cell(value).map(TrialValue::Scalar);
    };
    let window = result::first_window(array, Some(profile));
    let cells = window
        .cells
        .iter()
        .enumerate()
        .map(|(row, values)| {
            (0..values.len())
                .map(|col| {
                    to_cell(
                        array
                            .get(row, col)
                            .expect("cell in the first result window"),
                    )
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(TrialValue::Array {
        rows: window.total_rows,
        columns: window.total_cols,
        cells,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_skin::Trial;

    fn evaluate(host: &mut FormulaHost, formula: &str) -> Result<FormulaAnswer, Refusal> {
        host.handle(FormulaQuery::Evaluate(Trial {
            formula: formula.into(),
            culture: "en-US".into(),
            number_format: "0.00".into(),
            now: None,
            inputs: Vec::new(),
        }))
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_workbookless_trial_keeps_exact_results_and_refuses_only_reached_caller_reads() {
        let mut host = FormulaHost::default();
        for formula in [
            "=ROW()",
            "=CELL(\"address\")",
            "=SHEETS()",
            "=SHEET(\"Trial\")",
            "=A1",
            "=SUM(A1:A2)",
            "=ROW(A1)",
            "=ROW(INDIRECT(\"A1\"))",
        ] {
            assert!(
                matches!(evaluate(&mut host, formula),
                Err(Refusal::RequiresWorkbook { route, .. }) if route.0 == formula),
                "{formula}"
            );
        }
        let answer = evaluate(&mut host, "=IF(FALSE,ROW(),1/3)").unwrap();
        let FormulaAnswer::Evaluated(Shown {
            value:
                TrialValue::Scalar(TrialCell {
                    raw: RawValue::Number { bits },
                    formatted,
                }),
            provenance: Provenance::Calculated { calc },
        }) = answer
        else {
            panic!("expected calculated number");
        };
        assert_eq!(bits, (1_f64 / 3.0).to_bits());
        assert_eq!(formatted, "0.33");
        assert_eq!(calc.0, 1);
        assert_eq!(
            host.session
                .calculate_text("=ROW()")
                .result
                .unwrap()
                .as_number(),
            Some(1.0)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn local_inputs_use_the_name_binder_and_recompile_when_changed() {
        let mut host = FormulaHost::default();
        let trial = |constant: &str| {
            FormulaQuery::Evaluate(Trial {
                formula: "=SUM(Figures)".into(),
                culture: "en-US".into(),
                number_format: "General".into(),
                now: None,
                inputs: vec![TestInput {
                    name: "figures".into(),
                    constant: constant.into(),
                }],
            })
        };
        for (literal, expected) in [("{1,2;3,4}", 10_f64), ("{10,20}", 30_f64)] {
            let FormulaAnswer::Evaluated(shown) = host.handle(trial(literal)).unwrap() else {
                panic!("expected result")
            };
            let TrialValue::Scalar(cell) = shown.value else {
                panic!("expected scalar")
            };
            assert_eq!(
                cell.raw,
                RawValue::Number {
                    bits: expected.to_bits()
                }
            );
        }
        assert!(matches!(
            host.handle(trial("ROW()")),
            Err(Refusal::Invalid { .. })
        ));
    }
}
