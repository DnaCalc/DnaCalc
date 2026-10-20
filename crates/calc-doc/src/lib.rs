//! The package for the CalcDoc codec: reading, writing and preserving .xlsx/.xlsm packages (ZIP/XML).
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
//!   final DEFLATE block are typed refusals.
//! - [`load`] reads an opened package's workbook: its sheets, from the workbook part and its
//!   relationships, and each worksheet's cells (numbers, shared and inline strings, logicals,
//!   errors and formulas, a shared formula's members each with its own text) into a
//!   [`model::ModelRoot`] at the document's first revision ([`LoadedWorkbook`]). Only occupied
//!   cells are materialized. Each cell's style index and each formula's shared formula group and
//!   cached result are kept beside the model; a cached result is [`model::FileCached`] with the
//!   identity of the part it was read from. The loaded workbook also carries its
//!   [`DateSystem`] and the number of defined names it holds and does not load; `calcPr` is not
//!   read. A cell or formula it does not read is a typed refusal ([`LoadError`]).
//! - [`prepare_edit`] applies a batch of cell operations (set a cell's value or formula, clear an
//!   area) to a revision of a loaded workbook's model, making the next revision through
//!   [`model::ModelRoot::apply`]. It refuses a batch the model refuses and a formula a save could
//!   not write: one longer than [`FORMULA_MAX_UTF16_CODE_UNITS`] or one CalcFml's parser reads
//!   with a diagnostic ([`EditError`]). The caller's revision is never changed; the caller adopts
//!   the new one.
//! - [`prepare_save`] plans a save of a [`DocumentSnapshot`]: of an opened document as it was
//!   opened ([`OpenedDocument::snapshot`]), or of a revision of a loaded workbook's model
//!   ([`LoadedWorkbook::snapshot`]). Every part the save does not rewrite is copied as its
//!   stored bytes. A revision whose cells differ from the loaded ones rewrites each worksheet part
//!   that holds a changed cell or a formula's cached result, appends new text to the shared
//!   strings, sets `fullCalcOnLoad` on the workbook's `calcPr` and, when the set of formula cells
//!   changed, removes the calculation chain. A formula the revision sets is checked as
//!   [`prepare_edit`] checks it, so a revision made another way (by [`model::ModelRoot::apply`])
//!   is refused when it holds one a save could not write. Everything a save rewrites is serialized
//!   while it is prepared, so a refusal ([`SaveRefusal`]) comes before any byte is written.
//!   [`write()`] then streams the package to a [`PackageWrite`] sink entry by entry. Before its
//!   first byte it reads every entry it copies and refuses one whose stored bytes no longer have
//!   the CRC-32 they had when the package was opened ([`WriteError`]), leaving the sink empty.
//!
//! The codec opens no file: the source and the sink are supplied by the caller.

mod edit;
mod error;
mod load;
mod opc;
mod policy;
mod save;
mod source;
mod xml;
mod zip;

pub use calc_doc_model as model;
pub use edit::{EditError, PreparedEdit, prepare_edit};
pub use error::{OpenError, PartReadError, UnsafeName};
pub use load::{
    CellError, DateSystem, FORMULA_MAX_UTF16_CODE_UNITS, FormulaRecord, LoadError, LoadedSheet,
    LoadedWorkbook, SharedFormula, load,
};
pub use opc::{
    CONTENT_TYPES_ENTRY, CONTENT_TYPES_NAMESPACE, ContentTypes, Package, Part,
    RELATIONSHIPS_NAMESPACE, Relationship, Relationships, TargetMode,
};
pub use policy::{OpenPolicy, PackageLimits};
pub use save::{
    DocumentSnapshot, SavePlan, SavePolicy, SaveReceipt, SaveRefusal, WriteError, prepare_save,
    write,
};
pub use source::{MemorySource, PackageRead, PackageWrite, SinkError, SourceError};
pub use xml::{
    XmlAttribute, XmlElement, XmlError, XmlEvent, XmlLimits, XmlName, XmlPart, XmlReader,
};

use std::sync::Arc;

/// A document opened from a package.
pub struct OpenedDocument {
    package: Arc<opc::Package>,
}

impl OpenedDocument {
    pub fn package(&self) -> &Package {
        &self.package
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
    let package = opc::Package::open(source, policy)?;
    Ok(OpenedDocument {
        package: Arc::new(package),
    })
}
