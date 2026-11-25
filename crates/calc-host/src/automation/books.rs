//! The application's open workbooks (dc-oxh8w.42): `Workbooks` Count/Item/Add/Open, `ActiveWorkbook`,
//! `Workbook.Activate`, and the route a call takes into another open workbook. One registry per
//! application thread; each open workbook is one [`WorkbookAutomation`] (its identity is the
//! `ObjectKey::Workbook` id it issues). A workbook the application owns also has a [`BookRunner`]:
//! the loan of its own session, so a macro in another workbook can run it.
use super::*;

/// The application's way into one workbook that owns its session.
pub trait BookRunner {
    /// `Application.Run` of `request` (an unqualified `Module.Name` or `Name`) in this workbook.
    fn run(&self, name: &str, request: &str, arguments: &[AutoValue]) -> Reply;
}

/// Makes new workbooks for `Workbooks.Add` and `Workbooks.Open`; the host installs one.
pub trait BookFactory {
    /// A new blank workbook, registered and active. Returns its workbook identity.
    fn add(&self) -> Result<u64, AutoError>;
    /// The workbook at `path`, registered and active. Returns its workbook identity.
    fn open(&self, path: &str) -> Result<u64, AutoError>;
}

/// See [`WorkbookAutomation::locate_book`].
pub enum BookTarget {
    /// The workbook the call comes from.
    This,
    /// Another open workbook; the runner is `None` while that workbook's session is lent out
    /// (it is the one running).
    Other(Option<Rc<dyn BookRunner>>),
    /// No open workbook has that name.
    Closed,
}

struct Entry {
    automation: std::rc::Weak<WorkbookAutomation>,
    runner: Option<Rc<dyn BookRunner>>,
}

struct Books {
    entries: Vec<Entry>,
    next: u64,
    active: u64,
    factory: Option<Rc<dyn BookFactory>>,
}

impl Drop for Books {
    /// The open workbooks live as long as the application thread. Dropping them while the thread
    /// tears down its other thread-locals would touch destroyed ones, so they are left to the process.
    fn drop(&mut self) {
        std::mem::forget(std::mem::take(&mut self.entries));
    }
}

thread_local! {
    static BOOKS: RefCell<Books> = RefCell::new(Books { entries: Vec::new(), next: 1, active: 0, factory: None });
}

/// The next workbook identity of this application; identities are unique across its workbooks.
pub(super) fn next_identity() -> u64 {
    BOOKS.with(|books| {
        let mut books = books.borrow_mut();
        let id = books.next;
        books.next = id.checked_add(1).expect("workbook identity exhausted");
        id
    })
}

/// Install the factory `Workbooks.Add` and `Workbooks.Open` use on this thread.
pub fn set_book_factory(factory: Option<Rc<dyn BookFactory>>) {
    BOOKS.with(|books| books.borrow_mut().factory = factory);
}

/// One open workbook as the collection lists it.
pub(super) struct Listed {
    pub id: u64,
    pub full_name: String,
    pub runner: Option<Rc<dyn BookRunner>>,
}

impl WorkbookAutomation {
    /// Add this workbook to the application's open workbooks (once). The first one open is active.
    pub fn register(self: &Rc<Self>) {
        self.register_with(None, false);
    }
    /// Register with the loan of its own session, so other workbooks can run it. `activate` makes
    /// it the active workbook, as opening or adding one does.
    pub fn register_owned(self: &Rc<Self>, runner: Rc<dyn BookRunner>, activate: bool) {
        self.register_with(Some(runner), activate);
    }
    fn register_with(self: &Rc<Self>, runner: Option<Rc<dyn BookRunner>>, activate: bool) {
        let Ok(meta) = self.metadata() else { return };
        BOOKS.with(|books| {
            let mut books = books.borrow_mut();
            books.entries.retain(|e| e.automation.strong_count() > 0);
            let existing = books.entries.iter_mut().find(|e| {
                e.automation.upgrade().is_some_and(|a| Rc::ptr_eq(&a, self))
            });
            match existing {
                Some(entry) => {
                    if runner.is_some() {
                        entry.runner = runner;
                    }
                }
                None => books.entries.push(Entry {
                    automation: Rc::downgrade(self),
                    runner,
                }),
            }
            if activate || books.active == 0 {
                books.active = meta.workbook;
            }
        });
    }
    /// The open workbooks in opening order, with this one when it was never registered.
    pub(super) fn open_books(&self) -> Result<Vec<Listed>, AutoError> {
        let meta = self.metadata()?;
        let mut listed: Vec<Listed> = BOOKS.with(|books| {
            books.borrow().entries.iter().filter_map(|e| {
                let automation = e.automation.upgrade()?;
                let book = automation.metadata().ok()?;
                Some(Listed { id: book.workbook, full_name: book.full_name.clone(), runner: e.runner.clone() })
            }).collect()
        });
        if !listed.iter().any(|b| b.id == meta.workbook) {
            listed.push(Listed { id: meta.workbook, full_name: meta.full_name.clone(), runner: None });
        }
        Ok(listed)
    }
    /// This workbook's identity in the application.
    pub fn workbook_identity(&self) -> Result<u64, AutoError> {
        Ok(self.metadata()?.workbook)
    }
    /// Where `Application.Run "Book!Name"` goes: this workbook, another open one, or none.
    pub fn locate_book(&self, name: &str) -> Result<BookTarget, AutoError> {
        if self.matches_workbook_name(name)? {
            return Ok(BookTarget::This);
        }
        Ok(self
            .open_books()?
            .into_iter()
            .find(|book| {
                calc_value::workbook_names_equal(&book.full_name, name)
                    || calc_value::workbook_names_equal(book_name(&book.full_name), name)
            })
            .map_or(BookTarget::Closed, |book| BookTarget::Other(book.runner)))
    }
    pub(super) fn register_books(handlers: &mut Handlers<Self>) {
        handlers.on("Application", "ActiveWorkbook", InvokeKind::Get, Self::active_workbook);
        handlers.on("Workbooks", "Count", InvokeKind::Get, Self::books_count);
        handlers.on("Workbooks", "Item", InvokeKind::Get, Self::books_item);
        handlers.on("Workbooks", "_Default", InvokeKind::Get, Self::books_item);
        handlers.on("Workbooks", "Add", InvokeKind::Method, Self::books_add);
        handlers.on("Workbooks", "Open", InvokeKind::Method, Self::books_open);
        handlers.on("Workbook", "Activate", InvokeKind::Method, Self::workbook_activate);
    }
    fn book_object(id: u64) -> AutoValue {
        AutoValue::Object(object("Workbook", ObjectKey::Workbook(id)))
    }
    fn active_workbook(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            no_args(call)?;
            let books = self.open_books()?;
            let active = BOOKS.with(|books| books.borrow().active);
            let id = books.iter().find(|b| b.id == active).map_or(meta.workbook, |b| b.id);
            Ok(Self::book_object(id))
        })
    }
    fn books_count(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, _| {
            no_args(call)?;
            Ok(AutoValue::I4(self.open_books()?.len() as i32))
        })
    }
    fn books_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, _| {
            if call.args.len() != 1 {
                return Err(geometry::bad_count());
            }
            let books = self.open_books()?;
            let found = match call.arg(0) {
                AutoValue::Text(text) => {
                    let wanted = text.to_string_lossy();
                    books.iter().find(|b| {
                        wanted.eq_ignore_ascii_case(book_name(&b.full_name))
                            || wanted.eq_ignore_ascii_case(&b.full_name)
                    })
                }
                AutoValue::I2(n) => index_book(&books, f64::from(*n)),
                AutoValue::I4(n) => index_book(&books, f64::from(*n)),
                AutoValue::R4(n) => index_book(&books, f64::from(*n)),
                AutoValue::R8(n) => index_book(&books, *n),
                _ => None,
            };
            found
                .map(|b| Self::book_object(b.id))
                .ok_or(AutoError::Status(0x8002_000Bu32 as i32))
        })
    }
    fn factory(&self) -> Result<Rc<dyn BookFactory>, AutoError> {
        BOOKS
            .with(|books| books.borrow().factory.clone())
            .ok_or_else(|| AutoError::not_modelled("Workbooks.Add and Workbooks.Open without a workbook factory", "dc-oxh8w.42"))
    }
    fn books_add(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, _| {
            if call.args.len() > 1 {
                return Err(geometry::bad_count());
            }
            // Template: only the default, a new worksheet workbook (xlWBATWorksheet = -4167).
            match call.arg(0) {
                AutoValue::Missing => {}
                AutoValue::I2(-4167) | AutoValue::I4(-4167) => {}
                AutoValue::R8(n) if *n == -4167. => {}
                _ => return Err(AutoError::not_modelled("Workbooks.Add Template", "dc-oxh8w.56")),
            }
            Ok(Self::book_object(self.factory()?.add()?))
        })
    }
    fn books_open(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, _| {
            if call.args.is_empty() || call.args.len() > 15 {
                return Err(geometry::bad_count());
            }
            let AutoValue::Text(path) = call.arg(0) else {
                return Err(AutoError::excel(1004, "Workbooks.Open: the file name must be text"));
            };
            if call.args.iter().skip(1).any(|a| !a.is_missing()) {
                return Err(AutoError::not_modelled("Workbooks.Open options after FileName", "dc-oxh8w.56"));
            }
            Ok(Self::book_object(self.factory()?.open(&path.to_string_lossy())?))
        })
    }
    fn workbook_activate(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            no_args(call)?;
            BOOKS.with(|books| books.borrow_mut().active = meta.workbook);
            Ok(AutoValue::Empty)
        })
    }
    /// A call whose target belongs to another open workbook goes to that workbook's own session.
    /// Only the members that read the workbook's identity route; the rest of its objects name
    /// ranges and sheets this automation does not hold (dc-oxh8w.56).
    pub(super) fn foreign_call(&self, target: &ObjectRef, call: &Call<'_>) -> Option<Reply> {
        let id = match target.key {
            ObjectKey::Workbook(id) => id,
            ObjectKey::Sheets(id)
            | ObjectKey::Worksheets(id)
            | ObjectKey::Charts(id)
            | ObjectKey::Names(id)
            | ObjectKey::Style { workbook: id, .. }
            | ObjectKey::Name { workbook: id, .. }
            | ObjectKey::Worksheet { workbook: id, .. }
            | ObjectKey::Report { workbook: id, .. } => id,
            _ => return None,
        };
        let own = self.metadata().ok()?.workbook;
        if id == own {
            return None;
        }
        let book = self.open_books().ok()?.into_iter().find(|b| b.id == id)?;
        let routed = matches!(target.key, ObjectKey::Workbook(_))
            && matches!(call.member.name.as_str(), "Name" | "FullName" | "Path" | "Activate");
        if !routed {
            return Some(Err(AutoError::not_modelled(
                format!("{} of another open workbook", call.label()),
                "dc-oxh8w.56",
            )));
        }
        if no_args(call).is_err() {
            return Some(Err(geometry::bad_count()));
        }
        Some(Ok(match call.member.name.as_str() {
            "Name" => AutoValue::text(book_name(&book.full_name)),
            "FullName" => AutoValue::text(&book.full_name),
            "Path" => AutoValue::text(book_path(&book.full_name)),
            _ => {
                BOOKS.with(|books| books.borrow_mut().active = id);
                AutoValue::Empty
            }
        }))
    }
}

fn index_book(books: &[Listed], index: f64) -> Option<&Listed> {
    if index.fract() != 0. || index < 1. {
        return None;
    }
    books.get(index as usize - 1)
}
