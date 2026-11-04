//! Table controls submit the same revision-bound workbook commands as the grid.
use crate::{
    controller::Controller,
    wire::{self, Intent},
};
use leptos::prelude::*;
fn current(controller: Controller) -> Option<wire::TableView> {
    controller.table_view()
}
fn submit(controller: Controller, change: wire::TableChange) {
    let generation = controller.generation.get_untracked();
    let sheet = controller.sheet.get_untracked();
    let expected = controller.summary.get_untracked().revision;
    let active = controller.active_key_untracked();
    let retained = match &change {
        wire::TableChange::Resize { area, .. } | wire::TableChange::Create { area, .. } => {
            area.area()
        }
        wire::TableChange::Options {
            headers, totals, ..
        } => current(controller).and_then(|t| {
            let a = t.area.area()?;
            let top = if t.headers && !*headers {
                a.first().row().checked_add(1)?
            } else if !t.headers && *headers {
                a.first().row().checked_sub(1)?
            } else {
                a.first().row()
            };
            let bottom = if t.totals && !*totals {
                a.last().row().checked_sub(1)?
            } else if !t.totals && *totals {
                a.last().row().checked_add(1)?
            } else {
                a.last().row()
            };
            calc_value::GridArea::new(
                calc_value::CellKey::new(a.sheet(), top, a.first().column()).ok()?,
                calc_value::CellKey::new(a.sheet(), bottom, a.last().column()).ok()?,
            )
            .ok()
        }),
        _ => None,
    };
    controller.after_input(move || {
        if controller.generation.get_untracked() != generation
            || controller.sheet.get_untracked() != sheet
        {
            controller.say("The table target changed; reopen its controls", true);
            return;
        }
        let revision = controller.summary.get_untracked().revision;
        if revision != expected {
            controller.say("The workbook changed; retry the table control", true);
            return;
        }
        controller.dispatch(Intent::Table { revision, change }, move |outcome| {
            if outcome.accepted {
                if controller.generation.get_untracked() == generation
                    && controller.sheet.get_untracked() == sheet
                    && controller.active_key_untracked() == active
                {
                    if let (Some(at), Some(area)) = (active, retained) {
                        if !area.contains(at) {
                            controller.select(
                                at.row().clamp(area.first().row(), area.last().row()),
                                at.column()
                                    .clamp(area.first().column(), area.last().column()),
                            );
                        }
                    }
                }
                controller.say("Table updated", false);
            }
        });
    });
}
fn options(controller: Controller, field: &str, value: String) {
    let Some(mut t) = current(controller) else {
        return;
    };
    match field {
        "headers" => t.headers = !t.headers,
        "totals" => t.totals = !t.totals,
        "rows" => t.row_stripes = !t.row_stripes,
        "columns" => t.column_stripes = !t.column_stripes,
        "first" => t.first_column = !t.first_column,
        "last" => t.last_column = !t.last_column,
        "style" => t.style = value,
        _ => return,
    };
    submit(
        controller,
        wire::TableChange::Options {
            part: t.part,
            headers: t.headers,
            totals: t.totals,
            style: t.style,
            row_stripes: t.row_stripes,
            column_stripes: t.column_stripes,
            first_column: t.first_column,
            last_column: t.last_column,
        },
    );
}
pub fn controls(controller: Controller) -> AnyView {
    let name = controller.tool_field(|s| &s.table.name, |s| &mut s.table.name);
    let rows = controller.tool_field(|s| &s.table.rows, |s| &mut s.table.rows);
    let columns = controller.tool_field(|s| &s.table.columns, |s| &mut s.table.columns);
    let total_column =
        controller.tool_field(|s| &s.table.total_column, |s| &mut s.table.total_column);
    let function =
        controller.tool_field(|s| &s.table.total_function, |s| &mut s.table.total_function);
    let target = Memo::new(move |_| {
        (
            controller.generation.get(),
            current(controller).map(|t| t.part),
        )
    });
    Effect::new(move |_| {
        target.track();
        rows.set(String::new());
        columns.set(String::new());
        total_column.set("1".into());
    });
    let create = move || {
        let areas = controller.areas.get_untracked();
        let [area] = areas.as_slice() else {
            controller.say("Select one rectangle including a header and data", true);
            return;
        };
        submit(
            controller,
            wire::TableChange::Create {
                area: wire::Area::of(*area),
                name: name.get_untracked(),
            },
        );
    };
    let resize = move || {
        let Some(t) = current(controller) else {
            return;
        };
        let Some(area) = t.area.area() else {
            return;
        };
        let count = |s: String, default: u32| {
            if s.trim().is_empty() {
                Some(default)
            } else {
                s.parse::<u32>().ok().filter(|v| *v > 0)
            }
        };
        let (Some(r), Some(c)) = (
            count(rows.get_untracked(), area.row_count()),
            count(columns.get_untracked(), area.column_count()),
        ) else {
            controller.say("Enter positive row and column counts", true);
            return;
        };
        let last = area
            .first()
            .row()
            .checked_add(r - 1)
            .zip(area.first().column().checked_add(c - 1))
            .and_then(|(r, c)| calc_value::CellKey::new(area.sheet(), r, c).ok());
        let Some(last) = last else {
            controller.say("The resized table must fit in the worksheet", true);
            return;
        };
        let Ok(area) = calc_value::GridArea::new(area.first(), last) else {
            return;
        };
        submit(
            controller,
            wire::TableChange::Resize {
                part: t.part,
                area: wire::Area::of(area),
            },
        );
    };
    view!{
        <details class="dc-clipboard-tools" name="worksheet-tools" data-dnacalc="table-tools">
            <summary>"Excel table"</summary>
            <p class="dc-note" data-dnacalc="table-summary">{move ||current(controller).map(|t|format!("{} · {}{}",t.name,t.area.area().map(|a|controller.area_label(a)).unwrap_or_default(),if t.opaque{" · size, headers and totals are read-only"}else{""})).unwrap_or_else(||"Select a rectangle including its header and data rows.".into())}</p>
            <div class="dc-command-row"><label>"New table name "<input data-dnacalc="table-name" prop:value=move ||name.get() on:input=move |ev|name.set(event_target_value(&ev))/></label><button type="button" data-dnacalc="table-create" on:click=move |_|create()>"Create from selection"</button></div>
            <Show when=move ||current(controller).is_some()>
                <div class="dc-command-row"><label>"Rows including header and totals "<input data-dnacalc="table-rows" inputmode="numeric" prop:value=move ||rows.get() placeholder=move ||current(controller).and_then(|t|t.area.area()).map(|a|a.row_count().to_string()).unwrap_or_default() on:input=move |ev|rows.set(event_target_value(&ev))/></label><label>"Columns "<input data-dnacalc="table-columns" inputmode="numeric" prop:value=move ||columns.get() placeholder=move ||current(controller).map(|t|t.columns.len().to_string()).unwrap_or_default() on:input=move |ev|columns.set(event_target_value(&ev))/></label><button data-dnacalc="table-resize" on:click=move |_|resize()>"Resize table"</button></div>
                <div class="dc-command-row">
                    <label><input type="checkbox" data-dnacalc="table-headers" prop:checked=move ||current(controller).is_some_and(|t|t.headers) on:change=move |_|options(controller,"headers",String::new())/>"Header row"</label>
                    <label><input type="checkbox" data-dnacalc="table-totals" prop:checked=move ||current(controller).is_some_and(|t|t.totals) on:change=move |_|options(controller,"totals",String::new())/>"Total row"</label>
                    <label><input type="checkbox" data-dnacalc="table-bands" prop:checked=move ||current(controller).is_some_and(|t|t.row_stripes) on:change=move |_|options(controller,"rows",String::new())/>"Banded rows"</label>
                    <label><input type="checkbox" data-dnacalc="table-column-bands" prop:checked=move ||current(controller).is_some_and(|t|t.column_stripes) on:change=move |_|options(controller,"columns",String::new())/>"Banded columns"</label>
                    <label><input type="checkbox" data-dnacalc="table-first" prop:checked=move ||current(controller).is_some_and(|t|t.first_column) on:change=move |_|options(controller,"first",String::new())/>"First column"</label>
                    <label><input type="checkbox" data-dnacalc="table-last" prop:checked=move ||current(controller).is_some_and(|t|t.last_column) on:change=move |_|options(controller,"last",String::new())/>"Last column"</label>
                    <label>"Style "<select data-dnacalc="table-style" prop:value=move ||current(controller).map(|t|t.style).unwrap_or_default() on:change=move |ev|options(controller,"style",event_target_value(&ev))><option value="">"None"</option><option value="TableStyleMedium2">"Blue, medium 2"</option>{move ||current(controller).filter(|t|!t.style.is_empty()&&t.style!="TableStyleMedium2").map(|t|view!{<option value=t.style.clone()>{format!("{} (preserved)",t.style)}</option>})}</select></label>
                </div>
                <Show when=move ||current(controller).is_some_and(|t|t.totals)><div class="dc-command-row"><label>"Total column within table "<input data-dnacalc="table-total-column" inputmode="numeric" prop:value=move ||total_column.get() on:input=move |ev|total_column.set(event_target_value(&ev))/></label><label>"Function "<select data-dnacalc="table-total-function" prop:value=move ||function.get() on:change=move |ev|function.set(event_target_value(&ev))><option value="sum">"Sum"</option><option value="average">"Average"</option><option value="count">"Count"</option><option value="countNums">"Count numbers"</option><option value="min">"Minimum"</option><option value="max">"Maximum"</option><option value="stdDev">"Standard deviation"</option><option value="var">"Variance"</option><option value="">"Clear total"</option></select></label><button data-dnacalc="table-total-apply" on:click=move |_|{let Some(t)=current(controller)else{return;};let Some(a)=t.area.area()else{return;};let Some(c)=total_column.get_untracked().parse::<u32>().ok().filter(|c|*c>0&&*c<=a.column_count())else{controller.say("Choose a column within the table",true);return;};submit(controller,wire::TableChange::Total{part:t.part,column:a.first().column()+c-1,function:function.get_untracked()});}>"Set total"</button></div></Show>
            </Show>
        </details>
    }.into_any()
}
