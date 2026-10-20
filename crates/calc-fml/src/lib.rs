//! The package for CalcFml: the Excel formula language, reference binding, shared formula plans and number formats.
//!
//! Dependency rule: it never depends on calc-engine, calc-host or the calc-doc codec.
//!
//! [`syntax`] is the lossless Excel lexer and parser: [`parse_formula`] turns formula text into a
//! [`syntax::SyntaxTree`] that reprints the text exactly, with UTF-16 spans for editors.
//!
//! [`reference`] binds the grid references of a parsed formula: [`bind_references`] resolves
//! their sheets through a [`ReferenceCatalog`] and gives typed operands relative to the formula's
//! anchor, which place at any anchor as grid areas whose cells a [`ReferenceReader`] supplies
//! through [`read_cells`].

pub mod reference;
pub mod syntax;

pub use reference::{ReferenceCatalog, ReferenceReader, bind_references, read_cells};
pub use syntax::parse_formula;
