//! Binding reference text, as INDIRECT reads it, against a catalogue and the calling formula's
//! cell.
//!
//! The text is one reference: an optional sheet qualifier and then a cell, or two endpoints of
//! one form joined by `:` (`B3:D7`, `2:4`, `B:D` in A1 notation; `R3C2:R4C4`, `R2:R4`, `C2` in
//! R1C1 notation, where a lone row or column is a reference). Each endpoint is spelled, and the
//! two of a range ordered, exactly as the formula binder spells and orders a reference token
//! (the `notation` module and `bind::pair`), and the qualifier's sheet is asked of the same
//! [`ReferenceCatalog`]. Its grammar is INDIRECT's, which Excel 16.0.20430 showed to differ from a
//! formula's in these ways (bead dc-51ka's oracle workbook holds a row for each example named):
//!
//! - spaces may follow the text, follow the qualifier's `!`, quoted or not (`data! A1`,
//!   `'data'! A1`), and stand on either side of the `:`, but may not lead it; a tab, a carriage
//!   return or a no-break space leading the text, following it, following the `!` or on either side
//!   of the `:` names no reference, so none of the three is read as a space or dropped, and each is
//!   a character that no reference token or name holds;
//! - an unquoted sheet name is made of letters, digits, `_` and `.`, does not start with `.` and
//!   is not TRUE or FALSE (`1x!A1` and `A1!B2` name the sheets 1x and A1); its letters may be
//!   non-ASCII (é, ß, Ø, ñ, Ω, Ж and 中 were each accepted; the binder accepts every character
//!   Unicode calls alphabetic). A name holding any other printable ASCII character, or `€`, names no
//!   reference unquoted; each of the 29 such characters was shown in a name of a sheet the workbook
//!   holds where a sheet name may hold it (`Q2 Data!C3`, `a(b!A1`, `a"b!A1`, ...), and `*`, `/`, `?`
//!   and `\`, which none may, in a name of no sheet. Such a name must be quoted (`'Q2 Data'!C3`,
//!   `'It''s'!A1`), and a quoted name is followed at once by its `!` (`'data' !A1` names none);
//! - only the first endpoint carries the qualifier: `Sheet2!A1:Sheet2!B2` names no reference;
//! - an operator, a parenthesis, a second range, a run of sheets (`Sheet1:Sheet2!A1`) and
//!   `#REF!` name no reference;
//! - a relative R1C1 coordinate is an offset from the calling formula's cell that wraps around
//!   the grid's edge, each corner of a range on its own (`R[-1]C` in row 1 names the last row,
//!   `R[1048575]C` in row 5 names row 4, and `R[-6]C1:R[-4]C2` in row 6 names A2:B1048576), while
//!   an offset of the grid's size or more names no reference.
//!
//! Text that names no reference is [`TextReference::Invalid`], INDIRECT's #REF!: text of neither
//! a reference's nor a defined name's form, a coordinate outside the grid that no name can spell
//! (`$A$1048577`, `R0C1`, `1:1048577`), an unknown sheet. An unknown sheet leaves no trace: no area
//! is resolved, so nothing records that the text would name a reference once that sheet exists
//! (work package dc-mzk, whose absent-target case owns it). Text whose reference is not modelled is
//! [`TextReference::NotModelled`]:
//!
//! - text that could be a defined name, which INDIRECT resolves when the workbook defines it
//!   (Excel resolved `Rate` and `\x`, which the oracle workbook defines); names are not bound yet,
//!   work package dc-mzk;
//! - a qualifier naming another workbook (`[Book2]Sheet1!A1`, bead cfml-m3.5);
//! - a line feed, which Excel read as a space after the text (`A1` and a line feed is A1), and
//!   every control character but tab and carriage return, whose reading is not measured (bead
//!   dc-ea6r);
//! - an unquoted sheet name holding a tab or a carriage return (bead dc-ea6r), or a non-ASCII
//!   character that is neither a letter nor `€` (work package dc-mzk), whose reading is not measured;
//! - text that is not UTF-16 throughout.

use super::bind::pair;
use super::notation::{self, Spelled};
use super::{AxisRef, FormulaAnchor, ReferenceCatalog, ReferenceNotation, ReferenceShape};
use calc_func::TextReference;
use calc_func::value::{CellKey, GridArea, MAX_COLUMNS, MAX_ROWS, SheetId};

/// Whether `c`, in an unquoted sheet name, makes INDIRECT's text name no reference: a printable
/// ASCII character other than a letter, a digit, `_`, `.` and the brackets (which mark another
/// workbook), and `€`; each was shown in the oracle workbook (see the module documentation).
fn not_in_unquoted_name(c: char) -> bool {
    ((c.is_ascii_graphic() || c == ' ')
        && !c.is_ascii_alphanumeric()
        && !matches!(c, '_' | '.' | '[' | ']'))
        || c == '\u{20ac}'
}

/// Whether `c` may stand in an unquoted sheet name: an ASCII letter or digit, `_`, `.`, and a
/// non-ASCII letter.
fn in_unquoted_name(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || (!c.is_ascii() && c.is_alphabetic())
}

/// The reference `text` (UTF-16 code units) names, read in `notation` for a formula anchored at
/// `anchor`, its sheet qualifier resolved through `catalog`. See the module documentation.
pub(crate) fn bind_text(
    text: &[u16],
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
) -> TextReference {
    let Ok(text) = String::from_utf16(text) else {
        return TextReference::NotModelled;
    };
    // A tab and a carriage return are characters no reference or name holds; a line feed and the
    // other control characters are not modelled (bead dc-ea6r).
    if text
        .chars()
        .any(|c| c.is_control() && c != '\t' && c != '\r')
    {
        return TextReference::NotModelled;
    }
    let text = text.trim_end_matches(' ');
    let (qualifier, rest) = match split_qualifier(text) {
        Ok(split) => split,
        Err(outcome) => return outcome,
    };
    let target = match target(rest, notation, anchor) {
        Ok(target) => target,
        Err(outcome) => return outcome,
    };
    let sheet = match qualifier {
        None => anchor.sheet(),
        Some(name) => match catalog.sheet_named(&name) {
            Some(sheet) => sheet,
            // No area, and so no footprint: creating the sheet later re-dirties nothing (bead
            // dc-mzk).
            None => return TextReference::Invalid,
        },
    };
    match target {
        Target::Shape(shape) => TextReference::Area(placed(shape, sheet, anchor.cell())),
        Target::Name => TextReference::NotModelled,
    }
}

/// What the text after its qualifier names before its sheet is known.
enum Target {
    Shape(ReferenceShape),
    /// Text that could be a defined name.
    Name,
}

/// The text's sheet name, unquoted, when it has a qualifier, and the text after the qualifier,
/// without the spaces that may follow its `!`; or the outcome when the qualifier already decides
/// it.
fn split_qualifier(text: &str) -> Result<(Option<String>, &str), TextReference> {
    if text.starts_with('[') {
        return Err(TextReference::NotModelled);
    }
    if let Some(quoted) = text.strip_prefix('\'') {
        let (name, after) = quoted_name(quoted).ok_or(TextReference::Invalid)?;
        let rest = after.strip_prefix('!').ok_or(TextReference::Invalid)?;
        if name.contains(['[', ']']) {
            return Err(TextReference::NotModelled);
        }
        return Ok((Some(name), rest.trim_start_matches(' ')));
    }
    let Some(bang) = text.find('!') else {
        return Ok((None, text));
    };
    let (name, rest) = (&text[..bang], &text[bang + 1..]);
    if name.contains(['[', ']']) {
        return Err(TextReference::NotModelled);
    }
    if name.is_empty()
        || name.starts_with('.')
        || name.eq_ignore_ascii_case("TRUE")
        || name.eq_ignore_ascii_case("FALSE")
        || name.contains(not_in_unquoted_name)
    {
        return Err(TextReference::Invalid);
    }
    // A tab, a carriage return (bead dc-ea6r) or a non-ASCII character that is neither a letter
    // nor `€` (work package dc-mzk): not measured in an unquoted name.
    if !name.chars().all(in_unquoted_name) {
        return Err(TextReference::NotModelled);
    }
    Ok((Some(name.to_owned()), rest.trim_start_matches(' ')))
}

/// The name inside a quoted qualifier, each doubled quote one quote, and the text after its
/// closing quote; `None` when the quote is not closed.
fn quoted_name(text: &str) -> Option<(String, &str)> {
    let mut name = String::new();
    let mut chars = text.char_indices();
    while let Some((at, c)) = chars.next() {
        if c != '\'' {
            name.push(c);
            continue;
        }
        if text[at + 1..].starts_with('\'') {
            name.push('\'');
            chars.next();
            continue;
        }
        return Some((name, &text[at + 1..]));
    }
    None
}

/// What `text`, the part after any qualifier, names in `notation` at `anchor`: one endpoint, or
/// two of one form joined by a `:` with spaces on either side.
fn target(
    text: &str,
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
) -> Result<Target, TextReference> {
    if text.starts_with(' ') {
        return Err(TextReference::Invalid);
    }
    let spell = |token: &str| match notation {
        ReferenceNotation::A1 => {
            let at = anchor.cell();
            notation::a1(token, at.row(), at.column())
        }
        ReferenceNotation::R1C1 => notation::r1c1(token),
    };
    let mut parts = text.split(':');
    let first = parts.next().unwrap_or_default().trim_matches(' ');
    let Some(last) = parts.next() else {
        return match spell(first) {
            Some(Spelled::Cell(cell)) => Ok(Target::Shape(ReferenceShape::Cell(cell))),
            Some(Spelled::Row(row)) if notation == ReferenceNotation::R1C1 => {
                Ok(Target::Shape(ReferenceShape::Rows {
                    first: row,
                    last: row,
                }))
            }
            Some(Spelled::Column(column)) if notation == ReferenceNotation::R1C1 => {
                Ok(Target::Shape(ReferenceShape::Columns {
                    first: column,
                    last: column,
                }))
            }
            Some(Spelled::OutsideGrid(_)) => Err(TextReference::Invalid),
            // A lone A1 row or column is no reference, but a column's letters can be a name.
            Some(Spelled::Row(_) | Spelled::Column(_)) | None => unspelled(first, notation),
        };
    };
    if parts.next().is_some() {
        return Err(TextReference::Invalid);
    }
    let last = last.trim_matches(' ');
    let (first_spelled, last_spelled) = (spell(first), spell(last));
    match (first_spelled, last_spelled) {
        (Some(first), Some(last)) if first.form() == last.form() => pair(first, last)
            .map(Target::Shape)
            .map_err(|_| TextReference::Invalid),
        // `A1:B` and `A1:Rate` are ranges only if B or Rate name a cell, which is not known here.
        _ if could_be_name(first, first_spelled, notation)
            || could_be_name(last, last_spelled, notation) =>
        {
            Err(TextReference::NotModelled)
        }
        _ => Err(TextReference::Invalid),
    }
}

/// Whether a range's endpoint `token`, spelled `spelled`, could be a defined name: a cell, an R1C1
/// row or column and a coordinate outside the grid cannot.
fn could_be_name(token: &str, spelled: Option<Spelled>, notation: ReferenceNotation) -> bool {
    match spelled {
        Some(Spelled::Cell(_) | Spelled::OutsideGrid(_)) => false,
        Some(Spelled::Row(_) | Spelled::Column(_)) if notation == ReferenceNotation::R1C1 => false,
        Some(Spelled::Row(_) | Spelled::Column(_)) | None => {
            matches!(unspelled(token, notation), Ok(Target::Name))
        }
    }
}

/// A token that is no reference by itself in `notation`: [`Target::Name`] when it could be a
/// defined name, else no reference. Text that looks like a cell outside the grid (`XFE1`,
/// `A1048577`, `A0`) or like a column's letters (`XFE`) has a name's characters and is taken as a
/// possible name; an R1C1 reference (`R1`, `R1C1`) and an A1 cell on the grid are not names
/// (Microsoft's rules for names: a name cannot be the same as a cell reference), nor are TRUE and
/// FALSE in any case, which a formula reads as logicals.
fn unspelled(token: &str, notation: ReferenceNotation) -> Result<Target, TextReference> {
    let reference = match notation {
        ReferenceNotation::A1 => notation::r1c1(token).is_some(),
        ReferenceNotation::R1C1 => matches!(notation::a1(token, 1, 1), Some(Spelled::Cell(_))),
    };
    if reference
        || token.eq_ignore_ascii_case("TRUE")
        || token.eq_ignore_ascii_case("FALSE")
        || !name_form(token)
    {
        return Err(TextReference::Invalid);
    }
    Ok(Target::Name)
}

/// Whether `token` has the characters of a defined name: a letter, `_` or `\` first, then
/// letters, digits, `_`, `.`, `?` and `\` (Excel resolved `\x` through the name the oracle
/// workbook defines).
fn name_form(token: &str) -> bool {
    let mut chars = token.chars();
    chars
        .next()
        .is_some_and(|c| c.is_alphabetic() || matches!(c, '_' | '\\'))
        && chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '?' | '\\'))
}

/// The area `shape` names on `sheet` for a formula at `at`, each relative coordinate an offset
/// from `at`'s that wraps around the grid's edge. A shape here has no axis outside the grid: the
/// spellers refuse an absolute coordinate off it and an offset of the grid's size or more.
fn placed(shape: ReferenceShape, sheet: SheetId, at: CellKey) -> GridArea {
    let row = |axis| wrapped(axis, at.row(), MAX_ROWS);
    let column = |axis| wrapped(axis, at.column(), MAX_COLUMNS);
    let (top, left, bottom, right) = match shape {
        ReferenceShape::Cell(cell) => {
            let (r, c) = (row(cell.row), column(cell.column));
            (r, c, r, c)
        }
        ReferenceShape::Area { first, last } => (
            row(first.row),
            column(first.column),
            row(last.row),
            column(last.column),
        ),
        ReferenceShape::Rows { first, last } => (row(first), 1, row(last), MAX_COLUMNS),
        ReferenceShape::Columns { first, last } => (1, column(first), MAX_ROWS, column(last)),
        ReferenceShape::Deleted => unreachable!("reference text never spells a deleted reference"),
    };
    let first = CellKey::new(sheet, top, left).expect("a wrapped coordinate lies on the grid");
    let last = CellKey::new(sheet, bottom, right).expect("a wrapped coordinate lies on the grid");
    GridArea::new(first, last).expect("both corners lie on one sheet")
}

/// The 1-based index `axis` names when the anchor's is `anchor`, on an axis of `max` indexes, a
/// relative offset wrapping around the edge.
fn wrapped(axis: AxisRef, anchor: u32, max: u32) -> u32 {
    match axis {
        AxisRef::Absolute(index) => index,
        AxisRef::Relative(offset) => {
            let zero_based = (i64::from(anchor) - 1 + i64::from(offset)).rem_euclid(i64::from(max));
            u32::try_from(zero_based + 1).expect("an index below the axis's size")
        }
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
                .find(|(known, _)| known.to_lowercase() == name.to_lowercase())
                .map(|(_, sheet)| *sheet)
        }
    }

    /// The sheets of the tests, and the anchor D10 on the first.
    fn fixture() -> (Sheets, Vec<SheetId>, FormulaAnchor) {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let names = [
            "here", "data", "Q2 Data", "It's", "1x", "A1", "a.b", "TRUE", "\u{e9}",
        ];
        let sheets: Vec<(&'static str, SheetId)> = names
            .iter()
            .map(|name| (*name, ids.issue().unwrap()))
            .collect();
        let ids: Vec<SheetId> = sheets.iter().map(|(_, id)| *id).collect();
        let anchor = FormulaAnchor::new(CellKey::new(ids[0], 10, 4).unwrap());
        (Sheets(sheets), ids, anchor)
    }

    fn bound(text: &str, notation: ReferenceNotation) -> TextReference {
        let (catalog, _, anchor) = fixture();
        let units: Vec<u16> = text.encode_utf16().collect();
        bind_text(&units, notation, anchor, &catalog)
    }

    fn area(sheet: SheetId, top: u32, left: u32, bottom: u32, right: u32) -> TextReference {
        TextReference::Area(
            GridArea::new(
                CellKey::new(sheet, top, left).unwrap(),
                CellKey::new(sheet, bottom, right).unwrap(),
            )
            .unwrap(),
        )
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a1_text() {
        use ReferenceNotation::A1;
        let (_, s, _) = fixture();
        let (here, data) = (s[0], s[1]);
        for (text, expected) in [
            ("A1", area(here, 1, 1, 1, 1)),
            ("$a$1", area(here, 1, 1, 1, 1)),
            ("A01", area(here, 1, 1, 1, 1)),
            ("A1   ", area(here, 1, 1, 1, 1)),
            (
                "XFD1048576",
                area(here, MAX_ROWS, MAX_COLUMNS, MAX_ROWS, MAX_COLUMNS),
            ),
            ("B2:A1", area(here, 1, 1, 2, 2)),
            ("$A1 : B$2", area(here, 1, 1, 2, 2)),
            ("A:A", area(here, 1, 1, MAX_ROWS, 1)),
            ("2:1", area(here, 1, 1, 2, MAX_COLUMNS)),
            ("data!B2", area(data, 2, 2, 2, 2)),
            ("DATA!  b2:c3 ", area(data, 2, 2, 3, 3)),
            ("'data'!A1", area(data, 1, 1, 1, 1)),
            ("'data'! A1", area(data, 1, 1, 1, 1)),
            ("'Q2 Data'!C3", area(s[2], 3, 3, 3, 3)),
            ("'It''s'!A1", area(s[3], 1, 1, 1, 1)),
            ("1x!A1", area(s[4], 1, 1, 1, 1)),
            ("A1!B2", area(s[5], 2, 2, 2, 2)),
            ("a.b!A1", area(s[6], 1, 1, 1, 1)),
            ("'TRUE'!A1", area(s[7], 1, 1, 1, 1)),
            ("\u{e9}!A1", area(s[8], 1, 1, 1, 1)),
        ] {
            assert_eq!(bound(text, A1), expected, "{text:?}");
        }
        for text in [
            "",
            " ",
            " A1",
            "=A1",
            "(A1)",
            "A1+1",
            "A1 B1",
            "A1,B2",
            "A1:B2:C3",
            "$A$1:$A",
            "$A",
            "1",
            "$A$1048577",
            "$XFE$1",
            "1:1048577",
            "R1C1",
            "R",
            "TRUE",
            "true",
            "$$A1",
            "A1$",
            "A1!",
            "!A1",
            "data!",
            "data! ",
            "data!A1:data!B2",
            "A1:data!B2",
            "A1:'data'!B2",
            "'data!A1",
            "'data'A1",
            "'data' !A1",
            "data !A1",
            " data!A1",
            "''data''!A1",
            "Q2 Data!C3",
            "It's!A1",
            "a-b!A1",
            ".x!A1",
            "TRUE!A1",
            "true!A1",
            "nosuch!A1",
            "here:data!A1",
            "#REF!",
            "#REF!A1",
            "data!#REF!",
            "a@b!A1",
            "a}b!A1",
            "a*b!A1",
            "a\\b!A1",
            "\u{20ac}!A1",
            "a\u{20ac}!A1",
            "A1\t",
            "\tA1",
            "data!\tA1",
            "A1\t:B2",
            "A1:\tB2",
            "A1\r",
            "\rA1",
            "data!\rA1",
            "A1\r:B2",
            "A1:\rB2",
            "A1\u{a0}",
            "\u{a0}A1",
            "data!\u{a0}A1",
            "A1\u{a0}:B2",
            "A1:\u{a0}B2",
        ] {
            assert_eq!(bound(text, A1), TextReference::Invalid, "{text:?}");
        }
        // Text that could be a defined name (the binder takes XFE1, A1048577, A0 and XFE as possible
        // names, and \x, which Excel resolved through the name the oracle workbook defines), another
        // workbook, a control character other than tab and carriage return, and an unquoted sheet
        // name holding a tab, a carriage return or a non-ASCII character that is neither a letter
        // nor the euro sign.
        for text in [
            "NoSuchName",
            "data!Foo",
            "_x",
            "x?",
            "A1.0",
            "A",
            "XFE",
            "XFE1",
            "A1048577",
            "A0",
            "A1:Foo",
            "A1:XFE2",
            "A1:B",
            "A:B2",
            "[Book1]data!A1",
            "'[Book1]Q2 Data'!C3",
            "\\x",
            "\\",
            "_a\\b",
            "A1\n",
            "data!\nA1",
            "A1\u{1}",
            "A1\u{85}",
            "a\tb!A1",
            "a\rb!A1",
            "a\u{a0}b!A1",
            "a\u{2603}!A1",
        ] {
            assert_eq!(bound(text, A1), TextReference::NotModelled, "{text:?}");
        }
        // An unknown sheet names no reference, whatever follows its qualifier.
        assert_eq!(bound("nosuch!Foo", A1), TextReference::Invalid);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn r1c1_text() {
        use ReferenceNotation::R1C1;
        let (_, s, _) = fixture();
        let (here, data) = (s[0], s[1]);
        for (text, expected) in [
            ("R1C1", area(here, 1, 1, 1, 1)),
            ("r2c2 ", area(here, 2, 2, 2, 2)),
            ("R[-9]C[-3]", area(here, 1, 1, 1, 1)),
            ("RC", area(here, 10, 4, 10, 4)),
            ("R1", area(here, 1, 1, 1, MAX_COLUMNS)),
            ("C2", area(here, 1, 2, MAX_ROWS, 2)),
            ("R1:R2", area(here, 1, 1, 2, MAX_COLUMNS)),
            ("R2C1:R1C1", area(here, 1, 1, 2, 1)),
            ("data!R1C1", area(data, 1, 1, 1, 1)),
            ("data! R2C2:R4C2", area(data, 2, 2, 4, 2)),
            ("1x!R1C1", area(s[4], 1, 1, 1, 1)),
            // Offsets wrap around the grid's edge: row 10 - 10 is the last row, column 4 - 4 the
            // last column, and row 10 + 1048575 is row 9.
            ("R[-10]C1", area(here, MAX_ROWS, 1, MAX_ROWS, 1)),
            ("R1C[-4]", area(here, 1, MAX_COLUMNS, 1, MAX_COLUMNS)),
            ("R[1048575]C", area(here, 9, 4, 9, 4)),
            ("R[1048575]C[-2]", area(here, 9, 2, 9, 2)),
            ("R[-10]C1:R[-9]C1", area(here, 1, 1, MAX_ROWS, 1)),
            ("R[-10]C1:R[-8]C2", area(here, 2, 1, MAX_ROWS, 2)),
        ] {
            assert_eq!(bound(text, R1C1), expected, "{text:?}");
        }
        for text in [
            "",
            "A1",
            "$A$1",
            "R0C1",
            "R1C16385",
            "R[1048576]C",
            "R[-1048576]C",
            "R1C1:R2",
            " R1C1",
            "R1C1:R2C2:R3C3",
            "TRUE",
            "nosuch!R1C1",
        ] {
            assert_eq!(bound(text, R1C1), TextReference::Invalid, "{text:?}");
        }
        assert_eq!(bound("Foo", R1C1), TextReference::NotModelled);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn text_that_is_not_utf16_throughout_is_not_modelled() {
        let (catalog, _, anchor) = fixture();
        assert_eq!(
            bind_text(
                &[0x41, 0xD800, 0x31],
                ReferenceNotation::A1,
                anchor,
                &catalog
            ),
            TextReference::NotModelled
        );
    }
}
