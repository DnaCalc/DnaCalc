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
mod records;
mod sheet;
mod strings;
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
    /// The BIFF version: 8 (BIFF5/7 is refused until dc-4j0e.8).
    pub biff_version: u8,
    /// The sheets of the workbook, of every kind.
    pub sheets: usize,
    /// Record types not read, with how many of each.
    pub unknown_records: BTreeMap<u16, u32>,
}

/// Whether `bytes` start as an OLE compound file.
pub(crate) fn is_legacy(bytes: &[u8]) -> bool {
    cfb::is_compound(bytes)
}

fn refuse(stream: &str, detail: impl Into<String>) -> OpenError {
    OpenError::LegacyWorkbook {
        stream: stream.to_owned(),
        detail: detail.into(),
    }
}

/// The SpreadsheetML package (the bytes of a ZIP archive) the legacy workbook in `bytes` stands
/// for, with what was found reading it.
pub(crate) fn convert(bytes: &[u8]) -> Result<(Vec<u8>, LegacyReport), OpenError> {
    let compound = cfb::Compound::open(bytes).map_err(|e| refuse("(compound file)", e.0))?;
    let name = ["Workbook", "Book"]
        .into_iter()
        .find(|name| compound.find(name).is_some())
        .ok_or_else(|| refuse("(compound file)", "no Workbook or Book stream"))?;
    let stream = compound
        .read(name)
        .map_err(|e| refuse(name, e.0))?
        .expect("found above");
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
    match version {
        0x0600 => {}
        0x0500 => {
            return Err(refuse(
                name,
                "a BIFF5/7 workbook, which dc-4j0e.8 reads",
            ));
        }
        other => return Err(refuse(name, format!("BIFF version {other:#06x}"))),
    }
    let mut globals = workbook::read(&records).map_err(|e| refuse(name, e))?;
    if globals.encrypted {
        return Err(refuse(name, "an encrypted workbook (FILEPASS), which dc-4j0e.7 reads"));
    }
    // Each sheet's substream: its BOF at the BOUNDSHEET offset, to its EOF.
    let mut parts: Vec<(String, &'static str, Vec<u8>)> = Vec::new();
    let mut sheet_entries: Vec<(String, u8, String)> = Vec::new();
    let mut number = 0usize;
    let mut sheet_unknown: BTreeMap<u16, u32> = BTreeMap::new();
    for sheet in &globals.sheets {
        if sheet.kind == SheetKind::Module {
            continue;
        }
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
        let mut end = None;
        for (k, record) in substream.iter().enumerate() {
            match record.id {
                workbook::BOF => depth += 1,
                workbook::EOF => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(k + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else {
            return Err(refuse(name, format!("sheet {:?} has no EOF record", sheet.name)));
        };
        number += 1;
        let context = sheet::Context {
            strings: &globals.strings,
        };
        let (part, content_type, xml) = match sheet.kind {
            SheetKind::MacroSheet | SheetKind::Worksheet => {
                let macro_sheet = sheet.kind == SheetKind::MacroSheet;
                let read = sheet::read(&substream[..end], &context, macro_sheet, &sheet.name)
                    .map_err(|detail| refuse(name, detail))?;
                for (id, n) in read.unknown {
                    *sheet_unknown.entry(id).or_insert(0) += n;
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
                for record in &substream[1..end - 1] {
                    *sheet_unknown.entry(record.id).or_insert(0) += 1;
                }
                (
                    format!("xl/chartsheets/sheet{number}.xml"),
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.chartsheet+xml",
                    empty_chartsheet(),
                )
            }
            SheetKind::Module | SheetKind::Other(_) => {
                return Err(refuse(name, format!("sheet {:?} of an unknown kind", sheet.name)));
            }
        };
        sheet_entries.push((sheet.name.clone(), sheet.state, part.clone()));
        parts.push((part, content_type, xml));
    }
    if sheet_entries.is_empty() {
        return Err(refuse(name, "the workbook has no sheets"));
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
            sheet::text_element(&mut sst, &string.units);
            sst.close("si");
        }
        sst.close("sst");
        parts.push((
            "xl/sharedStrings.xml".to_owned(),
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml",
            sst.out,
        ));
    }
    let macros = globals.sheets.iter().any(|s| s.kind == SheetKind::MacroSheet);
    let report = LegacyReport {
        biff_version: 8,
        sheets: globals.sheets.len(),
        unknown_records: {
            let mut all = std::mem::take(&mut globals.unknown);
            for (id, n) in sheet_unknown {
                *all.entry(id).or_insert(0) += n;
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
    xml.empty(
        "workbookPr",
        &if globals.date_1904 {
            vec![("date1904", "1".to_owned())]
        } else {
            Vec::new()
        },
    );
    if let Some(tab) = globals.active_tab.filter(|t| usize::from(*t) < sheet_entries.len()) {
        xml.open("bookViews", &[]);
        xml.empty("workbookView", &[("activeTab", tab.to_string())]);
        xml.close("bookViews");
    }
    xml.open("sheets", &[]);
    for (index, (sheet_name, state, _)) in sheet_entries.iter().enumerate() {
        let mut attributes = vec![
            ("name", sheet_name.clone()),
            ("sheetId", (index + 1).to_string()),
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
    for (index, (_, _, part)) in sheet_entries.iter().enumerate() {
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
    let archive = crate::save::new_archive(&entries)
        .ok_or_else(|| refuse(name, "the converted package is too large for a ZIP archive"))?;
    Ok((archive, report))
}

fn empty_chartsheet() -> Vec<u8> {
    let mut xml = Xml::new();
    xml.open("chartsheet", &[("xmlns", MAIN_NAMESPACE.to_owned())]);
    xml.close("chartsheet");
    xml.out
}
