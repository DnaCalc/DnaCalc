//! The rounding functions F1 adds beside the rounding family, ported from OxFunc
//! (functions/ceiling_floor_family.rs at 6fca3518): CEILING.MATH, CEILING.PRECISE, FLOOR.MATH,
//! FLOOR.PRECISE and ISO.CEILING. The significance defaults to 1 and the mode to 0 when the call
//! does not reach them; an argument left empty is read as 0.

use super::kit::{num, opt, out};
use crate::kernel::{Site, Step};
use crate::ported::ceiling_floor_family as cf;
use crate::value::{CalcValue, WorksheetErrorCode};

const CODE: u16 = 101;

fn two(
    site: &Site<'_>,
    a: &[CalcValue],
    kernel: fn(f64, f64) -> Result<f64, WorksheetErrorCode>,
) -> Step<CalcValue> {
    let x = num(site, a, 0)?;
    let significance = opt(site, a, 1, 1.0)?;
    out(site, kernel(x, significance))
}

fn three(
    site: &Site<'_>,
    a: &[CalcValue],
    kernel: fn(f64, f64, f64) -> Result<f64, WorksheetErrorCode>,
) -> Step<CalcValue> {
    let x = num(site, a, 0)?;
    let significance = opt(site, a, 1, 1.0)?;
    let mode = opt(site, a, 2, 0.0)?;
    out(site, kernel(x, significance, mode))
}

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
ported_family! {
    code: CODE;
    1 ceiling_math "CEILING.MATH" (1, 3) [V "number", V "significance", V "mode"] three_d (None, None, Some(&[])); |s, a, sv| {
        three(s, a, cf::ceiling_math_kernel)
    }
    2 ceiling_precise "CEILING.PRECISE" (1, 2) [V "number", V "significance"] three_d (None, None, Some(&[])); |s, a, sv| {
        two(s, a, cf::ceiling_precise_kernel)
    }
    3 floor_math "FLOOR.MATH" (1, 3) [V "number", V "significance", V "mode"] three_d (None, None, Some(&[])); |s, a, sv| {
        three(s, a, cf::floor_math_kernel)
    }
    4 floor_precise "FLOOR.PRECISE" (1, 2) [V "number", V "significance"] three_d (None, None, Some(&[])); |s, a, sv| {
        two(s, a, cf::floor_precise_kernel)
    }
    5 iso_ceiling "ISO.CEILING" (1, 2) [V "number", V "significance"] three_d (None, None, Some(&[])); |s, a, sv| {
        two(s, a, cf::iso_ceiling_kernel)
    }
}
