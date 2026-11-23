//! Capability reach for Automation members. The caller's host supplies its level and audit sink.

use crate::{AutoError, Call, descriptor::Capability};
use std::rc::Rc;

pub use calc_macro_security::MacroSecurity;

/// One decision per reaching call, including all of that member's declared capabilities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyAudit {
    pub subject: String,
    pub reach: Vec<Capability>,
    pub level: MacroSecurity,
    pub allowed: bool,
    pub error_number: Option<i32>,
}

#[derive(Clone)]
pub struct PolicyChecker {
    level: Rc<dyn Fn() -> MacroSecurity>,
    audit: Rc<dyn Fn(PolicyAudit)>,
}

impl Default for PolicyChecker {
    /// Existing trusted in-process callers retain their access. Hosts running restricted
    /// code supply its level and audit sink when they construct the profile.
    fn default() -> Self {
        Self::new(|| MacroSecurity::Unguarded, |_| {})
    }
}

impl PolicyChecker {
    pub fn new(
        level: impl Fn() -> MacroSecurity + 'static,
        audit: impl Fn(PolicyAudit) + 'static,
    ) -> Self {
        Self {
            level: Rc::new(level),
            audit: Rc::new(audit),
        }
    }

    pub fn check(&self, call: &Call<'_>) -> Result<(), AutoError> {
        let reach = &call.member.reach;
        let Some(first) = reach.first() else {
            return Ok(());
        };
        let subject = call.label();
        let level = (self.level)();
        let allowed = level == MacroSecurity::Unguarded;
        let error_number = (!allowed).then_some(70);
        (self.audit)(PolicyAudit {
            subject: subject.clone(),
            reach: reach.clone(),
            level,
            allowed,
            error_number,
        });
        match error_number {
            None => Ok(()),
            Some(number) => Err(AutoError::PolicyDenied {
                capability: first.key(),
                subject,
                number,
            }),
        }
    }
}
