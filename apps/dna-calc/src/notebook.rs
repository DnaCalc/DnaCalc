//! The notebook skin: the active sheet's cells as one column of blocks, in row order, each with
//! its address, what it holds (a formula, a value, or a value an array formula spilled), the text
//! that was entered, and its value with that value's provenance. A block's text is edited in place
//! and entered with Enter, one undoable edit through calc-host, as the grid enters it; a refused
//! entry keeps the text and shows why.
//!
//! The skin subscribes to the whole sheet ([`calc_host::Interest::Sheet`]) while it is shown and
//! pages through the cells its mirror holds, [`PAGE`] blocks at a time.
//!
//! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnacalc-stage-notebook/src/lib.rs` and
//! `src/model.rs` (`derive_entries`: one block per authored cell in row order, the entry kinds,
//! the per-block editor and its outcome chip). Left behind: the personas, the reference X-ray,
//! scenario chips and the defined-name form, none of which calc-host offers. The notebook view of
//! calc-doc's skin layout (blocks placed in a notebook order) is not read by calc-host yet, so the
//! notebook lists the sheet's cells.

use crate::cells::{self, Provenance};
use crate::controller::Controller;
use crate::skin::{Skin, SkinContext, SkinId};
use calc_host::{DeltaMirror, Interest, SubscriptionId};
use leptos::prelude::*;

/// The blocks one page shows.
pub const PAGE: usize = 40;

pub const NOTEBOOK_CSS: &str = r#"
.dc-notebook{overflow:auto;flex:1 1 auto;padding:12px 16px;background:#f6f8fa;font:13px system-ui,"Segoe UI",sans-serif}
.dc-nb-pager{display:flex;gap:8px;align-items:center;margin:0 0 10px}
.dc-block{background:#fff;border:1px solid #d0d7de;border-radius:6px;padding:8px 10px;margin:0 0 8px;max-width:900px}
.dc-block header{display:flex;gap:10px;align-items:baseline;color:#57606a;font-size:12px}
.dc-block header strong{color:#1b1f24;font:600 13px ui-monospace,Consolas,monospace}
.dc-block input{width:100%;box-sizing:border-box;font:13px ui-monospace,Consolas,monospace;border:1px solid #d0d7de;border-radius:4px;padding:3px 6px;margin:6px 0}
.dc-block .dc-value{font:600 14px ui-monospace,Consolas,monospace}
.dc-chip{display:inline-block;font-size:11px;border-radius:10px;padding:1px 8px;background:#eaeef2;color:#424a53}
.dc-chip[data-provenance=calculated]{background:#dafbe1;color:#116329}
.dc-chip[data-provenance=refused]{background:#ffebe9;color:#a40e26}
.dc-chip[data-provenance=file-cache]{background:#eaeef2;color:#57606a}
.dc-chip[data-provenance=stale]{background:#fff8c5;color:#9a6700}
.dc-why{color:#57606a;font-size:12px;margin:4px 0 0}
"#;

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
    let mirror = RwSignal::new(DeltaMirror::default());
    let subscription: StoredValue<Option<(u64, SubscriptionId)>> = StoredValue::new(None);
    let page = RwSignal::new(0usize);
    Effect::new(move |_| {
        let generation = controller.generation.get();
        let Some(sheet) = controller.sheet.get() else {
            return;
        };
        page.set(0);
        match subscription.get_value() {
            Some((held, id)) if held == generation => {
                controller.set_interest(id, Interest::Sheet(sheet));
            }
            _ => {
                let id = controller.subscribe(Interest::Sheet(sheet), mirror);
                subscription.set_value(id.map(|id| (generation, id)));
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
    let on_sheet =
        move |cell: &calc_host::ProjectedCell| Some(cell.at().sheet()) == controller.sheet.get();
    let count = Memo::new(move |_| {
        mirror.with(|mirror| mirror.cells().filter(|cell| on_sheet(cell)).count())
    });
    let pages = move || count.get().div_ceil(PAGE).max(1);

    let blocks = move || {
        let profile = controller.with(|book| book.profile().cloned());
        let first = page.get() * PAGE;
        let cells: Vec<_> = mirror.with(|mirror| {
            mirror
                .cells()
                .filter(|cell| on_sheet(cell))
                .skip(first)
                .take(PAGE)
                .cloned()
                .collect()
        });
        cells
            .into_iter()
            .map(|cell| {
                let format = controller
                    .with_untracked(|book| book.host().number_format(cell.at()).map(str::to_owned));
                let shown = cells::shown(&cell, profile.as_ref(), format.as_deref());
                let at = cell.at();
                let address = cells::address(at.row(), at.column());
                let label = address.clone();
                let kind = block_kind(&shown, cell.input().is_some());
                let why = controller.with_untracked(|book| cells::provenance_text(book.host(), &cell));
                let text = RwSignal::new(cells::input_text(cell.input()));
                let outcome = RwSignal::new(String::new());
                let editable = cell.input().is_some();
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
                                spellcheck="false"
                                prop:value=move || text.get()
                                on:input=move |ev| text.set(event_target_value(&ev))
                                on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                                    if ev.key() == "Enter" {
                                        ev.prevent_default();
                                        let receipt = controller.enter(at, text.get_untracked());
                                        outcome.set(match receipt.error {
                                            Some(error) => error.to_string(),
                                            None => String::new(),
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
            <style>{NOTEBOOK_CSS}</style>
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
