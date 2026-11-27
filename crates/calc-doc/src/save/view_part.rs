//! The versioned custom XML part. Retain its source spans so saving an anchor leaves other
//! sections and data from a later writer intact.

use super::text::{RawTag, Splice, attribute_value, xstring};
use crate::load::address;
use crate::xml::{XmlElement, XmlEvent, XmlPart};
use calc_doc_model::value::{CellKey, GridArea};
use calc_doc_model::{
    AnchorId, CellInput, DocId, ModelRoot, NoteId, SheetKey, VIEW_PART_NAMESPACE, ViewAnchor,
    ViewDocument, ViewNote, ViewPart,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

type Span = (usize, usize);

struct StoredAnchor {
    anchor: ViewAnchor,
    fingerprint: Option<String>,
    start: Span,
    whole: Span,
}

struct StoredNote {
    note: ViewNote,
    start: Span,
    whole: Span,
}

struct StoredDocument {
    document: ViewDocument,
    start: Span,
    whole: Span,
}

struct Index {
    root: Span,
    digest: Option<String>,
    root_close: usize,
    prefs: Option<(Span, Option<String>)>,
    anchors: Option<(Span, usize)>,
    known: BTreeMap<AnchorId, StoredAnchor>,
    ids: BTreeSet<AnchorId>,
    notes: Option<(Span, usize)>,
    known_notes: BTreeMap<NoteId, StoredNote>,
    note_ids: BTreeSet<NoteId>,
    declarations: Option<(Span, usize)>,
    known_documents: BTreeMap<DocId, StoredDocument>,
    document_ids: BTreeSet<DocId>,
}

fn is(element: &XmlElement, local: &str) -> bool {
    element.name.local == local && element.name.namespace.as_deref() == Some(VIEW_PART_NAMESPACE)
}

fn parse_anchor(el: &XmlElement, model: &ModelRoot, id: AnchorId) -> Option<ViewAnchor> {
    let spill = match el.attribute("kind")? {
        "cells" => false,
        "spill" => true,
        _ => return None,
    };
    let location = || {
        if el.attribute("state") == Some("orphan") {
            return None;
        }
        let key = SheetKey::new(el.attribute("sheet_key")?.parse().ok()?);
        let sheet = model.sheet_by_key(key)?.id();
        let reference = el.attribute("ref")?;
        let reference = reference
            .rsplit_once('!')
            .map_or(reference, |(_, address)| address);
        let (first, last) = reference.split_once(':').unwrap_or((reference, reference));
        let cell = |text: &str| {
            let (row, column) = address::cell(&text.replace('$', ""))?;
            CellKey::new(sheet, row, column).ok()
        };
        GridArea::new(cell(first)?, cell(last)?).ok()
    };
    match location() {
        Some(area) if spill && area.cell_count() == 1 => Some(ViewAnchor::spill(id, area.first())),
        Some(area) if !spill => Some(ViewAnchor::cells(id, area)),
        _ => Some(ViewAnchor::orphan(id, spill)),
    }
}

fn index(xml: &XmlPart, model: &ModelRoot) -> Option<Index> {
    let mut read = xml.reader();
    let Some(XmlEvent::Start(root)) = read.next_event().ok()? else {
        return None;
    };
    if !is(&root, "dnaViews") || root.attribute("version") != Some("1") {
        return None;
    }
    let digest = root.attribute("model").map(str::to_owned);
    let root = read.span();
    let mut index = Index {
        root,
        digest,
        root_close: root.1,
        prefs: None,
        anchors: None,
        known: BTreeMap::new(),
        ids: BTreeSet::new(),
        notes: None,
        known_notes: BTreeMap::new(),
        note_ids: BTreeSet::new(),
        declarations: None,
        known_documents: BTreeMap::new(),
        document_ids: BTreeSet::new(),
    };
    let mut depth = 1;
    let mut in_anchors = false;
    let mut in_notes = false;
    let mut pending = None;
    let mut pending_note: Option<StoredNote> = None;
    let mut in_declarations = false;
    let mut pending_document: Option<StoredDocument> = None;
    while let Some(event) = read.next_event().ok()? {
        let span = read.span();
        match event {
            XmlEvent::Start(el) => {
                if depth == 1 && is(&el, "prefs") {
                    let preferred = el.attribute("preferred").map(str::to_owned);
                    if index.prefs.replace((span, preferred)).is_some() {
                        return None;
                    }
                } else if depth == 1 && is(&el, "anchors") {
                    if index.anchors.replace((span, span.1)).is_some() {
                        return None;
                    }
                    in_anchors = true;
                } else if depth == 2 && in_anchors && is(&el, "anchor") {
                    if let Some(id) = el.attribute("id").and_then(|id| id.parse().ok()) {
                        if !index.ids.insert(id) {
                            return None;
                        }
                        pending = parse_anchor(&el, model, id).map(|anchor| StoredAnchor {
                            anchor,
                            fingerprint: el.attribute("signature").map(str::to_owned),
                            start: span,
                            whole: span,
                        });
                    }
                } else if depth == 1 && is(&el, "notes") {
                    if index.notes.replace((span, span.1)).is_some() {
                        return None;
                    }
                    in_notes = true;
                } else if depth == 2 && in_notes && is(&el, "note") {
                    if let Some(id) = el.attribute("id").and_then(|id| id.parse().ok()) {
                        if !index.note_ids.insert(id) {
                            return None;
                        }
                        pending_note =
                            el.attribute("anchor")
                                .and_then(|id| id.parse().ok())
                                .map(|anchor| StoredNote {
                                    note: ViewNote {
                                        id,
                                        anchor,
                                        text: String::new(),
                                    },
                                    start: span,
                                    whole: span,
                                });
                    }
                } else if depth == 1 && is(&el, "decls") {
                    if index.declarations.replace((span, span.1)).is_some() {
                        return None;
                    }
                    in_declarations = true;
                } else if depth == 2 && in_declarations && is(&el, "decl") {
                    if let Some(id) = el.attribute("id").and_then(|id| id.parse().ok()) {
                        if !index.document_ids.insert(id) {
                            return None;
                        }
                        pending_document = parse_document(&el, id).map(|document| StoredDocument {
                            document,
                            start: span,
                            whole: span,
                        });
                    }
                } else if depth >= 3 && in_declarations {
                    pending_document = None;
                } else if depth >= 3 && in_notes {
                    // Retain richer note elements from future writers as source.
                    pending_note = None;
                }
                depth += 1;
            }
            XmlEvent::End(_) => {
                depth -= 1;
                if depth == 2
                    && let Some(mut stored) = pending.take()
                {
                    stored.whole.1 = span.1;
                    index.known.insert(stored.anchor.id, stored);
                }
                if depth == 2
                    && let Some(mut stored) = pending_note.take()
                {
                    stored.whole.1 = span.1;
                    if let Ok(text) =
                        String::from_utf16(&crate::load::strings::decode(&stored.note.text))
                    {
                        stored.note.text = text;
                        index.known_notes.insert(stored.note.id, stored);
                    }
                }
                if depth == 2
                    && let Some(mut stored) = pending_document.take()
                {
                    stored.whole.1 = span.1;
                    index.known_documents.insert(stored.document.id, stored);
                }
                if depth == 1 && in_declarations {
                    index.declarations.as_mut().unwrap().1 = span.0;
                    in_declarations = false;
                }
                if depth == 1 && in_notes {
                    index.notes.as_mut().unwrap().1 = span.0;
                    in_notes = false;
                }
                if depth == 1 && in_anchors {
                    index.anchors.as_mut().unwrap().1 = span.0;
                    in_anchors = false;
                }
                if depth == 0 {
                    index.root_close = span.0;
                }
            }
            XmlEvent::Text(text) => {
                if let Some(note) = pending_note.as_mut() {
                    note.note.text.push_str(&text);
                }
                if let Some(document) = pending_document.as_mut() {
                    document.document.body.push_str(&text);
                }
            }
        }
    }
    Some(index)
}

fn parse_document(el: &XmlElement, id: DocId) -> Option<ViewDocument> {
    Some(ViewDocument {
        id,
        view: el.attribute("view")?.to_owned(),
        sheet: el
            .attribute("sheet_key")
            .map(|key| key.parse().map(SheetKey::new))
            .transpose()
            .ok()?,
        schema: el.attribute("schema")?.parse().ok()?,
        body: String::new(),
    })
}

fn sized(digest: &mut Sha256, bytes: &[u8]) {
    digest.update((bytes.len() as u64).to_le_bytes());
    digest.update(bytes);
}

fn text(digest: &mut Sha256, text: &calc_doc_model::value::ExcelText) {
    let units = text.utf16_code_units();
    digest.update((units.len() as u64).to_le_bytes());
    for unit in units {
        digest.update(unit.to_le_bytes());
    }
}

fn input(digest: &mut Sha256, input: &CellInput) {
    match input {
        CellInput::Number(number) => {
            digest.update([1]);
            digest.update(number.get().to_bits().to_le_bytes());
        }
        CellInput::Text(value) => {
            digest.update([2]);
            text(digest, value.text());
        }
        CellInput::Logical(value) => digest.update([3, u8::from(*value)]),
        CellInput::Error(code) => {
            digest.update([4]);
            sized(digest, code.excel_spelling().as_bytes());
        }
        CellInput::Formula(formula) => {
            digest.update([5]);
            // The workbook's shared-formula representation can change on save.
            let rendered = formula.rendered().to_string_lossy();
            let canonical = calc_fml::assist::display_formula(rendered.trim_start_matches('='));
            sized(digest, canonical.as_bytes());
        }
    }
}

fn model_digest(model: &ModelRoot) -> String {
    let mut digest = Sha256::new();
    digest.update(b"dnacalc-view-part-cells-v1");
    for sheet in model.sheets() {
        digest.update([1]);
        digest.update(sheet.key().get().to_le_bytes());
        text(&mut digest, sheet.name().text());
        for (at, cell) in model.sheet_cells(sheet.id()).into_iter().flatten() {
            digest.update([2]);
            digest.update(at.row().to_le_bytes());
            digest.update(at.column().to_le_bytes());
            input(&mut digest, cell);
        }
    }
    for name in model.definitions().names() {
        digest.update([3]);
        let scope = match name.scope {
            calc_doc_model::NameScope::Workbook => 0,
            calc_doc_model::NameScope::Sheet(sheet) => {
                u64::from(
                    model
                        .sheet_by_id(sheet)
                        .expect("local name sheet")
                        .key()
                        .get(),
                ) + 1
            }
        };
        digest.update(scope.to_le_bytes());
        sized(&mut digest, name.name.as_bytes());
        sized(&mut digest, name.formula.as_bytes());
    }
    format!("sha256:{:x}", digest.finalize())
}

fn fingerprint(area: GridArea, model: &ModelRoot) -> String {
    let mut digest = Sha256::new();
    digest.update(b"dnacalc-view-part-anchor-v1");
    digest.update(area.row_count().to_le_bytes());
    digest.update(area.column_count().to_le_bytes());
    for (at, cell) in model.cells(area).into_iter().flatten() {
        digest.update((at.row() - area.first().row()).to_le_bytes());
        digest.update((at.column() - area.first().column()).to_le_bytes());
        input(&mut digest, cell);
    }
    format!("sha256:{:x}", digest.finalize())
}

pub(crate) fn read(package: &crate::opc::Package, model: &ModelRoot) -> ViewPart {
    let parts = crate::custom_xml::parts_matching(package, |namespace| {
        namespace
            .strip_prefix("urn:dnacalc:views:v")
            .and_then(|version| version.parse::<u32>().ok())
            .is_some()
    });
    let Some(path) = parts.first() else {
        return ViewPart::default();
    };
    let Ok(bytes) = package.read_part(path) else {
        return ViewPart::opaque(Vec::new());
    };
    let xml = XmlPart::from_bytes(path, bytes);
    if parts.len() != 1 {
        return ViewPart::opaque(xml.bytes().to_vec());
    }
    match index(&xml, model) {
        Some(index) => {
            let same_model = index
                .digest
                .as_deref()
                .is_some_and(|digest| digest == model_digest(model));
            let anchors = index.known.into_values().map(|stored| {
                let Some(area) = stored.anchor.area() else {
                    return stored.anchor;
                };
                if same_model
                    || stored
                        .fingerprint
                        .as_deref()
                        .is_some_and(|stored| stored == fingerprint(area, model))
                {
                    stored.anchor
                } else {
                    ViewAnchor::orphan(stored.anchor.id, stored.anchor.spill)
                }
            });
            let notes = index.known_notes.into_values().map(|stored| stored.note);
            let documents = index
                .known_documents
                .into_values()
                .map(|stored| stored.document);
            ViewPart::loaded(xml.bytes().to_vec(), anchors, notes, documents)
                .with_preferred(index.prefs.and_then(|(_, preferred)| preferred))
        }
        None => ViewPart::opaque(xml.bytes().to_vec()),
    }
}

fn attribute_fields(anchor: &ViewAnchor, model: &ModelRoot) -> (Option<String>, Option<String>) {
    let Some(area) = anchor.area() else {
        return (None, None);
    };
    let key = model
        .sheet_by_id(area.sheet())
        .expect("view anchors have live sheets")
        .key();
    let address_of =
        |cell: CellKey| format!("{}{}", address::column_letters(cell.column()), cell.row());
    let mut reference = address_of(area.first());
    if area.first() != area.last() {
        reference.push(':');
        reference.push_str(&address_of(area.last()));
    }
    (Some(key.get().to_string()), Some(reference))
}

fn new_anchor(anchor: &ViewAnchor, model: &ModelRoot, out: &mut Vec<u8>) {
    let (sheet, reference) = attribute_fields(anchor, model);
    let signature = anchor.area().map(|area| fingerprint(area, model));
    out.extend_from_slice(b"<anchor");
    for (name, value) in [
        ("xmlns", Some(VIEW_PART_NAMESPACE)),
        ("id", Some(&anchor.id.to_string())),
        ("kind", Some(if anchor.spill { "spill" } else { "cells" })),
        ("sheet_key", sheet.as_deref()),
        ("ref", reference.as_deref()),
        ("signature", signature.as_deref()),
        ("state", anchor.area().is_none().then_some("orphan")),
    ] {
        if let Some(value) = value {
            out.push(b' ');
            out.extend_from_slice(name.as_bytes());
            out.push(b'=');
            attribute_value(value, out);
        }
    }
    out.extend_from_slice(b"/>");
}

/// Inserts into a container which may have been written with a self-closing start tag.
fn insert(splice: &mut PartEdits, source: &[u8], start: Span, close: usize, body: &[u8]) {
    let tag = RawTag::parse(&source[start.0..start.1]);
    if tag.is_empty() {
        let mut open = tag.rebuilt(&[], false);
        open.extend_from_slice(body);
        open.extend_from_slice(b"</");
        open.extend_from_slice(tag.qualified_name());
        open.push(b'>');
        splice.replace(start, &open);
    } else {
        splice.replace((close, close), body);
    }
}

pub(super) fn item(
    base: &ModelRoot,
    model: &ModelRoot,
) -> Result<Option<crate::CustomXmlItem>, super::SaveRefusal> {
    let part = model.view_part();
    if part.is_opaque()
        || (part == base.view_part()
            && (part.source().is_none() || model.revision() == base.revision()))
    {
        return Ok(None);
    }
    let fallback = format!("<dnaViews xmlns=\"{VIEW_PART_NAMESPACE}\" version=\"1\"/>");
    let original = base.view_part().source().unwrap_or(fallback.as_bytes());
    let xml = XmlPart::from_bytes(VIEW_PART_NAMESPACE, original.to_vec());
    let index = index(&xml, base).ok_or_else(|| super::SaveRefusal::InvalidPart {
        part: VIEW_PART_NAMESPACE.to_owned(),
        detail: "the retained view part is not editable".into(),
    })?;
    let mut splice = PartEdits::default();
    let digest = model_digest(model);
    let root = RawTag::parse(&original[index.root.0..index.root.1]);
    if !root.is_empty() {
        splice.replace(
            index.root,
            &root.rebuilt(&[("model", Some(&digest))], false),
        );
    }
    let mut ordered = index.known.iter().collect::<Vec<_>>();
    ordered.sort_unstable_by_key(|(_, stored)| stored.start.0);
    for (&id, stored) in ordered {
        let Some(anchor) = part.anchor(id) else {
            splice.replace(stored.whole, &[]);
            continue;
        };
        let signature = anchor.area().map(|area| fingerprint(area, model));
        if anchor == &stored.anchor && signature == stored.fingerprint {
            continue;
        }
        let tag = RawTag::parse(&original[stored.start.0..stored.start.1]);
        let (sheet, reference) = attribute_fields(anchor, model);
        let mut changes = vec![
            ("kind", Some(if anchor.spill { "spill" } else { "cells" })),
            ("state", anchor.area().is_none().then_some("orphan")),
        ];
        if anchor.area().is_some() {
            changes.extend([
                ("sheet_key", sheet.as_deref()),
                ("ref", reference.as_deref()),
                ("signature", signature.as_deref()),
            ]);
        }
        splice.replace(stored.start, &tag.rebuilt(&changes, tag.is_empty()));
    }
    let mut new = Vec::new();
    for anchor in part
        .anchors()
        .filter(|anchor| !index.known.contains_key(&anchor.id))
    {
        if index.ids.contains(&anchor.id) {
            return Err(super::SaveRefusal::InvalidPart {
                part: VIEW_PART_NAMESPACE.to_owned(),
                detail: format!("a retained anchor already uses {}", anchor.id),
            });
        }
        new_anchor(anchor, model, &mut new);
    }
    let mut new_sections = Vec::new();
    let preferred = part.preferred();
    match (&index.prefs, preferred) {
        (Some((span, old)), preferred) if old.as_deref() != preferred => {
            let tag = RawTag::parse(&original[span.0..span.1]);
            splice.replace(*span, &tag.rebuilt(&[("preferred", preferred)], tag.is_empty()));
        }
        (None, Some(preferred)) => {
            new_sections.extend_from_slice(b"<prefs xmlns=");
            attribute_value(VIEW_PART_NAMESPACE, &mut new_sections);
            new_sections.extend_from_slice(b" preferred=");
            attribute_value(preferred, &mut new_sections);
            new_sections.extend_from_slice(b"/>");
        }
        _ => {}
    }
    append_section(
        &mut splice,
        original,
        &mut new_sections,
        "anchors",
        index.anchors,
        &new,
    );

    let mut ordered_notes = index.known_notes.values().collect::<Vec<_>>();
    ordered_notes.sort_unstable_by_key(|stored| stored.start.0);
    for stored in ordered_notes {
        let Some(note) = part.note(stored.note.id) else {
            splice.replace(stored.whole, &[]);
            continue;
        };
        if note != &stored.note {
            let tag = RawTag::parse(&original[stored.start.0..stored.start.1]);
            let anchor = note.anchor.to_string();
            let mut open = tag.rebuilt(&[("anchor", Some(&anchor))], false);
            xstring(&note.text.encode_utf16().collect::<Vec<_>>(), &mut open);
            open.extend_from_slice(b"</");
            open.extend_from_slice(tag.qualified_name());
            open.push(b'>');
            splice.replace(stored.whole, &open);
        }
    }
    let mut new = Vec::new();
    for note in part
        .notes()
        .filter(|note| !index.known_notes.contains_key(&note.id))
    {
        if index.note_ids.contains(&note.id) {
            return Err(super::SaveRefusal::InvalidPart {
                part: VIEW_PART_NAMESPACE.to_owned(),
                detail: format!("a retained note already uses {}", note.id),
            });
        }
        new.extend_from_slice(b"<note xmlns=");
        attribute_value(VIEW_PART_NAMESPACE, &mut new);
        new.extend_from_slice(b" id=");
        attribute_value(&note.id.to_string(), &mut new);
        new.extend_from_slice(b" anchor=");
        attribute_value(&note.anchor.to_string(), &mut new);
        new.push(b'>');
        xstring(&note.text.encode_utf16().collect::<Vec<_>>(), &mut new);
        new.extend_from_slice(b"</note>");
    }
    append_section(
        &mut splice,
        original,
        &mut new_sections,
        "notes",
        index.notes,
        &new,
    );
    write_documents(part, &index, original, &mut splice, &mut new_sections)?;
    if root.is_empty() {
        let mut open = root.rebuilt(&[("model", Some(&digest))], new_sections.is_empty());
        if !new_sections.is_empty() {
            open.extend_from_slice(&new_sections);
            open.extend_from_slice(b"</");
            open.extend_from_slice(root.qualified_name());
            open.push(b'>');
        }
        splice.replace(index.root, &open);
    } else if !new_sections.is_empty() {
        insert(
            &mut splice,
            original,
            index.root,
            index.root_close,
            &new_sections,
        );
    }
    Ok(Some(crate::CustomXmlItem {
        namespace: VIEW_PART_NAMESPACE.to_owned(),
        content: splice.finish(original),
    }))
}

fn append_section(
    splice: &mut PartEdits,
    original: &[u8],
    sections: &mut Vec<u8>,
    name: &str,
    existing: Option<(Span, usize)>,
    new: &[u8],
) {
    if new.is_empty() {
        return;
    }
    if let Some((start, close)) = existing {
        insert(splice, original, start, close, new);
    } else {
        sections.extend_from_slice(format!("<{name} xmlns=\"{VIEW_PART_NAMESPACE}\">").as_bytes());
        sections.extend_from_slice(new);
        sections.extend_from_slice(format!("</{name}>").as_bytes());
    }
}

fn write_documents(
    part: &ViewPart,
    index: &Index,
    original: &[u8],
    edits: &mut PartEdits,
    new_sections: &mut Vec<u8>,
) -> Result<(), super::SaveRefusal> {
    for stored in index.known_documents.values() {
        let Some(document) = part.document(stored.document.id) else {
            edits.replace(stored.whole, &[]);
            continue;
        };
        if document != &stored.document {
            let tag = RawTag::parse(&original[stored.start.0..stored.start.1]);
            let sheet = document.sheet.map(|sheet| sheet.get().to_string());
            let mut bytes = tag.rebuilt(
                &[
                    ("view", Some(&document.view)),
                    ("schema", Some(&document.schema.to_string())),
                    ("sheet_key", sheet.as_deref()),
                ],
                false,
            );
            document_text(document, tag.qualified_name(), &mut bytes);
            edits.replace(stored.whole, &bytes);
        }
    }
    let mut new = Vec::new();
    for document in part
        .documents()
        .filter(|doc| !index.known_documents.contains_key(&doc.id))
    {
        if index.document_ids.contains(&document.id) {
            return Err(super::SaveRefusal::InvalidPart {
                part: VIEW_PART_NAMESPACE.to_owned(),
                detail: format!("a retained declaration already uses {}", document.id),
            });
        }
        let sheet = document.sheet.map(|sheet| sheet.get().to_string());
        new.extend_from_slice(b"<decl xmlns=");
        attribute_value(VIEW_PART_NAMESPACE, &mut new);
        for (key, value) in [
            ("id", Some(document.id.to_string())),
            ("view", Some(document.view.clone())),
            ("schema", Some(document.schema.to_string())),
            ("sheet_key", sheet),
        ] {
            if let Some(value) = value {
                new.push(b' ');
                new.extend_from_slice(key.as_bytes());
                new.push(b'=');
                attribute_value(&value, &mut new);
            }
        }
        new.push(b'>');
        document_text(document, b"decl", &mut new);
    }
    append_section(
        edits,
        original,
        new_sections,
        "decls",
        index.declarations,
        &new,
    );
    Ok(())
}

fn document_text(document: &ViewDocument, tag: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(quick_xml::escape::escape(&document.body).as_bytes());
    out.extend_from_slice(b"</");
    out.extend_from_slice(tag);
    out.push(b'>');
}

/// A file can place its recognized sections in any order. Splice in source order after
/// collecting the replacements, leaving unknown attributes and neighboring elements intact.
#[derive(Default)]
struct PartEdits(Vec<(Span, Vec<u8>)>);

impl PartEdits {
    fn replace(&mut self, span: Span, bytes: &[u8]) {
        self.0.push((span, bytes.to_vec()));
    }

    fn finish(mut self, source: &[u8]) -> Vec<u8> {
        self.0.sort_by_key(|(span, _)| *span);
        let mut splice = Splice::new(source);
        for (span, bytes) in self.0 {
            splice.replace(span, &bytes);
        }
        splice.finish()
    }
}
