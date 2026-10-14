//! The route probe of `cargo xtask ci wasm`. Every CI wasm run sends it to Chrome and to Firefox
//! through the same route as the portable packages' wasm tests. It configures no run mode, so only
//! the route's `WASM_BINDGEN_USE_BROWSER` sends it to a browser, and the runner's banner must name
//! that browser. The test prints, through the console the runner forwards under `--nocapture`, the
//! user agent of whatever executed it; the CI manifest keeps that line as the browser's version.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;
use wasm_bindgen_test::{console_log, wasm_bindgen_test};

#[wasm_bindgen(
    inline_js = "export function ci_user_agent() { return typeof navigator === 'undefined' || navigator.userAgent === undefined ? 'no navigator' : navigator.userAgent; }"
)]
extern "C" {
    fn ci_user_agent() -> String;
}

#[wasm_bindgen_test]
fn route_probe() {
    console_log!("CI-BROWSER-AGENT route_probe: {}", ci_user_agent());
}
