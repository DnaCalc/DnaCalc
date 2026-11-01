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
//! Text that could be a defined name is looked up in the catalogue, from the formula's sheet or
//! from the qualifier's: a name whose formula is one reference with absolute coordinates names
//! that reference (Excel resolved `Rate` and `\x`, which the oracle workbook defines), and a name
//! the workbook does not define names none (`NoSuchName`, `data!Foo`, `XFE1`; and `A1:B`, a range
//! whose endpoint B is no name). A line feed after the text reads as a space (`A1` and a line
//! feed is A1).
//!
//! Text that names no reference is [`TextReference::Invalid`], INDIRECT's #REF!: text of neither
//! a reference's nor a defined name's form, a coordinate outside the grid that no name can spell
//! (`$A$1048577`, `R0C1`, `1:1048577`), an unknown sheet, a name the workbook does not define. An
//! unknown sheet leaves no trace: no area is resolved, so nothing records that the text would name
//! a reference once that sheet exists (work package dc-m3u owns it).
//! Text whose reference is not modelled is [`TextReference::NotModelled`]:
//!
//! - a defined name whose formula is not one absolute reference (a constant, a computed or a
//!   relative reference), a range through a defined name, and a name when the catalogue carries
//!   no names;
//! - a qualifier naming another workbook (`[Book2]Sheet1!A1`, bead dc-m3u);
//! - a line feed other than after the text, and every control character but tab and carriage
//!   return, whose reading is not measured (bead dc-m3u);
//! - an unquoted sheet name holding a tab or a carriage return (bead dc-m3u), or a non-ASCII
//!   character that is neither a letter nor `€` (work package dc-m3u), whose reading is not measured;
//! - text that is not UTF-16 throughout.

use super::bind::pair;
use super::notation::{self, Spelled};
use super::{
    AxisRef, FormulaAnchor, NameDefinition, NameScope, ReferenceCatalog, ReferenceNotation,
    ReferenceShape,
};
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
pub fn bind_text(
    text: &[u16],
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
) -> TextReference {
    let Ok(text) = String::from_utf16(text) else {
        return TextReference::NotModelled;
    };
    // Excel 16.0.20430 read a line feed after the text as a space (`A1` and a line feed is A1,
    // bead dc-51ka's oracle row); a line feed anywhere else is not measured.
    let text = text.trim_end_matches([' ', '\n']);
    bind_str(text, notation, anchor, anchor, catalog, 0, None)
}

/// How deep defined names may refer to other names in INDIRECT's text before the text is refused
/// as not modelled.
const MAX_NAME_DEPTH: usize = 16;

/// [`bind_text`] of decoded text, `depth` names deep, its relative coordinates spelled as offsets
/// from `base` and placed from `anchor` (the two differ for a defined name's formula, which a file
/// stores relative to A1).
fn bind_str(
    text: &str,
    notation: ReferenceNotation,
    base: FormulaAnchor,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
    depth: usize,
    parent: Option<calc_func::value::NameId>,
) -> TextReference {
    // A tab and a carriage return are characters no reference or name holds; a line feed and the
    // other control characters are not modelled (bead dc-m3u).
    if text
        .chars()
        .any(|c| c.is_control() && c != '\t' && c != '\r')
    {
        return TextReference::NotModelled;
    }
    let text = text.trim_end_matches(' ');
    let (qualifier, rest) = match split_qualifier(text, catalog) {
        Ok(split) => split,
        Err(outcome) => return outcome,
    };
    let target = match target(rest, notation, base) {
        Ok(target) => target,
        Err(outcome) => return outcome,
    };
    let qualified = qualifier.is_some();
    let sheet = match qualifier {
        None => anchor.sheet(),
        Some(name) => match catalog.sheet_named(&name) {
            Some(sheet) => sheet,
            // No area, and so no footprint: creating the sheet later re-dirties nothing (bead
            // dc-m3u).
            None => return TextReference::Invalid,
        },
    };
    let scope = if qualified {
        NameScope::Sheet(sheet)
    } else {
        NameScope::Formula(anchor.sheet())
    };
    match target {
        Target::Shape(shape) => TextReference::Area(placed(shape, sheet, anchor.cell())),
        Target::Name(name) => bind_name(&name, scope, anchor, catalog, depth, parent),
        // A range whose endpoint could be a name names a reference only when that name does:
        // with no such name defined it names none (`A1:B` is #REF!, Excel 16.0.20430, bead
        // dc-51ka's oracle row); a range through a defined name is not modelled.
        Target::NamedRange(names) => {
            if names.iter().all(|name| {
                let _ = catalog.name_binding(name, scope, parent);
                catalog.defined_name(name, scope) == NameDefinition::Unknown
            }) {
                TextReference::Invalid
            } else {
                TextReference::NotModelled
            }
        }
    }
}

/// The reference the defined name `name`, looked up from `scope`, names, as INDIRECT reads it:
/// none (#REF!) when the workbook defines no such name (Excel 16.0.20430: `NoSuchName`,
/// `data!Foo` and `XFE1` are #REF!, bead dc-51ka's oracle rows), and the area its formula names
/// when that formula is one reference with absolute coordinates, as a file stores a name's
/// references (`Rate` and `\x` resolved). A relative reference in the name is stored relative to
/// A1 and read from the calling cell, wrapping around the grid's edge (L3, dc-m3u; Excel 16.0,
/// 2026-10-23: `=INDIRECT("LeftCell")` in E3, LeftCell being `RC[-1]` stored as `Nm!XFD1`, reads
/// D3). A name whose formula is a constant names none (`=INDIRECT("Konst")`, Konst being 5, is
/// #REF!, the same measurement). A name whose formula is anything else (a computed reference) is
/// not modelled.
fn bind_name(
    name: &str,
    scope: NameScope,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
    depth: usize,
    parent: Option<calc_func::value::NameId>,
) -> TextReference {
    let binding = catalog
        .name_binding(name, scope, parent)
        .and_then(|proof| proof.resolved);
    match catalog.defined_name(name, scope) {
        NameDefinition::Unknown => TextReference::Invalid,
        NameDefinition::NotModelled => TextReference::NotModelled,
        NameDefinition::Formula(_) if depth >= MAX_NAME_DEPTH => TextReference::NotModelled,
        NameDefinition::Formula(formula) => {
            let text = formula.strip_prefix('=').unwrap_or(&formula);
            if is_constant(text) {
                return TextReference::Invalid;
            }
            let sheet = binding
                .and_then(|bound| match bound.scope {
                    calc_func::value::NameScope::Sheet(sheet) => Some(sheet),
                    calc_func::value::NameScope::Workbook => None,
                })
                .unwrap_or(anchor.sheet());
            // A local definition's unqualified references and nested names belong to its
            // defining sheet; relative coordinates still use the caller's row and column.
            let place = FormulaAnchor::new(
                CellKey::new(sheet, anchor.cell().row(), anchor.cell().column())
                    .expect("the caller's coordinates lie on the grid"),
            );
            let a1 = CellKey::new(sheet, 1, 1).expect("A1 lies on the grid");
            let base = if has_relative_coordinate(text) {
                FormulaAnchor::new(a1)
            } else {
                place
            };
            match bind_str(
                text,
                ReferenceNotation::A1,
                base,
                place,
                catalog,
                depth + 1,
                binding.map(|bound| bound.id),
            ) {
                TextReference::Area(area) => TextReference::Area(area),
                _ => TextReference::NotModelled,
            }
        }
    }
}

/// Whether a name's formula text is one constant: a number (signed or not), a text, TRUE or
/// FALSE, an error or an array constant.
fn is_constant(text: &str) -> bool {
    use crate::syntax::SyntaxKind;
    let tree = crate::parse_formula(&format!("={text}"));
    if !tree.diagnostics().is_empty() {
        return false;
    }
    let Some(expression) = tree.expression() else {
        return false;
    };
    match expression.kind() {
        SyntaxKind::NumberLiteralExpr
        | SyntaxKind::StringLiteralExpr
        | SyntaxKind::ErrorLiteralExpr
        | SyntaxKind::ArrayLiteralExpr => true,
        SyntaxKind::IdentifierExpr => {
            let word = text.trim();
            word.eq_ignore_ascii_case("TRUE") || word.eq_ignore_ascii_case("FALSE")
        }
        SyntaxKind::PrefixExpr => expression
            .child_nodes()
            .next()
            .is_some_and(|operand| operand.kind() == SyntaxKind::NumberLiteralExpr),
        _ => false,
    }
}

/// Whether a name's formula text holds an A1 cell coordinate, outside quotes, whose column or
/// row lacks its `$`: a file stores such a reference relative to A1, and Excel reads it relative
/// to the calling cell, which is not modelled here.
fn has_relative_coordinate(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let identifier = |c: char| c.is_alphanumeric() || matches!(c, '_' | '.' | '\\');
    let mut quoted = false;
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c == '\'' {
            quoted = !quoted;
            at += 1;
            continue;
        }
        let starts = at == 0 || !(identifier(chars[at - 1]) || chars[at - 1] == '$');
        if quoted || !starts || !(c == '$' || c.is_ascii_alphabetic()) {
            at += 1;
            continue;
        }
        let mut i = at;
        let column_absolute = chars[i] == '$';
        if column_absolute {
            i += 1;
        }
        let letters = chars[i..]
            .iter()
            .take_while(|c| c.is_ascii_alphabetic())
            .count();
        i += letters;
        let row_absolute = chars.get(i) == Some(&'$');
        if row_absolute {
            i += 1;
        }
        let digits = chars[i..].iter().take_while(|c| c.is_ascii_digit()).count();
        i += digits;
        let ends = chars
            .get(i)
            .is_none_or(|next| !(identifier(*next) || matches!(next, '(' | '!')));
        if (1..=3).contains(&letters) && digits > 0 && ends && !(column_absolute && row_absolute) {
            return true;
        }
        at = i.max(at + 1);
    }
    false
}

/// What the text after its qualifier names before its sheet is known.
enum Target {
    Shape(ReferenceShape),
    /// Text that could be a defined name.
    Name(String),
    /// A range one or both of whose endpoints could be a defined name: those endpoints.
    NamedRange(Vec<String>),
}

/// The text's sheet name, unquoted, when it has a qualifier, and the text after the qualifier,
/// without the spaces that may follow its `!`; or the outcome when the qualifier already decides
/// it.
fn split_qualifier<'t>(
    text: &'t str,
    catalog: &dyn ReferenceCatalog,
) -> Result<(Option<String>, &'t str), TextReference> {
    if let Some(bracketed) = text.strip_prefix('[') {
        let (book, after) = book_name(bracketed)?;
        own_workbook(book, catalog)?;
        // This workbook, named before a sheet qualifier that is not quoted: the text after the
        // name reads as text without it, but must name a sheet (`[Book1.xlsx]A1`,
        // `[Book1.xlsx]Sheet1` and `[Book1.xlsx]'Sheet1'!A1` are #REF!).
        if after.starts_with('\'') || after.starts_with('[') {
            return Err(TextReference::Invalid);
        }
        return match split_qualifier(after, catalog)? {
            (None, _) => Err(TextReference::Invalid),
            split => Ok(split),
        };
    }
    if let Some(quoted) = text.strip_prefix('\'') {
        let (name, after) = quoted_name(quoted).ok_or(TextReference::Invalid)?;
        let rest = after.strip_prefix('!').ok_or(TextReference::Invalid)?;
        if name.contains(['[', ']']) {
            let sheet = quoted_book_sheet(&name, catalog)?;
            return Ok((Some(sheet), rest.trim_start_matches(' ')));
        }
        return Ok((Some(name), rest.trim_start_matches(' ')));
    }
    let Some(bang) = text.find('!') else {
        return Ok((None, text));
    };
    let (name, rest) = (&text[..bang], &text[bang + 1..]);
    if name.is_empty()
        || name.starts_with('.')
        || name.eq_ignore_ascii_case("TRUE")
        || name.eq_ignore_ascii_case("FALSE")
        || name.contains(not_in_unquoted_name)
    {
        return Err(TextReference::Invalid);
    }
    // A workbook name inside the sheet name (`Sheet[1]!A1`), a tab, a carriage return or a
    // non-ASCII character that is neither a letter nor `€` (dc-10y): not measured in an unquoted
    // name. A space before a workbook name names no reference (` [Book1.xlsx]Sheet1!A1`, Excel
    // 16.0.20430, 2026-10-25), as a space anywhere in an unquoted name does.
    if name.contains(['[', ']']) || !name.chars().all(in_unquoted_name) {
        return Err(TextReference::NotModelled);
    }
    Ok((Some(name.to_owned()), rest.trim_start_matches(' ')))
}

/// The workbook name inside `[...]` at the start of `text` (after its `[`), and the text after the
/// `]`. A name holding another `[` names no reference (`[[Book1.xlsx]]Sheet1!A1` is #REF!, Excel
/// 16.0.20430, 2026-10-25); one with no `]` is not measured.
fn book_name(text: &str) -> Result<(&str, &str), TextReference> {
    let close = text.find(']').ok_or(TextReference::NotModelled)?;
    let book = &text[..close];
    if book.contains('[') {
        return Err(TextReference::Invalid);
    }
    Ok((book, &text[close + 1..]))
}

/// `Ok` when the workbook name `book` names the formula's own workbook. No other workbook is open
/// beside it in this product, and INDIRECT of text naming a workbook that is not open is #REF!
/// (Microsoft's INDIRECT page). Excel 16.0.20430 (2026-10-25), in a saved `p5.xlsx`:
/// `[Book1]data!A1`, `[1]Sheet1!A1`, `[]Sheet1!A1`, `[p5]Sheet1!A1` and `'[Book1]x y'!A1` are
/// #REF!, while `[p5.xlsx]Sheet1!A1` and `[P5.XLSX]Sheet1!A1` name the cell. A catalogue that does
/// not know the workbook's file name cannot tell, and the text is not modelled.
fn own_workbook(book: &str, catalog: &dyn ReferenceCatalog) -> Result<(), TextReference> {
    let own = catalog.workbook_name().ok_or(TextReference::NotModelled)?;
    let same = book == own
        || book
            .chars()
            .flat_map(char::to_uppercase)
            .eq(own.chars().flat_map(char::to_uppercase));
    if same {
        Ok(())
    } else {
        Err(TextReference::Invalid)
    }
}

/// The sheet a quoted qualifier holding a workbook name names (`'[Book1.xlsx]Sheet 2'!A1`): the
/// sheet after the formula's own workbook's name. Another workbook's name names no reference. A
/// qualifier with a folder before this workbook's name (`'C:\Data\[Book1.xlsx]Sheet1'!A1`), which
/// Excel resolved against the folder the workbook was opened from, is not modelled: the catalogue
/// knows no folder.
fn quoted_book_sheet(name: &str, catalog: &dyn ReferenceCatalog) -> Result<String, TextReference> {
    let Some(open) = name.find('[') else {
        return Err(TextReference::NotModelled);
    };
    let (book, sheet) = book_name(&name[open + 1..])?;
    own_workbook(book, catalog)?;
    if open > 0 || sheet.is_empty() || sheet.contains(['[', ']']) {
        return Err(TextReference::NotModelled);
    }
    Ok(sheet.to_owned())
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
        // `A1:B` and `A1:Rate` are ranges only if B or Rate name a cell.
        _ if could_be_name(first, first_spelled, notation)
            || could_be_name(last, last_spelled, notation) =>
        {
            Ok(Target::NamedRange(
                [(first, first_spelled), (last, last_spelled)]
                    .into_iter()
                    .filter(|(token, spelled)| could_be_name(token, *spelled, notation))
                    .map(|(token, _)| token.to_owned())
                    .collect(),
            ))
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
            matches!(unspelled(token, notation), Ok(Target::Name(_)))
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
    Ok(Target::Name(token.to_owned()))
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
        fn defined_name(
            &self,
            _: &str,
            _: crate::reference::NameScope,
        ) -> crate::reference::NameDefinition {
            crate::reference::NameDefinition::NotModelled
        }

        fn table(&self, name: &str) -> crate::reference::TableLookup {
            let _ = name;
            crate::reference::TableLookup::NotModelled
        }

        fn table_containing(
            &self,
            cell: calc_func::value::CellKey,
        ) -> crate::reference::TableLookup {
            let _ = cell;
            crate::reference::TableLookup::NotModelled
        }

        fn sheet_span(
            &self,
            first: calc_func::value::SheetId,
            last: calc_func::value::SheetId,
        ) -> Option<Vec<calc_func::value::SheetId>> {
            let _ = (first, last);
            None
        }

        fn unknown_calls_are_names(&self) -> bool {
            false
        }

        fn workbook_name(&self) -> Option<&str> {
            None
        }

        fn external_sheet(
            &self,
            book: &crate::reference::ExternalBookRef,
            sheet: &str,
        ) -> Option<calc_func::value::SheetId> {
            let _ = (book, sheet);
            None
        }

        fn external_function_target(
            &self,
            book: &crate::reference::ExternalBookRef,
        ) -> Option<String> {
            let _ = book;
            None
        }

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
            // A line feed after the text reads as a space (Excel 16.0.20430, bead dc-51ka).
            ("A1\n", area(here, 1, 1, 1, 1)),
            ("A1 \n", area(here, 1, 1, 1, 1)),
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

    /// A catalogue that also defines names: workbook-scoped `Rate` (an absolute cell of data), `\x`
    /// (with a leading `=`), `Chain` (another name), `Rel` (a relative reference), `Const` (a
    /// number), and data's own `Local`.
    struct Named(Sheets);

    impl ReferenceCatalog for Named {
        fn table(&self, name: &str) -> crate::reference::TableLookup {
            let _ = name;
            crate::reference::TableLookup::NotModelled
        }

        fn table_containing(
            &self,
            cell: calc_func::value::CellKey,
        ) -> crate::reference::TableLookup {
            let _ = cell;
            crate::reference::TableLookup::NotModelled
        }

        fn sheet_span(
            &self,
            first: calc_func::value::SheetId,
            last: calc_func::value::SheetId,
        ) -> Option<Vec<calc_func::value::SheetId>> {
            let _ = (first, last);
            None
        }

        fn unknown_calls_are_names(&self) -> bool {
            false
        }

        fn workbook_name(&self) -> Option<&str> {
            None
        }

        fn external_sheet(
            &self,
            book: &crate::reference::ExternalBookRef,
            sheet: &str,
        ) -> Option<calc_func::value::SheetId> {
            let _ = (book, sheet);
            None
        }

        fn external_function_target(
            &self,
            book: &crate::reference::ExternalBookRef,
        ) -> Option<String> {
            let _ = book;
            None
        }

        fn sheet_named(&self, name: &str) -> Option<SheetId> {
            self.0.sheet_named(name)
        }

        fn defined_name(&self, name: &str, scope: NameScope) -> NameDefinition {
            let data = self.0.sheet_named("data").unwrap();
            let formula = match (name.to_ascii_lowercase().as_str(), scope) {
                ("local", NameScope::Sheet(sheet)) if sheet == data => "data!$B$2",
                ("local", _) => return NameDefinition::Unknown,
                (_, NameScope::Sheet(_)) => return NameDefinition::Unknown,
                ("rate", _) => "data!$A$2",
                ("\\x", _) => "=data!$A$3:$B$4",
                ("chain", _) => "Rate",
                ("rel", _) => "data!A1",
                ("const", _) => "0.05",
                _ => return NameDefinition::Unknown,
            };
            NameDefinition::Formula(formula.to_owned())
        }
    }

    /// INDIRECT's text names a defined name: one whose formula is one absolute reference resolves
    /// to it (Excel 16.0.20430 resolved `Rate` and `\x`, bead dc-51ka); a name the workbook does
    /// not define, alone or as a range's endpoint, names no reference (`NoSuchName`, `data!Foo`,
    /// `XFE1` and `A1:B` were #REF!); a relative name is read from the calling cell and a constant
    /// one names no reference (L3).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn names_in_text() {
        let (sheets, s, anchor) = fixture();
        let data = s[1];
        let catalog = Named(sheets);
        let bound = |text: &str| {
            let units: Vec<u16> = text.encode_utf16().collect();
            bind_text(&units, ReferenceNotation::A1, anchor, &catalog)
        };
        assert_eq!(bound("Rate"), area(data, 2, 1, 2, 1));
        assert_eq!(bound("rate "), area(data, 2, 1, 2, 1));
        assert_eq!(bound("\\x"), area(data, 3, 1, 4, 2));
        assert_eq!(bound("Chain"), area(data, 2, 1, 2, 1));
        assert_eq!(bound("data!Local"), area(data, 2, 2, 2, 2));
        for text in ["NoSuchName", "data!Foo", "XFE1", "A1:B", "Local"] {
            assert_eq!(bound(text), TextReference::Invalid, "{text:?}");
        }
        // L3 (dc-m3u): a relative name is read from the calling cell (D10 here), a constant
        // names no reference (Excel 16.0, 2026-10-23: `=INDIRECT("Konst")` is #REF!).
        assert_eq!(bound("Rel"), area(data, 10, 4, 10, 4));
        assert_eq!(bound("Const"), TextReference::Invalid);
        assert_eq!(bound("A1:Rate"), TextReference::NotModelled);
        assert!(has_relative_coordinate("data!A1"));
        assert!(has_relative_coordinate("$A1"));
        assert!(has_relative_coordinate("A$1"));
        assert!(!has_relative_coordinate("data!$A$1:$B$2"));
        assert!(!has_relative_coordinate("'Q2 A1'!$C$3"));
        assert!(!has_relative_coordinate("Q2!$C$3"));
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

    /// The sheets of [`fixture`] in a workbook whose file name is `p5.xlsx`.
    struct NamedBook(Sheets);

    impl ReferenceCatalog for NamedBook {
        fn defined_name(
            &self,
            _: &str,
            _: crate::reference::NameScope,
        ) -> crate::reference::NameDefinition {
            crate::reference::NameDefinition::NotModelled
        }

        fn table(&self, name: &str) -> crate::reference::TableLookup {
            let _ = name;
            crate::reference::TableLookup::NotModelled
        }

        fn table_containing(
            &self,
            cell: calc_func::value::CellKey,
        ) -> crate::reference::TableLookup {
            let _ = cell;
            crate::reference::TableLookup::NotModelled
        }

        fn sheet_span(
            &self,
            first: calc_func::value::SheetId,
            last: calc_func::value::SheetId,
        ) -> Option<Vec<calc_func::value::SheetId>> {
            let _ = (first, last);
            None
        }

        fn unknown_calls_are_names(&self) -> bool {
            false
        }

        fn external_sheet(
            &self,
            book: &crate::reference::ExternalBookRef,
            sheet: &str,
        ) -> Option<calc_func::value::SheetId> {
            let _ = (book, sheet);
            None
        }

        fn external_function_target(
            &self,
            book: &crate::reference::ExternalBookRef,
        ) -> Option<String> {
            let _ = book;
            None
        }

        fn sheet_named(&self, name: &str) -> Option<SheetId> {
            self.0.sheet_named(name)
        }

        fn workbook_name(&self) -> Option<&str> {
            Some("p5.xlsx")
        }
    }

    /// Text naming a workbook, against Excel 16.0.20430 (2026-10-25) in a saved `p5.xlsx` with the
    /// sheets Sheet1 and `Sheet 2` (see `own_workbook`): this workbook's name before a sheet
    /// qualifier names that sheet's cells, any other workbook's name, or this one's without a
    /// sheet, names none. A catalogue that does not know the workbook's name refuses all of it.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn text_naming_a_workbook() {
        use ReferenceNotation::{A1, R1C1};
        let (sheets, s, anchor) = fixture();
        let (data, spaced) = (s[1], s[2]);
        let named = NamedBook(sheets);
        let bound_in = |catalog: &dyn ReferenceCatalog, text: &str, notation| {
            let units: Vec<u16> = text.encode_utf16().collect();
            bind_text(&units, notation, anchor, catalog)
        };
        for (text, notation, expected) in [
            ("[p5.xlsx]data!A1", A1, area(data, 1, 1, 1, 1)),
            ("[P5.XLSX]data!A1", A1, area(data, 1, 1, 1, 1)),
            ("[p5.xlsx]DATA!a1", A1, area(data, 1, 1, 1, 1)),
            ("[p5.xlsx]data!$A$1", A1, area(data, 1, 1, 1, 1)),
            ("[p5.xlsx]data! A1", A1, area(data, 1, 1, 1, 1)),
            ("[p5.xlsx]data!A1:B2", A1, area(data, 1, 1, 2, 2)),
            ("[p5.xlsx]data!R1C1", R1C1, area(data, 1, 1, 1, 1)),
            ("'[p5.xlsx]Q2 Data'!A1", A1, area(spaced, 1, 1, 1, 1)),
            ("'[p5.xlsx]data'! A1", A1, area(data, 1, 1, 1, 1)),
            ("[Book1]data!A1", A1, TextReference::Invalid),
            ("[1]data!A1", A1, TextReference::Invalid),
            ("[]data!A1", A1, TextReference::Invalid),
            ("[p5]data!A1", A1, TextReference::Invalid),
            ("'[Book1]x y'!A1", A1, TextReference::Invalid),
            ("'C:\\x\\[Book1.xlsx]data'!A1", A1, TextReference::Invalid),
            ("[Book1.xlsx]data!R1C1", R1C1, TextReference::Invalid),
            ("[p5.xlsx]A1", A1, TextReference::Invalid),
            ("[p5.xlsx]data", A1, TextReference::Invalid),
            ("[p5.xlsx]'data'!A1", A1, TextReference::Invalid),
            ("[p5.xlsx] data!A1", A1, TextReference::Invalid),
            ("[p5.xlsx]Q2 Data!A1", A1, TextReference::Invalid),
            ("[p5.xlsx]data!here!A1", A1, TextReference::Invalid),
            ("[[p5.xlsx]]data!A1", A1, TextReference::Invalid),
            (" [p5.xlsx]data!A1", A1, TextReference::Invalid),
            // The folder the workbook was opened from is not known.
            ("'C:\\x\\[p5.xlsx]data'!A1", A1, TextReference::NotModelled),
            ("[p5.xlsx data!A1", A1, TextReference::NotModelled),
        ] {
            assert_eq!(bound_in(&named, text, notation), expected, "{text:?}");
        }
        let (unnamed, _, _) = fixture();
        for text in ["[p5.xlsx]data!A1", "[Book1]data!A1", "'[Book1]x y'!A1"] {
            assert_eq!(
                bound_in(&unnamed, text, A1),
                TextReference::NotModelled,
                "{text:?}"
            );
        }
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
