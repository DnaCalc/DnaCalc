use calc_xll_abi::*;
use std::{
    cell::UnsafeCell,
    collections::BTreeMap,
    ffi::c_void,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
    fn GetModuleHandleExW(flags: u32, name: *const u16, module: *mut *mut c_void) -> i32;
    fn VirtualProtect(address: *mut c_void, bytes: usize, protection: u32, old: *mut u32) -> i32;
    fn GetCurrentThreadId() -> u32;
}
static CALLBACK: AtomicUsize = AtomicUsize::new(0);
static FREES: AtomicUsize = AtomicUsize::new(0);
static OWNERS: Mutex<BTreeMap<usize, u32>> = Mutex::new(BTreeMap::new());
static BAD_FREES: AtomicUsize = AtomicUsize::new(0);
static ASYNC_ENTRIES: AtomicUsize = AtomicUsize::new(0);
static ASYNC_RETURNS: AtomicUsize = AtomicUsize::new(0);
static ASYNC_LAST_CODE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);
static REGISTRATIONS: Mutex<Vec<(f64, String)>> = Mutex::new(Vec::new());
static WORKERS: Mutex<Vec<std::thread::JoinHandle<()>>> = Mutex::new(Vec::new());
static OVERLAP: Mutex<(usize, usize, usize)> = Mutex::new((0, 0, 0));
static OVERLAP_READY: std::sync::Condvar = std::sync::Condvar::new();
thread_local! {
    static RETURN:UnsafeCell<XlOper12>=UnsafeCell::new(XlOper12::default());
    static TEXT:UnsafeCell<Vec<u16>>=const{UnsafeCell::new(Vec::new())};
}
fn callback() -> Callback12 {
    unsafe { std::mem::transmute(CALLBACK.load(Ordering::Relaxed)) }
}
unsafe fn call(function: i32, args: &[*mut XlOper12], result: *mut XlOper12) -> i32 {
    unsafe { callback()(function, args.len() as i32, args.as_ptr(), result) }
}
fn returned(oper: XlOper12) -> *mut XlOper12 {
    RETURN.with(|p| unsafe {
        *p.get() = oper;
        p.get()
    })
}
fn returned_text(text: &str) -> *mut XlOper12 {
    TEXT.with(|p| unsafe {
        let buffer = &mut *p.get();
        buffer.clear();
        buffer.push(text.encode_utf16().count() as u16);
        buffer.extend(text.encode_utf16());
        returned(XlOper12 {
            value: XlValue12 {
                string: buffer.as_mut_ptr(),
            },
            xltype: XL_STR,
        })
    })
}
unsafe fn register(module: &XlOper12, procedure: &str, signature: &str, name: &str) -> bool {
    let mut arena = Arena::new();
    let mut file = *module;
    let mut proc = arena.text(procedure).unwrap();
    let mut types = arena.text(signature).unwrap();
    let mut name_oper = arena.text(name).unwrap();
    let args = [&mut file as *mut _, &mut proc, &mut types, &mut name_oper];
    let mut result = XlOper12::default();
    let code = unsafe { call(XLF_REGISTER, &args, &mut result) };
    if code != SUCCESS || result.xltype & TYPE_MASK != XL_NUM {
        return false;
    }
    REGISTRATIONS
        .lock()
        .unwrap()
        .push((unsafe { result.value.number }, name.to_owned()));
    unsafe {
        call(XL_FREE_FN, &[&mut result], std::ptr::null_mut());
    }
    true
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn xlAutoOpen() -> i32 {
    let address = unsafe {
        GetProcAddress(
            GetModuleHandleW(std::ptr::null()),
            c"MdCallBack12".as_ptr().cast(),
        )
    };
    if address.is_null() {
        return 0;
    }
    CALLBACK.store(address as usize, Ordering::Relaxed);
    let dll = "XLCALL32.DLL\0".encode_utf16().collect::<Vec<_>>();
    let shim = unsafe { LoadLibraryW(dll.as_ptr()) };
    if shim.is_null() {
        return 0;
    }
    let version = unsafe { GetProcAddress(shim, c"XLCallVer".as_ptr().cast()) };
    let pen = unsafe { GetProcAddress(shim, c"LPenHelper".as_ptr().cast()) };
    if version.is_null() || pen.is_null() {
        unsafe {
            FreeLibrary(shim);
        }
        return 0;
    }
    let version: unsafe extern "system" fn() -> i32 = unsafe { std::mem::transmute(version) };
    let pen: unsafe extern "system" fn(i32, *mut u8) -> i32 = unsafe { std::mem::transmute(pen) };
    #[repr(C, align(8))]
    struct Info([u8; 32]);
    let mut info = Info([0; 32]);
    let valid = unsafe {
        version() == 3072
            && pen(16398, info.0.as_mut_ptr()) == 0
            && *(info.0.as_ptr().cast::<i32>()) == 0
    };
    unsafe {
        FreeLibrary(shim);
    }
    if !valid {
        return 0;
    }
    let outside = std::thread::spawn(|| {
        for function in [XL_GET_NAME, XL_STACK, XLF_CALLER, XL_FREE_FN] {
            let mut result = XlOper12::default();
            let args = if function == XL_FREE_FN {
                vec![&mut result as *mut _]
            } else {
                vec![]
            };
            if unsafe { call(function, &args, std::ptr::null_mut()) } != FAILED {
                return false;
            }
        }
        true
    })
    .join()
    .unwrap_or(false);
    if !outside {
        return 0;
    }
    let mut version_item = XlOper12::integer(2);
    let mut version_result = XlOper12::default();
    if unsafe { call(186, &[&mut version_item], &mut version_result) } != SUCCESS {
        return 0;
    }
    unsafe {
        call(XL_FREE_FN, &[&mut version_result], std::ptr::null_mut());
    }
    let mut module = XlOper12::default();
    if unsafe { call(XL_GET_NAME, &[], &mut module) } != SUCCESS {
        return 0;
    }
    let module_name = unsafe {
        let ptr = module.value.string;
        String::from_utf16_lossy(std::slice::from_raw_parts(ptr.add(1), usize::from(*ptr)))
    };
    let targeted_failure = std::env::var("DNA_XLL_FAIL_MODULE")
        .ok()
        .is_some_and(|name| module_name.ends_with(&name));
    if targeted_failure {
        // Change a previous add-in's use count and unrelated hidden name before failing.
        let mut arena = Arena::new();
        let mut name = arena.text("MyHelperAdd").unwrap();
        let mut id = XlOper12::default();
        unsafe {
            call(XLF_EVALUATE, &[&mut name], &mut id);
        }
        let mut result = XlOper12::default();
        unsafe {
            call(XLF_UNREGISTER, &[&mut id], &mut result);
        }
        let mut name = arena.text("MyAdd").unwrap();
        unsafe {
            call(XLF_SET_NAME, &[&mut name], &mut result);
        }
    }
    if let Ok(helper) = std::env::var("DNA_XLL_HELPER") {
        let path = helper.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let handle = unsafe { LoadLibraryW(path.as_ptr()) };
        if handle.is_null() {
            return 0;
        }
        let mut arena = Arena::new();
        let helper = arena.text(&helper).unwrap();
        let registered = unsafe {
            register(&helper, "MyAdd", "BBB", "MyHelperAdd")
                && register(&helper, "MyOwned", "Q", "MyHelperOwned")
                && register(&helper, "MyFreeCount", "J", "MyHelperFreeCount")
                && register(&module, "MyFreeCount", "J", "MyFreeCount")
        };
        unsafe {
            FreeLibrary(handle);
        }
        if !registered
            || (std::env::var_os("DNA_XLL_FAIL_AFTER_HELPER").is_some()
                && (std::env::var_os("DNA_XLL_FAIL_MODULE").is_none() || targeted_failure))
        {
            // Deliberately leave xlGetName storage outstanding as well: failed-load cleanup
            // must release allocations and registrations owned by this originating load.
            return 0;
        }
    }
    let entries = [
        ("MyAdd", "BBB"),
        ("MyEcho", "QQ"),
        ("MyOwned", "Q"),
        ("MyThreadSafe", "QE$"),
        ("MyCaller", "Q#"),
        ("MyReference", "QU#"),
        ("MyAsync", ">EX"),
        ("MyCallbacks", "QQQ"),
        ("MyWide", "QD%"),
        ("MyFpSum", "BK%"),
        ("MyMix", "BBQBQB"),
        ("MyVolatile", "QE!"),
        ("MyUnsupported", "Q"),
        ("MyNonfiniteValue", "B"),
        ("MyNonfinitePointer", "E"),
        ("MyNonfiniteOper", "Q"),
    ];
    for (proc, signature) in entries {
        if !unsafe { register(&module, proc, signature, proc) } {
            return 0;
        }
    }
    // A second registration exercises use counts without creating another callable name.
    if !unsafe { register(&module, "MyAdd", "BBB", "MyAdd") } {
        return 0;
    }
    if !unsafe { dynamic_registrations(&module) } {
        return 0;
    }
    if let Ok(extra) = std::env::var("DNA_XLL_EXTRA_REGISTRATIONS") {
        let Ok(extra) = extra.parse::<usize>() else {
            return 0;
        };
        if extra > 10000 {
            return 0;
        }
        for n in 0..extra {
            if !unsafe { register(&module, "MyAdd", "BBB", &format!("Pool{n}")) } {
                return 0;
            }
        }
    }
    let directory = std::path::Path::new(&module_name).parent();
    if std::env::var_os("DNA_XLL_WORKER_TESTS").is_some()
        || directory.is_some_and(|path| path.join("worker-tests.enable").is_file())
    {
        for (procedure, signature, name) in [
            ("MyOwned", "Q$", "MyWorkerOwned"),
            ("MyReference", "QU$", "MyWorkerReference"),
            ("MyCallbacks", "QQQ$", "MyWorkerCallbacks"),
            ("MyThreadId", "J$", "MyWorkerThread"),
            ("MyThreadId", "J", "MyMainThread"),
            ("MyFreeCount", "J", "MyFreeCount"),
            ("MyUnsupported", "Q$", "MyWorkerUnsupported"),
            ("MyDeniedStatus", "J$", "MyWorkerDeniedStatus"),
            ("MyUnsupported", "Q#", "MyMacroSetStatus"),
        ] {
            if !unsafe { register(&module, procedure, signature, name) } {
                return 0;
            }
        }
    }
    if std::env::var_os("DNA_XLL_ASYNC_TESTS").is_some() {
        for name in ["MyAsyncEntries", "MyAsyncReturns", "MyAsyncLastCode"] {
            if !unsafe { register(&module, name, "J", name) } {
                return 0;
            }
        }
    }
    if std::env::var_os("DNA_XLL_POOL_TESTS").is_some()
        || directory.is_some_and(|path| path.join("pool-tests.enable").is_file())
    {
        if !unsafe {
            register(&module, "MyOverlap", "QE$", "MyOverlap")
                && register(&module, "MyOverlapMax", "J", "MyOverlapMax")
                && register(&module, "MyOverlapReset", "J#", "MyOverlapReset")
        } {
            return 0;
        }
    }
    if std::env::var_os("DNA_XLL_ABI_NEGATIVE_TESTS").is_some() {
        // These deliberately unsupported registrations must refuse before
        // constructing an ABI call, so their procedures must never be entered.
        if !unsafe {
            register(&module, "MyUnsupported", "P$", "MyPoolUnsupportedAbi")
                && register(&module, "MyAsync", ">CX", "MyAsyncUnsupportedAbi")
        } {
            return 0;
        }
    }
    unsafe {
        call(XL_FREE_FN, &[&mut module], std::ptr::null_mut());
    }
    1
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn xlAutoClose() -> i32 {
    for worker in std::mem::take(&mut *WORKERS.lock().unwrap()) {
        if worker.join().is_err() {
            return 0;
        }
    }
    let entries = std::mem::take(&mut *REGISTRATIONS.lock().unwrap());
    for (id, name) in entries {
        let mut id = XlOper12::number(id);
        let mut result = XlOper12::default();
        unsafe {
            call(XLF_UNREGISTER, &[&mut id], &mut result);
            call(XL_FREE_FN, &[&mut result], std::ptr::null_mut());
        }
        let mut arena = Arena::new();
        let mut name = arena.text(&name).unwrap();
        unsafe {
            call(XLF_SET_NAME, &[&mut name], &mut result);
            call(XL_FREE_FN, &[&mut result], std::ptr::null_mut());
        }
    }
    1
}
#[unsafe(no_mangle)]
pub extern "system" fn MyAdd(a: f64, b: f64) -> f64 {
    a + b
}
#[unsafe(no_mangle)]
pub extern "system" fn MyThreadId() -> i32 {
    (unsafe { GetCurrentThreadId() }) as i32
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyEcho(value: *mut XlOper12) -> *mut XlOper12 {
    value
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyWide(value: *mut u16) -> *mut XlOper12 {
    returned(XlOper12 {
        value: XlValue12 { string: value },
        xltype: XL_STR,
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyThreadSafe(value: *const f64) -> *mut XlOper12 {
    returned(XlOper12::number(unsafe { *value } + 1.))
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyOverlap(value: *const f64) -> *mut XlOper12 {
    let mut held = OVERLAP.lock().unwrap();
    held.0 += 1;
    held.1 += 1;
    held.2 = held.2.max(held.0);
    OVERLAP_READY.notify_all();
    let (mut held, _) = OVERLAP_READY
        .wait_timeout_while(held, std::time::Duration::from_secs(10), |s| s.1 < 2)
        .unwrap();
    let both_entered = held.1 >= 2;
    held.0 -= 1;
    drop(held);
    returned(if both_entered {
        XlOper12::number(unsafe { *value } + 1.)
    } else {
        XlOper12::error(15)
    })
}
#[unsafe(no_mangle)]
pub extern "system" fn MyOverlapMax() -> i32 {
    OVERLAP.lock().unwrap().2 as i32
}
#[unsafe(no_mangle)]
pub extern "system" fn MyOverlapReset() -> i32 {
    let mut held = OVERLAP.lock().unwrap();
    if held.0 != 0 {
        return 0;
    }
    *held = (0, 0, 0);
    1
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyVolatile(value: *const f64) -> *mut XlOper12 {
    returned(XlOper12::number(unsafe { *value } + 1.))
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyMix(
    a: f64,
    b: *const XlOper12,
    c: f64,
    d: *const XlOper12,
    e: f64,
) -> f64 {
    a + unsafe { (*b).value.number } + c + unsafe { (*d).value.number } + e
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyFpSum(value: *const Fp12) -> f64 {
    let count = unsafe { (*value).rows as usize * (*value).columns as usize };
    unsafe { std::slice::from_raw_parts(value.cast::<u8>().add(8).cast::<f64>(), count) }
        .iter()
        .sum()
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyCallbacks(a: *mut XlOper12, b: *mut XlOper12) -> *mut XlOper12 {
    let mut result = XlOper12::default();
    if unsafe { call(4, &[a, b], &mut result) } != SUCCESS {
        result = XlOper12::error(15);
    }
    returned(result)
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyUnsupported() -> *mut XlOper12 {
    let mut result = XlOper12::default();
    let function = std::env::var("DNA_XLL_NEGATIVE_CALLBACK")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(12345);
    let mut arena = Arena::new();
    if function == XL_UDF {
        let mut target = arena.text("MyReference").unwrap();
        unsafe {
            call(function, &[&mut target], &mut result);
        }
    } else if function == 32890 {
        let mut show = XlOper12::boolean(true);
        let mut text = arena.text("worker status").unwrap();
        unsafe {
            call(function, &[&mut show, &mut text], &mut result);
        }
    } else if function == 16401 {
        let mut procedure = arena.text("MyThreadId").unwrap();
        let mut event = XlOper12::number(1.);
        unsafe {
            call(function, &[&mut procedure, &mut event], &mut result);
        }
    } else if let Ok(text) = std::env::var("DNA_XLL_EVALUATE_TEXT") {
        let mut text = arena.text(&text).unwrap();
        unsafe {
            call(function, &[&mut text], &mut result);
        }
    } else {
        unsafe {
            call(function, &[], &mut result);
        }
    }
    // Ignore the callback status. Unsupported operations must retain a refusal;
    // modeled permission denials may be ignored, as in Excel.
    returned(XlOper12::number(0.))
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyDeniedStatus() -> i32 {
    let mut arena = Arena::new();
    let mut show = XlOper12::boolean(true);
    let mut text = arena.text("denied worker status").unwrap();
    let mut result = XlOper12::default();
    unsafe { call(32890, &[&mut show, &mut text], &mut result) }
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyReference(reference: *mut XlOper12) -> *mut XlOper12 {
    let mut result = XlOper12::default();
    if unsafe { call(XL_COERCE, &[reference], &mut result) } != SUCCESS {
        result = XlOper12::error(15);
    }
    // Any host-owned nested storage is released after the host copies this result.
    result.xltype |= XL_FREE;
    returned(result)
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyCaller() -> *mut XlOper12 {
    let mut result = XlOper12::default();
    if unsafe { call(XLF_CALLER, &[], &mut result) } != SUCCESS {
        return returned(XlOper12::error(15));
    }
    let rect = unsafe {
        match result.xltype & TYPE_MASK {
            XL_SREF => result.value.single_reference.rectangle,
            XL_REF => (*result.value.reference.table).rectangles[0],
            _ => return returned(result),
        }
    };
    unsafe {
        call(XL_FREE_FN, &[&mut result], std::ptr::null_mut());
    }
    returned_text(&format!(
        "{},{},{},{}",
        rect.row_first, rect.row_last, rect.col_first, rect.col_last
    ))
}
#[unsafe(no_mangle)]
pub extern "system" fn MyOwned() -> *mut XlOper12 {
    let text = "native owned".encode_utf16().collect::<Vec<_>>();
    let mut units = std::iter::once(text.len() as u16)
        .chain(text)
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let pointer = units.as_mut_ptr();
    std::mem::forget(units);
    let result = Box::into_raw(Box::new(XlOper12 {
        value: XlValue12 { string: pointer },
        xltype: XL_STR | DLL_FREE,
    }));
    OWNERS
        .lock()
        .unwrap()
        .insert(result as usize, unsafe { GetCurrentThreadId() });
    result
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn xlAutoFree12(value: *mut XlOper12) {
    if value.is_null() {
        return;
    }
    FREES.fetch_add(1, Ordering::Relaxed);
    if OWNERS.lock().unwrap().remove(&(value as usize)) != Some(unsafe { GetCurrentThreadId() }) {
        BAD_FREES.fetch_add(1, Ordering::Relaxed);
    }
    let owned = unsafe { Box::from_raw(value) };
    if owned.xltype & TYPE_MASK == XL_STR {
        let pointer = unsafe { owned.value.string };
        let length = unsafe { *pointer } as usize + 1;
        unsafe {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                pointer, length,
            )));
        }
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn MyFreeCount() -> i32 {
    if BAD_FREES.load(Ordering::Relaxed) != 0 {
        return -1;
    }
    FREES.load(Ordering::Relaxed) as i32
}
#[unsafe(no_mangle)]
pub extern "system" fn MyNonfiniteValue() -> f64 {
    f64::NAN
}
#[unsafe(no_mangle)]
pub extern "system" fn MyNonfinitePointer() -> *const f64 {
    static VALUE: f64 = f64::INFINITY;
    &VALUE
}
#[unsafe(no_mangle)]
pub extern "system" fn MyNonfiniteOper() -> *mut XlOper12 {
    returned(XlOper12::number(f64::NAN))
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyAsync(value: *const f64, handle: *const XlOper12) {
    ASYNC_ENTRIES.fetch_add(1, Ordering::AcqRel);
    let value = unsafe { *value };
    let delay = std::env::var("DNA_XLL_ASYNC_DELAY_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
        .min(5000);
    let handle_data = unsafe { (*handle).value.big_data };
    let pointer = handle_data.data as usize;
    let byte_count = handle_data.byte_count;
    let worker = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(delay));
        let mut handle = XlOper12 {
            value: XlValue12 {
                big_data: XlBigData {
                    data: pointer as *mut u8,
                    byte_count,
                },
            },
            xltype: XL_BIG_DATA,
        };
        let mut answer = XlOper12::number(value + 1.);
        let mut result = XlOper12::default();
        let code = unsafe { call(XL_ASYNC_RETURN, &[&mut handle, &mut answer], &mut result) };
        ASYNC_LAST_CODE.store(code, Ordering::Release);
        ASYNC_RETURNS.fetch_add(1, Ordering::AcqRel);
    });
    WORKERS.lock().unwrap().push(worker);
}
#[unsafe(no_mangle)]
pub extern "system" fn MyAsyncEntries() -> i32 {
    ASYNC_ENTRIES.load(Ordering::Acquire) as i32
}
#[unsafe(no_mangle)]
pub extern "system" fn MyAsyncReturns() -> i32 {
    ASYNC_RETURNS.load(Ordering::Acquire) as i32
}
#[unsafe(no_mangle)]
pub extern "system" fn MyAsyncLastCode() -> i32 {
    ASYNC_LAST_CODE.load(Ordering::Acquire)
}

#[unsafe(no_mangle)]
pub extern "system" fn DynamicSlot(value: f64) -> f64 {
    value + 1.
}
#[unsafe(no_mangle)]
pub extern "system" fn DynamicOther(value: f64) -> f64 {
    value + 2.
}

/// Emulate xlOil's documented registration strategy, changing only our own PE metadata.
/// Register the same procedure name twice with different addresses, then restore its export.
unsafe fn dynamic_registrations(module: &XlOper12) -> bool {
    let mut handle = std::ptr::null_mut();
    if unsafe { GetModuleHandleExW(6, DynamicSlot as *const u16, &mut handle) } == 0 {
        return false;
    }
    let base = handle as usize;
    let u16_at = |offset: usize| unsafe { std::ptr::read_unaligned((base + offset) as *const u16) };
    let u32_at = |offset: usize| unsafe { std::ptr::read_unaligned((base + offset) as *const u32) };
    if u16_at(0) != 0x5a4d {
        return false;
    }
    let pe = u32_at(0x3c) as usize;
    if u32_at(pe) != 0x4550 || u16_at(pe + 24) != 0x20b {
        return false;
    }
    let export = u32_at(pe + 24 + 112) as usize;
    if export == 0 {
        return false;
    }
    let count = u32_at(export + 24) as usize;
    let functions = u32_at(export + 28) as usize;
    let names = u32_at(export + 32) as usize;
    let ordinals = u32_at(export + 36) as usize;
    let mut slot = std::ptr::null_mut::<u32>();
    for i in 0..count {
        let name = unsafe {
            std::ffi::CStr::from_ptr((base + u32_at(names + i * 4) as usize) as *const i8)
        };
        if name.to_bytes() == b"DynamicSlot" {
            let ordinal = u16_at(ordinals + i * 2) as usize;
            slot = (base + functions + ordinal * 4) as *mut u32;
            break;
        }
    }
    if slot.is_null() {
        return false;
    }
    if !unsafe { register(module, "DynamicSlot", "BB", "MyDynamicA") } {
        return false;
    }
    let original = unsafe { *slot };
    let replacement = DynamicOther as *const () as usize - base;
    let Ok(replacement) = u32::try_from(replacement) else {
        return false;
    };
    let mut old = 0;
    if unsafe { VirtualProtect(slot.cast(), 4, 4, &mut old) } == 0 {
        return false;
    }
    unsafe {
        *slot = replacement;
    }
    let registered = unsafe { register(module, "DynamicSlot", "BB", "MyDynamicB") };
    unsafe {
        *slot = original;
    }
    let mut unused = 0;
    let restored = unsafe { VirtualProtect(slot.cast(), 4, old, &mut unused) } != 0;
    registered && restored
}
