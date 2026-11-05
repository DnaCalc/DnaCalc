//! A save after rows or columns were inserted or deleted (dc-jhn): the package's worksheets and
//! the parts that name their cells, moved as Excel's own insertion and deletion moves them, before
//! the authored rewrite ([`super::sheet`]) writes the revision's cells.
//!
//! Each edit, in order, makes one pass over each part:
//!
//! - The edited sheet's worksheet part: its rows and cells take their new positions (`r`), those
//!   the edit deletes go, and an inserted band takes the format of the row above or the column to
//!   the left (the row's attributes but `hidden` and `collapsed`, each styled cell's `s`, the
//!   column span's attributes but `hidden`), as Excel's insertion does by default (measured,
//!   calc-host's tests/fixtures/d2/structural-excel.json). A shared formula's, an array
//!   formula's and a data table's `ref` move with their cells; a shared formula group whose area
//!   the edit cuts or grows is written as individual formulas, since its members' offsets from
//!   its master change. Merged cells, conditional formats, data validations, hyperlinks, ignored
//!   errors, protected ranges, the sheet's filter and sort, its page breaks, its views' selection
//!   and scroll position, and its column spans move as references do (grown by an insertion
//!   inside them, shrunk by a deletion, gone when all of them is deleted).
//! - Every worksheet part: the formulas of its conditional formats and data validations, and of
//!   its extensions (`extLst`: `xm:f`, where Excel writes a validation naming another sheet, and
//!   sparklines), are rewritten as the model rewrites a defined name's formula (the caller's
//!   [`FormulaRewriter`]), since they may name the edited sheet's cells; on the edited sheet the
//!   extensions' areas (`xm:sqref`) move.
//! - The edited sheet's comments, threaded comments, note shapes and drawing anchors, and its
//!   tables' areas, filters and sorts, move with the cells; a comment on a deleted cell goes.
//! - Every chart's series formulas (`c:f`) are rewritten.
//!
//! What names the edited sheet's cells and is not moved here is refused
//! ([`SaveRefusal::StructuralNotMoved`]): a pivot table or cache over the sheet, a relationship of
//! the sheet of a kind not listed above, a worksheet element that holds positions this pass does
//! not move (scenarios, OLE objects, controls, cell watches, custom sheet views, data
//! consolidation, smart tags, web publishing items), an extension's item all of whose cells an
//! edit deletes, and a data table whose input cell an edit deletes.

use super::SaveRefusal;
use super::reader::Reader;
use super::strings::read_part;
use super::text::{RawTag, Splice, a1, xstring};
use crate::load::address;
use crate::load::sheet::collapsed;
use crate::load::{LoadedSheet, MAIN_NAMESPACE};
use crate::opc::{Package, TargetMode};
use crate::xml::{XmlEvent, XmlPart};
use calc_doc_model::value::ExcelText;
use calc_doc_model::value::{CellKey, MAX_COLUMNS, MAX_ROWS, SheetId};
use calc_doc_model::{
    Axis, AxisChange, AxisEdit, FormulaRewriter, FormulaText, ModelRoot, NameScope,
    StructuralChange, moved_position, moved_run,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// The row and column insertions and deletions that lead from the loaded workbook to the revision
/// a save writes, in order, and the rewriter that rewrites formulas the model does not hold for
/// them (calc-engine's `ReferenceRewriter`, the one the edits were applied with).
#[derive(Clone)]
pub struct StructuralEdits {
    pub(super) edits: Arc<[AxisEdit]>,
    pub(super) rewriter: Arc<dyn FormulaRewriter + Send + Sync>,
}

impl StructuralEdits {
    pub fn new(edits: Vec<AxisEdit>, rewriter: Arc<dyn FormulaRewriter + Send + Sync>) -> Self {
        StructuralEdits {
            edits: edits.into(),
            rewriter,
        }
    }

    pub fn edits(&self) -> &[AxisEdit] {
        &self.edits
    }
}

const SPREADSHEET_DRAWING: &str =
    "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
const THREADED_COMMENTS: &str =
    "http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments";
const CHART_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";
const PIVOT_CACHE_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";

/// The relationship types of a worksheet whose targets hold no position of its cells.
const HELD_KINDS: [&str; 4] = ["printerSettings", "hyperlink", "image", "customProperty"];
const THREADED_COMMENT_KIND: &str =
    "http://schemas.microsoft.com/office/2017/10/relationships/threadedComment";

/// The worksheet elements whose positions this save does not move: refused on an edited sheet.
const NOT_MOVED: [&str; 8] = [
    "scenarios",
    "dataConsolidate",
    "customSheetViews",
    "cellWatches",
    "oleObjects",
    "controls",
    "smartTags",
    "webPublishItems",
];

/// The namespace of the formulas (`xm:f`) and areas (`xm:sqref`) of a worksheet's extensions
/// (its `extLst`): Excel 2010's data validations naming another sheet, conditional formats,
/// sparklines.
const EXTENSION_MAIN: &str = "http://schemas.microsoft.com/office/excel/2006/main";

fn not_moved(part: &str, what: impl Into<String>) -> SaveRefusal {
    SaveRefusal::StructuralNotMoved {
        part: part.to_owned(),
        what: what.into(),
    }
}

/// The position before `edit` that the position `index` after it moved from; `None` for one the
/// edit inserted, or one past the end of the axis that a deletion left empty.
fn moved_from(edit: AxisEdit, index: u32) -> Option<u32> {
    match edit.change() {
        AxisChange::Insert => {
            if index < edit.first() {
                Some(index)
            } else if index <= edit.last() {
                None
            } else {
                Some(index - edit.count())
            }
        }
        AxisChange::Delete => {
            if index < edit.first() {
                Some(index)
            } else {
                index
                    .checked_add(edit.count())
                    .filter(|from| *from <= edit.axis().size())
            }
        }
    }
}

/// Where a position goes under `edit`, a deleted one to the position after the deleted band and
/// one pushed past the end of the axis to its last: where a selection or an object's corner lands.
fn clamped(edit: AxisEdit, index: u32) -> u32 {
    moved_position(edit, index).unwrap_or_else(|| match edit.change() {
        AxisChange::Delete => edit.first().min(edit.axis().size()),
        AxisChange::Insert => edit.axis().size(),
    })
}

/// An area (top, left, bottom, right) under `edit` on its sheet.
type Area = (u32, u32, u32, u32);

fn moved_area(edit: AxisEdit, (top, left, bottom, right): Area) -> Option<Area> {
    Some(match edit.axis() {
        Axis::Rows => {
            let (top, bottom) = moved_run(edit, top, bottom)?;
            (top, left, bottom, right)
        }
        Axis::Columns => {
            let (left, right) = moved_run(edit, left, right)?;
            (top, left, bottom, right)
        }
    })
}

fn moved_cell(edit: AxisEdit, (row, column): (u32, u32)) -> Option<(u32, u32)> {
    Some(match edit.axis() {
        Axis::Rows => (moved_position(edit, row)?, column),
        Axis::Columns => (row, moved_position(edit, column)?),
    })
}

fn area_text((top, left, bottom, right): Area) -> String {
    if (top, left) == (bottom, right) {
        a1(top, left)
    } else {
        format!("{}:{}", a1(top, left), a1(bottom, right))
    }
}

/// A `sqref` (areas separated by spaces) under `edit`: `None` when every area is deleted; the
/// text unchanged when no area moves.
fn moved_sqref(edit: AxisEdit, text: &str) -> Option<String> {
    let mut out = Vec::new();
    for item in text.split_ascii_whitespace() {
        match address::area(item) {
            Some(area) => {
                if let Some(moved) = moved_area(edit, area) {
                    out.push(area_text(moved));
                }
            }
            // Not an area this reads (a whole row or column): kept as written.
            None => out.push(item.to_owned()),
        }
    }
    (!out.is_empty()).then(|| out.join(" "))
}

/// Where the positions of one sheet go under the edits on it, in order.
pub(super) struct SheetMoves {
    edits: Vec<AxisEdit>,
}

impl SheetMoves {
    /// The edits of `edits` on `sheet`; `None` when none is.
    pub(super) fn of(edits: &[AxisEdit], sheet: SheetId) -> Option<SheetMoves> {
        let edits: Vec<AxisEdit> = edits
            .iter()
            .copied()
            .filter(|edit| edit.sheet() == sheet)
            .collect();
        (!edits.is_empty()).then_some(SheetMoves { edits })
    }

    /// Where the loaded cell (row, column) is in the revision; `None` when it was deleted.
    pub(super) fn forward(&self, at: (u32, u32)) -> Option<(u32, u32)> {
        self.edits
            .iter()
            .try_fold(at, |at, edit| moved_cell(*edit, at))
    }

    /// The loaded cell the revision's cell (row, column) moved from; `None` for one inserted.
    pub(super) fn back(&self, (row, column): (u32, u32)) -> Option<(u32, u32)> {
        let (mut row, mut column) = (row, column);
        for edit in self.edits.iter().rev() {
            match edit.axis() {
                Axis::Rows => row = moved_from(*edit, row)?,
                Axis::Columns => column = moved_from(*edit, column)?,
            }
        }
        Some((row, column))
    }

    /// Where a loaded area is in the revision; `None` when all of it was deleted.
    pub(super) fn area(&self, area: Area) -> Option<Area> {
        self.edits
            .iter()
            .try_fold(area, |area, edit| moved_area(*edit, area))
    }
}

/// A rewriter of formula texts for one edit: the text the edit leaves of a formula as the file
/// stores it (without `=`), `None` when it is left as it is.
pub(super) struct Rewrite<'a> {
    pub(super) root: &'a ModelRoot,
    pub(super) rewriter: &'a dyn FormulaRewriter,
    pub(super) change: StructuralChange,
}

impl Rewrite<'_> {
    /// A formula outside any cell that names cells by their position (a chart's series): as a
    /// defined name's formula of `scope`.
    fn text(&self, formula: &str, scope: NameScope) -> Option<String> {
        self.rewriter
            .name(self.root, formula, scope, &self.change)
            .filter(|rewritten| rewritten != formula)
    }

    /// A formula whose relative references are relative to the cell `at` (a conditional format's
    /// or a data validation's, to the top-left cell of its range): as the formula of that cell.
    fn relative(&self, formula: &str, at: CellKey) -> Option<String> {
        let text = FormulaText::new(ExcelText::from_utf16_code_units(
            formula.encode_utf16().collect(),
        ))
        .ok()?;
        let rewritten = self.rewriter.formula(self.root, &text, at, &self.change)?;
        let rewritten = rewritten.text().to_string_lossy();
        (rewritten != formula).then_some(rewritten)
    }
}

/// The top-left cell of the first area of a `sqref` on `sheet`; `A1` when it names none.
fn anchor_of(sheet: SheetId, sqref: Option<&str>) -> CellKey {
    let (row, column) = sqref
        .and_then(|text| text.split_ascii_whitespace().next())
        .and_then(address::area)
        .map_or((1, 1), |(top, left, _, _)| (top, left));
    CellKey::new(sheet, row, column).expect("an address in the grid")
}

/// The row above an inserted band of rows, as read: its start tag's range and its styled cells
/// (column, style).
type AboveRow = ((usize, usize), Vec<(u32, String)>);

/// The formulas of an element's children (text, range) and its area, while an extension list is
/// read.
type ExtensionFrame = (Vec<(String, (usize, usize))>, Option<String>);

/// What one edit's pass over a worksheet part made.
#[derive(Default)]
pub(super) struct SheetPass {
    /// The part's new bytes; `None` when the pass changed nothing.
    pub(super) bytes: Option<Vec<u8>>,
    /// The cells holding a shared string that the edit deleted.
    pub(super) strings_removed: u64,
    /// The shared formula groups (`si`) whose area the edit cut or grew.
    pub(super) unshared: BTreeSet<u32>,
}

/// One pass of `edit` over the worksheet part `xml` of the sheet `sheet`.
pub(super) fn worksheet(
    xml: &XmlPart,
    sheet: SheetId,
    edit: AxisEdit,
    rewrite: &Rewrite<'_>,
) -> Result<SheetPass, SaveRefusal> {
    let mut pass = Pass::new(xml, sheet, edit, rewrite);
    pass.run()?;
    let bytes = applied(xml.bytes(), pass.edits);
    let mut out = pass.out;
    out.bytes = bytes;
    Ok(out)
}

/// `source` with `edits` (range, new bytes) applied, in order of their ranges; `None` when there
/// are none. An insertion (an empty range) goes before a replacement that starts where it is.
fn applied(source: &[u8], mut edits: Vec<((usize, usize), Vec<u8>)>) -> Option<Vec<u8>> {
    if edits.is_empty() {
        return None;
    }
    edits.sort_by_key(|(range, _)| *range);
    let mut splice = Splice::new(source);
    for (range, bytes) in edits {
        splice.replace(range, &bytes);
    }
    Some(splice.finish())
}

struct Pass<'a, 'r> {
    reader: Reader<'a>,
    source: &'a [u8],
    /// The replacements made, as (range, new bytes), applied in order when the pass ends.
    edits: Vec<((usize, usize), Vec<u8>)>,
    part: &'a str,
    sheet: SheetId,
    edit: AxisEdit,
    on_sheet: bool,
    rewrite: &'r Rewrite<'r>,
    out: SheetPass,
}

/// A child element as read: its local name in the SpreadsheetML namespace (`None` for another
/// namespace), its attributes, its start tag's range and the whole element's range.
struct Child {
    local: Option<String>,
    element: crate::xml::XmlElement,
    tag: (usize, usize),
    whole: (usize, usize),
}

impl<'a, 'r> Pass<'a, 'r> {
    fn new(xml: &'a XmlPart, sheet: SheetId, edit: AxisEdit, rewrite: &'r Rewrite<'r>) -> Self {
        Pass {
            reader: Reader::new(xml),
            source: xml.bytes(),
            edits: Vec::new(),
            part: xml.name(),
            sheet,
            edit,
            on_sheet: edit.sheet() == sheet,
            rewrite,
            out: SheetPass::default(),
        }
    }

    fn source(&self) -> &'a [u8] {
        self.source
    }

    fn replace(&mut self, range: (usize, usize), bytes: &[u8]) {
        self.edits.push((range, bytes.to_vec()));
    }

    /// Leaves out every replacement inside `range`, which is replaced whole.
    fn replace_whole(&mut self, range: (usize, usize), bytes: &[u8]) {
        self.edits
            .retain(|(edit, _)| !(range.0 <= edit.0 && edit.1 <= range.1));
        self.replace(range, bytes);
    }

    /// The tag at `range` rebuilt with `changes`.
    fn retag(&mut self, range: (usize, usize), changes: &[(&str, Option<&str>)]) {
        let source = self.source();
        let tag = RawTag::parse(&source[range.0..range.1]);
        let bytes = tag.rebuilt(changes, tag.is_empty());
        self.replace(range, &bytes);
    }

    fn run(&mut self) -> Result<(), SaveRefusal> {
        self.reader.root("worksheet")?;
        loop {
            match self.reader.expect()? {
                (XmlEvent::Start(element), span) => {
                    let main = element.name.namespace.as_deref() == Some(MAIN_NAMESPACE);
                    let local = element.name.local.as_str();
                    match (main, self.on_sheet, local) {
                        (true, true, "sheetData") => self.sheet_data(span)?,
                        (true, true, "cols") => self.cols(span)?,
                        (true, _, "conditionalFormatting") => {
                            self.conditional_formatting(&element, span)?
                        }
                        (true, _, "dataValidations") => self.data_validations(span)?,
                        (true, true, "mergeCells") => self.listed(span, "ref", true)?,
                        (true, true, "hyperlinks") => self.listed(span, "ref", false)?,
                        (true, true, "ignoredErrors") => self.listed(span, "sqref", false)?,
                        (true, true, "protectedRanges") => self.listed(span, "sqref", false)?,
                        (true, true, "rowBreaks") if self.edit.axis() == Axis::Rows => {
                            self.breaks(span)?
                        }
                        (true, true, "colBreaks") if self.edit.axis() == Axis::Columns => {
                            self.breaks(span)?
                        }
                        (true, true, "autoFilter") => self.auto_filter(&element, span)?,
                        (true, true, "sortState") => self.sort_state(&element, span)?,
                        (true, true, "sheetViews") => self.sheet_views()?,
                        (true, _, "extLst") => self.extensions()?,
                        (true, true, local) if NOT_MOVED.contains(&local) => {
                            return Err(not_moved(self.part, format!("the worksheet's {local}")));
                        }
                        _ => {
                            self.reader.element_end()?;
                        }
                    }
                }
                (XmlEvent::Text(_), _) => {}
                (XmlEvent::End(_), _) => break,
            }
        }
        self.reader.end()
    }

    /// Reads the children of the element whose start tag was just read, to its end; returns them
    /// and the range of its end tag.
    fn children(&mut self) -> Result<(Vec<Child>, (usize, usize)), SaveRefusal> {
        let mut children = Vec::new();
        loop {
            match self.reader.expect()? {
                (XmlEvent::Start(element), tag) => {
                    let end = self.reader.element_end()?;
                    let local = (element.name.namespace.as_deref() == Some(MAIN_NAMESPACE))
                        .then(|| element.name.local.to_string());
                    children.push(Child {
                        local,
                        element,
                        tag,
                        whole: (tag.0, end),
                    });
                }
                (XmlEvent::Text(_), _) => {}
                (XmlEvent::End(_), end_tag) => return Ok((children, end_tag)),
            }
        }
    }

    /// A container whose children each name an area in `attribute` (`mergeCell`, `hyperlink`,
    /// `ignoredError`, `protectedRange`): each moved, those deleted (and, for merged cells, those
    /// left one cell) left out, the container's `count` set when it has one, and the container
    /// left out when nothing remains in it.
    fn listed(
        &mut self,
        span: (usize, usize),
        attribute: &str,
        merged: bool,
    ) -> Result<(), SaveRefusal> {
        let source = self.source();
        let container = RawTag::parse(&source[span.0..span.1]);
        if container.is_empty() {
            self.reader.expect()?;
            return Ok(());
        }
        let (children, end_tag) = self.children()?;
        let mut kept = 0usize;
        let mut edits: Vec<((usize, usize), Vec<u8>)> = Vec::new();
        for child in &children {
            let Some(text) = child.element.attribute(attribute) else {
                kept += 1;
                continue;
            };
            let moved = moved_sqref(self.edit, collapsed(text)).filter(|moved| {
                !merged || address::area(moved).is_some_and(|(t, l, b, r)| (t, l) != (b, r))
            });
            match moved {
                Some(moved) => {
                    kept += 1;
                    if moved != collapsed(text) {
                        let tag = RawTag::parse(&source[child.tag.0..child.tag.1]);
                        edits.push((
                            child.tag,
                            tag.rebuilt(&[(attribute, Some(&moved))], tag.is_empty()),
                        ));
                    }
                }
                None => edits.push((child.whole, Vec::new())),
            }
        }
        if kept == 0 {
            self.replace((span.0, end_tag.1), b"");
            return Ok(());
        }
        let removed = edits.iter().any(|(_, bytes)| bytes.is_empty());
        if removed && container.has("count") {
            let count = kept.to_string();
            self.replace(span, &container.rebuilt(&[("count", Some(&count))], false));
        }
        for (range, bytes) in edits {
            self.replace(range, &bytes);
        }
        Ok(())
    }

    /// The text content of the element whose start tag was just read, to its end, and the range
    /// of the content (from the start tag's end to the end tag's start).
    fn content(&mut self, tag: (usize, usize)) -> Result<(String, (usize, usize)), SaveRefusal> {
        let mut text = String::new();
        let source = self.source();
        if RawTag::parse(&source[tag.0..tag.1]).is_empty() {
            self.reader.expect()?;
            return Ok((text, (tag.1, tag.1)));
        }
        loop {
            match self.reader.expect()? {
                (XmlEvent::Text(more), _) => text.push_str(&more),
                (XmlEvent::Start(_), _) => {
                    self.reader.element_end()?;
                }
                (XmlEvent::End(_), (start, _)) => return Ok((text, (tag.1, start))),
            }
        }
    }

    /// Rewrites the formula text of the element (`formula`, `formula1`, `formula2`) whose start
    /// tag was just read, its relative references relative to `at`.
    fn formula(&mut self, tag: (usize, usize), at: CellKey) -> Result<(), SaveRefusal> {
        let (text, range) = self.content(tag)?;
        let units = crate::load::strings::decode(&text);
        let text = String::from_utf16_lossy(&units);
        if let Some(rewritten) = self.rewrite.relative(&text, at) {
            let mut bytes = Vec::new();
            xstring(&rewritten.encode_utf16().collect::<Vec<_>>(), &mut bytes);
            self.replace(range, &bytes);
        }
        Ok(())
    }

    /// Walks the element whose start tag was just read, rewriting every `formula`, `formula1`
    /// and `formula2` element in it, relative to `at`.
    fn formulas_within(&mut self, at: CellKey) -> Result<(), SaveRefusal> {
        loop {
            match self.reader.expect()? {
                (XmlEvent::Start(element), tag) => {
                    let main = element.name.namespace.as_deref() == Some(MAIN_NAMESPACE);
                    if main
                        && matches!(
                            element.name.local.as_str(),
                            "formula" | "formula1" | "formula2"
                        )
                    {
                        self.formula(tag, at)?;
                    } else if RawTag::parse(&self.source()[tag.0..tag.1]).is_empty() {
                        self.reader.expect()?;
                    } else {
                        self.formulas_within(at)?;
                    }
                }
                (XmlEvent::Text(_), _) => {}
                (XmlEvent::End(_), _) => return Ok(()),
            }
        }
    }

    fn conditional_formatting(
        &mut self,
        element: &crate::xml::XmlElement,
        span: (usize, usize),
    ) -> Result<(), SaveRefusal> {
        let at = anchor_of(self.sheet, element.attribute("sqref").map(collapsed));
        if self.on_sheet {
            let sqref = element
                .attribute("sqref")
                .map(collapsed)
                .unwrap_or_default();
            match moved_sqref(self.edit, sqref) {
                None => {
                    let end = self.reader.element_end()?;
                    self.replace((span.0, end), b"");
                    return Ok(());
                }
                Some(moved) if moved != sqref => {
                    self.retag(span, &[("sqref", Some(&moved))]);
                }
                Some(_) => {}
            }
        }
        if RawTag::parse(&self.source()[span.0..span.1]).is_empty() {
            self.reader.expect()?;
            return Ok(());
        }
        self.formulas_within(at)
    }

    /// `dataValidations`: on the edited sheet each validation's `sqref` moved, those deleted left
    /// out (with `count` set, and the container when none remains); on every sheet their
    /// formulas rewritten.
    fn data_validations(&mut self, span: (usize, usize)) -> Result<(), SaveRefusal> {
        let source = self.source();
        let container = RawTag::parse(&source[span.0..span.1]);
        if container.is_empty() {
            self.reader.expect()?;
            return Ok(());
        }
        let mut kept = 0usize;
        let mut removed = false;
        loop {
            match self.reader.expect()? {
                (XmlEvent::Start(element), tag) => {
                    let validation = element.name.is(MAIN_NAMESPACE, "dataValidation");
                    let sqref = element.attribute("sqref").map(collapsed).map(str::to_owned);
                    let at = anchor_of(self.sheet, sqref.as_deref());
                    if validation
                        && self.on_sheet
                        && let Some(sqref) = sqref
                    {
                        match moved_sqref(self.edit, &sqref) {
                            None => {
                                let end = self.reader.element_end()?;
                                self.replace((tag.0, end), b"");
                                removed = true;
                                continue;
                            }
                            Some(moved) if moved != sqref => {
                                self.retag(tag, &[("sqref", Some(&moved))]);
                            }
                            Some(_) => {}
                        }
                    }
                    kept += 1;
                    if RawTag::parse(&source[tag.0..tag.1]).is_empty() {
                        self.reader.expect()?;
                    } else {
                        self.formulas_within(at)?;
                    }
                }
                (XmlEvent::Text(_), _) => {}
                (XmlEvent::End(_), end_tag) => {
                    if kept == 0 {
                        self.replace_whole((span.0, end_tag.1), b"");
                    } else if removed && container.has("count") {
                        let count = kept.to_string();
                        self.replace(span, &container.rebuilt(&[("count", Some(&count))], false));
                    }
                    return Ok(());
                }
            }
        }
    }

    /// `rowBreaks` or `colBreaks`: each break (`brk id`, the position before the break) moved,
    /// those deleted left out, `count` and `manualBreakCount` set.
    fn breaks(&mut self, span: (usize, usize)) -> Result<(), SaveRefusal> {
        let source = self.source();
        let container = RawTag::parse(&source[span.0..span.1]);
        if container.is_empty() {
            self.reader.expect()?;
            return Ok(());
        }
        let (children, end_tag) = self.children()?;
        let mut kept = 0usize;
        let mut manual = 0usize;
        let mut edits = Vec::new();
        for child in &children {
            let id = child
                .element
                .attribute("id")
                .and_then(|id| collapsed(id).parse::<u32>().ok());
            let is_manual = crate::load::manifest::is_true(child.element.attribute("man"));
            let moved = match id {
                None | Some(0) => id,
                Some(id) => moved_position(self.edit, id),
            };
            match moved {
                Some(moved) => {
                    kept += 1;
                    manual += usize::from(is_manual);
                    if Some(moved) != id {
                        let tag = RawTag::parse(&source[child.tag.0..child.tag.1]);
                        let text = moved.to_string();
                        edits.push((
                            child.tag,
                            tag.rebuilt(&[("id", Some(&text))], tag.is_empty()),
                        ));
                    }
                }
                None => edits.push((child.whole, Vec::new())),
            }
        }
        let removed = edits
            .iter()
            .any(|(_, bytes): &((usize, usize), Vec<u8>)| bytes.is_empty());
        if !removed {
            for (range, bytes) in edits {
                self.replace(range, &bytes);
            }
            return Ok(());
        }
        if kept == 0 {
            self.replace((span.0, end_tag.1), b"");
            return Ok(());
        }
        let (count, manual) = (kept.to_string(), manual.to_string());
        self.replace(
            span,
            &container.rebuilt(
                &[("count", Some(&count)), ("manualBreakCount", Some(&manual))],
                false,
            ),
        );
        for (range, bytes) in edits {
            self.replace(range, &bytes);
        }
        Ok(())
    }

    /// The sheet's `autoFilter`: its `ref` moved (the filter left out when all of it is deleted),
    /// a column edit's filter columns (`colId`, from the range's first column) moved and those
    /// deleted left out, and its `sortState` moved.
    fn auto_filter(
        &mut self,
        element: &crate::xml::XmlElement,
        span: (usize, usize),
    ) -> Result<(), SaveRefusal> {
        let area = element
            .attribute("ref")
            .and_then(|text| address::area(collapsed(text)));
        let Some(area) = area else {
            self.reader.element_end()?;
            return Ok(());
        };
        let Some(moved) = moved_area(self.edit, area) else {
            let end = self.reader.element_end()?;
            self.replace((span.0, end), b"");
            return Ok(());
        };
        if moved != area {
            self.retag(span, &[("ref", Some(&area_text(moved)))]);
        }
        if RawTag::parse(&self.source()[span.0..span.1]).is_empty() {
            self.reader.expect()?;
            return Ok(());
        }
        loop {
            match self.reader.expect()? {
                (XmlEvent::Start(child), tag) => {
                    if child.name.is(MAIN_NAMESPACE, "filterColumn")
                        && self.edit.axis() == Axis::Columns
                    {
                        let end = self.reader.element_end()?;
                        let id = child
                            .attribute("colId")
                            .and_then(|id| collapsed(id).parse::<u32>().ok());
                        if let Some(id) = id {
                            match moved_position(self.edit, area.1 + id) {
                                Some(column) if column >= moved.1 => {
                                    let text = (column - moved.1).to_string();
                                    if column - moved.1 != id {
                                        self.retag(tag, &[("colId", Some(&text))]);
                                    }
                                }
                                _ => self.replace((tag.0, end), b""),
                            }
                        }
                    } else if child.name.is(MAIN_NAMESPACE, "sortState") {
                        self.sort_state(&child, tag)?;
                    } else {
                        self.reader.element_end()?;
                    }
                }
                (XmlEvent::Text(_), _) => {}
                (XmlEvent::End(_), _) => return Ok(()),
            }
        }
    }

    /// A `sortState`: its `ref` and each `sortCondition`'s moved, those deleted left out (the
    /// whole sort when its range is).
    fn sort_state(
        &mut self,
        element: &crate::xml::XmlElement,
        span: (usize, usize),
    ) -> Result<(), SaveRefusal> {
        let area = element
            .attribute("ref")
            .and_then(|text| address::area(collapsed(text)));
        let moved = area.map(|area| moved_area(self.edit, area));
        if let Some(None) = moved {
            let end = self.reader.element_end()?;
            self.replace((span.0, end), b"");
            return Ok(());
        }
        if let (Some(area), Some(Some(moved))) = (area, moved)
            && moved != area
        {
            self.retag(span, &[("ref", Some(&area_text(moved)))]);
        }
        if RawTag::parse(&self.source()[span.0..span.1]).is_empty() {
            self.reader.expect()?;
            return Ok(());
        }
        let (children, _) = self.children()?;
        for child in children {
            if child.local.as_deref() != Some("sortCondition") {
                continue;
            }
            let Some(area) = child
                .element
                .attribute("ref")
                .and_then(|text| address::area(collapsed(text)))
            else {
                continue;
            };
            match moved_area(self.edit, area) {
                Some(moved) if moved != area => {
                    self.retag(child.tag, &[("ref", Some(&area_text(moved)))])
                }
                Some(_) => {}
                None => self.replace(child.whole, b""),
            }
        }
        Ok(())
    }

    /// The sheet's views: each view's and pane's `topLeftCell`, and each selection's
    /// `activeCell` and `sqref`, moved; a deleted position lands on the position after the
    /// deleted band, a deleted selection on its active cell.
    fn sheet_views(&mut self) -> Result<(), SaveRefusal> {
        let mut depth = 1u32;
        loop {
            match self.reader.expect()? {
                (XmlEvent::Start(element), tag) => {
                    let empty = RawTag::parse(&self.source()[tag.0..tag.1]).is_empty();
                    let main = element.name.namespace.as_deref() == Some(MAIN_NAMESPACE);
                    let local = element.name.local.as_str();
                    let cell = |name: &str| {
                        element
                            .attribute(name)
                            .and_then(|text| address::cell(collapsed(text)))
                    };
                    let place = |at: (u32, u32)| -> (u32, u32) {
                        match self.edit.axis() {
                            Axis::Rows => (clamped(self.edit, at.0), at.1),
                            Axis::Columns => (at.0, clamped(self.edit, at.1)),
                        }
                    };
                    let mut changes: Vec<(&str, String)> = Vec::new();
                    if main
                        && matches!(local, "sheetView" | "pane")
                        && let Some(at) = cell("topLeftCell")
                        && place(at) != at
                    {
                        let (row, column) = place(at);
                        changes.push(("topLeftCell", a1(row, column)));
                    }
                    if main && local == "selection" {
                        let active = cell("activeCell").map(place);
                        if let Some((row, column)) = active
                            && Some((row, column)) != cell("activeCell")
                        {
                            changes.push(("activeCell", a1(row, column)));
                        }
                        if let Some(sqref) = element.attribute("sqref").map(collapsed) {
                            let moved = moved_sqref(self.edit, sqref)
                                .or_else(|| active.map(|(row, column)| a1(row, column)));
                            if let Some(moved) = moved
                                && moved != sqref
                            {
                                changes.push(("sqref", moved));
                            }
                        }
                    }
                    if !changes.is_empty() {
                        let changes: Vec<(&str, Option<&str>)> = changes
                            .iter()
                            .map(|(name, value)| (*name, Some(value.as_str())))
                            .collect();
                        self.retag(tag, &changes);
                    }
                    if empty {
                        self.reader.expect()?;
                    } else {
                        depth += 1;
                    }
                }
                (XmlEvent::Text(_), _) => {}
                (XmlEvent::End(_), _) => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
            }
        }
    }

    /// The worksheet's extensions (`extLst`): every formula (`xm:f`) rewritten, relative to the
    /// top-left cell of the area (`xm:sqref`) of the item it belongs to, and on the edited sheet
    /// every area moved. An area the edit deletes entirely is refused: the item it belongs to
    /// would have to go, with its container's count.
    fn extensions(&mut self) -> Result<(), SaveRefusal> {
        // For each element open: the formulas of its children (text, range) and its area.
        let mut frames: Vec<ExtensionFrame> = vec![(Vec::new(), None)];
        loop {
            match self.reader.expect()? {
                (XmlEvent::Start(element), tag) => {
                    let ours = element.name.namespace.as_deref() == Some(EXTENSION_MAIN);
                    match element.name.local.as_str() {
                        "f" if ours => {
                            let (text, range) = self.content(tag)?;
                            if let Some(frame) = frames.last_mut() {
                                frame.0.push((text, range));
                            }
                        }
                        "sqref" if ours => {
                            let (text, range) = self.content(tag)?;
                            let sqref = collapsed(&text).to_owned();
                            if self.on_sheet {
                                match moved_sqref(self.edit, &sqref) {
                                    Some(moved) if moved != sqref => {
                                        self.replace(range, moved.as_bytes())
                                    }
                                    Some(_) => {}
                                    None => {
                                        return Err(not_moved(
                                            self.part,
                                            "an extension's item whose cells the edit deletes",
                                        ));
                                    }
                                }
                            }
                            if let Some(frame) = frames.last_mut() {
                                frame.1 = Some(sqref);
                            }
                        }
                        _ => {
                            if RawTag::parse(&self.source()[tag.0..tag.1]).is_empty() {
                                self.reader.expect()?;
                            } else {
                                frames.push((Vec::new(), None));
                            }
                        }
                    }
                }
                (XmlEvent::Text(_), _) => {}
                (XmlEvent::End(_), _) => {
                    let Some((formulas, sqref)) = frames.pop() else {
                        return Ok(());
                    };
                    let at = anchor_of(self.sheet, sqref.as_deref());
                    for (text, range) in formulas {
                        let units = crate::load::strings::decode(&text);
                        let text = String::from_utf16_lossy(&units);
                        if let Some(rewritten) = self.rewrite.relative(&text, at) {
                            let mut bytes = Vec::new();
                            xstring(&rewritten.encode_utf16().collect::<Vec<_>>(), &mut bytes);
                            self.replace(range, &bytes);
                        }
                    }
                    if frames.is_empty() {
                        return Ok(());
                    }
                }
            }
        }
    }

    /// The column spans (`cols`) under a column edit; a row edit leaves them.
    fn cols(&mut self, span: (usize, usize)) -> Result<(), SaveRefusal> {
        if self.edit.axis() != Axis::Columns {
            self.reader.element_end()?;
            return Ok(());
        }
        let source = self.source();
        if RawTag::parse(&source[span.0..span.1]).is_empty() {
            self.reader.expect()?;
            return Ok(());
        }
        let (children, end_tag) = self.children()?;
        let (first, last, count) = (self.edit.first(), self.edit.last(), self.edit.count());
        // Each new span: (first, last, the source tag's range, whether `hidden` is dropped).
        let mut spans: Vec<(u32, u32, (usize, usize), bool)> = Vec::new();
        let mut before_band: Option<(usize, usize)> = None;
        for child in &children {
            let bound = |name: &str| {
                child
                    .element
                    .attribute(name)
                    .and_then(|text| collapsed(text).parse::<u32>().ok())
            };
            let (Some(min), Some(max)) = (bound("min"), bound("max")) else {
                return Err(self.reader.invalid("a col without min and max"));
            };
            let tag = child.tag;
            if (min..=max).contains(&(first.saturating_sub(1))) && first > 1 {
                before_band = Some(tag);
            }
            match self.edit.change() {
                AxisChange::Insert => {
                    if max < first {
                        spans.push((min, max, tag, false));
                    } else if min >= first {
                        if min + count <= MAX_COLUMNS {
                            spans.push((min + count, (max + count).min(MAX_COLUMNS), tag, false));
                        }
                    } else {
                        spans.push((min, first - 1, tag, false));
                        if last < MAX_COLUMNS {
                            spans.push((last + 1, (max + count).min(MAX_COLUMNS), tag, false));
                        }
                    }
                }
                AxisChange::Delete => {
                    let before = (min < first).then(|| (min, max.min(first - 1)));
                    let after = (max > last).then(|| (min.max(last + 1) - count, max - count));
                    match (before, after) {
                        (Some((a, _)), Some((_, d))) => spans.push((a, d, tag, false)),
                        (Some((a, b)), None) | (None, Some((a, b))) => {
                            spans.push((a, b, tag, false))
                        }
                        (None, None) => {}
                    }
                }
            }
        }
        if self.edit.change() == AxisChange::Insert
            && let Some(tag) = before_band
        {
            spans.push((first, last.min(MAX_COLUMNS), tag, true));
        }
        spans.sort_by_key(|span| span.0);
        let mut bytes = Vec::new();
        for (min, max, tag, band) in &spans {
            let raw = RawTag::parse(&source[tag.0..tag.1]);
            let (min, max) = (min.to_string(), max.to_string());
            let mut changes = vec![("min", Some(min.as_str())), ("max", Some(max.as_str()))];
            if *band {
                changes.push(("hidden", None));
                changes.push(("collapsed", None));
            }
            bytes.extend(raw.rebuilt(&changes, true));
        }
        if spans.is_empty() {
            // CT_Cols holds at least one col: the element goes.
            self.replace((span.0, end_tag.1), b"");
        } else {
            let first_child = children.first().map_or(span.1, |child| child.whole.0);
            let last_child = children.last().map_or(span.1, |child| child.whole.1);
            self.replace((first_child, last_child), &bytes);
        }
        Ok(())
    }

    /// The rows and cells, moved; see the module's documentation.
    fn sheet_data(&mut self, span: (usize, usize)) -> Result<(), SaveRefusal> {
        if RawTag::parse(&self.source()[span.0..span.1]).is_empty() {
            self.reader.expect()?;
            return Ok(());
        }
        let edit = self.edit;
        let rows_insert = edit.axis() == Axis::Rows && edit.change() == AxisChange::Insert;
        // The row above an inserted band of rows, as read: its start tag's range and its styled
        // cells (column, style).
        let mut above: Option<AboveRow> = None;
        let mut band_written = !rows_insert || edit.first() == 1;
        let mut previous = 0u32;
        loop {
            match self.reader.expect()? {
                (XmlEvent::Start(element), row_tag) if element.name.is(MAIN_NAMESPACE, "row") => {
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
                    if !band_written && number >= edit.first() {
                        band_written = true;
                        let band = self.row_band(above.as_ref());
                        if !band.is_empty() {
                            self.replace((row_tag.0, row_tag.0), &band);
                        }
                    }
                    self.row(&element, row_tag, number, rows_insert, &mut above)?;
                }
                (XmlEvent::Start(_), _) => {
                    return Err(self
                        .reader
                        .invalid("an element other than row in sheetData"));
                }
                (XmlEvent::Text(_), _) => {}
                (XmlEvent::End(_), (start, _)) => {
                    if !band_written {
                        let band = self.row_band(above.as_ref());
                        if !band.is_empty() {
                            self.replace((start, start), &band);
                        }
                    }
                    return Ok(());
                }
            }
        }
    }

    /// The rows an insertion adds, formatted as the row above (`above`): nothing when that row
    /// states no format.
    fn row_band(&self, above: Option<&AboveRow>) -> Vec<u8> {
        let mut out = Vec::new();
        let Some((tag, styled)) = above else {
            return out;
        };
        let source = self.source();
        let raw = RawTag::parse(&source[tag.0..tag.1]);
        let prefix = raw.prefix();
        for row in self.edit.first()..=self.edit.last().min(MAX_ROWS) {
            let number = row.to_string();
            out.extend(raw.rebuilt(
                &[
                    ("r", Some(&number)),
                    ("spans", None),
                    ("hidden", None),
                    ("collapsed", None),
                ],
                styled.is_empty(),
            ));
            if styled.is_empty() {
                continue;
            }
            for (column, style) in styled {
                out.push(b'<');
                out.extend_from_slice(prefix);
                out.extend_from_slice(
                    format!("c r=\"{}\" s=\"{style}\"/>", a1(row, *column)).as_bytes(),
                );
            }
            out.extend_from_slice(b"</");
            out.extend_from_slice(prefix);
            out.extend_from_slice(b"row>");
        }
        out
    }

    /// One row: moved or deleted, its cells moved.
    fn row(
        &mut self,
        element: &crate::xml::XmlElement,
        tag: (usize, usize),
        number: u32,
        rows_insert: bool,
        above: &mut Option<AboveRow>,
    ) -> Result<(), SaveRefusal> {
        let edit = self.edit;
        let source = self.source();
        let raw = RawTag::parse(&source[tag.0..tag.1]);
        let empty = raw.is_empty();
        let new_number = match edit.axis() {
            Axis::Rows => moved_position(edit, number),
            Axis::Columns => Some(number),
        };
        let keep_above = rows_insert && number + 1 == edit.first();
        let Some(new_number) = new_number else {
            // A deleted row goes with its cells.
            let end = if empty {
                self.reader.expect()?;
                tag.1
            } else {
                self.count_strings_to_end()?
            };
            self.replace((tag.0, end), b"");
            return Ok(());
        };
        let row_text = new_number.to_string();
        if new_number != number
            || element.attribute("spans").is_some()
            || element.attribute("r").is_none()
        {
            self.replace(
                tag,
                &raw.rebuilt(&[("r", Some(&row_text)), ("spans", None)], empty),
            );
        }
        if empty {
            self.reader.expect()?;
            if keep_above {
                *above = Some((tag, Vec::new()));
            }
            return Ok(());
        }
        let prefix = raw.prefix().to_vec();
        let columns_insert = edit.axis() == Axis::Columns && edit.change() == AxisChange::Insert;
        // The style of the cell left of an inserted band of columns, when it has one.
        let mut left_style: Option<String> = None;
        let mut band_written = !columns_insert || edit.first() == 1;
        let mut styled = Vec::new();
        let mut previous = 0u32;
        loop {
            match self.reader.expect()? {
                (XmlEvent::Start(cell), cell_tag) if cell.name.is(MAIN_NAMESPACE, "c") => {
                    let column = match cell.attribute("r") {
                        Some(text) => match address::cell(collapsed(text)) {
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
                    let style = cell.attribute("s").map(collapsed).map(str::to_owned);
                    if keep_above && let Some(style) = &style {
                        styled.push((column, style.clone()));
                    }
                    if !band_written && column >= edit.first() {
                        band_written = true;
                        let band = self.column_band(&prefix, new_number, left_style.as_deref());
                        if !band.is_empty() {
                            self.replace((cell_tag.0, cell_tag.0), &band);
                        }
                    }
                    if columns_insert && column + 1 == edit.first() {
                        left_style = style;
                    }
                    self.cell(&cell, cell_tag, new_number, column)?;
                }
                (XmlEvent::Start(_), (start, _)) => {
                    if !band_written {
                        band_written = true;
                        let band = self.column_band(&prefix, new_number, left_style.as_deref());
                        if !band.is_empty() {
                            self.replace((start, start), &band);
                        }
                    }
                    self.reader.element_end()?;
                }
                (XmlEvent::Text(_), _) => {}
                (XmlEvent::End(_), (start, _)) => {
                    if !band_written {
                        let band = self.column_band(&prefix, new_number, left_style.as_deref());
                        if !band.is_empty() {
                            self.replace((start, start), &band);
                        }
                    }
                    if keep_above {
                        *above = Some((tag, styled));
                    }
                    return Ok(());
                }
            }
        }
    }

    /// The cells an insertion of columns adds to the row `row`, styled as the cell to their left
    /// (`style`): nothing when it has no style.
    fn column_band(&self, prefix: &[u8], row: u32, style: Option<&str>) -> Vec<u8> {
        let mut out = Vec::new();
        let Some(style) = style else {
            return out;
        };
        for column in self.edit.first()..=self.edit.last().min(MAX_COLUMNS) {
            out.push(b'<');
            out.extend_from_slice(prefix);
            out.extend_from_slice(
                format!("c r=\"{}\" s=\"{style}\"/>", a1(row, column)).as_bytes(),
            );
        }
        out
    }

    /// Reads to the end of the row whose start tag was just read, counting its cells that hold a
    /// shared string; returns the offset after the row.
    fn count_strings_to_end(&mut self) -> Result<usize, SaveRefusal> {
        loop {
            match self.reader.expect()? {
                (XmlEvent::Start(cell), _) => {
                    if cell.name.is(MAIN_NAMESPACE, "c") && cell.attribute("t") == Some("s") {
                        self.out.strings_removed += 1;
                    }
                    self.reader.element_end()?;
                }
                (XmlEvent::Text(_), _) => {}
                (XmlEvent::End(_), (_, end)) => return Ok(end),
            }
        }
    }

    /// One cell of a kept row (now `row`), at `column` before the edit: moved or deleted, its
    /// formula's areas moved.
    fn cell(
        &mut self,
        element: &crate::xml::XmlElement,
        tag: (usize, usize),
        row: u32,
        column: u32,
    ) -> Result<(), SaveRefusal> {
        let edit = self.edit;
        let new_column = match edit.axis() {
            Axis::Columns => moved_position(edit, column),
            Axis::Rows => Some(column),
        };
        let empty = RawTag::parse(&self.source()[tag.0..tag.1]).is_empty();
        let Some(new_column) = new_column else {
            let end = if empty {
                self.reader.expect()?;
                tag.1
            } else {
                self.reader.element_end()?
            };
            if element.attribute("t") == Some("s") {
                self.out.strings_removed += 1;
            }
            self.replace((tag.0, end), b"");
            return Ok(());
        };
        let at = a1(row, new_column);
        if element.attribute("r").map(collapsed) != Some(at.as_str()) {
            self.retag(tag, &[("r", Some(&at))]);
        }
        if empty {
            self.reader.expect()?;
            return Ok(());
        }
        loop {
            match self.reader.expect()? {
                (XmlEvent::Start(child), child_tag) => {
                    if child.name.is(MAIN_NAMESPACE, "f") {
                        self.formula_areas(&child, child_tag)?;
                    }
                    self.reader.element_end()?;
                }
                (XmlEvent::Text(_), _) => {}
                (XmlEvent::End(_), _) => return Ok(()),
            }
        }
    }

    /// A cell's `<f>`: a shared group's, an array's or a data table's `ref`, and a data table's
    /// input cells (`r1`, `r2`), moved.
    fn formula_areas(
        &mut self,
        formula: &crate::xml::XmlElement,
        tag: (usize, usize),
    ) -> Result<(), SaveRefusal> {
        let kind = formula.attribute("t");
        let area = formula
            .attribute("ref")
            .and_then(|text| address::area(collapsed(text)));
        let mut changes: Vec<(&str, String)> = Vec::new();
        if let Some(area) = area {
            let moved = moved_area(self.edit, area);
            if kind == Some("shared") {
                let size = |(t, l, b, r): Area| (b - t, r - l);
                let cut = moved.is_none_or(|moved| size(moved) != size(area));
                if cut
                    && let Some(group) = formula
                        .attribute("si")
                        .and_then(|si| collapsed(si).parse::<u32>().ok())
                {
                    self.out.unshared.insert(group);
                }
            }
            if let Some(moved) = moved
                && moved != area
            {
                changes.push(("ref", area_text(moved)));
            }
        }
        if kind == Some("dataTable") {
            for input in ["r1", "r2"] {
                let Some(at) = formula
                    .attribute(input)
                    .and_then(|text| address::cell(collapsed(text)))
                else {
                    continue;
                };
                match moved_cell(self.edit, at) {
                    Some(moved) if moved != at => changes.push((input, a1(moved.0, moved.1))),
                    Some(_) => {}
                    None => {
                        return Err(not_moved(
                            self.part,
                            "a data table whose input cell the edit deletes",
                        ));
                    }
                }
            }
        }
        if !changes.is_empty() {
            let changes: Vec<(&str, Option<&str>)> = changes
                .iter()
                .map(|(name, value)| (*name, Some(value.as_str())))
                .collect();
            self.retag(tag, &changes);
        }
        Ok(())
    }
}

impl Pass<'_, '_> {
    /// A table part of the edited sheet: its `ref`, its filter's and its sort's moved.
    fn table(&mut self) -> Result<(), SaveRefusal> {
        let (root, span) = self.reader.root("table")?;
        let area = root
            .attribute("ref")
            .and_then(|text| address::area(collapsed(text)))
            .ok_or_else(|| self.reader.invalid("a table without a ref that is an area"))?;
        let moved = moved_area(self.edit, area)
            .ok_or_else(|| not_moved(self.part, "a table the edit deletes"))?;
        if moved != area {
            self.retag(span, &[("ref", Some(&area_text(moved)))]);
        }
        loop {
            match self.reader.expect()? {
                (XmlEvent::Start(element), tag)
                    if element.name.is(MAIN_NAMESPACE, "autoFilter") =>
                {
                    self.auto_filter(&element, tag)?
                }
                (XmlEvent::Start(element), tag) if element.name.is(MAIN_NAMESPACE, "sortState") => {
                    self.sort_state(&element, tag)?
                }
                (XmlEvent::Start(_), _) => {
                    self.reader.element_end()?;
                }
                (XmlEvent::Text(_), _) => {}
                (XmlEvent::End(_), _) => break,
            }
        }
        self.reader.end()
    }

    /// A part whose elements `local` in `namespace` each name a cell of the edited sheet in
    /// `ref` (comments, threaded comments): each moved, those whose cell is deleted left out.
    fn references(&mut self, namespace: &str, local: &str) -> Result<(), SaveRefusal> {
        loop {
            let Some((event, tag)) = self.reader.next()? else {
                return Ok(());
            };
            let XmlEvent::Start(element) = event else {
                continue;
            };
            if !element.name.is(namespace, local) {
                continue;
            }
            let area = element
                .attribute("ref")
                .and_then(|text| address::area(collapsed(text)));
            let Some(area) = area else {
                continue;
            };
            match moved_area(self.edit, area) {
                // The element's content is read on by the loop.
                Some(moved) if moved != area => {
                    self.retag(tag, &[("ref", Some(&area_text(moved)))]);
                }
                Some(_) => {}
                None => {
                    let end = if RawTag::parse(&self.source()[tag.0..tag.1]).is_empty() {
                        self.reader.expect()?;
                        tag.1
                    } else {
                        self.reader.element_end()?
                    };
                    self.replace((tag.0, end), b"");
                }
            }
        }
    }

    /// A drawing part of the edited sheet: each anchor's corners (`from`, `to`: 0-based `col` and
    /// `row`) moved as Excel moves an object over inserted or deleted cells: a two-cell anchor's
    /// corners each to where their cell goes (a deleted one to the position after the deleted
    /// band), a one-cell anchor's top-left corner so, with its size kept, an absolute one not at
    /// all.
    fn drawing(&mut self) -> Result<(), SaveRefusal> {
        let mut depth = 0u32;
        // The anchor being read: its kind, and its corners as (top-left, axis, position, the
        // range of the position's text).
        let mut anchor: Option<(String, Vec<Corner>)> = None;
        let mut corner: Option<bool> = None;
        loop {
            let Some((event, span)) = self.reader.next()? else {
                return Ok(());
            };
            match event {
                XmlEvent::Start(element) => {
                    depth += 1;
                    let ours = element.name.namespace.as_deref() == Some(SPREADSHEET_DRAWING);
                    let local = element.name.local.as_str();
                    if ours && depth == 2 {
                        let kind = match local {
                            "twoCellAnchor" => element.attribute("editAs").unwrap_or("twoCell"),
                            other => other,
                        };
                        anchor = Some((kind.to_owned(), Vec::new()));
                    } else if ours && depth == 3 && matches!(local, "from" | "to") {
                        corner = Some(local == "from");
                    } else if ours
                        && depth == 4
                        && matches!(local, "col" | "row")
                        && let Some(from) = corner
                    {
                        let axis = if local == "row" {
                            Axis::Rows
                        } else {
                            Axis::Columns
                        };
                        let (text, range) = self.content(span)?;
                        depth -= 1;
                        let position = collapsed(&text).parse::<u32>().map_err(|_| {
                            self.reader
                                .invalid("an anchor position that is not a number")
                        })?;
                        if let Some((_, corners)) = anchor.as_mut() {
                            corners.push(Corner {
                                from,
                                axis,
                                position,
                                range,
                            });
                        }
                    }
                }
                XmlEvent::End(_) => {
                    depth -= 1;
                    if depth == 2 {
                        corner = None;
                    }
                    if depth == 1
                        && let Some((kind, corners)) = anchor.take()
                    {
                        self.anchor(&kind, &corners);
                    }
                }
                XmlEvent::Text(_) => {}
            }
        }
    }

    fn anchor(&mut self, kind: &str, corners: &[Corner]) {
        if matches!(kind, "absolute" | "absoluteAnchor") {
            return;
        }
        let axis = self.edit.axis();
        let Some(from) = corners
            .iter()
            .find(|corner| corner.from && corner.axis == axis)
            .map(|corner| corner.position)
        else {
            return;
        };
        let new_from = clamped(self.edit, from + 1) - 1;
        for corner in corners.iter().filter(|corner| corner.axis == axis) {
            let moved = if corner.from {
                new_from
            } else if kind == "oneCell" {
                new_from + corner.position.saturating_sub(from)
            } else {
                clamped(self.edit, corner.position + 1) - 1
            };
            if moved != corner.position {
                self.replace(corner.range, moved.to_string().as_bytes());
            }
        }
    }

    /// Every element `local` in `namespace` whose text is a formula (a chart's `c:f`): rewritten
    /// as a workbook-scoped name's formula.
    fn texts(&mut self, namespace: &str, local: &str) -> Result<(), SaveRefusal> {
        loop {
            let Some((event, tag)) = self.reader.next()? else {
                return Ok(());
            };
            let XmlEvent::Start(element) = event else {
                continue;
            };
            if !element.name.is(namespace, local) {
                continue;
            }
            let (text, range) = self.content(tag)?;
            if let Some(rewritten) = self.rewrite.text(&text, NameScope::Workbook) {
                let mut bytes = Vec::new();
                xstring(&rewritten.encode_utf16().collect::<Vec<_>>(), &mut bytes);
                self.replace(range, &bytes);
            }
        }
    }
}

/// One corner coordinate of a drawing's anchor.
struct Corner {
    /// Whether it is of the top-left corner (`from`).
    from: bool,
    axis: Axis,
    /// The 0-based position.
    position: u32,
    /// The range of the position's text.
    range: (usize, usize),
}

const CHART: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";

/// What a pass over a part other than a worksheet does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PartPass {
    Table,
    Comments,
    ThreadedComments,
    Drawing,
    Vml,
}

/// One pass of `edit` over the part `xml`; `None` when it changes nothing.
fn part_pass(
    xml: &XmlPart,
    edit: AxisEdit,
    rewrite: &Rewrite<'_>,
    kind: PartPass,
) -> Result<Option<Vec<u8>>, SaveRefusal> {
    if kind == PartPass::Vml {
        return Ok(vml(xml.bytes(), edit));
    }
    let mut pass = Pass::new(xml, edit.sheet(), edit, rewrite);
    match kind {
        PartPass::Table => pass.table()?,
        PartPass::Comments => pass.references(MAIN_NAMESPACE, "comment")?,
        PartPass::ThreadedComments => pass.references(THREADED_COMMENTS, "threadedComment")?,
        PartPass::Drawing => pass.drawing()?,
        PartPass::Vml => unreachable!("passed above"),
    }
    Ok(applied(xml.bytes(), pass.edits))
}

/// Move newly authored frames through the same anchor pass as imported drawing parts.
pub(super) fn drawing_pass(xml:&XmlPart,edit:AxisEdit,rewrite:&Rewrite<'_>)->Result<Option<Vec<u8>>,SaveRefusal>{
    part_pass(xml,edit,rewrite,PartPass::Drawing)
}

/// A chart part's series formulas (`c:f`) rewritten for `edit`; `None` when none changes.
pub(super) fn chart_pass(
    xml: &XmlPart,
    edit: AxisEdit,
    rewrite: &Rewrite<'_>,
) -> Result<Option<Vec<u8>>, SaveRefusal> {
    let mut pass = Pass::new(xml, edit.sheet(), edit, rewrite);
    pass.texts(CHART, "f")?;
    Ok(applied(xml.bytes(), pass.edits))
}

fn find(bytes: &[u8], at: usize, needle: &[u8]) -> Option<usize> {
    bytes
        .get(at..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| at + offset)
}

/// The range of the text between the first `from` in `bytes` and the first `to` after it.
fn between(bytes: &[u8], from: &[u8], to: &[u8]) -> Option<(usize, usize)> {
    let start = find(bytes, 0, from)? + from.len();
    let end = find(bytes, start, to)?;
    Some((start, end))
}

/// A VML drawing of the edited sheet (the shapes of its notes): each note's cell (`x:Row`,
/// `x:Column`, 0-based) and its box's anchor (`x:Anchor`: column, offset, row, offset of the
/// top-left and then the bottom-right corner) moved, and the shape of a note whose cell is
/// deleted left out. VML is read as text, since Excel's is not always well-formed XML: only a
/// shape whose client data is a note's is changed.
fn vml(bytes: &[u8], edit: AxisEdit) -> Option<Vec<u8>> {
    let mut edits: Vec<((usize, usize), Vec<u8>)> = Vec::new();
    let mut at = 0;
    while let Some(start) = find(bytes, at, b"<v:shape ") {
        let Some(end) = find(bytes, start, b"</v:shape>").map(|end| end + b"</v:shape>".len())
        else {
            break;
        };
        at = end;
        let shape = &bytes[start..end];
        if find(shape, 0, b"ObjectType=\"Note\"").is_none() {
            continue;
        }
        let number = |from: &[u8], to: &[u8]| -> Option<(u32, (usize, usize))> {
            let range = between(shape, from, to)?;
            let text = std::str::from_utf8(&shape[range.0..range.1]).ok()?;
            Some((
                text.trim().parse().ok()?,
                (start + range.0, start + range.1),
            ))
        };
        let (Some((row, row_range)), Some((column, column_range))) = (
            number(b"<x:Row>", b"</x:Row>"),
            number(b"<x:Column>", b"</x:Column>"),
        ) else {
            continue;
        };
        let Some((new_row, new_column)) = moved_cell(edit, (row + 1, column + 1)) else {
            edits.push(((start, end), Vec::new()));
            continue;
        };
        if new_row - 1 != row {
            edits.push((row_range, (new_row - 1).to_string().into_bytes()));
        }
        if new_column - 1 != column {
            edits.push((column_range, (new_column - 1).to_string().into_bytes()));
        }
        if let Some(range) = between(shape, b"<x:Anchor>", b"</x:Anchor>")
            && let Ok(text) = std::str::from_utf8(&shape[range.0..range.1])
        {
            let values: Option<Vec<u32>> = text
                .split(',')
                .map(|value| value.trim().parse().ok())
                .collect();
            if let Some(values) = values.filter(|values| values.len() == 8) {
                let mut moved = values.clone();
                let indexes = match edit.axis() {
                    Axis::Rows => [2, 6],
                    Axis::Columns => [0, 4],
                };
                for index in indexes {
                    moved[index] = clamped(edit, values[index] + 1) - 1;
                }
                if moved != values {
                    let text = moved
                        .iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(", ");
                    edits.push(((start + range.0, start + range.1), text.into_bytes()));
                }
            }
        }
    }
    applied(bytes, edits)
}

/// The cells of `sheet` whose input in `model` is not the input the loaded cell it moved from
/// held in `base`, by their position in `model`: the changes of a sheet whose rows or columns
/// were inserted or deleted, against its part after the edits' passes. A cell the edits inserted
/// is a change when the model holds an input in it; a loaded cell that moved and that the model
/// no longer holds is emptied.
pub(super) fn sheet_changes(
    base: &ModelRoot,
    model: &ModelRoot,
    sheet: SheetId,
    moves: &SheetMoves,
) -> super::diff::SheetChanges {
    let mut changes = super::diff::SheetChanges::new();
    let now: BTreeMap<(u32, u32), &calc_doc_model::CellInput> = model
        .sheet_cells(sheet)
        .into_iter()
        .flatten()
        .map(|(at, input)| ((at.row(), at.column()), input))
        .collect();
    for (&(row, column), &input) in &now {
        let was = moves
            .back((row, column))
            .and_then(|(row, column)| CellKey::new(sheet, row, column).ok())
            .and_then(|at| base.cell(at));
        if was != Some(input) {
            changes.insert((row, column), Some(input.clone()));
        }
    }
    for (at, _) in base.sheet_cells(sheet).into_iter().flatten() {
        if let Some(position) = moves.forward((at.row(), at.column()))
            && !now.contains_key(&position)
        {
            changes.insert(position, None);
        }
    }
    changes
}

/// What the edits' passes made of the package, before the authored rewrite.
#[derive(Default)]
pub(super) struct Moved {
    /// Each sheet's worksheet part after the passes, by the sheet's position among the loaded
    /// sheets; `None` when the passes left it as it is.
    pub(super) sheets: Vec<Option<XmlPart>>,
    /// Where each sheet's positions went; `None` for a sheet no edit is on.
    pub(super) moves: Vec<Option<SheetMoves>>,
    /// The shared formula groups of each sheet written as individual formulas.
    pub(super) unshared: Vec<BTreeSet<u32>>,
    /// The cells holding a shared string that the edits deleted.
    pub(super) strings_removed: u64,
    /// The other parts the passes changed, with their new content.
    pub(super) parts: BTreeMap<String, Vec<u8>>,
}

/// Runs the edits' passes over the package (see the module's documentation), each edit's against
/// the parts the edits before it left; `base` is the loaded model, against which every formula is
/// rewritten (an axis edit changes no sheet name, defined name or table name, which is what the
/// rewriter looks up).
pub(super) fn moved(
    package: &Package,
    sheets: &[LoadedSheet],
    base: &ModelRoot,
    structural: &StructuralEdits,
) -> Result<Moved, SaveRefusal> {
    let edits = structural.edits();
    let rewrite_for = |edit: AxisEdit| Rewrite {
        root: base,
        rewriter: structural.rewriter.as_ref(),
        change: StructuralChange::Axis(edit),
    };
    let mut out = Moved {
        sheets: Vec::with_capacity(sheets.len()),
        moves: sheets
            .iter()
            .map(|sheet| SheetMoves::of(edits, sheet.id()))
            .collect(),
        unshared: vec![BTreeSet::new(); sheets.len()],
        ..Moved::default()
    };
    // The parts of each edited sheet that the passes move: (part, pass, sheet).
    let mut related: Vec<(String, PartPass, SheetId)> = Vec::new();
    for (sheet, moves) in sheets.iter().zip(&out.moves) {
        if moves.is_none() {
            continue;
        }
        let part = sheet.part().name();
        let Some(relationships) = package.relationships_of(Some(part)) else {
            continue;
        };
        for relationship in relationships.items() {
            if relationship.mode == TargetMode::External {
                continue;
            }
            let kind = relationship
                .kind
                .strip_prefix(RELATIONSHIPS)
                .unwrap_or(&relationship.kind);
            let pass = match kind {
                "comments" => PartPass::Comments,
                "drawing" => PartPass::Drawing,
                "table" => PartPass::Table,
                "vmlDrawing" => PartPass::Vml,
                _ if relationship.kind == THREADED_COMMENT_KIND => PartPass::ThreadedComments,
                _ if HELD_KINDS.contains(&kind) => continue,
                _ => {
                    return Err(not_moved(
                        part,
                        format!("a relationship of type {}", relationship.kind),
                    ));
                }
            };
            if let Some(target) = &relationship.resolved {
                related.push((target.clone(), pass, sheet.id()));
            }
        }
    }
    // A pivot cache over an edited sheet would keep its source range where it was.
    for part in package.parts() {
        if part.content_type() != Some(PIVOT_CACHE_CONTENT_TYPE) {
            continue;
        }
        let bytes = package
            .read_part(part.name())
            .map_err(|error| SaveRefusal::Part {
                part: part.name().to_owned(),
                error,
            })?;
        for (sheet, moves) in base.sheets().iter().zip(&out.moves) {
            if moves.is_none() {
                continue;
            }
            let mut needle = b"sheet=".to_vec();
            super::text::attribute_value(&sheet.name().text().to_string_lossy(), &mut needle);
            if find(&bytes, 0, &needle).is_some() {
                return Err(not_moved(
                    part.name(),
                    "a pivot cache whose source is an edited sheet",
                ));
            }
        }
    }
    for (index, sheet) in sheets.iter().enumerate() {
        let mut current = read_part(package, sheet.part().name())?;
        let mut changed = false;
        let formulas = find(current.bytes(), 0, b"conditionalFormatting").is_some()
            || find(current.bytes(), 0, b"dataValidation").is_some()
            || find(current.bytes(), 0, b"extLst").is_some();
        for &edit in edits {
            if edit.sheet() != sheet.id() && !formulas {
                continue;
            }
            let pass = worksheet(&current, sheet.id(), edit, &rewrite_for(edit))?;
            out.strings_removed += pass.strings_removed;
            out.unshared[index].extend(pass.unshared);
            if let Some(bytes) = pass.bytes {
                current = current.with_bytes(bytes);
                changed = true;
            }
        }
        out.sheets.push(changed.then_some(current));
    }
    for (part, pass, sheet) in related {
        let mut current = read_part(package, &part)?;
        let mut changed = false;
        for &edit in edits.iter().filter(|edit| edit.sheet() == sheet) {
            if let Some(next) = part_pass(&current, edit, &rewrite_for(edit), pass)? {
                current = current.with_bytes(next);
                changed = true;
            }
        }
        if changed {
            out.parts.insert(part, current.bytes().to_vec());
        }
    }
    for chart in package
        .parts()
        .iter()
        .filter(|part| part.content_type() == Some(CHART_CONTENT_TYPE))
    {
        let mut current = read_part(package, chart.name())?;
        let mut changed = false;
        for &edit in edits {
            if let Some(next) = chart_pass(&current, edit, &rewrite_for(edit))? {
                current = current.with_bytes(next);
                changed = true;
            }
        }
        if changed {
            out.parts
                .insert(chart.name().to_owned(), current.bytes().to_vec());
        }
    }
    Ok(out)
}
