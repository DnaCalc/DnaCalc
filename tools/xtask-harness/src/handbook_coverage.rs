//! Join handbook pages and executable catalogue entries to the catalogue workbook's verdicts.
use super::{compare_program, oracle_workbooks, parse_args, repository_root, sectioned_workbook};
use crate::classify::analyze_formula;
use crate::corpus_summary::{Bucket, Compared, Times, Workbook, bucket};
use calc_catalogue::{EntryKind, Implementation, Notation, declarations};
use calc_verdict::sha256_hex;
use serde::Serialize;
use serde_json::json;
use std::{collections::BTreeMap, fs, path::Path, process::ExitCode};

pub(crate) const USAGE: &str = "usage: cargo xtask handbook-coverage (--handbook <api/functions directory> | --catalogue-only) --out <directory>";

#[derive(Default, Serialize)]
struct FunctionCoverage {
    handbook_page: bool,
    implemented_in_calc_func: bool,
    catalogue_oracle_rows: u64,
    matched: u64,
    mismatched: u64,
    refused: u64,
    failed_or_missing: u64,
    awaiting_capture: u64,
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|error| format!("{}: {error}", path.display()))
}

/// The handbook's signatures and help come from the same declarations the apps render.
fn export_catalogue(out: &Path) -> Result<(), String> {
    fs::create_dir_all(out).map_err(|error| format!("{}: {error}", out.display()))?;
    let mut entries: Vec<_> = declarations().collect();
    entries.sort_by_key(|entry| entry.id().raw());
    let functions: Vec<_> = entries
        .into_iter()
        .map(|entry| {
            let signature = entry.signature_help();
            let help = entry.help().map(|help| {
                json!({
                    "microsoft": help.microsoft,
                    "source": help.source,
                    "own": help.own,
                })
            });
            let parameters: Vec<_> = signature.parameters.iter().map(|parameter| {
                json!({
                    "name": parameter.name,
                    "optional": parameter.optional,
                    "repeats": parameter.repeats,
                })
            }).collect();
            let localized_names = entry.localized_names().map(|names| {
                names.iter().map(|name| json!({
                    "locale": name.locale,
                    "spelling": name.spelling,
                })).collect::<Vec<_>>()
            });
            json!({
                "id": entry.id().raw(),
                "spelling": entry.spelling(),
                "export_name": match entry.kind() { EntryKind::AddIn { export, .. } => Some(export), _ => None },
                "notation": match entry.notation() {
                    Notation::Call => "call",
                    Notation::Prefix => "prefix",
                    Notation::Infix => "infix",
                    Notation::Postfix => "postfix",
                },
                "signature": signature.display,
                "parameters": parameters,
                "help": help,
                "localized_names": localized_names,
            })
        })
        .collect();
    let output = json!({
        "format": "dnacalc-handbook-catalogue/1",
        "functions": functions
    });
    let path = out.join("function-catalogue.json");
    let bytes = serde_json::to_vec_pretty(&output).map_err(|error| error.to_string())?;
    fs::write(&path, bytes).map_err(|error| format!("{}: {error}", path.display()))?;
    println!(
        "xtask handbook-coverage: {} catalogue entries; wrote {}",
        functions.len(),
        path.display()
    );
    Ok(())
}

fn export(handbook: &Path, out: &Path) -> Result<(), String> {
    fs::create_dir_all(out).map_err(|error| format!("{}: {error}", out.display()))?;
    let mut functions = BTreeMap::<String, FunctionCoverage>::new();
    for page in
        fs::read_dir(handbook).map_err(|error| format!("{}: {error}", handbook.display()))?
    {
        let page = page.map_err(|error| error.to_string())?;
        if page.path().is_file()
            && let Some(name) = page
                .file_name()
                .to_str()
                .and_then(|name| name.strip_prefix("FUNC."))
                .and_then(|name| name.strip_suffix(".json"))
        {
            functions
                .entry(name.to_ascii_uppercase())
                .or_default()
                .handbook_page = true;
        }
    }
    for entry in declarations() {
        if entry.notation() == Notation::Call && !matches!(entry.kind(), EntryKind::Operator { .. })
        {
            let spelling = match entry.kind() {
                EntryKind::AddIn { export, .. } => export,
                _ => entry.spelling(),
            };
            functions
                .entry(spelling.to_ascii_uppercase())
                .or_default()
                .implemented_in_calc_func = entry.implementation() == Implementation::Held;
        }
    }

    let mut entry = oracle_workbooks(&repository_root())?
        .into_iter()
        .find(|entry| entry.name == "calc-func/catalogue/oracle.xlsx")
        .ok_or("the catalogue oracle workbook is missing")?;
    let case = entry.oracle.as_ref().expect("oracle entry has a case");
    let bytes = read(&entry.path)?;
    let digest = sha256_hex(&bytes);
    let (identity, _) = crate::corpus_batch::input_identity(&entry.path, &entry.name, &digest);
    entry.batch_input_identity = Some(identity);
    let inventory = crate::compare_inventory::source_record(&bytes, &entry.name)
        .map_err(|error| error.to_string())?;
    let mut workbook = Workbook {
        name: entry.name.clone(),
        sha256: Some(digest.clone()),
        input_identity: None,
        capture: None,
        inventory_formula_cells: Some(inventory.formulas.len() as u64),
        failure_key: None,
        outcome: Ok(Compared::default()),
        times: Times::default(),
    };
    let args = parse_args(&["--oracle".to_owned(), "--no-capture".to_owned()])?;
    let (program, _) = compare_program(None)?;
    let program_digest =
        crate::corpus_batch::file_digest(&program).map_err(|error| error.to_string())?;
    let mut verdicts = BTreeMap::new();
    sectioned_workbook(
        &mut workbook,
        &program,
        &entry,
        case,
        &bytes,
        &digest,
        out,
        &args,
        &program_digest,
        None,
        None,
        |triage| {
            for cell in triage["cells"].as_array().expect("full triage has cells") {
                let key = (
                    cell["sheet"].as_str().unwrap_or_default().to_owned(),
                    cell["row"].as_u64().unwrap_or_default(),
                    cell["column"].as_u64().unwrap_or_default(),
                );
                let result = match bucket(cell["verdict"].as_str().unwrap_or_default()) {
                    Bucket::Pass
                        if cell["live_excel_observation"] != true
                            || cell["external_links"] == true
                            || cell["circular_rule"] == true =>
                    {
                        Bucket::FailedOrMissing
                    }
                    result => result,
                };
                verdicts.insert(key, result);
            }
        },
    );
    let counts = workbook.outcome.map_err(|error| error.reason)?.counts;
    let mut unattributed = 0;
    for formula in &inventory.formulas {
        let Some(text) = &formula.formula else {
            unattributed += 1;
            continue;
        };
        let names = analyze_formula(text)
            .map_err(|error| format!("{}!{}: {error:?}", formula.cell.sheet(), formula.cell.a1()))?
            .functions;
        let result = verdicts.get(&(
            formula.cell.sheet().to_owned(),
            u64::from(formula.cell.row()),
            u64::from(formula.cell.column()),
        ));
        let mut attributed = false;
        for name in names.keys() {
            let Some(row) = functions.get_mut(name) else {
                continue;
            };
            attributed = true;
            row.catalogue_oracle_rows += 1;
            match result {
                Some(Bucket::Pass | Bucket::AgreedRejection) => row.matched += 1,
                Some(Bucket::Mismatched) => row.mismatched += 1,
                Some(Bucket::Refused) => row.refused += 1,
                Some(Bucket::FailedOrMissing) => row.failed_or_missing += 1,
                None => row.awaiting_capture += 1,
            }
        }
        unattributed += u64::from(!attributed);
    }
    let output = json!({
        "format": "dnacalc-handbook-coverage/1",
        "catalogue_oracle": entry.name,
        "catalogue_oracle_sha256": digest,
        "catalogue_oracle_rows": counts.formula_cells,
        "catalogue_oracle_counts": counts,
        "unattributed_oracle_rows": unattributed,
        "functions": functions,
    });
    let target = out.join("function-coverage.json");
    let bytes = serde_json::to_vec_pretty(&output).map_err(|error| error.to_string())?;
    fs::write(&target, bytes).map_err(|error| format!("{}: {error}", target.display()))?;
    println!(
        "xtask handbook-coverage: {} functions, {} oracle rows, {unattributed} unattributed; wrote {}",
        functions.len(),
        counts.formula_cells,
        target.display()
    );
    Ok(())
}

pub(crate) fn run(args: &[String]) -> ExitCode {
    let result = match args {
        [flag, handbook, output_flag, out] if flag == "--handbook" && output_flag == "--out" => {
            let out = Path::new(out);
            export_catalogue(out).and_then(|()| export(Path::new(handbook), out))
        }
        [flag, output_flag, out] if flag == "--catalogue-only" && output_flag == "--out" => {
            export_catalogue(Path::new(out))
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask handbook-coverage: {error}");
            ExitCode::FAILURE
        }
    }
}
