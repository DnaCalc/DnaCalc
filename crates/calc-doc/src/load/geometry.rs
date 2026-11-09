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
    /// `customWidth`: the span's width is its own rather than the sheet's default (dc-uar:
    /// CELL("width") reports it).
    pub custom_width: bool,
    pub outline_level: u8,
    pub collapsed: bool,
}

/// What a worksheet's first window (`<sheetView>`) shows as the file states it (dc-uar): its top
/// left cell (`topLeftCell`) and its active cell (the `activeCell` of the selection in the active
/// pane), each as (row, column). INFO("origin") and CELL without a reference read them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SheetView {
    pub zoom: Option<u16>,
    pub top_left: Option<(u32, u32)>,
    pub active_cell: Option<(u32, u32)>,
    pub frozen: Option<calc_doc_model::FrozenPanes>,
    /// The bottom-right pane's first visible cell, independent of the first pane's origin.
    pub pane_top_left: Option<(u32, u32)>,
    /// Separate split positions in twips, stored as finite binary64 bits.
    pub split: Option<(u64, u64)>,
    /// Column bit 0, row bit 1.
    pub active_pane: u8,
    /// A separate split window, whose positions are stored in twips rather than band counts.
    pub split_window: bool,
}
impl SheetView {
    pub fn pane_layout(self) -> Option<calc_doc_model::PaneLayout> {
        use calc_doc_model::{PaneLayout, SplitPanes};
        if let Some(p) = self.frozen {
            return Some(PaneLayout::Frozen(p));
        }
        let (x, y) = self.split?;
        let (r, c) = self.pane_top_left.unwrap_or((1, 1));
        SplitPanes::new(f64::from_bits(x), f64::from_bits(y), r, c).map(PaneLayout::Split)
    }
    pub fn with_pane_layout(mut self, layout: Option<calc_doc_model::PaneLayout>) -> Self {
        use calc_doc_model::PaneLayout;
        self.frozen = None;
        self.split = None;
        self.split_window = false;
        match layout {
            Some(PaneLayout::Frozen(p)) => {
                self.frozen = Some(p);
                self.top_left = Some((1, 1));
                self.pane_top_left = Some((p.rows() + 1, p.columns() + 1));
                let (r, c) = self.active_cell.unwrap_or((1, 1));
                self.active_pane = u8::from(c > p.columns() && p.columns() > 0)
                    | (u8::from(r > p.rows() && p.rows() > 0) << 1);
            }
            Some(PaneLayout::Split(p)) => {
                self.split = Some(p.positions());
                self.split_window = true;
                self.pane_top_left = Some(p.origin());
                let (x, y) = p.positions();
                self.active_pane &=
                    u8::from(f64::from_bits(x) > 0.) | (u8::from(f64::from_bits(y) > 0.) << 1);
            }
            None => {
                self.top_left = Some((1, 1));
                self.pane_top_left = None;
                self.active_pane = 0;
            }
        }
        self
    }
}

/// One row's `ht` and `hidden`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RowFormat {
    /// The height in points; `None` when the row states none.
    pub height: Option<f64>,
    pub hidden: bool,
    pub outline_level: u8,
    pub collapsed: bool,
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
    pub summary_below: Option<bool>,
    pub outline_unsupported: bool,
    pub summary_right: Option<bool>,
    pub(crate) outline_state: Option<super::outline::OutlineState>,
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

fn invalid_outline(element: &XmlElement) -> bool {
    element
        .attribute("outlineLevel")
        .is_some_and(|s| s.parse::<u8>().ok().is_none_or(|n| n > 7))
        || element
            .attribute("collapsed")
            .is_some_and(|s| !matches!(s, "0" | "1" | "false" | "true"))
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
        self.outline_unsupported |= invalid_outline(element);
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
            custom_width: super::manifest::is_true(element.attribute("customWidth")),
            outline_level: element
                .attribute("outlineLevel")
                .and_then(|n| n.parse::<u8>().ok())
                .unwrap_or(0)
                .min(7),
            collapsed: super::manifest::is_true(element.attribute("collapsed")),
        });
    }

    /// Reads one `<row>`'s height and hidden flag.
    pub(crate) fn read_row(&mut self, row: u32, element: &XmlElement) {
        self.outline_unsupported |= invalid_outline(element);
        let format = RowFormat {
            height: number(element, "ht"),
            hidden: super::manifest::is_true(element.attribute("hidden")),
            outline_level: element
                .attribute("outlineLevel")
                .and_then(|n| n.parse::<u8>().ok())
                .unwrap_or(0)
                .min(7),
            collapsed: super::manifest::is_true(element.attribute("collapsed")),
        };
        if format != RowFormat::default() {
            self.rows.insert(row, format);
        }
    }
}

impl SheetGeometry {
    /// The geometry after `edit` inserts or deletes rows or columns of the sheet: rows and column
    /// spans after the edit's first position shift with it, deleted ones go, a span the edit cuts
    /// shrinks or is split around the inserted columns, and each inserted row or column takes the
    /// height or width of the one before the inserted band, as Excel's insertion does by default
    /// (measured, calc-host's tests/fixtures/d2/structural-excel.json: heights and widths, not
    /// the hidden flag). An inserted band at the start of the axis takes none.
    pub fn moved(&self, edit: calc_doc_model::AxisEdit) -> SheetGeometry {
        use calc_doc_model::{Axis, AxisChange, moved_position};
        let mut moved = self.clone();
        let (first, last, count) = (edit.first(), edit.last(), edit.count());
        match edit.axis() {
            Axis::Rows => {
                let mut rows = BTreeMap::new();
                for (&row, format) in &self.rows {
                    if let Some(to) = moved_position(edit, row) {
                        rows.insert(to, *format);
                    }
                }
                if edit.change() == AxisChange::Insert
                    && first > 1
                    && let Some(height) = self.rows.get(&(first - 1)).and_then(|row| row.height)
                {
                    for row in first..=last {
                        rows.insert(
                            row,
                            RowFormat {
                                height: Some(height),
                                hidden: false,
                                ..RowFormat::default()
                            },
                        );
                    }
                }
                moved.rows = rows;
            }
            Axis::Columns => {
                let max = calc_doc_model::value::MAX_COLUMNS;
                let mut columns = Vec::new();
                for span in &self.columns {
                    match edit.change() {
                        AxisChange::Insert => {
                            if span.last < first {
                                columns.push(*span);
                            } else if span.first >= first {
                                if span.first + count <= max {
                                    columns.push(ColumnSpan {
                                        first: span.first + count,
                                        last: (span.last + count).min(max),
                                        ..*span
                                    });
                                }
                            } else {
                                columns.push(ColumnSpan {
                                    last: first - 1,
                                    ..*span
                                });
                                if last < max {
                                    columns.push(ColumnSpan {
                                        first: last + 1,
                                        last: (span.last + count).min(max),
                                        ..*span
                                    });
                                }
                            }
                        }
                        AxisChange::Delete => {
                            // The span's columns before the deleted band, and those after it
                            // moved back by the band.
                            let before = (span.first < first)
                                .then(|| (span.first, span.last.min(first - 1)));
                            let after = (span.last > last)
                                .then(|| (span.first.max(last + 1) - count, span.last - count));
                            match (before, after) {
                                (Some((a, _)), Some((_, d))) => columns.push(ColumnSpan {
                                    first: a,
                                    last: d,
                                    ..*span
                                }),
                                (Some((a, b)), None) | (None, Some((a, b))) => {
                                    columns.push(ColumnSpan {
                                        first: a,
                                        last: b,
                                        ..*span
                                    })
                                }
                                (None, None) => {}
                            }
                        }
                    }
                }
                if edit.change() == AxisChange::Insert
                    && first > 1
                    && let Some(source) = self.column(first - 1)
                    && source.width.is_some()
                {
                    columns.push(ColumnSpan {
                        first,
                        last,
                        width: source.width,
                        hidden: false,
                        custom_width: source.custom_width,
                        outline_level: source.outline_level,
                        collapsed: false,
                    });
                }
                columns.sort_by_key(|span| span.first);
                moved.columns = columns;
            }
        }
        moved
    }
}

impl SheetGeometry {
    /// Physical dimensions in the canonical authored model; defaults stay with the worksheet.
    pub fn authored(
        &self,
        sheet: calc_doc_model::value::SheetId,
        layouts: &calc_doc_model::AxisLayouts,
    ) -> Self {
        use calc_doc_model::Axis;
        let rows = layouts
            .bands(sheet, Axis::Rows)
            .map(|(index, format)| {
                (
                    index,
                    RowFormat {
                        height: format.size(),
                        hidden: format.hidden(),
                        outline_level: format.outline_level(),
                        collapsed: format.collapsed() != 0,
                    },
                )
            })
            .collect();
        let mut columns: Vec<ColumnSpan> = Vec::new();
        for (index, format) in layouts.bands(sheet, Axis::Columns) {
            let span = ColumnSpan {
                first: index,
                last: index,
                width: format.size(),
                hidden: format.hidden(),
                custom_width: format.custom(),
                outline_level: format.outline_level(),
                collapsed: format.collapsed() != 0,
            };
            if let Some(last) = columns.last_mut()
                && last.last + 1 == index
                && last.width == span.width
                && last.hidden == span.hidden
                && last.custom_width == span.custom_width
                && last.outline_level == span.outline_level
                && last.collapsed == span.collapsed
            {
                last.last = index;
            } else {
                columns.push(span);
            }
        }
        Self {
            default_row_height: self.default_row_height,
            default_column_width: self.default_column_width,
            base_column_width: self.base_column_width,
            columns,
            rows,
            summary_below: Some(layouts.summary_after(sheet, calc_doc_model::Axis::Rows)),
            summary_right: Some(layouts.summary_after(sheet, calc_doc_model::Axis::Columns)),
            outline_state: None,
            outline_unsupported: self.outline_unsupported,
        }
    }
    pub(crate) fn add_to_model(
        &self,
        sheet: calc_doc_model::value::SheetId,
        layouts: &mut calc_doc_model::AxisLayouts,
    ) {
        use calc_doc_model::{Axis, AxisFormat, AxisFormatEdit};
        for (&index, row) in &self.rows {
            let format = AxisFormat::new(row.height, row.hidden, false)
                .expect("finite loaded row dimensions")
                .with_outline(row.outline_level, 0)
                .unwrap();
            layouts
                .set(AxisFormatEdit::new(sheet, Axis::Rows, index, format).expect("a loaded row"));
        }
        // The loader resolves overlapping spans by the first one, as `column` does.
        let mut seen = std::collections::BTreeSet::new();
        for span in &self.columns {
            for index in span.first..=span.last {
                if seen.insert(index) {
                    let format = AxisFormat::new(span.width, span.hidden, span.custom_width)
                        .expect("finite loaded column dimensions")
                        .with_outline(span.outline_level, 0)
                        .unwrap();
                    layouts.set(
                        AxisFormatEdit::new(sheet, Axis::Columns, index, format)
                            .expect("a loaded column"),
                    );
                }
            }
        }
        for (axis, after) in [
            (Axis::Rows, self.summary_below.unwrap_or(true)),
            (Axis::Columns, self.summary_right.unwrap_or(true)),
        ] {
            let f = layouts.get(sheet, axis, 1);
            layouts.set(
                AxisFormatEdit::new(sheet, axis, 1, f)
                    .unwrap()
                    .with_summary_after(after),
            );
            let groups = layouts.outline_groups(sheet, axis);
            let mut markers = std::collections::BTreeSet::new();
            match axis {
                Axis::Rows => markers.extend(
                    self.rows
                        .iter()
                        .filter(|(_, f)| f.collapsed)
                        .map(|(i, _)| *i),
                ),
                Axis::Columns => markers.extend(
                    self.columns
                        .iter()
                        .filter(|f| f.collapsed)
                        .flat_map(|f| f.first..=f.last),
                ),
            }
            // A foreign XLSX has one collapsed flag per summary. Select the outermost group
            // sharing that control; DNA's source-checked extension can restore every bit.
            for index in markers {
                if let Some(level) = groups
                    .iter()
                    .filter(|g| {
                        g.summary == index
                            && (g.first..=g.last).all(|i| match axis {
                                Axis::Rows => self.rows.get(&i).is_some_and(|f| f.hidden),
                                Axis::Columns => self.column(i).is_some_and(|f| f.hidden),
                            })
                    })
                    .map(|g| g.level)
                    .min()
                {
                    let f = layouts.get(sheet, axis, index);
                    layouts.set(
                        AxisFormatEdit::new(
                            sheet,
                            axis,
                            index,
                            f.with_outline(f.outline_level(), 1 << (level - 1)).unwrap(),
                        )
                        .unwrap(),
                    );
                }
            }
        }
        if let Some(state) = &self.outline_state {
            state.restore(sheet, layouts);
        } else {
            // Imported physical hiding beneath a collapsed group belongs to that outline.
            // XLSX cannot also identify manual hiding there without DNA's extension.
            let outlined = layouts.outlined();
            for axis in [Axis::Rows, Axis::Columns] {
                let groups = layouts.outline_groups(sheet, axis);
                let indices: Vec<_> = layouts
                    .bands(sheet, axis)
                    .filter(|(i, _)| {
                        groups
                            .iter()
                            .any(|g| g.collapsed && *i >= g.first && *i <= g.last)
                    })
                    .map(|(i, _)| i)
                    .collect();
                for index in indices {
                    let f = layouts.get(sheet, axis, index);
                    if outlined.get(sheet, axis, index).hidden() {
                        layouts.set(
                            AxisFormatEdit::new(sheet, axis, index, f.with_hidden(false)).unwrap(),
                        );
                    }
                }
            }
        }
    }
}
