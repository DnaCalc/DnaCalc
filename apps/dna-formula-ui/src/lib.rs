//! The formula UI DNA OneCalc and DNA Calc both show, implemented once.
//!
//! Dependency rule: its closure holds calc-fml, calc-func and the web crates, and never
//! calc-engine, calc-doc or calc-host; neither does DNA OneCalc's. What a label needs from a
//! workbook comes through a trait defined here ([`display::SheetNames`]), which DNA OneCalc's
//! session and DNA Calc's workbook each implement.
//!
//! - [`formula_field`]: the formula field (highlighting, bracket pairs, completion, signature help
//!   and the function help panel), with its state in [`editor`].
//! - [`display`]: values, cells, areas and calc-fml's refusals as text for people.
//! - [`result`]: a value as a cell shows it, an array as a bounded window, an error's meaning.
//! - [`theme`]: the Strand theme of the page, remembered for both apps.
//! - [`web`]: the browser start (panic hook, then mount) each app's entry runs with its own page.
//!
//! Its stylesheet is `web/css/formula-field.css`, which both bundles load (`cargo xtask app
//! bundle`).

pub mod display;
pub mod editor;
pub mod formula_field;
pub mod result;
pub mod theme;
pub mod web;
