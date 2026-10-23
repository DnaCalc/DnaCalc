//! Parsed non-test Rust sources: the module tree of a crate, with test-only items removed, and
//! helpers for scanning the tokens that remain.

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
    let mut seen = BTreeSet::new();
    let mut files = Vec::new();
    let mut pending: Vec<(PathBuf, PathBuf)> = roots
        .iter()
        .map(|root| {
            let dir = root.parent().map(Path::to_path_buf).unwrap_or_default();
            (root.clone(), dir)
        })
        .collect();
    while let Some((path, dir)) = pending.pop() {
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
        strip_tests(&mut syntax.items);
        let found = ModuleFiles {
            file: &path,
            rel: &rel,
        };
        found.collect(&syntax.items, &dir, false, &mut pending)?;
        files.push(SourceFile { rel, syntax });
    }
    files.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(files)
}

struct ModuleFiles<'a> {
    file: &'a Path,
    rel: &'a str,
}

impl ModuleFiles<'_> {
    /// Queues the file of every out-of-line `mod name;` among `items`. `dir` is where the child
    /// modules of this scope live; `inline` says whether `items` sit inside an inline module.
    fn collect(
        &self,
        items: &[Item],
        dir: &Path,
        inline: bool,
        pending: &mut Vec<(PathBuf, PathBuf)>,
    ) -> Result<(), String> {
        for item in items {
            let Item::Mod(declared) = item else {
                continue;
            };
            let name = declared.ident.to_string();
            let name = name.trim_start_matches("r#").to_owned();
            match &declared.content {
                Some((_, inner)) => self.collect(inner, &dir.join(&name), true, pending)?,
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
                    pending.push((path, child_dir));
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
