//! The bodies of the infra-m0.5 acceptance cases (the `#[test]` functions sit at the root of
//! `tests/acceptance.rs`, so libtest lists them under their registered names).
//!
//! Both cases read the committed migration record (`migration/source-manifest.json`,
//! `migration/numeric-provenance.json`, its review file, `migration/release-source-allowlist.json`
//! and `migration/witnesses/`) with this module's own code: its own reading of the dispositions
//! file's entries and of the content-derivation rule (the orchestrator's ruling of 2026-10-15)
//! over the facts the manifest records, its own `git grep` citation sweep and its own
//! touched-by-commit lists, its own `git hash-object` and SHA-256, and, through the legacy root,
//! its own `git rev-parse`, `git cat-file`, `git diff-tree`, `git log`, `git merge-base` and
//! `git show` in the legacy repositories, which it only reads. They drive the xtask binary this
//! test build compiled (`CARGO_BIN_EXE_xtask`), the binary `cargo xtask` runs: `pin-sources` over a
//! fixture tracker and fixture dispositions written to a temporary directory (never the tracked
//! files), `pin-sources --lookup`, and `accept` in copies of the working tree
//! (`infra_m0_3::WorkspaceCopy`) whose allowlist, manifest, dispositions or witness set is altered
//! by hand, so each rejection is observed through the check that every `cargo xtask accept` run
//! performs. A host without the legacy root prints `ACCEPT-NOT-RUN`. Evidence goes to
//! `target/accept/infra-m0.5/<case>/`.

use super::infra_m0_3::{Accepted, Evidence, Fixture, WorkspaceCopy};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BEAD: &str = "infra-m0.5";
const MANIFEST: &str = "migration/source-manifest.json";
const DISPOSITIONS: &str = "migration/numeric-provenance.json";
const ALLOWLIST: &str = "migration/release-source-allowlist.json";
const WITNESSES: &str = "migration/witnesses";
const LEGACY_ROOT_VARIABLE: &str = "DNACALC_LEGACY_ROOT";
const CHECK: &str = "numeric-provenance";
/// The bound plan section 10.2's "small minimized witnesses" takes here: about 60 KB in all.
const WITNESS_BOUND: u64 = 61_440;

/// The seven files of the quarantined numeric directory, named by the bead.
const NUMERIC_DIR: &str = "crates/oxfunc_core/src/excel_numeric/";
const NUMERIC_FILES: [&str; 7] = [
    "x87.rs",
    "mod.rs",
    "tables.rs",
    "gen_tables.py",
    "gammaln.rs",
    "research.rs",
    "x87_excel_ground_truth.tsv",
];

/// The packages that enable oxfunc_core's research-x87 feature (infra-m0.4's quarantine_derived
/// inputs), as this test names them.
const RESEARCH_PACKAGES: [&str; 3] = [
    "smart-fuzzer/tools/calc_graph_racer/",
    "smart-fuzzer/tools/pmt_ppmt_local_eval/",
    "smart-fuzzer/tools/w111/complex_kernel_probe/",
];

/// The rules of the content-derivation ruling, by id.
const RULES: [&str; 5] = [
    "citation-sweep",
    "tainted-commit",
    "unread-include",
    "include-of-quarantined",
    "calls-quarantined",
];

/// The tokens of the staging formulas the clean room keeps out of both provenance files.
const STAGING_TOKENS: [&str; 6] = ["FPREM1", "exp_x87", "ln_x87", "LN87", "RN53", "RN64"];
/// The mark of a redaction in the review file.
const REDACTION_MARK: &str = "[staging formula removed for the clean room]";

/// The owner beads the record must name (round-3 finding 5).
const OWNERS: [&str; 4] = ["dc-4uy4", "dc-2203", "dc-0huc", "dc-1c6a"];

/// The five inputs whose include macros the pin cannot read.
const UNREAD_INCLUDES: [&str; 5] = [
    "OxFunc/crates/oxfunc_core/tests/w111_distribution_numeric_live_replay.rs",
    "OxFunc/crates/oxfunc_core/tests/w111_integer_live_replay.rs",
    "OxFunc/crates/oxfunc_core/tests/w111_numeric_publication_live_replay.rs",
    "OxFunc/crates/oxfunc_core/tests/w111_radix_live_replay.rs",
    "OxFunc/tools/xll-addin/oxfunc_xll/src/lib.rs",
];

/// The fixture package the workspace copies add (the infra-m0.3 fixture), whose `fixture-pass`
/// bead has one case that passes.
static ACCEPT_FIXTURE: Fixture = Fixture {
    package: "xtask-fixture",
    dir: "tools/xtask-fixture",
    source: "accept",
    files: &[
        ("package.toml", "Cargo.toml"),
        ("lib.rs", "src/lib.rs"),
        ("acceptance.rs", "tests/acceptance.rs"),
        ("zero_run.rs", "tests/zero_run.rs"),
        ("acceptance.json", "acceptance.json"),
        ("seams.json", "seams.json"),
    ],
};

fn repo_root() -> PathBuf {
    super::repo_root()
}

fn host_platform() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "linux" => "linux",
        other => panic!("the infra-m0.5 cases run on Windows and Linux hosts, not {other}"),
    }
}

/// Checks that `case` is registered under infra-m0.5 for windows and linux, with exactly the
/// bead's two Acceptance commands and the unit-test command its checks rely on (so an accept run
/// never runs `pin-sources` and never rewrites a tracked file).
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
    assert!(
        platforms.iter().any(|p| p == "windows") && platforms.iter().any(|p| p == "linux"),
        "{case} must declare windows and linux, not {platforms:?}"
    );
    let commands: Vec<Vec<String>> = entry["commands"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|command| super::strings(command, "a registered command"))
        .collect();
    let argv = |args: &[&str]| -> Vec<String> { args.iter().map(|a| (*a).to_owned()).collect() };
    let accept = |platform: &str| {
        argv(&[
            "cargo",
            "xtask",
            "accept",
            "--id",
            BEAD,
            "--platform",
            platform,
            "--require-all",
        ])
    };
    assert_eq!(
        commands,
        vec![
            accept("windows"),
            accept("linux"),
            argv(&["cargo", "test", "-p", "xtask", "--bins"])
        ],
        "{BEAD} must register exactly its two Acceptance commands and cargo test -p xtask --bins"
    );
}

/// The legacy root, as the generator chooses it, with how it was chosen (never shown as an
/// absolute host path).
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

fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// A read-only git command in a legacy repository.
fn legacy_git(dir: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command
        .arg("--no-optional-locks")
        .arg("-c")
        .arg(format!("safe.directory={}", slashed(dir)))
        .arg("-C")
        .arg(dir)
        .args(args);
    command
}

fn legacy_bytes(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = legacy_git(dir, args)
        .output()
        .map_err(|e| format!("cannot start git {args:?}: {e}"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(format!(
            "git {args:?} exited with {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn legacy_read(dir: &Path, args: &[&str]) -> Result<String, String> {
    legacy_bytes(dir, args).map(|b| String::from_utf8_lossy(&b).trim().to_owned())
}

/// The `-c` settings added to `status`, as the generator and the infra-m0.4 cases add them, so
/// that a Windows host and a WSL host reading the same checkouts measure the same thing.
const STATUS_CONFIG: [&str; 4] = [
    "core.excludesFile=",
    "core.autocrlf=true",
    "core.checkStat=minimal",
    "core.trustctime=false",
];

/// `git status --porcelain` of every repository the bead reads, before and after.
fn statuses(root: &Path, repositories: &BTreeSet<String>) -> BTreeMap<String, String> {
    repositories
        .iter()
        .map(|name| {
            let mut args: Vec<&str> = Vec::new();
            for setting in STATUS_CONFIG {
                args.push("-c");
                args.push(setting);
            }
            args.extend(["status", "--porcelain"]);
            let out = legacy_git(&root.join(name), &args).output();
            let shown = match out {
                Ok(out) => format!(
                    "exit {:?}\n{}{}",
                    out.status.code(),
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                ),
                Err(e) => format!("cannot start git status: {e}"),
            };
            (name.clone(), shown)
        })
        .collect()
}

fn write_statuses(path: &Path, statuses: &BTreeMap<String, String>) {
    let mut text = String::new();
    for (name, status) in statuses {
        text.push_str(&format!("== {name}\n{status}\n"));
    }
    super::write(path, text);
}

/// This module's own blob ID of a file's bytes.
fn own_blob(path: &Path) -> Result<String, String> {
    let out = Command::new("git")
        .args(["hash-object", "--no-filters", "--"])
        .arg(path)
        .output()
        .map_err(|e| format!("cannot start git hash-object: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        Err(format!(
            "git hash-object exited with {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// This module's own blob ID of some bytes, through `git hash-object --stdin`.
fn own_blob_of(bytes: &[u8]) -> Result<String, String> {
    let mut child = Command::new("git")
        .args(["hash-object", "--no-filters", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start git hash-object: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("no standard input")?
        .write_all(bytes)
        .map_err(|e| format!("cannot write to git hash-object: {e}"))?;
    let out = child
        .wait_with_output()
        .map_err(|e| format!("git hash-object did not finish: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        Err(format!(
            "git hash-object exited with {:?}",
            out.status.code()
        ))
    }
}

/// SHA-256 (FIPS 180-4) as lower-case hex, this module's own, for the recorded capture digests.
fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bits.to_be_bytes());
    for block in message.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (state, add) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *state = state.wrapping_add(add);
        }
    }
    h.iter().map(|word| format!("{word:08x}")).collect()
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}

fn key_of(entry: &Value) -> String {
    format!("{}/{}", text(&entry["repository"]), text(&entry["path"]))
}

fn split_key(key: &str) -> (String, String) {
    key.split_once('/')
        .map(|(r, p)| (r.to_owned(), p.to_owned()))
        .unwrap_or_default()
}

/// Runs the xtask binary from the workspace root, keeping its transcript as `<label>.*`.
fn xtask(evidence: &Evidence, label: &str, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
    command.args(args).current_dir(repo_root());
    super::run_recorded(label, command, &evidence.dir)
}

fn scratch(case: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    super::remove_tree(&dir).unwrap_or_else(|e| panic!("{e}"));
    fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("cannot create {}: {e}", dir.display()));
    dir
}

fn list_of(value: &Value, key: &str) -> Vec<String> {
    value[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// The disposition the dispositions file gives `<repository>/<path>` at `blob`, by this test's
/// own reading of its entries: a module entry for the path (a different blob is at least
/// unresolved), else the first class whose selector matches.
fn own_disposition(file: &Value, repository: &str, path: &str, blob: &str) -> Option<String> {
    let modules = file["modules"].as_array().cloned().unwrap_or_default();
    if let Some(module) = modules
        .iter()
        .find(|m| text(&m["repository"]) == repository && text(&m["path"]) == path)
    {
        let reviewed = text(&module["disposition"]).to_owned();
        return Some(if text(&module["blob"]) == blob || reviewed == "tainted" {
            reviewed
        } else {
            "unresolved".to_owned()
        });
    }
    for class in file["classes"].as_array().into_iter().flatten() {
        let selector = &class["selector"];
        if text(&selector["repository"]) != repository {
            continue;
        }
        let (paths, prefixes, suffixes) = (
            list_of(selector, "paths"),
            list_of(selector, "prefixes"),
            list_of(selector, "suffixes"),
        );
        let by_prefix = prefixes.iter().any(|p| path.starts_with(p.as_str()))
            && (suffixes.is_empty() || suffixes.iter().any(|s| path.ends_with(s.as_str())));
        if paths.iter().any(|p| p == path) || by_prefix {
            return Some(text(&class["disposition"]).to_owned());
        }
    }
    None
}

fn non_cleared(disposition: Option<&str>) -> bool {
    disposition.is_some_and(|d| d != "cleared")
}

fn rank(disposition: &str) -> u8 {
    match disposition {
        "cleared" => 0,
        "unresolved" => 1,
        _ => 2,
    }
}

/// At least `floor`.
fn raise(current: &mut Option<String>, floor: &str) {
    let keep = current.as_deref().is_some_and(|c| rank(c) >= rank(floor));
    if !keep {
        *current = Some(floor.to_owned());
    }
}

/// What this test's own reading gives one input: its disposition and its calls_quarantined list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct OwnTag {
    disposition: Option<String>,
    calls: Vec<String>,
}

/// The tainted commits of the dispositions file, as `Repo:commit`.
fn tainted_commits(dispositions: &Value) -> BTreeSet<String> {
    dispositions["tainted_commits"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| format!("{}:{}", text(&t["repository"]), text(&t["commit"])))
        .collect()
}

/// Every input's numeric provenance by this test's own reading of the content-derivation rule
/// over the facts the manifest records: the dispositions file's entry for its pinned blob, else
/// unresolved when the pin quarantines it; at least unresolved for a citation sweep match, a
/// change by a listed tainted commit or an include macro the pin could not read; at least the
/// disposition of every file it includes (to a fixed point); and, as calls_quarantined, the files
/// of its repository it refers to that are tainted or unresolved, the crates of other
/// repositories it refers to that hold such a file, and its run-time loads.
fn own_tags(manifest: &Value, dispositions: &Value) -> BTreeMap<String, OwnTag> {
    let inputs = manifest["inputs"].as_array().cloned().unwrap_or_default();
    let by_key: BTreeMap<String, &Value> = inputs.iter().map(|e| (key_of(e), e)).collect();
    let tainted = tainted_commits(dispositions);
    let mut found: BTreeMap<String, Option<String>> = BTreeMap::new();
    for input in &inputs {
        let repository = text(&input["repository"]);
        let path = text(&input["path"]);
        let mut disposition = own_disposition(dispositions, repository, path, text(&input["blob"]))
            .or_else(|| {
                matches!(
                    text(&input["disposition"]),
                    "quarantined" | "quarantine_derived"
                )
                .then(|| "unresolved".to_owned())
            });
        let facts = &input["content_facts"];
        if facts["citations"].as_array().is_some_and(|c| !c.is_empty()) {
            raise(&mut disposition, "unresolved");
        }
        if list_of(facts, "changed_by")
            .iter()
            .any(|c| tainted.contains(&format!("{repository}:{c}")))
        {
            raise(&mut disposition, "unresolved");
        }
        let edges = &input["code_refs"];
        if !list_of(edges, "unread_includes").is_empty()
            || !list_of(edges, "test_unread_includes").is_empty()
        {
            raise(&mut disposition, "unresolved");
        }
        found.insert(key_of(input), disposition);
    }
    let target_of = |found: &BTreeMap<String, Option<String>>, repository: &str, path: &str| {
        let key = format!("{repository}/{path}");
        if by_key.contains_key(&key) {
            return found.get(&key).cloned().flatten();
        }
        own_disposition(dispositions, repository, path, "").or_else(|| {
            (repository == "OxFunc" && path.starts_with(NUMERIC_DIR))
                .then(|| "unresolved".to_owned())
        })
    };
    loop {
        let mut moved = false;
        for input in &inputs {
            let repository = text(&input["repository"]);
            let edges = &input["code_refs"];
            for target in list_of(edges, "includes")
                .into_iter()
                .chain(list_of(edges, "test_includes"))
            {
                if target == text(&input["path"]) {
                    continue;
                }
                let Some(d) = target_of(&found, repository, &target).filter(|d| d != "cleared")
                else {
                    continue;
                };
                let key = key_of(input);
                let mut current = found.get(&key).cloned().flatten();
                let before = current.clone();
                raise(&mut current, &d);
                if current != before {
                    found.insert(key, current);
                    moved = true;
                }
            }
        }
        if !moved {
            break;
        }
    }
    let crates = &manifest["numeric_content"]["library_crates"];
    let crate_bad = |label: &str| -> bool {
        let Some(record) = crates.get(label) else {
            return true;
        };
        let repository = text(&record["repository"]);
        let dir = format!("{}/", text(&record["src"]));
        let numeric = repository == "OxFunc" && NUMERIC_DIR.starts_with(&dir);
        let inputs_under = found.iter().any(|(key, d)| {
            let (r, p) = split_key(key);
            r == repository && p.starts_with(&dir) && non_cleared(d.as_deref())
        });
        let modules_under = dispositions["modules"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|m| {
                text(&m["repository"]) == repository
                    && text(&m["path"]).starts_with(&dir)
                    && text(&m["disposition"]) != "cleared"
            });
        let classes_under = dispositions["classes"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|c| {
                let s = &c["selector"];
                text(&s["repository"]) == repository
                    && text(&c["disposition"]) != "cleared"
                    && (list_of(s, "paths").iter().any(|p| p.starts_with(&dir))
                        || list_of(s, "prefixes")
                            .iter()
                            .any(|p| p.starts_with(&dir) || dir.starts_with(p.as_str())))
            });
        numeric || inputs_under || modules_under || classes_under
    };
    let mut out = BTreeMap::new();
    for input in &inputs {
        let repository = text(&input["repository"]);
        let path = text(&input["path"]);
        let edges = &input["code_refs"];
        let files: BTreeSet<String> = list_of(edges, "refs")
            .into_iter()
            .chain(list_of(edges, "test_refs"))
            .filter(|t| t != path && non_cleared(target_of(&found, repository, t).as_deref()))
            .collect();
        let foreign: BTreeSet<String> = list_of(edges, "crates")
            .into_iter()
            .chain(list_of(edges, "test_crates"))
            .filter(|label| crate_bad(label))
            .collect();
        let mut calls: Vec<String> = files.into_iter().chain(foreign).collect();
        if !list_of(edges, "runtime").is_empty() || !list_of(edges, "test_runtime").is_empty() {
            calls.push(format!("{repository} run-time loads"));
        }
        out.insert(
            key_of(input),
            OwnTag {
                disposition: found.get(&key_of(input)).cloned().flatten(),
                calls,
            },
        );
    }
    out
}

/// The inputs whose numeric provenance or calls_quarantined list differs between two derivations.
fn changed(before: &BTreeMap<String, OwnTag>, after: &BTreeMap<String, OwnTag>) -> Vec<String> {
    after
        .iter()
        .filter(|(key, tag)| before.get(*key) != Some(*tag))
        .map(|(key, _)| key.clone())
        .collect()
}

/// The allowlist entries against this test's derivation: every entry a port input at its pinned
/// blob outside the quarantined directory, admitted by the derivation, with exactly its list; and
/// every port input the derivation admits listed.
fn allowlist_problems(
    manifest: &Value,
    allowlist: &Value,
    own: &BTreeMap<String, OwnTag>,
) -> (Vec<String>, usize, usize) {
    let inputs = manifest["inputs"].as_array().cloned().unwrap_or_default();
    let by_key: BTreeMap<String, &Value> = inputs.iter().map(|e| (key_of(e), e)).collect();
    let listed: Vec<Value> = allowlist["entries"].as_array().cloned().unwrap_or_default();
    let listed_keys: BTreeSet<String> = listed.iter().map(key_of).collect();
    let mut problems = Vec::new();
    let mut with_list = 0;
    for entry in &listed {
        let key = key_of(entry);
        let Some(input) = by_key.get(&key) else {
            problems.push(format!("{key} is not a manifest input"));
            continue;
        };
        if input["disposition"] != "port" {
            problems.push(format!(
                "{key} has read disposition {}",
                input["disposition"]
            ));
        }
        if input["blob"] != entry["blob"] || input["commit"] != entry["commit"] {
            problems.push(format!("{key} is listed at another commit or blob"));
        }
        if text(&entry["repository"]) == "OxFunc" && text(&entry["path"]).starts_with(NUMERIC_DIR) {
            problems.push(format!(
                "{key} lies under the quarantined numeric directory"
            ));
        }
        let mine = own.get(&key).cloned().unwrap_or_default();
        if non_cleared(mine.disposition.as_deref()) {
            problems.push(format!("{key} is {mine:?} by this test's derivation"));
        }
        let calls = list_of(entry, "calls_quarantined");
        if !calls.is_empty() {
            with_list += 1;
        }
        if calls != mine.calls {
            problems.push(format!(
                "{key} is listed with calls_quarantined {calls:?} but this test derives {:?}",
                mine.calls
            ));
        }
    }
    let expected: BTreeSet<String> = inputs
        .iter()
        .filter(|i| {
            i["disposition"] == "port"
                && !non_cleared(own.get(&key_of(i)).and_then(|t| t.disposition.as_deref()))
        })
        .map(key_of)
        .collect();
    for key in expected.difference(&listed_keys) {
        problems.push(format!(
            "{key} is admitted by this test's derivation but not listed"
        ));
    }
    (problems, listed.len(), with_list)
}

/// Whether `pattern` occurs in `line` not directly after an ASCII letter, digit or underscore.
fn holds_pattern(line: &str, pattern: &str) -> bool {
    line.match_indices(pattern).any(|(at, _)| {
        line[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'))
    })
}

/// This test's own citation sweep: `git grep` of the patterns at each repository's pinned commit,
/// kept for the pinned inputs, each matching line with the first pattern (in the recorded order)
/// it holds at a word start: `Repo/path` to (line, pattern).
fn git_sweep(
    root: &Path,
    manifest: &Value,
    patterns: &[String],
) -> Result<BTreeMap<String, Vec<(u64, String)>>, String> {
    let inputs: BTreeSet<String> = manifest["inputs"]
        .as_array()
        .into_iter()
        .flatten()
        .map(key_of)
        .collect();
    let repositories: BTreeSet<String> = inputs.iter().map(|k| split_key(k).0).collect();
    let mut found: BTreeMap<String, Vec<(u64, String)>> = BTreeMap::new();
    for repository in repositories {
        let commit = manifest["repositories"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|r| r["name"] == repository.as_str())
            .map(|r| text(&r["commit"]).to_owned())
            .unwrap_or_default();
        let mut args: Vec<&str> = vec!["grep", "-z", "-n", "-I", "-F"];
        for pattern in patterns {
            args.push("-e");
            args.push(pattern);
        }
        args.push(&commit);
        args.push("--");
        let out = legacy_git(&root.join(&repository), &args)
            .output()
            .map_err(|e| format!("cannot start git grep: {e}"))?;
        // git grep exits 1 when nothing matches.
        if !out.status.success() && out.status.code() != Some(1) {
            return Err(format!(
                "git grep in {repository} exited with {:?}",
                out.status.code()
            ));
        }
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        for record in stdout.split('\n') {
            let mut fields = record.splitn(3, '\0');
            let (Some(named), Some(number), Some(line)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let path = named.split_once(':').map_or(named, |(_, p)| p);
            let key = format!("{repository}/{path}");
            if !inputs.contains(&key) {
                continue;
            }
            if let Some(pattern) = patterns.iter().find(|p| holds_pattern(line, p)) {
                found
                    .entry(key)
                    .or_default()
                    .push((number.parse().unwrap_or(0), pattern.clone()));
            }
        }
    }
    Ok(found)
}

/// This test's own touched-by-commit lists: for each listed tainted commit, the paths it changed
/// against its first parent (with rename detection), followed through later renames to the
/// pinned commit: `Repo/path` to the commits.
fn git_changed(
    root: &Path,
    manifest: &Value,
    dispositions: &Value,
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let mut found: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for tainted in dispositions["tainted_commits"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let repository = text(&tainted["repository"]);
        let commit = text(&tainted["commit"]);
        let dir = root.join(repository);
        let pinned = manifest["repositories"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|r| r["name"] == repository)
            .map(|r| text(&r["commit"]).to_owned())
            .unwrap_or_default();
        let changed = legacy_bytes(
            &dir,
            &[
                "diff-tree",
                "-r",
                "--root",
                "-M",
                "--no-commit-id",
                "--name-only",
                "-z",
                commit,
            ],
        )?;
        let mut paths: BTreeSet<String> = String::from_utf8_lossy(&changed)
            .split('\0')
            .filter(|p| !p.trim().is_empty())
            .map(|p| p.trim().to_owned())
            .collect();
        let renames = legacy_bytes(
            &dir,
            &[
                "log",
                "--reverse",
                "--format=",
                "-M",
                "--diff-filter=R",
                "--name-status",
                "-z",
                &format!("{commit}..{pinned}"),
            ],
        )?;
        let renames = String::from_utf8_lossy(&renames).into_owned();
        let mut fields = renames.split('\0').map(str::trim).filter(|f| !f.is_empty());
        while let Some(status) = fields.next() {
            if !status.starts_with('R') {
                continue;
            }
            let (Some(from), Some(to)) = (fields.next(), fields.next()) else {
                break;
            };
            if paths.contains(from) {
                paths.insert(to.to_owned());
            }
        }
        for path in paths {
            found
                .entry(format!("{repository}/{path}"))
                .or_default()
                .insert(commit.to_owned());
        }
    }
    Ok(found)
}

/// The manifest's recorded content facts against this test's own sweep and touched-by-commit
/// lists, input by input.
fn facts_problems(
    manifest: &Value,
    sweep: &BTreeMap<String, Vec<(u64, String)>>,
    touched: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<String> {
    let mut problems = Vec::new();
    for input in manifest["inputs"].as_array().into_iter().flatten() {
        let key = key_of(input);
        let facts = &input["content_facts"];
        let recorded: Vec<(u64, String)> = facts["citations"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| {
                (
                    c["line"].as_u64().unwrap_or(0),
                    text(&c["pattern"]).to_owned(),
                )
            })
            .collect();
        let mine = sweep.get(&key).cloned().unwrap_or_default();
        if recorded != mine {
            problems.push(format!(
                "{key}: the manifest records citations {recorded:?}, this test's git grep finds {mine:?}"
            ));
        }
        let recorded: BTreeSet<String> = list_of(facts, "changed_by").into_iter().collect();
        let mine = touched.get(&key).cloned().unwrap_or_default();
        if recorded != mine {
            problems.push(format!(
                "{key}: the manifest records changed_by {recorded:?}, this test's git diff-tree finds {mine:?}"
            ));
        }
    }
    problems
}

/// The library crates of a repository at a commit, by this test's own reading of its tree: crate
/// name (directory name, `-` read as `_`) to its `src` directory.
fn library_crates(dir: &Path, commit: &str) -> BTreeMap<String, String> {
    let listing = legacy_read(dir, &["ls-tree", "-r", "--name-only", commit]).unwrap_or_default();
    let files: BTreeSet<&str> = listing.lines().collect();
    files
        .iter()
        .filter_map(|path| path.strip_suffix("/src/lib.rs"))
        .filter(|crate_dir| files.contains(format!("{crate_dir}/Cargo.toml").as_str()))
        .map(|crate_dir| {
            (
                crate_dir
                    .rsplit('/')
                    .next()
                    .unwrap_or(crate_dir)
                    .replace('-', "_"),
                format!("{crate_dir}/src"),
            )
        })
        .collect()
}

/// A spot check of the cross-repository edges: for every library crate of OxFunc that holds the
/// quarantined directory or a tainted or unresolved input, this test's own `git grep` for
/// `<crate>::` and `use <crate>` (comment lines left out) in the Rust files of every other
/// repository's pinned commit; a Rust port or reference input there records the crate under
/// crates or test_crates exactly when the grep finds it. Returns the problems, the crates checked
/// and the inputs that record one.
fn cross_repository_problems(
    root: &Path,
    manifest: &Value,
    own: &BTreeMap<String, OwnTag>,
) -> (Vec<String>, Vec<String>, usize) {
    let commit_of = |name: &str| {
        manifest["repositories"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|r| r["name"] == name)
            .map(|r| text(&r["commit"]).to_owned())
            .unwrap_or_default()
    };
    let oxfunc = commit_of("OxFunc");
    let mut problems = Vec::new();
    let mut checked = Vec::new();
    let mut recording = 0;
    let inputs = manifest["inputs"].as_array().cloned().unwrap_or_default();
    for (name, src) in library_crates(&root.join("OxFunc"), &oxfunc) {
        let dir = format!("{src}/");
        let relevant = NUMERIC_DIR.starts_with(&dir)
            || own.iter().any(|(key, tag)| {
                let (r, p) = split_key(key);
                r == "OxFunc" && p.starts_with(&dir) && non_cleared(tag.disposition.as_deref())
            });
        if !relevant {
            continue;
        }
        let label = format!("OxFunc crate {name}");
        checked.push(label.clone());
        let repositories: BTreeSet<String> = inputs
            .iter()
            .map(|i| text(&i["repository"]).to_owned())
            .filter(|r| r != "OxFunc")
            .collect();
        for repository in repositories {
            let commit = commit_of(&repository);
            let separator = format!("{name}::");
            let used = format!("use {name}");
            let out = legacy_git(
                &root.join(&repository),
                &[
                    "grep", "-z", "-n", "-F", "-e", &separator, "-e", &used, &commit, "--", "*.rs",
                ],
            )
            .output();
            let stdout = out
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                .unwrap_or_default();
            let mut hits: BTreeSet<String> = BTreeSet::new();
            for record in stdout.split('\n') {
                let mut fields = record.splitn(3, '\0');
                let (Some(named), Some(_), Some(line)) =
                    (fields.next(), fields.next(), fields.next())
                else {
                    continue;
                };
                if line.trim_start().starts_with("//") {
                    continue;
                }
                let path = named.split_once(':').map_or(named, |(_, p)| p);
                hits.insert(path.to_owned());
            }
            for input in inputs.iter().filter(|i| {
                text(&i["repository"]) == repository
                    && text(&i["path"]).ends_with(".rs")
                    && matches!(text(&i["disposition"]), "port" | "reference")
            }) {
                let edges = &input["code_refs"];
                let records = list_of(edges, "crates")
                    .into_iter()
                    .chain(list_of(edges, "test_crates"))
                    .any(|l| l == label);
                if records {
                    recording += 1;
                }
                let found = hits.contains(text(&input["path"]));
                if records != found {
                    problems.push(format!(
                        "{}: the manifest {} {label} but this test's git grep {} it",
                        key_of(input),
                        if records {
                            "records"
                        } else {
                            "does not record"
                        },
                        if found { "finds" } else { "does not find" }
                    ));
                }
            }
        }
    }
    (problems, checked, recording)
}

/// The canonical form of the dispositions as reviewed, by this test's own reading: every module
/// and class entry without an after_review mark as it stands, a changed one as its reviewed_as
/// record, an added one left out; the deciding fields only, sorted by kind and id.
fn own_reviewed_view(file: &Value) -> Value {
    let mut view: Vec<Value> = Vec::new();
    for (kind, key) in [("module", "modules"), ("class", "classes")] {
        for entry in file[key].as_array().into_iter().flatten() {
            let mark = &entry["after_review"];
            let base = match mark["kind"].as_str() {
                Some("added") => continue,
                Some(_) => &mark["reviewed_as"],
                None => entry,
            };
            let mut record = serde_json::Map::new();
            record.insert("kind".to_owned(), json!(kind));
            record.insert("id".to_owned(), entry["id"].clone());
            record.insert("disposition".to_owned(), base["disposition"].clone());
            for field in ["repository", "path", "blob", "selector"] {
                if !base[field].is_null() {
                    record.insert(field.to_owned(), base[field].clone());
                }
            }
            if let Some(groups) = base["symbols"].as_array() {
                record.insert(
                    "symbols".to_owned(),
                    Value::Array(
                        groups
                            .iter()
                            .map(|g| json!({"names": g["names"], "disposition": g["disposition"]}))
                            .collect(),
                    ),
                );
            }
            view.push(Value::Object(record));
        }
    }
    view.sort_by(|a, b| {
        (text(&a["kind"]), text(&a["id"])).cmp(&(text(&b["kind"]), text(&b["id"])))
    });
    Value::Array(view)
}

/// What a fixture input must show: its key, read disposition, numeric provenance, whether the
/// allowlist lists it, and an entry its calls_quarantined list must hold.
type FixtureWanted<'a> = (&'a str, &'a str, Option<&'a str>, bool, Option<String>);

/// What the numeric-provenance check of a receipt found, or `None` when the receipt has no such
/// check.
fn check_of(run: &Accepted) -> Option<(bool, Vec<String>)> {
    let check = run.receipt["architecture"]
        .as_array()?
        .iter()
        .find(|c| c["check"] == CHECK)?;
    let findings = check["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    Some((check["passed"] == true, findings))
}

/// A hand edit of a workspace copy, the accept run after it, and what that run must show: the
/// numeric-provenance check fails with a finding for which `wanted` holds, every finding names
/// one of `named`, and every other architecture check passes.
fn rejected(
    copy: &WorkspaceCopy,
    label: &str,
    named: &[String],
    wanted: &dyn Fn(&str) -> bool,
    what: &str,
    evidence: &mut Evidence,
) {
    let host = host_platform();
    let args = ["--id", "fixture-pass", "--platform", host, "--require-all"];
    let run = copy.accept(&args, label, true, evidence);
    let found = check_of(&run);
    let findings = found.clone().map(|(_, f)| f).unwrap_or_default();
    let names_only = findings
        .iter()
        .all(|f| named.iter().any(|name| f.contains(name.as_str())));
    let has_wanted = findings.iter().any(|f| wanted(f));
    evidence.expect(
        run.exit_code == Some(1)
            && run.result() == "failed"
            && found.as_ref().is_some_and(|(passed, _)| !passed)
            && has_wanted
            && names_only
            && run.checks_ok_except(&[CHECK]),
        format!(
            "{label}: {what}; accept exits {:?} with result {}, the {CHECK} check fails and names only {named:?}, and the other architecture checks pass; findings: {findings:?}",
            run.exit_code,
            run.result()
        ),
    );
}

fn write_json(path: &Path, value: &Value) {
    super::write(
        path,
        serde_json::to_string_pretty(value).expect("serialise"),
    );
}

/// infra_m0_5__quarantine_gate: no entry marked unresolved or tainted enters the release source
/// allowlist, and a file that only calls one enters it only with its calls_quarantined list. The
/// committed record is read independently: the allowlist and the manifest's recorded provenance
/// against this test's own derivation of the content-derivation rule; the manifest's content
/// facts against this test's own git citation sweep and touched-by-commit lists; the
/// cross-repository edges spot-checked by its own git grep; each quoted provenance line or value
/// found in its pinned blob or commit message; the review record and its hash recomputed; the
/// owners named; the clean room kept. The generator, run over a fixture tracker and fixture
/// dispositions, admits a pure caller with its list and a cross-repository caller with a
/// crate-level list and leaves out a file changed by a tainted commit and a file that includes a
/// tainted file. Hand edits of workspace copies fail the numeric-provenance check of
/// `cargo xtask accept`, which passes on the unaltered copy.
pub fn quarantine() {
    const CASE: &str = "infra_m0_5__quarantine_gate";
    registered(CASE);
    let mut evidence = Evidence::for_bead(BEAD, CASE);
    let (root, chosen) = legacy_root();
    if !root.is_dir() {
        let reason = format!("the legacy root ({chosen}) does not exist on this host");
        println!("ACCEPT-NOT-RUN: {CASE}: {reason}");
        evidence.lines.push(format!("not run: {reason}"));
        evidence.finish();
        return;
    }
    evidence.lines.push(format!("legacy root: {chosen}"));
    let read_repositories: BTreeSet<String> = ["OxFunc", "OxFml"].map(str::to_owned).into();
    let before = statuses(&root, &read_repositories);
    write_statuses(&evidence.dir.join("legacy-status-before.txt"), &before);
    let tracked: Vec<(String, Option<Vec<u8>>)> = [MANIFEST, DISPOSITIONS, ALLOWLIST]
        .iter()
        .map(|rel| ((*rel).to_owned(), fs::read(repo_root().join(rel)).ok()))
        .collect();

    let manifest = super::read_json(&repo_root().join(MANIFEST));
    let dispositions = super::read_json(&repo_root().join(DISPOSITIONS));
    let allowlist = super::read_json(&repo_root().join(ALLOWLIST));
    let inputs = manifest["inputs"].as_array().cloned().unwrap_or_default();
    let by_key: BTreeMap<String, &Value> = inputs.iter().map(|e| (key_of(e), e)).collect();
    let own = own_tags(&manifest, &dispositions);

    // 1. The committed allowlist and the manifest's recorded provenance against this test's own
    //    derivation.
    let (problems, listed_count, with_list) = allowlist_problems(&manifest, &allowlist, &own);
    evidence.expect(
        listed_count > 0 && with_list > 0 && problems.is_empty(),
        format!(
            "{ALLOWLIST}: all {listed_count} entries are port inputs at their pinned blob, none under {NUMERIC_DIR}, exactly the port inputs this test's own derivation of the content-derivation rule leaves without a tainted or unresolved provenance, and the {with_list} entries with a calls_quarantined list carry exactly the list this test derives; problems: {:?}",
            problems.iter().take(10).collect::<Vec<_>>()
        ),
    );
    let mut recorded_problems = Vec::new();
    let mut lists = 0;
    for input in &inputs {
        let key = key_of(input);
        let mine = own.get(&key).cloned().unwrap_or_default();
        let recorded = &input["numeric_provenance"];
        if recorded["disposition"].as_str() != mine.disposition.as_deref() {
            recorded_problems.push(format!(
                "{key}: recorded {} but derived {:?}",
                recorded["disposition"], mine.disposition
            ));
        }
        let calls = list_of(recorded, "calls_quarantined");
        if !calls.is_empty() {
            lists += 1;
        }
        if calls != mine.calls {
            recorded_problems.push(format!(
                "{key}: recorded calls_quarantined {calls:?} but derived {:?}",
                mine.calls
            ));
        }
    }
    evidence.expect(
        recorded_problems.is_empty() && lists > 0,
        format!(
            "{MANIFEST}: every input's recorded numeric provenance, and the {lists} calls_quarantined lists, equal this test's own derivation; problems: {:?}",
            recorded_problems.iter().take(10).collect::<Vec<_>>()
        ),
    );

    // 2. The facts the derivation rests on, recomputed from git: the citation sweep and the
    //    touched-by-commit lists; and a spot check of the cross-repository edges.
    let patterns = list_of(&dispositions["citation_sweep"], "patterns");
    let tainted = tainted_commits(&dispositions);
    let recorded_content = &manifest["numeric_content"];
    let recorded_commits: BTreeSet<String> = recorded_content["tainted_commits"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| format!("{}:{}", text(&t["repository"]), text(&t["commit"])))
        .collect();
    evidence.expect(
        patterns.iter().any(|p| p == "C-runtime")
            && list_of(&recorded_content["citation_sweep"], "patterns") == patterns
            && recorded_commits == tainted
            && tainted.len() >= 5,
        format!(
            "{MANIFEST}'s numeric_content was computed with the {} sweep patterns of {DISPOSITIONS} (C-runtime among them) and its {} tainted commits",
            patterns.len(),
            tainted.len()
        ),
    );
    let named_commits = ["a4e94d1e", "af79125b", "5d991255", "853675ee", "85e91e45"];
    evidence.expect(
        named_commits.iter().all(|c| {
            tainted
                .iter()
                .any(|t| t.starts_with(&format!("OxFunc:{c}")))
        }),
        format!("{DISPOSITIONS} lists the tainted commits {named_commits:?} among {tainted:?}"),
    );
    match (
        git_sweep(&root, &manifest, &patterns),
        git_changed(&root, &manifest, &dispositions),
    ) {
        (Ok(sweep), Ok(touched)) => {
            let problems = facts_problems(&manifest, &sweep, &touched);
            let swept = sweep.len();
            let changed_inputs = touched.keys().filter(|k| by_key.contains_key(*k)).count();
            evidence.expect(
                problems.is_empty() && swept > 0 && changed_inputs > 0,
                format!(
                    "every input's recorded content_facts equal this test's own git grep sweep ({swept} inputs with a match) and its own git diff-tree touched-by-commit lists ({changed_inputs} inputs changed by a listed commit); problems: {:?}",
                    problems.iter().take(10).collect::<Vec<_>>()
                ),
            );
            // A coordinated edit of the facts themselves passes the check of cargo xtask accept
            // (it trusts the recorded facts); this test's own recomputation catches it.
            let corpus =
                "OxFunc/crates/oxfunc_core/tests/fixtures/oxfunc_adapter_function_corpus.json";
            let mut altered = manifest.clone();
            for input in altered["inputs"].as_array_mut().expect("inputs") {
                if key_of(input) == corpus {
                    let object = input.as_object_mut().expect("an input");
                    object.remove("content_facts");
                    object.remove("numeric_provenance");
                }
            }
            let caught = facts_problems(&altered, &sweep, &touched);
            evidence.expect(
                caught.len() == 1 && caught[0].starts_with(&format!("{corpus}: ")),
                format!(
                    "a manifest whose facts for {corpus} were removed together with its provenance is caught by this test's recomputation of the facts: {caught:?}"
                ),
            );
        }
        (sweep, touched) => evidence.expect(
            false,
            format!(
                "this test's own git sweep and touched-by-commit lists ran: {:?} / {:?}",
                sweep.err(),
                touched.err()
            ),
        ),
    }
    let mut crate_problems = Vec::new();
    for (label, record) in recorded_content["library_crates"]
        .as_object()
        .into_iter()
        .flatten()
    {
        let repository = text(&record["repository"]);
        let commit = manifest["repositories"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|r| r["name"] == repository)
            .map(|r| text(&r["commit"]).to_owned())
            .unwrap_or_default();
        let crates = library_crates(&root.join(repository), &commit);
        let name = label
            .split_once(" crate ")
            .map_or("", |(_, n)| n)
            .to_owned();
        if crates.get(&name).map(String::as_str) != Some(text(&record["src"])) {
            crate_problems.push(format!(
                "{label}: {record} is not a library crate at {commit}"
            ));
        }
    }
    let (cross, checked, recording) = cross_repository_problems(&root, &manifest, &own);
    crate_problems.extend(cross);
    evidence.expect(
        crate_problems.is_empty() && recording > 0 && !checked.is_empty(),
        format!(
            "the library crates numeric_content names are library crates (src/lib.rs beside a Cargo.toml) at the pinned commits, and for {checked:?} the {recording} Rust inputs of other repositories that record the crate are exactly those whose code (comment lines left out) names it, by this test's own git grep; problems: {crate_problems:?}"
        ),
    );

    // 3. Every candidate the bead names has a recorded disposition and stays out. The outcomes
    //    asserted here (the seven files tainted or unresolved and quarantined, the research
    //    packages quarantine_derived and tainted, the production replays tainted) are today's
    //    record: a cleared entry does not lift the pin's quarantine of an excel_numeric path, and
    //    bead dc-ime9 owns the route that would, with fixtures in place of these assertions.
    let modules: Vec<Value> = dispositions["modules"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let listed_keys: BTreeSet<String> = allowlist["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .map(key_of)
        .collect();
    let mut module_problems = Vec::new();
    for file in NUMERIC_FILES {
        let path = format!("{NUMERIC_DIR}{file}");
        let key = format!("OxFunc/{path}");
        let module = modules
            .iter()
            .find(|m| text(&m["repository"]) == "OxFunc" && text(&m["path"]) == path);
        match (module, by_key.get(&key)) {
            (Some(module), Some(input)) => {
                let d = text(&module["disposition"]);
                if !matches!(d, "tainted" | "unresolved") {
                    module_problems.push(format!("{file}: disposition {d}"));
                }
                if module["blob"] != input["blob"] {
                    module_problems.push(format!(
                        "{file}: reviewed blob {} but pinned {}",
                        module["blob"], input["blob"]
                    ));
                }
                if text(&module["reason"]).is_empty()
                    || module["evidence"].as_array().is_none_or(Vec::is_empty)
                {
                    module_problems.push(format!("{file}: no reason or evidence"));
                }
                let recorded = text(&input["numeric_provenance"]["disposition"]);
                if rank(recorded) < rank(d) || input["disposition"] != "quarantined" {
                    module_problems.push(format!(
                        "{file}: the manifest records {} / {recorded}",
                        input["disposition"]
                    ));
                }
                evidence.lines.push(format!(
                    "{file}: {d} ({}), recorded {recorded}",
                    text(&module["id"])
                ));
            }
            _ => module_problems.push(format!("{file}: no module entry or no pinned input")),
        }
    }
    evidence.expect(
        module_problems.is_empty(),
        format!(
            "each of the seven files of OxFunc/{NUMERIC_DIR} has a module entry at its pinned blob, tainted or unresolved, with a reason and quoted evidence, and the manifest keeps it quarantined with at least that numeric provenance (today's outcome; dc-ime9 owns the route by which a cleared entry would lift the quarantine); problems: {module_problems:?}"
        ),
    );
    let mut class_problems = Vec::new();
    let mut research = 0;
    let mut replays = 0;
    for input in &inputs {
        let key = key_of(input);
        let path = text(&input["path"]);
        let numeric = input["numeric_provenance"]["disposition"].as_str();
        let oxfunc = text(&input["repository"]) == "OxFunc";
        if oxfunc && RESEARCH_PACKAGES.iter().any(|p| path.starts_with(p)) {
            research += 1;
            if input["disposition"] != "quarantine_derived" || numeric != Some("tainted") {
                class_problems.push(format!("{key}: {} / {numeric:?}", input["disposition"]));
            }
        }
        if path.ends_with("-production-replay.json") {
            replays += 1;
            if numeric != Some("tainted") {
                class_problems.push(format!("{key}: numeric provenance {numeric:?}"));
            }
        }
        if non_cleared(numeric) && listed_keys.contains(&key) {
            class_problems.push(format!("{key} is listed"));
        }
    }
    evidence.expect(
        research > 0 && replays > 0 && class_problems.is_empty(),
        format!(
            "the {research} inputs of the research-x87 packages {RESEARCH_PACKAGES:?} are quarantine_derived and tainted and the {replays} production-replay files are tainted (today's outcome; dc-ime9 owns the route that would change it), and no tainted or unresolved input is listed; problems: {:?}",
            class_problems.iter().take(10).collect::<Vec<_>>()
        ),
    );
    let odd_bond = "crates/oxfunc_core/src/functions/odd_bond_family.rs";
    let odd = modules
        .iter()
        .find(|m| text(&m["path"]) == odd_bond)
        .cloned()
        .unwrap_or(Value::Null);
    let odd_lines: BTreeSet<u64> = odd["evidence"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| e["line"].as_u64())
        .collect();
    evidence.expect(
        odd["disposition"] == "tainted"
            && odd["after_review"]["kind"] == "added"
            && [392, 393, 394, 397, 1047].iter().all(|l| odd_lines.contains(l))
            && by_key
                .get(&format!("OxFunc/{odd_bond}"))
                .is_some_and(|i| i["blob"] == odd["blob"])
            && !listed_keys.contains(&format!("OxFunc/{odd_bond}")),
        format!(
            "OxFunc/{odd_bond} has a tainted module entry at its pinned blob, marked as added after the review, quoting lines 392-394 and 397 (its own copy of bond_core_family::excel_bond_pow) and 1047 (the C-runtime pow), and is not listed: {}",
            odd["id"]
        ),
    );

    // 4. Every quoted line or value of the dispositions file is in its pinned blob or commit.
    let mut quotes = 0;
    let mut quote_problems = Vec::new();
    let mut entries: Vec<(&Value, Option<String>, Option<String>)> = modules
        .iter()
        .map(|m| {
            (
                m,
                Some(text(&m["repository"]).to_owned()),
                Some(text(&m["blob"]).to_owned()),
            )
        })
        .collect();
    for class in dispositions["classes"].as_array().into_iter().flatten() {
        entries.push((
            class,
            Some(text(&class["selector"]["repository"]).to_owned()),
            None,
        ));
    }
    for (entry, repository, blob) in entries {
        for item in entry["evidence"].as_array().into_iter().flatten() {
            quotes += 1;
            let repository = item["repository"]
                .as_str()
                .map(str::to_owned)
                .or(repository.clone())
                .unwrap_or_default();
            let dir = root.join(&repository);
            let quote = text(&item["quote"]);
            let blob = item["blob"]
                .as_str()
                .map(str::to_owned)
                .or(blob.clone())
                .unwrap_or_default();
            let found = match text(&item["kind"]) {
                "commit" => {
                    legacy_read(&dir, &["show", "-s", "--format=%B", text(&item["commit"])])
                        .map(|message| message.contains(quote))
                }
                "data" => legacy_bytes(&dir, &["cat-file", "blob", &blob]).map(|bytes| {
                    serde_json::from_slice::<Value>(&bytes)
                        .ok()
                        .and_then(|data| data.pointer(text(&item["pointer"])).cloned())
                        .is_some_and(|value| value == item["value"])
                }),
                _ => {
                    let line = item["line"].as_u64().unwrap_or(0) as usize;
                    legacy_bytes(&dir, &["cat-file", "blob", &blob]).map(|bytes| {
                        String::from_utf8_lossy(&bytes)
                            .split('\n')
                            .nth(line.saturating_sub(1))
                            .is_some_and(|l| l.contains(quote))
                    })
                }
            };
            if found != Ok(true) {
                quote_problems.push(format!(
                    "{} {}: {quote:?} {} ({found:?})",
                    text(&entry["id"]),
                    text(&item["kind"]),
                    text(&item["pointer"])
                ));
            }
        }
    }
    for item in dispositions["tainted_commits"]
        .as_array()
        .into_iter()
        .flatten()
    {
        quotes += 1;
        let quote = text(&item["quote"]);
        let found = legacy_read(
            &root.join(text(&item["repository"])),
            &["show", "-s", "--format=%B", text(&item["commit"])],
        )
        .map(|message| !quote.is_empty() && message.contains(quote));
        if found != Ok(true) {
            quote_problems.push(format!(
                "tainted commit {}: {quote:?}",
                text(&item["commit"])
            ));
        }
    }
    evidence.expect(
        quotes > 0 && quote_problems.is_empty(),
        format!(
            "all {quotes} quoted evidence lines and values of {DISPOSITIONS}, and the quote of every listed tainted commit, appear at their stated line or JSON pointer of the pinned blob, or in the commit message, read with this test's own git; problems: {quote_problems:?}"
        ),
    );

    // 5. The review: the dispositions match the review file entry by entry, the record repeats
    //    its verdict counts, the owner's confirmation is pending or recorded, the hash of the
    //    dispositions as reviewed is this test's own, and every entry changed after the review is
    //    tainted or unresolved.
    let record = &dispositions["status"]["review"];
    let review_rel = text(&record["outcome"]["file"]).to_owned();
    let review = if review_rel.is_empty() {
        Value::Null
    } else {
        super::read_json(&repo_root().join(&review_rel))
    };
    let mut review_problems = Vec::new();
    let by_id: BTreeMap<String, String> = modules
        .iter()
        .chain(dispositions["classes"].as_array().into_iter().flatten())
        .map(|e| {
            (
                text(&e["id"]).to_owned(),
                text(&e["disposition"]).to_owned(),
            )
        })
        .collect();
    let mut compared = 0;
    let mut replaced: Vec<String> = Vec::new();
    let mut tally: BTreeMap<String, u64> = BTreeMap::new();
    for entry in review["entries"].as_array().into_iter().flatten() {
        *tally.entry(text(&entry["verdict"]).to_owned()).or_default() += 1;
        let word: String = text(&entry["proposed_disposition"])
            .chars()
            .take_while(char::is_ascii_alphabetic)
            .collect::<String>()
            .to_ascii_lowercase();
        if !matches!(word.as_str(), "cleared" | "tainted" | "unresolved") {
            continue;
        }
        let id = text(&entry["id"]);
        match by_id.get(id) {
            None => replaced.push(id.to_owned()),
            Some(found) => {
                compared += 1;
                if *found != word {
                    review_problems.push(format!("{id}: file {found}, review proposes {word}"));
                }
            }
        }
    }
    let outcome = &record["outcome"];
    for verdict in ["confirmed", "confirmed_with_changes", "rejected"] {
        if outcome[verdict] != json!(tally.get(verdict).copied().unwrap_or(0)) {
            review_problems.push(format!(
                "{verdict}: recorded {} but the review holds {:?}",
                outcome[verdict],
                tally.get(verdict)
            ));
        }
    }
    let confirmation = &record["owner_confirmation_of_cleared_witness_sources"];
    let confirmed = confirmation == "pending"
        || (!text(&confirmation["confirmer"]).trim().is_empty()
            && !text(&confirmation["date"]).trim().is_empty());
    if !confirmed
        || text(&record["reviewer"]).trim().is_empty()
        || text(&record["date"]).trim().is_empty()
    {
        review_problems.push(format!("the review record is {record}"));
    }
    let view = own_reviewed_view(&dispositions);
    let hash = own_blob_of(view.to_string().as_bytes());
    if hash.as_deref().ok() != record["dispositions_hash"]["value"].as_str()
        || record["dispositions_hash"]["algorithm"] != "git-blob-sha1"
    {
        review_problems.push(format!(
            "this test hashes the dispositions as reviewed to {hash:?}, the record holds {}",
            record["dispositions_hash"]
        ));
    }
    let late: Vec<String> = modules
        .iter()
        .chain(dispositions["classes"].as_array().into_iter().flatten())
        .filter(|e| e["after_review"].is_object())
        .map(|e| format!("{} ({})", text(&e["id"]), text(&e["disposition"])))
        .collect();
    if late.iter().any(|l| l.ends_with("(cleared)")) {
        review_problems.push(format!(
            "an entry changed after the review is cleared: {late:?}"
        ));
    }
    if replaced != ["numeric-callers"] || dispositions["rules"].as_array().map(Vec::len) != Some(5)
    {
        review_problems.push(format!(
            "the review entries whose id the file no longer holds are {replaced:?}, not just numeric-callers (replaced by the rules of the ruling)"
        ));
    }
    // The x87 symbols the review settles, and the excel_atan reason.
    let symbols = |id: &str| -> BTreeMap<String, String> {
        modules
            .iter()
            .find(|m| m["id"] == id)
            .map(|m| {
                m["symbols"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|group| {
                        list_of(group, "names")
                            .into_iter()
                            .map(|name| (name, text(&group["disposition"]).to_owned()))
                            .collect::<Vec<_>>()
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let x87 = symbols("x87");
    for name in ["add", "sub", "mul", "div", "recip", "raw"] {
        if x87.get(name).map(String::as_str) != Some("tainted") {
            review_problems.push(format!("x87::{name} is {:?}", x87.get(name)));
        }
    }
    if x87.contains_key("log2") || x87.get("atan").map(String::as_str) != Some("unresolved") {
        review_problems.push(format!("x87 symbols {x87:?}"));
    }
    let numeric_mod = symbols("excel-numeric-mod");
    if numeric_mod.get("excel_atan").map(String::as_str) != Some("unresolved") {
        review_problems.push(format!("excel_atan is {:?}", numeric_mod.get("excel_atan")));
    }
    evidence.expect(
        compared >= 14 && review_problems.is_empty(),
        format!(
            "{DISPOSITIONS} holds the review record of {review_rel} (reviewer and date named, owner confirmation {confirmation}), whose verdict counts {tally:?} it repeats; the {compared} review entries that propose a disposition for an entry the file holds match it ({replaced:?} is replaced by the ruling's rules); this test's own hash of the dispositions as reviewed equals the recorded {}; the entries changed after the review ({late:?}) are tainted or unresolved; x87's add, sub, mul, div, recip and raw are tainted, log2 is gone, and x87::atan and excel_atan are unresolved; problems: {review_problems:?}",
            record["dispositions_hash"]["value"]
        ),
    );

    // 6. Owners: every rule, every class that selects by prefix and every unresolved entry names
    //    one; the gaps name the owners of the deferred work; the manifest counts the five unread
    //    include macros.
    let mut owner_problems = Vec::new();
    for rule in dispositions["rules"].as_array().into_iter().flatten() {
        if text(&rule["owner"]["bead"]).is_empty() {
            owner_problems.push(format!("rule {}", rule["id"]));
        }
    }
    let rule_ids: BTreeSet<String> = dispositions["rules"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| text(&r["id"]).to_owned())
        .collect();
    if rule_ids != RULES.iter().map(|r| (*r).to_owned()).collect() {
        owner_problems.push(format!("rules {rule_ids:?}"));
    }
    for entry in &modules {
        if entry["disposition"] == "unresolved" && text(&entry["owner"]["bead"]).is_empty() {
            owner_problems.push(format!("module {}", entry["id"]));
        }
    }
    for class in dispositions["classes"].as_array().into_iter().flatten() {
        let wide = !list_of(&class["selector"], "prefixes").is_empty()
            || class["disposition"] == "unresolved";
        if wide && text(&class["owner"]["bead"]).is_empty() {
            owner_problems.push(format!("class {}", class["id"]));
        }
    }
    let gap_owners: BTreeSet<String> = dispositions["gaps"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|g| text(&g["owner"]).to_owned())
        .collect();
    for owner in OWNERS {
        if !gap_owners.contains(owner) {
            owner_problems.push(format!("no gap names {owner}"));
        }
    }
    let ground_truth = modules
        .iter()
        .find(|m| m["id"] == "excel-numeric-ground-truth")
        .map(|m| text(&m["owner"]["bead"]).to_owned());
    if ground_truth.as_deref() != Some("dc-2203") {
        owner_problems.push(format!(
            "the ground-truth entry's owner is {ground_truth:?}"
        ));
    }
    let skipped = &manifest["summary"]["resolution_skipped"]["an include macro whose argument is not a string literal is not resolved"];
    let unread_note = manifest["notes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .find(|n| n.contains("an include macro whose argument is not a string literal"))
        .unwrap_or_default()
        .to_owned();
    let unread_inputs: BTreeSet<String> = inputs
        .iter()
        .filter(|i| {
            !list_of(&i["code_refs"], "unread_includes").is_empty()
                || !list_of(&i["code_refs"], "test_unread_includes").is_empty()
        })
        .map(key_of)
        .collect();
    let wanted_unread: BTreeSet<String> = UNREAD_INCLUDES.iter().map(|k| (*k).to_owned()).collect();
    if *skipped != json!(5) || unread_inputs != wanted_unread {
        owner_problems.push(format!(
            "resolution_skipped counts {skipped} unread include macros and the inputs holding one are {unread_inputs:?}"
        ));
    }
    let summary = &manifest["summary"]["numeric_provenance"];
    let unpinned = &summary["edges_into_files_not_pinned"];
    if summary["edges_owner"] != "dc-4uy4" || !unpinned["OxFunc"].is_u64() {
        owner_problems.push(format!("the manifest's edge counts {summary}"));
    }
    evidence.expect(
        owner_problems.is_empty(),
        format!(
            "every rule {RULES:?}, every class that selects by prefix and every unresolved entry of {DISPOSITIONS} names its owner bead, the gaps name {OWNERS:?} (owners {gap_owners:?}), the ground-truth table's owner is dc-2203, summary.resolution_skipped counts the 5 unread include macros of {UNREAD_INCLUDES:?} ({unread_note:.200}), and the manifest counts the edges into files that are not pinned inputs ({unpinned}) for owner dc-4uy4; problems: {owner_problems:?}"
        ),
    );

    // 7. The clean room: neither provenance file holds a staging formula token, and the review
    //    file says its redactions are the only change from the reviewer's output.
    let dispositions_text = fs::read_to_string(repo_root().join(DISPOSITIONS)).unwrap_or_default();
    let review_text = fs::read_to_string(repo_root().join(&review_rel)).unwrap_or_default();
    let tokens: Vec<String> = STAGING_TOKENS
        .iter()
        .filter(|t| dispositions_text.contains(**t) || review_text.contains(**t))
        .map(|t| (*t).to_owned())
        .collect();
    let header = text(&review["clean_room_redactions"]);
    // The header quotes the mark once; every other occurrence marks a redaction.
    let marks =
        review_text.matches(REDACTION_MARK).count() - header.matches(REDACTION_MARK).count();
    evidence.expect(
        tokens.is_empty()
            && marks > 0
            && header.contains(&format!("These {marks} redactions are the only change")),
        format!(
            "neither {DISPOSITIONS} nor {review_rel} holds any of {STAGING_TOKENS:?} (found {tokens:?}); the review file marks {marks} redactions {REDACTION_MARK:?} and its clean_room_redactions header says they are the only change from the reviewer's verbatim output"
        ),
    );

    // 8. The lookup route prints the disposition and exits as AGENTS.md says.
    for (label, wanted, codes, line) in [
        (
            "lookup-x87",
            "OxFunc/crates/oxfunc_core/src/excel_numeric/x87.rs",
            &[3][..],
            "quarantined: do not copy (infra-m0.5)",
        ),
        (
            "lookup-oxfml-eval",
            "OxFml/crates/oxfml_core/src/eval/mod.rs",
            &[6][..],
            "admitted: port only against clean replacements of the listed modules: OxFunc crate oxfunc_core (infra-m0.5)",
        ),
        (
            "lookup-adapter-corpus",
            "OxFunc/crates/oxfunc_core/tests/fixtures/oxfunc_adapter_function_corpus.json",
            &[5][..],
            "  changed by tainted commits a4e94d1e21a9db4c5f1342c51858ff61e9f5b96a",
        ),
        (
            "lookup-live-replay",
            "OxFunc/crates/oxfunc_core/tests/w111_distribution_numeric_live_replay.rs",
            &[5][..],
            "  release source allowlist: left out",
        ),
        (
            "lookup-odd-bond",
            "OxFunc/crates/oxfunc_core/src/functions/odd_bond_family.rs",
            &[3, 5][..],
            "  citation sweep matches line 392 (C-runtime), line 1047 (C-runtime)",
        ),
        (
            "lookup-power-fn",
            "OxFunc/crates/oxfunc_core/src/functions/power_fn.rs",
            &[5][..],
            "  release source allowlist: left out",
        ),
        (
            "lookup-gammaln",
            "OxFunc/crates/oxfunc_core/src/excel_numeric/gammaln.rs",
            &[3][..],
            "  numeric provenance tainted (excel-numeric-gammaln",
        ),
    ] {
        let out = xtask(&evidence, label, &["pin-sources", "--lookup", wanted]);
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let staging: Vec<&str> = STAGING_TOKENS
            .iter()
            .copied()
            .chain(["DR(", "LS2PI", "x87_mul"])
            .filter(|t| stdout.contains(*t))
            .collect();
        evidence.expect(
            out.status.code().is_some_and(|c| codes.contains(&c))
                && stdout.contains(line)
                && staging.is_empty(),
            format!(
                "--lookup {wanted} exits one of {codes:?} (exit {:?}), prints {line:?}, and prints no staging-formula token (found {staging:?})",
                out.status.code()
            ),
        );
    }

    // 9. The generator over a fixture tracker and fixture dispositions.
    let oxfml = root.join("OxFml");
    let head = legacy_read(&oxfml, &["rev-parse", "HEAD"]).unwrap_or_default();
    let source_blob = legacy_read(
        &oxfml,
        &[
            "rev-parse",
            &format!("{head}:crates/oxfml_core/src/source.rs"),
        ],
    )
    .unwrap_or_default();
    let fixture_commit = "b0035c80306ac13f440534f42ebec72525717633";
    let fixture_touches = legacy_read(
        &oxfml,
        &[
            "diff-tree",
            "-r",
            "--no-commit-id",
            "--name-only",
            fixture_commit,
        ],
    )
    .unwrap_or_default();
    evidence.expect(
        source_blob.len() == 40
            && fixture_touches
                .lines()
                .any(|l| l == "crates/oxfml_core/src/syntax/token.rs"),
        format!(
            "at OxFml {head}, source.rs is blob {source_blob:?}, and the commit {fixture_commit} the fixture lists as tainted changed syntax/token.rs ({:?})",
            fixture_touches.lines().collect::<Vec<_>>()
        ),
    );
    let fixture_evidence = json!([{"kind": "commit", "commit": "fixture", "quote": "fixture"}]);
    let fixture_owner = json!({"bead": "dc-fixture", "replacement": "fixture"});
    let fixture_dispositions = json!({
        "format": "dnacalc-numeric-provenance/2",
        "rules": RULES.iter().map(|id| json!({"id": id, "reason": format!("fixture {id}"), "owner": fixture_owner})).collect::<Vec<_>>(),
        "tainted_commits": [{"repository": "OxFml", "commit": fixture_commit, "reason": "fixture: changed syntax/token.rs"}],
        "citation_sweep": {"patterns": ["87tran", "C-runtime"]},
        "modules": [
            {"id": "fixture-tainted", "repository": "OxFml", "path": "crates/oxfml_core/src/source.rs", "blob": source_blob,
             "disposition": "tainted", "reason": "fixture: tainted", "evidence": fixture_evidence},
            {"id": "fixture-stale-clearance", "repository": "OxFml", "path": "crates/oxfml_core/src/syntax/green.rs", "blob": "0000000000000000000000000000000000000000",
             "disposition": "cleared", "reason": "fixture: cleared at a stale blob", "evidence": fixture_evidence},
        ],
        "classes": [
            {"id": "fixture-unresolved-class", "selector": {"repository": "OxFml", "prefixes": ["crates/oxfml_core/src/format/"]},
             "disposition": "unresolved", "reason": "fixture: unresolved", "owner": fixture_owner, "evidence": fixture_evidence},
            {"id": "fixture-included", "selector": {"repository": "OxFml", "paths": ["crates/oxfml_core/src/semantics/mod.rs"]},
             "disposition": "tainted", "reason": "fixture: an included file", "evidence": fixture_evidence},
        ],
    });
    let dir = scratch(CASE);
    let fixture_dispositions_path = dir.join("fixture-numeric-provenance.json");
    write_json(&fixture_dispositions_path, &fixture_dispositions);
    write_json(
        &evidence.dir.join("fixture-numeric-provenance.json"),
        &fixture_dispositions,
    );
    let tracker =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/provenance/issues.jsonl");
    let out_path = dir.join("source-manifest.json");
    let out = xtask(
        &evidence,
        "pin-sources-fixture",
        &[
            "pin-sources",
            "--tracker",
            &tracker.to_string_lossy(),
            "--provenance",
            &fixture_dispositions_path.to_string_lossy(),
            "--out",
            &out_path.to_string_lossy(),
        ],
    );
    evidence.expect(
        out.status.success(),
        format!(
            "pin-sources over the fixture tracker and fixture dispositions exits 0 (exit {:?})",
            out.status.code()
        ),
    );
    let fixture_allowlist_path = dir.join("release-source-allowlist.json");
    if out.status.success() && fixture_allowlist_path.is_file() {
        let fixture_manifest = super::read_json(&out_path);
        let fixture_allowlist = super::read_json(&fixture_allowlist_path);
        fs::copy(
            &fixture_allowlist_path,
            evidence.dir.join("fixture-release-source-allowlist.json"),
        )
        .expect("keep the fixture allowlist");
        fs::copy(&out_path, evidence.dir.join("fixture-source-manifest.json"))
            .expect("keep the fixture manifest");
        let fixture_own = own_tags(&fixture_manifest, &fixture_dispositions);
        let numeric: BTreeMap<String, (String, Option<String>, Vec<String>)> =
            fixture_manifest["inputs"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|e| {
                    (
                        key_of(e),
                        (
                            text(&e["disposition"]).to_owned(),
                            e["numeric_provenance"]["disposition"]
                                .as_str()
                                .map(str::to_owned),
                            list_of(&e["numeric_provenance"], "calls_quarantined"),
                        ),
                    )
                })
                .collect();
        let fixture_listed: BTreeMap<String, Vec<String>> = fixture_allowlist["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| (key_of(e), list_of(e, "calls_quarantined")))
            .collect();
        // What the fixture was built to produce: (read disposition, numeric provenance, listed,
        // an entry its calls_quarantined list must hold).
        let src = "crates/oxfml_core/src/";
        let expected: Vec<FixtureWanted> = vec![
            (
                "OxFml/crates/oxfml_core/src/source.rs",
                "port",
                Some("tainted"),
                false,
                None,
            ),
            (
                "OxFml/crates/oxfml_core/src/red.rs",
                "port",
                None,
                true,
                Some(format!("{src}source.rs")),
            ),
            (
                "OxFml/crates/oxfml_core/src/eval/mod.rs",
                "port",
                None,
                true,
                Some("OxFunc crate oxfunc_core".to_owned()),
            ),
            (
                "OxFml/crates/oxfml_core/src/syntax/token.rs",
                "port",
                Some("unresolved"),
                false,
                None,
            ),
            (
                "OxFml/crates/oxfml_core/tests/language_service_tests.rs",
                "port",
                Some("tainted"),
                false,
                None,
            ),
            (
                "OxFml/crates/oxfml_core/src/format/general.rs",
                "port",
                Some("unresolved"),
                false,
                None,
            ),
            (
                "OxFml/crates/oxfml_core/src/syntax/green.rs",
                "port",
                Some("unresolved"),
                false,
                None,
            ),
            (
                "OxFml/crates/oxfml_core/src/carrier.rs",
                "reference",
                None,
                false,
                Some(format!("{src}source.rs")),
            ),
            (
                "OxFunc/crates/oxfunc_core/src/excel_numeric/x87.rs",
                "quarantined",
                Some("unresolved"),
                false,
                None,
            ),
        ];
        let mut fixture_problems = Vec::new();
        for (key, read, numeric_wanted, in_list, call) in &expected {
            let found = numeric.get(*key);
            let ok = found.is_some_and(|(r, n, calls)| {
                r == read
                    && n.as_deref() == *numeric_wanted
                    && call.as_ref().is_none_or(|c| calls.contains(c))
            });
            if !ok {
                fixture_problems.push(format!(
                    "{key}: manifest {found:?}, wanted {read} with {numeric_wanted:?} and a list holding {call:?}"
                ));
            }
            match (fixture_listed.get(*key), in_list) {
                (Some(calls), true) => {
                    if call.as_ref().is_some_and(|c| !calls.contains(c)) {
                        fixture_problems.push(format!(
                            "{key} is listed with calls_quarantined {calls:?}, not holding {call:?}"
                        ));
                    }
                }
                (None, false) => {}
                (listed, _) => fixture_problems.push(format!(
                    "{key}: listed {listed:?} but wanted listed {in_list}"
                )),
            }
        }
        let changed_by = fixture_manifest["inputs"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| key_of(e) == "OxFml/crates/oxfml_core/src/syntax/token.rs")
            .map(|e| list_of(&e["content_facts"], "changed_by"))
            .unwrap_or_default();
        if changed_by != [fixture_commit] {
            fixture_problems.push(format!("token.rs records changed_by {changed_by:?}"));
        }
        let includes = fixture_manifest["inputs"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| key_of(e) == "OxFml/crates/oxfml_core/tests/language_service_tests.rs")
            .map(|e| list_of(&e["numeric_provenance"], "includes"))
            .unwrap_or_default();
        if !includes.contains(&format!("{src}semantics/mod.rs")) {
            fixture_problems.push(format!("language_service_tests.rs includes {includes:?}"));
        }
        // Every fixture input against this test's own derivation, and the allowlist with it.
        for (key, (_, recorded, calls)) in &numeric {
            let mine = fixture_own.get(key).cloned().unwrap_or_default();
            if *recorded != mine.disposition || *calls != mine.calls {
                fixture_problems.push(format!(
                    "{key}: recorded {recorded:?} {calls:?}, derived {mine:?}"
                ));
            }
        }
        let (allowlist_issues, _, _) =
            allowlist_problems(&fixture_manifest, &fixture_allowlist, &fixture_own);
        fixture_problems.extend(allowlist_issues);
        evidence.expect(
            fixture_problems.is_empty(),
            format!(
                "the fixture allowlist admits the pure caller red.rs with a calls_quarantined list naming the tainted source.rs, and the cross-repository caller eval/mod.rs with the crate-level entry OxFunc crate oxfunc_core; it leaves out syntax/token.rs, which the listed tainted commit {fixture_commit} changed, language_service_tests.rs, which includes the tainted semantics/mod.rs, the tainted source.rs, the unresolved class (format/), the clearance at a stale blob (green.rs), the reference input carrier.rs and the quarantined x87.rs; every input's recorded provenance and list equal this test's own derivation; listed {:?}; problems: {fixture_problems:?}",
                fixture_listed.keys().collect::<Vec<_>>()
            ),
        );
        for (label, wanted, code, line) in [
            (
                "lookup-fixture-pure-caller",
                "OxFml/crates/oxfml_core/src/red.rs",
                6,
                "admitted: port only against clean replacements of the listed modules: crates/oxfml_core/src/source.rs",
            ),
            (
                "lookup-fixture-cross-repository",
                "OxFml/crates/oxfml_core/src/eval/mod.rs",
                6,
                "OxFunc crate oxfunc_core",
            ),
            (
                "lookup-fixture-tainted-commit",
                "OxFml/crates/oxfml_core/src/syntax/token.rs",
                5,
                "  numeric provenance unresolved (tainted-commit)",
            ),
            (
                "lookup-fixture-include",
                "OxFml/crates/oxfml_core/tests/language_service_tests.rs",
                5,
                "  numeric provenance tainted (include-of-quarantined)",
            ),
        ] {
            let out = xtask(
                &evidence,
                label,
                &[
                    "pin-sources",
                    "--lookup",
                    wanted,
                    "--manifest",
                    &out_path.to_string_lossy(),
                ],
            );
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            evidence.expect(
                out.status.code() == Some(code) && stdout.contains(line),
                format!(
                    "--lookup {wanted} on the fixture manifest exits {code} (exit {:?}) and prints {line:?}",
                    out.status.code()
                ),
            );
        }
    }

    // 10. Hand edits of workspace copies fail the check that cargo xtask accept runs.
    let host = host_platform();
    let args = ["--id", "fixture-pass", "--platform", host, "--require-all"];
    let copy = WorkspaceCopy::with_fixture("m0_5_quarantine", &ACCEPT_FIXTURE, &mut evidence);
    let control = copy.accept(&args, "01-unaltered", true, &mut evidence);
    let control_check = check_of(&control);
    evidence.expect(
        control.exit_code == Some(0)
            && control.result() == "passed"
            && control_check == Some((true, Vec::new())),
        format!(
            "01-unaltered: accept in the copy exits {:?} with result {} and its {CHECK} check passes with no finding: {control_check:?}",
            control.exit_code,
            control.result()
        ),
    );
    let pristine: Vec<(String, String)> = [MANIFEST, DISPOSITIONS, ALLOWLIST]
        .iter()
        .map(|rel| {
            (
                (*rel).to_owned(),
                fs::read_to_string(copy.path(rel)).expect("read the copy's record"),
            )
        })
        .collect();
    let restore = |copy: &WorkspaceCopy| {
        for (rel, contents) in &pristine {
            super::write(&copy.path(rel), contents.clone());
        }
    };
    let entry_of = |input: &Value| {
        json!({
            "repository": input["repository"], "path": input["path"],
            "commit": input["commit"], "blob": input["blob"],
            "license_status": input["license_status"], "category": input["category"],
        })
    };
    let add_to_allowlist = |copy: &WorkspaceCopy, keys: &[&str]| {
        let path = copy.path(ALLOWLIST);
        let mut altered = super::read_json(&path);
        for key in keys {
            let input = by_key
                .get(*key)
                .unwrap_or_else(|| panic!("{key} is not a manifest input"));
            altered["entries"]
                .as_array_mut()
                .expect("the allowlist has entries")
                .push(entry_of(input));
        }
        write_json(&path, &altered);
    };

    // 02: entries added to the allowlist by hand.
    let hand_added = [
        "OxFunc/crates/oxfunc_core/src/excel_numeric/x87.rs",
        "OxFunc/crates/oxfunc_core/src/functions/power_fn.rs",
    ];
    add_to_allowlist(&copy, &hand_added);
    evidence.lines.push(format!(
        "02: the copy's {ALLOWLIST} gains, by hand, entries for {hand_added:?} at their pinned commit and blob (x87.rs is quarantined and tainted; power_fn.rs is a port input whose numeric provenance is tainted)"
    ));
    rejected(
        &copy,
        "02-hand-added",
        &hand_added.map(str::to_owned),
        &|f| f.contains("power_fn.rs") && f.contains("is tainted (crt-citing-callers"),
        "the findings say power_fn.rs's recomputed provenance is tainted and name x87.rs",
        &mut evidence,
    );
    restore(&copy);

    // 03: a pure caller listed without its calls_quarantined list.
    let caller = "OxFml/crates/oxfml_core/src/eval/mod.rs";
    let path = copy.path(ALLOWLIST);
    let mut altered = super::read_json(&path);
    let mut stripped = false;
    for entry in altered["entries"].as_array_mut().expect("entries") {
        if key_of(entry) == caller {
            stripped = entry
                .as_object_mut()
                .expect("an entry")
                .remove("calls_quarantined")
                .is_some();
        }
    }
    write_json(&path, &altered);
    evidence.lines.push(format!(
        "03: the copy's {ALLOWLIST} entry for {caller} loses its calls_quarantined list (it was removed: {stripped})"
    ));
    rejected(
        &copy,
        "03-list-dropped",
        &[caller.to_owned()],
        &|f| {
            f.contains(&format!("{caller} is listed with calls_quarantined []"))
                && f.contains("OxFunc crate oxfunc_core")
        },
        "the finding says eval/mod.rs is listed without the list the recomputation gives, OxFunc crate oxfunc_core",
        &mut evidence,
    );
    restore(&copy);

    // 04: a file that a listed file includes is marked tainted (a class added after the review).
    let include = allowlist["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|entry| {
            let includer = key_of(entry);
            let edges = &by_key[&includer]["code_refs"];
            list_of(edges, "includes")
                .into_iter()
                .chain(list_of(edges, "test_includes"))
                .next()
                .map(|included| {
                    (
                        includer.clone(),
                        text(&entry["repository"]).to_owned(),
                        included,
                    )
                })
        });
    match include {
        None => evidence.expect(
            false,
            "04-include: the committed allowlist holds a file that includes another".to_owned(),
        ),
        Some((includer, repository, included)) => {
            let path = copy.path(DISPOSITIONS);
            let mut altered = super::read_json(&path);
            altered["classes"].as_array_mut().expect("classes").insert(
                0,
                json!({
                    "id": "fixture-included", "selector": {"repository": repository, "paths": [included]},
                    "disposition": "tainted", "reason": "fixture: an included file",
                    "after_review": {"kind": "added", "date": "2026-10-15", "reason": "fixture"},
                    "evidence": [{"kind": "commit", "commit": "fixture", "quote": "fixture"}],
                }),
            );
            write_json(&path, &altered);
            evidence.lines.push(format!(
                "04: the copy's {DISPOSITIONS} gains a class, marked as added after the review, marking {repository}/{included} tainted; the listed {includer} includes it"
            ));
            let reachers = changed(&own, &own_tags(&manifest, &altered));
            rejected(
                &copy,
                "04-include",
                &reachers,
                &|f| {
                    f.contains(&format!("{includer} is listed"))
                        && f.contains("include-of-quarantined")
                        && f.contains(&included)
                },
                &format!(
                    "the finding for {includer} says it is tainted by the include rule and names {included}"
                ),
                &mut evidence,
            );
            restore(&copy);
        }
    }

    // 05: a coordinated hand edit: the adapter corpus, changed by the tainted commit a4e94d1e,
    //     loses its recorded provenance in the manifest and gains an allowlist entry, so the two
    //     committed files agree with each other.
    let corpus = "OxFunc/crates/oxfunc_core/tests/fixtures/oxfunc_adapter_function_corpus.json";
    let manifest_path = copy.path(MANIFEST);
    let mut altered = super::read_json(&manifest_path);
    for input in altered["inputs"].as_array_mut().expect("inputs") {
        if key_of(input) == corpus {
            input
                .as_object_mut()
                .expect("an input record")
                .remove("numeric_provenance");
        }
    }
    super::write(
        &manifest_path,
        serde_json::to_string(&altered).expect("serialise"),
    );
    add_to_allowlist(&copy, &[corpus]);
    evidence.lines.push(format!(
        "05: in the copy, {MANIFEST} drops the numeric_provenance of {corpus} and {ALLOWLIST} gains its entry at its pinned commit and blob"
    ));
    rejected(
        &copy,
        "05-coordinated",
        &[corpus.to_owned()],
        &|f| {
            f.contains(&format!(
                "{corpus} is listed but its numeric provenance, recomputed"
            )) && f.contains("unresolved (tainted-commit)")
                && f.contains("a4e94d1e21a9db4c5f1342c51858ff61e9f5b96a")
        },
        "the check recomputes the corpus's provenance from its recorded changed_by fact (a4e94d1e) and rejects both the entry and the manifest's record",
        &mut evidence,
    );
    restore(&copy);

    // 06: a cleared entry added without a review, and one marked as added after the review. Their
    //     paths are no input's, so only the review record can reject them.
    let path = copy.path(DISPOSITIONS);
    let mut altered = super::read_json(&path);
    let cleared = |id: &str, path: &str| {
        json!({
            "id": id, "repository": "OxFml", "path": path, "blob": "0000000000000000000000000000000000000000",
            "disposition": "cleared", "reason": "fixture: a clearance nobody reviewed",
            "evidence": [{"kind": "commit", "commit": "fixture", "quote": "fixture"}],
        })
    };
    let mut marked = cleared(
        "fixture-marked-clearance",
        "crates/oxfml_core/src/fixture_marked.rs",
    );
    marked["after_review"] = json!({"kind": "added", "date": "2026-10-15", "reason": "fixture"});
    let modules_mut = altered["modules"].as_array_mut().expect("modules");
    modules_mut.push(cleared(
        "fixture-unreviewed-clearance",
        "crates/oxfml_core/src/fixture_unreviewed.rs",
    ));
    modules_mut.push(marked);
    write_json(&path, &altered);
    evidence.lines.push(format!(
        "06: the copy's {DISPOSITIONS} gains a cleared module entry with no after_review mark (fixture-unreviewed-clearance) and a cleared one marked as added after the review (fixture-marked-clearance)"
    ));
    let run = copy.accept(&args, "06-unreviewed-clearance", true, &mut evidence);
    let found = check_of(&run);
    let findings = found.clone().map(|(_, f)| f).unwrap_or_default();
    let hash_finding = findings
        .iter()
        .any(|f| f.contains("status.review.dispositions_hash records"));
    let marked_finding = findings.iter().any(|f| {
        f.contains("module fixture-marked-clearance is cleared but marked as added or changed after the review")
    });
    evidence.expect(
        run.exit_code == Some(1)
            && run.result() == "failed"
            && hash_finding
            && marked_finding
            && findings.len() == 2
            && run.checks_ok_except(&[CHECK]),
        format!(
            "06-unreviewed-clearance: accept exits {:?} with result {}; the {CHECK} check fails with exactly two findings, that the entries as reviewed no longer hash to the recorded value (the unmarked clearance) and that fixture-marked-clearance is cleared after the review: {findings:?}",
            run.exit_code,
            run.result()
        ),
    );
    restore(&copy);

    // 07: a tainted commit dropped from the dispositions file, so the manifest's facts are stale.
    let path = copy.path(DISPOSITIONS);
    let mut altered = super::read_json(&path);
    let dropped = altered["tainted_commits"]
        .as_array_mut()
        .expect("tainted_commits")
        .remove(0);
    write_json(&path, &altered);
    evidence.lines.push(format!(
        "07: the copy's {DISPOSITIONS} drops the tainted commit {} from its list",
        dropped["commit"]
    ));
    let reachers = changed(&own, &own_tags(&manifest, &altered));
    let mut named = reachers.clone();
    named.push("numeric_content.tainted_commits".to_owned());
    rejected(
        &copy,
        "07-commit-dropped",
        &named,
        &|f| f.contains("numeric_content.tainted_commits") && f.contains("regenerate"),
        "the check finds the manifest's content facts were computed with another tainted-commit list",
        &mut evidence,
    );
    restore(&copy);

    // The legacy repositories and the tracked files are unchanged.
    let after = statuses(&root, &read_repositories);
    write_statuses(&evidence.dir.join("legacy-status-after.txt"), &after);
    evidence.expect(
        before == after,
        format!("git status --porcelain of {read_repositories:?} is identical before and after"),
    );
    let unchanged = tracked
        .iter()
        .all(|(rel, bytes)| fs::read(repo_root().join(rel)).ok() == *bytes);
    evidence.expect(
        unchanged,
        format!("{MANIFEST}, {DISPOSITIONS} and {ALLOWLIST} are byte-identical before and after the case"),
    );
    evidence.finish();
}

/// The quarantined implementations and generators this test names itself: no retained witness
/// may name one as a generator.
fn quarantined_generator(repository: &str, path: &str) -> bool {
    repository == "OxFunc"
        && (path.starts_with(NUMERIC_DIR) || RESEARCH_PACKAGES.iter().any(|p| path.starts_with(p)))
}

/// The packages of the tainted classes of the dispositions file, by this test's own reading: each
/// class prefix that is a directory holding a Cargo.toml at the pinned commit, with its name.
fn tainted_packages(dispositions: &Value, root: &Path, pin: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for class in dispositions["classes"].as_array().into_iter().flatten() {
        if class["disposition"] != "tainted" {
            continue;
        }
        let repository = text(&class["selector"]["repository"]);
        for prefix in list_of(&class["selector"], "prefixes") {
            if !prefix.ends_with('/') {
                continue;
            }
            let manifest = format!("{pin}:{prefix}Cargo.toml");
            let is_package = legacy_git(&root.join(repository), &["cat-file", "-e", &manifest])
                .output()
                .is_ok_and(|o| o.status.success());
            if is_package {
                let name = prefix
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                out.push((prefix.clone(), name));
            }
        }
    }
    out
}

/// Whether a listed tainted commit changed `path` of `repository` up to `pin`.
fn changed_by_tainted(
    root: &Path,
    repository: &str,
    pin: &str,
    path: &str,
    tainted: &BTreeSet<String>,
) -> Vec<String> {
    legacy_read(
        &root.join(repository),
        &["log", "--format=%H", pin, "--", path],
    )
    .unwrap_or_default()
    .lines()
    .filter(|c| tainted.contains(&format!("{repository}:{c}")))
    .map(str::to_owned)
    .collect()
}

/// What this test finds for one witness's generator chain and its equality with the retention.
#[derive(Debug, Default)]
struct Chain {
    problems: Vec<String>,
    /// Each generator that builds or runs a package of a tainted class, with the packages.
    flagged: Vec<String>,
    compared: usize,
    matched: usize,
    retention: String,
}

/// The generator chain of one witness, by this test's own reading of the generators' blobs: every
/// generator whose text names a package of a tainted class is flagged, and its record must name
/// that package under builds_or_runs; a witness with a flagged generator is retained only when
/// its bits equal the earlier independent retention it records, which this test reads at its
/// pinned blob and compares row by row (and whose writer names no tainted package and no
/// quarantined implementation).
fn check_chain(
    witness: &Value,
    selection: &Value,
    root: &Path,
    pin: &str,
    packages: &[(String, String)],
    tainted: &BTreeSet<String>,
) -> Chain {
    let mut chain = Chain::default();
    let names_tainted = |body: &str| -> Vec<String> {
        packages
            .iter()
            .filter(|(dir, name)| body.contains(dir.as_str()) || body.contains(name.as_str()))
            .map(|(dir, _)| dir.clone())
            .collect()
    };
    for generator in witness["generators"].as_array().into_iter().flatten() {
        let repository = text(&generator["repository"]);
        let path = text(&generator["path"]);
        let body = legacy_bytes(
            &root.join(repository),
            &["cat-file", "blob", text(&generator["blob"])],
        )
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
        let found: BTreeSet<String> = names_tainted(&body).into_iter().collect();
        // A package the generator only names (in comments) is recorded under names_packages
        // with the lines that name it; this test reads those lines.
        let lines: Vec<&str> = body.split('\n').collect();
        let mut named_only: BTreeSet<String> = BTreeSet::new();
        for named in generator["names_packages"].as_array().into_iter().flatten() {
            let package = text(&named["package"]).to_owned();
            let name = package
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_owned();
            let cited = named["lines"].as_array().cloned().unwrap_or_default();
            let all_name_it = !cited.is_empty()
                && cited.iter().all(|l| {
                    l.as_u64()
                        .and_then(|n| lines.get((n as usize).saturating_sub(1)))
                        .is_some_and(|line| line.contains(name.as_str()))
                });
            if !all_name_it {
                chain.problems.push(format!(
                    "generator {path} records that lines {cited:?} name {package}, but they do not"
                ));
            }
            named_only.insert(package);
        }
        let recorded: BTreeSet<String> = list_of(generator, "builds_or_runs")
            .into_iter()
            .chain(named_only.iter().cloned())
            .filter(|dir| packages.iter().any(|(d, _)| d == dir))
            .collect();
        if found != recorded {
            chain.problems.push(format!(
                "generator {path} names the tainted-class packages {found:?} but records {recorded:?} under builds_or_runs and names_packages"
            ));
        }
        if !found.is_empty() {
            let how = if found.iter().all(|p| named_only.contains(p)) {
                "names"
            } else {
                "builds or runs"
            };
            chain.flagged.push(format!("{path} {how} {found:?}"));
        }
    }
    let retention = &witness["independent_retention"];
    if !retention.is_object() {
        if !chain.flagged.is_empty() {
            chain.problems.push(format!(
                "{} (a package of a tainted class), and the witness records no equality with an independent retention",
                chain.flagged.join("; ")
            ));
        }
        return chain;
    }
    let repository = text(&retention["repository"]);
    let path = text(&retention["path"]);
    let dir = root.join(repository);
    chain.retention = format!("{repository} {path} at blob {}", text(&retention["blob"]));
    let pinned = legacy_read(&dir, &["rev-parse", &format!("{pin}:{path}")]);
    if pinned.as_deref() != Ok(text(&retention["blob"])) {
        chain.problems.push(format!(
            "the retention {path} is {pinned:?} at the pin, not the recorded {}",
            retention["blob"]
        ));
    }
    let bank: Value = legacy_bytes(&dir, &["cat-file", "blob", text(&retention["blob"])])
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null);
    let function = text(&selection["function"]);
    let retained = &bank["functions"][function];
    if retained["answer_sha256"] != witness["acquisition"]["known"]["raw_capture_sha256"] {
        chain.problems.push(format!(
            "the retention's {function} answer_sha256 {} is not the raw capture's {}",
            retained["answer_sha256"], witness["acquisition"]["known"]["raw_capture_sha256"]
        ));
    }
    if retained["capture_provenance"] != selection["capture_provenance"] {
        chain.problems.push(format!(
            "the retention's {function} capture provenance differs from the selection's"
        ));
    }
    let bits = retained["expected_bits"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for row in selection["witnesses"].as_array().into_iter().flatten() {
        let index: usize = text(&row["id"])
            .rsplit('-')
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or(usize::MAX);
        chain.compared += 1;
        if bits.get(index) == Some(&row["expected_bits"]) {
            chain.matched += 1;
        }
    }
    let rows = selection["witnesses"].as_array().map_or(0, Vec::len);
    if chain.compared != rows
        || chain.matched != chain.compared
        || retention["compared_rows"] != json!(chain.compared)
        || retention["matched_rows"] != json!(chain.matched)
    {
        chain.problems.push(format!(
            "{} of {} rows match the retention (the record says {} of {})",
            chain.matched, chain.compared, retention["matched_rows"], retention["compared_rows"]
        ));
    }
    let writer = &retention["writer"];
    let writer_path = text(&writer["path"]);
    let writer_pinned = legacy_read(&dir, &["rev-parse", &format!("{pin}:{writer_path}")]);
    let writer_body = legacy_bytes(&dir, &["cat-file", "blob", text(&writer["blob"])])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let writer_names: Vec<String> = names_tainted(&writer_body)
        .into_iter()
        .chain(
            ["excel_numeric", "x87", "87tran", "oxfunc_core", "cargo"]
                .iter()
                .filter(|w| writer_body.contains(**w))
                .map(|w| (*w).to_owned()),
        )
        .collect();
    if writer_pinned.as_deref() != Ok(text(&writer["blob"]))
        || writer_body.is_empty()
        || !writer_names.is_empty()
    {
        chain.problems.push(format!(
            "the retention's writer {writer_path} is {writer_pinned:?} at the pin (recorded {}) and names {writer_names:?}",
            writer["blob"]
        ));
    }
    for (what, path) in [("retention", path), ("writer", writer_path)] {
        let commits = changed_by_tainted(root, repository, pin, path, tainted);
        if !commits.is_empty() {
            chain.problems.push(format!(
                "the {what} {path} was changed by the tainted commits {commits:?}"
            ));
        }
    }
    chain
}

/// infra_m0_5__independent_witness: every retained numeric witness is a small selection of a
/// pinned capture: its rows equal the source rows the recorded rule picks (this test's own reading
/// of the source blob, its own SHA-256 and git blob IDs), it carries black-box acquisition
/// metadata that its own embedded capture record repeats, the directory stays within its bound,
/// its source was changed by no listed tainted commit and matches no citation, and its generators
/// are pinned at commits of the pinned repository with none a quarantined implementation; every
/// generator that builds or runs a package of a tainted class is flagged, and each such witness's
/// bits are compared row by row with the capture runner's own earlier retention
/// (extended-numeric/numeric-outcomes.json at its pinned blob), the equality recorded per witness
/// in an ACCEPT-NOTE line of the receipt. The excluded candidates stay out; and a witness without
/// metadata, one naming a quarantined generator, one whose expected bits were edited and one whose
/// equality record was removed fail the check that cargo xtask accept runs, while a generator that
/// names a tainted package without the equality proof, and bits that differ from the retention,
/// fail this test's own chain check.
pub fn independent_witness() {
    const CASE: &str = "infra_m0_5__independent_witness";
    registered(CASE);
    let mut evidence = Evidence::for_bead(BEAD, CASE);
    let (root, chosen) = legacy_root();
    if !root.is_dir() {
        let reason = format!("the legacy root ({chosen}) does not exist on this host");
        println!("ACCEPT-NOT-RUN: {CASE}: {reason}");
        evidence.lines.push(format!("not run: {reason}"));
        evidence.finish();
        return;
    }
    evidence.lines.push(format!("legacy root: {chosen}"));
    let read_repositories: BTreeSet<String> = ["OxFunc"].map(str::to_owned).into();
    let before = statuses(&root, &read_repositories);
    write_statuses(&evidence.dir.join("legacy-status-before.txt"), &before);

    let manifest = super::read_json(&repo_root().join(MANIFEST));
    let dispositions = super::read_json(&repo_root().join(DISPOSITIONS));
    let inputs = manifest["inputs"].as_array().cloned().unwrap_or_default();
    let by_key: BTreeMap<String, &Value> = inputs.iter().map(|e| (key_of(e), e)).collect();
    let own = own_tags(&manifest, &dispositions);
    let tainted = tainted_commits(&dispositions);
    let patterns = list_of(&dispositions["citation_sweep"], "patterns");
    let retained: Vec<Value> = dispositions["witnesses"]["retained"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    // The directory holds exactly the retained selections (and the attributes file that keeps
    // their bytes), within the size bound.
    let dir = repo_root().join(WITNESSES);
    let mut present: BTreeSet<String> = BTreeSet::new();
    let mut total: u64 = 0;
    for entry in fs::read_dir(&dir).unwrap_or_else(|e| panic!("cannot list {WITNESSES}: {e}")) {
        let path = entry.expect("a directory entry").path();
        assert!(path.is_file(), "{} is not a file", path.display());
        total += fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        present.insert(format!(
            "{WITNESSES}/{}",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
    }
    let attributes = format!("{WITNESSES}/.gitattributes");
    let listed: BTreeSet<String> = retained
        .iter()
        .map(|w| text(&w["file"]).to_owned())
        .collect();
    let mut expected_present = listed.clone();
    expected_present.insert(attributes.clone());
    let attribute_text = fs::read_to_string(repo_root().join(&attributes)).unwrap_or_default();
    let bound = dispositions["witnesses"]["max_total_bytes"]
        .as_u64()
        .unwrap_or(u64::MAX);
    evidence.expect(
        !retained.is_empty()
            && present == expected_present
            && attribute_text.lines().any(|l| l.trim() == "* -text")
            && total <= bound
            && bound <= WITNESS_BOUND,
        format!(
            "{WITNESSES}/ holds exactly the {} retained selections of {DISPOSITIONS} plus .gitattributes (which keeps their bytes with `* -text`), {total} bytes in all, within the recorded bound {bound} and this test's bound {WITNESS_BOUND}: {present:?}",
            retained.len()
        ),
    );

    let pin = manifest["repositories"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|r| r["name"] == "OxFunc")
        .map(|r| text(&r["commit"]).to_owned())
        .unwrap_or_default();
    let packages = tainted_packages(&dispositions, &root, &pin);
    evidence.lines.push(format!(
        "the packages of tainted classes at {pin}: {packages:?}"
    ));
    let mut witness_problems = Vec::new();
    let mut rows_total = 0;
    let mut sources = BTreeSet::new();
    let mut functions = Vec::new();
    let mut equalities = Vec::new();
    let mut flagged_any = 0;
    for witness in &retained {
        let file = text(&witness["file"]);
        let mut fail = |what: String| witness_problems.push(format!("{file}: {what}"));
        let source = &witness["source"];
        let repository = text(&source["repository"]);
        let commit = text(&source["commit"]);
        let path = text(&source["path"]);
        let blob = text(&source["blob"]);
        sources.insert(format!("{repository}/{path}"));
        let legacy = root.join(repository);
        let local = repo_root().join(file);
        let local_bytes = fs::read(&local).unwrap_or_default();
        // The selection's own identity, and the source's: this test's hash of the selection, the
        // pinned commit's blob, the blob's bytes and their SHA-256.
        match own_blob(&local) {
            Ok(found) if found == text(&witness["blob"]) => {}
            other => fail(format!(
                "git hash-object gives {other:?}, not the recorded {}",
                witness["blob"]
            )),
        }
        let pinned = legacy_read(&legacy, &["rev-parse", &format!("{commit}:{path}")]);
        if pinned.as_deref() != Ok(blob) {
            fail(format!(
                "{repository} {commit}:{path} is {pinned:?}, not {blob}"
            ));
        }
        let source_bytes = legacy_bytes(&legacy, &["cat-file", "blob", blob]).unwrap_or_default();
        if sha256_hex(&source_bytes) != text(&source["sha256"]) {
            fail(format!(
                "the source blob's SHA-256 is {}, not the recorded {}",
                sha256_hex(&source_bytes),
                source["sha256"]
            ));
        }
        if commit != pin {
            fail(format!("commit {commit} is not the pinned commit {pin}"));
        }
        // The source's own content: no listed tainted commit changed it and it cites nothing.
        let source_text = String::from_utf8_lossy(&source_bytes).into_owned();
        let cited = source_text
            .split('\n')
            .filter(|line| patterns.iter().any(|p| holds_pattern(line, p)))
            .count();
        let touched = changed_by_tainted(&root, repository, &pin, path, &tainted);
        if cited > 0 || !touched.is_empty() {
            fail(format!(
                "its source has {cited} lines matching the citation sweep and was changed by the tainted commits {touched:?}"
            ));
        }
        let content: Value = serde_json::from_slice(&local_bytes).unwrap_or(Value::Null);
        let full: Value = serde_json::from_slice(&source_bytes).unwrap_or(Value::Null);
        let full_rows = full["witnesses"].as_array().cloned().unwrap_or_default();
        // The selection: the edge rows, then every stride-th random row, copied unchanged.
        let selection = &witness["selection"];
        let edge = selection["edge_rows"].as_u64().unwrap_or(0) as usize;
        let random = selection["random_rows"].as_u64().unwrap_or(0) as usize;
        let stride = selection["stride"].as_u64().unwrap_or(0) as usize;
        let mut picked: Vec<usize> = (0..edge).collect();
        if stride > 0 {
            picked.extend((0..random).step_by(stride).map(|k| edge + k));
        }
        let wanted_rows: Vec<Value> = picked
            .iter()
            .filter_map(|i| full_rows.get(*i).cloned())
            .collect();
        let rows = content["witnesses"].as_array().cloned().unwrap_or_default();
        rows_total += rows.len();
        if full_rows.len() != edge + random
            || source["rows"] != json!(full_rows.len())
            || stride == 0
            || rows != wanted_rows
            || witness["rows"] != json!(rows.len())
            || content["capture_provenance"] != full["capture_provenance"]
            || content["function"] != full["function"]
            || content["function"] != witness["function"]
        {
            fail(format!(
                "the selection is not the {} rows ({edge} edge rows and every {stride}th of {random} random rows) of the {}-row source, with its capture record and function",
                picked.len(),
                full_rows.len()
            ));
        }
        // The black-box acquisition metadata, and the selection's embedded capture record.
        let acquisition = &witness["acquisition"];
        let capture = &content["capture_provenance"];
        if acquisition["method"] != "excel-capture" {
            fail(format!("acquisition method {}", acquisition["method"]));
        }
        for field in [
            "excel_version",
            "excel_build",
            "excel_bitness",
            "workbook_compatibility",
            "excel_operating_system",
            "excel_input_plumbing",
        ] {
            let embedded = &capture["environment"][field];
            if text(embedded).is_empty() || embedded != &acquisition["excel"][field] {
                fail(format!(
                    "environment {field}: the file says {embedded}, the record {}",
                    acquisition["excel"][field]
                ));
            }
        }
        if capture["oracle_cache"]["mode"] != "no_cache"
            || acquisition["oracle_cache"] != "no_cache"
            || capture["oracle_cache"]["hits"] != 0
        {
            fail(format!(
                "cache {} / {}: not every answer was computed live",
                capture["oracle_cache"], acquisition["oracle_cache"]
            ));
        }
        if text(&capture["captured_utc"]).is_empty()
            || capture["captured_utc"] != acquisition["captured_utc"]
        {
            fail(format!(
                "capture time {} / {}",
                capture["captured_utc"], acquisition["captured_utc"]
            ));
        }
        if capture["runner"]["name"] != acquisition["runner"]["name"]
            || capture["runner"]["version"] != acquisition["runner"]["version"]
        {
            fail(format!(
                "runner {} / {}",
                capture["runner"], acquisition["runner"]
            ));
        }
        for unknown in ["cpu", "locale", "calculation_settings"] {
            if !text(&acquisition["unknown"][unknown]).starts_with("unknown") {
                fail(format!(
                    "the acquisition does not record {unknown} as unknown: {}",
                    acquisition["unknown"]
                ));
            }
        }
        if !text(&acquisition["known"]["date_system"]).starts_with("1900") {
            fail(format!("date system {}", acquisition["known"]));
        }
        let prefix = text(&acquisition["input_id_prefix"]);
        if prefix.is_empty() || !rows.iter().all(|row| text(&row["id"]).starts_with(prefix)) {
            fail(format!("a row id does not start with {prefix:?}"));
        }
        functions.push(format!(
            "{} ({} of {} rows)",
            text(&witness["function"]),
            rows.len(),
            full_rows.len()
        ));
        // Generators: each blob at a commit of the pinned repository that is an ancestor of the
        // pin, none quarantined; the expected bits come from the runner the capture names, which
        // drives Excel.Application through COM and reads Value2, and no generator's text names
        // the quarantined implementation.
        let generators = witness["generators"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut runner_drives_excel = false;
        for generator in &generators {
            let g_repository = text(&generator["repository"]);
            let g_path = text(&generator["path"]);
            let g_blob = text(&generator["blob"]);
            let key = format!("{g_repository}/{g_path}");
            let g_legacy = root.join(g_repository);
            let at = generator["commit"].as_str().unwrap_or(commit);
            let is_commit =
                legacy_read(&g_legacy, &["cat-file", "-t", at]).as_deref() == Ok("commit");
            let ancestor = legacy_git(&g_legacy, &["merge-base", "--is-ancestor", at, commit])
                .output()
                .is_ok_and(|out| out.status.success());
            if !is_commit
                || !ancestor
                || legacy_read(&g_legacy, &["rev-parse", &format!("{at}:{g_path}")]).as_deref()
                    != Ok(g_blob)
            {
                fail(format!(
                    "generator {key} is not blob {g_blob} at a commit {at} of {g_repository} that is an ancestor of the pin (commit object: {is_commit}, ancestor: {ancestor})"
                ));
            }
            let body = legacy_bytes(&g_legacy, &["cat-file", "blob", g_blob])
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .unwrap_or_default();
            let names_quarantined = ["excel_numeric", "x87", "87tran", "oxfunc_core"]
                .iter()
                .any(|word| body.contains(word));
            if body.is_empty() || names_quarantined {
                fail(format!(
                    "generator {key}: its text is empty or names the quarantined implementation"
                ));
            }
            if quarantined_generator(g_repository, g_path) {
                fail(format!("generator {key} is a quarantined implementation"));
            }
            match by_key.get(&key) {
                Some(input) => {
                    if matches!(
                        text(&input["disposition"]),
                        "quarantined" | "quarantine_derived"
                    ) || non_cleared(own.get(&key).and_then(|t| t.disposition.as_deref()))
                        || (input["blob"] != g_blob && generator["commit"].is_null())
                    {
                        fail(format!(
                            "generator {key} is {} / {:?} at {} in the manifest",
                            input["disposition"],
                            own.get(&key),
                            input["blob"]
                        ));
                    }
                }
                None => fail(format!("generator {key} is not a pinned input")),
            }
            if generator["role"] == "expected_bits"
                && g_path.ends_with(&format!("/{}", text(&capture["runner"]["name"])))
            {
                runner_drives_excel = body.contains("New-Object -ComObject Excel.Application")
                    && body.contains(".Value2");
            }
        }
        if !runner_drives_excel {
            fail("no expected-bits generator is the runner the capture names, driving Excel.Application through COM and reading Value2".to_owned());
        }
        if own_disposition(&dispositions, repository, path, blob).as_deref() != Some("cleared") {
            fail(format!(
                "its source {repository}/{path} is not cleared at blob {blob}"
            ));
        }
        // The generator chain and the equality with the capture runner's own retention.
        let chain = check_chain(witness, &content, &root, &pin, &packages, &tainted);
        if !chain.flagged.is_empty() {
            flagged_any += 1;
        }
        for problem in &chain.problems {
            fail(problem.clone());
        }
        let note = format!(
            "{file} ({}): {} rows compared with the retention {} (written by {} from the raw captures); {} of {} matched{}; generators that build, run or name a package of a tainted class: {:?}",
            text(&witness["function"]),
            chain.compared,
            chain.retention,
            text(&witness["independent_retention"]["writer"]["path"]),
            chain.matched,
            chain.compared,
            if chain.matched == chain.compared && chain.compared > 0 {
                ", all rows matched"
            } else {
                ""
            },
            chain.flagged
        );
        println!("ACCEPT-NOTE: {CASE}: {note}");
        equalities.push((chain.compared, chain.matched));
        evidence.lines.push(note);
    }
    evidence.expect(
        witness_problems.is_empty(),
        format!(
            "every retained selection ({}) is the rows its recorded rule picks from its source capture, read at the pinned commit with this test's git and checked against the recorded blob and SHA-256; its source was changed by no listed tainted commit and matches no citation; it repeats the Excel version, build, bitness, compatibility, operating system, input plumbing, capture time, runner and no-cache mode its capture record states, records CPU, locale and calculation settings as unknown and the 1900 date system, holds {rows_total} rows in all, and names generators at commits of the pinned repository that are ancestors of the pin, none quarantined, quarantine_derived, tainted or unresolved, none whose text names excel_numeric, x87, 87tran or oxfunc_core, with the expected bits from the COM runner that drives Excel.Application; every generator whose text names a package of a tainted class records it under builds_or_runs; problems: {witness_problems:?}",
            functions.join(", ")
        ),
    );
    evidence.expect(
        equalities.len() == retained.len()
            && flagged_any == retained.len()
            && equalities.iter().all(|(c, m)| *c > 0 && c == m),
        format!(
            "all {} witnesses have a generator that builds, runs or names a package of a tainted class (freeze_campaign_snapshot_20260929.py builds smart-fuzzer/tools/pmt_ppmt_local_eval/; the runner's comments name smart-fuzzer/tools/calc_graph_racer/), and each is retained because its bits equal the earlier independent retention, row by row: (compared, matched) {equalities:?}",
            retained.len()
        ),
    );

    // This test's own chain check rejects a generator that names a tainted package without the
    // equality proof, and bits that differ from the retention.
    if let Some(first) = retained.first() {
        let content = super::read_json(&repo_root().join(text(&first["file"])));
        let mut no_proof = first.clone();
        no_proof
            .as_object_mut()
            .expect("a witness")
            .remove("independent_retention");
        for generator in no_proof["generators"].as_array_mut().expect("generators") {
            let object = generator.as_object_mut().expect("a generator");
            object.remove("builds_or_runs");
            object.remove("names_packages");
        }
        let chain = check_chain(&no_proof, &content, &root, &pin, &packages, &tainted);
        let unrecorded = chain
            .problems
            .iter()
            .any(|p| p.contains("records {} under builds_or_runs"));
        let unproven = chain
            .problems
            .iter()
            .any(|p| p.contains("records no equality with an independent retention"));
        evidence.expect(
            unrecorded && unproven,
            format!(
                "a copy of {}'s record without builds_or_runs, names_packages and independent_retention fails this test's chain check: the generators name tainted-class packages and no equality proof is recorded: {:?}",
                text(&first["file"]),
                chain.problems
            ),
        );
        let mut edited = content.clone();
        let rows = edited["witnesses"].as_array_mut().expect("rows");
        let last = rows.len() - 1;
        let old = text(&rows[last]["expected_bits"]).to_owned();
        let new = if old == "0x0000000000000000" {
            "0x0000000000000001"
        } else {
            "0x0000000000000000"
        };
        rows[last]["expected_bits"] = json!(new);
        let chain = check_chain(first, &edited, &root, &pin, &packages, &tainted);
        evidence.expect(
            chain.matched + 1 == chain.compared
                && chain.problems.iter().any(|p| p.contains("rows match the retention")),
            format!(
                "a copy of {} whose last row's expected bits {old} became {new} fails this test's chain check: {} of {} rows match the retention: {:?}",
                text(&first["file"]),
                chain.matched,
                chain.compared,
                chain.problems
            ),
        );
    }

    // The excluded candidates stay out: neither by path nor by blob.
    let excluded: Vec<String> = dispositions["witnesses"]["excluded"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|e| {
            let repository = text(&e["repository"]).to_owned();
            e["paths"]
                .as_array()
                .into_iter()
                .flatten()
                .map(move |p| format!("{repository}/{}", text(p)))
                .collect::<Vec<_>>()
        })
        .collect();
    let ground_truth = format!("OxFunc/{NUMERIC_DIR}x87_excel_ground_truth.tsv");
    let source_blobs: BTreeSet<String> = retained
        .iter()
        .map(|w| text(&w["source"]["blob"]).to_owned())
        .collect();
    let excluded_blobs: BTreeSet<String> = excluded
        .iter()
        .filter_map(|k| by_key.get(k).map(|i| text(&i["blob"]).to_owned()))
        .collect();
    let judges = excluded
        .iter()
        .filter(|k| k.ends_with("-judge.json"))
        .count();
    evidence.expect(
        excluded.contains(&ground_truth)
            && excluded.iter().filter(|k| k.ends_with("-production-replay.json")).count() >= 5
            && judges == 8
            && excluded.iter().all(|k| !sources.contains(k))
            && source_blobs.is_disjoint(&excluded_blobs)
            && !excluded_blobs.is_empty(),
        format!(
            "{} excluded candidates, among them {ground_truth}, the production-replay files and the {judges} pinned distribution-surface judge files, are neither a retained source nor the blob of one",
            excluded.len()
        ),
    );

    // The lookup names a retained witness's source as such.
    let first_source = format!(
        "{}/{}",
        text(&retained[0]["source"]["repository"]),
        text(&retained[0]["source"]["path"])
    );
    let out = xtask(
        &evidence,
        "lookup-witness-source",
        &["pin-sources", "--lookup", &first_source],
    );
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    evidence.expect(
        out.status.code() == Some(0)
            && stdout.contains(&format!("retained witness source {first_source}\n"))
            && stdout.contains(&format!(
                "  retained selection {} ",
                text(&retained[0]["file"])
            )),
        format!(
            "--lookup {first_source} exits 0 (exit {:?}) and names it the source of the retained selection {}",
            out.status.code(),
            text(&retained[0]["file"])
        ),
    );

    // A witness without acquisition metadata, one naming a quarantined generator, one whose
    // expected bits were edited, and one whose equality record was removed fail the check that
    // cargo xtask accept runs.
    let host = host_platform();
    let copy = WorkspaceCopy::with_fixture("m0_5_witness", &ACCEPT_FIXTURE, &mut evidence);
    let args = ["--id", "fixture-pass", "--platform", host, "--require-all"];
    let control = copy.accept(&args, "01-unaltered", true, &mut evidence);
    let control_check = check_of(&control);
    evidence.expect(
        control.exit_code == Some(0)
            && control.result() == "passed"
            && control_check == Some((true, Vec::new())),
        format!(
            "01-unaltered: accept in the copy exits {:?} with result {} and its {CHECK} check passes with no finding: {control_check:?}",
            control.exit_code,
            control.result()
        ),
    );
    let copy_dispositions = copy.path(DISPOSITIONS);
    let pristine_dispositions =
        fs::read_to_string(&copy_dispositions).expect("read the copy's dispositions");
    let unlisted = format!("{WITNESSES}/hand-added-witness.json");
    super::write(
        &copy.path(&unlisted),
        json!({"function": "EXP", "witnesses": [{"id": "x", "args": ["0x3ff0000000000000"], "expected_bits": "0x4005bf0a8b145769"}]}).to_string(),
    );
    let gen_tables = format!("OxFunc/{NUMERIC_DIR}gen_tables.py");
    let gen_tables_blob = by_key
        .get(&gen_tables)
        .map(|i| text(&i["blob"]).to_owned())
        .unwrap_or_default();
    let mut altered = super::read_json(&copy_dispositions);
    altered["witnesses"]["retained"][0]["generators"]
        .as_array_mut()
        .expect("the first witness has generators")
        .push(json!({
            "role": "inputs", "what": "hand-added quarantined generator",
            "repository": "OxFunc", "path": format!("{NUMERIC_DIR}gen_tables.py"), "blob": gen_tables_blob,
        }));
    write_json(&copy_dispositions, &altered);
    let first = text(&retained[0]["file"]).to_owned();
    let second = text(&retained[1]["file"]).to_owned();
    let second_path = copy.path(&second);
    let second_text = fs::read_to_string(&second_path).expect("read the second selection");
    let row = &retained[1];
    let probe = super::read_json(&second_path);
    let old_bits = text(&probe["witnesses"][0]["expected_bits"]).to_owned();
    let new_bits = if old_bits == "0x0000000000000000" {
        "0x0000000000000001"
    } else {
        "0x0000000000000000"
    };
    let edited = second_text.replacen(
        &format!("\"expected_bits\":\"{old_bits}\""),
        &format!("\"expected_bits\":\"{new_bits}\""),
        1,
    );
    super::write(&second_path, edited.clone());
    evidence.lines.push(format!(
        "the copy gains, by hand, {unlisted} (no acquisition metadata), the quarantined generator {gen_tables} on {first} in {DISPOSITIONS}, and in {second} ({}) the first row's expected bits {old_bits} replaced by {new_bits}; the edit changed the file: {}",
        text(&row["function"]),
        edited != second_text
    ));
    let run = copy.accept(&args, "02-hand-added", true, &mut evidence);
    let found = check_of(&run);
    let findings = found.clone().map(|(_, f)| f).unwrap_or_default();
    let names_unlisted = findings
        .iter()
        .any(|f| f.contains(&unlisted) && f.contains("no acquisition metadata"));
    let names_generator = findings
        .iter()
        .any(|f| f.contains(&first) && f.contains(&gen_tables) && f.contains("quarantined"));
    let names_edit = findings
        .iter()
        .any(|f| f.contains(&second) && f.contains("its bytes hash to blob"));
    let only_these = findings
        .iter()
        .all(|f| f.contains(&unlisted) || f.contains(&gen_tables) || f.contains(&second));
    evidence.expect(
        run.exit_code == Some(1)
            && run.result() == "failed"
            && found.as_ref().is_some_and(|(passed, _)| !passed)
            && names_unlisted
            && names_generator
            && names_edit
            && only_these
            && run.checks_ok_except(&[CHECK]),
        format!(
            "02-hand-added: accept exits {:?} with result {}; the {CHECK} check fails naming the unlisted witness, the quarantined generator and the edited selection and nothing else, and the other checks pass: {findings:?}",
            run.exit_code,
            run.result()
        ),
    );
    // 03: the first witness loses its equality record.
    let _ = fs::remove_file(copy.path(&unlisted));
    super::write(&second_path, second_text);
    let mut altered: Value =
        serde_json::from_str(&pristine_dispositions).expect("the copy's dispositions parse");
    altered["witnesses"]["retained"][0]
        .as_object_mut()
        .expect("a witness")
        .remove("independent_retention");
    write_json(&copy_dispositions, &altered);
    evidence.lines.push(format!(
        "03: in the copy, {first}'s record in {DISPOSITIONS} loses its independent_retention"
    ));
    let run = copy.accept(&args, "03-equality-removed", true, &mut evidence);
    let found = check_of(&run);
    let findings = found.clone().map(|(_, f)| f).unwrap_or_default();
    let names_missing = findings.iter().any(|f| {
        f.contains(&first)
            && f.contains("pmt_ppmt_local_eval")
            && f.contains("records no equality of its bits with an independent retention")
    });
    evidence.expect(
        run.exit_code == Some(1)
            && run.result() == "failed"
            && names_missing
            && findings.iter().all(|f| f.contains(&first))
            && run.checks_ok_except(&[CHECK]),
        format!(
            "03-equality-removed: accept exits {:?} with result {}; the {CHECK} check fails only for {first}: its snapshot step builds or runs smart-fuzzer/tools/pmt_ppmt_local_eval/ and it records no equality with an independent retention: {findings:?}",
            run.exit_code,
            run.result()
        ),
    );

    let after = statuses(&root, &read_repositories);
    write_statuses(&evidence.dir.join("legacy-status-after.txt"), &after);
    evidence.expect(
        before == after,
        format!("git status --porcelain of {read_repositories:?} is identical before and after"),
    );
    evidence.finish();
}
