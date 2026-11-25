//! Running a workbook project's Subs and Functions through the command route
//! (dc-vba-command-es70w). One call is one isolated invocation with one outcome: a success, a
//! run-time error that reached the caller, or a typed refusal naming the first member or command
//! the run needed that is not modelled. The execution policy's denials of the call are recorded
//! whether or not VBA caught the error. A procedure call is not an automatic event: events have
//! their own delivery records (`HostSession::workbook_event_failures`).
use super::WorkbookEvents;
use calc_host::HostSession;
pub use calc_vba::events::{
    ProcedureArg, ProcedureEntry, ProcedureError, ProcedureKind, ProcedureResult,
};
use std::rc::Rc;

/// How the procedure was reached; events are not in this log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InvocationKind {
    /// The command route (`dnacalc run`, a host command): a caller named the procedure.
    Command,
}

/// One request the execution policy denied during an invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Denial {
    /// `files`, `com-activate`, ... for the VBA library layer; the descriptor capability key for an
    /// object-model member.
    pub capability: String,
    /// The detail the request named (a path, a ProgID) or the descriptor member (`_Application.Run`).
    pub subject: String,
    /// Where the policy sits: `vba-host` (CalcVBA's host services) or `object-model`.
    pub layer: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// The procedure returned; a Sub returns `Empty`.
    Success(ProcedureResult),
    /// A VBA run-time error reached the caller.
    RuntimeError { number: i32, description: String, source: String },
    /// No such public Sub or Function, or the argument count did not fit it.
    NotCallable(String),
    /// A host member the run needed is not modelled. `member` is the descriptor member as the
    /// refusal names it; `owner` is the work package that owns it.
    Refused { member: String, owner: String, reason: String },
    /// The run exceeded its step budget or the backend failed.
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct InvocationRecord {
    /// `Module.Name` as the catalogue spells it, or the request when none matched.
    pub entry: String,
    pub kind: InvocationKind,
    pub outcome: Outcome,
    /// Requests the policy denied during the call, even when VBA caught the error.
    pub denials: Vec<Denial>,
    /// Requests the policy allowed (the audit total less the denials), for the Guarded/Full record.
    pub allowed: u64,
}

/// `DNA Calc does not model {what} yet ({owner})` back into its parts.
fn split_refusal(reason: &str) -> (String, String) {
    let body = reason.strip_prefix("DNA Calc does not model ").unwrap_or(reason);
    if let Some(inner) = body.strip_suffix(')')
        && let Some((what, owner)) = inner.rsplit_once(" yet (")
    {
        return (what.to_owned(), owner.to_owned());
    }
    (body.to_owned(), String::new())
}

impl WorkbookEvents {
    /// Every Sub and Function the project declares.
    pub fn procedures(&self) -> Vec<ProcedureEntry> {
        self.project.procedures()
    }

    /// The invocation log of this project, oldest first.
    pub fn invocations(&self) -> Vec<InvocationRecord> {
        self.invocations.borrow().clone()
    }

    /// Resolve `Module.Name`, or a bare `Name` that exactly one public procedure carries.
    fn resolve(&self, request: &str) -> Result<ProcedureEntry, String> {
        let entries = self.procedures();
        let matches: Vec<&ProcedureEntry> = match request.split_once('.') {
            Some((module, name)) => entries
                .iter()
                .filter(|p| {
                    p.public
                        && p.module.eq_ignore_ascii_case(module)
                        && p.name.eq_ignore_ascii_case(name)
                })
                .collect(),
            None => entries
                .iter()
                .filter(|p| p.public && p.name.eq_ignore_ascii_case(request))
                .collect(),
        };
        match matches.as_slice() {
            [one] => Ok((*one).clone()),
            [] => Err(format!("no public Sub or Function {request:?} in the project")),
            _ => Err(format!("{request:?} names more than one public procedure; qualify it with its module")),
        }
    }

    /// Run one procedure in a command context against `session`: its workbook is live, the
    /// project's policy applies and the outcome is recorded. Isolated per invocation: the audit
    /// log is drained before and after, so the record holds this call's requests only.
    pub fn run_procedure(
        self: &Rc<Self>,
        session: &mut HostSession,
        request: &str,
        arguments: &[ProcedureArg],
    ) -> InvocationRecord {
        let record = match self.resolve(request) {
            Err(why) => InvocationRecord {
                entry: request.to_owned(),
                kind: InvocationKind::Command,
                outcome: Outcome::NotCallable(why),
                denials: vec![],
                allowed: 0,
            },
            Ok(entry) => {
                if let Some(audit) = &self.audit {
                    let _ = audit.drain();
                }
                let (result, object_denials) = self.automation.with_event_session(
                    session,
                    self.file_project,
                    || {
                        self.project.run_procedure_recorded(
                            &entry.module,
                            &entry.name,
                            arguments,
                            self.max_steps,
                        )
                    },
                );
                let audit = self.audit.as_ref().map(|a| a.drain()).unwrap_or_default();
                let mut denials: Vec<Denial> = audit
                    .records
                    .iter()
                    .filter(|r| r.decision == calc_vba::policy::Decision::Denied)
                    .map(|r| Denial {
                        capability: format!("{:?}", r.capability),
                        subject: r.detail.clone(),
                        layer: "vba-host",
                    })
                    .collect();
                denials.extend(object_denials.into_iter().map(|(capability, subject)| Denial {
                    capability,
                    subject,
                    layer: "object-model",
                }));
                let outcome = match result {
                    Ok(value) => Outcome::Success(value),
                    Err(ProcedureError::Runtime { number, description, source }) => {
                        Outcome::RuntimeError { number, description, source }
                    }
                    Err(ProcedureError::Unknown(name)) => Outcome::NotCallable(name),
                    Err(ProcedureError::Arguments) => Outcome::NotCallable(format!(
                        "{}.{} does not take {} argument(s)",
                        entry.module,
                        entry.name,
                        arguments.len()
                    )),
                    Err(ProcedureError::Refused(reason)) => {
                        let (member, owner) = split_refusal(&reason);
                        Outcome::Refused { member, owner, reason }
                    }
                    Err(ProcedureError::Budget) => {
                        Outcome::Failed("the step budget was exhausted".into())
                    }
                    Err(ProcedureError::Backend(why)) => Outcome::Failed(why),
                };
                InvocationRecord {
                    entry: format!("{}.{}", entry.module, entry.name),
                    kind: InvocationKind::Command,
                    outcome,
                    allowed: audit.total.saturating_sub(audit.denied),
                    denials,
                }
            }
        };
        self.invocations.borrow_mut().push(record.clone());
        record
    }
}

fn interned(text: String) -> &'static str {
    use std::cell::RefCell;
    use std::collections::BTreeSet;
    thread_local! {
        static OWNERS: RefCell<BTreeSet<&'static str>> = const { RefCell::new(BTreeSet::new()) };
    }
    OWNERS.with(|owners| {
        let mut owners = owners.borrow_mut();
        if let Some(found) = owners.get(text.as_str()) {
            return *found;
        }
        let leaked: &'static str = Box::leak(text.into_boxed_str());
        owners.insert(leaked);
        leaked
    })
}

/// How deep `Application.Run` may nest before it raises VBA's "Out of stack space" (error 28).
/// Excel's own limit is not measured here (dc-oxh8w.34, RUN-ON-WINDOWS); this keeps the native stack safe.
const RUN_NESTING_LIMIT: usize = 48;

thread_local! {
    static RUN_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

struct RunDepth;
impl RunDepth {
    fn enter() -> Option<Self> {
        RUN_DEPTH.with(|depth| {
            if depth.get() >= RUN_NESTING_LIMIT {
                None
            } else {
                depth.set(depth.get() + 1);
                Some(RunDepth)
            }
        })
    }
}
impl Drop for RunDepth {
    fn drop(&mut self) {
        RUN_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

impl WorkbookEvents {
    /// `Application.Run "Name", args...` from a running procedure: the same invocation, nested.
    /// Excel's own errors: an unknown macro raises 1004 with its message; the macro's run-time
    /// error reaches the caller as that error.
    pub(crate) fn application_run(
        &self,
        name: &str,
        arguments: &[calc_object_model::AutoValue],
    ) -> Result<calc_object_model::AutoValue, calc_object_model::AutoError> {
        use calc_object_model::{AutoError, AutoValue};
        let request = match name.rsplit_once('!') {
            Some((book, request)) => {
                let book = book.trim();
                let book = book
                    .strip_prefix('\'')
                    .and_then(|quoted| quoted.strip_suffix('\''))
                    .unwrap_or(book)
                    .replace("''", "'");
                if !self.automation.matches_workbook_name(&book)? {
                    return Err(AutoError::not_modelled(
                        "_Application.Run of another workbook's project",
                        "dc-oxh8w.42",
                    ));
                }
                request
            }
            None => name,
        };
        let request = request.trim().trim_matches('\'');
        let entry = self.resolve(request).map_err(|_| {
            AutoError::excel(
                1004,
                format!(
                    "Cannot run the macro '{name}'. The macro may not be available in this workbook or all macros may be disabled."
                ),
            )
        })?;
        let mut args = Vec::new();
        for argument in arguments {
            args.push(match argument {
                AutoValue::Empty | AutoValue::Null => ProcedureArg::Empty,
                AutoValue::Missing => ProcedureArg::Missing,
                AutoValue::Bool(b) => ProcedureArg::Bool(*b),
                AutoValue::I2(n) => ProcedureArg::Number(f64::from(*n)),
                AutoValue::I4(n) => ProcedureArg::Number(f64::from(*n)),
                AutoValue::R4(n) => ProcedureArg::Number(f64::from(*n)),
                AutoValue::R8(n) => ProcedureArg::Number(*n),
                AutoValue::Text(t) => ProcedureArg::Text(t.to_string_lossy()),
                _ => {
                    return Err(AutoError::not_modelled(
                        "_Application.Run with an object or array argument",
                        "dc-80m0f",
                    ));
                }
            });
        }
        let Some(_depth) = RunDepth::enter() else {
            return Err(AutoError::excel(28, "Out of stack space"));
        };
        let (result, _denials) =
            self.project.run_procedure_recorded(&entry.module, &entry.name, &args, self.max_steps);
        match result {
            Ok(ProcedureResult::Empty) => Ok(AutoValue::Empty),
            Ok(ProcedureResult::Bool(b)) => Ok(AutoValue::Bool(b)),
            Ok(ProcedureResult::Number(n)) => Ok(AutoValue::R8(n)),
            Ok(ProcedureResult::Text(t)) => Ok(AutoValue::Text(calc_value::ExcelText::from_utf16_code_units(t.encode_utf16().collect()))),
            Ok(ProcedureResult::Other(kind)) => Err(AutoError::not_modelled(
                format!("_Application.Run returning a {kind} value"),
                "dc-80m0f",
            )),
            Err(ProcedureError::Runtime { number, description, .. }) => {
                Err(AutoError::excel(number, description))
            }
            Err(ProcedureError::Refused(reason)) => {
                let (member, owner) = split_refusal(&reason);
                Err(AutoError::not_modelled(member, interned(owner)))
            }
            Err(ProcedureError::Arguments) => Err(AutoError::excel(
                1004,
                format!("Cannot run the macro '{name}' with these arguments."),
            )),
            Err(ProcedureError::Backend(why)) if why.contains("reentry") => {
                // CalcVBA's dynamic call is not reentrant (dc-80m0f).
                Err(AutoError::not_modelled(
                    "_Application.Run of a project procedure from a running procedure",
                    "dc-80m0f",
                ))
            }
            Err(other) => Err(AutoError::excel(1004, format!("Application.Run: {other:?}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_host::{HostOptions, MemorySource, OpenPolicy};
    use calc_object_model::{AutoError, AutoValue, Call, Descriptor, InvokeKind, ObjectModel};
    use std::sync::Arc;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn application_run_keeps_the_workbook_qualifier() {
        let (mut session, owner) = project();
        let other = run(&mut session, &owner, &[AutoValue::text("'other.xlsm'!Module1.Answer")]);
        assert!(matches!(other, Err(AutoError::NotModelled { .. })), "{other:?}");
        let same = run(&mut session, &owner, &[AutoValue::text("'TEST BOOK.xlsm'!Module1.Answer")]);
        assert!(matches!(same, Ok(AutoValue::R8(42.0))), "{same:?}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn application_run_maps_arguments_defaults_and_procedure_results() {
        let (mut session, owner) = project();
        let args = [AutoValue::text("Answer"), AutoValue::Missing, AutoValue::I4(8)];
        let defaulted = run(&mut session, &owner, &args);
        assert!(matches!(defaulted, Ok(AutoValue::R8(45.0))), "{defaulted:?}");
        let missing_required = [AutoValue::text("EchoText"), AutoValue::Missing];
        assert!(matches!(
            run(&mut session, &owner, &missing_required),
            Err(AutoError::Excel { number: 1004, .. })
        ));
        let text = [AutoValue::text("EchoText"), AutoValue::text("hello")];
        let echoed = run(&mut session, &owner, &text).unwrap();
        assert_eq!(echoed.as_text().as_deref(), Some("hello"));
        for name in ["PrivateAnswer", "PropertyAnswer"] {
            let result = run(&mut session, &owner, &[AutoValue::text(name)]);
            assert!(matches!(result, Err(AutoError::Excel { number: 1004, .. })), "{result:?}");
        }
        let boom = run(&mut session, &owner, &[AutoValue::text("Boom")]);
        assert!(matches!(boom, Err(AutoError::Excel { number: 11, .. })), "{boom:?}");
    }

    fn project() -> (HostSession, Rc<WorkbookEvents>) {
        let bytes = include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/local.xlsm");
        let session = HostSession::open(
            Arc::new(MemorySource::new(bytes.to_vec()).named("Test Book.xlsm")),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        let owner = crate::prepare(
            session.host(),
            "Test Book.xlsm",
            &[crate::ModuleSource {
                name: "Module1".into(),
                target: crate::ModuleTarget::Standard,
                source: r#"
Public Function Answer(Optional ByVal first As Double = 37, Optional ByVal second As Double = 5) As Double
 Answer = first + second
End Function
Public Function EchoText(ByVal value As String) As String
 EchoText = value
End Function
Private Function PrivateAnswer() As Double
 PrivateAnswer = 1
End Function
Public Property Get PropertyAnswer() As Double
 PropertyAnswer = 2
End Property
Public Sub Boom()
 Dim x As Long
 x = 0
 x = 1 \ x
End Sub
"#.into(),
            }],
            1_000_000,
            Some(calc_vba::Backend::Interpreter),
        )
        .unwrap();
        (session, owner)
    }

    fn run(
        session: &mut HostSession,
        owner: &WorkbookEvents,
        args: &[AutoValue],
    ) -> Result<AutoValue, AutoError> {
        let model = &owner.automation;
        let app = model.application();
        let member = Descriptor::get().member(app.interface, "Run", InvokeKind::Method).unwrap();
        model.with_event_session(session, false, || {
            model.invoke(&app, &Call { interface: app.interface, member, args: args.to_vec(), lcid: 0 })
        })
    }
}
