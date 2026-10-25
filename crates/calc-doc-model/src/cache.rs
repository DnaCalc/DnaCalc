//! Values a file stored as formula results. A cached value never counts as calculated: each one is
//! [`FileCached`], carrying the identity of the package part it was read from. It is a calculation
//! input in one place only, as Excel's: the value an iterating formula on a circular reference
//! starts its first pass from (CalcEngine's recalculation, dc-10y).

use crate::input::{CellText, FiniteNumber};
use calc_value::WorksheetErrorCode;
use std::sync::Arc;

/// The identity of the package part a value was read from: its part name (its ZIP item name,
/// without a leading `/`), and the CRC-32 and uncompressed size its archive entry declares, which
/// the reader checked the part's content against. The CRC-32 is not a cryptographic hash: it tells
/// the part's bytes apart from other bytes the same part held, not from deliberately forged ones.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SourcePart {
    name: String,
    crc32: u32,
    size: u64,
}

impl SourcePart {
    pub fn new(name: String, crc32: u32, size: u64) -> Self {
        SourcePart { name, crc32, size }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn crc32(&self) -> u32 {
        self.crc32
    }

    pub fn size(&self) -> u64 {
        self.size
    }
}

/// A value a file stored as a formula's result.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CachedValue {
    Number(FiniteNumber),
    Text(CellText),
    Logical(bool),
    Error(WorksheetErrorCode),
}

/// A formula result whose provenance is a file cache: the value a package stored beside the
/// formula, and the part it was read from. Nothing here calculated it, and it is kept apart from
/// the formula's [`crate::CellInput`]; it is never evidence that the formula was evaluated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileCached {
    value: CachedValue,
    source: Arc<SourcePart>,
}

impl FileCached {
    pub fn new(value: CachedValue, source: Arc<SourcePart>) -> Self {
        FileCached { value, source }
    }

    pub fn value(&self) -> &CachedValue {
        &self.value
    }

    /// The part the value was read from, shared by every cache read from that part.
    pub fn source(&self) -> &Arc<SourcePart> {
        &self.source
    }
}
