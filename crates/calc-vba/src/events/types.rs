//! DNA nominal event interfaces, independent of installed Excel type libraries.
//! Slot metadata follows OxVba7eb4fdd9 crates/oxvba-embed/src/provider.rs.
use oxvba_com::{
    ComInterfaceIid, TypeLibEventDispatchPath, TypeLibEventMetadata, TypeLibMemberInvokeKind as K,
    TypeLibMemberMetadata, TypeLibMetadataBlob, TypeLibParamType as T, TypeLibResolveRequest,
    TypeLibResolvedIdentity,
};
use oxvba_symbol::TypeLibResolver;

pub(super) const RANGE: [u8; 16] = *b"DnaCalcEvtRange1";
pub(super) const SHEET: [u8; 16] = *b"DnaCalcSheet0001";
pub(super) const BOOK: [u8; 16] = *b"DnaCalcBook00001";
pub(super) const SHEET_EVENTS: [u8; 16] = *b"DnaSheetEvents01";
pub(super) const BOOK_EVENTS: [u8; 16] = *b"DnaBookEvents001";
pub(super) fn iid(v: [u8; 16]) -> ComInterfaceIid {
    ComInterfaceIid {
        data1: u32::from_le_bytes(v[..4].try_into().unwrap()),
        data2: u16::from_le_bytes(v[4..6].try_into().unwrap()),
        data3: u16::from_le_bytes(v[6..8].try_into().unwrap()),
        data4: v[8..].try_into().unwrap(),
    }
}
fn iid_text(v: [u8; 16]) -> String {
    let v = iid(v);
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        v.data1,
        v.data2,
        v.data3,
        v.data4[0],
        v.data4[1],
        v.data4[2],
        v.data4[3],
        v.data4[4],
        v.data4[5],
        v.data4[6],
        v.data4[7]
    )
}
fn member(
    name: &str,
    slot: u16,
    result: T,
    interface: [u8; 16],
    bound: u16,
) -> TypeLibMemberMetadata {
    TypeLibMemberMetadata {
        name: name.into(),
        token: i32::from(slot) + 1,
        vtable_slot: Some(slot),
        requires_argument: false,
        invoke_kind: K::PropertyGet,
        parameter_names: vec![],
        parameter_optional: vec![],
        parameter_optional_defaults: vec![],
        is_default_member: false,
        parameter_types: vec![],
        parameter_wire_types: vec![],
        parameter_iids: vec![],
        return_type: Some(result),
        return_wire_type: None,
        callconv_is_stdcall: false,
        is_dual: false,
        interface_iid: Some(iid(interface)),
        source_typekind: None,
        vtable_slot_bound: Some(bound),
    }
}
fn event(
    name: &str,
    token: i32,
    class: &str,
    interface: [u8; 16],
    parameters: Vec<T>,
    parameter_iids: Vec<Option<ComInterfaceIid>>,
) -> TypeLibEventMetadata {
    TypeLibEventMetadata {
        name: name.into(),
        token,
        callback_arity: parameters.len() as u8,
        parameter_types: parameters,
        parameter_iids,
        dispatch_path: TypeLibEventDispatchPath::Dispatch,
        connection_point_iid: Some(iid_text(interface)),
        dispatch_member_id: Some(token),
        coclass: Some(class.into()),
    }
}
pub(crate) struct EventTypes;
impl TypeLibResolver for EventTypes {
    fn interface_iid(
        &self,
        request: &TypeLibResolveRequest,
        blob: &TypeLibMetadataBlob,
    ) -> Option<ComInterfaceIid> {
        if request.reference_name.eq_ignore_ascii_case("Excel") {
            match request
                .requested_coclass
                .as_deref()
                .unwrap_or("Range")
                .to_ascii_lowercase()
                .as_str()
            {
                "range" => Some(iid(RANGE)),
                "worksheet" => Some(iid(SHEET)),
                "workbook" => Some(iid(BOOK)),
                _ => None,
            }
        } else {
            crate::runtime::spreadsheet::SpreadsheetTypes.interface_iid(request, blob)
        }
    }
    fn resolve(&self, request: &TypeLibResolveRequest) -> Option<TypeLibMetadataBlob> {
        if !request.reference_name.eq_ignore_ascii_case("Excel") {
            return crate::runtime::spreadsheet::SpreadsheetTypes.resolve(request);
        }
        let class = request.requested_coclass.as_deref().unwrap_or("Range");
        let (members, events) = match class.to_ascii_lowercase().as_str() {
            "range" => (
                vec![
                    member("Value2", 0, T::Variant, RANGE, 5),
                    member("Row", 1, T::Long, RANGE, 5),
                    member("Column", 2, T::Long, RANGE, 5),
                    member("Count", 3, T::Long, RANGE, 5),
                    member("Address", 4, T::String, RANGE, 5),
                ],
                vec![],
            ),
            "worksheet" => (
                vec![member("Name", 0, T::String, SHEET, 1)],
                vec![
                    event(
                        "Change",
                        1,
                        "Worksheet",
                        SHEET_EVENTS,
                        vec![T::Object],
                        vec![Some(iid(RANGE))],
                    ),
                    event("Calculate", 2, "Worksheet", SHEET_EVENTS, vec![], vec![]),
                ],
            ),
            "workbook" => (
                vec![member("Name", 0, T::String, BOOK, 1)],
                vec![event(
                    "BeforeSave",
                    1,
                    "Workbook",
                    BOOK_EVENTS,
                    vec![T::Boolean, T::ByRefBoolean],
                    vec![None, None],
                )],
            ),
            _ => return None,
        };
        Some(TypeLibMetadataBlob {
            constants: Vec::new(),
            identity: TypeLibResolvedIdentity {
                reference_name: request.reference_name.clone(),
                requested_coclass: Some(class.into()),
                importlib: "calcvba-host-slots-v1".into(),
                libid: None,
                major_version: 1,
                minor_version: 0,
                lcid: None,
                cache_key: format!("dnacalc-events-v1:{class}"),
            },
            activation_prog_id: None,
            member_name_to_token: members.iter().map(|m| (m.name.clone(), m.token)).collect(),
            members,
            events,
            coclass_names: vec![class.into()],
        })
    }
}
