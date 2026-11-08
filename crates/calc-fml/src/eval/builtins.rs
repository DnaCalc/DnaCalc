//! Built-in name lookup uses the catalogue's generated surface table.

use calc_func::{Notation, catalogue};

#[cfg(test)]
static NAMES: std::sync::LazyLock<Vec<&'static str>> = std::sync::LazyLock::new(|| {
    catalogue().entries().filter(|target| target.notation() == Notation::Call && !matches!(target.kind(), calc_func::EntryKind::AddIn { .. }))
        .map(|target| target.spelling()).collect()
});

/// Whether `name` is a built-in function, matched without ASCII case.
pub(crate) fn is_builtin(name: &str) -> bool {
    catalogue().resolve_surface(Notation::Call, name).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn names_are_sorted_and_found_in_any_case() {
        assert!(NAMES.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(is_builtin("gcd") && is_builtin("Sum") && is_builtin("STDEV.P"));
        assert!(!is_builtin("zzgcd") && !is_builtin("TaxRate"));
    }
}
