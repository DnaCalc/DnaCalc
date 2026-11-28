use calc_host::{HostOptions, HostSession, MemorySource, OpenPolicy, blank_package};
use calc_skin::grid::{Area, Key, Sheet};
use calc_skin::query::Query;
use calc_skin::*;
use calc_skin_host::SkinHost;
use std::num::NonZeroU128;
use std::sync::Arc;

const BUDGET: Budget = Budget {
    cells: 1024,
    nodes: 1024,
    edges: 1024,
    bytes: 1024 * 1024,
    millis: 100,
};

fn workbook() -> (SkinHost, Sheet) {
    let session = HostSession::open(
        Arc::new(MemorySource::new(blank_package())),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap();
    let sheet = Sheet::of(session.host().sheets()[0].id());
    (SkinHost::new(session), sheet)
}

fn port(host: &SkinHost) -> impl WorkbookPort + use<> {
    host.connect(
        Origin::User {
            view: ViewId("classic".into()),
        },
        WritePolicy::Commit,
    )
}

fn enter(at: Key, text: &str) -> Change {
    Change::Model(ModelChange::Enter {
        at,
        text: text.into(),
    })
}

fn batch(id: u64, changes: Vec<Change>) -> Batch {
    Batch {
        id: id.to_string(),
        workbook: None,
        base: None,
        expects: Vec::new(),
        selection_seq: None,
        label: None,
        changes,
    }
}

fn target(at: Key) -> SourceRef {
    SourceRef::Cells {
        sheet: Sheet(at.0, at.1),
        areas: vec![Area(at, at)],
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn evaluate_draft_part_uses_workbook_values_without_replacing_the_formula() {
    let (host, sheet) = workbook();
    let connection = port(&host);
    let source = Key(sheet.0, sheet.1, 1, 1);
    let caller = Key(sheet.0, sheet.1, 1, 2);
    connection.commit(batch(1, vec![enter(source, "3"), enter(caller, "=A1+1")])).unwrap();
    while host.advance() {}
    let formula = "=LET(amount,A1*2,amount+10)";
    let start = formula.find("amount+10").unwrap();
    let original = connection.ask(query::Formula { at: caller }.request(), BUDGET).unwrap();
    let request = query::EvaluateDraftPart { at: caller, formula: formula.into(), start, len: 9 };
    let answer = query::EvaluateDraftPart::decode(connection.ask(request.request(), BUDGET).unwrap()).unwrap();
    assert_eq!(answer.body.literal, "16");
    assert_eq!(answer.stamp, original.stamp);
    assert_eq!(connection.ask(query::Formula { at: caller }.request(), BUDGET).unwrap(), original);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn linked_input_reads_value_and_write_authority_together() {
    let (host, sheet) = workbook();
    let connection = port(&host);
    let at = Key(sheet.0, sheet.1, 1, 1);
    let anchor = AnchorId(NonZeroU128::new(10).unwrap());
    connection.commit(batch(1, vec![
        enter(at, "3"),
        Change::Presentation(PresentationChange::Bind { anchor, source: target(at) }),
    ])).unwrap();
    let read = |address| {
        let request = query::BoundInput { bound: Bound { anchor, address: String::from(address) } };
        query::BoundInput::decode(connection.ask(request.request(), BUDGET)?)
    };
    let result = read("Sheet1!A1").unwrap();
    assert_eq!(result.body.input, "3");
    assert!(result.body.writable);
    assert!(read("Sheet1!B1").is_err());
    connection.commit(batch(2, vec![enter(at, "=1+2")])).unwrap();
    let result = read("Sheet1!A1").unwrap();
    assert_eq!(result.body.input, "=1+2");
    assert!(!result.body.writable);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn defined_name_facts_project_the_bound_cells_and_spill_anchor() {
    let (host, sheet) = workbook();
    let connection = port(&host);
    let anchor = Key(sheet.0, sheet.1, 10, 1);
    let stamp = connection.ask(WorkbookQuery::Summary, BUDGET).unwrap().stamp;
    let mut edit = batch(1, vec![
        enter(anchor, "=SEQUENCE(3,2)"),
        Change::Model(ModelChange::Name {
            scope: NameScope::Workbook,
            change: NameChange::Define { name: "outputRows".into(), formula: "=Sheet1!$A$10#".into() },
        }),
        Change::Model(ModelChange::Name {
            scope: NameScope::Workbook,
            change: NameChange::Define { name: "outputCell".into(), formula: "=Sheet1!$A$10".into() },
        }),
        Change::Model(ModelChange::Name {
            scope: NameScope::Workbook,
            change: NameChange::Define { name: "calculatedScalar".into(), formula: "=$A$10+1".into() },
        }),
    ]);
    edit.base = Some(stamp.model);
    connection.commit(edit).unwrap();
    while host.advance() {}
    for (name, expected) in [
        ("outputRows", Some(SourceRef::Spill { anchor })),
        ("outputCell", Some(target(anchor))),
        ("calculatedScalar", None),
    ] {
        let query = query::DefinedName { scope: NameScope::Workbook, name: name.into() };
        let answer = connection.ask(query.request(), BUDGET).unwrap();
        assert_eq!(query::DefinedName::decode(answer).unwrap().body.source, expected, "{name}");
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn formula_name_links_respect_utf16_spans_and_lexical_shadowing() {
    let (host, sheet) = workbook();
    let connection = port(&host);
    let anchor = Key(sheet.0, sheet.1, 10, 1);
    let linked = Key(sheet.0, sheet.1, 2, 5);
    let local = Key(sheet.0, sheet.1, 3, 5);
    let stamp = connection.ask(WorkbookQuery::Summary, BUDGET).unwrap().stamp;
    let mut edit = batch(1, vec![
        enter(anchor, "=SEQUENCE(3,2)"),
        enter(linked, "=LET(emoji,\"🙂\",SUM(outputRows)+LEN(emoji))"),
        enter(local, "=LET(outputRows,4,outputRows+1)"),
        Change::Model(ModelChange::Name {
            scope: NameScope::Workbook,
            change: NameChange::Define { name: "outputRows".into(), formula: "=Sheet1!$A$10#".into() },
        }),
    ]);
    edit.base = Some(stamp.model);
    connection.commit(edit).unwrap();
    while host.advance() {}
    let request = query::FormulaNames { cells: vec![linked, local] };
    let answer = query::FormulaNames::decode(connection.ask(request.request(), BUDGET).unwrap()).unwrap();
    let [entry] = answer.body.as_slice() else { panic!("expected one linked formula") };
    assert_eq!(entry.at, linked);
    let [link] = entry.links.as_slice() else { panic!("expected one workbook name link") };
    let encoded = entry.text.encode_utf16().collect::<Vec<_>>();
    assert_eq!(String::from_utf16(&encoded[link.start..link.end]).unwrap(), "outputRows");
    assert_eq!(link.source, SourceRef::Spill { anchor });
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn page_inputs_require_both_binding_identities_and_preserve_formulas() {
    let (host, sheet) = workbook();
    let connection = port(&host);
    let at = Key(sheet.0, sheet.1, 1, 1);
    let anchor = AnchorId(NonZeroU128::new(10).unwrap());
    connection
        .commit(batch(1, vec![Change::Presentation(PresentationChange::Bind {
            anchor,
            source: target(at),
        })]))
        .unwrap();
    let page_input = |id, address| {
        batch(id, vec![serde_json::from_value(serde_json::json!({
            "Model": {"EnterBound": {"bound": {"anchor": anchor, "address": address}, "text": "42"}}
        })).expect("the skin wire accepts bound input entries")])
    };
    let address = "Sheet1!A1";
    for (id, wrong) in [(2, "Sheet1!B1"), (3, "A1")] {
        assert_eq!(
            connection.preview(&page_input(id, wrong), PreviewDepth::WritesOnly).unwrap().refusal,
            Some(Refusal::Invalid { reason: "Source missing".into() }),
        );
    }
    let edit = page_input(4, address);
    assert_eq!(connection.preview(&edit, PreviewDepth::WritesOnly).unwrap().writes[0].at, at);
    connection.commit(edit).unwrap();

    let stale = page_input(5, address);
    connection.preview(&stale, PreviewDepth::WritesOnly).unwrap();
    connection.commit(batch(6, vec![enter(at, "=21*2")])).unwrap();
    assert!(connection.commit(stale).is_err());
    let formula = query::Formula { at };
    let answer = query::Formula::decode(connection.ask(formula.request(), BUDGET).unwrap()).unwrap();
    assert_eq!(answer.body.input, "=21*2");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn calculation_changes_report_previous_and_published_formula_values() {
    let (host, sheet) = workbook();
    let port = port(&host);
    let input = Key(sheet.0, sheet.1, 1, 1);
    let output = Key(sheet.0, sheet.1, 1, 2);
    let unchanged = Key(sheet.0, sheet.1, 1, 3);
    let read = || {
        query::CalcChanges::decode(port.ask(query::CalcChanges.request(), BUDGET).unwrap())
            .unwrap()
    };
    assert!(matches!(read().body, Evidence::Unavailable { .. }));
    port.commit(batch(
        1,
        vec![
            enter(input, "5"),
            enter(output, "=A1*2"),
            enter(unchanged, "=A1*0"),
        ],
    ))
    .unwrap();
    while host.advance() {}
    port.commit(batch(2, vec![enter(input, "7")])).unwrap();
    while host.advance() {}

    let answer = read();
    let Evidence::Observed { at, body, .. } = answer.body else {
        panic!("the published result differences must be available");
    };
    assert_eq!(at, answer.stamp);
    assert_eq!(answer.coverage, Coverage::Complete);
    assert_eq!(body.len(), 1);
    let change = &body[0];
    assert_eq!(change.at, output);
    assert_eq!(change.before.value, Some(RawValue::Number { bits: 10f64.to_bits() }));
    assert_eq!(change.after.value, Some(RawValue::Number { bits: 14f64.to_bits() }));
    assert!(matches!(
        (&change.before.origin, &change.after.origin),
        (query::ResultOrigin::Calculated { tick: before }, query::ResultOrigin::Calculated { tick: after })
            if before < after
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn spill_blockers_report_the_current_attempted_extent_and_authored_obstruction() {
    let (host, sheet) = workbook();
    let connection = port(&host);
    let at = |row, column| Key(sheet.0, sheet.1, row, column);
    let anchor = at(10, 1);
    let obstruction = at(12, 1);
    connection.commit(batch(1, vec![
        enter(obstruction, "Review"),
        enter(at(2, 2), "4"),
        enter(anchor, "=SEQUENCE(B2,2)"),
    ])).unwrap();
    while host.advance() {}
    let query = query::SpillBlockers { anchor };
    let answer = query::SpillBlockers::decode(connection.ask(query.request(), BUDGET).unwrap()).unwrap();
    assert_eq!(answer.body.desired, Area(anchor, at(13, 2)));
    assert_eq!(answer.body.blockers, vec![SpillBlocker {
        area: Area(obstruction, obstruction),
        cell_to_move: Some(obstruction),
    }]);
    assert!(!answer.body.more);
    connection.commit(batch(2, vec![enter(at(2, 2), "1")])).unwrap();
    while host.advance() {}
    assert!(connection.ask(query.request(), BUDGET).is_err());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn row_watches_refresh_for_a_spill_change_in_their_read_footprint() {
    let (host, sheet) = workbook();
    let port = port(&host);
    let at = |row, column| Key(sheet.0, sheet.1, row, column);
    port.commit(batch(
        1,
        vec![enter(at(1, 11), "3"), enter(at(1, 1), "=SEQUENCE(K1,2)")],
    ))
    .unwrap();
    while host.advance() {}
    let watch = port
        .watch(
            query::Rows {
                // This range reads a spill follower and excludes its formula anchor.
                source: target(at(3, 2)),
                filter: vec![],
                sort: vec![],
                window: RowWindow {
                    offset: 0,
                    limit: 1,
                },
            }
            .request(),
            BUDGET,
        )
        .unwrap();
    let first = port.next_frame().unwrap().unwrap();
    let value = |frame: Frame| {
        frame.updates.into_iter().find_map(|update| match update {
            Update::Ready {
                watch: id,
                body,
                coverage,
            } if id == watch => {
                let answer = query::Rows::decode(Answer {
                    stamp: frame.stamp,
                    coverage,
                    body,
                })
                .unwrap();
                Some(answer.body.rows[0].cells[0].text.clone())
            }
            _ => None,
        })
    };
    assert_eq!(value(first).as_deref(), Some("6"));

    port.commit(batch(2, vec![enter(at(1, 4), "=5+5")]))
        .unwrap();
    while host.advance() {}
    let unrelated = port.next_frame().unwrap().unwrap();
    assert_eq!(value(unrelated), None);

    port.commit(batch(3, vec![enter(at(1, 11), "2")])).unwrap();
    while host.advance() {}
    let shortened = port.next_frame().unwrap().unwrap();
    assert_eq!(value(shortened).as_deref(), Some(""));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn rows_filter_and_sort_the_whole_source_before_windowing() {
    let (host, sheet) = workbook();
    let port = port(&host);
    let at = |r, c| Key(sheet.0, sheet.1, r, c);
    let stamp = port
        .commit(batch(
            1,
            vec![
                enter(at(1, 1), "West"),
                enter(at(1, 2), "10"),
                enter(at(2, 1), "East"),
                enter(at(2, 2), "100"),
                enter(at(4, 1), "West"),
                enter(at(4, 2), "30"),
                enter(at(5, 1), "West"),
                enter(at(5, 2), "30"),
            ],
        ))
        .unwrap();
    let query = query::Rows {
        source: SourceRef::Cells {
            sheet,
            areas: vec![Area(at(1, 1), at(5, 2))],
        },
        filter: vec![RowFilter {
            column: 0,
            equals: RawValue::Text("West".encode_utf16().collect()),
        }],
        sort: vec![RowSort {
            column: 1,
            descending: true,
        }],
        window: RowWindow {
            offset: 1,
            limit: 1,
        },
    };
    let answer = query::Rows::decode(port.ask(query.request(), BUDGET).unwrap()).unwrap();
    assert_eq!(answer.stamp, stamp);
    assert_eq!(
        answer.coverage,
        Coverage::Partial {
            done: 1,
            total: Some(3),
            why: PartialWhy::Window
        }
    );
    assert_eq!(answer.body.source_rows, 5);
    assert_eq!(answer.body.matched_rows, 3);
    assert_eq!(answer.body.rows[0].row, 5);
    assert_eq!(answer.body.rows[0].cells[1].text, "30");
    assert!(matches!(
        port.ask(query.request(), Budget { cells: 9, ..BUDGET }),
        Err(Refusal::OverBudget { counts, .. }) if counts.cells >= 10
    ));
    for budget in [
        Budget { nodes: 4, ..BUDGET },
        Budget { bytes: 1, ..BUDGET },
        Budget {
            millis: 0,
            ..BUDGET
        },
    ] {
        assert!(matches!(
            port.ask(query.request(), budget),
            Err(Refusal::OverBudget { .. })
        ));
    }
    let raw = port
        .ask(
            WorkbookQuery::Cells {
                interest: Interest::Area(Area(at(1, 1), at(5, 2))),
            },
            BUDGET,
        )
        .unwrap();
    assert_eq!(raw.stamp, stamp);
    let cells: CellWindow = serde_json::from_value(raw.body).unwrap();
    assert_eq!(cells.cells[1].text, "10");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn notebook_workbook_lambda_has_a_typed_definition_and_signature() {
    let (host, _) = workbook();
    let port = port(&host);
    let summary = query::Summary::decode(port.ask(query::Summary.request(), BUDGET).unwrap()).unwrap();
    let definition = query::DefinedName {
        scope: NameScope::Workbook,
        name: "DoubleIt".into(),
    };
    let declare = |id, formula: &str| {
        let mut edit = batch(id, vec![Change::Model(ModelChange::Name {
            scope: NameScope::Workbook,
            change: NameChange::DefineFunction {
                name: "DoubleIt".into(),
                formula: formula.into(),
            },
        })]);
        edit.base = Some(summary.stamp.model);
        edit
    };
    let invoked = declare(1, "=LAMBDA(amount,amount*2)(21)");
    assert!(port.preview(&invoked, PreviewDepth::WritesOnly).unwrap().refusal.is_some());
    let authored = declare(2, "=LAMBDA(amount,amount*2)");
    let preview = port.preview(&authored, PreviewDepth::WritesOnly).unwrap();
    assert_eq!(preview.kind(), Some(ChangeKind::Model));
    assert!(port.ask(definition.request(), BUDGET).is_err());
    port.commit(authored).unwrap();

    let answer = query::DefinedName::decode(port.ask(definition.request(), BUDGET).unwrap()).unwrap();
    assert_eq!(answer.body.definition.name, "DoubleIt");
    assert_eq!(answer.body.definition.formula, "=LAMBDA(amount,amount*2)");
    assert_eq!(answer.body.parameters, Some(vec!["amount".into()]));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn new_sheet_placement_is_typed_and_does_not_create_the_sheet() {
    let (host, original) = workbook();
    let port = port(&host);
    let proposal = query::NewSheetPlacement {
        name: "Analysis".into(),
        kind: PlacementKind::Array,
        shape: Some(PlacementShape { rows: 5, columns: 7 }),
    };
    let answer = query::NewSheetPlacement::decode(port.ask(proposal.request(), BUDGET).unwrap()).unwrap();
    let at = answer.body.at;
    assert_ne!(Sheet(at.0, at.1), original);
    assert_eq!((at.2, at.3), (10, 1));
    assert_eq!(answer.body.lane.unwrap().1.3, 7);
    let summary = query::Summary::decode(port.ask(query::Summary.request(), BUDGET).unwrap()).unwrap();
    assert_eq!(summary.stamp, answer.stamp);
    assert_eq!(summary.body.sheets.len(), 1);
    assert_eq!(summary.body.sheets[0].sheet, original);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn placement_leaves_a_spacer_and_accounts_for_cells_far_below_the_spill() {
    let (host, sheet) = workbook();
    let port = port(&host);
    let at = |r, c| Key(sheet.0, sheet.1, r, c);
    let ask = |kind, shape, budget| {
        let query = query::Placement {
            sheet,
            kind,
            shape,
            reserved: vec![],
        };
        port.ask(query.request(), budget)
            .and_then(query::Placement::decode)
    };
    let initial = ask(PlacementKind::Scalar, None, BUDGET).unwrap();
    assert_eq!(initial.body.at, at(2, 2));
    assert_eq!(initial.body.label, Some(at(2, 1)));
    port.commit(batch(
        1,
        vec![
            enter(at(10, 1), "=SEQUENCE(3,2)"),
            enter(at(30_000, 5), "review"),
            enter(at(2, 1), "Label"),
        ],
    ))
    .unwrap();
    while host.advance() {}
    let shape = Some(PlacementShape {
        rows: 4,
        columns: 2,
    });
    let array = ask(PlacementKind::Array, shape, BUDGET).unwrap();
    assert_eq!(array.body.at, at(10, 7));
    assert_eq!(array.body.footprint, Area(at(10, 6), at(1_048_576, 9)));
    let scalar = ask(PlacementKind::Scalar, None, BUDGET).unwrap();
    assert_eq!(scalar.body.at, at(3, 2));
    assert_eq!(scalar.stamp, array.stamp);
    assert!(matches!(
        ask(PlacementKind::Array, shape, Budget { cells: 1, ..BUDGET }),
        Err(Refusal::OverBudget { .. })
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn placement_preserves_a_declared_lane_while_its_formula_is_narrower() {
    let (host, sheet) = workbook();
    let port = port(&host);
    let at = |row, column| Key(sheet.0, sheet.1, row, column);
    port.commit(batch(1, vec![enter(at(10, 1), "=SEQUENCE(3)")]))
        .unwrap();
    while host.advance() {}
    let placement = ask(
        &port,
        query::Placement {
            sheet,
            kind: PlacementKind::Array,
            shape: Some(PlacementShape {
                rows: 3,
                columns: 2,
            }),
            reserved: vec![Area(at(10, 1), at(1_048_576, 3))],
        },
    );
    assert_eq!(placement.at, at(10, 5));
    assert_eq!(placement.lane, Some(Area(at(10, 5), at(1_048_576, 6))));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mixed_preview_commit_retry_and_undo_use_one_transaction() {
    let (host, sheet) = workbook();
    let port = port(&host);
    let a1 = Key(sheet.0, sheet.1, 1, 1);
    let anchor = AnchorId(NonZeroU128::new(3).unwrap());
    let edit = batch(
        1,
        vec![
            enter(a1, "=SEQUENCE(3)"),
            Change::Presentation(PresentationChange::Bind {
                anchor,
                source: SourceRef::Spill { anchor: a1 },
            }),
        ],
    );
    let preview = port.preview(&edit, PreviewDepth::WritesOnly).unwrap();
    assert_eq!(preview.kind(), Some(ChangeKind::Both));
    assert_eq!(preview.writes.len(), 1);
    assert_eq!(preview.writes[0].at, a1);
    let initial = port.ask(WorkbookQuery::History, BUDGET).unwrap();
    assert_eq!(initial.stamp, preview.base);

    let committed = port.commit(edit.clone()).unwrap();
    assert!(committed.model > preview.base.model);
    assert!(committed.view > preview.base.view);
    assert_eq!(port.commit(edit).unwrap(), committed);
    let watches = port
        .watch(
            WorkbookQuery::Cells {
                interest: Interest::Anchors(vec![anchor]),
            },
            BUDGET,
        )
        .unwrap();
    let first = port.next_frame().unwrap().unwrap();
    assert!(
        first
            .updates
            .iter()
            .any(|update| matches!(update, Update::Ready { watch, .. } if *watch == watches))
    );

    let undone = port.run(Operation::Undo).unwrap();
    assert!(undone.view > committed.view);
    assert_eq!(
        port.ask(WorkbookQuery::Formula { at: a1 }, BUDGET)
            .unwrap()
            .body["input"],
        ""
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn stale_reader_conflicts_on_touched_cells_and_allows_an_unrelated_edit() {
    let (host, sheet) = workbook();
    let first = port(&host);
    let second = port(&host);
    let a1 = Key(sheet.0, sheet.1, 1, 1);
    let b1 = Key(sheet.0, sheet.1, 1, 2);
    let c1 = Key(sheet.0, sheet.1, 1, 3);
    let initial = first.ask(WorkbookQuery::Summary, BUDGET).unwrap().stamp;
    first.commit(batch(1, vec![enter(a1, "2")])).unwrap();

    let mut edit = batch(1, vec![enter(b1, "3")]);
    edit.base = Some(initial.model);
    edit.expects = vec![target(a1)];
    assert!(
        matches!(second.commit(edit), Err(Refusal::Conflict { conflicts, .. }) if conflicts == vec![Target::Source(target(a1))])
    );

    let mut edit = batch(2, vec![enter(b1, "3")]);
    edit.base = Some(initial.model);
    edit.expects = vec![target(c1)];
    assert!(second.commit(edit).is_ok());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn binary_windows_wait_for_ack_and_republish_the_live_anchor_extent() {
    use calc_skin::tiles::{TileBatch, decode_tiles};

    let (host, sheet) = workbook();
    let origin = Origin::User {
        view: ViewId("classic".into()),
    };
    let port = host.connect(origin, WritePolicy::Commit);
    let a1 = Key(sheet.0, sheet.1, 1, 1);
    let anchor = AnchorId(NonZeroU128::new(7).unwrap());
    port.commit(batch(
        1,
        vec![
            enter(a1, "=SEQUENCE(4)"),
            Change::Presentation(PresentationChange::Bind {
                anchor,
                source: SourceRef::Spill { anchor: a1 },
            }),
        ],
    ))
    .unwrap();
    while host.advance() {}
    let watch = port
        .watch(
            WorkbookQuery::Cells {
                interest: Interest::Anchors(vec![anchor]),
            },
            BUDGET,
        )
        .unwrap();
    let (transport, client) = InProcess::pair(8);
    let publish = || {
        for packet in port.next_packets().unwrap() {
            transport.send(&packet).unwrap();
        }
    };
    publish();
    let first = client.receive().unwrap().unwrap();
    let tile: TileBatch<WorkbookCell, Area> =
        decode_tiles(first.bytes.as_deref().unwrap()).unwrap();
    assert_eq!(tile.cells.len(), 4);
    assert_eq!(tile.geometry[0].1.1.2, 4);
    assert!(matches!(
        client.receive().unwrap().unwrap().envelope.message,
        Message::Frame(Frame { updates, .. })
            if matches!(&updates[0], Update::Tiles { watch: id, barrier, .. }
                if *id == watch && *barrier == tile.stamp.barrier())
    ));
    port.commit(batch(2, vec![enter(a1, "=SEQUENCE(2)")]))
        .unwrap();
    while host.advance() {}
    publish();
    assert!(client.receive().unwrap().is_none());
    client
        .send(&Packet {
            envelope: Envelope::new(
                3,
                Message::WorkbookRequest(WorkbookRequest::AcknowledgeTiles {
                    transaction: tile.stamp.transaction,
                }),
            ),
            bytes: None,
        })
        .unwrap();
    let reply = port.handle(transport.receive().unwrap().unwrap());
    assert_eq!(
        reply.envelope.message,
        Message::WorkbookReply(WorkbookReply::Accepted)
    );
    publish();
    let updated = client.receive().unwrap().unwrap();
    let tile: TileBatch<WorkbookCell, Area> =
        decode_tiles(updated.bytes.as_deref().unwrap()).unwrap();
    assert_eq!(tile.cells.len(), 2);
    assert_eq!(tile.geometry[0].1.1.2, 2);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn replacements_refuse_previous_workbook_drafts_and_old_history() {
    let (host, sheet) = workbook();
    let port = host.connect(
        Origin::User {
            view: ViewId("classic".into()),
        },
        WritePolicy::Commit,
    );
    let at = Key(sheet.0, sheet.1, 1, 1);
    let before = port.ask(WorkbookQuery::Summary, BUDGET).unwrap().stamp;
    let open = port.handle(Packet {
        envelope: Envelope::new(
            1,
            Message::WorkbookRequest(WorkbookRequest::Open {
                name: "Example.xlsx".into(),
            }),
        ),
        bytes: Some(blank_package()),
    });
    assert!(
        matches!(open.envelope.message, Message::WorkbookReply(WorkbookReply::Stamp(stamp)) if stamp.workbook != before.workbook)
    );
    let mut draft = batch(1, vec![enter(at, "19")]);
    draft.workbook = Some(before.workbook);
    assert!(matches!(port.commit(draft), Err(Refusal::Invalid { .. })));
    assert_eq!(
        port.ask(WorkbookQuery::History, BUDGET).unwrap().body["undo_depth"],
        0
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn suspended_screen_updates_resume_as_one_frame() {
    let (host, sheet) = workbook();
    let port = port(&host);
    let a1 = Key(sheet.0, sheet.1, 1, 1);
    port.watch(WorkbookQuery::Summary, BUDGET).unwrap();
    port.watch(WorkbookQuery::History, BUDGET).unwrap();
    port.next_frame().unwrap().unwrap();
    host.with_session(SelectionCause::Open, |session| {
        session.set_screen_updating(false)
    });
    port.commit(batch(1, vec![enter(a1, "1")])).unwrap();
    assert!(port.next_frame().unwrap().is_none());
    port.commit(batch(2, vec![enter(a1, "2")])).unwrap();
    assert!(port.next_frame().unwrap().is_none());
    host.with_session(SelectionCause::Open, |session| {
        session.set_screen_updating(true)
    });
    let frame = port.next_frame().unwrap().unwrap();
    assert_eq!(frame.updates.len(), 2);
    assert!(port.next_frame().unwrap().is_none());
}

/// Asks through the port and decodes the answer as the query's own type.
fn ask<Q: query::Query>(port: &impl WorkbookPort, query: Q) -> Q::Answer {
    Q::decode(port.ask(query.request(), BUDGET).unwrap())
        .unwrap()
        .body
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn every_answer_decodes_as_its_query_type() {
    use query::{CalcActivity, CalcMode};
    let (host, sheet) = workbook();
    let port = port(&host);
    let a1 = Key(sheet.0, sheet.1, 1, 1);
    let a2 = Key(sheet.0, sheet.1, 2, 1);
    port.commit(batch(1, vec![enter(a1, "2"), enter(a2, "=A1*21")]))
        .unwrap();

    let history = ask(&port, query::History);
    assert_eq!((history.undo_depth, history.redo_depth), (1, 0));
    assert_eq!(history.undo.len(), 1);
    assert_eq!(history.undo[0].kind, ChangeKind::Model);
    assert!(history.redo.is_empty());
    port.run(Operation::Undo).unwrap();
    let history = ask(&port, query::History);
    assert_eq!((history.undo.len(), history.redo.len()), (0, 1));
    port.run(Operation::Redo).unwrap();

    let calc = ask(&port, query::CalcState);
    assert_eq!(calc.mode, CalcMode::Automatic);
    assert_eq!(calc.circular, None);
    assert!(matches!(
        calc.activity,
        CalcActivity::Done | CalcActivity::Pending
    ));

    let census = ask(
        &port,
        query::CensusOf {
            scope: SourceRef::Sheet(sheet),
        },
    );
    assert_eq!((census.formulas, census.literals), (1, 1));

    let formula = ask(&port, query::Formula { at: a2 });
    assert!(formula.formula);
    assert_eq!(formula.input, "=A1*21");
    assert_eq!(formula.authored.as_deref(), Some("=A1*21"));
    assert_eq!(formula.as_saved, None);
    assert_eq!(formula.precedents, [Area(a1, a1)]);

    let rules = ask(&port, query::Rules { at: a2 });
    assert_eq!((rules.refusal, rules.validation), (None, None));

    assert_eq!(ask(&port, query::Summary).sheets[0].sheet, sheet);
    assert_eq!(
        ask(
            &port,
            query::Resolve {
                address: "A2".into()
            }
        ),
        target(a2)
    );
    assert!(ask(&port, query::Geometry { sheet }).default_row_height > 0.);
    let window = ask(
        &port,
        query::Cells {
            interest: Interest::Area(Area(a1, a2)),
        },
    );
    assert_eq!(window.cells.len(), 2);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn census_counts_sparse_scopes_and_current_errors_without_calculating() {
    use calc_skin::query::{CensusOf, Query};

    let (host, sheet) = workbook();
    host.with_session(SelectionCause::Open, |session| {
        session.dispatch(calc_host::SkinIntent::SetCalcMode(
            calc_host::CalcMode::Manual,
        ));
    });
    let port = port(&host);
    let a1 = Key(sheet.0, sheet.1, 1, 1);
    let a2 = Key(sheet.0, sheet.1, 2, 1);
    let a3 = Key(sheet.0, sheet.1, 3, 1);
    let z9 = Key(sheet.0, sheet.1, 9, 26);
    port.commit(batch(
        1,
        vec![enter(a1, "7"), enter(a3, "=1/0"), enter(z9, "=NA()")],
    ))
    .unwrap();
    let query = CensusOf {
        scope: SourceRef::Cells {
            sheet,
            areas: vec![Area(a1, a3), Area(a2, a3)],
        },
    };
    let budget = Budget { cells: 2, ..BUDGET };
    let pending = CensusOf::decode(port.ask(query.request(), budget).unwrap()).unwrap();
    assert_eq!(pending.coverage, Coverage::Complete);
    assert_eq!(pending.body.formulas, 1);
    assert_eq!(pending.body.literals, 1);
    assert!(pending.body.sheets[0].errors.is_empty());

    port.run(Operation::Calculate {
        scope: SourceRef::Workbook,
    })
    .unwrap();
    while host.advance() {}
    let calculated = CensusOf::decode(port.ask(query.request(), budget).unwrap()).unwrap();
    assert!(calculated.stamp.calc > pending.stamp.calc);
    assert_eq!(calculated.body.sheets[0].used, Some(Area(a1, a3)));
    assert_eq!(
        calculated.body.sheets[0].errors,
        [("#DIV/0!".into(), 1)].into()
    );

    assert!(matches!(
        port.ask(query.request(), Budget { cells: 1, ..BUDGET }),
        Err(Refusal::OverBudget { counts, .. }) if counts.cells == 2
    ));
    assert!(matches!(
        port.ask(
            query.request(),
            Budget {
                millis: 0,
                ..BUDGET
            }
        ),
        Err(Refusal::OverBudget { .. })
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn inventory_preserves_visibility_and_named_scope_with_a_node_budget() {
    use calc_skin::query::{Inventory, Query};

    let (host, sheet) = workbook();
    let notes = host.with_session(SelectionCause::Open, |session| {
        use calc_host::{CellEdit, SheetEdit, SheetKey, SheetName, SkinIntent};
        let insert = session.dispatch(SkinIntent::Edit {
            edits: vec![CellEdit::Sheet(SheetEdit::Insert {
                key: SheetKey::new(2),
                name: SheetName::new(calc_host::value::ExcelText::from_utf16_code_units(
                    "Notes".encode_utf16().collect(),
                ))
                .unwrap(),
                position: std::num::NonZeroU32::new(2).unwrap(),
            })],
        });
        assert!(insert.error.is_none(), "{:?}", insert.error);
        let other = session.host().sheets()[1].id();
        let visibility = session.dispatch(SkinIntent::Edit {
            edits: vec![CellEdit::Sheet(SheetEdit::SetVisibility {
                sheet: other,
                visibility: calc_host::SheetVisibility::VeryHidden,
            })],
        });
        assert!(visibility.error.is_none(), "{:?}", visibility.error);
        let name = session.dispatch(SkinIntent::Edit {
            edits: vec![CellEdit::SetName {
                name: "RevenueWindow".into(),
                scope: calc_host::NameScope::Sheet(other),
                formula: Some("#REF!".into()),
            }],
        });
        assert!(name.error.is_none(), "{:?}", name.error);
        Sheet::of(other)
    });
    let port = port(&host);
    let query = Inventory {
        kinds: vec![InventoryKind::Sheets, InventoryKind::Names],
    };
    let entries = Inventory::decode(port.ask(query.request(), BUDGET).unwrap())
        .unwrap()
        .body;
    assert_eq!(entries.len(), 3);
    assert!(matches!(
        &entries[0], InventoryItem::Sheet { sheet: id, visibility: SheetVisibility::Visible, .. }
        if *id == sheet
    ));
    assert!(matches!(
        &entries[1], InventoryItem::Sheet { sheet: id, visibility: SheetVisibility::VeryHidden, .. }
        if *id == notes
    ));
    assert!(matches!(
        &entries[2], InventoryItem::Name { scope: NameScope::Sheet(id), name, formula, .. }
        if *id == notes && name == "RevenueWindow" && formula == "=#REF!"
    ));
    assert!(matches!(
        port.ask(query.request(), Budget { nodes: 2, ..BUDGET }),
        Err(Refusal::OverBudget { counts, .. }) if counts.nodes == 3
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn structure_reports_exact_run_members_around_an_override() {
    use calc_skin::query::{Query, StructureOf};

    let (host, sheet) = workbook();
    let port = port(&host);
    let key = |row| Key(sheet.0, sheet.1, row, 2);
    let b1 = key(1);
    let b2 = key(2);
    let b3 = key(3);
    let b4 = key(4);
    let b5 = key(5);
    port.commit(batch(
        1,
        vec![
            enter(b1, "=A1*2"),
            enter(b2, "=A2*2"),
            enter(b3, "100"),
            enter(b4, "=A4*2"),
            enter(b5, "=A5*2"),
        ],
    ))
    .unwrap();
    let answer = port
        .ask(
            (StructureOf {
                scope: SourceRef::Sheet(sheet),
            })
            .request(),
            BUDGET,
        )
        .unwrap();
    let answer = StructureOf::decode(answer).unwrap();
    assert_eq!(
        answer.body,
        Evidence::Derived {
            from: Derivation::Structure,
            at: answer.stamp,
            body: Structure {
                tables: Vec::new(),
                formulas: 4,
                standalone: Vec::new(),
                ports: Vec::new(),
                port_count: 0,
                runs: vec![FormulaRun {
                    extent: Area(b1, b5),
                    r1c1: "=RC[-1]*2".into(),
                    members: vec![Area(b1, b2), Area(b4, b5)],
                    exceptions: vec![b3],
                }],
            },
        }
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn source_watch_tracks_rtd_admission_without_calculating() {
    use calc_skin::query::{Query, SourceInventory};

    let (host, _) = workbook();
    let port = port(&host);
    let query = SourceInventory;
    let watch = port.watch(query.request(), BUDGET).unwrap();
    let original = port.next_frame().unwrap().unwrap();
    let topic = calc_host::TopicKey::new("DnaCalc.Price", "", &["LARK".into(), "last".into()]);
    let lease = calc_host::RtdLease(1);
    host.with_session(SelectionCause::Open, |session| {
        assert_eq!(
            session.update_rtd(
                topic.clone(),
                lease,
                3,
                calc_host::value::CalcValue::number(42.0)
            ),
            calc_host::RtdDisposition::Accepted
        );
    });
    let updated = port
        .next_frame()
        .unwrap()
        .expect("source observation frame");
    assert_eq!(updated.stamp, original.stamp);
    assert!(matches!(&updated.updates[..], [Update::Ready { watch: id, .. }] if *id == watch));
    let answer = SourceInventory::decode(port.ask(query.request(), BUDGET).unwrap()).unwrap();
    assert_eq!(answer.body.topics[0].sequence, 3);
    assert_eq!(
        answer.body.topics[0].last.value,
        RawValue::Number {
            bits: 42.0f64.to_bits()
        }
    );
    assert!(matches!(
        answer.body.topics[0].last.provenance,
        Provenance::External { .. }
    ));
    host.with_session(SelectionCause::Open, |session| {
        assert_eq!(
            session.retire_rtd(topic, lease),
            calc_host::RtdDisposition::Accepted
        );
    });
    let retired = port.next_frame().unwrap().expect("source retirement frame");
    assert_eq!(retired.stamp, original.stamp);
    assert!(
        SourceInventory::decode(port.ask(query.request(), BUDGET).unwrap())
            .unwrap()
            .body
            .topics
            .is_empty()
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dependency_atlas_counts_cross_sheet_readers_at_sheet_granularity() {
    let (host, report) = workbook();
    let connection = port(&host);
    let placement = query::NewSheetPlacement { name: "Inputs".into(), kind: PlacementKind::Scalar, shape: None };
    let placed = query::NewSheetPlacement::decode(connection.ask(placement.request(), BUDGET).unwrap()).unwrap();
    let data = Sheet(placed.body.at.0, placed.body.at.1);
    let a1 = Key(data.0, data.1, 1, 1);
    let b1 = Key(data.0, data.1, 1, 2);
    let c1 = Key(report.0, report.1, 1, 3);
    let c2 = Key(report.0, report.1, 2, 3);
    let mut new_sheet = batch(1, vec![
        Change::Model(ModelChange::InsertWorksheet { name: "Inputs".into(), sheet: data }),
    ]);
    new_sheet.base = Some(placed.stamp.model);
    connection.commit(new_sheet).unwrap();
    connection.commit(batch(2, vec![enter(a1, "3"), enter(b1, "4"), enter(c1, "=Inputs!A1+Inputs!B1"), enter(c2, "=C1+1")])).unwrap();
    while host.advance() {}
    let request = query::DependencyAtlasOf;
    let answer = query::DependencyAtlasOf::decode(connection.ask(request.request(), BUDGET).unwrap()).unwrap();
    assert_eq!(answer.coverage, Coverage::Complete);
    let Evidence::Derived { from: Derivation::Dependencies, body, .. } = answer.body else {
        panic!("the sheet atlas is derived from the dependency graph");
    };
    assert_eq!(body.sheets.len(), 2);
    let [link] = body.links.as_slice() else { panic!("one cross-sheet link") };
    assert_eq!(link.consumer, report);
    assert_eq!(link.precedent, data);
    assert_eq!((link.readers, link.placed, link.dynamic), (1, 2, 0));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn cell_comparison_uses_current_numeric_values_and_bound_coverage() {
    let (host, sheet) = workbook();
    let connection = port(&host);
    let key = |row, column| Key(sheet.0, sheet.1, row, column);
    let left = key(1, 3);
    let right = key(1, 4);
    connection.commit(batch(1, vec![
        enter(key(1, 1), "10"),
        enter(key(2, 1), "20"),
        enter(key(3, 1), "4"),
        enter(left, "=SUM(A1:A3)"),
        enter(right, "=SUM(A1:A2)"),
    ])).unwrap();
    while host.advance() {}

    let compare = || {
        let request = query::CompareCells { left, right };
        let answer = query::CompareCells::decode(connection.ask(request.request(), BUDGET).unwrap()).unwrap();
        let Evidence::Derived { body, .. } = answer.body else { panic!("currently bound comparison") };
        body
    };
    let compared = compare();
    assert_eq!(compared.difference, Some(4f64.to_bits()));
    assert_eq!(compared.common, 0);
    for (reads, end) in [(compared.left_only, 3), (compared.right_only, 2)] {
        let [read] = reads.as_slice() else { panic!("one differing placed area") };
        assert_eq!(read.kind, DependencyKind::Placed);
        assert_eq!(read.target, DependencyTarget::Source(SourceRef::Cells {
            sheet, areas: vec![Area(key(1, 1), key(end, 1))],
        }));
    }

    host.with_session(SelectionCause::Open, |session| {
        assert!(session.dispatch(calc_host::SkinIntent::SetCalcMode(calc_host::CalcMode::Manual)).accepted());
    });
    connection.commit(batch(2, vec![enter(key(1, 1), "11")])).unwrap();
    let pending = compare();
    assert_eq!(pending.difference, None);
    assert!(matches!(pending.left.provenance, Provenance::Stale { .. }));
    assert!(matches!(pending.right.provenance, Provenance::Stale { .. }));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dependency_query_preserves_dynamic_edges_and_bounds_both_directions() {
    use calc_skin::query::{Deps, Query};

    let (host, sheet) = workbook();
    let port = port(&host);
    let a1 = Key(sheet.0, sheet.1, 1, 1);
    let b1 = Key(sheet.0, sheet.1, 1, 2);
    let c1 = Key(sheet.0, sheet.1, 1, 3);
    port.commit(batch(
        1,
        vec![
            enter(a1, "4"),
            enter(b1, "=INDIRECT(\"A1\")"),
            enter(c1, "=B1+1"),
        ],
    ))
    .unwrap();
    while host.advance() {}
    let upstream = Deps {
        of: target(c1),
        dir: DependencyDirection::Precedents,
        depth: 2,
        group: DependencyGrouping::Sheet,
    };
    let upstream = Deps::decode(port.ask(upstream.request(), BUDGET).unwrap()).unwrap();
    let at = upstream.stamp;
    assert_eq!(
        upstream.body,
        Evidence::Derived {
            from: Derivation::Dependencies,
            at,
            body: Dependencies {
                roots: vec![0],
                nodes: vec![c1, b1, a1]
                    .into_iter()
                    .map(|cell| DependencyTarget::Source(target(cell)))
                    .collect(),
                edges: vec![
                    DependencyEdge {
                        from: 0,
                        to: 1,
                        kind: DependencyKind::Placed
                    },
                    DependencyEdge {
                        from: 1,
                        to: 2,
                        kind: DependencyKind::Dynamic
                    },
                ],
                references: Vec::new(),
                sheets: vec![DependencySheetGroup {
                    sheet,
                    members: vec![0, 1, 2]
                }],
                unbound: Vec::new(),
            },
        }
    );

    let downstream = Deps {
        of: target(a1),
        dir: DependencyDirection::Dependents,
        depth: 2,
        group: DependencyGrouping::Reference,
    };
    let answer = Deps::decode(port.ask(downstream.request(), BUDGET).unwrap()).unwrap();
    let Evidence::Derived { body, .. } = answer.body else {
        panic!("derived dependencies");
    };
    assert_eq!(
        body.nodes,
        vec![a1, b1, c1]
            .into_iter()
            .map(|cell| DependencyTarget::Source(target(cell)))
            .collect::<Vec<_>>()
    );
    assert_eq!(body.edges.len(), 2);
    assert!(matches!(
        port.ask(downstream.request(), Budget { edges: 1, ..BUDGET }),
        Err(Refusal::OverBudget { counts, .. }) if counts.edges == 2
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dependency_query_preserves_spill_reference_identity() {
    let (host, sheet) = workbook();
    let connection = port(&host);
    let key = |row, column| Key(sheet.0, sheet.1, row, column);
    let a1 = key(1, 1);
    let c1 = key(1, 3);
    connection
        .commit(batch(
            1,
            vec![enter(a1, "=SEQUENCE(3)"), enter(c1, "=SUM(A1#)")],
        ))
        .unwrap();
    while host.advance() {}

    let deps = |at, dir| {
        let request = query::Deps {
            of: target(at),
            dir,
            depth: 1,
            group: DependencyGrouping::Reference,
        };
        let answer =
            query::Deps::decode(connection.ask(request.request(), BUDGET).unwrap()).unwrap();
        let Evidence::Derived { body, .. } = answer.body else {
            panic!("dependency graph")
        };
        body
    };
    let forwards = deps(c1, DependencyDirection::Precedents);
    assert!(
        forwards
            .edges
            .iter()
            .any(|edge| edge.kind == DependencyKind::Dynamic
                && forwards.nodes[edge.to]
                    == DependencyTarget::Source(SourceRef::Cells {
                        sheet,
                        areas: vec![Area(a1, key(3, 1))],
                    }))
    );
    let backwards = deps(key(2, 1), DependencyDirection::Dependents);
    for (graph, kind) in [
        (forwards, DependencyKind::Placed),
        (backwards, DependencyKind::Reader),
    ] {
        let [reference] = graph.references.as_slice() else {
            panic!("one explicit spill operand")
        };
        assert_eq!(reference.edge.kind, kind);
        assert_eq!(reference.spelling, "A1#");
        assert_eq!(reference.source, SourceRef::Spill { anchor: a1 });
        assert_eq!(reference.published_extent, Some(Area(a1, key(3, 1))));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dependency_query_retains_bound_table_selectors() {
    let (host, sheet) = workbook();
    let connection = port(&host);
    let key = |row, column| Key(sheet.0, sheet.1, row, column);
    connection
        .commit(batch(
            1,
            vec![
                enter(key(1, 1), "Region"),
                enter(key(1, 2), "Amount"),
                enter(key(2, 1), "West"),
                enter(key(2, 2), "10"),
                enter(key(3, 1), "East"),
                enter(key(3, 2), "20"),
            ],
        ))
        .unwrap();
    while host.advance() {}
    host.with_session(SelectionCause::Open, |session| {
        let revision = session.host().revision().get();
        let change = calc_host::TableChange::Create {
            area: Area(key(1, 1), key(3, 2)).area().unwrap(),
            name: "Orders".into(),
        };
        let receipt = session.dispatch(calc_host::SkinIntent::Command(
            calc_host::WorkbookCommand::Table { revision, change },
        ));
        assert!(receipt.accepted(), "{receipt:?}");
    });
    let report = key(1, 4);
    connection
        .commit(batch(
            2,
            vec![enter(
                report,
                r#"=SUMIFS(Orders[Amount],Orders[Region],"West")"#,
            )],
        ))
        .unwrap();
    while host.advance() {}
    let request = query::Deps {
        of: SourceRef::Cells {
            sheet,
            areas: vec![Area(report, report)],
        },
        dir: DependencyDirection::Precedents,
        depth: 1,
        group: DependencyGrouping::Reference,
    };

    let answer = query::Deps::decode(connection.ask(request.request(), BUDGET).unwrap()).unwrap();
    let Evidence::Derived { body: graph, .. } = answer.body else {
        panic!("dependency graph")
    };
    assert_eq!(graph.edges.len(), 2);
    assert_eq!(graph.references.len(), 2);
    for (spelling, column) in [("Orders[Amount]", 2), ("Orders[Region]", 1)] {
        let reference = graph
            .references
            .iter()
            .find(|reference| reference.spelling == spelling)
            .unwrap();
        assert_eq!(reference.edge.kind, DependencyKind::Placed);
        assert_eq!(
            reference.source,
            SourceRef::Cells {
                sheet,
                areas: vec![Area(key(2, column), key(3, column))],
            }
        );
        assert_eq!(reference.published_extent, None);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn issues_retain_distinct_error_origins_and_count_possible_affected_cells() {
    use calc_skin::query::{IssuesIn, Query};

    let (host, sheet) = workbook();
    let port = port(&host);
    let a1 = Key(sheet.0, sheet.1, 1, 1);
    let a2 = Key(sheet.0, sheet.1, 2, 1);
    let a3 = Key(sheet.0, sheet.1, 3, 1);
    let b1 = Key(sheet.0, sheet.1, 1, 2);
    port.commit(batch(
        1,
        vec![
            enter(a1, "=1/0"),
            enter(a2, "=A1"),
            enter(a3, "=A2+1"),
            enter(b1, "=NA()"),
        ],
    ))
    .unwrap();
    while host.advance() {}
    let query = IssuesIn {
        scope: SourceRef::Workbook,
    };
    let reply = IssuesIn::decode(port.ask(query.request(), BUDGET).unwrap()).unwrap();
    let Evidence::Derived { body, .. } = reply.body else {
        panic!("derived issue roots");
    };
    assert_eq!(
        body.roots,
        vec![
            ErrorRoot {
                at: a1,
                affected: 2
            },
            ErrorRoot {
                at: b1,
                affected: 0
            }
        ]
    );
    assert_eq!(
        body.errors[0],
        ErrorIssue {
            at: a1,
            code: "#DIV/0!".into(),
            origin: ErrorOrigin::RootCandidate,
            exact_root: Some(a1),
            named_errors: Vec::new(),
        }
    );
    for at in [a2, a3] {
        assert!(body.errors.contains(&ErrorIssue {
            at,
            code: "#DIV/0!".into(),
            origin: ErrorOrigin::CandidatePropagation { roots: vec![a1] },
            exact_root: (at == a2).then_some(a1),
            named_errors: Vec::new(),
        }));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn issues_report_only_names_that_produced_the_error_during_evaluation() {
    let (host, sheet) = workbook();
    let connection = port(&host);
    let direct = Key(sheet.0, sheet.1, 1, 1);
    let independent = Key(sheet.0, sheet.1, 2, 1);
    let stamp = connection
        .ask(WorkbookQuery::Summary, BUDGET)
        .unwrap()
        .stamp;
    let mut changes = batch(
        1,
        vec![
            Change::Model(ModelChange::Name {
                scope: NameScope::Workbook,
                change: NameChange::Define {
                    name: "Broken".into(),
                    formula: "=#REF!".into(),
                },
            }),
            enter(direct, "=Broken"),
            enter(independent, "=IF(FALSE,Broken,#REF!)"),
        ],
    );
    changes.base = Some(stamp.model);
    connection.commit(changes).unwrap();
    while host.advance() {}
    let request = query::IssuesIn {
        scope: SourceRef::Workbook,
    };
    let answer =
        query::IssuesIn::decode(connection.ask(request.request(), BUDGET).unwrap()).unwrap();
    let Evidence::Derived { body, .. } = answer.body else {
        panic!("worksheet error evidence")
    };
    assert_eq!(body.errors.len(), 2);
    let error = &body.errors[0];
    assert_eq!(error.at, direct);
    assert_eq!(error.code, "#REF!");
    let [name] = error.named_errors.as_slice() else {
        panic!("one executed definition")
    };
    assert_eq!(name.name, "Broken");
    assert_eq!(name.scope, NameScope::Workbook);
    assert_eq!(body.errors[1].at, independent);
    assert!(body.errors[1].named_errors.is_empty());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contributions_scan_every_position_and_use_the_formula_kernels_matches() {
    use calc_skin::query::{ContributionsAt, Query};

    let (host, sheet) = workbook();
    let port = port(&host);
    let key = |row, col| Key(sheet.0, sheet.1, row, col);
    let mut writes = Vec::new();
    for (row, region, status, amount) in [
        (1, "West", "Paid", "10"),
        (2, "East", "Paid", "17"),
        (3, "West", "Pending", "42"),
        (4, "East", "Pending", "99"),
    ] {
        writes.extend([
            enter(key(row, 1), region),
            enter(key(row, 2), status),
            enter(key(row, 3), amount),
        ]);
    }
    let at = key(1, 5);
    writes.push(enter(at, r#"=SUMIFS(C1:C5,A1:A5,"West",B1:B5,"Paid")"#));
    let committed = port.commit(batch(1, writes)).unwrap();
    while host.advance() {}
    let query = ContributionsAt { at };
    assert!(matches!(
        port.ask(query.request(), Budget { cells: 14, ..BUDGET }),
        Err(Refusal::OverBudget { counts, .. }) if counts.cells == 15
    ));
    let answer = ContributionsAt::decode(port.ask(query.request(), BUDGET).unwrap()).unwrap();
    let Evidence::Derived { body, .. } = answer.body else {
        panic!("derived contributions");
    };
    assert_eq!(body.inspected, 5);
    assert_eq!(body.source, Area(key(1, 3), key(5, 3)));
    assert_eq!(
        body.result,
        RawValue::Number {
            bits: 10.0f64.to_bits()
        }
    );
    assert_eq!(body.matching_count, 1);
    assert_eq!(body.matching, vec![Area(key(1, 3), key(1, 3))]);
    assert_eq!(body.criteria[0].matching, 2);
    assert_eq!(
        body.criteria[0].failing_only_this,
        vec![Area(key(2, 3), key(2, 3))]
    );
    assert_eq!(
        body.criteria[1].failing_only_this,
        vec![Area(key(3, 3), key(3, 3))]
    );
    assert_eq!(body.undetermined, 0);
    assert_eq!(answer.stamp.model, committed.model);
    assert_eq!(answer.stamp.calc, committed.calc);
}

fn contribution_fixture() -> (SkinHost, Sheet) {
    let (host, sheet) = workbook();
    let port = port(&host);
    let key = |row, column| Key(sheet.0, sheet.1, row, column);
    let changes = [
        (key(1, 1), "5"),
        (key(2, 1), "8"),
        (key(3, 1), "5"),
        (key(4, 1), "7"),
        (key(1, 2), "10"),
        (key(2, 2), "20"),
        (key(3, 2), "30"),
        (key(4, 2), "40"),
        (key(1, 4), "=FILTER(B1:B4,A1:A4=5)"),
        (key(1, 5), "=XLOOKUP(5,A1:A4,B1:B4,,0,-1)"),
        (key(6, 1), "5"),
        (key(6, 2), "7"),
        (key(6, 3), "5"),
        (key(6, 4), "8"),
        (key(7, 1), "10"),
        (key(7, 2), "20"),
        (key(7, 3), "30"),
        (key(7, 4), "40"),
        (key(8, 1), "=FILTER(A7:D7,A6:D6=5)"),
    ];
    port.commit(batch(
        1,
        changes
            .into_iter()
            .map(|(at, text)| enter(at, text))
            .collect(),
    ))
    .unwrap();
    while host.advance() {}

    (host, sheet)
}

fn contribution_at(host: &SkinHost, at: Key) -> Contributions {
    let query = query::ContributionsAt { at };
    let reply =
        query::ContributionsAt::decode(port(host).ask(query.request(), BUDGET).unwrap()).unwrap();
    let Evidence::Derived { body, .. } = reply.body else {
        panic!("a kernel contribution report")
    };
    body
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn filter_contributions_report_the_kernel_mask() {
    let (host, sheet) = contribution_fixture();
    let key = |row, column| Key(sheet.0, sheet.1, row, column);
    let filter = contribution_at(&host, key(1, 4));
    assert_eq!(filter.function, "FILTER");
    assert_eq!(filter.selected, None);
    assert_eq!((filter.inspected, filter.matching_count), (4, 2));
    assert_eq!(
        filter.matching,
        [Area(key(1, 2), key(1, 2)), Area(key(3, 2), key(3, 2))]
    );
    let column_filter = contribution_at(&host, key(8, 1));
    assert_eq!(column_filter.source, Area(key(7, 1), key(7, 4)));
    assert_eq!(
        column_filter.matching,
        [Area(key(7, 1), key(7, 1)), Area(key(7, 3), key(7, 3))]
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn lookup_contributions_distinguish_matches_and_the_selected_position() {
    let (host, sheet) = contribution_fixture();
    let key = |row, column| Key(sheet.0, sheet.1, row, column);
    let lookup = contribution_at(&host, key(1, 5));
    assert_eq!(lookup.function, "XLOOKUP");
    assert_eq!(lookup.selected, Some(Area(key(3, 2), key(3, 2))));
    assert_eq!((lookup.inspected, lookup.matching_count), (4, 2));
    assert_eq!(
        lookup.matching,
        [Area(key(1, 2), key(1, 2)), Area(key(3, 2), key(3, 2))]
    );
    assert_eq!(
        lookup.result,
        RawValue::Number {
            bits: 30f64.to_bits()
        }
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn anchored_note_uses_one_undo_entry_and_round_trips_as_presentation() {
    use calc_skin::query::{NotesOf, Query};

    let (host, sheet) = workbook();
    let connection = port(&host);
    let at = Key(sheet.0, sheet.1, 1, 1);
    let anchor = AnchorId(NonZeroU128::new(101).unwrap());
    let note = NoteId(NonZeroU128::new(102).unwrap());
    let text = "Check <West> & \"Paid\"\r\nbefore sign-off. _x0020_";
    let edit = batch(
        1,
        vec![
            Change::Presentation(PresentationChange::Bind {
                anchor,
                source: target(at),
            }),
            Change::Presentation(PresentationChange::PutNote {
                id: note,
                anchor,
                text: text.into(),
            }),
        ],
    );
    let preview = connection.preview(&edit, PreviewDepth::WritesOnly).unwrap();
    assert_eq!(preview.kind(), Some(ChangeKind::Presentation));
    assert!(preview.writes.is_empty());
    let added = connection.commit(edit).unwrap();
    assert_eq!(added.model, preview.base.model);
    assert!(added.view > preview.base.view);

    let query = NotesOf {
        anchor: Some(anchor),
    };
    let read = |connection: &dyn WorkbookPort| {
        NotesOf::decode(connection.ask(query.request(), BUDGET).unwrap())
            .unwrap()
            .body
    };
    assert_eq!(
        read(&connection),
        vec![AnchoredNote {
            id: note,
            anchor,
            text: text.into(),
            source: Some(target(at)),
        }]
    );
    connection.run(Operation::Undo).unwrap();
    assert!(read(&connection).is_empty());
    connection.run(Operation::Redo).unwrap();
    assert_eq!(read(&connection)[0].text, text);

    host.with_session(SelectionCause::History, |session| {
        use calc_host::{Axis, AxisChange, AxisEdit, SkinIntent};
        let change = AxisEdit::new(sheet.id(), Axis::Rows, AxisChange::Insert, 1, 1).unwrap();
        let result = session.dispatch(SkinIntent::EditAxis(change));
        assert!(result.error.is_none(), "{:?}", result.error);
    });
    let moved = Key(sheet.0, sheet.1, 2, 1);
    assert_eq!(read(&connection)[0].source, Some(target(moved)));

    let saved = host.with_session(SelectionCause::Open, |session| {
        let mut bytes = Vec::new();
        session
            .save(&calc_host::SavePolicy::default(), &mut bytes)
            .unwrap();
        bytes
    });
    let opened = SkinHost::new(
        HostSession::open(
            Arc::new(MemorySource::new(saved)),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap(),
    );
    let reopened_connection = port(&opened);
    let reopened = read(&reopened_connection);
    assert_eq!(reopened.len(), 1);
    assert_eq!(reopened[0].id, note);
    assert_eq!(reopened[0].text, text);
    assert!(
        matches!(&reopened[0].source, Some(SourceRef::Cells { areas, .. }) if areas[0].0.2 == 2 && areas[0].0.3 == 1)
    );

    reopened_connection
        .commit(batch(
            2,
            vec![Change::Presentation(PresentationChange::RemoveAnchor {
                anchor,
            })],
        ))
        .unwrap();
    assert_eq!(read(&reopened_connection)[0].source, None);
    reopened_connection
        .commit(batch(
            3,
            vec![Change::Presentation(PresentationChange::RemoveNote {
                id: note,
            })],
        ))
        .unwrap();
    assert!(read(&reopened_connection).is_empty());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn the_view_part_answer_decodes_to_its_query_type() {
    use calc_skin::query::{AnchorFacts, Query, ViewPartOf};

    let (host, sheet) = workbook();
    let connection = port(&host);
    let at = Key(sheet.0, sheet.1, 3, 2);
    let anchor = AnchorId(NonZeroU128::new(7).unwrap());
    connection
        .commit(batch(
            1,
            vec![Change::Presentation(PresentationChange::Bind {
                anchor,
                source: target(at),
            })],
        ))
        .unwrap();
    let part = ViewPartOf::decode(connection.ask(ViewPartOf.request(), BUDGET).unwrap())
        .unwrap()
        .body;
    assert!(!part.opaque);
    assert_eq!(
        part.anchors,
        vec![AnchorFacts {
            id: anchor,
            source: Some(target(at)),
            area: Some(Area(at, at)),
        }]
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_view_document_round_trips_without_editing_the_workbook_model() {
    use calc_skin::query::ViewPartOf;

    let (host, _) = workbook();
    let connection = port(&host);
    let doc = DocId(NonZeroU128::new(121).unwrap());
    let body = serde_json::json!({"prose": "# West & East\n\n<Notes> ]]> _x0020_\r\n"});
    let edit = batch(
        1,
        vec![Change::Presentation(PresentationChange::CreateDoc {
            doc,
            expect_view_rev: None,
            view: ViewId("notebook".into()),
            sheet: None,
            schema: 1,
            body: body.clone(),
        })],
    );
    let preview = connection.preview(&edit, PreviewDepth::WritesOnly).unwrap();
    assert_eq!(
        preview.kind(),
        Some(ChangeKind::Presentation),
        "{preview:?}"
    );
    let committed = connection.commit(edit).unwrap();
    assert_eq!(committed.model, preview.base.model);
    assert!(committed.view > preview.base.view);

    let read = |connection: &dyn WorkbookPort| {
        ViewPartOf::decode(connection.ask(ViewPartOf.request(), BUDGET).unwrap())
            .unwrap()
            .body
    };
    assert_eq!(read(&connection).documents[0].body, body);
    let patch = |id, expect_view_rev| {
        batch(
            id,
            vec![Change::Presentation(PresentationChange::PatchDoc {
                doc,
                expect_view_rev,
                body: serde_json::json!({"prose": "Updated"}),
            })],
        )
    };
    assert!(connection.commit(patch(2, preview.base.view)).is_err());
    let patched = connection.commit(patch(3, committed.view)).unwrap();
    assert_eq!(patched.model, committed.model);
    assert!(patched.view > committed.view);

    let before = read(&connection);
    assert_eq!(
        before.documents[0].body,
        serde_json::json!({"prose": "Updated"})
    );
    connection.run(Operation::Undo).unwrap();
    assert_eq!(read(&connection).documents[0].body, body);
    connection.run(Operation::Undo).unwrap();
    assert!(read(&connection).documents.is_empty());
    connection.run(Operation::Redo).unwrap();

    let saved = host.with_session(SelectionCause::Open, |session| {
        let mut bytes = Vec::new();
        session
            .save(&calc_host::SavePolicy::default(), &mut bytes)
            .unwrap();
        bytes
    });
    let reopened = SkinHost::new(
        HostSession::open(
            Arc::new(MemorySource::new(saved)),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap(),
    );
    let reopened_port = port(&reopened);
    assert_eq!(read(&reopened_port).documents[0].body, body);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn expression_structure_reads_live_let_bindings_through_the_workbook() {
    use calc_skin::query::ExpressionTree;

    let (host, sheet) = workbook();
    let port = port(&host);
    let at = |column| Key(sheet.0, sheet.1, 1, column);
    port.commit(batch(
        1,
        vec![enter(at(1), "2"), enter(at(2), "=LET(q,A1*3,q+1)")],
    ))
    .unwrap();
    while host.advance() {}
    let query = ExpressionTree { at: at(2) };
    let read = || {
        let answer = ExpressionTree::decode(port.ask(query.request(), BUDGET).unwrap()).unwrap();
        let Evidence::Derived {
            from: Derivation::ExpressionStructure,
            body,
            ..
        } = answer.body
        else {
            panic!("authoritative LET expression structure is missing");
        };
        body.steps
            .iter()
            .find(|step| step.text == "q+1")
            .unwrap_or_else(|| panic!("{body:?}"))
            .value
            .clone()
    };
    assert_eq!(read(), "7");
    port.commit(batch(2, vec![enter(at(1), "4")])).unwrap();
    while host.advance() {}
    assert_eq!(read(), "13");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn opening_an_automatic_workbook_publishes_calculated_spills() {
    let (source, sheet) = workbook();
    let anchor = Key(sheet.0, sheet.1, 1, 1);
    port(&source)
        .commit(batch(1, vec![enter(anchor, "=SEQUENCE(3)")]))
        .unwrap();
    while source.advance() {}
    let saved = source.with_session(SelectionCause::Open, |session| {
        let mut bytes = Vec::new();
        session
            .save(&calc_host::SavePolicy::default(), &mut bytes)
            .unwrap();
        bytes
    });

    let (host, _) = workbook();
    let connection = host.connect(
        Origin::User {
            view: ViewId("notebook".into()),
        },
        WritePolicy::Commit,
    );
    let opened = connection.handle(Packet {
        envelope: Envelope::new(
            1,
            Message::WorkbookRequest(WorkbookRequest::Open {
                name: "Opened spill.xlsx".into(),
            }),
        ),
        bytes: Some(saved),
    });
    assert!(matches!(
        opened.envelope.message,
        Message::WorkbookReply(WorkbookReply::Stamp(_))
    ));
    while host.advance() {}
    host.with_session(SelectionCause::Open, |session| {
        assert!(
            session
                .intent_log()
                .iter()
                .all(|entry| !matches!(entry.intent, calc_host::SkinIntent::Recalculate)),
            "opening a file preserves its automatic scheduling and does not issue Calculate Now"
        );
    });
    let query = query::Rows {
        source: SourceRef::Spill { anchor },
        filter: vec![],
        sort: vec![],
        window: RowWindow {
            offset: 0,
            limit: 3,
        },
    };
    let answer = query::Rows::decode(connection.ask(query.request(), BUDGET).unwrap()).unwrap();
    assert_eq!(answer.body.source_rows, 3);
    let cell = &answer.body.rows[2].cells[0];
    assert_eq!(cell.text, "3");
    assert!(matches!(
        cell.shown.provenance,
        Provenance::Calculated { .. }
    ));
}
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn local_rtd_clock_starts_paused_with_the_calculated_opening_tick() {
    use std::time::Duration;

    let (host, sheet) = workbook();
    host.enable_model_clock();
    let port = port(&host);
    let at = |column| Key(sheet.0, sheet.1, 1, column);
    port.commit(batch(
        1,
        vec![
            enter(at(2), "2"),
            enter(at(3), "FALSE"),
            enter(at(4), "0"),
            enter(at(5), "0"),
            enter(at(6), "TRUE"),
            enter(
                at(1),
                r#"=RTD("DnaCalc.Clock",,"stock","tick",B1,AND(C1,F1),D1,E1)"#,
            ),
        ],
    ))
    .unwrap();
    while host.advance() {}

    let read = query::Rows {
        source: target(at(1)),
        filter: Vec::new(),
        sort: Vec::new(),
        window: RowWindow {
            offset: 0,
            limit: 1,
        },
    };
    let read = || query::Rows::decode(port.ask(read.request(), BUDGET).unwrap()).unwrap();
    let opening = read();
    let actual = &opening.body.rows[0].cells[0].shown;
    assert_eq!(
        actual.value,
        RawValue::Number {
            bits: 0.0f64.to_bits()
        }
    );
    assert!(matches!(actual.provenance, Provenance::Calculated { .. }));
    let value = |n: f64| RawValue::Number { bits: n.to_bits() };
    let tick = || read().body.rows[0].cells[0].shown.value.clone();
    assert!(!host.advance_model_clock(Duration::from_secs(1)));
    assert_eq!(read().stamp, opening.stamp);

    port.commit(batch(2, vec![enter(at(3), "TRUE")])).unwrap();
    while host.advance() {}
    let running = read().stamp;
    assert!(!host.advance_model_clock(Duration::from_millis(499)));
    assert!(host.advance_model_clock(Duration::from_millis(1)));
    while host.advance() {}
    assert_eq!(tick(), value(1.0));
    assert_eq!(read().stamp.model, running.model);
    assert!(read().stamp.calc.0 > running.calc.0);

    port.commit(batch(3, vec![enter(at(6), "FALSE")])).unwrap();
    while host.advance() {}
    assert!(!host.advance_model_clock(Duration::from_secs(1)));
    assert_eq!(tick(), value(1.0));

    port.commit(batch(4, vec![enter(at(3), "FALSE"), enter(at(4), "1")]))
        .unwrap();
    while host.advance() {}
    assert_eq!(tick(), value(2.0));
    assert!(!host.advance_model_clock(Duration::from_secs(1)));

    port.commit(batch(5, vec![enter(at(5), "1")])).unwrap();
    while host.advance() {}
    assert_eq!(tick(), value(0.0));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn binding_snapshot_reads_calculated_outputs_at_one_workbook_generation() {
    let (host, sheet) = workbook();
    let port = port(&host);
    let at = |column| Key(sheet.0, sheet.1, 1, column);
    let mut changes = vec![enter(at(1), "2"), enter(at(2), "=A1*3")];
    let reads = (1..=2)
        .map(|column| {
            let anchor = AnchorId(NonZeroU128::new(u128::from(column)).unwrap());
            changes.push(Change::Presentation(PresentationChange::Bind {
                anchor,
                source: target(at(column)),
            }));
            BindingRead {
                bound: Bound {
                    anchor,
                    address: format!("Sheet1!{}1", char::from(b'A' + column as u8 - 1)),
                },
                kind: BindingKind::Value,
            }
        })
        .collect();
    port.commit(batch(1, changes)).unwrap();
    while host.advance() {}
    let query = query::ConsistentBindings { reads };
    let read = |budget| {
        query::ConsistentBindings::decode(port.ask(query.request(), budget).unwrap()).unwrap()
    };
    let values = |answer: &Answer<BindingSnapshot>| {
        answer.body.items.iter()
            .map(|item| {
                let BindingValue::Cell(cell) = item.value.as_ref().unwrap() else {
                    panic!("expected a cell");
                };
                cell.shown.value.clone()
            })
            .collect::<Vec<_>>()
    };
    let value = |n: f64| RawValue::Number { bits: n.to_bits() };
    let opening = read(BUDGET);
    assert_eq!(opening.coverage, Coverage::Complete);
    assert!(opening.body.captured_unix_ms.is_some());
    assert_eq!(values(&opening), [value(2.0), value(6.0)]);
    let limited = read(Budget { cells: 1, ..BUDGET });
    assert_eq!(limited.stamp, opening.stamp);
    assert!(matches!(limited.body.items[0].value, Ok(BindingValue::Cell(_))));
    assert!(matches!(limited.body.items[1].value, Err(Refusal::OverBudget { .. })));
    port.commit(batch(2, vec![enter(at(1), "5")])).unwrap();
    while host.advance() {}
    let current = read(BUDGET);
    assert!(current.stamp.calc.0 > opening.stamp.calc.0);
    assert_eq!(values(&current), [value(5.0), value(15.0)]);
    assert_eq!(values(&opening), [value(2.0), value(6.0)]);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn an_agent_look_reaches_the_operator_without_editing_the_workbook() {
    let (host, _) = workbook();
    let operator = port(&host);
    let before = operator.next_frame().unwrap().unwrap().stamp;
    let agent = host.connect(
        Origin::Agent {
            agent: "audit assistant".into(),
            proposal: "calm-classic".into(),
        },
        WritePolicy::Propose,
    );
    let skin = serde_json::from_value(serde_json::json!({
        "schema": "dna.skin/1",
        "id": "calm-classic",
        "name": "Calm Classic",
        "theme": {
            "base": "cockpit-dark",
            "density": "working",
            "overrides": { "formula.font.size": "15.5px" }
        }
    })).unwrap();
    let look = TryOnPayload::Skin(Box::new(skin));
    let request = Operation::TryOn {
        expected: before,
        look: look.clone(),
    };
    let reply = agent.handle(Packet {
        envelope: Envelope::new(1, Message::WorkbookRequest(WorkbookRequest::Run(request.clone()))),
        bytes: None,
    });
    assert_eq!(reply.envelope.message, Message::WorkbookReply(WorkbookReply::Stamp(before)));
    assert_eq!(operator.run(request), Err(Refusal::PolicyRefused));
    let frame = operator.next_frame().unwrap().expect("operator receives the look");
    assert_eq!(frame.stamp, before);
    assert!(frame.events.contains(&Event::TryOn(LookOffer {
        at: before,
        agent: "audit assistant".into(),
        proposal: "calm-classic".into(),
        look: look.clone(),
    })));
    assert!(operator.next_frame().unwrap().is_none());

    let history = query::History::decode(operator.ask(query::History.request(), BUDGET).unwrap())
        .unwrap().body;
    assert!(history.undo.is_empty());

    let joined = port(&host);
    assert!(joined.next_frame().unwrap().unwrap().events.iter()
        .all(|event| !matches!(event, Event::TryOn(_))));
    let TryOnPayload::Skin(mut malformed) = look else { panic!("expected a skin") };
    malformed.pages.insert("unbound".into(), Page {
        schema: PageSchema::V1,
        title: "Unbound page".into(),
        input: InputPolicy::Live,
        root: PageNode::Part {
            id: "unbound".into(),
            component: "value".into(),
            bind: [("value".into(), Bound {
                anchor: AnchorId(NonZeroU128::new(991).unwrap()),
                address: "Sheet1!A1".into(),
            })].into(),
            props: Default::default(),
        },
    });
    assert!(agent.run(Operation::TryOn {
        expected: before,
        look: TryOnPayload::Skin(malformed),
    }).is_err());
    assert!(operator.next_frame().unwrap().is_none());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn preview_effects_calculate_the_candidate_in_isolation() {
    let (host, sheet) = workbook();
    let connection = port(&host);
    let assumption = Key(sheet.0, sheet.1, 1, 1);
    let output = Key(sheet.0, sheet.1, 1, 2);
    connection.commit(batch(1, vec![enter(assumption, "3"), enter(output, "=A1*2")])).unwrap();
    while host.advance() {}
    let before = connection.next_frame().unwrap().unwrap().stamp;
    let candidate = batch(2, vec![enter(assumption, "4")]);
    let agent = host.connect(
        Origin::Agent { agent: "Planner assistant".into(), proposal: "assumption".into() },
        WritePolicy::Propose,
    );
    let preview = agent.preview(&candidate, PreviewDepth::WithEffects { budget: BUDGET }).unwrap();
    assert_eq!(agent.commit(candidate), Err(Refusal::PolicyRefused));
    assert!(preview.refusal.is_none(), "{preview:?}");
    let effect = preview.effects.iter().find(|effect| effect.at == output).unwrap();
    assert_eq!(effect.before.value, RawValue::Number { bits: 6.0f64.to_bits() });
    assert_eq!(effect.after.value, RawValue::Number { bits: 8.0f64.to_bits() });
    assert!(matches!(effect.before.provenance, Provenance::Calculated { .. }));
    assert!(matches!(effect.after.provenance, Provenance::Calculated { .. }));
    let live = ask(&connection, query::Cells { interest: Interest::Area(Area(output, output)) });
    assert_eq!(live.cells[0].shown.value, RawValue::Number { bits: 6.0f64.to_bits() });
    assert_eq!(preview.base, before);
    assert!(connection.next_frame().unwrap().is_none());
    assert_eq!(ask(&connection, query::History).undo.len(), 1);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn preview_effects_refuses_volatile_dependencies_and_enforces_the_budget() {
    let (host, sheet) = workbook();
    let connection = port(&host);
    let input = Key(sheet.0, sheet.1, 1, 1);
    let output = Key(sheet.0, sheet.1, 1, 2);
    let volatile = Key(sheet.0, sheet.1, 1, 3);
    connection.commit(batch(1, vec![enter(input, "3"), enter(output, "=A1+C1"), enter(volatile, "=RAND()")])).unwrap();
    while host.advance() {}
    let before = connection.ask(WorkbookQuery::Summary, BUDGET).unwrap().stamp;
    let candidate = batch(2, vec![enter(input, "4")]);
    let preview = connection.preview(&candidate, PreviewDepth::WithEffects { budget: BUDGET }).unwrap();
    assert!(matches!(&preview.refusal, Some(Refusal::NotModelled { owner, what })
        if owner == "dc-zh096" && what.contains("volatile")), "{preview:?}");
    assert_eq!(preview.writes.len(), 1);
    assert!(preview.effects.is_empty());
    let preview = connection.preview(&candidate, PreviewDepth::WithEffects {
        budget: Budget { cells: 0, ..BUDGET },
    }).unwrap();
    assert!(matches!(&preview.refusal, Some(Refusal::OverBudget { .. })), "{preview:?}");
    assert_eq!(connection.ask(WorkbookQuery::Summary, BUDGET).unwrap().stamp, before);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn preview_effects_can_leave_an_unrelated_volatile_formula_outside_the_trial() {
    let (host, sheet) = workbook();
    let connection = port(&host);
    let input = Key(sheet.0, sheet.1, 1, 1);
    let output = Key(sheet.0, sheet.1, 1, 2);
    let unrelated = Key(sheet.0, sheet.1, 1, 26);
    connection.commit(batch(1, vec![
        enter(input, "3"),
        enter(output, "=A1*2"),
        enter(unrelated, "=RAND()"),
    ])).unwrap();
    while host.advance() {}
    let volatile = || ask(&connection, query::Cells {
        interest: Interest::Area(Area(unrelated, unrelated))
    }).cells[0].shown.clone();
    let before = volatile();
    let preview = connection.preview(&batch(2, vec![enter(input, "4")]),
        PreviewDepth::WithEffects { budget: BUDGET }).unwrap();
    assert!(preview.refusal.is_none(), "{preview:?}");
    assert_eq!(preview.effects.iter().map(|item| item.at).collect::<Vec<_>>(), [output]);
    assert_eq!(volatile(), before);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn preview_effects_include_new_spill_members() {
    let (host, sheet) = workbook();
    let connection = port(&host);
    let input = Key(sheet.0, sheet.1, 1, 1);
    let anchor = Key(sheet.0, sheet.1, 1, 2);
    let new_member = Key(sheet.0, sheet.1, 3, 2);
    connection.commit(batch(1, vec![enter(input, "2"), enter(anchor, "=SEQUENCE(A1,1)")])).unwrap();
    while host.advance() {}
    let candidate = batch(2, vec![enter(input, "3")]);
    let preview = connection.preview(&candidate, PreviewDepth::WithEffects { budget: BUDGET }).unwrap();
    assert!(preview.refusal.is_none(), "{preview:?}");
    let member = preview.effects.iter().find(|effect| effect.at == new_member).unwrap();
    assert_eq!(member.before.value, RawValue::Blank);
    assert_eq!(member.after.value, RawValue::Number { bits: 3.0f64.to_bits() });
    assert!(matches!(member.after.provenance, Provenance::Calculated { .. }));
    let live = ask(&connection, query::Cells { interest: Interest::Area(Area(new_member, new_member)) });
    assert!(live.cells.is_empty());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn agent_proposal_accepts_only_the_reviewed_subset_and_undoes_each_unit() {
    let (host, sheet) = workbook();
    let operator = host.connect(
        Origin::User {
            view: ViewId("classic".into()),
        },
        WritePolicy::Commit,
    );
    let agent = host.connect(
        Origin::Agent {
            agent: "Jo".into(),
            proposal: "loan-assumptions".into(),
        },
        WritePolicy::Propose,
    );
    let principal = Key(sheet.0, sheet.1, 2, 2);
    let rate = Key(sheet.0, sheet.1, 3, 2);
    let term = Key(sheet.0, sheet.1, 4, 2);
    let output = Key(sheet.0, sheet.1, 6, 2);
    operator
        .commit(batch(
            1,
            vec![
                enter(principal, "30"),
                enter(rate, "2"),
                enter(term, "1"),
                enter(output, "=B2*B3/B4"),
            ],
        ))
        .unwrap();
    while host.advance() {}
    let stamp = operator.ask(WorkbookQuery::Summary, BUDGET).unwrap().stamp;
    let watch = operator.watch(WorkbookQuery::Proposals, BUDGET).unwrap();
    operator.next_frame().unwrap().unwrap();
    let unit = |id: &str, need: Option<&str>, at, text: &str| {
        let mut batch = batch(1, vec![enter(at, text)]);
        batch.expects = vec![target(at)];
        ProposalUnit {
            id: UnitId(id.into()),
            needs: need.into_iter().map(|name| UnitId(name.into())).collect(),
            batch,
        }
    };
    agent
        .run(Operation::Propose(Proposal {
            at: stamp,
            intent: "Review planner assumptions".into(),
            units: vec![
                unit("principal", None, principal, "40"),
                unit("rate", Some("principal"), rate, "3"),
                unit("term", None, term, "2"),
            ],
        }))
        .unwrap();
    assert_eq!(
        operator.ask(WorkbookQuery::Summary, BUDGET).unwrap().stamp,
        stamp
    );
    assert_eq!(ask(&operator, query::Proposals).len(), 1);
    let offered = operator.next_frame().unwrap().unwrap();
    assert_eq!(offered.stamp, stamp);
    assert!(offered.updates.iter().any(|update| matches!(update,
        Update::Ready { watch: id, .. } if *id == watch
    )));
    let request = |name: &str| UnitId(name.into());
    let proposal = ProposalId("loan-assumptions".into());
    let decide = |accept: Vec<UnitId>, reject: Vec<UnitId>| Operation::Decide {
        proposal: proposal.clone(),
        accept,
        reject,
    };
    assert!(
        operator
            .run(decide(vec![request("rate")], Vec::new()))
            .is_err()
    );
    assert_eq!(
        operator.ask(WorkbookQuery::Summary, BUDGET).unwrap().stamp,
        stamp
    );
    let packet = operator.handle(Packet {
        envelope: Envelope::new(
            1,
            Message::WorkbookRequest(WorkbookRequest::PreviewProposal {
                proposal: proposal.clone(),
                units: vec![request("rate"), request("principal")],
                depth: PreviewDepth::WithEffects { budget: BUDGET },
            }),
        ),
        bytes: None,
    });
    let Message::WorkbookReply(WorkbookReply::Preview(preview)) = packet.envelope.message else {
        panic!("expected a preview");
    };
    assert!(preview.refusal.is_none(), "{preview:?}");
    assert_eq!(
        preview
            .writes
            .iter()
            .map(|write| write.at)
            .collect::<Vec<_>>(),
        [principal, rate]
    );
    assert_eq!(
        preview
            .effects
            .iter()
            .find(|effect| effect.at == output)
            .unwrap()
            .after
            .value,
        RawValue::Number {
            bits: 120.0f64.to_bits()
        }
    );
    operator
        .run(decide(
            vec![request("rate"), request("principal")],
            vec![request("term")],
        ))
        .unwrap();
    while host.advance() {}
    let value = |at| {
        ask(
            &operator,
            query::Cells {
                interest: Interest::Area(Area(at, at)),
            },
        )
        .cells[0]
            .shown
            .value
            .clone()
    };
    assert_eq!(
        value(term),
        RawValue::Number {
            bits: 1.0f64.to_bits()
        }
    );
    assert_eq!(
        value(output),
        RawValue::Number {
            bits: 120.0f64.to_bits()
        }
    );
    let history = ask(&operator, query::History);
    assert_eq!(history.undo.len(), 3);
    assert!(matches!(history.undo[0].origin, Origin::Agent { .. }));
    assert_eq!(history.undo[0].group, history.undo[1].group);
    assert!(history.undo[0].group.is_some());
    operator.run(Operation::Undo).unwrap();
    while host.advance() {}
    assert_eq!(
        value(output),
        RawValue::Number {
            bits: 80.0f64.to_bits()
        }
    );
    operator.run(Operation::Undo).unwrap();
    while host.advance() {}
    assert_eq!(
        value(output),
        RawValue::Number {
            bits: 60.0f64.to_bits()
        }
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn stale_proposal_refuses_every_unit_and_returns_reviewed_and_current_inputs() {
    let (host, sheet) = workbook();
    let operator = port(&host);
    let agent = host.connect(
        Origin::Agent {
            agent: "Jo".into(),
            proposal: "shared-assumptions".into(),
        },
        WritePolicy::Propose,
    );
    let a = Key(sheet.0, sheet.1, 1, 1);
    let b = Key(sheet.0, sheet.1, 1, 2);
    let c = Key(sheet.0, sheet.1, 1, 3);
    operator
        .commit(batch(1, vec![enter(a, "1"), enter(b, "2"), enter(c, "3")]))
        .unwrap();
    let exposed = operator.ask(WorkbookQuery::Summary, BUDGET).unwrap().stamp;
    let unit = |id: &str, at, text: &str| {
        let mut batch = batch(1, vec![enter(at, text)]);
        batch.expects = vec![target(at)];
        ProposalUnit {
            id: UnitId(id.into()),
            needs: Vec::new(),
            batch,
        }
    };
    let proposal = ProposalId("shared-assumptions".into());
    agent
        .run(Operation::Propose(Proposal {
            at: exposed,
            intent: "Review three inputs".into(),
            units: vec![unit("a", a, "10"), unit("b", b, "20"), unit("c", c, "30")],
        }))
        .unwrap();
    operator.commit(batch(2, vec![enter(b, "25"), enter(c, "35")])).unwrap();
    let before_decide = operator.ask(WorkbookQuery::Summary, BUDGET).unwrap().stamp;
    let decision = Operation::Decide {
        proposal,
        accept: vec![UnitId("a".into()), UnitId("b".into())],
        reject: Vec::new(),
    };
    assert_eq!(agent.run(decision.clone()), Err(Refusal::PolicyRefused));
    let Err(Refusal::Conflict {
        base,
        conflicts,
        diffs,
    }) = operator.run(decision)
    else {
        panic!("expected both units to be refused for an intervening cell edit");
    };
    assert_eq!(base, exposed.model);
    assert_eq!(conflicts, vec![Target::Source(target(b))]);
    assert_eq!(
        diffs,
        vec![ModelWriteDiff {
            at: b,
            before: Some("2".into()),
            after: Some("25".into())
        }]
    );
    let first = ask(
        &operator,
        query::Cells {
            interest: Interest::Area(Area(a, a)),
        },
    );
    assert_eq!(first.cells[0].input, "1");
    assert_eq!(
        operator.ask(WorkbookQuery::Summary, BUDGET).unwrap().stamp,
        before_decide
    );
    assert_eq!(ask(&operator, query::History).undo.len(), 2);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn proposal_trail_records_decisions_and_undo_in_the_saved_view_part() {
    let intent = "Check revised\nassumptions \"<xml>\" _x0041_ \u{0001}";
    let (host, sheet) = workbook();
    let operator = port(&host);
    let agent = host.connect(
        Origin::Agent {
            agent: "Planner assistant".into(),
            proposal: "adjust-inputs".into(),
        },
        WritePolicy::Propose,
    );
    let a = Key(sheet.0, sheet.1, 1, 1);
    let b = Key(sheet.0, sheet.1, 1, 2);
    operator
        .commit(batch(1, vec![enter(a, "3"), enter(b, "4")]))
        .unwrap();
    let at = operator.ask(WorkbookQuery::Summary, BUDGET).unwrap().stamp;
    let unit = |name: &str, at, text: &str| ProposalUnit {
        id: UnitId(name.into()),
        needs: Vec::new(),
        batch: batch(1, vec![enter(at, text)]),
    };
    agent
        .run(Operation::Propose(Proposal {
            at,
            intent: intent.into(),
            units: vec![unit("first", a, "10"), unit("second", b, "11")],
        }))
        .unwrap();
    operator
        .run(Operation::Decide {
            proposal: ProposalId("adjust-inputs".into()),
            accept: vec![UnitId("first".into())],
            reject: vec![UnitId("second".into())],
        })
        .unwrap();
    host.with_session(SelectionCause::Open, |session| {
        let receipt = session.dispatch(calc_host::SkinIntent::Edit {
            edits: vec![calc_host::CellEdit::Clear {
                area: calc_host::value::GridArea::cell(b.cell().unwrap()),
            }],
        });
        assert!(receipt.error.is_none());
    });
    operator.run(Operation::Undo).unwrap();
    assert_eq!(ask(&operator, query::ViewPartOf).trail.events.len(), 2);
    operator.run(Operation::Undo).unwrap();
    let view = ask(&operator, query::ViewPartOf);
    assert!(view.trail.enabled);
    assert_eq!(
        view.trail
            .events
            .iter()
            .map(|item| item.outcome)
            .collect::<Vec<_>>(),
        [
            TrailOutcome::Accepted,
            TrailOutcome::Rejected,
            TrailOutcome::Undone
        ]
    );
    assert_eq!(view.trail.events[2].unit, UnitId("first".into()));
    assert_eq!(view.trail.events[2].agent, "Planner assistant");
    assert_eq!(view.trail.events[2].intent, intent);
    assert!(!view.trail.events[2].targets.is_empty());
    assert!(
        view.trail
            .events
            .iter()
            .all(|item| item.timestamp_millis > 0)
    );

    let released = host.with_session(SelectionCause::Open, |session| {
        let mut bytes = Vec::new();
        session
            .save(&calc_host::SavePolicy::default(), &mut bytes)
            .unwrap();
        bytes
    });
    let reopened = SkinHost::new(
        HostSession::open(
            Arc::new(MemorySource::new(released)),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap(),
    );
    assert_eq!(ask(&port(&reopened), query::ViewPartOf).trail, view.trail);
    reopened.with_session(SelectionCause::Open, |session| {
        let source = session.host().model().view_part().source().unwrap();
        assert!(!source.contains(&1), "The saved XML contains U+0001");
        assert!(std::str::from_utf8(source).unwrap().contains("_x0001_"));
    });

    operator
        .commit(batch(
            2,
            vec![Change::Presentation(PresentationChange::Trail {
                enabled: false,
            })],
        ))
        .unwrap();
    let at = operator.ask(WorkbookQuery::Summary, BUDGET).unwrap().stamp;
    let second = host.connect(
        Origin::Agent {
            agent: "Planner assistant".into(),
            proposal: "disabled".into(),
        },
        WritePolicy::Propose,
    );
    second
        .run(Operation::Propose(Proposal {
            at,
            intent: "Try a later input".into(),
            units: vec![unit("later", a, "5")],
        }))
        .unwrap();
    operator
        .run(Operation::Decide {
            proposal: ProposalId("disabled".into()),
            accept: vec![UnitId("later".into())],
            reject: Vec::new(),
        })
        .unwrap();
    let disabled = ask(&operator, query::ViewPartOf).trail;
    assert!(!disabled.enabled);
    assert_eq!(disabled.events, view.trail.events);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn view_trail_caps_records_and_counts_the_earliest_evictions() {
    use calc_doc_model::{TRAIL_LIMIT, ViewTrail, ViewTrailEvent, ViewTrailOutcome};
    let event = |i: usize| ViewTrailEvent {
        timestamp_millis: i as u64 + 1,
        agent: "Planner assistant".into(),
        proposal: "adjust-inputs".into(),
        unit: i.to_string(),
        intent: "Check revised assumptions".into(),
        targets: vec!["Sheet1!A1".into()],
        outcome: ViewTrailOutcome::Accepted,
    };
    let mut trail = ViewTrail::default();
    trail.record((0..=TRAIL_LIMIT).map(event));
    assert_eq!(trail.dropped, 1);
    assert_eq!(trail.events().len(), TRAIL_LIMIT);
    assert_eq!(trail.events().first().unwrap().unit, "1");
    trail.enabled = false;
    trail.record([event(TRAIL_LIMIT + 1)]);
    assert_eq!(trail.dropped, 1);
    assert_eq!(trail.events().last().unwrap().unit, TRAIL_LIMIT.to_string());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn iteration_reports_actual_pass_count_and_terminal_residual() {
    let (host, sheet) = workbook();
    let connection = port(&host);
    let at = Key(sheet.0, sheet.1, 1, 1);
    let peer = Key(sheet.0, sheet.1, 1, 2);
    host.with_session(SelectionCause::Open, |session| {
        let receipt = session.dispatch(calc_host::SkinIntent::Edit {
            edits: vec![calc_host::CellEdit::SetIteration(calc_host::IterationSettings::new(true, 3, 0.01))],
        });
        assert!(receipt.accepted(), "{receipt:?}");
    });
    connection.commit(batch(1, vec![enter(at, "=(B1+1)/2"), enter(peer, "=A1")])).unwrap();
    while host.advance() {}
    let q = query::Cells { interest: Interest::Area(Area(at, at)) };
    let answer = query::Cells::decode(connection.ask(q.request(), BUDGET).unwrap()).unwrap();
    assert_eq!(answer.body.cells[0].shown.value, RawValue::Number { bits: 0.875f64.to_bits() });
    let request = query::CalcIteration;
    let answer = query::CalcIteration::decode(connection.ask(request.request(), BUDGET).unwrap()).unwrap();
    let Evidence::Observed { body, .. } = answer.body else { panic!("observed iteration") };
    assert_eq!(body.passes, 3);
    assert_eq!(body.residual_bits, Some(0.125f64.to_bits()));
    assert_eq!(body.max_change_bits, 0.01f64.to_bits());
    assert_eq!(body.stop, IterationStop::MaximumIterations);
    assert_eq!(body.members, [at, peer]);
    assert!(body.reads.contains(&IterationRead { reader: at, source: peer, current_pass: false }));
    assert!(body.reads.contains(&IterationRead { reader: peer, source: at, current_pass: true }));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn execution_is_observed_only_when_requested_on_the_recalculation_route() {
    let (host, sheet) = workbook();
    let connection = port(&host);
    let at = |row, column| Key(sheet.0, sheet.1, row, column);
    connection.commit(batch(1, vec![
        enter(at(1, 1), "2"),
        enter(at(1, 2), "=A1*2"),
        enter(at(1, 3), "=B1+1"),
    ])).unwrap();
    while host.advance() {}

    let read = || {
        let request = query::CalcExecution;
        query::CalcExecution::decode(connection.ask(request.request(), BUDGET).unwrap()).unwrap().body
    };
    assert!(matches!(read(), Evidence::Unavailable { .. }));
    connection.run(Operation::CalculateObserved).unwrap();
    while host.advance() {}
    let Evidence::Observed { body, .. } = read() else { panic!("observed dispatch evidence") };
    assert!(body.finished);
    assert_eq!(body.total_entries, 2);
    assert_eq!(body.bands.len(), 2);
    assert_eq!(body.bands.iter().map(|band| band.cells[0]).collect::<Vec<_>>(), [at(1, 2), at(1, 3)]);
    assert!(body.bands.iter().all(|band| !band.workers));

    connection.run(Operation::Calculate { scope: SourceRef::Workbook }).unwrap();
    while host.advance() {}
    assert!(matches!(read(), Evidence::Unavailable { .. }));
}

fn changed_cells_fixture() -> (Vec<query::FormulaResultChange>, Sheet) {
    let (host, sheet) = workbook();
    let connection = port(&host);
    let at = |row, column| Key(sheet.0, sheet.1, row, column);
    let stamp = connection.ask(WorkbookQuery::Summary, BUDGET).unwrap().stamp;
    let mut initial = batch(1, vec![
        enter(at(1, 1), "2"), enter(at(1, 2), "=A1*2"), enter(at(1, 4), "=SEQUENCE(2,1,A1)"),
        Change::Model(ModelChange::Name {
            scope: NameScope::Workbook,
            change: NameChange::Define { name: "AliasValue".into(), formula: "=Sheet1!$B$1".into() },
        }),
    ]);
    initial.base = Some(stamp.model);
    connection.commit(initial).unwrap();
    while host.advance() {}
    connection.commit(batch(2, vec![enter(at(1, 1), "3")])).unwrap();
    while host.advance() {}
    let answers = connection.ask(query::CalcChanges.request(), BUDGET).unwrap();
    let Evidence::Observed { body, .. } = query::CalcChanges::decode(answers).unwrap().body else {
        panic!("latest publication must carry changes");
    };
    (body, sheet)
}


#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn calculation_changes_report_a_published_spill_follower() {
    let (changes, sheet) = changed_cells_fixture();
    let follower = Key(sheet.0, sheet.1, 2, 4);
    let change = changes.iter().find(|change| change.at == follower)
        .expect("the evaluated spill follower changed");
    assert_eq!(change.spill_anchor, Some(Key(sheet.0, sheet.1, 1, 4)));
    assert_eq!(change.before.value, Some(RawValue::Number { bits: 3f64.to_bits() }));
    assert_eq!(change.after.value, Some(RawValue::Number { bits: 4f64.to_bits() }));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn calculation_changes_identify_a_stable_workbook_name_for_the_changed_cell() {
    let (changes, sheet) = changed_cells_fixture();
    let cell = Key(sheet.0, sheet.1, 1, 2);
    let change = changes.iter().find(|change| change.at == cell).unwrap();
    assert_eq!(change.before.value, Some(RawValue::Number { bits: 4f64.to_bits() }));
    assert_eq!(change.after.value, Some(RawValue::Number { bits: 6f64.to_bits() }));
    assert_eq!(change.aliases, [query::ResultAlias { name: "AliasValue".into(), scope: NameScope::Workbook }]);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn live_topics_count_host_admissions_independently_of_the_provider_sequence() {
    let (host, _) = workbook();
    let connection = port(&host);
    let topic = calc_host::TopicKey::new("DnaCalc.Price", "", &["LARK".into(), "last".into()]);
    let lease = calc_host::RtdLease(1);
    host.with_session(SelectionCause::Open, |session| {
        for (sequence, disposition) in [
            (3, calc_host::RtdDisposition::Accepted),
            (5, calc_host::RtdDisposition::Accepted),
            (4, calc_host::RtdDisposition::StaleSequence),
        ] {
            let update = session.update_rtd(topic.clone(), lease, sequence, calc_host::value::CalcValue::number(sequence as f64));
            assert_eq!(update, disposition);
        }
    });
    host.with_session(SelectionCause::Open, |session| {
        assert_eq!(session.retire_rtd(topic.clone(), calc_host::RtdLease(0)), calc_host::RtdDisposition::Accepted);
    });
    let request = query::SourceInventory;
    let answer = query::SourceInventory::decode(connection.ask(request.request(), BUDGET).unwrap()).unwrap();
    let [source] = answer.body.topics.as_slice() else { panic!("one connected topic") };
    assert_eq!(source.sequence, 5);
    let receipt = source.receipt.unwrap();
    assert_eq!(receipt.received, 2);
    assert_eq!(receipt.timed_received, 2);
    assert!(receipt.first_unix_ms.unwrap() <= receipt.last_unix_ms.unwrap());
    host.with_session(SelectionCause::Open, |session| {
        let accepted = session.take_rtd_admissions();
        assert_eq!(accepted.iter().map(|record| record.sequence).collect::<Vec<_>>(), [3, 5]);
    });
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn preview_effects_scope_an_edited_formula_and_reject_an_unread_spill_cell() {
    let (host, sheet) = workbook();
    let connection = port(&host);
    let at = |row, column| Key(sheet.0, sheet.1, row, column);
    let edited = at(1, 2);
    connection.commit(batch(1, vec![
        enter(at(1, 1), "3"),
        enter(edited, "=A1*2"),
        enter(at(1, 4), "=SEQUENCE(2)"),
        enter(at(1, 5), "=RAND()"),
    ])).unwrap();
    while host.advance() {}
    let before = connection.ask(WorkbookQuery::Summary, BUDGET).unwrap().stamp;
    let trial = |formula| connection.preview(
        &batch(2, vec![enter(edited, formula)]),
        PreviewDepth::WithEffects { budget: BUDGET },
    ).unwrap();
    let scoped = trial("=A1*3");
    assert!(scoped.refusal.is_none(), "{scoped:?}");
    assert!(scoped.warnings.iter().any(|warning| warning.contains("scoped to the edited formulas")));
    let [effect] = scoped.effects.as_slice() else { panic!("one observed edited formula") };
    assert_eq!(effect.at, edited);
    assert_eq!(effect.before.value, RawValue::Number { bits: 6f64.to_bits() });
    assert_eq!(effect.after.value, RawValue::Number { bits: 9f64.to_bits() });
    let uncertain = trial("=A1*3+D2");
    assert!(matches!(uncertain.refusal, Some(Refusal::NotModelled { .. })), "{uncertain:?}");
    assert!(uncertain.effects.is_empty());
    let expanding = trial("=SEQUENCE(2)");
    assert!(matches!(expanding.refusal, Some(Refusal::NotModelled { .. })), "{expanding:?}");
    assert!(expanding.effects.is_empty());
    assert_eq!(connection.ask(WorkbookQuery::Summary, BUDGET).unwrap().stamp, before);
}
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn pivot_cache_freshness_tracks_source_changes_and_refresh() {
    let (host, sheet) = workbook();
    let connection = port(&host);
    let at = |row, column| Key(sheet.0, sheet.1, row, column);
    connection.commit(batch(1, vec![
        enter(at(1, 1), "Region"), enter(at(1, 2), "Amount"),
        enter(at(2, 1), "West"), enter(at(2, 2), "2"),
        enter(at(3, 1), "East"), enter(at(3, 2), "5"),
        enter(at(4, 1), "West"), enter(at(4, 2), "7"),
    ])).unwrap();
    while host.advance() {}
    let name = host.with_session(SelectionCause::Open, |session| {
        let revision = session.host().revision().get();
        let source = Area(at(1, 1), at(4, 2)).area().unwrap();
        let receipt = session.dispatch(calc_host::SkinIntent::Command(calc_host::WorkbookCommand::PivotCreate {
            revision, source, destination: "M3".into(), row: 0, value: 1, subtotal: "sum".into(),
        }));
        assert!(receipt.accepted(), "{receipt:?}");
        session.host().model().definitions().pivots().tables().next().unwrap().name.clone()
    });
    let inventory = query::Inventory { kinds: vec![InventoryKind::PivotTables] };
    let answer = query::Inventory::decode(connection.ask(inventory.request(), BUDGET).unwrap()).unwrap();
    assert!(matches!(&answer.body[..], [InventoryItem::PivotTable { name: held, .. }] if held == &name));
    let compare = || {
        let request = query::PivotFreshnessAt { at: at(3, 13) };
        query::PivotFreshnessAt::decode(connection.ask(request.request(), BUDGET).unwrap()).unwrap().body
    };
    let initial = compare();
    assert_eq!(initial.freshness, PivotFreshness::Current);
    let read_at = initial.observed_refresh_unix_ms.expect("measured creation time");
    connection.commit(batch(2, vec![enter(at(2, 2), "3")])).unwrap();
    while host.advance() {}
    let stale = compare();
    assert_eq!(stale.freshness, PivotFreshness::Stale);
    assert_eq!(stale.observed_refresh_unix_ms, Some(read_at));
    let before = connection.ask(WorkbookQuery::Summary, BUDGET).unwrap().stamp;
    let mut request = batch(3, vec![Change::Model(ModelChange::RefreshPivot { at: at(3, 13), expected: before })]);
    request.base = Some(before.model);
    request.workbook = Some(before.workbook);
    request.label = Some("Refresh PivotTable".into());
    let review = connection.preview(&request, PreviewDepth::WritesOnly).unwrap();
    assert!(review.refusal.is_none(), "{review:?}");
    assert_eq!(review.base, before);
    assert!(review.properties.iter().any(|change| matches!(change, ModelPropertyDiff::Scope {
        source: SourceRef::Cells { areas, .. }, property: ModelProperty::PivotCache,
    } if areas.iter().any(|area| area.0 == at(3, 13)))));
    assert_eq!(compare(), stale);
    assert_eq!(connection.ask(WorkbookQuery::Summary, BUDGET).unwrap().stamp, before);
    let committed = connection.commit(request.clone()).unwrap();
    assert!(committed.model > before.model);
    assert_eq!(connection.commit(request).unwrap(), committed);
    while host.advance() {}
    let refreshed = compare();
    assert_eq!(refreshed.freshness, PivotFreshness::Current);
    assert!(refreshed.observed_refresh_unix_ms.unwrap() >= read_at);
    connection.run(Operation::Undo).unwrap();
    while host.advance() {}
    assert_eq!(compare(), stale);
    connection.run(Operation::Redo).unwrap();
    while host.advance() {}
    assert_eq!(compare(), refreshed);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn close_summary_counts_distinct_inputs_and_added_native_scenarios() {
    let (host, sheet) = workbook();
    let operator = port(&host);
    let principal = Key(sheet.0, sheet.1, 2, 2);
    let rate = Key(sheet.0, sheet.1, 3, 2);
    assert_eq!(ask(&operator, query::CloseSummaryOf), CloseSummary::default());
    operator.commit(batch(1, vec![enter(principal, "90000"), enter(rate, "0.1")])).unwrap();
    let stamp = operator.ask(WorkbookQuery::Summary, BUDGET).unwrap().stamp;
    operator.commit(Batch {
        workbook: Some(stamp.workbook),
        base: Some(stamp.model),
        ..batch(2, vec![Change::Model(ModelChange::SaveScenarioCells {
            scenario: ScenarioRef { sheet, name: "Audit assumptions".into() },
            areas: vec![Area(principal, rate)],
        })])
    }).unwrap();
    assert_eq!(ask(&operator, query::CloseSummaryOf), CloseSummary {
        has_changes: true,
        changed_inputs: 2,
        added_scenarios: 1,
        layout_edited: false,
    });

    operator.commit(batch(3, vec![enter(rate, "0.12")])).unwrap();
    assert_eq!(ask(&operator, query::CloseSummaryOf).changed_inputs, 2);
    operator.run(Operation::Undo).unwrap();
    operator.run(Operation::Undo).unwrap();
    assert_eq!(ask(&operator, query::CloseSummaryOf), CloseSummary {
        has_changes: true,
        changed_inputs: 2,
        added_scenarios: 0,
        layout_edited: false,
    });
    operator.run(Operation::Undo).unwrap();
    assert_eq!(ask(&operator, query::CloseSummaryOf), CloseSummary::default());

    operator.commit(batch(4, vec![enter(principal, "=40+2")])).unwrap();
    assert_eq!(ask(&operator, query::CloseSummaryOf), CloseSummary {
        has_changes: true,
        ..CloseSummary::default()
    });
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn worksheet_objects_include_the_host_chart_and_its_data() {
    let (host, sheet) = object_fixture(include_bytes!("../../calc-host/tests/fixtures/dc_6yw/charts.xlsx"));
    let connection = port(&host);
    let objects = read_objects(&connection, sheet);
    assert_eq!(objects.unread_charts, 0);
    let column = objects.charts.iter().find(|chart| chart.image.label == "Column").unwrap();
    assert_eq!(column.image, ask(&connection, query::ChartImageOf { object: column.source.clone(), comparison: None }));
    let plot = &column.data.plots[0];
    assert_eq!(plot.kind, ChartPlotKind::Column);
    assert_eq!(plot.series[0].name, "North");
    assert_eq!(plot.series[0].categories[0], Some(RawValue::Text("Jan".encode_utf16().collect())));
    assert_eq!(plot.series[0].values,
        [12., 18., 9., 22., 15., 27.].map(|value: f64| Some(value.to_bits())));
    assert!(!plot.series[0].uses_saved_values);
    assert!(matches!(column.frame, ObjectFrame::Cells { from, to }
        if from.cell == Key(sheet.0, sheet.1, 1, 7) && to.cell == Key(sheet.0, sheet.1, 15, 13)));
    assert!(matches!(connection.ask(query::Objects { sheet }.request(), Budget { nodes: 0, ..BUDGET }),
        Err(Refusal::OverBudget { .. })));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn worksheet_objects_keep_pictures_sparklines_and_linked_controls() {
    let (host, sheet) = object_fixture(include_bytes!("../../calc-doc/tests/fixtures/drawing/pictures-paint.xlsx"));
    let connection = port(&host);
    let pictures = read_objects(&connection, sheet);
    let cropped = pictures.drawings.iter().find(|drawing| drawing.name == "Pic PNG crop").unwrap();
    assert_eq!(cropped.kind, DrawingKind::Picture);
    assert!(cropped.frame.is_some());
    let picture = cropped.picture.as_ref().unwrap();
    assert_eq!(picture.crop, [25_000, 10_000, 25_000, 40_000]);
    assert!(picture.data_url.as_ref().unwrap().starts_with("data:image/png;base64,iVBORw0KGgo"));
    assert!(matches!(connection.ask(query::Objects { sheet }.request(), Budget { bytes: 64, ..BUDGET }),
        Err(Refusal::OverBudget { .. })));

    let (host, sheet) = object_fixture(include_bytes!("../../calc-doc/tests/fixtures/drawing/sparklines.xlsx"));
    let objects = read_objects(&port(&host), sheet);
    assert_eq!(objects.sparklines.len(), 7);
    let line = objects.sparklines.iter().find(|line| line.at == Key(sheet.0, sheet.1, 2, 6)).unwrap();
    assert_eq!(line.kind, "line");
    assert!(line.marker_points);
    assert_eq!(line.values, [3., 5., 2., 8., 6.].map(|value: f64| Some(value.to_bits())));

    let (host, sheet) = object_fixture(include_bytes!("../../calc-doc/tests/fixtures/drawing/form-controls.xlsx"));
    let objects = read_objects(&port(&host), sheet);
    let check = objects.controls.iter().find(|control| control.id == 1026).unwrap();
    assert_eq!(check.kind, "CheckBox");
    assert_eq!(check.caption.as_deref(), Some("Include tax"));
    assert_eq!(check.linked_cell, Some(Key(sheet.0, sheet.1, 2, 2)));
    assert_eq!(check.linked_value, Some(RawValue::Logical(true)));
    assert!(objects.drawings.iter().any(|shape| shape.hidden && shape.text.as_ref()
        .is_some_and(|text| text.iter().any(|line| line == "Include tax"))));
    let list = objects.controls.iter().find(|control| control.id == 1029).unwrap();
    assert_eq!(list.items.len(), 4);
    assert!(objects.controls.iter().any(|control| control.id == 1025 && control.has_macro));
}

fn object_fixture(bytes: &[u8]) -> (SkinHost, Sheet) {
    let book = HostSession::open(
        Arc::new(MemorySource::new(bytes.to_vec())),
        &OpenPolicy::default(),
        HostOptions::default(),
    ).unwrap();
    let sheet = Sheet::of(book.host().sheets()[0].id());
    let host = SkinHost::new(book);
    while host.advance() {}
    (host, sheet)
}

fn read_objects(port: &impl WorkbookPort, sheet: Sheet) -> WorksheetObjects {
    let budget = Budget { bytes: 8 * 1024 * 1024, millis: 2_000, ..BUDGET };
    let reply = port.ask(query::Objects { sheet }.request(), budget).unwrap();
    query::Objects::decode(reply).unwrap().body
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn validation_settings_round_trip_and_require_an_exact_scope_and_revision() {
    let (host, sheet) = workbook();
    let operator = port(&host);
    let (first, last) = (Key(sheet.0, sheet.1, 2, 2), Key(sheet.0, sheet.1, 3, 2));
    let request = query::ValidationFor { selected: Area(first, last) };
    let author = |id, stamp: Stamp, settings| {
        let mut edit = batch(id, vec![Change::Model(ModelChange::SetValidation {
            selected: request.selected,
            settings,
        })]);
        edit.workbook = Some(stamp.workbook);
        edit.base = Some(stamp.model);
        edit
    };
    let settings = ValidationSettings::List { source: r#"Red,Blue & "Green""#.into() };
    let original = operator.ask(request.request(), BUDGET).unwrap().stamp;
    let edit = author(1, original, settings.clone());
    let preview = operator.preview(&edit, PreviewDepth::WritesOnly).unwrap();
    assert!(preview.refusal.is_none(), "{preview:?}");
    operator.commit(edit).unwrap();
    assert_eq!(ask(&operator, request.clone()), settings);
    assert!(matches!(operator.ask(
        query::ValidationFor { selected: Area(first, first) }.request(), BUDGET,
    ), Err(Refusal::NotModelled { .. })));
    let stale = author(2, original, ValidationSettings::AnyValue);
    assert!(matches!(operator.commit(stale), Err(Refusal::Conflict { .. })));
    let mut bytes = Vec::new();
    host.with_session(SelectionCause::History, |session| {
        session.save(&calc_host::SavePolicy::default(), &mut bytes).unwrap();
    });

    let (reopened, reopened_sheet) = object_fixture(&bytes);
    assert_eq!(reopened_sheet, sheet);
    let operator = port(&reopened);
    assert_eq!(ask(&operator, request.clone()), settings);
    assert!(matches!(operator.commit(batch(1, vec![enter(last, "Purple")])),
        Err(Refusal::Validation { .. })));
    let current = operator.ask(request.request(), BUDGET).unwrap().stamp;
    operator.commit(author(2, current, ValidationSettings::AnyValue)).unwrap();
    assert_eq!(ask(&operator, request.clone()), ValidationSettings::AnyValue);
    operator.run(Operation::Undo).unwrap();
    assert_eq!(ask(&operator, request), settings);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn input_editors_follow_linked_controls_and_excel_validation() {
    use query::{InputChoice, InputEditor};

    let (host, sheet) =
        object_fixture(include_bytes!("../../calc-doc/tests/fixtures/drawing/form-controls.xlsx"));
    let operator = port(&host);
    let at = |row, column| Key(sheet.0, sheet.1, row, column);
    let editor = |at| ask(&operator, query::Rules { at }).input_editor;
    assert_eq!(editor(at(2, 2)), InputEditor::Checkbox);
    assert_eq!(editor(at(3, 2)), InputEditor::Field);
    assert_eq!(
        editor(at(5, 2)),
        InputEditor::Slider { min: 0, max: 100, step: 1 }
    );
    assert_eq!(
        editor(at(6, 2)),
        InputEditor::Stepper { min: 1, max: 10, step: 1 }
    );
    assert_eq!(
        editor(at(7, 2)),
        InputEditor::Choices(["North", "South", "East", "West"].iter().enumerate()
            .map(|(index, label)| InputChoice {
                label: (*label).into(),
                entry: (index + 1).to_string(),
            })
            .collect())
    );
    host.with_session(SelectionCause::History, |session| {
        let receipt = session.dispatch(calc_host::SkinIntent::Edit {
            edits: vec![calc_host::CellEdit::Validations {
                sheet: sheet.id(),
                rules: vec![calc_doc_model::Validation {
                    areas: vec![(9, 2, 9, 2)],
                    kind: "list".into(),
                    in_cell_dropdown: true,
                    show_error: true,
                    error_style: "stop".into(),
                    formula1: Some(r#""North,001,=East""#.into()),
                    authored_copy: false,
                    source: None,
                    operator: "between".into(),
                    allow_blank: false,
                    show_input: false,
                    error_title: None,
                    error: None,
                    prompt_title: None,
                    prompt: None,
                    formula2: None,
                }],
            }],
        });
        assert!(receipt.accepted(), "{receipt:?}");
    });
    assert_eq!(
        editor(at(9, 2)),
        InputEditor::Choices(["North", "001", "=East"].iter().map(|label| InputChoice {
            label: (*label).into(),
            entry: format!("'{label}"),
        }).collect())
    );
    let list = at(9, 2);
    operator.commit(batch(1, vec![enter(list, "'001")])).unwrap();
    let rows = ask(&operator, query::Rows {
        source: target(list),
        filter: Vec::new(),
        sort: Vec::new(),
        window: RowWindow { offset: 0, limit: 1 },
    });
    assert_eq!(rows.rows[0].cells[0].shown.value, RawValue::Text("001".encode_utf16().collect()));

}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn map_overview_counts_local_cross_sheet_and_unattributed_references() {
    let (host, sheet) = workbook();
    let connection = port(&host);
    let at = |row, column| Key(sheet.0, sheet.1, row, column);
    connection.commit(batch(1, vec![
        enter(at(1, 1), "2"),
        enter(at(2, 1), "=A1+1"),
        enter(at(2, 2), "=A2*2"),
    ])).unwrap();
    while host.advance() {}
    let answer = connection.ask(query::DependencyAtlasOf.request(), BUDGET).unwrap();
    let atlas = query::DependencyAtlasOf::decode(answer).unwrap();
    let Evidence::Derived { body, .. } = atlas.body else { panic!("expected derived dependencies") };
    assert_eq!(body.local_references, 2);
    assert_eq!(body.total_references, 2);
    assert!(body.links.is_empty());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn replace_reference_rewrites_only_the_chosen_utf16_operand_and_undoes() {
    use calc_skin::query::{Formula, ReferenceOperandsOf};

    let (host, sheet) = workbook();
    let connection = port(&host);
    let at = |row, column| Key(sheet.0, sheet.1, row, column);
    let caller = at(3, 3);
    let before = r#"=IF("🌍"="🌍",$A1+$A1,$B$2)"#;
    let after = r#"=IF("🌍"="🌍",$A1+$B$1,$B$2)"#;
    connection.commit(batch(1, vec![
        enter(at(1, 1), "2"), enter(at(1, 2), "5"), enter(at(2, 2), "10"),
        enter(caller, before),
    ])).unwrap();
    while host.advance() {}
    let operands = ReferenceOperandsOf::decode(
        connection.ask(ReferenceOperandsOf { at: caller }.request(), BUDGET).unwrap(),
    ).unwrap();
    assert_eq!(operands.body.formula, before);
    assert_eq!(operands.body.operands.iter().map(|item| item.text.as_str()).collect::<Vec<_>>(),
        ["$A1", "$A1", "$B$2"]);
    let occurrence = operands.body.operands[1].span;
    let set = |id, occurrence, with: &str| Batch {
        base: Some(operands.stamp.model),
        ..batch(id, vec![Change::Model(ModelChange::ReplaceReference {
            at: caller, expected_old: before.into(), occurrence, with: with.into(),
        })])
    };
    let edit = set(2, occurrence, "$B$1");
    let preview = connection.preview(&edit, PreviewDepth::WritesOnly).unwrap();
    assert_eq!(preview.writes.len(), 1);
    assert_eq!(preview.writes[0].at, caller);
    assert!(connection.preview(&set(3, occurrence, "0"), PreviewDepth::WritesOnly).unwrap().refusal.is_some());
    assert!(connection.preview(&set(4, OperandSpan {start: 0, end: 1}, "$B$1"), PreviewDepth::WritesOnly).unwrap().refusal.is_some());
    let formula = || Formula::decode(connection.ask(Formula { at: caller }.request(), BUDGET).unwrap())
        .unwrap().body.authored.unwrap();
    assert_eq!(formula(), before);
    connection.commit(edit).unwrap();
    assert_eq!(formula(), after);
    assert!(matches!(
        connection.preview(&set(5, occurrence, "$B$2"), PreviewDepth::WritesOnly).unwrap().refusal,
        Some(Refusal::Conflict { .. }),
    ));
    connection.run(Operation::Undo).unwrap();
    assert_eq!(formula(), before);
}
