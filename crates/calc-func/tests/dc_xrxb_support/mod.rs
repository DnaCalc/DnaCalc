//! The case dc_xrxb__prefix_metadata_matches_excel: the file prefix calc_func gives each catalogued
//! named function ([`FunctionCallTarget::file_prefix`]) against the spelling Excel itself writes.
//!
//! The oracle workbook `tests/fixtures/dc_xrxb/oracle.xlsx` (written by the committed generator
//! `tests/fixtures/make-fixtures.ps1`) calls every catalogued named function once per row of its
//! sheet prefix_metadata: in column A with the prefix `_xlfn.`, in column B without one. Excel's
//! capture of it (`tests/captures/dc_xrxb/oracle.capture.json`, written by `cargo xtask capture`)
//! holds Excel's raw value of each cell and the SHA-256 of the package Excel saved from it, which
//! is kept beside the capture as `tests/captures/dc_xrxb/oracle.saved.xlsx`. The case reads the
//! capture through calc_verdict's CaptureRecord, checks that it is a live capture of the committed
//! workbook by the committed worker and that the saved package is the one it names, and then, for
//! each catalogued named function, compares calc_func's prefix with what Excel wrote and gave:
//! column A saved as the prefix and the call, column B saved as the call when the function has no
//! prefix and as the unknown name `_xludf.` and the call when it has one, and column B's raw value
//! `#NAME?` exactly when it has one. Nothing here is derived from calc_func's own reading of a
//! formula: every expectation is Excel's.

use calc_func::value::{CalcValue, CoreValue, WorksheetErrorCode};
use calc_func::{FilePrefix, FunctionCallTarget, Notation, catalogue};
use calc_verdict::{Acquisition, CaptureRecord, CellObservation, Observation, sha256_hex};
use std::collections::BTreeMap;

const CASE: &str = "dc_xrxb__prefix_metadata_matches_excel";
/// The sheet of each oracle workbook that calls each function.
const SHEET: &str = "prefix_metadata";

/// One oracle workbook of the case, its capture and the package Excel saved from it. dc-xrxb's
/// workbook holds the rows of the functions catalogued before the bulk port; the catalogue
/// oracle (catalogue/oracle.xlsx, written by make-catalogue-fixtures.ps1 in one section per
/// package, F1 dc-8dq's first) holds the rest. A catalogued named function has its row in
/// exactly one.
struct Oracle {
    /// The workbook's directory below tests/fixtures/ (and its capture's below tests/captures/).
    name: &'static str,
    /// The case the capture names.
    case: &'static str,
    /// The workbook, as committed.
    workbook: &'static [u8],
    /// Excel's capture of the workbook.
    capture: &'static [u8],
    /// The package Excel saved from the workbook during that capture, as the capture's work
    /// directory kept it (`out/oracle.xlsx`).
    saved: &'static [u8],
    /// The command that wrote the capture.
    command: &'static str,
}

const ORACLES: [Oracle; 2] = [
    Oracle {
        name: "dc_xrxb",
        case: CASE,
        workbook: include_bytes!("../fixtures/dc_xrxb/oracle.xlsx"),
        capture: include_bytes!("../captures/dc_xrxb/oracle.capture.json"),
        saved: include_bytes!("../captures/dc_xrxb/oracle.saved.xlsx"),
        command: "cargo xtask capture --fixture crates/calc-func/tests/fixtures/dc_xrxb/oracle.xlsx --case dc_xrxb__prefix_metadata_matches_excel --out crates/calc-func/tests/captures/dc_xrxb/oracle.capture.json",
    },
    Oracle {
        name: "catalogue",
        case: "catalogue__oracle",
        workbook: include_bytes!("../fixtures/catalogue/oracle.xlsx"),
        capture: include_bytes!("../captures/catalogue/oracle.capture.json"),
        saved: include_bytes!("../captures/catalogue/oracle.saved.xlsx"),
        command: "cargo xtask capture --fixture crates/calc-func/tests/fixtures/catalogue/oracle.xlsx --case catalogue__oracle --out crates/calc-func/tests/captures/catalogue/oracle.capture.json --work-dir <dir>",
    },
];

/// The parts of a ZIP package by name, each stored or deflated entry as its uncompressed bytes,
/// read through the central directory.
fn package_parts(package: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let u16_at = |at: usize| usize::from(u16::from_le_bytes([package[at], package[at + 1]]));
    let u32_at = |at: usize| {
        u32::from_le_bytes([
            package[at],
            package[at + 1],
            package[at + 2],
            package[at + 3],
        ]) as usize
    };
    let end = (0..=package.len() - 22)
        .rev()
        .find(|&at| u32_at(at) == 0x0605_4b50)
        .expect("the package has an end of central directory record");
    let (entries, mut at) = (u16_at(end + 10), u32_at(end + 16));
    let mut parts = BTreeMap::new();
    for _ in 0..entries {
        assert_eq!(u32_at(at), 0x0201_4b50, "a central directory entry");
        let (method, compressed_size, size) = (u16_at(at + 10), u32_at(at + 20), u32_at(at + 24));
        let (name_length, extra_length, comment_length) =
            (u16_at(at + 28), u16_at(at + 30), u16_at(at + 32));
        let name = String::from_utf8(package[at + 46..at + 46 + name_length].to_vec())
            .expect("a UTF-8 part name");
        let local = u32_at(at + 42);
        assert_eq!(u32_at(local), 0x0403_4b50, "{name}: a local entry");
        let data = local + 30 + u16_at(local + 26) + u16_at(local + 28);
        let compressed = &package[data..data + compressed_size];
        let bytes = match method {
            0 => compressed.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec(compressed)
                .unwrap_or_else(|e| panic!("{name}: the deflated entry does not inflate: {e:?}")),
            other => panic!("{name}: compression method {other}"),
        };
        assert_eq!(bytes.len(), size, "{name}: the uncompressed size");
        assert!(parts.insert(name, bytes).is_none(), "a part repeats");
        at += 46 + name_length + extra_length + comment_length;
    }
    parts
}

fn text<'p>(parts: &'p BTreeMap<String, Vec<u8>>, name: &str) -> &'p str {
    let bytes = parts
        .get(name)
        .unwrap_or_else(|| panic!("the package has no part {name}"));
    std::str::from_utf8(bytes).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The value of attribute `name` in `tag`, the text of a start tag after its element name (or the
/// whole tag): the first `name="` at the start or after a space.
fn attribute<'x>(tag: &'x str, name: &str) -> Option<&'x str> {
    let marker = format!("{name}=\"");
    let start = tag
        .match_indices(&marker)
        .map(|(at, _)| at)
        .find(|&at| at == 0 || tag.as_bytes()[at - 1] == b' ')?
        + marker.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The worksheet part of the sheet named `sheet`, through the workbook part and its relationships.
fn sheet_part<'p>(parts: &'p BTreeMap<String, Vec<u8>>, sheet: &str) -> &'p str {
    let workbook = text(parts, "xl/workbook.xml");
    let tag = workbook
        .split("<sheet ")
        .skip(1)
        .find(|tag| attribute(tag, "name") == Some(sheet))
        .unwrap_or_else(|| panic!("the workbook names no sheet {sheet}"));
    let id = attribute(tag, "r:id").expect("a sheet's relationship");
    let relationships = text(parts, "xl/_rels/workbook.xml.rels");
    let relationship = relationships
        .split("<Relationship ")
        .skip(1)
        .find(|tag| attribute(tag, "Id") == Some(id))
        .unwrap_or_else(|| panic!("no relationship {id}"));
    let target = attribute(relationship, "Target").expect("a target");
    let target = target.strip_prefix("/xl/").unwrap_or(target);
    text(parts, &format!("xl/{target}"))
}

/// The formula text of each cell of a worksheet part that holds a formula, by its A1 reference.
/// Every `<f>` must be an ordinary formula whose only attribute, if any, is `ca` (calculate the
/// cell always, which Excel writes for a call of an unknown name): a shared or array formula here
/// would not spell each cell's call in full.
fn formulas(xml: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for cell in xml.split("<c ").skip(1) {
        let tag_end = cell.find('>').expect("a cell's start tag ends");
        let reference = attribute(&cell[..tag_end], "r").expect("a reference");
        if cell[..tag_end].ends_with('/') {
            continue;
        }
        let body = &cell[tag_end + 1..cell.find("</c>").expect("a cell ends")];
        if let Some(start) = body.find("<f") {
            let open = &body[start..];
            let tag_end = open.find('>').expect("a formula's start tag ends");
            let tag = &open[..tag_end];
            assert!(
                tag == "<f" || tag == "<f ca=\"1\"",
                "{reference}: the formula element is not an ordinary formula: {open}"
            );
            let end = open.find("</f>").expect("a formula ends");
            out.insert(reference.to_owned(), unescape(&open[tag_end + 1..end]));
        }
    }
    out
}

/// Excel's raw value of `column` (1 for A) of `row` on the sheet, from the capture.
fn raw(capture: &CaptureRecord, row: u32, column: u32) -> &CalcValue {
    let observed: Vec<&CellObservation> = capture
        .observations
        .iter()
        .filter(|observed| {
            observed.cell.sheet() == SHEET
                && observed.cell.row() == row
                && observed.cell.column() == column
        })
        .collect();
    match observed[..] {
        [
            CellObservation {
                observation: Observation::ExcelRaw(value),
                ..
            },
        ] => value,
        _ => panic!("{SHEET} row {row} column {column}: not one raw observation: {observed:?}"),
    }
}

fn is_name_error(value: &CalcValue) -> bool {
    matches!(value.core(), CoreValue::Error(WorksheetErrorCode::Name))
}

/// One row of the sheet: its function's spelling and the call as column B writes it, without a
/// prefix, or with `_xlws.` alone for a function a file stores with `_xlfn._xlws.` (FILTER, SORT):
/// Excel refuses to open a package that writes such a function after `_xlfn.` alone (Excel 16.0
/// on this machine, 2026-10-23), so its row writes `_xlfn._xlws.` in column A and `_xlws.` in B.
struct Row {
    row: u32,
    function: String,
    call: String,
}

/// The sheet's rows as the oracle workbook holds them: in row n, column A is `_xlfn.` and the call
/// and column B the call itself.
fn oracle_rows(workbook: &[u8]) -> Vec<Row> {
    let parts = package_parts(workbook);
    let cells = formulas(sheet_part(&parts, SHEET));
    let mut rows = Vec::new();
    for row in 1u32.. {
        let (Some(prefixed), Some(call)) =
            (cells.get(&format!("A{row}")), cells.get(&format!("B{row}")))
        else {
            break;
        };
        assert_eq!(
            prefixed,
            &format!("_xlfn.{call}"),
            "row {row} of the oracle workbook"
        );
        let bare = call.strip_prefix("_xlws.").unwrap_or(call);
        let function = bare[..bare.find('(').expect("a call")].to_owned();
        rows.push(Row {
            row,
            function,
            call: call.clone(),
        });
    }
    assert_eq!(
        cells.len(),
        2 * rows.len(),
        "the oracle sheet holds formulas outside its rows' columns A and B"
    );
    rows
}

/// The catalogue generator, run into a fresh directory: the workbook it writes.
#[cfg(all(windows, not(target_arch = "wasm32")))]
fn catalogue_generated() -> Vec<u8> {
    use std::path::Path;
    use std::process::{Command, Stdio};
    let generator = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("make-catalogue-fixtures.ps1");
    let out = std::env::temp_dir().join(format!("calc-func-catalogue-fixtures-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let system_root = std::env::var_os("SystemRoot").expect("SystemRoot is set on Windows");
    let powershell = Path::new(&system_root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    let ran = Command::new(&powershell)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&generator)
        .arg("-OutDir")
        .arg(&out)
        .stdin(Stdio::null())
        .output()
        .expect("Windows PowerShell starts");
    assert!(
        ran.status.success(),
        "the catalogue generator failed ({:?}):\n{}\n{}",
        ran.status,
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr)
    );
    let bytes = std::fs::read(out.join("catalogue").join("oracle.xlsx")).expect("the catalogue workbook");
    let _ = std::fs::remove_dir_all(&out);
    bytes
}

/// One oracle's capture, checked: a live capture of the committed workbook by the committed
/// worker, with no failure, naming the saved package kept beside it.
fn checked_capture(oracle: &Oracle) -> CaptureRecord {
    let capture = CaptureRecord::decode(oracle.capture).unwrap_or_else(|e| {
        panic!("the capture record does not decode ({}): {e}", oracle.command)
    });
    assert_eq!(capture.case, oracle.case);
    assert_eq!(capture.acquisition, Acquisition::LiveCapture);
    assert_eq!(
        capture.source,
        format!("sha256:{}", sha256_hex(oracle.workbook))
    );
    let session = capture
        .session
        .as_ref()
        .expect("a version 2 capture session");
    assert!(
        session.tool.contains(&format!(
            "worker sha256:{}",
            sha256_hex(crate::CAPTURE_WORKER)
        )),
        "{}: the capture was taken by another worker: {}",
        oracle.name,
        session.tool
    );
    assert!(
        session.failures.is_empty(),
        "{}: capture failures: {:?}",
        oracle.name,
        session.failures
    );
    let saved_hash = format!("sha256:{}", sha256_hex(oracle.saved));
    assert_eq!(
        session.output_package.as_deref(),
        Some(saved_hash.as_str()),
        "{}: the kept package is not the one Excel saved during the capture",
        oracle.name
    );
    let environment = capture.environment.as_ref().expect("Excel's environment");
    note!(
        "ACCEPT-NOTE: {CASE}: capture of {} by Excel {} ({}), worker sha256:{}; Excel saved the package {saved_hash}, kept as tests/captures/{}/oracle.saved.xlsx",
        capture.source,
        environment.target.excel_build(),
        environment.target.channel(),
        sha256_hex(crate::CAPTURE_WORKER),
        oracle.name
    );
    capture
}

pub fn prefix_metadata_matches_excel() {
    // Each committed oracle workbook is what its committed generator writes, byte for byte, and
    // the generator of the earlier workbooks leaves every other workbook as committed.
    #[cfg(all(windows, not(target_arch = "wasm32")))]
    {
        let generated = crate::generated_fixtures();
        let names: Vec<&str> = generated.keys().map(String::as_str).collect();
        assert_eq!(
            names,
            crate::GENERATED_WORKBOOKS,
            "the generator's workbooks"
        );
        assert!(
            generated["dc_xrxb/oracle.xlsx"] == ORACLES[0].workbook,
            "the committed dc_xrxb/oracle.xlsx differs from the generator's output"
        );
        assert!(
            catalogue_generated() == ORACLES[1].workbook,
            "the committed catalogue/oracle.xlsx differs from make-catalogue-fixtures.ps1's output"
        );
        note!(
            "ACCEPT-NOTE: {CASE}: the committed generators wrote {} workbooks and catalogue/oracle.xlsx; dc_xrxb/oracle.xlsx (sha256 {}) and catalogue/oracle.xlsx (sha256 {}) equal the committed fixtures byte for byte",
            generated.len(),
            sha256_hex(ORACLES[0].workbook),
            sha256_hex(ORACLES[1].workbook)
        );
    }

    // Every oracle's capture, saved formulas and rows; each function's row, in whichever oracle
    // holds it.
    let captures: Vec<CaptureRecord> = ORACLES.iter().map(checked_capture).collect();
    let saved: Vec<BTreeMap<String, String>> = ORACLES
        .iter()
        .map(|oracle| formulas(sheet_part(&package_parts(oracle.saved), SHEET)))
        .collect();
    let rows: Vec<Vec<Row>> = ORACLES
        .iter()
        .map(|oracle| oracle_rows(oracle.workbook))
        .collect();
    let mut by_function: BTreeMap<&str, (usize, &Row)> = BTreeMap::new();
    for (index, oracle_rows) in rows.iter().enumerate() {
        for row in oracle_rows {
            assert!(
                by_function
                    .insert(row.function.as_str(), (index, row))
                    .is_none(),
                "{}: a function has two rows",
                row.function
            );
        }
    }

    let named: Vec<FunctionCallTarget<'static>> = catalogue()
        .surfaces()
        .filter(|target| target.notation() == Notation::Call)
        .collect();
    for target in catalogue().surfaces() {
        if target.notation() != Notation::Call {
            assert_eq!(target.file_prefix(), None, "{}", target.spelling());
        }
    }
    let mut problems = Vec::new();
    let mut prefixed = Vec::new();
    let mut bare = 0;
    for target in &named {
        let name = target.spelling();
        let Some(&(oracle, row)) = by_function.get(name) else {
            problems.push(format!(
                "{name}: catalogued, and no oracle workbook has a row calling it"
            ));
            continue;
        };
        let Some(prefix) = target.file_prefix() else {
            problems.push(format!("{name}: catalogued with no file prefix"));
            continue;
        };
        let (capture, saved) = (&captures[oracle], &saved[oracle]);
        let at = row.row;
        let saved_a = saved.get(&format!("A{at}"));
        let saved_b = saved.get(&format!("B{at}"));
        let unprefixed = row.call.strip_prefix("_xlws.").unwrap_or(&row.call);
        if (prefix == FilePrefix::XlfnXlws) != (unprefixed.len() != row.call.len()) {
            problems.push(format!(
                "{name}: prefix {:?}, and its row writes {}",
                prefix.text(),
                row.call
            ));
            continue;
        }
        let expected_a = format!("{}{}", prefix.text(), unprefixed);
        let (expected_b, b_unknown) = match prefix {
            FilePrefix::Bare => (row.call.clone(), false),
            FilePrefix::Xlfn => (format!("_xludf.{}", row.call), true),
            FilePrefix::XlfnXlws => (row.call.clone(), true),
        };
        let (raw_a, raw_b) = (raw(capture, at, 1), raw(capture, at, 2));
        if saved_a != Some(&expected_a) {
            problems.push(format!(
                "{name}: prefix {:?}, and Excel saved _xlfn.{} as {saved_a:?}",
                prefix.text(),
                row.call
            ));
        }
        if saved_b != Some(&expected_b) {
            problems.push(format!(
                "{name}: prefix {:?}, and Excel saved {} as {saved_b:?}",
                prefix.text(),
                row.call
            ));
        }
        if is_name_error(raw_a) {
            problems.push(format!("{name}: Excel gave _xlfn.{} #NAME?", row.call));
        }
        if is_name_error(raw_b) != b_unknown || (!b_unknown && raw_a != raw_b) {
            problems.push(format!(
                "{name}: prefix {:?}, and Excel's load gave {} {raw_b:?} beside _xlfn.{} {raw_a:?}",
                prefix.text(),
                row.call,
                row.call
            ));
        }
        match prefix {
            FilePrefix::Bare => bare += 1,
            FilePrefix::Xlfn | FilePrefix::XlfnXlws => prefixed.push(name),
        }
    }
    let catalogued: Vec<&str> = named.iter().map(|target| target.spelling()).collect();
    for (function, (oracle, row)) in &by_function {
        if !catalogued.contains(function) {
            problems.push(format!(
                "{function}: the oracle workbook {} calls it in row {}, and the catalogue holds no such function",
                ORACLES[*oracle].name, row.row
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "{} disagreements with Excel's saved packages and captures:\n{}",
        problems.len(),
        problems.join("\n")
    );
    note!(
        "ACCEPT-NOTE: {CASE}: all {} catalogued named functions agree with the packages Excel saved: {bare} saved without a prefix in column A and as written in column B, with the same raw value in both; {} saved with their prefix (_xlfn. or _xlfn._xlws.) and the call in column A and as _xludf. and the call in column B, whose raw value is #NAME?: {}",
        named.len(),
        prefixed.len(),
        prefixed.join(", ")
    );
    note!(
        "ACCEPT-NOTE: {CASE}: every operator has no file prefix"
    );
}
