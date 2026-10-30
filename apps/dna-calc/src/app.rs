//! The DNA Calc shell: the mast with the document's name and the file commands (New, Open, Save),
//! the edit and calculation commands (Undo, Redo, Calculate, the calculation mode), the skin
//! switcher, the formula bar (the name box and DNA OneCalc's formula field), the active skin, the
//! sheet tabs, and the status line with the active cell's provenance.
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
use crate::grid::{self, GridSkin};
use crate::ledger::LedgerSkin;
use crate::notebook::NotebookSkin;
use crate::skin::{SkinContext, SkinRegistry, SkinResolution, resolve_active_skin, switch_skin};
use crate::tree::TreeSkin;

use dna_onecalc::formula_field::FormulaAssist;
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
    use wasm_bindgen::JsCast;
    let array = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::of1(&array);
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet");
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
        .map_err(|e| format!("{e:?}"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|e| format!("{e:?}"))?;
    let anchor: web_sys::HtmlAnchorElement = document()
        .create_element("a")
        .map_err(|e| format!("{e:?}"))?
        .dyn_into()
        .map_err(|_| "no anchor".to_owned())?;
    anchor.set_href(&url);
    anchor.set_download(name);
    anchor.click();
    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn download(_name: &str, _bytes: &[u8]) -> Result<(), String> {
    Ok(())
}

/// Reads the file an input chose and opens it.
#[cfg(target_arch = "wasm32")]
fn open_chosen(controller: Controller, input: web_sys::HtmlInputElement) {
    let Some(file) = input.files().and_then(|files| files.get(0)) else {
        return;
    };
    let name = file.name();
    controller.status.set(crate::controller::Status {
        text: format!("Opening {name}"),
        refused: false,
    });
    wasm_bindgen_futures::spawn_local(async move {
        match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
            Ok(buffer) => {
                let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
                controller.open(&name, bytes, |_| {});
            }
            Err(error) => controller.status.set(crate::controller::Status {
                text: format!("{name} could not be read: {error:?}"),
                refused: true,
            }),
        }
        input.set_value("");
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn open_chosen(_controller: Controller, _input: web_sys::HtmlInputElement) {}

/// The application.
pub fn app() -> impl IntoView {
    let controller = Controller::new();
    let registry = registry();
    let active_skin: RwSignal<Option<String>> = RwSignal::new(None);
    let assist = FormulaAssist::new(controller.edit_text);

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
    let go_to = move |text: String| match cells::parse_address(&text) {
        Some((row, col)) => {
            controller.cancel_edit();
            controller.select(row, col);
            grid::focus_grid();
        }
        None => controller.status.set(crate::controller::Status {
            text: format!("{text:?} is not a cell address such as B3"),
            refused: true,
        }),
    };

    let bar_key = Callback::new(
        move |ev: leptos::ev::KeyboardEvent| match ev.key().as_str() {
            "Enter" if !ev.shift_key() && !ev.alt_key() => {
                ev.prevent_default();
                if controller.editing.get_untracked().is_none() {
                    controller.editing.set(Some(EditOrigin::Bar));
                }
                controller.commit_edit(|accepted| {
                    if accepted {
                        grid::focus_grid();
                    }
                });
            }
            "Escape" => {
                ev.prevent_default();
                controller.cancel_edit();
                grid::focus_grid();
            }
            _ => {}
        },
    );

    let save = move |_| {
        controller.save(move |name, bytes| {
            if let Err(error) = download(&name, &bytes) {
                controller.status.set(crate::controller::Status {
                    text: format!("Saved, but the download did not start: {error}"),
                    refused: true,
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
                        aria-pressed=pressed
                        title=format!("{title} (Alt+{slot})")
                        on:click=move |_| {
                            controller.cancel_edit();
                            switch_skin(active_skin, &registry, slot);
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
        let ctrl = ev.ctrl_key() || ev.meta_key();
        if ev.alt_key()
            && !ctrl
            && let Ok(slot) = ev.key().parse::<u8>()
            && switch_skin(active_skin, &registry_for_keys, slot).is_some()
        {
            ev.prevent_default();
            controller.cancel_edit();
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
                        data-dnacalc="sheet-tab"
                        data-sheet=name
                        aria-selected=if selected { "true" } else { "false" }
                        on:click=move |_| controller.show_sheet(id)
                    >
                        {tab.name}
                    </button>
                }
            })
            .collect_view()
    };

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
            Some(cell) => format!("{label}: {}", cell.why),
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
        <div class="dc-app" data-dnacalc="app" on:keydown=on_app_key>
            <header class="dc-mast">
                <strong>"DNA Calc"</strong>
                <select
                    class="dc-theme"
                    data-dnacalc="theme"
                    aria-label="Theme"
                    on:change=move |ev| dna_onecalc::theme::set(&event_target_value(&ev))
                    prop:value=dna_onecalc::theme::current()
                >
                    {dna_onecalc::theme::CHOICES
                        .iter()
                        .map(|(id, label)| view! { <option value=*id>{*label}</option> })
                        .collect_view()}
                </select>
                <span class="dc-doc" data-dnacalc="document">{document_line}</span>
                <button type="button" data-dnacalc="new" on:click=move |_| controller.new_workbook()>"New"</button>
                <label class="dc-open">
                    "Open"
                    <input
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
                                open_chosen(controller, input);
                            }
                        }
                    />
                </label>
                <button type="button" data-dnacalc="save" on:click=save>"Save"</button>
                <button type="button" data-dnacalc="undo" title="Undo (Ctrl+Z)" on:click=move |_| controller.undo()>"Undo"</button>
                <button type="button" data-dnacalc="redo" title="Redo (Ctrl+Y)" on:click=move |_| controller.redo()>"Redo"</button>
                <button type="button" data-dnacalc="calculate" title="Calculate now (F9)" on:click=move |_| controller.recalculate()>"Calculate"</button>
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
                    on:focusin=move |_| {
                        if controller.editing.get_untracked().is_none() {
                            controller.editing.set(Some(EditOrigin::Bar));
                        }
                    }
                >
                    {assist.field(Signal::derive(Vec::new), Some(bar_key), 1, "")}
                    {assist.popup()}
                    {assist.signature()}
                </div>
            </div>
            <main class="dc-main">{skin_view}</main>
            <nav class="dc-tabs" role="tablist" data-dnacalc="sheet-tabs">{tabs}</nav>
            <footer class="dc-status">
                <span
                    data-dnacalc="status"
                    data-refused=move || if controller.status.get().refused { "true" } else { "false" }
                >
                    {move || controller.status.get().text}
                </span>
                <span data-dnacalc="cell-provenance">{provenance}</span>
            </footer>
        </div>
    }
}

/// The application, mounted in `parent` for the life of the page.
pub fn mount(parent: web_sys::HtmlElement) {
    std::mem::forget(leptos::mount::mount_to(parent, app));
}
