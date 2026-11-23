//! Per-position lifting for operands supplied to a catalogue call.

use crate::{
    ArgumentDemand, Availability, DeclarationRef, FunctionId, HeldReferences, Implementation,
    LegacyPlace, Notation, ParameterClass,
};

/// The enclosing expression's placement for a legacy call. Use `Value` for a root call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyContext {
    /// A single-value position, including the formula root.
    Value,
    /// A position below a reference-taking aggregate.
    Aggregate,
    /// A position below a parameter that permits array evaluation.
    Array,
    /// An aggregate's argument evaluated in an enclosing array position.
    ArrayAggregate,
    /// An unclassified enclosing position.
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Evaluation mode used to interpret an argument's enclosing position.
pub enum LiftingMode {
    /// Dynamic-array evaluation.
    Dynamic,
    /// Legacy evaluation with the enclosing expression's placement.
    Legacy(LegacyContext),
}

/// The evaluated argument's origin before admission. Arrays are ordinary, unboxed arrays;
/// ranges are single-area references with more than one cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiftingOrigin {
    /// An ordinary value array.
    Array,
    /// A multi-cell reference to one area.
    Range,
    /// An array whose elements are references.
    ReferenceArray,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// The call's treatment of an argument at one position.
pub enum Lifting {
    /// This argument does not trigger or extend lifting and is passed as a single operand.
    Whole,
    /// The call can run once per element. Reference elements may supply worksheet errors.
    Elementwise,
    /// The call can run once per reference, supplying that reference or reading its value.
    PerReference,
    /// Legacy implicit intersection supplies a single cell or array element.
    Intersect,
    /// The first element of an array that holds references supplies the argument.
    FirstReference,
    /// A measured input-origin restriction produces a worksheet error before lifting.
    Rejected,
    /// A lazy branch is consumed elementwise only when the selector lifts.
    ElementwiseWhenLifted,
    /// A lazy branch can be taken whole, but a lifted selector makes this input a typed fault.
    RefusedWhenLifted,
    /// The evaluator has no measured route for this case.
    Unmeasured,
    /// The entry is unavailable as a worksheet call.
    Unavailable,
    /// Formula binding, reference operators and provider-specific forms have their own routes.
    SpecialForm,
}

impl Lifting {
    /// Stable spelling used by the handbook and bindings export.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Whole => "whole",
            Self::Elementwise => "elementwise",
            Self::PerReference => "per-reference",
            Self::Intersect => "intersect",
            Self::FirstReference => "first-reference",
            Self::Rejected => "rejected",
            Self::ElementwiseWhenLifted => "elementwise-when-lifted",
            Self::RefusedWhenLifted => "refused-when-lifted",
            Self::Unmeasured => "unmeasured",
            Self::Unavailable => "unavailable",
            Self::SpecialForm => "special-form",
        }
    }
}

/// The lifting policy for one zero-based call position, including optional and repeated positions.
/// `None` means the entry's maximum arity excludes the position.
///
/// This describes the call before result publication. Argument acceptance, 3-D references, boxed
/// arrays, callable operands and the selected result of a scalar IFS/SWITCH use their own rules.
/// For IFS/SWITCH, also consult the entry's `whole_result_policy`.
pub fn parameter_lifting(
    entry: DeclarationRef,
    index: usize,
    mode: LiftingMode,
    origin: LiftingOrigin,
) -> Option<Lifting> {
    use Lifting::*;

    if index >= entry.arity().max {
        return None;
    }
    if entry.implementation() != Implementation::Held
        && matches!(
            entry.availability(),
            Availability::AbsentOnWindows | Availability::MacroSheetOnly
        )
    {
        return Some(Unavailable);
    }
    match entry.implementation() {
        Implementation::Binding => return Some(SpecialForm),
        Implementation::NotHeld { .. } | Implementation::Deferred { .. } => {
            return Some(Unmeasured);
        }
        Implementation::Held => {}
    }

    let traits = entry.traits();
    let context = match mode {
        LiftingMode::Dynamic => None,
        LiftingMode::Legacy(context) => Some(context),
    };
    let takes_value = value_slot(entry, index);
    let ref_array = origin == LiftingOrigin::ReferenceArray;
    let lazy = entry.demand().is_lazy();
    let lifts_references = mode.lifts_references(entry);
    let position_lifts_references =
        lifts_references && index >= usize::from(traits.lift_references_from);

    if !lazy && ref_array && context.is_some() && !lifts_references {
        return Some(FirstReference);
    }

    if origin == LiftingOrigin::Range {
        // Legacy call lowering inserts @ on a reference at these slots, including reference slots.
        if !lazy && context.is_some_and(|place| intersects_reference(entry, index, place)) {
            return Some(Intersect);
        }
        if takes_value
            && (traits.reject_multi_cell_reference || entry.id().is_database_field(index))
        {
            return Some(Rejected);
        }
    }

    // The lazy selector admits its scalar operands as though placed below an aggregate, even
    // in a legacy array context. Eager named calls use the first parameter's SingleValue rule.
    let admission = match context {
        Some(LegacyContext::Array) if lazy => Some(LegacyContext::Aggregate),
        Some(LegacyContext::ArrayAggregate) if entry.notation() == Notation::Call => {
            Some(LegacyContext::Value)
        }
        Some(place)
            if !lazy
                && place != LegacyContext::Unknown
                && entry.notation() == Notation::Call
                && legacy_place(entry, 0) == LegacyPlace::SingleValue =>
        {
            Some(LegacyContext::Value)
        }
        other => other,
    };

    if takes_value {
        match (admission, origin) {
            (Some(LegacyContext::Unknown), _) => return Some(Unmeasured),
            (Some(LegacyContext::Value), LiftingOrigin::ReferenceArray) => {
                return Some(FirstReference);
            }
            (Some(LegacyContext::Value), _)
            | (Some(LegacyContext::Aggregate), LiftingOrigin::Range) => return Some(Intersect),
            _ => {}
        }
    }

    if entry.demand() == ArgumentDemand::BranchOnCondition && index > 0 {
        return Some(match (context, origin) {
            (
                Some(LegacyContext::Value | LegacyContext::ArrayAggregate | LegacyContext::Unknown),
                _,
            ) => Whole,
            (Some(LegacyContext::Aggregate), LiftingOrigin::Range) => RefusedWhenLifted,
            _ => ElementwiseWhenLifted,
        });
    }

    if origin == LiftingOrigin::Range {
        return Some(if takes_value { Elementwise } else { Whole });
    }

    if !lazy
        && ref_array
        && !takes_value
        && !position_lifts_references
        && !traits.take_reference_array
        && traits.held_references == HeldReferences::Refuse
    {
        return Some(Unmeasured);
    }

    if traits.lifts_admitted_array(
        entry.id(),
        index,
        mode == LiftingMode::Dynamic,
        takes_value,
        || ref_array,
        || lifts_references,
    ) {
        Some(
            if ref_array && (!takes_value || !lazy && lifts_references) {
                PerReference
            } else {
                Elementwise
            },
        )
    } else {
        Some(Whole)
    }
}

fn value_slot(entry: DeclarationRef, index: usize) -> bool {
    let parameters = entry.parameters();
    // Runtime parameter class uses the final declaration for the repeated tail. ArgumentGroups
    // maps entry-admission classes and signature names, which are separate from that dispatch.
    parameters
        .get(index.min(parameters.len().saturating_sub(1)))
        .is_some_and(|parameter| parameter.class == ParameterClass::Value)
}

fn legacy_place(entry: DeclarationRef, index: usize) -> LegacyPlace {
    LegacyPlace::for_position(entry.parameters(), index)
}

fn intersects_reference(entry: DeclarationRef, index: usize, place: LegacyContext) -> bool {
    legacy_place(entry, index).intersects_reference(place == LegacyContext::Array)
}

impl LiftingMode {
    fn lifts_references(self, entry: DeclarationRef) -> bool {
        match self {
            Self::Dynamic => entry.traits().lift_dynamic_references,
            Self::Legacy(_) => entry
                .parameters()
                .iter()
                .any(|parameter| parameter.legacy.lifts_references()),
        }
    }
}

impl FunctionId {
    /// The database family admits only a single referenced cell as its field.
    pub fn is_database_field(self, index: usize) -> bool {
        index == 1 && self.family() == crate::ported::database::CODE
    }
}

const MODES: [(&str, LiftingMode); 6] = [
    ("dynamic", LiftingMode::Dynamic),
    ("legacy-root", LiftingMode::Legacy(LegacyContext::Value)),
    (
        "legacy-aggregate",
        LiftingMode::Legacy(LegacyContext::Aggregate),
    ),
    ("legacy-array", LiftingMode::Legacy(LegacyContext::Array)),
    (
        "legacy-array-aggregate",
        LiftingMode::Legacy(LegacyContext::ArrayAggregate),
    ),
    (
        "legacy-unknown",
        LiftingMode::Legacy(LegacyContext::Unknown),
    ),
];
const ORIGINS: [(&str, LiftingOrigin); 3] = [
    ("array", LiftingOrigin::Array),
    ("range", LiftingOrigin::Range),
    ("reference-array", LiftingOrigin::ReferenceArray),
];

/// Generate the handbook and binding table. Consecutive positions with the same name and
/// policies share one row; the counts include every position covered by the row.
pub fn export_tsv() -> String {
    use std::fmt::Write;

    let mut body =
        String::from("id\tspelling\tnotation\tavailability\tfirst\tlast\tparameter\twhole-result");
    for (mode, _) in MODES {
        for (origin, _) in ORIGINS {
            write!(body, "\t{mode}/{origin}").unwrap();
        }
    }
    body.push('\n');

    let mut entries = 0;
    let mut positions = 0;
    let mut counts = std::collections::BTreeMap::new();
    for entry in crate::declarations() {
        entries += 1;
        let max = entry.arity().max;
        positions += max;
        let row = |index| {
            let parameters = entry.parameters();
            let name = parameters
                .get(index)
                .or_else(|| {
                    entry
                        .groups()
                        .and_then(|groups| groups.parameter(index))
                        .and_then(|named| parameters.get(named))
                })
                .or_else(|| parameters.last())
                .map_or("", |parameter| parameter.name);
            let policies = MODES.map(|(_, mode)| {
                ORIGINS.map(|(_, origin)| parameter_lifting(entry, index, mode, origin).unwrap())
            });
            (name, policies)
        };
        let prefix = format!(
            "{}\t{}\t{:?}\t{:?}",
            entry.id().raw(),
            entry.spelling(),
            entry.notation(),
            entry.availability()
        );
        if max == 0 {
            writeln!(
                body,
                "{prefix}\t\t\t\t{:?}{}",
                entry.traits().whole_result_policy,
                "\t".repeat(MODES.len() * ORIGINS.len())
            )
            .unwrap();
        }
        let mut first = 0;
        while first < max {
            let (name, policies) = row(first);
            let last = ((first + 1)..max)
                .find(|index| row(*index) != (name, policies))
                .unwrap_or(max);
            write!(
                body,
                "{prefix}\t{}\t{last}\t{name}\t{:?}",
                first + 1,
                entry.traits().whole_result_policy
            )
            .unwrap();
            for policy in policies.into_iter().flatten() {
                *counts.entry(policy.name()).or_insert(0) += last - first;
                write!(body, "\t{}", policy.name()).unwrap();
            }
            body.push('\n');
            first = last;
        }
    }
    let mut tsv = format!(
        "# Generated by cargo run -q -p calc-catalogue --example export_parameter_lifting\n\
         # First/last are inclusive one-based call positions. Empty positions mark a zero-argument entry.\n\
         # Counts are over argument positions, modes and origins.\n\
         # Entries: {entries}\n# Positions: {positions}\n"
    );
    for (policy, count) in counts {
        writeln!(tsv, "# {policy}: {count}").unwrap();
    }
    tsv.push_str(&body);
    tsv
}
