//! A sheet's sparklines with their values now (dc-zjsg.7): each group as the file states it, and each sparkline's
//! cell and the numbers of its data range, read as a chart series reads its data (the reference rewritten through
//! the session's row and column inserts and deletes, then evaluated). A value that is not a number (text, an
//! error, a blank cell) is an empty point. A value in a hidden row or column is left out unless the group shows
//! hidden cells (`displayHidden`), as Excel leaves out a chart's hidden points; on a date axis each point stands at
//! its date (the group's own date range, read the same way), a point without a date left out.

use crate::state::HostState;
use calc_doc_model::value::{CellKey, CoreRef, GridArea, SheetId};
use calc_doc_model::{ChartData, FormControl, SparklineGroup};

fn sheet_of(area: calc_doc_model::value::GridArea) -> SheetId {
    area.first().sheet()
}

/// One sparkline: the cell it is drawn in and its values in order.
#[derive(Clone, Debug, PartialEq)]
pub struct SparklineView {
    /// Worksheet cell where the sparkline is drawn.
    pub cell: CellKey,
    /// Numeric data points in reading order. A missing value denotes an empty point.
    pub values: Vec<Option<f64>>,
    /// On a date axis, the date each value stands at (the same length as `values`); empty otherwise.
    pub dates: Vec<Option<f64>>,
}

impl HostState {
    fn form_control_link_area(&self, sheet: SheetId, control: &FormControl) -> Option<GridArea> {
        let origin = CellKey::new(sheet, 1, 1).ok()?;
        let link = self.current_reference(control.link.as_deref()?, sheet);
        match self.resolve_reference_text(origin, &link) {
            crate::ReferenceTarget::Area(area) => Some(area),
            _ => None,
        }
    }

    /// The statically linked single cells of this sheet's Form Controls.
    pub fn form_control_links(&self, sheet: SheetId) -> impl Iterator<Item = (&FormControl, CellKey)> {
        self.loaded.sheet(sheet).into_iter().flat_map(move |loaded| {
            loaded.shapes().controls.iter().filter_map(move |control| {
                let area = self.form_control_link_area(sheet, control)?;
                (area.cell_count() == 1).then(|| (control, area.first()))
            })
        })
    }

    /// A sparkline group's colour against the workbook's theme (a theme colour with its tint as cells take it).
    pub fn sparkline_colour(&self, color: &calc_doc_model::StyleColor) -> Option<crate::LookColor> {
        crate::conditional::resolve(self, color)
    }

    /// The sparkline groups of `sheet` with their sparklines' cells and values; a sparkline whose cell does not
    /// resolve is left out.
    pub fn sparklines(&self, sheet: SheetId) -> Vec<(SparklineGroup, Vec<SparklineView>)> {
        let Ok(origin) = CellKey::new(sheet, 1, 1) else { return Vec::new() };
        self.model
            .definitions()
            .sparklines(sheet)
            .iter()
            .map(|group| {
                let views = group
                    .sparklines
                    .iter()
                    .filter_map(|line| {
                        let cell_text = self.current_reference(&line.cell, sheet);
                        let crate::ReferenceTarget::Area(area) = self.resolve_reference_text(origin, &cell_text) else {
                            return None;
                        };
                        let cell = area.first();
                        // A data area is read cell by cell, so a blank cell is an empty point (a reference's
                        // evaluation gives it as 0); anything else is evaluated as a chart series is.
                        let data_text = self.current_reference(&line.data, sheet);
                        if let crate::ReferenceTarget::Area(data) = self.resolve_reference_text(cell, &data_text)
                            && data.cell_count() <= 10_000
                        {
                            let mut values = self.sparkline_numbers(data, group.hidden);
                            let mut dates = Vec::new();
                            if group.date_axis
                                && let Some(text) = group.dates.as_deref()
                                && let crate::ReferenceTarget::Area(range) = self.resolve_reference_text(cell, &self.current_reference(text, sheet))
                                && range.cell_count() <= 10_000
                            {
                                dates = self.sparkline_numbers(range, group.hidden);
                                dates.resize(values.len(), None);
                                values.truncate(dates.len());
                            }
                            return Some(SparklineView { cell, values, dates });
                        }
                        let data = ChartData { formula: Some(line.data.clone()), cache: Vec::new(), numeric: true };
                        let (values, cached) = self.data_values(&data, cell);
                        let values = if cached {
                            Vec::new()
                        } else {
                            values
                                .iter()
                                .map(|v| match v.as_ref().map(|v| v.core_ref()) {
                                    Some(CoreRef::Number(n)) => Some(n),
                                    _ => None,
                                })
                                .collect()
                        };
                        Some(SparklineView { cell, values, dates: Vec::new() })
                    })
                    .collect();
                (group.clone(), views)
            })
            .collect()
    }
}

impl HostState {
    /// The numbers of `area` cell by cell in reading order (anything else an empty point), the cells of hidden rows
    /// and columns left out unless `hidden` (the group's `displayHidden`).
    fn sparkline_numbers(&self, area: calc_doc_model::value::GridArea, hidden: bool) -> Vec<Option<f64>> {
        let sheet = sheet_of(area);
        let layouts = self.model.definitions().axis_layouts();
        let shown = |axis, index| hidden || !layouts.get(sheet, axis, index).hidden();
        let (first, last) = (area.first(), area.last());
        (first.row()..=last.row())
            .filter(|&row| shown(calc_doc_model::Axis::Rows, row))
            .flat_map(|row| (first.column()..=last.column()).filter(move |&c| shown(calc_doc_model::Axis::Columns, c)).map(move |column| (row, column)))
            .filter_map(|(row, column)| CellKey::new(sheet, row, column).ok())
            .map(|at| match crate::conditional::cell_value(self, at).as_ref().map(|v| v.core_ref()) {
                Some(CoreRef::Number(n)) => Some(n),
                _ => None,
            })
            .collect()
    }
}

/// A legacy form control with what it shows now (dc-zjsg.8): its linked cell and that cell's value, and the texts of
/// its input range (a list or combo box's items).
#[derive(Clone, Debug, PartialEq)]
pub struct FormControlView {
    /// Imported form control definition.
    pub control: calc_doc_model::FormControl,
    /// First cell of the resolved linked range, when available.
    pub link: Option<CellKey>,
    /// Current value of the linked cell, when available.
    pub linked: Option<calc_doc_model::value::CalcValue>,
    /// Input range values converted to display text in reading order.
    pub items: Vec<String>,
}

impl HostState {
    /// The sheet's form controls as the file states them, each with its linked cell's current value and its input
    /// range's texts (references rewritten through the session's inserts and deletes).
    pub fn form_controls(&self, sheet: SheetId) -> Vec<FormControlView> {
        let Ok(origin) = CellKey::new(sheet, 1, 1) else { return Vec::new() };
        let Some(loaded) = self.loaded.sheet(sheet) else { return Vec::new() };
        loaded
            .shapes()
            .controls
            .iter()
            .map(|control| {
                let link = self.form_control_link_area(sheet, control).map(GridArea::first);
                let linked = link.and_then(|at| crate::conditional::cell_value(self, at));
                let items = control
                    .range
                    .as_deref()
                    .and_then(|text| match self.resolve_reference_text(origin, &self.current_reference(text, sheet)) {
                        crate::ReferenceTarget::Area(area) if area.cell_count() <= 10_000 => Some(area),
                        _ => None,
                    })
                    .map(|area| {
                        let (first, last) = (area.first(), area.last());
                        (first.row()..=last.row())
                            .flat_map(|row| (first.column()..=last.column()).map(move |column| (row, column)))
                            .filter_map(|(row, column)| CellKey::new(sheet_of(area), row, column).ok())
                            .map(|at| crate::conditional::cell_value(self, at).as_ref().map(crate::chart::text_of).unwrap_or_default())
                            .collect()
                    })
                    .unwrap_or_default();
                FormControlView { control: control.clone(), link, linked, items }
            })
            .collect()
    }
}

/// One change to a sheet's sparklines (dc-zjsg.7), each one undoable edit. A sparkline is found by its cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SparklineChange {
    /// A new sparkline group of one sparkline in `cell` over `data` (one row or one column), `kind` `line` (the
    /// default), `column` or `stacked` (win/loss), with the look Excel gives a sparkline it inserts.
    Add {
        /// Data range, one row or one column.
        data: calc_doc_model::value::GridArea,
        /// Destination cell for the new sparkline.
        cell: CellKey,
        /// Sparkline kind. Omit it to use a line.
        kind: Option<String>,
    },
    /// The type of the group holding the sparkline in `cell` (Excel changes the whole group).
    Kind {
        /// Cell identifying the group to update.
        cell: CellKey,
        /// New sparkline kind. Omit it to use a line.
        kind: Option<String>,
    },
    /// Which points the group holding the sparkline in `cell` shows apart: `markers`, `high`, `low`, `first`,
    /// `last` or `negative`.
    Show {
        /// Cell identifying the group to update.
        cell: CellKey,
        /// Point category whose display changes.
        point: String,
        /// Show the chosen category when true.
        on: bool,
    },
    /// The sparkline in `cell` deleted (its group with it when it was the last).
    Delete {
        /// Cell whose sparkline should be removed.
        cell: CellKey,
    },
}

fn refusal(s: &str) -> String {
    format!("{s} (dc-zjsg.7); nothing changed")
}

fn a1(at: CellKey) -> String {
    let mut n = at.column();
    let mut letters = String::new();
    while n > 0 {
        n -= 1;
        letters.insert(0, (b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    format!("{letters}{}", at.row())
}

/// Where the sparkline `line` of `sheet` is drawn, as `sparklines` resolves it.
fn cell_of(host: &HostState, sheet: SheetId, line: &calc_doc_model::Sparkline) -> Option<CellKey> {
    let origin = CellKey::new(sheet, 1, 1).ok()?;
    match host.resolve_reference_text(origin, &host.current_reference(&line.cell, sheet)) {
        crate::ReferenceTarget::Area(area) => Some(area.first()),
        _ => None,
    }
}

pub(crate) fn prepare(host: &HostState, sheet: SheetId, revision: u64, change: SparklineChange) -> Result<Vec<crate::CellEdit>, String> {
    if revision != host.revision().get() {
        return Err(refusal("The workbook changed; read the sparklines again"));
    }
    let source = host.loaded().sheet(sheet).ok_or_else(|| refusal("Save and reopen the new worksheet before adding sparklines"))?;
    if source.protected() || source.is_macro_sheet() {
        return Err(refusal("Sparklines need an editable worksheet"));
    }
    if host.loaded().workbook_part().ends_with(".bin") {
        return Err(refusal("Save as XLSX and reopen before editing sparklines"));
    }
    if !host.axis_edits.is_empty() {
        return Err(refusal("Save and reopen before editing sparklines after row or column insertion or deletion"));
    }
    let mut groups = host.model.definitions().sparklines(sheet).to_vec();
    let find = |groups: &[SparklineGroup], cell: CellKey| -> Option<(usize, usize)> {
        groups.iter().enumerate().find_map(|(g, group)| {
            group.sparklines.iter().position(|line| cell_of(host, sheet, line) == Some(cell)).map(|l| (g, l))
        })
    };
    let kind_of = |kind: Option<String>| -> Result<Option<String>, String> {
        match kind.as_deref() {
            None | Some("line") => Ok(None),
            Some("column" | "stacked") => Ok(kind),
            Some(_) => Err(refusal("Use a line, column or win/loss sparkline")),
        }
    };
    match change {
        SparklineChange::Add { data, cell, kind } => {
            if cell.sheet() != sheet {
                return Err(refusal("Put the sparkline on this sheet"));
            }
            if data.row_count() > 1 && data.column_count() > 1 {
                return Err(refusal("A sparkline in one cell takes one row or one column of data"));
            }
            if find(&groups, cell).is_some() {
                return Err(refusal("The cell already holds a sparkline"));
            }
            let name = host
                .model()
                .sheet_by_id(data.sheet())
                .ok_or_else(|| refusal("The data's worksheet is unavailable"))?
                .name()
                .text()
                .to_string_lossy()
                .replace('\'', "''");
            let range = if data.first() == data.last() { a1(data.first()) } else { format!("{}:{}", a1(data.first()), a1(data.last())) };
            let colour = |theme: &str, tint: Option<&str>| calc_doc_model::StyleColor {
                theme: Some(theme.into()),
                tint: tint.map(str::to_owned),
                ..Default::default()
            };
            let darker = Some("-0.499984740745262");
            let lighter = Some("0.39997558519241921");
            // Excel's look for a sparkline it inserts: the first accent darkened by half, negative points in the
            // second accent, a black axis, first and last points lightened (excel-answers, sparklines.xlsx).
            let group = SparklineGroup {
                kind: kind_of(kind)?,
                empty_cells: Some("gap".into()),
                colours: vec![
                    ("colorSeries".into(), colour("4", darker)),
                    ("colorNegative".into(), colour("5", None)),
                    ("colorAxis".into(), calc_doc_model::StyleColor { rgb: Some("FF000000".into()), ..Default::default() }),
                    ("colorMarkers".into(), colour("4", darker)),
                    ("colorFirst".into(), colour("4", lighter)),
                    ("colorLast".into(), colour("4", lighter)),
                    ("colorHigh".into(), colour("4", None)),
                    ("colorLow".into(), colour("4", None)),
                ],
                sparklines: vec![calc_doc_model::Sparkline { data: crate::chart::excel_reference(&format!("'{name}'!{range}")), cell: a1(cell) }],
                ..Default::default()
            };
            groups.push(group);
        }
        SparklineChange::Kind { cell, kind } => {
            let (g, _) = find(&groups, cell).ok_or_else(|| refusal("The cell holds no sparkline"))?;
            groups[g].kind = kind_of(kind)?;
        }
        SparklineChange::Show { cell, point, on } => {
            let (g, _) = find(&groups, cell).ok_or_else(|| refusal("The cell holds no sparkline"))?;
            let group = &mut groups[g];
            let flag = match point.as_str() {
                "markers" => &mut group.markers,
                "high" => &mut group.high,
                "low" => &mut group.low,
                "first" => &mut group.first,
                "last" => &mut group.last,
                "negative" => &mut group.negative,
                _ => return Err(refusal("Show markers, high, low, first, last or negative points")),
            };
            *flag = on;
        }
        SparklineChange::Delete { cell } => {
            let (g, l) = find(&groups, cell).ok_or_else(|| refusal("The cell holds no sparkline"))?;
            groups[g].sparklines.remove(l);
            if groups[g].sparklines.is_empty() {
                groups.remove(g);
            }
        }
    }
    Ok(vec![crate::CellEdit::Sparklines { sheet, groups }])
}
