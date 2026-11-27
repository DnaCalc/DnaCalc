//! Exact comparisons and the portable test-suite file schema.

use crate::Session;
use crate::trial::{constant, evaluate_value, invalid};
use calc_formula_kernel::display::formula_literal;
use calc_func::value::{CalcValue, CoreValue};
use calc_skin::{Refusal, TestCase, TestOutcome, TestSuite, TestVerdict};

pub(crate) fn validate(suite: &TestSuite) -> Result<(), Refusal> {
    if suite.schema != TestSuite::SCHEMA {
        return Err(invalid("This test suite uses an unsupported schema"));
    }
    if suite.cases.is_empty() {
        return Err(invalid("Add at least one test case"));
    }
    Ok(())
}

pub(crate) fn run(session: &mut Session, suite: TestSuite) -> Result<Vec<TestOutcome>, Refusal> {
    validate(&suite)?;
    for required in &suite.providers {
        let present = session.addins().iter().any(|loaded| {
            let name = loaded
                .path
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(&loaded.path);
            name.eq_ignore_ascii_case(&required.name)
                && loaded.hash.eq_ignore_ascii_case(&required.sha256)
        });
        if !present {
            return Err(invalid(format!(
                "Load the required provider {} (SHA-256 {})",
                required.name, required.sha256
            )));
        }
    }
    Ok(suite
        .cases
        .into_iter()
        .map(|case| run_case(session, case))
        .collect())
}

fn run_case(session: &mut Session, case: TestCase) -> TestOutcome {
    let outcome = |verdict, actual, formatted| TestOutcome {
        verdict,
        actual,
        formatted,
    };
    let expected = format!("={}", case.expected.trim());
    let syntax = calc_fml::parse_formula(&expected);
    if !syntax.diagnostics().is_empty() || !syntax.expression().is_some_and(constant) {
        return outcome(
            TestVerdict::Refused(invalid("Enter an Excel constant as the expected result")),
            None,
            None,
        );
    }
    let mut expected_trial = case.trial.clone();
    expected_trial.formula = expected;
    expected_trial.inputs.clear();
    expected_trial.caller = None;
    let expected = match evaluate_value(session, &expected_trial, true) {
        Ok(value) => value.value,
        Err(refusal) => return outcome(TestVerdict::Refused(refusal), None, None),
    };
    let actual = match evaluate_value(session, &case.trial, false) {
        Ok(value) => value,
        Err(refusal) => return outcome(TestVerdict::Refused(refusal), None, None),
    };
    let formatted = if actual.value.as_array().is_none() {
        match actual.cell(&actual.value) {
            Ok(cell) => Some(cell.formatted),
            Err(refusal) => return outcome(TestVerdict::Refused(refusal), None, None),
        }
    } else {
        None
    };
    let matches = same_value(&actual.value, &expected)
        && case
            .formatted
            .as_ref()
            .is_none_or(|text| formatted.as_ref() == Some(text));
    let literal = if let Some(array) = actual.value.as_array()
        && array.shape().cell_count().is_none_or(|count| count > 512)
    {
        Some(format!(
            "{} × {} array",
            array.shape().rows,
            array.shape().cols
        ))
    } else {
        formula_literal(&actual.value).ok()
    };
    outcome(
        if matches {
            TestVerdict::Match
        } else {
            TestVerdict::Mismatch
        },
        literal,
        formatted,
    )
}

fn same_value(actual: &CalcValue, expected: &CalcValue) -> bool {
    match (actual.core(), expected.core()) {
        (CoreValue::Number(left), CoreValue::Number(right)) => left.to_bits() == right.to_bits(),
        (CoreValue::Array(left), CoreValue::Array(right)) => {
            left.shape() == right.shape()
                && left
                    .iter_row_major()
                    .zip(right.iter_row_major())
                    .all(|(left, right)| same_value(left, right))
        }
        (left, right) => left == right,
    }
}
