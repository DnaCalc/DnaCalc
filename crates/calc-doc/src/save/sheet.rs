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
//! - A changed or emptied cell loses its cell and value metadata (`cm`, `vm`), which described
//!   the formula or value it no longer holds.
//! - The cells an array formula fills beside its anchor are emptied (keeping only their style,
//!   or removed when they had none): what they hold is the cached result of the array, which a
//!   save after an edit no longer trusts, and the cells of an array whose anchor the edit changed
//!   no longer belong to any formula. Each that held a cached value is counted as a cache
//!   dropped. A data table's cells are kept as the part wrote them.
//! - A save with values writes an array formula's result as Excel does: the anchor's `ref` is the
//!   area the caller gives for it (its spill range, or the anchor alone), and each cell of that
//!   area beside the anchor that the caller gives a value is written with that value alone
//!   (`<c r=".."><v>..</v></c>`, keeping the cell's style), a new cell where the part held none.
//! - A newer error (#SPILL!, #CALC! and the others) is written as Excel writes it: `t="e"`, the
//!   `#VALUE!` fallback as `<v>`, and the `vm` of its value metadata record.
//! - Every formula cell loses its `<v>`, its `t` and its `vm`, unless the caller gives it a value:
//!   once any cell of the workbook changed, no cached result can be trusted without a dependency
//!   graph. Excel calculates an ordinary formula without a value when it opens the package; for
//!   an array formula without one the workbook part is written with `fullCalcOnLoad`
//!   (`author.rs`). Each formula the edit did not change whose loaded record holds a cached
//!   result, and that the caller gives no value, is counted as a cache dropped.
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
use super::file_form::FileForm;
use super::reader::Reader;
use super::strings::SharedStrings;
use super::structural::SheetMoves;
use super::text::{RawTag, Splice, a1, number, xstring};
use crate::load::address;
use crate::load::sheet::{ERRORS, collapsed};
use crate::load::{LoadedSheet, MAIN_NAMESPACE};
use crate::xml::{XmlElement, XmlEvent, XmlPart};
use calc_doc_model::value::CellKey;
use calc_doc_model::{ArrayKind, CachedValue, CellInput, ModelRoot};
use std::collections::{BTreeMap, BTreeSet};

/// What one sheet's rewrite reads beside its part.
pub(super) struct Job<'a> {
    pub(super) changes: &'a SheetChanges,
    /// The areas, as (top, left, bottom, right), whose cells other than the top-left one the
    /// rewrite empties unless they are among the changes: the areas the array formulas of the
    /// loaded revision fill.
    pub(super) cleared: &'a [(u32, u32, u32, u32)],
    /// The file form of each formula among the changes.
    pub(super) forms: &'a BTreeMap<(u32, u32), FileForm>,
    /// The `cm` a dynamic-array formula among the changes names; set when one is.
    pub(super) dynamic_cm: Option<u32>,
    /// The caller's values for the sheet's formula cells, which are written as their cached
    /// values; `None` for a save without values, which leaves every cached value out.
    pub(super) values: Option<&'a BTreeMap<(u32, u32), CachedValue>>,
    pub(super) dirty_caches: &'a BTreeSet<(u32, u32)>,
    pub(super) cache_state: bool,
    /// The area each array formula's result fills, by anchor, as (top, left, bottom, right): its
    /// `ref`, and the cells beside the anchor whose values are written.
    pub(super) spills: &'a BTreeMap<(u32, u32), (u32, u32, u32, u32)>,
    /// The `vm` of each newer error's value metadata record.
    pub(super) error_vm: &'a BTreeMap<u32, u32>,
    /// The `vm` of each picture a value holds.
    pub(super) image_vm: &'a [(calc_doc_model::value::ImageValue, u32)],
    pub(super) loaded: &'a LoadedSheet,
    pub(super) model: &'a ModelRoot,
    /// Where the sheet's positions went, when rows or columns of it were inserted or deleted: the
    /// part rewritten is then the one the edits' passes moved (`structural.rs`), and each cell's
    /// loaded records are those of the cell it moved from.
    pub(super) moves: Option<&'a SheetMoves>,
    /// The shared formula groups the edits' passes found cut or grown, written as individual
    /// formulas.
    pub(super) cut_groups: &'a BTreeSet<u32>,
    /// The cell formats edits gave the sheet's cells (dc-09v), by (row, column): each cell's `s`
    /// over the one the part gave it.
    pub(super) formats: &'a BTreeMap<(u32, u32), u32>,
    pub(super) style_plan: Option<&'a super::styles::SheetStyles>,
}

/// Keep the complete text cell record, including inline runs, shared-string identity and
/// phonetics. Namespace declarations are retained by the existing source-fragment reader.
pub(crate) fn text_sources(
    xml: &XmlPart,
    sheet: calc_doc_model::value::SheetId,
    decorated: &BTreeSet<u32>,
) -> Result<BTreeMap<CellKey, std::sync::Arc<[u8]>>, SaveRefusal> {
    let cells =
        super::reader::element_sources_in(xml, Some(&["worksheet", "sheetData", "row"]), |e| {
            e.name.is(MAIN_NAMESPACE, "c") && matches!(e.attribute("t"), Some("s" | "inlineStr"))
        })?;
    let mut sources = BTreeMap::new();
    for bytes in cells {
        let cell = XmlPart::new(
            "text-cell.xml".into(),
            bytes.clone(),
            crate::xml::XmlLimits::default(),
        );
        let mut reader = Reader::new(&cell);
        let (root, _) = reader.root("c")?;
        let Some((row, column)) = root.attribute("r").and_then(crate::load::address::cell) else {
            return Err(reader.invalid("text cell has no valid address"));
        };
        let mut keep = root.attribute("ph").is_some();
        let mut in_value = false;
        while let Some((event, _)) = reader.next()? {
            match event {
                XmlEvent::Start(e) => {
                    in_value = e.name.is(MAIN_NAMESPACE, "v");
                    keep |= e.name.is(MAIN_NAMESPACE, "r")
                        || e.name.is(MAIN_NAMESPACE, "rPh")
                        || e.name.is(MAIN_NAMESPACE, "phoneticPr");
                }
                XmlEvent::Text(t) if in_value => {
                    keep |= t
                        .trim()
                        .parse::<u32>()
                        .is_ok_and(|i| decorated.contains(&i));
                }
                XmlEvent::End(_) => in_value = false,
                _ => {}
            }
        }
        if keep {
            sources.insert(CellKey::new(sheet, row, column).unwrap(), bytes.into());
        }
    }
    Ok(sources)
}

impl Job<'_> {
    /// The loaded position of the part's cell (row, column): itself, or for a moved sheet the
    /// cell it moved from (`None` for one the edits inserted).
    fn loaded_at(&self, row: u32, column: u32) -> Option<(u32, u32)> {
        match self.moves {
            Some(moves) => moves.back((row, column)),
            None => Some((row, column)),
        }
    }

    /// The `s` an edit gave the cell, or a save-plane exception for an inherited format.
    /// An explicit source format survives unless an authored assignment overrides it.
    /// Format zero is written as `s="0"` to override an axis default.
    fn style(&self, row: u32, column: u32, source_explicit: bool) -> Option<Option<String>> {
        self.formats
            .get(&(row, column))
            .copied()
            .or_else(|| {
                self.model
                    .definitions()
                    .styles()
                    .runs
                    .get(CellKey::new(self.loaded.id(), row, column).unwrap())
            })
            .or_else(|| {
                if source_explicit {
                    None
                } else {
                    self.style_plan.and_then(|p| {
                        p.cells
                            .iter()
                            .find(|r| {
                                r.area
                                    .contains(CellKey::new(self.loaded.id(), row, column).unwrap())
                            })
                            .map(|r| r.format)
                    })
                }
            })
            .map(|format| Some(format.to_string()))
    }
}

/// `changes` to a start tag, with the `s` an edit gave the cell when one did.
fn with_style<'s>(
    changes: &[(&'s str, Option<&'s str>)],
    style: &'s Option<Option<String>>,
) -> Vec<(&'s str, Option<&'s str>)> {
    let mut changes = changes.to_vec();
    if let Some(style) = style {
        changes.push(("s", style.as_deref()));
    }
    changes
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
    shared: BTreeMap<(u32, u32), SharedMember>,
    bounds: Bounds,
    /// The formula cells the edit did not change whose cached result the rewrite left out.
    caches_dropped: usize,
    values_written: usize,
    values_left_to_excel: usize,
}

/// A rewritten worksheet part.
pub(super) struct Rewritten {
    pub(super) bytes: Vec<u8>,
    /// The formula cells the edit did not change whose cached result was left out.
    pub(super) caches_dropped: usize,
    pub(super) values_written: usize,
    pub(super) values_left_to_excel: usize,
}

/// A cell's `<f>` element: as read, and the byte ranges of its start tag and of the whole element.
struct FormulaChild {
    read: XmlElement,
    tag: (usize, usize),
    element: (usize, usize),
}

/// What the rewrite writes at a cell.
#[derive(Clone, Copy)]
enum Change<'c> {
    /// The cell holds this input.
    Set(&'c CellInput),
    /// The cell is emptied.
    Emptied,
    /// The cell shows a value of an array formula's result beside its anchor: the caller's value
    /// for it.
    Spilled,
    /// Only the cell's format changed (dc-09v): it keeps what it holds, with the `s` an edit gave
    /// it.
    Restyle,
    /// A calculated TABLE output; retain the existing master/attributes, or add its cache
    /// when the source omitted that child cell entirely.
    TableCache,
}

/// A cell's changes, by column, of one row.
type RowChanges<'c> = BTreeMap<u32, Change<'c>>;

/// A cached value as written: its `t`, the text of its `<v>`, and its `vm`.
type Cache = (Option<&'static str>, Vec<u8>, Option<u32>);

/// The area (top, left, bottom, right) as a `ref`.
fn area_text((top, left, bottom, right): (u32, u32, u32, u32)) -> String {
    if (top, left) == (bottom, right) {
        a1(top, left)
    } else {
        format!("{}:{}", a1(top, left), a1(bottom, right))
    }
}

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

/// Only actual member coordinates get shared markers; a bounding ref allocates no cells.
#[derive(Clone, Copy)]
struct SharedMember {
    index: u32,
    master: bool,
    extent: (u32, u32, u32, u32),
}
struct SharedCandidate {
    block: std::sync::Arc<calc_doc_model::FormulaBlock>,
    cells: Vec<CellKey>,
    extent: calc_doc_model::value::GridArea,
    original: Option<u32>,
    unchanged: bool,
    /// The formula names a cell of another sheet.
    cross_sheet: bool,
}
fn shared_members(job: &Job<'_>, unshared: &BTreeSet<u32>) -> BTreeMap<(u32, u32), SharedMember> {
    let mut groups =
        BTreeMap::<usize, (std::sync::Arc<calc_doc_model::FormulaBlock>, Vec<CellKey>)>::new();
    for (at, input) in job.model.sheet_cells(job.loaded.id()).into_iter().flatten() {
        let CellInput::Formula(text) = input else {
            continue;
        };
        let Some(block) = text.block() else { continue };
        if text.save_mode() != calc_doc_model::FormulaSaveMode::SharedGroup
            || text.member_at() != Some(at)
            || job.model.array_containing(at).is_some()
            || job.model.definitions().data_table_containing(at).is_some()
            || job
                .model
                .definitions()
                .tables()
                .iter()
                .any(|table| table.area.contains(at))
        {
            continue;
        }
        groups
            .entry(std::sync::Arc::as_ptr(block) as usize)
            .or_insert_with(|| (block.clone(), Vec::new()))
            .1
            .push(at);
    }
    let mut candidates = Vec::new();
    for (_, (block, mut cells)) in groups {
        cells.sort_unstable();
        let top = cells
            .iter()
            .map(|at| at.row())
            .min()
            .expect("nonempty group");
        let bottom = cells
            .iter()
            .map(|at| at.row())
            .max()
            .expect("nonempty group");
        let left = cells
            .iter()
            .map(|at| at.column())
            .min()
            .expect("nonempty group");
        let right = cells
            .iter()
            .map(|at| at.column())
            .max()
            .expect("nonempty group");
        let extent = calc_doc_model::value::GridArea::new(
            CellKey::new(job.loaded.id(), top, left).expect("group inside grid"),
            CellKey::new(job.loaded.id(), bottom, right).expect("group inside grid"),
        )
        .expect("one sheet");
        let loaded_group = |at: CellKey| {
            job.loaded_at(at.row(), at.column())
                .and_then(|(row, column)| job.loaded.formula(row, column))
                .and_then(|record| record.shared())
        };
        let original = loaded_group(cells[0])
            .map(|group| group.index())
            .filter(|index| {
                cells
                    .iter()
                    .all(|at| loaded_group(*at).is_some_and(|group| group.index() == *index))
            });
        let unchanged = original.is_some_and(|index| {
            index == block.id().local
                && !unshared.contains(&index)
                && job.moves.is_none()
                && loaded_group(cells[0]).is_some_and(|group| {
                    group.area() == block.extent() && group.master() == block.source().anchor()
                })
        });
        let cross_sheet = block.source().source().to_string_lossy().contains('!');
        candidates.push(SharedCandidate {
            block,
            cells,
            extent,
            original,
            unchanged,
            cross_sheet,
        });
    }
    candidates.sort_by_key(|candidate| candidate.cells[0]);
    // Shared-master rectangles may not overlap, even when their actual cells are sparse.
    // Retain untouched file groups first; a conflicting new candidate stays individual.
    let mut extents: Vec<_> = candidates
        .iter()
        .filter(|c| c.unchanged)
        .map(|c| c.block.extent())
        .collect();
    extents.extend(
        job.model
            .definitions()
            .arrays()
            .filter(|array| array.area().sheet() == job.loaded.id())
            .map(|array| array.area()),
    );
    extents.extend(
        job.model
            .definitions()
            .data_tables()
            .iter()
            .filter(|table| table.area().sheet() == job.loaded.id())
            .map(|table| table.area()),
    );
    extents.extend(
        job.model
            .definitions()
            .tables()
            .iter()
            .filter(|table| table.sheet == job.loaded.id())
            .map(|table| table.area),
    );
    for &(top, left, bottom, right) in job.spills.values() {
        if let (Ok(first), Ok(last)) = (
            CellKey::new(job.loaded.id(), top, left),
            CellKey::new(job.loaded.id(), bottom, right),
        ) {
            if let Ok(area) = calc_doc_model::value::GridArea::new(first, last) {
                extents.push(area);
            }
        }
    }
    let mut overlap_checks = 0usize;
    let mut claimed: BTreeSet<_> = candidates
        .iter()
        .filter(|c| c.unchanged)
        .filter_map(|c| c.original)
        .collect();
    let mut used: BTreeSet<_> = job
        .loaded
        .formulas()
        .filter_map(|(_, _, record)| record.shared().map(|group| group.index()))
        .collect();
    let mut next = Some(0u32);
    let mut members = BTreeMap::new();
    for candidate in candidates {
        if candidate.unchanged || candidate.cells.len() < 2 {
            continue;
        }
        // Excel refuses to open a package with a shared group the file did not hold whose formula names
        // another sheet (measured on 16.0.20527 with `PvOne!$A$3` shared by B2 and B20, and by B2:B3,
        // in a saved pivot workbook: Workbooks.Open fails, not even a repair); such formulas are written
        // individually. Groups with no sheet reference open (the full-sheet and overlapping ones).
        if candidate.original.is_none() && candidate.cross_sheet {
            continue;
        }
        let mut overlap = false;
        for extent in &extents {
            if overlap_checks == 1_000_000 {
                overlap = true;
                break;
            }
            overlap_checks += 1;
            if extent.intersection(candidate.extent).is_some() {
                overlap = true;
                break;
            }
        }
        if overlap {
            continue;
        }
        let index = if let Some(index) = candidate.original.filter(|index| !claimed.contains(index))
        {
            index
        } else {
            while next.is_some_and(|index| used.contains(&index)) {
                next = next.and_then(|index| index.checked_add(1));
            }
            let Some(index) = next else { continue };
            next = index.checked_add(1);
            index
        };
        used.insert(index);
        claimed.insert(index);
        extents.push(candidate.extent);
        let first = candidate.extent.first();
        let last = candidate.extent.last();
        let extent = (first.row(), first.column(), last.row(), last.column());
        let master = candidate.cells[0];
        for at in candidate.cells {
            members.insert(
                (at.row(), at.column()),
                SharedMember {
                    index,
                    master: at == master,
                    extent,
                },
            );
        }
    }
    members
}

fn shared_formula(
    prefix: &[u8],
    source: Option<&RawTag<'_>>,
    member: SharedMember,
    text: &[u16],
    dirty: Option<bool>,
    out: &mut Vec<u8>,
) {
    let index = member.index.to_string();
    let reference = member.master.then(|| area_text(member.extent));
    let mut attributes = vec![
        ("t", Some("shared")),
        ("si", Some(index.as_str())),
        ("ref", reference.as_deref()),
    ];
    if let Some(dirty) = dirty {
        attributes.push(("ca", dirty.then_some("1")));
    }
    if let Some(source) = source {
        out.extend(source.rebuilt(&attributes, !member.master));
    } else {
        out.push(b'<');
        out.extend_from_slice(prefix);
        out.extend_from_slice(b"f");
        for (key, value) in attributes {
            if let Some(value) = value {
                out.extend_from_slice(format!(" {key}=\"{value}\"").as_bytes());
            }
        }
        out.extend_from_slice(if member.master { b">" } else { b"/>" });
    }
    if member.master {
        xstring(text, out);
        end_tag(prefix, b"f", out);
    }
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
        .filter(|(row, column, _)| {
            let at = match job.moves {
                Some(moves) => moves.forward((*row, *column)),
                None => Some((*row, *column)),
            };
            at.is_some_and(|at| job.changes.contains_key(&at)
                || job.model.cell(CellKey::new(job.loaded.id(),at.0,at.1)
                    .expect("mapped cell inside the grid")).is_some_and(|input|
                    matches!(input, CellInput::Formula(text) if text.save_mode() == calc_doc_model::FormulaSaveMode::Individual)))
        })
        .filter_map(|(_, _, record)| record.shared().map(|group| group.index()))
        .chain(job.cut_groups.iter().copied())
        .collect();
    let shared = shared_members(job, &unshared);
    let rewrite = Rewrite {
        job,
        reader: Reader::new(xml),
        splice: Splice::new(xml.bytes()),
        strings,
        unshared,
        shared,
        bounds: Bounds::default(),
        caches_dropped: 0,
        values_written: 0,
        values_left_to_excel: 0,
    };
    rewrite.worksheet()
}

/// Whether a cell's style index (`s`, 0 when absent) is not 0, as the loader reads it.
fn styled(element: &XmlElement) -> bool {
    element
        .attribute("s")
        .is_some_and(|style| collapsed(style).parse::<u32>() != Ok(0))
}

/// A value as a cell's cache: its `t`, the text of its `<v>` and its `vm`. A newer error is
/// `t="e"`, the `#VALUE!` fallback Excel writes, and the `vm` of its value metadata record in
/// `error_vm`; `None` for a newer error without one (#BUSY!), which is left for Excel to
/// calculate.
fn cache_xml(
    value: &CachedValue,
    error_vm: &BTreeMap<u32, u32>,
    image_vm: &[(calc_doc_model::value::ImageValue, u32)],
) -> Option<Cache> {
    Some(match value {
        CachedValue::Image(image) => (
            Some("e"),
            b"#VALUE!".to_vec(),
            Some(image_vm.iter().find(|(held, _)| held == image)?.1),
        ),
        CachedValue::Number(number) => (None, super::text::number(number.get()).into_bytes(), None),
        CachedValue::Text(text) => {
            let mut out = Vec::new();
            xstring(text.text().utf16_code_units(), &mut out);
            (Some("str"), out, None)
        }
        CachedValue::Logical(logical) => {
            (Some("b"), u8::from(*logical).to_string().into_bytes(), None)
        }
        CachedValue::Error(code) if crate::edit::error_constant_writable(*code) => (
            Some("e"),
            ERRORS
                .iter()
                .find(|(_, known)| known == code)
                .map(|(spelling, _)| spelling.as_bytes().to_vec())
                .expect("every worksheet error has a spelling"),
            None,
        ),
        CachedValue::Error(code) => (
            Some("e"),
            b"#VALUE!".to_vec(),
            Some(*error_vm.get(&super::array_metadata::rich_error_type(*code)?)?),
        ),
    })
}

impl<'j> Rewrite<'_, 'j> {
    /// The cached value a formula cell at `row` and `column` is written with, counted: the
    /// caller's value, or `None` (a save without values, a cell the caller gave none, or a value
    /// Excel stores only with value metadata).
    fn formula_cache(&mut self, row: u32, column: u32) -> Option<Cache> {
        let values = self.job.values?;
        match values
            .get(&(row, column))
            .and_then(|value| cache_xml(value, self.job.error_vm, self.job.image_vm))
        {
            Some(cache) => {
                self.values_written += 1;
                Some(cache)
            }
            None => {
                self.values_left_to_excel += 1;
                None
            }
        }
    }

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
            values_written: self.values_written,
            values_left_to_excel: self.values_left_to_excel,
        })
    }

    fn sheet_data(&mut self, span: (usize, usize)) -> Result<(), SaveRefusal> {
        let changes = self.job.changes;
        let mut rows: BTreeMap<u32, RowChanges<'_>> = BTreeMap::new();
        for (&(row, column), input) in changes {
            let change = match input {
                Some(input) => Change::Set(input),
                None => Change::Emptied,
            };
            rows.entry(row).or_default().insert(column, change);
        }
        // The cells beside each written anchor that the caller gives a value.
        if let Some(values) = self.job.values {
            for &(row, column) in values.keys() {
                let at = CellKey::new(self.job.loaded.id(), row, column)
                    .expect("a result cell is in the grid");
                if self
                    .job
                    .model
                    .definitions()
                    .data_table_containing(at)
                    .is_some()
                {
                    rows.entry(row)
                        .or_default()
                        .entry(column)
                        .or_insert(Change::TableCache);
                }
            }
            for (&anchor, &(top, left, bottom, right)) in self.job.spills {
                for (&(row, column), _) in values.range((top, left)..=(bottom, right)) {
                    if (left..=right).contains(&column)
                        && (row, column) != anchor
                        && !matches!(
                            rows.get(&row).and_then(|cells| cells.get(&column)),
                            Some(Change::Set(_))
                        )
                    {
                        rows.entry(row).or_default().insert(column, Change::Spilled);
                    }
                }
            }
        }
        // The cells whose format alone an edit changed.
        for &(row, column) in self.job.formats.keys() {
            rows.entry(row)
                .or_default()
                .entry(column)
                .or_insert(Change::Restyle);
        }
        if let Some(plan) = self.job.style_plan {
            for run in &plan.cells {
                for row in run.area.first().row()..=run.area.last().row() {
                    rows.entry(row).or_default();
                }
            }
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

    /// Appends a row the part did not hold, with the cells set in it; nothing when none of them
    /// is written.
    fn new_row(&mut self, row: u32, cells: &RowChanges<'_>, prefix: &[u8], out: &mut Vec<u8>) {
        let cells = self.formatted_row(row, cells);
        let mut written = Vec::new();
        for (&column, change) in &cells {
            self.new_cell(None, prefix, row, column, *change, &mut written);
        }
        if written.is_empty() {
            return;
        }
        out.push(b'<');
        out.extend_from_slice(prefix);
        out.extend_from_slice(format!("row r=\"{row}\">").as_bytes());
        out.extend_from_slice(&written);
        end_tag(prefix, b"row", out);
    }

    fn formatted_row<'c>(&self, row: u32, cells: &RowChanges<'c>) -> RowChanges<'c> {
        let mut cells = cells.clone();
        if let Some(plan) = self.job.style_plan {
            for run in &plan.cells {
                if (run.area.first().row()..=run.area.last().row()).contains(&row) {
                    for col in run.area.first().column()..=run.area.last().column() {
                        cells.entry(col).or_insert(Change::Restyle);
                    }
                }
            }
        }
        cells
    }
    /// Appends an empty cell when its format is an explicit exception, including style zero.
    fn styled_empty(&mut self, prefix: &[u8], row: u32, column: u32, out: &mut Vec<u8>) {
        if let Some(Some(style)) = self.job.style(row, column, false) {
            let at = a1(row, column);
            out.push(b'<');
            out.extend_from_slice(prefix);
            out.extend_from_slice(format!("c r=\"{at}\" s=\"{style}\"/>").as_bytes());
            self.bounds.add(row, column);
        }
    }

    /// Appends a cell holding what `change` gives it (for [`Change::Emptied`] and
    /// [`Change::Restyle`], which come without a `source`, at most an empty cell with its format),
    /// its start tag rebuilt from `source` (the cell it replaces) or new.
    fn new_cell(
        &mut self,
        source: Option<&RawTag<'_>>,
        prefix: &[u8],
        row: u32,
        column: u32,
        change: Change<'_>,
        out: &mut Vec<u8>,
    ) {
        let input = match change {
            Change::Set(input) => input,
            Change::Emptied | Change::Restyle => {
                return self.styled_empty(prefix, row, column, out);
            }
            Change::Spilled | Change::TableCache => {
                return self.spilled_cell(source, prefix, row, column, out);
            }
        };
        if matches!(input, CellInput::Text(_))
            && let Some(bytes) = self
                .job
                .model
                .definitions()
                .text_sources()
                .get(&CellKey::new(self.job.loaded.id(), row, column).unwrap())
        {
            let xml = XmlPart::new(
                "text-cell.xml".into(),
                bytes.to_vec(),
                crate::xml::XmlLimits::default(),
            );
            let (root, (_, end)) = Reader::new(&xml)
                .root("c")
                .expect("validated retained text cell");
            let tag = RawTag::parse(&bytes[..end]);
            let address = a1(row, column);
            let style = self.job.style(row, column, tag.has("s"));
            out.extend(tag.rebuilt(
                &with_style(&[("r", Some(&address))], &style),
                tag.is_empty(),
            ));
            out.extend_from_slice(&bytes[end..]);
            if root.attribute("t") == Some("s") {
                self.strings.added += 1;
            }
            self.bounds.add(row, column);
            return;
        }
        let at = a1(row, column);
        let style = self
            .job
            .style(row, column, source.is_some_and(|t| t.has("s")));
        let kind = match input {
            CellInput::Number(_) | CellInput::Formula(_) => None,
            CellInput::Text(_) => Some("s"),
            CellInput::Logical(_) => Some("b"),
            CellInput::Error(_) => Some("e"),
        };
        let cache = match input {
            CellInput::Formula(_) => self.formula_cache(row, column),
            _ => None,
        };
        let kind = match &cache {
            Some((cache_kind, _, _)) => *cache_kind,
            None => kind,
        };
        // A constant that holds a newer error (a pivot's corner when the pivot took one cell) is written as
        // Excel writes it: `#VALUE!` with the value metadata of the error.
        let rich_error = match input {
            CellInput::Error(code) if !crate::edit::error_constant_writable(*code) => {
                super::array_metadata::rich_error_type(*code)
                    .and_then(|kind| self.job.error_vm.get(&kind).copied())
            }
            _ => None,
        };
        let vm = cache
            .as_ref()
            .and_then(|(_, _, vm)| vm.map(|vm| vm.to_string()))
            .or_else(|| rich_error.map(|vm| vm.to_string()));
        let form = match input {
            CellInput::Formula(_) => Some(
                self.job
                    .forms
                    .get(&(row, column))
                    .expect("each formula among the changes has its file form"),
            ),
            _ => None,
        };
        // The array formula the model anchors at the cell, if any: an authored dynamic array is
        // written as one whatever its file form, and a legacy array over its area.
        let array = CellKey::new(self.job.loaded.id(), row, column)
            .ok()
            .and_then(|key| self.job.model.definitions().array(key).copied())
            .filter(|_| form.is_some());
        let legacy = array.filter(|array| array.kind() == ArrayKind::Legacy);
        // A formula is written as a dynamic array only when the model holds one there: an
        // ordinary formula is read with implicit intersection, as Excel reads an `<f>` without
        // `t="array"`, whatever its file form could be.
        let dynamic = array.is_some_and(|array| matches!(array.kind(), ArrayKind::Dynamic { .. }))
            || (legacy.is_none()
                && matches!(input,CellInput::Formula(text)
                if text.form() == calc_doc_model::FormulaForm::Dynamic));
        // A dynamic-array formula's cell names its cell metadata.
        let cm = dynamic.then(|| {
            self.job
                .dynamic_cm
                .expect("a save with a dynamic array has its cell metadata")
                .to_string()
        });
        let prefix = source.map_or(prefix, |tag| tag.prefix());
        match source {
            Some(tag) => out.extend(tag.rebuilt(
                &with_style(
                    &[
                        ("r", Some(&at)),
                        ("t", kind),
                        ("cm", cm.as_deref()),
                        ("vm", vm.as_deref()),
                    ],
                    &style,
                ),
                false,
            )),
            None => {
                out.push(b'<');
                out.extend_from_slice(prefix);
                out.extend_from_slice(format!("c r=\"{at}\"").as_bytes());
                if let Some(Some(style)) = &style {
                    out.extend_from_slice(format!(" s=\"{style}\"").as_bytes());
                }
                if let Some(kind) = kind {
                    out.extend_from_slice(format!(" t=\"{kind}\"").as_bytes());
                }
                if let Some(cm) = &cm {
                    out.extend_from_slice(format!(" cm=\"{cm}\"").as_bytes());
                }
                if let Some(vm) = &vm {
                    out.extend_from_slice(format!(" vm=\"{vm}\"").as_bytes());
                }
                out.push(b'>');
            }
        }
        if let Some(form) = form {
            if let Some(member) = self.shared.get(&(row, column)).copied() {
                shared_formula(
                    prefix,
                    None,
                    member,
                    &form.text,
                    Some(self.job.dirty_caches.contains(&(row, column))),
                    out,
                );
            } else {
                if let Some(array) = legacy {
                    let (first, last) = (array.area().first(), array.area().last());
                    let reference =
                        area_text((first.row(), first.column(), last.row(), last.column()));
                    out.push(b'<');
                    out.extend_from_slice(prefix);
                    out.extend_from_slice(format!("f t=\"array\" ref=\"{reference}\">").as_bytes());
                } else if dynamic {
                    let reference = self
                        .job
                        .spills
                        .get(&(row, column))
                        .map_or_else(|| at.clone(), |area| area_text(*area));
                    out.push(b'<');
                    out.extend_from_slice(prefix);
                    out.extend_from_slice(format!("f t=\"array\" ref=\"{reference}\">").as_bytes());
                } else if self.job.dirty_caches.contains(&(row, column)) {
                    out.push(b'<');
                    out.extend_from_slice(prefix);
                    out.extend_from_slice(b"f ca=\"1\">");
                } else {
                    start_tag(prefix, b"f", out);
                }
                xstring(&form.text, out);
                end_tag(prefix, b"f", out);
            }
            if let Some((_, text, _)) = &cache {
                start_tag(prefix, b"v", out);
                out.extend_from_slice(text);
                end_tag(prefix, b"v", out);
            }
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
                CellInput::Error(_) if rich_error.is_some() => "#VALUE!".to_owned(),
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

    /// Appends a cell an array formula's result fills beside its anchor: the caller's value alone,
    /// its start tag rebuilt from `source` (keeping its style) or new.
    fn spilled_cell(
        &mut self,
        source: Option<&RawTag<'_>>,
        prefix: &[u8],
        row: u32,
        column: u32,
        out: &mut Vec<u8>,
    ) {
        let Some((kind, text, vm)) = self
            .job
            .values
            .and_then(|values| values.get(&(row, column)))
            .and_then(|value| cache_xml(value, self.job.error_vm, self.job.image_vm))
        else {
            // A value Excel holds only with value metadata the save has none for: the cell is
            // left out, for Excel to fill when it calculates the anchor.
            match source {
                Some(tag) => {
                    let at = a1(row, column);
                    let style = self.job.style(row, column, tag.has("s"));
                    out.extend(tag.rebuilt(
                        &with_style(
                            &[("r", Some(&at)), ("t", None), ("cm", None), ("vm", None)],
                            &style,
                        ),
                        true,
                    ));
                    self.bounds.add(row, column);
                }
                None => self.styled_empty(prefix, row, column, out),
            }
            return;
        };
        let at = a1(row, column);
        let style = self
            .job
            .style(row, column, source.is_some_and(|t| t.has("s")));
        let vm = vm.map(|vm| vm.to_string());
        let prefix = source.map_or(prefix, |tag| tag.prefix());
        match source {
            Some(tag) => out.extend(tag.rebuilt(
                &with_style(
                    &[
                        ("r", Some(&at)),
                        ("t", kind),
                        ("cm", None),
                        ("vm", vm.as_deref()),
                    ],
                    &style,
                ),
                false,
            )),
            None => {
                out.push(b'<');
                out.extend_from_slice(prefix);
                out.extend_from_slice(format!("c r=\"{at}\"").as_bytes());
                if let Some(Some(style)) = &style {
                    out.extend_from_slice(format!(" s=\"{style}\"").as_bytes());
                }
                if let Some(kind) = kind {
                    out.extend_from_slice(format!(" t=\"{kind}\"").as_bytes());
                }
                if let Some(vm) = &vm {
                    out.extend_from_slice(format!(" vm=\"{vm}\"").as_bytes());
                }
                out.push(b'>');
            }
        }
        start_tag(prefix, b"v", out);
        out.extend_from_slice(&text);
        end_tag(prefix, b"v", out);
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
        let cells = self.formatted_row(number, cells);
        let prefix = tag.prefix().to_vec();
        let row_text = number.to_string();
        let changes: [(&str, Option<&str>); 2] = [("spans", None), ("r", Some(&row_text))];
        let retag = element.attribute("spans").is_some() || element.attribute("r").is_none();
        if tag.is_empty() {
            self.reader.expect()?;
            let mut added = Vec::new();
            for (&column, change) in &cells {
                self.new_cell(None, &prefix, number, column, *change, &mut added);
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
                    while let Some((&new, change)) = pending.next_if(|(new, _)| **new < column) {
                        let mut bytes = Vec::new();
                        self.new_cell(None, &prefix, number, new, *change, &mut bytes);
                        if !bytes.is_empty() {
                            self.splice.replace((span.0, span.0), &bytes);
                        }
                    }
                    let change = pending
                        .next_if(|(new, _)| **new == column)
                        .map(|(_, change)| *change);
                    self.cell(span, &element, &tag, number, column, change)?;
                }
                (XmlEvent::Start(_), (start, _)) => {
                    // A row holds its cells and then an `extLst` (CT_Row), so the cells still to
                    // add go before the first element that is not a cell. A cell after such an
                    // element must then lie after every cell added, or it is refused below.
                    for (&column, change) in pending.by_ref() {
                        let mut bytes = Vec::new();
                        self.new_cell(None, &prefix, number, column, *change, &mut bytes);
                        if !bytes.is_empty() {
                            self.splice.replace((start, start), &bytes);
                        }
                        previous = previous.max(column);
                    }
                    self.reader.element_end()?;
                }
                (XmlEvent::Text(_), _) => {}
                (XmlEvent::End(_), (start, _)) => {
                    let mut bytes = Vec::new();
                    for (&column, change) in pending {
                        self.new_cell(None, &prefix, number, column, *change, &mut bytes);
                    }
                    self.splice.replace((start, start), &bytes);
                    return Ok(());
                }
            }
        }
    }

    /// Rewrites one cell of the part, whose start tag, read as `element`, is `tag` and spans
    /// `span`; `change` is what it now holds, `None` when it did not change.
    fn cell(
        &mut self,
        span: (usize, usize),
        element: &XmlElement,
        tag: &RawTag<'_>,
        row: u32,
        column: u32,
        change: Option<Change<'_>>,
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
        // A cell whose format alone changed is written as an unchanged one, with its `s`.
        let change =
            change.filter(|change| !matches!(change, Change::Restyle | Change::TableCache));
        let style = self
            .job
            .style(row, column, element.attribute("s").is_some());
        let cleared = change.is_none()
            && self.job.cleared.iter().any(|&(top, left, bottom, right)| {
                (top..=bottom).contains(&row)
                    && (left..=right).contains(&column)
                    && (row, column) != (top, left)
            });
        let loaded = self.job.loaded_at(row, column);
        if cleared
            && loaded
                .is_some_and(|(row, column)| self.job.loaded.value_cache(row, column).is_some())
        {
            self.caches_dropped += 1;
        }
        let change = if cleared {
            Some(Change::Emptied)
        } else {
            change
        };
        match change {
            Some(
                change @ (Change::Set(_) | Change::Spilled | Change::Restyle | Change::TableCache),
            ) => {
                let mut bytes = Vec::new();
                self.new_cell(Some(tag), b"", row, column, change, &mut bytes);
                self.splice.replace((span.0, end), &bytes);
                if shared_string {
                    self.strings.removed += 1;
                }
            }
            Some(Change::Emptied) => {
                let keeps_style = match &style {
                    Some(style) => style.is_some(),
                    None => {
                        styled(element)
                            || (element.attribute("s").is_some()
                                && (self.job.moves.is_some()
                                    || self.job.style_plan.is_some()
                                    || self.job.loaded.default_styles().any(|r| r.format != 0)))
                    }
                };
                let bytes = if keeps_style {
                    self.bounds.add(row, column);
                    tag.rebuilt(
                        &with_style(
                            &[("r", Some(&at)), ("t", None), ("cm", None), ("vm", None)],
                            &style,
                        ),
                        true,
                    )
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
                let at_key = CellKey::new(self.job.loaded.id(), row, column)
                    .expect("a parsed worksheet cell is inside the grid");
                if self
                    .job
                    .model
                    .definitions()
                    .data_table_containing(at_key)
                    .is_some()
                    && self
                        .job
                        .values
                        .is_some_and(|values| values.contains_key(&(row, column)))
                    && let Some((kind, text, vm)) = self.formula_cache(row, column)
                {
                    // Change only the cache. The TABLE master, its r1/r2/ref attributes,
                    // child membership and every other cell attribute stay in the source.
                    let vm = vm.map(|vm| vm.to_string());
                    let mut cache = Vec::new();
                    start_tag(tag.prefix(), b"v", &mut cache);
                    cache.extend_from_slice(&text);
                    end_tag(tag.prefix(), b"v", &mut cache);
                    let rebuilt = tag.rebuilt(
                        &with_style(
                            &[("r", Some(&at)), ("t", kind), ("vm", vm.as_deref())],
                            &style,
                        ),
                        false,
                    );
                    if tag.is_empty() {
                        let mut bytes = rebuilt;
                        bytes.extend_from_slice(&cache);
                        end_tag(tag.prefix(), b"c", &mut bytes);
                        self.splice.replace(span, &bytes);
                    } else {
                        self.splice.replace(span, &rebuilt);
                        if let Some(value) = value {
                            self.splice.replace(value, &cache);
                        } else {
                            let closing = self.source()[span.1..end]
                                .iter()
                                .rposition(|b| *b == b'<')
                                .map(|offset| span.1 + offset)
                                .expect("a parsed nonempty cell has a closing tag");
                            self.splice.replace((closing, closing), &cache);
                        }
                    }
                    return Ok(());
                }
                let Some(FormulaChild {
                    read: formula_read,
                    tag: formula_tag,
                    element: formula_element,
                }) = formula
                else {
                    if element.attribute("r").is_none() || style.is_some() {
                        self.splice.replace(
                            span,
                            &tag.rebuilt(&with_style(&[("r", Some(&at))], &style), tag.is_empty()),
                        );
                    }
                    return Ok(());
                };
                // Without new calculated TABLE caches, keep the source cells and formula,
                // apart from a format an edit gave them.
                if formula_read.attribute("t") == Some("dataTable") {
                    if element.attribute("r").is_none() || style.is_some() {
                        self.splice.replace(
                            span,
                            &tag.rebuilt(&with_style(&[("r", Some(&at))], &style), tag.is_empty()),
                        );
                    }
                    return Ok(());
                }
                // A formula cell: its cache, its type and its value metadata go (replaced by the
                // caller's value when the save has one), and a cell of a group written as
                // individual formulas gets its own text.
                let cache = self.formula_cache(row, column);
                let kind = cache.as_ref().and_then(|(kind, _, _)| *kind);
                let vm = cache
                    .as_ref()
                    .and_then(|(_, _, vm)| vm.map(|vm| vm.to_string()));
                self.splice.replace(
                    span,
                    &tag.rebuilt(
                        &with_style(
                            &[("r", Some(&at)), ("t", kind), ("vm", vm.as_deref())],
                            &style,
                        ),
                        false,
                    ),
                );
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
                // An array formula's `ref` is the area its result fills now.
                if formula_read.attribute("t") == Some("array")
                    && let Some(area) = self.job.spills.get(&(row, column))
                {
                    let reference = area_text(*area);
                    edits.push((
                        formula_tag,
                        formula_raw.rebuilt(&[("ref", Some(&reference))], formula_raw.is_empty()),
                    ));
                }
                let dirty = self.job.dirty_caches.contains(&(row, column));
                let unshared = group.is_some_and(|group| self.unshared.contains(&group));
                let shared = self.shared.get(&(row, column)).copied();
                let tracked = self.job.cache_state
                    && cache.is_some()
                    && formula_read.attribute("t") != Some("array");
                if tracked && !unshared && shared.is_none() {
                    edits.push((
                        formula_tag,
                        formula_raw
                            .rebuilt(&[("ca", dirty.then_some("1"))], formula_raw.is_empty()),
                    ));
                }
                if let Some(member) = shared {
                    let key = CellKey::new(self.job.loaded.id(), row, column)
                        .expect("a member is inside the grid");
                    let Some(CellInput::Formula(text)) = self.job.model.cell(key) else {
                        return Err(self.reader.invalid("a shared member lost its formula"));
                    };
                    let mut bytes = Vec::new();
                    let rendered = member.master.then(|| text.rendered());
                    shared_formula(
                        formula_raw.prefix(),
                        Some(&formula_raw),
                        member,
                        rendered
                            .as_ref()
                            .map_or(&[][..], |text| text.utf16_code_units()),
                        tracked.then_some(dirty),
                        &mut bytes,
                    );
                    edits.push((formula_element, bytes));
                } else if let Some(group) = group
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
                    let mut attributes = vec![("t", None), ("ref", None), ("si", None)];
                    if tracked {
                        attributes.push(("ca", dirty.then_some("1")));
                    }
                    let mut bytes = formula_raw.rebuilt(&attributes, false);
                    xstring(text.rendered().utf16_code_units(), &mut bytes);
                    end_tag(prefix, b"f", &mut bytes);
                    edits.push((formula_element, bytes));
                }
                let cached = loaded
                    .and_then(|(row, column)| self.job.loaded.formula(row, column))
                    .and_then(|record| record.cache())
                    .is_some();
                let new_value = cache.map(|(_, text, _)| {
                    let prefix = formula_raw.prefix();
                    let mut bytes = Vec::new();
                    start_tag(prefix, b"v", &mut bytes);
                    bytes.extend_from_slice(&text);
                    end_tag(prefix, b"v", &mut bytes);
                    bytes
                });
                match (value, new_value) {
                    (Some(value), Some(bytes)) => edits.push((value, bytes)),
                    (Some(value), None) => {
                        edits.push((value, Vec::new()));
                        if cached {
                            self.caches_dropped += 1;
                        }
                    }
                    (None, Some(bytes)) => {
                        edits.push(((formula_element.1, formula_element.1), bytes));
                    }
                    (None, None) => {}
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
