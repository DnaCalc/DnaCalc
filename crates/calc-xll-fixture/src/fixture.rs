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

mod volatility;
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
static ASYNC_LAST_TYPE: AtomicUsize = AtomicUsize::new(usize::MAX);
static ASYNC_MARKER_DELAY: AtomicUsize = AtomicUsize::new(0);
static ASYNC_RESULT_SENTINEL: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
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
    lifecycle_log("xlAutoOpen");
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
    *SELF_PATH.lock().unwrap() = module_name.clone();
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
        // dc-gh22: Excel12(xlfCall) and Excel12(xlfRegisterId) probes and their CALL targets.
        ("MyXlf", "QQQQQQQQQQQ#"),
        ("MyXlfPlain", "QQQQQQQQQQQ"),
        ("MyXlfRaw", "QQQQQQQQQQQ#"),
        ("MyRegIdCheck", "QQQQQ#"),
        ("MyRegIdRepeat", "QQQQQ#"),
        ("MyOpenProbe", "QE"),
        ("MyStrInfoA", "JC"),
        ("MyStrInfoW", "JC%"),
        ("MyCountedInfoA", "JD"),
        ("MyCountedInfoW", "JD%"),
        ("MyBoolRaw", "JA"),
        ("MyBoolEcho", "AA"),
        ("MyShortAdd", "III"),
        ("MyUShortAdd", "HHH"),
        ("MyIntAdd", "JJJ"),
        ("MyDoubleInc", "BE"),
        ("MyIntInc", "JN"),
        ("MyShortInc", "IM"),
        ("MyBoolFlip", "AL"),
        ("MyUpperA", "1F"),
        ("MyUpperW", "1F%"),
        ("MyUpperCountedA", "1G"),
        ("MyUpperCountedW", "1G%"),
        ("MyCatA", "CCC"),
        ("MyCatCountedA", "DDD"),
        ("MyCatW", "C%C%C%"),
        ("MyCatCountedW", "D%D%D%"),
        ("MyOperInfo", "QQ"),
        ("MyThreeArgs", "BBBB"),
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
    ASYNC_RESULT_SENTINEL.store(
        directory.is_some_and(|path| path.join("async-result-sentinel.enable").is_file()),
        Ordering::Release,
    );
    let marker_delay = if let Some(path) = directory.map(|path| path.join("async-delay-ms.txt")) {
        match std::fs::read_to_string(path) {
            Ok(text) => match text.trim().parse::<usize>() {
                Ok(delay) if delay <= 5000 => delay,
                _ => return 0,
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(_) => return 0,
        }
    } else {
        0
    };
    ASYNC_MARKER_DELAY.store(marker_delay, Ordering::Release);
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
    if std::env::var_os("DNA_XLL_ASYNC_TESTS").is_some()
        || directory.is_some_and(|path| path.join("async-tests.enable").is_file())
    {
        for name in [
            "MyAsyncEntries",
            "MyAsyncReturns",
            "MyAsyncLastCode",
            "MyAsyncResultType",
        ] {
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
                && register(&module, "MyOverlapArray", "QE$", "MyOverlapArray")
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
    if std::env::var_os("DNA_XLL_VOLATILITY_TESTS").is_some()
        || directory.is_some_and(|path| path.join("volatility-tests.enable").is_file())
    {
        if !unsafe { volatility::register_all(&module) } {
            return 0;
        }
    }
    unsafe {
        lifecycle_probes(&module_name);
    }
    unsafe {
        call(XL_FREE_FN, &[&mut module], std::ptr::null_mut());
    }
    1
}
/// `DNA_XLL_LIFECYCLE_LOG=<file>` appends one line per lifecycle export entered, so a test can
/// see which of them a host called after the module is gone.
fn lifecycle_log(entry: &str) {
    if let Some(path) = std::env::var_os("DNA_XLL_LIFECYCLE_LOG") {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{entry}");
        }
    }
}
/// The Add-in Manager's removal action. Excel does not enter it on UNREGISTER or close.
#[unsafe(no_mangle)]
pub extern "system" fn xlAutoRemove() -> i32 {
    lifecycle_log("xlAutoRemove");
    1
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn xlAutoClose() -> i32 {
    lifecycle_log("xlAutoClose");
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
pub unsafe extern "system" fn MyOverlapArray(value: *const f64) -> *mut XlOper12 {
    let entered = unsafe { MyOverlap(value) };
    if unsafe { (*entered).xltype } & TYPE_MASK == XL_ERR {
        return entered;
    }
    let start = unsafe { *value };
    let cells = (1..=4)
        .map(|n| XlOper12::number(start + n as f64))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let values = Box::into_raw(cells) as *mut XlOper12;
    let result = Box::into_raw(Box::new(XlOper12 {
        value: XlValue12 {
            array: XlArray12 {
                values,
                rows: 2,
                columns: 2,
            },
        },
        xltype: XL_MULTI | DLL_FREE,
    }));
    OWNERS
        .lock()
        .unwrap()
        .insert(result as usize, unsafe { GetCurrentThreadId() });
    result
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
    let code = if std::env::var_os("DNA_XLL_REFERENCE_TEXT").is_some() {
        let mut style = XlOper12::boolean(true);
        unsafe { call(146, &[reference, &mut style], &mut result) }
    } else {
        unsafe { call(XL_COERCE, &[reference], &mut result) }
    };
    if code != SUCCESS {
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
    } else if owned.xltype & TYPE_MASK == XL_MULTI {
        let array = unsafe { owned.value.array };
        unsafe {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                array.values,
                (array.rows as usize) * (array.columns as usize),
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
        .unwrap_or(ASYNC_MARKER_DELAY.load(Ordering::Acquire) as u64)
        .min(5000);
    let handle_data = unsafe { (*handle).value.big_data };
    let pointer = handle_data.data as usize;
    let byte_count = handle_data.byte_count;
    let sentinel = ASYNC_RESULT_SENTINEL.load(Ordering::Acquire);
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
        let mut result = if sentinel {
            XlOper12::number(13.25)
        } else {
            XlOper12::default()
        };
        let code = unsafe { call(XL_ASYNC_RETURN, &[&mut handle, &mut answer], &mut result) };
        ASYNC_LAST_TYPE.store(result.xltype as usize, Ordering::Release);
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
pub extern "system" fn MyAsyncResultType() -> i32 {
    ASYNC_LAST_TYPE.load(Ordering::Acquire) as i32
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

// ---- dc-gh22: Excel12(xlfCall) / Excel12(xlfRegisterId) probes -------------------------------
const XLF_CALL: i32 = 150;
const XLF_REGISTER_ID: i32 = 267;
static SELF_PATH: Mutex<String> = Mutex::new(String::new());
static OPEN_PROBES: Mutex<Vec<String>> = Mutex::new(Vec::new());
thread_local! {
    static ANSI:UnsafeCell<Vec<u8>>=const{UnsafeCell::new(Vec::new())};
    static WIDE:UnsafeCell<Vec<u16>>=const{UnsafeCell::new(Vec::new())};
}
fn error_name(code: i32) -> String {
    match code {
        0 => "#NULL!".into(),
        7 => "#DIV/0!".into(),
        15 => "#VALUE!".into(),
        23 => "#REF!".into(),
        29 => "#NAME?".into(),
        36 => "#NUM!".into(),
        42 => "#N/A".into(),
        43 => "#GETTING_DATA".into(),
        other => format!("err{other}"),
    }
}
unsafe fn counted_text(pointer: *const u16) -> String {
    if pointer.is_null() {
        return "<null>".into();
    }
    let length = usize::from(unsafe { *pointer });
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(pointer.add(1), length) })
}
fn type_name(xltype: u32) -> String {
    let base = match xltype & TYPE_MASK {
        XL_NUM => "num".to_owned(),
        XL_STR => "str".to_owned(),
        XL_BOOL => "bool".to_owned(),
        XL_REF => "ref".to_owned(),
        XL_ERR => "err".to_owned(),
        XL_MULTI => "multi".to_owned(),
        XL_MISSING => "missing".to_owned(),
        XL_NIL => "nil".to_owned(),
        XL_SREF => "sref".to_owned(),
        XL_INT => "int".to_owned(),
        other => format!("type{other}"),
    };
    let mut flags = String::new();
    if xltype & XL_FREE != 0 {
        flags.push_str("+xlfree");
    }
    if xltype & DLL_FREE != 0 {
        flags.push_str("+dllfree");
    }
    base + &flags
}
unsafe fn describe_value(oper: &XlOper12) -> String {
    unsafe {
        match oper.xltype & TYPE_MASK {
            XL_NUM => format!("{}", oper.value.number),
            XL_STR => counted_text(oper.value.string),
            XL_BOOL => if oper.value.integer != 0 {
                "TRUE"
            } else {
                "FALSE"
            }
            .into(),
            XL_ERR => error_name(oper.value.integer),
            XL_INT => format!("{}", oper.value.integer),
            XL_MULTI => {
                let array = oper.value.array;
                let count = (array.rows.max(0) as usize) * (array.columns.max(0) as usize);
                let items = (0..count)
                    .map(|n| {
                        let item = &*array.values.add(n);
                        format!("{}:{}", type_name(item.xltype), describe_value(item))
                    })
                    .collect::<Vec<_>>();
                format!("{}x{}[{}]", array.rows, array.columns, items.join(";"))
            }
            XL_REF | XL_SREF => "reference".into(),
            _ => String::new(),
        }
    }
}
unsafe fn describe(code: i32, oper: &XlOper12) -> String {
    format!("{code}|{}|{}", type_name(oper.xltype), unsafe {
        describe_value(oper)
    })
}
/// Substitute the fixture's own module path for the tokens a formula cannot know.
unsafe fn resolved(oper: &XlOper12, arena: &mut Arena) -> XlOper12 {
    if oper.xltype & TYPE_MASK != XL_STR {
        return *oper;
    }
    let text = unsafe { counted_text(oper.value.string) };
    let path = SELF_PATH.lock().unwrap().clone();
    let name = path.rsplit('\\').next().unwrap_or("").to_owned();
    let stem = name
        .rsplit_once('.')
        .map(|(s, _)| s.to_owned())
        .unwrap_or(name.clone());
    let replaced = match text.as_str() {
        "@self" => path,
        "@selfname" => name,
        "@selfstem" => stem,
        "@selfupper" => path.to_uppercase(),
        "@selflower" => path.to_lowercase(),
        _ => return *oper,
    };
    arena.text(&replaced).unwrap()
}
fn function_number(oper: &XlOper12) -> Option<i32> {
    unsafe {
        match oper.xltype & TYPE_MASK {
            XL_NUM => Some(oper.value.number as i32),
            XL_INT => Some(oper.value.integer),
            _ => None,
        }
    }
}
/// Calls `Excel12(function, ...)` with the formula's arguments and returns what Excel gave.
unsafe fn forward(args: &[*mut XlOper12], arena: &mut Arena) -> Option<(i32, XlOper12)> {
    let function = function_number(unsafe { &*args[0] })?;
    let mut list = args[1..]
        .iter()
        .map(|p| unsafe { resolved(&**p, arena) })
        .collect::<Vec<_>>();
    while list
        .last()
        .is_some_and(|o| o.xltype & TYPE_MASK == XL_MISSING)
    {
        list.pop();
    }
    let pointers = list
        .iter_mut()
        .map(|o| o as *mut XlOper12)
        .collect::<Vec<_>>();
    let mut result = XlOper12::default();
    let code = unsafe { call(function, &pointers, &mut result) };
    Some((code, result))
}
unsafe fn release(code: i32, result: &mut XlOper12) {
    if code == SUCCESS && result.xltype & XL_FREE != 0 {
        unsafe {
            call(XL_FREE_FN, &[result], std::ptr::null_mut());
        }
    }
}
unsafe fn probe_text(args: &[*mut XlOper12]) -> *mut XlOper12 {
    let mut arena = Arena::new();
    let Some((code, mut result)) = (unsafe { forward(args, &mut arena) }) else {
        return returned(XlOper12::error(15));
    };
    // A register ID differs between processes: report its kind, never its value.
    let text = if function_number(unsafe { &*args[0] }) == Some(XLF_REGISTER_ID)
        && result.xltype & TYPE_MASK == XL_NUM
    {
        let number = unsafe { result.value.number };
        format!(
            "{code}|{}|id integer={}",
            type_name(result.xltype),
            number.fract() == 0.
        )
    } else {
        unsafe { describe(code, &result) }
    };
    unsafe { release(code, &mut result) };
    returned_text(&text)
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyXlf(
    a0: *mut XlOper12,
    a1: *mut XlOper12,
    a2: *mut XlOper12,
    a3: *mut XlOper12,
    a4: *mut XlOper12,
    a5: *mut XlOper12,
    a6: *mut XlOper12,
    a7: *mut XlOper12,
    a8: *mut XlOper12,
    a9: *mut XlOper12,
) -> *mut XlOper12 {
    unsafe { probe_text(&[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9]) }
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyXlfPlain(
    a0: *mut XlOper12,
    a1: *mut XlOper12,
    a2: *mut XlOper12,
    a3: *mut XlOper12,
    a4: *mut XlOper12,
    a5: *mut XlOper12,
    a6: *mut XlOper12,
    a7: *mut XlOper12,
    a8: *mut XlOper12,
    a9: *mut XlOper12,
) -> *mut XlOper12 {
    unsafe { probe_text(&[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9]) }
}
/// Returns the callback's own result operand (so a register ID can feed `xlfCall`), or #VALUE!.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyXlfRaw(
    a0: *mut XlOper12,
    a1: *mut XlOper12,
    a2: *mut XlOper12,
    a3: *mut XlOper12,
    a4: *mut XlOper12,
    a5: *mut XlOper12,
    a6: *mut XlOper12,
    a7: *mut XlOper12,
    a8: *mut XlOper12,
    a9: *mut XlOper12,
) -> *mut XlOper12 {
    let mut arena = Arena::new();
    match unsafe { forward(&[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9], &mut arena) } {
        Some((SUCCESS, result)) => returned(result),
        _ => returned(XlOper12::error(15)),
    }
}
fn registered_id(name: &str) -> Option<f64> {
    REGISTRATIONS
        .lock()
        .unwrap()
        .iter()
        .find(|(_, n)| n == name)
        .map(|(id, _)| *id)
}
fn id_facts(code: i32, result: &XlOper12, expected: Option<f64>) -> String {
    let number = (result.xltype & TYPE_MASK == XL_NUM).then(|| unsafe { result.value.number });
    format!(
        "{code}|{}|same={}|integer={}",
        type_name(result.xltype),
        number.is_some() && number == expected,
        number.is_some_and(|n| n.fract() == 0.),
    )
}
/// REGISTER.ID of one of the fixture's exports, compared with what `xlfRegister` returned for a name.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyRegIdCheck(
    procedure: *mut XlOper12,
    name: *mut XlOper12,
    types: *mut XlOper12,
    module: *mut XlOper12,
) -> *mut XlOper12 {
    let mut arena = Arena::new();
    let name = unsafe { describe_value(&*name) };
    let mut file = unsafe { resolved(&*module, &mut arena) };
    if file.xltype & TYPE_MASK == XL_MISSING {
        file = arena.text(&SELF_PATH.lock().unwrap().clone()).unwrap();
    }
    let mut procedure = unsafe { *procedure };
    let mut types = unsafe { *types };
    let mut list = vec![&mut file as *mut XlOper12, &mut procedure];
    if types.xltype & TYPE_MASK != XL_MISSING {
        list.push(&mut types);
    }
    let mut result = XlOper12::default();
    let code = unsafe { call(XLF_REGISTER_ID, &list, &mut result) };
    let text = id_facts(code, &result, registered_id(&name));
    unsafe { release(code, &mut result) };
    returned_text(&text)
}
/// Three REGISTER.ID calls (the third with the module name in upper case): do they agree?
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyRegIdRepeat(
    module: *mut XlOper12,
    procedure: *mut XlOper12,
    types: *mut XlOper12,
    unused: *mut XlOper12,
) -> *mut XlOper12 {
    let _ = unused;
    let mut arena = Arena::new();
    let first_module = unsafe { resolved(&*module, &mut arena) };
    let upper_module = if first_module.xltype & TYPE_MASK == XL_STR {
        arena
            .text(&unsafe { counted_text(first_module.value.string) }.to_uppercase())
            .unwrap()
    } else {
        first_module
    };
    let mut ids = Vec::new();
    let mut codes = Vec::new();
    for variant in [first_module, first_module, upper_module] {
        let mut file = variant;
        let mut procedure = unsafe { *procedure };
        let mut types = unsafe { *types };
        let mut list = vec![&mut file as *mut XlOper12, &mut procedure];
        if types.xltype & TYPE_MASK != XL_MISSING {
            list.push(&mut types);
        }
        let mut result = XlOper12::default();
        let code = unsafe { call(XLF_REGISTER_ID, &list, &mut result) };
        codes.push(code.to_string());
        ids.push((result.xltype & TYPE_MASK == XL_NUM).then(|| unsafe { result.value.number }));
        unsafe { release(code, &mut result) };
    }
    returned_text(&format!(
        "codes={}|first_repeat_same={}|case_same={}",
        codes.join(","),
        ids[0].is_some() && ids[0] == ids[1],
        ids[0].is_some() && ids[0] == ids[2],
    ))
}
unsafe fn lifecycle_probes(module_name: &str) {
    let mut arena = Arena::new();
    let mut probes = Vec::new();
    let module = arena.text(module_name).unwrap();
    let cases: [(i32, Vec<XlOper12>); 4] = [
        (
            XLF_CALL,
            vec![
                module,
                arena.text("MyAdd").unwrap(),
                arena.text("BBB").unwrap(),
                XlOper12::number(1.),
                XlOper12::number(2.),
            ],
        ),
        (
            XLF_CALL,
            vec![
                arena.text("kernel32").unwrap(),
                arena.text("lstrlenA").unwrap(),
                arena.text("JC").unwrap(),
                arena.text("hello").unwrap(),
            ],
        ),
        (XLF_REGISTER_ID, vec![module, arena.text("MyAdd").unwrap()]),
        (
            XLF_REGISTER_ID,
            vec![
                module,
                arena.text("MyAdd").unwrap(),
                arena.text("BBB").unwrap(),
            ],
        ),
    ];
    for (function, mut list) in cases {
        let pointers = list
            .iter_mut()
            .map(|o| o as *mut XlOper12)
            .collect::<Vec<_>>();
        let mut result = XlOper12::default();
        let code = unsafe { call(function, &pointers, &mut result) };
        let text = if function == XLF_REGISTER_ID {
            id_facts(code, &result, registered_id("MyAdd"))
        } else {
            unsafe { describe(code, &result) }
        };
        unsafe { release(code, &mut result) };
        probes.push(text);
    }
    *OPEN_PROBES.lock().unwrap() = probes;
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyOpenProbe(index: *const f64) -> *mut XlOper12 {
    let probes = OPEN_PROBES.lock().unwrap();
    let index = unsafe { *index } as usize;
    match probes.get(index) {
        Some(text) => {
            let text = text.clone();
            drop(probes);
            returned_text(&text)
        }
        None => returned(XlOper12::error(42)),
    }
}

// CALL targets: one export per type-text form the C API documents.
fn info(length: usize, first: u32) -> i32 {
    (length as i32) * 100000 + first as i32
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyStrInfoA(text: *const u8) -> i32 {
    let bytes = unsafe { std::ffi::CStr::from_ptr(text.cast()) }.to_bytes();
    info(bytes.len(), bytes.first().copied().unwrap_or(0).into())
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyStrInfoW(text: *const u16) -> i32 {
    let mut length = 0;
    while unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    info(
        length,
        if length == 0 {
            0
        } else {
            unsafe { *text }.into()
        },
    )
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyCountedInfoA(text: *const u8) -> i32 {
    let length = usize::from(unsafe { *text });
    info(
        length,
        if length == 0 {
            0
        } else {
            unsafe { *text.add(1) }.into()
        },
    )
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyCountedInfoW(text: *const u16) -> i32 {
    let length = usize::from(unsafe { *text });
    info(
        length,
        if length == 0 {
            0
        } else {
            unsafe { *text.add(1) }.into()
        },
    )
}
#[unsafe(no_mangle)]
pub extern "system" fn MyBoolRaw(value: i16) -> i32 {
    i32::from(value)
}
#[unsafe(no_mangle)]
pub extern "system" fn MyBoolEcho(value: i16) -> i16 {
    value
}
#[unsafe(no_mangle)]
pub extern "system" fn MyShortAdd(a: i16, b: i16) -> i16 {
    a.wrapping_add(b)
}
#[unsafe(no_mangle)]
pub extern "system" fn MyUShortAdd(a: u16, b: u16) -> u16 {
    a.wrapping_add(b)
}
#[unsafe(no_mangle)]
pub extern "system" fn MyIntAdd(a: i32, b: i32) -> i32 {
    a.wrapping_add(b)
}
#[unsafe(no_mangle)]
pub extern "system" fn MyThreeArgs(a: f64, b: f64, c: f64) -> f64 {
    a * 100. + b * 10. + c
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyDoubleInc(value: *mut f64) -> f64 {
    let old = unsafe { *value };
    unsafe { *value = -1. };
    old + 1.
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyIntInc(value: *mut i32) -> i32 {
    let old = unsafe { *value };
    unsafe { *value = -1 };
    old.wrapping_add(1)
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyShortInc(value: *mut i16) -> i16 {
    let old = unsafe { *value };
    unsafe { *value = -1 };
    old.wrapping_add(1)
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyBoolFlip(value: *mut i16) -> i16 {
    let old = unsafe { *value };
    unsafe { *value = 1 };
    i16::from(old == 0)
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyUpperA(text: *mut u8) {
    let mut at = 0;
    while unsafe { *text.add(at) } != 0 {
        unsafe { *text.add(at) = (*text.add(at)).to_ascii_uppercase() };
        at += 1;
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyUpperW(text: *mut u16) {
    let mut at = 0;
    while unsafe { *text.add(at) } != 0 {
        let unit = unsafe { *text.add(at) };
        if (b'a' as u16..=b'z' as u16).contains(&unit) {
            unsafe { *text.add(at) = unit - 32 };
        }
        at += 1;
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyUpperCountedA(text: *mut u8) {
    for at in 1..=usize::from(unsafe { *text }) {
        unsafe { *text.add(at) = (*text.add(at)).to_ascii_uppercase() };
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyUpperCountedW(text: *mut u16) {
    for at in 1..=usize::from(unsafe { *text }) {
        let unit = unsafe { *text.add(at) };
        if (b'a' as u16..=b'z' as u16).contains(&unit) {
            unsafe { *text.add(at) = unit - 32 };
        }
    }
}
fn ansi_return(bytes: Vec<u8>, counted: bool) -> *const u8 {
    ANSI.with(|buffer| unsafe {
        let buffer = &mut *buffer.get();
        buffer.clear();
        if counted {
            buffer.push(bytes.len().min(255) as u8);
            buffer.extend(bytes.iter().take(255));
        } else {
            buffer.extend(bytes);
            buffer.push(0);
        }
        buffer.as_ptr()
    })
}
fn wide_return(units: Vec<u16>, counted: bool) -> *const u16 {
    WIDE.with(|buffer| unsafe {
        let buffer = &mut *buffer.get();
        buffer.clear();
        if counted {
            buffer.push(units.len().min(32767) as u16);
            buffer.extend(units.iter().take(32767));
        } else {
            buffer.extend(units);
            buffer.push(0);
        }
        buffer.as_ptr()
    })
}
unsafe fn read_ansi(text: *const u8) -> Vec<u8> {
    unsafe { std::ffi::CStr::from_ptr(text.cast()) }
        .to_bytes()
        .to_vec()
}
unsafe fn read_counted_ansi(text: *const u8) -> Vec<u8> {
    unsafe { std::slice::from_raw_parts(text.add(1), usize::from(*text)) }.to_vec()
}
unsafe fn read_wide(text: *const u16) -> Vec<u16> {
    let mut length = 0;
    while unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    unsafe { std::slice::from_raw_parts(text, length) }.to_vec()
}
unsafe fn read_counted_wide(text: *const u16) -> Vec<u16> {
    unsafe { std::slice::from_raw_parts(text.add(1), usize::from(*text)) }.to_vec()
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyCatA(a: *const u8, b: *const u8) -> *const u8 {
    let mut joined = unsafe { read_ansi(a) };
    joined.extend(unsafe { read_ansi(b) });
    ansi_return(joined, false)
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyCatCountedA(a: *const u8, b: *const u8) -> *const u8 {
    let mut joined = unsafe { read_counted_ansi(a) };
    joined.extend(unsafe { read_counted_ansi(b) });
    ansi_return(joined, true)
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyCatW(a: *const u16, b: *const u16) -> *const u16 {
    let mut joined = unsafe { read_wide(a) };
    joined.extend(unsafe { read_wide(b) });
    wide_return(joined, false)
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyCatCountedW(a: *const u16, b: *const u16) -> *const u16 {
    let mut joined = unsafe { read_counted_wide(a) };
    joined.extend(unsafe { read_counted_wide(b) });
    wide_return(joined, true)
}
/// What type and value the callee received for an XLOPER12 argument.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyOperInfo(value: *mut XlOper12) -> *mut XlOper12 {
    let oper = unsafe { *value };
    returned_text(&format!("{}|{}", type_name(oper.xltype), unsafe {
        describe_value(&oper)
    }))
}
/// Exports that no fixture registration names: only REGISTER.ID and CALL reach them.
#[unsafe(no_mangle)]
pub extern "system" fn MyUnregAdd(a: f64, b: f64) -> f64 {
    a + b + 0.5
}
#[unsafe(no_mangle)]
pub extern "system" fn MyUnregMul(a: f64, b: f64) -> f64 {
    a * b + 0.25
}
#[unsafe(no_mangle)]
pub extern "system" fn MyUnregSub(a: f64, b: f64) -> f64 {
    a - b + 0.125
}
#[unsafe(no_mangle)]
pub extern "system" fn MyUnregNeg(a: f64) -> f64 {
    -a + 0.0625
}
