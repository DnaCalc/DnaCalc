//! The Excel answer service's callable arities, exported as plain text.
//!
//! The broker and Excel worker use the checked-in copy
//! (`tests/fixtures/admission-families.tsv`). The test
//! `admission_export_is_current` fails when the declarations move away from it; regenerate with
//! `cargo run -q -p calc-catalogue --example export_admission_families`.
use crate::{BindingForm, DeclarationRef, EntryKind, Notation, WorksheetDeclaration};
use std::collections::BTreeSet;
use std::fmt::Write;

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

/// One `family<TAB>NAME<TAB>min<TAB>max` line per call, with excluded counts in an optional last field.
pub fn export() -> String {
    let families = FAMILIES.iter().flat_map(|(family, entries)| {
        entries
            .iter()
            .map(|entry| (*family, DeclarationRef::Worksheet(entry)))
    });
    let other = crate::declarations().map(|entry| ("other", entry));
    let mut seen = BTreeSet::new();
    let mut text = String::new();
    for (family, entry) in families.chain(other) {
        let name = entry.spelling();
        if entry.notation() != Notation::Call || !seen.insert(name) {
            continue;
        }
        let arity = entry.arity();
        let groups = entry.groups();
        // LET's binding form has name/value pairs followed by one required expression.
        let let_binding = entry.kind() == EntryKind::Binding(BindingForm::Let);
        let excluded = (arity.min..=arity.max)
            .filter(|&count| {
                groups.is_some_and(|group| group.incomplete(count))
                    || (let_binding && count % 2 == 0)
            })
            .map(|count| count.to_string())
            .collect::<Vec<_>>()
            .join(",");
        write!(text, "{family}\t{name}\t{}\t{}", arity.min, arity.max)
            .expect("writing to a String");
        if !excluded.is_empty() {
            text.push('\t');
            text.push_str(&excluded);
        }
        text.push('\n');
    }
    text
}
