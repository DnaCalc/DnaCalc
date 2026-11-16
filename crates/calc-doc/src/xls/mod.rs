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
pub(crate) mod codepage;
mod crypto;
mod features;
mod formula;
mod records;
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
/// `password` opens a workbook that asks for one; without it the password Excel itself uses for
/// a workbook saved read-only recommended ("VelvetSweatshop") is tried.
pub(crate) fn convert(
    bytes: &[u8],
    password: Option<&str>,
) -> Result<(Vec<u8>, LegacyReport), OpenError> {
    let compound = cfb::Compound::open(bytes).map_err(|e| refuse("(compound file)", e.0))?;
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
    let mut sheet_entries: Vec<(String, u8, String)> = Vec::new();
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
    let mut registry = v5::Registry::default();
    let mut tables: Vec<v5::Table> = Vec::new();
    let mut global_table = v5::Table::default();
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
    }
    let mut links = formula::Links::new(
        &globals,
        globals
            .sheets
            .iter()
            .filter(|s| s.kind != SheetKind::Module)
            .map(|s| s.name.clone())
            .collect(),
        v5,
    );
    if v5.is_some() {
        let (books, xti) = global_table.references(&registry.documents);
        links = links.with_table(books, xti);
    }
    let mut sheet_unknown: BTreeMap<u16, u32> = BTreeMap::new();
    let mut extra_entries: Vec<(String, Vec<u8>)> = Vec::new();
    for sheet in &globals.sheets {
        if sheet.kind == SheetKind::Module {
            continue;
        }
        let (start, end) = locate(sheet)?;
        let substream = &records[start..end];
        let end = substream.len();
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
            styles: &globals.styles,
            tab: number as u32 - 1,
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
    let has_styles = !globals.styles.is_empty();
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
    for book in &globals.supbooks {
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
    let macros = globals.sheets.iter().any(|s| s.kind == SheetKind::MacroSheet);
    let report = LegacyReport {
        biff_version: if biff5 { 5 } else { 8 },
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
        .filter(|n| !n.rgce.is_empty() && !n.name.starts_with("_xlpm."))
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
                attributes.push(("localSheetId", scope.to_string()));
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

fn empty_chartsheet() -> Vec<u8> {
    let mut xml = Xml::new();
    xml.open("chartsheet", &[("xmlns", MAIN_NAMESPACE.to_owned())]);
    xml.close("chartsheet");
    xml.out
}
