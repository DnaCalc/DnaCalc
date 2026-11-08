//! Build the linked harness in the default workspace target, then run a private copy from
//! target/xtask/run after cargo releases its lock. The child inherits a concrete executable so
//! acceptance subprocesses never start cargo inside a target directory whose outer test run
//! still owns the lock.
use crate::cargo::{self, Workspace};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const HARNESS_ENV: &str = "DNACALC_XTASK_HARNESS";

fn executable(ws: &Workspace) -> Result<PathBuf, String> {
    if let Some(exe) = std::env::var_os(HARNESS_ENV) {
        let exe = PathBuf::from(exe);
        return if exe.is_file() {
            Ok(exe)
        } else {
            Err(format!(
                "{HARNESS_ENV} names no executable: {}",
                exe.display()
            ))
        };
    }
    profile_executable(ws, "dev")
}

/// Build into Cargo's standard profile directory and return a runnable copy. The caller owns
/// the copy's directory until its subprocesses exit; workspace builds may replace the original.
pub fn profile_executable(ws: &Workspace, profile: &str) -> Result<PathBuf, String> {
    let mut build = Command::new(cargo::cargo_program());
    build.current_dir(&ws.root).args([
        "build",
        "--locked",
        "-p",
        "xtask-harness",
        "--bin",
        "xtask-harness",
        "--message-format=json",
        "--profile",
        profile,
    ]);
    let out = cargo::output(&mut build)?;
    eprint!("{}", cargo::text(&out.stderr));
    let stdout = cargo::text(&out.stdout);
    for message in stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
    {
        if message["reason"] == "compiler-message"
            && let Some(rendered) = message["message"]["rendered"].as_str()
        {
            eprint!("{rendered}");
        }
    }
    if !out.status.success() {
        return Err(format!("building xtask-harness exited with {}", out.status));
    }
    let built = built_executable(&stdout)
        .ok_or_else(|| "cargo built no xtask-harness executable".to_owned())?;
    runnable_copy(ws, &built, profile)
}

/// Windows cannot replace a running executable. Each launcher runs its own copy, leaving the
/// Cargo artifact available for a workspace build, including commands nested inside the harness.
fn runnable_copy(ws: &Workspace, built: &Path, profile: &str) -> Result<PathBuf, String> {
    let dir = ws
        .root
        .join("target/xtask/run")
        .join(format!("{}-{profile}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let copy = dir.join(
        built
            .file_name()
            .ok_or("the harness executable has no file name")?,
    );
    std::fs::copy(built, &copy)
        .map_err(|e| format!("cannot copy {} to {}: {e}", built.display(), copy.display()))?;
    Ok(copy)
}

/// Cargo reports the actual executable, including any configured target triple and suffix.
fn built_executable(output: &str) -> Option<PathBuf> {
    output
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find_map(|artifact| {
            if artifact["reason"] == "compiler-artifact"
                && artifact["target"]["name"] == "xtask-harness"
            {
                artifact["executable"].as_str().map(PathBuf::from)
            } else {
                None
            }
        })
}

/// Removes an owned profile copy after all its subprocesses have exited.
pub struct CopyCleanup(pub PathBuf);

impl Drop for CopyCleanup {
    fn drop(&mut self) {
        if let Some(dir) = self.0.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

/// Run a prebuilt harness and preserve it for nested commands.
pub fn command(exe: &Path, args: &[String]) -> Command {
    let mut command = Command::new(exe);
    command.args(args).env(HARNESS_ENV, exe);
    command
}

pub fn run(args: &[String]) -> ExitCode {
    let result = std::env::current_dir()
        .map_err(|e| format!("no current directory: {e}"))
        .and_then(|cwd| Workspace::load(&cwd))
        .and_then(|ws| executable(&ws))
        .and_then(|exe| {
            let result = command(&exe, args)
                .status()
                .map_err(|e| format!("cannot run {}: {e}", exe.display()));
            // A nested launcher reuses the parent's copy; only its owner removes it, after exit.
            if std::env::var_os(HARNESS_ENV).is_none() {
                let _ = std::fs::remove_dir_all(exe.parent().expect("the run directory"));
            }
            result
        });
    match result {
        Ok(status) => ExitCode::from(status.code().unwrap_or(1).try_into().unwrap_or(1)),
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cargo_artifacts_name_the_configured_target_executable() {
        let output = r#"{"reason":"compiler-artifact","target":{"name":"xtask"},"executable":"wrong"}
{"reason":"compiler-artifact","target":{"name":"xtask-harness"},"executable":null}
{"reason":"compiler-artifact","target":{"name":"xtask-harness"},"executable":"C:\\build with spaces\\x86_64-pc-windows-msvc\\debug\\xtask-harness.exe"}
{"reason":"build-finished","success":true}"#;
        assert_eq!(
            built_executable(output),
            Some(PathBuf::from(
                r"C:\build with spaces\x86_64-pc-windows-msvc\debug\xtask-harness.exe"
            ))
        );
        assert_eq!(built_executable("not a cargo artifact"), None);
        let root = std::env::temp_dir().join(format!("xtask-copy-{}", std::process::id()));
        let ws = Workspace {
            root: root.clone(),
            members: Vec::new(),
        };
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join(format!("xtask-harness{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&source, b"first build").unwrap();
        let copy = runnable_copy(&ws, &source, "dev").unwrap();
        assert!(copy.starts_with(root.join("target/xtask/run")));
        std::fs::write(&source, b"next build").unwrap();
        assert_eq!(std::fs::read(copy).unwrap(), b"first build");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_child_gets_literal_arguments_and_the_prebuilt_harness_for_nested_commands() {
        let args = vec![
            "app".into(),
            "smoke".into(),
            "calc".into(),
            "--case".into(),
            "a block's entry".into(),
        ];
        let exe = Path::new("/some path/xtask-harness");
        let child = command(exe, &args);
        assert_eq!(
            child.get_args().collect::<Vec<_>>(),
            args.iter().map(std::ffi::OsStr::new).collect::<Vec<_>>()
        );
        assert!(
            child
                .get_envs()
                .any(|(name, value)| name == HARNESS_ENV && value == Some(exe.as_os_str()))
        );
    }
}
