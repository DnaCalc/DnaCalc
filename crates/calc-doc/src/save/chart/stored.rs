//! The objects a drawing part stores, as a session edited them (dc-zjsg.4): moved, resized, turned, flipped, given
//! new text, fill or outline, put in another place of the drawing order, or deleted; and their elements carried
//! into a group a session made or out of a group it took apart. Everything the session did not change stays byte
//! for byte as the file stores it.

use super::super::{SaveRefusal, reader::Reader, shapes};
use crate::xml::{XmlEvent, XmlPart};
use calc_doc_model::{ChartAnchor, DrawingPoint, Shape, ShapeKind, ShapeLine};
use std::collections::{BTreeMap, BTreeSet};

const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const C: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const CX: &str = "http://schemas.microsoft.com/office/drawing/2014/chartex";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn refuse(detail: &str) -> SaveRefusal {
    SaveRefusal::ChartNotModelled { part: "drawing".into(), detail: detail.into(), owner: "dc-zjsg.4" }
}

/// What the session did to an object the drawing part stores at its top level.
pub(crate) enum StoredEdit<'a> {
    /// Changed in place: the shape as it was and as it is now.
    Changed(&'a Shape, &'a Shape),
    Deleted,
}

/// One place of the drawing order the session leaves: a stored top-level object (by identity) or the `n`th new one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    Stored(u32),
    New(usize),
}

/// What a save does to a sheet's drawing objects.
pub(crate) struct StoredPlan<'a> {
    pub(crate) edits: BTreeMap<u32, StoredEdit<'a>>,
    /// The objects to write anew, in drawing order (made by the session, or a stored object's element in a new
    /// place: a group the session made, a child of a group it took apart).
    pub(crate) new: Vec<Shape>,
    /// The drawing order when it is not the stored objects in their order followed by the new ones.
    pub(crate) layout: Option<Vec<Slot>>,
    /// The stored objects (at any depth, by identity, as the file stores them) whose elements the new objects carry:
    /// each one not still in place, found once among the new objects. A new object that only reuses a stored
    /// identity is written from the model.
    pub(crate) originals: BTreeMap<u32, &'a Shape>,
    /// Every stored object at any depth, by identity, as the file stores it: what a copy is written from.
    pub(crate) sources: BTreeMap<u32, &'a Shape>,
}

impl StoredPlan<'_> {
    pub(crate) fn is_empty(&self) -> bool {
        self.edits.is_empty() && self.new.is_empty() && self.layout.is_none()
    }
}

fn flatten<'a>(shapes: &'a [Shape], out: &mut BTreeMap<u32, &'a Shape>) {
    for shape in shapes {
        out.insert(shape.id, shape);
        flatten(&shape.children, out);
    }
}

/// The plan for the stored objects `original` now that the sheet holds `shapes`: a stored top-level object still
/// at the top level (the same identity and kind) is kept, changed in place if it changed; one no longer there is
/// deleted (its element may live on in a new group or out of its group: `originals`); the rest are new. A stored
/// object changed in anything but its anchor, transform, text, fill and outline is refused, as is any edit of a
/// kept object (an equation, ink, SmartArt, OLE object: it stays as stored).
pub(crate) fn plan<'a>(original: &'a [Shape], shapes: &'a [Shape]) -> Result<StoredPlan<'a>, SaveRefusal> {
    let mut originals = BTreeMap::new();
    flatten(original, &mut originals);
    let sources = originals.clone();
    let top: BTreeMap<u32, &Shape> = original.iter().map(|s| (s.id, s)).collect();
    let mut edits = BTreeMap::new();
    let mut new = Vec::new();
    let mut order = Vec::new();
    let mut kept_ids = BTreeSet::new();
    for now in shapes {
        match top.get(&now.id) {
            Some(stored) if stored.kind == now.kind && !kept_ids.contains(&now.id) => {
                kept_ids.insert(now.id);
                order.push(Slot::Stored(now.id));
                if now == *stored {
                    continue;
                }
                let mut same = now.clone();
                (same.anchor, same.offset, same.extent, same.rotation) = (stored.anchor, stored.offset, stored.extent, stored.rotation);
                (same.flip_horizontal, same.flip_vertical) = (stored.flip_horizontal, stored.flip_vertical);
                (same.fill, same.line, same.text) = (stored.fill.clone(), stored.line.clone(), stored.text.clone());
                same.name = stored.name.clone();
                // A connector unglued (its shape deleted) or glued anew.
                (same.begin, same.end) = (stored.begin, stored.end);
                // An elbow rerouted to another class takes other adjust values with it.
                if stored.kind == ShapeKind::Connector {
                    (same.geometry, same.adjust) = (stored.geometry.clone(), stored.adjust.clone());
                }
                if same != **stored || stored.kept.is_some() {
                    return Err(refuse("Editing an imported drawing object other than placing, turning, flipping or naming it, or its text, fill or outline, is not modelled"));
                }
                edits.insert(stored.id, StoredEdit::Changed(stored, now));
            }
            _ => {
                order.push(Slot::New(new.len()));
                new.push(now.clone());
            }
        }
    }
    for stored in original {
        if !kept_ids.contains(&stored.id) {
            if stored.kept.is_some() {
                return Err(refuse("Removing a kept drawing object is not modelled"));
            }
            edits.insert(stored.id, StoredEdit::Deleted);
        }
    }
    // The order is the plain one when the stored objects keep theirs and every new one comes after them.
    let stored_order: Vec<u32> = original.iter().map(|s| s.id).filter(|id| kept_ids.contains(id)).collect();
    let plain: Vec<Slot> = stored_order.iter().map(|&id| Slot::Stored(id)).chain((0..new.len()).map(Slot::New)).collect();
    let layout = (order != plain).then_some(order);
    let mut found: BTreeMap<u32, usize> = BTreeMap::new();
    fn count(shapes: &[Shape], found: &mut BTreeMap<u32, usize>) {
        for shape in shapes {
            *found.entry(shape.id).or_default() += 1;
            count(&shape.children, found);
        }
    }
    count(&new, &mut found);
    let in_place: BTreeSet<u32> = kept_ids.iter().flat_map(|id| {
        let mut ids = BTreeMap::new();
        if let Some(shape) = top.get(id) {
            flatten(std::slice::from_ref(*shape), &mut ids);
        }
        ids.into_keys()
    }).collect();
    originals.retain(|id, stored| {
        !in_place.contains(id) && found.get(id) == Some(&1) && find_kind(&new, *id) == Some(stored.kind)
    });
    Ok(StoredPlan { edits, new, layout, originals, sources })
}

fn find_kind(shapes: &[Shape], id: u32) -> Option<ShapeKind> {
    shapes.iter().find_map(|s| if s.id == id { Some(s.kind) } else { find_kind(&s.children, id) })
}

/// One element of the drawing part.
#[derive(Clone, Debug)]
struct El {
    start: usize,
    /// The end of its start tag; equal to `end` for an empty element.
    tag_end: usize,
    end: usize,
    local: String,
    /// Its qualified name as written.
    name: String,
    parent: Option<usize>,
    /// `id` of a `cNvPr`.
    id: Option<u32>,
    /// `r:id` of a `c:chart` or `cx:chart` element.
    chart_rel: Option<String>,
}

impl El {
    fn empty(&self) -> bool {
        self.tag_end >= self.end
    }
    fn prefix(&self) -> &str {
        self.name.rsplit_once(':').map_or("", |(p, _)| p)
    }
    /// The start of its end tag.
    fn close(&self) -> usize {
        if self.empty() { self.end } else { self.end - self.name.len() - 3 }
    }
}

struct Index<'s> {
    source: &'s [u8],
    els: Vec<El>,
}

fn index(xml: &XmlPart) -> Result<Index<'_>, SaveRefusal> {
    let source = xml.bytes();
    let mut reader = Reader::new(xml);
    let mut els: Vec<El> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    let qualified = |at: usize| -> String {
        let tail = &source[at + 1..];
        let end = tail.iter().position(|b| matches!(b, b' ' | b'>' | b'/' | b'\t' | b'\r' | b'\n')).unwrap_or(tail.len());
        String::from_utf8_lossy(&tail[..end]).into_owned()
    };
    while let Some((event, span)) = reader.next()? {
        match event {
            XmlEvent::Start(e) => {
                let local = e.name.local.as_str().to_owned();
                let id = (local == "cNvPr").then(|| e.attribute("id").and_then(|v| v.parse().ok())).flatten();
                let chart_rel = (e.name.is(C, "chart") || e.name.is(CX, "chart"))
                    .then(|| {
                        e.attributes
                            .iter()
                            .find(|a| a.name.is(REL, "id"))
                            .map(|a| a.value.as_str().to_owned())
                    })
                    .flatten();
                els.push(El { start: span.0, tag_end: span.1, end: span.1, local, name: qualified(span.0), parent: open.last().copied(), id, chart_rel });
                open.push(els.len() - 1);
            }
            XmlEvent::End(_) => {
                let Some(at) = open.pop() else { break };
                els[at].end = span.1.max(els[at].start).max(els[at].tag_end);
            }
            XmlEvent::Text(_) => {}
        }
    }
    Ok(Index { source, els })
}

impl Index<'_> {
    fn children(&self, of: usize) -> impl Iterator<Item = usize> + '_ {
        (of + 1..self.els.len()).take_while(move |&i| self.els[i].start < self.els[of].end).filter(move |&i| self.els[i].parent == Some(of))
    }
    fn child(&self, of: usize, local: &str) -> Option<usize> {
        self.children(of).find(|&i| self.els[i].local == local)
    }
    fn bytes(&self, at: usize) -> &[u8] {
        &self.source[self.els[at].start..self.els[at].end]
    }
    fn ancestors(&self, at: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::successors(self.els[at].parent, move |&p| self.els[p].parent)
    }
    /// The object elements (`sp`, `pic`, `cxnSp`, `grpSp`, `graphicFrame`) whose own identity is `id`.
    fn objects(&self, id: u32) -> Vec<usize> {
        self.els
            .iter()
            .enumerate()
            .filter(|(_, e)| e.local == "cNvPr" && e.id == Some(id))
            .filter_map(|(i, _)| self.els[i].parent.and_then(|nv| self.els[nv].parent))
            .filter(|&o| matches!(self.els[o].local.as_str(), "sp" | "pic" | "cxnSp" | "grpSp" | "graphicFrame"))
            .collect()
    }
    /// The top-level children of the root, each with the first identity in it.
    fn top(&self) -> Vec<(usize, Option<u32>)> {
        self.els
            .iter()
            .enumerate()
            .filter(|(_, e)| e.parent == Some(0))
            .map(|(i, e)| (i, self.els[i + 1..].iter().take_while(|d| d.start < e.end).find_map(|d| d.id)))
            .collect()
    }
}

type Patch = ((usize, usize), Vec<u8>);

fn apply(source: &[u8], (from, to): (usize, usize), mut patches: Vec<Patch>) -> Vec<u8> {
    patches.sort_by_key(|p| p.0.0);
    let mut out = Vec::with_capacity(to - from + 64);
    let mut at = from;
    for ((s, e), bytes) in patches {
        if s < at {
            continue;
        }
        out.extend_from_slice(&source[at..s]);
        out.extend(bytes);
        at = e;
    }
    out.extend_from_slice(&source[at..to]);
    out
}

fn point_xml(prefix: &str, tag: &str, p: &DrawingPoint) -> String {
    let p_ = if prefix.is_empty() { String::new() } else { format!("{prefix}:") };
    format!(
        "<{p_}{tag}><{p_}col>{}</{p_}col><{p_}colOff>{}</{p_}colOff><{p_}row>{}</{p_}row><{p_}rowOff>{}</{p_}rowOff></{p_}{tag}>",
        p.column, p.column_offset, p.row, p.row_offset
    )
}

/// The anchor `at`'s corners, size or position as `anchor` gives them.
fn anchor_patches(ix: &Index<'_>, at: usize, anchor: &ChartAnchor, out: &mut Vec<Patch>) {
    for c in ix.children(at).collect::<Vec<_>>() {
        let e = &ix.els[c];
        let prefix = e.prefix();
        let bytes = match (e.local.as_str(), anchor) {
            ("from", ChartAnchor::TwoCell { from, .. } | ChartAnchor::OneCell { from, .. }) => point_xml(prefix, "from", from),
            ("to", ChartAnchor::TwoCell { to, .. }) => point_xml(prefix, "to", to),
            ("ext", ChartAnchor::OneCell { width, height, .. } | ChartAnchor::Absolute { width, height, .. }) => {
                format!("<{} cx=\"{width}\" cy=\"{height}\"/>", e.name)
            }
            ("pos", ChartAnchor::Absolute { x, y, .. }) => format!("<{} x=\"{x}\" y=\"{y}\"/>", e.name),
            _ => continue,
        };
        out.push(((e.start, e.end), bytes.into_bytes()));
    }
}

const FILLS: &[&str] = &["noFill", "solidFill", "gradFill", "blipFill", "pattFill", "grpFill"];

/// The patches that make the object element `at` (stored as `stored`) state `now`: its transform, and its fill,
/// outline and text where they changed.
fn object_patches(ix: &Index<'_>, at: usize, stored: &Shape, now: &Shape, out: &mut Vec<Patch>) -> Result<(), SaveRefusal> {
    let obj = &ix.els[at];
    let props = ix.child(at, "spPr").or_else(|| ix.child(at, "grpSpPr"));
    let Some(props) = props else {
        return if now.offset != stored.offset || now.extent != stored.extent || now.rotation != stored.rotation || now.fill != stored.fill || now.line != stored.line {
            Err(refuse("This stored object has no shape properties to change"))
        } else {
            Ok(())
        };
    };
    let p = &ix.els[props];
    let xfrm = ix.child(props, "xfrm");
    let a = xfrm.map_or_else(|| "a".to_owned(), |x| ix.els[x].prefix().to_owned());
    if a != "a" || !ix.source.windows(A.len()).any(|w| w == A.as_bytes()) {
        return Err(refuse("A drawing that does not bind the DrawingML prefix `a` is not rewritten"));
    }
    let mut tag = String::from("<a:xfrm");
    if now.rotation != 0 {
        tag.push_str(&format!(" rot=\"{}\"", now.rotation));
    }
    if now.flip_horizontal {
        tag.push_str(" flipH=\"1\"");
    }
    if now.flip_vertical {
        tag.push_str(" flipV=\"1\"");
    }
    let mut whole = format!("{tag}><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/>", now.offset.0, now.offset.1, now.extent.0, now.extent.1);
    if now.kind == ShapeKind::Group {
        whole.push_str(&format!(
            "<a:chOff x=\"{}\" y=\"{}\"/><a:chExt cx=\"{}\" cy=\"{}\"/>",
            now.child_offset.0, now.child_offset.1, now.child_extent.0, now.child_extent.1
        ));
    }
    whole.push_str("</a:xfrm>");
    let placed = (now.offset, now.extent, now.rotation, now.flip_horizontal, now.flip_vertical, now.child_offset, now.child_extent)
        != (stored.offset, stored.extent, stored.rotation, stored.flip_horizontal, stored.flip_vertical, stored.child_offset, stored.child_extent);
    // What goes inside the properties when it has no element yet, in schema order: the transform, the geometry, the
    // fill, the outline.
    let mut inserts: Vec<u8> = Vec::new();
    let fill_changed = now.fill != stored.fill;
    let line_changed = now.line != stored.line;
    let mut fill_bytes = Vec::new();
    if fill_changed {
        shapes::fill(&now.fill, &mut fill_bytes).map_err(refuse)?;
    }
    let line_bytes = if line_changed {
        Some(line_element(ix, props, &now.line)?)
    } else {
        None
    };
    if p.empty() {
        // `<xdr:spPr/>`: written whole.
        let mut body = format!("<{}>", p.name).into_bytes();
        if placed {
            body.extend(whole.as_bytes());
        }
        body.extend(&fill_bytes);
        body.extend(line_bytes.unwrap_or_default());
        body.extend(format!("</{}>", p.name).as_bytes());
        out.push(((p.start, p.end), body));
    } else {
        match xfrm {
            Some(x) if placed => {
                let x = &ix.els[x];
                out.push(((x.start, x.end), whole.into_bytes()));
            }
            None if placed => out.push(((p.tag_end, p.tag_end), whole.into_bytes())),
            _ => {}
        }
        let geometry = ix.child(props, "prstGeom").or_else(|| ix.child(props, "custGeom")).or(xfrm);
        let after = geometry.map_or(p.tag_end, |g| ix.els[g].end);
        let fill_el = ix.children(props).find(|&c| FILLS.contains(&ix.els[c].local.as_str()));
        let ln = ix.child(props, "ln");
        // A new outline where the properties have none goes right after the fill.
        let mut new_line = Vec::new();
        if let Some(bytes) = line_bytes {
            match ln {
                Some(l) => out.push(((ix.els[l].start, ix.els[l].end), bytes)),
                None => new_line = bytes,
            }
        }
        match fill_el {
            Some(f) if fill_changed => {
                fill_bytes.extend(new_line);
                out.push(((ix.els[f].start, ix.els[f].end), fill_bytes));
            }
            Some(f) => {
                if !new_line.is_empty() {
                    out.push(((ix.els[f].end, ix.els[f].end), new_line));
                }
            }
            None => {
                inserts.extend(fill_bytes);
                inserts.extend(new_line);
            }
        }
        if !inserts.is_empty() {
            out.push(((after, after), inserts));
        }
    }
    if (&now.geometry, &now.adjust) != (&stored.geometry, &stored.adjust) {
        let (Some(name), Some(g)) = (&now.geometry, ix.child(props, "prstGeom")) else {
            return Err(refuse("A connector's geometry is rewritten only where the file states a preset"));
        };
        let mut element = format!("<a:prstGeom prst=\"{name}\"><a:avLst>");
        for (guide, value) in &now.adjust {
            element.push_str(&format!("<a:gd name=\"{guide}\" fmla=\"val {value}\"/>"));
        }
        element.push_str("</a:avLst></a:prstGeom>");
        out.push(((ix.els[g].start, ix.els[g].end), element.into_bytes()));
    }
    if now.text != stored.text {
        text_patches(ix, at, obj, now, out)?;
    }
    if now.name != stored.name || now.id != stored.id {
        name_patch(ix, at, &now.name, (now.id != stored.id).then_some(now.id), out)?;
    }
    if (now.begin, now.end) != (stored.begin, stored.end) {
        glue_patches(ix, at, now, out)?;
    }
    Ok(())
}

/// A connector's ends (`a:stCxn`, `a:endCxn` in its `xdr:cNvCxnSpPr`) written as `now` states them: an end it no
/// longer has removed, one glued anew written, the locks beside them kept.
fn glue_patches(ix: &Index<'_>, at: usize, now: &Shape, out: &mut Vec<Patch>) -> Result<(), SaveRefusal> {
    let properties = ix
        .child(at, "nvCxnSpPr")
        .and_then(|nv| ix.child(nv, "cNvCxnSpPr"))
        .ok_or_else(|| refuse("Only a connector has ends to glue"))?;
    let p = &ix.els[properties];
    let mut ends = String::new();
    for (tag, end) in [("stCxn", now.begin), ("endCxn", now.end)] {
        if let Some(end) = end {
            ends.push_str(&format!("<a:{tag} id=\"{}\" idx=\"{}\"/>", end.shape, end.site));
        }
    }
    let locks: Vec<u8> = ix.children(properties).filter(|&c| ix.els[c].local == "cxnSpLocks").flat_map(|c| ix.bytes(c).to_vec()).collect();
    let mut body = format!("<{}>", p.name).into_bytes();
    body.extend(locks);
    body.extend(ends.as_bytes());
    body.extend(ix.children(properties).filter(|&c| ix.els[c].local == "extLst").flat_map(|c| ix.bytes(c).to_vec()));
    body.extend(format!("</{}>", p.name).as_bytes());
    out.push(((p.start, p.end), body));
    Ok(())
}

/// The object `at`'s `cNvPr` start tag with its `name` attribute set to `name`, everything else in it kept.
fn name_patch(ix: &Index<'_>, at: usize, name: &str, copy_id: Option<u32>, out: &mut Vec<Patch>) -> Result<(), SaveRefusal> {
    if name.is_empty() || !shapes::valid_text(name) {
        return Err(refuse("Use a shape name of 1 to 255 characters"));
    }
    let id = ix
        .children(at)
        .find(|&c| ix.els[c].local.starts_with("nv"))
        .and_then(|nv| ix.child(nv, "cNvPr"))
        .ok_or_else(|| refuse("The stored object has no name to change"))?;
    let e = &ix.els[id];
    let tag = String::from_utf8_lossy(&ix.source[e.start..e.tag_end]).into_owned();
    let mut value = Vec::new();
    super::super::text::attribute_value(name, &mut value);
    let value = String::from_utf8(value).expect("names are UTF-8");
    let rewritten = rename_tag(&tag, &value, copy_id)?;
    out.push(((e.start, e.tag_end), rewritten.into_bytes()));
    Ok(())
}

/// Each attribute of a start tag as (name, the span of its quoted value including the quotes), read by the XML
/// attribute grammar (any whitespace between attributes, either quote) so a value containing ` id="` is not taken
/// for an attribute.
fn tag_attributes(tag: &str) -> Vec<(&str, std::ops::Range<usize>)> {
    let bytes = tag.as_bytes();
    let mut at = 1;
    while at < bytes.len() && !bytes[at].is_ascii_whitespace() && bytes[at] != b'>' && bytes[at] != b'/' {
        at += 1;
    }
    let mut found = Vec::new();
    while at < bytes.len() {
        while at < bytes.len() && (bytes[at].is_ascii_whitespace() || bytes[at] == b'/') {
            at += 1;
        }
        let start = at;
        while at < bytes.len() && !bytes[at].is_ascii_whitespace() && !matches!(bytes[at], b'=' | b'>' | b'/') {
            at += 1;
        }
        let name = &tag[start..at];
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if name.is_empty() || at >= bytes.len() || bytes[at] != b'=' {
            break;
        }
        at += 1;
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        let Some(&quote) = bytes.get(at).filter(|q| matches!(q, b'"' | b'\'')) else { break };
        let Some(len) = tag[at + 1..].find(quote as char) else { break };
        found.push((name, at..at + len + 2));
        at += len + 2;
    }
    found
}

/// The start tag `tag` with its `name` attribute's value set to `quoted_name` (quotes included) and, for a copy, its
/// `id` to `copy_id`, everything else kept as written.
fn rename_tag(tag: &str, quoted_name: &str, copy_id: Option<u32>) -> Result<String, SaveRefusal> {
    let attributes = tag_attributes(tag);
    let span = |wanted: &str| attributes.iter().find(|(n, _)| *n == wanted).map(|(_, r)| r.clone());
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    match span("name") {
        Some(range) => edits.push((range, quoted_name.to_string())),
        None => {
            let close = if tag.ends_with("/>") { tag.len() - 2 } else { tag.len() - 1 };
            edits.push((close..close, format!(" name={quoted_name}")));
        }
    }
    if let Some(id) = copy_id {
        let range = span("id").ok_or_else(|| refuse("The stored object has no identity to copy"))?;
        edits.push((range, format!("\"{id}\"")));
    }
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut out = tag.to_string();
    for (range, text) in edits {
        out.replace_range(range, &text);
    }
    Ok(out)
}

/// The outline `line` as an `a:ln` element, keeping what the stored one states that the model does not hold (its
/// cap and compound attributes, a join, a custom dash, extensions).
fn line_element(ix: &Index<'_>, props: usize, line: &ShapeLine) -> Result<Vec<u8>, SaveRefusal> {
    let mut modelled = Vec::new();
    shapes::line(line, &mut modelled).map_err(refuse)?;
    let Some(ln) = ix.child(props, "ln") else { return Ok(modelled) };
    // Excel's attributes beside `w` (cap, cmpd, algn) and the children the model does not read.
    let tag = &ix.source[ix.els[ln].start..ix.els[ln].tag_end];
    let tag = String::from_utf8_lossy(tag);
    let mut attributes = String::new();
    for name in ["cap", "cmpd", "algn"] {
        if let Some(at) = tag.find(&format!(" {name}=\"")) {
            let rest = &tag[at + name.len() + 3..];
            if let Some(end) = rest.find('"') {
                attributes.push_str(&format!(" {name}=\"{}\"", &rest[..end]));
            }
        }
    }
    let keep = |names: &[&str]| -> Vec<u8> {
        ix.children(ln).filter(|&c| names.contains(&ix.els[c].local.as_str())).flat_map(|c| ix.bytes(c).to_vec()).collect()
    };
    let join = keep(&["round", "bevel", "miter"]);
    let custom_dash = if line.dash.is_none() { keep(&["custDash"]) } else { Vec::new() };
    let ext = keep(&["extLst"]);
    let stored_fill = if line.colour.is_none() && !line.hidden { keep(&["gradFill", "pattFill", "solidFill"]) } else { Vec::new() };
    let mut out = b"<a:ln".to_vec();
    if let Some(w) = line.weight {
        out.extend(format!(" w=\"{w}\"").as_bytes());
    }
    out.extend(attributes.as_bytes());
    out.push(b'>');
    // The modelled element's children, with what is kept placed in schema order.
    let inner = modelled
        .strip_prefix(b"<a:ln")
        .and_then(|rest| rest.iter().position(|&b| b == b'>').map(|i| rest[i + 1..].to_vec()))
        .and_then(|rest| rest.strip_suffix(b"</a:ln>").map(<[u8]>::to_vec))
        .unwrap_or_default();
    let split = |inner: &[u8], tags: &[&str]| -> usize {
        tags.iter().filter_map(|t| find(inner, t.as_bytes())).min().unwrap_or(inner.len())
    };
    let ends_at = split(&inner, &["<a:headEnd", "<a:tailEnd"]);
    let (body, ends) = inner.split_at(ends_at);
    out.extend(&stored_fill);
    out.extend(body);
    out.extend(&custom_dash);
    out.extend(&join);
    out.extend(ends);
    out.extend(&ext);
    out.extend(b"</a:ln>");
    Ok(out)
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// The text body of the object `at` written with `now`'s paragraphs, each paragraph in the first stored
/// paragraph's properties and its runs in the first stored run's (a session edits plain text).
fn text_patches(ix: &Index<'_>, at: usize, obj: &El, now: &Shape, out: &mut Vec<Patch>) -> Result<(), SaveRefusal> {
    if !matches!(obj.local.as_str(), "sp") {
        return Err(refuse("Only a shape or text box holds text"));
    }
    let paragraphs = now.text.clone().unwrap_or_default();
    let body = ix.child(at, "txBody");
    let mut run_properties = Vec::new();
    let mut paragraph_properties = Vec::new();
    if let Some(body) = body {
        let ps: Vec<usize> = ix.children(body).filter(|&c| ix.els[c].local == "p").collect();
        if let Some(&first) = ps.first() {
            if let Some(pp) = ix.child(first, "pPr") {
                paragraph_properties = ix.bytes(pp).to_vec();
            }
        }
        if let Some(rp) = ps
            .iter()
            .flat_map(|&p| ix.children(p).filter(|&c| ix.els[c].local == "r").collect::<Vec<_>>())
            .find_map(|r| ix.child(r, "rPr"))
        {
            run_properties = ix.bytes(rp).to_vec();
        }
    }
    let mut written = Vec::new();
    for paragraph in if paragraphs.is_empty() { vec![String::new()] } else { paragraphs } {
        if !shapes::valid_text(&paragraph) {
            return Err(refuse("The text holds a character a drawing cannot store"));
        }
        written.extend(b"<a:p>");
        written.extend(&paragraph_properties);
        if !paragraph.is_empty() {
            written.extend(b"<a:r>");
            written.extend(&run_properties);
            written.extend(b"<a:t>");
            shapes::element_text(&paragraph, &mut written);
            written.extend(b"</a:t></a:r>");
        }
        written.extend(b"</a:p>");
    }
    match body {
        Some(body) => {
            let ps: Vec<usize> = ix.children(body).filter(|&c| ix.els[c].local == "p").collect();
            let b = &ix.els[body];
            match (ps.first(), ps.last()) {
                (Some(&f), Some(&l)) => out.push(((ix.els[f].start, ix.els[l].end), written)),
                _ if b.empty() => {
                    let mut whole = format!("<{}><a:bodyPr/><a:lstStyle/>", b.name).into_bytes();
                    whole.extend(written);
                    whole.extend(format!("</{}>", b.name).as_bytes());
                    out.push(((b.start, b.end), whole));
                }
                _ => out.push(((b.close(), b.close()), written)),
            }
        }
        None => {
            let prefix = obj.prefix();
            let tag = if prefix.is_empty() { "txBody".to_owned() } else { format!("{prefix}:txBody") };
            let mut whole = format!("<{tag}><a:bodyPr rtlCol=\"0\" anchor=\"t\"/><a:lstStyle/>").into_bytes();
            whole.extend(written);
            whole.extend(format!("</{tag}>").as_bytes());
            if obj.empty() {
                return Err(refuse("An empty stored object cannot take text"));
            }
            out.push(((obj.close(), obj.close()), whole));
        }
    }
    Ok(())
}

/// The drawing part with the stored top-level objects `edits` names rewritten: a deleted one's anchor (or the block of
/// alternatives around it) dropped; a changed one's anchor and its object element (in every alternative alike)
/// written as it is now. Everything else stays byte for byte.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn edit(xml: &XmlPart, edits: &BTreeMap<u32, StoredEdit<'_>>) -> Result<Vec<u8>, SaveRefusal> {
    edit_with_charts(xml, edits, &BTreeMap::new(), &BTreeSet::new())
}

/// What the session changed about a chart frame the file holds: its anchor, its `cNvPr` name, or both.
pub(crate) struct FrameEdit {
    pub anchor: Option<ChartAnchor>,
    pub name: Option<String>,
}

/// Like [`edit`], also rewriting the anchors and names of chart frames whose `c:chart` / `cx:chart` relationship id appears in
/// `chart_anchors` (dc-zjsg.14), preserving `editAs` and every other byte of the frame and any alternatives around it.
pub(crate) fn edit_with_charts(
    xml: &XmlPart,
    edits: &BTreeMap<u32, StoredEdit<'_>>,
    chart_anchors: &BTreeMap<String, FrameEdit>,
    deleted_charts: &BTreeSet<String>,
) -> Result<Vec<u8>, SaveRefusal> {
    let ix = index(xml)?;
    let mut patches = Vec::new();
    let mut matched_charts: BTreeSet<String> = BTreeSet::new();
    let mut matched_deleted: BTreeSet<String> = BTreeSet::new();
    for (child, id) in ix.top() {
        let c = &ix.els[child];
        if let Some(edit) = id.and_then(|id| edits.get(&id)) {
            match edit {
                StoredEdit::Deleted => patches.push(((c.start, c.end), Vec::new())),
                StoredEdit::Changed(stored, now) => {
                    let inside = |i: usize| ix.els[i].start >= c.start && ix.els[i].end <= c.end;
                    if now.anchor != stored.anchor {
                        for anchor in (child..ix.els.len()).take_while(|&i| ix.els[i].start < c.end) {
                            if matches!(ix.els[anchor].local.as_str(), "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor") {
                                anchor_patches(&ix, anchor, &now.anchor, &mut patches);
                            }
                        }
                    }
                    for object in ix.objects(now.id).into_iter().filter(|&o| inside(o)) {
                        object_patches(&ix, object, stored, now, &mut patches)?;
                    }
                }
            }
            continue;
        }
        if chart_anchors.is_empty() && deleted_charts.is_empty() {
            continue;
        }
        // A chart frame whose chart the session moved to another sheet goes (dc-oxh8w.48).
        let gone: BTreeSet<&str> = (child..ix.els.len())
            .take_while(|&i| ix.els[i].start < c.end)
            .filter_map(|i| ix.els[i].chart_rel.as_deref())
            .filter(|rel| deleted_charts.contains(*rel))
            .collect();
        if !gone.is_empty() {
            patches.push(((c.start, c.end), Vec::new()));
            matched_deleted.extend(gone.iter().map(|r| (*r).to_owned()));
            continue;
        }
        let mut found: BTreeSet<&str> = BTreeSet::new();
        for i in (child..ix.els.len()).take_while(|&i| ix.els[i].start < c.end) {
            if let Some(rel) = ix.els[i].chart_rel.as_deref()
                && chart_anchors.contains_key(rel)
            {
                found.insert(rel);
            }
        }
        if found.is_empty() {
            continue;
        }
        if found.len() > 1 {
            return Err(refuse("Multiple chart anchors inside one drawing element are not modelled"));
        }
        let rel = *found.iter().next().unwrap();
        let edit = &chart_anchors[rel];
        let mut patched_anchor = edit.anchor.is_none();
        if let Some(new_anchor) = &edit.anchor {
            for anchor in (child..ix.els.len()).take_while(|&i| ix.els[i].start < c.end) {
                let local = ix.els[anchor].local.as_str();
                if matches!(local, "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor") {
                    let kind_matches = matches!(
                        (local, new_anchor),
                        ("twoCellAnchor", ChartAnchor::TwoCell { .. })
                            | ("oneCellAnchor", ChartAnchor::OneCell { .. })
                            | ("absoluteAnchor", ChartAnchor::Absolute { .. })
                    );
                    if !kind_matches {
                        return Err(refuse("Changing a chart anchor kind is not modelled"));
                    }
                    anchor_patches(&ix, anchor, new_anchor, &mut patches);
                    patched_anchor = true;
                }
            }
        }
        if !patched_anchor {
            return Err(refuse("A chart anchor to rewrite has no supported anchor element"));
        }
        if let Some(name) = &edit.name {
            let frame = (child..ix.els.len())
                .take_while(|&i| ix.els[i].start < c.end)
                .find(|&i| ix.els[i].local == "graphicFrame")
                .ok_or_else(|| refuse("A chart frame to rename has no graphic frame"))?;
            name_patch(&ix, frame, name, None, &mut patches)?;
        }
        matched_charts.insert(rel.to_owned());
    }
    if matched_deleted.len() != deleted_charts.len() {
        return Err(refuse("A chart frame to remove was not found in its drawing"));
    }
    if matched_charts.len() != chart_anchors.len() {
        return Err(refuse("A chart anchor to rewrite was not found in its drawing"));
    }
    Ok(apply(ix.source, (0, ix.source.len()), patches))
}

/// The element of the stored object `stored` (at any depth of the original part `xml`) as `now` states it, to be
/// written in a new place: into a group the session made, or out of a group it took apart. `None` when the part
/// does not hold it once, outside any block of alternatives.
pub(crate) fn element(xml: &XmlPart, stored: &Shape, now: &Shape) -> Result<Vec<u8>, SaveRefusal> {
    let ix = index(xml)?;
    let objects = ix.objects(stored.id);
    let [object] = objects[..] else {
        return Err(refuse("A stored object held more than once in its drawing cannot be grouped or ungrouped"));
    };
    if ix.ancestors(object).any(|a| ix.els[a].local == "AlternateContent") {
        return Err(refuse("A stored object kept with alternatives cannot be grouped or ungrouped yet"));
    }
    let mut patches = Vec::new();
    object_patches(&ix, object, stored, now, &mut patches)?;
    // The children of a stored group keep their own elements as stored.
    let o = &ix.els[object];
    Ok(apply(ix.source, (o.start, o.end), patches))
}

/// The drawing part with its shape anchors put in the order `layout` gives: each stored top-level object's place
/// in the part takes the object next in that order, a new one written where it comes (`new[n]`); what else the
/// part holds (charts, controls) keeps its place. New ones after the last stored one, and `tail`, go at the end.
pub(crate) fn arrange(xml: &XmlPart, layout: &[Slot], new: &[Vec<u8>], tail: &[u8]) -> Result<Vec<u8>, SaveRefusal> {
    let ix = index(xml)?;
    let stored: BTreeSet<u32> = layout.iter().filter_map(|s| if let Slot::Stored(id) = s { Some(*id) } else { None }).collect();
    let top = ix.top();
    let by_id: BTreeMap<u32, usize> = top.iter().filter_map(|&(c, id)| id.filter(|id| stored.contains(id)).map(|id| (id, c))).collect();
    if by_id.len() != stored.len() {
        return Err(refuse("A stored drawing object to put in order is not in its drawing"));
    }
    let mut patches = Vec::new();
    let mut slots = layout.iter().peekable();
    for &(child, id) in &top {
        if !id.is_some_and(|id| stored.contains(&id)) {
            continue;
        }
        let mut bytes = Vec::new();
        while let Some(Slot::New(n)) = slots.peek() {
            bytes.extend(&new[*n]);
            slots.next();
        }
        if let Some(Slot::Stored(id)) = slots.next() {
            bytes.extend(ix.bytes(by_id[id]));
        }
        let c = &ix.els[child];
        patches.push(((c.start, c.end), bytes));
    }
    let mut rest = Vec::new();
    for slot in slots {
        if let Slot::New(n) = slot {
            rest.extend(&new[*n]);
        }
    }
    rest.extend(tail);
    let root = &ix.els[0];
    if root.empty() {
        return Err(refuse("An empty drawing has nothing to put in order"));
    }
    patches.push(((root.close(), root.close()), rest));
    Ok(apply(ix.source, (0, ix.source.len()), patches))
}

#[cfg(test)]
mod tests {
    use super::rename_tag;

    /// dw3 review (c): the attributes are read, not searched for: tabs or newlines between them, and a `descr` or
    /// `title` that holds ` id="` or ` name="`, do not mislead the rename.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rename_tag_reads_attributes_not_substrings() {
        let tag = "<xdr:cNvPr\tid=\"5\"\nname=\"Old\" descr='a id=\"9\" name=\"x\"' title='t id=\"7\"'/>";
        let quoted = "\"New &amp; name\"";
        let renamed = rename_tag(tag, quoted, Some(12)).unwrap();
        assert_eq!(renamed, "<xdr:cNvPr\tid=\"12\"\nname=\"New &amp; name\" descr='a id=\"9\" name=\"x\"' title='t id=\"7\"'/>");
        // An id inside a description is not the identity, and a missing name is added.
        let tag = "<xdr:cNvPr descr='x id=\"3\"' id=\"4\"/>";
        assert_eq!(rename_tag(tag, "\"N\"", None).unwrap(), "<xdr:cNvPr descr='x id=\"3\"' id=\"4\" name=\"N\"/>");
        assert_eq!(rename_tag(tag, "\"N\"", Some(8)).unwrap(), "<xdr:cNvPr descr='x id=\"3\"' id=\"8\" name=\"N\"/>");
    }

    /// dc-zjsg.14: rewriting chart anchors updates `<xdr:from>`/`<xdr:to>` on both `c:chart` and `mc:AlternateContent`
    /// `cx:chart` frames while keeping `editAs="oneCell"` and every other byte unchanged.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dw5_dc_zjsg_14__chart_anchor_rewrite_preserves_edit_as_and_alternatives() {
        use super::{ChartAnchor, DrawingPoint, XmlPart, edit_with_charts};
        use std::collections::BTreeSet;
        use std::collections::BTreeMap;
        let xdr = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
        let a = "http://schemas.openxmlformats.org/drawingml/2006/main";
        let c = "http://schemas.openxmlformats.org/drawingml/2006/chart";
        let cx = "http://schemas.microsoft.com/office/drawing/2014/chartex";
        let mc = "http://schemas.openxmlformats.org/markup-compatibility/2006";
        let rel = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
        let xml = format!(
            concat!(
                "<xdr:wsDr xmlns:xdr=\"{xdr}\" xmlns:a=\"{a}\" xmlns:r=\"{rel}\" xmlns:mc=\"{mc}\">",
                "<xdr:twoCellAnchor editAs=\"oneCell\">",
                "<xdr:from><xdr:col>7</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>8</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from>",
                "<xdr:to><xdr:col>14</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>19</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>",
                "<xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"12\" name=\"Column sd\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr>",
                "<xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm>",
                "<a:graphic><a:graphicData uri=\"{c}\"><c:chart xmlns:c=\"{c}\" r:id=\"rId2\"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/>",
                "</xdr:twoCellAnchor>",
                "<xdr:twoCellAnchor editAs=\"oneCell\">",
                "<xdr:from><xdr:col>0</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>19</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from>",
                "<xdr:to><xdr:col>7</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>30</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>",
                "<mc:AlternateContent><mc:Choice Requires=\"cx1\" xmlns:cx1=\"http://schemas.microsoft.com/office/drawing/2015/9/8/chartex\">",
                "<xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"15\" name=\"Waterfall\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr>",
                "<a:graphic><a:graphicData uri=\"{cx}\"><cx:chart xmlns:cx=\"{cx}\" r:id=\"rId3\"/></a:graphicData></a:graphic></xdr:graphicFrame>",
                "</mc:Choice><mc:Fallback><xdr:sp/></mc:Fallback></mc:AlternateContent><xdr:clientData/>",
                "</xdr:twoCellAnchor>",
                "</xdr:wsDr>"
            ),
            xdr = xdr, a = a, c = c, cx = cx, mc = mc, rel = rel
        );
        let part = XmlPart::from_bytes("xl/drawings/drawing1.xml", xml.clone().into_bytes());
        let anchors = BTreeMap::from([
            (
                "rId2".to_owned(),
                ChartAnchor::TwoCell {
                    from: DrawingPoint { column: 7, column_offset: 0, row: 8, row_offset: 0 },
                    to: DrawingPoint { column: 14, column_offset: 0, row: 17, row_offset: 0 },
                },
            ),
            (
                "rId3".to_owned(),
                ChartAnchor::TwoCell {
                    from: DrawingPoint { column: 0, column_offset: 0, row: 19, row_offset: 0 },
                    to: DrawingPoint { column: 7, column_offset: 0, row: 28, row_offset: 127000 },
                },
            ),
        ]);
        let frames = |m: BTreeMap<String, ChartAnchor>| -> BTreeMap<String, super::FrameEdit> {
            m.into_iter().map(|(k, a)| (k, super::FrameEdit { anchor: Some(a), name: None })).collect()
        };
        let out = String::from_utf8(edit_with_charts(&part, &BTreeMap::new(), &frames(anchors), &BTreeSet::new()).unwrap()).unwrap();
        let expected = xml
            .replace("<xdr:row>19</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>", "<xdr:row>17</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>")
            .replace("<xdr:row>30</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>", "<xdr:row>28</xdr:row><xdr:rowOff>127000</xdr:rowOff></xdr:to>");
        assert_eq!(out, expected);
        // A missing relationship id is refused rather than silently skipped.
        let missing = BTreeMap::from([(
            "rId99".to_owned(),
            ChartAnchor::TwoCell {
                from: DrawingPoint::default(),
                to: DrawingPoint::default(),
            },
        )]);
        assert!(edit_with_charts(&part, &BTreeMap::new(), &frames(missing), &BTreeSet::new()).is_err());
    }
}
