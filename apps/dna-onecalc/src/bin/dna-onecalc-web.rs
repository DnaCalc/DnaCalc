//! The DNA OneCalc browser bundle's entry. `cargo xtask app bundle onecalc` builds this binary for
//! wasm32-unknown-unknown and runs wasm-bindgen over it; the page (`web/index.html`) loads the
//! generated module, whose start function mounts the app ([`dna_onecalc::web::start`]).

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    dna_onecalc::web::start();
}

#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!(
        "dna-onecalc-web runs in a browser: build the bundle with `cargo xtask app bundle onecalc` and open its index.html over HTTP"
    );
}
