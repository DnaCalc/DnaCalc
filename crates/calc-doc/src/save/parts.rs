//! The workbook-level parts an authored save changes, each rewritten from its own bytes: the
//! workbook part's `calcPr` and `definedNames`, the workbook part's relationships, and
//! `[Content_Types].xml`.

use super::SaveRefusal;
use super::reader::Reader;
use super::text::{RawTag, Splice, attribute_value, xstring};
use crate::load::MAIN_NAMESPACE;
use crate::opc::{CONTENT_TYPES_NAMESPACE, RELATIONSHIPS_NAMESPACE};
use crate::xml::{XmlEvent, XmlPart};

/// The elements CT_Workbook places after `calcPr`, before which a missing `calcPr` is inserted.
const AFTER_CALC_PR: [&str; 9] = [
    "oleSize",
    "customWorkbookViews",
    "pivotCaches",
    "smartTagPr",
    "smartTagTypes",
    "webPublishing",
    "fileRecoveryPr",
    "webPublishObjects",
    "extLst",
];

/// The locator names a save takes from and adds to the workbook part's `definedNames`.
#[derive(Default)]
pub(super) struct NameChanges {
    /// Names whose hidden, workbook-scoped `definedName` is removed, compared ignoring ASCII case.
    /// A name of this list the part holds visible, or scoped to a sheet, is kept.
    pub(super) removed: Vec<String>,
    /// Each name added, hidden and workbook-scoped, with its formula text as UTF-16 code units.
    pub(super) added: Vec<(String, Vec<u16>)>,
    /// Each name whose formula changed (rows or columns were inserted or deleted): its name, its
    /// `localSheetId` (`None` for a workbook-scoped name) and its new formula text.
    pub(super) replaced: Vec<(String, Option<u32>, Vec<u16>)>,
}

impl NameChanges {
    pub(super) fn is_empty(&self) -> bool {
        self.removed.is_empty() && self.added.is_empty() && self.replaced.is_empty()
    }
}

/// A `definedName` element's name, whether it is workbook-scoped (has no `localSheetId`) and
/// whether it is hidden.
struct NameScope {
    name: String,
    workbook_scoped: bool,
    hidden: bool,
    /// Its `localSheetId`, when it has one that is a number.
    local_sheet: Option<u32>,
    /// The range of its start tag.
    tag: (usize, usize),
}

/// The workbook part's `definedNames` element as read: the ranges of its start tag and end tag,
/// and each child's range, with, for a `definedName`, its name and scope.
struct DefinedNames {
    start_tag: (usize, usize),
    end_tag: (usize, usize),
    children: Vec<((usize, usize), Option<NameScope>)>,
}

/// Reads the `definedNames` element whose start tag `reader` just read at `start_tag`.
fn defined_names(
    reader: &mut Reader<'_>,
    start_tag: (usize, usize),
) -> Result<DefinedNames, SaveRefusal> {
    let mut children = Vec::new();
    loop {
        match reader.expect()? {
            (XmlEvent::Start(child), span) => {
                let end = reader.element_end()?;
                let name = child
                    .name
                    .is(MAIN_NAMESPACE, "definedName")
                    .then(|| NameScope {
                        name: child.attribute("name").unwrap_or_default().to_owned(),
                        workbook_scoped: child.attribute("localSheetId").is_none(),
                        hidden: crate::load::manifest::is_true(child.attribute("hidden")),
                        local_sheet: child
                            .attribute("localSheetId")
                            .and_then(|id| crate::load::sheet::collapsed(id).parse().ok()),
                        tag: span,
                    });
                children.push(((span.0, end), name));
            }
            (XmlEvent::Text(_), _) => {}
            (XmlEvent::End(_), end_tag) => {
                return Ok(DefinedNames {
                    start_tag,
                    end_tag,
                    children,
                });
            }
        }
    }
}

/// The workbook part rewritten: with `fullCalcOnLoad="1"` on its `calcPr` when `full_calc` (which
/// asks the application that opens the package to recalculate every formula; whether Excel does
/// is measured, tests/fixtures/d1/excel-open.json), set on the `calcPr` the part holds, its other
/// attributes kept, or a new `calcPr` at the place the schema gives it; and with `names` applied
/// to its `definedNames`: each removed name's hidden, workbook-scoped `definedName` left out (and
/// the element with it when nothing else remains in it), and each added name appended as a hidden,
/// workbook-scoped `definedName`, in a new `definedNames` at the place the schema gives it when
/// the part has none. Every other byte is the part's own. Refused when the part holds two
/// `calcPr` or two `definedNames` elements, and when an added name is already a workbook-scoped
/// defined name the part keeps ([`SaveRefusal::LocatorNameTaken`]).
pub(super) fn workbook(
    xml: &XmlPart,
    full_calc: bool,
    names: &NameChanges,
) -> Result<Vec<u8>, SaveRefusal> {
    let source = xml.bytes();
    let mut reader = Reader::new(xml);
    let (_, root) = reader.root("workbook")?;
    let prefix = RawTag::parse(&source[root.0..root.1]).prefix().to_vec();
    let mut calc_pr = None;
    let mut defined = None;
    let mut insert_calc_at = None;
    let mut insert_names_at = None;
    loop {
        match reader.expect()? {
            (XmlEvent::Start(element), span) => {
                let main = element.name.namespace.as_deref() == Some(MAIN_NAMESPACE);
                let local = element.name.local.as_str();
                if main && local == "definedNames" {
                    if defined.is_some() {
                        return Err(reader.invalid("a workbook with two definedNames elements"));
                    }
                    defined = Some(defined_names(&mut reader, span)?);
                    continue;
                }
                if main && (local == "calcPr" || AFTER_CALC_PR.contains(&local)) {
                    insert_names_at.get_or_insert(span.0);
                }
                if main && local == "calcPr" {
                    if calc_pr.is_some() {
                        return Err(reader.invalid("a workbook with two calcPr elements"));
                    }
                    calc_pr = Some(span);
                } else if main && AFTER_CALC_PR.contains(&local) {
                    insert_calc_at.get_or_insert(span.0);
                }
                reader.element_end()?;
            }
            (XmlEvent::Text(_), _) => {}
            (XmlEvent::End(_), (start, _)) => {
                insert_calc_at.get_or_insert(start);
                insert_names_at.get_or_insert(start);
                break;
            }
        }
    }
    reader.end()?;

    // Each change as (range replaced, its order among changes at one offset, new bytes).
    let mut changes: Vec<((usize, usize), u8, Vec<u8>)> = Vec::new();
    if !names.is_empty() {
        // A name is dropped only when it is a hidden, workbook-scoped name the save removes; every
        // other workbook-scoped name is kept, and an added name may not be one of those.
        let dropped = |child: &Option<NameScope>| {
            child.as_ref().is_some_and(|scope| {
                scope.workbook_scoped
                    && scope.hidden
                    && names
                        .removed
                        .iter()
                        .any(|removed| removed.eq_ignore_ascii_case(&scope.name))
            })
        };
        let held = defined
            .as_ref()
            .map(|d| d.children.as_slice())
            .unwrap_or_default();
        for (added, _) in &names.added {
            let taken = held.iter().any(|(_, child)| {
                !dropped(child)
                    && child.as_ref().is_some_and(|scope| {
                        scope.workbook_scoped && scope.name.eq_ignore_ascii_case(added)
                    })
            });
            if taken {
                return Err(SaveRefusal::LocatorNameTaken {
                    part: xml.name().to_owned(),
                    name: added.clone(),
                });
            }
        }
        let element_prefix = match &defined {
            Some(defined) => RawTag::parse(&source[defined.start_tag.0..defined.start_tag.1])
                .prefix()
                .to_vec(),
            None => prefix.clone(),
        };
        let mut added = Vec::new();
        for (name, text) in &names.added {
            added.push(b'<');
            added.extend_from_slice(&element_prefix);
            added.extend_from_slice(b"definedName name=");
            attribute_value(name, &mut added);
            added.extend_from_slice(b" hidden=\"1\">");
            xstring(text, &mut added);
            added.extend_from_slice(b"</");
            added.extend_from_slice(&element_prefix);
            added.extend_from_slice(b"definedName>");
        }
        let close = |out: &mut Vec<u8>| {
            out.extend_from_slice(b"</");
            out.extend_from_slice(&element_prefix);
            out.extend_from_slice(b"definedNames>");
        };
        match &defined {
            Some(defined) => {
                let remaining = defined.children.iter().filter(|(_, c)| !dropped(c)).count();
                let whole = (defined.start_tag.0, defined.end_tag.1);
                let tag = RawTag::parse(&source[defined.start_tag.0..defined.start_tag.1]);
                if remaining == 0 && added.is_empty() {
                    changes.push((whole, 0, Vec::new()));
                } else if tag.is_empty() {
                    let mut bytes = tag.rebuilt(&[], false);
                    bytes.extend_from_slice(&added);
                    close(&mut bytes);
                    changes.push((whole, 0, bytes));
                } else {
                    for (range, child) in &defined.children {
                        if dropped(child) {
                            changes.push((*range, 0, Vec::new()));
                            continue;
                        }
                        let Some(scope) = child else {
                            continue;
                        };
                        let replaced = names.replaced.iter().find(|(name, sheet, _)| {
                            name.eq_ignore_ascii_case(&scope.name)
                                && *sheet == scope.local_sheet
                                && scope.workbook_scoped == sheet.is_none()
                        });
                        let Some((_, _, text)) = replaced else {
                            continue;
                        };
                        let mut content = Vec::new();
                        xstring(text, &mut content);
                        let child_tag = RawTag::parse(&source[scope.tag.0..scope.tag.1]);
                        if child_tag.is_empty() {
                            let mut bytes = child_tag.rebuilt(&[], false);
                            bytes.extend_from_slice(&content);
                            bytes.extend_from_slice(b"</");
                            bytes.extend_from_slice(child_tag.prefix());
                            bytes.extend_from_slice(b"definedName>");
                            changes.push((*range, 0, bytes));
                        } else {
                            let end_tag = source[..range.1]
                                .windows(2)
                                .rposition(|window| window == b"</")
                                .expect("an element that is not empty has an end tag");
                            changes.push(((scope.tag.1, end_tag), 0, content));
                        }
                    }
                    let at = defined.end_tag.0;
                    changes.push(((at, at), 0, added));
                }
            }
            None if !added.is_empty() => {
                let at = insert_names_at.expect("set when the document element ends");
                let mut bytes = b"<".to_vec();
                bytes.extend_from_slice(&element_prefix);
                bytes.extend_from_slice(b"definedNames>");
                bytes.extend_from_slice(&added);
                close(&mut bytes);
                changes.push(((at, at), 0, bytes));
            }
            None => {}
        }
    }
    if full_calc {
        match calc_pr {
            Some(span) => {
                let tag = RawTag::parse(&source[span.0..span.1]);
                let empty = tag.is_empty();
                changes.push((
                    span,
                    1,
                    tag.rebuilt(&[("fullCalcOnLoad", Some("1"))], empty),
                ));
            }
            None => {
                let at = insert_calc_at.expect("set when the document element ends");
                let mut element = b"<".to_vec();
                element.extend_from_slice(&prefix);
                element.extend_from_slice(br#"calcPr fullCalcOnLoad="1"/>"#);
                changes.push(((at, at), 1, element));
            }
        }
    }
    changes.sort_by_key(|(range, order, _)| (range.0, *order, range.1));
    let mut splice = Splice::new(source);
    for (range, _, bytes) in changes {
        splice.replace(range, &bytes);
    }
    Ok(splice.finish())
}

/// A relationship to add.
pub(super) struct NewRelationship<'a> {
    pub(super) id: &'a str,
    pub(super) kind: &'a str,
    pub(super) target: &'a str,
}

/// A relationships part without its relationships of type `remove`, and with `add` appended.
pub(super) fn relationships(
    xml: &XmlPart,
    remove: Option<&str>,
    add: &[NewRelationship<'_>],
) -> Result<Vec<u8>, SaveRefusal> {
    let source = xml.bytes();
    let mut reader = Reader::new(xml);
    let root = match reader.next()? {
        Some((XmlEvent::Start(root), span))
            if root.name.is(RELATIONSHIPS_NAMESPACE, "Relationships") =>
        {
            span
        }
        _ => return Err(reader.invalid("the document element is not Relationships")),
    };
    let root_tag = RawTag::parse(&source[root.0..root.1]);
    let prefix = root_tag.prefix().to_vec();
    let mut splice = Splice::new(source);
    let mut added = Vec::new();
    for add in add {
        added.push(b'<');
        added.extend_from_slice(&prefix);
        added.extend_from_slice(b"Relationship Id=");
        attribute_value(add.id, &mut added);
        added.extend_from_slice(b" Type=");
        attribute_value(add.kind, &mut added);
        added.extend_from_slice(b" Target=");
        attribute_value(add.target, &mut added);
        added.extend_from_slice(b"/>");
    }
    if root_tag.is_empty() {
        reader.expect()?;
        if !add.is_empty() {
            let mut bytes = root_tag.rebuilt(&[], false);
            bytes.extend_from_slice(&added);
            bytes.extend_from_slice(b"</");
            bytes.extend_from_slice(&prefix);
            bytes.extend_from_slice(b"Relationships>");
            splice.replace(root, &bytes);
        }
    } else {
        loop {
            match reader.expect()? {
                (XmlEvent::Start(element), span) => {
                    let end = reader.element_end()?;
                    if remove.is_some() && element.attribute("Type") == remove {
                        splice.keep_to(span.0);
                        splice.skip_to(end);
                    }
                }
                (XmlEvent::Text(_), _) => {}
                (XmlEvent::End(_), (start, _)) => {
                    splice.keep_to(start);
                    splice.put(&added);
                    break;
                }
            }
        }
    }
    reader.end()?;
    Ok(splice.finish())
}

/// `[Content_Types].xml` without the `Override` of the part `remove` (a ZIP item name, compared
/// ignoring ASCII case), and with an `Override` giving each part `add[i].0` the content type
/// `add[i].1`.
pub(super) fn content_types(
    xml: &XmlPart,
    remove: Option<&str>,
    add: &[(&str, &str)],
) -> Result<Vec<u8>, SaveRefusal> {
    let source = xml.bytes();
    let mut reader = Reader::new(xml);
    let root = match reader.next()? {
        Some((XmlEvent::Start(root), span)) if root.name.is(CONTENT_TYPES_NAMESPACE, "Types") => {
            span
        }
        _ => return Err(reader.invalid("the document element is not Types")),
    };
    let root_tag = RawTag::parse(&source[root.0..root.1]);
    let prefix = root_tag.prefix().to_vec();
    let mut added = Vec::new();
    for (part, content_type) in add {
        added.push(b'<');
        added.extend_from_slice(&prefix);
        added.extend_from_slice(b"Override PartName=");
        attribute_value(&format!("/{part}"), &mut added);
        added.extend_from_slice(b" ContentType=");
        attribute_value(content_type, &mut added);
        added.extend_from_slice(b"/>");
    }
    let mut splice = Splice::new(source);
    if root_tag.is_empty() {
        return Err(reader.invalid("a Types element without content types"));
    }
    loop {
        match reader.expect()? {
            (XmlEvent::Start(element), span) => {
                let end = reader.element_end()?;
                let named = element
                    .attribute("PartName")
                    .and_then(|name| name.strip_prefix('/'));
                if element.name.is(CONTENT_TYPES_NAMESPACE, "Override")
                    && let (Some(name), Some(remove)) = (named, remove)
                    && name.eq_ignore_ascii_case(remove)
                {
                    splice.keep_to(span.0);
                    splice.skip_to(end);
                }
            }
            (XmlEvent::Text(_), _) => {}
            (XmlEvent::End(_), (start, _)) => {
                splice.keep_to(start);
                splice.put(&added);
                break;
            }
        }
    }
    reader.end()?;
    Ok(splice.finish())
}
