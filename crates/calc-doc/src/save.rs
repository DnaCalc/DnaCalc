//! Saving: a plan of how each entry is written, then the writing.
//!
//! Every entry is untouched until the authoring beads add edits, so a plan copies each entry's
//! stored bytes (local header, compressed payload and data descriptor) unchanged, in the order
//! they were stored, and writes a central directory whose records are the opened ones with only
//! their local header offsets changed. Bytes between entries that no entry stores are not copied.

use crate::opc::Package;
use crate::source::{PackageWrite, SinkError, SourceError};
use crate::zip::crc32::Crc32;
use crate::zip::{self, CENTRAL_LOCAL_OFFSET_AT};
use std::fmt;
use std::sync::Arc;

/// An immutable view of an opened document, which a save is prepared from.
#[derive(Clone)]
pub struct DocumentSnapshot {
    package: Arc<Package>,
}

impl DocumentSnapshot {
    pub(crate) fn new(package: Arc<Package>) -> Self {
        DocumentSnapshot { package }
    }

    pub fn package(&self) -> &Package {
        &self.package
    }
}

/// One entry's place in the saved package.
struct Placement {
    entry: usize,
    offset: u32,
}

/// How a snapshot will be written.
pub struct SavePlan {
    package: Arc<Package>,
    /// The entries in the order their stored bytes are written, each with its new offset.
    placements: Vec<Placement>,
}

impl SavePlan {
    /// The number of entries whose stored bytes the plan copies unchanged.
    pub fn copied_entries(&self) -> usize {
        self.placements.len()
    }
}

/// What a completed write produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveReceipt {
    pub bytes_written: u64,
    pub entries_copied: usize,
}

/// Why a write failed. The sink then holds an incomplete package, which must be discarded.
#[derive(Debug, PartialEq, Eq)]
pub enum WriteError {
    /// The backing source failed a read.
    Source(SourceError),
    /// An entry's stored bytes in the source are no longer the bytes that were opened.
    SourceChanged { entry: String },
    /// The sink refused bytes.
    Sink(SinkError),
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(f, "the package source failed: {error}"),
            Self::SourceChanged { entry } => write!(
                f,
                "the stored bytes of the entry {entry:?} changed in the source since it was opened"
            ),
            Self::Sink(error) => write!(f, "the sink failed: {error}"),
        }
    }
}

impl std::error::Error for WriteError {}

impl From<SourceError> for WriteError {
    fn from(error: SourceError) -> Self {
        WriteError::Source(error)
    }
}

/// Plans the save of `snapshot`: every entry is copied, in the order its bytes were stored.
pub fn prepare_save(snapshot: &DocumentSnapshot) -> SavePlan {
    let entries = &snapshot.package.archive().entries;
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by_key(|&index| entries[index].local_offset);
    let mut offset: u64 = 0;
    let placements = order
        .into_iter()
        .map(|entry| {
            // Entries are copied in stored order without the bytes between them, so each starts
            // no later than it did in the source, where offsets are 32-bit.
            let placement = Placement {
                entry,
                offset: u32::try_from(offset)
                    .expect("a copied entry starts no later than its 32-bit source offset"),
            };
            offset += entries[entry].span_len();
            placement
        })
        .collect();
    SavePlan {
        package: Arc::clone(&snapshot.package),
        placements,
    }
}

struct Counted<'a> {
    out: &'a mut dyn PackageWrite,
    written: u64,
}

impl Counted<'_> {
    fn put(&mut self, bytes: &[u8]) -> Result<(), WriteError> {
        self.out.write_all(bytes).map_err(WriteError::Sink)?;
        self.written += bytes.len() as u64;
        Ok(())
    }
}

/// Writes the plan's package to `out`.
pub fn write(plan: SavePlan, out: &mut dyn PackageWrite) -> Result<SaveReceipt, WriteError> {
    let package = &plan.package;
    let source = package.source();
    let archive = package.archive();
    let mut sink = Counted { out, written: 0 };
    let mut offsets = vec![0u32; archive.entries.len()];
    for placement in &plan.placements {
        let entry = &archive.entries[placement.entry];
        offsets[placement.entry] = placement.offset;
        let mut crc = Crc32::new();
        zip::each_chunk(
            source,
            u64::from(entry.local_offset),
            entry.span_end,
            |chunk| {
                crc.update(chunk);
                sink.put(chunk)
            },
        )?;
        if crc.finish() != entry.span_crc {
            return Err(WriteError::SourceChanged {
                entry: entry.name.clone(),
            });
        }
    }
    let directory_offset = sink.written;
    for (entry, offset) in archive.entries.iter().zip(&offsets) {
        let mut record = entry.central_record.clone();
        record[CENTRAL_LOCAL_OFFSET_AT..CENTRAL_LOCAL_OFFSET_AT + 4]
            .copy_from_slice(&offset.to_le_bytes());
        sink.put(&record)?;
    }
    let directory_size = sink.written - directory_offset;
    let count = archive.entries.len() as u16;
    let mut end = Vec::with_capacity(22 + archive.comment.len());
    end.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    end.extend_from_slice(&[0, 0, 0, 0]);
    end.extend_from_slice(&count.to_le_bytes());
    end.extend_from_slice(&count.to_le_bytes());
    // The central records are the opened ones and the entries start no later than they did, so
    // both fit the 32-bit fields the opened archive's end record used.
    end.extend_from_slice(&(directory_size as u32).to_le_bytes());
    end.extend_from_slice(&(directory_offset as u32).to_le_bytes());
    end.extend_from_slice(&(archive.comment.len() as u16).to_le_bytes());
    end.extend_from_slice(&archive.comment);
    sink.put(&end)?;
    Ok(SaveReceipt {
        bytes_written: sink.written,
        entries_copied: plan.placements.len(),
    })
}
