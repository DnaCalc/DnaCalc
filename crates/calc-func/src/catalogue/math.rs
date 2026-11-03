//! The math and trigonometry family fragment ported from OxFunc (functions/ at 6fca3518): the
//! circular and hyperbolic functions and their inverses, exponentials and logarithms, roots,
//! factorials and combinatorics, GCD and LCM, the numeral-system conversions, the paired sums of
//! squares, SERIESSUM and the matrix functions. The numeric kernels are OxFunc's, in
//! [`crate::ported`]; arguments are read by worksheet coercion ([`super::kit`]).
//!
//! POWER is the `^` operator's rule ([`super::arithmetic::raise`]).

use super::kit::{Grid, fail, given, grid, num, num_with_subnormal, out, out_matrix, out_text, reject_logical, text, values};
use crate::kernel::{Exit, Site, Step};
use crate::ported::real_policy::ExcelRealPolicy;
use crate::ported::{self as k};
use crate::value::{CalcValue, CoreRef, ExcelText, WorksheetErrorCode};
use crate::ported::excel_numeric::crt_pow;

/// F1's families take codes from 100 up, clear of the other lanes' families.
const CODE: u16 = 100;

/// A one-argument kernel whose raw result Excel publishes under `policy`.
fn raw(site: &Site<'_>, a: &[CalcValue], policy: ExcelRealPolicy, kernel: fn(f64) -> f64) -> Step<CalcValue> {
    let x = num(site, a, 0)?;
    out(site, policy.publish(x, kernel(x)))
}

/// A one-argument kernel that publishes its own result.
fn unary(site: &Site<'_>, a: &[CalcValue], kernel: fn(f64) -> Result<f64, WorksheetErrorCode>) -> Step<CalcValue> {
    let x = num(site, a, 0)?;
    out(site, kernel(x))
}

/// A two-argument kernel.
fn binary(
    site: &Site<'_>,
    a: &[CalcValue],
    kernel: fn(f64, f64) -> Result<f64, WorksheetErrorCode>,
) -> Step<CalcValue> {
    let x = num(site, a, 0)?;
    let y = num(site, a, 1)?;
    out(site, kernel(x, y))
}

ported_family! {
    code: CODE;
    1 acos "ACOS" (1, 1) [V "number"] |s, a, sv| { unary(s, a, k::acos::acos_kernel) }
    2 acosh "ACOSH" (1, 1) [V "number"] |s, a, sv| { unary(s, a, k::acosh::acosh_kernel) }
    3 acot "ACOT" (1, 1) [V "number"] |s, a, sv| { unary(s, a, k::acot::acot_kernel) }
    4 acoth "ACOTH" (1, 1) [V "number"] |s, a, sv| { unary(s, a, k::acoth::acoth_kernel) }
    5 asin "ASIN" (1, 1) [V "number"] |s, a, sv| { unary(s, a, k::asin::asin_kernel) }
    6 asinh "ASINH" (1, 1) [V "number"] |s, a, sv| { unary(s, a, k::asinh::asinh_kernel) }
    7 atan "ATAN" (1, 1) [V "number"] |s, a, sv| {
        raw(s, a, ExcelRealPolicy::PASS, k::atan::atan_kernel)
    }
    8 atan2 "ATAN2" (2, 2) [V "x_num", V "y_num"] |s, a, sv| {
        binary(s, a, k::atan2::atan2_kernel)
    }
    9 atanh "ATANH" (1, 1) [V "number"] |s, a, sv| {
        out(s, k::atanh::atanh_kernel(num_with_subnormal(s, a, 0)?))
    }
    10 cos "COS" (1, 1) [V "number"] |s, a, sv| {
        raw(s, a, ExcelRealPolicy::CIRCULAR_TRIG, k::cos::cos_kernel)
    }
    11 cosh "COSH" (1, 1) [V "number"] |s, a, sv| {
        raw(s, a, ExcelRealPolicy::FINITE, k::cosh::cosh_kernel)
    }
    12 cot "COT" (1, 1) [V "number"] |s, a, sv| { unary(s, a, k::cot::cot_kernel) }
    13 coth "COTH" (1, 1) [V "number"] |s, a, sv| { unary(s, a, k::coth::coth_kernel) }
    14 csc "CSC" (1, 1) [V "number"] |s, a, sv| { unary(s, a, k::csc::csc_kernel) }
    15 csch "CSCH" (1, 1) [V "number"] |s, a, sv| { unary(s, a, k::csch::csch_kernel) }
    16 sec "SEC" (1, 1) [V "number"] |s, a, sv| { unary(s, a, k::sec::sec_kernel) }
    17 sech "SECH" (1, 1) [V "number"] |s, a, sv| { unary(s, a, k::sech::sech_kernel) }
    18 sin "SIN" (1, 1) [V "number"] |s, a, sv| {
        raw(s, a, ExcelRealPolicy::CIRCULAR_TRIG, k::sin::sin_kernel)
    }
    19 sinh "SINH" (1, 1) [V "number"] |s, a, sv| {
        raw(s, a, ExcelRealPolicy::FINITE, k::sinh::sinh_kernel)
    }
    20 tan "TAN" (1, 1) [V "number"] |s, a, sv| {
        raw(s, a, ExcelRealPolicy::CIRCULAR_TRIG, k::tan::tan_kernel)
    }
    21 tanh "TANH" (1, 1) [V "number"] |s, a, sv| {
        raw(s, a, ExcelRealPolicy::SATURATE_SIGN, k::tanh::tanh_kernel)
    }
    22 degrees "DEGREES" (1, 1) [V "angle"] |s, a, sv| {
        raw(s, a, ExcelRealPolicy::FINITE, k::degrees::degrees_kernel)
    }
    23 radians "RADIANS" (1, 1) [V "angle"] |s, a, sv| {
        raw(s, a, ExcelRealPolicy::PASS, k::radians::radians_kernel)
    }
    24 pi "PI" (0, 0) [] |s, a, sv| { s.number(std::f64::consts::PI) }
    25 exp "EXP" (1, 1) [V "number"] |s, a, sv| {
        raw(s, a, ExcelRealPolicy::FINITE, k::exp_fn::exp_kernel)
    }
    26 ln "LN" (1, 1) [V "number"] |s, a, sv| { unary(s, a, k::ln_fn::ln_kernel) }
    27 log "LOG" (1, 2) [V "number", V "base"] |s, a, sv| {
        // Omitting the base selects the decimal logarithm; a base left empty is 0.
        let x = num(s, a, 0)?;
        if a.len() == 1 {
            out(s, k::log10_fn::log10_kernel(x))
        } else {
            let base = num(s, a, 1)?;
            out(s, k::log_fn::log_kernel(x, base))
        }
    }
    28 log10 "LOG10" (1, 1) [V "number"] |s, a, sv| { unary(s, a, k::log10_fn::log10_kernel) }
    29 sqrt "SQRT" (1, 1) [V "number"] |s, a, sv| { unary(s, a, k::sqrt_fn::sqrt_kernel) }
    30 sqrtpi "SQRTPI" (1, 1) [V "number"] |s, a, sv| {
        reject_logical(&a[0])?;
        unary(s, a, k::sqrtpi::sqrtpi_kernel)
    }
    31 power "POWER" (2, 2) [V "number", V "power"] |s, a, sv| {
        let x = num(s, a, 0)?;
        let y = num(s, a, 1)?;
        s.number(super::arithmetic::raise(s, x, y)?)
    }
    32 fact "FACT" (1, 1) [V "number"] |s, a, sv| { unary(s, a, k::fact::fact_kernel) }
    33 factdouble "FACTDOUBLE" (1, 1) [V "number"] |s, a, sv| {
        reject_logical(&a[0])?;
        unary(s, a, k::factdouble::factdouble_kernel)
    }
    34 combin "COMBIN" (2, 2) [V "number", V "number_chosen"] |s, a, sv| {
        out(s, k::combin::combin_kernel(num_with_subnormal(s, a, 0)?, num_with_subnormal(s, a, 1)?))
    }
    35 combina "COMBINA" (2, 2) [V "number", V "number_chosen"] |s, a, sv| {
        out(s, k::combina::combina_kernel(num_with_subnormal(s, a, 0)?, num_with_subnormal(s, a, 1)?))
    }
    36 permut "PERMUT" (2, 2) [V "number", V "number_chosen"] |s, a, sv| {
        binary(s, a, k::permut_fn::permut_kernel)
    }
    37 permutationa "PERMUTATIONA" (2, 2) [V "number", V "number_chosen"] |s, a, sv| {
        binary(s, a, k::permutationa_fn::permutationa_kernel)
    }
    38 gcd "GCD" (1, 255) [R "number1", R "number2"] |s, a, sv| {
        let items = integer_groups(s, a, sv)?;
        s.number(k::gcd_fn::gcd_kernel(&items))
    }
    39 lcm "LCM" (1, 255) [R "number1", R "number2"] |s, a, sv| {
        let items = integer_groups(s, a, sv)?;
        out(s, k::lcm_fn::lcm_kernel(&items))
    }
    40 multinomial "MULTINOMIAL" (1, 255) [R "number1", R "number2"] |s, a, sv| {
        reject_logical(&a[0])?;
        // Every value, held or direct, is read as a number and truncated; a blank cell is
        // skipped.
        let mut items = Vec::new();
        values(s, a, 0, a.len(), sv, &mut |held, value| {
            if held && matches!(value.core_ref(), CoreRef::Empty) {
                return Ok(());
            }
            let n = s.number_argument(0, value)?;
            if !n.is_finite() || n < 0.0 {
                return fail(WorksheetErrorCode::Num);
            }
            items.push(n.trunc() as i64);
            Ok(())
        })?;
        out(s, k::multinomial::multinomial_kernel(&items))
    }
    41 arabic "ARABIC" (1, 1) [V "text"] |s, a, sv| {
        let t = text(s, a, 0)?;
        out(s, k::arabic_fn::arabic_kernel(&ExcelText::from_interop_assignment(&t)))
    }
    42 roman "ROMAN" (1, 2) [V "number", V "form"] |s, a, sv| {
        let n = num(s, a, 0)?;
        let form = match a.get(1).map(CalcValue::core_ref) {
            None | Some(CoreRef::Missing | CoreRef::Empty | CoreRef::Logical(true)) => 0,
            Some(CoreRef::Logical(false)) => 4,
            Some(_) => {
                let f = num(s, a, 1)?.trunc();
                if !(0.0..=4.0).contains(&f) {
                    return fail(WorksheetErrorCode::Value);
                }
                f as i32
            }
        };
        out_text(k::roman_fn::roman_kernel(n, form))
    }
    43 base "BASE" (2, 3) [V "number", V "radix", V "min_length"] |s, a, sv| {
        let n = num(s, a, 0)?;
        let radix = num(s, a, 1)?;
        let min_length = if given(a, 2) { Some(num(s, a, 2)?) } else { None };
        out_text(k::base_fn::base_kernel(n, radix, min_length))
    }
    44 decimal "DECIMAL" (2, 2) [V "text", V "radix"] |s, a, sv| {
        let t = text(s, a, 0)?;
        let radix = num(s, a, 1)?;
        out(s, k::decimal_fn::decimal_kernel(&t, radix))
    }
    45 seriessum "SERIESSUM" (4, 4) [V "x", V "n", V "m", R "coefficients"] |s, a, sv| {
        reject_logical(&a[0])?;
        let x = num(s, a, 0)?;
        let n = num(s, a, 1)?;
        let m = num(s, a, 2)?;
        let coefficients = grid(s, a, 3, sv)?;
        let mut sum = 0.0;
        for (i, cell) in coefficients.cells.iter().enumerate() {
            let c = match cell.core_ref() {
                CoreRef::Number(c) => c,
                CoreRef::Error(code) => return fail(code),
                _ => return fail(WorksheetErrorCode::Value),
            };
            sum += c * crt_pow(x, n + (i as f64) * m);
        }
        s.number(sum)
    }
    46 sumx2my2 "SUMX2MY2" (2, 2) [R "array_x", R "array_y"] |s, a, sv| {
        sum_pairwise(s, a, sv, |x, y| x * x - y * y)
    }
    47 sumx2py2 "SUMX2PY2" (2, 2) [R "array_x", R "array_y"] |s, a, sv| {
        sum_pairwise(s, a, sv, |x, y| x * x + y * y)
    }
    48 sumxmy2 "SUMXMY2" (2, 2) [R "array_x", R "array_y"] |s, a, sv| {
        sum_pairwise(s, a, sv, |x, y| (x - y) * (x - y))
    }
    49 mdeterm "MDETERM" (1, 1) [R "array"] |s, a, sv| {
        let m = matrix(s, a, 0, sv)?;
        out(s, k::matrix_family::determinant_kernel(&m))
    }
    50 minverse "MINVERSE" (1, 1) [R "array"] |s, a, sv| {
        let m = matrix(s, a, 0, sv)?;
        let inverse = k::matrix_family::inverse_kernel(&m).map_err(Exit::Error)?;
        out_matrix(s, &inverse)
    }
    51 mmult "MMULT" (2, 2) [R "array1", R "array2"] |s, a, sv| {
        let left = matrix(s, a, 0, sv)?;
        let right = matrix(s, a, 1, sv)?;
        let product = k::matrix_family::mmult_kernel(&left, &right).map_err(Exit::Error)?;
        out_matrix(s, &product)
    }
    52 munit "MUNIT" (1, 1) [V "dimension"] |s, a, sv| {
        let n = num(s, a, 0)?.trunc();
        if !(1.0..=1_048_576.0).contains(&n) {
            return fail(WorksheetErrorCode::Value);
        }
        let size = n as usize;
        if size > 4096 {
            // A spill of more than 16M cells is not modelled.
            return Err(s.not_modelled(Some(0)));
        }
        let rows: Vec<Vec<f64>> = (0..size)
            .map(|r| (0..size).map(|c| if r == c { 1.0 } else { 0.0 }).collect())
            .collect();
        out_matrix(s, &rows)
    }
}

/// GCD's and LCM's integers: each argument's numbers (a held logical is #VALUE!, a blank cell of
/// a multi-cell argument is 0 and a direct omitted argument after the first is skipped), each at
/// least 0 and at most 2^53, truncated.
fn integer_groups(
    site: &Site<'_>,
    a: &[CalcValue],
    services: &mut crate::call::EvalServices<'_>,
) -> Step<Vec<i64>> {
    let mut numbers = Vec::new();
    let mut any = false;
    for (index, argument) in a.iter().enumerate() {
        if argument.is_missing() {
            if index == 0 {
                return fail(WorksheetErrorCode::NA);
            }
            continue;
        }
        let Grid { cells, held, .. } = grid(site, a, index, services)?;
        let multi = cells.len() > 1;
        for cell in &cells {
            match cell.core_ref() {
                CoreRef::Empty | CoreRef::Missing => {
                    if multi {
                        numbers.push(0.0);
                        any = true;
                    }
                }
                CoreRef::Logical(_) => return fail(WorksheetErrorCode::Value),
                _ => {
                    numbers.push(site.number_argument(index, cell)?);
                    any = true;
                }
            }
        }
        let _ = held;
    }
    if !any {
        return fail(WorksheetErrorCode::Value);
    }
    if numbers
        .iter()
        .any(|n| !n.is_finite() || *n < 0.0 || *n > k::gcd_lcm_common::GCD_LCM_LIMIT)
    {
        return fail(WorksheetErrorCode::Num);
    }
    Ok(numbers.into_iter().map(|n| n.trunc() as i64).collect())
}

/// SUMX2MY2, SUMX2PY2 and SUMXMY2: `term` summed over the positions where both arguments hold a
/// number; arguments of different shapes are #N/A, an error is the result, and no such position
/// is #DIV/0!.
fn sum_pairwise(
    site: &Site<'_>,
    a: &[CalcValue],
    services: &mut crate::call::EvalServices<'_>,
    term: fn(f64, f64) -> f64,
) -> Step<CalcValue> {
    let left = numeric_cells(site, a, 0, services)?;
    let right = numeric_cells(site, a, 1, services)?;
    if left.0 != right.0 {
        return fail(WorksheetErrorCode::NA);
    }
    let mut seen = false;
    let mut sum = 0.0;
    for (x, y) in left.1.iter().zip(&right.1) {
        if let (Some(x), Some(y)) = (x, y) {
            seen = true;
            sum += term(*x, *y);
        }
    }
    if !seen {
        return fail(WorksheetErrorCode::Div0);
    }
    site.number(sum)
}

/// An argument's shape (rows, columns) and its cells' numbers.
type NumericCells = ((usize, usize), Vec<Option<f64>>);

/// An argument's shape and, per cell, its number (`None` for text, a logical or a blank); an
/// error cell is the result.
fn numeric_cells(
    site: &Site<'_>,
    a: &[CalcValue],
    index: usize,
    services: &mut crate::call::EvalServices<'_>,
) -> Step<NumericCells> {
    let g = grid(site, a, index, services)?;
    let mut cells = Vec::with_capacity(g.cells.len());
    for cell in &g.cells {
        cells.push(match cell.core_ref() {
            CoreRef::Number(n) => Some(n),
            CoreRef::Error(code) => return fail(code),
            _ => None,
        });
    }
    Ok(((g.rows, g.cols), cells))
}

/// An argument as a matrix of numbers: every cell must hold a number (an error cell is the
/// result, anything else #VALUE!); a single direct value must be a number.
fn matrix(
    site: &Site<'_>,
    a: &[CalcValue],
    index: usize,
    services: &mut crate::call::EvalServices<'_>,
) -> Step<Vec<Vec<f64>>> {
    let g = grid(site, a, index, services)?;
    let mut rows = Vec::with_capacity(g.rows);
    for r in 0..g.rows {
        let mut row = Vec::with_capacity(g.cols);
        for cell in &g.cells[r * g.cols..(r + 1) * g.cols] {
            match cell.core_ref() {
                CoreRef::Number(n) => row.push(n),
                CoreRef::Error(code) => return fail(code),
                _ => return fail(WorksheetErrorCode::Value),
            }
        }
        rows.push(row);
    }
    Ok(rows)
}
