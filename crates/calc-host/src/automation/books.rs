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
    /// One call on an object of this workbook (a sheet, a range, a name), made from another
    /// workbook's macro: run against this workbook's own session.
    fn invoke(&self, target: &ObjectRef, call: &Call<'_>) -> Reply;
    /// `Workbook.Close` of this workbook: raises `BeforeClose`; `Ok(false)` when a handler
    /// cancelled it. `Err` when the workbook cannot be closed now (its macro is running).
    fn close(&self) -> Result<bool, AutoError>;
}

/// Makes new workbooks for `Workbooks.Add` and `Workbooks.Open`; the host installs one.
pub trait BookFactory {
    /// A new blank workbook, registered and active. Returns its workbook identity.
    fn add(&self) -> Result<u64, AutoError>;
    /// The workbook at `path`, registered and active. Returns its workbook identity.
    fn open(&self, path: &str) -> Result<u64, AutoError>;
    /// A new workbook from the template file at `path` (`Workbooks.Add Template:=path`).
    fn add_from(&self, path: &str) -> Result<u64, AutoError>;
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
    /// The application owns this workbook's session (`Workbooks.Add`/`Open`, or adopted), so it can close it.
    owned: bool,
}

struct Books {
    entries: Vec<Entry>,
    next: u64,
    active: u64,
    /// Activation history, most recent first: the workbook that becomes active when the active one closes.
    history: Vec<u64>,
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
    static BOOKS: RefCell<Books> = RefCell::new(Books { entries: Vec::new(), next: 1, active: 0, history: Vec::new(), factory: None });
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
    pub automation: Option<Rc<WorkbookAutomation>>,
    pub owned: bool,
    pub full_name: String,
    pub runner: Option<Rc<dyn BookRunner>>,
}

impl WorkbookAutomation {
    /// Add this workbook to the application's open workbooks (once). The first one open is active.
    pub fn register(self: &Rc<Self>) {
        self.register_with(None, false, false);
    }
    /// Give this registered workbook a runner that does not own its session: it answers while the
    /// session is lent to the workbook's own macro (`Application.Run` back into the running workbook).
    pub fn set_lent_runner(self: &Rc<Self>, runner: Rc<dyn BookRunner>) {
        self.register_with(Some(runner), false, false);
    }
    /// Register with the loan of its own session, so other workbooks can run it. `activate` makes
    /// it the active workbook, as opening or adding one does.
    pub fn register_owned(self: &Rc<Self>, runner: Rc<dyn BookRunner>, activate: bool) {
        self.register_with(Some(runner), activate, true);
    }
    fn register_with(self: &Rc<Self>, runner: Option<Rc<dyn BookRunner>>, activate: bool, owned: bool) {
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
                        // A lent runner never replaces the runner of a session the application owns.
                        if owned || !entry.owned {
                            entry.runner = runner;
                            entry.owned |= owned;
                        }
                    }
                }
                None => books.entries.push(Entry {
                    automation: Rc::downgrade(self),
                    runner,
                    owned,
                }),
            }
            if activate || books.active == 0 {
                books.active = meta.workbook;
                books.history.retain(|id| *id != meta.workbook);
                books.history.insert(0, meta.workbook);
            } else if !books.history.contains(&meta.workbook) {
                books.history.push(meta.workbook);
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
                Some(Listed { id: book.workbook, automation: Some(automation.clone()), owned: e.owned, full_name: book.full_name.clone(), runner: e.runner.clone() })
            }).collect()
        });
        if !listed.iter().any(|b| b.id == meta.workbook) {
            listed.push(Listed { id: meta.workbook, automation: None, owned: false, full_name: meta.full_name.clone(), runner: None });
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
        handlers.on("Workbooks", "Close", InvokeKind::Method, Self::books_close);
        handlers.on("Workbook", "Close", InvokeKind::Method, Self::workbook_close_own);
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
            // Template: the default worksheet workbook (xlWBATWorksheet = -4167), or a template file.
            let factory = self.factory()?;
            let id = match call.arg(0) {
                AutoValue::Missing => factory.add()?,
                AutoValue::Text(path) => factory.add_from(&path.to_string_lossy())?,
                other => match template_kind(other) {
                    Some(-4167) => factory.add()?,
                    Some(-4109 | 3 | 4) => {
                        return Err(AutoError::not_modelled("Workbooks.Add of a chart or macro-sheet workbook", "dc-oxh8w.64"));
                    }
                    _ => return Err(AutoError::excel(1004, "Workbooks.Add: Template must be a file name or an XlWBATemplate")),
                },
            };
            Ok(Self::book_object(id))
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
            open_options(call)?;
            Ok(Self::book_object(self.factory()?.open(&path.to_string_lossy())?))
        })
    }
    fn make_active(id: u64) {
        BOOKS.with(|books| {
            let mut books = books.borrow_mut();
            books.active = id;
            books.history.retain(|h| *h != id);
            books.history.insert(0, id);
        });
    }
    /// `Workbook.Close` of the workbook the call comes from: it is the one running, so it stays open.
    fn workbook_close_own(&self, target: &ObjectRef, _call: &Call<'_>) -> Reply {
        self.with_target(target, |_, _| {
            Err(AutoError::not_modelled("Workbook.Close of the workbook whose macro is running", "dc-oxh8w.64"))
        })
    }
    fn workbook_activate(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            no_args(call)?;
            Self::make_active(meta.workbook);
            Ok(AutoValue::Empty)
        })
    }
    /// The workbook other than this one that holds the tracked range `reference`.
    fn range_holder(&self, reference: &StoredReference, books: &[Listed]) -> Option<u64> {
        if self.ranges.borrow().contains_key(reference) {
            return None;
        }
        let own = self.metadata().ok()?.workbook;
        books
            .iter()
            .filter(|b| b.id != own)
            .find(|b| b.automation.as_ref().is_some_and(|a| a.ranges.borrow().contains_key(reference)))
            .map(|b| b.id)
    }
    /// The other open workbook that owns `target`'s object, if it is not this one's.
    fn foreign_owner(&self, target: &ObjectRef, books: &[Listed]) -> Option<u64> {
        let id = match &target.key {
            ObjectKey::Workbook(id)
            | ObjectKey::Sheets(id)
            | ObjectKey::Worksheets(id)
            | ObjectKey::Charts(id)
            | ObjectKey::Names(id)
            | ObjectKey::Style { workbook: id, .. }
            | ObjectKey::Name { workbook: id, .. }
            | ObjectKey::Worksheet { workbook: id, .. }
            | ObjectKey::Report { workbook: id, .. } => *id,
            ObjectKey::Range(reference)
            | ObjectKey::RangeFormat(reference)
            | ObjectKey::RangeEdge { reference, .. }
            | ObjectKey::RangeView { reference, .. }
            | ObjectKey::Areas(reference) => return self.range_holder(reference, books),
            _ => return None,
        };
        (id != self.metadata().ok()?.workbook).then_some(id)
    }
    /// `released` of an object another open workbook holds goes to that workbook.
    pub(super) fn foreign_release(&self, target: &ObjectRef) -> bool {
        let Ok(books) = self.open_books() else { return false };
        let Some(id) = self.foreign_owner(target, &books) else { return false };
        match books.iter().find(|b| b.id == id).and_then(|b| b.automation.as_ref()) {
            Some(holder) => {
                holder.released(target);
                true
            }
            None => false,
        }
    }
    /// A call whose target belongs to another open workbook goes to that workbook's own session:
    /// the loan it already holds while its macro runs, or the runner its application-owned session answers.
    /// `None` when the target is this workbook's.
    pub(super) fn foreign_call(&self, target: &ObjectRef, call: &Call<'_>) -> Option<Reply> {
        let books = self.open_books().ok()?;
        let id = self.foreign_owner(target, &books)?;
        let book = books.into_iter().find(|b| b.id == id)?;
        if matches!(target.key, ObjectKey::Workbook(_)) && call.member.name == "Close" {
            return Some(self.close_book(&book, call));
        }
        if let Some(automation) = book.automation.as_ref().filter(|a| a.is_lent()) {
            return Some(automation.invoke(target, call));
        }
        if let Some(runner) = &book.runner {
            return Some(runner.invoke(target, call));
        }
        // A workbook with no session to route to still answers who it is.
        let identity = matches!(target.key, ObjectKey::Workbook(_))
            && matches!(call.member.name.as_str(), "Name" | "FullName" | "Path" | "Activate");
        if !identity {
            return Some(Err(AutoError::not_modelled(
                format!("{} of another open workbook without a session", call.label()),
                "dc-oxh8w.64",
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
                Self::make_active(id);
                AutoValue::Empty
            }
        }))
    }
    /// `Workbook.Close(SaveChanges, Filename, RouteWorkbook)` of another open workbook.
    fn close_book(&self, book: &Listed, call: &Call<'_>) -> Reply {
        if call.args.len() > 3 {
            return Err(geometry::bad_count());
        }
        let flag = |index: usize| match call.arg(index) {
            AutoValue::Missing => Ok(None),
            AutoValue::Bool(b) => Ok(Some(*b)),
            _ => Err(AutoError::excel(1004, "Workbook.Close: SaveChanges and RouteWorkbook must be Boolean")),
        };
        let save = flag(0)?;
        if !matches!(call.arg(1), AutoValue::Missing | AutoValue::Text(_)) {
            return Err(AutoError::excel(1004, "Workbook.Close: Filename must be text"));
        }
        flag(2)?;
        if save == Some(true) {
            return Err(AutoError::not_modelled("Workbook.Close with SaveChanges True (saving the closed workbook)", "dc-oxh8w.64"));
        }
        let runner = book.runner.as_ref().filter(|_| book.owned).ok_or_else(|| {
            AutoError::not_modelled("Workbook.Close of a workbook the host opened", "dc-oxh8w.64")
        })?;
        // Without SaveChanges Excel asks about unsaved changes; DNA Calc's macro host never asks, so the
        // changes are dropped (as with DisplayAlerts off).
        if !runner.close()? {
            return Ok(AutoValue::Empty);
        }
        Self::unregister(book.id);
        Ok(AutoValue::Empty)
    }
    fn unregister(id: u64) {
        BOOKS.with(|books| {
            let mut books = books.borrow_mut();
            books.entries.retain(|e| e.automation.upgrade().is_some_and(|a| a.workbook_identity().is_ok_and(|w| w != id)));
            books.history.retain(|h| *h != id);
            if books.active == id {
                books.active = books.history.first().copied().unwrap_or(0);
            }
        });
    }
    /// `Workbooks.Close`: closes every open workbook.
    fn books_close(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, _| {
            no_args(call)?;
            let books = self.open_books()?;
            let own = self.metadata()?.workbook;
            if books.iter().any(|b| b.id == own || !b.owned || b.automation.as_ref().is_some_and(|a| a.is_lent())) {
                return Err(AutoError::not_modelled(
                    "Workbooks.Close including the workbook whose macro is running",
                    "dc-oxh8w.64",
                ));
            }
            for book in &books {
                if book.runner.as_ref().ok_or_else(busy)?.close()? {
                    Self::unregister(book.id);
                }
            }
            Ok(AutoValue::Empty)
        })
    }
}

fn index_book(books: &[Listed], index: f64) -> Option<&Listed> {
    if index.fract() != 0. || index < 1. {
        return None;
    }
    books.get(index as usize - 1)
}

/// An `XlWBATemplate` argument of `Workbooks.Add` as its number.
fn template_kind(value: &AutoValue) -> Option<i32> {
    match value {
        AutoValue::I2(n) => Some(i32::from(*n)),
        AutoValue::I4(n) => Some(*n),
        AutoValue::R8(n) if n.fract() == 0. && n.abs() < 1e9 => Some(*n as i32),
        _ => None,
    }
}

/// The options of `Workbooks.Open` after FileName: UpdateLinks, ReadOnly, Format, Password,
/// WriteResPassword, IgnoreReadOnlyRecommended, Origin, Delimiter, Editable, Notify, Converter,
/// AddToMru, Local, CorruptLoad. A wrong type is run-time error 1004. The options that only matter
/// to a text file, a link prompt, a read-only recommendation or the recent-file list change nothing
/// for a workbook opened here; the others are refused (dc-oxh8w.64).
fn open_options(call: &Call<'_>) -> Result<(), AutoError> {
    let wrong = |name: &str| AutoError::excel(1004, format!("Workbooks.Open: the {name} argument has the wrong type"));
    let boolean = |index: usize, name: &str| match call.arg(index) {
        AutoValue::Missing | AutoValue::Empty => Ok(None),
        AutoValue::Bool(b) => Ok(Some(*b)),
        _ => Err(wrong(name)),
    };
    let number = |index: usize, name: &str| match call.arg(index) {
        AutoValue::Missing | AutoValue::Empty => Ok(None),
        other => template_kind(other).map(Some).ok_or_else(|| wrong(name)),
    };
    let text = |index: usize, name: &str| match call.arg(index) {
        AutoValue::Missing | AutoValue::Empty => Ok(None),
        AutoValue::Text(t) => Ok(Some(t.to_string_lossy())),
        _ => Err(wrong(name)),
    };
    // UpdateLinks (xlUpdateLinksUserSetting 1 ... 0-3) is not validated by value: Excel treats 0 and 2 and 3 alike for a file without links.
    number(1, "UpdateLinks")?;
    if boolean(2, "ReadOnly")? == Some(true) {
        return Err(AutoError::not_modelled("Workbooks.Open ReadOnly:=True", "dc-oxh8w.64"));
    }
    number(3, "Format")?;
    if text(4, "Password")?.is_some_and(|p| !p.is_empty()) {
        return Err(AutoError::not_modelled("Workbooks.Open Password", "dc-oxh8w.64"));
    }
    if text(5, "WriteResPassword")?.is_some_and(|p| !p.is_empty()) {
        return Err(AutoError::not_modelled("Workbooks.Open WriteResPassword", "dc-oxh8w.64"));
    }
    boolean(6, "IgnoreReadOnlyRecommended")?;
    number(7, "Origin")?;
    text(8, "Delimiter")?;
    boolean(9, "Editable")?;
    boolean(10, "Notify")?;
    number(11, "Converter")?;
    boolean(12, "AddToMru")?;
    boolean(13, "Local")?;
    if number(14, "CorruptLoad")?.is_some_and(|n| n != 0) {
        return Err(AutoError::not_modelled("Workbooks.Open CorruptLoad other than xlNormalLoad", "dc-oxh8w.64"));
    }
    Ok(())
}
