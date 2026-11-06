//! Preserve shape/line attributes and unrelated DrawingML while replacing an authored colour.
use super::*;

pub(super) fn fill(color: Option<&str>) -> Vec<u8> {
    color
        .map(|c| {
            format!("<a:solidFill xmlns:a=\"{A}\"><a:srgbClr val=\"{c}\"/></a:solidFill>")
                .into_bytes()
        })
        .unwrap_or_default()
}

pub(super) fn fresh(color: Option<&str>) -> Vec<u8> {
    let Some(color) = color else {
        return Vec::new();
    };
    let f = String::from_utf8(fill(Some(color))).expect("RGB XML");
    format!("<c:spPr xmlns:c=\"{C}\" xmlns:a=\"{A}\">{f}<a:ln>{f}</a:ln></c:spPr>").into_bytes()
}

// `outline` distinguishes a:ln from c:spPr. Only fills are replaced; pen width, dash,
// joins, effects, transforms, extensions and the source's namespace spelling stay intact.
pub(super) fn replace(
    xml: &XmlPart,
    reader: &mut Reader<'_>,
    span: (usize, usize),
    color: Option<&str>,
    outline: bool,
) -> Result<(usize, Vec<u8>), SaveRefusal> {
    let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
    let mut children = Vec::new();
    let mut patches = Vec::new();
    let mut line = false;
    let (close, end) = loop {
        match reader.expect()? {
            (XmlEvent::Start(e), s) => {
                let end = if !outline && e.name.is(A, "ln") {
                    if line {
                        return Err(reader.invalid("Duplicate series outline records (dc-971p.58)"));
                    }
                    line = true;
                    let (end, bytes) = replace(xml, reader, s, color, true)?;
                    patches.push(((s.0, end), bytes));
                    end
                } else {
                    reader.element_end()?
                };
                children.push((e.name, (s.0, end)));
            }
            (XmlEvent::End(_), s) => break s,
            _ => {}
        }
    };
    let fill_kind = |n: &crate::xml::XmlName| {
        n.namespace.as_deref() == Some(A)
            && matches!(
                n.local.as_str(),
                "solidFill" | "noFill" | "gradFill" | "blipFill" | "pattFill" | "grpFill"
            )
    };
    let fills = children
        .iter()
        .filter(|(n, _)| fill_kind(n))
        .collect::<Vec<_>>();
    // A deliberately hidden outline stays hidden when its series colour changes.
    if outline && fills.iter().any(|(n, _)| n.is(A, "noFill")) {
        return Ok((end, xml.bytes()[span.0..end].to_vec()));
    }
    if fills.len() > 1 {
        return Err(reader.invalid("Duplicate series fill records (dc-971p.58)"));
    }
    let fill_at = children
        .iter()
        .find(|(n, _)| {
            outline
                || n.namespace.as_deref() != Some(A)
                || !matches!(n.local.as_str(), "xfrm" | "custGeom" | "prstGeom")
        })
        .map_or(close, |(_, s)| s.0);
    let line_at = children
        .iter()
        .find(|(n, _)| {
            n.namespace.as_deref() != Some(A)
                || (!matches!(n.local.as_str(), "xfrm" | "custGeom" | "prstGeom") && !fill_kind(n))
        })
        .map_or(close, |(_, s)| s.0);
    let mut inserts = std::collections::BTreeMap::<usize, Vec<u8>>::new();
    if let Some((_, s)) = fills.first() {
        patches.push((*s, fill(color)));
    } else {
        inserts.entry(fill_at).or_default().extend(fill(color));
    }
    if !outline && !line && color.is_some() {
        let bytes = inserts.entry(line_at).or_default();
        bytes.extend(format!("<a:ln xmlns:a=\"{A}\">").as_bytes());
        bytes.extend(fill(color));
        bytes.extend(b"</a:ln>");
    }
    if tag.is_empty() {
        let mut bytes = tag.rebuilt(&[], false);
        for inserted in inserts.values() {
            bytes.extend(inserted);
        }
        bytes.extend(b"</");
        bytes.extend(tag.qualified_name());
        bytes.extend(b">");
        return Ok((end, bytes));
    }
    for (at, bytes) in inserts {
        if !bytes.is_empty() {
            patches.push(((at, at), bytes));
        }
    }
    patches.sort_by_key(|(s, _)| *s);
    let mut splice = Splice::new(&xml.bytes()[span.0..end]);
    for ((a, b), bytes) in patches {
        splice.replace((a - span.0, b - span.0), &bytes);
    }
    Ok((end, splice.finish()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn replaces_fills_and_keeps_shape_outline_and_extension_records() {
        let xml=XmlPart::new("chart.xml".into(),format!("<c:spPr xmlns:c=\"{C}\" xmlns:a=\"{A}\" custom=\"retained\"><a:xfrm rot=\"12\"/><a:gradFill><a:gsLst/></a:gradFill><a:ln w=\"19050\"><a:solidFill><a:schemeClr val=\"accent3\"/></a:solidFill><a:prstDash val=\"dash\"/><a:round/></a:ln><a:effectLst/><a:extLst><a:ext uri=\"keep\"/></a:extLst></c:spPr>").into_bytes(),crate::xml::XmlLimits::default());
        let mut r = Reader::new(&xml);
        let (_, span) = r.expect().unwrap();
        let (_, bytes) = replace(&xml, &mut r, span, Some("123ABC"), false).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        for retained in [
            "custom=\"retained\"",
            "<a:xfrm rot=\"12\"/>",
            "<a:ln w=\"19050\">",
            "<a:prstDash val=\"dash\"/>",
            "<a:round/>",
            "<a:effectLst/>",
            "<a:extLst><a:ext uri=\"keep\"/></a:extLst>",
        ] {
            assert!(text.contains(retained), "{text}");
        }
        assert!(!text.contains("gradFill"));
        assert!(!text.contains("noFill"));
        assert_eq!(text.matches("123ABC").count(), 2);
        let xml = xml.with_bytes(text.into_bytes());
        let mut r = Reader::new(&xml);
        let (_, span) = r.expect().unwrap();
        let (_, bytes) = replace(&xml, &mut r, span, None, false).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("solidFill"));
        assert!(text.contains("<a:prstDash val=\"dash\"/>"));
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn missing_outlines_follow_fill_and_empty_source_tags_expand() {
        let hidden=XmlPart::new("hidden.xml".into(),format!("<c:spPr xmlns:c=\"{C}\" xmlns:a=\"{A}\"><a:ln w=\"19050\"><a:noFill/></a:ln></c:spPr>").into_bytes(),crate::xml::XmlLimits::default());
        let mut r = Reader::new(&hidden);
        let (_, span) = r.expect().unwrap();
        let (_, bytes) = replace(&hidden, &mut r, span, Some("123ABC"), false).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("<a:ln w=\"19050\"><a:noFill/></a:ln>"));
        assert_eq!(text.matches("123ABC").count(), 1);
        for inner in [
            "",
            "<a:solidFill><a:schemeClr val=\"accent2\"/></a:solidFill>",
        ] {
            let source =
                format!("<c:spPr xmlns:c=\"{C}\" xmlns:a=\"{A}\">{inner}<a:effectLst/></c:spPr>");
            let xml = XmlPart::new(
                "chart.xml".into(),
                source.into_bytes(),
                crate::xml::XmlLimits::default(),
            );
            let mut r = Reader::new(&xml);
            let (_, span) = r.expect().unwrap();
            let (_, bytes) = replace(&xml, &mut r, span, Some("123ABC"), false).unwrap();
            let text = String::from_utf8(bytes).unwrap();
            assert!(text.find("solidFill").unwrap() < text.find("<a:ln").unwrap());
            assert!(
                text.find("<a:ln").unwrap() < text.find("effectLst").unwrap(),
                "{text}"
            );
        }
        let xml = XmlPart::new(
            "chart.xml".into(),
            format!("<c:spPr xmlns:c=\"{C}\"/>").into_bytes(),
            crate::xml::XmlLimits::default(),
        );
        let mut r = Reader::new(&xml);
        let (_, span) = r.expect().unwrap();
        let (_, bytes) = replace(&xml, &mut r, span, Some("123ABC"), false).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(text.matches("123ABC").count(), 2);
        assert!(text.ends_with("</c:spPr>"));
    }
}
