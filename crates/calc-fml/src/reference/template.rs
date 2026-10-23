//! A formula's template: its text with every A1 reference written as what it means at its anchor
//! (an absolute coordinate, or an offset from the anchor), so the formulas a fill or a shared
//! formula copies across a range, whose texts differ only in how their relative references are
//! spelled, have one template.
//!
//! Two formulas on one sheet with one template compile to the same plan: their tokens are the
//! same but for reference tokens that bind to equal operands (the binder reads a reference only
//! through the same speller, `notation::a1`), and the parser sees only the tokens' kinds and the
//! texts the template keeps. A caller that keeps plans by template can therefore skip parsing and
//! compiling a formula whose template it has compiled before (OxCalc's per-template plan cache,
//! `src/oxcalc-core/src/grid/machine/r1c1_plan.rs` at commit 1bd514ef, keyed the same way; its
//! separate fast-path evaluator is not ported: plans are CalcFml's).
//!
//! Text the template cannot be sure of has none: a number beside a `:` (a row reference such as
//! `1:3`, which the A1 speller would read relative to the anchor).

use super::notation::{self, Spelled};
use super::{AxisRef, FormulaAnchor};
use crate::syntax::{TokenKind, lex_chars};
use std::fmt::Write;

/// The template of the formula `text`, in A1 notation, anchored at `anchor`; `None` when the text
/// holds a construct the template cannot be sure of. The template does not name the anchor's
/// sheet: a caller keeps templates per sheet, since a sheet's own defined names and unqualified
/// references depend on it.
pub fn template_key(text: &str, anchor: FormulaAnchor) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let tokens = lex_chars(&chars);
    let mut key = String::with_capacity(text.len() + 16);
    let mut word = String::new();
    let significant = |index: usize, step: isize| -> Option<TokenKind> {
        let mut at = index as isize + step;
        while at >= 0 && (at as usize) < tokens.len() {
            let kind = tokens[at as usize].0;
            if kind != TokenKind::Whitespace {
                return Some(kind);
            }
            at += step;
        }
        None
    };
    let at = anchor.cell();
    for (index, &(kind, start, end)) in tokens.iter().enumerate() {
        let beside_colon = significant(index, 1) == Some(TokenKind::Colon)
            || significant(index, -1) == Some(TokenKind::Colon);
        // A cell's spelling is a reference wherever it stands (no name looks like a cell), a
        // row's or a column's only as an end of a range: alone, `A` is a name.
        let spelled = match kind {
            // A function's name and a sheet's (`A1!B2` names sheet A1) are kept as written.
            TokenKind::Identifier
                if !matches!(
                    significant(index, 1),
                    Some(TokenKind::LParen | TokenKind::Bang)
                ) =>
            {
                word.clear();
                word.extend(&chars[start..end]);
                match notation::a1(&word, at.row(), at.column()) {
                    Some(spelled @ Spelled::Cell(_)) => Some(spelled),
                    Some(spelled @ (Spelled::Row(_) | Spelled::Column(_))) if beside_colon => {
                        Some(spelled)
                    }
                    _ => None,
                }
            }
            TokenKind::Number if beside_colon => return None,
            _ => None,
        };
        key.push(char::from(b'A' + kind as u8));
        match spelled {
            Some(spelled) => {
                key.push('<');
                match spelled {
                    Spelled::Cell(cell) => {
                        key.push('c');
                        push_axis(&mut key, cell.row);
                        push_axis(&mut key, cell.column);
                    }
                    Spelled::Row(row) => {
                        key.push('r');
                        push_axis(&mut key, row);
                    }
                    Spelled::Column(column) => {
                        key.push('k');
                        push_axis(&mut key, column);
                    }
                    Spelled::OutsideGrid(_) => unreachable!("kept as written"),
                }
                key.push('>');
            }
            None => {
                key.push('\u{1}');
                key.extend(&chars[start..end]);
                key.push('\u{1}');
            }
        }
    }
    Some(key)
}

/// Appends an axis to a template: `$` and the coordinate, or `~` and the offset.
fn push_axis(key: &mut String, axis: AxisRef) {
    let _ = match axis {
        AxisRef::Absolute(index) => write!(key, "${index}"),
        AxisRef::Relative(offset) => write!(key, "~{offset}"),
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_func::value::{CellKey, IdAllocator, SheetId};

    fn anchor(row: u32, column: u32) -> FormulaAnchor {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        FormulaAnchor::new(CellKey::new(ids.issue().unwrap(), row, column).unwrap())
    }

    /// A filled formula has one template down its column; an absolute reference, another
    /// function, another relative offset or a row range does not.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn copies_share_a_template() {
        let key = |text: &str, row, column| template_key(text, anchor(row, column));
        assert_eq!(key("=A1*2+$B$1", 1, 3), key("=A9*2+$B$1", 9, 3));
        assert_eq!(key("=SUM(A1:B2)", 3, 3), key("=SUM(B2:C3)", 4, 4));
        assert_ne!(key("=A1*2", 1, 3), key("=A1*2", 2, 3));
        assert_ne!(key("=$A$1", 1, 1), key("=$A$2", 2, 1));
        // Function names are kept as written, though LOG10 and LOG11 spell cells.
        assert_ne!(key("=LOG10(4)", 1, 1), key("=LOG11(4)", 1, 2));
        assert_eq!(key("=SUM(1:3)", 1, 1), None);
        assert_ne!(key("=\"A1\"", 1, 1), key("=\"A2\"", 2, 1));
        // A lone column's letters are a name: A in column 1 and B in column 2 differ.
        assert_ne!(key("=A+1", 1, 1), key("=B+1", 1, 2));
        assert_eq!(key("=SUM(A:A)", 1, 2), key("=SUM(B:B)", 1, 3));
        // A sheet named like a cell is kept as written.
        assert_ne!(key("=A1!B2", 1, 1), key("=A2!B3", 2, 1));
        assert_eq!(key("=Data!B2", 1, 1), key("=Data!B3", 2, 1));
    }
}
