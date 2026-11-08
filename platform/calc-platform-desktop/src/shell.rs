//! The desktop shell every DNA application starts from: a single-threaded COM apartment, the
//! `XLMAIN > XLDESK > EXCEL7` window chain that add-ins look for in Excel's process, and the
//! message pumps the owning thread runs.
//!
//! The chain is plain windows. What answers an add-in's `WM_GETOBJECT` (the Excel object model of
//! the add-in host) is not this package's business: the owner registers it with
//! [`Shell::set_object_handler`], so the shell has no product dependency.
use std::{
    cell::RefCell,
    path::Path,
    rc::Rc,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{Com::*, LibraryLoader::GetModuleHandleW},
    UI::WindowsAndMessaging::*,
};

/// Answers `WM_GETOBJECT` on the innermost window: `(wParam, objectId)` to the `LRESULT`.
pub type ObjectHandler = Rc<dyn Fn(usize, isize) -> isize>;

/// Where the innermost window finds its handler. The window procedure reads it through the
/// window's user data, so it lives as long as the shell.
#[derive(Default)]
struct Slot(RefCell<Option<ObjectHandler>>);

/// The window chain, owned by the thread that made it.
pub struct Shell {
    root: HWND,
    workbook: HWND,
    slot: Box<Slot>,
}

unsafe extern "system" fn window(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if message == WM_GETOBJECT {
        let slot = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const Slot;
        if !slot.is_null() {
            // The handler is cloned out before it runs: it may replace itself.
            let handler = unsafe { &*slot }.0.borrow().clone();
            if let Some(handler) = handler {
                return handler(w, l);
            }
        }
    }
    if message == WM_SIZE {
        // The Excel window chain (XLMAIN > XLDESK > EXCEL7) fills its parent, as Excel's does.
        let child = unsafe { GetWindow(hwnd, GW_CHILD) };
        if !child.is_null() {
            let (width, height) = ((l & 0xffff) as i32, ((l >> 16) & 0xffff) as i32);
            unsafe { MoveWindow(child, 0, 0, width, height, 1) };
        }
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, message, w, l) }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

impl Shell {
    /// Enters a single-threaded apartment on this thread and creates the three windows, hidden,
    /// titled `title`.
    pub fn new(title: &str) -> Result<Self, String> {
        let status = unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
        if status < 0 {
            return Err(format!("CoInitializeEx STA: {status:#x}"));
        }
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let mut parent: HWND = std::ptr::null_mut();
        let mut root: HWND = std::ptr::null_mut();
        for name in ["XLMAIN", "XLDESK", "EXCEL7"] {
            let class = wide(name);
            let mut wc: WNDCLASSW = unsafe { std::mem::zeroed() };
            wc.lpfnWndProc = Some(window);
            wc.hInstance = instance;
            wc.lpszClassName = class.as_ptr();
            if unsafe { RegisterClassW(&wc) } == 0
                && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS
            {
                if !root.is_null() {
                    unsafe {
                        DestroyWindow(root);
                    }
                }
                unsafe {
                    CoUninitialize();
                }
                return Err(format!(
                    "RegisterClassW {name}: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    wide(title).as_ptr(),
                    if parent.is_null() {
                        WS_OVERLAPPEDWINDOW
                    } else {
                        WS_CHILD
                    },
                    0,
                    0,
                    640,
                    480,
                    parent,
                    std::ptr::null_mut(),
                    instance,
                    std::ptr::null(),
                )
            };
            if hwnd.is_null() {
                if !root.is_null() {
                    unsafe {
                        DestroyWindow(root);
                    }
                }
                unsafe {
                    CoUninitialize();
                }
                return Err(format!(
                    "CreateWindowExW {name}: {}",
                    std::io::Error::last_os_error()
                ));
            }
            if root.is_null() {
                root = hwnd;
            }
            parent = hwnd;
        }
        let slot = Box::<Slot>::default();
        unsafe {
            SetWindowLongPtrW(parent, GWLP_USERDATA, (&*slot as *const Slot) as isize);
        }
        Ok(Self {
            root,
            workbook: parent,
            slot,
        })
    }

    /// Sets (or with `None` clears) what answers `WM_GETOBJECT` on the innermost window.
    pub fn set_object_handler(&self, handler: Option<ObjectHandler>) {
        *self.slot.0.borrow_mut() = handler;
    }

    /// Runs the thread's pending messages for `duration`.
    pub fn pump_for(&self, duration: Duration) {
        let start = Instant::now();
        loop {
            pump_pending();
            if start.elapsed() >= duration {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// The innermost window (EXCEL7), where an application puts its page.
    pub fn workbook_hwnd(&self) -> HWND {
        self.workbook
    }

    /// Sizes the window chain to `width` by `height` client pixels and shows it without taking
    /// the focus.
    pub fn show(&self, width: i32, height: i32) {
        unsafe {
            let mut frame = RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            };
            AdjustWindowRect(&mut frame, WS_OVERLAPPEDWINDOW, 0);
            SetWindowPos(
                self.root,
                std::ptr::null_mut(),
                0,
                0,
                frame.right - frame.left,
                frame.bottom - frame.top,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            ShowWindow(self.root, SW_SHOWNOACTIVATE);
        }
    }

    /// The outermost window (XLMAIN), as the number add-ins read from `Application.Hwnd`.
    pub fn hwnd(&self) -> usize {
        self.root as usize
    }

    /// The client size of the innermost window, when it has one.
    pub fn client_size(&self) -> Option<(i32, i32)> {
        client_size(self.workbook)
    }

    /// Runs the thread's messages until `path` exists, for at most two minutes.
    pub fn wait_for(&self, path: &Path) -> Result<(), String> {
        let start = Instant::now();
        while !path.exists() {
            pump_pending();
            if start.elapsed() > Duration::from_secs(120) {
                return Err(format!("timed out waiting for {}", path.display()));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }
}

/// The client size of `window` in pixels, when it has one.
pub fn client_size(window: HWND) -> Option<(i32, i32)> {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    (unsafe { GetClientRect(window, &mut rect) } != 0)
        .then(|| (rect.right - rect.left, rect.bottom - rect.top))
}

/// Dispatches every message waiting for this thread.
pub fn pump_pending() {
    let mut message: MSG = unsafe { std::mem::zeroed() };
    while unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        // Detach the handler before anything it reaches is released. A later WM_GETOBJECT
        // must never find it, including while COM releases pump sent messages.
        self.set_object_handler(None);
        unsafe {
            SetWindowLongPtrW(self.workbook, GWLP_USERDATA, 0);
        }
        unsafe {
            DestroyWindow(self.root);
            CoUninitialize();
        }
    }
}
