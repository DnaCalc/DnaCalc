//! Fixture wasm test target `acceptance`. The dc-e8ql case copies it into
//! `tools/browser-fixture/tests/` of a copy of the workspace. It configures no run mode, so
//! wasm-bindgen-test-runner runs it under Node.js unless `WASM_BINDGEN_USE_BROWSER` sends it to the
//! browser whose driver the run names. Each test prints, through the console the runner forwards
//! under `--nocapture`, the user agent of whatever executed it, so the case can see independently
//! of the runner which program ran it. The second test's name contains the first's, so only an
//! exact filter selects one test.

#![allow(non_snake_case)]

use wasm_bindgen::prelude::*;
use wasm_bindgen_test::{console_log, wasm_bindgen_test};

#[wasm_bindgen(
    inline_js = "export function fixture_agent() { return typeof navigator === 'undefined' || navigator.userAgent === undefined ? 'no navigator' : navigator.userAgent; }"
)]
extern "C" {
    fn fixture_agent() -> String;
}

#[wasm_bindgen_test]
fn fixture_browser__alpha() {
    console_log!("FIXTURE-AGENT fixture_browser__alpha: {}", fixture_agent());
}

#[wasm_bindgen_test]
fn twin_of_fixture_browser__alpha() {
    console_log!(
        "FIXTURE-AGENT twin_of_fixture_browser__alpha: {}",
        fixture_agent()
    );
}
