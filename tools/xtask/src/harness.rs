//! Build the linked harness once in the workspace target, then run it after cargo releases its
//! lock. The child inherits a concrete executable so acceptance subprocesses never start cargo
//! inside a target directory whose outer test run still owns the lock.
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use xtask::cargo::{self, Workspace};

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
    let mut build = Command::new(cargo::cargo_program());
    build
        .current_dir(&ws.root)
        .args([
            "build",
            "--locked",
            "-p",
            "xtask-harness",
            "--bin",
            "xtask-harness",
            "--message-format=json",
            "--target-dir",
        ])
        .arg(ws.accept_target_dir());
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
    built_executable(&stdout).ok_or_else(|| "cargo built no xtask-harness executable".to_owned())
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

fn command(exe: &Path, args: &[String]) -> Command {
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
            command(&exe, args)
                .status()
                .map_err(|e| format!("cannot run {}: {e}", exe.display()))
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
