use calc_host::{HostOptions, HostSession, MemorySource, OpenPolicy, blank_package};
use calc_skin::grid::{Area, Key, Sheet};
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
        port.ask(WorkbookQuery::History, BUDGET).unwrap().body["undo"],
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
