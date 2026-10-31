#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    dna_calc_worker::worker::start();
}
fn main() {}
