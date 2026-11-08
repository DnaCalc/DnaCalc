//! `Excel12(xlfCall, ...)` and `Excel12(xlfRegisterId, ...)` (dc-gh22).
//!
//! CALL and REGISTER.ID are macro-sheet functions that Excel refuses in worksheet cells (the entry
//! rule lives in CalcFml). A native add-in reaches them through the C API, so this module answers
//! only that route. It shares the registration table `xlfRegister` fills: REGISTER.ID returns the
//! first live registration of a module's export, or registers it; CALL by module and procedure
//! registers an unregistered export the same way and CALL by ID runs a registration. Every rule
//! here follows the answers Excel 16.0 build 20527 gave to the same fixture
//! (`crates/calc-xll-reference/fixture/x1-cases.json`): the argument conversions, the ranges, the
//! error values and which failures are a nonzero return code. Types the fixture did not measure
//! (`P`, `R`, `O`, asynchronous) refuse with a typed reason instead of guessing.
use crate::{
    callbacks::{self, Frame},
    native::{Module, Registration, RegistrationInfo, current_sheet},
};
use calc_value::{CalcValue, CoreRef, ExcelText, WorksheetErrorCode};
use calc_xll_abi::*;
use libffi::middle::{Arg, Cif, CodePtr, Ret, Type};
use std::{cell::Cell, rc::Rc};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn WideCharToMultiByte(
        code_page: u32,
        flags: u32,
        wide: *const u16,
        wide_length: i32,
        narrow: *mut u8,
        narrow_length: i32,
        default_character: *const u8,
        used_default: *mut i32,
    ) -> i32;
    fn MultiByteToWideChar(
        code_page: u32,
        flags: u32,
        narrow: *const u8,
        narrow_length: i32,
        wide: *mut u16,
        wide_length: i32,
    ) -> i32;
}

/// What a call answers instead of a value: an Excel error value (return code zero), or a refusal.
enum Fail {
    Error(WorksheetErrorCode),
    Refuse(String),
}
type Outcome<T> = Result<T, Fail>;
fn value_error<T>() -> Outcome<T> {
    Err(Fail::Error(WorksheetErrorCode::Value))
}
fn refuse<T>(why: impl Into<String>) -> Outcome<T> {
    Err(Fail::Refuse(format!("dc-gh22: {}", why.into())))
}

/// The ANSI code page conversion Excel applies: unmappable characters become `?`.
fn to_ansi(units: &[u16]) -> Vec<u8> {
    if units.is_empty() {
        return Vec::new();
    }
    const WC_NO_BEST_FIT_CHARS: u32 = 0x400;
    let default = b'?';
    let mut used = 0i32;
    let needed = unsafe {
        WideCharToMultiByte(
            0,
            WC_NO_BEST_FIT_CHARS,
            units.as_ptr(),
            units.len() as i32,
            std::ptr::null_mut(),
            0,
            &default,
            &mut used,
        )
    };
    let mut out = vec![0u8; needed.max(0) as usize];
    unsafe {
        WideCharToMultiByte(
            0,
            WC_NO_BEST_FIT_CHARS,
            units.as_ptr(),
            units.len() as i32,
            out.as_mut_ptr(),
            needed,
            &default,
            &mut used,
        );
    }
    out
}
fn from_ansi(bytes: &[u8]) -> Vec<u16> {
    if bytes.is_empty() {
        return Vec::new();
    }
    let needed = unsafe {
        MultiByteToWideChar(
            0,
            0,
            bytes.as_ptr(),
            bytes.len() as i32,
            std::ptr::null_mut(),
            0,
        )
    };
    let mut out = vec![0u16; needed.max(0) as usize];
    unsafe {
        MultiByteToWideChar(
            0,
            0,
            bytes.as_ptr(),
            bytes.len() as i32,
            out.as_mut_ptr(),
            needed,
        );
    }
    out
}

/// A single value: an array contributes its first element, as it does in Excel.
fn scalar(value: &CalcValue) -> CalcValue {
    match value.as_array() {
        Some(array) => array.get(0, 0).cloned().unwrap_or_else(CalcValue::missing),
        None => value.clone(),
    }
}
fn to_number(value: &CalcValue) -> Outcome<f64> {
    let value = scalar(value);
    if value.as_error().is_some() {
        return value_error();
    }
    match callbacks::coerce(value, XL_NUM) {
        Ok(number) => number.as_number().map_or_else(value_error, Ok),
        Err((_, why)) if why.contains("dc-snz.") => refuse(why),
        Err(_) => value_error(),
    }
}
fn to_logical(value: &CalcValue) -> Outcome<bool> {
    let value = scalar(value);
    if value.as_error().is_some() {
        return value_error();
    }
    match callbacks::coerce(value, XL_BOOL) {
        Ok(logical) => logical.as_logical().map_or_else(value_error, Ok),
        Err((_, why)) if why.contains("dc-snz.") => refuse(why),
        Err(_) => value_error(),
    }
}
fn to_units(value: &CalcValue) -> Outcome<Vec<u16>> {
    let value = scalar(value);
    if value.as_error().is_some() {
        return value_error();
    }
    if value.is_missing() || value.is_empty() {
        return Ok(Vec::new());
    }
    match callbacks::coerce(value, XL_STR) {
        Ok(text) => match text.core_ref() {
            CoreRef::Text(text) => Ok(text.utf16_code_units().to_vec()),
            _ => value_error(),
        },
        Err((_, why)) if why.contains("dc-snz.") => refuse(why),
        Err(_) => value_error(),
    }
}
/// Round to the nearest integer, halves upwards (measured: 0.5 gives 1, -0.5 gives 0, 2.5 gives 3).
fn rounded(number: f64) -> f64 {
    (number + 0.5).floor()
}
fn signed(number: f64, low: f64, high: f64) -> Outcome<f64> {
    let number = rounded(number);
    if (low..=high).contains(&number) {
        Ok(number)
    } else {
        Err(Fail::Error(WorksheetErrorCode::Num))
    }
}
/// Unsigned types take the absolute value first (measured: -5 gives 5).
fn unsigned(number: f64, high: f64) -> Outcome<f64> {
    let number = rounded(number.abs());
    if (0. ..=high).contains(&number) {
        Ok(number)
    } else {
        Err(Fail::Error(WorksheetErrorCode::Num))
    }
}

enum Prepared {
    Double(f64),
    Short(i16),
    UShort(u16),
    Int(i32),
    Pointer(usize),
}
/// Storage that must outlive the native call, and what to read back afterwards.
#[derive(Default)]
struct Storage {
    ansi: Vec<Box<[u8]>>,
    doubles: Vec<Box<f64>>,
    shorts: Vec<Box<i16>>,
    ints: Vec<Box<i32>>,
    fp: Vec<Box<[f64]>>,
    arena: Arena,
}
fn pointer<T>(boxed: &mut Box<T>) -> usize {
    &mut **boxed as *mut T as usize
}

fn prepare(
    kind: XllType,
    value: Option<&CalcValue>,
    storage: &mut Storage,
    sheets: &mut SheetMap,
) -> Outcome<Prepared> {
    use XllType::*;
    let missing = CalcValue::missing();
    let value = value.unwrap_or(&missing);
    Ok(match kind {
        Double => Prepared::Double(to_number(value)?),
        DoublePointer => {
            let mut boxed = Box::new(to_number(value)?);
            let at = pointer(&mut boxed);
            storage.doubles.push(boxed);
            Prepared::Pointer(at)
        }
        Boolean => Prepared::Short(i16::from(to_logical(value)?)),
        BooleanPointer => {
            let mut boxed = Box::new(i16::from(to_logical(value)?));
            let at = pointer(&mut boxed);
            storage.shorts.push(boxed);
            Prepared::Pointer(at)
        }
        Short => Prepared::Short(signed(to_number(value)?, -32768., 32767.)? as i16),
        ShortPointer => {
            let mut boxed = Box::new(signed(to_number(value)?, -32768., 32767.)? as i16);
            let at = pointer(&mut boxed);
            storage.shorts.push(boxed);
            Prepared::Pointer(at)
        }
        UShort => Prepared::UShort(unsigned(to_number(value)?, 65535.)? as u16),
        Int => Prepared::Int(signed(to_number(value)?, -2147483648., 2147483647.)? as i32),
        IntPointer => {
            let mut boxed = Box::new(signed(to_number(value)?, -2147483648., 2147483647.)? as i32);
            let at = pointer(&mut boxed);
            storage.ints.push(boxed);
            Prepared::Pointer(at)
        }
        CString | CountedString | CStringInPlace | CountedStringInPlace => {
            let ansi = to_ansi(&to_units(value)?);
            let counted = matches!(kind, CountedString | CountedStringInPlace);
            let in_place = matches!(kind, CStringInPlace | CountedStringInPlace);
            if counted && ansi.len() > 255 {
                return value_error();
            }
            let capacity = if in_place { 32768 } else { ansi.len() + 1 };
            let mut buffer = vec![0u8; capacity.max(ansi.len() + 2)].into_boxed_slice();
            if counted {
                buffer[0] = ansi.len() as u8;
                buffer[1..=ansi.len()].copy_from_slice(&ansi);
            } else {
                buffer[..ansi.len()].copy_from_slice(&ansi);
            }
            let at = buffer.as_mut_ptr() as usize;
            storage.ansi.push(buffer);
            Prepared::Pointer(at)
        }
        WideString | WideCountedString | WideStringInPlace | WideCountedStringInPlace => {
            let units = to_units(value)?;
            if units.len() > 32767 {
                return value_error();
            }
            let counted = matches!(kind, WideCountedString | WideCountedStringInPlace);
            let in_place = matches!(kind, WideStringInPlace | WideCountedStringInPlace);
            let at = storage
                .arena
                .wide(&units, counted, in_place)
                .map_err(Fail::Refuse)? as usize;
            Prepared::Pointer(at)
        }
        Fp12 => Prepared::Pointer(
            storage
                .arena
                .fp12(&value.clone())
                .map_err(|_| Fail::Error(WorksheetErrorCode::Value))? as usize,
        ),
        Fp => {
            let (rows, cols, numbers) = numbers_of(value)?;
            let rows = u16::try_from(rows).map_err(|_| Fail::Error(WorksheetErrorCode::Value))?;
            let cols = u16::try_from(cols).map_err(|_| Fail::Error(WorksheetErrorCode::Value))?;
            let mut buffer =
                std::iter::once(f64::from_bits(u64::from(rows) | u64::from(cols) << 16))
                    .chain(numbers)
                    .collect::<Vec<_>>()
                    .into_boxed_slice();
            let at = buffer.as_mut_ptr() as usize;
            storage.fp.push(buffer);
            Prepared::Pointer(at)
        }
        Oper12 | Oper12Reference => {
            let oper = storage.arena.oper(value, sheets).map_err(Fail::Refuse)?;
            Prepared::Pointer(storage.arena.store(oper) as usize)
        }
        other => return refuse(format!("type {other:?} is not modelled for CALL")),
    })
}
fn numbers_of(value: &CalcValue) -> Outcome<(usize, usize, Vec<f64>)> {
    if let Some(array) = value.as_array() {
        let shape = array.shape();
        let mut numbers = Vec::with_capacity(shape.rows * shape.cols);
        for row in 0..shape.rows {
            for col in 0..shape.cols {
                let cell = array.get(row, col).unwrap();
                numbers.push(cell.as_number().map_or_else(value_error, Ok)?);
            }
        }
        Ok((shape.rows, shape.cols, numbers))
    } else {
        Ok((1, 1, vec![value.as_number().map_or_else(value_error, Ok)?]))
    }
}

fn native_type(kind: XllType) -> Option<Type> {
    use XllType::*;
    Some(match kind {
        Double => Type::f64(),
        Boolean | Short => Type::i16(),
        UShort => Type::u16(),
        Int => Type::i32(),
        Void | InPlace(_) => Type::void(),
        _ => Type::pointer(),
    })
}

/// Run an export with Excel's CALL conversions. `values` are the call's arguments after the
/// identifying ones; fewer than declared leave the rest missing (zero), more is #VALUE!.
unsafe fn invoke(
    frame: &mut Frame<'_>,
    module: &Rc<Module>,
    address: usize,
    signature: &XllSignature,
    values: &[CalcValue],
) -> Outcome<CalcValue> {
    if values.len() > signature.arguments.len() {
        return value_error();
    }
    let mut storage = Storage::default();
    let mut prepared = Vec::with_capacity(signature.arguments.len());
    {
        let mut state = frame.state.borrow_mut();
        for (index, kind) in signature.arguments.iter().enumerate() {
            prepared.push(prepare(
                *kind,
                values.get(index),
                &mut storage,
                &mut state.sheets,
            )?);
        }
    }
    let types = signature
        .arguments
        .iter()
        .map(|kind| native_type(*kind))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| Fail::Refuse("dc-gh22: argument type is not modelled".into()))?;
    let result_type = match signature.result {
        XllType::Double => Type::f64(),
        XllType::Boolean | XllType::Short => Type::i16(),
        XllType::UShort => Type::u16(),
        XllType::Int => Type::i32(),
        XllType::Void | XllType::InPlace(_) => Type::void(),
        XllType::Fp | XllType::Array | XllType::Array12 | XllType::AsyncHandle => {
            return refuse("this return type is not modelled for CALL");
        }
        _ => Type::pointer(),
    };
    let cif = Cif::new(types, result_type);
    let arguments = prepared
        .iter()
        .map(|slot| match slot {
            Prepared::Double(value) => Arg::new(value),
            Prepared::Short(value) => Arg::new(value),
            Prepared::UShort(value) => Arg::new(value),
            Prepared::Int(value) => Arg::new(value),
            Prepared::Pointer(value) => Arg::new(value),
        })
        .collect::<Vec<_>>();
    let mut word = 0usize;
    let mut number = 0f64;
    let output = match signature.result {
        XllType::Double => Ret::new(&mut number),
        XllType::Void | XllType::InPlace(_) => Ret::void(),
        _ => Ret::new(&mut word),
    };
    unsafe {
        cif.call_return_into(CodePtr(address as *mut _), &arguments, output);
    }
    let pointer_slot = |index: usize| match prepared.get(index) {
        Some(Prepared::Pointer(at)) => Some(*at),
        _ => None,
    };
    let sheet = current_sheet(&frame.context);
    let value = match signature.result {
        XllType::Double => CalcValue::number(number),
        XllType::Boolean => CalcValue::logical(word as i16 != 0),
        XllType::Short => CalcValue::number(f64::from(word as i16)),
        XllType::UShort => CalcValue::number(f64::from(word as u16)),
        XllType::Int => CalcValue::number(f64::from(word as i32)),
        XllType::Void => CalcValue::logical(true),
        XllType::InPlace(n) => {
            let at = pointer_slot(usize::from(n) - 1)
                .ok_or_else(|| Fail::Refuse("dc-gh22: in-place return needs a pointer".into()))?;
            unsafe { read_pointer(signature.arguments[usize::from(n) - 1], at)? }
        }
        XllType::Oper12 | XllType::Oper12Reference => {
            let oper = word as *mut XlOper12;
            if oper.is_null() {
                CalcValue::error(WorksheetErrorCode::Num)
            } else {
                let flags = unsafe { oper.read_unaligned().xltype };
                let copy = unsafe { from_oper(oper, &frame.state.borrow().sheets, sheet) }
                    .map_err(Fail::Refuse)?;
                if flags & DLL_FREE != 0 {
                    match module.address("xlAutoFree12") {
                        Ok(free) => {
                            let free: unsafe extern "system" fn(*mut XlOper12) =
                                unsafe { std::mem::transmute(free) };
                            unsafe { free(oper) };
                        }
                        Err(why) => {
                            return refuse(format!("DLLFree result without xlAutoFree12: {why}"));
                        }
                    }
                } else if flags & XL_FREE != 0 {
                    callbacks::free(&frame.state, &unsafe { oper.read_unaligned() });
                }
                copy
            }
        }
        kind => {
            if word == 0 {
                CalcValue::error(WorksheetErrorCode::Num)
            } else {
                unsafe { read_pointer(kind, word)? }
            }
        }
    };
    Ok(crate::invoke::normalize_return(value))
}

/// Read the value a pointer-typed argument or result designates.
unsafe fn read_pointer(kind: XllType, at: usize) -> Outcome<CalcValue> {
    use XllType::*;
    Ok(match kind {
        DoublePointer => CalcValue::number(unsafe { (at as *const f64).read_unaligned() }),
        BooleanPointer => CalcValue::logical(unsafe { (at as *const i16).read_unaligned() } != 0),
        ShortPointer => {
            CalcValue::number(f64::from(unsafe { (at as *const i16).read_unaligned() }))
        }
        IntPointer => CalcValue::number(f64::from(unsafe { (at as *const i32).read_unaligned() })),
        CString | CStringInPlace => {
            let mut bytes = Vec::new();
            while bytes.len() < 32768 {
                let byte = unsafe { (at as *const u8).add(bytes.len()).read() };
                if byte == 0 {
                    break;
                }
                bytes.push(byte);
            }
            CalcValue::text(ExcelText::from_utf16_code_units(from_ansi(&bytes)))
        }
        CountedString | CountedStringInPlace => {
            let length = usize::from(unsafe { (at as *const u8).read() });
            let bytes = unsafe { std::slice::from_raw_parts((at as *const u8).add(1), length) };
            CalcValue::text(ExcelText::from_utf16_code_units(from_ansi(bytes)))
        }
        WideString | WideStringInPlace => {
            let mut units = Vec::new();
            while units.len() < 32768 {
                let unit = unsafe { (at as *const u16).add(units.len()).read_unaligned() };
                if unit == 0 {
                    break;
                }
                units.push(unit);
            }
            CalcValue::text(ExcelText::from_utf16_code_units(units))
        }
        WideCountedString | WideCountedStringInPlace => {
            let length = usize::from(unsafe { (at as *const u16).read_unaligned() });
            let units = unsafe { std::slice::from_raw_parts((at as *const u16).add(1), length) };
            CalcValue::text(ExcelText::from_utf16_code_units(units.to_vec()))
        }
        Fp12 => {
            let rows = unsafe { (at as *const i32).read_unaligned() };
            let columns = unsafe { (at as *const i32).add(1).read_unaligned() };
            let (Ok(rows), Ok(cols)) = (usize::try_from(rows), usize::try_from(columns)) else {
                return refuse("negative FP12 shape");
            };
            let count = rows
                .checked_mul(cols)
                .filter(|n| *n > 0 && *n <= 1_048_576)
                .ok_or_else(|| Fail::Refuse("dc-gh22: oversized FP12 result".into()))?;
            let data = (0..count)
                .map(|i| {
                    CalcValue::number(unsafe {
                        (at as *const u8)
                            .add(8)
                            .cast::<f64>()
                            .add(i)
                            .read_unaligned()
                    })
                })
                .collect::<Vec<_>>();
            CalcValue::array(
                calc_value::CalcArray::new(calc_value::ArrayShape { rows, cols }, data)
                    .ok_or_else(|| Fail::Refuse("dc-gh22: invalid FP12 shape".into()))?,
            )
        }
        other => return refuse(format!("result type {other:?} is not modelled for CALL")),
    })
}

fn put(frame: &mut Frame<'_>, result: *mut XlOper12, value: &CalcValue) -> i32 {
    let mut arena = Arena::new();
    let oper = arena.oper(value, &mut frame.state.borrow_mut().sheets);
    match oper {
        Ok(oper) => {
            // Excel keeps a callback result valid until the calling function returns and sets
            // no ownership flag on it (measured), so the frame owns the storage.
            frame.transient.push(arena);
            if !result.is_null() {
                unsafe { result.write_unaligned(oper) };
            }
            SUCCESS
        }
        Err(why) => {
            callbacks::refuse_frame(frame, &why);
            FAILED
        }
    }
}
fn fail_code(result: *mut XlOper12, code: i32) -> i32 {
    // Excel leaves #VALUE! in the result operand of a failing CALL / REGISTER.ID.
    if !result.is_null() {
        unsafe { result.write_unaligned(XlOper12::error(15)) };
    }
    code
}

/// Open the module the way Excel does: an already loaded module by name, otherwise LoadLibrary.
fn open_module(name: &str) -> Option<Rc<Module>> {
    Module::open_library(name).ok()
}
fn find_registration(
    frame: &Frame<'_>,
    module: &Rc<Module>,
    address: usize,
) -> Option<Rc<Registration>> {
    frame
        .state
        .borrow()
        .registrations
        .values()
        .find(|r| r.uses.get() > 0 && r.module.handle == module.handle && r.info.address == address)
        .cloned()
}
fn register_hidden(
    frame: &mut Frame<'_>,
    module: Rc<Module>,
    procedure: &str,
    address: usize,
    type_text: &str,
    signature: XllSignature,
) -> Rc<Registration> {
    let mut state = frame.state.borrow_mut();
    let id = state.next_id;
    state.next_id += 1;
    let info = RegistrationInfo {
        id,
        name: String::new(),
        procedure: procedure.to_owned(),
        type_text: type_text.to_owned(),
        signature,
        address,
        module: module.path.clone(),
        origin: frame.origin.path.clone(),
        use_count: 1,
        macro_type: 1,
    };
    let cif = crate::invoke::call_interface(&info.signature);
    let registration = Rc::new(Registration {
        info,
        module,
        uses: Cell::new(1),
        origin: frame.origin.clone(),
        cif,
    });
    state.registrations.insert(id, registration.clone());
    registration
}
/// The signature of a registration made without type text: no arguments and no result.
fn typeless() -> XllSignature {
    XllSignature {
        result: XllType::Void,
        arguments: Vec::new(),
        volatile: false,
        macro_equivalent: false,
        thread_safe: false,
        cluster_safe: false,
        asynchronous: false,
    }
}
/// CALL type text: case-insensitive, only the letters of a plain registration.
fn parse_type(text: &str) -> Option<XllSignature> {
    let signature = XllSignature::parse(&text.to_ascii_uppercase()).ok()?;
    (!signature.asynchronous).then_some(signature)
}
fn text_of(value: &CalcValue) -> Option<String> {
    match value.core_ref() {
        CoreRef::Text(text) => String::from_utf16(text.utf16_code_units()).ok(),
        _ => None,
    }
}

/// `Excel12(xlfCall, ...)` and `Excel12(xlfRegisterId, ...)`. Returns the C API return code.
pub(crate) unsafe fn dispatch(
    frame: &mut Frame<'_>,
    function: i32,
    args: &[*mut XlOper12],
    result: *mut XlOper12,
) -> i32 {
    // Both are macro-sheet functions: a worksheet function needs the `#` registration flag.
    if let Some(signature) = &frame.signature {
        if signature.thread_safe {
            return fail_code(result, NOT_THREAD_SAFE);
        }
        if !signature.macro_equivalent {
            return fail_code(result, INVALID_FUNCTION);
        }
    }
    let values = {
        let state = frame.state.borrow();
        let sheet = current_sheet(&frame.context);
        args.iter()
            .map(|p| unsafe { from_oper(*p, &state.sheets, sheet) })
            .collect::<Result<Vec<_>, _>>()
    };
    let mut values = match values {
        Ok(values) => values,
        Err(why) => {
            callbacks::refuse_frame(frame, &why);
            return FAILED;
        }
    };
    for value in &mut values {
        if let Some(reference) = value.as_reference().cloned() {
            let Some(context) = frame.context.as_mut() else {
                callbacks::refuse_frame(frame, "dc-gh22: a reference argument needs a reader");
                return FAILED;
            };
            match context.argument_values_of(&reference) {
                Ok(read) => *value = read,
                Err(why) => {
                    callbacks::refuse_frame(frame, &why);
                    return FAILED;
                }
            }
        }
    }
    let outcome = if function == XLF_CALL {
        if values.is_empty() {
            return fail_code(result, INVALID_COUNT);
        }
        unsafe { call(frame, &values) }
    } else {
        if !(2..=3).contains(&values.len()) {
            return fail_code(result, INVALID_COUNT);
        }
        unsafe { register_id(frame, &values) }
    };
    match outcome {
        Ok(value) => put(frame, result, &value),
        Err(Fail::Error(code)) => put(frame, result, &CalcValue::error(code)),
        Err(Fail::Refuse(why)) => {
            callbacks::refuse_frame(frame, &why);
            FAILED
        }
    }
}

unsafe fn call(frame: &mut Frame<'_>, values: &[CalcValue]) -> Outcome<CalcValue> {
    let first = &values[0];
    if let Some(error) = first.as_error() {
        return Ok(CalcValue::error(error));
    }
    if first.as_logical().is_some() {
        return value_error();
    }
    if let Some(id) = first.as_number() {
        // CALL(register_id, arguments...)
        let registration = if id.fract() == 0. && id >= 0. {
            frame
                .state
                .borrow()
                .registrations
                .get(&(id as u64))
                .cloned()
        } else {
            None
        };
        let Some(registration) = registration.filter(|r| r.uses.get() > 0) else {
            return value_error();
        };
        return unsafe {
            invoke(
                frame,
                &registration.module.clone(),
                registration.info.address,
                &registration.info.signature.clone(),
                &values[1..],
            )
        };
    }
    // CALL(module, procedure[, type_text, arguments...])
    let Some(module_name) = text_of(first) else {
        return value_error();
    };
    if values.len() < 2 {
        return value_error();
    }
    let Some(procedure) = text_of(&values[1]) else {
        return value_error();
    };
    let Some(module) = open_module(&module_name) else {
        return value_error();
    };
    let Ok(address) = module.address(&procedure) else {
        return value_error();
    };
    if values.len() == 2 {
        // No type text: only an export that is already registered can be called.
        let Some(registration) = find_registration(frame, &module, address) else {
            return value_error();
        };
        return unsafe {
            invoke(
                frame,
                &registration.module.clone(),
                registration.info.address,
                &registration.info.signature.clone(),
                &[],
            )
        };
    }
    let Some(type_text) = text_of(&values[2]).as_deref().and_then(parse_type_text) else {
        return value_error();
    };
    let (type_text, signature) = type_text;
    if find_registration(frame, &module, address).is_none() {
        register_hidden(
            frame,
            module.clone(),
            &procedure,
            address,
            &type_text,
            signature.clone(),
        );
    }
    unsafe { invoke(frame, &module, address, &signature, &values[3..]) }
}
fn parse_type_text(text: &str) -> Option<(String, XllSignature)> {
    if text.is_empty() {
        return None;
    }
    parse_type(text).map(|signature| (text.to_owned(), signature))
}

unsafe fn register_id(frame: &mut Frame<'_>, values: &[CalcValue]) -> Outcome<CalcValue> {
    let (Some(module_name), Some(procedure)) = (text_of(&values[0]), text_of(&values[1])) else {
        return value_error();
    };
    // A text type is checked even when the export is already registered; any other third
    // argument (empty text, a number) registers an export without type text.
    let type_text = match values.get(2) {
        None => None,
        Some(value) => match text_of(value) {
            Some(text) if !text.is_empty() => match parse_type(&text) {
                Some(signature) => Some((text, signature)),
                None => return value_error(),
            },
            _ => Some((String::new(), typeless())),
        },
    };
    let Some(module) = open_module(&module_name) else {
        return value_error();
    };
    let Ok(address) = module.address(&procedure) else {
        return value_error();
    };
    if let Some(existing) = find_registration(frame, &module, address) {
        return Ok(CalcValue::number(existing.info.id as f64));
    }
    let Some((type_text, signature)) = type_text else {
        return value_error();
    };
    let registration = register_hidden(frame, module, &procedure, address, &type_text, signature);
    Ok(CalcValue::number(registration.info.id as f64))
}
