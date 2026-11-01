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
        let rewritten = transform_formula(
            &text,
            at,
            &fml_edit(root, change),
            &SnapshotCatalog::new(root),
        );
        // A structural edit may split one group's reference occurrences differently. The
        // first slice materializes it before moving any member, even for constant formulas.
        let rewritten = match rewritten {
            Some(text) => text,
            None if formula.block().is_some() => text,
            None => return None,
        };
        FormulaText::with_form(
            ExcelText::from_utf16_code_units(rewritten.encode_utf16().collect()),
            formula.form(),
        )
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
        transform_name(
            text,
            sheet,
            &fml_edit(root, change),
            &SnapshotCatalog::new(root),
        )
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
    let catalog = SnapshotCatalog::new(root);
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
    FormulaText::with_form(
        ExcelText::from_utf16_code_units(placed.encode_utf16().collect()),
        formula.form(),
    )
    .ok()
}

/// CalcFml's move of the cells of `area` so that its top-left cell is `to`, with the two sheets'
/// names from `root`; `None` when `root` holds either sheet no more.
fn move_edit(root: &ModelRoot, area: GridArea, to: CellKey) -> Option<StructuralEdit> {
    let name = |sheet| {
        root.sheet_by_id(sheet)
            .map(|entry| entry.name().text().to_string_lossy())
    };
    Some(StructuralEdit::MoveCells {
        area,
        to,
        source_name: name(area.sheet())?,
        target_name: name(to.sheet())?,
    })
}

/// The text a move of the cells of `area` to `to` (cut and paste, on one sheet or to another)
/// leaves the formula in the cell `at` with, as Excel's move adjusts references (CalcFml's
/// [`StructuralEdit::MoveCells`]): a reference to the moved cells follows them, one to cells the
/// paste writes over is `#REF!`, and a formula moved to another sheet keeps reading the cells it
/// read; `at` is the formula's cell before the move. `None` when no reference of it changes
/// (dc-09v, dc-gf0).
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
        &move_edit(root, area, to)?,
        &SnapshotCatalog::new(root),
    )?;
    FormulaText::with_form(
        ExcelText::from_utf16_code_units(moved.encode_utf16().collect()),
        formula.form(),
    )
    .ok()
}

/// The formula a move of the cells of `area` to `to` leaves the defined name of `scope` with,
/// whose formula is `formula` (as the model holds it): its reference moves only when every
/// coordinate it holds is absolute (CalcFml's [`transform_name`]). `None` when it does not change
/// (dc-gf0).
pub fn move_name(
    root: &ModelRoot,
    formula: &str,
    scope: NameScope,
    area: GridArea,
    to: CellKey,
) -> Option<String> {
    let sheet = match scope {
        NameScope::Sheet(sheet) => sheet,
        NameScope::Workbook => root.sheets().first()?.id(),
    };
    let text = formula.strip_prefix('=').unwrap_or(formula);
    transform_name(
        text,
        sheet,
        &move_edit(root, area, to)?,
        &SnapshotCatalog::new(root),
    )
}

/// Reference rewrite for a rectangular cell shift, after checking the supported contract.
pub fn shift_formula(
    root: &ModelRoot,
    formula: &str,
    at: CellKey,
    area: GridArea,
    down: bool,
    insert: bool,
    name: bool,
) -> Result<Option<String>, &'static str> {
    let text = formula.strip_prefix('=').unwrap_or(formula);
    let catalog = SnapshotCatalog::new(root);
    if !calc_fml::reference::shift_formula_supported(text, at, area, down, name, &catalog) {
        return Err(
            "cell shifts with crossing ranges, whole-axis/3D references or relative names need Excel readings (dc-971p.7)",
        );
    }
    let edit = StructuralEdit::ShiftCells { area, down, insert };
    Ok(if name {
        transform_name(text, at.sheet(), &edit, &catalog)
    } else {
        transform_formula(text, at, &edit, &catalog)
    })
}
