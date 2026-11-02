//! Bounded name projections and explicit, identity-qualified evaluations.
use crate::{cells, wire, workbook::Workbook};
use calc_host::value::{CellKey, CoreRef, NameId, NameScope};
use dna_calc_model::names::{NameEvaluation, NamePage, NameRecord};

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
            formula: d.formula.clone(),
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
    let text = host
        .evaluate_name(NameId::from_parts(id.0, id.1), version, anchor)
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
        assert!(value.text.unwrap_err().contains("changed"));
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
