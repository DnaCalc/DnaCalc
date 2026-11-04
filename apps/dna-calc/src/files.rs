//! File kinds and personal templates over the existing worker open/save routes.
use crate::{app::Replacement, controller::Controller, wire::{self, FileFormat}};
use leptos::prelude::*;

type Templates = Vec<(String, Vec<u8>)>;
const STORE: &str = "dnacalc.personal-templates.v1";
fn templates() -> Templates {
    #[cfg(target_arch="wasm32")]
    if let Ok(Some(storage))=window().local_storage() {
        if let Ok(Some(text))=storage.get_item(STORE) {return wire::decode(&text).unwrap_or_default();}
    }
    Vec::new()
}
fn store(items:&Templates)->Result<(),String> {
    #[cfg(target_arch="wasm32")]
    {let storage=window().local_storage().map_err(|_|"Template storage is unavailable")?.ok_or("Template storage is unavailable")?;
    storage.set_item(STORE,&wire::encode(items)).map_err(|_|"The browser could not store these templates; its storage may be full".to_owned())}
    #[cfg(not(target_arch="wasm32"))]
    {let _=items;Ok(())}
}
pub(crate) fn default_template(stem:&str)->Option<(String,Vec<u8>)> {
    templates().into_iter().find(|(n,_)| n.eq_ignore_ascii_case(&format!("{stem}.xltx")) || n.eq_ignore_ascii_case(&format!("{stem}.xltm")))
}
fn format(value:&str)->FileFormat {match value {"xlsm"=>FileFormat::Xlsm,"xltx"=>FileFormat::Xltx,"xltm"=>FileFormat::Xltm,_=>FileFormat::Xlsx}}

pub(crate) fn controls(controller:Controller,replace:Callback<Replacement>,save_open:RwSignal<bool>)->impl IntoView {
    let items=RwSignal::new(templates());
    let selected=RwSignal::new(String::new());
    let input=NodeRef::<leptos::html::Input>::new();
    let editing=NodeRef::<leptos::html::Input>::new();
    let menu=NodeRef::<leptos::html::Details>::new();
    let dialog=NodeRef::<leptos::html::Dialog>::new();
    let name=RwSignal::new(String::new());
    let kind=RwSignal::new("xlsx".to_owned());
    Effect::new(move |_| {
        if save_open.get() {
            let summary=controller.summary.get_untracked();
            name.set(summary.name.rsplit_once('.').map_or(summary.name.as_str(),|(n,_)|n).to_owned());
            kind.set(match(summary.template,summary.macro_enabled){(true,true)=>"xltm",(true,false)=>"xltx",(false,true)=>"xlsm",_=>"xlsx"}.into());
        }
        if let Some(dialog)=dialog.get() {
            if save_open.get() && !dialog.open(){let _=dialog.show_modal();}
            if !save_open.get() && dialog.open(){dialog.close();}
        }
    });
    let preparing=controller.saving;
    let close=move || {if preparing.get_untracked(){return;}save_open.set(false);crate::app::focus("[data-dnacalc=save]");};
    let change_store=move |next:Templates| match store(&next) {
        Ok(())=>{items.set(next);controller.say("Personal templates stored in this browser",false);},
        Err(e)=>controller.say(e,true),
    };
    let chosen=move ||items.get_untracked().into_iter().find(|(n,_)|n==&selected.get_untracked());
    view! {
        <details class="dc-file-menu" node_ref=menu data-dnacalc="file-menu" on:keydown=move |ev:leptos::ev::KeyboardEvent|{if ev.key()=="Escape"{ev.prevent_default();if let Some(menu)=menu.get_untracked(){menu.set_open(false);}crate::app::focus("[data-dnacalc=file-toggle]");}}>
            <summary data-dnacalc="file-toggle">"File"</summary>
            <div class="dc-file-body">
                <h2>"Workbooks and templates"</h2>
                <button data-dnacalc="save-as" disabled=move ||controller.assist.readonly.get() on:click=move |_|{if let Some(m)=menu.get_untracked(){m.set_open(false);}controller.prepare_save_as(move ||save_open.set(true));}>"Save As…"</button>
                <button data-dnacalc="open-editing" disabled=move ||controller.assist.readonly.get() on:click=move |_|{if let Some(m)=menu.get_untracked(){m.set_open(false);}if let Some(i)=editing.get(){i.click();}}>"Open for editing…"</button>
                <input node_ref=editing type="file" hidden=true accept=".xlsx,.xlsm,.xlsb,.xltx,.xltm" data-dnacalc="open-editing-file" on:change=move |ev|{use wasm_bindgen::JsCast;if let Some(i)=ev.target().and_then(|t|t.dyn_into::<web_sys::HtmlInputElement>().ok()){crate::app::open_chosen(controller,i,move |name,bytes|{if let Some(m)=menu.get_untracked(){m.set_open(false);}replace.run(Replacement::OpenEditing(name,bytes));});}}/>
                <h3>"New from template"</h3>
                <p>"Choose personal templates to keep in this browser. Book.xltx or Book.xltm supplies New workbooks."</p>
                <button data-dnacalc="template-add" on:click=move |_|{if let Some(i)=input.get(){i.click();}}>"Add personal template…"</button>
                <input node_ref=input type="file" hidden=true accept=".xltx,.xltm" data-dnacalc="template-file" on:change=move |ev|{use wasm_bindgen::JsCast;if let Some(i)=ev.target().and_then(|t|t.dyn_into::<web_sys::HtmlInputElement>().ok()){crate::app::open_chosen(controller,i,move |name,bytes|{
                    if !["xltx","xltm"].iter().any(|e|name.to_ascii_lowercase().ends_with(&format!(".{e}"))){controller.say("Choose an .xltx or .xltm template",true);return;}
                    let mut next=items.get_untracked();next.retain(|(n,_)|!n.eq_ignore_ascii_case(&name));next.push((name.clone(),bytes));change_store(next);selected.set(name);
                });}}/>
                <label>"Personal template"<select data-dnacalc="template-list" prop:value=move ||selected.get() on:change=move |ev|selected.set(event_target_value(&ev))>
                    <option value="">"Choose a template"</option>
                    {move ||items.get().into_iter().map(|(n,_)|view!{<option value=n.clone()>{n.clone()}</option>}).collect_view()}
                </select></label>
                <button data-dnacalc="template-new" disabled=move ||selected.get().is_empty() || controller.assist.readonly.get() on:click=move |_|{if let Some((name,bytes))=chosen(){if let Some(m)=menu.get_untracked(){m.set_open(false);}replace.run(Replacement::Open(name,bytes));}}>"Create workbook"</button>
                <button data-dnacalc="template-remove" disabled=move ||selected.get().is_empty() on:click=move |_|{let mut next=items.get_untracked();next.retain(|(n,_)|n!=&selected.get_untracked());change_store(next);selected.set(String::new());}>"Remove from personal templates"</button>
            </div>
        </details>
        <dialog node_ref=dialog class="dc-replace dc-save-as" data-dnacalc="save-as-dialog" aria-labelledby="dc-save-as-title" on:cancel=move |ev:leptos::ev::Event|{ev.prevent_default();close();}>
            <h2 id="dc-save-as-title">"Save As"</h2>
            <label>"File name"<input data-dnacalc="save-as-name" disabled=move ||preparing.get() prop:value=move ||name.get() on:input=move |ev|name.set(event_target_value(&ev))/></label>
            <label>"File type"<select data-dnacalc="save-as-format" disabled=move ||preparing.get() prop:value=move ||kind.get() on:change=move |ev|kind.set(event_target_value(&ev))>
                <option value="xlsx">"Excel workbook (.xlsx)"</option><option value="xlsm">"Excel macro-enabled workbook (.xlsm)"</option><option value="xltx">"Excel template (.xltx)"</option><option value="xltm">"Excel macro-enabled template (.xltm)"</option>
            </select></label>
            <p data-dnacalc="save-as-warning" role="status">{move ||if controller.summary.get().macro_enabled && matches!(format(&kind.get()),FileFormat::Xlsx|FileFormat::Xltx){"VBA would be lost in this format. If the file carries VBA or Excel macro sheets, saving is refused. Choose a macro-enabled file type."}else{"The file downloads to your browser. Check your downloads before closing this workbook."}}</p>
            <div class="dc-dialog-actions"><button data-dnacalc="save-as-cancel" disabled=move ||preparing.get() on:click=move |_|close()>"Keep editing"</button><button data-dnacalc="save-as-confirm" disabled=move ||preparing.get() on:click=move |_|{
                let format=format(&kind.get_untracked());let stem=name.get_untracked();let stem=stem.trim();if stem.is_empty(){controller.say("Type a file name",true);return;}
                let name=if [".xlsx",".xlsm",".xltx",".xltm"].iter().any(|e|stem.to_ascii_lowercase().ends_with(e)){let n=stem.rsplit_once('.').unwrap().0;format!("{n}.{}",format.extension())}else{format!("{stem}.{}",format.extension())};
                controller.save_as(name,format,move |name,bytes|{match crate::app::download(&name,&bytes){Ok(())=>{close();controller.say(format!("Download started: {name}. Check your downloads; durable storage is unconfirmed."),false);},Err(e)=>controller.say(e,true)}});
            }>{move ||if preparing.get(){"Preparing…"}else{"Download"}}</button></div>
        </dialog>
    }
}
