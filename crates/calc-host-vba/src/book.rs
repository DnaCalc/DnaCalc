//! The application's open workbooks (dc-oxh8w.42): a workbook that owns its session can be run by
//! another workbook's macro (`Application.Run "Book.xlsm!Name"`), and `Workbooks.Add` and
//! `Workbooks.Open` make such workbooks. The registry is `calc_host::automation`'s.
use super::{ModuleSource, ProjectGrant, WorkbookEvents};
use calc_host::{
    HostOptions, HostSession, MemorySource, OpenPolicy,
    automation::{BookFactory, BookRunner, set_book_factory},
};
use calc_object_model::{AutoError, AutoValue};
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

impl BookRunner for OwnedBook {
    fn run(&self, name: &str, request: &str, arguments: &[AutoValue]) -> Reply {
        let mut session = self.session.try_borrow_mut().map_err(|_| calc_host::automation::busy())?;
        self.events.automation.with_event_session(&mut session, self.events.file_project, || {
            self.events.run_here(name, request, arguments)
        })
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

impl BookFactory for Factory {
    fn add(&self) -> Result<u64, AutoError> {
        let n = self.blank.get() + 1;
        self.blank.set(n);
        self.create(&format!("Book{n}"), calc_doc::blank_package(), None)
    }
    fn open(&self, path: &str) -> Result<u64, AutoError> {
        let loader = self.options.loader.as_ref().ok_or_else(|| {
            AutoError::not_modelled("Workbooks.Open without a file loader", "dc-oxh8w.56")
        })?;
        let bytes = loader(path).map_err(|_| {
            AutoError::excel(
                1004,
                format!("'{path}' could not be found. Check the spelling of the file name, and verify that the file location is correct."),
            )
        })?;
        self.create(path, bytes, self.options.grant.as_ref())
    }
}

/// Install the workbook factory of this application thread.
pub fn install_application(options: ApplicationOptions) {
    set_book_factory(Some(Rc::new(Factory { options, blank: Cell::new(0) })));
}
