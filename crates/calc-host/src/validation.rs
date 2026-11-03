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
        self.validation_list_for(at, true)
    }

    fn validation_list_for(&self, at: CellKey, caches: bool) -> Option<Vec<CalcValue>> {
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
            self.formula_values_with_caches(formula, origin, at, caches)?
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
                if !caches
                    && page.cells().iter().any(|c| {
                        !matches!(
                            c.value(),
                            crate::ProjectedValue::Empty
                                | crate::ProjectedValue::Literal(_)
                                | crate::ProjectedValue::Calculated { .. }
                        )
                    })
                {
                    return None;
                }
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
        self.check_entry_rule(at, input, true)
    }

    fn check_entry_rule(
        &self,
        at: CellKey,
        input: Option<&CellInput>,
        alerts: bool,
    ) -> Result<(), ValidationFailure> {
        let Some((validation, origin)) = self.validation_of(at) else {
            return Ok(());
        };
        if validation.kind == "none" || (alerts && !validation.show_error ) {
            return Ok(());
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
        let bound = |text: &Option<String>| self.validation_bound(text , origin, at)
                ;
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
    fn validation_bound(&self, text: &Option<String>, origin: CellKey, at: CellKey) -> Option<f64> {
        let text = text.as_deref()?.trim_start_matches('=');
        if let Ok(n) = text.trim().parse::<f64>() {
            return n.is_finite().then_some(n);
        }
        let formula = DetachedFormula::compile(&self.model, text, origin).ok()?;
        match formula
            .evaluate(&self.engine, &self.model, at)
            .ok()?
            .core_ref()
        {
            CoreRef::Number(n) if n.is_finite() => Some(n),
            _ => None,
        }
    }

    /// Inspect the current published cell, independent of whether its entry alert is enabled.
    /// File caches, stale or refused calculations and unavailable criteria remain counted apart.
    pub fn validation_status(&self, at: CellKey) -> Option<ValidationStatus> {
        use crate::ProjectedValue;
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
        let page = match project(self, &ProjectionRequest { cells: vec![at] }) {
            Ok(p) => p,
            Err(_) => return unavailable("The cell cannot be read"),
        };
        let Some(cell) = page.cells().first() else {
            return unavailable("The cell cannot be read");
        };
        let value = match cell.value() {
            ProjectedValue::Empty => None,
            ProjectedValue::Literal(v) | ProjectedValue::Calculated { value: v, .. } => Some(v),
            _ => return unavailable("The cell has no current calculated value"),
        };
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
        let relative = !self.axis_edits.is_empty();
        if relative
            && [&rule.formula1, &rule.formula2]
                .iter()
                .filter_map(|f| f.as_deref())
                .any(|f| f.trim().parse::<f64>().is_err() && literal_items(f).is_none())
        {
            return unavailable(
                "Validation references changed; save and reopen before checking them",
            );
        }
        if rule.kind == "custom" {
            let Some(formula) = rule.formula1.as_deref().and_then(|f| {
                DetachedFormula::compile(&self.model, f.trim_start_matches('='), origin).ok()
            }) else {
                return unavailable("The custom validation formula is unavailable");
            };
            let Ok(value) = formula.evaluate(&self.engine, &self.model, at) else {
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
                if self.validation_bound(&rule.formula1, origin, at).is_none()
                    || (matches!(rule.operator.as_str(), "between" | "notBetween" | "")
                        && self.validation_bound(&rule.formula2, origin, at).is_none())
                {
                    return unavailable("A validation bound has no current value");
                }
            }
            "list" => {
                if self.validation_list_for(at, false).is_none() {
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
        Some(match self.check_entry_rule(at, input.as_ref(), false) {
            Ok(()) => ValidationStatus::Valid,
            Err(f) => ValidationStatus::Invalid(f),
        })
    }

    /// Bounded, deduplicated worksheet audit. Unchecked cells are explicit, never counted valid.
    pub fn scan_validation(&self, sheet: calc_doc_model::value::SheetId) -> ValidationScan {
        const LIMIT: usize = 10_000;
        let mut report = ValidationScan::default();
        let mut keys = std::collections::BTreeSet::new();
        if let Some(loaded) = self.loaded.sheet(sheet) {
            'rules: for rule in loaded.validations().iter().filter(|r| r.kind != "none") {
                for area in self.current_areas(sheet, &rule.areas) {
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
        for at in keys {
            report.checked += 1;
                match self.validation_status(at) {
                    Some(ValidationStatus::Invalid(_ )) => report.invalid.push(at),
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
