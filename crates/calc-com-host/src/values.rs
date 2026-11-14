//! The only VARIANT/Automation conversion. Both COM call routes and RTD use it.
use calc_object_model::{AutoArray, AutoError, AutoValue, ObjectRef, descriptor::TypeRef};
use calc_value::ExcelText;
use std::mem::ManuallyDrop;
use windows::{
    Win32::{
        Foundation::*,
        System::{Com::*, Ole::*, Variant::*},
    },
    core::{BSTR, IUnknown, Interface},
};

pub(crate) fn status(error: windows::core::Error) -> AutoError {
    AutoError::Status(error.code().0)
}
pub(crate) fn mismatch<T>() -> Result<T, AutoError> {
    Err(AutoError::Status(DISP_E_TYPEMISMATCH.0))
}

pub(crate) trait Objects {
    fn wrap(&self, target: &ObjectRef) -> Result<IDispatch, AutoError>;
    fn identify(&self, dispatch: &IDispatch) -> Result<AutoValue, AutoError>;
    fn foreign(&self, key: u64) -> Result<IDispatch, AutoError>;
}

pub(crate) struct NoObjects;
impl Objects for NoObjects {
    fn wrap(&self, _: &ObjectRef) -> Result<IDispatch, AutoError> {
        mismatch()
    }
    fn identify(&self, _: &IDispatch) -> Result<AutoValue, AutoError> {
        mismatch()
    }
    fn foreign(&self, _: u64) -> Result<IDispatch, AutoError> {
        mismatch()
    }
}

pub(crate) fn tagged(vt: VARENUM, data: VARIANT_0_0_0) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: data,
            }),
        },
    }
}

pub(crate) fn to_variant(objects: &dyn Objects, value: &AutoValue) -> Result<VARIANT, AutoError> {
    Ok(match value {
        AutoValue::Empty => VARIANT::default(),
        AutoValue::Null => tagged(VT_NULL, VARIANT_0_0_0::default()),
        AutoValue::Missing => tagged(
            VT_ERROR,
            VARIANT_0_0_0 {
                scode: DISP_E_PARAMNOTFOUND.0,
            },
        ),
        AutoValue::Bool(value) => (*value).into(),
        AutoValue::I2(value) => (*value).into(),
        AutoValue::I4(value) => (*value).into(),
        AutoValue::R4(value) => (*value).into(),
        AutoValue::R8(value) => (*value).into(),
        AutoValue::Currency(value) => tagged(
            VT_CY,
            VARIANT_0_0_0 {
                cyVal: CY { int64: *value },
            },
        ),
        AutoValue::Date(value) => tagged(VT_DATE, VARIANT_0_0_0 { date: *value }),
        AutoValue::Text(value) => BSTR::from_wide(value.utf16_code_units()).into(),
        AutoValue::Error(number) => tagged(
            VT_ERROR,
            VARIANT_0_0_0 {
                scode: AutoValue::scode(*number),
            },
        ),
        AutoValue::Object(target) => objects.wrap(target)?.into(),
        AutoValue::Foreign(key) => objects.foreign(*key)?.into(),
        AutoValue::Array(array) => {
            if array.rows == 0
                || array.columns == 0
                || array.rows > i32::MAX as u32
                || array.columns > i32::MAX as u32
                || u64::from(array.rows) * u64::from(array.columns) != array.values.len() as u64
            {
                return Err(AutoError::Status(E_INVALIDARG.0));
            }
            // SafeArrayCreate takes bounds in logical dimension order. The storage order is
            // private to OLE; Get/PutElement take logical [row, column] indices as well.
            let bounds = [
                SAFEARRAYBOUND {
                    cElements: array.rows,
                    lLbound: 1,
                },
                SAFEARRAYBOUND {
                    cElements: array.columns,
                    lLbound: 1,
                },
            ];
            let held = ArrayOwner::new(unsafe { SafeArrayCreate(VT_VARIANT, 2, bounds.as_ptr()) })?;
            for row in 0..array.rows {
                for column in 0..array.columns {
                    let value = to_variant(
                        objects,
                        &array.values[row as usize * array.columns as usize + column as usize],
                    )?;
                    unsafe {
                        SafeArrayPutElement(
                            held.0,
                            [row as i32 + 1, column as i32 + 1].as_ptr(),
                            (&value as *const VARIANT).cast(),
                        )
                    }
                    .map_err(status)?;
                }
            }
            tagged(
                VT_ARRAY | VT_VARIANT,
                VARIANT_0_0_0 {
                    parray: held.take(),
                },
            )
        }
    })
}

pub(crate) fn result_variant(
    objects: &dyn Objects,
    value: &AutoValue,
    member: &calc_object_model::Member,
) -> Result<VARIANT, AutoError> {
    let result = if let Some(param) = member.retval() {
        match param.type_ref().underlying() {
            TypeRef::Pointer(inner) => *inner,
            other => other,
        }
    } else {
        TypeRef::parse(&member.ret).underlying()
    };
    if let TypeRef::Pointer(inner) = result {
        if let TypeRef::Named(name) = &*inner {
            let object = interface_result(objects, value, name)?;
            return Ok(if name == "IUnknown" {
                tagged(
                    VT_UNKNOWN,
                    VARIANT_0_0_0 {
                        punkVal: ManuallyDrop::new(object),
                    },
                )
            } else {
                // The declared interface derives from IDispatch; QI, not the original
                // wrapper's default pointer, supplies the interface being returned.
                let dispatch =
                    object.map(|object| unsafe { IDispatch::from_raw(object.into_raw()) });
                tagged(
                    VT_DISPATCH,
                    VARIANT_0_0_0 {
                        pdispVal: ManuallyDrop::new(dispatch),
                    },
                )
            });
        }
    }
    to_variant(objects, value)
}

pub(crate) fn interface_result(
    objects: &dyn Objects,
    value: &AutoValue,
    name: &str,
) -> Result<Option<IUnknown>, AutoError> {
    let object = match value {
        AutoValue::Object(target) => objects.wrap(target)?,
        AutoValue::Foreign(key) => objects.foreign(*key)?,
        AutoValue::Empty | AutoValue::Null => return Ok(None),
        _ => return mismatch(),
    };
    let iid = match name {
        "IUnknown" => IUnknown::IID,
        "IDispatch" => IDispatch::IID,
        _ => {
            let descriptor = calc_object_model::Descriptor::get();
            let id = descriptor
                .id(name)
                .ok_or(AutoError::Status(E_NOINTERFACE.0))?;
            crate::runtime::guid(descriptor.default_interface(id))
        }
    };
    let mut raw = std::ptr::null_mut();
    unsafe { object.query(&iid, &mut raw) }
        .ok()
        .map_err(status)?;
    Ok(Some(unsafe { IUnknown::from_raw(raw) }))
}

/// VariantCopyInd handles every legal VT_BYREF scalar, object and array representation.
pub(crate) fn from_variant(
    objects: &dyn Objects,
    source: &VARIANT,
) -> Result<AutoValue, AutoError> {
    let mut value = VARIANT::default();
    unsafe { VariantCopyInd(&mut value, source) }.map_err(status)?;
    unsafe {
        let data = &value.Anonymous.Anonymous.Anonymous;
        Ok(match value.vt() {
            VT_EMPTY => AutoValue::Empty,
            VT_NULL => AutoValue::Null,
            VT_BOOL => AutoValue::Bool(data.boolVal.0 != 0),
            VT_I2 => AutoValue::I2(data.iVal),
            VT_I4 | VT_INT => AutoValue::I4(data.lVal),
            VT_UI1 => AutoValue::I2(i16::from(data.bVal)),
            VT_I1 => AutoValue::I2(i16::from(data.cVal)),
            VT_UI2 => AutoValue::I4(i32::from(data.uiVal)),
            VT_UI4 | VT_UINT if data.ulVal <= i32::MAX as u32 => AutoValue::I4(data.ulVal as i32),
            VT_UI4 | VT_UINT => AutoValue::R8(f64::from(data.ulVal)),
            VT_R4 => AutoValue::R4(data.fltVal),
            VT_R8 => AutoValue::R8(data.dblVal),
            VT_CY => AutoValue::Currency(data.cyVal.int64),
            VT_DATE => AutoValue::Date(data.date),
            VT_BSTR => AutoValue::Text(ExcelText::from_utf16_code_units(data.bstrVal.to_vec())),
            VT_ERROR if data.scode == DISP_E_PARAMNOTFOUND.0 => AutoValue::Missing,
            VT_ERROR => AutoValue::Error(data.scode & 0xffff),
            VT_DISPATCH => match data.pdispVal.as_ref() {
                Some(dispatch) => objects.identify(dispatch)?,
                None => AutoValue::Empty,
            },
            VT_UNKNOWN => match data.punkVal.as_ref() {
                Some(unknown) => objects.identify(&unknown.cast::<IDispatch>().map_err(status)?)?,
                None => AutoValue::Empty,
            },
            kind if kind == VT_ARRAY | VT_VARIANT => from_array(objects, data.parray)?,
            kind if kind.0 & VT_ARRAY.0 != 0 => {
                return Err(AutoError::not_modelled(
                    "Automation SAFEARRAY element type (E11)",
                    "dc-1nbp.6",
                ));
            }
            _ => return mismatch(),
        })
    }
}

pub(crate) unsafe fn from_array(
    objects: &dyn Objects,
    array: *const SAFEARRAY,
) -> Result<AutoValue, AutoError> {
    let (r, c, end_r, end_c) = unsafe { array_bounds(array)? };
    let rows = (i64::from(end_r) - i64::from(r) + 1).max(0) as u32;
    let columns = (i64::from(end_c) - i64::from(c) + 1).max(0) as u32;
    let mut values = Vec::new();
    values
        .try_reserve_exact(
            (rows as usize)
                .checked_mul(columns as usize)
                .ok_or(AutoError::Status(E_OUTOFMEMORY.0))?,
        )
        .map_err(|_| AutoError::Status(E_OUTOFMEMORY.0))?;
    for row in r..=end_r {
        for column in c..=end_c {
            values.push(from_variant(objects, &unsafe {
                array_element(array, row, column)?
            })?);
        }
    }
    Ok(AutoValue::Array(Box::new(AutoArray {
        rows,
        columns,
        lower: (r, c),
        values,
    })))
}

/// Logical lower/upper bounds of a two-dimensional SAFEARRAY(VARIANT).
pub(crate) unsafe fn array_bounds(
    array: *const SAFEARRAY,
) -> Result<(i32, i32, i32, i32), AutoError> {
    if array.is_null() {
        return mismatch();
    }
    if unsafe { SafeArrayGetDim(array) } != 2 {
        return Err(AutoError::not_modelled(
            "Automation SAFEARRAY rank (E11)",
            "dc-1nbp.6",
        ));
    }
    let kind = unsafe { SafeArrayGetVartype(array) }.map_err(status)?;
    if kind != VT_VARIANT {
        return Err(AutoError::not_modelled(
            "Automation SAFEARRAY element type (E11)",
            "dc-1nbp.6",
        ));
    }
    let (r, c, end_r, end_c) = unsafe {
        (
            SafeArrayGetLBound(array, 1),
            SafeArrayGetLBound(array, 2),
            SafeArrayGetUBound(array, 1),
            SafeArrayGetUBound(array, 2),
        )
    };
    Ok((
        r.map_err(status)?,
        c.map_err(status)?,
        end_r.map_err(status)?,
        end_c.map_err(status)?,
    ))
}
pub(crate) unsafe fn array_element(
    array: *const SAFEARRAY,
    row: i32,
    column: i32,
) -> Result<VARIANT, AutoError> {
    let mut value = VARIANT::default();
    unsafe {
        SafeArrayGetElement(
            array,
            [row, column].as_ptr(),
            (&mut value as *mut VARIANT).cast(),
        )
    }
    .map_err(status)?;
    Ok(value)
}

pub(crate) struct ArrayOwner(pub *mut SAFEARRAY);
impl ArrayOwner {
    pub fn new(array: *mut SAFEARRAY) -> Result<Self, AutoError> {
        if array.is_null() {
            Err(AutoError::Status(E_OUTOFMEMORY.0))
        } else {
            Ok(Self(array))
        }
    }
    pub fn take(mut self) -> *mut SAFEARRAY {
        std::mem::replace(&mut self.0, std::ptr::null_mut())
    }
}
impl Drop for ArrayOwner {
    fn drop(&mut self) {
        if !self.0.is_null() {
            let _ = unsafe { SafeArrayDestroy(self.0) };
        }
    }
}

pub(crate) unsafe fn bstr_text(raw: usize) -> AutoValue {
    let text = ManuallyDrop::new(unsafe { BSTR::from_raw(raw as *const u16) });
    AutoValue::Text(ExcelText::from_utf16_code_units(text.to_vec()))
}

pub(crate) fn number(value: &AutoValue) -> Result<f64, AutoError> {
    match value {
        AutoValue::I2(v) => Ok(f64::from(*v)),
        AutoValue::I4(v) => Ok(f64::from(*v)),
        AutoValue::R4(v) => Ok(f64::from(*v)),
        AutoValue::R8(v) | AutoValue::Date(v) => Ok(*v),
        AutoValue::Bool(v) => Ok(if *v { -1.0 } else { 0.0 }),
        AutoValue::Empty => Ok(0.0),
        AutoValue::Text(v) => v
            .to_string_lossy()
            .parse()
            .map_err(|_| AutoError::Status(DISP_E_TYPEMISMATCH.0)),
        _ => mismatch(),
    }
}

// Rust APIs still expose CalcValue. This conversion is shared by ApplicationHost and RTD.
pub(crate) fn to_calc(value: &AutoValue) -> Result<calc_value::CalcValue, AutoError> {
    use calc_value::{CalcValue, WorksheetErrorCode};
    Ok(match value {
        AutoValue::Empty | AutoValue::Null => CalcValue::empty(),
        AutoValue::Missing => CalcValue::missing(),
        AutoValue::Text(v) => CalcValue::text(v.clone()),
        AutoValue::Bool(v) => CalcValue::logical(*v),
        AutoValue::Error(v) => CalcValue::error(match *v {
            2000 => WorksheetErrorCode::Null,
            2007 => WorksheetErrorCode::Div0,
            2015 => WorksheetErrorCode::Value,
            2023 => WorksheetErrorCode::Ref,
            2029 => WorksheetErrorCode::Name,
            2036 => WorksheetErrorCode::Num,
            2042 => WorksheetErrorCode::NA,
            2043 => WorksheetErrorCode::GettingData,
            _ => return mismatch(),
        }),
        AutoValue::Array(array) => {
            let cells = array
                .values
                .iter()
                .map(to_calc)
                .collect::<Result<Vec<_>, _>>()?;
            CalcValue::array(
                calc_value::CalcArray::new(
                    calc_value::ArrayShape {
                        rows: array.rows as usize,
                        cols: array.columns as usize,
                    },
                    cells,
                )
                .ok_or(AutoError::Status(E_INVALIDARG.0))?,
            )
        }
        _ => {
            let n = number(value)?;
            if !n.is_finite() {
                return Err(AutoError::Status(E_INVALIDARG.0));
            }
            CalcValue::number(n)
        }
    })
}

pub(crate) fn from_calc(
    value: &calc_value::CalcValue,
    integer: bool,
) -> Result<AutoValue, AutoError> {
    use calc_value::CoreRef;
    Ok(match value.core_ref() {
        CoreRef::Empty => AutoValue::Empty,
        CoreRef::Missing => AutoValue::Missing,
        CoreRef::Number(n) if integer => AutoValue::I4(n as i32),
        CoreRef::Number(n) => AutoValue::R8(n),
        CoreRef::Logical(b) => AutoValue::Bool(b),
        CoreRef::Text(s) => AutoValue::Text(s.clone()),
        CoreRef::Error(error) => AutoValue::Error(
            2000 + calc_xll_host::error_number(error).ok_or_else(|| {
                AutoError::not_modelled("Automation worksheet error", "dc-1nbp.5")
            })?,
        ),
        CoreRef::Array(array) => AutoValue::Array(Box::new(AutoArray {
            rows: array.shape().rows as u32,
            columns: array.shape().cols as u32,
            lower: (1, 1),
            values: array
                .iter_row_major()
                .map(|v| from_calc(v, false))
                .collect::<Result<Vec<_>, _>>()?,
        })),
        _ => {
            return Err(AutoError::not_modelled(
                "Automation value kind",
                "dc-1nbp.5",
            ));
        }
    })
}
