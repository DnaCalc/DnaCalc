//! The scalar-computation families the Excel answer service admits, exported as plain text.
//!
//! `tools/excel-service/policy.py` reads the checked-in copy
//! (`tests/fixtures/admission-families.tsv`) instead of parsing Rust source. The test
//! `admission_export_is_current` fails when the declarations move away from it; regenerate with
//! `cargo run -q -p calc-catalogue --example export_admission_families`.
use crate::WorksheetDeclaration;

/// Families the service may draw from, with their declarations.
pub const FAMILIES: &[(&str, &[WorksheetDeclaration])] = &[
    ("math", crate::ported::math::ENTRIES),
    ("statistics", crate::ported::statistics::ENTRIES),
    ("distributions", crate::ported::distributions::ENTRIES),
    ("engineering", crate::ported::engineering::ENTRIES),
    ("rounding_more", crate::ported::rounding_more::ENTRIES),
    ("financial", crate::ported::financial::ENTRIES),
    ("bonds", crate::ported::bonds::ENTRIES),
    ("dates", crate::ported::dates::ENTRIES),
    ("text_functions", crate::ported::text_functions::ENTRIES),
    ("text_format", crate::ported::text_format::ENTRIES),
    ("logical_more", crate::logical_more::ENTRIES),
    ("logical", crate::logical::ENTRIES),
    ("error_fallback", crate::error_fallback::ENTRIES),
    ("lookup", crate::lookup::ENTRIES),
    ("information", crate::information::ENTRIES),
    ("arrays", crate::ported::arrays::ENTRIES),
    ("rounding", crate::rounding::ENTRIES),
    ("aggregate", crate::aggregate::ENTRIES),
];

/// One `family<TAB>NAME` line per declaration, in declaration order.
pub fn export() -> String {
    let mut text = String::new();
    for (family, entries) in FAMILIES {
        for entry in *entries {
            text.push_str(family);
            text.push('\t');
            text.push_str(entry.spelling);
            text.push('\n');
        }
    }
    text
}
