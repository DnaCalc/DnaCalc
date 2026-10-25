//! The OPC layer: the part directory, content types and relationships of an opened package.
//!
//! Opening reads `[Content_Types].xml` and every relationships part, because they are the
//! package's structure. Every other part is decompressed only when it is read, and tokenized only
//! as far as its [`XmlReader`](crate::XmlReader) is pulled.

use crate::error::{OpenError, PartReadError};
use crate::policy::OpenPolicy;
use crate::source::PackageRead;
use crate::xml::{XmlEvent, XmlLimits, XmlPart};
use crate::zip::{self, Archive, Entry};
use std::collections::BTreeSet;
use std::sync::Arc;

/// The name of the content types stream, which is an entry but not a part.
pub const CONTENT_TYPES_ENTRY: &str = "[Content_Types].xml";
/// The namespace of `[Content_Types].xml`.
pub const CONTENT_TYPES_NAMESPACE: &str =
    "http://schemas.openxmlformats.org/package/2006/content-types";
/// The namespace of relationships parts.
pub const RELATIONSHIPS_NAMESPACE: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships";

/// One part of the package's part directory.
#[derive(Clone, Debug)]
pub struct Part {
    name: String,
    content_type: Option<String>,
    entry: usize,
    compressed: bool,
    compressed_size: u64,
    size: u64,
    crc32: u32,
}

impl Part {
    /// The part's ZIP item name, which is its OPC part name without the leading `/`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The part's content type, from its override or its extension's default; `None` when
    /// `[Content_Types].xml` gives it none.
    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    /// Whether the part's payload is deflated rather than stored.
    pub fn compressed(&self) -> bool {
        self.compressed
    }

    pub fn compressed_size(&self) -> u64 {
        self.compressed_size
    }

    /// The uncompressed size the archive declares.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// The CRC-32 of the content, as the archive declares it.
    pub fn crc32(&self) -> u32 {
        self.crc32
    }

    /// The index of the part's entry among the archive's entries.
    pub(crate) fn entry(&self) -> usize {
        self.entry
    }
}

/// The content types of `[Content_Types].xml`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContentTypes {
    defaults: Vec<(String, String)>,
    overrides: Vec<(String, String)>,
}

impl ContentTypes {
    /// The `Default` elements, as (extension, content type) in document order.
    pub fn defaults(&self) -> &[(String, String)] {
        &self.defaults
    }

    /// The `Override` elements, as (part name with its leading `/`, content type) in document
    /// order.
    pub fn overrides(&self) -> &[(String, String)] {
        &self.overrides
    }

    /// The content type of the part with ZIP item name `name`: its override, else the default for
    /// its extension. Part names and extensions compare ignoring ASCII case.
    pub fn of(&self, name: &str) -> Option<&str> {
        let overridden = self.overrides.iter().find(|(part, _)| {
            part.strip_prefix('/')
                .is_some_and(|part| part.eq_ignore_ascii_case(name))
        });
        if let Some((_, content_type)) = overridden {
            return Some(content_type);
        }
        let file = name.rsplit('/').next().unwrap_or(name);
        let (_, extension) = file.rsplit_once('.')?;
        self.defaults
            .iter()
            .find(|(default, _)| default.eq_ignore_ascii_case(extension))
            .map(|(_, content_type)| content_type.as_str())
    }
}

/// Whether a relationship's target is a part of this package or a resource outside it. An
/// external target is recorded and never fetched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetMode {
    Internal,
    External,
}

/// One relationship.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relationship {
    pub id: String,
    /// The relationship type URI.
    pub kind: String,
    /// The target as written.
    pub target: String,
    pub mode: TargetMode,
    /// For an internal target, the ZIP item name it resolves to against its source part;
    /// `None` for an external target or one that resolves above the package root.
    pub resolved: Option<String>,
}

/// The relationships of one source: the package itself or one part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relationships {
    part: String,
    source: Option<String>,
    items: Vec<Relationship>,
}

impl Relationships {
    /// The relationships part these came from.
    pub fn part(&self) -> &str {
        &self.part
    }

    /// The source part's ZIP item name, or `None` for the package's own relationships.
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    pub fn items(&self) -> &[Relationship] {
        &self.items
    }
}

/// An opened package: its backing source, its entries as opened, and its part directory.
pub struct Package {
    source: Arc<dyn PackageRead>,
    archive: Archive,
    parts: Vec<Part>,
    content_types: ContentTypes,
    relationships: Vec<Relationships>,
    xml_limits: XmlLimits,
}

/// The source part of a relationships part `<folder>/_rels/<file>.rels`: `<folder>/<file>`, or
/// `None` for `_rels/.rels`. Other names are not relationships parts.
fn relationships_source(name: &str) -> Option<Option<String>> {
    let (folder, file) = match name.rsplit_once('/') {
        Some((folder, file)) => (folder, file),
        None => return None,
    };
    let stem = file.strip_suffix(".rels")?;
    let parent = match folder.rsplit_once('/') {
        Some((parent, "_rels")) => Some(parent),
        None if folder == "_rels" => None,
        _ => return None,
    };
    Some(match parent {
        None if stem.is_empty() => None,
        None => Some(stem.to_owned()),
        Some(parent) => Some(format!("{parent}/{stem}")),
    })
}

/// Resolves an internal relationship target against its source part.
fn resolve_target(source: Option<&str>, target: &str) -> Option<String> {
    let target = target.split(['#', '?']).next().unwrap_or(target);
    let mut segments: Vec<&str> = Vec::new();
    let relative = match target.strip_prefix('/') {
        Some(absolute) => absolute,
        None => {
            if let Some((folder, _)) = source.and_then(|source| source.rsplit_once('/')) {
                segments.extend(folder.split('/'));
            }
            target
        }
    };
    for segment in relative.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            segment => segments.push(segment),
        }
    }
    (!segments.is_empty()).then(|| segments.join("/"))
}

impl std::fmt::Debug for Package {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Package")
            .field("parts", &self.parts.len())
            .field("relationships", &self.relationships.len())
            .finish_non_exhaustive()
    }
}

impl Package {
    /// The file name the package's source is known by ([`PackageRead::name`]).
    pub fn source_name(&self) -> Option<&str> {
        self.source.name()
    }

    pub(crate) fn open(
        source: Arc<dyn PackageRead>,
        policy: &OpenPolicy,
    ) -> Result<Package, OpenError> {
        let archive = zip::read_archive(source.as_ref(), &policy.package)?;
        let mut package = Package {
            source,
            archive,
            parts: Vec::new(),
            content_types: ContentTypes::default(),
            relationships: Vec::new(),
            xml_limits: policy.xml,
        };
        let content_types_entry = package
            .archive
            .entries
            .iter()
            .position(|entry| entry.name.eq_ignore_ascii_case(CONTENT_TYPES_ENTRY))
            .ok_or(OpenError::MissingContentTypes)?;
        package.content_types = package.read_content_types(content_types_entry)?;
        for (index, entry) in package.archive.entries.iter().enumerate() {
            if index == content_types_entry || entry.is_directory() {
                continue;
            }
            package.parts.push(Part {
                name: entry.name.clone(),
                content_type: package.content_types.of(&entry.name).map(str::to_owned),
                entry: index,
                compressed: entry.method == zip::Method::Deflated,
                compressed_size: u64::from(entry.compressed_size),
                size: u64::from(entry.uncompressed_size),
                crc32: entry.crc32,
            });
        }
        for index in 0..package.parts.len() {
            let part = &package.parts[index];
            if let Some(source) = relationships_source(&part.name) {
                let relationships = package.read_relationships(part.entry, source)?;
                package.relationships.push(relationships);
            }
        }
        Ok(package)
    }

    pub(crate) fn source(&self) -> &dyn PackageRead {
        self.source.as_ref()
    }

    pub(crate) fn archive(&self) -> &Archive {
        &self.archive
    }

    /// The part directory, in central directory order. `[Content_Types].xml` and directory
    /// entries are entries of the archive but not parts.
    pub fn parts(&self) -> &[Part] {
        &self.parts
    }

    /// The part named `name`, comparing ASCII case-insensitively as OPC does.
    pub fn part(&self, name: &str) -> Option<&Part> {
        self.parts
            .iter()
            .find(|part| part.name.eq_ignore_ascii_case(name))
    }

    pub fn content_types(&self) -> &ContentTypes {
        &self.content_types
    }

    /// Every relationships part's relationships, in part directory order.
    pub fn relationships(&self) -> &[Relationships] {
        &self.relationships
    }

    /// The relationships of `source` (`None` for the package's own).
    pub fn relationships_of(&self, source: Option<&str>) -> Option<&Relationships> {
        self.relationships
            .iter()
            .find(|relationships| match (relationships.source(), source) {
                (None, None) => true,
                (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                _ => false,
            })
    }

    /// The part's decompressed content, checked against its declared size and CRC-32.
    pub fn read_part(&self, name: &str) -> Result<Vec<u8>, PartReadError> {
        let part = self.part(name).ok_or_else(|| PartReadError::NotAPart {
            name: name.to_owned(),
        })?;
        zip::read_content(self.source(), &self.archive.entries[part.entry])
    }

    /// The part's content, ready to be read lazily as XML under the package's XML limits.
    pub fn read_xml(&self, name: &str) -> Result<XmlPart, PartReadError> {
        let bytes = self.read_part(name)?;
        let name = self.part(name).map_or(name, Part::name).to_owned();
        Ok(XmlPart::new(name, bytes, self.xml_limits))
    }

    /// The index of `[Content_Types].xml` among the archive's entries, which opening found.
    pub(crate) fn content_types_entry(&self) -> usize {
        self.archive
            .entries
            .iter()
            .position(|entry| entry.name.eq_ignore_ascii_case(CONTENT_TYPES_ENTRY))
            .expect("an opened package has [Content_Types].xml")
    }

    /// The content of `[Content_Types].xml`, read again from the source, ready to be read as XML.
    pub(crate) fn content_types_xml(&self) -> Result<XmlPart, PartReadError> {
        let entry = &self.archive.entries[self.content_types_entry()];
        let bytes = zip::read_content(self.source(), entry)?;
        Ok(XmlPart::new(entry.name.clone(), bytes, self.xml_limits))
    }

    fn structural_xml(&self, index: usize) -> Result<XmlPart, OpenError> {
        let entry: &Entry = &self.archive.entries[index];
        let bytes = zip::read_content(self.source(), entry).map_err(|error| OpenError::Part {
            part: entry.name.clone(),
            error,
        })?;
        Ok(XmlPart::new(entry.name.clone(), bytes, self.xml_limits))
    }

    fn read_content_types(&self, index: usize) -> Result<ContentTypes, OpenError> {
        let part = self.structural_xml(index)?;
        let invalid = |detail| OpenError::InvalidContentTypes { detail };
        let mut reader = part.reader();
        let mut next = || {
            reader.next_event().map_err(|error| OpenError::Xml {
                part: part.name().to_owned(),
                error,
            })
        };
        let mut types = ContentTypes::default();
        match next()? {
            Some(XmlEvent::Start(root)) if root.name.is(CONTENT_TYPES_NAMESPACE, "Types") => {}
            _ => return Err(invalid("the document element is not Types")),
        }
        let mut extensions = BTreeSet::new();
        let mut part_names = BTreeSet::new();
        loop {
            let element = match next()? {
                Some(XmlEvent::Start(element)) => element,
                Some(XmlEvent::Text(text)) if text.trim().is_empty() => continue,
                Some(XmlEvent::Text(_)) => return Err(invalid("Types holds text")),
                Some(XmlEvent::End(_)) => break,
                None => return Err(invalid("the document ends inside Types")),
            };
            let content_type = element
                .attribute("ContentType")
                .ok_or(invalid("a Default or Override without ContentType"))?
                .to_owned();
            if element.name.is(CONTENT_TYPES_NAMESPACE, "Default") {
                let extension = element
                    .attribute("Extension")
                    .filter(|extension| !extension.is_empty())
                    .ok_or(invalid("a Default without Extension"))?;
                if !extensions.insert(extension.to_ascii_lowercase()) {
                    return Err(invalid("two Defaults for one extension"));
                }
                types.defaults.push((extension.to_owned(), content_type));
            } else if element.name.is(CONTENT_TYPES_NAMESPACE, "Override") {
                let part_name = element
                    .attribute("PartName")
                    .filter(|name| name.starts_with('/'))
                    .ok_or(invalid("an Override without a PartName starting with /"))?;
                if !part_names.insert(part_name.to_ascii_lowercase()) {
                    return Err(invalid("two Overrides for one part"));
                }
                types.overrides.push((part_name.to_owned(), content_type));
            } else {
                return Err(invalid("an element other than Default or Override"));
            }
            match next()? {
                Some(XmlEvent::End(_)) => {}
                _ => return Err(invalid("a Default or Override has content")),
            }
        }
        match next()? {
            None => Ok(types),
            Some(_) => Err(invalid("content after Types")),
        }
    }

    fn read_relationships(
        &self,
        index: usize,
        source: Option<String>,
    ) -> Result<Relationships, OpenError> {
        let part = self.structural_xml(index)?;
        let name = part.name().to_owned();
        let invalid = |detail| OpenError::InvalidRelationships {
            part: name.clone(),
            detail,
        };
        let mut reader = part.reader();
        let mut next = || {
            reader.next_event().map_err(|error| OpenError::Xml {
                part: name.clone(),
                error,
            })
        };
        match next()? {
            Some(XmlEvent::Start(root))
                if root.name.is(RELATIONSHIPS_NAMESPACE, "Relationships") => {}
            _ => return Err(invalid("the document element is not Relationships")),
        }
        let mut items = Vec::new();
        let mut ids = BTreeSet::new();
        loop {
            let element = match next()? {
                Some(XmlEvent::Start(element)) => element,
                Some(XmlEvent::Text(text)) if text.trim().is_empty() => continue,
                Some(XmlEvent::Text(_)) => return Err(invalid("Relationships holds text")),
                Some(XmlEvent::End(_)) => break,
                None => return Err(invalid("the document ends inside Relationships")),
            };
            if !element.name.is(RELATIONSHIPS_NAMESPACE, "Relationship") {
                return Err(invalid("an element other than Relationship"));
            }
            let required = |local: &'static str, detail: &'static str| {
                element
                    .attribute(local)
                    .map(str::to_owned)
                    .ok_or_else(|| invalid(detail))
            };
            let id = required("Id", "a Relationship without Id")?;
            let kind = required("Type", "a Relationship without Type")?;
            let target = required("Target", "a Relationship without Target")?;
            let mode = match element.attribute("TargetMode") {
                None | Some("Internal") => TargetMode::Internal,
                Some("External") => TargetMode::External,
                Some(_) => return Err(invalid("a TargetMode other than Internal or External")),
            };
            if !ids.insert(id.clone()) {
                return Err(invalid("two Relationships with one Id"));
            }
            let resolved = match mode {
                TargetMode::Internal => resolve_target(source.as_deref(), &target),
                TargetMode::External => None,
            };
            items.push(Relationship {
                id,
                kind,
                target,
                mode,
                resolved,
            });
            match next()? {
                Some(XmlEvent::End(_)) => {}
                _ => return Err(invalid("a Relationship has content")),
            }
        }
        match next()? {
            None => Ok(Relationships {
                part: name.clone(),
                source,
                items,
            }),
            Some(_) => Err(invalid("content after Relationships")),
        }
    }
}
