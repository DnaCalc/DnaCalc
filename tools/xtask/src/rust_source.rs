//! Parsed Rust sources: the module tree of a crate, with test-only items removed, and helpers for
//! scanning the tokens that remain; and the test functions of a crate's test targets with where
//! each runs (natively and in the browsers, or natively only and why).

use proc_macro2::{Spacing, TokenStream, TokenTree};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use syn::punctuated::Punctuated;
use syn::{Attribute, ImplItem, Item, Meta, Token, TraitItem};

/// One file of a crate's non-test sources.
pub struct SourceFile {
    /// The path relative to the workspace root, with forward slashes.
    pub rel: String,
    /// The file's syntax with every test-only item removed.
    pub syntax: syn::File,
}

/// The non-test sources of one crate: every file reached from `roots` through `mod` declarations
/// that are not test-only, each parsed and stripped of its test-only items. Plain `//` comments
/// are not part of the syntax; doc comments are, as `doc` attributes.
pub fn crate_sources(workspace_root: &Path, roots: &[PathBuf]) -> Result<Vec<SourceFile>, String> {
    Ok(module_tree(workspace_root, roots, false)?
        .into_iter()
        .map(|(file, _)| file)
        .collect())
}

/// Every file reached from `roots` through `mod` declarations, parsed, with the scope its
/// declarations and its own inner attributes give it. Without `tests`, test-only modules are not
/// followed and each file is stripped of its test-only items.
fn module_tree(
    workspace_root: &Path,
    roots: &[PathBuf],
    tests: bool,
) -> Result<Vec<(SourceFile, Scope)>, String> {
    let mut seen = BTreeSet::new();
    let mut files = Vec::new();
    let mut pending: Vec<(PathBuf, PathBuf, Scope)> = roots
        .iter()
        .map(|root| {
            let dir = root.parent().map(Path::to_path_buf).unwrap_or_default();
            (root.clone(), dir, Scope::default())
        })
        .collect();
    while let Some((path, dir, scope)) = pending.pop() {
        if !seen.insert(path.clone()) {
            continue;
        }
        let rel = path
            .strip_prefix(workspace_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let source = fs::read_to_string(&path).map_err(|e| format!("cannot read {rel}: {e}"))?;
        let mut syntax = syn::parse_file(&source)
            .map_err(|e| format!("cannot parse {rel} (line {}): {e}", e.span().start().line))?;
        if !tests {
            strip_tests(&mut syntax.items);
        }
        let scope = scope.enter(None, &syntax.attrs);
        let found = ModuleFiles {
            file: &path,
            rel: &rel,
        };
        found.collect(&syntax.items, &dir, false, &scope, &mut pending)?;
        files.push((SourceFile { rel, syntax }, scope));
    }
    files.sort_by(|a, b| a.0.rel.cmp(&b.0.rel));
    Ok(files)
}

struct ModuleFiles<'a> {
    file: &'a Path,
    rel: &'a str,
}

impl ModuleFiles<'_> {
    /// Queues the file of every out-of-line `mod name;` among `items`, with the scope inside its
    /// declaration. `dir` is where the child modules of this scope live; `inline` says whether
    /// `items` sit inside an inline module, whose scope is `scope`.
    fn collect(
        &self,
        items: &[Item],
        dir: &Path,
        inline: bool,
        scope: &Scope,
        pending: &mut Vec<(PathBuf, PathBuf, Scope)>,
    ) -> Result<(), String> {
        for item in items {
            let Item::Mod(declared) = item else {
                continue;
            };
            let name = declared.ident.to_string();
            let name = name.trim_start_matches("r#").to_owned();
            match &declared.content {
                Some((_, inner)) => self.collect(
                    inner,
                    &dir.join(&name),
                    true,
                    &scope.enter(Some(&name), &declared.attrs),
                    pending,
                )?,
                None => {
                    let (path, child_dir) = match path_attribute(&declared.attrs) {
                        Some(relative) => {
                            // Outside inline modules a path attribute is relative to the
                            // directory of the declaring file; inside them, to `dir`.
                            let base = if inline {
                                dir.to_path_buf()
                            } else {
                                self.file
                                    .parent()
                                    .map(Path::to_path_buf)
                                    .unwrap_or_default()
                            };
                            let path = base.join(relative);
                            let child_dir =
                                path.parent().map(Path::to_path_buf).unwrap_or_default();
                            (path, child_dir)
                        }
                        None => {
                            let flat = dir.join(format!("{name}.rs"));
                            let nested = dir.join(&name).join("mod.rs");
                            if flat.is_file() {
                                (flat, dir.join(&name))
                            } else if nested.is_file() {
                                (nested, dir.join(&name))
                            } else {
                                return Err(format!(
                                    "{} declares `mod {name};` but neither {} nor {} exists",
                                    self.rel,
                                    flat.display(),
                                    nested.display()
                                ));
                            }
                        }
                    };
                    if !path.is_file() {
                        return Err(format!(
                            "{} declares `mod {name};` at {}, which does not exist",
                            self.rel,
                            path.display()
                        ));
                    }
                    pending.push((path, child_dir, scope.enter(Some(&name), &declared.attrs)));
                }
            }
        }
        Ok(())
    }
}

/// The value of a `#[path = "..."]` attribute among `attrs`.
pub fn path_attribute(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| match &attr.meta {
        Meta::NameValue(pair) if pair.path.is_ident("path") => match &pair.value {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(value),
                ..
            }) => Some(value.value()),
            _ => None,
        },
        _ => None,
    })
}

/// Whether a `cfg` predicate can only hold in a test build: `test`, an `all(...)` with a test-only
/// member, or an `any(...)` whose members are all test-only.
fn cfg_is_test_only(predicate: &Meta) -> bool {
    match predicate {
        Meta::Path(path) => path.is_ident("test"),
        Meta::List(list) => {
            let Ok(members) = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            else {
                return false;
            };
            if list.path.is_ident("all") {
                members.iter().any(cfg_is_test_only)
            } else if list.path.is_ident("any") {
                !members.is_empty() && members.iter().all(cfg_is_test_only)
            } else {
                false
            }
        }
        Meta::NameValue(_) => false,
    }
}

/// Whether the attributes make an item test-only: `#[test]`, or a `#[cfg(...)]` that only holds
/// in test builds.
pub fn is_test_only(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if attr.path().is_ident("test") {
            return true;
        }
        match &attr.meta {
            Meta::List(list) if list.path.is_ident("cfg") => list
                .parse_args::<Meta>()
                .is_ok_and(|predicate| cfg_is_test_only(&predicate)),
            _ => false,
        }
    })
}

fn item_attrs(item: &Item) -> Option<&[Attribute]> {
    Some(match item {
        Item::Const(i) => &i.attrs,
        Item::Enum(i) => &i.attrs,
        Item::ExternCrate(i) => &i.attrs,
        Item::Fn(i) => &i.attrs,
        Item::ForeignMod(i) => &i.attrs,
        Item::Impl(i) => &i.attrs,
        Item::Macro(i) => &i.attrs,
        Item::Mod(i) => &i.attrs,
        Item::Static(i) => &i.attrs,
        Item::Struct(i) => &i.attrs,
        Item::Trait(i) => &i.attrs,
        Item::TraitAlias(i) => &i.attrs,
        Item::Type(i) => &i.attrs,
        Item::Union(i) => &i.attrs,
        Item::Use(i) => &i.attrs,
        _ => return None,
    })
}

fn impl_item_attrs(item: &ImplItem) -> Option<&[Attribute]> {
    Some(match item {
        ImplItem::Const(i) => &i.attrs,
        ImplItem::Fn(i) => &i.attrs,
        ImplItem::Type(i) => &i.attrs,
        ImplItem::Macro(i) => &i.attrs,
        _ => return None,
    })
}

fn trait_item_attrs(item: &TraitItem) -> Option<&[Attribute]> {
    Some(match item {
        TraitItem::Const(i) => &i.attrs,
        TraitItem::Fn(i) => &i.attrs,
        TraitItem::Type(i) => &i.attrs,
        TraitItem::Macro(i) => &i.attrs,
        _ => return None,
    })
}

/// Removes every test-only item, at any module depth, and every test-only item of an impl or a
/// trait.
pub fn strip_tests(items: &mut Vec<Item>) {
    items.retain(|item| !item_attrs(item).is_some_and(is_test_only));
    for item in items.iter_mut() {
        match item {
            Item::Mod(module) => {
                if let Some((_, inner)) = &mut module.content {
                    strip_tests(inner);
                }
            }
            Item::Impl(block) => block
                .items
                .retain(|inner| !impl_item_attrs(inner).is_some_and(is_test_only)),
            Item::Trait(block) => block
                .items
                .retain(|inner| !trait_item_attrs(inner).is_some_and(is_test_only)),
            _ => {}
        }
    }
}

/// The doc line that marks a test, or every test of a module or file, as native-only; the rest
/// of the line is the reason.
pub const NATIVE_ONLY_MARK: &str = "Native-only:";

/// Where a test function runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TestRoute {
    /// Natively and, through `wasm_bindgen_test`, in the browsers.
    Browser,
    /// Natively only, for the reason a `Native-only:` doc line gives on the function, an
    /// enclosing module or the file.
    NativeOnly(String),
    /// Natively only, with no reason given.
    Unmarked,
}

/// One test function of a package's sources.
#[derive(Debug)]
pub struct TestFunction {
    /// The file, relative to the workspace root, with forward slashes.
    pub rel: String,
    /// The function's module path from the crate root and its name.
    pub name: String,
    pub line: usize,
    pub route: TestRoute,
}

/// What the enclosing modules and files say about the items in them.
#[derive(Clone, Debug, Default)]
pub struct Scope {
    /// The module path from the crate root.
    path: Vec<String>,
    /// The reason of the innermost `Native-only:` doc line on an enclosing module or file.
    reason: Option<String>,
    /// An enclosing `cfg` that does not hold on wasm32.
    excluded: bool,
}

impl Scope {
    /// The scope inside an item named `name` (a module) with attributes `attrs`.
    fn enter(&self, name: Option<&str>, attrs: &[Attribute]) -> Scope {
        let mut scope = self.clone();
        scope.path.extend(name.map(str::to_owned));
        if let Some(reason) = native_only_reason(attrs) {
            scope.reason = Some(reason);
        }
        scope.excluded |= excludes_wasm32(attrs);
        scope
    }
}

/// Whether a `cfg` predicate holds in a test build for wasm32-unknown-unknown: `Some` when the
/// target decides it, `None` when it does not (a feature or any other configuration).
fn holds_on_wasm32(predicate: &Meta) -> Option<bool> {
    match predicate {
        Meta::Path(path) if path.is_ident("windows") || path.is_ident("unix") => Some(false),
        Meta::Path(path) if path.is_ident("test") => Some(true),
        Meta::Path(_) => None,
        Meta::NameValue(pair) => {
            let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(value),
                ..
            }) = &pair.value
            else {
                return None;
            };
            let value = value.value();
            let key = pair.path.get_ident()?.to_string();
            match key.as_str() {
                "target_arch" => Some(value == "wasm32"),
                "target_os" => Some(value == "unknown"),
                "target_env" => Some(value.is_empty()),
                "target_family" => Some(value == "wasm"),
                "target_pointer_width" => Some(value == "32"),
                _ => None,
            }
        }
        Meta::List(list) => {
            let members = list
                .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                .ok()?;
            let values: Vec<Option<bool>> = members.iter().map(holds_on_wasm32).collect();
            if list.path.is_ident("not") {
                match values[..] {
                    [value] => value.map(|holds| !holds),
                    _ => None,
                }
            } else if list.path.is_ident("all") {
                if values.contains(&Some(false)) {
                    Some(false)
                } else if values.iter().all(|value| *value == Some(true)) {
                    Some(true)
                } else {
                    None
                }
            } else if list.path.is_ident("any") {
                if values.contains(&Some(true)) {
                    Some(true)
                } else if values.iter().all(|value| *value == Some(false)) {
                    Some(false)
                } else {
                    None
                }
            } else {
                None
            }
        }
    }
}

/// Whether `attrs` hold a `cfg` that is false on wasm32, so the item is not compiled there.
fn excludes_wasm32(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| match &attr.meta {
        Meta::List(list) if list.path.is_ident("cfg") => list
            .parse_args::<Meta>()
            .is_ok_and(|predicate| holds_on_wasm32(&predicate) == Some(false)),
        _ => false,
    })
}

/// The reason of the first `Native-only:` doc line among `attrs`.
fn native_only_reason(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| match &attr.meta {
        Meta::NameValue(pair) if pair.path.is_ident("doc") => match &pair.value {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(text),
                ..
            }) => text
                .value()
                .trim()
                .strip_prefix(NATIVE_ONLY_MARK)
                .map(|reason| reason.trim().to_owned()),
            _ => None,
        },
        _ => None,
    })
}

/// Whether `attrs` carry libtest's `test` attribute and wasm-bindgen-test's, directly or through
/// `cfg_attr`.
fn test_attributes(attrs: &[Attribute]) -> (bool, bool) {
    let kind = |path: &syn::Path| {
        (
            path.is_ident("test"),
            path.segments
                .last()
                .is_some_and(|segment| segment.ident == "wasm_bindgen_test"),
        )
    };
    let (mut native, mut browser) = (false, false);
    let mut note = |(n, b): (bool, bool)| {
        native |= n;
        browser |= b;
    };
    for attr in attrs {
        note(kind(attr.path()));
        if let Meta::List(list) = &attr.meta
            && list.path.is_ident("cfg_attr")
            && let Ok(args) = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        {
            for meta in args.iter().skip(1) {
                note(kind(meta.path()));
            }
        }
    }
    (native, browser)
}

/// The test functions among `items`, inline modules included, in `scope`.
fn collect_tests(rel: &str, items: &[Item], scope: &Scope, out: &mut Vec<TestFunction>) {
    for item in items {
        match item {
            Item::Mod(module) => {
                if let Some((_, inner)) = &module.content {
                    let name = module.ident.to_string();
                    collect_tests(rel, inner, &scope.enter(Some(&name), &module.attrs), out);
                }
            }
            Item::Fn(function) => {
                let (native, browser) = test_attributes(&function.attrs);
                if !native && !browser {
                    continue;
                }
                let own = scope.enter(None, &function.attrs);
                let route = if browser && !own.excluded {
                    TestRoute::Browser
                } else if let Some(reason) = own.reason {
                    TestRoute::NativeOnly(reason)
                } else {
                    TestRoute::Unmarked
                };
                let mut name = scope.path.clone();
                name.push(function.sig.ident.to_string());
                out.push(TestFunction {
                    rel: rel.to_owned(),
                    name: name.join("::"),
                    line: function.sig.ident.span().start().line,
                    route,
                });
            }
            _ => {}
        }
    }
}

/// Every test function of the files reached from `roots` through `mod` declarations, test-only
/// modules included, with where it runs: in the browsers when it carries wasm-bindgen-test's
/// attribute and no `cfg` around it (on the function, a module or the file) is false on wasm32;
/// otherwise natively only, with the reason of the nearest `Native-only:` doc line, or unmarked.
/// A file reached from several roots is read once.
pub fn test_functions(
    workspace_root: &Path,
    roots: &[PathBuf],
) -> Result<Vec<TestFunction>, String> {
    let mut tests = Vec::new();
    for (file, scope) in module_tree(workspace_root, roots, true)? {
        collect_tests(&file.rel, &file.syntax.items, &scope, &mut tests);
    }
    Ok(tests)
}

/// Calls `visit` with the tokens of every nesting level of `stream`: the top level, then the
/// contents of each delimited group, recursively.
pub fn each_level(stream: TokenStream, visit: &mut dyn FnMut(&[TokenTree])) {
    let tokens: Vec<TokenTree> = stream.into_iter().collect();
    visit(&tokens);
    for token in &tokens {
        if let TokenTree::Group(group) = token {
            each_level(group.stream(), visit);
        }
    }
}

/// Whether `tokens[i]` and `tokens[i + 1]` form a `::` path separator.
pub fn is_path_separator(tokens: &[TokenTree], i: usize) -> bool {
    matches!(tokens.get(i), Some(TokenTree::Punct(p)) if p.as_char() == ':' && p.spacing() == Spacing::Joint)
        && matches!(tokens.get(i + 1), Some(TokenTree::Punct(p)) if p.as_char() == ':')
}

/// The identifier `token` holds, without a raw `r#` prefix.
pub fn ident_text(token: Option<&TokenTree>) -> Option<String> {
    match token {
        Some(TokenTree::Ident(ident)) => {
            Some(ident.to_string().trim_start_matches("r#").to_owned())
        }
        _ => None,
    }
}

/// The 1-based line on which `token` starts.
pub fn line_of(token: &TokenTree) -> usize {
    token.span().start().line
}

/// The value of a string literal token (plain, raw, byte or C string), or `None` for any other
/// literal. Escapes other than `\n`, `\r`, `\t`, `\0`, `\\`, `\"`, `\'` and line continuations are
/// kept as written.
pub fn string_literal_value(literal: &proc_macro2::Literal) -> Option<String> {
    let written = literal.to_string();
    let body = written
        .strip_prefix('b')
        .or_else(|| written.strip_prefix('c'))
        .unwrap_or(&written);
    if let Some(raw) = body.strip_prefix('r') {
        let hashes = raw.len() - raw.trim_start_matches('#').len();
        let fence = "#".repeat(hashes);
        let inner = raw[hashes..]
            .strip_prefix('"')?
            .strip_suffix(fence.as_str())?
            .strip_suffix('"')?;
        return Some(inner.to_owned());
    }
    let inner = body.strip_prefix('"')?;
    let inner = &inner[..inner.rfind('"')?];
    let mut value = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            value.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => value.push('\n'),
            Some('r') => value.push('\r'),
            Some('t') => value.push('\t'),
            Some('0') => value.push('\0'),
            Some(quoted @ ('\\' | '"' | '\'')) => value.push(quoted),
            Some('\n') => {
                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
            }
            Some(other) => {
                value.push('\\');
                value.push(other);
            }
            None => value.push('\\'),
        }
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;

    fn fixture_dir(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    #[test]
    fn module_tree_skips_test_only_modules_and_items() {
        let root = fixture_dir("module_tree");
        let files = crate_sources(&root, &[root.join("src").join("lib.rs")]).unwrap();
        let shown: Vec<&str> = files.iter().map(|file| file.rel.as_str()).collect();
        assert_eq!(
            shown,
            [
                "src/lib.rs",
                "src/nested/inner.rs",
                "src/nested/mod.rs",
                "src/renamed.rs",
            ]
        );
        let text = files[0].syntax.to_token_stream().to_string();
        assert!(text.contains("kept_in_release"), "{text}");
        for removed in [
            "only_in_tests",
            "test_helper",
            "all_with_test",
            "any_of_tests",
        ] {
            assert!(!text.contains(removed), "{removed} survived: {text}");
        }
        assert!(text.contains("test_or_feature"), "{text}");
    }

    /// Each test function's route from its own attributes and docs and those around it.
    #[test]
    fn test_functions_carry_their_route() {
        let source = r#"
            //! Native-only: the file's reason.
            #[cfg(test)]
            mod tests {
                #[cfg_attr(not(target_arch = "wasm32"), test)]
                #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
                fn paired() {}

                #[wasm_bindgen_test]
                fn browser_only_attribute() {}

                /// Native-only: its own reason.
                #[test]
                fn own_reason() {}

                #[test]
                fn file_reason() {}

                #[cfg(target_arch = "x86_64")]
                #[cfg_attr(not(target_arch = "wasm32"), test)]
                #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
                fn paired_but_not_compiled_for_wasm32() {}

                fn helper() {}
            }

            /// Native-only: the module's reason.
            #[cfg(all(test, not(target_arch = "wasm32")))]
            mod native {
                #[cfg_attr(not(target_arch = "wasm32"), test)]
                #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
                fn module_reason() {}
            }
        "#;
        let file = syn::parse_file(source).unwrap();
        let mut tests = Vec::new();
        let scope = Scope::default().enter(None, &file.attrs);
        collect_tests("src/lib.rs", &file.items, &scope, &mut tests);
        let routes: Vec<(&str, TestRoute)> = tests
            .iter()
            .map(|test| (test.name.as_str(), test.route.clone()))
            .collect();
        let native = |reason: &str| TestRoute::NativeOnly(reason.to_owned());
        assert_eq!(
            routes,
            [
                ("tests::paired", TestRoute::Browser),
                ("tests::browser_only_attribute", TestRoute::Browser),
                ("tests::own_reason", native("its own reason.")),
                ("tests::file_reason", native("the file's reason.")),
                (
                    "tests::paired_but_not_compiled_for_wasm32",
                    native("the file's reason.")
                ),
                ("native::module_reason", native("the module's reason.")),
            ]
        );

        let unmarked = syn::parse_file("#[test] fn bare() {}").unwrap();
        let mut tests = Vec::new();
        collect_tests("tests/t.rs", &unmarked.items, &Scope::default(), &mut tests);
        assert_eq!(tests[0].route, TestRoute::Unmarked);
    }

    #[test]
    fn cfg_predicates_are_decided_for_wasm32() {
        let holds = |text: &str| holds_on_wasm32(&syn::parse_str::<Meta>(text).unwrap());
        assert_eq!(holds("test"), Some(true));
        assert_eq!(holds(r#"target_arch = "wasm32""#), Some(true));
        assert_eq!(holds(r#"not(target_arch = "wasm32")"#), Some(false));
        assert_eq!(holds(r#"target_arch = "x86_64""#), Some(false));
        assert_eq!(holds(r#"target_env = "msvc""#), Some(false));
        assert_eq!(holds("windows"), Some(false));
        assert_eq!(
            holds(r#"all(test, not(target_arch = "wasm32"))"#),
            Some(false)
        );
        assert_eq!(holds(r#"all(windows, feature = "x")"#), Some(false));
        assert_eq!(holds(r#"any(test, windows)"#), Some(true));
        assert_eq!(holds(r#"feature = "x""#), None);
        assert_eq!(holds(r#"not(feature = "x")"#), None);
    }

    #[test]
    fn string_literals_decode() {
        let lit = |s: &str| s.parse::<proc_macro2::Literal>().unwrap();
        assert_eq!(string_literal_value(&lit(r#""a\nb""#)).unwrap(), "a\nb");
        assert_eq!(
            string_literal_value(&lit(r###"r#"x "y""#"###)).unwrap(),
            "x \"y\""
        );
        assert_eq!(string_literal_value(&lit(r#"b"tree""#)).unwrap(), "tree");
        assert_eq!(string_literal_value(&lit("12")), None);
        assert_eq!(string_literal_value(&lit("'c'")), None);
    }
}
