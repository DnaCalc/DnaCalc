//! Level-zero provider controls. The native file picker is a host operation.

use crate::tools::Port;
use calc_skin::{
    FormulaAnswer, FormulaOperation, FormulaQuery, NativeProvider, ProviderCatalog,
    ProviderOperation,
};
use leptos::prelude::*;

#[component]
pub(crate) fn Providers(
    port: Port,
    catalogue: RwSignal<ProviderCatalog>,
    changed: Callback<()>,
) -> impl IntoView {
    let notice = RwSignal::new(String::new());
    let source = RwSignal::new(String::new());
    let module = RwSignal::new("Module1".to_owned());
    let trust = RwSignal::new(None::<NativeProvider>);
    let locked = RwSignal::new(false);
    port.with_value(|port| {
        port.ask(
            FormulaQuery::Providers,
            Box::new(move |answer| {
                if let Ok(FormulaAnswer::Providers(info)) = answer {
                    let _ = catalogue.try_set(info);
                }
            }),
        )
    });
    let operate = move |operation| {
        locked.set(true);
        notice.set(String::new());
        port.with_value(|port| {
            port.operate(
                FormulaOperation::Provider(operation),
                Box::new(move |answer| {
                    let _ = locked.try_set(false);
                    match answer {
                        Ok(FormulaAnswer::ProviderCandidate(candidate)) => {
                            let _ = trust.try_set(candidate);
                        }
                        Ok(FormulaAnswer::Providers(info)) => {
                            let _ = catalogue.try_set(info);
                            let _ = trust.try_set(None);
                            changed.run(());
                        }
                        Ok(_) => {
                            let _ = notice.try_set("Unexpected provider reply".into());
                        }
                        Err(refusal) => {
                            let _ = notice.try_set(crate::refusal_reason(&refusal));
                        }
                    }
                }),
            )
        });
    };
    let libraries = move || {
        catalogue.get().libraries.into_iter().map(|library| {
        let path = library.path.clone();
        let name = library.path.rsplit(['/', '\\']).next().unwrap_or(&library.path).to_owned();
        view! {
            <li><code>{name}</code>" · "{library.functions.join(", ")}
                <button type="button" disabled=locked on:click=move |_| operate(ProviderOperation::UnloadNative { path: path.clone() })>"Unload add-in"</button>
            </li>
        }
    }).collect_view()
    };
    let trust_card = move || {
        trust.get().map(|candidate| {
        let NativeProvider { path, sha256 } = candidate;
        view! {
            <section class="dna-fe-trust" role="group" aria-label="Add-in trust">
                <h3>"Load this add-in?"</h3>
                <p><code>{path}</code></p>
                <p>"SHA-256 " <code>{sha256}</code></p>
                <p>"The Windows desktop host will load this file. XLL add-ins execute native machine code outside VBA Guarded containment. Publisher signature unverified."</p>
                <div class="dna-fe-actions">
                    <button type="button" disabled=locked on:click=move |_| operate(ProviderOperation::LoadNative)>"Load once"</button>
                    <button type="button" disabled=locked on:click=move |_| { trust.set(None); operate(ProviderOperation::CancelNative); }>"Cancel"</button>
                </div>
            </section>
        }
    })
    };
    view! {
        <section class="dna-fe-section" aria-labelledby="dna-fe-addins-heading">
            <h2 id="dna-fe-addins-heading">"Add-ins"</h2>
            <Show when=move || catalogue.with(|info| info.native) fallback=|| view! { <p>"Native add-ins are available in the Windows desktop app."</p> }>
                <button type="button" disabled=move || locked.get() || trust.with(Option::is_some)
                    on:click=move |_| operate(ProviderOperation::ChooseNative)>"Choose native add-in"</button>
                <ul>{libraries}</ul>
                {trust_card}
            </Show>
            <details data-skin0="vba">
                <summary>"VBA module"</summary>
                <label>"Module name" <input prop:value=module on:input=move |event| module.set(event_target_value(&event))/></label>
                <label>"Module source" <textarea rows="4" data-skin0="vba-source" prop:value=source on:input=move |event| source.set(event_target_value(&event))></textarea></label>
                <div class="dna-fe-actions">
                    <button type="button" disabled=move || locked.get() || source.with(|source| source.trim().is_empty())
                        data-skin0="load-vba" on:click=move |_| operate(ProviderOperation::LoadVba { name: module.get_untracked(), source: source.get_untracked() })>"Load VBA module"</button>
                    <button type="button" disabled=move || locked.get() || !catalogue.with(|info| info.vba_loaded)
                        data-skin0="unload-vba" on:click=move |_| operate(ProviderOperation::UnloadVba)>"Unload VBA module"</button>
                </div>
            </details>
            <p aria-live="polite">{move || notice.get()}</p>
        </section>
    }
}
