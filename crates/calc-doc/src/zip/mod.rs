//! The ZIP container under an OPC package: the end record, the central directory, each entry's
//! local header and payload.
//!
//! Every limit that the central directory alone can check is checked before any local header or
//! payload is read. Each entry's stored bytes (local header, payload and data descriptor) are then
//! located, checked against its central record and against every other entry's (no two overlap),
//! and their CRC-32 is recorded, so a save can copy them unchanged and check that what it copied
//! has the CRC-32 it had when it was opened.

pub(crate) mod crc32;
mod name;

use crate::error::{OpenError, PartReadError};
use crate::policy::PackageLimits;
use crate::source::{PackageRead, SourceError};
use crc32::Crc32;
use miniz_oxide::inflate::{TINFLStatus, decompress_to_vec_with_limit};
use std::collections::BTreeSet;

const END_SIGNATURE: u32 = 0x0605_4b50;
const ZIP64_LOCATOR_SIGNATURE: u32 = 0x0706_4b50;
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
const DESCRIPTOR_SIGNATURE: u32 = 0x0807_4b50;
const END_LEN: usize = 22;
const ZIP64_LOCATOR_LEN: u64 = 20;
const CENTRAL_LEN: usize = 46;
const LOCAL_LEN: usize = 30;
const MAX_COMMENT: usize = 0xFFFF;
const FLAG_ENCRYPTED: u16 = 1;
const FLAG_DESCRIPTOR: u16 = 1 << 3;
const FLAG_STRONG_ENCRYPTION: u16 = 1 << 6;
const FLAG_UTF8: u16 = 1 << 11;
const FLAG_MASKED_HEADER: u16 = 1 << 13;
/// The offset of the local header offset within a central record.
pub(crate) const CENTRAL_LOCAL_OFFSET_AT: usize = 42;
/// The unit in which stored bytes are read, hashed and copied.
pub(crate) const CHUNK: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Method {
    Stored,
    Deflated,
}

/// One entry of the archive, as its central record declares it and its local header confirms.
pub(crate) struct Entry {
    pub(crate) name: String,
    pub(crate) method: Method,
    pub(crate) crc32: u32,
    pub(crate) compressed_size: u32,
    pub(crate) uncompressed_size: u32,
    pub(crate) local_offset: u32,
    pub(crate) payload_offset: u64,
    /// The end of the entry's stored bytes: its payload, and its data descriptor when it has one.
    pub(crate) span_end: u64,
    /// The CRC-32 of the stored bytes from `local_offset` to `span_end`, as opened.
    pub(crate) span_crc: u32,
    /// The central record's bytes, as opened.
    pub(crate) central_record: Vec<u8>,
}

impl Entry {
    pub(crate) fn is_directory(&self) -> bool {
        self.name.ends_with('/')
    }

    pub(crate) fn span_len(&self) -> u64 {
        self.span_end - u64::from(self.local_offset)
    }
}

/// The archive's entries in central directory order, and its comment.
pub(crate) struct Archive {
    pub(crate) entries: Vec<Entry>,
    pub(crate) comment: Vec<u8>,
}

fn le16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn le32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn read(source: &dyn PackageRead, offset: u64, len: usize) -> Result<Vec<u8>, SourceError> {
    let mut bytes = vec![0; len];
    source.read_at(offset, &mut bytes)?;
    Ok(bytes)
}

fn malformed(offset: u64, detail: &'static str) -> OpenError {
    OpenError::MalformedArchive { offset, detail }
}

fn decode_name(raw: &[u8], flags: u16) -> Option<String> {
    if flags & FLAG_UTF8 != 0 {
        String::from_utf8(raw.to_vec()).ok()
    } else if raw.is_ascii() {
        Some(raw.iter().map(|&b| char::from(b)).collect())
    } else {
        None
    }
}

/// Reads the archive's structure from `source` under `limits`.
pub(crate) fn read_archive(
    source: &dyn PackageRead,
    limits: &PackageLimits,
) -> Result<Archive, OpenError> {
    let length = source.size();
    if length > limits.max_package_bytes {
        return Err(OpenError::PackageTooLarge {
            length,
            limit: limits.max_package_bytes,
        });
    }
    let (end_offset, end) = read_end_record(source, length)?;
    let disk = le16(&end, 4);
    let directory_disk = le16(&end, 6);
    let disk_entries = le16(&end, 8);
    let total = le16(&end, 10);
    let directory_size = le32(&end, 12);
    let directory_offset = le32(&end, 16);
    if disk_entries == u16::MAX
        || total == u16::MAX
        || directory_size == u32::MAX
        || directory_offset == u32::MAX
    {
        return Err(OpenError::Zip64Archive);
    }
    if disk != 0 || directory_disk != 0 || disk_entries != total {
        return Err(OpenError::SpannedArchive);
    }
    if u32::from(total) > limits.max_entries {
        return Err(OpenError::TooManyEntries {
            declared: u64::from(total),
            limit: limits.max_entries,
        });
    }
    let directory_offset = u64::from(directory_offset);
    if directory_offset + u64::from(directory_size) != end_offset {
        if end_offset >= ZIP64_LOCATOR_LEN {
            let locator =
                read(source, end_offset - ZIP64_LOCATOR_LEN, 4).map_err(OpenError::Source)?;
            if le32(&locator, 0) == ZIP64_LOCATOR_SIGNATURE {
                return Err(OpenError::Zip64Archive);
            }
        }
        return Err(malformed(
            end_offset,
            "the central directory does not end where the end record starts",
        ));
    }
    let directory =
        read(source, directory_offset, directory_size as usize).map_err(OpenError::Source)?;
    let mut entries = read_central_directory(&directory, directory_offset, total, limits)?;
    for entry in &mut entries {
        read_local_header(source, entry, directory_offset)?;
    }
    check_overlaps(&entries)?;
    for entry in &mut entries {
        entry.span_crc = span_crc(source, entry).map_err(OpenError::Source)?;
    }
    Ok(Archive {
        entries,
        comment: end[END_LEN..].to_vec(),
    })
}

/// Finds the end-of-central-directory record: the last one whose comment ends exactly at the end
/// of the source. Returns its offset and its bytes, comment included.
fn read_end_record(source: &dyn PackageRead, length: u64) -> Result<(u64, Vec<u8>), OpenError> {
    if length < END_LEN as u64 {
        return Err(OpenError::NotAZipArchive);
    }
    let tail_len = length.min((END_LEN + MAX_COMMENT) as u64) as usize;
    let tail_start = length - tail_len as u64;
    let tail = read(source, tail_start, tail_len).map_err(OpenError::Source)?;
    let at = (0..=tail_len - END_LEN)
        .rev()
        .find(|&at| {
            le32(&tail, at) == END_SIGNATURE
                && usize::from(le16(&tail, at + 20)) == tail_len - at - END_LEN
        })
        .ok_or(OpenError::NotAZipArchive)?;
    Ok((tail_start + at as u64, tail[at..].to_vec()))
}

fn read_central_directory(
    directory: &[u8],
    directory_offset: u64,
    total: u16,
    limits: &PackageLimits,
) -> Result<Vec<Entry>, OpenError> {
    let mut entries = Vec::with_capacity(usize::from(total));
    let mut seen = BTreeSet::new();
    let mut declared_total: u64 = 0;
    let mut at = 0usize;
    for _ in 0..total {
        let record_offset = directory_offset + at as u64;
        let header = directory.get(at..at + CENTRAL_LEN).ok_or_else(|| {
            malformed(record_offset, "the central directory ends inside a record")
        })?;
        if le32(header, 0) != CENTRAL_SIGNATURE {
            return Err(malformed(
                record_offset,
                "a central record lacks its signature",
            ));
        }
        let flags = le16(header, 8);
        let method = le16(header, 10);
        let crc32 = le32(header, 16);
        let compressed_size = le32(header, 20);
        let uncompressed_size = le32(header, 24);
        let name_len = usize::from(le16(header, 28));
        let record_len =
            CENTRAL_LEN + name_len + usize::from(le16(header, 30)) + usize::from(le16(header, 32));
        let disk_start = le16(header, 34);
        let local_offset = le32(header, CENTRAL_LOCAL_OFFSET_AT);
        let record = directory.get(at..at + record_len).ok_or_else(|| {
            malformed(record_offset, "the central directory ends inside a record")
        })?;
        let name = decode_name(&record[CENTRAL_LEN..CENTRAL_LEN + name_len], flags).ok_or(
            OpenError::EntryNameEncoding {
                offset: record_offset,
            },
        )?;
        name::check(&name).map_err(|reason| OpenError::UnsafeEntryName {
            name: name.clone(),
            reason,
        })?;
        if flags & (FLAG_ENCRYPTED | FLAG_STRONG_ENCRYPTION | FLAG_MASKED_HEADER) != 0 {
            return Err(OpenError::EncryptedEntry { name });
        }
        let method = match method {
            0 => Method::Stored,
            8 => Method::Deflated,
            method => return Err(OpenError::UnsupportedCompression { name, method }),
        };
        if compressed_size == u32::MAX || uncompressed_size == u32::MAX || local_offset == u32::MAX
        {
            return Err(OpenError::Zip64Archive);
        }
        if disk_start != 0 {
            return Err(OpenError::SpannedArchive);
        }
        if method == Method::Stored && compressed_size != uncompressed_size {
            return Err(malformed(
                record_offset,
                "a stored entry's compressed and uncompressed sizes differ",
            ));
        }
        if name.ends_with('/') && uncompressed_size != 0 {
            return Err(malformed(record_offset, "a directory entry has content"));
        }
        if u64::from(uncompressed_size) > limits.max_part_bytes {
            return Err(OpenError::PartTooLarge {
                name,
                declared: u64::from(uncompressed_size),
                limit: limits.max_part_bytes,
            });
        }
        declared_total += u64::from(uncompressed_size);
        if !seen.insert(name.to_ascii_lowercase()) {
            return Err(OpenError::DuplicateEntryName { name });
        }
        if u64::from(local_offset) + LOCAL_LEN as u64 > directory_offset {
            return Err(malformed(
                record_offset,
                "a local header offset reaches into the central directory",
            ));
        }
        entries.push(Entry {
            name,
            method,
            crc32,
            compressed_size,
            uncompressed_size,
            local_offset,
            payload_offset: 0,
            span_end: 0,
            span_crc: 0,
            central_record: record.to_vec(),
        });
        at += record_len;
    }
    if at != directory.len() {
        return Err(malformed(
            directory_offset + at as u64,
            "the central directory holds bytes past the records its end record counts",
        ));
    }
    if declared_total > limits.max_expanded_bytes {
        return Err(OpenError::PackageExpansionTooLarge {
            declared: declared_total,
            limit: limits.max_expanded_bytes,
        });
    }
    Ok(entries)
}

/// Checks an entry's local header against its central record and locates its payload and data
/// descriptor, all before the central directory.
fn read_local_header(
    source: &dyn PackageRead,
    entry: &mut Entry,
    directory_offset: u64,
) -> Result<(), OpenError> {
    let offset = u64::from(entry.local_offset);
    let header = read(source, offset, LOCAL_LEN).map_err(OpenError::Source)?;
    if le32(&header, 0) != LOCAL_SIGNATURE {
        return Err(malformed(
            offset,
            "a central record points at no local header",
        ));
    }
    let central = &entry.central_record;
    let flags = le16(&header, 6);
    if flags != le16(central, 8) || le16(&header, 8) != le16(central, 10) {
        return Err(malformed(
            offset,
            "a local header's flags or method differ from its central record's",
        ));
    }
    let name_len = usize::from(le16(&header, 26));
    let name_offset = offset + LOCAL_LEN as u64;
    let payload_offset = name_offset + name_len as u64 + u64::from(le16(&header, 28));
    let payload_end = payload_offset + u64::from(entry.compressed_size);
    if payload_end > directory_offset {
        return Err(malformed(
            offset,
            "an entry's payload runs into the central directory",
        ));
    }
    let central_name = &central[CENTRAL_LEN..CENTRAL_LEN + usize::from(le16(central, 28))];
    let local_name = read(source, name_offset, name_len).map_err(OpenError::Source)?;
    if local_name != central_name {
        return Err(malformed(
            offset,
            "a local header's name differs from its central record's",
        ));
    }
    let descriptor = flags & FLAG_DESCRIPTOR != 0;
    let agrees = |local: u32, declared: u32| local == declared || (descriptor && local == 0);
    if !agrees(le32(&header, 14), entry.crc32)
        || !agrees(le32(&header, 18), entry.compressed_size)
        || !agrees(le32(&header, 22), entry.uncompressed_size)
    {
        return Err(malformed(
            offset,
            "a local header's CRC or sizes differ from its central record's",
        ));
    }
    let mut span_end = payload_end;
    if descriptor {
        let available = (directory_offset - payload_end).min(16) as usize;
        let bytes = read(source, payload_end, available).map_err(OpenError::Source)?;
        let declares = |at: usize| {
            bytes.len() >= at + 12
                && le32(&bytes, at) == entry.crc32
                && le32(&bytes, at + 4) == entry.compressed_size
                && le32(&bytes, at + 8) == entry.uncompressed_size
        };
        if bytes.len() >= 4 && le32(&bytes, 0) == DESCRIPTOR_SIGNATURE && declares(4) {
            span_end += 16;
        } else if declares(0) {
            span_end += 12;
        } else {
            return Err(malformed(
                payload_end,
                "a data descriptor differs from its central record",
            ));
        }
    }
    entry.payload_offset = payload_offset;
    entry.span_end = span_end;
    Ok(())
}

fn check_overlaps(entries: &[Entry]) -> Result<(), OpenError> {
    let mut order: Vec<&Entry> = entries.iter().collect();
    order.sort_by_key(|entry| entry.local_offset);
    for pair in order.windows(2) {
        if u64::from(pair[1].local_offset) < pair[0].span_end {
            return Err(OpenError::OverlappingEntries {
                first: pair[0].name.clone(),
                second: pair[1].name.clone(),
            });
        }
    }
    Ok(())
}

/// The CRC-32 of an entry's stored bytes, from its local header to the end of its span.
pub(crate) fn span_crc(source: &dyn PackageRead, entry: &Entry) -> Result<u32, SourceError> {
    let mut crc = Crc32::new();
    each_chunk(
        source,
        u64::from(entry.local_offset),
        entry.span_end,
        |chunk| {
            crc.update(chunk);
            Ok(())
        },
    )?;
    Ok(crc.finish())
}

/// Reads the source from `start` to `end` in chunks, handing each to `visit`.
pub(crate) fn each_chunk<E: From<SourceError>>(
    source: &dyn PackageRead,
    start: u64,
    end: u64,
    mut visit: impl FnMut(&[u8]) -> Result<(), E>,
) -> Result<(), E> {
    let mut buffer = vec![0; ((end - start) as usize).min(CHUNK)];
    let mut offset = start;
    while offset < end {
        let len = ((end - offset) as usize).min(CHUNK);
        source.read_at(offset, &mut buffer[..len])?;
        visit(&buffer[..len])?;
        offset += len as u64;
    }
    Ok(())
}

/// Reads and decompresses an entry's content, refusing a payload that expands past or short of
/// its declared size or whose CRC-32 is not the declared one.
pub(crate) fn read_content(
    source: &dyn PackageRead,
    entry: &Entry,
) -> Result<Vec<u8>, PartReadError> {
    let raw = read(source, entry.payload_offset, entry.compressed_size as usize)
        .map_err(PartReadError::Source)?;
    let declared = entry.uncompressed_size;
    let content = match entry.method {
        Method::Stored => raw,
        Method::Deflated => inflate(&raw, declared)?,
    };
    if content.len() != declared as usize {
        return Err(PartReadError::ShorterThanDeclared {
            declared: u64::from(declared),
            actual: content.len() as u64,
        });
    }
    let actual = crc32::crc32(&content);
    if actual != entry.crc32 {
        return Err(PartReadError::CrcMismatch {
            declared: entry.crc32,
            actual,
        });
    }
    Ok(content)
}

/// Decodes a raw DEFLATE payload into at most `declared` bytes. The decoder's output buffer
/// never grows past that size: a stream that would produce more stops there with
/// `HasMoreOutput`, so a small payload cannot expand into memory the declared size (checked
/// against the limits at open) did not admit.
fn inflate(raw: &[u8], declared: u32) -> Result<Vec<u8>, PartReadError> {
    decompress_to_vec_with_limit(raw, declared as usize).map_err(|error| match error.status {
        TINFLStatus::HasMoreOutput => PartReadError::ExpandsPastDeclaredSize {
            declared: u64::from(declared),
        },
        TINFLStatus::FailedCannotMakeProgress => PartReadError::CorruptDeflate {
            detail: "the stream ends before its final block does",
        },
        TINFLStatus::Failed => PartReadError::CorruptDeflate {
            detail: "the stream is not valid DEFLATE",
        },
        // Raw DEFLATE without the more-input flag reports none of the other statuses; one that
        // did is still a refusal, never content.
        _ => PartReadError::CorruptDeflate {
            detail: "the decoder stopped with a status raw DEFLATE does not produce",
        },
    })
}
