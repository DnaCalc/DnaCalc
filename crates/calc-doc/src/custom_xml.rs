//! Custom XML items: reading the item parts of a package whose document element is in a given
//! namespace, and the identity a save gives an item it adds.
//!
//! A custom XML item is a part reached by a relationship of type `customXml` (the DNA view
//! manifest is one, `load::manifest`). An application keeps a few settings Excel's format has no
//! place for in one item of its own namespace: Excel keeps such an item and its item properties
//! when it saves the workbook. The codec treats the item as opaque bytes: it finds an item by
//! the namespace of its document element, and a save writes one the same way (a new item with
//! its item properties, its relationship from the workbook part and its content types, or the
//! content of the item the package already holds for that namespace).
//! [`crate::DocumentSnapshot::with_custom_xml`] is the write side, and
//! [`crate::LoadedWorkbook::custom_xml`] the read side.

use crate::load::manifest::CUSTOM_XML;
use crate::opc::{Package, TargetMode};
use crate::xml::XmlEvent;
use crate::zip::crc32::Crc32;

/// A custom XML item an application keeps: the namespace of its document element and its bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomXmlItem {
    pub namespace: String,
    /// The part's content: a complete XML document whose document element is in `namespace`.
    pub content: Vec<u8>,
}

/// The names of the custom XML item parts the package's relationships reach whose document
/// element is in `namespace`, each once, in the order the relationships parts list them.
pub(crate) fn parts_in(package: &Package, namespace: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for relationships in package.relationships() {
        for item in relationships.items() {
            if item.kind != CUSTOM_XML || item.mode != TargetMode::Internal {
                continue;
            }
            let Some(part) = item.resolved.as_deref().and_then(|name| package.part(name)) else {
                continue;
            };
            if found
                .iter()
                .any(|name| name.eq_ignore_ascii_case(part.name()))
            {
                continue;
            }
            let Ok(xml) = package.read_xml(part.name()) else {
                continue;
            };
            if let Ok(Some(XmlEvent::Start(element))) = xml.reader().next_event()
                && element.name.namespace.as_deref() == Some(namespace)
            {
                found.push(part.name().to_owned());
            }
        }
    }
    found
}

/// The datastore item ID a new item of `namespace` gets: a GUID derived from the namespace, so
/// the same application's items carry the same ID in every package.
pub(crate) fn item_id(namespace: &str) -> String {
    let mut words = [0u32; 3];
    for (index, word) in words.iter_mut().enumerate() {
        let mut crc = Crc32::new();
        crc.update(&[index as u8 + 1]);
        crc.update(namespace.as_bytes());
        *word = crc.finish();
    }
    format!(
        "{{{:08X}-{:04X}-4{:03X}-8{:03X}-{:04X}{:08X}}}",
        words[0],
        words[1] >> 16,
        words[1] & 0xFFF,
        (words[2] >> 20) & 0xFFF,
        words[2] & 0xFFFF,
        words[0] ^ words[2]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn an_item_id_is_a_guid_of_the_namespace() {
        let id = item_id("urn:example:one");
        assert_eq!(id.len(), 38, "{id}");
        assert!(id.starts_with('{') && id.ends_with('}'));
        assert_eq!(id, item_id("urn:example:one"));
        assert_ne!(id, item_id("urn:example:two"));
    }
}
