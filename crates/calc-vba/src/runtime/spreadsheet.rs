//! Spreadsheet host adapter over `calc_object_model::ObjectModel` (dc-piw0.5).
//!
//! Excel and Office type information comes from `crate::descriptor` (backed by
//! `crates/calc-object-model/descriptor/{excel,office}.json`), and member calls
//! route through `crate::projection` to the session's `ObjectModel` (`WorkbookAutomation`
//! in DNA Calc, or `InvocationModel` in Formula Explorer / standalone callers).
use super::{Error, Value, from_variant, to_variant};
use crate::{descriptor, invocation_model, projection};
use calc_object_model::{AutoError, ObjectKey, ObjectModel, ObjectRef as ModelObjectRef, geometry::RangeAxis};
use calcvba_com::{
    ComCallbackPayload, ComCallbackToken, ComMemberToken, ComObjectDescriptor,
    ComObjectTransportKind, ComSubscriptionToken, DynamicCallRequest, TypeLibCacheScope,
    TypeLibMemberInvokeKind, TypeLibMemberMetadata, TypeLibMetadataBlob, TypeLibResolveRequest,
    TypeLibResolvedIdentity,
};
use calcvba_hal::{
    HostServices,
    adapters::builder::HostBuilder,
    error::{HalError, HalResult},
    model::{CapabilityId, HalDescriptor, HalProfileId, HostPolicy},
    traits::*,
};
use calcvba_runtime::{ObjectRef, ObjectRefExt, Variant};
use std::{cell::RefCell, rc::Rc, sync::Arc};

/// Opaque DNA-owned Range identity, never sheet coordinates from a previous call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Range {
    pub id: u64,
}
#[derive(Debug)]
pub struct RangeInfo {
    pub row: u32,
    pub column: u32,
    pub rows: u32,
    pub columns: u32,
    pub cells: u64,
    pub areas: u32,
    pub address: String,
}
#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    #[error("VBA error {0}: {1}")]
    Vba(i32, String),
    #[error("{0}")]
    Refused(String),
}
impl From<String> for ContextError {
    fn from(message: String) -> Self {
        Self::Refused(message)
    }
}
impl From<&str> for ContextError {
    fn from(message: &str) -> Self {
        Self::Refused(message.into())
    }
}

/// Borrowed for a synchronous invocation only. It may read live references and invoke
/// the host's ordinary worksheet-function catalogue; neither backend implements one.
pub trait SpreadsheetContext {
    /// A reentered worksheet UDF cannot use a surrounding document event Target.
    fn event_view(&self) -> bool {
        false
    }
    /// Lend the live `UserFunctionContext` when running a worksheet UDF (`WorkbookAutomation`).
    fn with_user_context(
        &self,
        _call: &mut dyn FnMut(&mut dyn calc_func::UserFunctionContext),
    ) -> bool {
        false
    }
    /// The retained `StoredReference` backing `range` when invoked via `ContextBridge`.
    fn stored_reference(&self, _range: Range) -> Option<calc_func::StoredReference> {
        None
    }
    fn caller(&self) -> Option<Range>;
    /// The name of the sheet the person has active (`ActiveSheet.Name`), or why the host does not
    /// model one.
    fn active_sheet_name(&self) -> Result<String, ContextError> {
        Err(ContextError::Refused(
            "ActiveSheet (the host does not model the user's active sheet)".into(),
        ))
    }
    /// A range of the sheet the person has active, by A1 address (`ActiveSheet.Range("B2")`).
    fn active_range(&self, _address: &str) -> Result<Range, ContextError> {
        Err(ContextError::Refused(
            "ActiveSheet (the host does not model the user's active sheet)".into(),
        ))
    }
    /// The range the person has selected (`Selection`).
    fn selection(&self) -> Result<Range, ContextError> {
        Err(ContextError::Refused(
            "Selection (the host does not model the user's selection)".into(),
        ))
    }
    /// Record a capability refusal in the owning evaluator, independently of VBA On Error.
    fn refuse(&self, reason: &'static str) -> String {
        reason.into()
    }
    /// Record a capability refusal with its owning work-package id.
    fn refuse_by(&self, reason: &'static str, _owner: &'static str) -> String {
        self.refuse(reason)
    }
    fn metadata(&self, range: Range) -> Result<RangeInfo, ContextError>;
    fn external_address(&self, _range: Range) -> Result<String, ContextError> {
        Err(self
            .refuse("reference names without an admitted metadata view")
            .into())
    }
    fn cell(&self, range: Range, row: u32, column: u32) -> Result<Range, ContextError>;
    fn area(&self, range: Range, index: u32) -> Result<Range, ContextError>;
    fn read_range(&self, range: Range) -> Result<Value, ContextError>;
    fn read_argument_range(&self, range: Range) -> Result<Value, ContextError> {
        self.read_range(range)
    }
    fn worksheet_function(&self, name: &str, args: &[Value]) -> Result<Value, ContextError>;
    fn set_volatile(&self, value: bool) -> Result<(), ContextError>;
}

scoped_tls_hkt::scoped_thread_local!(static CONTEXT: for<'a> (
    &'a (dyn SpreadsheetContext + 'a), &'a RefCell<Option<String>>, bool
));

pub(crate) fn with_context<T>(
    context: &dyn SpreadsheetContext,
    call: impl FnOnce() -> Result<T, Error>,
) -> Result<T, Error> {
    scoped_context(context, true, call)
}

pub(crate) fn without_context<T>(call: impl FnOnce() -> Result<T, Error>) -> Result<T, Error> {
    struct Unavailable;
    impl SpreadsheetContext for Unavailable {
        fn caller(&self) -> Option<Range> {
            None
        }
        fn metadata(&self, _: Range) -> Result<RangeInfo, ContextError> {
            Err("spreadsheet context unavailable".into())
        }
        fn cell(&self, _: Range, _: u32, _: u32) -> Result<Range, ContextError> {
            Err("spreadsheet context unavailable".into())
        }
        fn area(&self, _: Range, _: u32) -> Result<Range, ContextError> {
            Err("spreadsheet context unavailable".into())
        }
        fn read_range(&self, _: Range) -> Result<Value, ContextError> {
            Err("spreadsheet context unavailable".into())
        }
        fn worksheet_function(&self, _: &str, _: &[Value]) -> Result<Value, ContextError> {
            Err("spreadsheet context unavailable".into())
        }
        fn set_volatile(&self, _: bool) -> Result<(), ContextError> {
            Err("spreadsheet context unavailable".into())
        }
    }
    scoped_context(&Unavailable, false, call)
}

fn scoped_context<T>(
    context: &dyn SpreadsheetContext,
    available: bool,
    call: impl FnOnce() -> Result<T, Error>,
) -> Result<T, Error> {
    let refused = RefCell::new(None);
    let result = CONTEXT.set((context, &refused, available), call);
    match refused.into_inner() {
        Some(message) => Err(Error::HostRefused(message)),
        None => result,
    }
}

pub(crate) fn record_capability_refusal(reason: &'static str) {
    if CONTEXT.is_set() {
        CONTEXT.with(|(context, refused, _)| {
            refused
                .borrow_mut()
                .get_or_insert_with(|| context.refuse(reason));
        });
    }
}

pub(crate) fn record_not_modelled(owner: &'static str, description: String) {
    if CONTEXT.is_set() {
        CONTEXT.with(|(context, refused, available)| {
            let mut slot = refused.borrow_mut();
            if slot.is_none() {
                if available && !context.event_view() {
                    let reason = if description.contains(crate::NEEDS_A_WORKBOOK) {
                        crate::NEEDS_A_WORKBOOK
                    } else {
                        "an unmodelled spreadsheet object-model operation"
                    };
                    let _ = context.refuse_by(reason, owner);
                }
                *slot = Some(description);
            }
        });
    }
}

pub(crate) fn record_refusal_message(message: String) {
    if CONTEXT.is_set() {
        CONTEXT.with(|(_, refused, _)| {
            refused.borrow_mut().get_or_insert(message);
        });
    }
}

pub(crate) fn with_user_context(call: &mut dyn FnMut(&mut dyn calc_func::UserFunctionContext)) {
    if CONTEXT.is_set() {
        CONTEXT.with(|(context, _, available)| {
            if available {
                context.with_user_context(call);
            }
        });
    }
}

fn active_stored_reference(range: Range) -> Option<calc_func::StoredReference> {
    if !CONTEXT.is_set() {
        return None;
    }
    CONTEXT.with(|(context, _, available)| available.then(|| context.stored_reference(range)).flatten())
}

pub(crate) fn with_active_context<T>(
    call: impl FnOnce(&dyn SpreadsheetContext) -> Result<T, ContextError>,
) -> Result<T, AutoError> {
    if !CONTEXT.is_set() {
        return Err(AutoError::not_modelled(
            "spreadsheet call context is not available",
            "dc-piw0",
        ));
    }
    CONTEXT.with(|(context, refused, available)| {
        let outcome = if available {
            call(context)
        } else {
            Err(ContextError::Refused(
                "spreadsheet call context is not available".into(),
            ))
        };
        match outcome {
            Ok(value) => Ok(value),
            Err(ContextError::Vba(number, message)) => Err(AutoError::excel(number, message)),
            Err(ContextError::Refused(message)) => {
                refused.borrow_mut().get_or_insert_with(|| message.clone());
                Err(AutoError::not_modelled(message, "dc-piw0"))
            }
        }
    })
}

pub(crate) fn event_view_active() -> bool {
    CONTEXT.is_set() && CONTEXT.with(|(context, _, available)| available && context.event_view())
}

/// Whether the running code was called as a worksheet function (a context with a calling cell),
/// not from a document event or outside a calculation.
pub(crate) fn worksheet_function_call_active() -> bool {
    CONTEXT.is_set() && CONTEXT.with(|(context, _, available)| available && !context.event_view())
}

/// Owns the session's `ObjectModel` registration (via thread-local id so `HostServices` stays
/// `Send + Sync`) and forwards non-DNA COM references to Windows COM when enabled.
pub(crate) struct SpreadsheetHost {
    base: Arc<dyn HostServices>,
    /// Thread-local handle to this session's `Rc<dyn ObjectModel>` (`projection::SESSION_MODELS`).
    model_id: u64,
    /// `true` when the caller supplied an external `ObjectModel` (`WorkbookAutomation`); `false`
    /// for the portable `InvocationModel`.
    external_model: bool,
    /// Platform COM for the objects of the project's type-library references (early-bound
    /// through their registered type libraries). DNA's own objects never reach it.
    com: Option<Arc<dyn HostServices>>,
    /// Native Declare is available only while loading a Windows project in Full mode.
    native_dynlink: Option<Arc<dyn HostServices>>,
    /// What the project's own source activates late-bound (refused).
    late: super::late_binding::LateBinding,
}

impl Drop for SpreadsheetHost {
    fn drop(&mut self) {
        projection::unregister_model(self.model_id);
    }
}

const MAX_VALUE_CELLS: u64 = 1_048_576;

/// The reason an explicit `CreateObject` or `GetObject` is refused.
const CREATE_OBJECT_REFUSAL: &str = "CreateObject or GetObject (late-bound COM activation; DNA Calc - Formula Explorer binds COM early, through imported type libraries)";

/// The reason a late-bound call on a COM object is refused (owner rule: early-bound COM through
/// imported type libraries first, never late binding as a start).
const LATE_BOUND_REFUSAL: &str =
    "a late-bound COM call (DNA Calc - Formula Explorer binds COM early, through imported type libraries)";

fn fault(code: i32, message: impl Into<String>) -> HalError {
    HalError::adapter_fault(
        HalProfileId::Null,
        CapabilityId::ComActivationDispatch,
        "spreadsheet",
        message,
    )
    .with_host_error_code(code)
}

fn typed_fault_to_hal(err: calcvba_host_api::TypedFault) -> HalError {
    fault(err.number, err.description)
}

pub(crate) const TYPED_RANGE_IID: calcvba_host_api::InterfaceId = descriptor::TYPED_RANGE_IID;

impl SpreadsheetHost {
    pub(crate) fn typed_range(self: &Arc<Self>, range: Range) -> calcvba_host_api::HostObject {
        let host = self.clone();
        calcvba_host_api::HostObject::new(
            TYPED_RANGE_IID,
            vec![calcvba_host_api::HostSlot::double(move |_| {
                let value = host
                    .read(range)
                    .map_err(|error| error.host_error_code.unwrap_or(5))?;
                let variant = to_variant(&value).map_err(|_| 13)?;
                let value =
                    super::coerce_argument(variant, &calcvba_oxir::OxTy::Double).map_err(|_| 13)?;
                value.as_f64().ok_or(13)
            })],
        )
    }

    pub(crate) fn new(
        com: bool,
        late: super::late_binding::LateBinding,
        model: Rc<dyn ObjectModel>,
        external_model: bool,
    ) -> Self {
        let model_id = projection::register_model(model);
        let native_dynlink = if crate::policy::thread_policy() == Some(crate::policy::PolicyMode::Full) {
            platform_services(false, true)
        } else {
            None
        };
        #[cfg(not(target_arch = "wasm32"))]
        let mounted_host = crate::files::current_host();
        #[cfg(target_arch = "wasm32")]
        let mounted_host = None;
        let base = mounted_host.unwrap_or_else(|| {
            HostBuilder::new()
                .policy(HostPolicy {
                    allow_dynamic_link: native_dynlink.is_some(),
                    ..HostPolicy::default()
                })
                .build()
        });
        Self {
            late,
            base,
            model_id,
            external_model,
            com: if com {
                platform_services(true, false)
            } else {
                None
            },
            native_dynlink,
        }
    }

    fn model(&self) -> HalResult<Rc<dyn ObjectModel>> {
        projection::get_model(self.model_id)
            .ok_or_else(|| fault(429, "spreadsheet object model is unavailable on this thread"))
    }

    /// Whether `object` is one of DNA's own spreadsheet or document objects.
    fn owns(&self, object: &ObjectRef) -> bool {
        self.native_range(object).is_some() || projection::is_model_object(object)
    }

    /// The platform COM host for an object DNA does not own.
    fn foreign(&self, object: &ObjectRef) -> Option<&dyn ComHal> {
        match &self.com {
            Some(com) if !self.owns(object) => Some(com.com()),
            _ => None,
        }
    }

    fn native_range(&self, object: &ObjectRef) -> Option<calcvba_host_api::HostObject> {
        let object = calcvba_host_api::HostObject::from_object_ref(object)?;
        (object.header().iid == TYPED_RANGE_IID).then_some(object)
    }

    fn native_value(&self, object: &calcvba_host_api::HostObject) -> HalResult<Variant> {
        // Result/default materialization admits no further VM work. This callback reads
        // through the current invocation's already-authorized DNA context.
        let mut context = calcvba_host_api::TypedContext {
            remaining_blocks: 0,
        };
        // SAFETY: from_object_ref proved this exact HostObject allocation; the retained
        // owner spans the checked slot call, and this local context has one mutable borrow.
        let value = unsafe {
            calcvba_host_api::object::invoke_double_slot(
                object.argument().as_ptr(),
                TYPED_RANGE_IID,
                0,
                None,
                &mut context,
            )
        }
        .map_err(|number| fault(number, "numeric Range callback failed"))?;
        Ok(Variant::from_f64(value.as_double()))
    }

    pub(crate) fn range_variant(&self, range: Range) -> Result<Variant, Error> {
        let model = self
            .model()
            .map_err(|e| Error::Backend(e.to_string()))?;
        let range_iface = descriptor::interface_id_by_name("Range")
            .ok_or_else(|| Error::Backend("Range missing from object-model descriptor".into()))?;
        let key = if self.external_model {
            let stored = active_stored_reference(range).ok_or_else(|| {
                Error::ValueConversion("Range requires a live stored reference".into())
            })?;
            ObjectKey::Range(stored)
        } else {
            ObjectKey::Host(invocation_model::encode_range(range, RangeAxis::Cells))
        };
        let host_obj = projection::wrap_object(
            model,
            ModelObjectRef::new(range_iface, key),
            None,
            None,
        );
        Ok(Variant::from_object_ref(host_obj.object_ref()))
    }

    fn context<T>(
        &self,
        call: impl FnOnce(&dyn SpreadsheetContext) -> Result<T, ContextError>,
    ) -> HalResult<T> {
        if !CONTEXT.is_set() {
            return Err(fault(5, "spreadsheet call context is not available"));
        }
        CONTEXT.with(|(context, refused, available)| {
            match if available {
                call(context)
            } else {
                Err("spreadsheet call context is not available".into())
            } {
                Ok(value) => Ok(value),
                Err(ContextError::Vba(number, message)) => Err(fault(number, message)),
                Err(ContextError::Refused(message)) => {
                    refused.borrow_mut().get_or_insert_with(|| message.clone());
                    Err(fault(5, message))
                }
            }
        })
    }

    fn unsupported<T>(&self, reason: &'static str) -> HalResult<T> {
        self.context(|context| Err(context.refuse(reason).into()))
    }

    pub(crate) fn read_argument_range(&self, range: Range) -> Result<Value, Error> {
        self.read_for(range, true)
            .map_err(|e| Error::ValueConversion(e.to_string()))
    }

    fn read(&self, range: Range) -> HalResult<Value> {
        self.read_for(range, false)
    }

    fn read_for(&self, range: Range, argument: bool) -> HalResult<Value> {
        let info = self.context(|context| context.metadata(range))?;
        if info.cells > MAX_VALUE_CELLS {
            return self.unsupported("Range.Value2 exceeds the per-read cell limit");
        }
        self.context(|context| {
            let value = if argument {
                context.read_argument_range(range)
            } else {
                context.read_range(range)
            }?;
            let shape_ok = match &value {
                Value::Array { rows, columns, .. } => {
                    *rows == info.rows
                        && *columns == info.columns
                        && (info.rows != 1 || info.columns != 1)
                }
                Value::Range(_) | Value::Missing => false,
                _ => info.rows == 1 && info.columns == 1,
            };
            if !shape_ok {
                return Err(ContextError::Refused(
                    "host supplied an invalid Range.Value2 shape".into(),
                ));
            }
            Ok(value)
        })
    }

    pub(crate) fn result(&self, value: &Variant) -> Result<Value, Error> {
        if let Some(native) = value
            .as_object_ref()
            .and_then(|object| self.native_range(&object))
        {
            return self
                .native_value(&native)
                .map_err(|error| Error::Backend(error.to_string()))
                .and_then(|value| from_variant(&value, false));
        }
        if let Some(inner) = value
            .as_object_ref()
            .and_then(|object| projection::unwrap_model_object(&object))
            && let Some(materialized) = inner
                .materialize_udf_range()
                .map_err(|err| Error::ValueConversion(err.description))?
        {
            // Excel materializes an object-valued UDF result through its Range value.
            return from_variant(&materialized, false);
        }
        from_variant(value, false)
    }
}

impl HostServices for SpreadsheetHost {
    fn is_host_class(&self, target: &str) -> bool {
        target.eq_ignore_ascii_case(descriptor::GLOBAL_GUID_TEXT)
    }
    fn profile(&self) -> HalProfileId {
        HalProfileId::Null
    }
    fn descriptor(&self) -> HalDescriptor {
        let mut descriptor = self.base.descriptor();
        for capability in &mut descriptor.capabilities {
            match capability.id {
                CapabilityId::ComActivationDispatch | CapabilityId::EventPump => capability.supported = true,
                CapabilityId::DynamicLinking if self.native_dynlink.is_some() => {
                    capability.supported = true;
                }
                _ => {}
            }
        }
        descriptor
    }
    fn policy(&self) -> &HostPolicy {
        self.base.policy()
    }
    fn console(&self) -> &dyn ConsoleHal {
        self.base.console()
    }
    fn ui(&self) -> &dyn UiInteractionHal {
        self
    }
    fn events(&self) -> &dyn EventPumpHal {
        self
    }
    fn fs(&self) -> &dyn FileSystemHal {
        self.base.fs()
    }
    fn process(&self) -> &dyn ProcessEnvHal {
        self.base.process()
    }
    fn com(&self) -> &dyn ComHal {
        self
    }
    fn time_locale(&self) -> &dyn TimeLocaleHal {
        self.base.time_locale()
    }
    fn dynlink(&self) -> &dyn DynamicLinkHal {
        self.native_dynlink
            .as_deref()
            .unwrap_or(self.base.as_ref())
            .dynlink()
    }
    fn diag(&self) -> &dyn DiagnosticsHal {
        self.base.diag()
    }
}

impl UiInteractionHal for SpreadsheetHost {
    fn msg_box_variant(&self, prompt: Variant, style: Variant) -> HalResult<Variant> {
        match crate::dialog::msgbox(&prompt, &style) {
            Some(answer) => Ok(answer),
            None => self.base.ui().msg_box_variant(prompt, style),
        }
    }

    fn input_box_variant(&self, prompt: Variant, default: Variant) -> HalResult<Variant> {
        match crate::dialog::inputbox(&prompt) {
            Some(answer) => Ok(answer),
            None => self.base.ui().input_box_variant(prompt, default),
        }
    }

    fn send_keys_variant(&self, keys: Variant, wait: Variant) -> HalResult<Variant> {
        self.base.ui().send_keys_variant(keys, wait)
    }

    fn app_activate_variant(&self, title: Variant, wait: Variant) -> HalResult<Variant> {
        self.base.ui().app_activate_variant(title, wait)
    }
}

/// Windows supplies COM for referenced libraries and native Declare for Full projects.
fn platform_services(allow_com: bool, allow_dynamic_link: bool) -> Option<Arc<dyn HostServices>> {
    #[cfg(all(windows, not(target_arch = "wasm32")))]
    {
        let policy = HostPolicy {
            allow_interaction: false,
            allow_process_spawn: false,
            allow_filesystem_mutation: false,
            allow_dynamic_link,
            allow_com_activation: allow_com,
            deterministic_mode: false,
            com_invocation_strategy: calcvba_hal::model::ComInvocationStrategy::PreferVtable,
            ..HostPolicy::default()
        };
        Some(
            HostBuilder::new()
                .profile(HalProfileId::Windows)
                .policy(policy)
                .build(),
        )
    }
    #[cfg(not(all(windows, not(target_arch = "wasm32"))))]
    {
        let _ = (allow_com, allow_dynamic_link);
        None
    }
}

/// `DoEvents` hands control to Excel to process pending events and returns 0. This host raises
/// its events synchronously, as the change that causes them completes, so none is pending.
impl EventPumpHal for SpreadsheetHost {
    fn do_events_variant(&self) -> HalResult<Variant> {
        Ok(Variant::from_i32(0))
    }
}

impl ComHal for SpreadsheetHost {
    fn get_object_variant(&self, pathname: Variant, class: Variant) -> HalResult<Variant> {
        match &self.com {
            Some(com) => com.com().get_object_variant(pathname, class),
            None => self.unsupported("GetObject needs a COM reference of the project"),
        }
    }
    unsafe fn bind_native_dispatch_object_variant(
        &self,
        prog_id: &str,
        dispatch: *mut core::ffi::c_void,
    ) -> HalResult<Variant> {
        match &self.com {
            // SAFETY: the caller's retained-pointer contract is forwarded unchanged.
            Some(com) => unsafe {
                com.com()
                    .bind_native_dispatch_object_variant(prog_id, dispatch)
            },
            None => Err(fault(
                429,
                "native COM objects need a COM reference of the project",
            )),
        }
    }
    fn retain_native_dispatch_object(
        &self,
        object: ObjectRef,
    ) -> HalResult<*mut core::ffi::c_void> {
        match self.foreign(&object) {
            Some(com) => com.retain_native_dispatch_object(object),
            None => Err(fault(438, "a DNA object has no native IDispatch")),
        }
    }
    unsafe fn bind_native_dispatch_value(
        &self,
        type_hint: &str,
        dispatch: *mut core::ffi::c_void,
    ) -> HalResult<Variant> {
        match &self.com {
            // SAFETY: the caller's retained-pointer contract is forwarded unchanged.
            Some(com) => unsafe { com.com().bind_native_dispatch_value(type_hint, dispatch) },
            None => Err(fault(
                429,
                "native COM objects need a COM reference of the project",
            )),
        }
    }
    unsafe fn prepare_typed_object(
        &self,
        object: ObjectRef,
        members: &[TypeLibMemberMetadata],
    ) -> HalResult<calcvba_host_api::HostObject> {
        match self.foreign(&object) {
            // SAFETY: the compiler-retained metadata contract is forwarded unchanged.
            Some(com) => unsafe { com.prepare_typed_object(object, members) },
            None => self.prepare_dynamic_object(object),
        }
    }
    fn release_object_variant(&self, object: ObjectRef) -> HalResult<Variant> {
        match self.foreign(&object) {
            Some(com) => com.release_object_variant(object),
            None => Ok(Variant::empty()),
        }
    }
    unsafe fn prepare_typed_object_with_interfaces(
        &self,
        object: ObjectRef,
        members: &[TypeLibMemberMetadata],
        interfaces: &[calcvba_com::TypeLibInterfaceMetadata],
    ) -> HalResult<calcvba_host_api::HostObject> {
        match self.foreign(&object) {
            // SAFETY: the compiler-retained metadata contract is forwarded unchanged.
            Some(com) => unsafe {
                com.prepare_typed_object_with_interfaces(object, members, interfaces)
            },
            None => self.prepare_dynamic_object(object),
        }
    }
    unsafe fn prepare_typed_object_with_locale(
        &self,
        object: ObjectRef,
        members: &[TypeLibMemberMetadata],
        interfaces: &[calcvba_com::TypeLibInterfaceMetadata],
        locale_id: u32,
    ) -> HalResult<calcvba_host_api::HostObject> {
        match self.foreign(&object) {
            // SAFETY: the compiler-retained metadata contract is forwarded unchanged.
            Some(com) => unsafe {
                com.prepare_typed_object_with_locale(object, members, interfaces, locale_id)
            },
            None => self.prepare_dynamic_object(object),
        }
    }
    fn enumerate_object(&self, object: ObjectRef) -> HalResult<Vec<Variant>> {
        if let Some(com) = self.foreign(&object) {
            return com.enumerate_object(object);
        }
        if let Some(inner) = projection::unwrap_model_object(&object) {
            return inner.enumerate_items().map_err(typed_fault_to_hal);
        }
        self.unsupported("For Each over a DNA object")
    }
    fn dispatch_invoke_variant(&self, request: &calcvba_com::ComInvokeRequest) -> HalResult<Variant> {
        let _ = request;
        self.unsupported(LATE_BOUND_REFUSAL)
    }
    fn dispatch_invoke_dynamic_variant_with_writebacks(
        &self,
        request: &DynamicCallRequest,
    ) -> HalResult<(Variant, Vec<Option<Variant>>)> {
        Ok((self.dispatch_invoke_dynamic_variant(request)?, Vec::new()))
    }
    fn unsubscribe_event_variant(&self, subscription: ComSubscriptionToken) -> HalResult<Variant> {
        match &self.com {
            Some(com) => com.com().unsubscribe_event_variant(subscription),
            None => Err(fault(438, "events are unavailable")),
        }
    }
    fn event_callback_variant(
        &self,
        callback: ComCallbackToken,
        index: usize,
    ) -> HalResult<Variant> {
        match &self.com {
            Some(com) => com.com().event_callback_variant(callback, index),
            None => Err(fault(438, "events are unavailable")),
        }
    }
    fn release_event_callback_variant(&self, callback: ComCallbackToken) -> HalResult<Variant> {
        match &self.com {
            Some(com) => com.com().release_event_callback_variant(callback),
            None => Err(fault(438, "events are unavailable")),
        }
    }
    fn com_dispatch_transport_counts(&self) -> (u64, u64) {
        self.com
            .as_ref()
            .map_or((0, 0), |com| com.com().com_dispatch_transport_counts())
    }
    fn prepare_dynamic_object(&self, object: ObjectRef) -> HalResult<calcvba_host_api::HostObject> {
        if let Some(com) = self.foreign(&object) {
            return com.prepare_dynamic_object(object);
        }
        if let Some(host_obj) = calcvba_host_api::HostObject::from_object_ref(&object)
            && (host_obj.header().iid == TYPED_RANGE_IID || projection::is_model_object(&object))
        {
            return Ok(host_obj);
        }
        if let Some(native) = object.project_field_get(calcvba_bundle::HOST_DOCUMENT_FIELD)
            && let Some(doc_ref) = native.as_object_ref()
            && let Some(host_obj) = calcvba_host_api::HostObject::from_object_ref(&doc_ref)
            && projection::is_model_object(&doc_ref)
        {
            return Ok(host_obj);
        }
        Err(fault(
            429,
            "only an admitted DNA object can enter a typed activation",
        ))
    }
    fn invoke_early_bound(
        &self,
        object: ObjectRef,
        member: &TypeLibMemberMetadata,
        args: &[calcvba_com::DynamicCallArg],
    ) -> HalResult<calcvba_runtime::RuntimeCallResult> {
        if let Some(com) = self.foreign(&object) {
            return com.invoke_early_bound(object, member, args);
        }
        if let Some(native) = self.native_range(&object) {
            if member.token == 1
                && member.invoke_kind == TypeLibMemberInvokeKind::PropertyGet
                && args.is_empty()
            {
                return self
                    .native_value(&native)
                    .map(calcvba_runtime::RuntimeCallResult::value);
            }
            return Err(fault(438, "unknown numeric Range member"));
        }
        if let Some(inner) = projection::unwrap_model_object(&object) {
            return inner
                .invoke_early_hal(member.token, member.invoke_kind, args)
                .map(calcvba_runtime::RuntimeCallResult::value)
                .map_err(typed_fault_to_hal);
        }
        Err(fault(438, "foreign or expired spreadsheet object"))
    }

    fn app_object_variant(&self, name: Variant) -> HalResult<Variant> {
        if name
            .as_bstr()
            .is_some_and(|s| s.as_str().eq_ignore_ascii_case(descriptor::GLOBAL_GUID_TEXT))
        {
            let model = self.model()?;
            let app = model.application();
            let host_obj = projection::wrap_object(model, app, None, None);
            return Ok(Variant::from_object_ref(host_obj.object_ref()));
        }
        self.create_object_variant(name)
    }

    fn create_object_variant(&self, name: Variant) -> HalResult<Variant> {
        if let Some(com) = self.com.as_ref() {
            // Owner rule: early-bound COM through imported type libraries first, never late
            // binding as a start. `New X` of an imported class compiles to this activation with the
            // class's ProgID; an explicit CreateObject or GetObject in the project's source is late
            // binding and is refused (see late_binding.rs).
            if name
                .as_bstr()
                .is_some_and(|s| self.late.refuses(&s.as_str()))
            {
                return self.unsupported(CREATE_OBJECT_REFUSAL);
            }
            com.com().create_object_variant(name)
        } else {
            self.unsupported("CreateObject of a class that is not an imported type library class (only the injected spreadsheet Application is available; DNA binds COM early)")
        }
    }
    fn describe_object(&self, object: ObjectRef) -> HalResult<Option<ComObjectDescriptor>> {
        if let Some(com) = self.foreign(&object) {
            return com.describe_object(object);
        }
        if !self.owns(&object) {
            return Err(fault(438, "foreign or expired spreadsheet object"));
        }
        Ok(Some(ComObjectDescriptor {
            object,
            prog_id_name: "Excel.Object".into(),
            transport: ComObjectTransportKind::Projection,
            supports_events: false,
            known_member_tokens: vec![],
            known_event_tokens: vec![],
            default_member_token: Some(0.into()),
            default_member_name: Some("_Default".into()),
            typelib_cache_key: None,
        }))
    }
    fn object_default_value(&self, object: ObjectRef) -> HalResult<Option<Variant>> {
        if let Some(com) = self.foreign(&object) {
            return com.object_default_value(object);
        }
        if let Some(native) = self.native_range(&object) {
            return self.native_value(&native).map(Some);
        }
        if let Some(inner) = projection::unwrap_model_object(&object) {
            return inner.default_value().map_err(typed_fault_to_hal);
        }
        Ok(None)
    }
    fn object_type_name(&self, object: ObjectRef) -> HalResult<Option<String>> {
        if let Some(com) = self.foreign(&object) {
            return com.object_type_name(object);
        }
        if self.native_range(&object).is_some() {
            return Ok(Some("Range".into()));
        }
        if let Some(name) = projection::object_type_name(&object) {
            return Ok(Some(name));
        }
        Err(fault(438, "foreign or expired spreadsheet object"))
    }
    fn dispatch_invoke_dynamic_variant(&self, request: &DynamicCallRequest) -> HalResult<Variant> {
        if self.foreign(&request.object).is_some() {
            return self.unsupported(LATE_BOUND_REFUSAL);
        }
        if let Some(native) = self.native_range(&request.object) {
            let is_value = match &request.member {
                calcvba_com::DynamicMemberSelector::Name(n)
                | calcvba_com::DynamicMemberSelector::TokenNamed { name: n, .. } => {
                    n.as_str().eq_ignore_ascii_case("NumericValue")
                        || n.as_str().eq_ignore_ascii_case("Item")
                }
                calcvba_com::DynamicMemberSelector::DefaultMember => true,
                _ => false,
            };
            if request.args.is_empty() && is_value {
                return self.native_value(&native);
            }
            return self.unsupported("a numeric Range member outside its typed interface");
        }
        if let Some(inner) = projection::unwrap_model_object(&request.object) {
            return inner
                .invoke_dynamic_hal(&request.member, request.call_kind_hint, &request.args)
                .map_err(typed_fault_to_hal);
        }
        Err(fault(438, "foreign or expired spreadsheet object"))
    }
    fn advise_events(
        &self,
        source: &ObjectRef,
        interface: &str,
        key: u64,
    ) -> HalResult<Box<dyn calcvba_com::call::ComEventConnection>> {
        if let Some(com) = self.foreign(source) {
            return com.advise_events(source, interface, key);
        }
        self.unsupported("spreadsheet WithEvents subscriptions are not modelled")
    }

    fn subscribe_event(
        &self,
        object: ObjectRef,
        event: ComMemberToken,
    ) -> HalResult<ComSubscriptionToken> {
        if let Some(com) = self.foreign(&object) {
            return com.subscribe_event(object, event);
        }
        self.unsupported("spreadsheet WithEvents subscriptions are not modelled")
    }
    fn poll_event_callback(&self) -> HalResult<Option<ComCallbackPayload>> {
        match &self.com {
            Some(com) => com.com().poll_event_callback(),
            None => Ok(None),
        }
    }
    fn event_callback_subscription(
        &self,
        callback: ComCallbackToken,
    ) -> HalResult<ComSubscriptionToken> {
        match &self.com {
            Some(com) => com.com().event_callback_subscription(callback),
            None => Err(fault(438, "events are unavailable")),
        }
    }
    fn event_callback_arity(&self, callback: ComCallbackToken) -> HalResult<usize> {
        match &self.com {
            Some(com) => com.com().event_callback_arity(callback),
            None => Err(fault(438, "events are unavailable")),
        }
    }
    fn resolve_typelib_reference(
        &self,
        request: &TypeLibResolveRequest,
    ) -> HalResult<TypeLibResolvedIdentity> {
        match &self.com {
            Some(com) => com.com().resolve_typelib_reference(request),
            None => Err(fault(438, "external type libraries are unavailable")),
        }
    }
    fn load_typelib_metadata(
        &self,
        identity: &TypeLibResolvedIdentity,
    ) -> HalResult<TypeLibMetadataBlob> {
        match &self.com {
            Some(com) => com.com().load_typelib_metadata(identity),
            None => Err(fault(438, "external type libraries are unavailable")),
        }
    }
    fn invalidate_typelib_cache(
        &self,
        scope: TypeLibCacheScope,
        reference_name: Option<&str>,
    ) -> HalResult<Variant> {
        match &self.com {
            Some(com) => com.com().invalidate_typelib_cache(scope, reference_name),
            None => Ok(Variant::empty()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::DescriptorTypeLibResolver;
    use calcvba_symbol::TypeLibResolver;

    fn test_host() -> SpreadsheetHost {
        SpreadsheetHost::new(
            false,
            Default::default(),
            Rc::new(invocation_model::InvocationModel::new(false)),
            false,
        )
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn descriptor_projection_uses_real_dispids_and_refuses_unknown_dispid() {
        let host = test_host();
        struct Fixture;
        impl SpreadsheetContext for Fixture {
            fn caller(&self) -> Option<Range> {
                None
            }
            fn selection(&self) -> Result<Range, ContextError> {
                Ok(Range { id: 1 })
            }
            fn metadata(&self, _: Range) -> Result<RangeInfo, ContextError> {
                Ok(RangeInfo {
                    row: 1,
                    column: 1,
                    rows: 2,
                    columns: 3,
                    cells: 6,
                    areas: 1,
                    address: "$A$1:$C$2".into(),
                })
            }
            fn cell(&self, _: Range, _: u32, _: u32) -> Result<Range, ContextError> {
                unreachable!()
            }
            fn area(&self, _: Range, _: u32) -> Result<Range, ContextError> {
                unreachable!()
            }
            fn read_range(&self, _: Range) -> Result<Value, ContextError> {
                unreachable!()
            }
            fn worksheet_function(&self, _: &str, _: &[Value]) -> Result<Value, ContextError> {
                unreachable!()
            }
            fn set_volatile(&self, _: bool) -> Result<(), ContextError> {
                unreachable!()
            }
        }
        let value = host.range_variant(Range { id: 0 }).unwrap();
        let object = value.as_object_ref().unwrap();
        let resolver = DescriptorTypeLibResolver {
            has_workbook: true,
            events: crate::events::DocumentEvents::All,
        };
        let metadata = resolver
            .resolve(&TypeLibResolveRequest {
                reference_name: "Excel".into(),
                requested_coclass: Some("Range".into()),
                importlib_hint: None,
                libid_hint: None,
                major_version_hint: None,
                minor_version_hint: None,
                lcid_hint: None,
            })
            .unwrap();
        // Real Excel Range.Count DISPID is 118 from excel.json.
        let mut count_member = metadata
            .members
            .iter()
            .find(|m| m.name == "Count" && m.invoke_kind == TypeLibMemberInvokeKind::PropertyGet)
            .cloned()
            .unwrap();
        assert_eq!(count_member.token, 118);
        count_member.name = "not-a-host-member".into();
        let result = with_context(&Fixture, || {
            host.invoke_early_bound(object.clone(), &count_member, &[])
                .map_err(|e| Error::Backend(e.to_string()))
        })
        .unwrap();
        assert_eq!(result.value, Some(Variant::from_i32(6)));
        count_member.name = "Count".into();
        count_member.token = 999_999;
        assert_eq!(
            host.invoke_early_bound(object, &count_member, &[])
                .unwrap_err()
                .host_error_code,
            Some(438),
            "a recognizable name cannot recover an unknown descriptor DISPID"
        );
        // Excel.Global appobject returns Application, whose Selection returns a Range with
        // real Range IID and TypeName "Range".
        with_context(&Fixture, || {
            let app = host
                .app_object_variant(Variant::from_string(descriptor::GLOBAL_GUID_TEXT))
                .unwrap()
                .as_object_ref()
                .unwrap();
            assert_eq!(
                host.object_type_name(app.clone()).unwrap().as_deref(),
                Some("Application")
            );
            let app_meta = resolver
                .resolve(&TypeLibResolveRequest {
                    reference_name: "Excel".into(),
                    requested_coclass: Some("Application".into()),
                    importlib_hint: None,
                    libid_hint: None,
                    major_version_hint: None,
                    minor_version_hint: None,
                    lcid_hint: None,
                })
                .unwrap();
            let sel_member = app_meta
                .members
                .iter()
                .find(|m| m.name == "Selection")
                .unwrap();
            let sel = host
                .invoke_early_bound(app, sel_member, &[])
                .unwrap()
                .value
                .unwrap()
                .as_object_ref()
                .unwrap();
            let native = host.prepare_dynamic_object(sel.clone()).unwrap();
            let again = host.prepare_dynamic_object(sel).unwrap();
            assert!(native.same_identity(&again));
            assert_eq!(
                host.object_type_name(native.object_ref())
                    .unwrap()
                    .as_deref(),
                Some("Range")
            );
            let addr_member = metadata
                .members
                .iter()
                .find(|m| m.name == "Address")
                .unwrap();
            let result = host
                .invoke_early_bound(native.object_ref(), addr_member, &[])
                .unwrap();
            assert_eq!(result.value, Some(Variant::from_string("$A$1:$C$2")));
            Ok(())
        })
        .unwrap();
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cell_write_is_error_1004_in_udf_and_refusal_outside_udf() {
        struct Bare {
            event: bool,
        }
        impl SpreadsheetContext for Bare {
            fn event_view(&self) -> bool {
                self.event
            }
            fn caller(&self) -> Option<Range> {
                None
            }
            fn metadata(&self, _: Range) -> Result<RangeInfo, ContextError> {
                unreachable!()
            }
            fn cell(&self, _: Range, _: u32, _: u32) -> Result<Range, ContextError> {
                unreachable!()
            }
            fn area(&self, _: Range, _: u32) -> Result<Range, ContextError> {
                unreachable!()
            }
            fn read_range(&self, _: Range) -> Result<Value, ContextError> {
                unreachable!()
            }
            fn worksheet_function(&self, _: &str, _: &[Value]) -> Result<Value, ContextError> {
                unreachable!()
            }
            fn set_volatile(&self, _: bool) -> Result<(), ContextError> {
                unreachable!()
            }
        }
        let host = test_host();
        let range = host.range_variant(Range { id: 0 }).unwrap().as_object_ref().unwrap();
        let resolver = DescriptorTypeLibResolver {
            has_workbook: true,
            events: crate::events::DocumentEvents::All,
        };
        let put_v2 = resolver
            .resolve(&TypeLibResolveRequest {
                reference_name: "Excel".into(),
                requested_coclass: Some("Range".into()),
                importlib_hint: None,
                libid_hint: None,
                major_version_hint: None,
                minor_version_hint: None,
                lcid_hint: None,
            })
            .unwrap()
            .members
            .into_iter()
            .find(|m| m.name == "Value2" && m.invoke_kind == TypeLibMemberInvokeKind::PropertyPut)
            .unwrap();
        let arg = [calcvba_com::DynamicCallArg {
            value: Some(Variant::from_i32(7).into()),
            name: None,
            by_ref: None,
        }];
        let udf_err = with_context(&Bare { event: false }, || {
            let err = host.invoke_early_bound(range.clone(), &put_v2, &arg).unwrap_err();
            assert_eq!(err.host_error_code, Some(1004));
            Ok(())
        });
        assert!(udf_err.is_ok(), "UDF write raises trappable 1004, not a host refusal");

        let event_err = with_context(&Bare { event: true }, || {
            let _ = host.invoke_early_bound(range.clone(), &put_v2, &arg);
            Ok(())
        })
        .unwrap_err();
        assert!(
            matches!(event_err, Error::HostRefused(ref why) if why.contains("read-only")),
            "unexpected: {event_err:?}"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn office_descriptor_resolves_textframe2_and_cross_library_shape_return() {
        let resolver = DescriptorTypeLibResolver {
            has_workbook: true,
            events: crate::events::DocumentEvents::All,
        };
        let shape_meta = resolver
            .resolve(&TypeLibResolveRequest {
                reference_name: "Excel".into(),
                requested_coclass: Some("Shape".into()),
                importlib_hint: None,
                libid_hint: None,
                major_version_hint: None,
                minor_version_hint: None,
                lcid_hint: None,
            })
            .unwrap();
        let tf2 = shape_meta
            .members
            .iter()
            .find(|m| m.name == "TextFrame2")
            .expect("Shape.TextFrame2 in descriptor");
        assert!(matches!(
            tf2.return_wire_type,
            Some(calcvba_com::TypeLibWireType::InterfacePointer { ref name })
                if name.eq_ignore_ascii_case("Office.TextFrame2")
        ));
        let office_tf2 = resolver
            .resolve(&TypeLibResolveRequest {
                reference_name: "Office".into(),
                requested_coclass: Some("TextFrame2".into()),
                importlib_hint: None,
                libid_hint: None,
                major_version_hint: None,
                minor_version_hint: None,
                lcid_hint: None,
            })
            .unwrap();
        assert!(
            office_tf2.members.iter().any(|m| m.name == "TextRange"),
            "Office.TextFrame2 must expose TextRange from office.json"
        );
    }
}
