//! Read-only access to the legacy repositories: every git call here reads. None fetches, checks
//! out, stashes, writes an index or changes a configuration. Each call passes
//! `--no-optional-locks` and `-c safe.directory=<repository>` (a WSL host reading a Windows
//! checkout through `/mnt/c` needs the second), never editing any git configuration.
//!
//! The working-tree state is read with [`STATUS_CONFIG`] added to `status --porcelain`, so that
//! every host measures the same thing: only the repository's own ignore rules apply (a user's
//! global ignore file differs between a Windows and a WSL account), line endings compare as the
//! Windows checkout wrote them, and a file whose size and modification time are unchanged is not
//! re-read (a WSL host otherwise re-hashes every file through `/mnt/c`).

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

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

    /// Whether a git call exits 0, for probes such as `rev-parse --verify`.
    pub fn succeeds(&self, args: &[&str]) -> bool {
        self.command(&[], args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
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

    /// Every blob of `commit`'s tree: path to object ID (submodule entries are left out).
    pub fn tree(&self, commit: &str) -> Result<BTreeMap<String, String>, String> {
        let out = self.read(&["ls-tree", "-r", "-z", "--full-tree", commit])?;
        let text = String::from_utf8_lossy(&out);
        let mut files = BTreeMap::new();
        for entry in text.split('\0').filter(|e| !e.is_empty()) {
            let Some((meta, path)) = entry.split_once('\t') else {
                return Err(format!("ls-tree printed an entry without a path: {entry}"));
            };
            let mut fields = meta.split(' ');
            let (_mode, kind, object) = (fields.next(), fields.next(), fields.next());
            if kind == Some("blob")
                && let Some(object) = object
            {
                files.insert(path.to_owned(), object.to_owned());
            }
        }
        Ok(files)
    }

    /// A reader of blob contents through one `git cat-file --batch`.
    pub fn blobs(&self) -> Result<BlobReader, String> {
        let mut child = self
            .command(&[], &["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot start git cat-file in {}: {e}", self.dir.display()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or("git cat-file has no standard input")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("git cat-file has no standard output")?;
        Ok(BlobReader {
            child,
            stdin: Some(stdin),
            stdout: BufReader::new(stdout),
        })
    }
}

/// Blob contents by object ID, one request at a time (`cat-file --batch` flushes after each
/// object).
pub struct BlobReader {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl BlobReader {
    pub fn read(&mut self, object: &str) -> Result<Vec<u8>, String> {
        let stdin = self.stdin.as_mut().ok_or("git cat-file is closed")?;
        writeln!(stdin, "{object}")
            .and_then(|()| stdin.flush())
            .map_err(|e| format!("cannot ask git cat-file for {object}: {e}"))?;
        let mut header = String::new();
        self.stdout
            .read_line(&mut header)
            .map_err(|e| format!("cannot read git cat-file's answer for {object}: {e}"))?;
        let fields: Vec<&str> = header.split_whitespace().collect();
        let [_, kind, size] = fields.as_slice() else {
            return Err(format!("git cat-file answered {object} with {header:?}"));
        };
        if *kind != "blob" {
            return Err(format!("{object} is a {kind}, not a blob"));
        }
        let size: usize = size
            .parse()
            .map_err(|_| format!("git cat-file printed a size that is not a number: {size}"))?;
        let mut content = vec![0; size + 1];
        self.stdout
            .read_exact(&mut content)
            .map_err(|e| format!("cannot read blob {object}: {e}"))?;
        content.pop();
        Ok(content)
    }
}

impl Drop for BlobReader {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.wait();
    }
}
