//! Fixture test target `matrix`, copied into `tools/ci-matrix-fixture/tests/` of a copy of the
//! workspace by the infra-m0.2 cases. It is not an acceptance target, so `cargo xtask ci` runs it.
//! Natively its test writes a marker file named after the nonce the case compiled into it
//! (`CI_MATRIX_NONCE`) into `CI_MATRIX_MARKERS`; for wasm32 its test prints that nonce and the user
//! agent of whatever executed it. Either shows, independently of the CI manifest, that this build
//! of the test ran, and where.

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn ci_matrix_native() {
    let nonce = env!("CI_MATRIX_NONCE");
    println!("CI-MATRIX native {nonce}");
    let dir = std::env::var_os("CI_MATRIX_MARKERS").expect("CI_MATRIX_MARKERS names a directory");
    let dir = std::path::Path::new(&dir);
    std::fs::create_dir_all(dir).expect("create the marker directory");
    std::fs::write(dir.join(format!("matrix-native-{nonce}")), nonce).expect("write the marker");
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_test::{console_log, wasm_bindgen_test};

    #[wasm_bindgen(
        inline_js = "export function ci_matrix_agent() { return typeof navigator === 'undefined' || navigator.userAgent === undefined ? 'no navigator' : navigator.userAgent; }"
    )]
    extern "C" {
        fn ci_matrix_agent() -> String;
    }

    #[wasm_bindgen_test]
    fn ci_matrix_wasm() {
        console_log!(
            "CI-MATRIX-AGENT {}: {}",
            env!("CI_MATRIX_NONCE"),
            ci_matrix_agent()
        );
    }
}
