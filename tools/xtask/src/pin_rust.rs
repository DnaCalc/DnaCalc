//! What the source pin reads from legacy Rust files, as committed blobs (nothing is built): the
//! names a file exports, the modules it declares and refers to, the function catalogue of OxFunc's
//! `surface_dispatch.rs`, the function IDs a test names and the files a source includes.
//! Test-only items are removed before any of these is read, except where a test file itself is
//! the subject and for module declarations, which keep test-only modules (marked as such).

use crate::rust_source::{
    each_level, ident_text, is_path_separator, is_test_only, path_attribute, strip_tests,
};
use proc_macro2::TokenTree;
use quote::ToTokens;
use std::collections::{BTreeMap, BTreeSet};
use syn::{Attribute, Expr, Item, Meta, Stmt, UseTree, Visibility};

/// A Rust source parsed with its test-only items removed.
pub fn parse(source: &str) -> Result<syn::File, String> {
    let mut file = parse_full(source)?;
    strip_tests(&mut file.items);
    Ok(file)
}

/// A Rust source parsed with every item kept.
pub fn parse_full(source: &str) -> Result<syn::File, String> {
    syn::parse_file(source)
        .map_err(|e| format!("does not parse (line {}): {e}", e.span().start().line))
}

/// One inline module (`mod a { ... }`) enclosing a declaration, with its `#[path]` value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineModule {
    pub name: String,
    pub path: Option<String>,
}

/// One out-of-line `mod name;` declaration of a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModDecl {
    /// The inline modules that enclose it, outermost first.
    pub inline: Vec<InlineModule>,
    pub name: String,
    /// Its `#[path = "..."]` value, as written.
    pub path: Option<String>,
    /// Test-only (`#[cfg(test)]` or similar) on it or on an enclosing inline module.
    pub test_only: bool,
    /// A `#[cfg_attr(..., path = ...)]`, which names a file only under a condition this reader
    /// does not evaluate.
    pub cfg_attr_path: bool,
}

/// Every out-of-line `mod name;` declaration of `file` (parsed with [`parse_full`]), in source
/// order, at any inline-module depth.
pub fn declared_modules(file: &syn::File) -> Vec<ModDecl> {
    let mut out = Vec::new();
    declarations_in(&file.items, &[], false, &mut out);
    out
}

fn has_cfg_attr_path(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| match &attr.meta {
        Meta::List(list) if list.path.is_ident("cfg_attr") => list
            .tokens
            .clone()
            .into_iter()
            .any(|token| ident_text(Some(&token)).as_deref() == Some("path")),
        _ => false,
    })
}

fn declarations_in(
    items: &[Item],
    inline: &[InlineModule],
    test_only: bool,
    out: &mut Vec<ModDecl>,
) {
    for item in items {
        let Item::Mod(declared) = item else {
            continue;
        };
        let name = declared
            .ident
            .to_string()
            .trim_start_matches("r#")
            .to_owned();
        let test_only = test_only || is_test_only(&declared.attrs);
        let path = path_attribute(&declared.attrs);
        match &declared.content {
            Some((_, inner)) => {
                let mut nested = inline.to_vec();
                nested.push(InlineModule { name, path });
                declarations_in(inner, &nested, test_only, out);
            }
            None => out.push(ModDecl {
                inline: inline.to_vec(),
                name,
                path,
                test_only,
                cfg_attr_path: has_cfg_attr_path(&declared.attrs),
            }),
        }
    }
}

fn is_pub(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

/// The names a file exports: its `pub` items (functions, types, traits, constants, statics,
/// re-exports and `#[macro_export]` macros as `name!`), and those of its `pub` inline modules as
/// `module::name`.
pub fn exported(file: &syn::File) -> Vec<String> {
    let mut out = BTreeSet::new();
    exported_items(&file.items, "", &mut out);
    out.into_iter().collect()
}

fn use_leaves(tree: &UseTree, out: &mut Vec<String>) {
    match tree {
        UseTree::Path(path) => use_leaves(&path.tree, out),
        UseTree::Name(name) if name.ident != "self" => out.push(name.ident.to_string()),
        UseTree::Name(_) => {}
        UseTree::Rename(rename) => out.push(rename.rename.to_string()),
        UseTree::Glob(_) => out.push("*".to_owned()),
        UseTree::Group(group) => group.items.iter().for_each(|item| use_leaves(item, out)),
    }
}

fn exported_items(items: &[Item], prefix: &str, out: &mut BTreeSet<String>) {
    for item in items {
        let named = match item {
            Item::Const(i) => is_pub(&i.vis).then(|| i.ident.to_string()),
            Item::Enum(i) => is_pub(&i.vis).then(|| i.ident.to_string()),
            Item::Fn(i) => is_pub(&i.vis).then(|| i.sig.ident.to_string()),
            Item::Static(i) => is_pub(&i.vis).then(|| i.ident.to_string()),
            Item::Struct(i) => is_pub(&i.vis).then(|| i.ident.to_string()),
            Item::Trait(i) => is_pub(&i.vis).then(|| i.ident.to_string()),
            Item::TraitAlias(i) => is_pub(&i.vis).then(|| i.ident.to_string()),
            Item::Type(i) => is_pub(&i.vis).then(|| i.ident.to_string()),
            Item::Union(i) => is_pub(&i.vis).then(|| i.ident.to_string()),
            Item::Macro(i) => i
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("macro_export"))
                .then(|| i.ident.as_ref().map(|ident| format!("{ident}!")))
                .flatten(),
            Item::Use(i) if is_pub(&i.vis) => {
                let mut leaves = Vec::new();
                use_leaves(&i.tree, &mut leaves);
                out.extend(leaves.into_iter().map(|leaf| format!("{prefix}{leaf}")));
                None
            }
            Item::Mod(i) if is_pub(&i.vis) => {
                if let Some((_, inner)) = &i.content {
                    exported_items(inner, &format!("{prefix}{}::", i.ident), out);
                }
                None
            }
            _ => None,
        };
        if let Some(name) = named {
            out.insert(format!("{prefix}{name}"));
        }
    }
}

/// A reference from a file to a module path: in the file's own crate (`krate` is `None`) or in
/// another crate of the same repository. `relative` marks a path that started with a name rather
/// than `crate`, `self` or `super`; it counts only when that name is a child module.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModuleRef {
    pub krate: Option<String>,
    pub path: Vec<String>,
    pub relative: bool,
}

/// Every module path the non-test code of `file` names, from its `use` declarations and from the
/// paths in the rest of its code. `module` is the file's module path inside its crate;
/// `other_crates` are the crate names of the repository's other library crates. A path that
/// starts with a plain name (`decimal::parse`) is kept as `relative`; the caller resolves it only
/// to a child module of `module`. `mod` declarations are read by [`declared_modules`].
pub fn module_refs(
    file: &syn::File,
    module: &[String],
    other_crates: &BTreeSet<String>,
) -> BTreeSet<ModuleRef> {
    let mut out = BTreeSet::new();
    refs_in_items(&file.items, module, other_crates, &mut out);
    out
}

fn refs_in_items(
    items: &[Item],
    module: &[String],
    other_crates: &BTreeSet<String>,
    out: &mut BTreeSet<ModuleRef>,
) {
    for item in items {
        match item {
            Item::Use(declaration) => {
                use_paths(&declaration.tree, Vec::new(), module, other_crates, out)
            }
            Item::Mod(declared) => {
                if let Some((_, inner)) = &declared.content {
                    let mut inner_module = module.to_vec();
                    inner_module.push(declared.ident.to_string());
                    refs_in_items(inner, &inner_module, other_crates, out);
                }
            }
            other => {
                each_level(other.to_token_stream(), &mut |tokens| {
                    path_refs(tokens, module, other_crates, out)
                });
            }
        }
    }
}

fn use_paths(
    tree: &UseTree,
    mut prefix: Vec<String>,
    module: &[String],
    other_crates: &BTreeSet<String>,
    out: &mut BTreeSet<ModuleRef>,
) {
    match tree {
        UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            use_paths(&path.tree, prefix, module, other_crates, out);
        }
        UseTree::Name(name) => {
            if name.ident != "self" {
                prefix.push(name.ident.to_string());
            }
            out.extend(resolve(&prefix, module, other_crates));
        }
        UseTree::Rename(rename) => {
            prefix.push(rename.ident.to_string());
            out.extend(resolve(&prefix, module, other_crates));
        }
        UseTree::Glob(_) => out.extend(resolve(&prefix, module, other_crates)),
        UseTree::Group(group) => {
            for item in &group.items {
                use_paths(item, prefix.clone(), module, other_crates, out);
            }
        }
    }
}

/// The module path `segments` names when written inside `module`.
fn resolve(
    segments: &[String],
    module: &[String],
    other_crates: &BTreeSet<String>,
) -> Option<ModuleRef> {
    let first = segments.first()?;
    let (krate, path, relative) = match first.as_str() {
        "crate" => (None, segments[1..].to_vec(), false),
        "self" => (None, [module, &segments[1..]].concat(), false),
        "super" => {
            let supers = segments.iter().take_while(|s| *s == "super").count();
            let base = module.len().checked_sub(supers)?;
            (None, [&module[..base], &segments[supers..]].concat(), false)
        }
        name if other_crates.contains(name) => {
            (Some(name.to_owned()), segments[1..].to_vec(), false)
        }
        _ => (None, [module, segments].concat(), true),
    };
    Some(ModuleRef {
        krate,
        path,
        relative,
    })
}

/// The paths among one level of tokens: `crate::`, `self::`, `super::`, other-crate and relative
/// ones (a relative path names a child module only when its first name is one).
fn path_refs(
    tokens: &[TokenTree],
    module: &[String],
    other_crates: &BTreeSet<String>,
    out: &mut BTreeSet<ModuleRef>,
) {
    let mut i = 0;
    while i < tokens.len() {
        let Some(first) = ident_text(tokens.get(i)) else {
            i += 1;
            continue;
        };
        let after_separator = i >= 2 && is_path_separator(tokens, i - 2);
        if after_separator || !is_path_separator(tokens, i + 1) {
            i += 1;
            continue;
        }
        let mut segments = vec![first];
        let mut j = i + 1;
        while is_path_separator(tokens, j) {
            match ident_text(tokens.get(j + 2)) {
                Some(next) => {
                    segments.push(next);
                    j += 3;
                }
                None => break,
            }
        }
        out.extend(resolve(&segments, module, other_crates));
        i = j.max(i + 1);
    }
}

/// Whether the code of `file` names the module path `first::second`: as part of a path
/// (`a::first::second::f`) or through a use group (`use a::first::{second, ...}`). Doc comments and
/// string literals are not code and do not count.
pub fn names_module_path(file: &syn::File, first: &str, second: &str) -> bool {
    let mut found = false;
    each_level(file.to_token_stream(), &mut |tokens| {
        for i in 0..tokens.len() {
            if found
                || ident_text(tokens.get(i)).as_deref() != Some(first)
                || !is_path_separator(tokens, i + 1)
            {
                continue;
            }
            found = match tokens.get(i + 3) {
                Some(TokenTree::Group(group)) => {
                    let inner: Vec<TokenTree> = group.stream().into_iter().collect();
                    inner.iter().enumerate().any(|(j, token)| {
                        ident_text(Some(token)).as_deref() == Some(second)
                            && (j == 0
                                || matches!(&inner[j - 1], TokenTree::Punct(p) if p.as_char() == ','))
                    })
                }
                token => ident_text(token).as_deref() == Some(second),
            };
        }
    });
    found
}

/// One function of the catalogue: its ID and the modules that define it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogueEntry {
    pub id: String,
    pub constant: String,
    pub modules: Vec<String>,
}

/// The function catalogue of OxFunc's `surface_dispatch.rs`: every `pub const FUNC_ID_<X>: &str =
/// "<id>"`, with the modules of `crate::functions` that its arm of `arg_preparation_profile` names
/// (directly as `crate::functions::<module>::...`, or through a `<X>_META` imported from one).
/// A constant without such an arm takes the module from which `eval_<name>_surface` is imported;
/// otherwise its module list stays empty.
pub fn catalogue(file: &syn::File) -> Vec<CatalogueEntry> {
    let mut imports: BTreeMap<String, String> = BTreeMap::new();
    let mut constants: BTreeMap<String, String> = BTreeMap::new();
    let mut profile = None;
    for item in &file.items {
        match item {
            Item::Use(declaration) => functions_imports(&declaration.tree, &[], &mut imports),
            Item::Const(constant) if constant.ident.to_string().starts_with("FUNC_ID_") => {
                let tokens: Vec<TokenTree> = constant.expr.to_token_stream().into_iter().collect();
                if let [TokenTree::Literal(literal)] = tokens.as_slice()
                    && let Some(value) = crate::rust_source::string_literal_value(literal)
                {
                    constants.insert(constant.ident.to_string(), value);
                }
            }
            Item::Fn(function) if function.sig.ident == "arg_preparation_profile" => {
                profile = Some(function)
            }
            _ => {}
        }
    }
    let mut modules: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let arms = profile
        .and_then(|function| function.block.stmts.iter().find_map(match_of))
        .map(|found| found.arms.as_slice())
        .unwrap_or_default();
    for arm in arms {
        let pattern: Vec<String> = arm
            .pat
            .to_token_stream()
            .into_iter()
            .filter_map(|token| ident_text(Some(&token)))
            .filter(|ident| ident.starts_with("FUNC_ID_"))
            .collect();
        let mut named = BTreeSet::new();
        each_level(arm.body.to_token_stream(), &mut |tokens| {
            for i in 0..tokens.len() {
                let Some(ident) = ident_text(tokens.get(i)) else {
                    continue;
                };
                let after_separator = i >= 2 && is_path_separator(tokens, i - 2);
                if ident == "crate"
                    && is_path_separator(tokens, i + 1)
                    && ident_text(tokens.get(i + 3)).as_deref() == Some("functions")
                    && is_path_separator(tokens, i + 4)
                    && let Some(module) = ident_text(tokens.get(i + 6))
                {
                    named.insert(module);
                } else if ident.ends_with("_META")
                    && !after_separator
                    && let Some(module) = imports.get(&ident)
                {
                    named.insert(module.clone());
                }
            }
        });
        for constant in pattern {
            modules
                .entry(constant)
                .or_default()
                .extend(named.iter().cloned());
        }
    }
    let mut entries: Vec<CatalogueEntry> = constants
        .into_iter()
        .map(|(constant, id)| {
            let mut found: Vec<String> = modules
                .get(&constant)
                .map(|set| set.iter().cloned().collect())
                .unwrap_or_default();
            if found.is_empty() {
                let name = id.strip_prefix("FUNC.").unwrap_or(&id);
                let surface = format!(
                    "eval_{}_surface",
                    name.to_ascii_lowercase().replace('.', "_")
                );
                found.extend(imports.get(&surface).cloned());
            }
            CatalogueEntry {
                id,
                constant,
                modules: found,
            }
        })
        .collect();
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    entries
}

fn match_of(stmt: &Stmt) -> Option<&syn::ExprMatch> {
    match stmt {
        Stmt::Expr(Expr::Match(found), _) => Some(found),
        _ => None,
    }
}

/// The names imported from `crate::functions::<module>`, each mapped to its module.
fn functions_imports(tree: &UseTree, prefix: &[String], out: &mut BTreeMap<String, String>) {
    match tree {
        UseTree::Path(path) => {
            let mut next = prefix.to_vec();
            next.push(path.ident.to_string());
            functions_imports(&path.tree, &next, out);
        }
        UseTree::Group(group) => {
            for item in &group.items {
                functions_imports(item, prefix, out);
            }
        }
        UseTree::Name(name) => {
            if let [krate, functions, module, ..] = prefix
                && krate == "crate"
                && functions == "functions"
            {
                out.insert(name.ident.to_string(), module.clone());
            }
        }
        UseTree::Rename(rename) => {
            if let [krate, functions, module, ..] = prefix
                && krate == "crate"
                && functions == "functions"
            {
                out.insert(rename.rename.to_string(), module.clone());
            }
        }
        UseTree::Glob(_) => {}
    }
}

/// The catalogue IDs `text` names: a `FUNC_ID_<X>` constant as a whole word, or `FUNC.<NAME>`
/// followed by a character that cannot continue a name.
pub fn mentioned_ids(
    text: &str,
    constants: &BTreeMap<String, String>,
    ids: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let bytes = text.as_bytes();
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut from = 0;
    while let Some(at) = text[from..].find("FUNC_ID_") {
        let start = from + at;
        let mut end = start + "FUNC_ID_".len();
        while end < bytes.len() && word(bytes[end]) {
            end += 1;
        }
        let whole = start == 0 || !word(bytes[start - 1]);
        if whole && let Some(id) = constants.get(&text[start..end]) {
            found.insert(id.clone());
        }
        from = end;
    }
    let mut from = 0;
    while let Some(at) = text[from..].find("FUNC.") {
        let start = from + at;
        let mut end = start + "FUNC.".len();
        while end < bytes.len() && (word(bytes[end]) || bytes[end] == b'.') {
            end += 1;
        }
        let candidate = text[start..end].trim_end_matches('.');
        let whole = start == 0 || !word(bytes[start - 1]);
        if whole && ids.contains(candidate) {
            found.insert(candidate.to_owned());
        }
        from = end;
    }
    found
}

/// The include macros (`include!`, `include_str!`, `include_bytes!`) of a source text.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Includes {
    /// The literal paths, as written.
    pub literal: Vec<String>,
    /// The macro calls whose argument is not a string literal (`include_str!(concat!(...))`),
    /// which this reader does not resolve, shortened to their first line.
    pub non_literal: Vec<String>,
}

/// The include macros of `text`, found as text (comments included); a macro name counts only
/// where no identifier character precedes it.
pub fn include_paths(text: &str) -> Includes {
    let mut found = Includes::default();
    let bytes = text.as_bytes();
    for marker in ["include!(", "include_str!(", "include_bytes!("] {
        let mut from = 0;
        while let Some(at) = text[from..].find(marker) {
            let start = from + at;
            from = start + marker.len();
            if start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
                continue;
            }
            let rest = text[from..].trim_start();
            let literal = rest.strip_prefix('"').and_then(|literal| {
                literal
                    .find('"')
                    .map(|end| literal[..end].replace("\\\\", "/"))
            });
            match literal {
                Some(path) => found.literal.push(path),
                None => found.non_literal.push(
                    text[start..]
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .chars()
                        .take(80)
                        .collect(),
                ),
            }
        }
    }
    found
}

/// `relative` joined to the directory `dir` of a repository path, with `.` and `..` resolved, or
/// `None` when it climbs out of the repository.
pub fn join(dir: &str, relative: &str) -> Option<String> {
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(source: &str) -> syn::File {
        parse(source).unwrap()
    }

    #[test]
    fn exported_names_skip_private_and_test_items() {
        let source = r#"
            pub fn visible() {}
            fn hidden() {}
            pub(crate) struct Internal;
            pub struct Shown;
            pub use crate::inner::{Again, Other as Renamed};
            #[macro_export]
            macro_rules! shout { () => {} }
            pub mod nested { pub enum Kind { A } fn quiet() {} }
            #[cfg(test)]
            pub fn only_in_tests() {}
        "#;
        assert_eq!(
            exported(&file(source)),
            [
                "Again",
                "Renamed",
                "Shown",
                "nested::Kind",
                "shout!",
                "visible"
            ]
        );
    }

    #[test]
    fn module_references_resolve_crate_self_super_and_other_crates() {
        let source = r#"
            use crate::coercion::CoercionError;
            use super::adapters::{self, prepare};
            use other_crate::value::Cell;
            use std::fmt;
            mod child;
            fn f() -> u8 { crate::value::make(); self::local::g(); decimal::parse(); 0 }
            #[cfg(test)]
            mod tests { use crate::only_tests::X; }
        "#;
        let module = vec!["functions".to_owned(), "round_fn".to_owned()];
        let crates = BTreeSet::from(["other_crate".to_owned()]);
        let refs: Vec<(Option<String>, String, bool)> =
            module_refs(&file(source), &module, &crates)
                .into_iter()
                .map(|r| (r.krate, r.path.join("::"), r.relative))
                .collect();
        assert_eq!(
            refs,
            [
                (None, "coercion::CoercionError".to_owned(), false),
                (None, "functions::adapters".to_owned(), false),
                (None, "functions::adapters::prepare".to_owned(), false),
                (None, "functions::round_fn::decimal::parse".to_owned(), true),
                (None, "functions::round_fn::local::g".to_owned(), false),
                (None, "functions::round_fn::std::fmt".to_owned(), true),
                (None, "value::make".to_owned(), false),
                (
                    Some("other_crate".to_owned()),
                    "value::Cell".to_owned(),
                    false
                ),
            ]
        );
    }

    #[test]
    fn module_declarations_are_read_with_paths_inline_modules_and_tests() {
        let source = r#"
            mod plain;
            #[path = "coercion_decimal.rs"]
            mod decimal;
            pub(crate) mod inline {
                mod nested;
                #[path = "elsewhere.rs"]
                mod moved;
            }
            #[cfg(test)]
            mod tests;
            #[cfg_attr(windows, path = "win.rs")]
            mod platform;
            mod body { fn f() {} }
        "#;
        let found: Vec<String> = declared_modules(&parse_full(source).unwrap())
            .into_iter()
            .map(|d| {
                let inline: Vec<String> = d.inline.iter().map(|m| m.name.clone()).collect();
                format!(
                    "{}{} path={:?} test_only={} cfg_attr_path={}",
                    inline.iter().map(|m| format!("{m}::")).collect::<String>(),
                    d.name,
                    d.path,
                    d.test_only,
                    d.cfg_attr_path
                )
            })
            .collect();
        assert_eq!(
            found,
            [
                "plain path=None test_only=false cfg_attr_path=false",
                "decimal path=Some(\"coercion_decimal.rs\") test_only=false cfg_attr_path=false",
                "inline::nested path=None test_only=false cfg_attr_path=false",
                "inline::moved path=Some(\"elsewhere.rs\") test_only=false cfg_attr_path=false",
                "tests path=None test_only=true cfg_attr_path=false",
                "platform path=None test_only=false cfg_attr_path=true",
            ]
        );
    }

    #[test]
    fn the_catalogue_names_modules_through_paths_and_imports() {
        let source = r#"
            use crate::functions::abs::abs_kernel;
            use crate::functions::callable_helpers::{BYCOL_META, MAP_META as MAPPED};
            use crate::functions::criteria_family::{eval_countif_surface};
            pub const FUNC_ID_ABS: &str = "FUNC.ABS";
            pub const FUNC_ID_BYCOL: &str = "FUNC.BYCOL";
            pub const FUNC_ID_CEILING_MATH: &str = "FUNC.CEILING.MATH";
            pub const FUNC_ID_COUNTIF: &str = "FUNC.COUNTIF";
            pub const FUNC_ID_LOST: &str = "FUNC.LOST";
            pub fn arg_preparation_profile(function_id: &str) -> Option<u8> {
                match function_id {
                    FUNC_ID_ABS => Some(crate::functions::abs::ABS_META.arg_preparation_profile),
                    FUNC_ID_BYCOL => Some(BYCOL_META.arg_preparation_profile),
                    FUNC_ID_CEILING_MATH => {
                        Some(crate::functions::ceiling_floor_family::CEILING_MATH_META.p)
                    }
                    _ => None,
                }
            }
        "#;
        let found: Vec<(String, Vec<String>)> = catalogue(&file(source))
            .into_iter()
            .map(|entry| (entry.id, entry.modules))
            .collect();
        let expected: Vec<(String, Vec<String>)> = [
            ("FUNC.ABS", vec!["abs"]),
            ("FUNC.BYCOL", vec!["callable_helpers"]),
            ("FUNC.CEILING.MATH", vec!["ceiling_floor_family"]),
            ("FUNC.COUNTIF", vec!["criteria_family"]),
            ("FUNC.LOST", vec![]),
        ]
        .into_iter()
        .map(|(id, modules)| {
            (
                id.to_owned(),
                modules.into_iter().map(str::to_owned).collect(),
            )
        })
        .collect();
        assert_eq!(found, expected);
    }

    #[test]
    fn mentions_includes_and_joins_are_read() {
        let constants = BTreeMap::from([("FUNC_ID_ROUND".to_owned(), "FUNC.ROUND".to_owned())]);
        let ids = BTreeSet::from(["FUNC.ROUND".to_owned(), "FUNC.CEILING.MATH".to_owned()]);
        let text =
            r#"call(FUNC_ID_ROUND); call("FUNC.CEILING.MATH"); MY_FUNC_ID_ROUND; "FUNC.ROUNDX""#;
        assert_eq!(
            mentioned_ids(text, &constants, &ids)
                .into_iter()
                .collect::<Vec<_>>(),
            ["FUNC.CEILING.MATH", "FUNC.ROUND"]
        );
        let text = r#"replay(include_str!("../../docs/a.json")); include_bytes!( "b.bin"); include_str!(concat!("x")); include!("gen.rs"); my_include!("no.rs")"#;
        let found = include_paths(text);
        assert_eq!(found.literal, ["gen.rs", "../../docs/a.json", "b.bin"]);
        assert_eq!(found.non_literal.len(), 1);
        assert!(found.non_literal[0].starts_with("include_str!(concat!"));
        assert_eq!(
            join("crates/core/tests", "../../../docs/a.json").as_deref(),
            Some("docs/a.json")
        );
        assert_eq!(join("a", "../../b"), None);
    }
}
