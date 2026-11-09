//! The workbook's VBA project as calc-doc keeps it: the `vbaProject.bin` part, its relationship
//! from the workbook part and its content type, carried as an opaque blob (theory 5a, dc-0f1k.4).
//!
//! What is inside the blob (the compound storage, the MS-OVBA `dir` stream, modules, code page
//! and references) is CalcVBA's to read: `calc-vba` hands the blob to it. Normal workbook loading
//! never reads the part; saving puts it back as it was, or as the caller replaced it.

use crate::{Package, PartReadError};
use std::fmt;

pub(crate) const VBA_PROJECT: &str =
    "http://schemas.microsoft.com/office/2006/relationships/vbaProject";
/// The content type of the `vbaProject.bin` part.
pub const VBA_PROJECT_CONTENT_TYPE: &str = "application/vnd.ms-office.vbaProject";

pub(crate) fn has_project(package: &Package, workbook: &str) -> bool {
    package
        .relationships_of(Some(workbook))
        .into_iter()
        .flat_map(|r| r.items())
        .any(|r| r.kind == VBA_PROJECT)
}

/// Limits the accepted project part. Its contents are limited by CalcVBA when it reads them.
#[derive(Clone, Copy, Debug)]
pub struct VbaReadPolicy {
    pub max_project_bytes: usize,
}

impl Default for VbaReadPolicy {
    fn default() -> Self {
        Self {
            max_project_bytes: 8 * 1024 * 1024,
        }
    }
}

/// The project part of a workbook, as bytes. No VBA code has been read, compiled or executed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VbaProjectPart {
    /// The part's name in the package (`xl/vbaProject.bin` as Excel writes it).
    pub part: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub enum VbaReadError {
    Part(PartReadError),
    Relationship,
    Limit(&'static str),
    /// A document part's XML could not be read for its code name.
    CodeName(String),
}

impl fmt::Display for VbaReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Part(error) => write!(f, "VBA project part: {error}"),
            Self::Relationship => {
                f.write_str("invalid or multiple workbook VBA project relationships")
            }
            Self::Limit(what) => write!(f, "VBA project exceeds {what} limit"),
            Self::CodeName(why) => write!(f, "document code name: {why}"),
        }
    }
}
impl std::error::Error for VbaReadError {}

pub(crate) fn read(
    package: &Package,
    workbook: &str,
    policy: &VbaReadPolicy,
) -> Result<Option<VbaProjectPart>, VbaReadError> {
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
    Ok(Some(VbaProjectPart {
        part: part.to_owned(),
        bytes,
    }))
}
