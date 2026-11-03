//! Review parts retain their original attributes, identities and unsupported children.
//! Local author fields follow the published CT_Person contract:
//! https://learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/6274371e-7c5c-46e3-b661-cbeb4abfe968
//! New record fields follow CT_ThreadedComment; no schema source code is included.
//! https://learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/adb84732-9fc8-48b6-bddc-6b0bcdaad940
use super::{
    SaveRefusal,
    author::Changes,
    parts,
    reader::Reader,
    text::{RawTag, Splice, attribute_value, xstring},
};
use crate::{
    load::{MAIN_NAMESPACE, review::THREAD_NAMESPACE},
    opc::Package,
    xml::{XmlEvent, XmlLimits, XmlPart},
};
use calc_doc_model::value::CellKey;
use calc_doc_model::{NoteSpan, NoteStyle, ReviewNote, ReviewSource};
use std::collections::{BTreeMap, BTreeSet};
const COMMENTS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
const THREADS: &str = "http://schemas.microsoft.com/office/2017/10/relationships/threadedComment";
const PERSONS: &str = "http://schemas.microsoft.com/office/2017/10/relationships/person";
const VML_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing";
const REL_ATTR: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const XR: &str = "http://schemas.microsoft.com/office/spreadsheetml/2014/revision";
const VML: &str = "urn:schemas-microsoft-com:vml";
const EXCEL: &str = "urn:schemas-microsoft-com:office:excel";

pub(crate) fn sources(
    xml: &XmlPart,
) -> Result<BTreeMap<String, std::sync::Arc<[u8]>>, SaveRefusal> {
    let mut out = BTreeMap::new();
    for bytes in super::reader::element_sources(xml, |e| {
        e.name.is(MAIN_NAMESPACE, "comment") || e.name.is(THREAD_NAMESPACE, "threadedComment")
    })? {
        let part = XmlPart::new(
            "review-source.xml".into(),
            bytes.clone(),
            XmlLimits::default(),
        );
        let mut reader = Reader::new(&part);
        if let (XmlEvent::Start(e), _) = reader.expect()? {
            let key = if e.name.is(THREAD_NAMESPACE, "threadedComment") {
                e.attribute("id")
            } else {
                e.attribute("ref")
            };
            if let Some(key) = key {
                out.insert(key.to_owned(), bytes.into());
            }
        }
    }
    Ok(out)
}
pub(crate) fn rich_spans(xml: &XmlPart) -> Result<Vec<NoteSpan>, SaveRefusal> {
    let properties=super::reader::element_sources(xml,|e|e.name.is(MAIN_NAMESPACE,"rPr"))?;
    let mut properties=properties.into_iter();
    let mut events=crate::load::Events::new(xml);
    let invalid=|e:crate::load::LoadError|SaveRefusal::InvalidPart{part:xml.name().into(),detail:e.to_string()};
    let mut parents=Vec::new();let mut spans=Vec::new();let mut offset=0u32;let mut style=NoteStyle::default();
    while let Some(event)=events.next().map_err(&invalid)? {
        match event {
            XmlEvent::Start(e) if e.name.is(MAIN_NAMESPACE,"rPr")=>{
                // Consume the same source record even when it belongs to an opaque subtree.
                let source=properties.next().ok_or_else(||SaveRefusal::InvalidPart{part:xml.name().into(),detail:"Note run properties are missing".into()})?;
                if parents==["comment","text","r"] {style=read_style(&source)?;}
                events.skip().map_err(&invalid)?;
            }
            XmlEvent::Start(e) if e.name.is(MAIN_NAMESPACE,"t")=>{
                let text=events.text().map_err(&invalid)?;
                if parents==["comment","text"]||parents==["comment","text","r"] {
                    let len=crate::load::strings::decode(&text).len() as u32;
                    if parents.len()==3&&len>0&&style!=NoteStyle::default(){spans.push(NoteSpan{start:offset,end:offset+len,style:style.clone()});}
                    offset+=len;
                }
            }
            XmlEvent::Start(e)=>{
                if (parents==["comment","text"]&&!e.name.is(MAIN_NAMESPACE,"r"))||parents==["comment","text","r"] {
                    return Err(SaveRefusal::InvalidPart{part:xml.name().into(),detail:"This note has text annotations whose offsets are not modelled; its source is retained".into()});
                }
                if e.name.is(MAIN_NAMESPACE,"r")&&parents==["comment","text"]{style=NoteStyle::default();}
                parents.push(if e.name.namespace.as_deref()==Some(MAIN_NAMESPACE){e.name.local.to_string()}else{String::new()});
            }
            XmlEvent::End(_)=>{parents.pop();},
            _=>{}
        }
    }
    Ok(spans)
}
fn read_style(bytes: &[u8]) -> Result<NoteStyle, SaveRefusal> {
    let xml = XmlPart::new("note-run.xml".into(), bytes.to_vec(), XmlLimits::default());
    let mut reader = Reader::new(&xml);
    let mut style = NoteStyle {
        source: Some(bytes.to_vec().into()),
        ..Default::default()
    };
    reader.root("rPr")?;
    loop {
        match reader.expect()? {
            (XmlEvent::Start(e),_)=>{
                if e.name.namespace.as_deref()==Some(MAIN_NAMESPACE){
                    let flag=!matches!(e.attribute("val"),Some("0"|"false"|"none"));
                    match e.name.local.as_str(){
                        "b"=>style.bold=flag,"i"=>style.italic=flag,"u"=>style.underline=flag,"strike"=>style.strike=flag,
                        "color"=>style.color=e.attribute("rgb").map(str::to_owned),"sz"=>style.size=e.attribute("val").map(str::to_owned),_=>{}
                    }
                }
                reader.element_end()?;
            }
            (XmlEvent::End(_),_)=>break,
            _=>{}
        }
    }
    Ok(style)
}
fn style_xml(style: &NoteStyle) -> Result<Vec<u8>, SaveRefusal> {
    let empty = format!("<rPr xmlns=\"{MAIN_NAMESPACE}\"/>");
    let bytes = style.source.as_deref().unwrap_or(empty.as_bytes());
    let xml = XmlPart::new(
        "note-properties.xml".into(),
        bytes.to_vec(),
        XmlLimits::default(),
    );
    let mut reader = Reader::new(&xml);
    let (_, root) = reader.root("rPr")?;
    let tag = RawTag::parse(&xml.bytes()[root.0..root.1]);
    let prefix = String::from_utf8_lossy(tag.prefix());
    let mut add = Vec::new();
    let mut seen = BTreeSet::new();
    let mut splice = Splice::new(xml.bytes());
    if !tag.is_empty() {
        loop {
            match reader.expect()? {
                (XmlEvent::Start(e), span) => {
                    let end = reader.element_end()?;
                    let name = e.name.local.as_str();
                    if e.name.namespace.as_deref() == Some(MAIN_NAMESPACE)
                        && ["b", "i", "u", "strike"].contains(&name)
                    {
                        seen.insert(name.to_owned());
                        let wanted = match name {
                            "b" => style.bold,
                            "i" => style.italic,
                            "u" => style.underline,
                            _ => style.strike,
                        };
                        let actual = !matches!(e.attribute("val"), Some("0" | "false" | "none"));
                        if wanted != actual {
                            let replacement = if wanted {
                                format!("<{prefix}{name}/>").into_bytes()
                            } else {
                                Vec::new()
                            };
                            splice.replace((span.0, end), &replacement);
                        }
                    } else if e.name.is(MAIN_NAMESPACE, "color") {
                        seen.insert("color".into());
                        if style.color.is_none() && e.attribute("rgb").is_some() {splice.replace((span.0,end),&[]);}
                        if let Some(color) = &style.color
                            && e.attribute("rgb") != Some(color.as_str())
                        {
                            splice.replace(
                                (span.0, end),
                                format!("<{prefix}color rgb=\"{}\"/>", rgba(color)).as_bytes(),
                            );
                        }
                    }
                }
                (XmlEvent::End(_), span) => {
                    for (name, wanted) in [
                        ("b", style.bold),
                        ("i", style.italic),
                        ("u", style.underline),
                        ("strike", style.strike),
                    ] {
                        if wanted && !seen.contains(name) {
                            add.extend(format!("<{prefix}{name}/>").as_bytes());
                        }
                    }
                    if !seen.contains("color")
                        && let Some(color) = &style.color
                    {
                        add.extend(format!("<{prefix}color rgb=\"{}\"/>", rgba(color)).as_bytes());
                    }
                    splice.replace((span.0, span.0), &add);
                    break;
                }
                _ => {}
            }
        }
        Ok(splice.finish())
    } else {
        for (name, wanted) in [
            ("b", style.bold),
            ("i", style.italic),
            ("strike", style.strike),
        ] {
            if wanted {
                add.extend(format!("<{prefix}{name}/>").as_bytes());
            }
        }
        if let Some(color) = &style.color {
            add.extend(format!("<{prefix}color rgb=\"{}\"/>", rgba(color)).as_bytes());
        }
        if style.underline {
            add.extend(format!("<{prefix}u/>").as_bytes());
        }
        let mut out = tag.rebuilt(&[], false);
        out.extend(add);
        out.extend(format!("</{prefix}rPr>").as_bytes());
        Ok(out)
    }
}
fn rgba(color: &str) -> String {
    let color = color.trim_start_matches('#');
    if color.len() == 8 && color.bytes().all(|b| b.is_ascii_hexdigit()) {
        color.to_uppercase()
    } else if color.len() == 6 && color.bytes().all(|b| b.is_ascii_hexdigit()) {
        format!("FF{}", color.to_uppercase())
    } else {
        "FF000000".into()
    }
}
fn text_xml(note: &ReviewNote) -> Result<Vec<u8>, SaveRefusal> {
    let threaded = note.source.as_ref().is_some_and(ReviewSource::threaded);
    let namespace = if threaded {
        THREAD_NAMESPACE
    } else {
        MAIN_NAMESPACE
    };
    let mut out = format!("<text xmlns=\"{namespace}\">").into_bytes();
    if threaded {
        xstring(&note.text.encode_utf16().collect::<Vec<_>>(), &mut out);
    } else if note.spans.is_empty() {
        out.extend(b"<t xml:space=\"preserve\">");
        xstring(&note.text.encode_utf16().collect::<Vec<_>>(), &mut out);
        out.extend(b"</t>");
    } else {
        for (text, style) in calc_doc_model::note_segments(note) {
            out.extend(b"<r>");
            if style != NoteStyle::default() {
                out.extend(style_xml(&style)?);
            }
            out.extend(b"<t xml:space=\"preserve\">");
            xstring(&text.encode_utf16().collect::<Vec<_>>(), &mut out);
            out.extend(b"</t></r>");
        }
    }
    out.extend(b"</text>");
    Ok(out)
}
fn plain_text(xml: &XmlPart, threaded: bool) -> Result<String, SaveRefusal> {
    let mut events = crate::load::Events::new(xml);
    while let Some(event) = events.next().map_err(|e| SaveRefusal::InvalidPart {
        part: xml.name().into(),
        detail: e.to_string(),
    })? {
        if let XmlEvent::Start(e) = event
            && e.name.is(
                if threaded {
                    THREAD_NAMESPACE
                } else {
                    MAIN_NAMESPACE
                },
                "text",
            )
        {
            return if threaded {
                events
                    .text()
                    .map(|t| String::from_utf16_lossy(&crate::load::strings::decode(&t)))
            } else {
                crate::load::strings::string_item(&mut events).map(|t| String::from_utf16_lossy(&t))
            }
            .map_err(|e| SaveRefusal::InvalidPart {
                part: xml.name().into(),
                detail: e.to_string(),
            });
        }
    }
    Ok(String::new())
}

fn render_note(
    note: &ReviewNote,
    raw: Option<&[u8]>,
    author: usize,
) -> Result<Vec<u8>, SaveRefusal> {
    let threaded = note.source.as_ref().is_some_and(ReviewSource::threaded);
    let namespace = if threaded {
        THREAD_NAMESPACE
    } else {
        MAIN_NAMESPACE
    };
    let local = if threaded {
        "threadedComment"
    } else {
        "comment"
    };
    let empty = format!("<{local} xmlns=\"{namespace}\"/>");
    let xml = XmlPart::new(
        "review-record.xml".into(),
        note.source_xml
            .as_deref()
            .or(raw)
            .unwrap_or(empty.as_bytes())
            .to_vec(),
        XmlLimits::default(),
    );
    let mut reader = Reader::new(&xml);
    let mut splice = Splice::new(xml.bytes());
    let (e, root) = match reader.expect()? {
        (XmlEvent::Start(e), s) => (e, s),
        _ => return Err(reader.invalid("Review record has no root")),
    };
    if !e.name.is(namespace, local) {
        return Err(reader.invalid("Review source has the wrong kind"));
    }
    let tag = RawTag::parse(&xml.bytes()[root.0..root.1]);
    let prefix = String::from_utf8_lossy(tag.prefix());
    let reference = super::text::a1(note.at.row(), note.at.column());
    let author = author.to_string();
    let mut values = vec![
        ("ref".to_owned(), Some(reference)),
        (
            "authorId".to_owned(),
            if threaded { None } else { Some(author) },
        ),
    ];
    if let Some(ReviewSource::Thread {
        id,
        parent,
        person,
        timestamp,
        resolved,
        ..
    }) = &note.source
    {
        values.extend([
            ("id".into(), Some(id.clone())),
            ("parentId".into(), parent.clone()),
            ("personId".into(), Some(person.clone())),
            ("dT".into(), timestamp.clone()),
            ("done".into(), resolved.then(|| "1".into())),
        ]);
    }
    if let Some(ReviewSource::NewNote { id, .. }) = &note.source {
        let raw_names = tag
            .attribute_names()
            .filter(|n| *n != b"xmlns" && !n.starts_with(b"xmlns:"))
            .collect::<Vec<_>>();
        for (attribute, raw_name) in e.attributes.iter().zip(raw_names) {
            if attribute.name.is(XR, "uid") {
                values.push((String::from_utf8_lossy(raw_name).into_owned(), None));
            }
        }
        let prefix = (0u32..)
            .map(|n| format!("dnaRev{n}"))
            .find(|p| !tag.has(&format!("xmlns:{p}")))
            .expect("revision prefix");
        values.push((format!("xmlns:{prefix}"), Some(XR.into())));
        values.push((format!("{prefix}:uid"), Some(id.clone())));
        const MC:&str="http://schemas.openxmlformats.org/markup-compatibility/2006";
        let raw_names=tag.attribute_names().filter(|n|*n!=b"xmlns"&&!n.starts_with(b"xmlns:")).collect::<Vec<_>>();
        if let Some((attribute,name))=e.attributes.iter().zip(raw_names).find(|(a,_)|a.name.is(MC,"Ignorable")) {
            values.push((String::from_utf8_lossy(name).into_owned(),Some(format!("{} {prefix}",attribute.value))));
        }else{
            let mc=(0u32..).map(|n|format!("dnaMc{n}")).find(|p|!tag.has(&format!("xmlns:{p}"))).expect("compatibility prefix");
            values.push((format!("xmlns:{mc}"),Some(MC.into())));values.push((format!("{mc}:Ignorable"),Some(prefix.clone())));
        }

    }
    let changes = values
        .iter()
        .map(|(n, v)| (n.as_str(), v.as_deref()))
        .collect::<Vec<_>>();
    let original_text = plain_text(&xml, threaded)?;
    let original_spans = if threaded {
        Vec::new()
    } else if note.annotated {
        note.spans.clone()
    } else {
        rich_spans(&xml)?
    };
    let replace = original_text != note.text || original_spans != note.spans;
    if tag.is_empty() {
        let mut bytes = tag.rebuilt(&changes, false);
        bytes.extend(text_xml(note)?);
        bytes.extend(format!("</{prefix}{local}>").as_bytes());
        splice.replace(root, &bytes);
    } else {
        splice.replace(root, &tag.rebuilt(&changes, false));
        let mut seen = false;
        loop {
            match reader.expect()? {
                (XmlEvent::Start(e), span) => {
                    let end = reader.element_end()?;
                    if e.name.is(namespace, "text") {
                        seen = true;
                        if replace {
                            splice.replace((span.0, end), &text_xml(note)?);
                        }
                    }
                }
                (XmlEvent::End(_), span) => {
                    if !seen {
                        splice.replace((span.0, span.0), &text_xml(note)?);
                    }
                    break;
                }
                _ => {}
            }
        }
    }
    Ok(splice.finish())
}
fn identity_matches(old: &ReviewNote, new: &ReviewNote) -> bool {
    match (&old.source, &new.source) {
        (Some(ReviewSource::Thread { id: a, .. }), Some(ReviewSource::Thread { id: b, .. })) => {
            a.eq_ignore_ascii_case(b)
        }
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}
fn child_part(xml: &XmlPart, namespace: &str, local: &str) -> Result<XmlPart, SaveRefusal> {
    let bytes = super::reader::element_sources(xml, |e| e.name.is(namespace, local))?
        .into_iter()
        .next()
        .ok_or_else(|| SaveRefusal::InvalidPart {
            part: xml.name().into(),
            detail: format!("Missing {local}"),
        })?;
    Ok(XmlPart::new(
        format!("{local}.xml"),
        bytes,
        XmlLimits::default(),
    ))
}

fn author_list(xml: &XmlPart) -> Result<Vec<String>, SaveRefusal> {
    let mut events = crate::load::Events::new(xml);
    let mut out = Vec::new();
    while let Some(e) = events.next().map_err(|e| SaveRefusal::InvalidPart {
        part: xml.name().into(),
        detail: e.to_string(),
    })? {
        if let XmlEvent::Start(e) = e
            && e.name.is(MAIN_NAMESPACE, "author")
        {
            out.push(String::from_utf16_lossy(&crate::load::strings::decode(
                &events.text().map_err(|e| SaveRefusal::InvalidPart {
                    part: xml.name().into(),
                    detail: e.to_string(),
                })?,
            )));
        }
    }
    Ok(out)
}
fn reconcile(
    xml: &XmlPart,
    threaded: bool,
    old: &[(&ReviewNote, CellKey)],
    notes: &[&ReviewNote],
) -> Result<Vec<u8>, SaveRefusal> {
    let namespace = if threaded {
        THREAD_NAMESPACE
    } else {
        MAIN_NAMESPACE
    };
    let root_name = if threaded {
        "ThreadedComments"
    } else {
        "comments"
    };
    let local = if threaded {
        "threadedComment"
    } else {
        "comment"
    };
    let mut authors = if threaded {
        Vec::new()
    } else {
        author_list(xml)?
    };
    let original_authors = authors.len();
    for note in notes {
        if !authors.contains(&note.author) {
            authors.push(note.author.clone());
        }
    }
    let mut added_authors = Vec::new();
    for author in authors.iter().skip(original_authors) {
        added_authors.extend(format!("<author xmlns=\"{MAIN_NAMESPACE}\">").as_bytes());
        xstring(
            &author.encode_utf16().collect::<Vec<_>>(),
            &mut added_authors,
        );
        added_authors.extend(b"</author>");
    }
    let mut reader = Reader::new(xml);
    let mut splice = Splice::new(xml.bytes());
    let mut matched = BTreeSet::new();
    let mut authors_seen = false;
    let mut list_seen = threaded;
    let mut root_close = None;
    let mut list_close = None;
    while let Some((event, span)) = reader.next()? {
        match event {
            XmlEvent::Start(e) if e.name.is(MAIN_NAMESPACE, "authors") && !threaded => {
                authors_seen = true;
                let end = reader.element_end()?;
                if !added_authors.is_empty() {
                    let part = child_part(xml, MAIN_NAMESPACE, "authors")?;
                    let bytes = parts::append_root(&part, &added_authors)?;
                    splice.replace((span.0, end), &bytes);
                }
            }
            XmlEvent::Start(e) if e.name.is(MAIN_NAMESPACE, "commentList") && !threaded => {
                list_seen = true
            }
            XmlEvent::Start(e) if e.name.is(namespace, local) => {
                let end = reader.element_end()?;
                let previous=old.iter().find(|(n,at)|if threaded{matches!(&n.source,Some(ReviewSource::Thread{id,..})if e.attribute("id").is_some_and(|v|v.eq_ignore_ascii_case(id)))}else{e.attribute("ref").and_then(crate::load::address::cell)==Some((at.row(),at.column()))});
                if let Some((previous, _)) = previous {
                    if let Some((index, note)) = notes
                        .iter()
                        .enumerate()
                        .find(|(_, n)| identity_matches(previous, n))
                    {
                        if !matched.insert(index) {
                            return Err(reader.invalid("A review source identity is duplicated"));
                        }
                        if note.at != previous.at
                            || note.text != previous.text
                            || note.spans != previous.spans
                            || note.source != previous.source
                        {
                            let bytes = render_note(
                                note,
                                Some(&xml.bytes()[span.0..end]),
                                authors.iter().position(|a| a == &note.author).unwrap_or(0),
                            )?;
                            splice.replace((span.0, end), &bytes);
                        }
                    } else {
                        splice.replace((span.0, end), &[]);
                    }
                }
            }
            XmlEvent::End(e) if e.is(MAIN_NAMESPACE, "commentList") && !threaded => {
                list_close = Some(span.0)
            }
            XmlEvent::End(e) if e.is(namespace, root_name) => root_close = Some(span.0),
            _ => {}
        }
    }
    let mut appended = Vec::new();
    for (index, note) in notes.iter().enumerate() {
        if !matched.contains(&index) {
            appended.extend(render_note(
                note,
                None,
                authors.iter().position(|a| a == &note.author).unwrap_or(0),
            )?);
        }
    }
    // The original comment list is the final child in standard comments parts. Reparse after
    // earlier replacements so insertion positions never overlap preserved source ranges.
    let mut xml = xml.with_bytes(splice.finish());
    if threaded {
        return parts::append_root_before(&xml, &appended, &["extLst"]);
    }
    if !authors_seen {
        let mut root = Reader::new(&xml);
        let (_, span) = root.root("comments")?;
        let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
        let mut bytes = format!("<authors xmlns=\"{MAIN_NAMESPACE}\">").into_bytes();
        bytes.extend(added_authors);
        bytes.extend(b"</authors>");
        if tag.is_empty() {
            xml = xml.with_bytes(parts::append_root(&xml, &bytes)?);
        } else {
            let mut splice = Splice::new(xml.bytes());
            splice.replace((span.1, span.1), &bytes);
            xml = xml.with_bytes(splice.finish());
        }
    }
    if appended.is_empty() {
        return Ok(xml.bytes().to_vec());
    }
    if list_seen && list_close.is_some() {
        let mut reader = Reader::new(&xml);
        let mut splice = Splice::new(xml.bytes());
        while let Some((event, span)) = reader.next()? {
            if let XmlEvent::Start(e) = event
                && e.name.is(MAIN_NAMESPACE, "commentList")
            {
                let end = reader.element_end()?;
                let child = child_part(&xml, MAIN_NAMESPACE, "commentList")?;
                splice.replace((span.0, end), &parts::append_root(&child, &appended)?);
                break;
            }
        }
        Ok(splice.finish())
    } else {
        let mut list = format!("<commentList xmlns=\"{MAIN_NAMESPACE}\">").into_bytes();
        list.extend(appended);
        list.extend(b"</commentList>");
        if root_close.is_none() {
            return Err(SaveRefusal::InvalidPart {
                part: xml.name().into(),
                detail: "Review part has no closing root".into(),
            });
        }
        parts::append_root(&xml, &list)
    }
}
fn new_part(
    package: &Package,
    out: &Changes,
    workbook: &str,
    folder: &str,
    suffix: &str,
) -> String {
    super::author::available_part_name(
        package,
        workbook,
        &format!("{folder}/dnaReview"),
        suffix,
        &out.added.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(),
    )
}
fn related(package: &Package, from: &str, suffix: &str) -> Option<String> {
    package
        .relationships_of(Some(from))?
        .items()
        .iter()
        .find(|r| r.kind.ends_with(suffix) && r.mode == crate::opc::TargetMode::Internal)?
        .resolved
        .clone()
}
pub(super) fn finish(
    package: &Package,
    workbook: &str,
    sheets: &[std::borrow::Cow<'_, crate::load::LoadedSheet>],
    base: &calc_doc_model::ModelRoot,
    model: &calc_doc_model::ModelRoot,
    moves: &[Option<super::structural::SheetMoves>],
    out: &mut Changes,
) -> Result<(), SaveRefusal> {
    let mut old: BTreeMap<String, Vec<(&ReviewNote, CellKey)>> = BTreeMap::new();
    let mut desired: BTreeMap<String, Vec<&ReviewNote>> = BTreeMap::new();
    let mut thread_ids = BTreeSet::new();
    for (sheet, notes) in base.definitions().all_review_notes() {
        for note in notes {
            let Some(source) = &note.source else {
                continue;
            };
            if let ReviewSource::Thread { id, .. } = source {
                thread_ids.insert(id.to_ascii_lowercase());
            }
            let at = sheets.iter().position(|s| s.id() == sheet).and_then(|i| {
                if let Some(m) = &moves[i] {
                    let a = m.area((
                        note.at.row(),
                        note.at.column(),
                        note.at.row(),
                        note.at.column(),
                    ))?;
                    CellKey::new(sheet, a.0, a.1).ok()
                } else {
                    Some(note.at)
                }
            });
            if let Some(at) = at.or_else(||source.threaded().then_some(note.at)) {
                old.entry(source.part().into())
                    .or_default()
                    .push((note, at));
            }
        }
    }
    for (_, notes) in model.definitions().all_review_notes() {
        for note in notes {
            if let Some(source) = &note.source {
                desired.entry(source.part().into()).or_default().push(note);
            }
        }
    }
    let names: BTreeSet<_> = old.keys().chain(desired.keys()).cloned().collect();
    let mut new_people = BTreeMap::new();
    for name in names {
        let old = old.get(&name).map(Vec::as_slice).unwrap_or(&[]);
        let notes = desired.get(&name).map(Vec::as_slice).unwrap_or(&[]);
        let source = notes
            .first()
            .map(|n| *n)
            .or_else(|| old.first().map(|(n, _)| *n))
            .expect("review record");
        let threaded = source.source.as_ref().is_some_and(ReviewSource::threaded);
        if notes
            .iter()
            .any(|n| n.source.as_ref().is_some_and(ReviewSource::threaded) != threaded)
        {
            return Err(SaveRefusal::InvalidPart {
                part: name,
                detail: "A review part mixes notes and threaded comments".into(),
            });
        }
        let empty = if threaded {
            format!("<ThreadedComments xmlns=\"{THREAD_NAMESPACE}\"></ThreadedComments>")
        } else {
            format!("<comments xmlns=\"{MAIN_NAMESPACE}\"><authors/><commentList/></comments>")
        };
        let xml = parts::current_part(package, out, &name, empty.as_bytes())?;
        let bytes = reconcile(&xml, threaded, old, notes)?;
        parts::put_part(package, out, &name, bytes);
        parts::ensure_content_type(
            package,
            out,
            &name,
            if threaded {
                "application/vnd.ms-excel.threadedcomments+xml"
            } else {
                "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml"
            },
        )?;
        let sheet_ids: BTreeSet<_> = notes.iter().map(|n| n.at.sheet()).collect();
        for sheet in sheet_ids {
            let part = sheets.iter().find(|s| s.id() == sheet).ok_or_else(|| {
                SaveRefusal::InvalidPart {
                    part: name.clone(),
                    detail: "Review worksheet disappeared".into(),
                }
            })?;
            parts::ensure_relationship(
                package,
                out,
                part.part().name(),
                if threaded { THREADS } else { COMMENTS },
                &name,
            )?;
        }
        for note in notes {
            if let Some(ReviewSource::Thread { id, person, .. }) = &note.source
                && !thread_ids.contains(&id.to_ascii_lowercase())
            {
                new_people.insert(person.clone(), note.author.clone());
            }
        }
    }
    if !new_people.is_empty() {
        let name = related(package, workbook, "/person")
            .unwrap_or_else(|| new_part(package, out, workbook, "persons", "xml"));
        let empty = format!("<personList xmlns=\"{THREAD_NAMESPACE}\"/>");
        let xml = parts::current_part(package, out, &name, empty.as_bytes())?;
        let mut present = BTreeSet::new();
        let mut reader = Reader::new(&xml);
        while let Some((event, _)) = reader.next()? {
            if let XmlEvent::Start(e) = event
                && e.name.is(THREAD_NAMESPACE, "person")
                && let Some(id) = e.attribute("id")
            {
                present.insert(id.to_ascii_lowercase());
            }
        }
        let mut added = Vec::new();
        for (id, author) in new_people {
            if !present.contains(&id.to_ascii_lowercase()) {
                added.extend(
                    format!("<person xmlns=\"{THREAD_NAMESPACE}\" displayName=").as_bytes(),
                );
                super::text::xstring_attribute(
                    &author.encode_utf16().collect::<Vec<_>>(),
                    &mut added,
                );
                added.extend(b" id=");
                attribute_value(&id, &mut added);
                added.extend(b" providerId=\"None\" userId=");
                super::text::xstring_attribute(
                    &author.encode_utf16().collect::<Vec<_>>(),
                    &mut added,
                );
                added.extend(b"/>");
            }
        }
        parts::put_part(
            package,
            out,
            &name,
            parts::append_root_before(&xml, &added, &["extLst"])?,
        );
        parts::ensure_content_type(package, out, &name, "application/vnd.ms-excel.person+xml")?;
        parts::ensure_relationship(package, out, workbook, PERSONS, &name)?;
    }
    for sheet in sheets
        .iter()
        .filter(|s| model.sheet_by_id(s.id()).is_some())
    {
        let notes = model
            .definitions()
            .review_notes(sheet.id())
            .iter()
            .filter(|n| !n.source.as_ref().is_some_and(ReviewSource::threaded))
            .collect::<Vec<_>>();
        let desired_positions = notes
            .iter()
            .map(|n| (n.at.row(), n.at.column()))
            .collect::<BTreeSet<_>>();
        let index = sheets.iter().position(|s| s.id() == sheet.id()).unwrap();
        let previous_positions = base
            .definitions()
            .review_notes(sheet.id())
            .iter()
            .filter(|n| !n.source.as_ref().is_some_and(ReviewSource::threaded))
            .filter_map(|n| {
                let (r, c) = (n.at.row(), n.at.column());
                if let Some(m) = &moves[index] {
                    m.area((r, c, r, c)).map(|a| (a.0, a.1))
                } else {
                    Some((r, c))
                }
            })
            .collect::<BTreeSet<_>>();
        if desired_positions != previous_positions {
            let removed = previous_positions
                .difference(&desired_positions)
                .copied()
                .collect();
            finish_vml(
                package,
                workbook,
                sheet.part().name(),
                &notes,
                &removed,
                out,
            )?;
        }
    }
    Ok(())
}
fn finish_vml(
    package: &Package,
    workbook: &str,
    sheet: &str,
    notes: &[&ReviewNote],
    removed: &BTreeSet<(u32, u32)>,
    out: &mut Changes,
) -> Result<(), SaveRefusal> {
    let name = related(package, sheet, "/vmlDrawing")
        .unwrap_or_else(|| new_part(package, out, workbook, "drawings", "vml"));
    let empty=b"<xml xmlns:v=\"urn:schemas-microsoft-com:vml\" xmlns:o=\"urn:schemas-microsoft-com:office:office\" xmlns:x=\"urn:schemas-microsoft-com:office:excel\"></xml>";
    let xml = parts::current_part(package, out, &name, empty)?;
    let mut shape_sources =
        super::reader::element_sources(&xml, |e| e.name.is(VML, "shape"))?.into_iter();
    let mut reader = Reader::new(&xml);
    let mut splice = Splice::new(xml.bytes());
    let mut positions = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut types = BTreeSet::new();
    while let Some((event, span)) = reader.next()? {
        if let XmlEvent::Start(e) = event {
            if let Some(id) = e.attribute("id") {
                ids.insert(id.to_owned());
                if e.name.is(VML, "shapetype") {
                    types.insert(id.to_owned());
                }
            }
            if e.name.is(VML, "shape") {
                let end = reader.element_end()?;
                let shape = XmlPart::new(
                    "note-shape.xml".into(),
                    shape_sources
                        .next()
                        .ok_or_else(|| reader.invalid("Missing note shape source"))?,
                    XmlLimits::default(),
                );
                let mut events = crate::load::Events::new(&shape);
                let mut note = false;
                let (mut row, mut col) = (None, None);
                while let Some(event) = events.next().map_err(|e| SaveRefusal::InvalidPart {
                    part: name.clone(),
                    detail: e.to_string(),
                })? {
                    if let XmlEvent::Start(e) = event {
                        if e.name.is(EXCEL, "ClientData") {
                            note = e.attribute("ObjectType") == Some("Note");
                        }
                        if note && (e.name.is(EXCEL, "Row") || e.name.is(EXCEL, "Column")) {
                            let text = events.text().map_err(|e| SaveRefusal::InvalidPart {
                                part: name.clone(),
                                detail: e.to_string(),
                            })?;
                            let value = text
                                .trim()
                                .parse::<u32>()
                                .ok()
                                .and_then(|n| n.checked_add(1));
                            if e.name.local == "Row" {
                                row = value;
                            } else {
                                col = value;
                            }
                        }
                    }
                }
                if note && let (Some(row), Some(col)) = (row, col) {
                    if notes
                        .iter()
                        .any(|n| n.at.row() == row && n.at.column() == col)
                    {
                        positions.insert((row, col));
                    } else if removed.contains(&(row, col)) {
                        splice.replace((span.0, end), &[]);
                    }
                }
            }
        }
    }
    let mut added = Vec::new();
    let type_id = if types.contains("_x0000_t202") {
        "_x0000_t202".to_owned()
    } else {
        let id = if !ids.contains("_x0000_t202") {
            "_x0000_t202".to_owned()
        } else {
            (1u32..)
                .map(|n| format!("_dna_note_type{n}"))
                .find(|id| !ids.contains(id))
                .expect("shape type")
        };
        added.extend(format!("<v:shapetype xmlns:v=\"{VML}\" xmlns:o=\"urn:schemas-microsoft-com:office:office\" id=\"{id}\" coordsize=\"21600,21600\" o:spt=\"202\" path=\"m,l,21600r21600,l21600,xe\"><v:stroke joinstyle=\"miter\"/><v:path gradientshapeok=\"t\" o:connecttype=\"rect\"/></v:shapetype>").as_bytes());
        id
    };
    for note in notes {
        if positions.insert((note.at.row(), note.at.column())) {
            let id = (1025u32..)
                .map(|n| format!("_x0000_s{n}"))
                .find(|id| !ids.contains(id))
                .expect("shape identity");
            ids.insert(id.clone());
            let row = note.at.row() - 1;
            let col = note.at.column() - 1;
            let top = row.min(calc_doc_model::value::MAX_ROWS - 4);
            let left = col.saturating_add(1).min(calc_doc_model::value::MAX_COLUMNS - 4);
            added.extend(format!("<v:shape xmlns:v=\"{VML}\" xmlns:o=\"urn:schemas-microsoft-com:office:office\" xmlns:x=\"{EXCEL}\" id=\"{id}\" type=\"#{type_id}\" style=\"position:absolute;width:144pt;height:72pt;visibility:hidden\" fillcolor=\"#ffffe1\" o:insetmode=\"auto\"><v:fill color2=\"#ffffe1\"/><v:shadow on=\"t\" color=\"black\" obscured=\"t\"/><v:path o:connecttype=\"none\"/><v:textbox style=\"mso-direction-alt:auto\"><div style=\"text-align:left\"/></v:textbox><x:ClientData ObjectType=\"Note\"><x:MoveWithCells/><x:SizeWithCells/><x:Anchor>{left}, 15, {top}, 2, {}, 31, {}, 9</x:Anchor><x:AutoFill>False</x:AutoFill><x:Row>{row}</x:Row><x:Column>{col}</x:Column></x:ClientData></v:shape>",left+3,top+3).as_bytes());
        }
    }
    let current = xml.with_bytes(splice.finish());
    parts::put_part(package, out, &name, parts::append_root(&current, &added)?);
    parts::ensure_content_type(
        package,
        out,
        &name,
        "application/vnd.openxmlformats-officedocument.vmlDrawing",
    )?;
    let id = parts::ensure_relationship(package, out, sheet, VML_REL, &name)?;
    let xml = parts::current_part(
        package,
        out,
        sheet,
        super::sheets::read_sheet(package, sheet)?.bytes(),
    )?;
    let mut reader = Reader::new(&xml);
    let mut present = false;
    while let Some((event, _)) = reader.next()? {
        if let XmlEvent::Start(e) = event
            && e.name.is(MAIN_NAMESPACE, "legacyDrawing")
        {
            present = true;
        }
    }
    if !present {
        let body = format!(
            "<legacyDrawing xmlns=\"{MAIN_NAMESPACE}\" xmlns:r=\"{REL_ATTR}\" r:id=\"{id}\"/>"
        );
        parts::put_part(
            package,
            out,
            sheet,
            parts::worksheet_child(
                &xml,
                "legacyDrawing",
                body.as_bytes(),
                &[
                    "legacyDrawingHF",
                    "picture",
                    "oleObjects",
                    "controls",
                    "webPublishItems",
                    "tableParts",
                    "extLst",
                ],
            )?,
        );
    }
    Ok(())
}
