//! Level-zero requests over the existing session evaluator.

use crate::{Calculation, Diagnostic, InputValue, Session, display, environment, fixture::Fixture};
use calc_fml::reference::{BindError, ReferenceNotation, is_cell_callee};
use calc_fml::syntax::{SyntaxKind, SyntaxNode, TokenKind, lex};
use calc_fml::{EvalFault, FormatProfile, parse_formula};
use calc_formula_kernel::{parts, result};
use calc_func::value::fold_defined_name;
use calc_func::value::{CalcArray, CalcValue, CellKey, CoreValue};
use calc_skin::{
    ARRAY_PAGE_COLUMNS, ARRAY_PAGE_ROWS, ArrayPosition, CalcGeneration, CallerSnapshot,
    ConditionalRule, FormulaAnswer, FormulaLink, FormulaOperation, FormulaQuery, Provenance,
    RawValue, Refusal, Shown, TestInput, TrialCell, TrialValue,
};
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct FormulaHost {
    session: Session,
    queries: FormulaRequests,
}

impl Default for FormulaHost {
    fn default() -> Self {
        Self {
            session: Session::new("Trial").expect("valid internal compilation scope"),
            queries: FormulaRequests::default(),
        }
    }
}

impl FormulaHost {
    pub fn handle(&mut self, query: FormulaQuery) -> Result<FormulaAnswer, Refusal> {
        self.queries.handle(&mut self.session, query)
    }

    pub fn operate(&mut self, operation: FormulaOperation) -> Result<FormulaAnswer, Refusal> {
        crate::workbench_provider::operate(&mut self.session, operation)
    }
}

/// Keeps the result behind a formula port available for paged reads.
#[derive(Debug, Default)]
pub struct FormulaRequests {
    generation: u64,
    result: Option<PinnedResult>,
}

#[derive(Debug)]
pub(crate) struct PinnedResult {
    pub(crate) value: CalcValue,
    environment: calc_fml::CalcEnvironment,
    profile: FormatProfile,
    format: String,
}

impl FormulaRequests {
    pub fn handle(
        &mut self,
        session: &mut Session,
        query: FormulaQuery,
    ) -> Result<FormulaAnswer, Refusal> {
        match query {
            FormulaQuery::Providers => Ok(FormulaAnswer::Providers(
                crate::workbench_provider::providers(session),
            )),
            FormulaQuery::ExportWorkbook(trial) => {
                crate::workbench_file::export(session, trial).map(FormulaAnswer::File)
            }
            FormulaQuery::OpenWorkbook(bytes) => {
                crate::workbench_file::import(bytes).map(FormulaAnswer::Opened)
            }
            FormulaQuery::RunSuite(suite) => {
                crate::workbench_suite::run(session, suite).map(FormulaAnswer::TestResults)
            }
            FormulaQuery::ArrayWindow { calc, position } => self
                .pinned(calc)?
                .answer(calc, position)
                .map(FormulaAnswer::Evaluated),
            FormulaQuery::ConditionalFormat {
                calc,
                position,
                rule,
            } => self.pinned(calc)?.conditional(calc, position, rule),
            FormulaQuery::Evaluate(trial) => self.evaluate(session, trial, false),
            FormulaQuery::Preview(trial) => self.evaluate(session, trial, true),
            FormulaQuery::Trace(trial) => in_context(session, &trial, |session, _, _| {
                let calculated = session.calculate_trial(&trial.formula);
                value(session, calculated, &trial.formula)?;
                let trace = parts::trace(&mut TrialParts(session, &trial.formula), &trial.formula);
                Ok(FormulaAnswer::Steps(trace))
            }),
            FormulaQuery::EvaluatePart { trial, start, len } => {
                in_context(session, &trial, |session, _, _| {
                    parts::evaluate_part(
                        &mut TrialParts(session, &trial.formula),
                        &trial.formula,
                        start,
                        len,
                    )
                    .map(FormulaAnswer::Part)
                    .map_err(invalid)
                })
            }
        }
    }

    fn evaluate(
        &mut self,
        session: &mut Session,
        trial: calc_skin::Trial,
        preview: bool,
    ) -> Result<FormulaAnswer, Refusal> {
        self.result = None;
        let pinned = evaluate_value(session, &trial, preview)?;
        self.generation += 1;
        let answer = pinned.answer(CalcGeneration(self.generation), ArrayPosition::default())?;
        self.result = Some(pinned);
        Ok(FormulaAnswer::Evaluated(answer))
    }

    fn pinned(&self, calc: CalcGeneration) -> Result<&PinnedResult, Refusal> {
        self.result
            .as_ref()
            .filter(|_| calc.0 == self.generation)
            .ok_or_else(|| invalid("Evaluate to refresh this result"))
    }
}

pub(crate) fn evaluate_value(
    session: &mut Session,
    trial: &calc_skin::Trial,
    preview: bool,
) -> Result<PinnedResult, Refusal> {
    in_context(session, trial, |session, profile, environment| {
        let calculated = if preview {
            session.preview_trial(&trial.formula)
        } else {
            session.calculate_trial(&trial.formula)
        };
        let value = value(session, calculated, &trial.formula)?;
        Ok(PinnedResult {
            value,
            profile,
            format: trial.number_format.clone(),
            environment,
        })
    })
}

fn in_context<T>(
    session: &mut Session,
    trial: &calc_skin::Trial,
    run: impl FnOnce(&mut Session, FormatProfile, calc_fml::CalcEnvironment) -> Result<T, Refusal>,
) -> Result<T, Refusal> {
    if trial.formula.trim().is_empty() {
        return Err(invalid("Enter a formula"));
    }
    let date_1904 = trial.caller.as_ref().is_some_and(|caller| caller.date_1904);
    let environment = environment(&trial.culture, trial.now, date_1904);
    let profile = environment
        .profile
        .ok_or_else(|| invalid("This locale is unavailable"))?;
    let (mut fixture, at) = trial_context(trial.caller.as_ref())?;
    fixture.local_names = local_names(&trial.inputs)?;
    session.with_trial(fixture, at, environment, |session| {
        run(session, profile, environment)
    })
}

impl PinnedResult {
    fn answer(
        &self,
        calc: CalcGeneration,
        position: ArrayPosition,
    ) -> Result<Shown<TrialValue>, Refusal> {
        let value = match self.value.as_array() {
            Some(array) => {
                let shape = array.shape();
                if position.row >= shape.rows || position.column >= shape.cols {
                    return Err(invalid("That page is outside the result array"));
                }
                let rows = (shape.rows - position.row).min(ARRAY_PAGE_ROWS);
                let columns = (shape.cols - position.column).min(ARRAY_PAGE_COLUMNS);
                let cells = (position.row..position.row + rows)
                    .map(|row| {
                        (position.column..position.column + columns)
                            .map(|col| {
                                self.cell(array.get(row, col).expect("cell in the result window"))
                            })
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                TrialValue::Array {
                    rows: shape.rows,
                    columns: shape.cols,
                    position,
                    cells,
                }
            }
            None if position == ArrayPosition::default() => {
                TrialValue::Scalar(self.cell(&self.value)?)
            }
            None => return Err(invalid("This result has only one cell")),
        };
        Ok(Shown {
            value,
            provenance: Provenance::Calculated { calc },
        })
    }

    fn conditional(
        &self,
        calc: CalcGeneration,
        position: ArrayPosition,
        rule: ConditionalRule,
    ) -> Result<FormulaAnswer, Refusal> {
        let mut result = self.answer(calc, position)?;
        let (rows, columns) = match &result.value {
            TrialValue::Scalar(_) => (1, 1),
            TrialValue::Array { cells, .. } => (cells.len(), cells.first().map_or(0, Vec::len)),
        };
        let (kind, operator, operand) = match rule {
            ConditionalRule::GreaterThan(value) => ("cellIs", "greaterThan", value),
            ConditionalRule::LessThan(value) => ("cellIs", "lessThan", value),
            ConditionalRule::EqualTo(value) => ("cellIs", "equal", value),
            ConditionalRule::Expression(formula) => ("expression", "", formula),
        };
        let mut spec = crate::cf::CfRuleSpec::new(kind);
        spec.operator = operator.into();
        spec.operand1 = operand;
        spec.bold = true;
        spec.rule(1).map_err(invalid)?;
        let scalar;
        let array = match self.value.as_array() {
            Some(array) => array,
            None => {
                scalar = CalcArray::from_scalar(self.value.clone());
                &scalar
            }
        };
        let outcome = crate::cf::evaluate_window(
            &[spec],
            array,
            position,
            rows,
            columns,
            Some(self.environment),
        );
        match &mut result.value {
            TrialValue::Scalar(cell) => cell.conditional_match = !outcome.looks[0][0].is_empty(),
            TrialValue::Array { cells, .. } => {
                for (row, looks) in cells.iter_mut().zip(outcome.looks) {
                    for (cell, look) in row.iter_mut().zip(looks) {
                        cell.conditional_match = !look.is_empty();
                    }
                }
            }
        }
        Ok(FormulaAnswer::Conditional {
            result,
            notes: outcome.notes,
        })
    }

    pub(crate) fn cell(&self, value: &CalcValue) -> Result<TrialCell, Refusal> {
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
        let formatted =
            if self.format.trim().is_empty() || self.format.eq_ignore_ascii_case("General") {
                result::shown(value, Some(&self.profile))
            } else {
                result::formatted_text(
                    value,
                    &self.profile,
                    &self.format,
                    self.environment.date_system,
                )
                .map_err(invalid)?
            };
        Ok(TrialCell {
            raw,
            formatted,
            conditional_match: false,
        })
    }
}

fn trial_context(snapshot: Option<&CallerSnapshot>) -> Result<(Fixture, CellKey), Refusal> {
    let mut fixture = Fixture::default();
    let Some(snapshot) = snapshot else {
        let sheet = fixture.add_sheet("Trial");
        return Ok((
            fixture,
            CellKey::new(sheet, 1, 1).expect("valid internal placement"),
        ));
    };
    fixture.detached = true;
    for sheet in &snapshot.sheets {
        if sheet.name.is_empty()
            || fixture.sheet_name(sheet.sheet.id()).is_some()
            || calc_fml::ReferenceCatalog::sheet_named(&fixture, &sheet.name).is_some()
        {
            return Err(invalid("The caller snapshot has an invalid sheet list"));
        }
        fixture.add_snapshot_sheet(sheet.name.clone(), sheet.sheet.id());
    }
    let cell = |key: calc_skin::grid::Key| {
        key.cell()
            .filter(|at| fixture.sheet_name(at.sheet()).is_some())
            .ok_or_else(|| invalid("A caller snapshot address is outside its sheets"))
    };
    let at = cell(snapshot.at)?;
    let mut inputs = BTreeMap::new();
    for precedent in &snapshot.precedents {
        match &precedent.shown.provenance {
            Provenance::Literal | Provenance::Empty => (),
            Provenance::Calculated { calc } if *calc == snapshot.stamp.calc => (),
            _ => {
                return Err(invalid(
                    "The caller snapshot contains a value that is out of date or unavailable",
                ));
            }
        }
        let key = cell(precedent.at)?;
        if key == at
            || inputs
                .insert(key, snapshot_input(&precedent.shown.value)?)
                .is_some()
        {
            return Err(invalid(
                "The caller snapshot contains a duplicate or self reference",
            ));
        }
    }
    *fixture.inputs_mut() = inputs;
    Ok((fixture, at))
}

fn snapshot_input(raw: &RawValue) -> Result<InputValue, Refusal> {
    let invalid_value = || invalid("The caller snapshot contains an invalid cell value");
    match raw {
        RawValue::Number { bits } => {
            InputValue::number(f64::from_bits(*bits)).map_err(|_| invalid_value())
        }
        RawValue::Text(units) => InputValue::text_units(units.clone()).map_err(|_| invalid_value()),
        RawValue::Logical(value) => Ok(InputValue::logical(*value)),
        RawValue::Error(code) => crate::grid::error_literal(code)
            .map(InputValue::error)
            .ok_or_else(invalid_value),
        RawValue::Blank => Ok(InputValue::blank()),
        RawValue::Other { .. } => Err(invalid_value()),
    }
}

pub(crate) fn local_names(inputs: &[TestInput]) -> Result<BTreeMap<String, String>, Refusal> {
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

pub(crate) fn constant(node: &SyntaxNode) -> bool {
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
        let calculated = self.0.calculate_trial(text);
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

pub(crate) fn invalid(reason: impl Into<String>) -> Refusal {
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
        | Diagnostic::Eval(EvalFault::MissingInput { .. })
        | Diagnostic::Compile(calc_fml::CompileError::Reference {
            error:
                BindError::UnknownSheet { .. }
                | BindError::ExternalWorkbook
                | BindError::SheetSpan
                | BindError::TablesNotModelled,
            ..
        }) => Err(Refusal::RequiresWorkbook {
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
            caller: None,
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
            "='Other sheet'!A1",
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
                    ..
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
                caller: None,
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

#[cfg(test)]
mod tool_tests {
    use super::*;
    use calc_skin::{
        ArrayPosition, CallerSheet, CallerSnapshot, SnapshotCell, Stamp, Trial,
        grid::{Key, Sheet},
    };

    fn trial(formula: &str) -> Trial {
        Trial {
            formula: formula.into(),
            culture: "en-US".into(),
            number_format: "0".into(),
            now: None,
            inputs: vec![],
            caller: None,
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn snapshot_uses_the_supplied_caller_and_values_once() {
        let mut host = FormulaHost::default();
        let mut placed = trial("=ROW()+A1+'Other sheet'!B2");
        placed.caller = Some(CallerSnapshot {
            at: Key(7, 2, 19, 3),
            stamp: Stamp {
                workbook: 1.into(),
                model: 2.into(),
                view: 0.into(),
                calc: 3.into(),
            },
            sheets: vec![
                CallerSheet {
                    sheet: Sheet(7, 2),
                    name: "Inputs".into(),
                },
                CallerSheet {
                    sheet: Sheet(8, 2),
                    name: "Other sheet".into(),
                },
            ],
            precedents: vec![
                SnapshotCell {
                    at: Key(7, 2, 1, 1),
                    shown: Shown {
                        provenance: Provenance::Literal,
                        value: RawValue::Number {
                            bits: 4_f64.to_bits(),
                        },
                    },
                },
                SnapshotCell {
                    at: Key(8, 2, 2, 2),
                    shown: Shown {
                        provenance: Provenance::Calculated { calc: 3.into() },
                        value: RawValue::Number {
                            bits: 8_f64.to_bits(),
                        },
                    },
                },
            ],
            date_1904: false,
        });
        let FormulaAnswer::Evaluated(shown) =
            host.handle(FormulaQuery::Evaluate(placed.clone())).unwrap()
        else {
            panic!("missing output")
        };
        let TrialValue::Scalar(cell) = shown.value else {
            panic!("expected scalar")
        };
        assert_eq!(
            cell.raw,
            RawValue::Number {
                bits: 31_f64.to_bits()
            }
        );
        placed.caller.as_mut().unwrap().precedents[1]
            .shown
            .provenance = Provenance::AsSaved;
        assert!(matches!(
            host.handle(FormulaQuery::Evaluate(placed)),
            Err(Refusal::Invalid { .. })
        ));
        assert!(matches!(
            host.handle(FormulaQuery::Evaluate(trial("=ROW()"))),
            Err(Refusal::RequiresWorkbook { .. })
        ));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn array_pages_read_the_pinned_generation() {
        let mut host = FormulaHost::default();
        let FormulaAnswer::Evaluated(first) = host
            .handle(FormulaQuery::Evaluate(trial("=SEQUENCE(101,33)")))
            .unwrap()
        else {
            panic!("missing output")
        };
        let Provenance::Calculated { calc } = first.provenance else {
            panic!("missing generation")
        };
        let position = ArrayPosition {
            row: 99,
            column: 31,
        };
        let query = FormulaQuery::ArrayWindow { calc, position };
        let FormulaAnswer::Evaluated(next) = host.handle(query.clone()).unwrap() else {
            panic!("missing array page")
        };
        let TrialValue::Array {
            position: actual,
            cells,
            ..
        } = next.value
        else {
            panic!("expected array")
        };
        assert_eq!(actual, position);
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[1][1].raw.display(), "3333");
        host.handle(FormulaQuery::Evaluate(trial("=1"))).unwrap();
        assert!(matches!(host.handle(query), Err(Refusal::Invalid { .. })));
    }
}

#[cfg(test)]
mod tools_tests;
