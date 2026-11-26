//! LINEST, LOGEST, TREND and GROWTH (and FORECAST's pair kernel), ported from OxFunc
//! functions/regression_forecast_family.rs (6fca3518) with its reference resolution removed: the
//! statistics family reads a reference argument into an array before the call.

use crate::ported::excel_numeric;
use crate::value::{CalcArray, CalcValue, CoreRef, CoreValue, WorksheetErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VectorShape {
    Scalar,
    Row(usize),
    Column(usize),
    Matrix { rows: usize, cols: usize },
}

#[derive(Debug, Clone, PartialEq)]
struct NumericVector {
    values: Vec<f64>,
    shape: VectorShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputShape {
    Scalar,
    Row(usize),
    Column(usize),
    Matrix { rows: usize, cols: usize },
}

#[derive(Debug, Clone, PartialEq)]
struct NumericMatrix {
    rows: usize,
    cols: usize,
    values: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq)]
struct PreparedPredictorInput {
    predictors: NumericMatrix,
    output_shape: OutputShape,
}

#[derive(Debug, Clone, PartialEq)]
struct LinearRegressionModel {
    coefficients: Vec<f64>,
    intercept: f64,
}

/// A least-squares fit with the QR factors LINEST's statistics come from.
#[derive(Debug, Clone, PartialEq)]
struct RegressionFit {
    model: LinearRegressionModel,
    /// The matrix columns in their QR order: the predictor each holds (None for the ones column).
    order: Vec<Option<usize>>,
    /// `r[j][k]` for `k >= j`: the triangular factor, `r[j][j]` the step's `alpha`.
    r: Vec<Vec<f64>>,
    /// `Q'y` (of the centred y with a constant).
    qty: Vec<f64>,
    /// The predictor means (all 0 without a constant).
    means: Vec<f64>,
    /// y as the fit took it (centred with a constant).
    y: Vec<f64>,
    observations: usize,
}

/// An argument as the regression functions read it: the caller has read a reference into an
/// array; an empty or omitted argument is #VALUE!.
fn resolve_arg_eval(arg: &CalcValue) -> Result<CalcValue, WorksheetErrorCode> {
    match arg.core_ref() {
        CoreRef::Missing | CoreRef::Empty => Err(WorksheetErrorCode::Value),
        _ => Ok(arg.clone()),
    }
}

fn optional_arg_value(arg: Option<&CalcValue>) -> Result<Option<CalcValue>, WorksheetErrorCode> {
    match arg {
        None => Ok(None),
        Some(value) if matches!(value.core_ref(), CoreRef::Missing) => Ok(None),
        // Excel 20527 reads a blank cell in regression flag positions as FALSE.
        Some(value) if matches!(value.core_ref(), CoreRef::Empty) => Ok(Some(CalcValue::logical(false))),
        Some(other) => Ok(Some(resolve_arg_eval(other)?)),
    }
}

fn numeric_from_cell(cell: &CalcValue) -> Result<f64, WorksheetErrorCode> {
    match cell.core_ref() {
        CoreRef::Number(n) => Ok(n),
        CoreRef::Error(code) => Err(code),
        CoreRef::Text(_)
        | CoreRef::Logical(_)
        | CoreRef::Empty
        | CoreRef::Missing
        | CoreRef::Array(_)
        | CoreRef::Reference(_) => Err(WorksheetErrorCode::Value),
    }
}

fn numeric_scalar_from_eval(value: &CalcValue) -> Result<f64, WorksheetErrorCode> {
    match value.core_ref() {
        CoreRef::Number(n) => Ok(n),
        CoreRef::Logical(flag) => Ok(if flag { 1.0 } else { 0.0 }),
        CoreRef::Error(code) => Err(code),
        CoreRef::Array(array) => {
            let shape = array.shape();
            if shape.rows == 1 && shape.cols == 1 {
                numeric_from_cell(array.get(0, 0).expect("single-cell array"))
            } else {
                Err(WorksheetErrorCode::Value)
            }
        }
        CoreRef::Text(_) | CoreRef::Reference(_) => Err(WorksheetErrorCode::Value),
        _ => Err(WorksheetErrorCode::Value),
    }
}

fn bool_flag_from_eval_or_default(
    value: Option<CalcValue>,
    default: bool,
) -> Result<bool, WorksheetErrorCode> {
    match value {
        None => Ok(default),
        Some(value) if matches!(value.core_ref(), CoreRef::Logical(_)) => match value.core_ref() {
            CoreRef::Logical(flag) => Ok(flag),
            _ => unreachable!("guarded logical"),
        },
        Some(other) => Ok(numeric_scalar_from_eval(&other)? != 0.0),
    }
}

fn linest_flag_from_eval_or_default(
    value: Option<CalcValue>,
    default: bool,
) -> Result<bool, WorksheetErrorCode> {
    if let Some(value) = value.as_ref() {
        if matches!(value.core_ref(), CoreRef::Error(_)) {
            return Err(WorksheetErrorCode::Value);
        }
        if let CoreRef::Array(array) = value.core_ref() {
            if array.shape().rows == 1 && array.shape().cols == 1
                && matches!(array.get(0, 0).expect("single-cell array").core_ref(), CoreRef::Error(_))
            {
                return Err(WorksheetErrorCode::Value);
            }
        }
    }
    bool_flag_from_eval_or_default(value, default)
}

fn collect_numeric_vector_from_eval(
    value: &CalcValue,
) -> Result<NumericVector, WorksheetErrorCode> {
    match value.core_ref() {
        CoreRef::Number(n) => Ok(NumericVector {
            values: vec![n],
            shape: VectorShape::Scalar,
        }),
        CoreRef::Error(code) => Err(code),
        CoreRef::Array(array) => {
            let shape = array.shape();
            if shape.rows > 1 && shape.cols > 1 {
                return Err(WorksheetErrorCode::Ref);
            }
            let mut values = Vec::with_capacity(shape.rows * shape.cols);
            for cell in array.iter_row_major() {
                values.push(numeric_from_cell(cell)?);
            }
            let vector_shape = if shape.rows == 1 && shape.cols == 1 {
                VectorShape::Scalar
            } else if shape.rows == 1 {
                VectorShape::Row(shape.cols)
            } else {
                VectorShape::Column(shape.rows)
            };
            Ok(NumericVector {
                values,
                shape: vector_shape,
            })
        }
        CoreRef::Text(_) | CoreRef::Logical(_) | CoreRef::Reference(_) => {
            Err(WorksheetErrorCode::Value)
        }
        _ => Err(WorksheetErrorCode::Value),
    }
}

fn collect_numeric_vector_arg(arg: &CalcValue) -> Result<NumericVector, WorksheetErrorCode> {
    let eval = resolve_arg_eval(arg)?;
    collect_numeric_vector_from_eval(&eval)
}

/// A known_y or known_x argument of LINEST, LOGEST, TREND and GROWTH, read for its shape before
/// its values: text, logicals, blanks and held errors give #VALUE! after the shapes agree.
/// A shape disagreement gives #REF!. Direct errors follow scalar admission, and an omitted
/// known_y is #VALUE! (Excel 16.0.20527, dcztvv_r23 and dcztvv_r24:
/// `=LINEST("-11.827",X)`, `LINEST(TRUE,X)`, `LINEST(E1,X)` and `LINEST("",X)` with X eight rows
/// by two columns are #REF!, `LINEST(NA(),X)` #N/A, `LINEST(1/0,X)` #DIV/0!, `LINEST(,X)` #VALUE!;
/// `=LINEST("5",{1;2;3})` is #VALUE! and `LINEST(E1:E2,{1;2;3})` #REF!; eight known y with the
/// known x "53.130000000000003" #REF!, and `LINEST(5,"1")` #VALUE!).
struct ShapeFirst<T> {
    value: T,
    non_numeric: bool,
}

fn shape_first_cell(cell: &CalcValue, non_numeric: &mut bool) -> Result<f64, WorksheetErrorCode> {
    match cell.core_ref() {
        CoreRef::Number(n) => Ok(n),
        CoreRef::Error(code) => Err(code),
        CoreRef::Text(_) | CoreRef::Logical(_) | CoreRef::Empty => {
            *non_numeric = true;
            Ok(f64::NAN)
        }
        CoreRef::Missing | CoreRef::Array(_) | CoreRef::Reference(_) => {
            Err(WorksheetErrorCode::Value)
        }
    }
}

fn shape_first_matrix(arg: &CalcValue) -> Result<ShapeFirst<NumericMatrix>, WorksheetErrorCode> {
    if matches!(arg.core_ref(), CoreRef::Missing) {
        return Err(WorksheetErrorCode::Value);
    }
    let mut non_numeric = false;
    let value = match arg.core_ref() {
        CoreRef::Array(array) => {
            let shape = array.shape();
            let mut values = Vec::with_capacity(shape.rows * shape.cols);
            for cell in array.iter_row_major() {
                if matches!(cell.core_ref(), CoreRef::Error(_)) {
                    non_numeric = true;
                    values.push(f64::NAN);
                } else {
                    values.push(shape_first_cell(cell, &mut non_numeric)?);
                }
            }
            NumericMatrix {
                rows: shape.rows,
                cols: shape.cols,
                values,
            }
        }
        _ => NumericMatrix {
            rows: 1,
            cols: 1,
            values: vec![shape_first_cell(arg, &mut non_numeric)?],
        },
    };
    Ok(ShapeFirst { value, non_numeric })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RegressionInput {
    Forecast,
    Coefficients,
}

/// known_y, then known_x read into the predictors (#REF! when the shapes disagree), then #VALUE!
/// for a value that is not a number.
fn shape_first_inputs(
    args: &[CalcValue],
    input: RegressionInput,
) -> Result<(NumericVector, PreparedPredictorInput), WorksheetErrorCode> {
    if input == RegressionInput::Coefficients {
        if let CoreRef::Array(array) = args[0].core_ref() {
            if array.shape().rows == 1 && array.shape().cols == 1
                && matches!(array.get(0, 0).expect("single-cell array").core_ref(), CoreRef::Error(_))
            {
                return Err(WorksheetErrorCode::Value);
            }
        }
    }
    let x_argument = args
        .get(1)
        .filter(|arg| !matches!(arg.core_ref(), CoreRef::Missing));
    if input == RegressionInput::Forecast && let Some(x) = x_argument {
        let shape = |arg: &CalcValue| {
            arg.as_array().map_or((1, 1), |array| {
                let shape = array.shape();
                (shape.rows, shape.cols)
            })
        };
        let (y_rows, y_cols) = shape(&args[0]);
        let (x_rows, x_cols) = shape(x);
        let rectangular_y = y_rows > 1 && y_cols > 1;
        let rectangular_x = x_rows > 1 && x_cols > 1;
        if (rectangular_y && (y_rows, y_cols) != (x_rows, x_cols))
            || ((y_rows, y_cols) == (1, 1)
                && rectangular_x
                && !matches!(args[0].core_ref(), CoreRef::Missing))
        {
            return Err(WorksheetErrorCode::Ref);
        }
    }
    let known_y = shape_first_matrix(&args[0])?;
    let y = known_y.value;
    let paired_matrix = y.rows > 1 && y.cols > 1;
    let mut known_x_raw = x_argument.map(shape_first_matrix).transpose()?;
    // Equal history rectangles pair cell for cell in row-major order.
    // Omitted known_x uses the ordinary sequence over all of the cells.
    if paired_matrix && let Some(x) = &mut known_x_raw {
        if (x.value.rows, x.value.cols) != (y.rows, y.cols) {
            return Err(WorksheetErrorCode::Ref);
        }
        x.value.rows = x.value.values.len();
        x.value.cols = 1;
    } else if let Some(x) = &mut known_x_raw {
        // The y vector fixes the observation axis. Normalize row-oriented
        // predictors to one observation per row for the fit.
        if y.rows == 1 && y.cols > 1 {
            if x.value.cols != y.cols {
                return Err(WorksheetErrorCode::Ref);
            }
            x.value = x.value.transpose();
        } else if y.rows > 1 && x.value.rows != y.rows {
            return Err(WorksheetErrorCode::Ref);
        }
    }
    let shape = match (y.rows, y.cols) {
        (1, 1) => VectorShape::Scalar,
        (1, cols) => VectorShape::Row(cols),
        (rows, 1) => VectorShape::Column(rows),
        (rows, cols) => VectorShape::Matrix { rows, cols },
    };
    let y = NumericVector { values: y.values, shape };
    let x_non_numeric = known_x_raw.as_ref().is_some_and(|x| x.non_numeric);
    let known_x = predictor_input_from_optional(known_x_raw.map(|x| x.value), &y)?;
    if known_y.non_numeric || x_non_numeric {
        return Err(WorksheetErrorCode::Value);
    }
    Ok((y, known_x))
}

impl NumericMatrix {
    fn cell(&self, row: usize, col: usize) -> f64 {
        self.values[row * self.cols + col]
    }

    fn transpose(&self) -> Self {
        let mut values = vec![0.0; self.rows * self.cols];
        for row in 0..self.rows {
            for col in 0..self.cols {
                values[col * self.rows + row] = self.cell(row, col);
            }
        }
        Self {
            rows: self.cols,
            cols: self.rows,
            values,
        }
    }

    fn row_values(&self, row: usize) -> &[f64] {
        let start = row * self.cols;
        &self.values[start..(start + self.cols)]
    }
}

fn collect_numeric_matrix_from_eval(
    value: &CalcValue,
) -> Result<NumericMatrix, WorksheetErrorCode> {
    match value.core_ref() {
        CoreRef::Number(n) => Ok(NumericMatrix {
            rows: 1,
            cols: 1,
            values: vec![n],
        }),
        CoreRef::Error(code) => Err(code),
        CoreRef::Array(array) => {
            let shape = array.shape();
            let mut values = Vec::with_capacity(shape.rows * shape.cols);
            for cell in array.iter_row_major() {
                values.push(numeric_from_cell(cell)?);
            }
            Ok(NumericMatrix {
                rows: shape.rows,
                cols: shape.cols,
                values,
            })
        }
        CoreRef::Text(_) | CoreRef::Logical(_) | CoreRef::Reference(_) => {
            Err(WorksheetErrorCode::Value)
        }
        _ => Err(WorksheetErrorCode::Value),
    }
}

fn collect_numeric_matrix_arg(arg: &CalcValue) -> Result<NumericMatrix, WorksheetErrorCode> {
    let eval = resolve_arg_eval(arg)?;
    collect_numeric_matrix_from_eval(&eval)
}

fn sequence_predictors(len: usize) -> NumericMatrix {
    NumericMatrix {
        rows: len,
        cols: 1,
        values: (1..=len).map(|n| n as f64).collect(),
    }
}

fn default_output_shape_for_known_y(shape: VectorShape) -> OutputShape {
    match shape {
        VectorShape::Scalar => OutputShape::Scalar,
        VectorShape::Row(cols) => OutputShape::Row(cols),
        VectorShape::Column(rows) => OutputShape::Column(rows),
        VectorShape::Matrix { rows, cols } => OutputShape::Matrix { rows, cols },
    }
}

fn normalize_known_x_matrix(
    raw: NumericMatrix,
    observation_count: usize,
) -> Result<NumericMatrix, WorksheetErrorCode> {
    if raw.rows == observation_count {
        Ok(raw)
    } else if raw.rows == 1 && raw.cols == observation_count {
        Ok(raw.transpose())
    } else if observation_count == 1 && (raw.rows == 1 || raw.cols == 1) {
        // One known y: every cell of a known x row or column is a predictor of that one
        // observation (Excel 16.0.20430, 2026-10-25, dc-6au: `=LINEST(1,{1;2;3})` and
        // `=LINEST(1,{1,2,3})` spill 0,0,0,1, three zero slopes and the intercept;
        // `=TREND(1,{1;2;3})` and `=GROWTH(1,{1;2;3})` are 1, `=LOGEST(1,{1;2;3})` 1,1,1,1;
        // `=TREND(5,{1;2;3},4)` is #REF!). A known x of several rows and columns is #REF!
        // (Excel 16.0.20527, dcztvv_r23 and dcztvv_r24: `=LINEST(0,X)` with X eight rows by two
        // columns or four rows by two).
        Ok(NumericMatrix {
            rows: 1,
            cols: raw.rows * raw.cols,
            values: raw.values,
        })
    } else {
        Err(WorksheetErrorCode::Ref)
    }
}

fn prepare_predictor_input(
    raw: Option<NumericMatrix>,
    known_y: &NumericVector,
    treat_single_predictor_cells_as_observations: bool,
) -> Result<PreparedPredictorInput, WorksheetErrorCode> {
    let observation_count = known_y.values.len();
    match raw {
        None => Ok(PreparedPredictorInput {
            predictors: sequence_predictors(observation_count),
            output_shape: default_output_shape_for_known_y(known_y.shape),
        }),
        Some(matrix) => {
            let normalized = normalize_known_x_matrix(matrix.clone(), observation_count)?;
            if treat_single_predictor_cells_as_observations && normalized.cols == 1 {
                let output_shape = if matrix.rows == 1 && matrix.cols == 1 {
                    OutputShape::Scalar
                } else if matrix.rows == 1 {
                    OutputShape::Row(matrix.cols)
                } else if matrix.cols == 1 {
                    OutputShape::Column(matrix.rows)
                } else {
                    OutputShape::Matrix {
                        rows: matrix.rows,
                        cols: matrix.cols,
                    }
                };
                return Ok(PreparedPredictorInput {
                    predictors: NumericMatrix {
                        rows: matrix.rows * matrix.cols,
                        cols: 1,
                        values: matrix.values,
                    },
                    output_shape,
                });
            }

            if normalized.cols == 1 {
                let output_shape = if matrix.rows == 1 && matrix.cols == 1 {
                    OutputShape::Scalar
                } else if matrix.rows == 1 {
                    OutputShape::Row(matrix.cols)
                } else {
                    OutputShape::Column(matrix.rows)
                };
                return Ok(PreparedPredictorInput {
                    predictors: normalized,
                    output_shape,
                });
            }

            if matrix.cols != normalized.cols && observation_count != 1 {
                return Err(WorksheetErrorCode::Ref);
            }
            let output_shape = if matrix.rows == 1 || observation_count == 1 {
                OutputShape::Scalar
            } else {
                OutputShape::Column(matrix.rows)
            };
            Ok(PreparedPredictorInput {
                predictors: normalized,
                output_shape,
            })
        }
    }
}

fn predictor_input_from_optional(
    raw: Option<NumericMatrix>,
    known_y: &NumericVector,
) -> Result<PreparedPredictorInput, WorksheetErrorCode> {
    prepare_predictor_input(raw, known_y, false)
}

fn prediction_input_from_optional(
    raw: Option<NumericMatrix>,
    known_y: &NumericVector,
    known_x: &PreparedPredictorInput,
    row_oriented: bool,
) -> Result<PreparedPredictorInput, WorksheetErrorCode> {
    match raw {
        None => Ok(PreparedPredictorInput {
            predictors: known_x.predictors.clone(),
            output_shape: default_output_shape_for_known_y(known_y.shape),
        }),
        Some(matrix) => {
            let predictor_count = known_x.predictors.cols;
            if predictor_count == 1 {
                let output_shape = if matrix.rows == 1 && matrix.cols == 1 {
                    OutputShape::Scalar
                } else if matrix.rows == 1 {
                    OutputShape::Row(matrix.cols)
                } else if matrix.cols == 1 {
                    OutputShape::Column(matrix.rows)
                } else {
                    OutputShape::Matrix {
                        rows: matrix.rows,
                        cols: matrix.cols,
                    }
                };
                return Ok(PreparedPredictorInput {
                    predictors: NumericMatrix {
                        rows: matrix.rows * matrix.cols,
                        cols: 1,
                        values: matrix.values,
                    },
                    output_shape,
                });
            }

            let matrix = if row_oriented { matrix.transpose() } else { matrix };
            if matrix.cols != predictor_count {
                return Err(WorksheetErrorCode::Ref);
            }
            let output_shape = if matrix.rows == 1 {
                OutputShape::Scalar
            } else if row_oriented {
                OutputShape::Row(matrix.rows)
            } else {
                OutputShape::Column(matrix.rows)
            };
            Ok(PreparedPredictorInput {
                predictors: matrix,
                output_shape,
            })
        }
    }
}

/// The current least-squares model of LINEST, LOGEST, TREND and GROWTH (dc-j9k,
/// identified on captured coefficients and predictions of one, two and three predictors, with and
/// without a constant; the catalogue oracle's sheet dcj9k_regression holds them):
///
/// 1. With a constant, each predictor column and `y` are centred on their means (each mean the
///    forward sum divided by the count), and a column of ones is stored after the predictors and
///    swapped to the front, so the first predictor moves to the last place (R43); without one,
///    the raw columns are the matrix.
/// 2. Householder QR. Before each predictor column's step, the predictor column with the largest
///    running remainder (5 below, a negative one read as zero) is swapped in; on an exact tie the
///    earlier column in the current order wins (R43: with a constant this is the later one in
///    the given order, which R42 read as a constant-dependent tie). At step
///    `j`, with `x0` the diagonal entry, `rest` the sum of squares below it and `s` the sum of
///    squares from it down (each a forward sum from 0): `alpha = -sqrt(s)` when `x0 > 0` and
///    `+sqrt(s)` otherwise (an exactly zero entry reflects as a negative one, R42),
///    `v = (x0 - alpha, the entries below)`, `vv = rest + v0*v0`, and each later column and `y`
///    become `a - (2*(v.a)/vv)*v`.
/// 3. Back substitution: `b_j = (y_j - d)/alpha_j`, with `d` the reverse-`k` sum of `r_jk*b_k`
///    over the later columns (`rank - 1` down to `j + 1`), all in stored PC64 x87 arithmetic
///    (Carbon cb12 / `dc-j9k.69`).
/// 4. The intercept is `my - b_1*m_1 - b_2*m_2 - ...`, subtracted in predictor order in PC64 x87.
///
/// 5. Collinearity: at a predictor column's step, after the pivot is chosen, its remaining sum
///    of squares is taken as its sum of squares before the steps less the square of each entry
///    above the diagonal (a forward running difference), and the column is left out of the fit
///    (coefficient 0, standard error 0, not counted in the degrees of freedom) when that remainder
///    is below 2^-41 times its sum of squares before the steps, or its entries from the diagonal
///    down are all zero. While other columns are still to come the test reads the reflector's
///    `vv` in place of that remainder (R43). The decomposition then stops: every column not yet
///    stepped is left out with it (R42). A column that was zero before the steps and is stepped before that is kept
///    with a zero coefficient and counts in the degrees of freedom.
///    Identified on the degrees of freedom of 1,121 fits: the 1,025 captured ones and 96 fits
///    asked of Excel 16.0.20527 on 2026-11-07 (dc-j9k.14, n = 3, 4, 8 and 16, with and without a
///    constant, the tested column the larger or the smaller one, the remainder ratio at
///    4.54e-13 to 4.80e-13): every fit dropped by Excel has a remainder ratio below 2^-41 and every
///    kept one a ratio of at least 2^-41, four of them exactly 2^-41. The sum of squares the
///    columns are reflected with, recomputed from the entries, places those 96 fits on the wrong
///    side 11 times, so the test reads the running difference, not that sum.
/// R42 (Excel 16.0.20527, 2026-11-09): the retained R41 cohort (15,150 LINEST and 12,140 LOGEST
/// selections) and R38's 216 rank-deficient designs gain 937/876 and 264/258 selections with
/// none lost; a fresh set of 1,344 selections per function, frozen before asking, then matched
/// 1,303 for each (the previous program 903 and 932).
/// R43 (Excel 16.0.20527, 2026-11-09): the ones column's place, the clipped tie-to-the-earlier
/// pivot and the `vv` test, found on the zero-column layouts R42 left open and on threshold maps
/// (n = 4 to 32: the switch is 2^-41 of the remainder for the last column and 2^-41 of `vv`
/// when a zero or tiny column is still to come). A fresh set frozen before asking (2,439
/// selections per function, 2,414 answered) matched 2,413 and 2,414 of the answered (the
/// previous program 2,225 and 2,255). Private QR and residual arithmetic remain incomplete.
/// The collinearity test's bound on a column's remaining over its initial sum of squares (5 above).
const COLLINEAR_REMAINDER: f64 = 1.0 / (1u64 << 41) as f64;

/// Excel 20625 substitutes zero for overflow at these stored regression intermediates.
fn regression_intermediate(value: f64) -> f64 {
    if value.is_infinite() { 0.0 } else { value }
}

fn least_squares(
    columns: &[Vec<f64>],
    y: &[f64],
    use_const: bool,
) -> Result<RegressionFit, WorksheetErrorCode> {
    use excel_numeric::{
        excel_sqrt_staged, excel_x87_add as add, excel_x87_div as div, excel_x87_mul as mul,
        excel_x87_sub as sub,
    };

    let n = y.len();
    let mean = |values: &[f64]| div(values.iter().fold(0.0, |sum, v| add(sum, *v)), n as f64);
    let means: Vec<f64> = columns.iter().map(|column| mean(column)).collect();
    let y_mean = if use_const { mean(y) } else { 0.0 };
    // The matrix's columns, each with the predictor it holds (None for the ones column).
    let mut matrix: Vec<(Option<usize>, Vec<f64>)> = Vec::with_capacity(columns.len() + 1);
    for (index, column) in columns.iter().enumerate() {
        let values = if use_const {
            column.iter().map(|v| sub(*v, means[index])).collect()
        } else {
            column.clone()
        };
        matrix.push((Some(index), values));
    }
    if use_const {
        // The ones column is stored after the predictors and swapped to the front, so the
        // first predictor moves to the last place (R43: zero-column layouts beside a
        // collinear pair, and the exact ties R42 read as "the later column wins").
        matrix.push((None, vec![1.0; n]));
        let last = matrix.len() - 1;
        matrix.swap(0, last);
    }
    let mut target: Vec<f64> = if use_const {
        y.iter().map(|v| sub(*v, y_mean)).collect()
    } else {
        y.to_vec()
    };
    let centred_y = target.clone();
    let p = matrix.len();
    if n == 1 && use_const {
        // One observation with a constant: every centred predictor is 0, so each coefficient is
        // 0 and the intercept is the one y (Excel 16.0.20430, 2026-10-25, dc-6au:
        // `=LINEST(1,{1;2;3})` 0,0,0,1, `=LINEST(5,2)` 0,5, `=TREND(1,{1;2;3})` 1).
        return Ok(RegressionFit {
            model: LinearRegressionModel {
                coefficients: vec![0.0; columns.len()],
                intercept: y_mean,
            },
            order: Vec::new(),
            r: Vec::new(),
            qty: Vec::new(),
            means,
            y: Vec::new(),
            observations: n,
        });
    }
    // Uniform stored PC64 x87 sum of squares (Carbon cb12 / dc-j9k.69, Excel 16.0.20527).
    let squares_from = |values: &[f64], from: usize| {
        values[from..]
            .iter()
            .fold(0.0, |sum, v| add(sum, mul(*v, *v)))
    };
    // Each column's sum of squares before the steps, the scale its remainder is judged against.
    let initial: Vec<f64> = matrix
        .iter()
        .map(|(_, column)| squares_from(column, 0))
        .collect();
    let mut scale: Vec<(Option<usize>, f64)> = matrix
        .iter()
        .map(|(predictor, _)| *predictor)
        .zip(initial)
        .collect();
    let mut diagonal = vec![0.0; p];
    // Pivot by the remaining sum of squares, then apply the collinearity test (5 above).
    let mut j = 0;
    while j < matrix.len() {
        if j >= n {
            matrix.truncate(j);
            scale.truncate(j);
            break;
        }
        if matrix[j].0.is_some() {
            // The pivot is the column with the largest running remainder, the same quantity
            // the collinearity test reads (R42 fresh designs where a small independent column
            // competes with a collinear column's remainder). An exact tie goes to the later
            // column with a constant and to the earlier one without.
            let remainder = |k: usize| {
                matrix[k].1[..j]
                    .iter()
                    .fold(scale[k].1, |rest, r| {
                        excel_numeric::excel_x87_sub(rest, excel_numeric::excel_x87_mul(*r, *r))
                    })
                    .max(0.0)
            };
            let mut best = j;
            let mut best_remainder = remainder(j);
            for k in (j + 1)..matrix.len() {
                let candidate = remainder(k);
                if candidate > best_remainder {
                    best_remainder = candidate;
                    best = k;
                }
            }
            matrix.swap(j, best);
            scale.swap(j, best);
        }
        let x0 = matrix[j].1[j];
        let rest = squares_from(&matrix[j].1, j + 1);
        let squares = squares_from(&matrix[j].1, j);
        let collinear = scale[j].1 > 0.0 && matrix[j].0.is_some() && {
            let before = scale[j].1;
            if j + 1 == matrix.len() {
                let remainder = matrix[j].1[..j]
                    .iter()
                    .fold(before, |rest, r| sub(rest, mul(*r, *r)));
                remainder < mul(before, COLLINEAR_REMAINDER)
            } else {
                // With columns still to come the test reads the reflector's normaliser
                // `vv = rest + (x0 - alpha)^2` (R43: near-collinear pairs beside a zero or tiny
                // third column switch at vv = 2^-41 times the initial sum of squares, n = 4 to
                // 32, with and without a constant; the last column switches on its remainder).
                let norm = excel_sqrt_staged(squares);
                let alpha = if x0 > 0.0 { -norm } else { norm };
                let v0 = sub(x0, alpha);
                let vv = add(rest, mul(v0, v0));
                vv < mul(before, COLLINEAR_REMAINDER)
            }
        };
        if scale[j].1 > 0.0 && matrix[j].0.is_some() && (squares == 0.0 || collinear) {
            // Rank-revealing stop: the pivot is the largest remaining column, so once it fails
            // the test every column not yet stepped is left out with it, a column that was
            // zero before the steps included (R42, dc-j9k.14).
            matrix.truncate(j);
            scale.truncate(j);
            break;
        }
        if squares == 0.0 {
            diagonal[j] = 0.0;
            j += 1;
            continue;
        }
        let norm = excel_sqrt_staged(squares);
        // A zero diagonal entry reflects as a negative one (R42: mirrored designs whose entry
        // is exactly zero after the ones column's step).
        let alpha = if x0 > 0.0 { -norm } else { norm };
        let mut v = matrix[j].1[j..].to_vec();
        // Householder's I - 2vv'/(v'v), with every reflector operation
        // stored as RN53(RN64(operation)). Use the shared native/software
        // arithmetic, including each dot-product term and vector update.
        v[0] = excel_numeric::excel_x87_sub(x0, alpha);
        let vv = excel_numeric::excel_x87_add(rest, excel_numeric::excel_x87_mul(v[0], v[0]));
        // Excel 20625 replaces an overflowing dot-product term, running sum,
        // or doubled sum with zero before continuing the reflector.
        let reflect = |a: &mut [f64]| {
            let t = v.iter().zip(a[j..].iter()).fold(0.0, |sum, (vi, ai)| {
                regression_intermediate(add(sum, regression_intermediate(mul(*vi, *ai))))
            });
            let f = div(regression_intermediate(mul(2.0, t)), vv);
            for (ai, vi) in a[j..].iter_mut().zip(v.iter()) {
                *ai = excel_numeric::excel_x87_sub(*ai, excel_numeric::excel_x87_mul(f, *vi));
            }
        };
        for (_, column) in matrix.iter_mut().skip(j + 1) {
            reflect(column);
        }
        reflect(&mut target);
        diagonal[j] = alpha;
        j += 1;
    }
    let rank = matrix.len();
    let mut solution = vec![0.0; rank];
    for j in (0..rank).rev() {
        let d = ((j + 1)..rank)
            .rev()
            .fold(0.0, |sum, k| {
                add(sum, regression_intermediate(mul(matrix[k].1[j], solution[k])))
            });
        solution[j] = if diagonal[j] == 0.0 {
            0.0
        } else {
            regression_intermediate(div(sub(target[j], d), diagonal[j]))
        };
    }
    let mut coefficients = vec![0.0; columns.len()];
    for (j, (predictor, _)) in matrix.iter().take(rank).enumerate() {
        if let Some(index) = predictor {
            coefficients[*index] = solution[j];
        }
    }
    let intercept = if use_const {
        coefficients
            .iter()
            .zip(means.iter())
            .fold(y_mean, |a, (b, m)| sub(a, regression_intermediate(mul(*b, *m))))
    } else {
        0.0
    };
    let r = (0..rank)
        .map(|j| {
            (0..rank)
                .map(|k| {
                    if k < j {
                        0.0
                    } else if k == j {
                        diagonal[j]
                    } else {
                        matrix[k].1[j]
                    }
                })
                .collect()
        })
        .collect();
    Ok(RegressionFit {
        model: LinearRegressionModel {
            coefficients,
            intercept,
        },
        order: matrix.iter().map(|(predictor, _)| *predictor).collect(),
        r,
        qty: target,
        means,
        y: centred_y,
        observations: n,
    })
}

fn linear_model_from_data(
    known_y: &NumericVector,
    known_x: &PreparedPredictorInput,
    use_const: bool,
) -> Result<LinearRegressionModel, WorksheetErrorCode> {
    linear_fit_from_data(known_y, known_x, use_const).map(|fit| fit.model)
}

fn linear_fit_from_data(
    known_y: &NumericVector,
    known_x: &PreparedPredictorInput,
    use_const: bool,
) -> Result<RegressionFit, WorksheetErrorCode> {
    let observation_count = known_y.values.len();
    if known_x.predictors.rows != observation_count {
        return Err(WorksheetErrorCode::Ref);
    }
    let columns: Vec<Vec<f64>> = (0..known_x.predictors.cols)
        .map(|col| {
            (0..observation_count)
                .map(|row| known_x.predictors.cell(row, col))
                .collect()
        })
        .collect();
    least_squares(&columns, &known_y.values, use_const)
}

/// A TREND prediction: `b_1*x_1 + b_2*x_2 + ...`, added in predictor order, then the intercept,
/// using stored PC64 x87 arithmetic (Carbon cb12 / dc-j9k.69).
fn linear_prediction(model: &LinearRegressionModel, x: &[f64]) -> f64 {
    let mut terms = model
        .coefficients
        .iter()
        .zip(x.iter())
        .map(|(b, x)| excel_numeric::excel_x87_mul(*b, *x));
    let first = terms.next().unwrap_or(0.0);
    excel_numeric::excel_x87_add(
        terms.fold(first, |sum, term| excel_numeric::excel_x87_add(sum, term)),
        model.intercept,
    )
}

/// A GROWTH prediction: the product of `EXP(b_j*x_j)` in predictor order, from 1, then
/// `EXP(intercept)` times it, using stored PC64 x87 arithmetic (Carbon cb12 / dc-j9k.69).
fn exponential_prediction(model: &LinearRegressionModel, x: &[f64]) -> f64 {
    let mul = excel_numeric::excel_x87_mul;
    let product = model
        .coefficients
        .iter()
        .zip(x.iter())
        .fold(1.0, |product, (b, x)| {
            mul(product, excel_numeric::excel_exp(mul(*b, *x)))
        });
    mul(excel_numeric::excel_exp(model.intercept), product)
}

/// The fit of LOGEST and GROWTH: the least-squares fit of the worksheet LN of each y (#NUM! for
/// a y that is not positive).
fn exponential_model_from_data(
    known_y: &NumericVector,
    known_x: &PreparedPredictorInput,
    use_const: bool,
) -> Result<LinearRegressionModel, WorksheetErrorCode> {
    exponential_fit_from_data(known_y, known_x, use_const).map(|fit| fit.model)
}

fn exponential_fit_from_data(
    known_y: &NumericVector,
    known_x: &PreparedPredictorInput,
    use_const: bool,
) -> Result<RegressionFit, WorksheetErrorCode> {
    let mut log_y = Vec::with_capacity(known_y.values.len());
    for value in &known_y.values {
        if *value <= 0.0 || !value.is_finite() {
            return Err(WorksheetErrorCode::Num);
        }
        log_y.push(excel_numeric::excel_log(*value));
    }
    let logged = NumericVector {
        values: log_y,
        shape: known_y.shape,
    };
    linear_fit_from_data(&logged, known_x, use_const)
}

fn eval_value_from_output_shape(
    values: &[f64],
    shape: OutputShape,
) -> Result<CalcValue, WorksheetErrorCode> {
    match shape {
        OutputShape::Scalar => Ok(CalcValue::number(values[0])),
        OutputShape::Row(cols) => CalcArray::from_rows(vec![
            values
                .iter()
                .copied()
                .take(cols)
                .map(CalcValue::number)
                .collect::<Vec<_>>(),
        ])
        .map(CalcValue::array)
        .ok_or(WorksheetErrorCode::Value),
        OutputShape::Column(rows) => CalcArray::from_rows(
            values
                .iter()
                .copied()
                .take(rows)
                .map(|value| vec![CalcValue::number(value)])
                .collect::<Vec<_>>(),
        )
        .map(CalcValue::array)
        .ok_or(WorksheetErrorCode::Value),
        OutputShape::Matrix { rows, cols } => {
            let row_major = values
                .chunks(cols)
                .take(rows)
                .map(|chunk| chunk.iter().copied().map(CalcValue::number).collect())
                .collect::<Vec<Vec<CalcValue>>>();
            CalcArray::from_rows(row_major)
                .map(CalcValue::array)
                .ok_or(WorksheetErrorCode::Value)
        }
    }
}

fn row_array(values: &[f64]) -> CalcValue {
    CalcValue::array(
        CalcArray::from_rows(vec![
            values.iter().copied().map(CalcValue::number).collect(),
        ])
        .expect("non-empty coefficient row"),
    )
}

fn trend_kernel(
    known_y: &NumericVector,
    known_x: &PreparedPredictorInput,
    new_x: &PreparedPredictorInput,
    use_const: bool,
) -> Result<CalcValue, WorksheetErrorCode> {
    let model = linear_model_from_data(known_y, known_x, use_const)?;
    if new_x.predictors.cols != model.coefficients.len() {
        return Err(WorksheetErrorCode::Ref);
    }
    let predicted = (0..new_x.predictors.rows)
        .map(|row_idx| linear_prediction(&model, new_x.predictors.row_values(row_idx)))
        .collect::<Vec<_>>();
    eval_value_from_output_shape(&predicted, new_x.output_shape)
}

fn growth_kernel(
    known_y: &NumericVector,
    known_x: &PreparedPredictorInput,
    new_x: &PreparedPredictorInput,
    use_const: bool,
) -> Result<CalcValue, WorksheetErrorCode> {
    let model = exponential_model_from_data(known_y, known_x, use_const)?;
    if new_x.predictors.cols != model.coefficients.len() {
        return Err(WorksheetErrorCode::Ref);
    }
    let predicted = (0..new_x.predictors.rows)
        .map(|row_idx| exponential_prediction(&model, new_x.predictors.row_values(row_idx)))
        .collect::<Vec<_>>();
    eval_value_from_output_shape(&predicted, new_x.output_shape)
}

/// The statistics of a fit to one observation with a constant: the first row `first` (the
/// coefficients, then the constant), every standard error 0, R squared 1 and its standard error
/// 0, F #NUM! with 0 degrees of freedom, and both sums of squares 0, the cells beyond the second
/// column of rows 3 to 5 #N/A (Excel 16.0.20430, 2026-10-25, dc-6au: `=LINEST(5,{1;2;3},TRUE,TRUE)`
/// spills 0,0,0,5 / 0,0,0,0 / 1,0,#N/A,#N/A / #NUM!,0,#N/A,#N/A / 0,0,#N/A,#N/A, `=LINEST(5,2,TRUE,TRUE)`
/// 0,5 / 0,0 / 1,0 / #NUM!,0 / 0,0, and LOGEST's the same below its first row).
fn one_observation_statistics(first: Vec<f64>) -> CalcValue {
    let cols = first.len();
    let pad = |head: Vec<CalcValue>| {
        let mut row = head;
        while row.len() < cols {
            row.push(CalcValue::error(WorksheetErrorCode::NA));
        }
        row
    };
    let n = CalcValue::number;
    CalcValue::array(
        CalcArray::from_rows(vec![
            first.into_iter().map(n).collect(),
            vec![n(0.0); cols],
            pad(vec![n(1.0), n(0.0)]),
            pad(vec![CalcValue::error(WorksheetErrorCode::Num), n(0.0)]),
            pad(vec![n(0.0), n(0.0)]),
        ])
        .expect("five rows of one length"),
    )
}

/// The statistics block of LINEST and LOGEST below the first row `first` (the coefficients in
/// reverse predictor order, then the constant), from the fit's QR factors: the standard errors
/// of the coefficients and of the constant (#N/A without one), R squared and the standard error
/// of y, F and the degrees of freedom, and the regression and residual sums of squares. LOGEST's
/// statistics are those of the fit to ln y.
fn statistics_block(fit: &RegressionFit, use_const: bool, first: Vec<f64>) -> CalcValue {
    use excel_numeric::{
        excel_sqrt_staged, excel_x87_add as add, excel_x87_div as div, excel_x87_mul as mul,
        excel_x87_sub as sub,
    };

    if fit.order.is_empty() {
        return one_observation_statistics(first);
    }
    let n = fit.observations;
    let p = fit.order.len();
    let predictors = p - usize::from(use_const);
    // R41 coordinate predictors leave the response tail unchanged, exposing
    // the residual square and accumulation without an approximate QR factor.
    // Both SSresid and SStotal use forward RN53(RN64) products and additions.
    let squares = |values: &[f64]| {
        values.iter().fold(0.0, |sum, v| {
            regression_intermediate(add(sum, regression_intermediate(mul(*v, *v))))
        })
    };
    let ss_resid = squares(&fit.qty[p..]);
    let ss_total = squares(&fit.y);
    let ss_reg = sub(ss_total, ss_resid);
    let df = (n - p) as f64;
    // With no degrees of freedom left Excel gives R squared 1, every standard error 0 and F
    // #NUM! (an exact fit of as many columns as observations). When ss_total == 0.0 with n > p
    // (constant y with const=TRUE, or y == 0 / ln(y) == 0 with const=FALSE), Excel also returns
    // R squared 1.0 and F #NUM! (Carbon cb12 / dc-j9k.69).
    let exact = n == p;
    let r2 = if exact || ss_total == 0.0 {
        1.0
    } else {
        div(ss_reg, ss_total)
    };
    let se_y = if exact {
        0.0
    } else {
        excel_sqrt_staged(div(ss_resid, df))
    };
    let f = if exact {
        f64::NAN
    } else {
        div(div(ss_reg, predictors as f64), div(ss_resid, df))
    };
    // The inverse of the triangular factor over the columns with a nonzero diagonal (a column
    // kept as zero comes last and has standard error 0), column by column.
    let nz = (0..p).take_while(|j| fit.r[*j][*j] != 0.0).count();
    let mut inverse = vec![vec![0.0; nz]; nz];
    for j in 0..nz {
        inverse[j][j] = div(1.0, fit.r[j][j]);
        for i in (0..j).rev() {
            let sum = ((i + 1)..=j).fold(0.0, |sum, k| add(sum, mul(fit.r[i][k], inverse[k][j])));
            inverse[i][j] = div(-sum, fit.r[i][i]);
        }
    }
    // (R'R)^-1 = R^-1 R^-T. The reported coefficient covariance multiplies
    // each entry by the residual sum of squares and then divides by df.
    let scaled_covariance = |a: usize, b: usize| {
        let entry = (0..nz).fold(0.0, |sum, k| add(sum, mul(inverse[a][k], inverse[b][k])));
        div(mul(ss_resid, entry), df)
    };
    let mut se = vec![0.0; fit.model.coefficients.len()];
    for (position, predictor) in fit.order.iter().enumerate().take(nz) {
        if let Some(index) = predictor {
            if !exact {
                se[*index] = excel_sqrt_staged(scaled_covariance(position, position));
            }
        }
    }
    let n_a = || CalcValue::error(WorksheetErrorCode::NA);
    let number = |v: f64| {
        if v.is_finite() {
            CalcValue::number(v)
        } else {
            CalcValue::error(WorksheetErrorCode::Num)
        }
    };
    let cols = first.len();
    let mut second: Vec<CalcValue> = se.iter().rev().map(|v| number(*v)).collect();
    if use_const {
        let se_b = if exact {
            0.0
        } else {
            // The constant is my - sum(b_j m_j). Its variance starts with the
            // constant's diagonal entry, adds active predictor diagonals in
            // unpermuted 0..k order, then row-major active predictor pairs
            // (p_i, p_j) for i < j, and finally the (p_i, const) pairs
            // (Carbon cb12 / dc-j9k.69, Excel 16.0.20527).
            let weights: Vec<f64> = fit
                .order
                .iter()
                .take(nz)
                .map(|predictor| match predictor {
                    None => 1.0,
                    Some(index) => -fit.means[*index],
                })
                .collect();
            let active_preds: Vec<usize> = (0..fit.model.coefficients.len())
                .filter_map(|pred| fit.order.iter().take(nz).position(|p| *p == Some(pred)))
                .collect();
            let mut variance = active_preds.iter().fold(scaled_covariance(0, 0), |sum, &pos| {
                let squared_weight = mul(weights[pos], weights[pos]);
                add(sum, mul(squared_weight, scaled_covariance(pos, pos)))
            });
            for (idx_a, &a) in active_preds.iter().enumerate() {
                for &b in &active_preds[(idx_a + 1)..] {
                    let weight = mul(2.0, mul(weights[a], weights[b]));
                    variance = add(variance, mul(weight, scaled_covariance(a, b)));
                }
            }
            for &a in &active_preds {
                let weight = mul(2.0, mul(weights[a], weights[0]));
                variance = add(variance, mul(weight, scaled_covariance(a, 0)));
            }
            excel_sqrt_staged(variance)
        };
        second.push(number(se_b));
    } else {
        second.push(n_a());
    }
    let pad = |head: Vec<CalcValue>| {
        let mut row = head;
        while row.len() < cols {
            row.push(n_a());
        }
        row
    };
    CalcValue::array(
        CalcArray::from_rows(vec![
            first.into_iter().map(CalcValue::number).collect(),
            second,
            pad(vec![number(r2), number(se_y)]),
            pad(vec![number(f), number(df)]),
            pad(vec![number(ss_reg), number(ss_resid)]),
        ])
        .expect("five rows of one length"),
    )
}

fn linest_kernel(
    known_y: &NumericVector,
    known_x: &PreparedPredictorInput,
    use_const: bool,
    stats: bool,
) -> Result<CalcValue, WorksheetErrorCode> {
    let fit = linear_fit_from_data(known_y, known_x, use_const)?;
    let model = &fit.model;
    let mut row = model.coefficients.iter().rev().copied().collect::<Vec<_>>();
    row.push(if use_const { model.intercept } else { 0.0 });
    if stats {
        return Ok(statistics_block(&fit, use_const, row));
    }
    Ok(row_array(&row))
}

fn logest_kernel(
    known_y: &NumericVector,
    known_x: &PreparedPredictorInput,
    use_const: bool,
    stats: bool,
) -> Result<CalcValue, WorksheetErrorCode> {
    let fit = exponential_fit_from_data(known_y, known_x, use_const)?;
    let model = &fit.model;
    let mut row = model
        .coefficients
        .iter()
        .rev()
        .map(|b| excel_numeric::excel_exp(*b))
        .collect::<Vec<_>>();
    row.push(if use_const {
        excel_numeric::excel_exp(model.intercept)
    } else {
        1.0
    });
    if stats {
        return Ok(statistics_block(&fit, use_const, row));
    }
    Ok(row_array(&row))
}

/// Worksheet `FORECAST`/`FORECAST.LINEAR` — bit-exact to 64-bit Excel on
/// `x86_64` (W109 sweep, 65/65 discovery + adversarial rows incl. 1e12
/// common offsets, n=2, near-constant x, n=12 irregular sets):
///
/// ```text
/// mx  = (Σx)/n ; my = (Σy)/n            (forward sums, plain double)
/// num = Σ (x-mx)·(y-my) ; den = Σ (x-mx)²   (single fused loop)
/// b   = num/den ; a = my - b·mx
/// FORECAST = a + b·xp                    (intercept form, NOT point-slope)
/// ```
///
/// Excel does NOT route FORECAST through the LINEST least-squares pipeline
/// (unlike TREND, which does and therefore publishes different bits).
fn forecast_pair_kernel(
    x: f64,
    known_y: &NumericVector,
    known_x: &NumericVector,
) -> Result<CalcValue, WorksheetErrorCode> {
    if known_x.values.len() != known_y.values.len() {
        return Err(WorksheetErrorCode::NA);
    }
    let n = known_x.values.len();
    if n == 0 {
        return Err(WorksheetErrorCode::Div0);
    }
    let mut sum_x = 0.0;
    for v in &known_x.values {
        sum_x += *v;
    }
    let mut sum_y = 0.0;
    for v in &known_y.values {
        sum_y += *v;
    }
    let nf = n as f64;
    let mean_x = sum_x / nf;
    let mean_y = sum_y / nf;
    let mut num = 0.0;
    let mut den = 0.0;
    for (xv, yv) in known_x.values.iter().zip(known_y.values.iter()) {
        let dx = *xv - mean_x;
        num += dx * (*yv - mean_y);
        den += dx * dx;
    }
    if den == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    let b = num / den;
    let a = mean_y - b * mean_x;
    Ok(CalcValue::number(a + b * x))
}

pub(crate) fn eval_trend_surface(args: &[CalcValue]) -> Result<CalcValue, WorksheetErrorCode> {
    evaluate_forecast_series(args, trend_kernel)
}

pub(crate) fn eval_growth_surface(args: &[CalcValue]) -> Result<CalcValue, WorksheetErrorCode> {
    evaluate_forecast_series(args, growth_kernel)
}

fn evaluate_forecast_series(
    args: &[CalcValue],
    evaluate: fn(&NumericVector, &PreparedPredictorInput, &PreparedPredictorInput, bool)
        -> Result<CalcValue, WorksheetErrorCode>,
) -> Result<CalcValue, WorksheetErrorCode> {
    let (known_y, known_x) = shape_first_inputs(args, RegressionInput::Forecast)?;
    let new_x_raw = args
        .get(2)
        .filter(|arg| !matches!(arg.core_ref(), CoreRef::Missing))
        .map(|arg| {
            if matches!(known_y.shape, VectorShape::Matrix { .. }) {
                let x = shape_first_matrix(arg).map_err(|_| WorksheetErrorCode::Value)?;
                if x.non_numeric {
                    return Err(WorksheetErrorCode::Value);
                }
                Ok(x.value)
            } else {
                collect_numeric_matrix_arg(arg)
            }
        })
        .transpose()?;
    // One observation has no axis of its own. A column of predictors puts
    // observations in columns in new_x, just like a row-shaped known_y.
    let column_of_predictors = known_y.shape == VectorShape::Scalar
        && args.get(1).and_then(CalcValue::as_array).is_some_and(|array| {
            array.shape().rows > 1 && array.shape().cols == 1
        });
    let row_oriented = matches!(known_y.shape, VectorShape::Row(_)) || column_of_predictors;
    let new_x = prediction_input_from_optional(new_x_raw, &known_y, &known_x, row_oriented)?;
    let use_const = bool_flag_from_eval_or_default(optional_arg_value(args.get(3))?, true)?;
    evaluate(&known_y, &known_x, &new_x, use_const)
}

pub(crate) fn eval_forecast_surface(args: &[CalcValue]) -> Result<CalcValue, WorksheetErrorCode> {
    let x = numeric_scalar_from_eval(&resolve_arg_eval(&args[0])?)?;
    let known_y = collect_numeric_vector_arg(&args[1])?;
    let known_x = collect_numeric_vector_arg(&args[2])?;
    forecast_pair_kernel(x, &known_y, &known_x)
}

pub(crate) fn eval_forecast_linear_surface(
    args: &[CalcValue],
) -> Result<CalcValue, WorksheetErrorCode> {
    let x = numeric_scalar_from_eval(&resolve_arg_eval(&args[0])?)?;
    let known_y = collect_numeric_vector_arg(&args[1])?;
    let known_x = collect_numeric_vector_arg(&args[2])?;
    forecast_pair_kernel(x, &known_y, &known_x)
}

pub(crate) fn eval_linest_surface(args: &[CalcValue]) -> Result<CalcValue, WorksheetErrorCode> {
    let (known_y, known_x) = shape_first_inputs(args, RegressionInput::Coefficients)?;
    let use_const = linest_flag_from_eval_or_default(optional_arg_value(args.get(2))?, true)?;
    let stats = linest_flag_from_eval_or_default(optional_arg_value(args.get(3))?, false)?;
    linest_kernel(&known_y, &known_x, use_const, stats)
}

pub(crate) fn eval_logest_surface(args: &[CalcValue]) -> Result<CalcValue, WorksheetErrorCode> {
    let (known_y, known_x) = shape_first_inputs(args, RegressionInput::Coefficients)?;
    let use_const = linest_flag_from_eval_or_default(optional_arg_value(args.get(2))?, true)?;
    let stats = linest_flag_from_eval_or_default(optional_arg_value(args.get(3))?, false)?;
    logest_kernel(&known_y, &known_x, use_const, stats)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::ArrayShape;
    use std::collections::BTreeMap;

    fn nv(values: Vec<f64>) -> NumericVector {
        let shape = VectorShape::Column(values.len());
        NumericVector { values, shape }
    }

    /// W109 sweep pins — live Excel 16.0 build 20131 (65/65 discovery +
    /// adversarial rows replay bit-exactly through this kernel; corpora at
    /// smart-fuzzer/work/w109/G3-03-answers-forecast.json / -fc-adv.json).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn forecast_matches_live_excel_pinned_witnesses() {
        let cases: [(f64, Vec<f64>, Vec<f64>, u64); 3] = [
            (
                1.0,
                vec![10.5, -3.25, 7.75, 22.0, 0.125],
                vec![0.5, 1.75, 2.25, 5.0, 3.125],
                0x4006f2ee1992538a,
            ),
            (
                0.5,
                vec![1e12 + 1.0, 1e12 + 2.0, 1e12 + 2.5, 1e12 + 4.75],
                vec![1e9 + 1.0, 1e9 + 2.0, 1e9 + 3.0, 1e9 + 4.0],
                0x426d11d37f8806cd,
            ),
            (
                0.5,
                vec![
                    3.1, -1.4, 15.9, 2.65, 35.8, -9.79, 32.3, 8.46, 26.4, 33.8, 3.27, 9.5,
                ],
                vec![
                    2.71, 8.28, 1.82, 8.45, 9.04, 5.23, 5.36, 2.87, 4.71, 3.52, 6.6, 2.87,
                ],
                0x402beba52e78bf17,
            ),
        ];
        for (x, ys, xs, want) in cases {
            let got = forecast_pair_kernel(x, &nv(ys), &nv(xs)).expect("numeric");
            let CoreRef::Number(v) = got.core_ref() else {
                panic!("expected number");
            };
            assert_eq!(v.to_bits(), want, "x={x}");
        }
    }

    fn num(n: f64) -> CalcValue {
        CalcValue::number(n)
    }

    fn bool_arg(flag: bool) -> CalcValue {
        CalcValue::logical(flag)
    }

    fn col(values: &[f64]) -> CalcValue {
        CalcValue::array(
            CalcArray::from_rows(
                values
                    .iter()
                    .copied()
                    .map(|n| vec![CalcValue::number(n)])
                    .collect(),
            )
            .unwrap(),
        )
    }

    fn row(values: &[f64]) -> CalcValue {
        CalcValue::array(
            CalcArray::from_rows(vec![
                values.iter().copied().map(CalcValue::number).collect(),
            ])
            .unwrap(),
        )
    }

    fn matrix(rows: &[&[f64]]) -> CalcValue {
        CalcValue::array(
            CalcArray::from_rows(
                rows.iter()
                    .map(|row| row.iter().copied().map(CalcValue::number).collect())
                    .collect(),
            )
            .unwrap(),
        )
    }

    fn assert_close(left: f64, right: f64) {
        assert!((left - right).abs() < 1e-9, "left={left}, right={right}");
    }

    fn expect_number(value: CalcValue) -> f64 {
        match value.core_ref() {
            CoreRef::Number(n) => n,
            other => panic!("expected scalar, got {other:?}"),
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn coordinate_predictor_reads_stored_pc64_residual_squares_and_forward_sums() {
        // Original question-service observations, Excel 16.0 build 20527,
        // R41 residual-isolation and residual-sums. With X=e0 and no
        // constant the reflector leaves every residual coordinate unchanged.
        // These cases distinguish product precision, addition precision and
        // forward order without conditioning on an approximate QR factor.
        for (y, residual, regression) in [
            (vec![0.0, 1.8975095069898755, 0.0], 0x400c_cde9_22fb_5e3c, 0),
            (
                vec![
                    2.750390944694963e-6,
                    0.0032663285051918933,
                    72054.06702097756,
                    0.005624079914020067,
                ],
                0x41f3_5746_81e4_3792,
                0,
            ),
            (
                vec![
                    50.85618900492427,
                    665182.4723075805,
                    501243.51410530537,
                    1020.9925815877413,
                ],
                0x4264_3092_1d48_76cb,
                0x40a4_34b4_4000_0000,
            ),
            (
                vec![
                    1.3874764646265145,
                    1302.388046265071,
                    6.7993072824179475,
                    1.8687740262216257e-5,
                ],
                0x4139_e204_da87_bc76,
                0x3ffe_cd2c_2840_0000,
            ),
        ] {
            let mut x = vec![0.0; y.len()];
            x[0] = 1.0;
            let got =
                eval_linest_surface(&[col(&y), col(&x), bool_arg(false), bool_arg(true)]).unwrap();
            let CoreRef::Array(array) = got.core_ref() else {
                panic!("LINEST statistics array")
            };
            assert_eq!(
                numeric_from_cell(array.get(4, 1).unwrap())
                    .unwrap()
                    .to_bits(),
                residual
            );
            assert_eq!(
                numeric_from_cell(array.get(4, 0).unwrap())
                    .unwrap()
                    .to_bits(),
                regression
            );
        }
    }

    fn expect_row_array(value: CalcValue) -> Vec<f64> {
        match value.core_ref() {
            CoreRef::Array(array) => {
                assert_eq!(
                    array.shape(),
                    ArrayShape {
                        rows: 1,
                        cols: array.shape().cols
                    }
                );
                array
                    .iter_row_major()
                    .map(|cell| match cell.core_ref() {
                        CoreRef::Number(n) => n,
                        other => panic!("unexpected cell: {other:?}"),
                    })
                    .collect()
            }
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn trend_defaults_known_x_to_sequence_and_preserves_row_shape() {
        let got = eval_trend_surface(&[row(&[2.0, 4.0, 6.0])]).unwrap();
        let values = expect_row_array(got);
        assert_eq!(values.len(), 3);
        assert_close(values[0], 2.0);
        assert_close(values[1], 4.0);
        assert_close(values[2], 6.0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn trend_projects_column_vector() {
        let got = eval_trend_surface(&[
            col(&[2.0, 4.0, 6.0]),
            col(&[1.0, 2.0, 3.0]),
            col(&[4.0, 5.0]),
        ])
        .unwrap();
        match got.core_ref() {
            CoreRef::Array(array) => {
                assert_eq!(array.shape(), ArrayShape { rows: 2, cols: 1 });
                assert_close(numeric_from_cell(array.get(0, 0).unwrap()).unwrap(), 8.0);
                assert_close(numeric_from_cell(array.get(1, 0).unwrap()).unwrap(), 10.0);
            }
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn growth_projects_exponential_sequence() {
        let got = eval_growth_surface(&[
            col(&[2.0, 4.0, 8.0]),
            CalcValue::missing(),
            col(&[4.0, 5.0]),
        ])
        .unwrap();
        match got.core_ref() {
            CoreRef::Array(array) => {
                assert_close(numeric_from_cell(array.get(0, 0).unwrap()).unwrap(), 16.0);
                assert_close(numeric_from_cell(array.get(1, 0).unwrap()).unwrap(), 32.0);
            }
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn growth_projects_multivariate_rows_to_column_result() {
        let got = eval_growth_surface(&[
            col(&[9.0, 18.0, 27.0, 54.0, 72.0]),
            matrix(&[
                &[1.0, 1.0],
                &[2.0, 1.0],
                &[1.0, 2.0],
                &[2.0, 2.0],
                &[3.0, 2.0],
            ]),
            matrix(&[&[4.0, 1.0], &[4.0, 2.0]]),
        ])
        .unwrap();
        match got.core_ref() {
            CoreRef::Array(array) => {
                assert_eq!(array.shape(), ArrayShape { rows: 2, cols: 1 });
                assert_close(numeric_from_cell(array.get(0, 0).unwrap()).unwrap(), 48.0);
                assert_close(
                    numeric_from_cell(array.get(1, 0).unwrap()).unwrap(),
                    136.421762958185,
                );
            }
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn forecast_and_forecast_linear_match_seed_lanes() {
        let forecast =
            eval_forecast_surface(&[num(3.0), row(&[1.0, 2.0]), row(&[2.0, 4.0])]).unwrap();
        let forecast_linear =
            eval_forecast_linear_surface(&[num(3.0), row(&[1.0, 2.0]), row(&[2.0, 4.0])]).unwrap();
        assert_close(expect_number(forecast), 1.5);
        assert_close(expect_number(forecast_linear), 1.5);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn linest_returns_single_predictor_coefficients() {
        let got = eval_linest_surface(&[col(&[3.0, 5.0, 7.0]), col(&[1.0, 2.0, 3.0])]).unwrap();
        let coeffs = expect_row_array(got);
        assert_eq!(coeffs.len(), 2);
        assert_close(coeffs[0], 2.0);
        assert_close(coeffs[1], 1.0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn linest_without_const_keeps_trailing_zero_intercept_cell() {
        let got = eval_linest_surface(&[
            col(&[2.0, 4.0, 6.0]),
            col(&[1.0, 2.0, 3.0]),
            bool_arg(false),
        ])
        .unwrap();
        let coeffs = expect_row_array(got);
        // Excel 16.0.20430: =INDEX(LINEST({2;4;6},{1;2;3},FALSE),1) is 2.0000000000000004, the QR
        // fit's rounding (dc-j9k), and the intercept cell is 0.
        assert_eq!(coeffs, vec![2.0000000000000004, 0.0]);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn linest_returns_reversed_multivariate_coefficients() {
        let got = eval_linest_surface(&[
            col(&[6.0, 8.0, 9.0, 11.0, 10.0]),
            matrix(&[
                &[1.0, 1.0],
                &[2.0, 1.0],
                &[1.0, 2.0],
                &[2.0, 2.0],
                &[3.0, 1.0],
            ]),
        ])
        .unwrap();
        let coeffs = expect_row_array(got);
        assert_close(coeffs[0], 3.0);
        assert_close(coeffs[1], 2.0);
        assert_close(coeffs[2], 1.0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn logest_returns_growth_factor_and_base() {
        let got = eval_logest_surface(&[col(&[3.0, 6.0, 12.0]), col(&[1.0, 2.0, 3.0])]).unwrap();
        let coeffs = expect_row_array(got);
        assert_eq!(coeffs.len(), 2);
        assert_close(coeffs[0], 2.0);
        assert_close(coeffs[1], 1.5);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn logest_without_const_keeps_trailing_one_base_cell() {
        let got = eval_logest_surface(&[
            col(&[3.0, 6.0, 12.0]),
            col(&[1.0, 2.0, 3.0]),
            bool_arg(false),
        ])
        .unwrap();
        let coeffs = expect_row_array(got);
        assert_close(coeffs[0], 2.37956557896877);
        assert_close(coeffs[1], 1.0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn logest_returns_reversed_multivariate_factors() {
        let got = eval_logest_surface(&[
            col(&[9.0, 18.0, 27.0, 54.0, 72.0]),
            matrix(&[
                &[1.0, 1.0],
                &[2.0, 1.0],
                &[1.0, 2.0],
                &[2.0, 2.0],
                &[3.0, 2.0],
            ]),
        ])
        .unwrap();
        let coeffs = expect_row_array(got);
        assert_close(coeffs[0], 2.84212006162885);
        assert_close(coeffs[1], 1.70056600083439);
        assert_close(coeffs[2], 2.01941161117829);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn scalar_new_x_returns_scalar_trend_result() {
        let got =
            eval_trend_surface(&[col(&[2.0, 4.0, 6.0]), col(&[1.0, 2.0, 3.0]), num(4.0)]).unwrap();
        assert_close(expect_number(got), 8.0);
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn trend_projects_matrix_shape_for_single_predictor_new_x() {
        let got = eval_trend_surface(&[
            col(&[2.0, 4.0, 6.0]),
            col(&[1.0, 2.0, 3.0]),
            matrix(&[&[1.0, 2.0], &[3.0, 4.0]]),
        ])
        .unwrap();
        match got.core_ref() {
            CoreRef::Array(array) => {
                assert_eq!(array.shape(), ArrayShape { rows: 2, cols: 2 });
                assert_close(numeric_from_cell(array.get(0, 0).unwrap()).unwrap(), 2.0);
                assert_close(numeric_from_cell(array.get(0, 1).unwrap()).unwrap(), 4.0);
                assert_close(numeric_from_cell(array.get(1, 0).unwrap()).unwrap(), 6.0);
                assert_close(numeric_from_cell(array.get(1, 1).unwrap()).unwrap(), 8.0);
            }
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn trend_projects_multivariate_rows_to_column_result() {
        let got = eval_trend_surface(&[
            col(&[6.0, 8.0, 9.0, 11.0, 10.0]),
            matrix(&[
                &[1.0, 1.0],
                &[2.0, 1.0],
                &[1.0, 2.0],
                &[2.0, 2.0],
                &[3.0, 1.0],
            ]),
            matrix(&[&[4.0, 1.0], &[4.0, 2.0]]),
        ])
        .unwrap();
        match got.core_ref() {
            CoreRef::Array(array) => {
                assert_eq!(array.shape(), ArrayShape { rows: 2, cols: 1 });
                assert_close(numeric_from_cell(array.get(0, 0).unwrap()).unwrap(), 12.0);
                assert_close(numeric_from_cell(array.get(1, 0).unwrap()).unwrap(), 15.0);
            }
            other => panic!("expected array, got {other:?}"),
        }
    }

    /// The cells of an array result: a number as its bits, an error as its code.
    fn block(value: CalcValue) -> Vec<Result<u64, WorksheetErrorCode>> {
        match value.core_ref() {
            CoreRef::Array(array) => array
                .iter_row_major()
                .map(|cell| match cell.core_ref() {
                    CoreRef::Number(n) => Ok(n.to_bits()),
                    CoreRef::Error(e) => Err(e),
                    other => panic!("unexpected cell: {other:?}"),
                })
                .collect(),
            other => panic!("expected array, got {other:?}"),
        }
    }

    /// dc-j9k: LINEST's statistics block as Excel 16.0.20430 gives it (2026-10-25), cell for
    /// cell: an ordinary fit; one observation without a constant (fitted through the origin, no
    /// degrees of freedom: R squared 1, standard errors 0, F #NUM!); and a zero predictor, kept
    /// with coefficient 0 and counted in the degrees of freedom. The catalogue oracle's sheet
    /// dcj9k_linest_stats holds 1,970 such readings.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn linest_statistics_match_excel_readings() {
        use WorksheetErrorCode::{NA, Num};
        let ordinary = eval_linest_surface(&[
            col(&[1.2, 2.9, 3.1, 4.8, 5.5]),
            col(&[1.0, 2.0, 3.0, 4.0, 5.0]),
            bool_arg(true),
            bool_arg(true),
        ])
        .unwrap();
        let cells = block(ordinary);
        assert_eq!(cells[0], Ok(0x3ff0cccccccccccd)); // slope 1.05
        assert_eq!(cells[2], Ok(0x3fc01b375bea0831)); // its standard error
        assert_eq!(cells[5], Ok(0x3fd9776097813568)); // the standard error of y
        let one =
            eval_linest_surface(&[num(5.0), num(2.0), bool_arg(false), bool_arg(true)]).unwrap();
        assert_eq!(
            block(one),
            vec![
                Ok(2.5f64.to_bits()),
                Ok(0),
                Ok(0),
                Err(NA),
                Ok(1.0f64.to_bits()),
                Ok(0),
                Err(Num),
                Ok(0),
                Ok(25.0f64.to_bits()),
                Ok(0),
            ]
        );
        let zero = eval_linest_surface(&[
            col(&[1.0, 2.0, 4.0]),
            col(&[0.0, 0.0, 0.0]),
            bool_arg(false),
            bool_arg(true),
        ])
        .unwrap();
        assert_eq!(
            block(zero),
            vec![
                Ok(0),
                Ok(0),
                Ok(0),
                Err(NA),
                Ok(0x3fa8618618618618), // 1/21
                Ok(0x40094c583ada5b53), // sqrt(10)
                Ok(0x3fb999999999999a), // 0.1
                Ok(2.0f64.to_bits()),
                Ok(1.0f64.to_bits()),
                Ok(20.0f64.to_bits()),
            ]
        );
    }

    fn bits_col(values: &[u64]) -> CalcValue {
        col(&values
            .iter()
            .map(|b| f64::from_bits(*b))
            .collect::<Vec<_>>())
    }

    /// dc-j9k.14: the collinearity test reads a column's running remainder, its sum of squares
    /// before the steps less the square of each entry above the diagonal, against 2^-41 of that
    /// sum. Excel 16.0.20527 (2026-11-07) degrees of freedom for two predictors, one exactly a
    /// power-of-two multiple of b and one b plus a small multiple of a vector orthogonal to b:
    /// the remainder ratio of the second pivot lies within 0.05% of 2^-41 in each, and the
    /// fourth is exactly 2^-41 (kept). The remaining sum of squares the reflections use, taken
    /// from the entries, puts each of these on the other side of 2^-41.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn collinearity_test_matches_excel_degrees_of_freedom() {
        let cases: [(&[u64], &[u64], &[u64], bool, u64); 6] = [
            (
                &[0x3ff0000000000000, 0x4000000000000000, 0x4010000000000000],
                &[0x3ff0000376b70fde, 0x3ffffffe44a47811, 0x4014000000000000],
                &[0x4000000000000000, 0x4010000000000000, 0x4024000000000000],
                false,
                0x3ff0000000000000,
            ),
            (
                &[0x3ff0000000000000, 0x4000000000000000, 0x4010000000000000],
                &[0x3ff00001843ac3a9, 0x3ffffffe7bc53c57, 0x4014000000000000],
                &[0x3fe0000000000000, 0x3ff0000000000000, 0x4004000000000000],
                true,
                0x3ff0000000000000,
            ),
            (
                &[
                    0x3ff0000000000000,
                    0x4000000000000000,
                    0x4010000000000000,
                    0x401c000000000000,
                ],
                &[
                    0x3ff000044a3dd906,
                    0x3ffffffddae1137d,
                    0x4014000000000000,
                    0x4010000000000000,
                ],
                &[
                    0x4000000000000000,
                    0x4010000000000000,
                    0x4024000000000000,
                    0x4020000000000000,
                ],
                false,
                0x4008000000000000,
            ),
            (
                &[
                    0x3ff0000000000000,
                    0x4000000000000000,
                    0x4010000000000000,
                    0x401c000000000000,
                ],
                &[
                    0x3ff000044a3dd90b,
                    0x3ffffffddae1137b,
                    0x4014000000000000,
                    0x4010000000000000,
                ],
                &[
                    0x3fe0000000000000,
                    0x3ff0000000000000,
                    0x4004000000000000,
                    0x4000000000000000,
                ],
                false,
                0x4000000000000000,
            ),
            (
                &[
                    0x3ff0000000000000,
                    0x4000000000000000,
                    0x4010000000000000,
                    0x401c000000000000,
                    0x4008000000000000,
                    0x4020000000000000,
                    0x4018000000000000,
                    0x3ff0000000000000,
                ],
                &[
                    0x3ff00007971f6708,
                    0x3ffffffc34704c7c,
                    0x4014000000000000,
                    0x4010000000000000,
                    0x4008000000000000,
                    0x401c000000000000,
                    0x4018000000000000,
                    0x4000000000000000,
                ],
                &[
                    0x4000000000000000,
                    0x4010000000000000,
                    0x4024000000000000,
                    0x4020000000000000,
                    0x4018000000000000,
                    0x402c000000000000,
                    0x4028000000000000,
                    0x4010000000000000,
                ],
                false,
                0x401c000000000000,
            ),
            (
                &[
                    0x3ff0000000000000,
                    0x4000000000000000,
                    0x4010000000000000,
                    0x401c000000000000,
                    0x4008000000000000,
                    0x4020000000000000,
                    0x4018000000000000,
                    0x3ff0000000000000,
                    0x4022000000000000,
                    0x4000000000000000,
                    0x4020000000000000,
                    0x4014000000000000,
                    0x4008000000000000,
                    0x4018000000000000,
                    0x4000000000000000,
                    0x4010000000000000,
                ],
                &[
                    0x3ff00004c84789fa,
                    0x3ffffffb37b87606,
                    0x4014000000000000,
                    0x4010000000000000,
                    0x4008000000000000,
                    0x401c000000000000,
                    0x4018000000000000,
                    0x4000000000000000,
                    0x4020000000000000,
                    0x3ff0000000000000,
                    0x4022000000000000,
                    0x4010000000000000,
                    0x4000000000000000,
                    0x4018000000000000,
                    0x4008000000000000,
                    0x4014000000000000,
                ],
                &[
                    0x3fe0000000000000,
                    0x3ff0000000000000,
                    0x4004000000000000,
                    0x4000000000000000,
                    0x3ff8000000000000,
                    0x400c000000000000,
                    0x4008000000000000,
                    0x3ff0000000000000,
                    0x4010000000000000,
                    0x3fe0000000000000,
                    0x4012000000000000,
                    0x4000000000000000,
                    0x3ff0000000000000,
                    0x4008000000000000,
                    0x3ff8000000000000,
                    0x4004000000000000,
                ],
                true,
                0x402a000000000000,
            ),
        ];
        for (index, (y, x1, x2, constant, df)) in cases.iter().enumerate() {
            let n = y.len();
            let x = CalcValue::array(
                CalcArray::from_rows(
                    (0..n)
                        .map(|i| {
                            vec![
                                CalcValue::number(f64::from_bits(x1[i])),
                                CalcValue::number(f64::from_bits(x2[i])),
                            ]
                        })
                        .collect(),
                )
                .unwrap(),
            );
            let cells = block(
                eval_linest_surface(&[bits_col(y), x, bool_arg(*constant), bool_arg(true)])
                    .unwrap(),
            );
            assert_eq!(
                cells[3 * 3 + 1],
                Ok(*df),
                "case {index}: degrees of freedom"
            );
        }
    }

    /// dc-j9k.14 (R42): the QR stops at the first pivot that fails the collinearity test, so a
    /// zero predictor not yet stepped is left out with it. Excel 16.0.20527 (2026-11-09, the R41
    /// regression cohort retained in DnaCalc-Research): known_x {0, x, 2x} with x = 1..8, no
    /// constant, y the second unit vector: 2x alone is fitted, over seven degrees of freedom.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rank_stop_leaves_out_an_unstepped_zero_column() {
        let rows: Vec<Vec<f64>> = (1..=8)
            .map(|i| vec![0.0, i as f64, 2.0 * i as f64])
            .collect();
        let rows: Vec<&[f64]> = rows.iter().map(|r| r.as_slice()).collect();
        let y = col(&[0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        let cells = block(
            eval_linest_surface(&[y, matrix(&rows), bool_arg(false), bool_arg(true)]).unwrap(),
        );
        assert_eq!(cells[0], Ok(0x3f74141414141412)); // the coefficient of 2x
        assert_eq!(cells[1], Ok(0)); // x, left out
        assert_eq!(cells[2], Ok(0)); // the zero column, left out with it
        assert_eq!(cells[3 * 4 + 1], Ok(7.0f64.to_bits())); // degrees of freedom
        assert_eq!(cells[4 * 4 + 1], Ok(0x3fef5f5f5f5f5f5e)); // residual sum of squares
    }

    /// dc-j9k.14 (R42): a diagonal entry that is exactly zero when its step comes reflects as a
    /// negative one. With a constant, x = {-3, -1, 1, 3} has a zero entry after the ones
    /// column's step; Excel 16.0.20527 (R41 regression cohort) gives the slope's standard error
    /// below, and the mirrored x = {3, 1, -1, -3} with the mirrored y the same bits.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn zero_diagonal_entry_reflects_as_negative() {
        for (x, y) in [
            ([-3.0, -1.0, 1.0, 3.0], [1.0, 0.0, 0.0, 0.0]),
            ([3.0, 1.0, -1.0, -3.0], [0.0, 0.0, 0.0, 1.0]),
        ] {
            let cells = block(
                eval_linest_surface(&[col(&y), col(&x), bool_arg(true), bool_arg(true)]).unwrap(),
            );
            assert_eq!(cells[2], Ok(0x3fb62b9586ad0a22), "x {x:?}"); // the slope's standard error
        }
    }

    /// dc-j9k.14 (R42): the pivot reads the running remainder. The second column is 2a, 2^-19
    /// off in its fourth entry, and is stepped first; a's running remainder then exceeds the
    /// tiny third column's, so a is pivoted next, fails the collinearity test and the
    /// decomposition stops. Only the second column is fitted. Excel 16.0.20527, 2026-11-09, a
    /// fresh question frozen before asking (R42 pivot set 3, DnaCalc-Research evidence/round42).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn pivot_reads_the_running_remainder() {
        let rows: [[u64; 3]; 6] = [
            [0xc03b000000000000, 0xc04b000000000000, 0x3e20000000000000],
            [0xc046800000000000, 0xc056800000000000, 0x3e10000000000000],
            [0x0000000000000000, 0x0000000000000000, 0x3e34000000000000],
            [0x4018000000000000, 0x4028000040000000, 0x3e34000000000000],
            [0xc008000000000000, 0xc018000000000000, 0x3e10000000000000],
            [0xc043800000000000, 0xc053800000000000, 0xbe10000000000000],
        ];
        let x = CalcValue::array(
            CalcArray::from_rows(
                rows.iter()
                    .map(|r| {
                        r.iter()
                            .map(|b| CalcValue::number(f64::from_bits(*b)))
                            .collect()
                    })
                    .collect(),
            )
            .unwrap(),
        );
        let y = col(&[11.0, 3.0, 15.0, 3.0, 4.0, -1.0]);
        let cells = block(eval_linest_surface(&[y, x, bool_arg(true), bool_arg(true)]).unwrap());
        assert_eq!(cells[0], Ok(0)); // the tiny column, left out
        assert_eq!(cells[1], Ok(0x3faa2e8b958fd776)); // the near multiple of a
        assert_eq!(cells[2], Ok(0)); // a, collinear
        assert_eq!(cells[3 * 4 + 1], Ok(4.0f64.to_bits())); // degrees of freedom
        assert_eq!(cells[4 * 4 + 1], Ok(0x4062ff64db601692)); // residual sum of squares
    }

    /// dc-j9k.14 (R43): with a constant the ones column is stored after the predictors and
    /// swapped to the front, and a pivot tie goes to the earlier column of that order. For
    /// known_x {0, x, -x} the pivot is x, -x is collinear and the zero column is left out with
    /// it. Excel 16.0.20527, 2026-11-09 (R42 pivot set 3, DnaCalc-Research evidence/round42).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn ones_column_swapped_in_decides_a_mirrored_tie() {
        let x = [21.0, -23.0, 2.0, -24.0, -5.0, 9.0, 3.0, 15.0];
        let rows: Vec<Vec<f64>> = x.iter().map(|v| vec![0.0, *v, -*v]).collect();
        let rows: Vec<&[f64]> = rows.iter().map(|r| r.as_slice()).collect();
        let y = col(&[19.0, 6.0, 7.0, 12.0, -6.0, 3.0, 6.0, 7.0]);
        let cells = block(
            eval_linest_surface(&[y, matrix(&rows), bool_arg(true), bool_arg(true)]).unwrap(),
        );
        assert_eq!(cells[0], Ok(0)); // -x, collinear
        assert_eq!(cells[1], Ok(0x3fb874848140d471)); // x
        assert_eq!(cells[2], Ok(0)); // the zero column, left out with -x
        assert_eq!(cells[3 * 4 + 1], Ok(6.0f64.to_bits())); // degrees of freedom
        assert_eq!(cells[4 * 4 + 1], Ok(0x4075241d892ddca4)); // residual sum of squares
        assert_eq!(cells[2 * 4 + 1], Ok(0x401e089c4d08d0b2)); // standard error of y
    }

    /// dc-j9k.14 (R43): while a column is still to come the collinearity test reads the
    /// reflector's vv. Here the second column is the third halved with 2.5 * 2^-18 added to its last
    /// entry: its remainder is below 2^-41 of its sum of squares but vv is not, so all three
    /// predictors are kept over zero degrees of freedom. Excel 16.0.20527, 2026-11-09, a fresh
    /// question frozen before asking (R43 proof set, DnaCalc-Research evidence/round43).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn collinearity_test_reads_vv_while_columns_remain() {
        let rows: [[u64; 3]; 4] = [
            [0x0000000000000000, 0x4029000000000000, 0x4039000000000000],
            [0x0000000000000000, 0xc016000000000000, 0xc026000000000000],
            [0x0000000000000000, 0x4022000000000000, 0x4032000000000000],
            [0x0000000000000000, 0x4014000280000000, 0x4024000000000000],
        ];
        let x = CalcValue::array(
            CalcArray::from_rows(
                rows.iter()
                    .map(|r| {
                        r.iter()
                            .map(|b| CalcValue::number(f64::from_bits(*b)))
                            .collect()
                    })
                    .collect(),
            )
            .unwrap(),
        );
        let y = col(&[7.0, -14.0, 12.0, 11.0]);
        let cells = block(eval_linest_surface(&[y, x, bool_arg(true), bool_arg(true)]).unwrap());
        assert_eq!(cells[0], Ok(0xc11f4e5bd4c16924));
        assert_eq!(cells[1], Ok(0x412f4e5e87b2a7ab));
        assert_eq!(cells[2], Ok(0)); // the zero column, kept
        assert_eq!(cells[3], Ok(0xc0161f629af00000)); // intercept
        assert_eq!(cells[3 * 4 + 1], Ok(0)); // degrees of freedom
    }

    /// dc-j9k.14: with the larger column of a near-collinear pair kept and the other left out,
    /// LOGEST's factors are finite (Excel 16.0.20527: 1.1912... and 1, where both columns in the
    /// fit overflow EXP to #NUM!; catalogue sheet dcj9k_r16 and workbook dcj9k_r21).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn logest_leaves_out_a_near_collinear_column() {
        let x = matrix(&[
            &[1.0, 2.0],
            &[2.0, 4.0],
            &[5.0, 10.0],
            &[4.0, 8.00000381469726],
        ]);
        let cells = block(
            eval_logest_surface(&[
                col(&[1.0, 2.0, 4.0, 7.0]),
                x,
                bool_arg(false),
                bool_arg(true),
            ])
            .unwrap(),
        );
        assert_eq!(cells[0], Ok(0x3ff30f6b4559ebb7));
        assert_eq!(cells[1], Ok(1.0f64.to_bits()));
    }

    /// The shapes of known_y and known_x are compared before their values are read (Excel
    /// 16.0.20527, dcztvv_r23 and dcztvv_r24).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn regression_shapes_come_before_values() {
        use WorksheetErrorCode::{Ref, Value};
        let wide = || matrix(&[&[1.0, 2.0], &[2.0, 4.0], &[5.0, 10.0], &[4.0, 8.0001]]);
        let text = |t: &str| CalcValue::text(crate::value::ExcelText::from_interop_assignment(t));
        assert_eq!(
            eval_linest_surface(&[num(0.0), wide(), bool_arg(true), bool_arg(true)]),
            Err(Ref)
        );
        assert_eq!(
            eval_linest_surface(&[text("-11.827"), wide(), bool_arg(true), bool_arg(true)]),
            Err(Ref)
        );
        assert_eq!(eval_linest_surface(&[bool_arg(true), wide()]), Err(Ref));
        assert_eq!(
            eval_logest_surface(&[num(0.0), wide(), bool_arg(true), bool_arg(true)]),
            Err(Ref)
        );
        assert_eq!(
            eval_linest_surface(&[col(&[1.0, 2.0, 4.0, 7.0]), text("53.13")]),
            Err(Ref)
        );
        assert_eq!(
            eval_linest_surface(&[text("5"), col(&[1.0, 2.0, 3.0])]),
            Err(Value)
        );
        assert_eq!(eval_linest_surface(&[num(5.0), text("1")]), Err(Value));
        assert_eq!(
            eval_linest_surface(&[CalcValue::error(WorksheetErrorCode::NA), wide()]),
            Err(WorksheetErrorCode::NA)
        );
    }

    fn bits_matrix(rows: &[&[u64]]) -> CalcValue {
        CalcValue::array(
            CalcArray::from_rows(
                rows.iter()
                    .map(|r| {
                        r.iter()
                            .map(|b| CalcValue::number(f64::from_bits(*b)))
                            .collect()
                    })
                    .collect(),
            )
            .unwrap(),
        )
    }

    /// dc-j9k.69 / dc-iuwct.62 (Carbon cb12, Excel 16.0.20527.20196): unified x87 PC64 QR,
    /// `excel_sqrt_staged`, unpermuted `se_b`, reverse-`k` back-substitution, zero-total `R^2 = 1`,
    /// and `GROWTH`/`TREND` prediction staging across frozen unseen holdouts and the `sx1` gross
    /// `intercept_se` cohort.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cb12_unified_x87_regression_and_forecast_matches_excel() {
        // 1. excel_sqrt_staged: sx1 intercept_se[344] (97.2B ULP -> 0), cb12_h2_0001 se_y,
        //    and unseen cb12_h2_0006 (102.4B ULP -> 0).
        let y344: Vec<u64> = [
            0x3fe8000000000000,
            0x4012000000000000,
            0x3ff4000000000000,
            0x4016000000000000,
        ]
        .repeat(4);
        let x344_unit: [[u64; 2]; 4] = [
            [0x4010000000000000, 0x401ffffe00000000],
            [0x4018000000000000, 0x4027ffff00000000],
            [0x4010000000000000, 0x4020000100000000],
            [0x4018000000000000, 0x4028000100000000],
        ];
        let x344_rows: Vec<&[u64]> = (0..16).map(|i| x344_unit[i % 4].as_slice()).collect();
        let cells = block(
            eval_linest_surface(&[
                bits_col(&y344),
                bits_matrix(&x344_rows),
                bool_arg(true),
                bool_arg(true),
            ])
            .unwrap(),
        );
        assert_eq!(cells[1 * 3 + 2], Ok(0x3fc6a087c5d584f3), "sx1 intercept_se[344]");

        let cells = block(
            eval_linest_surface(&[
                bits_col(&[
                    0x3ff0000000000000,
                    0x3ffc617704dff392,
                    0x3ff41a9df6e73729,
                    0x0000000000000000,
                    0x0000000000000000,
                ]),
                bits_col(&[
                    0x3ff0000000000000,
                    0x0000000000000000,
                    0x0000000000000000,
                    0x0000000000000000,
                    0x0000000000000000,
                ]),
                bool_arg(false),
                bool_arg(true),
            ])
            .unwrap(),
        );
        assert_eq!(cells[2 * 2 + 1], Ok(0x3ff163d0c9476b90), "cb12_h2_0001 se_y");

        let cells = block(
            eval_linest_surface(&[
                bits_col(&[
                    0x4020000000000000,
                    0x402ec00000000000,
                    0xc026800000000000,
                    0xc023c00000000000,
                    0x4033800000000000,
                    0xc020400000000000,
                    0x4036400000000000,
                    0x4005000000000000,
                    0x4036000000000000,
                    0x4036600000000000,
                    0xc00a000000000000,
                    0x4026400000000000,
                ]),
                bits_matrix(&[
                    &[0x401c000000000000, 0x402c0001ecd721fc],
                    &[0x4011000000000000, 0x4020fffe89f9cd6f],
                    &[0x4012000000000000, 0x402200011fb65c8e],
                    &[0x4013000000000000, 0x4022fffecba65de4],
                    &[0x4024000000000000, 0x40340000b34857fb],
                    &[0x4011000000000000, 0x4020fffec06b179d],
                    &[0x4016000000000000, 0x4026000114a5c566],
                    &[0x4023800000000000, 0x40337fff084b108a],
                    &[0x4024000000000000, 0x40340000d6d3e8f3],
                    &[0x4020800000000000, 0x40307fff21d7324a],
                    &[0x4012000000000000, 0x402200015a205b9e],
                    &[0x4017000000000000, 0x4026fffe383776cf],
                ]),
                bool_arg(true),
                bool_arg(true),
            ])
            .unwrap(),
        );
        assert_eq!(cells[1 * 3 + 2], Ok(0x40257c2f4a15e991), "cb12_h2_0006 QR-amplified se_b");

        // 2. Unpermuted predictor-order se_b (4 predictors, cb12_h_0004).
        let cells = block(
            eval_linest_surface(&[
                bits_col(&[
                    0x4034000000000000,
                    0x403e400000000000,
                    0xc03b800000000000,
                    0xc033400000000000,
                    0x4044800000000000,
                    0x4043a00000000000,
                    0x402d000000000000,
                ]),
                bits_matrix(&[
                    &[0x4042500000000000, 0x406a240000000000, 0xc029c00000000000, 0x40761a0000000000],
                    &[0x404ca00000000000, 0x406da80000000000, 0xc03bc00000000000, 0xc08b860000000000],
                    &[0x4007000000000000, 0x406aec0000000000, 0xc034a00000000000, 0xc08ef10000000000],
                    &[0x4051200000000000, 0xc060700000000000, 0xc03b800000000000, 0x4039800000000000],
                    &[0xc017800000000000, 0x4047500000000000, 0xc035600000000000, 0x4044900000000000],
                    &[0x4050300000000000, 0xc049200000000000, 0xc036400000000000, 0x407afc0000000000],
                    &[0x4046b00000000000, 0x4059b80000000000, 0xc032200000000000, 0x4074260000000000],
                ]),
                bool_arg(true),
                bool_arg(true),
            ])
            .unwrap(),
        );
        assert_eq!(cells[1 * 5 + 4], Ok(0x405671480784f433), "cb12_h_0004 unpermuted se_b");

        // 3. Pre-QR PC64 mean/centring (cb12_h_0044) & reverse-k back-substitution (cb12_h_0071).
        let cells = block(
            eval_linest_surface(&[
                bits_col(&[
                    0x3fb000000000003e,
                    0x3c60010000000000,
                    0x0000000000000000,
                    0x0000000000000000,
                ]),
                bits_col(&[
                    0xbff0000000000000,
                    0xbff0000000000000,
                    0x3ff0000000000000,
                    0x3ff0000000000000,
                ]),
                bool_arg(true),
                bool_arg(false),
            ])
            .unwrap(),
        );
        assert_eq!(cells[1], Ok(0x3f9000000000003e), "cb12_h_0044 pre-QR x87");

        let cells = block(
            eval_linest_surface(&[
                bits_col(&[
                    0x0000000000000000,
                    0x4050000000000000,
                    0x4020000000000000,
                    0x3ff0000000000000,
                    0x0000000000000000,
                ]),
                bits_matrix(&[
                    &[0x4080000000000000, 0xbfc00000000000aa, 0x3c70000000000000, 0x3c70000000000000],
                    &[0x0000000000000000, 0x4050000000000000, 0x0000000000000000, 0x0000000000000000],
                    &[0x0000000000000000, 0x0000000000000000, 0x4020000000000000, 0x0000000000000000],
                    &[0x0000000000000000, 0x0000000000000000, 0x0000000000000000, 0x3ff0000000000000],
                    &[0x0000000000000000, 0x0000000000000000, 0x0000000000000000, 0x0000000000000000],
                ]),
                bool_arg(false),
                bool_arg(false),
            ])
            .unwrap(),
        );
        assert_eq!(cells[3], Ok(0x3f300000000000a9), "cb12_h_0071 reverse-k backsub");

        // 4. Zero-total R^2 = 1.0 & F = #NUM! for both LINEST (cb12_h_0111) and LOGEST (cb12_h_0151).
        let x_degen = bits_matrix(&[
            &[0x3ff0000000000000, 0x4000000000000000],
            &[0x4000000000000000, 0x4014000000000000],
            &[0x4008000000000000, 0x3ff0000000000000],
            &[0x4010000000000000, 0x4008000000000000],
        ]);
        let lin_degen = block(
            eval_linest_surface(&[
                bits_col(&[0xc00c000000000000; 4]),
                x_degen.clone(),
                bool_arg(true),
                bool_arg(true),
            ])
            .unwrap(),
        );
        assert_eq!(lin_degen[2 * 3], Ok(1.0f64.to_bits()), "cb12_h_0111 LINEST R^2");
        assert_eq!(lin_degen[3 * 3], Err(WorksheetErrorCode::Num), "cb12_h_0111 LINEST F");

        let log_degen = block(
            eval_logest_surface(&[
                bits_col(&[0x3ff0000000000000; 4]),
                x_degen,
                bool_arg(false),
                bool_arg(true),
            ])
            .unwrap(),
        );
        assert_eq!(log_degen[2 * 3], Ok(1.0f64.to_bits()), "cb12_h_0151 LOGEST R^2");
        assert_eq!(log_degen[3 * 3], Err(WorksheetErrorCode::Num), "cb12_h_0151 LOGEST F");

        // 5. GROWTH (cb12_h_0191, cb12_h2_0010) and TREND (cb12_h_0199) PC64 prediction loops.
        let growth1 = eval_growth_surface(&[
            bits_col(&[
                0x40199abb303faa3c,
                0x401566396cb2a00d,
                0x400fd7b35c2c096a,
                0x400f6dc871e0a156,
            ]),
            bits_matrix(&[
                &[0x0000000000000000, 0x0000000000000000, 0x0000000000000000],
                &[0x3ff0000000000000, 0x0000000000000000, 0x0000000000000000],
                &[0x0000000000000000, 0x3ff0000000000000, 0x0000000000000000],
                &[0x0000000000000000, 0x0000000000000000, 0x3ff0000000000000],
            ]),
            bits_matrix(&[&[0x3fdeb00edcca20ba, 0xbfdba5d5373cd1d9, 0x3fd8ec7f59f97b8b]]),
            bool_arg(true),
        ])
        .unwrap();
        assert_eq!(expect_number(growth1).to_bits(), 0x4017da1dd1049ef6, "cb12_h_0191 GROWTH");

        let growth2 = eval_growth_surface(&[
            bits_col(&[
                0x401793c8a6aa1cf8,
                0x400f8146e6d0f47b,
                0x4013b54cf2ae4977,
                0x3ffb2e4a7e7c6a6c,
                0x401d59d9ca09f057,
                0x401abc95ec5c1207,
                0x3ffea3a9c902371e,
                0x3ffe33fb6355124a,
            ]),
            bits_matrix(&[
                &[0xbfff7b9e5b618ada, 0x40044f2de40bd8ec],
                &[0xbfff45274700270a, 0x400860d38341f0dc],
                &[0x3ffc0fa53b25e6db, 0xc0043b239ec19fec],
                &[0xc001f9ebf783be2d, 0xc0065aa1f5742634],
                &[0x3ff499f9927cb916, 0x400868cfe83f2c64],
                &[0x400f4cb449341e2d, 0x4001ea6c761c3be2],
                &[0x3ffeb7fd8fb8f577, 0x40020d7b403c475e],
                &[0xbffd9efbe405bd14, 0x3fffad30c796433a],
            ]),
            bits_matrix(&[&[0x3ff7de4dbefa68b4, 0xbfe46ddfd6135e2a]]),
            bool_arg(false),
        ])
        .unwrap();
        assert_eq!(expect_number(growth2).to_bits(), 0x3fef0c56a8859440, "cb12_h2_0010 GROWTH");

        let trend = eval_trend_surface(&[
            bits_col(&[
                0x40082113484cd2f9,
                0x400ee8dbe3c9cc80,
                0xc00a49ffdb9c2117,
                0xc00d0aa75779f8d2,
                0xc00e8623d2adf8f5,
                0xc00d72dfe677a3e8,
                0xc00e1c65d24c9032,
                0xc008537572158f7d,
            ]),
            bits_matrix(&[
                &[0x3ffb0745cf804580, 0x3ffe5ac165c47773],
                &[0xc00f3d868f1b3b0d, 0x3ff7822ce5550570],
                &[0x40096cffd538524c, 0x400e9114e4915a77],
                &[0x3ff8d476f9e8841f, 0x3ffa6067182e4e34],
                &[0x40111ea578e8d004, 0x400d11d75495447d],
                &[0x4010af32a69c0666, 0x40130e80be4e0e36],
                &[0xc00947f0372d0086, 0x3ff654671f86f541],
                &[0x400b56038d5293f9, 0xc00b5425c462c521],
            ]),
            bits_matrix(&[&[0x40062fc8467b6289, 0xbff6afaad016ea31]]),
            bool_arg(false),
        ])
        .unwrap();
        assert_eq!(expect_number(trend).to_bits(), 0xbff405455e383506, "cb12_h_0199 TREND");
    }
}
