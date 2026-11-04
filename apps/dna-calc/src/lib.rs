//! DNA Calc's page, shell and skins over the neutral application model.
//! Commands and projections cross `dna_calc_model::WorkbookTransport`. Workbook composition
//! and the existing SVG projection adapters live in the separate dna-calc-worker binary.
//! This package's normal dependencies contain no CalcHost, CalcEngine or document codec.
//! The headless tests explicitly inject the worker's transport; the page uses a browser worker.

pub mod app;
pub mod atlas;
pub mod canvas;
pub mod cells;
pub mod clipboard;
mod commands;
pub mod controller;
mod format;
mod names;
pub mod grid;
pub mod ledger;
mod model_adapter;
pub mod notebook;
pub mod port;
mod presentation;
mod tables;
mod review;
pub mod sheets;
pub mod skin;
pub mod style;
pub mod tree;
pub mod web;
pub mod wire;

mod files;
