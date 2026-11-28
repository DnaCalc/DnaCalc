//! Update native Scenario Manager records while retaining their worksheet XML.

use super::{
    SaveRefusal,
    page_setup::rank,
    reader::Reader,
    text::{RawTag, Splice, a1, xstring_attribute},
};
use crate::{
    load::{MAIN_NAMESPACE, strings::decode},
    xml::{XmlElement, XmlEvent, XmlPart},
};
use calc_doc_model::WorksheetScenario;
use std::io::Write;

fn encode(records: &[WorksheetScenario]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for scenario in records {
        bytes.extend_from_slice(b"<scenario name=");
        xstring_attribute(
            &scenario.name.encode_utf16().collect::<Vec<_>>(),
            &mut bytes,
        );
        write!(
            bytes,
            " count=\"{}\" xmlns=\"{MAIN_NAMESPACE}\"",
            scenario.inputs.len()
        )
        .unwrap();
        flag(&mut bytes, "hidden", scenario.hidden);
        flag(&mut bytes, "locked", scenario.locked);
        bytes.push(b'>');
        for input in &scenario.inputs {
            write!(
                bytes,
                "<inputCells r=\"{}\" val=",
                a1(input.at.row(), input.at.column())
            )
            .unwrap();
            xstring_attribute(input.value.utf16_code_units(), &mut bytes);
            if input.number_format != 0 {
                write!(bytes, " numFmtId=\"{}\"", input.number_format).unwrap();
            }
            flag(&mut bytes, "deleted", input.deleted);
            flag(&mut bytes, "undone", input.undone);
            bytes.extend_from_slice(b"/>");
        }
        bytes.extend_from_slice(b"</scenario>");
    }
    bytes
}

fn flag(bytes: &mut Vec<u8>, name: &str, enabled: bool) {
    if enabled {
        write!(bytes, " {name}=\"1\"").unwrap();
    }
}

/// Map each retained source record to its new position. The worksheet editor only reorders
/// records by removing them, and appends newly authored records after the imported ones.
fn retained_indices(
    reader: &Reader<'_>,
    before: &[WorksheetScenario],
    after: &[WorksheetScenario],
) -> Result<Vec<Option<usize>>, SaveRefusal> {
    let mut retained = vec![None; before.len()];
    let mut next_source = 0;
    for (position, record) in after.iter().enumerate() {
        let Some(source) = record.source_index else {
            next_source = before.len();
            continue;
        };
        let Some(original) = before
            .get(source)
            .filter(|old| old.source_index == Some(source))
        else {
            return Err(reader.invalid("a scenario no longer matches its opened worksheet record"));
        };
        if source < next_source
            || original.inputs != record.inputs
            || original.hidden != record.hidden
            || original.locked != record.locked
        {
            return Err(reader.invalid(
                "reordering scenarios or editing their inputs is unavailable (dc-s2xup)",
            ));
        }
        retained[source] = Some(position);
        next_source = source + 1;
    }
    Ok(retained)
}

fn rebase_index(
    element: &XmlElement,
    name: &str,
    retained: &[Option<usize>],
) -> Option<Option<String>> {
    let original = element.attribute(name)?.parse::<usize>().ok()?;
    let next = *retained.get(original)?;
    (next != Some(original)).then(|| next.map(|index| index.to_string()))
}

fn rewrite_group(
    xml: &XmlPart,
    reader: &mut Reader<'_>,
    group: (XmlElement, (usize, usize)),
    before: &[WorksheetScenario],
    after: &[WorksheetScenario],
) -> Result<Vec<u8>, SaveRefusal> {
    let (element, start) = group;
    let tag = RawTag::parse(&xml.bytes()[start.0..start.1]);
    let retained = retained_indices(reader, before, after)?;
    let mut replacements = Vec::new();
    let mut source = 0;
    let mut other_content = false;
    let end = loop {
        match reader.expect()? {
            (XmlEvent::Start(child), span) if child.name.is(MAIN_NAMESPACE, "scenario") => {
                let limit = reader.element_end()?;
                let raw_name = child
                    .attribute("name")
                    .and_then(|name| String::from_utf16(&decode(name)).ok());
                let Some(old) = before
                    .get(source)
                    .filter(|record| raw_name.as_ref() == Some(&record.name))
                else {
                    return Err(reader
                        .invalid("the worksheet scenario records differ from the opened model"));
                };
                match retained[source].map(|index| &after[index]) {
                    Some(next) if next.name != old.name => {
                        let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
                        let units = next.name.encode_utf16().collect::<Vec<_>>();
                        replacements
                            .push((span, tag.rebuilt_xstring("name", &units, tag.is_empty())));
                    }
                    Some(_) => {}
                    None => replacements.push(((span.0, limit), Vec::new())),
                }
                source += 1;
            }
            (XmlEvent::Start(_), _) => {
                other_content = true;
                reader.element_end()?;
            }
            (XmlEvent::Text(text), _) => other_content |= !text.trim().is_empty(),
            (XmlEvent::End(_), span) => break span,
        }
    };
    if source != before.len() {
        return Err(reader.invalid("the worksheet is missing opened scenario records"));
    }

    let mut splice = Splice::new(xml.bytes());
    if after.is_empty() {
        if other_content {
            return Err(reader
                .invalid("removing unread Scenario Manager content is unavailable (dc-s2xup)"));
        }
        splice.replace((start.0, end.1), &[]);
        return Ok(splice.finish());
    }
    let changes: Vec<_> = ["current", "show"]
        .into_iter()
        .filter_map(|name| rebase_index(&element, name, &retained).map(|value| (name, value)))
        .collect();
    let attrs: Vec<_> = changes
        .iter()
        .map(|(name, value)| (*name, value.as_deref()))
        .collect();
    let added_at = after
        .iter()
        .position(|scenario| scenario.source_index.is_none())
        .unwrap_or(after.len());
    let entries = encode(&after[added_at..]);
    if tag.is_empty() {
        let mut bytes = tag.rebuilt(&attrs, false);
        bytes.extend_from_slice(&entries);
        bytes.extend_from_slice(b"</");
        bytes.extend_from_slice(tag.qualified_name());
        bytes.push(b'>');
        splice.replace(start, &bytes);
    } else {
        if !attrs.is_empty() {
            splice.replace(start, &tag.rebuilt(&attrs, false));
        }
        for (span, replacement) in replacements {
            splice.replace(span, &replacement);
        }
        splice.replace((end.0, end.0), &entries);
    }
    Ok(splice.finish())
}

pub(super) fn rewrite(
    xml: &XmlPart,
    before: &[WorksheetScenario],
    after: &[WorksheetScenario],
) -> Result<Vec<u8>, SaveRefusal> {
    let mut reader = Reader::new(xml);
    reader.root("worksheet")?;
    let mut splice = Splice::new(xml.bytes());
    let mut insert_at = None;
    loop {
        match reader.expect()? {
            (XmlEvent::Start(element), span) if element.name.is(MAIN_NAMESPACE, "scenarios") => {
                return rewrite_group(xml, &mut reader, (element, span), before, after);
            }
            (XmlEvent::Start(element), span) => {
                if element.name.namespace.as_deref() == Some(MAIN_NAMESPACE)
                    && rank("worksheet", element.name.local.as_str())
                        > rank("worksheet", "scenarios")
                {
                    insert_at.get_or_insert(span.0);
                }
                reader.element_end()?;
            }
            (XmlEvent::End(_), span) => {
                if !before.is_empty()
                    || after.iter().any(|scenario| scenario.source_index.is_some())
                {
                    return Err(reader
                        .invalid("the worksheet is missing its opened Scenario Manager group"));
                }
                if after.is_empty() {
                    return Ok(splice.finish());
                }
                let at = insert_at.unwrap_or(span.0);
                let mut bytes = format!("<scenarios xmlns=\"{MAIN_NAMESPACE}\">").into_bytes();
                bytes.extend_from_slice(&encode(after));
                bytes.extend_from_slice(b"</scenarios>");
                splice.replace((at, at), &bytes);
                return Ok(splice.finish());
            }
            _ => {}
        }
    }
}
