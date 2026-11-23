//! Office.js face 1, host side (dc-17spk.5; docs/architecture/API_FACES.md 5.1-5.4).
//!
//! An Office Add-in page runs in its own webview and uses the clean-room client library
//! (`apps/dna-calc/web/office-js/office.js`). That library queues operations on proxy objects and
//! sends them as one [`BatchRequest`] per `context.sync()`. [`Executor`] runs a batch against any
//! [`calc_object_model::ObjectModel`] and answers a [`BatchReply`]. [`manifest`] loads the
//! add-in's XML or unified JSON manifest.

pub mod exec;
pub mod manifest;
pub mod map;
pub mod wire;

pub use exec::Executor;
pub use wire::{BatchReply, BatchRequest, Op, WireValue};
