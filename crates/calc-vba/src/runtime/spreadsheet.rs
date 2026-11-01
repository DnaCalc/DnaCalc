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

/// A rectangle in a host-owned sheet namespace. Coordinates are one-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Range {
    pub sheet: u64,
    pub row: u32,
    pub column: u32,
    pub rows: u32,
    pub columns: u32,
}

impl Range {
    pub fn validate(&self) -> Result<(), String> {
        if self.row == 0
            || self.column == 0
            || self.rows == 0
            || self.columns == 0
            || self
                .row
                .checked_add(self.rows - 1)
                .is_none_or(|n| n > 1_048_576)
            || self
                .column
                .checked_add(self.columns - 1)
                .is_none_or(|n| n > 16_384)
        {
            return Err("range is outside the worksheet grid".into());
        }
        Ok(())
    }

    fn address(&self) -> String {
        fn cell(row: u32, mut col: u32) -> String {
            let mut name = Vec::new();
            while col > 0 {
                col -= 1;
                name.push(b'A' + (col % 26) as u8);
                col /= 26;
            }
            name.reverse();
            format!("${}${row}", String::from_utf8(name).expect("ASCII column"))
        }
        let first = cell(self.row, self.column);
        if self.rows == 1 && self.columns == 1 {
            first
        } else {
            format!(
                "{first}:{}",
                cell(self.row + self.rows - 1, self.column + self.columns - 1)
            )
        }
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
    fn read_range(&self, range: Range) -> Result<Value, String>;
    fn worksheet_function(&self, name: &str, args: &[Value]) -> Result<Value, String>;
    fn set_volatile(&self, value: bool) -> Result<(), String>;
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
        fn read_range(&self, _: Range) -> Result<Value, String> {
            Err("spreadsheet context unavailable".into())
        }
        fn worksheet_function(&self, _: &str, _: &[Value]) -> Result<Value, String> {
            Err("spreadsheet context unavailable".into())
        }
        fn set_volatile(&self, _: bool) -> Result<(), String> {
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    Cells,
    Rows,
    Columns,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Object {
    Application,
    Functions,
    Range(Range, Axis),
}

/// Owns only object identities and range coordinates, never a borrowed reader or cell cache.
pub(crate) struct SpreadsheetHost {
    base: Arc<dyn HostServices>,
    objects: Mutex<Vec<Object>>,
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

impl SpreadsheetHost {
    pub(crate) fn new() -> Self {
        Self {
            base: HostBuilder::new().build(),
            objects: Mutex::new(Vec::new()),
        }
    }

    fn object(&self, object: Object) -> HalResult<Variant> {
        if let Object::Range(range, _) = object {
            range.validate().map_err(|e| fault(1004, e))?;
        }

        let mut objects = self
            .objects
            .lock()
            .map_err(|_| fault(5, "object table is unavailable"))?;
        let index = match objects.iter().position(|item| *item == object) {
            Some(index) => index,
            None => {
                if objects.len() == MAX_OBJECTS {
                    return self.unsupported("spreadsheet object limit exceeded");
                }
                objects.push(object);
                objects.len() - 1
            }
        };
        Ok(Variant::from_object_ref(ObjectRef::from_compat_identity(
            FIRST_OBJECT + index as i32,
        )))
    }

    fn lookup(&self, object: &ObjectRef) -> HalResult<Object> {
        let index = object
            .raw()
            .checked_sub(FIRST_OBJECT)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| fault(438, "foreign spreadsheet object"))?;
        self.objects
            .lock()
            .map_err(|_| fault(5, "object table is unavailable"))?
            .get(index)
            .copied()
            .ok_or_else(|| fault(438, "unknown spreadsheet object"))
    }

    pub(crate) fn range_variant(&self, range: Range) -> Result<Variant, Error> {
        range.validate().map_err(Error::ValueConversion)?;
        self.object(Object::Range(range, Axis::Cells))
            .map_err(|e| Error::Backend(e.to_string()))
    }

    fn context<T>(
        &self,
        call: impl FnOnce(&dyn SpreadsheetContext) -> Result<T, String>,
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
                Err(message) => {
                    refused.borrow_mut().get_or_insert_with(|| message.clone());
                    Err(fault(5, message))
                }
            }
        })
    }

    fn unsupported<T>(&self, reason: &'static str) -> HalResult<T> {
        self.context(|context| Err(context.refuse(reason)))
    }

    pub(crate) fn read_range(&self, range: Range) -> Result<Value, Error> {
        self.read(range)
            .map_err(|e| Error::ValueConversion(e.to_string()))
    }

    fn read(&self, range: Range) -> HalResult<Value> {
        if u64::from(range.rows) * u64::from(range.columns) > MAX_VALUE_CELLS {
            return self.unsupported("Range.Value2 exceeds the per-read cell limit");
        }
        self.context(|context| {
            let value = context.read_range(range)?;
            let shape_ok = match &value {
                Value::Array { rows, columns, .. } => {
                    *rows == range.rows
                        && *columns == range.columns
                        && (range.rows != 1 || range.columns != 1)
                }
                Value::Range(_) | Value::Missing => false,
                _ => range.rows == 1 && range.columns == 1,
            };
            if !shape_ok {
                return Err("host supplied an invalid Range.Value2 shape".into());
            }
            Ok(value)
        })
    }

    pub(crate) fn result(&self, value: &Variant) -> Result<Value, Error> {
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
        match name {
            "VALUE2" if args.is_empty() => {
                to_variant(&self.read(range)?).map_err(|e| fault(13, e.to_string()))
            }
            "COUNT" if args.is_empty() => {
                let count = match axis {
                    Axis::Rows => u64::from(range.rows),
                    Axis::Columns => u64::from(range.columns),
                    Axis::Cells => u64::from(range.rows) * u64::from(range.columns),
                };
                Ok(Variant::from_i32(
                    i32::try_from(count).map_err(|_| fault(6, "Overflow"))?,
                ))
            }
            "ROW" if args.is_empty() => Ok(Variant::from_i32(range.row as i32)),
            "COLUMN" if args.is_empty() => Ok(Variant::from_i32(range.column as i32)),
            "ADDRESS" if args.is_empty() => Ok(Variant::from_string(range.address())),
            "ROWS" if args.is_empty() => self.object(Object::Range(range, Axis::Rows)),
            "COLUMNS" if args.is_empty() => self.object(Object::Range(range, Axis::Columns)),
            "CELLS" | "ITEM" if args.is_empty() => {
                if name == "ITEM" {
                    to_variant(&self.read(range)?).map_err(|e| fault(13, e.to_string()))
                } else {
                    self.object(Object::Range(range, Axis::Cells))
                }
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
                        (n / range.columns + 1, n % range.columns + 1)
                    }
                    [row, column] => (index(row)?, index(column)?),
                    _ => return Err(fault(450, "wrong number of Range indices")),
                };
                let selected = Range {
                    row: range
                        .row
                        .checked_add(row - 1)
                        .ok_or_else(|| fault(1004, "Range row overflow"))?,
                    column: range
                        .column
                        .checked_add(column - 1)
                        .ok_or_else(|| fault(1004, "Range column overflow"))?,
                    rows: 1,
                    columns: 1,
                    ..range
                };
                selected.validate().map_err(|e| fault(1004, e))?;
                self.object(Object::Range(selected, Axis::Cells))
            }
            _ => self.unsupported("a read-only Range member outside the supported host surface"),
        }
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
                _ => return Err(fault(438, "unknown typed projection member")),
            },
            _ => return Err(fault(438, "unsupported typed projection member/accessor")),
        };
        if args.iter().any(|arg| arg.name.is_some()) {
            return self.unsupported("named host arguments are not supported");
        }
        let args = args
            .iter()
            .map(|arg| {
                arg.value
                    .as_ref()
                    .map(|v| v.variant().clone())
                    .unwrap_or_else(|| Variant::from_error_code(-2147352572))
            })
            .collect::<Vec<_>>();
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
        self.lookup(&object)?;
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
        match self.lookup(&object)? {
            Object::Range(range, axis) => self.range_member(range, axis, "VALUE2", &[]).map(Some),
            _ => Ok(None),
        }
    }
    fn object_type_name(&self, object: ObjectRef) -> HalResult<Option<String>> {
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
        if request.args.iter().any(|a| a.name.is_some()) {
            return self.unsupported("named host arguments are not supported");
        }
        let name = match &request.member {
            DynamicMemberSelector::Name(n) | DynamicMemberSelector::TokenNamed { name: n, .. } => {
                n.as_str()
            }
            DynamicMemberSelector::DefaultMember => "Item",
            _ => return Err(fault(438, "member name is required")),
        }
        .to_ascii_uppercase();
        let args = request
            .args
            .iter()
            .map(|arg| {
                arg.value
                    .as_ref()
                    .map(|v| v.variant().clone())
                    .unwrap_or_else(|| Variant::from_error_code(-2147352572))
            })
            .collect::<Vec<_>>();
        self.invoke_projected_member(self.lookup(&request.object)?, &name, &args)
    }
    fn subscribe_event(&self, _: ObjectRef, _: ComMemberToken) -> HalResult<ComSubscriptionToken> {
        Err(fault(438, "events are unavailable"))
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
        let application = request.reference_name.eq_ignore_ascii_case("DnaCalc");
        if !application && !request.reference_name.eq_ignore_ascii_case("Excel") {
            return None;
        }
        let class = if application { "Application" } else { "Range" };
        let members = if application {
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
                member("Address", 5, Kind::PropertyGet, 0, Ty::String),
                member("Rows", 6, Kind::PropertyGet, 0, Ty::Object),
                member("Columns", 7, Kind::PropertyGet, 0, Ty::Object),
                member("Cells", 8, Kind::PropertyGet, 2, Ty::Object),
            ]
        };
        Some(TypeLibMetadataBlob {
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
        let value = host
            .range_variant(Range {
                sheet: 1,
                row: 1,
                column: 1,
                rows: 2,
                columns: 3,
            })
            .unwrap();
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
        let result = host
            .invoke_early_bound(object.clone(), &member, &[])
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
