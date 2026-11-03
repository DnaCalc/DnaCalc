//! Text as SpreadsheetML stores it: string items (`<si>` in the shared strings part, `<is>` in an
//! inline string cell) and the `_xHHHH_` escapes of its string type.

use super::error::LoadError;
use super::{Events, MAIN_NAMESPACE};
use crate::opc::Package;
use crate::xml::XmlEvent;
use calc_doc_model::value::ExcelText;

/// Text with each `_xHHHH_` escape (four hexadecimal digits, either case) replaced by the UTF-16
/// code unit it names, as ECMA-376's ST_Xstring writes characters XML cannot hold (`_x000D_`) and
/// an underscore that would otherwise start one (`_x005F_`). An unpaired surrogate is kept.
pub(crate) fn decode(text: &str) -> Vec<u16> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let hex = |unit: u16| char::from_u32(u32::from(unit)).and_then(|c| c.to_digit(16));
    let mut out = Vec::with_capacity(units.len());
    let mut index = 0;
    while index < units.len() {
        if units.len() - index >= 7
            && units[index] == u16::from(b'_')
            && units[index + 1] == u16::from(b'x')
            && units[index + 6] == u16::from(b'_')
        {
            let digits: Option<Vec<u32>> = units[index + 2..index + 6]
                .iter()
                .map(|&u| hex(u))
                .collect();
            if let Some(digits) = digits {
                let value = digits.iter().fold(0u32, |value, digit| value * 16 + digit);
                out.push(value as u16);
                index += 7;
                continue;
            }
        }
        out.push(units[index]);
        index += 1;
    }
    out
}

/// The text of a string item whose start tag `events` has just read: the `<t>` elements directly
/// inside it and inside its runs (`<r>`), in order, each decoded on its own. Phonetic runs
/// (`<rPh>`), phonetic properties and run properties are not part of the text.
pub(crate) fn string_item(events: &mut Events<'_>) -> Result<Vec<u16>, LoadError> {
    let mut text = Vec::new();
    loop {
        match events.next()? {
            Some(XmlEvent::Start(element)) => {
                if element.name.is(MAIN_NAMESPACE, "t") {
                    text.extend(decode(&events.text()?));
                } else if element.name.is(MAIN_NAMESPACE, "r") {
                    loop {
                        match events.next()? {
                            Some(XmlEvent::Start(inner)) => {
                                if inner.name.is(MAIN_NAMESPACE, "t") {
                                    text.extend(decode(&events.text()?));
                                } else {
                                    events.skip()?;
                                }
                            }
                            Some(XmlEvent::Text(_)) => {}
                            Some(XmlEvent::End(_)) => break,
                            None => return Err(events.invalid("the part ends inside a run")),
                        }
                    }
                } else {
                    events.skip()?;
                }
            }
            Some(XmlEvent::Text(_)) => {}
            Some(XmlEvent::End(_)) => return Ok(text),
            None => return Err(events.invalid("the part ends inside a string item")),
        }
    }
}

/// The workbook's shared strings, in the order their part lists them.
pub(super) fn shared_strings(package: &Package, part: &str) -> Result<Vec<ExcelText>, LoadError> {
    let xml = super::read_xml(package, part)?;
    let mut events = Events::new(&xml);
    events.root(
        MAIN_NAMESPACE,
        "sst",
        "the document element is not a SpreadsheetML sst",
    )?;
    let mut strings = Vec::new();
    loop {
        match events.next()? {
            Some(XmlEvent::Start(element)) => {
                if element.name.is(MAIN_NAMESPACE, "si") {
                    strings.push(ExcelText::from_utf16_code_units(string_item(&mut events)?));
                } else {
                    events.skip()?;
                }
            }
            Some(XmlEvent::Text(text)) if text.trim().is_empty() => {}
            Some(XmlEvent::Text(_)) => return Err(events.invalid("sst holds text")),
            Some(XmlEvent::End(_)) => break,
            None => return Err(events.invalid("the part ends inside sst")),
        }
    }
    events.end()?;
    Ok(strings)
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn escapes() {
        assert_eq!(decode("a_x000D_b"), vec![0x61, 0x0D, 0x62]);
        assert_eq!(
            decode("_x005F_x0041_"),
            "_x0041_".encode_utf16().collect::<Vec<_>>()
        );
        assert_eq!(decode("_xd83d_"), vec![0xD83D]);
        assert_eq!(
            decode("_x12G4_ _x12_"),
            "_x12G4_ _x12_".encode_utf16().collect::<Vec<_>>()
        );
        assert_eq!(decode("\u{1F600}"), vec![0xD83D, 0xDE00]);
    }
}
