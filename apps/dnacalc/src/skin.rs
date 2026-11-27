use calc_host::{CalcMode, HostOptions, HostSession, MemorySource, OpenPolicy};
use calc_skin::{Origin, WorkbookPort, WritePolicy, query, query::Query};
use dna_skin_runtime::client::PAGE_BUDGET;
use dna_skin_runtime::skin;
use std::sync::Arc;

use crate::{Failure, fail};

pub(crate) fn validate(args: &[String]) -> Result<serde_json::Value, Failure> {
    let [subcommand, path, options @ ..] = args else {
        return Err(fail(
            "usage: dnacalc skin validate <manifest.json> [--workbook <file>] [--json]",
        ));
    };
    if subcommand != "validate" {
        return Err(fail(format!("Unknown skin command {subcommand}")));
    }
    let mut options = options.iter();
    let mut workbook = None;
    while let Some(option) = options.next() {
        match option.as_str() {
            "--json" => {}
            "--workbook" if workbook.is_none() => {
                workbook = Some(
                    options
                        .next()
                        .filter(|name| !name.starts_with("--"))
                        .ok_or_else(|| fail("--workbook needs a file"))?,
                );
            }
            _ => return Err(fail(format!("Unknown skin option {option}"))),
        }
    }
    let source = std::fs::read_to_string(path).map_err(|error| fail(format!("{path}: {error}")))?;
    let valid = skin::validate(&source).map_err(|reason| fail(format!("{path}: {reason}")))?;
    let bindings = skin::bindings(&valid.document);
    if let Some(book) = workbook {
        let bytes = std::fs::read(book).map_err(|error| fail(format!("{book}: {error}")))?;
        let session = HostSession::open(
            Arc::new(MemorySource::new(bytes)),
            &OpenPolicy::default(),
            HostOptions {
                calc_mode: Some(CalcMode::Manual),
                ..HostOptions::default()
            },
        )
        .map_err(|error| fail(format!("{book}: {error}")))?;
        let host = calc_skin_host::SkinHost::with_read_only(session, true);
        let port = host.connect(Origin::Automation, WritePolicy::Look);
        for bound in &bindings {
            let request = query::ResolveBound {
                bound: (*bound).clone(),
            };
            let resolved = port
                .ask(request.request(), PAGE_BUDGET)
                .and_then(query::ResolveBound::decode)
                .map_err(|reason| {
                    fail(format!(
                        "{}: {}",
                        bound.address,
                        dna_skin_runtime::client::refusal_text(reason)
                    ))
                })?;
            if let calc_skin::Coverage::Partial { .. } = resolved.coverage {
                return Err(fail(format!(
                    "{}: source resolution was incomplete",
                    bound.address
                )));
            }
        }
    }
    let schema = match &valid.document {
        skin::Document::Skin(_) => "dna.skin/1",
        skin::Document::Page(_) => "dna.page/1",
    };
    Ok(serde_json::json!({
        "schema": schema,
        "valid": true,
        "theme": valid.theme.map(|theme| theme.id()),
        "density": valid.density.map(|density| density.id()),
        "formula_font_px": valid.formula_size_px,
        "colour_overrides": valid.colours,
        "contrast_pairs": valid.contrast_pairs,
        "bindings": bindings.len(),
        "bindings_checked": workbook.is_some().then_some(bindings.len()),
    }))
}
