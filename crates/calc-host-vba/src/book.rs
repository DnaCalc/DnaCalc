//! The application's open workbooks (dc-oxh8w.42): a workbook that owns its session can be run by
//! another workbook's macro (`Application.Run "Book.xlsm!Name"`), and `Workbooks.Add` and
//! `Workbooks.Open` make such workbooks. The registry is `calc_host::automation`'s.
use super::{ModuleSource, ProjectGrant, WorkbookEvents};
use calc_host::{
    HostOptions, HostSession, MemorySource, OpenPolicy,
    automation::{BookFactory, BookRunner, set_book_factory},
};
use calc_object_model::{AutoError, AutoValue, Call, ObjectModel, ObjectRef};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

type Reply = Result<AutoValue, AutoError>;

/// A workbook whose session the application holds.
struct OwnedBook {
    events: Rc<WorkbookEvents>,
    session: RefCell<HostSession>,
}

impl OwnedBook {
    /// Run `call` with this workbook's session lent; when it is already lent (its own macro is
    /// running further up), the loan it holds serves.
    fn lent<T>(&self, busy: impl FnOnce() -> T, call: impl FnOnce() -> T) -> T {
        match self.session.try_borrow_mut() {
            Ok(mut session) => self.events.automation.with_event_session(&mut session, self.events.file_project, call),
            Err(_) if self.events.automation.is_lent() => call(),
            Err(_) => busy(),
        }
    }
}

impl BookRunner for OwnedBook {
    fn run(&self, name: &str, request: &str, arguments: &[AutoValue]) -> Reply {
        self.lent(|| Err(calc_host::automation::busy()), || self.events.run_here(name, request, arguments))
    }
    fn invoke(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.lent(|| Err(calc_host::automation::busy()), || self.events.automation.invoke(target, call))
    }
    fn close(&self) -> Result<bool, AutoError> {
        // A session borrowed or lent now belongs to a macro that is running in this workbook.
        let mut session = self.session.try_borrow_mut().map_err(|_| {
            AutoError::not_modelled("Workbook.Close of a workbook whose macro is running", "dc-oxh8w.62")
        })?;
        if self.events.automation.is_lent() {
            return Err(AutoError::not_modelled("Workbook.Close of a workbook whose macro is running", "dc-oxh8w.62"));
        }
        Ok(!session.notify_before_close())
    }
}

/// The running workbook has no session of its own to lend: it answers while its macro is running.
pub(crate) struct LentBook(pub std::rc::Weak<WorkbookEvents>);

impl LentBook {
    fn events(&self) -> Result<Rc<WorkbookEvents>, AutoError> {
        let events = self.0.upgrade().ok_or(AutoError::Disconnected)?;
        if events.automation.is_lent() { Ok(events) } else { Err(calc_host::automation::busy()) }
    }
}

impl BookRunner for LentBook {
    fn run(&self, name: &str, request: &str, arguments: &[AutoValue]) -> Reply {
        self.events()?.run_here(name, request, arguments)
    }
    fn invoke(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.events()?.automation.invoke(target, call)
    }
    fn close(&self) -> Result<bool, AutoError> {
        Err(AutoError::not_modelled("Workbook.Close of the workbook whose macro is running", "dc-oxh8w.62"))
    }
}

impl WorkbookEvents {
    /// Hand this project's session to the application: the workbook is open for the other
    /// workbooks' `Workbooks(...)` and `Application.Run`. Install the project in the session
    /// first. `activate` makes it the active workbook.
    pub fn adopt(self: &Rc<Self>, session: HostSession, activate: bool) {
        let runner = Rc::new(OwnedBook { events: self.clone(), session: RefCell::new(session) });
        self.automation.register_owned(runner, activate);
    }
}

/// How `Workbooks.Add` and `Workbooks.Open` build workbooks.
pub struct ApplicationOptions {
    pub max_steps: u64,
    pub backend: Option<calc_vba::Backend>,
    /// The file bytes of a path `Workbooks.Open` names; `None` leaves `Open` refused. The host
    /// supplies it (the desktop reads the file system, a page its file store).
    pub loader: Option<Rc<dyn Fn(&str) -> Result<Vec<u8>, String>>>,
    /// Lets an opened file's own VBA project run; without it a file opens with no macros.
    pub grant: Option<ProjectGrant>,
}

struct Factory {
    options: ApplicationOptions,
    blank: Cell<u32>,
}

impl Factory {
    fn create(&self, name: &str, bytes: Vec<u8>, grant: Option<&ProjectGrant>) -> Result<u64, AutoError> {
        let fail = |what: String| AutoError::excel(1004, what);
        let mut session = HostSession::open(
            Arc::new(MemorySource::new(bytes).named(name)),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .map_err(|why| fail(format!("'{name}' could not be opened: {why:?}")))?;
        let project = match grant {
            Some(grant) => crate::open_file_project(
                grant,
                &mut session,
                name,
                self.options.max_steps,
                self.options.backend,
            )
            .map_err(|why| fail(why.to_string()))?,
            None => None,
        };
        let events = match project {
            Some(project) => project,
            None => crate::prepare(
                session.host(),
                name,
                &[ModuleSource {
                    name: "Module1".into(),
                    source: "Option Explicit\n".into(),
                    target: crate::ModuleTarget::Standard,
                }],
                self.options.max_steps,
                self.options.backend,
            )
            .map_err(fail)?,
        };
        let id = events.automation.workbook_identity()?;
        events.adopt(session, true);
        Ok(id)
    }
}

impl Factory {
    fn read(&self, path: &str) -> Result<Vec<u8>, AutoError> {
        let loader = self.options.loader.as_ref().ok_or_else(|| {
            AutoError::not_modelled("Workbooks.Open without a file loader", "dc-oxh8w.62")
        })?;
        loader(path).map_err(|_| {
            AutoError::excel(
                1004,
                format!("'{path}' could not be found. Check the spelling of the file name, and verify that the file location is correct."),
            )
        })
    }
}

impl BookFactory for Factory {
    fn add(&self) -> Result<u64, AutoError> {
        let n = self.blank.get() + 1;
        self.blank.set(n);
        self.create(&format!("Book{n}"), calc_doc::blank_package(), None)
    }
    fn add_from(&self, path: &str) -> Result<u64, AutoError> {
        let bytes = self.read(path)?;
        let n = self.blank.get() + 1;
        self.blank.set(n);
        // A new workbook made from a template is named BookN and has the template's contents and macros.
        self.create(&format!("Book{n}"), bytes, self.options.grant.as_ref())
    }
    fn open(&self, path: &str) -> Result<u64, AutoError> {
        let bytes = self.read(path)?;
        self.create(path, bytes, self.options.grant.as_ref())
    }
}

/// Install the workbook factory of this application thread.
pub fn install_application(options: ApplicationOptions) {
    set_book_factory(Some(Rc::new(Factory { options, blank: Cell::new(0) })));
}
