//! The DNA Calc shell: the mast with the document's name and the file commands (New, Open, Save),
//! the edit and calculation commands (Undo, Redo, Calculate, the calculation mode), the skin
//! switcher, the formula bar (the name box and the formula field DNA OneCalc shows too), the active
//! skin, the sheet tabs, and the status line with the active cell's provenance.
//!
//! Every element a person uses carries a `data-dnacalc` attribute naming its role; the browser
//! smoke test (`cargo xtask app smoke calc`) drives the page through them.
//!
//! Opening reads the chosen file's bytes in the browser and hands them to calc-host; saving
//! writes calc-host's package bytes to a file the browser downloads. No file leaves the browser.
//!
//! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnacalc-app/src/app.rs` (`CalcApp`: the document
//! line, the open and save verbs and their status, the stage registry with the sheet, notebook and
//! atlas stages) and `src/dnacalc-shell/src/shell.rs` (the mast's stage switcher, the bridge slot
//! that holds the formula editor, the strip). Left behind: the desktop file bridge, the command
//! deck, the inspector and registry rails, and the profile badge.

use crate::atlas::AtlasSkin;
use crate::canvas::CanvasSkin;
use crate::cells;
use crate::controller::{Controller, EditOrigin};
use crate::grid::GridSkin;
use crate::ledger::LedgerSkin;
use crate::notebook::NotebookSkin;
use crate::skin::{SkinContext, SkinRegistry, SkinResolution, resolve_active_skin, switch_skin};
use crate::tree::TreeSkin;

use leptos::prelude::*;
use std::sync::Arc;

/// The skins DNA Calc registers, in the switcher's order.
pub fn registry() -> SkinRegistry {
    SkinRegistry::new()
        .with_skin(Arc::new(GridSkin))
        .with_skin(Arc::new(NotebookSkin))
        .with_skin(Arc::new(AtlasSkin))
        .with_skin(Arc::new(TreeSkin))
        .with_skin(Arc::new(CanvasSkin))
        .with_skin(Arc::new(LedgerSkin))
}

/// Offers `bytes` as a file the browser downloads under `name`.
#[cfg(target_arch = "wasm32")]
fn download(name: &str, bytes: &[u8]) -> Result<(), String> {
    dna_formula_ui::web::download_bytes(
        name,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        bytes,
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn download(_name: &str, _bytes: &[u8]) -> Result<(), String> {
    Ok(())
}

/// Reads the file an input chose and opens it.
#[cfg(target_arch = "wasm32")]
fn open_chosen(
    controller: Controller,
    input: web_sys::HtmlInputElement,
    chosen: impl FnOnce(String, Vec<u8>) + 'static,
) {
    if let Some(file) = input.files().and_then(|f| f.get(0)) {
        controller.status.set(crate::controller::Status {
            text: format!("Opening {}", file.name()),
            refused: false,
        });
    }
    dna_formula_ui::web::read_chosen(input, None, move |result| match result {
        Ok((name, bytes)) => chosen(name, bytes),
        Err(error) => controller.status.set(crate::controller::Status {
            text: error,
            refused: true,
        }),
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn open_chosen(
    _controller: Controller,
    _input: web_sys::HtmlInputElement,
    _chosen: impl FnOnce(String, Vec<u8>) + 'static,
) {
}

#[derive(Clone)]
enum Replacement {
    New,
    Open(String, Vec<u8>),
}

pub(crate) fn focus(selector: &str) {
    use wasm_bindgen::JsCast;
    if let Some(element) = selector
        .split(", ")
        .find_map(|selector| document().query_selector(selector).ok().flatten())
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = element.focus();
    }
}

/// The application.
pub fn app() -> impl IntoView {
    app_with_transport(std::rc::Rc::new(crate::port::BrowserTransport))
}
/// The shell over an injected transport. Native compositions keep the worker outside the UI.
pub fn app_with_transport(
    transport: std::rc::Rc<dyn dna_calc_model::WorkbookTransport>,
) -> impl IntoView {
    let controller = Controller::with_transport(transport);
    controller.install_input_queue();
    let registry = registry();
    controller.register_skins(registry.clone());
    let active_skin = controller.active_skin;
    let assist = controller.assist;
    let file_input = NodeRef::<leptos::html::Input>::new();
    let replace_dialog = NodeRef::<leptos::html::Dialog>::new();
    let replacement = RwSignal::new(None::<Replacement>);
    let replace = move |next: Replacement| match next {
        Replacement::New => controller.new_workbook(),
        Replacement::Open(name, bytes) => controller.open(&name, bytes, |_| {}),
    };
    let request_replace = Callback::new(move |next: Replacement| {
        if controller.has_dirty_work() {
            replacement.set(Some(next));
        } else {
            replace(next);
        }
    });
    let keep_working = move || {
        replacement.set(None);
        if let Some(dialog) = replace_dialog.get_untracked() {
            dialog.close();
        }
        controller.status.set(crate::controller::Status {
            text: "Current workbook kept; replacement cancelled".to_owned(),
            refused: false,
        });
        controller.focus_pending_draft();
    };
    Effect::new(move |_| {
        let pending = replacement.get().is_some();
        if let Some(dialog) = replace_dialog.get() {
            if pending && !dialog.open() {
                let _ = dialog.show_modal();
            } else if !pending && dialog.open() {
                dialog.close();
            }
        }
    });
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        let unload = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::BeforeUnloadEvent)>::new(
            move |event: web_sys::BeforeUnloadEvent| {
                if controller.has_dirty_work() {
                    event.prevent_default();
                    event.set_return_value("");
                }
            },
        );
        let _ = window()
            .add_event_listener_with_callback("beforeunload", unload.as_ref().unchecked_ref());
        let unload = StoredValue::new_local(unload);
        on_cleanup(move || {
            unload.with_value(|unload| {
                let _ = window().remove_event_listener_with_callback(
                    "beforeunload",
                    unload.as_ref().unchecked_ref(),
                );
            });
        });
    }
    let result_details = NodeRef::<leptos::html::Details>::new();
    let result_open = RwSignal::new(false);
    Effect::new(
        move |previous: Option<(Option<calc_value::CellKey>, Option<cells::Provenance>)>| {
            let cell = controller.active_cell();
            let key = (
                controller.active_key(),
                cell.as_ref().map(|cell| cell.provenance),
            );
            if previous.as_ref() != Some(&key) {
                result_open.set(cell.as_ref().is_some_and(|cell| {
                    cell.provenance == cells::Provenance::Refused || cell.kind == "error"
                }));
            }
            key
        },
    );

    // While nothing is edited, the formula bar shows the active cell's input.
    Effect::new(move |_| {
        if controller.editing.get().is_none() {
            let text = controller
                .active_cell()
                .map(|cell| cell.input)
                .unwrap_or_default();
            controller.edit_text.set(text);
            assist.reset();
        }
    });

    let name_box = RwSignal::new(String::new());
    Effect::new(move |_| {
        let (row, col) = controller.active.get();
        name_box.set(cells::address(row, col));
    });
    let go_to = move |text: String| {
        controller.go_to_text(text, move |accepted| {
            let (row, col) = controller.active.get_untracked();
            name_box.set(cells::address(row, col));
            if accepted {
                controller.focus_active_skin();
            }
        })
    };

    let bar_key = Callback::new(move |ev| controller.editor_key(ev, assist));

    let save = move |_| {
        controller.save(move |name, bytes| {
            if let Err(error) = download(&name, &bytes) {
                controller.status.set(crate::controller::Status {
                    text: format!("The download did not start: {error}. Keep this workbook open."),
                    refused: true,
                });
            } else {
                controller.status.set(crate::controller::Status {
                    text: format!("Download started: {name}. Check your downloads; durable storage is unconfirmed."),
                    refused: false,
                });
            }
        });
    };

    let skin_buttons = {
        let registry = registry.clone();
        registry
            .skins()
            .iter()
            .enumerate()
            .map(|(index, skin)| {
                let id = skin.id().stable_id();
                let title = skin.title();
                let registry = registry.clone();
                let slot = (index + 1) as u8;
                let pressed = move || {
                    let active = active_skin.get();
                    let shown = match active.as_deref() {
                        Some(active) => active == id,
                        None => slot == 1,
                    };
                    if shown { "true" } else { "false" }
                };
                view! {
                    <button
                        type="button"
                        data-dnacalc=format!("skin-{id}")
                        disabled=move || controller.input_pending()
                        aria-pressed=pressed
                        title=format!("{title} (Alt+{slot})")
                        on:click=move |_| {
                            switch_skin(controller, &registry, slot);
                        }
                    >
                        {title}
                    </button>
                }
            })
            .collect_view()
    };

    let skin_view = {
        let registry = registry.clone();
        move || {
            match resolve_active_skin(&registry, active_skin.get().as_deref()) {
            SkinResolution::Mount(skin) => skin.mount(SkinContext { controller }),
            SkinResolution::Unavailable { active } => view! {
                <div class="dc-unavailable" data-dnacalc="skin-unavailable">
                    {format!("The skin {:?} is not available in DNA Calc.", active.unwrap_or_default())}
                </div>
            }
            .into_any(),
        }
        }
    };
    let registry_for_keys = registry.clone();
    let on_app_key = move |ev: leptos::ev::KeyboardEvent| {
        if ev.is_composing() || ev.key_code() == 229 || assist.composing.get_untracked() {
            return;
        }
        if ev.default_prevented() {
            return;
        }
        let chord = dna_calc_model::selection::KeyChord::new(
            ev.key(),
            ev.shift_key(),
            ev.ctrl_key() || ev.meta_key(),
            ev.alt_key(),
        );
        if let Some(id) = dna_calc_model::resolve_shell_key(&chord) {
            use wasm_bindgen::JsCast;
            let native_text = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                .is_some_and(|element| {
                    matches!(element.tag_name().as_str(), "INPUT" | "TEXTAREA")
                        || element
                            .closest("[contenteditable=true]")
                            .ok()
                            .flatten()
                            .is_some()
                });
            if !native_text
                || !matches!(
                    id,
                    dna_calc_model::CommandId::Undo | dna_calc_model::CommandId::Redo
                )
            {
                ev.prevent_default();
                if let Some(command) = dna_calc_model::Command::simple(id) {
                    controller.command(command);
                }
                return;
            }
        }
        let ctrl = ev.ctrl_key() || ev.meta_key();
        if ev.key() == "F6" && !ctrl && !ev.alt_key() {
            ev.prevent_default();
            let selectors = [
                "[data-dnacalc=new]",
                "[data-onecalc=formula]",
                "#dc-workspace",
                "[data-dnacalc=sheet-tab][aria-selected=true]",
            ];
            let active = document().active_element();
            let regions = [".dc-mast", selectors[1], ".dc-main", selectors[3]];
            let index = regions
                .iter()
                .position(|selector| {
                    document()
                        .query_selector(selector)
                        .ok()
                        .flatten()
                        .is_some_and(|element| {
                            active.as_ref().is_some_and(|active| {
                                element == *active || element.contains(Some(active))
                            })
                        })
                })
                .unwrap_or(2);
            let next = if ev.shift_key() {
                (index + 3) % 4
            } else {
                (index + 1) % 4
            };
            if next == 1 && controller.editing.get_untracked() == Some(EditOrigin::Cell) {
                controller.carry_edit_to_bar();
            }
            use wasm_bindgen::JsCast;
            if let Some(settings) = document()
                .query_selector("[data-dnacalc=settings]")
                .ok()
                .flatten()
                .and_then(|e| e.dyn_into::<web_sys::HtmlDetailsElement>().ok())
            {
                settings.set_open(false);
            }
            if next == 2 {
                controller.focus_active_skin();
            } else {
                focus(selectors[next]);
            }
            return;
        }
        if ev.alt_key()
            && !controller.input_pending()
            && !ctrl
            && let Ok(slot) = ev.key().parse::<u8>()
            && switch_skin(controller, &registry_for_keys, slot).is_some()
        {
            ev.prevent_default();
        }
    };

    let tabs = move || {
        let current = controller.sheet.get();
        controller
            .sheets()
            .into_iter()
            .map(|tab| {
                let selected = Some(tab.id) == current;
                let id = tab.id;
                let name = tab.name.clone();
                view! {
                    <button
                        type="button"
                        role="tab"
                        aria-controls="dc-workspace"
                        tabindex=if selected { "0" } else { "-1" }
                        data-dnacalc="sheet-tab"
                        title=name.clone()
                        data-sheet=name
                        aria-selected=if selected { "true" } else { "false" }
                        on:click=move |_| controller.show_sheet(id)
                        on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                            let tabs = controller.sheets();
                            let index = tabs.iter().position(|tab| tab.id == id).unwrap_or(0);
                            let next = match ev.key().as_str() {
                                "ArrowLeft" => Some((index + tabs.len() - 1) % tabs.len()),
                                "ArrowRight" => Some((index + 1) % tabs.len()),
                                "Home" => Some(0),
                                "End" => Some(tabs.len() - 1),
                                _ => None,
                            };
                            if let Some(next) = next {
                                ev.prevent_default();
                                controller.show_sheet(tabs[next].id);
                                set_timeout(move || focus("[data-dnacalc=sheet-tab][aria-selected=true]"), std::time::Duration::from_millis(20));
                            }
                        }
                    >
                        {tab.name}
                    </button>
                }
            })
            .collect_view()
    };

    Effect::new(move |_| {
        controller.sheet.track();
        controller.summary.track();
        #[cfg(target_arch = "wasm32")]
        set_timeout(
            move || {
                let Ok(Some(tab)) =
                    document().query_selector("[data-dnacalc=sheet-tab][aria-selected=true]")
                else {
                    return;
                };
                let Some(nav) = tab.parent_element() else {
                    return;
                };
                let bounds = nav.get_bounding_client_rect();
                let cell = tab.get_bounding_client_rect();
                let shift = if cell.left() < bounds.left() {
                    cell.left() - bounds.left()
                } else if cell.right() > bounds.right() {
                    cell.right() - bounds.right()
                } else {
                    0.0
                };
                if shift != 0.0 {
                    nav.set_scroll_left(nav.scroll_left() + (shift.ceil() as i32));
                }
            },
            std::time::Duration::from_millis(20),
        );
    });

    let document_line = move || {
        controller.summary.with(|summary| {
            format!(
                "{}{}",
                summary.name,
                if summary.changed { " (changed)" } else { "" }
            )
        })
    };
    let provenance = move || {
        let Some(at) = controller.active_key() else {
            return String::new();
        };
        let cell = controller.active_cell();
        controller.summary.track();
        let label = controller.cell_label(at);
        match cell {
            Some(cell) => format!(
                "{label}: {}",
                if cell.provenance == cells::Provenance::Calculated {
                    "calculated by DNA Calc's engine"
                } else {
                    &cell.why
                }
            ),
            None => format!("{label}: empty"),
        }
    };
    let mode_value = move || {
        if controller.manual() {
            "manual"
        } else {
            "automatic"
        }
    };

    view! {
        <div class="dc-app" data-dnacalc="app" inert=move || controller.replacing.get().then_some("") on:keydown=on_app_key>
            <header class="dc-mast">
                <strong>"DNA Calc"</strong>
                <span class="dc-doc" data-dnacalc="document">{document_line}</span>
                <button type="button" data-dnacalc="new" disabled=move || assist.readonly.get() on:click=move |_| request_replace.run(Replacement::New)>"New"</button>
                <button type="button" data-dnacalc="open" disabled=move || assist.readonly.get() on:click=move |_| { if let Some(input) = file_input.get() { input.click(); } }>"Open"</button>
                    <input
                        node_ref=file_input
                        type="file"
                        data-dnacalc="open-file"
                        accept=".xlsx,.xlsm,.xlsb"
                        hidden=true
                        on:change=move |ev| {
                            use wasm_bindgen::JsCast;
                            if let Some(input) = ev
                                .target()
                                .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
                            {
                                open_chosen(controller, input, move |name, bytes| request_replace.run(Replacement::Open(name, bytes)));
                            }
                        }
                    />
                <button type="button" data-dnacalc="save" disabled=move || assist.readonly.get() on:click=save>"Save"</button>
                <button type="button" data-dnacalc="undo" disabled=move || controller.command_availability(dna_calc_model::CommandId::Undo).is_err() title="Undo (Ctrl+Z)" on:click=move |_| controller.undo()>"Undo"</button>
                <button type="button" data-dnacalc="redo" disabled=move || controller.command_availability(dna_calc_model::CommandId::Redo).is_err() title="Redo (Ctrl+Y)" on:click=move |_| controller.redo()>"Redo"</button>
                <button type="button" data-dnacalc="calculate" title="Calculate now (F9)" on:click=move |_| controller.recalculate()>"Calculate"</button>
                <button type="button" data-dnacalc="name-manager" disabled=move ||controller.command_availability(dna_calc_model::CommandId::NameManager).is_err() on:click=move |_|controller.command(dna_calc_model::Command::NameManager)>"Name Manager…"</button>
                <button type="button" data-dnacalc="format-cells" disabled=move ||controller.command_availability(dna_calc_model::CommandId::FormatCells).is_err() on:click=move |_|controller.command(dna_calc_model::Command::FormatCells)>"Format Cells…"</button>
                <button type="button" data-dnacalc="find-tools" disabled=move ||controller.command_availability(dna_calc_model::CommandId::Find).is_err() on:click=move |_|controller.command(dna_calc_model::Command::Find)>"Find…"</button>
                <button type="button" data-dnacalc="review-tools" disabled=move ||controller.assist.readonly.get() || controller.editing.get().is_some() on:click=move |_|controller.review_open.set(true)>"Rules & review"</button>
                <select
                    data-dnacalc="calc-mode"
                    aria-label="Calculation mode"
                    prop:value=mode_value
                    on:change=move |ev| {
                        controller.set_calc_mode(event_target_value(&ev) == "manual")
                    }
                >
                    <option value="automatic">"Automatic"</option>
                    <option value="manual">"Manual"</option>
                </select>
                <details class="dc-settings" data-dnacalc="settings" on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                    if ev.key() == "Escape" {
                        ev.prevent_default();
                        use wasm_bindgen::JsCast;
                        if let Some(settings) = document().query_selector("[data-dnacalc=settings]").ok().flatten().and_then(|e|e.dyn_into::<web_sys::HtmlDetailsElement>().ok()) { settings.set_open(false); }
                        focus("[data-dnacalc=settings-toggle]");
                    }
                }>
                    <summary data-dnacalc="settings-toggle">"Settings & diagnostics"</summary>
                    <div class="dc-settings-body">
                        <h2>"Display and calculation"</h2>
                <label class="dc-setting-label">"Theme"
                <select
                    class="dc-theme"
                    data-dnacalc="theme"
                    aria-label="Theme"
                    on:change=move |ev| dna_formula_ui::theme::set(&event_target_value(&ev))
                    prop:value=dna_formula_ui::theme::current()
                >
                    {dna_formula_ui::theme::CHOICES
                        .iter()
                        .map(|(id, label)| view! { <option value=*id>{*label}</option> })
                        .collect_view()}
                </select>
                </label>
                <label class="dc-iterate" title="Calculate circular references iteratively">
                    <input
                        type="checkbox"
                        data-dnacalc="iterate"
                        prop:checked=move || controller.iteration().0
                        on:change=move |ev| {
                            let (_, count, change) = controller.iteration();
                            controller.set_iteration(event_target_checked(&ev), count, change)
                        }
                    />
                    "Iterate"
                </label>
                <label class="dc-setting-label">"Maximum iterations"
                <input
                    type="number"
                    class="dc-iterations"
                    data-dnacalc="max-iterations"
                    aria-label="Maximum iterations"
                    title="Maximum iterations"
                    min="1"
                    max="32767"
                    prop:value=move || controller.iteration().1.to_string()
                    on:change=move |ev| {
                        let (on, _, change) = controller.iteration();
                        if let Ok(count) = event_target_value(&ev).trim().parse::<u32>()
                            && (1..=32767).contains(&count)
                        {
                            controller.set_iteration(on, count, change)
                        }
                    }
                />
                </label>
                <label class="dc-setting-label">"Maximum change"
                <input
                    type="text"
                    class="dc-max-change"
                    data-dnacalc="max-change"
                    aria-label="Maximum change"
                    title="Maximum change"
                    prop:value=move || controller.iteration().2.to_string()
                    on:change=move |ev| {
                        let (on, count, _) = controller.iteration();
                        if let Ok(change) = event_target_value(&ev).trim().parse::<f64>()
                            && change.is_finite()
                            && change >= 0.0
                        {
                            controller.set_iteration(on, count, change)
                        }
                    }
                />
                </label>
                        <p>"Calculation mode applies to this session. Downloads retain the opened file's calculation-mode declaration."</p>
                        <h2>"Diagnostics"</h2>
                        <p>{move || controller.active_cell().map(|cell| cell.why).unwrap_or_default()}</p>
                        <p data-dnacalc="status" data-refused=move || controller.status.get().refused.to_string()>{move || controller.status.get().text}</p>
                        <p>{move || format!("Authored revision {}. Open: {:?} ms; last operation: {:?} ms. Page wasm memory: {:?} bytes.", controller.summary.get().revision, controller.timings.get().open_ms, controller.timings.get().last_intent_ms, crate::controller::memory_bytes())}</p>
                        <p>"Keyboard: F6 and Shift+F6 move between commands, formula bar, workspace and sheets. F2 edits the active cell; Escape cancels a draft."</p>
                    </div>
                </details>
                <span class="dc-circular" data-dnacalc="circular">
                    {move || controller.circular_warning().unwrap_or_default()}
                </span>
                <nav class="dc-skins" aria-label="Skins">{skin_buttons}</nav>
            </header>
            <div class="dc-bar">
                <input
                    class="dc-namebox"
                    data-dnacalc="name-box"
                    aria-label="Name box"
                    prop:value=move || name_box.get()
                    on:input=move |ev| name_box.set(event_target_value(&ev))
                    on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                        if ev.key() == "Enter" {
                            ev.prevent_default();
                            go_to(name_box.get_untracked());
                        }
                    }
                />
                <div
                    class="dc-fx"
                    data-dnacalc="formula-bar"
                    // Input can arrive without focusin after Firefox's download UI, or
                    // after a commit leaves the same field focused. Capture its target
                    // before FormulaAssist reads the changed DOM value.
                    on:beforeinput=move |_| controller.activate_editor(EditOrigin::Bar, assist)
                    on:input=move |_| controller.edit_changed()
                    on:focusin=move |_| {
                        controller.activate_editor(EditOrigin::Bar, assist);
                    }
                >
                    {assist.field(Signal::derive(Vec::new), Some(bar_key), 1, "")}
                    {move || (controller.editing.get() == Some(EditOrigin::Bar)).then(|| view! {
                        {assist.popup()}
                        {assist.signature()}
                    })}
                </div>
            </div>
            <details class="dc-result" node_ref=result_details prop:open=move || result_open.get()
                data-dnacalc="result-details"
                on:toggle=move |_| { if let Some(details) = result_details.get_untracked() { if details.open() != result_open.get_untracked() { result_open.set(details.open()); } } }>
                <summary data-dnacalc="result-toggle">
                    <span class="dc-result-state" data-dnacalc="result-state">{move || controller.value_state().label()}</span>
                    <span class="dc-mono">{move || controller.active_key().map(|at| controller.cell_label(at)).unwrap_or_default()}</span>
                    <span class="dc-result-value" data-dnacalc="selected-value">{move || controller.active_cell().map(|cell| cell.text).unwrap_or_default()}</span>
                </summary>
                <div class="dc-result-body">
                    <p data-dnacalc="state-explanation">{move || controller.value_state().explanation()}</p>
                    <p data-dnacalc="cell-provenance">{provenance}</p>
                    {move || controller.active_cell().and_then(|cell| cell.error_meaning).map(|meaning| view! { <p data-dnacalc="error-meaning">{meaning}</p> })}
                    {move || controller.active_cell().and_then(|cell| cell.owner).map(|owner| view! { <p data-dnacalc="refusal-owner">{format!("Work package: {owner}")}</p> })}
                    {move || controller.active_cell().filter(|cell| cell.provenance == cells::Provenance::Refused || cell.kind == "error").map(|_| view! {
                        <button type="button" data-dnacalc="recover-edit" on:click=move |_| { controller.begin_edit(EditOrigin::Bar, None); controller.carry_edit_to_bar(); }>"Edit formula or input"</button>
                    })}
                    {move || controller.active_cell().filter(|cell| matches!(cell.provenance, cells::Provenance::Stale | cells::Provenance::FileCache)).map(|_| view! {
                        <button type="button" data-dnacalc="recover-calculate" on:click=move |_| controller.recalculate()>"Calculate now"</button>
                    })}
                </div>
            </details>
            <span class="dc-sr-only" role="status" aria-live="polite" aria-atomic="true" data-dnacalc="result-announcement">{move || format!("{}. {}. {}", controller.active_key().map(|at| controller.cell_label(at)).unwrap_or_default(), controller.value_state().label(), controller.active_cell().map(|cell| format!("{} {}", cell.text, cell.why)).unwrap_or_default())}</span>
            {move || controller.editing.get().map(|_| view! {
                <div class="dc-edit-tools" data-dnacalc="edit-session">
                    <span>{move || format!("Editing {}", controller.edit_label())}</span>
                    <span data-dnacalc="edit-mode">{move || if controller.point_mode.get() {
                        "Point · F2 to edit text"
                    } else { "Edit · F2 to point to references" }}</span>
                    <button type="button" data-dnacalc="commit-edit"
                        disabled=move || assist.readonly.get()
                        on:click=move |_| controller.finish_edit(crate::grid::selection::NavAction::CommitDown)>"Enter"</button>
                    <button type="button" data-dnacalc="cancel-edit"
                        disabled=move || assist.readonly.get()
                        on:click=move |_| { controller.cancel_edit(); controller.focus_active_skin(); }>"Cancel"</button>
                </div>
            })}
            <main id="dc-workspace" class="dc-main" tabindex="-1" aria-label="Workbook workspace">{skin_view}</main>
            {crate::commands::summary(controller)}
            <div class="dc-sheet-strip"><nav class="dc-tabs" role="tablist" aria-label="Worksheets" data-dnacalc="sheet-tabs">{tabs}</nav>{crate::sheets::controls(controller)}</div>
            <footer class="dc-status">
                <span data-dnacalc="worker-status" role="status" aria-live="polite" aria-atomic="true">{move || {
                    let health = controller.transport.get();
                    if health.pending > 0 { format!("Pending · {} workbook requests", health.pending) } else { health.state }
                }}</span>
                <span data-dnacalc="feedback" role="status" aria-live="polite" data-refused=move || controller.status.get().refused.to_string()>{move || {
                    let status = controller.status.get();
                    let message = if status.refused { status.text } else { status.text.split(" in ").next().unwrap_or("").split(": ").next().unwrap_or("").to_owned() };
                    format!("Last action: {message}")
                }}</span>
                {move || controller.download_unconfirmed.get().then(|| view! { <span data-dnacalc="download-warning">"Download unconfirmed · check your files"</span> })}
            </footer>
            {crate::review::controls(controller)}
            {crate::commands::controls(controller)}
            {crate::format::controls(controller)}
            {crate::names::controls(controller)}
            {crate::format::diagnostic(controller)}
            <dialog class="dc-replace" node_ref=replace_dialog data-dnacalc="replace-dialog" aria-labelledby="dc-replace-title" aria-describedby="dc-replace-description"
                on:cancel=move |ev: leptos::ev::Event| { ev.prevent_default(); keep_working(); }>
                <h2 id="dc-replace-title">"Keep your current work?"</h2>
                <p id="dc-replace-description">"This workbook has changes, an edited draft, or a download whose storage is unconfirmed. Keep working to download it first, or discard it to continue."</p>
                <div class="dc-dialog-actions">
                    <button type="button" autofocus=true data-dnacalc="keep-working" on:click=move |_| keep_working()>"Keep working"</button>
                    <button type="button" data-dnacalc="discard-work" on:click=move |_| { if let Some(next) = replacement.get_untracked() { replacement.set(None); replace(next); } }>"Discard and continue"</button>
                </div>
            </dialog>
        </div>
    }
}

/// The application, mounted in `parent` for the life of the page.
pub fn mount(parent: web_sys::HtmlElement) {
    std::mem::forget(leptos::mount::mount_to(parent, app));
}
