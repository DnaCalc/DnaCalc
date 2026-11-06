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
