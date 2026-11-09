//! The catalogue's shared help text, selected for a resolved function.

use crate::{FunctionCallTarget, Notation};
pub use calc_catalogue::{FunctionHelp, description_of, help_of};
#[cfg(test)]
use calc_catalogue::HELP;

/// The description of a named function (see [`FunctionHelp::description`]), or `None` for an
/// operator and for a function the table does not describe.
pub fn description(target: FunctionCallTarget<'_>) -> Option<&'static str> {
    help(target).map(|h| h.description())
}

/// The help of a named function, or `None` for an operator and for a function the table does not
/// describe.
pub fn help(target: FunctionCallTarget<'_>) -> Option<&'static FunctionHelp> {
    if target.notation() != Notation::Call {
        return None;
    }
    help_of(target.spelling())
}

#[cfg(test)]
mod tests {
    use super::{HELP, description, description_of, help_of};
    use crate::{Notation, catalogue};

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_table_is_in_byte_order_without_duplicates() {
        for pair in HELP.windows(2) {
            assert!(
                pair[0].spelling.as_bytes() < pair[1].spelling.as_bytes(),
                "{:?}",
                pair
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn named_functions_are_described_and_operators_are_not() {
        let sum = catalogue()
            .resolve_surface(Notation::Call, "SUM")
            .expect("SUM is in the catalogue");
        assert_eq!(
            description(sum),
            Some("Use this function to add the values in cells.")
        );
        let plus = catalogue()
            .resolve_surface(Notation::Infix, "+")
            .expect("+ is in the catalogue");
        assert_eq!(description(plus), None);
        assert_eq!(description_of("NOSUCHFUNCTION"), None);
    }

    /// Microsoft's text is attributed: every Microsoft description has its support article, and
    /// every function has our own line too.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn microsoft_descriptions_carry_their_source() {
        for h in HELP {
            assert!(!h.own.is_empty(), "{}", h.spelling);
            if h.microsoft.is_some() {
                let source = h.source.unwrap_or_default();
                assert!(
                    source.starts_with("https://support.microsoft.com/"),
                    "{}",
                    h.spelling
                );
            }
        }
        let sum = help_of("SUM").unwrap();
        assert_eq!(sum.own, "Total of the numbers");
        assert_eq!(help_of("OP_IMPLICIT_INTERSECTION").unwrap().microsoft, None);
    }
}
