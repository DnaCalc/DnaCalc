use calc_fml::CallableTable as UserFunctions;
use calc_func::{UserFunction, UserFunctionContext};
use calc_value::{CalcValue, SheetId};
use calc_xll_abi::*;
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet, HashMap},
    ffi::{CString, c_void},
    fmt,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryExW(path: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
    pub(crate) fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    fn GetModuleHandleExW(flags: u32, name: *const u16, module: *mut *mut c_void) -> i32;
    fn GetModuleFileNameW(module: *mut c_void, path: *mut u16, length: u32) -> u32;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
    fn FindResourceW(module: *mut c_void, name: *const u16, kind: *const u16) -> *mut c_void;
    fn SizeofResource(module: *mut c_void, resource: *mut c_void) -> u32;
    fn LoadResource(module: *mut c_void, resource: *mut c_void) -> *mut c_void;
    fn LockResource(resource: *mut c_void) -> *const u8;
    pub(crate) fn GetCurrentThreadId() -> u32;
    fn GetCurrentProcessId() -> u32;
    pub(crate) fn GetCurrentThreadStackLimits(low: *mut usize, high: *mut usize);
}
#[repr(C)]
struct WindowClass {
    style: u32,
    procedure: usize,
    class_extra: i32,
    window_extra: i32,
    instance: *mut c_void,
    icon: *mut c_void,
    cursor: *mut c_void,
    background: *mut c_void,
    menu: *const u16,
    name: *const u16,
}
#[link(name = "user32")]
unsafe extern "system" {
    fn GetClassInfoW(instance: *mut c_void, name: *const u16, class: *mut WindowClass) -> i32;
    fn UnregisterClassW(name: *const u16, instance: *mut c_void) -> i32;
    fn EnumWindows(
        callback: unsafe extern "system" fn(*mut c_void, isize) -> i32,
        data: isize,
    ) -> i32;
    fn EnumChildWindows(
        parent: *mut c_void,
        callback: unsafe extern "system" fn(*mut c_void, isize) -> i32,
        data: isize,
    ) -> i32;
    fn GetWindowThreadProcessId(window: *mut c_void, process: *mut u32) -> u32;
    fn GetWindowLongPtrW(window: *mut c_void, index: i32) -> isize;
    fn GetClassNameW(window: *mut c_void, name: *mut u16, length: i32) -> i32;
    fn DestroyWindow(window: *mut c_void) -> i32;
}
#[link(name = "dbghelp")]
unsafe extern "system" {
    fn ImageDirectoryEntryToData(
        base: *mut c_void,
        mapped_as_image: u8,
        directory: u16,
        size: *mut u32,
    ) -> *mut c_void;
}

pub(crate) fn wide(path: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.encode_wide().chain(Some(0)).collect()
}

pub(crate) struct Module {
    pub handle: *mut c_void,
    pub path: PathBuf,
    measured_xloil: bool,
    qualified_native_profile: bool,
    image_size: usize,
    delay_cleanup: Cell<bool>,
}

// Compatibility handling for the exact original PyPI xlOil 0.22.1 loader measured
// in X3 (both wheel profiles contain this file). Its still-loaded core crashes
// during plugin reinitialization after teardown. Do not extrapolate to other
// releases or pretend that a refused reload is a compatibility pass (dc-snz.21).
const XLOIL_0221: [u8; 32] = [
    0xf0, 0x8e, 0x0b, 0x44, 0xee, 0x61, 0x14, 0x7b, 0x25, 0xba, 0x15, 0x3b, 0x41, 0x26, 0x43, 0x54,
    0x72, 0xd3, 0xcd, 0x62, 0x52, 0xf1, 0xa2, 0x6a, 0x11, 0x77, 0x18, 0x0c, 0x6d, 0xc7, 0xa8, 0xb0,
];
static XLOIL_0221_REMOVED: AtomicBool = AtomicBool::new(false);
static XLOIL_0221_CLEANED: AtomicBool = AtomicBool::new(false);
static XLOIL_DELAY_RELEASES: AtomicUsize = AtomicUsize::new(0);
const XLOIL_CORE_0221: [u8; 32] = [
    0xb5, 0x3a, 0x0c, 0x36, 0x75, 0xc4, 0x9e, 0x5b, 0xa6, 0x13, 0x9f, 0xdc, 0x61, 0x1d, 0xc4, 0xbf,
    0x58, 0x85, 0xb0, 0x35, 0xe6, 0xeb, 0xe6, 0xee, 0xa5, 0x5b, 0x10, 0x47, 0x66, 0xa2, 0x4a, 0xbf,
];
impl fmt::Debug for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Module").field(&self.path).finish()
    }
}
impl Module {
    fn measured_native_profile(path: &Path) -> bool {
        // Python and arbitrary plugins may own additional threads, extension
        // modules or native UI. They retain the existing process-lifetime refusal.
        if path.file_name().is_none_or(|name| name != "xlOil.xll")
            || std::env::var_os("XLOIL_SETTINGS_DIR").is_some_and(|value| !value.is_empty())
        {
            return false;
        }
        let Some(appdata) = std::env::var_os("APPDATA") else {
            return false;
        };
        if !matches!(
            PathBuf::from(appdata).join("xlOil/xlOil.ini").try_exists(),
            Ok(false)
        ) {
            return false;
        }
        [
            (
                "xlOil.ini",
                "fc0efa4fcd582988425a94685249c28e217116f6bb321eef5e8e4d0b901b04c2",
            ),
            (
                "xlOil_SQL.dll",
                "3294a2525cfb1039bffb308261d2a3620b4f5ac63545dfcfd26eb0df055aff5d",
            ),
            (
                "xlOil_Utils.dll",
                "74a29dffb30d3827c5f2a8d9f2b1581a1c7bd6ec33ac44a4490ab5e0300f89ff",
            ),
        ]
        .iter()
        .all(|(name, expected)| {
            std::fs::read(path.with_file_name(name))
                .is_ok_and(|bytes| format!("{:x}", Sha256::digest(bytes)) == *expected)
        })
    }
    fn release_native_window_classes(core: *mut c_void) -> bool {
        // Successful xlAutoClose destroys its message window but leaves the
        // xlOilHidden class registered against this host EXE. The log sink also
        // leaves a window whose procedure lives in the core. Remove only that
        // exact owner-thread UI while its object and code are still alive.
        struct Windows {
            core: *mut c_void,
            process: u32,
            thread: u32,
            log: Vec<*mut c_void>,
            valid: bool,
        }
        unsafe extern "system" fn visit(window: *mut c_void, data: isize) -> i32 {
            let state = unsafe { &mut *(data as *mut Windows) };
            let mut process = 0;
            let thread = unsafe { GetWindowThreadProcessId(window, &mut process) };
            if process != state.process {
                return 1;
            }
            let procedure = unsafe { GetWindowLongPtrW(window, -4) };
            let mut owner = std::ptr::null_mut();
            if procedure == 0
                || unsafe { GetModuleHandleExW(6, procedure as *const u16, &mut owner) } == 0
            {
                state.valid = false;
                return 1;
            }
            if owner != state.core {
                return 1;
            }
            let mut name = [0u16; 128];
            let len = unsafe { GetClassNameW(window, name.as_mut_ptr(), name.len() as i32) };
            if thread == state.thread
                && len > 0
                && String::from_utf16_lossy(&name[..len as usize]) == "xlOil_Log"
                && unsafe { GetWindowLongPtrW(window, -6) } as *mut c_void == state.core
            {
                state.log.push(window);
            } else {
                state.valid = false;
            }
            1
        }
        unsafe extern "system" fn visit_root(window: *mut c_void, data: isize) -> i32 {
            unsafe {
                visit(window, data);
            }
            let state = unsafe { &*(data as *const Windows) };
            let mut process = 0;
            unsafe {
                GetWindowThreadProcessId(window, &mut process);
            }
            if process == state.process {
                unsafe {
                    EnumChildWindows(window, visit, data);
                }
            }
            1
        }
        let mut windows = Windows {
            core,
            process: unsafe { GetCurrentProcessId() },
            thread: unsafe { GetCurrentThreadId() },
            log: Vec::new(),
            valid: true,
        };
        if unsafe { EnumWindows(visit_root, &mut windows as *mut Windows as isize) } == 0
            || !windows.valid
        {
            return false;
        }
        // UnregisterClass refuses if any window still uses the class. Verify the
        // procedure's module before touching either process-local registration.
        // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-unregisterclassw
        let mut classes = Vec::new();
        for (name, instance) in [
            ("xlOilHidden", unsafe { GetModuleHandleW(std::ptr::null()) }),
            ("xlOil_Log", core),
        ] {
            let name = wide(std::ffi::OsStr::new(name));
            // ubs:ignore -- WindowClass contains only integers/raw pointers; GetClassInfoW must succeed before fields are read.
            let mut class: WindowClass = unsafe { std::mem::zeroed() };
            if unsafe { GetClassInfoW(instance, name.as_ptr(), &mut class) } == 0 {
                return false;
            }
            let mut owner = std::ptr::null_mut();
            if class.instance != instance
                || unsafe { GetModuleHandleExW(6, class.procedure as *const u16, &mut owner) } == 0
                || owner != core
            {
                return false;
            }
            classes.push((name, instance));
        }
        // The message class is first. Its removal must succeed before changing
        // shared log UI: another live XLL context still has a message window and
        // makes UnregisterClass fail without changing that window or its class.
        for (index, (name, instance)) in classes.into_iter().enumerate() {
            if index == 1 {
                for window in &windows.log {
                    if unsafe { DestroyWindow(*window) } == 0 {
                        return false;
                    }
                }
            }
            if unsafe { UnregisterClassW(name.as_ptr(), instance) } == 0 {
                return false;
            }
        }
        true
    }
    fn delayed_core(&self) -> Option<*mut c_void> {
        if !self.measured_xloil
            || !self.delay_cleanup.get()
            || self.image_size == 0
            || !self.qualified_native_profile
            || !Self::measured_native_profile(&self.path)
        {
            return None;
        }
        // This exact loader uses the documented PE32+ delay-load helper once for
        // xlOil.dll and never releases that reference. Its unload table is absent.
        // After successful teardown and final XLL unload, that orphaned reference
        // prevents the core's static COM connector from being constructed again.
        // https://learn.microsoft.com/windows/win32/debug/pe-format
        static DBGHELP: Mutex<()> = Mutex::new(());
        let _serial = DBGHELP.lock().ok()?;
        let mut size = 0;
        let directory = unsafe { ImageDirectoryEntryToData(self.handle, 1, 13, &mut size) };
        let offset = (directory as usize).checked_sub(self.handle as usize)?;
        if directory.is_null()
            || size == 0
            || size % 32 != 0
            || offset.checked_add(size as usize)? > self.image_size
        {
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(directory.cast::<u8>(), size as usize) };
        for descriptor in bytes.chunks_exact(32) {
            let word =
                |at: usize| u32::from_le_bytes(descriptor[at..at + 4].try_into().unwrap()) as usize;
            if word(0) == 0 {
                break;
            }
            if word(0) != 1 {
                return None;
            }
            let (name, slot) = (word(4), word(8));
            if name.checked_add(10)? > self.image_size
                || slot.checked_add(std::mem::size_of::<usize>())? > self.image_size
            {
                return None;
            }
            let base = self.handle.cast::<u8>();
            let name = unsafe { std::slice::from_raw_parts(base.add(name), 10) };
            if !name.eq_ignore_ascii_case(b"xlOil.dll\0") {
                continue;
            }
            let core = unsafe { base.add(slot).cast::<*mut c_void>().read_unaligned() };
            if core.is_null() {
                return None;
            }
            let mut path = [0u16; 32768];
            let len =
                unsafe { GetModuleFileNameW(core, path.as_mut_ptr(), path.len() as u32) } as usize;
            if len == 0 || len >= path.len() {
                return None;
            }
            use std::os::windows::ffi::OsStringExt;
            let path = PathBuf::from(std::ffi::OsString::from_wide(&path[..len]));
            if std::fs::canonicalize(&path).ok()?
                != std::fs::canonicalize(self.path.with_file_name("xlOil.dll")).ok()?
                || Sha256::digest(std::fs::read(path).ok()?).as_slice() != XLOIL_CORE_0221
            {
                return None;
            }
            return Some(core);
        }
        None
    }
    fn resource(&self, name: &str, kind: &str) -> Option<&[u8]> {
        unsafe {
            let resource = FindResourceW(
                self.handle,
                wide(std::ffi::OsStr::new(name)).as_ptr(),
                wide(std::ffi::OsStr::new(kind)).as_ptr(),
            );
            if resource.is_null() {
                return None;
            }
            let size = SizeofResource(self.handle, resource) as usize;
            let loaded = LoadResource(self.handle, resource);
            if loaded.is_null() {
                return None;
            }
            let bytes = LockResource(loaded);
            if bytes.is_null() {
                return None;
            }
            Some(std::slice::from_raw_parts(bytes, size))
        }
    }
    fn reserve_runtime_initialization(&self) -> Result<(), String> {
        if self.measured_xloil && XLOIL_0221_REMOVED.load(Ordering::Acquire) {
            return Err("dc-snz.21: reopening the measured xlOil 0.22.1 runtime after teardown is not modelled; restart the host".into());
        }
        let Some(payload) = self.resource("__MAIN__", "NATIVE_ASSEMBLY") else {
            return Ok(());
        };
        let digest: [u8; 32] = Sha256::digest(payload).into();
        let runtime = self
            .resource("__MAIN___ORIGINAL_DLL_FILE_NAME", "PROPERTY")
            .map(|bytes| {
                let units = bytes
                    .chunks_exact(2)
                    .map(|b| u16::from_le_bytes([b[0], b[1]]))
                    .take_while(|u| *u != 0)
                    .collect::<Vec<_>>();
                let path = self
                    .path
                    .parent()
                    .unwrap()
                    .join(String::from_utf16_lossy(&units));
                std::fs::canonicalize(&path).unwrap_or(path)
            });
        // Runtime initialization belongs to the process, not an XllHost instance.
        // Retain identities before entry, including a failed partial initialization.
        // A payload hash also catches renamed XLL copies and extracted runtimes.
        static ATTEMPTED: OnceLock<Mutex<(BTreeSet<[u8; 32]>, BTreeSet<PathBuf>)>> =
            OnceLock::new();
        let mut attempted = ATTEMPTED.get_or_init(Default::default).lock().unwrap();
        if attempted.0.contains(&digest)
            || runtime.as_ref().is_some_and(|p| attempted.1.contains(p))
        {
            return Err("dc-snz.18: reinitializing a packed Native AOT runtime already attempted in this process is not modelled; restart the host".into());
        }
        attempted.0.insert(digest);
        if let Some(path) = runtime {
            attempted.1.insert(path);
        }
        Ok(())
    }
    fn load(path: &Path) -> Result<Rc<Self>, String> {
        let path = std::fs::canonicalize(path).map_err(|e| format!("{}: {e}", path.display()))?;
        // Framework add-ins use System.IO.Path, which rejects the extended Win32 prefix.
        let text = path.to_string_lossy();
        let path = if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
            PathBuf::from(format!(r"\\{unc}"))
        } else if let Some(local) = text.strip_prefix(r"\\?\") {
            PathBuf::from(local)
        } else {
            path
        };
        let bytes = std::fs::read(&path);
        let measured_xloil = bytes
            .as_ref()
            .is_ok_and(|bytes| Sha256::digest(bytes).as_slice() == XLOIL_0221);
        // Qualification is fixed before native initialization: a sibling ini
        // does not describe a runtime loaded through an override/user setting or
        // a previously resident core. Rechecking at release can only withdraw it.
        let qualified_native_profile = measured_xloil
            && Self::measured_native_profile(&path)
            && unsafe { GetModuleHandleW(wide(std::ffi::OsStr::new("xlOil.dll")).as_ptr()) }
                .is_null();
        // Reinitialization is permitted only after the old core has actually
        // left the process. Retained registrations and Python's extension lease
        // can keep it mapped, in which case the existing refusal still applies.
        if measured_xloil
            && XLOIL_0221_REMOVED.load(Ordering::Acquire)
            && XLOIL_0221_CLEANED.load(Ordering::Acquire)
            && unsafe { GetModuleHandleW(wide(std::ffi::OsStr::new("xlOil.dll")).as_ptr()) }
                .is_null()
        {
            XLOIL_0221_REMOVED.store(false, Ordering::Release);
            XLOIL_0221_CLEANED.store(false, Ordering::Release);
        }
        let image_size = bytes
            .as_ref()
            .ok()
            .and_then(|bytes| {
                let pe = u32::from_le_bytes(bytes.get(60..64)?.try_into().ok()?) as usize;
                if bytes.get(pe..pe + 4)? != b"PE\0\0" {
                    return None;
                }
                let at = pe.checked_add(24 + 56)?;
                Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize)
            })
            .unwrap_or(0);
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
        Ok(Rc::new(Self {
            handle,
            path,
            measured_xloil,
            qualified_native_profile,
            image_size,
            delay_cleanup: Cell::new(false),
        }))
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
            measured_xloil: false,
            qualified_native_profile: false,
            image_size: 0,
            delay_cleanup: Cell::new(false),
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
        let delayed_core = self.delayed_core();
        unsafe {
            let unloaded = FreeLibrary(self.handle) != 0
                && GetModuleHandleW(wide(self.path.as_os_str()).as_ptr()).is_null();
            if unloaded && let Some(core) = delayed_core {
                // Balance only the measured loader's own delay-load reference,
                // once its image is gone. Never decrement another live XLL's
                // reference or remove a retained callable/async code lease.
                if Self::release_native_window_classes(core) && FreeLibrary(core) != 0 {
                    XLOIL_DELAY_RELEASES.fetch_add(1, Ordering::AcqRel);
                    if GetModuleHandleW(
                        wide(self.path.with_file_name("xlOil.dll").as_os_str()).as_ptr(),
                    )
                    .is_null()
                    {
                        XLOIL_0221_CLEANED.store(true, Ordering::Release);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod reload_tests {
    use super::*;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn RegisterClassW(class: *const WindowClass) -> u16;
        fn CreateWindowExW(
            ex: u32,
            class: *const u16,
            title: *const u16,
            style: u32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            parent: *mut c_void,
            menu: *mut c_void,
            instance: *mut c_void,
            parameter: *mut c_void,
        ) -> *mut c_void;
        fn DefWindowProcW(window: *mut c_void, message: u32, wparam: usize, lparam: isize)
        -> isize;
        fn IsWindow(window: *mut c_void) -> i32;
    }
    unsafe extern "system" fn procedure(
        window: *mut c_void,
        message: u32,
        wparam: usize,
        lparam: isize,
    ) -> isize {
        unsafe { DefWindowProcW(window, message, wparam, lparam) }
    }
    /// Native-only: exercises process-local Win32 window/class ownership without Excel.
    #[test]
    fn native_reload_ui_cleanup_preserves_live_or_unknown_contexts() {
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let create = |name: &str, message_only: bool| {
            let name = wide(std::ffi::OsStr::new(name));
            // ubs:ignore -- WindowClass contains only integers/raw pointers; GetClassInfoW must succeed before fields are read.
            let mut class: WindowClass = unsafe { std::mem::zeroed() };
            class.instance = instance;
            class.name = name.as_ptr();
            class.procedure = procedure as usize;
            assert_ne!(unsafe { RegisterClassW(&class) }, 0);
            let window = unsafe {
                CreateWindowExW(
                    0,
                    name.as_ptr(),
                    name.as_ptr(),
                    0,
                    0,
                    0,
                    1,
                    1,
                    if message_only {
                        -3isize as *mut c_void
                    } else {
                        std::ptr::null_mut()
                    },
                    std::ptr::null_mut(),
                    instance,
                    std::ptr::null_mut(),
                )
            };
            assert!(!window.is_null());
            window
        };
        let hidden = create("xlOilHidden", true);
        let log = create("xlOil_Log", false);
        assert!(!Module::release_native_window_classes(instance));
        assert_ne!(
            unsafe { IsWindow(log) },
            0,
            "live context lost shared log UI"
        );
        assert_ne!(unsafe { DestroyWindow(hidden) }, 0);
        let unknown = create("DnaCalc.Reload.Unknown", false);
        assert!(!Module::release_native_window_classes(instance));
        assert_ne!(
            unsafe { IsWindow(log) },
            0,
            "unknown core UI allowed partial teardown"
        );
        assert_ne!(unsafe { DestroyWindow(unknown) }, 0);
        assert_ne!(
            unsafe {
                UnregisterClassW(
                    wide(std::ffi::OsStr::new("DnaCalc.Reload.Unknown")).as_ptr(),
                    instance,
                )
            },
            0
        );
        assert!(Module::release_native_window_classes(instance));
        assert_eq!(unsafe { IsWindow(log) }, 0);
        for name in ["xlOilHidden", "xlOil_Log"] {
            // ubs:ignore -- WindowClass contains only integers/raw pointers; GetClassInfoW must succeed before fields are read.
            let mut class: WindowClass = unsafe { std::mem::zeroed() };
            assert_eq!(
                unsafe {
                    GetClassInfoW(
                        instance,
                        wide(std::ffi::OsStr::new(name)).as_ptr(),
                        &mut class,
                    )
                },
                0
            );
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
    pub sheet_names: BTreeMap<SheetId, String>,
    pub active_sheet: Option<SheetId>,
    pub events: BTreeMap<(usize, i32), usize>,
    pub allocations: HashMap<usize, CallbackAllocation>,
    pub diagnostics: Vec<String>,
    pub hwnd: usize,
    pub thread: u32,
    pub closed: bool,
    pub status_message: Option<String>,
    pub workers: Option<Rc<crate::worker::Pool>>,
    pub pending_async: BTreeMap<usize, crate::callbacks::PendingOwner>,
    pub async_continuations: bool,
    pub rtd: Option<Rc<dyn crate::RtdService>>,
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
#[derive(Clone, Debug)]
pub struct AsyncReturn {
    pub request: u64,
    pub value: Result<CalcValue, String>,
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
                sheet_names: BTreeMap::new(),
                active_sheet: None,
                events: BTreeMap::new(),
                allocations: HashMap::new(),
                diagnostics: Vec::new(),
                hwnd,
                thread: unsafe { GetCurrentThreadId() },
                closed: false,
                status_message: None,
                workers: Some(Rc::new(crate::worker::Pool::new())),
                pending_async: BTreeMap::new(),
                async_continuations: false,
                rtd: None,
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
        module.reserve_runtime_initialization()?;
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
        // ubs:ignore -- xlAutoOpen has this fixed Excel lifecycle ABI; the export is resolved from the leased module.
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
    /// Publish the document's sheet identity and qualified C API name at a main-lane boundary.
    pub fn set_sheet(&self, sheet: SheetId, qualified_name: String, active: bool) {
        let mut state = self.state.borrow_mut();
        state.sheets.encode(sheet);
        state.sheet_names.insert(sheet, qualified_name);
        if active {
            state.active_sheet = Some(sheet);
        }
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
            .events
            .retain(|(owner, _), _| *owner != module.handle as usize);
        state
            .allocations
            .retain(|_, allocation| allocation.owner != module.handle as usize);
    }
    pub fn functions(&self) -> Result<UserFunctions, String> {
        self.functions_with_early_execution(&[])
    }
    /// Explicit trusted-owner grants, for example the recorded P08 XPure probe.
    /// `$` alone never grants ordering permission. Republish the resulting callable
    /// table so changed grants receive new callable generations.
    pub fn functions_with_early_execution(
        &self,
        registrations: &[u64],
    ) -> Result<UserFunctions, String> {
        let state = self.state.borrow();
        for id in registrations {
            let r = state
                .registrations
                .get(id)
                .ok_or("unknown early-execution registration")?;
            let signature = &r.info.signature;
            if r.uses.get() == 0
                || r.info.macro_type == 2
                || !signature.thread_safe
                || signature.asynchronous
                || signature.macro_equivalent
                || signature.volatile
            {
                return Err("early execution needs an active nonvolatile synchronous worksheet registration without cached-read privileges (dc-hsa5.4)".into());
            }
        }
        let mut functions = UserFunctions::new();
        for (name, id) in &state.names {
            let Some(registration) = state.registrations.get(id) else {
                continue;
            };
            if registration.uses.get() == 0 || registration.info.macro_type == 2 {
                continue;
            }
            let adapter = Rc::new(XllFunction {
                early: registrations.contains(id),
                parallel: if registration.info.signature.thread_safe
                    && !registration.info.signature.asynchronous
                    && !registration.info.signature.volatile
                {
                    state.workers.as_ref().map_or(1, |pool| pool.capacity())
                } else {
                    1
                },
                state: self.state.clone(),
                registration: registration.clone(),
                bound: true,
            });
            functions.register(name, adapter.clone())?;
        }
        Ok(functions)
    }
    /// Execute a registered XLL entry on the owning STA. The entry's originating
    /// module supplies its callback context even when a different add-in loaded last.
    pub fn run(&self, name: &str, arguments: &[CalcValue]) -> Result<CalcValue, String> {
        self.run_variant(name, arguments).map(|(value, _)| value)
    }
    /// Preserve the native integer result when projecting Application.Run to VARIANT.
    pub fn run_variant(
        &self,
        name: &str,
        arguments: &[CalcValue],
    ) -> Result<(CalcValue, bool), String> {
        let registration = {
            let state = self.state.borrow();
            let id = state
                .names
                .get(&calc_value::fold_workbook_name(name))
                .ok_or_else(|| format!("unknown XLL command: {name}"))?;
            state
                .registrations
                .get(id)
                .cloned()
                .ok_or("missing command registration")?
        };
        if registration.info.signature.asynchronous {
            return Err(
                "dc-snz.23: asynchronous Application.Run has no admitted continuation".into(),
            );
        }
        XllFunction {
            early: false,
            parallel: 1,
            state: self.state.clone(),
            registration,
            bound: false,
        }
        .invoke_run(arguments)
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
    /// The currently visible active binding, including re-registration of an older ID.
    /// Inventory order does not determine which registration a worksheet name calls.
    pub fn active_registration_id(&self, name: &str) -> Option<u64> {
        let state = self.state.borrow();
        let id = *state.names.get(&calc_value::fold_workbook_name(name))?;
        state
            .registrations
            .get(&id)
            .filter(|r| r.uses.get() > 0)
            .map(|_| id)
    }
    pub fn diagnostics(&self) -> Vec<String> {
        self.state.borrow().diagnostics.clone()
    }
    pub fn status_message(&self) -> Option<String> {
        self.state.borrow().status_message.clone()
    }
    pub fn live_callback_allocations(&self) -> usize {
        self.state.borrow().allocations.len()
    }
    pub fn runtime_delay_releases(&self) -> usize {
        XLOIL_DELAY_RELEASES.load(Ordering::Acquire)
    }
    /// Includes canceled tasks until native code acknowledges physical return.
    pub fn outstanding_native_calls(&self) -> usize {
        self.reap_cancelled_async();
        self.state.borrow().pending_async.len()
    }
    pub fn set_rtd_service(&self, service: Rc<dyn crate::RtdService>) -> Result<(), String> {
        let mut state = self.state.borrow_mut();
        if state.closed || state.rtd.is_some() {
            return Err("dc-snz.23: RTD service already set or host closed".into());
        }
        state.rtd = Some(service);
        Ok(())
    }
    /// Return messages only. The composition must resolve each request to its complete
    /// owner-issued P07 token and call HostSession::complete_async before resuming.
    pub fn take_async_returns(&self) -> Vec<AsyncReturn> {
        self.reap_cancelled_async();
        self.state
            .borrow()
            .pending_async
            .iter()
            .filter_map(|(&request, held)| {
                if !held.completion.active.load(Ordering::Acquire) {
                    return None;
                }
                let value = held.completion.result.lock().unwrap().clone()?;
                (!held.completion.forwarded.swap(true, Ordering::AcqRel)).then_some(AsyncReturn {
                    request: request as u64,
                    value: value.map(crate::invoke::normalize_return),
                })
            })
            .collect()
    }
    fn reap_cancelled_async(&self) {
        let keys = self
            .state
            .borrow()
            .pending_async
            .iter()
            .filter_map(|(&key, held)| {
                (!held.completion.active.load(Ordering::Acquire)
                    && held.completion.physical_returned.load(Ordering::Acquire))
                .then_some(key)
            })
            .collect::<Vec<_>>();
        for key in keys {
            crate::callbacks::cancel(key);
            self.state.borrow_mut().pending_async.remove(&key);
        }
    }
    /// Select the owner-driven async route before publishing callable adapters.
    /// OneCalc's explicit single-formula controller keeps the synchronous wait route.
    pub fn set_async_continuations(&self, enabled: bool) -> Result<(), String> {
        let mut state = self.state.borrow_mut();
        if !state.pending_async.is_empty() {
            return Err("native async calls are still pending".into());
        }
        state.async_continuations = enabled;
        Ok(())
    }
    pub fn close(&self) -> Result<(), String> {
        if self.state.borrow().closed {
            return Ok(());
        }
        self.reap_cancelled_async();
        if !self.state.borrow().pending_async.is_empty() {
            return Err("dc-snz.23: XLL removal waits for outstanding native async handles".into());
        }
        let service = self.state.borrow().rtd.clone();
        if let Some(service) = service {
            service.close()?;
        }
        let modules = self.state.borrow().modules.clone();
        let mut failures = Vec::new();
        for module in modules.iter().rev() {
            if let Err(why) = self.unload_module(module) {
                failures.push(why);
            }
        }
        self.state.borrow_mut().closed = true;
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join(", "))
        }
    }
    /// Remove one add-in at an execution boundary. Outstanding registry snapshots are
    /// invalidated before their module leases can be released; the host can then reload it.
    pub fn unload(&self, path: &Path) -> Result<(), String> {
        let target = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
        let module = self
            .state
            .borrow()
            .modules
            .iter()
            .find(|m| std::fs::canonicalize(&m.path).ok().as_ref() == Some(&target))
            .cloned()
            .ok_or("XLL is not loaded")?;
        self.unload_module(&module)
    }
    fn unload_module(&self, module: &Rc<Module>) -> Result<(), String> {
        self.reap_cancelled_async();
        let service = self.state.borrow().rtd.clone();
        if service.is_some_and(|service| service.has_live_servers()) {
            return Err("dc-snz.23: XLL removal waits for RTD server termination".into());
        }
        if self.state.borrow().pending_async.values().any(|pending| {
            pending.registration.origin.handle == module.handle
                || pending.registration.module.handle == module.handle
        }) {
            return Err("dc-snz.23: XLL removal waits for outstanding native async handles".into());
        }
        if module.measured_xloil {
            // Retain this process-lifetime fact even if teardown only partly succeeds.
            XLOIL_0221_REMOVED.store(true, Ordering::Release);
        }
        let mut failures = Vec::new();
        for entry in ["xlAutoRemove", "xlAutoClose"] {
            if let Ok(address) = module.address(entry) {
                let mut frame =
                    crate::callbacks::Frame::lifecycle(self.state.clone(), module.clone());
                let guard = unsafe { crate::callbacks::enter(&mut frame) };
                // ubs:ignore -- xlAutoRemove/xlAutoClose use this fixed lifecycle ABI and a resolved, leased-module export.
                let remove: unsafe extern "system" fn() -> i32 =
                    unsafe { std::mem::transmute(address) };
                if unsafe { remove() } as i16 == 0 {
                    failures.push(format!("{entry} returned zero: {}", module.path.display()));
                }
                drop(guard);
            }
        }
        if failures.is_empty() {
            module.delay_cleanup.set(true);
        }
        self.remove_module(module);
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join(", "))
        }
    }
}
impl Drop for XllHost {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            self.state.borrow_mut().refusal(e);
            // A parked invocation can outlive its host handle. Keep native code
            // loaded until process exit when its owner did not drain/cancel it.
            let state = self.state.borrow();
            std::mem::forget(state.modules.clone());
            if !state.pending_async.is_empty() {
                // Final teardown cannot join a hung native task or a task
                // waiting for a callback owner that is being dropped.
                std::mem::forget(state.workers.clone());
            }
            for pending in state.pending_async.values() {
                std::mem::forget(pending.registration.clone());
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct XllFunction {
    pub early: bool,
    pub parallel: usize,
    pub state: Shared,
    pub registration: Rc<Registration>,
    pub bound: bool,
}
impl fmt::Debug for XllFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("XllFunction")
            .field("registration", &self.registration.info)
            .finish()
    }
}
pub(crate) fn refuse_native_error(context: &mut dyn UserFunctionContext, why: &str) {
    for (prefix, owner) in [
        ("dc-snz.2:", "dc-snz.2"),
        ("dc-snz.18:", "dc-snz.18"),
        ("dc-snz.23:", "dc-snz.23"),
    ] {
        if why.starts_with(prefix) {
            context.refuse_by("an XLL invocation capability is not modelled", owner);
            break;
        }
    }
}
impl UserFunction for XllFunction {
    fn permits_early_execution(&self) -> bool {
        self.early
    }
    fn max_parallel_calls(&self) -> usize {
        self.parallel
    }
    fn state_lane(&self) -> Option<u64> {
        (!self.registration.info.signature.thread_safe)
            .then_some(self.registration.origin.handle as u64)
    }
    fn begin_call(
        &self,
        arguments: &[CalcValue],
        frame: calc_func::UserInvocationFrame,
        context: &mut dyn UserFunctionContext,
    ) -> Result<Option<Box<dyn calc_func::UserFunctionInvocation>>, String> {
        if frame.formula_entry {
            if self.registration.info.signature.asynchronous {
                return Err(context.refuse_by(
                    "native async formula-entry evaluation is not modelled",
                    "dc-snz.23",
                ));
            }
            return Ok(None);
        }
        if !self.state.borrow().async_continuations {
            return Ok(None);
        }
        if self.registration.info.signature.thread_safe
            && !self.registration.info.signature.asynchronous
        {
            let pool =
                self.state.borrow().workers.clone().ok_or_else(|| {
                    context.refuse_by("native calculation pool stopped", "dc-snz.23")
                })?;
            let call = crate::worker::submit(self, &pool, arguments, context.caller(), true)
                .map_err(|why| {
                    refuse_native_error(context, &why);
                    why
                })?;
            return Ok(Some(Box::new(call)));
        }
        if !self.registration.info.signature.asynchronous {
            return Ok(None);
        }
        if self.registration.info.signature.thread_safe {
            return Err(context.refuse_by(
                "thread-safe native async entry is not modelled",
                "dc-snz.23",
            ));
        }
        Ok(Some(Box::new(NativeAsyncInvocation {
            function: self.clone(),
            arguments: arguments.to_vec(),
            pending: None,
            finished: false,
        })))
    }
    fn cached_reads(&self) -> bool {
        self.registration.info.signature.macro_equivalent
    }
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
            refuse_native_error(context, why);
        }
        result
    }
}

struct NativeAsyncInvocation {
    function: XllFunction,
    arguments: Vec<CalcValue>,
    pending: Option<crate::callbacks::Pending>,
    finished: bool,
}
impl fmt::Debug for NativeAsyncInvocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeAsyncInvocation")
            .field("request", &self.pending.as_ref().map(|p| p.key))
            .field("finished", &self.finished)
            .finish()
    }
}
impl calc_func::UserFunctionInvocation for NativeAsyncInvocation {
    fn resume(
        &mut self,
        context: &mut dyn UserFunctionContext,
        _quantum: u64,
    ) -> Result<calc_func::UserFunctionStep, String> {
        use calc_func::UserFunctionStep;
        if self.finished {
            return Err("native async activation already completed".into());
        }
        if let Some(pending) = &self.pending {
            let request = pending.key as u64;
            if let Some(value) = context.take_async_result(request) {
                self.pending.take();
                self.finished = true;
                return Ok(UserFunctionStep::Complete(value));
            }
            return Ok(UserFunctionStep::Awaiting { request });
        }
        match self
            .function
            .start_inline(&self.arguments, Some(&mut *context), false)
            .map_err(|why| {
                refuse_native_error(context, &why);
                why
            })? {
            crate::invoke::NativeReturn::Value(value, _) => {
                self.finished = true;
                Ok(UserFunctionStep::Complete(value))
            }
            crate::invoke::NativeReturn::Pending(pending) => {
                let request = pending.key as u64;
                self.pending = Some(pending);
                Ok(UserFunctionStep::Awaiting { request })
            }
        }
    }
}

pub(crate) fn current_sheet(context: &Option<&mut dyn UserFunctionContext>) -> Option<SheetId> {
    context.as_ref().and_then(|c| c.caller()).map(|c| c.sheet())
}
