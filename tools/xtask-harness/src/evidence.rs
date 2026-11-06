//! `cargo xtask evidence verify [--all | --sample N] [--seed S]`, `cargo xtask evidence restore
//! <dir> [--sample N] [--seed S]` and `cargo xtask evidence labels [--write]`: the repository
//! commands over the backup of OxFunc's smart-fuzzer evidence in the evidence repository
//! `CalcFunc-Evidence` (bead `infra-m0.6`).
//!
//! `verify` and `restore` run that repository's own `archive-evidence.py`, which is the one
//! implementation of both; nothing here reads a blob. The evidence checkout is
//! `<legacy root>/CalcFunc-Evidence`, where the legacy root is the workspace root's parent or
//! `DNACALC_LEGACY_ROOT`. The script runs with `python` on Windows and `python3` elsewhere, with
//! `--dest` naming the checkout and no `--source`: neither subcommand reads the originals, so a
//! restore needs only the checkout. Before the script runs, the command prints the commit that
//! `migration/evidence/oxfunc-smart-fuzzer.json` pins, the checkout's HEAD and whether its tree is
//! clean, with a warning line for each difference; the command still exits with the script's exit
//! code, which it also prints.
//!
//! `labels` recomputes the record's acquisition labels (`evidence_labels.rs`).

use crate::cargo::Workspace;
use crate::evidence_labels;
use crate::legacy::Legacy;
use crate::legacy::{LEGACY_ROOT_VARIABLE, legacy_root};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

pub const USAGE: &str = "usage: cargo xtask evidence verify [--all | --sample N] [--seed S]\n       cargo xtask evidence restore <dir> [--sample N] [--seed S]\n       cargo xtask evidence labels [--write]";

const EXIT_USAGE: u8 = 2;

/// The evidence repository's directory under the legacy root.
pub const CHECKOUT: &str = "CalcFunc-Evidence";

/// The script in the evidence repository that archives, verifies and restores.
pub const SCRIPT: &str = "archive-evidence.py";

#[derive(Debug, PartialEq, Eq)]
enum Request {
    /// A run of the evidence repository's script.
    Script(Script),
    /// Recompute the record's labels, and write them into the record with `--write`.
    Labels { write: bool },
}

#[derive(Debug, PartialEq, Eq)]
enum Script {
    /// Decompress all blobs, or a random manifest sample, and compare them with the manifest.
    Verify {
        all: bool,
        sample: Option<u64>,
        seed: Option<u64>,
    },
    /// Decompress all blobs, or a random manifest sample, into an empty directory.
    Restore {
        dir: PathBuf,
        sample: Option<u64>,
        seed: Option<u64>,
    },
}

fn number(name: &str, value: Option<&String>) -> Result<u64, String> {
    let value = value.ok_or_else(|| format!("{name} needs a value"))?;
    value
        .parse::<u64>()
        .map_err(|_| format!("{name} needs a non-negative integer, not {value:?}"))
}

fn parse_args(args: &[String]) -> Result<Request, String> {
    let (command, rest) = args
        .split_first()
        .ok_or("missing subcommand: verify, restore or labels")?;
    if command == "labels" {
        return match rest {
            [] => Ok(Request::Labels { write: false }),
            [flag] if flag == "--write" => Ok(Request::Labels { write: true }),
            _ => Err(format!("labels takes only --write, not {rest:?}")),
        };
    }
    let mut all = false;
    let mut sample = None;
    let mut seed = None;
    let mut dir = None;
    let mut items = rest.iter();
    while let Some(arg) = items.next() {
        match arg.as_str() {
            "--all" if command == "verify" => all = true,
            "--sample" => sample = Some(number("--sample", items.next())?),
            "--seed" => seed = Some(number("--seed", items.next())?),
            other if command == "restore" && !other.starts_with('-') && dir.is_none() => {
                dir = Some(PathBuf::from(other));
            }
            other => return Err(format!("unknown argument {other} for {command}")),
        }
    }
    if sample == Some(0) {
        return Err("--sample needs at least 1".to_owned());
    }
    match command.as_str() {
        "verify" => {
            if all && (sample.is_some() || seed.is_some()) {
                return Err("--all takes neither --sample nor --seed".to_owned());
            }
            Ok(Request::Script(Script::Verify { all, sample, seed }))
        }
        "restore" => {
            let dir = dir.ok_or("restore needs a target directory")?;
            if seed.is_some() && sample.is_none() {
                return Err("--seed needs --sample".to_owned());
            }
            Ok(Request::Script(Script::Restore { dir, sample, seed }))
        }
        other => Err(format!("unknown subcommand {other}")),
    }
}

/// The script's arguments after its path: the checkout as `--dest`, then the subcommand. There is
/// no `--source`: verify and restore read only the checkout. A relative restore directory is taken
/// relative to `cwd`.
fn script_args(script: &Script, dest: &Path, cwd: &Path) -> Vec<OsString> {
    let mut argv: Vec<OsString> = vec!["--dest".into(), dest.as_os_str().to_owned()];
    let (sample, seed) = match script {
        Script::Verify { all, sample, seed } => {
            argv.push("verify".into());
            if *all {
                argv.push("--all".into());
            }
            (sample, seed)
        }
        Script::Restore { dir, sample, seed } => {
            argv.push("restore".into());
            argv.push(cwd.join(dir).into_os_string());
            (sample, seed)
        }
    };
    if let Some(sample) = sample {
        argv.push("--sample".into());
        argv.push(sample.to_string().into());
    }
    if let Some(seed) = seed {
        argv.push("--seed".into());
        argv.push(seed.to_string().into());
    }
    argv
}

/// The Python interpreter the script runs with on this host.
fn python() -> &'static str {
    if cfg!(windows) { "python" } else { "python3" }
}

/// `path` with links resolved, so that git reads the checkout at its real place (a link to the
/// checkout is otherwise named in `safe.directory` under a path git does not compare); `path`
/// itself when it cannot be resolved. Windows' verbatim prefix is dropped for a drive path.
pub fn real_dir(path: &Path) -> PathBuf {
    let Ok(real) = std::fs::canonicalize(path) else {
        return path.to_path_buf();
    };
    let text = real.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => real,
    }
}

/// The commit the record pins.
fn pinned_commit(workspace_root: &Path) -> Result<String, String> {
    let record = evidence_labels::read_record(workspace_root)?;
    record["evidence_repository"]["commit"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| {
            format!(
                "{} has no evidence_repository.commit",
                evidence_labels::RECORD
            )
        })
}

/// The lines that compare the checkout with the record's pin: the pin, the HEAD and the tree
/// state, and a `WARNING` line for each difference or each value that could not be read.
fn checkout_lines(
    pin: &Result<String, String>,
    head: &Result<String, String>,
    status: &Result<Vec<String>, String>,
) -> Vec<String> {
    let mut lines = Vec::new();
    match pin {
        Ok(pin) => lines.push(format!(
            "xtask evidence: pinned commit {pin} ({})",
            evidence_labels::RECORD
        )),
        Err(e) => lines.push(format!("xtask evidence: WARNING: no pinned commit: {e}")),
    }
    match head {
        Ok(head) => lines.push(format!("xtask evidence: checkout HEAD {head}")),
        Err(e) => lines.push(format!(
            "xtask evidence: WARNING: cannot read the checkout's HEAD: {e}"
        )),
    }
    match status {
        Ok(changed) if changed.is_empty() => {
            lines.push("xtask evidence: checkout tree clean".to_owned());
        }
        Ok(changed) => {
            lines.push(format!(
                "xtask evidence: checkout tree not clean: {} changed paths, first {:?}",
                changed.len(),
                changed.iter().take(5).collect::<Vec<_>>()
            ));
            lines.push(format!(
                "xtask evidence: WARNING: the checkout has {} uncommitted changes, so its files may differ from the pinned commit",
                changed.len()
            ));
        }
        Err(e) => lines.push(format!(
            "xtask evidence: WARNING: cannot read the checkout's status: {e}"
        )),
    }
    if let (Ok(pin), Ok(head)) = (pin, head)
        && pin != head
    {
        lines.push(format!(
            "xtask evidence: WARNING: the checkout's HEAD {head} is not the pinned commit {pin}"
        ));
    }
    lines
}

pub fn run(raw: &[String]) -> ExitCode {
    let request = match parse_args(raw) {
        Ok(request) => request,
        Err(e) => {
            eprintln!("xtask evidence: {e}\n{USAGE}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("xtask evidence: no current directory: {e}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let ws = match Workspace::load(&cwd) {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("xtask evidence: cannot read the workspace: {e}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let root = legacy_root(&ws.root);
    let checkout = root.path.join(CHECKOUT);
    let script = match request {
        Request::Labels { write } => return evidence_labels::run(&ws.root, &root.path, write),
        Request::Script(script) => script,
    };
    let script_path = checkout.join(SCRIPT);
    if !script_path.is_file() {
        eprintln!(
            "xtask evidence: {} does not exist; the evidence checkout belongs at <legacy root>/{CHECKOUT}, where the legacy root is {}",
            script_path.display(),
            if root.from_variable {
                format!("the directory {LEGACY_ROOT_VARIABLE} names")
            } else {
                format!(
                    "the workspace root's parent (set {LEGACY_ROOT_VARIABLE} to choose another)"
                )
            }
        );
        return ExitCode::from(1);
    }
    let git = Legacy::new(real_dir(&checkout));
    for line in checkout_lines(
        &pinned_commit(&ws.root),
        &git.text(&["rev-parse", "HEAD"]),
        &git.status(),
    ) {
        println!("{line}");
    }
    let args = script_args(&script, &checkout, &cwd);
    let mut command = Command::new(python());
    command.arg(&script_path).args(&args);
    let shown: Vec<String> = std::iter::once(python().to_owned())
        .chain(std::iter::once(script_path.display().to_string()))
        .chain(args.iter().map(|arg| arg.to_string_lossy().into_owned()))
        .collect();
    println!("xtask evidence: running {shown:?}");
    let status = match command.status() {
        Ok(status) => status,
        Err(e) => {
            eprintln!("xtask evidence: cannot start {}: {e}", python());
            return ExitCode::from(1);
        }
    };
    let code = status.code();
    println!("xtask evidence: {SCRIPT} exited with {code:?}");
    match code {
        Some(code) => u8::try_from(code).map_or(ExitCode::from(1), ExitCode::from),
        None => ExitCode::from(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn verify_takes_all_or_a_sample_with_an_optional_seed() {
        assert_eq!(
            parse_args(&args("verify")),
            Ok(Request::Script(Script::Verify {
                all: false,
                sample: None,
                seed: None
            }))
        );
        assert_eq!(
            parse_args(&args("verify --all")),
            Ok(Request::Script(Script::Verify {
                all: true,
                sample: None,
                seed: None
            }))
        );
        assert_eq!(
            parse_args(&args("verify --sample 100 --seed 7")),
            Ok(Request::Script(Script::Verify {
                all: false,
                sample: Some(100),
                seed: Some(7)
            }))
        );
        assert!(parse_args(&args("verify --all --sample 5")).is_err());
        assert!(parse_args(&args("verify --all --seed 5")).is_err());
        assert!(parse_args(&args("verify --sample")).is_err());
        assert!(parse_args(&args("verify --sample -3")).is_err());
        assert!(parse_args(&args("verify --sample 0")).is_err());
        assert!(parse_args(&args("verify somewhere")).is_err());
    }

    #[test]
    fn restore_needs_one_directory_and_a_seed_only_with_a_sample() {
        assert_eq!(
            parse_args(&args("restore out --sample 100 --seed 7")),
            Ok(Request::Script(Script::Restore {
                dir: PathBuf::from("out"),
                sample: Some(100),
                seed: Some(7)
            }))
        );
        assert_eq!(
            parse_args(&args("restore out")),
            Ok(Request::Script(Script::Restore {
                dir: PathBuf::from("out"),
                sample: None,
                seed: None
            }))
        );
        assert!(parse_args(&args("restore")).is_err());
        assert!(parse_args(&args("restore out other")).is_err());
        assert!(parse_args(&args("restore out --seed 7")).is_err());
        assert!(parse_args(&args("restore out --all")).is_err());
        assert!(parse_args(&args("archive")).is_err());
        assert!(parse_args(&[]).is_err());
    }

    #[test]
    fn labels_takes_only_write() {
        assert_eq!(
            parse_args(&args("labels")),
            Ok(Request::Labels { write: false })
        );
        assert_eq!(
            parse_args(&args("labels --write")),
            Ok(Request::Labels { write: true })
        );
        assert!(parse_args(&args("labels --write --write")).is_err());
        assert!(parse_args(&args("labels --sample 5")).is_err());
        assert!(parse_args(&args("labels out")).is_err());
    }

    #[test]
    fn the_script_gets_the_checkout_no_source_and_an_absolute_restore_target() {
        let dest = Path::new("legacy").join(CHECKOUT);
        let cwd = Path::new("work");
        let shown = |script: &Script| -> Vec<String> {
            script_args(script, &dest, cwd)
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect()
        };
        let prefix = vec!["--dest".to_owned(), dest.display().to_string()];
        let restore = Script::Restore {
            dir: PathBuf::from("out"),
            sample: Some(100),
            seed: Some(7),
        };
        let mut expected = prefix.clone();
        expected.extend([
            "restore".to_owned(),
            cwd.join("out").display().to_string(),
            "--sample".to_owned(),
            "100".to_owned(),
            "--seed".to_owned(),
            "7".to_owned(),
        ]);
        assert_eq!(shown(&restore), expected);
        let verify = Script::Verify {
            all: true,
            sample: None,
            seed: None,
        };
        let mut expected = prefix;
        expected.extend(["verify".to_owned(), "--all".to_owned()]);
        assert_eq!(shown(&verify), expected);
        for script in [&restore, &verify] {
            assert!(!shown(script).iter().any(|arg| arg == "--source"));
        }
    }

    #[test]
    fn checkout_lines_warn_on_each_difference_from_the_pin() {
        let pin: Result<String, String> = Ok("a".repeat(40));
        let same: Result<String, String> = Ok("a".repeat(40));
        let other: Result<String, String> = Ok("b".repeat(40));
        let clean: Result<Vec<String>, String> = Ok(Vec::new());
        let dirty: Result<Vec<String>, String> = Ok(vec![" M README.md".to_owned()]);
        let warnings = |lines: &[String]| lines.iter().filter(|l| l.contains("WARNING:")).count();

        let lines = checkout_lines(&pin, &same, &clean);
        assert_eq!(warnings(&lines), 0, "{lines:?}");
        assert!(lines.contains(&format!("xtask evidence: checkout HEAD {}", "a".repeat(40))));
        assert!(lines.contains(&"xtask evidence: checkout tree clean".to_owned()));

        let lines = checkout_lines(&pin, &other, &clean);
        assert_eq!(warnings(&lines), 1, "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("WARNING")
            && l.contains(&"a".repeat(40))
            && l.contains(&"b".repeat(40))));

        let lines = checkout_lines(&pin, &same, &dirty);
        assert_eq!(warnings(&lines), 1, "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("1 changed paths")));

        let lines = checkout_lines(&pin, &other, &dirty);
        assert_eq!(warnings(&lines), 2, "{lines:?}");

        let lines = checkout_lines(
            &Err("no record".to_owned()),
            &Err("no git".to_owned()),
            &Err("no status".to_owned()),
        );
        assert_eq!(warnings(&lines), 3, "{lines:?}");
    }
}
