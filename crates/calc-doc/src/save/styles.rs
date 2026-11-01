//! Append authored styles while retaining source records, including fields this codec
//! does not interpret. A derived record copies its source and changes only edited fields.
use super::{SaveRefusal, reader::Reader, text::RawTag};
use crate::{
    load::MAIN_NAMESPACE,
    xml::{XmlElement, XmlEvent, XmlPart},
};
use calc_doc_model::{Alignment, Border, CellFormat, Fill, Font, StyleColor, StyleTable};

pub(super) const RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
pub(super) const CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml";

#[derive(Clone)]
struct Node {
    local: String,
    main: bool,
    raw: Vec<u8>,
    tag_end: usize,
    close_start: usize,
    children: Vec<(usize, usize, Option<Node>)>,
    added: Vec<Node>,
    attrs: Vec<(String, Option<String>)>,
}
impl Node {
    fn read(
        xml: &XmlPart,
        reader: &mut Reader<'_>,
        e: XmlElement,
        span: (usize, usize),
    ) -> Result<Self, SaveRefusal> {
        let mut children = Vec::new();
        let (close_start, end) = loop {
            match reader.expect()? {
                (XmlEvent::Start(child), range) => {
                    let node = Self::read(xml, reader, child, range)?;
                    children.push((
                        range.0 - span.0,
                        range.0 - span.0 + node.raw.len(),
                        Some(node),
                    ));
                }
                (XmlEvent::End(_), range) => break (range.0, range.1),
                _ => {}
            }
        };
        Ok(Self {
            main: e.name.is(MAIN_NAMESPACE, &e.name.local),
            local: e.name.local.to_string(),
            raw: xml.bytes()[span.0..end].to_vec(),
            tag_end: span.1 - span.0,
            close_start: close_start - span.0,
            children,
            added: Vec::new(),
            attrs: Vec::new(),
        })
    }
    fn new(prefix: &str, local: &str) -> Self {
        let raw = format!("<{prefix}{local}/>").into_bytes();
        let len = raw.len();
        Self {
            local: local.into(),
            main: true,
            raw,
            tag_end: len,
            close_start: len,
            children: Vec::new(),
            added: Vec::new(),
            attrs: Vec::new(),
        }
    }
    fn prefix(&self) -> String {
        String::from_utf8_lossy(RawTag::parse(&self.raw[..self.tag_end]).prefix()).into_owned()
    }
    fn attr(&mut self, key: &str, value: Option<String>) {
        if let Some(entry) = self.attrs.iter_mut().find(|a| a.0 == key) {
            entry.1 = value;
        } else {
            self.attrs.push((key.into(), value));
        }
    }
    fn child(&self, local: &str) -> Option<&Node> {
        self.children
            .iter()
            .filter_map(|c| c.2.as_ref())
            .chain(&self.added)
            .find(|c| c.main && c.local == local)
    }
    fn set_child(&mut self, local: &str, value: Option<Node>) {
        if let Some(child) = self
            .children
            .iter_mut()
            .find(|c| c.2.as_ref().is_some_and(|n| n.main && n.local == local))
        {
            child.2 = value;
        } else if let Some(index) = self.added.iter().position(|n| n.main && n.local == local) {
            if let Some(value) = value {
                self.added[index] = value;
            } else {
                self.added.remove(index);
            }
        } else if let Some(value) = value {
            // New fields precede their existing schema successors; raw siblings keep
            // their source order and bytes. XF and border children are sequences.
            let order: &[&str] = match self.local.as_str() {
                "xf" => &["alignment", "protection", "extLst"],
                "border" => &[
                    "start",
                    "end",
                    "left",
                    "right",
                    "top",
                    "bottom",
                    "diagonal",
                    "vertical",
                    "horizontal",
                    "extLst",
                ],
                _ => &[],
            };
            if let Some(rank) = order.iter().position(|name| *name == local) {
                let at = self
                    .children
                    .iter()
                    .find(|(_, _, node)| {
                        node.as_ref().is_some_and(|n| {
                            n.main
                                && order
                                    .iter()
                                    .position(|name| *name == n.local)
                                    .is_some_and(|r| r > rank)
                        })
                    })
                    .map_or(self.close_start, |c| c.0);
                self.children.push((at, at, Some(value)));
                self.children.sort_by_key(|c| (c.0, c.1));
            } else {
                self.added.push(value);
            }
        }
    }
    fn records(&self, local: &str) -> Vec<Node> {
        self.children
            .iter()
            .filter_map(|c| c.2.clone())
            .chain(self.added.clone())
            .filter(|c| c.main && c.local == local)
            .collect()
    }
    fn render(&self) -> Vec<u8> {
        let tag = RawTag::parse(&self.raw[..self.tag_end]);
        let attrs: Vec<_> = self
            .attrs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_deref()))
            .collect();
        let added: Vec<_> = self.added.iter().flat_map(Node::render).collect();
        if tag.is_empty() && (!added.is_empty() || self.children.iter().any(|c| c.2.is_some())) {
            let mut out = tag.rebuilt(&attrs, false);
            for (_, _, child) in &self.children {
                if let Some(child) = child {
                    out.extend(child.render());
                }
            }
            out.extend(added);
            out.extend(format!("</{}{}>", self.prefix(), self.local).bytes());
            return out;
        }
        let mut edits = Vec::new();
        if !attrs.is_empty() {
            edits.push((0, self.tag_end, tag.rebuilt(&attrs, tag.is_empty())));
        }
        for (start, end, node) in &self.children {
            let bytes = node.as_ref().map(Node::render).unwrap_or_default();
            if bytes != self.raw[*start..*end] {
                edits.push((*start, *end, bytes));
            }
        }
        if !added.is_empty() {
            let at = self
                .children
                .iter()
                .find(|c| c.2.as_ref().is_some_and(|n| n.main && n.local == "extLst"))
                .map_or(self.close_start, |c| c.0);
            edits.push((at, at, added));
        }
        edits.sort_by_key(|e| (e.0, e.1));
        let mut out = Vec::new();
        let mut cursor = 0;
        for (start, end, bytes) in edits {
            out.extend_from_slice(&self.raw[cursor..start]);
            out.extend(bytes);
            cursor = end;
        }
        out.extend_from_slice(&self.raw[cursor..]);
        out
    }
}
fn value_child(node: &mut Node, key: &str, before: &Option<String>, after: &Option<String>) {
    if before != after {
        let value = after.as_ref().map(|v| {
            let mut child = node
                .child(key)
                .cloned()
                .unwrap_or_else(|| Node::new(&node.prefix(), key));
            child.attr("val", Some(v.clone()));
            child
        });
        node.set_child(key, value);
    }
}
fn boolean_child(node: &mut Node, key: &str, before: bool, after: bool) {
    if before != after {
        let value = if let Some(mut child) = node.child(key).cloned() {
            child.attr("val", Some(if after { "1" } else { "0" }.into()));
            Some(child)
        } else {
            after.then(|| Node::new(&node.prefix(), key))
        };
        node.set_child(key, value);
    }
}
fn colour(mut node: Node, c: &StyleColor) -> Node {
    for (k, v) in [
        ("rgb", &c.rgb),
        ("indexed", &c.indexed),
        ("theme", &c.theme),
        ("tint", &c.tint),
    ] {
        node.attr(k, v.clone());
    }
    node.attr("auto", c.auto.then(|| "1".into()));
    node
}
fn color_child(
    node: &mut Node,
    key: &str,
    before: &Option<StyleColor>,
    after: &Option<StyleColor>,
) {
    if before != after {
        let value = after.as_ref().map(|c| {
            let child = node
                .child(key)
                .cloned()
                .unwrap_or_else(|| Node::new(&node.prefix(), key));
            colour(child, c)
        });
        node.set_child(key, value);
    }
}
fn font(mut node: Node, before: &Font, after: &Font) -> Node {
    for (k, a, b) in [
        ("name", &before.name, &after.name),
        ("sz", &before.size, &after.size),
        ("family", &before.family, &after.family),
        ("scheme", &before.scheme, &after.scheme),
        ("u", &before.underline, &after.underline),
    ] {
        value_child(&mut node, k, a, b);
    }
    for (k, a, b) in [
        ("b", before.bold, after.bold),
        ("i", before.italic, after.italic),
        ("strike", before.strike, after.strike),
    ] {
        boolean_child(&mut node, k, a, b);
    }
    color_child(&mut node, "color", &before.color, &after.color);
    node
}
fn fill(mut node: Node, before: &Fill, after: &Fill) -> Node {
    if before != after {
        node.set_child("gradientFill", None);
        let mut pattern = node
            .child("patternFill")
            .cloned()
            .unwrap_or_else(|| Node::new(&node.prefix(), "patternFill"));
        if before.pattern_type != after.pattern_type {
            pattern.attr("patternType", after.pattern_type.clone());
        }
        color_child(&mut pattern, "fgColor", &before.fg_color, &after.fg_color);
        color_child(&mut pattern, "bgColor", &before.bg_color, &after.bg_color);
        node.set_child("patternFill", Some(pattern));
    }
    node
}
fn border(mut node: Node, before: &Border, after: &Border) -> Node {
    for (k, a, b) in [
        ("left", &before.left, &after.left),
        ("right", &before.right, &after.right),
        ("top", &before.top, &after.top),
        ("bottom", &before.bottom, &after.bottom),
        ("diagonal", &before.diagonal, &after.diagonal),
    ] {
        if a == b {
            continue;
        }
        let mut edge = node
            .child(k)
            .cloned()
            .unwrap_or_else(|| Node::new(&node.prefix(), k));
        let old = a.clone().unwrap_or_default();
        let new = b.clone().unwrap_or_default();
        if old.style != new.style {
            edge.attr("style", new.style.clone());
        }
        color_child(&mut edge, "color", &old.color, &new.color);
        node.set_child(k, Some(edge));
    }
    for (k, a, b) in [
        ("diagonalUp", before.diagonal_up, after.diagonal_up),
        ("diagonalDown", before.diagonal_down, after.diagonal_down),
    ] {
        if a != b {
            node.attr(k, b.then(|| "1".into()));
        }
    }
    node
}
fn alignment(mut node: Node, before: &Alignment, after: &Alignment) -> Node {
    for (k, a, b) in [
        ("horizontal", &before.horizontal, &after.horizontal),
        ("vertical", &before.vertical, &after.vertical),
        ("textRotation", &before.text_rotation, &after.text_rotation),
        ("indent", &before.indent, &after.indent),
    ] {
        if a != b {
            node.attr(k, b.clone());
        }
    }
    for (k, a, b) in [
        ("wrapText", before.wrap_text, after.wrap_text),
        ("shrinkToFit", before.shrink_to_fit, after.shrink_to_fit),
    ] {
        if a != b {
            node.attr(k, Some(if b { "1" } else { "0" }.into()));
        }
    }
    node
}
fn xf(mut node: Node, before: &CellFormat, after: &CellFormat) -> Node {
    for (k, a, b) in [
        ("numFmtId", before.number_format_id, after.number_format_id),
        ("fontId", before.font_id, after.font_id),
        ("fillId", before.fill_id, after.fill_id),
        ("borderId", before.border_id, after.border_id),
        ("xfId", before.xf_id, after.xf_id),
    ] {
        if a != b {
            node.attr(k, b.map(|v| v.to_string()));
        }
    }
    for (k, a, b) in [
        (
            "applyNumberFormat",
            before.apply_number_format,
            after.apply_number_format,
        ),
        ("applyFont", before.apply_font, after.apply_font),
        ("applyFill", before.apply_fill, after.apply_fill),
        ("applyBorder", before.apply_border, after.apply_border),
        (
            "applyAlignment",
            before.apply_alignment,
            after.apply_alignment,
        ),
        (
            "applyProtection",
            before.apply_protection,
            after.apply_protection,
        ),
        ("quotePrefix", before.quote_prefix, after.quote_prefix),
        ("pivotButton", before.pivot_button, after.pivot_button),
    ] {
        if a != b {
            node.attr(k, Some(if b { "1" } else { "0" }.into()));
        }
    }
    if before.alignment != after.alignment {
        let child = node
            .child("alignment")
            .cloned()
            .unwrap_or_else(|| Node::new(&node.prefix(), "alignment"));
        node.set_child(
            "alignment",
            after
                .alignment
                .as_ref()
                .map(|a| alignment(child, &before.alignment.clone().unwrap_or_default(), a)),
        );
    }
    node
}

fn append<T: Default + PartialEq>(
    root: &mut Node,
    local: &str,
    item: &str,
    before: &[T],
    after: &[T],
    sources: &std::collections::BTreeMap<u32, u32>,
    write: impl Fn(Node, &T, &T) -> Node,
) -> Result<(), String> {
    if !after.starts_with(before) {
        return Err(format!("Imported {local} records cannot be replaced"));
    }
    if before.len() == after.len() {
        return Ok(());
    }
    let mut collection = root
        .child(local)
        .cloned()
        .unwrap_or_else(|| Node::new(&root.prefix(), local));
    let mut nodes = collection.records(item);
    if nodes.len() != before.len() {
        return Err(format!(
            "The source {local} count differs from its parsed table"
        ));
    }
    for index in before.len()..after.len() {
        let source = sources.get(&(index as u32)).copied().map(|n| n as usize);
        if source.is_some_and(|n| n >= index) {
            return Err("A style derivation must precede its copy".into());
        }
        let default = T::default();
        let parent = source.map_or(&default, |n| &after[n]);
        let node = source
            .and_then(|n| nodes.get(n))
            .cloned()
            .unwrap_or_else(|| Node::new(&collection.prefix(), item));
        let node = write(node, parent, &after[index]);
        nodes.push(node.clone());
        collection.added.push(node);
    }
    collection.attr("count", Some(after.len().to_string()));
    root.set_child(local, Some(collection));
    Ok(())
}

pub(super) fn rewrite(
    xml: &XmlPart,
    before: &StyleTable,
    after: &StyleTable,
) -> Result<Vec<u8>, SaveRefusal> {
    let mut reader = Reader::new(xml);
    let (element, span) = reader.root("styleSheet")?;
    let mut root = Node::read(xml, &mut reader, element, span)?;
    reader.end()?;
    let error = |detail: String| SaveRefusal::InvalidPart {
        part: xml.name().into(),
        detail,
    };
    let empty = Default::default();
    append(
        &mut root,
        "fonts",
        "font",
        &before.fonts,
        &after.fonts,
        &after.sources.fonts,
        font,
    )
    .map_err(error)?;
    append(
        &mut root,
        "fills",
        "fill",
        &before.fills,
        &after.fills,
        &after.sources.fills,
        fill,
    )
    .map_err(error)?;
    append(
        &mut root,
        "borders",
        "border",
        &before.borders,
        &after.borders,
        &after.sources.borders,
        border,
    )
    .map_err(error)?;
    append(
        &mut root,
        "cellStyleXfs",
        "xf",
        &before.cell_style_formats,
        &after.cell_style_formats,
        &empty,
        xf,
    )
    .map_err(error)?;
    let parents = root
        .child("cellStyleXfs")
        .map(|c| c.records("xf"))
        .unwrap_or_default();
    append(
        &mut root,
        "cellXfs",
        "xf",
        &before.cell_formats,
        &after.cell_formats,
        &after.sources.cell_formats,
        |mut node, before, after| {
            if before.alignment.is_none() && after.alignment.is_some() {
                if let Some(alignment) = before
                    .xf_id
                    .and_then(|id| parents.get(id as usize))
                    .and_then(|p| p.child("alignment"))
                {
                    node.set_child("alignment", Some(alignment.clone()));
                }
            }
            xf(node, before, after)
        },
    )
    .map_err(error)?;
    if !after.number_formats.starts_with(&before.number_formats) {
        return Err(error("Imported number formats cannot be replaced".into()));
    }
    if after.number_formats.len() > before.number_formats.len() {
        let mut collection = root
            .child("numFmts")
            .cloned()
            .unwrap_or_else(|| Node::new(&root.prefix(), "numFmts"));
        for format in &after.number_formats[before.number_formats.len()..] {
            let mut n = Node::new(&collection.prefix(), "numFmt");
            n.attr("numFmtId", Some(format.id.to_string()));
            n.attr("formatCode", Some(format.code.clone()));
            collection.added.push(n);
        }
        collection.attr("count", Some(after.number_formats.len().to_string()));
        root.set_child("numFmts", Some(collection));
    }
    // Missing collections must be inserted in the schema's order, before existing successors.
    const ORDER: &[&str] = &[
        "numFmts",
        "fonts",
        "fills",
        "borders",
        "cellStyleXfs",
        "cellXfs",
        "cellStyles",
        "dxfs",
        "tableStyles",
        "colors",
        "extLst",
    ];
    let mut added_nodes = std::mem::take(&mut root.added);
    added_nodes.sort_by_key(|n| {
        ORDER
            .iter()
            .position(|s| *s == n.local)
            .unwrap_or(ORDER.len())
    });
    for added in added_nodes {
        let rank = ORDER
            .iter()
            .position(|n| *n == added.local)
            .unwrap_or(ORDER.len());
        let at = root
            .children
            .iter()
            .find(|(_, _, n)| {
                n.as_ref().is_some_and(|n| {
                    n.main
                        && ORDER
                            .iter()
                            .position(|s| *s == n.local)
                            .is_some_and(|r| r > rank)
                })
            })
            .map_or(root.close_start, |n| n.0);
        root.children.push((at, at, Some(added)));
    }
    root.children.sort_by_key(|c| (c.0, c.1));
    let mut bytes = xml.bytes()[..span.0].to_vec();
    bytes.extend(root.render());
    bytes.extend_from_slice(&xml.bytes()[span.0 + root.raw.len()..]);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn derived_styles_keep_unread_fields_and_original_records() {
        let source = format!(
            r#"<s:styleSheet xmlns:s="{MAIN_NAMESPACE}" xmlns:v="urn:vendor"><s:fonts count="1"><s:font v:keep="font"><s:name val="Example" v:keep="name"/><s:b val="0" v:keep="bold"/><s:color indexed="8" v:keep="color"/><s:vAlign val="superscript"/></s:font></s:fonts><s:cellXfs count="1"><s:xf fontId="0" v:keep="xf"><s:alignment horizontal="left" readingOrder="2"/><s:extLst><v:keep/></s:extLst></s:xf></s:cellXfs><s:extLst><v:root/></s:extLst></s:styleSheet>"#
        );
        let before = StyleTable {
            fonts: vec![Font {
                name: Some("Example".into()),
                color: Some(StyleColor {
                    indexed: Some("8".into()),
                    ..Default::default()
                }),
                other_children: vec!["vAlign".into()],
                ..Default::default()
            }],
            cell_formats: vec![CellFormat {
                font_id: Some(0),
                alignment: Some(Alignment {
                    horizontal: Some("left".into()),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut after = before.clone();
        let mut f = before.fonts[0].clone();
        f.name = Some("Changed".into());
        f.bold = true;
        f.color = Some(StyleColor {
            rgb: Some("FF123456".into()),
            ..Default::default()
        });
        after.fonts.push(f.clone());
        after.sources.fonts.insert(1, 0);
        f.bold = false;
        after.fonts.push(f);
        after.sources.fonts.insert(2, 1);
        let mut x = before.cell_formats[0].clone();
        x.font_id = Some(1);
        x.apply_font = true;
        x.alignment.as_mut().unwrap().wrap_text = true;
        after.cell_formats.push(x);
        after.sources.cell_formats.insert(1, 0);
        let xml = XmlPart::new(
            "xl/styles.xml".into(),
            source.as_bytes().to_vec(),
            Default::default(),
        );
        let bytes = rewrite(&xml, &before, &after).unwrap();
        let written = String::from_utf8(bytes.clone()).unwrap();
        assert!(written.contains(
            r#"<s:font v:keep="font"><s:name val="Example" v:keep="name"/><s:b val="0" v:keep="bold"/><s:color indexed="8" v:keep="color"/><s:vAlign val="superscript"/></s:font>"#
        ));
        assert!(written.contains(r#"<s:xf fontId="0" v:keep="xf"><s:alignment horizontal="left" readingOrder="2"/><s:extLst><v:keep/></s:extLst></s:xf>"#));
        assert_eq!(written.matches("superscript").count(), 3);
        for field in ["name", "bold", "color"] {
            assert_eq!(
                written.matches(&format!("v:keep=\"{field}\"")).count(),
                3,
                "{written}"
            );
        }
        assert_eq!(written.matches("val=\"Changed\"").count(), 2);
        assert_eq!(written.matches("rgb=\"FF123456\"").count(), 2);
        assert_eq!(written.matches("indexed=\"8\"").count(), 1);
        assert_eq!(
            written.matches(r#"<s:b val="0" v:keep="bold"/>"#).count(),
            2
        );
        assert_eq!(
            written.matches(r#"<s:b val="1" v:keep="bold"/>"#).count(),
            1
        );
        assert_eq!(written.matches("readingOrder=\"2\"").count(), 2);
        assert_eq!(written.matches("<v:keep/>").count(), 2);
        assert!(written.contains("wrapText=\"1\""));
        assert!(written.contains("fontId=\"1\""));
        let parsed = XmlPart::new("xl/styles.xml".into(), bytes, Default::default());
        let mut events = parsed.reader();
        while events.next_event().unwrap().is_some() {}
        let mut invalid = after;
        invalid.fonts[0].name = Some("Replaced".into());
        assert!(rewrite(&xml, &before, &invalid).is_err());
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn derived_alignment_and_border_sides_precede_existing_schema_successors() {
        let source = format!(
            r#"<styleSheet xmlns="{MAIN_NAMESPACE}" xmlns:v="urn:vendor"><borders count="1"><border><diagonal style="thin"/><vertical/><horizontal/></border></borders><cellXfs count="1"><xf borderId="0"><protection locked="0" hidden="1" v:keep="yes"/><extLst><v:keep/></extLst></xf></cellXfs></styleSheet>"#
        );
        let before = StyleTable {
            borders: vec![Border {
                diagonal: Some(calc_doc_model::BorderEdge {
                    style: Some("thin".into()),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            cell_formats: vec![CellFormat {
                border_id: Some(0),
                protection: Some(calc_doc_model::Protection {
                    locked: Some(false),
                    hidden: Some(true),
                }),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut after = before.clone();
        let mut border = before.borders[0].clone();
        border.left = Some(calc_doc_model::BorderEdge {
            style: Some("double".into()),
            ..Default::default()
        });
        border.top = Some(calc_doc_model::BorderEdge {
            style: Some("thick".into()),
            ..Default::default()
        });
        after.borders.push(border);
        after.sources.borders.insert(1, 0);
        let mut xf = before.cell_formats[0].clone();
        xf.border_id = Some(1);
        xf.alignment = Some(Alignment {
            wrap_text: true,
            ..Default::default()
        });
        after.cell_formats.push(xf);
        after.sources.cell_formats.insert(1, 0);
        let xml = XmlPart::new(
            "xl/styles.xml".into(),
            source.into_bytes(),
            Default::default(),
        );
        let bytes = rewrite(&xml, &before, &after).unwrap();
        let written = String::from_utf8(bytes.clone()).unwrap();
        assert!(written.contains(r#"<border><left style="double"/><top style="thick"/><diagonal style="thin"/><vertical/><horizontal/></border>"#),"{written}");
        assert!(written.contains(r#"<xf borderId="1"><alignment wrapText="1"/><protection locked="0" hidden="1" v:keep="yes"/><extLst><v:keep/></extLst></xf>"#),"{written}");
        assert_eq!(written.matches("v:keep=\"yes\"").count(), 2);
        let parsed = XmlPart::new("xl/styles.xml".into(), bytes, Default::default());
        let mut reader = parsed.reader();
        while reader.next_event().unwrap().is_some() {}
    }
}
