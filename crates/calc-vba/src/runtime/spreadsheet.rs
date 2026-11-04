//! Read-only spreadsheet objects. Values are always read through the current host call.
use super::{Error, Value, from_variant, to_variant};
use oxvba_com::{
    ComCallbackPayload, ComCallbackToken, ComMemberToken, ComObjectDescriptor,
    ComObjectTransportKind, ComSubscriptionToken, DynamicCallKind, DynamicCallRequest,
    DynamicMemberSelector, TypeLibCacheScope, TypeLibMetadataBlob, TypeLibResolveRequest,
    TypeLibResolvedIdentity,
};
use oxvba_hal::{
    HostServices,
    adapters::builder::HostBuilder,
    error::{HalError, HalResult},
    model::{CapabilityId, HalDescriptor, HalProfileId, HostPolicy},
    traits::*,
};
use oxvba_runtime::{ObjectRef, Variant};
use std::{
    cell::RefCell,
    sync::{Arc, Mutex},
};

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
    fn caller(&self) -> Option<Range>;
    /// Record a capability refusal in the owning evaluator, independently of VBA On Error.
    fn refuse(&self, reason: &'static str) -> String {
        reason.into()
    }
    fn metadata(&self, range: Range) -> Result<RangeInfo, ContextError>;
    fn external_address(&self, _range: Range) -> Result<String, ContextError> {
        Err(self.refuse("reference names without an admitted metadata view").into())
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

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Axis {
    Cells,
    Rows,
    Columns,
    Areas,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Object {
    Application,
    Functions,
    Range(Range, Axis),
}

/// Owns only object identities and range coordinates, never a borrowed reader or cell cache.
pub(crate) struct SpreadsheetHost {
    base: Arc<dyn HostServices>,
    objects: Mutex<ObjectTable>,
}

#[derive(Default)]
struct ObjectTable {
    by_id: std::collections::BTreeMap<i32, Object>,
    by_object: std::collections::BTreeMap<Object, i32>,
}
const FIRST_OBJECT: i32 = 0x1000_0000;
const MAX_OBJECTS: usize = 4096;
const MAX_VALUE_CELLS: u64 = 1_048_576;

fn fault(code: i32, message: impl Into<String>) -> HalError {
    HalError::adapter_fault(
        HalProfileId::Null,
        CapabilityId::ComActivationDispatch,
        "spreadsheet",
        message,
    )
    .with_host_error_code(code)
}

pub(crate) const TYPED_RANGE_IID: calcvba_host_api::InterfaceId = *b"DnaCalcRange0001";
pub(crate) fn typed_iid() -> oxvba_com::ComInterfaceIid {
    dna_iid(TYPED_RANGE_IID)
}

// Nominal identity for the Variant-valued projection, distinct from the numeric slot ABI.
// This is DNA's own descriptor identity, not Excel's COM interface identity.
fn projected_range_iid() -> oxvba_com::ComInterfaceIid {
    dna_iid(*b"DnaCalcRange0002")
}

fn dna_iid(bytes: [u8; 16]) -> oxvba_com::ComInterfaceIid {
    oxvba_com::ComInterfaceIid {
        data1: u32::from_le_bytes(bytes[..4].try_into().unwrap()),
        data2: u16::from_le_bytes(bytes[4..6].try_into().unwrap()),
        data3: u16::from_le_bytes(bytes[6..8].try_into().unwrap()),
        data4: bytes[8..].try_into().unwrap(),
    }
}
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
                    super::coerce_argument(variant, &oxvba_oxir::OxTy::Double).map_err(|_| 13)?;
                value.as_f64().ok_or(13)
            })],
        )
    }
    pub(crate) fn new() -> Self {
        Self {
            base: HostBuilder::new().build(),
            objects: Mutex::new(ObjectTable::default()),
        }
    }

    fn object(&self, object: Object) -> HalResult<Variant> {
        let mut objects = self
            .objects
            .lock()
            .map_err(|_| fault(5, "object table is unavailable"))?;
        let id = match objects.by_object.get(&object) {
            Some(id) => *id,
            None => {
                if objects.by_id.len() == MAX_OBJECTS {
                    return self.unsupported("spreadsheet object limit exceeded");
                }
                static NEXT: std::sync::atomic::AtomicI32 =
                    std::sync::atomic::AtomicI32::new(FIRST_OBJECT);
                let id = NEXT
                    .fetch_update(
                        std::sync::atomic::Ordering::Relaxed,
                        std::sync::atomic::Ordering::Relaxed,
                        |id| id.checked_add(1),
                    )
                    .map_err(|_| fault(5, "spreadsheet object identities exhausted (dc-hsa5.4)"))?;
                objects.by_id.insert(id, object);
                objects.by_object.insert(object, id);
                id
            }
        };
        Ok(Variant::from_object_ref(ObjectRef::from_compat_identity(
            id,
        )))
    }
    fn lookup(&self, object: &ObjectRef) -> HalResult<Object> {
        // raw() is only defined for compatibility carriers, never a native IUnknown.
        if !object.is_compat_object() {
            return self
                .unsupported("a native spreadsheet object outside the supported projection");
        }
        self.objects
            .lock()
            .map_err(|_| fault(5, "object table is unavailable"))?
            .by_id
            .get(&object.raw())
            .copied()
            .ok_or_else(|| fault(438, "foreign or expired spreadsheet object"))
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
        self.object(Object::Range(range, Axis::Cells))
            .map_err(|e| Error::Backend(e.to_string()))
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

    pub(crate) fn read_range(&self, range: Range) -> Result<Value, Error> {
        self.read(range)
            .map_err(|e| Error::ValueConversion(e.to_string()))
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
        if let Some(object) = value.as_object_ref()
            && let Object::Range(range, _) = self
                .lookup(&object)
                .map_err(|e| Error::ValueConversion(e.to_string()))?
        {
            // Excel materializes an object-valued UDF result through its Range value.
            let value = self.read_range(range)?;
            return from_variant(&to_variant(&value)?, false);
        }
        from_variant(value, false)
    }

    fn argument(&self, value: &Variant) -> HalResult<Value> {
        if let Some(object) = value.as_object_ref() {
            if let Some(native) = self.native_range(&object) {
                return from_variant(&self.native_value(&native)?, false)
                    .map_err(|error| fault(13, error.to_string()));
            }
            return match self.lookup(&object)? {
                Object::Range(range, _) => Ok(Value::Range(range)),
                _ => Err(fault(13, "expected a worksheet value or Range")),
            };
        }
        from_variant(value, false).map_err(|e| fault(13, e.to_string()))
    }

    fn range_member(
        &self,
        range: Range,
        axis: Axis,
        name: &str,
        args: &[Variant],
    ) -> HalResult<Variant> {
        let info = self.context(|context| context.metadata(range))?;
        match name {
            "VALUE2" if args.is_empty() => {
                to_variant(&self.read(range)?).map_err(|e| fault(13, e.to_string()))
            }
            "COUNT" if args.is_empty() => {
                let count = match axis {
                    Axis::Rows => u64::from(info.rows),
                    Axis::Columns => u64::from(info.columns),
                    Axis::Cells => info.cells,
                    Axis::Areas => u64::from(info.areas),
                };
                Ok(Variant::from_i32(
                    i32::try_from(count).map_err(|_| fault(6, "Overflow"))?,
                ))
            }
            "ROW" if args.is_empty() => Ok(Variant::from_i32(info.row as i32)),
            "COLUMN" if args.is_empty() => Ok(Variant::from_i32(info.column as i32)),
            "ADDRESS" => {
                let supplied = |index: usize| args.get(index)
                    .filter(|v| v.as_error_code() != Some(-2147352572));
                let boolean = |index: usize, default: bool| -> HalResult<bool> {
                    supplied(index).map_or(Ok(default), |value| {
                        oxvba_eval::arith::coerce_numeric(value, oxvba_bundle::NumericCoerceTarget::Boolean)
                            .map_err(|e| fault(e.code, e.message))
                            .map(|v| v.as_bool().expect("Boolean coercion"))
                    })
                };
                if !boolean(0, true)? || !boolean(1, true)? {
                    return self.unsupported("relative Range.Address coordinates");
                }
                if let Some(style) = supplied(2) {
                    let style = oxvba_eval::arith::coerce_numeric(style, oxvba_bundle::NumericCoerceTarget::Long)
                        .map_err(|e| fault(e.code, e.message))?;
                    if style.as_i32() != Some(1) {
                        return self.unsupported("a non-A1 Range.Address reference style");
                    }
                }
                if supplied(4).is_some() {
                    return self.unsupported("Range.Address RelativeTo");
                }
                let address = if boolean(3, false)? {
                    self.context(|context| context.external_address(range))?
                } else {
                    info.address
                };
                Ok(Variant::from_string(address))
            }
            "AREAS" if args.is_empty() => self.object(Object::Range(range, Axis::Areas)),
            "ROWS" if args.is_empty() => self.object(Object::Range(range, Axis::Rows)),
            "COLUMNS" if args.is_empty() => self.object(Object::Range(range, Axis::Columns)),
            "CELLS" | "ITEM" if args.is_empty() => {
                if name == "ITEM" {
                    to_variant(&self.read(range)?).map_err(|e| fault(13, e.to_string()))
                } else {
                    self.object(Object::Range(range, Axis::Cells))
                }
            }
            "ITEM" if axis == Axis::Areas => {
                let [index] = args else {
                    return Err(fault(450, "wrong Areas index count"));
                };
                let index = oxvba_eval::arith::coerce_numeric(
                    index,
                    oxvba_bundle::NumericCoerceTarget::Long,
                )
                .map_err(|e| fault(e.code, e.message))?
                .as_i32()
                .unwrap();
                let index = u32::try_from(index)
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or_else(|| fault(1004, "Areas index out of bounds"))?;
                let selected = self.context(|context| context.area(range, index))?;
                self.object(Object::Range(selected, Axis::Cells))
            }
            "ITEM" if axis != Axis::Cells => {
                self.unsupported("indexed Rows/Columns are not supported")
            }
            "CELLS" | "ITEM" => {
                fn index(value: &Variant) -> HalResult<u32> {
                    let n = oxvba_eval::arith::coerce_numeric(
                        value,
                        oxvba_bundle::NumericCoerceTarget::Long,
                    )
                    .map_err(|e| fault(e.code, e.message))?
                    .as_i32()
                    .expect("Long coercion");
                    u32::try_from(n)
                        .ok()
                        .filter(|n| *n > 0)
                        .ok_or_else(|| fault(1004, "Range index is outside this read-only host"))
                }
                let (row, column) = match args {
                    [one] => {
                        let n = index(one)? - 1;
                        (n / info.columns + 1, n % info.columns + 1)
                    }
                    [row, column] => (index(row)?, index(column)?),
                    _ => return Err(fault(450, "wrong number of Range indices")),
                };
                let selected = self.context(|context| context.cell(range, row, column))?;
                self.object(Object::Range(selected, Axis::Cells))
            }
            _ => self.unsupported("a read-only Range member outside the supported host surface"),
        }
    }

    fn projected_arguments(
        &self,
        object: Object,
        name: &str,
        args: &[oxvba_com::DynamicCallArg],
    ) -> HalResult<Vec<Variant>> {
        let missing = || Variant::from_error_code(-2147352572);
        if name == "ADDRESS" && matches!(object, Object::Range(..)) {
            let names = ["RowAbsolute", "ColumnAbsolute", "ReferenceStyle", "External", "RelativeTo"];
            let mut values = vec![missing(); names.len()];
            let mut supplied = [false; 5];
            for (position, arg) in args.iter().enumerate() {
                let index = match arg.name.as_deref() {
                    Some(name) => match names.iter().position(|n| n.eq_ignore_ascii_case(name)) {
                        Some(index) => index,
                        None => return self.unsupported("an unknown Range.Address argument"),
                    },
                    None => position,
                };
                if index >= values.len() || supplied[index] {
                    return Err(fault(450, "duplicate or excess Range.Address arguments"));
                }
                supplied[index] = true;
                values[index] = arg.value.as_ref().map(|v| v.variant().clone()).unwrap_or_else(missing);
            }
            return Ok(values);
        }
        if args.iter().any(|arg| arg.name.is_some()) {
            return self.unsupported("named host arguments are not supported");
        }
        Ok(args.iter().map(|arg| arg.value.as_ref()
            .map(|v| v.variant().clone()).unwrap_or_else(missing)).collect())
    }

    /// Shared legacy projection execution; member selection happens at the caller.
    fn invoke_projected_member(
        &self,
        object: Object,
        name: &str,
        args: &[Variant],
    ) -> HalResult<Variant> {
        match object {
            Object::Application => match name {
                "CALLER" if args.is_empty() => {
                    match self.context(|context| Ok(context.caller()))? {
                        Some(range) => self.object(Object::Range(range, Axis::Cells)),
                        None => Ok(Variant::from_error_code(2023)),
                    }
                }
                "WORKSHEETFUNCTION" if args.is_empty() => self.object(Object::Functions),
                "VOLATILE" => {
                    let enabled = match args {
                        [] => true,
                        [one] if one.as_error_code() == Some(-2147352572) => true,
                        [one] => oxvba_eval::arith::coerce_numeric(
                            one,
                            oxvba_bundle::NumericCoerceTarget::Boolean,
                        )
                        .map_err(|e| fault(e.code, e.message))?
                        .as_bool()
                        .expect("Boolean coercion"),
                        _ => return Err(fault(450, "wrong Volatile argument count")),
                    };
                    self.context(|context| context.set_volatile(enabled))?;
                    Ok(Variant::empty())
                }
                _ => self.unsupported("an Application member outside the supported host surface"),
            },
            Object::Functions => {
                let args = args
                    .iter()
                    .map(|arg| self.argument(arg))
                    .collect::<HalResult<Vec<_>>>()?;
                let value = self.context(|context| context.worksheet_function(name, &args))?;
                if matches!(value, Value::Error(_)) {
                    return Err(fault(
                        1004,
                        format!("WorksheetFunction.{name} could not calculate"),
                    ));
                }
                to_variant(&value).map_err(|e| fault(13, e.to_string()))
            }
            Object::Range(range, axis) => self.range_member(range, axis, name, args),
        }
    }
}

impl HostServices for SpreadsheetHost {
    fn profile(&self) -> HalProfileId {
        HalProfileId::Null
    }
    fn descriptor(&self) -> HalDescriptor {
        let mut descriptor = self.base.descriptor();
        for capability in &mut descriptor.capabilities {
            if capability.id == CapabilityId::ComActivationDispatch {
                capability.supported = true;
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
        self.base.ui()
    }
    fn events(&self) -> &dyn EventPumpHal {
        self.base.events()
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
        self.base.dynlink()
    }
    fn diag(&self) -> &dyn DiagnosticsHal {
        self.base.diag()
    }
}

impl ComHal for SpreadsheetHost {
    fn invoke_early_bound(
        &self,
        object: ObjectRef,
        member: &TypeLibMemberMetadata,
        args: &[oxvba_com::DynamicCallArg],
    ) -> HalResult<oxvba_runtime::RuntimeCallResult> {
        // Preserve only the existing portable projection until extraction to host
        // glue. Compiler-bound calls use its fixed tokens, never dynamic names.
        // This is not native COM/vtable compatibility evidence.
        if let Some(native) = self.native_range(&object) {
            if member.token == 1
                && member.invoke_kind == TypeLibMemberInvokeKind::PropertyGet
                && args.is_empty()
            {
                return self
                    .native_value(&native)
                    .map(oxvba_runtime::RuntimeCallResult::value);
            }
            return Err(fault(438, "unknown numeric Range member"));
        }
        let object = self.lookup(&object)?;
        let name = match (object, member.token, member.invoke_kind) {
            (Object::Application, 1, TypeLibMemberInvokeKind::PropertyGet) => "CALLER",
            (Object::Application, 2, TypeLibMemberInvokeKind::PropertyGet) => "WORKSHEETFUNCTION",
            (Object::Application, 3, TypeLibMemberInvokeKind::Method) => "VOLATILE",
            (Object::Range(..), token, TypeLibMemberInvokeKind::PropertyGet) => match token {
                1 => "VALUE2",
                2 => "COUNT",
                3 => "ROW",
                4 => "COLUMN",
                5 => "ADDRESS",
                6 => "ROWS",
                7 => "COLUMNS",
                8 => "CELLS",
                9 => "AREAS",
                100..=299 => {
                    return self
                        .unsupported("a declared Range member outside the supported host surface");
                }
                _ => return Err(fault(438, "unknown typed projection member")),
            },
            (_, 100..=399, _) => {
                return self.unsupported(
                    "a declared member or accessor outside the supported host surface",
                );
            }
            _ => return Err(fault(438, "unsupported typed projection member/accessor")),
        };
        let args = self.projected_arguments(object, name, args)?;
        self.invoke_projected_member(object, name, &args)
            .map(oxvba_runtime::RuntimeCallResult::value)
    }

    fn create_object_variant(&self, name: Variant) -> HalResult<Variant> {
        if name
            .as_bstr()
            .is_some_and(|s| s.as_str().eq_ignore_ascii_case("DnaCalc.Application"))
        {
            self.object(Object::Application)
        } else {
            self.unsupported("only the injected spreadsheet Application is available")
        }
    }
    fn describe_object(&self, object: ObjectRef) -> HalResult<Option<ComObjectDescriptor>> {
        if self.native_range(&object).is_none() {
            self.lookup(&object)?;
        }
        Ok(Some(ComObjectDescriptor {
            object,
            prog_id_name: "DnaCalc.ReadOnly".into(),
            transport: ComObjectTransportKind::Projection,
            supports_events: false,
            known_member_tokens: vec![],
            known_event_tokens: vec![],
            default_member_token: None,
            default_member_name: Some("Item".into()),
            typelib_cache_key: None,
        }))
    }
    fn object_default_value(&self, object: ObjectRef) -> HalResult<Option<Variant>> {
        if let Some(native) = self.native_range(&object) {
            return self.native_value(&native).map(Some);
        }
        match self.lookup(&object)? {
            Object::Range(range, axis) => self.range_member(range, axis, "VALUE2", &[]).map(Some),
            _ => Ok(None),
        }
    }
    fn object_type_name(&self, object: ObjectRef) -> HalResult<Option<String>> {
        if self.native_range(&object).is_some() {
            return Ok(Some("Range".into()));
        }
        Ok(Some(
            match self.lookup(&object)? {
                Object::Application => "Application",
                Object::Functions => "WorksheetFunction",
                Object::Range(..) => "Range",
            }
            .into(),
        ))
    }
    fn dispatch_invoke_dynamic_variant(&self, request: &DynamicCallRequest) -> HalResult<Variant> {
        if matches!(
            request.call_kind_hint,
            Some(DynamicCallKind::PropertyLet | DynamicCallKind::PropertySet)
        ) {
            return self.unsupported("spreadsheet objects are read-only");
        }
        let name = match &request.member {
            DynamicMemberSelector::Name(n) | DynamicMemberSelector::TokenNamed { name: n, .. } => {
                n.as_str()
            }
            DynamicMemberSelector::DefaultMember => "Item",
            _ => return Err(fault(438, "member name is required")),
        }
        .to_ascii_uppercase();
        if let Some(native) = self.native_range(&request.object) {
            if request.args.is_empty() && matches!(name.as_str(), "NUMERICVALUE" | "ITEM") {
                return self.native_value(&native);
            }
            return self.unsupported("a numeric Range member outside its typed interface");
        }
        let object = self.lookup(&request.object)?;
        let args = self.projected_arguments(object, &name, &request.args)?;
        self.invoke_projected_member(object, &name, &args)
    }
    fn subscribe_event(&self, _: ObjectRef, _: ComMemberToken) -> HalResult<ComSubscriptionToken> {
        self.unsupported("spreadsheet WithEvents subscriptions are not modelled")
    }
    fn poll_event_callback(&self) -> HalResult<Option<ComCallbackPayload>> {
        Ok(None)
    }
    fn event_callback_subscription(&self, _: ComCallbackToken) -> HalResult<ComSubscriptionToken> {
        Err(fault(438, "events are unavailable"))
    }
    fn event_callback_arity(&self, _: ComCallbackToken) -> HalResult<usize> {
        Err(fault(438, "events are unavailable"))
    }
    fn resolve_typelib_reference(
        &self,
        _: &TypeLibResolveRequest,
    ) -> HalResult<TypeLibResolvedIdentity> {
        Err(fault(438, "external type libraries are unavailable"))
    }
    fn load_typelib_metadata(&self, _: &TypeLibResolvedIdentity) -> HalResult<TypeLibMetadataBlob> {
        Err(fault(438, "external type libraries are unavailable"))
    }
    fn invalidate_typelib_cache(
        &self,
        _: TypeLibCacheScope,
        _: Option<&str>,
    ) -> HalResult<Variant> {
        Ok(Variant::empty())
    }
}

pub(crate) struct SpreadsheetTypes;
impl oxvba_symbol::TypeLibResolver for SpreadsheetTypes {
    fn interface_iid(
        &self,
        request: &TypeLibResolveRequest,
        _: &TypeLibMetadataBlob,
    ) -> Option<oxvba_com::ComInterfaceIid> {
        if request
            .reference_name
            .eq_ignore_ascii_case("DnaCalcObjects")
        {
            Some(typed_iid())
        } else if request.reference_name.eq_ignore_ascii_case("Excel") {
            // Without a nominal identity, declaration lowering continues to the next
            // library and can attach the numeric-only Range to an Excel.Value2 call.
            Some(projected_range_iid())
        } else {
            None
        }
    }
    fn resolve(&self, request: &TypeLibResolveRequest) -> Option<TypeLibMetadataBlob> {
        use oxvba_com::{
            TypeLibMemberInvokeKind as Kind, TypeLibMemberMetadata, TypeLibParamType as Ty,
        };
        fn member(
            name: &str,
            token: i32,
            kind: Kind,
            parameters: usize,
            return_type: Ty,
        ) -> TypeLibMemberMetadata {
            TypeLibMemberMetadata {
                name: name.into(),
                token,
                vtable_slot: None,
                requires_argument: false,
                invoke_kind: kind,
                parameter_names: (0..parameters).map(|i| format!("arg{i}")).collect(),
                parameter_optional: vec![true; parameters],
                parameter_optional_defaults: vec![],
                is_default_member: false,
                parameter_types: vec![Ty::Variant; parameters],
                parameter_wire_types: vec![],
                parameter_iids: vec![],
                return_type: Some(return_type),
                return_wire_type: None,
                callconv_is_stdcall: false,
                is_dual: false,
                interface_iid: None,
                source_typekind: None,
                vtable_slot_bound: None,
            }
        }
        if request.reference_name.eq_ignore_ascii_case("DnaCalc")
            && let Some(global) = request.requested_coclass.as_deref().filter(|name| {
                ["ActiveSheet", "ActiveWorkbook", "ActiveWindow", "Selection"]
                    .iter()
                    .any(|s| name.eq_ignore_ascii_case(s))
            })
        {
            let declarations = match global.to_ascii_lowercase().as_str() {
                "activesheet" => vec![
                    member("Name", 1, Kind::PropertyGet, 0, Ty::Variant),
                    member("Range", 2, Kind::PropertyGet, 2, Ty::Variant),
                ],
                "activeworkbook" => vec![member("Name", 1, Kind::PropertyGet, 0, Ty::Variant)],
                "activewindow" => vec![
                    member("ScrollRow", 1, Kind::PropertyGet, 0, Ty::Variant),
                    member("ScrollRow", 1, Kind::PropertyPut, 1, Ty::Variant),
                ],
                "selection" => vec![member("Address", 1, Kind::PropertyGet, 5, Ty::Variant)],
                _ => unreachable!("validated global"),
            };
            return Some(TypeLibMetadataBlob {
                constants: Vec::new(),
                identity: TypeLibResolvedIdentity {
                    reference_name: request.reference_name.clone(),
                    requested_coclass: Some(global.into()),
                    importlib: "host:dnacalc-readonly-v1".into(),
                    libid: None,
                    major_version: 1,
                    minor_version: 0,
                    lcid: None,
                    cache_key: format!("dnacalc-readonly-v1:{global}"),
                },
                activation_prog_id: Some(format!("DnaCalc.{global}")),
                member_name_to_token: declarations
                    .iter()
                    .map(|m| (m.name.clone(), m.token))
                    .collect(),
                members: declarations,
                events: vec![],
                coclass_names: vec![global.into()],
            });
        }
        if request
            .reference_name
            .eq_ignore_ascii_case("DnaCalcObjects")
        {
            if request
                .requested_coclass
                .as_deref()
                .is_some_and(|name| !name.eq_ignore_ascii_case("Range"))
            {
                return None;
            }
            let mut numeric = member("NumericValue", 1, Kind::PropertyGet, 0, Ty::Double);
            numeric.vtable_slot = Some(0);
            numeric.interface_iid = Some(typed_iid());
            numeric.vtable_slot_bound = Some(1);
            numeric.is_default_member = true;
            return Some(TypeLibMetadataBlob {
                constants: Vec::new(),
                identity: TypeLibResolvedIdentity {
                    reference_name: request.reference_name.clone(),
                    requested_coclass: Some("Range".into()),
                    importlib: "calcvba-host-slots-v1".into(),
                    libid: None,
                    major_version: 1,
                    minor_version: 0,
                    lcid: None,
                    cache_key: "dnacalc-numeric-range-v1".into(),
                },
                activation_prog_id: None,
                member_name_to_token: vec![(numeric.name.clone(), numeric.token)],
                members: vec![numeric],
                events: vec![],
                coclass_names: vec!["Range".into()],
            });
        }
        let application = request.reference_name.eq_ignore_ascii_case("DnaCalc");
        if !application && !request.reference_name.eq_ignore_ascii_case("Excel") {
            return None;
        }
        let class = if application { "Application" } else { "Range" };
        let mut members = if application {
            vec![
                member("Caller", 1, Kind::PropertyGet, 0, Ty::Object),
                member("WorksheetFunction", 2, Kind::PropertyGet, 0, Ty::Object),
                member("Volatile", 3, Kind::Method, 1, Ty::Variant),
            ]
        } else {
            vec![
                member("Value2", 1, Kind::PropertyGet, 0, Ty::Variant),
                member("Count", 2, Kind::PropertyGet, 0, Ty::Long),
                member("Row", 3, Kind::PropertyGet, 0, Ty::Long),
                member("Column", 4, Kind::PropertyGet, 0, Ty::Long),
                member("Address", 5, Kind::PropertyGet, 5, Ty::String),
                member("Rows", 6, Kind::PropertyGet, 0, Ty::Object),
                member("Columns", 7, Kind::PropertyGet, 0, Ty::Object),
                member("Cells", 8, Kind::PropertyGet, 2, Ty::Object),
                member("Areas", 9, Kind::PropertyGet, 0, Ty::Object),
            ]
        };
        if !application {
            let address = members.iter_mut().find(|m| m.name == "Address").expect("Range Address metadata");
            address.parameter_names = ["RowAbsolute", "ColumnAbsolute", "ReferenceStyle", "External", "RelativeTo"]
                .into_iter().map(str::to_owned).collect();
        }
        if application {
            for name in [
                "Version",
                "Build",
                "CalculationVersion",
                "OperatingSystem",
                "Calculation",
                "Iteration",
                "MaxIterations",
                "MaxChange",
                "CalculateBeforeSave",
                "EnableEvents",
                "MultiThreadedCalculation",
                "AutomationSecurity",
                "AddIns",
                "COMAddIns",
                "Workbooks",
                "CutCopyMode",
            ] {
                let token = 100 + members.len() as i32;
                members.push(member(name, token, Kind::PropertyGet, 0, Ty::Variant));
                if [
                    "Calculation",
                    "Iteration",
                    "MaxIterations",
                    "MaxChange",
                    "CalculateBeforeSave",
                    "EnableEvents",
                    "AutomationSecurity",
                    "CutCopyMode",
                ]
                .contains(&name)
                {
                    members.push(member(name, token, Kind::PropertyPut, 1, Ty::Variant));
                }
            }
            for (name, args) in [
                ("Calculate", 0),
                ("CalculateFull", 0),
                ("CalculateFullRebuild", 0),
                ("Run", 31),
                ("MacroOptions", 11),
                ("RegisterXLL", 1),
                ("Union", 30),
            ] {
                members.push(member(
                    name,
                    200 + members.len() as i32,
                    Kind::Method,
                    args,
                    Ty::Variant,
                ));
            }
            let metadata = members
                .iter_mut()
                .find(|m| m.name == "MacroOptions")
                .expect("declared member");
            metadata.parameter_names = [
                "Macro",
                "Description",
                "HasMenu",
                "MenuText",
                "HasShortcutKey",
                "ShortcutKey",
                "Category",
                "StatusBar",
                "HelpContextID",
                "HelpFile",
                "ArgumentDescriptions",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect();
        }
        Some(TypeLibMetadataBlob {
            constants: Vec::new(),
            identity: TypeLibResolvedIdentity {
                reference_name: request.reference_name.clone(),
                requested_coclass: Some(class.into()),
                importlib: "host:dnacalc-readonly-v1".into(),
                libid: None,
                major_version: 1,
                minor_version: 0,
                lcid: None,
                cache_key: format!("dnacalc-readonly-v1:{class}"),
            },
            activation_prog_id: application.then(|| "DnaCalc.Application".into()),
            member_name_to_token: members.iter().map(|m| (m.name.clone(), m.token)).collect(),
            members,
            events: vec![],
            coclass_names: vec![class.into()],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxvba_symbol::TypeLibResolver;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn legacy_typed_projection_uses_fixed_token_and_refuses_unknown_token() {
        let host = SpreadsheetHost::new();
        struct Fixture;
        impl SpreadsheetContext for Fixture {
            fn caller(&self) -> Option<Range> {
                None
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
        let metadata = SpreadsheetTypes
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
        let mut member = metadata.members.into_iter().find(|m| m.token == 2).unwrap();
        member.name = "not-a-host-member".into();
        let result = with_context(&Fixture, || {
            host.invoke_early_bound(object.clone(), &member, &[])
                .map_err(|e| Error::Backend(e.to_string()))
        })
        .unwrap();
        assert_eq!(result.value, Some(Variant::from_i32(6)));
        member.name = "Count".into();
        member.token = 999;
        assert_eq!(
            host.invoke_early_bound(object, &member, &[])
                .unwrap_err()
                .host_error_code,
            Some(438),
            "a recognizable name cannot recover an unbound typed member"
        );
    }
}
