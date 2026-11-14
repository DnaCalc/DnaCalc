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
    AutoArray, AutoError, AutoValue, Call, EventSource, Handlers, InvokeKind, ObjectKey,
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

const OWNER: &str = "dc-0f1k.24";
type Reply = Result<AutoValue, AutoError>;
mod mutation;
mod navigation;
mod value;

/// The application supplies access, never a copied value reader. `None` means no admitted
/// worksheet invocation; `Some(Err(..))` must never fall back to the command route.
pub trait AutomationHost: ApplicationHost {
    fn with_invocation(
        &self,
        call: &mut dyn FnMut(&mut dyn UserFunctionContext) -> Reply,
    ) -> Option<Reply>;
    fn with_session(&self, call: &mut dyn FnMut(&HostSession) -> Reply) -> Reply;
    /// A command lends the same session mutably for Automation ingress. Hosts that only
    /// expose reads explicitly refuse writes until they supply this callback.
    fn with_session_mut(&self, _call: &mut dyn FnMut(&mut HostSession) -> Reply) -> Reply {
        Err(AutoError::not_modelled(
            "Automation mutable command session",
            "dc-1nbp.5",
        ))
    }
    /// The desktop uses this metadata notification to name sheets for the native C API too.
    fn context_changed(&self, _session: &HostSession) {}
}

pub fn busy() -> AutoError {
    AutoError::not_modelled("Automation session is busy", OWNER)
}

scoped_tls_hkt::scoped_thread_local!(static EVENT_SESSION: for<'a> (
    &'a WorkbookAutomation, &'a (dyn EventSession + 'a)
));

trait EventSession {
    fn read(&self, call: &mut dyn FnMut(&HostSession) -> Reply) -> Reply;
    fn write(&self, call: &mut dyn FnMut(&mut HostSession) -> Reply) -> Reply;
}
struct EventLoan<'a>(RefCell<&'a mut HostSession>);
impl EventSession for EventLoan<'_> {
    fn read(&self, call: &mut dyn FnMut(&HostSession) -> Reply) -> Reply {
        let session = self.0.try_borrow().map_err(|_| busy())?;
        call(&session)
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
    workbook: u64,
}

/// Construct once per application. `observer` attaches the session's existing event source;
/// `context_changed` updates file identity and selection even with EnableEvents=False.
pub struct WorkbookAutomation {
    host: Rc<dyn AutomationHost>,
    fallback: ValuesOnly<Rc<dyn AutomationHost>>,
    handlers: Handlers<Self>,
    snapshot: RefCell<Option<Rc<Snapshot>>>,
    next_workbook: Cell<u64>,
    /// Tracker identities are interned. Every acquired wrapper owns one reference; views own
    /// their own acquisition too. The model drops its lease when the projection releases it.
    ranges: RefCell<HashMap<StoredReference, RangeLeases>>,
}

struct RangeLeases {
    count: usize,
    references: Rc<dyn ReferenceHandles>,
}

enum Access<'a> {
    Invocation(&'a mut dyn UserFunctionContext),
    Session(&'a HostSession),
}

impl WorkbookAutomation {
    pub fn new(host: Rc<dyn AutomationHost>) -> Self {
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
            "ActiveWorkbook",
            InvokeKind::Get,
            Self::current_book,
        );
        handlers.on(
            "Application",
            "ActiveSheet",
            InvokeKind::Get,
            Self::get_active_sheet,
        );
        handlers.on("Application", "Workbooks", InvokeKind::Get, Self::delegate);
        handlers.on("Application", "EnableEvents", InvokeKind::Get, Self::events);
        handlers.on("Workbooks", "Count", InvokeKind::Get, Self::delegate);
        handlers.on("Workbooks", "Item", InvokeKind::Get, Self::book_item);
        handlers.on("Workbooks", "_Default", InvokeKind::Get, Self::book_item);
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
        Self::register_mutation(&mut handlers);
        handlers.on(
            "Application",
            "EnableEvents",
            InvokeKind::Put,
            Self::put_events,
        );
        Self {
            fallback: ValuesOnly::new(host.clone()),
            host,
            handlers,
            snapshot: RefCell::new(None),
            next_workbook: Cell::new(1),
            ranges: RefCell::default(),
        }
    }
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
                                "dc-1nbp.5",
                            )
                        })
                    }
                    Access::Session(session) => {
                        let target = calc_func::catalogue()
                            .resolve_surface(calc_func::Notation::Call, name)
                            .map_err(|_| AutoError::not_modelled(call.label(), "dc-1nbp.5"))?;
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
                                        return read_session(session, r.areas()[0]);
                                    }
                                }
                                Ok(v.clone())
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        let environment = session.host().entry_environment();
                        let numeric = session.host().recalc_context().numeric;
                        let mut reader = SessionCells(session);
                        let mut services =
                            calc_func::EvalServices::with_cells(&numeric, &mut reader)
                                .with_environment(&environment);
                        worksheet_function::catalogue_call(name, &args, &mut services)
                    }
                },
            )
        })
    }
    pub fn observer(self: &Rc<Self>, events: Rc<dyn EventSource>) -> Rc<dyn WorkbookEventObserver> {
        Rc::new(AutomationObserver {
            model: self.clone(),
            events,
        })
    }
    pub fn context_changed(&self, session: &HostSession) {
        let enabled = session.workbook_events_switch();
        let previous = self.snapshot.borrow().clone();
        let workbook = match previous.as_ref() {
            Some(old) if Rc::ptr_eq(&old.enabled, &enabled) => old.workbook,
            _ => {
                if let Some(old) = previous {
                    enabled.set(old.enabled.get());
                }
                let id = self.next_workbook.get();
                self.next_workbook
                    .set(id.checked_add(1).expect("workbook identity exhausted"));
                id
            }
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
            full_name: session.file_full_name().to_owned(),
            active,
            enabled,
            workbook,
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
        if EVENT_SESSION.is_set() {
            let result = EVENT_SESSION.with(|(model, loan)| {
                if !std::ptr::eq(model, self) {
                    return None;
                }
                let status = loan.read(&mut |session| {
                    self.context_changed(session);
                    value = Some(call(Access::Session(session)));
                    Ok(AutoValue::Empty)
                });
                Some(status.and_then(|_| value.take().expect("event callback called once")))
            });
            if let Some(result) = result {
                return result;
            }
        }
        let status = self.host.with_session(&mut |session| {
            self.context_changed(session);
            value = Some(call(Access::Session(session)));
            Ok(AutoValue::Empty)
        });
        status?;
        value.expect("command callback called once")
    }
    fn validate(&self, target: &ObjectRef, meta: &Snapshot) -> Result<(), AutoError> {
        let workbook = match target.key {
            ObjectKey::Workbook(id)
            | ObjectKey::Sheets(id)
            | ObjectKey::Names(id)
            | ObjectKey::Name { workbook: id, .. } => Some(id),
            ObjectKey::Worksheet { workbook, sheet } => {
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
            Access::Session(session) => session.host().reference_view().resolve(reference),
        };
        result.map_err(|e| match e {
            ReferenceHandleError::Deleted => AutoError::excel(424, "Object required"),
            _ => AutoError::not_modelled(format!("Range reference {e}"), OWNER),
        })
    }
    fn acquire(&self, access: &mut Access<'_>, range: &ReferenceLike, axis: RangeAxis) -> Reply {
        let reference = match access {
            Access::Invocation(context) => context.retain_reference(range),
            Access::Session(session) => session.host().reference_view().retain(range),
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
        Ok(AutoValue::Object(object(
            "Range",
            if axis == RangeAxis::Cells {
                ObjectKey::Range(reference)
            } else {
                ObjectKey::RangeView { reference, axis }
            },
        )))
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
                    Access::Session(session) => session.host().resolve_reference_text(at, &text),
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
        self.fallback.invoke(target, call)
    }
    fn delegate(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, _| self.fallback.invoke(target, call))
    }
    fn application_range(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
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
    fn book_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            ValuesOnly::<Rc<dyn AutomationHost>>::workbook_item(
                call,
                book_name(&meta.full_name),
                meta.workbook,
            )
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
    fn get_active_sheet(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |access, meta| {
            no_args(call)?;
            self.active_sheet(access, meta)
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
                ObjectKey::Range(held) | ObjectKey::Areas(held) => (held, RangeAxis::Cells),
                ObjectKey::RangeView { reference, axis } => (reference, *axis),
                _ => return Err(geometry::type_mismatch()),
            };
            let range = self.resolve(access, held)?;
            f(access, meta, &range, axis)
        })
    }
    fn range_address(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, meta, range, _| {
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
            Ok(AutoValue::text(&geometry::address(
                &range,
                options,
                qualifier.as_deref(),
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
                Access::Session(session) => read_session(session, area)?,
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
    fn union(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() < 2
            || call.args.len() > 30
            || call.arg(0).is_missing()
            || call.arg(1).is_missing()
        {
            return Err(geometry::bad_count());
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
        let result = if let Some(handler) = self.handlers.get(call.interface, call.member) {
            handler(self, target, call)
        } else {
            self.fallback.invoke(target, call)
        };
        result
    }
    fn released(&self, target: &ObjectRef) {
        if let ObjectKey::Range(reference)
        | ObjectKey::RangeView { reference, .. }
        | ObjectKey::Areas(reference) = &target.key
        {
            let mut ranges = self.ranges.borrow_mut();
            if let Some(entry) = ranges.get_mut(reference) {
                entry.references.release(reference);
                entry.count -= 1;
                if entry.count == 0 {
                    ranges.remove(reference);
                }
            }
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

pub struct AutomationObserver {
    model: Rc<WorkbookAutomation>,
    events: Rc<dyn EventSource>,
}
impl WorkbookEventObserver for AutomationObserver {
    fn context_changed(&self, session: &HostSession) {
        self.model.context_changed(session);
    }
    fn worksheet_change(
        &self,
        session: &mut HostSession,
        sheet: SheetId,
        target: &[GridArea],
    ) -> Result<(), String> {
        self.model.context_changed(session);
        let reference = ReferenceLike::from_areas(target.to_vec()).map_err(|e| e.to_string())?;
        let meta = self.model.metadata().map_err(|e| e.description())?;
        let value = self
            .model
            .acquire(&mut Access::Session(session), &reference, RangeAxis::Cells)
            .map_err(|e| e.description())?;
        let args = [
            AutoValue::Object(object(
                "Worksheet",
                ObjectKey::Worksheet {
                    workbook: meta.workbook,
                    sheet,
                },
            )),
            value,
        ];
        let loan = EventLoan(RefCell::new(session));
        let result = EVENT_SESSION.set((&*self.model, &loan), || {
            self.events
                .raise(&self.model.application(), "AppEvents", "SheetChange", &args)
        });
        result.map_err(|e| e.description())
    }
    fn worksheet_calculate(&self, session: &mut HostSession, sheet: SheetId) -> Result<(), String> {
        self.model.context_changed(session);
        let meta = self.model.metadata().map_err(|e| e.description())?;
        let args = [AutoValue::Object(object(
            "Worksheet",
            ObjectKey::Worksheet {
                workbook: meta.workbook,
                sheet,
            },
        ))];
        let loan = EventLoan(RefCell::new(session));
        EVENT_SESSION
            .set((&*self.model, &loan), || {
                self.events.raise(
                    &self.model.application(),
                    "AppEvents",
                    "SheetCalculate",
                    &args,
                )
            })
            .map_err(|e| e.description())
    }
    fn after_calculate(&self, session: &mut HostSession) -> Result<(), String> {
        self.model.context_changed(session);
        let loan = EventLoan(RefCell::new(session));
        EVENT_SESSION
            .set((&*self.model, &loan), || {
                self.events.raise(
                    &self.model.application(),
                    "AppEvents",
                    "AfterCalculate",
                    &[],
                )
            })
            .map_err(|e| e.description())
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
fn read_session(session: &HostSession, area: GridArea) -> Result<CalcValue, AutoError> {
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

struct SessionCells<'a>(&'a HostSession);
impl calc_func::CellReader for SessionCells<'_> {
    fn read_area(
        &mut self,
        area: GridArea,
        visit: &mut dyn FnMut(CellKey, &CalcValue) -> std::ops::ControlFlow<()>,
    ) -> Result<(), calc_func::Unreadable> {
        let value = read_session(self.0, area).map_err(|_| calc_func::Unreadable)?;
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
