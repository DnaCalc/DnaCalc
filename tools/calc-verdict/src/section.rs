//! An oracle workbook captured one section (one worksheet) at a time.
//!
//! A section's capture is keyed by what decides its sheet's results, so it stays valid while the
//! sheet and what it reads are unchanged, whatever other sections are added or changed around it.
//! The key ([`WorkbookSection::key`], written `section:sha256:<hex>`) is the SHA-256 of a text
//! made of:
//!
//! - the sheet's name and its worksheet part as the package stores it (formulas, constants,
//!   style indices, column widths, views), and the part of its relationships, if any;
//! - the worksheet part of every sheet its part names as `Name!` or `'Name'!` (a formula's
//!   reference or a text INDIRECT reads), and of the sheets those name, in turn;
//! - the cell formats its parts use (each `xf` of `cellXfs` they name by index, the default one
//!   included, with the number format, font, fill, border and cell style it names);
//! - the workbook part without its list of sheets (date system, calculation properties, defined
//!   names), and the cell metadata part (dynamic-array formulas);
//! - the sheet's position when it calls SHEET or SHEETS, and the list of every sheet when it calls
//!   SHEETS() or INFO("numfile"), whose results depend on the whole workbook.
//!
//! A section capture is a CaptureRecord version 2 holding one sheet's inventory and observations
//! ([`section_record`]); its `source` is the section key, and its session's input package names
//! the package Excel opened, which may hold other sections, or other sections emptied. Its
//! environment and session (Excel build, worker, saved package) are provenance; none of them
//! decides whether the capture is valid. Only the key does.
//!
//! The package is read through its parts ([`stored_package_parts`] reads a package whose entries
//! are stored, as the repository's generators write them; a caller with a deflated package
//! inflates it itself).

use crate::capture::CaptureRecord;
use crate::sha256::sha256_hex;
use std::collections::{BTreeMap, BTreeSet};

/// The prefix of a section key.
pub const SECTION_KEY_PREFIX: &str = "section:sha256:";

/// The first line of the text a section key is the hash of; a change to what the key covers
/// changes it.
const KEY_TEXT_VERSION: &str = "dnacalc-section-key/1";

/// One worksheet of a workbook, as a section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkbookSection {
    /// The sheet's name.
    pub sheet: String,
    /// Its position among the workbook's sheets, from 1.
    pub position: usize,
    /// Its worksheet part.
    pub part: String,
    /// `section:sha256:<hex>`: see the module documentation.
    pub key: String,
    /// The formula cells its worksheet part holds: each cell with a formula element, and every
    /// cell of a legacy array formula's range.
    pub formula_cells: u64,
    /// The other sheets its key covers, because it reads them (directly or through each other).
    pub reads: Vec<String>,
}

/// The parts of a ZIP package whose entries are all stored (not compressed), by name, read
/// through its central directory. A compressed entry is refused, naming it.
pub fn stored_package_parts(package: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let u16_at = |at: usize| -> Result<usize, String> {
        package
            .get(at..at + 2)
            .map(|b| usize::from(u16::from_le_bytes([b[0], b[1]])))
            .ok_or_else(|| "the package ends inside a ZIP record".to_owned())
    };
    let u32_at = |at: usize| -> Result<usize, String> {
        package
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
            .ok_or_else(|| "the package ends inside a ZIP record".to_owned())
    };
    if package.len() < 22 {
        return Err("the package is too short to be a ZIP file".to_owned());
    }
    let end = (0..=package.len() - 22)
        .rev()
        .find(|&at| u32_at(at).is_ok_and(|v| v == 0x0605_4b50))
        .ok_or("the package has no end of central directory record")?;
    let (entries, mut at) = (u16_at(end + 10)?, u32_at(end + 16)?);
    let mut parts = BTreeMap::new();
    for _ in 0..entries {
        if u32_at(at)? != 0x0201_4b50 {
            return Err("a central directory entry has the wrong signature".to_owned());
        }
        let (method, size) = (u16_at(at + 10)?, u32_at(at + 24)?);
        let (name_length, extra_length, comment_length) =
            (u16_at(at + 28)?, u16_at(at + 30)?, u16_at(at + 32)?);
        let name = package
            .get(at + 46..at + 46 + name_length)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .ok_or("a part name is not UTF-8")?
            .to_owned();
        if method != 0 {
            return Err(format!(
                "the part {name} is compressed (method {method}); only stored parts are read here"
            ));
        }
        let local = u32_at(at + 42)?;
        if u32_at(local)? != 0x0403_4b50 {
            return Err(format!("{name}: the local entry has the wrong signature"));
        }
        let data = local + 30 + u16_at(local + 26)? + u16_at(local + 28)?;
        let bytes = package
            .get(data..data + size)
            .ok_or_else(|| format!("{name}: the data runs past the end of the package"))?;
        if parts.insert(name.clone(), bytes.to_vec()).is_some() {
            return Err(format!("the part {name} is listed twice"));
        }
        at += 46 + name_length + extra_length + comment_length;
    }
    Ok(parts)
}

fn part_text<'p>(parts: &'p BTreeMap<String, Vec<u8>>, name: &str) -> Result<&'p str, String> {
    let bytes = parts
        .get(name)
        .ok_or_else(|| format!("the package has no part {name}"))?;
    std::str::from_utf8(bytes).map_err(|e| format!("the part {name} is not UTF-8: {e}"))
}

/// The value of attribute `name` in `tag` (a start tag's text): the first `name="` at its start
/// or after white space.
fn attribute<'x>(tag: &'x str, name: &str) -> Option<&'x str> {
    let marker = format!("{name}=\"");
    let start = tag
        .match_indices(&marker)
        .map(|(at, _)| at)
        .find(|&at| at == 0 || tag.as_bytes()[at - 1].is_ascii_whitespace())?
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

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The start tags of the elements named `element` in `xml`, each the text after the element's
/// name up to its `>` (a self-closing tag's `/` included).
fn start_tags<'x>(xml: &'x str, element: &str) -> Vec<&'x str> {
    let opening = format!("<{element}");
    let mut tags = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find(&opening) {
        let after = &rest[at + opening.len()..];
        let boundary = after.chars().next();
        if matches!(boundary, Some(c) if c.is_ascii_whitespace() || c == '>' || c == '/') {
            let end = after.find('>').unwrap_or(after.len());
            tags.push(&after[..end]);
            rest = &after[end..];
        } else {
            rest = after;
        }
    }
    tags
}

/// The elements named `child` directly inside the first element named `container`, each as its
/// whole text. Neither may nest inside itself.
fn children<'x>(xml: &'x str, container: &str, child: &str) -> Vec<&'x str> {
    let Some(start) = start_tags(xml, container).first().map(|tag| {
        let at = tag.as_ptr() as usize - xml.as_ptr() as usize;
        at + tag.len() + 1
    }) else {
        return Vec::new();
    };
    let close = format!("</{container}>");
    let end = xml[start..].find(&close).map_or(xml.len(), |at| start + at);
    let inner = &xml[start..end];
    let opening = format!("<{child}");
    let closing = format!("</{child}>");
    let mut found = Vec::new();
    let mut rest = inner;
    while let Some(at) = rest.find(&opening) {
        let after = &rest[at + opening.len()..];
        if !matches!(after.chars().next(), Some(c) if c.is_ascii_whitespace() || c == '>' || c == '/')
        {
            rest = after;
            continue;
        }
        let tag_end = after.find('>').unwrap_or(after.len());
        let element_end = if after[..tag_end].ends_with('/') {
            at + opening.len() + tag_end + 1
        } else {
            match rest[at..].find(&closing) {
                Some(close_at) => at + close_at + closing.len(),
                None => rest.len(),
            }
        };
        found.push(&rest[at..element_end]);
        rest = &rest[element_end..];
    }
    found
}

/// The parts the relationships part `rels` targets, by relationship ID, each resolved against
/// `base` (the folder of the part the relationships belong to), with their type.
fn relationships(rels: &str, base: &str) -> Vec<(String, String, String)> {
    start_tags(rels, "Relationship")
        .into_iter()
        .filter_map(|tag| {
            let id = attribute(tag, "Id")?;
            let kind = attribute(tag, "Type")?;
            let target = unescape(attribute(tag, "Target")?);
            if attribute(tag, "TargetMode") == Some("External") {
                return None;
            }
            let resolved = match target.strip_prefix('/') {
                Some(absolute) => absolute.to_owned(),
                None => resolve(base, &target),
            };
            Some((id.to_owned(), kind.to_owned(), resolved))
        })
        .collect()
}

/// `target` relative to the folder `base` (`xl/` say), with `..` segments taken out.
fn resolve(base: &str, target: &str) -> String {
    let mut segments: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
    for segment in target.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    segments.join("/")
}

/// The relationships part of `part` (`xl/_rels/workbook.xml.rels` for `xl/workbook.xml`).
fn rels_name(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((folder, file)) => format!("{folder}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

fn folder(part: &str) -> &str {
    part.rsplit_once('/').map_or("", |(folder, _)| folder)
}

/// Whether `xml` names the sheet `name` as a reference's sheet: `name!` after a character that
/// cannot be part of an unquoted sheet name, or `'name'!` (an apostrophe in the name doubled).
fn names_sheet(xml: &str, name: &str) -> bool {
    let escaped = escape(name);
    let quoted = format!("'{}'!", escape(&name.replace('\'', "''")));
    let quoted_apos = quoted.replace('\'', "&apos;");
    if xml.contains(&quoted) || xml.contains(&quoted_apos) {
        return true;
    }
    let bare = format!("{escaped}!");
    xml.match_indices(&bare).any(|(at, _)| {
        xml[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '.' || c == '\\'))
    })
}

/// Whether `xml` calls the function `name` (case-insensitively, after a character that cannot be
/// part of a function name), with `argument` as the text right after its `(` when given.
fn calls(xml: &str, name: &str, argument: Option<&str>) -> bool {
    let lower = xml.to_ascii_lowercase();
    let call = format!("{}(", name.to_ascii_lowercase());
    lower.match_indices(&call).any(|(at, _)| {
        let before_ok = lower[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        let after = &lower[at + call.len()..];
        let argument_ok = match argument {
            None => true,
            Some(argument) => {
                let argument = argument.to_ascii_lowercase();
                after.starts_with(&argument)
                    || after
                        .strip_prefix("&quot;")
                        .or_else(|| after.strip_prefix('"'))
                        .is_some_and(|inner| {
                            argument
                                .strip_prefix('"')
                                .is_some_and(|word| inner.starts_with(word))
                        })
            }
        };
        before_ok && argument_ok
    })
}

/// The row and column of an A1 reference (`$` signs allowed), or `None`.
fn a1(reference: &str) -> Option<(u64, u64)> {
    let reference = reference.replace('$', "");
    let letters: String = reference
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .collect();
    let digits = &reference[letters.len()..];
    let row: u64 = digits.parse().ok()?;
    let column = letters.chars().try_fold(0u64, |acc, c| {
        Some(acc * 26 + u64::from(c.to_ascii_uppercase() as u8 - b'A' + 1))
    })?;
    (!letters.is_empty() && row > 0).then_some((row, column))
}

/// The formula cells of a worksheet part: each cell holding a formula element, and for a legacy
/// array formula (an array formula whose cell names no cell metadata) every cell of its range.
fn formula_cells(xml: &str) -> u64 {
    let mut count = 0;
    let mut rest = xml;
    while let Some(at) = rest.find("<c ") {
        let cell = &rest[at + 3..];
        let tag_end = cell.find('>').unwrap_or(cell.len());
        let tag = &cell[..tag_end];
        if tag.ends_with('/') {
            rest = &cell[tag_end..];
            continue;
        }
        let end = cell.find("</c>").unwrap_or(cell.len());
        let body = &cell[tag_end..end];
        if let Some(formula_tag) = start_tags(body, "f").first() {
            let legacy_array = attribute(formula_tag, "t") == Some("array")
                && attribute(tag, "cm").is_none();
            let area = legacy_array
                .then(|| attribute(formula_tag, "ref"))
                .flatten()
                .and_then(|reference| {
                    let (first, last) = reference.split_once(':').unwrap_or((reference, reference));
                    let ((r1, c1), (r2, c2)) = (a1(first)?, a1(last)?);
                    Some((r2.abs_diff(r1) + 1) * (c2.abs_diff(c1) + 1))
                });
            count += area.unwrap_or(1);
        }
        rest = &cell[end..];
    }
    count
}

/// The style indices a worksheet part names: each `s` of a cell or row and each `style` of a
/// column, and 0, the format of a cell that names none.
fn style_indices(xml: &str) -> BTreeSet<usize> {
    let mut indices = BTreeSet::from([0]);
    for (element, name) in [("c", "s"), ("row", "s"), ("col", "style")] {
        for tag in start_tags(xml, element) {
            if let Some(index) = attribute(tag, name).and_then(|v| v.parse().ok()) {
                indices.insert(index);
            }
        }
    }
    indices
}

/// The cell formats `indices` names, each with what it refers to, as text for the key.
fn styles_text(styles: Option<&str>, indices: &BTreeSet<usize>) -> String {
    let Some(styles) = styles else {
        return "no styles part\n".to_owned();
    };
    let cell_xfs = children(styles, "cellXfs", "xf");
    let style_xfs = children(styles, "cellStyleXfs", "xf");
    let fonts = children(styles, "fonts", "font");
    let fills = children(styles, "fills", "fill");
    let borders = children(styles, "borders", "border");
    let number_formats = children(styles, "numFmts", "numFmt");
    let mut text = String::new();
    let indexed = |list: &[&str], what: &str, index: Option<&str>, text: &mut String| {
        if let Some(index) = index {
            match index.parse::<usize>().ok().and_then(|i| list.get(i)) {
                Some(element) => text.push_str(&format!("  {what} {index} {element}\n")),
                None => text.push_str(&format!("  {what} {index} missing\n")),
            }
        }
    };
    for &index in indices {
        let Some(xf) = cell_xfs.get(index) else {
            text.push_str(&format!("xf {index} missing\n"));
            continue;
        };
        text.push_str(&format!("xf {index} {xf}\n"));
        let tag = xf.trim_start_matches("<xf");
        if let Some(id) = attribute(tag, "numFmtId") {
            if let Some(format) = number_formats
                .iter()
                .find(|format| attribute(format.trim_start_matches("<numFmt"), "numFmtId") == Some(id))
            {
                text.push_str(&format!("  numFmt {format}\n"));
            }
        }
        indexed(&fonts, "font", attribute(tag, "fontId"), &mut text);
        indexed(&fills, "fill", attribute(tag, "fillId"), &mut text);
        indexed(&borders, "border", attribute(tag, "borderId"), &mut text);
        indexed(&style_xfs, "cellStyleXf", attribute(tag, "xfId"), &mut text);
    }
    text
}

/// The workbook part without its `<sheets>` element.
fn workbook_without_sheets(workbook: &str) -> String {
    match (workbook.find("<sheets"), workbook.find("</sheets>")) {
        (Some(start), Some(end)) if end > start => {
            format!("{}{}", &workbook[..start], &workbook[end + "</sheets>".len()..])
        }
        _ => match workbook.find("<sheets") {
            Some(start) => {
                let end = workbook[start..]
                    .find("/>")
                    .map_or(workbook.len(), |at| start + at + 2);
                format!("{}{}", &workbook[..start], &workbook[end..])
            }
            None => workbook.to_owned(),
        },
    }
}

/// Every worksheet of the workbook in `parts`, in the workbook's order, as a section with its key.
pub fn workbook_sections(
    parts: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<WorkbookSection>, String> {
    let root_rels = part_text(parts, "_rels/.rels")?;
    let workbook_part = relationships(root_rels, "")
        .into_iter()
        .find(|(_, kind, _)| kind.ends_with("/officeDocument"))
        .map(|(_, _, target)| target)
        .ok_or("the package names no workbook part")?;
    let workbook = part_text(parts, &workbook_part)?;
    let workbook_rels = relationships(
        part_text(parts, &rels_name(&workbook_part))?,
        folder(&workbook_part),
    );
    let related = |suffix: &str| {
        workbook_rels
            .iter()
            .find(|(_, kind, _)| kind.ends_with(suffix))
            .map(|(_, _, target)| target.as_str())
    };
    let styles = related("/styles")
        .map(|name| part_text(parts, name))
        .transpose()?;
    let metadata = related("/sheetMetadata")
        .map(|name| part_text(parts, name))
        .transpose()?;

    // The sheets, in order: name, part and the part's text.
    let mut sheets: Vec<(String, String, &str)> = Vec::new();
    let sheets_xml = workbook
        .find("<sheets")
        .map_or("", |start| &workbook[start..]);
    let sheets_xml = &sheets_xml[..sheets_xml.find("</sheets>").unwrap_or(sheets_xml.len())];
    for tag in start_tags(sheets_xml, "sheet") {
        let name = unescape(attribute(tag, "name").ok_or("a sheet has no name")?);
        let id = attribute(tag, "r:id").ok_or_else(|| format!("the sheet {name} has no r:id"))?;
        let part = workbook_rels
            .iter()
            .find(|(rid, _, _)| rid == id)
            .map(|(_, _, target)| target.clone())
            .ok_or_else(|| format!("the sheet {name}'s relationship {id} is not in the package"))?;
        let text = part_text(parts, &part)?;
        sheets.push((name, part, text));
    }
    let names: Vec<&str> = sheets.iter().map(|(name, _, _)| name.as_str()).collect();
    let context = workbook_without_sheets(workbook);

    let mut sections = Vec::new();
    for (index, (name, part, xml)) in sheets.iter().enumerate() {
        // The sheets it reads, directly or through each other.
        let mut reads: BTreeSet<usize> = BTreeSet::new();
        let mut frontier = vec![index];
        while let Some(at) = frontier.pop() {
            for (other, other_name) in names.iter().enumerate() {
                if other != index && !reads.contains(&other) && names_sheet(sheets[at].2, other_name)
                {
                    reads.insert(other);
                    frontier.push(other);
                }
            }
        }
        let mut text = format!("{KEY_TEXT_VERSION}\nsheet {name}\n{xml}\n");
        if let Some(rels) = parts.get(&rels_name(part)) {
            text.push_str(&format!("sheet relationships\n{}\n", String::from_utf8_lossy(rels)));
        }
        let mut indices = style_indices(xml);
        for &other in &reads {
            let (other_name, _, other_xml) = &sheets[other];
            text.push_str(&format!("reads {other_name}\n{other_xml}\n"));
            indices.extend(style_indices(other_xml));
        }
        text.push_str("styles\n");
        text.push_str(&styles_text(styles, &indices));
        text.push_str(&format!("workbook\n{context}\n"));
        if let Some(metadata) = metadata {
            text.push_str(&format!("metadata\n{metadata}\n"));
        }
        if calls(xml, "SHEET", None) || calls(xml, "SHEETS", None) {
            text.push_str(&format!("position {}\n", index + 1));
        }
        if calls(xml, "SHEETS", Some(")")) || calls(xml, "INFO", Some("\"numfile")) {
            text.push_str(&format!("sheets {}\n", names.join("\n")));
        }
        sections.push(WorkbookSection {
            sheet: name.clone(),
            position: index + 1,
            part: part.clone(),
            key: format!("{SECTION_KEY_PREFIX}{}", sha256_hex(text.as_bytes())),
            formula_cells: formula_cells(xml),
            reads: reads.iter().map(|&other| sheets[other].0.clone()).collect(),
        });
    }
    Ok(sections)
}

/// The capture of one section from a capture of a workbook that holds it: the record's inventory,
/// observations, saved caches, reopened values, shared ranges and cell failures on `sheet`, and its
/// failures of no cell (which concern the whole session), with `source` the section's `key` and
/// case `case`. The environment and the rest of the session are kept as they are.
pub fn section_record(record: &CaptureRecord, sheet: &str, case: &str, key: &str) -> CaptureRecord {
    let on_sheet = |name: &str| name == sheet;
    let mut session = record.session.clone();
    if let Some(session) = &mut session {
        session.failures.retain(|failure| {
            failure
                .cell
                .as_ref()
                .is_none_or(|cell| on_sheet(cell.sheet()))
        });
        session.shared_ranges.retain(|range| on_sheet(range.sheet()));
        session.saved.retain(|observed| on_sheet(observed.cell.sheet()));
        session
            .reopened
            .retain(|observed| on_sheet(observed.cell.sheet()));
    }
    CaptureRecord {
        case: case.to_owned(),
        source: key.to_owned(),
        acquisition: record.acquisition.clone(),
        environment: record.environment.clone(),
        sheets: record
            .sheets
            .iter()
            .filter(|entry| on_sheet(&entry.name))
            .cloned()
            .collect(),
        formulas: record
            .formulas
            .iter()
            .filter(|formula| on_sheet(formula.cell.sheet()))
            .cloned()
            .collect(),
        observations: record
            .observations
            .iter()
            .filter(|observed| on_sheet(observed.cell.sheet()))
            .cloned()
            .collect(),
        session,
    }
}

/// One record of several section captures taken in one capture session (one input package, one
/// environment), for judging them together against the workbook whose content hash is `source`:
/// the environment and session of the first, the sheets, inventories and observations of all, and
/// the failures of no cell once. `None` when `records` is empty.
pub fn merged_record(records: &[&CaptureRecord], case: &str, source: &str) -> Option<CaptureRecord> {
    let (first, rest) = records.split_first()?;
    let mut merged = (*first).clone();
    merged.case = case.to_owned();
    merged.source = source.to_owned();
    for record in rest {
        merged.sheets.extend(record.sheets.iter().cloned());
        merged.formulas.extend(record.formulas.iter().cloned());
        merged
            .observations
            .extend(record.observations.iter().cloned());
        if let (Some(into), Some(from)) = (&mut merged.session, &record.session) {
            into.failures
                .extend(from.failures.iter().filter(|f| f.cell.is_some()).cloned());
            into.shared_ranges.extend(from.shared_ranges.iter().cloned());
            into.saved.extend(from.saved.iter().cloned());
            into.reopened.extend(from.reopened.iter().cloned());
        }
    }
    Some(merged)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stored package of the parts, as the repository's generators write one.
    fn package(parts: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, text) in parts {
            let offset = out.len() as u32;
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0x21, 0]);
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&(text.len() as u32).to_le_bytes());
            out.extend_from_slice(&(text.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(text.as_bytes());
            central.push((name, text.len() as u32, offset));
        }
        let start = out.len() as u32;
        for (name, size, offset) in &central {
            out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            out.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0x21, 0]);
            out.extend_from_slice(&0u32.to_le_bytes());
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
        out
    }

    const STYLES: &str = r#"<styleSheet><numFmts count="1"><numFmt numFmtId="164" formatCode="0.0"/></numFmts><fonts count="1"><font><sz val="11"/></font></fonts><fills count="1"><fill><patternFill patternType="none"/></fill></fills><borders count="1"><border><left/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0"/></cellStyleXfs><cellXfs count="3"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="164" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="14" fontId="0"/></cellXfs></styleSheet>"#;

    /// A workbook of the sheets (name, sheetData content), with the styles above.
    fn workbook(sheets: &[(&str, &str)], styles: &str) -> BTreeMap<String, Vec<u8>> {
        let mut entries = String::new();
        let mut rels = String::new();
        let mut parts: Vec<(String, String)> = Vec::new();
        for (i, (name, data)) in sheets.iter().enumerate() {
            let n = i + 1;
            entries.push_str(&format!(r#"<sheet name="{name}" sheetId="{n}" r:id="rId{n}"/>"#));
            rels.push_str(&format!(
                r#"<Relationship Id="rId{n}" Type="http://x/worksheet" Target="worksheets/sheet{n}.xml"/>"#
            ));
            parts.push((
                format!("xl/worksheets/sheet{n}.xml"),
                format!("<worksheet><sheetData>{data}</sheetData></worksheet>"),
            ));
        }
        let n = sheets.len() + 1;
        rels.push_str(&format!(
            r#"<Relationship Id="rId{n}" Type="http://x/styles" Target="styles.xml"/>"#
        ));
        parts.push((
            "_rels/.rels".to_owned(),
            r#"<Relationships><Relationship Id="rId1" Type="http://x/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.to_owned(),
        ));
        parts.push((
            "xl/workbook.xml".to_owned(),
            format!(r#"<workbook><sheets>{entries}</sheets><calcPr fullCalcOnLoad="1"/></workbook>"#),
        ));
        parts.push((
            "xl/_rels/workbook.xml.rels".to_owned(),
            format!("<Relationships>{rels}</Relationships>"),
        ));
        parts.push(("xl/styles.xml".to_owned(), styles.to_owned()));
        let borrowed: Vec<(&str, &str)> = parts
            .iter()
            .map(|(name, text)| (name.as_str(), text.as_str()))
            .collect();
        stored_package_parts(&package(&borrowed)).expect("the package reads")
    }

    fn keys(parts: &BTreeMap<String, Vec<u8>>) -> BTreeMap<String, String> {
        workbook_sections(parts)
            .unwrap()
            .into_iter()
            .map(|section| (section.sheet, section.key))
            .collect()
    }

    const DATA: &str = r#"<row r="1"><c r="A1"><v>1</v></c></row>"#;
    const ADD: &str = r#"<row r="1"><c r="D1"><f>Data!A1+1</f></c></row>"#;
    const POWER: &str = r#"<row r="1"><c r="D1"><f>2^3</f></c><c r="D2" s="1"><f>2^4</f></c></row>"#;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_section_key_holds_while_its_sheet_and_what_it_reads_hold() {
        let base = workbook(&[("Data", DATA), ("add", ADD), ("power", POWER)], STYLES);
        let sections = workbook_sections(&base).unwrap();
        assert_eq!(
            sections
                .iter()
                .map(|s| (s.sheet.as_str(), s.formula_cells, s.reads.clone()))
                .collect::<Vec<_>>(),
            [
                ("Data", 0, vec![]),
                ("add", 1, vec!["Data".to_owned()]),
                ("power", 2, vec![])
            ]
        );
        assert!(sections.iter().all(|s| s.key.starts_with(SECTION_KEY_PREFIX)));
        let before = keys(&base);

        // A sheet added after them, and a row added to another sheet: no key changes but that
        // sheet's.
        let more = workbook(
            &[
                ("Data", DATA),
                ("add", ADD),
                ("power", r#"<row r="1"><c r="D1"><f>2^3</f></c><c r="D2" s="1"><f>2^4</f></c></row><row r="2"><c r="D3"><f>1</f></c></row>"#),
                ("new", POWER),
            ],
            STYLES,
        );
        let after = keys(&more);
        assert_eq!(after["Data"], before["Data"]);
        assert_eq!(after["add"], before["add"]);
        assert_ne!(after["power"], before["power"]);

        // What a sheet reads decides its key: a changed input changes the reader's.
        let input = workbook(
            &[("Data", r#"<row r="1"><c r="A1"><v>2</v></c></row>"#), ("add", ADD), ("power", POWER)],
            STYLES,
        );
        let changed = keys(&input);
        assert_ne!(changed["add"], before["add"]);
        assert_eq!(changed["power"], before["power"]);

        // A cell format a sheet uses decides its key; one it does not use does not.
        let restyled = workbook(
            &[("Data", DATA), ("add", ADD), ("power", POWER)],
            &STYLES.replace(r#"formatCode="0.0""#, r#"formatCode="0.00""#),
        );
        let restyled = keys(&restyled);
        assert_ne!(restyled["power"], before["power"]);
        assert_eq!(restyled["add"], before["add"]);
        let unused = workbook(
            &[("Data", DATA), ("add", ADD), ("power", POWER)],
            &STYLES.replace(r#"<xf numFmtId="14" fontId="0"/>"#, r#"<xf numFmtId="15" fontId="0"/>"#),
        );
        assert_eq!(keys(&unused), before);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_sheet_that_reads_the_workbook_s_shape_is_keyed_by_it() {
        let shape = r#"<row r="1"><c r="D1"><f>SHEETS()</f></c><c r="D2"><f>INFO("numfile")</f></c></row>"#;
        let base = keys(&workbook(&[("Data", DATA), ("shape", shape), ("power", POWER)], STYLES));
        let more = keys(&workbook(
            &[("Data", DATA), ("shape", shape), ("power", POWER), ("new", POWER)],
            STYLES,
        ));
        assert_ne!(base["shape"], more["shape"]);
        assert_eq!(base["power"], more["power"]);
        // SHEET of a reference depends on the position alone; SHEETS of a reference on neither.
        let position = r#"<row r="1"><c r="D1"><f>SHEET()</f></c><c r="D2"><f>SHEETS(Data!A1)</f></c></row>"#;
        let base = keys(&workbook(&[("Data", DATA), ("p", position)], STYLES));
        let more = keys(&workbook(&[("Data", DATA), ("p", position), ("new", POWER)], STYLES));
        assert_eq!(base["p"], more["p"]);
        let moved = keys(&workbook(&[("Data", DATA), ("new", POWER), ("p", position)], STYLES));
        assert_ne!(base["p"], moved["p"]);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn formula_cells_count_a_legacy_array_s_range() {
        assert_eq!(
            formula_cells(
                r#"<row r="1"><c r="A1"><f t="array" ref="A1:B3">1</f></c><c r="C1" cm="1"><f t="array" ref="C1">SEQUENCE(3)</f></c><c r="D1"/><c r="E1"><v>1</v></c><c r="F1"><f t="shared" si="0"/></c></row>"#
            ),
            8
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_compressed_part_is_refused() {
        let mut bytes = package(&[("a.xml", "x")]);
        // The central directory entry's method (at 10 within it).
        let central = bytes.len() - 22 - (46 + 5);
        bytes[central + 10] = 8;
        assert!(
            stored_package_parts(&bytes)
                .unwrap_err()
                .contains("compressed")
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_record_splits_into_sections_and_merges_back() {
        let record = CaptureRecord::decode(
            br#"{"format": "dnacalc-capture-record", "version": 2, "case": "c", "source": "sha256:00",
                "acquisition": {"kind": "live_capture"}, "environment": null,
                "session": {"revision": 0, "operations": [], "context": "x", "tool": "t",
                            "packages": {"input": "sha256:00", "output": null},
                            "failures": [{"stage": "save", "message": "m"},
                                         {"stage": "value", "sheet": "B", "row": 1, "column": 4, "message": "v"}]},
                "sheets": [{"name": "A", "visibility": "visible"}, {"name": "B", "visibility": "visible"}],
                "formulas": [{"sheet": "A", "row": 1, "column": 4, "kind": "ordinary"},
                             {"sheet": "B", "row": 1, "column": 4, "kind": "ordinary"}],
                "shared_ranges": [],
                "observations": [{"sheet": "A", "row": 1, "column": 4, "excel_raw": {"number": "3ff0000000000000"}},
                                 {"sheet": "B", "row": 1, "column": 4, "excel_raw": {"number": "4000000000000000"}}],
                "saved": [], "reopened": []}"#,
        )
        .unwrap();
        let a = section_record(&record, "A", "c__A", "section:sha256:aa");
        let b = section_record(&record, "B", "c__B", "section:sha256:bb");
        assert_eq!((a.source.as_str(), a.formulas.len(), a.observations.len()), ("section:sha256:aa", 1, 1));
        assert_eq!(a.session.as_ref().unwrap().failures.len(), 1);
        assert_eq!(b.session.as_ref().unwrap().failures.len(), 2);
        assert!(CaptureRecord::decode(a.encode().unwrap().as_bytes()).is_ok());
        let merged = merged_record(&[&a, &b], "c", "sha256:00").unwrap();
        assert_eq!(merged.formulas, record.formulas);
        assert_eq!(merged.observations, record.observations);
        assert_eq!(merged.sheets, record.sheets);
        assert_eq!(merged.session, record.session);
        assert!(merged_record(&[], "c", "s").is_none());
    }
}
