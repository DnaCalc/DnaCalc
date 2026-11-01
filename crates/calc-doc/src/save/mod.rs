//! Saving: a plan of how each entry is written, then the writing.
//!
//! A plan copies each entry it does not rewrite as its stored bytes (local header, compressed
//! payload and data descriptor), unchanged, and writes a central directory whose records for
//! those entries are the opened ones with only their local header offsets changed. Bytes between
//! entries that no entry stores are not copied, and the receipt counts them
//! ([`SaveReceipt::gap_bytes_dropped`]). An authored save ([`author`]) also rewrites parts:
//! each is serialized and compressed while the plan is prepared, takes its source entry's place in
//! both orders, and gets a new local header and central record; a part it adds is written after
//! the others. A revision whose skin layout differs from the loaded one rewrites or adds the DNA
//! view manifest ([`manifest`]) and adds or removes its blocks' locator names. [`write`] streams
//! the plan to the sink entry by entry.

mod array_metadata;
mod author;
pub(crate) mod conditional;
pub use conditional::{conditional_rule, new_conditional_rule};
mod diff;
pub(crate) mod file_form;
mod geometry;
mod manifest;
mod panes;
mod parts;
mod reader;
mod sheet;
mod sheets;
mod strings;
mod structural;
#[cfg(test)]
mod tests;
pub(crate) mod text;

use crate::error::PartReadError;
use crate::load::LoadedSheet;
use crate::load::manifest::ManifestState;
use crate::opc::Package;
use crate::source::{PackageWrite, SinkError, SourceError};
use crate::zip::crc32::{self, Crc32};
use crate::zip::{self, CENTRAL_LOCAL_OFFSET_AT, Entry, Method};
use calc_doc_model::value::{CellKey, GridArea};
use calc_doc_model::{AuthoredRevision, CachedValue, ModelRoot};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

pub use structural::StructuralEdits;

/// An immutable view of a document, which a save is prepared from: an opened package as it was
/// opened, or a revision of a loaded workbook's model with the records it was loaded with.
#[derive(Clone)]
pub struct DocumentSnapshot {
    package: Arc<Package>,
    edit: Option<Edited>,
}

/// Cache metadata for a noncalculating save. Imported caches stay unverified on reopen.
#[derive(Clone)]
pub(crate) struct CalculationCacheState {
    pub(crate) dirty: BTreeSet<CellKey>,
    pub(crate) completed: bool,
    pub(crate) mode: crate::CalcMode,
}

/// A revision of a loaded workbook's model and what the save rewrites it against.
#[derive(Clone)]
pub(crate) struct Edited {
    workbook_part: String,
    sheets: Arc<[LoadedSheet]>,
    /// Whether and where the loaded package's DNA manifest was read.
    manifest: ManifestState,
    /// The model as loaded.
    base: Arc<ModelRoot>,
    /// The revision to save.
    model: Arc<ModelRoot>,
    /// Supplied cached values selected by the caller's policy; these grant no calculation or
    /// freshness proof. `None` saves without them.
    values: Option<Arc<BTreeMap<CellKey, CachedValue>>>,
    /// The area each array formula's result fills, by anchor
    /// ([`crate::LoadedWorkbook::snapshot_with_results`]).
    spills: Arc<BTreeMap<CellKey, GridArea>>,
    /// The rows and columns inserted and deleted since the load, which the save moves the
    /// package's rows, columns and what lies on them by ([`DocumentSnapshot::with_structural_edits`]).
    structural: Option<StructuralEdits>,
    cache_state: Option<CalculationCacheState>,
}

impl DocumentSnapshot {
    pub(crate) fn new(package: Arc<Package>) -> Self {
        DocumentSnapshot {
            package,
            edit: None,
        }
    }

    pub(crate) fn edited(
        package: Arc<Package>,
        workbook_part: String,
        sheets: Arc<[LoadedSheet]>,
        manifest: ManifestState,
        base: Arc<ModelRoot>,
        model: Arc<ModelRoot>,
        values: Option<Arc<BTreeMap<CellKey, CachedValue>>>,
        spills: Arc<BTreeMap<CellKey, GridArea>>,
    ) -> Self {
        DocumentSnapshot {
            package,
            edit: Some(Edited {
                workbook_part,
                sheets,
                manifest,
                base,
                model,
                values,
                spills,
                structural: None,
                cache_state: None,
            }),
        }
    }

    /// The snapshot of a revision that rows or columns were inserted into or deleted from since
    /// the load: `edits` lists them in order. The save moves the package's rows and cells, and
    /// what names their positions (merged cells, conditional formats, data validations,
    /// hyperlinks, filters, page breaks, views, column spans, comments, notes, drawings, tables,
    /// charts' series and defined names), as Excel's insertion and deletion move them, before it
    /// writes the revision's cells (dc-jhn); what it does not move it refuses
    /// ([`SaveRefusal::StructuralNotMoved`]). The edits' formulas outside cells are rewritten by
    /// their rewriter. A snapshot of a package as opened, or edits that are empty, are left as
    /// they are.
    pub fn with_structural_edits(mut self, edits: StructuralEdits) -> Self {
        if let Some(edit) = self.edit.as_mut()
            && !edits.edits().is_empty()
        {
            edit.structural = Some(edits);
        }
        self
    }

    /// Marks retained scalar caches and the calculation settings of a Save(false) profile.
    /// This does not calculate values or assert that imported values are current.
    pub fn with_calculation_cache_state(
        mut self,
        dirty: BTreeSet<CellKey>,
        completed: bool,
        mode: crate::CalcMode,
    ) -> Self {
        if let Some(edit) = self.edit.as_mut() {
            edit.cache_state = Some(CalculationCacheState {
                dirty,
                completed,
                mode,
            });
        }
        self
    }

    pub fn package(&self) -> &Package {
        &self.package
    }

    /// The model revision the snapshot saves; `None` for an opened package saved as opened.
    pub fn model(&self) -> Option<&Arc<ModelRoot>> {
        self.edit.as_ref().map(|edit| &edit.model)
    }
}

/// The package format a save writes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SaveFormat {
    /// SpreadsheetML (.xlsx/.xlsm), including conversion of XLSB inputs.
    #[default]
    Xml,
    /// Preserve an existing XLSB source when the snapshot needs no part rewrites.
    /// Binary authoring remains owned by dc-snz.14.
    Xlsb,
}

/// The format and bounds a save is planned under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavePolicy {
    pub format: SaveFormat,
    /// The longest package written, in bytes. A package past 4 GiB - 1 byte, the most a ZIP
    /// archive without ZIP64 records addresses, is refused whatever this says, since this writer
    /// writes no ZIP64 records.
    pub max_package_bytes: u64,
}

impl Default for SavePolicy {
    fn default() -> Self {
        SavePolicy {
            format: SaveFormat::Xml,
            max_package_bytes: u64::from(u32::MAX),
        }
    }
}

/// Why [`prepare_save`] refused a snapshot. A refusal is decided before any byte is written: no
/// plan exists, the package the document was opened from is unchanged, and so is the model
/// revision the caller holds.
#[derive(Debug, PartialEq, Eq)]
pub enum SaveRefusal {
    /// XLSB output needs the validated original binary package (dc-snz.14).
    BinarySourceRequired,
    /// The XML authoring route found part rewrites, for which no binary writer exists yet.
    BinaryChangesNotModelled { owner: &'static str },
    /// The snapshot's model is not a revision of the loaded workbook: it is not of the lineage of
    /// the model the load made (a model another load made, of the same file or with the same
    /// sheet IDs, included), or its sheets differ from the loaded ones.
    ForeignModel,
    /// A part the save rewrites could not be read from the source, such as a payload changed
    /// since the package was opened.
    Part { part: String, error: PartReadError },
    /// A part the save rewrites is not shaped as this writer rewrites it, such as a workbook part
    /// with two `calcPr` elements.
    InvalidPart { part: String, detail: String },
    /// A changed cell lies in the header row or the totals row of a table (ListObject) of its
    /// sheet. The table part records the names of its header row and the functions of its totals
    /// row, which the change could contradict; authoring tables is not built (no bead owns it
    /// yet). A change in a
    /// table's data rows is saved.
    EditInTable {
        part: String,
        table: String,
        row: u32,
        column: u32,
    },
    /// Two sheets of the workbook are this one part, which the save would rewrite for one of them.
    SharedSheetPart { part: String },
    /// The revision changes a cell, a format or the rows or columns of an Excel 4.0 macro sheet
    /// (`part`), whose part this writer does not rewrite, or inserts or deletes rows or columns
    /// of a workbook with one, whose formulas' references the save cannot move: a macro sheet is
    /// saved as it was read (dc-xuw).
    MacroSheetChanged { part: String },
    /// A formula the revision sets in a cell (of the worksheet part `part`) is longer than
    /// [`crate::FORMULA_MAX_UTF16_CODE_UNITS`], so [`crate::load`] would refuse the saved package.
    /// [`crate::prepare_edit`] refuses such a formula; this refuses one in a revision made
    /// another way, such as by [`calc_doc_model::ModelRoot::apply`].
    FormulaTooLong {
        part: String,
        row: u32,
        column: u32,
        units: usize,
    },
    /// CalcFml's parser reads a formula the revision sets in a cell (of the worksheet part
    /// `part`) with a diagnostic, as [`crate::prepare_edit`] would refuse it
    /// ([`crate::EditError::FormulaSyntax`]).
    FormulaSyntax {
        part: String,
        row: u32,
        column: u32,
        message: String,
    },
    /// The package would be longer than the policy's bound, or than a ZIP archive without ZIP64
    /// records addresses, or hold more entries than one counts.
    PackageTooLarge {
        bytes: u64,
        entries: usize,
        limit: u64,
    },
    /// The revision's skin layout differs from the loaded one, but the package's DNA view manifest
    /// at `part` was not read ([`crate::LayoutWarning`]): the save would have to overwrite a
    /// manifest this codec does not read. A save that leaves the layout as loaded copies the
    /// manifest's parts as they are.
    LayoutNotWritable { part: String },
    /// A block the revision creates has the locator name `name`, which the workbook part `part`
    /// already holds as a workbook-scoped defined name, compared ignoring ASCII case.
    LocatorNameTaken { part: String, name: String },
    /// A value given to save as a cached result (of the worksheet part `part`) is for a cell the
    /// revision holds no formula in.
    ValueWithoutFormula { part: String, row: u32, column: u32 },
    /// A formula the revision sets in a cell (of the worksheet part `part`) has no file form
    /// here ([`crate::FileFormError`]): its text is not readable, or it nests deeper than the
    /// converter walks.
    FormulaFileForm {
        part: String,
        row: u32,
        column: u32,
        error: crate::FileFormError,
    },
    /// The revision sets an error constant (in a cell of the worksheet part `part`) that Excel
    /// does not hold in a cell ([`crate::EditError::ErrorConstant`]).
    ErrorConstant {
        part: String,
        row: u32,
        column: u32,
        code: calc_doc_model::value::WorksheetErrorCode,
    },
    /// Rows or columns were inserted or deleted, and the part `part` holds `what`, which names
    /// positions of an edited sheet that the save does not move
    /// ([`DocumentSnapshot::with_structural_edits`]): it would be written where it was.
    StructuralNotMoved { part: String, what: String },
}

impl fmt::Display for SaveRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BinarySourceRequired => {
                f.write_str("XLSB output requires an original XLSB package (dc-snz.14)")
            }
            Self::BinaryChangesNotModelled { owner } => write!(
                f,
                "XLSB output for a snapshot requiring part rewrites is not modelled ({owner})"
            ),
            Self::ForeignModel => f.write_str("the model is not a revision of the loaded workbook"),
            Self::Part { part, error } => write!(f, "the part {part:?} could not be read: {error}"),
            Self::InvalidPart { part, detail } => {
                write!(f, "the part {part:?} cannot be rewritten: {detail}")
            }
            Self::EditInTable {
                part,
                table,
                row,
                column,
            } => write!(
                f,
                "{part}: the changed cell at row {row} column {column} lies in a header or totals row of the table {table:?}, which is not authored yet"
            ),
            Self::SharedSheetPart { part } => {
                write!(
                    f,
                    "two sheets are the part {part:?}, which the save would rewrite"
                )
            }
            Self::MacroSheetChanged { part } => write!(
                f,
                "the revision changes the Excel 4.0 macro sheet {part:?}, which is saved only as it was read"
            ),
            Self::FormulaTooLong {
                part,
                row,
                column,
                units,
            } => write!(
                f,
                "{part}: the formula set at row {row} column {column} is {units} UTF-16 code units, more than {}",
                crate::FORMULA_MAX_UTF16_CODE_UNITS
            ),
            Self::FormulaSyntax {
                part,
                row,
                column,
                message,
            } => write!(
                f,
                "{part}: the parser reads the formula set at row {row} column {column} with a diagnostic: {message}"
            ),
            Self::PackageTooLarge {
                bytes,
                entries,
                limit,
            } => write!(
                f,
                "the package would be {bytes} bytes in {entries} entries, over the limit of {limit} bytes or 65534 entries"
            ),
            Self::LayoutNotWritable { part } => write!(
                f,
                "the layout changed, but the view manifest {part:?} was not read and would be overwritten"
            ),
            Self::ValueWithoutFormula { part, row, column } => write!(
                f,
                "{part}: a calculated value was given for row {row} column {column}, which holds no formula"
            ),
            Self::LocatorNameTaken { part, name } => write!(
                f,
                "{part}: the locator name {name:?} is already a defined name of the workbook"
            ),
            Self::ErrorConstant {
                part,
                row,
                column,
                code,
            } => write!(
                f,
                "{part}: the error {code:?} set at row {row} column {column} is not a constant Excel holds in a cell"
            ),
            Self::StructuralNotMoved { part, what } => write!(
                f,
                "{part}: rows or columns were inserted or deleted, and {what} is not moved with them"
            ),
            Self::FormulaFileForm {
                part,
                row,
                column,
                error,
            } => write!(
                f,
                "{part}: the formula set at row {row} column {column} has no file form: {error:?}"
            ),
        }
    }
}

impl std::error::Error for SaveRefusal {}

/// An entry whose bytes the plan writes anew: a rewritten part, or one the save adds.
struct Written {
    name: Vec<u8>,
    method: Method,
    crc32: u32,
    size: u32,
    payload: Vec<u8>,
    flags: u16,
    time: u16,
    date: u16,
    made_by: u16,
    internal: u16,
    external: u32,
    comment: Vec<u8>,
}

fn le16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn le32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// The language encoding flag: the name is UTF-8.
const FLAG_UTF8: u16 = 1 << 11;
/// The version of the ZIP specification an entry needs: 2.0, for deflate.
const VERSION_NEEDED: u16 = 20;
/// The DOS date of an added entry: 1980-01-01, the earliest a DOS date holds.
const DOS_DATE_1980: u16 = 0x0021;

impl Written {
    /// `content` compressed by `method`, with its CRC-32 and size; `None` when its size does not
    /// fit a ZIP entry without ZIP64 records.
    fn content(content: &[u8], method: Method) -> Option<(Vec<u8>, u32, u32)> {
        let size = u32::try_from(content.len()).ok()?;
        let payload = match method {
            Method::Stored => content.to_vec(),
            Method::Deflated => miniz_oxide::deflate::compress_to_vec(content, 6),
        };
        Some((payload, crc32::crc32(content), size))
    }

    /// A rewritten part: its source entry's name, timestamps, attributes and comment, and its
    /// compression method, with the new content.
    fn replacing(entry: &Entry, content: &[u8]) -> Option<Written> {
        let record = &entry.central_record;
        let name_len = usize::from(le16(record, 28));
        let extra_len = usize::from(le16(record, 30));
        let comment_len = usize::from(le16(record, 32));
        let comment_at = 46 + name_len + extra_len;
        let (payload, crc32, size) = Self::content(content, entry.method)?;
        Some(Written {
            name: record[46..46 + name_len].to_vec(),
            method: entry.method,
            crc32,
            size,
            payload,
            flags: le16(record, 8) & FLAG_UTF8,
            time: le16(record, 12),
            date: le16(record, 14),
            made_by: le16(record, 4),
            internal: le16(record, 36),
            external: le32(record, 38),
            comment: record[comment_at..comment_at + comment_len].to_vec(),
        })
    }

    /// A part the save adds, deflated.
    fn added(name: &str, content: &[u8]) -> Option<Written> {
        let (payload, crc32, size) = Self::content(content, Method::Deflated)?;
        Some(Written {
            name: name.as_bytes().to_vec(),
            method: Method::Deflated,
            crc32,
            size,
            payload,
            flags: if name.is_ascii() { 0 } else { FLAG_UTF8 },
            time: 0,
            date: DOS_DATE_1980,
            made_by: VERSION_NEEDED,
            internal: 0,
            external: 0,
            comment: Vec::new(),
        })
    }

    fn method_code(&self) -> u16 {
        match self.method {
            Method::Stored => 0,
            Method::Deflated => 8,
        }
    }

    fn local_header(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(30 + self.name.len());
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        for field in [
            VERSION_NEEDED,
            self.flags,
            self.method_code(),
            self.time,
            self.date,
        ] {
            out.extend_from_slice(&field.to_le_bytes());
        }
        for field in [self.crc32, self.payload.len() as u32, self.size] {
            out.extend_from_slice(&field.to_le_bytes());
        }
        out.extend_from_slice(&(self.name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&self.name);
        out
    }

    fn stored_len(&self) -> u64 {
        30 + self.name.len() as u64 + self.payload.len() as u64
    }

    fn central_len(&self) -> u64 {
        46 + self.name.len() as u64 + self.comment.len() as u64
    }

    fn central_record(&self, offset: u32) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.central_len() as usize);
        out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        for field in [
            self.made_by,
            VERSION_NEEDED,
            self.flags,
            self.method_code(),
            self.time,
            self.date,
        ] {
            out.extend_from_slice(&field.to_le_bytes());
        }
        for field in [self.crc32, self.payload.len() as u32, self.size] {
            out.extend_from_slice(&field.to_le_bytes());
        }
        for field in [
            self.name.len() as u16,
            0,
            self.comment.len() as u16,
            0,
            self.internal,
        ] {
            out.extend_from_slice(&field.to_le_bytes());
        }
        out.extend_from_slice(&self.external.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&self.name);
        out.extend_from_slice(&self.comment);
        out
    }
}

/// The end-of-central-directory record of an archive of `count` entries whose central directory
/// is `size` bytes at `offset`, with the archive's `comment`.
fn end_record(count: u16, size: u32, offset: u32, comment: &[u8]) -> Vec<u8> {
    let mut end = Vec::with_capacity(22 + comment.len());
    end.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    end.extend_from_slice(&[0, 0, 0, 0]);
    end.extend_from_slice(&count.to_le_bytes());
    end.extend_from_slice(&count.to_le_bytes());
    end.extend_from_slice(&size.to_le_bytes());
    end.extend_from_slice(&offset.to_le_bytes());
    end.extend_from_slice(&(comment.len() as u16).to_le_bytes());
    end.extend_from_slice(comment);
    end
}

/// A new archive of `entries` (name, content), in order, each written as a save writes a part it
/// adds ([`Written::added`]); `None` when an entry or the archive is too large for a ZIP without
/// ZIP64 records. The package of a new workbook ([`crate::blank_package`]) is made with it.
pub(crate) fn new_archive(entries: &[(&str, &[u8])]) -> Option<Vec<u8>> {
    let written: Vec<Written> = entries
        .iter()
        .map(|(name, content)| Written::added(name, content))
        .collect::<Option<_>>()?;
    let mut out = Vec::new();
    let mut offsets = Vec::with_capacity(written.len());
    for entry in &written {
        offsets.push(u32::try_from(out.len()).ok()?);
        out.extend_from_slice(&entry.local_header());
        out.extend_from_slice(&entry.payload);
    }
    let directory_offset = u32::try_from(out.len()).ok()?;
    for (entry, offset) in written.iter().zip(offsets) {
        out.extend_from_slice(&entry.central_record(offset));
    }
    let directory_size = u32::try_from(out.len()).ok()? - directory_offset;
    let count = u16::try_from(written.len()).ok()?;
    out.extend_from_slice(&end_record(count, directory_size, directory_offset, &[]));
    Some(out)
}

/// One entry of the saved package.
enum Planned {
    /// The source entry at this index, copied as its stored bytes.
    Copy(usize),
    /// Bytes written anew, in place of the source entry at `source` or, for `None`, added.
    Write {
        source: Option<usize>,
        written: Written,
    },
}

impl Planned {
    fn stored_len(&self, entries: &[Entry]) -> u64 {
        match self {
            Planned::Copy(entry) => entries[*entry].span_len(),
            Planned::Write { written, .. } => written.stored_len(),
        }
    }

    fn central_len(&self, entries: &[Entry]) -> u64 {
        match self {
            Planned::Copy(entry) => entries[*entry].central_record.len() as u64,
            Planned::Write { written, .. } => written.central_len(),
        }
    }
}

/// How a snapshot will be written.
pub struct SavePlan {
    package: Arc<Package>,
    /// The saved package's entries, in central directory order.
    entries: Vec<Planned>,
    /// The entries in the order their bytes are written, each with its offset.
    stored: Vec<(usize, u32)>,
    revision: Option<AuthoredRevision>,
    caches_dropped: usize,
    sheets_rewritten_for_caches: usize,
    values_written: usize,
    values_left_to_excel: usize,
}

impl SavePlan {
    /// The number of entries whose stored bytes the plan copies unchanged.
    pub fn copied_entries(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| matches!(entry, Planned::Copy(_)))
            .count()
    }
}

/// What a completed write produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveReceipt {
    pub bytes_written: u64,
    /// The entries copied as their stored bytes.
    pub entries_copied: usize,
    /// The parts serialized anew: rewritten or added.
    pub parts_written: usize,
    /// The model revision saved; `None` for an opened package saved as opened.
    pub revision: Option<AuthoredRevision>,
    /// The formula cells the edit did not change whose file-cached result the save left out:
    /// after any authored edit, every one the caller gives no value for, since a cached result is
    /// not known to be current. A save with the values the engine calculated
    /// ([`crate::LoadedWorkbook::snapshot_with_results`], calc-host's save) writes them instead.
    pub caches_dropped: usize,
    /// The worksheet parts rewritten that hold no changed cell, only to leave out or replace
    /// their formulas' cached results.
    pub sheets_rewritten_for_caches: usize,
    /// The formula cells written with a supplied cached value; this count grants no freshness
    /// ([`crate::LoadedWorkbook::snapshot_with_values`]).
    pub values_written: usize,
    /// The formula cells of a save with values written without a cached value, for Excel to
    /// calculate when it opens the package (measured: Excel 16.0.20430 calculates an ordinary
    /// formula cell without `<v>` on open, and an array formula without one only under
    /// `fullCalcOnLoad`, which the save then sets, tests/fixtures/d1/excel-open.json): those the
    /// caller gave no value for (a formula it refused) and those whose value is #BUSY!, whose
    /// value metadata is not written.
    pub values_left_to_excel: usize,
    /// The bytes of the opened package that lie before its central directory in no entry's
    /// stored bytes (before the first entry, between entries or after the last), which the save
    /// did not write: it writes each entry's stored bytes, not the bytes around them. No entry
    /// reaches these bytes, so no part loses content, but a byte-level comparison with the opened
    /// package finds them gone: a save of the package as opened is this many bytes shorter.
    pub gap_bytes_dropped: u64,
}

/// Why a write failed. A source that failed a read or no longer holds an entry's stored bytes as
/// opened is found before the first byte is written, by the check [`write`] makes of every entry
/// it copies, and the sink then holds nothing. A failure after that check (a sink that refuses
/// bytes, or a source that changes or fails while the write runs) leaves an incomplete package in
/// the sink, which must be discarded.
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

/// Plans the save of `snapshot` under `policy`. Every entry the save does not rewrite is copied,
/// in the order its bytes were stored. For a revision of a loaded workbook whose cells differ from
/// the loaded ones, the changed worksheet parts, the shared strings, the workbook part and, when
/// the set of formula cells changed, the relationships and content types without the calculation
/// chain are serialized here; for one whose skin layout differs, the view manifest (rewritten, or
/// added with its item properties, its relationships, a relationship from the workbook part and
/// its content types) and the workbook part's locator names. A refusal comes before any of it is
/// written.
pub fn prepare_save(
    snapshot: &DocumentSnapshot,
    policy: &SavePolicy,
) -> Result<SavePlan, SaveRefusal> {
    let converted = &snapshot.package;
    let changes = match &snapshot.edit {
        Some(edit) => author::plan(converted, edit)?,
        None => author::Changes::default(),
    };
    let package = match policy.format {
        SaveFormat::Xml => converted,
        SaveFormat::Xlsb => {
            let original = converted
                .original_binary()
                .ok_or(SaveRefusal::BinarySourceRequired)?;
            if !changes.rewritten.is_empty()
                || changes.content_types.is_some()
                || !changes.removed.is_empty()
                || !changes.added.is_empty()
            {
                return Err(SaveRefusal::BinaryChangesNotModelled { owner: "dc-snz.14" });
            }
            original
        }
    };
    let archive = package.archive();
    let mut new_content: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
    for (name, content) in changes.rewritten {
        let part = package
            .part(&name)
            .expect("a rewritten part is a part of the package");
        new_content.insert(part.entry(), content);
    }
    if let Some(content) = changes.content_types {
        new_content.insert(package.content_types_entry(), content);
    }
    let removed: BTreeSet<usize> = changes
        .removed
        .iter()
        .filter_map(|name| package.part(name).map(|part| part.entry()))
        .collect();
    let too_large = |bytes: u64, entries: usize| SaveRefusal::PackageTooLarge {
        bytes,
        entries,
        limit: policy.max_package_bytes.min(u64::from(u32::MAX)),
    };
    let mut entries = Vec::with_capacity(archive.entries.len() + changes.added.len());
    for (index, entry) in archive.entries.iter().enumerate() {
        if removed.contains(&index) {
            continue;
        }
        entries.push(match new_content.remove(&index) {
            Some(content) => Planned::Write {
                source: Some(index),
                written: Written::replacing(entry, &content)
                    .ok_or_else(|| too_large(content.len() as u64, archive.entries.len()))?,
            },
            None => Planned::Copy(index),
        });
    }
    for (name, content) in &changes.added {
        entries.push(Planned::Write {
            source: None,
            written: Written::added(name, content)
                .ok_or_else(|| too_large(content.len() as u64, archive.entries.len()))?,
        });
    }
    // Entries copied or rewritten keep the order their source's bytes were stored in; added ones
    // follow.
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by_key(|&index| match &entries[index] {
        Planned::Copy(entry)
        | Planned::Write {
            source: Some(entry),
            ..
        } => (0, u64::from(archive.entries[*entry].local_offset)),
        Planned::Write { source: None, .. } => (1, index as u64),
    });
    let mut offset: u64 = 0;
    let mut placements = Vec::with_capacity(entries.len());
    for index in order {
        placements.push((index, offset));
        offset += entries[index].stored_len(&archive.entries);
    }
    let directory: u64 = entries
        .iter()
        .map(|entry| entry.central_len(&archive.entries))
        .sum();
    let total = offset + directory + 22 + archive.comment.len() as u64;
    let limit = policy.max_package_bytes.min(u64::from(u32::MAX));
    if total > limit || entries.len() >= usize::from(u16::MAX) {
        return Err(too_large(total, entries.len()));
    }
    let stored = placements
        .into_iter()
        .map(|(index, offset)| (index, offset as u32))
        .collect();
    Ok(SavePlan {
        package: Arc::clone(package),
        entries,
        stored,
        revision: snapshot.edit.as_ref().map(|edit| edit.model.revision()),
        caches_dropped: changes.caches_dropped,
        sheets_rewritten_for_caches: changes.sheets_rewritten_for_caches,
        values_written: changes.values_written,
        values_left_to_excel: changes.values_left_to_excel,
    })
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

/// Reads the stored bytes of every entry the plan copies, in stored order, and checks that each
/// still has the CRC-32 it had when the package was opened.
fn check_copied(plan: &SavePlan) -> Result<(), WriteError> {
    let source = plan.package.source();
    let archive = plan.package.archive();
    for &(index, _) in &plan.stored {
        let Planned::Copy(entry) = &plan.entries[index] else {
            continue;
        };
        let entry = &archive.entries[*entry];
        if zip::span_crc(source, entry)? != entry.span_crc {
            return Err(WriteError::SourceChanged {
                entry: entry.name.clone(),
            });
        }
    }
    Ok(())
}

/// Writes the plan's package to `out`, front to back: each entry's bytes in stored order, copied
/// from the source in chunks or written from the plan, then the central directory and the end
/// record. Before the first byte, every entry the plan copies is read and checked against the
/// CRC-32 its stored bytes had when the package was opened; each is checked again as it is
/// copied.
pub fn write(plan: SavePlan, out: &mut dyn PackageWrite) -> Result<SaveReceipt, WriteError> {
    check_copied(&plan)?;
    let package = &plan.package;
    let source = package.source();
    let archive = package.archive();
    let mut sink = Counted { out, written: 0 };
    let mut offsets = vec![0u32; plan.entries.len()];
    for &(index, offset) in &plan.stored {
        offsets[index] = offset;
        match &plan.entries[index] {
            Planned::Copy(entry) => {
                let entry = &archive.entries[*entry];
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
            Planned::Write { written, .. } => {
                sink.put(&written.local_header())?;
                sink.put(&written.payload)?;
            }
        }
    }
    let directory_offset = sink.written;
    for (planned, offset) in plan.entries.iter().zip(&offsets) {
        match planned {
            Planned::Copy(entry) => {
                let mut record = archive.entries[*entry].central_record.clone();
                record[CENTRAL_LOCAL_OFFSET_AT..CENTRAL_LOCAL_OFFSET_AT + 4]
                    .copy_from_slice(&offset.to_le_bytes());
                sink.put(&record)?;
            }
            Planned::Write { written, .. } => sink.put(&written.central_record(*offset))?,
        }
    }
    let directory_size = sink.written - directory_offset;
    // prepare_save refused a package past 4 GiB - 1 byte, so both fit the 32-bit fields.
    sink.put(&end_record(
        plan.entries.len() as u16,
        directory_size as u32,
        directory_offset as u32,
        &archive.comment,
    ))?;
    let entries_copied = plan.copied_entries();
    Ok(SaveReceipt {
        bytes_written: sink.written,
        entries_copied,
        parts_written: plan.entries.len() - entries_copied,
        revision: plan.revision,
        caches_dropped: plan.caches_dropped,
        sheets_rewritten_for_caches: plan.sheets_rewritten_for_caches,
        values_written: plan.values_written,
        values_left_to_excel: plan.values_left_to_excel,
        gap_bytes_dropped: archive.unreferenced_bytes,
    })
}
