//! Periodic check for independent tables of catalogue function spellings.
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use syn::visit::{self, Visit};

fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let syn::Meta::List(list) = &attr.meta else { return false; };
        let tokens = list.tokens.to_string();
        (list.path.is_ident("cfg") && tokens.trim() == "test")
            || (list.path.is_ident("cfg_attr") && tokens.split(',').any(|part| part.trim() == "test"))
    })
}

struct Literals<'a> {
    names: &'a BTreeSet<String>,
    found: BTreeSet<String>,
}
impl<'ast> Visit<'ast> for Literals<'_> {
    fn visit_lit_str(&mut self, value: &'ast syn::LitStr) {
        let text = value.value();
        if self.names.contains(&text) { self.found.insert(text); }
    }
}

struct Arrays<'a> {
    names: &'a BTreeSet<String>,
    found: BTreeSet<String>,
}
impl<'ast> Visit<'ast> for Arrays<'_> {
    fn visit_expr_array(&mut self, array: &'ast syn::ExprArray) {
        let mut literals = Literals { names: self.names, found: BTreeSet::new() };
        visit::visit_expr_array(&mut literals, array);
        if literals.found.len() >= 2 { self.found.extend(literals.found); }
    }
}

struct Tables<'a> {
    names: &'a BTreeSet<String>,
    findings: Vec<String>,
}
impl Tables<'_> {
    fn check(&mut self, name: &syn::Ident, expr: &syn::Expr) {
        let mut arrays = Arrays { names: self.names, found: BTreeSet::new() };
        arrays.visit_expr(expr);
        let looks_like_table = arrays.found.iter().any(|name| name.len() > 1)
            || name.to_string().to_ascii_uppercase().contains("FUNC");
        if looks_like_table && !arrays.found.is_empty() {
            self.findings.push(format!("{name}: {}", arrays.found.into_iter().collect::<Vec<_>>().join(", ")));
        }
    }
}
impl<'ast> Visit<'ast> for Tables<'_> {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if !test_only(&item.attrs) { visit::visit_item_mod(self, item); }
    }
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if !test_only(&item.attrs) && !item.attrs.iter().any(|attr| attr.path().is_ident("test")) {
            visit::visit_item_fn(self, item);
        }
    }
    fn visit_item_const(&mut self, item: &'ast syn::ItemConst) {
        if !test_only(&item.attrs) { self.check(&item.ident, &item.expr); }
    }
    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        if !test_only(&item.attrs) { self.check(&item.ident, &item.expr); }
    }
}

fn source_findings(source: &str, names: &BTreeSet<String>) -> Result<Vec<String>, String> {
    let file = syn::parse_file(source).map_err(|error| error.to_string())?;
    let mut tables = Tables { names, findings: Vec::new() };
    tables.visit_file(&file);
    Ok(tables.findings)
}

fn rust_sources(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for item in fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))? {
        let item = item.map_err(|error| error.to_string())?;
        let path = item.path();
        if path.is_dir() {
            if matches!(item.file_name().to_str(), Some("target" | "tests" | "fixtures" | "calc-catalogue")) { continue; }
            rust_sources(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

pub fn check(root: &Path) -> (bool, String) {
    let names: BTreeSet<String> = calc_catalogue::declarations()
        .filter(|entry| entry.notation() == calc_catalogue::Notation::Call)
        .map(|entry| entry.spelling().to_owned()).collect();
    let mut files = Vec::new();
    for dir in ["crates", "apps", "platform", "tools"] {
        if let Err(error) = rust_sources(&root.join(dir), &mut files) { return (false, error); }
    }
    files.sort();
    let mut findings = Vec::new();
    for file in &files {
        let source = match fs::read_to_string(file) {
            Ok(source) => source,
            Err(error) => return (false, format!("{}: {error}", file.display())),
        };
        match source_findings(&source, &names) {
            Ok(rows) => {
                let relative = file.strip_prefix(root).unwrap_or(file);
                // Cetab is Excel's macro-command namespace, not worksheet function metadata.
                let macro_commands = relative == Path::new("crates/calc-doc/src/xlsb/commands.rs");
                findings.extend(rows.into_iter().filter(|row| !(macro_commands && row.starts_with("COMMANDS:")))
                    .map(|row| format!("{}: {row}", relative.display())));
            },
            Err(error) => return (false, format!("{}: {error}", file.display())),
        }
    }
    for finding in &findings { println!("  {finding}"); }
    (findings.is_empty(), format!("{} Rust files; {} separate constant or static name tables", files.len(), findings.len()))
}

pub const USAGE: &str = "usage: cargo xtask name-tables";

pub fn run(raw: &[String]) -> std::process::ExitCode {
    if !raw.is_empty() {
        eprintln!("{USAGE}");
        return std::process::ExitCode::from(2);
    }
    let workspace = std::env::current_dir().map_err(|error| error.to_string())
        .and_then(|cwd| crate::cargo::Workspace::load(&cwd));
    let workspace = match workspace {
        Ok(workspace) => workspace,
        Err(error) => { eprintln!("xtask name-tables: {error}"); return std::process::ExitCode::from(1); }
    };
    let (passed, summary) = check(&workspace.root);
    println!("xtask name-tables: {summary}");
    if passed { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::from(1) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_tables_and_ignores_single_names_and_test_modules() {
        let names = ["SUM", "COUNT", "MAX"].into_iter().map(str::to_owned).collect();
        let source = r#"
            const DUPLICATE: &[&str] = &["SUM", "COUNT"];
            const SINGLE: &str = "MAX";
            #[cfg(test)] mod tests { const EXAMPLE: &[&str] = &["SUM", "MAX"]; }
        "#;
        assert_eq!(source_findings(source, &names).unwrap(), vec!["DUPLICATE: COUNT, SUM"]);
    }

    #[test]
    fn short_non_function_labels_do_not_form_a_function_table() {
        let names = ["N", "T"].into_iter().map(str::to_owned).collect();
        assert!(source_findings(r#"const WEEKDAYS: &[&str] = &["N", "T"];"#, &names).unwrap().is_empty());
        assert_eq!(source_findings(r#"const FUNCTIONS: &[&str] = &["N", "T"];"#, &names).unwrap(),
            vec!["FUNCTIONS: N, T"]);
    }

}
