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
    // Colour filters append differential fonts/fills. Imported records, including their
    // unread fields, remain the source prefix just as they do for ordinary cell styles.
    for dxf in after
        .differential_formats
        .get(before.differential_formats.len()..)
        .unwrap_or(&[])
    {
        if dxf.border.is_some()
            || dxf.alignment.is_some()
            || dxf.protection.is_some()
            || dxf
                .font
                .as_ref()
                .is_some_and(|f| !f.other_children.is_empty())
            || dxf
                .fill
                .as_ref()
                .is_some_and(|f| f.gradient.is_some() || !f.other_children.is_empty())
        {
            return Err(error(
                "Authoring this differential format is not modelled (dc-971p.37)".into(),
            ));
        }
    }
    append(
        &mut root,
        "dxfs",
        "dxf",
        &before.differential_formats,
        &after.differential_formats,
        &empty,
        |mut node, before, after| {
            if let Some(f) = &after.font {
                node.set_child(
                    "font",
                    Some(font(
                        Node::new(&node.prefix(), "font"),
                        &before.font.clone().unwrap_or_default(),
                        f,
                    )),
                );
            }
            // A differential format's number format (schema order: font, numFmt, fill).
            if let Some(format) = &after.number_format {
                let mut n = Node::new(&node.prefix(), "numFmt");
                n.attr("numFmtId", Some(format.id.to_string()));
                n.attr("formatCode", Some(format.code.clone()));
                node.set_child("numFmt", Some(n));
            }
            if let Some(f) = &after.fill {
                node.set_child(
                    "fill",
                    Some(fill(
                        Node::new(&node.prefix(), "fill"),
                        &before.fill.clone().unwrap_or_default(),
                        f,
                    )),
                );
            }
            node
        },
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

/// Compact defaults plus the rectangles SpreadsheetML must express as cell records.
/// These are a save plan only: authored workbook storage remains sparse StyleRuns.
#[derive(Default)]
pub(super) struct SheetStyles {
    pub columns: Vec<(u32, u32, u32)>,
    pub rows: Vec<(u32, u32, u32)>,
    pub cells: Vec<calc_doc_model::StyleRun>,
}
impl SheetStyles {
    pub fn column(&self, index: u32) -> u32 {
        self.columns
            .iter()
            .find(|(a, b, _)| (*a..=*b).contains(&index))
            .map_or(0, |(_, _, s)| *s)
    }
    pub fn row(&self, index: u32) -> Option<u32> {
        self.rows
            .iter()
            .find(|(a, b, _)| (*a..=*b).contains(&index))
            .map(|(_, _, s)| *s)
    }
}
pub(super) fn sheet_plan(
    xml: &crate::xml::XmlPart,
    sheet: calc_doc_model::value::SheetId,
    runs: &calc_doc_model::StyleRuns,
) -> Result<Option<SheetStyles>, super::SaveRefusal> {
    use crate::load::MAIN_NAMESPACE;
    use crate::xml::XmlEvent;
    use calc_doc_model::value::{CellKey, GridArea, MAX_COLUMNS, MAX_ROWS};
    use calc_doc_model::{StyleRun, StyleRuns};
    if !runs.iter().any(|r| r.area.sheet() == sheet) {
        return Ok(None);
    }
    let area = |r1, c1, r2, c2| {
        GridArea::new(
            CellKey::new(sheet, r1, c1).unwrap(),
            CellKey::new(sheet, r2, c2).unwrap(),
        )
        .unwrap()
    };
    let mut reader = super::reader::Reader::new(xml);
    reader.root("worksheet")?;
    let mut cols = Vec::new();
    let mut rows = Vec::new();
    let mut previous_row = 0;
    // Read only axis defaults; cell-specific records continue through the ordinary sheet writer.
    loop {
        match reader.expect()? {
            (XmlEvent::Start(e), _) if e.name.is(MAIN_NAMESPACE, "cols") => loop {
                match reader.expect()? {
                    (XmlEvent::Start(col), _) => {
                        if col.name.is(MAIN_NAMESPACE, "col") {
                            let span = col
                                .attribute("min")
                                .and_then(|s| s.parse::<u32>().ok())
                                .zip(col.attribute("max").and_then(|s| s.parse::<u32>().ok()));
                            if let (Some((a, b)), Some(style)) =
                                (span, col.attribute("style").and_then(|s| s.parse().ok()))
                                && a >= 1
                                && a <= b
                                && b <= MAX_COLUMNS
                            {
                                cols.push(StyleRun {
                                    area: area(1, a, MAX_ROWS, b),
                                    format: style,
                                });
                            }
                        }
                        reader.element_end()?;
                    }
                    (XmlEvent::End(_), _) => break,
                    _ => {}
                }
            },
            (XmlEvent::Start(e), _) if e.name.is(MAIN_NAMESPACE, "sheetData") => loop {
                match reader.expect()? {
                    (XmlEvent::Start(row), _) => {
                        if row.name.is(MAIN_NAMESPACE, "row") {
                            let index = row
                                .attribute("r")
                                .and_then(|s| s.parse::<u32>().ok())
                                .unwrap_or(previous_row + 1);
                            previous_row = index;
                            if let Some(style) = row.attribute("s").and_then(|s| s.parse().ok())
                                && (1..=MAX_ROWS).contains(&index)
                                && matches!(row.attribute("customFormat"), Some("1" | "true"))
                            {
                                rows.push(StyleRun {
                                    area: area(index, 1, index, MAX_COLUMNS),
                                    format: style,
                                });
                            }
                        }
                        reader.element_end()?;
                    }
                    (XmlEvent::End(_), _) => break,
                    _ => {}
                }
            },
            (XmlEvent::Start(_), _) => {
                reader.element_end()?;
            }
            (XmlEvent::End(_), _) => break,
            _ => {}
        }
    }
    let plane = StyleRuns::regions(
        GridArea::full_sheet(sheet),
        cols.into_iter().rev().chain(rows).chain(runs.iter()),
    );
    let mut edges = std::collections::BTreeSet::from([1, MAX_ROWS + 1]);
    for r in &plane {
        edges.insert(r.area.first().row());
        edges.insert(r.area.last().row() + 1);
    }
    let edges: Vec<_> = edges.into_iter().collect();
    let strip = |row: u32| {
        let mut spans: Vec<_> = plane
            .iter()
            .filter(|r| (r.area.first().row()..=r.area.last().row()).contains(&row))
            .map(|r| (r.area.first().column(), r.area.last().column(), r.format))
            .collect();
        spans.sort();
        let mut merged: Vec<(u32, u32, u32)> = Vec::new();
        for (a, b, s) in spans {
            if let Some(last) = merged.last_mut()
                && last.1 + 1 == a
                && last.2 == s
            {
                last.1 = b;
            } else {
                merged.push((a, b, s));
            }
        }
        merged
    };
    // Pick the column defaults by the resulting XML work, rather than strip height:
    // a uniform row override is much cheaper than thousands of cell exceptions.
    let strips: Vec<_> = edges
        .windows(2)
        .map(|e| (e[0], e[1] - 1, strip(e[0])))
        .collect();
    let exceptions = |columns: &[(u32, u32, u32)], spans: &[(u32, u32, u32)]| {
        let mut weights = std::collections::BTreeMap::<u32, u64>::new();
        for &(a, b, s) in spans {
            *weights.entry(s).or_default() += u64::from(b - a + 1);
        }
        let default = *weights
            .iter()
            .max_by_key(|(s, n)| (**n, std::cmp::Reverse(**s)))
            .unwrap()
            .0;
        let mut differences = Vec::new();
        for &(a, b, s) in spans {
            for &(c, d, old) in columns {
                let (first, last) = (a.max(c), b.min(d));
                if first <= last && s != old {
                    differences.push((first, last, s));
                }
            }
        }
        let by_column: u64 = differences
            .iter()
            .map(|(a, b, _)| u64::from(b - a + 1))
            .sum();
        let by_row = u64::from(MAX_COLUMNS) - weights[&default];
        if by_column <= by_row {
            (None, differences)
        } else {
            (
                Some(default),
                spans
                    .iter()
                    .copied()
                    .filter(|(_, _, s)| *s != default)
                    .collect(),
            )
        }
    };
    let mut candidates: Vec<_> = strips.iter().map(|(_, _, s)| s.clone()).collect();
    candidates.push(vec![(1, MAX_COLUMNS, 0)]);
    candidates.sort();
    candidates.dedup();
    let columns = candidates
        .into_iter()
        .min_by_key(|columns| {
            strips
                .iter()
                .map(|(first, last, spans)| {
                    let (row, cells) = exceptions(columns, spans);
                    let width: u64 = cells.iter().map(|(a, b, _)| u64::from(b - a + 1)).sum();
                    u64::from(last - first + 1) * (width * 32 + u64::from(row.is_some()) * 24)
                })
                .sum::<u64>()
        })
        .unwrap();
    let mut plan = SheetStyles {
        columns,
        ..Default::default()
    };
    for (first, last, spans) in strips {
        let (row, cells) = exceptions(&plan.columns, &spans);
        if let Some(default) = row {
            plan.rows.push((first, last, default));
        }
        for (a, b, s) in cells {
            plan.cells.push(StyleRun {
                area: area(first, a, last, b),
                format: s,
            });
        }
    }
    let cells: u64 = plan.cells.iter().map(|r| r.area.cell_count()).sum();
    if cells > 2_000_000 {
        return Err(super::SaveRefusal::StyleExpansionNotModelled {
            cells,
            owner: "dc-971p.6",
        });
    }
    Ok(Some(plan))
}

#[cfg(test)]
mod sheet_style_tests {
    use super::*;
    use calc_doc_model::value::{CellKey, GridArea, MAX_ROWS, SheetId};
    use calc_doc_model::{StyleRun, StyleRuns};
    fn xml() -> XmlPart {
        XmlPart::new("xl/worksheets/sheet1.xml".into(),br#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData/></worksheet>"#.to_vec(),Default::default())
    }
    fn area(s: SheetId, r1: u32, c1: u32, r2: u32, c2: u32) -> GridArea {
        GridArea::new(
            CellKey::new(s, r1, c1).unwrap(),
            CellKey::new(s, r2, c2).unwrap(),
        )
        .unwrap()
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn half_sheet_uses_axis_defaults_instead_of_billions_of_empty_cells() {
        let s = SheetId::from_parts(0, 0);
        let mut runs = StyleRuns::default();
        runs.assign(StyleRun {
            area: area(s, 1, 1, 524288, 8192),
            format: 1,
        });
        let plan = sheet_plan(&xml(), s, &runs).unwrap().unwrap();
        assert!(plan.cells.is_empty());
        assert_eq!(plan.columns.len(), 2);
        assert_eq!(plan.rows, vec![(524289, MAX_ROWS, 0)]);
        assert_eq!(plan.column(1), 1);
        assert_eq!(plan.column(8193), 0);
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn serialization_refuses_excessive_cell_expansion_before_building_rows() {
        let s = SheetId::from_parts(0, 0);
        let mut runs = StyleRuns::default();
        // A different permanent column default prevents representing the surrounding
        // nonuniform rows with just a uniform row override.
        runs.assign(StyleRun {
            area: area(s, 1, 26, MAX_ROWS, 702),
            format: 2,
        });
        runs.assign(StyleRun {
            area: area(s, 2, 2, 300002, 13),
            format: 1,
        });
        assert!(
            matches!(sheet_plan(&xml(),s,&runs),Err(SaveRefusal::StyleExpansionNotModelled {cells,owner:"dc-971p.6"}) if cells>2_000_000)
        );
    }
}
