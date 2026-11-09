//! The checked-in admission export matches the declarations.
//! Regenerate: `cargo run -q -p calc-catalogue --example export_admission_families > crates/calc-catalogue/tests/fixtures/admission-families.tsv`
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn admission_export_is_current() {
    let checked_in = include_str!("fixtures/admission-families.tsv").replace("\r\n", "\n");
    assert_eq!(
        checked_in,
        calc_catalogue::admission::export(),
        "admission-families.tsv is stale; regenerate it"
    );
}
