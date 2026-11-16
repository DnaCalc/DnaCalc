//! dc-4j0e: legacy Excel workbooks (.xls) open as the SpreadsheetML package they stand for.
//!
//! The fixtures are built in the test (`xls_kit`): an OLE compound file holding BIFF8 records.
//! Each test states what the file holds and what the loaded workbook must show.

mod xls_kit;

use calc_doc::model::value::{IdAllocator, SheetId};
use calc_doc::model::{ModelRoot, SheetVisibility};
use calc_doc::{
    DateSystem, LoadedWorkbook, MemorySource, OpenError, OpenPolicy, PackageRead, legacy_workbook_report,
};
use std::sync::Arc;
use xls_kit::*;

fn source(bytes: &[u8]) -> Arc<dyn PackageRead> {
    Arc::new(MemorySource::new(bytes.to_vec()))
}

fn load(bytes: &[u8]) -> LoadedWorkbook {
    let document = calc_doc::open(source(bytes), &OpenPolicy::default()).expect("the file opens");
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    calc_doc::load(&document, &mut ids).expect("the workbook loads")
}

fn names(model: &ModelRoot) -> Vec<(String, SheetVisibility)> {
    model
        .sheets()
        .iter()
        .map(|s| (s.name().text().to_string_lossy(), s.visibility()))
        .collect()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn sheets_in_order_with_state_and_date_system() {
    let bytes = xls(
        true,
        &[],
        &[
            Sheet::worksheet("First", Vec::new()),
            Sheet {
                state: 1,
                ..Sheet::worksheet("Hidden one", Vec::new())
            },
            Sheet {
                state: 2,
                ..Sheet::worksheet("Très secret ✓", Vec::new())
            },
        ],
    );
    let loaded = load(&bytes);
    assert_eq!(
        names(loaded.model()),
        vec![
            ("First".to_string(), SheetVisibility::Visible),
            ("Hidden one".to_string(), SheetVisibility::Hidden),
            ("Très secret ✓".to_string(), SheetVisibility::VeryHidden),
        ]
    );
    assert_eq!(loaded.date_system(), DateSystem::Date1904);
    let plain = load(&xls(false, &[], &[Sheet::worksheet("S", Vec::new())]));
    assert_eq!(plain.date_system(), DateSystem::Date1900);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_large_workbook_stream_is_read_from_regular_sectors_and_unknown_records_are_counted() {
    // Padding records of an unknown type push the stream past the mini stream cutoff.
    let mut filler = Vec::new();
    for _ in 0..600 {
        filler.extend(record(0x01C1, &[0; 8]));
    }
    let bytes = xls(false, &[], &[Sheet::worksheet("Big", filler)]);
    let report = legacy_workbook_report(source(&bytes), &OpenPolicy::default())
        .unwrap()
        .expect("a legacy workbook");
    assert_eq!(report.biff_version, 8);
    assert_eq!(report.sheets, 1);
    assert_eq!(report.unknown_records.get(&0x01C1), Some(&600));
    assert_eq!(load(&bytes).model().sheets().len(), 1);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_chart_sheet_and_a_macro_sheet_keep_their_kind_and_a_module_sheet_is_not_a_sheet() {
    let bytes = xls(
        false,
        &[],
        &[
            Sheet::worksheet("Data", Vec::new()),
            Sheet {
                kind: 2,
                ..Sheet::worksheet("Chart1", Vec::new())
            },
            Sheet {
                kind: 1,
                ..Sheet::worksheet("Macro1", Vec::new())
            },
            Sheet {
                kind: 6,
                ..Sheet::worksheet("Module1", Vec::new())
            },
        ],
    );
    let loaded = load(&bytes);
    let sheets = loaded.sheets();
    assert_eq!(sheets.len(), 3);
    assert_eq!(sheets[1].preserved_kind(), Some("chart sheet"));
    assert!(sheets[2].is_macro_sheet());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn the_source_bytes_are_kept() {
    let bytes = xls(false, &[], &[Sheet::worksheet("S", Vec::new())]);
    let document = calc_doc::open(source(&bytes), &OpenPolicy::default()).unwrap();
    assert_eq!(document.legacy_source(), Some(&bytes[..]));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn files_that_are_not_read_are_typed_refusals() {
    // A compound file with no Workbook stream.
    let refused = |bytes: &[u8]| {
        match calc_doc::open(source(bytes), &OpenPolicy::default()) {
            Err(OpenError::LegacyWorkbook { stream, detail }) => (stream, detail),
            other => panic!("expected a legacy refusal, got {:?}", other.err()),
        }
    };
    let (_, detail) = refused(&compound(&[("WordDocument", vec![1, 2, 3])]));
    assert!(detail.contains("no Workbook"), "{detail}");
    // BIFF5/7, which dc-4j0e.8 reads.
    let mut stream = record(0x0809, &u16s(&[0x0500, 0x0005, 0x1234, 0x07CC]));
    stream.extend(eof());
    let (name, detail) = refused(&compound(&[("Book", stream)]));
    assert_eq!(name, "Book");
    assert!(detail.contains("BIFF5/7"), "{detail}");
    // FILEPASS: an encrypted workbook, which dc-4j0e.7 reads.
    let encrypted = xls(false, &record(0x002F, &[0, 0, 1, 0, 1, 0]), &[Sheet::worksheet("S", Vec::new())]);
    let (_, detail) = refused(&encrypted);
    assert!(detail.contains("encrypted"), "{detail}");
    // A truncated compound file and a sheet offset into nowhere.
    let good = xls(false, &[], &[Sheet::worksheet("S", Vec::new())]);
    refused(&good[..600]);
    let mut stream = workbook_stream(false, &[], &[Sheet::worksheet("S", Vec::new())]);
    // BOUNDSHEET's offset field: point it one byte into a record.
    let at = stream.windows(2).position(|w| w == [0x85, 0x00]).unwrap();
    stream[at + 4] = stream[at + 4].wrapping_add(1);
    let (_, detail) = refused(&compound(&[("Workbook", stream)]));
    assert!(detail.contains("where no record does"), "{detail}");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_zip_package_and_other_bytes_are_unchanged() {
    // A source that is neither a compound file nor a ZIP archive is still the ZIP refusal.
    let result = calc_doc::open(source(b"plain text, nothing else"), &OpenPolicy::default());
    assert_eq!(result.err(), Some(OpenError::NotAZipArchive));
    assert!(legacy_workbook_report(source(b"PK\x03\x04 and so on"), &OpenPolicy::default()).unwrap().is_none());
}
