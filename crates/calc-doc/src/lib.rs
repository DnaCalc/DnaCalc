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
//!   [`DateSystem`], its [`CalcMode`] (`calcPr`'s `calcMode`; no other `calcPr` attribute is read)
//!   and the number of defined names it holds and does not load. A cell or formula it does not read is a typed refusal ([`LoadError`]). The package's
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

mod blank;
mod edit;
mod formula_sources;
pub use formula_sources::{FormulaSourceBudget, RegroupedFormulaSources, regroup_formula_sources};
mod error;
mod load;
mod opc;
mod policy;
mod save;
mod source;
mod vba;
mod xlsb;
mod xml;
mod zip;

pub use blank::blank_package;
pub use calc_doc_model as model;
pub use edit::{
    EditError, PreparedEdit, prepare_edit, prepare_sheet_edit, prepare_structural_edit,
};
pub use error::{OpenError, PartReadError, UnsafeName};
pub use load::{
    CalcMode, CellError, Chart, ChartAnchor, ChartData, ChartPlot, ChartSeries, ColorScale,
    ColumnSpan, ConditionalFormat, ConditionalRule, DataBar, DateSystem, DrawingPoint,
    FORMULA_MAX_UTF16_CODE_UNITS, FormulaRecord, IconSet, LayoutWarning, LoadError, LoadedSheet,
    LoadedWorkbook, ReviewLink, ReviewNote, RowFormat, SharedFormula, SheetCharts, SheetGeometry,
    SheetReview, SheetView, Threshold, Validation, load,
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
    DocumentSnapshot, SaveFormat, SavePlan, SavePolicy, SaveViews, SaveReceipt, SaveRefusal, StructuralEdits,
    WriteError, conditional_rule, new_conditional_rule, prepare_save, write,
};
pub use source::{MemorySource, PackageRead, PackageWrite, SinkError, SourceError};
pub use vba::{VbaModuleKind, VbaModuleSource, VbaProjectSource, VbaReadError, VbaReadPolicy};
pub use xml::{
    XmlAttribute, XmlElement, XmlError, XmlEvent, XmlLimits, XmlName, XmlPart, XmlReader, XmlStr,
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
    // An Excel binary workbook is read as the SpreadsheetML package it stands for.
    if let Some(workbook) = xlsb::binary_workbook(&package) {
        let converted = xlsb::convert(&package, &workbook)?;
        let original = Arc::new(package);
        let package = opc::Package::open(Arc::new(MemorySource::new(converted)), policy)?
            .with_original_binary(original);
        return Ok(OpenedDocument {
            package: Arc::new(package),
        });
    }
    Ok(OpenedDocument {
        package: Arc::new(package),
    })
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
