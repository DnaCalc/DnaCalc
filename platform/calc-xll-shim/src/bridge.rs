use calc_xll_abi::*;

#[unsafe(no_mangle)]
pub extern "system" fn XLCallVer() -> i32 {
    12 * 256
}

#[repr(C)]
pub struct FormulaInfo {
    pub mode: i32,
    pub characters: i32,
    pub text: *const u16,
    pub first: i32,
    pub last: i32,
    pub caret: i32,
}

/// Ready is the actual state of the headless X1 shell. Its remaining fields are undefined
/// by the API in ready mode, so leave them untouched.
/// # Safety
/// `info` must point to a writable FormulaInfo for xlGetFmlaInfo.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn LPenHelper(code: i32, info: *mut FormulaInfo) -> i32 {
    if code != 16398 || info.is_null() {
        return -1;
    }
    unsafe {
        (*info).mode = 0;
    }
    0
}

/// # Safety
/// Arguments and result follow the Excel12v ABI and remain valid during the callback.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Excel12v(
    function: i32,
    result: *mut XlOper12,
    count: i32,
    args: *const *mut XlOper12,
) -> i32 {
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetModuleHandleW(name: *const u16) -> *mut c_void;
            fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
        }
        let main = unsafe { GetModuleHandleW(std::ptr::null()) };
        let address = unsafe { GetProcAddress(main, c"MdCallBack12".as_ptr().cast()) };
        if address.is_null() {
            return FAILED;
        }
        let callback: Callback12 = unsafe { std::mem::transmute(address) };
        unsafe { callback(function, count, args, result) }
    }
    #[cfg(not(windows))]
    {
        let _ = (function, result, count, args);
        FAILED
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Native-only: checks the companion DLL ABI and the headless shell's edit mode.
    #[test]
    fn version_and_ready_mode() {
        assert_eq!(XLCallVer(), 3072);
        let mut info = FormulaInfo {
            mode: 9,
            characters: 99,
            text: std::ptr::null(),
            first: 7,
            last: 8,
            caret: 9,
        };
        assert_eq!(unsafe { LPenHelper(16398, &mut info) }, 0);
        assert_eq!((info.mode, info.characters, info.first), (0, 99, 7));
        assert_eq!(unsafe { LPenHelper(0, &mut info) }, -1);
    }
}
