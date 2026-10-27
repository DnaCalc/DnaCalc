//! Structural edits (L3, dc-m3u): the formula rewriter the model applies them with, and the
//! engine's share of them.
//!
//! [`ReferenceRewriter`] is the [`calc_doc_model::FormulaRewriter`] that
//! [`calc_doc_model::ModelRoot::apply_structural`] rewrites every formula and defined name with:
//! CalcFml's [`calc_fml::reference::transform_formula`] over the sheets of the root before the
//! edit. [`crate::prepare_edit`] stages a structural batch as a move of every formula node's
//! anchor (or its removal, when the edit deletes its cell or sheet), and [`crate::commit_edit`]
//! moves the nodes and their published results, keeps their IDs, and leaves every node unbound
//! and pending: the next recalculation binds each formula's rewritten text and calculates it.
//! Nothing of the graph is patched in place; that refinement (ceng-m4.1's) is not done.
//!
//! [`relocate_formula`] is the text a copy of a formula cell writes in another cell, and
//! [`move_references`] the text a move (cut and paste) of cells leaves a formula with (dc-09v).

use crate::read::SnapshotCatalog;
use calc_doc_model::{
    Axis, AxisChange, FormulaRewriter, FormulaText, ModelRoot, NameScope, StructuralChange,
};
use calc_fml::reference::{
    EditAxis, ReferenceNotation, StructuralEdit, convert_notation, transform_formula,
    transform_name,
};
use calc_value::{CellKey, ExcelText, GridArea};

/// The formula rewriter of structural edits: CalcFml's reference transform over the root before
/// the edit.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReferenceRewriter;

fn fml_edit(root: &ModelRoot, change: &StructuralChange) -> StructuralEdit {
    match change {
        StructuralChange::Axis(edit) => {
            let axis = match edit.axis() {
                Axis::Rows => EditAxis::Rows,
                Axis::Columns => EditAxis::Columns,
            };
            match edit.change() {
                AxisChange::Insert => StructuralEdit::Insert {
                    sheet: edit.sheet(),
                    axis,
                    first: edit.first(),
                    count: edit.count(),
                },
                AxisChange::Delete => StructuralEdit::Delete {
                    sheet: edit.sheet(),
                    axis,
                    first: edit.first(),
                    count: edit.count(),
                },
            }
        }
        StructuralChange::RenameSheet { sheet, name } => StructuralEdit::RenameSheet {
            sheet: *sheet,
            name: name.text().to_string_lossy(),
        },
        StructuralChange::RemoveSheet { sheet } => StructuralEdit::RemoveSheet {
            sheet: *sheet,
            order: root
                .sheets()
                .iter()
                .map(|entry| (entry.id(), entry.name().text().to_string_lossy()))
                .collect(),
        },
    }
}

impl FormulaRewriter for ReferenceRewriter {
    fn formula(
        &self,
        root: &ModelRoot,
        formula: &FormulaText,
        at: CellKey,
        change: &StructuralChange,
    ) -> Option<FormulaText> {
        let text = String::from_utf16(formula.text().utf16_code_units()).ok()?;
        let rewritten =
            transform_formula(&text, at, &fml_edit(root, change), &SnapshotCatalog(root))?;
        FormulaText::new(ExcelText::from_utf16_code_units(
            rewritten.encode_utf16().collect(),
        ))
        .ok()
    }

    fn name(
        &self,
        root: &ModelRoot,
        formula: &str,
        scope: NameScope,
        change: &StructuralChange,
    ) -> Option<String> {
        let sheet = match scope {
            NameScope::Sheet(sheet) => sheet,
            NameScope::Workbook => root.sheets().first()?.id(),
        };
        let text = formula.strip_prefix('=').unwrap_or(formula);
        transform_name(text, sheet, &fml_edit(root, change), &SnapshotCatalog(root))
    }
}

/// The formula a copy of the cell `from` writes in the cell `to`, as Excel's copy and paste and its
/// fill write it ("Move or copy cells"): every relative row or column keeps its offset from the
/// formula's cell and every absolute one (`$`) stays, through CalcFml's A1 and R1C1 spellings of
/// the formula's references (the formula read in R1C1 at `from`, then written in A1 at `to`); a
/// relative reference the copy moves off the grid is `#REF!`. A reference qualified with a sheet
/// keeps its sheet. `None` when the formula's text does not parse (dc-09v).
pub fn relocate_formula(
    root: &ModelRoot,
    formula: &FormulaText,
    from: CellKey,
    to: CellKey,
) -> Option<FormulaText> {
    let text = String::from_utf16(formula.text().utf16_code_units()).ok()?;
    let catalog = SnapshotCatalog(root);
    let relative = convert_notation(
        &text,
        from,
        ReferenceNotation::A1,
        ReferenceNotation::R1C1,
        &catalog,
    )?;
    let placed = convert_notation(
        &relative,
        to,
        ReferenceNotation::R1C1,
        ReferenceNotation::A1,
        &catalog,
    )?;
    FormulaText::new(ExcelText::from_utf16_code_units(
        placed.encode_utf16().collect(),
    ))
    .ok()
}

/// The text a move of the cells of `area` to `to` (cut and paste on one sheet) leaves the formula
/// in the cell `at` with, as Excel's move adjusts references: a reference to a moved cell, or to a
/// range all of whose cells moved, follows them (CalcFml's [`StructuralEdit::MoveCells`]); `at`
/// is the formula's cell before the move. `None` when no reference of it changes (dc-09v).
pub fn move_references(
    root: &ModelRoot,
    formula: &FormulaText,
    at: CellKey,
    area: GridArea,
    to: CellKey,
) -> Option<FormulaText> {
    let text = String::from_utf16(formula.text().utf16_code_units()).ok()?;
    let moved = transform_formula(
        &text,
        at,
        &StructuralEdit::MoveCells { area, to },
        &SnapshotCatalog(root),
    )?;
    FormulaText::new(ExcelText::from_utf16_code_units(
        moved.encode_utf16().collect(),
    ))
    .ok()
}
