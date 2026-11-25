//! Type-library metadata for VBA's `Excel` and `Office` references, derived directly from
//! `calc-object-model`'s committed descriptor (`excel.json`, `office.json`). No private member
//! tables or synthetic Excel/Office IIDs live here (dc-piw0.5, docs/architecture/OBJECT_MODEL.md).
use calc_object_model::descriptor::{
    Descriptor, Guid, InterfaceId, InvokeKind, Member, Param, ParamFlag, TypeInfo, TypeKind,
    TypeRef,
};
use calcvba_com::{
    ComDefaultValue, ComInterfaceIid, OptionalParamDefault, SourceTypeKind, TypeLibAppObject,
    TypeLibConstantMetadata, TypeLibEventDispatchPath, TypeLibEventMetadata,
    TypeLibMemberInvokeKind, TypeLibMemberMetadata, TypeLibMetadataBlob, TypeLibParamType,
    TypeLibResolveRequest, TypeLibResolvedIdentity, TypeLibWireType,
};
use calcvba_symbol::TypeLibResolver;
use std::collections::HashSet;

/// Excel's real `_Global` interface GUID (`EXCEL.EXE`), used for the `Excel.Global` appobject.
pub(crate) const GLOBAL_GUID_TEXT: &str = "{000208DB-0000-0000-C000-000000000046}";

/// DNA Calc's own resumable-continuation numeric benchmark interface (`DnaCalcObjects.Range`).
pub(crate) const TYPED_RANGE_IID: [u8; 16] = *b"DnaCalcRange0001";

/// Document events currently delivered by DNA Calc's host (`dc-hsa5.12`).
pub(crate) fn guid_to_com_iid(guid: Guid) -> ComInterfaceIid {
    ComInterfaceIid {
        data1: guid.data1,
        data2: guid.data2,
        data3: guid.data3,
        data4: guid.data4,
    }
}

pub(crate) fn guid_to_bytes(guid: Guid) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[..4].copy_from_slice(&guid.data1.to_le_bytes());
    bytes[4..6].copy_from_slice(&guid.data2.to_le_bytes());
    bytes[6..8].copy_from_slice(&guid.data3.to_le_bytes());
    bytes[8..].copy_from_slice(&guid.data4);
    bytes
}

pub(crate) fn bytes_to_guid(bytes: [u8; 16]) -> Guid {
    Guid {
        data1: u32::from_le_bytes(bytes[..4].try_into().unwrap()),
        data2: u16::from_le_bytes(bytes[4..6].try_into().unwrap()),
        data3: u16::from_le_bytes(bytes[6..8].try_into().unwrap()),
        data4: bytes[8..].try_into().unwrap(),
    }
}

pub(crate) fn bytes_to_com_iid(bytes: [u8; 16]) -> ComInterfaceIid {
    guid_to_com_iid(bytes_to_guid(bytes))
}

pub(crate) fn global_iid_bytes() -> [u8; 16] {
    guid_to_bytes(Guid::parse(GLOBAL_GUID_TEXT).expect("valid _Global GUID"))
}

pub(crate) fn typed_range_com_iid() -> ComInterfaceIid {
    bytes_to_com_iid(TYPED_RANGE_IID)
}

pub(crate) fn interface_iid_bytes(id: InterfaceId) -> [u8; 16] {
    let descriptor = Descriptor::get();
    let iface = descriptor.default_interface(id);
    let guid = Guid::parse(&descriptor.ty(iface).guid).expect("descriptor interface GUID");
    guid_to_bytes(guid)
}

pub(crate) fn interface_id_by_name(name: &str) -> Option<InterfaceId> {
    let descriptor = Descriptor::get();
    let id = descriptor.id(name)?;
    Some(descriptor.default_interface(id))
}

/// The argument shapes a document event's callback can carry (`EventParameter`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EventShape {
    Object,
    Boolean,
    BooleanByRef,
}

/// The shapes of an Excel document event's arguments, or `None` when one is a scalar the event
/// callback cannot carry (Long, String, enumerations).
fn event_shape(member: &Member) -> Option<Vec<EventShape>> {
    member
        .arguments()
        .map(|param| match param.ty.as_str() {
            "BOOL" => Some(EventShape::Boolean),
            "BOOL*" => Some(EventShape::BooleanByRef),
            ty => {
                let base = ty.strip_suffix('*')?;
                let scalar = base.starts_with("enum:")
                    || matches!(base, "BSTR" | "I4" | "I2" | "UI4" | "R8" | "VARIANT" | "BOOL");
                (!scalar).then_some(EventShape::Object)
            }
        })
        .collect()
}

/// The Excel event `event_name` of `host_document_type` (`Workbook` or `Worksheet`), from the
/// descriptor's `WorkbookEvents` or `DocEvents`, when its arguments are carriable.
pub(crate) fn document_event<'a>(
    descriptor: &'a Descriptor,
    host_document_type: &str,
    event_name: &str,
) -> Option<(&'a Member, Vec<EventShape>)> {
    let bare = host_document_type
        .rsplit('.')
        .next()
        .unwrap_or(host_document_type);
    let events = if bare.eq_ignore_ascii_case("Worksheet") {
        "DocEvents"
    } else if bare.eq_ignore_ascii_case("Workbook") {
        "WorkbookEvents"
    } else {
        return None;
    };
    let id = descriptor.id(events)?;
    let member = descriptor
        .members(id)
        .into_iter()
        .find(|member| member.name.eq_ignore_ascii_case(event_name))?;
    let shape = event_shape(member)?;
    Some((member, shape))
}

/// Whether `event_name` is an Excel event of `host_document_type` the project may handle. The
/// host raises only the events its commands cause; the others never fire in this host.
pub(crate) fn is_delivered_document_event(host_document_type: &str, event_name: &str) -> bool {
    document_event(Descriptor::get(), host_document_type, event_name).is_some()
}

/// Supplies `Excel` and `Office` metadata from `calc-object-model`'s committed descriptor, plus
/// DNA Calc's `DnaCalcObjects.Range` resumable-continuation benchmark interface.
pub(crate) struct DescriptorTypeLibResolver {
    /// Whether this compilation has a workbook document (`ThisWorkbook` on `Excel.Global`).
    pub(crate) has_workbook: bool,
    /// Which document events this project delivers (`None`, `Workbook`, or `All`).
    pub(crate) events: crate::events::DocumentEvents,
}

/// Resolves DNA Calc's descriptor libraries (`Excel`, `Office`, `DnaCalcObjects`) first, then
/// delegates other project references (`stdole`, `Scripting`, Windows registered libraries) to
/// CalcVBA's catalogue resolver.
pub(crate) struct WithReferences<'a> {
    pub(crate) own: &'a dyn TypeLibResolver,
    pub(crate) libraries: &'a [String],
}

impl WithReferences<'_> {
    fn is_own_library(request: &TypeLibResolveRequest) -> bool {
        if request.reference_name.eq_ignore_ascii_case("Excel")
            || request.reference_name.eq_ignore_ascii_case("Office")
            || request
                .reference_name
                .eq_ignore_ascii_case("DnaCalcObjects")
        {
            return true;
        }
        request.libid_hint.as_deref().is_some_and(|hint| {
            let upper = hint.to_ascii_uppercase();
            upper.contains(crate::office::EXCEL_LIBRARY)
                || upper.contains(crate::office::OFFICE_LIBRARY)
        })
    }

    fn is_catalog_library(&self, request: &TypeLibResolveRequest) -> bool {
        self.libraries
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&request.reference_name))
    }
}

impl TypeLibResolver for WithReferences<'_> {
    fn dictionary_source(&self) -> calcvba_symbol::DictionarySource {
        if crate::policy::thread_policy() == Some(crate::policy::PolicyMode::Safe) {
            calcvba_symbol::DictionarySource::Engine
        } else {
            self.own.dictionary_source()
        }
    }

    fn interface_iid(
        &self,
        request: &TypeLibResolveRequest,
        blob: &TypeLibMetadataBlob,
    ) -> Option<ComInterfaceIid> {
        if Self::is_own_library(request) {
            self.own.interface_iid(request, blob)
        } else if self.is_catalog_library(request) {
            calcvba_symbol::CatalogTypeLibResolver.interface_iid(request, blob)
        } else {
            self.own.interface_iid(request, blob)
        }
    }

    fn resolve(&self, request: &TypeLibResolveRequest) -> Option<TypeLibMetadataBlob> {
        if Self::is_own_library(request) {
            self.own.resolve(request)
        } else if self.is_catalog_library(request) {
            calcvba_symbol::CatalogTypeLibResolver.resolve(request)
        } else {
            self.own.resolve(request)
        }
    }
}

impl DescriptorTypeLibResolver {
    fn match_library(request: &TypeLibResolveRequest) -> Option<&'static str> {
        if request
            .reference_name
            .eq_ignore_ascii_case("DnaCalcObjects")
        {
            return Some("DnaCalcObjects");
        }
        if request.reference_name.eq_ignore_ascii_case("Excel")
            || request.libid_hint.as_deref().is_some_and(|h| {
                h.to_ascii_uppercase()
                    .contains(crate::office::EXCEL_LIBRARY)
            })
        {
            return Some("Excel");
        }
        if request.reference_name.eq_ignore_ascii_case("Office")
            || request.libid_hint.as_deref().is_some_and(|h| {
                h.to_ascii_uppercase()
                    .contains(crate::office::OFFICE_LIBRARY)
            })
        {
            return Some("Office");
        }
        None
    }

    fn resolve_dnacalc_objects(request: &TypeLibResolveRequest) -> Option<TypeLibMetadataBlob> {
        let Some(requested) = request.requested_coclass.as_deref() else {
            return Some(TypeLibMetadataBlob {
                constants: Vec::new(),
                identity: TypeLibResolvedIdentity {
                    reference_name: request.reference_name.clone(),
                    requested_coclass: None,
                    importlib: "calcvba-host-slots-v1".into(),
                    libid: None,
                    major_version: 1,
                    minor_version: 0,
                    lcid: None,
                    cache_key: "dnacalc-numeric-range-lib-v1".into(),
                },
                activation_prog_id: None,
                member_name_to_token: vec![],
                members: vec![],
                events: vec![],
                coclass_names: vec!["Range".into()],
                enum_type_names: Vec::new(),
                coclass_prog_ids: vec![],
                app_objects: vec![],
                extensible: false,
            });
        };
        if !requested.eq_ignore_ascii_case("Range") {
            return None;
        }
        let numeric = TypeLibMemberMetadata {
            name: "NumericValue".into(),
            token: 1,
            vtable_slot: Some(0),
            requires_argument: false,
            invoke_kind: TypeLibMemberInvokeKind::PropertyGet,
            parameter_names: vec![],
            parameter_optional: vec![],
            parameter_optional_defaults: vec![],
            is_default_member: true,
            parameter_types: vec![],
            parameter_wire_types: vec![],
            parameter_iids: vec![],
            return_type: Some(TypeLibParamType::Double),
            return_wire_type: None,
            callconv_is_stdcall: false,
            is_dual: false,
            interface_iid: Some(typed_range_com_iid()),
            source_typekind: None,
            vtable_slot_bound: Some(1),
        };
        Some(TypeLibMetadataBlob {
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
            enum_type_names: Vec::new(),
            coclass_prog_ids: vec![],
            app_objects: vec![],
            extensible: false,
        })
    }

    /// The enumeration names the descriptor's parameters and return types mention
    /// (`enum:XlCalculation[I4]`): a variable declared `As XlCalculation` holds a Long.
    fn enum_type_names(descriptor: &Descriptor) -> Vec<String> {
        let mut names = std::collections::BTreeSet::new();
        for (id, _) in descriptor.types() {
            for member in descriptor.members(id) {
                let texts = member.params.iter().map(|p| p.ty.as_str()).chain([member.ret.as_str()]);
                for text in texts {
                    if let Some(rest) = text.strip_prefix("enum:") {
                        names.insert(rest.split('[').next().unwrap_or(rest).to_owned());
                    }
                }
            }
        }
        names.into_iter().collect()
    }

    fn library_coclasses(descriptor: &Descriptor, library: &str) -> Vec<String> {
        let mut names: Vec<String> = descriptor
            .types()
            .filter(|(id, _)| descriptor.library(*id).name.eq_ignore_ascii_case(library))
            .map(|(_, ty)| ty.name.clone())
            .collect();
        if library.eq_ignore_ascii_case("Excel") {
            names.push("Global".into());
        }
        names
    }

    fn library_identity(
        descriptor: &Descriptor,
        library: &str,
        request_name: &str,
        coclass: Option<&str>,
    ) -> TypeLibResolvedIdentity {
        let info = descriptor
            .libraries
            .iter()
            .find(|lib| lib.name.eq_ignore_ascii_case(library));
        let (major, minor) = info
            .and_then(|lib| lib.version.split_once('.'))
            .and_then(|(maj, min)| Some((maj.parse().ok()?, min.parse().ok()?)))
            .unwrap_or((1, 0));
        let libid = info.map(|lib| lib.guid.clone());
        let importlib = info
            .map(|lib| lib.source_file.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("descriptor:{library}"));
        let cache_key = match coclass {
            Some(name) => format!("dnacalc-descriptor:{library}:{name}"),
            None => format!("dnacalc-descriptor:{library}"),
        };
        TypeLibResolvedIdentity {
            reference_name: request_name.to_owned(),
            requested_coclass: coclass.map(str::to_owned),
            importlib,
            libid,
            major_version: major,
            minor_version: minor,
            lcid: info.map(|lib| lib.lcid),
            cache_key,
        }
    }

    fn resolve_descriptor_library(
        &self,
        library: &str,
        request: &TypeLibResolveRequest,
    ) -> Option<TypeLibMetadataBlob> {
        let descriptor = Descriptor::get();
        let coclass_names = Self::library_coclasses(descriptor, library);
        let is_excel = library.eq_ignore_ascii_case("Excel");
        let app_objects = if is_excel {
            vec![TypeLibAppObject {
                coclass: "Global".into(),
                clsid: GLOBAL_GUID_TEXT.into(),
                creatable: false,
            }]
        } else {
            vec![]
        };
        let Some(requested) = request.requested_coclass.as_deref() else {
            return Some(TypeLibMetadataBlob {
                constants: if is_excel {
                    excel_constants()
                } else {
                    office_constants()
                },
                identity: Self::library_identity(descriptor, library, &request.reference_name, None),
                activation_prog_id: None,
                member_name_to_token: vec![],
                members: vec![],
                events: vec![],
                coclass_names,
                enum_type_names: Self::enum_type_names(descriptor),
                coclass_prog_ids: vec![],
                app_objects,
                extensible: false,
            });
        };

        if is_excel && requested.eq_ignore_ascii_case("Global") {
            let app_id = descriptor.default_interface(descriptor.id("Application")?);
            let global_iid = bytes_to_com_iid(global_iid_bytes());
            let mut members = Vec::new();
            let mut seen = HashSet::new();
            for member in descriptor.members(app_id) {
                if member.dispid == 0 {
                    continue;
                }
                if !self.has_workbook && member.name.eq_ignore_ascii_case("ThisWorkbook") {
                    continue;
                }
                if !seen.insert((member.dispid, member.invoke)) {
                    continue;
                }
                let mut meta = member_to_metadata(descriptor, app_id, member);
                meta.interface_iid = Some(global_iid);
                meta.is_dual = false;
                meta.source_typekind = Some(SourceTypeKind::Dispatch);
                meta.vtable_slot = None;
                meta.vtable_slot_bound = Some(7);
                members.push(meta);
            }
            members.extend(application_alias_metadata(descriptor, Some(global_iid)));
            let member_name_to_token = unique_name_tokens(&members);
            return Some(TypeLibMetadataBlob {
                constants: vec![],
                identity: Self::library_identity(
                    descriptor,
                    library,
                    &request.reference_name,
                    Some("Global"),
                ),
                activation_prog_id: None,
                member_name_to_token,
                members,
                events: vec![],
                coclass_names,
                coclass_prog_ids: vec![],
                enum_type_names: Vec::new(),
                app_objects,
                extensible: false,
            });
        }

        let id = descriptor.id(requested)?;
        if !descriptor.library(id).name.eq_ignore_ascii_case(library) {
            return None;
        }
        let requested_ty = descriptor.ty(id);
        let iface_id = descriptor.default_interface(id);
        let mut members = Vec::new();
        let mut seen = HashSet::new();
        for member in descriptor.members(iface_id) {
            if !seen.insert((member.dispid, member.invoke)) {
                continue;
            }
            members.push(member_to_metadata(descriptor, iface_id, member));
        }
        if descriptor.id("Application").map(|app| descriptor.default_interface(app)) == Some(iface_id) {
            let iid = Guid::parse(&descriptor.ty(iface_id).guid).map(guid_to_com_iid);
            members.extend(application_alias_metadata(descriptor, iid));
        }
        let events = coclass_events(descriptor, requested_ty, self.events);
        let member_name_to_token = unique_name_tokens(&members);
        Some(TypeLibMetadataBlob {
            constants: vec![],
            identity: Self::library_identity(
                descriptor,
                library,
                &request.reference_name,
                Some(&requested_ty.name),
            ),
            activation_prog_id: None,
            member_name_to_token,
            members,
            events,
            coclass_names,
            enum_type_names: Vec::new(),
            coclass_prog_ids: vec![],
            app_objects,
            extensible: false,
        })
    }
}

impl TypeLibResolver for DescriptorTypeLibResolver {
    fn interface_iid(
        &self,
        request: &TypeLibResolveRequest,
        _blob: &TypeLibMetadataBlob,
    ) -> Option<ComInterfaceIid> {
        match Self::match_library(request)? {
            "DnaCalcObjects" => request
                .requested_coclass
                .as_deref()
                .filter(|name| name.eq_ignore_ascii_case("Range"))
                .map(|_| typed_range_com_iid()),
            library => {
                let requested = request.requested_coclass.as_deref()?;
                if library.eq_ignore_ascii_case("Excel") && requested.eq_ignore_ascii_case("Global")
                {
                    return Some(bytes_to_com_iid(global_iid_bytes()));
                }
                let descriptor = Descriptor::get();
                let id = descriptor.id(requested)?;
                if !descriptor.library(id).name.eq_ignore_ascii_case(library) {
                    return None;
                }
                let iface = descriptor.default_interface(id);
                Guid::parse(&descriptor.ty(iface).guid).map(guid_to_com_iid)
            }
        }
    }

    fn resolve(&self, request: &TypeLibResolveRequest) -> Option<TypeLibMetadataBlob> {
        match Self::match_library(request)? {
            "DnaCalcObjects" => Self::resolve_dnacalc_objects(request),
            library => self.resolve_descriptor_library(library, request),
        }
    }
}

/// First token of the worksheet-function names Excel's `Application` answers though its type
/// library does not declare them (`Application.Match`, `.CountA`, `.Transpose`, dc-711ye).
pub(crate) const APPLICATION_ALIAS_BASE: i32 = 0x4000_0000;

/// The `WorksheetFunction` methods `Application` does not declare itself, in token order: the
/// member at index `i` has token [`APPLICATION_ALIAS_BASE`] + `i`.
pub(crate) fn application_alias_members(descriptor: &Descriptor) -> Vec<&Member> {
    let (Some(app), Some(functions)) = (descriptor.id("Application"), descriptor.id("WorksheetFunction"))
    else {
        return Vec::new();
    };
    let app = descriptor.default_interface(app);
    let functions = descriptor.default_interface(functions);
    let mut seen = HashSet::new();
    descriptor
        .members(functions)
        .into_iter()
        .filter(|m| m.invoke == InvokeKind::Method && m.dispid != 0)
        .filter(|m| descriptor.members(app).iter().all(|a| !a.name.eq_ignore_ascii_case(&m.name)))
        .filter(|m| seen.insert(m.name.to_ascii_lowercase()))
        .collect()
}

/// Blob members for [`application_alias_members`]: late-by-token methods on the Application.
fn application_alias_metadata(
    descriptor: &Descriptor,
    iid: Option<ComInterfaceIid>,
) -> Vec<TypeLibMemberMetadata> {
    let Some(functions) = descriptor.id("WorksheetFunction") else {
        return Vec::new();
    };
    let functions = descriptor.default_interface(functions);
    application_alias_members(descriptor)
        .into_iter()
        .enumerate()
        .map(|(index, member)| {
            let mut meta = member_to_metadata(descriptor, functions, member);
            meta.token = APPLICATION_ALIAS_BASE + index as i32;
            meta.interface_iid = iid;
            meta.is_dual = false;
            meta.source_typekind = Some(SourceTypeKind::Dispatch);
            meta.vtable_slot = None;
            meta.vtable_slot_bound = Some(7);
            meta
        })
        .collect()
}

fn unique_name_tokens(members: &[TypeLibMemberMetadata]) -> Vec<(String, i32)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for m in members {
        if seen.insert(m.name.to_ascii_lowercase()) {
            out.push((m.name.clone(), m.token));
        }
    }
    out
}

fn coclass_events(
    descriptor: &Descriptor,
    ty: &TypeInfo,
    scope: crate::events::DocumentEvents,
) -> Vec<TypeLibEventMetadata> {
    if ty.kind != TypeKind::Coclass {
        return vec![];
    }
    let is_worksheet = ty.name.eq_ignore_ascii_case("Worksheet");
    let is_workbook = ty.name.eq_ignore_ascii_case("Workbook");
    if (is_worksheet && scope != crate::events::DocumentEvents::All)
        || (is_workbook && scope == crate::events::DocumentEvents::None)
    {
        return vec![];
    }
    let mut events = Vec::new();
    for ci in ty.coclass_interfaces.iter().filter(|ci| ci.source) {
        let Some(source_id) = descriptor.id(&ci.name) else {
            continue;
        };
        let source_ty = descriptor.ty(source_id);
        for member in descriptor.members(source_id) {
            if (is_worksheet || is_workbook) && !is_delivered_document_event(&ty.name, &member.name)
            {
                continue;
            }
            let mut param_types = Vec::new();
            let mut param_iids = Vec::new();
            for param in member.arguments() {
                let (pty, _, iid) = map_param_type(descriptor, param);
                param_types.push(pty);
                param_iids.push(iid);
            }
            events.push(TypeLibEventMetadata {
                name: member.name.clone(),
                token: member.dispid,
                callback_arity: param_types.len() as u8,
                parameter_types: param_types,
                parameter_iids: param_iids,
                dispatch_path: TypeLibEventDispatchPath::Dispatch,
                connection_point_iid: Some(source_ty.guid.clone()),
                dispatch_member_id: Some(member.dispid),
                coclass: Some(ty.name.clone()),
            });
        }
    }
    events
}

fn member_to_metadata(
    descriptor: &Descriptor,
    iface_id: InterfaceId,
    member: &Member,
) -> TypeLibMemberMetadata {
    let iface = descriptor.ty(iface_id);
    let iface_iid = Guid::parse(&iface.guid).map(guid_to_com_iid);
    let invoke_kind = match member.invoke {
        InvokeKind::Get => TypeLibMemberInvokeKind::PropertyGet,
        InvokeKind::Put => TypeLibMemberInvokeKind::PropertyPut,
        InvokeKind::Putref => TypeLibMemberInvokeKind::PropertyPutRef,
        InvokeKind::Method => TypeLibMemberInvokeKind::Method,
    };

    let mut parameter_names = Vec::new();
    let mut parameter_optional = Vec::new();
    let mut parameter_optional_defaults = Vec::new();
    let mut parameter_types = Vec::new();
    let mut parameter_wire_types = Vec::new();
    let mut parameter_iids = Vec::new();
    let mut requires_argument = false;

    for (index, param) in member.params.iter().enumerate() {
        if param.has(ParamFlag::Retval) {
            continue;
        }
        if param.has(ParamFlag::Lcid) {
            parameter_names.push(param.name.clone().unwrap_or_else(|| "lcid".into()));
            parameter_optional.push(true);
            parameter_optional_defaults.push(OptionalParamDefault::Lcid);
            parameter_types.push(TypeLibParamType::Long);
            parameter_wire_types.push(TypeLibWireType::Automation(TypeLibParamType::Long));
            parameter_iids.push(None);
            continue;
        }
        let name = param
            .name
            .clone()
            .unwrap_or_else(|| format!("arg{index}"));
        let optional = param.has(ParamFlag::Optional) || param.has(ParamFlag::Hasdefault);
        let (pty, wire, iid) = map_param_type(descriptor, param);
        let default = if let Some(value) = param.default.as_ref().and_then(|v| json_default(v, pty))
        {
            OptionalParamDefault::HasDefault(value)
        } else if optional {
            OptionalParamDefault::OptionalVariant
        } else {
            requires_argument = true;
            OptionalParamDefault::Required
        };
        parameter_names.push(name);
        parameter_optional.push(optional);
        parameter_optional_defaults.push(default);
        parameter_types.push(pty);
        parameter_wire_types.push(wire);
        parameter_iids.push(iid);
    }

    let (return_type, return_wire_type) = map_return_type(descriptor, iface, member);

    TypeLibMemberMetadata {
        name: member.name.clone(),
        token: member.dispid,
        vtable_slot: if iface.dual { member.slot } else { None },
        requires_argument,
        invoke_kind,
        parameter_names,
        parameter_optional,
        parameter_optional_defaults,
        is_default_member: member.dispid == 0,
        parameter_types,
        parameter_wire_types,
        parameter_iids,
        return_type,
        return_wire_type,
        callconv_is_stdcall: true,
        is_dual: iface.dual,
        interface_iid: iface_iid,
        source_typekind: Some(if iface.dual {
            SourceTypeKind::Interface
        } else {
            SourceTypeKind::Dispatch
        }),
        vtable_slot_bound: if iface.dual {
            iface.vtable_slots
        } else {
            Some(7)
        },
    }
}

fn json_default(value: &serde_json::Value, pty: TypeLibParamType) -> Option<ComDefaultValue> {
    match value {
        serde_json::Value::Bool(b) => Some(ComDefaultValue::Bool(*b)),
        serde_json::Value::Number(n) => {
            if matches!(pty, TypeLibParamType::Double | TypeLibParamType::Single) {
                n.as_f64().map(|v| ComDefaultValue::FloatBits(v.to_bits()))
            } else {
                n.as_i64()
                    .map(ComDefaultValue::Int)
                    .or_else(|| n.as_f64().map(|v| ComDefaultValue::FloatBits(v.to_bits())))
            }
        }
        serde_json::Value::String(s) => Some(ComDefaultValue::Str(s.clone())),
        serde_json::Value::Null => None,
        _ => None,
    }
}

fn known_interface_pointer(
    descriptor: &Descriptor,
    name: &str,
) -> Option<(String, Option<ComInterfaceIid>)> {
    if name.eq_ignore_ascii_case("IDispatch") || name.eq_ignore_ascii_case("IUnknown") {
        return None;
    }
    let id = descriptor.id(name)?;
    let lib = descriptor.library(id);
    if !lib.name.eq_ignore_ascii_case("Excel") && !lib.name.eq_ignore_ascii_case("Office") {
        return None;
    }
    let ty = descriptor.ty(id);
    let iface = descriptor.default_interface(id);
    let iid = Guid::parse(&descriptor.ty(iface).guid).map(guid_to_com_iid);
    Some((format!("{}.{}", lib.name, ty.name), iid))
}

fn map_base_type(base: &str, by_ref: bool) -> TypeLibParamType {
    use TypeLibParamType as P;
    match (base, by_ref) {
        ("VARIANT", false) => P::Variant,
        ("VARIANT", true) => P::ByRefVariant,
        ("BSTR", false) => P::String,
        ("BSTR", true) => P::ByRefString,
        ("BOOL", false) => P::Boolean,
        ("BOOL", true) => P::ByRefBoolean,
        ("I4" | "INT" | "UI4" | "UINT" | "ERROR" | "HRESULT", false) => P::Long,
        ("I4" | "INT" | "UI4" | "UINT" | "ERROR" | "HRESULT", true) => P::ByRefLong,
        ("I2" | "UI2", false) => P::Integer,
        ("I2" | "UI2", true) => P::ByRefInteger,
        ("UI1" | "I1", false) => P::Byte,
        ("UI1" | "I1", true) => P::ByRefByte,
        ("I8" | "UI8", false) => P::LongLong,
        ("I8" | "UI8", true) => P::ByRefLongLong,
        ("R4", false) => P::Single,
        ("R4", true) => P::ByRefSingle,
        ("R8", false) => P::Double,
        ("R8", true) => P::ByRefDouble,
        ("DATE", false) => P::Date,
        ("DATE", true) => P::ByRefDate,
        ("CY", false) => P::Currency,
        ("CY", true) => P::ByRefCurrency,
        ("DECIMAL", false) => P::Decimal,
        ("DECIMAL", true) => P::ByRefDecimal,
        (_, false) => P::Variant,
        (_, true) => P::ByRefVariant,
    }
}

fn map_param_type(
    descriptor: &Descriptor,
    param: &Param,
) -> (TypeLibParamType, TypeLibWireType, Option<ComInterfaceIid>) {
    let by_ref = param.has(ParamFlag::Out);
    let underlying = param.type_ref().underlying();
    let effective = if by_ref {
        match underlying {
            TypeRef::Pointer(inner) => *inner,
            other => other,
        }
    } else {
        underlying
    };
    map_type_ref(descriptor, &effective, by_ref)
}

fn map_return_type(
    descriptor: &Descriptor,
    iface: &TypeInfo,
    member: &Member,
) -> (Option<TypeLibParamType>, Option<TypeLibWireType>) {
    if iface.dual {
        let Some(retval) = member.retval() else {
            return (None, None);
        };
        let unwrapped = match retval.type_ref().underlying() {
            TypeRef::Pointer(inner) => *inner,
            other => other,
        };
        let (ty, wire, _) = map_type_ref(descriptor, &unwrapped, false);
        (Some(ty), Some(wire))
    } else {
        let ret = TypeRef::parse(&member.ret).underlying();
        if matches!(&ret, TypeRef::Base(b) if b == "VOID" || b == "HRESULT") {
            return (None, None);
        }
        let (ty, wire, _) = map_type_ref(descriptor, &ret, false);
        (Some(ty), Some(wire))
    }
}

fn map_type_ref(
    descriptor: &Descriptor,
    ty: &TypeRef,
    by_ref: bool,
) -> (TypeLibParamType, TypeLibWireType, Option<ComInterfaceIid>) {
    match ty {
        TypeRef::Base(b) => {
            let p = map_base_type(b, by_ref);
            (p, TypeLibWireType::Automation(p), None)
        }
        TypeRef::Pointer(inner) => match inner.as_ref() {
            TypeRef::Named(name) => {
                if !by_ref
                    && let Some((qualified, iid)) = known_interface_pointer(descriptor, name)
                {
                    (
                        TypeLibParamType::Object,
                        TypeLibWireType::InterfacePointer { name: qualified },
                        iid,
                    )
                } else {
                    let p = if by_ref {
                        TypeLibParamType::ByRefObject
                    } else {
                        TypeLibParamType::Object
                    };
                    (p, TypeLibWireType::Automation(p), None)
                }
            }
            TypeRef::Base(b) => {
                let p = map_base_type(b, by_ref);
                (p, TypeLibWireType::Automation(p), None)
            }
            TypeRef::SafeArray(_) => (
                TypeLibParamType::Variant,
                if by_ref {
                    TypeLibWireType::ByRefSafeArrayVariant
                } else {
                    TypeLibWireType::SafeArrayVariant
                },
                None,
            ),
            other => map_type_ref(descriptor, other, by_ref),
        },
        TypeRef::Named(name) => {
            if !by_ref
                && let Some((qualified, iid)) = known_interface_pointer(descriptor, name)
            {
                (
                    TypeLibParamType::Object,
                    TypeLibWireType::InterfacePointer { name: qualified },
                    iid,
                )
            } else {
                let p = if by_ref {
                    TypeLibParamType::ByRefObject
                } else {
                    TypeLibParamType::Object
                };
                (p, TypeLibWireType::Automation(p), None)
            }
        }
        TypeRef::SafeArray(_) => (
            TypeLibParamType::Variant,
            if by_ref {
                TypeLibWireType::ByRefSafeArrayVariant
            } else {
                TypeLibWireType::SafeArrayVariant
            },
            None,
        ),
        TypeRef::Enum { base, .. } => {
            let p = map_base_type(base, by_ref);
            (p, TypeLibWireType::Automation(p), None)
        }
        TypeRef::Alias { base, .. } => map_type_ref(descriptor, base, by_ref),
        TypeRef::Record(_) => {
            let p = if by_ref {
                TypeLibParamType::ByRefVariant
            } else {
                TypeLibParamType::Variant
            };
            (p, TypeLibWireType::Automation(p), None)
        }
    }
}

/// XlCVError and standard Excel enum constants referenced by VBA projects (pending enum export
/// in `tools/com-descriptor`).
pub(crate) const XL_CV_ERROR: [(&str, i64); 7] = [
    ("xlErrNull", 2000),
    ("xlErrDiv0", 2007),
    ("xlErrValue", 2015),
    ("xlErrRef", 2023),
    ("xlErrName", 2029),
    ("xlErrNum", 2036),
    ("xlErrNA", 2042),
];

const EXCEL_ENUM_CONSTANTS: &[(&str, i64)] = &[
    // XlCalculation
    ("xlCalculationAutomatic", -4105),
    ("xlCalculationManual", -4135),
    ("xlCalculationSemiautomatic", 2),
    // XlReferenceStyle
    ("xlA1", 1),
    ("xlR1C1", -4150),
    // XlListObjectSourceType
    ("xlSrcExternal", 0),
    ("xlSrcRange", 1),
    ("xlSrcXml", 2),
    ("xlSrcQuery", 3),
    ("xlSrcModel", 4),
    // XlYesNoGuess
    ("xlGuess", 0),
    ("xlYes", 1),
    ("xlNo", 2),
    // XlRowCol
    ("xlRows", 1),
    ("xlColumns", 2),
    // XlRangeValueDataType
    ("xlRangeValueDefault", 10),
    ("xlRangeValueXMLSpreadsheet", 11),
    ("xlRangeValueMSPersistXML", 12),
    // Common XlChartType values
    ("xlColumnClustered", 51),
    ("xlColumnStacked", 52),
    ("xlColumnStacked100", 53),
    ("xlBarClustered", 57),
    ("xlBarStacked", 58),
    ("xlLine", 4),
    ("xlLineMarkers", 65),
    ("xlPie", 5),
    ("xlXYScatter", -4169),
    ("xlXYScatterLines", 74),
    ("xlArea", 1),
    // XlFileFormat / XlWBATemplate / XlThreadMode
    ("xlOpenXMLWorkbook", 51),
    ("xlOpenXMLWorkbookMacroEnabled", 52),
    ("xlOpenXMLAddIn", 55),
    ("xlWBATWorksheet", -4167),
    ("xlThreadModeAutomatic", 0),
    ("xlThreadModeManual", 1),
    // Common XlConstants
    ("xlAutomatic", -4105),
    ("xlNone", -4142),
    ("xlSolid", 1),
];

/// Excel enumeration members the host members already implemented take, with the values Microsoft documents
/// for each enumeration (dc-oxh8w.71). The whole set belongs to the typelib export of `tools/com-descriptor`
/// (open: dc-md2j3); a name missing from here resolves nowhere, and in a module without
/// Option Explicit reads as Empty.
const EXCEL_ENUM_CONSTANTS_DOCUMENTED: &[(&str, i64)] = &[
    // dc-cmruz: members the corpus macro projects name (values as Microsoft documents each enumeration)
    // XlOrder, xlTopToBottom / xlLeftToRight
    ("xlTopToBottom", 1),
    ("xlLeftToRight", 2),
    ("xlDownThenOver", 1),
    ("xlOverThenDown", 2),
    // XlBordersIndex diagonals
    ("xlDiagonalDown", 5),
    ("xlDiagonalUp", 6),
    // XlUnderlineStyle
    ("xlUnderlineStyleNone", -4142),
    ("xlUnderlineStyleSingle", 2),
    ("xlUnderlineStyleDouble", -4119),
    ("xlUnderlineStyleSingleAccounting", 4),
    ("xlUnderlineStyleDoubleAccounting", 5),
    // XlThemeColor, XlThemeFont
    ("xlThemeColorDark1", 1),
    ("xlThemeColorLight1", 2),
    ("xlThemeColorDark2", 3),
    ("xlThemeColorLight2", 4),
    ("xlThemeColorAccent1", 5),
    ("xlThemeColorAccent2", 6),
    ("xlThemeColorAccent3", 7),
    ("xlThemeColorAccent4", 8),
    ("xlThemeColorAccent5", 9),
    ("xlThemeColorAccent6", 10),
    ("xlThemeColorHyperlink", 11),
    ("xlThemeColorFollowedHyperlink", 12),
    ("xlThemeFontNone", 0),
    ("xlThemeFontMajor", 1),
    ("xlThemeFontMinor", 2),
    // XlCellType (the rest of the enumeration)
    ("xlCellTypeComments", -4144),
    ("xlCellTypeAllFormatConditions", -4172),
    ("xlCellTypeSameFormatConditions", -4173),
    ("xlCellTypeAllValidation", -4174),
    ("xlCellTypeSameValidation", -4175),
    // XlLinkType
    ("xlLinkTypeExcelLinks", 1),
    ("xlLinkTypeOLELinks", 2),
    // XlSortDataOption, XlSortOn
    ("xlSortNormal", 0),
    ("xlSortTextAsNumbers", 1),
    ("xlSortOnValues", 0),
    ("xlSortOnCellColor", 1),
    ("xlSortOnFontColor", 2),
    ("xlSortOnIcon", 3),
    // XlDVType, XlDVAlertStyle
    ("xlValidateInputOnly", 0),
    ("xlValidateWholeNumber", 1),
    ("xlValidateDecimal", 2),
    ("xlValidateList", 3),
    ("xlValidateDate", 4),
    ("xlValidateTime", 5),
    ("xlValidateTextLength", 6),
    ("xlValidateCustom", 7),
    ("xlValidAlertStop", 1),
    ("xlValidAlertWarning", 2),
    ("xlValidAlertInformation", 3),
    // XlFormatConditionOperator, XlFormatConditionType
    ("xlBetween", 1),
    ("xlNotBetween", 2),
    ("xlEqual", 3),
    ("xlNotEqual", 4),
    ("xlGreater", 5),
    ("xlLess", 6),
    ("xlGreaterEqual", 7),
    ("xlLessEqual", 8),
    ("xlCellValue", 1),
    ("xlExpression", 2),
    // XlTextQualifier, XlColumnDataType (xlSkipColumn)
    ("xlTextQualifierDoubleQuote", 1),
    ("xlTextQualifierSingleQuote", 2),
    ("xlTextQualifierNone", -4142),
    ("xlSkipColumn", 9),
    // XlCellInsertionMode
    ("xlOverwriteCells", 0),
    ("xlInsertDeleteCells", 1),
    ("xlInsertEntireRows", 2),
    // XlPlatform, XlReadingOrder, XlEnableCancelKey, XlMousePointer
    ("xlMacintosh", 1),
    ("xlWindows", 2),
    ("xlMSDOS", 3),
    ("xlContext", -5002),
    ("xlLTR", -5003),
    ("xlRTL", -5004),
    ("xlDisabled", 0),
    ("xlInterrupt", 1),
    ("xlErrorHandler", 2),
    ("xlDefault", -4143),
    ("xlWait", 2),
    // XlBuiltInDialog
    ("xlDialogSaveAs", 5),
    ("xlDialogPrint", 8),
    ("xlDialogPrinterSetup", 9),
    ("xlDialogPrintPreview", 222),
    // XlPrintLocation, XlPrintErrors
    ("xlPrintSheetEnd", 1),
    ("xlPrintInPlace", 16),
    ("xlPrintNoComments", -4142),
    ("xlPrintErrorsDisplayed", 0),
    ("xlPrintErrorsBlank", 1),
    ("xlPrintErrorsDash", 2),
    ("xlPrintErrorsNA", 3),
    // XlConsolidationFunction, XlPivotFieldOrientation, XlPivotTableSourceType, XlPivotTableVersionList
    ("xlSum", -4157),
    ("xlCount", -4112),
    ("xlHidden", 0),
    ("xlRowField", 1),
    ("xlColumnField", 2),
    ("xlPageField", 3),
    ("xlDataField", 4),
    ("xlDatabase", 1),
    ("xlPivotTableVersion10", 1),
    // XlSummaryRow, XlSummaryColumn
    ("xlSummaryAbove", 0),
    ("xlSummaryBelow", 1),
    ("xlSummaryOnLeft", -4131),
    ("xlSummaryOnRight", -4152),
    // XlPlacement, XlCopyPictureFormat, XlPictureAppearance
    ("xlMoveAndSize", 1),
    ("xlMove", 2),
    ("xlFreeFloating", 3),
    ("xlBitmap", 2),
    ("xlPicture", -4147),
    ("xlScreen", 1),
    ("xlPrinter", 2),
    // XlFileFormat (more), XlFixedFormatType
    ("xlCSVUTF8", 62),
    ("xlDBF3", 8),
    ("xlTypePDF", 0),
    ("xlTypeXPS", 1),
    // XlApplicationInternational (separators), XlMarkerStyle, XlChartType, XlTrendlineType, XlDataLabelsShowNone
    ("xlListSeparator", 5),
    ("xlMarkerStyleNone", -4142),
    ("xlMarkerStyleCircle", 8),
    ("xlXYScatterSmooth", 72),
    ("xlLinear", -4132),
    ("xlDataLabelsShowNone", -4142),
    // XlPasteType (more), XlSheetVisibility alias
    ("xlPasteAllMergingConditionalFormats", 14),
    ("xlVeryHidden", 2),
    // XlFormatFromRightOrBelow members of the insert-format enumeration
    ("xlFormatFromLeftOrAbove", 0),
    ("xlFormatFromRightOrBelow", 1),
    // XlFilterAction
    ("xlFilterInPlace", 1),
    ("xlFilterCopy", 2),
    // XlDirection
    ("xlDown", -4121),
    ("xlToLeft", -4159),
    ("xlToRight", -4161),
    ("xlUp", -4162),
    // XlLookAt, XlFindLookIn, XlSearchOrder, XlSearchDirection, XlSearchWithin
    ("xlWhole", 1),
    ("xlPart", 2),
    ("xlFormulas", -4123),
    ("xlValues", -4163),
    ("xlComments", -4144),
    ("xlByRows", 1),
    ("xlByColumns", 2),
    ("xlNext", 1),
    ("xlPrevious", 2),
    ("xlWithinSheet", 1),
    ("xlWithinWorkbook", 2),
    // XlSortOrder, XlSortOrientation, XlSortMethod
    ("xlAscending", 1),
    ("xlDescending", 2),
    ("xlSortColumns", 1),
    ("xlSortRows", 2),
    ("xlPinYin", 1),
    ("xlStroke", 2),
    // XlSheetVisibility, XlSheetType
    ("xlSheetVisible", -1),
    ("xlSheetHidden", 0),
    ("xlSheetVeryHidden", 2),
    ("xlWorksheet", -4167),
    ("xlChart", -4109),
    ("xlDialogSheet", -4116),
    ("xlExcel4MacroSheet", 3),
    ("xlExcel4IntlMacroSheet", 4),
    // XlPasteType, XlPasteSpecialOperation
    ("xlPasteAll", -4104),
    ("xlPasteValues", -4163),
    ("xlPasteFormulas", -4123),
    ("xlPasteFormats", -4122),
    ("xlPasteComments", -4144),
    ("xlPasteValidation", 6),
    ("xlPasteAllExceptBorders", 7),
    ("xlPasteColumnWidths", 8),
    ("xlPasteAllUsingSourceTheme", 13),
    ("xlPasteSpecialOperationNone", -4142),
    ("xlPasteSpecialOperationAdd", 2),
    ("xlPasteSpecialOperationSubtract", 3),
    ("xlPasteSpecialOperationMultiply", 4),
    ("xlPasteSpecialOperationDivide", 5),
    // XlCutCopyMode
    ("xlCopy", 1),
    ("xlCut", 2),
    // XlCellType
    ("xlCellTypeBlanks", 4),
    ("xlCellTypeConstants", 2),
    ("xlCellTypeFormulas", -4123),
    ("xlCellTypeLastCell", 11),
    ("xlCellTypeVisible", 12),
    // XlHAlign, XlVAlign, XlOrientation
    ("xlHAlignGeneral", 1),
    ("xlHAlignCenter", -4108),
    ("xlHAlignLeft", -4131),
    ("xlHAlignRight", -4152),
    ("xlHAlignFill", 5),
    ("xlHAlignJustify", -4130),
    ("xlHAlignDistributed", -4117),
    ("xlHAlignCenterAcrossSelection", 7),
    ("xlVAlignTop", -4160),
    ("xlVAlignCenter", -4108),
    ("xlVAlignBottom", -4107),
    ("xlVAlignJustify", -4130),
    ("xlVAlignDistributed", -4117),
    ("xlHorizontal", -4128),
    ("xlVertical", -4166),
    ("xlUpward", -4171),
    ("xlDownward", -4170),
    // XlBordersIndex, XlLineStyle, XlBorderWeight
    ("xlEdgeLeft", 7),
    ("xlEdgeTop", 8),
    ("xlEdgeBottom", 9),
    ("xlEdgeRight", 10),
    ("xlInsideVertical", 11),
    ("xlInsideHorizontal", 12),
    ("xlContinuous", 1),
    ("xlDash", -4115),
    ("xlDot", -4118),
    ("xlDashDot", 4),
    ("xlDashDotDot", 5),
    ("xlSlantDashDot", 13),
    ("xlDouble", -4119),
    ("xlLineStyleNone", -4142),
    ("xlHairline", 1),
    ("xlThin", 2),
    ("xlMedium", -4138),
    ("xlThick", 4),
    // XlDeleteShiftDirection, XlInsertShiftDirection
    ("xlShiftUp", -4162),
    ("xlShiftToLeft", -4159),
    ("xlShiftDown", -4121),
    ("xlShiftToRight", -4161),
    // XlWindowState, XlWindowView, XlEnableSelection
    ("xlMaximized", -4137),
    ("xlMinimized", -4140),
    ("xlNormal", -4143),
    ("xlNormalView", 1),
    ("xlPageBreakPreview", 2),
    ("xlPageLayoutView", 3),
    ("xlNoRestrictions", 0),
    ("xlUnlockedCells", 1),
    ("xlNoSelection", -4142),
    // XlConstants
    ("xlCenter", -4108),
    ("xlLeft", -4131),
    ("xlRight", -4152),
    ("xlTop", -4160),
    ("xlBottom", -4107),
    ("xlOn", 1),
    ("xlOff", -4146),
    ("xlAll", -4104),
    ("xlManual", -4135),
    ("xlGeneral", 1),
    // XlAutoFillType
    ("xlFillDefault", 0),
    ("xlFillCopy", 1),
    ("xlFillSeries", 2),
    ("xlFillFormats", 3),
    ("xlFillValues", 4),
    ("xlFillDays", 5),
    ("xlFillWeekdays", 6),
    ("xlFillMonths", 7),
    ("xlFillYears", 8),
    ("xlLinearTrend", 9),
    ("xlGrowthTrend", 10),
    ("xlFlashFill", 11),
    // XlAutoFilterOperator
    ("xlAnd", 1),
    ("xlOr", 2),
    ("xlTop10Items", 3),
    ("xlBottom10Items", 4),
    ("xlTop10Percent", 5),
    ("xlBottom10Percent", 6),
    ("xlFilterValues", 7),
    // XlPageOrientation, XlPaperSize
    ("xlPortrait", 1),
    ("xlLandscape", 2),
    ("xlPaperLetter", 1),
    ("xlPaperLegal", 5),
    ("xlPaperA4", 9),
    // XlReferenceType
    ("xlAbsolute", 1),
    ("xlAbsRowRelColumn", 2),
    ("xlRelRowAbsColumn", 3),
    ("xlRelative", 4),
    // XlCalculationState, XlFileAccess, XlTextParsingType, XlColumnDataType
    ("xlDone", 0),
    ("xlCalculating", 1),
    ("xlPending", 2),
    ("xlReadWrite", 2),
    ("xlReadOnly", 3),
    ("xlDelimited", 1),
    ("xlFixedWidth", 2),
    ("xlGeneralFormat", 1),
    ("xlTextFormat", 2),
    // XlFileFormat
    ("xlWorkbookNormal", -4143),
    ("xlCSV", 6),
    ("xlTextWindows", 20),
    ("xlTextMSDOS", 21),
    ("xlUnicodeText", 42),
    ("xlHtml", 44),
    ("xlExcel12", 50),
    ("xlWorkbookDefault", 51),
    ("xlOpenXMLTemplate", 54),
    ("xlExcel8", 56),
    ("xlCurrentPlatformText", -4158),
    // XlUpdateLinks, XlDisplayBlanksAs, XlPageBreak
    ("xlUpdateLinksUserSetting", 1),
    ("xlUpdateLinksNever", 2),
    ("xlUpdateLinksAlways", 3),
    ("xlNotPlotted", 1),
    ("xlZero", 2),
    ("xlInterpolated", 3),
    ("xlPageBreakManual", -4135),
    ("xlPageBreakAutomatic", -4105),
    ("xlPageBreakNone", -4142),
    // XlAxisType, XlAxisGroup, XlLegendPosition
    ("xlCategory", 1),
    ("xlValue", 2),
    ("xlSeriesAxis", 3),
    ("xlPrimary", 1),
    ("xlSecondary", 2),
    ("xlLegendPositionBottom", -4107),
    ("xlLegendPositionCorner", 2),
    ("xlLegendPositionLeft", -4131),
    ("xlLegendPositionRight", -4152),
    ("xlLegendPositionTop", -4160),
];

const OFFICE_ENUM_CONSTANTS: &[(&str, i64)] = &[
    // MsoTriState
    ("msoTrue", -1),
    ("msoFalse", 0),
    ("msoCTrue", 1),
    ("msoTriStateToggle", -3),
    ("msoTriStateMixed", -2),
    // MsoAutoShapeType
    ("msoShapeRectangle", 1),
    ("msoShapeRoundedRectangle", 5),
    ("msoShapeOval", 9),
    ("msoShapeDiamond", 4),
    ("msoShapeIsoscelesTriangle", 7),
    ("msoShapeRightArrow", 33),
    // MsoShapeType
    ("msoAutoShape", 1),
    ("msoChart", 3),
    ("msoLine", 9),
    ("msoPicture", 13),
    ("msoTextBox", 17),
    // MsoConnectorType
    ("msoConnectorStraight", 1),
    ("msoConnectorElbow", 2),
    ("msoConnectorCurve", 3),
    // MsoTextOrientation
    ("msoTextOrientationHorizontal", 1),
    ("msoTextOrientationUpward", 2),
    ("msoTextOrientationDownward", 3),
    ("msoTextOrientationVerticalFarEast", 4),
    ("msoTextOrientationVertical", 5),
    // MsoAutomationSecurity
    ("msoAutomationSecurityLow", 1),
    ("msoAutomationSecurityByUI", 2),
    ("msoAutomationSecurityForceDisable", 3),
];

fn excel_constants() -> Vec<TypeLibConstantMetadata> {
    XL_CV_ERROR
        .iter()
        .chain(EXCEL_ENUM_CONSTANTS.iter())
        .chain(EXCEL_ENUM_CONSTANTS_DOCUMENTED.iter())
        .map(|&(name, value)| TypeLibConstantMetadata {
            owner: "Excel".into(),
            name: name.into(),
            value: ComDefaultValue::Int(value),
        })
        .collect()
}

fn office_constants() -> Vec<TypeLibConstantMetadata> {
    OFFICE_ENUM_CONSTANTS
        .iter()
        .map(|&(name, value)| TypeLibConstantMetadata {
            owner: "Office".into(),
            name: name.into(),
            value: ComDefaultValue::Int(value),
        })
        .collect()
}
