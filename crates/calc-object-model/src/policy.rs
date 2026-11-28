//! Capability reach for Automation members. The caller's host supplies its level and audit sink.

use crate::{
    AutoError, Call,
    descriptor::{Capability, Need},
};
use std::rc::Rc;

pub use calc_macro_security::MacroSecurity;

/// How a host supplies a facility, ordered from absent to native.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Provision {
    /// The host cannot supply the facility.
    Reject,
    /// Access requires platform or user permission.
    Mediated,
    /// A portable implementation supplies the facility.
    Replace,
    /// An adapter emulates the Excel facility.
    Shim,
    /// The host supplies a value or UI response.
    Host,
    /// The workbook model supplies the facility.
    Model,
    /// The host supplies the facility directly.
    Native,
}

/// The first facility a call needs that the host cannot supply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unavailable {
    /// The first unavailable facility.
    pub capability: Need,
    /// An alternative host when one is known.
    pub route: Option<&'static str>,
}

/// Check availability without consulting the caller's authority.
pub fn check_availability(
    needs: &[Need],
    provision: impl Fn(Need) -> Provision,
) -> Result<(), Unavailable> {
    let Some(capability) = needs
        .iter()
        .copied()
        .find(|need| provision(*need) == Provision::Reject)
    else {
        return Ok(());
    };
    let route = match capability {
        Need::Dll | Need::Xll | Need::OsCom | Need::Process | Need::Keys => {
            Some("DNA Calc on Windows")
        }
        Need::Dylib => Some("DNA Calc on macOS"),
        Need::Dialog | Need::Forms | Need::FileDialog | Need::Print => Some("DNA Calc desktop"),
        _ => None,
    };
    Err(Unavailable { capability, route })
}

/// One decision per reaching or unavailable call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyAudit {
    /// Descriptor member named as `Interface.Member`.
    pub subject: String,
    /// All capabilities declared for the member.
    pub reach: Vec<Capability>,
    /// Execution level used for this decision.
    pub level: MacroSecurity,
    /// Whether the policy allowed the call.
    pub allowed: bool,
    /// Automation error number for a denial, when present.
    pub error_number: Option<i32>,
    /// Host facility that prevented the call from reaching the policy decision.
    pub unavailable: Option<Unavailable>,
}

/// Checks an Automation call against the execution level supplied by its host.
#[derive(Clone)]
pub struct PolicyChecker {
    level: Rc<dyn Fn() -> MacroSecurity>,
    audit: Rc<dyn Fn(PolicyAudit)>,
    provision: Option<Rc<dyn Fn(Need) -> Provision>>,
}

impl Default for PolicyChecker {
    /// Existing trusted in-process callers retain their access. Hosts running restricted
    /// code supply its level and audit sink when they construct the profile.
    fn default() -> Self {
        Self::new(|| MacroSecurity::Unguarded, |_| {})
    }
}

impl PolicyChecker {
    /// Use the supplied execution-level lookup and audit callback for each call that declares
    /// outside capabilities.
    pub fn new(
        level: impl Fn() -> MacroSecurity + 'static,
        audit: impl Fn(PolicyAudit) + 'static,
    ) -> Self {
        Self {
            level: Rc::new(level),
            audit: Rc::new(audit),
            provision: None,
        }
    }

    /// Use an embedder's provisions before checking authority.
    pub fn with_provisions(mut self, provision: impl Fn(Need) -> Provision + 'static) -> Self {
        self.provision = Some(Rc::new(provision));
        self
    }

    /// Install the host's default provisions when the embedder supplied no overrides.
    pub fn with_default_provisions(
        mut self,
        provision: impl Fn(Need) -> Provision + 'static,
    ) -> Self {
        if self.provision.is_none() {
            self.provision = Some(Rc::new(provision));
        }
        self
    }

    /// Check the member's required facilities before consulting execution policy.
    pub fn check(&self, call: &Call<'_>) -> Result<(), AutoError> {
        let reach = &call.member.reach;
        if let Some(provision) = &self.provision
            && let Err(unavailable) = check_availability(&call.member.needs, |need| provision(need))
        {
            let subject = call.label();
            let error = AutoError::NotAvailableHere {
                capability: unavailable.capability,
                route: unavailable.route,
                subject: subject.clone(),
            };
            (self.audit)(PolicyAudit {
                subject,
                reach: reach.clone(),
                level: (self.level)(),
                allowed: false,
                error_number: Some(error.number()),
                unavailable: Some(unavailable),
            });
            return Err(error);
        }

        if reach.is_empty() {
            return Ok(());
        }
        let subject = call.label();
        let level = (self.level)();
        let denied = reach.iter().copied().find(|capability| match level {
            MacroSecurity::Off => true,
            MacroSecurity::Guarded => !capability.allowed_in_guarded(),
            MacroSecurity::Unguarded => false,
        });
        (self.audit)(PolicyAudit {
            subject: subject.clone(),
            reach: reach.clone(),
            level,
            allowed: denied.is_none(),
            error_number: denied.map(|_| 70),
            unavailable: None,
        });
        match denied {
            None => Ok(()),
            Some(capability) => Err(AutoError::PolicyDenied {
                capability: capability.key(),
                subject,
                number: 70,
            }),
        }
    }
}
