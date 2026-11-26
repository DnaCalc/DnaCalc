//! The workbook object model add-ins see, bound to DNA Calc's session: one binding for both
//! modes. App mode reaches its session through the page's `Server`; the `--engine` harness
//! holds its `HostSession` directly. Member semantics live in `calc_host::automation`.
use calc_addin_host::AddinHost;
use dna_calc_worker::server::Server;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

/// Where the binding finds the session for a command-context call (outside a worksheet
/// invocation and outside an event, which carry their own access).
pub(crate) enum SessionAccess {
    Server(Weak<RefCell<Server>>),
    Session(Weak<RefCell<calc_host::HostSession>>),
}

impl SessionAccess {
    fn with<T>(
        &self,
        call: impl FnOnce(&calc_host::HostSession) -> T,
    ) -> Result<T, calc_object_model::AutoError> {
        match self {
            SessionAccess::Server(server) => {
                let server = server
                    .upgrade()
                    .ok_or(calc_object_model::AutoError::Disconnected)?;
                let server = server
                    .try_borrow()
                    .map_err(|_| calc_host::automation::busy())?;
                Ok(call(server.book().session()))
            }
            SessionAccess::Session(session) => {
                let session = session
                    .upgrade()
                    .ok_or(calc_object_model::AutoError::Disconnected)?;
                let session = session
                    .try_borrow()
                    .map_err(|_| calc_host::automation::busy())?;
                Ok(call(&session))
            }
        }
    }
}

/// Session access for the workbook profile.
struct DesktopAutomationHost {
    application: Rc<dyn calc_object_model::profile::ApplicationHost>,
    native: Weak<calc_xll_host::XllHost>,
    session: SessionAccess,
    events: RefCell<Rc<Cell<bool>>>,
}
impl calc_object_model::profile::ApplicationHost for DesktopAutomationHost {
    fn hwnd(&self) -> isize {
        self.application.hwnd()
    }
    fn run(
        &self,
        name: &str,
        args: &[calc_object_model::AutoValue],
    ) -> Result<calc_object_model::AutoValue, calc_object_model::AutoError> {
        self.application.run(name, args)
    }
    fn events_enabled(&self) -> bool {
        self.events.borrow().get()
    }
    fn set_events_enabled(&self, enabled: bool) {
        self.events.borrow().set(enabled);
    }
    fn path(&self) -> Result<String, calc_object_model::AutoError> {
        self.application.path()
    }
    fn write_file(&self, path: &str, bytes: &[u8]) -> Result<(), calc_object_model::AutoError> {
        std::fs::write(path, bytes).map_err(|error| {
            calc_object_model::AutoError::excel(1004, format!("Cannot write {path}: {error}"))
        })
    }
}
impl calc_host::automation::AutomationHost for DesktopAutomationHost {
    fn with_invocation(
        &self,
        call: &mut dyn FnMut(
            &mut dyn calc_func::UserFunctionContext,
        )
            -> Result<calc_object_model::AutoValue, calc_object_model::AutoError>,
    ) -> Option<Result<calc_object_model::AutoValue, calc_object_model::AutoError>> {
        let native = self.native.upgrade()?;
        let mut answer = None;
        let status = native.try_with_workbook_context(|context| {
            answer = Some(call(context));
            Ok(())
        });
        if answer.is_some() {
            return answer;
        }
        match status {
            Ok(None) => None,
            Err(why) => Some(Err(calc_object_model::AutoError::not_modelled(
                why,
                "dc-17spk",
            ))),
            Ok(Some(())) => Some(Err(calc_host::automation::busy())),
        }
    }
    fn with_session(
        &self,
        call: &mut dyn FnMut(
            &calc_host::HostSession,
        )
            -> Result<calc_object_model::AutoValue, calc_object_model::AutoError>,
    ) -> Result<calc_object_model::AutoValue, calc_object_model::AutoError> {
        self.session.with(call)?
    }
    fn with_session_mut(
        &self,
        call: &mut dyn FnMut(
            &mut calc_host::HostSession,
        )
            -> Result<calc_object_model::AutoValue, calc_object_model::AutoError>,
    ) -> Result<calc_object_model::AutoValue, calc_object_model::AutoError> {
        match &self.session {
            SessionAccess::Server(server) => {
                let server = server
                    .upgrade()
                    .ok_or(calc_object_model::AutoError::Disconnected)?;
                let mut server = server
                    .try_borrow_mut()
                    .map_err(|_| calc_host::automation::busy())?;
                server.book_mut().with_automation_session(|session| call(session))
            }
            SessionAccess::Session(session) => {
                let session = session
                    .upgrade()
                    .ok_or(calc_object_model::AutoError::Disconnected)?;
                let mut session = session
                    .try_borrow_mut()
                    .map_err(|_| calc_host::automation::busy())?;
                call(&mut session)
            }
        }
    }
    fn context_changed(&self, session: &calc_host::HostSession) {
        self.events.replace(session.workbook_events_switch());
        if let Some(native) = self.native.upgrade() {
            let active = session
                .selection()
                .active
                .map(|at| at.sheet())
                .or_else(|| session.host().loaded().active_sheet())
                .or_else(|| session.host().sheets().first().map(|sheet| sheet.id()));
            let full = session.file_full_name();
            let name = full.rsplit(['/', '\\']).next().unwrap_or(full);
            for sheet in session.host().sheets() {
                native.set_sheet(
                    sheet.id(),
                    format!("[{name}]{}", sheet.name().text().to_string_lossy()),
                    Some(sheet.id()) == active,
                );
            }
        }
    }
}

/// Binds the workbook profile into the add-ins' projection, before any add-in is loaded. The
/// caller attaches the returned observer to its session: the session's one event source then
/// raises SheetChange (every committed input area, after the VBA handler) and AfterCalculate.
pub(crate) fn bind_automation(
    addins: &AddinHost,
    session: SessionAccess,
) -> Result<
    (
        Rc<calc_host::automation::WorkbookAutomation>,
        Rc<dyn calc_host::WorkbookEventObserver>,
    ),
    String,
> {
    let projection = addins.projection_handle();
    let events = session
        .with(calc_host::HostSession::workbook_events_switch)
        .map_err(|error| error.description())?;
    calc_chart_render::install();
    let host = Rc::new(DesktopAutomationHost {
        application: projection.application_host(),
        native: Rc::downgrade(&addins.native_handle()),
        session,
        events: RefCell::new(events),
    });
    let model = Rc::new(calc_host::automation::WorkbookAutomation::new(host));
    projection.bind_model(model.clone())?;
    let observer = model.observer(projection);
    Ok((model, observer))
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Native-only: the desktop host shares the session switch even while the server is borrowed.
    #[test]
    fn automation_host_enable_events_uses_the_session_switch_during_callbacks() {
        use calc_host::automation::AutomationHost;
        use calc_object_model::profile::ApplicationHost;
        let application =
            calc_com_host::Projection::new(1, Rc::new(calc_xll_host::XllHost::new(1))).unwrap();
        let application = application.application_host();
        application.set_events_enabled(true);
        let server = Rc::new(RefCell::new(Server::new()));
        let events = server.borrow().book().session().workbook_events_switch();
        let host = DesktopAutomationHost {
            application: application.clone(),
            native: std::rc::Weak::new(),
            session: SessionAccess::Server(Rc::downgrade(&server)),
            events: RefCell::new(events.clone()),
        };
        let callback = server.borrow_mut();
        callback
            .book()
            .session()
            .workbook_events_switch()
            .set(false);
        assert!(!host.events_enabled());
        host.set_events_enabled(true);
        assert!(callback.book().session().workbook_events_enabled());
        assert!(application.events_enabled());
        host.set_events_enabled(false);
        assert!(!callback.book().session().workbook_events_enabled());
        assert!(
            application.events_enabled(),
            "DNA Calc never writes the values-only host switch"
        );
        drop(callback);
        let replacement = Server::new();
        host.context_changed(replacement.book().session());
        assert!(host.events_enabled());
        host.set_events_enabled(false);
        assert!(!replacement.book().session().workbook_events_enabled());
        assert!(!events.get());
    }

    /// Native-only: drives the page's session route (a `Server` lent mutably by
    /// `with_session_mut`) with command-context writes, then reads the workbook state back, as
    /// the engine-route `dc_piw0__excel_answers_*` tests do.
    #[test]
    fn desktop_command_route_writes_range_table_shape_and_chart_through_the_server() {
        use calc_object_model::{AutoValue, Call, Descriptor, InvokeKind, ObjectModel, ObjectRef};
        let application =
            calc_com_host::Projection::new(1, Rc::new(calc_xll_host::XllHost::new(1))).unwrap();
        let server = Rc::new(RefCell::new(Server::new()));
        let events = server.borrow().book().session().workbook_events_switch();
        let host = Rc::new(DesktopAutomationHost {
            application: application.application_host(),
            native: std::rc::Weak::new(),
            session: SessionAccess::Server(Rc::downgrade(&server)),
            events: RefCell::new(events),
        });
        let model = calc_host::automation::WorkbookAutomation::new(host);
        let call = |object: &ObjectRef, name: &str, kind: InvokeKind, args: Vec<AutoValue>| {
            model.invoke(
                object,
                &Call {
                    interface: object.interface,
                    member: Descriptor::get().member(object.interface, name, kind).unwrap(),
                    args,
                    lcid: 0,
                },
            )
        };
        let get = |object: &ObjectRef, name: &str| call(object, name, InvokeKind::Get, vec![]).unwrap();
        let method = |object: &ObjectRef, name: &str, args: Vec<AutoValue>| {
            match call(object, name, InvokeKind::Method, args).unwrap() {
                AutoValue::Object(o) => o,
                other => panic!("{name}: {other:?}"),
            }
        };
        let object = |value: AutoValue| match value {
            AutoValue::Object(o) => o,
            other => panic!("object: {other:?}"),
        };
        let text = |o: &ObjectRef, name: &str| get(o, name).as_text().unwrap();
        let range = |address: &str| {
            object(call(&model.application(), "Range", InvokeKind::Get, vec![AutoValue::text(address)]).unwrap())
        };
        assert!(!server.borrow().book().changed());

        // Range.Value put, cell by cell, through with_session_mut.
        let rows = [["Region", "Q1", "Q2"], ["North", "10", "20"], ["South", "30", "40"]];
        for (r, row) in rows.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                let at = range(&format!("{}{}", (b'A' + c as u8) as char, r + 1));
                let value = cell.parse::<f64>().map_or_else(|_| AutoValue::text(cell), AutoValue::R8);
                call(&at, "Value2", InvokeKind::Put, vec![value]).unwrap();
            }
        }
        assert!(server.borrow().book().changed(), "an authored write marks the workbook changed");
        assert_eq!(text(&range("A2"), "Text"), "North");
        assert_eq!(text(&range("C3"), "Text"), "40");

        // ListObjects.Add(1, Range("A1:C3"), , 1).
        let sheet = object(get(&model.application(), "ActiveSheet"));
        let tables = object(get(&sheet, "ListObjects"));
        let table = method(&tables, "Add", vec![
            AutoValue::I4(1), AutoValue::Object(range("A1:C3")), AutoValue::Missing, AutoValue::I4(1),
        ]);
        assert_eq!(text(&table, "Name"), "Table1");
        assert_eq!(text(&object(get(&table, "Range")), "Address"), "$A$1:$C$3");

        // Shapes.AddShape(1, 20, 120, 100, 50).
        let shapes = object(get(&sheet, "Shapes"));
        let rectangle = method(&shapes, "AddShape", vec![
            AutoValue::I4(1), AutoValue::R8(20.0), AutoValue::R8(120.0), AutoValue::R8(100.0), AutoValue::R8(50.0),
        ]);
        assert_eq!(text(&rectangle, "Name"), "Rectangle 1");

        // ChartObjects.Add(200, 20, 300, 200) then Chart.SetSourceData(table range).
        let charts = method(&sheet, "ChartObjects", vec![]);
        let frame = method(&charts, "Add", vec![
            AutoValue::R8(200.0), AutoValue::R8(20.0), AutoValue::R8(300.0), AutoValue::R8(200.0),
        ]);
        assert_eq!(text(&frame, "Name"), "Chart 2", "named after the shape count, as Excel does");
        let chart = object(get(&frame, "Chart"));
        call(&chart, "SetSourceData", InvokeKind::Method, vec![AutoValue::Object(range("A1:C3"))]).unwrap();
        let series = method(&chart, "SeriesCollection", vec![]);
        let count = match get(&series, "Count") {
            AutoValue::I4(n) => n,
            other => panic!("{other:?}"),
        };
        assert_eq!(count, 2);

        // The state is in the page's workbook, not a copy.
        let server = server.borrow();
        let session = server.book().session();
        let sheet_id = session.host().sheets()[0].id();
        assert_eq!(session.host().shapes(sheet_id).len(), 1, "the rectangle");
        let (charts, _) = session.host().charts(sheet_id);
        assert_eq!(charts.len(), 1, "the chart frame");
        assert!(server.book().changed());
    }
}
