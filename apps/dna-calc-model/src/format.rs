//! Format Cells drafts and projections. An absent field leaves that property unchanged.
use crate::projection::{Area, Cell};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormatPatch {
    pub number_format: Option<String>,
    pub horizontal: Option<String>,
    pub vertical: Option<String>,
    pub wrap: Option<bool>,
    pub rotation: Option<i16>,
    pub indent: Option<u16>,
    pub font_name: Option<String>,
    pub font_size: Option<String>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
    /// Empty means automatic font colour or no fill.
    pub font_color: Option<String>,
    pub fill: Option<String>,
    pub borders: Option<BorderPatch>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BorderPatch {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
    pub style: String,
    pub color: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormatSelection {
    pub revision: u64,
    pub areas: Vec<Area>,
    /// A missing property is mixed across the selected cells.
    pub common: FormatPatch,
    pub sample: Cell,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FormatState {
    pub tab: u8,
    pub open: bool,
    pub selection: Option<FormatSelection>,
    pub patch: FormatPatch,
    pub preview: Option<Cell>,
    pub message: String,
    pub epoch: u64,
}
