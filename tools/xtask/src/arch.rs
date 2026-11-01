//! The architecture checks that every `cargo xtask accept` run performs before any case, whatever
//! the bead:
//!
//! - `dependencies` (plan section 1.3): the layering rules of `tools/xtask/architecture.json` on
//!   resolved cargo metadata with all features, for every shipped target triple
//!   (x86_64-pc-windows-msvc, x86_64-unknown-linux-gnu and wasm32-unknown-unknown, plus the host
//!   triple when it is none of them) on either host, over normal and build dependencies; and no
//!   portable package reaches a native-only one.
//! - `os-imports` (plan section 1.3, last rule): the non-test sources of the shipped targets (lib
//!   and bin) of portable packages name no denied std module (`std::fs`, `std::process`,
//!   `std::os`, `std::net`), declare no denied ABI (`extern "system"`) and carry no denied
//!   attribute (`#[link]`). Build scripts run on the build host and never enter a library or
//!   browser build, so they are outside this source scan. No portable package depends directly
//!   (normal or build dependency) on a denied OS-binding crate on any shipped target, and none has
//!   one anywhere in its wasm32-unknown-unknown closure (normal dependencies); a transitive
//!   OS-binding crate on a native target (libc or rustix under a std-only crate) is allowed.
//!   A flagged transitive wasm edge is confirmed with the selected target's cargo tree:
//!   metadata can retain optional features activated by an inactive native target.
//! - `wasm-refusal`: every native-only package fails to compile for wasm32-unknown-unknown, with
//!   the error raised by a `compile_error!` in its own sources. Its wasm32 artifacts are removed
//!   first (`cargo clean -p`), so a cached success can never answer for this run's compile.
//! - `tree-reference-tokens` (plan sections 1.1, 1.3, 4.2 and 11.1): no identifier, string literal
//!   or doc comment in the non-test code of a portable package (its lib and bin targets and its
//!   build script) contains a denied phrase, in any case or joining (`TreeRef`, `tree_ref`,
//!   `tree-ref`, `Tree ref` are one phrase). Plain comments, `tests/`, `benches/`, `examples/` and
//!   `#[cfg(test)]` items are outside its scope by design.
//!
//! Neither source scan follows `include!`, `include_str!` or `include_bytes!`: a portable package
//! that uses one, in a shipped target or its build script, fails both `os-imports` and
//! `tree-reference-tokens` until a bead extends them to read the included file.

use crate::cargo::{self, Edge, Resolved, SHIPPED_TARGETS, WASM_TARGET, Workspace};
use crate::rust_source::{self, SourceFile};
use crate::text;
use proc_macro2::{Delimiter, TokenStream, TokenTree};
use quote::ToTokens;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

/// The reviewed rules and deny lists, relative to the workspace root.
pub const CONFIG_PATH: &str = "tools/xtask/architecture.json";

/// The result of one check, as the run prints it and the receipt records it.
pub struct CheckResult {
    pub name: &'static str,
    pub passed: bool,
    pub summary: String,
    pub findings: Vec<String>,
    pub details: Value,
}

impl CheckResult {
    pub fn new(name: &'static str, summary: String, findings: Vec<String>, details: Value) -> Self {
        CheckResult {
            name,
            passed: findings.is_empty(),
            summary,
            findings,
            details,
        }
    }

    fn unavailable(name: &'static str, why: &str) -> Self {
        CheckResult::new(
            name,
            format!("not checked: {why}"),
            vec![why.to_owned()],
            Value::Null,
        )
    }
}

struct Rule {
    package: String,
    direct: bool,
    forbidden: Vec<String>,
    reason: String,
}

pub struct Config {
    rules: Vec<Rule>,
    os_binding_crates: BTreeSet<String>,
    os_import_paths: Vec<(String, String)>,
    os_import_abis: Vec<String>,
    os_import_attributes: Vec<String>,
    /// The distinct word sequences of the denied phrases (`TreeRef` and `tree_ref` are one).
    tree_reference_phrases: Vec<Vec<String>>,
}

fn string_list(value: &Value, what: &str) -> Result<Vec<String>, String> {
    value
        .as_array()
        .ok_or_else(|| format!("{what} is not an array"))?
        .iter()
        .map(|item| {
            item.as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| format!("{what} holds an item that is not a non-empty string"))
        })
        .collect()
}

fn only_keys(value: &Value, keys: &[&str], what: &str) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{what} is not an object"))?;
    for key in keys {
        if !object.contains_key(*key) {
            return Err(format!("{what} has no \"{key}\""));
        }
    }
    match object.keys().find(|key| !keys.contains(&key.as_str())) {
        Some(key) => Err(format!("{what} has an unknown key \"{key}\"")),
        None => Ok(()),
    }
}

impl Config {
    pub fn parse(text: &str) -> Result<Config, String> {
        let value: Value =
            serde_json::from_str(text).map_err(|e| format!("{CONFIG_PATH} is not JSON: {e}"))?;
        only_keys(
            &value,
            &[
                "dependency_rules",
                "os_binding_crates",
                "os_import_paths",
                "os_import_abis",
                "os_import_attributes",
                "tree_reference_phrases",
            ],
            CONFIG_PATH,
        )?;
        let mut rules = Vec::new();
        for rule in value["dependency_rules"]
            .as_array()
            .ok_or("dependency_rules is not an array")?
        {
            only_keys(
                rule,
                &["package", "scope", "forbidden", "reason"],
                "a dependency rule",
            )?;
            let package = rule["package"]
                .as_str()
                .ok_or("a dependency rule has no package")?;
            let direct = match rule["scope"].as_str() {
                Some("direct") => true,
                Some("closure") => false,
                _ => {
                    return Err(format!(
                        "the rule for {package} has a scope other than \"direct\" or \"closure\""
                    ));
                }
            };
            rules.push(Rule {
                package: package.to_owned(),
                direct,
                forbidden: string_list(&rule["forbidden"], "a rule's forbidden list")?,
                reason: rule["reason"]
                    .as_str()
                    .ok_or("a dependency rule has no reason")?
                    .to_owned(),
            });
        }
        let mut os_import_paths = Vec::new();
        for path in string_list(&value["os_import_paths"], "os_import_paths")? {
            let (krate, module) = path
                .split_once("::")
                .filter(|(k, m)| !k.is_empty() && !m.is_empty() && !m.contains("::"))
                .ok_or_else(|| format!("os_import_paths entry {path} is not <crate>::<module>"))?;
            os_import_paths.push((krate.to_owned(), module.to_owned()));
        }
        let mut tree_reference_phrases: Vec<Vec<String>> = Vec::new();
        for phrase in string_list(&value["tree_reference_phrases"], "tree_reference_phrases")? {
            let words = text::phrase_words(&phrase);
            if words.is_empty() {
                return Err(format!(
                    "tree_reference_phrases entry {phrase:?} has no words"
                ));
            }
            if !tree_reference_phrases.contains(&words) {
                tree_reference_phrases.push(words);
            }
        }
        Ok(Config {
            rules,
            os_binding_crates: string_list(&value["os_binding_crates"], "os_binding_crates")?
                .into_iter()
                .collect(),
            os_import_paths,
            os_import_abis: string_list(&value["os_import_abis"], "os_import_abis")?,
            os_import_attributes: string_list(
                &value["os_import_attributes"],
                "os_import_attributes",
            )?,
            tree_reference_phrases,
        })
    }

    fn load(root: &Path) -> Result<Config, String> {
        let path = root.join(CONFIG_PATH);
        let text =
            fs::read_to_string(&path).map_err(|e| format!("cannot read {CONFIG_PATH}: {e}"))?;
        Config::parse(&text)
    }
}

fn normal_or_build(edge: &Edge) -> bool {
    edge.normal || edge.build
}

fn normal_only(edge: &Edge) -> bool {
    edge.normal
}

type ResolvedTargets = Vec<(String, Result<Resolved, String>)>;
type Sources = BTreeMap<String, Result<Vec<SourceFile>, String>>;

/// The triples the dependency checks resolve: every shipped triple, plus the host triple when it
/// is none of them.
fn resolved_targets(ws: &Workspace) -> ResolvedTargets {
    let mut targets: Vec<String> = SHIPPED_TARGETS.iter().map(|t| (*t).to_owned()).collect();
    let mut resolved: ResolvedTargets = Vec::new();
    match cargo::rustc_version() {
        Ok((host, _)) if !targets.contains(&host) => targets.push(host),
        Ok(_) => {}
        Err(e) => resolved.push(("host".to_owned(), Err(e))),
    }
    for target in targets {
        let graph = Resolved::load(ws, &target);
        resolved.push((target, graph));
    }
    resolved
}

/// Runs the five checks in order. Logs of the cargo runs they start go to `log_dir`.
pub fn run_all(ws: &Workspace, log_dir: &Path) -> Vec<CheckResult> {
    let config = Config::load(&ws.root);
    let resolved = resolved_targets(ws);
    let parse = |roots: Vec<std::path::PathBuf>| rust_source::crate_sources(&ws.root, &roots);
    let sources: Sources = ws
        .members
        .iter()
        .map(|member| (member.name.clone(), parse(member.shipped_roots())))
        .collect();
    let build_sources: Sources = ws
        .members
        .iter()
        .map(|member| (member.name.clone(), parse(member.build_script_roots())))
        .collect();
    vec![
        match &config {
            Ok(config) => dependencies(ws, config, &resolved),
            Err(e) => CheckResult::unavailable("dependencies", e),
        },
        match &config {
            Ok(config) => os_imports(ws, config, &resolved, &sources, &build_sources),
            Err(e) => CheckResult::unavailable("os-imports", e),
        },
        wasm_refusal(ws, log_dir),
        match &config {
            Ok(config) => tree_reference_tokens(ws, config, &sources, &build_sources),
            Err(e) => CheckResult::unavailable("tree-reference-tokens", e),
        },
    ]
}

fn target_names(resolved: &ResolvedTargets) -> String {
    resolved
        .iter()
        .map(|(target, _)| target.as_str())
        .collect::<Vec<_>>()
        .join(" and ")
}

fn dependencies(ws: &Workspace, config: &Config, resolved: &ResolvedTargets) -> CheckResult {
    let mut findings = Vec::new();
    for member in &ws.members {
        if !member.is_portable() && !member.is_native_only() {
            findings.push(format!(
                "{} declares portability {:?}; expected \"portable\" or \"native-only\"",
                member.name, member.portability
            ));
        }
    }
    for rule in &config.rules {
        for name in std::iter::once(&rule.package).chain(&rule.forbidden) {
            if ws.member(name).is_none() {
                findings.push(format!(
                    "the rule for {} names {name}, which is not a workspace member",
                    rule.package
                ));
            }
        }
    }
    let native_only: BTreeSet<&str> = ws
        .members
        .iter()
        .filter(|member| member.is_native_only())
        .map(|member| member.name.as_str())
        .collect();
    let mut details = Map::new();
    for (target, result) in resolved {
        let graph = match result {
            Ok(graph) => graph,
            Err(e) => {
                findings.push(format!("[{target}] {e}"));
                continue;
            }
        };
        let mut closures = Map::new();
        for member in &ws.members {
            let reach = graph.reach(&member.id, normal_or_build, false);
            let in_workspace: Vec<&String> = reach
                .iter()
                .filter(|name| ws.member(name).is_some())
                .collect();
            closures.insert(member.name.clone(), json!(in_workspace));
            if member.is_portable() {
                for name in reach
                    .iter()
                    .filter(|name| native_only.contains(name.as_str()))
                {
                    findings.push(format!(
                        "[{target}] portable {} has native-only {name} in its dependency closure (plan 1.3: native implementations do not enter portable library or browser builds)",
                        member.name
                    ));
                }
            }
        }
        for rule in &config.rules {
            let Some(member) = ws.member(&rule.package) else {
                continue;
            };
            let reach = graph.reach(&member.id, normal_or_build, rule.direct);
            for forbidden in rule.forbidden.iter().filter(|name| reach.contains(*name)) {
                let relation = if rule.direct {
                    "depends directly on"
                } else {
                    "has in its dependency closure"
                };
                findings.push(format!(
                    "[{target}] {} {relation} {forbidden} ({})",
                    rule.package, rule.reason
                ));
            }
        }
        details.insert(target.clone(), json!({ "workspace_closures": closures }));
    }
    let summary = format!(
        "{} layering rules and the portable/native-only rule over normal and build dependencies with all features, on {}",
        config.rules.len(),
        target_names(resolved)
    );
    let details = json!({
        "targets": resolved.iter().map(|(target, _)| target.as_str()).collect::<Vec<_>>(),
        "by_target": details,
    });
    CheckResult::new("dependencies", summary, findings, details)
}

/// The macros that read another file into the crate, which the source scans do not follow.
const INCLUDE_MACROS: [&str; 3] = ["include", "include_str", "include_bytes"];

/// Every `include!`, `include_str!` or `include_bytes!` in one file's non-test tokens, with its
/// line. `check` names the check whose scan the macro defeats.
fn include_uses(file: &SourceFile, check: &str, found: &mut BTreeSet<(usize, String)>) {
    rust_source::each_level(file.syntax.to_token_stream(), &mut |tokens| {
        for i in 0..tokens.len() {
            if let Some(name) = rust_source::ident_text(tokens.get(i))
                && INCLUDE_MACROS.contains(&name.as_str())
                && matches!(tokens.get(i + 1), Some(TokenTree::Punct(p)) if p.as_char() == '!')
            {
                found.insert((
                    rust_source::line_of(&tokens[i]),
                    format!(
                        "{name}! reads a file that {check} does not follow, so that file is unchecked"
                    ),
                ));
            }
        }
    });
}

fn shown(file: &SourceFile, found: BTreeSet<(usize, String)>) -> Vec<String> {
    found
        .into_iter()
        .map(|(line, what)| format!("{}:{line}: {what}", file.rel))
        .collect()
}

/// The include-macro findings of one file for `check`, in line order.
pub fn include_findings(file: &SourceFile, check: &str) -> Vec<String> {
    let mut found = BTreeSet::new();
    include_uses(file, check, &mut found);
    shown(file, found)
}

/// Every denied OS import, and every include macro, in one file's non-test tokens, one finding per
/// occurrence, in line order.
pub fn os_import_findings(file: &SourceFile, config: &Config) -> Vec<String> {
    let mut found: BTreeSet<(usize, String)> = BTreeSet::new();
    include_uses(file, "os-imports", &mut found);
    rust_source::each_level(file.syntax.to_token_stream(), &mut |tokens| {
        for i in 0..tokens.len() {
            if let Some(first) = rust_source::ident_text(tokens.get(i)) {
                let line = rust_source::line_of(&tokens[i]);
                let modules: Vec<&str> = config
                    .os_import_paths
                    .iter()
                    .filter(|(krate, _)| *krate == first)
                    .map(|(_, module)| module.as_str())
                    .collect();
                if !modules.is_empty() {
                    if rust_source::ident_text(tokens.get(i + 1)).as_deref() == Some("as") {
                        found.insert((
                            line,
                            format!(
                                "`{first} as ...` renames {first}, which hides its module paths"
                            ),
                        ));
                    }
                    if rust_source::is_path_separator(tokens, i + 1) {
                        match tokens.get(i + 3) {
                            Some(TokenTree::Ident(ident)) => {
                                let name = ident.to_string();
                                if modules.contains(&name.as_str()) {
                                    found.insert((line, format!("{first}::{name}")));
                                }
                            }
                            Some(TokenTree::Group(group))
                                if group.delimiter() == Delimiter::Brace =>
                            {
                                for (name, at) in use_group_heads(group.stream()) {
                                    if modules.contains(&name.as_str()) {
                                        found.insert((at, format!("{first}::{{{name}}}")));
                                    }
                                }
                            }
                            Some(TokenTree::Punct(p)) if p.as_char() == '*' => {
                                found.insert((
                                    line,
                                    format!(
                                        "{first}::* (a glob import brings {} into scope)",
                                        modules.join(", ")
                                    ),
                                ));
                            }
                            _ => {}
                        }
                    }
                }
                if first == "extern"
                    && let Some(TokenTree::Literal(literal)) = tokens.get(i + 1)
                    && let Some(abi) = rust_source::string_literal_value(literal)
                    && config.os_import_abis.contains(&abi)
                {
                    found.insert((line, format!("extern \"{abi}\"")));
                }
            }
            if let Some(TokenTree::Punct(pound)) = tokens.get(i)
                && pound.as_char() == '#'
            {
                let next = match tokens.get(i + 1) {
                    Some(TokenTree::Punct(bang)) if bang.as_char() == '!' => i + 2,
                    _ => i + 1,
                };
                if let Some(TokenTree::Group(group)) = tokens.get(next)
                    && group.delimiter() == Delimiter::Bracket
                {
                    for (name, at) in idents_anywhere(group.stream()) {
                        if config.os_import_attributes.contains(&name) {
                            found.insert((at, format!("#[{name}] attribute")));
                        }
                    }
                }
            }
        }
    });
    shown(file, found)
}

/// The first identifier of each comma-separated element of a `use` group, with its line.
fn use_group_heads(stream: TokenStream) -> Vec<(String, usize)> {
    let mut heads = Vec::new();
    let mut at_start = true;
    for token in stream {
        match &token {
            TokenTree::Punct(p) if p.as_char() == ',' => at_start = true,
            TokenTree::Ident(ident) if at_start => {
                heads.push((ident.to_string(), rust_source::line_of(&token)));
                at_start = false;
            }
            _ => at_start = false,
        }
    }
    heads
}

/// Every identifier in `stream` at any depth, with its line.
fn idents_anywhere(stream: TokenStream) -> Vec<(String, usize)> {
    let mut found = Vec::new();
    rust_source::each_level(stream, &mut |tokens| {
        for token in tokens {
            if let TokenTree::Ident(ident) = token {
                found.push((ident.to_string(), rust_source::line_of(token)));
            }
        }
    });
    found
}

/// What `os-imports` covers, recorded in every receipt.
const OS_IMPORTS_SCOPE: &str = "the non-test code of the lib and bin targets of every portable package, as parsed, for denied std modules, ABIs and attributes and for include!, include_str! and include_bytes!; build scripts run on the build host and are scanned only for those include macros; plain comments, tests/, benches/, examples/ and #[cfg(test)] items are outside the scan; OS-binding crates are denied as direct normal or build dependencies of a portable package on every resolved target and anywhere in its wasm32-unknown-unknown closure of normal dependencies excluding host proc-macro trees, while a transitive one on a native target (libc or rustix under a std-only crate) is allowed";

fn os_imports(
    ws: &Workspace,
    config: &Config,
    resolved: &ResolvedTargets,
    sources: &Sources,
    build_sources: &Sources,
) -> CheckResult {
    let mut findings = Vec::new();
    let mut files_scanned = Map::new();
    let mut build_files = 0;
    for member in ws.members.iter().filter(|member| member.is_portable()) {
        match sources.get(&member.name) {
            Some(Ok(files)) => {
                files_scanned.insert(member.name.clone(), json!(files.len()));
                for file in files {
                    findings.extend(os_import_findings(file, config));
                }
            }
            Some(Err(e)) => findings.push(format!("{}: {e}", member.name)),
            None => findings.push(format!("{}: no sources were read", member.name)),
        }
        match build_sources.get(&member.name) {
            Some(Ok(files)) => {
                build_files += files.len();
                for file in files {
                    findings.extend(include_findings(file, "os-imports"));
                }
            }
            Some(Err(e)) => findings.push(format!("{} build script: {e}", member.name)),
            None => {}
        }
    }
    let mut wasm_dependents = BTreeMap::new();
    for (target, result) in resolved {
        let Ok(graph) = result else {
            findings.push(format!(
                "[{target}] the resolved dependencies are not available"
            ));
            continue;
        };
        for member in ws.members.iter().filter(|member| member.is_portable()) {
            let direct = graph.reach(&member.id, normal_or_build, true);
            let closure = if target == WASM_TARGET {
                graph.reach(&member.id, normal_only, false)
            } else {
                BTreeSet::new()
            };
            for name in direct.union(&closure) {
                if !config.os_binding_crates.contains(name) {
                    continue;
                }
                if !direct.contains(name) {
                    let actual = wasm_dependents
                        .entry(name.clone())
                        .or_insert_with(|| graph.normal_dependents(ws, target, name));
                    match actual {
                        Ok(dependents) if !dependents.contains(&member.name) => continue,
                        Ok(_) => {}
                        Err(error) => {
                            findings.push(format!("[{target}] cannot establish {}'s normal dependency on {name}: {error}", member.name));
                            continue;
                        }
                    }
                }
                let relation = if direct.contains(name) {
                    format!("depends directly on the OS-binding crate {name}")
                } else {
                    format!(
                        "has the OS-binding crate {name} in its {WASM_TARGET} dependency closure"
                    )
                };
                findings.push(format!("[{target}] portable {} {relation}", member.name));
            }
        }
    }
    let total: u64 = files_scanned.values().filter_map(Value::as_u64).sum();
    let summary = format!(
        "{total} non-test source files of {} portable packages against {} denied std modules, {} ABIs, {} attributes and the include macros, plus {build_files} build-script files for the include macros; {} OS-binding crates as direct dependencies on {} and in the {WASM_TARGET} closure",
        files_scanned.len(),
        config.os_import_paths.len(),
        config.os_import_abis.len(),
        config.os_import_attributes.len(),
        config.os_binding_crates.len(),
        target_names(resolved)
    );
    CheckResult::new(
        "os-imports",
        summary,
        findings,
        json!({
            "scope": OS_IMPORTS_SCOPE,
            "wasm_normal_dependency_source": "cargo tree --locked --workspace --all-features --target wasm32-unknown-unknown --edges normal,no-proc-macro --invert <resolved-package-id> --prefix none --format {p}",
            "files_scanned": files_scanned,
            "build_script_files_scanned": build_files,
            "targets": resolved.iter().map(|(target, _)| target.as_str()).collect::<Vec<_>>(),
        }),
    )
}

/// Whether `path` lies inside `dir`, comparing text with forward slashes, and case-insensitively
/// on Windows.
fn path_within(path: &Path, dir: &Path) -> bool {
    let normal = |p: &Path| {
        let text = p.to_string_lossy().replace('\\', "/");
        if cfg!(windows) {
            text.to_lowercase()
        } else {
            text
        }
    };
    let (path, dir) = (normal(path), normal(dir));
    path.strip_prefix(dir.trim_end_matches('/'))
        .is_some_and(|rest| rest.starts_with('/'))
}

/// The `compile_error!` refusal among cargo's JSON messages for one package: an error-level
/// compiler message of that package whose primary span lies in the package's own directory and
/// starts with `compile_error!`.
fn refusal_message(
    stdout: &str,
    root: &Path,
    package_id: &str,
    package_dir: &Path,
) -> Option<String> {
    for line in stdout.lines() {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if message["reason"] != "compiler-message"
            || message["package_id"] != package_id
            || message["message"]["level"] != "error"
        {
            continue;
        }
        let refused = message["message"]["spans"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|span| span["is_primary"] == true)
            .any(|span| {
                let file = span["file_name"].as_str().map(|f| root.join(f));
                let first = span["text"][0]["text"].as_str().unwrap_or_default();
                file.is_some_and(|file| path_within(&file, package_dir))
                    && first.trim_start().starts_with("compile_error!")
            });
        if refused {
            return message["message"]["message"].as_str().map(str::to_owned);
        }
    }
    None
}

fn wasm_refusal(ws: &Workspace, log_dir: &Path) -> CheckResult {
    let mut findings = Vec::new();
    let mut details = Map::new();
    let _ = fs::create_dir_all(log_dir);
    let native: Vec<_> = ws.members.iter().filter(|m| m.is_native_only()).collect();
    for member in &native {
        // A cached successful build would answer for this run's compile, and cargo trusts file
        // modification times, so the package's wasm32 artifacts are removed first.
        let mut clean = Command::new(cargo::cargo_program());
        clean
            .args([
                "clean",
                "-p",
                &member.name,
                "--target",
                WASM_TARGET,
                "--locked",
            ])
            .arg("--target-dir")
            .arg(ws.accept_target_dir())
            .current_dir(&ws.root);
        let mut command = Command::new(cargo::cargo_program());
        command
            .args([
                "check",
                "-p",
                &member.name,
                "--target",
                WASM_TARGET,
                "--locked",
            ])
            .arg("--message-format=json")
            .arg("--target-dir")
            .arg(ws.accept_target_dir())
            .current_dir(&ws.root);
        let (clean_argv, argv) = (cargo::argv(&clean), cargo::argv(&command));
        let log = log_dir.join(format!("wasm-refusal-{}.log", member.name));
        let cleaned = match cargo::output(&mut clean) {
            Ok(out) if out.status.success() => Ok(out),
            Ok(out) => Err(format!(
                "{clean_argv:?} exited with {:?}: {}",
                out.status.code(),
                cargo::tail(&out.stderr, 10)
            )),
            Err(e) => Err(e),
        };
        let cleaned = match cleaned {
            Ok(out) => out,
            Err(e) => {
                findings.push(format!("{}: {e}", member.name));
                continue;
            }
        };
        let out = match cargo::output(&mut command) {
            Ok(out) => out,
            Err(e) => {
                findings.push(format!("{}: {e}", member.name));
                continue;
            }
        };
        let _ = fs::write(
            &log,
            format!(
                "{clean_argv:?}
exit code: {:?}
--- stderr
{}

{argv:?}
exit code: {:?}
--- stdout
{}
--- stderr
{}",
                cleaned.status.code(),
                cargo::text(&cleaned.stderr),
                out.status.code(),
                cargo::text(&out.stdout),
                cargo::text(&out.stderr)
            ),
        );
        let refusal = if out.status.success() {
            findings.push(format!(
                "{} compiled for {WASM_TARGET}; a native-only package must refuse it with compile_error!",
                member.name
            ));
            None
        } else {
            let refusal =
                refusal_message(&cargo::text(&out.stdout), &ws.root, &member.id, &member.dir);
            if refusal.is_none() {
                findings.push(format!(
                    "{} failed to compile for {WASM_TARGET} (exit code {:?}), but not through a compile_error! in its own sources; see {}",
                    member.name,
                    out.status.code(),
                    ws.rel(&log)
                ));
            }
            refusal
        };
        details.insert(
            member.name.clone(),
            json!({
                "clean_argv": clean_argv,
                "argv": argv,
                "exit_code": out.status.code(),
                "refusal": refusal,
                "log": ws.rel(&log),
            }),
        );
    }
    let names: Vec<&str> = native.iter().map(|m| m.name.as_str()).collect();
    let summary = format!(
        "{} native-only packages ({}) compiled afresh for {WASM_TARGET}; each must fail through its own compile_error!",
        names.len(),
        names.join(", ")
    );
    CheckResult::new("wasm-refusal", summary, findings, Value::Object(details))
}

/// Every denied phrase in one file's identifiers, string literals and doc comments, and every
/// include macro, in line order.
pub fn tree_reference_findings(file: &SourceFile, config: &Config) -> Vec<String> {
    let mut found: BTreeSet<(usize, String)> = BTreeSet::new();
    include_uses(file, "tree-reference-tokens", &mut found);
    rust_source::each_level(file.syntax.to_token_stream(), &mut |tokens| {
        for token in tokens {
            let (kind, value) = match token {
                TokenTree::Ident(ident) => ("identifier", ident.to_string()),
                TokenTree::Literal(literal) => match rust_source::string_literal_value(literal) {
                    Some(value) => ("string or doc comment", value),
                    None => continue,
                },
                _ => continue,
            };
            let pieces = text::pieces(&value, true);
            for words in &config.tree_reference_phrases {
                for (start, end) in text::find_words(&value, &pieces, words, true) {
                    found.insert((
                        rust_source::line_of(token),
                        format!(
                            "{kind} contains `{}` (denied phrase `{}`)",
                            &value[start..end],
                            words.join(" ")
                        ),
                    ));
                }
            }
        }
    });
    shown(file, found)
}

/// What `tree-reference-tokens` covers, recorded in every receipt.
const TREE_REFERENCE_SCOPE: &str = "identifiers, string literals and doc comments in the non-test code of every portable package's lib and bin targets and build script (build.rs), as parsed; plain comments, tests/, benches/, examples/ and #[cfg(test)] items are outside its scope by design; include!, include_str! and include_bytes! are not followed, so any use of them fails this check";

fn tree_reference_tokens(
    ws: &Workspace,
    config: &Config,
    sources: &Sources,
    build_sources: &Sources,
) -> CheckResult {
    let mut findings = Vec::new();
    let mut files = 0;
    let mut build_files = 0;
    let mut packages = 0;
    for member in ws.members.iter().filter(|member| member.is_portable()) {
        match sources.get(&member.name) {
            Some(Ok(list)) => {
                packages += 1;
                files += list.len();
                for file in list {
                    findings.extend(tree_reference_findings(file, config));
                }
            }
            Some(Err(e)) => findings.push(format!("{}: {e}", member.name)),
            None => findings.push(format!("{}: no sources were read", member.name)),
        }
        match build_sources.get(&member.name) {
            Some(Ok(list)) => {
                build_files += list.len();
                for file in list {
                    findings.extend(tree_reference_findings(file, config));
                }
            }
            Some(Err(e)) => findings.push(format!("{} build script: {e}", member.name)),
            None => {}
        }
    }
    let phrases: Vec<String> = config
        .tree_reference_phrases
        .iter()
        .map(|words| words.join(" "))
        .collect();
    let summary = format!(
        "{files} non-test source files and {build_files} build-script files of {packages} portable packages against {} denied phrases and the include macros",
        phrases.len()
    );
    CheckResult::new(
        "tree-reference-tokens",
        summary,
        findings,
        json!({
            "scope": TREE_REFERENCE_SCOPE,
            "phrases": phrases,
            "files_scanned": files,
            "build_script_files_scanned": build_files,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn real_config() -> Config {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("architecture.json");
        Config::parse(&fs::read_to_string(path).unwrap()).unwrap()
    }

    fn fixture_root(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    fn fixture(name: &str) -> (PathBuf, Vec<SourceFile>) {
        let root = fixture_root(name);
        let files = rust_source::crate_sources(&root, &[root.join("src").join("lib.rs")]).unwrap();
        (root, files)
    }

    fn build_script(name: &str) -> Vec<SourceFile> {
        let root = fixture_root(name);
        rust_source::crate_sources(&root, &[root.join("build.rs")]).unwrap()
    }

    /// Check (d): the reviewed phrase list finds each denied phrase in identifiers, string
    /// literals and doc comments, in any case or joining, in the crate and in its build script,
    /// and nothing in test-only code, plain comments or unrelated words; an include macro, which
    /// the scan does not follow, fails it.
    #[test]
    fn tree_reference_tokens_are_found_in_fixture_sources() {
        let config = real_config();
        let (_, files) = fixture("tree_tokens");
        let found: Vec<String> = files
            .iter()
            .chain(&build_script("tree_tokens"))
            .flat_map(|file| tree_reference_findings(file, &config))
            .collect();
        let expected = [
            "src/lib.rs:3: string or doc comment contains `tree reference` (denied phrase `tree reference`)",
            "src/lib.rs:5: identifier contains `TreeRef` (denied phrase `tree ref`)",
            "src/lib.rs:7: identifier contains `TREE_REF` (denied phrase `tree ref`)",
            "src/lib.rs:9: identifier contains `ReferenceProfile` (denied phrase `reference profile`)",
            "src/lib.rs:11: identifier contains `ReferenceSystemProvider` (denied phrase `reference system provider`)",
            "src/lib.rs:14: identifier contains `profile_id` (denied phrase `profile id`)",
            "src/lib.rs:17: string or doc comment contains `Host-Reference` (denied phrase `host reference`)",
            "src/lib.rs:20: string or doc comment contains `tree-references` (denied phrase `tree reference`)",
            "src/lib.rs:33: include_str! reads a file that tree-reference-tokens does not follow, so that file is unchecked",
            "src/profiles.rs:3: identifier contains `ProfileIds` (denied phrase `profile id`)",
            "build.rs:5: string or doc comment contains `TREE_REF` (denied phrase `tree ref`)",
            "build.rs:6: include_bytes! reads a file that tree-reference-tokens does not follow, so that file is unchecked",
        ];
        assert_eq!(found, expected, "{found:#?}");
    }

    /// Check (b): every denied std module path, use-group member, glob, rename, ABI, attribute and
    /// include macro in non-test code is found with its line; test-only code, strings and doc
    /// comments are not. In a build script only the include macro is reported.
    #[test]
    fn os_imports_are_found_in_fixture_sources() {
        let config = real_config();
        let (_, files) = fixture("os_imports");
        let found: Vec<String> = files
            .iter()
            .flat_map(|file| os_import_findings(file, &config))
            .collect();
        let expected = [
            "src/lib.rs:10: std::fs",
            "src/lib.rs:12: std::{net}",
            "src/lib.rs:14: std::* (a glob import brings fs, process, os, net into scope)",
            "src/lib.rs:16: `std as ...` renames std, which hides its module paths",
            "src/lib.rs:19: std::process",
            "src/lib.rs:22: #[link] attribute",
            "src/lib.rs:23: extern \"system\"",
            "src/lib.rs:28: std::os",
            "src/lib.rs:37: include_bytes! reads a file that os-imports does not follow, so that file is unchecked",
        ];
        assert_eq!(found, expected, "{found:#?}");
        let build: Vec<String> = build_script("os_imports")
            .iter()
            .flat_map(|file| include_findings(file, "os-imports"))
            .collect();
        assert_eq!(
            build,
            [
                "build.rs:6: include_str! reads a file that os-imports does not follow, so that file is unchecked"
            ],
            "{build:#?}"
        );
    }

    #[test]
    fn the_real_configuration_parses_and_names_the_plan_rules() {
        let config = real_config();
        let rule = |package: &str| config.rules.iter().find(|r| r.package == package).unwrap();
        assert_eq!(rule("dna-onecalc").forbidden, ["calc-engine", "calc-host"]);
        assert!(!rule("dna-onecalc").direct);
        assert_eq!(
            rule("calc-fml").forbidden,
            ["calc-engine", "calc-host", "calc-doc", "calc-vba"]
        );
        assert_eq!(
            rule("dna-calc").forbidden,
            [
                "calc-host",
                "calc-engine",
                "calc-doc",
                "dna-calc-worker",
                "dna-onecalc"
            ]
        );
        assert!(!rule("dna-calc").direct);
        assert_eq!(
            rule("dna-calc-protocol").forbidden,
            [
                "calc-host",
                "calc-engine",
                "calc-doc",
                "dna-calc-worker",
                "dna-calc",
                "dna-formula-ui"
            ]
        );
        assert!(!rule("dna-calc-protocol").direct);
        assert_eq!(
            rule("dna-calc-worker").forbidden,
            ["dna-calc", "dna-onecalc"]
        );
        assert!(!rule("dna-calc-worker").direct);
        assert!(config.os_binding_crates.contains("windows-sys"));
    }
}
