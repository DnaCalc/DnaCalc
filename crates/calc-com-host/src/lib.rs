//! Excel's interface identities and vtable layout for the measured add-in surface.
//! No class registration, activation hook, type-library registration or registry write.
#[cfg(target_arch = "wasm32")]
compile_error!("calc-com-host is native-only");
#[cfg(all(windows, not(target_arch = "x86_64")))]
compile_error!("calc-com-host supports Windows x64 only");

#[cfg(test)]
mod layout_tests {
    use calc_object_model::{Descriptor, InterfaceId};
    include!(concat!(env!("OUT_DIR"), "/layouts.rs"));

    /// Native-only: checks the native COM build generator against the committed descriptor.
    #[test]
    fn every_generated_layout_has_exactly_the_described_slots() {
        let descriptor = Descriptor::get();
        assert!(!GENERATED_LAYOUTS.is_empty());
        for &(index, slots) in GENERATED_LAYOUTS {
            let id = InterfaceId(index);
            assert_eq!(descriptor.ty(id).vtable_slots, Some(slots));
            for slot in 7..slots {
                assert!(descriptor.member_by_slot(id, slot).is_some(), "{} slot {slot}", descriptor.ty(id).name);
            }
        }
        for name in ["_Application", "_Workbook", "Workbooks", "_Worksheet", "Range", "Window", "COMAddIns", "COMAddIn", "CommandBarPopup", "_CommandBarButton", "IAccessible"] {
            let id = descriptor.id(name).unwrap();
            assert!(GENERATED_LAYOUTS.iter().any(|(index, _)| *index == id.0), "{name}");
        }
    }
}

#[cfg(windows)]
mod projection;
#[cfg(windows)]
pub use projection::Projection;
#[cfg(windows)]
mod rtd;
#[cfg(windows)]
pub use rtd::{RtdClient, RtdInput};

/// Read-only workbook metadata supplied by the full desktop application. OneCalc supplies no
/// bridge. The projection owns no workbook state and never borrows a live session by address.
pub trait WorkbookBridge {
    fn full_name(&self) -> String;
    fn active_sheet(&self) -> Option<String>;
    fn events_enabled(&self) -> bool;
    fn set_events_enabled(&self, enabled: bool);
    fn range(&self, address: &str) -> Result<calc_func::StoredReference, String>;
    fn retain(
        &self,
        reference: &calc_value::ReferenceLike,
    ) -> Result<calc_func::StoredReference, String>;
    fn resolve(
        &self,
        reference: &calc_func::StoredReference,
    ) -> Result<calc_value::ReferenceLike, String>;
    fn sheet_name(&self, sheet: calc_value::SheetId) -> Result<String, String>;
    /// Refuse Value's unimplemented Date/Currency conversions; Value2 always keeps raw values.
    fn value_format_supported(&self, cell: calc_value::CellKey) -> Result<(), String>;
}
