//! dc-ndi (D1): the host's round trip over a workbook Excel saved with dynamic arrays, legacy
//! arrays, names and a table (calc-doc's `tests/fixtures/cdoc_m1_2/d1.saved.xlsx`): open, edit,
//! recalculate, save with the calculated values as cached values, and the cells that read a
//! removed array's area recalculated.

#![cfg(not(target_arch = "wasm32"))]

use calc_host::value::{CalcValue, CellKey, SheetId};
use calc_host::{
    CellEdit, CellInput, FiniteNumber, FormulaText, HostCommand, HostOptions, HostState,
    MemorySource, NotModeled, OpenPolicy, PackageRead, ProjectedValue, ProjectionRequest,
    SavePolicy, Unobserved, WorkBudget, dispatch, project, save, step,
};
use std::sync::Arc;

fn fixture() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../calc-doc/tests/fixtures/cdoc_m1_2/d1.saved.xlsx");
    std::fs::read(path).expect("the D1 fixture")
}

fn open(bytes: &[u8]) -> HostState {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    HostState::open(source, &OpenPolicy::default(), HostOptions::default()).expect("it opens")
}

fn sheet(host: &HostState, name: &str) -> SheetId {
    host.sheets()
        .iter()
        .find(|sheet| sheet.name().text().to_string_lossy() == name)
        .expect("the sheet")
        .id()
}

fn at(sheet: SheetId, address: &str) -> CellKey {
    let split = address.find(|c: char| c.is_ascii_digit()).unwrap();
    let (letters, digits) = address.split_at(split);
    let column = letters
        .bytes()
        .fold(0u32, |n, b| n * 26 + u32::from(b - b'A' + 1));
    CellKey::new(sheet, digits.parse().unwrap(), column).unwrap()
}

fn value(host: &HostState, at: CellKey) -> ProjectedValue {
    project(host, &ProjectionRequest { cells: vec![at] })
        .unwrap()
        .cells()[0]
        .value()
        .clone()
}

fn calculated(host: &HostState, at: CellKey) -> Option<CalcValue> {
    match value(host, at) {
        ProjectedValue::Calculated { value, .. } => Some(value),
        _ => None,
    }
}

fn edit(host: &mut HostState, edits: Vec<CellEdit>) {
    dispatch(
        host,
        HostCommand {
            base: host.revision(),
            edits,
        },
    )
    .expect("the edit commits");
}

fn recalculate(host: &mut HostState) {
    step(host, WorkBudget::UNLIMITED, &mut Unobserved);
}

fn number(value: f64) -> CellInput {
    CellInput::Number(FiniteNumber::new(value).unwrap())
}

fn formula(text: &str) -> CellInput {
    CellInput::Formula(
        FormulaText::new(calc_host::value::ExcelText::from_utf16_code_units(
            text.encode_utf16().collect(),
        ))
        .unwrap(),
    )
}

fn part_text(package: &[u8], name: &str) -> String {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(package.to_vec()));
    let document = calc_doc::open(source, &OpenPolicy::default()).expect("the saved package opens");
    String::from_utf8(document.package().read_part(name).expect("the part")).unwrap()
}

#[test]
fn d1_round_trip__a_workbook_with_arrays_opens_calculates_and_saves_its_values() {
    let mut host = open(&fixture());
    let calc = sheet(&host, "Calc");
    recalculate(&mut host);
    // The rest of the workbook calculates; a spilling array is refused per cell.
    assert_eq!(
        calculated(&host, at(calc, "U1")),
        Some(CalcValue::number(2.0))
    );
    assert_eq!(
        calculated(&host, at(calc, "U3")),
        Some(CalcValue::number(21.0))
    );
    assert!(matches!(
        value(&host, at(calc, "A1")),
        ProjectedValue::NotModeled {
            refusal: NotModeled::ArrayFormula(_),
            ..
        }
    ));
    assert!(matches!(
        value(&host, at(calc, "B2")),
        ProjectedValue::NotModeled { .. }
    ));
    // Edit an input, recalculate, save.
    edit(
        &mut host,
        vec![CellEdit::Set {
            at: at(calc, "U2"),
            input: number(8.0),
        }],
    );
    recalculate(&mut host);
    assert_eq!(
        calculated(&host, at(calc, "U3")),
        Some(CalcValue::number(24.0))
    );
    let mut saved = Vec::new();
    let receipt = save(&host, &SavePolicy::default(), &mut saved).expect("saved");
    assert!(receipt.values_written > 0, "{receipt:?}");
    let xml = part_text(&saved, "xl/worksheets/sheet2.xml");
    // Calculated cells carry the engine's values, a text with t="str"; the refused anchor has
    // none, for Excel to calculate on open, and no full calculation is asked for.
    assert!(
        xml.contains(r#"<c r="U3"><f>U2*3</f><v>24</v></c>"#),
        "{xml}"
    );
    assert!(
        xml.contains(r#"<c r="U1"><f>Data!A1+1</f><v>2</v></c>"#),
        "{xml}"
    );
    assert!(
        xml.contains(r#"<c r="A1" cm="1"><f t="array" ref="A1:B3">_xlfn.SEQUENCE(3,2)</f></c>"#),
        "{xml}"
    );
    assert!(!part_text(&saved, "xl/workbook.xml").contains("fullCalcOnLoad"));
    let reopened = open(&saved);
    assert!(matches!(
        value(&reopened, at(sheet(&reopened, "Calc"), "U3")),
        ProjectedValue::FileCached(_)
    ));
    if let Ok(dir) = std::env::var("DNACALC_D1_SAVE_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            std::path::Path::new(&dir).join("d1_host_round_trip.xlsx"),
            &saved,
        )
        .unwrap();
    }
}

#[test]
fn d1_round_trip__readers_of_a_removed_array_recalculate() {
    let mut host = open(&fixture());
    let calc = sheet(&host, "Calc");
    // Z1 reads B2, a cell the array at A1:B3 fills: not available while the array stands.
    edit(
        &mut host,
        vec![CellEdit::Set {
            at: at(calc, "Z1"),
            input: formula("B2+1"),
        }],
    );
    recalculate(&mut host);
    assert!(matches!(
        value(&host, at(calc, "Z1")),
        ProjectedValue::NotModeled { .. }
    ));
    // Replacing the anchor removes the array: B2 is empty from then on, and Z1 recalculates.
    edit(
        &mut host,
        vec![CellEdit::Set {
            at: at(calc, "A1"),
            input: number(9.0),
        }],
    );
    recalculate(&mut host);
    assert_eq!(
        calculated(&host, at(calc, "Z1")),
        Some(CalcValue::number(1.0))
    );
}
