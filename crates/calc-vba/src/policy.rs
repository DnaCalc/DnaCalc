//! The capability policy a VBA project runs under (CalcVBA `PolicyHost`).
//!
//! The policy is per thread, because a project runs on the thread that loaded it: set it before
//! loading a module or project, and every host built for that load is wrapped. `Safe` denies the
//! eight capabilities (files, processes, environment, settings, `Declare`, COM activation,
//! `SendKeys`/`AppActivate`) with the VBA errors 70, 53 and 429, `Full` lets them reach the host,
//! and both write an audit log that [`drain_audit`] returns per case. With no policy set (the
//! default of this crate, which the applications' policy setting replaces), nothing is wrapped
//! and behaviour is what it was before the policy existed.

use std::cell::RefCell;
use std::sync::Arc;

pub use calcvba_lib::{Prediction, PredictionSource};
pub use oxvba_hal::{
    AuditSnapshot, Capability, CapabilitySet, Decision, POLICY_DENIED_CODE, PolicyMode,
};
use oxvba_hal::{AuditHandle, HostServices, PolicyHost};

struct Active {
    mode: PolicyMode,
    audit: AuditHandle,
}

thread_local! {
    static ACTIVE: RefCell<Option<Active>> = const { RefCell::new(None) };
}

/// Choose the policy for projects loaded on this thread from now on. `None` removes it. The
/// audit log starts empty.
pub fn set_thread_policy(mode: Option<PolicyMode>) {
    ACTIVE.with(|active| {
        *active.borrow_mut() = mode.map(|mode| Active {
            mode,
            audit: AuditHandle::default(),
        });
    });
}

/// The policy in force on this thread.
pub fn thread_policy() -> Option<PolicyMode> {
    ACTIVE.with(|active| active.borrow().as_ref().map(|a| a.mode))
}

/// Take the requests logged on this thread since the last drain (empty with no policy set).
pub fn drain_audit() -> AuditSnapshot {
    ACTIVE.with(|active| {
        active
            .borrow()
            .as_ref()
            .map(|a| a.audit.drain())
            .unwrap_or_default()
    })
}

/// Denials on this thread since the policy was set, across drains: a session with any denial
/// is tainted, because module state left by a denied case can change later cases.
pub fn denials_ever() -> u64 {
    ACTIVE.with(|active| {
        active
            .borrow()
            .as_ref()
            .map_or(0, |a| a.audit.denials_ever())
    })
}

/// Wrap a host with the thread's policy, sharing the thread's audit log.
pub(crate) fn wrap(host: Arc<dyn HostServices>) -> Arc<dyn HostServices> {
    ACTIVE.with(|active| match active.borrow().as_ref() {
        Some(active) => PolicyHost::with_mode(host, active.mode)
            .with_host_progid_prefix("DnaCalc.")
            .with_audit(active.audit.clone())
            .into_host(),
        None => host,
    })
}

/// The capabilities a set of project sources can request, from a lexical scan: names of the
/// gated built-ins and `Declare` statements. A static over-approximation, used where no bound
/// program is at hand.
pub fn predict_lexical(sources: &[&str]) -> Prediction {
    calcvba_lib::predicted_capabilities_lexical(sources)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_policy_means_no_wrapper_and_an_empty_audit() {
        set_thread_policy(None);
        assert_eq!(thread_policy(), None);
        assert_eq!(drain_audit().total, 0);
    }
}
