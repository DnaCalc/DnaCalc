//! The objects a drawing part stores, as a session edited them (dc-zjsg.4): moved, resized, turned, flipped, given
//! new text, fill or outline, put in another place of the drawing order, or deleted; and their elements carried
//! into a group a session made or out of a group it took apart. Everything the session did not change stays byte
//! for byte as the file stores it.

use super::super::{SaveRefusal, reader::Reader, shapes};
use crate::xml::{XmlEvent, XmlPart};
use calc_doc_model::{ChartAnchor, DrawingPoint, Shape, ShapeKind, ShapeLine};
use std::collections::{BTreeMap, BTreeSet};

const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

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
    Ok(StoredPlan { edits, new, layout, originals })
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
                els.push(El { start: span.0, tag_end: span.1, end: span.1, local, name: qualified(span.0), parent: open.last().copied(), id });
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
    if now.text != stored.text {
        text_patches(ix, at, obj, now, out)?;
    }
    if now.name != stored.name {
        name_patch(ix, at, &now.name, out)?;
    }
    Ok(())
}

/// The object `at`'s `cNvPr` start tag with its `name` attribute set to `name`, everything else in it kept.
fn name_patch(ix: &Index<'_>, at: usize, name: &str, out: &mut Vec<Patch>) -> Result<(), SaveRefusal> {
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
    // The tag's attributes, the `name` one found by its leading space so `descr` or `title` are not taken for it.
    let rewritten = match tag.find(" name=") {
        Some(at) => {
            let quote = tag[at + 6..].chars().next().unwrap_or('"');
            let rest = &tag[at + 7..];
            let end = rest.find(quote).ok_or_else(|| refuse("The stored object's name is not quoted"))?;
            format!("{} name={value}{}", &tag[..at], &rest[end + 1..])
        }
        None => {
            let close = if tag.ends_with("/>") { tag.len() - 2 } else { tag.len() - 1 };
            format!("{} name={value}{}", &tag[..close], &tag[close..])
        }
    };
    out.push(((e.start, e.tag_end), rewritten.into_bytes()));
    Ok(())
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
pub(crate) fn edit(xml: &XmlPart, edits: &BTreeMap<u32, StoredEdit<'_>>) -> Result<Vec<u8>, SaveRefusal> {
    let ix = index(xml)?;
    let mut patches = Vec::new();
    for (child, id) in ix.top() {
        let Some(edit) = id.and_then(|id| edits.get(&id)) else { continue };
        let c = &ix.els[child];
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
