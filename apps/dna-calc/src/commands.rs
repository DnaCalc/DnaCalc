//! Local workbook commands; searches and arithmetic are answered by the calculation worker.
use crate::{
    controller::Controller,
    wire::{self, Intent, Reply, Request},
};
use dna_calc_model::{Command, FindMode};
use leptos::prelude::*;
use wasm_bindgen::JsCast;

fn find_options(controller: Controller) -> impl IntoView {
    let scope = controller.tool_field(|s| &s.scope, |s| &mut s.scope);
    let formulas = controller.tool_field(|s| &s.formulas, |s| &mut s.formulas);
    let case = controller.tool_field(|s| &s.case, |s| &mut s.case);
    let whole = controller.tool_field(|s| &s.whole, |s| &mut s.whole);
    let wildcards = controller.tool_field(|s| &s.wildcards, |s| &mut s.wildcards);
    view! {
                <div class="dc-find-options"><label>"Within"<select data-dnacalc="find-scope" prop:value=move ||scope.get() on:change=move |ev|{scope.set(event_target_value(&ev));controller.invalidate_search();}><option value="sheet">"Worksheet"</option><option value="selection">"Original selection"</option><option value="workbook">"Workbook, including hidden sheets"</option></select></label><label>"Look in"<select data-dnacalc="find-look" prop:value=move ||if formulas.get(){"formulas"}else{"values"} on:change=move |ev|{formulas.set(event_target_value(&ev)=="formulas");controller.invalidate_search();}><option value="formulas">"Formulas & entries"</option><option value="values">"Displayed values"</option></select></label></div>
                <div class="dc-find-toggles"><label><input type="checkbox" data-dnacalc="find-case" prop:checked=move ||case.get() on:change=move |ev|{case.set(event_target_checked(&ev));controller.invalidate_search();}/ >"Match case"</label><label><input type="checkbox" data-dnacalc="find-whole" prop:checked=move ||whole.get() on:change=move |ev|{whole.set(event_target_checked(&ev));controller.invalidate_search();}/ >"Whole cell"</label><label><input type="checkbox" data-dnacalc="find-wildcards" prop:checked=move ||wildcards.get() on:change=move |ev|{wildcards.set(event_target_checked(&ev));controller.invalidate_search();}/ >"Wildcards (* ? ~)"</label></div>
    }
}

// Bound the parent view size: retained controller closures otherwise accumulate on the wasm stack.
pub fn controls(controller: Controller) -> AnyView {
    let dialog = NodeRef::<leptos::html::Dialog>::new();
    let query_field = NodeRef::<leptos::html::Input>::new();
    let goto_field = NodeRef::<leptos::html::Input>::new();
    let query = controller.tool_field(|s| &s.query, |s| &mut s.query);
    let replacement = controller.tool_field(|s| &s.replacement, |s| &mut s.replacement);
    let target = controller.tool_field(|s| &s.target, |s| &mut s.target);
    let case = controller.tool_field(|s| &s.case, |s| &mut s.case);
    let whole = controller.tool_field(|s| &s.whole, |s| &mut s.whole);
    let wildcards = controller.tool_field(|s| &s.wildcards, |s| &mut s.wildcards);
    let formulas = controller.tool_field(|s| &s.formulas, |s| &mut s.formulas);
    let scope = controller.tool_field(|s| &s.scope, |s| &mut s.scope);
    let found = controller.tool_field(|s| &s.found, |s| &mut s.found);
    let selected = controller.tool_field(|s| &s.selected, |s| &mut s.selected);
    let found_revision = controller.tool_field(|s| &s.found_revision, |s| &mut s.found_revision);
    let message = controller.tool_field(|s| &s.message, |s| &mut s.message);
    let epoch = controller.tool_field(|s| &s.epoch, |s| &mut s.epoch);
    let anchor = controller.tool_field(|s| &s.anchor, |s| &mut s.anchor);
    let areas = controller.tool_field(|s| &s.areas, |s| &mut s.areas);
    let stale = move || found.get().is_some() && found_revision.get() != controller.revision.get();
    let invalidate = move || controller.invalidate_search();
    Effect::new(move |_| {
        controller.generation.track();
        controller.find_open.set(None);
        found.set(None);
        anchor.set_value(None);
    });
    Effect::new(move |_| {
        let mode = controller.find_open.get();
        if mode.is_some() {
            if anchor.get_value().is_none() {
                anchor.set_value(controller.active_key_untracked().map(wire::Key::of));
                areas.set_value(
                    controller
                        .areas
                        .get_untracked()
                        .into_iter()
                        .map(wire::Area::of)
                        .collect(),
                );
            }
            invalidate();
        } else {
            anchor.set_value(None);
            epoch.update_value(|n| *n += 1);
        }
        if let Some(dialog) = dialog.get() {
            if let Some(mode) = mode
                && !dialog.open()
            {
                let _ = dialog.show_modal();
                if mode == FindMode::GoTo {
                    if let Some(field) = goto_field.get() {
                        let _ = field.focus();
                        field.select();
                    }
                } else if let Some(field) = query_field.get() {
                    let _ = field.focus();
                    field.select();
                }
            } else if mode.is_none() && dialog.open() {
                dialog.close();
            }
        }
    });
    Effect::new(move |_| match controller.find_open.get() {
        Some(FindMode::GoTo) => {
            if let Some(field) = goto_field.get() {
                let _ = field.focus();
                field.select();
            }
        }
        Some(_) => {
            if let Some(field) = query_field.get() {
                let _ = field.focus();
                field.select();
            }
        }
        None => {}
    });
    let close = move || {
        if let Some(dialog) = dialog.get_untracked() {
            dialog.close();
        }
        controller.find_open.set(None);
        controller.focus_active_skin();
    };
    let options = move || {
        anchor.get_value().map(|at| wire::FindOptions {
            sheet: wire::Sheet(at.0, at.1),
            areas: areas.get_value(),
            text: query.get_untracked(),
            case_sensitive: case.get_untracked(),
            whole: whole.get_untracked(),
            wildcards: wildcards.get_untracked(),
            formulas: formulas.get_untracked(),
            scope: match scope.get_untracked().as_str() {
                "workbook" => wire::FindScope::Workbook,
                "selection" => wire::FindScope::Selection,
                _ => wire::FindScope::Sheet,
            },
        })
    };
    let run = move || {
        let Some(options) = options() else {
            return;
        };
        epoch.update_value(|n| *n += 1);
        let request = epoch.get_value();
        let generation = controller.generation.get_untracked();
        let revision = controller.revision.get_untracked();
        message.set("Searching…".into());
        found.set(None);
        controller.ask(Request::Find(options),move |reply|{
            if epoch.get_value()!=request || controller.generation.get_untracked()!=generation{return;}
            if revision!=controller.revision.get_untracked(){message.set("Workbook changed; run Find again".into());return;}
            found_revision.set(revision);
            if let Reply::Found(result)=reply {match result {Ok(result)=>{message.set(if result.rows.is_empty(){if result.partial {"No match in inspected cells; narrow the scope to complete the search".into()}else{"No matches".into()}}else{format!("{} matches shown · {} occupied cells inspected{}",result.rows.len(),result.scanned,if result.partial {" · search limit reached"}else{""})});selected.set(0);found.set(Some(result.into()));},Err(error)=>message.set(error)}}
        });
    };
    let navigate = move |index: usize| {
        if let Some(row) = found
            .get_untracked()
            .and_then(|f| f.rows.get(index).cloned())
            && !row.hidden
            && let Some(at) = row.at.cell()
        {
            selected.set(index);
            controller.navigate_to(at, |_| {});
        }
    };
    let replace = move |all: bool| {
        let (Some(options), Some(found)) = (options(), found.get_untracked()) else {
            return;
        };
        let at = if all {
            None
        } else {
            found.rows.get(selected.get_untracked()).map(|r| r.at)
        };
        if !all && at.is_none() {
            return;
        }
        let context = epoch.get_value();
        let generation = controller.generation.get_untracked();
        controller.dispatch(
            Intent::Replace {
                options,
                replacement: replacement.get_untracked(),
                revision: found.revision,
                only: at,
            },
            move |outcome| {
                if context != epoch.get_value()
                    || generation != controller.generation.get_untracked()
                    || controller.find_open.get_untracked().is_none()
                {
                    return;
                }

                if outcome.accepted {
                    run();
                } else {
                    message.set(
                        outcome
                            .error
                            .clone()
                            .unwrap_or_else(|| "No cells changed".into()),
                    );
                }
            },
        );
    };
    let results = move || {
        if stale() {
            return None;
        }
        found.get().map(|f|f.rows.clone().into_iter().enumerate().map(|(index,row)|{
        let hidden=row.hidden;let label=row.label;let source=row.source;let display=row.display;let provenance=row.provenance.name();
        view!{<li><button type="button" class="dc-find-hit" data-dnacalc="find-hit" aria-pressed=move ||(selected.get()==index).to_string() disabled=hidden title=if hidden {"Unhide this worksheet to navigate"}else{"Go to this cell"} on:click=move |_|navigate(index)><strong>{label}</strong><code>{source}</code><span>{display}" · "{provenance}{if hidden {" · hidden worksheet"}else{""}}</span></button></li>}
    }).collect_view())
    };
    let menu = NodeRef::<leptos::html::Dialog>::new();
    let permission = controller.tool_field(|s| &s.permission, |s| &mut s.permission);
    Effect::new(move |_| {
        let revision = controller.revision.get();
        let generation = controller.generation.get();
        let location = controller.context_menu.get();
        let at = controller.active_key();
        permission.set(None);
        if location.is_some()
            && let Some(at) = at
        {
            controller.ask(Request::CanEdit(wire::Key::of(at)), move |reply| {
                if revision == controller.revision.get_untracked()
                    && generation == controller.generation.get_untracked()
                    && controller.active_key_untracked() == Some(at)
                    && controller.context_menu.get_untracked().is_some()
                    && let Reply::CanEdit(found) = reply
                {
                    permission.set(Some(found));
                }
            });
        }
        if let Some(menu) = menu.get() {
            if location.is_some() && !menu.open() {
                let _ = menu.show_modal();
                if let Ok(Some(first)) = menu.query_selector("[role=menuitem]:not(:disabled)")
                    && let Some(first) = first.dyn_ref::<web_sys::HtmlElement>()
                {
                    let _ = first.focus();
                }
            } else if location.is_none() && menu.open() {
                menu.close();
            }
        }
    });
    let close_menu = move || {
        if let Some(menu) = menu.get_untracked() {
            menu.close();
        }
        controller.context_menu.set(None);
        controller.focus_active_skin();
    };
    let writable = move || permission.get().is_some_and(|p| p.is_ok());
    let unavailable = move || {
        permission
            .get()
            .map(|p| p.err().unwrap_or_default())
            .unwrap_or_else(|| "Checking worksheet permissions…".into())
    };
    let command = move |command: &str| {
        close_menu();
        match command {
            "copy" => controller.copy(),
            "cut" => controller.cut(),
            "paste" => controller.clipboard_fallback.set(Some(false)),
            "clear" => controller.clear_active(),
            "insert" => controller.edit_axis(true),
            "delete" => controller.edit_axis(false),
            "find" => controller.command(Command::Find),
            "goto" => controller.command(Command::GoTo),
            "review" => controller.review_open.set(true),
            _ => {}
        }
    };
    let menu_keys = move |ev: leptos::ev::KeyboardEvent| {
        ev.stop_propagation();
        if ev.key() == "Escape" || ev.key() == "Tab" {
            ev.prevent_default();
            close_menu();
            return;
        }
        if let Some(menu) = menu.get_untracked()
            && let Ok(items) = menu.query_selector_all("[role=menuitem]:not(:disabled)")
        {
            let count = items.length() as usize;
            let active = document().active_element();
            let current = (0..count)
                .find(|&i| {
                    items
                        .item(i as u32)
                        .and_then(|n| n.dyn_into::<web_sys::Element>().ok())
                        .as_ref()
                        == active.as_ref()
                })
                .unwrap_or(0);
            if let Some(next) = crate::grid::validation_list_index(current, &ev.key(), count) {
                ev.prevent_default();
                if let Some(item) = items
                    .item(next as u32)
                    .and_then(|n| n.dyn_into::<web_sys::HtmlElement>().ok())
                {
                    let _ = item.focus();
                }
            }
        }
    };
    view! {
        <dialog class="dc-replace dc-find-dialog" node_ref=dialog data-dnacalc="find-dialog" aria-labelledby="dc-find-title" on:cancel=move |ev:leptos::ev::Event|{ev.prevent_default();close();}>
            <div class="dc-review-heading"><h2 id="dc-find-title">{move ||match controller.find_open.get(){Some(FindMode::Replace)=>"Find & replace",Some(FindMode::GoTo)=>"Go to cell or range",_=>"Find in workbook"}}</h2><button data-dnacalc="find-close" on:click=move |_|close()>"Close"</button></div>
            {move ||(controller.find_open.get()==Some(FindMode::GoTo)).then(||view!{
                <label>"Address, range or defined name"<input node_ref=goto_field data-dnacalc="goto-target" prop:value=move ||target.get() on:input=move |ev|target.set(event_target_value(&ev)) on:keydown=move |ev:leptos::ev::KeyboardEvent|{if ev.key()=="Enter"{ev.prevent_default();controller.go_to_text(target.get_untracked(),move |ok|{if ok{close();}});}}/></label>
                <button data-dnacalc="goto-apply" on:click=move |_|controller.go_to_text(target.get_untracked(),move |ok|{if ok{close();}})>"Go"</button><p role="status">{move ||controller.status.get().text}</p>
            })}
            <div hidden=move ||controller.find_open.get()==Some(FindMode::GoTo)>
                <label>"Find"<input node_ref=query_field data-dnacalc="find-query" prop:value=move ||query.get() on:input=move |ev|{query.set(event_target_value(&ev));invalidate();} on:keydown=move |ev:leptos::ev::KeyboardEvent|{if ev.key()=="Enter"{ev.prevent_default();run();}}/></label>
                {find_options(controller)}
                <div class="dc-dialog-actions"><button data-dnacalc="find-all" on:click=move |_|run()>"Find all"</button><button data-dnacalc="find-next" disabled=move ||stale()||found.get().is_none_or(|f|f.rows.is_empty()) on:click=move |_|{if let Some(found)=found.get_untracked(){let len=found.rows.len();for step in 1..=len{let next=(selected.get_untracked()+step)%len;if !found.rows[next].hidden{navigate(next);break;}}}}>"Next match"</button><button data-dnacalc="find-replace-mode" on:click=move |_|controller.command(Command::Replace)>"Replace…"</button></div>
                {move ||(controller.find_open.get()==Some(FindMode::Replace)).then(||view!{
                    <label>"Replace with"<input data-dnacalc="replace-text" prop:value=move ||replacement.get() on:input=move |ev|replacement.set(event_target_value(&ev))/></label>
                    <p class="dc-muted">"Replacement changes literal formulas and entries. Choose Formulas & entries and turn off wildcards. Validation failures stop the whole command."</p>
                    <div class="dc-dialog-actions"><button data-dnacalc="replace-one" disabled=move ||stale()||found.get().is_none_or(|f|f.rows.is_empty()||f.partial)||!formulas.get()||wildcards.get() on:click=move |_|replace(false)>"Replace selected match"</button><button data-dnacalc="replace-all" disabled=move ||stale()||found.get().is_none_or(|f|f.rows.is_empty()||f.partial)||!formulas.get()||wildcards.get() on:click=move |_|replace(true)>"Replace all matches"</button></div>
                })}
                <p role="status" data-dnacalc="find-message">{move ||if stale(){"Workbook changed; run Find again".into()}else{message.get()}}</p><ol class="dc-find-results" data-dnacalc="find-results">{results}</ol>
            </div>
            <p class="dc-muted">"Workbook shortcuts work while the grid has focus. Move focus outside it to use browser Find."</p>
        </dialog>
        <dialog role="menu" class="dc-replace dc-context-menu" node_ref=menu data-dnacalc="context-menu" aria-label="Worksheet commands" style:left=move ||format!("clamp(8px,{}px,max(8px,calc(100vw - 260px)))",controller.context_menu.get().unwrap_or_default().0) style:top=move ||format!("clamp(8px,{}px,max(8px,calc(100dvh - 450px)))",controller.context_menu.get().unwrap_or_default().1) on:keydown=menu_keys on:cancel=move |ev:leptos::ev::Event|{ev.prevent_default();close_menu();}>
            <strong>{move ||controller.active_key().map(|at|controller.cell_label(at)).unwrap_or_default()}</strong>
            <button role="menuitem" data-dnacalc="context-copy" on:click=move |_|command("copy")>"Copy"</button><button role="menuitem" data-dnacalc="context-cut" disabled=move ||!writable() title=unavailable on:click=move |_|command("cut")>"Cut"</button><button role="menuitem" data-dnacalc="context-paste" disabled=move ||!writable() title=unavailable on:click=move |_|command("paste")>"Paste table text…"</button>
            <button role="menuitem" data-dnacalc="context-clear" disabled=move ||!writable() title=unavailable on:click=move |_|command("clear")>"Clear contents"</button><button role="menuitem" data-dnacalc="context-insert" disabled=move ||!writable() title=unavailable on:click=move |_|command("insert")>"Insert cells…"</button><button role="menuitem" data-dnacalc="context-delete" disabled=move ||!writable() title=unavailable on:click=move |_|command("delete")>"Delete cells…"</button>
            <button role="menuitem" data-dnacalc="context-find" on:click=move |_|command("find")>"Find…"</button><button role="menuitem" data-dnacalc="context-goto" on:click=move |_|command("goto")>"Go to…"</button><button role="menuitem" data-dnacalc="context-review" on:click=move |_|command("review")>"Rules & review"</button><small role="status" data-dnacalc="context-reason">{unavailable}</small><small>"Shift + right click opens browser commands."</small>
        </dialog>
    }.into_any()
}

pub fn summary(controller: Controller) -> impl IntoView {
    let data = controller.tool_field(|s| &s.selection_stats, |s| &mut s.selection_stats);
    Effect::new(move |_| {
        let areas = controller.areas.get();
        let revision = controller.revision.get();
        let generation = controller.generation.get();
        data.set(None);
        controller.ask_result(Request::SelectionStats, move |reply| {
            if areas == controller.areas.get_untracked()
                && revision == controller.revision.get_untracked()
                && generation == controller.generation.get_untracked()
            {
                match reply {
                    Ok(Reply::SelectionStats(stats)) => data.set(Some(stats)),
                    Err(error) => data.set(Some(Err(format!("Summary unavailable: {error}")))),
                    _ => {}
                }
            }
        });
    });
    let heading = move || match data.get() {
        Some(Ok(d)) => format!(
            "Selection · Current count {} · Numbers {} · Sum {}",
            d.count, d.numeric, d.sum
        ),
        Some(Err(message)) => format!("Selection · {message}"),
        None => "Selection · Waiting for workbook".into(),
    };
    let details = move || {
        data.get().and_then(Result::ok).map(|d|view!{
        <dl><dt>"Selected cells"</dt><dd>{d.selected}</dd><dt>"Count of current data cells"</dt><dd data-dnacalc="summary-count">{d.count}</dd><dt>"Numeric count"</dt><dd data-dnacalc="summary-numeric">{d.numeric}</dd><dt>"Sum"</dt><dd data-dnacalc="summary-sum">{d.sum}</dd><dt>"Average"</dt><dd data-dnacalc="summary-average">{d.average}</dd><dt>"Minimum"</dt><dd data-dnacalc="summary-min">{d.min}</dd><dt>"Maximum"</dt><dd data-dnacalc="summary-max">{d.max}</dd></dl>
        <p data-dnacalc="summary-provenance">{format!("{} literals · {} calculated · {} blanks · {} empty text values · {} errors. Excluded: {} file caches · {} stale · {} refused.",d.literal,d.calculated,d.blank,d.empty_text,d.errors,d.cached,d.stale,d.refused)}</p>
        <p class="dc-muted">"Counts and numeric summaries use current literals and calculated results. Errors and empty text count as data. Numeric summaries exclude errors, text and logical values; empty results count as blanks."</p>
    })
    };
    view! {<details class="dc-selection-summary" data-dnacalc="selection-summary"><summary data-dnacalc="selection-summary-toggle">{heading}</summary><div class="dc-selection-summary-body">{details}</div></details>}
}

#[cfg(all(test, target_arch = "wasm32"))]
mod browser_tests {
    use super::*;
    use wasm_bindgen::JsValue;

    #[wasm_bindgen_test::wasm_bindgen_test]
    async fn retained_find_options_match_controls_after_remount() {
        let root = Owner::new();
        let controller = root.with(|| {
            Controller::with_transport(std::rc::Rc::new(
                dna_calc_worker::transport::InProcessTransport::default(),
            ))
        });
        let parent = document()
            .create_element("div")
            .unwrap()
            .unchecked_into::<web_sys::HtmlElement>();
        let mount =
            root.with(|| leptos::mount::mount_to(parent.clone(), move || find_options(controller)));
        for (name, property, value) in [
            ("scope", "value", JsValue::from_str("workbook")),
            ("look", "value", JsValue::from_str("values")),
            ("case", "checked", JsValue::TRUE),
            ("whole", "checked", JsValue::TRUE),
            ("wildcards", "checked", JsValue::TRUE),
        ] {
            let field = parent
                .query_selector(&format!("[data-dnacalc=find-{name}]"))
                .unwrap()
                .unwrap();
            js_sys::Reflect::set(&field, &JsValue::from_str(property), &value).unwrap();
            field
                .dispatch_event(&web_sys::Event::new("change").unwrap())
                .unwrap();
        }
        drop(mount);
        let mount =
            root.with(|| leptos::mount::mount_to(parent.clone(), move || find_options(controller)));
        wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&JsValue::UNDEFINED))
            .await
            .unwrap();
        for (name, property, value) in [
            ("scope", "value", JsValue::from_str("workbook")),
            ("look", "value", JsValue::from_str("values")),
            ("case", "checked", JsValue::TRUE),
            ("whole", "checked", JsValue::TRUE),
            ("wildcards", "checked", JsValue::TRUE),
        ] {
            let field = parent
                .query_selector(&format!("[data-dnacalc=find-{name}]"))
                .unwrap()
                .unwrap();
            assert_eq!(
                js_sys::Reflect::get(&field, &JsValue::from_str(property)).unwrap(),
                value,
                "{name} after remount"
            );
        }
        root.with(|| {
            assert_eq!(
                controller
                    .tool_field(|s| &s.scope, |s| &mut s.scope)
                    .get_untracked(),
                "workbook"
            );
            assert!(
                !controller
                    .tool_field(|s| &s.formulas, |s| &mut s.formulas)
                    .get_untracked()
            );
        });
        drop(mount);
        root.cleanup();
    }
}

/// Range sorting shares the model's command route with every grid gesture.
pub fn sort_controls(controller: Controller) -> AnyView {
    let header=controller.tool_field(|s| &s.sort_header,|s| &mut s.sort_header);
    let column=controller.tool_field(|s| &s.sort_column,|s| &mut s.sort_column);
    let levels=controller.tool_field(|s| &s.sort_levels,|s| &mut s.sort_levels);
    view! {
        <details class="dc-clipboard-tools" data-dnacalc="sort-tools" name="worksheet-tools">
            <summary>"Sort rows"</summary>
            <p class="dc-note">"Select the complete data rectangle. Only selected cells move. Text uses case-insensitive en-ZA order. Blank keys stay last and equal keys keep their order."</p>
            <div class="dc-command-row">
                <label>"Column within selection "<input data-dnacalc="sort-column" type="number" min="1" step="1" inputmode="numeric" prop:value=move ||column.get() on:input=move |ev|column.set(event_target_value(&ev))/></label>
                <label><input data-dnacalc="sort-header" type="checkbox" prop:checked=move ||header.get() on:change=move |ev|header.set(event_target_checked(&ev))/><span>"First row is a header"</span></label>
                <button data-dnacalc="sort-ascending" disabled=move ||controller.editing.get().is_some() on:click=move |_|controller.sort_rows(false)>"A to Z / smallest to largest"</button>
                <button data-dnacalc="sort-descending" disabled=move ||controller.editing.get().is_some() on:click=move |_|controller.sort_rows(true)>"Z to A / largest to smallest"</button>
            </div>
            <For each={move ||(0..levels.get().len()).collect::<Vec<_>>()} key=|i|*i children={move |i|view! {
                <div class="dc-command-row">
                    <label>{format!("Then column {} ",i+2)}<input data-dnacalc=format!("sort-level-column-{}",i+2) type="number" min="1" step="1" inputmode="numeric"
                        prop:value=move ||levels.get().get(i).map(|v|v.0.clone()).unwrap_or_default()
                        on:input=move |ev|levels.update(|all|if let Some(v)=all.get_mut(i){v.0=event_target_value(&ev);})/></label>
                    <label>"Order "<select data-dnacalc=format!("sort-level-order-{}",i+2)
                        prop:value=move ||if levels.get().get(i).is_some_and(|v|v.1){"desc"}else{"asc"}
                        on:change=move |ev|levels.update(|all|if let Some(v)=all.get_mut(i){v.1=event_target_value(&ev)=="desc";})>
                        <option value="asc">"Ascending"</option><option value="desc">"Descending"</option>
                    </select></label>
                    <button data-dnacalc=format!("sort-level-remove-{}",i+2) aria-label=format!("Remove sort key {}",i+2)
                        on:click=move |_|levels.update(|all|{if i<all.len(){all.remove(i);}})>"Remove key"</button>
                </div>
            }}/>
            <button data-dnacalc="sort-add-key" disabled={move ||levels.get().len()>=31}
                on:click=move |_|levels.update(|all|{if all.len()<31 {all.push(((all.len()+2).to_string(),false));}})>"Add sort key"</button>
            <p class="dc-muted">"Keys are applied from top to bottom. One Undo restores the sorted range. Hidden rows, tables, merges, rich text, attached rules, notes, links and sheet-qualified formulas need additional sort support."</p>
        </details>
    }.into_any()
}

/// AutoFilter drafts and choices use the same command/history path as other worksheet edits.
pub fn filter_controls(controller:Controller)->AnyView {
    let column=controller.tool_field(|s|&s.filter.column,|s|&mut s.filter.column);
    let search=controller.tool_field(|s|&s.filter.search,|s|&mut s.filter.search);
    let values=controller.tool_field(|s|&s.filter.values,|s|&mut s.filter.values);
    let blank=controller.tool_field(|s|&s.filter.blank,|s|&mut s.filter.blank);
    let operator=controller.tool_field(|s|&s.filter.operator,|s|&mut s.filter.operator);
    let value=controller.tool_field(|s|&s.filter.value,|s|&mut s.filter.value);
    let operator2=controller.tool_field(|s|&s.filter.operator2,|s|&mut s.filter.operator2);
    let value2=controller.tool_field(|s|&s.filter.value2,|s|&mut s.filter.value2);
    let and=controller.tool_field(|s|&s.filter.and,|s|&mut s.filter.and);
    let epoch=controller.tool_field(|s|&s.filter.epoch,|s|&mut s.filter.epoch);
    let loaded=controller.tool_field(|s|&s.filter.loaded,|s|&mut s.filter.loaded);
    let message=controller.tool_field(|s|&s.filter.message,|s|&mut s.filter.message);
    let open=controller.tool_field(|s|&s.filter.open,|s|&mut s.filter.open);
    let current=move ||controller.filter_view();
    let options=||view!{<option value="equal">"Equals"</option><option value="notEqual">"Does not equal"</option><option value="greaterThan">"Greater than"</option><option value="greaterThanOrEqual">"Greater than or equal"</option><option value="lessThan">"Less than"</option><option value="lessThanOrEqual">"Less than or equal"</option>};
    view!{
        <details class="dc-clipboard-tools" data-dnacalc="filter-tools" name="worksheet-tools" open=move ||open.get()
            on:toggle=move |ev|{if let Some(e)=ev.target().and_then(|t|t.dyn_into::<web_sys::HtmlDetailsElement>().ok()){open.set(e.open());}}>
            <summary>"AutoFilter"</summary>
            <p class="dc-note" data-dnacalc="filter-summary">{move ||current().map(|f|format!("{} · {} rows filtered out{}",f.area.area().map(|a|controller.area_label(a)).unwrap_or_default(),f.filtered,if f.unsupported{" · imported criteria are preserved; editing is unavailable"}else{""})).unwrap_or_else(||"Select one rectangle including a header, then enable AutoFilter.".into())}</p>
            <div class="dc-command-row">
                <button data-dnacalc="filter-enable" disabled=move ||current().is_some() on:click=move |_|controller.enable_filter()>"Enable for selection"</button>
                <button data-dnacalc="filter-clear" disabled=move ||current().is_none() on:click=move |_|controller.filter_command(wire::FilterChange::Clear)>"Clear all filters"</button>
                <button data-dnacalc="filter-reapply" disabled=move ||current().is_none() on:click=move |_|controller.filter_command(wire::FilterChange::Reapply)>"Reapply"</button>
                <button data-dnacalc="filter-remove" disabled=move ||current().is_none() on:click=move |_|controller.filter_command(wire::FilterChange::Remove)>"Remove AutoFilter"</button>
            </div>
            <Show when=move ||current().is_some_and(|f|!f.unsupported)>
                <div class="dc-command-row">
                    <label>"Column within filter range "<input data-dnacalc="filter-column" type="number" min="1" prop:value=move ||column.get() on:input=move |ev|{column.set(event_target_value(&ev));loaded.set(false);epoch.update(|n|*n+=1);}/></label>
                    <button data-dnacalc="filter-load" on:click=move |_|controller.load_filter_values()>"Load values"</button>
                    <button data-dnacalc="filter-clear-column" on:click=move |_|controller.clear_filter_column()>"Clear this column"</button>
                </div>
                <p class="dc-muted" role="status" data-dnacalc="filter-message">{move ||message.get()}</p>
                <Show when=move ||loaded.get()>
                    <label>"Find values "<input data-dnacalc="filter-search" prop:value=move ||search.get() on:input=move |ev|search.set(event_target_value(&ev))/></label>
                    <div class="dc-command-row">
                        <button data-dnacalc="filter-select-all" on:click=move |_|{values.update(|v|for (_,checked) in v{*checked=true;});blank.set(true);}>"Select all"</button>
                        <button data-dnacalc="filter-select-none" on:click=move |_|{values.update(|v|for (_,checked) in v{*checked=false;});blank.set(false);}>"Select none"</button>
                    </div>
                    <div class="dc-filter-values" role="group" aria-label="Filter values">
                        <label><input data-dnacalc="filter-blank" type="checkbox" prop:checked=move ||blank.get() on:change=move |ev|blank.set(event_target_checked(&ev))/><span>"(Blanks)"</span></label>
                        <For each=move ||{let find=search.get().to_lowercase();values.get().into_iter().enumerate().filter(|(_, (v,_))|v.to_lowercase().contains(&find)).map(|(i,(v,_))|(i,v)).collect::<Vec<_>>()} key=|(i,_)|*i children=move |(i,v)|view!{
                            <label><input data-dnacalc="filter-choice" data-filter-value=v.clone() type="checkbox" prop:checked=move ||values.get().get(i).is_some_and(|(_,c)|*c) on:change=move |ev|values.update(|v|{if let Some((_,checked))=v.get_mut(i){*checked=event_target_checked(&ev);}})/><span>{v}</span></label>
                        }/>
                    </div>
                    <button data-dnacalc="filter-apply" on:click=move |_|controller.apply_filter(false)>"Apply selected values"</button>
                </Show>
                <details data-dnacalc="filter-custom-tools">
                    <summary>"Custom number or text filter"</summary>
                    <p class="dc-muted">"Text accepts * and ? wildcards; ~ escapes them. Dates use their stored serial number. Both conditions apply to this column."</p>
                    <div class="dc-command-row">
                        <label>"First condition "<select data-dnacalc="filter-operator" prop:value=move ||operator.get() on:change=move |ev|operator.set(event_target_value(&ev))>{options()}</select></label>
                        <label>"First value "<input data-dnacalc="filter-value" prop:value=move ||value.get() on:input=move |ev|value.set(event_target_value(&ev))/></label>
                        <label>"Combine "<select data-dnacalc="filter-and" prop:value=move ||if and.get(){"and"}else{"or"} on:change=move |ev|and.set(event_target_value(&ev)=="and")><option value="and">"And"</option><option value="or">"Or"</option></select></label>
                        <label>"Second condition "<select data-dnacalc="filter-operator-2" prop:value=move ||operator2.get() on:change=move |ev|operator2.set(event_target_value(&ev))><option value="">"None"</option>{options()}</select></label>
                        <label>"Second value "<input data-dnacalc="filter-value-2" prop:value=move ||value2.get() on:input=move |ev|value2.set(event_target_value(&ev))/></label>
                        <button data-dnacalc="filter-custom-apply" on:click=move |_|controller.apply_filter(true)>"Apply conditions"</button>
                    </div>
                </details>
            </Show>
        </details>
    }.into_any()
}
