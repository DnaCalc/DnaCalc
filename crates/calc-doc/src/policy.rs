//! The explicit resource limits a package is opened under.
//!
//! The defaults are chosen bounds, measured neither against Excel's own limits nor against the
//! plan's 100 MB workload; they keep one part's decompressed bytes addressable on wasm32.

use crate::xml::XmlLimits;

/// Bounds on the archive, all checked against the central directory before any payload is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackageLimits {
    /// The longest source accepted, in bytes.
    pub max_package_bytes: u64,
    /// The most entries the central directory may declare.
    pub max_entries: u32,
    /// The most uncompressed bytes one entry may declare. Decompression also stops at the
    /// declared size, and no single allocation of its output buffer passes it (a reallocation
    /// that moves briefly holds the old buffer too), so no part ever decompresses past this
    /// bound; a part's compressed payload is read into memory only when it is no larger than its
    /// declared size allows, so no read of a part holds more than about 1.14 times this bound,
    /// plus 1 KiB, in compressed bytes.
    pub max_part_bytes: u64,
    /// The most uncompressed bytes all entries together may declare.
    pub max_expanded_bytes: u64,
}

impl Default for PackageLimits {
    fn default() -> Self {
        PackageLimits {
            max_package_bytes: 1 << 30,
            max_entries: 20_000,
            max_part_bytes: 1 << 30,
            max_expanded_bytes: 4 << 30,
        }
    }
}

/// Everything [`crate::open`] is told: the archive's limits and the XML readers' limits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpenPolicy {
    pub package: PackageLimits,
    pub xml: XmlLimits,
}
