//! Where a file the person opens came from: its Mark of the Web and whether its path is on the
//! network. The desktop applications decide from these whether an opened workbook's VBA project
//! may be enabled (docs/architecture/ONECALC_ADDINS.md, Trust).

use std::path::Path;

pub use calc_macro_security::FileOrigin;
use std::io::{self, Read};

const MAX_ZONE_BYTES: u64 = 16 * 1024;

/// File bytes, native identity and origin from the same native open route.
pub struct OpenedFile {
    pub path: std::path::PathBuf,
    pub name: String,
    pub bytes: Vec<u8>,
    pub origin: FileOrigin,
}

pub fn open_file(path: &Path) -> io::Result<OpenedFile> {
    let path = default_data_path(path);
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(3); // Share read/write with editors; deny delete to keep the selected identity stable.
    }
    let mut file = options.open(&path)?;
    let origin = file_origin(&path);
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a file"))?
        .to_string_lossy()
        .into_owned();
    Ok(OpenedFile {
        path,
        name,
        bytes,
        origin,
    })
}

/// Remove explicit unnamed data-stream suffixes before appending Zone.Identifier.
fn default_data_path(path: &Path) -> std::path::PathBuf {
    let bytes = path.as_os_str().as_encoded_bytes();
    for suffix in [b"::$DATA".as_slice(), b":$DATA".as_slice()] {
        if bytes
            .get(bytes.len().saturating_sub(suffix.len())..)
            .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix))
        {
            // SAFETY: removing a known ASCII suffix from an OS string preserves every
            // platform encoding boundary, including non-Unicode names in its prefix.
            let prefix = unsafe {
                std::ffi::OsStr::from_encoded_bytes_unchecked(&bytes[..bytes.len() - suffix.len()])
            };
            return Path::new(prefix).to_owned();
        }
    }
    path.to_owned()
}

/// Only an absent stream means unmarked. Unreadable or malformed MOTW is an error.
pub fn mark_of_the_web(path: &Path) -> io::Result<Option<u32>> {
    let mut stream = default_data_path(path).as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    let file = match std::fs::File::open(std::path::PathBuf::from(stream)) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut bytes = Vec::new();
    file.take(MAX_ZONE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_ZONE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "MOTW stream too large",
        ));
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "MOTW encoding"))?;
    zone_of(text)
        .map(Some)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "malformed MOTW"))
}

/// Exactly one ZoneId under ZoneTransfer, bounded to the five Windows security zones.
pub fn zone_of(text: &str) -> Option<u32> {
    if text.len() as u64 > MAX_ZONE_BYTES {
        return None;
    }
    let mut in_zone = false;
    let mut zone = None;
    for line in text.trim_start_matches('\u{feff}').lines().map(str::trim) {
        if line.starts_with('[') {
            in_zone = line.eq_ignore_ascii_case("[ZoneTransfer]");
        } else if in_zone {
            if let Some((key, value)) = line.split_once('=') {
                if key.trim().eq_ignore_ascii_case("ZoneId") {
                    if zone.is_some() {
                        return None;
                    }
                    let parsed = value.trim().parse::<u32>().ok()?;
                    if parsed > 4 {
                        return None;
                    }
                    zone = Some(parsed);
                }
            }
        }
    }
    zone
}

pub fn file_origin(path: &Path) -> FileOrigin {
    if is_network_path(path) {
        FileOrigin::Network
    } else {
        match mark_of_the_web(path) {
            Ok(Some(zone)) => FileOrigin::Zone(zone),
            Ok(None) => FileOrigin::Local,
            Err(_) => FileOrigin::Unknown,
        }
    }
}

/// Whether `path` names a file on the network by its UNC form (`\\server\share\...`,
/// `//server/share/...` or `\\?\UNC\server\...`). A local device path (`\\?\C:\...`, `\\.\...`)
/// is not. On Windows mapped drives are checked with GetDriveTypeW.
pub fn is_network_path(path: &Path) -> bool {
    #[cfg(windows)]
    {
        // Resolve relative and drive-relative paths to the volume actually opened.
        let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
        network_path_with_drive_type(&absolute, |root| {
            // SAFETY: root is a live NUL-terminated drive-root string.
            unsafe { windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(root.as_ptr()) }
        })
    }
    #[cfg(not(windows))]
    {
        network_path_with_drive_type(path, |_| 0)
    }
}

fn network_path_with_drive_type(path: &Path, drive_type: impl FnOnce(&[u16]) -> u32) -> bool {
    let bytes = path.as_os_str().as_encoded_bytes();
    let disk = |bytes: &[u8]| {
        bytes
            .first()
            .copied()
            .filter(u8::is_ascii_alphabetic)
            .filter(|_| bytes.get(1) == Some(&b':'))
    };
    let letter = disk(bytes).or_else(|| {
        // Verbatim and device namespace drive paths still name the mapped volume.
        (bytes.len() >= 6
            && bytes[0] == 92
            && bytes[1] == 92
            && matches!(bytes[2], b'?' | b'.')
            && bytes[3] == 92)
            .then(|| disk(&bytes[4..]))
            .flatten()
    });
    if let Some(letter) = letter {
        let root = [u16::from(letter), b':' as u16, 92, 0];
        if drive_type(&root) == 4 {
            return true;
        } // DRIVE_REMOTE
    }
    let text = path.as_os_str().to_string_lossy();
    let bytes = text.as_bytes();
    let separator = |b: u8| b == b'\\' || b == b'/';
    if bytes.len() < 3 || !separator(bytes[0]) || !separator(bytes[1]) {
        return false;
    }
    match bytes[2] {
        b'?' => text[3..].get(..5).is_some_and(|unc| {
            unc.eq_ignore_ascii_case("\\UNC\\") || unc.eq_ignore_ascii_case("/UNC/")
        }),
        b'.' if bytes.get(3).copied().is_some_and(separator) => false,
        b if separator(b) => false,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: checks Windows sharing while an editor already has write access.
    #[cfg(windows)]
    #[test]
    fn a_native_open_can_read_a_file_an_editor_has_open_for_writing() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = std::env::temp_dir().join(format!("dnacalc-shared-open-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("book.xlsm");
        std::fs::write(&path, b"workbook").unwrap();
        let editor = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(3)
            .open(&path)
            .unwrap();
        assert_eq!(open_file(&path).unwrap().bytes, b"workbook");
        drop(editor);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn mapped_network_drives_include_normal_verbatim_and_device_paths() {
        for path in [r"Z:\book.xlsm", r"\\?\Z:\book.xlsm", r"\\.\Z:\book.xlsm"] {
            assert!(
                network_path_with_drive_type(Path::new(path), |root| {
                    assert_eq!(root, &[b'Z' as u16, b':' as u16, 92, 0]);
                    4 // DRIVE_REMOTE
                }),
                "{path}"
            );
        }
        assert!(!network_path_with_drive_type(
            Path::new(r"C:\book.xlsm"),
            |_| 3
        ));
    }

    #[test]
    fn zone_ids_outside_zone_transfer_are_not_trusted() {
        assert_eq!(
            zone_of("[Unrelated]\nZoneId=0\n[ZoneTransfer]\nZoneId=3"),
            Some(3)
        );
        assert_eq!(zone_of("[Unrelated]\nZoneId=0"), None);
        assert_eq!(zone_of("[ZoneTransfer]\nZoneId=0\nZoneId=3"), None);
        assert_eq!(zone_of("[ZoneTransfer]\nZoneId=99"), None);
    }

    /// The `Zone.Identifier` stream of a downloaded file (Windows writes `ZoneId` under `[ZoneTransfer]`).
    #[test]
    fn the_zone_is_read_from_the_stream_text() {
        assert_eq!(
            zone_of("[ZoneTransfer]\r\nZoneId=3\r\nReferrerUrl=https://example.test/\r\n"),
            Some(3)
        );
        assert_eq!(zone_of("[ZoneTransfer]\nZoneId=4"), Some(4));
        assert_eq!(zone_of("[ZoneTransfer]\r\nZoneId=x\r\n"), None);
        assert_eq!(zone_of(""), None);
    }

    /// Native-only: writes a file and its `Zone.Identifier` stream (on Windows an alternate data
    /// stream; elsewhere a file of that name, read the same way).
    #[test]
    fn a_file_with_an_alternate_stream_reports_its_zone() {
        let dir = std::env::temp_dir().join(format!("dnacalc-zone-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("marked.xlsm");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(mark_of_the_web(&file).unwrap(), None);
        let mut stream = file.as_os_str().to_owned();
        stream.push(":Zone.Identifier");
        std::fs::write(
            std::path::PathBuf::from(stream),
            "[ZoneTransfer]\r\nZoneId=3\r\n",
        )
        .unwrap();
        assert_eq!(mark_of_the_web(&file).unwrap(), Some(3));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unc_paths_are_on_the_network_and_device_paths_are_not() {
        for (path, network) in [
            (r"\\server\share\book.xlsm", true),
            ("//server/share/book.xlsm", true),
            (r"\\?\UNC\server\share\book.xlsm", true),
            (r"\\?\unc\server\share\book.xlsm", true),
            (r"\\?\C:\Users\me\book.xlsm", false),
            (r"\\.\C:\book.xlsm", false),
            (r"C:\Users\me\book.xlsm", false),
            ("/home/me/book.xlsm", false),
            ("book.xlsm", false),
            (r"\\", false),
        ] {
            assert_eq!(is_network_path(Path::new(path)), network, "{path}");
        }
    }
}
