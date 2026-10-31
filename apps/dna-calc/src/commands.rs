//! Local workbook commands; searches and arithmetic are answered by the calculation worker.
use crate::{
    controller::Controller,
    wire::{self, Intent, Reply, Request},
};
use leptos::prelude::*;
use dna_calc_model::{Command, FindMode};
use wasm_bindgen::JsCast;

pub fn controls(controller: Controller) -> impl IntoView {
    let dialog = NodeRef::<leptos::html::Dialog>::new();
    let query_field = NodeRef::<leptos::html::Input>::new();
    let goto_field = NodeRef::<leptos::html::Input>::new();
    let query = RwSignal::new(String::new());
    let replacement = RwSignal::new(String::new());
    let target = RwSignal::new(String::new());
    let case = RwSignal::new(false);
    let whole = RwSignal::new(false);
    let wildcards = RwSignal::new(false);
    let formulas = RwSignal::new(true);
    let scope = RwSignal::new("sheet".to_owned());
    let found = RwSignal::new(None::<wire::Found>);
    let selected = RwSignal::new(0usize);
    let found_revision = RwSignal::new(0u64);
    let stale = move || found.get().is_some() && found_revision.get() != controller.revision.get();
    let message = RwSignal::new(String::new());
    let epoch = StoredValue::new(0u64);
    let anchor = StoredValue::new(None::<wire::Key>);
    let areas = StoredValue::new(Vec::<wire::Area>::new());
    let invalidate = move || {
        epoch.update_value(|n| *n += 1);
        found.set(None);
        message.set(String::new());
    };
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
            if let Reply::Found(result)=reply {match result {Ok(result)=>{message.set(if result.rows.is_empty(){if result.partial {"No match in inspected cells; narrow the scope to complete the search".into()}else{"No matches".into()}}else{format!("{} matches shown · {} occupied cells inspected{}",result.rows.len(),result.scanned,if result.partial {" · search limit reached"}else{""})});selected.set(0);found.set(Some(result));},Err(error)=>message.set(error)}}
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
        found.get().map(|f|f.rows.into_iter().enumerate().map(|(index,row)|{
        let hidden=row.hidden;let label=row.label;let source=row.source;let display=row.display;let provenance=row.provenance.name();
        view!{<li><button type="button" class="dc-find-hit" data-dnacalc="find-hit" aria-pressed=move ||(selected.get()==index).to_string() disabled=hidden title=if hidden {"Unhide this worksheet to navigate"}else{"Go to this cell"} on:click=move |_|navigate(index)><strong>{label}</strong><code>{source}</code><span>{display}" · "{provenance}{if hidden {" · hidden worksheet"}else{""}}</span></button></li>}
    }).collect_view())
    };
    let menu = NodeRef::<leptos::html::Dialog>::new();
    let permission = RwSignal::new(None::<Result<(), String>>);
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
                <div class="dc-find-options"><label>"Within"<select data-dnacalc="find-scope" on:change=move |ev|{scope.set(event_target_value(&ev));invalidate();}><option value="sheet">"Worksheet"</option><option value="selection">"Original selection"</option><option value="workbook">"Workbook, including hidden sheets"</option></select></label><label>"Look in"<select data-dnacalc="find-look" on:change=move |ev|{formulas.set(event_target_value(&ev)=="formulas");invalidate();}><option value="formulas">"Formulas & entries"</option><option value="values">"Displayed values"</option></select></label></div>
                <div class="dc-find-toggles"><label><input type="checkbox" data-dnacalc="find-case" on:change=move |ev|{case.set(event_target_checked(&ev));invalidate();}/ >"Match case"</label><label><input type="checkbox" data-dnacalc="find-whole" on:change=move |ev|{whole.set(event_target_checked(&ev));invalidate();}/ >"Whole cell"</label><label><input type="checkbox" data-dnacalc="find-wildcards" on:change=move |ev|{wildcards.set(event_target_checked(&ev));invalidate();}/ >"Wildcards (* ? ~)"</label></div>
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
    }
}

pub fn summary(controller: Controller) -> impl IntoView {
    let data = RwSignal::new(None::<Result<wire::SelectionStats, String>>);
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
