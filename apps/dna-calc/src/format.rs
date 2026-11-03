//! Format Cells is an adapter over the shared command, draft and worker projection.
use crate::{
    controller::Controller,
    style::css,
    wire::{Intent, Reply, Request},
};
use dna_calc_model::format::{BorderPatch, FormatPatch};
use leptos::prelude::*;
use wasm_bindgen::JsCast;

fn text(
    controller: Controller,
    id: &'static str,
    label: &'static str,
    get: fn(&FormatPatch) -> &Option<String>,
    set: fn(&mut FormatPatch) -> &mut Option<String>,
) -> impl IntoView {
    let patch = controller.format_field(|s| &s.patch, |s| &mut s.patch);
    let selection = controller.format_field(|s| &s.selection, |s| &mut s.selection);
    view! {<label>{label}<input data-dnacalc=id prop:value=move ||patch.with(|p|get(p).clone()).or_else(||selection.with(|s|s.as_ref().and_then(|s|get(&s.common).clone()))).unwrap_or_default()
        placeholder=move ||if selection.with(|s|s.as_ref().is_some_and(|s|get(&s.common).is_none())) {"Mixed / unchanged"} else {""}
        on:input=move |ev|patch.update(|p|*set(p)=Some(event_target_value(&ev)))/></label>}.into_any()
}
fn choice(
    controller: Controller,
    id: &'static str,
    label: &'static str,
    get: fn(&FormatPatch) -> &Option<String>,
    set: fn(&mut FormatPatch) -> &mut Option<String>,
    options: &'static [(&'static str, &'static str)],
) -> impl IntoView {
    let patch = controller.format_field(|s| &s.patch, |s| &mut s.patch);
    let selection = controller.format_field(|s| &s.selection, |s| &mut s.selection);
    view!{<label>{label}<select data-dnacalc=id prop:value=move ||patch.with(|p|get(p).clone()).or_else(||selection.with(|s|s.as_ref().and_then(|s|get(&s.common).clone()))).unwrap_or_default()
        on:change=move |ev|{let v=event_target_value(&ev);patch.update(|p|*set(p)=(!v.is_empty()).then_some(v));}>
        <option value="">"Mixed / unchanged"</option>{options.iter().map(|(v,l)|view!{<option value=*v>{*l}</option>}).collect_view()}
    </select></label>}.into_any()
}
fn toggle(
    controller: Controller,
    id: &'static str,
    label: &'static str,
    get: fn(&FormatPatch) -> Option<bool>,
    set: fn(&mut FormatPatch) -> &mut Option<bool>,
) -> impl IntoView {
    let patch = controller.format_field(|s| &s.patch, |s| &mut s.patch);
    let selection = controller.format_field(|s| &s.selection, |s| &mut s.selection);
    view!{<label>{label}<select data-dnacalc=id prop:value=move ||patch.with(get).or_else(||selection.with(|s|s.as_ref().and_then(|s|get(&s.common)))).map_or("",|v|if v {"true"}else{"false"})
        on:change=move |ev|patch.update(|p|*set(p)=match event_target_value(&ev).as_str(){"true"=>Some(true),"false"=>Some(false),_=>None})>
        <option value="">"Mixed / unchanged"</option><option value="true">"On"</option><option value="false">"Off"</option>
    </select></label>}.into_any()
}
fn border_side(
    controller: Controller,
    id: &'static str,
    label: &'static str,
    get: fn(&BorderPatch) -> bool,
    set: fn(&mut BorderPatch) -> &mut bool,
) -> impl IntoView {
    let patch = controller.format_field(|s| &s.patch, |s| &mut s.patch);
    view!{<label><input type="checkbox" data-dnacalc=id disabled=move ||patch.with(|p|p.borders.is_none()) prop:checked=move ||patch.with(|p|p.borders.as_ref().is_some_and(get)) on:change=move |ev|patch.update(|p|{if let Some(b)=&mut p.borders {*set(b)=event_target_checked(&ev);}})/>{label}</label>}.into_any()
}
#[inline(never)]
pub fn controls(controller: Controller) -> impl IntoView {
    let dialog = NodeRef::<leptos::html::Dialog>::new();
    let tab = controller.format_field(|s| &s.tab, |s| &mut s.tab);
    let first = NodeRef::<leptos::html::Input>::new();
    let open = controller.format_field(|s| &s.open, |s| &mut s.open);
    let selection = controller.format_field(|s| &s.selection, |s| &mut s.selection);
    let patch = controller.format_field(|s| &s.patch, |s| &mut s.patch);
    let preview = controller.format_field(|s| &s.preview, |s| &mut s.preview);
    let message = controller.format_field(|s| &s.message, |s| &mut s.message);
    let epoch = controller.format_field(|s| &s.epoch, |s| &mut s.epoch);
    let close = move || {
        open.set(false);
        if let Some(d) = dialog.get_untracked() {
            // The browser restores the launch button while closing a modal. That is part of
            // this command, not a newer user focus request that cancels its return to the grid.
            #[cfg(target_arch = "wasm32")]
            controller.restoring_editor(|| d.close());
            #[cfg(not(target_arch = "wasm32"))]
            d.close();
        }
        controller.focus_active_skin();
    };
    Effect::new(move |_| {
        if open.get() {
            if let Some(d) = dialog.get() {
                if !d.open() {
                    let _ = d.show_modal();
                }
            }
            let areas = controller
                .areas
                .get_untracked()
                .into_iter()
                .map(crate::wire::Area::of)
                .collect();
            let generation = controller.generation.get_untracked();
            message.set("Reading selected formats…".into());
            controller.ask_result(Request::FormatSelection(areas), move |reply| {
                if !open.get_untracked() || controller.generation.get_untracked() != generation {
                    return;
                }
                match reply {
                    Ok(Reply::FormatSelection(Ok(value))) => {
                        preview.set(Some(value.sample.clone()));
                        selection.set(Some(value));
                        message.set(String::new());
                        if let Some(f) = first.get_untracked() {
                            let _ = f.focus();
                            f.select();
                        }
                    }
                    Ok(Reply::FormatSelection(Err(e))) | Err(e) => message.set(e),
                    _ => message.set("The format selection did not answer".into()),
                }
            });
        } else if let Some(d) = dialog.get() {
            if d.open() {
                d.close();
            }
        }
    });
    Effect::new(move |_| {
        let p = patch.get();
        let selected = selection.get();
        if !open.get_untracked() {
            return;
        }
        let Some(selected) = selected else {
            return;
        };
        epoch.update_value(|e| *e += 1);
        let request = epoch.get_value();
        let generation = controller.generation.get_untracked();
        controller.ask_result(
            Request::PreviewFormat {
                at: selected.sample.at,
                revision: selected.revision,
                patch: p,
            },
            move |reply| {
                if epoch.get_value() != request
                    || !open.get_untracked()
                    || controller.generation.get_untracked() != generation
                {
                    return;
                }
                match reply {
                    Ok(Reply::FormatPreview(Ok(cell))) => {
                        preview.set(Some(cell));
                        message.set(String::new());
                    }
                    Ok(Reply::FormatPreview(Err(e))) | Err(e) => {
                        preview.set(None);
                        message.set(e);
                    }
                    _ => {}
                }
            },
        );
    });
    let apply = move || {
        let Some(selected) = selection.get_untracked() else {
            return;
        };
        let generation = controller.generation.get_untracked();
        controller.dispatch(
            Intent::FormatCells {
                areas: selected.areas,
                revision: selected.revision,
                patch: patch.get_untracked(),
            },
            move |outcome| {
                if controller.generation.get_untracked() != generation || !open.get_untracked() {
                    return;
                }
                if outcome.accepted {
                    close();
                } else {
                    message.set(
                        outcome
                            .error
                            .clone()
                            .unwrap_or("The format was not applied".into()),
                    );
                }
            },
        );
    };
    view! {
        <dialog node_ref=dialog data-dnacalc="format-dialog" class="dc-format-dialog" aria-labelledby="dc-format-title" on:cancel=move |_:web_sys::Event|close() on:close=move |_:web_sys::Event|open.set(false)>
            <h2 id="dc-format-title">"Format Cells"</h2>
            <p class="dc-note">{move ||selection.with(|s|s.as_ref().map(|s|format!("{} · changes apply to the original selection in one Undo step",s.areas.iter().filter_map(|a|a.area()).map(|a|controller.area_label(a)).collect::<Vec<_>>().join(", "))).unwrap_or_default())}</p>
            <div class="dc-format-tabs" role="tablist" aria-label="Formatting categories" on:keydown=move |ev:web_sys::KeyboardEvent| {
                let n=match ev.key().as_str(){"ArrowRight"=>(tab.get_untracked()+1)%5,"ArrowLeft"=>(tab.get_untracked()+4)%5,"Home"=>0,"End"=>4,_=>return};
                ev.prevent_default();tab.set(n);
                if let Some(e)=web_sys::window().and_then(|w|w.document()).and_then(|d|d.get_element_by_id(&format!("dc-format-tab-{n}"))).and_then(|e|e.dyn_into::<web_sys::HtmlElement>().ok()){let _=e.focus();}
            }>{["Number","Alignment","Font","Fill","Borders"].into_iter().enumerate().map(|(i,label)|{let n=i as u8;view!{<button type="button" role="tab" id=format!("dc-format-tab-{n}") data-dnacalc=format!("format-tab-{}",label.to_ascii_lowercase()) aria-controls=format!("dc-format-panel-{n}") aria-selected=move ||(tab.get()==n).to_string() tabindex=move ||if tab.get()==n{0}else{-1} on:click=move |_|tab.set(n)>{label}</button>}}).collect_view()}</div>
            <div class="dc-format-fields">
                <fieldset role="tabpanel" id="dc-format-panel-0" aria-labelledby="dc-format-tab-0" hidden=move ||tab.get()!=0><legend>"Number"</legend>
                    <label>"Format code"<input node_ref=first data-dnacalc="format-number" placeholder="Mixed / unchanged" prop:value=move ||patch.with(|p|p.number_format.clone()).or_else(||selection.with(|s|s.as_ref().and_then(|s|s.common.number_format.clone()))).unwrap_or_default() on:input=move |ev|patch.update(|p|p.number_format=Some(event_target_value(&ev)))/></label>
                    <label>"Category"<select data-dnacalc="format-preset" on:change=move |ev|{let code=event_target_value(&ev);if !code.is_empty(){patch.update(|p|p.number_format=Some(code));}}>
                        <option value="">"Custom / unchanged"</option><option value="General">"General"</option><option value="#,##0.00">"Number"</option><option value="0%">"Percentage"</option><option value="yyyy-mm-dd">"Date"</option><option value="h:mm:ss">"Time"</option><option value="0.00E+00">"Scientific"</option><option value="@">"Text"</option>
                    </select></label>
                </fieldset>
                <fieldset role="tabpanel" id="dc-format-panel-1" aria-labelledby="dc-format-tab-1" hidden=move ||tab.get()!=1><legend>"Alignment"</legend>
                    {choice(controller,"format-horizontal","Horizontal",|p|&p.horizontal,|p|&mut p.horizontal,&[("general","General"),("left","Left"),("center","Centre"),("right","Right"),("fill","Fill"),("justify","Justify"),("centerContinuous","Centre across selection"),("distributed","Distributed")])}
                    {choice(controller,"format-vertical","Vertical",|p|&p.vertical,|p|&mut p.vertical,&[("top","Top"),("center","Centre"),("bottom","Bottom"),("justify","Justify"),("distributed","Distributed")])}
                    {toggle(controller,"format-wrap","Wrap text",|p|p.wrap,|p|&mut p.wrap)}
                    {toggle(controller,"format-shrink","Shrink to fit",|p|p.shrink,|p|&mut p.shrink)}
                    <label>"Rotation (degrees)"<input type="number" min="-90" max="90" data-dnacalc="format-rotation" placeholder="Mixed / unchanged" prop:value=move ||patch.with(|p|p.rotation).or_else(||selection.with(|s|s.as_ref().and_then(|s|s.common.rotation))).map(|n|n.to_string()).unwrap_or_default() on:input=move |ev|patch.update(|p|p.rotation=event_target_value(&ev).parse().ok())/></label>
                    <label>"Indent"<input type="number" min="0" max="250" data-dnacalc="format-indent" placeholder="Mixed / unchanged" prop:value=move ||patch.with(|p|p.indent).or_else(||selection.with(|s|s.as_ref().and_then(|s|s.common.indent))).map(|n|n.to_string()).unwrap_or_default() on:input=move |ev|patch.update(|p|p.indent=event_target_value(&ev).parse().ok())/></label>
                </fieldset>
                <fieldset role="tabpanel" id="dc-format-panel-2" aria-labelledby="dc-format-tab-2" hidden=move ||tab.get()!=2><legend>"Font"</legend>
                    {text(controller,"format-font","Family",|p|&p.font_name,|p|&mut p.font_name)}
                    {text(controller,"format-size","Size (points)",|p|&p.font_size,|p|&mut p.font_size)}
                    {toggle(controller,"format-bold","Bold",|p|p.bold,|p|&mut p.bold)}
                    {toggle(controller,"format-italic","Italic",|p|p.italic,|p|&mut p.italic)}
                    {toggle(controller,"format-underline","Underline",|p|p.underline,|p|&mut p.underline)}
                    {toggle(controller,"format-strike","Strikethrough",|p|p.strike,|p|&mut p.strike)}
                    {text(controller,"format-font-color","Colour (#rrggbb; empty = automatic)",|p|&p.font_color,|p|&mut p.font_color)}
                </fieldset>
                <fieldset role="tabpanel" id="dc-format-panel-3" aria-labelledby="dc-format-tab-3" hidden=move ||tab.get()!=3><legend>"Fill"</legend>
                    {text(controller,"format-fill","Solid colour (#rrggbb; empty = no fill)",|p|&p.fill,|p|&mut p.fill)}
                    <button type="button" data-dnacalc="format-no-fill" on:click=move |_|patch.update(|p|p.fill=Some(String::new()))>"No fill"</button>
                </fieldset>
                <fieldset role="tabpanel" id="dc-format-panel-4" aria-labelledby="dc-format-tab-4" hidden=move ||tab.get()!=4><legend>"Borders"</legend>
                    <label><input type="checkbox" data-dnacalc="format-border-replace" prop:checked=move ||patch.with(|p|p.borders.is_some()) on:change=move |ev|{let checked=event_target_checked(&ev);let base=selection.with_untracked(|s|s.as_ref().and_then(|s|s.common.borders.clone())).unwrap_or(BorderPatch {style:"thin".into(),color:"#000000".into(),..Default::default()});patch.update(|p|p.borders=checked.then_some(base));}/>"Replace the four sides"</label>
                    <p class="dc-note">"Mixed borders stay unchanged until you select Replace."</p>
                    <div class="dc-format-sides">
                        {border_side(controller,"format-border-left","Left",|b|b.left,|b|&mut b.left)}
                        {border_side(controller,"format-border-right","Right",|b|b.right,|b|&mut b.right)}
                        {border_side(controller,"format-border-top","Top",|b|b.top,|b|&mut b.top)}
                        {border_side(controller,"format-border-bottom","Bottom",|b|b.bottom,|b|&mut b.bottom)}
                    </div>
                    <label>"Line"<select data-dnacalc="format-border-style" disabled=move ||patch.with(|p|p.borders.is_none()) prop:value=move ||patch.with(|p|p.borders.as_ref().map(|b|b.style.clone())).unwrap_or("thin".into()) on:change=move |ev|patch.update(|p|{if let Some(b)=&mut p.borders{b.style=event_target_value(&ev);}})>
                        <option value="thin">"Thin"</option><option value="medium">"Medium"</option><option value="thick">"Thick"</option><option value="double">"Double"</option><option value="dotted">"Dotted"</option><option value="dashed">"Dashed"</option>
                    </select></label>
                    <label>"Colour"<input data-dnacalc="format-border-color" disabled=move ||patch.with(|p|p.borders.is_none()) prop:value=move ||patch.with(|p|p.borders.as_ref().map(|b|b.color.clone())).unwrap_or("#000000".into()) on:input=move |ev|patch.update(|p|{if let Some(b)=&mut p.borders{b.color=event_target_value(&ev);}})/></label>
                </fieldset>
            </div>
            <div class="dc-format-preview" aria-label="Base format preview of first selected cell">{move ||preview.get().map(|cell|{
                let mut style=crate::grid::autofit::font_css(&cell.style,cell.conditional.as_ref(),false,1.);
                if let Some(color)=cell.color.as_ref().or(cell.style.font_color.as_ref()){style.push_str(&format!(";color:{color}"));}
                if let Some(fill)=&cell.style.fill{style.push_str(&format!(";background-color:{fill}"));}
                style.push_str(crate::grid::autofit::decoration_css(cell.style.underline,cell.style.strike));
                view!{<output data-dnacalc="format-preview" use:css=style>{cell.text}</output><span class="dc-note">{if cell.format==crate::wire::FormatShown::NotModelled {"This format is preserved, but its display is not modelled"}else{""}}</span>}
            })}</div>
            <p class="dc-note">"Base format sample; conditional formatting can override the grid display."</p>
            <p role="status" data-dnacalc="format-message">{move ||message.get()}</p>
            <p class="dc-note" hidden=move ||selection.with(|s|s.as_ref().is_none_or(|s|s.revision==controller.summary.get().revision))>"The workbook changed. Close and reopen Format Cells."</p>
            <div class="dc-dialog-actions"><button type="button" data-dnacalc="format-apply" disabled=move ||selection.with(|s|s.as_ref().is_none_or(|s|s.revision!=controller.summary.get().revision))||preview.get().is_none() on:click=move |_|apply()>"Apply"</button><button type="button" data-dnacalc="format-cancel" on:click=move |_|close()>"Cancel"</button></div>
        </dialog>
    }.into_any()
}

pub fn diagnostic(controller: Controller) -> impl IntoView {
    let note = Memo::new(move |_| {
        if controller
            .active_skin
            .get()
            .is_some_and(|skin| skin != "grid")
        {
            return None;
        }
        controller.font_revision.track();
        let cell = controller.active_cell()?;
        if let Some(reason) = cell.style.unsupported {
            return Some(reason);
        }
        if !crate::grid::autofit::font_available(&cell.style) {
            return Some(format!(
                "Font {} is unavailable; the grid uses a fallback. The stored font is preserved.",
                cell.style.family.as_deref().unwrap_or("unknown")
            ));
        }
        (cell.format == crate::wire::FormatShown::NotModelled).then(|| {
            "The stored number format is preserved, but its display is not modelled".into()
        })
    });
    view! {<p class="dc-note" role="status" data-dnacalc="style-note" hidden=move ||note.get().is_none()>{move ||note.get().unwrap_or_default()}</p>}
}
