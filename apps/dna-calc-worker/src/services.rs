//! Runtime clock, memory and session identity adapters.
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
/// Per-worker identity prevents another tab's clipboard token from selecting a local snapshot.
pub fn session_nonce() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let mut bytes = [0u8; 16];
        let crypto = js_sys::Reflect::get(&js_sys::global(), &"crypto".into())
            .ok()
            .and_then(|v| v.dyn_into::<web_sys::Crypto>().ok());
        if crypto.is_some_and(|crypto| crypto.get_random_values_with_u8_array(&mut bytes).is_ok()) {
            return bytes.iter().map(|b| format!("{b:02x}")).collect();
        }
        String::new() // Missing crypto disables implicit internal matching; explicit snapshot paste still works.
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        format!(
            "native-{}",
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )
    }
}

pub use dna_formula_ui::web::{memory_bytes, now_ms};
