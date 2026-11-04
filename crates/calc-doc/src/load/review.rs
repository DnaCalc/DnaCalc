//! Note, threaded-comment and hyperlink content; unchanged fields remain in their source parts.
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
    /// Complete source ranges of extension-backed rules, or `None` if their scope is unread.
    pub conditional_extension_areas: Option<Vec<(u32, u32, u32, u32)>>,
    pub notes: Vec<ReviewNote>,
    pub links: Vec<ReviewLink>,
    pub unsupported: Vec<String>,
}
pub use calc_doc_model::ReviewNote ;
use calc_doc_model::ReviewSource;

pub (crate) const THREAD_NAMESPACE: &str =
    "http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments";

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
            } else if rel.kind.ends_with("/threadedComment") && rel.mode == TargetMode::Internal {
                match rel
                    .resolved
                    .as_deref()
                    .ok_or_else(|| "Threaded comment relationship has no part".to_owned())
                    .and_then(|p| threaded(package, p, sheet).map_err(|e| e.to_string()))
                {
                    Ok(notes) => review.notes.extend(notes),
                    Err(error) => review
                        .unsupported.push(format!("Threaded comments could not be read: {error}")),
                }
            }
        }
    }
    let thread_ids: std::collections::BTreeSet<_> = review
        .notes
        .iter()
        .filter_map(|n| match &n.source {
            Some(ReviewSource::Thread { id, .. }) => Some((n.at, id.to_ascii_lowercase())),
            _ => None,
        })
        .collect();
    for note in &mut review.notes {
        if note.source.as_ref().is_some_and(|s| !s.threaded())
            && note
                .author
                .strip_prefix("tc=")
                .is_some_and(|id| thread_ids.contains(&(note.at, id.to_ascii_lowercase())))
        {
            note.compatibility = true;
            note.editable = false;
        }
    }
    let parts=review.notes.iter().filter_map(|n|n.source.as_ref().map(|s|s.part().to_owned())).collect::<std::collections::BTreeSet<_>>();
    for part in parts {
        let captured=read_xml(package,&part).map_err(|e|e.to_string()).and_then(|xml|crate::save::review::sources(&xml).map_err(|e|e.to_string()));
        match captured {
            Ok(sources)=>for note in review.notes.iter_mut().filter(|n|n.source.as_ref().is_some_and(|s|s.part()==part)) {
                let key=match &note.source {Some(ReviewSource::Thread{id,..})=>id.clone(),_=>crate::save::text::a1(note.at.row(),note.at.column())};
                if let Some(source)=sources.get(&key) {
                    note.source_xml=Some(source.clone());
                    if !note.source.as_ref().is_some_and(ReviewSource::threaded) {
                        let xml=crate::xml::XmlPart::new(part.clone(),source.to_vec(),crate::xml::XmlLimits::default());
                        match crate::save::review::rich_spans(&xml) {Ok(spans)=>{note.spans=spans;if !note.valid_spans(){note.spans.clear();note.editable=false;review.unsupported.push("A note has invalid rich text offsets; its source is retained".into());}},Err(error)=>{note.editable=false;note.annotated=true;review.unsupported.push(error.to_string());}}
                        let mut events=Events::new(&xml);while let Ok(Some(event))=events.next(){if matches!(event,XmlEvent::Start(e)if e.name.is(MAIN_NAMESPACE,"rPh")||e.name.is(MAIN_NAMESPACE,"phoneticPr")){note.editable=false;note.annotated=true;}}
                    }
                }else{note.editable=false;review.unsupported.push("A review source identity is unavailable".into());}
            },
            Err(error)=>{for note in review.notes.iter_mut().filter(|n|n.source.as_ref().is_some_and(|s|s.part()==part)){note.editable=false;}review.unsupported.push(error);}
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
                let uid=e.attributes.iter().find(|a|a.name.is("http://schemas.microsoft.com/office/spreadsheetml/2014/revision","uid")).map(|a|a.value.to_string());
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
                    held.push((at, author, String::from_utf16_lossy(&text),uid));
                }
            }
        }
    }
    Ok(held
        .into_iter()
        .map(|(at, author, text,uid)| ReviewNote {
            at,
            author: author
                .and_then(|a| authors.get(a))
                .cloned()
                .unwrap_or_else(|| "Unknown author".into()),
            text,
            spans:Vec::new(),source_xml:None,
        source: Some(ReviewSource::Note {
                part: part.to_owned(),
                origin: at,uid,
            }),
            editable: true,annotated:false,
            compatibility: false,
        })
        .collect())
}
/// Authors are read by relationship, never by assuming a package path.
pub(crate) fn persons(package: &Package) -> std::collections::BTreeMap<String, String> {
    let mut people = std::collections::BTreeMap::new();
    for rel in package.relationships().iter().flat_map(|r| r.items()) {
        if !rel.kind.ends_with("/person") || rel.mode != TargetMode::Internal {
            continue;
        }
        let Some(xml) = rel
            .resolved
            .as_deref()
            .and_then(|p| read_xml(package, p).ok())
        else {
            continue;
        };
        let mut events = Events::new(&xml);
        while let Ok(Some(event)) = events.next() {
            if let XmlEvent::Start(e) = event
                && e.name.is(THREAD_NAMESPACE, "person")
                && let (Some(id), Some(name)) = (e.attribute("id"), e.attribute("displayName"))
            {
                people.insert(
                    id.to_ascii_lowercase(),
                    String::from_utf16_lossy(&strings::decode(name)),
                );
            }
        }
    }
    people
}
fn threaded(package: &Package, part: &str, sheet: SheetId) -> Result<Vec<ReviewNote>, LoadError> {
    let xml = read_xml(package, part)?;
    let mut events = Events::new(&xml);
    let people = persons(package);
    match events.next()? {
        Some(XmlEvent::Start(e)) if e.name.is(THREAD_NAMESPACE, "ThreadedComments") => {}
        _ => return Err(events.invalid("the part is not ThreadedComments")),
    }
    let mut held = Vec::new();
    while let Some(event) = events.next()? {
        if let XmlEvent::Start(e) = event {
            if !e.name.is(THREAD_NAMESPACE, "threadedComment") {
                events.skip()?;
                continue;
            }
            let at = e
                .attribute("ref")
                .and_then(address::cell)
                .and_then(|(r, c)| CellKey::new(sheet, r, c).ok());
            let id = e
                .attribute("id")
                .ok_or_else(|| events.invalid("a threaded comment has no id"))?
                .to_owned();
            let person = e
                .attribute("personId")
                .ok_or_else(|| events.invalid("a threaded comment has no personId"))?
                .to_owned();
            let parent = e.attribute("parentId").map(str::to_owned);
            let timestamp = e.attribute("dT").map(str::to_owned);
            let resolved = matches!(e.attribute("done"), Some("1" | "true"));
            let mut text = String::new();
            let mut editable = true;
            loop {
                match events.next()? {
                    Some(XmlEvent::Start(e)) if e.name.is(THREAD_NAMESPACE, "text") => {
                        text = String::from_utf16_lossy(&strings::decode(&events.text()?))
                    }
                    Some(XmlEvent::Start(_)) => {
                        editable = false;
                        events.skip()?;
                    }
                    Some(XmlEvent::End(_)) => break,
                    Some(XmlEvent::Text(_)) => {}
                    None => return Err(events.invalid("the part ends inside a threaded comment")),
                }
            }
            let author = people
                .get(&person.to_ascii_lowercase())
                .cloned()
                .unwrap_or_else(|| "Unknown author".into());
            held.push((
                at,
                ReviewNote {
                    at: at.unwrap_or_else(|| CellKey::new(sheet, 1, 1).unwrap()),
                    author,
                    text,
                    spans:Vec::new(),source_xml:None,
                    source: Some(ReviewSource::Thread {
                        part: part.into(),
                        id,
                        parent,
                        person,
                        timestamp,
                        resolved,
                    }),
                    editable,annotated:!editable,
                    compatibility: false,
                },
            ));
        }
    }
    // A reply may omit ref and inherit the cell of its parent, even when it precedes the parent.
    let mut addresses: std::collections::BTreeMap<String, CellKey> = held
        .iter()
        .filter_map(|(at, n)| match (&n.source, at) {
            (Some(ReviewSource::Thread { id, .. }), Some(at)) => {
                Some((id.to_ascii_lowercase(), *at))
            }
            _ => None,
        })
        .collect();
    for _ in 0..held.len() {
        let mut changed = false;
        for (at, n) in &mut held {
            if at.is_none()
                && let Some(ReviewSource::Thread {
                    id,
                    parent: Some(parent),
                    ..
                }) = &n.source
                && let Some(found) = addresses.get(&parent.to_ascii_lowercase()).copied()
            {
                *at = Some(found);
                n.at = found;
                addresses.insert(id.to_ascii_lowercase(), found);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    if held.iter().any(|(at, _)| at.is_none()) {
        return Err(events.invalid("a threaded reply has no cell or resolvable parent"));
    }
    let mut seen = std::collections::BTreeSet::new();
    for (_, n) in &held {
        if let Some(ReviewSource::Thread { id, .. }) = &n.source
            && !seen.insert(id.to_ascii_lowercase())
        {
            return Err(events.invalid("threaded comment ids are duplicated"));
        }
    }
    Ok(held.into_iter().map(|(_, n)| n)
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
