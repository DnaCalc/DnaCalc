//! Where a workbook file named on the command line came from, which decides whether its VBA
//! project may be enabled (dc-0f1k.22): a Mark of the Web names its zone, a UNC path is the
//! network, and any other path is a local file.
use calc_host_vba::FileOrigin;
use std::path::Path;

#[cfg_attr(not(windows), allow(dead_code))]
pub fn file_origin(path: &Path) -> FileOrigin {
    if calc_platform_desktop::origin::is_network_path(path) {
        FileOrigin::Network
    } else if let Some(zone) = calc_platform_desktop::origin::mark_of_the_web(path) {
        FileOrigin::Zone(zone)
    } else {
        FileOrigin::Local
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: writes a file and its `Zone.Identifier` stream (an alternate data stream on
    /// Windows, a file of that name elsewhere) and reads the origin back.
    #[test]
    fn a_marked_file_is_blocked_and_an_unmarked_one_is_local() {
        let dir = std::env::temp_dir().join(format!("dna-calc-origin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("book.xlsm");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(file_origin(&file), FileOrigin::Local);
        assert_eq!(file_origin(&file).block(), None);
        let mut stream = file.as_os_str().to_owned();
        stream.push(":Zone.Identifier");
        std::fs::write(std::path::PathBuf::from(stream), "[ZoneTransfer]\r\nZoneId=3\r\n").unwrap();
        assert_eq!(file_origin(&file), FileOrigin::Zone(3));
        assert!(file_origin(&file).block().is_some());
        assert_eq!(
            file_origin(Path::new(r"\\server\share\book.xlsm")),
            FileOrigin::Network
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
