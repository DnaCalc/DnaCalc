//! The native file open and save dialogs.
//!
//! They block the calling thread until the person answers, so an application opens one from a
//! command it handles, never from a pump. A cancelled dialog is `Ok(None)`. Only the filter text,
//! which is plain data, is checked by this package's tests: opening a dialog needs a person.
use std::path::PathBuf;
use windows_sys::Win32::{
    Foundation::HWND,
    UI::Controls::Dialogs::{
        CommDlgExtendedError, GetOpenFileNameW, GetSaveFileNameW, OFN_EXPLORER,
        OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    },
};

/// One entry of a dialog's file type list: a name and the patterns it shows (`*.xll;*.xlam`).
#[derive(Clone, Copy, Debug)]
pub struct FileFilter<'a> {
    pub name: &'a str,
    pub patterns: &'a str,
}

/// The double-null-terminated list the dialog reads: `name\0patterns\0` for each filter.
pub fn filter_text(filters: &[FileFilter<'_>]) -> Vec<u16> {
    let mut out = Vec::new();
    for filter in filters {
        out.extend(filter.name.encode_utf16());
        out.push(0);
        out.extend(filter.patterns.encode_utf16());
        out.push(0);
    }
    out.push(0);
    out
}

/// The file the person chooses to open, or `None` when they cancel.
pub fn open_file(
    owner: HWND,
    title: &str,
    filters: &[FileFilter<'_>],
) -> Result<Option<PathBuf>, String> {
    run(owner, title, filters, None, false)
}

/// The file the person chooses to save as (asking before it replaces one), or `None` when they
/// cancel. `extension` is added to a name given without one, without its dot.
pub fn save_file(
    owner: HWND,
    title: &str,
    filters: &[FileFilter<'_>],
    extension: &str,
) -> Result<Option<PathBuf>, String> {
    run(owner, title, filters, Some(extension), true)
}

fn run(
    owner: HWND,
    title: &str,
    filters: &[FileFilter<'_>],
    extension: Option<&str>,
    save: bool,
) -> Result<Option<PathBuf>, String> {
    let filter = filter_text(filters);
    let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    let extension: Option<Vec<u16>> = extension.map(|e| e.encode_utf16().chain(Some(0)).collect());
    let mut buffer = vec![0u16; 32 * 1024];
    let mut dialog: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    dialog.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = filter.as_ptr();
    dialog.nFilterIndex = 1;
    dialog.lpstrFile = buffer.as_mut_ptr();
    dialog.nMaxFile = buffer.len() as u32;
    dialog.lpstrTitle = title.as_ptr();
    dialog.lpstrDefExt = extension.as_ref().map_or(std::ptr::null(), |e| e.as_ptr());
    dialog.Flags = OFN_EXPLORER | OFN_NOCHANGEDIR | OFN_PATHMUSTEXIST;
    if save {
        dialog.Flags |= OFN_OVERWRITEPROMPT;
    } else {
        dialog.Flags |= OFN_FILEMUSTEXIST;
    }
    // SAFETY: every pointer in `dialog` is to a buffer that outlives the call.
    let chosen = unsafe {
        if save {
            GetSaveFileNameW(&mut dialog)
        } else {
            GetOpenFileNameW(&mut dialog)
        }
    };
    if chosen == 0 {
        return match unsafe { CommDlgExtendedError() } {
            0 => Ok(None),
            code => Err(format!("file dialog failed: {code:#x}")),
        };
    }
    let length = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Ok(Some(PathBuf::from(String::from_utf16_lossy(&buffer[..length]))))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: the filter list is name and patterns pairs ending in an empty entry.
    #[test]
    fn filters_become_a_double_null_terminated_list() {
        let text = filter_text(&[
            FileFilter { name: "Add-ins", patterns: "*.xll;*.xlam" },
            FileFilter { name: "All", patterns: "*.*" },
        ]);
        let expected: Vec<u16> = "Add-ins\0*.xll;*.xlam\0All\0*.*\0\0".encode_utf16().collect();
        assert_eq!(text, expected);
        assert_eq!(filter_text(&[]), [0]);
    }
}
