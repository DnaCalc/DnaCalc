//! Descriptor decoding for vtables and IDispatch. Both produce the same Call.
use crate::{
    projection::State,
    runtime::{Wrapper, abi, error},
    values::{self, Objects},
};
use calc_object_model::{
    AutoError, AutoValue, Call, Descriptor, InterfaceId, InvokeKind, Member, ParamFlag,
    descriptor::TypeRef,
};
use std::{ffi::c_void, ptr, rc::Rc};
use windows::{
    Win32::{
        Foundation::*,
        System::{Com::*, Variant::*},
    },
    core::{GUID, HRESULT, Interface, PCWSTR},
};

fn pick(interface: InterfaceId, dispid: i32, flags: u16) -> Option<&'static Member> {
    let descriptor = Descriptor::get();
    [
        (DISPATCH_PROPERTYPUTREF.0, InvokeKind::Putref),
        (DISPATCH_PROPERTYPUT.0, InvokeKind::Put),
        (DISPATCH_METHOD.0, InvokeKind::Method),
        (DISPATCH_PROPERTYGET.0, InvokeKind::Get),
    ]
    .into_iter()
    .filter(|(flag, _)| flags & *flag != 0)
    .find_map(|(_, kind)| descriptor.member_by_dispid(interface, dispid, kind))
    // Automation permits clients to ask a method through PROPERTYGET (Office.Item).
    .or_else(|| {
        (flags & DISPATCH_PROPERTYGET.0 != 0)
            .then(|| descriptor.member_by_dispid(interface, dispid, InvokeKind::Method))
            .flatten()
    })
}

pub(crate) unsafe extern "system" fn names(
    this: *mut Wrapper,
    iid: *const GUID,
    names: *const PCWSTR,
    count: u32,
    _: u32,
    ids: *mut i32,
) -> HRESULT {
    abi(|| {
        if iid.is_null() || names.is_null() || ids.is_null() {
            return E_POINTER;
        }
        if unsafe { *iid } != GUID::zeroed() {
            return DISP_E_UNKNOWNINTERFACE;
        }
        let wrapper = unsafe { &*this };
        if let Err(e) = wrapper.state() {
            return error(&e, ptr::null_mut());
        }
        if count == 0 {
            return E_INVALIDARG;
        }
        let descriptor = Descriptor::get();
        let names = unsafe { std::slice::from_raw_parts(names, count as usize) };
        let ids = unsafe { std::slice::from_raw_parts_mut(ids, count as usize) };
        ids.fill(-1);
        if names.iter().any(|n| n.is_null()) {
            return E_POINTER;
        }
        let first = unsafe { names[0].to_string() }.unwrap_or_default();
        let members = descriptor.members(wrapper.target.interface);
        let Some(member) = members
            .iter()
            .find(|member| member.name.eq_ignore_ascii_case(&first))
        else {
            return DISP_E_UNKNOWNNAME;
        };
        ids[0] = member.dispid;
        let mut result = S_OK;
        for (name, out) in names.iter().zip(ids.iter_mut()).skip(1) {
            let text = unsafe { name.to_string() }.unwrap_or_default();
            // Getter and setter parameter lists may differ. Resolve a name on any overload
            // with this DISPID; Invoke checks it against the selected invocation kind.
            let found = members
                .iter()
                .filter(|m| m.dispid == member.dispid)
                .find_map(|m| {
                    m.arguments().position(|p| {
                        p.name
                            .as_deref()
                            .is_some_and(|n| n.eq_ignore_ascii_case(&text))
                    })
                });
            if let Some(position) = found {
                *out = position as i32;
            } else {
                result = DISP_E_UNKNOWNNAME;
            }
        }
        result
    })
}

pub(crate) unsafe extern "system" fn invoke(
    this: *mut Wrapper,
    dispid: i32,
    iid: *const GUID,
    lcid: u32,
    flags: u16,
    params: *const DISPPARAMS,
    output: *mut VARIANT,
    exception: *mut EXCEPINFO,
    arg_error: *mut u32,
) -> HRESULT {
    abi(|| {
        if !output.is_null() {
            unsafe { output.write(VARIANT::default()) };
        }
        if !arg_error.is_null() {
            unsafe { *arg_error = 0 };
        }
        if iid.is_null() || params.is_null() {
            return E_POINTER;
        }
        if unsafe { *iid } != GUID::zeroed() {
            return DISP_E_UNKNOWNINTERFACE;
        }
        let wrapper = unsafe { &*this };
        let state = match wrapper.state() {
            Ok(s) => s,
            Err(e) => return error(&e, exception),
        };
        let Some(member) = pick(wrapper.target.interface, dispid, flags) else {
            return DISP_E_MEMBERNOTFOUND;
        };
        let result = (|| {
            let _lease = state.office.prepare(&wrapper.target, member)?;
            let _arguments = state.argument_scope();
            let params = unsafe { &*params };
            if params.cNamedArgs > params.cArgs
                || params.cArgs as usize > member.arguments().count()
            {
                return Err(AutoError::Status(DISP_E_BADPARAMCOUNT.0));
            }
            if (params.cArgs > 0 && params.rgvarg.is_null())
                || (params.cNamedArgs > 0 && params.rgdispidNamedArgs.is_null())
            {
                return Err(AutoError::Status(E_POINTER.0));
            }
            let declared: Vec<_> = member.arguments().collect();
            let raw = if params.cArgs == 0 {
                &[]
            } else {
                unsafe { std::slice::from_raw_parts(params.rgvarg, params.cArgs as usize) }
            };
            let named = if params.cNamedArgs == 0 {
                &[]
            } else {
                unsafe {
                    std::slice::from_raw_parts(params.rgdispidNamedArgs, params.cNamedArgs as usize)
                }
            };
            let slots = crate::arguments::bind(member, raw.len(), named).map_err(|error| {
                if !arg_error.is_null() {
                    unsafe { *arg_error = error.argument as u32 };
                }
                AutoError::Status(error.code)
            })?;
            let mut args = vec![AutoValue::Missing; declared.len()];
            for (raw_index, (&slot, raw_value)) in slots.iter().zip(raw).enumerate() {
                if !arg_error.is_null() {
                    unsafe { *arg_error = raw_index as u32 };
                }
                args[slot] = values::from_variant(&state, raw_value)?;
            }
            let value = state.call(
                &wrapper.target,
                &Call {
                    interface: wrapper.target.interface,
                    member,
                    args,
                    lcid,
                },
            )?;
            let objects = state.result_values(std::slice::from_ref(&value));
            if !output.is_null() {
                unsafe { output.write(values::result_variant(&objects, &value, member)?) };
            }
            Ok(())
        })();
        match result {
            Ok(()) => S_OK,
            Err(e) => {
                state.record(&e);
                error(&e, exception)
            }
        }
    })
}

pub(crate) unsafe fn decode_slot(
    this: *mut Wrapper,
    interface: InterfaceId,
    slot: u16,
    raw: &[usize],
) -> HRESULT {
    let wrapper = unsafe { &*this };
    let state = match wrapper.state() {
        Ok(s) => s,
        Err(e) => return error(&e, ptr::null_mut()),
    };
    let Some(member) = Descriptor::get().member_by_slot(interface, slot) else {
        return E_UNEXPECTED;
    };
    let result = (|| {
        let _lease = state.office.prepare(&wrapper.target, member)?;
        let _arguments = state.argument_scope();
        let mut args = Vec::new();
        let mut lcid = 0;
        let mut retval = None;
        for (param, &value) in member.params.iter().zip(raw) {
            if param.has(ParamFlag::Retval) {
                if value == 0 {
                    return Err(AutoError::Status(E_POINTER.0));
                }
                unsafe {
                    zero_out(value, &param.type_ref())?;
                }
                retval = Some((value, param.type_ref()));
            } else if param.has(ParamFlag::Lcid) {
                lcid = value as u32;
            } else if param.has(ParamFlag::Out) && !param.has(ParamFlag::In) {
                if value == 0 {
                    return Err(AutoError::Status(E_POINTER.0));
                }
                unsafe {
                    zero_out(value, &param.type_ref())?;
                }
                args.push(AutoValue::Missing);
            } else {
                args.push(unsafe { read_abi(&state, value, &param.type_ref())? });
            }
        }
        let value = state.call(
            &wrapper.target,
            &Call {
                interface,
                member,
                args,
                lcid,
            },
        )?;
        let objects = state.result_values(std::slice::from_ref(&value));
        if let Some((pointer, ty)) = retval {
            unsafe { write_abi(&objects, pointer, &ty, &value) }?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => S_OK,
        Err(e) => {
            state.record(&e);
            error(&e, ptr::null_mut())
        }
    }
}

unsafe fn read_abi(state: &Rc<State>, raw: usize, ty: &TypeRef) -> Result<AutoValue, AutoError> {
    Ok(match ty.underlying() {
        TypeRef::Base(base) => match base.as_str() {
            "VARIANT" => {
                if raw == 0 {
                    return Err(AutoError::Status(E_POINTER.0));
                }
                values::from_variant(state, unsafe { &*(raw as *const VARIANT) })?
            }
            "BSTR" => unsafe { values::bstr_text(raw) },
            "I2" => AutoValue::I2(raw as i16),
            "BOOL" => AutoValue::Bool(raw as i16 != 0),
            "I4" | "INT" => AutoValue::I4(raw as i32),
            "R4" => AutoValue::R4(f32::from_bits(raw as u32)),
            "R8" => AutoValue::R8(f64::from_bits(raw as u64)),
            "DATE" => AutoValue::Date(f64::from_bits(raw as u64)),
            "CY" => AutoValue::Currency(raw as i64),
            _ => return values::mismatch(),
        },
        TypeRef::Pointer(inner) => {
            if raw == 0 && matches!(&*inner, TypeRef::Named(_)) {
                return Ok(AutoValue::Empty);
            }
            if raw == 0 {
                return Err(AutoError::Status(E_POINTER.0));
            }
            match &*inner {
                TypeRef::Named(_) => {
                    let raw = raw as *mut c_void;
                    let object = unsafe { IDispatch::from_raw_borrowed(&raw) }
                        .ok_or(AutoError::Status(E_POINTER.0))?
                        .clone();
                    state.identify(&object)?
                }
                TypeRef::Base(base) if base == "VARIANT" => {
                    values::from_variant(state, unsafe { &*(raw as *const VARIANT) })?
                }
                TypeRef::Base(base) if base == "I4" || base == "INT" => {
                    AutoValue::I4(unsafe { *(raw as *const i32) })
                }
                TypeRef::Base(base) if base == "BOOL" => {
                    AutoValue::Bool(unsafe { *(raw as *const i16) } != 0)
                }
                TypeRef::Base(base) if base == "I2" => {
                    AutoValue::I2(unsafe { *(raw as *const i16) })
                }
                TypeRef::Base(base) if base == "BSTR" => unsafe {
                    values::bstr_text(*(raw as *const usize))
                },
                TypeRef::Base(base) if base == "R4" => {
                    AutoValue::R4(unsafe { *(raw as *const f32) })
                }
                TypeRef::Base(base) if base == "R8" => {
                    AutoValue::R8(unsafe { *(raw as *const f64) })
                }
                TypeRef::Base(base) if base == "DATE" => {
                    AutoValue::Date(unsafe { *(raw as *const f64) })
                }
                TypeRef::Base(base) if base == "CY" => {
                    AutoValue::Currency(unsafe { *(raw as *const i64) })
                }
                TypeRef::Pointer(_) => unsafe { read_abi(state, *(raw as *const usize), &inner)? },
                TypeRef::SafeArray(_) => unsafe {
                    values::from_array(state, *(raw as *const *mut SAFEARRAY))?
                },
                _ => return values::mismatch(),
            }
        }
        _ => return values::mismatch(),
    })
}
fn out_type(ty: &TypeRef) -> TypeRef {
    match ty.underlying() {
        TypeRef::Pointer(inner) => *inner,
        other => other,
    }
}
unsafe fn zero_out(raw: usize, ty: &TypeRef) -> Result<(), AutoError> {
    let size = match out_type(ty) {
        TypeRef::Base(base) if base == "VARIANT" => size_of::<VARIANT>(),
        TypeRef::Base(base) if base == "BOOL" || base == "I2" => 2,
        TypeRef::Base(base) if ["I4", "INT", "R4", "ERROR"].contains(&base.as_str()) => 4,
        _ => 8,
    };
    unsafe { ptr::write_bytes(raw as *mut u8, 0, size) };
    Ok(())
}
unsafe fn write_abi(
    objects: &dyn Objects,
    raw: usize,
    ty: &TypeRef,
    value: &AutoValue,
) -> Result<(), AutoError> {
    unsafe {
        match out_type(ty) {
            TypeRef::Base(base) if base == "VARIANT" => {
                (raw as *mut VARIANT).write(values::to_variant(objects, value)?)
            }
            TypeRef::Base(base) if base == "BSTR" => {
                let AutoValue::Text(text) = value else {
                    return values::mismatch();
                };
                (raw as *mut windows::core::BSTR)
                    .write(windows::core::BSTR::from_wide(text.utf16_code_units()));
            }
            TypeRef::Base(base) => match base.as_str() {
                "BOOL" => {
                    (raw as *mut i16).write(if values::number(value)? != 0.0 { -1 } else { 0 })
                }
                "I2" => (raw as *mut i16).write(values::number(value)? as i16),
                "I4" | "INT" => (raw as *mut i32).write(values::number(value)? as i32),
                "R4" => (raw as *mut f32).write(values::number(value)? as f32),
                "R8" | "DATE" => (raw as *mut f64).write(values::number(value)?),
                "CY" => {
                    let AutoValue::Currency(value) = value else {
                        return values::mismatch();
                    };
                    (raw as *mut i64).write(*value);
                }
                _ => return values::mismatch(),
            },
            TypeRef::Pointer(inner) => {
                let TypeRef::Named(name) = &*inner else {
                    return values::mismatch();
                };
                let object = values::interface_result(objects, value, name)?
                    .map_or(ptr::null_mut(), Interface::into_raw);
                (raw as *mut *mut c_void).write(object);
            }
            _ => return values::mismatch(),
        }
    }
    Ok(())
}
