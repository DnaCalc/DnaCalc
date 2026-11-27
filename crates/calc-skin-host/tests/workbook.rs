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
            origin: ErrorOrigin::RootCandidate
        }
    );
    for at in [a2, a3] {
        assert!(body.errors.contains(&ErrorIssue {
            at,
            code: "#DIV/0!".into(),
            origin: ErrorOrigin::CandidatePropagation { roots: vec![a1] },
        }));
    }
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
