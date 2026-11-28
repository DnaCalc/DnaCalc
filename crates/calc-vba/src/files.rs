//! The file facet a native workbook session supplies to its VBA project.

use calcvba_hal::{HalProfileId, HostServices, adapters::builder::HostBuilder};
use calcvba_runtime::Variant;
use std::{path::Path, sync::Arc};

pub use calcvba_hal::model::{HostPolicy, WindowsMountAccess};

scoped_tls_hkt::scoped_thread_local!(static FILES: for<'a> &'a FileSession);

/// One mounted file namespace and its per-drive current directories.
pub struct FileSession {
    host: Arc<dyn HostServices>,
    default_directory: Option<String>,
}

impl FileSession {
    pub fn new(policy: HostPolicy) -> Self {
        let profile = if cfg!(windows) {
            HalProfileId::Windows
        } else if cfg!(target_os = "macos") {
            HalProfileId::MacOs
        } else {
            HalProfileId::Linux
        };
        Self {
            host: HostBuilder::new().profile(profile).policy(policy).build(),
            default_directory: None,
        }
    }

    /// Translate a host-selected path through the session's existing VBA file facet.
    pub fn windows_path(&self, path: &Path) -> Option<String> {
        self.host.fs().windows_path_for_host(path)
    }

    pub fn set_default_directory(&mut self, directory: String) -> Result<(), String> {
        let facet = self.host.fs();
        if let Some((drive, _)) = directory.split_once(':') {
            facet
                .ch_drive_variant(Variant::from_string(drive))
                .map_err(|e| e.to_string())?;
        }
        facet
            .ch_dir_variant(Variant::from_string(directory.as_str()))
            .map_err(|e| e.to_string())?;
        self.default_directory = Some(directory);
        Ok(())
    }

    pub fn current_directory(&self) -> Option<String> {
        let answer = self
            .host
            .fs()
            .cur_dir_variant(Variant::from_string(""))
            .ok()?;
        answer.as_bstr().map(|path| path.as_str().to_owned())
    }

    /// Provide the file facet while the workbook project is prepared.
    pub fn with<T>(&self, prepare: impl FnOnce() -> T) -> T {
        FILES.set(self, prepare)
    }
}

pub(crate) fn current_host() -> Option<Arc<dyn HostServices>> {
    FILES
        .is_set()
        .then(|| FILES.with(|files| files.host.clone()))
}

pub fn default_directory() -> Option<String> {
    if FILES.is_set() {
        FILES.with(|files| files.default_directory.clone())
    } else {
        None
    }
}
