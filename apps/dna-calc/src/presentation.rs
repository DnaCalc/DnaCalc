//! Browser font completion invalidates layout through the model, without a workbook command.
use leptos::prelude::*;
pub(crate) fn install(model: RwSignal<dna_calc_model::Model>) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{JsCast, closure::Closure};
        let fonts = document().fonts();
        let invalidate = move || {
            model.try_update(|m| {
                m.runtime.font_revision = m
                    .runtime
                    .font_revision
                    .checked_add(1)
                    .expect("Font presentation identities exhausted");
            });
        };
        let completed = Closure::<dyn FnMut()>::new(invalidate);
        for event in ["loadingdone", "loadingerror"] {
            let _ =
                fonts.add_event_listener_with_callback(event, completed.as_ref().unchecked_ref());
        }
        if let Ok(ready) = fonts.ready() {
            wasm_bindgen_futures::spawn_local(async move {
                if wasm_bindgen_futures::JsFuture::from(ready).await.is_ok() {
                    invalidate();
                }
            });
        }
        let held = StoredValue::new_local((fonts, completed));
        on_cleanup(move || {
            held.with_value(|(fonts, callback)| {
                for event in ["loadingdone", "loadingerror"] {
                    let _ = fonts.remove_event_listener_with_callback(
                        event,
                        callback.as_ref().unchecked_ref(),
                    );
                }
            })
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = model;
}
