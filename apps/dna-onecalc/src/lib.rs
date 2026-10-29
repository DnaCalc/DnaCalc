//! The package for the DNA OneCalc application core: formulas without a calculation engine.
//!
//! Dependency rule: it never depends on calc-engine or calc-host, not even transitively.
//!
//! [`Session`] is the OneCalc application controller. It holds named sheets, typed inputs (a
//! number, text, logical, error or blank value per cell) and one formula with the cell it is
//! entered in. [`Session::calculate`] parses the formula with `calc_fml::parse_formula`, compiles
//! it with `calc_fml::compile` into a plan the session's own `PlanCache` shares, places it with
//! `calc_fml::instantiate` and runs it with `calc_fml::evaluate`, whose operators and functions
//! are calc-func's. The session's sheets are the compile's `ReferenceCatalog`, and its inputs are
//! the evaluation's `ReferenceReader`: a cell the session holds no input for is reported missing,
//! never given a value. The [`Calculation`] it returns holds the result, the typed
//! [`Diagnostic`]s that stopped one, and the [`Read`]s the evaluation made, in order, with what
//! each returned.
//!
//! [`display`] renders those for people, [`formula_field`] is the formula field (highlighting,
//! completion, signature and function help) DNA OneCalc and DNA Calc share, and [`surface`] is the
//! browser surface over a session:
//! a formula box, a table of typed inputs, the result, the diagnostics and the reads, recalculated
//! on every edit.

pub mod display;
pub mod editor;
mod fixture;
pub mod formula_field;
pub mod grid;
pub mod result;
mod session;
pub mod surface;
pub mod theme;
pub mod web;

pub use fixture::{Read, ReadRun, RunCells};
pub use session::{Calculation, Diagnostic, InputError, InputValue, Session, SheetError};
