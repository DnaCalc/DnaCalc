//! The package for the CalcDoc codec: reading, writing and preserving .xlsx/.xlsm packages (ZIP/XML),
//! and reading .xlsb packages through SpreadsheetML with an explicit binary preservation save.
//!
//! [`model`] re-exports calc-doc-model, the engine-neutral authored model and operation codec this
//! codec reads into and writes from.
//!
//! The route is [`open`] and [`load`], then [`prepare_edit`], then [`prepare_save`] and
//! [`write()`]:
//!
//! - [`open`] reads an OPC package from a seekable [`PackageRead`] source under explicit
//!   [`OpenPolicy`] limits into a [`Package`]: its part directory, content types and
//!   relationships. Part payloads are decompressed only when read ([`Package::read_part`]) and
//!   XML parts are tokenized only as far as their [`XmlReader`] is pulled. Unsafe entry names,
//!   document type declarations (and so every declared entity), declared or actual sizes past
//!   the limits, a compressed payload larger than its declared size allows (refused before the
//!   part read reads any of it; open still streams it for its CRC) and bytes after a payload's
//!   final DEFLATE block are typed refusals. An Excel binary workbook (.xlsb) is converted, as
//!   it is opened, into the SpreadsheetML package it stands for ([`convert_binary_workbook`]),
//!   which everything after reads and a save writes by default. The validated original package
//!   is also retained: [`SaveFormat::Xlsb`] copies it when the snapshot needs no part rewrites;
//!   authored binary output is refused under dc-snz.14.
//! - [`load`] reads an opened package's workbook: its sheets, from the workbook part and its
//!   relationships, and each worksheet's cells (numbers, shared and inline strings, logicals,
//!   errors and formulas, a shared formula's members each with its own text) into a
//!   [`model::ModelRoot`] at the document's first revision ([`LoadedWorkbook`]). Only occupied
//!   cells are materialized. Each cell's style index and each formula's shared formula group and
//!   cached result are kept beside the model; a cached result is [`model::FileCached`] with the
//!   identity of the part it was read from. The loaded workbook also carries its
//!   [`DateSystem`], its [`CalcMode`] (`calcPr`'s `calcMode`; its `iterate`, `iterateCount` and
//!   `iterateDelta` are read into iteration settings), its defined names in the model's definitions,
//!   and the count of defined names not loaded into the model (such as resolved locators). A cell
//!   or formula it does not read is a typed refusal ([`LoadError`]). The package's
//!   DNA view manifest, one custom XML part, is read into the model's skin layout
//!   ([`model::SkinLayout`]): each block with its opaque ID, its kind and its locator, a hidden,
//!   workbook-scoped defined name named `_dna.block.` and 32 hex digits whose reference becomes
//!   the block's anchor, and each view's placements. A manifest of a version it does not read, or
//!   one it cannot read (the XML reader's refusal of an item that names the manifest's namespace
//!   included), and a locator that does not resolve are [`LayoutWarning`]s, never refusals: no
//!   cell is lost and no layout is invented.
//! - [`prepare_edit`] applies a batch of cell operations (set a cell's value or formula, clear an
//!   area) and skin layout edits (create or remove a block over a sheet's cells, add, move or
//!   remove its placements in the notebook, tree, canvas and grid views) to a revision of a loaded
//!   workbook's model, making the next revision through [`model::ModelRoot::apply`]. It refuses
//!   a batch the model refuses and a formula a save could not write: one longer than
//!   [`FORMULA_MAX_UTF16_CODE_UNITS`] or one CalcFml's parser reads with a diagnostic
//!   ([`EditError`]). The caller's revision is never changed; the caller adopts the new one.
//!   [`LoadedWorkbook::check_layout_save`] tells the caller, before [`prepare_edit`], whether a
//!   save can write a batch's layout edits, so a layout edit a save would refuse can be dropped
//!   before it shares a revision with cell edits.
//! - [`prepare_save`] plans a save of a [`DocumentSnapshot`]: of an opened document as it was
//!   opened ([`OpenedDocument::snapshot`]), or of a revision of a loaded workbook's model
//!   ([`LoadedWorkbook::snapshot`]). Every part the save does not rewrite is copied as its
//!   stored bytes. A revision whose cells differ from the loaded ones rewrites each worksheet part
//!   that holds a changed cell or a formula's cached result, appends new text to the shared
//!   strings, sets `fullCalcOnLoad` on the workbook's `calcPr` when it writes an array formula
//!   without a value or the workbook holds a data table and, when the set of formula cells
//!   changed, removes the calculation chain. A revision whose skin layout differs from the loaded
//!   one rewrites the view manifest from its own bytes, keeping every field version 1 does not
//!   name, or adds one (its item, item properties, relationships and content types) to a package
//!   without one, and writes each created block's locator as a hidden defined name of the
//!   workbook part and removes each removed block's hidden locator; every other defined name stays
//!   as the source wrote it. A manifest it did not read is never overwritten
//!   ([`SaveRefusal::LayoutNotWritable`]). A formula the revision sets is checked as
//!   [`prepare_edit`] checks it, so a revision made another way (by [`model::ModelRoot::apply`])
//!   is refused when it holds one a save could not write. Everything a save rewrites is serialized
//!   while it is prepared, so a refusal ([`SaveRefusal`]) comes before any byte is written.
//!   [`write()`] then streams the package to a [`PackageWrite`] sink entry by entry. Before its
//!   first byte it reads every entry it copies and refuses one whose stored bytes no longer have
//!   the CRC-32 they had when the package was opened ([`WriteError`]), leaving the sink empty.
//!
//! [`blank_package`] makes the package of a new workbook, one empty worksheet, to open as any other.
//!
//! The codec opens no file: the source and the sink are supplied by the caller.
//!
//! ## Read a workbook from memory
//!
//! Open workbook bytes under an explicit policy, then load the authored worksheet model.
//!
//! ```
//! use std::sync::Arc;
//! use calc_doc::{MemorySource, OpenPolicy, blank_package_named, load, open};
//! use calc_doc::model::value::{IdAllocator, SheetId};
//!
//! let bytes = blank_package_named("Inputs");
//! let document = open(Arc::new(MemorySource::new(bytes)), &OpenPolicy::default()).unwrap();
//! let mut sheet_ids = IdAllocator::<SheetId>::new();
//! let workbook = load(&document, &mut sheet_ids).unwrap();
//! let sheets = workbook.model().sheets();
//! assert_eq!(sheets.len(), 1);
//! assert_eq!(sheets[0].name().text().to_string_lossy(), "Inputs");
//! ```
//!
//! See the [crate contract](https://github.com/DnaCalc/DnaCalc/blob/master/crates/calc-doc/CONTRACT.md).

#![warn(missing_docs)]

mod blank;
mod custom_xml;
pub use custom_xml::CustomXmlItem;
mod edit;
mod formula_sources;
pub use formula_sources::{FormulaSourceBudget, RegroupedFormulaSources, regroup_formula_sources};
mod error;
mod load;
pub mod look;
pub use load::pivot;
mod opc;
mod policy;
mod save;
mod session;
mod session_look;
mod source;
mod vba;
mod vba_write;
mod xls;
mod xlsb;
mod xml;
mod zip;

pub use blank::{blank_package, blank_package_named};
pub use calc_doc_model as model;
pub use edit::{
    EditError, PreparedEdit, prepare_edit, prepare_sheet_edit, prepare_structural_edit,
};
pub use error::{OpenError, PartReadError, UnsafeName};
pub use load::{
    CalcMode, CellError, Chart, ChartAnchor, ChartData, ChartPlot, ChartSeries, ColorScale,
    ColumnSpan, ConditionalFormat, ConditionalRule, DataBar, DateSystem, DrawingPoint,
    FORMULA_MAX_UTF16_CODE_UNITS, FileProtection, FormulaRecord, IconSet, LayoutWarning, LoadError, LoadRepair,
    LoadedSheet, LoadedWorkbook, ReviewLink, ReviewNote, RowFormat, SharedFormula, SheetCharts, SheetGeometry,
    SheetReview, SheetView, SheetViewMode, Threshold, Validation, load, load_preserving_sheet_ids,
};
pub use opc::{
    CONTENT_TYPES_ENTRY, CONTENT_TYPES_NAMESPACE, ContentTypes, Package, Part,
    RELATIONSHIPS_NAMESPACE, Relationship, Relationships, TargetMode,
};
pub use policy::{OpenPolicy, PackageLimits};
pub use save::file_form::{
    FileForm, FileFormError, file_form, file_form_with_links, file_form_with_lookup,
};
pub use save::{
    DocumentSnapshot, SaveFormat, SavePlan, SavePolicy, SaveReceipt, SaveRefusal, SaveViews, SheetProtectionSave,
    SpreadsheetFormat, StructuralEdits, WriteError, conditional_rule, merge_template_sheets,
    new_condition_rule, new_conditional_rule, prepare_save, rename_template_sheet, write,
};
pub use session::{
    SessionArray, SessionConstant, SessionDxf, SessionFile, SessionFormat, SessionFormula,
    SessionLook, SessionRefusal, SessionRule, SessionValue, SessionVba, plain_formula_needs_legacy,
    read_session, read_vba_file, write_session,
};
pub use source::{MemorySource, PackageRead, PackageWrite, SinkError, SourceError};
pub use vba::{VBA_PROJECT_CONTENT_TYPE, VbaProjectPart, VbaReadError, VbaReadPolicy};
pub use vba_write::{
    SourceModule, SourceModuleKind, VbaWriteError, compress_vba_stream, source_vba_package,
    source_vba_package_for_sheets, write_vba_project,
};
pub use xls::{LegacyFormat, LegacyReport};
pub use xml::{
    XmlAttribute, XmlElement, XmlError, XmlEvent, XmlLimits, XmlName, XmlPart, XmlReader, XmlStr,
};

use std::sync::Arc;

/// A document opened from a package.
pub struct OpenedDocument {
    package: Arc<opc::Package>,
    legacy: Option<Arc<[u8]>>,
}

impl OpenedDocument {
    /// Borrow the opened workbook's underlying OPC package.
    pub fn package(&self) -> &Package {
        &self.package
    }

    /// The bytes of the legacy Excel workbook (.xls, .xla, .xlt) this document was converted from,
    /// `None` for any other document.
    pub fn legacy_source(&self) -> Option<&[u8]> {
        self.legacy.as_deref()
    }

    pub(crate) fn package_arc(&self) -> &Arc<Package> {
        &self.package
    }

    /// An immutable view of the document as opened, which a save is prepared from.
    pub fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot::new(Arc::clone(&self.package))
    }
}

/// Opens the package in `source` under `policy`. The document keeps `source` and reads part
/// payloads from it lazily and when it is saved.
pub fn open(
    source: Arc<dyn PackageRead>,
    policy: &OpenPolicy,
) -> Result<OpenedDocument, OpenError> {
    open_with_password(source, policy, None)
}

/// [`open`] with the password of an encrypted legacy Excel workbook (.xls, .xla, .xlt: XOR
/// obfuscation, RC4 or RC4 CryptoAPI). `None` tries only the password Excel uses for a workbook
/// it saves read-only recommended (`VelvetSweatshop`); a workbook that needs another is
/// [`OpenError::PasswordRequired`], and a wrong one [`OpenError::WrongPassword`]. A package that
/// is not a legacy workbook ignores the password.
pub fn open_with_password(
    source: Arc<dyn PackageRead>,
    policy: &OpenPolicy,
    password: Option<&str>,
) -> Result<OpenedDocument, OpenError> {
    let package = match opc::Package::open(source.clone(), policy) {
        Ok(package) => package,
        // Not a ZIP archive: a compound file or raw BIFF stream is a legacy workbook. (The ZIP route is
        // tried first so a package's own bytes are not read at offset 0 before it is opened.)
        Err(OpenError::NotAZipArchive) => {
            let Some(bytes) = legacy_bytes(source.as_ref(), policy)? else {
                return Err(OpenError::NotAZipArchive);
            };
            let (converted, _) = xls::convert(&bytes, password)?;
            let package = opc::Package::open(Arc::new(MemorySource::new(converted)), policy)?;
            return Ok(OpenedDocument {
                package: Arc::new(package),
                legacy: Some(bytes.into()),
            });
        }
        Err(other) => return Err(other),
    };
    // An Excel binary workbook is read as the SpreadsheetML package it stands for.
    if let Some(workbook) = xlsb::binary_workbook(&package) {
        let converted = xlsb::convert(&package, &workbook)?;
        let original = Arc::new(package);
        let package = opc::Package::open(Arc::new(MemorySource::new(converted)), policy)?
            .with_original_binary(original);
        return Ok(OpenedDocument {
            package: Arc::new(package),
            legacy: None,
        });
    }
    Ok(OpenedDocument {
        package: Arc::new(package),
        legacy: None,
    })
}

/// The bytes of `source` when it is a legacy workbook container, within the
/// policy's package size limit.
fn legacy_bytes(source: &dyn PackageRead, policy: &OpenPolicy) -> Result<Option<Vec<u8>>, OpenError> {
    let length = source.size();
    if length < 8 {
        return Ok(None);
    }
    let mut signature = [0u8; 8];
    source.read_at(0, &mut signature).map_err(OpenError::Source)?;
    if !xls::is_legacy(&signature) {
        return Ok(None);
    }
    if length > policy.package.max_package_bytes {
        return Err(OpenError::PackageTooLarge {
            length,
            limit: policy.package.max_package_bytes,
        });
    }
    let mut bytes = vec![0u8; length as usize];
    source.read_at(0, &mut bytes).map_err(OpenError::Source)?;
    Ok(Some(bytes))
}

/// What reading the legacy Excel workbook in `source` found (its BIFF version, its sheets and the
/// record types not read, counted); `None` when `source` is not a legacy workbook.
pub fn legacy_workbook_report(
    source: Arc<dyn PackageRead>,
    policy: &OpenPolicy,
) -> Result<Option<LegacyReport>, OpenError> {
    match legacy_bytes(source.as_ref(), policy)? {
        Some(bytes) => xls::convert(&bytes, None).map(|(_, report)| Some(report)),
        None => Ok(None),
    }
}

/// The SpreadsheetML package (the bytes of an .xlsx or .xlsm archive) an Excel binary workbook
/// stands for, as [`open`] reads it; `None` when `source` is not an Excel binary workbook.
pub fn convert_binary_workbook(
    source: Arc<dyn PackageRead>,
    policy: &OpenPolicy,
) -> Result<Option<Vec<u8>>, OpenError> {
    let package = opc::Package::open(source, policy)?;
    match xlsb::binary_workbook(&package) {
        Some(workbook) => xlsb::convert(&package, &workbook).map(Some),
        None => Ok(None),
    }
}

pub use save::{conditional_with_dxf, copied_conditional_source, copied_validation_source};

pub use load::validation::movement_supported as validation_movement_supported;
