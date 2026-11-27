//! The Classic Sort dialog uses the workbook's current published values and the shared review seam.

use crate::{Live, Seam, Session, ViewCommand};
use calc_skin::grid::{Area, Key};
use calc_skin::{
    Batch, Change, Interest, ModelChange, Revision, RowSort, SourceRef, ViewId, WorkbookId,
    address, query,
};
use leptos::prelude::*;

pub const OPEN: &str = "classic.sort";
const REVIEW: &str = "classic.sort-review";

#[derive(Clone, Copy, PartialEq, Eq)]
struct Target {
    workbook: WorkbookId,
    revision: Revision,
    selected: Area,
}

#[derive(Clone)]
struct Draft {
    target: Target,
    levels: Vec<RowSort>,
    header: bool,
    case_sensitive: bool,
}

#[derive(Clone, Copy, Default)]
struct SortDialog {
    draft: RwSignal<Option<Draft>>,
    element: NodeRef<leptos::html::Dialog>,
}

pub fn commands(view: &ViewId, session: Session) -> [ViewCommand; 2] {
    let dialog = session.local::<SortDialog>();
    [
        ViewCommand::new(view, OPEN, "Sort", &[], move |_| {
            let selection = session.selection().ok_or("Select the range to sort.")?;
            let SourceRef::Cells { areas, .. } = selection.target else {
                return Err("Select a rectangular range to sort.".into());
            };
            let [selected] = areas.as_slice() else {
                return Err("Select one rectangular range to sort.".into());
            };
            let stamp = session.stamp().ok_or("The workbook is unavailable.")?;
            dialog.draft.set(Some(Draft {
                target: Target {
                    workbook: stamp.workbook,
                    revision: stamp.model,
                    selected: *selected,
                },
                levels: vec![RowSort {
                    column: 0,
                    descending: false,
                }],
                header: false,
                case_sensitive: false,
            }));
            Ok(())
        })
        .when("mode:ready && writable"),
        ViewCommand::new(view, REVIEW, "Review sort", &[], move |_| {
            let draft = dialog
                .draft
                .get_untracked()
                .ok_or("Open Sort to choose keys.")?;
            if !matches!(session.seam(), Seam::Idle) {
                return Err("Finish reviewing the current edit first.".into());
            }
            let Target {
                workbook,
                revision,
                selected,
            } = draft.target;
            if !session
                .stamp()
                .is_some_and(|stamp| stamp.workbook == workbook)
            {
                return Err("Reopen Sort for the current workbook.".into());
            }
            close(session);
            session.submit_for_review(Batch {
                id: String::new(),
                workbook: Some(workbook),
                base: Some(revision),
                expects: Vec::new(),
                selection_seq: None,
                label: Some("Sort rows".into()),
                changes: vec![Change::Model(ModelChange::SortRows {
                    selected,
                    keys: draft.levels,
                    header: draft.header,
                    case_sensitive: draft.case_sensitive,
                })],
            });
            Ok(())
        })
        .when("mode:ready && writable"),
    ]
}

pub fn close(session: Session) -> bool {
    let dialog = session.local::<SortDialog>();
    if dialog.draft.with_untracked(Option::is_none) {
        return false;
    }
    if let Some(element) = dialog.element.get_untracked() {
        element.close();
    }
    dialog.draft.set(None);
    true
}

pub fn mount(session: Session) -> impl IntoView {
    let dialog = session.local::<SortDialog>();
    let target = Memo::new(move |_| {
        let stamp = session.stamp()?;
        dialog.draft.with(|draft| {
            draft
                .as_ref()
                .map(|draft| draft.target)
                .filter(|target| target.workbook == stamp.workbook)
        })
    });
    move || target.get().map(|target| dialog.show(session, target))
}

impl SortDialog {
    fn show(self, session: Session, target: Target) -> impl IntoView {
        crate::dialog::show_modal(self.element);
        let Area(first, last) = target.selected;
        let header = Area(first, Key(first.0, first.1, first.2, last.3));
        let watch = session.watch(query::Cells {
            interest: Interest::Area(header),
        });
        on_cleanup(move || session.unwatch(watch));

        let columns = last.3 - first.3 + 1;
        let labels = Memo::new(move |_| {
            let mut labels: Vec<_> = (first.3..=last.3)
                .map(|column| format!("Column {}", address::column_letters(column)))
                .collect();
            if self
                .draft
                .with(|draft| draft.as_ref().is_some_and(|draft| draft.header))
                && let Live::Ready(header) = session.read(watch)
            {
                for cell in header.body.cells {
                    if let Some(index) = cell.at.3.checked_sub(first.3)
                        && let Some(label) = labels.get_mut(index as usize)
                        && !cell.text.is_empty()
                    {
                        *label = cell.text;
                    }
                }
            }
            labels
        });
        let header_enabled = move || {
            self.draft
                .with(|draft| draft.as_ref().is_some_and(|draft| draft.header))
        };
        let case_enabled = move || {
            self.draft
                .with(|draft| draft.as_ref().is_some_and(|draft| draft.case_sensitive))
        };
        let level_count = move || {
            self.draft
                .with(|draft| draft.as_ref().map_or(0, |draft| draft.levels.len()))
        };
        let level_limit = columns.min(32) as usize;
        let full = move || level_count() >= level_limit;

        view! {
            <dialog class="dna-workbook-dialog dna-sort-dialog" node_ref=self.element data-dnacalc="classic-sort" data-dna-action-field aria-labelledby="dna-sort-title"
                on:cancel=move |event: leptos::web_sys::Event| { event.prevent_default(); close(session); }>
                <h2 id="dna-sort-title">"Sort"</h2>
                <form on:submit=move |event| { event.prevent_default(); let _ = session.command(REVIEW); }>
                    <label>
                        <input type="checkbox" data-dnacalc="classic-sort-header" prop:checked=header_enabled
                            on:change=move |event| self.draft.update(|draft| {
                                if let Some(draft) = draft { draft.header = event_target_checked(&event); }
                            })/>
                        "My data has headers"
                    </label>
                    <For each=move || 0..level_count() key=|index| *index
                        children=move |index| self.level(index, labels) />
                    <button type="button" data-dnacalc="classic-sort-add-level" disabled=full
                        on:click=move |_| self.draft.update(|draft| {
                            if let Some(draft) = draft
                                && let Some(column) = (0..columns).find(|column| draft.levels.iter().all(|key| key.column != *column))
                            {
                                draft.levels.push(RowSort { column, descending: false });
                            }
                        })>"Add Level"</button>
                    <label>
                        <input type="checkbox" prop:checked=case_enabled
                            on:change=move |event| self.draft.update(|draft| {
                                if let Some(draft) = draft { draft.case_sensitive = event_target_checked(&event); }
                            })/>
                        "Case sensitive"
                    </label>
                    <div class="dna-sort-actions">
                        <button type="button" on:click=move |_| { close(session); }>"Cancel"</button>
                        <button type="submit" data-dnacalc="classic-sort-review" data-command=REVIEW>"Review sort"</button>
                    </div>
                </form>
            </dialog>
        }
    }

    fn level(self, index: usize, labels: Memo<Vec<String>>) -> impl IntoView {
        let key = move || {
            self.draft.with(|draft| {
                draft
                    .as_ref()
                    .and_then(|draft| draft.levels.get(index))
                    .copied()
            })
        };
        view! {
            <div class="dna-sort-level">
                <label>{if index == 0 { "Sort by" } else { "Then by" }}
                    <select data-dnacalc="classic-sort-key"
                        prop:value=move || key().map_or_else(String::new, |key| key.column.to_string())
                        on:change=move |event| {
                            if let Ok(column) = event_target_value(&event).parse::<u32>() {
                                self.draft.update(|draft| {
                                    if let Some(key) = draft.as_mut().and_then(|draft| draft.levels.get_mut(index)) {
                                        key.column = column;
                                    }
                                });
                            }
                        }>
                        {move || labels.get().into_iter().enumerate().map(|(column, label)| view! {
                            <option value=column.to_string()
                                selected=move || key().is_some_and(|key| key.column as usize == column)
                                disabled=move || self.draft.with(|draft| draft.as_ref().is_some_and(|draft| {
                                    draft.levels.iter().enumerate().any(|(i, key)| i != index && key.column as usize == column)
                                }))>{label}</option>
                        }).collect_view()}
                    </select>
                </label>
                <label>"Order"
                    <select prop:value=move || if key().is_some_and(|key| key.descending) { "descending" } else { "ascending" }
                        on:change=move |event| self.draft.update(|draft| {
                            if let Some(key) = draft.as_mut().and_then(|draft| draft.levels.get_mut(index)) {
                                key.descending = event_target_value(&event) == "descending";
                            }
                        })>
                        <option value="ascending">"Ascending"</option>
                        <option value="descending">"Descending"</option>
                    </select>
                </label>
                <button type="button" disabled=move || self.draft.with(|draft| draft.as_ref().is_none_or(|draft| draft.levels.len() < 2))
                    on:click=move |_| self.draft.update(|draft| {
                        if let Some(draft) = draft && draft.levels.len() > 1 && index < draft.levels.len() {
                            draft.levels.remove(index);
                        }
                    })>"Remove level"</button>
            </div>
        }
    }
}
