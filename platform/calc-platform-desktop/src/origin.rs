//! Where a file the person opens came from: its Mark of the Web and whether its path is on the
//! network. The desktop applications decide from these whether an opened workbook's VBA project
//! may be enabled (docs/architecture/ONECALC_ADDINS.md, Trust).

use std::path::Path;

/// The zone in a file's `Zone.Identifier` alternate data stream (`ZoneId=3`), when it has one.
pub fn mark_of_the_web(path: &Path) -> Option<u32> {
    let mut stream = path.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    let text = std::fs::read_to_string(std::path::PathBuf::from(stream)).ok()?;
    zone_of(&text)
}

/// `ZoneId=<n>` in the text of a `Zone.Identifier` stream.
pub fn zone_of(text: &str) -> Option<u32> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("ZoneId="))
        .find_map(|zone| zone.trim().parse().ok())
}

/// Whether `path` names a file on the network by its UNC form (`\\server\share\...`,
/// `//server/share/...` or `\\?\UNC\server\...`). A local device path (`\\?\C:\...`, `\\.\...`)
/// is not. A mapped network drive letter is not seen here: telling one apart needs the drive's
/// type from Windows (dc-0f1k.22 notes).
pub fn is_network_path(path: &Path) -> bool {
    let text = path.as_os_str().to_string_lossy();
    let bytes = text.as_bytes();
    let separator = |b: u8| b == b'\\' || b == b'/';
    if bytes.len() < 3 || !separator(bytes[0]) || !separator(bytes[1]) {
        return false;
    }
    match bytes[2] {
        b'?' => text[3..]
            .get(..5)
            .is_some_and(|unc| unc.eq_ignore_ascii_case("\\UNC\\") || unc.eq_ignore_ascii_case("/UNC/")),
        b'.' if bytes.get(3).copied().is_some_and(separator) => false,
        b if separator(b) => false,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(mark_of_the_web(&file), None);
        let mut stream = file.as_os_str().to_owned();
        stream.push(":Zone.Identifier");
        std::fs::write(
            std::path::PathBuf::from(stream),
            "[ZoneTransfer]\r\nZoneId=3\r\n",
        )
        .unwrap();
        assert_eq!(mark_of_the_web(&file), Some(3));
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
