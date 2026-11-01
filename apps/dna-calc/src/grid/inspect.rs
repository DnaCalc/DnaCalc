//! Read-only inspection of the actual renderer input. It retains no second cell projection.
use super::*;
pub(super) fn register(
    plan: Memo<render_plan::RenderPlan>,
    ctx: PaneContext,
    controller: Controller,
) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{JsCast, closure::Closure};
        let global = js_sys::global();
        let name = wasm_bindgen::JsValue::from_str("__dnacalcRenderPlans");
        let object = js_sys::Reflect::get(&global, &name)
            .ok()
            .filter(|v| v.is_object())
            .unwrap_or_else(|| {
                let object = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&global, &name, &object);
                object.into()
            });
        let key = wasm_bindgen::JsValue::from_str(&ctx.index.to_string());
        let inspect = Closure::<dyn FnMut() -> String>::new(move || {
            plan.try_with_untracked(|plan| {
                wire::encode(&dna_calc_model::render_plan::Inspection {
                    pane: ctx.index,
                    active: ctx.active.get_untracked() == ctx.index,
                    commands: crate::port::command_count(),
                    pending: controller.transport.get_untracked().pending,
                    plan,
                })
            })
            .unwrap_or_default()
        });
        let _ = js_sys::Reflect::set(&object, &key, inspect.as_ref().unchecked_ref());
        let held = StoredValue::new_local((object, key, inspect));
        on_cleanup(move || {
            held.with_value(|(object, key, callback)| {
                if js_sys::Reflect::get(object, key).ok().as_ref() == Some(callback.as_ref()) {
                    let _ = js_sys::Reflect::delete_property(
                        &js_sys::Object::from(object.clone()),
                        key,
                    );
                }
            })
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (plan, ctx, controller);
}
