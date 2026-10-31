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
    package_rows: BTreeSet<String>,
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
        let mut package_rows = BTreeSet::new();
        for package in metadata["packages"]
            .as_array()
            .ok_or("cargo metadata reports no packages")?
        {
            let name = string_field(package, "name", "a package")?;
            let version = string_field(package, "version", "a package")?;
            package_rows.insert(format!("{name} v{version}"));
            names.insert(string_field(package, "id", "a package")?, name);
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
        Ok(Resolved {
            names,
            package_rows,
            edges,
        })
    }

    /// Target-specific reverse normal edges. Cargo metadata can unify features from an
    /// inactive target-specific dependency before filtering its edges. Ask cargo tree for
    /// the selected target before treating such an optional native crate as a wasm import.
    /// Command semantics: https://doc.rust-lang.org/cargo/commands/cargo-tree.html#feature-unification
    pub fn normal_dependents(
        &self,
        ws: &Workspace,
        target: &str,
        dependency: &str,
    ) -> Result<BTreeSet<String>, String> {
        let known = &self.package_rows;
        let ids = self
            .names
            .iter()
            .filter(|(_, name)| name.as_str() == dependency)
            .map(|(id, _)| id);
        let mut found = false;
        let mut dependents = BTreeSet::new();
        for id in ids {
            found = true;
            let mut command = Command::new(cargo_program());
            command
                .args([
                    "tree",
                    "--locked",
                    "--workspace",
                    "--all-features",
                    "--target",
                    target,
                    "--edges",
                    "normal",
                    "--invert",
                    id,
                    "--prefix",
                    "none",
                    "--format",
                    "{p}",
                    "--color",
                    "never",
                ])
                .current_dir(&ws.root);
            let out = output(&mut command)?;
            if !out.status.success() {
                return Err(format!(
                    "{:?} exited with {:?}: {}",
                    argv(&command),
                    out.status.code(),
                    tail(&out.stderr, 20)
                ));
            }
            dependents.extend(tree_package_names(&text(&out.stdout), known)?);
        }
        if !found {
            return Err(format!("resolved metadata has no package {dependency}"));
        }
        Ok(dependents)
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

fn tree_package_names(output: &str, known: &BTreeSet<String>) -> Result<BTreeSet<String>, String> {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut words = line.split_whitespace();
            let name = words
                .next()
                .ok_or("cargo tree printed an empty package row")?;
            let version = words
                .next()
                .ok_or("cargo tree package row has no version")?;
            if !known.contains(&format!("{name} {version}")) {
                return Err(format!("unrecognized cargo tree package row: {line}"));
            }
            Ok(name.to_owned())
        })
        .collect()
}

#[cfg(test)]
mod target_dependency_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn inverted_tree_names_keep_duplicates_and_reject_unknown_rows() {
        let known = BTreeSet::from(["adapter v1.0.0".to_owned(), "libc v0.1.0".to_owned()]);
        assert_eq!(
            tree_package_names(
                "libc v0.1.0\nadapter v1.0.0 (/a path) (*)\nadapter v1.0.0\n",
                &known
            )
            .unwrap(),
            BTreeSet::from(["adapter".into(), "libc".into()])
        );
        assert!(tree_package_names("libc\n", &known).is_err());
        assert!(tree_package_names("libc vgarbage\n", &known).is_err());
        assert!(tree_package_names("libc v9.9.9\n", &known).is_err());
        assert!(tree_package_names("unknown v1.0.0\n", &known).is_err());
        assert!(tree_package_names("warning: nothing to print\n", &known).is_err());
        assert!(tree_package_names("", &known).unwrap().is_empty());
    }

    #[test]
    fn target_only_feature_does_not_hide_a_real_wasm_normal_dependency() {
        let root = std::env::temp_dir().join(format!(
            "dnacalc-target-feature-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        struct Fixture(PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        std::fs::create_dir(&root).unwrap();
        let _fixture = Fixture(root.clone());
        let write = |name: &str, content: &str| {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        };
        write(
            "Cargo.toml",
            "[workspace]\nresolver = \"2\"\nmembers = [\"adapter\"]\nexclude = [\"engine\", \"os\"]\n",
        );
        write(
            "engine/Cargo.toml",
            "[package]\nname = \"fixture-engine\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[features]\nnative = [\"dep:libc\"]\n[dependencies]\nlibc = { path = \"../os\", optional = true }\n",
        );
        write(
            "os/Cargo.toml",
            "[package]\nname = \"libc\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        );
        for name in ["adapter", "engine", "os"] {
            write(&format!("{name}/src/lib.rs"), "");
        }
        for normal_native in [false, true] {
            let features = if normal_native {
                ", features = [\"native\"]"
            } else {
                ""
            };
            write(
                "adapter/Cargo.toml",
                &format!(
                    "[package]\nname = \"adapter\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\nfixture-engine = {{ path = \"../engine\", default-features = false{features} }}\n[target.'cfg(not(target_arch = \"wasm32\"))'.dependencies]\nfixture-engine = {{ path = \"../engine\", default-features = false, features = [\"native\"] }}\n"
                ),
            );
            let out = Command::new(cargo_program())
                .args(["generate-lockfile", "--offline"])
                .current_dir(&root)
                .output()
                .unwrap();
            assert!(out.status.success(), "{}", text(&out.stderr));
            let ws = Workspace::load(&root).unwrap();
            let graph = Resolved::load(&ws, WASM_TARGET).unwrap();
            // Metadata exposes the native optional package in both cases. The actual target
            // tree must distinguish an inactive target-only feature from an active one.
            let adapter = ws.member("adapter").unwrap();
            assert!(
                graph
                    .reach(&adapter.id, |edge| edge.normal, false)
                    .contains("libc")
            );
            let actual = graph.normal_dependents(&ws, WASM_TARGET, "libc").unwrap();
            assert_eq!(actual.contains("adapter"), normal_native);
        }
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
