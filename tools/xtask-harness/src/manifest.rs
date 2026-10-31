//! The per-package acceptance manifests (`<package dir>/acceptance.json`):
//!
//! ```text
//! {"package": "<name>", "beads": [{"id": "<bead>", "test": "<test target>",
//!   "cases": ["<case>", ...], "platforms": {"<case>": ["windows", "linux", "wasm", "excel"], ...},
//!   "commands": [["cargo", ...], ...]}]}
//! ```
//!
//! Every key is required and no other key is accepted. Case names are `<bead in snake case>__<case>`
//! and unique across the workspace; `platforms` names exactly the registered cases; `test` names a
//! test target of the package.

use crate::cargo::Workspace;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Platform {
    Windows,
    Linux,
    Wasm,
    Excel,
}

pub const PLATFORMS: [Platform; 4] = [
    Platform::Windows,
    Platform::Linux,
    Platform::Wasm,
    Platform::Excel,
];

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Windows => "windows",
            Platform::Linux => "linux",
            Platform::Wasm => "wasm",
            Platform::Excel => "excel",
        }
    }

    pub fn parse(text: &str) -> Option<Platform> {
        PLATFORMS
            .into_iter()
            .find(|platform| platform.as_str() == text)
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One registered case and the platforms it declares.
pub struct CaseDecl {
    pub name: String,
    pub platforms: Vec<Platform>,
}

/// One bead's entry in one package's manifest.
pub struct BeadEntry {
    pub package: String,
    pub id: String,
    pub test: String,
    pub cases: Vec<CaseDecl>,
    /// The bead's Acceptance commands, each an argument vector.
    pub commands: Vec<Vec<String>>,
}

/// Every manifest of the workspace, with every problem found in them.
pub struct Manifests {
    pub files: Vec<PathBuf>,
    pub entries: Vec<BeadEntry>,
    pub errors: Vec<String>,
}

/// The bead ID as it prefixes case names: `infra-m0.3` gives `infra_m0_3`.
pub fn snake(id: &str) -> String {
    id.replace(['-', '.'], "_")
}

fn check_keys(
    object: &Map<String, Value>,
    expected: &[&str],
    what: &str,
    errors: &mut Vec<String>,
) {
    for key in expected {
        if !object.contains_key(*key) {
            errors.push(format!("{what} has no \"{key}\""));
        }
    }
    for key in object.keys() {
        if !expected.contains(&key.as_str()) {
            errors.push(format!("{what} has an unknown key \"{key}\""));
        }
    }
}

fn is_bead_id(id: &str) -> bool {
    id.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
}

/// Parses and checks one manifest. `has_test_target` says whether the package has a test target
/// of a given name.
pub fn parse_manifest(
    package: &str,
    shown: &str,
    text: &str,
    has_test_target: &dyn Fn(&str) -> bool,
    errors: &mut Vec<String>,
) -> Vec<BeadEntry> {
    let mut entries = Vec::new();
    let value: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(e) => {
            errors.push(format!("{shown} is not valid JSON: {e}"));
            return entries;
        }
    };
    let Some(object) = value.as_object() else {
        errors.push(format!("{shown} is not a JSON object"));
        return entries;
    };
    check_keys(object, &["package", "beads"], shown, errors);
    if object.get("package").and_then(Value::as_str) != Some(package) {
        errors.push(format!("{shown} does not name package {package}"));
    }
    let Some(beads) = object.get("beads").and_then(Value::as_array) else {
        errors.push(format!("{shown}: \"beads\" is not an array"));
        return entries;
    };
    let mut ids = BTreeSet::new();
    for (index, bead) in beads.iter().enumerate() {
        let what = format!("{shown} beads[{index}]");
        let Some(bead) = bead.as_object() else {
            errors.push(format!("{what} is not an object"));
            continue;
        };
        check_keys(
            bead,
            &["id", "test", "cases", "platforms", "commands"],
            &what,
            errors,
        );
        let Some(id) = bead.get("id").and_then(Value::as_str) else {
            errors.push(format!("{what}: \"id\" is not a string"));
            continue;
        };
        let what = format!("{shown} bead {id}");
        if !is_bead_id(id) {
            errors.push(format!(
                "{what}: the id is not of the form <prefix>-<milestone>.<n>"
            ));
        }
        if !ids.insert(id.to_owned()) {
            errors.push(format!("{what} appears twice"));
        }
        let test = bead.get("test").and_then(Value::as_str).unwrap_or_default();
        if test.is_empty() {
            errors.push(format!("{what}: \"test\" is not a non-empty string"));
        } else if !has_test_target(test) {
            errors.push(format!(
                "{what}: package {package} has no test target named {test}"
            ));
        }
        let prefix = format!("{}__", snake(id));
        let mut names = Vec::new();
        match bead.get("cases").and_then(Value::as_array) {
            Some(cases) if !cases.is_empty() => {
                for case in cases {
                    let Some(name) = case.as_str() else {
                        errors.push(format!("{what}: a case is not a string: {case}"));
                        continue;
                    };
                    let rest = name.strip_prefix(&prefix).unwrap_or_default();
                    if rest.is_empty()
                        || !rest
                            .chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                    {
                        errors.push(format!(
                            "{what}: case {name} is not of the form {prefix}<case>"
                        ));
                    }
                    if names.contains(&name) {
                        errors.push(format!("{what}: case {name} is registered twice"));
                    } else {
                        names.push(name);
                    }
                }
            }
            _ => errors.push(format!("{what}: \"cases\" is not a non-empty array")),
        }
        let mut cases = Vec::new();
        match bead.get("platforms").and_then(Value::as_object) {
            None => errors.push(format!("{what}: \"platforms\" is not an object")),
            Some(platforms) => {
                for key in platforms.keys() {
                    if !names.contains(&key.as_str()) {
                        errors.push(format!(
                            "{what}: \"platforms\" names {key}, which is not a registered case"
                        ));
                    }
                }
                for name in &names {
                    let Some(declared) = platforms.get(*name).and_then(Value::as_array) else {
                        errors.push(format!("{what}: case {name} declares no platforms"));
                        continue;
                    };
                    let mut list = Vec::new();
                    for platform in declared {
                        match platform.as_str().and_then(Platform::parse) {
                            Some(p) if !list.contains(&p) => list.push(p),
                            Some(p) => errors.push(format!(
                                "{what}: case {name} declares platform {p} twice"
                            )),
                            None => errors.push(format!(
                                "{what}: case {name} declares {platform}, which is not windows, linux, wasm or excel"
                            )),
                        }
                    }
                    if list.is_empty() {
                        errors.push(format!("{what}: case {name} declares no platforms"));
                    }
                    cases.push(CaseDecl {
                        name: (*name).to_owned(),
                        platforms: list,
                    });
                }
            }
        }
        let commands: Option<Vec<Vec<String>>> = bead
            .get("commands")
            .and_then(Value::as_array)
            .and_then(|commands| {
                commands
                    .iter()
                    .map(|command| {
                        command
                            .as_array()
                            .filter(|argv| !argv.is_empty())?
                            .iter()
                            .map(|arg| arg.as_str().map(str::to_owned))
                            .collect::<Option<Vec<String>>>()
                    })
                    .collect()
            });
        if commands.is_none() {
            errors.push(format!(
                "{what}: \"commands\" is not an array of non-empty argument vectors"
            ));
        }
        entries.push(BeadEntry {
            package: package.to_owned(),
            id: id.to_owned(),
            test: test.to_owned(),
            cases,
            commands: commands.unwrap_or_default(),
        });
    }
    entries
}

/// Reads and checks the manifest of every workspace member.
pub fn load(ws: &Workspace) -> Manifests {
    let mut manifests = Manifests {
        files: Vec::new(),
        entries: Vec::new(),
        errors: Vec::new(),
    };
    for member in &ws.members {
        let path = member.dir.join("acceptance.json");
        let shown = ws.rel(&path);
        match std::fs::read_to_string(&path) {
            Err(e) => manifests
                .errors
                .push(format!("{} has no readable {shown}: {e}", member.name)),
            Ok(text) => {
                manifests.files.push(path);
                let has_test_target = |name: &str| member.has_test_target(name);
                manifests.entries.extend(parse_manifest(
                    &member.name,
                    &shown,
                    &text,
                    &has_test_target,
                    &mut manifests.errors,
                ));
            }
        }
    }
    let mut owners: BTreeMap<&str, &str> = BTreeMap::new();
    for entry in &manifests.entries {
        for case in &entry.cases {
            if let Some(first) = owners.insert(&case.name, &entry.package) {
                manifests.errors.push(format!(
                    "case {} is registered by both {first} and {}",
                    case.name, entry.package
                ));
            }
        }
    }
    manifests
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> (Vec<BeadEntry>, Vec<String>) {
        let mut errors = Vec::new();
        let entries = parse_manifest(
            "pkg",
            "pkg/acceptance.json",
            text,
            &|t| t == "acceptance",
            &mut errors,
        );
        (entries, errors)
    }

    #[test]
    fn a_complete_manifest_parses() {
        let (entries, errors) = parse(
            r#"{"package": "pkg", "beads": [{"id": "infra-m0.3", "test": "acceptance",
                "cases": ["infra_m0_3__a", "infra_m0_3__b"],
                "platforms": {"infra_m0_3__a": ["windows", "linux"], "infra_m0_3__b": ["wasm"]},
                "commands": [["cargo", "test"]]}]}"#,
        );
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].cases[1].platforms, [Platform::Wasm]);
        assert_eq!(entries[0].commands, [["cargo", "test"]]);
    }

    #[test]
    fn manifest_problems_are_reported() {
        let (_, errors) = parse(
            r#"{"package": "other", "beads": [{"id": "infra-m0.3", "test": "acceptnce",
                "cases": ["infra_m0_3__a", "infra_m0_4__b", "infra_m0_3__a"],
                "platforms": {"infra_m0_3__a": ["windows", "macos"], "infra_m0_3__c": ["linux"]},
                "commands": [[]], "platform": []}]}"#,
        );
        let expected = [
            "does not name package pkg",
            "has an unknown key \"platform\"",
            "has no test target named acceptnce",
            "case infra_m0_4__b is not of the form infra_m0_3__<case>",
            "case infra_m0_3__a is registered twice",
            "\"platforms\" names infra_m0_3__c, which is not a registered case",
            "declares \"macos\", which is not windows, linux, wasm or excel",
            "case infra_m0_4__b declares no platforms",
            "\"commands\" is not an array of non-empty argument vectors",
        ];
        for text in expected {
            assert!(
                errors.iter().any(|e| e.contains(text)),
                "missing {text:?} in {errors:#?}"
            );
        }
        assert_eq!(errors.len(), expected.len(), "{errors:#?}");
    }
}
