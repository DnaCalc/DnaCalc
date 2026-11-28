use calc_skin::{Coverage, Provenance, RawValue, Shown, Stamp};
use dna_ext_sdk::{
    Bindings, ExtensionError, Intent, Output, Publisher, RenderTree, SlotValue, verify,
};
use ed25519_dalek::{Signer, SigningKey};
use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;

const MANIFEST: &[u8] = br#"{"format":"dna.component/1","id":"test.readout","publisher":"Test fixture","version":"1.0.0","read_slots":{"input":"value"}}"#;

const MODULE: &str = r#"(module
    (memory (export "memory") 1 2)
    (func (export "dna_alloc") (param i32) (result i32) i32.const 8)
    (func (export "dna_render") (param i32 i32) (result i32) i32.const 0)
    (func (export "dna_output_len") (result i32) i32.const 1)
)"#;

fn archive(manifest: &[u8], module: &[u8], signed_module: &[u8]) -> (Vec<u8>, [u8; 32]) {
    // The fixture's signing key is public test data.
    let key = SigningKey::from_bytes(&[42; 32]);
    let mut message = b"DNAEXT1\0".to_vec();
    message.extend((manifest.len() as u32).to_le_bytes());
    message.extend(manifest);
    message.extend(signed_module);
    let signature = key.sign(&message).to_bytes();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in [
        ("extension.json", manifest),
        ("module.wasm", module),
        ("extension.sig", signature.as_slice()),
    ] {
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    (
        writer.finish().unwrap().into_inner(),
        key.verifying_key().to_bytes(),
    )
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_review_names_the_verified_publisher_version_and_slots() {
    let wasm = wat::parse_str(MODULE).unwrap();
    let (bytes, key) = archive(MANIFEST, &wasm, &wasm);
    let extension = verify(
        &bytes,
        &[Publisher {
            name: "Test fixture",
            key,
        }],
    )
    .unwrap();
    assert_eq!(extension.manifest().id, "test.readout");
    assert_eq!(extension.manifest().version, "1.0.0");
    assert_eq!(extension.manifest().publisher, "Test fixture");
    assert_eq!(
        extension.manifest().read_slots["input"],
        dna_ext_sdk::SlotKind::Value
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn modified_module_and_unknown_publisher_are_refused() {
    let wasm = wat::parse_str(MODULE).unwrap();
    let (bytes, key) = archive(MANIFEST, b"changed", &wasm);
    assert_eq!(verify(&bytes, &[]).unwrap_err(), ExtensionError::Publisher);
    assert_eq!(
        verify(
            &bytes,
            &[Publisher {
                name: "Test fixture",
                key
            }]
        )
        .unwrap_err(),
        ExtensionError::Signature,
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn an_import_or_unbounded_memory_is_refused_even_with_a_valid_signature() {
    for source in [
        MODULE.replace("(module", "(module (import \"host\" \"callback\" (func))"),
        MODULE.replace("1 2", "1"),
    ] {
        let wasm = wat::parse_str(source).unwrap();
        let (bytes, key) = archive(MANIFEST, &wasm, &wasm);
        assert!(matches!(
            verify(
                &bytes,
                &[Publisher {
                    name: "Test fixture",
                    key
                }]
            ),
            Err(ExtensionError::Module(_)),
        ));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bindings_preserve_provenance_and_intents_cannot_escape_the_manifest() {
    let wasm = wat::parse_str(MODULE).unwrap();
    let (bytes, key) = archive(MANIFEST, &wasm, &wasm);
    let installed = verify(
        &bytes,
        &[Publisher {
            name: "Test fixture",
            key,
        }],
    )
    .unwrap()
    .approve();
    let slot = Shown {
        value: SlotValue::Value(RawValue::Blank),
        provenance: Provenance::AsSaved,
    };
    let mut bindings = Bindings {
        stamp: Stamp {
            workbook: 1.into(),
            model: 2.into(),
            view: 3.into(),
            calc: 4.into(),
        },
        coverage: Coverage::Complete,
        slots: BTreeMap::from([("input".into(), slot.clone())]),
    };
    let json = installed.encode_bindings(&bindings).unwrap();
    assert_eq!(serde_json::from_str::<Bindings>(&json).unwrap(), bindings);
    bindings.slots.insert("other".into(), slot);
    assert!(matches!(
        installed.encode_bindings(&bindings),
        Err(ExtensionError::Binding(_))
    ));

    let mut output = Output {
        tree: RenderTree::Text("Preview".into()),
        intents: vec![Intent::SelectSlot {
            slot: "input".into(),
        }],
    };
    let bytes = serde_json::to_vec(&output).unwrap();
    assert_eq!(installed.decode_output(&bytes).unwrap(), output);
    output.intents = vec![Intent::WriteSlot {
        slot: "input".into(),
        value: "changed".into(),
    }];
    assert!(matches!(
        installed.decode_output(&serde_json::to_vec(&output).unwrap()),
        Err(ExtensionError::Output(_)),
    ));
}
