//! A worksheet's row and column geometry as the file states it: the default row height and column
//! width (`<sheetFormatPr>`), the widths and hidden flags of column spans (`<cols><col>`), and each
//! row's height and hidden flag (`<row ht hidden>`). A skin lays the grid out with it (dc-09v);
//! calculation never reads it. Heights are in points and widths in characters of the default
//! font's maximum digit width, as the file writes them.

use super::sheet::collapsed;
use crate::xml::XmlElement;
use std::collections::BTreeMap;

/// One `<col>` element: the columns `first` to `last` (1-based, inclusive).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColumnSpan {
    pub first: u32,
    pub last: u32,
    /// The width, in characters; `None` when the element states none.
    pub width: Option<f64>,
    pub hidden: bool,
}

/// One row's `ht` and `hidden`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RowFormat {
    /// The height in points; `None` when the row states none.
    pub height: Option<f64>,
    pub hidden: bool,
}

/// A worksheet's geometry as the file states it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SheetGeometry {
    /// `sheetFormatPr`'s `defaultRowHeight`, in points.
    pub default_row_height: Option<f64>,
    /// `sheetFormatPr`'s `defaultColWidth`, in characters.
    pub default_column_width: Option<f64>,
    /// `sheetFormatPr`'s `baseColWidth`, in characters (Excel's default is 8).
    pub base_column_width: Option<f64>,
    /// The `<col>` spans, in file order.
    pub columns: Vec<ColumnSpan>,
    /// The rows that state a height or are hidden.
    pub rows: BTreeMap<u32, RowFormat>,
}

impl SheetGeometry {
    /// The span that holds `column`, when one does.
    pub fn column(&self, column: u32) -> Option<&ColumnSpan> {
        self.columns
            .iter()
            .find(|span| (span.first..=span.last).contains(&column))
    }

    /// The row's format, when it states a height or is hidden.
    pub fn row(&self, row: u32) -> Option<&RowFormat> {
        self.rows.get(&row)
    }
}

fn number(element: &XmlElement, name: &str) -> Option<f64> {
    element
        .attribute(name)
        .and_then(|text| collapsed(text).parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
}

fn index(element: &XmlElement, name: &str) -> Option<u32> {
    element
        .attribute(name)
        .and_then(|text| collapsed(text).parse::<u32>().ok())
        .filter(|value| (1..=calc_doc_model::value::MAX_COLUMNS).contains(value))
}

impl SheetGeometry {
    /// Reads `<sheetFormatPr>`'s defaults.
    pub(crate) fn read_format(&mut self, element: &XmlElement) {
        self.default_row_height = number(element, "defaultRowHeight");
        self.default_column_width = number(element, "defaultColWidth");
        self.base_column_width = number(element, "baseColWidth");
    }

    /// Reads one `<col>`; an element without a valid span is passed over.
    pub(crate) fn read_column(&mut self, element: &XmlElement) {
        let (Some(first), Some(last)) = (index(element, "min"), index(element, "max")) else {
            return;
        };
        if first > last {
            return;
        }
        self.columns.push(ColumnSpan {
            first,
            last,
            width: number(element, "width"),
            hidden: super::manifest::is_true(element.attribute("hidden")),
        });
    }

    /// Reads one `<row>`'s height and hidden flag.
    pub(crate) fn read_row(&mut self, row: u32, element: &XmlElement) {
        let format = RowFormat {
            height: number(element, "ht"),
            hidden: super::manifest::is_true(element.attribute("hidden")),
        };
        if format != RowFormat::default() {
            self.rows.insert(row, format);
        }
    }
}
