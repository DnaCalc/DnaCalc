//! The error-fallback family fragment: IFERROR and IFNA and their lazy kernels.
//!
//! Each asks for `value` alone first. When `value` is an error the entry catches (any error for
//! IFERROR, #N/A for IFNA), it asks for the fallback, which is the result as it is; otherwise
//! `value` is the result and the fallback is never asked for. Both parameters take values: a
//! reference to one cell reaches the kernel as the cell's value, so neither entry returns a
//! reference.
//!
//! Microsoft's pages say that an empty cell given as either argument is treated as empty text.
//! These kernels return a blank cell's value as it is, the empty value, which concatenates as empty
//! text, reads as 0 in arithmetic and equals 0, and which a formula's result shows as 0; an omitted
//! argument that becomes the result is the empty value too. Excel 16.0.20430.20182 gives the same
//! for each of bead dc-ibp2's oracle rows on these points.
//!
//! An array `value` is refused as not modelled: Excel lifts both entries over its elements, which
//! array evaluation owns (dc-m3u, in calc-fml). An array fallback is refused the same way, naming
//! argument 1, when it is asked for. A callable remains one value through selection; its visible
//! #CALC! core becomes a worksheet error only when it reaches a cell (fn5 selection005/006).

use super::FunctionId;
use crate::call::{EvalServices, FunctionFault, LazyArguments};
use crate::kernel::Site;
use crate::value::{CalcValue, CoreRef, WorksheetErrorCode};

use calc_catalogue::error_fallback::{CODE, ENTRIES as DECLARATIONS, IFERROR, IFNA};

attach_family! {
    code: CODE; declarations: DECLARATIONS;
    IFERROR => Lazy(iferror_selected);
    IFNA => Lazy(ifna_selected);
}

/// IFERROR catches every worksheet error. Microsoft's page lists seven (#N/A, #VALUE!, #REF!,
/// #DIV/0!, #NUM!, #NAME? and #NULL!); Excel 16.0.20430.20182 catches #GETTING_DATA, #CALC!,
/// #SPILL! and #FIELD! too (bead dc-ibp2's capture). It catches #BLOCKED!, #CONNECT! and #BUSY!
/// as well, which no capture measures: the functions that give them in Excel are external-data
/// routes the capture refuses.
fn iferror_selected(
    arguments: &mut LazyArguments<'_>,
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    fallback_selected(IFERROR, arguments, services, |_| true)
}

/// IFNA catches #N/A alone; every other error is its result.
fn ifna_selected(
    arguments: &mut LazyArguments<'_>,
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    fallback_selected(IFNA, arguments, services, |code| {
        code == WorksheetErrorCode::NA
    })
}

/// The kernel both entries share; `catches` says which errors select the fallback.
fn fallback_selected(
    id: FunctionId,
    arguments: &mut LazyArguments<'_>,
    services: &mut EvalServices<'_>,
    catches: fn(WorksheetErrorCode) -> bool,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(id, services);
    let value = arguments.argument(0)?;
    if not_scalar(&value) {
        return Site::finish(Err(site.not_modelled(Some(0))));
    }
    let chosen = match value.core_ref() {
        CoreRef::Error(code)
            if value.callable_value().is_none()
                && value.image_value().is_none()
                && catches(code) =>
        {
            let fallback = arguments.argument(1)?;
            if not_scalar(&fallback) {
                return Site::finish(Err(site.not_modelled(Some(1))));
            }
            fallback
        }
        _ => value,
    };
    Ok(if chosen.is_missing() {
        CalcValue::empty()
    } else {
        chosen
    })
}

/// Arrays need the evaluator's lifting route. A callable is one value and keeps
/// its identity through fallback selection (fn5 selection005/006).
fn not_scalar(value: &CalcValue) -> bool {
    value.callable_value().is_none() && matches!(value.core_ref(), CoreRef::Array(_))
}

/// IFERROR and IFNA through `invoke` on arguments that record what the call interface asks for.
/// Each test runs natively and, as a wasm-bindgen test, in Chrome and Firefox.
#[cfg(test)]
mod tests {
    use crate::call::{
        EvalServices, FunctionCallScratch, FunctionFault, PreparedArguments, invoke,
    };
    use crate::catalogue::{FunctionCallTarget, Notation, catalogue};
    use crate::value::{
        CalcArray, CalcValue, CallableArityShape, CallableValue, CellKey, IdAllocator, NumericMode,
        OpaqueCallable, ReferenceLike, SheetId, WorksheetErrorCode,
    };
    use std::any::Any;
    use std::sync::Arc;

    struct Asked {
        values: Vec<CalcValue>,
        asked: Vec<usize>,
    }

    impl PreparedArguments for Asked {
        fn count(&self) -> usize {
            self.values.len()
        }

        fn argument(&mut self, index: usize) -> CalcValue {
            self.asked.push(index);
            self.values[index].clone()
        }
    }

    fn named(spelling: &str) -> FunctionCallTarget<'static> {
        catalogue()
            .resolve_surface(Notation::Call, spelling)
            .expect("a catalogue entry")
    }

    /// The result of `spelling(value, fallback)` and the arguments asked for, in order.
    fn call(
        spelling: &str,
        value: CalcValue,
        fallback: CalcValue,
    ) -> (Result<CalcValue, FunctionFault>, Vec<usize>) {
        let mode = NumericMode::Fast;
        let mut arguments = Asked {
            values: vec![value, fallback],
            asked: Vec::new(),
        };
        let result = invoke(
            named(spelling),
            &mut arguments,
            &mut EvalServices::new(&mode),
            &mut FunctionCallScratch::new(),
        );
        (result, arguments.asked)
    }

    fn a_reference() -> CalcValue {
        let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
        let sheet = sheets.issue().unwrap();
        CalcValue::reference(ReferenceLike::cell(CellKey::new(sheet, 1, 1).unwrap()))
    }

    #[derive(Debug)]
    struct Handle;

    impl OpaqueCallable for Handle {
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    /// A callable, whose worksheet-visible core is #CALC!.
    fn a_callable() -> CalcValue {
        CalcValue::callable(CallableValue::new(
            CallableArityShape::exact(1),
            "LAMBDA(x,x)",
            Arc::new(Handle),
        ))
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_fallback_is_asked_for_only_on_a_caught_error() {
        let five = CalcValue::number(5.0);
        for code in WorksheetErrorCode::all() {
            let error = CalcValue::error(code);
            assert_eq!(
                call("IFERROR", error.clone(), five.clone()),
                (Ok(five.clone()), vec![0, 1]),
                "IFERROR catches {code:?}"
            );
            let ifna = call("IFNA", error.clone(), five.clone());
            if code == WorksheetErrorCode::NA {
                assert_eq!(ifna, (Ok(five.clone()), vec![0, 1]));
            } else {
                assert_eq!(ifna, (Ok(error), vec![0]), "IFNA passes {code:?} on");
            }
        }
        let held_back = CalcValue::error(WorksheetErrorCode::Div0);
        for spelling in ["IFERROR", "IFNA"] {
            for value in [
                CalcValue::number(2.0),
                CalcValue::logical(false),
                CalcValue::text(crate::value::ExcelText::from_interop_assignment("")),
            ] {
                assert_eq!(
                    call(spelling, value.clone(), held_back.clone()),
                    (Ok(value), vec![0]),
                    "{spelling}"
                );
            }
        }
    }

    /// An empty or omitted argument that becomes the result is the empty value, never Missing.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn empty_and_omitted_arguments_give_the_empty_value() {
        let na = CalcValue::error(WorksheetErrorCode::NA);
        for spelling in ["IFERROR", "IFNA"] {
            for empty in [CalcValue::empty(), CalcValue::missing()] {
                assert_eq!(
                    call(spelling, empty.clone(), na.clone()),
                    (Ok(CalcValue::empty()), vec![0])
                );
                assert_eq!(
                    call(spelling, na.clone(), empty),
                    (Ok(CalcValue::empty()), vec![0, 1])
                );
            }
        }
    }

    /// Fn5 selection005/006: callable identity survives either selected argument,
    /// and its visible #CALC! core never asks for a fallback.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn callables_keep_their_identity_and_lazy_argument_access() {
        let callable = a_callable();
        for spelling in ["IFERROR", "IFNA"] {
            assert_eq!(
                call(spelling, callable.clone(), CalcValue::number(0.0)),
                (Ok(callable.clone()), vec![0])
            );
            assert_eq!(
                call(
                    spelling,
                    CalcValue::error(WorksheetErrorCode::NA),
                    callable.clone()
                ),
                (Ok(callable.clone()), vec![0, 1])
            );
            assert_eq!(
                call(spelling, CalcValue::number(3.0), callable.clone()),
                (Ok(CalcValue::number(3.0)), vec![0])
            );
        }
    }

    /// An array value is refused before the fallback is asked for, and an array
    /// fallback once it is asked for; an unused fallback is not refused. A
    /// reference is refused at admission, since both parameters take values (the caller reads a
    /// reference's cell first).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn arrays_and_references_are_refused() {
        let array = CalcValue::array(
            CalcArray::from_rows(vec![vec![
                CalcValue::number(1.0),
                CalcValue::error(WorksheetErrorCode::NA),
            ]])
            .unwrap(),
        );
        for spelling in ["IFERROR", "IFNA"] {
            let id = named(spelling).id();
            for refused in [array.clone()] {
                assert_eq!(
                    call(spelling, refused.clone(), CalcValue::number(0.0)),
                    (
                        Err(FunctionFault::NotImplemented {
                            id,
                            argument: Some(0)
                        }),
                        vec![0]
                    )
                );
                assert_eq!(
                    call(
                        spelling,
                        CalcValue::error(WorksheetErrorCode::NA),
                        refused.clone()
                    ),
                    (
                        Err(FunctionFault::NotImplemented {
                            id,
                            argument: Some(1)
                        }),
                        vec![0, 1]
                    ),
                    "{spelling}: an array fallback, asked for"
                );
                assert_eq!(
                    call(spelling, CalcValue::number(3.0), refused),
                    (Ok(CalcValue::number(3.0)), vec![0]),
                    "{spelling}: an array fallback, not asked for"
                );
            }
            assert_eq!(
                call(spelling, a_reference(), CalcValue::number(0.0)),
                (
                    Err(FunctionFault::ReferenceArgument { id, index: 0 }),
                    vec![0]
                )
            );
            assert_eq!(
                call(
                    spelling,
                    CalcValue::error(WorksheetErrorCode::NA),
                    a_reference()
                ),
                (
                    Err(FunctionFault::ReferenceArgument { id, index: 1 }),
                    vec![0, 1]
                )
            );
        }
    }
}
