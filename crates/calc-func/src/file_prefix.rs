//! The file prefix associated with a catalogue entry.
use crate::catalogue::{FunctionCallTarget, Notation};
pub use calc_catalogue::FilePrefix;

impl FunctionCallTarget<'_> {
    /// The file prefix for a named function, when measured.
    pub fn file_prefix(self) -> Option<FilePrefix> {
        if self.notation() != Notation::Call {
            return None;
        }
        calc_catalogue::declaration(self.id()).and_then(|entry| entry.file_prefix())
    }
}

/// The name a file writes before a call of the function spelled `spelling` (an invariant,
/// upper-case name): the measured file prefix and the name (`_xlfn.XMATCH`, `SUM`); a name
/// without a measured prefix is written as it is.
pub fn file_call_name(spelling: &str) -> String {
    let prefix = calc_catalogue::file_prefix(spelling).map_or("", FilePrefix::text);
    format!("{prefix}{spelling}")
}

#[cfg(test)]
mod tests {
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn file_call_names_carry_the_measured_prefix() {
        assert_eq!(super::file_call_name("SORTBY"), "_xlfn.SORTBY");
        assert_eq!(super::file_call_name("SEQUENCE"), "_xlfn.SEQUENCE");
        assert_eq!(super::file_call_name("XMATCH"), "_xlfn.XMATCH");
        assert_eq!(super::file_call_name("AGGREGATE"), "_xlfn.AGGREGATE");
        assert_eq!(super::file_call_name("FILTER"), "_xlfn._xlws.FILTER");
        assert_eq!(super::file_call_name("SUM"), "SUM");
    }
}
