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
