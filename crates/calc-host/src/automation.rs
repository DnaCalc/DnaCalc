//! DNA Calc's workbook object-model profile. The projection supplies no member semantics.
//! Worksheet reads use the admitted UserFunctionContext. Events lend their session through a
//! lifetime-scoped loan; commands borrow the application's session for one call. A failed
//! borrow is a typed refusal, and cannot fall through to a different read route.

use crate::{
    HostSession, ModelRoot, ProjectedValue, ProjectionRequest, ReferenceTarget,
    WorkbookEventObserver,
};
use calc_func::{ReferenceHandleError, ReferenceHandles, StoredReference, UserFunctionContext};
use calc_object_model::geometry::{self, AddressOptions, RangeAxis};
use calc_object_model::profile::{
    ApplicationHost,
    values_only::{ValuesOnly, object},
    worksheet_function,
};
use calc_object_model::{
    AutoArray, AutoError, AutoValue, Call, Descriptor, EventSource, Handlers, InvokeKind, ObjectKey,
    ObjectModel, ObjectRef,
};
use calc_value::{
    CalcValue, CellKey, CoreRef, GridArea, ReferenceLike, SheetId, WorksheetErrorCode,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::Arc,
};

const OWNER: &str = "dc-oxh8w";
type Reply = Result<AutoValue, AutoError>;
mod application;
mod books;
pub use books::{AppCalculate, BookFactory, BookRunner, BookTarget, opener_events_enabled, set_book_factory, set_book_read_only};

mod cells;
mod clipboard;
mod formatting;
mod window;
mod protection;
pub(crate) use protection::ProtectionSave;
pub(crate) use protection::style_locked;
mod find;
mod advanced_filter;
mod autofilter;
mod range_end;
mod formula;
mod mutation;
mod navigation;
mod select;
mod print;
mod report;
mod chart_axes;
mod chart_sheets;
mod chart_export;
mod chart_format;
mod chart_labels;
mod report_draw;
mod sheet_copy;
mod structure;
mod value;

/// The application supplies access, never a copied value reader. `None` means no admitted
/// worksheet invocation; `Some(Err(..))` must never fall back to the command route.
pub trait AutomationHost: ApplicationHost {
    /// Run the callback with the currently admitted worksheet invocation, if any.
    fn with_invocation(
        &self,
        call: &mut dyn FnMut(&mut dyn UserFunctionContext) -> Reply,
    ) -> Option<Reply>;
    /// Lend the application session to an Automation command for a read.
    fn with_session(&self, call: &mut dyn FnMut(&HostSession) -> Reply) -> Reply;
    /// A command lends the same session mutably for Automation ingress. Hosts that only
    /// expose reads explicitly refuse writes until they supply this callback.
    fn with_session_mut(&self, _call: &mut dyn FnMut(&mut HostSession) -> Reply) -> Reply {
        Err(AutoError::not_modelled(
            "Automation mutable command session",
            "dc-oxh8w",
        ))
    }
    /// The desktop uses this metadata notification to name sheets for the native C API too.
    fn context_changed(&self, _session: &HostSession) {}
}

/// Return the typed refusal used when an Automation call cannot borrow the session.
pub fn busy() -> AutoError {
    AutoError::not_modelled("Automation session is busy", OWNER)
}

/// One workbook's lent session. A nested loan (a macro in one workbook running another's)
/// keeps the loans it covers in `outer`, so the workbook that is running stays reachable
/// (dc-oxh8w.56).
struct Frame<'a> {
    model: &'a WorkbookAutomation,
    loan: &'a (dyn EventSession + 'a),
    outer: Option<&'a Frame<'a>>,
}

impl<'a> Frame<'a> {
    /// The loan of `model`, searching the nested loans from the innermost.
    fn find(&self, model: &WorkbookAutomation) -> Option<&'a (dyn EventSession + 'a)> {
        let mut frame = Some(self);
        while let Some(f) = frame {
            if std::ptr::eq(f.model, model) {
                return Some(f.loan);
            }
            frame = f.outer;
        }
        None
    }
}

scoped_tls_hkt::scoped_thread_local!(static EVENT_SESSION: for<'a> &'a Frame<'a>);

/// The session loan the running event or macro holds for `model`, if any.
fn event_loan<T>(model: &WorkbookAutomation, use_loan: impl FnOnce(&dyn EventSession) -> T) -> Option<T> {
    if !EVENT_SESSION.is_set() {
        return None;
    }
    EVENT_SESSION.with(|frame| frame.find(model).map(use_loan))
}

trait EventSession {
    fn read(&self, call: &mut dyn FnMut(&HostSession, bool) -> Reply) -> Reply;
    fn write(&self, call: &mut dyn FnMut(&mut HostSession) -> Reply) -> Reply;
}
struct EventLoan<'a>(RefCell<&'a mut HostSession>, bool);
impl EventSession for EventLoan<'_> {
    fn read(&self, call: &mut dyn FnMut(&HostSession, bool) -> Reply) -> Reply {
        let session = self.0.try_borrow().map_err(|_| busy())?;
        call(&session, self.1)
    }
    fn write(&self, call: &mut dyn FnMut(&mut HostSession) -> Reply) -> Reply {
        let mut session = self.0.try_borrow_mut().map_err(|_| busy())?;
        call(&mut session)
    }
}

struct Snapshot {
    model: Arc<ModelRoot>,
    formats: value::Formats,
    references: Rc<dyn ReferenceHandles>,
    full_name: String,
    active: Option<CellKey>,
    enabled: Rc<Cell<bool>>,
    screen_updating: Option<Rc<Cell<bool>>>,
    workbook: u64,
    from_session: bool,
    /// The sheets that are chart sheets (dc-oxh8w.23); the rest of a workbook's sheets are worksheets.
    chart_sheets: Rc<Vec<SheetId>>,
    dialog_sheets: Rc<Vec<SheetId>>,
    /// Each chart sheet's chart part.
    chart_parts: Rc<Vec<(SheetId, String)>>,
}

/// The sheets of the workbook that open as `kind` (`chart sheet` or `dialog sheet`): a chart sheet is the model's
/// sheet kind (a loaded one and one a macro made), a dialog sheet a preserved part.
fn sheet_kinds(host: &crate::HostState, kind: &str) -> Vec<SheetId> {
    if kind == "chart sheet" {
        return host
            .model()
            .sheets()
            .iter()
            .filter(|sheet| sheet.kind() == calc_doc_model::SheetKind::Chart)
            .map(|sheet| sheet.id())
            .collect();
    }
    host.loaded().sheets().iter().filter(|sheet| sheet.preserved_kind() == Some(kind)).map(|sheet| sheet.id()).collect()
}

/// The chart part of each chart sheet that has one.
fn chart_parts(host: &crate::HostState) -> Vec<(SheetId, String)> {
    sheet_kinds(host, "chart sheet")
        .into_iter()
        .filter_map(|id| host.model().definitions().charts(id).first().map(|chart| (id, chart.part.clone())))
        .collect()
}

/// Construct once per application. `observer` attaches the session's existing event source;
/// `context_changed` updates file identity and selection even with EnableEvents=False.
pub struct WorkbookAutomation {
    policy: calc_object_model::policy::PolicyChecker,
    host: Rc<dyn AutomationHost>,
    fallback: ValuesOnly<Rc<dyn AutomationHost>>,
    handlers: Handlers<Self>,
    snapshot: RefCell<Option<Rc<Snapshot>>>,
    /// `Application.ScreenUpdating`: a flag the application holds; no renderer is attached here.
    screen_updating: Cell<bool>,
    /// `Application.MultiThreadedCalculation`: Enabled, ThreadMode (XlThreadMode) and the manual ThreadCount.
    multithreaded: Cell<(bool, i32, i32)>,
    /// `Application.DisplayAlerts`: a flag the application holds; no dialog is shown here.
    display_alerts: Cell<bool>,
    /// `Application.StatusBar`: the text set, or None for the default status.
    status_bar: RefCell<Option<String>>,
    /// Tracker identities are interned. Every acquired wrapper owns one reference; views own
    /// their own acquisition too. The model drops its lease when the projection releases it.
    ranges: RefCell<HashMap<StoredReference, RangeLeases>>,
    /// The settings `Range.Find` saves for the next call and for FindNext / FindPrevious.
    find: RefCell<find::FindState>,
    /// What the last Range.Copy or Range.Cut without a Destination left (dc-oxh8w.15).
    clipboard: RefCell<Option<clipboard::Clip>>,
    /// The areas the last `Application.Goto` selected: `Goto` without a Reference returns to them (dc-oxh8w.49).
    last_goto: RefCell<Option<StoredReference>>,
    /// The view state a macro sets on windows, per workbook and sheet (dc-oxh8w.50).
    window_views: RefCell<HashMap<(u64, SheetId), window::View>>,
    /// Window captions a macro set, by workbook (dc-p3rg9).
    window_captions: RefCell<HashMap<u64, String>>,
    /// Window states (`XlWindowState`) a macro set, by workbook (dc-p3rg9).
    window_states: RefCell<HashMap<u64, i32>>,
    /// The protection a macro set on each sheet (`None`: it unprotected a sheet the file protects) (dc-oxh8w.68).
    sheet_protect: RefCell<HashMap<SheetId, Option<protection::SheetProtect>>>,
    /// The rows an in-place `AdvancedFilter` hid, by sheet, with the sheet's row and column insertions and deletions
    /// counted then (dc-hduqd): FilterMode and ShowAllData read it; a structural edit since drops it.
    in_place: RefCell<HashMap<SheetId, (usize, std::collections::BTreeSet<u32>)>>,
    /// Stable handles for `ObjectKey::Style { index }`: each entry is `(xf_id, folded_name)` so
    /// deleting an earlier style neither shifts live handles nor aliases a deleted handle.
    style_handles: RefCell<Vec<(u32, String)>>,
}

struct RangeLeases {
    count: usize,
    references: Rc<dyn ReferenceHandles>,
}

enum Access<'a> {
    Invocation(&'a mut dyn UserFunctionContext),
    Session(&'a HostSession, bool),
}

impl WorkbookAutomation {
    /// Build a workbook Automation profile with the default member policy.
    pub fn new(host: Rc<dyn AutomationHost>) -> Self {
        Self::with_policy(host, Default::default())
    }

    /// Build a workbook Automation profile with an explicit member policy.
    pub fn with_policy(
        host: Rc<dyn AutomationHost>,
        policy: calc_object_model::policy::PolicyChecker,
    ) -> Self {
        let mut handlers = Handlers::new();
        handlers.on(
            "Application",
            "Range",
            InvokeKind::Get,
            Self::application_range,
        );
        handlers.on("Application", "Caller", InvokeKind::Get, Self::caller);
        handlers.on("Application", "ThisCell", InvokeKind::Get, Self::this_cell);
        handlers.on(
            "Application",
            "ThisWorkbook",
            InvokeKind::Get,
            Self::current_book,
        );
        handlers.on(
            "Application",
            "ActiveSheet",
            InvokeKind::Get,
            Self::app_active_sheet,
        );
        handlers.on(
            "Application",
            "Worksheets",
            InvokeKind::Get,
            Self::app_worksheets,
        );
        handlers.on("Application", "Sheets", InvokeKind::Get, Self::app_sheets);
        handlers.on("Application", "Names", InvokeKind::Get, Self::app_names);
        handlers.on("Application", "Cells", InvokeKind::Get, Self::app_cells);
        handlers.on(
            "Application",
            "Selection",
            InvokeKind::Get,
            Self::app_selection,
        );
        handlers.on("Application", "ActiveCell", InvokeKind::Get, Self::app_active_cell);
        handlers.on(
            "Application",
            "Volatile",
            InvokeKind::Method,
            Self::app_volatile,
        );
        handlers.on("Application", "Workbooks", InvokeKind::Get, Self::delegate);
        handlers.on("Application", "EnableEvents", InvokeKind::Get, Self::events);
        handlers.on("Workbooks", "Application", InvokeKind::Get, Self::delegate);
        handlers.on("Workbooks", "Parent", InvokeKind::Get, Self::delegate);
        handlers.on("Workbook", "Name", InvokeKind::Get, Self::workbook_name);
        handlers.on("Workbook", "FullName", InvokeKind::Get, Self::full_name);
        handlers.on("Workbook", "Path", InvokeKind::Get, Self::path);
        handlers.on(
            "Workbook",
            "ActiveSheet",
            InvokeKind::Get,
            Self::get_active_sheet,
        );
        handlers.on("Workbook", "Application", InvokeKind::Get, Self::delegate);
        handlers.on("Workbook", "Parent", InvokeKind::Get, Self::delegate);
        handlers.on("Worksheet", "Name", InvokeKind::Get, Self::worksheet_name);
        handlers.on("Worksheet", "Application", InvokeKind::Get, Self::delegate);
        handlers.on("Worksheet", "Parent", InvokeKind::Get, Self::current_book);
        handlers.on("Window", "Parent", InvokeKind::Get, Self::current_book);
        handlers.on("Range", "Value", InvokeKind::Get, Self::range_value);
        handlers.on("Range", "Value2", InvokeKind::Get, Self::range_value2);
        handlers.on("Range", "Address", InvokeKind::Get, Self::range_address);
        handlers.on("Range", "Row", InvokeKind::Get, Self::range_row);
        handlers.on("Range", "Column", InvokeKind::Get, Self::range_column);
        handlers.on("Range", "Count", InvokeKind::Get, Self::range_count);
        handlers.on("Range", "Rows", InvokeKind::Get, Self::range_rows);
        handlers.on("Range", "Columns", InvokeKind::Get, Self::range_columns);
        handlers.on("Range", "Worksheet", InvokeKind::Get, Self::range_sheet);
        handlers.on(
            "Range",
            "Application",
            InvokeKind::Get,
            Self::range_application,
        );
        handlers.on("Range", "Parent", InvokeKind::Get, Self::range_sheet);
        handlers.on("Application", "Union", InvokeKind::Method, Self::union);
        worksheet_function::register(
            &mut handlers,
            Self::worksheet_functions,
            Self::fallback_member,
            Self::fallback_member,
        );
        Self::register_navigation(&mut handlers);
        Self::register_select(&mut handlers);
        Self::register_mutation(&mut handlers);
        Self::register_structure(&mut handlers);
        Self::register_chart_sheets(&mut handlers);
        Self::register_sheet_copy(&mut handlers);
        Self::register_cells(&mut handlers);
        Self::register_formula(&mut handlers);
        Self::register_find(&mut handlers);
        Self::register_advanced_filter(&mut handlers);
        Self::register_autofilter(&mut handlers);
        Self::register_range_end(&mut handlers);
        Self::register_clipboard(&mut handlers);
        Self::register_application(&mut handlers);
        Self::register_books(&mut handlers);
        Self::register_protection(&mut handlers);
        Self::register_formatting(&mut handlers);
        // Report objects register where the committed descriptor describes them (dc-piw0).
        let _ = Self::register_report(&mut handlers);
        // Printing registers likewise (dc-oxh8w.8).
        let _ = Self::register_print(&mut handlers);
        let _ = Self::register_window(&mut handlers);
        handlers.on(
            "Application",
            "EnableEvents",
            InvokeKind::Put,
            Self::put_events,
        );
        Self {
            fallback: ValuesOnly::new(host.clone()),
            policy,
            host,
            handlers,
            snapshot: RefCell::new(None),
            screen_updating: Cell::new(true),
            multithreaded: Cell::new((true, 0, 1)),
            display_alerts: Cell::new(true),
            status_bar: RefCell::new(None),
            ranges: RefCell::default(),
            find: RefCell::default(),
            clipboard: RefCell::new(None),
            last_goto: RefCell::new(None),
            window_views: RefCell::new(HashMap::new()),
            window_captions: RefCell::new(HashMap::new()),
            window_states: RefCell::new(HashMap::new()),
            sheet_protect: RefCell::new(HashMap::new()),
            in_place: RefCell::new(HashMap::new()),
            style_handles: RefCell::new(Vec::new()),
        }
    }
    /// Read the workbook profile's registered member handlers.
    pub fn handlers(&self) -> &Handlers<Self> {
        &self.handlers
    }
    /// The workbook profile includes values-only fallback members.
    pub fn coverage(&self) -> String {
        self.handlers
            .coverage("workbook", &self.fallback.handlers().implemented())
    }
    fn worksheet_functions(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_access(|access| {
            let access = RefCell::new(access);
            worksheet_function::dispatch(
                call,
                |value| match value {
                    AutoValue::Object(ObjectRef {
                        key:
                            ObjectKey::Range(held)
                            | ObjectKey::RangeView {
                                reference: held, ..
                            },
                        ..
                    }) => self
                        .resolve(&mut access.borrow_mut(), held)
                        .map(CalcValue::reference),
                    _ => worksheet_function::to_calc(value),
                },
                |name, args| match &mut *access.borrow_mut() {
                    Access::Invocation(context) => {
                        context.worksheet_function(name, args).map_err(|why| {
                            AutoError::not_modelled(
                                format!("WorksheetFunction.{}: {why}", call.member.name),
                                "dc-oxh8w",
                            )
                        })
                    }
                    Access::Session(session, strict) => {
                        let target = calc_func::catalogue()
                            .resolve_surface(calc_func::Notation::Call, name)
                            .map_err(|_| AutoError::not_modelled(call.label(), "dc-oxh8w"))?;
                        let args = args
                            .iter()
                            .enumerate()
                            .map(|(i, v)| {
                                if target.parameter_class(i)
                                    == Some(calc_func::ParameterClass::Value)
                                {
                                    if let Some(r) = v.as_reference() {
                                        if r.area_count() != 1 || r.areas()[0].cell_count() != 1 {
                                            return Err(AutoError::not_modelled(
                                                "WorksheetFunction scalar Range argument",
                                                "dc-1nbp.6",
                                            ));
                                        }
                                        return read_session(session, r.areas()[0], *strict);
                                    }
                                }
                                Ok(v.clone())
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        let environment = session.host().entry_environment();
                        let numeric = session.host().recalc_context().numeric;
                        let mut reader = SessionCells(session, *strict);
                        let mut services =
                            calc_func::EvalServices::with_cells(&numeric, &mut reader)
                                .with_environment(&environment);
                        worksheet_function::catalogue_call(name, &args, &mut services)
                    }
                },
            )
        })
    }
    /// Create a session observer that forwards workbook events to `events`.
    pub fn observer(self: &Rc<Self>, events: Rc<dyn EventSource>) -> Rc<dyn WorkbookEventObserver> {
        Rc::new(AutomationObserver {
            model: self.clone(),
            events,
        })
    }
    /// Lend `session` synchronously for a VBA document event callback. When
    /// `strict_file_provenance` is true (file projects, dc-snz.15), reading a cached
    /// or uncertified cell refuses explicitly so VBA `On Error` cannot hide it.
    pub fn with_event_session<T>(
        self: &Rc<Self>,
        session: &mut HostSession,
        strict_file_provenance: bool,
        call: impl FnOnce() -> T,
    ) -> T {
        self.context_changed(session);
        let loan = EventLoan(RefCell::new(session), strict_file_provenance);
        if EVENT_SESSION.is_set() {
            return EVENT_SESSION.with(|outer| {
                let frame = Frame { model: &**self, loan: &loan, outer: Some(outer) };
                EVENT_SESSION.set(&frame, call)
            });
        }
        let frame = Frame { model: &**self, loan: &loan, outer: None };
        EVENT_SESSION.set(&frame, call)
    }
    /// Whether this workbook's session is lent to a running macro or event, so other workbooks'
    /// macros reach it through that loan.
    pub fn is_lent(&self) -> bool {
        event_loan(self, |_| ()).is_some()
    }
    /// The workbook object identity for the current snapshot.
    pub fn workbook_object(&self) -> Result<ObjectRef, AutoError> {
        let meta = self.metadata()?;
        Ok(object("Workbook", ObjectKey::Workbook(meta.workbook)))
    }
    /// Match an explicit macro workbook qualifier against the current workbook identity.
    pub fn matches_workbook_name(&self, name: &str) -> Result<bool, AutoError> {
        let meta = self.metadata()?;
        Ok(!name.is_empty()
            && (calc_value::workbook_names_equal(&meta.full_name, name)
                || calc_value::workbook_names_equal(book_name(&meta.full_name), name)))
    }
    /// The worksheet object identity for `sheet` in the current snapshot.
    pub fn worksheet_object(&self, sheet: SheetId) -> Result<ObjectRef, AutoError> {
        let meta = self.metadata()?;
        sheet_name(&meta, sheet)?;
        Ok(object(
            "Worksheet",
            ObjectKey::Worksheet {
                workbook: meta.workbook,
                sheet,
            },
        ))
    }
    /// The sheet object identity for `sheet` (`Chart` for a chart sheet, `Worksheet` otherwise).
    pub fn sheet_object(&self, sheet: SheetId) -> Result<ObjectRef, AutoError> {
        let meta = self.metadata()?;
        sheet_name(&meta, sheet)?;
        if meta.chart_sheets.contains(&sheet) {
            return match Self::chart_of_sheet(&meta, sheet)? {
                AutoValue::Object(obj) => Ok(obj),
                _ => unreachable!("chart_of_sheet always returns an Object"),
            };
        }
        Ok(object(
            "Worksheet",
            ObjectKey::Worksheet {
                workbook: meta.workbook,
                sheet,
            },
        ))
    }
    /// Acquire a tracked Range identity for an event Target on `session`.
    pub fn acquire_event_range(
        &self,
        session: &HostSession,
        reference: &ReferenceLike,
    ) -> Result<ObjectRef, AutoError> {
        self.context_changed(session);
        match self.acquire(
            &mut Access::Session(session, false),
            reference,
            RangeAxis::Cells,
        )? {
            AutoValue::Object(obj) => Ok(obj),
            _ => unreachable!("acquire always returns an Object"),
        }
    }
    /// Seed snapshot metadata from a `HostState` before a `HostSession` installs the handler.
    pub fn context_changed_from_host(&self, host: &crate::HostState, full_name: &str) {
        let previous = self.snapshot.borrow().clone();
        let (workbook, enabled) = match previous.as_ref() {
            Some(old) => (old.workbook, old.enabled.clone()),
            None => {
                (books::next_identity(), Rc::new(Cell::new(true)))
            }
        };
        let active = host
            .loaded()
            .active_sheet()
            .or_else(|| host.sheets().first().map(|s| s.id()))
            .and_then(|sheet| CellKey::new(sheet, 1, 1).ok());
        *self.snapshot.borrow_mut() = Some(Rc::new(Snapshot {
            model: host.model().clone(),
            formats: value::Formats::new(host),
            references: host.reference_view(),
            full_name: full_name.to_owned(),
            active,
            enabled,
            screen_updating: previous.as_ref().and_then(|state| state.screen_updating.clone()),
            workbook,
            from_session: false,
            chart_sheets: Rc::new(sheet_kinds(host, "chart sheet")),
            dialog_sheets: Rc::new(sheet_kinds(host, "dialog sheet")),
            chart_parts: Rc::new(chart_parts(host)),
        }));
    }
    /// Update the workbook's file identity (e.g. Save As) without replacing workbook identity.
    pub fn set_full_name(&self, full_name: &str) {
        let previous = self.snapshot.borrow().clone();
        if let Some(old) = previous {
            *self.snapshot.borrow_mut() = Some(Rc::new(Snapshot {
                model: old.model.clone(),
                formats: old.formats.clone(),
                references: old.references.clone(),
                full_name: full_name.to_owned(),
                active: old.active,
                enabled: old.enabled.clone(),
                screen_updating: old.screen_updating.clone(),
                workbook: old.workbook,
                from_session: old.from_session,
                chart_sheets: old.chart_sheets.clone(),
                dialog_sheets: old.dialog_sheets.clone(),
                chart_parts: old.chart_parts.clone(),
            }));
        }
    }
    /// Refresh the workbook identity and selection used by the Automation profile.
    pub fn context_changed(&self, session: &HostSession) {
        let enabled = session.workbook_events_switch();
        let screen_updating = session.screen_updating_switch();
        screen_updating.set(self.screen_updating.get());
        let previous = self.snapshot.borrow().clone();
        let same_workbook = matches!(
            previous.as_ref(),
            Some(old) if Rc::ptr_eq(&old.enabled, &enabled) || !old.from_session
        );
        let workbook = if same_workbook {
            previous.as_ref().expect("previous snapshot").workbook
        } else {
            if let Some(old) = previous.as_ref() {
                enabled.set(old.enabled.get());
            }
            books::next_identity()
        };
        let session_name = session.file_full_name();
        let full_name = if !session_name.is_empty() {
            session_name.to_owned()
        } else if same_workbook {
            previous
                .as_ref()
                .map(|old| old.full_name.clone())
                .unwrap_or_default()
        } else {
            String::new()
        };
        let active = session.selection().active.or_else(|| {
            let sheet = session
                .host()
                .loaded()
                .active_sheet()
                .or_else(|| session.host().sheets().first().map(|s| s.id()))?;
            CellKey::new(sheet, 1, 1).ok()
        });
        *self.snapshot.borrow_mut() = Some(Rc::new(Snapshot {
            model: session.host().model().clone(),
            formats: value::Formats::new(session.host()),
            references: session.host().reference_view(),
            full_name,
            active,
            enabled,
            screen_updating: Some(screen_updating),
            workbook,
            from_session: true,
            chart_sheets: Rc::new(sheet_kinds(session.host(), "chart sheet")),
            dialog_sheets: Rc::new(sheet_kinds(session.host(), "dialog sheet")),
            chart_parts: Rc::new(chart_parts(session.host())),
        }));
        self.host.context_changed(session);
    }
    fn metadata(&self) -> Result<Rc<Snapshot>, AutoError> {
        self.snapshot
            .borrow()
            .clone()
            .ok_or(AutoError::Disconnected)
    }
    fn with_access<T>(
        &self,
        mut call: impl FnMut(Access<'_>) -> Result<T, AutoError>,
    ) -> Result<T, AutoError> {
        let mut value = None;
        let attempted = self.host.with_invocation(&mut |context| {
            value = Some(call(Access::Invocation(context)));
            Ok(AutoValue::Empty)
        });
        if let Some(status) = attempted {
            status?;
            return value.expect("invocation callback called once");
        }
        // With no worksheet invocation, an event reads its borrowed session. The scoped
        // loan cannot survive the callback, including unwinding and nested delivery.
        let result = event_loan(self, |loan| {
            let status = loan.read(&mut |session, strict| {
                self.context_changed(session);
                value = Some(call(Access::Session(session, strict)));
                Ok(AutoValue::Empty)
            });
            status.and_then(|_| value.take().expect("event callback called once"))
        });
        if let Some(result) = result {
            return result;
        }
        let status = self.host.with_session(&mut |session| {
            self.context_changed(session);
            value = Some(call(Access::Session(session, false)));
            Ok(AutoValue::Empty)
        });
        status?;
        value.expect("command callback called once")
    }
    fn validate(&self, target: &ObjectRef, meta: &Snapshot) -> Result<(), AutoError> {
        let workbook = match target.key {
            ObjectKey::Windows(0) => None,
            ObjectKey::Workbook(id)
            | ObjectKey::Sheets(id)
            | ObjectKey::Worksheets(id)
            | ObjectKey::Charts(id)
            | ObjectKey::Names(id)
            | ObjectKey::Styles(id)
            | ObjectKey::BookWindow(id)
            | ObjectKey::Windows(id)
            | ObjectKey::SheetGroup { workbook: id, .. }
            | ObjectKey::Style { workbook: id, .. }
            | ObjectKey::Name { workbook: id, .. } => Some(id),
            ObjectKey::Worksheet { workbook, sheet }
            | ObjectKey::SheetNames { workbook, sheet }
            | ObjectKey::Report {
                workbook, sheet, ..
            } => {
                if meta.model.sheet_by_id(sheet).is_none() {
                    return Err(AutoError::not_modelled("Worksheet was deleted", OWNER));
                }
                Some(workbook)
            }
            _ => None,
        };
        if workbook.is_some_and(|id| id != meta.workbook) {
            return Err(AutoError::Disconnected);
        }
        Ok(())
    }
    fn resolve(
        &self,
        access: &mut Access<'_>,
        reference: &StoredReference,
    ) -> Result<ReferenceLike, AutoError> {
        let result = match access {
            Access::Invocation(context) => context.resolve_reference(reference),
            Access::Session(session, _) => session.host().reference_view().resolve(reference),
        };
        result.map_err(|e| match e {
            ReferenceHandleError::Deleted => AutoError::excel(424, "Object required"),
            _ => AutoError::not_modelled(format!("Range reference {e}"), OWNER),
        })
    }
    fn acquire(&self, access: &mut Access<'_>, range: &ReferenceLike, axis: RangeAxis) -> Reply {
        let reference = self.retain_range(access, range)?;
        Ok(AutoValue::Object(object(
            "Range",
            if axis == RangeAxis::Cells {
                ObjectKey::Range(reference)
            } else {
                ObjectKey::RangeView { reference, axis }
            },
        )))
    }
    fn retain_range(
        &self,
        access: &mut Access<'_>,
        range: &ReferenceLike,
    ) -> Result<StoredReference, AutoError> {
        let reference = match access {
            Access::Invocation(context) => context.retain_reference(range),
            Access::Session(session, _) => session.host().reference_view().retain(range),
        }
        .map_err(|e| AutoError::not_modelled(format!("Range reference {e}"), OWNER))?;
        let references = self.metadata()?.references.clone();
        self.ranges
            .borrow_mut()
            .entry(reference.clone())
            .and_modify(|entry| entry.count += 1)
            .or_insert(RangeLeases {
                count: 1,
                references,
            });
        Ok(reference)
    }
    pub(super) fn release_stored(&self, reference: &StoredReference) {
        let mut ranges = self.ranges.borrow_mut();
        if let Some(entry) = ranges.get_mut(reference) {
            entry.references.release(reference);
            entry.count -= 1;
            if entry.count == 0 {
                ranges.remove(reference);
            }
        }
    }
    fn range_argument(
        &self,
        access: &mut Access<'_>,
        meta: &Snapshot,
        value: &AutoValue,
        sheet: Option<SheetId>,
    ) -> Result<ReferenceLike, AutoError> {
        match value {
            AutoValue::Object(ObjectRef {
                key: ObjectKey::Range(reference) | ObjectKey::RangeView { reference, .. },
                ..
            }) => self.resolve(access, reference),
            AutoValue::Text(_) => {
                let text = value.as_text().unwrap();
                // `Range("A1,C1:C2")`: a comma outside quotes separates the areas of a union.
                let mut parts = Vec::new();
                let (mut quoted, mut start) = (false, 0);
                for (i, c) in text.char_indices() {
                    match c {
                        '\'' => quoted = !quoted,
                        ',' if !quoted => {
                            parts.push(&text[start..i]);
                            start = i + 1;
                        }
                        _ => {}
                    }
                }
                if !parts.is_empty() {
                    parts.push(&text[start..]);
                    let mut ranges = Vec::new();
                    for part in parts {
                        ranges.push(self.range_argument(access, meta, &AutoValue::text(part.trim()), sheet)?);
                    }
                    return geometry::union(&ranges);
                }
                let at = sheet
                    .and_then(|sheet| CellKey::new(sheet, 1, 1).ok())
                    .or_else(|| match access {
                        Access::Invocation(context) => context
                            .user_view()
                            .map(|view| view.selection)
                            .or(meta.active),
                        _ => meta.active,
                    })
                    .ok_or_else(|| {
                        AutoError::not_modelled(
                            "Application.Range without an active worksheet",
                            OWNER,
                        )
                    })?;
                let target = match access {
                    Access::Session(session, _) => session.host().resolve_reference_text(at, &text),
                    _ => crate::resolve_reference_text(&meta.model, at, &text),
                };
                match target {
                    ReferenceTarget::Area(area) => Ok(ReferenceLike::area(area)),
                    ReferenceTarget::Invalid => Err(AutoError::excel(1004, "")),
                    _ => Err(AutoError::not_modelled(
                        "Application.Range address form",
                        OWNER,
                    )),
                }
            }
            _ => Err(geometry::type_mismatch()),
        }
    }
    fn get_range(
        &self,
        access: &mut Access<'_>,
        meta: &Snapshot,
        sheet: Option<SheetId>,
        call: &Call<'_>,
    ) -> Reply {
        if call.args.is_empty() || call.args.len() > 2 {
            return Err(geometry::bad_count());
        }
        let mut range = self.range_argument(access, meta, call.arg(0), sheet)?;
        if sheet.is_some_and(|sheet| range.sheet() != sheet) {
            return Err(AutoError::excel(1004, ""));
        }
        if !call.arg(1).is_missing() {
            if matches!(
                call.arg(1),
                AutoValue::Empty | AutoValue::Null | AutoValue::Error(_) | AutoValue::Bool(_)
            ) {
                return Err(AutoError::excel(1004, ""));
            }
            let second = self.range_argument(access, meta, call.arg(1), sheet)?;
            if range.sheet() != second.sheet()
                || range.area_count() != 1
                || second.area_count() != 1
            {
                return Err(AutoError::excel(1004, ""));
            }
            let a = range.areas()[0];
            let b = second.areas()[0];
            let first = CellKey::new(
                a.sheet(),
                a.first().row().min(b.first().row()),
                a.first().column().min(b.first().column()),
            )
            .expect("valid endpoints");
            let last = CellKey::new(
                a.sheet(),
                a.last().row().max(b.last().row()),
                a.last().column().max(b.last().column()),
            )
            .expect("valid endpoints");
            range = ReferenceLike::area(GridArea::new(first, last).expect("same worksheet"));
        }
        self.acquire(access, &range, RangeAxis::Cells)
    }
    /// Member-independent access and lifetime validation, shared by workbook handlers.
    fn with_target(
        &self,
        target: &ObjectRef,
        mut f: impl FnMut(&mut Access<'_>, &Snapshot) -> Reply,
    ) -> Reply {
        self.with_access(|mut access| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            f(&mut access, &meta)
        })
    }
    fn fallback_member(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let interface = Descriptor::get().ty(call.interface).name.as_str();
        let owner = match (interface, call.member.name.as_str()) {
            ("Series", "Points")
            | ("Points", "Count" | "Item" | "_Default")
            | ("Point", "HasDataLabel" | "DataLabel")
            | ("DataLabels", "Font")
            | ("DataLabel", "Font" | "NumberFormat" | "NumberFormatLinked" | "Text" | "Caption" | "Left" | "Top") => {
                chart_labels::POINTS
            }
            _ => OWNER,
        };
        self.fallback.handlers().dispatch(&self.fallback, target, call, owner)
    }
    fn delegate(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, _| self.fallback_member(target, call))
    }
    fn application_range(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if let AutoValue::Object(ObjectRef {
            key: ObjectKey::Range(reference) | ObjectKey::RangeView { reference, .. },
            ..
        }) = call.arg(0)
        {
            if let Some(routed) = self.range_book_call(reference, target, call) {
                return routed;
            }
            if let AutoValue::Object(ObjectRef {
                key: ObjectKey::Range(second) | ObjectKey::RangeView { reference: second, .. },
                ..
            }) = call.arg(1)
                && !self.owns_range(second)
            {
                return Err(AutoError::excel(1004, ""));
            }
        } else if let Some(routed) = self.active_book_call(target, call) {
            return routed;
        }
        self.with_target(target, |access, meta| {
            self.get_range(access, meta, None, call)
        })
    }
    fn caller(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.calling_cell(target, call, false)
    }
    fn this_cell(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.calling_cell(target, call, true)
    }
    fn calling_cell(&self, target: &ObjectRef, call: &Call<'_>, this_cell: bool) -> Reply {
        self.with_target(target, |access, _| {
            if (this_cell && !call.args.is_empty()) || call.args.len() > 1 {
                return Err(geometry::bad_count());
            }
            if !call.arg(0).is_missing() {
                return Err(AutoError::not_modelled("_Application.Caller Index", OWNER));
            }
            let caller = match access {
                Access::Invocation(_) if this_cell => {
                    return Err(AutoError::excel(
                        1004,
                        "Application.ThisCell has no cell in an add-in function",
                    ));
                }
                Access::Invocation(context) => context.caller(),
                _ => None,
            };
            match caller {
                Some(at) => self.acquire(access, &ReferenceLike::cell(at), RangeAxis::Cells),
                None => Ok(AutoValue::Error(2023)),
            }
        })
    }
    fn current_book(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            no_args(call)?;
            Ok(AutoValue::Object(object(
                "Workbook",
                ObjectKey::Workbook(meta.workbook),
            )))
        })
    }
    fn app_active_sheet(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if let Some(routed) = self.active_book_call(target, call) {
            return routed;
        }
        self.get_active_sheet(target, call)
    }
    fn get_active_sheet(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |access, meta| {
            no_args(call)?;
            self.active_sheet(access, meta)
        })
    }
    fn app_worksheets(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            no_args(call)?;
            let id = self.active_book_id(meta.workbook)?;
            Ok(AutoValue::Object(object(
                "Sheets",
                ObjectKey::Worksheets(id),
            )))
        })
    }
    fn app_sheets(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            no_args(call)?;
            let id = self.active_book_id(meta.workbook)?;
            Ok(AutoValue::Object(object(
                "Sheets",
                ObjectKey::Sheets(id),
            )))
        })
    }
    fn app_names(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            no_args(call)?;
            let id = self.active_book_id(meta.workbook)?;
            Ok(AutoValue::Object(object(
                "Names",
                ObjectKey::Names(id),
            )))
        })
    }
    fn app_cells(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if let Some(routed) = self.active_book_call(target, call) {
            return routed;
        }
        self.with_target(target, |access, meta| {
            no_args(call)?;
            let sheet = match access {
                Access::Invocation(context) => context
                    .user_view()
                    .map(|v| v.active_sheet)
                    .or_else(|| meta.active.map(|at| at.sheet())),
                _ => meta.active.map(|at| at.sheet()),
            }
            .ok_or_else(|| {
                AutoError::not_modelled("_Application.Cells without an active worksheet", OWNER)
            })?;
            sheet_name(meta, sheet)?;
            self.acquire(
                access,
                &ReferenceLike::area(GridArea::full_sheet(sheet)),
                RangeAxis::Cells,
            )
        })
    }
    fn app_selection(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if let Some(routed) = self.active_book_call(target, call) {
            return routed;
        }
        self.own_selection(target, call)
    }
    fn own_selection(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |access, meta| {
            no_args(call)?;
            // ww24 c/ case7: a chart sheet is active, so nothing is selected (Nothing).
            if let Access::Session(session, _) = access
                && Self::session_active_sheet(session).is_some_and(|sheet| meta.chart_sheets.contains(&sheet))
            {
                return Ok(AutoValue::Empty);
            }
            if let Access::Session(session, _) = access
                && !session.selection().areas.is_empty()
            {
                let selection = ReferenceLike::from_areas(session.selection().areas.clone())
                    .map_err(|error| AutoError::excel(1004, error.to_string()))?;
                return self.acquire(access, &selection, RangeAxis::Cells);
            }
            self.active_cell_range(access, meta, "_Application.Selection without a user view")
        })
    }
    fn app_active_cell(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if let Some(routed) = self.active_book_call(target, call) {
            return routed;
        }
        self.own_active_cell(target, call)
    }
    fn own_active_cell(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |access, meta| {
            no_args(call)?;
            // ww24 c/ case7: with a chart sheet active there is no active cell (91).
            if let Access::Session(session, _) = access
                && Self::session_active_sheet(session).is_some_and(|sheet| meta.chart_sheets.contains(&sheet))
            {
                return Err(AutoError::excel(91, "Object variable or With block variable not set"));
            }
            self.active_cell_range(access, meta, "_Application.ActiveCell without a user view")
        })
    }
    fn active_cell_range(&self, access: &mut Access<'_>, meta: &Snapshot, refusal: &str) -> Reply {
        let at = match access {
            Access::Invocation(context) => context.user_view().map(|v| v.selection),
            Access::Session(..) => meta.active,
        }
        .ok_or_else(|| AutoError::not_modelled(refusal, OWNER))?;
        sheet_name(meta, at.sheet())?;
        self.acquire(access, &ReferenceLike::cell(at), RangeAxis::Cells)
    }
    fn app_volatile(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |access, _| {
            if call.args.len() > 1 {
                return Err(geometry::bad_count());
            }
            let enabled = if call.arg(0).is_missing() {
                true
            } else {
                geometry::boolean(call.arg(0))?
            };
            if let Access::Invocation(context) = access {
                context.set_volatile(enabled);
            }
            Ok(AutoValue::Empty)
        })
    }
    fn events(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            no_args(call)?;
            Ok(AutoValue::Bool(meta.enabled.get()))
        })
    }
    fn workbook_name(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            no_args(call)?;
            Ok(AutoValue::text(book_name(&meta.full_name)))
        })
    }
    fn full_name(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            no_args(call)?;
            Ok(AutoValue::text(&meta.full_name))
        })
    }
    fn path(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            no_args(call)?;
            Ok(AutoValue::text(book_path(&meta.full_name)))
        })
    }
    fn worksheet_name(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            no_args(call)?;
            let ObjectKey::Worksheet { sheet, .. } = target.key else {
                return Err(geometry::type_mismatch());
            };
            Ok(AutoValue::text(&sheet_name(meta, sheet)?))
        })
    }
    fn active_sheet(&self, access: &Access<'_>, meta: &Snapshot) -> Reply {
        let sheet = match access {
            Access::Invocation(context) => context.user_view().map(|v| v.active_sheet),
            _ => meta.active.map(|at| at.sheet()),
        }
        .ok_or_else(|| {
            AutoError::not_modelled("_Application.ActiveSheet without a user view", OWNER)
        })?;
        sheet_name(meta, sheet)?;
        if meta.chart_sheets.contains(&sheet) {
            return Self::chart_of_sheet(meta, sheet);
        }
        Ok(AutoValue::Object(object(
            "Worksheet",
            ObjectKey::Worksheet {
                workbook: meta.workbook,
                sheet,
            },
        )))
    }
    fn with_range(
        &self,
        target: &ObjectRef,
        mut f: impl FnMut(&mut Access<'_>, &Snapshot, &ReferenceLike, RangeAxis) -> Reply,
    ) -> Reply {
        self.with_target(target, |access, meta| {
            let (held, axis) = match &target.key {
                ObjectKey::Range(held)
                | ObjectKey::RangeFormat(held)
                | ObjectKey::RangeEdge { reference: held, .. }
                | ObjectKey::Areas(held) => {
                    (held, RangeAxis::Cells)
                }
                ObjectKey::RangeView { reference, axis } => (reference, *axis),
                _ => return Err(geometry::type_mismatch()),
            };
            let range = self.resolve(access, held)?;
            f(access, meta, &range, axis)
        })
    }
    fn range_address(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, meta, range, view| {
            let mut options = AddressOptions::from_call(call)?;
            if options.style == geometry::ReferenceStyle::R1C1
                && (!options.row_absolute || !options.column_absolute)
                && !geometry::address_default(call.arg(4))
            {
                let AutoValue::Object(ObjectRef {
                    key: ObjectKey::Range(reference) | ObjectKey::RangeView { reference, .. },
                    ..
                }) = call.arg(4)
                else {
                    return Err(geometry::type_mismatch());
                };
                options.relative_to = Some(self.resolve(access, reference)?.areas()[0].first());
            }
            let qualifier = if options.external {
                Some(match access {
                    Access::Invocation(context) => context
                        .reference_qualifier(range.sheet())
                        .map_err(|_| AutoError::not_modelled("Range.Address qualifier", OWNER))?,
                    _ => calc_fml::reference::format_workbook_sheet_qualifier(
                        book_name(&meta.full_name),
                        &sheet_name(meta, range.sheet())?,
                    ),
                })
            } else {
                None
            };
            Ok(AutoValue::text(&geometry::address_of_view(
                range,
                options,
                qualifier.as_deref(),
                view,
            )?))
        })
    }
    fn range_value(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, meta, range, _| {
            if call.args.len() > 1 {
                return Err(geometry::bad_count());
            }
            if matches!(call.arg(0), AutoValue::Null | AutoValue::Error(_)) {
                return Err(geometry::type_mismatch());
            }
            if matches!(call.arg(0), AutoValue::Bool(_)) {
                return Err(AutoError::excel(1004, ""));
            }
            if !matches!(call.arg(0), AutoValue::Missing | AutoValue::Empty) {
                return Err(AutoError::not_modelled(
                    "Range.Value RangeValueDataType",
                    "dc-1nbp.6",
                ));
            }
            self.read(access, meta, range, false)
        })
    }
    fn range_value2(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, meta, range, _| {
            no_args(call)?;
            self.read(access, meta, range, true)
        })
    }
    fn range_row(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, meta, range, axis| {
            let _ = (&access, meta, range, axis);
            no_args(call)?;
            Ok(AutoValue::I4(geometry::row(range) as i32))
        })
    }
    fn range_column(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, meta, range, axis| {
            let _ = (&access, meta, range, axis);
            no_args(call)?;
            Ok(AutoValue::I4(geometry::column(range) as i32))
        })
    }
    fn range_count(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, meta, range, axis| {
            let _ = (&access, meta, range, axis);
            no_args(call)?;
            Ok(geometry::count_value(geometry::count(range, axis)))
        })
    }
    fn range_rows(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, meta, range, axis| {
            let _ = (&access, meta, range, axis);
            no_args(call)?;
            self.acquire(access, range, RangeAxis::Rows)
        })
    }
    fn range_columns(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, meta, range, axis| {
            let _ = (&access, meta, range, axis);
            no_args(call)?;
            self.acquire(access, range, RangeAxis::Columns)
        })
    }
    fn range_sheet(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, meta, range, axis| {
            let _ = (&access, meta, range, axis);
            no_args(call)?;
            Ok(AutoValue::Object(object(
                "Worksheet",
                ObjectKey::Worksheet {
                    workbook: meta.workbook,
                    sheet: range.sheet(),
                },
            )))
        })
    }
    fn range_application(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, meta, range, axis| {
            let _ = (&access, meta, range, axis);
            no_args(call)?;
            self.fallback.application_member(target, call)
        })
    }
    fn read(
        &self,
        access: &mut Access<'_>,
        meta: &Snapshot,
        reference: &ReferenceLike,
        value2: bool,
    ) -> Reply {
        // Excel reads the first area; all areas remain part of Address/Count/Union.
        let area = reference.areas()[0];
        if area.cell_count() > 1_048_576 {
            return Err(AutoError::not_modelled(
                "Range.Value materialization limit",
                OWNER,
            ));
        }
        let value =
            match access {
                Access::Invocation(context) => context
                    .values_of(&ReferenceLike::area(area))
                    .map_err(|why| {
                        AutoError::not_modelled(format!("Range.Value read: {why}"), OWNER)
                    })?,
                Access::Session(session, strict) => read_session(session, area, *strict)?,
            };
        let cells = value
            .as_array()
            .map(|a| a.iter_row_major().collect::<Vec<_>>())
            .unwrap_or_else(|| vec![&value]);
        if cells.len() as u64 != area.cell_count() {
            return Err(AutoError::not_modelled("Range.Value read shape", OWNER));
        }
        let mut values = Vec::with_capacity(cells.len());
        for (index, value) in cells.into_iter().enumerate() {
            if let CoreRef::Number(number) = value.core_ref() {
                if !value2 {
                    let at = CellKey::new(
                        area.sheet(),
                        area.first().row() + index as u32 / area.column_count(),
                        area.first().column() + index as u32 % area.column_count(),
                    )
                    .expect("cell in valid area");
                    values.push(meta.formats.number(&meta.model, at, number)?);
                    continue;
                }
            }
            values.push(auto_value(value)?);
        }
        if area.cell_count() == 1 {
            return Ok(values.pop().expect("one cell"));
        }
        Ok(AutoValue::Array(Box::new(AutoArray {
            rows: area.row_count(),
            columns: area.column_count(),
            lower: (1, 1),
            values,
        })))
    }
    fn union(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() < 2
            || call.args.len() > 30
            || call.arg(0).is_missing()
            || call.arg(1).is_missing()
        {
            return Err(geometry::bad_count());
        }
        if let AutoValue::Object(ObjectRef {
            key: ObjectKey::Range(held) | ObjectKey::RangeView { reference: held, .. },
            ..
        }) = call.arg(0)
            && let Some(routed) = self.range_book_call(held, target, call)
        {
            return routed;
        }
        self.with_access(|mut access| {
            let mut ranges = Vec::new();
            for arg in &call.args {
                if arg.is_missing() {
                    continue;
                }
                let AutoValue::Object(ObjectRef {
                    key:
                        ObjectKey::Range(held)
                        | ObjectKey::RangeView {
                            reference: held, ..
                        },
                    ..
                }) = arg
                else {
                    return Err(geometry::type_mismatch());
                };
                if !self.owns_range(held) {
                    // A range of another workbook: no union (as Application.Range's two ranges).
                    return Err(AutoError::excel(1004, ""));
                }
                ranges.push(self.resolve(&mut access, held)?);
            }
            self.acquire(&mut access, &geometry::union(&ranges)?, RangeAxis::Cells)
        })
    }
    fn put_events(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        self.with_access(|_| {
            self.metadata()?
                .enabled
                .set(geometry::boolean(call.arg(0))?);
            Ok(AutoValue::Empty)
        })
    }
}

impl ObjectModel for WorkbookAutomation {
    fn application(&self) -> ObjectRef {
        self.fallback.application()
    }
    fn invoke(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.policy.check(call)?;
        if let Some(routed) = self.foreign_call(target, call) {
            return routed;
        }
        if let Some(handler) = self.handlers.get(call.interface, call.member) {
            handler(self, target, call)
        } else {
            self.fallback_member(target, call)
        }
    }
    fn released(&self, target: &ObjectRef) {
        if self.foreign_release(target) {
            return;
        }
        if let ObjectKey::Range(reference)
        | ObjectKey::RangeFormat(reference)
        | ObjectKey::RangeEdge { reference, .. }
        | ObjectKey::RangeView { reference, .. }
        | ObjectKey::Areas(reference) = &target.key
        {
            self.release_stored(reference);
        }
    }
    fn record_refusal(&self, error: &AutoError) {
        if let AutoError::NotModelled { owner, .. } = error {
            let _ = self.host.with_invocation(&mut |context| {
                context.refuse_by("Automation member is not modelled", owner);
                Ok(AutoValue::Empty)
            });
        }
    }
}

impl Drop for WorkbookAutomation {
    fn drop(&mut self) {
        for (reference, entry) in self.ranges.get_mut().drain() {
            for _ in 0..entry.count {
                entry.references.release(&reference);
            }
        }
    }
}

/// Adapter that forwards session events through the workbook Automation profile.
pub struct AutomationObserver {
    model: Rc<WorkbookAutomation>,
    events: Rc<dyn EventSource>,
}
impl AutomationObserver {
    fn sheet_target_event(
        &self,
        session: &mut HostSession,
        sheet: SheetId,
        target: &[GridArea],
        document_event: &str,
        sheet_event: &str,
    ) -> Result<(), AutoError> {
        let reference = ReferenceLike::from_areas(target.to_vec())
            .map_err(|error| AutoError::not_modelled(error.to_string(), "dc-17spk"))?;
        self.model.context_changed(session);
        let worksheet = self.model.worksheet_object(sheet)?;
        let workbook = self.model.workbook_object()?;
        let application = self.model.application();
        let mut first_failure = None;
        for (source, interface, event) in [
            (&worksheet, "DocEvents", document_event),
            (&workbook, "WorkbookEvents", sheet_event),
            (&application, "AppEvents", sheet_event),
        ] {
            if !session.workbook_events_enabled() {
                break;
            }
            // Each delivery owns one acquisition of the Range argument.
            let target = AutoValue::Object(self.model.acquire_event_range(session, &reference)?);
            let mut args = Vec::with_capacity(2);
            if interface != "DocEvents" {
                args.push(AutoValue::Object(worksheet.clone()));
            }
            args.push(target);
            let result = self.model.with_event_session(session, false, || {
                self.events.raise(source, interface, event, &args)
            });
            if let Err(error) = result {
                first_failure.get_or_insert(error);
            }
        }
        first_failure.map_or(Ok(()), Err)
    }
}

impl WorkbookEventObserver for AutomationObserver {
    fn context_changed(&self, session: &HostSession) {
        self.model.context_changed(session);
    }

    fn workbook_open(&self, session: &mut HostSession) -> Result<(), String> {
        let events_enabled = session.workbook_events_switch();
        self.model
            .with_event_session(session, false, || {
                let workbook = self.model.workbook_object()?;
                let document = self.events.raise(&workbook, "WorkbookEvents", "Open", &[]);
                if !events_enabled.get() {
                    return document;
                }
                let application = self.events.raise(
                    &self.model.application(),
                    "AppEvents",
                    "WorkbookOpen",
                    &[AutoValue::Object(workbook)],
                );
                document.and(application)
            })
            .map_err(|error: AutoError| error.description())
    }

    fn worksheet_change(
        &self,
        session: &mut HostSession,
        sheet: SheetId,
        target: &[GridArea],
    ) -> Result<(), String> {
        self.sheet_target_event(session, sheet, target, "Change", "SheetChange")
            .map_err(|error| error.description())
    }

    fn selection_changed(
        &self,
        session: &mut HostSession,
        sheet: SheetId,
        target: &[GridArea],
    ) -> Result<(), String> {
        self.sheet_target_event(
            session,
            sheet,
            target,
            "SelectionChange",
            "SheetSelectionChange",
        )
        .map_err(|error| error.description())
    }

    fn worksheet_calculate(&self, session: &mut HostSession, sheet: SheetId) -> Result<(), String> {
        let events_enabled = session.workbook_events_switch();
        self.model
            .with_event_session(session, false, || {
                let worksheet = self.model.worksheet_object(sheet)?;
                let workbook = self.model.workbook_object()?;
                let document = self.events.raise(&worksheet, "DocEvents", "Calculate", &[]);
                if !events_enabled.get() {
                    return document;
                }
                let args = [AutoValue::Object(worksheet)];
                let book = self
                    .events
                    .raise(&workbook, "WorkbookEvents", "SheetCalculate", &args);
                if !events_enabled.get() {
                    return document.and(book);
                }
                let application = self.events.raise(
                    &self.model.application(),
                    "AppEvents",
                    "SheetCalculate",
                    &args,
                );
                document.and(book).and(application)
            })
            .map_err(|error: AutoError| error.description())
    }

    fn after_calculate(&self, session: &mut HostSession) -> Result<(), String> {
        self.model
            .with_event_session(session, false, || {
                self.events.raise(
                    &self.model.application(),
                    "AppEvents",
                    "AfterCalculate",
                    &[],
                )
            })
            .map_err(|error| error.description())
    }

    fn before_save(
        &self,
        session: &mut HostSession,
        operation: crate::SaveOperation,
        cancel: &mut bool,
    ) -> Result<(), String> {
        let events_enabled = session.workbook_events_switch();
        self.model
            .with_event_session(session, false, || {
                let workbook = self.model.workbook_object()?;
                let save_as_ui = AutoValue::Bool(operation == crate::SaveOperation::SaveAsDialog);
                let document = self.events.raise_with_cancel(
                    &workbook,
                    "WorkbookEvents",
                    "BeforeSave",
                    &[save_as_ui.clone()],
                    cancel,
                );
                if !events_enabled.get() {
                    return document;
                }
                let application = self.events.raise_with_cancel(
                    &self.model.application(),
                    "AppEvents",
                    "WorkbookBeforeSave",
                    &[AutoValue::Object(workbook), save_as_ui],
                    cancel,
                );
                document.and(application)
            })
            .map_err(|error: AutoError| error.description())
    }
}

fn no_args(call: &Call<'_>) -> Result<(), AutoError> {
    if call.args.is_empty() {
        Ok(())
    } else {
        Err(geometry::bad_count())
    }
}
fn book_name(full: &str) -> &str {
    full.rsplit(['/', '\\']).next().unwrap_or(full)
}
fn book_path(full: &str) -> &str {
    match full.rfind(['/', '\\']) {
        Some(n) if n == 0 || (n == 2 && full.as_bytes()[1] == b':') => &full[..n + 1],
        Some(n) => &full[..n],
        None => "",
    }
}
fn sheet_name(meta: &Snapshot, sheet: SheetId) -> Result<String, AutoError> {
    meta.model
        .sheet_by_id(sheet)
        .map(|s| s.name().text().to_string_lossy())
        .ok_or_else(|| AutoError::not_modelled("Range worksheet was deleted", OWNER))
}
fn auto_value(value: &CalcValue) -> Reply {
    Ok(match value.core_ref() {
        CoreRef::Empty | CoreRef::Missing => AutoValue::Empty,
        CoreRef::Number(n) => AutoValue::R8(n),
        CoreRef::Logical(v) => AutoValue::Bool(v),
        CoreRef::Text(v) => AutoValue::Text(v.clone()),
        CoreRef::Error(e) => AutoValue::Error(match e {
            WorksheetErrorCode::Null => 2000,
            WorksheetErrorCode::Div0 => 2007,
            WorksheetErrorCode::Value => 2015,
            WorksheetErrorCode::Ref => 2023,
            WorksheetErrorCode::Name => 2029,
            WorksheetErrorCode::Num => 2036,
            WorksheetErrorCode::NA => 2042,
            WorksheetErrorCode::GettingData => 2043,
            // Existing XLL conversion refuses the newer CVErr codes pending measurement.
            _ => return Err(AutoError::not_modelled("Range.Value error code", OWNER)),
        }),
        _ => return Err(AutoError::not_modelled("Range.Value cell kind", OWNER)),
    })
}
// A macro read of a cache or an uncertified value is admitted, as Excel returns the loaded cache;
// the host records the observation with its provenance so the use stays traceable.
fn observed_provenance(value: &ProjectedValue) -> Option<crate::ObservedProvenance> {
    use crate::ObservedProvenance as P;
    match value {
        ProjectedValue::FileCached(_) => Some(P::FileCache),
        ProjectedValue::Stale(_) => Some(P::Stale),
        ProjectedValue::Calculated { calculation, .. } => {
            if calculation.reads_external_links() {
                Some(P::ExternalLink)
            } else if calculation.held_by_circular_rule() {
                Some(P::CircularHeld)
            } else {
                match calculation.stamp() {
                    None => Some(P::Uncertified),
                    Some(stamp)
                        if stamp.reads.uses_imported_cache
                            || stamp.reads.uses_boundary_cache
                            || stamp.reads.legacy_boundary
                            || !stamp.reads.observations.is_empty() =>
                    {
                        Some(P::CachedInputs)
                    }
                    Some(_) => None,
                }
            }
        }
        _ => None,
    }
}

fn read_session(
    session: &HostSession,
    area: GridArea,
    strict_provenance: bool,
) -> Result<CalcValue, AutoError> {
    if area.cell_count() > 1_048_576 {
        return Err(AutoError::not_modelled(
            "Automation range materialization limit",
            OWNER,
        ));
    }
    let mut cells = Vec::with_capacity(area.cell_count() as usize);
    for row in area.first().row()..=area.last().row() {
        for col in area.first().column()..=area.last().column() {
            cells.push(CellKey::new(area.sheet(), row, col).expect("cell in valid area"));
        }
    }
    let page = crate::project(session.host(), &ProjectionRequest { cells })
        .map_err(|e| AutoError::not_modelled(e.to_string(), OWNER))?;
    let values = page
        .cells()
        .iter()
        .map(|cell| match cell.value() {
            value if strict_provenance && observed_provenance(value).is_some() => {
                session.record_macro_observation(crate::MacroObservation {
                    at: cell.at(),
                    provenance: observed_provenance(value).expect("checked"),
                });
                match value {
                    ProjectedValue::Calculated { value: v, .. } => Ok(v.clone()),
                    ProjectedValue::FileCached(cache) => Ok(cache.value().to_value()),
                    ProjectedValue::Stale(stale) => Ok(stale
                        .last_value()
                        .cloned()
                        .or_else(|| stale.file_cache().map(|c| c.value().to_value()))
                        .unwrap_or_else(CalcValue::empty)),
                    _ => unreachable!("only cached or calculated values carry provenance"),
                }
            }
            ProjectedValue::Empty => Ok(CalcValue::empty()),
            ProjectedValue::Literal(v) | ProjectedValue::Calculated { value: v, .. } => {
                Ok(v.clone())
            }
            // Automation observes Excel's current cache. This is never counted as a calculation;
            // invocation reads above always go through the evaluator's admitted read service.
            ProjectedValue::FileCached(cache) => Ok(cache.value().to_value()),
            ProjectedValue::Stale(stale) => Ok(stale
                .last_value()
                .cloned()
                .or_else(|| stale.file_cache().map(|c| c.value().to_value()))
                .unwrap_or_else(CalcValue::empty)),
            ProjectedValue::NotModeled { .. } => Err(AutoError::not_modelled(
                "Range.Value read an unmodelled formula",
                OWNER,
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if values.len() == 1 {
        return Ok(values.into_iter().next().expect("one cell"));
    }
    calc_value::CalcArray::new(
        calc_value::ArrayShape {
            rows: area.row_count() as usize,
            cols: area.column_count() as usize,
        },
        values,
    )
    .map(CalcValue::array)
    .ok_or_else(|| AutoError::not_modelled("Range.Value array shape", OWNER))
}

struct SessionCells<'a>(&'a HostSession, bool);
impl calc_func::CellReader for SessionCells<'_> {
    fn read_area(
        &mut self,
        area: GridArea,
        visit: &mut dyn FnMut(CellKey, &CalcValue) -> std::ops::ControlFlow<()>,
    ) -> Result<(), calc_func::Unreadable> {
        let value = read_session(self.0, area, self.1).map_err(|_| calc_func::Unreadable)?;
        if let Some(array) = value.as_array() {
            for (index, value) in array.iter_row_major().enumerate() {
                let at = CellKey::new(
                    area.sheet(),
                    area.first().row() + index as u32 / area.column_count(),
                    area.first().column() + index as u32 % area.column_count(),
                )
                .expect("area cell");
                if visit(at, value).is_break() {
                    break;
                }
            }
        } else {
            let _ = visit(area.first(), &value);
        }
        Ok(())
    }
    fn subtotal_exclusions(&mut self, _: GridArea) -> Option<calc_func::SubtotalExclusions> {
        None
    }
}
