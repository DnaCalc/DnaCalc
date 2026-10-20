//! The package for CalcEngine: the one dependency graph and the derived calculation state.
//!
//! Dependency rule: it may depend on calc-doc-model and calc-fml, never on the calc-doc codec, the applications or the skins.
//!
//! - State: an [`EngineState`] is owned by its caller and holds one graph for the whole workbook.
//!   Each formula participant is a [`FormulaNode`] with a [`calc_value::NodeId`] the state issues
//!   and interns by the node's anchor cell, whose key carries its sheet, so two sheets' cells at
//!   equal rows and columns are two nodes of the same graph. [`EngineState::new`] adopts the
//!   formula cells of a [`calc_doc_model::ModelRoot`]. The state holds no authored input, source
//!   bytes or formatting: literal inputs are read through the snapshot's `Arc`-shared pages,
//!   borrowed, never copied.
//! - Edges: a node's [`Prerequisite`]s say what it reads: a cell, a rectangle, whole rows or
//!   columns ([`AxisInterval`]), or a [`SheetSpan`] across sheets. Each is one edge, filed in
//!   reverse indexes that find the nodes reading a changed cell; a span's sheets are read from
//!   the snapshot's current sheet order. [`begin_recalc`] states them from each formula's placed
//!   references: cells, rectangles and whole rows or columns. CalcFml's binder refuses sheet
//!   spans today, so only tests state a span; its binding is cfml-m3.2's and its calculation
//!   ceng-m3.5's.
//! - Edits: [`prepare_edit`] stages an authored [`calc_doc_model::OpBatch`]'s effects against the
//!   base snapshot and reserves the IDs of the nodes it adds, changing no node or edge: formula
//!   nodes added, replaced or removed ([`NodeChange`]) and the changed inputs, each with its prior
//!   input borrowed from the snapshot's page and the nodes that read it ([`InputSeed`]).
//!   [`commit_edit`] applies a [`PreparedEngineChange`], returns no error, and makes the added and
//!   replaced nodes and the seeds' readers pending; [`EngineState::stale`] lists the dirty
//!   closure of the pending nodes.
//! - Recalculation: [`begin_recalc`] compiles each unbound formula once through CalcFml (one plan
//!   per normal form, in the state's plan cache), places it at its anchor and files its
//!   prerequisites, then orders the dirty closure of the pending nodes, or every node when the
//!   numeric mode differs from the published generation's; [`step`] evaluates the ordered nodes
//!   through CalcFml's evaluator, reading literal inputs from the snapshot's pages and formula
//!   values from the results it staged or the calculated published ones, and shows every run it
//!   reads to the caller's [`ReadObserver`], which cannot change them; [`finish`] publishes the
//!   staged results as one generation and returns the [`ValueDelta`]. Each published
//!   [`FormulaResult`] is calculated (with its [`Calculation`]: revision, tick and numeric mode),
//!   a file cache ([`EngineState::with_file_caches`]) that is never read or counted as calculated,
//!   or a typed refusal ([`NotModeled`]).

mod edit;
mod graph;
mod read;
mod recalc;
mod result;
mod state;

pub use edit::{
    CommitReceipt, InputSeed, NodeChange, PrepareError, PreparedEngineChange, commit_edit,
    prepare_edit,
};
pub use graph::{AxisInterval, Prerequisite, SheetSpan};
pub use read::{ReadObserver, Unobserved};
pub use recalc::{
    RecalcFailure, RecalcProgress, RecalcWork, StartError, WorkBudget, begin_recalc, finish, step,
};
pub use result::{
    CacheWithoutFormula, CalcTick, Calculation, FormulaResult, NotModeled, ValueChange, ValueDelta,
};
pub use state::{EngineState, FormulaNode, PrerequisiteError};
