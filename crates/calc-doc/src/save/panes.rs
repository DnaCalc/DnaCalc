//! Rewrite the first worksheet window's pane metadata, retaining other window attributes and children.
use super::{SaveRefusal, reader::Reader, text::RawTag};
use crate::{
    load::{MAIN_NAMESPACE, address::column_letters},
    xml::{XmlEvent, XmlPart},
};
use calc_doc_model::FrozenPanes;

fn children(
    prefix: &str,
    panes: Option<FrozenPanes>,
    active: (u32, u32),
    selection: Option<&[u8]>,
) -> Vec<u8> {
    let address = format!("{}{}", column_letters(active.1), active.0);
    let mut out = String::new();
    let mut pane = None;
    if let Some(p) = panes {
        let right = p.columns() > 0 && active.1 > p.columns();
        let bottom = p.rows() > 0 && active.0 > p.rows();
        let name = match (bottom, right) {
            (false, false) => "topLeft",
            (false, true) => "topRight",
            (true, false) => "bottomLeft",
            (true, true) => "bottomRight",
        };
        pane = Some(name);
        out.push_str(&format!("<{prefix}pane"));
        if p.columns() > 0 {
            out.push_str(&format!(" xSplit=\"{}\"", p.columns()));
        }
        if p.rows() > 0 {
            out.push_str(&format!(" ySplit=\"{}\"", p.rows()));
        }
        out.push_str(&format!(
            " topLeftCell=\"{}{}\" activePane=\"{name}\" state=\"frozen\"/>",
            column_letters(p.columns() + 1),
            p.rows() + 1
        ));
    }
    if let Some(selection) = selection {
        let tag = RawTag::parse(selection);
        let mut bytes = out.into_bytes();
        bytes.extend(tag.rebuilt(&[("pane", pane)], true));
        return bytes;
    }
    out.push_str(&format!("<{prefix}selection"));
    if let Some(pane) = pane {
        out.push_str(&format!(" pane=\"{pane}\""));
    }
    out.push_str(&format!(" activeCell=\"{address}\" sqref=\"{address}\"/>"));
    out.into_bytes()
}

fn view(
    reader: &mut Reader<'_>,
    xml: &XmlPart,
    span: (usize, usize),
    panes: Option<FrozenPanes>,
    active: (u32, u32),
) -> Result<Vec<u8>, SaveRefusal> {
    let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
    let prefix = String::from_utf8_lossy(tag.prefix());
    let mut other = Vec::new();
    let mut selection = None;
    loop {
        match reader.expect()? {
            (XmlEvent::Start(child), start) => {
                let end = reader.element_end()?;
                if child.name.is(MAIN_NAMESPACE, "selection") {
                    if child
                        .attribute("activeCell")
                        .and_then(crate::load::address::cell)
                        .unwrap_or((1, 1))
                        == active
                    {
                        selection = Some(xml.bytes()[start.0..start.1].to_vec());
                    }
                } else if !child.name.is(MAIN_NAMESPACE, "pane") {
                    other.extend_from_slice(&xml.bytes()[start.0..end]);
                }
            }
            (XmlEvent::Text(_), span) => other.extend_from_slice(&xml.bytes()[span.0..span.1]),
            (XmlEvent::End(_), _) => break,
        }
    }
    let mut out = tag.rebuilt(&[("topLeftCell", Some("A1"))], false);
    out.extend(children(&prefix, panes, active, selection.as_deref()));
    out.extend(other);
    out.extend_from_slice(format!("</{prefix}sheetView>").as_bytes());
    Ok(out)
}

fn views(
    reader: &mut Reader<'_>,
    xml: &XmlPart,
    span: (usize, usize),
    panes: Option<FrozenPanes>,
    active: (u32, u32),
) -> Result<(usize, Vec<u8>), SaveRefusal> {
    let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
    let prefix = String::from_utf8_lossy(tag.prefix());
    let mut out = tag.rebuilt(&[], false);
    let mut first = true;
    let end = loop {
        match reader.expect()? {
            (XmlEvent::Start(child), span)
                if first && child.name.is(MAIN_NAMESPACE, "sheetView") =>
            {
                out.extend(view(reader, xml, span, panes, active)?);
                first = false;
            }
            (XmlEvent::Start(_), span) => {
                let end = reader.element_end()?;
                out.extend_from_slice(&xml.bytes()[span.0..end]);
            }
            (XmlEvent::Text(_), span) => out.extend_from_slice(&xml.bytes()[span.0..span.1]),
            (XmlEvent::End(_), (_, end)) => break end,
        }
    };
    if first {
        out.extend_from_slice(format!("<{prefix}sheetView workbookViewId=\"0\">").as_bytes());
        out.extend(children(&prefix, panes, active, None));
        out.extend_from_slice(format!("</{prefix}sheetView>").as_bytes());
    }
    out.extend_from_slice(format!("</{prefix}sheetViews>").as_bytes());
    Ok((end, out))
}

pub(super) fn rewrite(
    xml: &XmlPart,
    panes: Option<FrozenPanes>,
    active: (u32, u32),
) -> Result<Vec<u8>, SaveRefusal> {
    let mut reader = Reader::new(xml);
    let (_, root) = reader.root("worksheet")?;
    let tag = RawTag::parse(&xml.bytes()[root.0..root.1]);
    let prefix = String::from_utf8_lossy(tag.prefix());
    loop {
        match reader.expect()? {
            (XmlEvent::Start(child), span) if child.name.is(MAIN_NAMESPACE, "sheetViews") => {
                let (end, replacement) = views(&mut reader, xml, span, panes, active)?;
                let mut out = xml.bytes()[..span.0].to_vec();
                out.extend(replacement);
                out.extend_from_slice(&xml.bytes()[end..]);
                return Ok(out);
            }
            (XmlEvent::Start(child), span)
                if ["sheetFormatPr", "cols", "sheetData"]
                    .iter()
                    .any(|name| child.name.is(MAIN_NAMESPACE, name)) =>
            {
                let mut out = xml.bytes()[..span.0].to_vec();
                out.extend_from_slice(
                    format!("<{prefix}sheetViews><{prefix}sheetView workbookViewId=\"0\">")
                        .as_bytes(),
                );
                out.extend(children(&prefix, panes, active, None));
                out.extend_from_slice(
                    format!("</{prefix}sheetView></{prefix}sheetViews>").as_bytes(),
                );
                out.extend_from_slice(&xml.bytes()[span.0..]);
                return Ok(out);
            }
            (XmlEvent::Start(_), _) => {
                reader.element_end()?;
            }
            (XmlEvent::Text(_), _) => {}
            (XmlEvent::End(_), _) => return Err(reader.invalid("a worksheet has no sheetData")),
        }
    }
}
