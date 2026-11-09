//! The rounding functions F1 adds beside the rounding family, ported from OxFunc
//! (functions/ceiling_floor_family.rs at 6fca3518): CEILING.MATH, CEILING.PRECISE, FLOOR.MATH,
//! FLOOR.PRECISE and ISO.CEILING. The significance defaults to 1 and the mode to 0 when the call
//! does not reach them; an argument left empty is read as 0.

use super::kit::{num_with_subnormal, opt, out};
use crate::kernel::{Site, Step};
use crate::ported::ceiling_floor_family as cf;
use crate::value::{CalcValue, WorksheetErrorCode};

use calc_catalogue::ported::rounding_more::{CODE, ENTRIES as DECLARATIONS};

fn two(
    site: &Site<'_>,
    a: &[CalcValue],
    kernel: fn(f64, f64) -> Result<f64, WorksheetErrorCode>,
) -> Step<CalcValue> {
    let x = num_with_subnormal(site, a, 0)?;
    let significance = opt(site, a, 1, 1.0)?;
    out(site, kernel(x, significance))
}

fn three(
    site: &Site<'_>,
    a: &[CalcValue],
    kernel: fn(f64, f64, f64) -> Result<f64, WorksheetErrorCode>,
) -> Step<CalcValue> {
    let x = num_with_subnormal(site, a, 0)?;
    let significance = opt(site, a, 1, 1.0)?;
    let mode = opt(site, a, 2, 0.0)?;
    out(site, kernel(x, significance, mode))
}

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
ported_shared_family! {
    code: CODE;
    declarations: DECLARATIONS;
    1 ceiling_math |s, a, sv| {
        three(s, a, cf::ceiling_math_kernel)
    }
    2 ceiling_precise |s, a, sv| {
        two(s, a, cf::ceiling_precise_kernel)
    }
    3 floor_math |s, a, sv| {
        three(s, a, cf::floor_math_kernel)
    }
    4 floor_precise |s, a, sv| {
        two(s, a, cf::floor_precise_kernel)
    }
    5 iso_ceiling |s, a, sv| {
        two(s, a, cf::iso_ceiling_kernel)
    }
}
