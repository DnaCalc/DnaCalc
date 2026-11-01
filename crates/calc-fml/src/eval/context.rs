//! The complete formula boundary. Value overlays replace the reader, while the environment,
//! host questions and pass services remain explicit. Nothing is recovered from a cell reader.

use super::{EvaluationObserver, UserFunctionEffects};
use crate::{
    CalcEnvironment, DateSystem, FormatProfile, HostFacts, RandomSource, ReferenceCatalog,
    ReferenceReader,
};
use calc_func::value::NumericMode;
use std::fmt;

/// Stable semantics. Advancing time does not change this identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticEnvironment {
    pub date_system: DateSystem,
    pub profile: Option<FormatProfile>,
}

impl SemanticEnvironment {
    pub fn at(self, clock: Option<f64>) -> CalcEnvironment {
        CalcEnvironment {
            date_system: self.date_system,
            profile: self.profile,
            now: clock,
        }
    }
}

impl From<CalcEnvironment> for SemanticEnvironment {
    fn from(value: CalcEnvironment) -> Self {
        Self {
            date_system: value.date_system,
            profile: value.profile,
        }
    }
}

/// Services fixed for one pass; a physical retry reuses them.
pub struct PassServices<'s> {
    pub clock: Option<f64>,
    pub random: Option<&'s mut dyn RandomSource>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReadPolicy {
    DependencyWithinCommand,
    CachedMacroObservation,
    RegisteredWorksheetCallback,
    DiagnosticSnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReadPurpose {
    FormulaValue,
    FormulaText,
    FormulaKind,
    ReferenceShape,
    SheetMetadata,
    Diagnostic,
}

/// Facets validated independently of the displayed number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObservableFacets {
    Value,
    FormulaText,
    FormulaKind,
    Shape,
    SheetMetadata,
    Occupancy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ReadRequest {
    pub purpose: ReadPurpose,
    pub policy: ReadPolicy,
    pub facets: ObservableFacets,
}

impl ReadRequest {
    pub const DEPENDENCY: Self = Self {
        purpose: ReadPurpose::FormulaValue,
        policy: ReadPolicy::DependencyWithinCommand,
        facets: ObservableFacets::Value,
    };
    pub const DIAGNOSTIC: Self = Self {
        purpose: ReadPurpose::Diagnostic,
        policy: ReadPolicy::DiagnosticSnapshot,
        facets: ObservableFacets::Value,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FactInvalidation {
    OnChange,
    OnNextAuthorizedCalculation,
    ObservationOnly,
}

/// One formula evaluation context, owned by calc-fml. All service choices are made at this
/// boundary. `None` is an explicit absent service, never a reader fallback. The calling cell
/// belongs to the formula instance, so a scenario cannot accidentally substitute its caller.
pub struct FormulaEvaluationContext<'s> {
    pub(super) numeric: &'s NumericMode,
    pub(super) reader: &'s mut dyn ReferenceReader,
    pub(super) environment: Option<SemanticEnvironment>,
    pub(super) host: Option<&'s mut dyn HostFacts>,
    pub(super) pass: PassServices<'s>,
    pub(super) request: ReadRequest,
    pub(super) observer: Option<&'s mut dyn EvaluationObserver>,
    pub(super) catalog: Option<&'s dyn ReferenceCatalog>,
    pub(super) functions: Option<&'s crate::CallableTable>,
    pub(super) invocation: Option<calc_func::UserInvocationFrame>,
    pub(super) external_inputs: Option<&'s dyn calc_func::UserExternalInputs>,
    pub(super) reference_handles: Option<&'s dyn calc_func::ReferenceHandles>,
    pub(super) observation_reader: Option<&'s mut dyn ReferenceReader>,
    pub(super) function_diagnostics: Option<&'s mut Vec<String>>,
    pub(super) user_function_effects: Option<&'s mut UserFunctionEffects>,
}

impl<'s> FormulaEvaluationContext<'s> {
    pub fn new(
        numeric: &'s NumericMode,
        reader: &'s mut dyn ReferenceReader,
        environment: Option<CalcEnvironment>,
        host: Option<&'s mut dyn HostFacts>,
        request: ReadRequest,
    ) -> Self {
        Self {
            numeric,
            reader,
            environment: environment.map(Into::into),
            host,
            request,
            pass: PassServices {
                clock: environment.and_then(|e| e.now),
                random: None,
            },
            observer: None,
            catalog: None,
            functions: None,
            invocation: None,
            external_inputs: None,
            reference_handles: None,
            observation_reader: None,
            function_diagnostics: None,
            user_function_effects: None,
        }
    }

    pub fn with_external_inputs(mut self, inputs: &'s dyn calc_func::UserExternalInputs) -> Self {
        self.external_inputs = Some(inputs);
        self
    }
    pub fn with_reference_handles(mut self, handles: &'s dyn calc_func::ReferenceHandles) -> Self {
        self.reference_handles = Some(handles);
        self
    }
    pub fn with_invocation(mut self, invocation: calc_func::UserInvocationFrame) -> Self {
        self.invocation = Some(invocation);
        self
    }
    pub fn with_observation_reader(mut self, reader: &'s mut dyn ReferenceReader) -> Self {
        self.observation_reader = Some(reader);
        self
    }
    pub fn with_functions(mut self, functions: &'s crate::CallableTable) -> Self {
        self.functions = Some(functions);
        self
    }
    pub fn with_function_diagnostics(mut self, diagnostics: &'s mut Vec<String>) -> Self {
        self.function_diagnostics = Some(diagnostics);
        self
    }
    pub fn with_user_function_effects(mut self, effects: &'s mut UserFunctionEffects) -> Self {
        self.user_function_effects = Some(effects);
        self
    }
    pub fn with_random(mut self, random: &'s mut dyn RandomSource) -> Self {
        self.pass.random = Some(random);
        self
    }
    pub fn with_catalog(mut self, catalog: &'s dyn ReferenceCatalog) -> Self {
        self.catalog = Some(catalog);
        self
    }
    pub fn observed_by(mut self, observer: &'s mut dyn EvaluationObserver) -> Self {
        self.observer = Some(observer);
        self
    }
}

impl fmt::Debug for FormulaEvaluationContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FormulaEvaluationContext")
            .field("numeric", self.numeric)
            .field("environment", &self.environment)
            .field("request", &self.request)
            .field("host", &self.host.is_some())
            .finish_non_exhaustive()
    }
}

/// A host question, kept apart from reading its cell's numeric value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FactSubject {
    Cell(calc_func::value::CellKey),
    Sheet(calc_func::value::SheetId),
    Workbook,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FactRead {
    pub request: ReadRequest,
    pub subject: FactSubject,
    pub invalidation: FactInvalidation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadDenial {
    pub construct: &'static str,
    pub owner: &'static str,
}
