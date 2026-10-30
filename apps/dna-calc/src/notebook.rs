//! The notebook skin: the active sheet's cells as one column of blocks, in row order, each with
//! its address, what it holds (a formula, a value, or a value an array formula spilled), the text
//! that was entered, and its value with that value's provenance. A block's text is edited in place
//! and entered with Enter, one undoable edit through calc-host, as the grid enters it; a refused
//! entry keeps the text and shows why.
//!
//! The skin subscribes to the whole sheet while it is shown and pages through the cells its mirror
//! holds, [`PAGE`] blocks at a time; each cell comes from the workbook's server as the page shows
//! it (dc-p96: its text, provenance, input and the words for where its value came from).
//!
//! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnacalc-stage-notebook/src/lib.rs` and
//! `src/model.rs` (`derive_entries`: one block per authored cell in row order, the entry kinds,
//! the per-block editor and its outcome chip). Left behind: the personas, the reference X-ray,
//! scenario chips and the defined-name form, none of which calc-host offers. The notebook view of
//! calc-doc's skin layout (blocks placed in a notebook order) is not read by calc-host yet, so the
//! notebook lists the sheet's cells.

use crate::cells::{self, Provenance};
use crate::controller::{Controller, ViewMirror};
use crate::skin::{Skin, SkinContext, SkinId};
use crate::wire;
use leptos::prelude::*;

/// The blocks one page shows.
pub const PAGE: usize = 40;

#[derive(Clone, Copy, Debug, Default)]
pub struct NotebookSkin;

impl Skin for NotebookSkin {
    fn id(&self) -> SkinId {
        SkinId::Notebook
    }

    fn title(&self) -> &'static str {
        "Notebook"
    }

    fn mount(&self, ctx: SkinContext) -> AnyView {
        notebook_view(ctx.controller).into_any()
    }
}

/// What a block holds.
pub fn block_kind(shown: &cells::CellShown, has_input: bool) -> &'static str {
    match (shown.formula, has_input) {
        (true, _) => "formula",
        (false, true) => "value",
        (false, false) => "spilled value",
    }
}

fn notebook_view(controller: Controller) -> impl IntoView {
    let mirror = RwSignal::new(ViewMirror::default());
    let subscription: StoredValue<Option<(u64, u64)>> = StoredValue::new(None);
    let page = RwSignal::new(0usize);
    Effect::new(move |_| {
        let generation = controller.generation.get();
        let Some(sheet) = controller.sheet.get() else {
            return;
        };
        page.set(0);
        match subscription.get_value() {
            Some((held, id)) if held == generation => {
                controller.set_interest(id, wire::Interest::Sheet(wire::Sheet::of(sheet)));
            }
            _ => {
                let id =
                    controller.subscribe(wire::Interest::Sheet(wire::Sheet::of(sheet)), mirror);
                subscription.set_value(Some((generation, id)));
            }
        }
    });
    on_cleanup(move || {
        if let Some((generation, id)) = subscription.get_value()
            && generation == controller.generation.get_untracked()
        {
            controller.unsubscribe(id);
        }
    });
    // Only the shown sheet's cells: until the interest moves to a sheet just shown, the mirror still
    // holds the previous sheet's.
    let on_sheet = move |at: &calc_host::value::CellKey| Some(at.sheet()) == controller.sheet.get();
    let count = Memo::new(move |_| {
        mirror.with(|mirror| mirror.cells().filter(|(at, _)| on_sheet(at)).count())
    });
    let pages = move || count.get().div_ceil(PAGE).max(1);

    let blocks = move || {
        let first = page.get() * PAGE;
        let cells: Vec<(calc_host::value::CellKey, wire::Cell)> = mirror.with(|mirror| {
            mirror
                .cells()
                .filter(|(at, _)| on_sheet(at))
                .skip(first)
                .take(PAGE)
                .map(|(at, cell)| (*at, cell.clone()))
                .collect()
        });
        cells
            .into_iter()
            .filter_map(|(at, cell)| Some((at, cell.shown()?, cell)))
            .map(|(at, shown, cell)| {
                let address = cells::address(at.row(), at.column());
                let label = address.clone();
                let editable = !cell.input.is_empty();
                let kind = block_kind(&shown, editable);
                let why = cell.why.clone();
                let original = cell.input.clone();
                let displayed_original = original.clone();
                let text = Signal::derive(move || controller.notebook_drafts.with(|drafts| drafts.get(&at).cloned()).unwrap_or_else(|| displayed_original.clone()));
                let outcome = RwSignal::new(String::new());
                let value = if shown.provenance == Provenance::Refused {
                    "refused".to_owned()
                } else {
                    shown.text.clone()
                };
                view! {
                    <article class="dc-block" data-dnacalc="block" data-cell=address data-provenance=shown.provenance.name()>
                        <header>
                            <strong>{label}</strong>
                            <span>{kind}</span>
                            <span class="dc-chip" data-provenance=shown.provenance.name()>{shown.provenance.name()}</span>
                        </header>
                        {editable.then(|| view! {
                            <input
                                type="text"
                                data-dnacalc="block-input"
                                aria-label=format!("Input for {}", cells::address(at.row(), at.column()))
                                spellcheck="false"
                                prop:value=move || text.get()
                                on:input=move |ev| {
                                    let draft = event_target_value(&ev);
                                    controller.notebook_drafts.update(|drafts| {
                                        if draft == original { drafts.remove(&at); } else { drafts.insert(at, draft); }
                                    });
                                }
                                on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                                    if ev.is_composing() || ev.key_code() == 229 { return; }
                                    if ev.key() == "Escape" {
                                        ev.prevent_default();
                                        controller.notebook_drafts.update(|drafts| { drafts.remove(&at); });
                                    }
                                    if ev.key() == "Enter" {
                                        ev.prevent_default();
                                        controller.enter(at, text.get_untracked(), move |done| {
                                            if done.accepted { controller.notebook_drafts.update(|drafts| { drafts.remove(&at); }); }
                                            outcome.try_set(done.error.clone().unwrap_or_default());
                                        });
                                    }
                                }
                            />
                        })}
                        <div class="dc-value" data-dnacalc="block-value">{value}</div>
                        <p class="dc-why" data-dnacalc="block-provenance">{why}</p>
                        <p class="dc-why" data-dnacalc="block-outcome">{move || outcome.get()}</p>
                    </article>
                }
            })
            .collect_view()
    };

    view! {
        <section class="dc-notebook" data-skin="notebook">
            <div class="dc-nb-pager">
                <button type="button" data-dnacalc="notebook-previous"
                    on:click=move |_| page.update(|p| *p = p.saturating_sub(1))>"Previous"</button>
                <span data-dnacalc="notebook-page">
                    {move || format!("{} cells; page {} of {}", count.get(), page.get() + 1, pages())}
                </span>
                <button type="button" data-dnacalc="notebook-next"
                    on:click=move |_| page.update(|p| if *p + 1 < pages() { *p += 1 })>"Next"</button>
            </div>
            {move || (count.get() == 0).then(|| view! {
                <p data-dnacalc="notebook-empty">"This sheet holds no cells yet."</p>
            })}
            {blocks}
        </section>
    }
}
