use calc_com_host::{Projection, RtdClient, RtdInput};
use calc_fml::CallableTable;
use calc_platform_desktop::Shell;
use calc_value::{CalcValue, SheetId};
use calc_xll_host::{AsyncReturn, RegistrationInfo, XllHost};
use std::{
    path::{Path, PathBuf},
    rc::Rc,
};

/// The workbook an add-in set is told about: its name, the sheets it shows and the active one.
/// An add-in sees these through the Excel object model and the C API's sheet names.
#[derive(Clone, Copy, Debug)]
pub struct Document<'a> {
    pub workbook: &'a str,
    pub active_sheet: Option<&'a str>,
    pub sheets: &'a [(SheetId, &'a str)],
}

/// Everything an application has loaded from add-ins, and the host objects they run against.
///
/// Create it with the application's [`Shell`]: it answers the add-ins' `WM_GETOBJECT` on that
/// window chain. Drop it before the shell.
pub struct AddinHost {
    native: Rc<XllHost>,
    projection: Rc<Projection>,
    rtd: Option<Rc<RtdClient>>,
    xlls: Vec<PathBuf>,
    vba: Vec<(String, CallableTable)>,
    workbook: String,
}

/// Publishes the window chain's object model and tells it the workbook, in the order Excel's
/// startup does: sheet identities first, then the object model, then the names it exposes.
fn attach(
    shell: &Shell,
    native: &Rc<XllHost>,
    document: &Document<'_>,
) -> Result<Rc<Projection>, String> {
    for (id, name) in document.sheets {
        native.set_sheet(*id, format!("[{}]{name}", document.workbook), true);
    }
    let projection = Rc::new(Projection::new(shell.hwnd(), native.clone())?);
    projection.verify_layout()?;
    let answering = projection.clone();
    shell.set_object_handler(Some(Rc::new(move |w, l| answering.get_object(w, l))));
    projection.set_workbook(document.workbook)?;
    if let Some(sheet) = document.active_sheet {
        projection.set_sheet(sheet)?;
    }
    Ok(projection)
}

impl AddinHost {
    /// An empty add-in set against `document`, with its object model attached to `shell`.
    pub fn new(shell: &Shell, document: &Document<'_>) -> Result<Self, String> {
        let native = Rc::new(XllHost::new(shell.hwnd()));
        let projection = attach(shell, &native, document)?;
        Ok(Self {
            native,
            projection,
            rtd: None,
            xlls: Vec::new(),
            vba: Vec::new(),
            workbook: document.workbook.to_owned(),
        })
    }

    /// Replaces the native host and its object model with new, empty ones (the previous one's
    /// add-ins are closed first), as a process that reloads every XLL from nothing needs. VBA
    /// modules stay loaded; the RTD client and every XLL are gone.
    pub fn restart(&mut self, shell: &Shell, document: &Document<'_>) -> Result<(), String> {
        self.projection.close_addins()?;
        let native = Rc::new(XllHost::new(shell.hwnd()));
        let projection = attach(shell, &native, document)?;
        self.native = native;
        self.projection = projection;
        self.rtd = None;
        self.xlls.clear();
        self.workbook = document.workbook.to_owned();
        Ok(())
    }

    /// The name add-ins see for the workbook.
    pub fn workbook(&self) -> &str {
        &self.workbook
    }

    /// Tells the add-ins a sheet's identity and its name in the workbook, active.
    pub fn bind_sheet(&self, id: SheetId, name: &str) {
        self.native
            .set_sheet(id, format!("[{}]{name}", self.workbook), true);
    }

    /// Loads an XLL, running its open routine. A failure rolls the whole load back; the error
    /// says what is left registered afterwards, so a caller can see the rollback was complete.
    pub fn load(&mut self, path: &Path) -> Result<(), String> {
        if let Err(why) = self.native.load(path) {
            let active = self
                .native
                .registrations()
                .iter()
                .filter(|r| r.use_count > 0)
                .count();
            let names = self.native.functions().map(|f| f.names().count())?;
            return Err(format!(
                "{why}; after rollback: {active} active registrations, {names} callable names, {} callback allocations",
                self.native.live_callback_allocations()
            ));
        }
        self.xlls.push(path.to_owned());
        Ok(())
    }

    /// Runs the XLL's close routine and removes its registrations.
    pub fn unload(&mut self, path: &Path) -> Result<(), String> {
        self.native.unload(path)?;
        self.xlls.retain(|loaded| loaded != path);
        Ok(())
    }

    /// The XLLs loaded, in load order.
    pub fn xlls(&self) -> &[PathBuf] {
        &self.xlls
    }

    /// Every registration the XLLs made, including macros and hidden ones.
    pub fn registrations(&self) -> Vec<RegistrationInfo> {
        self.native.registrations()
    }

    /// The id of the active registration of `name`, as early-execution grants name it.
    pub fn active_registration_id(&self, name: &str) -> Option<u64> {
        self.native.active_registration_id(name)
    }

    /// Compiles a VBA standard module and publishes its public functions with the others. A module
    /// of the same name is replaced; a module that does not compile changes nothing.
    pub fn load_vba_module(
        &mut self,
        name: &str,
        source: &str,
        max_steps: u64,
    ) -> Result<(), String> {
        let functions = calc_vba::load_module(name, source, max_steps)?;
        match self.vba.iter_mut().find(|(held, _)| held == name) {
            Some(held) => held.1 = functions,
            None => self.vba.push((name.to_owned(), functions)),
        }
        Ok(())
    }

    /// Removes a VBA module; `false` when none of that name was loaded.
    pub fn unload_vba_module(&mut self, name: &str) -> bool {
        let before = self.vba.len();
        self.vba.retain(|(held, _)| held != name);
        self.vba.len() != before
    }

    /// The VBA modules loaded, in load order.
    pub fn vba_modules(&self) -> impl Iterator<Item = &str> {
        self.vba.iter().map(|(name, _)| name.as_str())
    }

    /// The one table of every loaded function (XLL registrations and VBA functions), to hand to
    /// the application's owner of formulas. A new table each call: one published after a load or
    /// unload replaces the one before it, and a handle bound to the old one stays unable to run.
    pub fn callables(&self) -> Result<CallableTable, String> {
        self.callables_with_early_execution(&[])
    }

    /// [`Self::callables`], with the XLL registrations in `early_execution` (ids from
    /// [`Self::active_registration_id`]) granted execution before their dependencies settle.
    pub fn callables_with_early_execution(
        &self,
        early_execution: &[u64],
    ) -> Result<CallableTable, String> {
        let mut table = self.native.functions_with_early_execution(early_execution)?;
        for (_, module) in &self.vba {
            let registrations = module
                .registrations()
                .map(|(provider, name)| (provider.map(str::to_owned), name.to_owned()))
                .collect::<Vec<_>>();
            for (provider, name) in registrations {
                let function = module
                    .bind(provider.as_deref(), &name)
                    .and_then(|handle| module.resolve(handle))
                    .ok_or("a VBA registration lost its function")?;
                table.register_for(provider.as_deref(), &name, Rc::clone(function))?;
            }
        }
        Ok(table)
    }

    /// Lets native add-ins return asynchronously: the returns arrive through
    /// [`Self::take_async_returns`] instead of being waited for inside the call.
    pub fn enable_async_continuations(&self) -> Result<(), String> {
        self.native.set_async_continuations(true)
    }

    /// The asynchronous native returns that have arrived, oldest first.
    pub fn take_async_returns(&self) -> Vec<AsyncReturn> {
        self.native.take_async_returns()
    }

    /// Starts the RTD client the XLLs' real-time-data calls connect through.
    pub fn enable_rtd(&mut self) -> Result<Rc<RtdClient>, String> {
        let client = Rc::new(RtdClient::new()?);
        self.native.set_rtd_service(client.clone())?;
        self.rtd = Some(client.clone());
        Ok(client)
    }

    /// The RTD client, once [`Self::enable_rtd`] started it.
    pub fn rtd(&self) -> Option<&Rc<RtdClient>> {
        self.rtd.as_ref()
    }

    /// The RTD updates that have arrived, oldest first (none without a client). While the owner
    /// is evaluating, only the updates its evaluation may see are taken.
    pub fn poll_rtd(&self, evaluating: bool) -> Result<Vec<RtdInput>, String> {
        match &self.rtd {
            None => Ok(Vec::new()),
            Some(client) if evaluating => client.poll_during_evaluation(),
            Some(client) => client.poll(),
        }
    }

    /// Runs a macro or registered command on the add-ins' own thread.
    pub fn run(&self, name: &str, arguments: &[CalcValue]) -> Result<CalcValue, String> {
        self.native.run(name, arguments)
    }

    /// Runs a command through the early-bound Excel `Application.Run` of the object model.
    pub fn run_early(&self, name: &str) -> Result<CalcValue, String> {
        self.projection.run_early(name)
    }

    /// Names the active sheet to the object model.
    pub fn set_active_sheet(&self, name: &str) -> Result<(), String> {
        self.projection.set_sheet(name)
    }

    /// Tells add-ins that a cell's input changed (the `SheetChange` event).
    pub fn sheet_changed(&self, row: u32, column: u32) -> Result<(), String> {
        self.projection.sheet_change(row, column)
    }

    /// Tells add-ins a calculation finished (the `AfterCalculate` event).
    pub fn after_calculate(&self) -> Result<(), String> {
        self.projection.after_calculate()
    }

    /// Turns the application's events on or off for the add-ins.
    pub fn enable_events(&self, enabled: bool) -> Result<(), String> {
        self.projection.enable_events(enabled)
    }

    /// The ribbon XML COM add-ins supplied, as (add-in, XML).
    pub fn ribbons(&self) -> Result<Vec<(String, String)>, String> {
        self.projection.ribbons()
    }

    /// Closes every COM add-in. [`Drop`] does it too; closing twice is harmless.
    pub fn close_addins(&self) -> Result<(), String> {
        self.projection.close_addins()
    }

    /// The native host, for what an application reads of it: counters, diagnostics, the C API's
    /// status line, outstanding calls, teardown.
    pub fn native(&self) -> &XllHost {
        &self.native
    }

    /// The Excel object model add-ins see.
    pub fn projection(&self) -> &Projection {
        &self.projection
    }
}

impl Drop for AddinHost {
    fn drop(&mut self) {
        if let Err(why) = self.projection.close_addins() {
            eprintln!("desktop shutdown: {why}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_value::{CalcValue, WorksheetErrorCode};

    fn shell_and_host() -> (Shell, AddinHost) {
        let shell = Shell::new("calc-addin-host test").unwrap();
        let document = Document {
            workbook: "Book1",
            active_sheet: Some("Sheet1"),
            sheets: &[],
        };
        let host = AddinHost::new(&shell, &document).unwrap();
        (shell, host)
    }

    /// Native-only: creates the window chain and the object model, as the applications do.
    /// VBA modules publish through the one callable table, a replacement keeps one copy of each
    /// name, a module that does not compile changes nothing, and a removed module's names are
    /// no longer callable.
    #[test]
    fn vba_modules_share_the_one_callable_table() {
        let (_shell, mut host) = shell_and_host();
        assert_eq!(host.callables().unwrap().names().count(), 0);
        host.load_vba_module("Module1", calc_vba::EXAMPLE_MODULE, calc_vba::DEFAULT_STEP_BUDGET)
            .unwrap();
        let table = host.callables().unwrap();
        assert!(table.contains("Add2") && table.contains("Module1.Add2"));
        let before = table.names().count();
        // The same module again replaces; it does not duplicate.
        host.load_vba_module("Module1", calc_vba::EXAMPLE_MODULE, calc_vba::DEFAULT_STEP_BUDGET)
            .unwrap();
        assert_eq!(host.callables().unwrap().names().count(), before);
        assert!(host.load_vba_module("Broken", "Function (", 1000).is_err());
        assert_eq!(host.vba_modules().collect::<Vec<_>>(), ["Module1"]);
        assert!(host.unload_vba_module("Module1"));
        assert!(!host.unload_vba_module("Module1"));
        let table = host.callables().unwrap();
        assert!(!table.contains("Add2"));
        assert_eq!(
            table.call("Add2", &[CalcValue::number(1.)]).unwrap(),
            CalcValue::error(WorksheetErrorCode::Name)
        );
    }

    /// Native-only: a restart gives the add-in set a fresh native host and object model while the
    /// VBA modules stay, and the old object model's add-ins are closed first.
    #[test]
    fn a_restart_keeps_the_vba_modules_and_forgets_the_xlls() {
        let (shell, mut host) = shell_and_host();
        host.load_vba_module("Module1", calc_vba::EXAMPLE_MODULE, calc_vba::DEFAULT_STEP_BUDGET)
            .unwrap();
        host.enable_rtd().unwrap();
        assert!(host.rtd().is_some());
        let document = Document {
            workbook: "Book2",
            active_sheet: None,
            sheets: &[],
        };
        host.restart(&shell, &document).unwrap();
        assert_eq!(host.workbook(), "Book2");
        assert!(host.rtd().is_none() && host.xlls().is_empty());
        assert!(host.callables().unwrap().contains("Add2"));
        assert!(host.poll_rtd(false).unwrap().is_empty());
    }
}
