//! LINEST, LOGEST, TREND and GROWTH (and FORECAST's pair kernel), ported from OxFunc
//! functions/regression_forecast_family.rs (6fca3518) with its reference resolution removed: the
//! statistics family reads a reference argument into an array before the call.

use crate::ported::excel_numeric;
use crate::value::{CalcArray, CalcValue, CoreValue, WorksheetErrorCode};


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VectorShape {
    Scalar,
    Row(usize),
    Column(usize),
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
/// An argument as the regression functions read it: the caller has read a reference into an
/// array; an empty or omitted argument is #VALUE!.
fn resolve_arg_eval(arg: &CalcValue) -> Result<CalcValue, WorksheetErrorCode> {
    match arg.core() {
        CoreValue::Missing | CoreValue::Empty => Err(WorksheetErrorCode::Value),
        _ => Ok(arg.clone()),
    }
}


fn optional_arg_value(
    arg: Option<&CalcValue>
) -> Result<Option<CalcValue>, WorksheetErrorCode> {
    match arg {
        None => Ok(None),
        Some(value) if matches!(value.core(), CoreValue::Missing) => Ok(None),
        Some(other) => Ok(Some(resolve_arg_eval(other)?)),
    }
}

fn numeric_from_cell(cell: &CalcValue) -> Result<f64, WorksheetErrorCode> {
    match cell.core() {
        CoreValue::Number(n) => Ok(*n),
        CoreValue::Error(code) => Err(*code),
        CoreValue::Text(_)
        | CoreValue::Logical(_)
        | CoreValue::Empty
        | CoreValue::Missing
        | CoreValue::Array(_)
        | CoreValue::Reference(_) => Err(WorksheetErrorCode::Value,),
    }
}

fn numeric_scalar_from_eval(value: &CalcValue) -> Result<f64, WorksheetErrorCode> {
    match value.core() {
        CoreValue::Number(n) => Ok(*n),
        CoreValue::Logical(flag) => Ok(if *flag { 1.0 } else { 0.0 }),
        CoreValue::Error(code) => Err(*code),
        CoreValue::Array(array) => {
            let shape = array.shape();
            if shape.rows == 1 && shape.cols == 1 {
                numeric_from_cell(array.get(0, 0).expect("single-cell array"))
            } else {
                Err(WorksheetErrorCode::Value,)
            }
        }
        CoreValue::Text(_) | CoreValue::Reference(_) => Err(WorksheetErrorCode::Value,),
        _ => Err(WorksheetErrorCode::Value,),
    }
}

fn bool_flag_from_eval_or_default(
    value: Option<CalcValue>,
    default: bool,
) -> Result<bool, WorksheetErrorCode> {
    match value {
        None => Ok(default),
        Some(value) if matches!(value.core(), CoreValue::Logical(_)) => match value.core() {
            CoreValue::Logical(flag) => Ok(*flag),
            _ => unreachable!("guarded logical"),
        },
        Some(other) => Ok(numeric_scalar_from_eval(&other)? != 0.0),
    }
}

fn collect_numeric_vector_from_eval(
    value: &CalcValue,
) -> Result<NumericVector, WorksheetErrorCode> {
    match value.core() {
        CoreValue::Number(n) => Ok(NumericVector {
            values: vec![*n],
            shape: VectorShape::Scalar,
        }),
        CoreValue::Error(code) => Err(*code),
        CoreValue::Array(array) => {
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
        CoreValue::Text(_) | CoreValue::Logical(_) | CoreValue::Reference(_) => Err(
            WorksheetErrorCode::Value,
        ),
        _ => Err(WorksheetErrorCode::Value,),
    }
}

fn collect_numeric_vector_arg(
    arg: &CalcValue
) -> Result<NumericVector, WorksheetErrorCode> {
    let eval = resolve_arg_eval(arg)?;
    collect_numeric_vector_from_eval(&eval)
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
    match value.core() {
        CoreValue::Number(n) => Ok(NumericMatrix {
            rows: 1,
            cols: 1,
            values: vec![*n],
        }),
        CoreValue::Error(code) => Err(*code),
        CoreValue::Array(array) => {
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
        CoreValue::Text(_) | CoreValue::Logical(_) | CoreValue::Reference(_) => Err(
            WorksheetErrorCode::Value,
        ),
        _ => Err(WorksheetErrorCode::Value,),
    }
}

fn collect_numeric_matrix_arg(
    arg: &CalcValue
) -> Result<NumericMatrix, WorksheetErrorCode> {
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

            if matrix.cols != normalized.cols {
                return Err(WorksheetErrorCode::Ref);
            }
            let output_shape = if matrix.rows == 1 {
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

            if matrix.cols != predictor_count {
                return Err(WorksheetErrorCode::Ref);
            }
            let output_shape = if matrix.rows == 1 {
                OutputShape::Scalar
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

/// The least-squares fit of LINEST, LOGEST, TREND and GROWTH as Excel computes it (dc-j9k,
/// identified on captured coefficients and predictions of one, two and three predictors, with and
/// without a constant; the catalogue oracle's sheet dcj9k_regression holds them):
///
/// 1. With a constant, each predictor column and `y` are centred on their means (each mean the
///    forward sum divided by the count), and a column of ones leads the matrix; without one, the
///    raw columns are the matrix.
/// 2. Householder QR. Before each predictor column's step, the predictor column with the largest
///    sum of squares over the remaining rows comes next (the first such column on a tie); the
///    ones column always goes first. At step `j`, with `x0` the diagonal entry, `rest` the sum
///    of squares below it and `s` the sum of squares from it down (each a forward sum from 0):
///    `alpha = -sign(x0)*sqrt(s)`, `v = (x0 - alpha, the entries below)`, `vv = rest + v0*v0`, and
///    each later column and `y` become `a - (2*(v.a)/vv)*v`.
/// 3. Back substitution: `b_j = (y_j - d)/alpha_j`, with `d` the forward sum of `r_jk*b_k` over
///    the later columns.
/// 4. The intercept is `my - b_1*m_1 - b_2*m_2 - ...`, subtracted in predictor order.
///
/// A column whose norm at its step is below 1e-12 (a constant or collinear predictor, which
/// Excel answers with a zero coefficient) is #NUM!, as before.
fn least_squares(
    columns: &[Vec<f64>],
    y: &[f64],
    use_const: bool,
) -> Result<LinearRegressionModel, WorksheetErrorCode> {
    let n = y.len();
    let mean = |values: &[f64]| values.iter().fold(0.0, |sum, v| sum + v) / n as f64;
    let means: Vec<f64> = columns.iter().map(|column| mean(column)).collect();
    let y_mean = if use_const { mean(y) } else { 0.0 };
    // The matrix's columns, each with the predictor it holds (None for the ones column).
    let mut matrix: Vec<(Option<usize>, Vec<f64>)> = Vec::with_capacity(columns.len() + 1);
    if use_const {
        matrix.push((None, vec![1.0; n]));
    }
    for (index, column) in columns.iter().enumerate() {
        let values = if use_const {
            column.iter().map(|v| v - means[index]).collect()
        } else {
            column.clone()
        };
        matrix.push((Some(index), values));
    }
    let mut target: Vec<f64> = if use_const {
        y.iter().map(|v| v - y_mean).collect()
    } else {
        y.to_vec()
    };
    let p = matrix.len();
    if p > n {
        return Err(WorksheetErrorCode::Num);
    }
    let squares_from =
        |values: &[f64], from: usize| values[from..].iter().fold(0.0, |sum, v| sum + v * v);
    let mut diagonal = vec![0.0; p];
    for j in 0..p {
        if matrix[j].0.is_some() {
            let mut best = j;
            let mut best_squares = -1.0;
            for (k, (_, column)) in matrix.iter().enumerate().skip(j) {
                let squares = squares_from(column, j);
                if squares > best_squares {
                    best_squares = squares;
                    best = k;
                }
            }
            matrix.swap(j, best);
        }
        let x0 = matrix[j].1[j];
        let rest = squares_from(&matrix[j].1, j + 1);
        let norm = squares_from(&matrix[j].1, j).sqrt();
        if norm < 1e-12 {
            return Err(WorksheetErrorCode::Num);
        }
        let alpha = -norm.copysign(x0);
        let mut v = matrix[j].1[j..].to_vec();
        v[0] = x0 - alpha;
        let vv = rest + v[0] * v[0];
        let reflect = |a: &mut [f64]| {
            let t = v
                .iter()
                .zip(a[j..].iter())
                .fold(0.0, |sum, (vi, ai)| sum + vi * ai);
            let f = 2.0 * t / vv;
            for (ai, vi) in a[j..].iter_mut().zip(v.iter()) {
                *ai -= f * vi;
            }
        };
        for (_, column) in matrix.iter_mut().skip(j + 1) {
            reflect(column);
        }
        reflect(&mut target);
        diagonal[j] = alpha;
    }
    let mut solution = vec![0.0; p];
    for j in (0..p).rev() {
        let d = ((j + 1)..p).fold(0.0, |sum, k| sum + matrix[k].1[j] * solution[k]);
        solution[j] = (target[j] - d) / diagonal[j];
    }
    let mut coefficients = vec![0.0; columns.len()];
    for (j, (predictor, _)) in matrix.iter().enumerate() {
        if let Some(index) = predictor {
            coefficients[*index] = solution[j];
        }
    }
    let intercept = if use_const {
        coefficients
            .iter()
            .zip(means.iter())
            .fold(y_mean, |a, (b, m)| a - b * m)
    } else {
        0.0
    };
    Ok(LinearRegressionModel {
        coefficients,
        intercept,
    })
}

fn linear_model_from_data(
    known_y: &NumericVector,
    known_x: &PreparedPredictorInput,
    use_const: bool,
) -> Result<LinearRegressionModel, WorksheetErrorCode> {
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

/// A TREND prediction: `b_1*x_1 + b_2*x_2 + ...`, added in predictor order, then the intercept.
fn linear_prediction(model: &LinearRegressionModel, x: &[f64]) -> f64 {
    let mut terms = model.coefficients.iter().zip(x.iter()).map(|(b, x)| b * x);
    let first = terms.next().unwrap_or(0.0);
    terms.fold(first, |sum, term| sum + term) + model.intercept
}

/// A GROWTH prediction: the product of `EXP(b_j*x_j)` in predictor order, from 1, then
/// `EXP(intercept)` times it.
fn exponential_prediction(model: &LinearRegressionModel, x: &[f64]) -> f64 {
    let product = model
        .coefficients
        .iter()
        .zip(x.iter())
        .fold(1.0, |product, (b, x)| product * excel_numeric::excel_exp(b * x));
    excel_numeric::excel_exp(model.intercept) * product
}

/// The fit of LOGEST and GROWTH: the least-squares fit of the worksheet LN of each y (#NUM! for
/// a y that is not positive).
fn exponential_model_from_data(
    known_y: &NumericVector,
    known_x: &PreparedPredictorInput,
    use_const: bool,
) -> Result<LinearRegressionModel, WorksheetErrorCode> {
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
    linear_model_from_data(&logged, known_x, use_const)
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

fn linest_kernel(
    known_y: &NumericVector,
    known_x: &PreparedPredictorInput,
    use_const: bool,
    stats: bool,
) -> Result<CalcValue, WorksheetErrorCode> {
    if stats {
        return Err(WorksheetErrorCode::Ref);
    }
    let model = linear_model_from_data(known_y, known_x, use_const)?;
    let mut row = model.coefficients.iter().rev().copied().collect::<Vec<_>>();
    row.push(if use_const { model.intercept } else { 0.0 });
    Ok(row_array(&row))
}

fn logest_kernel(
    known_y: &NumericVector,
    known_x: &PreparedPredictorInput,
    use_const: bool,
    stats: bool,
) -> Result<CalcValue, WorksheetErrorCode> {
    if stats {
        return Err(WorksheetErrorCode::Ref);
    }
    let model = exponential_model_from_data(known_y, known_x, use_const)?;
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

pub(crate) fn eval_trend_surface(
    args: &[CalcValue]
) -> Result<CalcValue, WorksheetErrorCode> {
    let known_y = collect_numeric_vector_arg(&args[0])?;
    let known_x_raw = args
        .get(1)
        .filter(|arg| !matches!(arg.core(), CoreValue::Missing))
        .map(|arg| collect_numeric_matrix_arg(arg))
        .transpose()?;
    let known_x = predictor_input_from_optional(known_x_raw, &known_y)?;
    let new_x_raw = args
        .get(2)
        .filter(|arg| !matches!(arg.core(), CoreValue::Missing))
        .map(|arg| collect_numeric_matrix_arg(arg))
        .transpose()?;
    let new_x = prediction_input_from_optional(new_x_raw, &known_y, &known_x)?;
    let use_const =
        bool_flag_from_eval_or_default(optional_arg_value(args.get(3))?, true)?;
    trend_kernel(&known_y, &known_x, &new_x, use_const)
}

pub(crate) fn eval_growth_surface(
    args: &[CalcValue]
) -> Result<CalcValue, WorksheetErrorCode> {
    let known_y = collect_numeric_vector_arg(&args[0])?;
    let known_x_raw = args
        .get(1)
        .filter(|arg| !matches!(arg.core(), CoreValue::Missing))
        .map(|arg| collect_numeric_matrix_arg(arg))
        .transpose()?;
    let known_x = predictor_input_from_optional(known_x_raw, &known_y)?;
    let new_x_raw = args
        .get(2)
        .filter(|arg| !matches!(arg.core(), CoreValue::Missing))
        .map(|arg| collect_numeric_matrix_arg(arg))
        .transpose()?;
    let new_x = prediction_input_from_optional(new_x_raw, &known_y, &known_x)?;
    let use_const =
        bool_flag_from_eval_or_default(optional_arg_value(args.get(3))?, true)?;
    growth_kernel(&known_y, &known_x, &new_x, use_const)
}

pub(crate) fn eval_forecast_surface(
    args: &[CalcValue]
) -> Result<CalcValue, WorksheetErrorCode> {
    let x = numeric_scalar_from_eval(&resolve_arg_eval(&args[0])?)?;
    let known_y = collect_numeric_vector_arg(&args[1])?;
    let known_x = collect_numeric_vector_arg(&args[2])?;
    forecast_pair_kernel(x, &known_y, &known_x)
}

pub(crate) fn eval_forecast_linear_surface(
    args: &[CalcValue]
) -> Result<CalcValue, WorksheetErrorCode> {
    let x = numeric_scalar_from_eval(&resolve_arg_eval(&args[0])?)?;
    let known_y = collect_numeric_vector_arg(&args[1])?;
    let known_x = collect_numeric_vector_arg(&args[2])?;
    forecast_pair_kernel(x, &known_y, &known_x)
}

pub(crate) fn eval_linest_surface(
    args: &[CalcValue]
) -> Result<CalcValue, WorksheetErrorCode> {
    let known_y = collect_numeric_vector_arg(&args[0])?;
    let known_x_raw = args
        .get(1)
        .filter(|arg| !matches!(arg.core(), CoreValue::Missing))
        .map(|arg| collect_numeric_matrix_arg(arg))
        .transpose()?;
    let known_x = predictor_input_from_optional(known_x_raw, &known_y)?;
    let use_const =
        bool_flag_from_eval_or_default(optional_arg_value(args.get(2))?, true)?;
    let stats = bool_flag_from_eval_or_default(optional_arg_value(args.get(3))?, false)?;
    linest_kernel(&known_y, &known_x, use_const, stats)
}

pub(crate) fn eval_logest_surface(
    args: &[CalcValue]
) -> Result<CalcValue, WorksheetErrorCode> {
    let known_y = collect_numeric_vector_arg(&args[0])?;
    let known_x_raw = args
        .get(1)
        .filter(|arg| !matches!(arg.core(), CoreValue::Missing))
        .map(|arg| collect_numeric_matrix_arg(arg))
        .transpose()?;
    let known_x = predictor_input_from_optional(known_x_raw, &known_y)?;
    let use_const =
        bool_flag_from_eval_or_default(optional_arg_value(args.get(2))?, true)?;
    let stats = bool_flag_from_eval_or_default(optional_arg_value(args.get(3))?, false)?;
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
            let CoreValue::Number(v) = got.core() else {
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
        match value.core() {
            CoreValue::Number(n) => *n,
            other => panic!("expected scalar, got {other:?}"),
        }
    }

    fn expect_row_array(value: CalcValue) -> Vec<f64> {
        match value.core() {
            CoreValue::Array(array) => {
                assert_eq!(
                    array.shape(),
                    ArrayShape {
                        rows: 1,
                        cols: array.shape().cols
                    }
                );
                array
                    .iter_row_major()
                    .map(|cell| match cell.core() {
                        CoreValue::Number(n) => *n,
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
        let got = eval_trend_surface(
            &[row(&[2.0, 4.0, 6.0])]
        )
        .unwrap();
        let values = expect_row_array(got);
        assert_eq!(values.len(), 3);
        assert_close(values[0], 2.0);
        assert_close(values[1], 4.0);
        assert_close(values[2], 6.0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn trend_projects_column_vector() {
        let got = eval_trend_surface(
            &[
                col(&[2.0, 4.0, 6.0]),
                col(&[1.0, 2.0, 3.0]),
                col(&[4.0, 5.0]),
            ]
        )
        .unwrap();
        match got.core() {
            CoreValue::Array(array) => {
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
        let got = eval_growth_surface(
            &[
                col(&[2.0, 4.0, 8.0]),
                CalcValue::missing(),
                col(&[4.0, 5.0]),
            ]
        )
        .unwrap();
        match got.core() {
            CoreValue::Array(array) => {
                assert_close(numeric_from_cell(array.get(0, 0).unwrap()).unwrap(), 16.0);
                assert_close(numeric_from_cell(array.get(1, 0).unwrap()).unwrap(), 32.0);
            }
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn growth_projects_multivariate_rows_to_column_result() {
        let got = eval_growth_surface(
            &[
                col(&[9.0, 18.0, 27.0, 54.0, 72.0]),
                matrix(&[
                    &[1.0, 1.0],
                    &[2.0, 1.0],
                    &[1.0, 2.0],
                    &[2.0, 2.0],
                    &[3.0, 2.0],
                ]),
                matrix(&[&[4.0, 1.0], &[4.0, 2.0]]),
            ]
        )
        .unwrap();
        match got.core() {
            CoreValue::Array(array) => {
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
        let forecast = eval_forecast_surface(
            &[num(3.0), row(&[1.0, 2.0]), row(&[2.0, 4.0])]
        )
        .unwrap();
        let forecast_linear = eval_forecast_linear_surface(
            &[num(3.0), row(&[1.0, 2.0]), row(&[2.0, 4.0])]
        )
        .unwrap();
        assert_close(expect_number(forecast), 1.5);
        assert_close(expect_number(forecast_linear), 1.5);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn linest_returns_single_predictor_coefficients() {
        let got = eval_linest_surface(
            &[col(&[3.0, 5.0, 7.0]), col(&[1.0, 2.0, 3.0])]
        )
        .unwrap();
        let coeffs = expect_row_array(got);
        assert_eq!(coeffs.len(), 2);
        assert_close(coeffs[0], 2.0);
        assert_close(coeffs[1], 1.0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn linest_without_const_keeps_trailing_zero_intercept_cell() {
        let got = eval_linest_surface(
            &[
                col(&[2.0, 4.0, 6.0]),
                col(&[1.0, 2.0, 3.0]),
                bool_arg(false),
            ]
        )
        .unwrap();
        let coeffs = expect_row_array(got);
        // Excel 16.0.20430: =INDEX(LINEST({2;4;6},{1;2;3},FALSE),1) is 2.0000000000000004, the QR
        // fit's rounding (dc-j9k), and the intercept cell is 0.
        assert_eq!(coeffs, vec![2.0000000000000004, 0.0]);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn linest_returns_reversed_multivariate_coefficients() {
        let got = eval_linest_surface(
            &[
                col(&[6.0, 8.0, 9.0, 11.0, 10.0]),
                matrix(&[
                    &[1.0, 1.0],
                    &[2.0, 1.0],
                    &[1.0, 2.0],
                    &[2.0, 2.0],
                    &[3.0, 1.0],
                ]),
            ]
        )
        .unwrap();
        let coeffs = expect_row_array(got);
        assert_close(coeffs[0], 3.0);
        assert_close(coeffs[1], 2.0);
        assert_close(coeffs[2], 1.0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn logest_returns_growth_factor_and_base() {
        let got = eval_logest_surface(
            &[col(&[3.0, 6.0, 12.0]), col(&[1.0, 2.0, 3.0])]
        )
        .unwrap();
        let coeffs = expect_row_array(got);
        assert_eq!(coeffs.len(), 2);
        assert_close(coeffs[0], 2.0);
        assert_close(coeffs[1], 1.5);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn logest_without_const_keeps_trailing_one_base_cell() {
        let got = eval_logest_surface(
            &[
                col(&[3.0, 6.0, 12.0]),
                col(&[1.0, 2.0, 3.0]),
                bool_arg(false),
            ]
        )
        .unwrap();
        let coeffs = expect_row_array(got);
        assert_close(coeffs[0], 2.37956557896877);
        assert_close(coeffs[1], 1.0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn logest_returns_reversed_multivariate_factors() {
        let got = eval_logest_surface(
            &[
                col(&[9.0, 18.0, 27.0, 54.0, 72.0]),
                matrix(&[
                    &[1.0, 1.0],
                    &[2.0, 1.0],
                    &[1.0, 2.0],
                    &[2.0, 2.0],
                    &[3.0, 2.0],
                ]),
            ]
        )
        .unwrap();
        let coeffs = expect_row_array(got);
        assert_close(coeffs[0], 2.84212006162885);
        assert_close(coeffs[1], 1.70056600083439);
        assert_close(coeffs[2], 2.01941161117829);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn scalar_new_x_returns_scalar_trend_result() {
        let got = eval_trend_surface(
            &[col(&[2.0, 4.0, 6.0]), col(&[1.0, 2.0, 3.0]), num(4.0)]
        )
        .unwrap();
        assert_close(expect_number(got), 8.0);
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn trend_projects_matrix_shape_for_single_predictor_new_x() {
        let got = eval_trend_surface(
            &[
                col(&[2.0, 4.0, 6.0]),
                col(&[1.0, 2.0, 3.0]),
                matrix(&[&[1.0, 2.0], &[3.0, 4.0]]),
            ]
        )
        .unwrap();
        match got.core() {
            CoreValue::Array(array) => {
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
        let got = eval_trend_surface(
            &[
                col(&[6.0, 8.0, 9.0, 11.0, 10.0]),
                matrix(&[
                    &[1.0, 1.0],
                    &[2.0, 1.0],
                    &[1.0, 2.0],
                    &[2.0, 2.0],
                    &[3.0, 1.0],
                ]),
                matrix(&[&[4.0, 1.0], &[4.0, 2.0]]),
            ]
        )
        .unwrap();
        match got.core() {
            CoreValue::Array(array) => {
                assert_eq!(array.shape(), ArrayShape { rows: 2, cols: 1 });
                assert_close(numeric_from_cell(array.get(0, 0).unwrap()).unwrap(), 12.0);
                assert_close(numeric_from_cell(array.get(1, 0).unwrap()).unwrap(), 15.0);
            }
            other => panic!("expected array, got {other:?}"),
        }
    }
}
