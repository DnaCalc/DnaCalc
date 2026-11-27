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
