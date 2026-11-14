//! COM-owned Office members composed with the supplied object model.
//! State transitions and native leases preserve projection-office.h at ef34ca50.
use crate::{
    interfaces::{IDTExtensibility2, IRibbonExtensibility},
    projection::State,
    runtime::id,
    values::{self, Objects, status},
};
use calc_object_model::{AutoError, AutoValue, Call, Handlers, InvokeKind, ObjectKey, ObjectRef};
use calc_value::ExcelText;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::{Path, PathBuf},
    ptr,
    rc::{Rc, Weak},
};
use windows::{
    Win32::{
        Foundation::*,
        System::{Com::*, LibraryLoader::*, Ole::*, Registry::*, Variant::*},
    },
    core::{BSTR, GUID, HRESULT, Interface, PCWSTR, PWSTR},
};

type Result<T> = std::result::Result<T, AutoError>;
fn failure<T>(code: HRESULT) -> Result<T> {
    Err(AutoError::Status(code.0))
}
fn text(value: &str) -> ExcelText {
    ExcelText::from_utf16_code_units(value.encode_utf16().collect())
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
pub(crate) struct ModuleLease(pub HMODULE);
impl Drop for ModuleLease {
    fn drop(&mut self) {
        let _ = unsafe { FreeLibrary(self.0) };
    }
}
impl ModuleLease {
    fn load(path: &Path) -> Result<Self> {
        use std::os::windows::ffi::OsStrExt;
        let path: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            LoadLibraryExW(
                PCWSTR(path.as_ptr()),
                None,
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
            )
        }
        .map(Self)
        .map_err(status)
    }
    fn retain(path: &Path) -> Result<Self> {
        use std::os::windows::ffi::OsStrExt;
        let path: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut module = HMODULE::default();
        unsafe { GetModuleHandleExW(0, PCWSTR(path.as_ptr()), &mut module) }.map_err(status)?;
        Ok(Self(module))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Addins,
    Addin,
    Bars,
    Bar,
    Controls,
    Popup,
    Button,
}
impl Role {
    fn interface(self) -> &'static str {
        match self {
            Self::Addins => "COMAddIns",
            Self::Addin => "COMAddIn",
            Self::Bars => "_CommandBars",
            Self::Bar => "CommandBar",
            Self::Controls => "CommandBarControls",
            Self::Popup => "CommandBarPopup",
            Self::Button => "_CommandBarButton",
        }
    }
}
struct Node {
    target: ObjectRef,
    parent: ObjectRef,
    children: RefCell<Vec<u64>>,
    controls: Cell<Option<u64>>,
    caption: RefCell<ExcelText>,
    action: RefCell<ExcelText>,
    deleted: Cell<bool>,
    addin: RefCell<Option<Rc<Addin>>>,
}
struct Addin {
    clsid: GUID,
    progid: String,
    module: PathBuf,
    description: ExcelText,
    extension: RefCell<Option<IDTExtensibility2>>,
    public: RefCell<Option<IDispatch>>,
    lease: RefCell<Option<ModuleLease>>,
    connected: Cell<bool>,
    transition: Cell<bool>,
    disconnecting: Cell<bool>,
    object_setting: Cell<bool>,
    prepared_generation: Cell<Option<u64>>,
    generation: Cell<u64>,
    failure: Cell<Option<i32>>,
    ui_requested: Cell<bool>,
    xml: RefCell<String>,
}
impl Drop for Addin {
    fn drop(&mut self) {
        // Only explicit successful transitions release provider objects or callable code.
        // Failed teardown pins the provider and module until process exit.
        if let Some(object) = self.public.get_mut().take() {
            std::mem::forget(object);
        }
        if let Some(extension) = self.extension.get_mut().take() {
            std::mem::forget(extension);
        }
        if let Some(lease) = self.lease.get_mut().take() {
            std::mem::forget(lease);
        }
    }
}
struct Transition(Rc<Addin>);
impl Drop for Transition {
    fn drop(&mut self) {
        self.0.transition.set(false);
        self.0.disconnecting.set(false);
    }
}
struct Setting(Rc<Addin>);
impl Drop for Setting {
    fn drop(&mut self) {
        self.0.object_setting.set(false);
        self.0.prepared_generation.set(None);
    }
}
pub(crate) struct CallLease {
    _setting: Option<Setting>,
}

pub(crate) struct Office {
    state: Weak<State>,
    handlers: Handlers<Office>,
    nodes: RefCell<HashMap<u64, Rc<Node>>>,
    next: Cell<u64>,
    addins: Cell<Option<u64>>,
    bars: Cell<Option<u64>>,
    closing: Cell<bool>,
    failure: Cell<Option<i32>>,
}
impl Office {
    /// Arm native Object assignment before VARIANT marshalling can call a provider's AddRef.
    pub(crate) fn prepare(
        &self,
        target: &ObjectRef,
        member: &calc_object_model::Member,
    ) -> Result<CallLease> {
        if target.interface != id("COMAddIn")
            || member.name != "Object"
            || member.invoke == InvokeKind::Get
        {
            return Ok(CallLease { _setting: None });
        }
        let data = self
            .node(target)?
            .addin
            .borrow()
            .clone()
            .ok_or(AutoError::Disconnected)?;
        if data.object_setting.replace(true) {
            return failure(E_UNEXPECTED);
        }
        data.prepared_generation.set(Some(data.generation.get()));
        Ok(CallLease {
            _setting: Some(Setting(data)),
        })
    }
    pub fn new(state: Weak<State>) -> Self {
        let mut handlers = Handlers::new();
        for member in ["COMAddIns", "CommandBars"] {
            handlers.on("_Application", member, InvokeKind::Get, Self::application);
        }
        for role in [
            Role::Addins,
            Role::Addin,
            Role::Bars,
            Role::Bar,
            Role::Controls,
            Role::Popup,
            Role::Button,
        ] {
            let interface = role.interface();
            for member in ["Application", "Creator"] {
                handlers.on(interface, member, InvokeKind::Get, Self::common);
            }
            if matches!(
                role,
                Role::Addins | Role::Addin | Role::Bars | Role::Controls
            ) {
                handlers.on(interface, "Parent", InvokeKind::Get, Self::common);
            }
            if matches!(role, Role::Addins | Role::Bars | Role::Controls) {
                // Office describes Item as a method even when clients request PROPERTYGET.
                let descriptor = calc_object_model::Descriptor::get();
                for kind in [InvokeKind::Get, InvokeKind::Method] {
                    if descriptor.member(id(interface), "Item", kind).is_some() {
                        handlers.on(interface, "Item", kind, Self::collection);
                    }
                }
                handlers.on(interface, "Count", InvokeKind::Get, Self::collection);
            }
        }
        handlers.on("COMAddIns", "Update", InvokeKind::Method, Self::collection);
        for member in ["Description", "ProgId", "Guid", "Connect", "Object"] {
            handlers.on("COMAddIn", member, InvokeKind::Get, Self::addin_member);
        }
        for member in ["Connect", "Object"] {
            handlers.on("COMAddIn", member, InvokeKind::Put, Self::addin_member);
        }
        for interface in ["CommandBar", "CommandBarPopup"] {
            handlers.on(interface, "Controls", InvokeKind::Get, Self::control);
        }
        handlers
            .on("CommandBar", "Name", InvokeKind::Get, Self::control)
            .on("CommandBar", "Name", InvokeKind::Put, Self::control);
        handlers.on(
            "CommandBarControls",
            "Add",
            InvokeKind::Method,
            Self::control,
        );
        for interface in ["CommandBarPopup", "_CommandBarButton"] {
            for member in ["Caption", "OnAction"] {
                handlers
                    .on(interface, member, InvokeKind::Get, Self::control)
                    .on(interface, member, InvokeKind::Put, Self::control);
            }
            handlers.on(interface, "Delete", InvokeKind::Method, Self::control);
        }
        Self {
            state,
            handlers,
            nodes: RefCell::default(),
            next: Cell::new(1),
            addins: Cell::new(None),
            bars: Cell::new(None),
            closing: Cell::new(false),
            failure: Cell::new(None),
        }
    }
    fn state(&self) -> Result<Rc<State>> {
        self.state.upgrade().ok_or(AutoError::Disconnected)
    }
    pub fn handles(&self, target: &ObjectRef, call: &Call<'_>) -> bool {
        matches!(target.key, ObjectKey::Host(_))
            || self.handlers.implements(call.interface, call.member)
    }
    pub fn invoke(&self, target: &ObjectRef, call: &Call<'_>) -> Result<AutoValue> {
        if let ObjectKey::Host(_) = target.key {
            self.node(target)?;
        }
        self.handlers.dispatch(self, target, call, "dc-snz.2")
    }
    fn node(&self, target: &ObjectRef) -> Result<Rc<Node>> {
        let ObjectKey::Host(key) = target.key else {
            return failure(E_INVALIDARG);
        };
        let node = self
            .nodes
            .borrow()
            .get(&key)
            .cloned()
            .ok_or(AutoError::Disconnected)?;
        if node.deleted.get() {
            return Err(AutoError::Disconnected);
        }
        Ok(node)
    }
    fn make(&self, parent: ObjectRef, role: Role) -> Result<Rc<Node>> {
        let key = self.next.get();
        self.next.set(
            key.checked_add(1)
                .ok_or(AutoError::Status(E_UNEXPECTED.0))?,
        );
        let node = Rc::new(Node {
            target: ObjectRef::new(id(role.interface()), ObjectKey::Host(key)),
            parent,
            children: RefCell::default(),
            controls: Cell::new(None),
            caption: RefCell::new(text("")),
            action: RefCell::new(text("")),
            deleted: Cell::new(false),
            addin: RefCell::default(),
        });
        self.nodes.borrow_mut().insert(key, node.clone());
        Ok(node)
    }
    fn key(node: &Node) -> u64 {
        let ObjectKey::Host(key) = node.target.key else {
            unreachable!()
        };
        key
    }
    fn children(&self, node: &Node) -> Vec<Rc<Node>> {
        let keys = node.children.borrow().clone();
        let nodes = self.nodes.borrow();
        keys.iter()
            .filter_map(|key| nodes.get(key).filter(|node| !node.deleted.get()).cloned())
            .collect()
    }
    fn addins(&self) -> Result<Rc<Node>> {
        if self.closing.get() {
            return Err(AutoError::Disconnected);
        }
        let key = match self.addins.get() {
            Some(key) => key,
            None => {
                let node = self.make(self.state()?.application(), Role::Addins)?;
                let key = Self::key(&node);
                self.addins.set(Some(key));
                key
            }
        };
        self.nodes
            .borrow()
            .get(&key)
            .cloned()
            .ok_or(AutoError::Disconnected)
    }
    fn application(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue> {
        match call.member.name.as_str() {
            "COMAddIns" => {
                let addins = self.addins()?;
                self.refresh(&addins)?;
                Ok(AutoValue::Object(addins.target.clone()))
            }
            "CommandBars" => {
                if let Some(key) = self.bars.get() {
                    return Ok(AutoValue::Object(self.nodes.borrow()[&key].target.clone()));
                }
                let bars = self.make(self.state()?.application(), Role::Bars)?;
                let bar = self.make(bars.target.clone(), Role::Bar)?;
                *bar.caption.borrow_mut() = text("Worksheet Menu Bar");
                bars.children.borrow_mut().push(Self::key(&bar));
                self.bars.set(Some(Self::key(&bars)));
                Ok(AutoValue::Object(bars.target.clone()))
            }
            _ => Err(AutoError::not_modelled(call.label(), "dc-snz.2")),
        }
    }
    fn common(&self, target: &ObjectRef, call: &Call<'_>) -> Result<AutoValue> {
        match call.member.name.as_str() {
            "Creator" => Ok(AutoValue::I4(0x5843454c)),
            "Application" => Ok(AutoValue::Object(self.state()?.application())),
            "Parent" => Ok(AutoValue::Object(self.node(target)?.parent.clone())),
            _ => Err(AutoError::not_modelled(call.label(), "dc-snz.2")),
        }
    }
    fn collection(&self, target: &ObjectRef, call: &Call<'_>) -> Result<AutoValue> {
        let node = self.node(target)?;
        match call.member.name.as_str() {
            "Count" => Ok(AutoValue::I4(self.children(&node).len() as i32)),
            "Update" => {
                self.refresh(&node)?;
                Ok(AutoValue::Empty)
            }
            "Item" => {
                let children = self.children(&node);
                let found = if let Some(key) = call.arg(0).as_text() {
                    children.into_iter().find(|child| {
                        child
                            .addin
                            .borrow()
                            .as_ref()
                            .map(|a| a.progid.clone())
                            .unwrap_or_else(|| child.caption.borrow().to_string_lossy())
                            .eq_ignore_ascii_case(&key)
                    })
                } else {
                    let n = values::number(call.arg(0))?;
                    if n < 1.0 || n.fract() != 0.0 {
                        None
                    } else {
                        children.get(n as usize - 1).cloned()
                    }
                };
                found
                    .map(|child| AutoValue::Object(child.target.clone()))
                    .ok_or(AutoError::Status(DISP_E_BADINDEX.0))
            }
            _ => Err(AutoError::not_modelled(call.label(), "dc-snz.2")),
        }
    }
    fn controls(&self, node: &Node) -> Result<Rc<Node>> {
        if let Some(key) = node.controls.get() {
            return Ok(self.nodes.borrow()[&key].clone());
        }
        let controls = self.make(node.target.clone(), Role::Controls)?;
        node.controls.set(Some(Self::key(&controls)));
        Ok(controls)
    }
    fn control(&self, target: &ObjectRef, call: &Call<'_>) -> Result<AutoValue> {
        let node = self.node(target)?;
        let get = call.member.invoke == InvokeKind::Get;
        match call.member.name.as_str() {
            "Controls" => Ok(AutoValue::Object(self.controls(&node)?.target.clone())),
            "Name" | "Caption" | "OnAction" => {
                let field = if call.member.name == "OnAction" {
                    &node.action
                } else {
                    &node.caption
                };
                if get {
                    Ok(AutoValue::Text(field.borrow().clone()))
                } else {
                    let AutoValue::Text(value) = call.arg(0) else {
                        return values::mismatch();
                    };
                    *field.borrow_mut() = value.clone();
                    Ok(AutoValue::Empty)
                }
            }
            "Delete" => {
                node.deleted.set(true);
                Ok(AutoValue::Empty)
            }
            "Add" => {
                let role = match values::number(call.arg(0))? as i32 {
                    1 => Role::Button,
                    10 => Role::Popup,
                    _ => return Err(AutoError::not_modelled(call.label(), "dc-snz.2")),
                };
                let missing = |v: &AutoValue| matches!(v, AutoValue::Missing | AutoValue::Empty);
                if (!missing(call.arg(1)) && values::number(call.arg(1))? != 1.0)
                    || !missing(call.arg(2))
                    || (!missing(call.arg(4)) && values::number(call.arg(4))? == 0.0)
                {
                    return Err(AutoError::not_modelled(call.label(), "dc-snz.2"));
                }
                let mut before = node.children.borrow().len();
                if !missing(call.arg(3)) {
                    let n = values::number(call.arg(3))?;
                    let children = self.children(&node);
                    if n < 1.0 || n.fract() != 0.0 || n > (children.len() + 1) as f64 {
                        return failure(E_INVALIDARG);
                    }
                    if let Some(child) = children.get(n as usize - 1) {
                        before = node
                            .children
                            .borrow()
                            .iter()
                            .position(|key| *key == Self::key(child))
                            .unwrap();
                    }
                }
                let child = self.make(node.target.clone(), role)?;
                node.children.borrow_mut().insert(before, Self::key(&child));
                Ok(AutoValue::Object(child.target.clone()))
            }
            _ => Err(AutoError::not_modelled(call.label(), "dc-snz.2")),
        }
    }
    fn addin_member(&self, target: &ObjectRef, call: &Call<'_>) -> Result<AutoValue> {
        let node = self.node(target)?;
        let data = node.addin.borrow().clone().ok_or(AutoError::Disconnected)?;
        let get = call.member.invoke == InvokeKind::Get;
        match call.member.name.as_str() {
            "Description" => Ok(AutoValue::Text(data.description.clone())),
            "ProgId" => Ok(AutoValue::text(&data.progid)),
            "Guid" => {
                let mut buffer = [0u16; 40];
                let n = unsafe { StringFromGUID2(&data.clsid, &mut buffer) };
                if n == 0 {
                    return failure(E_UNEXPECTED);
                }
                Ok(AutoValue::Text(ExcelText::from_utf16_code_units(
                    buffer[..n as usize - 1].to_vec(),
                )))
            }
            "Connect" if get => Ok(AutoValue::Bool(data.connected.get())),
            "Connect" => {
                self.connect(&node, values::number(call.arg(0))? != 0.0, false)?;
                Ok(AutoValue::Empty)
            }
            "Object" if get => {
                let raw = data.public.borrow().as_ref().map(Interface::as_raw);
                match raw {
                    Some(raw) => self.state()?.foreign_result(
                        unsafe { IDispatch::from_raw_borrowed(&raw) }
                            .unwrap()
                            .clone(),
                    ),
                    None => Ok(AutoValue::Empty),
                }
            }
            "Object" => {
                let _setting = if data.prepared_generation.get().is_none() {
                    if data.object_setting.replace(true) {
                        return failure(E_UNEXPECTED);
                    }
                    Some(Setting(data.clone()))
                } else {
                    None
                };
                let generation = data
                    .prepared_generation
                    .get()
                    .unwrap_or_else(|| data.generation.get());
                let next = match call.arg(0) {
                    AutoValue::Empty => None,
                    AutoValue::Foreign(key) => Some(self.state()?.take_foreign(*key)?),
                    _ => return values::mismatch(),
                };
                if generation != data.generation.get() {
                    return failure(E_ABORT);
                }
                if next.is_some()
                    && (!data.connected.get() || data.disconnecting.get() || self.closing.get())
                {
                    return failure(E_UNEXPECTED);
                }
                if next.is_some()
                    && (generation != data.generation.get()
                        || !data.connected.get()
                        || data.disconnecting.get()
                        || self.closing.get())
                {
                    return failure(E_ABORT);
                }
                let old = data.public.replace(next);
                drop(old);
                Ok(AutoValue::Empty)
            }
            _ => Err(AutoError::not_modelled(call.label(), "dc-snz.2")),
        }
    }

    fn extension(data: &Addin) -> Result<IDTExtensibility2> {
        let raw = data
            .extension
            .borrow()
            .as_ref()
            .map(Interface::as_raw)
            .ok_or(AutoError::Disconnected)?;
        Ok(unsafe { IDTExtensibility2::from_raw_borrowed(&raw) }
            .unwrap()
            .clone())
    }
    fn custom(call: impl FnOnce(*mut *mut SAFEARRAY) -> HRESULT) -> Result<()> {
        let mut array = ptr::null_mut();
        let result = call(&mut array);
        drop(values::ArrayOwner(array));
        result.ok().map_err(status)
    }
    fn connect(&self, node: &Rc<Node>, connect: bool, shutdown: bool) -> Result<()> {
        let data = node.addin.borrow().clone().ok_or(AutoError::Disconnected)?;
        if connect && self.closing.get() {
            return Err(AutoError::Disconnected);
        }
        if let Some(code) = data.failure.get() {
            return Err(AutoError::Status(code));
        }
        if data.transition.get() {
            return failure(E_UNEXPECTED);
        }
        if data.connected.get() == connect {
            return Ok(());
        }
        data.generation.set(
            data.generation
                .get()
                .checked_add(1)
                .ok_or(AutoError::Status(E_UNEXPECTED.0))?,
        );
        data.transition.set(true);
        data.disconnecting.set(!connect);
        let _transition = Transition(data.clone());
        if !connect {
            let extension = Self::extension(&data)?;
            let begin = if shutdown {
                Self::custom(|array| unsafe { extension.OnBeginShutdown(array) })
            } else {
                Ok(())
            };
            let disconnected = Self::custom(|array| unsafe {
                extension.OnDisconnection(if shutdown { 0 } else { 1 }, array)
            });
            if let Err(e) = begin.and(disconnected) {
                data.failure.set(Some(e.number()));
                return Err(e);
            }
            data.connected.set(false);
            data.ui_requested.set(false);
            data.xml.borrow_mut().clear();
            let object = data.public.take();
            let owned = data.extension.take();
            let lease = data.lease.take();
            drop(object);
            drop(owned);
            drop(extension);
            drop(lease);
            return Ok(());
        }
        let state = self.state()?;
        if !state.host.contains_loaded_module(&data.module) {
            return failure(E_ACCESSDENIED);
        }
        let lease = ModuleLease::load(&data.module)?;
        let extension = unsafe {
            CoCreateInstance::<_, IDTExtensibility2>(&data.clsid, None, CLSCTX_INPROC_SERVER)
        }
        .map_err(status)?;
        let mut custom = unsafe { SafeArrayCreateVector(VT_VARIANT, 1, 1) };
        let initial = values::ArrayOwner::new(custom)?;
        let startup = VARIANT::from(3i32);
        unsafe { SafeArrayPutElement(custom, &1, (&startup as *const VARIANT).cast()) }
            .map_err(status)?;
        // Publish all live ownership before OnConnection can set COMAddIn.Object or reenter.
        *data.lease.borrow_mut() = Some(lease);
        *data.extension.borrow_mut() = Some(extension);
        data.connected.set(true);
        let generation = data.generation.get();
        let application = state.wrap(&state.application())?;
        let item = state.wrap(&node.target)?;
        let extension = Self::extension(&data)?;
        let _ = initial.take();
        let connected =
            unsafe { extension.OnConnection(application.as_raw(), 0, item.as_raw(), &mut custom) }
                .ok()
                .map_err(status);
        drop(values::ArrayOwner(custom));
        let result = connected
            .and_then(|_| Self::custom(|array| unsafe { extension.OnAddInsUpdate(array) }));
        if let Err(e) = result {
            data.failure.set(Some(e.number()));
            return Err(e);
        }
        if generation != data.generation.get() || !data.connected.get() {
            return failure(E_ABORT);
        }
        Ok(())
    }
    fn refresh(&self, collection: &Rc<Node>) -> Result<()> {
        if self.closing.get() {
            return Err(AutoError::Disconnected);
        }
        let mut key = HKEY::default();
        let code = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                windows::core::w!("Software\\Microsoft\\Office\\Excel\\Addins"),
                None,
                KEY_READ,
                &mut key,
            )
        };
        if code == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        code.ok().map_err(status)?;
        struct Key(HKEY);
        impl Drop for Key {
            fn drop(&mut self) {
                let _ = unsafe { RegCloseKey(self.0) };
            }
        }
        let _key = Key(key);
        for index in 0.. {
            let mut buffer = [0u16; 256];
            let mut length = buffer.len() as u32;
            let result = unsafe {
                RegEnumKeyExW(
                    key,
                    index,
                    Some(PWSTR(buffer.as_mut_ptr())),
                    &mut length,
                    None,
                    None,
                    None,
                    None,
                )
            };
            if result == ERROR_NO_MORE_ITEMS {
                break;
            }
            result.ok().map_err(status)?;
            let progid = String::from_utf16_lossy(&buffer[..length as usize]);
            if self.children(collection).iter().any(|node| {
                node.addin
                    .borrow()
                    .as_ref()
                    .is_some_and(|a| a.progid.eq_ignore_ascii_case(&progid))
            }) {
                continue;
            }
            let Some(clsid) = registry_string(
                HKEY_CURRENT_USER,
                &format!("Software\\Classes\\{progid}\\CLSID"),
                None,
            ) else {
                continue;
            };
            let clsid_wide = wide(&clsid);
            let Ok(id) = (unsafe { CLSIDFromString(PCWSTR(clsid_wide.as_ptr())) }) else {
                continue;
            };
            let Some(module) = registry_string(
                HKEY_CURRENT_USER,
                &format!("Software\\Classes\\CLSID\\{clsid}\\InprocServer32"),
                None,
            ) else {
                continue;
            };
            let module = PathBuf::from(module);
            if !self.state()?.host.contains_loaded_module(&module) {
                continue;
            }
            let description = registry_string(
                HKEY_CURRENT_USER,
                &format!("Software\\Microsoft\\Office\\Excel\\Addins\\{progid}"),
                Some("Description"),
            )
            .unwrap_or_default();
            let node = self.make(collection.target.clone(), Role::Addin)?;
            *node.addin.borrow_mut() = Some(Rc::new(Addin {
                clsid: id,
                progid,
                module,
                description: text(&description),
                extension: RefCell::default(),
                public: RefCell::default(),
                lease: RefCell::default(),
                connected: Cell::new(false),
                transition: Cell::new(false),
                disconnecting: Cell::new(false),
                object_setting: Cell::new(false),
                prepared_generation: Cell::new(None),
                generation: Cell::new(0),
                failure: Cell::new(None),
                ui_requested: Cell::new(false),
                xml: RefCell::default(),
            }));
            collection.children.borrow_mut().push(Self::key(&node));
            self.connect(&node, true, false)?;
        }
        Ok(())
    }
    pub fn close(&self) -> Result<()> {
        self.closing.set(true);
        if let Some(code) = self.failure.get() {
            return Err(AutoError::Status(code));
        }
        let mut first = None;
        if let Some(key) = self.addins.get() {
            let collection = self.nodes.borrow()[&key].clone();
            for node in self.children(&collection) {
                if let Err(error) = self.connect(&node, false, true) {
                    first.get_or_insert(error);
                }
            }
        }
        if let Some(error) = first {
            self.failure.set(Some(error.number()));
            return Err(error);
        }
        // Retained controls remain real COM objects but are disconnected from this bounded tree.
        for node in self.nodes.borrow().values() {
            node.deleted.set(true);
        }
        Ok(())
    }
    pub fn ribbons(&self) -> Result<Vec<(String, String)>> {
        if self.closing.get() {
            return Err(AutoError::Disconnected);
        }
        let Some(key) = self.addins.get() else {
            return Ok(Vec::new());
        };
        let collection = self.nodes.borrow()[&key].clone();
        let snapshot = self.children(&collection);
        for node in &snapshot {
            let Some(data) = node.addin.borrow().clone() else {
                continue;
            };
            if !data.connected.get() || data.ui_requested.get() {
                continue;
            }
            let generation = data.generation.get();
            let _lease = ModuleLease::retain(&data.module)?;
            let extension = Self::extension(&data)?;
            if generation != data.generation.get() || !data.connected.get() {
                return failure(E_ABORT);
            }
            let ribbon = extension.cast::<IRibbonExtensibility>();
            if generation != data.generation.get() || !data.connected.get() {
                return failure(E_ABORT);
            }
            let ribbon = match ribbon {
                Ok(ribbon) => ribbon,
                Err(error) if error.code() == E_NOINTERFACE => {
                    data.ui_requested.set(true);
                    continue;
                }
                Err(error) => return Err(status(error)),
            };
            let ribbon_id = BSTR::from("Microsoft.Excel.Workbook");
            let mut xml = ptr::null();
            let result = unsafe { ribbon.GetCustomUI(ribbon_id.as_ptr(), &mut xml) }
                .ok()
                .map_err(status);
            let xml = unsafe { BSTR::from_raw(xml) };
            drop(ribbon);
            if result.is_ok() && (generation != data.generation.get() || !data.connected.get()) {
                return failure(E_ABORT);
            }
            result?;
            *data.xml.borrow_mut() = xml.to_string();
            data.ui_requested.set(true);
        }
        // GetCustomUI may grow the collection; include only connected, completed providers.
        Ok(self
            .children(&collection)
            .iter()
            .filter_map(|node| {
                let data = node.addin.borrow().clone()?;
                if data.connected.get() && data.ui_requested.get() && !data.xml.borrow().is_empty()
                {
                    Some((data.progid.clone(), data.xml.borrow().clone()))
                } else {
                    None
                }
            })
            .collect())
    }
}

pub(crate) fn registry_string(root: HKEY, key: &str, name: Option<&str>) -> Option<String> {
    let key = wide(key);
    let name = name.map(wide);
    let name = name.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr()));
    let mut bytes = 0;
    if unsafe {
        RegGetValueW(
            root,
            PCWSTR(key.as_ptr()),
            name,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut bytes),
        )
    } != ERROR_SUCCESS
        || bytes < 2
        || bytes > 65536
        || bytes % 2 != 0
    {
        return None;
    }
    let mut buffer = vec![0u16; bytes as usize / 2];
    if unsafe {
        RegGetValueW(
            root,
            PCWSTR(key.as_ptr()),
            name,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    } != ERROR_SUCCESS
        || bytes % 2 != 0
        || bytes as usize > buffer.len() * 2
    {
        return None;
    }
    let buffer = &buffer[..bytes as usize / 2];
    let end = buffer.iter().position(|c| *c == 0)?;
    if buffer[end..].iter().any(|c| *c != 0) {
        return None;
    }
    String::from_utf16(&buffer[..end]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interfaces::{IDTExtensibility2_Impl, IRibbonExtensibility_Impl};
    use windows::core::{IUnknownImpl, implement};

    #[implement(IDTExtensibility2, IRibbonExtensibility, Agile = false)]
    struct Provider {
        state: Weak<State>,
        node: Rc<Node>,
        mode: i32,
        live: Rc<Cell<u32>>,
    }
    impl Drop for Provider {
        fn drop(&mut self) {
            self.live.set(self.live.get() - 1);
        }
    }
    impl IDTExtensibility2_Impl for Provider_Impl {
        unsafe fn OnConnection(
            &self,
            _: *mut std::ffi::c_void,
            _: i32,
            _: *mut std::ffi::c_void,
            _: *mut *mut SAFEARRAY,
        ) -> HRESULT {
            if self.mode != 5 {
                return S_OK;
            }
            let state = self.state.upgrade().unwrap();
            let item = state.wrap(&self.node.target).unwrap();
            let provider: IDTExtensibility2 = self.to_interface();
            let provider: IDispatch = provider.cast().unwrap();
            let mut arg = VARIANT::from(provider);
            let mut named = -3;
            let params = DISPPARAMS {
                rgvarg: &mut arg,
                rgdispidNamedArgs: &mut named,
                cArgs: 1,
                cNamedArgs: 1,
            };
            unsafe {
                item.Invoke(
                    7,
                    &GUID::zeroed(),
                    0,
                    DISPATCH_PROPERTYPUT,
                    &params,
                    None,
                    None,
                    None,
                )
            }
            .map_or_else(|e| e.code(), |_| S_OK)
        }
        unsafe fn OnDisconnection(&self, _: i32, _: *mut *mut SAFEARRAY) -> HRESULT {
            if self.mode == 4 {
                let state = self.state.upgrade().unwrap();
                let collection = state.office.node(&self.node.parent).unwrap();
                if state.office.refresh(&collection) != Err(AutoError::Disconnected) {
                    return E_FAIL;
                }
            }
            if self.mode == 6 { E_FAIL } else { S_OK }
        }
        unsafe fn OnAddInsUpdate(&self, _: *mut *mut SAFEARRAY) -> HRESULT {
            S_OK
        }
        unsafe fn OnStartupComplete(&self, _: *mut *mut SAFEARRAY) -> HRESULT {
            S_OK
        }
        unsafe fn OnBeginShutdown(&self, _: *mut *mut SAFEARRAY) -> HRESULT {
            S_OK
        }
    }
    impl IRibbonExtensibility_Impl for Provider_Impl {
        unsafe fn GetCustomUI(&self, _: *const u16, result: *mut *const u16) -> HRESULT {
            if result.is_null() {
                return E_POINTER;
            }
            unsafe {
                *result = ptr::null();
            }
            let state = self.state.upgrade().unwrap();
            if self.mode == 1 {
                let collection = state.office.node(&self.node.parent).unwrap();
                for _ in 0..32 {
                    let node = state
                        .office
                        .make(collection.target.clone(), Role::Addin)
                        .unwrap();
                    *node.addin.borrow_mut() = Some(Rc::new(data(PathBuf::new())));
                    collection.children.borrow_mut().push(Office::key(&node));
                }
            }
            if self.mode == 2 {
                if let Err(error) = state.office.connect(&self.node, false, false) {
                    return crate::runtime::error(&error, ptr::null_mut());
                }
            }
            unsafe {
                *result = BSTR::from("<customUI/>").into_raw();
            }
            S_OK
        }
    }
    impl IDispatch_Impl for Provider_Impl {
        fn GetTypeInfoCount(&self) -> windows::core::Result<u32> {
            Ok(0)
        }
        fn GetTypeInfo(&self, _: u32, _: u32) -> windows::core::Result<ITypeInfo> {
            Err(windows::core::Error::from_hresult(E_NOTIMPL))
        }
        fn GetIDsOfNames(
            &self,
            _: *const GUID,
            _: *const PCWSTR,
            _: u32,
            _: u32,
            _: *mut i32,
        ) -> windows::core::Result<()> {
            Err(windows::core::Error::from_hresult(E_NOTIMPL))
        }
        fn Invoke(
            &self,
            _: i32,
            _: *const GUID,
            _: u32,
            _: DISPATCH_FLAGS,
            _: *const DISPPARAMS,
            _: *mut VARIANT,
            _: *mut EXCEPINFO,
            _: *mut u32,
        ) -> windows::core::Result<()> {
            Err(windows::core::Error::from_hresult(E_NOTIMPL))
        }
    }
    fn data(module: PathBuf) -> Addin {
        Addin {
            clsid: GUID::zeroed(),
            progid: "fixture".into(),
            module,
            description: text(""),
            extension: RefCell::default(),
            public: RefCell::default(),
            lease: RefCell::default(),
            connected: Cell::new(false),
            transition: Cell::new(false),
            disconnecting: Cell::new(false),
            object_setting: Cell::new(false),
            prepared_generation: Cell::new(None),
            generation: Cell::new(0),
            failure: Cell::new(None),
            ui_requested: Cell::new(false),
            xml: RefCell::default(),
        }
    }

    type Query = unsafe extern "system" fn(
        *mut std::ffi::c_void,
        *const GUID,
        *mut *mut std::ffi::c_void,
    ) -> HRESULT;
    struct QueryHook {
        original: Query,
        state: Weak<State>,
        node: Rc<Node>,
    }
    thread_local! { static QUERY_HOOK: RefCell<Option<QueryHook>> = const { RefCell::new(None) }; }
    unsafe extern "system" fn disconnect_in_query(
        this: *mut std::ffi::c_void,
        iid: *const GUID,
        out: *mut *mut std::ffi::c_void,
    ) -> HRESULT {
        let (original, state, node) = QUERY_HOOK.with(|hook| {
            let hook = hook.borrow();
            let hook = hook.as_ref().unwrap();
            (hook.original, hook.state.clone(), hook.node.clone())
        });
        if !iid.is_null() && unsafe { *iid } == IRibbonExtensibility::IID {
            let state = state.upgrade().unwrap();
            if let Err(error) = state.office.connect(&node, false, false) {
                return crate::runtime::error(&error, ptr::null_mut());
            }
        }
        unsafe { original(this, iid, out) }
    }

    /// Native-only: real IRibbonExtensibility/IDTExtensibility2 callbacks grow, disconnect and set Object reentrantly.
    #[test]
    fn ribbon_callbacks_retain_providers_and_reject_stale_connections() {
        let _apartment = crate::tests::Apartment::new();
        for mode in 1..=5 {
            let projection =
                crate::Projection::new(1, Rc::new(calc_xll_host::XllHost::new(1))).unwrap();
            let office = &projection.state.office;
            let collection = office.addins().unwrap();
            let node = office.make(collection.target.clone(), Role::Addin).unwrap();
            collection.children.borrow_mut().push(Office::key(&node));
            let live = Rc::new(Cell::new(1));
            let provider: IDTExtensibility2 = Provider {
                state: Rc::downgrade(&projection.state),
                node: node.clone(),
                mode,
                live: live.clone(),
            }
            .into();
            // This controlled provider substitutes only its own QueryInterface slot. The
            // copied windows-rs vtable retains the concrete object's correct identity offsets.
            // Its owned copy outlives every provider reference; production vtables stay static.
            let mut query_table = None;
            if mode == 3 {
                let mut table = Box::new(unsafe { ptr::read(provider.vtable()) });
                QUERY_HOOK.with(|hook| {
                    *hook.borrow_mut() = Some(QueryHook {
                        original: table.base__.base__.QueryInterface,
                        state: Rc::downgrade(&projection.state),
                        node: node.clone(),
                    })
                });
                table.base__.base__.QueryInterface = disconnect_in_query;
                unsafe {
                    (provider.as_raw() as *mut *const crate::interfaces::IDTExtensibility2_Vtbl)
                        .write(&*table);
                }
                query_table = Some(table);
            }
            let data = Rc::new(data(std::env::current_exe().unwrap()));
            data.connected.set(true);
            data.generation.set(1);
            *data.extension.borrow_mut() = Some(provider);
            *node.addin.borrow_mut() = Some(data.clone());
            let result = if mode == 4 {
                office.close().map(|_| Vec::new())
            } else if mode == 5 {
                data.transition.set(true);
                let result = Office::custom(|array| unsafe {
                    Office::extension(&data).unwrap().OnConnection(
                        ptr::null_mut(),
                        0,
                        ptr::null_mut(),
                        array,
                    )
                });
                data.transition.set(false);
                assert!(data.public.borrow().is_some());
                let item = projection.state.wrap(&node.target).unwrap();
                let mut output = VARIANT::default();
                let params = DISPPARAMS::default();
                unsafe {
                    item.Invoke(
                        7,
                        &GUID::zeroed(),
                        0,
                        DISPATCH_PROPERTYGET,
                        &params,
                        Some(&mut output),
                        None,
                        None,
                    )
                }
                .unwrap();
                assert!(IDispatch::try_from(&output).is_ok());
                drop(output);
                result.map(|_| Vec::new())
            } else {
                office.ribbons()
            };
            if mode == 2 || mode == 3 {
                assert_eq!(result.unwrap_err(), AutoError::Status(E_ABORT.0));
                assert!(!data.ui_requested.get());
                assert!(data.xml.borrow().is_empty());
                assert_eq!(live.get(), 0);
            } else {
                result.unwrap();
                if mode == 1 {
                    assert!(data.ui_requested.get());
                    assert_eq!(*data.xml.borrow(), "<customUI/>");
                    assert_eq!(office.children(&collection).len(), 33);
                }
            }
            office.close().unwrap();
            assert_eq!(live.get(), 0);
            if mode == 3 {
                QUERY_HOOK.with(|hook| {
                    hook.take();
                });
            }
            drop(query_table);
        }
    }

    /// Native-only: a failed foreign disconnection is permanent and keeps its provider alive.
    #[test]
    fn failed_disconnection_pins_native_provider_and_repeats_the_failure() {
        let _apartment = crate::tests::Apartment::new();
        let projection =
            crate::Projection::new(1, Rc::new(calc_xll_host::XllHost::new(1))).unwrap();
        let office = &projection.state.office;
        let collection = office.addins().unwrap();
        let node = office.make(collection.target.clone(), Role::Addin).unwrap();
        collection.children.borrow_mut().push(Office::key(&node));
        let live = Rc::new(Cell::new(1));
        let provider: IDTExtensibility2 = Provider {
            state: Rc::downgrade(&projection.state),
            node: node.clone(),
            mode: 6,
            live: live.clone(),
        }
        .into();
        let data = Rc::new(data(std::env::current_exe().unwrap()));
        data.connected.set(true);
        *data.extension.borrow_mut() = Some(provider);
        *node.addin.borrow_mut() = Some(data.clone());
        assert_eq!(office.close().unwrap_err(), AutoError::Status(E_FAIL.0));
        assert_eq!(office.close().unwrap_err(), AutoError::Status(E_FAIL.0));
        assert_eq!(live.get(), 1);
        assert!(data.connected.get() && data.extension.borrow().is_some());
        // Only the controlled fixture is reclaimed after proving the production failure state.
        let extension = data.extension.take();
        drop(extension);
        data.connected.set(false);
        assert_eq!(live.get(), 0);
    }
}
