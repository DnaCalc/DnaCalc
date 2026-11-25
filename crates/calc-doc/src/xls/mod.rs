//! Legacy Excel binary workbooks (.xls, .xla, .xlt; MS-XLS, dc-4j0e): an OLE compound file
//! ([`cfb`]) whose `Workbook` stream is a BIFF8 record stream ([`records`]). [`crate::open`]
//! reads such a file as it reads an .xlsb: it converts it, before anything else reads it, into the
//! SpreadsheetML package it stands for, which the loader reads and a save writes by default, and
//! keeps the source bytes ([`crate::OpenedDocument::legacy_source`]).
//!
//! A file this conversion does not read is a typed refusal ([`crate::OpenError::LegacyWorkbook`])
//! naming the stream and what in it; records it does not read are counted by type in the
//! [`LegacyReport`], never fatal.

pub(crate) mod cfb;
pub(crate) mod cfb_write;
pub(crate) mod codepage;
mod chart;
mod crypto;
mod drawing;
mod external_cache;
mod pivot_cache;
mod properties;
mod raw;
mod features;
mod formula;
mod records;
mod rules;
mod scenario;
mod sheet;
mod strings;
mod styles;
mod v5;
mod workbook;

use crate::OpenError;
use crate::xlsb::Xml;
use std::collections::BTreeMap;
use workbook::SheetKind;

const MAIN_NAMESPACE: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RELATIONSHIPS_NAMESPACE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL_TYPE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// What reading a legacy workbook found, beside the package it became.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LegacyReport {
    /// The source BIFF version (2, 3, 4, 5, 7 or 8).
    pub biff_version: u8,
    /// The sheets of the workbook, of every kind.
    pub sheets: usize,
    /// Record types not read, with how many of each.
    pub unknown_records: BTreeMap<u16, u32>,
    /// Drawing objects not written, with how many of each OBJ type (MS-XLS 2.5.145).
    pub unread_objects: BTreeMap<u16, u32>,
    /// OLE user-property value types not retained; 0xffff means the type could not be read.
    pub unread_custom_property_types: BTreeMap<u16, u32>,
    /// OLE user-property sets whose dictionary or structure could not be decoded.
    pub unread_custom_property_sets: u32,
}

impl LegacyReport {
    /// How many pivot tables (SXVIEW records) the workbook holds; they are not read (dc-4j0e.13),
    /// so a conversion drops them and says so here.
    pub fn pivot_tables(&self) -> u32 {
        self.unknown_records.get(&0x00B0).copied().unwrap_or(0)
    }
}

/// A legacy workbook's container, identified without interpreting workbook content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyFormat {
    /// Workbook stored in an OLE compound document.
    Compound,
    /// Standalone BIFF stream.
    RawBiff {
        /// BIFF version declared by the stream.
        version: u8,
    },
}

impl LegacyFormat {
    /// Sniff an OLE signature or an old raw BIFF BOF header. Raw BIFF2/3/4 reading belongs
    /// to dc-4j0e.14; recognising the header must not silently exclude those files.
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if cfb::is_compound(bytes) { return Some(Self::Compound); }
        if bytes.len() < 8 { return None; }
        let version = match bytes[..4] {
            [0x09, 0x00, 0x04, 0x00] => 2,
            [0x09, 0x02, 0x06, 0x00] => 3,
            [0x09, 0x04, 0x06, 0x00] => 4,
            _ => return None,
        };
        Some(Self::RawBiff { version })
    }
}

/// Whether `bytes` start as a legacy workbook container.
pub(crate) fn is_legacy(bytes: &[u8]) -> bool {
    LegacyFormat::sniff(bytes).is_some()
}

fn refuse(stream: &str, detail: impl Into<String>) -> OpenError {
    OpenError::LegacyWorkbook {
        stream: stream.to_owned(),
        detail: detail.into(),
    }
}

/// The SpreadsheetML package (the bytes of a ZIP archive) the legacy workbook in `bytes` stands
/// for, with what was found reading it.
/// `password` opens a workbook that asks for one; without it the password Excel itself uses for
/// a workbook saved read-only recommended ("VelvetSweatshop") is tried.
pub(crate) fn convert(
    bytes: &[u8],
    password: Option<&str>,
) -> Result<(Vec<u8>, LegacyReport), OpenError> {
    if let Some(LegacyFormat::RawBiff { version }) = LegacyFormat::sniff(bytes) {
        if version == 2 { return raw::convert_biff2(bytes, password); }
        if matches!(version, 3 | 4) { return raw::convert_biff34(bytes, version, password); }
        return Err(refuse("(raw stream)", format!("raw BIFF{version} streams are not read (dc-4j0e.14)")));
    }
    convert_compound(bytes, password, None)
}

fn convert_compound(
    bytes: &[u8], password: Option<&str>, layout: Option<crate::xlsb::formula::Layout>,
) -> Result<(Vec<u8>, LegacyReport), OpenError> {
    let compound = cfb::Compound::open(bytes).map_err(|e| refuse("(compound file)", e.0))?;
    let summary = compound.read("\u{5}SummaryInformation").ok().flatten()
        .and_then(|bytes| properties::summary(&bytes)).unwrap_or_default();
    let document_stream = compound.read("\u{5}DocumentSummaryInformation").ok().flatten();
    let document_properties = document_stream.as_deref().and_then(properties::document).unwrap_or_default();
    let custom_read = document_stream.as_deref().and_then(properties::custom);
    let unread_custom_property_sets = u32::from(custom_read.is_none()
        && document_stream.as_deref().is_some_and(properties::has_custom));
    let custom_read = custom_read.unwrap_or_default();
    let unread_custom_property_types = custom_read.unread;
    let custom_properties = custom_read.entries;
    let has_core_properties = summary.keys().any(|id| *id != 18) || document_properties.contains_key(&2);
    let has_extended_properties = summary.contains_key(&18) || document_properties.contains_key(&14)
        || document_properties.contains_key(&15);
    let name = ["Workbook", "Book"]
        .into_iter()
        .find(|name| compound.find(name).is_some())
        .ok_or_else(|| refuse("(compound file)", "no Workbook or Book stream"))?;
    let stream = compound
        .read(name)
        .map_err(|e| refuse(name, e.0))?
        .expect("found above");
    let (stream, decrypted) = match crypto::decrypt(&stream, password.unwrap_or("VelvetSweatshop")) {
        Ok(Some(plain)) => (plain, true),
        Ok(None) => (stream, false),
        Err(crypto::CryptoError::WrongPassword) => {
            return Err(if password.is_some() {
                OpenError::WrongPassword
            } else {
                OpenError::PasswordRequired
            });
        }
        Err(crypto::CryptoError::Unsupported(what)) => return Err(refuse(name, what)),
    };
    let records = records::read_all(&stream).map_err(|e| refuse(name, e.0))?;
    let first = records
        .first()
        .ok_or_else(|| refuse(name, "the stream holds no records"))?;
    let Some(version) = workbook::bof_version(first) else {
        return Err(refuse(name, "the stream does not start with a BOF record"));
    };
    if workbook::bof_type(first) != Some(0x0005) {
        return Err(refuse(name, "the first substream is not the workbook globals"));
    }
    let biff5 = match version {
        0x0600 => false,
        0x0500 => true,
        other => return Err(refuse(name, format!("BIFF version {other:#06x}"))),
    };
    let mut globals = workbook::read(&records, biff5).map_err(|e| refuse(name, e))?;
    // A BIFF5 or BIFF7 workbook: its strings are bytes in this code page.
    let v5 = biff5.then(|| globals.code_page.unwrap_or(1252));
    if globals.encrypted && !decrypted {
        return Err(refuse(name, "an encrypted workbook (FILEPASS), which dc-4j0e.7 reads"));
    }
    // Each sheet's substream: its BOF at the BOUNDSHEET offset, to its EOF.
    let mut parts: Vec<(String, &'static str, Vec<u8>)> = Vec::new();
    let mut sheet_entries: Vec<(String, u8, String, u32)> = Vec::new();
    let mut number = 0usize;
    // Where each sheet's substream starts and ends in the record list.
    let locate = |sheet: &workbook::BoundSheet| -> Result<(usize, usize), OpenError> {
        let Some(start) = records.iter().position(|r| r.offset == sheet.offset as usize) else {
            return Err(refuse(
                name,
                format!("sheet {:?} starts at byte {} where no record does", sheet.name, sheet.offset),
            ));
        };
        let substream = &records[start..];
        if workbook::bof_type(&substream[0]).is_none() {
            return Err(refuse(
                name,
                format!("sheet {:?} does not start with a BOF record", sheet.name),
            ));
        }
        let mut depth = 0usize;
        for (k, record) in substream.iter().enumerate() {
            match record.id {
                workbook::BOF => depth += 1,
                workbook::EOF => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok((start, start + k + 1));
                    }
                }
                _ => {}
            }
        }
        Err(refuse(name, format!("sheet {:?} has no EOF record", sheet.name)))
    };
    // BIFF5 link tables: the globals' and each sheet's own, gathered before any formula prints.
    let images = drawing::blip_store(&globals.drawing_group);
    let solid_fill_defaults_safe = drawing::solid_fill_defaults_safe(&globals.drawing_group);
    let hidden_line_defaults_safe = drawing::hidden_line_defaults_safe(&globals.drawing_group);
    let line_defaults = drawing::line_defaults(&globals.drawing_group);
    let mut registry = v5::Registry::default();
    let mut tables: Vec<v5::Table> = Vec::new();
    let mut global_table = v5::Table::default();
    let mut v5_cache_consumed = BTreeMap::new();
    if let Some(page) = v5 {
        global_table = registry
            .table(&records[..globals.end], page)
            .map_err(|e| refuse(name, e))?;
        for sheet in globals.sheets.iter().filter(|s| s.kind != SheetKind::Module) {
            let (start, end) = locate(sheet)?;
            tables.push(registry.table(&records[start..end], page).map_err(|e| refuse(name, e))?);
        }
        globals.supbooks = registry
            .documents
            .iter()
            .map(|d| workbook::SupBookRecord::External {
                path: d.path.clone(),
                sheets: d.sheets.clone(),
                names: d.names.clone(),
            })
            .collect();
        let mut cache_tables = vec![(&records[..globals.end], &global_table)];
        for (sheet, table) in globals.sheets.iter().filter(|s| s.kind != SheetKind::Module).zip(&tables) {
            let (start, end) = locate(sheet)?;
            cache_tables.push((&records[start..end], table));
        }
        let (caches, consumed) = external_cache::read_biff5(cache_tables, page);
        globals.external_cache = caches;
        v5_cache_consumed = consumed;
    }
    let mut links = formula::Links::new(
        &globals,
        globals
            .sheets
            .iter()
            .map(|s| s.name.clone())
            .collect(),
        v5,
        layout,
    );
    if v5.is_some() {
        let (books, xti) = global_table.references(&registry.documents);
        links = links.with_table(books, xti);
    }
    let mut sheet_unknown: BTreeMap<u16, u32> = BTreeMap::new();
    let mut calculation_mode: Option<&str> = None;
    let (mut calc_on_save, mut iterate, mut iteration_count, mut iteration_delta, mut ref_a1) = (None, None, None, None, None);
    fn agree<T: PartialEq>(slot: &mut Option<T>, value: T, what: &str) -> Result<(), String> {
        if slot.as_ref().is_some_and(|previous| *previous != value) {
            return Err(format!("conflicting {what} across sheets"));
        }
        *slot = Some(value);
        Ok(())
    }
    let mut extra_entries: Vec<(String, Vec<u8>)> = Vec::new();
    let mut unread_objects: BTreeMap<u16, u32> = BTreeMap::new();
    for (source_tab, sheet) in globals.sheets.iter().enumerate() {
        if sheet.kind == SheetKind::Module {
            continue;
        }
        let (start, end) = locate(sheet)?;
        let substream = &records[start..end];
        let end = substream.len();
        for record in substream.iter().filter(|r| r.id == 0x000D) {
            let raw = record.body.get(..2).ok_or_else(|| refuse(name, "a short CalcMode record"))?;
            let mode = match i16::from_le_bytes([raw[0], raw[1]]) {
                0 => "manual",
                1 => "auto",
                2 => "autoNoTable",
                -1 if biff5 => "autoNoTable",
                other => return Err(refuse(name, format!("the calculation mode {other}"))),
            };
            if calculation_mode.is_some_and(|previous| previous != mode) {
                return Err(refuse(name, "conflicting calculation modes across sheets"));
            }
            calculation_mode = Some(mode);
        }
        for record in substream.iter().filter(|r| matches!(r.id, 0x000C | 0x000F | 0x0010 | 0x0011 | 0x005F)) {
            let read_word = || -> Result<u16, OpenError> {
                let b = record.body.get(..2).ok_or_else(|| refuse(name, "a short calculation settings record"))?;
                Ok(u16::from_le_bytes([b[0], b[1]]))
            };
            match record.id {
                0x000C => {
                    let count = read_word()?;
                    if count == 0 || count > 32767 { return Err(refuse(name, "an invalid iteration count")); }
                    agree(&mut iteration_count, count, "iteration counts").map_err(|e| refuse(name, e))?;
                }
                0x0010 => {
                    let b: [u8; 8] = record.body.get(..8).ok_or_else(|| refuse(name, "a short CalcDelta record"))?
                        .try_into().expect("eight bytes");
                    let value = f64::from_le_bytes(b);
                    if !value.is_finite() || value < 0.0 { return Err(refuse(name, "an invalid iteration delta")); }
                    agree(&mut iteration_delta, value.to_bits(), "iteration deltas").map_err(|e| refuse(name, e))?;
                }
                0x000F | 0x0011 | 0x005F => {
                    let value = read_word()?;
                    if value > 1 { return Err(refuse(name, "an invalid calculation setting")); }
                    let slot = match record.id { 0x000F => &mut ref_a1, 0x0011 => &mut iterate, _ => &mut calc_on_save };
                    agree(slot, value != 0, "calculation settings").map_err(|e| refuse(name, e))?;
                }
                _ => unreachable!(),
            }
        }
        number += 1;
        let sheet_links;
        let links = if v5.is_some() {
            let (books, xti) = tables[number - 1].references(&registry.documents);
            sheet_links = links.with_table(books, xti);
            &sheet_links
        } else {
            &links
        };
        let context = sheet::Context {
            strings: &globals.strings,
            links,
            v5,
            images: &images,
            solid_fill_defaults_safe,
            hidden_line_defaults_safe,
            line_defaults: &line_defaults,
            filter_range: filter_range(&globals, &links, source_tab as u32),
            styles: &globals.styles,
            tab: source_tab as u32,
            number,
        };
        let (part, content_type, xml) = match sheet.kind {
            SheetKind::MacroSheet | SheetKind::Worksheet => {
                let macro_sheet = sheet.kind == SheetKind::MacroSheet;
                let read = sheet::read(&substream[..end], &context, macro_sheet, &sheet.name)
                    .map_err(|detail| refuse(name, detail))?;
                for (id, n) in read.unknown {
                    *sheet_unknown.entry(id).or_insert(0) += n;
                }
                extra_entries.extend(read.entries.iter().cloned());
                for (kind, n) in &read.unread_objects {
                    *unread_objects.entry(*kind).or_insert(0) += n;
                }
                parts.extend(read.parts.iter().map(|(n, t, b)| (n.clone(), *t, b.clone())));
                if !read.rels.is_empty() {
                    let mut r = Xml::new();
                    r.open(
                        "Relationships",
                        &[(
                            "xmlns",
                            "http://schemas.openxmlformats.org/package/2006/relationships".to_owned(),
                        )],
                    );
                    for rel in &read.rels {
                        let mut a = vec![
                            ("Id", rel.id.clone()),
                            ("Type", format!("{REL_TYPE}/{}", rel.kind)),
                            ("Target", rel.target.clone()),
                        ];
                        if rel.external {
                            a.push(("TargetMode", "External".to_owned()));
                        }
                        r.empty("Relationship", &a);
                    }
                    r.close("Relationships");
                    let folder = if macro_sheet { "macrosheets" } else { "worksheets" };
                    extra_entries.push((format!("xl/{folder}/_rels/sheet{number}.xml.rels"), r.out));
                }
                if macro_sheet {
                    (
                        format!("xl/macrosheets/sheet{number}.xml"),
                        "application/vnd.ms-excel.macrosheet+xml",
                        read.xml,
                    )
                } else {
                    (
                        format!("xl/worksheets/sheet{number}.xml"),
                        "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml",
                        read.xml,
                    )
                }
            }
            SheetKind::Chart => {
                let mut chart = chart::Chart::default();
                let mut print_features = features::Features::default();
                print_features.v5 = v5;
                let mut print_records = [Vec::new(), Vec::new()];
                let mut margin_records = [Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()];
                let mut window_records = Vec::new();
                let mut custom_view_depth = 0usize;
                let mut depth = 1usize;
                let (mut chart_groups, mut supported_groups) = (0usize, 0usize);
                let mut unsupported_chart_type = false;
                let mut chart_size = None;
                let print = |rgce: &[u8]| -> Option<String> {
                    let context = links.context(Some(source_tab as u32));
                    match crate::xlsb::formula::print(rgce, &[], &context, crate::xlsb::formula::Origin::default()) {
                        Ok(Ok(text)) => Some(text),
                        _ => None,
                    }
                };
                for record in &substream[1..end - 1] {
                    if !matches!(record.id, 0x000C | 0x000D | 0x000F | 0x0010 | 0x0011 | 0x005F) {
                        *sheet_unknown.entry(record.id).or_insert(0) += 1;
                    }
                    match record.id {
                        workbook::BOF => depth += 1,
                        workbook::EOF => depth = depth.saturating_sub(1),
                        _ if depth == 1 => {
                            match record.id {
                                0x01AA => custom_view_depth += 1,
                                0x01AB => custom_view_depth = custom_view_depth.saturating_sub(1),
                                0x0014 | 0x0015 if custom_view_depth == 0 => print_records[usize::from(record.id - 0x0014)].push(record),
                                0x023E if custom_view_depth == 0 => window_records.push(record),
                                0x0026..=0x0029 if custom_view_depth == 0 => margin_records[usize::from(record.id - 0x0026)].push(record),
                                0x00A1 if custom_view_depth == 0 => margin_records[4].push(record),
                                _ => {},
                            }
                            match record.id {
                                0x1002 if record.body.len() >= 16 => {
                                    let fixed = |at: usize| i32::from_le_bytes(record.body[at..at + 4].try_into().expect("four bytes"));
                                    let (width, height) = (fixed(8), fixed(12));
                                    if width > 0 && height > 0 {
                                        chart_size = Some((i64::from(width) * 12_700 / 65_536, i64::from(height) * 12_700 / 65_536));
                                    }
                                }
                                0x1014 => chart_groups += 1,
                                0x1017..=0x101A => supported_groups += 1,
                                0x101B => {
                                    supported_groups += 1;
                                    // Scatter also describes bubble charts, which need a third value.
                                    if !(v5.is_some() && record.body.is_empty())
                                        && (record.body.len() < 6 || u16::from_le_bytes([record.body[4], record.body[5]]) & 1 != 0) {
                                        unsupported_chart_type = true;
                                    }
                                }
                                // 3-D, radar, surface, filled radar and pie-of-pie groups.
                                0x103A | 0x103E..=0x1040 | 0x1061 => unsupported_chart_type = true,
                                _ => {},
                            }
                            chart.read(record, &print, v5, &globals.styles, false);
                        },
                        _ => {},
                    }
                }
                for records in &print_records {
                    if let [record] = records.as_slice() {
                        if matches!(print_features.read(record, &globals.styles), Ok(true)) {
                            if let Some(count) = sheet_unknown.get_mut(&record.id) {
                                *count -= 1;
                                if *count == 0 { sheet_unknown.remove(&record.id); }
                            }
                        }
                    }
                }
                if let [record] = margin_records[4].as_slice() {
                    print_features.read_chart_setup(record);
                }
                // A chart sheet's pageMargins requires all four margins and the two in Setup.
                // Keep Setup unread: its other print settings are not converted here.
                let valid_margins = margin_records.iter().enumerate().all(|(index, records)| {
                    let [record] = records.as_slice() else { return false };
                    if !record.continues.is_empty() || record.body.len() != if index == 4 { 34 } else { 8 } {
                        return false;
                    }
                    let offsets: &[usize] = if index == 4 { &[16, 24] } else { &[0] };
                    offsets.iter().all(|&at| {
                        let value = f64::from_le_bytes(record.body[at..at + 8].try_into().expect("eight bytes"));
                        value.is_finite() && value >= 0.0 && if index == 4 { value < 49.0 } else { value <= 49.0 }
                    })
                });
                if valid_margins {
                    for records in &margin_records {
                        let record = records[0];
                        let _ = print_features.read(record, &globals.styles);
                        if record.id != 0x00A1 {
                            if let Some(count) = sheet_unknown.get_mut(&record.id) {
                                *count -= 1;
                                if *count == 0 { sheet_unknown.remove(&record.id); }
                            }
                        }
                    }
                }
                // On a chart sheet only fSelected is defined in the ten-byte Window2.
                let selected = match window_records.as_slice() {
                    [record] if record.body.len() == 10 && record.continues.is_empty() => {
                        if let Some(count) = sheet_unknown.get_mut(&record.id) {
                            *count -= 1;
                            if *count == 0 { sheet_unknown.remove(&record.id); }
                        }
                        Some(u16::from_le_bytes([record.body[0], record.body[1]]) & 0x0200 != 0)
                    }
                    _ => None,
                };
                let written_chart = (!unsupported_chart_type && chart_groups > 0 && chart_groups == supported_groups
                    && (v5.is_none() || chart.has_all_value_sources()))
                    .then(|| chart.write(&sheet.name)).flatten();
                let xml = if let Some(chart_xml) = written_chart {
                    let chart_part = format!("xl/charts/chart{number}_1.xml");
                    let drawing_part = format!("xl/drawings/drawing{number}.xml");
                    parts.push((chart_part, "application/vnd.openxmlformats-officedocument.drawingml.chart+xml", chart_xml));
                    parts.push((drawing_part, "application/vnd.openxmlformats-officedocument.drawing+xml", chartsheet_drawing(chart_size)));
                    extra_entries.push((format!("xl/chartsheets/_rels/sheet{number}.xml.rels"),
                        single_relationship("drawing", &format!("../drawings/drawing{number}.xml"))));
                    extra_entries.push((format!("xl/drawings/_rels/drawing{number}.xml.rels"),
                        single_relationship("chart", &format!("../charts/chart{number}_1.xml"))));
                    linked_chartsheet(&print_features, selected)
                } else { empty_chartsheet(&print_features, selected) };
                (
                    format!("xl/chartsheets/sheet{number}.xml"),
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.chartsheet+xml",
                    xml,
                )
            }
            SheetKind::Module | SheetKind::Other(_) => {
                return Err(refuse(name, format!("sheet {:?} of an unknown kind", sheet.name)));
            }
        };
        let sheet_id = globals.tab_ids.as_ref().map_or(number as u32, |ids| u32::from(ids[source_tab]));
        sheet_entries.push((sheet.name.clone(), sheet.state, part.clone(), sheet_id));
        parts.push((part, content_type, xml));
    }
    if sheet_entries.is_empty() {
        return Err(refuse(name, "the workbook has no sheets"));
    }
    let has_styles = globals.styles.needs_part();
    if has_styles {
        parts.push((
            "xl/styles.xml".to_owned(),
            "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml",
            globals.styles.write(),
        ));
    }
    let has_strings = !globals.strings.is_empty();
    if has_strings {
        let mut sst = Xml::new();
        sst.open(
            "sst",
            &[
                ("xmlns", MAIN_NAMESPACE.to_owned()),
                ("count", globals.strings.len().to_string()),
                ("uniqueCount", globals.strings.len().to_string()),
            ],
        );
        for string in &globals.strings {
            sst.open("si", &[]);
            if string.runs.is_empty() || globals.styles.is_empty() {
                sheet::text_element(&mut sst, &string.units);
            } else {
                // Rich text: each run from its start to the next one's, in its font; text before
                // the first run is in the default font (font 0).
                let mut starts: Vec<(usize, u16)> = Vec::new();
                if string.runs[0].0 != 0 {
                    starts.push((0, 0));
                }
                starts.extend(string.runs.iter().map(|(at, font)| (usize::from(*at), *font)));
                for (k, (at, font)) in starts.iter().enumerate() {
                    let end = starts.get(k + 1).map_or(string.units.len(), |next| next.0);
                    let (at, end) = (*at.min(&string.units.len()), end.min(string.units.len()));
                    if at >= end {
                        continue;
                    }
                    sst.open("r", &[]);
                    globals.styles.write_run_properties(&mut sst, *font);
                    sheet::text_element(&mut sst, &string.units[at..end]);
                    sst.close("r");
                }
            }
            if let Some(p) = &string.phonetic {
                let font = if p.font == 4 { None } else { globals.styles.font_index(p.font).or((p.font == 0).then_some(0)) };
                if let Some(font) = font {
                    for run in &p.runs {
                        sst.open("rPh", &[("sb", run.first.to_string()), ("eb", run.end.to_string())]);
                        sheet::text_element(&mut sst, &run.text);
                        sst.close("rPh");
                    }
                    let kind = ["halfwidthKatakana", "fullwidthKatakana", "Hiragana", "noConversion"][p.kind as usize];
                    let alignment = ["noControl", "left", "center", "distributed"][p.alignment as usize];
                    sst.empty("phoneticPr", &[("fontId", font.to_string()), ("type", kind.to_owned()), ("alignment", alignment.to_owned())]);
                }
            }
            sst.close("si");
        }
        sst.close("sst");
        parts.push((
            "xl/sharedStrings.xml".to_owned(),
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml",
            sst.out,
        ));
    }
    // The external workbooks the formulas name, as external link parts.
    let mut external_links: Vec<String> = Vec::new();
    for (book_index, book) in globals.supbooks.iter().enumerate() {
        let workbook::SupBookRecord::External { path, sheets, names } = book else {
            continue;
        };
        let number = external_links.len() + 1;
        let mut x = Xml::new();
        x.open(
            "externalLink",
            &[
                ("xmlns", MAIN_NAMESPACE.to_owned()),
                ("xmlns:r", RELATIONSHIPS_NAMESPACE.to_owned()),
            ],
        );
        x.open("externalBook", &[("r:id", "rId1".to_owned())]);
        if !sheets.is_empty() {
            x.open("sheetNames", &[]);
            for sheet in sheets {
                x.empty("sheetName", &[("val", sheet.clone())]);
            }
            x.close("sheetNames");
        }
        if !names.is_empty() {
            x.open("definedNames", &[]);
            for name in names {
                x.empty("definedName", &[("name", name.clone())]);
            }
            x.close("definedNames");
        }
        let caches: Vec<_> = globals.external_cache.iter().filter(|((book, _), _)| *book == book_index).collect();
        if !caches.is_empty() {
            x.open("sheetDataSet", &[]);
            for ((_, sheet_index), cache) in caches {
                let mut attributes = vec![("sheetId", sheet_index.to_string())];
                if cache.refresh_error { attributes.push(("refreshError", "1".to_owned())); }
                x.open("sheetData", &attributes);
                let mut open_row = None;
                for (&(row, column), value) in &cache.cells {
                    if matches!(value, external_cache::Value::Empty) { continue; }
                    if open_row != Some(row) {
                        if open_row.is_some() { x.close("row"); }
                        x.open("row", &[("r", (u32::from(row) + 1).to_string())]);
                        open_row = Some(row);
                    }
                    let (kind, text): (&str, Vec<u16>) = match value {
                        external_cache::Value::Empty => unreachable!(),
                        external_cache::Value::Number(v) => ("n", crate::xlsb::formula::number_text(*v).encode_utf16().collect()),
                        external_cache::Value::Text(v) => ("str", v.clone()),
                        external_cache::Value::Boolean(v) => ("b", if *v { "1" } else { "0" }.encode_utf16().collect()),
                        external_cache::Value::Error(v) => ("e", v.encode_utf16().collect()),
                    };
                    x.open("cell", &[("r", format!("{}{}", sheet::column_name(u32::from(column)), u32::from(row) + 1)), ("t", kind.to_owned())]);
                    x.open("v", &[]); x.text(&text); x.close("v"); x.close("cell");
                }
                if open_row.is_some() { x.close("row"); }
                x.close("sheetData");
            }
            x.close("sheetDataSet");
        }
        x.close("externalBook");
        x.close("externalLink");
        let part = format!("xl/externalLinks/externalLink{number}.xml");
        let mut r = Xml::new();
        r.open(
            "Relationships",
            &[(
                "xmlns",
                "http://schemas.openxmlformats.org/package/2006/relationships".to_owned(),
            )],
        );
        r.empty(
            "Relationship",
            &[
                ("Id", "rId1".to_owned()),
                ("Type", format!("{REL_TYPE}/externalLinkPath")),
                ("Target", path.clone()),
                ("TargetMode", "External".to_owned()),
            ],
        );
        r.close("Relationships");
        extra_entries.push((format!("xl/externalLinks/_rels/externalLink{number}.xml.rels"), r.out));
        parts.push((
            part.clone(),
            "application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml",
            x.out,
        ));
        external_links.push(part);
    }
    // Preserve complete local pivot-cache source records through the existing pivot package
    // writer. The views are still unread, so their SXVIEW records remain in the report.
    let mut pivot_caches = Vec::new();
    if !biff5 {
        use calc_doc_model::pivot::{PivotCache, PivotSnapshot, PivotSnapshotOrigin};
        use std::sync::Arc;
        let globals_records = &records[..globals.end];
        let ids: Vec<_> = globals_records.iter().enumerate().filter(|(_, r)| r.id == 0x00D5).collect();
        for (cache_id, (position, id_record)) in ids.iter().enumerate() {
            if id_record.body.len() != 2 || !id_record.continues.is_empty() { continue; }
            let id = u16::from_le_bytes([id_record.body[0], id_record.body[1]]);
            if ids.iter().filter(|(_, r)| r.body == id_record.body).count() != 1 { continue; }
            let (Some(kind), Some(source)) = (globals_records.get(position + 1), globals_records.get(position + 2)) else { continue };
            if kind.id != 0x00E3 || kind.body != [1, 0] || !kind.continues.is_empty() { continue; }
            let Some(source) = pivot_cache::source(source, &globals.sheets, &globals.names) else { continue };
            let Ok(Some(bytes)) = compound.read(&format!("_SX_DB_CUR/{id:04X}")) else { continue };
            let bytes = if decrypted {
                let Some(filepass) = records.iter().find(|r| r.id == 0x002F) else { continue };
                if !filepass.continues.is_empty() { continue; }
                let Ok(plain) = crypto::decrypt_pivot_cache(&bytes, filepass.body,
                    password.unwrap_or("VelvetSweatshop")) else { continue };
                plain
            } else { bytes };
            let Some(parsed) = pivot_cache::read(&bytes, id) else { continue };
            let (sheet, range, source_name) = match source {
                pivot_cache::Source::Range { sheet, range, columns } if parsed.fields.len() == columns =>
                    (Some(sheet), Some(range), None),
                pivot_cache::Source::WorkbookName(name) => (None, None, Some(name)),
                _ => continue,
            };
            let number = cache_id + 1;
            let part = format!("xl/pivotCache/pivotCacheDefinition{number}.xml");
            let records_part = format!("xl/pivotCache/pivotCacheRecords{number}.xml");
            let mut attributes = BTreeMap::from([("refreshOnLoad".into(), u8::from(parsed.refresh_on_load).to_string())]);
            if !parsed.enable_refresh {
                attributes.insert("enableRefresh".into(), "0".into());
            }
            if let Some(user) = parsed.refreshed_by { attributes.insert("refreshedBy".into(), user); }
            if !globals.date_1904 {
                if let Some(date) = parsed.refreshed_date { attributes.insert("refreshedDate".into(), date); }
            }
            let cache = PivotCache {
                id: cache_id as u32, part: part.clone(), records_part: Some(records_part.clone()),
                record_count: Some(parsed.rows.len() as u64), source_type: "worksheet".into(),
                source_connection_id: None, source_sheet: sheet, source_range: range, source_name,
                fields: parsed.fields, calculated_items: Vec::new(),
                attributes,
                unsupported: Vec::new(),
                snapshot: Some(Arc::new(PivotSnapshot { records: parsed.rows,
                    origin: PivotSnapshotOrigin::FileRecords { part: records_part.clone() },
                    source_revision: 0, numeric: Default::default() })),
            };
            let Ok((definition, data)) = crate::save::pivot::legacy_cache_parts(&cache) else { continue };
            parts.push((part.clone(), "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml", definition));
            parts.push((records_part, "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheRecords+xml", data));
            let mut r = Xml::new();
            r.open("Relationships", &[("xmlns", "http://schemas.openxmlformats.org/package/2006/relationships".to_owned())]);
            r.empty("Relationship", &[("Id", "rId1".to_owned()), ("Type", format!("{REL_TYPE}/pivotCacheRecords")),
                ("Target", format!("pivotCacheRecords{number}.xml"))]);
            r.close("Relationships");
            extra_entries.push((format!("xl/pivotCache/_rels/pivotCacheDefinition{number}.xml.rels"), r.out));
            pivot_caches.push((cache_id, part));
        }
    }
    // The VBA project: the `_VBA_PROJECT_CUR` storage becomes the compound file of its own an
    // .xlsm keeps in `xl/vbaProject.bin`.
    let vba = compound
        .extract_storage("_VBA_PROJECT_CUR")
        .map_err(|e| refuse("_VBA_PROJECT_CUR", e.0))?;
    if let Some(bytes) = &vba {
        parts.push((
            "xl/vbaProject.bin".to_owned(),
            "application/vnd.ms-office.vbaProject",
            bytes.clone(),
        ));
    }
    let macros = vba.is_some() || globals.sheets.iter().any(|s| s.kind == SheetKind::MacroSheet);
    if has_core_properties {
        let mut core = Xml::new();
        core.open("cp:coreProperties", &[("xmlns:cp", "http://schemas.openxmlformats.org/package/2006/metadata/core-properties".to_owned()),
            ("xmlns:dc", "http://purl.org/dc/elements/1.1/".to_owned()),
            ("xmlns:dcterms", "http://purl.org/dc/terms/".to_owned()),
            ("xmlns:xsi", "http://www.w3.org/2001/XMLSchema-instance".to_owned())]);
        for (id, name) in [(2, "dc:title"), (3, "dc:subject"), (4, "dc:creator"), (5, "cp:keywords"),
            (6, "dc:description"), (8, "cp:lastModifiedBy"), (9, "cp:revision")] {
            if let Some(text) = summary.get(&id) {
                core.open(name, &[]); core.xml_text_str(text); core.close(name);
            }
        }
        for (id, name) in [(12, "dcterms:created"), (13, "dcterms:modified")] {
            if let Some(text) = summary.get(&id) {
                core.element(name, &[("xsi:type", "dcterms:W3CDTF".to_owned())], text);
            }
        }
        if let Some(text) = document_properties.get(&2) {
            core.open("cp:category", &[]); core.xml_text_str(text); core.close("cp:category");
        }
        core.close("cp:coreProperties");
        parts.push(("docProps/core.xml".to_owned(), "application/vnd.openxmlformats-package.core-properties+xml", core.out));
    }
    if has_extended_properties {
        let mut app = Xml::new();
        app.open("Properties", &[("xmlns", "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties".to_owned())]);
        for (name, text) in [("Application", summary.get(&18)), ("Manager", document_properties.get(&14)),
            ("Company", document_properties.get(&15))] {
            if let Some(text) = text { app.open(name, &[]); app.xml_text_str(text); app.close(name); }
        }
        app.close("Properties");
        parts.push(("docProps/app.xml".to_owned(), "application/vnd.openxmlformats-officedocument.extended-properties+xml", app.out));
    }
    if !custom_properties.is_empty() {
        let mut custom = Xml::new();
        custom.open("Properties", &[("xmlns", "http://schemas.openxmlformats.org/officeDocument/2006/custom-properties".to_owned()),
            ("xmlns:vt", "http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes".to_owned())]);
        for (index, property) in custom_properties.iter().enumerate() {
            custom.open("property", &[("fmtid", "{D5CDD505-2E9C-101B-9397-08002B2CF9AE}".to_owned()),
                ("pid", (index + 2).to_string()), ("name", property.name.clone())]);
            custom.open(property.kind, &[]); custom.xml_text_str(&property.value); custom.close(property.kind);
            custom.close("property");
        }
        custom.close("Properties");
        parts.push(("docProps/custom.xml".to_owned(), "application/vnd.openxmlformats-officedocument.custom-properties+xml", custom.out));
    }
    let report = LegacyReport {
        biff_version: if biff5 { 5 } else { 8 },
        sheets: globals.sheets.len(),
        unread_objects,
        unread_custom_property_types,
        unread_custom_property_sets,
        unknown_records: {
            let mut all = std::mem::take(&mut globals.unknown);
            for (id, n) in sheet_unknown {
                *all.entry(id).or_insert(0) += n;
            }
            for (id, n) in v5_cache_consumed {
                if let Some(count) = all.get_mut(&id) {
                    *count = count.saturating_sub(n);
                    if *count == 0 { all.remove(&id); }
                }
            }
            all
        },
    };

    // xl/workbook.xml
    let mut xml = Xml::new();
    xml.open(
        "workbook",
        &[
            ("xmlns", MAIN_NAMESPACE.to_owned()),
            ("xmlns:r", RELATIONSHIPS_NAMESPACE.to_owned()),
        ],
    );
    let mut workbook_properties = Vec::new();
    if globals.date_1904 {
        workbook_properties.push(("date1904", "1".to_owned()));
    }
    if globals.backup_file { workbook_properties.push(("backupFile", "1".to_owned())); }
    if globals.refresh_all { workbook_properties.push(("refreshAllConnections", "1".to_owned())); }
    if let Some(flags) = globals.book_bool {
        if flags & 1 != 0 { workbook_properties.push(("saveExternalLinkValues", "0".to_owned())); }
        if flags & 0x100 != 0 { workbook_properties.push(("showBorderUnselectedTables", "0".to_owned())); }
        match (flags >> 5) & 3 {
            1 => workbook_properties.push(("updateLinks", "never".to_owned())),
            2 => workbook_properties.push(("updateLinks", "always".to_owned())),
            _ => {}
        }
    }
    if let Some(code_name) = &globals.code_name {
        workbook_properties.push(("codeName", code_name.clone()));
    }
    match globals.hide_objects {
        Some(1) => workbook_properties.push(("showObjects", "placeholders".to_owned())),
        Some(2) => workbook_properties.push(("showObjects", "none".to_owned())),
        _ => {}
    }
    xml.empty("workbookPr", &workbook_properties);
    if globals.protect_structure || globals.protect_windows || globals.protection_verifier.is_some() {
        let mut protection = Vec::new();
        if globals.protect_structure { protection.push(("lockStructure", "1".to_owned())); }
        if globals.protect_windows { protection.push(("lockWindows", "1".to_owned())); }
        if let Some(verifier) = globals.protection_verifier {
            protection.push(("workbookPassword", format!("{verifier:04X}")));
        }
        xml.empty("workbookProtection", &protection);
    }
    if let Some(tab) = globals.active_tab.filter(|t| usize::from(*t) < sheet_entries.len()) {
        xml.open("bookViews", &[]);
        xml.empty("workbookView", &[("activeTab", tab.to_string())]);
        xml.close("bookViews");
    }
    xml.open("sheets", &[]);
    for (index, (sheet_name, state, _, sheet_id)) in sheet_entries.iter().enumerate() {
        let mut attributes = vec![
            ("name", sheet_name.clone()),
            ("sheetId", sheet_id.to_string()),
        ];
        match state {
            1 => attributes.push(("state", "hidden".to_owned())),
            2 => attributes.push(("state", "veryHidden".to_owned())),
            _ => {}
        }
        attributes.push(("r:id", format!("rId{}", index + 1)));
        xml.empty("sheet", &attributes);
    }
    xml.close("sheets");
    if !external_links.is_empty() {
        xml.open("externalReferences", &[]);
        for k in 0..external_links.len() {
            xml.empty("externalReference", &[("r:id", format!("rIdLink{}", k + 1))]);
        }
        xml.close("externalReferences");
    }
    let listed: Vec<&workbook::NameRecord> = globals
        .names
        .iter()
        .filter(|n| !n.rgce.is_empty() && !n.name.starts_with(calc_catalogue::PARAMETER_PREFIX))
        .collect();
    let stream_name = name;
    if !listed.is_empty() {
        xml.open("definedNames", &[]);
        for name in listed {
            let context = links.context(name.scope);
            let text = match crate::xlsb::formula::print(
                &name.rgce,
                &name.rgcb,
                &context,
                crate::xlsb::formula::Origin::default(),
            ) {
                Ok(Ok(text)) => text,
                Ok(Err(_)) => return Err(refuse(stream_name, "a defined name holding a PtgExp")),
                Err(error) => {
                    return Err(refuse(
                        stream_name,
                        format!("the defined name {:?}: {}", name.name, error.0),
                    ));
                }
            };
            let mut attributes = vec![("name", name.name.clone())];
            if let Some(description) = &name.description {
                attributes.push(("description", description.clone()));
            }
            if let Some(help) = &name.help {
                attributes.push(("help", help.clone()));
            }
            if let Some(scope) = name.scope {
                let source = globals.sheets.get(scope as usize)
                    .ok_or_else(|| refuse(stream_name, format!("the sheet {scope} of a defined name")))?;
                if source.kind == SheetKind::Module {
                    return Err(refuse(stream_name, format!("a name scoped to omitted module sheet {scope}")));
                }
                let output_scope = globals.sheets[..scope as usize].iter().filter(|s| s.kind != SheetKind::Module).count();
                attributes.push(("localSheetId", output_scope.to_string()));
            }
            if name.flags & 0x0001 != 0 {
                attributes.push(("hidden", "1".to_owned()));
            }
            // fFunc (bit 1), fOB (bit 2, a VBA procedure) and fProc (bit 3, a macro).
            let (function, visual_basic, procedure) = (
                name.flags & 0x0002 != 0,
                name.flags & 0x0004 != 0,
                name.flags & 0x0008 != 0,
            );
            if function {
                attributes.push(("function", "1".to_owned()));
            }
            if visual_basic {
                attributes.push(("vbProcedure", "1".to_owned()));
            }
            if procedure && !visual_basic {
                attributes.push(("xlm", "1".to_owned()));
            }
            let group = (name.flags >> 6) & 0xFF;
            if procedure && group != 0 {
                attributes.push(("functionGroupId", group.to_string()));
            }
            if name.key != 0 {
                attributes.push(("shortcutKey", char::from(name.key).to_string()));
            }
            xml.element("definedName", &attributes, &text);
        }
        xml.close("definedNames");
    }
    let mut calc_properties = Vec::new();
    if let Some(mode) = calculation_mode { calc_properties.push(("calcMode", mode.to_owned())); }
    if ref_a1 == Some(false) { calc_properties.push(("refMode", "R1C1".to_owned())); }
    if let Some(save) = calc_on_save { calc_properties.push(("calcOnSave", if save { "1" } else { "0" }.to_owned())); }
    if let Some(enabled) = iterate { calc_properties.push(("iterate", if enabled { "1" } else { "0" }.to_owned())); }
    if let Some(count) = iteration_count { calc_properties.push(("iterateCount", count.to_string())); }
    if let Some(delta) = iteration_delta { calc_properties.push(("iterateDelta", f64::from_bits(delta).to_string())); }
    if globals.full_precision == Some(false) { calc_properties.push(("fullPrecision", "0".to_owned())); }
    if !calc_properties.is_empty() {
        xml.empty("calcPr", &calc_properties);
    }
    if !pivot_caches.is_empty() {
        xml.open("pivotCaches", &[]);
        for (id, _) in &pivot_caches {
            xml.empty("pivotCache", &[("cacheId", id.to_string()), ("r:id", format!("rIdPivot{id}"))]);
        }
        xml.close("pivotCaches");
    }
    xml.close("workbook");
    let workbook_xml = xml.out;

    // xl/_rels/workbook.xml.rels
    let mut rels = Xml::new();
    rels.open(
        "Relationships",
        &[(
            "xmlns",
            "http://schemas.openxmlformats.org/package/2006/relationships".to_owned(),
        )],
    );
    for (index, (_, _, part, _)) in sheet_entries.iter().enumerate() {
        let kind = if part.starts_with("xl/macrosheets/") {
            "http://schemas.microsoft.com/office/2006/relationships/xlMacrosheet"
        } else if part.starts_with("xl/chartsheets/") {
            "chartsheet"
        } else {
            "worksheet"
        };
        let kind = if kind.starts_with("http") {
            kind.to_owned()
        } else {
            format!("{REL_TYPE}/{kind}")
        };
        rels.empty(
            "Relationship",
            &[
                ("Id", format!("rId{}", index + 1)),
                ("Type", kind),
                ("Target", part.trim_start_matches("xl/").to_owned()),
            ],
        );
    }
    for (k, part) in external_links.iter().enumerate() {
        rels.empty(
            "Relationship",
            &[
                ("Id", format!("rIdLink{}", k + 1)),
                ("Type", format!("{REL_TYPE}/externalLink")),
                ("Target", part.trim_start_matches("xl/").to_owned()),
            ],
        );
    }
    for (id, part) in &pivot_caches {
        rels.empty("Relationship", &[("Id", format!("rIdPivot{id}")), ("Type", format!("{REL_TYPE}/pivotCacheDefinition")),
            ("Target", part.trim_start_matches("xl/").to_owned())]);
    }
    if vba.is_some() {
        rels.empty(
            "Relationship",
            &[
                ("Id", "rIdVba".to_owned()),
                ("Type", "http://schemas.microsoft.com/office/2006/relationships/vbaProject".to_owned()),
                ("Target", "vbaProject.bin".to_owned()),
            ],
        );
    }
    if has_styles {
        rels.empty(
            "Relationship",
            &[
                ("Id", "rIdStyles".to_owned()),
                ("Type", format!("{REL_TYPE}/styles")),
                ("Target", "styles.xml".to_owned()),
            ],
        );
    }
    if has_strings {
        rels.empty(
            "Relationship",
            &[
                ("Id", "rIdStrings".to_owned()),
                ("Type", format!("{REL_TYPE}/sharedStrings")),
                ("Target", "sharedStrings.xml".to_owned()),
            ],
        );
    }
    rels.close("Relationships");

    let mut root_rels = Xml::new();
    root_rels.open(
        "Relationships",
        &[(
            "xmlns",
            "http://schemas.openxmlformats.org/package/2006/relationships".to_owned(),
        )],
    );
    root_rels.empty(
        "Relationship",
        &[
            ("Id", "rId1".to_owned()),
            ("Type", format!("{REL_TYPE}/officeDocument")),
            ("Target", "xl/workbook.xml".to_owned()),
        ],
    );
    if has_core_properties {
        root_rels.empty("Relationship", &[("Id", "rIdCore".to_owned()),
            ("Type", "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties".to_owned()),
            ("Target", "docProps/core.xml".to_owned())]);
    }
    if has_extended_properties {
        root_rels.empty("Relationship", &[("Id", "rIdApp".to_owned()),
            ("Type", format!("{REL_TYPE}/extended-properties")), ("Target", "docProps/app.xml".to_owned())]);
    }
    if !custom_properties.is_empty() {
        root_rels.empty("Relationship", &[("Id", "rIdCustom".to_owned()),
            ("Type", format!("{REL_TYPE}/custom-properties")), ("Target", "docProps/custom.xml".to_owned())]);
    }
    root_rels.close("Relationships");

    let mut types = Xml::new();
    types.open(
        "Types",
        &[(
            "xmlns",
            "http://schemas.openxmlformats.org/package/2006/content-types".to_owned(),
        )],
    );
    types.empty(
        "Default",
        &[
            ("Extension", "rels".to_owned()),
            (
                "ContentType",
                "application/vnd.openxmlformats-package.relationships+xml".to_owned(),
            ),
        ],
    );
    types.empty(
        "Default",
        &[
            ("Extension", "xml".to_owned()),
            ("ContentType", "application/xml".to_owned()),
        ],
    );
    let main = if macros {
        "application/vnd.ms-excel.sheet.macroEnabled.main+xml"
    } else {
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"
    };
    types.empty(
        "Override",
        &[
            ("PartName", "/xl/workbook.xml".to_owned()),
            ("ContentType", main.to_owned()),
        ],
    );
    for (part, content_type, _) in &parts {
        types.empty(
            "Override",
            &[
                ("PartName", format!("/{part}")),
                ("ContentType", (*content_type).to_owned()),
            ],
        );
    }
    types.close("Types");

    let mut entries: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", &types.out),
        ("_rels/.rels", &root_rels.out),
        ("xl/workbook.xml", &workbook_xml),
        ("xl/_rels/workbook.xml.rels", &rels.out),
    ];
    entries.extend(parts.iter().map(|(n, _, c)| (n.as_str(), c.as_slice())));
    entries.extend(extra_entries.iter().map(|(n, c)| (n.as_str(), c.as_slice())));
    let archive = crate::save::new_archive(&entries)
        .ok_or_else(|| refuse(name, "the converted package is too large for a ZIP archive"))?;
    Ok((archive, report))
}

fn single_relationship(kind: &str, target: &str) -> Vec<u8> {
    let mut xml = Xml::new();
    xml.open("Relationships", &[("xmlns", "http://schemas.openxmlformats.org/package/2006/relationships".to_owned())]);
    xml.empty("Relationship", &[("Id", "rId1".to_owned()), ("Type", format!("{REL_TYPE}/{kind}")), ("Target", target.to_owned())]);
    xml.close("Relationships");
    xml.out
}

fn linked_chartsheet(features: &features::Features, selected: Option<bool>) -> Vec<u8> {
    let mut xml = Xml::new();
    xml.open("chartsheet", &[("xmlns", MAIN_NAMESPACE.to_owned()), ("xmlns:r", RELATIONSHIPS_NAMESPACE.to_owned())]);
    xml.open("sheetViews", &[]);
    let mut attributes = vec![("workbookViewId", "0".to_owned()), ("zoomToFit", "1".to_owned())];
    if selected == Some(true) { attributes.push(("tabSelected", "1".to_owned())); }
    xml.empty("sheetView", &attributes);
    xml.close("sheetViews");
    features.write_chart_margins(&mut xml);
    features.write_chart_setup(&mut xml);
    features.write_header_footer(&mut xml);
    xml.empty("drawing", &[("r:id", "rId1".to_owned())]);
    xml.close("chartsheet");
    xml.out
}

fn chartsheet_drawing(size: Option<(i64, i64)>) -> Vec<u8> {
    // The BIFF chart size is in 16.16 points; a missing size uses default drawing extents.
    let (width, height) = size.unwrap_or((9293678, 6000000));
    let mut xml = Xml::new();
    xml.open("xdr:wsDr", &[("xmlns:xdr", "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing".to_owned()),
        ("xmlns:a", "http://schemas.openxmlformats.org/drawingml/2006/main".to_owned()),
        ("xmlns:c", "http://schemas.openxmlformats.org/drawingml/2006/chart".to_owned()),
        ("xmlns:r", RELATIONSHIPS_NAMESPACE.to_owned())]);
    xml.open("xdr:absoluteAnchor", &[]);
    xml.empty("xdr:pos", &[("x", "0".to_owned()), ("y", "0".to_owned())]);
    xml.empty("xdr:ext", &[("cx", width.to_string()), ("cy", height.to_string())]);
    xml.open("xdr:graphicFrame", &[("macro", String::new())]);
    xml.open("xdr:nvGraphicFramePr", &[]);
    xml.empty("xdr:cNvPr", &[("id", "2".to_owned()), ("name", "Chart 1".to_owned())]);
    xml.empty("xdr:cNvGraphicFramePr", &[]);
    xml.close("xdr:nvGraphicFramePr");
    xml.open("xdr:xfrm", &[]);
    xml.empty("a:off", &[("x", "0".to_owned()), ("y", "0".to_owned())]);
    xml.empty("a:ext", &[("cx", "0".to_owned()), ("cy", "0".to_owned())]);
    xml.close("xdr:xfrm");
    xml.open("a:graphic", &[]);
    xml.open("a:graphicData", &[("uri", "http://schemas.openxmlformats.org/drawingml/2006/chart".to_owned())]);
    xml.empty("c:chart", &[("r:id", "rId1".to_owned())]);
    xml.close("a:graphicData"); xml.close("a:graphic"); xml.close("xdr:graphicFrame");
    xml.empty("xdr:clientData", &[]);
    xml.close("xdr:absoluteAnchor"); xml.close("xdr:wsDr");
    xml.out
}

fn empty_chartsheet(features: &features::Features, selected: Option<bool>) -> Vec<u8> {
    let mut xml = Xml::new();
    xml.open("chartsheet", &[("xmlns", MAIN_NAMESPACE.to_owned())]);
    if let Some(selected) = selected {
        xml.open("sheetViews", &[]);
        let mut attributes = vec![("workbookViewId", "0".to_owned())];
        if selected { attributes.push(("tabSelected", "1".to_owned())); }
        xml.empty("sheetView", &attributes);
        xml.close("sheetViews");
    }
    features.write_chart_margins(&mut xml);
    features.write_chart_setup(&mut xml);
    features.write_header_footer(&mut xml);
    xml.close("chartsheet");
    xml.out
}

/// The range of the automatic filter of the sheet at tab index `tab`: the formula of its
/// `_FilterDatabase` name without the sheet's name.
fn filter_range(globals: &workbook::Globals, links: &formula::Links, tab: u32) -> Option<String> {
    let name = globals
        .names
        .iter()
        .find(|n| n.name == "_xlnm._FilterDatabase" && n.scope == Some(tab))?;
    let context = links.context(Some(tab));
    let text = match crate::xlsb::formula::print(
        &name.rgce,
        &name.rgcb,
        &context,
        crate::xlsb::formula::Origin::default(),
    ) {
        Ok(Ok(text)) => text,
        _ => return None,
    };
    let range = text.rsplit_once('!').map_or(text.as_str(), |(_, range)| range);
    Some(range.replace('$', ""))
}
