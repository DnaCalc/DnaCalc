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
        // Excel rounds bottom up and top down to integers; only a raw bottom above the raw top is
        // #NUM!, and a top that floors below the rounded bottom collapses to it (dc-j9k.185).
        let bottom = num(s, a, 0)?;
        let top = num(s, a, 1)?;
        if bottom > top {
            return fail(WorksheetErrorCode::Num);
        }
        let lo = bottom.ceil();
        let hi = top.floor().max(lo);
        let unit = draw(s, sv)?;
        let range = hi - lo + 1.0;
        s.number((lo + (unit * range).floor()).min(hi))
    }
    3 randarray |s, a, sv| {
        let floor = crate::ported::text_kernels::date_integer_floor;
        let rows = if given(a, 0) { floor(num(s, a, 0)?) } else { 1.0 };
        let cols = if given(a, 1) { floor(num(s, a, 1)?) } else { 1.0 };
        let min = if given(a, 2) { num(s, a, 2)? } else { 0.0 };
        let max = if given(a, 3) { num(s, a, 3)? } else { 1.0 };
        let whole = if given(a, 4) { opt_flag(s, a, 4, false)? } else { false };
        // Excel 16.0.20527 (dc-ztvv.26.132): a floored dimension below 0 or above the sheet's
        // rows is #VALUE!, then a zero one is #CALC!; more than 16,384 columns cannot spill.
        let sheet_rows = 1_048_576.0;
        if rows < 0.0 || cols < 0.0 || rows > sheet_rows || cols > sheet_rows {
            return fail(WorksheetErrorCode::Value);
        }
        if rows == 0.0 || cols == 0.0 {
            return fail(WorksheetErrorCode::Calc);
        }
        let (lo, hi) = if whole { (min.ceil(), max.floor()) } else { (min, max) };
        if hi < lo {
            return fail(WorksheetErrorCode::Value);
        }
        // Excel 16.0.20527: a minimum above the maximum is #VALUE! before the size can spill.
        if cols > 16_384.0 {
            return fail(WorksheetErrorCode::Spill);
        }
        if rows * cols > RANDARRAY_LIMIT {
            return Err(s.not_modelled(None));
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
