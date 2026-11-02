//! The dynamic-array family fragment ported from OxFunc (functions/ at 6fca3518):
//! dynamic_array_reshape_family.rs (FILTER, SORT, SORTBY, UNIQUE, TAKE, DROP, CHOOSEROWS,
//! CHOOSECOLS, VSTACK, TOCOL, TOROW, WRAPROWS, WRAPCOLS, EXPAND), sequence.rs (SEQUENCE) and
//! hstack.rs (HSTACK). The shaping kernels are OxFunc's ([`crate::ported::dynamic_array_reshape_family`]);
//! each returns an array, which a formula spills (calc-engine owns the spill).
//!
//! An array argument is a value-or-reference parameter: a reference reaches the kernel as the
//! array of its cells' values (a blank cell as an empty value), an array as itself and a single
//! value as a one-by-one array. The counts, flags and pads are value parameters, so a formula lifts
//! them over an array. CHOOSEROWS' and CHOOSECOLS' selectors may be arrays, read in row-major
//! order. A result with no cell is #CALC!.
//!
//! SORT and SORTBY compare as the catalogue's comparison operators do ([`super::comparison`]):
//! numbers by Excel's 15-digit rule, text ignoring case under Excel's collation, FALSE before
//! TRUE; UNIQUE takes text equal under the same collation and numbers equal as binary64. OxFunc orders kinds as blank, numbers, text, logicals, errors,
//! and two errors by their code; this fragment keeps that order. A comparison the operators refuse
//! is refused here too.

use super::comparison::{self, Want};
use super::kit::{fail, grid};
use crate::call::EvalServices;
use crate::kernel::{Exit, Site, Step};
use crate::ported::dynamic_array_reshape_family::{
    self as k, CoercionError, DynamicArrayReshapeEvalError as E,
};
use crate::value::{
    ArrayShape, CalcArray, CalcValue, CellKey, CoreRef, GridArea, ReferenceLike, WorksheetErrorCode,
};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;

const CODE: u16 = 310;

/// The arguments as OxFunc's kernels take them: each reference read as the array of its values.
fn prepared(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<Vec<CalcValue>> {
    let mut out = Vec::with_capacity(a.len());
    for index in 0..a.len() {
        out.push(prepared_one(site, a, index, sv)?);
    }
    Ok(out)
}

fn prepared_one(
    site: &Site<'_>,
    a: &[CalcValue],
    index: usize,
    sv: &mut EvalServices<'_>,
) -> Step<CalcValue> {
    if !matches!(a[index].core_ref(), CoreRef::Reference(_)) {
        return Ok(a[index].clone());
    }
    let g = grid(site, a, index, sv)?;
    let shape = ArrayShape {
        rows: g.rows,
        cols: g.cols,
    };
    CalcArray::new(shape, g.cells)
        .map(CalcValue::array)
        .ok_or_else(|| site.not_modelled(Some(index)))
}

/// TOCOL (`column`) or TOROW of a reference of several areas (a 3-D reference): each area flattened
/// by the call's own ignore and scan arguments, in the reference's order (sheet by sheet), the
/// results joined into one column or row; `None` for any other first argument. Excel 16.0.20430
/// (2026-10-25, dc-6au), sheets Ja, Fe and Ma holding 1, 10 / 100 at A1, B1 / A2 (times 1, 2, 3):
/// `=TOCOL(Ja:Ma!A1:B1)` and `=TOROW(..)` give 1,10,2,20,3,30, and `=TOCOL(Ja:Ma!A1:B2,1)`, blanks
/// ignored, 1,10,100,2,20,200,3,30,300 (9 rows). An area whose cells are all left out adds
/// nothing; when every area does, the result is the kernel's own for an empty one.
fn by_area(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
    kernel: fn(&[CalcValue]) -> Result<CalcValue, E>,
    column: bool,
) -> Step<Option<CalcValue>> {
    let Some(reference) = a[0].as_reference() else {
        return Ok(None);
    };
    if reference.areas().len() < 2 {
        return Ok(None);
    }
    let mut cells = Vec::new();
    let mut empty = None;
    for &area in reference.areas() {
        let mut args = a.to_vec();
        args[0] = CalcValue::reference(ReferenceLike::area(area));
        args[0] = prepared_one(site, &args, 0, sv)?;
        match result(site, kernel(&args)) {
            Ok(value) => match value.as_array() {
                Some(array) => cells.extend(array.iter_row_major().cloned()),
                None => cells.push(value),
            },
            // An area whose every cell is left out (#CALC!, an empty array).
            Err(Exit::Error(WorksheetErrorCode::Calc)) => {
                empty.get_or_insert(WorksheetErrorCode::Calc);
            }
            Err(other) => return Err(other),
        }
    }
    if cells.is_empty() {
        return match empty {
            Some(code) => fail(code),
            None => Ok(None),
        };
    }
    let count = cells.len();
    let shape = if column {
        ArrayShape {
            rows: count,
            cols: 1,
        }
    } else {
        ArrayShape {
            rows: 1,
            cols: count,
        }
    };
    Ok(Some(CalcValue::array(
        CalcArray::new(shape, cells).expect("one cell per item of the shape"),
    )))
}

/// A kernel's result: its array, or its failure as a worksheet error (a coercion calc-func does
/// not model is refused).
fn result<T>(site: &Site<'_>, r: Result<T, E>) -> Step<T> {
    match r {
        Ok(value) => Ok(value),
        Err(E::Preparation(CoercionError::NotModelled)) => Err(site.not_modelled(None)),
        Err(error) => fail(k::map_dynamic_array_reshape_error_to_ws(&error)),
    }
}

/// Runs `kernel` on the prepared arguments.
fn run(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
    kernel: fn(&[CalcValue]) -> Result<CalcValue, E>,
) -> Step<CalcValue> {
    let args = prepared(site, a, sv)?;
    result(site, kernel(&args))
}

/// TAKE (`take`) or DROP of a reference to one area: the reference to the rows and columns kept,
/// or `None` for an array argument. Excel returns a reference here: `=_xlfn.DROP(Data!A1:A3,1)`
/// entered as a legacy formula outside rows 1 to 3 is #VALUE!, the implicit intersection of the
/// reference A2:A3 (the catalogue oracle's capture), not 5, the first of the values.
fn sub_area(site: &Site<'_>, a: &[CalcValue], take: bool) -> Step<Option<CalcValue>> {
    let Some(reference) = a[0].as_reference() else {
        return Ok(None);
    };
    let [area] = reference.areas() else {
        return Ok(None);
    };
    let (rows, cols) = (area.row_count() as usize, area.column_count() as usize);
    let count = |index: usize, full: isize| -> Step<isize> {
        match a.get(index) {
            None => Ok(full),
            Some(value) if value.is_missing() && index == 1 && a.len() > 2 => Ok(full),
            Some(value) => match k::parse_integer(value) {
                Ok(n) => Ok(n),
                Err(error) => result::<CalcValue>(site, Err(error)).map(|_| 0),
            },
        }
    };
    let (row_full, col_full) = if take {
        (rows as isize, cols as isize)
    } else {
        (0, 0)
    };
    let (row_count, col_count) = (count(1, row_full)?, count(2, col_full)?);
    let span = |len: usize, n: isize| {
        if take {
            k::take_span(len, n)
        } else {
            k::drop_span(len, n)
        }
    };
    let ((r0, r1), (c0, c1)) = match (span(rows, row_count), span(cols, col_count)) {
        (Ok(r), Ok(c)) => (r, c),
        (Err(error), _) | (_, Err(error)) => {
            return result::<CalcValue>(site, Err(error)).map(|_| None);
        }
    };
    let first = area.first();
    let corner = |row: usize, col: usize| {
        CellKey::new(
            area.sheet(),
            first.row() + row as u32,
            first.column() + col as u32,
        )
    };
    match (corner(r0, c0), corner(r1 - 1, c1 - 1)) {
        (Ok(top_left), Ok(bottom_right)) => match GridArea::new(top_left, bottom_right) {
            Ok(kept) => Ok(Some(CalcValue::reference(ReferenceLike::area(kept)))),
            Err(_) => Err(site.not_modelled(Some(0))),
        },
        _ => Err(site.not_modelled(Some(0))),
    }
}

/// CHOOSEROWS and CHOOSECOLS: the selectors after the first argument, each array flattened in
/// row-major order, as OxFunc's surface expands them.
fn choose(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
    kernel: fn(&[CalcValue]) -> Result<CalcValue, E>,
) -> Step<CalcValue> {
    let args = prepared(site, a, sv)?;
    let mut flat = vec![args[0].clone()];
    for arg in &args[1..] {
        match arg.as_array() {
            Some(array) => flat.extend(array.iter_row_major().cloned()),
            None => flat.push(arg.clone()),
        }
    }
    result(site, kernel(&flat))
}

/// A prepared argument as an array (a single value as one by one).
fn as_array(value: &CalcValue) -> CalcArray {
    match value.as_array() {
        Some(array) => array.clone(),
        None => CalcArray::from_scalar(match value.core_ref() {
            CoreRef::Missing => CalcValue::empty(),
            _ => value.clone(),
        }),
    }
}

/// An integer argument of SORT and SORTBY: a blank or omitted one is `None`.
fn integer(site: &Site<'_>, value: Option<&CalcValue>) -> Step<Option<isize>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if matches!(value.core_ref(), CoreRef::Missing | CoreRef::Empty) {
        return Ok(None);
    }
    match k::coerce_prepared_to_number(value) {
        Ok(n) if n.is_finite() => Ok(Some(n.trunc() as isize)),
        Ok(_) => fail(WorksheetErrorCode::Value),
        Err(CoercionError::WorksheetError(code)) => fail(code),
        Err(CoercionError::NotModelled) => Err(site.not_modelled(None)),
        Err(CoercionError::UnsupportedValueKind(_)) => fail(WorksheetErrorCode::Value),
    }
}

/// A sort order: 1 (or omitted) ascending, -1 descending, anything else #VALUE!.
fn descending(site: &Site<'_>, value: Option<&CalcValue>) -> Step<bool> {
    match integer(site, value)? {
        None | Some(1) => Ok(false),
        Some(-1) => Ok(true),
        Some(_) => fail(WorksheetErrorCode::Value),
    }
}

/// A logical flag argument (by_col, exactly_once): omitted or blank is FALSE.
fn flag(site: &Site<'_>, value: Option<&CalcValue>) -> Step<bool> {
    match value.map(CalcValue::core_ref) {
        None | Some(CoreRef::Missing | CoreRef::Empty) => Ok(false),
        Some(CoreRef::Logical(b)) => Ok(b),
        Some(CoreRef::Number(n)) => Ok(n != 0.0),
        Some(CoreRef::Error(code)) => fail(code),
        Some(_) => match integer(site, value)? {
            Some(n) => Ok(n != 0),
            None => Ok(false),
        },
    }
}

/// The rank of a value's kind in the sort order.
fn kind(value: &CalcValue) -> u8 {
    match value.core_ref() {
        CoreRef::Empty | CoreRef::Missing => 0,
        CoreRef::Number(_) => 1,
        CoreRef::Text(_) => 2,
        CoreRef::Logical(_) => 3,
        CoreRef::Error(_) => 4,
        CoreRef::Array(_) | CoreRef::Reference(_) => 5,
    }
}

/// The order of two values for SORT and SORTBY.
fn order(site: &Site<'_>, x: &CalcValue, y: &CalcValue) -> Step<Ordering> {
    match (x.core_ref(), y.core_ref()) {
        (CoreRef::Number(a), CoreRef::Number(b)) => comparison::numbers(site, a, b),
        (CoreRef::Text(a), CoreRef::Text(b)) => {
            comparison::texts(a.utf16_code_units(), b.utf16_code_units(), Want::Order)
                .map_err(|_| site.not_modelled(None))
        }
        (CoreRef::Logical(a), CoreRef::Logical(b)) => Ok(a.cmp(&b)),
        (CoreRef::Error(a), CoreRef::Error(b)) => Ok((a as u8).cmp(&(b as u8))),
        (CoreRef::Array(_) | CoreRef::Reference(_), _)
        | (_, CoreRef::Array(_) | CoreRef::Reference(_)) => Err(site.not_modelled(None)),
        _ => Ok(kind(x).cmp(&kind(y))),
    }
}

/// The indices `0..len` stably sorted by `keys` (each a key per index and its direction); the
/// first refused comparison refuses the call.
fn sorted_indices(
    site: &Site<'_>,
    len: usize,
    keys: &[(Vec<CalcValue>, bool)],
) -> Step<Vec<usize>> {
    let refused: RefCell<Option<Exit>> = RefCell::new(None);
    let mut indices: Vec<usize> = (0..len).collect();
    indices.sort_by(|&i, &j| {
        if refused.borrow().is_some() {
            return Ordering::Equal;
        }
        for (key, descending) in keys {
            match order(site, &key[i], &key[j]) {
                Ok(Ordering::Equal) => continue,
                Ok(o) => return if *descending { o.reverse() } else { o },
                Err(exit) => {
                    *refused.borrow_mut() = Some(exit);
                    return Ordering::Equal;
                }
            }
        }
        Ordering::Equal
    });
    match refused.into_inner() {
        Some(exit) => Err(exit),
        None => Ok(indices),
    }
}

/// `array`'s rows (or, `by_col`, its columns) in the order of `indices`.
fn reordered(array: &CalcArray, indices: &[usize], by_col: bool) -> Step<CalcValue> {
    let ArrayShape { rows, cols } = array.shape();
    let mut cells = Vec::with_capacity(rows * cols);
    for row in 0..rows {
        for col in 0..cols {
            let (r, c) = if by_col {
                (row, indices[col])
            } else {
                (indices[row], col)
            };
            cells.push(array.get(r, c).cloned().unwrap_or_else(CalcValue::empty));
        }
    }
    Ok(CalcValue::array(
        CalcArray::new(ArrayShape { rows, cols }, cells).expect("the array's own shape"),
    ))
}

/// The cells of row (or column, `by_col`) `index` of `array`.
fn line(array: &CalcArray, index: usize, by_col: bool) -> Vec<CalcValue> {
    let ArrayShape { rows, cols } = array.shape();
    if by_col {
        (0..rows)
            .map(|r| {
                array
                    .get(r, index)
                    .cloned()
                    .unwrap_or_else(CalcValue::empty)
            })
            .collect()
    } else {
        (0..cols)
            .map(|c| {
                array
                    .get(index, c)
                    .cloned()
                    .unwrap_or_else(CalcValue::empty)
            })
            .collect()
    }
}

/// SORT: by the sort_index columns (rows, `by_col`), each with its order; an array of indices
/// with an array of orders, or one order for all.
fn sort(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let args = prepared(site, a, sv)?;
    let array = as_array(&args[0]);
    let by_col = flag(site, args.get(3))?;
    let ArrayShape { rows, cols } = array.shape();
    let (len, width) = if by_col { (cols, rows) } else { (rows, cols) };
    let indices: Vec<CalcValue> = match args.get(1) {
        Some(value) => match value.as_array() {
            Some(list) => list.iter_row_major().cloned().collect(),
            None => vec![value.clone()],
        },
        None => vec![CalcValue::missing()],
    };
    let orders: Vec<CalcValue> = match args.get(2) {
        Some(value) => match value.as_array() {
            Some(list) => list.iter_row_major().cloned().collect(),
            None => vec![value.clone()],
        },
        None => vec![CalcValue::missing()],
    };
    if orders.len() > 1 && orders.len() != indices.len() {
        return fail(WorksheetErrorCode::Value);
    }
    let mut keys = Vec::with_capacity(indices.len());
    for (n, index) in indices.iter().enumerate() {
        let at = match integer(site, Some(index))? {
            None => 0,
            Some(i) if i >= 1 && (i as usize) <= width => i as usize - 1,
            Some(_) => return fail(WorksheetErrorCode::Value),
        };
        let down = descending(site, orders.get(if orders.len() == 1 { 0 } else { n }))?;
        let key: Vec<CalcValue> = (0..len)
            .map(|i| {
                let (r, c) = if by_col { (at, i) } else { (i, at) };
                array.get(r, c).cloned().unwrap_or_else(CalcValue::empty)
            })
            .collect();
        keys.push((key, down));
    }
    let order = sorted_indices(site, len, &keys)?;
    reordered(&array, &order, by_col)
}

/// SORTBY: by each by_array, a row or column the length of the array's rows (or of its columns,
/// when the array is one row and a key is a row), with its order.
fn sortby(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let args = prepared(site, a, sv)?;
    // An argument that is one error value is the result (Excel 16.0.20430, 2026-10-25, dc-6au:
    // `=SORTBY(1,#REF!)` #REF!, `=SORTBY(1,#DIV/0!)` #DIV/0!, `=SORTBY({3;1;2},{3;1;2},#REF!)`
    // and `=SORTBY(1,1,#REF!)` #REF!).
    for argument in &args {
        if let CoreRef::Error(code) = argument.core_ref() {
            return fail(code);
        }
    }
    let array = as_array(&args[0]);
    let ArrayShape { rows, cols } = array.shape();
    let mut keys: Vec<(Vec<CalcValue>, bool)> = Vec::new();
    let mut by_col = None;
    let mut at = 1;
    while at < args.len() {
        let key = as_array(&args[at]);
        at += 1;
        let down = match args.get(at) {
            Some(value) if value.as_array().is_none() => {
                at += 1;
                descending(site, Some(value))?
            }
            _ => false,
        };
        let shape = key.shape();
        let (values, columns) = if shape.cols == 1 && shape.rows == rows {
            (
                (0..rows)
                    .map(|r| key.get(r, 0).cloned().unwrap_or_else(CalcValue::empty))
                    .collect(),
                false,
            )
        } else if shape.rows == 1 && shape.cols == cols {
            (
                (0..cols)
                    .map(|c| key.get(0, c).cloned().unwrap_or_else(CalcValue::empty))
                    .collect(),
                true,
            )
        } else {
            return fail(WorksheetErrorCode::Value);
        };
        if *by_col.get_or_insert(columns) != columns {
            return fail(WorksheetErrorCode::Value);
        }
        keys.push((values, down));
    }
    let by_col = by_col.unwrap_or(false);
    let len = if by_col { cols } else { rows };
    let order = sorted_indices(site, len, &keys)?;
    reordered(&array, &order, by_col)
}

/// Whether two cells are the same for UNIQUE: numbers when equal as binary64 (OxFunc's rule), text
/// ignoring case under the collation, and otherwise equal kinds with equal values.
fn same(site: &Site<'_>, x: &CalcValue, y: &CalcValue) -> Step<bool> {
    match (x.core_ref(), y.core_ref()) {
        (CoreRef::Text(a), CoreRef::Text(b)) => {
            comparison::texts(a.utf16_code_units(), b.utf16_code_units(), Want::Equality)
                .map(Ordering::is_eq)
                .map_err(|_| site.not_modelled(None))
        }
        (CoreRef::Number(a), CoreRef::Number(b)) => Ok(a == b),
        _ => order(site, x, y).map(Ordering::is_eq),
    }
}

/// A coarse key under which equal cells always fall together (text by its collation sort key,
/// numbers by their bits), so each line is compared only with the lines that share it; `None`
/// when a text's sort key is not built (dc-zsn.1), whose equality to the other texts is then not
/// decided.
fn bucket(cells: &[CalcValue]) -> Option<String> {
    let mut key = String::new();
    for cell in cells {
        match cell.core_ref() {
            CoreRef::Number(n) => key.push_str(&format!("n{:x}|", (n + 0.0).to_bits())),
            CoreRef::Text(t) => {
                key.push('t');
                for byte in comparison::text_bucket(t.utf16_code_units())? {
                    key.push_str(&format!("{byte:02x}"));
                }
                key.push('|');
            }
            CoreRef::Logical(b) => key.push_str(if b { "T|" } else { "F|" }),
            CoreRef::Error(code) => key.push_str(&format!("e{}|", code as u8)),
            _ => key.push_str("b|"),
        }
    }
    Some(key)
}

fn unique(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let args = prepared(site, a, sv)?;
    let array = as_array(&args[0]);
    let by_col = flag(site, args.get(1))?;
    let exactly_once = flag(site, args.get(2))?;
    let ArrayShape { rows, cols } = array.shape();
    let len = if by_col { cols } else { rows };
    let lines: Vec<Vec<CalcValue>> = (0..len).map(|i| line(&array, i, by_col)).collect();
    // Each line's group: the first earlier line equal to it, or itself.
    let mut buckets: HashMap<String, Vec<usize>> = HashMap::new();
    let mut group = vec![0usize; len];
    for i in 0..len {
        let key = bucket(&lines[i]).ok_or_else(|| site.not_modelled(None))?;
        let members = buckets.entry(key).or_default();
        let mut found = None;
        for &j in members.iter() {
            let mut equal = true;
            for (x, y) in lines[i].iter().zip(&lines[j]) {
                if !same(site, x, y)? {
                    equal = false;
                    break;
                }
            }
            if equal {
                found = Some(j);
                break;
            }
        }
        match found {
            Some(j) => group[i] = j,
            None => {
                group[i] = i;
                members.push(i);
            }
        }
    }
    let mut counts = vec![0usize; len];
    for &g in &group {
        counts[g] += 1;
    }
    let kept: Vec<usize> = (0..len)
        .filter(|&i| group[i] == i && (!exactly_once || counts[i] == 1))
        .collect();
    if kept.is_empty() {
        return fail(WorksheetErrorCode::Calc);
    }
    let (out_rows, out_cols) = if by_col {
        (rows, kept.len())
    } else {
        (kept.len(), cols)
    };
    let mut cells = Vec::with_capacity(out_rows * out_cols);
    for r in 0..out_rows {
        for c in 0..out_cols {
            let value = if by_col {
                &lines[kept[c]][r]
            } else {
                &lines[kept[r]][c]
            };
            cells.push(value.clone());
        }
    }
    Ok(CalcValue::array(
        CalcArray::new(
            ArrayShape {
                rows: out_rows,
                cols: out_cols,
            },
            cells,
        )
        .expect("a shape of its cells"),
    ))
}

#[derive(Debug)]
struct SequenceTask(k::SequenceWork);
impl crate::call::KernelTask for SequenceTask {
    fn advance(
        &mut self,
        elements: usize,
        _: &mut EvalServices<'_>,
    ) -> Result<Option<CalcValue>, crate::call::FunctionFault> {
        match self.0.advance(elements) {
            None => Ok(None),
            Some(Ok(value)) => Ok(Some(value)),
            Some(Err(error)) => Ok(Some(CalcValue::error(
                k::map_dynamic_array_reshape_error_to_ws(&error),
            ))),
        }
    }
    fn completed_elements(&self) -> usize {
        self.0.completed_elements()
    }
}

fn sequence_task(
    site: &Site<'_>,
    args: &[CalcValue],
    _: &mut EvalServices<'_>,
) -> Step<Box<dyn crate::call::KernelTask>> {
    Ok(Box::new(SequenceTask(result(
        site,
        k::SequenceWork::new(args),
    )?)))
}

ported_family! {
    code: CODE;
    1 sequence_fn "SEQUENCE" (1, 4) [V "rows", V "columns", V "start", V "step"] resume sequence_resume => sequence_task; |s, a, sv| {
        result(s, k::eval_sequence_prepared(a))
    }
    2 filter_fn "FILTER" (2, 3) [R "array", R "include", R "if_empty"] |s, a, sv| {
        run(s, a, sv, k::eval_filter_prepared)
    }
    3 sort_fn "SORT" (1, 4) [R "array", R "sort_index", R "sort_order", V "by_col"] |s, a, sv| {
        sort(s, a, sv)
    }
    4 sortby_fn "SORTBY" (2, 255) [R "array", R "by_array1", R "sort_order1"] |s, a, sv| {
        sortby(s, a, sv)
    }
    5 unique_fn "UNIQUE" (1, 3) [R "array", V "by_col", V "exactly_once"] |s, a, sv| {
        unique(s, a, sv)
    }
    6 take_fn "TAKE" (2, 3) [R "array", V "rows", V "columns"] |s, a, sv| {
        match sub_area(s, a, true)? {
            Some(reference) => Ok(reference),
            None => run(s, a, sv, k::eval_take_prepared),
        }
    }
    7 drop_fn "DROP" (2, 3) [R "array", V "rows", V "columns"] |s, a, sv| {
        match sub_area(s, a, false)? {
            Some(reference) => Ok(reference),
            None => run(s, a, sv, k::eval_drop_prepared),
        }
    }
    8 chooserows_fn "CHOOSEROWS" (2, 255) [R "array", R "row_num1", R "row_num2"] |s, a, sv| {
        choose(s, a, sv, k::eval_chooserows_prepared)
    }
    9 choosecols_fn "CHOOSECOLS" (2, 255) [R "array", R "col_num1", R "col_num2"] |s, a, sv| {
        choose(s, a, sv, k::eval_choosecols_prepared)
    }
    10 vstack_fn "VSTACK" (1, 254) [R "array1", R "array2"] |s, a, sv| {
        run(s, a, sv, k::eval_vstack_prepared)
    }
    11 hstack_fn "HSTACK" (1, 254) [R "array1", R "array2"] |s, a, sv| {
        // dc-h9u: a row of functions, as GROUPBY and PIVOTBY take them
        // (`GROUPBY(A2:A9,B2:B9,HSTACK(SUM,MAX))`, Excel 16.0.20527).
        if a.iter().all(|v| v.callable_value().is_some()) {
            return CalcArray::from_rows(vec![a.to_vec()])
                .map(CalcValue::array)
                .ok_or_else(|| s.not_modelled(None));
        }
        run(s, a, sv, k::eval_hstack_prepared)
    }
    12 tocol_fn "TOCOL" (1, 3) [R "array", V "ignore", V "scan_by_column"] |s, a, sv| {
        match by_area(s, a, sv, k::eval_tocol_prepared, true)? {
            Some(value) => Ok(value),
            None => run(s, a, sv, k::eval_tocol_prepared),
        }
    }
    13 torow_fn "TOROW" (1, 3) [R "array", V "ignore", V "scan_by_column"] |s, a, sv| {
        match by_area(s, a, sv, k::eval_torow_prepared, false)? {
            Some(value) => Ok(value),
            None => run(s, a, sv, k::eval_torow_prepared),
        }
    }
    14 wraprows_fn "WRAPROWS" (2, 3) [R "vector", V "wrap_count", V "pad_with"] |s, a, sv| {
        run(s, a, sv, k::eval_wraprows_prepared)
    }
    15 wrapcols_fn "WRAPCOLS" (2, 3) [R "vector", V "wrap_count", V "pad_with"] |s, a, sv| {
        run(s, a, sv, k::eval_wrapcols_prepared)
    }
    16 expand_fn "EXPAND" (2, 4) [R "array", V "rows", V "columns", V "pad_with"] |s, a, sv| {
        run(s, a, sv, k::eval_expand_prepared)
    }
}
