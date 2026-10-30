//! The cargo, rustc and git processes xtask starts, and what cargo reports about the workspace.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The target directory of every cargo that xtask starts, relative to the workspace root. The
/// cargo that started xtask owns `target/` itself.
pub const ACCEPT_TARGET_DIR: &str = "target/xtask-accept";

pub const WASM_TARGET: &str = "wasm32-unknown-unknown";

/// The target triples the workspace ships for (plan section 10.4: Windows, Linux and wasm). The
/// architecture checks resolve the dependency graph for each of them on either host, so one
/// receipt covers every shipped target.
pub const SHIPPED_TARGETS: [&str; 3] = [
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
    WASM_TARGET,
];

/// The cargo that started xtask (the `CARGO` variable cargo sets for `cargo run`), or `cargo` from
/// PATH when it is unset.
pub fn cargo_program() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

/// The rustc cargo would use: the `RUSTC` variable, or `rustc` from PATH when it is unset.
pub fn rustc_program() -> OsString {
    std::env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"))
}

/// Runs `command` to completion, or says why it could not start.
pub fn output(command: &mut Command) -> Result<Output, String> {
    command
        .output()
        .map_err(|e| format!("cannot start {command:?}: {e}"))
}

pub fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The last `lines` lines of `bytes`.
pub fn tail(bytes: &[u8], lines: usize) -> String {
    let text = String::from_utf8_lossy(bytes);
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// The argument vector of `command` as text: the program, then its arguments.
pub fn argv(command: &Command) -> Vec<String> {
    std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

/// One target of a workspace package, as cargo metadata reports it.
pub struct Target {
    pub name: String,
    pub kinds: Vec<String>,
    pub src_path: PathBuf,
    /// The features cargo builds the target only with (`required-features`).
    pub required_features: Vec<String>,
}

/// One workspace member, as `cargo metadata --no-deps` reports it.
pub struct Member {
    pub name: String,
    pub id: String,
    pub dir: PathBuf,
    pub portability: Option<String>,
    pub targets: Vec<Target>,
}

/// The target kinds whose crates ship: libraries and binaries, not tests, benches, examples or
/// build scripts.
const SHIPPED_KINDS: [&str; 7] = [
    "lib",
    "rlib",
    "dylib",
    "cdylib",
    "staticlib",
    "proc-macro",
    "bin",
];

impl Member {
    pub fn is_portable(&self) -> bool {
        self.portability.as_deref() == Some("portable")
    }

    pub fn is_native_only(&self) -> bool {
        self.portability.as_deref() == Some("native-only")
    }

    /// The crate roots of the targets that ship.
    pub fn shipped_roots(&self) -> Vec<PathBuf> {
        self.targets
            .iter()
            .filter(|target| {
                target
                    .kinds
                    .iter()
                    .any(|kind| SHIPPED_KINDS.contains(&kind.as_str()))
            })
            .map(|target| target.src_path.clone())
            .collect()
    }

    /// The roots of the package's build script, if it has one (`build.rs`).
    pub fn build_script_roots(&self) -> Vec<PathBuf> {
        self.targets
            .iter()
            .filter(|target| target.kinds.iter().any(|kind| kind == "custom-build"))
            .map(|target| target.src_path.clone())
            .collect()
    }

    pub fn has_test_target(&self, name: &str) -> bool {
        self.targets
            .iter()
            .any(|target| target.name == name && target.kinds.iter().any(|kind| kind == "test"))
    }
}

/// The workspace, as `cargo metadata --no-deps` reports it from a directory inside it.
pub struct Workspace {
    pub root: PathBuf,
    pub members: Vec<Member>,
}

fn string_field(value: &Value, field: &str, what: &str) -> Result<String, String> {
    value[field]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("{what} has no string {field}"))
}

impl Workspace {
    pub fn load(cwd: &Path) -> Result<Workspace, String> {
        let mut command = Command::new(cargo_program());
        command
            .args(["metadata", "--no-deps", "--format-version", "1", "--locked"])
            .current_dir(cwd);
        let out = output(&mut command)?;
        if !out.status.success() {
            return Err(format!(
                "{:?} exited with {:?}:\n{}",
                argv(&command),
                out.status.code(),
                tail(&out.stderr, 20)
            ));
        }
        let metadata: Value = serde_json::from_slice(&out.stdout)
            .map_err(|e| format!("cargo metadata printed no JSON: {e}"))?;
        let root = PathBuf::from(string_field(&metadata, "workspace_root", "cargo metadata")?);
        let packages = metadata["packages"]
            .as_array()
            .ok_or("cargo metadata reports no packages")?;
        let ids = metadata["workspace_members"]
            .as_array()
            .ok_or("cargo metadata reports no workspace_members")?;
        let mut members = Vec::new();
        for id in ids {
            let id = id.as_str().ok_or("a workspace member id is not a string")?;
            let package = packages
                .iter()
                .find(|package| package["id"] == id)
                .ok_or_else(|| format!("workspace member {id} is missing from packages"))?;
            let manifest = PathBuf::from(string_field(package, "manifest_path", id)?);
            let dir = manifest
                .parent()
                .ok_or_else(|| format!("{} has no parent directory", manifest.display()))?
                .to_path_buf();
            let mut targets = Vec::new();
            for target in package["targets"]
                .as_array()
                .ok_or_else(|| format!("{id} has no targets"))?
            {
                targets.push(Target {
                    name: string_field(target, "name", "a target")?,
                    kinds: target["kind"]
                        .as_array()
                        .ok_or("a target has no kind list")?
                        .iter()
                        .filter_map(|kind| kind.as_str().map(str::to_owned))
                        .collect(),
                    src_path: PathBuf::from(string_field(target, "src_path", "a target")?),
                    required_features: target["required-features"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|feature| feature.as_str().map(str::to_owned))
                        .collect(),
                });
            }
            members.push(Member {
                name: string_field(package, "name", id)?,
                id: id.to_owned(),
                dir,
                portability: package["metadata"]["dnacalc"]["portability"]
                    .as_str()
                    .map(str::to_owned),
                targets,
            });
        }
        Ok(Workspace { root, members })
    }

    pub fn member(&self, name: &str) -> Option<&Member> {
        self.members.iter().find(|member| member.name == name)
    }

    /// `path` relative to the workspace root with forward slashes, or as given when it lies
    /// outside the root.
    pub fn rel(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    pub fn accept_target_dir(&self) -> PathBuf {
        self.root.join("target").join("xtask-accept")
    }
}

/// The host triple rustc reports in `rustc -vV`, with that output's first line.
pub fn rustc_version() -> Result<(String, String), String> {
    let mut command = Command::new(rustc_program());
    command.arg("-vV");
    let out = output(&mut command)?;
    let stdout = text(&out.stdout);
    let host = stdout
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or_else(|| format!("rustc -vV printed no host line:\n{stdout}"))?;
    let first = stdout.lines().next().unwrap_or_default();
    Ok((host.trim().to_owned(), first.trim().to_owned()))
}

/// One resolved dependency edge.
pub struct Edge {
    pub to: String,
    pub normal: bool,
    pub build: bool,
}

/// The dependency graph cargo resolves for one target platform with every feature of every
/// workspace member enabled.
pub struct Resolved {
    names: BTreeMap<String, String>,
    edges: BTreeMap<String, Vec<Edge>>,
}

impl Resolved {
    /// Runs `cargo metadata --all-features --filter-platform <target>` in the workspace.
    pub fn load(ws: &Workspace, target: &str) -> Result<Resolved, String> {
        let mut command = Command::new(cargo_program());
        command
            .args([
                "metadata",
                "--format-version",
                "1",
                "--all-features",
                "--locked",
                "--filter-platform",
                target,
            ])
            .current_dir(&ws.root);
        let out = output(&mut command)?;
        if !out.status.success() {
            return Err(format!(
                "{:?} exited with {:?}:\n{}",
                argv(&command),
                out.status.code(),
                tail(&out.stderr, 20)
            ));
        }
        let metadata: Value = serde_json::from_slice(&out.stdout)
            .map_err(|e| format!("cargo metadata printed no JSON: {e}"))?;
        let mut names = BTreeMap::new();
        for package in metadata["packages"]
            .as_array()
            .ok_or("cargo metadata reports no packages")?
        {
            names.insert(
                string_field(package, "id", "a package")?,
                string_field(package, "name", "a package")?,
            );
        }
        let mut edges = BTreeMap::new();
        for node in metadata["resolve"]["nodes"]
            .as_array()
            .ok_or("cargo metadata reports no resolve nodes")?
        {
            let id = string_field(node, "id", "a resolve node")?;
            let mut out_edges = Vec::new();
            for dep in node["deps"]
                .as_array()
                .ok_or_else(|| format!("resolve node {id} has no deps"))?
            {
                let kinds = dep["dep_kinds"]
                    .as_array()
                    .ok_or_else(|| format!("a dependency of {id} has no dep_kinds"))?;
                let kind_is =
                    |wanted: Option<&str>| kinds.iter().any(|kind| kind["kind"].as_str() == wanted);
                out_edges.push(Edge {
                    to: string_field(dep, "pkg", "a resolved dependency")?,
                    normal: kind_is(None),
                    build: kind_is(Some("build")),
                });
            }
            edges.insert(id, out_edges);
        }
        Ok(Resolved { names, edges })
    }

    /// The names of the packages reachable from package `id` through edges that `follow` accepts,
    /// `id` itself excluded. With `direct`, only its own dependencies.
    pub fn reach(&self, id: &str, follow: fn(&Edge) -> bool, direct: bool) -> BTreeSet<String> {
        let mut seen = BTreeSet::from([id.to_owned()]);
        let mut pending = vec![id.to_owned()];
        let mut names = BTreeSet::new();
        while let Some(next) = pending.pop() {
            for edge in self.edges.get(&next).into_iter().flatten() {
                if follow(edge) && seen.insert(edge.to.clone()) {
                    names.insert(
                        self.names
                            .get(&edge.to)
                            .cloned()
                            .unwrap_or_else(|| edge.to.clone()),
                    );
                    if !direct {
                        pending.push(edge.to.clone());
                    }
                }
            }
        }
        names
    }
}

/// A git command's standard output, or why it did not succeed.
pub fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new("git");
    command.arg("-C").arg(root).args(args);
    let out = output(&mut command)?;
    if out.status.success() {
        Ok(text(&out.stdout))
    } else {
        Err(format!(
            "git {args:?} exited with {:?}: {}",
            out.status.code(),
            text(&out.stderr).trim()
        ))
    }
}
