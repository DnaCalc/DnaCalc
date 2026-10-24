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

use crate::read::SnapshotCatalog;
use calc_doc_model::{
    Axis, AxisChange, FormulaRewriter, FormulaText, ModelRoot, NameScope, StructuralChange,
};
use calc_fml::reference::{EditAxis, StructuralEdit, transform_formula};
use calc_value::{CellKey, ExcelText};

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
        let a1 = CellKey::new(sheet, 1, 1).ok()?;
        let text = formula.strip_prefix('=').unwrap_or(formula);
        transform_formula(text, a1, &fml_edit(root, change), &SnapshotCatalog(root))
    }
}
