//! Explicit structural choices for a rectangular selection.
use crate::controller::Controller;
use leptos::prelude::*;
pub fn controls(controller: Controller) -> impl IntoView {
    let dialog = NodeRef::<leptos::html::Dialog>::new();
    Effect::new(move |_| {
        if let Some(dialog) = dialog.get() {
            if controller.structural_choice.get().is_some() && !dialog.open() {
                let _ = dialog.show_modal();
            }
            if controller.structural_choice.get().is_none() && dialog.open() {
                dialog.close();
                super::focus_grid();
            }
        }
    });
    let cancel = move || {
        if let Some(dialog) = dialog.get_untracked() {
            dialog.close();
        }
        controller.structural_choice.set(None);
        super::focus_grid();
    };
    let command = move |mode| {
        if let Some(insert) = controller.structural_choice.get_untracked() {
            controller.structure_command(insert, mode);
        }
    };
    view! {
        <dialog
            class="dc-replace dc-structure-dialog"
            node_ref=dialog
            data-dnacalc="structure-dialog"
            aria-labelledby="dc-structure-title"
            on:cancel=move |ev: leptos::ev::Event| {
                ev.prevent_default();
                cancel();
            }
        >
            <h2 id="dc-structure-title">
                {move || {
                    if controller.structural_choice.get() == Some(true) {
                        "Insert cells"
                    } else {
                        "Delete cells"
                    }
                }}
            </h2>
            <p>
                {move || {
                    controller.selected_area().map(|a| controller.area_label(a)).unwrap_or_default()
                }}
            </p>
            <div class="dc-structure-choices">
                <button data-dnacalc="shift-down" on:click=move |_| command("down")>
                    {move || {
                        if controller.structural_choice.get() == Some(true) {
                            "Shift cells down"
                        } else {
                            "Shift cells up"
                        }
                    }}
                </button>
                <button data-dnacalc="shift-right" on:click=move |_| command("right")>
                    {move || {
                        if controller.structural_choice.get() == Some(true) {
                            "Shift cells right"
                        } else {
                            "Shift cells left"
                        }
                    }}
                </button>
                <button data-dnacalc="shift-rows" on:click=move |_| command("rows")>
                    "Entire rows"
                </button>
                <button data-dnacalc="shift-columns" on:click=move |_| command("columns")>
                    "Entire columns"
                </button>
            </div>
            <p role="status">
                {move || {
                    if controller.status.get().refused {
                        controller.status.get().text
                    } else {
                        String::new()
                    }
                }}
            </p>
            <div class="dc-dialog-actions">
                <button data-dnacalc="structure-cancel" on:click=move |_| cancel()>
                    "Cancel"
                </button>
            </div>
        </dialog>
    }
}
