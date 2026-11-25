//! One VBA projection onto `calc_object_model::ObjectModel` (dc-piw0.5,
//! docs/architecture/OBJECT_MODEL.md section 8).
//!
//! Every Excel and Office object exposed to VBA (whether in DNA Calc over
//! `WorkbookAutomation`, or in OneCalc / standalone hosts over `InvocationModel`)
//! is a service-backed `calcvba_host_api::HostObject` carrying an `ObjectRef` and
//! an `Rc<dyn ObjectModel>`. This module translates between CalcVBA's `Variant` /
//! `EarlyMemberCall` / `DynamicMemberCall` and `calc_object_model::Call` /
//! `AutoValue` / `AutoError`, and never implements Excel member semantics itself.
use crate::descriptor;
use calc_object_model::{
    AutoArray, AutoError, AutoValue, Call, Descriptor, InterfaceId, InvokeKind, Member,
    ObjectModel, ObjectRef, ParamFlag,
};
use calc_func::value::ExcelText;
use calcvba_com::{DynamicCallArg, DynamicCallKind, DynamicMemberSelector, TypeLibMemberInvokeKind};
use calcvba_host_api::{
    self as api,
    object::{
        DynamicArgument, DynamicMemberCall, DynamicMemberKind, EarlyMemberCall, EventCallback,
        EventDescriptor, EventParameter, EventSubscription, HostEnumerator, HostObject,
        HostObjectServices, InterfaceId as VbaIid,
    },
};
use calcvba_runtime::{
    ObjectRef as VbaObjectRef, ObjectRefExt, VarType, Variant,
    safe_array::{SafeArray, SafeArrayBound},
};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
    rc::{Rc, Weak},
    sync::atomic::{AtomicU64, Ordering},
};

const DISP_E_PARAMNOTFOUND: i32 = 0x8002_0004u32 as i32;
const DISP_E_TYPEMISMATCH: i32 = 0x8002_0005u32 as i32;
const DISP_E_UNKNOWNNAME: i32 = 0x8002_0006u32 as i32;
const DISP_E_OVERFLOW: i32 = 0x8002_000Au32 as i32;
const DISP_E_BADINDEX: i32 = 0x8002_000Bu32 as i32;
const DISP_E_BADPARAMCOUNT: i32 = 0x8002_000Eu32 as i32;

// Per-thread registry of active session `ObjectModel`s, keyed by an opaque `u64`
// id held by `SpreadsheetHost` (which must be `Send + Sync` for `HostServices`).
thread_local! {
    static SESSION_MODELS: RefCell<HashMap<u64, Rc<dyn ObjectModel>>> = RefCell::new(HashMap::new());
}

static NEXT_MODEL_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn register_model(model: Rc<dyn ObjectModel>) -> u64 {
    let id = NEXT_MODEL_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .expect("VBA object-model id exhausted");
    SESSION_MODELS.with(|map| {
        map.borrow_mut().insert(id, model);
    });
    id
}

pub(crate) fn unregister_model(id: u64) {
    if id != 0 {
        SESSION_MODELS.with(|map| {
            map.borrow_mut().remove(&id);
        });
    }
}

pub(crate) fn get_model(id: u64) -> Option<Rc<dyn ObjectModel>> {
    if id == 0 {
        return None;
    }
    SESSION_MODELS.with(|map| map.borrow().get(&id).cloned())
}

/// Active event subscriptions for a document object (`Worksheet` or `Workbook`).
pub(crate) struct EventHub {
    pub(crate) interface: InterfaceId,
    next: Cell<u64>,
    callbacks: RefCell<BTreeMap<u64, (EventDescriptor, EventCallback)>>,
}

impl EventHub {
    pub(crate) fn new(interface: InterfaceId) -> Rc<Self> {
        Rc::new(Self {
            interface,
            next: Cell::new(1),
            callbacks: RefCell::default(),
        })
    }

    pub(crate) fn subscription_count(&self) -> usize {
        self.callbacks.borrow().len()
    }

    pub(crate) fn matching_ids(&self, name: &str) -> Vec<u64> {
        self.callbacks
            .borrow()
            .iter()
            .filter_map(|(id, (descriptor, _))| {
                descriptor.name.eq_ignore_ascii_case(name).then_some(*id)
            })
            .collect()
    }

    pub(crate) fn callback(&self, id: u64) -> Option<EventCallback> {
        self.callbacks.borrow().get(&id).map(|(_, cb)| cb.clone())
    }
}

struct Subscription {
    hub: Weak<EventHub>,
    id: u64,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(hub) = self.hub.upgrade() {
            hub.callbacks.borrow_mut().remove(&self.id);
        }
    }
}

/// Shared state behind one VBA `HostObject` wrapper. Dropping the last `Rc` notifies
/// `ObjectModel::released` so tracked `StoredReference` leases are returned promptly.
pub(crate) struct ModelObject {
    self_weak: Weak<Self>,
    pub(crate) model: Rc<dyn ObjectModel>,
    pub(crate) object: ObjectRef,
    /// When `Some`, this object is an event `Target` bound to a specific event lease.
    pub(crate) lease: Option<u64>,
    pub(crate) hub: Option<Rc<EventHub>>,
}

struct ModelPayload(Rc<ModelObject>);

impl Drop for ModelObject {
    fn drop(&mut self) {
        self.model.released(&self.object);
    }
}

/// Wrap an `ObjectRef` as a service-backed `HostObject` carrying the real descriptor IID.
pub(crate) fn wrap_object(
    model: Rc<dyn ObjectModel>,
    object: ObjectRef,
    lease: Option<u64>,
    hub: Option<Rc<EventHub>>,
) -> HostObject {
    let iid = descriptor::interface_iid_bytes(object.interface);
    let inner = Rc::new_cyclic(|weak| ModelObject {
        self_weak: weak.clone(),
        model,
        object,
        lease,
        hub,
    });
    make_view(inner, iid)
}

fn make_view(inner: Rc<ModelObject>, iid: VbaIid) -> HostObject {
    let identity = Rc::as_ptr(&inner).cast();
    let payload = Rc::new(ModelPayload(inner.clone()));
    // SAFETY: `inner` is retained by both the payload and services for the full lifetime of the
    // `HostObject` and every view returned from `resolve_interface`, keeping `identity` canonical
    // and live without fabricating any native COM vtable.
    unsafe { HostObject::new_with_services(iid, vec![], payload, identity, inner) }
}

/// Extract the underlying `ModelObject` from a `VbaObjectRef` (including document module
/// singletons such as `Me` or `ThisWorkbook`, whose `HOST_DOCUMENT_FIELD` holds the `HostObject`).
pub(crate) fn unwrap_model_object(reference: &VbaObjectRef) -> Option<Rc<ModelObject>> {
    let host = HostObject::from_object_ref(reference).or_else(|| {
        let native = reference.project_field_get(calcvba_bundle::HOST_DOCUMENT_FIELD)?;
        HostObject::from_object_ref(&native.as_object_ref()?)
    })?;
    host.payload()?
        .downcast_ref::<ModelPayload>()
        .map(|p| p.0.clone())
}

pub(crate) fn is_model_object(reference: &VbaObjectRef) -> bool {
    unwrap_model_object(reference).is_some()
}

pub(crate) fn object_type_name(reference: &VbaObjectRef) -> Option<String> {
    let inner = unwrap_model_object(reference)?;
    let name = &Descriptor::get().ty(inner.object.interface).name;
    Some(name.trim_start_matches('_').to_owned())
}

fn interface_supports_iid(object_iface: InterfaceId, requested: VbaIid) -> bool {
    if requested == api::object::IDISPATCH {
        return true;
    }
    let d = Descriptor::get();
    if requested == descriptor::interface_iid_bytes(object_iface) {
        return true;
    }
    let iface_name = &d.ty(object_iface).name;
    if iface_name == "_Application" && requested == descriptor::global_iid_bytes() {
        return true;
    }
    let guid = descriptor::bytes_to_guid(requested);
    if let Some(req_id) = d.by_guid(&guid) {
        if d.default_interface(req_id) == object_iface {
            return true;
        }
        // Walk base interface chain (e.g. TextFrame2 -> _IMsoDispObj).
        let mut current = Some(object_iface);
        while let Some(at) = current {
            if at == req_id {
                return true;
            }
            current = d
                .ty(at)
                .base
                .as_deref()
                .filter(|b| !b.eq_ignore_ascii_case("IDispatch") && !b.eq_ignore_ascii_case("IUnknown"))
                .and_then(|b| d.id(b));
        }
    }
    false
}

fn check_lease(lease: Option<u64>) -> Result<(), api::TypedFault> {
    let Some(expected) = lease else {
        return Ok(());
    };
    if !crate::events::lease_is_active(expected) {
        let reason = "an event Target outside its admitted event view";
        crate::runtime::spreadsheet::record_capability_refusal(reason);
        crate::events::record_event_refusal(reason.to_owned());
        return Err(fault(91, reason));
    }
    Ok(())
}

fn fault(number: i32, description: impl Into<String>) -> api::TypedFault {
    api::TypedFault {
        number,
        description: description.into(),
        source: None,
        help_file: None,
        help_context: None,
    }
}

fn map_auto_error(model: &dyn ObjectModel, error: AutoError) -> api::TypedFault {
    match &error {
        AutoError::NotModelled { owner, .. } => {
            model.record_refusal(&error);
            let description = error.description();
            crate::runtime::spreadsheet::record_not_modelled(owner, description.clone());
            crate::events::record_event_refusal(description.clone());
            fault(1004, description)
        }
        AutoError::Excel { number, description } => fault(*number, description.clone()),
        AutoError::PolicyDenied { number, capability, subject } => {
            crate::events::record_object_denial(capability, subject.clone());
            fault(*number, error.description())
        }
        AutoError::Status(hr) => {
            let number = match *hr {
                DISP_E_TYPEMISMATCH => 13,
                DISP_E_BADPARAMCOUNT => 450,
                DISP_E_BADINDEX => 9,
                DISP_E_UNKNOWNNAME => 438,
                DISP_E_OVERFLOW => 6,
                DISP_E_PARAMNOTFOUND => 448,
                other if (other as u32 & 0xFFFF_0000) == 0x800A_0000 => other & 0xFFFF,
                _ => 1004,
            };
            fault(number, error.description())
        }
        AutoError::Disconnected => fault(424, error.description()),
    }
}

fn to_invoke_kind(kind: DynamicMemberKind) -> InvokeKind {
    match kind {
        DynamicMemberKind::Get => InvokeKind::Get,
        DynamicMemberKind::Let => InvokeKind::Put,
        DynamicMemberKind::Set => InvokeKind::Putref,
        DynamicMemberKind::Method => InvokeKind::Method,
    }
}

fn d_is_application(iface: InterfaceId) -> bool {
    let d = Descriptor::get();
    d.id("Application").map(|app| d.default_interface(app)) == Some(iface)
}

fn lookup_member_by_dispid(
    iface: InterfaceId,
    dispid: i32,
    kind: InvokeKind,
) -> Option<&'static Member> {
    let d = Descriptor::get();
    d.member_by_dispid(iface, dispid, kind).or_else(|| match kind {
        InvokeKind::Get => d.member_by_dispid(iface, dispid, InvokeKind::Method),
        InvokeKind::Method => d.member_by_dispid(iface, dispid, InvokeKind::Get),
        InvokeKind::Putref => d.member_by_dispid(iface, dispid, InvokeKind::Put),
        InvokeKind::Put => d.member_by_dispid(iface, dispid, InvokeKind::Putref),
    })
}

fn lookup_member_by_name(
    iface: InterfaceId,
    name: Option<&str>,
    kind: InvokeKind,
) -> Option<&'static Member> {
    match name {
        None => lookup_member_by_dispid(iface, 0, kind),
        Some(name) => {
            let d = Descriptor::get();
            d.member(iface, name, kind).or_else(|| match kind {
                InvokeKind::Get => d.member(iface, name, InvokeKind::Method),
                InvokeKind::Method => d.member(iface, name, InvokeKind::Get),
                InvokeKind::Putref => d.member(iface, name, InvokeKind::Put),
                InvokeKind::Put => d.member(iface, name, InvokeKind::Putref),
            })
        }
    }
}

fn bind_arguments(
    member: &Member,
    values: &[Variant],
    names: &[Option<&str>],
) -> Result<Vec<AutoValue>, AutoError> {
    let declared: Vec<_> = member.arguments().collect();
    let is_put = matches!(member.invoke, InvokeKind::Put | InvokeKind::Putref);
    let has_named = names.iter().any(Option::is_some);

    if !has_named {
        if values.len() > declared.len() {
            return Err(AutoError::Status(DISP_E_BADPARAMCOUNT));
        }
        return values.iter().map(variant_to_auto).collect();
    }

    let mut slots = vec![AutoValue::Missing; declared.len()];
    let mut supplied = vec![false; declared.len()];
    let value_count = if is_put && !declared.is_empty() {
        let rhs = values.last().ok_or(AutoError::Status(DISP_E_BADPARAMCOUNT))?;
        let last = declared.len() - 1;
        slots[last] = variant_to_auto(rhs)?;
        supplied[last] = true;
        values.len() - 1
    } else {
        values.len()
    };

    for pos in 0..value_count {
        let slot = match names.get(pos).copied().flatten() {
            Some(param_name) => declared
                .iter()
                .position(|p| {
                    p.name
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case(param_name))
                })
                .ok_or_else(|| AutoError::Status(DISP_E_PARAMNOTFOUND))?,
            None => pos,
        };
        if slot >= declared.len() || supplied[slot] {
            return Err(AutoError::Status(DISP_E_PARAMNOTFOUND));
        }
        slots[slot] = variant_to_auto(&values[pos])?;
        supplied[slot] = true;
    }

    if declared.iter().zip(&supplied).any(|(param, &present)| {
        !present && !param.has(ParamFlag::Optional) && !param.has(ParamFlag::Hasdefault)
    }) {
        return Err(AutoError::Status(DISP_E_BADPARAMCOUNT));
    }

    if !is_put {
        while slots.last().is_some_and(AutoValue::is_missing) {
            slots.pop();
        }
    }
    Ok(slots)
}

pub(crate) fn variant_to_auto(value: &Variant) -> Result<AutoValue, AutoError> {
    Ok(match value.vtype() {
        VarType::Empty => AutoValue::Empty,
        VarType::Null => AutoValue::Null,
        VarType::Boolean => AutoValue::Bool(value.as_bool().expect("Boolean carrier")),
        VarType::Byte => AutoValue::I2(i16::from(value.as_u8().expect("Byte carrier"))),
        VarType::Integer => AutoValue::I2(value.as_i16().expect("Integer carrier")),
        VarType::Long => AutoValue::I4(value.as_i32().expect("Long carrier")),
        VarType::LongLong => {
            let n = value.as_i64().expect("LongLong carrier");
            match i32::try_from(n) {
                Ok(v) => AutoValue::I4(v),
                Err(_) => AutoValue::R8(n as f64),
            }
        }
        VarType::Single => AutoValue::R4(value.as_f32().expect("Single carrier")),
        VarType::Double => AutoValue::R8(value.as_f64().expect("Double carrier")),
        VarType::Currency => AutoValue::Currency(value.as_currency_scaled_i64().expect("Currency carrier")),
        VarType::Date => AutoValue::Date(value.as_date_f64().expect("Date carrier")),
        VarType::Decimal => {
            let number = calcvba_runtime::coerce::coerce_to(value, VarType::Double)
                .map_err(|_| AutoError::Status(DISP_E_TYPEMISMATCH))?;
            AutoValue::R8(number.as_f64().expect("Double coercion"))
        }
        VarType::Error => {
            let code = value.as_error_code().expect("Error carrier");
            if code == DISP_E_PARAMNOTFOUND {
                AutoValue::Missing
            } else if (code as u32 & 0xFFFF_0000) == 0x800A_0000 {
                AutoValue::Error(code & 0xFFFF)
            } else {
                AutoValue::Error(code)
            }
        }
        VarType::String => {
            let text = value.as_bstr().expect("String carrier");
            if !text.byte_len().is_multiple_of(2) {
                return Err(AutoError::not_modelled(
                    "odd BSTR byte length in Automation",
                    "dc-piw0.5",
                ));
            }
            AutoValue::Text(ExcelText::from_utf16_code_units(text.to_utf16_units()))
        }
        VarType::Object => match value.as_object_ref() {
            None => AutoValue::Empty,
            Some(reference) => {
                let Some(inner) = unwrap_model_object(&reference) else {
                    return Err(AutoError::Status(DISP_E_TYPEMISMATCH));
                };
                if let Some(expected) = inner.lease
                    && !crate::events::lease_is_active(expected)
                {
                    return Err(AutoError::not_modelled(
                        "an event Target outside its admitted event view",
                        "dc-hsa5.12",
                    ));
                }
                AutoValue::Object(inner.object.clone())
            }
        },
        VarType::ArrayVariant => {
            let array = value
                .as_safearray()
                .ok_or(AutoError::Status(DISP_E_TYPEMISMATCH))?;
            let bounds = array
                .bounds()
                .ok_or(AutoError::Status(DISP_E_TYPEMISMATCH))?;
            let (rows, columns, lower) = match bounds.as_slice() {
                [cols] => (1u32, cols.count, (1, cols.lower)),
                [rows, cols] => (rows.count, cols.count, (rows.lower, cols.lower)),
                _ => {
                    return Err(AutoError::not_modelled(
                        "Automation array with rank > 2",
                        "dc-1nbp.6",
                    ));
                }
            };
            if rows == 0 || columns == 0 {
                return Err(AutoError::Status(DISP_E_TYPEMISMATCH));
            }
            // Convert VBA column-major (first dimension fastest) to AutoArray row-major.
            let mut values = Vec::with_capacity((rows as usize) * (columns as usize));
            for r in 0..rows as usize {
                for c in 0..columns as usize {
                    let elem = array
                        .variant_element(r + c * rows as usize)
                        .map_err(|_| AutoError::Status(DISP_E_TYPEMISMATCH))?;
                    values.push(variant_to_auto(&elem)?);
                }
            }
            AutoValue::Array(Box::new(AutoArray {
                rows,
                columns,
                lower,
                values,
            }))
        }
        _ => {
            return Err(AutoError::not_modelled(
                "unsupported VBA Variant carrier in Automation",
                "dc-piw0.5",
            ));
        }
    })
}

pub(crate) fn auto_to_variant(
    value: AutoValue,
    model: &Rc<dyn ObjectModel>,
) -> Result<Variant, AutoError> {
    Ok(match value {
        AutoValue::Empty => Variant::empty(),
        AutoValue::Null => Variant::null(),
        AutoValue::Missing => Variant::from_error_code(DISP_E_PARAMNOTFOUND),
        AutoValue::Bool(v) => Variant::from_bool(v),
        AutoValue::I2(v) => Variant::from_i16(v),
        AutoValue::I4(v) => Variant::from_i32(v),
        AutoValue::R4(v) => Variant::from_f32(v),
        AutoValue::R8(v) => Variant::from_f64(v),
        AutoValue::Currency(v) => Variant::from_currency_scaled_i64(v),
        AutoValue::Date(v) => Variant::from_date_f64(v),
        AutoValue::Error(code) => Variant::from_error_code(code),
        AutoValue::Text(t) => {
            let text = String::from_utf16(t.utf16_code_units()).map_err(|_| {
                AutoError::not_modelled("invalid UTF-16 text in Automation result", "dc-hsa5.12")
            })?;
            Variant::from_string(text)
        }
        AutoValue::Object(obj) => {
            Variant::from_object_ref(wrap_object(model.clone(), obj, None, None).object_ref())
        }
        AutoValue::Foreign(_) => {
            return Err(AutoError::not_modelled(
                "foreign Automation object in VBA",
                "dc-piw0.5",
            ));
        }
        AutoValue::Array(arr) => {
            let (rows, cols) = (arr.rows as usize, arr.columns as usize);
            if rows == 0 || cols == 0 || rows.checked_mul(cols) != Some(arr.values.len()) {
                return Err(AutoError::not_modelled(
                    "malformed Automation array result",
                    "dc-piw0.5",
                ));
            }
            // Convert AutoArray row-major to VBA column-major (first dimension fastest).
            let mut cells = Vec::with_capacity(arr.values.len());
            for c in 0..cols {
                for r in 0..rows {
                    cells.push(auto_to_variant(arr.values[r * cols + c].clone(), model)?);
                }
            }
            let safe = SafeArray::from_shape_and_variants(
                vec![
                    SafeArrayBound {
                        count: arr.rows,
                        lower: arr.lower.0,
                    },
                    SafeArrayBound {
                        count: arr.columns,
                        lower: arr.lower.1,
                    },
                ],
                cells,
            )
            .map_err(|_| AutoError::not_modelled("SafeArray allocation", "dc-piw0.5"))?;
            Variant::from_safearray(safe)
        }
    })
}

impl ModelObject {
    fn invoke_member(
        &self,
        member: &Member,
        values: &[Variant],
        names: &[Option<&str>],
    ) -> Result<Variant, api::TypedFault> {
        check_lease(self.lease)?;
        let args = bind_arguments(member, values, names)
            .map_err(|err| map_auto_error(&*self.model, err))?;
        let call = Call {
            interface: self.object.interface,
            member,
            args,
            lcid: 0,
        };
        let result = self
            .model
            .invoke(&self.object, &call)
            .map_err(|err| map_auto_error(&*self.model, err))?;
        auto_to_variant(result, &self.model).map_err(|err| map_auto_error(&*self.model, err))
    }

    pub(crate) fn invoke_early_hal(
        &self,
        member_dispid: i32,
        invoke_kind: TypeLibMemberInvokeKind,
        args: &[DynamicCallArg],
    ) -> Result<Variant, api::TypedFault> {
        let kind = match invoke_kind {
            TypeLibMemberInvokeKind::Method => InvokeKind::Method,
            TypeLibMemberInvokeKind::PropertyGet => InvokeKind::Get,
            TypeLibMemberInvokeKind::PropertyPut => InvokeKind::Put,
            TypeLibMemberInvokeKind::PropertyPutRef => InvokeKind::Putref,
        };
        let (values, names) = unpack_dynamic_args(args);
        if member_dispid >= crate::descriptor::APPLICATION_ALIAS_BASE {
            let index = (member_dispid - crate::descriptor::APPLICATION_ALIAS_BASE) as usize;
            let alias = crate::descriptor::application_alias_members(Descriptor::get())
                .get(index)
                .copied()
                .ok_or_else(|| fault(438, format!("unknown member dispid {member_dispid}")))?;
            return self.invoke_application_alias(alias, &values, &names);
        }
        let member = lookup_member_by_dispid(self.object.interface, member_dispid, kind)
            .ok_or_else(|| fault(438, format!("unknown member dispid {member_dispid}")))?;
        self.invoke_member(member, &values, &names)
    }

    /// `Application.<name>(...)` for a worksheet-function name: the call `WorksheetFunction.<name>`
    /// answers, except that a worksheet error comes back as a value (dc-711ye).
    fn invoke_application_alias(
        &self,
        member: &'static Member,
        values: &[Variant],
        names: &[Option<&str>],
    ) -> Result<Variant, api::TypedFault> {
        let d = Descriptor::get();
        let functions_member = d
            .member(self.object.interface, "WorksheetFunction", InvokeKind::Get)
            .ok_or_else(|| fault(438, "Application.WorksheetFunction missing from descriptor"))?;
        let functions = self.invoke_member(functions_member, &[], &[])?;
        let target = functions
            .as_object_ref()
            .and_then(|r| unwrap_model_object(&r))
            .ok_or_else(|| fault(438, "Application.WorksheetFunction is not an object"))?;
        calc_object_model::profile::worksheet_function::call_as_application(|| {
            target.invoke_member(member, values, names)
        })
    }

    fn invoke_late(
        &self,
        name: Option<&str>,
        kind: InvokeKind,
        values: &[Variant],
        names: &[Option<&str>],
    ) -> Result<Variant, api::TypedFault> {
        if let Some(member) = lookup_member_by_name(self.object.interface, name, kind) {
            // In OLE Automation / VBA, indexing a 0-arg object-valued property on a late-bound
            // receiver (e.g. `Worksheets("S").Range("A1:B2").Cells(2, 2)` or `wb.Worksheets("S")`)
            // evaluates the 0-arg getter and forwards the arguments to the returned object's
            // default member (DISPID 0).
            if name.is_some()
                && !values.is_empty()
                && matches!(member.invoke, InvokeKind::Get | InvokeKind::Method)
                && member.arguments().next().is_none()
            {
                let inner = self.invoke_member(member, &[], &[])?;
                if let Some(target) = inner.as_object_ref().and_then(|r| unwrap_model_object(&r)) {
                    return target.invoke_late(None, kind, values, names);
                }
            }
            return self.invoke_member(member, values, names);
        }
        // VBA forwards Let through a read-only object getter to the returned object's default
        // setter (`lo.Range = 9`). Set with no indices targets the named property itself.
        // Indexed assignments such as `ws.Cells(2, 2) = 9` pass their indices to the setter.
        if name.is_some()
            && !values.is_empty()
            && (kind == InvokeKind::Put || (kind == InvokeKind::Putref && values.len() > 1))
            && let Some(getter) =
                lookup_member_by_name(self.object.interface, name, InvokeKind::Get)
        {
            let params = getter.arguments().count();
            if params == 0 {
                let inner = self.invoke_member(getter, &[], &[])?;
                if let Some(target) = inner.as_object_ref().and_then(|r| unwrap_model_object(&r)) {
                    return target.invoke_late(None, kind, values, names);
                }
            } else if values.len() > 1 {
                // `ws.Range("A1") = 3`: the getter takes the leading arguments (its own
                // parameters), the returned object's default member takes the assigned value
                // (dc-z34rd).
                let split = params.min(values.len() - 1);
                let inner = self.invoke_member(getter, &values[..split], &names[..split])?;
                if let Some(target) = inner.as_object_ref().and_then(|r| unwrap_model_object(&r)) {
                    return target.invoke_late(None, kind, &values[split..], &names[split..]);
                }
            }
        }
        if let Some(name) = name
            && d_is_application(self.object.interface)
            && let Some(alias) = crate::descriptor::application_alias_members(Descriptor::get())
                .into_iter()
                .find(|m| m.name.eq_ignore_ascii_case(name))
        {
            return self.invoke_application_alias(alias, values, names);
        }
        Err(fault(438, format!("unknown member {name:?}")))
    }

    pub(crate) fn invoke_dynamic_hal(
        &self,
        member: &DynamicMemberSelector,
        kind_hint: Option<DynamicCallKind>,
        args: &[DynamicCallArg],
    ) -> Result<Variant, api::TypedFault> {
        let kind = match kind_hint {
            Some(DynamicCallKind::PropertyLet) => InvokeKind::Put,
            Some(DynamicCallKind::PropertySet) => InvokeKind::Putref,
            Some(DynamicCallKind::Method) => InvokeKind::Method,
            Some(DynamicCallKind::PropertyGet) | None => InvokeKind::Get,
        };
        let (values, names) = unpack_dynamic_args(args);
        match member {
            DynamicMemberSelector::DefaultMember => self.invoke_late(None, kind, &values, &names),
            DynamicMemberSelector::Name(name) => {
                self.invoke_late(Some(name.as_str()), kind, &values, &names)
            }
            DynamicMemberSelector::Token(dispid) => {
                let resolved = lookup_member_by_dispid(self.object.interface, *dispid, kind)
                    .ok_or_else(|| fault(438, format!("unknown member {member:?}")))?;
                self.invoke_member(resolved, &values, &names)
            }
            DynamicMemberSelector::TokenNamed { token, name } => {
                if let Some(resolved) = lookup_member_by_dispid(self.object.interface, *token, kind)
                {
                    self.invoke_member(resolved, &values, &names)
                } else {
                    self.invoke_late(Some(name.as_str()), kind, &values, &names)
                }
            }
        }
    }

    pub(crate) fn default_value(&self) -> Result<Option<Variant>, api::TypedFault> {
        check_lease(self.lease)?;
        let Some(member) = lookup_member_by_dispid(self.object.interface, 0, InvokeKind::Get)
        else {
            return Ok(None);
        };
        self.invoke_member(member, &[], &[]).map(Some)
    }

    /// Materialize a `Range` returned from a worksheet function through `Value2` (matching Excel
    /// without failing on unmodelled date/currency formats).
    pub(crate) fn materialize_udf_range(&self) -> Result<Option<Variant>, api::TypedFault> {
        let d = Descriptor::get();
        if d.ty(self.object.interface).name != "Range" {
            return Ok(None);
        }
        let member = d
            .member(self.object.interface, "Value2", InvokeKind::Get)
            .ok_or_else(|| fault(438, "Range.Value2 missing from descriptor"))?;
        self.invoke_member(member, &[], &[]).map(Some)
    }

    pub(crate) fn enumerate_items(&self) -> Result<Vec<Variant>, api::TypedFault> {
        check_lease(self.lease)?;
        let d = Descriptor::get();
        // For Range, iterate cells in each area in area order (matches Excel, OBJECT_MODEL.md §9).
        if d.ty(self.object.interface).name == "Range" {
            let areas_member = d
                .member(self.object.interface, "Areas", InvokeKind::Get)
                .ok_or_else(|| fault(438, "Range.Areas missing"))?;
            let AutoValue::Object(areas_ref) = self
                .model
                .invoke(
                    &self.object,
                    &Call {
                        interface: self.object.interface,
                        member: areas_member,
                        args: vec![],
                        lcid: 0,
                    },
                )
                .map_err(|err| map_auto_error(&*self.model, err))?
            else {
                return Err(fault(438, "Range.Areas did not return Areas"));
            };
            let areas_obj = Rc::new_cyclic(|weak| ModelObject {
                self_weak: weak.clone(),
                model: self.model.clone(),
                object: areas_ref,
                lease: None,
                hub: None,
            });
            let area_count = areas_obj.collection_count()?;
            let mut items = Vec::new();
            for a in 1..=area_count {
                let area_var = areas_obj.collection_item(a)?;
                let Some(area_ref) = area_var.as_object_ref().and_then(|r| unwrap_model_object(&r))
                else {
                    return Err(fault(438, "Areas.Item did not return Range"));
                };
                let cell_count = area_ref.collection_count()?;
                for c in 1..=cell_count {
                    items.push(area_ref.collection_item(c)?);
                }
            }
            return Ok(items);
        }
        let count = self.collection_count()?;
        let mut items = Vec::with_capacity(count as usize);
        for i in 1..=count {
            items.push(self.collection_item(i)?);
        }
        Ok(items)
    }

    fn collection_count(&self) -> Result<i32, api::TypedFault> {
        let d = Descriptor::get();
        let member = d
            .member(self.object.interface, "Count", InvokeKind::Get)
            .ok_or_else(|| fault(438, "object is not enumerable"))?;
        let val = self
            .model
            .invoke(
                &self.object,
                &Call {
                    interface: self.object.interface,
                    member,
                    args: vec![],
                    lcid: 0,
                },
            )
            .map_err(|err| map_auto_error(&*self.model, err))?;
        match val {
            AutoValue::I4(n) if n >= 0 => Ok(n),
            AutoValue::I2(n) if n >= 0 => Ok(i32::from(n)),
            AutoValue::R8(n) if n >= 0.0 && n <= i32::MAX as f64 => Ok(n as i32),
            _ => Err(fault(6, "Overflow")),
        }
    }

    fn collection_item(&self, index: i32) -> Result<Variant, api::TypedFault> {
        let d = Descriptor::get();
        let member = d
            .member(self.object.interface, "Item", InvokeKind::Get)
            .or_else(|| d.member(self.object.interface, "Item", InvokeKind::Method))
            .ok_or_else(|| fault(438, "collection has no Item"))?;
        let val = self
            .model
            .invoke(
                &self.object,
                &Call {
                    interface: self.object.interface,
                    member,
                    args: vec![AutoValue::I4(index)],
                    lcid: 0,
                },
            )
            .map_err(|err| map_auto_error(&*self.model, err))?;
        auto_to_variant(val, &self.model).map_err(|err| map_auto_error(&*self.model, err))
    }
}

fn unpack_dynamic_args(args: &[DynamicCallArg]) -> (Vec<Variant>, Vec<Option<&str>>) {
    let mut values = Vec::with_capacity(args.len());
    let mut names = Vec::with_capacity(args.len());
    for arg in args {
        names.push(arg.name.as_deref());
        values.push(
            arg.value
                .as_ref()
                .map(|v| v.variant().clone())
                .unwrap_or_else(|| Variant::from_error_code(DISP_E_PARAMNOTFOUND)),
        );
    }
    (values, names)
}

fn names_from_specs(specs: &[DynamicArgument]) -> Vec<Option<&str>> {
    specs.iter().map(|s| s.name.as_deref()).collect()
}

impl HostObjectServices for ModelObject {
    fn resolve_interface(&self, interface: VbaIid) -> Result<HostObject, i32> {
        if interface_supports_iid(self.object.interface, interface)
            && let Some(inner) = self.self_weak.upgrade()
        {
            Ok(make_view(inner, interface))
        } else {
            Err(438)
        }
    }

    fn invoke_early(
        &self,
        call: &EarlyMemberCall<'_>,
        arguments: &mut [Variant],
        context: &mut api::TypedContext,
    ) -> Result<Variant, api::TypedFault> {
        let kind = to_invoke_kind(call.kind);
        let member = lookup_member_by_dispid(self.object.interface, call.member_id, kind)
            .or_else(|| lookup_member_by_name(self.object.interface, Some(call.name), kind))
            .ok_or_else(|| fault(438, format!("unknown member {}", call.name)))?;
        let names = names_from_specs(call.arguments);
        api::with_native_call(context, || self.invoke_member(member, arguments, &names))
    }

    fn invoke_dynamic(
        &self,
        call: &DynamicMemberCall,
        arguments: &mut [Variant],
        context: &mut api::TypedContext,
    ) -> Result<Variant, api::TypedFault> {
        let kind = to_invoke_kind(call.kind);
        let names = names_from_specs(&call.arguments);
        api::with_native_call(context, || {
            self.invoke_late(call.name.as_deref(), kind, arguments, &names)
        })
    }

    fn enumerator(&self, context: &mut api::TypedContext) -> Result<HostEnumerator, i32> {
        let items = api::with_native_call(context, || self.enumerate_items())
            .map_err(|fault| fault.publish(context))?;
        let mut iter = items.into_iter().map(api::Value::Variant);
        Ok(HostEnumerator::new(move |_| Ok(iter.next())))
    }

    fn subscribe_event(
        &self,
        descriptor: &EventDescriptor,
        callback: EventCallback,
    ) -> Result<EventSubscription, i32> {
        let Some(hub) = &self.hub else {
            let err = AutoError::not_modelled(
                "spreadsheet WithEvents subscriptions are not modelled",
                "dc-hsa5.12",
            );
            self.model.record_refusal(&err);
            crate::runtime::spreadsheet::record_refusal_message(err.description());
            crate::events::record_event_refusal(err.description());
            return Err(438);
        };
        let d = Descriptor::get();
        let iface_name = &d.ty(hub.interface).name;
        let doc_events_iid = descriptor::interface_iid_bytes(
            d.id("DocEvents").expect("DocEvents in descriptor"),
        );
        let book_events_iid = descriptor::interface_iid_bytes(
            d.id("WorkbookEvents").expect("WorkbookEvents in descriptor"),
        );

        let host_type = match iface_name.as_str() {
            "_Worksheet" => "Worksheet",
            "_Workbook" => "Workbook",
            _ => "",
        };
        let expected_events = if host_type == "Worksheet" {
            doc_events_iid
        } else {
            book_events_iid
        };
        let valid = descriptor.interface == expected_events
            && descriptor::document_event(d, host_type, &descriptor.name).is_some_and(
                |(member, shape)| {
                    member.dispid == descriptor.dispatch_id
                        && shape.len() == descriptor.parameters.len()
                        && shape.iter().zip(&descriptor.parameters).all(|(want, got)| {
                            match (want, got) {
                                (descriptor::EventShape::Object, EventParameter::Object(_))
                                | (descriptor::EventShape::Boolean, EventParameter::Boolean)
                                | (
                                    descriptor::EventShape::BooleanByRef,
                                    EventParameter::BooleanByRef,
                                ) => true,
                                _ => false,
                            }
                        })
                },
            );
        if !valid {
            return Err(13);
        }
        let id = hub.next.get();
        hub.next.set(id.checked_add(1).ok_or(7)?);
        hub.callbacks
            .borrow_mut()
            .insert(id, (descriptor.clone(), callback));
        Ok(EventSubscription::new(Subscription {
            hub: Rc::downgrade(hub),
            id,
        }))
    }
}
