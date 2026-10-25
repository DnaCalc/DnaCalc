//! Rewriting a formula's references for a structural edit (L3, dc-m3u): rows or columns inserted
//! or deleted, a sheet renamed or removed. [`transform_formula`] takes a formula's text as a file
//! stores it (A1 notation, no leading `=`) and its cell before the edit, and gives the text the
//! edit leaves, every other character kept as written.
//!
//! The rules are Excel's for its reference adjustment (Microsoft's "Insert or delete rows and
//! columns" and "Move or copy cells"): a reference on the edited sheet moves with the cells it
//! names, whether its coordinates are relative or absolute (`$`); a range that the inserted rows
//! fall inside grows; a range some of whose rows are deleted shrinks; a reference all of whose
//! cells are deleted becomes `#REF!` (`Sheet2!#REF!` with its qualifier); a reference pushed past
//! the grid's last row or column becomes `#REF!`, and a range is cut at the grid's edge. The axis
//! arithmetic is ported from OxCalc 1bd514ef (`src/oxcalc-core/src/grid/reference_engine/
//! transform.rs`, `transform_structural_axis_index` and `transform_structural_axis_range`); its
//! own reference model and payload codecs are left behind. A renamed sheet is renamed in every
//! qualifier that names it, quoted when its new name needs quotes; a removed sheet's qualifier
//! becomes `#REF!` (`#REF!A1`), and a run of sheets loses an end that is removed to the next sheet
//! inward.
//!
//! Left as written: a 3-D reference under a row or column edit of one of its sheets (Excel
//! 16.0.20430, 2026-10-25, through COM: `=SUM(Ja:Ma!A1:A3)`, `=SUM(Ja:Ma!B5)`, `=SUM(Ja:Fe!A2:B2)`
//! and `=SUM(Fe:Ma!$A$4)` kept their text when a row was inserted on Fe or on Ja, a row deleted on
//! Ma and a column inserted on Fe, and read the cells now there), a structured reference (its table
//! moves in the document), and reference text a function reads (INDIRECT's).
//!
//! A defined name's formula ([`transform_name`], dc-10y) moves only the coordinates it holds
//! absolute: its relative ones are offsets from the cell that uses it, which no edit changes. Excel
//! 16.0.20430 (2026-10-25), names defined relative to Nm!C3: LeftCell (`RC[-1]`, stored `Nm!XFD1`)
//! and AboveCell (`R[-1]C`, `Nm!A1048576`) kept their text when column A and row 1 were inserted on
//! Nm and row 3 deleted; ColA (`Nm!$A1`) became `Nm!$B1` when column A was inserted and kept it
//! when a row was; Abs (`Nm!$A$1`) became `Nm!$B$1`, then `Nm!$B$2`.

use super::bind::{ReferenceSite, bind_references};
use super::notation;
use super::{
    AxisRef, FormulaAnchor, ReferenceCatalog, ReferenceNotation, ReferenceShape, SheetTarget,
};
use crate::syntax::{SyntaxChild, SyntaxKind, SyntaxNode, TextSpan, Token, TokenKind};
use calc_func::value::{CellKey, GridArea, MAX_COLUMNS, MAX_ROWS, SheetId};

/// The rows or the columns of a sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EditAxis {
    Rows,
    Columns,
}

/// A structural edit, as [`transform_formula`] applies it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum StructuralEdit {
    /// `count` rows or columns inserted on `sheet` before the 1-based position `first`.
    Insert {
        sheet: SheetId,
        axis: EditAxis,
        first: u32,
        count: u32,
    },
    /// `count` rows or columns deleted on `sheet` from the 1-based position `first`.
    Delete {
        sheet: SheetId,
        axis: EditAxis,
        first: u32,
        count: u32,
    },
    /// `sheet` renamed to `name`.
    RenameSheet { sheet: SheetId, name: String },
    /// `sheet` removed; `order` is the workbook's sheets, with their names, in their order before
    /// the removal.
    RemoveSheet {
        sheet: SheetId,
        order: Vec<(SheetId, String)>,
    },
}

impl StructuralEdit {
    /// Where the 1-based position `index` on `axis` of `sheet` lies after the edit: `None` when
    /// the edit deletes it or pushes it past the grid's edge.
    pub fn position(&self, sheet: SheetId, axis: EditAxis, index: u32) -> Option<u32> {
        match self {
            StructuralEdit::Insert {
                sheet: edited,
                axis: on,
                first,
                count,
            } if *edited == sheet && *on == axis => {
                transform_index(index, Change::Insert(*first, *count), max(axis))
            }
            StructuralEdit::Delete {
                sheet: edited,
                axis: on,
                first,
                count,
            } if *edited == sheet && *on == axis => {
                transform_index(index, Change::Delete(*first, *count), max(axis))
            }
            _ => Some(index),
        }
    }

    /// The cell `at` moves to; `None` when the edit deletes it (its row, its column or its sheet)
    /// or pushes it off the grid.
    pub fn cell(&self, at: CellKey) -> Option<CellKey> {
        if let StructuralEdit::RemoveSheet { sheet, .. } = self
            && *sheet == at.sheet()
        {
            return None;
        }
        let row = self.position(at.sheet(), EditAxis::Rows, at.row())?;
        let column = self.position(at.sheet(), EditAxis::Columns, at.column())?;
        CellKey::new(at.sheet(), row, column).ok()
    }

    /// The area `area` covers after the edit, as a range reference to it is adjusted: grown by
    /// rows or columns inserted inside it, shrunk by those deleted from it, cut at the grid's
    /// edge; `None` when every cell of it is deleted (or its sheet removed).
    pub fn area(&self, area: GridArea) -> Option<GridArea> {
        if let StructuralEdit::RemoveSheet { sheet, .. } = self
            && *sheet == area.sheet()
        {
            return None;
        }
        let (first, last) = (area.first(), area.last());
        let (top, bottom) = self.range(area.sheet(), EditAxis::Rows, first.row(), last.row())?;
        let (left, right) = self.range(
            area.sheet(),
            EditAxis::Columns,
            first.column(),
            last.column(),
        )?;
        GridArea::new(
            CellKey::new(area.sheet(), top, left).ok()?,
            CellKey::new(area.sheet(), bottom, right).ok()?,
        )
        .ok()
    }

    fn range(&self, sheet: SheetId, axis: EditAxis, start: u32, end: u32) -> Option<(u32, u32)> {
        match self {
            StructuralEdit::Insert {
                sheet: edited,
                axis: on,
                first,
                count,
            } if *edited == sheet && *on == axis => {
                transform_range(start, end, Change::Insert(*first, *count), max(axis))
            }
            StructuralEdit::Delete {
                sheet: edited,
                axis: on,
                first,
                count,
            } if *edited == sheet && *on == axis => {
                transform_range(start, end, Change::Delete(*first, *count), max(axis))
            }
            _ => Some((start, end)),
        }
    }
}

fn max(axis: EditAxis) -> u32 {
    match axis {
        EditAxis::Rows => MAX_ROWS,
        EditAxis::Columns => MAX_COLUMNS,
    }
}

/// An axis edit: positions inserted before `.0`, or deleted from `.0`, `.1` of them.
#[derive(Clone, Copy)]
enum Change {
    Insert(u32, u32),
    Delete(u32, u32),
}

/// Ported from OxCalc's `transform_structural_axis_index`: where one position goes.
fn transform_index(index: u32, change: Change, max: u32) -> Option<u32> {
    match change {
        Change::Insert(before, count) => {
            if index < before {
                return Some(index);
            }
            let moved = index.checked_add(count)?;
            (moved <= max).then_some(moved)
        }
        Change::Delete(first, count) => {
            let last = first + (count - 1);
            if index < first {
                Some(index)
            } else if index <= last {
                None
            } else {
                Some(index - count)
            }
        }
    }
}

/// Ported from OxCalc's `transform_structural_axis_range`: where a run of positions goes; `None`
/// when every one of them is deleted or pushed past the edge.
fn transform_range(start: u32, end: u32, change: Change, max: u32) -> Option<(u32, u32)> {
    match change {
        Change::Insert(before, count) => {
            if before > end {
                return Some((start, end));
            }
            if before <= start {
                let moved = start.checked_add(count)?;
                if moved > max {
                    return None;
                }
                return Some((moved, end.saturating_add(count).min(max)));
            }
            Some((start, end.saturating_add(count).min(max)))
        }
        Change::Delete(first, count) => {
            let last = first + (count - 1);
            if last < start {
                return Some((start - count, end - count));
            }
            if first > end {
                return Some((start, end));
            }
            let overlap = end.min(last) - start.max(first) + 1;
            let length = end - start + 1;
            if overlap == length {
                return None;
            }
            let new_start = if first <= start { first } else { start };
            Some((new_start, new_start + (length - overlap) - 1))
        }
    }
}

/// The text `edit` leaves of the formula `text` (as a file stores it, without `=`) in the cell
/// `at` before the edit, its sheets resolved through `catalog`, the workbook before the edit; `None`
/// when the edit changes none of its references. See the module documentation.
pub fn transform_formula(
    text: &str,
    at: CellKey,
    edit: &StructuralEdit,
    catalog: &dyn ReferenceCatalog,
) -> Option<String> {
    transform(text, at, edit, catalog, false)
}

/// The text `edit` leaves of a defined name's formula `text` (as a file stores it, without `=`,
/// its relative coordinates relative to A1 of `sheet`), `None` when the edit changes none of its
/// references: a row or column edit moves only the coordinates the name holds absolute on the
/// edited axis (see the module documentation).
pub fn transform_name(
    text: &str,
    sheet: SheetId,
    edit: &StructuralEdit,
    catalog: &dyn ReferenceCatalog,
) -> Option<String> {
    let a1 = CellKey::new(sheet, 1, 1).expect("A1 lies on the grid");
    transform(text, a1, edit, catalog, true)
}

fn transform(
    text: &str,
    at: CellKey,
    edit: &StructuralEdit,
    catalog: &dyn ReferenceCatalog,
    name: bool,
) -> Option<String> {
    let source = format!("={text}");
    let tree = crate::parse_formula(&source);
    if !tree.diagnostics().is_empty() {
        return None;
    }
    let units: Vec<u16> = source.encode_utf16().collect();
    let mut replacements: Vec<(TextSpan, String)> = Vec::new();
    match edit {
        StructuralEdit::Insert { .. } | StructuralEdit::Delete { .. } => {
            let anchor = FormulaAnchor::new(at);
            for site in bind_references(&tree, ReferenceNotation::A1, anchor, catalog) {
                if name {
                    match name_ends(&site, &units, edit) {
                        NameEnds::Absolute => {}
                        NameEnds::Relative => continue,
                        NameEnds::Mixed => {
                            if let Some(text) = mixed_name_site(&site, &units, anchor, edit) {
                                replacements.push((site.span, text));
                            }
                            continue;
                        }
                    }
                }
                if let Some(text) = axis_site(&site, &units, anchor, edit) {
                    replacements.push((site.span, text));
                }
            }
        }
        StructuralEdit::RenameSheet { sheet, name } => {
            qualifier_tokens(tree.root(), &mut |token| {
                let written = unquoted(token.text());
                if let Some((first, last)) = quoted_span(token) {
                    let renamed = |part: &str| {
                        if catalog.sheet_named(part) == Some(*sheet) {
                            name.clone()
                        } else {
                            part.to_owned()
                        }
                    };
                    let (a, b) = (renamed(&first), renamed(&last));
                    if (a.as_str(), b.as_str()) != (first.as_str(), last.as_str()) {
                        replacements.push((token.span(), span_text(&a, &b)));
                    }
                } else if catalog.sheet_named(&written) == Some(*sheet) {
                    replacements.push((token.span(), quoted_if_needed(name)));
                }
            });
        }
        StructuralEdit::RemoveSheet { sheet, order } => {
            sheet_removal(tree.root(), *sheet, order, catalog, &mut replacements);
        }
    }
    if replacements.is_empty() {
        return None;
    }
    replacements.sort_by_key(|(span, _)| span.start);
    let mut out: Vec<u16> = Vec::with_capacity(units.len());
    let mut at_unit = 0;
    for (span, text) in replacements {
        if span.start < at_unit {
            continue;
        }
        out.extend_from_slice(&units[at_unit..span.start]);
        out.extend(text.encode_utf16());
        at_unit = span.end();
    }
    out.extend_from_slice(&units[at_unit..]);
    let rewritten = String::from_utf16(&out).ok()?;
    rewritten.strip_prefix('=').map(str::to_owned)
}

/// How a defined name's reference site holds its coordinates on the axis an edit edits: every one
/// absolute (the site moves as a cell formula's would), every one relative (an offset from the
/// cell that uses the name, which no edit changes: the site keeps its text), or a range with one
/// of each.
enum NameEnds {
    Absolute,
    Relative,
    Mixed,
}

/// One end of a reference as written, split into its column (`$` and letters) and row (`$` and
/// digits), either of which may be absent.
struct End<'t> {
    column_absolute: bool,
    letters: &'t str,
    row_absolute: bool,
    digits: &'t str,
}

fn parse_end(end: &str) -> End<'_> {
    let column_absolute = end.starts_with('$') && end[1..].starts_with(|c: char| c.is_ascii_alphabetic());
    let rest = if column_absolute { &end[1..] } else { end };
    let letters_len = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_alphabetic()).len();
    let (letters, rest) = rest.split_at(letters_len);
    let row_absolute = rest.starts_with('$');
    let digits = rest.trim_start_matches('$');
    End {
        column_absolute,
        letters,
        row_absolute,
        digits,
    }
}

impl End<'_> {
    /// Whether this end holds a coordinate on `axis`, and whether it is absolute.
    fn on(&self, axis: EditAxis) -> Option<bool> {
        match axis {
            EditAxis::Rows => (!self.digits.is_empty()).then_some(self.row_absolute),
            EditAxis::Columns => (!self.letters.is_empty()).then_some(self.column_absolute),
        }
    }
}

fn name_ends(site: &ReferenceSite, units: &[u16], edit: &StructuralEdit) -> NameEnds {
    let axis = match edit {
        StructuralEdit::Insert { axis, .. } | StructuralEdit::Delete { axis, .. } => *axis,
        _ => return NameEnds::Absolute,
    };
    let Ok(text) = String::from_utf16(&units[site.span.start..site.span.end()]) else {
        return NameEnds::Relative;
    };
    let (_, body) = split_qualifier(&text);
    let kinds: Vec<bool> = split_range(body)
        .iter()
        .filter_map(|end| parse_end(split_end(end).0).on(axis))
        .collect();
    if kinds.iter().all(|absolute| *absolute) {
        NameEnds::Absolute
    } else if kinds.iter().all(|absolute| !*absolute) {
        NameEnds::Relative
    } else {
        NameEnds::Mixed
    }
}

/// A defined name's range one of whose ends is absolute on the edited axis and the other
/// relative: the absolute coordinate moves with the edit, the relative one keeps its text, and
/// an absolute coordinate whose row or column the edit deletes makes the reference #REF!. Excel
/// 16.0.20430 (2026-10-25, dc-10y), names held as `R1C1:R[2]C1`, `RC1:R3C1`, `R1C1:R1C[1]` and
/// `R1C:R1C2`: inserting row 1 gave `R2C1:R[2]C1`, `RC1:R4C1`, `R2C1:R2C[1]` and `R2C:R2C2`;
/// inserting column A then `R2C2:R[2]C2`, `RC2:R4C2`, `R2C2:R2C[1]` and `R2C:R2C3`; deleting row 2
/// then #REF!, `RC2:R3C2`, #REF! and #REF!.
fn mixed_name_site(
    site: &ReferenceSite,
    units: &[u16],
    anchor: FormulaAnchor,
    edit: &StructuralEdit,
) -> Option<String> {
    let operand = site.binding.as_ref().ok()?;
    let (edited, axis) = match edit {
        StructuralEdit::Insert { sheet, axis, .. } | StructuralEdit::Delete { sheet, axis, .. } => {
            (*sheet, *axis)
        }
        _ => return None,
    };
    let sheet = match operand.sheet {
        SheetTarget::Anchor => anchor.sheet(),
        SheetTarget::Sheet(sheet) => sheet,
        SheetTarget::Deleted | SheetTarget::Span { .. } => return None,
    };
    if sheet != edited || site.this_row {
        return None;
    }
    let text = String::from_utf16(&units[site.span.start..site.span.end()]).ok()?;
    let (qualifier, body) = split_qualifier(&text);
    let mut written = Vec::new();
    for end in split_range(body) {
        let (end_text, end_qualifier) = split_end(end);
        let parsed = parse_end(end_text);
        let moved = match parsed.on(axis) {
            Some(true) => {
                let index = match axis {
                    EditAxis::Rows => parsed.digits.parse::<u32>().ok()?,
                    EditAxis::Columns => column_number(parsed.letters)?,
                };
                match edit.position(sheet, axis, index) {
                    Some(index) => Some(index),
                    None => return Some(format!("{qualifier}#REF!")),
                }
            }
            _ => None,
        };
        let column = if parsed.column_absolute { "$" } else { "" };
        let row = if parsed.row_absolute { "$" } else { "" };
        let (letters, digits) = match (axis, moved) {
            (EditAxis::Rows, Some(index)) => (parsed.letters.to_owned(), index.to_string()),
            (EditAxis::Columns, Some(index)) => (column_letters(index), parsed.digits.to_owned()),
            (_, None) => (parsed.letters.to_owned(), parsed.digits.to_owned()),
        };
        let column = if letters.is_empty() { "" } else { column };
        let row = if digits.is_empty() { "" } else { row };
        written.push(format!("{end_qualifier}{column}{letters}{row}{digits}"));
    }
    let rewritten = format!("{qualifier}{}", written.join(":"));
    (rewritten != text).then_some(rewritten)
}

/// The column number of letters (`A` 1, `AB` 28), case ignored; `None` past the grid.
fn column_number(letters: &str) -> Option<u32> {
    let mut number: u32 = 0;
    for c in letters.chars() {
        let digit = u32::from(c.to_ascii_uppercase()) - u32::from('A') + 1;
        number = number.checked_mul(26)?.checked_add(digit)?;
    }
    (1..=MAX_COLUMNS).contains(&number).then_some(number)
}

/// The text a reference site takes after a row or column edit, `None` when it keeps its own.
fn axis_site(
    site: &ReferenceSite,
    units: &[u16],
    anchor: FormulaAnchor,
    edit: &StructuralEdit,
) -> Option<String> {
    let operand = site.binding.as_ref().ok()?;
    let (edited, axis) = match edit {
        StructuralEdit::Insert { sheet, axis, .. } | StructuralEdit::Delete { sheet, axis, .. } => {
            (*sheet, *axis)
        }
        _ => return None,
    };
    let sheet = match operand.sheet {
        SheetTarget::Anchor => anchor.sheet(),
        SheetTarget::Sheet(sheet) => sheet,
        SheetTarget::Deleted | SheetTarget::Span { .. } => return None,
    };
    if sheet != edited || site.this_row {
        return None;
    }
    let placed = operand.instantiate(anchor).ok()?;
    let text = String::from_utf16(&units[site.span.start..site.span.end()]).ok()?;
    let (qualifier, body) = split_qualifier(&text);
    let ends: Vec<&str> = split_range(body);
    let deleted = || format!("{qualifier}#REF!");
    match operand.shape {
        ReferenceShape::Deleted => None,
        ReferenceShape::Cell(_) => {
            let moved = edit.cell(placed.first());
            let Some(moved) = moved else {
                return Some(deleted());
            };
            if moved == placed.first() {
                return None;
            }
            let [end] = ends[..] else { return None };
            Some(format!(
                "{qualifier}{}",
                cell_text(end, moved.row(), moved.column())
            ))
        }
        ReferenceShape::Area { .. } => {
            let Some(moved) = edit.area(placed) else {
                return Some(deleted());
            };
            if moved == placed {
                return None;
            }
            let [first, last] = ends[..] else { return None };
            let (first_text, first_qualifier) = split_end(first);
            let (last_text, last_qualifier) = split_end(last);
            Some(format!(
                "{qualifier}{first_qualifier}{}:{last_qualifier}{}",
                cell_text(first_text, moved.first().row(), moved.first().column()),
                cell_text(last_text, moved.last().row(), moved.last().column())
            ))
        }
        ReferenceShape::Rows { .. } | ReferenceShape::Columns { .. } => {
            let rows = matches!(operand.shape, ReferenceShape::Rows { .. });
            if (axis == EditAxis::Rows) != rows {
                return None;
            }
            let Some(moved) = edit.area(placed) else {
                return Some(deleted());
            };
            if moved == placed {
                return None;
            }
            let (start, end) = if rows {
                (moved.first().row(), moved.last().row())
            } else {
                (moved.first().column(), moved.last().column())
            };
            let pieces: Vec<(&str, &str)> = ends.iter().map(|end| split_end(end)).collect();
            let render = |piece: &str, index: u32| {
                let dollar = if piece.starts_with('$') { "$" } else { "" };
                if rows {
                    format!("{dollar}{index}")
                } else {
                    format!("{dollar}{}", column_letters(index))
                }
            };
            match pieces[..] {
                [(only, q)] => Some(format!(
                    "{qualifier}{q}{}:{q}{}",
                    render(only, start),
                    render(only, end)
                )),
                [(a, qa), (b, qb)] => Some(format!(
                    "{qualifier}{qa}{}:{qb}{}",
                    render(a, start),
                    render(b, end)
                )),
                _ => None,
            }
        }
    }
}

/// The formula `text` (without `=`) written in `from` notation, rewritten in `to` notation for a
/// formula in the cell `at`: each reference the binder reads (a cell, a rectangle, whole rows or
/// columns, with any qualifier as written) spelled in the other notation, every other character
/// kept. A1 `$B$3` is R1C1 `R3C2`, `B3` in C5 is `R[-2]C[-1]`, `2:4` is `R2:R4`, `B:B` is `C2`; a
/// relative R1C1 offset that leaves the grid at `at` is `#REF!` in A1. `None` when the text does
/// not parse. Excel shows a formula in either notation (`Range.Formula`, `Range.FormulaR1C1`);
/// the document holds A1 text.
pub fn convert_notation(
    text: &str,
    at: CellKey,
    from: ReferenceNotation,
    to: ReferenceNotation,
    catalog: &dyn ReferenceCatalog,
) -> Option<String> {
    let source = format!("={text}");
    let tree = crate::parse_formula(&source);
    if !tree.diagnostics().is_empty() {
        return None;
    }
    if from == to {
        return Some(text.to_owned());
    }
    let units: Vec<u16> = source.encode_utf16().collect();
    let anchor = FormulaAnchor::new(at);
    let mut replacements: Vec<(TextSpan, String)> = Vec::new();
    for site in bind_references(&tree, from, anchor, catalog) {
        let Ok(operand) = &site.binding else {
            continue;
        };
        if matches!(operand.shape, ReferenceShape::Deleted) {
            continue;
        }
        let written = String::from_utf16(&units[site.span.start..site.span.end()]).ok()?;
        let (qualifier, _) = split_qualifier(&written);
        let body = match to {
            ReferenceNotation::R1C1 => r1c1_text(operand.shape),
            ReferenceNotation::A1 => a1_text(operand.shape, at),
        };
        replacements.push((site.span, format!("{qualifier}{body}")));
    }
    let mut out: Vec<u16> = Vec::with_capacity(units.len());
    let mut at_unit = 0;
    for (span, text) in replacements {
        out.extend_from_slice(&units[at_unit..span.start]);
        out.extend(text.encode_utf16());
        at_unit = span.end();
    }
    out.extend_from_slice(&units[at_unit..]);
    String::from_utf16(&out).ok()?.strip_prefix('=').map(str::to_owned)
}

/// One axis in R1C1: `R3`, `R`, `R[-2]` (with `letter` R or C).
fn r1c1_axis(letter: char, axis: AxisRef) -> String {
    match axis {
        AxisRef::Absolute(index) => format!("{letter}{index}"),
        AxisRef::Relative(0) => letter.to_string(),
        AxisRef::Relative(offset) => format!("{letter}[{offset}]"),
    }
}

fn r1c1_text(shape: ReferenceShape) -> String {
    let cell = |cell: super::CellRef| {
        format!("{}{}", r1c1_axis('R', cell.row), r1c1_axis('C', cell.column))
    };
    match shape {
        ReferenceShape::Cell(one) => cell(one),
        ReferenceShape::Area { first, last } => format!("{}:{}", cell(first), cell(last)),
        ReferenceShape::Rows { first, last } if first == last => r1c1_axis('R', first),
        ReferenceShape::Rows { first, last } => {
            format!("{}:{}", r1c1_axis('R', first), r1c1_axis('R', last))
        }
        ReferenceShape::Columns { first, last } if first == last => r1c1_axis('C', first),
        ReferenceShape::Columns { first, last } => {
            format!("{}:{}", r1c1_axis('C', first), r1c1_axis('C', last))
        }
        ReferenceShape::Deleted => "#REF!".to_owned(),
    }
}

/// One A1 row (`3`, `$3`) or column (`B`, `$B`) at `anchor`; `None` off the grid.
fn a1_axis(axis: AxisRef, anchor: u32, max: u32, column: bool) -> Option<String> {
    let (dollar, index) = match axis {
        AxisRef::Absolute(index) => ("$", i64::from(index)),
        AxisRef::Relative(offset) => ("", i64::from(anchor) + i64::from(offset)),
    };
    let index = u32::try_from(index).ok().filter(|i| (1..=max).contains(i))?;
    Some(if column {
        format!("{dollar}{}", column_letters(index))
    } else {
        format!("{dollar}{index}")
    })
}

fn a1_text(shape: ReferenceShape, at: CellKey) -> String {
    let row = |axis| a1_axis(axis, at.row(), MAX_ROWS, false);
    let column = |axis| a1_axis(axis, at.column(), MAX_COLUMNS, true);
    let cell = |cell: super::CellRef| Some(format!("{}{}", column(cell.column)?, row(cell.row)?));
    let text = match shape {
        ReferenceShape::Cell(one) => cell(one),
        ReferenceShape::Area { first, last } => {
            cell(first).and_then(|a| Some(format!("{a}:{}", cell(last)?)))
        }
        ReferenceShape::Rows { first, last } => {
            row(first).and_then(|a| Some(format!("{a}:{}", row(last)?)))
        }
        ReferenceShape::Columns { first, last } => {
            column(first).and_then(|a| Some(format!("{a}:{}", column(last)?)))
        }
        ReferenceShape::Deleted => None,
    };
    text.unwrap_or_else(|| "#REF!".to_owned())
}

/// A site's text split into its leading qualifier (`Sheet2!`, `'Q2 Data'!`, `#REF!` or nothing)
/// and the rest.
fn split_qualifier(text: &str) -> (&str, &str) {
    if let Some(rest) = text.strip_prefix('\'') {
        // A quoted qualifier: its closing quote is followed by `!`.
        let mut chars = rest.char_indices().peekable();
        while let Some((at, c)) = chars.next() {
            if c == '\'' {
                if matches!(chars.peek(), Some((_, '\''))) {
                    chars.next();
                    continue;
                }
                let end = 1 + at + 1;
                if text[end..].starts_with('!') {
                    return (&text[..end + 1], &text[end + 1..]);
                }
                return ("", text);
            }
        }
        return ("", text);
    }
    match text.find('!') {
        Some(bang) if !text[..bang].contains(':') => (&text[..bang + 1], &text[bang + 1..]),
        _ => ("", text),
    }
}

/// An endpoint split into its own text and the qualifier it repeats (`sheet2!D7` in
/// `Sheet2!B3:sheet2!D7`), returned as (text, qualifier).
fn split_end(end: &str) -> (&str, &str) {
    let (qualifier, rest) = split_qualifier(end);
    (rest, qualifier)
}

/// A range's two ends, or the one text of a cell, split at the `:` outside quotes.
fn split_range(text: &str) -> Vec<&str> {
    let mut quoted = false;
    for (at, c) in text.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            ':' if !quoted => return vec![&text[..at], &text[at + 1..]],
            _ => {}
        }
    }
    vec![text]
}

/// A cell written as `written` was (its `$`s kept) at `row` and `column`.
fn cell_text(written: &str, row: u32, column: u32) -> String {
    let column_absolute = written.starts_with('$');
    let after_letters = written
        .trim_start_matches('$')
        .trim_start_matches(|c: char| c.is_ascii_alphabetic());
    let row_absolute = after_letters.starts_with('$');
    format!(
        "{}{}{}{}",
        if column_absolute { "$" } else { "" },
        column_letters(column),
        if row_absolute { "$" } else { "" },
        row
    )
}

/// The letters of a 1-based column: 1 is A, 27 is AA.
pub(crate) fn column_letters(mut column: u32) -> String {
    let mut letters = Vec::new();
    while column > 0 {
        let rest = (column - 1) % 26;
        letters.push(char::from(b'A' + rest as u8));
        column = (column - 1) / 26;
    }
    letters.iter().rev().collect()
}

/// Shows `visit` each qualifier token naming one sheet in the tree under `root`: the qualifier of
/// one sheet (`Sheet2` in `Sheet2!A1`) and each end of a run of sheets (`Sheet1:Sheet3!A1`).
fn qualifier_tokens(root: &SyntaxNode, visit: &mut dyn FnMut(&Token)) {
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        match node.kind() {
            SyntaxKind::QualifiedReferenceExpr => {
                if let Some(SyntaxChild::Token(token)) = node.children().first()
                    && matches!(
                        token.kind(),
                        TokenKind::Identifier | TokenKind::QuotedIdentifier
                    )
                {
                    visit(token);
                }
            }
            SyntaxKind::SheetSpan3DReferenceExpr => {
                for child in node.children() {
                    if let SyntaxChild::Token(token) = child
                        && matches!(
                            token.kind(),
                            TokenKind::Identifier | TokenKind::QuotedIdentifier
                        )
                    {
                        visit(token);
                    }
                }
            }
            _ => {}
        }
        pending.extend(node.child_nodes());
    }
}

/// What removing a sheet leaves of a run of sheets.
enum SpanAfter {
    /// The removed sheet is no end of the run.
    Unchanged,
    /// The run's new ends, in the order they are written.
    Ends(String, String),
    /// The run held the removed sheet alone.
    Gone,
}

/// What removing `removed` leaves of the run from `first` to `last` (as written): an end that is
/// removed moves to the next sheet inward.
fn span_without(
    first: &str,
    last: &str,
    removed: SheetId,
    order: &[(SheetId, String)],
    catalog: &dyn ReferenceCatalog,
) -> SpanAfter {
    let position = |name: &str| {
        let id = catalog.sheet_named(name)?;
        order.iter().position(|(sheet, _)| *sheet == id)
    };
    let (Some(i), Some(j)) = (position(first), position(last)) else {
        return SpanAfter::Unchanged;
    };
    if order[i].0 != removed && order[j].0 != removed {
        return SpanAfter::Unchanged;
    }
    let (lo, hi) = (i.min(j), i.max(j));
    let kept: Vec<&String> = order[lo..=hi]
        .iter()
        .filter(|(sheet, _)| *sheet != removed)
        .map(|(_, name)| name)
        .collect();
    match (kept.first(), kept.last()) {
        (Some(low), Some(high)) => {
            let (a, b) = if i <= j { (low, high) } else { (high, low) };
            SpanAfter::Ends((*a).clone(), (*b).clone())
        }
        _ => SpanAfter::Gone,
    }
}

/// The two sheet names of a quoted run of sheets (`'Q1 Data:Q3 Data'`), when `token` is one.
fn quoted_span(token: &Token) -> Option<(String, String)> {
    if token.kind() != TokenKind::QuotedIdentifier {
        return None;
    }
    let name = unquoted(token.text());
    let (first, last) = name.split_once(':')?;
    Some((first.to_owned(), last.to_owned()))
}

/// A run of sheets as a qualifier writes it: `First:Last`, quoted whole when either name needs
/// quotes.
fn span_text(first: &str, last: &str) -> String {
    let plain = quoted_if_needed(first) == first && quoted_if_needed(last) == last;
    if plain {
        format!("{first}:{last}")
    } else {
        format!(
            "'{}:{}'",
            first.replace('\'', "''"),
            last.replace('\'', "''")
        )
    }
}

/// The replacements a sheet's removal makes: each qualifier naming it becomes `#REF!`, and a run
/// of sheets with it at an end loses that end to the next sheet inward (the whole run becomes
/// `#REF!` when it held that sheet alone).
fn sheet_removal(
    root: &SyntaxNode,
    removed: SheetId,
    order: &[(SheetId, String)],
    catalog: &dyn ReferenceCatalog,
    out: &mut Vec<(TextSpan, String)>,
) {
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        match node.kind() {
            SyntaxKind::QualifiedReferenceExpr => {
                if let [SyntaxChild::Token(token), SyntaxChild::Token(bang), ..] = node.children()
                    && matches!(
                        token.kind(),
                        TokenKind::Identifier | TokenKind::QuotedIdentifier
                    )
                {
                    if let Some((first, last)) = quoted_span(token) {
                        // A quoted run of sheets (`'Q1 Data:Q3 Data'!A1`).
                        match span_without(&first, &last, removed, order, catalog) {
                            SpanAfter::Unchanged => {}
                            SpanAfter::Ends(a, b) => out.push((token.span(), span_text(&a, &b))),
                            SpanAfter::Gone => out.push((
                                TextSpan::covering(token.span(), bang.span()),
                                "#REF!".to_owned(),
                            )),
                        }
                    } else if catalog.sheet_named(&unquoted(token.text())) == Some(removed) {
                        out.push((
                            TextSpan::covering(token.span(), bang.span()),
                            "#REF!".to_owned(),
                        ));
                    }
                }
            }
            SyntaxKind::SheetSpan3DReferenceExpr => {
                let tokens: Vec<&Token> = node
                    .children()
                    .iter()
                    .filter_map(|child| match child {
                        SyntaxChild::Token(token)
                            if matches!(
                                token.kind(),
                                TokenKind::Identifier
                                    | TokenKind::QuotedIdentifier
                                    | TokenKind::Bang
                            ) =>
                        {
                            Some(token)
                        }
                        _ => None,
                    })
                    .collect();
                if let [first, last, bang] = tokens[..] {
                    let position = |token: &Token| {
                        let id = catalog.sheet_named(&unquoted(token.text()))?;
                        order.iter().position(|(sheet, _)| *sheet == id)
                    };
                    if let (Some(i), Some(j)) = (position(first), position(last)) {
                        let (lo, hi) = (i.min(j), i.max(j));
                        let kept: Vec<&(SheetId, String)> = order[lo..=hi]
                            .iter()
                            .filter(|(sheet, _)| *sheet != removed)
                            .collect();
                        let touched = order[i].0 == removed || order[j].0 == removed;
                        match (kept.first(), kept.last()) {
                            _ if !touched => {}
                            (Some((_, low)), Some((_, high))) => {
                                // The run keeps its direction: the end written first stays first.
                                let (new_first, new_last) =
                                    if i <= j { (low, high) } else { (high, low) };
                                out.push((first.span(), quoted_if_needed(new_first)));
                                out.push((last.span(), quoted_if_needed(new_last)));
                            }
                            _ => out.push((
                                TextSpan::covering(first.span(), bang.span()),
                                "#REF!".to_owned(),
                            )),
                        }
                    }
                }
            }
            _ => {}
        }
        pending.extend(node.child_nodes());
    }
}

/// A qualifier token's sheet name, without its quotes and with each doubled quote one quote.
fn unquoted(text: &str) -> String {
    match text
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
    {
        Some(inner) => inner.replace("''", "'"),
        None => text.to_owned(),
    }
}

/// A sheet name as a qualifier writes it: quoted (each quote doubled) unless it is letters,
/// digits, `_` and `.` only, starts with a letter or `_`, and is spelled like no cell, row,
/// column or logical.
pub(crate) fn quoted_if_needed(name: &str) -> String {
    let plain = name
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        && notation::a1(name, 1, 1).is_none()
        && notation::r1c1(name).is_none()
        && !name.eq_ignore_ascii_case("TRUE")
        && !name.eq_ignore_ascii_case("FALSE");
    if plain {
        name.to_owned()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_func::value::IdAllocator;

    struct Sheets(Vec<(&'static str, SheetId)>);

    impl ReferenceCatalog for Sheets {
        fn sheet_named(&self, name: &str) -> Option<SheetId> {
            self.0
                .iter()
                .find(|(known, _)| known.eq_ignore_ascii_case(name))
                .map(|(_, sheet)| *sheet)
        }
    }

    fn fixture() -> (Sheets, SheetId, SheetId) {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let (one, two) = (ids.issue().unwrap(), ids.issue().unwrap());
        (Sheets(vec![("Sheet1", one), ("Sheet2", two)]), one, two)
    }

    fn rows(sheet: SheetId, change: &str, first: u32, count: u32) -> StructuralEdit {
        match change {
            "insert" => StructuralEdit::Insert {
                sheet,
                axis: EditAxis::Rows,
                first,
                count,
            },
            _ => StructuralEdit::Delete {
                sheet,
                axis: EditAxis::Rows,
                first,
                count,
            },
        }
    }

    /// Sheets F, Ja, Fe, Ma and Nm in that order, with their order known.
    struct Ordered(Vec<(&'static str, SheetId)>);

    impl ReferenceCatalog for Ordered {
        fn sheet_named(&self, name: &str) -> Option<SheetId> {
            self.0
                .iter()
                .find(|(known, _)| known.eq_ignore_ascii_case(name))
                .map(|(_, sheet)| *sheet)
        }

        fn sheet_span(&self, first: SheetId, last: SheetId) -> Option<Vec<SheetId>> {
            let at = |sheet| self.0.iter().position(|(_, held)| *held == sheet);
            let (a, b) = (at(first)?, at(last)?);
            Some(self.0[a.min(b)..=a.max(b)].iter().map(|(_, s)| *s).collect())
        }
    }

    fn ordered() -> (Ordered, Vec<SheetId>) {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let names = ["F", "Ja", "Fe", "Ma", "Nm"];
        let sheets: Vec<(&'static str, SheetId)> =
            names.iter().map(|name| (*name, ids.issue().unwrap())).collect();
        let ids = sheets.iter().map(|(_, id)| *id).collect();
        (Ordered(sheets), ids)
    }

    fn axis(sheet: SheetId, axis: EditAxis, insert: bool, first: u32) -> StructuralEdit {
        if insert {
            StructuralEdit::Insert {
                sheet,
                axis,
                first,
                count: 1,
            }
        } else {
            StructuralEdit::Delete {
                sheet,
                axis,
                first,
                count: 1,
            }
        }
    }

    /// Excel 16.0.20430 through COM, 2026-10-25 (module documentation): a 3-D reference keeps its
    /// text under a row or column edit of one of its sheets, and a defined name moves only its
    /// absolute coordinates.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn three_d_references_and_names_under_row_and_column_edits() {
        let (catalog, s) = ordered();
        let (f, ja, fe, ma, nm) = (s[0], s[1], s[2], s[3], s[4]);
        let at = CellKey::new(f, 1, 1).unwrap();
        for edit in [
            axis(fe, EditAxis::Rows, true, 1),
            axis(ja, EditAxis::Rows, true, 1),
            axis(ma, EditAxis::Rows, false, 2),
            axis(fe, EditAxis::Columns, true, 1),
        ] {
            for formula in [
                "SUM(Ja:Ma!A1:A3)",
                "SUM(Ja:Ma!B5)",
                "SUM(Ja:Fe!A2:B2)",
                "SUM(Fe:Ma!$A$4)",
            ] {
                assert_eq!(transform_formula(formula, at, &edit, &catalog), None, "{formula}");
            }
        }
        let column_a = axis(nm, EditAxis::Columns, true, 1);
        let row_1 = axis(nm, EditAxis::Rows, true, 1);
        let row_3_deleted = axis(nm, EditAxis::Rows, false, 3);
        for edit in [&column_a, &row_1, &row_3_deleted] {
            for name in ["Nm!XFD1", "Nm!A1048576"] {
                assert_eq!(transform_name(name, nm, edit, &catalog), None, "{name}");
            }
        }
        assert_eq!(
            transform_name("Nm!$A1", nm, &column_a, &catalog).as_deref(),
            Some("Nm!$B1")
        );
        assert_eq!(transform_name("Nm!$B1", nm, &row_1, &catalog), None);
        assert_eq!(
            transform_name("Nm!$A$1", nm, &column_a, &catalog).as_deref(),
            Some("Nm!$B$1")
        );
        assert_eq!(
            transform_name("Nm!$B$1", nm, &row_1, &catalog).as_deref(),
            Some("Nm!$B$2")
        );
        assert_eq!(transform_name("Nm!$B$2", nm, &row_3_deleted, &catalog), None);
        // Ranges with an absolute and a relative end on the edited axis (mixed_name_site): each
        // name through row 1 inserted, column A inserted and row 2 deleted, in turn.
        let row_2_deleted = axis(nm, EditAxis::Rows, false, 2);
        for steps in [
            ["Nm!$A$1:$A3", "Nm!$A$2:$A3", "Nm!$B$2:$B3", "Nm!#REF!"],
            ["Nm!$A1:$A$3", "Nm!$A1:$A$4", "Nm!$B1:$B$4", "Nm!$B1:$B$3"],
            ["Nm!$A$1:B$1", "Nm!$A$2:B$2", "Nm!$B$2:B$2", "Nm!#REF!"],
            ["Nm!A$1:$B$1", "Nm!A$2:$B$2", "Nm!A$2:$C$2", "Nm!#REF!"],
        ] {
            for (edit, (before, after)) in [&row_1, &column_a, &row_2_deleted]
                .into_iter()
                .zip(steps.iter().zip(steps.iter().skip(1)))
            {
                let got = transform_name(before, nm, edit, &catalog);
                assert_eq!(got.as_deref().unwrap_or(before), *after, "{before}");
            }
        }
    }

    /// Hand-derived from Excel's documented adjustment rules (module documentation).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rows_inserted_and_deleted_move_and_resize_references() {
        let (catalog, one, two) = fixture();
        let at = CellKey::new(one, 10, 5).unwrap();
        let t = |text: &str, edit: &StructuralEdit| {
            transform_formula(text, at, edit, &catalog).unwrap_or_else(|| text.to_owned())
        };
        let insert = rows(one, "insert", 3, 2);
        assert_eq!(t("B3+$B$2+B$4", &insert), "B5+$B$2+B$6");
        assert_eq!(t("SUM(A1:A5)", &insert), "SUM(A1:A7)");
        assert_eq!(t("SUM(A3:A5)", &insert), "SUM(A5:A7)");
        assert_eq!(t("SUM(A1:A2)", &insert), "SUM(A1:A2)");
        assert_eq!(t("SUM(2:4)", &insert), "SUM(2:6)");
        assert_eq!(t("SUM(B:B)", &insert), "SUM(B:B)");
        assert_eq!(t("Sheet2!B3+B3", &insert), "Sheet2!B3+B5");
        assert_eq!(t("Sheet1!B3:Sheet1!B4", &insert), "Sheet1!B5:Sheet1!B6");
        assert_eq!(t("A1048575", &insert), "#REF!");
        assert_eq!(t("SUM(A1:A1048576)", &insert), "SUM(A1:A1048576)");
        let delete = rows(one, "delete", 3, 2);
        assert_eq!(t("B3+B5+B2", &delete), "#REF!+B3+B2");
        assert_eq!(t("SUM(A1:A5)", &delete), "SUM(A1:A3)");
        assert_eq!(t("SUM(A3:A4)", &delete), "SUM(#REF!)");
        assert_eq!(t("SUM(A4:A6)", &delete), "SUM(A3:A4)");
        assert_eq!(t("Sheet1!B3*2", &delete), "Sheet1!#REF!*2");
        assert_eq!(t("SUM(3:4)", &delete), "SUM(#REF!)");
        let on_two = rows(two, "insert", 1, 1);
        assert_eq!(t("B3+Sheet2!B3", &on_two), "B3+Sheet2!B4");
        let columns = StructuralEdit::Insert {
            sheet: one,
            axis: EditAxis::Columns,
            first: 2,
            count: 1,
        };
        assert_eq!(t("SUM(A1:C1)+$B$1+D:D", &columns), "SUM(A1:D1)+$C$1+E:E");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn sheets_renamed_and_removed() {
        let (catalog, one, two) = fixture();
        let at = CellKey::new(one, 1, 1).unwrap();
        let rename = StructuralEdit::RenameSheet {
            sheet: two,
            name: "Q2 Data".to_owned(),
        };
        assert_eq!(
            transform_formula("Sheet2!A1+'Sheet2'!B2+A1", at, &rename, &catalog).as_deref(),
            Some("'Q2 Data'!A1+'Q2 Data'!B2+A1")
        );
        let remove = StructuralEdit::RemoveSheet {
            sheet: two,
            order: vec![(one, "Sheet1".to_owned()), (two, "Sheet2".to_owned())],
        };
        assert_eq!(
            transform_formula("Sheet2!A1+SUM(Sheet2!A1:B2)+A1", at, &remove, &catalog).as_deref(),
            Some("#REF!A1+SUM(#REF!A1:B2)+A1")
        );
        assert_eq!(quoted_if_needed("Data"), "Data");
        assert_eq!(quoted_if_needed("A1"), "'A1'");
        assert_eq!(quoted_if_needed("It's"), "'It''s'");
        assert_eq!(column_letters(28), "AB");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    /// Measured: Excel 16.0 through COM on 2026-10-23, each formula set in Sheet1!C5 through
    /// `Range.Formula` and read back through `Range.FormulaR1C1`, and the reverse.
    fn formulas_convert_between_a1_and_r1c1() {
        use ReferenceNotation::{A1, R1C1};
        let (catalog, one, _) = fixture();
        let at = CellKey::new(one, 5, 3).unwrap();
        let to_r1c1 = |text: &str| convert_notation(text, at, A1, R1C1, &catalog).unwrap();
        let to_a1 = |text: &str| convert_notation(text, at, R1C1, A1, &catalog).unwrap();
        assert_eq!(to_r1c1("$B$3+B3+C5"), "R3C2+R[-2]C[-1]+RC");
        assert_eq!(to_r1c1("SUM(Sheet2!A1:B2)*$A1"), "SUM(Sheet2!R[-4]C[-2]:R[-3]C[-1])*R[-4]C1");
        assert_eq!(
            to_r1c1("SUM(2:4)+SUM(B:B)+SUM($B:$D)"),
            "SUM(R[-3]:R[-1])+SUM(C[-1])+SUM(C2:C4)"
        );
        assert_eq!(to_r1c1("\"B3\"&TaxRate"), "\"B3\"&TaxRate");
        assert_eq!(to_a1("R3C2+R[-2]C[-1]+RC"), "$B$3+B3+C5");
        assert_eq!(to_r1c1("SUM(C:E)+$A$1:B9"), "SUM(C:C[2])+R1C1:R[4]C[-1]");
        assert_eq!(to_a1("SUM(R2:R4)+SUM(C2)"), "SUM($2:$4)+SUM($B:$B)");
        // Not measured: an offset that leaves the grid.
        assert_eq!(to_a1("R[-10]C"), "#REF!");
        for text in ["$B$3*2+B3", "SUM(Sheet2!A1:B2)", "SUM(C:E)+$A$1:B9"] {
            assert_eq!(to_a1(&to_r1c1(text)), text, "{text}");
        }
    }
}
