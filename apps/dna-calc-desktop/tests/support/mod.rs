//! Shared by the two desktop executables' native test files: a scratch directory with a copy of the
//! executable under test, the original X1 XLL and the XLCALL32 companion.
#![allow(dead_code)]
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
pub struct Fixture {
    pub directory: PathBuf,
    name: std::ffi::OsString,
}
impl Fixture {
    /// A scratch directory holding a copy of `executable` (the exe under test, by its own name), the
    /// X1 fixture as `fixture.xll` and the XLCALL32 companion.
    pub fn new(executable: &Path) -> Self {
        assert_eq!(calc_xll_fixture::abi_version(), 12);
        assert_eq!(xlcall32::XLCallVer(), 3072);
        let directory = std::env::temp_dir().join(format!(
            "dnacalc-xll-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let name = executable.file_name().unwrap();
        std::fs::copy(executable, directory.join(name)).unwrap();
        for (source, target) in [
            ("calc_xll_fixture.dll", "fixture.xll"),
            ("xlcall32.dll", "XLCALL32.DLL"),
        ] {
            let root = executable.parent().unwrap();
            let candidates = [root.join("deps").join(source), root.join(source)];
            let source = candidates
                .iter()
                .find(|p| p.is_file())
                .unwrap_or_else(|| panic!("missing built {source}"));
            std::fs::copy(source, directory.join(target)).unwrap();
        }
        Self {
            directory,
            name: name.to_owned(),
        }
    }
    pub fn command(&self) -> Command {
        let mut command = Command::new(self.directory.join(&self.name));
        command
            .args(["--xll"])
            .arg(self.directory.join("fixture.xll"));
        command
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let resolved = std::fs::canonicalize(&self.directory).unwrap();
        let root = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        assert_eq!(resolved.parent(), Some(root.as_path()));
        assert!(
            resolved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&format!("dnacalc-xll-{}-", std::process::id()))
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            match std::fs::remove_dir_all(&resolved) {
                Ok(()) => break,
                Err(error)
                    if matches!(error.raw_os_error(), Some(5 | 32 | 33))
                        && std::time::Instant::now() < deadline =>
                {
                    // Retry possible brief file locks; permanent cleanup failures still fail.
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                Err(error) => panic!(
                    "cannot remove verified fixture {}: {error}",
                    resolved.display()
                ),
            }
        }
    }
}
pub fn checked(output: Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
