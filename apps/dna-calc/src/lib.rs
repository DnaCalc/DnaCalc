//! The package for the DNA Calc application core: the Excel, notebook, tree and canvas skins over one CalcHost workbook.
//!
//! DNA Calc opens a workbook through calc-host and shows it in a skin: the Excel grid ([`grid`]),
//! the notebook ([`notebook`]), the atlas ([`atlas`]), the tree ([`tree`]), the canvas
//! ([`canvas`]) and the ledger ([`ledger`]), in one framework ([`skin`]). Every skin reads the
//! workbook through calc-host's projections and subscriptions and changes it through calc-host's
//! intents, by way of the one [`controller::Controller`] over the open [`workbook::Workbook`]; no
//! skin reaches the engine or the document another way (dependency rule: dna-calc never depends on
//! calc-engine directly).
//!
//! - [`workbook`]: the open workbook (calc-host's session, its name, the deltas waiting).
//! - [`controller`]: the workbook behind reactive signals, the active sheet and cell, the edit.
//! - [`cells`]: a projected cell as a skin shows it, with its provenance.
//! - [`skin`]: the skin contract, the registry and switching.
//! - [`app`]: the shell (file commands, formula bar, skin switcher, sheet tabs, status line).
//! - [`web`]: the browser entry of the bundle (`cargo xtask app bundle calc`).

pub mod app;
pub mod atlas;
pub mod canvas;
pub mod cells;
pub mod controller;
pub mod grid;
pub mod ledger;
pub mod notebook;
pub mod skin;
pub mod tree;
pub mod web;
pub mod workbook;
