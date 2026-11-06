//! Revision-bound chart properties, submitted through the workbook command route.
use crate::{controller::Controller, wire};
use leptos::prelude::*;
use wasm_bindgen::JsCast;

pub fn controls(controller: Controller) -> AnyView {
    let create_type=controller.tool_field(|s|&s.chart.create_type,|s|&mut s.chart.create_type);
    let create_rows=controller.tool_field(|s|&s.chart.create_rows,|s|&mut s.chart.create_rows);
    let create_headers=controller.tool_field(|s|&s.chart.create_headers,|s|&mut s.chart.create_headers);
    let create_categories=controller.tool_field(|s|&s.chart.create_categories,|s|&mut s.chart.create_categories);
    let open = RwSignal::new(false);
    let charts = RwSignal::new(Vec::<wire::Chart>::new());
    let chosen = controller.tool_field(|s| &s.chart.part, |s| &mut s.chart.part);
    let title = controller.tool_field(|s| &s.chart.title, |s| &mut s.chart.title);
    let legend_position=controller.tool_field(|s|&s.chart.legend_position,|s|&mut s.chart.legend_position);
    let legend = controller.tool_field(|s| &s.chart.legend, |s| &mut s.chart.legend);
    let plot = controller.tool_field(|s| &s.chart.plot, |s| &mut s.chart.plot);
    let series = controller.tool_field(|s| &s.chart.series, |s| &mut s.chart.series);
    let series_name = controller.tool_field(|s| &s.chart.series_name, |s| &mut s.chart.series_name);
    let categories = controller.tool_field(|s| &s.chart.categories, |s| &mut s.chart.categories);
    let values = controller.tool_field(|s| &s.chart.values, |s| &mut s.chart.values);
    let chart_type = controller.tool_field(|s| &s.chart.chart_type, |s| &mut s.chart.chart_type);
    let chart_grouping = controller.tool_field(|s| &s.chart.chart_grouping, |s| &mut s.chart.chart_grouping);
    let axis = controller.tool_field(|s| &s.chart.axis, |s| &mut s.chart.axis);
    let axis_minimum = controller.tool_field(|s| &s.chart.axis_minimum, |s| &mut s.chart.axis_minimum);
    let axis_maximum = controller.tool_field(|s| &s.chart.axis_maximum, |s| &mut s.chart.axis_maximum);
    let axis_unit = controller.tool_field(|s| &s.chart.axis_unit, |s| &mut s.chart.axis_unit);
    let axis_log=controller.tool_field(|s|&s.chart.axis_log,|s|&mut s.chart.axis_log);
    let axis_base=controller.tool_field(|s|&s.chart.axis_base,|s|&mut s.chart.axis_base);
    let axis_reverse = controller.tool_field(|s| &s.chart.axis_reverse, |s| &mut s.chart.axis_reverse);
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
    let create=move |_| {
        let areas=controller.areas.get_untracked();
        let [area]=areas.as_slice() else{controller.say("Select one rectangular chart data range",true);return;};
        let area=*area;let generation=controller.generation.get_untracked();let revision=controller.summary.get_untracked().revision;
        let kind=create_type.get_untracked();let kind=if kind.is_empty(){"column".into()}else{kind};
        let rows=create_rows.get_untracked();let headers=create_headers.get_untracked();let categories=create_categories.get_untracked();
        controller.after_input(move || {
            if controller.generation.get_untracked()!=generation || controller.summary.get_untracked().revision!=revision || controller.sheet.get_untracked()!=Some(area.sheet()) || controller.editing.get_untracked().is_some(){controller.say("The workbook changed; select the chart data again",true);return;}
            controller.dispatch(wire::Intent::ChartCreate{revision,area:wire::Area::of(area),kind,rows,headers,categories},move |outcome| {
                if outcome.accepted {
                    controller.say("Chart created",false);
                    let accepted_revision=outcome.summary.revision;let active=controller.active_key_untracked();
                    controller.ask(wire::Request::Charts(wire::Sheet::of(area.sheet())),move |reply| {
                        if controller.generation.get_untracked()!=generation || controller.summary.get_untracked().revision!=accepted_revision || controller.sheet.get_untracked()!=Some(area.sheet()) || controller.active_key_untracked()!=active || controller.editing.get_untracked().is_some(){return;}
                        if let wire::Reply::Charts(values)=reply {if let Some(chart)=values.last(){
                            chosen.set(chart.part.clone());
                            if let wire::ChartPlace::Sized{from,..}=chart.place {if let Some(at)=from.0.cell(){controller.go_to(at);}}
                        }}
                    });
                }
            });
        });
    };
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
        legend_position.set(chart.as_ref().and_then(|c|c.legend.clone()).unwrap_or("r".into()));
    });
    let current_part=Memo::new(move |_|current.get().map(|c|c.part));
    Effect::new(move |_| {
        open.track();current_part.track();plot.set("0".into());
    });
    let current_plot=Memo::new(move |_| current.get().and_then(|c|c.plots.get(plot.get().parse::<usize>().unwrap_or(0)).cloned()));
    Effect::new(move |_| {
        open.track();current_part.track();let p=current_plot.get();
        chart_type.set(p.as_ref().map(edit_type).unwrap_or_default().into());
        chart_grouping.set(p.as_ref().map(edit_grouping).unwrap_or("normal").into());
    });
    let apply_type=move |_| {
        let Some((generation,revision,sheet))=ready.get_untracked()else{return;};
        let Some(chart)=current.get_untracked()else{return;};let Some(old)=current_plot.get_untracked()else{return;};
        let kind=chart_type.get_untracked();let grouping=chart_grouping.get_untracked();
        if kind==edit_type(&old) && grouping==edit_grouping(&old){return;}
        if grouping=="percent"{controller.say("Choose Normal or Stacked grouping before applying this type",true);return;}
        let plot=plot.get_untracked().parse::<u32>().unwrap_or(0);
        controller.after_input(move || {
            if controller.generation.get_untracked()!=generation || controller.summary.get_untracked().revision!=revision || controller.sheet.get_untracked()!=Some(sheet) || controller.editing.get_untracked().is_some(){controller.say("The workbook changed; reopen the chart controls",true);return;}
            controller.dispatch(wire::Intent::ChartType{sheet:wire::Sheet::of(sheet),revision,part:chart.part,plot,kind,stacked:grouping=="stacked"},move |outcome|{if outcome.accepted{controller.say("Chart type updated",false);}});
        });
    };
    Effect::new(move |_| {
        let p=current_plot.get();
        let chosen=series.get_untracked().parse::<u32>().ok();
        if !p.as_ref().is_some_and(|p|p.series.iter().any(|s|Some(s.index)==chosen)) {
            series.set(p.and_then(|p|p.series.first().map(|s|s.index.to_string())).unwrap_or_default());
        }
    });
    let current_series=Memo::new(move |_| {
        let index=series.get().parse::<u32>().ok()?;
        current_plot.get()?.series.into_iter().find(|s|s.index==index)
    });
    Effect::new(move |_| {
        open.track();current_part.track();let s=current_series.get();
        series_name.set(s.as_ref().map(|s|s.name.clone()).unwrap_or_default());
        categories.set(s.as_ref().and_then(|s|s.categories.clone()).unwrap_or_default());
        values.set(s.as_ref().and_then(|s|s.values.clone()).unwrap_or_default());
    });
    let edit_series=move |remove:bool| {
        let Some((generation,revision,sheet))=ready.get_untracked() else{return;};
        let Some(chart)=current.get_untracked() else{return;};
        let plot=plot.get_untracked().parse::<u32>().unwrap_or(0);
        let old=current_series.get_untracked();
        let name=series_name.get_untracked();let cats=categories.get_untracked();let vals=values.get_untracked();
        let name=(old.as_ref().is_none_or(|s|s.name!=name)).then_some(name);
        let categories=(old.as_ref().is_none_or(|s|s.categories.as_deref().unwrap_or_default()!=cats)).then_some(cats);
        let values=(old.as_ref().is_none_or(|s|s.values.as_deref().unwrap_or_default()!=vals)).then_some(vals);
        if !remove && old.is_some() && name.is_none() && categories.is_none() && values.is_none(){return;}
        controller.after_input(move || {
            if controller.generation.get_untracked()!=generation || controller.summary.get_untracked().revision!=revision || controller.sheet.get_untracked()!=Some(sheet) || controller.editing.get_untracked().is_some() {
                controller.say("The workbook changed; reopen the chart controls",true);return;
            }
            controller.dispatch(wire::Intent::ChartSeries{sheet:wire::Sheet::of(sheet),revision,part:chart.part,plot,series:old.map(|s|s.index),name,categories,values,remove},move |outcome|{
                if outcome.accepted{controller.say(if remove{"Series removed"}else{"Chart series updated"},false);}
            });
        });
    };
    Effect::new(move |_| {
        let chart=current.get();let chosen=axis.get_untracked().parse::<u32>().ok();
        if !chart.as_ref().is_some_and(|c|c.axes.iter().any(|a|Some(a.id)==chosen)) {
            axis.set(chart.and_then(|c|c.axes.first().map(|a|a.id.to_string())).unwrap_or_default());
        }
    });
    let current_axis=Memo::new(move |_|{let id=axis.get().parse::<u32>().ok()?;current.get()?.axes.into_iter().find(|a|a.id==id)});
    Effect::new(move |_| {
        open.track();current_part.track();let a=current_axis.get();
        axis_minimum.set(a.as_ref().and_then(|a|a.minimum.clone()).unwrap_or_default());
        axis_maximum.set(a.as_ref().and_then(|a|a.maximum.clone()).unwrap_or_default());
        axis_unit.set(a.as_ref().and_then(|a|a.major_unit.clone()).unwrap_or_default());
        axis_reverse.set(a.as_ref().is_some_and(|a|a.reversed));
        axis_log.set(a.as_ref().is_some_and(|a|a.logarithmic));
        axis_base.set(a.as_ref().and_then(|a|a.log_base.clone()).unwrap_or("10".into()));
    });
    let apply_log=move |_| {
        let Some((generation,revision,sheet))=ready.get_untracked()else{return;};
        let (Some(chart),Some(axis))=(current.get_untracked(),current_axis.get_untracked())else{return;};
        let base=axis_log.get_untracked().then(||axis_base.get_untracked());
        controller.after_input(move || {
            if controller.generation.get_untracked()!=generation||controller.summary.get_untracked().revision!=revision||controller.sheet.get_untracked()!=Some(sheet)||controller.editing.get_untracked().is_some(){controller.say("The workbook changed; reopen the chart controls",true);return;}
            controller.dispatch(wire::Intent::ChartLogAxis{sheet:wire::Sheet::of(sheet),revision,part:chart.part,axis:axis.id,base},move |outcome|{if outcome.accepted{controller.say("Chart scale updated",false);}});
        });
    };
    let apply_axis=move |_| {
        let Some((generation,revision,sheet))=ready.get_untracked() else{return;};
        let (Some(chart),Some(axis))=(current.get_untracked(),current_axis.get_untracked()) else{return;};
        let nonempty=|s:String|(!s.trim().is_empty()).then_some(s);
        let minimum=nonempty(axis_minimum.get_untracked());let maximum=nonempty(axis_maximum.get_untracked());let major_unit=nonempty(axis_unit.get_untracked());let reversed=axis_reverse.get_untracked();
        if minimum==axis.minimum && maximum==axis.maximum && major_unit==axis.major_unit && reversed==axis.reversed{return;}
        controller.after_input(move ||{
            if controller.generation.get_untracked()!=generation || controller.summary.get_untracked().revision!=revision || controller.sheet.get_untracked()!=Some(sheet) || controller.editing.get_untracked().is_some() {
                controller.say("The workbook changed; reopen the chart controls",true);return;
            }
            controller.dispatch(wire::Intent::ChartAxis{sheet:wire::Sheet::of(sheet),revision,part:chart.part,axis:axis.id,minimum,maximum,major_unit,reversed},move |outcome|{if outcome.accepted{controller.say("Chart axis updated",false);}});
        });
    };
    let place_legend=move |_| {
        let Some((generation,revision,sheet))=ready.get_untracked()else{return;};let Some(chart)=current.get_untracked()else{return;};let position=legend_position.get_untracked();
        if chart.legend.as_deref()==Some(position.as_str()){return;}
        controller.after_input(move || {
            if controller.generation.get_untracked()!=generation||controller.summary.get_untracked().revision!=revision||controller.sheet.get_untracked()!=Some(sheet)||controller.editing.get_untracked().is_some(){controller.say("The workbook changed; reopen the chart controls",true);return;}
            controller.dispatch(wire::Intent::ChartLegend{sheet:wire::Sheet::of(sheet),revision,part:chart.part,position},move |outcome|{if outcome.accepted{controller.say("Chart legend updated",false);}});
        });
    };
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
    let reveal_chart=move |part:String| {
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
                    if at.is_none(){controller.say("Revealing an absolute chart frame is not modelled (dc-971p)",true);}
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
            <fieldset class="dc-chart-series"><legend>"Create from selection"</legend>
                <p class="dc-note" data-dnacalc="chart-create-range">{move ||controller.areas.get().first().map(|a|controller.area_label(*a)).unwrap_or_else(||"Select a rectangular data range".into())}</p>
                <div class="dc-command-row"><label>"Type "<select data-dnacalc="chart-create-type" prop:value=move ||{let v=create_type.get();if v.is_empty(){"column".into()}else{v}} on:change=move |ev|create_type.set(event_target_value(&ev))>
                    <option value="column">"Column"</option><option value="bar">"Bar"</option><option value="line">"Line"</option><option value="area">"Area"</option><option value="pie">"Pie"</option><option value="doughnut">"Doughnut"</option><option value="scatter">"Scatter (XY)"</option><option value="combo">"Column + line (secondary axis)"</option>
                </select></label></div>
                <div class="dc-command-row"><label><input data-dnacalc="chart-create-rows" type="checkbox" prop:checked=move ||create_rows.get() on:change=move |ev|create_rows.set(event_target_checked(&ev))/>"Series in rows"</label>
                    <label><input data-dnacalc="chart-create-headers" type="checkbox" prop:checked=move ||create_headers.get() on:change=move |ev|create_headers.set(event_target_checked(&ev))/>"First row contains series names (first column for rows)"</label>
                    <label><input data-dnacalc="chart-create-categories" type="checkbox" prop:checked=move ||create_categories.get() on:change=move |ev|create_categories.set(event_target_checked(&ev))/>"First column contains categories / X values (first row for rows)"</label></div>
                <button type="button" data-dnacalc="chart-create" disabled=move ||controller.assist.readonly.get() || controller.editing.get().is_some() || controller.areas.get().len()!=1 on:click=create>"Create chart"</button>
            <p class="dc-note">"Column + line uses the last values series as a line on the right axis; the other series are columns on the left axis."</p>
            </fieldset>
            <Show when=move ||charts.get().is_empty()><p class="dc-note">"This worksheet has no readable charts. Select data above to create one."</p></Show>
            <Show when=move ||!charts.get().is_empty()>
                <div class="dc-command-row"><label>"Chart "<select data-dnacalc="chart-choice" prop:value=move ||chosen.get() on:change=move |ev| {
                    let part=event_target_value(&ev);
                    chosen.set(part.clone());
                    reveal_chart(part);
                }>
                    {move ||charts.get().into_iter().map(|chart|view! {<option value=chart.part>{chart.name}</option>}).collect_view()}
                </select></label><button type="button" data-dnacalc="chart-show" disabled=move ||ready.get().is_none() on:click=move |_|reveal_chart(chosen.get_untracked())>"Show chart"</button></div>
                <div class="dc-command-row"><label>"Title "<input data-dnacalc="chart-edit-title" maxlength="32767" prop:value=move ||title.get() on:input=move |ev|title.set(event_target_value(&ev))/></label>
                    <label><input data-dnacalc="chart-edit-legend" type="checkbox" prop:checked=move ||legend.get() on:change=move |ev|legend.set(event_target_checked(&ev))/>"Show legend"</label>
                    <button type="button" data-dnacalc="chart-edit-apply" disabled=move ||ready.get().is_none() || controller.assist.readonly.get() || controller.editing.get().is_some() on:click=apply>"Apply"</button>
                </div>
                <div class="dc-command-row"><label>"Legend position "<select data-dnacalc="chart-legend-position" prop:value=move ||legend_position.get() on:change=move |ev|legend_position.set(event_target_value(&ev))>
                    <option value="r">"Right"</option><option value="l">"Left"</option><option value="t">"Top"</option><option value="b">"Bottom"</option><option value="tr">"Top right"</option>
                </select></label><button type="button" data-dnacalc="chart-legend-apply" disabled=move ||ready.get().is_none()||!legend.get()||controller.assist.readonly.get()||controller.editing.get().is_some() on:click=place_legend>"Place legend"</button></div>
                <Show when=move ||current.get().is_some_and(|chart|chart.title_linked)><p class="dc-note">"Changing this title replaces its link to a cell with the text you enter."</p></Show>
                <p class="dc-note">"Leave the title empty to hide it. Undo restores the previous chart settings."</p>
                <Show when=move ||current_plot.get().is_some_and(|p|edit_type(&p)!="unsupported")>
                    <fieldset class="dc-chart-series"><legend>"Chart type"</legend>
                        <div class="dc-command-row"><label>"Type "<select data-dnacalc="chart-type" prop:value=move ||chart_type.get() on:change=move |ev|chart_type.set(event_target_value(&ev))>
                            {move ||current_plot.get().map(|p|{
                                let options=if matches!(p.kind.as_str(),"barChart"|"lineChart"|"areaChart"){vec![("column","Column"),("bar","Bar"),("line","Line"),("area","Area")]}else{vec![("pie","Pie"),("doughnut","Doughnut")]};
                                options.into_iter().map(|(value,label)|view!{<option value=value>{label}</option>}).collect_view()
                            })}
                        </select></label>
                        <Show when=move ||current_plot.get().is_some_and(|p|matches!(p.kind.as_str(),"barChart"|"lineChart"|"areaChart"))>
                            <label>"Grouping "<select data-dnacalc="chart-grouping" prop:value=move ||chart_grouping.get() on:change=move |ev|chart_grouping.set(event_target_value(&ev))>
                                <option value="normal">"Normal"</option><option value="stacked">"Stacked"</option>
                                <Show when=move ||current_plot.get().is_some_and(|p|p.grouping.as_deref()==Some("percentStacked"))><option value="percent" disabled>"Percent stacked"</option></Show>
                            </select></label>
                        </Show>
                        <button type="button" data-dnacalc="chart-type-apply" disabled=move ||ready.get().is_none() || chart_grouping.get()=="percent" || controller.assist.readonly.get() || controller.editing.get().is_some() on:click=apply_type>"Apply type"</button></div>
                        <p class="dc-note">"Type changes use defaults for type-specific options. Series data and shared formatting are kept."</p>
                    </fieldset>
                </Show>
                <fieldset class="dc-chart-series"><legend>"Series data"</legend>
                    <div class="dc-command-row">
                        <label>"Plot "<select data-dnacalc="chart-plot" prop:value=move ||plot.get() on:change=move |ev|plot.set(event_target_value(&ev))>
                            {move ||current.get().map(|c|c.plots.into_iter().enumerate().map(|(i,p)|view!{<option value=i.to_string()>{format!("{}: {}",i+1,plot_name(&p.kind))}</option>}).collect_view())}
                        </select></label>
                        <label>"Series "<select data-dnacalc="chart-series" prop:value=move ||series.get() on:change=move |ev|series.set(event_target_value(&ev))>
                            {move ||current_plot.get().map(|p|p.series.into_iter().map(|s|view!{<option value=s.index.to_string()>{s.label}</option>}).collect_view())}
                            <option value="new" disabled=move ||current_plot.get().is_some_and(|p|matches!(p.kind.as_str(),"pieChart"|"doughnutChart"))>"New series…"</option>
                        </select></label>
                    </div>
                    <div class="dc-command-row"><label>"Name "<input data-dnacalc="chart-series-name" prop:value=move ||series_name.get() on:input=move |ev|series_name.set(event_target_value(&ev))/></label></div>
                    <div class="dc-command-row"><label>"Categories / X range "<input data-dnacalc="chart-series-categories" placeholder="Sheet1!$A$2:$A$7" prop:value=move ||categories.get() on:input=move |ev|categories.set(event_target_value(&ev))/></label></div>
                    <div class="dc-command-row"><label>"Values / Y range "<input data-dnacalc="chart-series-values" placeholder="Sheet1!$B$2:$B$7" prop:value=move ||values.get() on:input=move |ev|values.set(event_target_value(&ev))/></label></div>
                    <div class="dc-command-row">
                        <button type="button" data-dnacalc="chart-series-apply" disabled=move ||ready.get().is_none() || controller.assist.readonly.get() || controller.editing.get().is_some() on:click=move |_|edit_series(false)>{move ||if current_series.get().is_some(){"Update series"}else{"Add series"}}</button>
                        <button type="button" data-dnacalc="chart-series-remove" disabled=move ||ready.get().is_none() || current_series.get().is_none() || current_plot.get().is_none_or(|p|p.series.len()<2) || controller.assist.readonly.get() || controller.editing.get().is_some() on:click=move |_|edit_series(true)>"Remove series"</button>
                    </div>
                    <p class="dc-note">"Use a one-row or one-column worksheet range. Names can be text or =Sheet1!$B$1. Empty categories use point numbers; unchanged literal data stays as stored."</p>
                </fieldset>
                <Show when=move ||current.get().is_some_and(|c|!c.axes.is_empty())>
                    <fieldset class="dc-chart-series"><legend>"Value axes"</legend>
                        <div class="dc-command-row"><label>"Axis "<select data-dnacalc="chart-axis" prop:value=move ||axis.get() on:change=move |ev|axis.set(event_target_value(&ev))>
                            {move ||current.get().map(|c|c.axes.into_iter().map(|a|view!{<option value=a.id.to_string()>{axis_name(&a.position).to_owned()}</option>}).collect_view())}
                        </select></label></div>
                        <div class="dc-command-row">
                            <label>"Minimum "<input data-dnacalc="chart-axis-minimum" inputmode="decimal" maxlength="128" placeholder="Automatic" prop:value=move ||axis_minimum.get() on:input=move |ev|axis_minimum.set(event_target_value(&ev))/></label>
                            <label>"Maximum "<input data-dnacalc="chart-axis-maximum" inputmode="decimal" maxlength="128" placeholder="Automatic" prop:value=move ||axis_maximum.get() on:input=move |ev|axis_maximum.set(event_target_value(&ev))/></label>
                            <label>"Major unit "<input data-dnacalc="chart-axis-unit" inputmode="decimal" maxlength="128" placeholder="Automatic" prop:value=move ||axis_unit.get() on:input=move |ev|axis_unit.set(event_target_value(&ev))/></label>
                        </div>
                        <div class="dc-command-row">
                            <label><input type="checkbox" data-dnacalc="chart-axis-reverse" prop:checked=move ||axis_reverse.get() on:change=move |ev|axis_reverse.set(event_target_checked(&ev))/>"Reverse direction"</label>
                            <button type="button" data-dnacalc="chart-axis-apply" disabled=move ||ready.get().is_none() || current_axis.get().is_none() || controller.assist.readonly.get() || controller.editing.get().is_some() on:click=apply_axis>"Apply axis"</button>
                        </div>
                        <p class="dc-note">"Leave bounds and units empty for automatic scaling. Undo restores the previous axis settings."</p>
                        <div class="dc-command-row">
                            <label><input type="checkbox" data-dnacalc="chart-axis-log" prop:checked=move ||axis_log.get() on:change=move |ev|axis_log.set(event_target_checked(&ev))/>"Logarithmic scale"</label>
                            <label>"Base "<input data-dnacalc="chart-axis-base" inputmode="decimal" maxlength="128" prop:value=move ||axis_base.get() on:input=move |ev|axis_base.set(event_target_value(&ev))/></label>
                            <button type="button" data-dnacalc="chart-axis-scale-apply" disabled=move ||ready.get().is_none()||current_axis.get().is_none()||controller.assist.readonly.get()||controller.editing.get().is_some() on:click=apply_log>"Apply scale"</button>
                        </div>
                        <p class="dc-note">"Logarithmic axes use positive values and bounds. Base 2–1,000; major units apply to linear scales. Stacked logarithmic plots are not drawn here."</p>
                    </fieldset>
                </Show>

            </Show>
        </details>
    }.into_any()
}

fn plot_name(kind:&str)->&str{match kind{"barChart"=>"Column / bar","lineChart"=>"Line","areaChart"=>"Area","pieChart"=>"Pie","doughnutChart"=>"Doughnut","scatterChart"=>"Scatter",_=>"Other plot"}}
fn axis_name(position:&str)->&str{match position{"b"=>"Bottom (X)","t"=>"Top (X)","l"=>"Left (Y)","r"=>"Right (Y)",_=>"Value axis"}}

fn edit_type(plot:&wire::ChartPlotSource)->&'static str{match plot.kind.as_str(){"barChart" if plot.bar_direction.as_deref()==Some("bar")=>"bar","barChart"=>"column","lineChart"=>"line","areaChart"=>"area","pieChart"=>"pie","doughnutChart"=>"doughnut",_=>"unsupported"}}
fn edit_grouping(plot:&wire::ChartPlotSource)->&'static str{match plot.grouping.as_deref(){Some("percentStacked")=>"percent",Some("stacked")=>"stacked",_=>"normal"}}
