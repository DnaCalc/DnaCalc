//! Conditional rules, review details and explicit validation decisions through worker messages.
use crate::{
    controller::Controller,
    wire::{self, ConditionalChange, Intent, Reply, Request, Sheet},
};
use leptos::prelude::*;
use crate::style::css;

/// Only user-activated web and mail links become navigable anchors.
pub fn external_link(target: &str) -> Option<String> {
    let target = target.trim();
    let lower = target.to_ascii_lowercase();
    (!target.chars().any(|c| c.is_control())
        && ["https://", "http://", "mailto:"]
            .iter()
            .any(|p| lower.starts_with(p)))
    .then(|| target.to_owned())
}

use dna_calc_model::ReviewDraft as NoteDraft;

pub fn controls(controller: Controller) -> impl IntoView {
    let note_edit = controller.review_edit();
    let note_field = NodeRef::<leptos::html::Textarea>::new();
    let note_identity = Memo::new(move |_| {
        note_edit.with(|n| n.as_ref().map(|n| (n.session, n.at, n.index, n.threaded)))
    });
    Effect::new(move |_| {
        if controller.review_open.get()
            && note_identity.get().is_some()
            && let Some(field) = note_field.get()
        {
            let _ = field.focus();
        }
    });
    Effect::new(move |_| {
        let sheet = controller.sheet.get();
        let generation = controller.generation.get();
        let refresh = controller.revision.get();
        let revision = controller.summary.with(|s|s.revision);
        let epoch = controller.validation_epoch().get();
        let enabled =
            sheet.is_some_and(|s| controller.validation_circles().with(|all| all.contains(&s)));
        controller.validation_scan().set(None);
        if !enabled {
            return;
        }
        let sheet = sheet.expect("enabled validation sheet");
        controller.ask_result(Request::ValidationInvalid(Sheet::of(sheet)), move |reply| {
            if controller.sheet.get_untracked() == Some(sheet)
                && controller.generation.get_untracked() == generation
                && controller.revision.get_untracked() == refresh
                && controller.summary.with_untracked(|s|s.revision)==revision
                && controller.validation_epoch().get_untracked() == epoch
                && controller
                    .validation_circles()
                    .with_untracked(|all| all.contains(&sheet))
            {
                match reply {
                    Ok(Reply::ValidationInvalid(report))
                        if report.revision == revision && report.sheet.id() == sheet =>
                    {
                        controller.validation_scan().set(Some(Ok(report)));
                    }
                    Err(error) => controller.validation_scan().set(Some(Err(error))),
                    _ => {}
                }
            }
        });
    });
    let circle_controls = move || {
        view! {<section aria-labelledby="dc-validation-audit-title"><h3 id="dc-validation-audit-title">"Validation audit"</h3>
            <div class="dc-dialog-actions"><button data-dnacalc="validation-circle" on:click=move |_|controller.show_validation_circles(true)>"Circle Invalid Data"</button><button data-dnacalc="validation-clear-circles" on:click=move |_|controller.show_validation_circles(false)>"Clear circles"</button></div>
            {move ||controller.validation_scan().get().map(|result|match result {Err(error)=>view!{<p role="alert" data-dnacalc="validation-scan-error">{format!("Validation audit unavailable: {error}")}</p>}.into_any(),Ok(r)=>view!{<p data-dnacalc="validation-scan" role="status">{format!("{} invalid · {} valid · {} unavailable of {} checked{}",r.invalid.len(),r.valid,r.unavailable,r.checked,if r.truncated {"; more cells remain (10,000-cell limit)"}else{""})}</p>
                {r.reasons.into_iter().map(|(reason,count)|view!{<p class="dc-muted">{format!("{count}: {reason}")}</p>}).collect_view()}
                <div class="dc-invalid-links">{r.invalid.iter().take(50).filter_map(|key|key.cell()).map(|at|view!{<button data-dnacalc="validation-invalid-cell" on:click=move |_|{controller.review_open.set(false);controller.navigate_to(at,|_|{});}>{controller.cell_label(at)}</button>}).collect_view()}</div>
                {(r.invalid.len()>50).then(||view!{<p class="dc-muted">"The first 50 invalid addresses are listed; the grid marks all checked invalid cells."</p>})}
            }.into_any()})}
            {move ||(controller.sheet.get().is_some_and(|s|controller.validation_circles().with(|all|all.contains(&s))) && controller.validation_scan().get().is_none()).then(||view!{<p role="status">"Checking validation rules…"</p>})}
        </section>}
    };
    let validation = NodeRef::<leptos::html::Dialog>::new();
    Effect::new(move |_| {
        if let Some(dialog) = validation.get() {
            if controller.validation_decision.get().is_some() && !dialog.open() {
                let _ = dialog.show_modal();
            } else if controller.validation_decision.get().is_none() && dialog.open() {
                dialog.close();
            }
        }
    });
    let decide = move |choice| {
        if let Some(dialog) = validation.get_untracked() {
            dialog.close();
        }
        controller.validation_answer(choice);
    };
    let dialog = NodeRef::<leptos::html::Dialog>::new();
    let data = RwSignal::new(None::<wire::Review>);
    let selected = RwSignal::new(None::<u32>);
    let applies = RwSignal::new(String::new());
    let formulas = RwSignal::new(String::new());
    let stop = RwSignal::new(false);
    let kind = RwSignal::new("dataBar".to_owned());
    let color = RwSignal::new("327A94".to_owned());
    let new_formulas = RwSignal::new("0".to_owned());
    let operator = RwSignal::new("greaterThan".to_owned());
    let rank = RwSignal::new("10".to_owned());
    let percent = RwSignal::new(false);
    let new_stop = RwSignal::new(false);
    Effect::new(move |_| {
        controller.generation.track();
        controller.review_open.set(false);
        data.set(None);
        selected.set(None);
    note_edit.set(None);
    });
    Effect::new(move |_| {
        let open = controller.review_open.get();
        let at = controller.active_key();
        let revision = controller.revision.get();
        let generation = controller.generation.get();
        if !open {
            return;
        }
        data.set(None);
        selected.set(None);
        let Some(at) = at else {
            data.set(None);
            return;
        };
        controller.ask(Request::Review(wire::Key::of(at)), move |reply| {
            if controller.review_open.get_untracked()
                && controller.active_key_untracked() == Some(at)
                && controller.revision.get_untracked() == revision
                && controller.generation.get_untracked() == generation
                && let Reply::Review(found) = reply
            {
                data.set(Some(found));
                selected.set(None);
                applies.set(
                    controller
                        .selected_area()
                        .map(|a| {
                            format!(
                                "{}{}:{}{}",
                                dna_formula_ui::display::column_letters(a.first().column()),
                                a.first().row(),
                                dna_formula_ui::display::column_letters(a.last().column()),
                                a.last().row()
                            )
                        })
                        .unwrap_or_default(),
                );
            }
        });
    });
    Effect::new(move |_| {
        if let Some(dialog) = dialog.get() {
            if controller.review_open.get() && !dialog.open() {
                let _ = dialog.show_modal();
            } else if !controller.review_open.get() && dialog.open() {
                dialog.close();
            }
        }
    });
    let close = move || {
        if let Some(dialog) = dialog.get_untracked() {
            dialog.close();
        }
        controller.review_open.set(false);
        data.set(None);
        selected.set(None);
        crate::grid::focus_grid();
    };
    let submit = move |change: ConditionalChange| {
        if note_edit.get_untracked().is_some() {
            controller.say("Save or cancel the note edit first", true);
            return;
        }
        let (Some(sheet), Some(data)) = (controller.sheet.get_untracked(), data.get_untracked())
        else {
            return;
        };
        if data.at.and_then(wire::Key::cell) != controller.active_key_untracked()
            || data
                .at
                .is_none_or(|at| at.cell().is_none_or(|at| at.sheet() != sheet))
        {
            controller.say("Wait for the current cell's rules to load", true);
            return;
        }
        controller.dispatch(
            Intent::EditConditional {
                sheet: Sheet::of(sheet),
                revision: data.revision,
                index: selected.get_untracked(),
                change,
            },
            move |outcome| {
                if outcome.accepted {
                    selected.set(None);
                }
            },
        );
    };
    let choose = move |index: u32| {
        if let Some(rule) = data
            .get_untracked()
            .and_then(|d| d.rules.into_iter().find(|r| r.index == index))
        {
            selected.set(Some(index));
            applies.set(rule.applies);
            formulas.set(rule.formulas.join("\n"));
            stop.set(rule.stop);
        }
    };
    let current = Memo::new(move |_| {
        data.get().and_then(|d| {
            d.rules
                .into_iter()
                .find(|r| Some(r.index) == selected.get())
        })
    });
    let rules = move || {
        data.get().map(|d|d.rules.into_iter().map(|r|{
        let index=r.index;let title=format!("{} · {}",r.priority,r.kind);let state=r.state;let area=r.applies;
        view!{<li><button type="button" class="dc-rule-row" data-dnacalc="rule-row" data-index=index aria-pressed=move ||(selected.get()==Some(index)).to_string() on:click=move |_|choose(index)><strong>{title}</strong><span>{area}</span><small>{state}</small></button></li>}
    }).collect_view())
    };
    let begin_new=move |threaded:bool,parent:Option<u32>| {
        if note_edit.get_untracked().is_some(){return;}
        if let Some(d)=data.get_untracked() && let Some(at)=d.at && at.cell()==controller.active_key_untracked(){controller.begin_review_create(at,d.revision,d.next_index,threaded,parent);}
    };
    let format_note=move |patch:wire::ReviewFormat| {
        let Some(draft)=note_edit.get_untracked().filter(|d|!d.threaded&&d.creation.is_none())else{return;};
        let Some(field)=note_field.get_untracked()else{return;};
        let (mut start,mut end)=(field.selection_start().ok().flatten().unwrap_or(0),field.selection_end().ok().flatten().unwrap_or(0));
        if start==end{start=0;end=draft.text.encode_utf16().count() as u32;}
        let generation=controller.generation.get_untracked();
        controller.dispatch(Intent::FormatReview{at:draft.at,revision:draft.revision,index:draft.index,text:draft.text.clone(),start,end,patch},move |outcome|{
            if outcome.accepted&&controller.generation.get_untracked()==generation {note_edit.update(|current|{if let Some(current)=current&&current.session==draft.session&&current.at==draft.at&&current.index==draft.index&&current.revision==draft.revision{current.revision=outcome.summary.revision;}});}
        });
    };
    let notes = move || {
        data.get().map(|d|{
        if d.notes.is_empty(){return view!{<p class="dc-muted">"No note or comment on this cell."</p>}.into_any();}
        let at=d.at;let revision=d.revision;
            d.notes.into_iter().map(|n|{
                let index=n.index;let threaded=n.threaded;let draft=n.text.clone();
                view!{<article class="dc-review-note" data-dnacalc="review-note" data-note-index=index><h4>{n.author}<small>{if threaded {if n.reply {" · Reply"}else{" · Comment"}}else{" · Note"}}{n.resolved.then_some(" · Resolved")}</small></h4><p class="dc-note-rich" data-dnacalc="note-body">{n.runs.into_iter().map(|run|{
                    let decoration=match (run.underline,run.strike){(true,true)=>"underline line-through",(true,false)=>"underline",(false,true)=>"line-through",_=>"none"};
                    let color=run.color.map(|c|format!("#{}",if c.len()==8{c[2..].to_owned()}else{c})).unwrap_or_else(||"inherit".into());
                    let size=run.size.map(|s|format!("{s}pt")).unwrap_or_else(||"inherit".into());
                    view!{<span data-dnacalc="note-run" use:css=format!("font-weight:{};font-style:{};text-decoration:{decoration};color:{color};font-size:{size}",if run.bold{"700"}else{"400"},if run.italic{"italic"}else{"normal"})>{run.text}</span>}
                }).collect_view()}</p>{n.editable.then(||view!{<button data-dnacalc="note-edit" data-note-index=index disabled=move ||note_edit.get().is_some() on:click=move |_|{if let Some(at)=at {controller.begin_review_edit(at,revision,index,draft.clone(),threaded);}}>{if threaded {"Edit comment"}else{"Edit note"}}</button>})}
                    {(threaded&&!n.resolved).then(||view!{<button data-dnacalc="note-reply" data-note-index=index disabled=move ||note_edit.get().is_some() on:click=move |_|begin_new(true,Some(index))>"Reply"</button>})}
                    {(threaded&&!n.reply).then(||view!{<button data-dnacalc="note-resolve" disabled=move ||note_edit.get().is_some() on:click=move |_|{if let Some(at)=at{controller.dispatch(Intent::ResolveReview{at,revision,index,resolved:!n.resolved},|_|{});}}>{if n.resolved{"Reopen thread"}else{"Resolve thread"}}</button>})}
                    {(!n.editable).then(||view!{<p class="dc-muted">"Text editing is unavailable for this comment’s mentions or additional metadata; the file is preserved."</p>})}
                </article>}}).collect_view().into_any()})
    };
    let note_editor = move || {
        note_identity.get().map(|(_session,at,_index,threaded)|{
        let title=if threaded {"Comment"}else{"Note"};
        view!{<fieldset data-dnacalc="note-editor"><legend>{title}</legend>
            <p>{at.cell().map(|at|controller.cell_label(at))}</p>
            {move ||note_edit.get().and_then(|n|n.creation).map(|_|view!{<label>"Author"<input data-dnacalc="note-author" maxlength="255" prop:value=move ||note_edit.get().and_then(|n|n.creation).map(|c|c.author).unwrap_or_default() on:input=move |ev|note_edit.update(|n|{if let Some(n)=n&&let Some(c)=&mut n.creation{c.author=event_target_value(&ev);}})/></label>})}
            {move ||note_edit.get().filter(|n|!n.threaded&&n.creation.is_none()).map(|_|view!{
                <p class="dc-muted">"Select text to format, or format the whole note."</p>
                <div class="dc-dialog-actions" aria-label="Note formatting">
                    <button data-dnacalc="note-bold" on:mousedown=move |ev|ev.prevent_default() on:click=move |_|format_note(wire::ReviewFormat{bold:Some(true),..Default::default()})>"Bold"</button>
                    <button data-dnacalc="note-italic" on:mousedown=move |ev|ev.prevent_default() on:click=move |_|format_note(wire::ReviewFormat{italic:Some(true),..Default::default()})>"Italic"</button>
                    <button data-dnacalc="note-underline" on:mousedown=move |ev|ev.prevent_default() on:click=move |_|format_note(wire::ReviewFormat{underline:Some(true),..Default::default()})>"Underline"</button>
                    <label>"Text colour"<input type="color" data-dnacalc="note-color" value="#123456" on:change=move |ev|format_note(wire::ReviewFormat{color:Some(event_target_value(&ev)),..Default::default()})/></label>
                    <button data-dnacalc="note-clear-format" on:mousedown=move |ev|ev.prevent_default() on:click=move |_|format_note(wire::ReviewFormat{clear:true,..Default::default()})>"Clear formatting"</button>
                </div>
            })}
            <label>"Text"<textarea data-dnacalc="note-text" node_ref=note_field maxlength="32767" prop:value=move ||note_edit.get().map(|n|n.text).unwrap_or_default() on:input=move |ev|note_edit.update(|n|{if let Some(n)=n {n.text=event_target_value(&ev);}})></textarea></label>
            <div class="dc-dialog-actions"><button data-dnacalc="note-save" on:click=move |_|{
                let Some(draft)=note_edit.get_untracked() else{return;};
                if draft.at.cell()!=controller.active_key_untracked() {controller.say("Return to the note’s cell before saving; your text is retained",true);return;}
                let generation=controller.generation.get_untracked();
                let intent=if let Some(c)=&draft.creation{Intent::CreateReview{at:draft.at,revision:draft.revision,parent:c.parent,threaded:draft.threaded,author:c.author.clone(),text:draft.text.clone(),timestamp:c.timestamp.clone()}}else{Intent::EditReview {at:draft.at,revision:draft.revision,index:draft.index,text:draft.text.clone()}};
                controller.dispatch(intent,move |outcome|{
                    if outcome.accepted && controller.generation.get_untracked()==generation {
                        note_edit.update(|current|NoteDraft::accepted_save(current,&draft,outcome.summary.revision));
                    }
                });
            }>"Save text"</button><button data-dnacalc="note-cancel" on:click=move |_|note_edit.set(None)>"Cancel edit"</button></div>
        </fieldset>}
    })
    };
    let links = move || {
        data.get().map(|d|d.links.into_iter().map(|link|{
        let external=link.target.as_deref().and_then(external_link);
        let location=link.location.clone();let description=link.tooltip;let display=link.display;
        let target=link.target.clone();
        if let Some(href)=external {
            let href=if let Some(location)=location {format!("{href}#{location}")}else{href};
            view!{<div class="dc-review-link" data-dnacalc="review-link"><a href=href target="_blank" rel="noopener noreferrer" title=description>{display}</a><small>{target}</small></div>}.into_any()
        } else if target.is_none() && let Some(location)=location {
            view!{<div class="dc-review-link" data-dnacalc="review-link"><button title=description on:click=move |_|{close();controller.go_to_text(location.clone(),|_|{});}>{display}</button></div>}.into_any()
        } else {view!{<p class="dc-review-link" data-dnacalc="review-link">{display}" · Opening this link type is unavailable. "<code>{target}</code></p>}.into_any()}
    }).collect_view())
    };
    view! {

        <dialog class="dc-replace dc-validation-dialog" node_ref=validation data-dnacalc="validation-dialog" aria-labelledby="dc-validation-title" aria-describedby="dc-validation-message" on:cancel=move |ev:leptos::ev::Event|{ev.prevent_default();decide("cancel");}>
            <h2 id="dc-validation-title">{move ||controller.validation_decision.get().map(|d|format!("{} · {}",d.style,d.title)).unwrap_or_default()}</h2>
            <p id="dc-validation-message" data-dnacalc="validation-message">{move ||controller.validation_decision.get().map(|d|d.message).unwrap_or_default()}</p>
            <p class="dc-muted">"The cell has not changed."</p>
            <div class="dc-dialog-actions">
                {move ||controller.validation_decision.get().filter(|d|d.style!="Stop").map(|_|view!{<button data-dnacalc="validation-keep" on:click=move |_|decide("keep")>"Keep entry"</button>})}
                {move ||controller.validation_decision.get().filter(|d|d.style!="Information").map(|d|view!{<button data-dnacalc="validation-retry" on:click=move |_|decide("retry")>{if d.style=="Stop" {"Retry"} else {"Edit entry"}}</button>})}
                <button data-dnacalc="validation-cancel" on:click=move |_|decide("cancel")>"Cancel entry"</button>
            </div>
        </dialog>
        <dialog class="dc-replace dc-review-dialog" node_ref=dialog data-dnacalc="review-dialog" aria-labelledby="dc-review-title" on:cancel=move |ev:leptos::ev::Event|{ev.prevent_default();close();}>
            <div class="dc-review-heading"><div><h2 id="dc-review-title">"Manage Rules & review"</h2><p>{move ||controller.active_key().map(|at|controller.cell_label(at)).unwrap_or_default()}</p></div><button data-dnacalc="review-close" on:click=move |_|close()>"Close"</button></div>
            {move ||data.get().is_none().then(||view!{<p role="status">"Loading this cell’s rules and review details…"</p>})}
            <div class="dc-review-layout" aria-busy=move ||data.get().is_none().to_string()>
                <section aria-labelledby="dc-rules-title"><h3 id="dc-rules-title">"Conditional formats"</h3><p class="dc-muted">"Supported rules appear below; other imported rules remain preserved. Lowest priority number runs first."</p>
                    <ol class="dc-rule-list" data-dnacalc="rule-list">{rules}</ol>
                    {move ||data.get().filter(|d|d.rules.is_empty()).map(|_|view!{<p>"No supported rules to display on this worksheet."</p>})}
                    <div class="dc-dialog-actions"><button data-dnacalc="rule-up" disabled=move ||selected.get().is_none() on:click=move |_|submit(ConditionalChange::Move {up:true})>"Move up"</button><button data-dnacalc="rule-down" disabled=move ||selected.get().is_none() on:click=move |_|submit(ConditionalChange::Move {up:false})>"Move down"</button><button data-dnacalc="rule-remove" disabled=move ||current.get().is_none_or(|r|!r.editable) on:click=move |_|submit(ConditionalChange::Remove)>"Remove rule"</button></div>
                    <label>"Applies to"<input data-dnacalc="rule-applies" prop:value=move ||applies.get() on:input=move |ev|applies.set(event_target_value(&ev))/></label>
                    <p class="dc-muted">"Space-separated ranges on this worksheet, such as A1:A10 C1:C10."</p>
                    {move ||current.get().filter(|r|r.formula_editable).map(|_|view!{<label>"Condition formulas, one per line"<textarea data-dnacalc="rule-formulas" prop:value=move ||formulas.get() on:input=move |ev|formulas.set(event_target_value(&ev))></textarea></label>})}
                    {move ||current.get().filter(|r|!r.visual).map(|_|view!{<label class="dc-inline-label"><input type="checkbox" data-dnacalc="rule-stop" prop:checked=move ||stop.get() on:change=move |ev|stop.set(event_target_checked(&ev))/ >"Stop if true"</label>})}
                    {move ||current.get().filter(|r|!r.editable).map(|_|view!{<p>"This extension-backed rule is preserved. Its range, condition and removal cannot be edited yet."</p>})}
                    <button data-dnacalc="rule-apply" disabled=move ||current.get().is_none_or(|r|!r.editable) on:click=move |_|submit(ConditionalChange::Update {applies:applies.get_untracked(),formulas:formulas.get_untracked().lines().map(str::to_owned).collect(),stop:stop.get_untracked()})>"Apply rule changes"</button>
                    <fieldset><legend>"New conditional format"</legend>
                        <label>"Rule type"<select data-dnacalc="rule-kind" prop:value=move ||kind.get() on:change=move |ev|kind.set(event_target_value(&ev))>
                            <option value="cellIs">"Cell value"</option><option value="expression">"Formula is true"</option>
                            <option value="top10">"Top values"</option><option value="bottom10">"Bottom values"</option>
                            <option value="aboveAverage">"Above average"</option><option value="belowAverage">"Below average"</option>
                            <option value="duplicateValues">"Duplicate values"</option><option value="uniqueValues">"Unique values"</option>
                            <option value="dataBar">"Data bar"</option><option value="colorScale">"Colour scale"</option><option value="iconSet">"Three arrows"</option>
                        </select></label>
                        {move ||(kind.get()=="cellIs").then(||view!{<label>"Comparison"<select data-dnacalc="rule-operator" prop:value=move ||operator.get() on:change=move |ev|operator.set(event_target_value(&ev))>
                            <option value="greaterThan">"Greater than"</option><option value="lessThan">"Less than"</option><option value="equal">"Equal to"</option><option value="notEqual">"Not equal to"</option><option value="greaterThanOrEqual">"Greater than or equal to"</option><option value="lessThanOrEqual">"Less than or equal to"</option><option value="between">"Between"</option><option value="notBetween">"Not between"</option>
                        </select></label>})}
                        {move ||matches!(kind.get().as_str(),"cellIs"|"expression").then(||view!{<label>"Condition formulas, one per line"<textarea data-dnacalc="rule-new-formulas" prop:value=move ||new_formulas.get() on:input=move |ev|new_formulas.set(event_target_value(&ev))></textarea></label><p class="dc-muted">"Relative references start at the first cell of Applies to. Between comparisons need two lines."</p>})}
                        {move ||matches!(kind.get().as_str(),"top10"|"bottom10").then(||view!{<label>"Rank"<input data-dnacalc="rule-rank" inputmode="numeric" prop:value=move ||rank.get() on:input=move |ev|rank.set(event_target_value(&ev))/></label><label class="dc-inline-label"><input type="checkbox" data-dnacalc="rule-percent" prop:checked=move ||percent.get() on:change=move |ev|percent.set(event_target_checked(&ev))/ >"Percent"</label>})}
                        <label>"RGB colour"<input data-dnacalc="rule-color" maxlength="7" prop:value=move ||color.get() on:input=move |ev|color.set(event_target_value(&ev))/></label>
                        {move ||(!matches!(kind.get().as_str(),"dataBar"|"colorScale"|"iconSet")).then(||view!{<label class="dc-inline-label"><input type="checkbox" data-dnacalc="rule-new-stop" prop:checked=move ||new_stop.get() on:change=move |ev|new_stop.set(event_target_checked(&ev))/ >"Stop if true"</label>})}
                        <button data-dnacalc="rule-add" disabled=move ||data.get().is_none() on:click=move |_|{
                            let kind=kind.get_untracked();
                            if matches!(kind.as_str(),"dataBar"|"colorScale"|"iconSet") {submit(ConditionalChange::Add {kind,applies:applies.get_untracked(),color:color.get_untracked()});}
                            else {
                                let number=if matches!(kind.as_str(),"top10"|"bottom10") {match rank.get_untracked().trim().parse::<u32>() {Ok(n)=>n,Err(_)=>{controller.say("Enter a whole-number rank",true);return;}}}else{10};
                                let formulas=if matches!(kind.as_str(),"cellIs"|"expression") {new_formulas.get_untracked().lines().map(str::to_owned).collect()}else{Vec::new()};
                                submit(ConditionalChange::Condition {kind,applies:applies.get_untracked(),color:color.get_untracked(),formulas,operator:operator.get_untracked(),rank:number,percent:percent.get_untracked(),stop:new_stop.get_untracked()});
                            }
                        }>"Add rule"</button>
                    </fieldset>
                    <div class="dc-dialog-actions"><button data-dnacalc="review-undo" disabled=move ||controller.summary.get().undo==0 || note_edit.get().is_some() on:click=move |_|controller.undo()>"Undo"</button><button data-dnacalc="review-redo" disabled=move ||controller.summary.get().redo==0 || note_edit.get().is_some() on:click=move |_|controller.redo()>"Redo"</button></div>
                </section>
                <section aria-labelledby="dc-notes-title"><h3 id="dc-notes-title">"Notes, comments & links"</h3>{circle_controls}<div class="dc-dialog-actions"><button data-dnacalc="note-new" disabled=move ||data.get().is_none_or(|d|!d.notes.is_empty())||note_edit.get().is_some() on:click=move |_|begin_new(false,None)>"New note"</button><button data-dnacalc="comment-new" disabled=move ||data.get().is_none_or(|d|!d.notes.is_empty())||note_edit.get().is_some() on:click=move |_|begin_new(true,None)>"New comment"</button></div>{note_editor}{notes}{links}<p class="dc-muted">"Notes, replies and formatting use Undo and XLSX saving. Imported author identities and thread relationships are retained."</p>{move ||data.get().and_then(|d|d.validation).map(|v|view!{<h3>"Data validation"</h3><p data-dnacalc="review-validation">{v}</p>})}{move ||data.get().map(|d|d.unsupported.into_iter().map(|text|view!{<p>{text}</p>}).collect_view())}</section>
            </div>
            <p class="dc-review-status" role="status" data-dnacalc="review-status">{move ||controller.status.get().text}</p>
        </dialog>
    }
}

#[cfg(test)]
mod tests {
    use super::external_link;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn only_explicit_web_or_mail_targets_are_links() {
        assert!(external_link(" https://example.test/a?q=b ").is_some());
        assert!(external_link("HTTP://example.test").is_some());
        assert!(external_link("mailto:a@example.test").is_some());
        for target in [
            "javascript:alert(1)",
            "data:text/html,hello",
            "file:///tmp/a",
            "https:\n//example.test",
            "//example.test",
            "tel:123",
        ] {
            assert!(external_link(target).is_none(), "{target}");
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn validation_list_stops_at_edges_and_handles_empty_lists() {
        let next = crate::grid::validation_list_index;
        assert_eq!(next(0, "ArrowUp", 3), Some(0));
        assert_eq!(next(2, "ArrowDown", 3), Some(2));
        assert_eq!(next(8, "Home", 3), Some(0));
        assert_eq!(next(0, "End", 3), Some(2));
        assert_eq!(next(0, "ArrowDown", 0), None);
        assert_eq!(next(0, "Enter", 3), None);
    }
}
