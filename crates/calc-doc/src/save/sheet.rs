//! Rewriting one worksheet part for an authored save, streaming its rows in order.
//!
//! Every byte outside `<sheetData>` is kept as the part wrote it, except the `ref` of its
//! `<dimension>`, which is recomputed as the area of the cells the rewritten part holds (a part
//! without a `<dimension>` gets none). Inside `<sheetData>`:
//!
//! - A changed cell is written from its new input with the style index (`s`) and the other
//!   attributes its source cell had: a number as `<v>`; a text as the index of a shared string;
//!   a logical or an error as `<v>` with `t` `b` or `e`; a formula as `<f>` without a `<v>`, since
//!   nothing here calculated it. An emptied cell keeps only its style (`<c r=".." s=".."/>`), or
//!   is removed when it had none. A new cell goes into its row at its column, before the row's
//!   `extLst` when it has one, and a new row between the rows around it.
//! - Every formula cell loses its `<v>` and its `t`: once any cell of the workbook changed, no
//!   cached result can be trusted without a dependency graph, and the workbook part is written
//!   with `fullCalcOnLoad`, which asks the application that opens the package to recalculate
//!   every formula. Each formula the edit did not change whose loaded record holds a cached result
//!   is counted as a cache dropped.
//! - A shared formula group any of whose cells changed is written as individual formulas: each
//!   remaining cell's `<f>` holds the text the model holds for it, moved from the master when the
//!   package was loaded, without the group's `t`, `ref` and `si`. Every other group keeps its
//!   `<f>` elements as the part wrote them.
//! - A row whose start tag carries `spans` (a hint of the columns the row's block of rows holds)
//!   loses it, and a row or a cell written without `r` gets one, so removing a cell cannot move
//!   the cells after it.
//! - Every other cell and row is kept as the part wrote it.
//!
//! The attributes the rewrite reads (`r`, `s`, `t`, `si`) are read from the XML reader's elements,
//! with their entity and character references replaced, as the loader reads them; the rewrite
//! copies or rebuilds each start tag from the part's own bytes.

use super::SaveRefusal;
use super::diff::SheetChanges;
use super::reader::Reader;
use super::strings::SharedStrings;
use super::text::{RawTag, Splice, a1, number, xstring};
use crate::load::address;
use crate::load::sheet::{ERRORS, collapsed};
use crate::load::{LoadedSheet, MAIN_NAMESPACE};
use crate::xml::{XmlElement, XmlEvent, XmlPart};
use calc_doc_model::value::CellKey;
use calc_doc_model::{CellInput, ModelRoot};
use std::collections::{BTreeMap, BTreeSet};

/// What one sheet's rewrite reads beside its part.
pub(super) struct Job<'a> {
    pub(super) changes: &'a SheetChanges,
    pub(super) loaded: &'a LoadedSheet,
    pub(super) model: &'a ModelRoot,
}

/// The area of the cells written, as (top, left, bottom, right).
#[derive(Default)]
struct Bounds(Option<(u32, u32, u32, u32)>);

impl Bounds {
    fn add(&mut self, row: u32, column: u32) {
        self.0 = Some(match self.0 {
            None => (row, column, row, column),
            Some((top, left, bottom, right)) => (
                top.min(row),
                left.min(column),
                bottom.max(row),
                right.max(column),
            ),
        });
    }

    /// The area as a `<dimension>` gives it: `A1` for a sheet without cells.
    fn text(&self) -> String {
        match self.0 {
            None => "A1".to_owned(),
            Some((top, left, bottom, right)) if top == bottom && left == right => a1(top, left),
            Some((top, left, bottom, right)) => {
                format!("{}:{}", a1(top, left), a1(bottom, right))
            }
        }
    }
}

struct Rewrite<'a, 'j> {
    job: &'j Job<'a>,
    reader: Reader<'j>,
    splice: Splice<'j>,
    strings: &'j mut SharedStrings,
    /// The shared formula groups (`si`) written as individual formulas.
    unshared: BTreeSet<u32>,
    bounds: Bounds,
    /// The formula cells the edit did not change whose cached result the rewrite left out.
    caches_dropped: usize,
}

/// A rewritten worksheet part.
pub(super) struct Rewritten {
    pub(super) bytes: Vec<u8>,
    /// The formula cells the edit did not change whose cached result was left out.
    pub(super) caches_dropped: usize,
}

/// A cell's `<f>` element: as read, and the byte ranges of its start tag and of the whole element.
struct FormulaChild {
    read: XmlElement,
    tag: (usize, usize),
    element: (usize, usize),
}

/// A cell's changes, by column, of one row.
type RowChanges<'c> = BTreeMap<u32, Option<&'c CellInput>>;

fn end_tag(prefix: &[u8], local: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(b"</");
    out.extend_from_slice(prefix);
    out.extend_from_slice(local);
    out.push(b'>');
}

fn start_tag(prefix: &[u8], local: &[u8], out: &mut Vec<u8>) {
    out.push(b'<');
    out.extend_from_slice(prefix);
    out.extend_from_slice(local);
    out.push(b'>');
}

/// The worksheet part `xml` rewritten for `job`'s changes.
pub(super) fn rewrite(
    xml: &XmlPart,
    job: &Job<'_>,
    strings: &mut SharedStrings,
) -> Result<Rewritten, SaveRefusal> {
    let unshared = job
        .loaded
        .formulas()
        .filter(|(row, column, _)| job.changes.contains_key(&(*row, *column)))
        .filter_map(|(_, _, record)| record.shared().map(|group| group.index()))
        .collect();
    let rewrite = Rewrite {
        job,
        reader: Reader::new(xml),
        splice: Splice::new(xml.bytes()),
        strings,
        unshared,
        bounds: Bounds::default(),
        caches_dropped: 0,
    };
    rewrite.worksheet()
}

/// Whether a cell's style index (`s`, 0 when absent) is not 0, as the loader reads it.
fn styled(element: &XmlElement) -> bool {
    element
        .attribute("s")
        .is_some_and(|style| collapsed(style).parse::<u32>() != Ok(0))
}

impl<'j> Rewrite<'_, 'j> {
    fn source(&self) -> &'j [u8] {
        self.splice.source()
    }

    fn worksheet(mut self) -> Result<Rewritten, SaveRefusal> {
        self.reader.root("worksheet")?;
        // Where the `<dimension>` goes in the output, and its source start tag.
        let mut dimension: Option<(usize, (usize, usize))> = None;
        let mut sheet_data = false;
        loop {
            match self.reader.expect()? {
                (XmlEvent::Start(element), span)
                    if element.name.is(MAIN_NAMESPACE, "dimension") =>
                {
                    let end = self.reader.element_end()?;
                    self.splice.keep_to(span.0);
                    dimension = Some((self.splice.out.len(), span));
                    self.splice.skip_to(end);
                }
                (XmlEvent::Start(element), span)
                    if element.name.is(MAIN_NAMESPACE, "sheetData") =>
                {
                    if sheet_data {
                        return Err(self
                            .reader
                            .invalid("a worksheet with two sheetData elements"));
                    }
                    sheet_data = true;
                    self.sheet_data(span)?;
                }
                (XmlEvent::Start(_), _) => {
                    self.reader.element_end()?;
                }
                (XmlEvent::Text(_), _) => {}
                (XmlEvent::End(_), _) => break,
            }
        }
        self.reader.end()?;
        if !sheet_data {
            return Err(self.reader.invalid("a worksheet without sheetData"));
        }
        let text = self.bounds.text();
        let source = self.splice.source();
        let mut bytes = self.splice.finish();
        if let Some((at, span)) = dimension {
            let tag = RawTag::parse(&source[span.0..span.1]);
            bytes.splice(at..at, tag.rebuilt(&[("ref", Some(&text))], true));
        }
        Ok(Rewritten {
            bytes,
            caches_dropped: self.caches_dropped,
        })
    }

    fn sheet_data(&mut self, span: (usize, usize)) -> Result<(), SaveRefusal> {
        let changes = self.job.changes;
        let mut rows: BTreeMap<u32, RowChanges<'_>> = BTreeMap::new();
        for (&(row, column), input) in changes {
            rows.entry(row).or_default().insert(column, input.as_ref());
        }
        let mut pending = rows.into_iter().peekable();
        let tag = RawTag::parse(&self.source()[span.0..span.1]);
        let prefix = tag.prefix().to_vec();
        if tag.is_empty() {
            self.reader.expect()?;
            let mut rows = Vec::new();
            for (row, cells) in pending {
                self.new_row(row, &cells, &prefix, &mut rows);
            }
            if !rows.is_empty() {
                let mut bytes = tag.rebuilt(&[], false);
                bytes.extend_from_slice(&rows);
                end_tag(&prefix, b"sheetData", &mut bytes);
                self.splice.replace(span, &bytes);
            }
            return Ok(());
        }
        let mut previous = 0u32;
        loop {
            match self.reader.expect()? {
                (XmlEvent::Start(element), span) if element.name.is(MAIN_NAMESPACE, "row") => {
                    let tag = RawTag::parse(&self.source()[span.0..span.1]);
                    let number = match element.attribute("r") {
                        Some(text) => address::row_number(collapsed(text))
                            .ok_or_else(|| self.reader.invalid("a row number outside the grid"))?,
                        None => previous + 1,
                    };
                    if number <= previous {
                        return Err(self
                            .reader
                            .invalid("a row that is not after the row before it"));
                    }
                    previous = number;
                    while let Some((row, cells)) = pending.next_if(|(row, _)| *row < number) {
                        let mut bytes = Vec::new();
                        self.new_row(row, &cells, &prefix, &mut bytes);
                        self.splice.replace((span.0, span.0), &bytes);
                    }
                    let cells = pending
                        .next_if(|(row, _)| *row == number)
                        .map(|(_, cells)| cells)
                        .unwrap_or_default();
                    self.row(span, &element, &tag, number, &cells)?;
                }
                (XmlEvent::Start(_), _) => {
                    return Err(self
                        .reader
                        .invalid("an element other than row in sheetData"));
                }
                (XmlEvent::Text(_), _) => {}
                (XmlEvent::End(_), (start, _)) => {
                    let mut bytes = Vec::new();
                    for (row, cells) in pending {
                        self.new_row(row, &cells, &prefix, &mut bytes);
                    }
                    self.splice.replace((start, start), &bytes);
                    return Ok(());
                }
            }
        }
    }

    /// Appends a row the part did not hold, with the cells set in it.
    fn new_row(&mut self, row: u32, cells: &RowChanges<'_>, prefix: &[u8], out: &mut Vec<u8>) {
        if cells.values().all(Option::is_none) {
            return;
        }
        out.push(b'<');
        out.extend_from_slice(prefix);
        out.extend_from_slice(format!("row r=\"{row}\">").as_bytes());
        for (&column, input) in cells {
            if let Some(input) = input {
                self.new_cell(None, prefix, row, column, input, out);
            }
        }
        end_tag(prefix, b"row", out);
    }

    /// Appends a cell holding `input`, its start tag rebuilt from `source` (the cell it replaces)
    /// or new.
    fn new_cell(
        &mut self,
        source: Option<&RawTag<'_>>,
        prefix: &[u8],
        row: u32,
        column: u32,
        input: &CellInput,
        out: &mut Vec<u8>,
    ) {
        let at = a1(row, column);
        let kind = match input {
            CellInput::Number(_) | CellInput::Formula(_) => None,
            CellInput::Text(_) => Some("s"),
            CellInput::Logical(_) => Some("b"),
            CellInput::Error(_) => Some("e"),
        };
        let prefix = source.map_or(prefix, |tag| tag.prefix());
        match source {
            Some(tag) => out.extend(tag.rebuilt(&[("r", Some(&at)), ("t", kind)], false)),
            None => {
                out.push(b'<');
                out.extend_from_slice(prefix);
                out.extend_from_slice(format!("c r=\"{at}\"").as_bytes());
                if let Some(kind) = kind {
                    out.extend_from_slice(format!(" t=\"{kind}\"").as_bytes());
                }
                out.push(b'>');
            }
        }
        if let CellInput::Formula(formula) = input {
            start_tag(prefix, b"f", out);
            xstring(formula.text().utf16_code_units(), out);
            end_tag(prefix, b"f", out);
        } else {
            let value = match input {
                CellInput::Number(value) => number(value.get()),
                CellInput::Text(text) => {
                    self.strings.added += 1;
                    self.strings
                        .index(text.text().utf16_code_units())
                        .to_string()
                }
                CellInput::Logical(value) => u8::from(*value).to_string(),
                CellInput::Error(code) => ERRORS
                    .iter()
                    .find(|(_, known)| known == code)
                    .map(|(spelling, _)| (*spelling).to_owned())
                    .expect("every worksheet error has a spelling"),
                CellInput::Formula(_) => unreachable!("written above"),
            };
            start_tag(prefix, b"v", out);
            out.extend_from_slice(value.as_bytes());
            end_tag(prefix, b"v", out);
        }
        end_tag(prefix, b"c", out);
        self.bounds.add(row, column);
    }

    /// Rewrites one row of the part, whose start tag, read as `element`, is `tag` and spans
    /// `span`.
    fn row(
        &mut self,
        span: (usize, usize),
        element: &XmlElement,
        tag: &RawTag<'_>,
        number: u32,
        cells: &RowChanges<'_>,
    ) -> Result<(), SaveRefusal> {
        let prefix = tag.prefix().to_vec();
        let row_text = number.to_string();
        let changes: [(&str, Option<&str>); 2] = [("spans", None), ("r", Some(&row_text))];
        let retag = element.attribute("spans").is_some() || element.attribute("r").is_none();
        if tag.is_empty() {
            self.reader.expect()?;
            let mut added = Vec::new();
            for (&column, input) in cells {
                if let Some(input) = input {
                    self.new_cell(None, &prefix, number, column, input, &mut added);
                }
            }
            if !added.is_empty() {
                let mut bytes = tag.rebuilt(&changes, false);
                bytes.extend_from_slice(&added);
                end_tag(&prefix, b"row", &mut bytes);
                self.splice.replace(span, &bytes);
            } else if retag {
                self.splice.replace(span, &tag.rebuilt(&changes, true));
            }
            return Ok(());
        }
        if retag {
            self.splice.replace(span, &tag.rebuilt(&changes, false));
        }
        let mut pending = cells.iter().peekable();
        let mut previous = 0u32;
        loop {
            match self.reader.expect()? {
                (XmlEvent::Start(element), span) if element.name.is(MAIN_NAMESPACE, "c") => {
                    let source = self.source();
                    let tag = RawTag::parse(&source[span.0..span.1]);
                    let column = match element.attribute("r") {
                        Some(text) => match address::cell(text) {
                            Some((row, column)) if row == number => column,
                            _ => return Err(self.reader.invalid("a cell outside its row")),
                        },
                        None => previous + 1,
                    };
                    if column <= previous {
                        return Err(self
                            .reader
                            .invalid("a cell that is not after the cell before it"));
                    }
                    previous = column;
                    while let Some((&new, input)) = pending.next_if(|(new, _)| **new < column) {
                        if let Some(input) = input {
                            let mut bytes = Vec::new();
                            self.new_cell(None, &prefix, number, new, input, &mut bytes);
                            self.splice.replace((span.0, span.0), &bytes);
                        }
                    }
                    let change = pending
                        .next_if(|(new, _)| **new == column)
                        .map(|(_, input)| *input);
                    self.cell(span, &element, &tag, number, column, change)?;
                }
                (XmlEvent::Start(_), (start, _)) => {
                    // A row holds its cells and then an `extLst` (CT_Row), so the cells still to
                    // add go before the first element that is not a cell. A cell after such an
                    // element must then lie after every cell added, or it is refused below.
                    for (&column, input) in pending.by_ref() {
                        if let Some(input) = input {
                            let mut bytes = Vec::new();
                            self.new_cell(None, &prefix, number, column, input, &mut bytes);
                            self.splice.replace((start, start), &bytes);
                        }
                        previous = previous.max(column);
                    }
                    self.reader.element_end()?;
                }
                (XmlEvent::Text(_), _) => {}
                (XmlEvent::End(_), (start, _)) => {
                    let mut bytes = Vec::new();
                    for (&column, input) in pending {
                        if let Some(input) = input {
                            self.new_cell(None, &prefix, number, column, input, &mut bytes);
                        }
                    }
                    self.splice.replace((start, start), &bytes);
                    return Ok(());
                }
            }
        }
    }

    /// Rewrites one cell of the part, whose start tag, read as `element`, is `tag` and spans
    /// `span`; `change` is its new input (`Some(None)` when it was emptied), `None` when it did
    /// not change.
    fn cell(
        &mut self,
        span: (usize, usize),
        element: &XmlElement,
        tag: &RawTag<'_>,
        row: u32,
        column: u32,
        change: Option<Option<&CellInput>>,
    ) -> Result<(), SaveRefusal> {
        // The cell's `<f>` and `<v>`, and where it ends.
        let mut formula: Option<FormulaChild> = None;
        let mut value: Option<(usize, usize)> = None;
        let end = if tag.is_empty() {
            self.reader.expect()?;
            span.1
        } else {
            loop {
                match self.reader.expect()? {
                    (XmlEvent::Start(child), child_span) => {
                        let child_end = self.reader.element_end()?;
                        if child.name.is(MAIN_NAMESPACE, "f") {
                            formula = Some(FormulaChild {
                                read: child,
                                tag: child_span,
                                element: (child_span.0, child_end),
                            });
                        } else if child.name.is(MAIN_NAMESPACE, "v") {
                            value = Some((child_span.0, child_end));
                        }
                    }
                    (XmlEvent::Text(_), _) => {}
                    (XmlEvent::End(_), (_, end)) => break end,
                }
            }
        };
        let shared_string = element.attribute("t") == Some("s");
        let at = a1(row, column);
        match change {
            Some(Some(input)) => {
                let mut bytes = Vec::new();
                self.new_cell(Some(tag), b"", row, column, input, &mut bytes);
                self.splice.replace((span.0, end), &bytes);
                if shared_string {
                    self.strings.removed += 1;
                }
            }
            Some(None) => {
                let bytes = if styled(element) {
                    self.bounds.add(row, column);
                    tag.rebuilt(&[("r", Some(&at)), ("t", None)], true)
                } else {
                    Vec::new()
                };
                self.splice.replace((span.0, end), &bytes);
                if shared_string {
                    self.strings.removed += 1;
                }
            }
            None => {
                self.bounds.add(row, column);
                let Some(FormulaChild {
                    read: formula_read,
                    tag: formula_tag,
                    element: formula_element,
                }) = formula
                else {
                    if element.attribute("r").is_none() {
                        self.splice
                            .replace(span, &tag.rebuilt(&[("r", Some(&at))], tag.is_empty()));
                    }
                    return Ok(());
                };
                // A formula cell: its cache and its type go, and a cell of a group written as
                // individual formulas gets its own text.
                self.splice
                    .replace(span, &tag.rebuilt(&[("r", Some(&at)), ("t", None)], false));
                if shared_string {
                    self.strings.removed += 1;
                }
                let source = self.source();
                let formula_raw = RawTag::parse(&source[formula_tag.0..formula_tag.1]);
                let group = formula_read
                    .attribute("si")
                    .and_then(|si| collapsed(si).parse::<u32>().ok())
                    .filter(|_| formula_read.attribute("t") == Some("shared"));
                let mut edits: Vec<((usize, usize), Vec<u8>)> = Vec::new();
                if let Some(group) = group
                    && self.unshared.contains(&group)
                {
                    let key = CellKey::new(self.job.loaded.id(), row, column)
                        .expect("a cell of the part lies in the grid");
                    let Some(CellInput::Formula(text)) = self.job.model.cell(key) else {
                        return Err(self.reader.invalid(
                            "a shared formula cell the loaded model does not hold as a formula",
                        ));
                    };
                    let prefix = formula_raw.prefix();
                    let mut bytes =
                        formula_raw.rebuilt(&[("t", None), ("ref", None), ("si", None)], false);
                    xstring(text.text().utf16_code_units(), &mut bytes);
                    end_tag(prefix, b"f", &mut bytes);
                    edits.push((formula_element, bytes));
                }
                if let Some(value) = value {
                    edits.push((value, Vec::new()));
                    let cached = self
                        .job
                        .loaded
                        .formula(row, column)
                        .and_then(|record| record.cache())
                        .is_some();
                    if cached {
                        self.caches_dropped += 1;
                    }
                }
                edits.sort_by_key(|(range, _)| range.0);
                for (range, bytes) in edits {
                    self.splice.replace(range, &bytes);
                }
            }
        }
        Ok(())
    }
}
