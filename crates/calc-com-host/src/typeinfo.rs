//! Private, in-memory Automation type information generated from the one descriptor.
//! No file is saved and no library is registered. The cache belongs to the caller's STA.
use crate::{runtime::guid, values::status};
use calc_object_model::{
    AutoError, Descriptor, InterfaceId, InvokeKind, ParamFlag,
    descriptor::{TypeKind, TypeRef},
};
use std::{cell::RefCell, collections::HashMap, ptr};
use windows::{
    Win32::System::{Com::*, Ole::*, Variant::*},
    core::{Interface, PCWSTR},
};

struct Types {
    _library: ICreateTypeLib2,
    interfaces: HashMap<InterfaceId, ITypeInfo>,
}
thread_local! { static TYPES: RefCell<Option<Result<Types, AutoError>>> = const { RefCell::new(None) }; }
pub(crate) fn get(id: InterfaceId) -> Result<ITypeInfo, AutoError> {
    TYPES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.is_none() {
            *cache = Some(build());
        }
        match cache.as_ref().unwrap() {
            Ok(types) => types.interfaces.get(&id).cloned().ok_or(AutoError::Status(
                windows::Win32::Foundation::DISP_E_BADINDEX.0,
            )),
            Err(error) => Err(error.clone()),
        }
    })
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn build() -> Result<Types, AutoError> {
    unsafe {
        let library =
            CreateTypeLib2(SYS_WIN64, windows::core::w!("DnaCalcObjects.tlb")).map_err(status)?;
        library
            .SetName(windows::core::w!("DnaCalcObjects"))
            .map_err(status)?;
        let descriptor = Descriptor::get();
        let mut creates = HashMap::new();
        let mut interfaces = HashMap::new();
        for (id, ty) in descriptor
            .types()
            .filter(|(_, ty)| ty.kind != TypeKind::Coclass)
        {
            let name = wide(&ty.name);
            let create = library
                .CreateTypeInfo(PCWSTR(name.as_ptr()), TKIND_DISPATCH)
                .map_err(status)?;
            create.SetGuid(&guid(id)).map_err(status)?;
            create
                .SetTypeFlags(TYPEFLAG_FDISPATCHABLE.0 as u32)
                .map_err(status)?;
            interfaces.insert(id, create.cast::<ITypeInfo>().map_err(status)?);
            creates.insert(id, create);
        }
        for (id, create) in &creates {
            for (index, member) in descriptor.members(*id).into_iter().enumerate() {
                add_member(create, *id, index as u32, member, &interfaces)?;
            }
            if let Err(why) = create.LayOut() {
                let name = &descriptor.ty(*id).name;
                eprintln!("type information for {name} did not lay out: {why}");
                // Diagnose: lay out each DISPID's functions alone in a scratch library.
                let mut groups: std::collections::BTreeMap<i32, Vec<&calc_object_model::Member>> =
                    Default::default();
                for member in descriptor.members(*id) {
                    groups.entry(member.dispid).or_default().push(member);
                }
                for (dispid, members) in groups {
                    let scratch = CreateTypeLib2(SYS_WIN64, windows::core::w!("Scratch.tlb")).map_err(status)?;
                    let probe = scratch
                        .CreateTypeInfo(windows::core::w!("Probe"), TKIND_DISPATCH)
                        .map_err(status)?;
                    for (index, member) in members.iter().enumerate() {
                        add_member(&probe, *id, index as u32, member, &interfaces)?;
                    }
                    if let Err(why) = probe.LayOut() {
                        let kinds: Vec<_> = members.iter().map(|m| format!("{}:{:?}/{}", m.name, m.invoke, m.params.len())).collect();
                        eprintln!("  dispid {dispid:#x} {kinds:?}: {why}");
                    }
                }
                return Err(status(why));
            }
        }
        Ok(Types {
            _library: library,
            interfaces,
        })
    }
}

/// Add one member's FUNCDESC and names (the dispatch view of the descriptor member).
unsafe fn add_member(
    create: &ICreateTypeInfo,
    owner: InterfaceId,
    index: u32,
    member: &calc_object_model::Member,
    interfaces: &HashMap<InterfaceId, ITypeInfo>,
) -> Result<(), AutoError> {
    unsafe {
        let mut arena = Vec::new();
        let mut defaults = Vec::new();
        let mut params = Vec::new();
        // IDispatch exposes the dispatch view: lcid and retval are removed from the
        // parameter list and retval becomes the return type (the dual ABI remains
        // in the descriptor and generated vtable). This is MIDL's dual conversion.
        for param in member.arguments() {
            let mut flags = 0;
            for (flag, value) in [
                (ParamFlag::In, PARAMFLAG_FIN),
                (ParamFlag::Out, PARAMFLAG_FOUT),
                (ParamFlag::Lcid, PARAMFLAG_FLCID),
                (ParamFlag::Retval, PARAMFLAG_FRETVAL),
                (ParamFlag::Optional, PARAMFLAG_FOPT),
                (ParamFlag::Hasdefault, PARAMFLAG_FHASDEFAULT),
            ] {
                if param.has(flag) {
                    flags |= value.0;
                }
            }
            let mut default = ptr::null_mut();
            if let Some(value) = &param.default {
                let value = if let Some(n) = value.as_i64() {
                    VARIANT::from(n as i32)
                } else if let Some(v) = value.as_bool() {
                    VARIANT::from(v)
                } else if let Some(v) = value.as_f64() {
                    VARIANT::from(v)
                } else if let Some(v) = value.as_str() {
                    VARIANT::from(v)
                } else {
                    VARIANT::default()
                };
                let mut held = Box::new(PARAMDESCEX {
                    cBytes: size_of::<PARAMDESCEX>() as u32,
                    varDefaultValue: value,
                });
                default = &mut *held;
                defaults.push(held);
            }
            params.push(ELEMDESC {
                tdesc: describe(&param.type_ref(), create, interfaces, &mut arena)?,
                Anonymous: ELEMDESC_0 {
                    paramdesc: PARAMDESC {
                        pparamdescex: default,
                        wParamFlags: PARAMFLAGS(flags),
                    },
                },
            });
        }
        let function = FUNCDESC {
            memid: member.dispid,
            lprgelemdescParam: params.as_mut_ptr(),
            funckind: FUNC_DISPATCH,
            invkind: match member.invoke {
                InvokeKind::Get => INVOKE_PROPERTYGET,
                InvokeKind::Put => INVOKE_PROPERTYPUT,
                InvokeKind::Putref => INVOKE_PROPERTYPUTREF,
                InvokeKind::Method => INVOKE_FUNC,
            },
            callconv: CC_STDCALL,
            cParams: params.len() as i16,
            cParamsOpt: if member.vararg {
                -1
            } else {
                member.optional_count as i16
            },
            oVft: 0,
            elemdescFunc: ELEMDESC {
                tdesc: describe(&dispatch_return(member), create, interfaces, &mut arena)?,
                ..Default::default()
            },
            wFuncFlags: FUNCFLAGS(
                if member.hidden { FUNCFLAG_FHIDDEN.0 } else { 0 }
                    | if member.restricted {
                        FUNCFLAG_FRESTRICTED.0
                    } else {
                        0
                    },
            ),
            ..Default::default()
        };
        create
            .AddFuncDesc(index, &function)
            .map_err(status)?;
        // Excel's library leaves some property-put arguments unnamed; LayOut requires a put's
        // arguments to carry its get's names, so a missing name is taken from that get.
        let getter = Descriptor::get()
            .member_by_dispid(owner, member.dispid, InvokeKind::Get)
            .filter(|_| matches!(member.invoke, InvokeKind::Put | InvokeKind::Putref));
        let strings: Vec<_> = std::iter::once(member.name.clone())
            .chain(member.arguments().enumerate().map(|(i, p)| {
                p.name
                    .clone()
                    .or_else(|| getter.and_then(|g| g.arguments().nth(i)).and_then(|g| g.name.clone()))
                    .unwrap_or_else(|| "value".into())
            }))
            .map(|name| wide(&name))
            .collect();
        let names: Vec<_> = strings.iter().map(|s| PCWSTR(s.as_ptr())).collect();
        // Property-put's last parameter is the assigned value and has no published name.
        let named = if matches!(member.invoke, InvokeKind::Put | InvokeKind::Putref) {
            names.len().saturating_sub(1)
        } else {
            names.len()
        };
        create
            .SetFuncAndParamNames(index, &names[..named])
            .map_err(status)?;
            Ok(())
    }
}

fn dispatch_return(member: &calc_object_model::Member) -> TypeRef {
    if let Some(retval) = member.retval() {
        match retval.type_ref().underlying() {
            TypeRef::Pointer(inner) => *inner,
            other => other,
        }
    } else if member.ret == "HRESULT" {
        TypeRef::Base("VOID".into())
    } else {
        TypeRef::parse(&member.ret)
    }
}

fn describe(
    ty: &TypeRef,
    create: &ICreateTypeInfo,
    interfaces: &HashMap<InterfaceId, ITypeInfo>,
    arena: &mut Vec<Box<TYPEDESC>>,
) -> Result<TYPEDESC, AutoError> {
    let ty = ty.underlying();
    let array = matches!(ty, TypeRef::SafeArray(_));
    let vt = match ty {
        TypeRef::Base(base) => match base.as_str() {
            "VARIANT" => VT_VARIANT,
            "BSTR" => VT_BSTR,
            "BOOL" => VT_BOOL,
            "I1" => VT_I1,
            "I2" => VT_I2,
            "I4" => VT_I4,
            "INT" => VT_INT,
            "I8" => VT_I8,
            "UI1" => VT_UI1,
            "UI2" => VT_UI2,
            "UI4" => VT_UI4,
            "UI8" => VT_UI8,
            "UINT" => VT_UINT,
            "R4" => VT_R4,
            "R8" => VT_R8,
            "DATE" => VT_DATE,
            "CY" => VT_CY,
            "ERROR" => VT_ERROR,
            "HRESULT" => VT_HRESULT,
            "VOID" => VT_VOID,
            "DECIMAL" => VT_DECIMAL,
            _ => VT_VARIANT,
        },
        TypeRef::Pointer(inner) | TypeRef::SafeArray(inner) => {
            let mut child = Box::new(describe(&inner, create, interfaces, arena)?);
            let pointer = &mut *child as *mut TYPEDESC;
            arena.push(child);
            return Ok(TYPEDESC {
                vt: if array { VT_SAFEARRAY } else { VT_PTR },
                Anonymous: TYPEDESC_0 { lptdesc: pointer },
            });
        }
        TypeRef::Named(name) => {
            if let Some(info) = Descriptor::get()
                .id(&name)
                .and_then(|id| interfaces.get(&Descriptor::get().default_interface(id)))
            {
                let mut href = 0u32;
                unsafe { create.AddRefTypeInfo(info, &mut href) }.map_err(status)?;
                return Ok(TYPEDESC {
                    vt: VT_USERDEFINED,
                    Anonymous: TYPEDESC_0 { hreftype: href },
                });
            }
            if name == "IUnknown" {
                VT_UNKNOWN
            } else {
                VT_DISPATCH
            }
        }
        _ => VT_VARIANT,
    };
    Ok(TYPEDESC {
        vt,
        ..Default::default()
    })
}
