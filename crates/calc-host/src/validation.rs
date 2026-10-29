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
//! to show, and the entry is taken (Excel asks first). Not modelled, each allowing the entry: a
//! custom rule (its formula reads the cell's new value, which a detached evaluation does not
//! see), and an entry that is a formula (Excel checks its result).

use crate::conditional::projected_value;
use crate::state::HostState;
use crate::{ProjectionRequest, project};
use calc_doc::Validation;
use calc_doc_model::CellInput;
use calc_doc_model::value::{CalcValue, CellKey, CoreRef};
use calc_engine::DetachedFormula;
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

impl HostState {
    /// The validation that applies to `at` (the first that holds it), with the areas moved by the
    /// session's row and column edits, and the cell its formulas are relative to.
    fn validation_of(&self, at: CellKey) -> Option<(&Validation, CellKey)> {
        let loaded = self.loaded.sheet(at.sheet())?;
        loaded.validations().iter().find_map(|validation| {
            let areas = self.current_areas(at.sheet(), &validation.areas);
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
            self.formula_values(formula, origin, at)?
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
        let compiled =
            DetachedFormula::compile_dynamic(&self.model, formula.trim_start_matches('='), origin)
                .ok()?;
        let value = compiled.evaluate(&self.engine, &self.model, at).ok()?;
        match value.core_ref() {
            CoreRef::Reference(reference) => {
                let mut cells = Vec::new();
                for area in reference.areas() {
                    for row in area.first().row()..=area.last().row() {
                        for column in area.first().column()..=area.last().column() {
                            if cells.len() >= LIST_LIMIT {
                                break;
                            }
                            if let Ok(cell) = CellKey::new(area.sheet(), row, column) {
                                cells.push(cell);
                            }
                        }
                    }
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
        let Some((validation, origin)) = self.validation_of(at) else {
            return Ok(());
        };
        if validation.kind == "none" || !validation.show_error {
            return Ok(());
        }
        let fail = || {
            ValidationFailure {
            style: style_of(validation),
            title: validation
                .error_title
                .clone()
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| "Microsoft Excel".to_owned()),
            message: validation.error.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| {
                "This value doesn't match the data validation restrictions defined for this cell."
                    .to_owned()
            }),
        }
        };
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
        let bound = |text: &Option<String>| -> Option<f64> {
            let text = text.as_deref()?.trim_start_matches('=');
            if let Ok(n) = text.trim().parse::<f64>() {
                return Some(n);
            }
            let compiled = DetachedFormula::compile(&self.model, text, origin).ok()?;
            match compiled
                .evaluate(&self.engine, &self.model, at)
                .ok()?
                .core_ref()
            {
                CoreRef::Number(n) => Some(n),
                _ => None,
            }
        };
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
                match self.validation_list(at) {
                    Some(items) => items.iter().any(|item| matches(item, input, literal)),
                    None => true,
                }
            }
            _ => true,
        };
        if allowed { Ok(()) } else { Err(fail()) }
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
