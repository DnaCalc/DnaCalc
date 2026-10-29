//! The DNA Calc browser bundle's entry. `cargo xtask app bundle calc` builds this binary for
//! wasm32-unknown-unknown and runs wasm-bindgen over it; the page (`web/index.html`) loads the
//! generated module, whose start function mounts the application ([`dna_calc::web::start`]).

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    // The same module starts the page and, in its own module worker, the calculation worker.
    if dna_calc::worker::in_worker() {
        dna_calc::worker::start();
    } else {
        dna_calc::web::start();
        dna_calc::client::spawn();
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!(
        "dna-calc-web runs in a browser: build the bundle with `cargo xtask app bundle calc` and open its index.html over HTTP"
    );
}
