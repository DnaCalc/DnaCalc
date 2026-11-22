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

#[cfg(test)]
use super::FunctionId;
use super::{CatalogueEntry, Family, Kernel};
use crate::call::{EvalServices, FunctionFault};
use crate::kernel::{Exit, Site, Step};
use crate::value::{
    CalcArray, CalcValue, CellKey, CoreRef, GridArea, MAX_COLUMNS, MAX_ROWS, ReferenceLike,
    WorksheetErrorCode,
};
use std::ops::ControlFlow;

use calc_catalogue::WorksheetDeclaration;
use calc_catalogue::offset::{CODE, ENTRIES as DECLARATIONS, ISOMITTED, OFFSET, TRIMRANGE};

const fn attach(data: WorksheetDeclaration) -> CatalogueEntry {
    let kernel = match data.id.raw() {
        x if x == OFFSET.raw() => Kernel::Eager(offset),
        x if x == ISOMITTED.raw() => Kernel::Eager(isomitted),
        x if x == TRIMRANGE.raw() => Kernel::Eager(trimrange),
        _ => panic!("a worksheet declaration has no attached kernel or refusal"),
    };
    CatalogueEntry {
        id: data.id,
        spelling: data.spelling,
        notation: data.notation,
        kind: data.kind,
        availability: data.availability,
        arity: data.arity,
        groups: data.groups,
        parameters: data.parameters,
        result: data.result,
        demand: data.demand,
        traits: data.traits,
        kernel,
    }
}

const ENTRIES: [CatalogueEntry; 3] = [
    attach(DECLARATIONS[0]),
    attach(DECLARATIONS[1]),
    attach(DECLARATIONS[2]),
];
const _: () = assert!(DECLARATIONS.len() == ENTRIES.len());
pub(super) const FAMILY: Family = Family {
    code: CODE,
    entries: &ENTRIES,
};

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
    let positions_known = !services
        .cells()
        .is_some_and(|reader| !reader.reference_positions_known());
    let columns_known = !services
        .cells()
        .is_some_and(|reader| !reader.reference_columns_known());
    let site = Site::new(OFFSET, services);
    if external {
        return Site::finish(Err(Exit::Error(WorksheetErrorCode::Value)));
    }
    Site::finish(shifted(&site, arguments, positions_known, columns_known))
}

fn shifted(site: &Site<'_>, arguments: &[CalcValue], positions_known: bool, columns_known: bool) -> Step<CalcValue> {
    site.scalar_operands(arguments)?;
    let Some(reference) = arguments[0].as_reference() else {
        return Err(Exit::Error(WorksheetErrorCode::Value));
    };
    let [base] = reference.areas() else {
        return Err(Exit::Error(WorksheetErrorCode::Value));
    };
    let rows = whole(site, 1, &arguments[1])?;
    let cols = whole(site, 2, &arguments[2])?;
    // A zero extent alone cannot determine the displayed result when the caller has not
    // supplied the reference position and spill placement (for example, Excel can show #SPILL!).
    let without_position = |exit| match exit {
        Exit::Error(WorksheetErrorCode::Ref) if !positions_known => site.not_modelled(Some(0)),
        other => other,
    };
    let (height, height_negative) = extent(site, 3, arguments.get(3), base.row_count())?;
    let (width, width_negative) = extent(site, 4, arguments.get(4), base.column_count())?;
    // Excel reads both dimensions before deciding whether a zero extent is valid. Only an
    // extent that is exactly zero is #REF!; a fraction truncates to 0 afterwards (below).
    if height.is_none() || width.is_none() {
        return Err(without_position(Exit::Error(WorksheetErrorCode::Ref)));
    }
    let (height, width) = (height.unwrap_or(0), width.unwrap_or(0));
    if !positions_known {
        // A zero shift of a one-cell reference still names that same cell, even
        // when the caller did not supply its worksheet position.
        if rows == 0 && cols == 0 && height.abs() == 1 && width.abs() == 1
            && base.row_count() == 1 && base.column_count() == 1
        {
            return Ok(arguments[0].clone());
        }
        // A known column and zero row shift identify the destination within the
        // same relocated row. Its absolute row remains unknown to the caller.
        if columns_known && rows == 0 && (1..=2).contains(&cols) && height.abs() == 1 && width.abs() == 1
            && base.row_count() == 1 && base.column_count() == 1
        {
            let column = i64::from(base.first().column()) + cols;
            if (1..=i64::from(MAX_COLUMNS)).contains(&column) {
                let cell = CellKey::new(base.sheet(), base.first().row(), column as u32)
                    .expect("checked column and existing row");
                return Ok(CalcValue::reference(ReferenceLike::area(GridArea::cell(cell))));
            }
        }
        // A single-cell result cannot spill, and a shift at least as large as the
        // grid leaves it regardless of the original cell's unknown position.
        if height.abs() == 1
            && width.abs() == 1
            && (rows.abs() >= i64::from(MAX_ROWS) || cols.abs() >= i64::from(MAX_COLUMNS))
        {
            return Err(Exit::Error(WorksheetErrorCode::Ref));
        }
        return Err(site.not_modelled(Some(0)));
    }
    let top = i64::from(base.first().row()) + rows;
    let left = i64::from(base.first().column()) + cols;
    let (top, bottom) = span(top, height, height_negative);
    let (left, right) = span(left, width, width_negative);
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

/// The height or width argument `index`, truncated, with the sign of the value given: the
/// reference's own `own` when it is not given, and `None` for an extent of exactly zero. Zero
/// is checked after both dimensions have been read.
fn extent(site: &Site<'_>, index: usize, value: Option<&CalcValue>, own: u32) -> Step<(Option<i64>, bool)> {
    match value {
        None => Ok((Some(i64::from(own)), false)),
        Some(value) if value.is_missing() => Ok((Some(i64::from(own)), false)),
        Some(value) => {
            let n = site.number_argument(index, value)?;
            if n == 0.0 {
                return Ok((None, false));
            }
            Ok((Some(n.trunc().clamp(-4_000_000.0, 4_000_000.0) as i64), n < 0.0))
        }
    }
}

/// The first and last row (or column) of an extent of `size` from `start`: down or right when the
/// extent given was positive, up or left when it was negative. Excel computes the far edge as
/// `start + size - 1` (or `start + size + 1`) from the truncated size, so a fraction between -1
/// and 1 (size 0) names two cells: `start` and the one before it (a positive fraction) or after
/// it (a negative one). Measured, Excel 16.0.20527 (env 76a02a5594d8, 2026-11-16, serial
/// one-call layout, B1 holding "x"): `OFFSET(B1,1,0,1E-300,1)` is B1:B2 ("x"),
/// `OFFSET(B1,1,0,1,0.92)` and `OFFSET(B1,1,0,1,-0.5)` are two columns wide (#SPILL! in A1),
/// `OFFSET(B1,1,0,1.9,1)` is B2 and `OFFSET(B1,1,0,1,0)` is #REF!.
fn span(start: i64, size: i64, negative: bool) -> (i64, i64) {
    let far = if negative { start + size + 1 } else { start + size - 1 };
    (start.min(far), start.max(far))
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
        if matches!(
            arguments[0].core_ref(),
            CoreRef::Number(_) | CoreRef::Text(_) | CoreRef::Logical(_)
        ) {
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

    /// Excel 20527's independent catalogue controls select the original numeric
    /// and text cells with a zero shift. A relocated one-cell reference is still
    /// usable without knowing its original worksheet address.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn offset_zero_shift_preserves_single_reference_without_position() {
        use crate::call::{CellReader, SubtotalExclusions, Unreadable};

        struct Relocated;
        impl CellReader for Relocated {
            fn reference_positions_known(&self) -> bool { false }
            fn read_area(
                &mut self,
                _: GridArea,
                _: &mut dyn FnMut(CellKey, &CalcValue) -> ControlFlow<()>,
            ) -> Result<(), Unreadable> { Err(Unreadable) }
            fn subtotal_exclusions(&mut self, _: GridArea) -> Option<SubtotalExclusions> { None }
        }

        let sheet = IdAllocator::<SheetId>::new().issue().unwrap();
        let cell = area(sheet, (4, 7), (4, 7));
        let wide = area(sheet, (4, 7), (4, 8));
        let mode = NumericMode::Fast;
        let n = CalcValue::number;
        let run = |args| {
            crate::call::invoke(
                catalogue().resolve(OFFSET).unwrap(),
                &mut Given(args),
                &mut EvalServices::with_cells(&mode, &mut Relocated),
                &mut FunctionCallScratch::new(),
            )
        };
        assert_eq!(run(vec![cell.clone(), n(0.0), n(0.0)]), Ok(cell.clone()));
        assert_eq!(run(vec![cell.clone(), n(0.5), n(-0.5), n(-1.0), n(1.0)]), Ok(cell.clone()));
        let refusal = Err(FunctionFault::NotImplemented { id: OFFSET, argument: Some(0) });
        assert_eq!(run(vec![cell.clone(), n(1.0), n(0.0)]), refusal);
        assert_eq!(
            run(vec![cell.clone(), n(0.0), n(0.0), n(0.5), CalcValue::error(WorksheetErrorCode::Div0)]),
            Ok(CalcValue::error(WorksheetErrorCode::Div0))
        );
        assert_eq!(run(vec![cell, n(0.0), n(0.0), n(0.0), n(1.0)]), refusal);
        assert_eq!(run(vec![wide, n(0.0), n(0.0)]), refusal);
    }

    /// Excel 20527 controls confirm same-row shifts between attested argument columns.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn offset_uses_known_column_without_claiming_the_row() {
        use crate::call::{CellReader, SubtotalExclusions, Unreadable};
        struct Columns;
        impl CellReader for Columns {
            fn reference_positions_known(&self) -> bool { false }
            fn reference_columns_known(&self) -> bool { true }
            fn read_area(&mut self, _: GridArea, _: &mut dyn FnMut(CellKey, &CalcValue) -> ControlFlow<()>)
                -> Result<(), Unreadable> { Err(Unreadable) }
            fn subtotal_exclusions(&mut self, _: GridArea) -> Option<SubtotalExclusions> { None }
        }
        let sheet = IdAllocator::<SheetId>::new().issue().unwrap();
        let cell = area(sheet, (4, 7), (4, 7));
        let mode = NumericMode::Fast;
        let n = CalcValue::number;
        let run = |args| crate::call::invoke(
            catalogue().resolve(OFFSET).unwrap(), &mut Given(args),
            &mut EvalServices::with_cells(&mode, &mut Columns), &mut FunctionCallScratch::new(),
        );
        assert_eq!(run(vec![cell.clone(), n(0.5), n(2.5)]), Ok(area(sheet, (4, 9), (4, 9))));
        let refusal = Err(FunctionFault::NotImplemented { id: OFFSET, argument: Some(0) });
        assert_eq!(run(vec![cell.clone(), n(1.0), n(2.0)]), refusal);
        assert_eq!(run(vec![cell, n(0.0), n(2.0), n(2.0)]), refusal);
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
            let t = catalogue()
                .resolve(FunctionId::from_raw((40 << 16) | member))
                .unwrap();
            actual.push_str(&format!(
                "{}\t{}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\n",
                t.id().raw(),
                t.spelling(),
                t.notation(),
                t.kind(),
                t.availability(),
                t.implementation(),
                t.arity(),
                t.result_form(),
                t.argument_demand(),
                t.traits(),
                t.signature_help()
            ));
        }
        let expected = crate::catalogue::original_declaration_snapshot(include_str!(
            "../../../calc-catalogue/tests/fixtures/offset-before.tsv"
        ));
        assert_eq!(actual, expected.as_ref());
    }
}
