//! What a save writes when the revision's skin layout differs from the loaded one: the DNA view
//! manifest's item part, rewritten or added with its item properties and relationships, and the
//! locator names a created or removed block adds to or takes from the workbook part.
//!
//! A manifest that was read is rewritten from its own bytes. Its document element's start and end
//! tags, and every element of it other than `blocks` and `views`, are kept as the part wrote them,
//! in place. Inside `blocks`, a block the revision keeps is kept as the part wrote it (a block's
//! ID, kind and locator never change); a new block is written; a removed one is left out. Inside
//! each view, the same holds for placements, except that a placement the revision moved to a new
//! parent or a new canvas area gets its start tag rebuilt with the new values, its other
//! attributes and its content as the part wrote them. Each view element keeps its place in
//! `views`, and a view the source lacks is added after the last one it holds.
//!
//! The blocks of `blocks`, and the placements of a view, take in their new order the places the
//! source's blocks or placements held, the n-th the n-th one's place. Every other child of
//! `blocks`, `views` or a view (an element version 1 does not name: an unknown field) and every
//! byte between children (whitespace, comments, processing instructions) is kept as the part
//! wrote it, in its place among them: an unknown child that followed the second placement still
//! follows the second placement of the new order. Blocks or placements beyond the source's count
//! follow the last of its places; a place no block or placement is left for is left empty, the
//! bytes around it kept. Every unknown attribute of a kept or rebuilt element is kept. What a
//! removed block or placement held, comments in it included, is left out with it, unless the
//! same revision creates one again with the same ID (and, for a block, the same kind and locator):
//! the source's element is then reused, as for one the revision keeps.

use super::SaveRefusal;
use super::text::{RawTag, attribute_value, number};
use crate::load::manifest::{
    CUSTOM_XML_PROPS, MANIFEST_NAMESPACE, MANIFEST_VERSION, ManifestDocument, ManifestProblem,
    Node, read_manifest,
};
use crate::opc::Package;
use calc_doc_model::{
    BlockId, BlockKind, LocatorName, Placement, PlacementId, SkinLayout, ViewKind,
};
use std::collections::BTreeMap;

/// The content type of a custom XML item part.
pub(super) const ITEM_CONTENT_TYPE: &str = "application/xml";
/// The content type of a custom XML item properties part.
pub(super) const PROPS_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.customXmlProperties+xml";
/// The content type of a relationships part.
pub(super) const RELATIONSHIPS_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-package.relationships+xml";
/// The datastore item ID the item properties of a manifest part a save adds carry. A package
/// holds at most one DNA manifest, so one ID serves every package.
const DATASTORE_ITEM_ID: &str = "{8C3B6A52-1D4E-4F7A-9B20-5E61D7C4A913}";
const DATASTORE_NAMESPACE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/customXml";
const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";

/// The manifest parts a save adds to a package that holds none: the item, its item properties and
/// the item's relationships to them, each with its name and content.
pub(super) struct NewManifest {
    pub(super) item: (String, Vec<u8>),
    pub(super) props: (String, Vec<u8>),
    pub(super) relationships: (String, Vec<u8>),
}

/// The bytes of a manifest written for `layout` from nothing.
pub(super) fn fresh(layout: &SkinLayout) -> Vec<u8> {
    let mut out = DECLARATION.as_bytes().to_vec();
    out.extend_from_slice(b"<manifest xmlns=");
    attribute_value(MANIFEST_NAMESPACE, &mut out);
    out.extend_from_slice(b" version=");
    attribute_value(MANIFEST_VERSION, &mut out);
    out.push(b'>');
    out.extend(blocks_element(&[], None, b"", layout, &BTreeMap::new()));
    out.extend(views_element(&[], None, b"", layout, &BTreeMap::new()));
    out.extend_from_slice(b"</manifest>");
    out
}

/// The parts of a manifest a save adds, numbered `n`: `customXml/item<n>.xml` holding `item`,
/// `customXml/itemProps<n>.xml` and `customXml/_rels/item<n>.xml.rels`.
pub(super) fn new_parts(n: u32, item: Vec<u8>) -> NewManifest {
    let props_target = format!("itemProps{n}.xml");
    let mut props = b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\r\n<ds:datastoreItem ds:itemID=".to_vec();
    attribute_value(DATASTORE_ITEM_ID, &mut props);
    props.extend_from_slice(b" xmlns:ds=");
    attribute_value(DATASTORE_NAMESPACE, &mut props);
    props.extend_from_slice(b"><ds:schemaRefs><ds:schemaRef ds:uri=");
    attribute_value(MANIFEST_NAMESPACE, &mut props);
    props.extend_from_slice(b"/></ds:schemaRefs></ds:datastoreItem>");
    let mut relationships = DECLARATION.as_bytes().to_vec();
    relationships.extend_from_slice(b"<Relationships xmlns=");
    attribute_value(crate::opc::RELATIONSHIPS_NAMESPACE, &mut relationships);
    relationships.extend_from_slice(b"><Relationship Id=\"rId1\" Type=");
    attribute_value(CUSTOM_XML_PROPS, &mut relationships);
    relationships.extend_from_slice(b" Target=");
    attribute_value(&props_target, &mut relationships);
    relationships.extend_from_slice(b"/></Relationships>");
    NewManifest {
        item: (format!("customXml/item{n}.xml"), item),
        props: (format!("customXml/{props_target}"), props),
        relationships: (format!("customXml/_rels/item{n}.xml.rels"), relationships),
    }
}

/// The smallest `n` from 1 for which the package holds none of the parts [`new_parts`] names,
/// and no content type override names one.
pub(super) fn free_number(package: &Package) -> u32 {
    let taken = |name: &str| {
        package.part(name).is_some()
            || package.content_types().overrides().iter().any(|(part, _)| {
                part.strip_prefix('/')
                    .is_some_and(|p| p.eq_ignore_ascii_case(name))
            })
    };
    (1u32..)
        .find(|n| {
            !taken(&format!("customXml/item{n}.xml"))
                && !taken(&format!("customXml/itemProps{n}.xml"))
                && !taken(&format!("customXml/_rels/item{n}.xml.rels"))
        })
        .expect("a free number before the counter runs out")
}

/// The manifest at `part` rewritten for `layout`, from its own bytes as the module describes.
pub(super) fn rewritten(
    package: &Package,
    part: &str,
    layout: &SkinLayout,
) -> Result<Vec<u8>, SaveRefusal> {
    let xml = super::strings::read_part(package, part)?;
    let invalid = |detail: String| SaveRefusal::InvalidPart {
        part: part.to_owned(),
        detail,
    };
    let document = read_manifest(&xml).map_err(|problem| match problem {
        ManifestProblem::UnsupportedVersion(version) => {
            invalid(format!("the manifest's version is now {version:?}"))
        }
        ManifestProblem::Unreadable(detail) => invalid(detail),
    })?;
    Ok(merged(xml.bytes(), &document, layout))
}

/// The end tag `</prefix:local>`.
fn end_tag(prefix: &[u8], local: &str) -> Vec<u8> {
    let mut out = b"</".to_vec();
    out.extend_from_slice(prefix);
    out.extend_from_slice(local.as_bytes());
    out.push(b'>');
    out
}

/// The prefix a container's children are written with: the container's own when the source holds
/// it, its parent's otherwise.
fn prefix_of(source: &[u8], node: Option<&Node>, parent_prefix: &[u8]) -> Vec<u8> {
    match node {
        Some(node) => RawTag::parse(&source[node.start_tag.0..node.start_tag.1])
            .prefix()
            .to_vec(),
        None => parent_prefix.to_vec(),
    }
}

/// A container element written for its new children. Without a source element, a new one with
/// `prefix` holding `after`. With one, rewritten in place: its tags, every byte between its
/// children (whitespace, comments, processing instructions) and every child `fill` gives `None`
/// for, kept as `source` wrote them; each child `fill` gives bytes for replaced by them (by
/// nothing, to leave it out); and `after` written right after the last child `fill` replaced, or
/// before the end tag when it replaced none. A source element written empty (`<a/>`) is kept as
/// written when `after` is empty, and given an end tag otherwise.
fn container(
    source: &[u8],
    node: Option<&Node>,
    prefix: &[u8],
    local: &str,
    mut fill: impl FnMut(&Node) -> Option<Vec<u8>>,
    after: Vec<u8>,
) -> Vec<u8> {
    let Some(node) = node else {
        let mut out = b"<".to_vec();
        out.extend_from_slice(prefix);
        out.extend_from_slice(local.as_bytes());
        out.push(b'>');
        out.extend(after);
        out.extend(end_tag(prefix, local));
        return out;
    };
    let start_tag = &source[node.start_tag.0..node.start_tag.1];
    let tag = RawTag::parse(start_tag);
    if tag.is_empty() {
        if after.is_empty() {
            return start_tag.to_vec();
        }
        let mut out = tag.rebuilt(&[], false);
        out.extend(after);
        out.extend(end_tag(tag.prefix(), local));
        return out;
    }
    let replaced: Vec<Option<Vec<u8>>> = node.children.iter().map(&mut fill).collect();
    let last = replaced.iter().rposition(Option::is_some);
    let mut after = Some(after);
    let mut out = start_tag.to_vec();
    let mut cursor = node.start_tag.1;
    for (index, (child, replacement)) in node.children.iter().zip(replaced).enumerate() {
        let (start, end) = child.whole();
        out.extend_from_slice(&source[cursor..start]);
        match replacement {
            Some(bytes) => out.extend(bytes),
            None => out.extend_from_slice(&source[start..end]),
        }
        if Some(index) == last {
            out.extend(after.take().unwrap_or_default());
        }
        cursor = end;
    }
    out.extend_from_slice(&source[cursor..node.end_tag.0]);
    out.extend(after.take().unwrap_or_default());
    out.extend_from_slice(&source[node.end_tag.0..node.end_tag.1]);
    out
}

/// A container whose known children (`known`) form one ordered list: `items`, the new known
/// children in their new order, take the places the source's known children held, the n-th item
/// the n-th known child's place, so every unknown child and every byte between children keeps its
/// place among them; items beyond the source's known children follow the last of them, and a
/// place no item is left for is left empty.
fn slotted(
    source: &[u8],
    node: Option<&Node>,
    prefix: &[u8],
    local: &str,
    known: impl Fn(&Node) -> bool,
    items: Vec<Vec<u8>>,
) -> Vec<u8> {
    let places = node.map_or(0, |node| {
        node.children.iter().filter(|child| known(child)).count()
    });
    let mut items = items.into_iter();
    let mut in_places: Vec<Vec<u8>> = items.by_ref().take(places).collect();
    in_places.reverse();
    let after: Vec<u8> = items.flatten().collect();
    container(
        source,
        node,
        prefix,
        local,
        |child| known(child).then(|| in_places.pop().unwrap_or_default()),
        after,
    )
}

/// The `blocks` element for `layout`: its blocks in the layout's order, in the places of the
/// source's blocks ([`slotted`]); a block the revision keeps as `source` wrote it (a block's ID,
/// kind and locator never change), a new one (a block removed and created again with another
/// kind among them) written. `before` holds each block's kind and locator as the source manifest
/// held them.
fn blocks_element(
    source: &[u8],
    node: Option<&Node>,
    parent_prefix: &[u8],
    layout: &SkinLayout,
    before: &BTreeMap<BlockId, (BlockKind, &LocatorName)>,
) -> Vec<u8> {
    let prefix = prefix_of(source, node, parent_prefix);
    let mut kept: BTreeMap<BlockId, (usize, usize)> = BTreeMap::new();
    for child in node
        .map(|node| node.children.as_slice())
        .unwrap_or_default()
    {
        if child.is("block")
            && let Some(id) = child.element.attribute("id").and_then(BlockId::parse)
        {
            kept.insert(id, child.whole());
        }
    }
    let items = layout
        .blocks()
        .iter()
        .map(|block| {
            let same = before.get(&block.id()) == Some(&(block.kind(), block.locator()));
            match kept.get(&block.id()) {
                Some(&(start, end)) if same => source[start..end].to_vec(),
                _ => {
                    let mut out = b"<".to_vec();
                    out.extend_from_slice(&prefix);
                    out.extend_from_slice(b"block id=");
                    attribute_value(&block.id().to_string(), &mut out);
                    out.extend_from_slice(b" kind=");
                    attribute_value(block.kind().name(), &mut out);
                    out.extend_from_slice(b" locator=");
                    attribute_value(block.locator().as_str(), &mut out);
                    out.extend_from_slice(b"/>");
                    out
                }
            }
        })
        .collect();
    slotted(
        source,
        node,
        &prefix,
        "blocks",
        |child| child.is("block"),
        items,
    )
}

/// The known attributes of a placement in its view, as (name, value) in the order a new element
/// writes them after `id`.
fn placement_values(placement: &Placement) -> Vec<(&'static str, Option<String>)> {
    let mut values = vec![("block", Some(placement.block().to_string()))];
    match placement.view() {
        ViewKind::Tree => values.push(("parent", placement.parent().map(|p| p.to_string()))),
        ViewKind::Canvas => {
            let area = placement
                .canvas_area()
                .expect("a canvas placement has an area");
            values.extend([
                ("x", Some(number(area.x.get()))),
                ("y", Some(number(area.y.get()))),
                ("width", Some(number(area.width.get()))),
                ("height", Some(number(area.height.get()))),
            ]);
        }
        ViewKind::Notebook | ViewKind::Grid => {}
    }
    values
}

/// One view's element for `placements`: the placements in the view's order, in the places of the
/// source's placements ([`slotted`]); one the revision keeps unchanged as `source` wrote it, a
/// changed one with its start tag rebuilt (its other attributes and its content as written), a
/// new one written. `before` holds the placements as the source manifest held them.
fn view_element(
    source: &[u8],
    node: Option<&Node>,
    parent_prefix: &[u8],
    view: ViewKind,
    placements: &[Placement],
    before: &BTreeMap<PlacementId, &Placement>,
) -> Vec<u8> {
    let prefix = prefix_of(source, node, parent_prefix);
    let mut kept: BTreeMap<PlacementId, &Node> = BTreeMap::new();
    for child in node
        .map(|node| node.children.as_slice())
        .unwrap_or_default()
    {
        if child.is("placement")
            && let Some(id) = child.element.attribute("id").and_then(PlacementId::parse)
        {
            kept.insert(id, child);
        }
    }
    let items = placements
        .iter()
        .map(|placement| match kept.get(&placement.id()) {
            Some(child) if before.get(&placement.id()) == Some(&placement) => {
                let (start, end) = child.whole();
                source[start..end].to_vec()
            }
            Some(child) => {
                let values = placement_values(placement);
                let changes: Vec<(&str, Option<&str>)> = values
                    .iter()
                    .map(|(name, value)| (*name, value.as_deref()))
                    .collect();
                let tag = RawTag::parse(&source[child.start_tag.0..child.start_tag.1]);
                let mut out = tag.rebuilt(&changes, tag.is_empty());
                out.extend_from_slice(&source[child.start_tag.1..child.end_tag.1]);
                out
            }
            None => {
                let mut out = b"<".to_vec();
                out.extend_from_slice(&prefix);
                out.extend_from_slice(b"placement id=");
                attribute_value(&placement.id().to_string(), &mut out);
                for (name, value) in placement_values(placement) {
                    if let Some(value) = value {
                        out.push(b' ');
                        out.extend_from_slice(name.as_bytes());
                        out.push(b'=');
                        attribute_value(&value, &mut out);
                    }
                }
                out.extend_from_slice(b"/>");
                out
            }
        })
        .collect();
    slotted(
        source,
        node,
        &prefix,
        view.name(),
        |child| child.is("placement"),
        items,
    )
}

/// The `views` element for `layout`: each view element the source holds rewritten in its own
/// place ([`view_element`]), every other child and every byte between them as written, and each
/// view the layout places a block in that the source lacks added, in the order [`ViewKind::ALL`]
/// lists them, after the last view element the source holds.
fn views_element(
    source: &[u8],
    node: Option<&Node>,
    parent_prefix: &[u8],
    layout: &SkinLayout,
    before: &BTreeMap<PlacementId, &Placement>,
) -> Vec<u8> {
    let prefix = prefix_of(source, node, parent_prefix);
    let view_of = |child: &Node| ViewKind::ALL.into_iter().find(|view| child.is(view.name()));
    let held: Vec<ViewKind> = node
        .map(|node| node.children.as_slice())
        .unwrap_or_default()
        .iter()
        .filter_map(view_of)
        .collect();
    let mut after = Vec::new();
    for view in ViewKind::ALL {
        if !held.contains(&view) && !layout.view(view).is_empty() {
            after.extend(view_element(
                source,
                None,
                &prefix,
                view,
                layout.view(view),
                before,
            ));
        }
    }
    container(
        source,
        node,
        &prefix,
        "views",
        |child| {
            view_of(child).map(|view| {
                view_element(
                    source,
                    Some(child),
                    &prefix,
                    view,
                    layout.view(view),
                    before,
                )
            })
        },
        after,
    )
}

/// The manifest `source` rewritten for `layout`: `blocks` and `views` replaced in place (added
/// before the document element's end tag when the source has none), everything else as written.
fn merged(source: &[u8], document: &ManifestDocument, layout: &SkinLayout) -> Vec<u8> {
    let root = &document.root;
    let root_tag = RawTag::parse(&source[root.start_tag.0..root.start_tag.1]);
    let prefix = root_tag.prefix().to_vec();
    let before: BTreeMap<PlacementId, &Placement> = document
        .placements
        .iter()
        .map(|placement| (placement.id(), placement))
        .collect();
    let blocks = root.children.iter().find(|child| child.is("blocks"));
    let views = root.children.iter().find(|child| child.is("views"));
    let blocks_before: BTreeMap<BlockId, (BlockKind, &LocatorName)> = document
        .blocks
        .iter()
        .map(|(id, kind, locator)| (*id, (*kind, locator)))
        .collect();
    let blocks_bytes = blocks_element(source, blocks, &prefix, layout, &blocks_before);
    let views_bytes = views_element(source, views, &prefix, layout, &before);
    let mut out = Vec::with_capacity(source.len() + 256);
    if root_tag.is_empty() {
        out.extend_from_slice(&source[..root.start_tag.0]);
        out.extend(root_tag.rebuilt(&[], false));
        out.extend(blocks_bytes);
        out.extend(views_bytes);
        out.extend(end_tag(&prefix, "manifest"));
        out.extend_from_slice(&source[root.start_tag.1..]);
        return out;
    }
    // Each replacement in source order; the containers the source lacks go before the end tag.
    let mut replacements: Vec<((usize, usize), Vec<u8>)> = Vec::new();
    let mut missing = Vec::new();
    match blocks {
        Some(node) => replacements.push((node.whole(), blocks_bytes)),
        None => missing.extend(blocks_bytes),
    }
    match views {
        Some(node) => replacements.push((node.whole(), views_bytes)),
        None => missing.extend(views_bytes),
    }
    replacements.push(((root.end_tag.0, root.end_tag.0), missing));
    replacements.sort_by_key(|(range, _)| *range);
    let mut cursor = 0;
    for ((start, end), bytes) in replacements {
        out.extend_from_slice(&source[cursor..start]);
        out.extend(bytes);
        cursor = end;
    }
    out.extend_from_slice(&source[cursor..]);
    out
}
