//! The worksheet's Shapes tools (dc-zjsg.4): insert an auto shape, a text box or a line at the active cell; for the
//! selected shape, its text, fill, outline, turn, place in the drawing order, and grouping. Each is one undoable edit
//! through the host's shape command; moving, resizing and deleting are the grid's (drag, handles, Delete). A shape
//! is inserted as Excel inserts one by a click: one inch square at the cell's top left (a text box two inches by half
//! an inch, a line one inch across).

use crate::{controller::Controller, wire};
use leptos::prelude::*;

/// The kinds offered: (label, kind, preset geometry).
const GALLERY: &[(&str, &str, &str)] = &[
    ("Rectangle", "shape", "rect"),
    ("Rounded rectangle", "shape", "roundRect"),
    ("Oval", "shape", "ellipse"),
    ("Isosceles triangle", "shape", "triangle"),
    ("Right triangle", "shape", "rtTriangle"),
    ("Diamond", "shape", "diamond"),
    ("Pentagon", "shape", "pentagon"),
    ("Hexagon", "shape", "hexagon"),
    ("Right arrow", "shape", "rightArrow"),
    ("Left arrow", "shape", "leftArrow"),
    ("Up arrow", "shape", "upArrow"),
    ("Down arrow", "shape", "downArrow"),
    ("Star: 5 points", "shape", "star5"),
    ("Heart", "shape", "heart"),
    ("Smiley face", "shape", "smileyFace"),
    ("Callout", "shape", "wedgeRectCallout"),
    ("Flowchart: process", "shape", "flowChartProcess"),
    ("Flowchart: decision", "shape", "flowChartDecision"),
    ("Flowchart: terminator", "shape", "flowChartTerminator"),
    ("Text box", "textbox", "rect"),
    ("Line", "connector", "line"),
    ("Arrow", "connector", "straightConnector1"),
];

/// The insert size of a kind, in hundredths of a point.
fn insert_size(kind: &str) -> (i64, i64) {
    match kind {
        "textbox" => (14_400, 3_600),
        "connector" => (7_200, 0),
        _ => (7_200, 7_200),
    }
}

pub fn controls(controller: Controller, shapes: RwSignal<Vec<wire::DrawnShape>>, selected: RwSignal<Option<u32>>, also: RwSignal<Vec<u32>>) -> AnyView {
    let choice = RwSignal::new(0usize);
    let text = RwSignal::new(String::new());
    let fill = RwSignal::new("#4472C4".to_owned());
    let outline = RwSignal::new("#000000".to_owned());
    let weight = RwSignal::new("0.75".to_owned());
    let degrees = RwSignal::new("0".to_owned());
    let current = move || selected.get().and_then(|id| shapes.get().into_iter().find(|s| s.id == id));
    // The selected shape's own values fill the fields when the selection changes.
    Effect::new(move |_| {
        if let Some(shape) = current() {
            text.set(shape.text.as_ref().map(|t| t.paragraphs.iter().map(|p| p.text.clone()).collect::<Vec<_>>().join("\n")).unwrap_or_default());
            if let Some(f) = shape.fill.clone() {
                fill.set(f);
            }
            if let Some(line) = shape.line.as_ref() {
                outline.set(line.colour.clone());
                weight.set(format!("{}", (line.width * 0.75 * 100.0).round() / 100.0));
            }
            degrees.set(format!("{}", shape.rotation.round()));
        }
    });
    let sheet = move || controller.sheet.get_untracked().map(wire::Sheet::of);
    let revision = move || controller.summary.get_untracked().revision;
    let send = move |intent: wire::Intent| controller.dispatch(intent, |_| {});
    let insert = move |_| {
        let Some(sheet_id) = controller.sheet.get_untracked() else { return };
        let (_, kind, geometry) = GALLERY[choice.get_untracked().min(GALLERY.len() - 1)];
        let (row, column) = controller.active.get_untracked();
        let Ok(cell) = calc_value::CellKey::new(sheet_id, row, column) else { return };
        let (width, height) = insert_size(kind);
        send(wire::Intent::ShapeAdd {
            sheet: wire::Sheet::of(sheet_id),
            revision: revision(),
            at: wire::Key::of(cell),
            kind: kind.to_owned(),
            geometry: geometry.to_owned(),
            width,
            height,
        });
    };
    let with = move |make: &dyn Fn(wire::Sheet, u64, u32) -> wire::Intent| {
        if let (Some(sheet), Some(id)) = (sheet(), selected.get_untracked()) {
            send(make(sheet, revision(), id));
        }
    };
    let set_fill = move |value: String| with(&move |sheet, revision, id| wire::Intent::ShapeFill { sheet, revision, id, fill: value.clone() });
    let set_line = move |value: String| {
        let points: Option<f64> = weight.get_untracked().trim().parse().ok();
        with(&move |sheet, revision, id| wire::Intent::ShapeLine {
            sheet,
            revision,
            id,
            colour: value.clone(),
            weight: points.map(|p| (p * 100.0).round() as i64),
        })
    };
    let turn = move |to: i64| with(&move |sheet, revision, id| wire::Intent::ShapeRotate { sheet, revision, id, degrees: to });
    let order = move |step: &'static str| with(&move |sheet, revision, id| wire::Intent::ShapeOrder { sheet, revision, id, order: step.to_owned() });
    let selected_view = move || {
        let shape = current()?;
        let group = shape.kind == "group";
        let line = shape.kind == "connector";
        let holds_text = matches!(shape.kind.as_str(), "shape" | "textbox") && shape.picture.is_none();
        let others = also.get().len();
        Some(view! {
            <fieldset data-dnacalc="shape-selected">
                <legend>{format!("Selected: {}", shape.name)}</legend>
                {holds_text.then(|| view! {
                    <label>"Text "
                        <textarea data-dnacalc="shape-text" rows="2" prop:value=move || text.get() on:input=move |ev| text.set(event_target_value(&ev))></textarea>
                    </label>
                    <button type="button" data-dnacalc="shape-text-apply" on:click=move |_| {
                        let value = text.get_untracked();
                        with(&move |sheet, revision, id| wire::Intent::ShapeText { sheet, revision, id, text: value.clone() });
                    }>"Set text"</button>
                })}
                {(!line && !group).then(|| view! {
                    <div class="dc-command-row">
                        <label>"Fill "
                            <input type="color" data-dnacalc="shape-fill" prop:value=move || fill.get() on:input=move |ev| fill.set(event_target_value(&ev))/>
                        </label>
                        <button type="button" data-dnacalc="shape-fill-apply" on:click=move |_| set_fill(fill.get_untracked())>"Set fill"</button>
                        <button type="button" data-dnacalc="shape-fill-none" on:click=move |_| set_fill("none".into())>"No fill"</button>
                        <button type="button" data-dnacalc="shape-fill-auto" on:click=move |_| set_fill(String::new())>"Automatic"</button>
                    </div>
                })}
                {(!group).then(|| view! {
                    <div class="dc-command-row">
                        <label>"Outline "
                            <input type="color" data-dnacalc="shape-line" prop:value=move || outline.get() on:input=move |ev| outline.set(event_target_value(&ev))/>
                        </label>
                        <label>"Weight (pt) "
                            <input data-dnacalc="shape-line-weight" inputmode="decimal" size="4" prop:value=move || weight.get() on:input=move |ev| weight.set(event_target_value(&ev))/>
                        </label>
                        <button type="button" data-dnacalc="shape-line-apply" on:click=move |_| set_line(outline.get_untracked())>"Set outline"</button>
                        <button type="button" data-dnacalc="shape-line-none" on:click=move |_| set_line("none".into())>"No outline"</button>
                    </div>
                })}
                <div class="dc-command-row">
                    <label>"Turn (degrees) "
                        <input data-dnacalc="shape-rotation" inputmode="numeric" size="4" prop:value=move || degrees.get() on:input=move |ev| degrees.set(event_target_value(&ev))/>
                    </label>
                    <button type="button" data-dnacalc="shape-rotate-apply" on:click=move |_| {
                        if let Ok(to) = degrees.get_untracked().trim().parse::<f64>() {
                            turn((to.round() as i64).rem_euclid(360));
                        }
                    }>"Turn"</button>
                    <button type="button" data-dnacalc="shape-rotate-left" on:click=move |_| turn(((shape.rotation.round() as i64) - 90).rem_euclid(360))>"Left 90°"</button>
                    <button type="button" data-dnacalc="shape-rotate-right" on:click=move |_| turn(((shape.rotation.round() as i64) + 90).rem_euclid(360))>"Right 90°"</button>
                </div>
                <div class="dc-command-row">
                    <button type="button" data-dnacalc="shape-front" on:click=move |_| order("front")>"Bring to front"</button>
                    <button type="button" data-dnacalc="shape-forward" on:click=move |_| order("forward")>"Bring forward"</button>
                    <button type="button" data-dnacalc="shape-backward" on:click=move |_| order("backward")>"Send backward"</button>
                    <button type="button" data-dnacalc="shape-back" on:click=move |_| order("back")>"Send to back"</button>
                </div>
                <div class="dc-command-row">
                    <button type="button" data-dnacalc="shape-group" disabled=others == 0 on:click=move |_| {
                        let (Some(sheet), Some(id)) = (sheet(), selected.get_untracked()) else { return };
                        let mut ids = vec![id];
                        ids.extend(also.get_untracked().into_iter().filter(|o| *o != id));
                        also.set(Vec::new());
                        send(wire::Intent::ShapeGroup { sheet, revision: revision(), ids });
                    }>{if others == 0 { "Group (Shift+click more shapes)".to_owned() } else { format!("Group {} shapes", others + 1) }}</button>
                    {group.then(|| view! {
                        <button type="button" data-dnacalc="shape-ungroup" on:click=move |_| {
                            with(&move |sheet, revision, id| wire::Intent::ShapeUngroup { sheet, revision, id });
                            selected.set(None);
                        }>"Ungroup"</button>
                    })}
                </div>
            </fieldset>
        })
    };
    view! {
        // A press in the panel keeps the shape selected (the grid lets a shape go on a press elsewhere).
        <details class="dc-clipboard-tools" name="worksheet-tools" data-dnacalc="shape-tools" on:mousedown=move |ev| ev.stop_propagation()>
            <summary data-dnacalc="shape-tools-toggle">"Shapes"</summary>
            <fieldset>
                <legend>"Insert at the active cell"</legend>
                <select data-dnacalc="shape-gallery" on:change=move |ev| choice.set(event_target_value(&ev).parse().unwrap_or(0))>
                    {GALLERY
                        .iter()
                        .enumerate()
                        .map(|(i, (label, _, _))| view! { <option value=i.to_string()>{*label}</option> })
                        .collect_view()}
                </select>
                <button type="button" data-dnacalc="shape-insert" on:click=insert>"Insert"</button>
                <label class="dc-command-button">"Picture… "
                    <input type="file" data-dnacalc="shape-picture-file" accept=".png,.jpg,.jpeg,.gif,.bmp,.emf,.wmf,image/png,image/jpeg,image/gif,image/bmp"
                        on:change=move |ev| {
                            use wasm_bindgen::JsCast;
                            if let Some(input) = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok()) {
                                insert_picture(controller, input);
                            }
                        }/>
                </label>
            </fieldset>
            {selected_view}
            <p class="dc-note">"Click a shape to select it; Shift+click adds shapes to group. Drag a shape to move it, its handles to resize it; Delete removes it."</p>
        </details>
    }
    .into_any()
}

/// Reads the image file an input chose and inserts it as a picture at the active cell, at its own size.
fn insert_picture(controller: Controller, input: web_sys::HtmlInputElement) {
    dna_formula_ui::web::read_chosen(input, None, move |result| match result {
        Ok((name, bytes)) => {
            let Some(sheet) = controller.sheet.get_untracked() else { return };
            let (row, column) = controller.active.get_untracked();
            let Ok(cell) = calc_value::CellKey::new(sheet, row, column) else { return };
            controller.dispatch_bytes(
                wire::Intent::ShapePicture {
                    sheet: wire::Sheet::of(sheet),
                    revision: controller.summary.get_untracked().revision,
                    at: wire::Key::of(cell),
                    name,
                },
                bytes,
                |_| {},
            );
        }
        Err(error) => controller.status.set(crate::controller::Status { text: error, refused: true }),
    });
}

