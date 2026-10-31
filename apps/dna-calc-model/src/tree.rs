//! Pure outline projection and navigation.
//! Extracted from DNA Calc; retains the DnaTreeCalc bf6dfd7
//! `src/dnatreecalc-skins/src/tree.rs` helpers ported there.
pub use crate::{Branch, TreeFocus as Focus};
use calc_value::{CellKey, SheetId};
use std::collections::BTreeSet;
/// The rows a sheet shows at first, and how many more each request shows.
pub const ROW_PAGE: usize = 50;
/// The most cells one row of the outline lists.
pub const ROW_CELLS: usize = 30;

/// One sheet's cells as the outline groups them: its rows in order, each with its cells in
/// column order, for the rows shown; and how many rows it holds beyond them.
#[derive(Clone, Debug, PartialEq)]
pub struct OutlineSheet {
    pub id: SheetId,
    pub name: String,
    pub cells: usize,
    pub rows: Vec<(u32, Vec<CellKey>)>,
    pub more_rows: usize,
}

/// The workbook's outline: its name, its defined names (as `Name = formula`) and its sheets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outline {
    pub workbook: String,
    pub names: Vec<String>,
    pub sheets: Vec<OutlineSheet>,
}

/// One line of the outline as shown.
#[derive(Clone, Debug, PartialEq)]
pub enum TreeRow {
    /// A foldable branch: its label, how many it holds, whether it is folded.
    Branch {
        branch: Branch,
        depth: u8,
        label: String,
        count: usize,
        folded: bool,
    },
    /// A defined name.
    Name { index: usize, text: String },
    /// A cell.
    Cell { at: CellKey },
    /// The rows of a sheet not shown yet.
    MoreRows { sheet: SheetId, hidden: usize },
    /// The cells of a row beyond [`ROW_CELLS`].
    MoreCells {
        row: (SheetId, u32),
        hidden: usize,
        first: CellKey,
    },
}

impl TreeRow {
    /// What the focus holds when this line has it.
    pub fn focus(&self) -> Focus {
        match self {
            TreeRow::Branch { branch, .. } => Focus::Branch(*branch),
            TreeRow::Name { index, .. } => Focus::Name(*index),
            TreeRow::Cell { at } => Focus::Cell(*at),
            TreeRow::MoreRows { sheet, .. } => Focus::MoreRows(*sheet),
            TreeRow::MoreCells { row, .. } => Focus::MoreCells(*row),
        }
    }

    pub fn depth(&self) -> u8 {
        match self {
            TreeRow::Branch { depth, .. } => *depth,
            TreeRow::Name { .. } | TreeRow::MoreRows { .. } => 2,
            TreeRow::Cell { .. } | TreeRow::MoreCells { .. } => 3,
        }
    }
}

/// The lines of `outline`, depth first, without what folded branches hold.
pub fn visible_rows(outline: &Outline, folded: &BTreeSet<Branch>) -> Vec<TreeRow> {
    visible_rows_with_reveal(outline, folded, None)
}
pub fn visible_rows_with_reveal(
    outline: &Outline,
    folded: &BTreeSet<Branch>,
    reveal: Option<CellKey>,
) -> Vec<TreeRow> {
    let mut rows = vec![TreeRow::Branch {
        branch: Branch::Workbook,
        depth: 0,
        label: outline.workbook.clone(),
        count: outline.sheets.len(),
        folded: folded.contains(&Branch::Workbook),
    }];
    if folded.contains(&Branch::Workbook) {
        return rows;
    }
    if !outline.names.is_empty() {
        rows.push(TreeRow::Branch {
            branch: Branch::Names,
            depth: 1,
            label: "Defined names".to_owned(),
            count: outline.names.len(),
            folded: folded.contains(&Branch::Names),
        });
        if !folded.contains(&Branch::Names) {
            rows.extend(
                outline
                    .names
                    .iter()
                    .enumerate()
                    .map(|(index, text)| TreeRow::Name {
                        index,
                        text: text.clone(),
                    }),
            );
        }
    }
    for sheet in &outline.sheets {
        let branch = Branch::Sheet(sheet.id);
        rows.push(TreeRow::Branch {
            branch,
            depth: 1,
            label: sheet.name.clone(),
            count: sheet.cells,
            folded: folded.contains(&branch),
        });
        if folded.contains(&branch) {
            continue;
        }
        for (row, cells) in &sheet.rows {
            let branch = Branch::Row(sheet.id, *row);
            rows.push(TreeRow::Branch {
                branch,
                depth: 2,
                label: format!("Row {row}"),
                count: cells.len(),
                folded: folded.contains(&branch),
            });
            if folded.contains(&branch) {
                continue;
            }
            let mut visible: Vec<_> = cells.iter().take(ROW_CELLS).copied().collect();
            if let Some(at) = reveal.filter(|at| cells.contains(at)) {
                visible.push(at);
                visible.sort();
                visible.dedup();
            }
            rows.extend(visible.iter().map(|at| TreeRow::Cell { at: *at }));
            let mut hidden = cells.iter().copied().filter(|at| !visible.contains(at));
            if let Some(first) = hidden.next() {
                rows.push(TreeRow::MoreCells {
                    row: (sheet.id, *row),
                    hidden: hidden.count() + 1,
                    first,
                });
            }
        }
        if sheet.more_rows > 0 {
            rows.push(TreeRow::MoreRows {
                sheet: sheet.id,
                hidden: sheet.more_rows,
            });
        }
    }
    rows
}

/// Folds `branch` (`fold`) or unfolds it; idempotent both ways.
pub fn apply_fold(folded: &mut BTreeSet<Branch>, branch: Branch, fold: bool) {
    if fold {
        folded.insert(branch);
    } else {
        folded.remove(&branch);
    }
}

pub fn toggle_fold(folded: &mut BTreeSet<Branch>, branch: Branch) {
    let fold = !folded.contains(&branch);
    apply_fold(folded, branch, fold);
}

/// The branch a line belongs to: a cell's row, a row's sheet, a sheet's or the names' workbook.
pub fn parent(focus: Focus) -> Option<Branch> {
    match focus {
        Focus::Branch(Branch::Workbook) => None,
        Focus::Branch(Branch::Names) | Focus::Branch(Branch::Sheet(_)) => Some(Branch::Workbook),
        Focus::Branch(Branch::Row(sheet, _)) | Focus::MoreRows(sheet) => Some(Branch::Sheet(sheet)),
        Focus::Name(_) => Some(Branch::Names),
        Focus::Cell(at) => Some(Branch::Row(at.sheet(), at.row())),
        Focus::MoreCells((sheet, row)) => Some(Branch::Row(sheet, row)),
    }
}

/// One sheet's outline from its cells in cell order: the first `shown_rows` rows and how many
/// rows follow.
pub fn outline_sheet(
    id: SheetId,
    name: String,
    cells: impl Iterator<Item = CellKey>,
    shown_rows: usize,
) -> OutlineSheet {
    let mut sheet = OutlineSheet {
        id,
        name,
        cells: 0,
        rows: Vec::new(),
        more_rows: 0,
    };
    let mut last_row = None;
    for at in cells {
        sheet.cells += 1;
        if last_row != Some(at.row()) {
            last_row = Some(at.row());
            if sheet.rows.len() < shown_rows {
                sheet.rows.push((at.row(), Vec::new()));
            } else {
                sheet.more_rows += 1;
                continue;
            }
        }
        if sheet.more_rows == 0
            && let Some((_, cells)) = sheet.rows.last_mut()
        {
            cells.push(at);
        }
    }
    sheet
}

/// WAI tree navigation keeps selection separate from the current keyboard item.
pub fn next_focus(lines: &[TreeRow], focus: Option<Focus>, key: &str) -> Option<Focus> {
    let index = focus
        .and_then(|f| lines.iter().position(|r| r.focus() == f))
        .unwrap_or(0);
    match key {
        "ArrowDown" => lines.get((index + 1).min(lines.len().saturating_sub(1))),
        "ArrowUp" => lines.get(index.saturating_sub(1)),
        "Home" => lines.first(),
        "End" => lines.last(),
        "ArrowRight" => lines
            .get(index + 1)
            .filter(|r| r.depth() > lines[index].depth()),
        "ArrowLeft" => return focus.and_then(parent).map(Focus::Branch),
        _ => None,
    }
    .map(TreeRow::focus)
}
