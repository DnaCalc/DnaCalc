//! Periodic check for independent tables of catalogue function spellings.
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use syn::visit::{self, Visit};

fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        let tokens = list.tokens.to_string();
        (list.path.is_ident("cfg") && tokens.trim() == "test")
            || (list.path.is_ident("cfg_attr")
                && tokens.split(',').any(|part| part.trim() == "test"))
    })
}

struct Literals<'a> {
    names: &'a BTreeSet<String>,
    found: BTreeSet<String>,
}
impl<'ast> Visit<'ast> for Literals<'_> {
    fn visit_lit_str(&mut self, value: &'ast syn::LitStr) {
        let text = value.value();
        if self.names.contains(&text) {
            self.found.insert(text);
        }
    }
}

struct Arrays<'a> {
    names: &'a BTreeSet<String>,
    found: BTreeSet<String>,
}
impl<'ast> Visit<'ast> for Arrays<'_> {
    fn visit_expr_array(&mut self, array: &'ast syn::ExprArray) {
        let mut literals = Literals {
            names: self.names,
            found: BTreeSet::new(),
        };
        visit::visit_expr_array(&mut literals, array);
        if literals.found.len() >= 2 {
            self.found.extend(literals.found);
        }
    }
}

struct Tables<'a> {
    names: &'a BTreeSet<String>,
    findings: Vec<String>,
}
impl Tables<'_> {
    fn check(&mut self, name: &syn::Ident, expr: &syn::Expr) {
        let mut arrays = Arrays {
            names: self.names,
            found: BTreeSet::new(),
        };
        arrays.visit_expr(expr);
        // Also recognize names stored together in a whitespace-delimited string.
        if let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(value),
            ..
        }) = expr
        {
            let value = value.value();
            let words: Vec<_> = value.split_whitespace().collect();
            if words.len() >= 2 && words.iter().all(|word| self.names.contains(*word)) {
                arrays.found.extend(words.into_iter().map(str::to_owned));
            }
        }
        let looks_like_table = arrays.found.iter().any(|name| name.len() > 1)
            || name.to_string().to_ascii_uppercase().contains("FUNC");
        if looks_like_table && !arrays.found.is_empty() {
            self.findings.push(format!(
                "{name}: {}",
                arrays.found.into_iter().collect::<Vec<_>>().join(", ")
            ));
        }
    }
}
impl<'ast> Visit<'ast> for Tables<'_> {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if !test_only(&item.attrs) {
            visit::visit_item_mod(self, item);
        }
    }
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if !test_only(&item.attrs) && !item.attrs.iter().any(|attr| attr.path().is_ident("test")) {
            visit::visit_item_fn(self, item);
        }
    }
    fn visit_local(&mut self, local: &'ast syn::Local) {
        if let (syn::Pat::Ident(binding), Some(init)) = (&local.pat, &local.init) {
            self.check(&binding.ident, &init.expr);
        }
        visit::visit_local(self, local);
    }
    fn visit_item_const(&mut self, item: &'ast syn::ItemConst) {
        if !test_only(&item.attrs) {
            self.check(&item.ident, &item.expr);
        }
    }
    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        if !test_only(&item.attrs) {
            self.check(&item.ident, &item.expr);
        }
    }
}

fn source_findings(source: &str, names: &BTreeSet<String>) -> Result<Vec<String>, String> {
    let file = syn::parse_file(source).map_err(|error| error.to_string())?;
    let mut tables = Tables {
        names,
        findings: Vec::new(),
    };
    tables.visit_file(&file);
    Ok(tables.findings)
}

fn powershell_findings(source: &str, names: &BTreeSet<String>) -> Vec<String> {
    source
        .lines()
        .enumerate()
        .filter_map(|(line, text)| {
            let (variable, _) = text.trim().strip_prefix('$')?.split_once(" = @(")?;
            if !variable
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            {
                return None;
            }
            let body = source.lines().skip(line).collect::<Vec<_>>().join("\n");
            let (_, body) = body.split_once(" = @(")?;
            let (array, _) = body.split_once(')')?;
            let found: BTreeSet<String> = array
                .split('\'')
                .skip(1)
                .step_by(2)
                .map(str::to_ascii_uppercase)
                .filter(|name| names.contains(name))
                .collect();
            (found.len() >= 2).then(|| {
                format!(
                    "${variable}: {}",
                    found.into_iter().collect::<Vec<_>>().join(", ")
                )
            })
        })
        .collect()
}

fn sources(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for item in fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))? {
        let item = item.map_err(|error| error.to_string())?;
        let path = item.path();
        if path.is_dir() {
            if matches!(
                item.file_name().to_str(),
                Some("target" | "tests" | "fixtures" | "calc-catalogue")
            ) {
                continue;
            }
            sources(&path, files)?;
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| matches!(ext, "rs" | "ps1"))
        {
            files.push(path);
        }
    }
    Ok(())
}

pub fn check(root: &Path) -> (bool, String) {
    let names: BTreeSet<String> = calc_catalogue::declarations()
        .filter(|entry| entry.notation() == calc_catalogue::Notation::Call)
        .map(|entry| entry.spelling().to_owned())
        .collect();
    let mut files = Vec::new();
    for dir in ["crates", "apps", "platform", "tools"] {
        if let Err(error) = sources(&root.join(dir), &mut files) {
            return (false, error);
        }
    }
    files.sort();
    let mut findings = Vec::new();
    let mut retained = Vec::new();
    for file in &files {
        let source = match fs::read_to_string(file) {
            Ok(source) => source,
            Err(error) => return (false, format!("{}: {error}", file.display())),
        };
        let relative = file.strip_prefix(root).unwrap_or(file);
        let rows = if file.extension().is_some_and(|ext| ext == "ps1") {
            powershell_findings(&source, &names)
        } else {
            match source_findings(&source, &names) {
                Ok(rows) => rows,
                Err(error) => return (false, format!("{}: {error}", file.display())),
            }
        };
        for row in rows {
            let (symbol, _) = row.split_once(':').expect("finding has a symbol");
            // Cetab macro commands and COM primitive types are distinct Excel namespaces.
            if (relative == Path::new("crates/calc-doc/src/xlsb/commands.rs")
                && symbol == "COMMANDS")
                || (relative == Path::new("crates/calc-object-model/src/descriptor.rs")
                    && symbol == "BASE_TYPES")
            {
                continue;
            }
            if relative == Path::new("platform/calc-oracle-excel/worker/capture-worker.ps1")
                && symbol == "$ExternalDataFunctions"
            {
                retained.push(format!(
                    "{}: {row} (capture isolation policy, dc-wi2wu)",
                    relative.display()
                ));
            } else {
                findings.push(format!("{}: {row}", relative.display()));
            }
        }
    }
    for finding in findings.iter().chain(&retained) {
        println!("  {finding}");
    }
    (
        findings.is_empty(),
        format!(
            "{} Rust and PowerShell files; {} function-name tables ({} new, {} retained)",
            files.len(),
            findings.len() + retained.len(),
            findings.len(),
            retained.len(),
        ),
    )
}

pub const USAGE: &str = "usage: cargo xtask name-tables";

pub fn run(raw: &[String]) -> std::process::ExitCode {
    if !raw.is_empty() {
        eprintln!("{USAGE}");
        return std::process::ExitCode::from(2);
    }
    let workspace = std::env::current_dir()
        .map_err(|error| error.to_string())
        .and_then(|cwd| crate::cargo::Workspace::load(&cwd));
    let workspace = match workspace {
        Ok(workspace) => workspace,
        Err(error) => {
            eprintln!("xtask name-tables: {error}");
            return std::process::ExitCode::from(1);
        }
    };
    let (passed, summary) = check(&workspace.root);
    println!("xtask name-tables: {summary}");
    if passed {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::from(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_tables_and_ignores_single_names_and_test_modules() {
        let names = ["SUM", "COUNT", "MAX"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let source = r#"
            const DUPLICATE: &[&str] = &["SUM", "COUNT"];
            const SINGLE: &str = "MAX";
            const PACKED: &str = "MAX SUM";
            fn authored() { let inline = ["COUNT", "MAX"]; }
            #[cfg(test)] mod tests { const EXAMPLE: &[&str] = &["SUM", "MAX"]; }
        "#;
        assert_eq!(
            source_findings(source, &names).unwrap(),
            vec![
                "DUPLICATE: COUNT, SUM",
                "PACKED: MAX, SUM",
                "inline: COUNT, MAX"
            ]
        );
    }

    #[test]
    fn short_non_function_labels_do_not_form_a_function_table() {
        let names = ["N", "T"].into_iter().map(str::to_owned).collect();
        assert!(
            source_findings(r#"const WEEKDAYS: &[&str] = &["N", "T"];"#, &names)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            source_findings(r#"const FUNCTIONS: &[&str] = &["N", "T"];"#, &names).unwrap(),
            vec!["FUNCTIONS: N, T"]
        );
    }
}
