//! Bounded name projections and explicit, identity-qualified evaluations.
use crate::{cells, wire, workbook::Workbook};
use calc_host::value::{CellKey, CoreRef, NameId, NameScope};
use dna_calc_model::names::{NameBodyEvaluation, NameBodyResult, NameEvaluation,
    NameEvaluationEvidence, NameEvaluationRoot, NamePage, NameRecord};

pub fn page(book: &Workbook, query: String, page: u64) -> NamePage {
    let host = book.host();
    let query = calc_host::value::fold_workbook_name(&query);
    let start = page.saturating_mul(200);
    let mut total = 0usize;
    let mut rows = Vec::new();
    for node in host.model().definitions().name_table().nodes() {
        let Some(node) = host.name_node(node.id()) else {
            continue;
        };
        if !node.key().folded.contains(&query) {
            continue;
        }
        let index = total as u64;
        total += 1;
        if index < start || rows.len() >= 200 {
            continue;
        }
        let d = node.definition();
        rows.push(NameRecord {
            id: (node.id().slot(), node.id().generation()),
            version: node.binding_version(),
            name: d.name.clone(),
            scope: match d.scope {
                NameScope::Workbook => None,
                NameScope::Sheet(s) => Some(wire::Sheet::of(s)),
            },
            formula: dna_formula_ui::display::formula(&format!("={}", d.formula))
                .strip_prefix('=').unwrap_or(&d.formula).to_owned(),
            hidden: d.hidden,
            comment: d.comment.clone(),
        });
    }
    NamePage {
        revision: host.revision().get(),
        rows,
        page,
        total,
    }
}
pub fn evaluate(book: &Workbook, id: (u32, u32), version: u64, anchor: CellKey) -> NameEvaluation {
    let host = book.host();
    let mut evidence = None;
    let text = host
        .evaluate_name_with_evidence(NameId::from_parts(id.0, id.1), version, anchor)
        .and_then(|diagnostic| {
            evidence = Some(NameEvaluationEvidence {
                results: diagnostic.results.0,
                workbook: (diagnostic.workbook.document, diagnostic.workbook.incarnation),
                numeric: if diagnostic.evaluation.numeric.target().is_some() { "Exact" } else { "Fast" }.into(),
                root: diagnostic.evaluation.root.as_ref().map(|root| NameEvaluationRoot {
                    command: (root.pass.command.domain_incarnation, root.pass.command.sequence),
                    pass: root.pass.ordinal, evaluation: root.evaluation,
                }),
                clock: diagnostic.evaluation.clock.map(|clock| format!("{:016x}", clock.to_bits())),
                random_draws: diagnostic.evaluation.random_draws.samples.iter()
                    .map(|bits| format!("{bits:016x}")).collect(),
                omitted_random_draws: diagnostic.evaluation.random_draws.omitted,
                bodies: diagnostic.evaluation.invocations.iter().map(|body| {
                    use calc_host::NameResultKind as K;
                    let result = match &body.result {
                        K::Empty => NameBodyResult::Empty,
                        K::Missing => NameBodyResult::Missing,
                        K::Number => NameBodyResult::Number,
                        K::Text => NameBodyResult::Text,
                        K::Logical => NameBodyResult::Logical,
                        K::Error(error) => NameBodyResult::Error(error.excel_spelling().to_owned()),
                        K::Reference { areas, omitted_areas } => NameBodyResult::Reference {
                            areas: areas.iter().copied().map(wire::Area::of).collect(),
                            omitted_areas: *omitted_areas,
                        },
                        K::Array(shape) => NameBodyResult::Array { rows: shape.rows, columns: shape.cols },
                        K::Callable => NameBodyResult::Callable,
                        K::Refused => NameBodyResult::Refused,
                    };
                    NameBodyEvaluation {
                        ordinal: body.ordinal, parent: body.parent,
                        id: (body.definition.id.slot(), body.definition.id.generation()),
                        version: body.definition.version,
                        scope: match body.definition.scope {
                            NameScope::Workbook => None,
                            NameScope::Sheet(sheet) => Some(wire::Sheet::of(sheet)),
                        },
                        caller: wire::Key::of(body.context.caller.cell()),
                        definition_sheet: body.context.definition_sheet.map(wire::Sheet::of),
                        body: body.body, result,
                    }
                }).collect(),
                omitted: diagnostic.evaluation.omitted,
            });
            diagnostic.evaluation.outcome.map_err(|e| e.to_string())
        })
        .map(|value| match value.core_ref() {
            CoreRef::Reference(reference) => {
                let areas = reference.areas();
                let label = areas
                    .iter()
                    .take(8)
                    .map(|a| cells::area_label(host, *a))
                    .collect::<Vec<_>>()
                    .join(", ");
                let total = areas
                    .iter()
                    .map(|a| a.cell_count())
                    .fold(0u64, u64::saturating_add);
                let mut keys = Vec::new();
                'areas: for a in areas {
                    for r in a.first().row()..=a.last().row() {
                        for c in a.first().column()..=a.last().column() {
                            if keys.len() >= 16 {
                                break 'areas;
                            }
                            keys.push(CellKey::new(a.sheet(), r, c).expect("reference cell"));
                        }
                    }
                }
                let values = keys
                    .into_iter()
                    .filter_map(|at| book.cell(at))
                    .map(|cell| {
                        let shown = book.shown(&cell);
                        format!("{} {}", shown.provenance.name(), shown.text)
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                format!(
                    "Reference {label} → {values}{}",
                    if total > 16 {
                        "; … (first 16 cells)"
                    } else {
                        ""
                    }
                )
            }
            CoreRef::Array(array) => {
                let shape = array.shape();
                let text = array
                    .iter_row_major()
                    .take(16)
                    .map(|v| dna_formula_ui::result::shown(v, book.profile()))
                    .collect::<Vec<_>>()
                    .join("; ");
                format!(
                    "{}×{} array: {text}{}",
                    shape.rows,
                    shape.cols,
                    if shape.rows.saturating_mul(shape.cols) > 16 {
                        "; … (first 16 values)"
                    } else {
                        ""
                    }
                )
            }
            _ => dna_formula_ui::result::shown(&value, book.profile()),
        });
    NameEvaluation {
        id,
        version,
        revision: host.revision().get(),
        command: 0,
        anchor: wire::Key::of(anchor),
        evidence,
        text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        server::Server,
        wire::{Intent, Reply, Request},
    };
    fn decoded(request: Request) -> Request {
        wire::decode(&wire::encode(&request)).unwrap()
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn definitions_are_passive_and_explicit_evaluations_are_versioned_journalled_commands() {
        let mut server = Server::new();
        let made = server.execute_current(
            decoded(Request::Intent(Intent::EditName {
                id: None,
                version: None,
                name: "Answer".into(),
                scope: None,
                formula: Some("=40+2".into()),
            })),
            None,
        );
        assert!(
            matches!(
                made.reply,
                Ok(Reply::Done {
                    outcome: wire::Outcome { accepted: true, .. },
                    ..
                })
            ),
            "{:?}",
            made.reply
        );
        let revision = server.book().host().revision();
        let undo = server.book().session().history_depth().0;
        let Ok(Reply::Names(page)) = server
            .handle(
                decoded(Request::Names {
                    query: "ans".into(),
                    page: 0,
                }),
                None,
            )
            .reply
        else {
            panic!("passive name projection")
        };
        assert_eq!(page.total, 1);
        let row = &page.rows[0];
        assert_eq!(row.formula, "40+2");
        assert_eq!(server.book().host().revision(), revision);
        let anchor = CellKey::new(server.book().sheets()[0].id, 1, 1).unwrap();
        let evaluate = Request::EvaluateName {
            id: row.id,
            version: row.version,
            anchor: wire::Key::of(anchor),
        };
        assert!(
            server.handle(evaluate.clone(), None).reply.is_err(),
            "unqualified queries cannot evaluate names"
        );
        let envelope = server.envelope(evaluate);
        let replay = envelope.clone();
        let request = decoded(envelope.clone());
        let first = server.handle(request, None).reply.unwrap();
        let again = server.handle(decoded(envelope), None).reply.unwrap();
        assert_eq!(first, again, "retry returns the recorded explicit result");
        let Reply::NameEvaluation(value) = first else {
            panic!("explicit evaluation")
        };
        assert!(value.command > 0);
        assert_eq!(value.id, row.id);
        assert_eq!(value.version, row.version);
        assert_eq!(value.text, Ok("42".into()));
        assert_eq!(value.anchor, wire::Key::of(anchor));
        let evidence = value.evidence.as_ref().expect("actual diagnostic evidence");
        assert_eq!(evidence.numeric, "Fast");
        let root = evidence.root.as_ref().unwrap();
        assert_eq!((root.pass, root.evaluation), (0, 0));
        assert!(root.command.1 > 0);
        assert!(evidence.random_draws.is_empty());
        assert_eq!(evidence.omitted, 0);
        assert_eq!(evidence.bodies.len(), 1);
        let body = &evidence.bodies[0];
        assert_eq!((body.id, body.version), (row.id, row.version));
        assert_eq!((body.ordinal, body.parent), (0, None));
        assert_eq!(body.caller, wire::Key::of(anchor));
        assert_eq!(body.scope, None);
        assert_eq!(body.result, NameBodyResult::Number);
        let encoded = wire::encode(&Reply::NameEvaluation(value.clone()));
        assert_eq!(wire::decode::<Reply>(&encoded).unwrap(), Reply::NameEvaluation(value.clone()));
        assert_eq!(server.book().host().revision(), revision);
        assert_eq!(server.book().session().history_depth().0, undo);
        server
            .execute_current(
                Request::Intent(Intent::EditName {
                    id: Some(row.id),
                    version: Some(row.version),
                    name: row.name.clone(),
                    scope: row.scope,
                    formula: Some("=43".into()),
                }),
                None,
            )
            .reply
            .unwrap();
        assert_eq!(
            server.handle(decoded(replay), None).reply.unwrap(),
            again,
            "a retry after an edit returns its earlier qualified value"
        );
        let reply = server
            .execute_current(
                Request::EvaluateName {
                    id: row.id,
                    version: row.version,
                    anchor: wire::Key::of(anchor),
                },
                None,
            )
            .reply
            .unwrap();
        let Reply::NameEvaluation(value) = reply else {
            panic!("qualified refusal")
        };
        assert!(value.evidence.is_none(), "stale definition never executes");
        assert!(value.text.unwrap_err().contains("changed"));
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn explicit_random_name_retries_retain_the_pass_and_raw_draws() {
        let mut server = Server::new();
        server.execute_current(decoded(Request::Intent(Intent::EditName {
            id: None, version: None, name: "Roll".into(), scope: None,
            formula: Some("=RAND()".into()),
        })), None).reply.unwrap();
        let row = page(server.book(), "Roll".into(), 0).rows.remove(0);
        let anchor = CellKey::new(server.book().sheets()[0].id, 1, 1).unwrap();
        let request = Request::EvaluateName { id: row.id, version: row.version, anchor: wire::Key::of(anchor) };
        let envelope = server.envelope(request.clone());
        let first = server.handle(decoded(envelope.clone()), None).reply.unwrap();
        let revision = server.book().host().revision();
        let history = server.book().session().history_depth();
        let again = server.handle(decoded(envelope.clone()), None).reply.unwrap();
        assert_eq!(first, again, "retry retains the actual name pass, not a new draw");
        assert_eq!(wire::decode::<Reply>(&wire::encode(&first)).unwrap(), first);
        let Reply::NameEvaluation(sample) = &first else { panic!("explicit random result"); };
        assert!(sample.text.is_ok());
        let evidence = sample.evidence.as_ref().unwrap();
        assert_eq!((evidence.random_draws.len(), evidence.omitted_random_draws), (1, 0));
        let bits = u64::from_str_radix(&evidence.random_draws[0], 16).unwrap();
        assert!((0.0..1.0).contains(&f64::from_bits(bits)));
        let second = server.execute_current(decoded(request), None).reply.unwrap();
        let Reply::NameEvaluation(second) = second else { panic!("another explicit result"); };
        assert_ne!(evidence.root.as_ref().unwrap().command,
            second.evidence.as_ref().unwrap().root.as_ref().unwrap().command);
        assert_eq!((server.book().host().revision(), server.book().session().history_depth()), (revision, history));
        server.execute_current(decoded(Request::Intent(Intent::EditName {
            id: Some(row.id), version: Some(row.version), name: "Roll".into(),
            scope: None, formula: Some("=0".into()),
        })), None).reply.unwrap();
        assert_eq!(server.handle(decoded(envelope), None).reply.unwrap(), first,
            "retry after an edit keeps the earlier input cut and draws");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn modern_name_entry_and_rename_keep_friendly_text_and_file_spelling() {
        let mut server = Server::new();
        server.execute_current(decoded(Request::Intent(Intent::EditName {
            id: None, version: None, name: "ArrayResult".into(), scope: None,
            formula: Some("=RANDARRAY(1,2)".into()),
        })), None).reply.unwrap();
        let row = page(server.book(), "ArrayResult".into(), 0).rows.remove(0);
        assert_eq!(row.formula, "RANDARRAY(1,2)");
        server.execute_current(decoded(Request::Intent(Intent::EditName {
            id: Some(row.id), version: Some(row.version), name: "RenamedArray".into(),
            scope: None, formula: Some(row.formula),
        })), None).reply.unwrap();
        let renamed = page(server.book(), "RenamedArray".into(), 0).rows.remove(0);
        assert_eq!(renamed.id, row.id);
        assert_eq!(renamed.formula, "RANDARRAY(1,2)");
        let stored = server.book().host().name_node(NameId::from_parts(renamed.id.0, renamed.id.1)).unwrap();
        assert_eq!(stored.definition().formula, "_xlfn.RANDARRAY(1,2)");
        let anchor = CellKey::new(server.book().sheets()[0].id, 1, 1).unwrap();
        let result = server.execute_current(decoded(Request::EvaluateName {
            id: renamed.id, version: renamed.version, anchor: wire::Key::of(anchor),
        }), None).reply.unwrap();
        let Reply::NameEvaluation(result) = result else { panic!("array name result"); };
        assert!(result.text.as_ref().unwrap().starts_with("1×2 array:"));
        assert_eq!(result.evidence.unwrap().random_draws.len(), 2);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn name_rename_and_scope_move_use_the_existing_journalled_wire_command() {
        let mut server = Server::new();
        let made = server.execute_current(
            decoded(Request::Intent(Intent::EditName {
                id: None,
                version: None,
                name: "Answer".into(),
                scope: None,
                formula: Some("=42".into()),
            })),
            None,
        );
        assert!(
            matches!(
                made.reply,
                Ok(Reply::Done {
                    outcome: wire::Outcome { accepted: true, .. },
                    ..
                })
            ),
            "{:?}",
            made.reply
        );
        let sheet = server.book().sheets()[0].id;
        let mut id = None;
        for scope in [None, Some(wire::Sheet::of(sheet)), None] {
            let Reply::Names(page) = server
                .handle(
                    decoded(Request::Names {
                        query: "".into(),
                        page: 0,
                    }),
                    None,
                )
                .reply
                .unwrap()
            else {
                panic!("name projection")
            };
            let row = page
                .rows
                .iter()
                .find(|row| row.name == if id.is_none() { "Answer" } else { "Renamed" })
                .unwrap();
            let previous = server.book().session().history_depth().0;
            id.get_or_insert(row.id);
            assert_eq!(Some(row.id), id);
            let request = server.envelope(Request::Intent(Intent::EditName {
                id: Some(row.id),
                version: Some(row.version),
                name: "Renamed".into(),
                scope,
                formula: Some("=42".into()),
            }));
            let first = server.handle(decoded(request.clone()), None).reply.unwrap();
            assert!(
                matches!(
                    first,
                    Reply::Done {
                        outcome: wire::Outcome { accepted: true, .. },
                        ..
                    }
                ),
                "{first:?}"
            );
            let revision = server.book().host().revision();
            assert_eq!(server.handle(decoded(request), None).reply.unwrap(), first);
            assert_eq!(server.book().host().revision(), revision);
            assert_eq!(server.book().session().history_depth().0, previous + 1);
        }
    }
}
