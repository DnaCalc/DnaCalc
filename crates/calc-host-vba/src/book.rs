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
    collections::HashMap,
    rc::Rc,
    sync::Arc,
};

type Reply = Result<AutoValue, AutoError>;
type FileSaver = Rc<dyn Fn(&str, &[u8]) -> Result<(), String>>;

/// A workbook whose session the application holds.
struct OwnedBook {
    events: Rc<WorkbookEvents>,
    session: RefCell<HostSession>,
    saver: Option<FileSaver>,
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
    fn knows(&self, request: &str) -> bool {
        self.events.has_procedure(request)
    }
    fn invoke(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.lent(|| Err(calc_host::automation::busy()), || self.events.automation.invoke(target, call))
    }
    fn close(&self) -> Result<bool, AutoError> {
        // A session borrowed or lent now belongs to a macro that is running in this workbook.
        let mut session = self.session.try_borrow_mut().map_err(|_| {
            AutoError::not_modelled("Workbook.Close of a workbook whose macro is running", "dc-oxh8w.75")
        })?;
        if self.events.automation.is_lent() {
            return Err(AutoError::not_modelled("Workbook.Close of a workbook whose macro is running", "dc-oxh8w.75"));
        }
        Ok(!session.notify_before_close())
    }
    fn close_saving(&self, destination: &str) -> Result<bool, AutoError> {
        let mut session = self.session.try_borrow_mut().map_err(|_| {
            AutoError::not_modelled("Workbook.Close of a workbook whose macro is running", "dc-oxh8w.75")
        })?;
        if self.events.automation.is_lent() {
            return Err(AutoError::not_modelled("Workbook.Close of a workbook whose macro is running", "dc-oxh8w.75"));
        }
        let Some(saver) = &self.saver else {
            return Err(AutoError::not_modelled(
                "Workbook.Close with SaveChanges True without a file saver",
                "dc-oxh8w.75",
            ));
        };
        if session.notify_before_close() {
            return Ok(false);
        }
        let mut bytes = Vec::new();
        match session.save_without_after_event(&calc_host::SavePolicy::default(), &mut bytes) {
            Ok(_) => {}
            Err(calc_host::SaveFailed::Cancelled) => return Err(AutoError::excel(1004, "Close: the save was cancelled")),
            Err(why) => {
                session.notify_after_save(false);
                return Err(AutoError::excel(1004, why.to_string()));
            }
        }
        // AfterSave follows the write: it reports whether the file reached its destination.
        let written = saver(destination, &bytes);
        session.notify_after_save(written.is_ok());
        written.map_err(|why| AutoError::excel(1004, why))?;
        Ok(true)
    }
    fn keep_sheets(&self, names: Vec<String>) -> Result<(), AutoError> {
        self.lent(|| Err(calc_host::automation::busy()), || self.events.automation.keep_sheets(&names))
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
    fn knows(&self, request: &str) -> bool {
        self.0.upgrade().is_some_and(|events| events.has_procedure(request))
    }
    fn invoke(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.events()?.automation.invoke(target, call)
    }
    fn close(&self) -> Result<bool, AutoError> {
        Err(AutoError::not_modelled("Workbook.Close of the workbook whose macro is running", "dc-oxh8w.75"))
    }
    fn keep_sheets(&self, names: Vec<String>) -> Result<(), AutoError> {
        self.events()?.automation.keep_sheets(&names)
    }
}

impl WorkbookEvents {
    /// Hand this project's session to the application: the workbook is open for the other
    /// workbooks' `Workbooks(...)` and `Application.Run`. Install the project in the session
    /// first. `activate` makes it the active workbook.
    pub fn adopt(self: &Rc<Self>, session: HostSession, activate: bool) {
        self.adopt_with_saver(session, activate, None);
    }
    fn adopt_with_saver(self: &Rc<Self>, session: HostSession, activate: bool, saver: Option<FileSaver>) {
        let runner = Rc::new(OwnedBook { events: self.clone(), session: RefCell::new(session), saver });
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
    /// Writes saved bytes when `Workbook.Close(SaveChanges:=True)` saves a workbook; `None` refuses.
    pub saver: Option<FileSaver>,
    /// Lets an opened file's own VBA project run; without it a file opens with no macros.
    pub grant: Option<ProjectGrant>,
}

struct Factory {
    options: ApplicationOptions,
    blank: Cell<u32>,
    stems: RefCell<HashMap<String, u32>>,
}

impl Factory {
    fn create(
        &self,
        name: &str,
        bytes: Vec<u8>,
        grant: Option<&ProjectGrant>,
        read_only: bool,
    ) -> Result<u64, AutoError> {
        let fail = |what: String| AutoError::excel(1004, what);
        let mut session = HostSession::open(
            Arc::new(MemorySource::new(bytes).named(name)),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .map_err(|why| fail(format!("'{name}' could not be opened: {why:?}")))?;
        let project = match grant {
            Some(grant) => crate::prepare_file_project_with_events(
                grant,
                session.host(),
                name,
                self.options.max_steps,
                self.options.backend,
                session.workbook_events_enabled(),
            )
            .map_err(|why| fail(why.to_string()))?,
            None => None,
        };
        let has_file_project = project.is_some();
        let events = match project {
            Some(project) => {
                project.clone().install(&mut session);
                project
            }
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
        calc_host::automation::set_book_read_only(id, read_only);
        let runner = Rc::new(OwnedBook {
            events: events.clone(),
            session: RefCell::new(session),
            saver: self.options.saver.clone(),
        });
        events.automation.register_owned(runner.clone(), true);
        // EnableEvents is off in the opener: Excel raises no Workbook_Open (the switch itself is per book here).
        if has_file_project && calc_host::automation::opener_events_enabled() {
            runner.session.borrow_mut().notify_workbook_open();
        }
        Ok(id)
    }
}

/// File stem for `Workbooks.Add(Template:=path)` naming (ww21: `Data.xlsx` -> `Data1`).
fn template_stem(path: &str) -> &str {
    let base = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let stem = match base.rfind('.') {
        Some(dot) if dot > 0 => &base[..dot],
        _ => base,
    };
    if stem.is_empty() { "Book" } else { stem }
}

const OLE2_MAGIC: &[u8; 8] = &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

impl Factory {
    fn read(&self, path: &str) -> Result<Vec<u8>, AutoError> {
        let loader = self.options.loader.as_ref().ok_or_else(|| {
            AutoError::not_modelled("Workbooks.Open without a file loader", "dc-oxh8w.75")
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
        self.create(&format!("Book{n}"), calc_doc::blank_package(), None, false)
    }
    fn add_from(&self, path: &str) -> Result<u64, AutoError> {
        let bytes = self.read(path)?;
        let stem = template_stem(path);
        // The name is reserved before `create` runs the book's Workbook_Open, which may add another
        // template book itself: the borrow is released first, and a failed create gives the number back.
        let key = stem.to_ascii_lowercase();
        let count = {
            let mut stems = self.stems.borrow_mut();
            let count = stems.entry(key.clone()).or_insert(0);
            *count += 1;
            *count
        };
        let name = format!("{stem}{count}");
        // ww21 Workbooks.Add.Template: a workbook made from Data.xlsx is named Data1 (empty Path).
        let made = self.create(&name, bytes, self.options.grant.as_ref(), false);
        if made.is_err() {
            if let Some(now) = self.stems.borrow_mut().get_mut(&key) {
                if *now == count {
                    *now -= 1;
                }
            }
        }
        made
    }
    fn add_package(&self, bytes: Vec<u8>) -> Result<u64, AutoError> {
        let n = self.blank.get() + 1;
        self.blank.set(n);
        self.create(&format!("Book{n}"), bytes, None, false)
    }
    fn open(&self, path: &str) -> Result<u64, AutoError> {
        self.open_with_options(path, false, None)
    }
    fn open_with_options(
        &self,
        path: &str,
        read_only: bool,
        password: Option<&str>,
    ) -> Result<u64, AutoError> {
        let bytes = self.read(path)?;
        if bytes.starts_with(OLE2_MAGIC) && password.is_some_and(|p| !p.is_empty()) {
            return Err(AutoError::not_modelled(
                "Workbooks.Open Password for encrypted workbook",
                "dc-oxh8w.75",
            ));
        }
        // ww21 Workbooks.Open.password.unencrypted: Password is ignored for unencrypted files.
        self.create(path, bytes, self.options.grant.as_ref(), read_only)
    }
}

/// Install the workbook factory of this application thread.
pub fn install_application(options: ApplicationOptions) {
    set_book_factory(Some(Rc::new(Factory {
        options,
        blank: Cell::new(0),
        stems: RefCell::new(HashMap::new()),
    })));
}
