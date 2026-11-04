//! Worksheet lifecycle controls; workbook state and validation stay in the calculation worker.
use crate::{controller::Controller, wire::SheetChange};
use leptos::prelude::*;

/// The next unused conventional worksheet name, including hidden sheets in the comparison.
pub fn next_name(names: &[String]) -> String {
    (1u32..)
        .map(|n| format!("Sheet{n}"))
        .find(|name| {
            names
                .iter()
                .all(|held| !calc_value::workbook_names_equal(held, name))
        })
        .expect("an unused sheet name")
}

pub fn controls(controller: Controller) -> impl IntoView {
    let rename = RwSignal::new(String::new());
    let add = RwSignal::new(String::new());
    let position = RwSignal::new(String::new());
    let delete = RwSignal::new(false);
    let dialog = NodeRef::<leptos::html::Dialog>::new();
    let tools = NodeRef::<leptos::html::Details>::new();
    Effect::new(move |_| {
        controller.generation.track();
        if let Some(tools) = tools.get() {
            tools.set_open(false);
        }
        delete.set(false);
    });
    Effect::new(move |_| {
        let active = controller.sheet.get();
        let summary = controller.summary.get();
        rename.set(
            summary
                .sheets
                .iter()
                .find(|(s, _)| Some(s.id()) == active)
                .map(|(_, n)| n.clone())
                .unwrap_or_default(),
        );
        position.set(
            summary
                .sheet_order
                .iter()
                .position(|s| Some(s.id()) == active)
                .map(|i| (i + 1).to_string())
                .unwrap_or_default(),
        );
    });
    Effect::new(move |_| {
        if let Some(dialog) = dialog.get() {
            if delete.get() && !dialog.open() {
                let _ = dialog.show_modal();
            }
            if !delete.get() && dialog.open() {
                dialog.close();
            }
        }
    });
    let current = move || controller.sheet.get_untracked();
    let cancel = move || {
        if let Some(dialog) = dialog.get_untracked() {
            dialog.close();
        }
        delete.set(false);
        crate::grid::focus_grid();
    };
    let rename_sheet = move || {
        controller.sheet_command(
            current(),
            SheetChange::Rename {
                name: rename.get_untracked(),
            },
            |_| {},
        )
    };
    let add_sheet = move || {
        let text = add.get_untracked();
        let name = if text.is_empty() {
            let summary = controller.summary.get_untracked();
            let names: Vec<_> = summary
                .sheets
                .iter()
                .map(|(_, n)| n.clone())
                .chain(summary.hidden_sheets.iter().map(|(_, n, _)| n.clone()))
                .collect();
            next_name(&names)
        } else {
            text
        };
        let done=move |accepted|if accepted{add.set(String::new());};
        if let Some((_,bytes))=crate::files::default_template("Sheet") {controller.insert_template(bytes,Some(name),done);}
        else {controller.sheet_command(None, SheetChange::Add{name},done);}
    };
    view! {
        <details class="dc-sheet-tools" data-dnacalc="sheet-tools" node_ref=tools on:keydown=move |ev:leptos::ev::KeyboardEvent| {
            if ev.key()=="Escape" {ev.prevent_default();if let Some(tools)=tools.get_untracked(){tools.set_open(false);}crate::grid::focus_grid();}
        }>
            <summary>"Sheets"</summary>
            <div class="dc-sheet-body">
                <h2>"Worksheets"</h2>
                <label>"Go to sheet"<select data-dnacalc="sheet-jump" prop:value=move || controller.sheet.get().map(|s| format!("{}:{}",s.slot(),s.generation())).unwrap_or_default()
                    on:change=move |ev| { let value=event_target_value(&ev); if let Some(sheet)=controller.sheets().into_iter().find(|s| format!("{}:{}",s.id.slot(),s.id.generation()) == value) { controller.show_sheet(sheet.id); } }>
                    {move || controller.sheets().into_iter().map(|s|view!{<option value=format!("{}:{}",s.id.slot(),s.id.generation())>{s.name}</option>}).collect_view()}
                </select></label>
                <label>"New sheet name"<input data-dnacalc="sheet-new-name" placeholder="Automatic name" prop:value=move ||add.get() on:input=move |ev|add.set(event_target_value(&ev)) on:keydown=move |ev:leptos::ev::KeyboardEvent|{if ev.key()=="Enter" {ev.prevent_default();add_sheet();}}/></label>
                <button data-dnacalc="sheet-add" on:click=move |_|add_sheet()>"Add worksheet"</button>
                <label>"Current sheet name"<input data-dnacalc="sheet-rename-name" prop:value=move ||rename.get() on:input=move |ev|rename.set(event_target_value(&ev)) on:keydown=move |ev:leptos::ev::KeyboardEvent|{if ev.key()=="Enter" {ev.prevent_default();rename_sheet();}}/></label>
                <button data-dnacalc="sheet-rename" on:click=move |_|rename_sheet()>"Rename worksheet"</button>
                <label>"Position in workbook"<input data-dnacalc="sheet-position" inputmode="numeric" prop:value=move ||position.get() on:input=move |ev|position.set(event_target_value(&ev))/></label>
                <button data-dnacalc="sheet-move" on:click=move |_| { match position.get_untracked().parse::<u32>() { Ok(position) => controller.sheet_command(current(),SheetChange::Move { position },|_|{}), Err(_) => controller.say("Type a worksheet position",true) } }>"Move worksheet"</button>
                <div class="dc-command-row">
                    <button data-dnacalc="sheet-hide" on:click=move |_|controller.sheet_command(current(),SheetChange::Hide,|_|{})>"Hide worksheet"</button>
                    <button data-dnacalc="sheet-delete" on:click=move |_|delete.set(true)>"Delete worksheet…"</button>
                </div>
                <h3>"Hidden worksheets"</h3>
                <div data-dnacalc="hidden-sheets">{move || {
                    let hidden=controller.summary.get().hidden_sheets.clone();
                    if hidden.is_empty() {return view!{<p>"No hidden worksheets"</p>}.into_any();}
                    hidden.into_iter().map(|(sheet,name,very)|view!{
                        <div class="dc-hidden-sheet"><span>{name}</span><button data-dnacalc="sheet-unhide" data-sheet=format!("{}:{}",sheet.0,sheet.1) disabled=very title=if very {"VeryHidden sheets cannot be shown here"} else {"Show this worksheet"}
                            on:click=move |_|controller.sheet_command(Some(sheet.id()),SheetChange::Show,move |ok|{if ok {controller.show_sheet(sheet.id());}})>{if very {"VeryHidden"} else {"Unhide"}}</button></div>
                    }).collect_view().into_any()
                }}</div>
            </div>
        </details>
        <dialog class="dc-replace" node_ref=dialog data-dnacalc="sheet-delete-dialog" aria-labelledby="dc-sheet-delete-title" on:cancel=move |ev: leptos::ev::Event|{ev.prevent_default();cancel();}>
            <h2 id="dc-sheet-delete-title">"Delete worksheet?"</h2>
            <p>{move ||format!("Delete {} and its contents? References to it become #REF!. You can Undo this change.",controller.summary.get().sheets.iter().find(|(s,_)|Some(s.id())==controller.sheet.get()).map(|(_,n)|n.as_str()).unwrap_or("this worksheet"))}</p>
            <div class="dc-dialog-actions"><button data-dnacalc="sheet-delete-cancel" on:click=move |_|cancel()>"Keep worksheet"</button><button data-dnacalc="sheet-delete-confirm" on:click=move |_|controller.sheet_command(current(),SheetChange::Remove,move |ok|{if ok {cancel();}})>"Delete worksheet"</button></div>
        </dialog>
    }
}

#[cfg(test)]
mod tests {
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn conventional_name_includes_hidden_and_case_insensitive_names() {
        assert_eq!(
            super::next_name(&["Sheet1".into(), "sHEET2".into(), "Sheet4".into()]),
            "Sheet3"
        );
    }
}
