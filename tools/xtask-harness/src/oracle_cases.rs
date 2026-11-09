//! The oracle workbooks committed in this repository and their committed captures, as `cargo xtask
//! corpus --oracle` judges them and `cargo xtask capture --pending` takes them.
//!
//! An oracle workbook is `crates/<crate>/tests/fixtures/<case>/oracle.xlsx`. Its captures are in
//! `crates/<crate>/tests/captures/<case>/`, in one of two forms:
//!
//! - **whole**: `oracle.capture.json`, the capture of the whole workbook, valid while its `source`
//!   is the SHA-256 of the workbook as committed;
//! - **per section**, when the folder holds `sections/`: `sections/<sheet>.capture.json` for each
//!   sheet with formulas, the capture of that sheet alone, valid while its `source` is the sheet's
//!   section key (`calc_verdict::workbook_sections`), which covers the sheet and what it reads, so
//!   a section added or changed elsewhere in the workbook leaves it valid. A section may keep the
//!   worksheet part Excel saved for it beside its capture (`sections/<sheet>.saved.xml`, written
//!   with the key it belongs to), for a test that reads how Excel wrote its formulas.
//!
//! A workbook or section with no valid capture is awaiting capture. Nothing else (the Excel build,
//! the capture worker's hash, the saved package's hash) decides validity: they are provenance.

use calc_verdict::{CaptureRecord, MissingCapture, WorkbookSection, workbook_sections};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The folder of a case's section captures, below its captures folder.
pub(crate) const SECTIONS_DIR: &str = "sections";

/// One oracle workbook of the repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OracleCase {
    /// `<crate>/<case>/oracle.xlsx`.
    pub(crate) name: String,
    /// `<case>`.
    pub(crate) case: String,
    pub(crate) fixture: PathBuf,
    /// `crates/<crate>/tests/captures/<case>`.
    pub(crate) captures: PathBuf,
}

impl OracleCase {
    /// Whether the case is captured per section (its captures folder holds `sections/`).
    pub(crate) fn sectioned(&self) -> bool {
        self.captures.join(SECTIONS_DIR).is_dir()
    }

    pub(crate) fn whole_capture(&self) -> PathBuf {
        self.captures.join("oracle.capture.json")
    }

    pub(crate) fn section_capture(&self, sheet: &str) -> PathBuf {
        self.captures
            .join(SECTIONS_DIR)
            .join(format!("{}.capture.json", file_name(sheet)))
    }

    pub(crate) fn section_saved(&self, sheet: &str) -> PathBuf {
        self.captures
            .join(SECTIONS_DIR)
            .join(format!("{}.saved.xml", file_name(sheet)))
    }

    /// The case a section capture names: `<case>__<sheet>`.
    pub(crate) fn section_case(&self, sheet: &str) -> String {
        format!("{}__{}", self.case, file_name(sheet))
    }
}

/// A sheet name as a file name: ASCII letters, digits, `-`, `_` and `.` kept, anything else `_`.
pub(crate) fn file_name(sheet: &str) -> String {
    sheet
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Every `crates/*/tests/fixtures/*/oracle.xlsx` under `root`, in path order.
pub(crate) fn oracle_cases(root: &Path) -> Result<Vec<OracleCase>, String> {
    let list = |dir: &Path| -> Result<Vec<PathBuf>, String> {
        let mut dirs: Vec<PathBuf> = match fs::read_dir(dir) {
            Ok(listing) => listing
                .filter_map(Result::ok)
                .map(|item| item.path())
                .filter(|path| path.is_dir())
                .collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(format!("cannot list {}: {e}", dir.display())),
        };
        dirs.sort();
        Ok(dirs)
    };
    let mut cases = Vec::new();
    for krate in list(&root.join("crates"))? {
        for case in list(&krate.join("tests").join("fixtures"))? {
            let fixture = case.join("oracle.xlsx");
            if !fixture.is_file() {
                continue;
            }
            let crate_name = krate.file_name().unwrap_or_default().to_string_lossy();
            let case_name = case
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            cases.push(OracleCase {
                name: format!("{crate_name}/{case_name}/oracle.xlsx"),
                captures: krate.join("tests").join("captures").join(&case_name),
                case: case_name,
                fixture,
            });
        }
    }
    Ok(cases)
}

/// The parts of a package (stored or deflated), by name, read through calc-doc.
pub(crate) fn package_parts(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let source: Arc<dyn calc_doc::PackageRead> =
        Arc::new(calc_doc::MemorySource::new(bytes.to_vec()));
    let document = calc_doc::open(source, &calc_doc::OpenPolicy::default())
        .map_err(|e| format!("the package does not open: {e}"))?;
    let package = document.package();
    let mut parts = BTreeMap::new();
    for part in package.parts() {
        let content = package
            .read_part(part.name())
            .map_err(|e| format!("the part {} does not read: {e}", part.name()))?;
        parts.insert(part.name().to_owned(), content);
    }
    Ok(parts)
}

/// One section of a sectioned case, with its committed capture when that is valid (its source is
/// the section's key).
#[derive(Clone, Debug)]
pub(crate) struct SectionState {
    pub(crate) section: WorkbookSection,
    pub(crate) capture: Option<CaptureRecord>,
    /// Why the section has no valid capture, when it has none.
    pub(crate) awaiting: Option<String>,
}

/// The sections with formulas of the case's workbook `bytes`, each with its valid capture or why
/// it awaits one, and the section captures of sheets the workbook no longer has (or no longer
/// holds formulas on).
pub(crate) fn section_states(
    case: &OracleCase,
    bytes: &[u8],
) -> Result<(Vec<SectionState>, Vec<PathBuf>), String> {
    let workbook_hash = calc_verdict::sha256_hex(bytes);
    if calc_oracle_excel::capture_source_sha256(&case.fixture, &workbook_hash)? != workbook_hash {
        return Err("authored-entry fixtures require whole-workbook captures".to_owned());
    }
    let parts = package_parts(bytes)?;
    let sections = workbook_sections(&parts)?;
    let mut states = Vec::new();
    for section in sections.iter().filter(|s| s.formula_cells > 0) {
        let path = case.section_capture(&section.sheet);
        let (capture, awaiting) = match fs::read(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                (None, Some("no section capture".to_owned()))
            }
            Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
            Ok(record) => {
                let record = CaptureRecord::decode(&record)
                    .map_err(|e| format!("{} does not decode: {e}", path.display()))?;
                if record.source == section.key {
                    (Some(record), None)
                } else {
                    (
                        None,
                        Some(format!(
                            "the section capture is of {}, the section is now {}",
                            record.source, section.key
                        )),
                    )
                }
            }
        };
        states.push(SectionState {
            section: section.clone(),
            capture,
            awaiting,
        });
    }
    let mut orphans = Vec::new();
    if let Ok(listing) = fs::read_dir(case.captures.join(SECTIONS_DIR)) {
        for item in listing.filter_map(Result::ok) {
            let path = item.path();
            let known = states.iter().any(|state| {
                path == case.section_capture(&state.section.sheet)
                    || path == case.section_saved(&state.section.sheet)
            });
            if !known {
                orphans.push(path);
            }
        }
    }
    orphans.sort();
    Ok((states, orphans))
}

/// A whole-workbook case's committed capture: valid (`Ok`) when its source is the SHA-256 of the
/// workbook as committed (`sha256`, hex), or why the workbook awaits capture.
pub(crate) fn whole_state(case: &OracleCase, sha256: &str) -> Result<PathBuf, String> {
    let path = case.whole_capture();
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err("no committed capture".to_owned());
        }
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    if let Some(missing) = MissingCapture::recognize(&bytes) {
        return Err(format!(
            "the capture path holds the absent-capture marker: {missing}"
        ));
    }
    let record = CaptureRecord::decode(&bytes)
        .map_err(|e| format!("{} does not decode: {e}", path.display()))?;
    let input_sha256 = calc_oracle_excel::capture_source_sha256(&case.fixture, sha256)?;
    if record.source == format!("sha256:{input_sha256}") {
        Ok(path)
    } else {
        Err(format!(
            "the committed capture is of {}, the current capture inputs are sha256:{input_sha256}",
            record.source
        ))
    }
}

/// The CRC-32 (ISO-HDLC, as ZIP uses) of `bytes`.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// The package (stored or deflated) with each named part replaced. Entries keep
/// their order and dates; output entries are stored, as the fixture generators write them.
pub(crate) fn replace_package_parts(
    package: &[u8],
    replace: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>, String> {
    let mut parts = package_parts(package)?;
    // The names in central-directory order, with each entry's DOS time and date.
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
        .ok_or("the package has no end of central directory record")?;
    let (count, mut at) = (u16_at(end + 10), u32_at(end + 16));
    let mut order = Vec::new();
    for _ in 0..count {
        let name_length = u16_at(at + 28);
        let name = String::from_utf8_lossy(&package[at + 46..at + 46 + name_length]).into_owned();
        // OPC's part directory excludes the content-types entry and directory entries.
        // Preserve their exact content too, validating the original size and CRC.
        if !parts.contains_key(&name) {
            let local = u32_at(at + 42);
            let start = local + 30 + u16_at(local + 26) + u16_at(local + 28);
            let compressed = package
                .get(start..start + u32_at(at + 20))
                .ok_or("a ZIP entry extends past the package")?;
            let content = match u16_at(at + 10) {
                0 => compressed.to_vec(),
                8 => {
                    miniz_oxide::inflate::decompress_to_vec_with_limit(compressed, u32_at(at + 24))
                        .map_err(|error| format!("{name} does not inflate: {error:?}"))?
                }
                method => return Err(format!("{name} uses unsupported ZIP method {method}")),
            };
            if content.len() != u32_at(at + 24) || crc32(&content) != u32_at(at + 16) as u32 {
                return Err(format!("{name} does not match its ZIP size and CRC"));
            }
            parts.insert(name.clone(), content);
        }
        let stamp = [
            package[at + 12],
            package[at + 13],
            package[at + 14],
            package[at + 15],
        ];
        order.push((name, stamp));
        at += 46 + name_length + u16_at(at + 30) + u16_at(at + 32);
    }
    for name in replace.keys() {
        if !parts.contains_key(name) {
            return Err(format!("the package has no part {name} to replace"));
        }
    }
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, stamp) in &order {
        let data = replace.get(name).unwrap_or(&parts[name]);
        let crc = crc32(data);
        let offset = out.len() as u32;
        let size = u32::try_from(data.len()).map_err(|_| format!("{name} is too large"))?;
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&[20, 0, 0, 0, 0, 0]);
        out.extend_from_slice(stamp);
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        central.push((name, *stamp, crc, size, offset));
    }
    let start = out.len() as u32;
    for (name, stamp, crc, size, offset) in &central {
        out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        out.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0]);
        out.extend_from_slice(stamp);
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&[0; 8]);
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
    }
    let size = out.len() as u32 - start;
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(central.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u16).to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    Ok(out)
}

/// A worksheet part with its cells taken out (`<sheetData/>`), everything else kept.
pub(crate) fn emptied_sheet(xml: &str) -> String {
    if let Some(start) = xml.find("<sheetData") {
        let open_end = xml[start..]
            .find('>')
            .map_or(xml.len(), |at| start + at + 1);
        if xml[start..open_end].ends_with("/>") {
            return xml.to_owned();
        }
        if let Some(close) = xml[open_end..].find("</sheetData>") {
            let end = open_end + close + "</sheetData>".len();
            return format!("{}<sheetData/>{}", &xml[..start], &xml[end..]);
        }
    }
    xml.to_owned()
}

/// The text of a saved worksheet part kept beside a section capture: the part with a comment after
/// its XML declaration naming the section key it was saved for.
pub(crate) fn keyed_saved_sheet(xml: &str, key: &str) -> String {
    let comment = format!("<!-- dnacalc section key: {key} -->");
    match xml.find("?>").filter(|_| xml.starts_with("<?xml")) {
        Some(at) => format!(
            "{}\n{comment}\n{}",
            &xml[..at + 2],
            xml[at + 2..].trim_start()
        ),
        None => format!("{comment}\n{xml}"),
    }
}

#[cfg(test)]
mod tests {
    /// Native-only: changes private capture and sidecar files; never starts Excel.
    #[test]
    fn whole_capture_validity_tracks_authored_input_changes_and_removal() {
        let root = std::env::temp_dir().join(format!(
            "dnacalc-entry-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let fixture = root.join("oracle.xlsx");
        fs::write(&fixture, b"workbook bytes").unwrap();
        let captures = root.join("captures");
        fs::create_dir(&captures).unwrap();
        let case = super::OracleCase {
            name: "entry".into(),
            case: "entry".into(),
            fixture: fixture.clone(),
            captures: captures.clone(),
        };
        let sha = calc_verdict::sha256_hex(b"workbook bytes");
        let mut record = serde_json::json!({"format":"dnacalc-capture-record","version":1,"case":"entry","source":format!("sha256:{sha}"),"acquisition":{"kind":"file_cache_replay"},"environment":null,"sheets":[],"formulas":[],"observations":[]});
        let capture = case.whole_capture();
        let write_record =
            |r: &serde_json::Value| fs::write(&capture, serde_json::to_vec(r).unwrap()).unwrap();
        write_record(&record);
        assert!(super::whole_state(&case, &sha).is_ok());
        let sidecar = fixture.with_extension("entries.json");
        let mut layout = serde_json::json!({"questions":[{"question_id":"q","source_sheet":"S","source_cell":"J20","formula_form":"Dynamic","formula_text":"=1+2","stored_formula_text":"1+2"}]});
        let write_layout =
            |l: &serde_json::Value| fs::write(&sidecar, serde_json::to_vec(l).unwrap()).unwrap();
        write_layout(&layout);
        assert!(super::whole_state(&case, &sha).is_err());
        record["source"] = serde_json::json!(format!(
            "sha256:{}",
            calc_oracle_excel::capture_source_sha256(&fixture, &sha).unwrap()
        ));
        write_record(&record);
        assert!(super::whole_state(&case, &sha).is_ok());
        layout["file_form_provenance"] = serde_json::json!("different build, same question");
        write_layout(&layout);
        assert!(super::whole_state(&case, &sha).is_ok());
        layout["questions"][0]["source_cell"] = serde_json::json!("J2");
        write_layout(&layout);
        assert!(super::whole_state(&case, &sha).is_err());
        fs::remove_file(&sidecar).unwrap();
        assert!(super::whole_state(&case, &sha).is_err());
        fs::remove_file(capture).unwrap();
        fs::remove_file(fixture).unwrap();
        fs::remove_dir(captures).unwrap();
        fs::remove_dir(root).unwrap();
    }
    use super::*;

    #[test]
    fn a_crc_is_iso_hdlc() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn a_sheet_empties_to_its_frame() {
        assert_eq!(
            emptied_sheet(
                r#"<worksheet><cols/><sheetData><row r="1"/></sheetData><x/></worksheet>"#
            ),
            r#"<worksheet><cols/><sheetData/><x/></worksheet>"#
        );
        assert_eq!(
            emptied_sheet("<worksheet><sheetData/></worksheet>"),
            "<worksheet><sheetData/></worksheet>"
        );
    }

    #[test]
    fn a_saved_sheet_names_its_key_after_the_declaration() {
        assert_eq!(
            keyed_saved_sheet(
                "<?xml version=\"1.0\"?>\r\n<worksheet/>",
                "section:sha256:00"
            ),
            "<?xml version=\"1.0\"?>\n<!-- dnacalc section key: section:sha256:00 -->\n<worksheet/>"
        );
    }

    #[test]
    fn a_deflated_package_keeps_all_other_parts_when_one_is_replaced() {
        let bytes = include_bytes!("../../../crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx");
        let before = package_parts(bytes).unwrap();
        let workbook = String::from_utf8(before["xl/workbook.xml"].clone()).unwrap();
        let manual = workbook.replacen("<calcPr ", "<calcPr calcMode=\"manual\" ", 1);
        assert_ne!(manual, workbook);
        let mut expected = before.clone();
        expected.insert("xl/workbook.xml".to_owned(), manual.into_bytes());
        let changed = replace_package_parts(
            bytes,
            &BTreeMap::from([(
                "xl/workbook.xml".to_owned(),
                expected["xl/workbook.xml"].clone(),
            )]),
        )
        .unwrap();
        assert_eq!(package_parts(&changed).unwrap(), expected);
        let mut stored = calc_verdict::stored_package_parts(&changed).unwrap();
        assert!(stored.remove("[Content_Types].xml").is_some());
        assert_eq!(stored, expected);
    }

    #[test]
    fn a_stored_package_keeps_its_other_parts_when_one_is_replaced() {
        let catalogue =
            include_bytes!("../../../crates/calc-func/tests/fixtures/catalogue/oracle.xlsx");
        let parts = calc_verdict::stored_package_parts(catalogue).unwrap();
        let sheet = "xl/worksheets/sheet2.xml".to_owned();
        let emptied = emptied_sheet(std::str::from_utf8(&parts[&sheet]).unwrap());
        let replaced = replace_package_parts(
            catalogue,
            &BTreeMap::from([(sheet.clone(), emptied.clone().into_bytes())]),
        )
        .unwrap();
        let after = calc_verdict::stored_package_parts(&replaced).unwrap();
        assert_eq!(after.len(), parts.len());
        for (name, content) in &parts {
            if *name == sheet {
                assert_eq!(after[name], emptied.as_bytes());
            } else {
                assert_eq!(&after[name], content, "{name}");
            }
        }
        // Replacing nothing gives the package back byte for byte.
        assert_eq!(
            replace_package_parts(catalogue, &BTreeMap::new()).unwrap(),
            catalogue
        );
    }
}
