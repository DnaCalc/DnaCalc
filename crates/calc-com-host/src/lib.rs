//! Excel/Office COM projection of the one Rust object model and descriptor.
//! Vtables and IDispatch lower to the same Call; only COM-owned Office members live here.
//! No class registration, activation hook, type-library registration or registry write.
#[cfg(target_arch = "wasm32")]
compile_error!("calc-com-host is native-only");
#[cfg(all(windows, not(target_arch = "x86_64")))]
compile_error!("calc-com-host supports Windows x64 only");

#[cfg(any(windows, test))]
mod arguments;

#[cfg(test)]
mod layout_tests {
    use calc_object_model::{Descriptor, InterfaceId};
    include!(concat!(env!("OUT_DIR"), "/layouts.rs"));
    include!(concat!(env!("OUT_DIR"), "/foreign_coverage.rs"));

    /// Native-only: verifies that generated foreign-signature checks cover every descriptor parameter.
    #[test]
    fn every_foreign_parameter_has_a_descriptor_generated_type_check() {
        let d = Descriptor::get();
        for name in [
            "_IDTExtensibility2",
            "IRibbonExtensibility",
            "IRtdServer",
            "IRTDUpdateEvent",
        ] {
            let id = d.id(name).unwrap();
            let checked: Vec<_> = FOREIGN_PARAMETERS
                .iter()
                .filter(|(interface, _, _)| *interface == name)
                .collect();
            assert_eq!(checked.len(), d.members(id).len());
            for member in d.members(id) {
                assert!(
                    checked
                        .iter()
                        .any(|(_, slot, parameters)| Some(*slot) == member.slot
                            && *parameters == member.params.len())
                );
            }
        }
    }

    /// Native-only: checks the native COM build generator against the committed descriptor.
    #[test]
    fn every_generated_layout_has_exactly_the_described_slots() {
        let descriptor = Descriptor::get();
        assert!(!GENERATED_LAYOUTS.is_empty());
        for &(index, slots) in GENERATED_LAYOUTS {
            let id = InterfaceId(index);
            assert_eq!(descriptor.ty(id).vtable_slots, Some(slots));
            for slot in 7..slots {
                assert!(
                    descriptor.member_by_slot(id, slot).is_some(),
                    "{} slot {slot}",
                    descriptor.ty(id).name
                );
            }
        }
        for name in [
            "_Application",
            "_Workbook",
            "Workbooks",
            "_Worksheet",
            "Range",
            "Window",
            "COMAddIns",
            "COMAddIn",
            "CommandBarPopup",
            "_CommandBarButton",
            "IAccessible",
        ] {
            let id = descriptor.id(name).unwrap();
            assert!(
                GENERATED_LAYOUTS.iter().any(|(index, _)| *index == id.0),
                "{name}"
            );
        }
    }
}

#[cfg(windows)]
mod application;
#[cfg(windows)]
mod dispatch;
#[cfg(windows)]
mod events;
#[cfg(windows)]
mod interfaces;
#[cfg(windows)]
mod office;
#[cfg(windows)]
mod projection;
#[cfg(windows)]
mod rtd_native;
#[cfg(windows)]
mod runtime;
#[cfg(all(windows, test))]
mod tests;
#[cfg(windows)]
mod typeinfo;
#[cfg(windows)]
mod values;
#[cfg(windows)]
pub use projection::Projection;
#[cfg(windows)]
mod rtd;
#[cfg(windows)]
pub use rtd::{RtdClient, RtdInput};
