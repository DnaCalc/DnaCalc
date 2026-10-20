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
//!
//! [`eval`] compiles and evaluates formulas: [`compile`] turns a parsed formula into a shared,
//! immutable [`CompiledPlan`] held by the caller's [`PlanCache`], [`instantiate`] places a plan at
//! an anchor, and [`evaluate`] runs the instance in a per-call [`EvalFrame`] over the caller's
//! [`EvalScratch`], with every operator and function run by `calc_func::invoke` and every cell read
//! through the caller's [`ReferenceReader`].

pub mod eval;
pub mod reference;
pub mod syntax;

pub use eval::{
    CompileError, CompiledPlan, EvalFault, EvalFrame, EvalScratch, EvaluationServices,
    FormulaInstance, PlanCache, compile, evaluate, instantiate,
};
pub use reference::{ReferenceCatalog, ReferenceReader, bind_references, read_cells};
pub use syntax::parse_formula;
