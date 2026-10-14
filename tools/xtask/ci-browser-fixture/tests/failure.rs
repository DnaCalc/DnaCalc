//! The injected browser failure of `cargo xtask ci wasm`. The run sends this target to Chrome and
//! Firefox only when DNACALC_CI_INJECT_BROWSER_FAILURE=1, and the test fails there on purpose, after
//! printing the user agent of the browser that executed it. A CI wasm job that ends green with it has
//! not executed its browser tests.

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
fn injected_browser_failure() {
    console_log!(
        "CI-BROWSER-AGENT injected_browser_failure: {}",
        ci_user_agent()
    );
    panic!(
        "CI-INJECTED-FAILURE: this browser test fails on purpose (DNACALC_CI_INJECT_BROWSER_FAILURE=1)"
    );
}
