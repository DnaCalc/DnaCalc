//! The bodies of the infra-m0.6 acceptance cases (the `#[test]` functions sit at the root of
//! `tests/acceptance.rs`, so libtest lists them under their registered names).
//!
//! Both cases read the record `migration/evidence/oxfunc-smart-fuzzer.json`, the evidence checkout
//! `<legacy root>/OxFunc-Evidence` and the originals `<legacy root>/OxFunc/smart-fuzzer`, where the
//! legacy root is the workspace root's parent or `DNACALC_LEGACY_ROOT`. Nothing here trusts what
//! the record, `cargo xtask evidence labels` or the archive script reports about itself: the
//! pinned tree, the manifest's bytes, the remote's master and the local inventory come from this
//! module's own git calls and directory walk; the manifest's SHA-256 comes from Python's hashlib
//! over the bytes git prints; a restored sample is compared byte for byte with the originals; and
//! the path-only folders, every acquisition label, the unread metadata files and the entries with
//! a build key outside the permitted fields are recomputed here, by code of this module, from the
//! inventory paths, `migration/numeric-provenance.json` and the originals' metadata.
//!
//! The restore runs through the xtask binary this test build compiled (`CARGO_BIN_EXE_xtask`),
//! which `cargo xtask` runs, with `DNACALC_LEGACY_ROOT` naming a temporary directory under
//! `CARGO_TARGET_TMPDIR` that holds only a link `OxFunc-Evidence` to the evidence checkout (a
//! directory junction on Windows, a symbolic link elsewhere) and no `OxFunc`: the command finds
//! the checkout through the link, and the script's command line it prints holds no `--source`.
//! The script's restore reads only the archived blobs; its unused default source still resolves
//! beside the link. The link is removed afterwards, the link first and then the empty
//! directory, never by a recursive delete.
//!
//! The labels of a path-only run folder come from its paths alone, and labelling opens none of its
//! files (the infra-m0.5 reading rule). The restore sample is drawn from every manifest path, so it
//! may compare the bytes of such a file, which reads nothing of it for behaviour. Elsewhere only the permitted top-level metadata fields of JSON
//! files are read, and a text search for a build key, which reads no value but the digits after
//! the key. Every git call in the evidence checkout reads; the status read adds the `-c` settings
//! infra-m0.4 uses, so that a Windows host and a WSL host reading the same checkout through
//! `/mnt/c` measure the same thing. A host without the checkout or the originals prints
//! `ACCEPT-NOT-RUN`, and so does a host that cannot read GitHub's master. Evidence goes to
//! `target/accept/infra-m0.6/<case>/`.

use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const BEAD: &str = "infra-m0.6";
const RECORD: &str = "migration/evidence/oxfunc-smart-fuzzer.json";
const PROVENANCE: &str = "migration/numeric-provenance.json";
const LEGACY_ROOT_VARIABLE: &str = "DNACALC_LEGACY_ROOT";
const CHECKOUT: &str = "OxFunc-Evidence";
const URL: &str = "https://github.com/DnaCalc/OxFunc-Evidence";

/// The two archived directories under the originals.
const ROOTS: [&str; 2] = ["cache", "runs"];

/// The smallest restore sample the case accepts.
const SAMPLE: usize = 100;

/// How long `git ls-remote` may take before the remote counts as unreachable.
const REMOTE_DEADLINE: Duration = Duration::from_secs(120);

/// The `-c` settings added to `status`, as infra-m0.4 adds them.
const STATUS_CONFIG: [&str; 4] = [
    "core.excludesFile=",
    "core.autocrlf=true",
    "core.checkStat=minimal",
    "core.trustctime=false",
];

/// The run-folder name prefixes of the W108/W109 identification runs.
const IDENTIFICATION_RUNS: [&str; 2] = ["w108", "w109"];

/// The top-level objects of a metadata document whose `build` and `excel_build` are read.
const HOLDERS: [&str; 4] = [
    "excel_environment",
    "environment",
    "version_axes",
    "profile",
];

/// The objects one level further down whose `build` and `excel_build` are read, besides each
/// object of the top-level array `environments`.
const INNER_HOLDERS: [(&str, &str); 2] = [
    ("capture_provenance", "environment"),
    ("environment", "excel"),
];

/// The name of the temporary legacy root under `CARGO_TARGET_TMPDIR`.
const LINKED_ROOT: &str = "infra_m0_6-legacy-root";

fn repo_root() -> PathBuf {
    super::repo_root()
}

/// Checks that `case` is registered under infra-m0.6 for windows and linux, and that the bead
/// registers exactly its two Acceptance commands and the unit tests of `cargo xtask evidence`.
fn registered(case: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("acceptance.json");
    let manifest = super::read_json(&path);
    let entry = manifest["beads"]
        .as_array()
        .and_then(|beads| beads.iter().find(|bead| bead["id"] == BEAD))
        .unwrap_or_else(|| panic!("{} has no entry for {BEAD}", path.display()));
    let cases = super::strings(&entry["cases"], "registered cases");
    assert!(
        cases.iter().any(|registered| registered == case),
        "{case} is not registered under {BEAD} in {}; registered: {cases:?}",
        path.display()
    );
    let platforms = super::strings(&entry["platforms"][case], "the case's platforms");
    assert_eq!(
        platforms,
        ["windows", "linux"],
        "{case} must declare windows and linux"
    );
    let commands: Vec<Vec<String>> = entry["commands"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|command| super::strings(command, "a registered command"))
        .collect();
    let argv = |text: &str| -> Vec<String> { text.split(' ').map(str::to_owned).collect() };
    assert_eq!(
        commands,
        vec![
            argv("cargo xtask accept --id infra-m0.6 --platform windows --require-all"),
            argv("cargo xtask accept --id infra-m0.6 --platform linux --require-all"),
            argv("cargo test -p xtask -p xtask-harness --lib --bins"),
        ],
        "{BEAD} must register exactly its two Acceptance commands and the evidence unit tests"
    );
}

struct Evidence {
    case: &'static str,
    dir: PathBuf,
    lines: Vec<String>,
    failures: Vec<String>,
}

impl Evidence {
    fn new(case: &'static str) -> Evidence {
        let dir = repo_root()
            .join("target")
            .join("accept")
            .join(BEAD)
            .join(case);
        super::remove_tree(&dir).unwrap_or_else(|e| panic!("{e}"));
        fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("cannot create {}: {e}", dir.display()));
        Evidence {
            case,
            dir,
            lines: Vec::new(),
            failures: Vec::new(),
        }
    }

    fn line(&mut self, text: String) {
        self.lines.push(text);
    }

    fn expect(&mut self, holds: bool, what: String) {
        if holds {
            self.lines.push(format!("ok: {what}"));
        } else {
            self.failures.push(what);
        }
    }

    fn note(&mut self, text: String) {
        println!("ACCEPT-NOTE: {}: {text}", self.case);
        self.lines.push(format!("note: {text}"));
    }

    fn not_run(mut self, reason: String) {
        println!("ACCEPT-NOT-RUN: {}: {reason}", self.case);
        self.lines.push(format!("not run: {reason}"));
        self.finish();
    }

    fn write(&self, name: &str, contents: impl AsRef<[u8]>) {
        super::write(&self.dir.join(name), contents);
    }

    fn finish(self) {
        super::finish(self.case, &self.dir, &self.lines, &self.failures);
    }
}

/// The legacy root, as `cargo xtask evidence` chooses it, with how it was chosen. The path is shown
/// only as `..` or by the variable's name, never as an absolute host path.
fn legacy_root() -> (PathBuf, String) {
    match std::env::var_os(LEGACY_ROOT_VARIABLE).filter(|v| !v.is_empty()) {
        Some(value) => (
            PathBuf::from(value),
            format!("the directory {LEGACY_ROOT_VARIABLE} names"),
        ),
        None => (
            repo_root()
                .parent()
                .expect("the workspace root has a parent")
                .to_path_buf(),
            "the workspace root's parent (..)".to_owned(),
        ),
    }
}

/// Where the case's inputs are, or why this host cannot run it.
struct Inputs {
    checkout: PathBuf,
    originals: PathBuf,
    chosen: String,
}

fn inputs() -> Result<Inputs, String> {
    let (root, chosen) = legacy_root();
    let checkout = root.join(CHECKOUT);
    let originals = root.join("OxFunc").join("smart-fuzzer");
    if !checkout.join(".git").exists() {
        return Err(format!(
            "the evidence checkout <legacy root>/{CHECKOUT} does not exist on this host (legacy root: {chosen})"
        ));
    }
    for name in ROOTS {
        if !originals.join(name).is_dir() {
            return Err(format!(
                "the originals <legacy root>/OxFunc/smart-fuzzer/{name} do not exist on this host (legacy root: {chosen})"
            ));
        }
    }
    Ok(Inputs {
        checkout,
        originals,
        chosen,
    })
}

fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// A read-only git command in the evidence checkout.
fn checkout_git(dir: &Path, config: &[&str], args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command
        .arg("--no-optional-locks")
        .arg("-c")
        .arg(format!("safe.directory={}", slashed(dir)));
    for setting in config {
        command.arg("-c").arg(setting);
    }
    command.arg("-C").arg(dir).args(args);
    command
}

fn git_bytes(dir: &Path, config: &[&str], args: &[&str]) -> Result<Vec<u8>, String> {
    let output = checkout_git(dir, config, args)
        .output()
        .map_err(|e| format!("cannot run git {args:?}: {e}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(format!(
            "git {args:?} exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn git_text(dir: &Path, config: &[&str], args: &[&str]) -> Result<String, String> {
    git_bytes(dir, config, args).map(|out| String::from_utf8_lossy(&out).trim().to_owned())
}

/// The Python interpreter `cargo xtask evidence` uses on this host.
fn python() -> &'static str {
    if cfg!(windows) { "python" } else { "python3" }
}

/// The SHA-256 of `bytes`, from Python's hashlib (no hashing crate here).
fn sha256(bytes: &[u8]) -> Result<String, String> {
    python_over(
        "import hashlib, sys; print(hashlib.sha256(sys.stdin.buffer.read()).hexdigest())",
        &[],
        bytes,
    )
}

/// The paths `random.Random(seed).sample` draws from the sorted `paths`, sorted: the selection
/// rule archive-evidence.py states, computed here by Python's own `random` module rather than by
/// the script.
fn drawn_sample(
    paths: &BTreeSet<&String>,
    size: usize,
    seed: u64,
) -> Result<BTreeSet<String>, String> {
    let input: String = paths.iter().map(|path| format!("{path}\n")).collect();
    let drawn = python_over(
        "import random, sys; paths = sorted(p for p in sys.stdin.buffer.read().decode('utf-8').split('\\n') if p); \
         picked = sorted(random.Random(int(sys.argv[2])).sample(paths, min(int(sys.argv[1]), len(paths)))); \
         sys.stdout.buffer.write(''.join(p + '\\n' for p in picked).encode('utf-8'))",
        &[&size.to_string(), &seed.to_string()],
        input.as_bytes(),
    )?;
    Ok(drawn.lines().map(str::to_owned).collect())
}

/// What a Python one-liner prints for `input` on its standard input, trimmed.
fn python_over(code: &str, args: &[&str], input: &[u8]) -> Result<String, String> {
    let mut child = Command::new(python())
        .args(["-I", "-c", code])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", python()))?;
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(input)
        .map_err(|e| format!("cannot write to {}: {e}", python()))?;
    let output = child
        .wait_with_output()
        .map_err(|e| format!("{} did not finish: {e}", python()))?;
    if !output.status.success() {
        return Err(format!(
            "{} exited with {:?}: {}",
            python(),
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// One table of the evidence repository at the pinned commit: its header and, per path, the
/// columns after the path.
struct Table {
    header: Vec<String>,
    rows: BTreeMap<String, Vec<String>>,
    bytes: Vec<u8>,
}

fn pinned_table(checkout: &Path, pin: &str, name: &str) -> Result<Table, String> {
    let bytes = git_bytes(
        checkout,
        &[],
        &["cat-file", "blob", &format!("{pin}:{name}")],
    )?;
    let text = String::from_utf8(bytes.clone()).map_err(|e| format!("{name} is not UTF-8: {e}"))?;
    let mut lines = text.lines();
    let header: Vec<String> = lines
        .next()
        .ok_or(format!("{name} is empty"))?
        .split('\t')
        .map(str::to_owned)
        .collect();
    let mut rows = BTreeMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let mut cells = line.split('\t').map(str::to_owned);
        let path = cells.next().unwrap_or_default();
        let rest: Vec<String> = cells.collect();
        if rest.len() + 1 != header.len() {
            return Err(format!(
                "{name} row {path:?} has {} columns",
                rest.len() + 1
            ));
        }
        if rows.insert(path.clone(), rest).is_some() {
            return Err(format!("{name} lists {path:?} twice"));
        }
    }
    Ok(Table {
        header,
        rows,
        bytes,
    })
}

fn size_of(row: &[String], column: usize, what: &str) -> u64 {
    row[column]
        .parse()
        .unwrap_or_else(|_| panic!("{what} is not a byte count: {:?}", row[column]))
}

/// The inventory the pinned manifest and the excluded list give: every path with its raw size.
fn pinned_inventory(manifest: &Table, excluded: &Table) -> BTreeMap<String, u64> {
    manifest
        .rows
        .iter()
        .map(|(path, row)| (path.clone(), size_of(row, 0, path)))
        .chain(
            excluded
                .rows
                .iter()
                .map(|(path, row)| (path.clone(), size_of(row, 0, path))),
        )
        .collect()
}

/// Every file under `originals/cache` and `originals/runs`, with its size, keyed by its path
/// relative to `originals` with `/` separators.
fn local_inventory(originals: &Path) -> Result<BTreeMap<String, u64>, String> {
    let mut files = BTreeMap::new();
    let mut pending: Vec<(PathBuf, String)> = ROOTS
        .iter()
        .map(|name| (originals.join(name), (*name).to_owned()))
        .collect();
    while let Some((dir, rel)) = pending.pop() {
        let entries =
            fs::read_dir(&dir).map_err(|e| format!("cannot list {}: {e}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("cannot list {}: {e}", dir.display()))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            let meta =
                fs::metadata(&path).map_err(|e| format!("cannot stat {}: {e}", path.display()))?;
            let child = format!("{rel}/{name}");
            if meta.is_dir() {
                pending.push((path, child));
            } else if meta.is_file() {
                files.insert(child, meta.len());
            }
        }
    }
    Ok(files)
}

/// The paths of two maps that differ, as three short lists.
fn differences(
    expected: &BTreeMap<String, u64>,
    actual: &BTreeMap<String, u64>,
    side: &str,
) -> String {
    let missing: Vec<&String> = expected
        .keys()
        .filter(|p| !actual.contains_key(*p))
        .collect();
    let extra: Vec<&String> = actual
        .keys()
        .filter(|p| !expected.contains_key(*p))
        .collect();
    let resized: Vec<String> = expected
        .iter()
        .filter_map(|(p, size)| {
            actual
                .get(p)
                .filter(|other| *other != size)
                .map(|other| format!("{p} ({size} vs {other})"))
        })
        .collect();
    format!(
        "{} missing {side} {:?}, {} extra {side} {:?}, {} with another size {:?}",
        missing.len(),
        missing.iter().take(5).collect::<Vec<_>>(),
        extra.len(),
        extra.iter().take(5).collect::<Vec<_>>(),
        resized.len(),
        resized.iter().take(5).collect::<Vec<_>>()
    )
}

/// `git ls-remote origin refs/heads/master` with no prompt and a deadline: the commit, or why the
/// remote could not be read.
fn remote_master(checkout: &Path) -> Result<String, String> {
    let mut command = checkout_git(checkout, &[], &["ls-remote", "origin", "refs/heads/master"]);
    command
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|e| format!("cannot run git ls-remote: {e}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > REMOTE_DEADLINE => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "git ls-remote origin did not finish within {} s",
                    REMOTE_DEADLINE.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(format!("cannot wait for git ls-remote: {e}")),
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("cannot read git ls-remote: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        return Err(format!(
            "git ls-remote origin refs/heads/master exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    stdout
        .lines()
        .find_map(|line| {
            line.strip_suffix("\trefs/heads/master")
                .map(|commit| commit.trim().to_owned())
        })
        .ok_or_else(|| format!("git ls-remote origin printed no master: {stdout:?}"))
}

/// Runs the xtask binary from the workspace root, with `DNACALC_LEGACY_ROOT` set to `legacy_root`
/// when given, recording its output under the case's directory.
fn xtask(evidence: &Evidence, label: &str, args: &[&str], legacy_root: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
    command.args(args).current_dir(repo_root());
    if let Some(root) = legacy_root {
        command.env(LEGACY_ROOT_VARIABLE, root);
    }
    super::run_recorded(label, command, &evidence.dir)
}

/// A link at `link` to the directory `target`: a directory junction, which needs no privilege.
#[cfg(windows)]
fn link_dir(target: &Path, link: &Path) -> Result<(), String> {
    let output = Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(target)
        .output()
        .map_err(|e| format!("cannot run mklink: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "mklink /J exited with {:?}: {}{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// A link at `link` to the directory `target`: a symbolic link.
#[cfg(unix)]
fn link_dir(target: &Path, link: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target, link).map_err(|e| {
        format!(
            "cannot link {} to {}: {e}",
            link.display(),
            target.display()
        )
    })
}

/// Removes the link itself, never what it points to.
fn remove_link(link: &Path) -> std::io::Result<()> {
    if cfg!(windows) {
        fs::remove_dir(link)
    } else {
        fs::remove_file(link)
    }
}

/// Removes `<root>/OxFunc-Evidence` when it is a link and then `root` when it is empty; anything
/// else is left in place and reported. Nothing is deleted recursively.
fn clear_linked_root(root: &Path) -> Result<(), String> {
    let link = root.join(CHECKOUT);
    match fs::symlink_metadata(&link) {
        Ok(meta) if meta.file_type().is_symlink() => remove_link(&link)
            .map_err(|e| format!("cannot remove the link {}: {e}", link.display()))?,
        Ok(_) => {
            return Err(format!(
                "{} exists and is not a link; it is left in place",
                link.display()
            ));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("cannot inspect {}: {e}", link.display())),
    }
    match fs::remove_dir(root) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!(
            "cannot remove the temporary legacy root {}: {e}",
            root.display()
        )),
    }
}

/// The temporary legacy root: a directory under `CARGO_TARGET_TMPDIR` that holds only a link to
/// the evidence checkout. Dropping it removes the link and the directory.
struct LinkedRoot {
    root: PathBuf,
    removed: bool,
}

impl LinkedRoot {
    fn new(checkout: &Path) -> Result<LinkedRoot, String> {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(LINKED_ROOT);
        clear_linked_root(&root)?;
        fs::create_dir_all(&root).map_err(|e| format!("cannot create {}: {e}", root.display()))?;
        let linked = LinkedRoot {
            root,
            removed: false,
        };
        link_dir(checkout, &linked.root.join(CHECKOUT))?;
        Ok(linked)
    }

    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.root)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    fn remove(&mut self) -> Result<(), String> {
        self.removed = true;
        clear_linked_root(&self.root)
    }
}

impl Drop for LinkedRoot {
    fn drop(&mut self) {
        if !self.removed
            && let Err(e) = clear_linked_root(&self.root)
        {
            eprintln!("{e}");
        }
    }
}

/// A seed for the sample, from the clock: a different sample on every run.
fn fresh_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    (nanos % (1u128 << 32)) as u64
}

fn record() -> Value {
    super::read_json(&repo_root().join(RECORD))
}

fn text<'a>(value: &'a Value, what: &str) -> &'a str {
    value
        .as_str()
        .unwrap_or_else(|| panic!("{RECORD}: {what} is not a string: {value}"))
}

/// infra_m0_6__restore_roundtrip: the evidence checkout is clean at the pinned commit, which is
/// GitHub's master; the pinned tree holds one blob of the manifest's gzip size per manifest row
/// and the manifest's bytes hash to the recorded SHA-256; each path the record lists as removed
/// from the archive was a manifest row at its recorded size before the commit that removed it and
/// is in neither table at the pin; the local originals equal manifest.tsv plus excluded.tsv plus
/// those removed paths by path and size; and `cargo xtask evidence restore` of a random sample of at
/// least 100 manifest paths, run from a legacy root that holds the evidence checkout and no
/// originals, writes files byte-identical to the originals.
pub fn restore_roundtrip() {
    const CASE: &str = "infra_m0_6__restore_roundtrip";
    registered(CASE);
    let mut evidence = Evidence::new(CASE);
    let inputs = match inputs() {
        Ok(inputs) => inputs,
        Err(reason) => return evidence.not_run(reason),
    };
    evidence.line(format!("legacy root: {}", inputs.chosen));
    let record = record();
    let repository = &record["evidence_repository"];
    let pin = text(&repository["commit"], "evidence_repository.commit").to_owned();
    evidence.line(format!("pinned commit: {pin}"));
    let checkout = &inputs.checkout;

    // The checkout is the pinned commit of the recorded repository, clean.
    evidence.expect(
        repository["url"] == URL && repository["branch"] == "master",
        format!("the record names {URL}, branch master"),
    );
    let origin = git_text(checkout, &[], &["remote", "get-url", "origin"]);
    evidence.expect(
        origin
            .as_deref()
            .is_ok_and(|url| url.trim_end_matches(".git") == URL),
        format!("the checkout's origin is {URL} ({origin:?})"),
    );
    let status = git_text(
        checkout,
        &STATUS_CONFIG,
        &["status", "--porcelain", "--untracked-files=all"],
    );
    evidence.expect(
        status.as_deref() == Ok(""),
        format!(
            "the evidence checkout is clean: git -c {} status --porcelain --untracked-files=all prints nothing ({status:?})",
            STATUS_CONFIG.join(" -c ")
        ),
    );
    let head = git_text(checkout, &[], &["rev-parse", "HEAD"]);
    evidence.expect(
        head.as_deref() == Ok(pin.as_str()),
        format!("the checkout's HEAD is the pinned commit ({head:?})"),
    );

    // The pinned manifest: its bytes, blob ID, hash and counts.
    let manifest = pinned_table(checkout, &pin, "manifest.tsv").unwrap_or_else(|e| panic!("{e}"));
    let excluded = pinned_table(checkout, &pin, "excluded.tsv").unwrap_or_else(|e| panic!("{e}"));
    for (name, table, key) in [
        ("manifest.tsv", &manifest, "manifest"),
        ("excluded.tsv", &excluded, "excluded"),
    ] {
        let recorded = &record[key];
        let blob = git_text(checkout, &[], &["rev-parse", &format!("{pin}:{name}")]);
        evidence.expect(
            matches!((blob.as_deref(), recorded["git_blob"].as_str()), (Ok(a), Some(b)) if a == b),
            format!(
                "{name} at the pin is blob {} ({blob:?})",
                recorded["git_blob"]
            ),
        );
        let hashed = sha256(&table.bytes);
        evidence.expect(
            matches!((hashed.as_deref(), recorded["sha256"].as_str()), (Ok(a), Some(b)) if a == b),
            format!(
                "{name} at the pin hashes to the recorded SHA-256 {} with {}'s hashlib ({hashed:?})",
                recorded["sha256"],
                python()
            ),
        );
        evidence.expect(
            json!(table.header) == recorded["columns"],
            format!("{name}'s header is {}", recorded["columns"]),
        );
    }
    let raw: u64 = manifest
        .rows
        .iter()
        .map(|(p, row)| size_of(row, 0, p))
        .sum();
    let gz: u64 = manifest
        .rows
        .iter()
        .map(|(p, row)| size_of(row, 2, p))
        .sum();
    let in_root = |table: &Table, root: &str| {
        table
            .rows
            .keys()
            .filter(|p| p.starts_with(&format!("{root}/")))
            .count()
    };
    let counted = json!({
        "archived_files": manifest.rows.len(),
        "cache_files": in_root(&manifest, "cache"),
        "runs_files": in_root(&manifest, "runs"),
        "raw_bytes": raw,
        "gz_bytes": gz,
    });
    let recorded_counts: Value = [
        "archived_files",
        "cache_files",
        "runs_files",
        "raw_bytes",
        "gz_bytes",
    ]
    .iter()
    .map(|key| ((*key).to_owned(), record["manifest"][*key].clone()))
    .collect::<serde_json::Map<String, Value>>()
    .into();
    evidence.expect(
        counted == recorded_counts,
        format!("the manifest's counts {counted} are the recorded ones"),
    );
    let excluded_raw: u64 = excluded
        .rows
        .iter()
        .map(|(p, row)| size_of(row, 0, p))
        .sum();
    evidence.expect(
        record["excluded"]["files"] == json!(excluded.rows.len())
            && record["excluded"]["raw_bytes"] == json!(excluded_raw),
        format!(
            "excluded.tsv lists the recorded {} files of {} bytes ({} of {excluded_raw})",
            record["excluded"]["files"],
            record["excluded"]["raw_bytes"],
            excluded.rows.len()
        ),
    );
    let overlap: Vec<&String> = manifest
        .rows
        .keys()
        .filter(|p| excluded.rows.contains_key(*p))
        .collect();
    evidence.expect(
        overlap.is_empty(),
        format!("no path is both archived and excluded ({overlap:?})"),
    );

    // The GitHub copy's inventory: the pinned tree holds exactly one blob per manifest row, of the
    // manifest's gzip size, under cache/ and runs/.
    let tree = git_bytes(
        checkout,
        &[],
        &["ls-tree", "-r", "-l", "-z", &pin, "--", "cache", "runs"],
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let mut blobs: BTreeMap<String, u64> = BTreeMap::new();
    for entry in tree
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let entry = String::from_utf8_lossy(entry);
        let (meta, path) = entry
            .split_once('\t')
            .unwrap_or_else(|| panic!("git ls-tree printed {entry:?}"));
        let size = meta
            .split_whitespace()
            .nth(3)
            .and_then(|size| size.parse().ok())
            .unwrap_or_else(|| panic!("git ls-tree printed no size in {entry:?}"));
        blobs.insert(path.to_owned(), size);
    }
    let expected_blobs: BTreeMap<String, u64> = manifest
        .rows
        .iter()
        .map(|(p, row)| (format!("{p}.gz"), size_of(row, 2, p)))
        .collect();
    evidence.expect(
        blobs == expected_blobs,
        format!(
            "the pinned tree holds exactly the {} blobs <path>.gz of the manifest at their gzip sizes: {}",
            expected_blobs.len(),
            differences(&expected_blobs, &blobs, "in the tree")
        ),
    );

    // The paths the record says were removed from the archive (the licence cleanup): each was a row
    // of manifest.tsv, at the recorded size, in the parent of the commit that removed it, which is
    // the pin or one of its ancestors; none is in either table at the pin.
    let removal = &record["removed"];
    let removing = text(&removal["commit"], "removed.commit").to_owned();
    let removed: BTreeMap<String, u64> = removal["paths"]
        .as_array()
        .unwrap_or_else(|| panic!("{RECORD}: removed.paths is not a list"))
        .iter()
        .map(|row| {
            let path = text(&row["path"], "removed.paths[].path").to_owned();
            let size = row["raw_bytes"]
                .as_u64()
                .unwrap_or_else(|| panic!("{RECORD}: {path} has no raw_bytes"));
            (path, size)
        })
        .collect();
    let removed_bytes: u64 = removed.values().sum();
    evidence.expect(
        removal["files"] == json!(removed.len()) && removal["raw_bytes"] == json!(removed_bytes),
        format!(
            "removed lists the recorded {} files of {} bytes ({} of {removed_bytes})",
            removal["files"],
            removal["raw_bytes"],
            removed.len()
        ),
    );
    let ancestor = git_text(
        checkout,
        &[],
        &["merge-base", "--is-ancestor", &removing, &pin],
    );
    evidence.expect(
        ancestor.is_ok(),
        format!("the removing commit {removing} is the pin or one of its ancestors ({ancestor:?})"),
    );
    match pinned_table(checkout, &format!("{removing}^"), "manifest.tsv") {
        Ok(before) => {
            let listed: BTreeMap<String, u64> = removed
                .keys()
                .filter_map(|path| {
                    before
                        .rows
                        .get(path)
                        .map(|row| (path.clone(), size_of(row, 0, path)))
                })
                .collect();
            evidence.expect(
                listed == removed,
                format!(
                    "manifest.tsv at {removing}^ lists each removed path at its recorded size: {}",
                    differences(&removed, &listed, "there")
                ),
            );
        }
        Err(e) => evidence.expect(
            false,
            format!("manifest.tsv at {removing}^ is readable: {e}"),
        ),
    }
    let still: Vec<&String> = removed
        .keys()
        .filter(|path| manifest.rows.contains_key(*path) || excluded.rows.contains_key(*path))
        .collect();
    evidence.expect(
        still.is_empty(),
        format!("no removed path is in manifest.tsv or excluded.tsv at the pin ({still:?})"),
    );

    // The local originals against the pinned inventory and the removed paths, by path and size.
    let mut pinned = pinned_inventory(&manifest, &excluded);
    let pinned_paths = pinned.len();
    pinned.extend(removed.clone());
    let local = local_inventory(&inputs.originals).unwrap_or_else(|e| panic!("{e}"));
    let local_bytes: u64 = local.values().sum();
    evidence.expect(
        local == pinned,
        format!(
            "the local originals ({} files, {local_bytes} bytes) equal manifest.tsv plus excluded.tsv ({pinned_paths} paths) plus the {} removed paths by path and size: {}",
            local.len(),
            removed.len(),
            differences(&pinned, &local, "locally")
        ),
    );

    // GitHub's master.
    let remote = remote_master(checkout);
    let remote_unreadable = match &remote {
        Ok(commit) => {
            evidence.expect(
                *commit == pin,
                format!("GitHub's master (git ls-remote origin) is the pinned commit ({commit})"),
            );
            None
        }
        Err(reason) => {
            evidence.line(format!("GitHub's master could not be read: {reason}"));
            Some(reason.clone())
        }
    };

    // A random sample restored through the repository command from a legacy root that holds only a
    // link to the evidence checkout, then compared byte for byte with the originals.
    let seed = fresh_seed();
    let target = evidence.dir.join("restore");
    evidence.note(format!(
        "restore sample of {SAMPLE} manifest paths drawn with seed {seed}"
    ));
    let mut linked = LinkedRoot::new(checkout).unwrap_or_else(|e| panic!("{e}"));
    let names = linked.names();
    evidence.expect(
        names == [CHECKOUT] && !linked.root.join("OxFunc").exists(),
        format!(
            "the temporary legacy root <CARGO_TARGET_TMPDIR>/{LINKED_ROOT} that {LEGACY_ROOT_VARIABLE} names for the restore holds only {CHECKOUT}, a link to the evidence checkout, and no OxFunc/smart-fuzzer ({names:?})"
        ),
    );
    let target_arg = target.to_string_lossy().into_owned();
    let sample = SAMPLE.to_string();
    let seed_arg = seed.to_string();
    let out = xtask(
        &evidence,
        "evidence-restore",
        &[
            "evidence",
            "restore",
            &target_arg,
            "--sample",
            &sample,
            "--seed",
            &seed_arg,
        ],
        Some(&linked.root),
    );
    let removed = linked.remove();
    evidence.expect(
        removed.is_ok(),
        format!("the link and the temporary legacy root were removed afterwards ({removed:?})"),
    );
    evidence.expect(
        out.status.code() == Some(0),
        format!(
            "cargo xtask evidence restore <dir> --sample {SAMPLE} --seed {seed} exits 0 ({:?})",
            out.status.code()
        ),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let running: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("xtask evidence: running "))
        .collect();
    evidence.expect(
        running.len() == 1
            && running.iter().all(|line| {
                line.contains(LINKED_ROOT)
                    && line.contains("\"--dest\"")
                    && line.contains("\"restore\"")
                    && !line.contains("--source")
                    && !line.contains("smart-fuzzer")
            }),
        format!(
            "the restore ran archive-evidence.py through the temporary legacy root with --dest and no --source, so it was given no path to the originals: {running:?}"
        ),
    );
    let compared: Vec<&str> = stdout
        .lines()
        .filter(|line| {
            line.starts_with("xtask evidence: pinned commit ")
                || line.starts_with("xtask evidence: checkout ")
                || line.contains("WARNING:")
        })
        .collect();
    evidence.expect(
        compared.iter().any(|l| *l == format!("xtask evidence: pinned commit {pin} ({RECORD})"))
            && compared.contains(&format!("xtask evidence: checkout HEAD {pin}").as_str())
            && compared.contains(&"xtask evidence: checkout tree clean")
            && !compared.iter().any(|l| l.contains("WARNING:")),
        format!(
            "the restore printed the pinned commit, the checkout's HEAD equal to it and a clean tree, with no warning: {compared:?}"
        ),
    );
    let restored = if target.is_dir() {
        local_inventory_under(&target).unwrap_or_else(|e| panic!("{e}"))
    } else {
        BTreeMap::new()
    };
    let mut listing = String::new();
    let mut identical = 0usize;
    let mut compared_bytes = 0u64;
    let mut problems = Vec::new();
    for (path, size) in &restored {
        listing.push_str(&format!("{path}\t{size}\n"));
        if !manifest.rows.contains_key(path) {
            problems.push(format!("{path} is not a manifest path"));
            continue;
        }
        let ours = fs::read(target.join(path));
        let original = fs::read(inputs.originals.join(path));
        match (ours, original) {
            (Ok(ours), Ok(original)) if ours == original => {
                identical += 1;
                compared_bytes += ours.len() as u64;
            }
            (Ok(ours), Ok(original)) => problems.push(format!(
                "{path} differs from its original ({} vs {} bytes)",
                ours.len(),
                original.len()
            )),
            (ours, original) => problems.push(format!(
                "{path} cannot be read: restored {:?}, original {:?}",
                ours.err(),
                original.err()
            )),
        }
    }
    evidence.write("restored.tsv", &listing);
    evidence.expect(
        restored.len() == SAMPLE && identical == SAMPLE && problems.is_empty(),
        format!(
            "the restore wrote {} files, {identical} of them byte-identical to the originals ({compared_bytes} bytes compared; the sample size is {SAMPLE}): {problems:?}",
            restored.len()
        ),
    );
    let drawn = drawn_sample(&manifest.rows.keys().collect(), SAMPLE, seed);
    let written: BTreeSet<String> = restored.keys().cloned().collect();
    evidence.expect(
        drawn.as_ref() == Ok(&written),
        format!(
            "the restored paths are exactly the {SAMPLE} that random.Random({seed}).sample draws from the sorted manifest paths ({})",
            match &drawn {
                Ok(drawn) => format!(
                    "{} drawn, {} of them restored",
                    drawn.len(),
                    drawn.intersection(&written).count()
                ),
                Err(e) => e.clone(),
            }
        ),
    );
    super::remove_tree(&target).unwrap_or_else(|e| panic!("{e}"));

    if evidence.failures.is_empty()
        && let Some(reason) = remote_unreadable
    {
        return evidence.not_run(format!(
            "every local check passed, but GitHub's master could not be read to compare with the pinned commit: {reason}"
        ));
    }
    evidence.finish();
}

/// Every file under `dir`, with its size, keyed by its path relative to `dir` with `/` separators.
fn local_inventory_under(dir: &Path) -> Result<BTreeMap<String, u64>, String> {
    let mut files = BTreeMap::new();
    let mut pending = vec![(dir.to_path_buf(), String::new())];
    while let Some((at, rel)) = pending.pop() {
        for entry in fs::read_dir(&at).map_err(|e| format!("cannot list {}: {e}", at.display()))? {
            let entry = entry.map_err(|e| format!("cannot list {}: {e}", at.display()))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let child = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            let meta = fs::metadata(entry.path())
                .map_err(|e| format!("cannot stat {}: {e}", entry.path().display()))?;
            if meta.is_dir() {
                pending.push((entry.path(), child));
            } else {
                files.insert(child, meta.len());
            }
        }
    }
    Ok(files)
}

/// The entry an inventory path belongs to: the directory directly under cache/ or runs/ (ending in
/// `/`), or the file itself when it lies directly there.
fn entry_of(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() > 2 {
        format!("{}/{}/", parts[0], parts[1])
    } else {
        path.to_owned()
    }
}

/// Whether `c` may continue a run-folder name.
fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
}

/// Whether `text` names `smart-fuzzer/runs/<name>` with nothing but dots and a character that
/// ends a name (or the end of the text) after it.
fn mentions_folder(text: &str, name: &str) -> bool {
    let marker = format!("smart-fuzzer/runs/{name}");
    text.match_indices(&marker).any(|(at, _)| {
        let rest = text[at + marker.len()..].trim_start_matches('.');
        !rest.chars().next().is_some_and(name_char)
    })
}

/// The values of a selector field that is an array of strings.
fn listed<'a>(selector: &'a Value, field: &str) -> Vec<&'a str> {
    selector[field]
        .as_array()
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// The run folders of `inventory` that are read from their paths only, each with why: a name of a
/// W108/W109 identification run, or a `modules` or `classes` entry of numeric-provenance.json with
/// disposition tainted or unresolved that selects one of the folder's files (as the OxFunc path
/// `smart-fuzzer/<path>`) or whose serialised text names the folder.
fn derive_path_only(
    inventory: &BTreeMap<String, u64>,
    provenance: &Value,
) -> BTreeMap<String, Vec<String>> {
    let mut folders: BTreeMap<String, Vec<&String>> = BTreeMap::new();
    for path in inventory.keys() {
        let parts: Vec<&str> = path.splitn(3, '/').collect();
        if parts.len() == 3 && parts[0] == "runs" {
            folders
                .entry(format!("runs/{}/", parts[1]))
                .or_default()
                .push(path);
        }
    }
    let mut derived = BTreeMap::new();
    for (folder, paths) in folders {
        let name = &folder["runs/".len()..folder.len() - 1];
        let mut because = Vec::new();
        for prefix in IDENTIFICATION_RUNS {
            if name.to_ascii_lowercase().starts_with(prefix) {
                because.push(format!("its name starts with {prefix}, ignoring case"));
            }
        }
        for list in ["modules", "classes"] {
            for item in provenance[list].as_array().map_or(&[][..], Vec::as_slice) {
                let disposition = item["disposition"].as_str().unwrap_or("");
                if !matches!(disposition, "tainted" | "unresolved") {
                    continue;
                }
                let selector = if item["selector"].is_object() {
                    &item["selector"]
                } else {
                    item
                };
                let prefixes = listed(selector, "prefixes");
                let suffixes = listed(selector, "suffixes");
                let paths_listed = listed(selector, "paths");
                let selected = selector["repository"] == "OxFunc"
                    && paths.iter().any(|path| {
                        let full = format!("smart-fuzzer/{path}");
                        selector["path"].as_str() == Some(full.as_str())
                            || paths_listed.contains(&full.as_str())
                            || (!(prefixes.is_empty() && suffixes.is_empty())
                                && (prefixes.is_empty()
                                    || prefixes.iter().any(|p| full.starts_with(p)))
                                && (suffixes.is_empty()
                                    || suffixes.iter().any(|s| full.ends_with(s))))
                    });
                let id = item["id"].as_str().unwrap_or("");
                if selected {
                    because.push(format!(
                        "{PROVENANCE} {list} entry {id} ({disposition}) selects its files"
                    ));
                } else if mentions_folder(&item.to_string(), name) {
                    because.push(format!(
                        "{PROVENANCE} {list} entry {id} ({disposition}) names smart-fuzzer/runs/{name}/"
                    ));
                }
            }
        }
        if !because.is_empty() {
            derived.insert(folder, because);
        }
    }
    derived
}

/// A build value: a string of ASCII digits or a non-negative integer.
fn build_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) => {
            Some(text.clone())
        }
        Value::Number(number) => number.as_u64().map(|n| n.to_string()),
        _ => None,
    }
}

/// The permitted metadata fields of one document, in the record's rule order.
fn permitted_fields(document: &Value) -> Vec<(String, String)> {
    if !document.is_object() {
        return Vec::new();
    }
    let mut found: Vec<(String, String)> = build_value(&document["excel_build"])
        .map(|build| ("excel_build".to_owned(), build))
        .into_iter()
        .collect();
    let mut holders: Vec<(String, &Value)> = HOLDERS
        .iter()
        .map(|name| ((*name).to_owned(), &document[*name]))
        .collect();
    for (outer, inner) in INNER_HOLDERS {
        holders.push((format!("{outer}.{inner}"), &document[outer][inner]));
    }
    if let Some(items) = document["environments"].as_array() {
        for (index, item) in items.iter().enumerate() {
            holders.push((format!("environments[{index}]"), item));
        }
    }
    for (name, holder) in holders.into_iter().filter(|(_, h)| h.is_object()) {
        for field in ["build", "excel_build"] {
            if let Some(build) = build_value(&holder[field]) {
                found.push((format!("{name}.{field}"), build));
            }
        }
    }
    found
}

/// The build values a key `"build"` or `"excel_build"` takes in `bytes`, by splitting the text
/// (NUL bytes dropped) at its quotes: a key part followed by a colon, then a string of digits or an
/// integer.
fn key_builds(bytes: &[u8]) -> BTreeSet<String> {
    let kept: Vec<u8>;
    let bytes = if bytes.contains(&0) {
        kept = bytes.iter().copied().filter(|b| *b != 0).collect();
        &kept[..]
    } else {
        bytes
    };
    let text = String::from_utf8_lossy(bytes);
    let parts: Vec<&str> = text.split('"').collect();
    let mut builds = BTreeSet::new();
    for index in 0..parts.len().saturating_sub(1) {
        if parts[index] != "build" && parts[index] != "excel_build" {
            continue;
        }
        let Some(after) = parts[index + 1].trim_start().strip_prefix(':') else {
            continue;
        };
        let after = after.trim_start();
        if after.is_empty() {
            // A string value: the next part is its text.
            if let Some(value) = parts.get(index + 2)
                && !value.is_empty()
                && value.bytes().all(|b| b.is_ascii_digit())
            {
                builds.insert((*value).to_owned());
            }
        } else {
            let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
            let next = after[digits.len()..].chars().next();
            if !digits.is_empty() && !matches!(next, Some('.' | 'e' | 'E')) {
                builds.insert(digits);
            }
        }
    }
    builds
}

/// What reading one JSON file under the metadata rule gives.
enum Read {
    Fields(Vec<(String, String)>),
    NoBuildText,
    Unread(&'static str),
}

fn read_json_metadata(bytes: &[u8]) -> Read {
    if bytes.is_empty() {
        return Read::Unread("empty");
    }
    if bytes.contains(&0) {
        return Read::Unread("holds NUL bytes");
    }
    let body = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(body) {
        Err(_) => Read::Unread("not UTF-8"),
        Ok(text) if !text.contains("build\"") => Read::NoBuildText,
        Ok(text) => match serde_json::from_str::<Value>(text) {
            Ok(document) => Read::Fields(permitted_fields(&document)),
            Err(_) => Read::Unread("not valid JSON"),
        },
    }
}

/// Everything the case recomputes from the inventory, the path-only folders and the originals.
struct Recomputed {
    labels: Vec<Value>,
    unread: Vec<Value>,
    outside: Vec<Value>,
    counts: Value,
    opened: BTreeSet<String>,
}

fn recompute(
    inventory: &BTreeMap<String, u64>,
    path_only: &BTreeMap<String, Vec<String>>,
    originals: &Path,
) -> Recomputed {
    let mut opened = BTreeSet::new();
    let mut read = |path: &str| -> Vec<u8> {
        opened.insert(path.to_owned());
        fs::read(originals.join(path))
            .unwrap_or_else(|e| panic!("cannot read the original {path}: {e}"))
    };
    // Every file's metadata first, then the labels per entry.
    let mut per_file: BTreeMap<&String, Vec<(String, String)>> = BTreeMap::new();
    let mut unread = Vec::new();
    let (mut json_files, mut parsed, mut no_text) = (0usize, 0usize, 0usize);
    for path in inventory.keys() {
        if !path.ends_with(".json") || path_only.contains_key(&entry_of(path)) {
            continue;
        }
        json_files += 1;
        let bytes = read(path);
        match read_json_metadata(&bytes) {
            Read::Fields(fields) => {
                parsed += 1;
                per_file.insert(path, fields);
            }
            Read::NoBuildText => no_text += 1,
            Read::Unread(reason) => unread.push(json!({
                "file": path,
                "reason": reason,
                "holds_build_key": !key_builds(&bytes).is_empty(),
            })),
        }
    }
    let mut entries: BTreeMap<String, Vec<&String>> = BTreeMap::new();
    for path in inventory.keys() {
        entries.entry(entry_of(path)).or_default().push(path);
    }
    let mut labels = Vec::new();
    let mut outside = Vec::new();
    for (entry, paths) in &entries {
        let closed = path_only.contains_key(entry);
        let mut sources: Vec<Value> = Vec::new();
        let mut named_dirs: BTreeSet<String> = BTreeSet::new();
        for path in paths {
            let mut dir = String::new();
            let components: Vec<&str> = path.split('/').collect();
            for component in &components[..components.len() - 1] {
                dir.push_str(component);
                dir.push('/');
                let build = component.strip_prefix("build-").unwrap_or("");
                if !build.is_empty()
                    && build.bytes().all(|b| b.is_ascii_digit())
                    && named_dirs.insert(dir.clone())
                {
                    sources.push(json!({"path": dir, "build": build}));
                }
            }
        }
        for path in paths {
            for (field, build) in per_file.get(path).into_iter().flatten() {
                sources.push(json!({"file": path, "field": field, "build": build}));
            }
        }
        let builds: BTreeSet<String> = sources
            .iter()
            .filter_map(|source| source["build"].as_str().map(str::to_owned))
            .collect();
        let build = if builds.len() == 1 {
            builds.iter().next().cloned()
        } else {
            None
        };
        if !closed && sources.is_empty() {
            let mut files = Vec::new();
            let mut found = BTreeSet::new();
            for path in paths
                .iter()
                .filter(|p| p.ends_with(".json") || p.ends_with(".jsonl"))
            {
                let keys = key_builds(&read(path));
                if !keys.is_empty() {
                    files.push((*path).clone());
                    found.extend(keys);
                }
            }
            if !files.is_empty() {
                outside.push(json!({"entry": entry, "files": files, "builds": found}));
            }
        }
        let mut label = json!({
            "entry": entry,
            "files": paths.len(),
            "reading": if closed { "path only" } else { "path and metadata" },
            "label": if build.is_some() { "known" } else { "unknown" },
            "excel_build": build,
            "sources": sources,
            "certifies_exact_target": build.as_ref().map(|b| format!("excel-build-{b}")),
        });
        if builds.len() > 1 {
            label["conflicting_builds"] = json!(builds);
        }
        labels.push(label);
    }
    let known = labels.iter().filter(|l| l["label"] == "known").count();
    let mut by_build: BTreeMap<String, usize> = BTreeMap::new();
    for label in labels.iter().filter(|l| l["label"] == "known") {
        *by_build
            .entry(label["excel_build"].as_str().unwrap_or("").to_owned())
            .or_default() += 1;
    }
    let counts = json!({
        "entries": labels.len(),
        "files": inventory.len(),
        "known": known,
        "unknown": labels.len() - known,
        "path_only_entries": labels.iter().filter(|l| l["reading"] == "path only").count(),
        "known_by_build": by_build,
        "unknown_without_fingerprint": labels
            .iter()
            .filter(|l| l["label"] == "unknown" && l["sources"] == json!([]))
            .count(),
        "unknown_conflicting": labels.iter().filter(|l| !l["conflicting_builds"].is_null()).count(),
        "outside_rule_entries": outside.len(),
        "metadata_files": {
            "json": json_files,
            "parsed": parsed,
            "without_build_key": no_text,
            "unread": unread.len(),
        },
    });
    Recomputed {
        labels,
        unread,
        outside,
        counts,
        opened,
    }
}

/// Checks one label's shape and its certification: an unknown entry certifies nothing, and a known
/// entry certifies only its own build, which every source gives.
fn label_problems(label: &Value) -> Vec<String> {
    let mut problems = Vec::new();
    let entry = label["entry"].as_str().unwrap_or("<no entry>");
    let sources = label["sources"].as_array().cloned().unwrap_or_default();
    let builds: BTreeSet<&str> = sources
        .iter()
        .filter_map(|source| source["build"].as_str())
        .collect();
    let keys: BTreeSet<&str> = label
        .as_object()
        .map(|object| object.keys().map(String::as_str).collect())
        .unwrap_or_default();
    let mut expected_keys: BTreeSet<&str> = [
        "entry",
        "files",
        "reading",
        "label",
        "excel_build",
        "sources",
        "certifies_exact_target",
    ]
    .into_iter()
    .collect();
    if builds.len() > 1 {
        expected_keys.insert("conflicting_builds");
    }
    if keys != expected_keys {
        problems.push(format!("{entry} has the fields {keys:?}"));
    }
    match label["label"].as_str() {
        Some("known") => {
            let build = label["excel_build"].as_str().unwrap_or("");
            if build.is_empty() || !build.bytes().all(|b| b.is_ascii_digit()) {
                problems.push(format!("{entry} is known without a build"));
            }
            if label["certifies_exact_target"] != json!(format!("excel-build-{build}")) {
                problems.push(format!(
                    "{entry} is known at build {build} but certifies {}",
                    label["certifies_exact_target"]
                ));
            }
            if builds != BTreeSet::from([build]) {
                problems.push(format!(
                    "{entry} is known at build {build} but its sources give {builds:?}"
                ));
            }
        }
        Some("unknown") => {
            if !label["excel_build"].is_null() || !label["certifies_exact_target"].is_null() {
                problems.push(format!(
                    "{entry} is unknown but names build {} and certifies {}",
                    label["excel_build"], label["certifies_exact_target"]
                ));
            }
            if builds.len() == 1 {
                problems.push(format!(
                    "{entry} is unknown although its sources give the one build {builds:?}"
                ));
            }
        }
        other => problems.push(format!("{entry} has the label {other:?}")),
    }
    problems
}

/// The entries of `values` whose `key` differs between two lists keyed by `key`, for a failure
/// message.
fn differing(record: &[Value], recomputed: &[Value], key: &str) -> Vec<String> {
    let index = |values: &[Value]| -> BTreeMap<String, Value> {
        values
            .iter()
            .map(|value| (value[key].as_str().unwrap_or("").to_owned(), value.clone()))
            .collect()
    };
    let (ours, theirs) = (index(recomputed), index(record));
    let all: BTreeSet<&String> = ours.keys().chain(theirs.keys()).collect();
    all.into_iter()
        .filter(|name| ours.get(*name) != theirs.get(*name))
        .map(|name| {
            format!(
                "{name}: record {} vs recomputed {}",
                theirs
                    .get(name)
                    .map_or("absent".to_owned(), Value::to_string),
                ours.get(name).map_or("absent".to_owned(), Value::to_string)
            )
        })
        .collect()
}

fn array(value: &Value, what: &str) -> Vec<Value> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("{RECORD}: {what} is not an array: {value}"))
        .clone()
}

/// infra_m0_6__unknown_metadata: every label is known with one build and certifies only that
/// build's Exact target, or unknown and certifies none; literal examples hold (the oracle cache of
/// build 20131 known from its path and its env.json, a run known at build 20430 from a capture one
/// directory below it, runs known from the holders profile and environment.excel, a run with no
/// fingerprint unknown, a W108 identification run unknown from its path alone, an all-NUL file
/// listed as unread, a file whose build sits only in per-row provenance listed outside the rule);
/// the path-only folders are derived from their names and numeric-provenance.json; and the
/// record's labels, unread files, outside-rule entries and counts equal a recomputation that opens
/// no file of a path-only folder.
pub fn unknown_metadata() {
    const CASE: &str = "infra_m0_6__unknown_metadata";
    registered(CASE);
    let mut evidence = Evidence::new(CASE);
    let record = record();
    let labels = array(&record["labels"], "labels");
    let by_entry: BTreeMap<&str, &Value> = labels
        .iter()
        .map(|label| (label["entry"].as_str().unwrap_or(""), label))
        .collect();
    evidence.expect(
        by_entry.len() == labels.len() && !by_entry.contains_key(""),
        format!("each of the {} labels names a distinct entry", labels.len()),
    );

    // Literal examples, compared whole.
    let examples = [
        json!({
            "entry": "cache/oracle/",
            "files": 73,
            "reading": "path and metadata",
            "label": "known",
            "excel_build": "20131",
            "sources": [
                {"path": "cache/oracle/build-20131/", "build": "20131"},
                {"file": "cache/oracle/env.json", "field": "excel_build", "build": "20131"}
            ],
            "certifies_exact_target": "excel-build-20131"
        }),
        json!({
            "entry": "runs/w088-pmt-ppmt-pilot/",
            "files": 29,
            "reading": "path and metadata",
            "label": "known",
            "excel_build": "19929",
            "sources": [
                {"file": "runs/w088-pmt-ppmt-pilot/manifest.json", "field": "environment.excel.excel_build", "build": "19929"}
            ],
            "certifies_exact_target": "excel-build-19929"
        }),
        json!({
            "entry": "runs/w111-text-slice-discovery-20260929/",
            "files": 6,
            "reading": "path and metadata",
            "label": "known",
            "excel_build": "20430",
            "sources": [
                {"file": "runs/w111-text-slice-discovery-20260929/utf16-observations.json", "field": "profile.excel_build", "build": "20430"},
                {"file": "runs/w111-text-slice-discovery-20260929/utf16-tail-observations.json", "field": "profile.excel_build", "build": "20430"}
            ],
            "certifies_exact_target": "excel-build-20430"
        }),
        json!({
            "entry": "runs/broad-scalar-cycle-001/",
            "files": 5,
            "reading": "path and metadata",
            "label": "unknown",
            "excel_build": null,
            "sources": [],
            "certifies_exact_target": null
        }),
        json!({
            "entry": "runs/w108a-reference-hunt/",
            // 58 inventory paths before dc-96l.4 removed 9 of them from the archive (the record's
            // `removed` paths under this folder).
            "files": 49,
            "reading": "path only",
            "label": "unknown",
            "excel_build": null,
            "sources": [],
            "certifies_exact_target": null
        }),
        json!({
            "entry": "cache/w111-regenerated-baseline.json",
            "files": 1,
            "reading": "path and metadata",
            "label": "unknown",
            "excel_build": null,
            "sources": [],
            "certifies_exact_target": null
        }),
    ];
    for example in &examples {
        let entry = example["entry"].as_str().unwrap_or("");
        evidence.expect(
            by_entry.get(entry).copied() == Some(example),
            format!(
                "the record labels {entry} exactly as {example} (record: {})",
                by_entry
                    .get(entry)
                    .map_or("absent".to_owned(), |l| l.to_string())
            ),
        );
    }
    // A run whose build is recorded only in captures one directory below it.
    let nested_entry = "runs/w111-extended-numeric-20260929/";
    let nested_source = json!({
        "file": "runs/w111-extended-numeric-20260929/answers/answers-acosh.json",
        "field": "capture_provenance.environment.excel_build",
        "build": "20430"
    });
    let nested = by_entry
        .get(nested_entry)
        .copied()
        .cloned()
        .unwrap_or(Value::Null);
    let nested_sources = nested["sources"].as_array().cloned().unwrap_or_default();
    evidence.expect(
        nested["label"] == "known"
            && nested["excel_build"] == "20430"
            && nested["certifies_exact_target"] == "excel-build-20430"
            && nested["reading"] == "path and metadata"
            && nested_sources.contains(&nested_source)
            && nested_sources.iter().all(|s| {
                s["build"] == "20430"
                    && s["file"].as_str().is_some_and(|f| f.matches('/').count() >= 3)
            }),
        format!(
            "the record labels {nested_entry} known at build 20430, certifying excel-build-20430, from its {} sources, every one a file below a subdirectory of the run, among them {nested_source}",
            nested_sources.len()
        ),
    );
    let unread_example = json!({"file": "runs/wd-hol-001/rollup.json", "reason": "holds NUL bytes", "holds_build_key": false});
    let outside_example = json!({"entry": "cache/w111-regenerated-baseline.json", "files": ["cache/w111-regenerated-baseline.json"], "builds": ["20430"]});
    let record_unread = array(&record["unread_metadata"], "unread_metadata");
    let record_outside = array(&record["builds_outside_rule"], "builds_outside_rule");
    evidence.expect(
        record_unread.contains(&unread_example),
        format!("the record lists {unread_example} among its unread metadata files"),
    );
    evidence.expect(
        record_outside.contains(&outside_example),
        format!(
            "the record lists {outside_example} among the unknown entries with a build key outside the permitted fields"
        ),
    );

    // Every label's certification.
    let problems: Vec<String> = labels.iter().flat_map(label_problems).collect();
    evidence.expect(
        problems.is_empty(),
        format!(
            "every unknown label certifies no Exact target and every known label certifies only its own build, which all its sources give: {problems:?}"
        ),
    );
    let known_builds: BTreeSet<&str> = labels
        .iter()
        .filter(|label| label["label"] == "known")
        .filter_map(|label| label["excel_build"].as_str())
        .collect();
    let targets: BTreeSet<&str> = labels
        .iter()
        .filter_map(|label| label["certifies_exact_target"].as_str())
        .collect();
    let expected_targets: BTreeSet<String> = known_builds
        .iter()
        .map(|build| format!("excel-build-{build}"))
        .collect();
    evidence.expect(
        targets.iter().map(|t| (*t).to_owned()).collect::<BTreeSet<_>>() == expected_targets,
        format!(
            "the only Exact targets any entry certifies are the known builds' own {targets:?}; any other target, a newly selected build among them, has no certifying entry"
        ),
    );

    // The recomputation, which needs the checkout and the originals.
    let inputs = match inputs() {
        Ok(inputs) => inputs,
        Err(reason) => {
            if !evidence.failures.is_empty() {
                return evidence.finish();
            }
            return evidence.not_run(reason);
        }
    };
    evidence.line(format!("legacy root: {}", inputs.chosen));
    let pin = text(
        &record["evidence_repository"]["commit"],
        "evidence_repository.commit",
    )
    .to_owned();
    let manifest =
        pinned_table(&inputs.checkout, &pin, "manifest.tsv").unwrap_or_else(|e| panic!("{e}"));
    let excluded =
        pinned_table(&inputs.checkout, &pin, "excluded.tsv").unwrap_or_else(|e| panic!("{e}"));
    let inventory = pinned_inventory(&manifest, &excluded);
    evidence.line(format!(
        "inventory at the pinned commit {pin}: {} paths",
        inventory.len()
    ));

    // The path-only folders, derived from their names and numeric-provenance.json.
    let provenance = super::read_json(&repo_root().join(PROVENANCE));
    let path_only = derive_path_only(&inventory, &provenance);
    let derived: Vec<Value> = path_only
        .iter()
        .map(|(entry, because)| json!({"entry": entry, "because": because}))
        .collect();
    evidence.write(
        "path-only.json",
        serde_json::to_string_pretty(&derived).expect("the list serialises"),
    );
    let record_path_only = array(&record["path_only"], "path_only");
    evidence.expect(
        record_path_only == derived,
        format!(
            "the record's path_only list equals the {} folders derived from the w108/w109 name prefixes and the tainted or unresolved entries of {PROVENANCE}: {}",
            derived.len(),
            serde_json::to_string(&derived).expect("the list serialises")
        ),
    );
    let provenance_reason = format!(
        "{PROVENANCE} modules entry excel-numeric-gen-tables (unresolved) names smart-fuzzer/runs/w108a-reference-hunt/"
    );
    evidence.expect(
        path_only
            .get("runs/w108a-reference-hunt/")
            .is_some_and(|because| because.contains(&provenance_reason)),
        format!(
            "the derivation reads {PROVENANCE}: runs/w108a-reference-hunt/ is path-only also because \"{provenance_reason}\""
        ),
    );
    let read_by_path: BTreeSet<&str> = labels
        .iter()
        .filter(|label| label["reading"] == "path only")
        .filter_map(|label| label["entry"].as_str())
        .collect();
    let derived_set: BTreeSet<&str> = path_only.keys().map(String::as_str).collect();
    evidence.expect(
        read_by_path == derived_set,
        format!(
            "exactly the derived folders are labelled from their paths only (labelled {read_by_path:?}, derived {derived_set:?})"
        ),
    );

    // Labels, unread files and outside-rule entries, recomputed.
    let started = Instant::now();
    let recomputed = recompute(&inventory, &path_only, &inputs.originals);
    evidence.line(format!(
        "recomputed {} labels in {:.1} s, opening {} original files; metadata files {}",
        recomputed.labels.len(),
        started.elapsed().as_secs_f64(),
        recomputed.opened.len(),
        recomputed.counts["metadata_files"]
    ));
    evidence.write(
        "opened.txt",
        recomputed
            .opened
            .iter()
            .map(|path| format!("{path}\n"))
            .collect::<String>(),
    );
    let opened_closed: Vec<&String> = recomputed
        .opened
        .iter()
        .filter(|path| path_only.contains_key(&entry_of(path)))
        .collect();
    evidence.expect(
        opened_closed.is_empty(),
        format!(
            "no file of a path-only folder was opened ({} of the {} opened)",
            opened_closed.len(),
            recomputed.opened.len()
        ),
    );
    evidence.write(
        "recomputed-labels.json",
        serde_json::to_string_pretty(&recomputed.labels).expect("labels serialise"),
    );
    let label_differences = differing(&labels, &recomputed.labels, "entry");
    evidence.expect(
        recomputed.labels == labels,
        format!(
            "the record's {} labels equal the recomputation from the pinned inventory's paths and the permitted metadata of JSON files at any depth ({} differ: {:?})",
            labels.len(),
            label_differences.len(),
            label_differences.iter().take(5).collect::<Vec<_>>()
        ),
    );
    evidence.expect(
        recomputed.unread == record_unread,
        format!(
            "the record's unread_metadata equals the {} files the recomputation could not read: {}",
            recomputed.unread.len(),
            serde_json::to_string(&recomputed.unread).expect("the list serialises")
        ),
    );
    let hiding: Vec<&Value> = recomputed
        .unread
        .iter()
        .filter(|file| file["holds_build_key"] != false)
        .collect();
    evidence.expect(
        hiding.is_empty(),
        format!(
            "no unread metadata file holds a build key in its text, so none hides a label ({} of {} do: {hiding:?})",
            hiding.len(),
            recomputed.unread.len()
        ),
    );
    evidence.expect(
        recomputed.outside == record_outside,
        format!(
            "the record's builds_outside_rule equals the {} unknown entries without a fingerprint whose JSON or JSONL text holds a build key outside the permitted fields: {}",
            recomputed.outside.len(),
            serde_json::to_string(&recomputed.outside).expect("the list serialises")
        ),
    );
    evidence.expect(
        recomputed.counts == record["counts"],
        format!(
            "the record's counts {} equal the recomputed {}",
            record["counts"], recomputed.counts
        ),
    );

    // What the reading at any depth and the holders profile and environment.excel add, measured.
    let known: Vec<&Value> = recomputed
        .labels
        .iter()
        .filter(|label| label["label"] == "known")
        .collect();
    let only = |keep: &dyn Fn(&Value) -> bool| {
        known
            .iter()
            .filter(|label| {
                label["sources"]
                    .as_array()
                    .is_some_and(|sources| sources.iter().all(keep))
            })
            .count()
    };
    let below = only(&|source: &Value| {
        source["file"]
            .as_str()
            .is_some_and(|file| file.matches('/').count() >= 3)
    });
    let new_holders = only(&|source: &Value| {
        source["field"].as_str().is_some_and(|field| {
            field.starts_with("profile.") || field.starts_with("environment.excel.")
        })
    });
    evidence.line(format!(
        "{below} entries are known only from JSON files below a subdirectory of the entry, and {new_holders} only from the fields profile.* or environment.excel.*"
    ));
    let without = recomputed.counts["unknown_without_fingerprint"].clone();
    let unknown_nothing = recomputed
        .labels
        .iter()
        .filter(|label| label["sources"] == json!([]))
        .all(|label| label["label"] == "unknown" && label["certifies_exact_target"].is_null());
    evidence.expect(
        unknown_nothing,
        format!(
            "all {without} entries without an Excel fingerprint in their paths or in the permitted metadata fields of their JSON files are unknown and certify nothing; {} of them hold a build key outside those fields (builds_outside_rule), and {} metadata files could not be read, none holding a build key",
            recomputed.outside.len(),
            recomputed.unread.len()
        ),
    );
    evidence.finish();
}
