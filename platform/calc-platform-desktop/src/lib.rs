//! The native-only package for the desktop platform adapter: native filesystem and operating-system services for the desktop application.
//!
//! Native only: it refuses to compile for wasm32, so it never enters portable library or browser builds.
//!
//! It has no product dependency: nothing here knows of CalcFunc, CalcFml, an add-in or an
//! application. On Windows it holds the shell pieces the desktop applications share:
//! - [`Shell`]: the single-threaded apartment, the `XLMAIN > XLDESK > EXCEL7` window chain and the
//!   message pumps (`shell.rs`);
//! - [`WebView`]: the WebView2 host with its string and bytes bridge, and the thread timer and
//!   message loop that drive it (`webview.rs`);
//! - [`winhttp`]: one GET over WinHTTP, and [`Background`]: work on a thread whose result is
//!   announced to the thread that asked (`winhttp.rs`, `background.rs`);
//! - [`dialogs`]: the native file open and save dialogs (`dialogs.rs`).

#[cfg(target_arch = "wasm32")]
compile_error!(
    "calc-platform-desktop is native-only and must not be built for wasm32; portable and browser builds exclude it"
);

pub mod assets;
#[cfg(windows)]
mod background;
#[cfg(windows)]
pub mod dialogs;
#[cfg(windows)]
mod shell;
#[cfg(windows)]
mod webview;
#[cfg(windows)]
pub mod winhttp;

#[cfg(windows)]
pub use background::{Background, WM_BACKGROUND};
#[cfg(windows)]
pub use shell::{
    CloseHandler, ObjectHandler, Placement, Shell, client_size, placement_of, pump_pending,
    set_window_title,
};
#[cfg(windows)]
pub use webview::{
    RUNTIME_DOWNLOAD, runtime_version,
    SEPARATOR, ThreadMessage, ThreadTimer, WebView, WebViewOptions, base64, per_monitor_dpi_aware,
    run_message_loop, split_message, unbase64,
};

/// The most threads a native host's recalculation uses by default (dc-dxm): past eight the
/// Ofgem workbooks measured no gain, on a machine of eight cores.
pub const MOST_DEFAULT_THREADS: usize = 8;

/// The number of threads a native host's recalculation uses unless told otherwise: the machine's
/// physical cores, at most [`MOST_DEFAULT_THREADS`], and at least one. `HostState::set_threads`
/// applies it; wasm32 hosts stay on one thread.
pub fn default_threads() -> usize {
    physical_cores().clamp(1, MOST_DEFAULT_THREADS)
}

/// The machine's physical processor cores (a core with two hardware threads counts once), or its
/// logical processors when the system does not say.
pub fn physical_cores() -> usize {
    cores_of_system().unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1)
    })
}

/// The machine's local wall clock as a 1900-system Excel serial (what the browser's
/// `serial_clock` gives the web worker), so TODAY, NOW and date-based rules see the same day in the
/// desktop as in a browser.
#[cfg(windows)]
pub fn local_serial_clock() -> Option<f64> {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    // SAFETY: GetLocalTime fills the SYSTEMTIME it is given.
    let t = unsafe {
        let mut t = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    // Days from the civil date to 1970-01-01 (Howard Hinnant's algorithm), then to the serial epoch.
    let (y, m, d) = (i64::from(t.wYear), i64::from(t.wMonth), i64::from(t.wDay));
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let seconds = f64::from(t.wHour) * 3600.0 + f64::from(t.wMinute) * 60.0 + f64::from(t.wSecond)
        + f64::from(t.wMilliseconds) / 1000.0;
    Some(days as f64 + 25_569.0 + seconds / 86_400.0)
}

/// The cores Windows lists for this process's processor group.
#[cfg(windows)]
fn cores_of_system() -> Option<usize> {
    use windows_sys::Win32::System::SystemInformation::{
        GetLogicalProcessorInformation, RelationProcessorCore, SYSTEM_LOGICAL_PROCESSOR_INFORMATION,
    };
    let entry = std::mem::size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION>();
    let mut length = 0u32;
    // SAFETY: a null buffer of length 0 asks for the length needed, written to `length`.
    unsafe { GetLogicalProcessorInformation(std::ptr::null_mut(), &mut length) };
    let count = length as usize / entry;
    if count == 0 {
        return None;
    }
    let mut entries = vec![SYSTEM_LOGICAL_PROCESSOR_INFORMATION::default(); count];
    let mut length = (count * entry) as u32;
    // SAFETY: `entries` holds `length` bytes of entries, which the call fills and counts.
    let filled = unsafe { GetLogicalProcessorInformation(entries.as_mut_ptr(), &mut length) };
    if filled == 0 {
        return None;
    }
    let cores = entries[..(length as usize / entry).min(count)]
        .iter()
        .filter(|entry| entry.Relationship == RelationProcessorCore)
        .count();
    (cores > 0).then_some(cores)
}

/// The distinct (package, core) pairs Linux lists under /sys for its online processors.
#[cfg(not(windows))]
fn cores_of_system() -> Option<usize> {
    let mut cores = std::collections::BTreeSet::new();
    for cpu in std::fs::read_dir("/sys/devices/system/cpu").ok()?.flatten() {
        let name = cpu.file_name();
        let name = name.to_string_lossy();
        if !name.strip_prefix("cpu").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())) {
            continue;
        }
        let read = |file: &str| std::fs::read_to_string(cpu.path().join("topology").join(file)).ok();
        let (Some(package), Some(core)) = (read("physical_package_id"), read("core_id")) else {
            continue;
        };
        cores.insert((package.trim().to_owned(), core.trim().to_owned()));
    }
    (!cores.is_empty()).then_some(cores.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(non_snake_case)]
    fn dc_dxm__the_default_threads_are_the_physical_cores_up_to_eight() {
        let cores = physical_cores();
        assert!(cores >= 1);
        let logical = std::thread::available_parallelism().map(usize::from).unwrap_or(1);
        assert!(cores <= logical, "{cores} cores, {logical} logical processors");
        assert_eq!(default_threads(), cores.min(MOST_DEFAULT_THREADS));
    }
}

#[cfg(all(test, windows))]
mod clock_tests {
    use super::*;

    /// Native-only: the machine's clock. The serial agrees with the UTC time of the system
    /// within the largest time zone offset (a day), and names a date after 2026-01-01 (46023).
    #[test]
    fn dc_0f1k_11_1__the_local_clock_is_a_current_excel_serial() {
        let serial = local_serial_clock().expect("a clock");
        let utc_days = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64()
            / 86_400.0
            + 25_569.0;
        assert!(serial > 46_023.0);
        assert!((serial - utc_days).abs() < 1.0, "{serial} against {utc_days}");
    }
}
