//! One ID lookup across the shared function declaration fragments.
use crate::unheld::{UnheldEntry, UnheldKind};
use crate::{
    ArgumentDemand, ArgumentGroups, Availability, EntryKind, EntryTraits, FunctionId,
    HostRouteKind, Implementation, Notation, Parameter, ResultForm, WorksheetDeclaration,
};
use calc_value::CallableArityShape;

const FAMILIES: &[&[WorksheetDeclaration]] = &[
    crate::ported::distributions::ENTRIES,
    crate::ported::bonds::ENTRIES,
    crate::ported::math::ENTRIES,
    crate::ported::lambda_helpers::ENTRIES,
    crate::ported::statistics::ENTRIES,
    crate::ported::grouping::ENTRIES,
    crate::ported::host_facts::ENTRIES,
    crate::ported::financial::ENTRIES,
    crate::ported::dates::ENTRIES,
    crate::ported::rounding_more::ENTRIES,
    crate::ported::engineering::ENTRIES,
    crate::ported::database::ENTRIES,
    crate::ported::arrays::ENTRIES,
    crate::ported::subtotal::ENTRIES,
    crate::ported::text_format::ENTRIES,
    crate::ported::text_functions::ENTRIES,
    crate::ported::random::ENTRIES,
    crate::ported::cube_functions::ENTRIES,
    crate::ported::fetching::ENTRIES,
    crate::ported::importing::ENTRIES,
    crate::operators::arithmetic::ENTRIES,
    crate::operators::comparison::ENTRIES,
    crate::operators::text::ENTRIES,
    crate::offset::ENTRIES,
    crate::aggregate::ENTRIES,
    crate::logical::ENTRIES,
    crate::logical_more::ENTRIES,
    crate::information::ENTRIES,
    crate::rounding::ENTRIES,
    crate::conditional::ENTRIES,
    crate::lookup::ENTRIES,
    crate::error_fallback::ENTRIES,
    crate::euro_tools::ENTRIES,
    crate::reference_text::ENTRIES,
    crate::group_pivot::ENTRIES,
];

const fn declaration_count() -> usize {
    let mut count = crate::unheld::UNHELD_ENTRIES.len();
    let mut i = 0;
    while i < FAMILIES.len() {
        count += FAMILIES[i].len();
        i += 1;
    }
    count
}

/// Number of declared IDs, including binding forms and typed refusals.
pub const DECLARATION_COUNT: usize = declaration_count();

/// A declaration, whether it has an attached kernel or a typed refusal.
#[derive(Clone, Copy, Debug)]
pub enum DeclarationRef {
    /// A worksheet declaration with an execution route.
    Worksheet(&'static WorksheetDeclaration),
    /// A declaration whose syntax, availability or execution ownership needs separate metadata.
    Other(&'static UnheldEntry),
}

impl DeclarationRef {
    /// Stable identity used to look up this declaration.
    pub const fn id(self) -> FunctionId {
        match self {
            Self::Worksheet(x) => x.id,
            Self::Other(x) => x.id,
        }
    }
    /// Invariant formula spelling.
    pub const fn spelling(self) -> &'static str {
        match self {
            Self::Worksheet(x) => x.spelling,
            Self::Other(x) => x.spelling,
        }
    }
    /// Formula notation of the entry.
    pub const fn notation(self) -> Notation {
        match self {
            Self::Worksheet(x) => x.notation,
            Self::Other(x) => x.notation,
        }
    }
    /// Execution owner and call kind.
    pub const fn kind(self) -> EntryKind {
        match self {
            Self::Worksheet(x) => x.kind,
            Self::Other(x) => match x.kind {
                UnheldKind::Worksheet => EntryKind::Worksheet,
                UnheldKind::Binding(form) => EntryKind::Binding(form),
                UnheldKind::Operator { file_spelling } => EntryKind::Operator { file_spelling },
                UnheldKind::Rtd { provider } => EntryKind::HostRoute {
                    provider,
                    route: HostRouteKind::Rtd,
                },
            },
        }
    }
    /// Availability observed in Excel.
    pub const fn availability(self) -> Availability {
        match self {
            Self::Worksheet(x) => x.availability,
            Self::Other(x) => x.availability,
        }
    }
    /// Supported argument counts.
    pub const fn arity(self) -> CallableArityShape {
        match self {
            Self::Worksheet(x) => x.arity,
            Self::Other(x) => x.arity,
        }
    }
    /// Repeating groups in the argument list, when declared.
    pub const fn groups(self) -> Option<ArgumentGroups> {
        match self {
            Self::Worksheet(x) => x.groups,
            Self::Other(x) => x.groups,
        }
    }
    /// Ordered parameter declarations.
    pub const fn parameters(self) -> &'static [Parameter] {
        match self {
            Self::Worksheet(x) => x.parameters,
            Self::Other(x) => x.parameters,
        }
    }
    /// The parameter declaration at zero-based call position `index`, repeating a declared
    /// group or the final parameter beyond the named slice. `None` when `index` exceeds the
    /// entry's maximum arity or the declaration has no parameters.
    pub const fn parameter(self, index: usize) -> Option<&'static Parameter> {
        if index >= self.arity().max {
            return None;
        }
        let parameters = self.parameters();
        if parameters.is_empty() {
            return None;
        }
        if index < parameters.len() {
            return Some(&parameters[index]);
        }
        if let Some(groups) = self.groups() {
            if let Some(named) = groups.parameter(index) {
                if named < parameters.len() {
                    return Some(&parameters[named]);
                }
            }
        }
        Some(&parameters[parameters.len() - 1])
    }
    /// Result form visible to formula admission.
    pub const fn result(self) -> ResultForm {
        match self {
            Self::Worksheet(x) => x.result,
            Self::Other(x) => x.result,
        }
    }
    /// Evaluation demand for the argument positions.
    pub const fn demand(self) -> ArgumentDemand {
        match self {
            Self::Worksheet(x) => x.demand,
            Self::Other(x) => x.demand,
        }
    }
    /// Call behavior shared by the evaluator and binder.
    pub const fn traits(self) -> EntryTraits {
        match self {
            Self::Worksheet(x) => x.traits,
            Self::Other(x) => x.traits,
        }
    }
    /// Translations are absent until a source records them for this entry.
    pub const fn localized_names(self) -> Option<&'static [crate::LocalizedDisplayName]> {
        match self {
            Self::Worksheet(x) => x.localized_names,
            Self::Other(x) => x.localized_names,
        }
    }
    /// An absent value means the complete service set has not been classified.
    pub const fn context_facilities(self) -> Option<&'static [crate::ContextFacility]> {
        match self {
            Self::Worksheet(x) => x.context_facilities,
            Self::Other(x) => x.context_facilities,
        }
    }
    /// Whether the classified facilities include the requested one.
    pub fn uses_context(self, facility: crate::ContextFacility) -> bool {
        self.context_facilities()
            .is_some_and(|facilities| facilities.contains(&facility))
    }
    /// How the entry is implemented or deferred.
    pub const fn implementation(self) -> Implementation {
        match self {
            Self::Worksheet(_) => Implementation::Held,
            Self::Other(x) => x.implementation,
        }
    }
    /// The measured prefix associated with the executable catalogue, when known.
    pub fn file_prefix(self) -> Option<crate::FilePrefix> {
        if self.notation() != Notation::Call {
            return None;
        }
        crate::executable_file_prefix(self.spelling())
    }
    /// The measured dynamic-array file positions, when known.
    pub fn file_form(self) -> Option<&'static crate::FunctionFileForm> {
        if self.notation() != Notation::Call {
            return None;
        }
        crate::file_form(crate::file_spelling(self.id()))
    }
    /// Excel's binary function index and fixed arity, when present in the Ftab measurements.
    pub fn xlsb_function(self) -> Option<(u16, i8)> {
        if self.notation() != Notation::Call {
            return None;
        }
        crate::EXCEL_FUNCTION_IDS
            .iter()
            .find(|(_, name, _)| *name == crate::file_spelling(self.id()))
            .map(|(index, _, arity)| (*index, *arity))
    }
}

/// Look up a declaration by its stable ID. Missing IDs stay missing.
pub const fn declaration(id: FunctionId) -> Option<DeclarationRef> {
    if id.family() == crate::unheld::CODE {
        let entries = crate::unheld::UNHELD_ENTRIES;
        let mut i = 0;
        while i < entries.len() {
            if entries[i].id.raw() == id.raw() {
                return Some(DeclarationRef::Other(&entries[i]));
            }
            i += 1;
        }
        return None;
    }
    let family = match id.family() {
        crate::ported::distributions::CODE => crate::ported::distributions::ENTRIES,
        crate::ported::bonds::CODE => crate::ported::bonds::ENTRIES,
        crate::ported::math::CODE => crate::ported::math::ENTRIES,
        crate::ported::lambda_helpers::CODE => crate::ported::lambda_helpers::ENTRIES,
        crate::ported::statistics::CODE => crate::ported::statistics::ENTRIES,
        crate::ported::grouping::CODE => crate::ported::grouping::ENTRIES,
        crate::ported::host_facts::CODE => crate::ported::host_facts::ENTRIES,
        crate::ported::financial::CODE => crate::ported::financial::ENTRIES,
        crate::ported::dates::CODE => crate::ported::dates::ENTRIES,
        crate::ported::rounding_more::CODE => crate::ported::rounding_more::ENTRIES,
        crate::ported::engineering::CODE => crate::ported::engineering::ENTRIES,
        crate::ported::database::CODE => crate::ported::database::ENTRIES,
        crate::ported::arrays::CODE => crate::ported::arrays::ENTRIES,
        crate::ported::subtotal::CODE => crate::ported::subtotal::ENTRIES,
        crate::ported::text_format::CODE => crate::ported::text_format::ENTRIES,
        crate::ported::text_functions::CODE => crate::ported::text_functions::ENTRIES,
        crate::ported::random::CODE => crate::ported::random::ENTRIES,
        crate::ported::cube_functions::CODE => crate::ported::cube_functions::ENTRIES,
        crate::ported::fetching::CODE => crate::ported::fetching::ENTRIES,
        crate::ported::importing::CODE => crate::ported::importing::ENTRIES,
        crate::operators::arithmetic::CODE => crate::operators::arithmetic::ENTRIES,
        crate::operators::comparison::CODE => crate::operators::comparison::ENTRIES,
        crate::operators::text::CODE => crate::operators::text::ENTRIES,
        crate::offset::CODE => crate::offset::ENTRIES,
        crate::aggregate::CODE => crate::aggregate::ENTRIES,
        crate::logical::CODE => crate::logical::ENTRIES,
        crate::logical_more::CODE => crate::logical_more::ENTRIES,
        crate::information::CODE => crate::information::ENTRIES,
        crate::rounding::CODE => crate::rounding::ENTRIES,
        crate::conditional::CODE => crate::conditional::ENTRIES,
        crate::lookup::CODE => crate::lookup::ENTRIES,
        crate::error_fallback::CODE => crate::error_fallback::ENTRIES,
        crate::euro_tools::CODE => crate::euro_tools::ENTRIES,
        crate::reference_text::CODE => crate::reference_text::ENTRIES,
        crate::group_pivot::CODE => crate::group_pivot::ENTRIES,
        _ => return None,
    };
    let mut i = 0;
    while i < family.len() {
        if family[i].id.raw() == id.raw() {
            return Some(DeclarationRef::Worksheet(&family[i]));
        }
        i += 1;
    }
    None
}

/// Every declared entry, including typed refusals and binding forms.
pub fn declarations() -> impl Iterator<Item = DeclarationRef> {
    FAMILIES
        .iter()
        .flat_map(|family| family.iter().map(DeclarationRef::Worksheet))
        .chain(
            crate::unheld::UNHELD_ENTRIES
                .iter()
                .map(DeclarationRef::Other),
        )
}

/// Look up an invariant worksheet spelling by exact, case-sensitive comparison.
pub fn named(spelling: &str) -> Option<DeclarationRef> {
    declarations().find(|entry| entry.notation() == Notation::Call && entry.spelling() == spelling)
}

#[cfg(test)]
mod tests {
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn id_lookup_retains_measured_binary_and_addin_file_metadata() {
        let sum = super::named("SUM").unwrap();
        assert_eq!(sum.xlsb_function(), Some((4, -1)));
        let addin = super::declaration(crate::euro_tools::EUROCONVERT).unwrap();
        assert_eq!(addin.file_form().unwrap().name, "EUROCONVERT");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn pass_week_state_names_the_week_functions() {
        let names: Vec<_> = super::declarations()
            .filter(|entry| entry.uses_context(crate::ContextFacility::WeekState))
            .map(super::DeclarationRef::spelling)
            .collect();
        assert_eq!(names, ["WEEKNUM", "ISOWEEKNUM"]);
    }
}
