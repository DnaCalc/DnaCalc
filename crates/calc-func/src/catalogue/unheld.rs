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
        Implementation::Held => match data.kind {
            UnheldKind::Rtd { .. } => Kernel::Eager(rtd_without_connector),
            _ => panic!("a held entry needs an attached kernel"),
        },
    };
    CatalogueEntry {
        id: data.id,
        spelling: data.spelling,
        notation: data.notation,
        kind,
        availability: data.availability,
        arity: data.arity,
        groups: data.groups,
        parameters: data.parameters,
        result: data.result,
        demand: data.demand,
        traits: data.traits,
        kernel,
    }
}

/// RTD where the evaluation has no owner connector (the browser, a host without COM, a judged
/// call): no real-time-data server can be started there, and Excel's answer when no server can
/// be started for the ProgID is #N/A (Excel 16.0.20527, dc-snz.23.4: an unknown, numeric or
/// empty ProgID gives #N/A without a connection; 1,546 Touchstone calls over random ProgIDs,
/// servers and topics answered #N/A, or an argument's error). An error argument is returned as
/// that error, the first one in argument order, before any server is looked for; an array or a
/// reference of more than one cell is refused as not modelled, as the connector route refuses
/// it. A host with a connector registers CalcFml's provider RTD, which this kernel never sees.
fn rtd_without_connector(
    arguments: &[crate::value::CalcValue],
    services: &mut crate::call::EvalServices<'_>,
) -> Result<crate::value::CalcValue, crate::call::FunctionFault> {
    use crate::kernel::Site;
    use crate::value::{CalcValue, CoreRef, WorksheetErrorCode};
    let site = Site::new(calc_catalogue::unheld::RTD_ID, services);
    let step = (|| {
        for (index, argument) in arguments.iter().enumerate() {
            let value = match argument.core_ref() {
                CoreRef::Reference(reference) => {
                    let [area] = reference.areas() else {
                        return Err(site.not_modelled(Some(index)));
                    };
                    if area.cell_count() != 1 {
                        return Err(site.not_modelled(Some(index)));
                    }
                    let cells = super::aggregate::reader(&site, services, index)?;
                    let mut held = CalcValue::empty();
                    super::aggregate::read_reference(&site, cells, reference, index, &mut |_, _, value| {
                        held = value.clone();
                        Ok(())
                    })?;
                    held
                }
                _ => argument.clone(),
            };
            match value.core_ref() {
                CoreRef::Error(code) => return Ok(CalcValue::error(code)),
                CoreRef::Array(_) | CoreRef::Reference(_) => {
                    return Err(site.not_modelled(Some(index)));
                }
                _ => {}
            }
        }
        Ok(CalcValue::error(WorksheetErrorCode::NA))
    })();
    Site::finish(step)
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
