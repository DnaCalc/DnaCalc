//! Write a worksheet's `sheetProtection` (ECMA-376 18.3.1.85) as a macro's `Worksheet.Protect` set it (dc-9b7zd).
//! The attributes and their order are Excel's own (ww24 g/case10): the password hash first, then `sheet`, `objects`,
//! `scenarios` (each only when set), then the commands the protection allows, written `0`.
use super::{
    SaveRefusal, SheetProtectionSave,
    reader::Reader,
    text::RawTag,
};
use crate::{
    load::MAIN_NAMESPACE,
    xml::{XmlEvent, XmlPart},
};

/// The allowed-command attributes in `FileProtection::allow` order.
const COMMANDS: [&str; 11] = [
    "formatCells", "formatColumns", "formatRows", "insertColumns", "insertRows", "insertHyperlinks", "deleteColumns",
    "deleteRows", "sort", "autoFilter", "pivotTables",
];

fn element(prefix: &str, protection: &SheetProtectionSave) -> Vec<u8> {
    let file = &protection.file;
    let mut out = format!("<{prefix}sheetProtection");
    if !file.hash.is_empty() {
        out += &format!(
            " algorithmName=\"{}\" hashValue=\"{}\" saltValue=\"{}\" spinCount=\"{}\"",
            file.algorithm, file.hash, file.salt, file.spin_count
        );
    }
    for (name, on) in [("sheet", protection.contents), ("objects", file.objects), ("scenarios", file.scenarios)] {
        if on {
            out += &format!(" {name}=\"1\"");
        }
    }
    for (name, allowed) in COMMANDS.iter().zip(file.allow) {
        if allowed {
            out += &format!(" {name}=\"0\"");
        }
    }
    out += "/>";
    out.into_bytes()
}

/// `xml` with its `sheetProtection` replaced by `protection`, or removed when `None`.
pub(super) fn rewrite(xml: &XmlPart, protection: Option<&SheetProtectionSave>) -> Result<Vec<u8>, SaveRefusal> {
    let mut r = Reader::new(xml);
    let (_, root) = r.root("worksheet")?;
    let tag = RawTag::parse(&xml.bytes()[root.0..root.1]);
    let prefix = String::from_utf8_lossy(tag.prefix()).into_owned();
    let after = super::page_setup::rank("worksheet", "sheetProtection");
    // The byte range of the old element, and where a new one goes when there is none.
    let (mut old, mut insert_at) = (None, None);
    let closing = loop {
        match r.expect()? {
            (XmlEvent::Start(e), span) => {
                let end = r.element_end()?;
                if e.name.is(MAIN_NAMESPACE, "sheetProtection") {
                    old = Some((span.0, end));
                } else if insert_at.is_none()
                    && e.name.namespace.as_deref() == Some(MAIN_NAMESPACE)
                    && super::page_setup::rank("worksheet", &e.name.local) > after
                {
                    insert_at = Some(span.0);
                }
            }
            (XmlEvent::Text(_), _) => {}
            (XmlEvent::End(_), (closing, _)) => break closing,
        }
    };
    let bytes = xml.bytes();
    let mut out = Vec::with_capacity(bytes.len() + 128);
    match (old, protection) {
        (Some((start, end)), replacement) => {
            out.extend_from_slice(&bytes[..start]);
            if let Some(protection) = replacement {
                out.extend(element(&prefix, protection));
            }
            out.extend_from_slice(&bytes[end..]);
        }
        (None, Some(protection)) => {
            let at = insert_at.unwrap_or(closing);
            out.extend_from_slice(&bytes[..at]);
            out.extend(element(&prefix, protection));
            out.extend_from_slice(&bytes[at..]);
        }
        (None, None) => out.extend_from_slice(bytes),
    }
    Ok(out)
}
