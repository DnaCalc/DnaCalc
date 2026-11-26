//! The database functions family fragment ported from OxFunc (functions/database_family.rs at
//! 6fca3518): DAVERAGE, DCOUNT, DCOUNTA, DGET, DMAX, DMIN, DPRODUCT, DSTDEV, DSTDEVP, DSUM, DVAR
//! and DVARP over a database range and a criteria range.
//!
//! The database's first row holds the field labels; the criteria range's first row names fields
//! by label (any case) and each later row is one set of conditions, a record matching when it
//! meets every condition of any row. A criteria cell is read as a criterion of the *IFS functions
//! ([`super::conditional`]), except that text with no operator matches the text that begins with
//! it; a blank criteria cell sets no condition, and a criteria reference needs a row below its
//! labels. The field is a label or a 1-based column number; DCOUNT and DCOUNTA may leave it out to
//! count matching records. A label the database lacks in the field is #VALUE!. A criteria
//! header that names no field (or is blank) makes its cells computed criteria: a nonzero number or
//! TRUE holds for every record, zero, FALSE, text and errors for none (live Excel, dc-j9k.271-.273).
//!
//! In the field's column, the counts, sums and extremes read numbers and skip text, logicals and
//! blank cells (DCOUNTA counts every cell that is not blank); the first error in a matching
//! record is the result. DMAX and DMIN of no number are 0, DAVERAGE #DIV/0!; DGET gives the one
//! matching record's cell, #VALUE! for none and #NUM! for several. A record whose verdict is not
//! modelled (see the *IFS criteria) refuses the call. The sums are a sequential left fold in
//! record order, each addition an arithmetic site.

use super::conditional::{Criterion, Match, database_criterion};
use super::aggregate;
use super::kit::{GRID_LIMIT, Grid, fail, grid};
use crate::call::EvalServices;
use crate::kernel::{Exit, Site, Step};
use crate::ported::variance_common::{VarianceDivisor, stdev_from_values, variance_from_values};
use crate::value::{CalcValue, CoreRef, WorksheetErrorCode};

type W = WorksheetErrorCode;

use calc_catalogue::ported::database::{CODE, ENTRIES as DECLARATIONS};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Average,
    Count,
    CountA,
    Get,
    Max,
    Min,
    Product,
    Stdev,
    StdevP,
    Sum,
    Var,
    VarP,
}

/// A field label's text: text as it is; a number, a logical or an error as their text.
fn label(site: &Site<'_>, index: usize, value: &CalcValue) -> Step<Option<Vec<u16>>> {
    match value.core_ref() {
        CoreRef::Empty | CoreRef::Missing => Ok(None),
        // Excel 16.0.20625 (dc-j9k.528): an error in the label row is a label with the error's text.
        CoreRef::Error(code) => Ok(Some(code.excel_spelling().encode_utf16().collect())),
        _ => Ok(Some(
            site.argument(index, crate::coerce::text(value))?
                .into_owned(),
        )),
    }
}

fn same_label(a: &[u16], b: &[u16]) -> bool {
    use crate::ported::text_kernels::fold_unit;
    a.len() == b.len() && a.iter().zip(b).all(|(&x, &y)| fold_unit(x) == fold_unit(y))
}

/// The database's column for a field argument.
fn field(
    site: &Site<'_>,
    value: &CalcValue,
    headers: &[Option<Vec<u16>>],
    optional: bool,
) -> Step<Option<usize>> {
    match value.core_ref() {
        CoreRef::Missing | CoreRef::Empty if optional => Ok(None),
        CoreRef::Text(text) => {
            let units = text.utf16_code_units();
            headers
                .iter()
                .position(|h| h.as_deref().is_some_and(|h| same_label(h, units)))
                .map(Some)
                .ok_or(Exit::Error(W::Value))
        }
        CoreRef::Error(code) => fail(code),
        _ => {
            let n = site.number_argument(1, value)?;
            let column = n.trunc();
            if column < 1.0 || column > headers.len() as f64 {
                return fail(W::Value);
            }
            Ok(Some(column as usize - 1))
        }
    }
}

/// One condition of a criteria row: a criterion on a database column, or a computed criterion's
/// fixed truth.
enum Condition {
    Field(usize, Criterion),
    Fixed(bool),
}

/// The records that meet the criteria, by row index below the label row.
fn matching(
    site: &Site<'_>,
    database: &Grid,
    headers: &[Option<Vec<u16>>],
    criteria: &Grid,
    criteria_area: Option<crate::value::GridArea>,
    sv: &mut EvalServices<'_>,
) -> Step<Vec<usize>> {
    let records = database.rows.saturating_sub(1);
    if criteria.rows < 2 {
        return Ok((0..records).collect());
    }
    // Each criteria row: the conditions it sets.
    let mut rows = Vec::new();
    for r in 1..criteria.rows {
        let mut conditions = Vec::new();
        for c in 0..criteria.cols {
            let cell = &criteria.cells[r * criteria.cols + c];
            // A header that names no field (or is blank or an error) makes the cell a computed
            // criterion: a blank sets no condition, a nonzero number or TRUE holds for every record,
            // and zero, FALSE, text and an error hold for none (live Excel, dc-j9k.271-.273).
            let column = match &criteria.cells[c].core_ref() {
                CoreRef::Text(_) | CoreRef::Number(_) | CoreRef::Logical(_) => {
                    let name = label(site, 2, &criteria.cells[c])?.expect("a header with a value has a label");
                    headers
                        .iter()
                        .position(|h| h.as_deref().is_some_and(|h| same_label(h, &name)))
                }
                _ => None,
            };
            let Some(column) = column else {
                // A formula cell under a non-field header is a per-record computed criterion in
                // Excel; refuse typed rather than collapsing it to one fixed truth (dc-y7tqj).
                if let Some(area) = criteria_area
                    && let Ok(key) = crate::value::CellKey::new(
                        area.sheet(),
                        area.first().row() + r as u32,
                        area.first().column() + c as u32,
                    )
                    && sv.host().and_then(|host| host.is_formula(key)) == Some(true)
                {
                    return Err(site.not_modelled(Some(2)));
                }
                match cell.core_ref() {
                    CoreRef::Empty | CoreRef::Missing => {}
                    CoreRef::Number(n) => conditions.push(Condition::Fixed(n != 0.0)),
                    CoreRef::Logical(on) => conditions.push(Condition::Fixed(on)),
                    _ => conditions.push(Condition::Fixed(false)),
                }
                continue;
            };
            let Some(criterion) = database_criterion(site, 2, cell)? else {
                continue;
            };
            conditions.push(Condition::Field(column, criterion));
        }
        rows.push(conditions);
    }
    let mut found = Vec::new();
    for record in 0..records {
        let mut any = false;
        let mut unknown = false;
        for conditions in &rows {
            let mut verdict = Match::Yes;
            for condition in conditions {
                let (column, criterion) = match condition {
                    Condition::Fixed(true) => continue,
                    Condition::Fixed(false) => {
                        verdict = Match::No;
                        break;
                    }
                    Condition::Field(column, criterion) => (column, criterion),
                };
                let cell = &database.cells[(record + 1) * database.cols + column];
                match criterion_verdict(site, criterion, cell)? {
                    Match::No => {
                        verdict = Match::No;
                        break;
                    }
                    Match::Unknown => verdict = Match::Unknown,
                    Match::Yes => {}
                }
            }
            match verdict {
                Match::Yes => {
                    any = true;
                    break;
                }
                Match::Unknown => unknown = true,
                Match::No => {}
            }
        }
        if any {
            found.push(record);
        } else if unknown {
            return Err(site.not_modelled(Some(2)));
        }
    }
    Ok(found)
}

fn criterion_verdict(site: &Site<'_>, criterion: &Criterion, cell: &CalcValue) -> Step<Match> {
    criterion.verdict(site, 0, cell)
}

/// The database argument as a grid. A database larger than the grid limit (a whole-column
/// reference, `$E:$J`) is read as the rows down to its last held cell, when no all-blank record
/// below can change the result: every kind with a field reads only numbers or non-blank cells of
/// that column (dc-r526). DGET, and a count of records with no field, keep the refusal, since a
/// blank record could match their criteria.
fn database_grid(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
    kind: Kind,
) -> Step<Grid> {
    let no_field = matches!(a[1].core_ref(), CoreRef::Missing | CoreRef::Empty);
    if kind != Kind::Get
        && !no_field
        && let CoreRef::Reference(reference) = a[0].core_ref()
        && let [area] = reference.areas()
        && area.cell_count() > GRID_LIMIT
    {
        let first = area.first();
        let cols = area.column_count() as usize;
        let mut held: Vec<(usize, usize, CalcValue)> = Vec::new();
        let reader = aggregate::reader(site, sv, 0)?;
        aggregate::read_reference(site, reader, reference, 0, &mut |_, key, value| {
            held.push((
                (key.row() - first.row()) as usize,
                (key.column() - first.column()) as usize,
                value.clone(),
            ));
            Ok(())
        })?;
        let rows = held.iter().map(|(row, _, _)| row + 1).max().unwrap_or(1);
        if (rows * cols) as u64 > GRID_LIMIT {
            return Err(site.not_modelled(Some(0)));
        }
        let mut cells = vec![CalcValue::empty(); rows * cols];
        for (row, column, value) in held {
            cells[row * cols + column] = value;
        }
        return Ok(Grid {
            rows,
            cols,
            cells,
            held: true,
        });
    }
    grid(site, a, 0, sv)
}

fn database(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
    kind: Kind,
) -> Step<CalcValue> {
    if !matches!(a[0].core_ref(), CoreRef::Reference(_) | CoreRef::Array(_)) {
        return match a[0].core_ref() {
            CoreRef::Error(code) => fail(code),
            _ => fail(W::Value),
        };
    }
    // Excel requires database and criteria references to include a row below their labels.
    // Inspect the references before their contents or the field: whole-column databases can
    // be compressed to held cells by `database_grid`, but still have many blank record rows.
    for &index in &[0, 2] {
        if let CoreRef::Reference(reference) = a[index].core_ref()
            && let [area] = reference.areas()
            && area.row_count() == 1
        {
            return fail(W::Value);
        }
    }
    let database = database_grid(site, a, sv, kind)?;
    if database.rows == 0 || database.cols == 0 {
        return fail(W::Value);
    }
    let mut headers = Vec::with_capacity(database.cols);
    for c in 0..database.cols {
        headers.push(label(site, 0, &database.cells[c])?);
    }
    let optional = matches!(kind, Kind::Count | Kind::CountA);
    let column = field(site, &a[1], &headers, optional)?;
    if !matches!(a[2].core_ref(), CoreRef::Reference(_)) {
        return match a[2].core_ref() {
            CoreRef::Error(code) => fail(code),
            _ => fail(W::Value),
        };
    }
    let criteria_area = match a[2].core_ref() {
        CoreRef::Reference(reference) => match reference.areas() {
            [area] => Some(*area),
            _ => None,
        },
        _ => None,
    };
    let criteria = grid(site, a, 2, sv)?;
    let records = matching(site, &database, &headers, &criteria, criteria_area, sv)?;
    let Some(column) = column else {
        return site.number(records.len() as f64);
    };
    let cell = |record: usize| &database.cells[(record + 1) * database.cols + column];
    if kind == Kind::Get {
        return match records.as_slice() {
            [] => fail(W::Value),
            [one] => {
                let value = cell(*one).clone();
                Ok(if matches!(value.core_ref(), CoreRef::Empty) {
                    CalcValue::number(0.0)
                } else {
                    value
                })
            }
            _ => fail(W::Num),
        };
    }
    if kind == Kind::CountA {
        let mut count = 0usize;
        for &record in &records {
            if !matches!(cell(record).core_ref(), CoreRef::Empty | CoreRef::Missing) {
                count += 1;
            }
        }
        return site.number(count as f64);
    }
    let mut numbers = Vec::new();
    for &record in &records {
        match cell(record).core_ref() {
            CoreRef::Number(n) => numbers.push(site.argument(0, crate::coerce::operand(n))?),
            CoreRef::Error(code) => return fail(code),
            _ => {}
        }
    }
    match kind {
        Kind::Count => site.number(numbers.len() as f64),
        Kind::Sum => {
            let mut total = 0.0;
            for n in numbers {
                total = site.add(total, n)?;
            }
            site.number(total)
        }
        Kind::Average => {
            if numbers.is_empty() {
                return fail(W::Div0);
            }
            let count = numbers.len() as f64;
            let mut total = 0.0;
            for n in numbers {
                total = site.add(total, n)?;
            }
            site.number(site.divide(total, count)?)
        }
        Kind::Max => site.number(numbers.into_iter().reduce(f64::max).unwrap_or(0.0)),
        Kind::Min => site.number(numbers.into_iter().reduce(f64::min).unwrap_or(0.0)),
        Kind::Product => {
            if numbers.is_empty() {
                return site.number(0.0);
            }
            let mut product = 1.0;
            for n in numbers {
                product = site.multiply(product, n)?;
            }
            site.number(product)
        }
        Kind::Stdev => published(site, stdev_from_values(&numbers, VarianceDivisor::Sample)),
        Kind::StdevP => published(
            site,
            stdev_from_values(&numbers, VarianceDivisor::Population),
        ),
        Kind::Var => published(
            site,
            variance_from_values(&numbers, VarianceDivisor::Sample),
        ),
        Kind::VarP => published(
            site,
            variance_from_values(&numbers, VarianceDivisor::Population),
        ),
        Kind::Get | Kind::CountA => unreachable!("handled above"),
    }
}

fn published(site: &Site<'_>, result: Result<f64, W>) -> Step<CalcValue> {
    site.binary64()?;
    match result {
        Ok(n) => site.number(n),
        Err(code) => fail(code),
    }
}

// Formula entry (dc-mx3x, Excel 16.0.20527, 2026-11-07, Formula2 and Formula alike): each
// function's database takes only an operand that may give a reference (`=DSUM(2,1,B1:B3)`,
// `=DSUM({1,2},1,B1:B3)` and `=DSUM(1+1,1,B1:B3)` are refused, `=DSUM(B1:B3,1,B1:B3)` and
// `=DSUM(INDEX(B1:B3,0),1,B1:B3)` enter, and likewise for the other eleven); the field and the
// criteria take any operand (`=DSUM(B1:B3,1,2)` enters).
// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
ported_shared_family! {
    code: CODE;
    declarations: DECLARATIONS;
    1 daverage |s, a, sv| { database(s, a, sv, Kind::Average) }
    2 dcount |s, a, sv| { database(s, a, sv, Kind::Count) }
    3 dcounta |s, a, sv| { database(s, a, sv, Kind::CountA) }
    4 dget |s, a, sv| { database(s, a, sv, Kind::Get) }
    5 dmax |s, a, sv| { database(s, a, sv, Kind::Max) }
    6 dmin |s, a, sv| { database(s, a, sv, Kind::Min) }
    7 dproduct |s, a, sv| { database(s, a, sv, Kind::Product) }
    8 dstdev |s, a, sv| { database(s, a, sv, Kind::Stdev) }
    9 dstdevp |s, a, sv| { database(s, a, sv, Kind::StdevP) }
    10 dsum |s, a, sv| { database(s, a, sv, Kind::Sum) }
    11 dvar |s, a, sv| { database(s, a, sv, Kind::Var) }
    12 dvarp |s, a, sv| { database(s, a, sv, Kind::VarP) }
}
