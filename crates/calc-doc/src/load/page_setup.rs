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
pub(crate) fn page_break(element: &XmlElement) -> Option<PageBreak> {
    let number = |name| element.attribute(name).and_then(|n| n.parse().ok());
    Some(PageBreak {
        id: number("id")?,
        min: number("min").unwrap_or(0),
        max: number("max").unwrap_or(0),
        manual: super::manifest::is_true(element.attribute("man")),
        pivot: super::manifest::is_true(element.attribute("pt")),
    })
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
                    if let Some(b) = page_break(&e) {
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
            "Diagrams, embedded objects, ink, worksheet controls, charts in groups or charts not printed with the sheet are preserved; their browser printing is not modelled (dc-971p.57)".to_owned()
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
                if e.name.is(MAIN_NAMESPACE, "oleObjects") {
                    return Err(fail());
                }
                // Legacy form controls print as the grid draws them (dc-zjsg.8); an ActiveX control does not.
                if e.name.is(MAIN_NAMESPACE, "controls") {
                    let mut depth = 1usize;
                    let (mut count, mut fallback) = (0usize, 0usize);
                    while depth > 0 {
                        match events.next().map_err(|_| fail())?.ok_or_else(fail)? {
                            XmlEvent::Start(c) => {
                                depth += 1;
                                if c.name.local == "Fallback" {
                                    fallback = depth;
                                }
                                if c.name.is(MAIN_NAMESPACE, "control") && fallback == 0 {
                                    count += 1;
                                }
                            }
                            XmlEvent::End(_) => {
                                if depth == fallback {
                                    fallback = 0;
                                }
                                depth -= 1;
                            }
                            XmlEvent::Text(_) => {}
                        }
                    }
                    if count != source.shapes().controls.len() {
                        return Err(fail());
                    }
                    continue;
                }
                if e.name.is(MAIN_NAMESPACE, "drawing") {
                    drawings += 1;
                    let part = target(&e, "/drawing")?;
                    // Charts, shapes, text boxes, connectors, groups and pictures print as the grid draws
                    // them (dc-zjsg.2); an object the drawing reader does not hold (SmartArt, OLE, ink, a
                    // control) is refused, and so is a chart frame in a group or not printed with the sheet.
                    if drawings > 1
                        || source.shapes().unread > 0
                        || super::chart::unprinted_objects(&self.package, &part).map_err(|_| fail())? > 0
                    {
                        return Err(fail());
                    }
                }
                if e.name.is(MAIN_NAMESPACE, "legacyDrawing") {
                    let part = target(&e, "/vmlDrawing")?;
                    let xml = self.package.read_xml(&part).map_err(|_| fail())?;
                    let controls = source.shapes().controls.len();
                    if controls > 0 {
                        // The VML beside form controls holds their legacy shapes (and notes, which do not print).
                        if vml_controls(&xml).map_err(|_| fail())? != controls {
                            return Err(fail());
                        }
                    } else {
                        let notes = self.model().definitions().review_notes(sheet);
                        crate::save::review::check_note_shapes(&xml, notes, notes)
                            .map_err(|_| fail())?;
                    }
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

/// The number of form-control shapes in a legacy VML drawing (`x:ClientData ObjectType` one of the form controls);
/// an error for a shape of any other client data than a note or a form control.
fn vml_controls(xml: &crate::xml::XmlPart) -> Result<usize, String> {
    let mut events = Events::new(xml);
    let mut count = 0;
    while let Some(event) = events.next().map_err(|e| e.to_string())? {
        if let XmlEvent::Start(e) = event
            && e.name.is("urn:schemas-microsoft-com:office:excel", "ClientData")
        {
            match e.attribute("ObjectType") {
                Some("Note") => {}
                Some("Button" | "Checkbox" | "Radio" | "List" | "Drop" | "Scroll" | "Spin" | "GBox" | "Label") => count += 1,
                _ => return Err("A legacy drawing shape is not a note or a form control".into()),
            }
        }
    }
    Ok(count)
}
