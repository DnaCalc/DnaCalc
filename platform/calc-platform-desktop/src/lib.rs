//! The native-only package for the desktop platform adapter: native filesystem and operating-system services for the desktop application.
//!
//! Native only: it refuses to compile for wasm32, so it never enters portable library or browser builds.

#[cfg(target_arch = "wasm32")]
compile_error!(
    "calc-platform-desktop is native-only and must not be built for wasm32; portable and browser builds exclude it"
);

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
