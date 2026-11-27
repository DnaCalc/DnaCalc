//! Append native Scenario Manager records while retaining the worksheet's source XML.

use super::{
    SaveRefusal,
    page_setup::rank,
    reader::Reader,
    text::{RawTag, Splice, a1, xstring_attribute},
};
use crate::{
    load::MAIN_NAMESPACE,
    xml::{XmlEvent, XmlPart},
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

pub(super) fn append(xml: &XmlPart, records: &[WorksheetScenario]) -> Result<Vec<u8>, SaveRefusal> {
    let entries = encode(records);
    let mut reader = Reader::new(xml);
    reader.root("worksheet")?;
    let mut splice = Splice::new(xml.bytes());
    let mut insert_at = None;
    loop {
        match reader.expect()? {
            (XmlEvent::Start(element), span) if element.name.is(MAIN_NAMESPACE, "scenarios") => {
                let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
                let closing = loop {
                    match reader.expect()? {
                        (XmlEvent::Start(_), _) => {
                            reader.element_end()?;
                        }
                        (XmlEvent::End(_), span) => break span.0,
                        _ => {}
                    }
                };
                if tag.is_empty() {
                    let mut bytes = tag.rebuilt(&[], false);
                    bytes.extend_from_slice(&entries);
                    bytes.extend_from_slice(b"</");
                    bytes.extend_from_slice(tag.qualified_name());
                    bytes.push(b'>');
                    splice.replace(span, &bytes);
                } else {
                    splice.replace((closing, closing), &entries);
                }
                return Ok(splice.finish());
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
                let at = insert_at.unwrap_or(span.0);
                let mut bytes = format!("<scenarios xmlns=\"{MAIN_NAMESPACE}\">").into_bytes();
                bytes.extend_from_slice(&entries);
                bytes.extend_from_slice(b"</scenarios>");
                splice.replace((at, at), &bytes);
                return Ok(splice.finish());
            }
            _ => {}
        }
    }
}
