//! The tree skin: the workbook as a foldable outline. The workbook holds its defined names and its
//! sheets; a sheet holds its rows that hold cells; a row holds its cells, each shown with what was
//! entered and its value with its provenance, and the focused cell with how many formulas read it
//! (asked of the engine's graph for that cell only: asked for every cell shown, it cost tens of
//! seconds on the default tariff cap workbook, dc-gf0). A branch folds and
//! unfolds (a click on its toggle, or Left and Right); Up and Down move through the outline; a
//! click on a cell, or Enter on it, makes it the workbook's active cell (calc-host's selection,
//! the grid's and the formula bar's), and the outline follows the active cell when another skin
//! or the formula bar moves it.
//!
//! The outline is read from calc-host after each change: which cells of each sheet hold an input
//! ([`calc_host::HostState::input_cells`], from the document, without projecting them), and the
//! projections of the cells the outline shows only. A sheet shows [`ROW_PAGE`] rows at first and
//! more on request; a row shows at most [`ROW_CELLS`] cells; every sheet but the active one starts
//! folded. So a change costs a walk over the workbook's cell keys and the shown cells' values,
//! not a projection of every cell (measured on the 165,810-formula default tariff cap workbook,
//! dc-gf0). A dynamic array's spilled cells hold no input and are not listed. The skin changes
//! nothing but the selection.
//!
//! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnatreecalc-skins/src/tree.rs`: the
//! depth-first `visible_rows` over the fold set (`fold`, `unfold`, `toggle_fold`), the dependents
//! badge, the h/l (here Left/Right) fold keys and the follow-the-selection rule, with the walk and
//! fold tests. Left behind: the legacy node paths it walked (an Excel workbook's outline
//! is its sheets, rows and cells), the meta-structure filter and the subtree verbs of
//! `node_management.rs` (duplicate, add, rename, move and reorder nodes), which have no Excel
//! form here; rows and columns are inserted and deleted in the grid.

use crate::cells::{self, Provenance};
use crate::controller::Controller;
use crate::skin::{Skin, SkinContext, SkinId};
use calc_host::value::{CellKey, SheetId};
use leptos::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

/// The rows a sheet shows at first, and how many more each request shows.
pub const ROW_PAGE: usize = 50;
/// The most cells one row of the outline lists.
pub const ROW_CELLS: usize = 30;

pub const TREE_CSS: &str = r#"
.dc-tree{overflow:auto;flex:1 1 auto;padding:8px 12px;font:13px system-ui,"Segoe UI",sans-serif;outline:none}
.dc-tree ul{list-style:none;margin:0;padding:0}
.dc-tree li{display:flex;gap:8px;align-items:baseline;padding:2px 6px;border-radius:4px;white-space:nowrap;cursor:default}
.dc-tree li[data-focused=true]{background:#ddf4ff;box-shadow:inset 0 0 0 1px #54aeff}
.dc-tree .dc-twisty{width:14px;border:0;background:none;padding:0;font:12px monospace;cursor:pointer;color:#57606a}
.dc-tree .dc-label{font-weight:600}
.dc-tree li[data-kind=cell] .dc-label{font:600 12px ui-monospace,Consolas,monospace;min-width:56px}
.dc-tree .dc-input{font:12px ui-monospace,Consolas,monospace;color:#57606a;max-width:420px;overflow:hidden;text-overflow:ellipsis}
.dc-tree .dc-count{color:#57606a;font-size:12px}
.dc-tree .dc-tvalue{font:600 12px ui-monospace,Consolas,monospace}
.dc-tree .dc-chip{display:inline-block;font-size:11px;border-radius:10px;padding:0 7px;background:#eaeef2;color:#424a53}
.dc-tree .dc-chip[data-provenance=calculated]{background:#dafbe1;color:#116329}
.dc-tree .dc-chip[data-provenance=refused]{background:#ffebe9;color:#a40e26}
.dc-tree .dc-chip[data-provenance=stale]{background:#fff8c5;color:#9a6700}
.dc-tree li[data-kind=more] button{font-size:12px}
"#;

#[derive(Clone, Copy, Debug, Default)]
pub struct TreeSkin;

impl Skin for TreeSkin {
    fn id(&self) -> SkinId {
        SkinId::Tree
    }

    fn title(&self) -> &'static str {
        "Tree"
    }

    fn mount(&self, ctx: SkinContext) -> AnyView {
        tree_view(ctx.controller).into_any()
    }
}

/// A branch of the outline, the unit the fold set holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Branch {
    Workbook,
    Names,
    Sheet(SheetId),
    Row(SheetId, u32),
}

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
    MoreCells { row: (SheetId, u32), hidden: usize },
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

/// The line the focus is on, by what it shows, so it stays on its line as lines come and go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Branch(Branch),
    Name(usize),
    Cell(CellKey),
    MoreRows(SheetId),
    MoreCells((SheetId, u32)),
}

/// The lines of `outline`, depth first, without what folded branches hold.
pub fn visible_rows(outline: &Outline, folded: &BTreeSet<Branch>) -> Vec<TreeRow> {
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
            rows.extend(
                cells
                    .iter()
                    .take(ROW_CELLS)
                    .map(|at| TreeRow::Cell { at: *at }),
            );
            if cells.len() > ROW_CELLS {
                rows.push(TreeRow::MoreCells {
                    row: (sheet.id, *row),
                    hidden: cells.len() - ROW_CELLS,
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

fn tree_view(controller: Controller) -> impl IntoView {
    // Every sheet but the active one starts folded: set before the first render (an effect runs
    // after it, which first drew every sheet unfolded), and again for each workbook opened.
    let initially_folded = move || -> BTreeSet<Branch> {
        let active = controller.sheet.get_untracked();
        controller.with_untracked(|book| {
            book.sheets()
                .into_iter()
                .filter(|tab| Some(tab.id) != active)
                .map(|tab| Branch::Sheet(tab.id))
                .collect()
        })
    };
    let folded: RwSignal<BTreeSet<Branch>> = RwSignal::new(initially_folded());
    Effect::new(move |opened: Option<u64>| {
        let generation = controller.generation.get();
        if opened.is_some_and(|opened| opened != generation) {
            folded.set(initially_folded());
        }
        generation
    });
    let shown_rows: RwSignal<BTreeMap<SheetId, usize>> = RwSignal::new(BTreeMap::new());
    let focus: RwSignal<Option<Focus>> = RwSignal::new(None);

    let outline = Memo::new(move |_| {
        controller.revision.track();
        let shown = shown_rows.get();
        let (workbook, names) = controller.with_untracked(|book| {
            let names = book
                .host()
                .defined_names()
                .iter()
                .map(|name| {
                    let formula = name.formula.strip_prefix('=').unwrap_or(&name.formula);
                    format!("{} = {formula}", name.name)
                })
                .collect();
            (book.name().to_owned(), names)
        });
        let sheets = controller.with_untracked(|book| {
            book.sheets()
                .into_iter()
                .map(|tab| {
                    let limit = shown.get(&tab.id).copied().unwrap_or(ROW_PAGE);
                    let cells = book.host().input_cells(tab.id);
                    outline_sheet(tab.id, tab.name, cells.into_iter(), limit)
                })
                .collect()
        });
        Outline {
            workbook,
            names,
            sheets,
        }
    });
    let rows =
        Memo::new(move |_| outline.with(|outline| folded.with(|f| visible_rows(outline, f))));

    // The outline follows the active cell (the grid's, the formula bar's, an undo's), unfolding
    // its sheet.
    Effect::new(move |_| {
        if let Some(at) = controller.active_key() {
            focus.set(Some(Focus::Cell(at)));
            if folded.with_untracked(|f| f.contains(&Branch::Sheet(at.sheet()))) {
                folded.update(|f| apply_fold(f, Branch::Sheet(at.sheet()), false));
            }
        }
    });

    let sheet_name = move |sheet: SheetId| -> String {
        outline.with_untracked(|outline| {
            outline
                .sheets
                .iter()
                .find(|s| s.id == sheet)
                .map(|s| s.name.clone())
                .unwrap_or_default()
        })
    };
    let activate = move |row: &TreeRow| match row {
        TreeRow::Cell { at } => controller.go_to(*at),
        TreeRow::Branch { branch, .. } => folded.update(|f| toggle_fold(f, *branch)),
        TreeRow::MoreRows { sheet, .. } => shown_rows.update(|shown| {
            *shown.entry(*sheet).or_insert(ROW_PAGE) += ROW_PAGE;
        }),
        TreeRow::Name { .. } | TreeRow::MoreCells { .. } => {}
    };
    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        let lines = rows.get_untracked();
        if lines.is_empty() {
            return;
        }
        let at = focus
            .get_untracked()
            .and_then(|f| lines.iter().position(|row| row.focus() == f));
        let index = at.unwrap_or(0);
        match ev.key().as_str() {
            "ArrowDown" => {
                ev.prevent_default();
                let next = if at.is_some() {
                    (index + 1).min(lines.len() - 1)
                } else {
                    0
                };
                focus.set(Some(lines[next].focus()));
            }
            "ArrowUp" => {
                ev.prevent_default();
                focus.set(Some(lines[index.saturating_sub(1)].focus()));
            }
            "Home" => {
                ev.prevent_default();
                focus.set(Some(lines[0].focus()));
            }
            "End" => {
                ev.prevent_default();
                focus.set(Some(lines[lines.len() - 1].focus()));
            }
            "ArrowLeft" => {
                ev.prevent_default();
                match &lines[index] {
                    TreeRow::Branch {
                        branch,
                        folded: false,
                        ..
                    } => folded.update(|f| apply_fold(f, *branch, true)),
                    row => {
                        if let Some(up) = parent(row.focus()) {
                            focus.set(Some(Focus::Branch(up)));
                        }
                    }
                }
            }
            "ArrowRight" => {
                ev.prevent_default();
                if let TreeRow::Branch {
                    branch,
                    folded: true,
                    ..
                } = &lines[index]
                {
                    folded.update(|f| apply_fold(f, *branch, false));
                }
            }
            "Enter" => {
                ev.prevent_default();
                activate(&lines[index]);
            }
            _ => {}
        }
    };

    let line = move |row: TreeRow| {
        let key = row.focus();
        let depth = row.depth();
        let focused = move || focus.get() == Some(key);
        let indent = format!("padding-left:{}px", 6 + 18 * u32::from(depth));
        let clicked = row.clone();
        let on_click = move |_| {
            focus.set(Some(clicked.focus()));
            if !matches!(clicked, TreeRow::Branch { .. }) {
                activate(&clicked);
            }
        };
        match row {
            TreeRow::Branch {
                branch,
                label,
                count,
                folded: is_folded,
                ..
            } => {
                let (kind, data_key, counted) = match branch {
                    Branch::Workbook => ("workbook", "workbook".to_owned(), format!("{count} sheets")),
                    Branch::Names => ("names", "names".to_owned(), format!("{count}")),
                    Branch::Sheet(_) => ("sheet", format!("sheet:{label}"), format!("{count} cells")),
                    Branch::Row(sheet, row) => {
                        let name = sheet_name(sheet);
                        ("row", format!("row:{name}!{row}"), format!("{count} cells"))
                    }
                };
                let toggle = move |ev: leptos::ev::MouseEvent| {
                    ev.stop_propagation();
                    focus.set(Some(Focus::Branch(branch)));
                    folded.update(|f| toggle_fold(f, branch));
                };
                view! {
                    <li data-dnacalc="tree-row" data-kind=kind data-key=data_key data-folded=is_folded.to_string()
                        data-focused=move || focused().to_string() style=indent on:click=on_click>
                        <button type="button" class="dc-twisty" data-dnacalc="tree-toggle" on:click=toggle>
                            {if is_folded { "+" } else { "-" }}
                        </button>
                        <span class="dc-label">{label}</span>
                        <span class="dc-count">{counted}</span>
                    </li>
                }
                .into_any()
            }
            TreeRow::Name { text, .. } => view! {
                <li data-dnacalc="tree-row" data-kind="name" data-focused=move || focused().to_string()
                    style=indent on:click=on_click>
                    <span class="dc-input">{text}</span>
                </li>
            }
            .into_any(),
            TreeRow::Cell { at } => {
                let (input, value, provenance, sheet_name) = controller.with_untracked(|book| {
                    let profile = book.profile().cloned();
                    let sheet_name = sheet_name(at.sheet());
                    match &book.cell(at) {
                        Some(cell) => {
                            let shown = cells::shown(cell, profile.as_ref(), book.cell_format(at));
                            let value = if shown.provenance == Provenance::Refused {
                                "refused".to_owned()
                            } else {
                                shown.text
                            };
                            (
                                cells::input_text(cell.input()),
                                value,
                                shown.provenance,
                                sheet_name,
                            )
                        }
                        None => (String::new(), String::new(), Provenance::Empty, sheet_name),
                    }
                });
                let address = cells::address(at.row(), at.column());
                let data_cell = address.clone();
                // The dependents of the focused cell only, read again after each change.
                let read_by = move || {
                    if !focused() {
                        return String::new();
                    }
                    controller.revision.track();
                    match controller.with_untracked(|book| book.host().dependents(at).len()) {
                        0 => "read by no formula".to_owned(),
                        1 => "read by 1 formula".to_owned(),
                        n => format!("read by {n} formulas"),
                    }
                };
                view! {
                    <li data-dnacalc="tree-row" data-kind="cell" data-sheet=sheet_name data-cell=data_cell
                        data-provenance=provenance.name() data-focused=move || focused().to_string()
                        style=indent on:click=on_click>
                        <span class="dc-label">{address}</span>
                        <span class="dc-input" data-dnacalc="tree-input">{input}</span>
                        <span class="dc-tvalue" data-dnacalc="tree-value">{value}</span>
                        <span class="dc-chip" data-provenance=provenance.name()>{provenance.name()}</span>
                        <span class="dc-count" data-dnacalc="tree-dependents">{read_by}</span>
                    </li>
                }
                .into_any()
            }
            TreeRow::MoreRows { hidden, .. } => view! {
                <li data-dnacalc="tree-row" data-kind="more" data-focused=move || focused().to_string() style=indent>
                    <button type="button" data-dnacalc="tree-more" on:click=on_click>
                        {format!("Show more rows ({hidden} not shown)")}
                    </button>
                </li>
            }
            .into_any(),
            TreeRow::MoreCells { hidden, .. } => view! {
                <li data-dnacalc="tree-row" data-kind="more-cells" data-focused=move || focused().to_string() style=indent>
                    <span class="dc-count">{format!("{hidden} more cells in this row (see the grid)")}</span>
                </li>
            }
            .into_any(),
        }
    };

    view! {
        <section class="dc-tree" data-skin="tree" data-dnacalc="tree" tabindex="0" on:keydown=on_key>
            <style>{TREE_CSS}</style>
            <ul>{move || rows.get().into_iter().map(line).collect_view()}</ul>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_host::value::IdAllocator;

    fn sheets() -> (SheetId, SheetId) {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        (ids.issue().unwrap(), ids.issue().unwrap())
    }

    fn at(sheet: SheetId, row: u32, column: u32) -> CellKey {
        CellKey::new(sheet, row, column).unwrap()
    }

    fn outline() -> (Outline, SheetId, SheetId) {
        let (one, two) = sheets();
        let cells = [at(one, 1, 1), at(one, 1, 2), at(one, 3, 1)];
        let outline = Outline {
            workbook: "Book1.xlsx".to_owned(),
            names: vec!["Rate = 0.05".to_owned()],
            sheets: vec![
                outline_sheet(one, "Data".to_owned(), cells.into_iter(), ROW_PAGE),
                outline_sheet(
                    two,
                    "Other".to_owned(),
                    [at(two, 2, 2)].into_iter(),
                    ROW_PAGE,
                ),
            ],
        };
        (outline, one, two)
    }

    fn focuses(rows: &[TreeRow]) -> Vec<Focus> {
        rows.iter().map(TreeRow::focus).collect()
    }

    /// Ported from tree.rs `visible_rows_walk_depth_first_in_engine_child_order`: the workbook,
    /// its names, then each sheet with its rows and cells in order, at their depths.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn visible_rows_walk_the_workbook_depth_first() {
        let (outline, one, two) = outline();
        let rows = visible_rows(&outline, &BTreeSet::new());
        assert_eq!(
            focuses(&rows),
            vec![
                Focus::Branch(Branch::Workbook),
                Focus::Branch(Branch::Names),
                Focus::Name(0),
                Focus::Branch(Branch::Sheet(one)),
                Focus::Branch(Branch::Row(one, 1)),
                Focus::Cell(at(one, 1, 1)),
                Focus::Cell(at(one, 1, 2)),
                Focus::Branch(Branch::Row(one, 3)),
                Focus::Cell(at(one, 3, 1)),
                Focus::Branch(Branch::Sheet(two)),
                Focus::Branch(Branch::Row(two, 2)),
                Focus::Cell(at(two, 2, 2)),
            ]
        );
        let depths: Vec<u8> = rows.iter().map(TreeRow::depth).collect();
        assert_eq!(depths, vec![0, 1, 2, 1, 2, 3, 3, 2, 3, 1, 2, 3]);
        assert!(matches!(&rows[3], TreeRow::Branch { count: 3, label, .. } if label == "Data"));
    }

    /// Ported from tree.rs `visible_rows_fold_skips_descendants`.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn folding_a_branch_skips_what_it_holds() {
        let (outline, one, two) = outline();
        let folded = BTreeSet::from([Branch::Sheet(one), Branch::Names]);
        let rows = visible_rows(&outline, &folded);
        assert_eq!(
            focuses(&rows),
            vec![
                Focus::Branch(Branch::Workbook),
                Focus::Branch(Branch::Names),
                Focus::Branch(Branch::Sheet(one)),
                Focus::Branch(Branch::Sheet(two)),
                Focus::Branch(Branch::Row(two, 2)),
                Focus::Cell(at(two, 2, 2)),
            ]
        );
        assert!(matches!(&rows[2], TreeRow::Branch { folded: true, .. }));
        let rows = visible_rows(&outline, &BTreeSet::from([Branch::Workbook]));
        assert_eq!(rows.len(), 1);
    }

    /// Ported from tree.rs `fold_helpers_update_the_shared_set`.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fold_helpers_are_idempotent() {
        let (one, _) = sheets();
        let mut folded = BTreeSet::new();
        apply_fold(&mut folded, Branch::Sheet(one), true);
        apply_fold(&mut folded, Branch::Sheet(one), true);
        assert_eq!(folded.len(), 1);
        apply_fold(&mut folded, Branch::Sheet(one), false);
        assert!(folded.is_empty());
        toggle_fold(&mut folded, Branch::Workbook);
        assert!(folded.contains(&Branch::Workbook));
        toggle_fold(&mut folded, Branch::Workbook);
        assert!(folded.is_empty());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_sheet_shows_its_first_rows_and_counts_the_rest() {
        let (one, _) = sheets();
        let cells = (1..=5).flat_map(|row| [at(one, row, 1), at(one, row, 2)]);
        let sheet = outline_sheet(one, "Data".to_owned(), cells, 2);
        assert_eq!(sheet.cells, 10);
        assert_eq!(
            sheet.rows,
            vec![
                (1, vec![at(one, 1, 1), at(one, 1, 2)]),
                (2, vec![at(one, 2, 1), at(one, 2, 2)])
            ]
        );
        assert_eq!(sheet.more_rows, 3);
        let outline = Outline {
            workbook: "Book1.xlsx".to_owned(),
            names: Vec::new(),
            sheets: vec![sheet],
        };
        let rows = visible_rows(&outline, &BTreeSet::new());
        assert_eq!(
            rows.last(),
            Some(&TreeRow::MoreRows {
                sheet: one,
                hidden: 3
            })
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_line_knows_its_parent() {
        let (one, _) = sheets();
        assert_eq!(parent(Focus::Branch(Branch::Workbook)), None);
        assert_eq!(
            parent(Focus::Branch(Branch::Sheet(one))),
            Some(Branch::Workbook)
        );
        assert_eq!(
            parent(Focus::Cell(at(one, 4, 2))),
            Some(Branch::Row(one, 4))
        );
        assert_eq!(
            parent(Focus::Branch(Branch::Row(one, 4))),
            Some(Branch::Sheet(one))
        );
        assert_eq!(parent(Focus::Name(0)), Some(Branch::Names));
    }
}
