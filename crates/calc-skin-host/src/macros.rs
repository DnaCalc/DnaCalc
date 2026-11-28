use crate::{Connection, State, invalid};
use calc_host::portability::HostKind;
use calc_host_vba::{Outcome, ProjectGrant, WorkbookEvents};
use calc_skin::{
    MacroDenial, MacroReview, MacroRun, MacroSecurity, Origin, Refusal, SelectionCause, Stamp,
};
use std::rc::Rc;

#[derive(Default)]
pub(super) struct MacroSession {
    mode: MacroSecurity,
    project: Option<Rc<WorkbookEvents>>,
    epoch: u64,
}

impl MacroSession {
    pub(super) fn reset_project(&mut self) {
        self.project = None;
        self.epoch += 1;
    }
}

struct ApprovedProcedure {
    id: u64,
    stamp: Stamp,
    epoch: u64,
    procedure: String,
}

#[derive(Default)]
pub(super) struct PendingReview {
    next: u64,
    current: Option<ApprovedProcedure>,
}

fn modes(state: &State) -> Vec<MacroSecurity> {
    match state.host_profile.kind() {
        HostKind::WebWasm | HostKind::WebHosted | HostKind::Server | HostKind::Wasi => {
            vec![MacroSecurity::Off, MacroSecurity::Guarded]
        }
        _ => MacroSecurity::ALL.to_vec(),
    }
}

impl Connection {
    pub(super) fn choose_macro_mode(
        &self,
        state: &mut State,
        mode: MacroSecurity,
    ) -> Result<(), Refusal> {
        self.check_macro_operator(state)?;
        if !modes(state).contains(&mode) {
            return Err(invalid("Unguarded VBA is unavailable on this host."));
        }
        if state.macros.mode != mode {
            state.macros.mode = mode;
            state.macros.reset_project();
        }
        Ok(())
    }

    fn check_macro_operator(&self, state: &State) -> Result<(), Refusal> {
        if !matches!(self.origin, Origin::User { .. }) {
            return Err(Refusal::PolicyRefused);
        }
        self.check_write(state)
    }

    pub(super) fn review_macro(&self, procedure: Option<String>) -> Result<MacroReview, Refusal> {
        let state = self.host.borrow();
        if !matches!(self.origin, Origin::User { .. }) {
            return Err(Refusal::PolicyRefused);
        }
        let mut local = self.local.borrow_mut();
        let pending = &mut local.macro_review;
        pending.current = None;
        let mode = state.macros.mode;
        let reach = match mode {
            MacroSecurity::Off => vec!["VBA execution is disabled.".into()],
            MacroSecurity::Guarded => vec![
                "Workbook data and supported Excel object-model operations".into(),
                "VBA file, process, environment, settings, native calls, keyboard and outside COM requests are subject to the Guarded runtime policy.".into(),
            ],
            MacroSecurity::Unguarded => vec![
                "Workbook data and supported Excel object-model operations".into(),
                "Available host services under your account, including outside effects".into(),
            ],
        };
        let mut review = MacroReview {
            mode,
            available_modes: modes(&state),
            procedure: procedure.clone(),
            module_source: None,
            reach,
            grants: Vec::new(),
            blocked: None,
            approval: None,
        };
        let Some(procedure) = procedure else {
            return Ok(review);
        };
        let (module, _) = procedure
            .split_once('.')
            .filter(|(module, name)| !module.is_empty() && !name.is_empty() && !name.contains('.'))
            .ok_or_else(|| invalid("Enter a procedure as Module.Procedure."))?;
        let host = state.session.host();
        review.module_source = calc_host_vba::file_module_source(host, module)
            .map_err(|error| invalid(error.to_string()))?;
        if review
            .module_source
            .as_ref()
            .is_some_and(|source| source.len() > 256 * 1024)
        {
            return Err(invalid(
                "This module exceeds the macro review source budget.",
            ));
        }
        review.blocked = if mode == MacroSecurity::Off {
            Some("Macro security is Off.".into())
        } else if state.read_only {
            Some("This workbook is read-only.".into())
        } else if review.module_source.is_none() {
            Some(format!("Module {module} is unavailable in this workbook."))
        } else {
            None
        };
        if review.blocked.is_none() {
            self.check_macro_operator(&state)?;
            pending.next += 1;
            pending.current = Some(ApprovedProcedure {
                id: pending.next,
                stamp: state.stamp(),
                epoch: state.macros.epoch,
                procedure,
            });
            review.approval = Some(pending.next);
        }
        Ok(review)
    }

    pub(super) fn run_macro(&self, state: &mut State, id: u64) -> Result<MacroRun, Refusal> {
        self.check_macro_operator(state)?;
        let before = state.stamp();
        let approval = {
            let mut local = self.local.borrow_mut();
            let pending = &mut local.macro_review.current;
            let Some(review) = pending.as_ref().filter(|review| review.id == id) else {
                return Err(Refusal::PolicyRefused);
            };
            if review.epoch != state.macros.epoch
                || review.stamp.workbook != before.workbook
                || review.stamp.model != before.model
                || review.stamp.view != before.view
            {
                *pending = None;
                return Err(invalid(
                    "The workbook or macro policy changed. Review the macro again.",
                ));
            }
            pending.take().expect("the approval was present")
        };
        let policy = state
            .macros
            .mode
            .vba_policy()
            .ok_or(Refusal::PolicyRefused)?;
        let project = if let Some(project) = &state.macros.project {
            project.clone()
        } else {
            let host = state.session.host();
            let name = host
                .model()
                .definitions()
                .workbook_name()
                .unwrap_or("Book1");
            let grant = ProjectGrant::trust_all("operator-approved macro invocation", policy);
            let project = calc_host_vba::prepare_file_project_with_events(
                &grant, host, name, 150_000, None, false,
            )
            .map_err(|error| invalid(error.to_string()))?
            .ok_or_else(|| invalid("This workbook has no VBA project."))?;
            state.macros.project = Some(project.clone());
            project
        };
        let result = project.run_procedure(&mut state.session, &approval.procedure, &[]);
        // An output needs a carrier that can display and deliver the file. No export is
        // delivered by the skin carrier as a side effect of confirming a macro review.
        let outputs = project.take_output_files();
        if &state.selection != state.session.selection() {
            state.update_selection(SelectionCause::Macro {
                procedure: result.entry.clone(),
            });
        }
        state.record_touch(before, None, Vec::new());
        state.record_calculation(before, None);
        let error = match result.outcome {
            Outcome::Success(_) if outputs.is_empty() => None,
            Outcome::Success(_) => {
                Some(crate::unavailable("dc-6gb1m", "macro output file delivery"))
            }
            Outcome::RuntimeError {
                number,
                description,
                ..
            } => Some(Refusal::Excel {
                reason: format!("VBA error {number} · {description}"),
            }),
            Outcome::Refused { member, owner, .. } => Some(Refusal::NotModelled {
                owner,
                what: member,
            }),
            Outcome::NotAvailableHere { member, capability } => Some(crate::unavailable(
                "dc-6gb1m",
                &format!("{member} with {}", capability.key()),
            )),
            Outcome::NotCallable(reason) | Outcome::Failed(reason) => Some(invalid(reason)),
        };
        Ok(MacroRun {
            stamp: state.stamp(),
            procedure: result.entry,
            denials: result
                .denials
                .into_iter()
                .map(|denial| MacroDenial {
                    capability: denial.capability,
                    subject: denial.subject,
                })
                .collect(),
            error,
        })
    }
}
