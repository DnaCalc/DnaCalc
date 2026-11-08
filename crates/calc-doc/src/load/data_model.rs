//! The workbook's data model, as far as a CUBE function needs it: the connections of
//! `xl/connections.xml`, and the measures in `xl/model/item.data`.
//!
//! The model part is an Analysis Services backup. Its UTF-16 `<VirtualDirectory>` lists each stored
//! file's offset and size, and its `LOG` file names them; `MdxScript.*.scr.xml` holds the measures
//! as `CREATE MEASURE 'Table1'[TotalAmount]=SUM(Table1[Amount]);`. A stored file is a run of blocks,
//! each a 16-bit uncompressed size and compressed size then its bytes: raw when the two sizes
//! are equal, otherwise the plain LZ77 Xpress form of [MS-XCA] 2.4. Read from Excel 16.0.20527's own
//! files (2026-11-07). A part that does not read this way leaves the model without measures.

use super::{Events, LoadError, read_xml};
use crate::opc::Package;
use crate::xml::{XmlEvent, XmlName};
use calc_doc_model::{DataModel, ModelMeasure};

/// The workbook's connections and model, when its relationships name a connections part.
pub(crate) fn read(package: &Package, workbook: &str) -> Result<Option<DataModel>, LoadError> {
    let Some(part) = super::metadata::related(package, workbook, "/connections") else {
        return Ok(None);
    };
    let xml = read_xml(package, &part)?;
    let mut events = Events::new(&xml);
    let mut model = DataModel::default();
    let mut from_worksheet = false;
    let mut connection_types = Vec::new();
    let mut connection_ids = std::collections::BTreeMap::<u32, usize>::new();
    let mut current_connection_id = None;
    let mut current_is_model_type = false;
    let mut model_connections = 0usize;
    let mut path = Vec::<XmlName>::new();
    let mut model_extension_uri = false;
    const MODEL_EXTENSION: &str = "{DE250136-89BD-433C-8126-D09CA5730AF9}";
    const X15: &str = "http://schemas.microsoft.com/office/spreadsheetml/2010/11/main";
    while let Some(event) = events.next()? {
        match event {
            XmlEvent::Start(element) => {
                if path.len() == 1 && path[0].is(super::MAIN_NAMESPACE, "connections")
                    && element.name.is(super::MAIN_NAMESPACE, "connection") {
                    current_connection_id = element.attribute("id").and_then(|id| id.parse::<u32>().ok()).filter(|id| *id != 0);
                    current_is_model_type = element.attribute("type") == Some("5");
                    if let Some(id) = current_connection_id { *connection_ids.entry(id).or_default() += 1; }
                }
                if path.len() == 3 && path[0].is(super::MAIN_NAMESPACE, "connections")
                    && path[1].is(super::MAIN_NAMESPACE, "connection") && path[2].is(super::MAIN_NAMESPACE, "extLst")
                    && element.name.is(super::MAIN_NAMESPACE, "ext") {
                    model_extension_uri = element.attribute("uri") == Some(MODEL_EXTENSION);
                }
                if element.name.local == "connection" {
                    if element.attribute("model") == Some("1") {
                        model_connections += 1;
                        let model_extension = path.len() == 4 && path[0].is(super::MAIN_NAMESPACE, "connections")
                            && path[1].is(super::MAIN_NAMESPACE, "connection")
                            && path[2].is(super::MAIN_NAMESPACE, "extLst") && path[3].is(super::MAIN_NAMESPACE, "ext");
                        model.model_connection_id = if model_extension && model_extension_uri && current_is_model_type && !from_worksheet
                            && element.attribute("name").is_none() && element.name.is(X15, "connection") {
                            current_connection_id
                        } else { None };
                    }
                    if let Some(name) = element.attribute("name") {
                        model.connections.push(name.to_owned());
                        connection_types.push((name.to_owned(), element.attribute("type").map(str::to_owned)));
                        // A worksheet table feeding the model: `x15:connection id` names it.
                        from_worksheet = element.attribute("type") == Some("102");
                    } else if from_worksheet && let Some(id) = element.attribute("id") {
                        model.tables.push(id.to_owned());
                    } else if element.attribute("model") == Some("1") && let Some(last) = model.connections.last() {
                        model.model_connection = Some(last.clone());
                    }
                }
                path.push(element.name);
            }
            XmlEvent::End(name) => {
                if path.len() == 4 && name.is(super::MAIN_NAMESPACE, "ext") { model_extension_uri = false; }
                if path.len() == 2 && name.is(super::MAIN_NAMESPACE, "connection") {
                    current_connection_id = None;
                    current_is_model_type = false;
                }
                path.pop();
            }
            XmlEvent::Text(_) => {}
        }
    }
    if model_connections != 1 || model.model_connection_id.is_none_or(|id| connection_ids.get(&id) != Some(&1)) {
        model.model_connection_id = None;
    }
    model.other_connections = connection_types.into_iter().filter(|(name, kind)| {
        kind.as_deref() != Some("102") && model.model_connection.as_ref().is_none_or(|model_name| !calc_doc_model::value::workbook_names_equal(name, model_name))
    }).map(|(name, _)| name).collect();
    if model.model_connection.is_some()
        && let Some(part) = super::metadata::related(package, workbook, "/powerPivotData")
        && let Ok(bytes) = package.read_part(&part)
    {
        if let Some((measures, tables)) = contents(&bytes) {
            model.measures = measures;
            model.model_tables = tables;
        }
    }
    Ok(Some(model))
}

fn utf16_text(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

fn utf16_needle(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|at| at + from)
}

fn between<'t>(text: &'t str, open: &str, close: &str) -> Option<&'t str> {
    let start = text.find(open)? + open.len();
    let end = text[start..].find(close)? + start;
    Some(&text[start..end])
}

/// The measures and table names of the model part; `None` when it does not read as a model
/// backup.
fn contents(bytes: &[u8]) -> Option<(Vec<ModelMeasure>, Vec<String>)> {
    let open = utf16_needle("<VirtualDirectory>");
    let close = utf16_needle("</VirtualDirectory>");
    let start = find(bytes, &open, 0)?;
    let end = find(bytes, &close, start)?;
    let directory = utf16_text(&bytes[start..end]);
    let mut stored = Vec::new();
    for entry in directory.split("<BackupFile>").skip(1) {
        stored.push((
            between(entry, "<Path>", "</Path>")?.to_owned(),
            between(entry, "<Size>", "</Size>")?.parse::<usize>().ok()?,
            between(entry, "<m_cbOffsetHeader>", "</m_cbOffsetHeader>")?
                .parse::<usize>()
                .ok()?,
        ));
    }
    let (_, size, offset) = stored.iter().find(|(path, ..)| path == "LOG")?;
    let log = utf16_text(bytes.get(*offset..offset + size)?);
    let mut measures = Vec::new();
    let mut tables = Vec::new();
    for entry in log.split("<BackupFile>").skip(1) {
        let Some(path) = between(entry, "<Path>", "</Path>") else {
            continue;
        };
        let file = path.rsplit('\\').next().unwrap_or(path);
        // `Table1.2.dim.xml`: a table of the model, whatever feeds it.
        if let Some(stem) = file.strip_suffix(".dim.xml")
            && let Some((table, _)) = stem.rsplit_once('.')
        {
            tables.push(table.to_owned());
        }
        if !(file.starts_with("MdxScript.") && file.ends_with(".scr.xml")) {
            continue;
        }
        let storage = between(entry, "<StoragePath>", "</StoragePath>")?;
        let (_, size, offset) = stored.iter().find(|(path, ..)| path == storage)?;
        let script = decompressed(bytes.get(*offset..offset + size)?)?;
        measures.extend(script_measures(&String::from_utf8_lossy(&script)));
    }
    Some((measures, tables))
}

/// A stored file's bytes, block by block, up to its trailer.
fn decompressed(file: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(header) = file.get(at..at + 4) {
        let plain = usize::from(u16::from_le_bytes([header[0], header[1]]));
        let stored = usize::from(u16::from_le_bytes([header[2], header[3]]));
        // The file ends in a 4-byte trailer, which reads as no block that fits.
        let Some(block) = file.get(at + 4..at + 4 + stored) else {
            break;
        };
        at += 4 + stored;
        if plain == stored {
            out.extend_from_slice(block);
        } else {
            lz77(block, plain, &mut out)?;
        }
    }
    Some(out)
}

/// [MS-XCA] 2.4, plain LZ77, of one block of up to `plain` bytes. The block's input can end first.
fn lz77(block: &[u8], plain: usize, out: &mut Vec<u8>) -> Option<()> {
    let base = out.len();
    let (mut at, mut flags, mut left, mut half) = (0, 0u32, 0, None::<usize>);
    while out.len() - base < plain {
        if left == 0 {
            let Some(word) = block.get(at..at + 4) else {
                return Some(());
            };
            flags = u32::from_le_bytes(word.try_into().ok()?);
            at += 4;
            left = 32;
        }
        left -= 1;
        if flags >> left & 1 == 0 {
            let Some(byte) = block.get(at) else {
                return Some(());
            };
            out.push(*byte);
            at += 1;
            continue;
        }
        let Some(pair) = block.get(at..at + 2) else {
            return Some(());
        };
        let pair = u16::from_le_bytes(pair.try_into().ok()?);
        at += 2;
        let offset = usize::from(pair >> 3) + 1;
        let mut length = usize::from(pair & 7);
        if length == 7 {
            length = match half.take() {
                Some(held) => usize::from(*block.get(held)? >> 4),
                None => {
                    half = Some(at);
                    at += 1;
                    usize::from(*block.get(at - 1)? & 15)
                }
            };
            if length == 15 {
                length = usize::from(*block.get(at)?);
                at += 1;
                if length == 255 {
                    length = usize::from(u16::from_le_bytes(
                        block.get(at..at + 2)?.try_into().ok()?,
                    ));
                    at += 2;
                    if length == 0 {
                        length = usize::try_from(u32::from_le_bytes(
                            block.get(at..at + 4)?.try_into().ok()?,
                        ))
                        .ok()?;
                        at += 4;
                    }
                    length = length.checked_sub(22)?;
                }
                length += 15;
            }
            length += 7;
        }
        length += 3;
        for _ in 0..length {
            let byte = *out.get(out.len().checked_sub(offset)?)?;
            out.push(byte);
        }
    }
    Some(())
}

/// The `CREATE MEASURE` statements of an MDX script.
fn script_measures(script: &str) -> Vec<ModelMeasure> {
    let script = script
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&")
        .replace("&apos;", "'")
        .replace("&quot;", "\"");
    let mut found = Vec::new();
    for statement in script.split("CREATE MEASURE").skip(1) {
        let Some((head, rest)) = statement.split_once('=') else {
            continue;
        };
        let Some(name) = head
            .rsplit_once('[')
            .and_then(|(_, name)| name.trim_end().strip_suffix(']'))
        else {
            continue;
        };
        let expression = rest.split(';').next().unwrap_or(rest).trim();
        found.push(ModelMeasure {
            name: name.to_owned(),
            expression: expression.to_owned(),
        });
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_plain_block_and_a_measure_script_read() {
        let mut block = vec![0, 0, 0, 0];
        block.extend_from_slice(b"abcd");
        let mut out = Vec::new();
        assert_eq!(lz77(&block, 4, &mut out), Some(()));
        assert_eq!(out, b"abcd");
        // A flag of 1 with offset 2 and length 3: "ababa".
        let mut block = 0x2000_0000_u32.to_le_bytes().to_vec();
        block.extend_from_slice(b"ab");
        block.extend_from_slice(&(1_u16 << 3).to_le_bytes());
        let mut out = Vec::new();
        assert_eq!(lz77(&block, 5, &mut out), Some(()));
        assert_eq!(out, b"ababa");
        let found = script_measures(
            "<Text>-- PowerPivot\nCREATE MEASURE 'Table1'[TotalAmount]=SUM(Table1[Amount]);\nCREATE MEASURE 'Table1'[Rows]=COUNTROWS(Table1);\n</Text>",
        );
        assert_eq!(found[0].name, "TotalAmount");
        assert_eq!(found[0].expression, "SUM(Table1[Amount])");
        assert_eq!(found[1].expression, "COUNTROWS(Table1)");
    }
}
