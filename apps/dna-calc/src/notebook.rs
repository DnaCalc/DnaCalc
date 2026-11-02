//! A bounded notebook of worksheet sources and saved links. Input uses the shared edit session;
//! block order is document layout metadata, independent of worksheet dependency order.
//!
//! Extends DnaTreeCalc bf6dfd7 `src/dnacalc-stage-notebook/src/lib.rs` and `src/model.rs`.

use crate::cells::{self, Provenance};
use crate::controller::Controller;
use crate::skin::{Skin, SkinContext, SkinId};
use crate::wire;
use leptos::prelude::*;

/// The blocks one page shows.
pub use dna_calc_model::projection::NOTEBOOK_PAGE as PAGE;

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

/// Pages are session state; cell edits and layout have workbook history.
#[derive(Clone, Copy)]
pub struct ViewState {
    pub insert_text: crate::model_adapter::ModelField<String>,
    address: crate::model_adapter::ModelField<String>,
    prose: crate::model_adapter::ModelField<bool>,
    pages: crate::model_adapter::ModelField<std::collections::BTreeMap<calc_value::SheetId, usize>>,
    selected: crate::model_adapter::ModelField<Option<String>>,
    last_active: crate::model_adapter::ModelField<Option<calc_value::CellKey>>,
}
impl ViewState {
    pub fn new(controller: Controller) -> Self {
        Self {
            insert_text: controller.view_field::<dna_calc_model::NotebookState, _>(
                SkinId::Notebook,
                |s| &s.insert_text,
                |s| &mut s.insert_text,
            ),
            address: controller.view_field::<dna_calc_model::NotebookState, _>(
                SkinId::Notebook,
                |s| &s.address,
                |s| &mut s.address,
            ),
            prose: controller.view_field::<dna_calc_model::NotebookState, _>(
                SkinId::Notebook,
                |s| &s.prose,
                |s| &mut s.prose,
            ),
            pages: controller.view_field::<dna_calc_model::NotebookState, _>(
                SkinId::Notebook,
                |s| &s.pages,
                |s| &mut s.pages,
            ),
            selected: controller.view_field::<dna_calc_model::NotebookState, _>(
                SkinId::Notebook,
                |s| &s.selected,
                |s| &mut s.selected,
            ),
            last_active: controller.view_field::<dna_calc_model::NotebookState, _>(
                SkinId::Notebook,
                |s| &s.last_active,
                |s| &mut s.last_active,
            ),
        }
    }
}
fn notebook_view(controller: Controller) -> impl IntoView {
    let state = ViewState::new(controller);
    let notebook = RwSignal::new(wire::Notebook::default());
    let reveal = RwSignal::new(None);
    let reveal_placement = StoredValue::new(None::<(String, calc_value::CellKey)>);
    let epoch = StoredValue::new(0u64);
    let pending = RwSignal::new(true);
    let address = state.address;
    let insert_text = state.insert_text;
    let prose = state.prose;
    let open_details = RwSignal::new(None::<String>);
    let detail = RwSignal::new(None::<wire::BlockDetail>);
    #[cfg(target_arch = "wasm32")]
    let desired_focus = controller.view_field::<dna_calc_model::NotebookState, _>(
        SkinId::Notebook,
        |s| &s.pending_focus,
        |s| &mut s.pending_focus,
    );
    let root: NodeRef<leptos::html::Section> = NodeRef::new();
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        let observer = StoredValue::new_local(
            None::<(
                web_sys::ResizeObserver,
                wasm_bindgen::closure::Closure<dyn FnMut()>,
            )>,
        );
        Effect::new(move |_| {
            if let Some(element) = root.get()
                && observer.with_value(Option::is_none)
            {
                let callback = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || {
                    if let Some(id) = state.selected.try_get_untracked().flatten() {
                        reveal_block(&format!("[data-dnacalc=block][data-block-key=\"{id}\"]"));
                    }
                });
                if let Ok(watch) = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()) {
                    watch.observe(&element);
                    observer.set_value(Some((watch, callback)));
                }
            }
        });
        on_cleanup(move || {
            observer.with_value(|o| {
                if let Some((watch, _)) = o {
                    watch.disconnect();
                }
            })
        });
    }
    let page = move || {
        controller
            .sheet
            .get()
            .and_then(|s| state.pages.with(|p| p.get(&s).copied()))
            .unwrap_or(0)
    };
    Effect::new(move |_| {
        let active = controller.active_key();
        if active != state.last_active.get_untracked() {
            state.last_active.set(active);
            reveal.set(active);
        }
    });
    Effect::new(move |_| {
        controller.revision.track();
        let Some(sheet) = controller.sheet.get() else {
            return;
        };
        let page = page();
        let reveal = reveal.get();
        epoch.update_value(|e| *e += 1);
        let sent = epoch.get_value();
        let generation = controller.generation.get_untracked();
        pending.set(true);
        let focus_ticket = controller.focus_snapshot();
        controller.ask(
            wire::Request::Notebook {
                sheet: wire::Sheet::of(sheet),
                page: page as u64,
                reveal: reveal
                    .and_then(|_| controller.active_key_untracked())
                    .map(wire::Key::of),
                placement: reveal
                    .and_then(|_| reveal_placement.get_value())
                    .filter(|(id, at)| {
                        Some(*at) == controller.active_key_untracked()
                            && state.selected.get_untracked().as_ref() == Some(id)
                    })
                    .map(|(id, _)| id),
            },
            move |reply| {
                if epoch.try_get_value() != Some(sent)
                    || controller.generation.get_untracked() != generation
                {
                    return;
                }
                if let wire::Reply::Notebook(found) = reply {
                    #[cfg(target_arch = "wasm32")]
                    let restore_focus = desired_focus
                        .try_update_value(|s| s.take())
                        .flatten()
                        .map(|(id, edit, ticket)| {
                            (
                                format!(
                                    "[data-dnacalc=block][data-block-key=\"{id}\"] {}",
                                    if edit {
                                        "textarea"
                                    } else {
                                        "[data-dnacalc=block-select]"
                                    }
                                ),
                                ticket,
                            )
                        })
                        .or_else(|| {
                            focused_block_selector().map(|selector| (selector, focus_ticket))
                        });
                    let active = controller.active_key_untracked();
                    let id = |b: &wire::NotebookBlock| {
                        b.placement
                            .clone()
                            .unwrap_or_else(|| format!("{:?}", b.anchor))
                    };
                    if !found.blocks.iter().any(|b| {
                        Some(id(b)) == state.selected.get_untracked()
                            && b.anchor
                                .and_then(|a| a.area())
                                .is_some_and(|a| Some(a.first()) == active)
                    }) {
                        state.selected.set(
                            found
                                .blocks
                                .iter()
                                .find(|b| {
                                    b.anchor
                                        .and_then(|a| a.area())
                                        .is_some_and(|a| Some(a.first()) == active)
                                })
                                .map(id),
                        );
                    }
                    if page != found.page as usize {
                        state.pages.update(|p| {
                            p.insert(sheet, found.page as usize);
                        });
                    }
                    notebook.try_set(found);
                    pending.try_set(false);
                    #[cfg(target_arch = "wasm32")]
                    {
                        if let Some((selector, ticket)) = restore_focus {
                            controller.focus_projection(SkinId::Notebook, selector, ticket);
                        } else if let Some(id) = state.selected.get_untracked() {
                            let selector = format!("[data-dnacalc=block][data-block-key=\"{id}\"]");
                            request_animation_frame(move || reveal_block(&selector));
                        }
                    }
                }
            },
        );
    });
    Effect::new(move |_| {
        let revision = controller.revision.get();
        let identity = open_details.get();
        detail.set(None);
        if pending.get() {
            return;
        }
        let Some(identity) = identity else {
            return;
        };
        let anchor = notebook.with(|n| {
            n.blocks
                .iter()
                .find(|b| {
                    b.placement
                        .clone()
                        .unwrap_or_else(|| format!("{:?}", b.anchor))
                        == identity
                })
                .and_then(|b| b.anchor)
        });
        let Some(anchor) = anchor else {
            return;
        };
        let generation = controller.generation.get_untracked();
        controller.ask(wire::Request::BlockDetail(anchor), move |reply| {
            if controller.generation.get_untracked() == generation
                && controller.revision.get_untracked() == revision
                && open_details.try_get_untracked().flatten().as_ref() == Some(&identity)
                && let wire::Reply::BlockDetail(found) = reply
            {
                detail.try_set(Some(found));
            }
        });
    });
    let select = move |at: calc_value::CellKey, edit: bool, identity: String| {
        let ticket = controller.focus_snapshot();
        controller.navigate_to(at, move |ok| {
            if ok {
                state.selected.set(Some(identity.clone()));
                reveal_placement.set_value(Some((identity.clone(), at)));
                reveal.set(Some(at));
                if edit {
                    controller.begin_edit(crate::controller::EditOrigin::Notebook, None);
                }
                #[cfg(target_arch = "wasm32")]
                {
                    let selector = format!(
                        "[data-dnacalc=block][data-block-key=\"{}\"] {}",
                        identity,
                        if edit {
                            "textarea"
                        } else {
                            "[data-dnacalc=block-select]"
                        }
                    );
                    desired_focus.set_value(Some((identity.clone(), edit, ticket)));
                    controller.focus_projection(SkinId::Notebook, selector, ticket);
                }
            }
        });
    };
    let show_source = move |at, skin| controller.reveal(at, skin);
    let linked = move |done: &wire::Outcome, at: calc_value::CellKey, submitted: Option<String>| {
        let same_draft = submitted
            .as_ref()
            .is_none_or(|text| insert_text.get_untracked() == *text);
        if done.accepted
            && same_draft
            && let Some(id) = &done.notebook_placement
        {
            state.selected.set(Some(id.clone()));
            reveal_placement.set_value(Some((id.clone(), at)));
            #[cfg(target_arch = "wasm32")]
            desired_focus.set_value(Some((id.clone(), false, controller.focus_snapshot())));
            reveal.set(Some(at));
        }
        if submitted.is_some() && same_draft && done.accepted {
            insert_text.try_set(String::new());
        }
    };
    let link = move |insert: bool| {
        let reference = address.get_untracked();
        let text = insert_text.get_untracked();
        let prose = prose.get_untracked();
        let controller = controller;
        controller.go_to_text(
            if reference.trim().is_empty() {
                controller
                    .active_key_untracked()
                    .map(|at| cells::address(at.row(), at.column()))
                    .unwrap_or("A1".into())
            } else {
                reference
            },
            move |ok| {
                if !ok {
                    return;
                }
                let Some(area) = controller.selected_area_untracked() else {
                    return;
                };
                let submitted = insert.then_some(text);
                controller.dispatch(
                    wire::Intent::NotebookLink {
                        anchor: wire::Area::of(area),
                        prose,
                        text: submitted.clone(),
                    },
                    move |done| linked(done, area.first(), submitted),
                );
            },
        );
    };
    let blocks = move || {
        notebook.get().blocks.into_iter().map(|block| {
        let identity=block.placement.clone().unwrap_or_else(||format!("{:?}",block.anchor));
        let Some(anchor)=block.anchor.and_then(|a|a.area()) else{return view!{<article class="dc-block"><p>"This linked block has lost its worksheet anchor. Its source has not been recreated."</p><button on:click=move |_|if let Some(id)=block.placement.clone(){controller.dispatch(wire::Intent::NotebookOrder {placement:id,direction:0},|_|{});}>"Remove link"</button></article>}.into_any();};
        let at=anchor.first();let label=controller.cell_label(at);let address=cells::address(at.row(),at.column());
        let Some(cell)=block.cell else{return ().into_any();};
        let selected=Signal::derive({let identity=identity.clone();move ||controller.active_key()==Some(at)&&state.selected.get().as_ref()==Some(&identity)});
        let choose=Callback::new({let identity=identity.clone();move |edit:bool|select(at,edit,identity.clone())});
        let is_details=Signal::derive({let identity=identity.clone();move ||open_details.get().as_ref()==Some(&identity)});
        let assist=controller.assist.another_field();
        controller.register_editor_surface(dna_calc_model::SurfaceDescriptor::editor(crate::controller::EditOrigin::Notebook), assist);
        let on_key=Callback::new(move |ev:leptos::ev::KeyboardEvent| {
            if ev.is_composing()||ev.key_code()==229{return;}
            match ev.key().as_str() {
                "Enter" if !ev.shift_key()&&!ev.ctrl_key()&&!ev.meta_key()&&!ev.alt_key()=>{ev.prevent_default();ev.stop_propagation();controller.commit_edit(move |ok|if ok {choose.run(false);});},
                "Escape"=>{ev.prevent_default();ev.stop_propagation();controller.cancel_edit();choose.run(false);},
                _=>controller.editor_key(ev,assist)
            }
        });
        let source=cell.input.clone();let value=cell.text.clone();let why=cell.why.clone();let meaning=cell.error_meaning.clone();
        let prose_block=block.prose;
        let editable=!source.is_empty()||matches!(cell.provenance,Provenance::Empty|Provenance::Literal);
        let detail_why=why.clone();
        let literal=cell.provenance==Provenance::Literal;
        let kind=if block.prose {"prose"}else if cell.formula {"formula"}else if cell.provenance==Provenance::Empty {"empty"}else if source.is_empty(){"child value"}else{"value"};
        let placement=block.placement.clone(); let order=Callback::new(move |direction:i8|{if let Some(placement)=placement.clone(){
            let apply=move ||controller.dispatch(wire::Intent::NotebookOrder {placement,direction},|_|{});
            if controller.editing.get_untracked().is_some(){controller.commit_edit(move |ok|if ok {apply();});}else{apply();}
        }});
        let block_key=identity.clone();
        let detail_id=identity.clone();
        let details=move |_|{
            if is_details.get(){open_details.set(None);return;}
            open_details.set(Some(detail_id.clone()));detail.set(None);

        };
        let link_existing=move |_|controller.navigate_to(at,move |ok|if ok {controller.dispatch(wire::Intent::NotebookLink {anchor:wire::Area::of(anchor),prose:false,text:None},move |done|linked(done,at,None));});
        let range=(anchor.first()!=anchor.last()).then(||format!(" to {}",cells::address(anchor.last().row(),anchor.last().column())));
        view! {<article class="dc-block" data-dnacalc="block" data-cell=address data-block-key=block_key data-block-kind=kind data-provenance=cell.provenance.name() data-selected=move ||selected.get().to_string() data-placement=block.placement.clone()>
            <header><button data-dnacalc="block-select" aria-current=move ||selected.get().to_string() on:click=move |_|choose.run(false)>{label}{range}</button><span>{kind}</span>{(!literal).then(||view!{<span class="dc-chip" data-provenance=cell.provenance.name()>{cell.provenance.name()}</span>})}</header>
            {move ||(selected.get()&&editable).then(||view!{<div class="dc-block-editor" on:focusin=move |_|controller.activate_editor(crate::controller::EditOrigin::Notebook,assist) on:beforeinput=move |_|controller.activate_editor(crate::controller::EditOrigin::Notebook,assist) on:input=move |_|controller.edit_changed()>
                {assist.field_labelled(Signal::derive(Vec::new),Some(on_key),if prose_block{3}else{1},"",Some("block-input"),format!("Input for {}",cells::address(at.row(),at.column())))}
                {move ||(controller.editing.get()==Some(crate::controller::EditOrigin::Notebook)).then(||view!{{assist.popup()}{assist.signature()}})}
            </div>})}
            {move ||(!literal&&!selected.get()).then(||view!{<code class="dc-block-source" data-dnacalc="block-source">{source.clone()}</code>})}
            <div class="dc-value" data-dnacalc="block-value">{value}</div>
            {(cell.provenance==Provenance::Refused||cell.kind=="error").then(||view!{<p class="dc-why">{meaning.unwrap_or_else(||why.clone())}</p>})}
            <details class="dc-block-menu" data-dnacalc="block-actions" prop:open=move ||selected.get()><summary>"Actions"</summary>
            <nav class="dc-block-actions" aria-label="Block actions"><button data-dnacalc="block-grid" on:click=move |_|show_source(at, SkinId::Grid)>"Grid"</button><button data-dnacalc="block-canvas" on:click=move |_|show_source(at, SkinId::Canvas)>"Canvas"</button><button data-dnacalc="block-details" aria-expanded=move ||is_details.get().to_string() on:click=details>"Details"</button>
                {if block.placement.is_some(){view!{<button data-dnacalc="block-up" on:click=move |_|order.run(-1)>"Move up"</button><button data-dnacalc="block-down" on:click=move |_|order.run(1)>"Move down"</button><button data-dnacalc="block-unlink" on:click=move |_|order.run(0)>"Unlink"</button>}.into_any()}else{view!{<button data-dnacalc="block-link" on:click=link_existing>"Link into notebook"</button>}.into_any()}}
            </nav></details>
            {move ||is_details.get().then(||view!{<div class="dc-block-detail" data-dnacalc="block-detail"><p>{detail_why.clone()}</p>{move ||detail.get().map(|d|view!{<p>{d.summary}</p>{d.anchor.and_then(|key|key.cell()).filter(|owner|*owner!=at).map(|owner|view!{<button data-dnacalc="block-owner" on:click=move |_|show_source(owner, SkinId::Grid)>"Open array anchor"</button>})}<div class="dc-array-preview">{d.cells.into_iter().map(|c|view!{<span>{format!("{}: {}",cells::address(c.at.2,c.at.3),c.text)}</span>}).collect_view()}</div>{d.blockers.into_iter().map(|key|view!{<button data-dnacalc="block-spill-blocker" on:click=move |_|if let Some(area)=key.area(){show_source(area.first(), SkinId::Grid);}>{format!("Open blocker {}",key.area().map(|a|format!("{}:{}",cells::address(a.first().row(),a.first().column()),cells::address(a.last().row(),a.last().column()))).unwrap_or_default())}</button>}).collect_view()}})}</div>})}
        </article>}.into_any()
    }).collect_view()
    };
    view! {<section class="dc-notebook" data-skin="notebook" node_ref=root>
        <div class="dc-nb-tools"><label>"Cell or range "<input data-dnacalc="notebook-address" placeholder="A1 or A1:B3" prop:value=move ||address.get() on:input=move |ev|address.set(event_target_value(&ev))/></label><label>"Kind "<select data-dnacalc="notebook-kind" on:change=move |ev|prose.set(event_target_value(&ev)=="prose")><option value="calculation" selected=move ||!prose.get()>"Calculation"</option><option value="prose" selected=move ||prose.get()>"Prose"</option></select></label><button data-dnacalc="notebook-link" on:click=move |_|link(false)>"Link existing cells"</button><details data-dnacalc="notebook-insert-tools"><summary>"Insert into an empty cell"</summary><label>"Content "<textarea data-dnacalc="notebook-content" prop:value=move ||insert_text.get() on:input=move |ev|insert_text.set(event_target_value(&ev))></textarea></label><button data-dnacalc="notebook-insert" on:click=move |_|link(true)>"Insert and link"</button></details></div>
        <details class="dc-nb-guide"><summary>"About linked blocks"</summary><p class="dc-nb-caption">"Linked blocks keep their own order. Formulas calculate from worksheet dependencies."</p></details>
        <div class="dc-nb-pager"><button data-dnacalc="notebook-previous" disabled=move ||notebook.get().page==0||pending.get() on:click=move |_|{controller.carry_edit_to_bar();reveal.set(None);if let Some(s)=controller.sheet.get_untracked(){state.pages.update(|p|{p.insert(s,notebook.get_untracked().page.saturating_sub(1) as usize);});}}>"Previous"</button><span data-dnacalc="notebook-page">{move ||{let n=notebook.get();format!("{} blocks; page {} of {}",n.total,n.page+1,n.total.div_ceil(PAGE as u64).max(1))}}</span><button data-dnacalc="notebook-next" disabled=move ||{let n=notebook.get();pending.get()||(n.page+1)*PAGE as u64>=n.total} on:click=move |_|{controller.carry_edit_to_bar();reveal.set(None);if let Some(s)=controller.sheet.get_untracked(){state.pages.update(|p|{p.insert(s,notebook.get_untracked().page as usize+1);});}}>"Next"</button></div>
        <div class="dc-nb-blocks">{blocks}{move ||(notebook.get().total==0&&!pending.get()).then(||view!{<p data-dnacalc="notebook-empty">"Start with an ordinary worksheet cell: insert text or a formula above, or link a cell by address."</p>})}</div>
        {move ||(notebook.get().omitted>0).then(||view!{<p data-dnacalc="notebook-omitted">{format!("{} occupied child cells outside this window; open their source in Grid.",notebook.get().omitted)}</p>})}
    </section>}
}

#[cfg(target_arch = "wasm32")]
fn reveal_block(selector: &str) {
    use wasm_bindgen::JsCast;
    let Some(block) = document().query_selector(selector).ok().flatten() else {
        return;
    };
    let Some(parent) = block
        .closest(".dc-notebook")
        .ok()
        .flatten()
        .and_then(|p| p.dyn_into::<web_sys::HtmlElement>().ok())
    else {
        return;
    };
    let row = block.get_bounding_client_rect();
    let view = parent.get_bounding_client_rect();
    let dy = if row.top() < view.top() || row.height() > view.height() {
        row.top() - view.top()
    } else if row.bottom() > view.bottom() {
        row.bottom() - view.bottom()
    } else {
        0.
    };
    if dy != 0. {
        parent.set_scroll_top((f64::from(parent.scroll_top()) + dy).round() as i32);
    }
}

#[cfg(target_arch = "wasm32")]
fn focused_block_selector() -> Option<String> {
    let focused = document().active_element()?;
    let role = focused.get_attribute("data-dnacalc")?;
    if !matches!(role.as_str(), "block-select" | "block-input") {
        return None;
    }
    let id = focused
        .closest("[data-block-key]")
        .ok()??
        .get_attribute("data-block-key")?;
    Some(format!(
        "[data-dnacalc=block][data-block-key=\"{id}\"] [data-dnacalc={role}]"
    ))
}
