use crate::value::WorksheetErrorCode;
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

fn transpose(matrix: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let rows = matrix.len();
    let cols = matrix.first().map_or(0, Vec::len);
    let mut result = vec![vec![0.0; rows]; cols];
    for (row_idx, row) in matrix.iter().enumerate() {
        for (col_idx, value) in row.iter().enumerate() {
            result[col_idx][row_idx] = *value;
        }
    }
    result
}

fn mat_mul(left: &[Vec<f64>], right: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, WorksheetErrorCode> {
    if left.is_empty() || right.is_empty() {
        return Err(WorksheetErrorCode::Value);
    }
    let left_cols = left[0].len();
    let right_cols = right[0].len();
    if left.iter().any(|row| row.len() != left_cols)
        || right.iter().any(|row| row.len() != right_cols)
        || left_cols != right.len()
    {
        return Err(WorksheetErrorCode::Value);
    }
    let mut result = vec![vec![0.0; right_cols]; left.len()];
    for row in 0..left.len() {
        for col in 0..right_cols {
            let mut sum = 0.0;
            for idx in 0..left_cols {
                sum += left[row][idx] * right[idx][col];
            }
            result[row][col] = sum;
        }
    }
    Ok(result)
}

fn inverse_square(matrix: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, WorksheetErrorCode> {
    let n = matrix.len();
    if n == 0 || matrix.iter().any(|row| row.len() != n) {
        return Err(WorksheetErrorCode::Value);
    }

    let mut augmented = Vec::with_capacity(n);
    for (row_idx, row) in matrix.iter().enumerate() {
        let mut augmented_row = Vec::with_capacity(n * 2);
        augmented_row.extend_from_slice(row);
        for col in 0..n {
            augmented_row.push(if row_idx == col { 1.0 } else { 0.0 });
        }
        augmented.push(augmented_row);
    }

    const EPS: f64 = 1e-12;
    for pivot_idx in 0..n {
        let mut pivot_row = pivot_idx;
        let mut pivot_abs = augmented[pivot_idx][pivot_idx].abs();
        for row in (pivot_idx + 1)..n {
            let candidate = augmented[row][pivot_idx].abs();
            if candidate > pivot_abs {
                pivot_abs = candidate;
                pivot_row = row;
            }
        }

        if pivot_abs < EPS {
            return Err(WorksheetErrorCode::Num);
        }

        if pivot_row != pivot_idx {
            augmented.swap(pivot_row, pivot_idx);
        }

        let pivot = augmented[pivot_idx][pivot_idx];
        for col in 0..(n * 2) {
            augmented[pivot_idx][col] /= pivot;
        }

        for row in 0..n {
            if row == pivot_idx {
                continue;
            }
            let factor = augmented[row][pivot_idx];
            if factor == 0.0 {
                continue;
            }
            for col in 0..(n * 2) {
                augmented[row][col] -= factor * augmented[pivot_idx][col];
            }
        }
    }

    Ok(augmented
        .into_iter()
        .map(|row| row[n..].to_vec())
        .collect::<Vec<_>>())
}

fn solve_least_squares(x: &[Vec<f64>], y: &[f64]) -> Result<Vec<f64>, WorksheetErrorCode> {
    let xt = transpose(x);
    let y_column = y
        .iter()
        .copied()
        .map(|value| vec![value])
        .collect::<Vec<_>>();
    let xtx = mat_mul(&xt, x)?;
    let xty = mat_mul(&xt, &y_column)?;
    let inv = inverse_square(&xtx)?;
    let beta = mat_mul(&inv, &xty)?;
    Ok(beta.into_iter().map(|row| row[0]).collect())
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

    let mut design = Vec::with_capacity(observation_count);
    for row_idx in 0..observation_count {
        let mut row = known_x.predictors.row_values(row_idx).to_vec();
        if use_const {
            row.push(1.0);
        }
        design.push(row);
    }

    let beta = solve_least_squares(&design, &known_y.values)?;
    if use_const {
        let intercept = *beta.last().expect("const=true keeps intercept coefficient");
        Ok(LinearRegressionModel {
            coefficients: beta[..beta.len() - 1].to_vec(),
            intercept,
        })
    } else {
        Ok(LinearRegressionModel {
            coefficients: beta,
            intercept: 0.0,
        })
    }
}

fn exponential_model_from_data(
    known_y: &NumericVector,
    known_x: &PreparedPredictorInput,
    use_const: bool,
) -> Result<(Vec<f64>, f64), WorksheetErrorCode> {
    let mut log_y = Vec::with_capacity(known_y.values.len());
    for value in &known_y.values {
        if *value <= 0.0 || !value.is_finite() {
            return Err(WorksheetErrorCode::Num);
        }
        log_y.push(value.ln());
    }
    let logged = NumericVector {
        values: log_y,
        shape: known_y.shape,
    };
    let model = linear_model_from_data(&logged, known_x, use_const)?;
    Ok((
        model
            .coefficients
            .iter()
            .copied()
            .map(f64::exp)
            .collect::<Vec<_>>(),
        model.intercept.exp(),
    ))
}
