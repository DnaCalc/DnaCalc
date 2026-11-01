use calc_func::{UserFunction, UserFunctionContext, UserFunctions};
use calc_value::{CalcValue, SheetId};
use calc_xll_abi::*;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
    ffi::{CString, c_void},
    fmt,
    path::{Path, PathBuf},
    rc::Rc,
};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryExW(path: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
    fn GetModuleHandleExW(flags: u32, name: *const u16, module: *mut *mut c_void) -> i32;
    fn GetModuleFileNameW(module: *mut c_void, path: *mut u16, length: u32) -> u32;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
    pub(crate) fn GetCurrentThreadId() -> u32;
}

pub(crate) fn wide(path: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.encode_wide().chain(Some(0)).collect()
}

pub(crate) struct Module {
    pub handle: *mut c_void,
    pub path: PathBuf,
}
impl fmt::Debug for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Module").field(&self.path).finish()
    }
}
impl Module {
    fn load(path: &Path) -> Result<Rc<Self>, String> {
        let path = std::fs::canonicalize(path).map_err(|e| format!("{}: {e}", path.display()))?;
        // Search the XLL's directory and normal safe locations, never the current directory.
        let handle = unsafe {
            LoadLibraryExW(
                wide(path.as_os_str()).as_ptr(),
                std::ptr::null_mut(),
                0x1100,
            )
        };
        if handle.is_null() {
            return Err(format!(
                "LoadLibraryExW {}: {}",
                path.display(),
                std::io::Error::last_os_error()
            ));
        }
        Ok(Rc::new(Self { handle, path }))
    }
    pub(crate) fn loaded(name: &str) -> Result<Rc<Self>, String> {
        let mut handle = std::ptr::null_mut();
        if unsafe { GetModuleHandleExW(0, wide(std::ffi::OsStr::new(name)).as_ptr(), &mut handle) }
            == 0
        {
            return Err(format!("registration module is not loaded: {name}"));
        }
        let mut path = vec![0u16; 32768];
        let length =
            unsafe { GetModuleFileNameW(handle, path.as_mut_ptr(), path.len() as u32) } as usize;
        if length == 0 || length >= path.len() {
            unsafe {
                FreeLibrary(handle);
            }
            return Err("cannot read registered module path".into());
        }
        use std::os::windows::ffi::OsStringExt;
        Ok(Rc::new(Self {
            handle,
            path: std::ffi::OsString::from_wide(&path[..length]).into(),
        }))
    }
    pub(crate) fn address(&self, name: &str) -> Result<usize, String> {
        let name = CString::new(name).map_err(|_| "NUL in XLL export name")?;
        let ptr = unsafe { GetProcAddress(self.handle, name.as_ptr().cast()) };
        if ptr.is_null() {
            Err(format!(
                "missing export {} in {}",
                name.to_string_lossy(),
                self.path.display()
            ))
        } else {
            Ok(ptr as usize)
        }
    }
    pub(crate) fn ordinal(&self, ordinal: u16) -> Result<usize, String> {
        let ptr = unsafe { GetProcAddress(self.handle, usize::from(ordinal) as *const u8) };
        if ptr.is_null() {
            Err(format!("missing export ordinal {ordinal}"))
        } else {
            Ok(ptr as usize)
        }
    }
}
impl Drop for Module {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.handle);
        }
    }
}

#[derive(Clone, Debug)]
pub struct RegistrationInfo {
    pub id: u64,
    pub name: String,
    pub procedure: String,
    pub type_text: String,
    pub signature: XllSignature,
    pub address: usize,
    pub module: PathBuf,
    pub origin: PathBuf,
    pub use_count: u32,
    pub macro_type: i32,
}
pub(crate) struct Registration {
    pub info: RegistrationInfo,
    pub module: Rc<Module>,
    pub origin: Rc<Module>,
    pub uses: Cell<u32>,
    pub cif: Option<libffi::middle::Cif>,
}
pub(crate) struct State {
    pub modules: Vec<Rc<Module>>,
    pub registrations: BTreeMap<u64, Rc<Registration>>,
    pub names: BTreeMap<String, u64>,
    pub next_id: u64,
    pub sheets: SheetMap,
    pub allocations: HashMap<usize, CallbackAllocation>,
    pub diagnostics: Vec<String>,
    pub hwnd: usize,
    pub thread: u32,
    pub closed: bool,
}
impl State {
    pub fn refusal(&mut self, why: impl Into<String>) {
        self.diagnostics.push(why.into());
    }
}
pub(crate) type Shared = Rc<RefCell<State>>;
pub(crate) struct CallbackAllocation {
    pub owner: usize,
    pub _arena: Arena,
}

/// Main-thread owned XLL set. Registrations are caller-owned and can be snapshotted into
/// an existing OneCalc/CalcHost session. Each adapter leases its exact module and address.
pub struct XllHost {
    pub(crate) state: Shared,
}
impl XllHost {
    pub fn new(hwnd: usize) -> Self {
        Self {
            state: Rc::new(RefCell::new(State {
                modules: Vec::new(),
                registrations: BTreeMap::new(),
                names: BTreeMap::new(),
                next_id: 1,
                sheets: SheetMap::default(),
                allocations: HashMap::new(),
                diagnostics: Vec::new(),
                hwnd,
                thread: unsafe { GetCurrentThreadId() },
                closed: false,
            })),
        }
    }
    pub fn load(&self, path: &Path) -> Result<(), String> {
        if self.state.borrow().closed {
            return Err("XLL host is closed".into());
        }
        let module = Module::load(path)?;
        if self
            .state
            .borrow()
            .modules
            .iter()
            .any(|m| m.handle == module.handle)
        {
            return Err("XLL already loaded by this host".into());
        }
        let open = module.address("xlAutoOpen")?;
        // xlAutoOpen runs at an execution boundary. A failed load must restore names and
        // use counts it changed in previously loaded add-ins, as well as remove its own entries.
        let (previous_registrations, previous_names, previous_uses) = {
            let state = self.state.borrow();
            (
                state.registrations.clone(),
                state.names.clone(),
                state
                    .registrations
                    .values()
                    .map(|r| (r.clone(), r.uses.get()))
                    .collect::<Vec<_>>(),
            )
        };
        self.state.borrow_mut().modules.push(module.clone());
        let mut frame = crate::callbacks::Frame::lifecycle(self.state.clone(), module.clone());
        let guard = unsafe { crate::callbacks::enter(&mut frame) };
        let function: unsafe extern "system" fn() -> i32 = unsafe { std::mem::transmute(open) };
        let result = unsafe { function() };
        drop(guard);
        if result == 0 {
            self.remove_module(&module);
            let mut state = self.state.borrow_mut();
            for (id, registration) in &state.registrations {
                if !previous_registrations.contains_key(id) {
                    registration.uses.set(0);
                }
            }
            state.registrations = previous_registrations;
            state.names = previous_names;
            for (registration, uses) in previous_uses {
                registration.uses.set(uses);
            }
            return Err(format!(
                "xlAutoOpen returned zero: {}",
                module.path.display()
            ));
        }
        Ok(())
    }
    fn remove_module(&self, module: &Rc<Module>) {
        let mut state = self.state.borrow_mut();
        let ids = state
            .registrations
            .iter()
            .filter_map(|(id, r)| (r.origin.handle == module.handle).then_some(*id))
            .collect::<Vec<_>>();
        for id in ids {
            if let Some(r) = state.registrations.remove(&id) {
                r.uses.set(0);
            }
            state.names.retain(|_, n| *n != id);
        }
        state.modules.retain(|m| m.handle != module.handle);
        state
            .allocations
            .retain(|_, allocation| allocation.owner != module.handle as usize);
    }
    pub fn functions(&self) -> Result<UserFunctions, String> {
        let state = self.state.borrow();
        let mut functions = UserFunctions::new();
        for (name, id) in &state.names {
            let Some(registration) = state.registrations.get(id) else {
                continue;
            };
            if registration.uses.get() == 0 || registration.info.macro_type == 2 {
                continue;
            }
            functions.register(
                name,
                Rc::new(XllFunction {
                    state: self.state.clone(),
                    registration: registration.clone(),
                }),
            )?;
        }
        Ok(functions)
    }
    pub fn registrations(&self) -> Vec<RegistrationInfo> {
        self.state
            .borrow()
            .registrations
            .values()
            .map(|r| {
                let mut i = r.info.clone();
                i.use_count = r.uses.get();
                i
            })
            .collect()
    }
    pub fn diagnostics(&self) -> Vec<String> {
        self.state.borrow().diagnostics.clone()
    }
    pub fn live_callback_allocations(&self) -> usize {
        self.state.borrow().allocations.len()
    }
    pub fn close(&self) -> Result<(), String> {
        if self.state.borrow().closed {
            return Ok(());
        }
        let modules = self.state.borrow().modules.clone();
        let mut failures = Vec::new();
        for module in modules.iter().rev() {
            if let Ok(address) = module.address("xlAutoClose") {
                let mut frame =
                    crate::callbacks::Frame::lifecycle(self.state.clone(), module.clone());
                let guard = unsafe { crate::callbacks::enter(&mut frame) };
                let close: unsafe extern "system" fn() -> i32 =
                    unsafe { std::mem::transmute(address) };
                if unsafe { close() } == 0 {
                    failures.push(module.path.display().to_string());
                }
                drop(guard);
            }
            self.remove_module(module);
        }
        self.state.borrow_mut().closed = true;
        if failures.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "xlAutoClose returned zero: {}",
                failures.join(", ")
            ))
        }
    }
}
impl Drop for XllHost {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            self.state.borrow_mut().refusal(e);
        }
    }
}

pub(crate) struct XllFunction {
    pub state: Shared,
    pub registration: Rc<Registration>,
}
impl fmt::Debug for XllFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("XllFunction")
            .field("registration", &self.registration.info)
            .finish()
    }
}
impl UserFunction for XllFunction {
    fn call(&self, arguments: &[CalcValue]) -> Result<CalcValue, String> {
        self.invoke(arguments, None)
    }
    fn call_with_context(
        &self,
        arguments: &[CalcValue],
        context: &mut dyn UserFunctionContext,
    ) -> Result<CalcValue, String> {
        let result = self.invoke(arguments, Some(&mut *context));
        if let Err(why) = &result {
            if why.starts_with("dc-snz.2:") {
                context.refuse_by("an XLL invocation ABI is not modelled", "dc-snz.2");
            }
        }
        result
    }
}

pub(crate) fn current_sheet(context: &Option<&mut dyn UserFunctionContext>) -> Option<SheetId> {
    context.as_ref().and_then(|c| c.caller()).map(|c| c.sheet())
}
