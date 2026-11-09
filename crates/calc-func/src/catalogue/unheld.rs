//! Attach the refusal and binding implementations to shared catalogue declarations.
use super::{CatalogueEntry, EntryKind, Family, HostRouteKind, Implementation, Kernel};
use calc_catalogue::unheld::{UNHELD_ENTRIES, UnheldEntry, UnheldKind};

const fn attach(data: UnheldEntry) -> CatalogueEntry {
    let kind = match data.kind {
        UnheldKind::Worksheet => EntryKind::Worksheet,
        UnheldKind::Binding(form) => EntryKind::Binding(form),
        UnheldKind::Operator { file_spelling } => EntryKind::Operator { file_spelling },
        UnheldKind::Rtd { provider } => EntryKind::HostRoute {
            provider,
            route: HostRouteKind::Rtd,
        },
    };
    let kernel = match data.implementation {
        Implementation::Binding => Kernel::Binding,
        Implementation::NotHeld { owner } => Kernel::NotHeld { owner },
        Implementation::Deferred { class } => Kernel::Deferred { class },
        Implementation::Held => panic!("a held entry needs an attached kernel"),
    };
    CatalogueEntry {
        id: data.id,
        spelling: data.spelling,
        notation: data.notation,
        kind,
        availability: data.availability,
        arity: data.arity,
        groups: None,
        parameters: data.parameters,
        result: data.result,
        demand: data.demand,
        traits: data.traits,
        kernel,
    }
}

const fn attach_all() -> [CatalogueEntry; 16] {
    assert!(UNHELD_ENTRIES.len() == 16);
    let mut out = [attach(UNHELD_ENTRIES[0]); 16];
    let mut i = 1;
    while i < out.len() {
        out[i] = attach(UNHELD_ENTRIES[i]);
        i += 1;
    }
    out
}
const ENTRIES: [CatalogueEntry; 16] = attach_all();
pub(super) const FAMILY: Family = Family {
    code: 325,
    entries: &ENTRIES,
};

#[cfg(test)]
mod tests {
    use crate::{FunctionId, catalogue};

    // Snapshot produced by the previous catalogue at 5f2652f8, before moving these entries.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn original_entries_equal_shared_declarations() {
        let mut actual = String::new();
        for member in 1..=16 {
            let t = catalogue()
                .resolve(FunctionId::from_raw((325 << 16) | member))
                .unwrap();
            actual.push_str(&format!(
                "{}\t{}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\n",
                t.id().raw(),
                t.spelling(),
                t.notation(),
                t.kind(),
                t.availability(),
                t.implementation(),
                t.arity(),
                t.result_form(),
                t.argument_demand(),
                t.traits(),
                t.signature_help()
            ));
        }
        assert_eq!(
            actual,
            include_str!("../../../calc-catalogue/tests/fixtures/unheld-before.tsv")
        );
    }
}
