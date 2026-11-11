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
pub(super) const APP: [u8; 16] = *b"DnaCalcApp000001";
pub(super) const APP_EVENTS: [u8; 16] = *b"DnaAppEvents0001";
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
pub(crate) struct EventTypes {
    pub(crate) automatic_events: bool,
    /// Only the workbook document's own events (Open, AddinInstall, BeforeSave), with every
    /// other type as a worksheet function sees it: what an add-in's UDFs and its open-time
    /// set-up need, without the sheet events' Range objects.
    pub(crate) workbook_events: bool,
}

/// Whether `iid` is one of the document interfaces (a workbook or a worksheet).
pub(crate) fn is_document_interface(iid: &[u8; 16]) -> bool {
    *iid == BOOK || *iid == SHEET
}

/// A project's own types first; then, for the names of its type-library references, the
/// platform's registered type libraries through CalcVBA's catalogue (a registry read only).
pub(crate) struct WithReferences<'a> {
    pub(crate) own: &'a dyn TypeLibResolver,
    /// The reference names (as code qualifies types with them) the catalogue resolves.
    pub(crate) libraries: &'a [String],
}
impl WithReferences<'_> {
    fn library(&self, request: &TypeLibResolveRequest) -> bool {
        self.libraries
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&request.reference_name))
    }
}
impl TypeLibResolver for WithReferences<'_> {
    fn interface_iid(
        &self,
        request: &TypeLibResolveRequest,
        blob: &TypeLibMetadataBlob,
    ) -> Option<ComInterfaceIid> {
        if self.library(request) {
            oxvba_symbol::CatalogTypeLibResolver.interface_iid(request, blob)
        } else {
            self.own.interface_iid(request, blob)
        }
    }
    fn resolve(&self, request: &TypeLibResolveRequest) -> Option<TypeLibMetadataBlob> {
        if self.library(request) {
            oxvba_symbol::CatalogTypeLibResolver.resolve(request)
        } else {
            self.own.resolve(request)
        }
    }
}

// These descriptors allow complete original source to compile. No implementation
// is implied: a reached member without a host implementation must refuse.
fn declaration(
    name: &str,
    token: i32,
    kind: K,
    parameters: &[&str],
    required: usize,
    result: T,
) -> TypeLibMemberMetadata {
    let mut m = member(name, 0, result, RANGE, 0);
    m.token = token;
    m.vtable_slot = None;
    m.vtable_slot_bound = None;
    m.interface_iid = None;
    m.invoke_kind = kind;
    m.requires_argument = required != 0;
    m.parameter_names = parameters.iter().map(|s| (*s).into()).collect();
    m.parameter_optional = (0..parameters.len()).map(|i| i >= required).collect();
    m.parameter_types = vec![T::Variant; parameters.len()];
    m
}

fn application_members(mut members: Vec<TypeLibMemberMetadata>) -> Vec<TypeLibMemberMetadata> {
    let mut range = member("Range", 0, T::Object, APP, 6);
    range.token = 400;
    range.parameter_names = vec!["Cell1".into()];
    range.parameter_types = vec![T::String];
    range.parameter_optional = vec![false];
    range.requires_argument = true;
    members.push(range);
    for m in &mut members {
        let slot = match (m.name.as_str(), m.invoke_kind) {
            ("Range", K::PropertyGet) => 0,
            ("Calculate", K::Method) => 1,
            ("CalculateFull", K::Method) => 2,
            ("CalculateFullRebuild", K::Method) => 3,
            ("EnableEvents", K::PropertyGet) => 4,
            ("EnableEvents", K::PropertyPut) => 5,
            _ => continue,
        };
        m.interface_iid = Some(iid(APP));
        m.vtable_slot = Some(slot);
        m.vtable_slot_bound = Some(6);
        m.parameter_optional.fill(false);
        if slot == 0 {
            m.parameter_types = vec![T::String];
            m.return_type = Some(T::Object);
            m.return_wire_type = Some(oxvba_com::TypeLibWireType::InterfacePointer {
                name: "Excel.Range".into(),
            });
        } else if slot == 4 {
            m.return_type = Some(T::Boolean);
        } else if slot == 5 {
            m.parameter_types = vec![T::Boolean];
        }
    }
    members
}

impl TypeLibResolver for EventTypes {
    fn interface_iid(
        &self,
        request: &TypeLibResolveRequest,
        blob: &TypeLibMetadataBlob,
    ) -> Option<ComInterfaceIid> {
        if self.automatic_events
            && request.reference_name.eq_ignore_ascii_case("DnaCalc")
            && request
                .requested_coclass
                .as_deref()
                .is_none_or(|name| name.eq_ignore_ascii_case("Application"))
        {
            return Some(iid(APP));
        }
        if request.reference_name.eq_ignore_ascii_case("Excel") {
            match request
                .requested_coclass
                .as_deref()
                .unwrap_or("Range")
                .to_ascii_lowercase()
                .as_str()
            {
                "range" if !self.automatic_events => {
                    crate::runtime::spreadsheet::SpreadsheetTypes.interface_iid(request, blob)
                }
                "range" => Some(iid(RANGE)),
                "worksheet" => Some(iid(SHEET)),
                "workbook" => Some(iid(BOOK)),
                "application" => Some(iid(APP)),
                _ => None,
            }
        } else {
            crate::runtime::spreadsheet::SpreadsheetTypes.interface_iid(request, blob)
        }
    }
    fn resolve(&self, request: &TypeLibResolveRequest) -> Option<TypeLibMetadataBlob> {
        if !request.reference_name.eq_ignore_ascii_case("Excel") {
            let mut blob = crate::runtime::spreadsheet::SpreadsheetTypes.resolve(request)?;
            if self.automatic_events
                && request.reference_name.eq_ignore_ascii_case("DnaCalc")
                && request
                    .requested_coclass
                    .as_deref()
                    .is_none_or(|name| name.eq_ignore_ascii_case("Application"))
            {
                blob.members = application_members(blob.members);
            }
            return Some(blob);
        }
        let class = request.requested_coclass.as_deref().unwrap_or("Range");
        let (mut members, mut events) = match class.to_ascii_lowercase().as_str() {
            "application" => (
                crate::runtime::spreadsheet::SpreadsheetTypes
                    .resolve(&TypeLibResolveRequest {
                        reference_name: "DnaCalc".into(),
                        requested_coclass: Some("Application".into()),
                        ..request.clone()
                    })?
                    .members,
                vec![event(
                    "AfterCalculate",
                    1,
                    "Application",
                    APP_EVENTS,
                    vec![],
                    vec![],
                )],
            ),
            "range" if !self.automatic_events => (
                crate::runtime::spreadsheet::SpreadsheetTypes
                    .resolve(request)?
                    .members,
                vec![],
            ),
            "range" => (
                vec![
                    member("Value2", 0, T::Variant, RANGE, 7),
                    member("Row", 1, T::Long, RANGE, 7),
                    member("Column", 2, T::Long, RANGE, 7),
                    member("Count", 3, T::Long, RANGE, 7),
                    member("Address", 4, T::String, RANGE, 7),
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
                vec![
                    event("Open", 2, "Workbook", BOOK_EVENTS, vec![], vec![]),
                    event("AddinInstall", 3, "Workbook", BOOK_EVENTS, vec![], vec![]),
                    event(
                        "BeforeSave",
                        1,
                        "Workbook",
                        BOOK_EVENTS,
                        vec![T::Boolean, T::ByRefBoolean],
                        vec![None, None],
                    ),
                ],
            ),
            _ => return None,
        };
        if self.automatic_events && class.eq_ignore_ascii_case("Application") {
            members = application_members(members);
        }
        if !self.automatic_events
            && !class.eq_ignore_ascii_case("Application")
            && !(self.workbook_events && class.eq_ignore_ascii_case("Workbook"))
        {
            events.clear();
        }
        match class.to_ascii_lowercase().as_str() {
            "range" => {
                if self.automatic_events {
                    let mut put = member("Value2", 5, T::Variant, RANGE, 7);
                    put.invoke_kind = K::PropertyPut;
                    put.parameter_names = vec!["value".into()];
                    put.parameter_types = vec![T::Variant];
                    put.parameter_optional = vec![false];
                    put.requires_argument = true;
                    members.push(put);
                    let mut calculate = member("Calculate", 6, T::Variant, RANGE, 7);
                    calculate.invoke_kind = K::Method;
                    members.push(calculate);
                }
                for (name, kind, args, required) in [
                    (
                        "Address",
                        K::PropertyGet,
                        &[
                            "RowAbsolute",
                            "ColumnAbsolute",
                            "ReferenceStyle",
                            "External",
                            "RelativeTo",
                        ][..],
                        0,
                    ),
                    ("Cells", K::PropertyGet, &["RowIndex", "ColumnIndex"][..], 0),
                    ("HasSpill", K::PropertyGet, &[][..], 0),
                    ("SpillingToRange", K::PropertyGet, &[][..], 0),
                    ("Formula", K::PropertyGet, &[][..], 0),
                    ("FormulaR1C1", K::PropertyGet, &[][..], 0),
                    ("Formula2R1C1", K::PropertyGet, &[][..], 0),
                    ("HasArray", K::PropertyGet, &[][..], 0),
                    ("SavedAsArray", K::PropertyGet, &[][..], 0),
                    ("Formula2", K::PropertyGet, &[][..], 0),
                    ("Formula", K::PropertyPut, &["value"][..], 1),
                    ("FormulaR1C1", K::PropertyPut, &["value"][..], 1),
                    ("Formula2R1C1", K::PropertyPut, &["value"][..], 1),
                    ("Formula2", K::PropertyPut, &["value"][..], 1),
                    // Value2 has a typed put when events are active.
                    ("Value2", K::PropertyPut, &["value"][..], 1),
                    ("ClearContents", K::Method, &[][..], 0),
                    ("Clear", K::Method, &[][..], 0),
                    ("Calculate", K::Method, &[][..], 0),
                    ("Select", K::Method, &[][..], 0),
                    ("CalculateRowMajorOrder", K::Method, &[][..], 0),
                ] {
                    if name == "Address" {
                        if self.automatic_events {
                            continue;
                        }
                        members.retain(|m| m.name != "Address");
                    }
                    // The native no-argument Address slot stays available to event
                    // callbacks; worksheet calls keep the shared projection token.
                    let token = if name == "Address" {
                        5
                    } else {
                        100 + members.len() as i32
                    };
                    if !(self.automatic_events
                        && ((name == "Value2" && kind == K::PropertyPut)
                            || (name == "Calculate" && kind == K::Method)))
                    {
                        members.push(declaration(name, token, kind, args, required, T::Variant));
                    }
                }
            }
            "worksheet" => {
                for (name, args, required) in [
                    ("Range", &["Cell1", "Cell2"][..], 1),
                    ("Cells", &["RowIndex", "ColumnIndex"][..], 0),
                    ("UsedRange", &[][..], 0),
                    ("Rows", &["Index"][..], 0),
                    ("EnableCalculation", &[][..], 0),
                    ("ListObjects", &["Index"][..], 0),
                ] {
                    members.push(declaration(
                        name,
                        100 + members.len() as i32,
                        K::PropertyGet,
                        args,
                        required,
                        T::Variant,
                    ));
                }
                members.push(declaration(
                    "Name",
                    110,
                    K::PropertyPut,
                    &["value"],
                    1,
                    T::Variant,
                ));
                members.push(declaration(
                    "EnableCalculation",
                    111,
                    K::PropertyPut,
                    &["value"],
                    1,
                    T::Variant,
                ));
                members.push(declaration("Activate", 112, K::Method, &[], 0, T::Variant));
                members.push(declaration("Calculate", 113, K::Method, &[], 0, T::Variant));
            }
            "workbook" => {
                for (name, args, required) in [
                    ("Worksheets", &["Index"][..], 0),
                    ("Sheets", &["Index"][..], 0),
                    ("Names", &[][..], 0),
                    ("VBProject", &[][..], 0),
                    ("Date1904", &[][..], 0),
                    ("PrecisionAsDisplayed", &[][..], 0),
                    ("ForceFullCalculation", &[][..], 0),
                    ("FullName", &[][..], 0),
                ] {
                    members.push(declaration(
                        name,
                        100 + members.len() as i32,
                        K::PropertyGet,
                        args,
                        required,
                        T::Variant,
                    ));
                }
                members.push(declaration(
                    "SaveAs",
                    120,
                    K::Method,
                    &[
                        "Filename",
                        "FileFormat",
                        "Password",
                        "WriteResPassword",
                        "ReadOnlyRecommended",
                        "CreateBackup",
                        "AccessMode",
                        "ConflictResolution",
                        "AddToMru",
                        "TextCodepage",
                        "TextVisualLayout",
                        "Local",
                    ],
                    1,
                    T::Variant,
                ));
                members.push(declaration(
                    "Close",
                    121,
                    K::Method,
                    &["SaveChanges", "Filename", "RouteWorkbook"],
                    0,
                    T::Variant,
                ));
                members.push(declaration(
                    "SaveCopyAs",
                    122,
                    K::Method,
                    &["Filename"],
                    1,
                    T::Variant,
                ));
                members.push(declaration("Activate", 123, K::Method, &[], 0, T::Variant));
                members.push(declaration(
                    "ForceFullCalculation",
                    124,
                    K::PropertyPut,
                    &["value"],
                    1,
                    T::Variant,
                ));
                members.push(declaration(
                    "IsAddin",
                    125,
                    K::PropertyPut,
                    &["value"],
                    1,
                    T::Variant,
                ));
                members.push(declaration("Save", 126, K::Method, &[], 0, T::Variant));
            }
            _ => {}
        }
        Some(TypeLibMetadataBlob {
            constants: [
                ("xlCalculationManual", -4135),
                ("xlCalculationAutomatic", -4105),
                ("xlCalculationSemiautomatic", 2),
                ("xlOpenXMLWorkbookMacroEnabled", 52),
                ("xlWBATWorksheet", -4167),
                // XlCVError, the values CVErr takes in a worksheet function (Excel's type
                // library).
                ("xlErrNull", 2000),
                ("xlErrDiv0", 2007),
                ("xlErrValue", 2015),
                ("xlErrRef", 2023),
                ("xlErrName", 2029),
                ("xlErrNum", 2036),
                ("xlErrNA", 2042),
                ("xlYes", 1),
                ("xlSrcRange", 1),
                ("xlThreadModeManual", 1),
                ("xlOpenXMLWorkbook", 51),
                ("xlOpenXMLAddIn", 55),
            ]
            .into_iter()
            .map(|(name, value)| oxvba_com::TypeLibConstantMetadata {
                owner: "Excel".into(),
                name: name.into(),
                value: oxvba_com::ComDefaultValue::Int(value),
            })
            .collect(),
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
            coclass_names: ["Range", "Worksheet", "Workbook", "Application"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            coclass_prog_ids: vec![],
        })
    }
}
