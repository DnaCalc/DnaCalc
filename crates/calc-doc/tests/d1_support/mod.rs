//! Shared helpers of the dc-ndi (work package D1) tests: packages built from a directory of parts,
//! opened, loaded, edited and saved through calc-doc's own route, and the saved bytes read back.

#![allow(dead_code)]

#[path = "../cdoc_m1_1_support/mod.rs"]
mod zip_support;

use calc_doc::model::value::{CellKey, ExcelText, IdAllocator, SheetId};
use calc_doc::model::{CellInput, FiniteNumber, FormulaText, ModelRoot, OpBatch, WorkbookOp};
use calc_doc::{
    LoadedWorkbook, MemorySource, OpenPolicy, OpenedDocument, PackageRead, SavePolicy, SaveReceipt,
    prepare_edit, prepare_save, write,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use zip_support::{RawEntry, build_zip};

/// The directory of the D1 fixtures.
pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/d1")
}

/// The bytes of a fixture file under `tests/fixtures/d1`.
pub fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(name)).unwrap_or_else(|error| panic!("{name}: {error}"))
}

/// A package of every file under `dir`, each stored under its path relative to `dir`, in path
/// order with `[Content_Types].xml` first.
pub fn package_from_dir(dir: &Path) -> Vec<u8> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).expect("a fixture directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let name = path
                    .strip_prefix(root)
                    .expect("under the root")
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((name, std::fs::read(&path).expect("a part")));
            }
        }
    }
    let mut parts = Vec::new();
    walk(dir, dir, &mut parts);
    parts.sort_by(|a, b| {
        (a.0 != "[Content_Types].xml", &a.0).cmp(&(b.0 != "[Content_Types].xml", &b.0))
    });
    let entries: Vec<RawEntry> = parts
        .iter()
        .map(|(name, bytes)| RawEntry::stored(name, bytes))
        .collect();
    build_zip(&entries)
}

pub fn open(bytes: &[u8]) -> OpenedDocument {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    calc_doc::open(source, &OpenPolicy::default()).expect("the package opens")
}

pub fn load(bytes: &[u8]) -> LoadedWorkbook {
    let document = open(bytes);
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    calc_doc::load(&document, &mut ids).expect("the workbook loads")
}

/// The package saved as opened.
pub fn save_untouched(bytes: &[u8]) -> Vec<u8> {
    let document = open(bytes);
    let plan = prepare_save(&document.snapshot(), &SavePolicy::default()).expect("a plan");
    let mut out = Vec::new();
    write(plan, &mut out).expect("written");
    out
}

/// The revision `operations` make of the loaded revision.
pub fn edited(loaded: &LoadedWorkbook, operations: Vec<WorkbookOp>) -> Arc<ModelRoot> {
    let model = loaded.model();
    prepare_edit(
        model,
        &OpBatch {
            base: model.revision(),
            operations,
        },
    )
    .expect("the edit is admitted")
    .into_model()
}

/// The package saved at `model`, a revision of `loaded`'s model.
pub fn save_model(loaded: &LoadedWorkbook, model: &Arc<ModelRoot>) -> (Vec<u8>, SaveReceipt) {
    let plan = prepare_save(&loaded.snapshot(model), &SavePolicy::default()).expect("a plan");
    let mut out = Vec::new();
    let receipt = write(plan, &mut out).expect("written");
    (out, receipt)
}

/// The decompressed bytes of the part `name` of `package`, read through calc-doc; `None` when the
/// package has no such part.
pub fn part(package: &[u8], name: &str) -> Option<Vec<u8>> {
    let document = open(package);
    document.package().part(name)?;
    Some(document.package().read_part(name).expect("the part reads"))
}

pub fn part_text(package: &[u8], name: &str) -> String {
    String::from_utf8(part(package, name).unwrap_or_else(|| panic!("no part {name}")))
        .expect("UTF-8")
}

/// The ID of the sheet named `name`.
pub fn sheet(loaded: &LoadedWorkbook, name: &str) -> SheetId {
    loaded
        .model()
        .sheets()
        .iter()
        .find(|sheet| String::from_utf16_lossy(sheet.name().text().utf16_code_units()) == name)
        .unwrap_or_else(|| panic!("no sheet {name}"))
        .id()
}

/// The cell at an A1 address (`B3`) of `sheet`.
pub fn at(sheet: SheetId, address: &str) -> CellKey {
    let split = address
        .find(|c: char| c.is_ascii_digit())
        .expect("a row number");
    let (letters, digits) = address.split_at(split);
    let column = letters.bytes().fold(0u32, |n, b| {
        n * 26 + u32::from(b.to_ascii_uppercase() - b'A' + 1)
    });
    CellKey::new(sheet, digits.parse().expect("a row"), column).expect("in the grid")
}

pub fn number(value: f64) -> CellInput {
    CellInput::Number(FiniteNumber::new(value).expect("finite"))
}

pub fn formula(text: &str) -> CellInput {
    CellInput::Formula(
        FormulaText::new(ExcelText::from_utf16_code_units(
            text.encode_utf16().collect(),
        ))
        .expect("a formula"),
    )
}

pub fn formula_text(input: Option<&CellInput>) -> Option<String> {
    match input {
        Some(CellInput::Formula(text)) => {
            Some(String::from_utf16_lossy(text.text().utf16_code_units()))
        }
        _ => None,
    }
}

/// Writes `bytes` to `name` under the directory `DNACALC_D1_SAVE_DIR` names, when it is set, for
/// the Excel reopen script (`tests/fixtures/d1/reopen-in-excel.ps1`) to open.
pub fn keep_for_excel(name: &str, bytes: &[u8]) {
    if let Ok(dir) = std::env::var("DNACALC_D1_SAVE_DIR") {
        std::fs::create_dir_all(&dir).expect("the save directory");
        std::fs::write(Path::new(&dir).join(name), bytes).expect("written");
    }
}
