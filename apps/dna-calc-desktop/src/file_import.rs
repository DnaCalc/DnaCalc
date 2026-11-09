//! The desktop's file-read route for IMPORTCSV and IMPORTTEXT: the bytes of the file a workbook
//! names, read at once from the local file system. `calc_func::import` holds every rule (which
//! paths are good, the encodings, the fields); this only reads.
use calc_func::ImportRoute;

#[derive(Debug, Default)]
pub struct LocalFiles;

impl ImportRoute for LocalFiles {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        std::fs::read(path).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: reads a file of the temporary folder from disk. A folder and a missing
    /// file are unreadable, which the import turns into #VALUE!.
    #[test]
    fn reads_a_file_and_refuses_a_folder() {
        let folder = std::env::temp_dir().join(format!("dnacalc-import-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("a.csv");
        std::fs::write(
            &file,
            b"a,b
1,2
",
        )
        .unwrap();
        assert_eq!(
            LocalFiles.read(file.to_str().unwrap()).as_deref(),
            Some(
                &b"a,b
1,2
"[..]
            )
        );
        assert_eq!(LocalFiles.read(folder.to_str().unwrap()), None);
        assert_eq!(
            LocalFiles.read(folder.join("missing.csv").to_str().unwrap()),
            None
        );
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
