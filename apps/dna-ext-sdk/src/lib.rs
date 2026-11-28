//! The signed extension archive and the slot-only Worker contract.
use calc_skin::{Coverage, RawValue, RowPage, Shown, Stamp, parts::Trace};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::{Cursor, Read},
    sync::Arc,
};
use wasmparser::{Encoding, ExternalKind, Operator, Parser, Payload};
use zip::ZipArchive;

#[cfg(target_arch = "wasm32")]
mod worker;
#[cfg(target_arch = "wasm32")]
pub use worker::{RenderJob, start_worker};

pub const FORMAT: &str = "dna.component/1";
const MAX_MANIFEST: usize = 16 * 1024;
const MAX_MODULE: usize = 4 * 1024 * 1024;
const MAX_ARCHIVE: usize = MAX_MODULE + MAX_MANIFEST + 8 * 1024;
const MAX_MESSAGE: usize = 256 * 1024;
const MAX_MEMORY_PAGES: u64 = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotKind {
    Value,
    Rows,
    Expression,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub id: String,
    pub publisher: String,
    pub version: String,
    pub read_slots: BTreeMap<String, SlotKind>,
    #[serde(default)]
    pub write_slots: BTreeSet<String>,
}

pub struct Publisher<'a> {
    pub name: &'a str,
    pub key: [u8; 32],
}

#[derive(Debug, PartialEq, Eq)]
pub enum ExtensionError {
    Archive(String),
    Manifest(String),
    Publisher,
    Signature,
    Module(String),
    Binding(String),
    Output(String),
    Worker(String),
}

impl fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Archive(reason)
            | Self::Manifest(reason)
            | Self::Module(reason)
            | Self::Binding(reason)
            | Self::Output(reason)
            | Self::Worker(reason) => f.write_str(reason),
            Self::Publisher => f.write_str("The extension publisher is unknown."),
            Self::Signature => f.write_str("The extension signature could not be verified."),
        }
    }
}
impl std::error::Error for ExtensionError {}

/// The verified package can be shown on a trust card before the operator installs it.
#[derive(Debug)]
pub struct SignedExtension {
    manifest: Manifest,
    module: Arc<[u8]>,
}

impl SignedExtension {
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// The page calls this when the operator approves the authenticated manifest.
    pub fn approve(self) -> InstalledExtension {
        InstalledExtension(Arc::new(self))
    }
}

#[derive(Clone)]
pub struct InstalledExtension(Arc<SignedExtension>);

impl InstalledExtension {
    pub fn manifest(&self) -> &Manifest {
        self.0.manifest()
    }

    /// The authenticated Wasm bytes for the isolated runtime.
    pub fn module(&self) -> &[u8] {
        &self.0.module
    }

    pub fn encode_bindings(&self, bindings: &Bindings) -> Result<String, ExtensionError> {
        let declared = &self.manifest().read_slots;
        if declared.len() != bindings.slots.len()
            || bindings
                .slots
                .iter()
                .any(|(name, slot)| declared.get(name) != Some(&slot.value.kind()))
        {
            return Err(ExtensionError::Binding(
                "The extension bindings differ from its declared read slots.".into(),
            ));
        }
        let input = serde_json::to_string(bindings)
            .map_err(|error| ExtensionError::Binding(error.to_string()))?;
        if input.len() > MAX_MESSAGE {
            return Err(ExtensionError::Binding(
                "The extension bindings exceed the byte limit.".into(),
            ));
        }
        Ok(input)
    }

    pub fn decode_output(&self, bytes: &[u8]) -> Result<Output, ExtensionError> {
        if bytes.len() > MAX_MESSAGE {
            return Err(ExtensionError::Output(
                "The extension output exceeds the byte limit.".into(),
            ));
        }
        let output: Output = serde_json::from_slice(bytes)
            .map_err(|error| ExtensionError::Output(error.to_string()))?;
        if !output.tree.bounded(0, &mut 512) {
            return Err(ExtensionError::Output(
                "The extension render tree exceeds its limits.".into(),
            ));
        }
        let manifest = self.manifest();
        if output.intents.len() > 32
            || output.intents.iter().any(|intent| match intent {
                Intent::SelectSlot { slot } => !manifest.read_slots.contains_key(slot),
                Intent::WindowRows { slot, start, count } => {
                    manifest.read_slots.get(slot) != Some(&SlotKind::Rows)
                        || *count == 0
                        || *count > 1000
                        || start.checked_add(*count).is_none()
                }
                Intent::WriteSlot { slot, .. } => !manifest.write_slots.contains(slot),
            })
        {
            return Err(ExtensionError::Output(
                "An extension intent exceeds its declared slot access.".into(),
            ));
        }
        Ok(output)
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Bindings {
    pub stamp: Stamp,
    pub coverage: Coverage,
    pub slots: BTreeMap<String, Shown<SlotValue>>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum SlotValue {
    Value(RawValue),
    Rows(RowPage),
    Expression(Trace),
}

impl SlotValue {
    fn kind(&self) -> SlotKind {
        match self {
            Self::Value(_) => SlotKind::Value,
            Self::Rows(_) => SlotKind::Rows,
            Self::Expression(_) => SlotKind::Expression,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    pub tree: RenderTree,
    #[serde(default)]
    pub intents: Vec<Intent>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub enum RenderTree {
    Stack(Vec<RenderTree>),
    Text(String),
    Table {
        headers: Vec<String>,
        rows: Vec<RenderRow>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RenderRow {
    pub depth: u8,
    pub cells: Vec<String>,
}

impl RenderTree {
    fn bounded(&self, depth: usize, remaining: &mut usize) -> bool {
        if depth > 16 || *remaining == 0 {
            return false;
        }
        *remaining -= 1;
        let text = |value: &String| value.len() <= 16 * 1024;
        match self {
            Self::Text(value) => text(value),
            Self::Stack(children) => children
                .iter()
                .all(|child| child.bounded(depth + 1, remaining)),
            Self::Table { headers, rows } => {
                (1..=12).contains(&headers.len())
                    && headers.iter().all(text)
                    && rows.len() <= 500
                    && rows.iter().all(|row| {
                        row.depth <= 12
                            && row.cells.len() == headers.len()
                            && row.cells.iter().all(text)
                    })
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub enum Intent {
    SelectSlot {
        slot: String,
    },
    WindowRows {
        slot: String,
        start: u32,
        count: u32,
    },
    WriteSlot {
        slot: String,
        value: String,
    },
}

/// Read a stored ZIP containing the manifest, module and detached Ed25519 signature.
pub fn verify(
    archive: &[u8],
    publishers: &[Publisher<'_>],
) -> Result<SignedExtension, ExtensionError> {
    if archive.len() > MAX_ARCHIVE {
        return Err(ExtensionError::Archive(
            "The extension archive exceeds the byte limit.".into(),
        ));
    }
    let mut archive = ZipArchive::new(Cursor::new(archive))
        .map_err(|error| ExtensionError::Archive(error.to_string()))?;
    if archive.len() != 3 {
        return Err(ExtensionError::Archive(
            "An extension archive needs extension.json, module.wasm and extension.sig.".into(),
        ));
    }
    let source = entry(&mut archive, "extension.json", MAX_MANIFEST)?;
    let module = entry(&mut archive, "module.wasm", MAX_MODULE)?;
    let signature = entry(&mut archive, "extension.sig", 64)?;
    let manifest: Manifest = serde_json::from_slice(&source)
        .map_err(|error| ExtensionError::Manifest(error.to_string()))?;
    validate_manifest(&manifest)?;
    let publisher = publishers
        .iter()
        .find(|publisher| publisher.name == manifest.publisher)
        .ok_or(ExtensionError::Publisher)?;
    let key = VerifyingKey::from_bytes(&publisher.key).map_err(|_| ExtensionError::Publisher)?;
    let signature = Signature::from_slice(&signature).map_err(|_| ExtensionError::Signature)?;
    let mut signed = Vec::with_capacity(12 + source.len() + module.len());
    signed.extend(b"DNAEXT1\0");
    signed.extend((source.len() as u32).to_le_bytes());
    signed.extend(&source);
    signed.extend(&module);
    key.verify_strict(&signed, &signature)
        .map_err(|_| ExtensionError::Signature)?;
    validate_module(&module)?;
    Ok(SignedExtension {
        manifest,
        module: module.into(),
    })
}

fn entry(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    name: &str,
    maximum: usize,
) -> Result<Vec<u8>, ExtensionError> {
    let file = archive
        .by_name(name)
        .map_err(|error| ExtensionError::Archive(error.to_string()))?;
    if file.compression() != zip::CompressionMethod::Stored || file.size() > maximum as u64 {
        return Err(ExtensionError::Archive(format!(
            "{name} must be stored and within its byte limit."
        )));
    }
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| ExtensionError::Archive(error.to_string()))?;
    if bytes.len() > maximum {
        return Err(ExtensionError::Archive(format!(
            "{name} exceeds its byte limit."
        )));
    }
    Ok(bytes)
}

fn validate_manifest(manifest: &Manifest) -> Result<(), ExtensionError> {
    let id = |name: &str| {
        !name.is_empty()
            && name.len() <= 64
            && name.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
            })
    };
    if manifest.format != FORMAT
        || !manifest.id.contains('.')
        || manifest.id.len() > 128
        || !manifest.id.split('.').all(id)
        || manifest.publisher.trim().is_empty()
        || manifest.publisher.len() > 128
        || manifest.publisher.chars().any(char::is_control)
        || manifest.version.is_empty()
        || manifest.version.len() > 32
        || !manifest
            .version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
        || manifest.read_slots.len() > 16
        || manifest.write_slots.len() > 16
        || !manifest
            .read_slots
            .keys()
            .chain(&manifest.write_slots)
            .all(|name| id(name))
    {
        return Err(ExtensionError::Manifest(
            "The extension manifest has an invalid identity or slot declaration.".into(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_module(module: &[u8]) -> Result<(), ExtensionError> {
    let invalid = |reason: &str| ExtensionError::Module(reason.to_owned());
    let parse = |error: wasmparser::BinaryReaderError| ExtensionError::Module(error.to_string());
    let mut memory = false;
    let mut exports = BTreeSet::new();
    for part in Parser::new(0).parse_all(module) {
        match part.map_err(parse)? {
            Payload::Version {
                encoding: Encoding::Component,
                ..
            }
            | Payload::StartSection { .. } => {
                return Err(invalid(
                    "Extensions need a core Wasm module without a start function.",
                ));
            }
            Payload::ImportSection(imports) if imports.count() != 0 => {
                return Err(invalid("Extension Wasm imports are forbidden."));
            }
            Payload::MemorySection(memories) => {
                if memory || memories.count() != 1 {
                    return Err(invalid(
                        "An extension needs exactly one bounded Wasm memory.",
                    ));
                }
                let limits = memories
                    .into_iter()
                    .next()
                    .expect("one memory record")
                    .map_err(parse)?;
                if limits.memory64
                    || limits.shared
                    || limits.page_size_log2.is_some()
                    || limits.initial > MAX_MEMORY_PAGES
                    || !limits.maximum.is_some_and(|maximum| {
                        (limits.initial..=MAX_MEMORY_PAGES).contains(&maximum)
                    })
                {
                    return Err(invalid(
                        "An extension needs unshared 32-bit Wasm memory of at most 32 MiB.",
                    ));
                }
                memory = true;
            }
            Payload::TableSection(tables) => {
                for table in tables {
                    let ty = table.map_err(parse)?.ty;
                    if ty.table64
                        || ty.shared
                        || ty.initial > 4096
                        || ty.maximum.is_some_and(|max| max > 4096)
                    {
                        return Err(invalid("An extension needs a bounded Wasm function table."));
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                let mut code = body.get_operators_reader().map_err(parse)?;
                while !code.eof() {
                    if matches!(code.read().map_err(parse)?, Operator::TableGrow { .. }) {
                        return Err(invalid("An extension cannot grow Wasm tables."));
                    }
                }
            }
            Payload::ExportSection(entries) => {
                for export in entries {
                    let export = export.map_err(parse)?;
                    if matches!(
                        (export.name, export.kind),
                        ("memory", ExternalKind::Memory)
                            | (
                                "dna_alloc" | "dna_render" | "dna_output_len",
                                ExternalKind::Func
                            )
                    ) {
                        exports.insert(export.name);
                    }
                }
            }
            _ => {}
        }
    }
    if !memory || exports.len() != 4 {
        return Err(invalid(
            "The extension module is missing an ABI export or its memory.",
        ));
    }
    Ok(())
}
