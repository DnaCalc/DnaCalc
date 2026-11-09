//! Data validation (dc-6yw): the rule that applies to a cell, the items of a list's drop-down, and
//! whether an entry is one the rule allows, as Excel 16.0.20430 decides it (measured through
//! `Range.Validation.Value`: tests/fixtures/dc_6yw/dv-excel.txt).
//!
//! - A rule whose error message is off (`showErrorMessage="0"`) allows every entry, as Excel does.
//! - A blank entry is allowed when the rule allows blanks.
//! - Whole number, decimal, date and time: the entry must be a number (a whole one for a whole
//!   number rule) that meets the operator against the rule's formulas, which are evaluated at the
//!   cell by the one evaluator ([`calc_engine::DetachedFormula`]); text length: the entry's
//!   length as text.
//! - A list: the entry must be one of its items. A list written in the rule (`"Red,Green,Blue"`)
//!   matches text exactly, case included (`green` is refused); a list read from cells matches
//!   regardless of case (`north` is allowed against North), as Excel's measurements show.
//!
//! [`crate::HostSession`] refuses an entry a stop rule does not allow
//! ([`crate::IntentError::Validation`]); a warning or information rule's message is the caller's
//! to show before the entry is kept. Custom rules and formula entries use a disposable authored
//! candidate and the ordinary evaluator, including its dependent formulas. Unavailable candidate
//! calculations return a Stop refusal naming their owner; nothing commits.

use crate::conditional::projected_value;
use crate::state::HostState;
use crate::{ProjectionRequest, project};
use calc_doc::Validation;
use calc_doc_model::CellInput;
use calc_doc_model::value::{CalcValue, CellKey, CoreRef};
use calc_engine::{DetachedEvaluationContext, DetachedReadPolicy};
use std::fmt;

/// How Excel answers an entry a rule does not allow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorStyle {
    /// The entry is refused.
    Stop,
    /// The user is asked whether to keep it.
    Warning,
    /// The user is told, and it is kept.
    Information,
}

/// The rule that applies to a cell, as a skin shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationRule {
    pub kind: String,
    pub style: ErrorStyle,
    /// Whether the cell shows a drop-down (a list whose file does not hide it).
    pub dropdown: bool,
    /// The input message, when the rule shows one: its title and text.
    pub prompt: Option<(String, String)>,
}

/// Why an entry is not allowed: the rule's style, and the title and message Excel shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationFailure {
    pub style: ErrorStyle,
    pub title: String,
    pub message: String,
}

impl fmt::Display for ValidationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.title, self.message)
    }
}

/// The most items a drop-down lists.
pub const LIST_LIMIT: usize = 32_767;

fn style_of(validation: &Validation) -> ErrorStyle {
    match validation.error_style.as_str() {
        "warning" => ErrorStyle::Warning,
        "information" => ErrorStyle::Information,
        _ => ErrorStyle::Stop,
    }
}

/// The items of a list written in the rule: the text between its quotes, split at commas.
fn literal_items(formula: &str) -> Option<Vec<String>> {
    let inner = formula.trim().strip_prefix('"')?.strip_suffix('"')?;
    Some(
        inner
            .replace("\"\"", "\"")
            .split(',')
            .map(|item| item.trim().to_owned())
            .collect(),
    )
}

/// An audit never treats unavailable values or rules as valid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationStatus {
    Valid,
    Invalid(ValidationFailure),
    Unavailable {
        reason: &'static str,
        owner: &'static str,
    },
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ValidationScan {
    pub invalid: Vec<CellKey>,
    pub checked: u32,
    pub valid: u32,
    pub unavailable: u32,
    pub truncated: bool,
    pub reasons: Vec<(String, u32)>,
}

fn failure_of(validation: &Validation) -> ValidationFailure {
    ValidationFailure {
        style: style_of(validation),
        title: validation
            .error_title
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Microsoft Excel".into()),
        message: validation
            .error
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                "This value doesn't match the data validation restrictions defined for this cell."
                    .into()
            }),
    }
}

impl HostState {
    /// The validation that applies to `at` (the first that holds it), with the areas moved by the
    /// session's row and column edits, and the cell its formulas are relative to.
    fn validation_of(&self, at: CellKey) -> Option<(&Validation, CellKey)> {
        self.model
            .definitions()
            .validations(at.sheet())
            .iter()
            .find_map(|validation| {
                let areas = validation
                    .areas
                    .iter()
                    .filter_map(|&(t, l, b, r)| {
                        calc_value::GridArea::new(
                            CellKey::new(at.sheet(), t, l).ok()?,
                            CellKey::new(at.sheet(), b, r).ok()?,
                        )
                        .ok()
                    })
                    .collect::<Vec<_>>();
                areas
                    .iter()
                    .any(|area| area.contains(at))
                    .then(|| (validation, areas[0].first()))
            })
    }

    /// The data validation rule of the cell `at`, `None` when none applies.
    pub fn validation(&self, at: CellKey) -> Option<ValidationRule> {
        let (validation, _) = self.validation_of(at)?;
        if validation.kind == "none" {
            return None;
        }
        Some(ValidationRule {
            kind: validation.kind.clone(),
            style: style_of(validation),
            dropdown: validation.kind == "list" && validation.in_cell_dropdown,
            prompt: validation
                .show_input
                .then(|| {
                    (
                        validation.prompt_title.clone().unwrap_or_default(),
                        validation.prompt.clone().unwrap_or_default(),
                    )
                })
                .filter(|(title, text)| !title.is_empty() || !text.is_empty()),
        })
    }

    /// The items of the cell's list rule, in order: the values a list written in the rule names
    /// (as text), or those of the cells its formula refers to; `None` when no list rule applies
    /// or its formula gives no list.
    pub fn validation_list(&self, at: CellKey) -> Option<Vec<CalcValue>> {
        self.validation_list_for(at, true)
    }

    fn validation_list_for(&self, at: CellKey, caches: bool) -> Option<Vec<CalcValue>> {
        self.validation_list_in(at, caches, None)
    }

    fn validation_list_in(
        &self,
        at: CellKey,
        caches: bool,
        context: Option<&DetachedEvaluationContext<'_>>,
    ) -> Option<Vec<CalcValue>> {
        let (validation, origin) = self.validation_of(at)?;
        if validation.kind != "list" {
            return None;
        }
        let formula = validation.formula1.as_deref()?;
        if let Some(items) = literal_items(formula) {
            return Some(
                items
                    .into_iter()
                    .map(|item| {
                        CalcValue::text(calc_doc_model::value::ExcelText::from_utf16_code_units(
                            item.encode_utf16().collect(),
                        ))
                    })
                    .collect(),
            );
        }
        Some(
            self.formula_values_in(formula, origin, at, caches, context)?
                .into_iter()
                .flatten()
                .collect(),
        )
    }

    /// The values a formula gives at `at` (relative to `origin`), evaluated by the one evaluator
    /// ([`calc_engine::DetachedFormula`], in its dynamic-array form): a reference's cells in
    /// order, area by area and row by row (`None` for a blank cell or one without a value; at most
    /// [`LIST_LIMIT`]), an array's values row by row, or the one value. `None` when the formula
    /// does not compile or evaluate. A validation list's items and a chart's series read this.
    pub(crate) fn formula_values(
        &self,
        formula: &str,
        origin: CellKey,
        at: CellKey,
    ) -> Option<Vec<Option<CalcValue>>> {
        self.formula_values_with_caches(formula, origin, at, true)
    }

    fn formula_values_with_caches(
        &self,
        formula: &str,
        origin: CellKey,
        at: CellKey,
        caches: bool,
    ) -> Option<Vec<Option<CalcValue>>> {
        self.formula_values_in(formula, origin, at, caches, None)
    }

    fn formula_values_in(
        &self,
        formula: &str,
        origin: CellKey,
        at: CellKey,
        caches: bool,
        context: Option<&DetachedEvaluationContext<'_>>,
    ) -> Option<Vec<Option<CalcValue>>> {
        let owned;
        let context = match context {
            Some(context) => context,
            None => {
                owned = if caches {
                    DetachedEvaluationContext::new(
                        &self.engine,
                        &self.model,
                        DetachedReadPolicy::PublishedValues,
                    )
                } else {
                    DetachedEvaluationContext::new_with_numeric_mode(
                        &self.engine,
                        &self.model,
                        DetachedReadPolicy::CurrentValues,
                        &self.options.numeric,
                    )
                }
                .ok()?;
                &owned
            }
        };
        let compiled = context
            .compile(
                formula.trim_start_matches('='),
                origin,
                calc_fml::FormulaForm::Dynamic,
            )
            .ok()?;
        let value = compiled.evaluate_in(context, at).ok()?;
        match value.core_ref() {
            CoreRef::Reference(reference) => {
                if !caches
                    && reference
                        .areas()
                        .iter()
                        .try_fold(LIST_LIMIT as u64, |remaining, area| {
                            remaining.checked_sub(area.cell_count())
                        })
                        .is_none()
                {
                    return None;
                }
                let mut cells = Vec::new();
                'areas: for area in reference.areas() {
                    for row in area.first().row()..=area.last().row() {
                        for column in area.first().column()..=area.last().column() {
                            if cells.len() >= LIST_LIMIT {
                                break 'areas;
                            }
                            if let Ok(cell) = CellKey::new(area.sheet(), row, column) {
                                cells.push(cell);
                            }
                        }
                    }
                }
                if !caches {
                    return cells
                        .into_iter()
                        .map(|at| {
                            context.read_cell(at).ok().map(|value| {
                                (!matches!(value.core_ref(), CoreRef::Empty)).then_some(value)
                            })
                        })
                        .collect();
                }
                let page = project(self, &ProjectionRequest { cells }).ok()?;
                Some(
                    page.cells()
                        .iter()
                        .map(|cell| projected_value(cell.value()))
                        .collect(),
                )
            }
            CoreRef::Array(array) => Some(array.iter_row_major().cloned().map(Some).collect()),
            _ => Some(vec![Some(value)]),
        }
    }

    /// Whether the cell's rule allows `input` as its entry (`None` for a blank entry); the failure
    /// when it does not.
    pub fn check_entry(
        &self,
        at: CellKey,
        input: Option<&CellInput>,
    ) -> Result<(), ValidationFailure> {
        self.check_entry_rule(at, input, true, None)
    }

    fn check_entry_rule(
        &self,
        at: CellKey,
        input: Option<&CellInput>,
        alerts: bool,
        context: Option<&DetachedEvaluationContext<'_>>,
    ) -> Result<(), ValidationFailure> {
        let Some((validation, origin)) = self.validation_of(at) else {
            return Ok(());
        };
        if validation.kind == "none" || (alerts && !validation.show_error) {
            return Ok(());
        }
        let formula_criteria = if validation.kind == "list" {
            validation
                .formula1
                .as_deref()
                .is_some_and(|f| literal_items(f).is_none())
        } else {
            [&validation.formula1, &validation.formula2]
                .into_iter()
                .flatten()
                .any(|f| f.trim().parse::<f64>().is_err())
        };
        if alerts
            && (validation.kind == "custom"
                || formula_criteria
                || matches!(input, Some(CellInput::Formula(_))))
        {
            return self.check_candidate(at, input, validation, origin);
        }
        let fail = || failure_of(validation);
        let Some(input) = input else {
            return if validation.allow_blank {
                Ok(())
            } else {
                Err(fail())
            };
        };
        let number = |input: &CellInput| match input {
            CellInput::Number(n) => Some(n.get()),
            _ => None,
        };
        let bound = |text: &Option<String>| self.validation_bound(text, origin, at, context);
        let meets = |value: f64| -> Option<bool> {
            let one = bound(&validation.formula1)?;
            Some(match validation.operator.as_str() {
                "notBetween" => {
                    let two = bound(&validation.formula2)?;
                    value < one.min(two) || value > one.max(two)
                }
                "equal" => value == one,
                "notEqual" => value != one,
                "greaterThan" => value > one,
                "lessThan" => value < one,
                "greaterThanOrEqual" => value >= one,
                "lessThanOrEqual" => value <= one,
                _ => {
                    let two = bound(&validation.formula2)?;
                    value >= one.min(two) && value <= one.max(two)
                }
            })
        };
        if matches!(input, CellInput::Formula(_)) {
            return Ok(());
        }
        let allowed = match validation.kind.as_str() {
            "whole" => number(input).is_some_and(|n| n.fract() == 0.0 && meets(n).unwrap_or(true)),
            "decimal" | "date" | "time" => number(input).is_some_and(|n| meets(n).unwrap_or(true)),
            "textLength" => {
                let text = match input {
                    CellInput::Text(text) => text.text().to_string_lossy(),
                    CellInput::Number(n) => {
                        let n = n.get();
                        if n.fract() == 0.0 && n.abs() < 1e15 {
                            format!("{}", n as i64)
                        } else {
                            format!("{n}")
                        }
                    }
                    CellInput::Logical(b) => if *b { "TRUE" } else { "FALSE" }.to_owned(),
                    _ => String::new(),
                };
                meets(text.encode_utf16().count() as f64).unwrap_or(true)
            }
            "list" => {
                let literal = validation
                    .formula1
                    .as_deref()
                    .is_some_and(|f| f.trim().starts_with('"'));
                match self.validation_list_in(at, context.is_none(), context) {
                    Some(items) => items.iter().any(|item| matches(item, input, literal)),
                    None => true,
                }
            }
            _ => true,
        };
        if allowed { Ok(()) } else { Err(fail()) }
    }
    /// Evaluate the proposed authored input in a disposable engine through the ordinary
    /// calculation path. The live model, results and history are never modified. Dependent
    /// formulas see the candidate too; no file cache supplies a validation value.
    fn check_candidate(
        &self,
        at: CellKey,
        input: Option<&CellInput>,
        rule: &Validation,
        origin: CellKey,
    ) -> Result<(), ValidationFailure> {
        use calc_doc_model::{ArrayFormula, ArrayKind, FormulaText, OpBatch, WorkbookOp};
        let unavailable = |reason: &str| ValidationFailure {
            style: ErrorStyle::Stop,
            title: "Validation unavailable (dc-971p.40)".into(),
            message: reason.into(),
        };
        if input.is_none() && rule.allow_blank {
            return Ok(());
        }
        // Providers can carry mutable state shared by their Rc. A scratch engine must not call
        // them as a side effect of an entry that may be rejected.
        // The host's own routes (fetch, import, RTD) are registered in every page; only a workbook
        // that calls a registered function, or a registered add-in or VBA function, is refused.
        if self.functions.names().next().is_some() || self.engine.has_user_calls() {
            return Err(unavailable(
                "Candidate validation with registered user functions is unavailable.",
            ));
        }
        if self.engine.nodes().take(4097).count() > 4096 {
            return Err(unavailable(
                "Candidate validation currently checks workbooks with at most 4,096 formulas.",
            ));
        }
        let operation = match input {
            Some(CellInput::Formula(f)) => {
                let file = calc_doc::file_form(f, self.model.definitions().names())
                    .map_err(|_| unavailable("The candidate formula cannot be read."))?;
                let f = FormulaText::with_form(
                    calc_value::ExcelText::from_utf16_code_units(file.text),
                    f.form(),
                )
                .map_err(|_| unavailable("The candidate formula cannot be read."))?;
                if file.dynamic {
                    WorkbookOp::SetArray {
                        array: ArrayFormula::new(
                            calc_value::GridArea::cell(at),
                            ArrayKind::Dynamic { collapsed: false },
                        ),
                        input: CellInput::Formula(f),
                    }
                } else {
                    WorkbookOp::SetCell {
                        at,
                        input: CellInput::Formula(f),
                    }
                }
            }
            Some(input) => WorkbookOp::SetCell {
                at,
                input: input.clone(),
            },
            None => WorkbookOp::Clear {
                area: calc_value::GridArea::cell(at),
            },
        };
        let batch = OpBatch {
            base: self.model.revision(),
            operations: vec![operation],
        };
        let model = calc_doc::prepare_edit(&self.model, &batch)
            .map_err(|_| unavailable("The candidate cannot be applied to this cell."))?
            .into_model();
        let mut engine = self.engine.rebuilt(&model);
        engine.set_tables_deferred(false);
        engine.set_workbook_facts(Some(self.workbook_facts()));
        let mut work = calc_engine::begin_recalc(&mut engine, &model, self.options.numeric.clone())
            .map_err(|_| unavailable("The candidate calculation cannot start."))?
            .with_external_inputs(self.rtd_inputs.clone());
        if engine.has_user_calls() {
            return Err(unavailable(
                "Candidate validation with registered user functions is unavailable.",
            ));
        }
        if calc_engine::step(
            &engine,
            &model,
            &mut work,
            calc_engine::WorkBudget::evaluations(32768),
            &mut calc_engine::Unobserved,
        ) != calc_engine::RecalcProgress::Complete
        {
            return Err(unavailable(
                "The candidate calculation needs more work or an external input.",
            ));
        }
        calc_engine::finish(&mut engine, &model, work)
            .map_err(|_| unavailable("The candidate calculation did not finish."))?;
        if !engine.circular_cells().is_empty() {
            return Err(unavailable(
                "Candidate validation of circular calculations is unavailable.",
            ));
        }
        let context = DetachedEvaluationContext::new_with_numeric_mode(
            &engine,
            &model,
            DetachedReadPolicy::CurrentValues,
            &self.options.numeric,
        )
        .map_err(|_| unavailable("The candidate snapshot is unavailable."))?;
        let values = CandidateValues { context };
        let value = values
            .cell(at)
            .ok_or_else(|| unavailable("The candidate has no calculated value."))?;
        if matches!(value.core_ref(), CoreRef::Empty) && rule.allow_blank {
            return Ok(());
        }
        let bound = |f: &Option<String>| -> Option<f64> {
            match values
                .formula(f.as_deref()?, origin, at)?
                .first()?
                .as_ref()?
                .core_ref()
            {
                CoreRef::Number(n) => Some(n),
                _ => None,
            }
        };
        let meets = |v: f64| -> Option<bool> {
            let one = bound(&rule.formula1)?;
            Some(match rule.operator.as_str() {
                "equal" => v == one,
                "notEqual" => v != one,
                "greaterThan" => v > one,
                "lessThan" => v < one,
                "greaterThanOrEqual" => v >= one,
                "lessThanOrEqual" => v <= one,
                "between" | "" => {
                    let two = bound(&rule.formula2)?;
                    v >= one.min(two) && v <= one.max(two)
                }
                "notBetween" => {
                    let two = bound(&rule.formula2)?;
                    v < one.min(two) || v > one.max(two)
                }
                _ => return None,
            })
        };
        let allowed = match rule.kind.as_str() {
            "custom" => values
                .formula(
                    rule.formula1
                        .as_deref()
                        .ok_or_else(|| unavailable("The validation formula is missing."))?,
                    origin,
                    at,
                )
                .and_then(|v| v.into_iter().next().flatten())
                .and_then(|v| match v.core_ref() {
                    CoreRef::Logical(b) => Some(b),
                    CoreRef::Number(n) => Some(n != 0.),
                    CoreRef::Error(_) => Some(false),
                    _ => None,
                }),
            "whole" | "decimal" | "date" | "time" => match value.core_ref() {
                CoreRef::Number(n) => {
                    if rule.kind == "whole" && n.fract() != 0. {
                        Some(false)
                    } else {
                        meets(n)
                    }
                }
                _ => Some(false),
            },
            "textLength" => meets(crate::chart::text_of(&value).encode_utf16().count() as f64),
            "list" => rule.formula1.as_deref().and_then(|f| {
                let literal = literal_items(f);
                let is_literal = literal.is_some();
                let items = if let Some(items) = literal {
                    Some(
                        items
                            .into_iter()
                            .map(|s| {
                                Some(CalcValue::text(
                                    calc_value::ExcelText::from_utf16_code_units(
                                        s.encode_utf16().collect(),
                                    ),
                                ))
                            })
                            .collect(),
                    )
                } else {
                    values.formula(f, origin, at)
                }?;
                let input = value_input(&value)?;
                Some(
                    items
                        .into_iter()
                        .flatten()
                        .any(|item| matches(&item, &input, is_literal)),
                )
            }),
            _ => None,
        }
        .ok_or_else(|| unavailable("The validation criteria have no current calculated value."))?;
        if allowed {
            Ok(())
        } else {
            Err(failure_of(rule))
        }
    }

    fn validation_bound(
        &self,
        text: &Option<String>,
        origin: CellKey,
        at: CellKey,
        context: Option<&DetachedEvaluationContext<'_>>,
    ) -> Option<f64> {
        let text = text.as_deref()?.trim_start_matches('=');
        if let Ok(n) = text.trim().parse::<f64>() {
            return n.is_finite().then_some(n);
        }
        let owned;
        let context = match context {
            Some(context) => context,
            None => {
                owned = DetachedEvaluationContext::new_with_numeric_mode(
                    &self.engine,
                    &self.model,
                    DetachedReadPolicy::CurrentValues,
                    &self.options.numeric,
                )
                .ok()?;
                &owned
            }
        };
        let formula = context
            .compile(text, origin, calc_fml::FormulaForm::Legacy)
            .ok()?;
        match formula.evaluate_in(context, at).ok()?.core_ref() {
            CoreRef::Number(n) if n.is_finite() => Some(n),
            _ => None,
        }
    }

    /// Inspect the current published cell, independent of whether its entry alert is enabled.
    /// File caches, stale or refused calculations and unavailable criteria remain counted apart.
    pub fn validation_status(&self, at: CellKey) -> Option<ValidationStatus> {
        let context = DetachedEvaluationContext::new_with_numeric_mode(
            &self.engine,
            &self.model,
            DetachedReadPolicy::CurrentValues,
            &self.options.numeric,
        )
        .ok()?;
        self.validation_status_in(at, &context)
    }

    fn validation_status_in(
        &self,
        at: CellKey,
        context: &DetachedEvaluationContext<'_>,
    ) -> Option<ValidationStatus> {
        let (rule, origin) = self.validation_of(at)?;
        if rule.kind == "none" {
            return None;
        }
        let unavailable = |reason| {
            Some(ValidationStatus::Unavailable {
                reason,
                owner: "dc-971p.40",
            })
        };
        let current = match context.read_cell(at) {
            Ok(value) => value,
            Err(_) => return unavailable("The cell has no current calculated value"),
        };
        let value = (!matches!(current.core_ref(), CoreRef::Empty)).then_some(&current);
        if self
            .merged_areas(at.sheet())
            .iter()
            .any(|area| area.contains(at) && area.first() != at)
        {
            return unavailable("Validation on a covered merged cell cannot be checked");
        }
        let blank = value.is_none_or(|v| matches!(v.core_ref(), CoreRef::Empty));
        if blank && rule.allow_blank {
            return Some(ValidationStatus::Valid);
        }
        if rule.kind == "custom" {
            let Some(formula) = rule.formula1.as_deref().and_then(|f| {
                context
                    .compile(
                        f.trim_start_matches('='),
                        origin,
                        calc_fml::FormulaForm::Legacy,
                    )
                    .ok()
            }) else {
                return unavailable("The custom validation formula is unavailable");
            };
            let Ok(value) = formula.evaluate_in(context, at) else {
                return unavailable("The custom validation formula has no current value");
            };
            let allowed = match value.core_ref() {
                CoreRef::Logical(b) => b,
                CoreRef::Number(n) => n != 0.,
                _ => return unavailable("The custom validation formula has no truth value"),
            };
            return Some(if allowed {
                ValidationStatus::Valid
            } else {
                ValidationStatus::Invalid(failure_of(rule))
            });
        }
        match rule.kind.as_str() {
            "whole" | "decimal" | "date" | "time" | "textLength" => {
                if !matches!(
                    rule.operator.as_str(),
                    "" | "between"
                        | "notBetween"
                        | "equal"
                        | "notEqual"
                        | "greaterThan"
                        | "lessThan"
                        | "greaterThanOrEqual"
                        | "lessThanOrEqual"
                ) {
                    return unavailable("This validation comparison is unavailable");
                }
                if self
                    .validation_bound(&rule.formula1, origin, at, Some(context))
                    .is_none()
                    || (matches!(rule.operator.as_str(), "between" | "notBetween" | "")
                        && self
                            .validation_bound(&rule.formula2, origin, at, Some(context))
                            .is_none())
                {
                    return unavailable("A validation bound has no current value");
                }
            }
            "list" => {
                if self.validation_list_in(at, false, Some(context)).is_none() {
                    return unavailable("The validation list has no current values");
                }
            }
            _ => return unavailable("This validation type is unavailable"),
        }
        let input = match value.map(|v| v.core_ref()) {
            None | Some(CoreRef::Empty) => None,
            Some(CoreRef::Number(n)) => match calc_doc_model::FiniteNumber::new(n) {
                Some(n) => Some(CellInput::Number(n)),
                None => return unavailable("The cell number cannot be checked"),
            },
            Some(CoreRef::Text(t)) => match calc_doc_model::CellText::new(t.clone()) {
                Some(t) => Some(CellInput::Text(t)),
                None => return unavailable("The cell text is too long to check"),
            },
            Some(CoreRef::Logical(b)) => Some(CellInput::Logical(b)),
            Some(CoreRef::Error(e)) => Some(CellInput::Error(e)),
            _ => return unavailable("This cell value cannot be checked"),
        };
        Some(
            match self.check_entry_rule(at, input.as_ref(), false, Some(context)) {
                Ok(()) => ValidationStatus::Valid,
                Err(f) => ValidationStatus::Invalid(f),
            },
        )
    }

    /// Bounded, deduplicated worksheet audit. Unchecked cells are explicit, never counted valid.
    pub fn scan_validation(&self, sheet: calc_doc_model::value::SheetId) -> ValidationScan {
        const LIMIT: usize = 10_000;
        let mut report = ValidationScan::default();
        let mut keys = std::collections::BTreeSet::new();
        {
            'rules: for rule in self
                .model
                .definitions()
                .validations(sheet)
                .iter()
                .filter(|r| r.kind != "none")
            {
                for area in rule.areas.iter().filter_map(|&(t, l, b, r)| {
                    calc_value::GridArea::new(
                        CellKey::new(sheet, t, l).ok()?,
                        CellKey::new(sheet, b, r).ok()?,
                    )
                    .ok()
                }) {
                    for row in area.first().row()..=area.last().row() {
                        for col in area.first().column()..=area.last().column() {
                            let at = CellKey::new(sheet, row, col).expect("valid validation area");
                            if keys.contains(&at) {
                                continue;
                            }
                            if keys.len() == LIMIT {
                                report.truncated = true;
                                break 'rules;
                            }
                            keys.insert(at);
                        }
                    }
                }
            }
        }
        let mut reasons = std::collections::BTreeMap::<String, u32>::new();
        let context = DetachedEvaluationContext::new_with_numeric_mode(
            &self.engine,
            &self.model,
            DetachedReadPolicy::CurrentValues,
            &self.options.numeric,
        )
        .expect("host model and engine share a revision");
        for at in keys {
            report.checked += 1;
            match self.validation_status_in(at, &context) {
                Some(ValidationStatus::Invalid(_)) => report.invalid.push(at),
                Some(ValidationStatus::Unavailable { reason, .. }) => {
                    report.unavailable += 1;
                    *reasons.entry(reason.into()).or_default() += 1;
                }
                _ => report.valid += 1,
            }
        }
        report.reasons = reasons.into_iter().collect();
        report
    }
}

/// Whether a list item matches an entry: a number by value (an item written as text read as a
/// number when it is one), a text exactly for a list written in the rule and regardless of case
/// for one read from cells, a logical by value.
fn matches(item: &CalcValue, input: &CellInput, literal: bool) -> bool {
    match (item.core_ref(), input) {
        (CoreRef::Number(a), CellInput::Number(b)) => a == b.get(),
        (CoreRef::Text(a), CellInput::Number(b)) => a
            .to_string_lossy()
            .trim()
            .parse::<f64>()
            .is_ok_and(|a| a == b.get()),
        (CoreRef::Text(a), CellInput::Text(b)) => {
            let (a, b) = (a.to_string_lossy(), b.text().to_string_lossy());
            if literal {
                a == b
            } else {
                a.to_lowercase() == b.to_lowercase()
            }
        }
        (CoreRef::Logical(a), CellInput::Logical(b)) => a == *b,
        _ => false,
    }
}

struct CandidateValues<'a> {
    context: DetachedEvaluationContext<'a>,
}
impl CandidateValues<'_> {
    fn cell(&self, at: CellKey) -> Option<CalcValue> {
        self.context.read_cell(at).ok()
    }
    fn formula(&self, text: &str, origin: CellKey, at: CellKey) -> Option<Vec<Option<CalcValue>>> {
        let formula = self
            .context
            .compile(
                text.trim_start_matches('='),
                origin,
                calc_fml::FormulaForm::Dynamic,
            )
            .ok()?;
        let value = formula.evaluate_in(&self.context, at).ok()?;
        match value.core_ref() {
            CoreRef::Reference(r) => {
                let mut values = Vec::new();
                for area in r.areas() {
                    if values.len() as u64 + area.cell_count() > LIST_LIMIT as u64 {
                        return None;
                    }
                    for row in area.first().row()..=area.last().row() {
                        for col in area.first().column()..=area.last().column() {
                            values
                                .push(Some(self.cell(CellKey::new(area.sheet(), row, col).ok()?)?));
                        }
                    }
                }
                Some(values)
            }
            CoreRef::Array(a) => Some(a.iter_row_major().cloned().map(Some).collect()),
            _ => Some(vec![Some(value)]),
        }
    }
}
fn value_input(v: &CalcValue) -> Option<CellInput> {
    match v.core_ref() {
        CoreRef::Number(n) => Some(CellInput::Number(calc_doc_model::FiniteNumber::new(n)?)),
        CoreRef::Text(t) => Some(CellInput::Text(calc_doc_model::CellText::new(t.clone())?)),
        CoreRef::Logical(b) => Some(CellInput::Logical(b)),
        CoreRef::Error(e) => Some(CellInput::Error(e)),
        _ => None,
    }
}
