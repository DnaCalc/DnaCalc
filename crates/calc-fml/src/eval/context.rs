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
    /// Date system used to interpret and produce workbook serial dates.
    pub date_system: DateSystem,
    /// Excel compatibility version used by version-sensitive formula operations.
    pub compatibility_version: u32,
    /// Workbook locale and formatting rules, when supplied by the caller.
    pub profile: Option<FormatProfile>,
}

impl SemanticEnvironment {
    /// Combines these stable settings with the clock snapshot for a calculation.
    pub fn at(self, clock: Option<f64>) -> CalcEnvironment {
        CalcEnvironment {
            date_system: self.date_system,
            compatibility_version: self.compatibility_version,
            profile: self.profile,
            now: clock,
        }
    }
}

impl From<CalcEnvironment> for SemanticEnvironment {
    fn from(value: CalcEnvironment) -> Self {
        Self {
            date_system: value.date_system,
            compatibility_version: value.compatibility_version,
            profile: value.profile,
        }
    }
}

/// Services fixed for one pass; a physical retry reuses them.
pub struct PassServices<'s> {
    /// The pass clock as an Excel serial date, when a clock was supplied.
    pub clock: Option<f64>,
    /// The caller-owned random stream for volatile functions.
    pub random: Option<&'s mut dyn RandomSource>,
    /// The pass's WEEKNUM state (dc-amhpx), given only where the evaluation knows it.
    pub week: Option<&'s mut dyn calc_func::WeekState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// Authority and freshness requirements for a read.
pub enum ReadPolicy {
    /// A dependency read under the active calculation command.
    DependencyWithinCommand,
    /// An observation of published values by a macro.
    CachedMacroObservation,
    /// A read on behalf of a registered worksheet function.
    RegisteredWorksheetCallback,
    /// An inspection of the supplied snapshot for diagnostics.
    DiagnosticSnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// The semantic reason an evaluator or host asks for data.
pub enum ReadPurpose {
    /// The value consumed by a formula.
    FormulaValue,
    /// The authored text of a formula.
    FormulaText,
    /// Whether a cell holds a formula and how it is stored.
    FormulaKind,
    /// The bounds or shape of a reference.
    ReferenceShape,
    /// Information about a worksheet.
    SheetMetadata,
    /// A diagnostic observation of the supplied snapshot.
    Diagnostic,
}

/// Facets validated independently of the displayed number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObservableFacets {
    /// Published or in-flight calculated value, according to the read policy.
    Value,
    /// Authored formula text.
    FormulaText,
    /// Cell formula classification.
    FormulaKind,
    /// Dimensions or extent of a reference.
    Shape,
    /// Worksheet properties or identity.
    SheetMetadata,
    /// Whether a destination cell is occupied.
    Occupancy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// The reason, authority, and observable aspect of a workbook read.
pub struct ReadRequest {
    /// Why the caller is reading.
    pub purpose: ReadPurpose,
    /// Which values or observations the reader may supply.
    pub policy: ReadPolicy,
    /// Which independently tracked aspect of the subject is observed.
    pub facets: ObservableFacets,
}

impl ReadRequest {
    /// An ordinary calculation dependency on another cell value.
    pub const DEPENDENCY: Self = Self {
        purpose: ReadPurpose::FormulaValue,
        policy: ReadPolicy::DependencyWithinCommand,
        facets: ObservableFacets::Value,
    };
    /// A diagnostic observation of the snapshot value.
    pub const DIAGNOSTIC: Self = Self {
        purpose: ReadPurpose::Diagnostic,
        policy: ReadPolicy::DiagnosticSnapshot,
        facets: ObservableFacets::Value,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// When an observed fact may invalidate an evaluation.
pub enum FactInvalidation {
    /// Invalidate when the observed fact changes.
    OnChange,
    /// Revisit the fact when another calculation is authorized.
    OnNextAuthorizedCalculation,
    /// Record the observation without invalidating the evaluation.
    ObservationOnly,
}

/// One formula evaluation context, owned by calc-fml. All service choices are made at this
/// boundary. `None` is an explicit absent service, never a reader fallback. The calling cell
/// belongs to the formula instance, so a scenario cannot accidentally substitute its caller.
pub struct FormulaEvaluationContext<'s> {
    pub(super) reference_result: bool,
    /// The formula is placed in its anchor cell and spills from it (a worksheet cell's formula),
    /// so a range result holding that cell is checked for spill room before it is read.
    pub(super) placed: bool,
    pub(super) workbookless: bool,
    pub(super) numeric: &'s NumericMode,
    pub(super) reader: &'s mut dyn ReferenceReader,
    pub(super) environment: Option<SemanticEnvironment>,
    pub(super) host: Option<&'s mut dyn HostFacts>,
    pub(super) pass: PassServices<'s>,
    pub(super) request: ReadRequest,
    pub(super) observer: Option<&'s mut dyn EvaluationObserver>,
    pub(super) conditional: Option<&'s mut dyn calc_func::ConditionalObserver>,
    pub(super) catalog: Option<&'s dyn ReferenceCatalog>,
    pub(super) functions: Option<&'s crate::CallableTable>,
    pub(super) invocation: Option<calc_func::UserInvocationFrame>,
    pub(super) external_inputs: Option<&'s dyn calc_func::RtdCut>,
    pub(super) rtd_connector: Option<&'s dyn calc_func::RtdConnector>,
    pub(super) user_view: Option<calc_func::UserView>,
    pub(super) reference_handles: Option<&'s dyn calc_func::ReferenceHandles>,
    pub(super) observation_reader: Option<&'s mut dyn ReferenceReader>,
    pub(super) function_diagnostics: Option<&'s mut Vec<String>>,
    pub(super) user_function_effects: Option<&'s mut UserFunctionEffects>,
}

impl<'s> FormulaEvaluationContext<'s> {
    /// Creates a value evaluation with the reader, numeric mode and initial optional services.
    pub fn new(
        numeric: &'s NumericMode,
        reader: &'s mut dyn ReferenceReader,
        environment: Option<CalcEnvironment>,
        host: Option<&'s mut dyn HostFacts>,
        request: ReadRequest,
    ) -> Self {
        Self {
            reference_result: false,
            placed: false,
            workbookless: false,
            numeric,
            reader,
            environment: environment.map(Into::into),
            host,
            request,
            pass: PassServices {
                clock: environment.and_then(|e| e.now),
                random: None,
                week: None,
            },
            observer: None,
            conditional: None,
            catalog: None,
            functions: None,
            invocation: None,
            external_inputs: None,
            rtd_connector: None,
            user_view: None,
            reference_handles: None,
            observation_reader: None,
            function_diagnostics: None,
            user_function_effects: None,
        }
    }

    /// Requests the criteria verdicts produced by this evaluation's conditional aggregate.
    pub fn with_conditional_observer(mut self, observer: &'s mut dyn calc_func::ConditionalObserver) -> Self {
        self.conditional = Some(observer);
        self
    }

    /// Explicit name inspection asks for the reference or value the expression produced,
    /// before cell-result intersection, dereferencing or blank-to-zero conversion. Reads
    /// inside the expression still use the same reader, services and evaluator.
    pub fn with_reference_result(mut self) -> Self {
        self.reference_result = true;
        self
    }

    /// The formula is a worksheet cell's, placed at its anchor and spilling from it.
    pub fn placed_in_cell(mut self) -> Self {
        self.placed = true;
        self
    }

    /// Keeps the internal compilation anchor out of workbook references and host facts.
    pub fn without_workbook(mut self) -> Self {
        self.workbookless = true;
        self
    }

    /// Supplies the external RTD values visible to this evaluation.
    pub fn with_external_inputs(mut self, inputs: &'s dyn calc_func::RtdCut) -> Self {
        self.external_inputs = Some(inputs);
        self
    }
    /// The owner's capability to join or start an RTD topic during this evaluation.
    pub fn with_rtd_connector(mut self, connector: &'s dyn calc_func::RtdConnector) -> Self {
        self.rtd_connector = Some(connector);
        self
    }
    /// The sheet and selection the person has active, which VBA's `ActiveSheet` and `Selection`
    /// answer (a host that models neither leaves them out, and the calls are typed refusals).
    pub fn with_user_view(mut self, view: calc_func::UserView) -> Self {
        self.user_view = Some(view);
        self
    }
    /// Supplies the service that retains and resolves references for callable adapters.
    pub fn with_reference_handles(mut self, handles: &'s dyn calc_func::ReferenceHandles) -> Self {
        self.reference_handles = Some(handles);
        self
    }
    /// Carries the invocation identity of the calling user-function frame.
    pub fn with_invocation(mut self, invocation: calc_func::UserInvocationFrame) -> Self {
        self.invocation = Some(invocation);
        self
    }
    /// Supplies the reader used for observation requests from worksheet callbacks.
    pub fn with_observation_reader(mut self, reader: &'s mut dyn ReferenceReader) -> Self {
        self.observation_reader = Some(reader);
        self
    }
    /// Supplies the same callable registrations used to bind the compiled formula.
    pub fn with_functions(mut self, functions: &'s crate::CallableTable) -> Self {
        self.functions = Some(functions);
        self
    }
    /// Collects diagnostics emitted by user-function providers during evaluation.
    pub fn with_function_diagnostics(mut self, diagnostics: &'s mut Vec<String>) -> Self {
        self.function_diagnostics = Some(diagnostics);
        self
    }
    /// Collects out-of-band effects emitted by user-function providers.
    pub fn with_user_function_effects(mut self, effects: &'s mut UserFunctionEffects) -> Self {
        self.user_function_effects = Some(effects);
        self
    }
    /// These services with the pass's WEEKNUM state ([`calc_func::WeekState`]).
    pub fn with_week_state(mut self, week: &'s mut dyn calc_func::WeekState) -> Self {
        self.pass.week = Some(week);
        self
    }

    /// Supplies the random stream for RAND and related functions.
    pub fn with_random(mut self, random: &'s mut dyn RandomSource) -> Self {
        self.pass.random = Some(random);
        self
    }
    /// Supplies the workbook namespace used by run-time reference text such as INDIRECT.
    pub fn with_catalog(mut self, catalog: &'s dyn ReferenceCatalog) -> Self {
        self.catalog = Some(catalog);
        self
    }
    /// Receives evaluation events through the supplied observer.
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
    /// One worksheet cell.
    Cell(calc_func::value::CellKey),
    /// Every cell of an area, asked as one question (ISFORMULA or FORMULATEXT of a range).
    Area(calc_func::value::GridArea),
    /// One worksheet.
    Sheet(calc_func::value::SheetId),
    /// The workbook as a whole.
    Workbook,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// An observation of a workbook fact and its invalidation policy.
pub struct FactRead {
    /// Read intent and authorization for the observation.
    pub request: ReadRequest,
    /// Workbook entity whose fact was observed.
    pub subject: FactSubject,
    /// When a change to the observed fact affects recalculation.
    pub invalidation: FactInvalidation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// A typed refusal from the reference reader.
pub struct ReadDenial {
    /// Construct the reader cannot provide under the current policy.
    pub construct: &'static str,
    /// Work package responsible for the unavailable capability.
    pub owner: &'static str,
}
