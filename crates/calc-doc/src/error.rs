//! The typed failures of opening a package and of reading one of its parts.

use crate::source::SourceError;
use crate::xml::XmlError;
use std::fmt;

/// Why an entry name could be read as a path outside the package, or as something other than a
/// part name. Nothing is ever written under these names; the package is refused instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsafeName {
    /// The name is empty.
    Empty,
    /// The name starts with `/`, so it reads as a path from a file system root.
    Absolute,
    /// The name holds `\`, which Windows reads as a path separator.
    Backslash,
    /// The name holds `:`, which reads as a drive (`C:`) or an alternate data stream.
    Colon,
    /// The name holds a control character (below U+0020, or U+007F).
    ControlCharacter,
    /// A segment of the name is `.` or `..`.
    DotSegment,
    /// The name holds an empty segment (`//`).
    EmptySegment,
}

/// Why [`crate::open`] refused a package. Every refusal happens before any part is decompressed,
/// except a failure of the package's own structural parts (`[Content_Types].xml` and the
/// relationship parts), which opening reads.
#[derive(Debug, PartialEq, Eq)]
pub enum OpenError {
    /// The backing source failed a read.
    Source(SourceError),
    /// The source is longer than [`crate::PackageLimits::max_package_bytes`].
    PackageTooLarge {
        /// Package length in bytes.
        length: u64,
        /// Maximum permitted package length.
        limit: u64,
    },
    /// No end-of-central-directory record ends the source, so it is not a ZIP archive.
    NotAZipArchive,
    /// The archive uses ZIP64 records or sizes, which this reader does not read.
    Zip64Archive,
    /// The archive spans more than one disk.
    SpannedArchive,
    /// An entry is encrypted.
    EncryptedEntry {
        /// Encrypted ZIP entry name.
        name: String,
    },
    /// An entry uses a compression method other than stored (0) or deflated (8).
    UnsupportedCompression {
        /// ZIP entry name.
        name: String,
        /// Compression method identifier.
        method: u16,
    },
    /// The archive's records contradict each other or point outside the archive.
    MalformedArchive {
        /// Byte offset of the malformed record.
        offset: u64,
        /// Archive validation diagnostic.
        detail: &'static str,
    },
    /// The archive declares more entries than [`crate::PackageLimits::max_entries`].
    TooManyEntries {
        /// Entry count declared by the archive.
        declared: u64,
        /// Maximum permitted entry count.
        limit: u32,
    },
    /// An entry's name could be read as a path outside the package.
    UnsafeEntryName {
        /// Entry name as declared in the directory.
        name: String,
        /// Reason the name is unsafe.
        reason: UnsafeName,
    },
    /// An entry's name is not UTF-8 (with the UTF-8 flag) or not ASCII (without it).
    EntryNameEncoding {
        /// Byte offset of the entry name.
        offset: u64,
    },
    /// Two entries have names equal apart from ASCII case, which OPC treats as the same part.
    DuplicateEntryName {
        /// Duplicate ZIP entry name.
        name: String,
    },
    /// An entry declares more uncompressed bytes than [`crate::PackageLimits::max_part_bytes`].
    PartTooLarge {
        /// Name of the affected entry.
        name: String,
        /// Uncompressed length declared for the entry.
        declared: u64,
        /// Maximum uncompressed entry length.
        limit: u64,
    },
    /// The entries together declare more uncompressed bytes than
    /// [`crate::PackageLimits::max_expanded_bytes`].
    PackageExpansionTooLarge {
        /// Aggregate uncompressed size declared by the archive.
        declared: u64,
        /// Maximum total uncompressed size.
        limit: u64,
    },
    /// Two entries' stored bytes overlap, so one payload would be read twice.
    OverlappingEntries {
        /// Name of the first overlapping entry.
        first: String,
        /// Name of the second overlapping entry.
        second: String,
    },
    /// The package has no `[Content_Types].xml`.
    MissingContentTypes,
    /// A structural part could not be read.
    Part {
        /// Name of the package part.
        part: String,
        /// Reason the part could not be read.
        error: PartReadError,
    },
    /// A structural part is not acceptable XML.
    Xml {
        /// Name of the package part.
        part: String,
        /// XML decoding or validation error.
        error: XmlError,
    },
    /// `[Content_Types].xml` does not have the shape OPC gives it.
    InvalidContentTypes {
        /// Content types validation diagnostic.
        detail: &'static str,
    },
    /// A relationships part does not have the shape OPC gives it.
    InvalidRelationships {
        /// Part containing the relationship declarations.
        part: String,
        /// Relationship validation diagnostic.
        detail: &'static str,
    },
    /// An Excel binary workbook (.xlsb) holds a part, a record or a formula token its conversion
    /// to SpreadsheetML does not read (bead dc-5sr): `part` names the part, `detail` what in it.
    BinaryWorkbook {
        /// Name of the binary workbook part.
        part: String,
        /// Binary workbook diagnostic.
        detail: String,
    },
    /// A legacy Excel workbook (.xls, .xla, .xlt; dc-4j0e) holds a stream, a record or a form its
    /// conversion to SpreadsheetML does not read: `stream` names the stream, `detail` what in it.
    LegacyWorkbook {
        /// Name of the legacy workbook stream.
        stream: String,
        /// Legacy workbook diagnostic.
        detail: String,
    },
    /// A legacy Excel workbook is encrypted and the password it needs was not given
    /// ([`crate::open_with_password`] takes one).
    PasswordRequired,
    /// The password given does not open the legacy Excel workbook.
    WrongPassword,
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(f, "the package source failed: {error}"),
            Self::PackageTooLarge { length, limit } => write!(
                f,
                "the package is {length} bytes long, over the limit of {limit}"
            ),
            Self::NotAZipArchive => f.write_str("the source is not a ZIP archive"),
            Self::Zip64Archive => f.write_str("the archive uses ZIP64, which is not read"),
            Self::SpannedArchive => f.write_str("the archive spans more than one disk"),
            Self::EncryptedEntry { name } => write!(f, "the entry {name:?} is encrypted"),
            Self::UnsupportedCompression { name, method } => write!(
                f,
                "the entry {name:?} uses compression method {method}, which is not read"
            ),
            Self::MalformedArchive { offset, detail } => {
                write!(f, "the archive is malformed at byte {offset}: {detail}")
            }
            Self::TooManyEntries { declared, limit } => write!(
                f,
                "the archive declares {declared} entries, over the limit of {limit}"
            ),
            Self::UnsafeEntryName { name, reason } => {
                write!(f, "the entry name {name:?} is refused: {reason:?}")
            }
            Self::EntryNameEncoding { offset } => write!(
                f,
                "the entry name in the central record at byte {offset} is not readable text"
            ),
            Self::DuplicateEntryName { name } => {
                write!(f, "two entries are named {name:?} (ignoring ASCII case)")
            }
            Self::PartTooLarge {
                name,
                declared,
                limit,
            } => write!(
                f,
                "the entry {name:?} declares {declared} bytes, over the limit of {limit}"
            ),
            Self::PackageExpansionTooLarge { declared, limit } => write!(
                f,
                "the entries declare {declared} bytes in all, over the limit of {limit}"
            ),
            Self::OverlappingEntries { first, second } => {
                write!(f, "the entries {first:?} and {second:?} overlap")
            }
            Self::MissingContentTypes => f.write_str("the package has no [Content_Types].xml"),
            Self::Part { part, error } => write!(f, "the part {part:?} could not be read: {error}"),
            Self::Xml { part, error } => write!(f, "the part {part:?} is refused: {error}"),
            Self::InvalidContentTypes { detail } => {
                write!(f, "[Content_Types].xml is invalid: {detail}")
            }
            Self::InvalidRelationships { part, detail } => {
                write!(f, "the relationships part {part:?} is invalid: {detail}")
            }
            Self::PasswordRequired => f.write_str("the workbook is encrypted and needs a password"),
            Self::WrongPassword => f.write_str("the password does not open the workbook"),
            Self::LegacyWorkbook { stream, detail } => write!(
                f,
                "the legacy workbook stream {stream:?} holds {detail}, which is not read"
            ),
            Self::BinaryWorkbook { part, detail } => write!(
                f,
                "the binary workbook part {part:?} holds {detail}, which is not read"
            ),
        }
    }
}

impl std::error::Error for OpenError {}

/// Why a part's content could not be read.
#[derive(Debug, PartialEq, Eq)]
pub enum PartReadError {
    /// The package has no part of that name.
    NotAPart {
        /// Name that did not identify a readable package part.
        name: String,
    },
    /// The backing source failed a read.
    Source(SourceError),
    /// The entry's deflated payload is larger than its declared size allows: `compressed` bytes,
    /// past the `bound` that the expansion of stored or fixed Huffman blocks and a fixed allowance
    /// for flush markers give its `declared` uncompressed bytes. Refused before the part read
    /// reads any of the payload (open has already streamed it for its CRC).
    CompressedPayloadTooLarge {
        /// Compressed length declared for the entry.
        compressed: u64,
        /// Uncompressed length declared for the entry.
        declared: u64,
        /// Maximum expansion permitted for this compressed length.
        bound: u64,
    },
    /// The compressed payload is not valid DEFLATE.
    CorruptDeflate {
        /// Diagnostic from processing the DEFLATE stream.
        detail: &'static str,
    },
    /// The payload holds bytes after its final DEFLATE block: the stream ends within the first
    /// `consumed` of its `compressed` bytes.
    BytesAfterFinalBlock {
        /// Compressed length declared for the entry.
        compressed: u64,
        /// Number of bytes consumed by the stream.
        consumed: u64,
    },
    /// The payload expands past the size its entry declares; decoding stopped there.
    ExpandsPastDeclaredSize {
        /// Declared uncompressed length.
        declared: u64,
    },
    /// The payload expands to fewer bytes than its entry declares.
    ShorterThanDeclared {
        /// Declared uncompressed length.
        declared: u64,
        /// Number of bytes produced by decompression.
        actual: u64,
    },
    /// The content's CRC-32 is not the one its entry declares.
    CrcMismatch {
        /// CRC recorded by the archive.
        declared: u32,
        /// CRC calculated from the decoded bytes.
        actual: u32,
    },
}

impl fmt::Display for PartReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAPart { name } => write!(f, "the package has no part {name:?}"),
            Self::Source(error) => write!(f, "the package source failed: {error}"),
            Self::CompressedPayloadTooLarge {
                compressed,
                declared,
                bound,
            } => write!(
                f,
                "the deflated payload is {compressed} bytes, over the {bound} bytes its declared {declared} bytes allow"
            ),
            Self::CorruptDeflate { detail } => write!(f, "the payload is not DEFLATE: {detail}"),
            Self::BytesAfterFinalBlock {
                compressed,
                consumed,
            } => write!(
                f,
                "the payload's final DEFLATE block ends after {consumed} of its {compressed} bytes"
            ),
            Self::ExpandsPastDeclaredSize { declared } => {
                write!(f, "the payload expands past its declared {declared} bytes")
            }
            Self::ShorterThanDeclared { declared, actual } => write!(
                f,
                "the payload expands to {actual} bytes, not its declared {declared}"
            ),
            Self::CrcMismatch { declared, actual } => write!(
                f,
                "the content's CRC-32 is {actual:08x}, not its declared {declared:08x}"
            ),
        }
    }
}

impl std::error::Error for PartReadError {}
