use calc_office_js::manifest::{CommandAction, Kind, load, load_xml};

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn classic_xml_resolves_excel_override_resources() {
    let manifest = load(include_str!("fixtures/add-in.xml")).unwrap();
    assert_eq!(manifest.kind, Kind::Xml);
    assert!(manifest.runs_in_excel());
    assert!(manifest.shared_runtime);
    let pane = "https://addin.example/pane?view=a&mode=b";
    assert_eq!(manifest.task_pane_url.as_deref(), Some(pane));
    assert_eq!(manifest.function_file_url.as_deref(), Some(pane));
    assert_eq!(manifest.commands.len(), 2);
    assert_eq!(manifest.commands[0].label, "Review & Edit");
    assert_eq!(
        manifest.commands[0].action,
        CommandAction::ShowTaskpane { url: pane.into() }
    );
    assert_eq!(
        manifest.commands[1].action,
        CommandAction::ExecuteFunction {
            name: "refresh".into()
        }
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn unified_json_binds_ribbon_actions_to_their_own_runtime() {
    let manifest = load(include_str!("fixtures/add-in.json")).unwrap();
    assert_eq!(manifest.kind, Kind::UnifiedJson);
    assert!(manifest.runs_in_excel());
    assert!(!manifest.shared_runtime);
    let pane = "https://addin.example/pane";
    assert_eq!(manifest.task_pane_url.as_deref(), Some(pane));
    assert_eq!(
        manifest.function_file_url.as_deref(),
        Some("https://addin.example/commands.js")
    );
    assert_eq!(manifest.commands.len(), 2);
    assert_eq!(
        manifest.commands[0].action,
        CommandAction::ShowTaskpane { url: pane.into() }
    );
    assert_eq!(
        manifest.commands[1].action,
        CommandAction::ExecuteFunction {
            name: "refresh".into()
        }
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn malformed_classic_xml_is_rejected() {
    assert!(load_xml("<OfficeApp><Id>x</Id></OfficeApp><Other/>").is_err());
    assert!(load_xml("<OfficeApp><Id>x&unknown;</Id></OfficeApp>").is_err());
}
