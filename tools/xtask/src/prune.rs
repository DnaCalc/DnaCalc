use crate::cargo;
use std::collections::BTreeMap;
use std::fs::{self, File, Metadata, OpenOptions, TryLockError};
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime};

const USAGE: &str = "usage: cargo xtask prune [--dry]";
const MIN_AGE: Duration = Duration::from_secs(2 * 60 * 60);

#[derive(Default)]
struct Report {
    files: usize,
    incremental_dirs: usize,
    bytes: u64,
    busy: usize,
}

impl Report {
    fn print(&self, dry: bool) {
        let verb = if dry { "would remove" } else { "removed" };
        println!(
            "PRUNE {verb} {} executables and {} incremental directories ({} artifact bytes); {} busy profiles skipped",
            self.files, self.incremental_dirs, self.bytes, self.busy
        );
    }
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    metadata.file_type().is_symlink()
}

fn metadata(path: &Path) -> io::Result<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !is_link(&metadata) => Ok(Some(metadata)),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn directories(path: &Path) -> io::Result<Vec<PathBuf>> {
    if !metadata(path)?.is_some_and(|m| m.is_dir()) {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if metadata(&path)?.is_some_and(|m| m.is_dir()) {
            result.push(path);
        }
    }
    Ok(result)
}

fn executable_stem<'a>(name: &'a str, metadata: &Metadata) -> Option<&'a str> {
    if !metadata.is_file() {
        return None;
    }
    #[cfg(windows)]
    return name.strip_suffix(".exe");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        (metadata.permissions().mode() & 0o111 != 0)
            .then_some(name.strip_suffix(".exe").unwrap_or(name))
    }
    #[cfg(not(any(windows, unix)))]
    None
}

fn artifact_group<'a>(name: &'a str, metadata: &Metadata, incremental: bool) -> Option<&'a str> {
    let stem = if incremental {
        if !metadata.is_dir() {
            return None;
        }
        name
    } else {
        executable_stem(name, metadata)?
    };
    let (group, hash) = stem.rsplit_once('-')?;
    let valid_hash = if incremental {
        hash.len() == 13
            && hash
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    } else {
        hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit())
    };
    (!group.is_empty() && valid_hash).then_some(group)
}

/// Cargo can use the same test target name in different packages. The first input in
/// its dep-info is the crate root, which distinguishes those test binaries.
fn executable_source(path: &Path) -> Option<String> {
    let dep_info = path.with_extension("d");
    if !metadata(&dep_info).ok().flatten()?.is_file() {
        return None;
    }
    let file = File::open(dep_info).ok()?;
    let mut line = String::new();
    BufReader::new(file).read_line(&mut line).ok()?;
    let (_, inputs) = line.split_once(": ")?;
    let mut escaped = false;
    let end = inputs
        .char_indices()
        .find_map(|(index, ch)| {
            let split = ch.is_whitespace() && !escaped;
            escaped = ch == '\\' && !escaped;
            split.then_some(index)
        })
        .unwrap_or(inputs.len());
    Some(inputs[..end].to_owned())
}

fn directory_bytes(path: &Path) -> io::Result<u64> {
    let mut bytes = 0;
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if let Some(metadata) = metadata(&path)? {
            if metadata.is_dir() {
                bytes += directory_bytes(&path)?;
            } else if metadata.is_file() {
                bytes += metadata.len();
            }
        }
    }
    Ok(bytes)
}

fn prune_artifacts(
    dir: &Path,
    incremental: bool,
    cutoff: SystemTime,
    dry: bool,
    report: &mut Report,
) -> io::Result<()> {
    if !metadata(dir)?.is_some_and(|m| m.is_dir()) {
        return Ok(());
    }
    let mut groups = BTreeMap::<String, Vec<(SystemTime, PathBuf, u64)>>::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if let Some(metadata) = metadata(&path)?
            && let Some(group) = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| artifact_group(n, &metadata, incremental))
        {
            groups.entry(group.to_owned()).or_default().push((
                metadata.modified()?,
                path,
                metadata.len(),
            ));
        }
    }
    let mut variants = Vec::new();
    for group in groups
        .into_values()
        .filter(|group| group.len() > 1 && group.iter().any(|entry| entry.0 < cutoff))
    {
        if incremental {
            variants.push(group);
        } else {
            let mut sources = BTreeMap::<_, Vec<_>>::new();
            for artifact in group {
                sources
                    .entry(executable_source(&artifact.1))
                    .or_default()
                    .push(artifact);
            }
            variants.extend(sources.into_values());
        }
    }
    for mut group in variants {
        group.sort_unstable();
        group.pop(); // Keep the newest variant even when it is older than the cutoff.
        for (_, path, len) in group.iter().take_while(|entry| entry.0 < cutoff) {
            if incremental {
                let bytes = directory_bytes(path)?;
                if !dry {
                    fs::remove_dir_all(path)?;
                }
                report.bytes += bytes;
                report.incremental_dirs += 1;
            } else {
                if !dry {
                    fs::remove_file(path)?;
                }
                report.bytes += len;
                report.files += 1;
            }
        }
    }
    Ok(())
}

struct ProfileLock(File);

impl Drop for ProfileLock {
    fn drop(&mut self) {
        // A concurrent fork can briefly retain the file descriptor. Release the lock
        // explicitly so it ends with this pruning pass even if a copy is still open.
        let _ = self.0.unlock();
    }
}

fn prune_profile(
    path: &Path,
    cutoff: SystemTime,
    dry: bool,
    report: &mut Report,
) -> io::Result<bool> {
    // Cargo locks the intermediate directory separately from the final output directory.
    // When both locks are here, also exclude older Cargo versions using .cargo-lock.
    let mut locks = Vec::new();
    for name in [".cargo-build-lock", ".cargo-lock"] {
        let lock_path = path.join(name);
        if !metadata(&lock_path)?.is_some_and(|m| m.is_file()) {
            continue;
        }
        let lock = OpenOptions::new().read(true).write(true).open(lock_path)?;
        match lock.try_lock() {
            Ok(()) => locks.push(ProfileLock(lock)),
            Err(TryLockError::WouldBlock) => {
                report.busy += 1;
                return Ok(true);
            }
            Err(TryLockError::Error(error)) => return Err(error),
        }
    }
    if locks.is_empty() {
        return Ok(false);
    }
    prune_artifacts(&path.join("deps"), false, cutoff, dry, report)?;
    prune_artifacts(&path.join("incremental"), true, cutoff, dry, report)?;
    Ok(true)
}

fn prune(target: &Path, now: SystemTime, dry: bool) -> io::Result<Report> {
    let cutoff = now - MIN_AGE;
    let mut report = Report::default();
    for dir in directories(target)? {
        // Explicit target triples and the xtask launcher hold profiles one level deeper.
        if !prune_profile(&dir, cutoff, dry, &mut report)? {
            for profile in directories(&dir)? {
                prune_profile(&profile, cutoff, dry, &mut report)?;
            }
        }
    }
    Ok(report)
}

fn prune_cargo_build_dir(cwd: &Path, dry: bool) -> Result<Report, String> {
    let build_dir = cargo::build_dir(cwd)?;
    prune(&build_dir, SystemTime::now(), dry)
        .map_err(|error| format!("could not prune {}: {error}", build_dir.display()))
}

pub fn after_check(workspace_root: &Path) {
    match prune_cargo_build_dir(workspace_root, false) {
        Ok(report) => report.print(false),
        Err(error) => eprintln!("CHECK prune warning: {error}"),
    }
}

pub fn run(args: &[String]) -> ExitCode {
    let dry = match args {
        [] => false,
        [flag] if flag == "--dry" => true,
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let result = std::env::current_dir()
        .map_err(|error| error.to_string())
        .and_then(|cwd| prune_cargo_build_dir(&cwd, dry));
    match result {
        Ok(report) => {
            report.print(dry);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("xtask prune: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::FileTimes;

    #[test]
    fn profile_lock_releases_while_a_duplicate_handle_is_open() {
        let path = std::env::temp_dir().join(format!("xtask-prune-lock-{}", std::process::id()));
        let file = File::create_new(&path).unwrap();
        file.try_lock().unwrap();
        let lock = ProfileLock(file);
        let duplicate = lock.0.try_clone().unwrap();
        let contender = OpenOptions::new().write(true).open(&path).unwrap();

        assert!(matches!(contender.try_lock(), Err(TryLockError::WouldBlock)));
        drop(lock);
        let reacquired = contender.try_lock();

        drop(contender);
        drop(duplicate);
        fs::remove_file(path).unwrap();
        reacquired.expect("the profile lock is released when pruning leaves the profile");
    }

    fn executable(deps: &Path, stem: &str, stamp: SystemTime) -> PathBuf {
        let path = deps.join(format!("{stem}{}", std::env::consts::EXE_SUFFIX));
        fs::write(&path, b"binary").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(FileTimes::new().set_modified(stamp))
            .unwrap();
        path
    }

    #[test]
    fn prunes_only_old_duplicates_in_unlocked_cargo_profiles() {
        let root = std::env::temp_dir().join(format!("xtask-prune-{}", std::process::id()));
        let profile = root.join("x86_64-unknown-linux-gnu/debug");
        let busy = root.join("debug");
        let deps = profile.join("deps");
        let incremental = profile.join("incremental");
        let busy_deps = busy.join("deps");
        for dir in [&deps, &incremental, &busy_deps] {
            fs::create_dir_all(dir).unwrap();
        }
        File::create(profile.join(".cargo-lock")).unwrap();
        File::create(busy.join(".cargo-lock")).unwrap();
        let busy_lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(busy.join(".cargo-build-lock"))
            .unwrap();
        busy_lock.try_lock().unwrap();

        let now = SystemTime::now() + MIN_AGE + Duration::from_secs(60);
        let old = now - MIN_AGE - Duration::from_secs(60);
        let newer = old + Duration::from_secs(30);
        let stale = executable(&deps, "tools-0000000000000001", old);
        let newest_old = executable(&deps, "tools-0000000000000002", newer);
        let recent = executable(
            &deps,
            "suite-0000000000000001",
            now - Duration::from_secs(3600),
        );
        let newest = executable(
            &deps,
            "suite-0000000000000002",
            now + Duration::from_secs(1),
        );
        let only = executable(&deps, "only-0000000000000001", old);
        let busy_stale = executable(&busy_deps, "busy-0000000000000001", old);
        let busy_newest = executable(&busy_deps, "busy-0000000000000002", newer);
        let other_package = executable(&deps, "tools-0000000000000003", old);
        for (path, root) in [
            (&stale, "one/test.rs"),
            (&newest_old, "one/test.rs"),
            (&other_package, "other/test.rs"),
        ] {
            fs::write(path.with_extension("d"), format!("out: {root} helper.rs\n")).unwrap();
        }
        let companion = stale.with_extension("d");

        let stale_incremental = incremental.join("crate_name-0000000000001");
        let newest_incremental = incremental.join("crate_name-0000000000002");
        for path in [&stale_incremental, &newest_incremental] {
            fs::create_dir(path).unwrap();
            fs::write(path.join("object"), b"object code").unwrap();
        }

        let dry = prune(&root, now, true).unwrap();
        assert_eq!(
            (dry.files, dry.incremental_dirs, dry.bytes, dry.busy),
            (1, 1, 17, 1)
        );
        assert!(stale.exists() && stale_incremental.exists());
        let report = prune(&root, now, false).unwrap();
        assert_eq!(
            (
                report.files,
                report.incremental_dirs,
                report.bytes,
                report.busy
            ),
            (1, 1, 17, 1)
        );
        assert!(!stale.exists() && !stale_incremental.exists());
        for kept in [
            newest_old,
            recent,
            newest,
            only,
            companion,
            other_package,
            newest_incremental,
            busy_stale,
            busy_newest,
        ] {
            assert!(kept.exists(), "removed {}", kept.display());
        }
        drop(busy_lock);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prunes_the_cargo_configured_build_directory() {
        // A caller's explicit override takes precedence over this fixture's Cargo config.
        if std::env::var_os("CARGO_BUILD_BUILD_DIR").is_some() {
            return;
        }
        let root = std::env::temp_dir().join(format!("xtask-prune-config-{}", std::process::id()));
        let profile = root.join("intermediate/debug");
        let deps = profile.join("deps");
        for dir in [root.join(".cargo"), root.join("src"), deps.clone()] {
            fs::create_dir_all(dir).unwrap();
        }
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\n[workspace]\n",
        )
        .unwrap();
        fs::write(root.join("src/lib.rs"), "").unwrap();
        fs::write(
            root.join(".cargo/config.toml"),
            "[build]\ntarget-dir = \"output\"\nbuild-dir = \"intermediate\"\n",
        )
        .unwrap();
        File::create(profile.join(".cargo-build-lock")).unwrap();
        let now = SystemTime::now();
        let stale = executable(
            &deps,
            "fixture-0000000000000001",
            now - MIN_AGE - Duration::from_secs(60),
        );
        let newest = executable(&deps, "fixture-0000000000000002", now);

        let build_dir = cargo::build_dir(&root).unwrap();
        assert_eq!(
            fs::canonicalize(&build_dir).unwrap(),
            fs::canonicalize(root.join("intermediate")).unwrap()
        );
        let report = prune(&build_dir, now, false).unwrap();
        assert_eq!(report.files, 1);
        assert!(!stale.exists() && newest.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
