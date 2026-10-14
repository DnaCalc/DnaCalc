//! Check (e), plan section 8.8 and honesty rule 4 (no seams without traffic). Each portable
//! package keeps a reviewed `seams.json` beside its `Cargo.toml`:
//!
//! ```text
//! {"package": "<name>", "seams": [{"item": "<module path>::<Enum>::<Variant>" or "<module path>::<Trait>",
//!   "callers": [{"file": "<path from the workspace root>", "fn": "<fn>" or "<Type>::<fn>"}],
//!   "case": "<registered acceptance case that executes a caller>"}]}
//! ```
//!
//! The module path is empty for items at the crate root (`"item": "Color::Red"`). Every pub enum
//! variant and every pub trait of the package's non-test sources, as parsed, must be listed; every
//! listed item must exist; at least one caller of each item must be a function in the non-test
//! sources of a workspace member whose tokens name the item (`Enum::Variant`, or `Self::Variant`
//! inside an impl of the enum; the trait's name for a trait); and the case must be registered in
//! an acceptance manifest.
//!
//! This is the manifest half of plan section 8.8's "reviewed call-site manifest plus integration
//! coverage". The coverage half, proving that the named case really executes the listed caller
//! and refusing a caller inside the defining library, is owned by bead `dc-pz3u`. Until it lands,
//! no entry's reach is proven: every receipt counts the listed entries whose reach is unproven
//! and names `dc-pz3u` beside the count.

use crate::arch::CheckResult;
use crate::cargo::Workspace;
use crate::manifest::Manifests;
use crate::rust_source::{self, SourceFile};
use proc_macro2::{TokenStream, TokenTree};
use quote::ToTokens;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use syn::{ImplItem, Item, TraitItem, Type, Visibility};

pub const SEAMS_FILE: &str = "seams.json";

/// The bead that owns the coverage half of this check (proving each entry's case reaches its
/// caller).
pub const REACH_OWNER: &str = "dc-pz3u";

#[derive(Debug, PartialEq, Eq)]
pub enum Kind {
    Variant { enum_name: String, variant: String },
    Trait { name: String },
}

/// One pub enum variant or pub trait of a package.
#[derive(Debug)]
pub struct PubItem {
    pub key: String,
    pub kind: Kind,
    pub file: String,
    pub line: usize,
}

fn key(module: &[String], parts: &[String]) -> String {
    module
        .iter()
        .chain(parts)
        .cloned()
        .collect::<Vec<_>>()
        .join("::")
}

fn walk_items(items: &[Item], module: &[String], file: &str, out: &mut Vec<PubItem>) {
    for item in items {
        match item {
            Item::Enum(declared) if matches!(declared.vis, Visibility::Public(_)) => {
                let enum_name = declared.ident.to_string();
                for variant in &declared.variants {
                    let name = variant.ident.to_string();
                    out.push(PubItem {
                        key: key(module, &[enum_name.clone(), name.clone()]),
                        kind: Kind::Variant {
                            enum_name: enum_name.clone(),
                            variant: name,
                        },
                        file: file.to_owned(),
                        line: variant.ident.span().start().line,
                    });
                }
            }
            Item::Trait(declared) if matches!(declared.vis, Visibility::Public(_)) => {
                let name = declared.ident.to_string();
                out.push(PubItem {
                    key: key(module, std::slice::from_ref(&name)),
                    kind: Kind::Trait { name },
                    file: file.to_owned(),
                    line: declared.ident.span().start().line,
                });
            }
            Item::Mod(declared) => {
                if let Some((_, inner)) = &declared.content {
                    let mut child = module.to_vec();
                    child.push(declared.ident.to_string());
                    walk_items(inner, &child, file, out);
                }
            }
            _ => {}
        }
    }
}

/// Every pub enum variant and pub trait in `files`, in file and declaration order.
pub fn pub_items(files: &[SourceFile]) -> Vec<PubItem> {
    let mut out = Vec::new();
    for file in files {
        walk_items(&file.syntax.items, &file.module, &file.rel, &mut out);
    }
    out
}

struct Caller {
    file: String,
    function: String,
}

struct Entry {
    item: String,
    callers: Vec<Caller>,
    case: String,
}

fn parse_seams(text: &str, package: &str, shown: &str) -> Result<Vec<Entry>, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| format!("{shown} is not valid JSON: {e}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| format!("{shown} is not a JSON object"))?;
    if object.len() != 2 || object.get("package").and_then(Value::as_str) != Some(package) {
        return Err(format!(
            "{shown} must hold exactly \"package\": \"{package}\" and \"seams\""
        ));
    }
    let seams = object
        .get("seams")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{shown}: \"seams\" is not an array"))?;
    let mut entries = Vec::new();
    for seam in seams {
        let shape = || format!("{shown}: {seam} is not {{\"item\", \"callers\", \"case\"}}");
        let fields = seam
            .as_object()
            .filter(|o| o.len() == 3)
            .ok_or_else(shape)?;
        let item = fields
            .get("item")
            .and_then(Value::as_str)
            .ok_or_else(shape)?;
        let case = fields
            .get("case")
            .and_then(Value::as_str)
            .ok_or_else(shape)?;
        let mut callers = Vec::new();
        for caller in fields
            .get("callers")
            .and_then(Value::as_array)
            .filter(|callers| !callers.is_empty())
            .ok_or_else(shape)?
        {
            let pair = caller.as_object().filter(|o| o.len() == 2);
            let file = pair.and_then(|o| o.get("file")).and_then(Value::as_str);
            let function = pair.and_then(|o| o.get("fn")).and_then(Value::as_str);
            match (file, function) {
                (Some(file), Some(function)) => callers.push(Caller {
                    file: file.to_owned(),
                    function: function.to_owned(),
                }),
                _ => {
                    return Err(format!(
                        "{shown}: caller {caller} of {item} is not {{\"file\", \"fn\"}}"
                    ));
                }
            }
        }
        entries.push(Entry {
            item: item.to_owned(),
            callers,
            case: case.to_owned(),
        });
    }
    Ok(entries)
}

fn type_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(path) => path.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

/// The tokens of every non-test function named `name` in `items` (inside an impl of or a trait
/// named `owner`, when given), each with the self type of its impl.
fn find_functions(
    items: &[Item],
    owner: Option<&str>,
    name: &str,
    out: &mut Vec<(Option<String>, TokenStream)>,
) {
    for item in items {
        match item {
            Item::Fn(function) if owner.is_none() && function.sig.ident == name => {
                out.push((None, function.to_token_stream()));
            }
            Item::Impl(block) => {
                let self_name = type_name(&block.self_ty);
                if owner.is_some() && owner != self_name.as_deref() {
                    continue;
                }
                for inner in &block.items {
                    if let ImplItem::Fn(function) = inner
                        && function.sig.ident == name
                    {
                        out.push((self_name.clone(), function.to_token_stream()));
                    }
                }
            }
            Item::Trait(block) => {
                if owner.is_some_and(|owner| block.ident != owner) {
                    continue;
                }
                for inner in &block.items {
                    if let TraitItem::Fn(function) = inner
                        && function.default.is_some()
                        && function.sig.ident == name
                    {
                        out.push((None, function.to_token_stream()));
                    }
                }
            }
            Item::Mod(module) => {
                if let Some((_, inner)) = &module.content {
                    find_functions(inner, owner, name, out);
                }
            }
            _ => {}
        }
    }
}

/// Whether `tokens` name the item: `Enum::Variant`, `Self::Variant` inside an impl of the enum,
/// or the trait's name.
fn names_item(tokens: TokenStream, kind: &Kind, impl_self: Option<&str>) -> bool {
    let mut found = false;
    rust_source::each_level(tokens, &mut |level| {
        for i in 0..level.len() {
            match kind {
                Kind::Variant { enum_name, variant } => {
                    let head = rust_source::ident_text(level.get(i));
                    let head_matches = head.as_deref() == Some(enum_name.as_str())
                        || (head.as_deref() == Some("Self")
                            && impl_self == Some(enum_name.as_str()));
                    if head_matches
                        && rust_source::is_path_separator(level, i + 1)
                        && rust_source::ident_text(level.get(i + 3)).as_deref()
                            == Some(variant.as_str())
                    {
                        found = true;
                    }
                }
                Kind::Trait { name } => {
                    if matches!(level.get(i), Some(TokenTree::Ident(ident)) if ident == name) {
                        found = true;
                    }
                }
            }
        }
    });
    found
}

/// Checks one package's pub items against its seams manifest. `all` maps each non-test source
/// file of the workspace to its parsed syntax; `cases` holds every registered case name.
pub fn check_package(
    package: &str,
    shown: &str,
    seams_text: Option<&str>,
    own: &[SourceFile],
    all: &BTreeMap<String, &SourceFile>,
    cases: &BTreeSet<String>,
) -> (Vec<String>, Value) {
    let mut findings = Vec::new();
    let items = pub_items(own);
    let entries = match seams_text {
        None => {
            findings.push(format!("{package} has no {shown}"));
            Vec::new()
        }
        Some(text) => parse_seams(text, package, shown).unwrap_or_else(|e| {
            findings.push(e);
            Vec::new()
        }),
    };
    let listed: BTreeSet<&str> = entries.iter().map(|entry| entry.item.as_str()).collect();
    for item in &items {
        if !listed.contains(item.key.as_str()) {
            findings.push(format!(
                "{}:{}: pub {} {} of {package} is not listed in {shown}",
                item.file,
                item.line,
                match item.kind {
                    Kind::Variant { .. } => "enum variant",
                    Kind::Trait { .. } => "trait",
                },
                item.key
            ));
        }
    }
    for entry in &entries {
        let Some(item) = items.iter().find(|item| item.key == entry.item) else {
            findings.push(format!(
                "{shown} lists {}, which is not a pub enum variant or pub trait of {package}",
                entry.item
            ));
            continue;
        };
        if !cases.contains(&entry.case) {
            findings.push(format!(
                "{shown}: the case {} for {} is not registered in any acceptance manifest",
                entry.case, entry.item
            ));
        }
        let mut reached = false;
        for caller in &entry.callers {
            let Some(file) = all.get(&caller.file) else {
                findings.push(format!(
                    "{shown}: caller {}::{} of {} is not in the non-test sources of a workspace member",
                    caller.file, caller.function, entry.item
                ));
                continue;
            };
            let (owner, name) = match caller.function.rsplit_once("::") {
                Some((owner, name)) => (Some(owner), name),
                None => (None, caller.function.as_str()),
            };
            let mut functions = Vec::new();
            find_functions(&file.syntax.items, owner, name, &mut functions);
            if functions.is_empty() {
                findings.push(format!(
                    "{shown}: {} has no non-test function {} (caller of {})",
                    caller.file, caller.function, entry.item
                ));
            } else if functions
                .into_iter()
                .any(|(impl_self, tokens)| names_item(tokens, &item.kind, impl_self.as_deref()))
            {
                reached = true;
            } else {
                findings.push(format!(
                    "{shown}: {}::{} does not name {}",
                    caller.file, caller.function, entry.item
                ));
            }
        }
        if !reached {
            findings.push(format!(
                "{shown}: {} has no non-test caller that names it",
                entry.item
            ));
        }
    }
    let variants = items
        .iter()
        .filter(|item| matches!(item.kind, Kind::Variant { .. }))
        .count();
    // No entry's reach is proven here: that the case executes the caller is dc-pz3u's half.
    let details = json!({
        "pub_enum_variants": variants,
        "pub_traits": items.len() - variants,
        "entries": entries.len(),
        "entries_reach_unproven": entries.len(),
    });
    (findings, details)
}

pub fn check(
    ws: &Workspace,
    sources: &BTreeMap<String, Result<Vec<SourceFile>, String>>,
    manifests: &Manifests,
) -> CheckResult {
    let mut findings = Vec::new();
    let mut all: BTreeMap<String, &SourceFile> = BTreeMap::new();
    for (member, result) in sources {
        match result {
            Ok(files) => {
                for file in files {
                    all.insert(file.rel.clone(), file);
                }
            }
            Err(e) => findings.push(format!("{member}: {e}")),
        }
    }
    let cases = manifests.case_names();
    let mut packages_detail = Map::new();
    let (mut variants, mut traits, mut entries, mut unproven, mut packages) = (0, 0, 0, 0, 0);
    for member in ws.members.iter().filter(|member| member.is_portable()) {
        let Some(Ok(own)) = sources.get(&member.name) else {
            continue;
        };
        packages += 1;
        let path = member.dir.join(SEAMS_FILE);
        let shown = ws.rel(&path);
        let text = std::fs::read_to_string(&path).ok();
        let (found, detail) =
            check_package(&member.name, &shown, text.as_deref(), own, &all, &cases);
        findings.extend(found);
        variants += detail["pub_enum_variants"].as_u64().unwrap_or_default();
        traits += detail["pub_traits"].as_u64().unwrap_or_default();
        entries += detail["entries"].as_u64().unwrap_or_default();
        unproven += detail["entries_reach_unproven"]
            .as_u64()
            .unwrap_or_default();
        packages_detail.insert(member.name.clone(), detail);
    }
    let summary = format!(
        "{packages} portable packages declare {variants} pub enum variants and {traits} pub traits; their seams.json files list {entries} entries, {unproven} of them with reach unproven (the case reaching the caller is owned by {REACH_OWNER})"
    );
    let details = json!({
        "packages": packages_detail,
        "reach": {
            "entries_reach_unproven": unproven,
            "owner": REACH_OWNER,
            "what": "the manifest half only: each listed caller exists in non-test source and names the item, and the listed case is registered; that the case executes the caller is not yet proven",
        },
    });
    CheckResult::new("seams", summary, findings, details)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Check (e) on the fixture under tests/fixtures/seams: `lib_pkg` declares pub enum `Color`
    /// (Red, Green, Blue), a pub enum `shapes::Shape` (Circle) and pub trait `Paint`, plus a
    /// test-only pub enum; `app_pkg` has the callers. The manifest lists Red with a real caller,
    /// Green whose only naming caller is test-only, Circle named through `Self::` in an impl,
    /// Paint, a stale Purple, and Blue not at all.
    #[test]
    fn seams_are_checked_against_parsed_items_and_callers() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("seams");
        let lib = rust_source::crate_sources(&root, &[root.join("lib_pkg/src/lib.rs")]).unwrap();
        let app = rust_source::crate_sources(&root, &[root.join("app_pkg/src/lib.rs")]).unwrap();
        let items = pub_items(&lib);
        let keys: Vec<&str> = items.iter().map(|item| item.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "Color::Red",
                "Color::Green",
                "Color::Blue",
                "Paint",
                "shapes::Shape::Circle"
            ]
        );
        let all: BTreeMap<String, &SourceFile> = lib
            .iter()
            .chain(&app)
            .map(|file| (file.rel.clone(), file))
            .collect();
        let cases = BTreeSet::from(["fixture_m0_1__paints".to_owned()]);
        let text = std::fs::read_to_string(root.join("lib_pkg/seams.json")).unwrap();
        let (findings, details) = check_package(
            "lib-pkg",
            "lib_pkg/seams.json",
            Some(&text),
            &lib,
            &all,
            &cases,
        );
        let expected = [
            "lib_pkg/src/lib.rs:8: pub enum variant Color::Blue of lib-pkg is not listed in lib_pkg/seams.json",
            "lib_pkg/seams.json: the case fixture_m0_9__unregistered for Color::Green is not registered in any acceptance manifest",
            "lib_pkg/seams.json: app_pkg/src/lib.rs::only_in_tests does not name Color::Green",
            "lib_pkg/seams.json: Color::Green has no non-test caller that names it",
            "lib_pkg/seams.json: app_pkg/src/lib.rs has no non-test function missing_fn (caller of Paint)",
            "lib_pkg/seams.json lists Color::Purple, which is not a pub enum variant or pub trait of lib-pkg",
        ];
        assert_eq!(findings, expected, "{findings:#?}");
        assert_eq!(details["pub_enum_variants"], 4);
        assert_eq!(details["pub_traits"], 1);
        assert_eq!(details["entries_reach_unproven"], 5);

        let (missing, _) = check_package("lib-pkg", "lib_pkg/seams.json", None, &lib, &all, &cases);
        assert_eq!(missing.len(), 6, "{missing:#?}");
        assert_eq!(missing[0], "lib-pkg has no lib_pkg/seams.json");
    }

    /// The manifest half cannot see whether a case reaches its caller (dc-pz3u owns that), so one
    /// listed seam with a real caller and a registered case counts as one entry whose reach is
    /// unproven.
    #[test]
    fn one_listed_seam_counts_one_unproven_reach() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("seams");
        let lib = rust_source::crate_sources(&root, &[root.join("lib_pkg/src/lib.rs")]).unwrap();
        let app = rust_source::crate_sources(&root, &[root.join("app_pkg/src/lib.rs")]).unwrap();
        let all: BTreeMap<String, &SourceFile> = lib
            .iter()
            .chain(&app)
            .map(|file| (file.rel.clone(), file))
            .collect();
        let cases = BTreeSet::from(["fixture_m0_1__paints".to_owned()]);
        let one = r#"{"package": "lib-pkg", "seams": [{"item": "Color::Red",
            "callers": [{"file": "app_pkg/src/lib.rs", "fn": "red"}],
            "case": "fixture_m0_1__paints"}]}"#;
        let (findings, details) =
            check_package("lib-pkg", "one.json", Some(one), &lib, &all, &cases);
        assert!(
            !findings.iter().any(|f| f.contains("Color::Red")),
            "{findings:#?}"
        );
        assert_eq!(details["entries"], 1);
        assert_eq!(details["entries_reach_unproven"], 1);
        assert_eq!(REACH_OWNER, "dc-pz3u");
    }
}
