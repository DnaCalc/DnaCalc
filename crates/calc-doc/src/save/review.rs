//! Replace only the text of an identified note or threaded comment. Source attributes, authors,
//! rich metadata and unrelated comments retain their original bytes.
use super::{
    SaveRefusal,
    reader::Reader,
    text::{RawTag, Splice, xstring},
};
use crate::{
    load::{MAIN_NAMESPACE, review::THREAD_NAMESPACE},
    xml::{XmlEvent, XmlPart},
};
use calc_doc_model::{ReviewNote, ReviewSource};

pub(super) fn rewrite(
    xml: &XmlPart,
    notes: &[&ReviewNote],
    removed: &std::collections::BTreeSet<String>,
) -> Result<Vec<u8>, SaveRefusal> {
    let mut reader = Reader::new(xml);
    let mut splice = Splice::new(xml.bytes());
    let mut matched = std::collections::BTreeSet::new();
    while let Some((event, span)) = reader.next()? {
        let XmlEvent::Start(e) = event else {
            continue;
        };
        let thread = e.name.is(THREAD_NAMESPACE, "threadedComment");
        if !thread && !e.name.is(MAIN_NAMESPACE, "comment") {
            continue;
        }
        if thread && e.attribute("id").is_some_and(|id| removed.contains(id)) {
            let end = reader.element_end()?;
            splice.replace((span.0, end), &[]);
            continue;
        }
        let note = notes.iter().enumerate().find(|(_, n)| match &n.source {
            Some(ReviewSource::Note { .. }) if !thread => {
                e.attribute("ref").and_then(crate::load::address::cell)
                    == Some((n.at.row(), n.at.column()))
            }
            Some(ReviewSource::Thread { id, .. }) if thread => {
                e.attribute("id") == Some(id.as_str())
            }
            _ => false,
        });
        let Some((index, note)) = note else {
            reader.element_end()?;
            continue;
        };
        if !matched.insert(index) {
            return Err(reader.invalid("the note or comment source identity is duplicated"));
        }
        let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
        let prefix = tag.prefix().to_vec();
        let mut text_span = None;
        let close;
        loop {
            match reader.expect()? {
                (XmlEvent::Start(child), start) => {
                    let end = reader.element_end()?;
                    if child.name.is(
                        if thread {
                            THREAD_NAMESPACE
                        } else {
                            MAIN_NAMESPACE
                        },
                        "text",
                    ) {
                        if text_span.is_some() {
                            return Err(reader.invalid("duplicate comment text"));
                        }
                        text_span = Some((start.0, end));
                    }
                }
                (XmlEvent::End(_), end) => {
                    close = end;
                    break;
                }
                _ => {}
            }
        }
        let mut text = Vec::new();
        let prefix = String::from_utf8_lossy(&prefix);
        text.extend_from_slice(format!("<{prefix}text>").as_bytes());
        if !thread {
            text.extend_from_slice(format!("<{prefix}t xml:space=\"preserve\">").as_bytes());
        }
        xstring(&note.text.encode_utf16().collect::<Vec<_>>(), &mut text);
        if !thread {
            text.extend_from_slice(format!("</{prefix}t>").as_bytes());
        }
        text.extend_from_slice(format!("</{prefix}text>").as_bytes());
        if let Some(span) = text_span {
            splice.replace(span, &text);
        } else if tag.is_empty() {
            let mut replacement = tag.rebuilt(&[], false);
            replacement.extend_from_slice(&text);
            replacement.extend_from_slice(
                format!(
                    "</{prefix}{}>",
                    if thread { "threadedComment" } else { "comment" }
                )
                .as_bytes(),
            );
            splice.replace(span, &replacement);
        } else {
            splice.replace((close.0, close.0), &text);
        }
    }
    if matched.len() != notes.len() {
        return Err(
            reader.invalid("a note or comment source could not be found; nothing was saved")
        );
    }
    Ok(splice.finish())
}
