//! Recorded capture losses, described by their three original observation phases.
//! These classifications never change a failure, an expectation or a cell verdict.
use crate::json::Out;
use crate::value::WorksheetErrorCode;
use crate::{CaptureFailure, CaptureRecord, CellAddress, CellObservation, Observation};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureFailureClass {
    OtherCaptureStage,
    IncompletePhaseEvidence,
    NonfiniteNumberSavedAndReopenedAsNum,
    FiniteNumberChangedDuringSave,
    FiniteNumberChangedOnReopen,
    FiniteNumberChangedDuringSaveAndReopen,
    OtherObservedValueChange,
    NoObservedValueChange,
}

impl CaptureFailureClass {
    pub fn name(self) -> &'static str {
        match self {
            Self::OtherCaptureStage => "other_capture_stage",
            Self::IncompletePhaseEvidence => "incomplete_phase_evidence",
            Self::NonfiniteNumberSavedAndReopenedAsNum => {
                "nonfinite_number_saved_and_reopened_as_num"
            }
            Self::FiniteNumberChangedDuringSave => "finite_number_changed_during_save",
            Self::FiniteNumberChangedOnReopen => "finite_number_changed_on_reopen",
            Self::FiniteNumberChangedDuringSaveAndReopen => {
                "finite_number_changed_during_save_and_reopen"
            }
            Self::OtherObservedValueChange => "other_observed_value_change",
            Self::NoObservedValueChange => "no_observed_value_change",
        }
    }
}

/// An original recorded failure and its available phase evidence. This describes
/// observed transitions; it does not infer why Excel changed a value.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptureFailureEvidence {
    pub failure: CaptureFailure,
    pub classification: CaptureFailureClass,
    pub immediate: Option<Observation>,
    pub saved: Option<Observation>,
    pub reopened: Option<Observation>,
}

fn classify(evidence: &CaptureFailureEvidence) -> CaptureFailureClass {
    use CaptureFailureClass as C;
    if evidence.failure.stage != "reopen" {
        return C::OtherCaptureStage;
    }
    let (
        Some(Observation::ExcelRaw(before)),
        Some(Observation::ExcelSavedCache(saved)),
        Some(Observation::ExcelRaw(after)),
    ) = (&evidence.immediate, &evidence.saved, &evidence.reopened)
    else {
        return C::IncompletePhaseEvidence;
    };
    let saved = saved.value();
    if before.as_number().is_some_and(|number| !number.is_finite())
        && saved.as_error() == Some(WorksheetErrorCode::Num)
        && after.as_error() == Some(WorksheetErrorCode::Num)
    {
        return C::NonfiniteNumberSavedAndReopenedAsNum;
    }
    if let (Some(before), Some(saved), Some(after)) =
        (before.as_number(), saved.as_number(), after.as_number())
        && before.is_finite()
        && saved.is_finite()
        && after.is_finite()
    {
        return match (
            before.to_bits() != saved.to_bits(),
            saved.to_bits() != after.to_bits(),
        ) {
            (true, true) => C::FiniteNumberChangedDuringSaveAndReopen,
            (true, false) => C::FiniteNumberChangedDuringSave,
            (false, true) => C::FiniteNumberChangedOnReopen,
            (false, false) => C::NoObservedValueChange,
        };
    }
    if crate::compare(before, saved).is_none() && crate::compare(saved, after).is_none() {
        C::NoObservedValueChange
    } else {
        C::OtherObservedValueChange
    }
}

impl CaptureRecord {
    /// Retain every session failure, in recorded order, with available immediate,
    /// saved and reopened readings. Missing evidence stays explicit. No capture is
    /// edited and no classification grants calculation proof or certification.
    pub fn failure_evidence(&self) -> Vec<CaptureFailureEvidence> {
        let Some(session) = self.session.as_ref().filter(|s| !s.failures.is_empty()) else {
            return Vec::new();
        };
        let immediate = unique_observations(&self.observations);
        let saved = unique_observations(&session.saved);
        let reopened = unique_observations(&session.reopened);
        session
            .failures
            .iter()
            .map(|failure| {
                let mut evidence = CaptureFailureEvidence {
                    failure: failure.clone(),
                    classification: CaptureFailureClass::IncompletePhaseEvidence,
                    immediate: failure
                        .cell
                        .as_ref()
                        .and_then(|at| immediate.get(at))
                        .and_then(|o| *o)
                        .cloned(),
                    saved: failure
                        .cell
                        .as_ref()
                        .and_then(|at| saved.get(at))
                        .and_then(|o| *o)
                        .cloned(),
                    reopened: failure
                        .cell
                        .as_ref()
                        .and_then(|at| reopened.get(at))
                        .and_then(|o| *o)
                        .cloned(),
                };
                evidence.classification = classify(&evidence);
                evidence
            })
            .collect()
    }
}

fn unique_observations(
    observations: &[CellObservation],
) -> BTreeMap<&CellAddress, Option<&Observation>> {
    let mut unique = BTreeMap::new();
    for observation in observations {
        // judge refuses duplicates. Standalone evidence inspection must not choose
        // one of two conflicting readings either.
        unique
            .entry(&observation.cell)
            .and_modify(|held| *held = None)
            .or_insert(Some(&observation.observation));
    }
    unique
}

impl CaptureFailureEvidence {
    pub(crate) fn out(&self) -> Out {
        let mut members = vec![("stage", Out::text(&self.failure.stage))];
        if let Some(cell) = &self.failure.cell {
            members.extend(cell.out_members());
        }
        members.extend([
            ("message", Out::text(&self.failure.message)),
            ("classification", Out::text(self.classification.name())),
        ]);
        for (name, observation) in [
            ("immediate", &self.immediate),
            ("saved", &self.saved),
            ("reopened", &self.reopened),
        ] {
            let value = observation
                .as_ref()
                .map_or(Out::Null, |o| Out::Object(vec![(o.origin(), o.out())]));
            members.push((name, value));
        }
        Out::Object(members)
    }
}
