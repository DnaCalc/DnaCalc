//! SUBTOTAL and AGGREGATE, after OxFunc's functions/subtotal_aggregate_family.rs (6fca3518).
//!
//! Both leave out of a reference the cells Excel leaves out: SUBTOTAL every cell holding a
//! SUBTOTAL or AGGREGATE formula and every row a filter hides, and with function numbers 101 to
//! 111 also the rows hidden by hand; AGGREGATE what its options say. Only the cell reader knows
//! which cells those are ([`crate::CellReader::subtotal_exclusions`]); when it cannot say, a call
//! that must leave such cells out is refused as not modelled, never computed over every cell.
//! AGGREGATE's options 4 and 6, which leave nothing out, need no such knowledge.
//!
//! The values a reference holds are read as the aggregate family reads held values (numbers
//! only; COUNTA counts every cell that is not blank); SUM's total is the same sequential sum.

use super::aggregate::{self, read_reference};
use super::kit::{fail, num};
use crate::call::{EvalServices, SubtotalExclusions};
use crate::kernel::{Exit, Site, Step};
use crate::ported::percentile_common::{percentile_exc_kernel, percentile_inc_kernel};
use crate::ported::variance_common::{VarianceDivisor, stdev_from_values, variance_from_values};
use crate::value::{CalcValue, CoreRef, WorksheetErrorCode};
use std::collections::BTreeSet;

use calc_catalogue::ported::subtotal::{CODE, ENTRIES as DECLARATIONS};

/// What a call leaves out of its references.
#[derive(Clone, Copy)]
struct Rule {
    nested: bool,
    hidden: bool,
    filtered: bool,
    errors: bool,
}

/// The values of the reference arguments `from..`, with the cells `rule` leaves out removed.
fn held_values(
    site: &Site<'_>,
    a: &[CalcValue],
    from: usize,
    sv: &mut EvalServices<'_>,
    rule: Rule,
) -> Step<Vec<CalcValue>> {
    let mut found = Vec::new();
    for (index, argument) in a.iter().enumerate().skip(from) {
        let CoreRef::Reference(reference) = argument.core_ref() else {
            // SUBTOTAL and AGGREGATE's reference form read references only; an array here is
            // not modelled.
            return Err(site.not_modelled(Some(index)));
        };
        let needs_knowledge = rule.nested || rule.hidden || rule.filtered;
        let mut excluded_cells = BTreeSet::new();
        let mut excluded_rows = BTreeSet::new();
        if needs_knowledge {
            let reader = aggregate::reader(site, sv, index)?;
            for &area in reference.areas() {
                let Some(SubtotalExclusions {
                    nested_subtotals,
                    hidden_rows,
                    filtered_rows,
                }) = reader.subtotal_exclusions(area)
                else {
                    return Err(site.not_modelled(Some(index)));
                };
                if rule.nested {
                    excluded_cells.extend(nested_subtotals.iter().map(|k| (k.row(), k.column())));
                }
                if rule.hidden {
                    excluded_rows.extend(hidden_rows);
                }
                if rule.filtered {
                    excluded_rows.extend(filtered_rows);
                }
            }
        }
        let reader = aggregate::reader(site, sv, index)?;
        read_reference(site, reader, reference, index, &mut |_, key, value| {
            if excluded_rows.contains(&key.row())
                || excluded_cells.contains(&(key.row(), key.column()))
            {
                return Ok(());
            }
            if rule.errors && matches!(value.core_ref(), CoreRef::Error(_)) {
                return Ok(());
            }
            found.push(value.clone());
            Ok(())
        })?;
    }
    Ok(found)
}

/// The numbers among held values; the first error is the result.
fn numbers_of(values: &[CalcValue]) -> Step<Vec<f64>> {
    let mut found = Vec::new();
    for value in values {
        match value.core_ref() {
            CoreRef::Number(n) => found.push(n),
            CoreRef::Error(code) => return fail(code),
            _ => {}
        }
    }
    Ok(found)
}

/// Function `number` (1 to 13, SUBTOTAL's numbering for 1 to 11) over the held values.
fn compute(site: &Site<'_>, number: i64, values: &[CalcValue]) -> Step<CalcValue> {
    if number == 3 {
        let counted = values
            .iter()
            .filter(|v| !matches!(v.core_ref(), CoreRef::Empty | CoreRef::Missing))
            .count();
        return site.number(counted as f64);
    }
    if number == 2 {
        let counted = values
            .iter()
            .filter(|v| matches!(v.core_ref(), CoreRef::Number(_)))
            .count();
        return site.number(counted as f64);
    }
    let numbers = numbers_of(values)?;
    let result = match number {
        1 => {
            let mut total = 0.0;
            for n in &numbers {
                total = site.add(total, *n)?;
            }
            site.divide(total, numbers.len() as f64)?
        }
        4 => numbers
            .iter()
            .copied()
            .fold(None, |m: Option<f64>, n| Some(m.map_or(n, |m| m.max(n))))
            .unwrap_or(0.0),
        5 => numbers
            .iter()
            .copied()
            .fold(None, |m: Option<f64>, n| Some(m.map_or(n, |m| m.min(n))))
            .unwrap_or(0.0),
        6 => {
            if numbers.is_empty() {
                0.0
            } else {
                let mut product = 1.0;
                for n in &numbers {
                    product = site.multiply(product, *n)?;
                }
                product
            }
        }
        7 => stdev_from_values(&numbers, VarianceDivisor::Sample).map_err(Exit::Error)?,
        8 => stdev_from_values(&numbers, VarianceDivisor::Population).map_err(Exit::Error)?,
        9 => {
            let mut total = 0.0;
            for n in &numbers {
                total = site.add(total, *n)?;
            }
            total
        }
        10 => variance_from_values(&numbers, VarianceDivisor::Sample).map_err(Exit::Error)?,
        11 => variance_from_values(&numbers, VarianceDivisor::Population).map_err(Exit::Error)?,
        12 => {
            let mut sorted = numbers.clone();
            sorted.sort_by(f64::total_cmp);
            if sorted.is_empty() {
                return fail(WorksheetErrorCode::Num);
            }
            let mid = sorted.len() / 2;
            if sorted.len() % 2 == 1 {
                sorted[mid]
            } else {
                sorted[mid - 1] + (sorted[mid] - sorted[mid - 1]) / 2.0
            }
        }
        13 => {
            // MODE.SNGL: the first most frequent number, in reading order.
            let mut best: Option<(usize, f64)> = None;
            for (i, n) in numbers.iter().enumerate() {
                let count = numbers.iter().filter(|m| *m == n).count();
                if count >= 2
                    && best.is_none_or(|(c, _)| count > c)
                    && numbers[..i].iter().all(|m| m != n)
                {
                    best = Some((count, *n));
                }
            }
            match best {
                Some((_, n)) => n,
                None => return fail(WorksheetErrorCode::NA),
            }
        }
        _ => return fail(WorksheetErrorCode::Value),
    };
    site.number(result)
}

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
ported_shared_family! {
    code: CODE;
    declarations: DECLARATIONS;
    // Formula entry (dc-mx3x, Excel 16.0.20527, 2026-11-07, Formula2 and Formula alike): each
    // ref takes only an operand that may give a reference: `=SUBTOTAL(9,2)`, `=SUBTOTAL(9,{1,2})`,
    // `=SUBTOTAL(9,1+1)` and `=SUBTOTAL(9,B1,2)` are refused, `=SUBTOTAL(9,B1:B3)` and
    // `=SUBTOTAL(9,INDEX(B1:B3,0))` enter.
    1 subtotal |s, a, sv| {
        let number = num(s, a, 0)?.trunc() as i64;
        let (function, hidden) = match number {
            1..=11 => (number, false),
            101..=111 => (number - 100, true),
            _ => return fail(WorksheetErrorCode::Value),
        };
        let rule = Rule { nested: true, hidden, filtered: true, errors: false };
        let values = held_values(s, a, 1, sv, rule)?;
        compute(s, function, &values)
    }
    // AGGREGATE's third and fourth arguments take any operand, being the array form's array and
    // k as well as the reference form's ref1 and ref2, and every later one only an operand that
    // may give a reference (dc-mx3x, both forms): `=AGGREGATE(9,0,2)`,
    // `=AGGREGATE(9,0,{1,2},2)` and `=AGGREGATE(9,0,B1,2)` enter; `=AGGREGATE(9,0,B1,B1,2)`,
    // `=AGGREGATE(9,0,B1,B1,{1,2})` and `=AGGREGATE(9,0,B1,B1,B1,2)` are refused.
    2 aggregate_fn |s, a, sv| {
        let function = num(s, a, 0)?.trunc() as i64;
        let options = if a[1].is_missing() { 0 } else { num(s, a, 1)?.trunc() as i64 };
        if !(0..=7).contains(&options) || !(1..=19).contains(&function) {
            return fail(WorksheetErrorCode::Value);
        }
        let rule = Rule {
            nested: options <= 3,
            hidden: matches!(options, 1 | 3 | 5 | 7),
            filtered: matches!(options, 1 | 3 | 5 | 7),
            errors: matches!(options, 2 | 3 | 6 | 7),
        };
        if function <= 13 {
            // The reference form reads references only: a value there (a number, an array) is
            // #VALUE! (Excel 16.0.20430, 2026-10-25, dc-6au: `=AGGREGATE(1,1,{1,2})`,
            // `=AGGREGATE(9,0,1)` and `=AGGREGATE(1,1,A1:A3*1)` are #VALUE!). An error value
            // there is not measured and stays refused.
            if a[2..].iter().any(|argument| {
                !matches!(argument.core_ref(), CoreRef::Reference(_) | CoreRef::Error(_))
                    && !argument.is_missing()
            }) {
                return fail(WorksheetErrorCode::Value);
            }
            let values = held_values(s, a, 2, sv, rule)?;
            return compute(s, function, &values);
        }
        // The array form: AGGREGATE(function_num, options, array, k).
        if a.len() != 4 {
            return fail(WorksheetErrorCode::Value);
        }
        let k = num(s, a, 3)?;
        let values = match a[2].core_ref() {
            CoreRef::Reference(_) => held_values(s, &a[..3], 2, sv, rule)?,
            CoreRef::Array(array) => array
                .iter_row_major()
                .filter(|v| !(rule.errors && matches!(v.core_ref(), CoreRef::Error(_))))
                .cloned()
                .collect(),
            _ => vec![a[2].clone()],
        };
        let mut numbers = numbers_of(&values)?;
        let result = match function {
            14 | 15 => {
                let k = k.trunc();
                if k < 1.0 || k > numbers.len() as f64 {
                    return fail(WorksheetErrorCode::Num);
                }
                numbers.sort_by(f64::total_cmp);
                if function == 14 {
                    numbers.reverse();
                }
                Ok(numbers[k as usize - 1])
            }
            16 => percentile_inc_kernel(&mut numbers, k),
            17 => {
                let q = k.trunc();
                if !(0.0..=4.0).contains(&q) { Err(WorksheetErrorCode::Num) }
                else { percentile_inc_kernel(&mut numbers, q / 4.0) }
            }
            18 => percentile_exc_kernel(&mut numbers, k),
            _ => {
                let q = k.trunc();
                if !(1.0..=3.0).contains(&q) { Err(WorksheetErrorCode::Num) }
                else { percentile_exc_kernel(&mut numbers, q / 4.0) }
            }
        };
        super::kit::out(s, result)
    }
}
