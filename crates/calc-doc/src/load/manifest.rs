//! The DNA view manifest (plan section 8.2): finding it among the package's custom XML parts,
//! reading it, and resolving each block's locator into its anchor.
//!
//! The manifest is one custom XML item part, reached through a relationship of type `customXml`
//! from any part (a save adds it to the workbook part's relationships), with its item properties
//! part beside it. Its document element is `manifest` in the namespace [`MANIFEST_NAMESPACE`],
//! with a `version` attribute. Version 1, the only version this codec reads, is:
//!
//! ```text
//! <manifest xmlns="urn:dnacalc:view-manifest" version="1">
//!   <blocks>
//!     <block id="<uuid>" kind="prose|calculation" locator="<defined name>"/>
//!   </blocks>
//!   <views>
//!     <notebook><placement id="<uuid>" block="<uuid>"/></notebook>
//!     <tree><placement id="<uuid>" block="<uuid>" parent="<uuid>"/></tree>
//!     <canvas><placement id="<uuid>" block="<uuid>" x="<n>" y="<n>" width="<n>" height="<n>"/></canvas>
//!     <grid><placement id="<uuid>" block="<uuid>"/></grid>
//!   </views>
//! </manifest>
//! ```
//!
//! A block's anchor is not in the manifest: it is what the block's locator refers to. A locator is
//! a hidden (`hidden="1"`), workbook-scoped defined name of the workbook part whose name is
//! `_dna.block.` followed by 32 hex digits ([`is_locator_form`]); no other defined name is ever a
//! locator, so a manifest that names a user's name (`Rate`) as a block's locator leaves the block
//! without an anchor and never makes a layout save change that name. The manifest holds no cell
//! content and no calculated value. Every other element and attribute is an unknown field of
//! version 1: it is not read, and a save that rewrites the manifest keeps it (`save::manifest`).
//! Text other than whitespace directly inside a known element, a second `blocks`, `views` or view
//! element, a known attribute with an invalid value, or a layout that breaks a rule of
//! [`calc_doc_model::SkinLayout`] makes the manifest unreadable.
//!
//! A custom XML item is a DNA manifest when its document element is `manifest` in the manifest's
//! namespace. An item the XML reader refuses before its document element (UTF-16, an encoding
//! declaration other than UTF-8, a malformed start tag) is still counted as a DNA manifest, one
//! that cannot be read, when its bytes hold the namespace's URI as UTF-8 (or any encoding that
//! writes ASCII as ASCII), UTF-16 or UTF-32 of either byte order; so is an item whose document
//! element is another element of the manifest's namespace, and an item whose content cannot be
//! read at all. A manifest whose namespace URI is written only with character references is not
//! recognized in an item the reader refuses.

use crate::opc::{Package, TargetMode};
use crate::xml::{XmlElement, XmlError, XmlEvent, XmlPart};
use calc_doc_model::value::{CellKey, GridArea, SheetId};
use calc_doc_model::{
    Block, BlockId, BlockKind, CanvasArea, FiniteNumber, LocatorName, Placement, PlacementId,
    SheetName, SkinLayout, ViewKind,
};
use std::collections::BTreeSet;
use std::fmt;

/// The namespace of the manifest's elements.
pub(crate) const MANIFEST_NAMESPACE: &str = "urn:dnacalc:view-manifest";
/// The manifest version this codec reads and writes.
pub(crate) const MANIFEST_VERSION: &str = "1";
pub(crate) const CUSTOM_XML: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml";
pub(crate) const CUSTOM_XML_PROPS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXmlProps";

/// Why a workbook loaded without its views, or a block without its anchor. Nothing is deleted
/// and no layout is invented: the manifest's parts and every defined name stay in the package as
/// they are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutWarning {
    /// The DNA manifest at `part` declares a version this codec does not read (`version`, `None`
    /// when it declares none). The model's layout is empty, and a save that changes the layout is
    /// refused ([`crate::SaveRefusal::LayoutNotWritable`]).
    UnsupportedVersion {
        part: String,
        version: Option<String>,
    },
    /// The DNA manifest at `part` is version 1 but could not be read; or the XML reader refused
    /// the item before its document element (it names the manifest's namespace, so its version is
    /// not known), its document element is another element of the manifest's namespace, or its
    /// content could not be read; or the package holds more than one DNA manifest (`part` is the
    /// first). `detail` says which. The model's layout is empty, and a save that changes the
    /// layout is refused.
    UnreadableManifest { part: String, detail: String },
    /// The block's locator is not a locator the workbook part holds once (a hidden, workbook-scoped
    /// defined name of a locator's form, `_dna.block.` and 32 hex digits), or does not refer to one
    /// cell or range of one of the workbook's sheets. The block is loaded without an anchor, and
    /// a layout save changes no defined name for it unless the name is such a locator.
    UnresolvedLocator {
        block: BlockId,
        locator: LocatorName,
    },
}

impl fmt::Display for LayoutWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion { part, version } => write!(
                f,
                "the view manifest {part:?} has the version {version:?}, which is not read; its views were not loaded"
            ),
            Self::UnreadableManifest { part, detail } => write!(
                f,
                "the view manifest {part:?} could not be read ({detail}); its views were not loaded"
            ),
            Self::UnresolvedLocator { block, locator } => write!(
                f,
                "the locator {:?} of the block {block} does not resolve to a cell or range; the block has no anchor",
                locator.as_str()
            ),
        }
    }
}

/// Whether and how the package's DNA manifest was read, which a save of the layout needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ManifestState {
    /// The package holds no DNA manifest.
    Absent,
    /// The manifest at this part was read into the model's layout.
    Read { part: String },
    /// A manifest at this part was not read: its version is not supported, it is unreadable, or
    /// it is one of several.
    Unread { part: String },
}

/// One element of a part: the element, the byte ranges of its start tag and of its end tag (an
/// empty range just past the start tag for `<a/>`), its child elements, and whether it holds text
/// other than whitespace directly.
pub(crate) struct Node {
    pub(crate) element: XmlElement,
    pub(crate) start_tag: (usize, usize),
    pub(crate) end_tag: (usize, usize),
    pub(crate) children: Vec<Node>,
    pub(crate) text: bool,
}

impl Node {
    /// The byte range of the whole element.
    pub(crate) fn whole(&self) -> (usize, usize) {
        (self.start_tag.0, self.end_tag.1)
    }

    /// Whether the element is `local` in the manifest's namespace.
    pub(crate) fn is(&self, local: &str) -> bool {
        self.element.name.is(MANIFEST_NAMESPACE, local)
    }
}

/// The element tree of a whole part, each element with its byte ranges.
pub(crate) fn tree(xml: &XmlPart) -> Result<Node, XmlError> {
    let mut reader = xml.reader();
    let mut open: Vec<Node> = Vec::new();
    let mut root = None;
    while let Some(event) = reader.next_event()? {
        match event {
            XmlEvent::Start(element) => open.push(Node {
                element,
                start_tag: reader.span(),
                end_tag: (0, 0),
                children: Vec::new(),
                text: false,
            }),
            XmlEvent::Text(text) => {
                if !text.trim().is_empty()
                    && let Some(node) = open.last_mut()
                {
                    node.text = true;
                }
            }
            XmlEvent::End(_) => {
                let mut node = open.pop().expect("the reader pairs every end with a start");
                node.end_tag = reader.span();
                match open.last_mut() {
                    Some(parent) => parent.children.push(node),
                    None => root = Some(node),
                }
            }
        }
    }
    Ok(root.expect("the reader refuses a part without a document element"))
}

/// The document element of a part, read without reading further; `None` when the part is not XML
/// the reader accepts.
fn document_element(xml: &XmlPart) -> Option<XmlElement> {
    match xml.reader().next_event() {
        Ok(Some(XmlEvent::Start(element))) => Some(element),
        _ => None,
    }
}

/// Why a manifest was not read.
pub(crate) enum ManifestProblem {
    UnsupportedVersion(Option<String>),
    Unreadable(String),
}

/// A version 1 manifest as read: its element tree, its blocks (ID, kind and locator, without
/// anchors) and its placements, each view's in its order.
pub(crate) struct ManifestDocument {
    pub(crate) root: Node,
    pub(crate) blocks: Vec<(BlockId, BlockKind, LocatorName)>,
    pub(crate) placements: Vec<Placement>,
}

fn unreadable(detail: impl Into<String>) -> ManifestProblem {
    ManifestProblem::Unreadable(detail.into())
}

/// The value of the required attribute `name`, read by `parse`.
fn required<T>(
    node: &Node,
    name: &str,
    parse: impl FnOnce(&str) -> Option<T>,
) -> Result<T, ManifestProblem> {
    let value = node.element.attribute(name).ok_or_else(|| {
        unreadable(format!(
            "a {} without a {name} attribute",
            node.element.name.local
        ))
    })?;
    parse(value).ok_or_else(|| {
        unreadable(format!(
            "a {} whose {name} {value:?} is not admitted",
            node.element.name.local
        ))
    })
}

/// A world coordinate or size: a decimal number, finite.
fn number(text: &str) -> Option<FiniteNumber> {
    text.parse::<f64>().ok().and_then(FiniteNumber::new)
}

/// Reads a manifest part whose document element is the manifest's. Its version is judged first,
/// before the rest of the part is read, so a manifest of another version is reported as such
/// whatever it holds.
pub(crate) fn read_manifest(xml: &XmlPart) -> Result<ManifestDocument, ManifestProblem> {
    let element = document_element(xml)
        .ok_or_else(|| unreadable("the part is not XML the reader accepts"))?;
    match element.attribute("version") {
        Some(MANIFEST_VERSION) => {}
        version => {
            return Err(ManifestProblem::UnsupportedVersion(
                version.map(str::to_owned),
            ));
        }
    }
    let root = tree(xml).map_err(|error| unreadable(error.to_string()))?;
    if root.text {
        return Err(unreadable("text in the manifest element"));
    }
    let mut blocks = Vec::new();
    let mut placements = Vec::new();
    let (mut seen_blocks, mut seen_views) = (false, false);
    for child in &root.children {
        if child.is("blocks") {
            if std::mem::replace(&mut seen_blocks, true) {
                return Err(unreadable("two blocks elements"));
            }
            if child.text {
                return Err(unreadable("text in the blocks element"));
            }
            for block in child.children.iter().filter(|node| node.is("block")) {
                blocks.push((
                    required(block, "id", BlockId::parse)?,
                    required(block, "kind", BlockKind::from_name)?,
                    required(block, "locator", LocatorName::new)?,
                ));
            }
        } else if child.is("views") {
            if std::mem::replace(&mut seen_views, true) {
                return Err(unreadable("two views elements"));
            }
            if child.text {
                return Err(unreadable("text in the views element"));
            }
            let mut views_seen = BTreeSet::new();
            for view_node in &child.children {
                let Some(view) = ViewKind::ALL
                    .into_iter()
                    .find(|view| view_node.is(view.name()))
                else {
                    continue;
                };
                if !views_seen.insert(view) {
                    return Err(unreadable(format!("two {} elements", view.name())));
                }
                if view_node.text {
                    return Err(unreadable(format!("text in the {} element", view.name())));
                }
                for node in view_node
                    .children
                    .iter()
                    .filter(|node| node.is("placement"))
                {
                    placements.push(placement(node, view)?);
                }
            }
        }
    }
    Ok(ManifestDocument {
        root,
        blocks,
        placements,
    })
}

/// One `placement` element of the view `view`.
fn placement(node: &Node, view: ViewKind) -> Result<Placement, ManifestProblem> {
    let id = required(node, "id", PlacementId::parse)?;
    let block = required(node, "block", BlockId::parse)?;
    Ok(match view {
        ViewKind::Notebook => Placement::notebook(id, block),
        ViewKind::Tree => {
            let parent = match node.element.attribute("parent") {
                None => None,
                Some(_) => Some(required(node, "parent", PlacementId::parse)?),
            };
            Placement::tree(id, block, parent)
        }
        ViewKind::Canvas => Placement::canvas(
            id,
            block,
            CanvasArea {
                x: required(node, "x", number)?,
                y: required(node, "y", number)?,
                width: required(node, "width", number)?,
                height: required(node, "height", number)?,
            },
        ),
        ViewKind::Grid => Placement::grid(id, block),
    })
}

/// One defined name of the workbook part, as [`super::load`] read it: its name, whether it is
/// scoped to a sheet (`localSheetId`), whether it is hidden (`hidden` is `1` or `true`), and its
/// formula text with its `_xHHHH_` escapes decoded.
pub(crate) struct DefinedName {
    pub(crate) name: String,
    pub(crate) sheet_scoped: bool,
    pub(crate) hidden: bool,
    pub(crate) text: String,
    /// `localSheetId`, when it is a number: the 0-based position of the sheet in workbook order.
    pub(crate) local_sheet: Option<u32>,
    pub(crate) comment: Option<String>,
    pub(crate) function: bool,
    pub(crate) vb_procedure: bool,
    pub(crate) xlm: bool,
}

/// Whether `name` has a locator's form: `_dna.block.` followed by 32 hex digits, compared ignoring
/// ASCII case as defined names compare. Every locator name of a block this package creates
/// ([`calc_doc_model::LocatorName::for_block`]) has it.
pub(crate) fn is_locator_form(name: &str) -> bool {
    let prefix = LocatorName::PREFIX;
    name.len() == prefix.len() + 32
        && name.is_char_boundary(prefix.len())
        && name[..prefix.len()].eq_ignore_ascii_case(prefix)
        && name[prefix.len()..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
}

/// Whether an `xsd:boolean` attribute value is true (`1` or `true`, surrounding whitespace
/// collapsed as the type's facet does).
pub(crate) fn is_true(value: Option<&str>) -> bool {
    matches!(value.map(str::trim), Some("1" | "true"))
}

/// The cell `$A$1`, `A1`, `$A1` or `A$1`, as (row, column).
fn reference_cell(text: &str) -> Option<(u32, u32)> {
    let text = text.strip_prefix('$').unwrap_or(text);
    let letters = text.bytes().take_while(u8::is_ascii_alphabetic).count();
    let (column, rest) = text.split_at(letters);
    let row = rest.strip_prefix('$').unwrap_or(rest);
    Some((
        super::address::row_number(row)?,
        super::address::column_number(column)?,
    ))
}

/// The area a locator's formula text refers to: `Sheet!$A$1` or `'Sheet name'!$A$1:$C$4` (an
/// apostrophe in a quoted name doubled), on one of `sheets`, whose names compare ignoring ASCII
/// case. `None` for any other text, `#REF!` included.
pub(crate) fn resolve_locator(text: &str, sheets: &[(SheetId, &SheetName)]) -> Option<GridArea> {
    let (name, reference) = match text.strip_prefix('\'') {
        Some(quoted) => {
            let mut name = String::new();
            let mut chars = quoted.char_indices();
            let rest = loop {
                let (at, c) = chars.next()?;
                if c != '\'' {
                    name.push(c);
                    continue;
                }
                if quoted[at + 1..].starts_with('\'') {
                    name.push('\'');
                    chars.next();
                    continue;
                }
                break &quoted[at + 1..];
            };
            (name, rest.strip_prefix('!')?)
        }
        None => {
            let (name, reference) = text.split_once('!')?;
            (name.to_owned(), reference)
        }
    };
    let (first, last) = match reference.split_once(':') {
        Some((first, last)) => (reference_cell(first)?, reference_cell(last)?),
        None => {
            let only = reference_cell(reference)?;
            (only, only)
        }
    };
    let (sheet, _) = sheets
        .iter()
        .find(|(_, sheet)| sheet.text().to_string_lossy().eq_ignore_ascii_case(&name))?;
    let first = CellKey::new(*sheet, first.0, first.1).ok()?;
    let last = CellKey::new(*sheet, last.0, last.1).ok()?;
    GridArea::new(first, last).ok()
}

/// What [`load_layout`] found.
pub(crate) struct LoadedLayout {
    pub(crate) layout: SkinLayout,
    pub(crate) state: ManifestState,
    pub(crate) warnings: Vec<LayoutWarning>,
    /// The number of defined names that became a block's anchor.
    pub(crate) locators_resolved: usize,
    /// The workbook-scoped defined names that no layout save removes: every one but the hidden
    /// names of a locator's form that are the locator of a block of the loaded layout. A block a
    /// save creates cannot take the name of one of them.
    pub(crate) fixed_names: Vec<String>,
}

/// Whether `bytes` hold the manifest's namespace URI as UTF-8 (or any encoding that writes ASCII
/// as ASCII), as UTF-16 or as UTF-32, of either byte order.
fn names_manifest_namespace(bytes: &[u8]) -> bool {
    let ascii = MANIFEST_NAMESPACE.as_bytes();
    let widened = |width: usize, big_endian: bool| -> Vec<u8> {
        let mut out = Vec::with_capacity(ascii.len() * width);
        for &byte in ascii {
            let mut unit = vec![0u8; width];
            unit[if big_endian { width - 1 } else { 0 }] = byte;
            out.extend(unit);
        }
        out
    };
    let needles = [
        ascii.to_vec(),
        widened(2, false),
        widened(2, true),
        widened(4, false),
        widened(4, true),
    ];
    needles.iter().any(|needle| {
        bytes
            .windows(needle.len())
            .any(|window| window == needle.as_slice())
    })
}

/// Whether the custom XML item `part` is a DNA manifest, as the module describes: `None` when it is
/// not, `Some(None)` for one whose document element was read, and `Some(Some(detail))` for one
/// that cannot be read, with the reason.
fn manifest_item(package: &Package, part: &str) -> Option<Option<String>> {
    let xml = match package.read_xml(part) {
        Ok(xml) => xml,
        Err(error) => {
            return Some(Some(format!(
                "the part could not be read ({error}), so whether it is a DNA manifest is not known"
            )));
        }
    };
    match xml.reader().next_event() {
        Ok(Some(XmlEvent::Start(element))) if element.name.is(MANIFEST_NAMESPACE, "manifest") => {
            Some(None)
        }
        Ok(Some(XmlEvent::Start(element)))
            if element.name.namespace.as_deref() == Some(MANIFEST_NAMESPACE) =>
        {
            Some(Some(format!(
                "its document element is {:?} of the manifest's namespace, not \"manifest\"",
                element.name.local
            )))
        }
        Ok(_) => None,
        Err(_) if !names_manifest_namespace(xml.bytes()) => None,
        Err(error) => Some(Some(format!(
            "the XML reader refused the part ({error}), and the part names the manifest's namespace"
        ))),
    }
}

/// The custom XML item parts that the package's relationships reach and that are DNA manifests
/// ([`manifest_item`]), each once, in the order the relationships parts list them, each with the
/// reason it cannot be read, if it cannot.
fn manifest_parts(package: &Package) -> Vec<(String, Option<String>)> {
    let mut seen: Vec<String> = Vec::new();
    let mut found = Vec::new();
    for relationships in package.relationships() {
        for item in relationships.items() {
            if item.kind != CUSTOM_XML || item.mode != TargetMode::Internal {
                continue;
            }
            let Some(part) = item.resolved.as_deref().and_then(|name| package.part(name)) else {
                continue;
            };
            if seen
                .iter()
                .any(|name| name.eq_ignore_ascii_case(part.name()))
            {
                continue;
            }
            seen.push(part.name().to_owned());
            if let Some(problem) = manifest_item(package, part.name()) {
                found.push((part.name().to_owned(), problem));
            }
        }
    }
    found
}

/// The workbook-scoped names among `names` that a layout save keeps whatever it removes: every
/// one but the hidden names of a locator's form that are the locator of one of `layout`'s blocks.
fn fixed_names(names: &[DefinedName], layout: &SkinLayout) -> Vec<String> {
    names
        .iter()
        .filter(|name| !name.sheet_scoped)
        .filter(|name| {
            !(name.hidden
                && is_locator_form(&name.name)
                && layout
                    .blocks()
                    .iter()
                    .any(|block| block.locator().is(&name.name)))
        })
        .map(|name| name.name.clone())
        .collect()
}

/// Finds and reads the package's DNA manifest and resolves each block's locator among `names`
/// against `sheets`. A manifest that is not read, and a locator that does not resolve, is a
/// warning, never a refusal of the load.
pub(crate) fn load_layout(
    package: &Package,
    sheets: &[(SheetId, &SheetName)],
    names: &[DefinedName],
) -> LoadedLayout {
    let unread = |part: &str, warning: LayoutWarning| LoadedLayout {
        layout: SkinLayout::default(),
        state: ManifestState::Unread {
            part: part.to_owned(),
        },
        warnings: vec![warning],
        locators_resolved: 0,
        fixed_names: fixed_names(names, &SkinLayout::default()),
    };
    let parts = manifest_parts(package);
    let part = match parts.as_slice() {
        [] => {
            return LoadedLayout {
                layout: SkinLayout::default(),
                state: ManifestState::Absent,
                warnings: Vec::new(),
                locators_resolved: 0,
                fixed_names: fixed_names(names, &SkinLayout::default()),
            };
        }
        [(part, None)] => part,
        [(part, Some(detail))] => {
            return unread(
                part,
                LayoutWarning::UnreadableManifest {
                    part: part.clone(),
                    detail: detail.clone(),
                },
            );
        }
        [(first, _), ..] => {
            let found: Vec<&str> = parts.iter().map(|(part, _)| part.as_str()).collect();
            return unread(
                first,
                LayoutWarning::UnreadableManifest {
                    part: first.clone(),
                    detail: format!("the package holds {} DNA manifests: {found:?}", found.len()),
                },
            );
        }
    };
    let document = package
        .read_xml(part)
        .map_err(|error| unreadable(error.to_string()))
        .and_then(|xml| read_manifest(&xml));
    let document = match document {
        Ok(document) => document,
        Err(ManifestProblem::UnsupportedVersion(version)) => {
            return unread(
                part,
                LayoutWarning::UnsupportedVersion {
                    part: part.clone(),
                    version,
                },
            );
        }
        Err(ManifestProblem::Unreadable(detail)) => {
            return unread(
                part,
                LayoutWarning::UnreadableManifest {
                    part: part.clone(),
                    detail,
                },
            );
        }
    };
    let mut warnings = Vec::new();
    let mut locators_resolved = 0;
    let blocks = document
        .blocks
        .into_iter()
        .map(|(id, kind, locator)| {
            let mut matching = names
                .iter()
                .filter(|name| !name.sheet_scoped && locator.is(&name.name));
            let anchor = match (matching.next(), matching.next()) {
                (Some(name), None) if name.hidden && is_locator_form(&name.name) => {
                    resolve_locator(&name.text, sheets)
                }
                _ => None,
            };
            match anchor {
                Some(_) => locators_resolved += 1,
                None => warnings.push(LayoutWarning::UnresolvedLocator {
                    block: id,
                    locator: locator.clone(),
                }),
            }
            Block::new(id, kind, locator, anchor)
        })
        .collect();
    match SkinLayout::new(blocks, document.placements) {
        Ok(layout) => LoadedLayout {
            fixed_names: fixed_names(names, &layout),
            layout,
            state: ManifestState::Read { part: part.clone() },
            warnings,
            locators_resolved,
        },
        Err(error) => unread(
            part,
            LayoutWarning::UnreadableManifest {
                part: part.clone(),
                detail: error.to_string(),
            },
        ),
    }
}
