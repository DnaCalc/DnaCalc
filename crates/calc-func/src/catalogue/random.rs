//! The random-number family fragment, after OxFunc (functions/rand_fn.rs, randbetween_fn.rs and
//! misc_conversion_family.rs's RANDARRAY at 6fca3518): RAND, RANDBETWEEN and RANDARRAY. Each
//! draws from the services' random source ([`crate::RandomSource`]); without one the call is
//! refused as not modelled. A source that gives a number outside [0, 1) is a fault of the source
//! and is refused too. Recalculating these volatile functions is the engine's business (L2).

use super::kit::{fail, given, num, opt_flag, out_matrix};
use crate::call::EvalServices;
use crate::kernel::{Site, Step};
use crate::value::WorksheetErrorCode;

use calc_catalogue::ported::random::{CODE, ENTRIES as DECLARATIONS};

/// The next number of the services' random source, in [0, 1).
fn draw(site: &Site<'_>, sv: &mut EvalServices<'_>) -> Step<f64> {
    let Some(source) = sv.random() else {
        return Err(site.not_modelled(None));
    };
    let unit = source.next_unit();
    if unit.is_finite() && (0.0..1.0).contains(&unit) {
        Ok(unit)
    } else {
        Err(site.not_modelled(None))
    }
}

/// The largest RANDARRAY this family builds; a larger spill is not modelled.
const RANDARRAY_LIMIT: f64 = 1_048_576.0;

ported_shared_family! {
    code: CODE;
    declarations: DECLARATIONS;
    1 rand |s, a, sv| {
        let unit = draw(s, sv)?;
        s.number(unit)
    }
    2 randbetween |s, a, sv| {
        // Excel rounds bottom up and top down to integers.
        let lo = num(s, a, 0)?.ceil();
        let hi = num(s, a, 1)?.floor();
        if lo > hi {
            return fail(WorksheetErrorCode::Num);
        }
        let unit = draw(s, sv)?;
        let range = hi - lo + 1.0;
        s.number((lo + (unit * range).floor()).min(hi))
    }
    3 randarray |s, a, sv| {
        let rows = if given(a, 0) { num(s, a, 0)?.trunc() } else { 1.0 };
        let cols = if given(a, 1) { num(s, a, 1)?.trunc() } else { 1.0 };
        let min = if given(a, 2) { num(s, a, 2)? } else { 0.0 };
        let max = if given(a, 3) { num(s, a, 3)? } else { 1.0 };
        let whole = if given(a, 4) { opt_flag(s, a, 4, false)? } else { false };
        if rows < 1.0 || cols < 1.0 {
            return fail(WorksheetErrorCode::Value);
        }
        if rows * cols > RANDARRAY_LIMIT {
            return Err(s.not_modelled(None));
        }
        let (lo, hi) = if whole { (min.ceil(), max.floor()) } else { (min, max) };
        if hi < lo {
            return fail(WorksheetErrorCode::Value);
        }
        let mut cells = Vec::with_capacity(rows as usize);
        for _ in 0..rows as usize {
            let mut row = Vec::with_capacity(cols as usize);
            for _ in 0..cols as usize {
                let unit = draw(s, sv)?;
                row.push(if whole {
                    let count = hi - lo + 1.0;
                    lo + (unit * count).floor().min(count - 1.0)
                } else if hi == lo {
                    lo
                } else {
                    lo + unit * (hi - lo)
                });
            }
            cells.push(row);
        }
        out_matrix(s, &cells)
    }
}
