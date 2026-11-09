//! The reference-offset family fragment: OFFSET, and ISOMITTED, which LAMBDA parameters need.
//!
//! Ported from OxFunc `crates/oxfunc_core/src/functions/offset.rs` (`eval_offset_surface`), which
//! shifts reference text through a provider of reference text; here the reference is calc-value's
//! typed area and the shift is computed directly. Where OxFunc and Excel differ, Excel's measured
//! behaviour is kept (Excel 16.0 through COM, 2026-10-23):
//!
//! - `rows` and `cols` read as numbers, truncated; an omitted argument or a blank cell is 0
//!   (`=OFFSET(A1,,1)` is B1's value, and `=OFFSET(A1,G20,0)` over a blank G20 is A1's), where
//!   OxFunc gave #VALUE!.
//! - `height` and `width`, when given, are truncated; 0 is #REF!, and a negative one extends the
//!   area up or left from the shifted cell (`=OFFSET(A5,0,0,-2)` is A4:A5, a spill), where OxFunc
//!   gave #REF!. Omitted, they are the reference's own.
//! - A reference of more than one area is #VALUE!; an area that leaves the grid is #REF!.
//!
//! An array given to a numeric argument lifts in calc-fml, which owns lifting; this kernel
//! refuses one. OFFSET's result is a reference that the formula's own references do not name:
//! the caller learns what it read from the evaluator, as for INDIRECT.

use super::{CatalogueEntry, Family, Kernel};
#[cfg(test)]
use super::FunctionId;
use crate::call::{EvalServices, FunctionFault};
use crate::kernel::{Exit, Site, Step};
use crate::value::{
    CalcArray, CalcValue, CellKey, CoreRef, GridArea, MAX_COLUMNS, MAX_ROWS,
    ReferenceLike, WorksheetErrorCode,
};
use std::ops::ControlFlow;

use calc_catalogue::offset::{CODE, ENTRIES as DECLARATIONS, ISOMITTED, OFFSET, TRIMRANGE};
use calc_catalogue::WorksheetDeclaration;

const fn attach(data: WorksheetDeclaration) -> CatalogueEntry {
    let kernel = match data.id.raw() {
        x if x == OFFSET.raw() => Kernel::Eager(offset),
        x if x == ISOMITTED.raw() => Kernel::Eager(isomitted),
        x if x == TRIMRANGE.raw() => Kernel::Eager(trimrange),
        _ => panic!("a worksheet declaration has no attached kernel or refusal"),
    };
    CatalogueEntry {
        id: data.id, spelling: data.spelling, notation: data.notation,
        kind: data.kind, availability: data.availability,
        arity: data.arity, groups: data.groups, parameters: data.parameters,
        result: data.result, demand: data.demand, traits: data.traits, kernel,
    }
}

const ENTRIES: [CatalogueEntry; 3] = [attach(DECLARATIONS[0]), attach(DECLARATIONS[1]), attach(DECLARATIONS[2])];
const _: () = assert!(DECLARATIONS.len() == ENTRIES.len());
pub(super) const FAMILY: Family = Family { code: CODE, entries: &ENTRIES };

fn offset(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    // dc-ylr: Excel gives OFFSET of a reference to a closed workbook #VALUE!.
    let external = arguments[0].as_reference().is_some_and(|reference| {
        reference.areas().iter().any(|area| {
            services
                .host()
                .is_some_and(|host| host.is_external_sheet(area.sheet()))
        })
    });
    let site = Site::new(OFFSET, services);
    if external {
        return Site::finish(Err(Exit::Error(WorksheetErrorCode::Value)));
    }
    Site::finish(shifted(&site, arguments))
}

fn shifted(site: &Site<'_>, arguments: &[CalcValue]) -> Step<CalcValue> {
    site.scalar_operands(arguments)?;
    let Some(reference) = arguments[0].as_reference() else {
        return Err(Exit::Error(WorksheetErrorCode::Value));
    };
    let [base] = reference.areas() else {
        return Err(Exit::Error(WorksheetErrorCode::Value));
    };
    let rows = whole(site, 1, &arguments[1])?;
    let cols = whole(site, 2, &arguments[2])?;
    let height = extent(site, 3, arguments.get(3), base.row_count())?;
    let width = extent(site, 4, arguments.get(4), base.column_count())?;
    let top = i64::from(base.first().row()) + rows;
    let left = i64::from(base.first().column()) + cols;
    let (top, bottom) = span(top, height);
    let (left, right) = span(left, width);
    let on_grid = |low: i64, high: i64, max: u32| low >= 1 && high <= i64::from(max);
    if !on_grid(top, bottom, MAX_ROWS) || !on_grid(left, right, MAX_COLUMNS) {
        return Err(Exit::Error(WorksheetErrorCode::Ref));
    }
    let sheet = base.sheet();
    let corner = |row: i64, column: i64| {
        CellKey::new(sheet, row as u32, column as u32).expect("checked on the grid")
    };
    let area = GridArea::new(corner(top, left), corner(bottom, right))
        .expect("both corners lie on one sheet");
    Ok(CalcValue::reference(ReferenceLike::area(area)))
}

/// Argument `index` read as a number and truncated toward zero; a value beyond any grid offset
/// is kept large enough to leave the grid.
fn whole(site: &Site<'_>, index: usize, value: &CalcValue) -> Step<i64> {
    let n = site.number_argument(index, value)?.trunc();
    Ok(n.clamp(-4_000_000.0, 4_000_000.0) as i64)
}

/// The height or width argument `index`, truncated: the reference's own `own` when it is not
/// given, #REF! when it is 0.
fn extent(site: &Site<'_>, index: usize, value: Option<&CalcValue>, own: u32) -> Step<i64> {
    match value {
        None => Ok(i64::from(own)),
        Some(value) if value.is_missing() => Ok(i64::from(own)),
        Some(value) => match whole(site, index, value)? {
            0 => Err(Exit::Error(WorksheetErrorCode::Ref)),
            n => Ok(n),
        },
    }
}

/// The first and last row (or column) of an extent of `size` from `start`: down or right when it
/// is positive, up or left when it is negative.
fn span(start: i64, size: i64) -> (i64, i64) {
    if size > 0 {
        (start, start + size - 1)
    } else {
        (start + size + 1, start)
    }
}

/// TRIMRANGE(range, [trim_rows], [trim_cols]) (dc-h9u): the reference to `range` without its
/// leading and trailing blank rows and columns. Each trim argument, truncated, is 0 (none), 1
/// (leading), 2 (trailing) or 3 (both, the default); any other is #VALUE!. A blank cell is one
/// with no value (empty text is a value). A range with no value is #REF!. The trim references
/// `A1:.B9` (trailing), `A1.:B9` (leading) and `A1.:.B9` (both) call it with both arguments the
/// same (calc-fml).
fn trimrange(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(TRIMRANGE, services);
    let step = (|| {
        site.scalar_operands(&arguments[1..])?;
        if let CoreRef::Error(code) = arguments[0].core_ref() {
            return Err(Exit::Error(code));
        }
        let mode = |index: usize| -> Step<u8> {
            match arguments.get(index) {
                None => Ok(3),
                Some(value) if value.is_missing() => Ok(3),
                // Only 0 to 3 exactly: 2.9 is #VALUE! (Excel 16.0.20527).
                Some(value) => match site.number_argument(index, value)? {
                    n if (0.0..=3.0).contains(&n) && n.fract() == 0.0 => Ok(n as u8),
                    _ => Err(Exit::Error(WorksheetErrorCode::Value)),
                },
            }
        };
        let (rows, cols) = (mode(1)?, mode(2)?);
        // The retained unswept scalar controls keep their number, text or logical value.
        if matches!(arguments[0].core_ref(), CoreRef::Number(_) | CoreRef::Text(_) | CoreRef::Logical(_)) {
            return Ok(Err(Ok(arguments[0].clone())));
        }
        if let CoreRef::Array(array) = arguments[0].core_ref() {
            return Ok(Err(trim_array(array, rows, cols)));
        }
        let Some(reference) = arguments[0].as_reference() else {
            return Err(Exit::Error(WorksheetErrorCode::Value));
        };
        let [area] = reference.areas() else {
            return Err(Exit::Error(WorksheetErrorCode::Value));
        };
        Ok(Ok((*area, rows, cols)))
    })();
    let (area, rows, cols) = match step {
        Ok(Ok(found)) => found,
        Ok(Err(trimmed)) => return Site::finish(trimmed),
        Err(exit) => return Site::finish(Err(exit)),
    };
    // The rows and columns of the cells that hold a value.
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    let read = match services.cells() {
        Some(cells) => cells.read_area(area, &mut |key, value| {
            if !matches!(value.core_ref(), CoreRef::Empty | CoreRef::Missing) {
                let (r, c) = (key.row(), key.column());
                bounds = Some(match bounds {
                    None => (r, r, c, c),
                    Some((t, b, l, rt)) => (t.min(r), b.max(r), l.min(c), rt.max(c)),
                });
            }
            ControlFlow::Continue(())
        }),
        None => Err(crate::call::Unreadable),
    };
    if read.is_err() {
        return Err(FunctionFault::ReferenceUnreadable {
            id: TRIMRANGE,
            index: 0,
        });
    }
    let Some((top, bottom, left, right)) = bounds else {
        return Site::finish(Err(Exit::Error(WorksheetErrorCode::Ref)));
    };
    let (first, last) = (area.first(), area.last());
    let keep = |mode: u8, low: u32, high: u32, min: u32, max: u32| -> (u32, u32) {
        let lead = matches!(mode, 1 | 3);
        let trail = matches!(mode, 2 | 3);
        (if lead { min } else { low }, if trail { max } else { high })
    };
    let (r0, r1) = keep(rows, first.row(), last.row(), top, bottom);
    let (c0, c1) = keep(cols, first.column(), last.column(), left, right);
    let sheet = area.sheet();
    let corner = |row: u32, column: u32| CellKey::new(sheet, row, column).expect("inside the area");
    let trimmed = GridArea::new(corner(r0, c0), corner(r1, c1)).expect("one sheet");
    Site::finish(Ok(CalcValue::reference(ReferenceLike::area(trimmed))))
}

/// TRIMRANGE of an array: its leading and trailing rows and columns with no value left out, as
/// a range's are (Excel 16.0.20527: `ROWS(TRIMRANGE({1,2},1))` is 1); an array with no value is
/// #REF!.
fn trim_array(array: &CalcArray, rows: u8, cols: u8) -> Step<CalcValue> {
    let shape = array.shape();
    let held = |r: usize, c: usize| {
        array
            .get(r, c)
            .is_some_and(|v| !matches!(v.core_ref(), CoreRef::Empty | CoreRef::Missing))
    };
    let row_held: Vec<usize> = (0..shape.rows)
        .filter(|&r| (0..shape.cols).any(|c| held(r, c)))
        .collect();
    let col_held: Vec<usize> = (0..shape.cols)
        .filter(|&c| (0..shape.rows).any(|r| held(r, c)))
        .collect();
    let (Some(&top), Some(&bottom), Some(&left), Some(&right)) = (
        row_held.first(),
        row_held.last(),
        col_held.first(),
        col_held.last(),
    ) else {
        return Err(Exit::Error(WorksheetErrorCode::Ref));
    };
    let span = |mode: u8, low: usize, high: usize, first: usize, last: usize| {
        (
            if matches!(mode, 1 | 3) { first } else { low },
            if matches!(mode, 2 | 3) { last } else { high },
        )
    };
    let (r0, r1) = span(rows, 0, shape.rows - 1, top, bottom);
    let (c0, c1) = span(cols, 0, shape.cols - 1, left, right);
    let out: Vec<Vec<CalcValue>> = (r0..=r1)
        .map(|r| {
            (c0..=c1)
                .map(|c| array.get(r, c).cloned().unwrap_or_else(CalcValue::empty))
                .collect()
        })
        .collect();
    Ok(CalcValue::array(
        CalcArray::from_rows(out).expect("a rectangular array"),
    ))
}

/// ISOMITTED(argument): TRUE when the argument is omitted, as a LAMBDA parameter its call did not
/// supply is (Excel 16.0, 2026-10-23: `=LAMBDA(x,[y],IF(ISOMITTED(y),x*2,x+y))(5)` is 10 and
/// `=ISOMITTED(1)` FALSE).
fn isomitted(
    arguments: &[CalcValue],
    _services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    Ok(CalcValue::logical(arguments[0].is_missing()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call::{FunctionCallScratch, PreparedArguments};
    use crate::catalogue::catalogue;
    use crate::value::{IdAllocator, NumericMode, SheetId};

    struct Given(Vec<CalcValue>);

    impl PreparedArguments for Given {
        fn count(&self) -> usize {
            self.0.len()
        }

        fn argument(&mut self, index: usize) -> CalcValue {
            self.0[index].clone()
        }
    }

    fn call(id: FunctionId, arguments: Vec<CalcValue>) -> Result<CalcValue, FunctionFault> {
        let mode = NumericMode::Fast;
        crate::call::invoke(
            catalogue().resolve(id).unwrap(),
            &mut Given(arguments),
            &mut EvalServices::new(&mode),
            &mut FunctionCallScratch::new(),
        )
    }

    fn area(sheet: SheetId, a: (u32, u32), b: (u32, u32)) -> CalcValue {
        CalcValue::reference(ReferenceLike::area(
            GridArea::new(
                CellKey::new(sheet, a.0, a.1).unwrap(),
                CellKey::new(sheet, b.0, b.1).unwrap(),
            )
            .unwrap(),
        ))
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn offset_shifts_and_resizes() {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let s = ids.issue().unwrap();
        let n = CalcValue::number;
        let a1 = area(s, (1, 1), (1, 1));
        assert_eq!(
            call(OFFSET, vec![a1.clone(), CalcValue::missing(), n(1.0)]),
            Ok(area(s, (1, 2), (1, 2)))
        );
        assert_eq!(
            call(OFFSET, vec![a1.clone(), CalcValue::empty(), n(0.0)]),
            Ok(a1.clone())
        );
        assert_eq!(
            call(
                OFFSET,
                vec![area(s, (5, 1), (5, 1)), n(0.0), n(0.0), n(-2.0)]
            ),
            Ok(area(s, (4, 1), (5, 1)))
        );
        assert_eq!(
            call(OFFSET, vec![a1.clone(), n(1.9), n(2.0), n(2.0), n(3.0)]),
            Ok(area(s, (2, 3), (3, 5)))
        );
        assert_eq!(
            call(OFFSET, vec![area(s, (1, 1), (2, 2)), n(1.0), n(1.0)]),
            Ok(area(s, (2, 2), (3, 3)))
        );
        for refused in [
            vec![a1.clone(), n(-1.0), n(0.0)],
            vec![a1.clone(), n(0.0), n(0.0), n(0.0)],
        ] {
            assert_eq!(
                call(OFFSET, refused),
                Ok(CalcValue::error(WorksheetErrorCode::Ref))
            );
        }
        assert_eq!(
            call(
                OFFSET,
                vec![a1, CalcValue::error(WorksheetErrorCode::NA), n(0.0)]
            ),
            Ok(CalcValue::error(WorksheetErrorCode::NA))
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn isomitted_is_true_only_for_an_omitted_argument() {
        assert_eq!(
            call(ISOMITTED, vec![CalcValue::missing()]),
            Ok(CalcValue::logical(true))
        );
        assert_eq!(
            call(ISOMITTED, vec![CalcValue::number(1.0)]),
            Ok(CalcValue::logical(false))
        );
    }
}

#[cfg(test)]
mod declaration_equivalence {
    use crate::{FunctionId, catalogue};

    // Snapshot of the preceding declarations, whose source was unchanged since f0be6d14.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn original_entries_equal_shared_declarations() {
        let mut actual = String::new();
        for member in 1..=3 {
            let t = catalogue().resolve(FunctionId::from_raw((40 << 16) | member)).unwrap();
            actual.push_str(&format!("{}\t{}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\n",
                t.id().raw(), t.spelling(), t.notation(), t.kind(), t.availability(), t.implementation(),
                t.arity(), t.result_form(), t.argument_demand(), t.traits(), t.signature_help()));
        }
        assert_eq!(actual, include_str!("../../../calc-catalogue/tests/fixtures/offset-before.tsv"));
    }
}
