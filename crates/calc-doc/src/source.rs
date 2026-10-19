//! The seekable backing source a package is read from, and the sink a saved package is written to.
//! The codec never opens a file: a platform adapter or the caller supplies both.

use std::fmt;
use std::sync::Arc;

/// Why a source or a sink failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceError {
    detail: String,
}

impl SourceError {
    pub fn new(detail: impl Into<String>) -> Self {
        SourceError {
            detail: detail.into(),
        }
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for SourceError {}

/// Why a sink refused bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SinkError {
    detail: String,
}

impl SinkError {
    pub fn new(detail: impl Into<String>) -> Self {
        SinkError {
            detail: detail.into(),
        }
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for SinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for SinkError {}

/// A package's bytes, readable at any offset. An opened document keeps its source and reads part
/// payloads from it when they are needed and when the package is saved, so a source must keep
/// returning the bytes it returned when the package was opened; saving checks that it did.
pub trait PackageRead: Send + Sync {
    /// The source's length in bytes.
    fn size(&self) -> u64;

    /// Fills `buf` with the bytes starting at `offset`, or fails without a partial result.
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), SourceError>;
}

/// The sink a saved package is written to, front to back.
pub trait PackageWrite {
    /// Appends `bytes`.
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), SinkError>;
}

/// A package held in memory, as the browser receives a file.
#[derive(Clone, Debug)]
pub struct MemorySource {
    bytes: Arc<[u8]>,
}

impl MemorySource {
    pub fn new(bytes: impl Into<Arc<[u8]>>) -> Self {
        MemorySource {
            bytes: bytes.into(),
        }
    }
}

impl PackageRead for MemorySource {
    fn size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), SourceError> {
        let range = usize::try_from(offset)
            .ok()
            .and_then(|start| Some(start..start.checked_add(buf.len())?))
            .filter(|range| range.end <= self.bytes.len())
            .ok_or_else(|| {
                SourceError::new(format!(
                    "{} bytes at offset {offset} are past the end of a {}-byte source",
                    buf.len(),
                    self.bytes.len()
                ))
            })?;
        buf.copy_from_slice(&self.bytes[range]);
        Ok(())
    }
}

impl PackageWrite for Vec<u8> {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), SinkError> {
        self.extend_from_slice(bytes);
        Ok(())
    }
}
