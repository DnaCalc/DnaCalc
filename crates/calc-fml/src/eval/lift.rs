//! Lifting: a function of single values applied to arrays runs on each element.
//!
//! The result's shape is the largest row count by the largest column count of the arrays lifted
//! over. An array of one row is repeated down every row and one of one column across every
//! column; any other array that is too small for a cell of the result leaves that cell #N/A.
//! Measured by OxFml's operator array fixture (`operator_array_arithmetic_cases.json`):
//! `={1,2;3,4}+{10,20,30}` is `{11,22,#N/A;13,24,#N/A}` and `={1,2,3;2,3,4}*-1` lifts the
//! scalar over every element.

use calc_func::value::{ArrayShape, CalcArray, CalcValue};

/// The shape of the result of lifting over `arrays`.
pub(super) fn shape<'a>(arrays: impl Iterator<Item = &'a CalcArray>) -> ArrayShape {
    arrays.fold(ArrayShape { rows: 1, cols: 1 }, |shape, array| ArrayShape {
        rows: shape.rows.max(array.shape().rows),
        cols: shape.cols.max(array.shape().cols),
    })
}

/// The element of `array` that the result's cell at `row`, `col` takes, or `None` when the array
/// is too small for it.
pub(super) fn element(array: &CalcArray, row: usize, col: usize) -> Option<&CalcValue> {
    let ArrayShape { rows, cols } = array.shape();
    let row = if rows == 1 { 0 } else { row };
    let col = if cols == 1 { 0 } else { col };
    array.get(row, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbers(rows: &[&[f64]]) -> CalcArray {
        CalcArray::from_rows(
            rows.iter()
                .map(|row| row.iter().map(|n| CalcValue::number(*n)).collect())
                .collect(),
        )
        .unwrap()
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rows_and_columns_broadcast_and_short_arrays_leave_gaps() {
        let square = numbers(&[&[1.0, 2.0], &[3.0, 4.0]]);
        let row = numbers(&[&[10.0, 20.0, 30.0]]);
        assert_eq!(
            shape([&square, &row].into_iter()),
            ArrayShape { rows: 2, cols: 3 }
        );
        assert_eq!(element(&row, 1, 2), Some(&CalcValue::number(30.0)));
        assert_eq!(element(&square, 1, 2), None);
        assert_eq!(element(&square, 1, 1), Some(&CalcValue::number(4.0)));
    }
}
