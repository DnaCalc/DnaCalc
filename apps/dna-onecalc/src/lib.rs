//! The package for the DNA OneCalc application core: formulas without a calculation engine.
//!
//! Dependency rule: it never depends on calc-engine or calc-host, not even transitively.
//!
//! [`Session`] is the OneCalc application controller. It holds named sheets, typed inputs (a
//! number, text, logical, error or blank value per cell) and one formula with the cell it is
//! entered in. [`Session::calculate`] parses the formula with `calc_fml::parse_formula`, compiles
//! it with `calc_fml::compile_with_functions` into a plan the session's own `PlanCache` shares, places it with
//! `calc_fml::instantiate` and runs it with `calc_fml::evaluate`, whose operators and functions
//! are calc-func's. Caller-owned user functions use calc-func's engine-neutral contract;
//! the optional loaded VBA module is adapted by calc-vba. The session's sheets are the compile's `ReferenceCatalog`, and its inputs are
//! the evaluation's `ReferenceReader`: a cell the session holds no input for is reported missing,
//! never given a value. The [`Calculation`] it returns holds the result, the typed
//! [`Diagnostic`]s that stopped one, and the [`Read`]s the evaluation made, in order, with what
//! each returned.
//!
//! [`display`] renders those for people and [`surface`] is the browser surface over a session: a
//! formula box, a table of typed inputs, the result, the diagnostics and the reads, recalculated on
//! every edit. The formula field, the value and result display, the theme switch and the browser
//! start are dna-formula-ui's, which DNA Calc shares; this package keeps only what a session needs
//! (its sheets and inputs as calc-fml's reference reader, and the labels of its reads, diagnostics
//! and refused inputs). It has no dependency graph and no recalculation order: a session holds one
//! formula and calculates it on each edit.

pub mod display;
mod fixture;
pub mod grid;
pub mod port;
pub mod result;
mod session;
pub mod surface;
pub mod web;

pub use fixture::{Read, ReadRun, RunCells};
pub use session::{Calculation, Diagnostic, InputError, InputValue, Session, SheetError};
