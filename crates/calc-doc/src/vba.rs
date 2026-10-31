//! Explicit, read-only VBA source ingestion. Normal workbook loading never calls this reader.

use crate::{Package, PartReadError};
use std::fmt;

mod directory;

/// Limits accepted project/source sizes. The compressed directory is validated before parsing.
#[derive(Clone, Copy, Debug)]
pub struct VbaReadPolicy {
    pub max_project_bytes: usize,
    pub max_directory_bytes: usize,
    pub max_modules: usize,
    pub max_module_source_bytes: usize,
    pub max_total_source_bytes: usize,
}

impl Default for VbaReadPolicy {
    fn default() -> Self {
        Self {
            max_project_bytes: 8 * 1024 * 1024,
            max_directory_bytes: 1024 * 1024,
            max_modules: 1024,
            max_module_source_bytes: 1024 * 1024,
            max_total_source_bytes: 4 * 1024 * 1024,
        }
    }
}

/// The two module kinds encoded in the project directory. The second combines three kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VbaModuleKind {
    Procedural,
    DocumentClassOrDesigner,
}

/// Source bytes retain the project's code page and all exported Attribute lines.
/// This is source metadata, not an assertion about callable worksheet procedures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VbaModuleSource {
    pub name: String,
    pub stream_name: String,
    pub kind: VbaModuleKind,
    pub private: bool,
    pub read_only: bool,
    pub description: String,
    pub help_context: u32,
    pub source_bytes: Vec<u8>,
    pub source: String,
}

/// Workbook-project source provenance. No VBA code has been compiled or executed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VbaProjectSource {
    pub part: String,
    pub code_page: u16,
    pub modules: Vec<VbaModuleSource>,
}

#[derive(Debug)]
pub enum VbaReadError {
    Part(PartReadError),
    Relationship,
    Limit(&'static str),
    InvalidProject(String),
    UnsupportedCodePage(u16),
    InvalidText { module: String, code_page: u16 },
}

impl fmt::Display for VbaReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Part(error) => write!(f, "VBA project part: {error}"),
            Self::Relationship => {
                f.write_str("invalid or multiple workbook VBA project relationships")
            }
            Self::Limit(what) => write!(f, "VBA project exceeds {what} limit"),
            Self::InvalidProject(why) => write!(f, "invalid VBA project: {why}"),
            Self::UnsupportedCodePage(cp) => write!(f, "unsupported VBA code page {cp}"),
            Self::InvalidText { module, code_page } => write!(
                f,
                "VBA module {module} is not valid code page {code_page} text"
            ),
        }
    }
}
impl std::error::Error for VbaReadError {}

fn invalid(why: impl fmt::Display) -> VbaReadError {
    VbaReadError::InvalidProject(why.to_string())
}

fn read_directory(input: &[u8]) -> Result<directory::ProjectInformation, VbaReadError> {
    let code_page =
        directory::project_code_page(input).map_err(|_| invalid("project directory prefix"))?;
    if codepage::to_encoding(code_page).is_none() {
        return Err(VbaReadError::UnsupportedCodePage(code_page));
    }
    directory::read(input).map_err(|_| invalid("project directory records or text encoding"))
}

// MS-OVBA 2.4.1: compressed containers consist of independently bounded 4096-byte chunks.
// Validate headers, copy distances and output bounds before allocating or copying a token.
fn decompress(input: &[u8], limit: usize) -> Result<Vec<u8>, VbaReadError> {
    if input.first() != Some(&1) {
        return Err(invalid("compressed-container signature"));
    }
    let mut at = 1usize;
    let mut output = Vec::new();
    while at < input.len() {
        let bytes = input
            .get(at..at + 2)
            .ok_or_else(|| invalid("chunk header"))?;
        let header = u16::from_le_bytes([bytes[0], bytes[1]]);
        if header & 0x7000 != 0x3000 {
            return Err(invalid("chunk signature"));
        }
        let end = at
            .checked_add(usize::from(header & 0xfff) + 3)
            .filter(|end| *end <= input.len())
            .ok_or_else(|| invalid("chunk size"))?;
        at += 2;
        let start = output.len();
        if header & 0x8000 == 0 {
            if end - at != 4096 {
                return Err(invalid("raw chunk size"));
            }
            if output.len().checked_add(4096).is_none_or(|n| n > limit) {
                return Err(VbaReadError::Limit("decompressed bytes"));
            }
            output.extend_from_slice(&input[at..end]);
            at = end;
            continue;
        }
        while at < end {
            let flags = input[at];
            at += 1;
            for bit in 0..8 {
                if at == end {
                    break;
                }
                if flags & (1 << bit) == 0 {
                    if output.len() == limit {
                        return Err(VbaReadError::Limit("decompressed bytes"));
                    }
                    if output.len() - start == 4096 {
                        return Err(invalid("expanded chunk size"));
                    }
                    output.push(input[at]);
                    at += 1;
                } else {
                    if end - at < 2 {
                        return Err(invalid("copy token"));
                    }
                    let token = u16::from_le_bytes([input[at], input[at + 1]]);
                    at += 2;
                    let size = output.len() - start;
                    let mut bits = 4;
                    while (1usize << bits) < size {
                        bits += 1;
                    }
                    let mask = 0xffffu16 >> bits;
                    let length = usize::from(token & mask) + 3;
                    let distance = usize::from(token >> (16 - bits)) + 1;
                    if distance > size || size + length > 4096 {
                        return Err(invalid("copy distance or expanded chunk size"));
                    }
                    if output.len().checked_add(length).is_none_or(|n| n > limit) {
                        return Err(VbaReadError::Limit("decompressed bytes"));
                    }
                    for _ in 0..length {
                        let byte = output[output.len() - distance];
                        output.push(byte);
                    }
                }
            }
        }
    }
    Ok(output)
}

fn stream(
    cfb: &mut cfb::CompoundFile<std::io::Cursor<Vec<u8>>>,
    path: &str,
    limit: usize,
) -> Result<Vec<u8>, VbaReadError> {
    use std::io::Read;
    let input = cfb.open_stream(path).map_err(invalid)?;
    if input.len() > limit as u64 {
        return Err(VbaReadError::Limit("project stream bytes"));
    }
    let mut bytes = Vec::new();
    input
        .take(limit.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(invalid)?;
    if bytes.len() > limit {
        return Err(VbaReadError::Limit("project stream bytes"));
    }
    Ok(bytes)
}

pub(crate) fn read(
    package: &Package,
    workbook: &str,
    policy: &VbaReadPolicy,
) -> Result<Option<VbaProjectSource>, VbaReadError> {
    const VBA_PROJECT: &str = "http://schemas.microsoft.com/office/2006/relationships/vbaProject";
    let mut links = package
        .relationships_of(Some(workbook))
        .into_iter()
        .flat_map(|r| r.items())
        .filter(|r| r.kind == VBA_PROJECT);
    let Some(link) = links.next() else {
        return Ok(None);
    };
    if links.next().is_some() || link.mode != crate::TargetMode::Internal {
        return Err(VbaReadError::Relationship);
    }
    let part = link.resolved.as_deref().ok_or(VbaReadError::Relationship)?;
    let entry = package.part(part).ok_or(VbaReadError::Relationship)?;
    if entry.size() > policy.max_project_bytes as u64 {
        return Err(VbaReadError::Limit("project bytes"));
    }
    let bytes = package.read_part(part).map_err(VbaReadError::Part)?;
    if bytes.len() > policy.max_project_bytes {
        return Err(VbaReadError::Limit("project bytes"));
    }
    let mut container = cfb::CompoundFile::open(std::io::Cursor::new(bytes)).map_err(invalid)?;
    let directory = stream(&mut container, "/VBA/dir", policy.max_project_bytes)?;
    let directory = decompress(&directory, policy.max_directory_bytes)?;
    let project = read_directory(&directory)?;
    if project.modules.len() > policy.max_modules {
        return Err(VbaReadError::Limit("module count"));
    }
    let code_page = project.information.code_page;
    let encoding =
        codepage::to_encoding(code_page).ok_or(VbaReadError::UnsupportedCodePage(code_page))?;
    let mut modules = Vec::new();
    let mut names = std::collections::BTreeSet::new();
    let mut total = 0usize;
    for module in &project.modules {
        if module.name.is_empty()
            || module.stream_name.is_empty()
            || module.stream_name.contains(['/', '\\'])
            || module.stream_name == "."
            || module.stream_name == ".."
            || !names.insert(calc_doc_model::value::fold_workbook_name(&module.name))
        {
            return Err(invalid(
                "duplicate module identity or invalid module stream name",
            ));
        }
        let compressed = stream(
            &mut container,
            &format!("/VBA/{}", module.stream_name),
            policy.max_project_bytes,
        )?;
        let compressed = compressed
            .get(module.text_offset..)
            .ok_or_else(|| invalid("module source offset"))?;
        let available = policy
            .max_total_source_bytes
            .checked_sub(total)
            .ok_or(VbaReadError::Limit("total source bytes"))?;
        let source_bytes = decompress(compressed, policy.max_module_source_bytes.min(available))?;
        total += source_bytes.len();
        let source = encoding
            .decode_without_bom_handling_and_without_replacement(&source_bytes)
            .ok_or_else(|| VbaReadError::InvalidText {
                module: module.name.clone(),
                code_page,
            })?
            .into_owned();
        modules.push(VbaModuleSource {
            name: module.name.clone(),
            stream_name: module.stream_name.clone(),
            kind: match module.module_type {
                directory::ModuleType::Procedural => VbaModuleKind::Procedural,
                directory::ModuleType::DocClsDesigner => VbaModuleKind::DocumentClassOrDesigner,
            },
            private: module.private,
            read_only: module.read_only,
            description: module.doc_string.clone(),
            help_context: module.help_context,
            source_bytes,
            source,
        });
    }
    Ok(Some(VbaProjectSource {
        part: part.into(),
        code_page,
        modules,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measured_directory() -> Vec<u8> {
        let source = include_bytes!("../tests/fixtures/vba_udf_names/local.xlsm");
        let opened = crate::open(
            std::sync::Arc::new(crate::MemorySource::new(source.to_vec())),
            &crate::OpenPolicy::default(),
        )
        .unwrap();
        let bytes = opened.package().read_part("xl/vbaProject.bin").unwrap();
        let mut container = cfb::CompoundFile::open(std::io::Cursor::new(bytes)).unwrap();
        let compressed = stream(&mut container, "/VBA/dir", 1024 * 1024).unwrap();
        decompress(&compressed, 1024 * 1024).unwrap()
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn malformed_directory_and_unknown_code_page_return_errors_without_panics() {
        let directory = measured_directory();
        assert!(read_directory(&directory).is_ok());
        for end in 0..directory.len() {
            assert!(read_directory(&directory[..end]).is_err());
        }
        let mut trailing = directory.clone();
        trailing.push(0);
        assert!(read_directory(&trailing).is_err());
        let code_page_at = directory
            .windows(8)
            .position(|bytes| bytes[..6] == [3, 0, 2, 0, 0, 0])
            .unwrap()
            + 6;
        let mut unknown = directory;
        unknown[code_page_at..code_page_at + 2].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(matches!(
            read_directory(&unknown),
            Err(VbaReadError::UnsupportedCodePage(u16::MAX))
        ));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn malformed_compressed_streams_are_refused_before_copying() {
        for bytes in [&[][..], &[0], &[1, 0], &[1, 0, 0], &[1, 2, 0xb0, 1, 0, 0]] {
            assert!(decompress(bytes, 4096).is_err(), "{bytes:?}");
        }
        // A literal A and an overlapping copy of three A bytes.
        let compressed = [1, 3, 0xb0, 2, b'A', 0, 0];
        assert_eq!(decompress(&compressed, 4).unwrap(), b"AAAA");
        assert!(matches!(
            decompress(&compressed, 3),
            Err(VbaReadError::Limit(_))
        ));
    }
}
