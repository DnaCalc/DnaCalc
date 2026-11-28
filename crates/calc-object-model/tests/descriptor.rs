//! The committed descriptor holds the facts the COM projection measured from Excel 16.0.20527.

use calc_object_model::descriptor::{Descriptor, InvokeKind, ParamFlag, TypeKind};

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn application_slots_and_dispids_match_the_measured_layout() {
    let d = Descriptor::get();
    let app = d.default_interface(d.id("Application").expect("Application coclass"));
    assert_eq!(d.ty(app).name, "_Application");
    assert!(d.ty(app).dual);
    assert_eq!(d.ty(app).vtable_slots, Some(474));
    let get = |name: &str| d.member(app, name, InvokeKind::Get).unwrap_or_else(|| panic!("{name}"));
    assert_eq!((get("Range").dispid, get("Range").slot), (0xC5, Some(40)));
    assert_eq!(get("Caller").dispid, 0x13D);
    assert_eq!(get("Caller").slot, Some(74));
    assert!(get("Caller").takes_lcid());
    assert_eq!(get("Version").slot, Some(281));
    assert_eq!(get("Hwnd").slot, Some(351));
    let run = d.member(app, "Run", InvokeKind::Method).expect("Run");
    assert_eq!(run.slot, Some(42));
    assert_eq!(run.arguments().count(), 31);
    let union = d.member(app, "Union", InvokeKind::Method).expect("Union");
    assert_eq!((union.dispid, union.slot), (0x30B, Some(50)));
    assert!(union.retval().is_some_and(|p| p.has(ParamFlag::Retval)));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn range_and_events_are_dispinterfaces() {
    let d = Descriptor::get();
    let range = d.id("Range").expect("Range");
    assert!(d.ty(range).dispatch_only);
    assert_eq!(d.ty(range).vtable_slots, Some(7));
    assert_eq!(d.member(range, "Value2", InvokeKind::Get).map(|m| m.dispid), Some(0x56C));
    let events = d.id("AppEvents").expect("AppEvents");
    assert_eq!(d.ty(events).kind, TypeKind::Dispatch);
    assert_eq!(d.dispid_of(events, "SheetChange"), Some(0x61C));
    assert_eq!(d.dispid_of(events, "AfterCalculate"), Some(0xA34));
    for (name, slots) in [("_Workbook", 289), ("Workbooks", 26), ("_Worksheet", 158)] {
        assert_eq!(d.ty(d.id(name).expect(name)).vtable_slots, Some(slots), "{name}");
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn office_types_resolve_by_qualified_name_and_through_their_bases() {
    let d = Descriptor::get();
    let addins = d.id("Office.COMAddIns").expect("COMAddIns by qualified name");
    assert_eq!(d.ty(addins).name, "COMAddIns");
    let popup = d.id("CommandBarPopup").expect("CommandBarPopup");
    // Base members (CommandBarControl, then _IMsoOleAccDispObj) come first.
    assert!(d.members(popup).iter().any(|m| m.name == "Caption"));
    assert!(d.id("_IDTExtensibility2").is_some());
    // _IMsoOleAccDispObj (30 slots) extends IAccessible (28): its layout is complete.
    let acc = d.id("IAccessible").expect("IAccessible");
    assert_eq!(d.ty(acc).vtable_slots, Some(28));
    let ole = d.id("_IMsoOleAccDispObj").expect("_IMsoOleAccDispObj");
    assert_eq!(d.ty(ole).base.as_deref(), Some("IAccessible"));
    assert_eq!(d.ty(ole).vtable_slots, Some(30));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn descriptor_reach_overlay_resolves_every_entry() {
    let descriptor = Descriptor::get();
    let overlay: serde_json::Value =
        serde_json::from_slice(include_bytes!("../descriptor/reach.json")).unwrap();
    let entries = overlay["members"].as_object().unwrap();
    for ((interface, name, kind), capabilities) in [
        (("Workbooks", "Open", InvokeKind::Method), &["fs.read", "network"][..]),
        (("_Workbook", "SaveAs", InvokeKind::Method), &["fs.write", "network"][..]),
        (("_Workbook", "FollowHyperlink", InvokeKind::Method), &["network", "process"][..]),
        (("_Application", "ExecuteExcel4Macro", InvokeKind::Method), &["process", "ffi", "fs.read", "fs.write", "network"][..]),
        (("_Application", "RegisterXLL", InvokeKind::Method), &["fs.read", "ffi"][..]),
        (("_Application", "Run", InvokeKind::Method), &["fs.read", "ffi"][..]),
        (("_Application", "VBE", InvokeKind::Get), &["project.write"][..]),
        (("_Workbook", "VBProject", InvokeKind::Get), &["project.write"][..]),
    ] {
        let id = descriptor.id(interface).unwrap();
        let member = descriptor.member(id, name, kind).unwrap();
        assert_eq!(
            member.reach.iter().map(|reach| reach.key()).collect::<Vec<_>>(),
            capabilities,
        );
    }
    for (key, expected) in entries {
        let (member, kind) = key.strip_suffix(')').unwrap().rsplit_once(" (").unwrap();
        let (interface, name) = member.split_once('.').unwrap();
        let kind = match kind {
            "Get" => InvokeKind::Get,
            "Put" => InvokeKind::Put,
            "Putref" => InvokeKind::Putref,
            "Method" => InvokeKind::Method,
            _ => panic!("unknown invoke kind in {key}"),
        };
        let id = descriptor.id(interface).unwrap_or_else(|| panic!("{key}"));
        let described = descriptor.member(id, name, kind).unwrap_or_else(|| panic!("{key}"));
        assert_eq!(
            described.reach.iter().map(|reach| reach.key()).collect::<Vec<_>>(),
            serde_json::from_value::<Vec<String>>(expected.clone()).unwrap(),
            "{key}",
        );
    }
    assert_eq!(descriptor.xlc_reach("xlcExec")[0].key(), "process");
    assert!(descriptor.xlc_reach("xlcCalculateNow").is_empty());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_9x1fr_c_api_commands_declare_reach_and_needs() {
    use calc_object_model::descriptor::{Capability, Need};
    let descriptor = Descriptor::get();
    assert_eq!(descriptor.xlc_needs("XLCEXEC"), &[Need::Process]);
    assert_eq!(descriptor.xlc_needs("xlcSendKeys"), &[Need::Keys]);
    assert_eq!(descriptor.xlc_reach("xlcPrint"), &[Capability::Print]);
    assert_eq!(descriptor.xlc_needs("xlcFileDelete"), &[Need::FileTree, Need::WindowsPath]);
    assert!(descriptor.xlc_needs("xlcCalculateNow").is_empty());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_9x1fr_c_api_functions_distinguish_path_and_locale_items() {
    use calc_object_model::descriptor::{Capability, Need};
    let descriptor = Descriptor::get();
    assert_eq!(descriptor.xlf_needs("xlfRegisterId"), &[Need::Dll]);
    assert_eq!(descriptor.xlf_reach("XLFREGISTERID"), &[Capability::Ffi]);
    assert_eq!(descriptor.xlf_reach("xlfExec"), &[Capability::Process]);
    assert_eq!(descriptor.xlf_needs("xlfFopen"), &[Need::FileTree, Need::WindowsPath]);
    assert_eq!(descriptor.xlf_needs("xlfGetWorkspace(23)"), &[Need::WindowsPath]);
    assert_eq!(descriptor.xlf_needs("xlfGetWorkspace(37)"), &[Need::Locale]);
    assert_eq!(descriptor.xlf_needs("xlfGetDocument(2)"), &[Need::WindowsPath]);
    assert!(descriptor.xlf_needs("xlfGetDocument(1)").is_empty());
}
