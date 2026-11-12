//! Where a workbook file named on the command line came from, which decides whether its VBA
//! project may be enabled (dc-0f1k.22): a Mark of the Web names its zone, a UNC path is the
//! network, and any other path is a local file.
use calc_host_vba::FileOrigin;
use std::path::Path;

#[cfg_attr(not(windows), allow(dead_code))]
pub fn file_origin(path: &Path) -> FileOrigin {
    calc_platform_desktop::origin::file_origin(path)
}

/// Substitute a selected native file in the page's existing command envelope.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn open_request(
    text: &str,
    opened: &calc_platform_desktop::origin::OpenedFile,
    allow_startup: bool,
) -> Option<String> {
    use dna_calc_worker::wire::{self, Request, ToServer};
    let mut message = wire::decode::<ToServer>(text).ok()?;
    let Request::Command(envelope) = &mut message.request else {
        return None;
    };
    if !(matches!(*envelope.request, Request::OpenNative)
        || allow_startup
            && matches!(
                *envelope.request,
                Request::New | Request::OpenTemplate { .. }
            ))
    {
        return None;
    }
    *envelope.request = Request::Open {
        name: opened.name.clone(),
    };
    Some(wire::encode(&message))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: reads a marked native file and serves its page Open request.
    #[test]
    fn a_native_picker_open_carries_the_selected_origin_to_the_worker() {
        use dna_calc_worker::{
            server::{self, Server},
            wire::{self, Request, ToServer},
        };
        let dir = std::env::temp_dir().join(format!("dna-calc-native-open-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("book.xlsm");
        std::fs::write(
            &path,
            include_bytes!("../../../crates/calc-doc/tests/fixtures/vba_udf_names/local.xlsm"),
        )
        .unwrap();
        std::fs::write(
            dir.join("book.xlsm:Zone.Identifier"),
            "[ZoneTransfer]\nZoneId=3",
        )
        .unwrap();
        let opened = calc_platform_desktop::origin::open_file(&path).unwrap();
        let mut server = Server::new();
        let text = wire::encode(&ToServer {
            id: 7,
            request: server.envelope(Request::OpenNative),
        });
        let request = open_request(&text, &opened, false)
            .expect("native selection substitutes the Open request");
        let (reply, _) = server.with_open_origin(opened.origin, |server| {
            server::answer_text(server, &request, Some(opened.bytes))
        });
        assert!(reply.contains("Blocked"), "{reply}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Native-only: creates local files and malformed alternate streams.
    #[test]
    fn malformed_unreadable_and_explicit_data_streams_cannot_lose_origin() {
        let dir =
            std::env::temp_dir().join(format!("dna-calc-origin-security-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("book.xlsm");
        std::fs::write(&file, b"x").unwrap();
        let stream = dir.join("book.xlsm:Zone.Identifier");
        std::fs::write(&stream, "[ZoneTransfer]\nZoneId=garbage").unwrap();
        assert!(
            file_origin(&file).block().is_some(),
            "malformed stream requires unblock"
        );
        std::fs::remove_file(&stream).unwrap();
        #[cfg(not(windows))]
        {
            std::fs::create_dir(&stream).unwrap();
            assert!(
                file_origin(&file).block().is_some(),
                "unreadable stream requires unblock"
            );
            std::fs::remove_dir(&stream).unwrap();
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::write(&stream, "[ZoneTransfer]\nZoneId=3").unwrap();
            let lock = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&stream)
                .unwrap();
            assert!(
                file_origin(&file).block().is_some(),
                "unreadable stream requires unblock"
            );
            drop(lock);
        }
        std::fs::write(&stream, "[ZoneTransfer]\nZoneId=3").unwrap();
        for suffix in ["::$DATA", ":$DATA", "::$data"] {
            assert_eq!(
                file_origin(&dir.join(format!("book.xlsm{suffix}"))),
                FileOrigin::Zone(3)
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

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
        std::fs::write(
            std::path::PathBuf::from(stream),
            "[ZoneTransfer]\r\nZoneId=3\r\n",
        )
        .unwrap();
        assert_eq!(file_origin(&file), FileOrigin::Zone(3));
        assert!(file_origin(&file).block().is_some());
        assert_eq!(
            file_origin(Path::new(r"\\server\share\book.xlsm")),
            FileOrigin::Network
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
