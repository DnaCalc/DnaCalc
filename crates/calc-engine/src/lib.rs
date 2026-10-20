//! The package for CalcEngine: the one dependency graph and the derived calculation state.
//!
//! Dependency rule: it may depend on calc-doc-model, never on the calc-doc codec, the applications or the skins.
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
//!   the snapshot's current sheet order. Nothing outside the tests states prerequisites yet. Their
//!   product callers are CalcFml's plan bindings, one per kind: cell and rectangle prerequisites
//!   come with ceng-m1.2 (direct references and ranges), whole rows or columns with ceng-m3.1
//!   (whole-axis ranges), and sheet spans with cfml-m3.2 (3D span binding) and ceng-m3.5 (span
//!   calculation).
//! - Edits: [`prepare_edit`] stages an authored [`calc_doc_model::OpBatch`]'s effects against the
//!   base snapshot and reserves the IDs of the nodes it adds, changing no node or edge: formula
//!   nodes added, replaced or removed ([`NodeChange`]) and the changed inputs, each with its prior
//!   input borrowed from the snapshot's page and the nodes that read it ([`InputSeed`]).
//!   [`commit_edit`] applies a [`PreparedEngineChange`] and returns no error.
//!
//! Recalculation (`begin_recalc`, `step`, `finish`) and the binding of direct references and
//! ranges through CalcFml come with ceng-m1.2.

mod edit;
mod graph;
mod state;

pub use edit::{
    CommitReceipt, InputSeed, NodeChange, PrepareError, PreparedEngineChange, commit_edit,
    prepare_edit,
};
pub use graph::{AxisInterval, Prerequisite, SheetSpan};
pub use state::{EngineState, FormulaNode, PrerequisiteError};
