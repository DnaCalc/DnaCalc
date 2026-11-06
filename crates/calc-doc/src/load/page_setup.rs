//! Read print metadata beside cells; unrecognised XML stays in the original package.
use super::{Events, LoadError, MAIN_NAMESPACE, strings::decode};
use crate::xml::{XmlElement, XmlEvent, XmlPart};
use calc_doc_model::{HEADER_FIELDS, PAGE_SECTIONS, PageBreak, PageSetup};

pub(crate) fn attributes(setup: &mut PageSetup, element: &XmlElement) {
    if let Some((section, names)) = PAGE_SECTIONS
        .iter()
        .find(|(s, _)| element.name.is(MAIN_NAMESPACE, s))
    {
        let fields = setup.sections.entry((*section).into()).or_default();
        for name in *names {
            if let Some(value) = element.attribute(name) {
                fields.insert((*name).into(), value.into());
            }
        }
    }
}
pub(crate) fn read(xml: &XmlPart) -> Result<PageSetup, LoadError> {
    let mut events = Events::new(xml);
    let mut setup = PageSetup::default();
    let mut path = Vec::<String>::new();
    while let Some(event) = events.next()? {
        match event {
            XmlEvent::Start(e) => {
                let local = e.name.local.as_str();
                let main = e.name.namespace.as_deref() == Some(MAIN_NAMESPACE);
                let direct = path.len() == 1;
                if main && (direct || path.as_slice() == ["worksheet", "sheetPr"]) {
                    attributes(&mut setup, &e);
                }
                if main
                    && path.len() == 2
                    && path[1] == "headerFooter"
                    && HEADER_FIELDS.contains(&local)
                {
                    setup.headers.insert(
                        local.into(),
                        calc_doc_model::value::ExcelText::from_utf16_code_units(decode(
                            &events.text()?,
                        )),
                    );
                    continue;
                }
                if main
                    && path.len() == 2
                    && ["rowBreaks", "colBreaks"].contains(&path[1].as_str())
                    && local == "brk"
                {
                    if let Some(id) = e.attribute("id").and_then(|n| n.parse().ok()) {
                        let b = PageBreak {
                            id,
                            min: e.attribute("min").and_then(|n| n.parse().ok()).unwrap_or(0),
                            max: e.attribute("max").and_then(|n| n.parse().ok()).unwrap_or(0),
                            manual: super::manifest::is_true(e.attribute("man")),
                            pivot: super::manifest::is_true(e.attribute("pt")),
                        };
                        if path[1] == "rowBreaks" {
                            setup.row_breaks.push(b);
                        } else {
                            setup.column_breaks.push(b);
                        }
                    }
                }
                // Never materialize worksheet cell nodes for print metadata.
                if direct
                    && ![
                        "sheetPr",
                        "printOptions",
                        "pageMargins",
                        "pageSetup",
                        "headerFooter",
                        "rowBreaks",
                        "colBreaks",
                    ]
                    .contains(&local)
                {
                    events.skip()?;
                } else {
                    path.push(local.into());
                }
            }
            XmlEvent::End(_) => {
                path.pop();
            }
            _ => {}
        }
    }
    Ok(setup)
}

impl super::LoadedWorkbook {
    /// Inspect original drawing objects before printing; preserve unsupported objects in XLSX.
    pub fn check_print_objects(&self, sheet: calc_doc_model::value::SheetId) -> Result<(), String> {
        // A newly authored sheet has no retained package part yet. The caller checks that
        // the sheet exists in the current model; there are no source objects to inspect.
        let Some(source) = self.sheet(sheet) else {
            return Ok(());
        };
        let fail = || {
            "Pictures, shapes, text boxes or worksheet controls are preserved; their browser printing is not modelled (dc-971p.57)".to_owned()
        };
        let rels = self.package.relationships_of(Some(source.part().name()));
        let target = |e: &XmlElement, kind: &str| -> Result<String, String> {
            let id = e
                .attributes
                .iter()
                .find(|a| a.name.is(super::RELATIONSHIP_ATTRIBUTES, "id"))
                .ok_or_else(fail)?;
            rels.and_then(|r| {
                r.items().iter().find(|r| {
                    r.id == id.value
                        && r.kind.ends_with(kind)
                        && r.mode == crate::opc::TargetMode::Internal
                })
            })
            .and_then(|r| r.resolved.clone())
            .ok_or_else(fail)
        };
        let xml = self
            .package
            .read_xml(source.part().name())
            .map_err(|_| fail())?;
        let mut events = Events::new(&xml);
        let mut drawings = 0;
        while let Some(event) = events.next().map_err(|_| fail())? {
            if let XmlEvent::Start(e) = event {
                if e.name.is(MAIN_NAMESPACE, "sheetData") {
                    events.skip().map_err(|_| fail())?;
                    continue;
                }
                if e.name.is(MAIN_NAMESPACE, "controls") || e.name.is(MAIN_NAMESPACE, "oleObjects")
                {
                    return Err(fail());
                }
                if e.name.is(MAIN_NAMESPACE, "drawing") {
                    drawings += 1;
                    let part = target(&e, "/drawing")?;
                    if drawings > 1
                        || super::chart::unprinted_objects(&self.package, &part)
                            .map_err(|_| fail())?
                            > 0
                    {
                        return Err(fail());
                    }
                }
                if e.name.is(MAIN_NAMESPACE, "legacyDrawing") {
                    let part = target(&e, "/vmlDrawing")?;
                    let xml = self.package.read_xml(&part).map_err(|_| fail())?;
                    let notes = self.model().definitions().review_notes(sheet);
                    crate::save::review::check_note_shapes(&xml, notes, notes)
                        .map_err(|_| fail())?;
                }
            }
        }
        if rels.map_or(0, |r| {
            r.items()
                .iter()
                .filter(|r| r.kind.ends_with("/drawing"))
                .count()
        }) != drawings
        {
            return Err(fail());
        }
        Ok(())
    }
}
