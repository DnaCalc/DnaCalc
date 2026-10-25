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
//! immutable [`CompiledPlan`] held by the caller's [`PlanCache`] ([`compile_loaded`] for a formula's
//! text as a file stores it), [`instantiate`] places a plan at
//! an anchor, and [`evaluate`] runs the instance in a per-call [`EvalFrame`] over the caller's
//! [`EvalScratch`], with every operator and function run by `calc_func::invoke` and every cell read
//! through the caller's [`ReferenceReader`]. An [`EvaluationObserver`] the caller attaches is shown
//! each call the evaluation runs, each area INDIRECT resolved from text ([`ResolvedReference`])
//! and the outcome it returns, by the evaluator itself, the outcome in an [`EvaluationReport`]
//! that only the evaluator can make. INDIRECT's text is bound against the [`ReferenceCatalog`] the
//! caller gives the services ([`EvaluationServices::with_catalog`]).
//!
//! [`assist`] is editor assistance over formula text: token roles for highlighting, completion of
//! calc-func's function names, and the call the caret is in for signature help.
//!
//! Parsing, binding and compiling keep stacks of their own instead of recursing on the native
//! stack, so a formula nested as deep as Excel's 8,192 characters allow is handled on a 1 MB
//! stack. Two recursions remain. Evaluation recurses into the arguments of nested lazy calls, at
//! most 64 deep: the parser gives a diagnostic to a call nested deeper, Excel's limit of nested
//! levels of functions. The derived `Debug` of a syntax tree recurses once per level of the tree,
//! and nothing on the route from formula text to value calls it.

pub mod assist;
pub mod eval;
pub mod reference;
pub mod syntax;

pub use eval::{
    CompileError, CompiledPlan, EvalFault, EvalFrame, EvalScratch, EvaluationObserver,
    EvaluationReport, EvaluationServices, ExecutedCall, FormulaForm, FormulaInstance,
    MAX_CALL_DEPTH, PlanCache, ResolvedReference, compile, compile_loaded, compile_loaded_with,
    compile_with, evaluate, instantiate,
};
pub use reference::{ReferenceCatalog, ReferenceReader, bind_references, read_cells};
pub use syntax::parse_formula;

/// calc-func's random source and SUBTOTAL exclusions, which a caller of [`evaluate`] supplies
/// through [`EvaluationServices::with_random`] and [`ReferenceReader::subtotal_exclusions`].
pub use calc_func::locale::{CalcEnvironment, DateSystem, FormatProfile};
pub use calc_func::read_number_text;
pub use calc_func::{HostFacts, RandomSource, SubtotalExclusions};
