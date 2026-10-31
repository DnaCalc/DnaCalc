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
//! The outline is asked of the workbook's server after each change (dc-p96): which cells of each
//! sheet hold an input (from the document, without
//! projecting them), and the cells of the unfolded sheets' shown rows, as the page shows them. A sheet shows [`ROW_PAGE`] rows at first and
//! more on request; a row shows at most [`ROW_CELLS`] cells; every sheet but the active one starts
//! folded. So a change costs a walk over the workbook's cell keys and the shown cells' values,
//! not a projection of every cell (measured on the 165,810-formula default tariff cap workbook,
//! dc-gf0). Published spill children are included in a bounded window, with omissions counted. The skin
//! changes selection; formula editing uses the shared shell editor. Folds survive a skin return.
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
use crate::style::css;
use crate::wire;
use calc_value::{CellKey, SheetId};
use leptos::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default)]
pub struct TreeSkin;

impl Skin for TreeSkin {
    fn id(&self) -> SkinId {
        SkinId::Tree
    }

    fn title(&self) -> &'static str {
        "Tree"
    }

    fn focus_target(&self) -> &'static str {
        "[data-dnacalc=tree]"
    }

    fn mount(&self, ctx: SkinContext) -> AnyView {
        tree_view(ctx.controller).into_any()
    }
}

pub use dna_calc_model::{Branch, TreeFocus as Focus};

pub use dna_calc_model::tree::*;

/// Page-owned view state; the workbook installation resets it.
#[derive(Clone, Copy)]
pub struct ViewState {
    folded: crate::model_adapter::ModelField<BTreeSet<Branch>>,
    shown_rows: crate::model_adapter::ModelField<BTreeMap<SheetId, usize>>,
    focus: crate::model_adapter::ModelField<Option<Focus>>,
    query: crate::model_adapter::ModelField<String>,
    initialized: crate::model_adapter::ModelField<bool>,
    last_active: crate::model_adapter::ModelField<Option<CellKey>>,
}
impl ViewState {
    pub fn new(controller: Controller) -> Self {
        Self {
            folded: controller.view_field::<dna_calc_model::TreeState, _>(
                SkinId::Tree,
                |s| &s.folded,
                |s| &mut s.folded,
            ),
            shown_rows: controller.view_field::<dna_calc_model::TreeState, _>(
                SkinId::Tree,
                |s| &s.shown_rows,
                |s| &mut s.shown_rows,
            ),
            focus: controller.view_field::<dna_calc_model::TreeState, _>(
                SkinId::Tree,
                |s| &s.focus,
                |s| &mut s.focus,
            ),
            query: controller.view_field::<dna_calc_model::TreeState, _>(
                SkinId::Tree,
                |s| &s.query,
                |s| &mut s.query,
            ),
            initialized: controller.view_field::<dna_calc_model::TreeState, _>(
                SkinId::Tree,
                |s| &s.initialized,
                |s| &mut s.initialized,
            ),
            last_active: controller.view_field::<dna_calc_model::TreeState, _>(
                SkinId::Tree,
                |s| &s.last_active,
                |s| &mut s.last_active,
            ),
        }
    }
}

fn item_id(key: Focus) -> String {
    format!("dc-tree-{key:?}").replace([' ', '(', ')', ','], "-")
}

#[cfg(target_arch = "wasm32")]
fn reveal_focus(focus: crate::model_adapter::ModelField<Option<Focus>>) {
    if let Some(selected) = focus.get_untracked() {
        request_animation_frame(move || {
            use wasm_bindgen::JsCast;
            let Some(line) = document().get_element_by_id(&item_id(selected)) else {
                return;
            };
            let Ok(Some(tree)) = document().query_selector("[data-dnacalc=tree]") else {
                return;
            };
            let Some(tree) = tree.dyn_ref::<web_sys::HtmlElement>() else {
                return;
            };
            let rect = line.get_bounding_client_rect();
            let bounds = tree.get_bounding_client_rect();
            let delta = if rect.top() < bounds.top() {
                rect.top() - bounds.top()
            } else if rect.bottom() > bounds.bottom() {
                rect.bottom() - bounds.bottom()
            } else {
                0.
            };
            if delta != 0. {
                tree.set_scroll_top(
                    tree.scroll_top()
                        + if delta > 0. {
                            delta.ceil() as i32
                        } else {
                            delta.floor() as i32
                        },
                );
            }
        });
    }
}

fn tree_view(controller: Controller) -> impl IntoView {
    let ViewState {
        folded,
        shown_rows,
        focus,
        query,
        initialized,
        last_active,
    } = ViewState::new(controller);
    if !initialized.get_untracked() {
        let active = controller.sheet.get_untracked();
        folded.set(controller.summary.with_untracked(|s| {
            s.sheets
                .iter()
                .map(|(s, _)| s.id())
                .filter(|s| Some(*s) != active)
                .map(Branch::Sheet)
                .collect()
        }));
        folded.update(|f| {
            f.insert(Branch::Names);
        });
        initialized.set(true);
    }
    let outline = RwSignal::new(Outline::default());
    let omitted = RwSignal::new(0u64);
    let shown_cells = RwSignal::new(BTreeMap::<CellKey, wire::Cell>::new());
    let epoch = StoredValue::new(0u64);
    let ready = RwSignal::new(false);
    let reveal = RwSignal::new(controller.active_key_untracked());
    let details = RwSignal::new(false);
    let read_by = RwSignal::new(String::new());
    let reveal_active = move || {
        if let Some(at) = controller.active_key_untracked() {
            query.set(String::new());
            reveal.set(Some(at));
            focus.set(Some(Focus::Cell(at)));
            folded.update(|f| {
                f.remove(&Branch::Workbook);
                f.remove(&Branch::Sheet(at.sheet()));
                f.remove(&Branch::Row(at.sheet(), at.row()));
            });
        }
    };
    Effect::new(move |_| {
        let active = controller.active_key();
        if active != last_active.get_untracked() {
            last_active.set(active);
            reveal_active();
        }
    });
    Effect::new(move |_| {
        controller.revision.track();
        controller.summary.track();
        ready.set(false);
        let shown = shown_rows.with(|s| {
            s.iter()
                .map(|(id, n)| (wire::Sheet::of(*id), *n as u64))
                .collect()
        });
        let needle = query.get();
        let open = controller.summary.with_untracked(|s| {
            s.sheets
                .iter()
                .map(|(s, _)| *s)
                .filter(|s| {
                    !needle.is_empty() || !folded.with(|f| f.contains(&Branch::Sheet(s.id())))
                })
                .collect()
        });
        let generation = controller.generation.get_untracked();
        epoch.update_value(|e| *e += 1);
        let sent = epoch.get_value();
        controller.ask(
            wire::Request::Outline {
                shown,
                open,
                reveal: reveal.get().map(wire::Key::of),
                query: needle,
            },
            move |reply| {
                if controller.generation.get_untracked() != generation
                    || epoch.try_get_value() != Some(sent)
                {
                    return;
                }
                if let wire::Reply::Outline(found) = reply {
                    omitted.try_set(found.omitted);
                    outline.try_set(Outline {
                        workbook: found.workbook,
                        names: found.names,
                        sheets: found
                            .sheets
                            .into_iter()
                            .map(|s| OutlineSheet {
                                id: s.sheet.id(),
                                name: s.name,
                                cells: s.cells as usize,
                                rows: s
                                    .rows
                                    .into_iter()
                                    .map(|(row, keys)| {
                                        (row, keys.into_iter().filter_map(|k| k.cell()).collect())
                                    })
                                    .collect(),
                                more_rows: s.more_rows as usize,
                            })
                            .collect(),
                    });
                    shown_cells.try_set(
                        found
                            .cells
                            .into_iter()
                            .filter_map(|c| Some((c.at.cell()?, c)))
                            .collect(),
                    );
                    ready.try_set(true);
                }
            },
        );
    });
    let rows = Memo::new(move |_| {
        outline.with(|o| {
            if query.get().is_empty() {
                folded.with(|f| visible_rows_with_reveal(o, f, reveal.get()))
            } else {
                visible_rows(o, &BTreeSet::new())
            }
        })
    });
    Effect::new(move |_| {
        let lines = rows.get();
        if !ready.get() {
            return;
        }
        if focus
            .get()
            .is_none_or(|f| !lines.iter().any(|r| r.focus() == f))
        {
            focus.set(lines.first().map(TreeRow::focus));
        }
    });
    Effect::new(move |_| {
        focus.track();
        shown_cells.track();
        read_by.set(String::new());
        if details.get()
            && let Some(Focus::Cell(at)) = focus.get()
        {
            let revision = controller.revision.get();
            let generation = controller.generation.get_untracked();
            controller.ask(wire::Request::Dependents(wire::Key::of(at)), move |r| {
                if let wire::Reply::Keys(keys) = r {
                    if details.try_get_untracked() == Some(true)
                        && focus.get_untracked() == Some(Focus::Cell(at))
                        && controller.revision.get_untracked() == revision
                        && controller.generation.get_untracked() == generation
                    {
                        read_by.try_set(format!(
                            "read by {} {}",
                            keys.len(),
                            if keys.len() == 1 {
                                "formula"
                            } else {
                                "formulas"
                            }
                        ));
                    }
                }
            });
        }
    });
    let tree_node = NodeRef::<leptos::html::Div>::new();
    Effect::new(move |_| {
        focus.track();
        rows.track();
        details.track();
        #[cfg(target_arch = "wasm32")]
        reveal_focus(focus);
    });
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{JsCast, closure::Closure};
        let observer =
            StoredValue::new_local(None::<(web_sys::ResizeObserver, Closure<dyn FnMut()>)>);
        Effect::new(move |_| {
            if let Some(node) = tree_node.get()
                && observer.with_value(Option::is_none)
            {
                let callback = Closure::<dyn FnMut()>::new(move || reveal_focus(focus));
                if let Ok(watch) = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()) {
                    watch.observe(&node);
                    observer.set_value(Some((watch, callback)));
                }
            }
        });
        on_cleanup(move || {
            observer.with_value(|held| {
                if let Some((watch, _)) = held {
                    watch.disconnect();
                }
            })
        });
    }
    let activate = move |row: &TreeRow| match row {
        TreeRow::Cell { at } => controller.go_to(*at),
        TreeRow::Branch { branch, .. } => folded.update(|f| toggle_fold(f, *branch)),
        TreeRow::MoreRows { sheet, .. } => {
            shown_rows.update(|s| *s.entry(*sheet).or_insert(ROW_PAGE) += ROW_PAGE)
        }
        TreeRow::MoreCells { first, .. } => {
            controller.reveal(*first, SkinId::Grid);
        }
        _ => {}
    };
    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        let lines = rows.get_untracked();
        if lines.is_empty() {
            return;
        }
        let at = focus
            .get_untracked()
            .and_then(|f| lines.iter().position(|r| r.focus() == f))
            .unwrap_or(0);
        match ev.key().as_str() {
            "ArrowLeft" if matches!(lines[at], TreeRow::Branch { folded: false, .. }) => {
                ev.prevent_default();
                if let TreeRow::Branch { branch, .. } = lines[at] {
                    folded.update(|f| apply_fold(f, branch, true));
                }
            }
            "ArrowRight" if matches!(lines[at], TreeRow::Branch { folded: true, .. }) => {
                ev.prevent_default();
                if let TreeRow::Branch { branch, .. } = lines[at] {
                    folded.update(|f| apply_fold(f, branch, false));
                }
            }
            "ArrowDown" | "ArrowUp" | "ArrowLeft" | "ArrowRight" | "Home" | "End" => {
                ev.prevent_default();
                if let Some(f) = next_focus(&lines, focus.get_untracked(), &ev.key()) {
                    focus.set(Some(f));
                }
            }
            "Enter" | " " => {
                ev.prevent_default();
                activate(&lines[at]);
            }
            _ => {}
        }
    };
    let line = move |row: TreeRow| {
        let key = row.focus();
        let depth = row.depth();
        let focused = move || focus.get() == Some(key);
        let selected = move || matches!(key,Focus::Cell(at) if controller.active_key()==Some(at));
        let indent = format!("--dc-tree-depth:{depth}");
        let id = item_id(key);
        let clicked = row.clone();
        let click = move |_| {
            focus.set(Some(key));
            if !matches!(clicked, TreeRow::Branch { .. }) {
                activate(&clicked);
            }
        };
        let (kind, data_key, label, count, cell, expanded) = match row {
            TreeRow::Branch {
                branch,
                label,
                count,
                folded,
                ..
            } => {
                let name = outline.with_untracked(|o| {
                    o.sheets
                        .iter()
                        .find(|s| {
                            Some(s.id)
                                == match branch {
                                    Branch::Row(id, _) => Some(id),
                                    _ => None,
                                }
                        })
                        .map(|s| s.name.clone())
                        .unwrap_or_default()
                });
                let (kind, key, n) = match branch {
                    Branch::Workbook => ("workbook", "workbook".into(), format!("{count} sheets")),
                    Branch::Names => ("names", "names".into(), count.to_string()),
                    Branch::Sheet(_) => {
                        ("sheet", format!("sheet:{label}"), format!("{count} cells"))
                    }
                    Branch::Row(_, r) => {
                        ("row", format!("row:{name}!{r}"), format!("{count} cells"))
                    }
                };
                (kind, key, label, n, None, Some(!folded))
            }
            TreeRow::Cell { at } => (
                "cell",
                String::new(),
                cells::address(at.row(), at.column()),
                String::new(),
                shown_cells.with_untracked(|c| c.get(&at).cloned()),
                None,
            ),
            TreeRow::Name { text, .. } => ("name", String::new(), text, String::new(), None, None),
            TreeRow::MoreRows { hidden, .. } => (
                "more",
                String::new(),
                format!("Show more rows ({hidden} not shown)"),
                String::new(),
                None,
                None,
            ),
            TreeRow::MoreCells { hidden, .. } => (
                "more-cells",
                String::new(),
                format!("{hidden} more cells — open Grid"),
                String::new(),
                None,
                None,
            ),
        };
        let at = if let Focus::Cell(at) = key {
            Some(at)
        } else {
            None
        };
        let sheet = at.and_then(|at| {
            outline.with_untracked(|o| {
                o.sheets
                    .iter()
                    .find(|s| s.id == at.sheet())
                    .map(|s| s.name.clone())
            })
        });
        let branch = if let Focus::Branch(b) = key {
            Some(b)
        } else {
            None
        };
        let cell_input = cell.as_ref().map(|c| {
            if c.input.is_empty() && c.provenance != Provenance::Empty {
                "Child value".into()
            } else {
                c.input.clone()
            }
        });
        let redundant_input = cell
            .as_ref()
            .is_some_and(|c| c.provenance == Provenance::Literal && c.input == c.text);
        let value = cell.as_ref().map(|c| c.text.clone());
        let prov = cell.as_ref().map(|c| c.provenance.name());
        view! {<li role="treeitem" id=id aria-expanded=expanded.map(|x|x.to_string()) aria-selected=move ||selected().to_string() data-dnacalc="tree-row" data-kind=kind data-key=data_key data-focused=move ||focused().to_string() data-folded=expanded.map(|x|(!x).to_string()) data-sheet=sheet data-cell=at.map(|at|cells::address(at.row(),at.column())) data-provenance=prov data-redundant-input=redundant_input.to_string() use:css=indent on:click=click>
            <span class="dc-tree-label">{branch.map(|b|view!{<button class="dc-twisty" tabindex="-1" aria-label=if expanded==Some(true){"Collapse branch"}else{"Expand branch"} data-dnacalc="tree-toggle" on:click=move |ev|{ev.stop_propagation();focus.set(Some(key));folded.update(|f|toggle_fold(f,b));}>{if expanded==Some(true){"−"}else{"+"}}</button>})}<span class="dc-label">{label}</span><span class="dc-count">{count}</span></span>
            {cell_input.map(|text|view!{<span class="dc-input" data-dnacalc="tree-input">{text}</span>})}{value.map(|text|view!{<span class="dc-tvalue" data-dnacalc="tree-value">{text}</span>})}{prov.map(|p|view!{<span class="dc-chip" data-provenance=p>{p}</span>})}
        </li>}.attr("aria-level",(depth+1).to_string()).into_any()
    };
    let inspected = move || {
        focus.get().and_then(|f| {
            if let Focus::Cell(at) = f {
                shown_cells.with(|c| c.get(&at).cloned())
            } else {
                None
            }
        })
    };
    view! {<section class="dc-tree-skin" data-skin="tree">
        <div class="dc-tree-tools"><label>"Find source "<input data-dnacalc="tree-search" type="search" placeholder="Address, sheet, name or input" prop:value=move ||query.get() on:input=move |ev|query.set(event_target_value(&ev))/></label><button data-dnacalc="tree-reveal" on:click=move |_|reveal_active()>"Reveal active cell"</button><button data-dnacalc="tree-details-toggle" aria-expanded=move ||details.get().to_string() on:click=move |_|details.update(|v|*v=!*v)>"Details"</button></div>
        <details class="dc-tree-guide"><summary>"About this outline"</summary><p class="dc-tree-caption">"Worksheet outline · physical rows; grouping does not change formula scope."</p></details>
        <div class="dc-tree-head" aria-hidden="true"><span>"Outline / address"</span><span>"Input"</span><span>"Result"</span><span>"State"</span></div>
        <div class="dc-tree" node_ref=tree_node data-dnacalc="tree" role="tree" aria-label="Workbook outline" aria-activedescendant=move ||focus.get().filter(|f|rows.with(|r|r.iter().any(|r|r.focus()==*f))).map(item_id) tabindex="0" on:keydown=on_key><ul role="none">{move ||{shown_cells.track();rows.get().into_iter().map(line).collect_view()}}</ul></div>
        {move ||(omitted.get()>0).then(||view!{<p data-dnacalc="tree-omitted">{format!("{} occupied cells beyond the spill window; reveal an address or open Grid.",omitted.get())}</p>})}
        {move ||details.get().then(||view!{<aside class="dc-tree-detail" data-dnacalc="tree-details">{move ||inspected().map(|cell|view!{<p>{cell.why}{cell.error_meaning.map(|s|format!(" {s}")).unwrap_or_default()}</p><p data-dnacalc="tree-dependents">{move ||read_by.get()}</p><button data-dnacalc="tree-open-grid" on:click=move |_|{if let Some(at)=cell.at.cell(){controller.reveal(at, SkinId::Grid);}}>"Open source in Grid"</button>})}</aside>})}
    </section>}
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_value::IdAllocator;

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

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn navigation_enters_children_and_returns_to_parent_without_changing_selection() {
        let (outline, one, _) = outline();
        let lines = visible_rows(&outline, &BTreeSet::new());
        assert_eq!(
            next_focus(&lines, Some(Focus::Branch(Branch::Workbook)), "ArrowRight"),
            Some(Focus::Branch(Branch::Names))
        );
        assert_eq!(
            next_focus(&lines, Some(Focus::Cell(at(one, 1, 2))), "ArrowLeft"),
            Some(Focus::Branch(Branch::Row(one, 1)))
        );
        assert_eq!(next_focus(&[], None, "End"), None);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn revealing_a_sparse_row_keeps_column_order_and_counts_only_hidden_sources() {
        let (one, _) = sheets();
        let outline = Outline {
            workbook: "Book".into(),
            names: vec![],
            sheets: vec![outline_sheet(
                one,
                "Data".into(),
                (3..=34).map(|c| at(one, 1, c)),
                ROW_PAGE,
            )],
        };
        let lines = visible_rows_with_reveal(&outline, &BTreeSet::new(), Some(at(one, 1, 34)));
        let cells: Vec<_> = lines
            .iter()
            .filter_map(|r| {
                if let TreeRow::Cell { at } = r {
                    Some(at.column())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(cells, (3..=32).chain([34]).collect::<Vec<_>>());
        assert!(
            lines.iter().any(
                |r| matches!(r,TreeRow::MoreCells {hidden:1,first,..} if *first==at(one,1,33))
            )
        );
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
