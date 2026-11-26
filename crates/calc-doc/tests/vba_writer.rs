//! Source-only projects go through CalcDoc's normal package path and CalcVBA's Office reader.
use calc_doc::{
    MemorySource, SourceModule, SourceModuleKind, VbaWriteError, source_vba_package,
    write_vba_project,
};
use calc_vba::{OfficeModuleKind, read_office_project};
use std::sync::Arc;

const PROBE: &str = "Function Probe(x) As Variant\n    Probe = x * 2\nEnd Function\n";

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn source_project_round_trips_standard_and_class_modules() {
    let class = format!("' €\n{}Public Value As Long\n", "' padding\n".repeat(500));
    let input = [
        SourceModule {
            name: "Module1",
            kind: SourceModuleKind::Standard,
            source: PROBE,
        },
        SourceModule {
            name: "Counter",
            kind: SourceModuleKind::Class,
            source: &class,
        },
    ];
    let blob = write_vba_project(&input).unwrap();
    assert_eq!(blob, write_vba_project(&input).unwrap());
    let project = read_office_project(&blob).unwrap();
    assert_eq!(project.name(), "VBAProject");
    assert_eq!(project.code_page(), 1252);
    assert_eq!(project.modules().len(), 2);
    for ((module, input), kind) in project
        .modules()
        .iter()
        .zip(input)
        .zip([OfficeModuleKind::Standard, OfficeModuleKind::Class])
    {
        assert_eq!(module.name, input.name);
        assert_eq!(module.kind, kind);
        assert_eq!(module.source_offset, 0);
        assert!(
            module
                .source
                .starts_with(&format!("Attribute VB_Name = \"{}\"\r\n", input.name))
        );
        assert!(module.source.ends_with(&input.source.replace('\n', "\r\n")));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn source_project_refuses_invalid_identities_and_unencodable_source() {
    let module = SourceModule {
        name: "Module1",
        kind: SourceModuleKind::Standard,
        source: PROBE,
    };
    assert!(matches!(
        write_vba_project(&[
            module,
            SourceModule {
                name: "module1",
                ..module
            }
        ]),
        Err(VbaWriteError::ModuleName(_))
    ));
    for name in [
        "dir",
        "_VBA_PROJECT",
        "A/B",
        "1Module",
        "a very long name that cannot fit in CFB",
    ] {
        assert!(matches!(
            write_vba_project(&[SourceModule { name, ..module }]),
            Err(VbaWriteError::ModuleName(_))
        ));
    }
    assert!(matches!(
        write_vba_project(&[SourceModule {
            source: "' 雪",
            ..module
        }]),
        Err(VbaWriteError::Source(_))
    ));
    assert!(matches!(
        write_vba_project(&[SourceModule {
            source: "  attribute VB_Name = \"Other\"",
            ..module
        }]),
        Err(VbaWriteError::Source(_))
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn source_project_is_embedded_in_a_macro_enabled_workbook() {
    let input = [SourceModule {
        name: "Module1",
        kind: SourceModuleKind::Standard,
        source: PROBE,
    }];
    let bytes = source_vba_package(&input).unwrap();
    assert_eq!(bytes, source_vba_package(&input).unwrap());
    let document = calc_doc::open(Arc::new(MemorySource::new(bytes)), &Default::default()).unwrap();
    let book = calc_doc::load(&document, &mut Default::default()).unwrap();
    assert_eq!(book.spreadsheet_format(), calc_doc::SpreadsheetFormat::Xlsm);
    let part = book.vba_project(&Default::default()).unwrap().unwrap();
    let project = read_office_project(&part.bytes).unwrap();
    assert_eq!(part.part, "xl/vbaProject.bin");
    let modules = project.modules();
    assert_eq!(
        modules
            .iter()
            .filter(|m| m.kind == OfficeModuleKind::Standard)
            .count(),
        1
    );
    assert!(
        modules
            .iter()
            .any(|m| m.name == "Module1" && m.source.contains("Probe = x * 2"))
    );
    assert_eq!(
        book.vba_document_names().unwrap(),
        [
            ("ThisWorkbook".into(), None),
            ("Sheet1".into(), Some(book.sheets()[0].id())),
        ]
    );
    for (name, document) in [
        ("ThisWorkbook", "Excel.Workbook"),
        ("Sheet1", "Excel.Worksheet"),
    ] {
        let module = modules.iter().find(|module| module.name == name).unwrap();
        assert_eq!(module.kind, OfficeModuleKind::Document);
        assert_eq!(module.document.as_deref(), Some(document));
    }
}

/// Native-only: writes the synthetic Excel probe to a coordinator-supplied filesystem path.
#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "set DNACALC_VBA_PROBE to the output .xlsm path"]
fn write_windows_probe() {
    let path = std::env::var_os("DNACALC_VBA_PROBE").expect("set DNACALC_VBA_PROBE");
    let package = source_vba_package(&[SourceModule {
        name: "Module1",
        kind: SourceModuleKind::Standard,
        source: PROBE,
    }])
    .unwrap();
    std::fs::write(path, package).unwrap();
}

/// MS-OVBA 2.4.1.3.1, written independently of the writer.
fn decompress(data: &[u8]) -> Vec<u8> {
    assert_eq!(data[0], 1);
    let (mut out, mut at) = (Vec::new(), 1);
    while at < data.len() {
        let header = u16::from_le_bytes([data[at], data[at + 1]]) as usize;
        let end = at + (header & 0xfff) + 3;
        assert_eq!(header >> 12 & 7, 3);
        if header & 0x8000 == 0 {
            out.extend_from_slice(&data[at + 2..end]);
        } else {
            let (start, mut pos) = (out.len(), at + 2);
            while pos < end {
                let flags = data[pos];
                pos += 1;
                for bit in 0..8 {
                    if pos >= end {
                        break;
                    }
                    if flags >> bit & 1 == 0 {
                        out.push(data[pos]);
                        pos += 1;
                        continue;
                    }
                    let token = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
                    pos += 2;
                    let distance = out.len() - start;
                    let bits = (usize::BITS - (distance - 1).leading_zeros()).max(4) as usize;
                    let length = (token & (0xffff >> bits)) + 3;
                    let offset = (token >> (16 - bits)) + 1;
                    for _ in 0..length {
                        out.push(out[out.len() - offset]);
                    }
                }
            }
        }
        at = end;
    }
    out
}

/// rw14 finding 6: a final chunk of 3642..=4095 bytes was zero-padded to 4096 bytes, so the module
/// source read back with trailing NULs.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn compressed_streams_decompress_to_exactly_their_input() {
    let mut state = 0x2545_f491u32;
    let mut random = |length: usize| -> Vec<u8> {
        (0..length)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                b'a' + (state >> 24) as u8 % 26
            })
            .collect()
    };
    for length in [
        0usize, 1, 7, 8, 9, 3600, 3641, 3642, 3700, 3900, 4000, 4050, 4095, 4096, 4097, 8192, 8200, 12000,
    ] {
        for input in [random(length), vec![b'x'; length], random(length / 2).repeat(2)] {
            let Some(packed) = calc_doc::compress_vba_stream(&input) else {
                assert!(input.len() % 4096 != 0 && length > 3600, "length {length}");
                continue;
            };
            assert_eq!(decompress(&packed), input, "length {length}");
        }
    }
}
