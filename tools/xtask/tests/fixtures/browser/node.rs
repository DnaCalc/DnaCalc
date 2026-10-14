//! Fixture wasm test target `node`. The dc-e8ql case copies it into `tools/browser-fixture/tests/`
//! of a copy of the workspace. It configures `run_in_node_experimental`, which no variable
//! overrides, so wasm-bindgen-test-runner runs it under Node.js whatever the run sets, and it
//! passes there: it stands for a wasm case that is not configured for a browser. It prints the user
//! agent of whatever executed it, as the `acceptance` target does.

#![allow(non_snake_case)]

use wasm_bindgen::prelude::*;
use wasm_bindgen_test::{console_log, wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_node_experimental);

#[wasm_bindgen(
    inline_js = "export function fixture_agent() { return typeof navigator === 'undefined' || navigator.userAgent === undefined ? 'no navigator' : navigator.userAgent; }"
)]
extern "C" {
    fn fixture_agent() -> String;
}

#[wasm_bindgen_test]
fn fixture_browser_node__alpha() {
    console_log!(
        "FIXTURE-AGENT fixture_browser_node__alpha: {}",
        fixture_agent()
    );
}
