//! The add-in panel (dc-0f1k.23): load an .xll or .xlam add-in (the native file dialog, or a typed
//! path), see what is loaded with the worksheet functions each registered, and unload one.
//!
//! The page only asks. Loading is native code, so the desktop app answers these requests itself
//! over `calc-addin-host`'s add-in set, as OneCalc's desktop does, and publishes the add-ins'
//! functions to the workbook; the browser worker loads nothing and the panel says so.
use crate::{
    controller::Controller,
    wire::{AddinList, Reply, Request},
};
use leptos::prelude::*;

pub(crate) fn controls(controller: Controller) -> impl IntoView {
    let list = RwSignal::new(AddinList::default());
    let status = RwSignal::new(String::new());
    let path = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    // What the host loads now, asked once as the page starts.
    controller.ask_result(Request::Addins, move |reply| {
        if let Ok(Reply::Addins(next)) = reply {
            list.set(next);
        }
    });
    let send = move |request: Request, done: String| {
        busy.set(true);
        status.set("Working…".to_owned());
        controller.ask_result(request, move |reply| {
            busy.set(false);
            match reply {
                Ok(Reply::Addins(next)) => {
                    let cancelled = next.cancelled;
                    list.set(next);
                    if cancelled {
                        status.set("No add-in chosen; nothing changed.".to_owned());
                    } else {
                        status.set(done);
                        // The formula field completes and documents the functions loaded now.
                        controller.offer_defined_names();
                    }
                }
                Ok(other) => status.set(format!("Unexpected answer: {other:?}")),
                Err(message) => status.set(message),
            }
        });
    };
    let load_native = move |_| send(Request::LoadAddinNative, "Loaded.".to_owned());
    let load_path = move |_| {
        let typed = path.get_untracked().trim().trim_matches('"').to_owned();
        if typed.is_empty() {
            status.set("Type the path of an .xll or .xlam add-in first.".to_owned());
            return;
        }
        send(
            Request::LoadAddin {
                path: typed.clone(),
            },
            format!("Loaded {typed}."),
        );
    };
    let native = move || list.with(|l| l.native);
    view! {
        <details class="dc-file-menu dc-addin-menu" data-dnacalc="addin-menu">
            <summary data-dnacalc="addin-toggle">
                "Add-ins "
                <span data-dnacalc="addin-count">
                    {move || list.with(|l| format!("({} loaded)", l.loaded.len()))}
                </span>
            </summary>
            <div class="dc-file-body">
                <h2>"Add-ins"</h2>
                <p data-dnacalc="addin-browser-note" hidden=native>
                    "Add-ins (.xll, .xlam) load in the DNA Calc desktop app; this browser page cannot load them."
                </p>
                <p>
                    <button
                        type="button"
                        data-dnacalc="addin-load"
                        disabled=move || !native() || busy.get()
                        on:click=load_native
                    >
                        "Load add-in…"
                    </button>
                </p>
                <p>
                    <input
                        type="text"
                        data-dnacalc="addin-path"
                        aria-label="Add-in path"
                        placeholder="C:\\path\\to\\addin.xll"
                        disabled=move || !native()
                        prop:value=move || path.get()
                        on:input=move |ev| path.set(event_target_value(&ev))
                    />
                    <button
                        type="button"
                        data-dnacalc="addin-load-path"
                        disabled=move || !native() || busy.get()
                        on:click=load_path
                    >
                        "Load path"
                    </button>
                </p>
                <p role="status" data-dnacalc="addin-status">
                    {move || status.get()}
                </p>
                <ul data-dnacalc="addins-loaded">
                    {move || {
                        list.get()
                            .loaded
                            .into_iter()
                            .map(|addin| {
                                let unload = addin.path.clone();
                                let functions = if addin.functions.is_empty() {
                                    "no worksheet functions".to_owned()
                                } else {
                                    addin.functions.join(", ")
                                };
                                let notes = if addin.notes.is_empty() {
                                    String::new()
                                } else {
                                    format!(" ({})", addin.notes.join("; "))
                                };
                                view! {
                                    <li data-dnacalc="addin-loaded" data-path=addin.path.clone()>
                                        <strong>{addin.kind.clone()}</strong>
                                        " "
                                        {addin.path.clone()}
                                        " - "
                                        <span data-dnacalc="addin-functions">{functions}</span>
                                        {notes}
                                        " "
                                        <button
                                            type="button"
                                            data-dnacalc="addin-unload"
                                            disabled=move || busy.get()
                                            on:click=move |_| {
                                                send(
                                                    Request::UnloadAddin {
                                                        path: unload.clone(),
                                                    },
                                                    format!("Unloaded {}.", unload),
                                                )
                                            }
                                        >
                                            "Unload"
                                        </button>
                                    </li>
                                }
                            })
                            .collect_view()
                    }}
                </ul>
            </div>
        </details>
    }
}
