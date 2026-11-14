//! The workbook object model add-ins see, bound to DNA Calc's session: one binding for both
//! modes. App mode reaches its session through the page's `Server`; the `--engine` harness
//! holds its `HostSession` directly. Member semantics live in `calc_host::automation`.
use calc_addin_host::AddinHost;
use dna_calc_worker::server::Server;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

/// Where the binding finds the session for a command-context call (outside a worksheet
/// invocation and outside an event, which carry their own access).
pub(crate) enum SessionAccess {
    Server(Weak<RefCell<Server>>),
    Session(Weak<RefCell<calc_host::HostSession>>),
}

/// Session access for the workbook profile.
struct DesktopAutomationHost {
    application: Rc<dyn calc_object_model::profile::ApplicationHost>,
    native: Weak<calc_xll_host::XllHost>,
    session: SessionAccess,
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
        self.application.events_enabled()
    }
    fn set_events_enabled(&self, enabled: bool) {
        self.application.set_events_enabled(enabled);
    }
    fn path(&self) -> Result<String, calc_object_model::AutoError> {
        self.application.path()
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
                "dc-0f1k.24",
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
        match &self.session {
            SessionAccess::Server(server) => {
                let server = server
                    .upgrade()
                    .ok_or(calc_object_model::AutoError::Disconnected)?;
                let server = server
                    .try_borrow()
                    .map_err(|_| calc_host::automation::busy())?;
                call(server.book().session())
            }
            SessionAccess::Session(session) => {
                let session = session
                    .upgrade()
                    .ok_or(calc_object_model::AutoError::Disconnected)?;
                let session = session
                    .try_borrow()
                    .map_err(|_| calc_host::automation::busy())?;
                call(&session)
            }
        }
    }
    fn context_changed(&self, session: &calc_host::HostSession) {
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
    let host = Rc::new(DesktopAutomationHost {
        application: projection.application_host(),
        native: Rc::downgrade(&addins.native_handle()),
        session,
    });
    let model = Rc::new(calc_host::automation::WorkbookAutomation::new(host));
    projection.bind_model(model.clone())?;
    let observer = model.observer(projection);
    Ok((model, observer))
}
