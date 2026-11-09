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
    /// Modelled source records removed, addressed by scoped occurrence.
    pub(super) record_removed: Vec<NameTarget>,
    pub(super) replaced: Vec<NameWrite>,
    pub(super) defined: Vec<NameWrite>,
    pub(super) order: Option<Vec<NamePlacement>>,
}

pub(super) enum NamePlacement {
    Source(NameTarget),
    Defined(usize),
}
#[derive(Clone)]
pub(super) struct NameTarget {
    pub(super) name: String,
    pub(super) sheet: Option<u32>,
    pub(super) occurrence: usize,
}
impl NameTarget {
    fn key(&self) -> (String, Option<u32>, usize) {
        (
            calc_doc_model::value::fold_workbook_name(&self.name),
            self.sheet,
            self.occurrence,
        )
    }
}
pub(super) struct NameWrite {
    pub(super) target: NameTarget,
    pub(super) sheet: Option<u32>,
    pub(super) definition: calc_doc_model::DefinedName,
    pub(super) attributes_changed: bool,
}

fn name_attributes<'a>(
    definition: &'a calc_doc_model::DefinedName,
    sheet: Option<&'a str>,
) -> [(&'static str, Option<&'a str>); 7] {
    [
        ("name", Some(definition.name.as_str())),
        ("localSheetId", sheet),
        ("hidden", definition.hidden.then_some("1")),
        ("comment", definition.comment.as_deref()),
        ("function", definition.function.then_some("1")),
        ("vbProcedure", definition.vb_procedure.then_some("1")),
        ("xlm", definition.xlm.then_some("1")),
    ]
}

fn new_name(write: &NameWrite, prefix: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(b'<');
    out.extend_from_slice(prefix);
    out.extend_from_slice(b"definedName");
    let sheet = write.sheet.map(|sheet| sheet.to_string());
    for (name, value) in name_attributes(&write.definition, sheet.as_deref()) {
        if let Some(value) = value {
            out.push(b' ');
            out.extend_from_slice(name.as_bytes());
            out.push(b'=');
            attribute_value(value, &mut out);
        }
    }
    out.push(b'>');
    xstring(
        &write.definition.formula.encode_utf16().collect::<Vec<_>>(),
        &mut out,
    );
    out.extend_from_slice(b"</");
    out.extend_from_slice(prefix);
    out.extend_from_slice(b"definedName>");
    out
}
fn rewritten_name(
    source: &[u8],
    range: (usize, usize),
    scope: &NameScope,
    write: Option<&NameWrite>,
) -> Vec<u8> {
    let Some(write) = write else {
        return source[range.0..range.1].to_vec();
    };
    let tag = RawTag::parse(&source[scope.tag.0..scope.tag.1]);
    let sheet = write.sheet.map(|sheet| sheet.to_string());
    let attributes = name_attributes(&write.definition, sheet.as_deref());
    let changes = if write.attributes_changed {
        attributes.as_slice()
    } else {
        &[]
    };
    let mut out = if tag.is_empty() || write.attributes_changed {
        tag.rebuilt(changes, false)
    } else {
        source[scope.tag.0..scope.tag.1].to_vec()
    };
    xstring(
        &write.definition.formula.encode_utf16().collect::<Vec<_>>(),
        &mut out,
    );
    if tag.is_empty() {
        out.extend_from_slice(b"</");
        out.extend_from_slice(tag.prefix());
        out.extend_from_slice(b"definedName>");
    } else {
        let end = source[..range.1]
            .windows(2)
            .rposition(|window| window == b"</")
            .expect("a nonempty element has an end tag");
        out.extend_from_slice(&source[end..range.1]);
    }
    out
}

impl NameChanges {
    pub(super) fn is_empty(&self) -> bool {
        self.removed.is_empty()
            && self.added.is_empty()
            && self.record_removed.is_empty()
            && self.replaced.is_empty()
            && self.defined.is_empty()
            && self.order.is_none()
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
/// is measured, tests/fixtures/d1/excel-open.json), and with `iteration`'s settings when it is
/// given (dc-y2o.2: `iterate="1"` when on, `iterateCount` and `iterateDelta` when they are not
/// Excel's defaults of 100 and 0.001, each attribute left out otherwise, as Excel writes them), set
/// on the `calcPr` the part holds, its other attributes kept, or a new `calcPr` at the place the
/// schema gives it; and with `names` applied
/// to its `definedNames`: each removed name's hidden, workbook-scoped `definedName` left out (and
/// the element with it when nothing else remains in it), and each added name appended as a hidden,
/// workbook-scoped `definedName`, in a new `definedNames` at the place the schema gives it when
/// the part has none. Every other byte is the part's own. Refused when the part holds two
/// `calcPr` or two `definedNames` elements, and when an added name is already a workbook-scoped
/// defined name the part keeps ([`SaveRefusal::LocatorNameTaken`]).
pub(super) fn workbook(
    xml: &XmlPart,
    full_calc: bool,
    iteration: Option<calc_doc_model::IterationSettings>,
    names: &NameChanges,
    cache_state: Option<&super::CalculationCacheState>,
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
        let held = defined
            .as_ref()
            .map(|d| d.children.as_slice())
            .unwrap_or_default();
        let mut occurrences = std::collections::HashMap::new();
        let targets: Vec<_> = held
            .iter()
            .map(|(_, scope)| {
                scope.as_ref().map(|scope| {
                    let key = (
                        calc_doc_model::value::fold_workbook_name(&scope.name),
                        scope.local_sheet,
                    );
                    let occurrence = occurrences.entry(key.clone()).or_insert(0usize);
                    let target = (key.0, key.1, *occurrence);
                    *occurrence += 1;
                    target
                })
            })
            .collect();
        let removed: std::collections::HashSet<_> =
            names.record_removed.iter().map(NameTarget::key).collect();
        let replacements: std::collections::HashMap<_, _> = names
            .replaced
            .iter()
            .map(|write| (write.target.key(), write))
            .collect();
        let dropped = |index: usize, child: &Option<NameScope>| {
            targets[index]
                .as_ref()
                .is_some_and(|key| removed.contains(key))
                || child.as_ref().is_some_and(|scope| {
                    scope.workbook_scoped
                        && scope.hidden
                        && names
                            .removed
                            .iter()
                            .any(|name| name.eq_ignore_ascii_case(&scope.name))
                })
        };
        for (added, _) in &names.added {
            let taken = held.iter().enumerate().any(|(index, (_, child))| {
                !dropped(index, child)
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
        for write in names.defined.iter().filter(|_| names.order.is_none()) {
            added.extend_from_slice(&new_name(write, &element_prefix));
        }
        let close = |out: &mut Vec<u8>| {
            out.extend_from_slice(b"</");
            out.extend_from_slice(&element_prefix);
            out.extend_from_slice(b"definedNames>");
        };
        if let Some(order) = &names.order {
            let sources: std::collections::HashMap<_, _> = targets
                .iter()
                .enumerate()
                .filter_map(|(index, key)| key.as_ref().map(|key| (key.clone(), index)))
                .collect();
            let mut used = std::collections::HashSet::new();
            let mut body = Vec::new();
            for placement in order {
                match placement {
                    NamePlacement::Source(target) => {
                        let key = target.key();
                        let Some(&index) = sources.get(&key) else {
                            return Err(reader.invalid(
                                "a retained name record is missing from the workbook part",
                            ));
                        };
                        used.insert(index);
                        if dropped(index, &held[index].1) {
                            continue;
                        }
                        let (range, scope) = &held[index];
                        let scope = scope.as_ref().expect("a source name has a parsed scope");
                        body.extend_from_slice(&rewritten_name(
                            source,
                            *range,
                            scope,
                            replacements.get(&key).copied(),
                        ));
                    }
                    NamePlacement::Defined(index) => {
                        body.extend_from_slice(&new_name(&names.defined[*index], &element_prefix))
                    }
                }
            }
            for (index, (range, child)) in held.iter().enumerate() {
                if !used.contains(&index) && !dropped(index, child) {
                    body.extend_from_slice(&source[range.0..range.1]);
                }
            }
            body.extend_from_slice(&added);
            match &defined {
                Some(defined) => {
                    let mut bytes = Vec::new();
                    if !body.is_empty() {
                        bytes = RawTag::parse(&source[defined.start_tag.0..defined.start_tag.1])
                            .rebuilt(&[], false);
                        bytes.extend_from_slice(&body);
                        close(&mut bytes);
                    }
                    changes.push(((defined.start_tag.0, defined.end_tag.1), 0, bytes));
                }
                None if !body.is_empty() => {
                    let mut bytes = Vec::new();
                    bytes.push(b'<');
                    bytes.extend_from_slice(&element_prefix);
                    bytes.extend_from_slice(b"definedNames>");
                    bytes.extend_from_slice(&body);
                    close(&mut bytes);
                    let at = insert_names_at.expect("the root end is a place for definedNames");
                    changes.push(((at, at), 0, bytes));
                }
                None => {}
            }
        } else {
            match &defined {
                Some(defined) => {
                    let remaining = defined
                        .children
                        .iter()
                        .enumerate()
                        .filter(|(index, (_, c))| !dropped(*index, c))
                        .count();
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
                        for (index, (range, child)) in defined.children.iter().enumerate() {
                            if dropped(index, child) {
                                changes.push((*range, 0, Vec::new()));
                                continue;
                            }
                            let Some(scope) = child else {
                                continue;
                            };
                            let Some(write) = targets[index]
                                .as_ref()
                                .and_then(|key| replacements.get(key))
                            else {
                                continue;
                            };
                            changes.push((
                                *range,
                                0,
                                rewritten_name(source, *range, scope, Some(write)),
                            ));
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
    }
    let mut calc_attributes: Vec<(&str, Option<String>)> = Vec::new();
    if let Some(state) = cache_state {
        let mode = match state.mode {
            crate::CalcMode::Manual => "manual",
            crate::CalcMode::Automatic => "auto",
            crate::CalcMode::AutomaticExceptTables => "autoNoTable",
        };
        calc_attributes.push(("calcMode", Some(mode.to_owned())));
        calc_attributes.push((
            "calcCompleted",
            Some(if state.completed { "1" } else { "0" }.to_owned()),
        ));
        calc_attributes.push((
            "calcOnSave",
            Some(
                if state.calculate_before_save {
                    "1"
                } else {
                    "0"
                }
                .to_owned(),
            ),
        ));
    }
    if full_calc {
        calc_attributes.push(("fullCalcOnLoad", Some("1".to_owned())));
    }
    if let Some(settings) = iteration {
        let default = calc_doc_model::IterationSettings::EXCEL_DEFAULT;
        calc_attributes.push(("iterate", settings.enabled().then(|| "1".to_owned())));
        calc_attributes.push((
            "iterateCount",
            (settings.max_iterations() != default.max_iterations())
                .then(|| settings.max_iterations().to_string()),
        ));
        calc_attributes.push((
            "iterateDelta",
            (settings.max_change().to_bits() != default.max_change().to_bits())
                .then(|| super::text::number(settings.max_change())),
        ));
    }
    if !calc_attributes.is_empty() {
        let attributes: Vec<(&str, Option<&str>)> = calc_attributes
            .iter()
            .map(|(name, value)| (*name, value.as_deref()))
            .collect();
        match calc_pr {
            Some(span) => {
                let tag = RawTag::parse(&source[span.0..span.1]);
                let empty = tag.is_empty();
                changes.push((span, 1, tag.rebuilt(&attributes, empty)));
            }
            None => {
                let at = insert_calc_at.expect("set when the document element ends");
                let mut element = b"<".to_vec();
                element.extend_from_slice(&prefix);
                element.extend_from_slice(b"calcPr");
                for (name, value) in &attributes {
                    if let Some(value) = value {
                        element.push(b' ');
                        element.extend_from_slice(name.as_bytes());
                        element.extend_from_slice(b"=\"");
                        element.extend_from_slice(value.as_bytes());
                        element.push(b'"');
                    }
                }
                element.extend_from_slice(b"/>");
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

/// Read the latest bytes in this save plan, including parts authored earlier in the same plan.
pub(super) fn current_part(
    package: &crate::opc::Package,
    out: &super::author::Changes,
    name: &str,
    empty: &[u8],
) -> Result<XmlPart, SaveRefusal> {
    if let Some(bytes) = out
        .rewritten
        .get(name)
        .or_else(|| out.added.iter().find(|(n, _)| n == name).map(|(_, b)| b))
    {
        return Ok(XmlPart::new(
            name.into(),
            bytes.clone(),
            crate::xml::XmlLimits::default(),
        ));
    }
    if package.part(name).is_some() {
        super::strings::read_part(package, name)
    } else {
        Ok(XmlPart::new(
            name.into(),
            empty.to_vec(),
            crate::xml::XmlLimits::default(),
        ))
    }
}
pub(super) fn put_part(
    package: &crate::opc::Package,
    out: &mut super::author::Changes,
    name: &str,
    bytes: Vec<u8>,
) {
    if package.part(name).is_some() || out.rewritten.contains_key(name) {
        out.rewritten.insert(name.into(), bytes);
    } else if let Some((_, value)) = out.added.iter_mut().find(|(n, _)| n == name) {
        *value = bytes;
    } else {
        out.added.push((name.into(), bytes));
    }
}
pub(super) fn ensure_relationship(
    package: &crate::opc::Package,
    out: &mut super::author::Changes,
    from: &str,
    kind: &str,
    to: &str,
) -> Result<String, SaveRefusal> {
    if let Some(id) = package
        .relationships_of(Some(from))
        .and_then(|r| {
            r.items().iter().find(|r| {
                r.kind == kind
                    && r.mode == crate::opc::TargetMode::Internal
                    && r.resolved.as_deref() == Some(to)
            })
        })
        .map(|r| r.id.clone())
    {
        return Ok(id);
    }
    let (parent, file) = from.rsplit_once('/').map_or(("", from), |(p, f)| (p, f));
    let name = if parent.is_empty() {
        format!("_rels/{file}.rels")
    } else {
        format!("{parent}/_rels/{file}.rels")
    };
    let empty = format!("<Relationships xmlns=\"{RELATIONSHIPS_NAMESPACE}\"/>");
    let xml = current_part(package, out, &name, empty.as_bytes())?;
    let target = super::author::relative_target(from, to);
    let mut ids = std::collections::BTreeSet::new();
    let mut reader = Reader::new(&xml);
    while let Some((event, _)) = reader.next()? {
        if let XmlEvent::Start(e) = event
            && e.name.is(RELATIONSHIPS_NAMESPACE, "Relationship")
        {
            if e.attribute("Type") == Some(kind)
                && e.attribute("Target") == Some(target.as_str())
                && e.attribute("TargetMode") != Some("External")
            {
                return Ok(e.attribute("Id").unwrap_or_default().into());
            }
            if let Some(id) = e.attribute("Id") {
                ids.insert(id.to_owned());
            }
        }
    }
    let id = (1u32..)
        .map(|n| format!("dnaRelationship{n}"))
        .find(|id| !ids.contains(id))
        .expect("relationship identity");
    let bytes = relationships(
        &xml,
        None,
        &[NewRelationship {
            id: &id,
            kind,
            target: &target,
        }],
    )?;
    put_part(package, out, &name, bytes);
    Ok(id)
}
pub(super) fn ensure_content_type(
    package: &crate::opc::Package,
    out: &mut super::author::Changes,
    name: &str,
    kind: &str,
) -> Result<(), SaveRefusal> {
    let xml = if let Some(bytes) = &out.content_types {
        XmlPart::new(
            "[Content_Types].xml".into(),
            bytes.clone(),
            crate::xml::XmlLimits::default(),
        )
    } else {
        package
            .content_types_xml()
            .map_err(|error| SaveRefusal::Part {
                part: crate::opc::CONTENT_TYPES_ENTRY.into(),
                error,
            })?
    };
    let part = format!("/{name}");
    let mut reader = Reader::new(&xml);
    while let Some((event, _)) = reader.next()? {
        if let XmlEvent::Start(e) = event
            && e.name.is(CONTENT_TYPES_NAMESPACE, "Override")
            && e.attribute("PartName") == Some(part.as_str())
        {
            return if e.attribute("ContentType") == Some(kind) {
                Ok(())
            } else {
                Err(reader.invalid("A new part collides with a different content type"))
            };
        }
    }
    out.content_types = Some(content_types(&xml, None, &[(name, kind)])?);
    Ok(())
}

/// Append to the root without rewriting its other content or namespace declarations.
pub(super) fn append_root(xml: &XmlPart, child: &[u8]) -> Result<Vec<u8>, SaveRefusal> {
    append_root_before(xml, child, &[])
}
pub(super) fn append_root_before(
    xml: &XmlPart,
    child: &[u8],
    before: &[&str],
) -> Result<Vec<u8>, SaveRefusal> {
    let mut reader = Reader::new(xml);
    let mut splice = Splice::new(xml.bytes());
    let (root, span) = match reader.expect()? {
        (XmlEvent::Start(e), s) => (e, s),
        _ => return Err(reader.invalid("Expected an XML document element")),
    };
    let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
    if tag.is_empty() {
        let mut bytes = tag.rebuilt(&[], false);
        bytes.extend(child);
        bytes.extend(
            format!(
                "</{}{}>",
                String::from_utf8_lossy(tag.prefix()),
                root.name.local
            )
            .as_bytes(),
        );
        splice.replace(span, &bytes);
    } else {
        let mut inserted = false;
        loop {
            match reader.expect()? {
                (XmlEvent::Start(e), span) => {
                    if !inserted
                        && e.name.namespace == root.name.namespace
                        && before.contains(&e.name.local.as_str())
                    {
                        splice.replace((span.0, span.0), child);
                        inserted = true;
                    }
                    reader.element_end()?;
                }
                (XmlEvent::End(_), s) => {
                    if !inserted {
                        splice.replace((s.0, s.0), child);
                    }
                    break;
                }
                _ => {}
            }
        }
    }
    Ok(splice.finish())
}
/// Replace a top-level worksheet element, or insert at its schema position.
pub(super) fn worksheet_child(
    xml: &XmlPart,
    local: &str,
    body: &[u8],
    before: &[&str],
) -> Result<Vec<u8>, SaveRefusal> {
    root_child(xml, "worksheet", local, body, before)
}
pub(super) fn root_child(
    xml: &XmlPart,
    root_name: &str,
    local: &str,
    body: &[u8],
    before: &[&str],
) -> Result<Vec<u8>, SaveRefusal> {
    let mut reader = Reader::new(xml);
    let (_, root) = reader.root(root_name)?;
    let tag = RawTag::parse(&xml.bytes()[root.0..root.1]);
    let mut splice = Splice::new(xml.bytes());
    if tag.is_empty() {
        return append_root(xml, body);
    }
    let mut inserted = false;
    loop {
        match reader.expect()? {
            (XmlEvent::Start(e), span) => {
                if e.name.is(super::super::load::MAIN_NAMESPACE, local) {
                    let end = reader.element_end()?;
                    splice.replace((span.0, end), if inserted { &[] } else { body });
                    inserted = true;
                } else {
                    if !inserted
                        && e.name.namespace.as_deref() == Some(super::super::load::MAIN_NAMESPACE)
                        && before.contains(&e.name.local.as_str())
                    {
                        splice.replace((span.0, span.0), body);
                        inserted = true;
                    }
                    reader.element_end()?;
                }
            }
            (XmlEvent::End(_), span) => {
                if !inserted {
                    splice.replace((span.0, span.0), body);
                }
                break;
            }
            _ => {}
        }
    }
    Ok(splice.finish())
}
