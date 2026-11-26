//! Running a workbook project's Subs and Functions through the command route
//! (dc-vba-command-es70w). One call is one isolated invocation with one outcome: a success, a
//! run-time error that reached the caller, or a typed refusal naming the first member or command
//! the run needed that is not modelled. The execution policy's denials of the call are recorded
//! whether or not VBA caught the error. A procedure call is not an automatic event: events have
//! their own delivery records (`HostSession::workbook_event_failures`).
use super::WorkbookEvents;
use calc_host::automation::BookTarget;
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

    /// The files macro members wrote since the last call (Chart.Export), oldest first, as the
    /// path the macro named and the bytes. The shell stores them; this crate has no file system.
    pub fn take_output_files(&self) -> Vec<(String, Vec<u8>)> {
        std::mem::take(&mut *self.application.files.borrow_mut())
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

fn cannot_run(name: &str) -> calc_object_model::AutoError {
    calc_object_model::AutoError::excel(
        1004,
        format!(
            "Cannot run the macro '{name}'. The macro may not be available in this workbook or all macros may be disabled."
        ),
    )
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
        let request = match name.rsplit_once('!') {
            Some((book, request)) => {
                let book = book.trim();
                let book = book
                    .strip_prefix('\'')
                    .and_then(|quoted| quoted.strip_suffix('\''))
                    .unwrap_or(book)
                    .replace("''", "'");
                match self.automation.locate_book(&book)? {
                    BookTarget::This => request,
                    BookTarget::Other(Some(runner)) => {
                        return runner.run(name, request.trim().trim_matches('\''), arguments);
                    }
                    BookTarget::Other(None) => return Err(calc_host::automation::busy()),
                    BookTarget::Closed => return Err(cannot_run(name)),
                }
            }
            None => {
                // ww21 Application.Run.bare.*: a bare procedure name from VBA resolves only in this
                // workbook's project regardless of which workbook is active; otherwise Excel raises 1004.
                name
            }
        };
        self.run_here(name, request, arguments)
    }

    /// Whether this project holds a public procedure `request` resolves to.
    pub(crate) fn has_procedure(&self, request: &str) -> bool {
        self.resolve(request).is_ok()
    }

    /// Run `request` (no workbook qualifier) in this project; `name` is the caller's text for messages.
    pub(crate) fn run_here(
        &self,
        name: &str,
        request: &str,
        arguments: &[calc_object_model::AutoValue],
    ) -> Result<calc_object_model::AutoValue, calc_object_model::AutoError> {
        use calc_object_model::{AutoError, AutoValue};
        let request = request.trim().trim_matches('\'');
        let entry = self.resolve(request).map_err(|_| cannot_run(name))?;
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
                        "dc-oxh8w",
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
                "dc-oxh8w",
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
                    "dc-oxh8w",
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
        assert!(matches!(other, Err(AutoError::Excel { number: 1004, .. })), "{other:?}");
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

    /// A second workbook the application owns: its own session, project and automation.
    fn second_book(name: &str, source: &str) {
        second_book_from(name, source, include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/local.xlsm"));
    }

    fn second_book_from(name: &str, source: &str, bytes: &[u8]) {
        let mut session = HostSession::open(
            Arc::new(MemorySource::new(bytes.to_vec()).named(name)),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        let book = crate::prepare(
            session.host(),
            name,
            &[crate::ModuleSource { name: "Module1".into(), target: crate::ModuleTarget::Standard, source: source.into() }],
            1_000_000,
            Some(calc_vba::Backend::Interpreter),
        )
        .unwrap();
        book.clone().install(&mut session);
        book.adopt(session, false);
    }

    const OTHER: &str = r#"
Public Function WhoAmI() As String
 WhoAmI = ThisWorkbook.Name
End Function
Public Function ActiveName() As String
 ActiveName = ActiveWorkbook.Name
End Function
Public Function BookCount() As Double
 BookCount = Workbooks.Count
End Function
Public Function SecondName() As String
 SecondName = Workbooks(2).Name
End Function
Public Function WriteMain(ByVal v As Double) As Double
 Workbooks(1).Worksheets(1).Range("B7").Value = v
 WriteMain = Workbooks(1).Worksheets(1).Range("B7").Value
End Function
Public Function ReadMine(ByVal address As String) As Double
 ReadMine = ThisWorkbook.Worksheets(1).Range(address).Value
End Function
Public Function SheetName() As String
 SheetName = Workbooks(1).Worksheets(1).Name & "|" & ThisWorkbook.Worksheets(1).Name
End Function
Public Function CallBack() As Double
 Dim n As String
 n = "Test Book.xlsm!Module1.Answer"
 CallBack = Application.Run(n, 40, 2)
End Function
Public Function OpenIt(ByVal path As String) As String
 Dim book As Workbook
 Set book = Workbooks.Open(path)
 OpenIt = book.Name & "/" & ActiveWorkbook.Name & "/" & Workbooks.Count
End Function
Public Function OpenWith(ByVal path As String) As String
 Dim book As Workbook
 Set book = Workbooks.Open(path, 0, False, , , , True, , , , False, , False)
 OpenWith = book.Name & "/" & Workbooks.Count
End Function
Public Function OpenReadOnly(ByVal path As String) As String
 Dim book As Workbook
 Set book = Workbooks.Open(path, 0, True)
 OpenReadOnly = book.Name & "/" & book.ReadOnly & "/" & Workbooks(1).ReadOnly
End Function
Public Function OpenBadType(ByVal path As String) As String
 OpenBadType = Workbooks.Open(path, "often").Name
End Function
Public Function OpenPassword(ByVal path As String) As String
 OpenPassword = Workbooks.Open(path, , , , "secret").Name
End Function
Public Function AddFrom(ByVal path As String) As String
 AddFrom = Workbooks.Add(path).Name & "/" & Workbooks.Count
End Function
Public Function AddChart() As String
 AddChart = Workbooks.Add(-4109).Name
End Function
Public Function CopyToNew() As String
 Dim here As String
 here = ThisWorkbook.Worksheets(1).Name
 ThisWorkbook.Worksheets(1).Range("A1").Value = 7
 ThisWorkbook.Worksheets(1).Copy
 CopyToNew = ActiveWorkbook.Name & "/" & Workbooks.Count & "/" & ActiveWorkbook.Worksheets.Count & "/" & (ActiveSheet.Name = here) & "/" & ActiveSheet.Range("A1").Value & "/" & ThisWorkbook.Worksheets(1).Range("A1").Value
End Function
Public Function CopyAllToNew() As String
 Dim n As Long
 n = ThisWorkbook.Worksheets.Count
 ThisWorkbook.Worksheets.Add
 ThisWorkbook.Worksheets.Copy
 CopyAllToNew = ActiveWorkbook.Name & "/" & ActiveWorkbook.Worksheets.Count & "/" & ThisWorkbook.Worksheets.Count & "/" & n
End Function
Public Function CopyOneOfTwo() As String
 ThisWorkbook.Worksheets.Add
 ThisWorkbook.Worksheets(1).Copy
 CopyOneOfTwo = ActiveWorkbook.Worksheets.Count & "/" & ThisWorkbook.Worksheets.Count & "/" & Workbooks.Count
End Function
Public Function CopyBeforeForeign() As String
 Dim other As Workbook
 Set other = Workbooks.Add
 ThisWorkbook.Worksheets(1).Copy Before:=other.Worksheets(1)
End Function
Public Function SheetsAddBeforeForeign() As String
 Dim other As Workbook
 Set other = Workbooks.Add
 ThisWorkbook.Sheets.Add Before:=other.Worksheets(1)
End Function
Public Function UnionThisThenForeign() As String
 Dim other As Workbook
 Set other = Workbooks.Add
 Dim r As Range
 Set r = Union(ThisWorkbook.Worksheets(1).Range("A1"), other.Worksheets(1).Range("A1"))
End Function
Public Function UnionForeignThenThis() As String
 Dim other As Workbook
 Set other = Workbooks.Add
 Dim r As Range
 Set r = Union(other.Worksheets(1).Range("A1"), ThisWorkbook.Worksheets(1).Range("A1"))
End Function
Public Function RangeForeignThenThis() As String
 Dim other As Workbook
 Set other = Workbooks.Add
 Dim r As Range
 Set r = Range(other.Worksheets(1).Range("A1"), ThisWorkbook.Worksheets(1).Range("B2"))
End Function
Public Function MoveBeforeInGroup() As String
 Dim src As Workbook
 Set src = Workbooks.Add
 src.Worksheets.Add
 src.Worksheets.Add
 src.Worksheets(Array(2, 3)).Move Before:=src.Worksheets(2)
End Function
Public Function MoveRefusedAtSource() As String
 Dim src As Workbook
 Set src = Workbooks.Add
 src.Worksheets.Add
 src.Worksheets(1).Range("A1:B2").Value = 1
 src.Worksheets(1).ListObjects.Add(1, src.Worksheets(1).Range("A1:B2"), , 1).Name = "T1"
 src.Worksheets(2).Range("A1").Formula = "=SUM(T1[Column1])"
 On Error Resume Next
 src.Worksheets(1).Move
 MoveRefusedAtSource = Err.Number & "/" & Workbooks.Count & "/" & ActiveWorkbook.Name & "/" & src.Name & "/" & src.Worksheets.Count
End Function
Public Function CountBooks() As String
 CountBooks = Workbooks.Count & "/" & ActiveWorkbook.Name & "/" & ThisWorkbook.Name
End Function
Public Function CopyArrayToNew() As String
 ThisWorkbook.Worksheets(Array(1, 2)).Copy
 CopyArrayToNew = ActiveWorkbook.Name & "/" & ActiveWorkbook.Worksheets.Count & "/" & (ActiveWorkbook.Worksheets(1).Name = ThisWorkbook.Worksheets(1).Name) & "/" & ThisWorkbook.Worksheets.Count
End Function
Public Function MoveToNew() As String
 Dim src As Workbook
 Set src = Workbooks.Add
 src.Worksheets.Add
 src.Worksheets(2).Range("A1").Value = 9
 src.Worksheets(2).Move
 MoveToNew = ActiveWorkbook.Name & "/" & ActiveWorkbook.Worksheets.Count & "/" & ActiveWorkbook.Worksheets(1).Range("A1").Value & "/" & src.Worksheets.Count & "/" & src.Name
End Function
Public Function MoveGroup() As String
 Dim src As Workbook
 Set src = Workbooks.Add
 src.Worksheets.Add
 src.Worksheets.Add
 src.Worksheets(1).Range("A1").Value = 1
 src.Worksheets(2).Range("A1").Value = 2
 src.Worksheets(3).Range("A1").Value = 3
 src.Worksheets(Array(1, 2)).Move After:=src.Worksheets(3)
 MoveGroup = src.Worksheets.Count & "/" & src.Worksheets(1).Range("A1").Value & src.Worksheets(2).Range("A1").Value & src.Worksheets(3).Range("A1").Value
End Function
Public Function MoveFromMacroBook() As String
 ThisWorkbook.Worksheets.Add
 ThisWorkbook.Worksheets(2).Move
End Function
Public Function MoveAllToNew() As String
 Dim src As Workbook
 Set src = Workbooks.Add
 src.Worksheets.Move
End Function
Public Function GroupCount() As String
 GroupCount = ThisWorkbook.Worksheets(Array(2, 1)).Count & "/" & ThisWorkbook.Worksheets(Array(2, 1))(1).Name = ThisWorkbook.Worksheets(1).Name
End Function
Public Function AddOne() As String
 Workbooks.Add
 AddOne = ActiveWorkbook.Name & "/" & Workbooks.Count
End Function
Public Function AddFromWithPath(ByVal path As String) As String
 Dim wb As Workbook
 Set wb = Workbooks.Add(path)
 AddFromWithPath = wb.Name & "|" & wb.Path
End Function
Public Function OpenBadUpdateLinks(ByVal path As String) As String
 OpenBadUpdateLinks = Workbooks.Open(path, 99).Name
End Function
Public Function AfterCloseName() As String
 Dim wb As Workbook
 Set wb = Workbooks.Add
 wb.Close False
 AfterCloseName = wb.Name
End Function
Public Function CloseReadOnlySave(ByVal path As String) As String
 Dim wb As Workbook
 Set wb = Workbooks.Open(path, 0, True)
 wb.Worksheets(1).Range("A1").Value = 8
 wb.Close True
 CloseReadOnlySave = "closed"
End Function
Public Function CloseWritableSave(ByVal path As String) As String
 Dim wb As Workbook
 Set wb = Workbooks.Open(path)
 wb.Worksheets(1).Range("A1").Value = 123
 wb.Close True
 CloseWritableSave = "closed"
End Function
"#;

    fn text(result: Result<AutoValue, AutoError>) -> String {
        result.unwrap().as_text().unwrap().to_string()
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn application_run_reaches_another_open_workbook() {
        let (mut session, owner) = project();
        second_book("Other.xlsm", OTHER);
        let who = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!Module1.WhoAmI")]);
        assert_eq!(text(who), "Other.xlsm", "ThisWorkbook is the workbook whose macro runs");
        let quoted = run(&mut session, &owner, &[AutoValue::text("'other.xlsm'!WhoAmI")]);
        assert_eq!(text(quoted), "Other.xlsm");
        // Running a workbook leaves the active workbook alone.
        let active = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!ActiveName")]);
        assert_eq!(text(active), "Test Book.xlsm");
        let count = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!BookCount")]);
        assert!(matches!(count, Ok(AutoValue::R8(2.0))), "{count:?}");
        let second = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!SecondName")]);
        assert_eq!(text(second), "Other.xlsm");
        let missing = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!Nope")]);
        assert!(matches!(missing, Err(AutoError::Excel { number: 1004, .. })), "{missing:?}");
        // Running back into the workbook whose macro is running runs it, nested.
        let q = run(&mut session, &owner, &[AutoValue::text("QualifiedSelf")]);
        assert!(matches!(q, Ok(AutoValue::R8(2.0))), "{q:?}");
        let back = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!CallBack")]);
        assert!(matches!(back, Ok(AutoValue::R8(42.0))), "{back:?}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn workbooks_collection_lists_the_open_workbooks() {
        let (mut session, owner) = project();
        second_book("Other.xlsm", OTHER);
        let model = &owner.automation;
        let app = model.application();
        let get = |object: &calc_object_model::ObjectRef, interface_name: &str, member: &str, args: Vec<AutoValue>| {
            let descriptor = Descriptor::get();
            let interface = object.interface;
            let _ = interface_name;
            let member = descriptor.member(interface, member, InvokeKind::Get).unwrap_or_else(|| panic!("{member}"));
            model.invoke(object, &Call { interface, member, args, lcid: 0 })
        };
        model.with_event_session(&mut session, false, || {
            let books = match get(&app, "Application", "Workbooks", vec![]).unwrap() {
                AutoValue::Object(o) => o,
                other => panic!("{other:?}"),
            };
            assert!(matches!(get(&books, "Workbooks", "Count", vec![]), Ok(AutoValue::I4(2))));
            let by_name = match get(&books, "Workbooks", "Item", vec![AutoValue::text("other.XLSM")]).unwrap() {
                AutoValue::Object(o) => o,
                other => panic!("{other:?}"),
            };
            let by_index = match get(&books, "Workbooks", "Item", vec![AutoValue::I4(2)]).unwrap() {
                AutoValue::Object(o) => o,
                other => panic!("{other:?}"),
            };
            assert_eq!(by_name.key, by_index.key);
            assert_eq!(text(get(&by_name, "Workbook", "Name", vec![])), "Other.xlsm");
            assert_eq!(text(get(&by_name, "Workbook", "FullName", vec![])), "Other.xlsm");
            let first = match get(&books, "Workbooks", "Item", vec![AutoValue::I4(1)]).unwrap() {
                AutoValue::Object(o) => o,
                other => panic!("{other:?}"),
            };
            assert_eq!(text(get(&first, "Workbook", "Name", vec![])), "Test Book.xlsm");
            for bad in [AutoValue::I4(0), AutoValue::I4(3), AutoValue::text("none.xlsx")] {
                assert!(get(&books, "Workbooks", "Item", vec![bad]).is_err());
            }
            // Another workbook's sheets answer from that workbook.
            let sheets = get(&by_name, "Workbook", "Worksheets", vec![]).unwrap();
            let AutoValue::Object(sheets) = sheets else { panic!() };
            assert!(matches!(get(&sheets, "Sheets", "Count", vec![]), Ok(AutoValue::I4(n)) if n >= 1));
            let activate = Descriptor::get().member(by_name.interface, "Activate", InvokeKind::Method).unwrap();
            model.invoke(&by_name, &Call { interface: by_name.interface, member: activate, args: vec![], lcid: 0 }).unwrap();
            let active = get(&app, "Application", "ActiveWorkbook", vec![]).unwrap();
            assert!(matches!(active, AutoValue::Object(o) if o.key == by_name.key));
        });
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn macros_reach_the_ranges_and_sheets_of_other_open_workbooks() {
        let (mut session, owner) = project();
        second_book("Other.xlsm", OTHER);
        // Other's macro writes and reads a cell of the first workbook, whose session is lent out.
        let wrote = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!WriteMain"), AutoValue::R8(7.5)]);
        assert!(matches!(wrote, Ok(AutoValue::R8(7.5))), "{wrote:?}");
        let mine = run(&mut session, &owner, &[AutoValue::text("ReadMine"), AutoValue::text("B7")]);
        assert!(matches!(mine, Ok(AutoValue::R8(7.5))), "the write landed in the first workbook: {mine:?}");
        let theirs = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!ReadMine"), AutoValue::text("B7")]);
        assert!(matches!(theirs, Ok(AutoValue::R8(0.0)) | Err(_)), "the other workbook's own B7 is untouched: {theirs:?}");
        let names = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!SheetName")]);
        assert_eq!(text(names), "Cases|Cases");
        // The first workbook's macro writes and reads a cell of the other workbook: its own session answers.
        let seen = run(&mut session, &owner, &[AutoValue::text("SeeOther")]);
        assert!(matches!(seen, Ok(AutoValue::R8(42.0))), "{seen:?}");
        let there = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!ReadMine"), AutoValue::text("C3")]);
        assert!(matches!(there, Ok(AutoValue::R8(41.0))), "{there:?}");
        let here = run(&mut session, &owner, &[AutoValue::text("ReadMine"), AutoValue::text("C3")]);
        assert!(matches!(here, Ok(AutoValue::R8(0.0)) | Err(_)), "the first workbook's C3 is untouched: {here:?}");
        let count = run(&mut session, &owner, &[AutoValue::text("OtherSheets")]);
        assert!(matches!(count, Ok(AutoValue::R8(n)) if n >= 1.0), "{count:?}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn workbook_close_removes_an_owned_workbook() {
        let (mut session, owner) = project();
        second_book("Other.xlsm", OTHER);
        let closed = run(&mut session, &owner, &[AutoValue::text("CloseOther")]);
        assert!(matches!(closed, Ok(AutoValue::R8(n)) if n == 12.0), "one workbook left, then 1+1: {closed:?}");
        let gone = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!WhoAmI")]);
        assert!(matches!(gone, Err(AutoError::Excel { number: 1004, .. })), "{gone:?}");
        // Excel makes the previously active workbook active again.
        second_book("Third.xlsm", OTHER);
        let active = run(&mut session, &owner, &[AutoValue::text("ActiveAfterClose")]);
        assert_eq!(text(active), "Test Book.xlsm");
        // The workbook whose macro runs stays open: a refusal naming its owner.
        for name in ["CloseSelf", "CloseAll"] {
            let refused = run(&mut session, &owner, &[AutoValue::text(name)]);
            assert!(matches!(&refused, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-oxh8w.75"), "{name}: {refused:?}");
        }
        // Closing with SaveChanges True needs the save writer.
        second_book("Fourth.xlsm", OTHER);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn workbooks_add_and_open_make_open_workbooks() {
        let (mut session, owner) = project();
        second_book("Other.xlsm", OTHER);
        let without = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!AddOne")]);
        assert!(matches!(&without, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-oxh8w.75"), "{without:?}");
        crate::install_application(crate::ApplicationOptions {
            max_steps: 1_000_000,
            backend: Some(calc_vba::Backend::Interpreter),
            loader: Some(Rc::new(|path: &str| {
                if path == "fixture.xlsm" {
                    Ok(include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/local.xlsm").to_vec())
                } else {
                    Err("missing".into())
                }
            })),
            saver: None,
            grant: None,
        });
        let added = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!AddOne")]);
        assert_eq!(text(added), "Book1/3", "Add activates the new blank workbook");
        let count = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!BookCount")]);
        assert!(matches!(count, Ok(AutoValue::R8(3.0))), "{count:?}");
        let opened = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!OpenIt"), AutoValue::text("fixture.xlsm")]);
        assert_eq!(text(opened), "fixture.xlsm/fixture.xlsm/4");
        let with = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!OpenWith"), AutoValue::text("fixture.xlsm")]);
        assert_eq!(text(with), "fixture.xlsm/5", "options after FileName that change nothing here are accepted");
        for (name, wanted) in [("AddChart", Some("dc-oxh8w.75")), ("OpenBadType", None)] {
            let mut arguments = vec![AutoValue::text(&format!("Other.xlsm!{name}"))];
            if name != "AddChart" {
                arguments.push(AutoValue::text("fixture.xlsm"));
            }
            let result = run(&mut session, &owner, &arguments);
            match (wanted, &result) {
                (Some(bead), Err(AutoError::NotModelled { owner, .. })) => assert_eq!(*owner, bead, "{name}"),
                (None, Err(AutoError::Excel { number: 1004, .. })) => {}
                _ => panic!("{name}: {result:?}"),
            }
        }
        let copied = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!CopyToNew")]);
        assert_eq!(text(copied), "Book2/6/1/True/7/7", "dc-oxh8w.43: Worksheet.Copy with no anchor makes a new active workbook holding the sheet");
        let copied = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!CopyAllToNew")]);
        assert_eq!(text(copied), "Book3/2/2/1", "Worksheets.Copy with no anchor copies every sheet into a new workbook");
        let copied = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!CopyArrayToNew")]);
        assert_eq!(text(copied), "Book4/2/True/2", "an Array of sheets copies just those sheets into a new workbook");
        // Move to a new workbook leaves the sheet there only; the last sheet cannot leave; a group moves in order.
        let moved = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!MoveToNew")]);
        assert_eq!(text(moved), "Book6/1/9/1/Book5", "Worksheet.Move with no anchor moves the sheet into a new active workbook");
        let group = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!MoveGroup")]);
        assert_eq!(text(group), "3/312", "a group moves after the anchor, keeping its order");
        let macro_book = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!MoveFromMacroBook")]);
        assert!(matches!(&macro_book, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-2g5fn"), "{macro_book:?}");
        let all = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!MoveAllToNew")]);
        assert!(matches!(all, Err(AutoError::Excel { number: 1004, .. })), "every sheet cannot leave: {all:?}");
        let template = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!AddFrom"), AutoValue::text("fixture.xlsm")]);
        assert_eq!(text(template), "fixture1/13", "ww21: a workbook made from a template file is named <stem>N");
        let missing = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!OpenIt"), AutoValue::text("gone.xlsm")]);
        assert!(matches!(&missing, Err(AutoError::Excel { number: 1004, description }) if description.contains("could not be found")), "{missing:?}");
        let read_only = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!OpenReadOnly"), AutoValue::text("fixture.xlsm")]);
        assert_eq!(text(read_only), "fixture.xlsm/True/False", "ReadOnly:=True opens a read-only workbook; the others stay writable");
        let plain = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!OpenIt"), AutoValue::text("fixture.xlsm")]);
        assert!(plain.is_ok(), "{plain:?}");
        // ww21 Application.Run.bare.*: an unqualified name not in the running workbook raises 1004.
        let not_here = run(&mut session, &owner, &[AutoValue::text("RunBare"), AutoValue::text("WhoAmI")]);
        assert!(matches!(&not_here, Err(AutoError::Excel { number: 1004, description }) if description.contains("Cannot run the macro")), "{not_here:?}");
        let nowhere = run(&mut session, &owner, &[AutoValue::text("RunBare"), AutoValue::text("NoSuchProcedure")]);
        assert!(matches!(&nowhere, Err(AutoError::Excel { number: 1004, description }) if description.contains("Cannot run the macro")), "{nowhere:?}");
        // A copy that fails leaves no orphan workbook behind and the active workbook as it was.
        // dc-9spk9 (rw13 finding 7): a Move whose source cannot drop the sheet (it holds a table) leaves no
        // orphan BookN behind: the workbooks are the caller's plus the source, the source stays active.
        let count = |s: String| s.split('/').next().unwrap().parse::<u32>().unwrap();
        let base = count(text(run(&mut session, &owner, &[AutoValue::text("Other.xlsm!CountBooks")])));
        let refused = text(run(&mut session, &owner, &[AutoValue::text("Other.xlsm!MoveRefusedAtSource")]));
        let parts: Vec<&str> = refused.split('/').collect();
        assert_eq!(parts[0], "1004", "{refused}");
        assert_eq!(parts[1].parse::<u32>().unwrap(), base + 1, "no orphan BookN after a refused Move: {refused}");
        assert_eq!(parts[2], parts[3], "the source workbook is active again: {refused}");
        assert_eq!(parts[4], "2", "{refused}");
        let before = text(run(&mut session, &owner, &[AutoValue::text("Other.xlsm!CountBooks")]));
        // local.xlsm holds defined names the save cannot rewrite, so dropping the other sheet is refused.
        let failed = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!CopyOneOfTwo")]);
        assert!(matches!(failed, Err(AutoError::Excel { number: 1004, .. })), "{failed:?}");
        assert_eq!(text(run(&mut session, &owner, &[AutoValue::text("Other.xlsm!CountBooks")])), before, "no orphan BookN after a failed copy");
        // Anchors the host cannot place faithfully are typed refusals (dc-fa8cx), never an alias or a guessed order.
        for name in ["CopyBeforeForeign", "MoveBeforeInGroup"] {
            let r = run(&mut session, &owner, &[AutoValue::text(&format!("Other.xlsm!{name}"))]);
            assert!(matches!(&r, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-fa8cx"), "{name}: {r:?}");
        }
        // dc-mkdhw: a Sheets.Add anchored on another workbook's sheet is refused likewise, not aliased to a local SheetId.
        let r = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!SheetsAddBeforeForeign")]);
        assert!(matches!(&r, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-fa8cx"), "Sheets.Add: {r:?}");
        // dc-mkdhw: ranges of two workbooks never make one range, whichever comes first (Application.Range and Union).
        for name in ["UnionThisThenForeign", "UnionForeignThenThis", "RangeForeignThenThis"] {
            let r = run(&mut session, &owner, &[AutoValue::text(&format!("Other.xlsm!{name}"))]);
            assert!(matches!(&r, Err(AutoError::Excel { number: 1004, .. })), "{name}: {r:?}");
        }
        // The added workbook is open by name and can be run (it has no procedures).
        let none = run(&mut session, &owner, &[AutoValue::text("Book1!Anything")]);
        assert!(matches!(none, Err(AutoError::Excel { number: 1004, .. })), "{none:?}");
    }

    /// dc-9spk9 (rw13 finding 10): `Application.Goto` goes to the active workbook; `Application.Calculate*` with
    /// another workbook open is a typed refusal (Excel calculates every open workbook; its answer is unrecorded).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_9spk9_goto_and_calculate_follow_the_active_workbook() {
        let bytes = include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/local.xlsm");
        let mut session = HostSession::open(
            Arc::new(MemorySource::new(bytes.to_vec()).named("MainBook.xlsm")),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        let owner = crate::prepare(
            session.host(),
            "MainBook.xlsm",
            &[crate::ModuleSource {
                name: "Module1".into(),
                target: crate::ModuleTarget::Standard,
                source: r#"
Public Function GotoActive() As String
 ThisWorkbook.Worksheets(1).Range("B2").Select
 Dim other As Workbook
 Set other = Workbooks("ActiveBook.xlsx")
 other.Activate
 Application.Goto "D6"
 GotoActive = ActiveWorkbook.Name & "|" & ActiveCell.Address & "|" & ThisWorkbook.Windows(1).ActiveCell.Address
End Function
Public Sub CalcActive()
 Workbooks("ActiveBook.xlsx").Activate
 Application.Calculate
End Sub
Public Sub CalcFullActive()
 Workbooks("ActiveBook.xlsx").Activate
 Application.CalculateFull
End Sub
"#.into(),
            }],
            1_000_000,
            Some(calc_vba::Backend::Interpreter),
        )
        .unwrap();
        let mut other_session = HostSession::open(
            Arc::new(MemorySource::new(calc_doc::blank_package()).named("ActiveBook.xlsx")),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        let other_book = crate::prepare(
            other_session.host(),
            "ActiveBook.xlsx",
            &[crate::ModuleSource { name: "Module1".into(), target: crate::ModuleTarget::Standard, source: "Option Explicit\n".into() }],
            1_000_000,
            Some(calc_vba::Backend::Interpreter),
        )
        .unwrap();
        other_book.clone().install(&mut other_session);
        other_book.adopt(other_session, false);
        let got = run(&mut session, &owner, &[AutoValue::text("GotoActive")]);
        assert_eq!(text(got), "ActiveBook.xlsx|$D$6|$B$2", "Goto selects in the active workbook, not the caller's");
        for name in ["CalcActive", "CalcFullActive"] {
            let calc = run(&mut session, &owner, &[AutoValue::text(name)]);
            assert!(matches!(&calc, Err(AutoError::NotModelled { owner: "dc-gw1be", .. })), "{name}: {calc:?}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zd4cx_unqualified_application_members_target_active_workbook() {
        let bytes = include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/local.xlsm");
        let mut session = HostSession::open(
            Arc::new(MemorySource::new(bytes.to_vec()).named("MainBook.xlsm")),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        let owner = crate::prepare(
            session.host(),
            "MainBook.xlsm",
            &[crate::ModuleSource {
                name: "Module1".into(),
                target: crate::ModuleTarget::Standard,
                source: r#"
Public Function ProbeActive() As String
 ThisWorkbook.Worksheets(1).Range("A1").Value = 10
 ThisWorkbook.Worksheets(1).Range("B2").Select
 ThisWorkbook.Names.Add Name:="MainOnly", RefersTo:="=Cases!$A$1"
 Dim other As Workbook
 Set other = Workbooks("ActiveBook.xlsx")
 other.Activate
 other.Worksheets(1).Range("A1").Value = 99
 other.Worksheets(1).Range("C4").Select
 other.Names.Add Name:="OtherOnly", RefersTo:="=Sheet1!$A$1"
 Range("D5").Value = 77
 Dim s As String
 s = ActiveWorkbook.Name
 s = s & "|" & ActiveSheet.Name
 s = s & "|" & ThisWorkbook.ActiveSheet.Name
 s = s & "|" & Sheets(1).Name
 s = s & "|" & Worksheets(1).Name
 s = s & "|" & Names("OtherOnly").Name
 s = s & "|" & Cells(1, 1).Value
 s = s & "|" & Range("A1").Value
 s = s & "|" & ActiveCell.Address & ":" & ActiveCell.Worksheet.Parent.Name
 s = s & "|" & Selection.Address & ":" & Selection.Worksheet.Parent.Name
 s = s & "|" & Rows(1).Worksheet.Parent.Name
 s = s & "|" & Columns(1).Worksheet.Parent.Name
 s = s & "|" & ActiveWindow.Caption & ":" & ActiveWindow.ActiveSheet.Name & ":" & ActiveWindow.ActiveCell.Address
 s = s & "|" & ThisWorkbook.Windows(1).ActiveSheet.Name & ":" & ThisWorkbook.Windows(1).ActiveCell.Address
 s = s & "|" & Range(ThisWorkbook.Worksheets(1).Range("A1"), ThisWorkbook.Worksheets(1).Range("A1")).Value
 s = s & "|" & other.Worksheets(1).Range("D5").Value & ":" & ThisWorkbook.Worksheets(1).Range("D5").Value
 other.Close False
 ProbeActive = s
End Function
"#.into(),
            }],
            1_000_000,
            Some(calc_vba::Backend::Interpreter),
        )
        .unwrap();
        let mut other_session = HostSession::open(
            Arc::new(MemorySource::new(calc_doc::blank_package()).named("ActiveBook.xlsx")),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        let other_book = crate::prepare(
            other_session.host(),
            "ActiveBook.xlsx",
            &[crate::ModuleSource { name: "Module1".into(), target: crate::ModuleTarget::Standard, source: "Option Explicit\n".into() }],
            1_000_000,
            Some(calc_vba::Backend::Interpreter),
        )
        .unwrap();
        other_book.clone().install(&mut other_session);
        other_book.adopt(other_session, false);
        let got = run(&mut session, &owner, &[AutoValue::text("ProbeActive")]);
        assert_eq!(
            text(got),
            "ActiveBook.xlsx|Sheet1|Cases|Sheet1|Sheet1|OtherOnly|99|99|$C$4:ActiveBook.xlsx|$C$4:ActiveBook.xlsx|ActiveBook.xlsx|ActiveBook.xlsx|ActiveBook.xlsx:Sheet1:$C$4|Cases:$B$2|10|77:"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_oxh8w_75__ww21_multi_workbook_rules() {
        let (mut session, owner) = project();
        second_book("Other.xlsm", OTHER);
        let saved: Rc<std::cell::RefCell<Vec<(String, Vec<u8>)>>> = Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = saved.clone();
        crate::install_application(crate::ApplicationOptions {
            max_steps: 1_000_000,
            backend: Some(calc_vba::Backend::Interpreter),
            loader: Some(Rc::new(|path: &str| {
                if path == "fixture.xlsm" || path == "dir/Data.xlsm" {
                    Ok(include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/local.xlsm").to_vec())
                } else if path == "encrypted.xlsx" {
                    let mut ole = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
                    ole.extend_from_slice(&[0u8; 64]);
                    Ok(ole)
                } else {
                    Err("missing".into())
                }
            })),
            saver: Some(Rc::new(move |path: &str, bytes: &[u8]| {
                sink.borrow_mut().push((path.to_owned(), bytes.to_vec()));
                Ok(())
            })),
            grant: None,
        });
        // ww21 Application.Run.bare.*: bare name never searches other open workbooks regardless of active book.
        let bare = run(&mut session, &owner, &[AutoValue::text("RunBare"), AutoValue::text("WhoAmI")]);
        assert!(
            matches!(&bare, Err(AutoError::Excel { number: 1004, description }) if description.contains("Cannot run the macro")),
            "bare Run must return 1004: {bare:?}"
        );
        // ww21 Workbooks.Add.Template: Data.xlsm -> Data1 with empty Path, second -> Data2.
        let t1 = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!AddFromWithPath"), AutoValue::text("dir/Data.xlsm")]);
        assert_eq!(text(t1), "Data1|");
        let t2 = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!AddFromWithPath"), AutoValue::text("dir/Data.xlsm")]);
        assert_eq!(text(t2), "Data2|");
        // ww21 Workbooks.Open.password.unencrypted: Password ignored for unencrypted workbook.
        let pw_plain = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!OpenPassword"), AutoValue::text("fixture.xlsm")]);
        assert_eq!(text(pw_plain), "fixture.xlsm");
        // Encrypted workbook with Password refuses citing dc-oxh8w.75.
        let pw_enc = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!OpenPassword"), AutoValue::text("encrypted.xlsx")]);
        assert!(matches!(&pw_enc, Err(AutoError::NotModelled { owner: "dc-oxh8w.75", .. })), "{pw_enc:?}");
        // ww21 Workbooks.Open.badUpdateLinks: out-of-range UpdateLinks gives 1004.
        let bad_ul = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!OpenBadUpdateLinks"), AutoValue::text("fixture.xlsm")]);
        assert!(matches!(bad_ul, Err(AutoError::Excel { number: 1004, .. })), "{bad_ul:?}");
        // ww21 VBA.AfterClose: accessing held Workbook after Close fails disconnected.
        let after_close = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!AfterCloseName")]);
        assert!(after_close.is_err(), "held Workbook after Close must fail: {after_close:?}");
        // ww21 Workbook.Close(True) on read-only refuses citing dc-oxh8w.75 (Save As dialog) without writing.
        let ro_close = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!CloseReadOnlySave"), AutoValue::text("fixture.xlsm")]);
        assert!(matches!(&ro_close, Err(AutoError::NotModelled { owner: "dc-oxh8w.75", .. })), "{ro_close:?}");
        assert!(saved.borrow().is_empty(), "read-only Close(True) must not write bytes");
        // Workbook.Close(True) on a writable opened workbook writes saved bytes that reopen with the edit.
        let rw_close = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!CloseWritableSave"), AutoValue::text("fixture.xlsm")]);
        assert_eq!(text(rw_close), "closed");
        let written = saved.borrow();
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].0, "fixture.xlsm");
        let reopened = HostSession::open(
            Arc::new(MemorySource::new(written[0].1.clone()).named("fixture.xlsm")),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        let sheet = reopened.host().model().sheets()[0].id();
        let a1 = reopened.host().model().cell(calc_value::CellKey::new(sheet, 1, 1).unwrap());
        assert!(matches!(a1, Some(calc_host::CellInput::Number(n)) if n.get() == 123.0), "{a1:?}");
    }

    /// dc-nxq05: one sheet of several copied out of a macro-enabled workbook makes a workbook of that sheet.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn copy_of_one_sheet_of_a_macro_workbook_makes_a_new_workbook() {
        let (mut session, owner) = project();
        second_book_from("Other.xlsm", OTHER, include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/provider.xlsm"));
        crate::install_application(crate::ApplicationOptions {
            max_steps: 1_000_000,
            backend: Some(calc_vba::Backend::Interpreter),
            loader: None,
            saver: None,
            grant: None,
        });
        let count = match run(&mut session, &owner, &[AutoValue::text("Other.xlsm!BookCount")]) {
            Ok(AutoValue::R8(n)) => n as i64,
            other => panic!("{other:?}"),
        };
        let one = run(&mut session, &owner, &[AutoValue::text("Other.xlsm!CopyOneOfTwo")]);
        assert_eq!(text(one), format!("1/{}/{}", 2, count + 1), "{count}");
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
Public Function SeeOther() As Double
 Workbooks(2).Worksheets(1).Range("C3").Value = 41
 SeeOther = Workbooks(2).Worksheets(1).Range("C3").Value + 1
End Function
Public Function ReadMine(ByVal address As String) As Double
 ReadMine = ThisWorkbook.Worksheets(1).Range(address).Value
End Function
Public Function OtherSheets() As Double
 OtherSheets = Workbooks(2).Worksheets.Count
End Function
Public Function CloseOther() As Double
 Workbooks(2).Close False
 CloseOther = Workbooks.Count * 10 + Application.Run("Module1.Answer", 1, 1)
End Function
Public Function RunBare(ByVal n As String) As String
 RunBare = Application.Run(n)
End Function
Public Function QualifiedSelf() As Double
 Dim n As String
 n = "Test Book.xlsm!Module1.Answer"
 QualifiedSelf = Application.Run(n, 1, 1)
End Function
Public Function CloseSelf() As Double
 ThisWorkbook.Close False
End Function
Public Function CloseAll() As Double
 Workbooks.Close
End Function
Public Function ActiveAfterClose() As String
 Workbooks(2).Activate
 Workbooks(2).Close False
 ActiveAfterClose = ActiveWorkbook.Name
End Function
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
