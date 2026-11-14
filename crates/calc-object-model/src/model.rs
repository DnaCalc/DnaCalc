//! The member dispatch every projection calls, and the registry profiles fill.

use crate::descriptor::{Descriptor, InterfaceId, InvokeKind, Member};
use crate::object::ObjectRef;
use crate::value::AutoValue;
use std::collections::HashMap;

/// One call of a described member, as every projection presents it.
pub struct Call<'a> {
    pub interface: InterfaceId,
    pub member: &'a Member,
    /// The caller-supplied arguments in declared order ([`Member::arguments`]): `[lcid]` and
    /// `[out, retval]` are not included; an omitted optional argument is [`AutoValue::Missing`].
    /// For a property put, the assigned value is the last argument.
    pub args: Vec<AutoValue>,
    /// The `[lcid]` the client passed, or 0.
    pub lcid: u32,
}

impl Call<'_> {
    pub fn arg(&self, index: usize) -> &AutoValue {
        self.args.get(index).unwrap_or(&AutoValue::Missing)
    }
    /// `Interface.Member` as refusals and coverage reports name it.
    pub fn label(&self) -> String {
        format!(
            "{}.{}",
            Descriptor::get().ty(self.interface).name,
            self.member.name
        )
    }
}

/// Why a call did not produce a value.
#[derive(Clone, Debug, PartialEq)]
pub enum AutoError {
    /// The descriptor lists the member but this profile does not implement it (or not for these
    /// arguments). Every route answers it the same way: a run-time error 1004 whose description
    /// names the member and its owning bead, and a typed refusal recorded on the calling formula
    /// when the call happens inside a worksheet invocation.
    NotModelled { what: String, owner: &'static str },
    /// An error Excel itself raises for this call (number and description as Excel gives them,
    /// for example 1004 "needs a workbook: open in DNA Calc" in the values-only profile).
    Excel { number: i32, description: String },
    /// A COM status a projection passes through unchanged (E_INVALIDARG, DISP_E_TYPEMISMATCH, ...).
    Status(i32),
    /// The object's workbook or projection is gone (CO_E_OBJNOTCONNECTED on COM).
    Disconnected,
}

impl AutoError {
    pub fn not_modelled(what: impl Into<String>, owner: &'static str) -> AutoError {
        AutoError::NotModelled {
            what: what.into(),
            owner,
        }
    }
    pub fn excel(number: i32, description: impl Into<String>) -> AutoError {
        AutoError::Excel {
            number,
            description: description.into(),
        }
    }
    /// The Automation error number a client sees (VBA's `Err.Number`).
    pub fn number(&self) -> i32 {
        match self {
            AutoError::NotModelled { .. } => 1004,
            AutoError::Excel { number, .. } => *number,
            AutoError::Status(status) => *status,
            AutoError::Disconnected => 0x8004_0201u32 as i32,
        }
    }
    pub fn description(&self) -> String {
        match self {
            AutoError::NotModelled { what, owner } => {
                format!("DNA Calc does not model {what} yet ({owner})")
            }
            AutoError::Excel { description, .. } => description.clone(),
            AutoError::Status(status) => format!("COM status {status:#x}"),
            AutoError::Disconnected => "the object is no longer connected".into(),
        }
    }
}

/// The object model as a projection sees it. Implementations: `profile::values_only` (here),
/// `calc_host::automation::WorkbookAutomation` (DNA Calc), and the COM layer's composition with
/// its Office objects. A projection never answers a member itself.
pub trait ObjectModel {
    /// The Application object.
    fn application(&self) -> ObjectRef;
    /// Perform `call` on `target`. An object result represents one acquisition. The projection
    /// releases it when its wrapper dies, or immediately if the result is discarded or cannot
    /// be wrapped; a borrowed object argument is not a new acquisition.
    fn invoke(&self, target: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError>;
    /// The projection released its last wrapper of `target` (a Range releases its tracker slot).
    fn released(&self, _target: &ObjectRef) {}
    /// Record a refusal the projection is about to return, so a worksheet invocation that
    /// swallows the COM error still carries a typed refusal (AGENTS.md honesty rule 2).
    fn record_refusal(&self, _error: &AutoError) {}
}

/// A member implementation of a profile with context `C`.
pub type Handler<C> = fn(&C, &ObjectRef, &Call<'_>) -> Result<AutoValue, AutoError>;

/// The member table of one profile, keyed by the descriptor's identities. Registration names
/// members by `Interface.Member` text and fails for a name the descriptor does not hold, so a
/// typo is a startup panic in tests, never a silent gap.
pub struct Handlers<C> {
    table: HashMap<(InterfaceId, i32, InvokeKind), Handler<C>>,
}

impl<C> Default for Handlers<C> {
    fn default() -> Self {
        Handlers {
            table: HashMap::new(),
        }
    }
}

impl<C> Handlers<C> {
    pub fn new() -> Self {
        Self::default()
    }
    /// Register `interface.member` for `kind`. Panics when the descriptor has no such member.
    pub fn on(
        &mut self,
        interface: &str,
        member: &str,
        kind: InvokeKind,
        handler: Handler<C>,
    ) -> &mut Self {
        let descriptor = Descriptor::get();
        let id = descriptor
            .id(interface)
            .unwrap_or_else(|| panic!("object model: no interface {interface} in the descriptor")); // ubs:ignore - Static profile registration must fail at startup on a descriptor typo.
        let id = descriptor.default_interface(id);
        let found = descriptor.member(id, member, kind).unwrap_or_else(|| {
            panic!("object model: no {kind:?} {interface}.{member} in the descriptor") // ubs:ignore - Static profile registration must fail at startup on a missing described member.
        });
        self.table.insert((id, found.dispid, kind), handler);
        self
    }
    pub fn get(&self, interface: InterfaceId, member: &Member) -> Option<Handler<C>> {
        self.table
            .get(&(interface, member.dispid, member.invoke))
            .copied()
    }
    /// Run the registered handler, or answer the uniform refusal.
    pub fn dispatch(
        &self,
        context: &C,
        target: &ObjectRef,
        call: &Call<'_>,
        owner: &'static str,
    ) -> Result<AutoValue, AutoError> {
        match self.get(call.interface, call.member) {
            Some(handler) => handler(context, target, call),
            None => Err(AutoError::not_modelled(call.label(), owner)),
        }
    }
    /// Whether this table implements the member.
    pub fn implements(&self, interface: InterfaceId, member: &Member) -> bool {
        self.get(interface, member).is_some()
    }
    /// `Interface.Member (kind)` of every registered member, sorted: the coverage list.
    pub fn implemented(&self) -> Vec<String> {
        let descriptor = Descriptor::get();
        let mut names: Vec<String> = self
            .table
            .keys()
            .filter_map(|(id, dispid, kind)| {
                descriptor
                    .member_by_dispid(*id, *dispid, *kind)
                    .map(|m| format!("{}.{} ({kind:?})", descriptor.ty(*id).name, m.name))
            })
            .collect();
        names.sort();
        names
    }

    /// Coverage against the complete layouts a COM projection can advertise. `inherited`
    /// holds a composed profile's fallback handlers. Counts are handlers, not a promise that
    /// every argument form or catalogue entry is modelled; conditional refusals remain typed.
    pub fn coverage(&self, profile: &str, inherited: &[String]) -> String {
        use std::collections::BTreeSet;
        let d = Descriptor::get();
        let names: BTreeSet<_> = self
            .implemented()
            .into_iter()
            .chain(inherited.iter().cloned())
            .collect();
        let mut rows = Vec::new();
        let (mut implemented, mut total) = (0, 0);
        for (id, ty) in d.types() {
            let Some(slots) = ty.vtable_slots else {
                continue;
            };
            if (7..slots).any(|slot| d.member_by_slot(id, slot).is_none()) {
                continue;
            }
            let members = d.members(id);
            let count = members
                .iter()
                .filter(|m| names.contains(&format!("{}.{} ({:?})", ty.name, m.name, m.invoke)))
                .count();
            implemented += count;
            total += members.len();
            rows.push(format!("{profile}: {} {count}/{}", ty.name, members.len()));
        }
        rows.sort();
        rows.push(format!("{profile}: TOTAL {implemented}/{total}"));
        rows.extend(
            names
                .into_iter()
                .map(|name| format!("{profile}: MEMBER {name}")),
        );
        rows.join("\n")
    }
}
