//! Rewrite the first worksheet window; retain other windows and unknown XML attributes/children.
use super::{
    SaveRefusal, SaveViews,
    reader::Reader,
    text::{RawTag, Splice},
};
use crate::{
    SheetView,
    load::{MAIN_NAMESPACE, address::column_letters},
    xml::{XmlEvent, XmlPart},
};
use calc_doc_model::{ModelRoot, PaneLayout};
fn refusal(detail: impl Into<String>) -> SaveRefusal {
    SaveRefusal::WindowView {
        detail: detail.into(),
        owner: "dc-971p.16",
    }
}
fn address((r, c): (u32, u32)) -> String {
    format!("{}{r}", column_letters(c))
}
fn pane_name(p: u8) -> &'static str {
    match p {
        1 => "topRight",
        2 => "bottomLeft",
        3 => "bottomRight",
        _ => "topLeft",
    }
}
fn pane_id(p: Option<&str>) -> u8 {
    match p {
        Some("topRight") => 1,
        Some("bottomLeft") => 2,
        Some("bottomRight") => 3,
        _ => 0,
    }
}
fn mask(v: SheetView) -> u8 {
    if let Some(p) = v.frozen {
        u8::from(p.columns() > 0) | (u8::from(p.rows() > 0) << 1)
    } else if let Some((x, y)) = v.split {
        u8::from(f64::from_bits(x) > 0.) | (u8::from(f64::from_bits(y) > 0.) << 1)
    } else {
        0
    }
}
pub(super) fn validate(views: &SaveViews, model: &ModelRoot) -> Result<(), SaveRefusal> {
    if views.revision != model.revision().get() {
        return Err(refusal(
            "The workbook changed before its window state was captured",
        ));
    }
    if let Some(s) = views.active_sheet {
        if model
            .sheet_by_id(s)
            .is_none_or(|s| s.visibility() != calc_doc_model::SheetVisibility::Visible)
        {
            return Err(refusal(
                "The active saved worksheet is unavailable or hidden",
            ));
        }
    }
    for (&sheet, &v) in &views.sheets {
        if model.sheet_by_id(sheet).is_none() {
            return Err(refusal("A saved worksheet window no longer exists"));
        }
        for (r, c) in [v.top_left, v.pane_top_left, v.active_cell]
            .into_iter()
            .flatten()
        {
            if calc_doc_model::value::CellKey::new(sheet, r, c).is_err() {
                return Err(refusal(
                    "A saved window origin or active cell is outside the grid",
                ));
            }
        }
        if v.zoom.is_some_and(|z| !(10..=400).contains(&z))
            || v.active_pane > 3
            || v.active_pane & !mask(v) != 0
            || (v.frozen.is_some() && v.split.is_some())
            || v.split_window != v.split.is_some()
        {
            return Err(refusal("Invalid saved zoom or pane state"));
        }
        let shape = match (model.definitions().pane_layout(sheet), v.pane_layout()) {
            (Some(PaneLayout::Split(a)), Some(PaneLayout::Split(b))) => {
                a.positions() == b.positions()
            }
            (a, b) => a == b,
        };
        if !shape {
            return Err(refusal(
                "The worksheet pane layout changed before saving its view",
            ));
        }
    }
    Ok(())
}
struct Child {
    kind: u8,
    start: (usize, usize),
    end: usize,
    pane: u8,
    active: Option<(u32, u32)>,
    frozen_split: bool,
}
fn rewritten_child(xml: &XmlPart, c: &Child, attrs: &[(&str, Option<&str>)]) -> Vec<u8> {
    let raw = &xml.bytes()[c.start.0..c.start.1];
    let empty = raw.ends_with(b"/>");
    let mut out = RawTag::parse(raw).rebuilt(attrs, empty);
    out.extend_from_slice(&xml.bytes()[c.start.1..c.end]);
    out
}
fn view(
    reader: &mut Reader<'_>,
    xml: &XmlPart,
    span: (usize, usize),
    state: Option<SheetView>,
    selected: Option<bool>,
) -> Result<Vec<u8>, SaveRefusal> {
    let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
    let prefix = String::from_utf8_lossy(tag.prefix());
    let mut children = Vec::new();
    let end = loop {
        match reader.expect()? {
            (XmlEvent::Start(e), start) => {
                let end = reader.element_end()?;
                children.push(Child {
                    kind: if e.name.is(MAIN_NAMESPACE, "pane") {
                        1
                    } else if e.name.is(MAIN_NAMESPACE, "selection") {
                        2
                    } else {
                        0
                    },
                    start,
                    end,
                    pane: pane_id(e.attribute("pane")),
                    frozen_split: e.attribute("state") == Some("frozenSplit"),
                    active: e
                        .attribute("activeCell")
                        .and_then(crate::load::address::cell),
                });
            }
            (XmlEvent::Text(_), _) => {}
            (XmlEvent::End(_), (_, end)) => break end,
        }
    };
    let mut attrs = Vec::new();
    let selection = selected.map(|s| if s { "1" } else { "0" });
    if selected.is_some() {
        attrs.push(("tabSelected", selection));
    }
    let origin = state.map(|v| address(v.top_left.unwrap_or((1, 1))));
    let zoom = state.and_then(|v| v.zoom).map(|z| z.to_string());
    if state.is_some() {
        attrs.push(("topLeftCell", origin.as_deref()));
    }
    if zoom.is_some() {
        attrs.push(("zoomScale", zoom.as_deref()));
    }
    let mut out = tag.rebuilt(
        &attrs,
        state.is_none() && xml.bytes()[span.0..span.1].ends_with(b"/>"),
    );
    let Some(v) = state else {
        out.extend_from_slice(&xml.bytes()[span.1..end]);
        return Ok(out);
    };
    let available = mask(v);
    let active_pane = v.active_pane & available;
    if available != 0 {
        let (x, y, state) = if let Some(p) = v.frozen {
            (
                p.columns().to_string(),
                p.rows().to_string(),
                if children.iter().any(|c| c.kind == 1 && c.frozen_split) {
                    "frozenSplit"
                } else {
                    "frozen"
                },
            )
        } else {
            let (x, y) = v.split.unwrap();
            (
                f64::from_bits(x).to_string(),
                f64::from_bits(y).to_string(),
                "split",
            )
        };
        let origin = address(
            v.pane_top_left
                .unwrap_or_else(|| v.frozen.map_or((1, 1), |p| (p.rows() + 1, p.columns() + 1))),
        );
        let attrs = [
            ("xSplit", (available & 1 != 0).then_some(x.as_str())),
            ("ySplit", (available & 2 != 0).then_some(y.as_str())),
            ("state", Some(state)),
            ("topLeftCell", Some(origin.as_str())),
            ("activePane", Some(pane_name(active_pane))),
        ];
        if let Some(old) = children.iter().find(|c| c.kind == 1) {
            out.extend(rewritten_child(xml, old, &attrs));
        } else {
            let raw = format!("<{prefix}pane/>");
            out.extend(RawTag::parse(raw.as_bytes()).rebuilt(&attrs, true));
        }
    }
    let chosen = children
        .iter()
        .position(|c| c.kind == 2 && c.pane == active_pane)
        .or_else(|| children.iter().position(|c| c.kind == 2));
    let active = v.active_cell.unwrap_or((1, 1));
    let active_text = address(active);
    let mut wrote = false;
    for (i, c) in children.iter().enumerate().filter(|(_, c)| c.kind == 2) {
        if Some(i) == chosen {
            let mut attrs = vec![("pane", (available != 0).then_some(pane_name(active_pane)))];
            if c.active != Some(active) {
                attrs.extend([
                    ("activeCell", Some(active_text.as_str())),
                    ("sqref", Some(active_text.as_str())),
                    ("activeCellId", None),
                ]);
            }
            out.extend(rewritten_child(xml, c, &attrs));
            wrote = true;
        } else if available != 0 && c.pane & !available == 0 {
            out.extend_from_slice(&xml.bytes()[c.start.0..c.end]);
        }
    }
    if !wrote {
        out.extend_from_slice(
            format!(
                "<{prefix}selection{} activeCell=\"{active_text}\" sqref=\"{active_text}\"/>",
                if available != 0 {
                    format!(" pane=\"{}\"", pane_name(active_pane))
                } else {
                    String::new()
                }
            )
            .as_bytes(),
        );
    }
    for c in children.iter().filter(|c| c.kind == 0) {
        out.extend_from_slice(&xml.bytes()[c.start.0..c.end]);
    }
    out.extend_from_slice(format!("</{prefix}sheetView>").as_bytes());
    Ok(out)
}
fn views(
    reader: &mut Reader<'_>,
    xml: &XmlPart,
    span: (usize, usize),
    state: Option<SheetView>,
    selected: Option<bool>,
) -> Result<(usize, Vec<u8>), SaveRefusal> {
    let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
    let prefix = String::from_utf8_lossy(tag.prefix());
    let mut out = tag.rebuilt(&[], false);
    let mut first = true;
    let end = loop {
        match reader.expect()? {
            (XmlEvent::Start(e), span) if first && e.name.is(MAIN_NAMESPACE, "sheetView") => {
                out.extend(view(reader, xml, span, state, selected)?);
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
        let stub = XmlPart::new(
            "window.xml".into(),
            format!("<sheetView xmlns=\"{MAIN_NAMESPACE}\" workbookViewId=\"0\"/>").into_bytes(),
            Default::default(),
        );
        let mut r = Reader::new(&stub);
        let (_, span) = r.root("sheetView")?;
        let bytes = view(&mut r, &stub, span, state, selected)?;
        out.extend_from_slice(&bytes);
    }
    out.extend_from_slice(format!("</{prefix}sheetViews>").as_bytes());
    Ok((end, out))
}
pub(super) fn rewrite(
    xml: &XmlPart,
    state: Option<SheetView>,
    selected: Option<bool>,
) -> Result<Vec<u8>, SaveRefusal> {
    let mut r = Reader::new(xml);
    let (_, root) = r.root("worksheet")?;
    let tag = RawTag::parse(&xml.bytes()[root.0..root.1]);
    let prefix = String::from_utf8_lossy(tag.prefix());
    loop {
        match r.expect()? {
            (XmlEvent::Start(e), span) if e.name.is(MAIN_NAMESPACE, "sheetViews") => {
                let (end, replacement) = views(&mut r, xml, span, state, selected)?;
                let mut out = xml.bytes()[..span.0].to_vec();
                out.extend(replacement);
                out.extend_from_slice(&xml.bytes()[end..]);
                return Ok(out);
            }
            (XmlEvent::Start(e), span)
                if ["sheetFormatPr", "cols", "sheetData"]
                    .iter()
                    .any(|n| e.name.is(MAIN_NAMESPACE, n)) =>
            {
                let stub = XmlPart::new(
                    "window.xml".into(),
                    format!(
                        "<{prefix}sheetViews xmlns=\"{MAIN_NAMESPACE}\"{}></{prefix}sheetViews>",
                        if prefix.is_empty() {
                            String::new()
                        } else {
                            format!(
                                " xmlns:{}=\"{MAIN_NAMESPACE}\"",
                                prefix.trim_end_matches(':')
                            )
                        }
                    )
                    .into_bytes(),
                    Default::default(),
                );
                let mut reader = Reader::new(&stub);
                let (_, start) = reader.root("sheetViews")?;
                let (_, replacement) = views(&mut reader, &stub, start, state, selected)?;
                let mut out = xml.bytes()[..span.0].to_vec();
                out.extend(replacement);
                out.extend_from_slice(&xml.bytes()[span.0..]);
                return Ok(out);
            }
            (XmlEvent::Start(_), _) => {
                r.element_end()?;
            }
            (XmlEvent::Text(_), _) => {}
            (XmlEvent::End(_), _) => return Err(refusal("A worksheet has no sheetData")),
        }
    }
}
pub(super) fn workbook(xml: &XmlPart, index: usize) -> Result<Vec<u8>, SaveRefusal> {
    let mut r = Reader::new(xml);
    let (_, root) = r.root("workbook")?;
    let tag = RawTag::parse(&xml.bytes()[root.0..root.1]);
    let prefix = String::from_utf8_lossy(tag.prefix());
    let mut splice = Splice::new(xml.bytes());
    let index = index.to_string();
    loop {
        match r.expect()? {
            (XmlEvent::Start(e), span) if e.name.is(MAIN_NAMESPACE, "bookViews") => {
                let raw = &xml.bytes()[span.0..span.1];
                let tag = RawTag::parse(raw);
                let vp = String::from_utf8_lossy(tag.prefix());
                let mut replacement = tag.rebuilt(&[], false);
                let mut first = true;
                loop {
                    match r.expect()? {
                        (XmlEvent::Start(e), start) => {
                            let end = r.element_end()?;
                            if first && e.name.is(MAIN_NAMESPACE, "workbookView") {
                                let raw = &xml.bytes()[start.0..start.1];
                                replacement.extend(RawTag::parse(raw).rebuilt(
                                    &[("activeTab", Some(index.as_str()))],
                                    raw.ends_with(b"/>"),
                                ));
                                replacement.extend_from_slice(&xml.bytes()[start.1..end]);
                                first = false;
                            } else {
                                replacement.extend_from_slice(&xml.bytes()[start.0..end]);
                            }
                        }
                        (XmlEvent::Text(_), s) => {
                            replacement.extend_from_slice(&xml.bytes()[s.0..s.1])
                        }
                        (XmlEvent::End(_), (_, end)) => {
                            if first {
                                replacement.extend_from_slice(
                                    format!("<{vp}workbookView activeTab=\"{index}\"/>").as_bytes(),
                                );
                            }
                            replacement.extend_from_slice(format!("</{vp}bookViews>").as_bytes());
                            splice.replace((span.0, end), &replacement);
                            return Ok(splice.finish());
                        }
                    }
                }
            }
            (XmlEvent::Start(e), span) if e.name.is(MAIN_NAMESPACE, "sheets") => {
                splice.replace((span.0,span.0),format!("<{prefix}bookViews><{prefix}workbookView activeTab=\"{index}\"/></{prefix}bookViews>").as_bytes());
                return Ok(splice.finish());
            }
            (XmlEvent::Start(_), _) => {
                r.element_end()?;
            }
            (XmlEvent::Text(_), _) => {}
            (XmlEvent::End(_), _) => return Err(refusal("A workbook has no sheets")),
        }
    }
}
