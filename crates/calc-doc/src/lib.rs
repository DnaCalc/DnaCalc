//! The package for the CalcDoc codec: reading, writing and preserving .xlsx/.xlsm packages (ZIP/XML).
//!
//! [`model`] re-exports calc-doc-model, the engine-neutral authored model and operation codec this
//! codec reads into and writes from. The package reader and writer (`open`, `prepare_edit`,
//! `prepare_save` and `write`) are not here yet; the cdoc-m1 beads add them.

pub use calc_doc_model as model;
