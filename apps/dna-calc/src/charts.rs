//! Revision-bound chart properties, submitted through the workbook command route.
use crate::{controller::Controller, wire};
use leptos::prelude::*;
use wasm_bindgen::JsCast;

pub fn controls(controller: Controller) -> AnyView {
    let open = RwSignal::new(false);
    let charts = RwSignal::new(Vec::<wire::Chart>::new());
    let chosen = controller.tool_field(|s| &s.chart.part, |s| &mut s.chart.part);
    let title = controller.tool_field(|s| &s.chart.title, |s| &mut s.chart.title);
    let legend = controller.tool_field(|s| &s.chart.legend, |s| &mut s.chart.legend);
    let ready = RwSignal::new(None);
    let target = Memo::new(move |_| {
        (
            controller.generation.get(),
            controller.summary.get().revision,
            controller.sheet.get(),
        )
    });
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        let (generation, revision, sheet) = target.get();
        let Some(sheet) = sheet else {
            charts.set(Vec::new());
            return;
        };
        ready.set(None);
        controller.ask(
            wire::Request::Charts(wire::Sheet::of(sheet)),
            move |reply| {
                if open.try_get_untracked() != Some(true)
                    || controller.generation.get_untracked() != generation
                    || controller.summary.get_untracked().revision != revision
                    || controller.sheet.get_untracked() != Some(sheet)
                {
                    return;
                }
                if let wire::Reply::Charts(values) = reply {
                    let part = chosen.get_untracked();
                    if !values.iter().any(|chart| chart.part == part) {
                        chosen.set(
                            values
                                .first()
                                .map(|chart| chart.part.clone())
                                .unwrap_or_default(),
                        );
                    }
                    charts.set(values);
                    ready.set(Some((generation, revision, sheet)));
                }
            },
        );
    });
    let current = Memo::new(move |_| {
        let part = chosen.get();
        charts.get().into_iter().find(|chart| chart.part == part)
    });
    Effect::new(move |_| {
        open.track();
        let chart = current.get();
        title.set(
            chart
                .as_ref()
                .and_then(|chart| chart.title.clone())
                .unwrap_or_default(),
        );
        legend.set(chart.as_ref().is_some_and(|chart| chart.legend.is_some()));
    });
    let apply = move |_| {
        let Some((generation, revision, sheet)) = ready.get_untracked() else {
            return;
        };
        let Some(chart) = current.get_untracked() else {
            return;
        };
        let draft = title.get_untracked();
        let set_title = draft != chart.title.as_deref().unwrap_or_default();
        let show = legend.get_untracked();
        let legend = (show != chart.legend.is_some()).then_some(show);
        if !set_title && legend.is_none() {
            return;
        }
        controller.after_input(move || {
            if controller.generation.get_untracked() != generation
                || controller.summary.get_untracked().revision != revision
                || controller.sheet.get_untracked() != Some(sheet)
                || controller.editing.get_untracked().is_some()
            {
                controller.say("The workbook changed; reopen the chart controls", true);
                return;
            }
            controller.dispatch(
                wire::Intent::ChartProperties {
                    sheet: wire::Sheet::of(sheet),
                    revision,
                    part: chart.part,
                    title: (!draft.is_empty()).then_some(draft),
                    set_title,
                    legend,
                },
                move |outcome| {
                    if outcome.accepted {
                        controller.say("Chart updated", false);
                    }
                },
            );
        });
    };
    view! {
        <details class="dc-clipboard-tools" name="worksheet-tools" data-dnacalc="chart-tools"
            on:toggle=move |ev| {if let Some(details)=ev.target().and_then(|target|target.dyn_into::<web_sys::HtmlDetailsElement>().ok()) {open.set(details.open());}}
            on:keydown=move |ev:leptos::ev::KeyboardEvent| {
                if ev.key()=="Escape" {ev.prevent_default();ev.stop_propagation();
                    if let Some(details)=document().query_selector("[data-dnacalc=chart-tools]").ok().flatten().and_then(|e|e.dyn_into::<web_sys::HtmlDetailsElement>().ok()){details.set_open(false);}
                    if let Some(summary)=document().query_selector("[data-dnacalc=chart-tools-toggle]").ok().flatten().and_then(|e|e.dyn_into::<web_sys::HtmlElement>().ok()){let _=summary.focus();}
                }
            }>
            <summary data-dnacalc="chart-tools-toggle">"Charts"</summary>
            <Show when=move ||charts.get().is_empty()><p class="dc-note">"This worksheet has no readable charts. Open a workbook containing a chart to edit its title and legend."</p></Show>
            <Show when=move ||!charts.get().is_empty()>
                <div class="dc-command-row"><label>"Chart "<select data-dnacalc="chart-choice" prop:value=move ||chosen.get() on:change=move |ev| {
                    let part=event_target_value(&ev);
                    chosen.set(part.clone());
                    let at=charts.get_untracked().into_iter().find(|chart|chart.part==part).and_then(|chart|match chart.place {
                        wire::ChartPlace::Cells{from,..}|wire::ChartPlace::Sized{from,..}=>from.0.cell(),
                        wire::ChartPlace::Absolute{..}=>None,
                    });
                    if let (Some(at),Some((generation,revision,sheet)))=(at,ready.get_untracked()) {
                        controller.after_input(move || {
                            if controller.generation.get_untracked()==generation && controller.summary.get_untracked().revision==revision && controller.sheet.get_untracked()==Some(sheet) && at.sheet()==sheet && controller.editing.get_untracked().is_none() {
                                controller.go_to(at);
                            }
                        });
                    }
                }>
                    {move ||charts.get().into_iter().map(|chart|view! {<option value=chart.part>{chart.name}</option>}).collect_view()}
                </select></label></div>
                <div class="dc-command-row"><label>"Title "<input data-dnacalc="chart-edit-title" maxlength="32767" prop:value=move ||title.get() on:input=move |ev|title.set(event_target_value(&ev))/></label>
                    <label><input data-dnacalc="chart-edit-legend" type="checkbox" prop:checked=move ||legend.get() on:change=move |ev|legend.set(event_target_checked(&ev))/>"Show legend"</label>
                    <button type="button" data-dnacalc="chart-edit-apply" disabled=move ||ready.get().is_none() || controller.assist.readonly.get() || controller.editing.get().is_some() on:click=apply>"Apply"</button>
                </div>
                <Show when=move ||current.get().is_some_and(|chart|chart.title_linked)><p class="dc-note">"Changing this title replaces its link to a cell with the text you enter."</p></Show>
                <p class="dc-note">"Leave the title empty to hide it. Undo restores the previous title and legend."</p>
            </Show>
        </details>
    }.into_any()
}
