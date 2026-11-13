//! The cells in which a revision of a loaded workbook's model differs from the loaded revision,
//! found page by page: a page the revision shares with the loaded root by pointer holds no change
//! and is not read.

use calc_doc_model::value::SheetId;
use calc_doc_model::{CellInput, CellPage, ModelRoot};
use std::collections::BTreeMap;
use std::sync::Arc;

/// One sheet's changed cells, by (row, column): the new input, or `None` for a cell the revision
/// emptied.
pub(super) type SheetChanges = BTreeMap<(u32, u32), Option<CellInput>>;

fn pages_by_block(root: &ModelRoot, sheet: SheetId) -> BTreeMap<(u32, u32), &Arc<CellPage>> {
    root.pages(sheet)
        .into_iter()
        .flatten()
        .map(|page| ((page.first_row(), page.first_column()), page))
        .collect()
}

/// The cells of `sheet` whose input in `model` is not the one in `base`.
pub(super) fn sheet_changes(base: &ModelRoot, model: &ModelRoot, sheet: SheetId) -> SheetChanges {
    let before = pages_by_block(base, sheet);
    let after = pages_by_block(model, sheet);
    let mut changes = SheetChanges::new();
    let mut blocks: Vec<&(u32, u32)> = before.keys().chain(after.keys()).collect();
    blocks.sort();
    blocks.dedup();
    for block in blocks {
        let (old, new) = (before.get(block), after.get(block));
        if let (Some(old), Some(new)) = (old, new)
            && Arc::ptr_eq(old, new)
        {
            continue;
        }
        let old: BTreeMap<(u32, u32), &CellInput> = old
            .map(|page| page.iter().map(|(r, c, input)| ((r, c), input)).collect())
            .unwrap_or_default();
        let new: BTreeMap<(u32, u32), &CellInput> = new
            .map(|page| page.iter().map(|(r, c, input)| ((r, c), input)).collect())
            .unwrap_or_default();
        for (at, input) in &new {
            if old.get(at) != Some(input) {
                changes.insert(*at, Some((*input).clone()));
            }
        }
        for at in old.keys() {
            if !new.contains_key(at) {
                changes.insert(*at, None);
            }
        }
    }
    // Equal plain text can carry different rich runs or phonetics. Source changes must
    // rewrite the cell even when its calculated input is unchanged.
    for at in base
        .definitions()
        .text_sources()
        .keys()
        .chain(model.definitions().text_sources().keys())
    {
        if at.sheet() == sheet
            && base.definitions().text_sources().get(at)
                != model.definitions().text_sources().get(at)
        {
            changes.insert((at.row(), at.column()), model.cell(*at).cloned());
        }
    }
    changes
}
