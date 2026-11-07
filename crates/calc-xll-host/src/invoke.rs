use crate::{
    callbacks::{self, Frame},
    native::{GetCurrentThreadId, XllFunction, current_sheet},
};
use calc_func::UserFunctionContext;
use calc_value::{CalcValue, CoreRef, ExcelText, WorksheetErrorCode};
use calc_xll_abi::*;

use libffi::middle::{Arg, Cif, CodePtr, Ret, Type};

#[derive(Default)]
struct Returned {
    word: usize,
    number: f64,
}
pub(crate) enum NativeReturn {
    Value(CalcValue, u32),
    Pending(callbacks::Pending),
}

pub(crate) fn call_interface(signature: &XllSignature) -> Option<Cif> {
    fn native_type(kind: XllType) -> Option<Type> {
        use XllType::*;
        Some(match kind {
            Double => Type::f64(),
            Boolean | Short => Type::i16(),
            UShort => Type::u16(),
            Int => Type::i32(),
            Void | InPlace(_) => Type::void(),
            Oper12
            | Oper12Reference
            | DoublePointer
            | BooleanPointer
            | ShortPointer
            | IntPointer
            | WideString
            | WideCountedString
            | WideStringInPlace
            | WideCountedStringInPlace
            | Fp12
            | AsyncHandle => Type::pointer(),
            _ => return None,
        })
    }
    let arguments = signature
        .arguments
        .iter()
        .copied()
        .map(native_type)
        .collect::<Option<Vec<_>>>()?;
    let result = native_type(signature.result)?;
    Some(Cif::new(arguments, result))
}

impl XllFunction {
    pub(crate) fn invoke(
        &self,
        arguments: &[CalcValue],
        context: Option<&mut dyn UserFunctionContext>,
    ) -> Result<CalcValue, String> {
        let formula_entry = context.as_ref().is_some_and(|context| {
            context
                .invocation()
                .is_some_and(|frame| frame.formula_entry)
        });
        if formula_entry && self.registration.info.signature.asynchronous {
            return Err(
                "dc-snz.23: native async formula-entry needs an admitted continuation".into(),
            );
        }
        if self.registration.info.signature.thread_safe
            && self.registration.info.signature.asynchronous
        {
            return Err("dc-snz.23: thread-safe native async entry is not modelled".into());
        }
        if self.registration.info.signature.thread_safe && !formula_entry {
            let pool = self.state.borrow().workers.clone();
            if let Some(pool) = pool {
                return crate::worker::invoke(self, &pool, arguments, context);
            }
        }
        self.invoke_inline(arguments, context)
    }
    pub(crate) fn invoke_inline(
        &self,
        arguments: &[CalcValue],
        context: Option<&mut dyn UserFunctionContext>,
    ) -> Result<CalcValue, String> {
        self.invoke_inline_as(arguments, context, false)
    }
    pub(crate) fn invoke_run(&self, arguments: &[CalcValue]) -> Result<(CalcValue, bool), String> {
        match self.start_inline(arguments, None, true)? {
            NativeReturn::Value(value, kind) => Ok((value, kind == XL_INT)),
            NativeReturn::Pending(_) => {
                Err("dc-snz.23: async Application.Run has no admitted continuation".into())
            }
        }
    }
    fn invoke_inline_as(
        &self,
        arguments: &[CalcValue],
        context: Option<&mut dyn UserFunctionContext>,
        as_run: bool,
    ) -> Result<CalcValue, String> {
        match self.start_inline(arguments, context, as_run)? {
            NativeReturn::Value(value, _) => Ok(value),
            NativeReturn::Pending(pending) => {
                callbacks::wait(pending.key, &pending.completion).map(normalize_return)
            }
        }
    }
    pub(crate) fn start_inline(
        &self,
        arguments: &[CalcValue],
        mut context: Option<&mut dyn UserFunctionContext>,
        as_run: bool,
    ) -> Result<NativeReturn, String> {
        let r = &self.registration;
        let signature = &r.info.signature;
        if r.uses.get() == 0 || self.state.borrow().closed {
            return Ok(NativeReturn::Value(
                CalcValue::error(WorksheetErrorCode::Name),
                XL_ERR,
            ));
        }
        if self.state.borrow().thread != unsafe { GetCurrentThreadId() } {
            return Err("dc-snz.2: XLL calls require the owning main lane in X1".into());
        }
        if signature.volatile {
            if let Some(context) = &mut context {
                context.set_volatile(true);
            }
        }
        let cif = r
            .cif
            .as_ref()
            .ok_or("dc-snz.2: registration uses an invocation ABI not yet modelled")?;
        if matches!(
            signature.result,
            XllType::InPlace(_) | XllType::WideStringInPlace | XllType::WideCountedStringInPlace
        ) || (signature.result == XllType::Void
            && !signature.asynchronous
            && r.info.macro_type != 2)
        {
            return Err("dc-snz.2: in-place return invocation is not modelled".into());
        }
        let expected = signature.arguments.len() - usize::from(signature.asynchronous);
        if arguments.len() > expected {
            return Err(format!("XLL expects at most {expected} arguments"));
        }
        let mut values = arguments.to_vec();
        values.resize(expected, CalcValue::missing());
        let mut arena = Arena::new();
        let mut slots = Vec::with_capacity(signature.arguments.len().max(4));
        let mut pending = signature
            .asynchronous
            .then(|| callbacks::owned_pending(&self.state, r))
            .transpose()?;
        let mut argument = 0;
        let sheet = current_sheet(&context);
        for kind in &signature.arguments {
            if *kind == XllType::AsyncHandle {
                let key = pending.as_ref().unwrap().key;
                let handle = XlOper12 {
                    value: XlValue12 {
                        big_data: XlBigData {
                            data: key as *mut u8,
                            byte_count: 0,
                        },
                    },
                    xltype: XL_BIG_DATA,
                };
                slots.push(arena.store(handle) as u64);
                continue;
            }
            let mut value = values[argument].clone();
            argument += 1;
            if !kind.allows_reference() {
                if let Some(reference) = value.as_reference() {
                    value = context
                        .as_mut()
                        .ok_or("dc-snz.2: XLL reference needs a reader")?
                        .argument_values_of(reference)?;
                }
            }
            let scalar_kind = match kind {
                XllType::Double
                | XllType::DoublePointer
                | XllType::UShort
                | XllType::Short
                | XllType::ShortPointer
                | XllType::Int
                | XllType::IntPointer => Some(XL_NUM),
                XllType::Boolean | XllType::BooleanPointer => Some(XL_BOOL),
                XllType::WideString
                | XllType::WideCountedString
                | XllType::WideStringInPlace
                | XllType::WideCountedStringInPlace => Some(XL_STR),
                _ => None,
            };
            if let Some(kind) = scalar_kind {
                if value.as_error().is_some() {
                    return Ok(NativeReturn::Value(
                        CalcValue::error(WorksheetErrorCode::Value),
                        XL_ERR,
                    ));
                }
                value = match callbacks::coerce(value, kind) {
                    Ok(value) => value,
                    Err((_, why)) if why.contains("dc-snz.") => return Err(why),
                    Err(_) => {
                        return Ok(NativeReturn::Value(
                            CalcValue::error(WorksheetErrorCode::Value),
                            XL_ERR,
                        ));
                    }
                };
            }
            // Match cached calls by the values actually passed to native code,
            // after reference reads, missing-argument padding and ABI coercion.
            values[argument - 1] = value.clone();
            let slot = marshal(
                *kind,
                &value,
                &mut arena,
                &mut self.state.borrow_mut().sheets,
                sheet,
            );
            match slot {
                Ok(slot) => slots.push(slot),
                Err(why) => {
                    return Err(why);
                }
            }
        }
        let sheet = current_sheet(&context);
        let provider = if self.bound {
            context.as_ref().and_then(|c| c.callable_identity())
        } else {
            None
        };
        let mut frame = Frame {
            state: self.state.clone(),
            module: r.module.clone(),
            origin: r.origin.clone(),
            context,
            signature: (!as_run && r.info.macro_type != 2).then(|| signature.clone()),
            provider,
            rtd_subscriptions: Default::default(),
            unadmitted_rtd_initial: false,
        };
        let guard = unsafe { callbacks::enter(&mut frame) };
        let mut returned = Returned::default();
        let ffi_arguments = slots.iter().map(Arg::new).collect::<Vec<_>>();
        let output = match signature.result {
            XllType::Double => Ret::new(&mut returned.number),
            XllType::Void | XllType::InPlace(_) => Ret::void(),
            _ => Ret::new(&mut returned.word),
        };
        unsafe {
            if let Some(pending) = &mut pending {
                pending.entered = true;
            }
            cif.call_return_into(CodePtr(r.info.address as *mut _), &ffi_arguments, output);
        }
        let mut result = if let Some(pending) = pending {
            Ok(NativeReturn::Pending(pending))
        } else {
            let kind = match signature.result {
                XllType::Oper12 | XllType::Oper12Reference if returned.word != 0 => unsafe {
                    (returned.word as *const XlOper12).read_unaligned().xltype & TYPE_MASK
                },
                XllType::Short | XllType::UShort | XllType::Int => XL_INT,
                _ => XL_NUM,
            };
            unsafe { self.convert_return(signature.result, &slots, returned, sheet) }
                .map(normalize_return)
                .map(|value| NativeReturn::Value(value, kind))
        };
        drop(guard);
        if frame.unadmitted_rtd_initial && matches!(&result, Ok(NativeReturn::Value(..))) {
            // Native code saw the real ConnectData return, but its input is not
            // in this immutable read view. Keep the worksheet unavailable until
            // normal owner admission; never certify the native result at cut0.
            result = Ok(NativeReturn::Value(
                CalcValue::error(WorksheetErrorCode::NA),
                XL_ERR,
            ));
        }
        if matches!(&result, Ok(NativeReturn::Value(..)))
            && let Some(provider) = frame.provider
            && let Some(caller) = frame.context.as_ref().and_then(|context| context.caller())
        {
            let service = frame.state.borrow().rtd.clone();
            if let Some(service) = service {
                let mut inputs = frame.rtd_subscriptions.clone();
                if inputs.is_empty() {
                    let context = frame.context.as_mut().unwrap();
                    for subscription in service.cached_call_inputs(provider, caller, &values)? {
                        // The old formula's input may have retired. A recovered
                        // native signal is queued for normal P07 admission; its
                        // cached UDF output cannot publish ahead of that input.
                        let admitted = context.rtd_value(subscription)?;
                        let newer_initial = service
                            .initial_value(provider, subscription)
                            .is_some_and(|(sequence, _)| {
                                admitted.as_ref().is_none_or(|(held, _)| sequence > *held)
                            });
                        if admitted.is_none() || newer_initial {
                            result = Ok(NativeReturn::Value(
                                CalcValue::error(WorksheetErrorCode::NA),
                                XL_ERR,
                            ));
                        }
                        inputs.insert(subscription);
                    }
                }
                service.finish_call(
                    provider,
                    caller,
                    frame
                        .context
                        .as_ref()
                        .and_then(|context| context.invocation())
                        .ok_or("dc-snz.23: RTD declaration needs an invocation identity")?
                        .id,
                    &values,
                    &frame.rtd_subscriptions.iter().copied().collect::<Vec<_>>(),
                    &inputs.iter().copied().collect::<Vec<_>>(),
                )?;
            }
        }
        result
    }
    unsafe fn convert_return(
        &self,
        kind: XllType,
        slots: &[u64],
        result: Returned,
        sheet: Option<calc_value::SheetId>,
    ) -> Result<CalcValue, String> {
        let r = &self.registration;
        let ptr = result.word as *mut XlOper12;
        let value = match kind {
            XllType::Oper12 | XllType::Oper12Reference => {
                // Copy before the next entry into this module, including xlAutoFree12.
                let flags = if ptr.is_null() {
                    0
                } else {
                    unsafe { ptr.read_unaligned().xltype }
                };
                let copy = unsafe { from_oper(ptr, &self.state.borrow().sheets, sheet) };
                if flags & DLL_FREE != 0 {
                    match r.module.address("xlAutoFree12") {
                        Ok(address) => {
                            let free: unsafe extern "system" fn(*mut XlOper12) =
                                unsafe { std::mem::transmute(address) };
                            unsafe {
                                free(ptr);
                            }
                        }
                        Err(why) => {
                            return Err(format!(
                                "dc-snz.2: DLLFree result has no xlAutoFree12: {why}"
                            ));
                        }
                    }
                } else if flags & XL_FREE != 0 {
                    callbacks::free(&self.state, &unsafe { ptr.read_unaligned() });
                }
                copy?
            }
            XllType::Double => CalcValue::number(result.number),
            XllType::Boolean => CalcValue::logical(result.word as i16 != 0),
            XllType::UShort => CalcValue::number(f64::from(result.word as u16)),
            XllType::Short => CalcValue::number(f64::from(result.word as i16)),
            XllType::Int => CalcValue::number(f64::from(result.word as i32)),
            XllType::Void => {
                if r.info.macro_type == 2 {
                    CalcValue::number(0.)
                } else if r
                    .info
                    .signature
                    .arguments
                    .first()
                    .is_some_and(|t| t.is_pointer())
                {
                    unsafe {
                        self.pointer_return(
                            r.info.signature.arguments[0],
                            slots[0] as usize,
                            sheet,
                        )?
                    }
                } else {
                    return Err(
                        "dc-snz.2: synchronous > in-place return requires a pointer first argument"
                            .into(),
                    );
                }
            }
            XllType::InPlace(n) => unsafe {
                self.pointer_return(
                    r.info.signature.arguments[usize::from(n) - 1],
                    slots[usize::from(n) - 1] as usize,
                    sheet,
                )?
            },
            kind => unsafe { self.pointer_return(kind, result.word, sheet)? },
        };
        Ok(value)
    }
    unsafe fn pointer_return(
        &self,
        kind: XllType,
        pointer: usize,
        sheet: Option<calc_value::SheetId>,
    ) -> Result<CalcValue, String> {
        if pointer == 0 {
            return Ok(CalcValue::error(WorksheetErrorCode::Num));
        }
        Ok(match kind {
            XllType::Oper12 | XllType::Oper12Reference => unsafe {
                from_oper(
                    pointer as *const XlOper12,
                    &self.state.borrow().sheets,
                    sheet,
                )
            }?,
            XllType::DoublePointer => {
                CalcValue::number(unsafe { (pointer as *const f64).read_unaligned() })
            }
            XllType::BooleanPointer => {
                CalcValue::logical(unsafe { (pointer as *const i16).read_unaligned() } != 0)
            }
            XllType::ShortPointer => CalcValue::number(f64::from(unsafe {
                (pointer as *const i16).read_unaligned()
            })),
            XllType::IntPointer => CalcValue::number(f64::from(unsafe {
                (pointer as *const i32).read_unaligned()
            })),
            XllType::WideCountedString | XllType::WideCountedStringInPlace => {
                let oper = XlOper12 {
                    value: XlValue12 {
                        string: pointer as *mut u16,
                    },
                    xltype: XL_STR,
                };
                unsafe { from_oper(&oper, &self.state.borrow().sheets, sheet) }?
            }
            XllType::WideString | XllType::WideStringInPlace => {
                let ptr = pointer as *const u16;
                let mut units = Vec::new();
                while units.len() < 32768 {
                    let c = unsafe { ptr.add(units.len()).read_unaligned() };
                    if c == 0 {
                        break;
                    }
                    units.push(c);
                }
                if units.len() > 32767 {
                    return Err("unterminated XLL UTF-16 string".into());
                }
                CalcValue::text(ExcelText::from_utf16_code_units(units))
            }
            XllType::Fp12 => {
                let fp = pointer as *const Fp12;
                let (rows, columns) = unsafe {
                    (
                        std::ptr::addr_of!((*fp).rows).read_unaligned(),
                        std::ptr::addr_of!((*fp).columns).read_unaligned(),
                    )
                };
                let rows = usize::try_from(rows).map_err(|_| "negative FP12 rows")?;
                let cols = usize::try_from(columns).map_err(|_| "negative FP12 columns")?;
                let count = rows
                    .checked_mul(cols)
                    .filter(|n| *n > 0 && *n <= 1_048_576)
                    .ok_or("oversized FP12 result")?;
                let data: Vec<_> = (0..count)
                    .map(|i| unsafe {
                        (pointer as *const u8)
                            .add(8)
                            .cast::<f64>()
                            .add(i)
                            .read_unaligned()
                    })
                    .collect();
                if data.iter().any(|n| !n.is_finite()) {
                    return Err("nonfinite FP12 result".into());
                }
                CalcValue::array(
                    calc_value::CalcArray::new(
                        calc_value::ArrayShape { rows, cols },
                        data.iter().map(|n| CalcValue::number(*n)).collect(),
                    )
                    .ok_or("invalid FP12 shape")?,
                )
            }
            _ => return Err(format!("dc-snz.2: return ABI {kind:?} not modelled")),
        })
    }
}

fn numeric(value: &CalcValue) -> Result<f64, String> {
    match value.core_ref() {
        CoreRef::Number(n) => Ok(n),
        CoreRef::Logical(b) => Ok(f64::from(u8::from(b))),
        CoreRef::Empty | CoreRef::Missing => Ok(0.),
        _ => Err("XLL requires a numeric argument".into()),
    }
}
fn marshal(
    kind: XllType,
    value: &CalcValue,
    arena: &mut Arena,
    sheets: &mut SheetMap,
    current_sheet: Option<calc_value::SheetId>,
) -> Result<u64, String> {
    use XllType::*;
    Ok(match kind {
        Oper12 | Oper12Reference => {
            if kind == Oper12Reference {
                if let Some(reference) = value.as_reference() {
                    if let [area] = reference.areas() {
                        if Some(area.sheet()) == current_sheet {
                            return Ok(arena.store(XlOper12 {
                                value: XlValue12 {
                                    single_reference: XlSRef12 {
                                        count: 1,
                                        rectangle: XlRef12 {
                                            row_first: (area.first().row() - 1) as i32,
                                            row_last: (area.last().row() - 1) as i32,
                                            col_first: (area.first().column() - 1) as i32,
                                            col_last: (area.last().column() - 1) as i32,
                                        },
                                    },
                                },
                                xltype: XL_SREF,
                            }) as u64);
                        }
                    }
                }
            }
            let wire = arena.oper(value, sheets)?;
            arena.store(wire) as u64
        }
        Double => numeric(value)?.to_bits(),
        DoublePointer => {
            let oper = arena.store(XlOper12::number(numeric(value)?));
            oper as u64
        }
        Boolean => u64::from(numeric(value)? != 0.),
        UShort => {
            let n = numeric(value)?;
            if !(0. ..=65535.).contains(&n) {
                return Err("XLL unsigned short out of range".into());
            }
            n.trunc() as u64
        }
        Short | ShortPointer | BooleanPointer => {
            let n = if kind == BooleanPointer {
                f64::from(u8::from(numeric(value)? != 0.))
            } else {
                numeric(value)?.trunc()
            };
            if !(f64::from(i16::MIN)..=f64::from(i16::MAX)).contains(&n) {
                return Err("XLL short out of range".into());
            }
            if kind == Short {
                n as i16 as u64
            } else {
                arena.short(n as i16) as u64
            }
        }
        Int | IntPointer => {
            let n = numeric(value)?.trunc();
            if !(f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&n) {
                return Err("XLL int out of range".into());
            }
            if kind == Int {
                n as i32 as u64
            } else {
                arena.int(n as i32) as u64
            }
        }
        WideString | WideCountedString | WideStringInPlace | WideCountedStringInPlace => {
            let CoreRef::Text(text) = value.core_ref() else {
                return Err("XLL requires a text argument".into());
            };
            arena.wide(
                text.utf16_code_units(),
                matches!(kind, WideCountedString | WideCountedStringInPlace),
                matches!(kind, WideStringInPlace | WideCountedStringInPlace),
            )? as u64
        }
        Fp12 => arena.fp12(value)? as u64,
        _ => return Err(format!("dc-snz.2: argument ABI {kind:?} not modelled")),
    })
}

pub(crate) fn worksheet_name(number: i32) -> Option<&'static str> {
    // New C API numbers are not serialized Ftab IDs. The original xlOil 0.22.1
    // dynamic-array probe uses 614 (observed in the paired Excel trace).
    if number == 614 {
        return Some("FILTER");
    }
    let number = u16::try_from(number).ok()?;
    calc_fml::EXCEL_FUNCTION_IDS
        .iter()
        .find(|(id, _, _)| *id == number)
        .map(|(_, name, _)| *name)
}

pub(crate) fn normalize_return(value: CalcValue) -> CalcValue {
    if value.as_number().is_some_and(|n| !n.is_finite()) {
        CalcValue::error(WorksheetErrorCode::Num)
    } else if value.is_missing() || value.is_empty() {
        CalcValue::number(0.)
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Native-only: both synchronous and asynchronous native values use this publication step.
    #[test]
    fn native_publication_never_exposes_nonfinite_or_missing_values() {
        for value in [CalcValue::missing(), CalcValue::empty()] {
            assert_eq!(normalize_return(value), CalcValue::number(0.));
        }
        for n in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                normalize_return(CalcValue::number(n)),
                CalcValue::error(WorksheetErrorCode::Num)
            );
        }
        assert_eq!(
            normalize_return(CalcValue::number(42.)),
            CalcValue::number(42.)
        );
    }
}
