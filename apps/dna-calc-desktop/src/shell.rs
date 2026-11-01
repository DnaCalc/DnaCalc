use std::{
    path::Path,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{Com::*, LibraryLoader::GetModuleHandleW},
    UI::WindowsAndMessaging::*,
};

pub struct Shell {
    root: HWND,
}
unsafe extern "system" fn window(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, message, w, l) }
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
impl Shell {
    pub fn new() -> Result<Self, String> {
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
                    wide("DNA Calc X1").as_ptr(),
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
        Ok(Self { root })
    }
    pub fn hwnd(&self) -> usize {
        self.root as usize
    }
    pub fn wait_for(&self, path: &Path) -> Result<(), String> {
        let start = Instant::now();
        while !path.exists() {
            let mut message: MSG = unsafe { std::mem::zeroed() };
            while unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0
            {
                unsafe {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            if start.elapsed() > Duration::from_secs(120) {
                return Err(format!("timed out waiting for {}", path.display()));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }
}
impl Drop for Shell {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.root);
            CoUninitialize();
        }
    }
}
