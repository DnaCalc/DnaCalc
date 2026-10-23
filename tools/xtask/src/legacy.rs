//! The legacy root and read-only access to the legacy repositories: every git call here reads. None fetches, checks
//! out, stashes, writes an index or changes a configuration. Each call passes
//! `--no-optional-locks` and `-c safe.directory=<repository>` (a WSL host reading a Windows
//! checkout through `/mnt/c` needs the second), never editing any git configuration.
//!
//! The working-tree state is read with [`STATUS_CONFIG`] added to `status --porcelain`, so that
//! every host measures the same thing: only the repository's own ignore rules apply (a user's
//! global ignore file differs between a Windows and a WSL account), line endings compare as the
//! Windows checkout wrote them, and a file whose size and modification time are unchanged is not
//! re-read (a WSL host otherwise re-hashes every file through `/mnt/c`).

use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The variable that names the directory holding the legacy repositories.
pub const LEGACY_ROOT_VARIABLE: &str = "DNACALC_LEGACY_ROOT";

pub struct LegacyRoot {
    pub path: PathBuf,
    pub from_variable: bool,
}

/// The legacy root: `DNACALC_LEGACY_ROOT` when set and not empty, else the workspace root's parent.
pub fn legacy_root(workspace_root: &Path) -> LegacyRoot {
    match std::env::var_os(LEGACY_ROOT_VARIABLE).filter(|value| !value.is_empty()) {
        Some(value) => LegacyRoot {
            path: PathBuf::from(value),
            from_variable: true,
        },
        None => LegacyRoot {
            path: workspace_root
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default(),
            from_variable: false,
        },
    }
}

/// The legacy root as a receipt shows it, never as an absolute host path: `..` when it is the
/// workspace root's parent, a workspace-relative path when it lies inside the workspace, and
/// `<legacy-root>` anywhere else (the record's `from` then names `DNACALC_LEGACY_ROOT`).
pub fn shown_legacy_root(workspace_root: &Path, path: &Path) -> String {
    let (root, legacy) = match (fs::canonicalize(workspace_root), fs::canonicalize(path)) {
        (Ok(root), Ok(legacy)) => (root, legacy),
        _ => (workspace_root.to_path_buf(), path.to_path_buf()),
    };
    if root.parent() == Some(legacy.as_path()) {
        return "..".to_owned();
    }
    match legacy.strip_prefix(&root) {
        Ok(inside) if inside.as_os_str().is_empty() => ".".to_owned(),
        Ok(inside) => inside.to_string_lossy().replace('\\', "/"),
        Err(_) => "<legacy-root>".to_owned(),
    }
}

/// The legacy root as an acceptance receipt records it.
pub fn legacy_root_record(workspace_root: &Path) -> Value {
    let root = legacy_root(workspace_root);
    json!({
        "path": shown_legacy_root(workspace_root, &root.path),
        "from": if root.from_variable {
            LEGACY_ROOT_VARIABLE
        } else {
            "default: the workspace root's parent directory"
        },
        "exists": root.path.is_dir(),
    })
}

/// The configuration added to `status`, as `-c` arguments; see the module documentation.
pub const STATUS_CONFIG: [&str; 4] = [
    "core.excludesFile=",
    "core.autocrlf=true",
    "core.checkStat=minimal",
    "core.trustctime=false",
];

/// A legacy repository directory, addressed read-only.
pub struct Legacy {
    pub dir: PathBuf,
}

/// `path` with forward slashes, as git accepts it on every host.
pub fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

impl Legacy {
    pub fn new(dir: PathBuf) -> Legacy {
        Legacy { dir }
    }

    /// `git --no-optional-locks -c safe.directory=<dir> [extra config] -C <dir> <args>`.
    fn command(&self, config: &[&str], args: &[&str]) -> Command {
        let mut command = Command::new("git");
        command
            .arg("--no-optional-locks")
            .arg("-c")
            .arg(format!("safe.directory={}", slashed(&self.dir)));
        for setting in config {
            command.arg("-c").arg(setting);
        }
        command.arg("-C").arg(&self.dir).args(args);
        command
    }

    /// The standard output of a git call that must succeed.
    pub fn read(&self, args: &[&str]) -> Result<Vec<u8>, String> {
        self.read_with(&[], args)
    }

    fn read_with(&self, config: &[&str], args: &[&str]) -> Result<Vec<u8>, String> {
        let out = self
            .command(config, args)
            .output()
            .map_err(|e| format!("cannot run git {args:?} in {}: {e}", self.dir.display()))?;
        if out.status.success() {
            Ok(out.stdout)
        } else {
            Err(format!(
                "git {args:?} in {} exited with {:?}: {}",
                self.dir.display(),
                out.status.code(),
                String::from_utf8_lossy(&out.stderr).trim()
            ))
        }
    }

    pub fn text(&self, args: &[&str]) -> Result<String, String> {
        self.read(args)
            .map(|out| String::from_utf8_lossy(&out).trim().to_owned())
    }

    /// The porcelain status lines, read with [`STATUS_CONFIG`].
    pub fn status(&self) -> Result<Vec<String>, String> {
        let out = self.read_with(&STATUS_CONFIG, &["status", "--porcelain=v1", "-z"])?;
        let text = String::from_utf8_lossy(&out).into_owned();
        let mut lines = Vec::new();
        let mut entries = text.split('\0').filter(|e| !e.is_empty());
        while let Some(entry) = entries.next() {
            if entry.starts_with(['R', 'C']) {
                let from = entries.next().unwrap_or_default();
                lines.push(format!("{entry} <- {from}"));
            } else {
                lines.push(entry.to_owned());
            }
        }
        lines.sort();
        Ok(lines)
    }
}
