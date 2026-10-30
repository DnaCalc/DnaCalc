//! GROUPBY and PIVOTBY (dc-h9u): the groups of a table's rows by their row fields (and, for
//! PIVOTBY, by their column fields), each group's values aggregated by the function given, a
//! function value (`SUM`, as a file stores `_xleta.SUM`), a LAMBDA, or a row of them (`HSTACK`).
//! Written from Excel's behaviour, measured in Excel 16.0.20527 through COM on 2026-10-30 (60
//! calls over a six-row table; the readings are restated where they decide a rule):
//!
//! - Rows group by equal row fields, text without regard to case (`East` and `east` are one
//!   group, shown as the first spelling met). Groups sort ascending by each row field: numbers,
//!   then text without regard to case, then logicals, errors, and blank fields last.
//! - `field_headers`: omitted, the first row is a header row when the values' first cell is text
//!   and the second a number, and is not shown; 0 no header row; 1 a header row, not shown; 2 no
//!   header row, with `Row Field n` and `Value n` shown; 3 a header row, shown. Any other is
//!   #VALUE!.
//! - `total_depth`: 1 (default) a `Total` row at the end, 0 none, 2 a subtotal row after each group
//!   of the first row field and `Grand Total` at the end (with two row fields or more, else
//!   #VALUE!); a negative depth puts the totals before what they total. `field_relationship` 1
//!   (table) takes no subtotals (depth 2 is #VALUE!).
//! - `sort_order`: each entry a column of the result, negative for descending; with the default
//!   hierarchy each row field keeps its place and takes its direction from the entries (`{2,-1}`
//!   gives the first field descending, the second ascending); an entry past the row fields sorts
//!   the groups by that aggregate. 0, or a column past the result, is #VALUE!.
//! - `filter_array`: a column of the rows' flags; none true is #VALUE!.
//! - Each aggregate is the function called with the group's values as a column; a function that
//!   takes two arguments (PERCENTOF) also receives every filtered row's values (PIVOTBY: those its
//!   `relative_to` names). A result that is an array is #VALUE! in its cell. With several
//!   functions, a header row names them, and each pairs with a value column (or every function
//!   with the one value column).
//! - Row fields and values of different heights, and a function that is not one, are #VALUE!.
//! - PIVOTBY lays the column fields' groups out across, sorted likewise, with a `Total` column; a
//!   cell no row reaches is blank. With several value columns each column group takes one column
//!   per value, and the `Total` columns are blank. `relative_to` names what a two-argument
//!   function's second argument holds: 0 the column's rows (the default), 1 and 3 the row's, 2
//!   every row, 4 the column's (3 and 4 measured with one field each). With several functions the
//!   `Total` columns are not blank.
//! - PIVOTBY's depths (measured 2026-10-30, 15 calls with two and three row fields and two column
//!   fields): depth n (up to the number of fields; a larger one is #VALUE!, and 2 with one field
//!   too) adds a subtotal after each group of the first k fields for each k below n, labelled by
//!   those fields with the rest blank, and labels the grand total `Grand Total` (`Total` at depth
//!   1); a negative depth puts each total before what it totals. A column subtotal's header shows
//!   its fields at their levels and is blank below. Column subtotals with several value columns
//!   are not modelled.

use super::comparison::{self, Want};
use super::kit::{fail, grid};
use crate::call::{CallAbandoned, EvalServices};
use crate::kernel::{Exit, Site, Step};
use crate::value::{ArrayShape, CalcArray, CalcValue, CoreRef, ExcelText, WorksheetErrorCode};
use std::cmp::Ordering;

type W = WorksheetErrorCode;

const CODE: u16 = 321;

/// A table argument: its rows of cells.
struct Table {
    rows: Vec<Vec<CalcValue>>,
    cols: usize,
}

impl Table {
    fn height(&self) -> usize {
        self.rows.len()
    }
}

fn table(site: &Site<'_>, a: &[CalcValue], index: usize, sv: &mut EvalServices<'_>) -> Step<Table> {
    let (rows, cols, cells) = match a[index].core_ref() {
        CoreRef::Reference(_) => {
            let g = grid(site, a, index, sv)?;
            (g.rows, g.cols, g.cells)
        }
        CoreRef::Array(array) => {
            let shape = array.shape();
            (shape.rows, shape.cols, array.iter_row_major().cloned().collect())
        }
        CoreRef::Error(code) => return fail(code),
        _ => (1, 1, vec![a[index].clone()]),
    };
    if rows == 0 || cols == 0 {
        return fail(W::Value);
    }
    Ok(Table {
        rows: cells.chunks(cols).map(<[CalcValue]>::to_vec).collect(),
        cols,
    })
}

/// The callables of the function argument: one function value or LAMBDA, or a row of them.
fn functions(a: &[CalcValue], index: usize) -> Step<Vec<CalcValue>> {
    let value = &a[index];
    if value.callable_value().is_some() {
        return Ok(vec![value.clone()]);
    }
    if let CoreRef::Array(array) = value.core_ref() {
        let all: Vec<CalcValue> = array.iter_row_major().cloned().collect();
        if !all.is_empty() && all.iter().all(|f| f.callable_value().is_some()) {
            return Ok(all);
        }
    }
    match value.core_ref() {
        CoreRef::Error(code) => fail(code),
        _ => fail(W::Value),
    }
}

/// An optional whole-number argument: `None` omitted, #VALUE! when not a whole number.
fn whole(site: &Site<'_>, a: &[CalcValue], index: usize) -> Step<Option<i64>> {
    match a.get(index) {
        None => Ok(None),
        Some(value) if value.is_missing() || matches!(value.core_ref(), CoreRef::Empty) => Ok(None),
        Some(value) => {
            let n = site.number_argument(index, value)?;
            if n.fract() != 0.0 || !n.is_finite() {
                return fail(W::Value);
            }
            Ok(Some(n as i64))
        }
    }
}

/// The sort entries of argument `index`: whole, non-zero numbers.
fn sort_entries(site: &Site<'_>, a: &[CalcValue], index: usize) -> Step<Vec<i64>> {
    let Some(value) = a.get(index) else {
        return Ok(Vec::new());
    };
    if value.is_missing() {
        return Ok(Vec::new());
    }
    let cells: Vec<CalcValue> = match value.core_ref() {
        CoreRef::Array(array) => array.iter_row_major().cloned().collect(),
        _ => vec![value.clone()],
    };
    let mut out = Vec::new();
    for cell in cells {
        let n = match cell.core_ref() {
            CoreRef::Number(n) => n,
            CoreRef::Error(code) => return fail(code),
            _ => site.number_argument(index, &cell)?,
        };
        if n == 0.0 || n.fract() != 0.0 || !n.is_finite() {
            return fail(W::Value);
        }
        out.push(n as i64);
    }
    Ok(out)
}

/// The rows the filter keeps, of `count` data rows (`all` rows with the header row).
fn filter(
    site: &Site<'_>,
    a: &[CalcValue],
    index: usize,
    count: usize,
    all: usize,
    sv: &mut EvalServices<'_>,
) -> Step<Vec<bool>> {
    let Some(value) = a.get(index) else {
        return Ok(vec![true; count]);
    };
    if value.is_missing() {
        return Ok(vec![true; count]);
    }
    let t = table(site, a, index, sv)?;
    let cells: Vec<CalcValue> = if t.cols == 1 {
        t.rows.into_iter().map(|mut row| row.remove(0)).collect()
    } else if t.height() == 1 {
        t.rows.into_iter().next().unwrap_or_default()
    } else {
        return fail(W::Value);
    };
    let skip = if cells.len() == count {
        0
    } else if cells.len() == all {
        all - count
    } else {
        return fail(W::Value);
    };
    let mut keep = Vec::with_capacity(count);
    for cell in &cells[skip..] {
        keep.push(match cell.core_ref() {
            CoreRef::Logical(b) => b,
            CoreRef::Number(n) => n != 0.0,
            CoreRef::Empty | CoreRef::Missing => false,
            CoreRef::Error(code) => return fail(code),
            _ => return fail(W::Value),
        });
    }
    if !keep.iter().any(|k| *k) {
        return fail(W::Value);
    }
    Ok(keep)
}

/// The rank of a field's kind in the group order: numbers, text, logicals, errors, blanks.
fn kind(value: &CalcValue) -> u8 {
    match value.core_ref() {
        CoreRef::Number(_) => 0,
        CoreRef::Text(_) => 1,
        CoreRef::Logical(_) => 2,
        CoreRef::Error(_) => 3,
        _ => 4,
    }
}

fn order(site: &Site<'_>, x: &CalcValue, y: &CalcValue) -> Step<Ordering> {
    match (x.core_ref(), y.core_ref()) {
        (CoreRef::Number(a), CoreRef::Number(b)) => comparison::numbers(site, a, b),
        (CoreRef::Text(a), CoreRef::Text(b)) => {
            comparison::texts(a.utf16_code_units(), b.utf16_code_units(), Want::Order)
                .map_err(|_| site.not_modelled(None))
        }
        (CoreRef::Logical(a), CoreRef::Logical(b)) => Ok(a.cmp(&b)),
        (CoreRef::Error(a), CoreRef::Error(b)) => Ok((a as u8).cmp(&(b as u8))),
        _ => Ok(kind(x).cmp(&kind(y))),
    }
}

fn same(site: &Site<'_>, x: &CalcValue, y: &CalcValue) -> Step<bool> {
    match (x.core_ref(), y.core_ref()) {
        (CoreRef::Text(a), CoreRef::Text(b)) => {
            comparison::texts(a.utf16_code_units(), b.utf16_code_units(), Want::Equality)
                .map(Ordering::is_eq)
                .map_err(|_| site.not_modelled(None))
        }
        _ => order(site, x, y).map(Ordering::is_eq),
    }
}

/// `keys` with each field shown as the first spelling met of the values equal to it in its
/// column (`east` shown as `East` where `East` comes first), as Excel shows a group's fields.
fn canonical(site: &Site<'_>, keys: &[Vec<CalcValue>], rows: &[usize]) -> Step<Vec<Vec<CalcValue>>> {
    let mut out = keys.to_vec();
    let width = keys.first().map_or(0, Vec::len);
    for column in 0..width {
        let mut seen: Vec<CalcValue> = Vec::new();
        for &row in rows {
            let value = &keys[row][column];
            let mut shown = None;
            for first in &seen {
                if same(site, first, value)? {
                    shown = Some(first.clone());
                    break;
                }
            }
            match shown {
                Some(first) => out[row][column] = first,
                None => seen.push(value.clone()),
            }
        }
    }
    Ok(out)
}

/// The groups of `keys` (one key row per data row, among `rows`): each the first key met and the
/// rows that share it, in the order first met.
fn group(site: &Site<'_>, keys: &[Vec<CalcValue>], rows: &[usize]) -> Step<Vec<(Vec<CalcValue>, Vec<usize>)>> {
    let mut groups: Vec<(Vec<CalcValue>, Vec<usize>)> = Vec::new();
    for &row in rows {
        let key = &keys[row];
        let mut found = None;
        for (at, (first, _)) in groups.iter().enumerate() {
            let mut equal = true;
            for (x, y) in key.iter().zip(first) {
                if !same(site, x, y)? {
                    equal = false;
                    break;
                }
            }
            if equal {
                found = Some(at);
                break;
            }
        }
        match found {
            Some(at) => groups[at].1.push(row),
            None => groups.push((key.clone(), vec![row])),
        }
    }
    Ok(groups)
}

/// Calls `f` through the services' invoker.
fn call(site: &Site<'_>, sv: &mut EvalServices<'_>, f: &CalcValue, args: &[CalcValue]) -> Step<CalcValue> {
    let Some(invoker) = sv.callables() else {
        return Err(site.not_modelled(None));
    };
    invoker.call(f, args).map_err(|CallAbandoned| site.not_modelled(None))
}

fn column(values: Vec<CalcValue>) -> CalcValue {
    let rows = values.len().max(1);
    let cells = if values.is_empty() { vec![CalcValue::empty()] } else { values };
    CalcValue::array(
        CalcArray::new(ArrayShape { rows, cols: 1 }, cells).expect("a column of its own length"),
    )
}

/// One aggregate: `f` called with the values of `column_index` of `rows`, and, when `f` takes two
/// arguments, those of `relative`; an array result is #VALUE!.
fn aggregate(
    site: &Site<'_>,
    sv: &mut EvalServices<'_>,
    f: &CalcValue,
    values: &Table,
    column_index: usize,
    rows: &[usize],
    relative: &[usize],
) -> Step<CalcValue> {
    let pick = |rows: &[usize]| -> Vec<CalcValue> {
        rows.iter()
            .map(|&r| match values.rows[r][column_index].core_ref() {
                CoreRef::Missing => CalcValue::empty(),
                _ => values.rows[r][column_index].clone(),
            })
            .collect()
    };
    let two = f.callable_value().is_some_and(|c| c.arity().min >= 2);
    let mut args = vec![column(pick(rows))];
    if two {
        args.push(column(pick(relative)));
    }
    let result = call(site, sv, f, &args)?;
    if result.callable_value().is_some() {
        return Ok(CalcValue::error(W::Calc));
    }
    Ok(match result.core_ref() {
        CoreRef::Array(_) | CoreRef::Reference(_) => CalcValue::error(W::Value),
        CoreRef::Missing => CalcValue::empty(),
        _ => result,
    })
}

fn text(s: &str) -> CalcValue {
    CalcValue::text(ExcelText::from_interop_assignment(s))
}

/// What the header argument says: whether the tables hold a header row, and whether the result
/// shows headers.
fn headers(site: &Site<'_>, a: &[CalcValue], index: usize, values: &Table) -> Step<(bool, bool)> {
    match whole(site, a, index)? {
        None => {
            let detected = values.height() >= 2
                && matches!(values.rows[0][0].core_ref(), CoreRef::Text(_))
                && matches!(values.rows[1][0].core_ref(), CoreRef::Number(_));
            Ok((detected, false))
        }
        Some(0) => Ok((false, false)),
        Some(1) => Ok((true, false)),
        Some(2) => Ok((false, true)),
        Some(3) => Ok((true, true)),
        Some(_) => fail(W::Value),
    }
}

/// The (value column, function) pairs the aggregates take.
fn pairs(functions: &[CalcValue], value_cols: usize) -> Step<Vec<(usize, usize)>> {
    if functions.len() == 1 {
        Ok((0..value_cols).map(|v| (v, 0)).collect())
    } else if value_cols == 1 {
        Ok((0..functions.len()).map(|f| (0, f)).collect())
    } else if value_cols == functions.len() {
        Ok((0..value_cols).map(|i| (i, i)).collect())
    } else {
        fail(W::Value)
    }
}

/// A function's name as a header shows it.
fn function_name(f: &CalcValue) -> CalcValue {
    text(f.callable_value().map(|c| c.summary()).unwrap_or(""))
}

/// The groups sorted: each row field ascending unless `entries` turn it, with the hierarchy kept
/// (`tabular` false) or in the entries' order (`tabular` true); an entry past the fields sorts by
/// that aggregate.
fn sort_groups(
    site: &Site<'_>,
    groups: &mut [(Vec<CalcValue>, Vec<usize>, Vec<CalcValue>)],
    fields: usize,
    entries: &[i64],
    tabular: bool,
) -> Step<()> {
    // (column, descending), in priority order.
    let mut keys: Vec<(usize, bool)> = Vec::new();
    let direction = |column: usize| {
        entries
            .iter()
            .find(|e| e.unsigned_abs() as usize == column + 1)
            .is_some_and(|e| *e < 0)
    };
    let value_sort = entries.iter().find(|e| e.unsigned_abs() as usize > fields);
    if tabular {
        for e in entries {
            keys.push(((e.unsigned_abs() - 1) as usize, *e < 0));
        }
        for c in 0..fields {
            if !keys.iter().any(|(k, _)| *k == c) {
                keys.push((c, false));
            }
        }
    } else {
        for c in 0..fields {
            if c + 1 == fields
                && let Some(e) = value_sort
            {
                keys.push(((e.unsigned_abs() - 1) as usize, *e < 0));
            }
            keys.push((c, direction(c)));
        }
    }
    let mut refused: Option<Exit> = None;
    groups.sort_by(|x, y| {
        if refused.is_some() {
            return Ordering::Equal;
        }
        for &(column, descending) in &keys {
            let (u, v) = if column < fields {
                (&x.0[column], &y.0[column])
            } else {
                (&x.2[column - fields], &y.2[column - fields])
            };
            match order(site, u, v) {
                Ok(Ordering::Equal) => continue,
                Ok(o) => return if descending { o.reverse() } else { o },
                Err(exit) => {
                    refused = Some(exit);
                    return Ordering::Equal;
                }
            }
        }
        Ordering::Equal
    });
    match refused {
        Some(exit) => Err(exit),
        None => Ok(()),
    }
}

fn result(rows: Vec<Vec<CalcValue>>) -> Step<CalcValue> {
    if rows.len() == 1 && rows[0].len() == 1 {
        return Ok(rows.into_iter().next().unwrap().remove(0));
    }
    Ok(CalcValue::array(
        CalcArray::from_rows(rows).ok_or(Exit::Error(W::Value))?,
    ))
}

fn groupby(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let fields = table(site, a, 0, sv)?;
    let values = table(site, a, 1, sv)?;
    let functions = functions(a, 2)?;
    if fields.height() != values.height() {
        return fail(W::Value);
    }
    let (has_header, show) = headers(site, a, 3, &values)?;
    let depth = whole(site, a, 4)?.unwrap_or(1);
    let entries = sort_entries(site, a, 5)?;
    let tabular = match whole(site, a, 7)? {
        None | Some(0) => false,
        Some(1) => true,
        Some(_) => return fail(W::Value),
    };
    let k = fields.cols;
    if !(-2..=2).contains(&depth) || (depth.abs() == 2 && (k < 2 || tabular)) {
        return fail(W::Value);
    }
    let pairs = pairs(&functions, values.cols)?;
    if entries
        .iter()
        .any(|e| e.unsigned_abs() as usize > k + pairs.len())
    {
        return fail(W::Value);
    }
    let start = usize::from(has_header);
    if values.height() <= start {
        return fail(W::Value);
    }
    let count = values.height() - start;
    let keep = filter(site, a, 6, count, values.height(), sv)?;
    let rows: Vec<usize> = (start..values.height()).filter(|r| keep[r - start]).collect();
    let mut groups = Vec::new();
    let keys = canonical(site, &fields.rows, &rows)?;
    for (key, members) in group(site, &keys, &rows)? {
        let mut aggregates = Vec::with_capacity(pairs.len());
        for &(v, f) in &pairs {
            aggregates.push(aggregate(site, sv, &functions[f], &values, v, &members, &rows)?);
        }
        groups.push((key, members, aggregates));
    }
    sort_groups(site, &mut groups, k, &entries, tabular)?;
    let width = k + pairs.len();
    let mut out: Vec<Vec<CalcValue>> = Vec::new();
    if show {
        let mut header: Vec<CalcValue> = (0..k)
            .map(|c| if has_header { fields.rows[0][c].clone() } else { text(&format!("Row Field {}", c + 1)) })
            .collect();
        for &(v, f) in &pairs {
            header.push(if functions.len() > 1 {
                function_name(&functions[f])
            } else if has_header {
                values.rows[0][v].clone()
            } else {
                text(&format!("Value {}", v + 1))
            });
        }
        out.push(header);
    } else if functions.len() > 1 {
        let mut header = vec![text(""); k];
        header.extend(pairs.iter().map(|&(_, f)| function_name(&functions[f])));
        out.push(header);
    }
    let totals = |site: &Site<'_>, sv: &mut EvalServices<'_>, label: &str, prefix: Option<&CalcValue>, members: &[usize]| -> Step<Vec<CalcValue>> {
        let mut row = vec![text(""); width];
        match prefix {
            Some(key) => row[0] = key.clone(),
            None => row[0] = text(label),
        }
        for (at, &(v, f)) in pairs.iter().enumerate() {
            row[k + at] = aggregate(site, sv, &functions[f], &values, v, members, &rows)?;
        }
        Ok(row)
    };
    let grand_label = if depth.abs() == 2 { "Grand Total" } else { "Total" };
    if depth < 0 {
        out.push(totals(site, sv, grand_label, None, &rows)?);
    }
    let mut at = 0;
    while at < groups.len() {
        let mut end = at + 1;
        if depth.abs() == 2 {
            while end < groups.len() && same(site, &groups[end].0[0], &groups[at].0[0])? {
                end += 1;
            }
        }
        let members: Vec<usize> = groups[at..end].iter().flat_map(|g| g.1.iter().copied()).collect();
        if depth == -2 {
            out.push(totals(site, sv, "", Some(&groups[at].0[0]), &members)?);
        }
        for g in &groups[at..end] {
            let mut row = g.0.clone();
            row.extend(g.2.iter().cloned());
            out.push(row);
        }
        if depth == 2 {
            out.push(totals(site, sv, "", Some(&groups[at].0[0]), &members)?);
        }
        at = end;
    }
    if depth > 0 {
        out.push(totals(site, sv, grand_label, None, &rows)?);
    }
    for row in &mut out {
        for cell in row.iter_mut() {
            if matches!(cell.core_ref(), CoreRef::Missing) {
                *cell = text("");
            }
        }
    }
    result(out)
}

/// One row or column of a PIVOTBY result along an axis: its labels (one per field), the rows it
/// aggregates, and whether it is the grand total.
struct Line {
    labels: Vec<CalcValue>,
    members: Vec<usize>,
    grand: bool,
}

/// The lines of an axis whose sorted `groups` have `fields` fields, at total depth `depth`: the
/// groups, a subtotal after (before, for a negative depth) each run of groups sharing their first
/// k fields for each k below the depth, and the grand total over `all`.
fn lines(
    site: &Site<'_>,
    groups: &[(Vec<CalcValue>, Vec<usize>, Vec<CalcValue>)],
    fields: usize,
    depth: i64,
    all: &[usize],
) -> Step<Vec<Line>> {
    let levels = depth.unsigned_abs() as usize;
    let before = depth < 0;
    let mut labels = vec![text(""); fields.max(1)];
    labels[0] = text(if levels >= 2 { "Grand Total" } else { "Total" });
    let grand = Line {
        labels,
        members: all.to_vec(),
        grand: true,
    };
    let mut out = Vec::new();
    subtotalled(site, groups, 0, levels, before, fields, &mut out)?;
    match depth {
        d if d > 0 => out.push(grand),
        d if d < 0 => out.insert(0, grand),
        _ => {}
    }
    Ok(out)
}

/// The lines of `groups`, which share their first `level` fields, with a subtotal for each run of
/// them sharing field `level` while `level + 1` is below `levels`.
fn subtotalled(
    site: &Site<'_>,
    groups: &[(Vec<CalcValue>, Vec<usize>, Vec<CalcValue>)],
    level: usize,
    levels: usize,
    before: bool,
    fields: usize,
    out: &mut Vec<Line>,
) -> Step<()> {
    if level + 1 >= levels {
        for g in groups {
            out.push(Line {
                labels: g.0.clone(),
                members: g.1.clone(),
                grand: false,
            });
        }
        return Ok(());
    }
    let mut at = 0;
    while at < groups.len() {
        let mut end = at + 1;
        while end < groups.len() && same(site, &groups[end].0[level], &groups[at].0[level])? {
            end += 1;
        }
        let run = &groups[at..end];
        let mut labels = vec![text(""); fields];
        labels[..=level].clone_from_slice(&run[0].0[..=level]);
        let subtotal = Line {
            labels,
            members: run.iter().flat_map(|g| g.1.iter().copied()).collect(),
            grand: false,
        };
        if before {
            out.push(subtotal);
            subtotalled(site, run, level + 1, levels, before, fields, out)?;
        } else {
            subtotalled(site, run, level + 1, levels, before, fields, out)?;
            out.push(subtotal);
        }
        at = end;
    }
    Ok(())
}

fn pivotby(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let row_fields = table(site, a, 0, sv)?;
    let col_fields = table(site, a, 1, sv)?;
    let values = table(site, a, 2, sv)?;
    let functions = functions(a, 3)?;
    if row_fields.height() != values.height() || col_fields.height() != values.height() {
        return fail(W::Value);
    }
    let (has_header, show) = headers(site, a, 4, &values)?;
    let row_depth = whole(site, a, 5)?.unwrap_or(1);
    let row_entries = sort_entries(site, a, 6)?;
    let col_depth = whole(site, a, 7)?.unwrap_or(1);
    let col_entries = sort_entries(site, a, 8)?;
    let relative_to = whole(site, a, 10)?.unwrap_or(0);
    let (kr, kc) = (row_fields.cols, col_fields.cols);
    for (depth, fields) in [(row_depth, kr), (col_depth, kc)] {
        if depth.unsigned_abs() > 1 && depth.unsigned_abs() as usize > fields {
            return fail(W::Value);
        }
    }
    if col_depth.unsigned_abs() > 1 && values.cols > 1 {
        // Column subtotals beside several value columns are not measured.
        return Err(site.not_modelled(Some(7)));
    }
    if !(0..=4).contains(&relative_to) {
        return fail(W::Value);
    }
    let pairs = pairs(&functions, values.cols)?;
    let start = usize::from(has_header);
    if values.height() <= start {
        return fail(W::Value);
    }
    let count = values.height() - start;
    let keep = filter(site, a, 9, count, values.height(), sv)?;
    let rows: Vec<usize> = (start..values.height()).filter(|r| keep[r - start]).collect();
    let row_keys = canonical(site, &row_fields.rows, &rows)?;
    let col_keys = canonical(site, &col_fields.rows, &rows)?;
    let mut row_groups: Vec<(Vec<CalcValue>, Vec<usize>, Vec<CalcValue>)> = group(site, &row_keys, &rows)?
        .into_iter()
        .map(|(k, m)| (k, m, Vec::new()))
        .collect();
    let mut col_groups: Vec<(Vec<CalcValue>, Vec<usize>, Vec<CalcValue>)> = group(site, &col_keys, &rows)?
        .into_iter()
        .map(|(k, m)| (k, m, Vec::new()))
        .collect();
    sort_groups(site, &mut row_groups, kr, &row_entries, false)?;
    sort_groups(site, &mut col_groups, kc, &col_entries, false)?;
    let np = pairs.len();
    // The rows each cell of the result aggregates, and the rows its two-argument function is
    // relative to.
    let intersect = |x: &[usize], y: &[usize]| -> Vec<usize> { x.iter().copied().filter(|r| y.contains(r)).collect() };
    let relative = |row_members: &[usize], col_members: &[usize]| -> Vec<usize> {
        match relative_to {
            1 | 3 => row_members.to_vec(),
            2 => rows.clone(),
            _ => col_members.to_vec(),
        }
    };
    let mut out: Vec<Vec<CalcValue>> = Vec::new();
    // Column layout: the column lines, each with np value columns.
    let columns = lines(site, &col_groups, kc, col_depth, &rows)?;
    let blank_totals = values.cols > 1;
    // Header rows.
    if show {
        for level in 0..kc {
            let mut header = vec![text(""); kr];
            if level == 0 {
                header.push(if has_header { col_fields.rows[0][0].clone() } else { text("Column Field 1") });
                header.extend(std::iter::repeat_n(text(""), columns.len() * np - 1));
            } else {
                header.extend(std::iter::repeat_n(text(""), columns.len() * np));
            }
            out.push(header);
        }
    }
    for level in 0..kc {
        let mut header = vec![text(""); kr];
        for column in &columns {
            for _ in 0..np {
                header.push(column.labels[level].clone());
            }
        }
        out.push(header);
    }
    if show {
        let mut header: Vec<CalcValue> = (0..kr)
            .map(|c| if has_header { row_fields.rows[0][c].clone() } else { text(&format!("Row Field {}", c + 1)) })
            .collect();
        for _ in &columns {
            for &(v, f) in &pairs {
                header.push(if functions.len() > 1 {
                    function_name(&functions[f])
                } else if has_header {
                    values.rows[0][v].clone()
                } else {
                    text(&format!("Value {}", v + 1))
                });
            }
        }
        out.push(header);
    } else if functions.len() > 1 {
        let mut header = vec![text(""); kr];
        for _ in &columns {
            header.extend(pairs.iter().map(|&(_, f)| function_name(&functions[f])));
        }
        out.push(header);
    }
    let body = |site: &Site<'_>, sv: &mut EvalServices<'_>, label: Vec<CalcValue>, row_members: &[usize]| -> Step<Vec<CalcValue>> {
        let mut row = label;
        for column in &columns {
            let col_members = &column.members;
            let members = intersect(row_members, col_members);
            for &(v, f) in &pairs {
                if column.grand && blank_totals {
                    row.push(text(""));
                    continue;
                }
                if members.is_empty() {
                    row.push(text(""));
                    continue;
                }
                let rel = relative(row_members, col_members);
                row.push(aggregate(site, sv, &functions[f], &values, v, &members, &rel)?);
            }
        }
        Ok(row)
    };
    for line in lines(site, &row_groups, kr, row_depth, &rows)? {
        out.push(body(site, sv, line.labels, &line.members)?);
    }
    for row in &mut out {
        for cell in row.iter_mut() {
            if matches!(cell.core_ref(), CoreRef::Missing) {
                *cell = text("");
            }
        }
    }
    result(out)
}

ported_family! {
    code: CODE;
    1 groupby_fn "GROUPBY" (3, 8) [R "row_fields", R "values", R "function", V "field_headers", V "total_depth", R "sort_order", R "filter_array", V "field_relationship"] |s, a, sv| { groupby(s, a, sv) }
    2 pivotby_fn "PIVOTBY" (4, 11) [R "row_fields", R "col_fields", R "values", R "function", V "field_headers", V "row_total_depth", R "row_sort_order", V "col_total_depth", R "col_sort_order", R "filter_array", V "relative_to"] |s, a, sv| { pivotby(s, a, sv) }
}
