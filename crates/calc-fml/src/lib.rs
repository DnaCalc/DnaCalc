//! The package for CalcFml: the Excel formula language, reference binding, shared formula plans and number formats.
//!
//! Dependency rule: it never depends on calc-engine, calc-host or the calc-doc codec.
//!
//! [`syntax`] is the lossless Excel lexer and parser: [`parse_formula`] turns formula text into a
//! [`syntax::SyntaxTree`] that reprints the text exactly, with UTF-16 spans for editors.

pub mod syntax;

pub use syntax::parse_formula;
