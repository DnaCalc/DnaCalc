use calc_js_host::{ExecLimits, FunctionsMetadata, JsAddin, QuickJs};
use calc_macro_security::MacroSecurity;

/// Load the module and metadata supplied by the owner of a native Office.js add-in.
///
/// Guarded and Unguarded both use the bounded QuickJS runtime. The embedded face exposes no I/O
/// or workbook mutation bridge. Keep the returned add-in alive while its callables are published.
pub fn load_js_addin(
    policy: MacroSecurity,
    id: &str,
    namespace: &str,
    source: &str,
    metadata_json: &str,
) -> Result<JsAddin, String> {
    if policy == MacroSecurity::Off {
        return Err("JavaScript add-in execution is off".into());
    }
    let metadata = FunctionsMetadata::parse(namespace, metadata_json).map_err(|e| e.to_string())?;
    JsAddin::load(
        Box::new(QuickJs::new(ExecLimits::default())),
        id,
        source,
        &metadata,
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_denies_loading_and_enabled_policies_publish_callables() {
        let load = |policy, source| {
            load_js_addin(
                policy,
                "test",
                "Probe",
                source,
                r#"{"functions":[{"id":"RUN","name":"RUN","parameters":[],"result":{}}]}"#,
            )
        };
        assert_eq!(
            load(
                MacroSecurity::Off,
                "throw new Error('unexpected evaluation')"
            )
            .unwrap_err(),
            "JavaScript add-in execution is off"
        );
        for policy in [MacroSecurity::Guarded, MacroSecurity::Unguarded] {
            let addin = load(policy, "CustomFunctions.associate('RUN', () => 1)").unwrap();
            assert!(addin.callables().contains_for(None, "Probe.RUN"));
        }
    }
}
