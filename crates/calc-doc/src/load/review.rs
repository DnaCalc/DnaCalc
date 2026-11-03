//! Read-only note and hyperlink content; the package remains the source for saved review parts.
use super::{
    Events, LoadError, MAIN_NAMESPACE, RELATIONSHIP_ATTRIBUTES, address, read_xml, strings,
};
use crate::{
    opc::{Package, TargetMode},
    xml::XmlEvent,
};
use calc_doc_model::value::{CellKey, GridArea, SheetId};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SheetReview {
    /// Source conditional extensions need their own interpretation before colour filtering.
    pub conditional_extensions: bool,
    pub notes: Vec<ReviewNote>,
    pub links: Vec<ReviewLink>,
    pub unsupported: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewNote {
    pub at: CellKey,
    pub author: String,
    pub text: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewLink {
    pub area: GridArea,
    pub target: Option<String>,
    pub location: Option<String>,
    pub display: Option<String>,
    pub tooltip: Option<String>,
}

pub(crate) fn read(package: &Package, part: &str, sheet: SheetId) -> SheetReview {
    let mut review = SheetReview::default();
    if let Some(rels) = package.relationships_of(Some(part)) {
        for rel in rels.items() {
            if rel.kind.ends_with("/comments") && rel.mode == TargetMode::Internal {
                match rel
                    .resolved
                    .as_deref()
                    .ok_or_else(|| "Note relationship has no part".to_owned())
                    .and_then(|p| notes(package, p, sheet).map_err(|e| e.to_string()))
                {
                    Ok(notes) => review.notes.extend(notes),
                    Err(e) => review
                        .unsupported
                        .push(format!("Notes could not be read: {e}")),
                }
            } else if rel.kind.ends_with("/threadedComment") {
                review.unsupported.push("Threaded comments are preserved in the file; reading and replying are unavailable (dc-971p.8)".into());
            }
        }
    }
    review
}
fn notes(package: &Package, part: &str, sheet: SheetId) -> Result<Vec<ReviewNote>, LoadError> {
    let xml = read_xml(package, part)?;
    let mut events = Events::new(&xml);
    let mut authors = Vec::new();
    let mut held = Vec::new();
    while let Some(event) = events.next()? {
        if let XmlEvent::Start(e) = event {
            if e.name.is(MAIN_NAMESPACE, "author") {
                authors.push(String::from_utf16_lossy(&strings::decode(&events.text()?)));
            } else if e.name.is(MAIN_NAMESPACE, "comment") {
                let at = e
                    .attribute("ref")
                    .and_then(address::cell)
                    .and_then(|(r, c)| CellKey::new(sheet, r, c).ok());
                let author = e
                    .attribute("authorId")
                    .and_then(|s| s.parse::<usize>().ok());
                let mut text = Vec::new();
                loop {
                    match events.next()? {
                        Some(XmlEvent::Start(e)) if e.name.is(MAIN_NAMESPACE, "text") => {
                            text = strings::string_item(&mut events)?
                        }
                        Some(XmlEvent::Start(_)) => events.skip()?,
                        Some(XmlEvent::End(_)) | None => break,
                        _ => {}
                    }
                }
                if let Some(at) = at {
                    held.push((at, author, String::from_utf16_lossy(&text)));
                }
            }
        }
    }
    Ok(held
        .into_iter()
        .map(|(at, author, text)| ReviewNote {
            at,
            author: author
                .and_then(|a| authors.get(a))
                .cloned()
                .unwrap_or_else(|| "Unknown author".into()),
            text,
        })
        .collect())
}
pub(super) fn links(
    events: &mut Events<'_>,
    package: &Package,
    part: &str,
    sheet: SheetId,
) -> Result<Vec<ReviewLink>, LoadError> {
    let mut links = Vec::new();
    while let Some(event) = events.next()? {
        match event {
            XmlEvent::Start(e) if e.name.is(MAIN_NAMESPACE, "hyperlink") => {
                let area = e
                    .attribute("ref")
                    .and_then(address::area)
                    .and_then(|(t, l, b, r)| {
                        GridArea::new(
                            CellKey::new(sheet, t, l).ok()?,
                            CellKey::new(sheet, b, r).ok()?,
                        )
                        .ok()
                    });
                let target = e
                    .attributes
                    .iter()
                    .find(|a| a.name.is(RELATIONSHIP_ATTRIBUTES, "id"))
                    .and_then(|a| {
                        package
                            .relationships_of(Some(part))?
                            .items()
                            .iter()
                            .find(|r| r.id == a.value && r.kind.ends_with("/hyperlink"))
                    })
                    .map(|r| r.target.clone());
                if let Some(area) = area {
                    links.push(ReviewLink {
                        area,
                        target,
                        location: e
                            .attribute("location")
                            .map(|s| String::from_utf16_lossy(&strings::decode(s))),
                        display: e
                            .attribute("display")
                            .map(|s| String::from_utf16_lossy(&strings::decode(s))),
                        tooltip: e
                            .attribute("tooltip")
                            .map(|s| String::from_utf16_lossy(&strings::decode(s))),
                    });
                }
                events.skip()?;
            }
            XmlEvent::Start(_) => events.skip()?,
            XmlEvent::End(_) => break,
            _ => {}
        }
    }
    Ok(links)
}
