//! The dynamic-array shaping kernels ported from OxFunc (functions/ at 6fca3518):
//! dynamic_array_reshape_family.rs's prepared evaluators, sequence.rs and hstack.rs, with the
//! surface layer removed. Each takes its arguments prepared: a reference already read as the array
//! of its values. OxFunc's coercion is read through calc-func's ([`crate::coerce`]).

use crate::coerce::{self, Numeric, Stop};
use crate::value::CalcValue;
use crate::value::{ArrayShape, CalcArray, CoreRef, CoreValue, WorksheetErrorCode};
use std::cmp::Ordering;
use std::collections::HashMap;

/// Why a prepared argument did not read as a number or a logical.
#[derive(Debug, Clone, PartialEq)]
pub enum CoercionError {
    /// The argument is, or reads as, this worksheet error.
    WorksheetError(WorksheetErrorCode),
    /// A kind of value the parameter does not take: #VALUE!.
    UnsupportedValueKind(&'static str),
    /// calc-func's coercion does not model Excel's reading of the value.
    NotModelled,
}

impl From<Stop> for CoercionError {
    fn from(stop: Stop) -> Self {
        match stop {
            Stop::Error(code) => CoercionError::WorksheetError(code),
            Stop::NotModelled => CoercionError::NotModelled,
        }
    }
}

/// A prepared scalar as a number, by worksheet number coercion (a blank is 0).
pub(crate) fn coerce_prepared_to_number(value: &CalcValue) -> Result<f64, CoercionError> {
    match coerce::number(value)? {
        Numeric::Number(n) => Ok(n),
        Numeric::Decimal(decimal) => Ok(decimal.nearest()?),
    }
}

fn coerce_scalar_calc_value_to_number(value: &CalcValue) -> Result<f64, CoercionError> {
    coerce_prepared_to_number(value)
}

#[derive(Debug, Clone, PartialEq)]
pub enum DynamicArrayReshapeEvalError {
    ArityMismatch {
        expected_min: usize,
        expected_max: usize,
        actual: usize,
    },
    Preparation(CoercionError),
    InvalidSelector,
    InvalidCount,
    EmptyArrayResult,
    DimensionTooSmall,
    InvalidIgnoreMode,
    InvalidSortOrder,
    InvalidSortIndex,
    InvalidIncludeShape,
}

fn scalar_cell(arg: &CalcValue) -> CalcValue {
    match arg.core_ref() {
        CoreRef::Number(n) => CalcValue::number(n),
        CoreRef::Text(t) => CalcValue::text(t.clone()),
        CoreRef::Logical(b) => CalcValue::logical(b),
        CoreRef::Error(code) => CalcValue::error(code),
        CoreRef::Reference(_) => CalcValue::error(WorksheetErrorCode::Value),
        CoreRef::Array(_) => CalcValue::error(WorksheetErrorCode::Value),
        CoreRef::Missing | CoreRef::Empty => CalcValue::empty(),
    }
}

fn materialize_array_arg(arg: &CalcValue) -> CalcArray {
    match arg.core_ref() {
        CoreRef::Array(array) => array.clone(),
        _ => CalcArray::from_scalar(scalar_cell(arg)),
    }
}

fn scalar_calc_value(value: &CalcValue) -> CalcValue {
    match value.core_ref() {
        CoreRef::Array(_) | CoreRef::Reference(_) => CalcValue::error(WorksheetErrorCode::Value),
        CoreRef::Missing | CoreRef::Empty => CalcValue::empty(),
        _ => value.clone(),
    }
}

enum StackArgSource<'a> {
    Array(&'a CalcArray),
    Scalar(CalcValue),
}

impl<'a> StackArgSource<'a> {
    fn new(arg: &'a CalcValue) -> Self {
        match arg.core_ref() {
            CoreRef::Array(array) => Self::Array(array),
            _ => Self::Scalar(scalar_cell(arg)),
        }
    }

    fn shape(&self) -> ArrayShape {
        match self {
            Self::Array(array) => array.shape(),
            Self::Scalar(_) => ArrayShape { rows: 1, cols: 1 },
        }
    }

    fn get(&self, row: usize, col: usize) -> Option<&CalcValue> {
        match self {
            Self::Array(array) => array.get(row, col),
            Self::Scalar(cell) if row == 0 && col == 0 => Some(cell),
            Self::Scalar(_) => None,
        }
    }
}

fn prepared_from_array_cell(cell: &CalcValue) -> CalcValue {
    match cell.core_ref() {
        CoreRef::Number(n) => CalcValue::number(n),
        CoreRef::Text(t) => CalcValue::text(t.clone()),
        CoreRef::Logical(b) => CalcValue::logical(b),
        CoreRef::Error(code) => CalcValue::error(code),
        CoreRef::Empty | CoreRef::Missing => CalcValue::empty(),
        CoreRef::Array(_) | CoreRef::Reference(_) => CalcValue::error(WorksheetErrorCode::Value),
    }
}

pub(crate) fn parse_integer(prepared: &CalcValue) -> Result<isize, DynamicArrayReshapeEvalError> {
    let raw =
        coerce_prepared_to_number(prepared).map_err(DynamicArrayReshapeEvalError::Preparation)?;
    if !raw.is_finite() {
        return Err(DynamicArrayReshapeEvalError::InvalidCount);
    }
    let truncated = raw.trunc();
    if (truncated - raw).abs() > f64::EPSILON {
        return Err(DynamicArrayReshapeEvalError::InvalidCount);
    }
    Ok(truncated as isize)
}

fn parse_integer_calc(value: &CalcValue) -> Result<isize, DynamicArrayReshapeEvalError> {
    // The same scalar-context rule `parse_integer` reads through `coerce_prepared_to_number`
    // (a blank count is 0), so the two prep helpers stay equivalent on a blank argument.
    let raw = coerce_scalar_calc_value_to_number(value)
        .map_err(DynamicArrayReshapeEvalError::Preparation)?;
    if !raw.is_finite() {
        return Err(DynamicArrayReshapeEvalError::InvalidCount);
    }
    let truncated = raw.trunc();
    if (truncated - raw).abs() > f64::EPSILON {
        return Err(DynamicArrayReshapeEvalError::InvalidCount);
    }
    Ok(truncated as isize)
}

fn parse_positive_integer(prepared: &CalcValue) -> Result<usize, DynamicArrayReshapeEvalError> {
    let value = parse_integer(prepared)?;
    if value < 1 {
        return Err(DynamicArrayReshapeEvalError::InvalidCount);
    }
    Ok(value as usize)
}

fn parse_bool_like(prepared: &CalcValue) -> Result<bool, DynamicArrayReshapeEvalError> {
    match prepared.core_ref() {
        CoreRef::Logical(b) => Ok(b),
        CoreRef::Number(n) => Ok(n != 0.0),
        CoreRef::Empty | CoreRef::Missing => Ok(false),
        CoreRef::Error(code) => Err(DynamicArrayReshapeEvalError::Preparation(
            CoercionError::WorksheetError(code),
        )),
        _ => Err(DynamicArrayReshapeEvalError::Preparation(
            CoercionError::UnsupportedValueKind("boolean_like"),
        )),
    }
}

fn resolve_selector(index: isize, len: usize) -> Result<usize, DynamicArrayReshapeEvalError> {
    if index == 0 {
        return Err(DynamicArrayReshapeEvalError::InvalidSelector);
    }
    let resolved = if index > 0 {
        (index - 1) as usize
    } else {
        let offset = len as isize + index;
        if offset < 0 {
            return Err(DynamicArrayReshapeEvalError::InvalidSelector);
        }
        offset as usize
    };
    if resolved >= len {
        return Err(DynamicArrayReshapeEvalError::InvalidSelector);
    }
    Ok(resolved)
}

fn build_array(
    rows: usize,
    cols: usize,
    cells: Vec<CalcValue>,
) -> Result<CalcValue, DynamicArrayReshapeEvalError> {
    CalcArray::new(ArrayShape { rows, cols }, cells)
        .map(CalcValue::array)
        .ok_or(DynamicArrayReshapeEvalError::EmptyArrayResult)
}

fn build_calc_array(
    rows: usize,
    cols: usize,
    cells: Vec<CalcValue>,
) -> Result<CalcValue, DynamicArrayReshapeEvalError> {
    CalcArray::new(ArrayShape { rows, cols }, cells)
        .map(CalcValue::array)
        .ok_or(DynamicArrayReshapeEvalError::EmptyArrayResult)
}

fn value_from_if_empty_arg(arg: &CalcValue) -> CalcValue {
    match arg.core_ref() {
        CoreRef::Missing | CoreRef::Empty => {
            CalcValue::text(crate::value::ExcelText::from_utf16_code_units(Vec::new()))
        }
        _ => arg.clone(),
    }
}

fn row_signature(array: &CalcArray, row: usize) -> Vec<CalcValue> {
    array.row_slice(row).expect("validated row").to_vec()
}

fn column_signature(array: &CalcArray, col: usize) -> Vec<CalcValue> {
    (0..array.shape().rows)
        .map(|row| array.get(row, col).expect("validated col").clone())
        .collect()
}

fn cell_signature(cell: &CalcValue) -> String {
    match cell.core_ref() {
        CoreRef::Number(n) => format!("n:{n:?}"),
        CoreRef::Text(t) => format!("t:{}", t.to_string_lossy()),
        CoreRef::Logical(b) => format!("b:{b}"),
        CoreRef::Error(code) => format!("e:{code:?}"),
        CoreRef::Empty | CoreRef::Missing => "m:".to_string(),
        CoreRef::Array(_) => "a:".to_string(),
        CoreRef::Reference(_) => "r:".to_string(),
    }
}

fn signature_key(cells: &[CalcValue]) -> String {
    cells
        .iter()
        .map(cell_signature)
        .collect::<Vec<_>>()
        .join("\u{001f}")
}

fn flatten_cells(array: &CalcArray, by_col: bool) -> Vec<CalcValue> {
    if !by_col {
        return array.iter_row_major().cloned().collect();
    }

    let mut cells = Vec::with_capacity(array.shape().rows * array.shape().cols);
    for col in 0..array.shape().cols {
        for row in 0..array.shape().rows {
            cells.push(array.get(row, col).expect("validated cell").clone());
        }
    }
    cells
}

fn compare_cell_values(lhs: &CalcValue, rhs: &CalcValue) -> Ordering {
    match (lhs.core_ref(), rhs.core_ref()) {
        (CoreRef::Number(a), CoreRef::Number(b)) => a.partial_cmp(&b).unwrap_or(Ordering::Equal),
        (CoreRef::Text(a), CoreRef::Text(b)) => a.to_string_lossy().cmp(&b.to_string_lossy()),
        (CoreRef::Logical(a), CoreRef::Logical(b)) => a.cmp(&b),
        (CoreRef::Empty | CoreRef::Missing, CoreRef::Empty | CoreRef::Missing) => Ordering::Equal,
        (CoreRef::Error(a), CoreRef::Error(b)) => (a as u8).cmp(&(b as u8)),
        (CoreRef::Empty | CoreRef::Missing, _) => Ordering::Less,
        (_, CoreRef::Empty | CoreRef::Missing) => Ordering::Greater,
        (CoreRef::Number(_), _) => Ordering::Less,
        (_, CoreRef::Number(_)) => Ordering::Greater,
        (CoreRef::Text(_), _) => Ordering::Less,
        (_, CoreRef::Text(_)) => Ordering::Greater,
        (CoreRef::Logical(_), _) => Ordering::Less,
        (_, CoreRef::Logical(_)) => Ordering::Greater,
        (CoreRef::Error(_), _) => Ordering::Less,
        (_, CoreRef::Error(_)) => Ordering::Greater,
        _ => Ordering::Equal,
    }
}

pub fn eval_choosecols_prepared(
    args: &[CalcValue],
) -> Result<CalcValue, DynamicArrayReshapeEvalError> {
    let array = materialize_array_arg(&args[0]);
    let cols: Vec<usize> = args[1..]
        .iter()
        .map(parse_integer)
        .map(|result| result.and_then(|idx| resolve_selector(idx, array.shape().cols)))
        .collect::<Result<_, _>>()?;

    let mut cells = Vec::with_capacity(array.shape().rows * cols.len());
    for row in 0..array.shape().rows {
        for col in &cols {
            cells.push(array.get(row, *col).expect("validated selector").clone());
        }
    }
    build_array(array.shape().rows, cols.len(), cells)
}

pub fn eval_chooserows_prepared(
    args: &[CalcValue],
) -> Result<CalcValue, DynamicArrayReshapeEvalError> {
    let array = materialize_array_arg(&args[0]);
    let rows: Vec<usize> = args[1..]
        .iter()
        .map(parse_integer)
        .map(|result| result.and_then(|idx| resolve_selector(idx, array.shape().rows)))
        .collect::<Result<_, _>>()?;

    let mut cells = Vec::with_capacity(rows.len() * array.shape().cols);
    for row in &rows {
        cells.extend(
            array
                .row_slice(*row)
                .expect("validated selector")
                .iter()
                .cloned(),
        );
    }
    build_array(rows.len(), array.shape().cols, cells)
}

pub(crate) fn take_span(
    len: usize,
    count: isize,
) -> Result<(usize, usize), DynamicArrayReshapeEvalError> {
    if count == 0 {
        return Err(DynamicArrayReshapeEvalError::EmptyArrayResult);
    }
    if count > 0 {
        let take = usize::min(count as usize, len);
        Ok((0, take))
    } else {
        let take = usize::min((-count) as usize, len);
        Ok((len - take, len))
    }
}

pub(crate) fn drop_span(
    len: usize,
    count: isize,
) -> Result<(usize, usize), DynamicArrayReshapeEvalError> {
    let (start, end) = if count >= 0 {
        (usize::min(count as usize, len), len)
    } else {
        (0, len.saturating_sub((-count) as usize))
    };
    if start >= end {
        return Err(DynamicArrayReshapeEvalError::EmptyArrayResult);
    }
    Ok((start, end))
}

pub fn eval_take_prepared(args: &[CalcValue]) -> Result<CalcValue, DynamicArrayReshapeEvalError> {
    let array = materialize_array_arg(&args[0]);
    let row_count = match args.get(1) {
        Some(arg) if matches!(arg.core_ref(), CoreRef::Missing) && args.get(2).is_some() => {
            array.shape().rows as isize
        }
        Some(arg) => parse_integer(arg)?,
        None => return Err(DynamicArrayReshapeEvalError::InvalidCount),
    };
    let col_count = if let Some(arg) = args.get(2) {
        parse_integer(arg)?
    } else {
        array.shape().cols as isize
    };
    let (row_start, row_end) = take_span(array.shape().rows, row_count)?;
    let (col_start, col_end) = take_span(array.shape().cols, col_count)?;
    let rows = row_end - row_start;
    let cols = col_end - col_start;
    let mut cells = Vec::with_capacity(rows * cols);
    for row in row_start..row_end {
        for col in col_start..col_end {
            cells.push(array.get(row, col).expect("validated slice").clone());
        }
    }
    build_array(rows, cols, cells)
}

pub fn eval_drop_prepared(args: &[CalcValue]) -> Result<CalcValue, DynamicArrayReshapeEvalError> {
    let array = materialize_array_arg(&args[0]);
    let row_count = match args.get(1) {
        Some(arg) if matches!(arg.core_ref(), CoreRef::Missing) && args.get(2).is_some() => 0,
        Some(arg) => parse_integer(arg)?,
        None => return Err(DynamicArrayReshapeEvalError::InvalidCount),
    };
    let col_count = if let Some(arg) = args.get(2) {
        parse_integer(arg)?
    } else {
        0
    };
    let (row_start, row_end) = drop_span(array.shape().rows, row_count)?;
    let (col_start, col_end) = drop_span(array.shape().cols, col_count)?;
    let rows = row_end - row_start;
    let cols = col_end - col_start;
    let mut cells = Vec::with_capacity(rows * cols);
    for row in row_start..row_end {
        for col in col_start..col_end {
            cells.push(array.get(row, col).expect("validated slice").clone());
        }
    }
    build_array(rows, cols, cells)
}

pub fn eval_expand_prepared(args: &[CalcValue]) -> Result<CalcValue, DynamicArrayReshapeEvalError> {
    let array = materialize_array_arg(&args[0]);
    let target_rows = parse_positive_integer(&args[1])?;
    let target_cols = if let Some(arg) = args.get(2) {
        parse_positive_integer(arg)?
    } else {
        array.shape().cols
    };
    if target_rows < array.shape().rows || target_cols < array.shape().cols {
        return Err(DynamicArrayReshapeEvalError::DimensionTooSmall);
    }

    let pad_cell = if let Some(arg) = args.get(3) {
        if matches!(arg.core_ref(), CoreRef::Array(_)) {
            return Err(DynamicArrayReshapeEvalError::Preparation(
                CoercionError::UnsupportedValueKind("scalar_pad"),
            ));
        }
        scalar_cell(arg)
    } else {
        CalcValue::error(WorksheetErrorCode::NA)
    };
    let mut cells = Vec::with_capacity(target_rows * target_cols);
    for row in 0..target_rows {
        for col in 0..target_cols {
            let cell = array
                .get(row, col)
                .cloned()
                .unwrap_or_else(|| pad_cell.clone());
            cells.push(cell);
        }
    }
    build_array(target_rows, target_cols, cells)
}

fn should_ignore_cell(cell: &CalcValue, ignore_mode: usize) -> bool {
    match ignore_mode {
        0 => false,
        1 => matches!(cell.core_ref(), CoreRef::Empty | CoreRef::Missing),
        2 => matches!(cell.core_ref(), CoreRef::Error(_)),
        3 => matches!(
            cell.core_ref(),
            CoreRef::Empty | CoreRef::Missing | CoreRef::Error(_)
        ),
        _ => false,
    }
}

fn parse_ignore_mode(arg: Option<&CalcValue>) -> Result<usize, DynamicArrayReshapeEvalError> {
    let Some(arg) = arg else {
        return Ok(0);
    };
    if matches!(arg.core_ref(), CoreRef::Missing | CoreRef::Empty) {
        return Ok(0);
    }
    let mode = parse_integer(arg)?;
    if !(0..=3).contains(&mode) {
        return Err(DynamicArrayReshapeEvalError::InvalidIgnoreMode);
    }
    Ok(mode as usize)
}

pub fn eval_tocol_prepared(args: &[CalcValue]) -> Result<CalcValue, DynamicArrayReshapeEvalError> {
    let array = materialize_array_arg(&args[0]);
    let ignore_mode = parse_ignore_mode(args.get(1))?;
    let by_col = args
        .get(2)
        .map(parse_bool_like)
        .transpose()?
        .unwrap_or(false);
    let cells: Vec<CalcValue> = flatten_cells(&array, by_col)
        .into_iter()
        .filter(|cell| !should_ignore_cell(cell, ignore_mode))
        .collect();
    if cells.is_empty() {
        return Err(DynamicArrayReshapeEvalError::EmptyArrayResult);
    }
    build_array(cells.len(), 1, cells)
}

pub fn eval_torow_prepared(args: &[CalcValue]) -> Result<CalcValue, DynamicArrayReshapeEvalError> {
    let array = materialize_array_arg(&args[0]);
    let ignore_mode = parse_ignore_mode(args.get(1))?;
    let by_col = args
        .get(2)
        .map(parse_bool_like)
        .transpose()?
        .unwrap_or(false);
    let cells: Vec<CalcValue> = flatten_cells(&array, by_col)
        .into_iter()
        .filter(|cell| !should_ignore_cell(cell, ignore_mode))
        .collect();
    if cells.is_empty() {
        return Err(DynamicArrayReshapeEvalError::EmptyArrayResult);
    }
    build_array(1, cells.len(), cells)
}

pub fn eval_transpose_prepared(
    args: &[CalcValue],
) -> Result<CalcValue, DynamicArrayReshapeEvalError> {
    let array = materialize_array_arg(&args[0]);
    let mut cells = Vec::with_capacity(array.shape().rows * array.shape().cols);
    for row in 0..array.shape().cols {
        for col in 0..array.shape().rows {
            cells.push(array.get(col, row).expect("validated transpose").clone());
        }
    }
    build_array(array.shape().cols, array.shape().rows, cells)
}

pub fn eval_vstack_prepared(args: &[CalcValue]) -> Result<CalcValue, DynamicArrayReshapeEvalError> {
    let sources: Vec<StackArgSource<'_>> = args.iter().map(StackArgSource::new).collect();
    let rows: usize = sources.iter().map(|source| source.shape().rows).sum();
    let cols = sources
        .iter()
        .map(|source| source.shape().cols)
        .max()
        .unwrap_or(1);

    let cells = sources.iter().flat_map(|source| {
        (0..source.shape().rows).flat_map(move |row| {
            (0..cols).map(move |col| {
                source
                    .get(row, col)
                    .cloned()
                    .unwrap_or(CalcValue::error(WorksheetErrorCode::NA))
            })
        })
    });
    build_array(rows, cols, cells.collect())
}

/// Wrapping has its own below-one error; other shaping counts keep their existing policy.
fn wrap_count(value: &CalcValue) -> Result<usize, DynamicArrayReshapeEvalError> {
    let number =
        coerce_prepared_to_number(value).map_err(DynamicArrayReshapeEvalError::Preparation)?;
    if number < 1.0 {
        return Err(DynamicArrayReshapeEvalError::Preparation(
            CoercionError::WorksheetError(WorksheetErrorCode::Num),
        ));
    }
    parse_positive_integer(value)
}

fn wrap_vector(value: &CalcValue) -> Result<CalcArray, DynamicArrayReshapeEvalError> {
    let source = materialize_array_arg(value);
    if source.shape().rows > 1 && source.shape().cols > 1 {
        return Err(DynamicArrayReshapeEvalError::Preparation(
            CoercionError::UnsupportedValueKind("wrap vector"),
        ));
    }
    Ok(source)
}

pub fn eval_wraprows_prepared(
    args: &[CalcValue],
) -> Result<CalcValue, DynamicArrayReshapeEvalError> {
    let wrap_count = wrap_count(&args[1])?;
    match args[0].core_ref() {
        CoreRef::Number(_) | CoreRef::Text(_) | CoreRef::Logical(_) | CoreRef::Error(_) => {
            return Ok(args[0].clone());
        }
        CoreRef::Array(_) | CoreRef::Reference(_) | CoreRef::Missing | CoreRef::Empty => {}
    }

    let source = wrap_vector(&args[0])?;
    let pad_cell = args
        .get(2)
        .map(scalar_cell)
        .unwrap_or(CalcValue::error(WorksheetErrorCode::NA));
    let flat = flatten_cells(&source, false);
    // A maximum width does not pad a vector that already fits in a single row.
    let cols = wrap_count.min(flat.len());
    let rows = flat.len().div_ceil(cols);
    let mut cells = Vec::with_capacity(rows * cols);
    let mut iter = flat.into_iter();
    for _row in 0..rows {
        for _col in 0..cols {
            cells.push(iter.next().unwrap_or_else(|| pad_cell.clone()));
        }
    }
    build_array(rows, cols, cells)
}

pub fn eval_wrapcols_prepared(
    args: &[CalcValue],
) -> Result<CalcValue, DynamicArrayReshapeEvalError> {
    let wrap_count = wrap_count(&args[1])?;
    match args[0].core_ref() {
        CoreRef::Number(_) | CoreRef::Text(_) | CoreRef::Logical(_) | CoreRef::Error(_) => {
            return Ok(args[0].clone());
        }
        CoreRef::Array(_) | CoreRef::Reference(_) | CoreRef::Missing | CoreRef::Empty => {}
    }
    let source = wrap_vector(&args[0])?;
    let pad_cell = args
        .get(2)
        .map(scalar_cell)
        .unwrap_or(CalcValue::error(WorksheetErrorCode::NA));
    let flat = flatten_cells(&source, false);
    let rows = wrap_count.min(flat.len());
    let cols = flat.len().div_ceil(rows);
    let mut grid = vec![pad_cell.clone(); rows * cols];
    for (index, cell) in flat.into_iter().enumerate() {
        let row = index % rows;
        let col = index / rows;
        grid[row * cols + col] = cell;
    }
    build_array(rows, cols, grid)
}

fn build_filter_mask(
    include: &CalcArray,
    target_shape: ArrayShape,
) -> Result<(bool, Vec<bool>), DynamicArrayReshapeEvalError> {
    if include.shape().cols == 1 && include.shape().rows == target_shape.rows {
        let mask = (0..include.shape().rows)
            .map(|row| {
                include
                    .get(row, 0)
                    .map_or(Ok(false), |cell| match cell.core_ref() {
                        CoreRef::Logical(b) => Ok(b),
                        CoreRef::Number(n) => Ok(n != 0.0),
                        CoreRef::Empty | CoreRef::Missing => Ok(false),
                        CoreRef::Error(code) => Err(DynamicArrayReshapeEvalError::Preparation(
                            CoercionError::WorksheetError(code),
                        )),
                        _ => Err(DynamicArrayReshapeEvalError::InvalidIncludeShape),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok((false, mask));
    }
    if include.shape().rows == 1 && include.shape().cols == target_shape.cols {
        let mask = (0..include.shape().cols)
            .map(|col| {
                include
                    .get(0, col)
                    .map_or(Ok(false), |cell| match cell.core_ref() {
                        CoreRef::Logical(b) => Ok(b),
                        CoreRef::Number(n) => Ok(n != 0.0),
                        CoreRef::Empty | CoreRef::Missing => Ok(false),
                        CoreRef::Error(code) => Err(DynamicArrayReshapeEvalError::Preparation(
                            CoercionError::WorksheetError(code),
                        )),
                        _ => Err(DynamicArrayReshapeEvalError::InvalidIncludeShape),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok((true, mask));
    }
    Err(DynamicArrayReshapeEvalError::InvalidIncludeShape)
}

pub fn eval_filter_prepared(args: &[CalcValue]) -> Result<CalcValue, DynamicArrayReshapeEvalError> {
    let array = materialize_array_arg(&args[0]);
    let include = materialize_array_arg(&args[1]);
    let (filter_cols, mask) = build_filter_mask(&include, array.shape())?;

    if !filter_cols {
        let selected_rows: Vec<usize> = mask
            .iter()
            .enumerate()
            .filter_map(|(idx, keep)| if *keep { Some(idx) } else { None })
            .collect();
        if selected_rows.is_empty() {
            return if let Some(if_empty) = args.get(2) {
                Ok(match if_empty.core_ref() {
                    CoreRef::Array(array) => CalcValue::array(array.clone()),
                    _ => value_from_if_empty_arg(if_empty),
                })
            } else {
                Err(DynamicArrayReshapeEvalError::EmptyArrayResult)
            };
        }
        let mut cells = Vec::with_capacity(selected_rows.len() * array.shape().cols);
        for row in selected_rows {
            cells.extend(array.row_slice(row).expect("validated row").iter().cloned());
        }
        return build_array(cells.len() / array.shape().cols, array.shape().cols, cells);
    }

    let selected_cols: Vec<usize> = mask
        .iter()
        .enumerate()
        .filter_map(|(idx, keep)| if *keep { Some(idx) } else { None })
        .collect();
    if selected_cols.is_empty() {
        return if let Some(if_empty) = args.get(2) {
            Ok(match if_empty.core_ref() {
                CoreRef::Array(array) => CalcValue::array(array.clone()),
                _ => value_from_if_empty_arg(if_empty),
            })
        } else {
            Err(DynamicArrayReshapeEvalError::EmptyArrayResult)
        };
    }
    let mut cells = Vec::with_capacity(array.shape().rows * selected_cols.len());
    for row in 0..array.shape().rows {
        for col in &selected_cols {
            cells.push(array.get(row, *col).expect("validated col").clone());
        }
    }
    build_array(array.shape().rows, selected_cols.len(), cells)
}

fn parse_sort_order(arg: Option<&CalcValue>) -> Result<bool, DynamicArrayReshapeEvalError> {
    let Some(arg) = arg else {
        return Ok(false);
    };
    if matches!(arg.core_ref(), CoreRef::Missing | CoreRef::Empty) {
        return Ok(false);
    }
    match parse_integer(arg)? {
        1 => Ok(false),
        -1 => Ok(true),
        _ => Err(DynamicArrayReshapeEvalError::InvalidSortOrder),
    }
}

fn parse_sort_index(
    arg: Option<&CalcValue>,
    len: usize,
) -> Result<usize, DynamicArrayReshapeEvalError> {
    let Some(arg) = arg else {
        return Ok(0);
    };
    let scalarized;
    let arg = match arg.core_ref() {
        CoreRef::Array(array) => {
            scalarized = array
                .get(0, 0)
                .map(prepared_from_array_cell)
                .ok_or(DynamicArrayReshapeEvalError::InvalidSortIndex)?;
            &scalarized
        }
        _ => arg,
    };
    if matches!(arg.core_ref(), CoreRef::Missing | CoreRef::Empty) {
        return Ok(0);
    }
    let idx = parse_integer(arg)?;
    if idx < 1 || idx as usize > len {
        return Err(DynamicArrayReshapeEvalError::InvalidSortIndex);
    }
    Ok(idx as usize - 1)
}

pub fn eval_sort_prepared(args: &[CalcValue]) -> Result<CalcValue, DynamicArrayReshapeEvalError> {
    let array = materialize_array_arg(&args[0]);
    let by_col = args
        .get(3)
        .map(parse_bool_like)
        .transpose()?
        .unwrap_or(false);
    let descending = parse_sort_order(args.get(2))?;
    let sort_across_columns = by_col;

    if sort_across_columns {
        let sort_index = parse_sort_index(args.get(1), array.shape().rows)?;
        let mut order: Vec<usize> = (0..array.shape().cols).collect();
        order.sort_by(|lhs, rhs| {
            let lhs_cell = array.get(sort_index, *lhs).expect("validated column");
            let rhs_cell = array.get(sort_index, *rhs).expect("validated column");
            let ord = compare_cell_values(lhs_cell, rhs_cell);
            if descending { ord.reverse() } else { ord }
        });
        let mut cells = Vec::with_capacity(array.shape().rows * array.shape().cols);
        for row in 0..array.shape().rows {
            for col in &order {
                cells.push(array.get(row, *col).expect("validated cell").clone());
            }
        }
        return build_array(array.shape().rows, array.shape().cols, cells);
    }

    let sort_index = parse_sort_index(args.get(1), array.shape().cols)?;
    let mut order: Vec<usize> = (0..array.shape().rows).collect();
    order.sort_by(|lhs, rhs| {
        let lhs_cell = array.get(*lhs, sort_index).expect("validated row");
        let rhs_cell = array.get(*rhs, sort_index).expect("validated row");
        let ord = compare_cell_values(lhs_cell, rhs_cell);
        if descending { ord.reverse() } else { ord }
    });
    let mut cells = Vec::with_capacity(array.shape().rows * array.shape().cols);
    for row in order {
        cells.extend(array.row_slice(row).expect("validated row").iter().cloned());
    }
    build_array(array.shape().rows, array.shape().cols, cells)
}

#[derive(Debug, Clone)]
struct SortbyKeySpec {
    keys: CalcArray,
    descending: bool,
}

fn parse_sortby_key_specs(
    args: &[CalcValue],
) -> Result<Vec<SortbyKeySpec>, DynamicArrayReshapeEvalError> {
    let mut specs = Vec::new();
    let mut idx = 1usize;
    while idx < args.len() {
        let keys = materialize_array_arg(&args[idx]);
        idx += 1;
        let descending = match args.get(idx) {
            None => false,
            Some(arg) if matches!(arg.core_ref(), CoreRef::Array(_)) => false,
            Some(arg) if matches!(arg.core_ref(), CoreRef::Missing | CoreRef::Empty) => {
                idx += 1;
                false
            }
            Some(arg) => {
                let descending = parse_sort_order(Some(arg))?;
                idx += 1;
                descending
            }
        };
        specs.push(SortbyKeySpec { keys, descending });
    }
    Ok(specs)
}

fn sortby_column_keys(
    by_array: &CalcArray,
    len: usize,
) -> Result<Vec<CalcValue>, DynamicArrayReshapeEvalError> {
    if by_array.shape().rows == 1 && by_array.shape().cols == len {
        return Ok((0..by_array.shape().cols)
            .map(|col| by_array.get(0, col).expect("validated key").clone())
            .collect());
    }
    if by_array.shape().cols == 1 && by_array.shape().rows == len {
        return Ok((0..by_array.shape().rows)
            .map(|row| by_array.get(row, 0).expect("validated key").clone())
            .collect());
    }
    Err(DynamicArrayReshapeEvalError::InvalidIncludeShape)
}

fn sortby_row_keys(
    by_array: &CalcArray,
    len: usize,
) -> Result<Vec<CalcValue>, DynamicArrayReshapeEvalError> {
    if by_array.shape().cols == 1 && by_array.shape().rows == len {
        return Ok((0..by_array.shape().rows)
            .map(|row| by_array.get(row, 0).expect("validated key").clone())
            .collect());
    }
    if by_array.shape().rows == 1 && by_array.shape().cols == len {
        return Ok((0..by_array.shape().cols)
            .map(|col| by_array.get(0, col).expect("validated key").clone())
            .collect());
    }
    Err(DynamicArrayReshapeEvalError::InvalidIncludeShape)
}

pub fn eval_sortby_prepared(args: &[CalcValue]) -> Result<CalcValue, DynamicArrayReshapeEvalError> {
    let array = materialize_array_arg(&args[0]);
    let specs = parse_sortby_key_specs(args)?;

    if array.shape().rows == 1 && array.shape().cols > 1 {
        let key_specs = specs
            .iter()
            .map(|spec| {
                sortby_column_keys(&spec.keys, array.shape().cols)
                    .map(|keys| (keys, spec.descending))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut order: Vec<usize> = (0..array.shape().cols).collect();
        order.sort_by(|lhs, rhs| {
            for (keys, descending) in &key_specs {
                let ord = compare_cell_values(&keys[*lhs], &keys[*rhs]);
                if ord != Ordering::Equal {
                    return if *descending { ord.reverse() } else { ord };
                }
            }
            Ordering::Equal
        });

        let mut cells = Vec::with_capacity(array.shape().cols);
        for col in order {
            cells.push(array.get(0, col).expect("validated cell").clone());
        }
        return build_array(array.shape().rows, array.shape().cols, cells);
    }

    let key_specs = specs
        .iter()
        .map(|spec| {
            sortby_row_keys(&spec.keys, array.shape().rows).map(|keys| (keys, spec.descending))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut order: Vec<usize> = (0..array.shape().rows).collect();
    order.sort_by(|lhs, rhs| {
        for (keys, descending) in &key_specs {
            let ord = compare_cell_values(&keys[*lhs], &keys[*rhs]);
            if ord != Ordering::Equal {
                return if *descending { ord.reverse() } else { ord };
            }
        }
        Ordering::Equal
    });

    let mut cells = Vec::with_capacity(array.shape().rows * array.shape().cols);
    for row in order {
        cells.extend(array.row_slice(row).expect("validated row").iter().cloned());
    }
    build_array(array.shape().rows, array.shape().cols, cells)
}

pub fn eval_unique_prepared(args: &[CalcValue]) -> Result<CalcValue, DynamicArrayReshapeEvalError> {
    let array = materialize_array_arg(&args[0]);
    let by_col = args
        .get(1)
        .map(parse_bool_like)
        .transpose()?
        .unwrap_or(false);
    let exactly_once = args
        .get(2)
        .map(parse_bool_like)
        .transpose()?
        .unwrap_or(false);

    if !by_col {
        let mut counts: HashMap<String, usize> = HashMap::new();
        let mut signatures = Vec::with_capacity(array.shape().rows);
        for row in 0..array.shape().rows {
            let sig = row_signature(&array, row);
            let key = signature_key(&sig);
            *counts.entry(key.clone()).or_insert(0) += 1;
            signatures.push((key, sig));
        }

        let mut seen: HashMap<String, bool> = HashMap::new();
        let mut cells = Vec::new();
        let mut out_rows = 0usize;
        for (key, sig) in signatures {
            let count = *counts.get(&key).expect("signature counted");
            if exactly_once && count != 1 {
                continue;
            }
            if !exactly_once && seen.insert(key, true).is_some() {
                continue;
            }
            out_rows += 1;
            cells.extend(sig);
        }
        if out_rows == 0 {
            return Err(DynamicArrayReshapeEvalError::EmptyArrayResult);
        }
        return build_array(out_rows, array.shape().cols, cells);
    }

    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut signatures = Vec::with_capacity(array.shape().cols);
    for col in 0..array.shape().cols {
        let sig = column_signature(&array, col);
        let key = signature_key(&sig);
        *counts.entry(key.clone()).or_insert(0) += 1;
        signatures.push((key, sig));
    }

    let mut seen: HashMap<String, bool> = HashMap::new();
    let mut kept = Vec::new();
    for (key, sig) in signatures {
        let count = *counts.get(&key).expect("signature counted");
        if exactly_once && count != 1 {
            continue;
        }
        if !exactly_once && seen.insert(key, true).is_some() {
            continue;
        }
        kept.push(sig);
    }
    if kept.is_empty() {
        return Err(DynamicArrayReshapeEvalError::EmptyArrayResult);
    }
    let rows = kept[0].len();
    let cols = kept.len();
    let mut cells = Vec::with_capacity(rows * cols);
    for row in 0..rows {
        for col in 0..cols {
            cells.push(kept[col][row].clone());
        }
    }
    build_array(rows, cols, cells)
}

pub fn map_dynamic_array_reshape_error_to_ws(
    error: &DynamicArrayReshapeEvalError,
) -> WorksheetErrorCode {
    match error {
        DynamicArrayReshapeEvalError::ArityMismatch { .. }
        | DynamicArrayReshapeEvalError::InvalidSelector
        | DynamicArrayReshapeEvalError::InvalidCount
        | DynamicArrayReshapeEvalError::InvalidIgnoreMode
        | DynamicArrayReshapeEvalError::InvalidSortOrder
        | DynamicArrayReshapeEvalError::InvalidSortIndex
        | DynamicArrayReshapeEvalError::InvalidIncludeShape
        | DynamicArrayReshapeEvalError::Preparation(CoercionError::UnsupportedValueKind(_))
        | DynamicArrayReshapeEvalError::Preparation(CoercionError::NotModelled) => {
            WorksheetErrorCode::Value
        }
        DynamicArrayReshapeEvalError::Preparation(CoercionError::WorksheetError(code)) => *code,
        DynamicArrayReshapeEvalError::EmptyArrayResult => WorksheetErrorCode::Calc,
        DynamicArrayReshapeEvalError::DimensionTooSmall => WorksheetErrorCode::Value,
    }
}

/// SEQUENCE (sequence.rs): `rows` by `columns` numbers from `start` by `step`, row by row. A
/// dimension which floors to 0 is #CALC!; a negative one is #VALUE!; an omitted argument takes
/// its default and a blank cell reads as zero.
#[derive(Debug)]
pub(crate) struct SequenceWork {
    rows: usize,
    cols: usize,
    start: f64,
    step: f64,
    count: usize,
    cells: Vec<CalcValue>,
    fractional_dimensions: bool,
    next: f64,
}

impl SequenceWork {
    pub(crate) fn new(args: &[CalcValue]) -> Result<Self, DynamicArrayReshapeEvalError> {
        // Fn2 roots164/165: formed argument errors keep their order before a
        // dimension-derived error. Arrays and callable values keep their own route.
        for value in args {
            if value.callable_value().is_none()
                && let CoreRef::Error(code) = value.core_ref()
            {
                return Err(DynamicArrayReshapeEvalError::Preparation(
                    CoercionError::WorksheetError(code),
                ));
            }
        }
        // For direct numeric and logical dimensions a negative value takes precedence over an
        // empty dimension (Excel 20527, dc-ymjf.2.69). Other coercion and error ordering stays on
        // its existing route.
        let direct_dimension = |arg: Option<&CalcValue>| match arg.map(CalcValue::core_ref) {
            Some(CoreRef::Number(n)) => Some(n),
            Some(CoreRef::Logical(b)) => Some(if b { 1.0 } else { 0.0 }),
            None | Some(CoreRef::Missing) => Some(1.0),
            Some(CoreRef::Empty) => Some(0.0),
            _ => None,
        };
        if let (Some(rows), Some(cols)) = (direct_dimension(args.first()), direct_dimension(args.get(1)))
            && (rows < 0.0 || cols < 0.0)
        {
            return Err(DynamicArrayReshapeEvalError::InvalidCount);
        }
        fn dimension(arg: Option<&CalcValue>) -> Result<(usize, bool), DynamicArrayReshapeEvalError> {
            let raw = match arg {
                None => return Ok((1, false)),
                Some(value) if matches!(value.core_ref(), CoreRef::Missing) => {
                    return Ok((1, false));
                }
                Some(value) => coerce_prepared_to_number(value)
                    .map_err(DynamicArrayReshapeEvalError::Preparation)?,
            };
            if !raw.is_finite() || raw < 0.0 {
                return Err(DynamicArrayReshapeEvalError::InvalidCount);
            }
            let count = super::text_kernels::date_integer_floor(raw);
            Ok((count as usize, raw.fract() != 0.0))
        }
        fn scalar(arg: Option<&CalcValue>) -> Result<f64, DynamicArrayReshapeEvalError> {
            match arg {
                None => Ok(1.0),
                Some(value) if matches!(value.core_ref(), CoreRef::Missing) => {
                    Ok(1.0)
                }
                Some(value) => coerce_prepared_to_number(value)
                    .map_err(DynamicArrayReshapeEvalError::Preparation),
            }
        }
        let (rows, fractional_rows) = dimension(args.first())?;
        let (cols, fractional_cols) = dimension(args.get(1))?;
        let start = scalar(args.get(2))?;
        let step = scalar(args.get(3))?;
        // Excel validates the other numeric arguments before returning an empty-array error.
        if rows == 0 || cols == 0 {
            return Err(DynamicArrayReshapeEvalError::EmptyArrayResult);
        }
        let count = ArrayShape { rows, cols }
            .cell_count()
            .ok_or(DynamicArrayReshapeEvalError::InvalidCount)?;
        Ok(Self {
            rows,
            cols,
            start,
            step,
            count,
            cells: Vec::new(),
            fractional_dimensions: fractional_rows || fractional_cols,
            next: start,
        })
    }

    pub(crate) fn completed_elements(&self) -> usize {
        self.cells.len()
    }

    pub(crate) fn advance(
        &mut self,
        elements: usize,
    ) -> Option<Result<CalcValue, DynamicArrayReshapeEvalError>> {
        let end = self.cells.len().saturating_add(elements).min(self.count);
        for idx in self.cells.len()..end {
            if self.fractional_dimensions {
                // Excel advances each cell by one stored binary64 addition. Defer overflow and
                // subnormal output until their complete array behavior has been measured.
                if !self.next.is_finite() || self.next.is_subnormal() {
                    return Some(Err(DynamicArrayReshapeEvalError::Preparation(
                        CoercionError::NotModelled,
                    )));
                }
                self.cells.push(CalcValue::number(self.next));
                self.next += self.step;
            } else {
                self.cells
                    .push(CalcValue::number(self.start + (idx as f64) * self.step));
            }
        }
        (end == self.count)
            .then(|| build_array(self.rows, self.cols, std::mem::take(&mut self.cells)))
    }
}

pub fn eval_sequence_prepared(
    args: &[CalcValue],
) -> Result<CalcValue, DynamicArrayReshapeEvalError> {
    SequenceWork::new(args)?
        .advance(usize::MAX)
        .expect("unlimited sequence completes")
}

/// HSTACK (hstack.rs): the arguments side by side, each padded with #N/A below to the tallest.
pub fn eval_hstack_prepared(args: &[CalcValue]) -> Result<CalcValue, DynamicArrayReshapeEvalError> {
    let sources: Vec<StackArgSource<'_>> = args.iter().map(StackArgSource::new).collect();
    let rows = sources
        .iter()
        .map(|source| source.shape().rows)
        .max()
        .unwrap_or(1);
    let cols: usize = sources.iter().map(|source| source.shape().cols).sum();
    let mut cells = Vec::with_capacity(rows * cols);
    for row in 0..rows {
        for source in &sources {
            for col in 0..source.shape().cols {
                cells.push(
                    source
                        .get(row, col)
                        .cloned()
                        .unwrap_or(CalcValue::error(WorksheetErrorCode::NA)),
                );
            }
        }
    }
    build_array(rows, cols, cells)
}
